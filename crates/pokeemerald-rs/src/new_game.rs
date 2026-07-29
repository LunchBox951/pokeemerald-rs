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
//! | `InitPlayerTrainerId()` (`Random() << 16 \| GetGeneratedTrainerIdLower()`) | [`SaveBlock2::player_trainer_id`], via `rng` (deviation below) |
//! | `ZeroPlayerPartyMons()` / `gPlayerPartyCount = 0`           | [`SaveBlock1::player_party`]/`player_party_count` `= 0` ([`SaveBlock1::default`]) |
//! | `SetMoney(&gSaveBlock1Ptr->money, 3000)`                    | [`SaveBlock1::money`] `= `[`STARTING_MONEY`] |
//! | `ClearBag()`                                                | [`SaveBlock1::bag`] `= Bag::default()` |
//! | `InitEventData()`                                           | [`SaveBlock1::event_data`] `= EventData::default()` |
//! | (naming screen / `Task_NewGameBirchSpeech_ChooseGender`)    | [`SaveBlock2::player_name`]/`player_gender` `= `[`DEFAULT_PLAYER_NAME`]`/`[`DEFAULT_PLAYER_GENDER`] (deviation below) |
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
//!   already treats as the v1 north-star's starting point (issue #126). This
//!   slice skips the truck entirely rather than modeling a cutscene with no
//!   consumer; [`SaveBlock1::location`] is set to the bedroom directly
//!   instead of `MAP_INSIDE_OF_TRUCK`.
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
//!   v1 default.
//! - **Trainer id has no link-cable lower half.** Upstream's
//!   `InitPlayerTrainerId` is `(Random() << 16) | GetGeneratedTrainerIdLower()`;
//!   `GetGeneratedTrainerIdLower` (`link_rfu.c`) draws on RFU/link-cable
//!   session state this engine slice has no model for. [`init_save_blocks`]
//!   instead draws both halves from the same [`engine::rng::Rng`] the caller
//!   supplies (`rng.next_u16()` twice), preserving the "one `Random()`-family
//!   draw per half" shape without the unmodeled link dependency.

use engine::overworld::{Direction, TilePos};
use engine::rng::Rng;
use engine::save::{Coords16, PlayerGender, SaveBlock1, SaveBlock2, WarpData};
use engine::text;

use crate::overworld::PlayerCharacter;

/// `sMalePresetNames[0]` == `gText_DefaultNameStu` (module docs) -- the v1
/// fixed default player name, standing in for the deferred naming screen.
/// Fits [`engine::save::block::PLAYER_NAME_LENGTH`] (7 glyphs) with room to
/// spare.
pub const DEFAULT_PLAYER_NAME: &str = "STU";

/// The v1 fixed default player gender (module docs): male, matching
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

/// Build a fresh [`SaveBlock1`]/[`SaveBlock2`] pair the way upstream
/// `NewGameInitData` (`new_game.c:149-207`) does, for the fields this
/// workspace's save model covers (module docs' table and deviations).
///
/// `rng` supplies the trainer id's two halves (module docs, "Trainer id has
/// no link-cable lower half") -- callers that don't care about a specific
/// sequence can pass a freshly seeded [`Rng`].
#[must_use]
pub fn init_save_blocks(rng: &mut Rng) -> (SaveBlock1, SaveBlock2) {
    let block2 = SaveBlock2 {
        player_name: encode_player_name(DEFAULT_PLAYER_NAME),
        player_gender: DEFAULT_PLAYER_GENDER,
        player_trainer_id: trainer_id_bytes(rng),
        encryption_key: 0,
    };

    let spawn = WarpData {
        map_group: SPAWN_MAP_GROUP,
        map_num: SPAWN_MAP_NUM,
        warp_id: -1, // WARP_ID_NONE: not arriving via a resolved warp event.
        x: i16::try_from(SPAWN_POSITION.0).unwrap_or(0),
        y: i16::try_from(SPAWN_POSITION.1).unwrap_or(0),
    };
    let block1 = SaveBlock1 {
        pos: Coords16 {
            x: spawn.x,
            y: spawn.y,
        },
        location: spawn,
        money: STARTING_MONEY,
        ..SaveBlock1::default()
    };

    (block1, block2)
}

/// Seed [`init_save_blocks`]'s [`Rng`] draws its trainer id from when the
/// caller has no seed of its own to supply (module docs, "Trainer id has no
/// link-cable lower half"): this workspace has no wall-clock/hardware
/// entropy source wired into any runtime path yet, and reaching for
/// `std::time` here just to look more "random" would make
/// [`init_save_blocks_for_new_game`]'s output non-deterministic for no
/// modeled behavioural gain -- fixed, like every other RNG seed this
/// workspace's own tests use.
pub const NEW_GAME_RNG_SEED: u32 = 0;

/// [`init_save_blocks`], seeded with [`NEW_GAME_RNG_SEED`] -- the version
/// [`crate::flow::OverworldPhase::load_default`] actually calls at the
/// intro -> overworld handoff (I-3, issue #149's review pass), so that
/// caller doesn't need its own [`Rng`] import just to start a new save.
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

/// `InitPlayerTrainerId`'s two-`Random()`-half shape, minus the unmodeled
/// link-cable lower word (module docs).
fn trainer_id_bytes(rng: &mut Rng) -> [u8; engine::save::block::TRAINER_ID_LENGTH] {
    let hi = rng.next_u16();
    let lo = rng.next_u16();
    let trainer_id = (u32::from(hi) << 16) | u32::from(lo);
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
        // `Rng::new(NEW_GAME_RNG_SEED)` -- proof the convenience wrapper
        // `crate::flow::OverworldPhase::load_default` calls doesn't
        // silently draw from a different sequence.
        let mut rng = Rng::new(NEW_GAME_RNG_SEED);
        let expected = init_save_blocks(&mut rng);
        let actual = init_save_blocks_for_new_game();
        assert_eq!(actual.0.money, expected.0.money);
        assert_eq!(actual.1.player_trainer_id, expected.1.player_trainer_id);
    }

    #[test]
    fn trainer_id_draws_two_rng_halves_deterministically() {
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
}
