//! The main menu's setup frames on a fresh boot (I-6): upstream's
//! `InitMainMenu(FALSE)` fades in from white and then runs the same
//! one-task-per-frame chain as the Options return
//! (`pokeemerald/src/main_menu.c:583-608,619-934`). Frame F is the first one
//! whose fade poll is already done; the menu is first drawn on F+2 and input
//! counts from F+4. `options_return_tests` pins the shared chain after the
//! Options screen.

use super::options_return_tests::{
    phase, press_on, FIRST_INPUT_FRAME, INPUT_BUTTONS, NONE, SETUP_PHASES,
};
use super::options_tests::Fixture;
use super::{advance_scene, title_to_main_menu, AppScene};
use crate::main_menu::MainMenuItem;
use platform::{ButtonState, Buttons, Frame};
use rendering::{NormalPaletteFade, PaletteFadeTarget};

/// Fully faded to `RGB_WHITEALPHA`: every channel at 31/31.
const WHITE: u32 = 0x00FF_FFFF;

/// Updates the white fade-in takes to report done after its first frame:
/// `rendering::palette_fade`'s 22-call trace.
const FADE_IN_UPDATES: usize = 22;

struct Boot {
    /// The frame the title's fade-out hands off on: the fade-in's first.
    handoff: Box<Frame>,
    /// Frames F onwards, one per entry of `held`.
    phases: Vec<&'static str>,
    frames: Vec<Box<Frame>>,
    scene: AppScene,
}

/// Runs the title's done frame (`title_to_main_menu`, which already begins the
/// fade), drives the white fade-in up to the frame before F, then steps one production frame per
/// entry of `held`, starting at F.
fn boot_menu(f: &mut Fixture, held: &[Buttons]) -> Boot {
    let mut buttons = ButtonState::new();
    let (mut scene, handoff) = title_to_main_menu(&f.pack_source, &mut f.slot, &mut None)
        .expect("the synthetic pack builds the main menu");
    assert_eq!(phase(&scene), "fade-in");
    let mut oracle = NormalPaletteFade::begin_in(PaletteFadeTarget::White);
    let mut fade_updates = 0;
    while !oracle.is_done() {
        oracle.update();
        fade_updates += 1;
        scene = f.step(scene, &mut buttons, NONE);
    }
    assert_eq!(
        fade_updates, FADE_IN_UPDATES,
        "the xtask `boot_to_first_fight` script counts these frames"
    );
    let (mut phases, mut frames) = (Vec::new(), Vec::new());
    for &press in held {
        buttons.update(press);
        let (next, frame) = advance_scene(scene, buttons, &mut f.slot, f.pack_source.clone());
        scene = next;
        phases.push(phase(&scene));
        frames.push(frame);
    }
    Boot {
        handoff,
        phases,
        frames,
        scene,
    }
}

fn selected(scene: &AppScene) -> MainMenuItem {
    match scene {
        AppScene::MainMenu(state) | AppScene::MainMenuHighlight(state) => state.scene.selected(),
        _ => panic!("expected a main menu scene"),
    }
}

#[test]
fn the_title_hands_off_on_white_and_fades_in_before_the_setup_frames() {
    let mut f = Fixture::new("boot-white-fade-in");
    let run = boot_menu(&mut f, &[NONE; 5]);
    assert!(
        run.handoff.iter().all(|&pixel| pixel == WHITE),
        "the fade-in begins at full white"
    );
    assert_ne!(
        run.frames[0], run.handoff,
        "by F the fade has revealed the backdrop"
    );
}

#[test]
fn the_windows_appear_on_the_third_frame_and_the_highlight_on_the_fourth() {
    let mut f = Fixture::new("boot-setup-frames");
    let run = boot_menu(&mut f, &[NONE; 5]);
    assert_eq!(run.phases, SETUP_PHASES);
    let AppScene::MainMenu(state) = &run.scene else {
        panic!("the menu takes input on F+4");
    };
    let (backdrop, settled) = (state.scene.compose_backdrop(), state.scene.compose_frame());
    let backdrop = crate::frame::to_platform_frame(&backdrop);
    assert_eq!(run.frames[0], backdrop, "F: save check, nothing drawn");
    assert_eq!(run.frames[1], backdrop, "F+1: battery check");
    assert_ne!(run.frames[2], backdrop, "F+2 draws the windows");
    assert_ne!(run.frames[2], settled, "F+2 has no highlight yet");
    assert_eq!(run.frames[3], settled, "F+3 highlights the selection");
    assert_eq!(run.frames[4], settled);
}

#[test]
fn no_press_is_read_before_the_fifth_frame() {
    let mut f = Fixture::new("boot-early-input");
    let baseline = boot_menu(&mut f, &[NONE; 5]);
    for offset in 1..FIRST_INPUT_FRAME {
        for press in INPUT_BUTTONS {
            let run = boot_menu(&mut f, &press_on(offset, press));
            let case = format!("{press:?} on F+{offset}");
            assert_eq!(run.phases, SETUP_PHASES, "{case}");
            assert_eq!(run.frames, baseline.frames, "{case}");
            assert_eq!(selected(&run.scene), MainMenuItem::NewGame, "{case}");
        }
    }
}

#[test]
fn a_fresh_a_on_the_fifth_frame_confirms_the_selection() {
    let mut f = Fixture::new("boot-input-a");
    let run = boot_menu(&mut f, &press_on(FIRST_INPUT_FRAME, Buttons::A));
    assert_eq!(run.phases[..4], SETUP_PHASES[..4]);
    assert_eq!(run.phases[FIRST_INPUT_FRAME], "confirm");
}

#[test]
fn a_fresh_down_on_the_fifth_frame_moves_the_selection() {
    let mut f = Fixture::new("boot-input-down");
    let run = boot_menu(&mut f, &press_on(FIRST_INPUT_FRAME, Buttons::DOWN));
    assert_eq!(run.phases[FIRST_INPUT_FRAME], "highlight");
    assert_eq!(selected(&run.scene), MainMenuItem::Option);
}

#[test]
fn an_a_held_since_the_setup_is_not_a_fresh_press_on_the_fifth_frame() {
    let mut f = Fixture::new("boot-held-a");
    let held = [NONE, NONE, Buttons::A, Buttons::A, Buttons::A];
    let run = boot_menu(&mut f, &held);
    assert_eq!(run.phases, SETUP_PHASES);
}

/// The title's done update installs `CB2_InitMainMenu`, so the handoff frame
/// begins the fade (`title_screen.c:824-828`); input is polled 27 frames on.
#[test]
fn a_fresh_a_27_frames_after_the_title_handoff_confirms() {
    let mut f = Fixture::new("boot-a-27");
    let mut buttons = ButtonState::new();
    let (mut scene, _) = title_to_main_menu(&f.pack_source, &mut f.slot, &mut None)
        .expect("the synthetic pack builds the main menu");
    for _ in 0..26 {
        scene = f.step(scene, &mut buttons, NONE);
    }
    assert_eq!(phase(&scene), "menu", "the menu polls input on P+49");
    scene = f.step(scene, &mut buttons, Buttons::A);
    assert_eq!(phase(&scene), "confirm", "a fresh A on P+49 confirms");
}
