//! I-3 (issue #1329) front-end palette fade-wait states -- split out of
//! `flow.rs` itself to keep that module within its own line-budget
//! convention (`crates/README.md`), the same reason [`super::overworld_phase`]
//! and this file's siblings are their own modules `(oop-boundaries)`. See
//! `flow.rs`'s module docs, "Waiting for the front-end palette fade"
//! section, for the upstream citations and contract both states share.

use platform::{ButtonState, Buttons, Frame};
use rendering::{NormalPaletteFade, PaletteFadeStatus, PaletteFadeTarget};

use super::{
    intro, log_game_continued, menu_action, new_game_options_for, title_advance_pressed,
    title_to_main_menu, AnimatedTitle, AppScene, MainMenuAction, MainMenuState, OverworldPhase,
};
use crate::frame::to_platform_frame;
use crate::game_save::SaveSlot;

/// [`AppScene::TitleFadeWait`]'s state: the title retained for its
/// unfaded-recovery frame ([`advance_title_fade_wait`]'s own doc comment)
/// alongside the fade blending its last composed tick toward white.
pub(crate) struct TitleFadeWait {
    title: Box<AnimatedTitle>,
    fade: NormalPaletteFade,
}

/// [`AppScene::MainMenuFadeWait`]'s state: the menu retained for its
/// unfaded-recovery frame ([`advance_main_menu_fade_wait`]'s own doc
/// comment), the [`MainMenuAction`] the press that started the fade
/// confirmed, and the fade blending the confirmed selection toward black.
pub(crate) struct MainMenuFadeWait {
    pub(crate) state: Box<MainMenuState>,
    action: MainMenuAction,
    fade: NormalPaletteFade,
}

/// The [`super::AppScene::Title`] arm of [`super::advance_scene`] (kept a
/// free function so the match arms stay under the line-budget lint,
/// `(oop-boundaries)`): advances the idle title's animation tick, then on a
/// fresh advance press begins the white fade and hands off to
/// [`AppScene::TitleFadeWait`] instead of loading the main menu directly.
pub(super) fn advance_title(
    mut title: Box<AnimatedTitle>,
    buttons: ButtonState,
) -> (AppScene, Box<Frame>) {
    if title.presented {
        title.tick = title.tick.wrapping_add(1);
    }
    title.presented = true;

    if title_advance_pressed(buttons) {
        let framebuffer = title.scene.compose(title.tick);
        let fade = NormalPaletteFade::begin(framebuffer, PaletteFadeTarget::White);
        let frame = to_platform_frame(fade.framebuffer());
        return (
            AppScene::TitleFadeWait(Box::new(TitleFadeWait { title, fade })),
            frame,
        );
    }

    let frame = to_platform_frame(&title.scene.compose(title.tick));
    (AppScene::Title(title), frame)
}

/// The [`AppScene::TitleFadeWait`] arm of [`super::advance_scene`]: drives
/// one fade update, ignoring input, and only once it reports done calls
/// [`title_to_main_menu`] -- restoring the retained title unfaded, at the
/// tick it last composed, if that load fails (that function's own doc
/// comment).
pub(super) fn advance_title_fade_wait(
    mut wait: Box<TitleFadeWait>,
    save_slot: &mut SaveSlot,
    pack_source: crate::pack_source::PackSource,
) -> (AppScene, Box<Frame>) {
    if wait.fade.update() == PaletteFadeStatus::Done {
        if let Some(result) = title_to_main_menu(pack_source, save_slot) {
            return result;
        }
        let frame = to_platform_frame(&wait.title.scene.compose(wait.title.tick));
        return (AppScene::Title(wait.title), frame);
    }
    let frame = to_platform_frame(wait.fade.framebuffer());
    (AppScene::TitleFadeWait(wait), frame)
}

/// The [`AppScene::MainMenu`] arm of [`super::advance_scene`]
/// ([`advance_title`]'s own doc comment on why this is a free function): a
/// fresh A over `NEW GAME`/`CONTINUE` begins the black fade and hands off to
/// [`AppScene::MainMenuFadeWait`] instead of dispatching [`MainMenuAction`]
/// directly; `OPTION`'s swallowed press and Up/Down selection are
/// unaffected.
pub(super) fn advance_main_menu(
    mut state: Box<MainMenuState>,
    buttons: ButtonState,
) -> (AppScene, Box<Frame>) {
    if buttons.is_newly_pressed(Buttons::A) {
        // A pure decision, not an inline match, so the item -> action
        // mapping is pinned by a pack-less test -- see `menu_action`'s own
        // doc comment.
        let action = menu_action(state.scene.selected());
        if !matches!(action, MainMenuAction::None) {
            let framebuffer = state.scene.compose();
            let fade = NormalPaletteFade::begin(framebuffer, PaletteFadeTarget::Black);
            let frame = to_platform_frame(fade.framebuffer());
            return (
                AppScene::MainMenuFadeWait(Box::new(MainMenuFadeWait {
                    state,
                    action,
                    fade,
                })),
                frame,
            );
        }
    } else if buttons.is_newly_pressed(Buttons::UP) {
        state.scene.move_up();
    } else if buttons.is_newly_pressed(Buttons::DOWN) {
        state.scene.move_down();
    }
    let frame = state.scene.compose_frame();
    (AppScene::MainMenu(state), frame)
}

/// The [`AppScene::MainMenuFadeWait`] arm of [`super::advance_scene`]:
/// drives one fade update, ignoring input, and only once it reports done
/// calls [`dispatch_main_menu_action`] -- restoring the retained menu
/// unfaded if that dispatch fails (that function's own doc comment).
pub(super) fn advance_main_menu_fade_wait(
    mut wait: Box<MainMenuFadeWait>,
    pack_source: crate::pack_source::PackSource,
) -> (AppScene, Box<Frame>) {
    if wait.fade.update() == PaletteFadeStatus::Done {
        if let Some(result) = dispatch_main_menu_action(wait.action, pack_source, &wait.state) {
            return result;
        }
        let frame = wait.state.scene.compose_frame();
        return (AppScene::MainMenu(wait.state), frame);
    }
    let frame = to_platform_frame(wait.fade.framebuffer());
    (AppScene::MainMenuFadeWait(wait), frame)
}

/// Dispatches the [`MainMenuAction`] a press confirmed before the fade
/// began, once [`advance_main_menu_fade_wait`] observes it done. `state` is
/// borrowed, not consumed, so a failed dispatch leaves the caller free to
/// restore the menu from it unchanged, mirroring [`title_to_main_menu`]'s
/// `None`-on-failure contract.
///
/// Returns `None`, after logging, for a failed load/resume, and for
/// [`MainMenuAction::None`] (unreachable here -- only `NewGame`/`Continue`
/// ever enter [`AppScene::MainMenuFadeWait`]).
fn dispatch_main_menu_action(
    action: MainMenuAction,
    pack_source: crate::pack_source::PackSource,
    state: &MainMenuState,
) -> Option<(AppScene, Box<Frame>)> {
    match action {
        MainMenuAction::NewGame => {
            match intro::load(pack_source, new_game_options_for(&state.saved)) {
                Ok(intro_scene) => {
                    let frame = intro_scene.compose_frame();
                    Some((AppScene::Intro(Box::new(intro_scene)), frame))
                }
                Err(err) => {
                    eprintln!("intro: {err} -- staying on the main menu");
                    None
                }
            }
        }
        // `ACTION_CONTINUE` (`main_menu.c:1064-1069`) ->
        // `CB2_ContinueSavedGame`. The blocks are cloned out of `state`
        // rather than moved, because a failed continue must leave the menu
        // exactly as it was -- still offering `CONTINUE`, still holding the
        // save it could not resume.
        MainMenuAction::Continue => {
            let (block1, block2) = (state.saved.block1.clone(), state.saved.block2.clone());
            match OverworldPhase::continue_saved_game(pack_source, block1, block2) {
                Ok(phase) => {
                    log_game_continued(&phase);
                    let frame = phase.compose_frame();
                    Some((AppScene::Overworld(Box::new(phase)), frame))
                }
                Err(err) => {
                    eprintln!("{err} -- staying on the main menu");
                    None
                }
            }
        }
        MainMenuAction::None => None,
    }
}
