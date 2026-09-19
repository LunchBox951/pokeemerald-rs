//! Decodes packed GBA tiles into row-major palette indices.

use crate::error::RenderError;

/// Packed tile bit depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BitDepth {
    /// 4 bits per pixel and 16 colors per tile.
    Bpp4,
    /// 8 bits per pixel and 256 colors per tile.
    Bpp8,
}

impl BitDepth {
    /// Width and height of every tile in pixels.
    pub const TILE_DIM: usize = 8;

    /// Raw byte length of one tile's pixel data at this bit depth.
    #[must_use]
    pub const fn tile_byte_len(self) -> usize {
        match self {
            Self::Bpp4 => Self::TILE_DIM * Self::TILE_DIM / 2,
            Self::Bpp8 => Self::TILE_DIM * Self::TILE_DIM,
        }
    }

    /// Mask for wrapping a derived OBJ tile index within character VRAM.
    ///
    /// Derived addresses wrap inside the 32 KiB hardware window. mGBA applies
    /// the equivalent `(xBase + charBase) & maskLo` byte-address mask in
    /// `mgba/src/gba/renderers/software-obj.c` `(behavioral-fidelity)`.
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "OBJ character VRAM contains at most 1024 tiles"
    )]
    pub const fn obj_tile_index_mask(self) -> u16 {
        const OBJ_CHARACTER_VRAM_BYTES: usize = 32 * 1024;
        let tile_count = OBJ_CHARACTER_VRAM_BYTES / self.tile_byte_len();
        (tile_count - 1) as u16
    }
}

/// One decoded tile containing row-major palette indices.
///
/// Indices are `0..=15` for [`BitDepth::Bpp4`] and `0..=255` for
/// [`BitDepth::Bpp8`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tile {
    indices: [u8; BitDepth::TILE_DIM * BitDepth::TILE_DIM],
}

impl Tile {
    /// The palette index at `(x, y)`.
    ///
    /// # Panics
    ///
    /// Panics unless both coordinates are in `0..8`.
    #[must_use]
    pub fn index(&self, x: usize, y: usize) -> u8 {
        assert!(
            x < BitDepth::TILE_DIM && y < BitDepth::TILE_DIM,
            "tile pixel ({x}, {y}) out of the 0..8 range"
        );
        self.indices[y * BitDepth::TILE_DIM + x]
    }

    fn decode_4bpp(bytes: &[u8]) -> Self {
        const BITS_PER_PIXEL: u32 = 4;
        const PIXELS_PER_BYTE: usize = 2;
        const LOW_NIBBLE_MASK: u8 = 0x0F;
        debug_assert_eq!(bytes.len(), BitDepth::Bpp4.tile_byte_len());
        let mut indices = [0u8; BitDepth::TILE_DIM * BitDepth::TILE_DIM];
        let bytes_per_row = BitDepth::TILE_DIM / PIXELS_PER_BYTE;
        for (row, chunk) in bytes.chunks_exact(bytes_per_row).enumerate() {
            for (byte_in_row, &byte) in chunk.iter().enumerate() {
                let first_pixel = row * BitDepth::TILE_DIM + byte_in_row * PIXELS_PER_BYTE;
                indices[first_pixel] = byte & LOW_NIBBLE_MASK;
                indices[first_pixel + 1] = byte >> BITS_PER_PIXEL;
            }
        }
        Self { indices }
    }

    fn decode_8bpp(bytes: &[u8]) -> Self {
        debug_assert_eq!(bytes.len(), BitDepth::Bpp8.tile_byte_len());
        let mut indices = [0u8; BitDepth::TILE_DIM * BitDepth::TILE_DIM];
        indices.copy_from_slice(bytes);
        Self { indices }
    }
}

/// Decoded tiles sharing one bit depth.
#[derive(Debug, Clone)]
pub struct Tileset {
    bit_depth: BitDepth,
    tiles: Vec<Tile>,
}

impl Tileset {
    /// Decode consecutive packed tiles at the given bit depth.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::InvalidTileDataLen`] if `data`'s length is not
    /// an exact multiple of the bit depth's per-tile byte size.
    pub fn decode(bit_depth: BitDepth, data: &[u8]) -> Result<Self, RenderError> {
        let tile_len = bit_depth.tile_byte_len();
        if !data.len().is_multiple_of(tile_len) {
            return Err(RenderError::InvalidTileDataLen {
                bit_depth,
                len: data.len(),
            });
        }
        let decode_one: fn(&[u8]) -> Tile = match bit_depth {
            BitDepth::Bpp4 => Tile::decode_4bpp,
            BitDepth::Bpp8 => Tile::decode_8bpp,
        };
        let tiles = data.chunks_exact(tile_len).map(decode_one).collect();
        Ok(Self { bit_depth, tiles })
    }

    /// The bit depth shared by every tile.
    #[must_use]
    pub const fn bit_depth(&self) -> BitDepth {
        self.bit_depth
    }

    /// Number of tiles in this set.
    #[must_use]
    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    /// Whether this tileset has no tiles.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    /// The decoded tile at `index`, or `None` if out of range.
    #[must_use]
    pub fn tile(&self, index: u16) -> Option<&Tile> {
        self.tiles.get(index as usize)
    }

    /// The palette index for one 1D-mapped OBJ pixel at `tile_index`,
    /// honoring an 8bpp sprite's 32-byte half-tile base offset.
    ///
    /// `tile_index` must already be wrapped within the OBJ character window
    /// (see [`BitDepth::obj_tile_index_mask`]). When `half_tile_offset` is
    /// set on an 8bpp tileset, hardware starts the logical 8x8 tile four
    /// rows into the decoded tile at `tile_index`; the remaining rows come
    /// from the next tile, wrapped the same way. This models mGBA's
    /// one-dimensional OBJ addressing for a packed OAM tile number whose low
    /// bit is set (`mgba/src/gba/renderers/software-obj.c:168-171`)
    /// `(behavioral-fidelity)`. Ignored for 4bpp tilesets, whose tiles are
    /// already 32-byte aligned.
    ///
    /// Returns `None` if either decoded tile this pixel needs is missing
    /// from this tileset.
    #[must_use]
    pub(crate) fn obj_pixel_index(
        &self,
        tile_index: u16,
        half_tile_offset: bool,
        x: usize,
        y: usize,
    ) -> Option<u8> {
        const HALF_TILE_ROWS: usize = BitDepth::TILE_DIM / 2;

        if !half_tile_offset || !matches!(self.bit_depth, BitDepth::Bpp8) {
            return Some(self.tile(tile_index)?.index(x, y));
        }

        let combined_row = HALF_TILE_ROWS + y;
        let (index, row) = if combined_row < BitDepth::TILE_DIM {
            (tile_index, combined_row)
        } else {
            let next_tile = tile_index.wrapping_add(1) & self.bit_depth.obj_tile_index_mask();
            (next_tile, combined_row - BitDepth::TILE_DIM)
        };
        Some(self.tile(index)?.index(x, row))
    }
}

#[cfg(test)]
mod tests {
    use super::{BitDepth, Tile, Tileset};
    use crate::error::RenderError;

    #[test]
    fn tile_byte_lengths_match_gba_hardware() {
        assert_eq!(BitDepth::Bpp4.tile_byte_len(), 32);
        assert_eq!(BitDepth::Bpp8.tile_byte_len(), 64);
    }

    #[test]
    fn decode_4bpp_reads_low_nibble_as_left_pixel() {
        let mut bytes = [0u8; BitDepth::Bpp4.tile_byte_len()];
        bytes[0] = 0x21;
        let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
        let tile = tileset.tile(0).unwrap();
        assert_eq!(tile.index(0, 0), 1);
        assert_eq!(tile.index(1, 0), 2);
        assert_eq!(tile.index(2, 0), 0);
        assert_eq!(tile.index(0, 1), 0);
    }

    #[test]
    fn decode_4bpp_rows_use_4_bytes_each() {
        let mut bytes = [0u8; BitDepth::Bpp4.tile_byte_len()];
        bytes[BitDepth::TILE_DIM / 2] = 0x0F;
        let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
        let tile = tileset.tile(0).unwrap();
        assert_eq!(tile.index(0, 1), 15);
        assert_eq!(tile.index(1, 1), 0);
        assert_eq!(tile.index(0, 0), 0, "row 0 must be untouched");
    }

    #[test]
    fn decode_8bpp_is_a_direct_row_major_copy() {
        let mut bytes = [0u8; BitDepth::Bpp8.tile_byte_len()];
        bytes[0] = 200;
        bytes[7] = 255;
        bytes[BitDepth::TILE_DIM] = 1;
        let tileset = Tileset::decode(BitDepth::Bpp8, &bytes).unwrap();
        let tile = tileset.tile(0).unwrap();
        assert_eq!(tile.index(0, 0), 200);
        assert_eq!(tile.index(7, 0), 255);
        assert_eq!(tile.index(0, 1), 1);
    }

    #[test]
    fn decode_multiple_tiles_in_sequence() {
        let mut bytes = [0u8; BitDepth::Bpp4.tile_byte_len() * 2];
        bytes[0] = 0x21;
        bytes[BitDepth::Bpp4.tile_byte_len()] = 0x43;
        let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
        assert_eq!(tileset.len(), 2);
        assert!(!tileset.is_empty());
        assert_eq!(tileset.tile(0).unwrap().index(0, 0), 1);
        assert_eq!(tileset.tile(1).unwrap().index(0, 0), 3);
        assert!(tileset.tile(2).is_none());
    }

    #[test]
    fn obj_pixel_index_without_the_offset_reads_the_tile_directly() {
        let mut bytes = [0u8; BitDepth::Bpp8.tile_byte_len()];
        bytes[0] = 9;
        let tileset = Tileset::decode(BitDepth::Bpp8, &bytes).unwrap();
        assert_eq!(tileset.obj_pixel_index(0, false, 0, 0), Some(9));
    }

    #[test]
    fn obj_pixel_index_with_the_offset_reads_four_rows_into_the_tile_then_the_next_one() {
        const HALF_TILE_ROW_BYTES: usize = 8 * 4;
        const FIRST_TILE_UPPER_HALF: u8 = 11;
        const SECOND_TILE_LOWER_HALF: u8 = 22;

        let mut bytes = [0u8; BitDepth::Bpp8.tile_byte_len() * 2];
        bytes[HALF_TILE_ROW_BYTES] = FIRST_TILE_UPPER_HALF;
        bytes[BitDepth::Bpp8.tile_byte_len()] = SECOND_TILE_LOWER_HALF;
        let tileset = Tileset::decode(BitDepth::Bpp8, &bytes).unwrap();

        assert_eq!(
            tileset.obj_pixel_index(0, true, 0, 0),
            Some(FIRST_TILE_UPPER_HALF),
            "row 0 of the offset tile is row 4 of decoded tile 0"
        );
        assert_eq!(
            tileset.obj_pixel_index(0, true, 0, 4),
            Some(SECOND_TILE_LOWER_HALF),
            "row 4 of the offset tile is row 0 of decoded tile 1"
        );
    }

    #[test]
    fn obj_pixel_index_with_the_offset_wraps_past_the_tileset_using_the_obj_tile_index_mask() {
        const WRAPPED_TO_FIRST_TILE: u8 = 42;

        let bit_depth = BitDepth::Bpp8;
        let last_tile = bit_depth.obj_tile_index_mask();
        let tile_count = usize::from(last_tile) + 1;
        let mut bytes = vec![0u8; bit_depth.tile_byte_len() * tile_count];
        bytes[0] = WRAPPED_TO_FIRST_TILE;
        let tileset = Tileset::decode(bit_depth, &bytes).unwrap();

        assert_eq!(
            tileset.obj_pixel_index(last_tile, true, 0, 4),
            Some(WRAPPED_TO_FIRST_TILE),
            "the offset tile's row 4 wraps from the last tile to tile 0"
        );
    }

    #[test]
    fn obj_pixel_index_ignores_the_offset_for_4bpp() {
        let mut bytes = [0u8; BitDepth::Bpp4.tile_byte_len() * 2];
        bytes[BitDepth::TILE_DIM / 2] = 0x0F;
        let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();

        assert_eq!(tileset.obj_pixel_index(0, true, 0, 1), Some(15));
        assert_eq!(tileset.obj_pixel_index(0, false, 0, 1), Some(15));
    }

    #[test]
    fn decode_rejects_length_not_a_multiple_of_tile_size() {
        const INVALID_4BPP_LEN: usize = BitDepth::Bpp4.tile_byte_len() - 1;
        const INVALID_8BPP_LEN: usize = BitDepth::Bpp8.tile_byte_len() - 1;

        let bytes = [0u8; INVALID_4BPP_LEN];
        assert_eq!(
            Tileset::decode(BitDepth::Bpp4, &bytes).unwrap_err(),
            RenderError::InvalidTileDataLen {
                bit_depth: BitDepth::Bpp4,
                len: INVALID_4BPP_LEN,
            }
        );

        let bytes = [0u8; INVALID_8BPP_LEN];
        assert_eq!(
            Tileset::decode(BitDepth::Bpp8, &bytes).unwrap_err(),
            RenderError::InvalidTileDataLen {
                bit_depth: BitDepth::Bpp8,
                len: INVALID_8BPP_LEN,
            }
        );
    }

    #[test]
    fn decode_accepts_empty_data() {
        let tileset = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
        assert!(tileset.is_empty());
    }

    #[test]
    #[should_panic(expected = "out of the 0..8 range")]
    fn index_panics_out_of_range() {
        let tile = Tile {
            indices: [0; BitDepth::TILE_DIM * BitDepth::TILE_DIM],
        };
        let _ = tile.index(8, 0);
    }
}
