//! The Berry Blender NPC berry tables: the ten three-berry sets the computer
//! opponents draw from, and the five berries the Blend Master uses.
//!
//! Source: `src/berry_blender.c:873-900` (`sOpponentBerrySets` and
//! `sBerryMasterBerries`), as read by `SetOpponentsBerryData`
//! (`src/berry_blender.c:1542-1579`). `NUM_NPC_BERRIES` is `5`
//! (`src/berry_blender.c:107`).
//!
//! # Berry index to item id
//!
//! Upstream stores each berry as a zero-based berry index
//! (`ITEM_TO_BERRY(item) - 1`, that is `item - FIRST_BERRY_INDEX`), and the
//! reader turns it back into a bag item with `index + FIRST_BERRY_INDEX`
//! (`src/berry_blender.c:1578`). `FIRST_BERRY_INDEX` is `ITEM_CHERI_BERRY`
//! (`include/constants/items.h:199`), so index `0` is Cheri Berry. This module
//! applies the same conversion in [`berry_index_item`], reusing
//! [`FIRST_BERRY_ITEM`] from [`crate::berries`], and returns [`ItemId`]s.
//!
//! Sets and slots are indexed from `0`, in upstream order. Every lookup
//! refuses an index that is out of range instead of clamping it. Choosing a
//! set from the player's berry, the Enigma Berry override, the Blend Master
//! replacement arithmetic and the blending itself are not ported here.

use std::error::Error;
use std::fmt;

use crate::berries::FIRST_BERRY_ITEM;
use crate::items::ItemId;

/// The number of distinct berries the NPCs use (`NUM_NPC_BERRIES`).
pub const NPC_BERRY_COUNT: usize = 5;

/// The number of opponent berry sets (`NUM_NPC_BERRIES * 2`): five used when
/// the player's berry is one of the NPC berries, five for any other berry.
pub const OPPONENT_BERRY_SET_COUNT: usize = NPC_BERRY_COUNT * 2;

/// The number of berries in each opponent set.
pub const OPPONENT_BERRIES_PER_SET: usize = 3;

/// The number of Blend Master berries (`ARRAY_COUNT(sBerryMasterBerries)`).
pub const BERRY_MASTER_BERRY_COUNT: usize = 5;

/// A failed lookup in the Berry Blender NPC berry tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlenderNpcBerryError {
    /// The set is not below [`OPPONENT_BERRY_SET_COUNT`].
    InvalidSet(usize),
    /// The berry within a set is not below [`OPPONENT_BERRIES_PER_SET`].
    InvalidSlot(usize),
    /// The Blend Master berry is not below [`BERRY_MASTER_BERRY_COUNT`].
    InvalidBerryMasterBerry(usize),
}

impl fmt::Display for BlenderNpcBerryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSet(set) => write!(
                f,
                "Berry Blender opponent berry set `{set}` is out of range (there are \
                 {OPPONENT_BERRY_SET_COUNT})"
            ),
            Self::InvalidSlot(slot) => write!(
                f,
                "Berry Blender opponent berry slot `{slot}` is out of range (there are \
                 {OPPONENT_BERRIES_PER_SET} per set)"
            ),
            Self::InvalidBerryMasterBerry(index) => write!(
                f,
                "Berry Blender Blend Master berry `{index}` is out of range (there are \
                 {BERRY_MASTER_BERRY_COUNT})"
            ),
        }
    }
}

impl Error for BlenderNpcBerryError {}

/// `sOpponentBerrySets` as upstream stores it: zero-based berry indices,
/// `ITEM_TO_BERRY(item) - 1` (`src/berry_blender.c:875-890`). The five NPC
/// berries are Cheri `0`, Chesto `1`, Pecha `2`, Rawst `3` and Aspear `4`.
const OPPONENT_BERRY_SETS: [[u8; OPPONENT_BERRIES_PER_SET]; OPPONENT_BERRY_SET_COUNT] = [
    [4, 3, 2],
    [0, 4, 3],
    [1, 0, 4],
    [2, 1, 0],
    [3, 2, 1],
    [0, 2, 3],
    [1, 3, 4],
    [2, 4, 0],
    [3, 0, 1],
    [4, 1, 2],
];

/// `sBerryMasterBerries` as upstream stores it: zero-based berry indices
/// (`src/berry_blender.c:894-900`), Spelon `30` through Belue `34`.
const BERRY_MASTER_BERRIES: [u8; BERRY_MASTER_BERRY_COUNT] = [30, 31, 32, 33, 34];

/// Converts an upstream zero-based berry index to its bag item.
///
/// This is the reader's `index + FIRST_BERRY_INDEX`
/// (`src/berry_blender.c:1578`): index `0` is [`FIRST_BERRY_ITEM`] (Cheri
/// Berry). Like upstream it does not check that the result is a berry; every
/// index the tables here hold is one.
#[must_use]
pub const fn berry_index_item(index: u8) -> ItemId {
    ItemId(FIRST_BERRY_ITEM.index() + index as u16)
}

/// Returns the three berries of one opponent set, as items, in upstream order.
///
/// # Errors
///
/// Returns [`BlenderNpcBerryError::InvalidSet`] when `set` is not below
/// [`OPPONENT_BERRY_SET_COUNT`].
pub fn opponent_berry_set(
    set: usize,
) -> Result<[ItemId; OPPONENT_BERRIES_PER_SET], BlenderNpcBerryError> {
    OPPONENT_BERRY_SETS
        .get(set)
        .map(|row| row.map(berry_index_item))
        .ok_or(BlenderNpcBerryError::InvalidSet(set))
}

/// Returns one berry of one opponent set, as an item.
///
/// # Errors
///
/// Returns the error of [`opponent_berry_set`], and
/// [`BlenderNpcBerryError::InvalidSlot`] when `slot` is not below
/// [`OPPONENT_BERRIES_PER_SET`].
pub fn opponent_berry(set: usize, slot: usize) -> Result<ItemId, BlenderNpcBerryError> {
    opponent_berry_set(set)?
        .get(slot)
        .copied()
        .ok_or(BlenderNpcBerryError::InvalidSlot(slot))
}

/// Returns one Blend Master berry, as an item.
///
/// # Errors
///
/// Returns [`BlenderNpcBerryError::InvalidBerryMasterBerry`] when `index` is
/// not below [`BERRY_MASTER_BERRY_COUNT`].
pub fn berry_master_berry(index: usize) -> Result<ItemId, BlenderNpcBerryError> {
    BERRY_MASTER_BERRIES
        .get(index)
        .copied()
        .map(berry_index_item)
        .ok_or(BlenderNpcBerryError::InvalidBerryMasterBerry(index))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Raw `ITEM_*` values from `include/constants/items.h`, read independently
    // of the tables above.
    const CHERI: u16 = 133;
    const CHESTO: u16 = 134;
    const PECHA: u16 = 135;
    const RAWST: u16 = 136;
    const ASPEAR: u16 = 137;

    /// The ten sets as raw item ids, in `sOpponentBerrySets` order: the five
    /// sets for a player who chose Cheri, Chesto, Pecha, Rawst or Aspear, then
    /// the five picked by the player's berry modulo five.
    const EXPECTED_SETS: [[u16; OPPONENT_BERRIES_PER_SET]; OPPONENT_BERRY_SET_COUNT] = [
        [ASPEAR, RAWST, PECHA],
        [CHERI, ASPEAR, RAWST],
        [CHESTO, CHERI, ASPEAR],
        [PECHA, CHESTO, CHERI],
        [RAWST, PECHA, CHESTO],
        [CHERI, PECHA, RAWST],
        [CHESTO, RAWST, ASPEAR],
        [PECHA, ASPEAR, CHERI],
        [RAWST, CHERI, CHESTO],
        [ASPEAR, CHESTO, PECHA],
    ];

    /// Spelon 163, Pamtre 164, Watmel 165, Durin 166, Belue 167.
    const EXPECTED_MASTER: [u16; BERRY_MASTER_BERRY_COUNT] = [163, 164, 165, 166, 167];

    #[test]
    fn every_opponent_set_is_pinned_slot_by_slot() {
        for (set, row) in EXPECTED_SETS.iter().enumerate() {
            for (slot, &raw) in row.iter().enumerate() {
                assert_eq!(
                    opponent_berry(set, slot),
                    Ok(ItemId(raw)),
                    "set {set} slot {slot}"
                );
            }
            assert_eq!(
                opponent_berry_set(set).map(|items| items.map(ItemId::index)),
                Ok(*row),
                "set {set}"
            );
        }
    }

    #[test]
    fn the_blend_master_berries_are_pinned_in_order() {
        for (index, &raw) in EXPECTED_MASTER.iter().enumerate() {
            assert_eq!(berry_master_berry(index), Ok(ItemId(raw)), "index {index}");
        }
    }

    #[test]
    fn berry_indices_convert_by_adding_the_first_berry_item() {
        assert_eq!(FIRST_BERRY_ITEM, ItemId(133));
        assert_eq!(berry_index_item(0), ItemId::CHERI_BERRY);
        assert_eq!(berry_index_item(4), ItemId::ASPEAR_BERRY);
        assert_eq!(berry_index_item(30), ItemId::SPELON_BERRY);
        assert_eq!(berry_index_item(34), ItemId::BELUE_BERRY);
        assert_eq!(berry_index_item(42), ItemId::ENIGMA_BERRY);
        assert_eq!(berry_index_item(255), ItemId(133 + 255));
    }

    #[test]
    fn the_sets_draw_three_distinct_berries_from_the_five_npc_berries() {
        let npc = [CHERI, CHESTO, PECHA, RAWST, ASPEAR];
        for set in 0..OPPONENT_BERRY_SET_COUNT {
            let row = opponent_berry_set(set).unwrap().map(ItemId::index);
            assert!(row.iter().all(|raw| npc.contains(raw)), "set {set}");
            assert!(
                row[0] != row[1] && row[0] != row[2] && row[1] != row[2],
                "set {set}"
            );
        }
        assert_eq!(OPPONENT_BERRY_SET_COUNT, 10);
    }

    #[test]
    fn out_of_range_indices_are_refused() {
        assert_eq!(
            opponent_berry_set(10),
            Err(BlenderNpcBerryError::InvalidSet(10))
        );
        assert_eq!(
            opponent_berry(10, 0),
            Err(BlenderNpcBerryError::InvalidSet(10))
        );
        assert_eq!(
            opponent_berry(9, 3),
            Err(BlenderNpcBerryError::InvalidSlot(3))
        );
        assert_eq!(
            opponent_berry_set(usize::MAX),
            Err(BlenderNpcBerryError::InvalidSet(usize::MAX))
        );
        assert_eq!(
            opponent_berry(0, usize::MAX),
            Err(BlenderNpcBerryError::InvalidSlot(usize::MAX))
        );
        assert_eq!(
            berry_master_berry(5),
            Err(BlenderNpcBerryError::InvalidBerryMasterBerry(5))
        );
        assert_eq!(
            berry_master_berry(usize::MAX),
            Err(BlenderNpcBerryError::InvalidBerryMasterBerry(usize::MAX))
        );
    }

    #[test]
    fn errors_name_the_offending_index() {
        assert!(BlenderNpcBerryError::InvalidSet(12)
            .to_string()
            .contains("`12`"));
        assert!(BlenderNpcBerryError::InvalidSlot(7)
            .to_string()
            .contains("`7`"));
        assert!(BlenderNpcBerryError::InvalidBerryMasterBerry(9)
            .to_string()
            .contains("`9`"));
    }

    fn upstream_file(relative: &str) -> String {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(std::path::Path::parent)
            .expect("crate sits two levels below the workspace root")
            .join("pokeemerald");
        std::fs::read_to_string(root.join(relative)).unwrap_or_else(|error| {
            panic!("cannot read pokeemerald/{relative}: {error}; run ./init.sh")
        })
    }

    /// The `ITEM_*_BERRY` names in a stretch of source, as raw item ids.
    fn berry_items(text: &str, items: &std::collections::HashMap<String, u16>) -> Vec<u16> {
        text.split("ITEM_TO_BERRY(")
            .skip(1)
            .map(|rest| {
                let name = rest.split(')').next().expect("closing parenthesis");
                *items
                    .get(name)
                    .unwrap_or_else(|| panic!("unknown item `{name}`"))
            })
            .collect()
    }

    #[test]
    #[ignore = "reads the pinned upstream checkout; run ./init.sh first"]
    fn upstream_source_matches_the_tables() {
        let items: std::collections::HashMap<String, u16> =
            upstream_file("include/constants/items.h")
                .lines()
                .filter_map(|line| {
                    let mut words = line.split_whitespace();
                    (words.next()? == "#define").then_some(())?;
                    let name = words.next()?.to_owned();
                    let value = words.next()?.parse().ok()?;
                    Some((name, value))
                })
                .collect();
        let source = upstream_file("src/berry_blender.c");

        let body = |declaration: &str| -> String {
            let start = source
                .find(declaration)
                .unwrap_or_else(|| panic!("`{declaration}` not found"));
            let rest = &source[start..];
            let open = rest.find('{').expect("initializer opens");
            let close = rest.find("};").expect("initializer closes");
            rest[open + 1..close].to_owned()
        };

        let sets = body("sOpponentBerrySets[");
        let lines: Vec<&str> = sets
            .lines()
            .filter(|line| line.trim().starts_with('{'))
            .collect();
        assert_eq!(lines.len(), OPPONENT_BERRY_SET_COUNT);
        for (set, line) in lines.iter().enumerate() {
            let code = line.split("//").next().expect("code before comment");
            let upstream = berry_items(code, &items);
            assert_eq!(
                upstream.as_slice(),
                EXPECTED_SETS[set].as_slice(),
                "set {set}"
            );
            assert_eq!(
                opponent_berry_set(set).map(|row| row.map(ItemId::index)),
                Ok(EXPECTED_SETS[set]),
                "set {set}"
            );
        }

        let master = berry_items(&body("sBerryMasterBerries[]"), &items);
        assert_eq!(master.as_slice(), EXPECTED_MASTER.as_slice());

        assert!(source.contains("#define NUM_NPC_BERRIES ITEM_TO_BERRY(ITEM_ASPEAR_BERRY)"));
        assert_eq!(items["ITEM_CHERI_BERRY"], FIRST_BERRY_ITEM.index());
        assert!(upstream_file("include/constants/items.h")
            .contains("#define FIRST_BERRY_INDEX ITEM_CHERI_BERRY"));
        assert!(source.contains("opponentBerryId + FIRST_BERRY_INDEX"));
    }
}
