//! I-6 (issues #214/#232) integration tests: start-menu save, exit, reload,
//! continue.
//!
//! These drive the *production* path end to end — the same
//! [`OverworldPhase::step`] the windowed game runs, the same
//! `OverworldPhase::advance_start_menu_frame` its `START` press reaches,
//! the same [`crate::start_menu::StartMenu`] state machine and
//! [`crate::game_save::SaveSlot::store`] behind its `SAVE` action, the same
//! [`SaveSlot::load`](crate::game_save::SaveSlot::load) its boot calls, and
//! the same [`OverworldPhase::from_saved`]
//! [`OverworldPhase::continue_saved_game`] builds from — with three
//! substitutions that keep them runnable in CI, where there is no extracted
//! asset pack:
//!
//! * the room is `crate::overworld::tests::synthetic_scene`'s flat, open
//!   grid rather than a pack-decoded one (the same fixture
//!   `crate::flow::overworld_phase::step_tests` already walks the player
//!   around),
//!   paired with a *real* map id so every `MapHeaderTable`/`MapEventsTable`
//!   lookup on the way resolves;
//! * the start menu is opened through
//!   `OverworldPhase::open_synthetic_start_menu` — blank glyph sheet and
//!   blank window frames, everything else (the item list, the cursor, the
//!   whole `sSaveDialogCallback` chain, and the write itself) production;
//!   and
//! * the save file is a per-test scratch path, passed in as a
//!   [`crate::game_save::SaveSlot`] value rather than read from the
//!   process-wide environment.
//!
//! The two production steps they cannot take are
//! [`OverworldPhase::continue_saved_game`]'s own
//! `crate::overworld::load_room` call and `crate::start_menu::open_default`,
//! both of which need the pack. The two `#[ignore]`d `real_pack_*` cases
//! below close that gap on the real-pack lane.
//!
//! [`OverworldPhase::step`]: crate::flow::overworld_phase::OverworldPhase::step

use engine::event_data::EventData;
use engine::overworld::{Direction, PlayerState};
use engine::save::{BoxPokemon, Pokemon, SaveBlock1, SaveBlock2, WarpData};
use platform::Buttons;

use super::overworld_phase::{saved_map_id, OverworldPhase};
use super::tests::{held, pressed, TempSave};
use super::{menu_type_for, AppScene, MainMenuState};
use crate::game_save::SaveSlot;
use crate::main_menu::{MainMenuItem, MainMenuType};
use crate::new_game;
use crate::start_menu::StartMenuItem;

/// `FLAG_RECEIVED_RUNNING_SHOES` (`pokeemerald/include/constants/flags.h:300`)
/// — an ordinary flag no new-game initialization sets, so setting it is a
/// real mid-play mutation rather than something the reloaded blocks would
/// have anyway.
const FLAG_RECEIVED_RUNNING_SHOES: u16 = 0x112;

/// `VAR_REPEL_STEP_COUNT` (`pokeemerald/include/constants/vars.h:51`) — an
/// ordinary (non-temp) var, likewise untouched by new-game init.
const VAR_REPEL_STEP_COUNT: u16 = 0x4021;

/// How many frames a save flow gets before the fixture calls it wedged.
/// Deliberately generous: `gText_DifferentSaveFile`'s four-page WARNING is
/// ~200 glyphs, and `TextSpeed::Mid` reveals one every four frames, so the
/// longest flow here is well over a thousand frames before the success
/// message's own `SaveStartTimer` 60 even starts.
const SAVE_FLOW_FRAME_BUDGET: usize = 4_000;

/// A brand-new game in the protagonist's bedroom, on a synthetic room grid
/// (module docs). Goes through `OverworldPhase::for_test` -> `::new`, i.e.
/// through `new_game::init_save_blocks`, so the starting state is the real
/// one.
pub(super) fn new_game_phase() -> OverworldPhase {
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

/// Everything the round trip asserts on, read out of a phase.
///
/// `lead` is the *battle-facing* mon (`OverworldPhase::party_lead`), not
/// the save block's bytes: that is the value a continued session actually
/// fights with, and re-deriving it from
/// [`new_game::provisional_starter`] is exactly the stand-in issue #232
/// removed.
#[derive(Debug, PartialEq)]
struct Snapshot {
    map: assets::MapId,
    position: (i32, i32),
    facing: Direction,
    elevation: u8,
    money: u32,
    running_shoes: bool,
    repel_steps: u16,
    /// `gSaveBlock1Ptr->playerPartyCount` as it sits in the block --
    /// `SavePlayerParty`'s own output (`src/load_save.c:180-186`). Pinned
    /// by name alongside `lead` because the two can disagree: a decode that
    /// silently dropped the mon would still leave `lead` matching a
    /// re-derived starter, and a count that never got written would still
    /// leave the bytes on disk.
    party_count: u8,
    lead: Option<battle::BattlePokemon>,
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
        elevation: phase.player.elevation(),
        money: phase.save1.money,
        running_shoes: flags.flag_get(FLAG_RECEIVED_RUNNING_SHOES).unwrap(),
        repel_steps: flags.var_get(VAR_REPEL_STEP_COUNT).unwrap(),
        party_count: phase.save1.player_party_count,
        lead: phase.party_lead.clone(),
        player_name: phase.save2.player_name,
        trainer_id: phase.save2.player_trainer_id,
        encryption_key: phase.save2.encryption_key,
    }
}

/// Run input-free frames until no step is in flight: the start menu does
/// not open while the player is moving
/// (`OverworldPhase::start_menu_may_open`, #230 review round five), and
/// these fixtures save from a player at rest.
pub(super) fn settle(phase: &mut OverworldPhase) {
    for _ in 0..20 {
        if !phase.mid_step() {
            return;
        }
        phase.step(held(Buttons::NONE));
    }
    panic!("the fixture must come to rest before it saves");
}

/// Play a little: walk south, then west (so the player ends up facing a
/// direction the tile-derived `DIR_SOUTH` fallback would *not* produce),
/// then make the state unmistakably mid-game — a flag, a var, spent money,
/// a damaged party lead, and a nonzero encryption key (so the save's
/// *encrypted* fields are exercised too, not just its plaintext ones).
fn play_a_bit(phase: &mut OverworldPhase) {
    for _ in 0..40 {
        phase.step(held(Buttons::DOWN));
    }
    for _ in 0..40 {
        phase.step(held(Buttons::LEFT));
    }
    settle(phase);
    assert_ne!(
        phase.player.position(),
        new_game::SPAWN_POSITION,
        "the fixture must actually walk the player off the spawn tile"
    );
    assert_eq!(
        phase.player.facing(),
        Direction::West,
        "the fixture must end facing somewhere the DIR_SOUTH fallback is not"
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
    phase.party_lead = Some(a_damaged_lead());
}

/// The provisional starter after a fight it did not walk away from
/// unscathed: HP spent and one move's PP spent. A full-HP, full-PP lead
/// would round-trip just as happily through an encoder that dropped both.
fn a_damaged_lead() -> battle::BattlePokemon {
    let mut lead = new_game::provisional_starter();
    lead.apply_damage(5);
    lead.deduct_pp(0).unwrap();
    lead
}

/// A recognizable, checksum-valid serialized party member whose bytes can
/// prove a dormant slot survived a continue/save cycle untouched.
fn dormant_party_member(marker: u32) -> Pokemon {
    let bytes = marker.to_le_bytes();
    let low = u16::from_le_bytes([bytes[0], bytes[1]]);
    let high = u16::from_le_bytes([bytes[2], bytes[3]]);
    Pokemon {
        box_data: BoxPokemon::new(marker, marker.rotate_left(7)),
        status: marker.rotate_right(3),
        level: bytes[0],
        mail: bytes[1],
        hp: low,
        max_hp: high,
        attack: low.wrapping_add(1),
        defense: low.wrapping_add(2),
        speed: low.wrapping_add(3),
        special_attack: low.wrapping_add(4),
        special_defense: low.wrapping_add(5),
    }
}

/// Drive an already-open start menu until it closes (or its SAVE flow is
/// cancelled back to the item list), answering each Yes/No prompt from
/// `answers` in order — `true` is YES — and defaulting to YES once
/// `answers` runs out.
///
/// Returns the cursor row each prompt *opened* on (`0` = YES, `1` = NO),
/// which is the observable difference between upstream's two overwrite
/// prompts: `DisplayYesNoMenuDefaultYes` for the ordinary one,
/// `DisplayYesNoMenuWithDefault(1)` for `gText_DifferentSaveFile`'s
/// WARNING.
fn drive_start_menu(
    phase: &mut OverworldPhase,
    save_slot: &mut SaveSlot,
    answers: &[bool],
) -> Vec<u8> {
    let mut opened_on: Vec<u8> = Vec::new();
    let mut answered = 0usize;
    let mut entered_save_flow = false;
    for _ in 0..SAVE_FLOW_FRAME_BUDGET {
        let Some(menu) = phase.start_menu() else {
            return opened_on;
        };
        if menu.saving() {
            entered_save_flow = true;
        } else if entered_save_flow {
            // `SAVE_CANCELED`: the flow put the item list back up. Stop
            // here rather than picking SAVE again forever.
            return opened_on;
        }
        let buttons = match menu.yes_no_cursor() {
            Some(cursor) => {
                if opened_on.len() == answered {
                    opened_on.push(cursor);
                }
                let wants_yes = answers.get(answered).copied().unwrap_or(true);
                let desired = u8::from(!wants_yes);
                match cursor.cmp(&desired) {
                    std::cmp::Ordering::Equal => {
                        answered += 1;
                        pressed(Buttons::A)
                    }
                    std::cmp::Ordering::Greater => pressed(Buttons::UP),
                    std::cmp::Ordering::Less => pressed(Buttons::DOWN),
                }
            }
            // No prompt waiting: A advances whatever message is printing,
            // picks SAVE off the item list, and dismisses the result
            // message once the write is done.
            None => pressed(Buttons::A),
        };
        assert!(
            phase.advance_start_menu_frame(buttons, save_slot),
            "an open start menu owns its frame"
        );
    }
    panic!("the save flow must terminate within {SAVE_FLOW_FRAME_BUDGET} frames");
}

/// `START` -> `SAVE` -> YES to everything, through the real menu.
pub(super) fn save_from_the_start_menu(
    phase: &mut OverworldPhase,
    save_slot: &mut SaveSlot,
) -> Vec<u8> {
    phase.open_synthetic_start_menu();
    assert_eq!(
        phase.start_menu().unwrap().selected(),
        StartMenuItem::Save,
        "SAVE is the first item, so a fresh menu opens on it"
    );
    let prompts = drive_start_menu(phase, save_slot, &[]);
    assert!(
        phase.start_menu().is_none(),
        "a completed save closes the start menu"
    );
    prompts
}

/// The I-6 acceptance round trip: new game -> play -> start-menu save ->
/// reload -> continue, with the restored phase matching what was saved.
#[test]
fn a_saved_game_reloads_into_an_overworld_phase_that_matches_it() {
    let temp = TempSave::new("round-trip");
    let mut slot = temp.slot();

    let mut phase = new_game_phase();
    play_a_bit(&mut phase);
    let before = snapshot(&phase);
    assert!(
        phase.different_save_file(),
        "a new-game session is upstream's gDifferentSaveFile == TRUE"
    );

    // The real write trigger: the player's own START -> SAVE.
    let prompts = save_from_the_start_menu(&mut phase, &mut slot);
    assert_eq!(
        prompts.len(),
        1,
        "an empty cartridge asks only gText_ConfirmSave -- there is nothing \
         to overwrite (start_menu.c:1008-1019)"
    );
    assert!(
        !phase.different_save_file(),
        "a successful SAVE_OVERWRITE_DIFFERENT_FILE clears gDifferentSaveFile"
    );

    // The real boot load, and the menu it selects.
    let saved = slot.load();
    assert!(
        saved.status.menu_shows_continue(),
        "a save just written must be offerable as CONTINUE, got {:?}",
        saved.status
    );
    assert_eq!(menu_type_for(&saved), MainMenuType::SavedGame);

    // The real map resolution `continue_saved_game` performs, then its
    // pack-free core (module docs on the substituted steps).
    let map =
        saved_map_id(saved.block1.location).expect("the saved location must resolve to a map");
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
    assert_eq!(
        after.elevation, before.elevation,
        "the resumed player must stand at the saved tile's own elevation"
    );
    assert_ne!(
        after.elevation,
        new_game::SPAWN_ELEVATION,
        "the fixture must resume off the spawn elevation, or a hardcoded \
         fallback in saved_tile_placement would pass this test"
    );
    // Issue #232: the saved facing is restored from the player object
    // event (`LoadObjectEvents`, `src/load_save.c:188-194`), not derived
    // from the tile -- which for this ordinary tile would be `DIR_SOUTH`.
    assert_eq!(
        after.facing,
        Direction::West,
        "the continue must restore the direction the player saved facing"
    );
    assert_ne!(
        after.facing,
        new_game::SPAWN_FACING,
        "the fixture must save facing somewhere the tile-derived stand-in is not"
    );
    assert_eq!(after.money, 1_234);
    assert!(after.running_shoes, "the set flag must survive the save");
    assert_eq!(after.repel_steps, 37, "the set var must survive the save");
    // `SavePlayerParty`'s raw count, pinned by name: the write itself is
    // what put it there (`copy_party_and_objects_to_save`), so `before`
    // still reads 0 -- the block only learns about the live lead at
    // `HandleSavingData` time, exactly as upstream's does.
    assert_eq!(
        before.party_count, 0,
        "the block's count is written by the save, not held live"
    );
    assert_eq!(
        after.party_count, 1,
        "gSaveBlock1Ptr->playerPartyCount must round-trip as bytes, not \
         just as a decoded lead"
    );
    // Issue #232: the actual saved party, not a fresh provisional starter.
    assert_eq!(
        after.lead,
        Some(a_damaged_lead()),
        "the continued session must fight with the mon that was saved -- \
         damage taken and PP spent included"
    );
    assert_ne!(
        after.lead,
        Some(new_game::provisional_starter()),
        "a re-derived starter would be undamaged, so the fixture must not \
         accidentally save one"
    );
    assert_eq!(after.player_name, before.player_name);
    assert_eq!(after.trainer_id, before.trainer_id);
    assert_eq!(after.encryption_key, 0x0BAD_F00D);
    assert!(
        !resumed.different_save_file(),
        "a continued session *is* the file on disk"
    );
}

/// The live runtime currently owns only the lead, but a continued save may
/// carry more party members in its exact serialized block. Re-saving that
/// session must update slot 0 without erasing the dormant members or count.
#[test]
fn a_continued_multi_member_party_preserves_trailing_slots_and_count() {
    let temp = TempSave::new("multi-member-party");
    let mut slot = temp.slot();
    let mut seed = new_game_phase();
    let trainer_id = u32::from_le_bytes(seed.save2.player_trainer_id);
    let lead = a_damaged_lead().with_original_trainer_id(trainer_id);
    seed.save1.player_party_count = 4;
    seed.save1.player_party[0] = crate::party::to_save_pokemon(&battle::Dex::new(), &lead);
    seed.save1.player_party[1] = dormant_party_member(0x1111_2222);
    seed.save1.player_party[2] = dormant_party_member(0x3333_4444);
    seed.save1.player_party[3] = dormant_party_member(0x5555_6666);
    let expected_trailing = seed.save1.player_party[1..].to_vec();

    let mut resumed = OverworldPhase::from_saved(
        crate::overworld::tests::synthetic_scene(10, 10),
        seed.map_id,
        seed.save1,
        seed.save2,
    );
    assert_eq!(resumed.save1.player_party_count, 4);
    assert_eq!(resumed.party_lead, Some(lead));

    save_from_the_start_menu(&mut resumed, &mut slot);
    let saved = slot.load();
    assert_eq!(saved.block1.player_party_count, 4);
    assert_eq!(saved.block1.player_party[1..], expected_trailing);
}

/// A traded lead is encrypted under its original trainer's id, not the id
/// of the player who currently owns and saves it.
#[test]
fn a_continued_traded_lead_preserves_its_original_trainer_id() {
    const PLAYER_ID: u32 = 0x1122_3344;
    const TRADED_OT_ID: u32 = 0xAABB_CCDD;

    let temp = TempSave::new("traded-lead");
    let mut slot = temp.slot();
    let mut seed = new_game_phase();
    seed.save2.player_trainer_id = PLAYER_ID.to_le_bytes();
    let traded_lead = a_damaged_lead().with_original_trainer_id(TRADED_OT_ID);
    seed.save1.player_party_count = 1;
    seed.save1.player_party[0] = crate::party::to_save_pokemon(&battle::Dex::new(), &traded_lead);

    let mut resumed = OverworldPhase::from_saved(
        crate::overworld::tests::synthetic_scene(10, 10),
        seed.map_id,
        seed.save1,
        seed.save2,
    );
    assert_eq!(
        resumed
            .party_lead
            .as_ref()
            .map(battle::BattlePokemon::original_trainer_id),
        Some(TRADED_OT_ID)
    );
    assert_ne!(TRADED_OT_ID, PLAYER_ID, "the fixture must model a trade");

    save_from_the_start_menu(&mut resumed, &mut slot);
    let saved = slot.load();
    assert_eq!(saved.block2.player_trainer_id, PLAYER_ID.to_le_bytes());
    assert_eq!(saved.block1.player_party[0].box_data.ot_id(), TRADED_OT_ID);

    let reloaded = OverworldPhase::from_saved(
        crate::overworld::tests::synthetic_scene(10, 10),
        seed.map_id,
        saved.block1,
        saved.block2,
    );
    assert_eq!(
        reloaded
            .party_lead
            .as_ref()
            .map(battle::BattlePokemon::original_trainer_id),
        Some(TRADED_OT_ID)
    );
}

/// Saving twice in one session must not lose the second save: the rotation
/// counter is re-derived from the file each time (`SaveSlot::store`'s
/// docs), so the newer write wins even though nothing held `gSaveCounter`
/// in memory between the two.
#[test]
fn the_most_recent_of_two_saves_is_the_one_that_reloads() {
    let temp = TempSave::new("two-slot");
    let mut slot = temp.slot();

    let mut phase = new_game_phase();
    play_a_bit(&mut phase);
    let first_position = phase.player.position();
    save_from_the_start_menu(&mut phase, &mut slot);

    // The second session *continues* the first save, which is the flow a
    // player actually takes; a second new-game session would meet the
    // WARNING prompt instead (see the consent test below).
    let loaded = slot.load();
    let map = saved_map_id(loaded.block1.location).expect("the saved location must resolve");
    let mut phase = OverworldPhase::from_saved(
        crate::overworld::tests::synthetic_scene(10, 10),
        map,
        loaded.block1,
        loaded.block2,
    );
    phase.save1.money = 4_321;
    for _ in 0..20 {
        phase.step(held(Buttons::RIGHT));
    }
    settle(&mut phase);
    let second_position = phase.player.position();
    assert_ne!(
        first_position, second_position,
        "the second session must end somewhere else, or this proves nothing"
    );
    let prompts = save_from_the_start_menu(&mut phase, &mut slot);
    assert_eq!(
        prompts,
        vec![0, 0],
        "a continued session is asked twice -- gText_ConfirmSave then \
         gText_AlreadySavedFile -- both defaulting to YES"
    );

    let saved = slot.load();
    assert!(saved.status.menu_shows_continue());
    assert_eq!(saved.block1.money, 4_321);
    assert_eq!(
        (i32::from(saved.block1.pos.x), i32::from(saved.block1.pos.y)),
        second_position
    );
}

/// Answering NO to `gText_ConfirmSave` must leave the file untouched: the
/// flow reports `SAVE_CANCELED` and puts the start menu back
/// (`SaveConfirmInputCallback`'s `MENU_B_PRESSED`/NO arm,
/// `start_menu.c:1026-1030`).
#[test]
fn declining_the_confirmation_writes_nothing() {
    let temp = TempSave::new("confirm-no");
    let mut slot = temp.slot();

    let mut phase = new_game_phase();
    play_a_bit(&mut phase);
    phase.open_synthetic_start_menu();
    drive_start_menu(&mut phase, &mut slot, &[false]);

    assert!(
        phase.start_menu().is_some(),
        "a cancelled save returns to the start menu rather than closing it"
    );
    assert!(
        !phase.start_menu().unwrap().saving(),
        "the SAVE flow must have handed the item list back"
    );
    assert!(
        !temp.slot().load().status.menu_shows_continue(),
        "nothing may have been written"
    );
    assert!(
        phase.different_save_file(),
        "a declined save leaves gDifferentSaveFile set"
    );
}

/// A corrupt save falls back to `NEW GAME` by upstream's own rule
/// (`SAVE_STATUS_CORRUPT` -> `HAS_NO_SAVED_GAME`, `main_menu.c:649-653`) --
/// the menu offers no `CONTINUE` at all, so there is nothing to select that
/// could resume damaged data.
#[test]
fn a_corrupt_save_offers_no_continue_and_falls_back_to_new_game() {
    let temp = TempSave::new("corrupt-fallback");
    let mut slot = temp.slot();

    let mut phase = new_game_phase();
    play_a_bit(&mut phase);
    save_from_the_start_menu(&mut phase, &mut slot);
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
    assert!(saved_map_id(block1.location).is_none());

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
    let mut slot = temp.slot();

    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
    play_a_bit(&mut phase);
    let before = snapshot(&phase);
    save_from_the_start_menu(&mut phase, &mut slot);

    let saved = slot.load();
    let menu_type = menu_type_for(&saved);
    assert_eq!(menu_type, MainMenuType::SavedGame);
    let menu = crate::main_menu::load_default(menu_type).expect("run `cargo xtask extract` first");
    assert_eq!(menu.selected(), MainMenuItem::Continue);

    let scene = AppScene::MainMenu(Box::new(MainMenuState { scene: menu, saved }));
    let (next, _frame) = super::advance_scene(scene, pressed(Buttons::A), &mut slot);

    let AppScene::Overworld(resumed) = next else {
        panic!("A on CONTINUE must hand off to the overworld");
    };
    let after = snapshot(&resumed);
    assert_eq!(after.map, before.map);
    assert_eq!(after.position, before.position);
    assert_eq!(after.facing, before.facing);
    assert_eq!(after.money, before.money);
    assert!(after.running_shoes);
    assert_eq!(after.repel_steps, 37);
    assert_eq!(after.party_count, 1);
    assert_eq!(after.lead, before.lead);
    assert_eq!(after.trainer_id, before.trainer_id);
    assert_eq!(after.encryption_key, before.encryption_key);
}

/// The other pack-gated step (module docs): `START` really opening the
/// menu through `crate::start_menu::open_default`, with real chrome, and a
/// whole save running through the pack-decoded windows.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_start_opens_the_menu_and_saves() {
    let temp = TempSave::new("real-pack-start-menu");
    let mut slot = temp.slot();

    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
    settle(&mut phase);
    assert!(
        phase.advance_start_menu_frame(pressed(Buttons::START), &mut slot),
        "START must open the menu and own the frame"
    );
    assert!(
        phase.start_menu().is_some(),
        "run `cargo xtask extract` first"
    );

    drive_start_menu(&mut phase, &mut slot, &[]);
    assert!(phase.start_menu().is_none());
    assert!(slot.load().status.menu_shows_continue());
}

/// The consent gate, now as upstream's own prompt (issue #232, replacing
/// #214's save-on-exit `store_unless_foreign_save` stand-in): a NEW GAME
/// session saving over someone else's file meets `gText_DifferentSaveFile`,
/// whose Yes/No menu opens on **NO** (`DisplayYesNoMenuWithDefault(1)`,
/// `start_menu.c:1049-1053`). Answering NO leaves the file byte-identical;
/// answering YES replaces it, because the player said so.
#[test]
fn a_new_game_session_must_answer_the_overwrite_warning_before_it_can_clobber_a_save() {
    let temp = TempSave::new("consent-gate");
    let mut slot = temp.slot();

    // A real prior save on disk, written by its own session.
    let mut original_phase = new_game_phase();
    play_a_bit(&mut original_phase);
    save_from_the_start_menu(&mut original_phase, &mut slot);
    let original = std::fs::read(temp.path()).unwrap();

    // A second session boots, sees that save (`gSaveFileStatus ==
    // SAVE_STATUS_OK`), and picks NEW GAME.
    let mut fresh_slot = temp.slot();
    assert!(fresh_slot.load().status.menu_shows_continue());
    let mut fresh = new_game_phase();
    fresh.open_synthetic_start_menu();
    let prompts = drive_start_menu(&mut fresh, &mut fresh_slot, &[true, false]);
    assert_eq!(
        prompts,
        vec![0, 1],
        "gText_ConfirmSave defaults to YES; the WARNING defaults to NO"
    );
    assert_eq!(
        std::fs::read(temp.path()).unwrap(),
        original,
        "declining the WARNING must leave the save byte-identical"
    );

    // The same session, this time consenting.
    fresh.open_synthetic_start_menu();
    drive_start_menu(&mut fresh, &mut fresh_slot, &[true, true]);
    assert_ne!(
        std::fs::read(temp.path()).unwrap(),
        original,
        "an answered WARNING really does replace the other file"
    );
}

/// `SaveDoSaveCallback` clears `gDifferentSaveFile` on the statement after
/// `TrySavingData(SAVE_OVERWRITE_DIFFERENT_FILE)`, inside the branch and
/// before `saveStatus` is read (`start_menu.c:1093-1096`) -- so a *failed*
/// overwrite retires the WARNING just as a successful one does, and the
/// next SAVE in that session meets `gText_AlreadySavedFile` instead.
///
/// Driven through the real `PhaseSaveTarget` (not the unit tests' fake) on
/// the one failure this port can stage honestly: the unprompted
/// empty-cartridge write meeting a save another instance left on disk
/// since this session booted (`SaveSlot::store_unless_foreign_save`).
/// Nothing is lost by clearing the flag there -- the retry is still
/// prompted, just by the ordinary overwrite question.
///
/// Staged as a real session is (#232 review round two): the boot load runs
/// *first*, against the absent file, so this session carries the save
/// identity that load established (`SAVE_STATUS_EMPTY`, counter 0) rather
/// than the `None` a fixture that skips `SaveSlot::load` would leave. That
/// identity is what makes the foreign save appearing afterwards read as
/// *newer persisted progress*, so both of this session's writes are refused
/// -- the first as stale before its foreign-save guard is even reached, the
/// retry as stale too -- and the other instance's file survives
/// byte-identical. That is the point of the test: the WARNING is retired by
/// the dispatch, the retry is asked the ordinary question, and nothing is
/// clobbered without the player having answered one. Whether the retry
/// *lands* is the stale-session policy's call
/// (`SaveSlot::store`'s docs), not this flow's.
#[test]
fn a_refused_overwrite_retires_the_warning_and_the_retry_is_still_prompted() {
    let temp = TempSave::new("refused-overwrite");

    // This session boots first, against a path with no save on it at all:
    // the main menu offers NEW GAME only, and the load still establishes
    // the session's save identity.
    let mut slot = temp.slot();
    assert!(
        !slot.load().status.menu_shows_continue(),
        "the boot load must see nothing -- that is what makes the first \
         SAVE take the empty-cartridge shortcut"
    );

    // *Then* someone else's save appears at the path, written by its own
    // session while this one was in the field.
    let mut first_slot = temp.slot();
    let mut original_phase = new_game_phase();
    play_a_bit(&mut original_phase);
    save_from_the_start_menu(&mut original_phase, &mut first_slot);
    let original = std::fs::read(temp.path()).unwrap();

    // The new-game session's first SAVE: its boot load saw nothing
    // (`SAVE_STATUS_EMPTY`), which with `gDifferentSaveFile` is upstream's
    // empty-cartridge shortcut, so it asks only `gText_ConfirmSave`.
    let mut phase = new_game_phase();
    play_a_bit(&mut phase);
    assert!(phase.different_save_file());
    phase.open_synthetic_start_menu();
    let prompts = drive_start_menu(&mut phase, &mut slot, &[]);
    assert_eq!(
        prompts,
        vec![0],
        "the empty-cartridge shortcut asks nothing about overwriting"
    );
    assert_eq!(
        std::fs::read(temp.path()).unwrap(),
        original,
        "the unprompted write must have been refused, file untouched"
    );
    assert!(
        phase.start_menu().is_none(),
        "gText_SaveError still closes the menu"
    );
    assert!(
        !phase.different_save_file(),
        "the dispatch cleared gDifferentSaveFile, the status did not"
    );

    // The retry: the ordinary overwrite question (default YES), not the
    // WARNING (default NO) all over again.
    phase.open_synthetic_start_menu();
    let prompts = drive_start_menu(&mut phase, &mut slot, &[]);
    assert_eq!(
        prompts,
        vec![0, 0],
        "gText_ConfirmSave then gText_AlreadySavedFile, both defaulting to YES"
    );
    assert_eq!(
        std::fs::read(temp.path()).unwrap(),
        original,
        "the retry is refused as stale -- the file that appeared after this \
         session's boot load is newer progress -- and must survive it \
         byte-identical"
    );
    assert!(
        temp.slot().load().status.menu_shows_continue(),
        "the other instance's save is still the one on disk"
    );
}

/// #232 review round two -- the leak the previous round's fix opened up.
///
/// `gDifferentSaveFile` answers exactly one question ("must the next SAVE
/// still show the WARNING") and is retired by the first overwrite
/// *dispatch*, failed ones included (`start_menu.c:1093-1096`). So it must
/// not be what decides whose bytes a write may keep: a new-game session
/// whose first overwrite fails retries as `SAVE_NORMAL`, and if that arm
/// kept the replaced trainer's payload as its serialization base, the retry
/// would write their deferred state -- play time, options, Pokédex,
/// ciphertext under an encryption key this session discarded -- back out
/// under the new game's checksums, which
/// [`engine::save::SaveStore::clear_base`]'s contract forbids.
///
/// Upstream cannot reach that state at all: `NewGameInitData` settles what
/// a new game's `gSaveBlock1/2` hold (`src/new_game.c:149-186`, `ClearSav1`
/// at `:160`) and `HandleSavingData` writes a whole slot out of that RAM
/// (`src/save.c:736-739`), so a `SAVE_NORMAL` retry there is already free
/// of the replaced file. This port defers unmodelled bytes to the image on
/// disk, so it re-applies that reset per write -- keyed on the *session*
/// (`OverworldPhase::save_lineage`), never on the save mode.
///
/// The first overwrite is failed here by a medium that cannot read the
/// image back (`SaveFile::read`'s error path, which `store_impl`
/// propagates before touching a byte). Any of the I/O failures upstream
/// collapses into `SAVE_STATUS_ERROR` has the same shape: `gText_SaveError`,
/// and `gDifferentSaveFile` cleared regardless. The failure is transient --
/// the foreign image is put back before the retry, as a medium that came
/// back would leave it.
#[test]
fn a_new_game_retry_after_a_failed_overwrite_keeps_none_of_the_replaced_saves_bytes() {
    use engine::save::{SaveBlock2, Sector, SECTOR_SIGNATURE, SECTOR_SIZE};

    const SECTOR_ID_SAVEBLOCK2: u16 = 0;
    // A SaveBlock2 payload offset no field models yet (play-time region),
    // so only the retained base can carry it forward -- the same probe
    // `game_save::tests`' deferred-byte cases use.
    const B2_DEFERRED: usize = 0x10;

    let temp = TempSave::new("newgame-retry-deferred");

    // The replaced adventure: a real save, written by its own session.
    let mut first_slot = temp.slot();
    let mut original_phase = new_game_phase();
    play_a_bit(&mut original_phase);
    save_from_the_start_menu(&mut original_phase, &mut first_slot);

    // Give it a recognizable deferred byte, re-signing the sector so the
    // image stays a perfectly ordinary, checksum-valid save.
    let mut image = std::fs::read(temp.path()).unwrap();
    let mut planted = false;
    for index in 0..image.len() / SECTOR_SIZE {
        let start = index * SECTOR_SIZE;
        let sector = Sector::from_bytes(image[start..start + SECTOR_SIZE].try_into().unwrap());
        if sector.signature() == SECTOR_SIGNATURE && sector.id() == SECTOR_ID_SAVEBLOCK2 {
            let mut payload = sector.data()[..SaveBlock2::PAYLOAD_LEN].to_vec();
            payload[B2_DEFERRED] = 0x5A;
            let replacement = Sector::write(SECTOR_ID_SAVEBLOCK2, &payload, sector.counter());
            image[start..start + SECTOR_SIZE].copy_from_slice(replacement.as_bytes());
            planted = true;
        }
    }
    assert!(planted, "the fixture must plant one deferred byte");
    std::fs::write(temp.path(), &image).unwrap();
    let foreign = std::fs::read(temp.path()).unwrap();

    // A second session boots against that save -- the menu offers CONTINUE
    // -- and the player picks NEW GAME.
    let mut slot = temp.slot();
    assert!(slot.load().status.menu_shows_continue());
    let mut phase = new_game_phase();
    play_a_bit(&mut phase);

    // The medium goes away under the first overwrite: the WARNING is shown
    // and answered YES, and the write fails at the read-back.
    std::fs::write(temp.path(), b"the medium hiccupped").unwrap();
    phase.open_synthetic_start_menu();
    let prompts = drive_start_menu(&mut phase, &mut slot, &[true, true]);
    assert_eq!(
        prompts,
        vec![0, 1],
        "gText_ConfirmSave defaults to YES; the WARNING defaults to NO"
    );
    assert!(
        !phase.different_save_file(),
        "the dispatch retires the WARNING even though the write failed"
    );

    // The medium comes back, still holding the other trainer's save, and
    // the player saves again -- now asked the ordinary overwrite question.
    std::fs::write(temp.path(), &foreign).unwrap();
    phase.open_synthetic_start_menu();
    let prompts = drive_start_menu(&mut phase, &mut slot, &[]);
    assert_eq!(
        prompts,
        vec![0, 0],
        "gText_ConfirmSave then gText_AlreadySavedFile, both defaulting to YES"
    );
    let after = std::fs::read(temp.path()).unwrap();
    assert_ne!(after, foreign, "the answered question let the write land");

    // The write this session just made must carry none of the replaced
    // trainer's deferred bytes -- it is still a new game's write, whichever
    // `TrySavingData` arm the retired WARNING routed it through.
    let mut checked = 0;
    for index in 0..after.len() / SECTOR_SIZE {
        let start = index * SECTOR_SIZE;
        let sector = Sector::from_bytes(after[start..start + SECTOR_SIZE].try_into().unwrap());
        if sector.signature() == SECTOR_SIGNATURE
            && sector.id() == SECTOR_ID_SAVEBLOCK2
            && sector.counter() == 2
        {
            assert_eq!(
                sector.data()[B2_DEFERRED],
                0,
                "the retry wrote the replaced adventure's deferred bytes \
                 back out -- clear_base must follow the session, not the \
                 save mode"
            );
            checked += 1;
        }
    }
    assert_eq!(
        checked, 1,
        "the fixture must find exactly the sector this session's write produced"
    );

    // And the save that reloads is this session's new game, not the one it
    // replaced.
    let saved = slot.load();
    assert!(saved.status.menu_shows_continue());
    assert_eq!(saved.block1.money, 1_234, "play_a_bit's own state");
}

/// `saved_tile_placement`'s one substitution, pinned (issue #214 review):
/// a save standing on an `ELEVATION_MULTI_LEVEL` (15) tile resumes at
/// `ELEVATION_TRANSITION`, exactly the `ObjectEventUpdateElevation`
/// behaviour the warp path already applies -- never at the raw 15, which
/// no walking player can legitimately hold. The ordinary-tile case (the
/// fixture's uniform elevation 3) is pinned by the round-trip snapshot.
#[test]
fn continue_on_a_multi_level_tile_resumes_at_the_transition_elevation() {
    use engine::overworld::{ELEVATION_MULTI_LEVEL, ELEVATION_TRANSITION};

    let bridge_tile = (4_u16, 4_u16);
    let scene = crate::overworld::tests::synthetic_scene_with_cell_elevation(
        10,
        10,
        bridge_tile,
        ELEVATION_MULTI_LEVEL,
    );
    let mut block1 = SaveBlock1::default();
    block1.pos.x = i16::try_from(bridge_tile.0).unwrap();
    block1.pos.y = i16::try_from(bridge_tile.1).unwrap();

    let resumed =
        OverworldPhase::from_saved(scene, new_game::SPAWN_MAP_ID, block1, SaveBlock2::default());
    assert_eq!(
        resumed.player.elevation(),
        ELEVATION_TRANSITION,
        "a multi-level tile must resume as a transition, not as raw {ELEVATION_MULTI_LEVEL}"
    );
}

/// A save block whose player object event was never written -- a zeroed
/// [`SaveBlock1`], or an image from before issue #232 modelled the field --
/// holds `DIR_NONE`, which is no walking direction at all. The continue
/// must fall back to the tile-derived `GetAdjustedInitialDirection`
/// (`DIR_SOUTH` on an ordinary tile) rather than face an arbitrary way.
#[test]
fn a_save_with_no_recorded_facing_falls_back_to_the_tile_derived_direction() {
    let block1 = SaveBlock1::default();
    assert_eq!(
        block1.player_object_event.facing_direction, 0,
        "a zeroed block holds DIR_NONE"
    );
    let resumed = OverworldPhase::from_saved(
        crate::overworld::tests::synthetic_scene(10, 10),
        new_game::SPAWN_MAP_ID,
        block1,
        SaveBlock2::default(),
    );
    assert_eq!(resumed.player.facing(), Direction::South);
}

/// A save written before issue #261 had no writer for `last_heal_location`
/// at all, so every such image carries the zeroed [`WarpData::default`] --
/// which *resolves* (group 0/num 0 is a real generated-table entry), so
/// without migration the first white-out of an upgraded save would warp to
/// Petalburg City at `(0, 0)` instead of home (issue #261 review). The
/// continue must adopt the same gender default a fresh game gets, and must
/// leave a genuinely written heal location alone.
#[test]
fn a_save_with_a_legacy_zeroed_heal_location_adopts_the_gender_default() {
    let block1 = SaveBlock1::default();
    assert_eq!(
        block1.last_heal_location,
        WarpData::default(),
        "a zeroed block holds the legacy marker"
    );
    let resumed = OverworldPhase::from_saved(
        crate::overworld::tests::synthetic_scene(10, 10),
        new_game::SPAWN_MAP_ID,
        block1,
        SaveBlock2::default(),
    );
    assert_eq!(
        resumed.save1.last_heal_location,
        new_game::default_last_heal_location(SaveBlock2::default().player_gender),
        "the legacy all-zero value migrates to the gender default"
    );

    // A modern save's genuinely written value survives untouched.
    let written = WarpData {
        map_group: 0,
        map_num: 9,
        warp_id: -1,
        x: 6,
        y: 8,
    };
    let block1 = SaveBlock1 {
        last_heal_location: written,
        ..SaveBlock1::default()
    };
    let resumed = OverworldPhase::from_saved(
        crate::overworld::tests::synthetic_scene(10, 10),
        new_game::SPAWN_MAP_ID,
        block1,
        SaveBlock2::default(),
    );
    assert_eq!(resumed.save1.last_heal_location, written);
}

/// A save carrying a *fainted* lead -- something only a build between
/// issues #261 and #251 could serialize: a lost Route 101 first battle
/// returned the player to the field fainted (`CB2_EndFirstBattle` has no
/// `IsPlayerDefeated` branch) and the start menu saved it, while the
/// first-battle conclusion now heals every outcome the frame the battle
/// ends and upstream cannot save a party-wide faint at all -- is healed on
/// continue (PR #291 review). Without the migration, every eligible grass
/// step on such a save would spend the encounter and wild-mon RNG draws
/// before `Battle::new` refused the fainted battler: repeatable draws with
/// no upstream counterpart. A merely *damaged* lead is no legacy marker
/// and must round-trip untouched -- the second half pins that boundary.
#[test]
fn a_save_with_a_fainted_lead_is_healed_on_continue() {
    let mut fainted = new_game::provisional_starter();
    fainted.apply_damage(u32::MAX);
    assert!(fainted.is_fainted(), "setup: the lead must start fainted");
    let mut seed = new_game_phase();
    seed.save1.player_party_count = 1;
    seed.save1.player_party[0] = crate::party::to_save_pokemon(&battle::Dex::new(), &fainted);

    let resumed = OverworldPhase::from_saved(
        crate::overworld::tests::synthetic_scene(10, 10),
        seed.map_id,
        seed.save1,
        seed.save2,
    );
    let lead = resumed
        .party_lead
        .as_ref()
        .expect("the migrated save still has its lead");
    assert!(
        !lead.is_fainted(),
        "a fainted lead from a pre-#251 save is healed on load"
    );

    // The boundary: a damaged-but-standing lead is genuine mid-playthrough
    // state, not the legacy marker, and keeps its spent HP.
    let damaged = a_damaged_lead();
    let expected_hp = damaged.current_hp();
    let mut seed = new_game_phase();
    seed.save1.player_party_count = 1;
    seed.save1.player_party[0] = crate::party::to_save_pokemon(&battle::Dex::new(), &damaged);

    let resumed = OverworldPhase::from_saved(
        crate::overworld::tests::synthetic_scene(10, 10),
        seed.map_id,
        seed.save1,
        seed.save2,
    );
    assert_eq!(
        resumed
            .party_lead
            .as_ref()
            .expect("the damaged save still has its lead")
            .current_hp(),
        expected_hp,
        "a standing lead's spent HP survives the continue untouched"
    );
}

/// A save whose party count is zero resumes with no lead at all -- not
/// with a fabricated [`new_game::provisional_starter`], which is what the
/// #214 slice did and what #232 removed.
#[test]
fn a_save_with_an_empty_party_resumes_with_no_lead() {
    let resumed = OverworldPhase::from_saved(
        crate::overworld::tests::synthetic_scene(10, 10),
        new_game::SPAWN_MAP_ID,
        SaveBlock1::default(),
        SaveBlock2::default(),
    );
    assert!(resumed.party_lead.is_none());
}
