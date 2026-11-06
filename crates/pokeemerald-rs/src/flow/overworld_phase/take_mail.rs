//! The lead's side of the start menu's TAKE MAIL flow
//! ([`crate::start_menu`]'s `TakeMailDialog`, upstream's `CursorCb_TakeMail`,
//! `pokeemerald/src/party_menu.c:3399-3478`): the two transfers a YES answer
//! runs, on the lead, the bag and `SaveBlock1::mail`.
//!
//! Each transfer runs on copies and commits only when it succeeded, so a
//! refusal (full mailbox, full bag, a record that does not hold the mail it
//! names) leaves the lead, the bag, the mail array and the save as they were.
//! The dialog opens its result message in the same call, from assets it
//! already holds, so no step can leave a transfer the player was not told of.

use crate::party;
use crate::start_menu::{ReturnMailToBag, SendMailToPc, TakeMailTarget};

use super::OverworldPhase;

impl OverworldPhase {
    /// Whether the TAKE entry belongs to the mail flow: the loaded lead holds
    /// one of the twelve mail items. Upstream decides this from the held item
    /// alone (`ItemIsMail`, `SetPartyMonFieldSelectionActions`,
    /// `party_menu.c:2631-2634`), so a mail holder whose record turns out
    /// inconsistent still reaches the mail prompt.
    pub(super) fn lead_holds_mail(&self) -> bool {
        self.lead
            .as_ref()
            .filter(|lead| !lead.is_lent())
            .is_some_and(|lead| engine::save::item_is_mail(lead.battler().held_item()))
    }

    /// `TakeMailFromMonAndSave` on the lead.
    fn send_lead_mail_to_pc(&mut self) -> SendMailToPc {
        let Some(lead) = self.lead.as_ref().filter(|lead| !lead.is_lent()) else {
            return SendMailToPc::NothingToSend;
        };
        let mut battler = lead.battler().clone();
        let mut mon_mail = lead.record().mail;
        let mut mail = self.save1.mail;
        match party::take_mail_from_mon_and_save(&mut battler, &mut mon_mail, &mut mail) {
            Ok(_) => {
                self.commit_mail_take(battler, mon_mail, mail, None);
                SendMailToPc::Sent
            }
            Err(party::TakeMailError::MailboxFull) => SendMailToPc::MailboxFull,
            Err(party::TakeMailError::InvalidSource) => {
                eprintln!("take mail: the lead does not hold the mail its record names");
                SendMailToPc::NothingToSend
            }
        }
    }

    /// `AddBagItem` then `TakeMailFromMon` on the lead
    /// (`Task_HandleLoseMailMessageYesNoInput`, `party_menu.c:3458-3470`).
    /// The record is checked first so a record `TakeMailFromMon` would refuse
    /// never gains the item in the bag.
    fn return_lead_mail_to_bag(&mut self) -> ReturnMailToBag {
        let Some(lead) = self.lead.as_ref().filter(|lead| !lead.is_lent()) else {
            return ReturnMailToBag::NothingToReturn;
        };
        let mut battler = lead.battler().clone();
        let mut mon_mail = lead.record().mail;
        let mut mail = self.save1.mail;
        if !party::holds_valid_mail(&battler, mon_mail, &mail) {
            eprintln!("take mail: the lead does not hold the mail its record names");
            return ReturnMailToBag::NothingToReturn;
        }
        let item = battler.held_item();
        let mut bag = self.save1.bag.clone();
        if !bag.add_item(item, 1) {
            return ReturnMailToBag::BagFull;
        }
        match party::take_mail_from_mon(&mut battler, &mut mon_mail, &mut mail) {
            Ok(true) => {
                self.commit_mail_take(battler, mon_mail, mail, Some(bag));
                ReturnMailToBag::Taken
            }
            Ok(false) | Err(_) => ReturnMailToBag::NothingToReturn,
        }
    }

    /// Installs a finished transfer: the lead (battler and `mail` byte), the
    /// mail array and, for a return to the bag, the bag, then files the lead
    /// into its saved slot.
    fn commit_mail_take(
        &mut self,
        battler: battle::BattlePokemon,
        mon_mail: u8,
        mail: [engine::save::Mail; engine::save::MAIL_COUNT],
        bag: Option<engine::save::Bag>,
    ) {
        let Some(lead) = self.lead.as_mut().filter(|lead| !lead.is_lent()) else {
            return;
        };
        lead.install_mail_take(battler, mon_mail);
        self.save1.mail = mail;
        if let Some(bag) = bag {
            self.save1.bag = bag;
        }
        self.flush_lead_to_save();
    }
}

impl TakeMailTarget for super::start_menu::PhaseSaveTarget<'_> {
    fn mail_text_speed(&self) -> engine::text::render::TextSpeed {
        self.phase.borrow_mut().field_dialog_text_speed()
    }

    fn send_mail_to_pc(&mut self) -> SendMailToPc {
        self.phase.get_mut().send_lead_mail_to_pc()
    }

    fn return_mail_to_bag(&mut self) -> ReturnMailToBag {
        self.phase.get_mut().return_lead_mail_to_bag()
    }
}
