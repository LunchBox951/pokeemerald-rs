//! The party menu's TAKE MAIL flow (`CursorCb_TakeMail`,
//! `pokeemerald/src/party_menu.c:3399-3478`), which the start menu's TAKE entry
//! reaches for a lead holding mail (`SetPartyMonFieldSelectionActions` offers
//! `MENU_MAIL` instead of `MENU_ITEM` for a mail holder, `:2631-2634`).
//!
//! 1. `gText_SendMailToPC`, then YES/NO (`Task_HandleSendMailToPCYesNoInput`).
//!    YES sends the mail to the PC mailbox: `gText_MailSentToPC`, or
//!    `gText_PCMailboxFull` when no slot is free.
//! 2. NO or B asks `gText_MailMessageWillBeLost`, then YES/NO
//!    (`Task_HandleLoseMailMessageYesNoInput`). YES returns the mail item to the
//!    bag with its message erased: `gText_MailTakenFromPkmn`, or the bag-full
//!    refusal. NO or B returns to the party menu with nothing changed.
//!
//! The YES/NO window is the one the SAVE flow uses ([`YesNoMenu`]), and the
//! prompts dispatch on the tick their message finishes printing, as
//! [`super::save_dialog`] does. Every result message waits for A or B
//! (`{PAUSE_UNTIL_PRESS}`). The port has no party menu to return to, so the
//! flow ends with the start menu closing, whichever way it ended.

use engine::text::render::TextSpeed;
use platform::ButtonState;

use crate::overworld::dialog::confirm_printer_input;
use crate::overworld::{DialogOutcome, NpcDialog};

use super::text::MailMessage;
use super::{StartMenuChrome, YesNoMenu};

/// What [`TakeMailTarget::send_mail_to_pc`] did
/// (`TakeMailFromMonAndSave`, `pokeemerald/src/mail_data.c:159-182`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SendMailToPc {
    /// The mail moved to a mailbox slot.
    Sent,
    /// Every mailbox slot is taken; nothing changed.
    MailboxFull,
    /// There is no mail to send (no lead, or a record that does not hold the
    /// mail its `mail` byte names); nothing changed.
    NothingToSend,
}

/// What [`TakeMailTarget::return_mail_to_bag`] did
/// (`Task_HandleLoseMailMessageYesNoInput`, `party_menu.c:3458-3470`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReturnMailToBag {
    /// The bag gained the mail item and the mon lost the mail.
    Taken,
    /// `AddBagItem` refused the item; nothing changed.
    BagFull,
    /// There is no mail to return; nothing changed.
    NothingToReturn,
}

/// The lead, bag and mail array a [`TakeMailDialog`] acts on.
pub(crate) trait TakeMailTarget {
    /// The text speed for the flow's messages.
    fn mail_text_speed(&self) -> TextSpeed;

    /// `TakeMailFromMonAndSave` on the lead.
    fn send_mail_to_pc(&mut self) -> SendMailToPc;

    /// `AddBagItem` then `TakeMailFromMon` on the lead.
    fn return_mail_to_bag(&mut self) -> ReturnMailToBag;
}

/// The dialog's effect on its owning start menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TakeMailOutcome {
    /// Keeps the dialog open for another frame.
    InProgress,
    /// The flow is over; the menu closes.
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    ShowSendPrompt,
    OpenSendChoice,
    AwaitSendChoice,
    ShowLosePrompt,
    OpenLoseChoice,
    AwaitLoseChoice,
    /// A result message is printing; the flow ends when it closes.
    Finish,
}

/// The state of an open TAKE MAIL flow.
#[derive(Debug)]
pub(super) struct TakeMailDialog {
    state: State,
    message: Option<NpcDialog>,
    message_is_printing: bool,
    yes_no: Option<YesNoMenu>,
    /// Every message opened so far, for tests (the box does not expose its text).
    #[cfg(test)]
    shown: Vec<MailMessage>,
}

impl TakeMailDialog {
    /// Creates the flow at its first tick, `CursorCb_TakeMail`.
    pub(super) const fn new() -> Self {
        Self {
            state: State::ShowSendPrompt,
            message: None,
            message_is_printing: false,
            yes_no: None,
            #[cfg(test)]
            shown: Vec::new(),
        }
    }

    /// Advances the flow by one frame. Like the save dialog, a state queued
    /// behind a message runs on the tick that message finishes printing.
    pub(super) fn run(
        &mut self,
        buttons: ButtonState,
        chrome: &StartMenuChrome,
        target: &mut impl TakeMailTarget,
    ) -> TakeMailOutcome {
        if self.message_is_printing {
            self.advance_message(buttons);
            if self.message_is_printing {
                return TakeMailOutcome::InProgress;
            }
        }
        match self.state {
            State::ShowSendPrompt => {
                self.show_message(MailMessage::SendMailToPc, chrome, target);
                self.state = State::OpenSendChoice;
            }
            State::OpenSendChoice => {
                self.yes_no = Some(YesNoMenu::new(false));
                self.state = State::AwaitSendChoice;
            }
            State::AwaitSendChoice => match self.take_answer(buttons) {
                Some(true) => {
                    let result = match target.send_mail_to_pc() {
                        SendMailToPc::Sent => MailMessage::SentToPc,
                        SendMailToPc::MailboxFull => MailMessage::MailboxFull,
                        SendMailToPc::NothingToSend => return self.finish_without_message(),
                    };
                    self.show_result(result, chrome, target);
                }
                Some(false) => self.state = State::ShowLosePrompt,
                None => {}
            },
            State::ShowLosePrompt => {
                self.show_message(MailMessage::MessageWillBeLost, chrome, target);
                self.state = State::OpenLoseChoice;
            }
            State::OpenLoseChoice => {
                self.yes_no = Some(YesNoMenu::new(false));
                self.state = State::AwaitLoseChoice;
            }
            State::AwaitLoseChoice => match self.take_answer(buttons) {
                Some(true) => {
                    let result = match target.return_mail_to_bag() {
                        ReturnMailToBag::Taken => MailMessage::TakenFromPokemon,
                        ReturnMailToBag::BagFull => MailMessage::BagFull,
                        ReturnMailToBag::NothingToReturn => return self.finish_without_message(),
                    };
                    self.show_result(result, chrome, target);
                }
                Some(false) => return self.finish_without_message(),
                None => {}
            },
            State::Finish => {
                self.close_windows();
                return TakeMailOutcome::Done;
            }
        }
        TakeMailOutcome::InProgress
    }

    /// The messages opened so far, in order.
    #[cfg(test)]
    pub(super) fn shown(&self) -> &[MailMessage] {
        &self.shown
    }

    /// The open message box, for composition.
    pub(super) const fn message(&self) -> Option<&NpcDialog> {
        self.message.as_ref()
    }

    /// The open YES/NO window, for composition.
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
        message: MailMessage,
        chrome: &StartMenuChrome,
        target: &impl TakeMailTarget,
    ) {
        #[cfg(test)]
        self.shown.push(message);
        self.message = Some(chrome.message_box(message.tokens(), target.mail_text_speed()));
        self.message_is_printing = true;
    }

    fn show_result(
        &mut self,
        message: MailMessage,
        chrome: &StartMenuChrome,
        target: &impl TakeMailTarget,
    ) {
        self.show_message(message, chrome, target);
        self.state = State::Finish;
    }

    /// Ends the flow with no further text: a cancel, or a transfer that had
    /// nothing to act on (the port has no upstream text for a malformed record).
    fn finish_without_message(&mut self) -> TakeMailOutcome {
        self.close_windows();
        TakeMailOutcome::Done
    }

    fn take_answer(&mut self, buttons: ButtonState) -> Option<bool> {
        let answer = self.yes_no.as_mut()?.process_input(buttons)?;
        self.yes_no = None;
        Some(answer)
    }

    fn close_windows(&mut self) {
        self.message = None;
        self.yes_no = None;
        self.message_is_printing = false;
    }
}
