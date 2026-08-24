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
//!
//! # Where the save file enters the machine (I-6, issue #214)
//!
//! Two points, both taking the session's [`SaveSlot`] as an argument rather
//! than reaching for a global `(oop-boundaries)`:
//!
//! * **Read**, on `Title` -> `MainMenu`: [`advance_scene`] loads the save and
//!   [`menu_type_for`] turns its `gSaveFileStatus` into the item list to
//!   show, so a valid save produces upstream's `HAS_SAVED_GAME` menu with
//!   `CONTINUE` on it while anything else falls back to `HAS_NO_SAVED_GAME`.
//!   The loaded blocks ride along in [`MainMenuState`] until
//!   [`MainMenuAction::Continue`] hands them to
//!   [`OverworldPhase::continue_saved_game`].
//! * **Write**, from the field start menu (I-6, issue #232): `START` in
//!   the overworld opens [`crate::start_menu`], and its `SAVE` action runs
//!   upstream's own confirm/overwrite chain before calling
//!   [`SaveSlot::store`] and friends.
//!   [`advance_scene`]'s `Overworld` arm drives that through
//!   [`OverworldPhase::advance_start_menu_frame`], which owns the frame
//!   whenever a menu is open. There is deliberately **no** save on exit:
//!   upstream writes nothing when the lid closes, and issue #214's
//!   `save_on_exit` stand-in existed only until this flow landed.

use engine::text::render::PrinterInput;
use platform::{ButtonState, Buttons, Frame};

use crate::frame::to_platform_frame;
use crate::game_save::{SaveSlot, SavedGame};
use crate::intro::{self, IntroScene, IntroStatus};
use crate::main_menu::{self, MainMenuItem, MainMenuScene, MainMenuType};
use crate::title::TitleScene;

pub(crate) mod first_battle;
/// The level-up move-replacement decision (issue #304), answered in one
/// place for all three headless battle drivers — see that module's docs for
/// the stand-in answer it gives and exactly where the seam ends.
mod move_learn;
/// The shared `CreateNPCTrainerParty` construction and headless one-turn
/// driver every NPC trainer battle in this crate goes through (issue #237,
/// generalized off [`route103_rival`] by issue #264): both
/// [`route103_rival`]'s own scripted rival and
/// [`overworld_phase`]'s `sight_trainer_trigger` (Route 103's Daisy, Andrew,
/// Miguel, Rhett, Marcos, Isabelle, Pete) build their battles through this
/// module.
pub(crate) mod npc_trainer_battle;
pub(crate) mod overworld_phase;
/// The scripted Route 103 rival battle's own choice of trainer, reachable
/// from real play since issue #248: [`overworld_phase`]'s own
/// `route103_rival_trigger` (beside its existing `first_battle_trigger` and,
/// since issue #264, `sight_trainer_trigger`) reads this module's
/// [`route103_rival::route103_rival_for`] table, then calls
/// [`npc_trainer_battle::start_npc_trainer_battle`]/
/// [`npc_trainer_battle::advance_npc_trainer_battle`] directly -- the same
/// reachability slice that retired [`first_battle`]'s own former
/// `#[allow(dead_code)]` between issues #221 and #231. This module carried
/// thin pass-throughs over those two functions (plus a `RivalBattleError`
/// alias) from issue #264 until issue #347 retired them: neither wrapper
/// held any Route-103-specific behaviour, so [`npc_trainer_battle`] is the
/// one place both this module's own trigger and
/// [`overworld_phase`]'s `sight_trainer_trigger` build their battles now.
pub(crate) mod route103_rival;
mod wild_encounter;
pub(crate) use overworld_phase::OverworldPhase;

/// I-6 (issue #214) save/exit/reload round-trip tests -- their own file
/// rather than more cases in `tests` below, because they exercise the whole
/// `overworld -> file -> main menu -> overworld` loop rather than one scene
/// transition (`one module = one concept` `(oop-boundaries)`).
#[cfg(test)]
mod save_continue_tests;

/// The field start menu's own behaviour (I-6, issue #232) -- the `START`
/// gate, frame ownership, and the close paths that write nothing. Split
/// from [`save_continue_tests`] for the same reason that file split from
/// [`tests`]: one module, one concept `(oop-boundaries)`.
#[cfg(test)]
mod start_menu_tests;

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
    /// The main menu (I-3, issue #216; I-6, issue #214): Up/Down move the
    /// selection, A confirms it (module docs on [`advance_scene`]'s
    /// `MainMenu` arm for what each item does).
    MainMenu(Box<MainMenuState>),
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

/// [`AppScene::MainMenu`]'s state: the rendered menu, plus the save the boot
/// load recovered (I-6, issue #214).
///
/// The two travel together because upstream keeps them together in globals
/// it can reach from anywhere: `Task_MainMenuCheckSaveFile` picks
/// `tMenuType` from `gSaveFileStatus` (`main_menu.c:641-670`) and
/// `CB2_ContinueSavedGame` then resumes from `gSaveBlock1Ptr`/`gSaveBlock2Ptr`
/// (`overworld.c:1705-1754`). This port has no globals `(oop-boundaries)`,
/// so the loaded blocks are owned by the scene that will hand them on.
pub(crate) struct MainMenuState {
    /// The composed menu, showing whichever [`MainMenuType`] `saved`'s
    /// status selected.
    pub(crate) scene: MainMenuScene,
    /// What the boot load recovered -- `gSaveFileStatus` plus the blocks
    /// [`MainMenuItem::Continue`] resumes from.
    pub(crate) saved: SavedGame,
}

/// `Task_MainMenuCheckSaveFile`'s `tMenuType` assignment
/// (`main_menu.c:641-670`), as a pure decision the `Title` -> `MainMenu`
/// transition acts on -- so which menu a given boot verdict produces is
/// pinned by a pack-less test rather than being observable only through a
/// composed frame.
const fn menu_type_for(saved: &SavedGame) -> MainMenuType {
    if saved.status.menu_shows_continue() {
        MainMenuType::SavedGame
    } else {
        MainMenuType::NoSavedGame
    }
}

/// Which of upstream's `ACTION_*` values confirming `item` maps to
/// (`HandleMainMenuInput`'s per-menu-type action switch,
/// `main_menu.c:955-983`), restricted to the actions this port can perform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MainMenuAction {
    /// `ACTION_CONTINUE` (`main_menu.c:1064-1069`): resume the loaded save.
    Continue,
    /// `ACTION_NEW_GAME` (`:1055-1063`): start the Birch-speech intro.
    NewGame,
    /// No action this port performs -- `ACTION_OPTION` (`:1070`), whose
    /// settings screen is not built (see [`crate::main_menu`]'s scope
    /// notes). The press is swallowed rather than incorrectly launching
    /// something else.
    None,
}

/// What confirming (A) `item` on the main menu does, as a pure decision
/// [`advance_scene`]'s `AppScene::MainMenu` arm acts on.
///
/// Split out of the scene arm so the mapping is testable without a pack:
/// inside the arm, a swallowed press, a failed [`intro::load_default`], and
/// a failed [`OverworldPhase::continue_saved_game`] all fall through to
/// "stay on the main menu", which no pack-less test can tell apart.
const fn menu_action(item: MainMenuItem) -> MainMenuAction {
    match item {
        MainMenuItem::Continue => MainMenuAction::Continue,
        MainMenuItem::NewGame => MainMenuAction::NewGame,
        MainMenuItem::Option => MainMenuAction::None,
    }
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
/// [`OverworldPhase::load_default`] this frame -- only on a *fresh* A or B
/// edge, the same two dialogue-advance buttons [`AppScene::Intro`] itself
/// reads (module docs on the finding this guards against: the previous
/// behaviour re-attempted, and re-logged, the load every single frame while
/// stuck here, since `IntroStatus::Finished` is sticky and was the only
/// condition gating the attempt).
fn should_retry_overworld_load(buttons: ButtonState) -> bool {
    buttons.is_newly_pressed(Buttons::A) || buttons.is_newly_pressed(Buttons::B)
}

/// Narrow a frame's real [`ButtonState`] down to the four bits
/// [`IntroScene::tick`] needs (`engine::text::render::PrinterInput`'s own
/// docs on why `engine` can't just take a [`ButtonState`] directly).
///
/// Issue #393: B is an ordinary dialogue-advance button here, exactly like
/// A -- upstream's own `JOY_NEW`/`JOY_HELD(A_BUTTON | B_BUTTON)`
/// (`pokeemerald/src/text.c:874-879`, `:943-953`) never distinguish which
/// button did it, so neither does this. There is no separate whole-intro
/// skip input anymore; the pre-1.0 `skip_pressed` this replaced is gone.
fn intro_printer_input(buttons: ButtonState) -> PrinterInput {
    PrinterInput {
        a_pressed: buttons.is_newly_pressed(Buttons::A),
        b_pressed: buttons.is_newly_pressed(Buttons::B),
        a_held: buttons.is_held(Buttons::A),
        b_held: buttons.is_held(Buttons::B),
    }
}

/// Log the one-time proof that `phase`'s fresh save state
/// ([`OverworldPhase::load_default`]'s own doc comment, finding 1 of this
/// module's review pass) actually reached the `Intro` -> `Overworld`
/// handoff -- the same "log-or-ignore is fine" pipeline-liveness style
/// [`crate::app::describe_newly_pressed`] already uses for input.
fn log_new_game_started(phase: &OverworldPhase) {
    eprintln!(
        "new game: money={} trainer_id={:02x?} gender={:?}",
        phase.save1().money,
        phase.save2().player_trainer_id,
        phase.save2().player_gender,
    );
}

/// [`log_new_game_started`]'s counterpart for a resumed session (I-6): the
/// one-time proof that a `CONTINUE` really restored the *saved* location and
/// money rather than silently re-running new-game initialization.
fn log_game_continued(phase: &OverworldPhase) {
    eprintln!(
        "continue: map={:?} pos=({}, {}) facing={:?} money={}",
        phase.map_id,
        phase.save1().pos.x,
        phase.save1().pos.y,
        phase.player.facing(),
        phase.save1().money,
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
///
/// `save_slot` is this session's save medium ([`SaveSlot`]). The `Title` ->
/// `MainMenu` transition reads it (upstream's boot-time
/// `LoadGameSave(SAVE_NORMAL)`, deferred to here because this port has no
/// pre-title copyright screen to run it on -- see the `MainMenu` arm), and
/// the `Overworld` arm hands it to
/// [`OverworldPhase::advance_start_menu_frame`], the *only* write side
/// (module docs).
pub(crate) fn advance_scene(
    scene: AppScene,
    buttons: ButtonState,
    save_slot: &mut SaveSlot,
) -> (AppScene, Box<Frame>) {
    match scene {
        AppScene::Title(mut title) => {
            if title.presented {
                title.tick = title.tick.wrapping_add(1);
            }
            title.presented = true;

            if title_advance_pressed(buttons) {
                // Upstream reads the save before the title screen ever
                // draws, in `CB2_InitCopyrightScreenAfterBootup`
                // (`src/intro.c:1147-1159`), and parks the verdict in
                // `gSaveFileStatus` for `Task_MainMenuCheckSaveFile` to
                // branch on later. This port has no copyright screen and no
                // globals `(oop-boundaries)`, so the load happens at the one
                // moment its result is first needed -- building the menu --
                // and travels onward as a value in `MainMenuState`. The
                // observable result is identical: the menu shown is the one
                // the save on disk selects.
                let saved = save_slot.load();
                match main_menu::load_default(menu_type_for(&saved)) {
                    Ok(menu) => {
                        let state = MainMenuState { scene: menu, saved };
                        let frame = state.scene.compose_frame();
                        return (AppScene::MainMenu(Box::new(state)), frame);
                    }
                    // The hint covers both failure shapes operators actually
                    // hit: no pack extracted at all (`PackError::NotFound`),
                    // and -- easy to mistake for a code bug -- a pack
                    // extracted before this screen existed, whose directory
                    // has no `interface/palette/main_menu_bg` entry
                    // (`PackError::UnknownAsset`). Both are fixed by
                    // re-extracting; neither changes what the error *is*.
                    Err(err) => eprintln!(
                        "main menu: {err} -- staying on the title screen; \
                         re-run `cargo xtask extract` (a pack extracted before \
                         this screen existed is missing its entries)"
                    ),
                }
            }

            let frame = to_platform_frame(&title.scene.compose(title.tick));
            (AppScene::Title(title), frame)
        }
        AppScene::MainMenu(mut state) => {
            if buttons.is_newly_pressed(Buttons::A) {
                // A pure decision, not an inline match, so the item ->
                // action mapping is pinned by a pack-less test -- see
                // `menu_action`'s own doc comment.
                match menu_action(state.scene.selected()) {
                    MainMenuAction::NewGame => match intro::load_default() {
                        Ok(intro_scene) => {
                            let frame = intro_scene.compose_frame();
                            return (AppScene::Intro(Box::new(intro_scene)), frame);
                        }
                        Err(err) => eprintln!("intro: {err} -- staying on the main menu"),
                    },
                    // `ACTION_CONTINUE` (`main_menu.c:1064-1069`) ->
                    // `CB2_ContinueSavedGame`. The blocks are moved out of
                    // the menu state by cloning rather than by consuming it,
                    // because a failed continue must leave the menu exactly
                    // as it was -- still offering `CONTINUE`, still holding
                    // the save it could not resume.
                    MainMenuAction::Continue => {
                        let (block1, block2) =
                            (state.saved.block1.clone(), state.saved.block2.clone());
                        match OverworldPhase::continue_saved_game(block1, block2) {
                            Ok(phase) => {
                                log_game_continued(&phase);
                                let frame = phase.compose_frame();
                                return (AppScene::Overworld(Box::new(phase)), frame);
                            }
                            Err(err) => eprintln!("{err} -- staying on the main menu"),
                        }
                    }
                    MainMenuAction::None => {}
                }
            } else if buttons.is_newly_pressed(Buttons::UP) {
                state.scene.move_up();
            } else if buttons.is_newly_pressed(Buttons::DOWN) {
                state.scene.move_down();
            }
            let frame = state.scene.compose_frame();
            (AppScene::MainMenu(state), frame)
        }
        // Issue #393: B is an ordinary dialogue-advance button here, not a
        // whole-intro skip -- `intro_printer_input`'s own doc comment.
        AppScene::Intro(mut intro_scene) => {
            let status = intro_scene.tick(intro_printer_input(buttons));

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
            // The field start menu runs ahead of movement and owns the
            // frame outright when it is open -- upstream's own ordering
            // (`ProcessPlayerFieldInput` before `PlayerStep`, returning
            // TRUE out of its `pressedStartButton` branch). This is the
            // only point in the whole state machine that hands the save
            // medium to something that can write it.
            if !phase.advance_start_menu_frame(buttons, save_slot) {
                phase.step(buttons);
            }
            let frame = phase.compose_frame();
            (AppScene::Overworld(phase), frame)
        }
    }
}

#[cfg(test)]
mod tests;
