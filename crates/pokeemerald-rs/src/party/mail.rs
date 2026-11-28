//! The mail transfers behind the party menu's TAKE MAIL entry
//! (`CursorCb_TakeMail`, `pokeemerald/src/party_menu.c:3399-3478`):
//! `TakeMailFromMonAndSave` (send the held mail to the PC mailbox) and
//! `TakeMailFromMon` (return the mail item to the bag, its message erased),
//! `pokeemerald/src/mail_data.c:137-182`.
//!
//! A Pokemon's mail lives in two places: its held item and its `mail` byte, an
//! index into `SaveBlock1::mail`'s party slots. Both transfers work on the
//! lead's battler and record byte plus the mail array, all passed in so the
//! caller can run them on copies and commit afterwards.
//!
//! Upstream indexes `mail[mon.mail]` without checking it. A loaded save can
//! hold a mail item whose byte is `MAIL_NONE`, out of range, or names a slot
//! holding other mail; the port refuses those ([`TakeMailError::InvalidSource`])
//! rather than read out of bounds, touching nothing.

use assets::items::ItemId;
use battle::BattlePokemon;
use engine::save::{item_is_mail, Mail, MAIL_COUNT, MAIL_NONE, MAIL_PARTY_SLOTS};

/// Why a mail transfer changed nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TakeMailError {
    /// No PC mailbox slot (`MAIL_PARTY_SLOTS..MAIL_COUNT`) is free
    /// (`TakeMailFromMonAndSave`'s `MAIL_NONE` return, `mail_data.c:181`).
    MailboxFull,
    /// The Pokemon does not hold mail that its `mail` byte points at: no mail
    /// item, a byte outside the party slots, or a slot whose item differs.
    InvalidSource,
}

/// `MonHasMail` (`pokeemerald/src/mail_data.c:36-42`): a mail item held with a
/// `mail` byte other than `MAIL_NONE`. Says nothing about the slot behind it.
pub(crate) fn mon_has_mail(held: ItemId, mon_mail: u8) -> bool {
    item_is_mail(held) && mon_mail != MAIL_NONE
}

/// The party slot `mon_mail` names, if it holds `held`.
fn source_slot(
    held: ItemId,
    mon_mail: u8,
    mail: &[Mail; MAIL_COUNT],
) -> Result<usize, TakeMailError> {
    let slot = usize::from(mon_mail);
    if item_is_mail(held) && slot < MAIL_PARTY_SLOTS && mail[slot].item_id == held {
        Ok(slot)
    } else {
        Err(TakeMailError::InvalidSource)
    }
}

/// Whether [`take_mail_from_mon_and_save`] and [`take_mail_from_mon`] can act
/// on `mon`: its held mail is the mail its `mon_mail` byte names.
pub(crate) fn holds_valid_mail(
    mon: &BattlePokemon,
    mon_mail: u8,
    mail: &[Mail; MAIL_COUNT],
) -> bool {
    source_slot(mon.held_item(), mon_mail, mail).is_ok()
}

/// `TakeMailFromMonAndSave` (`pokeemerald/src/mail_data.c:159-182`): copies the
/// mon's whole mail entry into the first free PC mailbox slot, empties the
/// source slot (its `itemId` only, as upstream), and clears the mon's `mail`
/// byte and held item. Returns the mailbox slot.
///
/// # Errors
///
/// [`TakeMailError::MailboxFull`] when every mailbox slot holds mail, and
/// [`TakeMailError::InvalidSource`] for a mon that does not hold the mail its
/// byte names; either leaves all three arguments unchanged.
pub(crate) fn take_mail_from_mon_and_save(
    mon: &mut BattlePokemon,
    mon_mail: &mut u8,
    mail: &mut [Mail; MAIL_COUNT],
) -> Result<u8, TakeMailError> {
    let source = source_slot(mon.held_item(), *mon_mail, mail)?;
    let destination = (MAIL_PARTY_SLOTS..MAIL_COUNT)
        .find(|&slot| mail[slot].is_empty())
        .ok_or(TakeMailError::MailboxFull)?;
    mail[destination] = mail[source];
    mail[source].item_id = ItemId::NONE;
    *mon_mail = MAIL_NONE;
    *mon = mon.clone().with_held_item(ItemId::NONE);
    Ok(u8::try_from(destination).unwrap_or(MAIL_NONE))
}

/// `TakeMailFromMon` (`pokeemerald/src/mail_data.c:137-151`): empties the
/// source slot's `itemId`, clears the mon's `mail` byte and held item. The
/// message is not cleared; the empty `itemId` is what makes the slot free.
/// Returns whether anything was taken; a mon that fails [`mon_has_mail`] is a
/// no-op, as upstream.
///
/// The caller returns the item to the bag first
/// (`Task_HandleLoseMailMessageYesNoInput`, `party_menu.c:3458-3470`), so it
/// must check [`holds_valid_mail`] before adding it.
///
/// # Errors
///
/// [`TakeMailError::InvalidSource`] for a mon that passes [`mon_has_mail`] but
/// does not hold the mail its byte names; nothing changes.
pub(crate) fn take_mail_from_mon(
    mon: &mut BattlePokemon,
    mon_mail: &mut u8,
    mail: &mut [Mail; MAIL_COUNT],
) -> Result<bool, TakeMailError> {
    if !mon_has_mail(mon.held_item(), *mon_mail) {
        return Ok(false);
    }
    let source = source_slot(mon.held_item(), *mon_mail, mail)?;
    mail[source].item_id = ItemId::NONE;
    *mon_mail = MAIL_NONE;
    *mon = mon.clone().with_held_item(ItemId::NONE);
    Ok(true)
}
