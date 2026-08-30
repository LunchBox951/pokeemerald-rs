//! Regular-background screen entries and tilemaps.
//!
//! [`ScreenEntry`] decodes the tile, flip, and palette fields stored in each
//! map entry. [`Tilemap`] resolves logical tile coordinates through the GBA's
//! row-major grid of 32×32-entry screenblocks.

use crate::error::RenderError;

/// A decoded 16-bit regular-background screen entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScreenEntry {
    tile_index: u16,
    h_flip: bool,
    v_flip: bool,
    palette_bank: u8,
}

impl ScreenEntry {
    const TILE_INDEX_MASK: u16 = 0x03FF;
    const HORIZONTAL_FLIP_FLAG: u16 = 0x0400;
    const VERTICAL_FLIP_FLAG: u16 = 0x0800;
    const PALETTE_BANK_SHIFT: u32 = 12;
    const PALETTE_BANK_MASK: u8 = 0x0F;

    /// Decodes a packed regular-background screen entry.
    #[must_use]
    pub const fn from_raw(raw: u16) -> Self {
        Self {
            tile_index: raw & Self::TILE_INDEX_MASK,
            h_flip: raw & Self::HORIZONTAL_FLIP_FLAG != 0,
            v_flip: raw & Self::VERTICAL_FLIP_FLAG != 0,
            palette_bank: (raw >> Self::PALETTE_BANK_SHIFT) as u8,
        }
    }

    /// Builds an entry, masking the tile index to 10 bits and palette bank to
    /// four bits.
    #[must_use]
    pub const fn new(tile_index: u16, h_flip: bool, v_flip: bool, palette_bank: u8) -> Self {
        Self {
            tile_index: tile_index & Self::TILE_INDEX_MASK,
            h_flip,
            v_flip,
            palette_bank: palette_bank & Self::PALETTE_BANK_MASK,
        }
    }

    /// The tile index into the layer's tileset.
    #[must_use]
    pub const fn tile_index(self) -> u16 {
        self.tile_index
    }

    /// Whether the tile is mirrored horizontally.
    #[must_use]
    pub const fn h_flip(self) -> bool {
        self.h_flip
    }

    /// Whether the tile is mirrored vertically.
    #[must_use]
    pub const fn v_flip(self) -> bool {
        self.v_flip
    }

    /// The 4bpp palette bank; ignored for 8bpp tiles.
    #[must_use]
    pub const fn palette_bank(self) -> u8 {
        self.palette_bank
    }
}

/// A grid of regular-background [`ScreenEntry`] values.
#[derive(Debug, Clone)]
pub struct Tilemap {
    width_tiles: usize,
    height_tiles: usize,
    entries: Vec<ScreenEntry>,
}

impl Tilemap {
    const SCREENBLOCK_SIDE_TILES: usize = 32;
    const DOUBLE_SCREENBLOCK_SIDE_TILES: usize = Self::SCREENBLOCK_SIDE_TILES * 2;
    const ENTRIES_PER_SCREENBLOCK: usize =
        Self::SCREENBLOCK_SIDE_TILES * Self::SCREENBLOCK_SIDE_TILES;

    /// Builds a regular-background tilemap from its packed storage order.
    ///
    /// Maps no larger than 32×32 tiles use flat row-major entries. The larger
    /// supported sizes are 64×32, 32×64, and 64×64 tiles; their entries are
    /// stored as contiguous row-major 32×32 screenblocks, with screenblocks
    /// ordered left-to-right and then top-to-bottom. Zero-area maps are valid.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::TilemapDimensionsInvalid`] for unsupported
    /// nonzero dimensions or an overflowing area. Returns
    /// [`RenderError::TilemapSizeMismatch`] when `entries` does not contain
    /// exactly `width_tiles * height_tiles` values.
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
        let requires_multiple_screenblocks = width_tiles > Self::SCREENBLOCK_SIDE_TILES
            || height_tiles > Self::SCREENBLOCK_SIDE_TILES;
        if has_area
            && requires_multiple_screenblocks
            && !Self::is_supported_multi_screenblock_size(width_tiles, height_tiles)
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

    const fn is_supported_multi_screenblock_size(width_tiles: usize, height_tiles: usize) -> bool {
        matches!(
            (width_tiles, height_tiles),
            (
                Self::DOUBLE_SCREENBLOCK_SIDE_TILES,
                Self::SCREENBLOCK_SIDE_TILES | Self::DOUBLE_SCREENBLOCK_SIDE_TILES
            ) | (
                Self::SCREENBLOCK_SIDE_TILES,
                Self::DOUBLE_SCREENBLOCK_SIDE_TILES
            )
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

    /// Returns the entry at logical tile coordinates `(col, row)`, or `None`
    /// when either coordinate is out of bounds.
    #[must_use]
    pub fn entry(&self, col: usize, row: usize) -> Option<ScreenEntry> {
        let index = self.entry_index(col, row)?;
        self.entries.get(index).copied()
    }

    fn entry_index(&self, col: usize, row: usize) -> Option<usize> {
        if col >= self.width_tiles || row >= self.height_tiles {
            return None;
        }
        if self.width_tiles <= Self::SCREENBLOCK_SIDE_TILES
            && self.height_tiles <= Self::SCREENBLOCK_SIDE_TILES
        {
            return Some(row * self.width_tiles + col);
        }

        let screenblock_col = col / Self::SCREENBLOCK_SIDE_TILES;
        let screenblock_row = row / Self::SCREENBLOCK_SIDE_TILES;
        let screenblocks_per_row = self.width_tiles / Self::SCREENBLOCK_SIDE_TILES;
        let screenblock_index = screenblock_row * screenblocks_per_row + screenblock_col;
        let col_within_screenblock = col % Self::SCREENBLOCK_SIDE_TILES;
        let row_within_screenblock = row % Self::SCREENBLOCK_SIDE_TILES;
        let entry_within_screenblock =
            row_within_screenblock * Self::SCREENBLOCK_SIDE_TILES + col_within_screenblock;
        Some(screenblock_index * Self::ENTRIES_PER_SCREENBLOCK + entry_within_screenblock)
    }
}

#[cfg(test)]
mod tests {
    use super::{ScreenEntry, Tilemap};
    use crate::error::RenderError;

    const EMPTY_ENTRY: ScreenEntry = ScreenEntry::new(0, false, false, 0);
    const EXPECTED_SCREENBLOCK_SIDE_TILES: usize = 32;
    const EXPECTED_DOUBLE_SCREENBLOCK_SIDE_TILES: usize = EXPECTED_SCREENBLOCK_SIDE_TILES * 2;
    const EXPECTED_ENTRIES_PER_SCREENBLOCK: usize =
        EXPECTED_SCREENBLOCK_SIDE_TILES * EXPECTED_SCREENBLOCK_SIDE_TILES;

    fn empty_entries(width_tiles: usize, height_tiles: usize) -> Vec<ScreenEntry> {
        vec![EMPTY_ENTRY; width_tiles * height_tiles]
    }

    fn marked_entry(tile_index: u16) -> ScreenEntry {
        ScreenEntry::new(tile_index, false, false, 0)
    }

    #[test]
    fn screen_entry_decodes_each_bitfield() {
        let tile_index = 0x0234;
        let palette_bank = 1;
        let raw = tile_index | (u16::from(palette_bank) << ScreenEntry::PALETTE_BANK_SHIFT);
        let entry = ScreenEntry::from_raw(raw);
        assert_eq!(entry.tile_index(), tile_index);
        assert!(!entry.h_flip());
        assert!(!entry.v_flip());
        assert_eq!(entry.palette_bank(), palette_bank);

        let palette_bank = ScreenEntry::PALETTE_BANK_MASK;
        let raw = ScreenEntry::HORIZONTAL_FLIP_FLAG
            | ScreenEntry::VERTICAL_FLIP_FLAG
            | (u16::from(palette_bank) << ScreenEntry::PALETTE_BANK_SHIFT);
        let entry = ScreenEntry::from_raw(raw);
        assert_eq!(entry.tile_index(), 0);
        assert!(entry.h_flip());
        assert!(entry.v_flip());
        assert_eq!(entry.palette_bank(), palette_bank);
    }

    #[test]
    fn screen_entry_new_masks_to_the_hardware_field_widths() {
        let entry = ScreenEntry::new(u16::MAX, true, false, u8::MAX);
        let ten_bit_tile_index = (1_u16 << 10) - 1;
        let four_bit_palette_bank = (1_u8 << 4) - 1;
        assert_eq!(entry.tile_index(), ten_bit_tile_index);
        assert_eq!(entry.palette_bank(), four_bit_palette_bank);
    }

    #[test]
    fn tilemap_new_rejects_entry_count_mismatch() {
        let entries = vec![EMPTY_ENTRY; 5];
        assert_eq!(
            Tilemap::new(2, 3, entries).unwrap_err(),
            RenderError::TilemapSizeMismatch {
                expected: 6,
                actual: 5,
            }
        );
    }

    #[test]
    fn tilemap_new_rejects_unsupported_multi_screenblock_dimensions() {
        for (width_tiles, height_tiles) in [
            (33, 1),
            (1, 33),
            (40, 40),
            (63, 32),
            (64, 33),
            (33, 32),
            (65, 64),
            (64, 65),
            (32, 33),
        ] {
            let entries = empty_entries(width_tiles, height_tiles);
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
        for (width_tiles, height_tiles) in [(usize::MAX, 0), (0, usize::MAX), (33, 0), (0, 33)] {
            let tilemap = Tilemap::new(width_tiles, height_tiles, Vec::new()).unwrap();
            assert!(tilemap.entry(0, 0).is_none());
        }
    }

    #[test]
    fn tilemap_new_returns_an_error_when_the_area_overflows() {
        assert_eq!(
            Tilemap::new(usize::MAX, 2, Vec::new()).unwrap_err(),
            RenderError::TilemapDimensionsInvalid {
                width_tiles: usize::MAX,
                height_tiles: 2,
            }
        );
    }

    #[test]
    fn tilemap_new_accepts_a_full_single_screenblock() {
        let side = EXPECTED_SCREENBLOCK_SIDE_TILES;
        let mut entries = empty_entries(side, side);
        entries[side] = marked_entry(9);
        let tilemap = Tilemap::new(side, side, entries).unwrap();
        assert_eq!(tilemap.entry(0, 1).unwrap().tile_index(), 9);
    }

    #[test]
    fn tilemap_new_accepts_all_hardware_regular_background_sizes() {
        let side = EXPECTED_SCREENBLOCK_SIDE_TILES;
        let double_side = EXPECTED_DOUBLE_SCREENBLOCK_SIDE_TILES;
        for (width_tiles, height_tiles) in [
            (side, side),
            (double_side, side),
            (side, double_side),
            (double_side, double_side),
        ] {
            let entries = empty_entries(width_tiles, height_tiles);
            assert!(
                Tilemap::new(width_tiles, height_tiles, entries).is_ok(),
                "({width_tiles}, {height_tiles}) should be a valid hardware size"
            );
        }
    }

    #[test]
    fn tilemap_entry_out_of_range_is_none() {
        let tilemap = Tilemap::new(2, 2, empty_entries(2, 2)).unwrap();
        assert!(tilemap.entry(2, 0).is_none());
        assert!(tilemap.entry(0, 2).is_none());
        assert!(tilemap.entry(1, 1).is_some());
    }

    #[test]
    fn entry_addresses_two_horizontal_screenblocks() {
        let side = EXPECTED_SCREENBLOCK_SIDE_TILES;
        let double_side = EXPECTED_DOUBLE_SCREENBLOCK_SIDE_TILES;
        let mut entries = empty_entries(double_side, side);
        entries[side] = marked_entry(111);
        entries[EXPECTED_ENTRIES_PER_SCREENBLOCK] = marked_entry(222);
        let tilemap = Tilemap::new(double_side, side, entries).unwrap();

        assert_eq!(tilemap.entry(0, 1).unwrap().tile_index(), 111);
        assert_eq!(tilemap.entry(side, 0).unwrap().tile_index(), 222);
        assert_eq!(tilemap.entry(0, 0).unwrap().tile_index(), 0);
    }

    #[test]
    fn entry_addresses_two_vertical_screenblocks() {
        let side = EXPECTED_SCREENBLOCK_SIDE_TILES;
        let double_side = EXPECTED_DOUBLE_SCREENBLOCK_SIDE_TILES;
        let mut entries = empty_entries(side, double_side);
        entries[side] = marked_entry(111);
        entries[EXPECTED_ENTRIES_PER_SCREENBLOCK] = marked_entry(222);
        let tilemap = Tilemap::new(side, double_side, entries).unwrap();

        assert_eq!(tilemap.entry(0, 1).unwrap().tile_index(), 111);
        assert_eq!(tilemap.entry(0, side).unwrap().tile_index(), 222);
        assert_eq!(tilemap.entry(0, 0).unwrap().tile_index(), 0);
    }

    #[test]
    fn entry_addresses_four_screenblocks() {
        let side = EXPECTED_SCREENBLOCK_SIDE_TILES;
        let double_side = EXPECTED_DOUBLE_SCREENBLOCK_SIDE_TILES;
        let mut entries = empty_entries(double_side, double_side);
        entries[EXPECTED_ENTRIES_PER_SCREENBLOCK] = marked_entry(1);
        entries[EXPECTED_ENTRIES_PER_SCREENBLOCK * 2] = marked_entry(2);
        entries[EXPECTED_ENTRIES_PER_SCREENBLOCK * 3] = marked_entry(3);
        let tilemap = Tilemap::new(double_side, double_side, entries).unwrap();

        assert_eq!(tilemap.entry(side, 0).unwrap().tile_index(), 1);
        assert_eq!(tilemap.entry(0, side).unwrap().tile_index(), 2);
        assert_eq!(tilemap.entry(side, side).unwrap().tile_index(), 3);
    }

    #[test]
    fn entry_uses_flat_row_major_storage_within_one_screenblock() {
        let width_tiles = 10;
        let height_tiles = 5;
        let mut entries = empty_entries(width_tiles, height_tiles);
        entries[width_tiles] = marked_entry(77);
        let tilemap = Tilemap::new(width_tiles, height_tiles, entries).unwrap();
        assert_eq!(tilemap.entry(0, 1).unwrap().tile_index(), 77);
    }
}
