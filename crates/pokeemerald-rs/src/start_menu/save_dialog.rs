//! The start menu's SAVE flow (I-6, issue #232): upstream's
//! `sSaveDialogCallback` chain in `pokeemerald/src/start_menu.c:978-1160`,
//! re-expressed as one owned state machine `(oop-boundaries)` instead of a
//! function pointer in EWRAM.
//!
//! # The chain, and where each callback went
//!
//! | upstream callback | here |
//! |---|---|
//! | `InitSave` (`:877-882`) | [`SaveDialog::new`] |
//! | `RunSaveCallback` (`:884-894`) | [`SaveDialog::run`]'s printer gate |
//! | `SaveConfirmSaveCallback` (`:978-994`) | [`Step::ShowConfirm`] |
//! | `SaveYesNoCallback` (`:996-1001`) | [`Step::OpenConfirmMenu`] |
//! | `SaveConfirmInputCallback` (`:1003-1033`) | [`Step::ConfirmInput`] |
//! | `SaveFileExistsCallback` (`:1035-1047`) | [`Step::ShowFileExists`] |
//! | `SaveConfirmOverwrite*Callback` (`:1049-1061`) | [`Step::OpenOverwriteMenu`] |
//! | `SaveOverwriteInputCallback` (`:1063-1078`) | [`Step::OverwriteInput`] |
//! | `SaveSavingMessageCallback` (`:1080-1084`) | [`Step::ShowSaving`] |
//! | `SaveDoSaveCallback` (`:1086-1110`) | [`Step::DoSave`] |
//! | `SaveSuccessCallback` + `SaveReturnSuccessCallback` (`:1112-1134`) | [`Step::AwaitSuccess`] |
//! | `SaveErrorCallback` + `SaveReturnErrorCallback` (`:1136-1159`) | [`Step::AwaitError`] |
//!
//! Each pair of "show a message, then act on it" callbacks stays two
//! frames apart the way upstream's do, because `RunSaveCallback` refuses to
//! advance while the text printer is still running — [`SaveDialog::run`]'s
//! first branch is that refusal. That is not decoration: it is what keeps
//! the single A press that answered one prompt from also answering the
//! next.
//!
//! # Not modelled
//!
//! `ShowSaveInfoWindow`/`RemoveSaveInfoWindow` (`:1332-1420`), the
//! PLAYER/BADGES/POKéDEX/TIME panel that opens beside the prompt: three of
//! its four lines read save state this port has no typed home for (the same
//! gap [`crate::main_menu`] records for `MainMenu_FormatSavegameText`).
//! `IncrementGameStat(GAME_STAT_SAVED_GAME)`, the Battle Pyramid
//! rest/retire variants, `SoftResetInBattlePyramid`, and every `PlaySE`
//! (`SE_SELECT`/`SE_SAVE`/`SE_BOO`) are likewise absent — the sound engine
//! has no field-SFX caller yet, so the two waits that upstream gates on
//! `IsSEPlaying()` simply proceed.

use engine::text::format::{expand_placeholders, PlaceholderResolver};
use engine::text::{Token, PLACEHOLDER_PLAYER};
use platform::{ButtonState, Buttons};

use crate::game_save::SaveFileStatus;
use crate::overworld::{DialogOutcome, NpcDialog};

use super::text::SaveMessage;
use super::{StartMenuChrome, YesNoMenu};

/// `TrySavingData`'s `saveType` (`pokeemerald/include/constants/save.h`),
/// restricted to the two arms the start menu's SAVE action can reach
/// (`SaveDoSaveCallback`, `start_menu.c:1093-1101`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SaveMode {
    /// `SAVE_NORMAL` — the ordinary in-place save.
    Normal,
    /// `SAVE_OVERWRITE_DIFFERENT_FILE` — this session started a *new* game
    /// (`gDifferentSaveFile`), so nothing of the previous file's state may
    /// survive the write (upstream erases the Hall of Fame sectors; see
    /// [`crate::game_save::SaveSlot::store_overwriting_different_file`] for
    /// this port's counterpart).
    ///
    /// `prompted` is not upstream's — it carries which of
    /// `SaveConfirmInputCallback`'s two YES paths got here
    /// (`start_menu.c:1008-1023`). `true` means the player answered YES to
    /// `gText_DifferentSaveFile`'s WARNING; `false` means the shortcut for
    /// an `SAVE_STATUS_EMPTY`/`SAVE_STATUS_CORRUPT` cartridge, where
    /// upstream asks nothing at all because there is provably nothing to
    /// overwrite. On a filesystem that premise can go stale between boot
    /// and save, so the unprompted write is the one the medium re-checks —
    /// see [`crate::game_save::SaveSlot::store_unless_foreign_save`].
    OverwriteDifferentFile {
        /// Whether an overwrite prompt was answered before this write.
        prompted: bool,
    },
}

/// Everything [`SaveDialog`] needs from the session it is saving —
/// upstream's `gSaveFileStatus`/`gDifferentSaveFile` globals and
/// `TrySavingData`, as an interface the caller supplies rather than state
/// this module reaches for `(oop-boundaries)`.
pub(crate) trait SaveTarget {
    /// `gSaveFileStatus` (`src/save.c:89`) — the *boot* load's verdict,
    /// which `SaveConfirmInputCallback` branches on.
    fn boot_status(&self) -> SaveFileStatus;

    /// `gDifferentSaveFile` (`src/new_game.c:55`) — set by
    /// `NewGameInitData` (`:154`) and cleared once a
    /// `SAVE_OVERWRITE_DIFFERENT_FILE` write has been *attempted*, whatever
    /// its status (`start_menu.c:1093-1096`).
    fn different_save_file(&self) -> bool;

    /// The tokens `{PLAYER}` expands to in `gText_PlayerSavedGame` — the
    /// live save block's player name.
    fn player_name(&self) -> Vec<Token>;

    /// `TrySavingData(mode)` (`src/save.c:765-783`): perform the write and
    /// report whether it returned `SAVE_STATUS_OK`.
    ///
    /// Clearing `gDifferentSaveFile` is the implementor's job, exactly as
    /// it is upstream's `SaveDoSaveCallback` — and on upstream's terms:
    /// the clear sits in the `SAVE_OVERWRITE_DIFFERENT_FILE` branch
    /// immediately after the `TrySavingData` call, before `saveStatus` is
    /// examined (`start_menu.c:1093-1096`), so *every*
    /// [`SaveMode::OverwriteDifferentFile`] call clears it — a failed one
    /// included. An implementor that cleared only on success would re-show
    /// `gText_DifferentSaveFile`'s WARNING on the next SAVE where upstream
    /// shows `gText_AlreadySavedFile`.
    fn try_saving_data(&mut self, mode: SaveMode) -> bool;
}

/// `RunSaveCallback`'s return values (`include/start_menu.h`'s `SAVE_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SaveDialogOutcome {
    /// `SAVE_IN_PROGRESS` — the flow owns another frame.
    InProgress,
    /// `SAVE_CANCELED` — the player answered NO (or pressed B); the start
    /// menu comes back (`SaveCallback`, `start_menu.c:822-827`).
    Canceled,
    /// `SAVE_SUCCESS` — the write landed; the start menu closes.
    Success,
    /// `SAVE_ERROR` — the write failed; the start menu closes all the same
    /// (`SaveCallback`'s shared `SAVE_SUCCESS`/`SAVE_ERROR` arm, `:828-834`).
    Error,
}

/// One `sSaveDialogCallback` value (module docs' table).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    ShowConfirm,
    OpenConfirmMenu,
    ConfirmInput,
    ShowFileExists,
    /// `SaveConfirmOverwriteDefaultNoCallback` when `default_no`
    /// (`DisplayYesNoMenuWithDefault(1)`, `:1051`), otherwise
    /// `SaveConfirmOverwriteCallback` (`DisplayYesNoMenuDefaultYes`, `:1058`).
    OpenOverwriteMenu {
        default_no: bool,
    },
    OverwriteInput,
    ShowSaving,
    DoSave,
    AwaitSuccess,
    AwaitError,
}

/// `SaveStartTimer`'s reload value (`start_menu.c:942-945`): the frames the
/// result message stays up before the flow returns to the field.
const RESULT_TIMER_FRAMES: u8 = 60;

/// The SAVE flow's live state (module docs).
#[derive(Debug)]
pub(super) struct SaveDialog {
    step: Step,
    /// The field message box the current `ShowSaveMessage` opened, kept
    /// drawn after it finishes printing (upstream leaves the window up
    /// until `HideSaveMessageWindow`).
    message: Option<NpcDialog>,
    /// `RunTextPrintersAndIsPrinter0Active()` (`:887`): while true, no
    /// callback runs.
    printing: bool,
    /// The Yes/No window, while one is open (`DisplayYesNoMenu*`).
    yes_no: Option<YesNoMenu>,
    /// Whether an overwrite prompt was shown on the way here — see
    /// [`SaveMode::OverwriteDifferentFile`]'s `prompted`. Set by
    /// [`Step::ShowFileExists`], the only step that shows one.
    prompted: bool,
    /// `sSaveDialogTimer` (`:89`).
    timer: u8,
}

impl SaveDialog {
    /// `InitSave` (`start_menu.c:877-882`): arm the chain at
    /// `SaveConfirmSaveCallback`. `SaveMapView` — upstream's "remember the
    /// tiles under the player for the continue warp" step — has no
    /// counterpart here (this port has no saved map view).
    pub(super) fn new() -> Self {
        Self {
            step: Step::ShowConfirm,
            message: None,
            printing: false,
            yes_no: None,
            prompted: false,
            timer: 0,
        }
    }

    /// Advance the flow by one frame — `RunSaveCallback` plus whichever
    /// callback it dispatches to.
    pub(super) fn run(
        &mut self,
        buttons: ButtonState,
        chrome: &StartMenuChrome,
        target: &mut impl SaveTarget,
    ) -> SaveDialogOutcome {
        if self.printing {
            // `RunTextPrintersAndIsPrinter0Active()` (`:887`). A/B are the
            // printer's own speed-up/advance edges (`NpcDialog::tick`'s
            // docs); nothing else in this flow reads them on a frame the
            // box is still printing, so the press that pages a message can
            // never also answer the prompt behind it.
            let confirm =
                buttons.is_newly_pressed(Buttons::A) || buttons.is_newly_pressed(Buttons::B);
            if let Some(message) = &mut self.message {
                if message.tick(confirm) == DialogOutcome::Closed {
                    self.printing = false;
                }
            } else {
                self.printing = false;
            }
            return SaveDialogOutcome::InProgress;
        }

        match self.step {
            // `SaveConfirmSaveCallback` (`:978-994`). The Battle Pyramid
            // "rest" variant of this prompt is out of scope (module docs).
            Step::ShowConfirm => {
                self.show(SaveMessage::ConfirmSave, chrome, target);
                self.step = Step::OpenConfirmMenu;
                SaveDialogOutcome::InProgress
            }
            // `SaveYesNoCallback` (`:996-1001`): `DisplayYesNoMenuDefaultYes`.
            Step::OpenConfirmMenu => {
                self.yes_no = Some(YesNoMenu::new(false));
                self.step = Step::ConfirmInput;
                SaveDialogOutcome::InProgress
            }
            // `SaveConfirmInputCallback` (`:1003-1033`).
            Step::ConfirmInput => match self.read_yes_no(buttons) {
                Some(true) => {
                    self.step = next_step_after_confirm(target);
                    SaveDialogOutcome::InProgress
                }
                Some(false) => self.cancel(),
                None => SaveDialogOutcome::InProgress,
            },
            // `SaveFileExistsCallback` (`:1035-1047`): which of the two
            // overwrite prompts, and which Yes/No default, follow from
            // `gDifferentSaveFile` alone.
            Step::ShowFileExists => {
                self.prompted = true;
                let different = target.different_save_file();
                let message = if different {
                    SaveMessage::DifferentSaveFile
                } else {
                    SaveMessage::AlreadySavedFile
                };
                self.show(message, chrome, target);
                self.step = Step::OpenOverwriteMenu {
                    default_no: different,
                };
                SaveDialogOutcome::InProgress
            }
            Step::OpenOverwriteMenu { default_no } => {
                self.yes_no = Some(YesNoMenu::new(default_no));
                self.step = Step::OverwriteInput;
                SaveDialogOutcome::InProgress
            }
            // `SaveOverwriteInputCallback` (`:1063-1078`).
            Step::OverwriteInput => match self.read_yes_no(buttons) {
                Some(true) => {
                    self.step = Step::ShowSaving;
                    SaveDialogOutcome::InProgress
                }
                Some(false) => self.cancel(),
                None => SaveDialogOutcome::InProgress,
            },
            // `SaveSavingMessageCallback` (`:1080-1084`).
            Step::ShowSaving => {
                self.show(SaveMessage::Saving, chrome, target);
                self.step = Step::DoSave;
                SaveDialogOutcome::InProgress
            }
            Step::DoSave => self.do_save(chrome, target),
            Step::AwaitSuccess => self.await_success(buttons),
            Step::AwaitError => self.await_error(buttons),
        }
    }

    /// `SaveDoSaveCallback` (`start_menu.c:1086-1110`): the one place this
    /// flow touches the save medium. Which `TrySavingData` arm is
    /// upstream's own `gDifferentSaveFile` branch (`:1093-1101`); the
    /// `prompted` bit riding along is this port's
    /// ([`SaveMode::OverwriteDifferentFile`]).
    fn do_save(
        &mut self,
        chrome: &StartMenuChrome,
        target: &mut impl SaveTarget,
    ) -> SaveDialogOutcome {
        let mode = if target.different_save_file() {
            SaveMode::OverwriteDifferentFile {
                prompted: self.prompted,
            }
        } else {
            SaveMode::Normal
        };
        let saved = target.try_saving_data(mode);
        let message = if saved {
            SaveMessage::PlayerSavedGame
        } else {
            SaveMessage::SaveError
        };
        self.show(message, chrome, target);
        // `SaveStartTimer` (`:1108`).
        self.timer = RESULT_TIMER_FRAMES;
        self.step = if saved {
            Step::AwaitSuccess
        } else {
            Step::AwaitError
        };
        SaveDialogOutcome::InProgress
    }

    /// `SaveSuccessCallback` -> `SaveReturnSuccessCallback`
    /// (`:1112-1134`): `SaveSuccesTimer` (`:947-962`) counts down every
    /// frame *and* a held A ends the wait early. `IsSEPlaying()` is always
    /// false here (module docs).
    fn await_success(&mut self, buttons: ButtonState) -> SaveDialogOutcome {
        self.timer = self.timer.saturating_sub(1);
        if buttons.is_held(Buttons::A) || self.timer == 0 {
            self.hide();
            SaveDialogOutcome::Success
        } else {
            SaveDialogOutcome::InProgress
        }
    }

    /// `SaveErrorCallback` -> `SaveReturnErrorCallback` (`:1136-1159`).
    /// `SaveErrorTimer` (`:964-976`) is the *other* shape: it counts the
    /// timer all the way down first and only then accepts a held A, so a
    /// failed save cannot be dismissed before its message has been on
    /// screen for a full second.
    fn await_error(&mut self, buttons: ButtonState) -> SaveDialogOutcome {
        if self.timer != 0 {
            self.timer -= 1;
            SaveDialogOutcome::InProgress
        } else if buttons.is_held(Buttons::A) {
            self.hide();
            SaveDialogOutcome::Error
        } else {
            SaveDialogOutcome::InProgress
        }
    }

    /// The open message box and Yes/No window, for
    /// [`super::StartMenu::compose_over`].
    pub(super) const fn message(&self) -> Option<&NpcDialog> {
        self.message.as_ref()
    }

    /// See [`Self::message`].
    pub(super) const fn yes_no(&self) -> Option<&YesNoMenu> {
        self.yes_no.as_ref()
    }

    /// `ShowSaveMessage` (`start_menu.c:902-909`): open the field message
    /// box on `message`, with `{PLAYER}` expanded against the live save
    /// block (upstream's `StringExpandPlaceholders`, which
    /// `AddTextPrinterForMessage` performs for every field message).
    ///
    /// A chrome that cannot build a box at all is impossible here — the
    /// chrome was decoded when the start menu opened — so this cannot fail.
    fn show(&mut self, message: SaveMessage, chrome: &StartMenuChrome, target: &impl SaveTarget) {
        let resolver = PlayerNameResolver {
            name: target.player_name(),
        };
        let tokens = expand_placeholders(&message.tokens(), &resolver)
            // The only failure mode is a resolver cycle, and this resolver
            // expands one id to plain characters.
            .unwrap_or_else(|_| message.tokens());
        self.message = Some(chrome.message_box(tokens));
        self.printing = true;
    }

    /// `HideSaveInfoWindow` + `HideSaveMessageWindow` (`:932-940`): take
    /// the chrome back down on the way out.
    fn hide(&mut self) {
        self.message = None;
        self.yes_no = None;
        self.printing = false;
    }

    /// `Menu_ProcessInputNoWrapClearOnChoose` (`src/menu.c:1211-1217`) as
    /// the two callbacks that read a Yes/No answer use it: `Some(true)` for
    /// YES, `Some(false)` for NO *or* B (`MENU_B_PRESSED` shares NO's arm),
    /// `None` while nothing has been chosen. A chosen answer erases the
    /// window (`EraseYesNoWindow`).
    fn read_yes_no(&mut self, buttons: ButtonState) -> Option<bool> {
        let menu = self.yes_no.as_mut()?;
        let answer = menu.process_input(buttons)?;
        self.yes_no = None;
        Some(answer)
    }

    /// The `MENU_B_PRESSED`/NO arm both input callbacks share
    /// (`:1026-1030`, `:1072-1076`): drop the chrome and report
    /// `SAVE_CANCELED`.
    fn cancel(&mut self) -> SaveDialogOutcome {
        self.hide();
        SaveDialogOutcome::Canceled
    }
}

/// `SaveConfirmInputCallback`'s YES arm (`start_menu.c:1006-1023`): which
/// callback the `gSaveFileStatus`/`gDifferentSaveFile` pair selects.
///
/// A brand-new game over an empty or corrupt cartridge is the *only* case
/// that skips the overwrite prompt — there is nothing there to overwrite,
/// and the session is the one that would be asked about it. Everything else
/// (including a continued session saving over its own file, which is what
/// makes Emerald ask twice) goes through `SaveFileExistsCallback`.
fn next_step_after_confirm(target: &impl SaveTarget) -> Step {
    let nothing_saved = matches!(
        target.boot_status(),
        SaveFileStatus::Empty | SaveFileStatus::Corrupt
    );
    if nothing_saved && target.different_save_file() {
        Step::ShowSaving
    } else {
        Step::ShowFileExists
    }
}

/// `GetExpandedPlaceholder`'s `PLACEHOLDER_ID_PLAYER` entry, over the live
/// save block's name rather than `gSaveBlock2Ptr`.
struct PlayerNameResolver {
    name: Vec<Token>,
}

impl PlaceholderResolver for PlayerNameResolver {
    fn resolve(&self, id: u8) -> Option<Vec<Token>> {
        (id == PLACEHOLDER_PLAYER).then(|| self.name.clone())
    }
}
