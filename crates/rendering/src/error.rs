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
    }
}
