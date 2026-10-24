//! I-3 front-end palette fade-wait states, kept apart from `flow.rs` to hold
//! that module within its line-budget convention (`crates/README.md`), the
//! same reason [`super::overworld_phase`] and this file's siblings are their
//! own modules `(oop-boundaries)`. See
//! `flow.rs`'s module docs, "Waiting for the front-end palette fade"
//! section, for the upstream citations and contract both states share.

use platform::{ButtonState, Buttons, Frame};
use rendering::PaletteColorTransform;
use rendering::{Framebuffer, NormalPaletteFade, PaletteFadeStatus, PaletteFadeTarget};

use super::{
    intro, log_game_continued, menu_action, new_game_options_for, title_advance_pressed,
    title_to_main_menu, AnimatedTitle, AppScene, MainMenuAction, MainMenuState, OverworldPhase,
};
use crate::frame::to_platform_frame;
use crate::main_menu::{MainMenuItem, MainMenuScene};
use engine::save::ButtonMode;

/// [`AppScene::TitleFadeWait`]'s state: the title retained for its
/// unfaded-recovery frame ([`advance_title_fade_wait`]'s own doc comment)
/// alongside the fade whose palettes recompose its last tick toward white.
pub(crate) struct TitleFadeWait {
    title: Box<AnimatedTitle>,
    fade: NormalPaletteFade,
}

impl TitleFadeWait {
    /// The boot save's `optionsButtonMode`, carried by the retained title.
    pub(super) fn button_mode(&self) -> ButtonMode {
        self.title.saved.block2.options_button_mode
    }
}

/// [`AppScene::MainMenuFadeWait`]'s state: the menu retained for its
/// unfaded-recovery frame ([`advance_main_menu_fade_wait`]'s own doc
/// comment), the [`MainMenuAction`] the press that started the fade
/// confirmed, and the fade whose palettes recompose the menu toward black.
pub(crate) struct MainMenuFadeWait {
    pub(crate) state: Box<MainMenuState>,
    action: MainMenuAction,
    fade: NormalPaletteFade,
}

/// Upstream's press frame runs two fade updates: `BeginNormalPaletteFade`'s
/// own, then the scene callback's after `RunTasks`
/// (`pokeemerald/src/title_screen.c:675-681`, `main_menu.c:532-538`).
fn begin_on_press_frame(target: PaletteFadeTarget) -> NormalPaletteFade {
    let mut fade = NormalPaletteFade::begin(target);
    let status = fade.update();
    debug_assert_eq!(status, PaletteFadeStatus::Active);
    fade
}

/// The composed backdrop-only `source` frame with the fade's BG blend applied
/// uniformly to every pixel; the frame is a single colour, so this equals
/// fading its palette entry.
fn faded_frame(source: &Framebuffer, fade: &NormalPaletteFade) -> Box<Frame> {
    let blend = fade.bg_blend();
    let mut faded = source.clone();
    for y in 0..source.height() {
        for x in 0..source.width() {
            if let Some(pixel) = source.pixel(x, y) {
                faded.set_pixel(x, y, blend.transform(pixel));
            }
        }
    }
    to_platform_frame(&faded)
}

/// The retained title at its frozen tick, recomposed from `fade`'s palettes.
fn faded_title_frame(title: &AnimatedTitle, fade: &NormalPaletteFade) -> Box<Frame> {
    to_platform_frame(
        &title
            .scene
            .compose_with_palette(title.tick, fade.palette_stage()),
    )
}

/// The retained menu recomposed from `fade`'s palettes.
fn faded_menu_frame(state: &MainMenuState, fade: &NormalPaletteFade) -> Box<Frame> {
    to_platform_frame(&state.scene.compose_with_palette(fade.palette_stage()))
}

/// The [`AppScene::Title`] arm of [`super::advance_scene`]: on a fresh
/// advance press, begins the white fade over the just-composed frame and
/// hands off to [`AppScene::TitleFadeWait`] instead of loading the main
/// menu directly.
pub(super) fn advance_title(
    mut title: Box<AnimatedTitle>,
    buttons: ButtonState,
) -> (AppScene, Box<Frame>) {
    if title.presented {
        title.tick = title.tick.wrapping_add(1);
    }
    title.presented = true;

    if title_advance_pressed(buttons) {
        let fade = begin_on_press_frame(PaletteFadeTarget::White);
        let frame = faded_title_frame(&title, &fade);
        return (
            AppScene::TitleFadeWait(Box::new(TitleFadeWait { title, fade })),
            frame,
        );
    }

    let frame = to_platform_frame(&title.scene.compose(title.tick));
    (AppScene::Title(title), frame)
}

/// The [`AppScene::TitleFadeWait`] arm of [`super::advance_scene`]. The
/// done frame still presents the faded title: upstream's `CB2_GoToMainMenu`
/// only installs the next callback (`pokeemerald/src/title_screen.c:824-828`).
pub(super) fn advance_title_fade_wait(
    mut wait: Box<TitleFadeWait>,
    pack_source: &crate::pack_source::PackSource,
) -> (AppScene, Box<Frame>) {
    if wait.fade.is_done() {
        return match title_to_main_menu(pack_source, wait.title) {
            Ok(result) => result,
            Err(title) => {
                let frame = to_platform_frame(&title.scene.compose(title.tick));
                (AppScene::Title(title), frame)
            }
        };
    }
    wait.fade.update();
    let frame = faded_title_frame(&wait.title, &wait.fade);
    (AppScene::TitleFadeWait(wait), frame)
}

/// The [`AppScene::MainMenu`] arm of [`super::advance_scene`]: a fresh A
/// over `NEW GAME`/`CONTINUE` begins the black fade and hands off to
/// [`AppScene::MainMenuFadeWait`] instead of dispatching [`MainMenuAction`]
/// directly -- `OPTION` included, whose action opens the Options screen -- and a successful
/// Up/Down hands off to [`AppScene::MainMenuHighlight`].
pub(super) fn advance_main_menu(
    state: Box<MainMenuState>,
    buttons: ButtonState,
) -> (AppScene, Box<Frame>) {
    if buttons.is_newly_pressed(Buttons::A) {
        // A pure decision, not an inline match, so the item -> action
        // mapping is pinned by a pack-less test -- see `menu_action`'s own
        // doc comment.
        let action = menu_action(state.scene.selected());
        let fade = begin_on_press_frame(PaletteFadeTarget::Black);
        let frame = faded_menu_frame(&state, &fade);
        return (
            AppScene::MainMenuFadeWait(Box::new(MainMenuFadeWait {
                state,
                action,
                fade,
            })),
            frame,
        );
    } else if buttons.is_newly_pressed(Buttons::UP) && state.scene.can_move_up() {
        // Upstream guards each arm (`main_menu.c:903`, `:915`), so a blocked
        // Up falls through to Down when both are newly pressed together.
        return navigate(state, MainMenuScene::move_up);
    } else if buttons.is_newly_pressed(Buttons::DOWN) && state.scene.can_move_down() {
        return navigate(state, MainMenuScene::move_down);
    }
    let frame = state.scene.compose_frame();
    (AppScene::MainMenu(state), frame)
}

/// Moves the selection but presents the prior highlight, then hands off to
/// [`AppScene::MainMenuHighlight`]: upstream's input task only schedules the
/// redraw callback (`main_menu.c:930-933`), so the new highlight appears a
/// frame later with input ignored.
fn navigate(
    mut state: Box<MainMenuState>,
    movement: fn(&mut MainMenuScene),
) -> (AppScene, Box<Frame>) {
    let frame = state.scene.compose_frame();
    movement(&mut state.scene);
    (AppScene::MainMenuHighlight(state), frame)
}

/// The [`AppScene::MainMenuFadeWait`] arm of [`super::advance_scene`]. The
/// done frame still presents the faded menu: upstream's task polls
/// `gPaletteFade.active` before that frame's update
/// (`pokeemerald/src/main_menu.c:532-538`, `:936-943`).
pub(super) fn advance_main_menu_fade_wait(
    mut wait: Box<MainMenuFadeWait>,
    pack_source: crate::pack_source::PackSource,
) -> (AppScene, Box<Frame>) {
    if wait.fade.is_done() {
        if matches!(wait.action, MainMenuAction::Options) {
            return match super::options::open(&pack_source, wait.state) {
                Ok(result) => result,
                Err(state) => {
                    let frame = state.scene.compose_frame();
                    (AppScene::MainMenu(state), frame)
                }
            };
        }
        if let Some(result) = dispatch_main_menu_action(wait.action, pack_source, &wait.state) {
            return result;
        }
        let frame = wait.state.scene.compose_frame();
        return (AppScene::MainMenu(wait.state), frame);
    }
    wait.fade.update();
    let frame = faded_menu_frame(&wait.state, &wait.fade);
    (AppScene::MainMenuFadeWait(wait), frame)
}

/// [`AppScene::MainMenuFadeIn`]'s state: the rebuilt menu, held back while the
/// fade from black reveals the bare backdrop.
///
/// `InitMainMenu(TRUE)` clears VRAM and fades in with no window drawn; the
/// menu is drawn by a task that waits for the fade to finish
/// (`pokeemerald/src/main_menu.c:556-650,740-745`).
pub(crate) struct MainMenuFadeIn {
    state: Box<MainMenuState>,
    source: Framebuffer,
    /// `None` on the handoff frame: the fade begins on the next one.
    fade: Option<NormalPaletteFade>,
}

impl MainMenuFadeIn {
    /// The retained save's committed `optionsButtonMode`.
    pub(super) fn button_mode(&self) -> engine::save::ButtonMode {
        self.state.saved.block2.options_button_mode
    }

    /// The item the retained menu has selected.
    pub(crate) fn selected(&self) -> MainMenuItem {
        self.state.scene.selected()
    }

    /// Hands off from the Options screen: a black frame, with the fade to
    /// follow (`pokeemerald/src/main_menu.c:583-586`).
    pub(super) fn begin(state: Box<MainMenuState>) -> (Self, Box<Frame>) {
        let source = state.scene.compose_backdrop();
        let mut black = Framebuffer::new();
        black.fill(rendering::Rgb888::BLACK);
        (
            Self {
                state,
                source,
                fade: None,
            },
            to_platform_frame(&black),
        )
    }
}

/// The [`AppScene::MainMenuFadeIn`] arm of [`super::advance_scene`]: input
/// is ignored until the fade finishes, then the menu is drawn.
pub(super) fn advance_main_menu_fade_in(mut wait: Box<MainMenuFadeIn>) -> (AppScene, Box<Frame>) {
    let Some(fade) = wait.fade.as_mut() else {
        let fade = NormalPaletteFade::begin_in(PaletteFadeTarget::Black);
        let frame = faded_frame(&wait.source, &fade);
        wait.fade = Some(fade);
        return (AppScene::MainMenuFadeIn(wait), frame);
    };
    if fade.is_done() {
        let frame = wait.state.scene.compose_frame();
        return (AppScene::MainMenu(wait.state), frame);
    }
    fade.update();
    let frame = faded_frame(&wait.source, fade);
    (AppScene::MainMenuFadeIn(wait), frame)
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
            match intro::load(&pack_source, new_game_options_for(&state.saved)) {
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
        MainMenuAction::Options => None,
    }
}
