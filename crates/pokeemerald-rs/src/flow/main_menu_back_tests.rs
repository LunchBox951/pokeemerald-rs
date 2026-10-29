//! The main menu's B path back to the title (I-2): `Task_HandleMainMenuInput`
//! checks A, then B, then the D-pad (`pokeemerald/src/main_menu.c:888-925`);
//! B begins the white fade with `WIN0` covering the screen, and
//! `Task_HandleMainMenuBPressed` installs `CB2_InitTitleScreen` once the fade
//! is done (`:1109-1120`). The press frame is P0; as for A's fade, P21 is the
//! frame whose update reports done and P22 is the first the task observes it.

use super::tests::{held, pressed, TempSave};
use super::{advance_scene, AppScene, MainMenuState};
use crate::main_menu::{synthetic_scene, MainMenuItem, MainMenuType};
use crate::pack_source::{pack_loads_on_this_thread, PackSource};
use platform::{ButtonState, Buttons, Frame};

const WHITE: u32 = 0x00ff_ffff;
const BOTH_TYPES: [MainMenuType; 2] = [MainMenuType::NoSavedGame, MainMenuType::SavedGame];
/// P1..=P21: every frame between the press and the hand-off frame.
const WAIT_FRAMES_AFTER_PRESS: usize = 21;

fn menu(
    menu_type: MainMenuType,
    selected_downs: usize,
    save_slot: &mut crate::game_save::SaveSlot,
) -> AppScene {
    let mut scene = synthetic_scene(menu_type);
    for _ in 0..selected_downs {
        scene.move_down();
    }
    AppScene::MainMenu(Box::new(MainMenuState {
        scene,
        saved: save_slot.load(),
    }))
}

fn missing_pack() -> PackSource {
    PackSource::Explicit(std::path::Path::new("/nonexistent/birch/back-to-title.pack").into())
}

fn selected(scene: &AppScene) -> MainMenuItem {
    match scene {
        AppScene::MainMenu(state) | AppScene::MainMenuHighlight(state) => state.scene.selected(),
        AppScene::MainMenuFadeWait(wait) => wait.state.scene.selected(),
        _ => panic!("not a main menu scene"),
    }
}

/// B alone, and B with a direction that could move the selection, take the
/// cancel path on the press frame and leave the selection where it was.
#[test]
fn b_with_or_without_a_direction_begins_the_white_back_wait() {
    for menu_type in BOTH_TYPES {
        for keys in [
            Buttons::B,
            Buttons::B | Buttons::DOWN,
            Buttons::B | Buttons::UP,
            Buttons::B | Buttons::UP | Buttons::DOWN,
        ] {
            let temp = TempSave::new("b-back-wait");
            let mut slot = temp.slot();
            // One step down so both Up and Down could move.
            let scene = menu(menu_type, 1, &mut slot);
            let before = selected(&scene);
            let (next, _) = advance_scene(scene, pressed(keys), &mut slot, missing_pack());
            assert!(next.returns_to_title(), "{menu_type:?} {keys:?}");
            assert_eq!(selected(&next), before, "{menu_type:?} {keys:?}");
        }
    }
}

/// A held B (not a fresh press) is ordinary menu input.
#[test]
fn a_held_b_does_not_leave_the_menu() {
    for menu_type in BOTH_TYPES {
        let temp = TempSave::new("b-held");
        let mut slot = temp.slot();
        let scene = menu(menu_type, 0, &mut slot);
        let (next, _) = advance_scene(scene, held(Buttons::B), &mut slot, missing_pack());
        assert!(matches!(next, AppScene::MainMenu(_)), "{menu_type:?}");
    }
}

/// `main_menu.c:895` checks A first: A with B takes A's black confirm wait.
#[test]
fn a_wins_over_b() {
    for menu_type in BOTH_TYPES {
        for keys in [
            Buttons::A | Buttons::B,
            Buttons::A | Buttons::B | Buttons::DOWN,
        ] {
            let temp = TempSave::new("a-over-b");
            let mut slot = temp.slot();
            let scene = menu(menu_type, 0, &mut slot);
            let (confirm, confirm_frame) =
                advance_scene(scene, pressed(Buttons::A), &mut slot, missing_pack());
            let scene = menu(menu_type, 0, &mut slot);
            let (next, frame) = advance_scene(scene, pressed(keys), &mut slot, missing_pack());
            assert!(
                matches!(next, AppScene::MainMenuFadeWait(_)) && !next.returns_to_title(),
                "{menu_type:?} {keys:?} must take A's confirm wait"
            );
            assert_eq!(
                selected(&next),
                selected(&confirm),
                "{menu_type:?} {keys:?}"
            );
            assert_eq!(*frame, *confirm_frame, "{menu_type:?} {keys:?}");
        }
    }
}

fn pixel(frame: &Frame, x: usize, y: usize) -> u32 {
    frame[y * 240 + x]
}

/// `WIN0` opens over the whole screen on the press, so no window is darkened
/// from P0 onwards: both unselected and selected headers are plain white.
#[test]
fn full_screen_window_undarkens_every_menu_window_from_the_press_frame() {
    let temp = TempSave::new("b-win0");
    let mut slot = temp.slot();
    let scene = menu(MainMenuType::NoSavedGame, 0, &mut slot);
    let (mut scene, press) = advance_scene(scene, pressed(Buttons::B), &mut slot, missing_pack());
    assert_eq!(pixel(&press, 18, 10), WHITE, "selected header");
    assert_eq!(pixel(&press, 18, 42), WHITE, "unselected header");

    // P1: coefficient 2 of the white blend lifts the black backdrop to 24.
    let (next, first) = advance_scene(scene, ButtonState::new(), &mut slot, missing_pack());
    scene = next;
    assert_eq!(pixel(&first, 0, 0), 0x0018_1818);
    assert_eq!(pixel(&first, 18, 42), WHITE);

    // P2..=P21 end fully white, windows included.
    let mut last = first;
    for _ in 1..WAIT_FRAMES_AFTER_PRESS {
        let (next, frame) = advance_scene(scene, ButtonState::new(), &mut slot, missing_pack());
        scene = next;
        last = frame;
    }
    assert!(scene.returns_to_title());
    assert!(last.iter().all(|&p| p == WHITE), "terminal frame is white");
}

/// Input is ignored through the fade; the title load happens once, on P22,
/// and a failed load restores the menu without retrying every frame.
#[test]
fn back_wait_ignores_input_and_loads_the_title_once_on_the_frame_after_done() {
    let temp = TempSave::new("b-wait-gate");
    let mut slot = temp.slot();
    let scene = menu(MainMenuType::SavedGame, 1, &mut slot);
    let loads = pack_loads_on_this_thread();
    let (mut scene, _) = advance_scene(scene, pressed(Buttons::B), &mut slot, missing_pack());
    let distraction = [
        pressed(Buttons::A),
        pressed(Buttons::B),
        pressed(Buttons::UP),
        ButtonState::new(),
    ];
    for buttons in distraction
        .into_iter()
        .cycle()
        .take(WAIT_FRAMES_AFTER_PRESS)
    {
        let (next, _) = advance_scene(scene, buttons, &mut slot, missing_pack());
        assert!(next.returns_to_title(), "must keep waiting");
        scene = next;
    }
    assert_eq!(pack_loads_on_this_thread(), loads, "no load while fading");

    let (restored, _) = advance_scene(scene, ButtonState::new(), &mut slot, missing_pack());
    assert_eq!(pack_loads_on_this_thread(), loads + 1, "one load on P22");
    assert!(matches!(restored, AppScene::MainMenu(_)));
    assert_eq!(selected(&restored), MainMenuItem::NewGame);
    let (_, _) = advance_scene(restored, ButtonState::new(), &mut slot, missing_pack());
    assert_eq!(pack_loads_on_this_thread(), loads + 1, "no retry");
}

/// The session save survives the round trip: a failed menu rebuild keeps
/// the title's retained save for the next attempt (pack-free), and a
/// successful one reuses it instead of re-reading the disk.
#[test]
fn a_failed_menu_rebuild_keeps_the_retained_save() {
    let temp = TempSave::new("b-retained-failure");
    let mut slot = temp.slot();
    let mut retained = Some(Box::new(slot.load()));
    if let Some(saved) = retained.as_mut() {
        saved.block2.options_window_frame_type = 5;
    }
    let result = super::title_to_main_menu(&missing_pack(), &mut slot, &mut retained);
    assert!(result.is_none());
    assert_eq!(
        retained.map(|saved| saved.block2.options_window_frame_type),
        Some(5),
        "a failed rebuild must hand the live save back to the title"
    );
}

/// Frames `CB2_InitTitleScreen` runs before `MainCB2` (S0..S34).
const TITLE_INIT_FRAMES: usize = 35;

/// Steps the title initializer installed by the hand-off to its last frame
/// with nothing held, returning the settled title and its last frame.
fn through_title_init(
    mut scene: AppScene,
    slot: &mut crate::game_save::SaveSlot,
) -> (AppScene, Box<Frame>) {
    let mut frame = None;
    for step in 0..TITLE_INIT_FRAMES {
        assert!(matches!(scene, AppScene::TitleInit(_)), "S{step}");
        let (next, shown) = advance_scene(scene, ButtonState::new(), slot, PackSource::Repo);
        scene = next;
        frame = Some(shown);
    }
    (scene, frame.expect("the initializer ran"))
}

/// With the real pack the hand-off installs `CB2_InitTitleScreen`, whose
/// frames then settle on a fresh title at tick 0 that behaves as the boot
/// title does.
///
/// Needs the real pack, so it is `#[ignore]`d and runs in CI's `real-pack` job.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn b_returns_to_a_fresh_title_with_the_real_pack() {
    let reference = crate::title::load_repo().expect("run `cargo xtask extract` first");
    for menu_type in BOTH_TYPES {
        let temp = TempSave::new("b-real-title");
        let mut slot = temp.slot();
        let mut scene = menu(menu_type, 0, &mut slot);
        // A setting committed in OPTION before leaving the menu.
        if let AppScene::MainMenu(state) = &mut scene {
            state.saved.block2.options_window_frame_type = 5;
        }
        let (mut scene, _) = advance_scene(scene, pressed(Buttons::B), &mut slot, PackSource::Repo);
        for _ in 0..WAIT_FRAMES_AFTER_PRESS {
            let (next, _) = advance_scene(scene, ButtonState::new(), &mut slot, PackSource::Repo);
            assert!(next.returns_to_title());
            scene = next;
        }
        let (init, frame) = advance_scene(scene, ButtonState::new(), &mut slot, PackSource::Repo);
        assert!(
            matches!(init, AppScene::TitleInit(_)),
            "{menu_type:?}: P22 must install the title initializer"
        );
        assert!(frame.iter().all(|&p| p == WHITE), "{menu_type:?}: P22");
        let (title, frame) = through_title_init(init, &mut slot);
        let AppScene::Title(animated) = &title else {
            panic!("{menu_type:?}: S34 must install MainCB2");
        };
        assert_eq!((animated.tick, animated.presented), (0, false));
        assert_eq!(*frame, *reference.compose_frame(0));

        // B does not leave the settled title; a fresh A does.
        let (title, _) = advance_scene(title, pressed(Buttons::B), &mut slot, PackSource::Repo);
        assert!(matches!(title, AppScene::Title(_)));
        let (title, _) = advance_scene(title, pressed(Buttons::A), &mut slot, PackSource::Repo);
        assert!(matches!(title, AppScene::TitleFadeWait(_)));

        // Confirming the title reopens the menu with the same live save, not
        // the disk's.
        let (faded, _, _) =
            super::tests::drive_through_fade_wait(title, &mut slot, &PackSource::Repo);
        let (reopened, _) =
            super::tests::drive_through_menu_setup(faded, &mut slot, &PackSource::Repo);
        let AppScene::MainMenu(state) = reopened else {
            panic!("{menu_type:?}: the title must reopen the main menu");
        };
        assert_eq!(state.saved.block2.options_window_frame_type, 5);
    }
}

/// The returning title must honour the retained save's L=A.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn returning_title_honours_the_retained_l_equals_a() {
    let temp = TempSave::new("b-l-equals-a");
    let mut slot = temp.slot();
    let mut scene = menu(MainMenuType::NoSavedGame, 0, &mut slot);
    if let AppScene::MainMenu(state) = &mut scene {
        state.saved.block2.options_button_mode = engine::save::ButtonMode::LEqualsA;
    }
    let (mut scene, _) = advance_scene(scene, pressed(Buttons::B), &mut slot, PackSource::Repo);
    for _ in 0..WAIT_FRAMES_AFTER_PRESS {
        let (next, _) = advance_scene(scene, ButtonState::new(), &mut slot, PackSource::Repo);
        scene = next;
    }
    let (init, _) = advance_scene(scene, ButtonState::new(), &mut slot, PackSource::Repo);
    let (title, _) = through_title_init(init, &mut slot);
    assert!(
        matches!(title, AppScene::Title(_)),
        "the initializer must land on the title"
    );
    let (next, _) = advance_scene(title, pressed(Buttons::L), &mut slot, PackSource::Repo);
    assert!(
        matches!(next, AppScene::TitleFadeWait(_)),
        "a fresh L under the retained L=A must advance the title like A"
    );
}
