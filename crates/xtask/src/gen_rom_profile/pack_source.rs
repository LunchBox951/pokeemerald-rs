//! The pack, read back as the generator's expectations.
//!
//! `cargo xtask extract` already turned the upstream checkout into
//! normalized pack entries. That pack is the authority on what each asset's
//! bytes *are*; this module turns each entry back into the byte string the
//! ROM would hold, so a locator can go looking for it.
//!
//! Three shapes cover almost everything:
//!
//! - An image entry is a raster. [`pack_format::tiles_from_image`] packs it
//!   back into GBA tiles, which is what a ROM stores.
//! - A palette entry is already GBA-native BGR555, so its payload *is* the
//!   ROM's bytes.
//! - A raw entry is opaque either way.
//!
//! The Latin glyph sheets are the exception: `gbagfx`'s `.latfont` layout
//! is neither a raster nor a plain tile sheet, so [`latin_font_bytes`]
//! reimplements the packing.

use std::collections::BTreeMap;
use std::path::Path;

use pack_format::{parse_directory, EntryKind};

use super::error::GenRomProfileError;

/// One pack entry, with its payload copied out.
#[derive(Debug, Clone)]
pub struct PackAsset {
    /// The entry's kind and fixed metadata.
    pub kind: EntryKind,
    /// The entry's payload bytes.
    pub payload: Vec<u8>,
}

impl PackAsset {
    /// The image metadata, or an error naming the id that is not an image.
    pub fn image_shape(&self, id: &str) -> Result<(u32, u32, u8), GenRomProfileError> {
        match self.kind {
            EntryKind::Image {
                width,
                height,
                bit_depth,
            } => Ok((width, height, bit_depth)),
            _ => Err(GenRomProfileError::WrongPackEntryKind {
                id: id.to_owned(),
                expected: "image",
            }),
        }
    }

    /// The colour count, or an error naming the id that is not a palette.
    pub fn palette_colors(&self, id: &str) -> Result<u16, GenRomProfileError> {
        match self.kind {
            EntryKind::Palette { color_count } => Ok(color_count),
            _ => Err(GenRomProfileError::WrongPackEntryKind {
                id: id.to_owned(),
                expected: "palette",
            }),
        }
    }
}

/// Every entry of one pack, keyed by id.
#[derive(Debug, Clone)]
pub struct PackSource {
    assets: BTreeMap<String, PackAsset>,
}

impl PackSource {
    /// Read and parse the pack at `path`.
    ///
    /// # Errors
    ///
    /// [`GenRomProfileError::PackUnreadable`] if the file cannot be read,
    /// [`GenRomProfileError::PackMalformed`] if it is not a pack.
    pub fn load(path: &Path) -> Result<Self, GenRomProfileError> {
        let bytes = std::fs::read(path).map_err(|err| GenRomProfileError::PackUnreadable {
            path: path.to_path_buf(),
            reason: err.to_string(),
        })?;
        let directory =
            parse_directory(&bytes).map_err(|err| GenRomProfileError::PackMalformed {
                path: path.to_path_buf(),
                reason: err.to_string(),
            })?;
        let assets = directory
            .into_iter()
            .map(|entry| {
                let payload = bytes[entry.offset..entry.offset + entry.length].to_vec();
                (
                    entry.id,
                    PackAsset {
                        kind: entry.kind,
                        payload,
                    },
                )
            })
            .collect();
        Ok(Self { assets })
    }

    /// Look up one entry.
    ///
    /// # Errors
    ///
    /// [`GenRomProfileError::MissingPackEntry`] if the pack has no such id.
    pub fn get(&self, id: &str) -> Result<&PackAsset, GenRomProfileError> {
        self.assets
            .get(id)
            .ok_or_else(|| GenRomProfileError::MissingPackEntry(id.to_owned()))
    }

    /// Every id starting with `prefix`, in ascending id order.
    pub fn ids_with_prefix(&self, prefix: &str) -> Vec<String> {
        self.assets
            .keys()
            .filter(|id| id.starts_with(prefix))
            .cloned()
            .collect()
    }
}

/// Repack an image entry's raster into the GBA tile bytes a ROM holds.
///
/// `rom_bit_depth` is the ROM's own depth, which is not always the pack
/// entry's: the title screen's press-start banner is an 8-bit-indexed PNG
/// stored as 4bpp tiles.
///
/// # Errors
///
/// [`GenRomProfileError::WrongPackEntryKind`] if `id` is not an image;
/// [`GenRomProfileError::EntryShape`] if the raster and the requested shape
/// disagree.
pub fn image_tiles(
    pack: &PackSource,
    id: &str,
    rom_bit_depth: u8,
    metatile: (u32, u32),
) -> Result<Vec<u8>, GenRomProfileError> {
    let asset = pack.get(id)?;
    let (width, height, _) = asset.image_shape(id)?;
    pack_format::tiles_from_image(&asset.payload, rom_bit_depth, width, height, Some(metatile))
        .map_err(|err| GenRomProfileError::EntryShape {
            id: id.to_owned(),
            reason: err.to_string(),
        })
}

/// Every metatile shape that divides an image's tile grid.
///
/// The ROM decides which one upstream used: only the right shape produces
/// bytes that appear in the image, so the generator searches for all of
/// them and lets the match settle it. Shapes are ordered largest first, so
/// a sheet whose whole grid is one metatile reports the shape upstream
/// wrote rather than the `1x1` that happens to produce the same bytes.
pub fn metatile_candidates(width: u32, height: u32) -> Vec<(u32, u32)> {
    let tiles_wide = width / 8;
    let tiles_high = height / 8;
    let mut shapes = Vec::new();
    for mw in 1..=tiles_wide {
        if !tiles_wide.is_multiple_of(mw) {
            continue;
        }
        for mh in 1..=tiles_high {
            if tiles_high.is_multiple_of(mh) {
                shapes.push((mw, mh));
            }
        }
    }
    shapes.sort_by_key(|&(mw, mh)| std::cmp::Reverse((mw * mh, mw)));
    shapes
}

/// The width of a 2bpp glyph sheet's row, in bytes.
const LATIN_FONT_ROW_BYTES: usize = 64;
/// How many 16-pixel glyph rows a Latin sheet holds.
const LATIN_FONT_ROWS: usize = 32;
/// How many glyph columns a Latin sheet holds.
const LATIN_FONT_COLUMNS: usize = 16;

/// Pack a Latin glyph sheet's raster into `gbagfx`'s `.latfont` layout.
///
/// The sheet is a 16x32 grid of 16x16 glyphs. `.latfont` walks it glyph by
/// glyph, each glyph as four 8x8 tiles in reading order, each tile row as
/// its two 2bpp bytes *swapped*: the tool writes the right-hand four pixels
/// first. Reimplemented from `pokeemerald/tools/gbagfx/font.c`'s
/// `ConvertToLatinFont` `(no-verbatim)`.
///
/// # Errors
///
/// [`GenRomProfileError::WrongPackEntryKind`] if `id` is not an image;
/// [`GenRomProfileError::EntryShape`] if the sheet is not the 256x512 2bpp
/// shape the layout assumes.
pub fn latin_font_bytes(pack: &PackSource, id: &str) -> Result<Vec<u8>, GenRomProfileError> {
    let asset = pack.get(id)?;
    let (width, height, bit_depth) = asset.image_shape(id)?;
    if width != 256 || height != 512 || bit_depth != 2 {
        return Err(GenRomProfileError::EntryShape {
            id: id.to_owned(),
            reason: format!(
                "expected a 256x512 2bpp glyph sheet, got {width}x{height}/{bit_depth}bpp"
            ),
        });
    }

    // First pack the raster to 2bpp rows, the shape `gbagfx` reads.
    let mut rows = vec![0u8; (height as usize) * LATIN_FONT_ROW_BYTES];
    for y in 0..height as usize {
        for x4 in 0..LATIN_FONT_ROW_BYTES {
            let base = y * width as usize + x4 * 4;
            let quad = &asset.payload[base..base + 4];
            rows[y * LATIN_FONT_ROW_BYTES + x4] = (quad[0] << 6)
                | ((quad[1] & 0x03) << 4)
                | ((quad[2] & 0x03) << 2)
                | (quad[3] & 0x03);
        }
    }

    let mut out = Vec::with_capacity(LATIN_FONT_ROWS * LATIN_FONT_COLUMNS * 64);
    for row in 0..LATIN_FONT_ROWS {
        for column in 0..LATIN_FONT_COLUMNS {
            for glyph_tile in 0..4usize {
                let pixels_x = column * 16 + (glyph_tile & 1) * 8;
                for line in 0..8usize {
                    let pixels_y = row * 16 + (glyph_tile >> 1) * 8 + line;
                    let at = pixels_y * LATIN_FONT_ROW_BYTES + pixels_x / 4;
                    out.push(rows[at + 1]);
                    out.push(rows[at]);
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::metatile_candidates;

    #[test]
    fn metatile_candidates_are_every_divisor_pair_largest_first() {
        // A 16x32 sheet is 2x4 tiles: 1,2 wide by 1,2,4 tall.
        let shapes = metatile_candidates(16, 32);
        assert_eq!(shapes.first(), Some(&(2, 4)));
        assert_eq!(shapes.len(), 6);
        assert!(shapes.contains(&(1, 1)));
        assert!(shapes.contains(&(2, 2)));
        assert!(!shapes.contains(&(3, 1)));
        // Sorted by area, then by width, both descending.
        for pair in shapes.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            assert!((a.0 * a.1, a.0) > (b.0 * b.1, b.0), "{shapes:?}");
        }
    }

    #[test]
    fn a_single_tile_sheet_has_one_shape() {
        assert_eq!(metatile_candidates(8, 8), vec![(1, 1)]);
    }
}
