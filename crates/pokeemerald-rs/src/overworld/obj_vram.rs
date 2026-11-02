//! Per-frame OBJ VRAM staging for the overworld's sprites.
//!
//! Upstream keeps every object event's sheet in ROM and copies only the
//! sprite's current frame into the slot it allocated in OBJ VRAM
//! (`RequestSpriteCopy`, `sprite.c`). VRAM therefore holds one frame per
//! drawn sprite, never a map's whole set of sheets, and the ten-bit OBJ tile
//! field bounds what one frame draws. The scene packs every resolved sheet
//! into one sheet tileset for frame selection; [`stage`] performs upstream's
//! copy for one composed frame, rewriting each entry's tile index into a
//! compact tileset of only the frames it draws.

use std::collections::HashMap;

use rendering::{BitDepth, OamEntry, Tile, Tileset};

/// The ten-bit OBJ tile field addresses this many 4bpp tiles.
pub(super) const OBJ_TILE_LIMIT: usize = 1024;

/// The staged VRAM of one frame: `entries` index `tiles`.
#[derive(Debug)]
pub(super) struct StagedObjVram {
    pub(super) tiles: Tileset,
    pub(super) entries: Vec<OamEntry>,
    /// Entries that found no free VRAM, as upstream's `AllocSpriteTiles`
    /// failure leaves a sprite undrawn.
    pub(super) dropped: usize,
}

/// Copies the frame each of `entries` draws from `sheet` into a fresh
/// tileset, one slot per distinct frame, and retargets the entries at it.
/// Entries past the OBJ tile limit are dropped and counted.
pub(super) fn stage(entries: &[OamEntry], sheet: &Tileset) -> StagedObjVram {
    debug_assert_eq!(sheet.bit_depth(), BitDepth::Bpp4);
    let mut tiles: Vec<Tile> = Vec::new();
    let mut slots: HashMap<(u16, usize), u16> = HashMap::new();
    let mut staged = Vec::with_capacity(entries.len());
    let mut dropped = 0;
    for &entry in entries {
        let (width, height) = entry.dimensions();
        let tile_count = (width / BitDepth::TILE_DIM) * (height / BitDepth::TILE_DIM);
        let key = (entry.tile_index(), tile_count);
        let base = if let Some(&base) = slots.get(&key) {
            base
        } else {
            if tiles.len() + tile_count > OBJ_TILE_LIMIT {
                dropped += 1;
                continue;
            }
            #[expect(
                clippy::cast_possible_truncation,
                reason = "the staged tileset never exceeds the ten-bit OBJ tile limit"
            )]
            let base = tiles.len() as u16;
            for offset in 0..tile_count {
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "a frame's tile count fits in u16"
                )]
                let index = entry.tile_index().wrapping_add(offset as u16);
                tiles.push(sheet.tile(index).copied().unwrap_or(BLANK_TILE));
            }
            slots.insert(key, base);
            base
        };
        staged.push(entry.with_tile_index(base));
    }
    StagedObjVram {
        tiles: Tileset::from_tiles(BitDepth::Bpp4, tiles),
        entries: staged,
        dropped,
    }
}

/// An all-transparent tile for a sheet index past the packed sheets.
const BLANK_TILE: Tile = Tile::transparent();

#[cfg(test)]
mod tests {
    use super::*;
    use rendering::ObjShape;

    fn entry(tile_index: u16) -> OamEntry {
        OamEntry::new(
            0,
            0,
            tile_index,
            0,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            0,
            0,
            true,
        )
    }

    fn sheet(tile_count: usize) -> Tileset {
        let mut bytes = Vec::new();
        for tile in 0..tile_count {
            #[expect(clippy::cast_possible_truncation, reason = "small test sheets")]
            let value = (tile as u8) & 0x0F;
            bytes.extend(std::iter::repeat_n(value | (value << 4), 32));
        }
        Tileset::decode(BitDepth::Bpp4, &bytes).unwrap()
    }

    #[test]
    fn stage_copies_only_the_drawn_frames_and_shares_repeated_ones() {
        let sheet = sheet(16);
        let staged = stage(&[entry(9), entry(3), entry(9)], &sheet);
        assert_eq!(staged.tiles.len(), 2);
        let indices: Vec<u16> = staged.entries.iter().map(|e| e.tile_index()).collect();
        assert_eq!(indices, [0, 1, 0]);
        assert_eq!(staged.tiles.tile(0).unwrap().index(0, 0), 9);
        assert_eq!(staged.tiles.tile(1).unwrap().index(0, 0), 3);
        assert_eq!(staged.dropped, 0);
    }

    #[test]
    fn stage_drops_entries_past_the_obj_tile_limit() {
        // 16x16 frames: four tiles each, 256 of them fill the limit exactly.
        let square_16 = |tile_index: u16| {
            OamEntry::new(
                0,
                0,
                tile_index,
                0,
                BitDepth::Bpp4,
                false,
                false,
                ObjShape::Square,
                1,
                0,
                true,
            )
        };
        let sheet = sheet_distinct(OBJ_TILE_LIMIT + 4);
        let mut entries: Vec<OamEntry> = (0..OBJ_TILE_LIMIT / 4)
            .map(|frame| square_16(u16::try_from(frame * 4).unwrap()))
            .collect();
        assert_eq!(stage(&entries, &sheet).dropped, 0);
        // A frame at an unaligned index is distinct from every slot above,
        // and nothing is left for it.
        entries.push(square_16(1));
        let staged = stage(&entries, &sheet);
        assert_eq!(staged.dropped, 1);
        assert_eq!(staged.entries.len(), OBJ_TILE_LIMIT / 4);
        assert_eq!(staged.tiles.len(), OBJ_TILE_LIMIT);
    }

    fn sheet_distinct(tile_count: usize) -> Tileset {
        Tileset::decode(BitDepth::Bpp4, &vec![0u8; tile_count * 32]).unwrap()
    }
}
