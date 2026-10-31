//! The main menu's setup frames after the Options screen returns (I-6):
//! upstream's `InitMainMenu(TRUE)` hands one task per frame from
//! the finished fade to the first input poll (`pokeemerald/src/main_menu.c:619-934`),
//! so the windows appear on the third frame and input counts from the fifth.
//! Frame F is the first one whose fade poll is already done.

use super::options_tests::{main_menu_state, option_menu_with_mode, Fixture};
use super::{advance_scene, AppScene};
use crate::frame::to_platform_frame;
use crate::main_menu::MainMenuItem;
use engine::save::ButtonMode;
use platform::{ButtonState, Buttons, Frame};
use rendering::{NormalPaletteFade, PaletteFadeTarget};

/// F+4: `Task_HandleMainMenuInput` first reads `JOY_NEW`.
pub(super) const FIRST_INPUT_FRAME: usize = 4;
pub(super) const NONE: Buttons = Buttons::NONE;
pub(super) const INPUT_BUTTONS: [Buttons; 4] = [Buttons::A, Buttons::UP, Buttons::DOWN, Buttons::B];

/// What `InitMainMenu(TRUE)` presents on F, F+1 (bare backdrop), F+2 (windows,
/// no highlight yet), F+3 (highlight), and F+4 onwards (ordinary input).
pub(super) const SETUP_PHASES: [&str; 5] = ["fade-in", "fade-in", "highlight", "menu", "menu"];

struct Return {
    backdrop: Box<Frame>,
    phases: Vec<&'static str>,
    frames: Vec<Box<Frame>>,
    scene: AppScene,
}

pub(super) fn phase(scene: &AppScene) -> &'static str {
    match scene {
        AppScene::MainMenuFadeIn(_) => "fade-in",
        AppScene::MainMenuHighlight(_) => "highlight",
        AppScene::MainMenu(_) => "menu",
        AppScene::MainMenuFadeWait(_) => "confirm",
        _ => "other",
    }
}

/// Opens Options from an intact save's `OPTION`, leaves with B, and drives the
/// production scene machine through the fade-in up to the frame before F, then
/// steps one frame per entry of `held`, starting at F.
fn return_from_options(f: &mut Fixture, held: &[Buttons]) -> Return {
    let mut buttons = ButtonState::new();
    let scene = option_menu_with_mode(f, ButtonMode::Normal);
    let backdrop = to_platform_frame(&main_menu_state(&scene).scene.compose_backdrop());
    let scene = f.open_options(scene, &mut buttons);
    let scene = f.step(scene, &mut buttons, Buttons::B);
    let mut scene = f.until(scene, &mut buttons, |s| {
        matches!(s, AppScene::MainMenuFadeIn(_))
    });
    // The first fade-in step begins the fade; each later one advances it.
    scene = f.step(scene, &mut buttons, NONE);
    let mut oracle = NormalPaletteFade::begin_in(PaletteFadeTarget::Black);
    while !oracle.is_done() {
        oracle.update();
        scene = f.step(scene, &mut buttons, NONE);
    }
    let (mut phases, mut frames) = (Vec::new(), Vec::new());
    for &press in held {
        buttons.update(press);
        let (next, frame) = advance_scene(scene, buttons, &mut f.slot, f.pack_source.clone());
        scene = next;
        phases.push(phase(&scene));
        frames.push(frame);
    }
    Return {
        backdrop,
        phases,
        frames,
        scene,
    }
}

/// `held` with `press` on frame F+`offset` and nothing on the other frames.
pub(super) fn press_on(offset: usize, press: Buttons) -> [Buttons; 5] {
    let mut held = [NONE; 5];
    held[offset] = press;
    held
}

#[test]
fn the_windows_appear_on_the_third_frame_and_the_highlight_on_the_fourth() {
    let mut f = Fixture::new("return-setup-frames");
    let run = return_from_options(&mut f, &[NONE; 5]);
    assert_eq!(run.phases, SETUP_PHASES);
    let settled = main_menu_state(&run.scene).scene.compose_frame();
    assert_eq!(run.frames[0], run.backdrop, "F: save check, nothing drawn");
    assert_eq!(run.frames[1], run.backdrop, "F+1: battery check");
    assert_ne!(run.frames[2], run.backdrop, "F+2 draws the windows");
    assert_ne!(run.frames[2], settled, "F+2 has no highlight yet");
    assert_eq!(run.frames[3], settled, "F+3 highlights the selection");
    assert_eq!(run.frames[4], settled);
}

#[test]
fn no_press_is_read_before_the_fifth_frame() {
    let mut f = Fixture::new("return-early-input");
    let baseline = return_from_options(&mut f, &[NONE; 5]);
    for offset in 1..FIRST_INPUT_FRAME {
        for press in INPUT_BUTTONS {
            let run = return_from_options(&mut f, &press_on(offset, press));
            let case = format!("{press:?} on F+{offset}");
            assert_eq!(run.phases, SETUP_PHASES, "{case}");
            assert_eq!(run.frames, baseline.frames, "{case}");
            assert_eq!(
                main_menu_state(&run.scene).scene.selected(),
                MainMenuItem::Option,
                "{case}"
            );
        }
    }
}

#[test]
fn a_fresh_a_on_the_fifth_frame_confirms_option() {
    let mut f = Fixture::new("return-input-a");
    let run = return_from_options(&mut f, &press_on(FIRST_INPUT_FRAME, Buttons::A));
    assert_eq!(run.phases[..4], SETUP_PHASES[..4]);
    assert_eq!(run.phases[FIRST_INPUT_FRAME], "confirm");
}

#[test]
fn a_fresh_up_on_the_fifth_frame_moves_the_selection() {
    let mut f = Fixture::new("return-input-up");
    let run = return_from_options(&mut f, &press_on(FIRST_INPUT_FRAME, Buttons::UP));
    assert_eq!(run.phases[FIRST_INPUT_FRAME], "highlight");
    let AppScene::MainMenuHighlight(state) = &run.scene else {
        panic!("expected the highlight frame after UP");
    };
    assert_eq!(state.scene.selected(), MainMenuItem::NewGame);
}

#[test]
fn an_a_held_since_the_setup_is_not_a_fresh_press_on_the_fifth_frame() {
    let mut f = Fixture::new("return-held-a");
    let held = [NONE, NONE, Buttons::A, Buttons::A, Buttons::A];
    let run = return_from_options(&mut f, &held);
    assert_eq!(run.phases, SETUP_PHASES);
}
