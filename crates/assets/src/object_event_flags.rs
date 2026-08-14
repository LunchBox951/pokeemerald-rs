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
//! floors, Professor Birch's lab) plus, since issue #177, `LAYOUT_ROUTE101`
//! (Littleroot's own north map connection, and I-4's traversal
//! prerequisite), plus, since issue #248, `LAYOUT_OLDALE_TOWN`/
//! `LAYOUT_ROUTE103` (the next two links of that same connection chain,
//! and I-5's Route 103 rival battle traversal prerequisite), so [`resolve`]
//! only ever needs to answer for a flag name actually reachable from one of
//! *those* maps' own `object_events` -- every such name (**53**: the 37
//! distinct `FLAG_HIDE_*` ids carried by object events across all nine
//! maps, the twelve generic `FLAG_DECORATION_1..12` ids Littleroot's two
//! player-house bedrooms also carry, and four more that are neither --
//! Route 103's own two `FLAG_TEMP_*` cuttable-tree ids and two
//! `FLAG_ITEM_*` hidden-item ids; the literal `"0"` no-flag sentinel is
//! handled by [`resolve`] itself and is not a table entry) is transcribed
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
    // Route 101 (issue #177, `data/maps/Route101/map.json`'s own
    // `object_events`) -- I-4's traversal prerequisite.
    ("FLAG_HIDE_ROUTE_101_BIRCH", 0x381),
    ("FLAG_HIDE_ROUTE_101_BIRCH_STARTERS_BAG", 0x2BC),
    ("FLAG_HIDE_ROUTE_101_BIRCH_ZIGZAGOON_BATTLE", 0x2D0),
    ("FLAG_HIDE_ROUTE_101_BOY", 0x3DF),
    ("FLAG_HIDE_ROUTE_101_ZIGZAGOON", 0x2EE),
    // Oldale Town (issue #248, `data/maps/OldaleTown/map.json`'s own
    // `object_events`) -- the second link of the Route 101 <-> Oldale Town
    // <-> Route 103 connection chain I-5's Route 103 rival battle needs.
    ("FLAG_HIDE_OLDALE_TOWN_RIVAL", 0x3D3),
    // Route 103 (issue #248, `data/maps/Route103/map.json`'s own
    // `object_events`) -- the third link, and I-5's own traversal target.
    ("FLAG_HIDE_ROUTE_103_BIRCH", 0x382),
    ("FLAG_HIDE_ROUTE_103_RIVAL", 0x2D3),
    // Route 103's two cuttable trees and two hidden items are the first
    // `FLAG_TEMP_*`/`FLAG_ITEM_*` ids this table carries: every map bundled
    // before issue #248 happened to declare none. `FLAG_TEMP_12`/`_13`
    // (`TEMP_FLAGS_START + 0x12`/`0x13`) are ordinary per-map-load-cleared
    // temp flags reused by countless other cuttable trees across the game;
    // nothing here is Route-103-specific about the *id*, only about which
    // object event on a bundled map happens to carry it.
    ("FLAG_TEMP_12", 0x12),
    ("FLAG_TEMP_13", 0x13),
    ("FLAG_ITEM_ROUTE_103_GUARD_SPEC", 0x45A),
    ("FLAG_ITEM_ROUTE_103_PP_UP", 0x471),
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

/// `FLAG_DECORATION_1` .. `FLAG_DECORATION_14`
/// (`include/constants/flags.h:196-209`), in the order upstream's
/// `SecretBase_EventScript_SetDecorationFlags` sets them
/// (`data/scripts/secret_base.inc:233-248`).
///
/// **Why this is a table of its own.** Every decoration object event is a
/// *placeholder*: a slot in the player's bedroom (or a secret base) that
/// only becomes a real object once a decoration is placed there. The flag
/// polarity is inverted from every other `FLAG_HIDE_*` in
/// [`OBJECT_EVENT_FLAGS`]: an *unused* slot has its flag **set** (hidden),
/// and placing a decoration **clears** it. Nothing sets these at new-game
/// time — `InitEventData` (`src/event_data.c:32-37`) zeroes the whole flag
/// array and `EventScript_ResetAllMapFlags` never mentions them — so the
/// "all slots empty" state is established per map entry instead, by the
/// bedroom's own `MAP_SCRIPT_ON_TRANSITION`
/// (`data/maps/LittlerootTown_BrendansHouse_2F/scripts.inc:6-12`, and
/// May's counterpart), which calls the script above.
///
/// All fourteen are listed even though only `1..=12` are reachable from a
/// bundled map's object events ([`OBJECT_EVENT_FLAGS`]' own bound,
/// `DECOR_MAX_PLAYERS_HOUSE == 12`): the upstream script sets all fourteen
/// unconditionally, and `13`/`14` exist for secret bases
/// (`DECOR_MAX_SECRET_BASE == 16`, `include/constants/global.h:57-58`).
/// Transcribing what the script actually does keeps this a transcription
/// rather than an inference.
pub const DECORATION_FLAGS: &[u16] = &[
    0xAE, 0xAF, 0xB0, 0xB1, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xBA, 0xBB,
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_events::MapEventsTable;
    use crate::map_headers::MapHeaderTable;
    use crate::new_game_flags::RESET_MAP_FLAGS;
    use crate::wild_encounters::MapId;

    /// This crate's mirror of `crates/xtask/src/extract/mod.rs`'s `LAYOUTS`
    /// -- the layout family the extraction pipeline bundles, and so the
    /// exact scope [`OBJECT_EVENT_FLAGS`] has to cover (module docs).
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

    /// Every map whose generated header names one of [`BUNDLED_LAYOUTS`] --
    /// derived from the layout list rather than restated, so bundling a new
    /// layout widens every test built on it automatically.
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

    /// The module docs' own arithmetic, pinned so it can't drift: the table
    /// is exactly 53 entries, 37 `FLAG_HIDE_*`, 12 `FLAG_DECORATION_*`, and
    /// 4 more (`FLAG_TEMP_12`/`_13`, `FLAG_ITEM_ROUTE_103_GUARD_SPEC`/
    /// `_PP_UP`) that are neither -- Route 103's cuttable trees and hidden
    /// items (issue #248).
    #[test]
    fn the_table_is_the_documented_37_hide_plus_12_decoration_plus_4_other_entries() {
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
            "module docs' own counts"
        );
        assert_eq!(
            hide + decoration + other,
            OBJECT_EVENT_FLAGS.len(),
            "every entry must be one of those three families"
        );
        assert_eq!(OBJECT_EVENT_FLAGS.len(), 53);
    }

    /// Every object event on the nine Littleroot-family-plus-Route-101-plus-
    /// Oldale-Town-plus-Route-103 maps this port's extraction pipeline loads
    /// must resolve -- the whole point of the bounded table (module docs).
    /// A `None` here would mean an object event this port can actually
    /// render has an unresolvable hide flag. The table must also carry
    /// *nothing else*: that equality is what keeps the module docs' "37 +
    /// 12 + 4" honest as map data changes.
    ///
    /// The map set is **derived**, not listed: every map whose header names
    /// one of [`BUNDLED_LAYOUTS`] (this crate's mirror of
    /// `crates/xtask/src/extract/mod.rs`'s `LAYOUTS` -- `assets` cannot
    /// depend on `xtask`) is in scope, so extending that list here
    /// automatically widens this test. `xtask`'s own
    /// `the_bundled_layout_set_is_pinned_for_the_tables_derived_from_it`
    /// is the tripwire that makes adding a layout *there* fail loudly and
    /// point back here.
    #[test]
    fn every_littleroot_family_object_event_flag_resolves() {
        let table = MapEventsTable::new();
        let maps = bundled_maps();
        assert_eq!(
            maps.len(),
            9,
            "the ten bundled layouts cover nine maps (the lab's \
             `_WITH_TABLE` variant is an alternate layout for a map already \
             listed, not a map of its own; Route 101 -- issue #177 -- Oldale \
             Town and Route 103 -- issue #248 -- each are)"
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
            "the table must be exactly the reachable set -- no unreachable \
             entries transcribed, none missing"
        );
    }
}
