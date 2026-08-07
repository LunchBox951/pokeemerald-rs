//! I-6 (issue #214) integration tests: save, exit, reload, continue.
//!
//! These drive the *production* path end to end — the same
//! [`OverworldPhase::step`] the windowed game runs, the same
//! [`super::save_on_exit`] its shutdown calls, the same
//! [`SaveSlot::load`](crate::game_save::SaveSlot::load) its boot calls, and
//! the same [`OverworldPhase::from_saved`]
//! [`OverworldPhase::continue_saved_game`] builds from — with two
//! substitutions that keep them runnable in CI, where there is no extracted
//! asset pack:
//!
//! * the room is `crate::overworld::tests::synthetic_scene`'s flat, open
//!   grid rather than a pack-decoded one (the same fixture
//!   `crate::flow::overworld_phase::tests` already walks the player around),
//!   paired with a *real* map id so every `MapHeaderTable`/`MapEventsTable`
//!   lookup on the way resolves; and
//! * the save file is a per-test scratch path, passed in as a
//!   [`crate::game_save::SaveSlot`] value rather than read from the
//!   process-wide environment.
//!
//! The one production step they cannot take is
//! [`OverworldPhase::continue_saved_game`]'s own
//! `crate::overworld::load_room` call, which needs the pack. The
//! `#[ignore]`d [`real_pack_continue_from_the_main_menu_restores_the_saved_game`]
//! at the bottom closes that gap on the real-pack lane, going all the way
//! through [`super::advance_scene`]'s `CONTINUE` press.
//!
//! [`OverworldPhase::step`]: crate::flow::overworld_phase::OverworldPhase::step

use engine::event_data::EventData;
use engine::overworld::{Direction, PlayerState};
use engine::save::{BoxPokemon, Pokemon, PokemonSubstructures, SaveBlock1, SaveBlock2};
use platform::Buttons;

use super::overworld_phase::{saved_map_id, OverworldPhase};
use super::tests::{held, pressed, TempSave};
use super::{menu_type_for, save_on_exit, AppScene, MainMenuState};
use crate::main_menu::{MainMenuItem, MainMenuType};
use crate::new_game;

/// `FLAG_RECEIVED_RUNNING_SHOES` (`pokeemerald/include/constants/flags.h:300`)
/// — an ordinary flag no new-game initialization sets, so setting it is a
/// real mid-play mutation rather than something the reloaded blocks would
/// have anyway.
const FLAG_RECEIVED_RUNNING_SHOES: u16 = 0x112;

/// `VAR_REPEL_STEP_COUNT` (`pokeemerald/include/constants/vars.h:51`) — an
/// ordinary (non-temp) var, likewise untouched by new-game init.
const VAR_REPEL_STEP_COUNT: u16 = 0x4021;

/// A party member built through the save model's own encoder: real
/// `personality`/`ot_id` (so the substructure order and XOR key are
/// non-trivial), real substructure bytes, and the party-only stats block.
///
/// Deliberately *not* `Pokemon::default()`: a zeroed mon would round-trip
/// through a buggy serializer just as happily as a correct one.
fn a_party_member(ot_id: u32) -> Pokemon {
    let mut box_data = BoxPokemon::new(0x1234_ABCD, ot_id);
    box_data.set_substructures(&PokemonSubstructures {
        // Growth: species 277 (Treecko) at offset 0, held item none.
        growth: [0x15, 0x01, 0, 0, 0x40, 0x1F, 0, 0, 0, 0, 70, 0],
        // Attacks: POUND / LEER, 35/30 PP.
        attacks: [0x01, 0x00, 0x2B, 0x00, 0, 0, 0, 0, 35, 30, 0, 0],
        evs_and_condition: [1, 2, 3, 4, 5, 6, 0, 0, 0, 0, 0, 0],
        misc: [0, 0, 0, 0, 0x11, 0x22, 0x33, 0x44, 0, 0, 0, 0],
    });
    Pokemon {
        box_data,
        status: 0,
        level: 7,
        mail: 0xFF,
        hp: 17,
        max_hp: 23,
        attack: 12,
        defense: 11,
        speed: 15,
        special_attack: 13,
        special_defense: 10,
    }
}

/// A brand-new game in the protagonist's bedroom, on a synthetic room grid
/// (module docs). Goes through `OverworldPhase::for_test` -> `::new`, i.e.
/// through `new_game::init_save_blocks`, so the starting state is the real
/// one.
fn new_game_phase() -> OverworldPhase {
    OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene(10, 10),
        new_game::SPAWN_MAP_ID,
        PlayerState::new(
            new_game::SPAWN_POSITION,
            new_game::SPAWN_ELEVATION,
            new_game::SPAWN_FACING,
        ),
        None,
    )
}

/// Everything the round-trip asserts on, read out of a phase.
#[derive(Debug, PartialEq, Eq)]
struct Snapshot {
    map: assets::MapId,
    position: (i32, i32),
    facing: Direction,
    money: u32,
    running_shoes: bool,
    repel_steps: u16,
    party_count: u8,
    lead: Pokemon,
    player_name: [u8; engine::save::block::PLAYER_NAME_BUF_LEN],
    trainer_id: [u8; engine::save::block::TRAINER_ID_LENGTH],
    encryption_key: u32,
}

fn snapshot(phase: &OverworldPhase) -> Snapshot {
    let flags: &EventData = &phase.save1.event_data;
    Snapshot {
        map: phase.map_id,
        position: phase.player.position(),
        facing: phase.player.facing(),
        money: phase.save1.money,
        running_shoes: flags.flag_get(FLAG_RECEIVED_RUNNING_SHOES).unwrap(),
        repel_steps: flags.var_get(VAR_REPEL_STEP_COUNT).unwrap(),
        party_count: phase.save1.player_party_count,
        lead: phase.save1.player_party[0],
        player_name: phase.save2.player_name,
        trainer_id: phase.save2.player_trainer_id,
        encryption_key: phase.save2.encryption_key,
    }
}

/// Play a little: walk south until the player has actually moved, then make
/// the state unmistakably mid-game — a flag, a var, spent money, a party
/// member, and a nonzero encryption key (so the save's *encrypted* fields
/// are exercised too, not just its plaintext ones).
fn play_a_bit(phase: &mut OverworldPhase) {
    for _ in 0..40 {
        phase.step(held(Buttons::DOWN));
    }
    assert_ne!(
        phase.player.position(),
        new_game::SPAWN_POSITION,
        "the fixture must actually walk the player off the spawn tile"
    );

    assert!(
        !phase
            .save1
            .event_data
            .flag_get(FLAG_RECEIVED_RUNNING_SHOES)
            .unwrap(),
        "the chosen flag must start clear, or setting it proves nothing"
    );
    phase
        .save1
        .event_data
        .flag_set(FLAG_RECEIVED_RUNNING_SHOES)
        .unwrap();
    phase
        .save1
        .event_data
        .var_set(VAR_REPEL_STEP_COUNT, 37)
        .unwrap();

    phase.save2.encryption_key = 0x0BAD_F00D;
    phase.save1.money = 1_234;
    phase.save1.player_party_count = 1;
    phase.save1.player_party[0] = a_party_member(0x0011_2233);
}

/// The I-6 acceptance round trip: new game -> play -> save -> reload ->
/// continue, with the restored phase matching what was saved.
#[test]
fn a_saved_game_reloads_into_an_overworld_phase_that_matches_it() {
    let temp = TempSave::new("round-trip");
    let slot = temp.slot();

    let mut phase = new_game_phase();
    play_a_bit(&mut phase);
    let before = snapshot(&phase);

    // The real write trigger, not a hand-rolled call to the store.
    save_on_exit(&AppScene::Overworld(Box::new(phase)), &slot)
        .expect("the overworld has save state to write")
        .expect("writing the scratch save file must succeed");

    // The real boot load, and the menu it selects.
    let saved = slot.load();
    assert!(
        saved.status.menu_shows_continue(),
        "a save just written must be offerable as CONTINUE, got {:?}",
        saved.status
    );
    assert_eq!(menu_type_for(&saved), MainMenuType::SavedGame);

    // The real map resolution `continue_saved_game` performs, then its
    // pack-free core (module docs on the one substituted step).
    let map = saved_map_id(&saved.block1).expect("the saved location must resolve to a map");
    assert_eq!(map, new_game::SPAWN_MAP_ID);
    let resumed = OverworldPhase::from_saved(
        crate::overworld::tests::synthetic_scene(10, 10),
        map,
        saved.block1,
        saved.block2,
    );

    let after = snapshot(&resumed);
    assert_eq!(
        after.position, before.position,
        "the player must reload on the tile they saved on"
    );
    assert_eq!(after.map, before.map);
    assert_eq!(after.money, 1_234);
    assert!(after.running_shoes, "the set flag must survive the save");
    assert_eq!(after.repel_steps, 37, "the set var must survive the save");
    assert_eq!(after.party_count, 1);
    assert_eq!(
        after.lead,
        a_party_member(0x0011_2233),
        "the party member must round-trip byte-for-byte, encrypted \
         substructures included"
    );
    assert_eq!(after.player_name, before.player_name);
    assert_eq!(after.trainer_id, before.trainer_id);
    assert_eq!(after.encryption_key, 0x0BAD_F00D);

    // Facing is the one field this continue does *not* restore, and the
    // assertion pins the stand-in rather than upstream: with no object-event
    // model there is nothing to read the saved direction out of, so the
    // continue-game *warp* branch's `GetAdjustedInitialDirection`
    // (`src/overworld.c:929-951`) decides it from the tile instead, and an
    // ordinary tile falls through to `DIR_SOUTH` (`:951`). Upstream's own
    // ordinary continue path restores the saved facing -- see the deferral
    // written up in `OverworldPhase::from_saved`. Change this expectation
    // when object events land; do not read it as fidelity.
    assert_eq!(
        after.facing,
        Direction::South,
        "the tile-derived stand-in must be what a continue places, until a \
         saved facing exists to restore"
    );
}

/// Saving twice in one session must not lose the second save: the rotation
/// counter is re-derived from the file each time
/// (`SaveSlot::store`'s docs), so the newer write wins even though nothing
/// held `gSaveCounter` in memory between the two.
#[test]
fn the_most_recent_of_two_saves_is_the_one_that_reloads() {
    let temp = TempSave::new("two-slot");
    let slot = temp.slot();

    let mut phase = new_game_phase();
    play_a_bit(&mut phase);
    let first_position = phase.player.position();
    save_on_exit(&AppScene::Overworld(Box::new(phase)), &slot)
        .unwrap()
        .unwrap();

    let mut phase = new_game_phase();
    play_a_bit(&mut phase);
    phase.save1.money = 4_321;
    for _ in 0..20 {
        phase.step(held(Buttons::RIGHT));
    }
    let second_position = phase.player.position();
    assert_ne!(
        first_position, second_position,
        "the second session must end somewhere else, or this proves nothing"
    );
    save_on_exit(&AppScene::Overworld(Box::new(phase)), &slot)
        .unwrap()
        .unwrap();

    let saved = slot.load();
    assert!(saved.status.menu_shows_continue());
    assert_eq!(saved.block1.money, 4_321);
    assert_eq!(
        (i32::from(saved.block1.pos.x), i32::from(saved.block1.pos.y)),
        second_position
    );
}

/// A corrupt save falls back to `NEW GAME` by upstream's own rule
/// (`SAVE_STATUS_CORRUPT` -> `HAS_NO_SAVED_GAME`, `main_menu.c:649-653`) --
/// the menu offers no `CONTINUE` at all, so there is nothing to select that
/// could resume damaged data.
#[test]
fn a_corrupt_save_offers_no_continue_and_falls_back_to_new_game() {
    let temp = TempSave::new("corrupt-fallback");
    let slot = temp.slot();

    let mut phase = new_game_phase();
    play_a_bit(&mut phase);
    save_on_exit(&AppScene::Overworld(Box::new(phase)), &slot)
        .unwrap()
        .unwrap();
    assert!(slot.load().status.menu_shows_continue());

    // Damage the slot the write landed in (the second half of the image --
    // `gSaveCounter` advanced to 1 and its parity picks slot 1). Slot 0 is
    // still erased, so no intact slot survives.
    let file = temp.path();
    let mut image = std::fs::read(file).unwrap();
    let slot_one = image.len() / 2;
    image[slot_one] ^= 0xFF;
    std::fs::write(file, &image).unwrap();

    let saved = slot.load();
    assert!(!saved.status.menu_shows_continue());
    assert_eq!(menu_type_for(&saved), MainMenuType::NoSavedGame);
    assert_eq!(
        crate::main_menu::MainMenuType::NoSavedGame.items(),
        [MainMenuItem::NewGame, MainMenuItem::Option],
        "the fallback menu must not contain CONTINUE"
    );
}

/// A save whose `location` names no known map must not resume into an
/// arbitrary one -- `continue_saved_game` fails closed
/// (`ContinueError::UnknownLocation`) and `advance_scene` leaves the player
/// on the main menu. Pack-free: the map lookup fails before any room load
/// is attempted.
#[test]
fn a_save_pointing_at_no_known_map_does_not_resume() {
    let mut block1 = SaveBlock1::default();
    block1.location.map_group = 127;
    block1.location.map_num = 127;
    assert!(saved_map_id(&block1).is_none());

    // `OverworldPhase` has no `Debug`, so this cannot be `expect_err`.
    let Err(err) = OverworldPhase::continue_saved_game(block1, SaveBlock2::default()) else {
        panic!("an unknown location must not resolve to some other map");
    };
    assert!(
        err.to_string().contains("map-header table"),
        "the diagnostic must say why: {err}"
    );
}

/// The `#[ignore]`d half of the round trip (module docs): the whole
/// `CONTINUE` press, through `advance_scene` and the real
/// `OverworldPhase::continue_saved_game` -> `crate::overworld::load_room`.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_continue_from_the_main_menu_restores_the_saved_game() {
    let temp = TempSave::new("real-pack-continue");
    let slot = temp.slot();

    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
    play_a_bit(&mut phase);
    let before = snapshot(&phase);
    save_on_exit(&AppScene::Overworld(Box::new(phase)), &slot)
        .unwrap()
        .unwrap();

    let saved = slot.load();
    let menu_type = menu_type_for(&saved);
    assert_eq!(menu_type, MainMenuType::SavedGame);
    let menu = crate::main_menu::load_default(menu_type).expect("run `cargo xtask extract` first");
    assert_eq!(menu.selected(), MainMenuItem::Continue);

    let scene = AppScene::MainMenu(Box::new(MainMenuState { scene: menu, saved }));
    let (next, _frame) = super::advance_scene(scene, pressed(Buttons::A), &slot);

    let AppScene::Overworld(resumed) = next else {
        panic!("A on CONTINUE must hand off to the overworld");
    };
    let after = snapshot(&resumed);
    assert_eq!(after.map, before.map);
    assert_eq!(after.position, before.position);
    assert_eq!(after.money, before.money);
    assert!(after.running_shoes);
    assert_eq!(after.repel_steps, 37);
    assert_eq!(after.party_count, before.party_count);
    assert_eq!(after.lead, before.lead);
    assert_eq!(after.trainer_id, before.trainer_id);
    assert_eq!(after.encryption_key, before.encryption_key);
}
