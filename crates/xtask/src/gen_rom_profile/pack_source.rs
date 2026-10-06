use std::collections::BTreeMap;
use std::path::Path;

use pack_format::{parse_directory, EntryKind};

use super::error::GenRomProfileError;

/// An owned pack entry; directory parsing does not validate metadata against payload.
#[derive(Debug, Clone)]
pub struct PackAsset {
    /// Entry kind with its declared metadata.
    pub kind: EntryKind,
    /// Raw entry bytes.
    pub payload: Vec<u8>,
}

impl PackAsset {
    /// Returns `(width, height, bit_depth)` without checking the payload.
    /// Returns [`GenRomProfileError::WrongPackEntryKind`] for non-images, naming `id`.
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

    /// Returns the declared colour count without checking the payload.
    /// Returns [`GenRomProfileError::WrongPackEntryKind`] for non-palettes, naming `id`.
    pub fn palette_colors(&self, id: &str) -> Result<u16, GenRomProfileError> {
        match self.kind {
            EntryKind::Palette { color_count } => Ok(color_count),
            _ => Err(GenRomProfileError::WrongPackEntryKind {
                id: id.to_owned(),
                expected: "palette",
            }),
        }
    }

    /// Returns `(payload, colour_count)` after checking exactly two bytes per colour.
    ///
    /// # Errors
    ///
    /// [`GenRomProfileError::WrongPackEntryKind`] for non-palettes;
    /// [`GenRomProfileError::EntryShape`] if payload length disagrees with the count.
    pub fn palette_payload(&self, id: &str) -> Result<(&[u8], u16), GenRomProfileError> {
        let colors = self.palette_colors(id)?;
        let expected = usize::from(colors) * 2;
        if self.payload.len() != expected {
            return Err(GenRomProfileError::EntryShape {
                id: id.to_owned(),
                reason: format!(
                    "palette declares {colors} colours ({expected} bytes) but holds {} bytes",
                    self.payload.len()
                ),
            });
        }
        Ok((&self.payload, colors))
    }

    /// Returns `(payload, width, height, bit_depth)` after checking one byte per pixel.
    ///
    /// # Errors
    ///
    /// [`GenRomProfileError::WrongPackEntryKind`] for non-images;
    /// [`GenRomProfileError::EntryShape`] if payload length disagrees with the dimensions.
    pub fn image_raster(&self, id: &str) -> Result<(&[u8], u32, u32, u8), GenRomProfileError> {
        let (width, height, bit_depth) = self.image_shape(id)?;
        let expected = u64::from(width) * u64::from(height);
        if self.payload.len() as u64 != expected {
            return Err(GenRomProfileError::EntryShape {
                id: id.to_owned(),
                reason: format!(
                    "a {width}x{height} raster holds {expected} bytes, got {}",
                    self.payload.len()
                ),
            });
        }
        Ok((&self.payload, width, height, bit_depth))
    }
}

/// Pack entries keyed by id.
#[derive(Debug, Clone)]
pub struct PackSource {
    assets: BTreeMap<String, PackAsset>,
}

impl PackSource {
    /// Loads a pack without checking payload lengths against entry metadata.
    ///
    /// # Errors
    ///
    /// [`GenRomProfileError::PackUnreadable`] if the file cannot be read;
    /// [`GenRomProfileError::PackMalformed`] if directory parsing fails.
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

    /// Returns the entry identified by `id`.
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

/// Packs an image raster into GBA tiles at `rom_bit_depth`, independent of pack depth.
/// `metatile` gives width and height in tiles; the result covers the whole raster.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] if `id` is absent;
/// [`GenRomProfileError::WrongPackEntryKind`] for non-images;
/// [`GenRomProfileError::EntryShape`] if tile conversion rejects the raster, depth or shape.
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

/// Returns all divisor-pair metatile shapes for the image's complete 8x8 tiles.
/// Dimensions are pixels; shapes are tiles, ordered by descending area, then width.
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
    shapes.sort_by_key(|&(mw, mh)| std::cmp::Reverse((u64::from(mw) * u64::from(mh), mw)));
    shapes
}

const LATIN_FONT_2BPP_RASTER_ROW_BYTES: usize = 64;
const LATIN_FONT_GLYPH_ROWS: usize = 32;
const LATIN_FONT_GLYPH_COLUMNS: usize = 16;

/// Packs a 256x512 2bpp raster into `gbagfx`'s `.latfont` layout.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] if `id` is absent;
/// [`GenRomProfileError::WrongPackEntryKind`] for non-images;
/// [`GenRomProfileError::EntryShape`] if dimensions, bit depth, payload length
/// or pixel indices disagree with that shape and its `0..=3` index range.
pub fn latin_font_bytes(pack: &PackSource, id: &str) -> Result<Vec<u8>, GenRomProfileError> {
    let asset = pack.get(id)?;
    let (_, width, height, bit_depth) = asset.image_raster(id)?;
    if width != 256 || height != 512 || bit_depth != 2 {
        return Err(GenRomProfileError::EntryShape {
            id: id.to_owned(),
            reason: format!(
                "expected a 256x512 2bpp glyph sheet, got {width}x{height}/{bit_depth}bpp"
            ),
        });
    }

    if let Some((at, &index)) = asset.payload.iter().enumerate().find(|(_, p)| **p > 3) {
        return Err(GenRomProfileError::EntryShape {
            id: id.to_owned(),
            reason: format!(
                "pixel {} (x {}, y {}) has index {index}, but a 2bpp glyph sheet holds 0..=3",
                at,
                at % width as usize,
                at / width as usize
            ),
        });
    }

    let mut packed_2bpp_rows = vec![0u8; (height as usize) * LATIN_FONT_2BPP_RASTER_ROW_BYTES];
    for y in 0..height as usize {
        for packed_byte_column in 0..LATIN_FONT_2BPP_RASTER_ROW_BYTES {
            let base = y * width as usize + packed_byte_column * 4;
            let four_pixel_indices = &asset.payload[base..base + 4];
            packed_2bpp_rows[y * LATIN_FONT_2BPP_RASTER_ROW_BYTES + packed_byte_column] =
                (four_pixel_indices[0] << 6)
                    | (four_pixel_indices[1] << 4)
                    | (four_pixel_indices[2] << 2)
                    | four_pixel_indices[3];
        }
    }

    let mut out = Vec::with_capacity(LATIN_FONT_GLYPH_ROWS * LATIN_FONT_GLYPH_COLUMNS * 64);
    for glyph_row in 0..LATIN_FONT_GLYPH_ROWS {
        for glyph_column in 0..LATIN_FONT_GLYPH_COLUMNS {
            for glyph_tile in 0..4usize {
                let pixels_x = glyph_column * 16 + (glyph_tile & 1) * 8;
                for line in 0..8usize {
                    let pixels_y = glyph_row * 16 + (glyph_tile >> 1) * 8 + line;
                    let at = pixels_y * LATIN_FONT_2BPP_RASTER_ROW_BYTES + pixels_x / 4;
                    // `.latfont` writes the right-hand 2bpp byte first
                    // (pokeemerald/tools/gbagfx/font.c:55-56, ConvertToLatinFont).
                    out.push(packed_2bpp_rows[at + 1]);
                    out.push(packed_2bpp_rows[at]);
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{latin_font_bytes, metatile_candidates, PackAsset, PackSource};
    use crate::gen_rom_profile::error::GenRomProfileError;
    use pack_format::EntryKind;

    /// A one-entry source built by hand, bypassing [`PackSource::load`]'s
    /// parse: exactly what a malformed pack's directory could deliver.
    fn unchecked_single_asset_source(id: &str, kind: EntryKind, payload: Vec<u8>) -> PackSource {
        PackSource {
            assets: [(id.to_owned(), PackAsset { kind, payload })].into(),
        }
    }

    #[test]
    fn a_palette_payload_shorter_than_its_colour_count_is_an_error_not_a_panic() {
        let source = unchecked_single_asset_source(
            "title/palette/pokemon_logo",
            EntryKind::Palette { color_count: 224 },
            vec![0u8; 10],
        );
        let asset = source.get("title/palette/pokemon_logo").unwrap();
        let err = asset
            .palette_payload("title/palette/pokemon_logo")
            .unwrap_err();
        assert!(
            matches!(err, GenRomProfileError::EntryShape { .. }),
            "{err}"
        );
        assert!(err.to_string().contains("224"), "{err}");
    }

    #[test]
    fn a_matching_palette_payload_passes_the_shape_check() {
        let source = unchecked_single_asset_source(
            "interface/palette/main_menu_bg",
            EntryKind::Palette { color_count: 16 },
            vec![0u8; 32],
        );
        let asset = source.get("interface/palette/main_menu_bg").unwrap();
        let (payload, colors) = asset
            .palette_payload("interface/palette/main_menu_bg")
            .unwrap();
        assert_eq!(colors, 16);
        assert_eq!(payload.len(), 32);
    }

    #[test]
    fn an_image_payload_disagreeing_with_its_dimensions_is_an_error_not_a_panic() {
        let source = unchecked_single_asset_source(
            "title/image/pokemon_logo",
            EntryKind::Image {
                width: u32::MAX,
                height: u32::MAX,
                bit_depth: 4,
            },
            vec![0u8; 64],
        );
        let asset = source.get("title/image/pokemon_logo").unwrap();
        let err = asset.image_raster("title/image/pokemon_logo").unwrap_err();
        assert!(
            matches!(err, GenRomProfileError::EntryShape { .. }),
            "{err}"
        );
    }

    #[test]
    fn a_glyph_sheet_payload_shorter_than_its_raster_is_an_error_not_a_panic() {
        let source = unchecked_single_asset_source(
            "fonts/latin_normal",
            EntryKind::Image {
                width: 256,
                height: 512,
                bit_depth: 2,
            },
            vec![0u8; 100],
        );
        let err = latin_font_bytes(&source, "fonts/latin_normal").unwrap_err();
        assert!(
            matches!(err, GenRomProfileError::EntryShape { .. }),
            "{err}"
        );
        assert!(err.to_string().contains("131072"), "{err}");
    }

    #[test]
    fn a_glyph_sheet_pixel_outside_two_bits_is_an_error_not_masked() {
        let mut payload = vec![0u8; 256 * 512];
        payload[1] = 4;
        let source = unchecked_single_asset_source(
            "fonts/latin_normal",
            EntryKind::Image {
                width: 256,
                height: 512,
                bit_depth: 2,
            },
            payload,
        );
        let result = latin_font_bytes(&source, "fonts/latin_normal");
        assert!(
            matches!(result, Err(GenRomProfileError::EntryShape { .. })),
            "an out-of-range pixel index must be rejected, got {:?}",
            result.as_ref().map(|bytes| bytes[..4].to_vec())
        );
        let err = latin_font_bytes(&source, "fonts/latin_normal").unwrap_err();
        let text = err.to_string();
        assert!(
            text.contains("fonts/latin_normal") && text.contains("index 4"),
            "{text}"
        );
    }

    #[test]
    fn latin_font_packs_two_bit_indices_msb_first_with_right_byte_first() {
        let mut payload = vec![0u8; 256 * 512];
        payload[..4].copy_from_slice(&[0, 1, 2, 3]);
        let source = unchecked_single_asset_source(
            "fonts/latin_normal",
            EntryKind::Image {
                width: 256,
                height: 512,
                bit_depth: 2,
            },
            payload,
        );
        let bytes = latin_font_bytes(&source, "fonts/latin_normal").unwrap();
        assert_eq!(&bytes[..2], &[0x00, 0x1B]);
    }

    #[test]
    fn metatile_candidates_are_all_divisor_pairs_in_descending_area_then_width_order() {
        let shapes = metatile_candidates(16, 32);
        assert_eq!(shapes.first(), Some(&(2, 4)));
        assert_eq!(shapes.len(), 6);
        assert!(shapes.contains(&(1, 1)));
        assert!(shapes.contains(&(2, 2)));
        assert!(!shapes.contains(&(3, 1)));
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
