//! The main menu's D-pad and A input sequencing: A against a same-frame
//! direction, selection order, boundary fall-through, and the highlight-only
//! frame that follows a move.

use super::tests::{empty_slot, pack_free_menu, pressed};
use super::{advance_scene, AppScene, MainMenuState};
use crate::game_save::SaveSlot;
use crate::main_menu::{MainMenuItem, MainMenuType};
use platform::{ButtonState, Buttons, Frame};

/// Upstream `Task_HandleMainMenuInput` reads A before the D-pad
/// (`main_menu.c:888` before `903`/`915` -- an if/else-if chain), so a
/// same-frame A + direction press acts on A and never moves the
/// selection. Pinned on OPTION with UP alongside: the A must still win,
/// beginning the fade with the selection exactly where it was.
#[test]
fn main_menu_a_wins_over_a_same_frame_direction_press() {
    let (_temp, mut save_slot) = empty_slot("a-wins");
    let mut menu = crate::main_menu::synthetic_scene(MainMenuType::NoSavedGame);
    menu.move_down();
    assert_eq!(menu.selected(), MainMenuItem::Option);
    let scene = AppScene::MainMenu(Box::new(MainMenuState {
        scene: menu,
        saved: save_slot.load(),
    }));

    let (next, _frame) = advance_scene(
        scene,
        pressed(Buttons::A | Buttons::UP),
        &mut save_slot,
        crate::pack_source::PackSource::Runtime,
    );

    let AppScene::MainMenuFadeWait(wait) = next else {
        panic!("A on OPTION must begin its fade, not move the selection");
    };
    assert_eq!(
        wait.state.scene.selected(),
        MainMenuItem::Option,
        "A must win over a same-frame UP press (upstream's else-if chain)"
    );
}

/// Issue #216 regression: `DPAD_UP`/`DPAD_DOWN`, newly pressed, move the
/// main menu's selection -- no pack needed (module docs on the test
/// above).
#[test]
fn main_menu_up_and_down_move_the_selection() {
    let (_temp, mut save_slot) = empty_slot("menu-updown");
    let menu = crate::main_menu::synthetic_scene(MainMenuType::NoSavedGame);
    let scene = AppScene::MainMenu(Box::new(MainMenuState {
        scene: menu,
        saved: save_slot.load(),
    }));

    let (after_down, _frame) = advance_scene(
        scene,
        pressed(Buttons::DOWN),
        &mut save_slot,
        crate::pack_source::PackSource::Runtime,
    );
    let after_down = settle_highlight(after_down, &mut save_slot);
    let AppScene::MainMenu(state) = after_down else {
        panic!("expected to stay on the main menu");
    };
    assert_eq!(state.scene.selected(), MainMenuItem::Option);

    let scene = AppScene::MainMenu(state);
    let (after_up, _frame) = advance_scene(
        scene,
        pressed(Buttons::UP),
        &mut save_slot,
        crate::pack_source::PackSource::Runtime,
    );
    let after_up = settle_highlight(after_up, &mut save_slot);
    let AppScene::MainMenu(state) = after_up else {
        panic!("expected to stay on the main menu");
    };
    assert_eq!(state.scene.selected(), MainMenuItem::NewGame);
}

/// I-6, issue #214: with a save present the menu grows a third item and
/// `CONTINUE` is what a fresh boot has selected, so the *first* thing an
/// A press does is resume -- upstream's `HAS_SAVED_GAME` `tCurrItem == 0`
/// arm (`main_menu.c:972-975`). Pack-free: only the selection order is
/// under test here, not the transition (which needs a room to load).
#[test]
fn a_saved_game_menu_selects_continue_first_and_then_new_game_and_option() {
    let (_temp, mut save_slot) = empty_slot("saved-menu-order");
    let menu = crate::main_menu::synthetic_scene(MainMenuType::SavedGame);
    let mut scene = AppScene::MainMenu(Box::new(MainMenuState {
        scene: menu,
        saved: save_slot.load(),
    }));

    for expected in [
        MainMenuItem::Continue,
        MainMenuItem::NewGame,
        MainMenuItem::Option,
        // A fourth DOWN must not wrap (`tCurrItem < tItemCount - 1`).
        MainMenuItem::Option,
    ] {
        let AppScene::MainMenu(state) = &scene else {
            panic!("expected to stay on the main menu");
        };
        assert_eq!(state.scene.selected(), expected);
        let (next, _frame) = advance_scene(
            scene,
            pressed(Buttons::DOWN),
            &mut save_slot,
            crate::pack_source::PackSource::Runtime,
        );
        // The fourth DOWN is blocked, so it schedules no highlight step.
        scene = if matches!(next, AppScene::MainMenuHighlight(_)) {
            settle_highlight(next, &mut save_slot)
        } else {
            next
        };
    }
}

/// Upstream guards `DPAD_UP` with `tCurrItem > 0` and `DPAD_DOWN` with the
/// last-item bound (`main_menu.c:903`, `:915`), so a blocked direction falls
/// through to the other when both are newly pressed together: UP+DOWN at the
/// top row moves down, and at the bottom row moves up (issue #1960).
#[test]
fn main_menu_simultaneous_up_down_falls_through_at_boundaries() {
    for (menu_type, start_downs, expected) in [
        (MainMenuType::NoSavedGame, 0, MainMenuItem::Option),
        (MainMenuType::NoSavedGame, 1, MainMenuItem::NewGame),
        (MainMenuType::SavedGame, 0, MainMenuItem::NewGame),
        (MainMenuType::SavedGame, 2, MainMenuItem::NewGame),
    ] {
        let (_temp, mut save_slot) = empty_slot("up-down-fallthrough");
        let mut menu = crate::main_menu::synthetic_scene(menu_type);
        for _ in 0..start_downs {
            menu.move_down();
        }
        let scene = AppScene::MainMenu(Box::new(MainMenuState {
            scene: menu,
            saved: save_slot.load(),
        }));
        let (next, _frame) = advance_scene(
            scene,
            pressed(Buttons::UP | Buttons::DOWN),
            &mut save_slot,
            crate::pack_source::PackSource::Runtime,
        );
        assert!(
            matches!(next, AppScene::MainMenuHighlight(_)),
            "{menu_type:?} after {start_downs} downs"
        );
        let AppScene::MainMenu(state) = settle_highlight(next, &mut save_slot) else {
            panic!("expected to stay on the main menu");
        };
        assert_eq!(
            state.scene.selected(),
            expected,
            "{menu_type:?} after {start_downs} downs"
        );
    }
}

/// Advances `scene` one frame with `held` as the raw held set, through a
/// persistent `buttons` so edges match a real multi-frame hold.
fn step(
    scene: AppScene,
    buttons: &mut ButtonState,
    held: Buttons,
    save_slot: &mut SaveSlot,
) -> (AppScene, Box<Frame>) {
    buttons.update(held);
    advance_scene(
        scene,
        *buttons,
        save_slot,
        crate::pack_source::PackSource::Runtime,
    )
}

/// Runs the highlight-only step a successful navigation hands off to, with
/// no input, so a test can read the settled menu.
fn settle_highlight(scene: AppScene, save_slot: &mut SaveSlot) -> AppScene {
    assert!(matches!(scene, AppScene::MainMenuHighlight(_)));
    advance_scene(
        scene,
        ButtonState::new(),
        save_slot,
        crate::pack_source::PackSource::Runtime,
    )
    .0
}

fn selected_of(scene: &AppScene) -> MainMenuItem {
    match scene {
        AppScene::MainMenu(state) | AppScene::MainMenuHighlight(state) => state.scene.selected(),
        _ => panic!("expected a main menu scene"),
    }
}

/// Upstream's navigation task only schedules `Task_MainMenuMain`'s highlight
/// callback after moving (`main_menu.c:930-933`); that callback redraws and
/// reinstalls input without polling (`main_menu.c:878-881`,
/// `task.c:118-120`). So the move frame shows the prior highlight, the next
/// frame shows the new one and ignores input, and input resumes after.
#[test]
fn main_menu_navigation_presents_prior_then_new_highlight_and_ignores_input_between() {
    for (menu_type, downs, direction) in [
        (MainMenuType::NoSavedGame, 0, Buttons::DOWN),
        (MainMenuType::SavedGame, 0, Buttons::DOWN),
        (MainMenuType::SavedGame, 1, Buttons::UP),
        (MainMenuType::NoSavedGame, 1, Buttons::UP),
    ] {
        let (_temp, mut slot) = empty_slot("highlight-frames");
        let mut before_scene = crate::main_menu::synthetic_scene(menu_type);
        let mut after_scene = crate::main_menu::synthetic_scene(menu_type);
        for _ in 0..downs {
            before_scene.move_down();
            after_scene.move_down();
        }
        let before = before_scene.compose_frame();
        if direction == Buttons::DOWN {
            after_scene.move_down();
        } else {
            after_scene.move_up();
        }
        let after = after_scene.compose_frame();
        assert_ne!(
            *before, *after,
            "{menu_type:?}: the highlight must be visible"
        );

        let mut scene = AppScene::MainMenu(Box::new(MainMenuState {
            scene: before_scene,
            saved: slot.load(),
        }));
        let mut buttons = ButtonState::new();
        let (next, frame) = step(scene, &mut buttons, direction, &mut slot);
        assert!(
            matches!(next, AppScene::MainMenuHighlight(_)),
            "{menu_type:?}"
        );
        assert_eq!(
            *frame, *before,
            "{menu_type:?}: frame N keeps the prior highlight"
        );
        scene = next;

        // A and the opposite direction pressed fresh on the highlight-only frame.
        let (next, frame) = step(
            scene,
            &mut buttons,
            Buttons::A | Buttons::UP | Buttons::DOWN,
            &mut slot,
        );
        assert!(matches!(next, AppScene::MainMenu(_)), "{menu_type:?}");
        assert_eq!(
            *frame, *after,
            "{menu_type:?}: frame N+1 shows the new highlight"
        );
        let (next, frame) = step(next, &mut buttons, Buttons::NONE, &mut slot);
        assert!(matches!(next, AppScene::MainMenu(_)), "{menu_type:?}");
        assert_eq!(
            *frame, *after,
            "{menu_type:?}: frame N+2 idles on the new highlight"
        );
    }
}

/// DOWN then an immediate fresh A: the A lands on the highlight-only frame
/// and must not confirm, nor be buffered into a later confirmation.
#[test]
fn main_menu_a_on_the_highlight_frame_after_down_is_ignored() {
    let (_temp, mut slot) = empty_slot("down-then-a");
    let mut buttons = ButtonState::new();
    let scene = pack_free_menu(MainMenuType::SavedGame, &mut slot);
    let (scene, _) = step(scene, &mut buttons, Buttons::DOWN, &mut slot);
    assert_eq!(selected_of(&scene), MainMenuItem::NewGame);
    let (scene, _) = step(scene, &mut buttons, Buttons::A, &mut slot);
    assert!(
        matches!(scene, AppScene::MainMenu(_)),
        "early A must not start the fade"
    );
    // Holding A is not a fresh edge on the first input-capable frame.
    let (scene, _) = step(scene, &mut buttons, Buttons::A, &mut slot);
    assert!(
        matches!(scene, AppScene::MainMenu(_)),
        "held A must not confirm"
    );
    let (scene, _) = step(scene, &mut buttons, Buttons::NONE, &mut slot);
    let (scene, _) = step(scene, &mut buttons, Buttons::A, &mut slot);
    assert!(
        matches!(scene, AppScene::MainMenuFadeWait(_)),
        "a fresh A confirms"
    );
}

/// The first frame after the highlight-only step accepts a fresh A.
#[test]
fn main_menu_a_after_the_highlight_frame_confirms_immediately() {
    let (_temp, mut slot) = empty_slot("down-idle-a");
    let mut buttons = ButtonState::new();
    let scene = pack_free_menu(MainMenuType::SavedGame, &mut slot);
    let (scene, _) = step(scene, &mut buttons, Buttons::DOWN, &mut slot);
    assert!(matches!(scene, AppScene::MainMenuHighlight(_)));
    let (scene, _) = step(scene, &mut buttons, Buttons::NONE, &mut slot);
    assert!(matches!(scene, AppScene::MainMenu(_)));
    let (scene, _) = step(scene, &mut buttons, Buttons::A, &mut slot);
    assert!(matches!(scene, AppScene::MainMenuFadeWait(_)));
}

/// DOWN then an immediate fresh UP: ignored on the highlight-only frame.
#[test]
fn main_menu_up_on_the_highlight_frame_after_down_is_ignored() {
    let (_temp, mut slot) = empty_slot("down-then-up");
    let mut buttons = ButtonState::new();
    let scene = pack_free_menu(MainMenuType::NoSavedGame, &mut slot);
    let (scene, _) = step(scene, &mut buttons, Buttons::DOWN, &mut slot);
    assert_eq!(selected_of(&scene), MainMenuItem::Option);
    let (scene, _) = step(scene, &mut buttons, Buttons::UP, &mut slot);
    assert_eq!(
        selected_of(&scene),
        MainMenuItem::Option,
        "early UP ignored"
    );
    // UP is still held, so not a fresh edge: nothing moves.
    let (scene, _) = step(scene, &mut buttons, Buttons::UP, &mut slot);
    assert_eq!(selected_of(&scene), MainMenuItem::Option);
    let (scene, _) = step(scene, &mut buttons, Buttons::NONE, &mut slot);
    let (scene, _) = step(scene, &mut buttons, Buttons::UP, &mut slot);
    assert_eq!(
        selected_of(&scene),
        MainMenuItem::NewGame,
        "movement resumes"
    );
}

/// The first input-capable frame accepts a fresh UP.
#[test]
fn main_menu_up_after_the_highlight_frame_moves_immediately() {
    let (_temp, mut slot) = empty_slot("down-idle-up");
    let mut buttons = ButtonState::new();
    let scene = pack_free_menu(MainMenuType::NoSavedGame, &mut slot);
    let (scene, _) = step(scene, &mut buttons, Buttons::DOWN, &mut slot);
    assert!(matches!(scene, AppScene::MainMenuHighlight(_)));
    let (scene, _) = step(scene, &mut buttons, Buttons::NONE, &mut slot);
    assert!(matches!(scene, AppScene::MainMenu(_)));
    let (scene, _) = step(scene, &mut buttons, Buttons::UP, &mut slot);
    assert_eq!(selected_of(&scene), MainMenuItem::NewGame);
}

/// A blocked direction moves nothing, so it schedules no highlight step.
#[test]
fn main_menu_blocked_navigation_adds_no_highlight_step() {
    let (_temp, mut slot) = empty_slot("blocked-nav");
    let mut buttons = ButtonState::new();
    let scene = pack_free_menu(MainMenuType::NoSavedGame, &mut slot);
    let (scene, _) = step(scene, &mut buttons, Buttons::UP, &mut slot);
    assert!(
        matches!(scene, AppScene::MainMenu(_)),
        "blocked UP at the top"
    );
}
