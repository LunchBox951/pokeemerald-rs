//! New-game state initialization (I-3, issue #149): the pure data half of
//! upstream `NewGameInitData` (`pokeemerald/src/new_game.c:149-207`), plus
//! the fixed spawn position [`intro::IntroScene`](crate::intro::IntroScene)
//! hands off to when the Birch-speech intro finishes.
//!
//! # `NewGameInitData` fields this slice mirrors
//!
//! [`init_save_blocks`] builds fresh [`SaveBlock1`]/[`SaveBlock2`] values,
//! field-for-field, for every upstream `NewGameInitData` effect this
//! workspace's [`engine::save::block`] model has a typed home for
//! (`crates/engine/src/save/block.rs`, S-5, issues #94/#117):
//!
//! | upstream (`new_game.c:149-207`)                          | here |
//! |------------------------------------------------------------|------|
//! | `gSaveBlock2Ptr->encryptionKey = 0;`                        | [`SaveBlock2::encryption_key`] `= 0` |
//! | `InitPlayerTrainerId()` (`Random() << 16 \| GetGeneratedTrainerIdLower()`) | [`SaveBlock2::player_trainer_id`], via `rng` ([`trainer_id_bytes`], deviation below) |
//! | `ZeroPlayerPartyMons()` / `gPlayerPartyCount = 0`           | [`SaveBlock1::player_party`]/`player_party_count` `= 0` ([`SaveBlock1::default`]) |
//! | `SetMoney(&gSaveBlock1Ptr->money, 3000)`                    | [`SaveBlock1::money`] `= `[`STARTING_MONEY`] |
//! | `ClearBag()`                                                | [`SaveBlock1::bag`] `= Bag::default()` |
//! | `InitEventData()`                                           | [`SaveBlock1::event_data`] `= EventData::default()` |
//! | `RunScriptImmediately(EventScript_ResetAllMapFlags)`'s 159 `setflag`s (`data/scripts/new_game.inc:116-274`) | [`SaveBlock1::event_data`]`.flag_set` for every id in [`assets::RESET_MAP_FLAGS`] (issue #164) |
//! | (skipped truck sequence) `InsideOfTruck_EventScript_SetIntroFlags`'s gender branch (`data/maps/InsideOfTruck/scripts.inc:16-48`) | [`apply_truck_intro_flags`] — [`assets::TRUCK_INTRO_FLAGS_MALE`]/`_FEMALE` plus their vars (issue #161 review) |
//! | (skipped truck sequence) `InsideOfTruck_EventScript_SetIntroFlags{Male,Female}`'s `setrespawn` (`data/maps/InsideOfTruck/scripts.inc:25,38`) | [`SaveBlock1::last_heal_location`] `= `[`default_last_heal_location`] (issue #261) |
//! | (naming screen / `Task_NewGameBirchSpeech_ChooseGender`)    | [`SaveBlock2::player_name`]/`player_gender` `= `[`DEFAULT_PLAYER_NAME`]`/`[`DEFAULT_PLAYER_GENDER`] (deviation below) |
//!
//! **Consumed as of issue #161:** the flag set written here is no longer
//! inert save state. `engine::overworld::object_event_is_visible` gates
//! every object-event query on it, so these flags now decide what renders,
//! what can be talked to, and what blocks movement — which is why the
//! skipped truck sequence's own gender branch had to be reproduced
//! alongside `EventScript_ResetAllMapFlags` (table row above,
//! [`apply_truck_intro_flags`]): with it missing, a fresh save reached the
//! player's house with the *rival's* family and Poké Ball still spawned.
//!
//! **Partially deferred (issue #164):** `EventScript_ResetAllMapFlags` ends
//! with `call EventScript_ResetAllBerries` (`new_game.inc:275`), which seeds
//! every wild berry tree's starting species/stage via 80 `setberrytree`
//! commands (`new_game.inc:1-113`). This workspace has no typed berry-tree
//! save state yet -- `data/scripts/berry_tree.inc` is still `pending` in the
//! coverage ledger, and no [`SaveBlock1`]/[`SaveBlock2`] field owns it (the
//! same "no typed home yet" reasoning the module docs' "deliberately not
//! mirrored" list below already applies to `ClearSecretBases`, `SetCoins`,
//! etc.). [`init_save_blocks`] therefore applies exactly
//! [`assets::RESET_MAP_FLAGS`] (the flag half) and leaves berry-tree state
//! untouched; a future slice that gives berry trees a save-state home should
//! extend this function to also run `ResetAllBerries`'s effect, at which
//! point this paragraph should be deleted rather than left stale.
//!
//! Deliberately **not** mirrored (no typed model exists yet, or genuinely
//! out of this issue's scope): `ResetPokedex`, `ClearFrontierRecord`,
//! `ClearAllMail`, `PlayTimeCounter_Reset`, `ClearTVShowData`,
//! `ClearSecretBases`, `ClearBerryTrees`, `SetCoins`, `ResetGameStats`,
//! `ClearAllContestWinnerPics`, `ResetPokemonStorageSystem`,
//! `gSaveBlock1Ptr->registeredItem`, `NewGameInitPCItems`, `ClearPokeblocks`,
//! `ClearDecorationInventories`, `InitEasyChatPhrases`, and every other
//! `new_game.c` call this workspace has no save-state field for. These stay
//! at their [`SaveBlock1`]/[`SaveBlock2`] default (zeroed) values, which is
//! exactly what `to_bytes` already does for every unmodeled offset (see
//! `engine::save::block`'s module docs) -- no silent data loss beyond what
//! that model already documents.
//!
//! # Documented deviations from `NewGameInitData`
//!
//! - **No truck sequence.** Upstream's `NewGameInitData` ends with
//!   `WarpToTruck()` -- `SetWarpDestination(MAP_INSIDE_OF_TRUCK, ...)` --
//!   the player boards the moving truck, and only reaches Littleroot Town
//!   (and their own bedroom) after the separate truck-ride cutscene
//!   (`src/field_specials.c`'s truck sequence, not ported here). This
//!   issue's own brief calls for spawning directly "in the protagonist's
//!   room" -- the same room
//!   [`overworld::load_default_room`](crate::overworld::load_default_room)
//!   already treats as the early playable slice's starting point (issue #126). This
//!   slice skips the truck entirely rather than modeling a cutscene with no
//!   consumer; [`SaveBlock1::location`] is set to the bedroom directly
//!   instead of `MAP_INSIDE_OF_TRUCK`. Its one *observable* side effect is
//!   reproduced rather than skipped with it: the exit-tile trigger
//!   `InsideOfTruck_EventScript_SetIntroFlags`, which every real
//!   playthrough passes through before reaching a house — see
//!   [`apply_truck_intro_flags`].
//! - **Fixed player name/gender, no naming screen.** Upstream's Birch-speech
//!   task chain runs a gender-select menu
//!   (`Task_NewGameBirchSpeech_ChooseGender`) and the real naming screen
//!   (`DoNamingScreen`, `Task_NewGameBirchSpeech_StartNamingScreen`) before
//!   `NewGameInitData` ever runs. Per this issue's own scope (naming UI
//!   deferred to M8), [`DEFAULT_PLAYER_GENDER`] and [`DEFAULT_PLAYER_NAME`]
//!   are fixed constants instead: [`DEFAULT_PLAYER_GENDER`] mirrors
//!   `crate::overworld::PlayerCharacter::Brendan`, the only playable sprite
//!   `load_default_room` already renders, and [`DEFAULT_PLAYER_NAME`] is
//!   `sMalePresetNames[0]` (`gText_DefaultNameStu`, `pokeemerald/src/main_menu.c:459-460`,
//!   `pokeemerald/src/strings.c:60`) -- upstream's own first suggested
//!   default name for a boy, rather than `Random() % NUM_PRESET_NAMES`'s
//!   runtime pick (`pokeemerald/src/main_menu.c:1603`), for a deterministic
//!   pre-1.0 default.
//! - **Trainer id's low half is the seed, not a second draw.** Upstream
//!   seeds `Random()` and sets the trainer id's low half from the very same
//!   raw value in one step -- `SeedRngAndSetTrainerId` reads `REG_TM1CNT_L`
//!   once into a local, calls `SeedRng` on it, and stashes it unmodified as
//!   `sTrainerId` (`pokeemerald/src/main.c:208-214`); `GetGeneratedTrainerIdLower`
//!   just returns that stashed value back (`main.c:216-219`), no RFU/link-cable
//!   state involved. `InitPlayerTrainerId` then shifts one `Random()` draw
//!   into the high half and keeps the untouched seed as the low half
//!   (`pokeemerald/src/new_game.c:84-88`). [`trainer_id_bytes`] mirrors
//!   that exactly: it takes `rng`'s own pre-draw state as the low half and
//!   calls `rng.next_u16()` once for the high half. The only real deviation
//!   is the seed's *source* -- see [`NEW_GAME_RNG_SEED`]'s docs for why this
//!   workspace uses a fixed constant instead of upstream's hardware Timer-1
//!   read.

use engine::overworld::{Direction, TilePos};
use engine::rng::Rng;
use engine::save::{Coords16, PlayerGender, SaveBlock1, SaveBlock2, WarpData};
use engine::text;

use crate::overworld::PlayerCharacter;

/// `sMalePresetNames[0]` == `gText_DefaultNameStu` (module docs) -- this
/// slice's fixed default player name, standing in for the naming screen (not
/// modelled yet -- deferred, still in v1 scope).
/// Fits [`engine::save::block::PLAYER_NAME_LENGTH`] (7 glyphs) with room to
/// spare.
pub const DEFAULT_PLAYER_NAME: &str = "STU";

/// This slice's fixed default player gender (module docs): male, matching
/// [`PlayerCharacter::Brendan`], the only overworld sprite
/// [`crate::overworld::load_default_room`] renders.
pub const DEFAULT_PLAYER_GENDER: PlayerGender = PlayerGender::Male;

/// [`PlayerCharacter`] this slice's fixed default resolves to, kept as one
/// named constant so [`main_menu`](crate::main_menu) and
/// [`intro`](crate::intro) never have to restate the Brendan/male pairing.
pub const DEFAULT_PLAYER_CHARACTER: PlayerCharacter = PlayerCharacter::Brendan;

/// `SetMoney(&gSaveBlock1Ptr->money, 3000)` (`new_game.c:172`): the fresh
/// starting money every new save begins with.
pub const STARTING_MONEY: u32 = 3000;

/// `SPECIES_TREECKO` — `sStarterMon[0]`
/// (`pokeemerald/src/starter_choose.c:113-118`), the first of Birch's three
/// bag options. The fixed pick standing in for the deferred starter-choose
/// UI, on the same first-listed-default reasoning as [`DEFAULT_PLAYER_NAME`].
pub const PROVISIONAL_STARTER_SPECIES: assets::SpeciesId = assets::SpeciesId(277);

/// The level every starter is handed over at: `CB2_GiveStarter`'s
/// `ScriptGiveMon(starterMon, 5, ITEM_NONE, 0, 0, 0)`
/// (`pokeemerald/src/battle_setup.c:917-929`).
pub const PROVISIONAL_STARTER_LEVEL: u8 = 5;

/// The battle-ready party lead a fresh game starts with — the stand-in for
/// the un-ported starter handout (issue #207 review, finding 1).
///
/// Upstream's party stays empty until the Route 101 rescue chain reaches
/// `CB2_GiveStarter` (`pokeemerald/src/battle_setup.c:917-929`), which runs
/// the starter-choose UI and `ScriptGiveMon`. This port has no script engine
/// to run any of that, so without a lead assigned at new-game time every
/// I-4 encounter would be rolled and then dropped — the acceptance path
/// would be reachable only from tests. Until the Birch-bag slice replaces
/// it, a fresh game therefore starts with the species/level upstream's
/// handout produces ([`PROVISIONAL_STARTER_SPECIES`] at
/// [`PROVISIONAL_STARTER_LEVEL`]), knowing its real
/// `GiveBoxMonInitialMoveset` level-up moves.
///
/// **Deliberately deterministic, drawing nothing.** Upstream's
/// `ScriptGiveMon` → `CreateMon` rolls personality and IVs off the global
/// stream, but those draws belong to the script sequence that is not
/// modelled yet; taking them here would shift every seed-pinned encounter
/// test and ledger-verified draw sequence for no behavioural gain. Fixed
/// personality `0` (nature `0 % 25` = Hardy, the neutral one) and all-zero
/// IVs instead — the future Birch-bag slice inherits the job of rolling
/// these for real, and should delete this function when it does.
///
/// The lead lives on the overworld phase, not in [`SaveBlock1`]:
/// `player_party_count` stays `0` exactly as [`init_save_blocks`] leaves it,
/// because this workspace's save model has no typed party-mon encoding yet
/// and no save writer consumes one (upstream's `gPlayerParty` is likewise
/// EWRAM state apart from the save block, synced only at save/load).
///
/// # Panics
///
/// Never in practice: [`PROVISIONAL_STARTER_SPECIES`] and its level-5
/// learnset are in the generated dex tables, pinned by this module's
/// `provisional_starter_is_a_fightable_lead` test.
#[must_use]
pub fn provisional_starter() -> battle::BattlePokemon {
    let moves = battle::initial_moveset(PROVISIONAL_STARTER_SPECIES, PROVISIONAL_STARTER_LEVEL);
    battle::BattlePokemon::new(
        &battle::Dex::new(),
        PROVISIONAL_STARTER_SPECIES,
        PROVISIONAL_STARTER_LEVEL,
        battle::Ivs::default(),
        0,
        moves,
    )
    .expect("the provisional starter's species and level-up moves are in the dex")
}

/// Where the intro hands control to the overworld loop (module docs, "No
/// truck sequence"): `LittlerootTown_BrendansHouse_1F`'s stairs warp lands
/// on `LittlerootTown_BrendansHouse_2F`'s own warp event index 0
/// (`pokeemerald/data/maps/LittlerootTown_BrendansHouse_1F/map.json`'s
/// `{"x": 8, "y": 2, "dest_map": "MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F",
/// "dest_warp_id": "0"}`), which is itself
/// `LittlerootTown_BrendansHouse_2F/map.json`'s sole `warp_events` entry,
/// `{"x": 7, "y": 1, ...}` -- the stairwell tile at the top of the stairs.
/// Landing exactly on a warp event's own tile (no offset) matches this
/// engine's already-implemented warp semantics
/// (`engine::overworld::warp::warp_destination_position`, S-5).
pub const SPAWN_POSITION: TilePos = (7, 1);

/// The spawn tile's elevation: `0`, matching every non-decoration object
/// event upstream places on that same row of the 2F layout (e.g.
/// `LOCALID_RIVALS_HOUSE_2F_RIVAL` at `(7, 1)`, `elevation: 0` --
/// `LittlerootTown_BrendansHouse_2F/map.json`).
pub const SPAWN_ELEVATION: u8 = 0;

/// The facing direction the player starts with. No upstream equivalent
/// exists for a *direct* spawn into this room (real Emerald always walks
/// the player in from the stairs, never places them pre-warped) -- this
/// port's own choice for "just arrived," facing further into the room.
pub const SPAWN_FACING: Direction = Direction::South;

/// `MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F` -- the upstream map id backing
/// [`SPAWN_POSITION`], for [`SaveBlock1::location`] and for looking up the
/// room's [`engine::overworld::MapRuntime`] once control reaches the
/// overworld loop.
pub const SPAWN_MAP_ID: assets::MapId = assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F");

/// [`SPAWN_MAP_ID`]'s `MAP_GROUP` index (`crates/assets/src/map_headers.rs`'s
/// generated `HEADERS` table, cross-checked against upstream
/// `map_groups.json`'s `group_order`): index `1`,
/// `gMapGroup_IndoorLittleroot` -- the group every indoor Littleroot Town map
/// (both houses, the lab) belongs to.
///
/// Was previously left at `0`/`0` (module docs' pre-fix history): group `0`
/// is `gMapGroup_TownsAndRoutes`, whose own position `0` is
/// `MAP_PETALBURG_CITY` -- a save's [`SaveBlock1::location`] pointing there
/// instead of the bedroom would resolve through `MapHeaderTable` to the
/// wrong map entirely. [`spawn_location_matches_the_bedrooms_map_header_position`]
/// cross-checks this constant against the generated table directly, so a
/// future `assets` regeneration can't silently desync it here.
pub const SPAWN_MAP_GROUP: i8 = 1;

/// [`SPAWN_MAP_ID`]'s `MAP_NUM` index within [`SPAWN_MAP_GROUP`] (module
/// docs): position `1` in `gMapGroup_IndoorLittleroot`
/// (`LittlerootTown_BrendansHouse_1F` is position `0`,
/// `LittlerootTown_BrendansHouse_2F` -- this room -- is position `1`).
pub const SPAWN_MAP_NUM: i8 = 1;

/// `HEAL_LOCATION_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F`/`_MAYS_HOUSE_2F`'s
/// shared `MAP_GROUP` (issue #261): both bedrooms are
/// `gMapGroup_IndoorLittleroot`, the same group [`SPAWN_MAP_GROUP`] names.
///
/// # Why a new-game save needs a heal location at all
///
/// Upstream never sets `gSaveBlock1Ptr->lastHealLocation` from
/// `NewGameInitData` directly -- `ClearSav1` (`src/load_save.c:64-67`)
/// zeroes it along with the rest of `SaveBlock1`, same as this crate's own
/// [`SaveBlock1::default`]. The real default comes from the truck sequence
/// this port skips (module docs' "No truck sequence"): stepping off the
/// truck runs `InsideOfTruck_EventScript_SetIntroFlagsMale`/`_Female`
/// (`pokeemerald/data/maps/InsideOfTruck/scripts.inc:24-25`, `:37-38`),
/// whose first line is `setrespawn HEAL_LOCATION_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F`/
/// `_MAYS_HOUSE_2F` -- `ScrCmd_setrespawn` (`src/scrcmd.c:2006-2012`) calling
/// `SetLastHealLocationWarp` (`src/overworld.c:670-675`), which resolves the
/// id through `src/data/heal_locations.json`
/// (`{"id": "HEAL_LOCATION_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F", "map":
/// "MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F", "x": 4, "y": 2}`, and the
/// `_MAYS_HOUSE_2F` entry at the same `x`/`y`) and writes it with
/// `WARP_ID_NONE` (`overworld.c:674`).
///
/// Every real playthrough passes through that exit-tile trigger before
/// `NewGameInitData`'s caller ever hands control to the overworld, so a
/// fresh save's `lastHealLocation` is never really the zeroed default --
/// [`apply_truck_intro_flags`] already reproduces this same script's other
/// observable effect (module docs) for the identical reason: the skipped
/// cutscene's *effects* still have to land somewhere. Without this, the
/// first loss of a fresh game (issue #261's white-out) would warp the player
/// to `MAP_PETALBURG_CITY` at `(0, 0)` -- [`SPAWN_MAP_GROUP`]'s own doc
/// comment records that exact wrong-map failure mode for `location`, and
/// `last_heal_location` starts at the identical zeroed [`WarpData::default`]
/// if nothing sets it here.
pub const DEFAULT_HEAL_LOCATION_MAP_GROUP: i8 = SPAWN_MAP_GROUP;

/// `HEAL_LOCATION_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F`'s `MAP_NUM` within
/// [`DEFAULT_HEAL_LOCATION_MAP_GROUP`] -- position `1`, the same index
/// [`SPAWN_MAP_NUM`] names (`InsideOfTruck_EventScript_SetIntroFlagsMale`'s
/// `setrespawn`, [`DEFAULT_HEAL_LOCATION_MAP_GROUP`]'s doc comment): a male
/// player's white-out destination is the same room the intro spawns into,
/// just not the same tile within it (module docs on
/// [`DEFAULT_HEAL_LOCATION_X`]/[`DEFAULT_HEAL_LOCATION_Y`]).
pub const DEFAULT_HEAL_LOCATION_MALE_MAP_NUM: i8 = SPAWN_MAP_NUM;

/// `HEAL_LOCATION_LITTLEROOT_TOWN_MAYS_HOUSE_2F`'s `MAP_NUM` within
/// [`DEFAULT_HEAL_LOCATION_MAP_GROUP`] -- position `3`
/// (`LittlerootTown_MaysHouse_1F` is position `2`, cross-checked against the
/// generated table by
/// [`tests::default_heal_locations_match_the_generated_map_header_positions`]),
/// written for a female player by
/// `InsideOfTruck_EventScript_SetIntroFlagsFemale`'s own `setrespawn`
/// ([`DEFAULT_HEAL_LOCATION_MAP_GROUP`]'s doc comment).
pub const DEFAULT_HEAL_LOCATION_FEMALE_MAP_NUM: i8 = 3;

/// `HEAL_LOCATION_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F`/`_MAYS_HOUSE_2F`'s
/// shared `x` (`src/data/heal_locations.json`,
/// [`DEFAULT_HEAL_LOCATION_MAP_GROUP`]'s doc comment) -- the bed tile, not
/// [`SPAWN_POSITION`]'s stairwell landing tile: a heal location and a warp
/// event are different upstream tables, and nothing requires them to agree.
pub const DEFAULT_HEAL_LOCATION_X: i16 = 4;

/// [`DEFAULT_HEAL_LOCATION_X`]'s `y` counterpart.
pub const DEFAULT_HEAL_LOCATION_Y: i16 = 2;

/// [`init_save_blocks`]'s `last_heal_location` -- `SetLastHealLocationWarp`'s
/// effect for the gender-branched `setrespawn` the skipped truck sequence
/// would have run ([`DEFAULT_HEAL_LOCATION_MAP_GROUP`]'s doc comment).
///
/// [`engine::save::PlayerGender::Other`] reproduces upstream's own true
/// no-op rather than picking a house: `checkplayergender`'s two
/// `goto_if_eq`s (`MALE`/`FEMALE`) both fail for an out-of-range gender byte
/// and fall through to a bare `end`
/// (`pokeemerald/data/maps/InsideOfTruck/scripts.inc:19-22`), so
/// `ScrCmd_setrespawn` never runs and `lastHealLocation` stays at
/// `ClearSav1`'s zeroed value -- exactly [`WarpData::default`], the value
/// [`SaveBlock1::default`] already leaves this field at. The same no-op
/// [`apply_truck_intro_flags`]'s own gender match documents for the
/// identical upstream branch, and equally unreachable in production today
/// ([`DEFAULT_PLAYER_GENDER`] is always `Male`).
#[must_use]
pub fn default_last_heal_location(gender: PlayerGender) -> WarpData {
    let map_num = match gender {
        PlayerGender::Male => DEFAULT_HEAL_LOCATION_MALE_MAP_NUM,
        PlayerGender::Female => DEFAULT_HEAL_LOCATION_FEMALE_MAP_NUM,
        PlayerGender::Other(_) => return WarpData::default(),
    };
    WarpData {
        map_group: DEFAULT_HEAL_LOCATION_MAP_GROUP,
        map_num,
        // WARP_ID_NONE (`overworld.c:674`): a heal location names an
        // explicit tile, not a resolved warp event.
        warp_id: -1,
        x: DEFAULT_HEAL_LOCATION_X,
        y: DEFAULT_HEAL_LOCATION_Y,
    }
}

/// Build a fresh [`SaveBlock1`]/[`SaveBlock2`] pair the way upstream
/// `NewGameInitData` (`new_game.c:149-207`) does, for the fields this
/// workspace's save model covers (module docs' table and deviations).
///
/// `rng` supplies the trainer id's two halves (module docs, "Trainer id's
/// low half is the seed, not a second draw"): its own pre-draw state is the
/// low half, and one draw off it is the high half. Callers that don't care
/// about a specific sequence can pass a freshly seeded [`Rng`]; passing one
/// that has already drawn still works, but then the "low half" is that
/// already-advanced state rather than a real seed -- harmless here since
/// every caller in this workspace passes a freshly constructed [`Rng`].
///
/// # Panics
///
/// Never panics in practice: every id in [`assets::RESET_MAP_FLAGS`] is
/// pinned (by `assets::new_game_flags`'s own tests) to be an ordinary flag
/// id, which [`EventData::flag_set`](engine::event_data::EventData::flag_set)
/// never rejects. The `expect` below exists only because that invariant
/// lives in a different crate's test suite, not this function's own type
/// signature.
#[must_use]
pub fn init_save_blocks(rng: &mut Rng) -> (SaveBlock1, SaveBlock2) {
    // `rng`'s current pre-draw state here, before this function has drawn
    // anything from it, may already be advanced from its construction seed;
    // upstream's corresponding state is the raw Timer-1 read
    // (`SeedRngAndSetTrainerId`, `pokeemerald/src/main.c:208-214`).
    // Captured now, alongside the generator, so `trainer_id_bytes` can reuse
    // it verbatim as the id's low half the same way upstream's `sTrainerId`
    // does.
    let new_game_seed = rng.state();
    let block2 = SaveBlock2 {
        player_name: encode_player_name(DEFAULT_PLAYER_NAME),
        player_gender: DEFAULT_PLAYER_GENDER,
        player_trainer_id: trainer_id_bytes(new_game_seed, rng),
        encryption_key: 0,
    };

    let spawn = WarpData {
        map_group: SPAWN_MAP_GROUP,
        map_num: SPAWN_MAP_NUM,
        warp_id: -1, // WARP_ID_NONE: not arriving via a resolved warp event.
        x: i16::try_from(SPAWN_POSITION.0).unwrap_or(0),
        y: i16::try_from(SPAWN_POSITION.1).unwrap_or(0),
    };
    let mut block1 = SaveBlock1 {
        pos: Coords16 {
            x: spawn.x,
            y: spawn.y,
        },
        location: spawn,
        money: STARTING_MONEY,
        // `SetLastHealLocationWarp` via the skipped truck sequence's own
        // `setrespawn` (`default_last_heal_location`'s doc comment) -- issue
        // #261's white-out needs a real destination from the first loss of
        // a fresh game onward, not the zeroed default.
        last_heal_location: default_last_heal_location(block2.player_gender),
        ..SaveBlock1::default()
    };

    // RunScriptImmediately(EventScript_ResetAllMapFlags) (module docs'
    // table): every id in `assets::RESET_MAP_FLAGS` is an ordinary flag id
    // (pinned by `assets::new_game_flags`'s own
    // `reset_map_flags_are_all_ordinary_ids` test), so `flag_set` cannot
    // fail here; `expect` documents that invariant rather than threading an
    // unreachable `Result` through this function's signature.
    for &flag in assets::RESET_MAP_FLAGS {
        block1
            .event_data
            .flag_set(flag)
            .expect("RESET_MAP_FLAGS ids are all ordinary flag ids");
    }

    apply_truck_intro_flags(&mut block1, block2.player_gender);

    (block1, block2)
}

/// Apply the gender-selected half of the skipped truck sequence:
/// `InsideOfTruck_EventScript_SetIntroFlags`
/// (`data/maps/InsideOfTruck/scripts.inc:16-48`), branching on
/// `checkplayergender` exactly as upstream does at `:19-21`.
///
/// # Why this belongs at new-game init
///
/// Upstream does not run it from `NewGameInitData` — it is a coord (trigger)
/// event on the truck's three exit tiles, gated on
/// `VAR_LITTLEROOT_INTRO_STATE == 0`
/// (`data/maps/InsideOfTruck/map.json:85-112`), so it fires once on the way
/// out of the truck and then sets that var so it cannot fire again. Every
/// real playthrough passes through it before reaching a house.
///
/// This port skips the truck and hands off straight into the bedroom
/// (module docs' "Deliberately not mirrored" section on the intro), so the
/// *only* honest place to reproduce the effect is here, at the point the
/// skipped flow would have produced it — the same reasoning that puts
/// `EventScript_ResetAllMapFlags` in this function rather than in a script
/// engine.
///
/// Without it a fresh save reaches the player's house with all ten of these
/// flags clear, which upstream never does, and the consequences are visible
/// rather than cosmetic — see [`assets::TRUCK_INTRO_FLAGS_MALE`]'s own docs
/// for the invisible-collider and duplicated-mother cases this prevents.
/// The two branches are exact mirror images (Brendan/May swapped), because
/// each Littleroot house declares *both* families' object events and hides
/// whichever set does not belong.
///
/// # Panics
///
/// Never in practice: every id in
/// [`assets::TRUCK_INTRO_FLAGS_MALE`]/[`assets::TRUCK_INTRO_FLAGS_FEMALE`]
/// is an ordinary `FLAG_HIDE_*` id and every
/// [`assets::TRUCK_INTRO_VARS_MALE`]/`_FEMALE` id an ordinary `VAR_*` id,
/// both pinned by `assets::new_game_flags`' own tests and by this module's
/// `truck_intro_ids_are_all_settable` — the same
/// invariant-lives-in-another-crate's-tests reasoning as the
/// `RESET_MAP_FLAGS` loop above.
fn apply_truck_intro_flags(block1: &mut SaveBlock1, gender: PlayerGender) {
    let (flags, vars) = match gender {
        PlayerGender::Male => (
            assets::TRUCK_INTRO_FLAGS_MALE,
            assets::TRUCK_INTRO_VARS_MALE,
        ),
        PlayerGender::Female => (
            assets::TRUCK_INTRO_FLAGS_FEMALE,
            assets::TRUCK_INTRO_VARS_FEMALE,
        ),
        // Upstream's script has no `else`: `checkplayergender` copies the
        // raw `gSaveBlock2Ptr->playerGender` byte into `VAR_RESULT`
        // (`src/scrcmd.c:2014-2018`) and the two `goto_if_eq`s fall through
        // to a bare `end` (`data/maps/InsideOfTruck/scripts.inc:19-22`), so
        // a byte that is neither `MALE` nor `FEMALE` sets nothing at all.
        // [`PlayerGender::Other`] exists only because the save model
        // decodes such a byte losslessly; matching upstream's no-op here
        // keeps that a save-fidelity concern rather than inventing a third
        // house layout. Unreachable from this function anyway --
        // [`DEFAULT_PLAYER_GENDER`] is `Male` and nothing else writes it yet.
        PlayerGender::Other(_) => return,
    };
    for &flag in flags {
        block1
            .event_data
            .flag_set(flag)
            .expect("truck-intro ids are all ordinary flag ids");
    }
    for &(var, value) in vars {
        block1
            .event_data
            .var_set(var, value)
            .expect("truck-intro var ids are all ordinary var ids");
    }
}

/// Seed [`init_save_blocks`]'s [`Rng`] draws its trainer id from when the
/// caller has no seed of its own to supply (module docs, "Trainer id's low
/// half is the seed, not a second draw"): this workspace has no
/// wall-clock/hardware entropy source wired into any runtime path yet. The
/// overworld runtime creates its owned generator from this seed, passes it
/// through [`init_save_blocks`], and retains the resulting advanced stream.
/// Reaching for `std::time` here just to look more "random" would make that
/// stream non-deterministic for no modeled behavioural gain -- fixed, like
/// every other RNG seed this workspace's own tests use.
pub const NEW_GAME_RNG_SEED: u32 = 0;

/// [`init_save_blocks`], seeded with [`NEW_GAME_RNG_SEED`], for callers that
/// need only the initialized save blocks and do not own the continuing
/// runtime RNG stream. The overworld handoff instead seeds its owned [`Rng`]
/// directly and passes it to [`init_save_blocks`] so later gameplay keeps
/// the state after the trainer-id draw.
#[must_use]
pub fn init_save_blocks_for_new_game() -> (SaveBlock1, SaveBlock2) {
    let mut rng = Rng::new(NEW_GAME_RNG_SEED);
    init_save_blocks(&mut rng)
}

/// Encode `name` into the fixed-size, `0xFF`-terminated buffer
/// [`SaveBlock2::player_name`] expects (upstream `StringCopy` into a
/// `PLAYER_NAME_BUF_LEN` buffer). `name` must encode to no more than
/// [`engine::save::block::PLAYER_NAME_LENGTH`] Gen-3 glyphs -- true for
/// [`DEFAULT_PLAYER_NAME`], the only caller.
///
/// # Panics
///
/// Panics if `name` contains a character with no Gen-3 encoding, or encodes
/// longer than the buffer -- unreachable for [`DEFAULT_PLAYER_NAME`], which
/// [`default_player_name_fits_the_save_buffer`] pins.
fn encode_player_name(name: &str) -> [u8; engine::save::block::PLAYER_NAME_BUF_LEN] {
    let encoded = text::encode_str(name).expect("DEFAULT_PLAYER_NAME is Gen-3 encodable");
    let mut buf = [0xFFu8; engine::save::block::PLAYER_NAME_BUF_LEN];
    assert!(
        encoded.len() <= buf.len(),
        "DEFAULT_PLAYER_NAME must fit PLAYER_NAME_BUF_LEN"
    );
    buf[..encoded.len()].copy_from_slice(&encoded);
    buf
}

/// Upstream's `SeedRngAndSetTrainerId` + `InitPlayerTrainerId` shape (module
/// docs, "Trainer id's low half is the seed, not a second draw"): `seed` --
/// `rng`'s own pre-draw state -- becomes the id's low half verbatim, and one
/// `rng.next_u16()` draw becomes the high half. Only `seed`'s low 16 bits
/// matter, matching the `u16` upstream's raw Timer-1 read actually is
/// (`Rng::new`'s docs); every seed this workspace uses is already in that
/// range.
fn trainer_id_bytes(seed: u32, rng: &mut Rng) -> [u8; engine::save::block::TRAINER_ID_LENGTH] {
    let hi = rng.next_u16();
    let trainer_id = (u32::from(hi) << 16) | (seed & 0xFFFF);
    trainer_id.to_le_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_player_name_fits_the_save_buffer() {
        let encoded = text::encode_str(DEFAULT_PLAYER_NAME).unwrap();
        // `encode_str` appends the EOS terminator, so this is glyph count + 1.
        assert!(encoded.len() <= engine::save::block::PLAYER_NAME_BUF_LEN);
        assert_eq!(DEFAULT_PLAYER_NAME, "STU");
    }

    /// Issue #207 review, finding 1: a fresh game must start with a lead
    /// that can actually fight the I-4 encounter. Pins the species/level to
    /// upstream's handout (`CB2_GiveStarter`, `battle_setup.c:917-929`) and
    /// the construction to be deterministic and draw-free — what makes
    /// [`provisional_starter`]'s `expect` unreachable rather than latent.
    #[test]
    fn provisional_starter_is_a_fightable_lead() {
        let starter = provisional_starter();
        assert_eq!(starter.species(), PROVISIONAL_STARTER_SPECIES);
        assert_eq!(starter.species(), assets::SpeciesId(277), "SPECIES_TREECKO");
        assert_eq!(starter.level(), PROVISIONAL_STARTER_LEVEL);
        assert!(!starter.is_fainted());
        assert_eq!(starter.current_hp(), starter.stats().max_hp);
        assert!(
            !starter.moves().is_empty(),
            "GiveBoxMonInitialMoveset gives a level-5 Treecko at least one move"
        );
        assert_eq!(starter.stages(), battle::StatStages::default());
        // Two calls agree exactly: nothing about the construction reads a
        // stream, so assigning it at new-game init can never shift the
        // seed-pinned draw sequences the encounter tests rely on.
        assert_eq!(starter, provisional_starter());
    }

    #[test]
    fn init_save_blocks_matches_new_game_init_data_for_modeled_fields() {
        let mut rng = Rng::new(0);
        let (block1, block2) = init_save_blocks(&mut rng);

        // gSaveBlock2Ptr->encryptionKey = 0;
        assert_eq!(block2.encryption_key, 0);
        // Fixed default identity (naming screen deferred).
        assert_eq!(block2.player_gender, PlayerGender::Male);
        assert_eq!(text::decode_to_string(&block2.player_name).unwrap(), "STU");

        // SetMoney(&gSaveBlock1Ptr->money, 3000);
        assert_eq!(block1.money, STARTING_MONEY);
        // ZeroPlayerPartyMons / gPlayerPartyCount = 0.
        assert_eq!(block1.player_party_count, 0);
        assert_eq!(block1.player_party, SaveBlock1::default().player_party);
        // ClearBag().
        assert_eq!(block1.bag, engine::save::Bag::default());

        // Spawn location: the bedroom, at SPAWN_POSITION -- not the truck.
        assert_eq!(block1.pos.x, 7);
        assert_eq!(block1.pos.y, 1);
        assert_eq!(block1.location.warp_id, -1);
        // Regression: this used to be left at (0, 0), which
        // `MapHeaderTable` resolves to Petalburg City, not the bedroom --
        // see `spawn_location_matches_the_bedrooms_map_header_position`.
        assert_eq!(block1.location.map_group, SPAWN_MAP_GROUP);
        assert_eq!(block1.location.map_num, SPAWN_MAP_NUM);
    }

    /// Issue #313: pins [`trainer_id_bytes`]'s corrected shape against
    /// upstream's own values for the configured seed, computed independently
    /// of this module's code from `SeedRngAndSetTrainerId`
    /// (`pokeemerald/src/main.c:208-214`) and `InitPlayerTrainerId`
    /// (`pokeemerald/src/new_game.c:84-88`): with seed `0`, the low half is
    /// the seed itself (`0`) and the high half is `Random()`'s first draw
    /// (also `0` for this seed, `engine::rng::Rng`'s own module docs'
    /// worked example) -- so the id is all-zero bytes, and the generator has
    /// advanced by exactly the one draw `ISO_RANDOMIZE1(0) == 24_691 ==
    /// 0x0000_6073` leaves behind, not two.
    #[test]
    fn trainer_id_bytes_matches_upstream_for_the_configured_seed() {
        let mut rng = Rng::new(NEW_GAME_RNG_SEED);
        let (_, block2) = init_save_blocks(&mut rng);
        assert_eq!(block2.player_trainer_id, [0x00, 0x00, 0x00, 0x00]);
        assert_eq!(rng.state(), 0x0000_6073);
    }

    /// Seed `0` is degenerate -- both halves come out `0`, so a hi/lo
    /// transposition would still pass the test above. This non-degenerate
    /// seed pins the halves' order and the little-endian byte layout:
    /// `Rng::new(0x1234)`'s first draw is `0x4DCB`
    /// (`engine::rng`'s own `next_u16_matches_known_sequence_from_u16_seed`),
    /// so the id is `0x4DCB_1234` stored as `[0x34, 0x12, 0xCB, 0x4D]`.
    #[test]
    fn trainer_id_halves_are_not_transposed_for_a_nondegenerate_seed() {
        let mut rng = Rng::new(0x1234);
        let (_, block2) = init_save_blocks(&mut rng);
        assert_eq!(block2.player_trainer_id, [0x34, 0x12, 0xCB, 0x4D]);
    }

    /// Regression for the finding that a fresh save's
    /// [`SaveBlock1::location`] stored `(map_group, map_num) == (0, 0)`,
    /// which `assets::MapHeaderTable` resolves to `MAP_PETALBURG_CITY`, not
    /// [`SPAWN_MAP_ID`]'s own bedroom. Cross-checks
    /// [`SPAWN_MAP_GROUP`]/[`SPAWN_MAP_NUM`] against the *generated*
    /// `assets::MapHeaderTable` entry for [`SPAWN_MAP_ID`] directly (rather
    /// than only pinning the literal `1`/`1`), so a future `assets`
    /// regeneration that ever renumbers Littleroot's indoor group can't
    /// silently desync this module's own copy of the position.
    #[test]
    fn spawn_location_matches_the_bedrooms_map_header_position() {
        let header = assets::MapHeaderTable::new()
            .header(SPAWN_MAP_ID)
            .expect("SPAWN_MAP_ID must resolve in the generated map-header table");
        assert_eq!(i8::try_from(header.group).unwrap(), SPAWN_MAP_GROUP);
        assert_eq!(i8::try_from(header.num).unwrap(), SPAWN_MAP_NUM);

        let mut rng = Rng::new(0);
        let (block1, _) = init_save_blocks(&mut rng);
        assert_eq!(block1.location.map_group, SPAWN_MAP_GROUP);
        assert_eq!(block1.location.map_num, SPAWN_MAP_NUM);

        // And the group/num pair must *not* still be Petalburg City's own
        // position (group 0, num 0) -- the exact wrong-map bug this
        // regression guards against.
        assert_ne!((block1.location.map_group, block1.location.map_num), (0, 0));
    }

    #[test]
    fn init_save_blocks_for_new_game_uses_the_fixed_seed() {
        // Matches calling `init_save_blocks` directly with
        // `Rng::new(NEW_GAME_RNG_SEED)` -- proof this fixed-seed convenience
        // helper for save-only callers doesn't silently draw from a different
        // sequence.
        let mut rng = Rng::new(NEW_GAME_RNG_SEED);
        let expected = init_save_blocks(&mut rng);
        let actual = init_save_blocks_for_new_game();
        assert_eq!(actual.0.money, expected.0.money);
        assert_eq!(actual.1.player_trainer_id, expected.1.player_trainer_id);
    }

    #[test]
    fn trainer_id_is_deterministic_from_seed_and_one_rng_draw() {
        let mut rng_a = Rng::new(42);
        let mut rng_b = Rng::new(42);
        let (_, block2_a) = init_save_blocks(&mut rng_a);
        let (_, block2_b) = init_save_blocks(&mut rng_b);
        // Same seed -> same trainer id, every time (no wall-clock/global RNG).
        assert_eq!(block2_a.player_trainer_id, block2_b.player_trainer_id);

        let mut rng_c = Rng::new(43);
        let (_, block2_c) = init_save_blocks(&mut rng_c);
        assert_ne!(block2_a.player_trainer_id, block2_c.player_trainer_id);
    }

    #[test]
    fn spawn_constants_match_the_bedroom_warp_chain() {
        assert_eq!(SPAWN_POSITION, (7, 1));
        assert_eq!(SPAWN_ELEVATION, 0);
        assert_eq!(SPAWN_FACING, Direction::South);
        assert_eq!(SPAWN_MAP_ID.0, "MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F");
    }

    /// [`DEFAULT_HEAL_LOCATION_MAP_GROUP`]/[`DEFAULT_HEAL_LOCATION_MALE_MAP_NUM`]/
    /// [`DEFAULT_HEAL_LOCATION_FEMALE_MAP_NUM`] must each resolve, through the
    /// *generated* `assets::MapHeaderTable`, to the real bedroom maps --
    /// [`spawn_location_matches_the_bedrooms_map_header_position`]'s own
    /// cross-check, applied to both houses instead of only Brendan's, so a
    /// future `assets` regeneration that renumbers Littleroot's indoor group
    /// can't silently desync either constant.
    #[test]
    fn default_heal_locations_match_the_generated_map_header_positions() {
        let brendans = assets::MapHeaderTable::new()
            .header(assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F"))
            .expect("must resolve in the generated map-header table");
        assert_eq!(
            i8::try_from(brendans.group).unwrap(),
            DEFAULT_HEAL_LOCATION_MAP_GROUP
        );
        assert_eq!(
            i8::try_from(brendans.num).unwrap(),
            DEFAULT_HEAL_LOCATION_MALE_MAP_NUM
        );

        let mays = assets::MapHeaderTable::new()
            .header(assets::MapId("MAP_LITTLEROOT_TOWN_MAYS_HOUSE_2F"))
            .expect("must resolve in the generated map-header table");
        assert_eq!(
            i8::try_from(mays.group).unwrap(),
            DEFAULT_HEAL_LOCATION_MAP_GROUP
        );
        assert_eq!(
            i8::try_from(mays.num).unwrap(),
            DEFAULT_HEAL_LOCATION_FEMALE_MAP_NUM
        );
    }

    /// `SetLastHealLocationWarp`'s gender pairing (module docs on
    /// [`default_last_heal_location`]): a male player's white-out
    /// destination is Brendan's house, a female player's is May's, and an
    /// out-of-range gender byte is upstream's own true no-op -- the zeroed
    /// [`WarpData::default`], not a guessed house.
    #[test]
    fn default_last_heal_location_matches_the_gender_pairing() {
        assert_eq!(
            default_last_heal_location(PlayerGender::Male),
            WarpData {
                map_group: DEFAULT_HEAL_LOCATION_MAP_GROUP,
                map_num: DEFAULT_HEAL_LOCATION_MALE_MAP_NUM,
                warp_id: -1,
                x: DEFAULT_HEAL_LOCATION_X,
                y: DEFAULT_HEAL_LOCATION_Y,
            }
        );
        assert_eq!(
            default_last_heal_location(PlayerGender::Female),
            WarpData {
                map_group: DEFAULT_HEAL_LOCATION_MAP_GROUP,
                map_num: DEFAULT_HEAL_LOCATION_FEMALE_MAP_NUM,
                warp_id: -1,
                x: DEFAULT_HEAL_LOCATION_X,
                y: DEFAULT_HEAL_LOCATION_Y,
            }
        );
        assert_eq!(
            default_last_heal_location(PlayerGender::Other(0xFF)),
            WarpData::default(),
            "upstream's own no-op: checkplayergender falls through to a bare `end`"
        );
    }

    /// [`init_save_blocks`] must actually apply
    /// [`default_last_heal_location`] to the fresh save, not merely leave
    /// the constant computable -- the regression
    /// [`spawn_location_matches_the_bedrooms_map_header_position`] guards
    /// for `location`, applied to `last_heal_location` (issue #261).
    #[test]
    fn fresh_save_has_a_real_last_heal_location() {
        let mut rng = Rng::new(0);
        let (block1, block2) = init_save_blocks(&mut rng);
        assert_eq!(block2.player_gender, PlayerGender::Male);
        assert_eq!(
            block1.last_heal_location,
            default_last_heal_location(PlayerGender::Male)
        );
        assert_ne!(
            (
                block1.last_heal_location.map_group,
                block1.last_heal_location.map_num
            ),
            (0, 0),
            "must not be Petalburg City's own position"
        );
    }

    /// Issue #164's `DoD`: a fresh save must have
    /// `FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM`
    /// (`new_game.inc:152`) set, so the rival object event this flag hides
    /// (`data/maps/LittlerootTown_BrendansHouse_2F/map.json`) is absent from
    /// its (7, 1) spawn tile the moment a new game starts -- matching
    /// `RunScriptImmediately(EventScript_ResetAllMapFlags)`'s effect, not
    /// `InitEventData`'s previous all-clear.
    #[test]
    fn fresh_save_hides_the_rival_bedroom_object_event() {
        let mut rng = Rng::new(0);
        let (block1, _) = init_save_blocks(&mut rng);
        assert!(block1
            .event_data
            .flag_get(assets::FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM)
            .unwrap());
        // Tie the numeric id to the real map data: the spawn map's object
        // events actually carry this hide flag (by its upstream name), so
        // #161's flag-filtered object-event rendering has a pre-wired case.
        let events = assets::MapEventsTable::new()
            .resolve(SPAWN_MAP_ID)
            .expect("spawn map has generated events");
        assert!(
            events
                .object_events
                .iter()
                .any(|obj| obj.flag == "FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM"),
            "the bedroom must have an object event hidden by the DoD-pinned flag"
        );
    }

    /// Every one of `EventScript_ResetAllMapFlags`'s 159 `setflag`s
    /// (`assets::RESET_MAP_FLAGS`, `data/scripts/new_game.inc:116-274`) must
    /// land on the fresh save's `EventData`, not just the one `DoD`-pinned
    /// flag above -- this is the "full effect", not a single spot check.
    #[test]
    fn fresh_save_applies_every_reset_map_flag() {
        let mut rng = Rng::new(0);
        let (block1, _) = init_save_blocks(&mut rng);
        for &flag in assets::RESET_MAP_FLAGS {
            assert!(
                block1.event_data.flag_get(flag).unwrap(),
                "flag {flag:#X} from RESET_MAP_FLAGS must be set on a fresh save"
            );
        }
        // And nothing more: the fresh save's exact bit population is the 159
        // script flags plus the five its gender branch of the skipped truck
        // sequence adds ([`apply_truck_intro_flags`]) -- catches a future
        // init step over-applying flags as well as this one under-applying
        // them. The sum is exact rather than an upper bound because the two
        // sets are disjoint (pinned by `assets`'
        // `no_truck_intro_flag_is_already_a_reset_map_flag`).
        let set_bits: usize = block1
            .event_data
            .flag_bytes()
            .iter()
            .map(|b| b.count_ones() as usize)
            .sum();
        assert_eq!(
            set_bits,
            assets::RESET_MAP_FLAGS.len() + assets::TRUCK_INTRO_FLAGS_MALE.len(),
            "a fresh save sets exactly the reset script's flags plus the \
             male truck-intro branch's"
        );
        assert_eq!(set_bits, 159 + 5, "the same counts, spelled literally");
        // Cross-crate range guard: `assets` can't depend on `engine`, so its
        // own range test pins a literal; this is the reciprocal assertion
        // against the real `FLAGS_COUNT`, where the dependency exists.
        assert!(assets::RESET_MAP_FLAGS
            .iter()
            .all(|&f| f < engine::event_data::FLAGS_COUNT));
    }

    /// Every truck-intro id must actually be settable through
    /// `EventData` -- what makes `apply_truck_intro_flags`' two `expect`s
    /// unreachable rather than latent panics. `assets` cannot depend on
    /// `engine`, so this reciprocal check lives here, where the dependency
    /// exists (same shape as the `FLAGS_COUNT` guard above).
    #[test]
    fn truck_intro_ids_are_all_settable() {
        let mut data = engine::event_data::EventData::new();
        for ids in [
            assets::TRUCK_INTRO_FLAGS_MALE,
            assets::TRUCK_INTRO_FLAGS_FEMALE,
        ] {
            for &flag in ids {
                assert!(data.flag_set(flag).is_ok(), "{flag:#X} must be settable");
            }
        }
        for vars in [
            assets::TRUCK_INTRO_VARS_MALE,
            assets::TRUCK_INTRO_VARS_FEMALE,
        ] {
            for &(var, value) in vars {
                assert!(
                    data.var_set(var, value).is_ok(),
                    "{var:#X} must be settable"
                );
            }
        }
    }

    /// `InsideOfTruck_EventScript_SetIntroFlags`, both branches
    /// (`data/maps/InsideOfTruck/scripts.inc:24-35` and `:37-48`): a fresh
    /// save must carry exactly its own gender's five flags and neither of
    /// the other gender's -- the mirror-image structure is the whole point,
    /// so applying the wrong branch (or both) has to fail here.
    #[test]
    fn a_fresh_save_applies_its_own_gender_truck_intro_branch_only() {
        for (gender, mine, theirs, vars) in [
            (
                PlayerGender::Male,
                assets::TRUCK_INTRO_FLAGS_MALE,
                assets::TRUCK_INTRO_FLAGS_FEMALE,
                assets::TRUCK_INTRO_VARS_MALE,
            ),
            (
                PlayerGender::Female,
                assets::TRUCK_INTRO_FLAGS_FEMALE,
                assets::TRUCK_INTRO_FLAGS_MALE,
                assets::TRUCK_INTRO_VARS_FEMALE,
            ),
        ] {
            let mut block1 = SaveBlock1::default();
            apply_truck_intro_flags(&mut block1, gender);

            for &flag in mine {
                assert!(
                    block1.event_data.flag_get(flag).unwrap(),
                    "{gender:?}: {flag:#X} must be set"
                );
            }
            for &flag in theirs {
                assert!(
                    !block1.event_data.flag_get(flag).unwrap(),
                    "{gender:?}: {flag:#X} belongs to the other gender's branch"
                );
            }
            for &(var, value) in vars {
                assert_eq!(
                    block1.event_data.var_get(var).unwrap(),
                    value,
                    "{gender:?}: {var:#X}"
                );
            }
        }
    }

    /// A gender byte outside `MALE`/`FEMALE` sets nothing, matching
    /// upstream's own missing `else`: `checkplayergender` copies the raw
    /// byte into `VAR_RESULT` (`src/scrcmd.c:2014-2018`) and both
    /// `goto_if_eq`s fall through to a bare `end`
    /// (`data/maps/InsideOfTruck/scripts.inc:19-22`).
    #[test]
    fn an_unrecognized_gender_byte_applies_no_truck_intro_branch() {
        let mut block1 = SaveBlock1::default();
        apply_truck_intro_flags(&mut block1, PlayerGender::Other(7));
        let set_bits: u32 = block1
            .event_data
            .flag_bytes()
            .iter()
            .map(|b| b.count_ones())
            .sum();
        assert_eq!(set_bits, 0, "upstream's script has no else branch");
    }

    /// The reported regression, over the real bundled map data and for
    /// **both** genders: a fresh save must leave the player's own house
    /// free of the *rival's* family and of the rival's Poké Ball, and must
    /// leave the rival's house free of the player's own mother.
    ///
    /// Each Littleroot house declares both families' object events and
    /// hides whichever set does not belong
    /// (`data/maps/LittlerootTown_{Brendans,Mays}House_{1F,2F}/map.json`),
    /// and the choice is made by
    /// `InsideOfTruck_EventScript_SetIntroFlags`' gender branch. Without
    /// it every one of these reads as spawned: the Poké Ball is an
    /// invisible collider (nothing in this port draws
    /// `OBJ_EVENT_GFX_ITEM_BALL`), the rival's mother is a second mother
    /// standing in the player's own living room, and the rival's sibling is
    /// another undrawn blocker.
    ///
    /// Pure generated-table plus flag-store data -- no pack needed.
    #[test]
    fn a_fresh_save_puts_each_familys_object_events_in_the_right_house() {
        /// `(own house 2F, own house 1F, rival's house 1F)`.
        struct Houses {
            own_2f: &'static str,
            own_1f: &'static str,
            rivals_1f: &'static str,
        }
        let brendans = Houses {
            own_2f: "MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F",
            own_1f: "MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F",
            rivals_1f: "MAP_LITTLEROOT_TOWN_MAYS_HOUSE_1F",
        };
        let mays = Houses {
            own_2f: "MAP_LITTLEROOT_TOWN_MAYS_HOUSE_2F",
            own_1f: "MAP_LITTLEROOT_TOWN_MAYS_HOUSE_1F",
            rivals_1f: "MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F",
        };

        let table = assets::MapEventsTable::new();
        let find = |map: &'static str, gfx: &str| -> &'static assets::ObjectEvent {
            table
                .resolve(assets::MapId(map))
                .unwrap_or_else(|_| panic!("{map} must resolve"))
                .object_events
                .iter()
                .find(|o| o.graphics_id == gfx)
                .unwrap_or_else(|| panic!("{map} must declare a {gfx}"))
        };

        for (gender, houses) in [
            (PlayerGender::Male, &brendans),
            (PlayerGender::Female, &mays),
        ] {
            let mut block1 = SaveBlock1::default();
            for &flag in assets::RESET_MAP_FLAGS {
                block1.event_data.flag_set(flag).unwrap();
            }
            apply_truck_intro_flags(&mut block1, gender);
            let data = &block1.event_data;
            let visible = |o| engine::overworld::object_event_is_visible(o, data);

            // The rival's Poké Ball is not staged in the player's own
            // bedroom -- the invisible collider this fixes.
            assert!(
                !visible(find(houses.own_2f, "OBJ_EVENT_GFX_ITEM_BALL")),
                "{gender:?}: the rival's Poké Ball must not be spawned in \
                 the player's own bedroom ({})",
                houses.own_2f
            );

            // The player's own mother *is* home...
            assert!(
                visible(find(houses.own_1f, "OBJ_EVENT_GFX_MOM")),
                "{gender:?}: the player's own mother must be home",
            );
            // ...and the rival's mother and sibling are not.
            assert!(
                !visible(find(houses.own_1f, "OBJ_EVENT_GFX_WOMAN_4")),
                "{gender:?}: the rival's mother must not be duplicated into \
                 the player's own house ({})",
                houses.own_1f
            );
            assert!(
                !visible(find(houses.own_1f, "OBJ_EVENT_GFX_NINJA_BOY")),
                "{gender:?}: the rival's sibling must not be in the \
                 player's own house ({})",
                houses.own_1f
            );

            // The mirror image next door: the rival's own family is home in
            // the rival's house, and the player's mother is not there.
            assert!(
                visible(find(houses.rivals_1f, "OBJ_EVENT_GFX_WOMAN_4")),
                "{gender:?}: the rival's mother belongs in the rival's house ({})",
                houses.rivals_1f
            );
            assert!(
                !visible(find(houses.rivals_1f, "OBJ_EVENT_GFX_MOM")),
                "{gender:?}: the player's own mother must not also be in the \
                 rival's house ({})",
                houses.rivals_1f
            );
        }
    }

    /// The vars this now writes must not move Mom off the branch
    /// `crate::overworld::npc_scripts` transcribes:
    /// `PlayersHouse_1F_EventScript_Mom`
    /// (`data/scripts/players_house.inc:299-310`) branches away only on
    /// `VAR_LITTLEROOT_HOUSES_STATE_{MAY,BRENDAN} == 4` or
    /// `VAR_LITTLEROOT_INTRO_STATE == 7`, and the truck sets `1`/`1`
    /// (male) or `2`/`1` (female). Pinned, because setting a var that
    /// *did* cross one of those guards would silently invalidate the
    /// transcribed dialog rather than fail anything.
    #[test]
    fn the_truck_intro_vars_stay_below_moms_script_guards() {
        const INTRO_STATE: u16 = 0x4092;
        const HOUSES_STATE_MAY: u16 = 0x4082;
        const HOUSES_STATE_BRENDAN: u16 = 0x408C;
        for vars in [
            assets::TRUCK_INTRO_VARS_MALE,
            assets::TRUCK_INTRO_VARS_FEMALE,
        ] {
            for &(var, value) in vars {
                match var {
                    INTRO_STATE => assert_ne!(value, 7, "would take DidYouMeetProfBirch"),
                    HOUSES_STATE_MAY | HOUSES_STATE_BRENDAN => {
                        assert_ne!(value, 4, "would take DontPushYourselfTooHard");
                    }
                    other => panic!("unexpected truck-intro var {other:#X}"),
                }
            }
        }
    }
}
