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
//!   `crate::flow::overworld_phase::tests` already walks the player around),
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
use engine::save::{SaveBlock1, SaveBlock2};
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
fn save_from_the_start_menu(phase: &mut OverworldPhase, save_slot: &mut SaveSlot) -> Vec<u8> {
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
    let map = saved_map_id(&loaded.block1).expect("the saved location must resolve");
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
