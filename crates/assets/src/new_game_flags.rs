//! `EventScript_ResetAllMapFlags` (issue #164): the object-hide/progression
//! flag set a fresh save begins with.
//!
//! Upstream `NewGameInitData` ends by running
//! `EventScript_ResetAllMapFlags` (`pokeemerald/src/new_game.c:196`,
//! `RunScriptImmediately(EventScript_ResetAllMapFlags)`) — a script body of
//! 159 `setflag FLAG_X` commands
//! (`pokeemerald/data/scripts/new_game.inc:116-274`) that hides every NPC
//! /item ball whose *presence* is gated behind later story progress (e.g.
//! `FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM`,
//! `new_game.inc:152`, so the rival isn't standing in their own bedroom on a
//! brand-new save), followed by `call EventScript_ResetAllBerries`
//! (`new_game.inc:275`).
//!
//! [`RESET_MAP_FLAGS`] transcribes exactly the 159 `setflag` operands, in
//! their upstream script order, as the numeric ids
//! `include/constants/flags.h` gives each `FLAG_*` name. Every `FLAG_*`
//! named in the script resolves there as a plain `#define FLAG_X <hex
//! literal>` (none of the 159 fall in the `SYSTEM_FLAGS`/
//! `SPECIAL_FLAGS_START`-relative ranges), so the transcription is a
//! straight name -> literal lookup, not an expression evaluation.
//!
//! **`EventScript_ResetAllBerries` is out of scope for this table.** It
//! calls `setberrytree` exactly 80 times over `BERRY_TREE_*` ids
//! (`new_game.inc:1-113`) to seed every wild berry tree's starting
//! stage/species. This workspace has no typed berry-tree save state yet —
//! `data/scripts/berry_tree.inc` is still `pending` in the coverage ledger,
//! and no `engine::save` field owns berry-tree data (unlike
//! `EventData`/flags, which `crate::new_game_flags` feeds). Applying
//! `RESET_MAP_FLAGS` without also running `ResetAllBerries` is a documented,
//! deliberate partial port of `EventScript_ResetAllMapFlags`'s *caller*
//! (`NewGameInitData`'s single `RunScriptImmediately` line covers both) —
//! see `crates/pokeemerald-rs/src/new_game.rs`'s module docs for how the
//! consuming slice records the deferral.
//!
//! # Provenance / re-deriving this table
//!
//! Transcribed by a one-off, non-checked-in script (same convention as
//! [`crate::map_headers`]'s "Re-running the extraction" section): walk
//! `EventScript_ResetAllMapFlags`'s `setflag` operands in
//! `data/scripts/new_game.inc` in file order, and for each `FLAG_X` look up
//! its value via the line `#define FLAG_X <value>` in
//! `include/constants/flags.h`. Each array entry below keeps its source
//! `FLAG_*` name as a trailing comment specifically so a reviewer can spot-
//! check any entry against `flags.h` directly, the same self-documenting
//! shape [`crate::map_headers`]'s generated table uses for its `MUS_*`/
//! `MAPSEC_*` comments.
//!
//! The tests below pin the exact count (159) plus a spread of individual
//! entries (including both `FLAG_UNKNOWN_0x363`/`FLAG_UNKNOWN_0x393` — real
//! upstream `setflag` operands despite the name — and the first/last
//! entries) against their cited `flags.h` line numbers, so a corrupted
//! transcription can't silently drift `(behavioral-fidelity)`.

/// `FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM`
/// (`include/constants/flags.h:811`, `0x2F8`) — hides the rival's bedroom
/// object event. Named separately (rather than only appearing inside
/// [`RESET_MAP_FLAGS`]) because issue #164's own `DoD` pins this exact flag
/// set on a fresh save; callers that only care about this one flag (e.g. a
/// future NPC-visibility test in #161) can reference it by name instead of
/// a magic `0x2F8`.
pub const FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM: u16 = 0x2F8;

/// The full effect of `EventScript_ResetAllMapFlags`'s 159 `setflag`
/// commands (module docs), in upstream script order. Each id is an ordinary
/// flag (`< engine::event_data::FLAGS_COUNT`), so every entry is valid for
/// `engine::event_data::EventData::flag_set`.
#[rustfmt::skip]
pub const RESET_MAP_FLAGS: &[u16] = &[
    0x56, // FLAG_HIDE_CONTEST_POKE_BALL
    0x301, // FLAG_HIDE_ROUTE_111_VICTORIA_WINSTRATE
    0x302, // FLAG_HIDE_ROUTE_111_VIVI_WINSTRATE
    0x303, // FLAG_HIDE_ROUTE_111_VICKY_WINSTRATE
    0x2D1, // FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_BIRCH
    0x379, // FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_RIVAL
    0x32B, // FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_POKEBALL_CYNDAQUIL
    0x32C, // FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_POKEBALL_TOTODILE
    0x346, // FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_POKEBALL_CHIKORITA
    0x2D6, // FLAG_HIDE_PETALBURG_CITY_WALLY
    0x363, // FLAG_UNKNOWN_0x363
    0x2DB, // FLAG_HIDE_RUSTBORO_CITY_AQUA_GRUNT
    0x2DC, // FLAG_HIDE_RUSTBORO_CITY_DEVON_EMPLOYEE_1
    0x32E, // FLAG_HIDE_RUSTBORO_CITY_RIVAL
    0x34C, // FLAG_HIDE_RUSTBORO_CITY_SCIENTIST
    0x364, // FLAG_HIDE_LITTLEROOT_TOWN_FAT_MAN
    0x2E3, // FLAG_HIDE_BRINEYS_HOUSE_MR_BRINEY
    0x371, // FLAG_HIDE_BRINEYS_HOUSE_PEEKO
    0x2E2, // FLAG_HIDE_ROUTE_104_MR_BRINEY
    0x2E4, // FLAG_HIDE_MR_BRINEY_DEWFORD_TOWN
    0x2E5, // FLAG_HIDE_ROUTE_109_MR_BRINEY
    0x2E7, // FLAG_HIDE_MR_BRINEY_BOAT_DEWFORD_TOWN
    0x2E8, // FLAG_HIDE_ROUTE_109_MR_BRINEY_BOAT
    0x38A, // FLAG_HIDE_ROUTE_104_WHITE_HERB_FLORIST
    0x345, // FLAG_HIDE_ROUTE_110_BIRCH
    0x306, // FLAG_HIDE_LILYCOVE_CONTEST_HALL_CONTEST_ATTENDANT_1
    0x37F, // FLAG_HIDE_LILYCOVE_CONTEST_HALL_CONTEST_ATTENDANT_2
    0x308, // FLAG_HIDE_LILYCOVE_MUSEUM_PATRON_1
    0x309, // FLAG_HIDE_LILYCOVE_MUSEUM_PATRON_2
    0x30A, // FLAG_HIDE_LILYCOVE_MUSEUM_PATRON_3
    0x30B, // FLAG_HIDE_LILYCOVE_MUSEUM_PATRON_4
    0x30C, // FLAG_HIDE_LILYCOVE_MUSEUM_TOURISTS
    0x30D, // FLAG_HIDE_PETALBURG_GYM_GREETER
    0x338, // FLAG_HIDE_PETALBURG_GYM_WALLYS_DAD
    0x2E9, // FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_BRENDAN
    0x2EA, // FLAG_HIDE_LITTLEROOT_TOWN_MAYS_HOUSE_MAY
    0x2F8, // FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM
    0x2D2, // FLAG_HIDE_LITTLEROOT_TOWN_MAYS_HOUSE_RIVAL_BEDROOM
    0x2DE, // FLAG_HIDE_PLAYERS_HOUSE_DAD
    0x351, // FLAG_HIDE_LITTLEROOT_TOWN_MAYS_HOUSE_2F_PICHU_DOLL
    0x32F, // FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F_SWABLU_DOLL
    0x315, // FLAG_HIDE_FANCLUB_OLD_LADY
    0x316, // FLAG_HIDE_FANCLUB_BOY
    0x317, // FLAG_HIDE_FANCLUB_LITTLE_BOY
    0x318, // FLAG_HIDE_FANCLUB_LADY
    0x2DA, // FLAG_HIDE_LILYCOVE_FAN_CLUB_INTERVIEWER
    0x31D, // FLAG_HIDE_ROUTE_118_GABBY_AND_TY_1
    0x31E, // FLAG_HIDE_ROUTE_120_GABBY_AND_TY_1
    0x31F, // FLAG_HIDE_ROUTE_111_GABBY_AND_TY_2
    0x385, // FLAG_HIDE_ROUTE_118_GABBY_AND_TY_2
    0x386, // FLAG_HIDE_ROUTE_120_GABBY_AND_TY_2
    0x387, // FLAG_HIDE_ROUTE_111_GABBY_AND_TY_3
    0x388, // FLAG_HIDE_ROUTE_118_GABBY_AND_TY_3
    0x323, // FLAG_HIDE_SLATEPORT_CITY_CONTEST_REPORTER
    0x322, // FLAG_HIDE_LILYCOVE_CONTEST_HALL_REPORTER
    0x326, // FLAG_HIDE_VERDANTURF_TOWN_WANDAS_HOUSE_WALLY
    0x328, // FLAG_HIDE_VERDANTURF_TOWN_WANDAS_HOUSE_WANDAS_BOYFRIEND
    0x329, // FLAG_HIDE_VERDANTURF_TOWN_WANDAS_HOUSE_WALLYS_UNCLE
    0x3D8, // FLAG_HIDE_VERDANTURF_TOWN_WANDAS_HOUSE_WANDA
    0x2FE, // FLAG_HIDE_VERDANTURF_TOWN_SCOTT
    0x33E, // FLAG_HIDE_PETALBURG_CITY_WALLYS_DAD
    0x362, // FLAG_HIDE_PETALBURG_GYM_WALLY
    0x365, // FLAG_HIDE_SLATEPORT_CITY_STERNS_SHIPYARD_MR_BRINEY
    0x33C, // FLAG_HIDE_SEAFLOOR_CAVERN_ROOM_9_ARCHIE
    0x33D, // FLAG_HIDE_SEAFLOOR_CAVERN_ROOM_9_MAXIE
    0x33F, // FLAG_HIDE_SEAFLOOR_CAVERN_ROOM_9_MAGMA_GRUNTS
    0x35B, // FLAG_HIDE_SEAFLOOR_CAVERN_ROOM_9_KYOGRE
    0x355, // FLAG_HIDE_MAGMA_HIDEOUT_4F_GROUDON
    0x349, // FLAG_HIDE_SLATEPORT_CITY_HARBOR_CAPTAIN_STERN
    0x34D, // FLAG_HIDE_SLATEPORT_CITY_HARBOR_AQUA_GRUNT
    0x34E, // FLAG_HIDE_SLATEPORT_CITY_HARBOR_ARCHIE
    0x35C, // FLAG_HIDE_SLATEPORT_CITY_HARBOR_SS_TIDAL
    0x35D, // FLAG_HIDE_LILYCOVE_HARBOR_SSTIDAL
    0x343, // FLAG_HIDE_SLATEPORT_CITY_GABBY_AND_TY
    0x348, // FLAG_HIDE_SLATEPORT_CITY_CAPTAIN_STERN
    0x350, // FLAG_HIDE_SLATEPORT_CITY_HARBOR_SUBMARINE_SHADOW
    0x353, // FLAG_HIDE_ROUTE_119_RIVAL
    0x312, // FLAG_HIDE_ROUTE_119_SCOTT
    0x3CD, // FLAG_HIDE_SOOTOPOLIS_CITY_STEVEN
    0x330, // FLAG_HIDE_SOOTOPOLIS_CITY_WALLACE
    0x366, // FLAG_HIDE_LANETTES_HOUSE_LANETTE
    0x368, // FLAG_HIDE_TRICK_HOUSE_ENTRANCE_MAN
    0x36D, // FLAG_HIDE_MT_CHIMNEY_TRAINERS
    0x3E2, // FLAG_HIDE_MT_CHIMNEY_LAVA_COOKIE_LADY
    0x36F, // FLAG_HIDE_RUSTURF_TUNNEL_BRINEY
    0x37B, // FLAG_HIDE_ROUTE_116_MR_BRINEY
    0x370, // FLAG_HIDE_RUSTURF_TUNNEL_PEEKO
    0x36E, // FLAG_HIDE_RUSTURF_TUNNEL_AQUA_GRUNT
    0x327, // FLAG_HIDE_RUSTURF_TUNNEL_WANDAS_BOYFRIEND
    0x3D7, // FLAG_HIDE_RUSTURF_TUNNEL_WANDA
    0x376, // FLAG_HIDE_SLATEPORT_CITY_OCEANIC_MUSEUM_2F_ARCHIE
    0x374, // FLAG_HIDE_SLATEPORT_CITY_OCEANIC_MUSEUM_2F_AQUA_GRUNT_1
    0x375, // FLAG_HIDE_SLATEPORT_CITY_OCEANIC_MUSEUM_2F_AQUA_GRUNT_2
    0x3C1, // FLAG_HIDE_SLATEPORT_MUSEUM_POPULATION
    0x378, // FLAG_HIDE_BATTLE_TOWER_OPPONENT
    0x2F0, // FLAG_HIDE_LITTLEROOT_TOWN_MOM_OUTSIDE
    0x2F5, // FLAG_HIDE_LITTLEROOT_TOWN_PLAYERS_BEDROOM_MOM
    0x31A, // FLAG_HIDE_LITTLEROOT_TOWN_RIVAL
    0x31B, // FLAG_HIDE_LITTLEROOT_TOWN_BIRCH
    0x37C, // FLAG_HIDE_WEATHER_INSTITUTE_1F_WORKERS
    0x380, // FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_UNKNOWN_0x380
    0x381, // FLAG_HIDE_ROUTE_101_BIRCH
    0x382, // FLAG_HIDE_ROUTE_103_BIRCH
    0x38D, // FLAG_HIDE_LILYCOVE_HARBOR_FERRY_SAILOR
    0x2EC, // FLAG_HIDE_LILYCOVE_HARBOR_EVENT_TICKET_TAKER
    0x38E, // FLAG_HIDE_SOUTHERN_ISLAND_EON_STONE
    0x38F, // FLAG_HIDE_SOUTHERN_ISLAND_UNCHOSEN_EON_DUO_MON
    0x393, // FLAG_UNKNOWN_0x393
    0x358, // FLAG_HIDE_MT_PYRE_SUMMIT_MAXIE
    0x390, // FLAG_HIDE_MAUVILLE_CITY_WATTSON
    0x2FD, // FLAG_HIDE_MAUVILLE_CITY_SCOTT
    0x398, // FLAG_HIDE_CHAMPIONS_ROOM_RIVAL
    0x399, // FLAG_HIDE_CHAMPIONS_ROOM_BIRCH
    0x39A, // FLAG_HIDE_ROUTE_110_RIVAL_ON_BIKE
    0x39B, // FLAG_HIDE_ROUTE_119_RIVAL_ON_BIKE
    0x2CF, // FLAG_HIDE_ROUTE_104_RIVAL
    0x39D, // FLAG_HIDE_LILYCOVE_MOTEL_GAME_DESIGNERS
    0x3A1, // FLAG_HIDE_LAVARIDGE_TOWN_RIVAL
    0x3A2, // FLAG_HIDE_LAVARIDGE_TOWN_RIVAL_ON_BIKE
    0x3A6, // FLAG_HIDE_MOSSDEEP_CITY_HOUSE_2_WINGULL
    0x3AA, // FLAG_HIDE_METEOR_FALLS_TEAM_AQUA
    0x3AC, // FLAG_HIDE_DEWFORD_HALL_SLUDGE_BOMB_MAN
    0x3A0, // FLAG_HIDE_FALLARBOR_HOUSE_PROF_COZMO
    0x3E0, // FLAG_HIDE_WEATHER_INSTITUTE_2F_AQUA_GRUNT_M
    0x342, // FLAG_HIDE_ROUTE_128_STEVEN
    0x3B0, // FLAG_HIDE_ROUTE_128_ARCHIE
    0x3B1, // FLAG_HIDE_ROUTE_128_MAXIE
    0x3B3, // FLAG_HIDE_ROUTE_116_DEVON_EMPLOYEE
    0x3B4, // FLAG_HIDE_SLATEPORT_CITY_TM_SALESMAN
    0x2ED, // FLAG_HIDE_SLATEPORT_CITY_SCOTT
    0x35A, // FLAG_HIDE_VICTORY_ROAD_ENTRANCE_WALLY
    0x2EF, // FLAG_HIDE_VICTORY_ROAD_EXIT_WALLY
    0x3B6, // FLAG_HIDE_SS_TIDAL_CORRIDOR_MR_BRINEY
    0x3C7, // FLAG_HIDE_MOSSDEEP_CITY_STEVENS_HOUSE_STEVEN
    0x3C8, // FLAG_HIDE_MOSSDEEP_CITY_STEVENS_HOUSE_BELDUM_POKEBALL
    0x2D7, // FLAG_HIDE_MOSSDEEP_CITY_STEVENS_HOUSE_INVISIBLE_NINJA_BOY
    0x3D3, // FLAG_HIDE_OLDALE_TOWN_RIVAL
    0x3DF, // FLAG_HIDE_ROUTE_101_BOY
    0x3E3, // FLAG_HIDE_PETALBURG_CITY_SCOTT
    0x3E4, // FLAG_HIDE_SOOTOPOLIS_CITY_RAYQUAZA
    0x3E5, // FLAG_HIDE_SOOTOPOLIS_CITY_KYOGRE
    0x3E6, // FLAG_HIDE_SOOTOPOLIS_CITY_GROUDON
    0x356, // FLAG_HIDE_SOOTOPOLIS_CITY_RESIDENTS
    0x33A, // FLAG_HIDE_SOOTOPOLIS_CITY_ARCHIE
    0x33B, // FLAG_HIDE_SOOTOPOLIS_CITY_MAXIE
    0x36C, // FLAG_HIDE_ROUTE_111_DESERT_FOSSIL
    0x36B, // FLAG_HIDE_ROUTE_111_PLAYER_DESCENT
    0x36A, // FLAG_HIDE_DESERT_UNDERPASS_FOSSIL
    0x337, // FLAG_HIDE_MOSSDEEP_CITY_TEAM_MAGMA
    0x2F4, // FLAG_HIDE_MOSSDEEP_CITY_SPACE_CENTER_1F_TEAM_MAGMA
    0x35E, // FLAG_HIDE_MOSSDEEP_CITY_SPACE_CENTER_2F_TEAM_MAGMA
    0x35F, // FLAG_HIDE_MOSSDEEP_CITY_SPACE_CENTER_2F_STEVEN
    0x340, // FLAG_HIDE_LILYCOVE_CONTEST_HALL_BLEND_MASTER
    0x2FB, // FLAG_HIDE_DEOXYS
    0x2EB, // FLAG_HIDE_SAFARI_ZONE_SOUTH_EAST_EXPANSION
    0x2FF, // FLAG_HIDE_FALLARBOR_TOWN_BATTLE_TENT_SCOTT
    0x319, // FLAG_HIDE_EVER_GRANDE_POKEMON_CENTER_1F_SCOTT
    0x357, // FLAG_HIDE_SKY_PILLAR_WALLACE
    0x50, // FLAG_HIDE_SKY_PILLAR_TOP_RAYQUAZA_STILL
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the exact `setflag` count: `data/scripts/new_game.inc:116-274`
    /// (`EventScript_ResetAllMapFlags`) has exactly 159 `setflag` lines
    /// before `call EventScript_ResetAllBerries` (`new_game.inc:275`).
    #[test]
    fn reset_map_flags_has_the_upstream_count() {
        assert_eq!(RESET_MAP_FLAGS.len(), 159);
    }

    /// Every id is an ordinary flag id — none of the 159 fall in or past
    /// `SPECIAL_FLAGS_START` (`0x4000`) or any other special range, so every
    /// entry is a plain `EventData::flag_set` call away from being applied
    /// (module docs).
    #[test]
    fn reset_map_flags_are_all_ordinary_ids() {
        // `engine::event_data::FLAGS_COUNT` (`DAILY_FLAGS_END + 1`) is
        // 0x960; not depended on directly here to keep `assets` free of an
        // `engine` dependency (`(oop-boundaries)`), but every one of these
        // literal ids is well under it.
        for &id in RESET_MAP_FLAGS {
            assert!(id < 0x960, "flag id {id:#X} outside the ordinary range");
        }
    }

    /// Spot-checks a spread of entries (first, last, both `FLAG_UNKNOWN_*`
    /// oddities, and the `DoD`-pinned rival-bedroom flag) against their cited
    /// `include/constants/flags.h` line numbers, so a transcription slip
    /// anywhere in the 159-entry table is caught even without re-deriving
    /// the whole list `(behavioral-fidelity)`.
    #[test]
    fn reset_map_flags_spot_checks_match_flags_h() {
        // include/constants/flags.h:104
        assert_eq!(RESET_MAP_FLAGS[0], 0x56); // FLAG_HIDE_CONTEST_POKE_BALL
                                              // include/constants/flags.h:918 ("Set, however has no purpose.")
        assert_eq!(RESET_MAP_FLAGS[10], 0x363); // FLAG_UNKNOWN_0x363
                                                // include/constants/flags.h:811 -- the DoD-pinned flag.
        assert_eq!(
            RESET_MAP_FLAGS[36],
            FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM
        );
        // include/constants/flags.h:966 ("Set, however has no purpose.")
        assert_eq!(RESET_MAP_FLAGS[107], 0x393); // FLAG_UNKNOWN_0x393
                                                 // include/constants/flags.h:1019
        assert_eq!(RESET_MAP_FLAGS[134], 0x3C8); // ..._BELDUM_POKEBALL
                                                 // include/constants/flags.h:814
        assert_eq!(RESET_MAP_FLAGS[153], 0x2FB); // FLAG_HIDE_DEOXYS
                                                 // include/constants/flags.h:96 -- the last setflag before `call
                                                 // EventScript_ResetAllBerries` (new_game.inc:274 vs :275).
        assert_eq!(RESET_MAP_FLAGS[158], 0x50); // ..._SKY_PILLAR_TOP_RAYQUAZA_STILL
    }

    /// Upstream's `setflag` list never repeats a flag id (each `FLAG_*` name
    /// in the script is distinct, and none alias); a duplicate here would
    /// mean a copy/paste transcription error even though it's harmless at
    /// runtime (`flag_set` is idempotent).
    #[test]
    fn reset_map_flags_has_no_duplicate_ids() {
        let mut sorted = RESET_MAP_FLAGS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), RESET_MAP_FLAGS.len());
    }

    /// The pinned rival-bedroom flag is actually one of the 159 -- if it
    /// weren't, [`FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM`]
    /// would be a dead constant instead of describing real reset behaviour.
    #[test]
    fn rival_bedroom_flag_is_in_the_reset_list() {
        assert!(RESET_MAP_FLAGS.contains(&FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM));
    }
}
