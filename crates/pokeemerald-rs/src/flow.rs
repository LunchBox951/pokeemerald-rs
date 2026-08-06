//! The real (windowed) game-flow state machine (I-3, issue #149): title ->
//! main menu -> intro -> overworld.
//!
//! Split out of [`crate::app`] to keep that module focused on the
//! platform/window shell (`one module = one concept` `(oop-boundaries)`):
//! [`AppScene`] is the state, [`advance_scene`] is the one-frame transition
//! function [`crate::app::App::step`] delegates to. See [`crate::app`]'s
//! module docs for the transition diagram (`Title` -> `MainMenu` -> `Intro`
//! -> `Overworld`) and the "log-or-ignore is fine" failure policy for a
//! transition's own pack load. The one exception is the `Intro` ->
//! `Overworld` transition specifically: a failed load there moves to the
//! distinct [`AppScene::OverworldLoadFailed`] waiting state rather than
//! back to `Intro` unchanged, so a persistently missing/broken pack is
//! retried (and re-logged) only on a fresh confirm/skip press, not every
//! single frame -- see that variant's own doc comment and
//! [`should_retry_overworld_load`].
//!
//! [`AnimatedTitle`] is the pre-I-3 per-frame title-animation state,
//! unchanged in shape and behaviour from before this issue (see
//! [`advance_scene`]'s `Title` arm and `crate::app`'s "Animating the real
//! title screen" docs) -- only its ownership moved here, into one variant of
//! [`AppScene`] instead of its own dedicated `App` field.

use platform::{ButtonState, Buttons, Frame};

use crate::frame::to_platform_frame;
use crate::intro::{self, IntroScene, IntroStatus};
use crate::main_menu::{self, MainMenuItem, MainMenuScene};
use crate::title::TitleScene;

mod overworld_phase;
mod wild_encounter;
pub(crate) use overworld_phase::OverworldPhase;

/// [`crate::app::App`]'s per-frame animation state for the real title screen
/// (`crate::app`'s "Animating the real title screen" docs): the loaded
/// scene, the tick most recently composed, and whether that frame has been
/// presented yet (so [`advance_scene`] advances the tick *before*
/// presenting every frame after the first -- keeping
/// [`crate::app::App::frame`]'s "most recently presented" contract true at
/// all times).
pub(crate) struct AnimatedTitle {
    pub(crate) scene: TitleScene,
    pub(crate) tick: u32,
    pub(crate) presented: bool,
}

/// [`crate::app::App`]'s real (windowed) game-flow state (module docs):
/// which scene is currently active. Every variant is boxed -- their sizes
/// vary wildly (an [`AnimatedTitle`] embeds a whole [`TitleScene`]'s
/// tile/palette data; an [`OverworldPhase`] embeds a whole
/// [`OverworldScene`](crate::overworld::OverworldScene)'s) -- so the enum
/// itself stays cheap to move around
/// (`clippy::large_enum_variant`).
pub(crate) enum AppScene {
    /// The idle/animating title screen, waiting for A or Start
    /// ([`title_advance_pressed`]).
    Title(Box<AnimatedTitle>),
    /// The no-save-present main menu (I-3, issue #216): Up/Down move the
    /// `NEW GAME`/`OPTION` selection, A confirms it (module docs on
    /// [`advance_scene`]'s `MainMenu` arm for what each item does).
    MainMenu(Box<MainMenuScene>),
    /// Birch's speech, paging through [`crate::intro::speech`]'s text.
    Intro(Box<IntroScene>),
    /// The intro finished, but [`OverworldPhase::load_default`] failed once
    /// already (module docs' "log-or-ignore is fine" policy, and
    /// [`advance_scene`]'s `Intro`/`OverworldLoadFailed` arms) -- kept
    /// distinct from [`AppScene::Intro`] so a still-failing pack load is
    /// retried only on a fresh confirm/skip edge, not re-attempted (and
    /// re-logged) every single frame while parked here.
    OverworldLoadFailed(Box<IntroScene>),
    /// The overworld loop: the player, movable, in
    /// [`crate::new_game::SPAWN_MAP_ID`].
    Overworld(Box<OverworldPhase>),
}

/// Whether the idle title screen should advance to the main menu this
/// frame.
///
/// Upstream's idle title-screen task accepts either button, newly pressed:
/// `Task_TitleScreenPhase3` (`pokeemerald/src/title_screen.c:780-786`,
/// the task that "processes main title screen input") opens with
/// `if (JOY_NEW(A_BUTTON) || JOY_NEW(START_BUTTON))` before running
/// `CB2_GoToMainMenu` `(behavioral-fidelity)`. Only *newly* pressed counts,
/// exactly as `JOY_NEW` means: a button already held when the title screen
/// took over (e.g. carried in from the preceding intro-skip press,
/// `Task_TitleScreenPhase2`'s own `JOY_NEW(A_B_START_SELECT)` skip) must not
/// immediately fall through to the menu.
///
/// The other `Task_TitleScreenPhase3` branches are held-combo cheats
/// (`CLEAR_SAVE_BUTTON_COMBO`, `RESET_RTC_BUTTON_COMBO`,
/// `BERRY_UPDATE_BUTTON_COMBO`) whose target screens this port has no
/// equivalent of yet -- out of this slice's scope, not modelled here.
fn title_advance_pressed(buttons: ButtonState) -> bool {
    buttons.is_newly_pressed(Buttons::A) || buttons.is_newly_pressed(Buttons::START)
}

/// Whether [`AppScene::OverworldLoadFailed`]'s waiting state should retry
/// [`OverworldPhase::load_default`] this frame -- only on a *fresh* confirm
/// (A) or skip (B) edge, the same two buttons [`AppScene::Intro`] itself
/// reads (module docs on the finding this guards against: the previous
/// behaviour re-attempted, and re-logged, the load every single frame while
/// stuck here, since `IntroStatus::Finished` is sticky and was the only
/// condition gating the attempt).
fn should_retry_overworld_load(buttons: ButtonState) -> bool {
    buttons.is_newly_pressed(Buttons::A) || buttons.is_newly_pressed(Buttons::B)
}

/// Log the one-time proof that `phase`'s fresh save state
/// ([`OverworldPhase::load_default`]'s own doc comment, finding 1 of this
/// module's review pass) actually reached the `Intro` -> `Overworld`
/// handoff -- the same "log-or-ignore is fine" pipeline-liveness style
/// [`crate::app::describe_newly_pressed`] already uses for input, since no
/// save-file writer exists yet to consume this state instead.
fn log_new_game_started(phase: &OverworldPhase) {
    eprintln!(
        "new game: money={} trainer_id={:02x?} gender={:?}",
        phase.save1().money,
        phase.save2().player_trainer_id,
        phase.save2().player_gender,
    );
}

/// Advance `scene` by exactly one frame given this frame's `buttons`,
/// returning the (possibly transitioned) next scene and the frame it
/// composed -- the pure state-transition core of
/// [`crate::app::App::step`]'s real (windowed) path (module docs), factored
/// out as a free function over an owned [`AppScene`] so it needs no
/// `&mut App` self-borrow and is directly unit-testable.
///
/// Every `Title`/`MainMenu`/`Intro` -> next-scene transition loads its own
/// fresh [`assets::AssetPack`] (mirroring how [`TitleScene`]/
/// [`OverworldScene`](crate::overworld::OverworldScene) already each load
/// their own pack independently) and
/// composes that scene's first frame immediately, so the returned frame is
/// always the *new* scene's -- never a stale one from the scene being left.
/// If a transition's pack load fails, this logs and returns the *original*
/// scene unchanged instead (module docs) -- except `Intro` -> `Overworld`
/// specifically, whose failure instead moves to
/// [`AppScene::OverworldLoadFailed`] (module docs' exception, and that
/// variant's own doc comment) so the failed attempt isn't repeated every
/// frame.
pub(crate) fn advance_scene(scene: AppScene, buttons: ButtonState) -> (AppScene, Box<Frame>) {
    match scene {
        AppScene::Title(mut title) => {
            if title.presented {
                title.tick = title.tick.wrapping_add(1);
            }
            title.presented = true;

            if title_advance_pressed(buttons) {
                match main_menu::load_default() {
                    Ok(menu) => {
                        let frame = menu.compose_frame();
                        return (AppScene::MainMenu(Box::new(menu)), frame);
                    }
                    Err(err) => eprintln!("main menu: {err} -- staying on the title screen"),
                }
            }

            let frame = to_platform_frame(&title.scene.compose(title.tick));
            (AppScene::Title(title), frame)
        }
        AppScene::MainMenu(mut menu) => {
            if buttons.is_newly_pressed(Buttons::A) {
                match menu.selected() {
                    MainMenuItem::NewGame => match intro::load_default() {
                        Ok(intro_scene) => {
                            let frame = intro_scene.compose_frame();
                            return (AppScene::Intro(Box::new(intro_scene)), frame);
                        }
                        Err(err) => eprintln!("intro: {err} -- staying on the main menu"),
                    },
                    // Upstream's `ACTION_OPTION` destination
                    // (`main_menu.c:963`) is a separate settings screen this
                    // port doesn't model yet (issue #216 scope notes) --
                    // swallow the press instead of incorrectly launching
                    // the intro.
                    MainMenuItem::Option => {}
                }
            } else if buttons.is_newly_pressed(Buttons::UP) {
                menu.move_up();
            } else if buttons.is_newly_pressed(Buttons::DOWN) {
                menu.move_down();
            }
            let frame = menu.compose_frame();
            (AppScene::MainMenu(menu), frame)
        }
        AppScene::Intro(mut intro_scene) => {
            let confirm_pressed = buttons.is_newly_pressed(Buttons::A);
            let skip_pressed = buttons.is_newly_pressed(Buttons::B);
            let status = intro_scene.tick(confirm_pressed, skip_pressed);

            if status == IntroStatus::Finished {
                match OverworldPhase::load_default() {
                    Ok(phase) => {
                        log_new_game_started(&phase);
                        let frame = phase.compose_frame();
                        return (AppScene::Overworld(Box::new(phase)), frame);
                    }
                    Err(err) => {
                        // Log once, on the attempt itself, then move to the
                        // explicit waiting state below -- not back into
                        // `Intro`, which would just repeat this same
                        // attempt (and this same log line) every following
                        // frame, since `status` stays `Finished` forever
                        // once reached (`IntroScene::tick`'s own contract).
                        eprintln!("overworld: {err} -- staying on the intro");
                        let frame = intro_scene.compose_frame();
                        return (AppScene::OverworldLoadFailed(intro_scene), frame);
                    }
                }
            }

            let frame = intro_scene.compose_frame();
            (AppScene::Intro(intro_scene), frame)
        }
        AppScene::OverworldLoadFailed(intro_scene) => {
            if should_retry_overworld_load(buttons) {
                match OverworldPhase::load_default() {
                    Ok(phase) => {
                        log_new_game_started(&phase);
                        let frame = phase.compose_frame();
                        return (AppScene::Overworld(Box::new(phase)), frame);
                    }
                    Err(err) => eprintln!("overworld: {err} -- staying on the intro"),
                }
            }
            let frame = intro_scene.compose_frame();
            (AppScene::OverworldLoadFailed(intro_scene), frame)
        }
        AppScene::Overworld(mut phase) => {
            phase.step(buttons);
            let frame = phase.compose_frame();
            (AppScene::Overworld(phase), frame)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        advance_scene, should_retry_overworld_load, title_advance_pressed, AnimatedTitle, AppScene,
    };
    use crate::intro::{self, IntroStatus};
    use crate::new_game;
    use platform::{ButtonState, Buttons};

    fn pressed(button: Buttons) -> ButtonState {
        let mut state = ButtonState::new();
        state.update(button);
        state
    }

    pub(super) fn held(button: Buttons) -> ButtonState {
        // Two updates: the first makes it newly-pressed, the second makes
        // it merely held (matching a real multi-frame hold).
        let mut state = ButtonState::new();
        state.update(button);
        state.update(button);
        state
    }

    /// Finding 3 regression: `AppScene::OverworldLoadFailed` must retry
    /// `OverworldPhase::load_default` only on a fresh confirm/skip edge, not
    /// merely because a frame elapsed -- an ordinary held button (already
    /// pressed on a previous frame) must not count.
    #[test]
    fn should_retry_overworld_load_only_on_a_fresh_confirm_or_skip_edge() {
        assert!(!should_retry_overworld_load(ButtonState::new()));
        assert!(should_retry_overworld_load(pressed(Buttons::A)));
        assert!(should_retry_overworld_load(pressed(Buttons::B)));
        assert!(
            !should_retry_overworld_load(held(Buttons::A)),
            "an already-held A (not a fresh edge) must not trigger a retry"
        );
        assert!(!should_retry_overworld_load(pressed(Buttons::START)));
    }

    /// Finding 3 regression: a failed `Intro` -> `Overworld` transition must
    /// leave `AppScene::Intro` for the explicit `AppScene::OverworldLoadFailed`
    /// waiting state after exactly one attempt -- not loop retrying (and
    /// re-logging) from inside `AppScene::Intro` every frame, which is what
    /// happens if the transition is only ever gated on
    /// `IntroStatus::Finished` (sticky forever once reached).
    ///
    /// No local pack is ever present in this crate's own `cargo test`
    /// environment (`assets-pack/` isn't written by anything in this repo --
    /// see `crate::title::tests::load_default_reports_pack_missing_when_no_pack_is_extracted`
    /// for the identical guard/rationale), so `OverworldPhase::load_default`
    /// reliably fails here, exercising the real failure path without
    /// `#[ignore]`. If a local pack *is* present, this test steps aside
    /// entirely rather than asserting the wrong thing.
    #[test]
    fn a_failed_overworld_load_waits_instead_of_retrying_every_frame() {
        if assets::pack::AssetPack::default_path().is_file() {
            return;
        }

        let scene = AppScene::Intro(Box::new(intro::synthetic_finished_scene()));

        let (after_first, _frame) = advance_scene(scene, ButtonState::new());
        assert!(
            matches!(after_first, AppScene::OverworldLoadFailed(_)),
            "a failed load must leave `Intro` for the explicit waiting state"
        );

        // No input edge across further frames -> stay waiting, not attempt
        // the load again (nor bounce back to `Intro`).
        let (after_second, _frame) = advance_scene(after_first, ButtonState::new());
        assert!(matches!(after_second, AppScene::OverworldLoadFailed(_)));

        // A fresh confirm edge retries the load -- still fails (no pack),
        // but must land back in the same waiting state, not panic.
        let (after_retry, _frame) = advance_scene(after_second, pressed(Buttons::A));
        assert!(matches!(after_retry, AppScene::OverworldLoadFailed(_)));
    }

    /// Finding 3 regression: the title screen advances on a freshly pressed
    /// A *or* Start -- `Task_TitleScreenPhase3`'s own
    /// `JOY_NEW(A_BUTTON) || JOY_NEW(START_BUTTON)`
    /// (`pokeemerald/src/title_screen.c:782`), not Start alone -- and on
    /// neither of them while merely held, nor on any other button.
    #[test]
    fn title_advances_on_a_freshly_pressed_a_or_start_only() {
        assert!(title_advance_pressed(pressed(Buttons::START)));
        assert!(
            title_advance_pressed(pressed(Buttons::A)),
            "upstream's idle title task accepts A as well as Start"
        );
        assert!(!title_advance_pressed(ButtonState::new()));
        assert!(
            !title_advance_pressed(held(Buttons::A)),
            "JOY_NEW means a fresh edge -- an already-held A must not advance"
        );
        assert!(!title_advance_pressed(held(Buttons::START)));
        assert!(!title_advance_pressed(pressed(Buttons::B)));
        assert!(!title_advance_pressed(pressed(Buttons::SELECT)));
    }

    /// I-3 scene-flow test: title screen, A or Start newly pressed -> main
    /// menu. Needs the real pack (both `TitleScene` and
    /// `main_menu::load_default` read from it).
    #[test]
    #[ignore = "needs a local pack: run `cargo xtask extract` first"]
    fn title_a_or_start_button_transitions_to_main_menu() {
        for button in [Buttons::START, Buttons::A] {
            let title_scene =
                crate::title::load_default().expect("run `cargo xtask extract` first");
            let scene = AppScene::Title(Box::new(AnimatedTitle {
                scene: title_scene,
                tick: 0,
                presented: false,
            }));

            let (next, _frame) = advance_scene(scene, pressed(button));

            assert!(
                matches!(next, AppScene::MainMenu(_)),
                "{button:?} on the title screen must transition to the main menu"
            );
        }
    }

    /// I-3 scene-flow test: title screen, no advance press -> stays on title
    /// and keeps animating (the pre-I-3 animated-title behaviour must
    /// survive the state-machine refactor unchanged).
    #[test]
    #[ignore = "needs a local pack: run `cargo xtask extract` first"]
    fn title_without_start_stays_on_title_and_keeps_animating() {
        let title_scene = crate::title::load_default().expect("run `cargo xtask extract` first");
        let scene = AppScene::Title(Box::new(AnimatedTitle {
            scene: title_scene,
            tick: 0,
            presented: true, // as if this were the second frame onward.
        }));

        let (next, _frame) = advance_scene(scene, ButtonState::new());

        let AppScene::Title(title) = next else {
            panic!("expected to stay on the title screen");
        };
        assert_eq!(title.tick, 1, "the tick must still advance every frame");
    }

    /// I-3 scene-flow test: main menu, `NEW GAME` selected (the default,
    /// per `crate::main_menu`'s module docs) and A newly pressed -> intro.
    #[test]
    #[ignore = "needs a local pack: run `cargo xtask extract` first"]
    fn main_menu_confirm_on_new_game_transitions_to_intro() {
        let menu = crate::main_menu::load_default().expect("run `cargo xtask extract` first");
        let scene = AppScene::MainMenu(Box::new(menu));

        let (next, _frame) = advance_scene(scene, pressed(Buttons::A));

        assert!(
            matches!(next, AppScene::Intro(_)),
            "A on NEW GAME must transition to the intro"
        );
    }

    /// Issue #216 regression: A on `OPTION` must stay on the main menu, not
    /// incorrectly launch the intro -- its own destination screen is a
    /// separate, not-yet-built slice (`crate::flow::advance_scene`'s
    /// `MainMenuItem::Option` arm). No local pack needed: this never
    /// reaches a pack-loading call at all (`crate::main_menu::synthetic_scene`'s
    /// own doc comment).
    #[test]
    fn main_menu_confirm_on_option_stays_on_the_main_menu() {
        let mut menu = crate::main_menu::synthetic_scene();
        menu.move_down();
        assert_eq!(menu.selected(), crate::main_menu::MainMenuItem::Option);
        let scene = AppScene::MainMenu(Box::new(menu));

        let (next, _frame) = advance_scene(scene, pressed(Buttons::A));

        let AppScene::MainMenu(menu) = next else {
            panic!("A on OPTION must not leave the main menu");
        };
        assert_eq!(menu.selected(), crate::main_menu::MainMenuItem::Option);
    }

    /// Issue #216 regression: `DPAD_UP`/`DPAD_DOWN`, newly pressed, move the
    /// main menu's selection -- no pack needed (module docs on the test
    /// above).
    #[test]
    fn main_menu_up_and_down_move_the_selection() {
        let menu = crate::main_menu::synthetic_scene();
        let scene = AppScene::MainMenu(Box::new(menu));

        let (after_down, _frame) = advance_scene(scene, pressed(Buttons::DOWN));
        let AppScene::MainMenu(menu) = after_down else {
            panic!("expected to stay on the main menu");
        };
        assert_eq!(menu.selected(), crate::main_menu::MainMenuItem::Option);

        let scene = AppScene::MainMenu(menu);
        let (after_up, _frame) = advance_scene(scene, pressed(Buttons::UP));
        let AppScene::MainMenu(menu) = after_up else {
            panic!("expected to stay on the main menu");
        };
        assert_eq!(menu.selected(), crate::main_menu::MainMenuItem::NewGame);
    }

    /// I-3 scene-flow test: intro finishing (here, via the skip path --
    /// `crate::intro`'s module docs on why B skips the whole intro) hands
    /// off to the overworld with the player placed at the upstream spawn
    /// tile (`crate::new_game::SPAWN_POSITION`), not left at `(0, 0)` or
    /// wherever the intro's own defaults would otherwise leave it.
    #[test]
    #[ignore = "needs a local pack: run `cargo xtask extract` first"]
    fn intro_skip_transitions_to_overworld_with_the_player_at_the_spawn_tile() {
        let intro_scene = crate::intro::load_default().expect("run `cargo xtask extract` first");
        let scene = AppScene::Intro(Box::new(intro_scene));

        let (next, _frame) = advance_scene(scene, pressed(Buttons::B));

        let AppScene::Overworld(phase) = next else {
            panic!("expected the skipped intro to hand off to the overworld");
        };
        assert_eq!(phase.player.position(), new_game::SPAWN_POSITION);
        assert_eq!(phase.player.elevation(), new_game::SPAWN_ELEVATION);
        assert_eq!(phase.player.facing(), new_game::SPAWN_FACING);
        assert_eq!(phase.map_id, new_game::SPAWN_MAP_ID);

        // Finding 1: the transition must actually call
        // `new_game::init_save_blocks_for_new_game` and retain its result,
        // not just the player's in-memory position -- pin the same
        // `NewGameInitData` effects `crate::new_game`'s own tests already
        // check against `init_save_blocks` directly.
        assert_eq!(phase.save1.money, new_game::STARTING_MONEY);
        assert_eq!(phase.save1.player_party_count, 0);
        assert_eq!(phase.save1.bag, engine::save::Bag::default());
        assert_eq!(phase.save1.location.map_group, new_game::SPAWN_MAP_GROUP);
        assert_eq!(phase.save1.location.map_num, new_game::SPAWN_MAP_NUM);
        assert_eq!(phase.save2.player_gender, new_game::DEFAULT_PLAYER_GENDER);
        assert_eq!(phase.save2.encryption_key, 0);
    }

    /// I-3 scene-flow test: the intro's own paged advance-on-confirm (not
    /// just the skip shortcut) also reaches the overworld once every page
    /// is read. Confirms every tick; `IntroScene`'s own headless tests
    /// (`crate::intro::tests`) already cover the finer per-page timing.
    #[test]
    #[ignore = "needs a local pack: run `cargo xtask extract` first"]
    fn intro_finishing_every_page_also_transitions_to_the_overworld() {
        let mut intro_scene =
            crate::intro::load_default().expect("run `cargo xtask extract` first");
        let mut status = IntroStatus::Continue;
        for _ in 0..20_000 {
            status = intro_scene.tick(true, false);
            if status == IntroStatus::Finished {
                break;
            }
        }
        assert_eq!(status, IntroStatus::Finished, "the intro must terminate");

        let scene = AppScene::Intro(Box::new(intro_scene));
        let (next, _frame) = advance_scene(scene, ButtonState::new());
        assert!(matches!(next, AppScene::Overworld(_)));
    }
}
