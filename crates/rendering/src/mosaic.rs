//! GBA mosaic block sampling (S-2 slice 4, issue #99).
//!
//! Ports the `MOSAIC` register's block-sampling effect: a mosaic-enabled BG
//! or OBJ layer does not sample its own pixel at `(x, y)` — it samples the
//! top-left pixel of the `H x V` block `(x, y)` falls into, so a whole block
//! shows one solid color. Verified against
//! `mgba/src/gba/renderers/software-bg.c`'s `BACKGROUND_BITMAP_INIT` (BG) and
//! `SPRITE_MOSAIC_LOOP` (OBJ) macros, both of which compute a block's pixel
//! origin as `coord - (coord % size)` `(behavioral-fidelity)`.
//!
//! BG and OBJ mosaic are independent: hardware packs both as separate H/V
//! nibble pairs in one `MOSAIC` register, and each BG/OBJ only mosaics if
//! its own per-layer mosaic bit ([`crate::compositor::BgSlot::with_mosaic`],
//! [`crate::oam::OamEntry::with_mosaic`]) is set — [`MosaicConfig`] carries
//! both sizes, and it is the caller applying them that decides per-layer
//! whether to use the snapped or raw coordinate.

/// One axis pair's mosaic block size (`1..=16` for each of H and V).
///
/// Hardware stores the block size minus one as a 4-bit register field
/// (`0..=15`); [`MosaicSize::from_raw`] adds the `+1` back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MosaicSize {
    h: u8,
    v: u8,
}

impl MosaicSize {
    /// No mosaic: every pixel is its own 1x1 "block", so [`snap`](Self::snap)
    /// is the identity function.
    pub const NONE: Self = Self { h: 1, v: 1 };

    /// Build a block size directly from already-decoded H/V sizes
    /// (`1..=16`); zero is masked up to `1` (a `0`-sized block is
    /// meaningless and would divide by zero in [`snap`](Self::snap)).
    #[must_use]
    pub const fn new(h: u8, v: u8) -> Self {
        Self {
            h: if h == 0 { 1 } else { h },
            v: if v == 0 { 1 } else { v },
        }
    }

    /// Build a block size from the raw 4-bit `MOSAIC` register fields
    /// (`0..=15`, masked): the effective block size is the raw field plus
    /// one (`1..=16`), matching hardware.
    #[must_use]
    pub const fn from_raw(h: u8, v: u8) -> Self {
        Self {
            h: (h & 0x0F) + 1,
            v: (v & 0x0F) + 1,
        }
    }

    /// Snap `(x, y)` to its mosaic block's top-left screen origin:
    /// `coord - (coord % size)` per axis.
    #[must_use]
    pub const fn snap(&self, x: usize, y: usize) -> (usize, usize) {
        (x - x % self.h as usize, y - y % self.v as usize)
    }
}

impl Default for MosaicSize {
    fn default() -> Self {
        Self::NONE
    }
}

/// Per-frame mosaic block sizes for BG layers and OBJ (sprite) entries —
/// hardware's `MOSAIC` register packs both as independent H/V pairs.
///
/// The all-[`MosaicSize::NONE`] [`Default`] disables mosaic entirely,
/// matching the GBA's power-on `MOSAIC = 0` state and reproducing pre-slice-4
/// output byte-for-byte when composited via
/// [`compose_frame`](crate::compositor::compose_frame).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MosaicConfig {
    /// Block size applied to BG layers with their own mosaic bit set
    /// ([`crate::compositor::BgSlot::with_mosaic`]).
    pub bg: MosaicSize,
    /// Block size applied to OBJ entries with their own mosaic bit set
    /// ([`crate::oam::OamEntry::with_mosaic`]).
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
        // 4x2 blocks: x floors to a multiple of 4, y to a multiple of 2.
        let size = MosaicSize::new(4, 2);
        assert_eq!(size.snap(0, 0), (0, 0));
        assert_eq!(size.snap(3, 1), (0, 0));
        assert_eq!(size.snap(4, 2), (4, 2));
        assert_eq!(size.snap(7, 3), (4, 2));
        assert_eq!(size.snap(8, 5), (8, 4));
    }

    #[test]
    fn from_raw_adds_one_to_the_register_field() {
        // Raw 0 -> block size 1 (no visible effect); raw 15 -> block size 16.
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
    fn new_masks_a_zero_size_up_to_one() {
        // A 0-sized block would divide by zero in `snap`; it must clamp to 1.
        let size = MosaicSize::new(0, 0);
        assert_eq!(size.snap(5, 9), (5, 9));
    }

    #[test]
    fn config_default_is_no_mosaic_on_either_layer_kind() {
        let cfg = MosaicConfig::default();
        assert_eq!(cfg.bg, MosaicSize::NONE);
        assert_eq!(cfg.obj, MosaicSize::NONE);
    }
}
