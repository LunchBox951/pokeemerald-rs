//! Regular-BG tilemap data: [`ScreenEntry`] bitfields and [`Tilemap`]
//! screenblock addressing (S-2 slice 1, split out of `bg.rs` in slice 2).
//!
//! Ports the regular-background screen-entry semantics of
//! `pokeemerald/src/bg.c`: a tilemap is a grid of 16-bit screen entries,
//! each selecting a tile index, a horizontal/vertical flip, and (for 4bpp
//! tiles) a palette bank. The bitfield layout matches the standard GBA
//! text-mode screen entry, verified against `mgba`'s `GBA_TEXT_MAP_*` decode
//! macros (`mgba/src/gba/renderers/software-mode0.c`): bits 0-9 tile index,
//! bit 10 h-flip, bit 11 v-flip, bits 12-15 palette bank
//! `(behavioral-fidelity)`.
//!
//! Compositing a [`Tilemap`] into a framebuffer — including the wrapping
//! scroll offsets a real regular BG applies — is [`BgLayer`](crate::bg::BgLayer)'s
//! job, not this module's.

use crate::error::RenderError;

/// One 16-bit regular-BG tilemap screen entry.
///
/// Mirrors the standard GBA text-mode screen entry layout: tile index in
/// bits 0-9, horizontal flip in bit 10, vertical flip in bit 11, and 4bpp
/// palette bank in bits 12-15 (ignored for 8bpp tiles).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScreenEntry {
    tile_index: u16,
    h_flip: bool,
    v_flip: bool,
    palette_bank: u8,
}

impl ScreenEntry {
    const TILE_INDEX_MASK: u16 = 0x03FF;
    const H_FLIP_BIT: u16 = 0x0400;
    const V_FLIP_BIT: u16 = 0x0800;

    /// Decode a raw 16-bit screen entry.
    #[must_use]
    pub const fn from_raw(raw: u16) -> Self {
        #[allow(clippy::cast_possible_truncation)] // raw >> 12 fits in 4 bits.
        let palette_bank = (raw >> 12) as u8;
        Self {
            tile_index: raw & Self::TILE_INDEX_MASK,
            h_flip: raw & Self::H_FLIP_BIT != 0,
            v_flip: raw & Self::V_FLIP_BIT != 0,
            palette_bank,
        }
    }

    /// Build a screen entry directly from its decoded fields. `tile_index`
    /// is masked to 10 bits, `palette_bank` to 4 bits.
    #[must_use]
    pub const fn new(tile_index: u16, h_flip: bool, v_flip: bool, palette_bank: u8) -> Self {
        Self {
            tile_index: tile_index & Self::TILE_INDEX_MASK,
            h_flip,
            v_flip,
            palette_bank: palette_bank & 0x0F,
        }
    }

    /// The tile index into the [`Tileset`](crate::tile::Tileset) (`0..1024`).
    #[must_use]
    pub const fn tile_index(self) -> u16 {
        self.tile_index
    }

    /// Whether the tile is drawn mirrored horizontally.
    #[must_use]
    pub const fn h_flip(self) -> bool {
        self.h_flip
    }

    /// Whether the tile is drawn mirrored vertically.
    #[must_use]
    pub const fn v_flip(self) -> bool {
        self.v_flip
    }

    /// The 4bpp palette bank (`0..16`); unused for 8bpp tiles.
    #[must_use]
    pub const fn palette_bank(self) -> u8 {
        self.palette_bank
    }
}

/// A static grid of [`ScreenEntry`] values: one regular BG layer's tilemap.
///
/// The tilemap itself carries no scroll offset —
/// [`BgLayer::composite_scrolled`](crate::bg::BgLayer::composite_scrolled)
/// takes the scroll position and wraps within this tilemap's pixel size at
/// composite time.
#[derive(Debug, Clone)]
pub struct Tilemap {
    width_tiles: usize,
    height_tiles: usize,
    entries: Vec<ScreenEntry>,
}

impl Tilemap {
    /// Edge length, in tiles, of one GBA tilemap screenblock (32x32 entries).
    const SCREENBLOCK_DIM: usize = 32;

    /// Build a tilemap from a grid of `width_tiles * height_tiles` screen
    /// entries.
    ///
    /// Storage order follows the GBA regular-BG tilemap layout: a map whose
    /// width or height exceeds 32 tiles is stored as a sequence of contiguous
    /// 32x32-entry *screenblocks*, ordered left-to-right then top-to-bottom
    /// (64x32 = left, right; 32x64 = top, bottom; 64x64 = TL, TR, BL, BR),
    /// exactly as `Tilemap::entry` decodes them. A map that is at most 32x32
    /// in both axes is a single screenblock and is therefore plain row-major
    /// -- this is the one relaxation beyond real hardware, since a single
    /// screenblock's row-major layout composites and wraps correctly at any
    /// sub-32x32 size, not just the full 32x32 one.
    ///
    /// Once either dimension exceeds 32, though, `Tilemap::entry`'s
    /// screenblock addressing assumes *complete* 32x32 blocks, so the
    /// dimensions must be exactly one of the three hardware multi-screenblock
    /// regular-BG sizes real GBA hardware can express: 64x32, 32x64, or
    /// 64x64 tiles (512x256 / 256x512 / 512x512 px). A size like 33x1 that
    /// isn't a whole number of screenblocks would pass this far but leave
    /// `Tilemap::entry` indexing past the end of `entries` for in-range
    /// coordinates, so it is rejected here instead. A zero-area map (either
    /// dimension `0`) is always allowed regardless of the other dimension,
    /// since `Tilemap::entry` can never resolve any coordinate into it.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::TilemapDimensionsInvalid`] if the tilemap has
    /// nonzero area and `width_tiles`/`height_tiles` are neither at most
    /// 32x32 nor one of the three hardware multi-screenblock sizes, or if
    /// `width_tiles * height_tiles` would overflow `usize`. Returns
    /// [`RenderError::TilemapSizeMismatch`] if `entries.len() !=
    /// width_tiles * height_tiles`.
    pub fn new(
        width_tiles: usize,
        height_tiles: usize,
        entries: Vec<ScreenEntry>,
    ) -> Result<Self, RenderError> {
        let dimensions_invalid = || RenderError::TilemapDimensionsInvalid {
            width_tiles,
            height_tiles,
        };
        let expected = width_tiles
            .checked_mul(height_tiles)
            .ok_or_else(dimensions_invalid)?;
        let has_area = width_tiles > 0 && height_tiles > 0;
        let exceeds_single_block =
            width_tiles > Self::SCREENBLOCK_DIM || height_tiles > Self::SCREENBLOCK_DIM;
        if has_area
            && exceeds_single_block
            && !Self::is_hardware_multi_block_size(width_tiles, height_tiles)
        {
            return Err(dimensions_invalid());
        }
        if entries.len() != expected {
            return Err(RenderError::TilemapSizeMismatch {
                expected,
                actual: entries.len(),
            });
        }
        Ok(Self {
            width_tiles,
            height_tiles,
            entries,
        })
    }

    /// Whether `(width_tiles, height_tiles)` is one of the three hardware
    /// regular-BG sizes that span more than one 32x32 screenblock: 64x32,
    /// 32x64, or 64x64 tiles. Sizes at most 32x32 in both axes are a single
    /// screenblock and are handled separately (see [`Tilemap::new`]).
    #[must_use]
    const fn is_hardware_multi_block_size(width_tiles: usize, height_tiles: usize) -> bool {
        const DIM: usize = Tilemap::SCREENBLOCK_DIM;
        const DOUBLE_DIM: usize = DIM * 2;
        matches!(
            (width_tiles, height_tiles),
            (DOUBLE_DIM, DIM | DOUBLE_DIM) | (DIM, DOUBLE_DIM)
        )
    }

    /// Width in tiles.
    #[must_use]
    pub const fn width_tiles(&self) -> usize {
        self.width_tiles
    }

    /// Height in tiles.
    #[must_use]
    pub const fn height_tiles(&self) -> usize {
        self.height_tiles
    }

    /// The screen entry at logical tile coordinates `(col, row)`, or `None`
    /// if out of range.
    ///
    /// GBA regular BG tilemaps larger than 32x32 are stored as contiguous
    /// 32x32-entry screenblocks (see [`Tilemap::new`]); this resolves the
    /// logical coordinate to the correct screenblock and in-block offset, so
    /// on a 64-wide map `(col=0, row=1)` is entry 32 (row 1 of screenblock 0)
    /// and `(col=32, row=0)` is entry 1024 (start of the next screenblock).
    /// A map at most 32x32 in both axes is a single screenblock, so this
    /// reduces to plain `row * width_tiles + col`.
    #[must_use]
    pub fn entry(&self, col: usize, row: usize) -> Option<ScreenEntry> {
        if col >= self.width_tiles || row >= self.height_tiles {
            return None;
        }
        let index = if self.width_tiles > Self::SCREENBLOCK_DIM
            || self.height_tiles > Self::SCREENBLOCK_DIM
        {
            const DIM: usize = Tilemap::SCREENBLOCK_DIM;
            const ENTRIES_PER_BLOCK: usize = DIM * DIM;
            let blocks_wide = self.width_tiles.div_ceil(DIM);
            let block = (row / DIM) * blocks_wide + (col / DIM);
            block * ENTRIES_PER_BLOCK + (row % DIM) * DIM + (col % DIM)
        } else {
            row * self.width_tiles + col
        };
        self.entries.get(index).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::{ScreenEntry, Tilemap};
    use crate::error::RenderError;

    #[test]
    fn screen_entry_decodes_gba_text_map_bitfields() {
        // 0x1234 = 0001 0010 0011 0100: tile index bits0-9 = 0x234 = 564,
        // h-flip bit10 clear, v-flip bit11 clear, palette bank = 0x1.
        let entry = ScreenEntry::from_raw(0x1234);
        assert_eq!(entry.tile_index(), 0x234);
        assert!(!entry.h_flip());
        assert!(!entry.v_flip());
        assert_eq!(entry.palette_bank(), 1);

        // 0xFC00 = 1111 1100 0000 0000: tile index = 0, h-flip and v-flip
        // set, palette bank = 0xF.
        let entry = ScreenEntry::from_raw(0xFC00);
        assert_eq!(entry.tile_index(), 0);
        assert!(entry.h_flip());
        assert!(entry.v_flip());
        assert_eq!(entry.palette_bank(), 0xF);
    }

    #[test]
    fn screen_entry_new_masks_out_of_range_fields() {
        let entry = ScreenEntry::new(0xFFFF, true, false, 0xFF);
        assert_eq!(entry.tile_index(), 0x03FF);
        assert_eq!(entry.palette_bank(), 0x0F);
    }

    #[test]
    fn tilemap_new_rejects_entry_count_mismatch() {
        let entries = vec![ScreenEntry::new(0, false, false, 0); 5];
        assert_eq!(
            Tilemap::new(2, 3, entries).unwrap_err(),
            RenderError::TilemapSizeMismatch {
                expected: 6,
                actual: 5,
            }
        );
    }

    #[test]
    fn tilemap_new_rejects_partial_multi_block_dimensions() {
        // 33x1 exceeds a single 32x32 screenblock on width but isn't a whole
        // number of screenblocks, so `Tilemap::entry` would compute an
        // in-bounds-looking index (block 1, offset 0 = 1024) past the end of
        // a 33-entry vec for the in-range coordinate (32, 0). Reject the
        // shape at construction instead of letting that happen silently.
        let entries = vec![ScreenEntry::new(0, false, false, 0); 33];
        assert_eq!(
            Tilemap::new(33, 1, entries).unwrap_err(),
            RenderError::TilemapDimensionsInvalid {
                width_tiles: 33,
                height_tiles: 1,
            }
        );

        // Same bug, transposed axis.
        let entries = vec![ScreenEntry::new(0, false, false, 0); 33];
        assert_eq!(
            Tilemap::new(1, 33, entries).unwrap_err(),
            RenderError::TilemapDimensionsInvalid {
                width_tiles: 1,
                height_tiles: 33,
            }
        );

        // Other partial multi-block sizes: neither whole-screenblock nor
        // one of the three hardware multi-block shapes.
        for (width_tiles, height_tiles) in [(40, 40), (63, 32), (64, 33), (65, 64), (32, 33)] {
            let entries = vec![ScreenEntry::new(0, false, false, 0); width_tiles * height_tiles];
            assert_eq!(
                Tilemap::new(width_tiles, height_tiles, entries).unwrap_err(),
                RenderError::TilemapDimensionsInvalid {
                    width_tiles,
                    height_tiles,
                },
                "({width_tiles}, {height_tiles}) should have been rejected"
            );
        }
    }

    #[test]
    fn tilemap_new_allows_zero_area_regardless_of_the_other_dimension() {
        // A zero-area map can never have any in-range coordinate (`entry`'s
        // own bounds check rejects every `col`/`row` first), so the
        // screenblock-shape restriction doesn't apply -- an extreme
        // dimension paired with a `0` is harmless and must stay accepted.
        for (width_tiles, height_tiles) in [(usize::MAX, 0), (0, usize::MAX), (33, 0), (0, 33)] {
            let tilemap = Tilemap::new(width_tiles, height_tiles, Vec::new()).unwrap();
            assert!(tilemap.entry(0, 0).is_none());
        }
    }

    #[test]
    fn tilemap_new_rejects_length_overflow() {
        // width_tiles * height_tiles overflows usize; this must return an
        // error instead of panicking (debug) or silently wrapping (release).
        assert_eq!(
            Tilemap::new(usize::MAX, 2, Vec::new()).unwrap_err(),
            RenderError::TilemapDimensionsInvalid {
                width_tiles: usize::MAX,
                height_tiles: 2,
            }
        );
    }

    #[test]
    fn tilemap_new_accepts_a_full_32x32_single_block() {
        // The upper edge of the single-screenblock relaxation: exactly 32x32
        // must still construct and address flat row-major.
        let mut entries = vec![ScreenEntry::new(0, false, false, 0); 32 * 32];
        entries[32] = ScreenEntry::new(9, false, false, 0); // row 1, col 0
        let tilemap = Tilemap::new(32, 32, entries).unwrap();
        assert_eq!(tilemap.entry(0, 1).unwrap().tile_index(), 9);
    }

    #[test]
    fn tilemap_new_accepts_all_four_hardware_regular_bg_sizes() {
        for (width_tiles, height_tiles) in [(32, 32), (64, 32), (32, 64), (64, 64)] {
            let entries = vec![ScreenEntry::new(0, false, false, 0); width_tiles * height_tiles];
            assert!(
                Tilemap::new(width_tiles, height_tiles, entries).is_ok(),
                "({width_tiles}, {height_tiles}) should be a valid hardware size"
            );
        }
    }

    #[test]
    fn tilemap_entry_out_of_range_is_none() {
        let entries = vec![ScreenEntry::new(0, false, false, 0); 4];
        let tilemap = Tilemap::new(2, 2, entries).unwrap();
        assert!(tilemap.entry(2, 0).is_none());
        assert!(tilemap.entry(0, 2).is_none());
        assert!(tilemap.entry(1, 1).is_some());
    }

    #[test]
    fn entry_uses_screenblock_addressing_for_a_64x32_map() {
        // A 64x32 map is two horizontal 32x32 screenblocks: SB0 (cols 0-31),
        // SB1 (cols 32-63). Mark two storage slots and confirm the logical
        // coordinates that must resolve to them.
        let mut entries = vec![ScreenEntry::new(0, false, false, 0); 64 * 32];
        entries[32] = ScreenEntry::new(111, false, false, 0); // SB0 row 1, col 0
        entries[1024] = ScreenEntry::new(222, false, false, 0); // SB1 row 0, col 0
        let tilemap = Tilemap::new(64, 32, entries).unwrap();

        // (col=0, row=1) -> entry 32 within SB0.
        assert_eq!(tilemap.entry(0, 1).unwrap().tile_index(), 111);
        // (col=32, row=0) -> entry 1024, the first entry of SB1.
        assert_eq!(tilemap.entry(32, 0).unwrap().tile_index(), 222);
        // A plain flat map would have put (col=32, row=0) at index 32; prove
        // the two addressings genuinely differ here.
        assert_eq!(tilemap.entry(0, 0).unwrap().tile_index(), 0);
    }

    #[test]
    fn entry_uses_screenblock_addressing_for_a_32x64_map() {
        // A 32x64 map is two vertical 32x32 screenblocks: SB0 (rows 0-31),
        // SB1 (rows 32-63).
        let mut entries = vec![ScreenEntry::new(0, false, false, 0); 32 * 64];
        entries[32] = ScreenEntry::new(111, false, false, 0); // SB0 row 1, col 0
        entries[1024] = ScreenEntry::new(222, false, false, 0); // SB1 row 0, col 0
        let tilemap = Tilemap::new(32, 64, entries).unwrap();

        assert_eq!(tilemap.entry(0, 1).unwrap().tile_index(), 111);
        // (col=0, row=32) -> entry 1024, the first entry of SB1.
        assert_eq!(tilemap.entry(0, 32).unwrap().tile_index(), 222);
        assert_eq!(tilemap.entry(0, 0).unwrap().tile_index(), 0);
    }

    #[test]
    fn entry_uses_screenblock_addressing_for_a_64x64_map() {
        // A 64x64 map is four screenblocks: SB0 TL, SB1 TR, SB2 BL, SB3 BR,
        // each 1024 entries.
        let mut entries = vec![ScreenEntry::new(0, false, false, 0); 64 * 64];
        entries[1024] = ScreenEntry::new(1, false, false, 0); // SB1 (TR), (32,0)
        entries[2048] = ScreenEntry::new(2, false, false, 0); // SB2 (BL), (0,32)
        entries[3072] = ScreenEntry::new(3, false, false, 0); // SB3 (BR), (32,32)
        let tilemap = Tilemap::new(64, 64, entries).unwrap();

        assert_eq!(tilemap.entry(32, 0).unwrap().tile_index(), 1);
        assert_eq!(tilemap.entry(0, 32).unwrap().tile_index(), 2);
        assert_eq!(tilemap.entry(32, 32).unwrap().tile_index(), 3);
    }

    #[test]
    fn entry_stays_flat_row_major_for_maps_up_to_32_wide() {
        // Maps at most 32x32 in both axes are a single screenblock, so the
        // legacy flat `row * width + col` addressing must be unchanged.
        let mut entries = vec![ScreenEntry::new(0, false, false, 0); 10 * 5];
        entries[10] = ScreenEntry::new(77, false, false, 0); // row 1, col 0
        let tilemap = Tilemap::new(10, 5, entries).unwrap();
        assert_eq!(tilemap.entry(0, 1).unwrap().tile_index(), 77);
    }
}
