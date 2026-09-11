//! Save confirmation, overwrite, and completion flow.
//!
//! Message printing owns each frame until it finishes. This keeps the A press
//! that advances one message from answering the next prompt in the same frame
//! (`pokeemerald/src/start_menu.c:884-894`).

use engine::text::format::{expand_placeholders, PlaceholderResolver};
use engine::text::render::TextSpeed;
use engine::text::{Token, PLACEHOLDER_PLAYER};
use platform::{ButtonState, Buttons};

use crate::game_save::SaveFileStatus;
use crate::overworld::dialog::confirm_printer_input;
use crate::overworld::{DialogOutcome, NpcDialog};

use super::text::SaveMessage;
use super::{StartMenuChrome, YesNoMenu};

/// Selects how the save medium handles an existing file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SaveMode {
    /// Uses the ordinary store path.
    Normal,
    /// Writes a new-game session over another save.
    ///
    /// Upstream adds only a Hall of Fame erase over `SAVE_NORMAL`
    /// (`src/save.c:751-760`), which this port does not model, so both modes
    /// reach the same write. Which bytes a session writes is
    /// [`crate::game_save::SaveLineage`]'s property, never this mode's.
    ///
    /// `prompted` is false only when the boot status was empty or corrupt. The
    /// store must then reject a foreign save that appeared after boot.
    OverwriteDifferentFile {
        /// Whether the player confirmed an overwrite prompt.
        prompted: bool,
    },
}

/// Supplies session state and persistence to a [`SaveDialog`].
pub(crate) trait SaveTarget {
    /// Returns the save-file status captured at boot.
    fn boot_status(&self) -> SaveFileStatus;

    /// Returns whether a new-game session has not yet attempted an overwrite.
    fn different_save_file(&self) -> bool;

    /// Returns the player name inserted into the completion message.
    fn player_name(&self) -> Vec<Token>;

    /// Returns the text speed for save-flow messages.
    fn player_text_speed(&self) -> TextSpeed;

    /// Attempts a store and reports whether it succeeded.
    ///
    /// An overwrite attempt must clear [`Self::different_save_file`] even when
    /// the store fails (`pokeemerald/src/start_menu.c:1093-1096`).
    fn try_saving_data(&mut self, mode: SaveMode) -> bool;
}

/// The save dialog's effect on its owning start menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SaveDialogOutcome {
    /// Keeps the save dialog open for another frame.
    InProgress,
    /// Returns to the start-menu item list without saving.
    Canceled,
    /// Closes the start menu after a successful store.
    Success,
    /// Closes the start menu after a failed store.
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SaveDialogState {
    ShowInitialPrompt,
    OpenInitialChoice,
    AwaitInitialChoice,
    ShowOverwritePrompt,
    OpenOverwriteChoice(OverwritePrompt),
    AwaitOverwriteChoice,
    ShowSavingMessage,
    Store,
    AwaitSuccessDismissal,
    AwaitErrorDismissal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OverwritePrompt {
    ExistingSave,
    DifferentSaveFileWarning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum YesNoChoice {
    Yes,
    No,
}

impl OverwritePrompt {
    fn for_target(target: &impl SaveTarget) -> Self {
        if target.different_save_file() {
            Self::DifferentSaveFileWarning
        } else {
            Self::ExistingSave
        }
    }

    const fn message(self) -> SaveMessage {
        match self {
            Self::ExistingSave => SaveMessage::AlreadySavedFile,
            Self::DifferentSaveFileWarning => SaveMessage::DifferentSaveFile,
        }
    }

    const fn default_choice(self) -> YesNoChoice {
        match self {
            Self::ExistingSave => YesNoChoice::Yes,
            Self::DifferentSaveFileWarning => YesNoChoice::No,
        }
    }
}

const RESULT_MESSAGE_FRAMES: u8 = 60;

/// The state of an open save dialog.
#[derive(Debug)]
pub(super) struct SaveDialog {
    state: SaveDialogState,
    message: Option<NpcDialog>,
    message_is_printing: bool,
    yes_no: Option<YesNoMenu>,
    overwrite_was_confirmed: bool,
    result_message_frames_remaining: u8,
}

impl SaveDialog {
    /// Creates a save dialog at its initial confirmation prompt.
    pub(super) fn new() -> Self {
        Self {
            state: SaveDialogState::ShowInitialPrompt,
            message: None,
            message_is_printing: false,
            yes_no: None,
            overwrite_was_confirmed: false,
            result_message_frames_remaining: 0,
        }
    }

    /// Advances the save dialog by one frame.
    pub(super) fn run(
        &mut self,
        buttons: ButtonState,
        chrome: &StartMenuChrome,
        target: &mut impl SaveTarget,
    ) -> SaveDialogOutcome {
        if self.message_is_printing {
            self.advance_message(buttons);
            return SaveDialogOutcome::InProgress;
        }

        match self.state {
            SaveDialogState::ShowInitialPrompt => {
                self.show_message(SaveMessage::ConfirmSave, chrome, target);
                self.state = SaveDialogState::OpenInitialChoice;
                SaveDialogOutcome::InProgress
            }
            SaveDialogState::OpenInitialChoice => {
                self.open_yes_no(YesNoChoice::Yes);
                self.state = SaveDialogState::AwaitInitialChoice;
                SaveDialogOutcome::InProgress
            }
            SaveDialogState::AwaitInitialChoice => match self.take_yes_no_answer(buttons) {
                Some(YesNoChoice::Yes) => {
                    self.state = next_state_after_initial_confirmation(target);
                    SaveDialogOutcome::InProgress
                }
                Some(YesNoChoice::No) => self.cancel(),
                None => SaveDialogOutcome::InProgress,
            },
            SaveDialogState::ShowOverwritePrompt => {
                let prompt = OverwritePrompt::for_target(target);
                self.show_message(prompt.message(), chrome, target);
                self.state = SaveDialogState::OpenOverwriteChoice(prompt);
                SaveDialogOutcome::InProgress
            }
            SaveDialogState::OpenOverwriteChoice(prompt) => {
                self.open_yes_no(prompt.default_choice());
                self.state = SaveDialogState::AwaitOverwriteChoice;
                SaveDialogOutcome::InProgress
            }
            SaveDialogState::AwaitOverwriteChoice => match self.take_yes_no_answer(buttons) {
                Some(YesNoChoice::Yes) => {
                    self.overwrite_was_confirmed = true;
                    self.state = SaveDialogState::ShowSavingMessage;
                    SaveDialogOutcome::InProgress
                }
                Some(YesNoChoice::No) => self.cancel(),
                None => SaveDialogOutcome::InProgress,
            },
            SaveDialogState::ShowSavingMessage => {
                self.show_message(SaveMessage::Saving, chrome, target);
                self.state = SaveDialogState::Store;
                SaveDialogOutcome::InProgress
            }
            SaveDialogState::Store => self.store(chrome, target),
            SaveDialogState::AwaitSuccessDismissal => self.await_success_dismissal(buttons),
            SaveDialogState::AwaitErrorDismissal => self.await_error_dismissal(buttons),
        }
    }

    /// Returns the open message box for composition.
    pub(super) const fn message(&self) -> Option<&NpcDialog> {
        self.message.as_ref()
    }

    /// Returns the open Yes/No menu for composition.
    pub(super) const fn yes_no(&self) -> Option<&YesNoMenu> {
        self.yes_no.as_ref()
    }

    fn advance_message(&mut self, buttons: ButtonState) {
        let finished = match &mut self.message {
            Some(message) => message.tick(confirm_printer_input(buttons)) == DialogOutcome::Closed,
            None => true,
        };
        if finished {
            self.message_is_printing = false;
        }
    }

    fn show_message(
        &mut self,
        message: SaveMessage,
        chrome: &StartMenuChrome,
        target: &impl SaveTarget,
    ) {
        let resolver = PlayerNameResolver {
            name: target.player_name(),
        };
        let tokens =
            expand_placeholders(&message.tokens(), &resolver).unwrap_or_else(|_| message.tokens());
        self.message = Some(chrome.message_box(tokens, target.player_text_speed()));
        self.message_is_printing = true;
    }

    fn store(
        &mut self,
        chrome: &StartMenuChrome,
        target: &mut impl SaveTarget,
    ) -> SaveDialogOutcome {
        let mode = if target.different_save_file() {
            SaveMode::OverwriteDifferentFile {
                prompted: self.overwrite_was_confirmed,
            }
        } else {
            SaveMode::Normal
        };
        let saved = target.try_saving_data(mode);
        let result_message = if saved {
            SaveMessage::PlayerSavedGame
        } else {
            SaveMessage::SaveError
        };
        self.show_message(result_message, chrome, target);
        self.result_message_frames_remaining = RESULT_MESSAGE_FRAMES;
        self.state = if saved {
            SaveDialogState::AwaitSuccessDismissal
        } else {
            SaveDialogState::AwaitErrorDismissal
        };
        SaveDialogOutcome::InProgress
    }

    // Success accepts held A during its countdown (`start_menu.c:947-962`).
    fn await_success_dismissal(&mut self, buttons: ButtonState) -> SaveDialogOutcome {
        self.result_message_frames_remaining =
            self.result_message_frames_remaining.saturating_sub(1);
        if buttons.is_held(Buttons::A) || self.result_message_frames_remaining == 0 {
            self.close_windows();
            SaveDialogOutcome::Success
        } else {
            SaveDialogOutcome::InProgress
        }
    }

    // Failure ignores held A until its full countdown ends (`start_menu.c:964-976`).
    fn await_error_dismissal(&mut self, buttons: ButtonState) -> SaveDialogOutcome {
        if self.result_message_frames_remaining != 0 {
            self.result_message_frames_remaining -= 1;
            SaveDialogOutcome::InProgress
        } else if buttons.is_held(Buttons::A) {
            self.close_windows();
            SaveDialogOutcome::Error
        } else {
            SaveDialogOutcome::InProgress
        }
    }

    fn open_yes_no(&mut self, default: YesNoChoice) {
        self.yes_no = Some(YesNoMenu::new(default == YesNoChoice::No));
    }

    fn take_yes_no_answer(&mut self, buttons: ButtonState) -> Option<YesNoChoice> {
        let answer = self.yes_no.as_mut()?.process_input(buttons)?;
        self.yes_no = None;
        Some(if answer {
            YesNoChoice::Yes
        } else {
            YesNoChoice::No
        })
    }

    fn cancel(&mut self) -> SaveDialogOutcome {
        self.close_windows();
        SaveDialogOutcome::Canceled
    }

    fn close_windows(&mut self) {
        self.message = None;
        self.yes_no = None;
        self.message_is_printing = false;
    }
}

fn next_state_after_initial_confirmation(target: &impl SaveTarget) -> SaveDialogState {
    let boot_had_no_usable_save = matches!(
        target.boot_status(),
        SaveFileStatus::Empty | SaveFileStatus::Corrupt
    );
    if boot_had_no_usable_save && target.different_save_file() {
        SaveDialogState::ShowSavingMessage
    } else {
        SaveDialogState::ShowOverwritePrompt
    }
}

struct PlayerNameResolver {
    name: Vec<Token>,
}

impl PlaceholderResolver for PlayerNameResolver {
    fn resolve(&self, id: u8) -> Option<Vec<Token>> {
        (id == PLACEHOLDER_PLAYER).then(|| self.name.clone())
    }
}
