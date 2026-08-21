//! Backend-neutral [`PackEntry`] constructors: the one place a normalized
//! input becomes pack bytes.
//!
//! Two backends write packs (Discussion #71): `cargo xtask extract` reads a
//! `pokeemerald/` checkout, and the shipped binary's `--import-rom` reads a
//! ROM. Policy A's promise is that both produce the *same* pack from the
//! same game. That only holds if both serialize through identical code, so
//! the payload shaping lives here rather than at each call site.
//!
//! The constructors validate what the format's payload contract promises
//! readers: an [`EntryKind::Palette`]'s `color_count` really addresses
//! `color_count * 2` payload bytes, an [`EntryKind::Image`]'s
//! `width`/`height` really addresses `width * height` payload bytes. A
//! hand-built literal can promise either without delivering it.

use std::fmt;

use crate::layout::EntryKind;
use crate::writer::PackEntry;

/// The GBA's tile side, in pixels.
const TILE_DIM: u32 = 8;

/// An input that cannot be shaped into a well-formed [`PackEntry`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryShapeError {
    /// A palette had more colours than the format's `color_count` field can
    /// represent (it is a `u16`; see the crate docs). Carries the actual
    /// colour count.
    PaletteColorCountUnrepresentable(usize),
    /// An image's pixel buffer was not exactly `width * height` bytes, the
    /// length its directory metadata promises readers. Carries the declared
    /// dimensions and the actual buffer length.
    ImageSizeMismatch {
        /// The declared width in pixels.
        width: u32,
        /// The declared height in pixels.
        height: u32,
        /// The pixel buffer's actual length.
        len: usize,
    },
    /// An image built from GBA tiles had a width or height that is not a
    /// whole multiple of 8, so it does not decompose into 8x8 tiles.
    /// Carries the declared dimensions.
    NotTileAligned {
        /// The declared width in pixels.
        width: u32,
        /// The declared height in pixels.
        height: u32,
    },
    /// A tile buffer was not a whole number of tiles, or held more tiles
    /// than the raster needs. Fewer tiles is legal (upstream's `gbagfx`
    /// `-num_tiles` cut drops all-zero trailing tiles, which unpack back to
    /// the zero fill this reconstructs). Carries the raster's full tile-data
    /// length and the buffer's actual length.
    TileDataLength {
        /// `tiles_wide * tiles_high * bytes_per_tile` for the target raster.
        expected: usize,
        /// The tile buffer's actual length.
        actual: usize,
    },
    /// A tile bit depth other than 4 or 8 was requested. The GBA has no
    /// other packed tile format, and 1bpp/2bpp sources reach the pack
    /// already expanded to one byte per pixel. Carries the requested depth.
    UnsupportedBitDepth(u8),
    /// A metatile shape did not tile the image's 8x8 grid evenly (or had a
    /// zero side). Carries the requested metatile shape and the grid it had
    /// to divide, both in tiles.
    MetatileMisaligned {
        /// Metatile width in tiles.
        metatile_width: u32,
        /// Metatile height in tiles.
        metatile_height: u32,
        /// The image's width in tiles.
        tiles_wide: u32,
        /// The image's height in tiles.
        tiles_high: u32,
    },
}

impl fmt::Display for EntryShapeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PaletteColorCountUnrepresentable(count) => {
                write!(f, "palette has {count} colours, more than u16 can hold")
            }
            Self::ImageSizeMismatch { width, height, len } => write!(
                f,
                "image is {width}x{height} but its pixel buffer is {len} bytes"
            ),
            Self::NotTileAligned { width, height } => {
                write!(f, "image {width}x{height} is not a multiple of 8x8 tiles")
            }
            Self::TileDataLength { expected, actual } => write!(
                f,
                "tile data is {actual} bytes, not a whole number of tiles at most {expected}"
            ),
            Self::UnsupportedBitDepth(depth) => {
                write!(f, "unsupported tile bit depth {depth} (expected 4 or 8)")
            }
            Self::MetatileMisaligned {
                metatile_width,
                metatile_height,
                tiles_wide,
                tiles_high,
            } => write!(
                f,
                "metatile {metatile_width}x{metatile_height} does not divide \
                 the {tiles_wide}x{tiles_high} tile grid"
            ),
        }
    }
}

impl std::error::Error for EntryShapeError {}

/// Build an [`EntryKind::Palette`] entry from GBA-native packed BGR555
/// colours.
///
/// The payload is one little-endian `u16` per colour, the shape the crate
/// docs promise. Callers holding 8-bit RGB convert first; the conversion
/// belongs to whichever backend decoded the source, not to the format.
///
/// # Errors
///
/// [`EntryShapeError::PaletteColorCountUnrepresentable`] if there are more
/// than `u16::MAX` colours.
pub fn palette_entry(id: String, colors_bgr555: &[u16]) -> Result<PackEntry, EntryShapeError> {
    let color_count = u16::try_from(colors_bgr555.len())
        .map_err(|_| EntryShapeError::PaletteColorCountUnrepresentable(colors_bgr555.len()))?;
    let mut payload = Vec::with_capacity(colors_bgr555.len() * 2);
    for color in colors_bgr555 {
        payload.extend_from_slice(&color.to_le_bytes());
    }
    Ok(PackEntry {
        id,
        kind: EntryKind::Palette { color_count },
        payload,
    })
}

/// Build an [`EntryKind::Image`] entry from an already-unpacked, row-major,
/// one-byte-per-pixel raster.
///
/// `bit_depth` is the source's own depth (2, 4, or 8) and is informational
/// metadata only: the payload is always one byte per pixel.
///
/// # Errors
///
/// [`EntryShapeError::ImageSizeMismatch`] if `pixels.len()` is not exactly
/// `width * height`.
pub fn image_entry(
    id: String,
    width: u32,
    height: u32,
    bit_depth: u8,
    pixels: Vec<u8>,
) -> Result<PackEntry, EntryShapeError> {
    let expected = (width as usize).checked_mul(height as usize);
    if expected != Some(pixels.len()) {
        return Err(EntryShapeError::ImageSizeMismatch {
            width,
            height,
            len: pixels.len(),
        });
    }
    Ok(PackEntry {
        id,
        kind: EntryKind::Image {
            width,
            height,
            bit_depth,
        },
        payload: pixels,
    })
}

/// Build an [`EntryKind::Image`] entry by unpacking GBA 8x8 tile data into a
/// row-major raster.
///
/// This is the ROM importer's entry point: a ROM holds tiles, while the
/// checkout backend's PNGs decode straight to a raster. Both must reach the
/// same bytes, so the unpacking is defined here once.
///
/// `bit_depth` must be 4 (two pixels per byte, low nibble first) or 8 (one
/// byte per pixel). Tiles fill the raster in the order upstream's `gbagfx`
/// writes them: plain row-major over the 8x8 grid when `metatile` is
/// `None`, or, when it is `Some((mw, mh))` in tiles, `gbagfx`'s
/// `-mwidth`/`-mheight` order: the grid is cut into `mw` x `mh` metatiles,
/// metatiles run row-major across the image, and tiles run row-major inside
/// each metatile. `Some((1, 1))` is the plain order.
///
/// A short `tiles` buffer zero-fills the remaining raster. Upstream's
/// `-num_tiles` cuts drop all-zero trailing tiles, so the stored art really
/// is shorter than its declared size; refusing it would reject valid input.
///
/// # Errors
///
/// [`EntryShapeError::UnsupportedBitDepth`] for a depth other than 4 or 8;
/// [`EntryShapeError::NotTileAligned`] if `width_px` or `height_px` is not a
/// multiple of 8; [`EntryShapeError::MetatileMisaligned`] if a metatile
/// shape does not divide the tile grid; [`EntryShapeError::TileDataLength`]
/// if `tiles` is not a whole number of tiles or holds more than the raster
/// needs.
pub fn image_entry_from_tiles(
    id: String,
    tiles: &[u8],
    bit_depth: u8,
    width_px: u32,
    height_px: u32,
    metatile: Option<(u32, u32)>,
) -> Result<PackEntry, EntryShapeError> {
    let bytes_per_tile = match bit_depth {
        4 => 32,
        8 => 64,
        other => return Err(EntryShapeError::UnsupportedBitDepth(other)),
    };
    if !width_px.is_multiple_of(TILE_DIM) || !height_px.is_multiple_of(TILE_DIM) {
        return Err(EntryShapeError::NotTileAligned {
            width: width_px,
            height: height_px,
        });
    }
    let tiles_wide = width_px / TILE_DIM;
    let tiles_high = height_px / TILE_DIM;
    let (mw, mh) = metatile.unwrap_or((1, 1));
    if mw == 0 || mh == 0 || !tiles_wide.is_multiple_of(mw) || !tiles_high.is_multiple_of(mh) {
        return Err(EntryShapeError::MetatileMisaligned {
            metatile_width: mw,
            metatile_height: mh,
            tiles_wide,
            tiles_high,
        });
    }

    let needed_tiles = (tiles_wide as usize) * (tiles_high as usize);
    let expected = needed_tiles * bytes_per_tile;
    if tiles.len() > expected || !tiles.len().is_multiple_of(bytes_per_tile) {
        return Err(EntryShapeError::TileDataLength {
            expected,
            actual: tiles.len(),
        });
    }

    let width = width_px as usize;
    let mut pixels = vec![0u8; width * (height_px as usize)];
    let metatiles_wide = (tiles_wide / mw) as usize;
    let (mw, mh) = (mw as usize, mh as usize);
    let per_metatile = mw * mh;

    for (index, tile) in tiles.chunks_exact(bytes_per_tile).enumerate() {
        // Undo `gbagfx`'s metatile walk: within a metatile the column
        // advances fastest, then the row; metatiles then advance the same
        // way across the image.
        let sub_x = index % mw;
        let sub_y = (index / mw) % mh;
        let metatile_index = index / per_metatile;
        let tile_x = (metatile_index % metatiles_wide) * mw + sub_x;
        let tile_y = (metatile_index / metatiles_wide) * mh + sub_y;

        for (row, packed) in tile.chunks_exact(bytes_per_tile / 8).enumerate() {
            let start = (tile_y * 8 + row) * width + tile_x * 8;
            let out = &mut pixels[start..start + 8];
            if bit_depth == 8 {
                out.copy_from_slice(packed);
            } else {
                for (pair, byte) in out.chunks_exact_mut(2).zip(packed) {
                    pair[0] = byte & 0x0F;
                    pair[1] = byte >> 4;
                }
            }
        }
    }

    image_entry(id, width_px, height_px, bit_depth, pixels)
}

/// Build a [`EntryKind::Raw`] entry: opaque bytes, no shape to validate.
#[must_use]
pub fn raw_entry(id: String, payload: Vec<u8>) -> PackEntry {
    PackEntry {
        id,
        kind: EntryKind::Raw,
        payload,
    }
}

#[cfg(test)]
mod tests;
