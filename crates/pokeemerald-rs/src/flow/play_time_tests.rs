//! I-6 saved play time through the whole flow: which scenes tick it, and
//! that a save carries what elapsed (`PlayTimeCounter_Update`,
//! `pokeemerald/src/main.c:165`). The phase-level wiring is pinned in
//! `overworld_phase::play_time_tests`; the counter's own rules in
//! `engine::save::play_time`.

use super::overworld_phase::OverworldPhase;
use super::save_continue_support::{
    new_game_phase, save_from_the_start_menu, SAVE_FLOW_FRAME_BUDGET,
};
use super::tests::{held, pressed, TempSave};
use super::{advance_scene, AnimatedTitle, AppScene, MainMenuState};
use crate::game_save::{SaveFileStatus, SaveSlot, SavedGame};
use crate::main_menu::{synthetic_scene, MainMenuType};
use crate::new_game;
use crate::pack_source::PackSource;
use crate::start_menu::{StartMenu, StartMenuItem};
use crate::title::TitleScene;
use engine::save::{PlayTime, SaveBlock1, SaveBlock2};
use platform::Buttons;

const LOADED: PlayTime = PlayTime {
    hours: 2,
    minutes: 3,
    seconds: 4,
    vblanks: 55,
};

/// The time as a vblank count, so "plus exactly N" is plain addition.
fn vblanks(time: PlayTime) -> u64 {
    ((u64::from(time.hours) * 60 + u64::from(time.minutes)) * 60 + u64::from(time.seconds)) * 60
        + u64::from(time.vblanks)
}

fn saved_with(time: PlayTime) -> SavedGame {
    let mut saved = SavedGame::defaulted(SaveFileStatus::Ok);
    saved.block2.play_time = time;
    saved
}

/// A continued session on the synthetic room, as the main menu's CONTINUE
/// builds one minus the pack load.
fn continued(block1: SaveBlock1, block2: SaveBlock2) -> OverworldPhase {
    let map = new_game::SPAWN_MAP_ID;
    OverworldPhase::from_saved(
        crate::overworld::test_support::synthetic_scene(10, 10).in_room_for_test(map),
        map,
        block1,
        block2,
    )
}

fn carried_time(scene: &AppScene) -> PlayTime {
    match scene {
        AppScene::Title(title) => title.saved.block2.play_time,
        AppScene::MainMenu(state) | AppScene::MainMenuHighlight(state) => {
            state.saved.block2.play_time
        }
        AppScene::Overworld(phase) => phase.save2().play_time,
        _ => panic!("the fixture only drives the title, the main menu and the overworld"),
    }
}

#[test]
fn title_frames_do_not_tick_the_loaded_time() {
    let mut slot = SaveSlot::disabled();
    let mut scene = AppScene::Title(Box::new(AnimatedTitle {
        presented: true,
        ..AnimatedTitle::new(TitleScene::blank(), saved_with(LOADED))
    }));
    for _ in 0..180 {
        scene = advance_scene(scene, held(Buttons::NONE), &mut slot, PackSource::Runtime).0;
        assert!(matches!(scene, AppScene::Title(_)));
        assert_eq!(carried_time(&scene), LOADED);
    }
}

#[test]
fn main_menu_frames_do_not_tick_the_loaded_time() {
    let mut slot = SaveSlot::disabled();
    let mut scene = AppScene::MainMenu(Box::new(MainMenuState {
        scene: synthetic_scene(MainMenuType::SavedGame),
        saved: saved_with(LOADED),
    }));
    for frame in 0..180 {
        // Down then up, so highlight frames are covered as well as idle ones.
        let buttons = match frame % 3 {
            0 => pressed(Buttons::DOWN),
            1 => pressed(Buttons::UP),
            _ => held(Buttons::NONE),
        };
        scene = advance_scene(scene, buttons, &mut slot, PackSource::Runtime).0;
        assert_eq!(carried_time(&scene), LOADED);
    }
}

#[test]
fn field_and_start_menu_frames_tick_through_advance_scene() {
    let mut slot = SaveSlot::disabled();
    let block2 = SaveBlock2 {
        play_time: LOADED,
        ..SaveBlock2::default()
    };
    let mut scene = AppScene::Overworld(Box::new(continued(SaveBlock1::default(), block2)));
    let entered = vblanks(carried_time(&scene));
    assert_eq!(entered, vblanks(LOADED) + 1, "the entry iteration ticks");
    for frame in 1..=60 {
        scene = advance_scene(scene, held(Buttons::NONE), &mut slot, PackSource::Runtime).0;
        assert_eq!(vblanks(carried_time(&scene)), entered + frame);
    }
    let AppScene::Overworld(mut phase) = scene else {
        unreachable!("the overworld stays the overworld");
    };
    phase.open_synthetic_start_menu();
    let mut scene = AppScene::Overworld(phase);
    for frame in 61..=90 {
        scene = advance_scene(scene, held(Buttons::NONE), &mut slot, PackSource::Runtime).0;
        assert_eq!(vblanks(carried_time(&scene)), entered + frame);
    }
}

/// Drive the already-open start menu, answering YES to every prompt, until
/// the save file's bytes change, and return how many frames that took
/// including the one that wrote.
fn frames_until_the_file_is_written(
    phase: &mut OverworldPhase,
    slot: &mut SaveSlot,
    path: &std::path::Path,
) -> u64 {
    let before = std::fs::read(path).ok();
    for frame in 1..=SAVE_FLOW_FRAME_BUDGET {
        let menu = phase.start_menu();
        let buttons = match menu.and_then(StartMenu::yes_no_cursor) {
            Some(1) => pressed(Buttons::UP),
            // BAG is the first item, so walk the cursor to SAVE before choosing.
            None if menu
                .is_some_and(|menu| !menu.saving() && menu.selected() != StartMenuItem::Save) =>
            {
                pressed(Buttons::DOWN)
            }
            _ => pressed(Buttons::A),
        };
        assert!(phase.advance_start_menu_frame(buttons, slot));
        if std::fs::read(path).ok() != before {
            return frame as u64;
        }
    }
    panic!("the save flow must write within {SAVE_FLOW_FRAME_BUDGET} frames");
}

/// The I-6 definition of done: N frames after Continue, a save and a reload
/// give the loaded time plus exactly N vblanks. N counts every game frame
/// after the continue's entry, the one that wrote the file included -- the
/// tick follows the save within a frame, so that frame's own vblank lands
/// after the write and the entry iteration's lands before it.
#[test]
fn a_save_n_frames_after_continue_reloads_the_loaded_time_plus_n_vblanks() {
    let temp = TempSave::new("play-time-save");
    let mut slot = temp.slot();

    // A first session leaves its play time on the cartridge, as an earlier play did.
    let mut first = new_game_phase();
    first.save2.play_time = LOADED;
    save_from_the_start_menu(&mut first, &mut slot);
    let loaded = slot.load();
    let on_disk = loaded.block2.play_time;
    assert!(
        vblanks(on_disk) > vblanks(LOADED),
        "the first save carries the time it ran for, {on_disk:?}"
    );

    let mut phase = continued(loaded.block1, loaded.block2);
    let mut idle_frames = 0;
    for _ in 0..137 {
        phase.step(held(Buttons::NONE));
        idle_frames += 1;
    }
    phase.open_synthetic_start_menu();
    let menu_frames = frames_until_the_file_is_written(&mut phase, &mut slot, temp.path());
    let frames = idle_frames + menu_frames;

    assert!(
        frames > 137 + 2,
        "the fixture must run a real save flow, not only idle frames"
    );
    let reloaded = temp.slot().load();
    assert_eq!(
        vblanks(reloaded.block2.play_time),
        vblanks(on_disk) + frames,
        "{:?} after {frames} frames",
        reloaded.block2.play_time
    );
    assert_eq!(
        vblanks(phase.save2().play_time),
        vblanks(reloaded.block2.play_time) + 1,
        "the writing frame's own vblank is counted straight after the save"
    );
}

/// A new game's first save carries only what elapsed since the 0:00:00 it
/// started at, not anything from the cartridge it overwrites.
#[test]
fn a_new_games_save_starts_from_zero() {
    let temp = TempSave::new("play-time-new-game");
    let mut slot = temp.slot();
    let mut phase = new_game_phase();
    let entered = vblanks(phase.save2().play_time);
    assert_eq!(entered, 1, "0:00:00 and the entry iteration's one vblank");
    for _ in 0..30 {
        phase.step(held(Buttons::NONE));
    }
    phase.open_synthetic_start_menu();
    let menu_frames = frames_until_the_file_is_written(&mut phase, &mut slot, temp.path());
    let reloaded = temp.slot().load();
    assert_eq!(
        vblanks(reloaded.block2.play_time),
        30 + menu_frames,
        "{:?}",
        reloaded.block2.play_time
    );
}
