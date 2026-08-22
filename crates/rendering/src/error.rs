//! Error types for the `rendering` crate.
//!
//! A concrete per-crate enum `(oop-boundaries)` — no `anyhow` in library
//! crates.

use std::error::Error;
use std::fmt;

use crate::tile::BitDepth;

/// An error produced while building rendering-crate types from raw data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderError {
    /// Raw tile pixel data was not an exact multiple of the tile byte size
    /// for the requested [`BitDepth`] (32 bytes/tile for 4bpp, 64 for 8bpp).
    ///
    /// Carries the bit depth and the offending byte length.
    InvalidTileDataLen {
        /// The bit depth the data was decoded against.
        bit_depth: BitDepth,
        /// The offending byte length.
        len: usize,
    },

    /// A [`Tilemap`](crate::tilemap::Tilemap)'s screen-entry count did not
    /// match `width_tiles * height_tiles`.
    TilemapSizeMismatch {
        /// The expected entry count (`width_tiles * height_tiles`).
        expected: usize,
        /// The actual number of entries supplied.
        actual: usize,
    },

    /// A [`Tilemap`](crate::tilemap::Tilemap) was constructed with a nonzero
    /// area whose `width_tiles`/`height_tiles` are not a shape
    /// [`Tilemap::entry`](crate::tilemap::Tilemap::entry)'s screenblock
    /// addressing can resolve: either at most 32x32 (a single screenblock,
    /// stored flat row-major) or exactly one of the three hardware
    /// multi-screenblock regular-BG sizes (64x32, 32x64, 64x64). This also
    /// covers `width_tiles * height_tiles` overflowing `usize`.
    TilemapDimensionsInvalid {
        /// The offending width, in tiles.
        width_tiles: usize,
        /// The offending height, in tiles.
        height_tiles: usize,
    },

    /// An [`AffineTilemap`](crate::bg_affine::AffineTilemap)'s raw tile-index
    /// count did not match `width_tiles * height_tiles`.
    AffineTilemapSizeMismatch {
        /// The expected tile-index count (`width_tiles * height_tiles`).
        expected: usize,
        /// The actual number of tile indices supplied.
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
    }
}
