//! Rendering data validation errors.

use std::error::Error;
use std::fmt;

use crate::tile::BitDepth;

/// Invalid data supplied to a rendering type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderError {
    /// Packed pixel data does not contain a whole number of tiles.
    InvalidTileDataLen {
        /// Requested tile bit depth.
        bit_depth: BitDepth,
        /// Supplied byte length.
        len: usize,
    },

    /// A regular tilemap's entry count does not match its area.
    TilemapSizeMismatch {
        /// Entry count required by the dimensions.
        expected: usize,
        /// Supplied entry count.
        actual: usize,
    },

    /// A nonempty regular tilemap has unsupported dimensions or an area that
    /// overflows `usize`.
    ///
    /// Valid dimensions are at most 32x32 or exactly 64x32, 32x64, or 64x64.
    TilemapDimensionsInvalid {
        /// Supplied width in tiles.
        width_tiles: usize,
        /// Supplied height in tiles.
        height_tiles: usize,
    },

    /// An affine tilemap's tile area or per-axis pixel extent (`width_tiles`
    /// or `height_tiles` times the tile side length) overflows `usize`.
    AffineTilemapDimensionsInvalid {
        /// Supplied width in tiles.
        width_tiles: usize,
        /// Supplied height in tiles.
        height_tiles: usize,
    },

    /// An affine tilemap's tile-index count does not match its area.
    AffineTilemapSizeMismatch {
        /// Tile-index count required by the dimensions.
        expected: usize,
        /// Supplied tile-index count.
        actual: usize,
    },
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTileDataLen { bit_depth, len } => write!(
                f,
                "tile data length {len} is not a multiple of the {bit_depth:?} tile byte size ({})",
                bit_depth.tile_byte_len(),
            ),
            Self::TilemapSizeMismatch { expected, actual } => write!(
                f,
                "tilemap expected {expected} screen entries, got {actual}"
            ),
            Self::TilemapDimensionsInvalid {
                width_tiles,
                height_tiles,
            } => write!(
                f,
                "tilemap dimensions {width_tiles}x{height_tiles} are not a valid single- or \
                 multi-screenblock size"
            ),
            Self::AffineTilemapDimensionsInvalid {
                width_tiles,
                height_tiles,
            } => write!(
                f,
                "affine tilemap dimensions {width_tiles}x{height_tiles} overflow usize"
            ),
            Self::AffineTilemapSizeMismatch { expected, actual } => write!(
                f,
                "affine tilemap expected {expected} tile indices, got {actual}"
            ),
        }
    }
}

impl Error for RenderError {}

#[cfg(test)]
mod tests {
    use super::RenderError;
    use crate::tile::BitDepth;

    #[test]
    fn display_messages_are_non_empty_and_mention_the_numbers() {
        let err = RenderError::InvalidTileDataLen {
            bit_depth: BitDepth::Bpp4,
            len: 31,
        };
        let msg = err.to_string();
        assert!(msg.contains("31"));
        assert!(msg.contains("32"));

        let err = RenderError::TilemapSizeMismatch {
            expected: 6,
            actual: 5,
        };
        let msg = err.to_string();
        assert!(msg.contains('6'));
        assert!(msg.contains('5'));

        let err = RenderError::TilemapDimensionsInvalid {
            width_tiles: 33,
            height_tiles: 1,
        };
        let msg = err.to_string();
        assert!(msg.contains("33"));
        assert!(msg.contains('1'));

        let err = RenderError::AffineTilemapDimensionsInvalid {
            width_tiles: usize::MAX,
            height_tiles: 2,
        };
        let msg = err.to_string();
        assert!(msg.contains(&usize::MAX.to_string()));
        assert!(msg.contains('2'));
    }
}
