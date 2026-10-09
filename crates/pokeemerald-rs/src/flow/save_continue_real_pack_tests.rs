//! `#[ignore]`d real-pack lane: continue-time room loading, flagged warps
//! and gender, which need the extracted asset pack.

use super::overworld_phase::OverworldPhase;
use super::save_continue_support::{
    blocks_with_divergent_continue_game_warp, drive_start_menu, new_game_phase, play_a_bit,
    save_from_the_start_menu, settle, snapshot,
};
use super::tests::{drive_through_fade_wait, pressed, TempSave};
use super::{menu_type_for, AppScene, MainMenuState};
use crate::main_menu::{MainMenuItem, MainMenuType};
use crate::new_game;
use engine::save::WarpData;
use platform::Buttons;

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
    // I-3, issue #1329: the press must first enter the black fade-wait
    // state, not hand off to the overworld on the press frame.
    let (waiting, _press_frame) = super::advance_scene(
        scene,
        pressed(Buttons::A),
        &mut slot,
        crate::pack_source::PackSource::Runtime,
    );
    assert!(
        matches!(waiting, AppScene::MainMenuFadeWait(_)),
        "A on CONTINUE must first enter the black fade-wait state"
    );
    let (next, _wait_frames, _destination_frame) =
        drive_through_fade_wait(waiting, &mut slot, &crate::pack_source::PackSource::Runtime);

    let AppScene::Overworld(resumed) = next else {
        panic!("A on CONTINUE must hand off to the overworld once the fade completes");
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

/// `WINDOW_FRAME_TYPE_5` and `OPTIONS_TEXT_SPEED_FAST` -- ordinary non-default
/// option choices, matching this module's other real-pack fixtures.
const NEW_GAME_OPTIONS_SAVED_WINDOW_FRAME: u8 = 5;
const NEW_GAME_OPTIONS_SAVED_TEXT_SPEED: u8 = 2;

/// A checksum-valid save whose `SaveBlock2` carries `text_speed`/
/// `window_frame` -- the two option fields issue #1125's NEW GAME regression
/// below needs a boot to recover.
fn write_save_with_new_game_options(temp: &TempSave, text_speed: u8, window_frame: u8) {
    let phase = new_game_phase();
    let (block1, mut block2) = (phase.save1.clone(), phase.save2.clone());
    block2.options_text_speed = text_speed;
    block2.options_window_frame_type = window_frame;

    let mut store = engine::save::SaveStore::new();
    store.save(&block1, &block2);
    engine::save::SaveFile::at(temp.path().to_path_buf())
        .write(&store)
        .expect("the fixture save must be writable");
}

/// The real end-to-end counterpart to `crate::flow::tests`' packless pin of
/// `new_game_options_for`'s status split (issue #1125; see that function's
/// own doc comment for the contract): confirms NEW GAME over an `Ok` save
/// really does carry its options through a real intro and overworld load,
/// which no packless test can exercise.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_new_game_over_a_saved_game_keeps_its_options() {
    let temp = TempSave::new("real-pack-new-game-options");
    write_save_with_new_game_options(
        &temp,
        NEW_GAME_OPTIONS_SAVED_TEXT_SPEED,
        NEW_GAME_OPTIONS_SAVED_WINDOW_FRAME,
    );
    let mut slot = temp.slot();

    let saved = slot.load();
    assert_eq!(
        menu_type_for(&saved),
        MainMenuType::SavedGame,
        "the fixture save must boot into the saved-game menu"
    );
    assert_eq!(
        super::window_frame_for(&saved),
        NEW_GAME_OPTIONS_SAVED_WINDOW_FRAME,
        "the boot half must already recover the saved window frame"
    );

    let mut menu = crate::main_menu::load_default_with_window_frame(
        MainMenuType::SavedGame,
        NEW_GAME_OPTIONS_SAVED_WINDOW_FRAME,
    )
    .expect("run `cargo xtask extract` first");
    menu.move_down();
    assert_eq!(
        menu.selected(),
        MainMenuItem::NewGame,
        "NEW GAME sits directly below CONTINUE on a saved-game menu"
    );

    let mut scene = AppScene::MainMenu(Box::new(MainMenuState { scene: menu, saved }));
    let mut started = None;
    for _ in 0..20_000 {
        let (next, _frame) = super::advance_scene(
            scene,
            pressed(Buttons::A),
            &mut slot,
            crate::pack_source::PackSource::Runtime,
        );
        assert!(
            !matches!(next, AppScene::OverworldLoadFailed(_)),
            "the overworld handoff must load: run `cargo xtask extract` first"
        );
        if let AppScene::Overworld(phase) = next {
            started = Some(phase);
            break;
        }
        scene = next;
    }
    let phase = started.expect("A on NEW GAME must reach the overworld through the intro");

    assert_eq!(
        phase.save2.options_text_speed, NEW_GAME_OPTIONS_SAVED_TEXT_SPEED,
        "NEW GAME over a recovered save keeps its optionsTextSpeed \
         (NewGameInitData never re-defaults the options)"
    );
    assert_eq!(
        phase.save2.options_window_frame_type, NEW_GAME_OPTIONS_SAVED_WINDOW_FRAME,
        "NEW GAME over a recovered save keeps its optionsWindowFrameType \
         (NewGameInitData never re-defaults the options)"
    );
}

/// The other pack-gated step (module docs): `START` really opening the
/// menu through `crate::start_menu::open`, with real chrome, and a
/// whole save running through the pack-decoded windows.
///
/// The press runs through [`crate::flow::advance_scene`] rather than
/// [`OverworldPhase::advance_start_menu_frame`], because a fresh `START`
/// reaches the menu only through that dispatch's no-menu branch and
/// [`OverworldPhase::step`]'s field-input ordering (issue #908); the
/// already-open frames below are `advance_start_menu_frame`'s own.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_start_opens_the_menu_and_saves() {
    let temp = TempSave::new("real-pack-start-menu");
    let mut slot = temp.slot();

    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
    settle(&mut phase);
    let (next, _frame) = super::advance_scene(
        AppScene::Overworld(Box::new(phase)),
        pressed(Buttons::START),
        &mut slot,
        crate::pack_source::PackSource::Runtime,
    );
    let AppScene::Overworld(mut phase) = next else {
        panic!("a START press must leave the overworld in place");
    };
    assert!(
        phase.start_menu().is_some(),
        "run `cargo xtask extract` first"
    );

    drive_start_menu(&mut phase, &mut slot, &[]);
    assert!(phase.start_menu().is_none());
    assert!(slot.load().status.menu_shows_continue());
}

/// The landing half of `UseContinueGameWarp`, on the real-pack lane (no
/// pack-free seam reaches a landed saved-location warp): a flagged save
/// resumes at `continue_game_warp` rather than `location`, the flag is gone,
/// temp field data is cleared, the supported on-transition effect (the
/// bedroom's decoration flags) ran, and the destination was loaded from the
/// pack exactly once.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_flagged_continue_lands_at_the_continue_game_warp() {
    const FLAG_TEMP_1: u16 = 0x1;
    let downstairs = assets::MapHeaderTable::new()
        .header(assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F"))
        .expect("the 1F map exists");
    let (mut block1, mut block2) = blocks_with_divergent_continue_game_warp(true);
    block1.location = WarpData {
        map_group: i8::try_from(downstairs.group).unwrap(),
        map_num: i8::try_from(downstairs.num).unwrap(),
        warp_id: -1,
        x: 3,
        y: 3,
    };
    block1.continue_game_warp = WarpData {
        warp_id: -1,
        x: i16::try_from(new_game::SPAWN_POSITION.0).unwrap(),
        y: i16::try_from(new_game::SPAWN_POSITION.1).unwrap(),
        ..block1.continue_game_warp
    };
    block2.player_gender = engine::save::PlayerGender::Male;
    block1.event_data.flag_set(FLAG_TEMP_1).unwrap();
    let decoration = assets::object_event_flags::DECORATION_FLAGS[0];
    assert!(!block1.event_data.flag_get(decoration).unwrap());

    let loads_before = crate::pack_source::pack_loads_on_this_thread();
    let phase = OverworldPhase::continue_saved_game(
        crate::pack_source::PackSource::Runtime,
        block1.clone(),
        block2,
    )
    .expect("run `cargo xtask extract` first");

    assert_eq!(
        crate::pack_source::pack_loads_on_this_thread() - loads_before,
        1,
        "a flagged continue loads its destination's pack exactly once"
    );
    assert_eq!(phase.map_id, new_game::SPAWN_MAP_ID);
    assert_eq!(phase.player.position(), new_game::SPAWN_POSITION);
    assert_eq!(phase.save1.location, block1.continue_game_warp);
    assert!(!phase.save2.continue_game_warp_pending());
    assert_eq!(phase.save2.special_save_warp_flags, 0x80);
    assert!(!phase.save1.event_data.flag_get(FLAG_TEMP_1).unwrap());
    assert!(phase.save1.event_data.flag_get(decoration).unwrap());
}

/// I-3 (#1648): both genders through the production `OverworldPhase::load`,
/// the start-menu save, and `continue_saved_game`. The selected identity is
/// explicit; the chooser UI is not involved.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn each_selected_gender_loads_saves_and_continues_into_its_own_room() {
    use crate::overworld::PlayerCharacter;

    for (gender, name, character, rival) in [
        (
            engine::save::PlayerGender::Male,
            "RED",
            PlayerCharacter::Brendan,
            super::route103_rival::Rival::May,
        ),
        (
            engine::save::PlayerGender::Female,
            "LEAF",
            PlayerCharacter::May,
            super::route103_rival::Rival::Brendan,
        ),
    ] {
        let identity = new_game::NewGameIdentity::new(name, gender).unwrap();
        let arrival = new_game::bedroom_arrival(gender);
        let mut phase = OverworldPhase::load(
            crate::pack_source::PackSource::Runtime,
            new_game::NewGameOptions::DEFAULT,
            identity,
        )
        .expect("run `cargo xtask extract` first");
        assert_eq!(phase.map_id, arrival.map_id);
        assert_eq!(phase.player.position(), arrival.position);
        assert_eq!(phase.save2().player_gender, gender);
        assert_eq!(
            super::route103_rival::Rival::for_gender(phase.save2().player_gender),
            Some(rival)
        );
        assert_eq!(character, PlayerCharacter::from(gender));

        let temp = TempSave::new(&format!("i3-gender-{name}"));
        let mut slot = temp.slot();
        save_from_the_start_menu(&mut phase, &mut slot);
        let saved = slot.load();
        assert_eq!(saved.block2.player_gender, gender);
        assert_eq!(
            engine::text::decode_to_string(&saved.block2.player_name).unwrap(),
            name
        );
        let resumed = OverworldPhase::continue_saved_game(
            crate::pack_source::PackSource::Runtime,
            saved.block1,
            saved.block2,
        )
        .unwrap_or_else(|_| panic!("continue must resume {name}'s save"));
        assert_eq!(resumed.map_id, arrival.map_id);
        assert_eq!(resumed.player.position(), arrival.position);
        assert_eq!(resumed.save2().player_gender, gender);
        assert_eq!(
            resumed.save1().last_heal_location,
            new_game::default_last_heal_location(gender)
        );
    }
}
