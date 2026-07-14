//! GBA indexed tile pixel decoding (S-2 slice 1).
//!
//! A GBA tile is an 8x8 block of palette indices, packed either 4 bits/pixel
//! (16-color tile, 32 bytes) or 8 bits/pixel (256-color tile, 64 bytes), as
//! loaded by `pokeemerald/src/bg.c`'s tileset loaders. Decode order is
//! verified against `mgba`'s `DRAW_BACKGROUND_MODE_0_TILE_*` macros
//! (`mgba/src/gba/renderers/software-mode0.c`): pixels are stored row-major,
//! and within a 4bpp byte the low nibble is the left pixel of the pair, the
//! high nibble the right one `(behavioral-fidelity)`.

use crate::error::RenderError;

/// GBA tile pixel-data bit depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BitDepth {
    /// 16 colors/tile, 4 bits/pixel, 32 bytes/tile.
    Bpp4,
    /// 256 colors/tile, 8 bits/pixel, 64 bytes/tile.
    Bpp8,
}

impl BitDepth {
    /// Tile width/height in pixels — always 8 on GBA hardware.
    pub const TILE_DIM: usize = 8;

    /// Raw byte length of one tile's pixel data at this bit depth.
    #[must_use]
    pub const fn tile_byte_len(self) -> usize {
        match self {
            Self::Bpp4 => Self::TILE_DIM * Self::TILE_DIM / 2,
            Self::Bpp8 => Self::TILE_DIM * Self::TILE_DIM,
        }
    }
}

/// One decoded 8x8 tile: palette indices in row-major order.
///
/// Indices are `0..16` for [`BitDepth::Bpp4`] tiles (a local index within a
/// palette bank) or `0..=255` for [`BitDepth::Bpp8`] tiles (a flat palette
/// index) — see [`Palette`](crate::palette::Palette).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tile {
    indices: [u8; BitDepth::TILE_DIM * BitDepth::TILE_DIM],
}

impl Tile {
    /// The palette index at pixel `(x, y)` within the tile (`0..8` each).
    ///
    /// # Panics
    ///
    /// Panics if `x` or `y` is outside the `0..8` tile range.
    #[must_use]
    pub fn index(&self, x: usize, y: usize) -> u8 {
        assert!(
            x < BitDepth::TILE_DIM && y < BitDepth::TILE_DIM,
            "tile pixel ({x}, {y}) out of the 0..8 range"
        );
        self.indices[y * BitDepth::TILE_DIM + x]
    }

    /// Decode one 4bpp tile from its packed 32-byte pixel data.
    fn decode_4bpp(bytes: &[u8]) -> Self {
        debug_assert_eq!(bytes.len(), BitDepth::Bpp4.tile_byte_len());
        let mut indices = [0u8; BitDepth::TILE_DIM * BitDepth::TILE_DIM];
        // 4 bytes/row (8 pixels at 4 bits each); low nibble is the left
        // pixel of each byte's pair, high nibble the right.
        for (row, chunk) in bytes.chunks_exact(4).enumerate() {
            for (byte_in_row, &byte) in chunk.iter().enumerate() {
                let base = row * BitDepth::TILE_DIM + byte_in_row * 2;
                indices[base] = byte & 0x0F;
                indices[base + 1] = byte >> 4;
            }
        }
        Self { indices }
    }

    /// Decode one 8bpp tile from its packed 64-byte pixel data: one byte
    /// per pixel, row-major, so this is a direct copy.
    fn decode_8bpp(bytes: &[u8]) -> Self {
        debug_assert_eq!(bytes.len(), BitDepth::Bpp8.tile_byte_len());
        let mut indices = [0u8; BitDepth::TILE_DIM * BitDepth::TILE_DIM];
        indices.copy_from_slice(bytes);
        Self { indices }
    }
}

/// An owned collection of decoded tiles at a single bit depth, indexable by
/// the raw tile number used in a [`ScreenEntry`](crate::bg::ScreenEntry).
#[derive(Debug, Clone)]
pub struct Tileset {
    bit_depth: BitDepth,
    tiles: Vec<Tile>,
}

impl Tileset {
    /// Decode a tileset from packed raw tile pixel data at the given bit
    /// depth. `data` is a flat concatenation of consecutive tiles.
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

    /// The bit depth every tile in this set was decoded at.
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
        // Row 0: byte 0x21 -> pixel(0,0)=1 (low nibble), pixel(1,0)=2 (high
        // nibble). Remaining 31 bytes are 0 (an all-index-0 tile).
        let mut bytes = [0u8; 32];
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
        // Second row starts at byte offset 4: byte 0x0F there -> pixel(0,1)
        // = 15 (max 4bpp index), pixel(1,1) = 0.
        let mut bytes = [0u8; 32];
        bytes[4] = 0x0F;
        let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
        let tile = tileset.tile(0).unwrap();
        assert_eq!(tile.index(0, 1), 15);
        assert_eq!(tile.index(1, 1), 0);
        assert_eq!(tile.index(0, 0), 0, "row 0 must be untouched");
    }

    #[test]
    fn decode_8bpp_is_a_direct_row_major_copy() {
        let mut bytes = [0u8; 64];
        bytes[0] = 200;
        bytes[7] = 255;
        bytes[8] = 1; // row 1, x=0
        let tileset = Tileset::decode(BitDepth::Bpp8, &bytes).unwrap();
        let tile = tileset.tile(0).unwrap();
        assert_eq!(tile.index(0, 0), 200);
        assert_eq!(tile.index(7, 0), 255);
        assert_eq!(tile.index(0, 1), 1);
    }

    #[test]
    fn decode_multiple_tiles_in_sequence() {
        let mut bytes = [0u8; 64]; // two 4bpp tiles
        bytes[0] = 0x21; // tile 0, pixel(0,0)=1, pixel(1,0)=2
        bytes[32] = 0x43; // tile 1, pixel(0,0)=3, pixel(1,0)=4
        let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
        assert_eq!(tileset.len(), 2);
        assert!(!tileset.is_empty());
        assert_eq!(tileset.tile(0).unwrap().index(0, 0), 1);
        assert_eq!(tileset.tile(1).unwrap().index(0, 0), 3);
        assert!(tileset.tile(2).is_none());
    }

    #[test]
    fn decode_rejects_length_not_a_multiple_of_tile_size() {
        let bytes = [0u8; 31];
        assert_eq!(
            Tileset::decode(BitDepth::Bpp4, &bytes).unwrap_err(),
            RenderError::InvalidTileDataLen {
                bit_depth: BitDepth::Bpp4,
                len: 31,
            }
        );

        let bytes = [0u8; 63];
        assert_eq!(
            Tileset::decode(BitDepth::Bpp8, &bytes).unwrap_err(),
            RenderError::InvalidTileDataLen {
                bit_depth: BitDepth::Bpp8,
                len: 63,
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
