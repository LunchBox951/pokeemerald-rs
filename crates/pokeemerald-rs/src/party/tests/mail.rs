//! `TakeMailFromMonAndSave` / `TakeMailFromMon` (`pokeemerald/src/mail_data.c:137-182`):
//! the transfers behind `CursorCb_TakeMail` (`party_menu.c:3399-3478`).

use super::super::{
    from_save_pokemon, holds_valid_mail, take_mail_from_mon, take_mail_from_mon_and_save,
    TakeMailError,
};
use super::common::saved_treecko_holding;
use assets::items::ItemId;
use battle::{BattlePokemon, Dex};
use engine::save::{Mail, MAIL_COUNT, MAIL_NONE, MAIL_PARTY_SLOTS};

fn mail(item: ItemId, seed: u8) -> Mail {
    Mail {
        words: [u16::from(seed); 9],
        player_name: [seed; 8],
        trainer_id: [seed; 4],
        species: u16::from(seed),
        item_id: item,
        padding: [seed, seed],
    }
}

fn lead_holding(item: ItemId) -> BattlePokemon {
    from_save_pokemon(&Dex::new(), &saved_treecko_holding(item)).unwrap()
}

fn array_with(slot: usize, item: ItemId) -> [Mail; MAIL_COUNT] {
    let mut all = [Mail::default(); MAIL_COUNT];
    all[slot] = mail(item, 7);
    all
}

#[test]
fn sending_to_the_pc_copies_the_whole_entry_to_the_first_free_mailbox_slot() {
    let mut lead = lead_holding(ItemId::BEAD_MAIL);
    let mut byte = 2;
    let mut all = array_with(2, ItemId::BEAD_MAIL);
    let original = all[2];
    all[MAIL_PARTY_SLOTS] = mail(ItemId::WOOD_MAIL, 1);

    let sent = take_mail_from_mon_and_save(&mut lead, &mut byte, &mut all);

    assert_eq!(sent, Ok(7), "slot 6 is taken, so slot 7");
    assert_eq!(all[7], original);
    assert_eq!(all[2].item_id, ItemId::NONE);
    assert_eq!(all[2].words, original.words, "only itemId is cleared");
    assert_eq!(lead.held_item(), ItemId::NONE);
    assert_eq!(byte, MAIL_NONE);
}

#[test]
fn a_full_mailbox_changes_nothing() {
    let mut lead = lead_holding(ItemId::BEAD_MAIL);
    let mut byte = 0;
    let mut all = array_with(0, ItemId::BEAD_MAIL);
    for slot in MAIL_PARTY_SLOTS..MAIL_COUNT {
        all[slot] = mail(ItemId::DREAM_MAIL, 3);
    }
    let before = all;

    assert_eq!(
        take_mail_from_mon_and_save(&mut lead, &mut byte, &mut all),
        Err(TakeMailError::MailboxFull)
    );
    assert_eq!(all, before);
    assert_eq!((lead.held_item(), byte), (ItemId::BEAD_MAIL, 0));
}

#[test]
fn taking_mail_from_the_mon_frees_its_slot_and_leaves_the_message() {
    let mut lead = lead_holding(ItemId::TROPIC_MAIL);
    let mut byte = 4;
    let mut all = array_with(4, ItemId::TROPIC_MAIL);
    let original = all[4];

    assert_eq!(take_mail_from_mon(&mut lead, &mut byte, &mut all), Ok(true));

    assert_eq!(all[4].item_id, ItemId::NONE);
    assert_eq!(all[4].words, original.words);
    assert_eq!(lead.held_item(), ItemId::NONE);
    assert_eq!(byte, MAIL_NONE);
}

#[test]
fn a_mon_without_mail_is_a_no_op_for_take_mail_from_mon() {
    let mut all = array_with(0, ItemId::TROPIC_MAIL);
    let before = all;

    let mut plain = lead_holding(ItemId::ORAN_BERRY);
    let mut byte = MAIL_NONE;
    assert_eq!(
        take_mail_from_mon(&mut plain, &mut byte, &mut all),
        Ok(false)
    );
    assert_eq!(plain.held_item(), ItemId::ORAN_BERRY);

    // `MonHasMail` is false for a mail item with `MAIL_NONE`.
    let mut orphan = lead_holding(ItemId::TROPIC_MAIL);
    assert_eq!(
        take_mail_from_mon(&mut orphan, &mut byte, &mut all),
        Ok(false)
    );
    assert_eq!(orphan.held_item(), ItemId::TROPIC_MAIL);
    assert_eq!(all, before);
}

#[test]
fn a_record_that_names_no_matching_mail_is_refused_untouched() {
    // (held item, the mon's mail byte, the slot that holds TROPIC_MAIL)
    for (item, byte, slot) in [
        (ItemId::TROPIC_MAIL, MAIL_NONE, 0),
        (ItemId::TROPIC_MAIL, 6, 6),
        (ItemId::TROPIC_MAIL, 0xFE, 0),
        (ItemId::ORAN_BERRY, 0, 0),
        (ItemId::MASTER_BALL, 0, 0),
    ] {
        let mut lead = lead_holding(item);
        let mut mon_byte = byte;
        let mut all = array_with(slot, ItemId::TROPIC_MAIL);
        let before = all;
        assert!(!holds_valid_mail(&lead, mon_byte, &all));
        assert_eq!(
            take_mail_from_mon_and_save(&mut lead, &mut mon_byte, &mut all),
            Err(TakeMailError::InvalidSource),
            "{item:?} byte {byte:#x}"
        );
        assert_eq!(all, before);
        assert_eq!((lead.held_item(), mon_byte), (item, byte));
    }
}

#[test]
fn a_matching_record_is_valid() {
    let lead = lead_holding(ItemId::TROPIC_MAIL);
    assert!(holds_valid_mail(
        &lead,
        0,
        &array_with(0, ItemId::TROPIC_MAIL)
    ));
}

#[test]
fn a_slot_holding_different_mail_is_refused_untouched() {
    let mut lead = lead_holding(ItemId::TROPIC_MAIL);
    let mut byte = 0;
    let mut all = array_with(0, ItemId::WOOD_MAIL);
    let before = all;

    assert!(!holds_valid_mail(&lead, byte, &all));
    assert_eq!(
        take_mail_from_mon(&mut lead, &mut byte, &mut all),
        Err(TakeMailError::InvalidSource)
    );
    assert_eq!(all, before);
}
