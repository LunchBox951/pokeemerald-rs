//! `ObjectEvent::flag` name -> numeric flag id resolution (issue #161).
//!
//! [`map_events::ObjectEvent::flag`](crate::map_events::ObjectEvent::flag)
//! transcribes each object event's upstream `flagId` as the literal
//! `FLAG_*` name from `data/maps/<Name>/map.json` (see that module's docs:
//! this crate owns neither a flags/vars store nor a full `flags.h` name
//! table, so turning every flag reference into a number would mean
//! inventing a mapping rather than transcribing one). [`resolve`] is that
//! missing mapping -- but only for the closed set of `FLAG_*` names that can
//! actually appear on a map this port ever loads.
//!
//! # Why a bounded table, not all of `flags.h`
//!
//! `include/constants/flags.h` defines hundreds of `FLAG_*` names across the
//! whole game; this port's own extraction pipeline
//! (`crates/xtask/src/extract/mod.rs`'s `LAYOUTS`) only ever bundles the
//! Littleroot Town layout family (the town itself, both player houses'
//! floors, Professor Birch's lab), so [`resolve`] only ever needs to answer
//! for a flag name actually reachable from one of *those* maps' own
//! `object_events` -- every such name (35, spot-checked against every
//! object event on all six maps, plus the twelve generic `FLAG_DECORATION_N`
//! ids Littleroot's two player-house bedrooms also carry) is transcribed
//! below. A name outside this set can never occur for any map this port
//! renders, so extending the table further would transcribe data no code
//! here consumes -- the same bounded-scope reasoning
//! `crates/pokeemerald-rs/src/overworld/mod.rs`'s `resolve_tileset_pack_name`
//! already applies to the five tilesets that pipeline bundles.
//!
//! Every id below was independently looked up in
//! `include/constants/flags.h` (not re-derived from
//! [`RESET_MAP_FLAGS`](crate::new_game_flags::RESET_MAP_FLAGS)), so this
//! table and that one cross-check each other: a flag present in both must
//! agree on its numeric id (pinned by
//! `object_event_flags_agree_with_reset_map_flags`).

/// `(FLAG_* name, numeric id)` pairs for every `ObjectEvent::flag` value
/// reachable from a map this port's extraction pipeline bundles (module
/// docs). The literal `"0"` sentinel (upstream: no flag, object never
/// hidden) is handled directly by [`resolve`], not listed here.
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
];

/// Resolve an `ObjectEvent::flag` string into the numeric id
/// [`engine::event_data::EventData::flag_get`](../../engine/event_data/struct.EventData.html#method.flag_get)
/// expects.
///
/// `"0"` (upstream: `flagId == 0`, meaning "no flag" -- the object is never
/// hidden) resolves to `Some(0)`: `EventData::flag_get(0)` already tolerates
/// id `0` as a permanent, always-`false` no-op (that module's own docs), so
/// this is the same "never hidden" outcome expressed as a real, checkable
/// id rather than a special early return.
///
/// Returns `None` for any name outside [`OBJECT_EVENT_FLAGS`] -- module docs
/// on why that never actually happens for a map this port loads.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_events::MapEventsTable;
    use crate::new_game_flags::RESET_MAP_FLAGS;
    use crate::wild_encounters::MapId;

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
        // include/constants/flags.h:811 -- also independently pinned by
        // `new_game_flags::FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM`.
        assert_eq!(
            resolve("FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM"),
            Some(0x2F8)
        );
        // include/constants/flags.h:809 -- distinct from, and NOT one of,
        // RESET_MAP_FLAGS's own 0x2F5 ("...PLAYERS_BEDROOM_MOM") -- the mom
        // in the player's own house is not hidden on a fresh save.
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

    /// Cross-check against the independently-transcribed
    /// [`RESET_MAP_FLAGS`] table (module docs): any name present in both
    /// must agree on its numeric id.
    #[test]
    fn shared_entries_agree_with_reset_map_flags() {
        // Every name here that also names one of RESET_MAP_FLAGS's own
        // FLAG_HIDE_LITTLEROOT_TOWN_* comments must resolve to that same id.
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

    /// Every object event on the six Littleroot-family maps this port's
    /// extraction pipeline loads must resolve -- the whole point of the
    /// bounded table (module docs). A `None` here would mean an object event
    /// this port can actually render has an unresolvable hide flag.
    #[test]
    fn every_littleroot_family_object_event_flag_resolves() {
        let table = MapEventsTable::new();
        let maps = [
            "MAP_LITTLEROOT_TOWN",
            "MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F",
            "MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F",
            "MAP_LITTLEROOT_TOWN_MAYS_HOUSE_1F",
            "MAP_LITTLEROOT_TOWN_MAYS_HOUSE_2F",
            "MAP_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB",
        ];
        for map in maps {
            let events = table.resolve(MapId(map)).unwrap();
            for object in events.object_events {
                assert!(
                    resolve(object.flag).is_some(),
                    "{map}: object event {:?} (local_id {}) has an unresolvable flag {:?}",
                    object.graphics_id,
                    object.local_id,
                    object.flag
                );
            }
        }
    }
}
