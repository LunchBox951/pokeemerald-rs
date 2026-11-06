//! The main menu's A confirmation: `NEW GAME` and `OPTION` fade-waits, their
//! deferred dispatch, and the fade-wait frame's palette composition.

use super::tests::{drive_through_fade_wait, empty_slot, pressed, write_empty_scratch_pack};
use super::{advance_scene, AppScene, MainMenuState};
use crate::main_menu::{MainMenuItem, MainMenuScene, MainMenuType};
use platform::{ButtonState, Buttons};

/// I-3 scene-flow test: main menu, `NEW GAME` selected (the default,
/// per `crate::main_menu`'s module docs) and A newly pressed -> intro, only
/// once the I-3 (issue #1329) black fade-wait state reports done.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn main_menu_confirm_on_new_game_transitions_to_intro() {
    let (_temp, mut save_slot) = empty_slot("confirm-new-game");
    let menu = MainMenuScene::from_pack(
        &assets::pack::AssetPack::load_repo().expect("run `cargo xtask extract` first"),
        MainMenuType::NoSavedGame,
    )
    .expect("the checkout pack builds the main menu");
    let scene = AppScene::MainMenu(Box::new(MainMenuState {
        scene: menu,
        saved: save_slot.load(),
    }));

    let (waiting, press_frame) = advance_scene(
        scene,
        pressed(Buttons::A),
        &mut save_slot,
        crate::pack_source::PackSource::Repo,
    );
    assert!(
        matches!(waiting, AppScene::MainMenuFadeWait(_)),
        "A on NEW GAME must first enter the black fade-wait state"
    );

    let (next, wait_frames, _destination_frame) = drive_through_fade_wait(
        waiting,
        &mut save_slot,
        &crate::pack_source::PackSource::Repo,
    );
    assert!(
        wait_frames.iter().any(|frame| **frame != *press_frame),
        "the retained menu frame must visibly fade toward black while waiting"
    );

    assert!(
        matches!(next, AppScene::Intro(_)),
        "A on NEW GAME must transition to the intro once the fade completes"
    );
}

/// A on `OPTION` is `ACTION_OPTION` (`main_menu.c:1070-1073`),
/// so it fades out like the other items, never launches the intro, and --
/// when the Options screen cannot load -- falls back to the retained menu on
/// `OPTION` once the fade finishes. The entryless scratch pack makes the load
/// fail without a local pack.
#[test]
fn main_menu_confirm_on_option_fades_then_falls_back_when_the_screen_cannot_load() {
    let (_temp, mut save_slot) = empty_slot("confirm-option");
    let mut menu = crate::main_menu::synthetic_scene(MainMenuType::NoSavedGame);
    menu.move_down();
    assert_eq!(menu.selected(), MainMenuItem::Option);
    let scene = AppScene::MainMenu(Box::new(MainMenuState {
        scene: menu,
        saved: save_slot.load(),
    }));
    let pack_path = write_empty_scratch_pack("confirm-option");
    let leaked_path: &'static std::path::Path = Box::leak(pack_path.clone().into_boxed_path());
    let pack_source = crate::pack_source::PackSource::Test(leaked_path);

    let (scene, _frame) = advance_scene(
        scene,
        pressed(Buttons::A),
        &mut save_slot,
        pack_source.clone(),
    );
    assert!(
        matches!(scene, AppScene::MainMenuFadeWait(_)),
        "A on OPTION must begin the black fade like the other items"
    );

    let (next, _, _) = drive_through_fade_wait(scene, &mut save_slot, &pack_source);
    drop(std::fs::remove_file(&pack_path));
    let AppScene::MainMenu(state) = next else {
        panic!("a failed Options load must fall back to the main menu");
    };
    assert_eq!(state.scene.selected(), MainMenuItem::Option);
}

/// I-3 regression (issue #1329): a fresh A on `NEW GAME` must enter
/// `AppScene::MainMenuFadeWait` and ignore every button until the fade
/// reports done, only then dispatching `intro::load`. Pack-free via
/// `PackSource::Test` pointed at an entryless scratch pack, so the deferred
/// dispatch still runs (and fails, falling back to the retained menu)
/// without needing a real one.
#[test]
fn main_menu_confirm_on_new_game_waits_for_the_fade_before_dispatching() {
    let (_temp, mut save_slot) = empty_slot("new-game-fade-wait");
    let menu = crate::main_menu::synthetic_scene(MainMenuType::NoSavedGame);
    assert_eq!(menu.selected(), MainMenuItem::NewGame);
    let scene = AppScene::MainMenu(Box::new(MainMenuState {
        scene: menu,
        saved: save_slot.load(),
    }));

    let pack_path = write_empty_scratch_pack("new-game-fade-wait");
    // `PackSource::Test` needs a `'static` path -- leaked once, reclaimed
    // only when the test process exits, matching
    // `overworld_phase::step_tests`' identical `PackSource::Test` fixture.
    let leaked_path: &'static std::path::Path = Box::leak(pack_path.clone().into_boxed_path());
    let pack_source = crate::pack_source::PackSource::Test(leaked_path);

    let (mut scene, press_frame) = advance_scene(
        scene,
        pressed(Buttons::A),
        &mut save_slot,
        pack_source.clone(),
    );
    assert!(
        matches!(scene, AppScene::MainMenuFadeWait(_)),
        "A on NEW GAME must enter the fade-wait state on the press frame, not dispatch immediately"
    );

    // The press frame runs both of upstream's updates at coefficient 0, so
    // the very next frame already blends at coefficient 2.
    let (next, first_wait_frame) = advance_scene(
        scene,
        ButtonState::new(),
        &mut save_slot,
        pack_source.clone(),
    );
    assert_ne!(
        *first_wait_frame, *press_frame,
        "the frame after the press must already show the first fade step, not a second \
         coefficient-0 frame"
    );
    scene = next;

    // 19 further frames, all still `Active`, with deliberately distracting
    // input every frame -- none of it may resume the selection or retrigger
    // the dispatch while waiting.
    for buttons in [
        pressed(Buttons::UP),
        pressed(Buttons::DOWN),
        pressed(Buttons::A),
        ButtonState::new(),
    ]
    .into_iter()
    .cycle()
    .take(19)
    {
        let (next, _frame) = advance_scene(scene, buttons, &mut save_slot, pack_source.clone());
        assert!(
            matches!(next, AppScene::MainMenuFadeWait(_)),
            "must keep waiting until the fade reports done, ignoring input meanwhile"
        );
        scene = next;
    }

    // The 21st frame after the press is the one whose update reports done:
    // it still presents the fully black retained menu rather than the
    // destination, as upstream's task only observes the finished fade on
    // the next frame.
    let (done, done_frame) = advance_scene(
        scene,
        ButtonState::new(),
        &mut save_slot,
        pack_source.clone(),
    );
    assert!(
        matches!(done, AppScene::MainMenuFadeWait(_)),
        "the frame the fade reports done must still present the retained frame, not dispatch"
    );
    assert!(
        done_frame.iter().all(|&pixel| pixel == 0),
        "the done frame must present the fully black fade"
    );
    scene = done;

    // The next frame dispatches exactly once, fails against the entryless
    // pack, and falls back to the retained menu -- still selecting
    // `NEW GAME`, proving the UP/DOWN presses above were ignored, not
    // merely unobserved.
    let (after, _frame) = advance_scene(scene, ButtonState::new(), &mut save_slot, pack_source);
    drop(std::fs::remove_file(&pack_path));
    let AppScene::MainMenu(state) = after else {
        panic!("a failed dispatch must fall back to the retained main menu, not stay waiting");
    };
    assert_eq!(
        state.scene.selected(),
        MainMenuItem::NewGame,
        "input pressed while waiting must not have moved the selection"
    );
}

/// The first fade-wait frame composes the menu from palettes already faded
/// toward black, so the highlight darkening acts on the faded colours: fading
/// the already darkened pixel would land elsewhere.
#[test]
fn main_menu_fade_wait_frame_fades_palettes_before_the_highlight_darkening() {
    let (_temp, mut save_slot) = empty_slot("menu-fade-wait-recompose");
    let menu = crate::main_menu::synthetic_scene(MainMenuType::NoSavedGame);
    let scene = AppScene::MainMenu(Box::new(MainMenuState {
        scene: menu,
        saved: save_slot.load(),
    }));
    let pack_path = write_empty_scratch_pack("menu-fade-wait-recompose");
    let leaked_path: &'static std::path::Path = Box::leak(pack_path.clone().into_boxed_path());
    let pack_source = crate::pack_source::PackSource::Test(leaked_path);

    let (scene, _press) = advance_scene(
        scene,
        pressed(Buttons::A),
        &mut save_slot,
        pack_source.clone(),
    );
    let (_scene, frame) = advance_scene(scene, ButtonState::new(), &mut save_slot, pack_source);
    drop(std::fs::remove_file(&pack_path));

    // Coefficient 2 of the 5-bit black blend: white 31 -> 31 + (-31 * 2 >> 4)
    // = 27, expanded to 222.
    let faded_white: u8 = 222;
    let pixel = |x: usize, y: usize| frame[y * 240 + x];
    assert_eq!(
        pixel(18, 10),
        u32::from_be_bytes([0, faded_white, faded_white, faded_white]),
        "selected header window"
    );
    let darkened = rendering::darken(
        rendering::Rgb888 {
            r: faded_white,
            g: faded_white,
            b: faded_white,
        },
        7,
    );
    assert_eq!(
        pixel(18, 42),
        u32::from_be_bytes([0, darkened.r, darkened.g, darkened.b]),
        "unselected header window darkens the faded colour"
    );
}
