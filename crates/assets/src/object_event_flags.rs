//! Resolves the `FLAG_*` names carried by bundled-map object events to
//! numeric flag IDs.
//!
//! [`map_events::ObjectEvent::flag`](crate::map_events::ObjectEvent::flag)
//! keeps the upstream `flagId` name from each map's `map.json`; [`resolve`]
//! maps it to the ID in `pokeemerald/include/constants/flags.h`. The table
//! covers exactly the names used by maps whose layouts the extraction
//! pipeline bundles, not all of `flags.h`. Its IDs were looked up
//! independently of [`RESET_MAP_FLAGS`](crate::new_game_flags::RESET_MAP_FLAGS),
//! so a flag in both tables must agree.

/// `(FLAG_* name, numeric id)` pairs; the `"0"` no-flag sentinel is handled by
/// [`resolve`] instead.
#[rustfmt::skip]
const OBJECT_EVENT_FLAGS: &[(&str, u16)] = &[
    ("FLAG_DECORATION_1", 0xAE),
    ("FLAG_DECORATION_2", 0xAF),
    ("FLAG_DECORATION_3", 0xB0),
    ("FLAG_DECORATION_4", 0xB1),
    ("FLAG_DECORATION_5", 0xB2),
    ("FLAG_DECORATION_6", 0xB3),
    ("FLAG_DECORATION_7", 0xB4),
    ("FLAG_DECORATION_8", 0xB5),
    ("FLAG_DECORATION_9", 0xB6),
    ("FLAG_DECORATION_10", 0xB7),
    ("FLAG_DECORATION_11", 0xB8),
    ("FLAG_DECORATION_12", 0xB9),
    ("FLAG_HIDE_LITTLEROOT_TOWN_BIRCH", 0x31B),
    ("FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_BIRCH", 0x2D1),
    ("FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_POKEBALL_CHIKORITA", 0x346),
    ("FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_POKEBALL_CYNDAQUIL", 0x32B),
    ("FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_POKEBALL_TOTODILE", 0x32C),
    ("FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_RIVAL", 0x379),
    ("FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F_POKE_BALL", 0x331),
    ("FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F_SWABLU_DOLL", 0x32F),
    ("FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_BRENDAN", 0x2E9),
    ("FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_MOM", 0x2F6),
    ("FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM", 0x2F8),
    ("FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_MOM", 0x310),
    ("FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_SIBLING", 0x2DF),
    ("FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_TRUCK", 0x2F9),
    ("FLAG_HIDE_LITTLEROOT_TOWN_FAT_MAN", 0x364),
    ("FLAG_HIDE_LITTLEROOT_TOWN_MAYS_HOUSE_2F_PICHU_DOLL", 0x351),
    ("FLAG_HIDE_LITTLEROOT_TOWN_MAYS_HOUSE_2F_POKE_BALL", 0x332),
    ("FLAG_HIDE_LITTLEROOT_TOWN_MAYS_HOUSE_MAY", 0x2EA),
    ("FLAG_HIDE_LITTLEROOT_TOWN_MAYS_HOUSE_MOM", 0x2F7),
    ("FLAG_HIDE_LITTLEROOT_TOWN_MAYS_HOUSE_RIVAL_BEDROOM", 0x2D2),
    ("FLAG_HIDE_LITTLEROOT_TOWN_MAYS_HOUSE_RIVAL_MOM", 0x311),
    ("FLAG_HIDE_LITTLEROOT_TOWN_MAYS_HOUSE_RIVAL_SIBLING", 0x2E0),
    ("FLAG_HIDE_LITTLEROOT_TOWN_MAYS_HOUSE_TRUCK", 0x2FA),
    ("FLAG_HIDE_LITTLEROOT_TOWN_MOM_OUTSIDE", 0x2F0),
    ("FLAG_HIDE_LITTLEROOT_TOWN_PLAYERS_BEDROOM_MOM", 0x2F5),
    ("FLAG_HIDE_LITTLEROOT_TOWN_PLAYERS_HOUSE_VIGOROTH_1", 0x2F2),
    ("FLAG_HIDE_LITTLEROOT_TOWN_PLAYERS_HOUSE_VIGOROTH_2", 0x2F3),
    ("FLAG_HIDE_LITTLEROOT_TOWN_RIVAL", 0x31A),
    ("FLAG_HIDE_PLAYERS_HOUSE_DAD", 0x2DE),
    ("FLAG_HIDE_ROUTE_101_BIRCH", 0x381),
    ("FLAG_HIDE_ROUTE_101_BIRCH_STARTERS_BAG", 0x2BC),
    ("FLAG_HIDE_ROUTE_101_BIRCH_ZIGZAGOON_BATTLE", 0x2D0),
    ("FLAG_HIDE_ROUTE_101_BOY", 0x3DF),
    ("FLAG_HIDE_ROUTE_101_ZIGZAGOON", 0x2EE),
    ("FLAG_HIDE_OLDALE_TOWN_RIVAL", 0x3D3),
    ("FLAG_HIDE_ROUTE_103_BIRCH", 0x382),
    ("FLAG_HIDE_ROUTE_103_RIVAL", 0x2D3),
    ("FLAG_TEMP_12", 0x12),
    ("FLAG_TEMP_13", 0x13),
    ("FLAG_ITEM_ROUTE_103_GUARD_SPEC", 0x45A),
    ("FLAG_ITEM_ROUTE_103_PP_UP", 0x471),
];

/// Resolves an `ObjectEvent::flag` name to its upstream numeric flag ID.
///
/// `"0"`, upstream's "no flag" `flagId`, resolves to `Some(0)`. Names outside
/// the bundled-map table return `None`.
#[must_use]
pub fn resolve(flag: &str) -> Option<u16> {
    if flag == "0" {
        return Some(0);
    }
    OBJECT_EVENT_FLAGS
        .iter()
        .find(|(name, _)| *name == flag)
        .map(|(_, id)| *id)
}

const FLAG_DECORATION_1: u16 = 0xAE;
const FLAG_DECORATION_2: u16 = 0xAF;
const FLAG_DECORATION_3: u16 = 0xB0;
const FLAG_DECORATION_4: u16 = 0xB1;
const FLAG_DECORATION_5: u16 = 0xB2;
const FLAG_DECORATION_6: u16 = 0xB3;
const FLAG_DECORATION_7: u16 = 0xB4;
const FLAG_DECORATION_8: u16 = 0xB5;
const FLAG_DECORATION_9: u16 = 0xB6;
const FLAG_DECORATION_10: u16 = 0xB7;
const FLAG_DECORATION_11: u16 = 0xB8;
const FLAG_DECORATION_12: u16 = 0xB9;
const FLAG_DECORATION_13: u16 = 0xBA;
const FLAG_DECORATION_14: u16 = 0xBB;

/// `FLAG_DECORATION_1` through `FLAG_DECORATION_14`
/// (`pokeemerald/include/constants/flags.h:196-209`), in the order
/// `SecretBase_EventScript_SetDecorationFlags` sets them
/// (`pokeemerald/data/scripts/secret_base.inc:233-248`).
///
/// Each decoration object event is a placeholder, hidden while its flag is
/// set; placing a decoration clears the flag. Nothing sets these at new-game
/// time: the bedroom map's `MAP_SCRIPT_ON_TRANSITION` runs the script above
/// (`pokeemerald/data/maps/LittlerootTown_BrendansHouse_2F/scripts.inc:6-12`,
/// and May's counterpart). All fourteen are listed because the script sets
/// all fourteen, though bundled maps use only `1..=12`.
pub const DECORATION_FLAGS: &[u16] = &[
    FLAG_DECORATION_1,
    FLAG_DECORATION_2,
    FLAG_DECORATION_3,
    FLAG_DECORATION_4,
    FLAG_DECORATION_5,
    FLAG_DECORATION_6,
    FLAG_DECORATION_7,
    FLAG_DECORATION_8,
    FLAG_DECORATION_9,
    FLAG_DECORATION_10,
    FLAG_DECORATION_11,
    FLAG_DECORATION_12,
    FLAG_DECORATION_13,
    FLAG_DECORATION_14,
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_events::MapEventsTable;
    use crate::map_headers::MapHeaderTable;
    use crate::new_game_flags::RESET_MAP_FLAGS;
    use crate::wild_encounters::MapId;

    /// Mirrors `crates/xtask/src/extract/mod.rs`'s `LAYOUTS`, which `assets`
    /// cannot depend on. xtask's
    /// `the_bundled_layout_set_is_pinned_for_the_tables_derived_from_it`
    /// fails when that manifest changes, as the prompt to update this mirror.
    const BUNDLED_LAYOUTS: [&str; 10] = [
        "LAYOUT_LITTLEROOT_TOWN",
        "LAYOUT_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F",
        "LAYOUT_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F",
        "LAYOUT_LITTLEROOT_TOWN_MAYS_HOUSE_1F",
        "LAYOUT_LITTLEROOT_TOWN_MAYS_HOUSE_2F",
        "LAYOUT_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB",
        "LAYOUT_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB_WITH_TABLE",
        "LAYOUT_ROUTE101",
        "LAYOUT_OLDALE_TOWN",
        "LAYOUT_ROUTE103",
    ];

    fn bundled_maps() -> Vec<MapId> {
        MapHeaderTable::new()
            .iter()
            .filter(|header| BUNDLED_LAYOUTS.contains(&header.layout.name()))
            .map(|header| header.id)
            .collect()
    }

    #[test]
    fn the_literal_no_flag_sentinel_resolves_to_the_tolerated_null_id() {
        assert_eq!(resolve("0"), Some(0));
    }

    #[test]
    fn an_unknown_name_resolves_to_none() {
        assert_eq!(resolve("FLAG_HIDE_SOME_MAP_THIS_PORT_NEVER_LOADS"), None);
    }

    #[test]
    fn a_spot_checked_name_resolves_to_its_flags_h_id() {
        // include/constants/flags.h:811
        assert_eq!(
            resolve("FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM"),
            Some(0x2F8)
        );
        // include/constants/flags.h:809
        assert_eq!(
            resolve("FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_MOM"),
            Some(0x2F6)
        );
    }

    #[test]
    fn no_duplicate_names_or_ids() {
        let mut names: Vec<_> = OBJECT_EVENT_FLAGS.iter().map(|(n, _)| *n).collect();
        names.sort_unstable();
        let mut unique_names = names.clone();
        unique_names.dedup();
        assert_eq!(names.len(), unique_names.len(), "duplicate FLAG_* name");

        let mut ids: Vec<_> = OBJECT_EVENT_FLAGS.iter().map(|(_, id)| *id).collect();
        ids.sort_unstable();
        let mut unique_ids = ids.clone();
        unique_ids.dedup();
        assert_eq!(ids.len(), unique_ids.len(), "duplicate flag id");
    }

    #[test]
    fn shared_entries_agree_with_reset_map_flags() {
        let shared = [
            ("FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_BRENDAN", 0x2E9),
            ("FLAG_HIDE_LITTLEROOT_TOWN_MAYS_HOUSE_MAY", 0x2EA),
            (
                "FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM",
                0x2F8,
            ),
            ("FLAG_HIDE_LITTLEROOT_TOWN_MAYS_HOUSE_RIVAL_BEDROOM", 0x2D2),
            ("FLAG_HIDE_PLAYERS_HOUSE_DAD", 0x2DE),
            ("FLAG_HIDE_LITTLEROOT_TOWN_FAT_MAN", 0x364),
            ("FLAG_HIDE_LITTLEROOT_TOWN_MOM_OUTSIDE", 0x2F0),
            ("FLAG_HIDE_LITTLEROOT_TOWN_PLAYERS_BEDROOM_MOM", 0x2F5),
            ("FLAG_HIDE_LITTLEROOT_TOWN_RIVAL", 0x31A),
            ("FLAG_HIDE_LITTLEROOT_TOWN_BIRCH", 0x31B),
            (
                "FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_POKEBALL_CYNDAQUIL",
                0x32B,
            ),
            (
                "FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_POKEBALL_TOTODILE",
                0x32C,
            ),
            (
                "FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_POKEBALL_CHIKORITA",
                0x346,
            ),
            ("FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_BIRCH", 0x2D1),
            ("FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_RIVAL", 0x379),
        ];
        for (name, expected) in shared {
            assert_eq!(resolve(name), Some(expected), "{name} disagrees");
            assert!(
                RESET_MAP_FLAGS.contains(&expected),
                "{name} ({expected:#x}) should be one of RESET_MAP_FLAGS's own ids"
            );
        }
    }

    #[test]
    fn object_event_flag_table_has_expected_family_counts() {
        let hide = OBJECT_EVENT_FLAGS
            .iter()
            .filter(|(name, _)| name.starts_with("FLAG_HIDE_"))
            .count();
        let decoration = OBJECT_EVENT_FLAGS
            .iter()
            .filter(|(name, _)| name.starts_with("FLAG_DECORATION_"))
            .count();
        let other = OBJECT_EVENT_FLAGS
            .iter()
            .filter(|(name, _)| {
                !name.starts_with("FLAG_HIDE_") && !name.starts_with("FLAG_DECORATION_")
            })
            .count();
        assert_eq!(
            (hide, decoration, other),
            (37, 12, 4),
            "FLAG_HIDE_*, FLAG_DECORATION_*, and other entries"
        );
        assert_eq!(
            hide + decoration + other,
            OBJECT_EVENT_FLAGS.len(),
            "every entry must be one of those three families"
        );
        assert_eq!(OBJECT_EVENT_FLAGS.len(), 53);
    }

    #[test]
    fn object_event_flag_table_exactly_covers_bundled_maps() {
        let table = MapEventsTable::new();
        let maps = bundled_maps();
        assert_eq!(
            maps.len(),
            9,
            "the ten bundled layouts cover nine maps (the lab's \
             `_WITH_TABLE` variant is an alternate layout for a map already \
             listed)"
        );
        let mut reachable: Vec<&str> = Vec::new();
        for map in maps {
            let events = table.resolve(map).unwrap();
            for object in events.object_events {
                assert!(
                    resolve(object.flag).is_some(),
                    "{map:?}: object event {:?} (local_id {}) has an unresolvable flag {:?}",
                    object.graphics_id,
                    object.local_id,
                    object.flag
                );
                if object.flag != "0" && !reachable.contains(&object.flag) {
                    reachable.push(object.flag);
                }
            }
        }

        reachable.sort_unstable();
        let mut listed: Vec<&str> = OBJECT_EVENT_FLAGS.iter().map(|(name, _)| *name).collect();
        listed.sort_unstable();
        assert_eq!(
            listed, reachable,
            "the table must be exactly the reachable set"
        );
    }
}
