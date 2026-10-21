//! The real (windowed) game-flow state machine (I-3): title ->
//! main menu -> intro -> overworld.
//!
//! [`crate::app`] stays focused on the platform/window shell (`one module =
//! one concept` `(oop-boundaries)`): [`AppScene`] is the state, [`advance_scene`] is the one-frame transition
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
//! # Waiting for the front-end palette fade (I-3)
//!
//! `Title` -> `MainMenu` and a `MainMenu` confirm on `NEW GAME`/`CONTINUE`
//! hold the outgoing frame through a [`rendering::NormalPaletteFade`]
//! before the destination loads, matching upstream's own
//! `BeginNormalPaletteFade`/`UpdatePaletteFade` gate
//! (`pokeemerald/src/title_screen.c:779-787,824-828`;
//! `main_menu.c:884-894,936-943`) -- see [`AppScene::TitleFadeWait`] and
//! [`AppScene::MainMenuFadeWait`] for the waiting states themselves.
//!
//! [`AnimatedTitle`] is the per-frame title-animation state, owned by one
//! variant of [`AppScene`] (see [`advance_scene`]'s `Title` arm and
//! `crate::app`'s "Animating the real title screen" docs).
//!
//! # Where the save file enters the machine (I-6)
//!
//! Two points, both taking the session's [`SaveSlot`] as an argument rather
//! than reaching for a global `(oop-boundaries)`:
//!
//! * **Read**, at boot, before the title ([`AnimatedTitle::boot`], as
//!   upstream's copyright screen does): the title scenes carry the loaded save,
//!   and on `Title` -> `MainMenu` [`menu_type_for`] turns its `gSaveFileStatus` into the item list to
//!   show, so a valid save produces upstream's `HAS_SAVED_GAME` menu with
//!   `CONTINUE` on it while anything else falls back to `HAS_NO_SAVED_GAME`.
//!   The loaded blocks ride along in [`MainMenuState`] until
//!   [`MainMenuAction::Continue`] hands them to
//!   [`OverworldPhase::continue_saved_game`].
//! * **Write**, from the field start menu: `START` in
//!   the overworld opens [`crate::start_menu`], and its `SAVE` action runs
//!   upstream's own confirm/overwrite chain before calling
//!   [`SaveSlot::store`] and friends.
//!   [`advance_scene`]'s `Overworld` arm drives that through
//!   [`OverworldPhase::advance_start_menu_frame`], which owns the frame
//!   whenever a menu is open. There is deliberately **no** save on exit:
//!   upstream writes nothing when the lid closes.

use engine::save::ButtonMode;
use engine::settings::GameSettings;
use engine::text::render::PrinterInput;
use platform::{ButtonState, Buttons, Frame};

use crate::game_save::{SaveSlot, SavedGame};
use crate::intro::{self, IntroScene, IntroStatus};
use crate::main_menu::{self, MainMenuItem, MainMenuScene, MainMenuSceneError, MainMenuType};
use crate::new_game::NewGameOptions;
use crate::title::TitleScene;
use fade_wait::{
    advance_main_menu, advance_main_menu_fade_in, advance_main_menu_fade_wait, advance_title,
    advance_title_fade_wait, MainMenuFadeIn, MainMenuFadeWait, TitleFadeWait,
};
use options::{advance_options, OptionsState};

/// The battle-turn finalization (issue #405) shared by all three headless
/// battle drivers -- see that module's docs for the shared decision and
/// write-back, and each driver's own docs for what stays driver-specific.
mod battle_finalize;
/// I-3 (issue #1329) front-end palette fade-wait states and the four
/// [`advance_scene`] arms that drive them -- this module's own docs.
mod fade_wait;
pub(crate) mod first_battle;
/// The first-usable-move headless action policy shared by [`first_battle`]
/// and [`npc_trainer_battle`] -- see that module's docs for the RNG-state
/// contract driving it.
mod first_usable_move;
/// The level-up move-replacement decision (issue #304), answered in one
/// place for all three headless battle drivers — see that module's docs for
/// the stand-in answer it gives and exactly where the seam ends.
mod move_learn;
/// The shared `CreateNPCTrainerParty` construction and headless one-turn
/// driver every NPC trainer battle in this crate goes through: both
/// [`route103_rival`]'s own scripted rival and
/// [`overworld_phase`]'s `sight_trainer_trigger` (Route 103's Daisy, Andrew,
/// Miguel, Rhett, Marcos, Isabelle, Pete) build their battles through this
/// module.
pub(crate) mod npc_trainer_battle;
mod options;
#[cfg(test)]
mod options_tests;

pub(crate) mod overworld_phase;
/// The scripted Route 103 rival battle's own choice of trainer:
/// [`overworld_phase`]'s `route103_rival_trigger` (beside its
/// `first_battle_trigger` and `sight_trainer_trigger`) reads this module's
/// [`route103_rival::route103_rival_for`] table, then calls
/// [`npc_trainer_battle::start_npc_trainer_battle`]/
/// [`npc_trainer_battle::advance_npc_trainer_battle`] directly. This module
/// holds no battle driver of its own: [`npc_trainer_battle`] is the one
/// place both this module's own trigger and [`overworld_phase`]'s
/// `sight_trainer_trigger` build their battles.
pub(crate) mod route103_rival;
/// The boot-loaded button mode on the title scenes (I-3, issue #2491).
#[cfg(test)]
mod title_button_mode_tests;
mod wild_encounter;
pub(crate) use overworld_phase::OverworldPhase;

/// Shared save/continue fixtures and drivers for the sibling `save_continue_*_tests` modules.
#[cfg(test)]
mod save_continue_support;

/// Elevation-history and stale-image regressions across save and continue.
#[cfg(test)]
mod save_continue_elevation_tests;

/// I-6 (issue #214) save/exit/reload round-trip tests: the whole `overworld -> file -> main menu -> overworld` loop.
#[cfg(test)]
mod save_continue_round_trip_tests;

/// Party preservation across save and continue.
#[cfg(test)]
mod save_continue_party_tests;

/// Repeated saves within one session file the same bytes.
#[cfg(test)]
mod save_continue_hp_tests;

/// Continue-warp selection and failure handling.
#[cfg(test)]
mod save_continue_warp_tests;

/// `#[ignore]`d real-pack lane for continue-time room loading, warps and gender.
#[cfg(test)]
mod save_continue_real_pack_tests;

/// Legacy/malformed-save migrations on continue: facing, elevation, heal
/// location, and fainted-lead fallbacks.
#[cfg(test)]
mod save_continue_migration_tests;

/// Overwrite confirmation, corruption fallback, and the foreign-save
/// consent gate (issue #232).
#[cfg(test)]
mod save_continue_overwrite_tests;

/// Issue #353's undecodable-slot retention: a party slot whose secure
/// region fails its own checksum.
#[cfg(test)]
mod save_continue_retention_tests;

/// The saved `optionsTextSpeed`'s start-menu message pacing.
#[cfg(test)]
mod save_continue_text_speed_tests;

/// Issue #795's main-menu window-frame chrome for a continuable save.
#[cfg(test)]
mod save_continue_window_frame_tests;

/// The field start menu's own behaviour (I-6, issue #232) -- the `START`
/// gate, frame ownership, and the close paths that write nothing. Split
/// from [`save_continue_round_trip_tests`] for the same reason that file split from
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
    /// The save the boot load recovered, boot defaults applied. Upstream reads
    /// it during the copyright screen (`pokeemerald/src/intro.c:1147-1156`),
    /// before the title screen and its `ReadKeys` (`main.c:279-288`), so the
    /// title scenes carry its `optionsButtonMode` and the `Title` ->
    /// `MainMenu` handoff hands the same blocks on instead of loading again.
    pub(crate) saved: SavedGame,
}

impl AnimatedTitle {
    /// The fresh, un-presented title at tick 0, after the boot load of
    /// `save_slot` (module docs on [`AnimatedTitle::saved`]).
    pub(crate) fn boot(scene: TitleScene, save_slot: &mut SaveSlot) -> Self {
        let mut saved = save_slot.load();
        apply_boot_defaults(&mut saved);
        Self {
            scene,
            tick: 0,
            presented: false,
            saved,
        }
    }
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
    /// Waiting for the title's white front-end palette fade before
    /// [`title_to_main_menu`] runs (I-3, issue #1329: module docs' "Waiting
    /// for the front-end palette fade" section). [`AnimatedTitle::tick`]
    /// stays fixed here -- the retained title stops animating, matching
    /// upstream parking `CB2_GoToMainMenu` on the fade rather than the
    /// title task.
    TitleFadeWait(Box<TitleFadeWait>),
    /// The main menu (I-3, issue #216; I-6, issue #214): Up/Down move the
    /// selection, A confirms it (module docs on [`advance_scene`]'s
    /// `MainMenu` arm for what each item does).
    MainMenu(Box<MainMenuState>),
    /// The highlight-only frame after a successful Up/Down (I-3, issue
    /// #2223): upstream's navigation task only schedules a callback that
    /// redraws the new highlight and reinstalls input without polling
    /// (`pokeemerald/src/main_menu.c:878-881`, `:930-933`;
    /// `pokeemerald/src/task.c:118-120`), so this step presents the new
    /// highlight and ignores every button before [`AppScene::MainMenu`]
    /// resumes.
    MainMenuHighlight(Box<MainMenuState>),
    /// Waiting for the main menu's black front-end palette fade before the
    /// [`MainMenuAction`] a fresh A confirmed on `NEW GAME`/`CONTINUE`
    /// dispatches (I-3, issue #1329: module docs' "Waiting for the
    /// front-end palette fade" section). `OPTION` enters it too, dispatching to
    /// the Options screen.
    MainMenuFadeWait(Box<MainMenuFadeWait>),
    /// The Options screen, opened from the main menu's `OPTION` item (I-6):
    /// it retains the menu it returns to.
    Options(Box<OptionsState>),
    /// The main menu fading in from black after the Options screen returns
    /// (`pokeemerald/src/main_menu.c:583-586`).
    MainMenuFadeIn(Box<MainMenuFadeIn>),
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

impl AppScene {
    /// The `optionsButtonMode` this scene's save carries
    /// (`pokeemerald/src/main.c:279-288`'s source). The title scenes carry the
    /// boot load's save ([`AnimatedTitle::saved`]), as upstream reads it during
    /// the copyright screen (`intro.c:1147-1156`). The intro carries the mode in its [`NewGameOptions`] rather than a `SaveBlock2`.
    fn button_mode(&self) -> ButtonMode {
        match self {
            Self::Title(title) => title.saved.block2.options_button_mode,
            Self::TitleFadeWait(wait) => wait.button_mode(),
            Self::MainMenu(state) | Self::MainMenuHighlight(state) => {
                state.saved.block2.options_button_mode
            }
            Self::MainMenuFadeWait(wait) => wait.state.saved.block2.options_button_mode,
            Self::Options(state) => state.button_mode(),
            Self::MainMenuFadeIn(fade) => fade.button_mode(),
            Self::Intro(intro) | Self::OverworldLoadFailed(intro) => {
                intro.new_game_options().button_mode
            }
            Self::Overworld(phase) => phase.button_mode(),
        }
    }
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

/// `SetDefaultOptions` (`new_game.c:91-98`) for the boot verdicts that clear
/// `SaveBlock2` -- exactly [`crate::game_save::SaveFileStatus::boot_clears_save_block2`]'s
/// `Empty`/`Corrupt` cases (`pokeemerald/src/intro.c:1154-1156`), even when a
/// checksum-valid block was recovered. Run once, as the boot load lands in
/// [`title_to_main_menu`], so the block the Options screen edits is the live
/// one: upstream defaults only at boot, never again on each read.
fn apply_boot_defaults(saved: &mut SavedGame) {
    if saved.status.boot_clears_save_block2() {
        GameSettings::from(NewGameOptions::DEFAULT).write_to_save(&mut saved.block2);
    }
}

/// `gSaveBlock2Ptr->optionsWindowFrameType` (`main_menu.c:2191-2193`): the
/// live block's frame, which [`apply_boot_defaults`] has already defaulted
/// for a boot that cleared it and the Options screen may since have changed.
const fn window_frame_for(saved: &SavedGame) -> u8 {
    saved.block2.options_window_frame_type
}

/// The [`NewGameOptions`] a NEW GAME confirm on `saved` hands onward to
/// [`intro::load`] and, once the intro finishes,
/// [`crate::new_game::init_save_blocks_with_options`] (issue #1125): the live
/// block's six options.
///
/// `NewGameInitData` never resets any `options*` field itself
/// (`pokeemerald/src/new_game.c:149-207`), so whatever `saved.block2` holds
/// -- the recovered save's own preferences, [`apply_boot_defaults`]' defaults
/// for an `Empty`/`Corrupt` boot, or an edit the Options screen committed
/// since -- carries forward. `NoFlash` is not a rescued block:
/// [`crate::game_save::SaveSlot::load`] synthesizes a zero-filled
/// `SaveBlock2` for it, and carrying those zero bytes forward still mirrors
/// a genuine `SAVE_STATUS_NO_FLASH` boot, which leaves `gSaveBlock2Ptr`
/// untouched (`pokeemerald/src/save.c:871-889`).
fn new_game_options_for(saved: &SavedGame) -> NewGameOptions {
    GameSettings::from_save(&saved.block2).into()
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
    /// `ACTION_OPTION` (`:1070-1073`): open the Options screen, which
    /// returns to this menu.
    Options,
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
        MainMenuItem::Option => MainMenuAction::Options,
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

/// Format the recovery log line the `Title` -> `MainMenu` transition emits
/// for `err`, without re-adding [`MainMenuSceneError`]'s own `main menu: `
/// prefix (its `Display` impl, `main_menu.rs`, already writes it).
///
/// Kept pure (no I/O) so it is unit-testable; [`title_to_main_menu`]'s error
/// arm is the only caller. The hint covers both failure shapes operators
/// actually hit: no pack built at all (`PackError::NotFound`), and -- easy
/// to mistake for a code bug -- a pack built before this screen existed,
/// whose directory has no `interface/palette/main_menu_bg` entry
/// (`PackError::UnknownAsset`). Both are fixed by rebuilding the pack by
/// whichever route built it in the first place -- a player has no decomp
/// checkout to extract from; neither changes what the error *is*.
fn main_menu_load_failure_message(err: &MainMenuSceneError) -> String {
    format!(
        "{err} -- staying on the title screen; a pack built before this \
         screen existed is missing its entries: players rebuild it with \
         `pokeemerald-rs --import-rom <path to your Pokemon Emerald (US) \
         ROM>`, developers with `cargo xtask extract`"
    )
}

/// The `Title` -> `MainMenu` half of [`advance_scene`]'s
/// `AppScene::TitleFadeWait` arm, split out so that arm stays under the
/// lint's line budget `(oop-boundaries)`: builds the menu the title's
/// boot-loaded save selects, bordered with that save's window frame, and
/// returns the next scene and its first frame, handing the save on in
/// `MainMenuState`. Upstream reads the save before the title ever draws
/// (`CB2_InitCopyrightScreenAfterBootup`, `src/intro.c:1147-1159`) and parks
/// the verdict in `gSaveFileStatus` for `Task_MainMenuCheckSaveFile`; here
/// [`AnimatedTitle::boot`] does that read and the title carries the result.
///
/// Returns the title back as `Err`, after logging, if the pack load fails --
/// the caller stays on the title screen.
fn title_to_main_menu(
    pack_source: &crate::pack_source::PackSource,
    title: Box<AnimatedTitle>,
) -> Result<(AppScene, Box<Frame>), Box<AnimatedTitle>> {
    let Some(menu) = load_menu_for(pack_source, &title.saved) else {
        return Err(title);
    };
    let AnimatedTitle { saved, .. } = *title;
    let state = MainMenuState { scene: menu, saved };
    let frame = state.scene.compose_frame();
    Ok((AppScene::MainMenu(Box::new(state)), frame))
}

/// The menu `saved` selects, bordered with its window frame, or `None` after
/// logging when the pack load fails.
fn load_menu_for(
    pack_source: &crate::pack_source::PackSource,
    saved: &SavedGame,
) -> Option<MainMenuScene> {
    match main_menu::load_with_window_frame(
        pack_source,
        menu_type_for(saved),
        window_frame_for(saved),
    ) {
        Ok(menu) => Some(menu),
        Err(err) => {
            eprintln!("{}", main_menu_load_failure_message(&err));
            None
        }
    }
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
/// their own pack independently) and composes that scene's first frame
/// immediately once it runs, so the returned frame is always the *new*
/// scene's -- never a stale one from the scene being left. `Title` ->
/// `MainMenu` and a `MainMenu` confirm now enter the front-end fade-wait
/// states first instead (module docs); `Intro` -> `Overworld` still loads
/// immediately (module docs' boundary note: no fade there). If a
/// transition's pack load fails, this logs and returns the *original*
/// scene unchanged instead (module docs) -- except `Intro` -> `Overworld`
/// specifically, whose failure instead moves to
/// [`AppScene::OverworldLoadFailed`] (module docs' exception, and that
/// variant's own doc comment) so the failed attempt isn't repeated every
/// frame.
///
/// `save_slot` is this session's save medium ([`SaveSlot`]). The boot load
/// ([`AnimatedTitle::boot`]) already read it before the title, so the `Title`
/// -> `MainMenu` transition hands that save on, and the `Overworld` arm hands it to
/// [`OverworldPhase::advance_start_menu_frame`], the *only* write side
/// (module docs).
///
/// `pack_source` is [`crate::App`]'s own retained
/// [`crate::pack_source::PackSource`] (issue #412), resolved once at
/// construction and passed down unchanged to every one of this function's
/// own pack loads -- and, for the `Overworld` arm, into the
/// [`OverworldPhase`] it builds, which retains it for every load *it*
/// performs afterwards (`crate::pack_source`'s own module docs).
pub(crate) fn advance_scene(
    scene: AppScene,
    buttons: ButtonState,
    save_slot: &mut SaveSlot,
    pack_source: crate::pack_source::PackSource,
) -> (AppScene, Box<Frame>) {
    // `ReadKeys` (`pokeemerald/src/main.c:279-288`) applies the saved L=A
    // mode before any scene reads input, so the projection lives here, ahead
    // of dispatch, using the mode the active scene's live save carries.
    let buttons = if scene.button_mode() == ButtonMode::LEqualsA {
        buttons.with_l_equals_a()
    } else {
        buttons
    };
    match scene {
        AppScene::Title(title) => advance_title(title, buttons),
        AppScene::TitleFadeWait(wait) => advance_title_fade_wait(wait, &pack_source),
        AppScene::MainMenu(state) => advance_main_menu(state, buttons),
        AppScene::MainMenuHighlight(state) => {
            let frame = state.scene.compose_frame();
            (AppScene::MainMenu(state), frame)
        }
        AppScene::MainMenuFadeWait(wait) => advance_main_menu_fade_wait(wait, pack_source),
        AppScene::Options(state) => advance_options(state, buttons, &pack_source),
        AppScene::MainMenuFadeIn(fade) => advance_main_menu_fade_in(fade),
        // Issue #393: B is an ordinary dialogue-advance button here, not a
        // whole-intro skip -- `intro_printer_input`'s own doc comment.
        AppScene::Intro(mut intro_scene) => {
            let status = intro_scene.tick(intro_printer_input(buttons));

            if status == IntroStatus::Finished {
                match OverworldPhase::load(
                    pack_source,
                    intro_scene.new_game_options(),
                    intro_scene.new_game_identity(),
                ) {
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
                match OverworldPhase::load(
                    pack_source,
                    intro_scene.new_game_options(),
                    intro_scene.new_game_identity(),
                ) {
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
            // An open menu owns the frame, as upstream's locked field
            // controls skip `ProcessPlayerFieldInput` while it is up; a
            // fresh `START` press is weighed inside `step`. This is the one
            // point that hands the save medium to something that writes it.
            //
            // `step` then `compose_frame` is the whole of one presented
            // frame, which is the boundary `step`'s own "Frame shape"
            // section reasons from (issue #1039).
            if phase.start_menu().is_some() {
                phase.advance_start_menu_frame(buttons, save_slot);
            } else {
                phase.step(buttons);
            }
            let frame = phase.compose_frame();
            (AppScene::Overworld(phase), frame)
        }
    }
}

#[cfg(test)]
mod tests;
