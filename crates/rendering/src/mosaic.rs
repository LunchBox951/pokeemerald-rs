//! Screen-aligned mosaic block sampling for backgrounds and sprites.
//!
//! Each mosaic-enabled layer samples the top-left pixel of its current block.
//! Backgrounds and sprites use independent dimensions from [`MosaicConfig`].

/// Horizontal and vertical mosaic block dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MosaicSize {
    h: u8,
    v: u8,
}

impl MosaicSize {
    const MIN_DIMENSION: u8 = 1;
    const REGISTER_FIELD_MASK: u8 = 0x0F;

    /// One-pixel blocks, which leave sampling unchanged.
    pub const NONE: Self = Self {
        h: Self::MIN_DIMENSION,
        v: Self::MIN_DIMENSION,
    };

    /// Creates decoded block dimensions, clamping zero dimensions to one.
    #[must_use]
    pub const fn new(h: u8, v: u8) -> Self {
        Self {
            h: if h == 0 { Self::MIN_DIMENSION } else { h },
            v: if v == 0 { Self::MIN_DIMENSION } else { v },
        }
    }

    /// Decodes the `MOSAIC` register's four-bit, size-minus-one fields.
    #[must_use]
    pub const fn from_raw(h: u8, v: u8) -> Self {
        Self {
            h: (h & Self::REGISTER_FIELD_MASK) + Self::MIN_DIMENSION,
            v: (v & Self::REGISTER_FIELD_MASK) + Self::MIN_DIMENSION,
        }
    }

    /// Returns the top-left screen coordinate of `(x, y)`'s mosaic block.
    #[must_use]
    pub const fn snap(&self, x: usize, y: usize) -> (usize, usize) {
        (x - x % self.h as usize, y - y % self.v as usize)
    }

    /// Returns the sprite-local sample for a screen-aligned mosaic block.
    ///
    /// `local_coordinate` is `screen_coordinate`'s offset in a non-empty sprite
    /// footprint. Blocks that cross an edge reuse its nearest edge texel; the
    /// caller still tests footprint membership with the unsnapped coordinate.
    /// This matches mGBA's `SPRITE_MOSAIC_LOOP` edge clamp
    /// `(behavioral-fidelity)`.
    #[must_use]
    pub const fn snap_local(
        &self,
        local_coordinate: (usize, usize),
        screen_coordinate: (usize, usize),
        dimensions: (usize, usize),
    ) -> (usize, usize) {
        let (local_x, local_y) = local_coordinate;
        let (screen_x, screen_y) = screen_coordinate;
        let (width, height) = dimensions;
        (
            Self::snap_local_axis(local_x, screen_x, self.h, width),
            Self::snap_local_axis(local_y, screen_y, self.v, height),
        )
    }

    const fn snap_local_axis(
        local_coordinate: usize,
        screen_coordinate: usize,
        block_size: u8,
        dimension: usize,
    ) -> usize {
        let block_offset = screen_coordinate % block_size as usize;
        let block_origin = local_coordinate.saturating_sub(block_offset);
        if block_origin >= dimension {
            dimension - 1
        } else {
            block_origin
        }
    }

    /// Extends a sprite's raw right edge to the next horizontal block boundary.
    ///
    /// Negative edges use Rust's truncating remainder, matching mGBA's
    /// `software-obj.c` condition rounding rather than Euclidean rounding
    /// `(behavioral-fidelity)`. [`MosaicSize::NONE`] leaves the edge unchanged.
    #[must_use]
    pub const fn round_trailing_edge(&self, raw_end: i32) -> i32 {
        let horizontal_size = self.h as i32;
        let remainder = raw_end % horizontal_size;
        if remainder == 0 {
            raw_end
        } else {
            raw_end + (horizontal_size - remainder)
        }
    }
}

impl Default for MosaicSize {
    fn default() -> Self {
        Self::NONE
    }
}

/// Per-frame background and sprite mosaic dimensions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MosaicConfig {
    /// Dimensions for mosaic-enabled background layers.
    pub bg: MosaicSize,
    /// Dimensions for mosaic-enabled sprites.
    pub obj: MosaicSize,
}

#[cfg(test)]
mod tests {
    use super::{MosaicConfig, MosaicSize};

    #[test]
    fn none_is_the_identity() {
        let none = MosaicSize::NONE;
        for (x, y) in [(0, 0), (1, 1), (7, 3), (239, 159)] {
            assert_eq!(none.snap(x, y), (x, y));
        }
    }

    #[test]
    fn snap_floors_to_the_block_origin() {
        let size = MosaicSize::new(4, 2);
        assert_eq!(size.snap(0, 0), (0, 0));
        assert_eq!(size.snap(3, 1), (0, 0));
        assert_eq!(size.snap(4, 2), (4, 2));
        assert_eq!(size.snap(7, 3), (4, 2));
        assert_eq!(size.snap(8, 5), (8, 4));
    }

    #[test]
    fn snap_local_clamps_the_leading_partial_block_to_the_edge() {
        let size = MosaicSize::new(4, 1);
        let dimensions = (8, 8);
        assert_eq!(size.snap_local((0, 0), (2, 0), dimensions), (0, 0));
        assert_eq!(size.snap_local((1, 0), (3, 0), dimensions), (0, 0));
        assert_eq!(size.snap_local((2, 0), (4, 0), dimensions), (2, 0));
        assert_eq!(size.snap_local((5, 0), (7, 0), dimensions), (2, 0));
    }

    #[test]
    fn snap_local_is_identity_at_none() {
        let none = MosaicSize::NONE;
        for (local, screen) in [((0, 0), (5, 9)), ((3, 7), (200, 130)), ((7, 7), (2, 2))] {
            assert_eq!(none.snap_local(local, screen, (8, 8)), local);
        }
    }

    #[test]
    fn from_raw_adds_one_to_the_register_field() {
        assert_eq!(MosaicSize::from_raw(0, 0), MosaicSize::new(1, 1));
        assert_eq!(MosaicSize::from_raw(15, 15), MosaicSize::new(16, 16));
    }

    #[test]
    fn from_raw_masks_to_4_bits() {
        assert_eq!(
            MosaicSize::from_raw(0xFF, 0xFF),
            MosaicSize::from_raw(0x0F, 0x0F)
        );
    }

    #[test]
    fn new_clamps_a_zero_size_up_to_one() {
        let size = MosaicSize::new(0, 0);
        assert_eq!(size.snap(5, 9), (5, 9));
    }

    #[test]
    fn round_trailing_edge_extends_to_the_next_block_boundary() {
        let size = MosaicSize::new(3, 1);
        assert_eq!(size.round_trailing_edge(4), 6);
        assert_eq!(size.round_trailing_edge(6), 6);
        assert_eq!(size.round_trailing_edge(0), 0);
    }

    #[test]
    fn round_trailing_edge_is_identity_at_none() {
        let none = MosaicSize::NONE;
        for raw_end in [0, 4, -4, 240] {
            assert_eq!(none.round_trailing_edge(raw_end), raw_end);
        }
    }

    #[test]
    fn config_default_is_no_mosaic_on_either_layer_kind() {
        let config = MosaicConfig::default();
        assert_eq!(config.bg, MosaicSize::NONE);
        assert_eq!(config.obj, MosaicSize::NONE);
    }
}
