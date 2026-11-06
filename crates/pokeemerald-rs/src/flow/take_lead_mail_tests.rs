//! The start menu's TAKE entry on a lead holding mail: upstream's `MENU_MAIL`
//! and `CursorCb_TakeMail` (`pokeemerald/src/party_menu.c:2631-2634`,
//! `:3399-3478`) in place of the item TAKE text, driven through a real
//! [`OverworldPhase`]. The flow's prompts and answers are
//! `start_menu::take_mail_tests`'s; this file owns what each answer does to
//! the lead, the bag, the mail array and the save.

use super::overworld_phase::OverworldPhase;
use super::save_continue_support::{new_game_phase, settle};
use super::tests::{pressed, TempSave};
use crate::flow::lead_screen::loaded_holding;
use crate::game_save::SaveSlot;
use crate::new_game;
use crate::party::to_save_pokemon;
use crate::start_menu::{MailMessage, StartMenuItem};
use assets::items::ItemId;
use battle::Dex;
use engine::save::bag::{ItemSlot, ITEMS_COUNT};
use engine::save::{Mail, SaveBlock1, MAIL_COUNT, MAIL_NONE, MAIL_PARTY_SLOTS};
use platform::Buttons;

const ALL_MAIL: [ItemId; 12] = [
    ItemId::ORANGE_MAIL,
    ItemId::HARBOR_MAIL,
    ItemId::GLITTER_MAIL,
    ItemId::MECH_MAIL,
    ItemId::WOOD_MAIL,
    ItemId::WAVE_MAIL,
    ItemId::BEAD_MAIL,
    ItemId::SHADOW_MAIL,
    ItemId::TROPIC_MAIL,
    ItemId::DREAM_MAIL,
    ItemId::FAB_MAIL,
    ItemId::RETRO_MAIL,
];

#[derive(Clone, Copy)]
enum Answer {
    Yes,
    No,
    B,
}

fn sample_mail(item: ItemId, seed: u8) -> Mail {
    Mail {
        words: [0x0100 | u16::from(seed); 9],
        player_name: [seed; 8],
        trainer_id: [seed + 1; 4],
        species: 300 + u16::from(seed),
        item_id: item,
        padding: [seed + 2, seed + 3],
    }
}

/// A phase whose lead sits in party `slot`, holds `item`, and carries the
/// mail in party mail slot `mail_id`.
fn phase_with_mail_lead(slot: usize, mail_id: u8, item: ItemId) -> OverworldPhase {
    let mut phase = new_game_phase();
    settle(&mut phase);
    let lead = loaded_holding(&new_game::provisional_starter(), item);
    let mut record = to_save_pokemon(&Dex::new(), &lead);
    record.mail = mail_id;
    phase.set_backed_lead_for_test(slot, record, lead);
    if let Some(entry) = phase.save1.mail.get_mut(usize::from(mail_id)) {
        *entry = sample_mail(item, 9);
    }
    phase
}

/// Opens the menu and selects TAKE; the mail flow starts on the next frame.
fn select_take(phase: &mut OverworldPhase, save_slot: &mut SaveSlot) {
    phase.open_synthetic_start_menu();
    // The menu reopens on the entry last used.
    if phase.start_menu().unwrap().selected() != StartMenuItem::TakeItem {
        phase.advance_start_menu_frame(pressed(Buttons::DOWN), save_slot);
    }
    assert_eq!(
        phase.start_menu().unwrap().selected(),
        StartMenuItem::TakeItem
    );
    phase.advance_start_menu_frame(pressed(Buttons::A), save_slot);
}

/// Runs the flow to its end, answering YES/NO windows from `answers`, and
/// returns the messages it showed.
fn run_flow(
    phase: &mut OverworldPhase,
    save_slot: &mut SaveSlot,
    answers: &[Answer],
) -> Vec<MailMessage> {
    let mut messages = Vec::new();
    let mut answered = 0;
    for _ in 0..4_000 {
        let Some(menu) = phase.start_menu() else {
            return messages;
        };
        let shown = menu.mail_messages_shown();
        if !shown.is_empty() {
            messages = shown;
        }
        let buttons = match menu.yes_no_cursor() {
            Some(cursor) => match answers.get(answered).copied().unwrap_or(Answer::Yes) {
                Answer::Yes => {
                    answered += 1;
                    pressed(Buttons::A)
                }
                Answer::B => {
                    answered += 1;
                    pressed(Buttons::B)
                }
                Answer::No if cursor == 0 => pressed(Buttons::DOWN),
                Answer::No => {
                    answered += 1;
                    pressed(Buttons::A)
                }
            },
            None => pressed(Buttons::A),
        };
        phase.advance_start_menu_frame(buttons, save_slot);
    }
    panic!("the TAKE MAIL flow wedged after {messages:?}");
}

fn ordinary_quantity(phase: &OverworldPhase, item: ItemId) -> u16 {
    phase
        .save1()
        .bag
        .items
        .iter()
        .filter(|slot| slot.item_id == item.0)
        .map(|slot| slot.quantity)
        .sum()
}

/// Everything TAKE MAIL can touch, for before/after comparison.
#[derive(Debug, PartialEq)]
struct Snapshot {
    mail: [Mail; MAIL_COUNT],
    bag: engine::save::Bag,
    party: [engine::save::Pokemon; MAIL_PARTY_SLOTS],
    lead_item: ItemId,
    lead_mail_byte: u8,
}

fn snapshot(phase: &OverworldPhase) -> Snapshot {
    Snapshot {
        mail: phase.save1().mail,
        bag: phase.save1().bag.clone(),
        party: phase.save1().player_party,
        lead_item: phase.lead_for_test().unwrap().held_item(),
        lead_mail_byte: phase.lead_record_for_test().unwrap().mail,
    }
}

fn reload(save1: &SaveBlock1) -> SaveBlock1 {
    SaveBlock1::from_bytes(&save1.to_bytes(0x1234_5678), 0x1234_5678).unwrap()
}

fn fill_mailbox(phase: &mut OverworldPhase, except: &[usize]) {
    for slot in MAIL_PARTY_SLOTS..MAIL_COUNT {
        if !except.contains(&slot) {
            phase.save1.mail[slot] = sample_mail(ItemId::DREAM_MAIL, u8::try_from(slot).unwrap());
        }
    }
}

#[test]
fn a_lead_holding_any_mail_item_reaches_the_mail_prompt_not_the_item_text() {
    for item in ALL_MAIL {
        let temp = TempSave::new("take-mail-prompt");
        let mut save_slot = temp.slot();
        let mut phase = phase_with_mail_lead(0, 0, item);
        let before = snapshot(&phase);

        select_take(&mut phase, &mut save_slot);
        // First frame of the flow, then wait for the prompt's window.
        for _ in 0..2_000 {
            if phase.start_menu().unwrap().yes_no_cursor().is_some() {
                break;
            }
            phase.advance_start_menu_frame(pressed(Buttons::A), &mut save_slot);
        }

        let menu = phase.start_menu().expect("the flow keeps the menu open");
        assert!(menu.taking_mail(), "{item:?}");
        assert_eq!(
            menu.mail_messages_shown(),
            [MailMessage::SendMailToPc],
            "{item:?}: gText_SendMailToPC, not 'isn't holding anything' or 'is holding MAIL'"
        );
        assert_eq!(menu.yes_no_cursor(), Some(0), "{item:?}: the choice is up");
        assert_eq!(snapshot(&phase), before, "{item:?}: nothing moves yet");
        // The frame composes with the prompt and the YES/NO window up.
        let _ = phase.compose_frame();
    }
}

#[test]
fn yes_files_the_mail_in_the_first_free_mailbox_slot_and_the_save_follows() {
    let temp = TempSave::new("take-mail-yes");
    let mut save_slot = temp.slot();
    // A non-zero party slot and mail id catch any hard-coded zero.
    let mut phase = phase_with_mail_lead(2, 3, ItemId::HARBOR_MAIL);
    let original = phase.save1().mail[3];
    // The first mailbox slot is taken, so the mail must land in the second.
    phase.save1.mail[MAIL_PARTY_SLOTS] = sample_mail(ItemId::WOOD_MAIL, 1);
    let bag = phase.save1().bag.clone();

    select_take(&mut phase, &mut save_slot);
    let messages = run_flow(&mut phase, &mut save_slot, &[Answer::Yes]);

    assert_eq!(messages, [MailMessage::SendMailToPc, MailMessage::SentToPc]);
    assert!(phase.start_menu().is_none(), "the flow closes the menu");
    let saved = phase.save1();
    assert_eq!(
        saved.mail[MAIL_PARTY_SLOTS + 1],
        original,
        "whole entry moved"
    );
    assert_eq!(
        saved.mail[3].item_id,
        ItemId::NONE,
        "the source slot is freed"
    );
    assert_eq!(
        saved.mail[3].words, original.words,
        "its message stays, as upstream"
    );
    assert_eq!(saved.mail[MAIL_PARTY_SLOTS].item_id, ItemId::WOOD_MAIL);
    assert_eq!(saved.bag, bag, "sending to the PC does not touch the bag");
    assert_eq!(phase.lead_for_test().unwrap().held_item(), ItemId::NONE);
    assert_eq!(phase.lead_record_for_test().unwrap().mail, MAIL_NONE);
    let slot_record = &saved.player_party[2];
    assert_eq!(slot_record.mail, MAIL_NONE, "the saved slot follows");
    let reloaded_party = crate::party::from_save_pokemon(&Dex::new(), slot_record).unwrap();
    assert_eq!(reloaded_party.held_item(), ItemId::NONE);

    let reloaded = reload(saved);
    assert_eq!(
        reloaded.mail, saved.mail,
        "the save blocks round-trip the move"
    );
    assert_eq!(reloaded.player_party[2].mail, MAIL_NONE);
}

#[test]
fn the_last_mailbox_slot_is_usable() {
    let temp = TempSave::new("take-mail-last-slot");
    let mut save_slot = temp.slot();
    let mut phase = phase_with_mail_lead(0, 0, ItemId::ORANGE_MAIL);
    let original = phase.save1().mail[0];
    fill_mailbox(&mut phase, &[MAIL_COUNT - 1]);

    select_take(&mut phase, &mut save_slot);
    let messages = run_flow(&mut phase, &mut save_slot, &[Answer::Yes]);

    assert_eq!(messages.last(), Some(&MailMessage::SentToPc));
    assert_eq!(phase.save1().mail[MAIL_COUNT - 1], original);
}

#[test]
fn a_full_mailbox_refuses_and_changes_nothing() {
    let temp = TempSave::new("take-mail-full");
    let mut save_slot = temp.slot();
    let mut phase = phase_with_mail_lead(0, 0, ItemId::ORANGE_MAIL);
    fill_mailbox(&mut phase, &[]);
    let before = snapshot(&phase);

    select_take(&mut phase, &mut save_slot);
    let messages = run_flow(&mut phase, &mut save_slot, &[Answer::Yes]);

    assert_eq!(
        messages,
        [MailMessage::SendMailToPc, MailMessage::MailboxFull]
    );
    assert_eq!(snapshot(&phase), before);
}

#[test]
fn a_mailbox_whose_only_free_slots_are_party_slots_is_full() {
    let temp = TempSave::new("take-mail-party-slots-free");
    let mut save_slot = temp.slot();
    let mut phase = phase_with_mail_lead(0, 0, ItemId::ORANGE_MAIL);
    fill_mailbox(&mut phase, &[]);
    // Party mail slots 1..6 are empty already; they are not PC capacity.
    assert!(phase.save1().mail[1].is_empty());
    let before = snapshot(&phase);

    select_take(&mut phase, &mut save_slot);
    run_flow(&mut phase, &mut save_slot, &[Answer::Yes]);

    assert_eq!(snapshot(&phase), before);
}

#[test]
fn no_then_no_and_b_then_b_change_nothing() {
    for (first, second) in [(Answer::No, Answer::No), (Answer::B, Answer::B)] {
        let temp = TempSave::new("take-mail-no");
        let mut save_slot = temp.slot();
        let mut phase = phase_with_mail_lead(0, 0, ItemId::MECH_MAIL);
        let before = snapshot(&phase);

        select_take(&mut phase, &mut save_slot);
        let messages = run_flow(&mut phase, &mut save_slot, &[first, second]);

        assert_eq!(
            messages,
            [MailMessage::SendMailToPc, MailMessage::MessageWillBeLost]
        );
        assert_eq!(snapshot(&phase), before);
        assert!(phase.start_menu().is_none());
    }
}

#[test]
fn yes_to_losing_the_message_returns_the_item_to_the_bag() {
    let temp = TempSave::new("take-mail-lose");
    let mut save_slot = temp.slot();
    let mut phase = phase_with_mail_lead(4, 5, ItemId::SHADOW_MAIL);
    fill_mailbox(&mut phase, &[]);
    let mailbox = phase.save1().mail;
    assert_eq!(ordinary_quantity(&phase, ItemId::SHADOW_MAIL), 0);

    select_take(&mut phase, &mut save_slot);
    let messages = run_flow(&mut phase, &mut save_slot, &[Answer::No, Answer::Yes]);

    assert_eq!(
        messages,
        [
            MailMessage::SendMailToPc,
            MailMessage::MessageWillBeLost,
            MailMessage::TakenFromPokemon
        ]
    );
    assert_eq!(ordinary_quantity(&phase, ItemId::SHADOW_MAIL), 1);
    let saved = phase.save1();
    assert_eq!(saved.mail[5].item_id, ItemId::NONE);
    assert_eq!(saved.mail[MAIL_PARTY_SLOTS..], mailbox[MAIL_PARTY_SLOTS..]);
    assert_eq!(phase.lead_for_test().unwrap().held_item(), ItemId::NONE);
    assert_eq!(phase.lead_record_for_test().unwrap().mail, MAIL_NONE);
    assert_eq!(saved.player_party[4].mail, MAIL_NONE);
    assert_eq!(reload(saved).bag, saved.bag);
}

#[test]
fn a_full_bag_keeps_the_mail_on_the_lead() {
    let temp = TempSave::new("take-mail-bag-full");
    let mut save_slot = temp.slot();
    let mut phase = phase_with_mail_lead(0, 0, ItemId::TROPIC_MAIL);
    for (index, slot) in phase
        .save1
        .bag
        .items
        .iter_mut()
        .enumerate()
        .take(ITEMS_COUNT)
    {
        *slot = ItemSlot {
            item_id: 14 + u16::try_from(index).unwrap(),
            quantity: 1,
        };
    }
    let before = snapshot(&phase);

    select_take(&mut phase, &mut save_slot);
    let messages = run_flow(&mut phase, &mut save_slot, &[Answer::B, Answer::Yes]);

    assert_eq!(
        messages,
        [
            MailMessage::SendMailToPc,
            MailMessage::MessageWillBeLost,
            MailMessage::BagFull
        ]
    );
    assert_eq!(snapshot(&phase), before);
}

#[test]
fn a_mail_holder_whose_record_names_no_mail_is_refused_untouched() {
    for mail_byte in [MAIL_NONE, 6, 15, 0xFE] {
        let temp = TempSave::new("take-mail-malformed");
        let mut save_slot = temp.slot();
        let mut phase = phase_with_mail_lead(0, 0, ItemId::FAB_MAIL);
        let lead = phase.lead_for_test().unwrap().clone();
        let mut record = *phase.lead_record_for_test().unwrap();
        record.mail = mail_byte;
        phase.set_backed_lead_for_test(0, record, lead);
        let before = snapshot(&phase);

        select_take(&mut phase, &mut save_slot);
        let messages = run_flow(&mut phase, &mut save_slot, &[Answer::Yes]);
        assert_eq!(messages, [MailMessage::SendMailToPc], "byte {mail_byte:#x}");
        assert_eq!(snapshot(&phase), before, "byte {mail_byte:#x}");

        select_take(&mut phase, &mut save_slot);
        run_flow(&mut phase, &mut save_slot, &[Answer::No, Answer::Yes]);
        assert_eq!(snapshot(&phase), before, "no item came from nowhere");
    }
}

#[test]
fn a_record_pointing_at_other_mail_is_refused_untouched() {
    let temp = TempSave::new("take-mail-mismatch");
    let mut save_slot = temp.slot();
    let mut phase = phase_with_mail_lead(0, 0, ItemId::FAB_MAIL);
    phase.save1.mail[0].item_id = ItemId::WOOD_MAIL;
    let before = snapshot(&phase);

    select_take(&mut phase, &mut save_slot);
    run_flow(&mut phase, &mut save_slot, &[Answer::No, Answer::Yes]);

    assert_eq!(snapshot(&phase), before);
}

/// Master Ball shares mail's item type but is not mail: it takes the ordinary
/// item path, which closes the menu and shows the item text.
#[test]
fn a_master_ball_holder_still_takes_the_item_path() {
    let temp = TempSave::new("take-mail-master-ball");
    let mut save_slot = temp.slot();
    let mut phase = phase_with_mail_lead(0, MAIL_NONE, ItemId::MASTER_BALL);

    select_take(&mut phase, &mut save_slot);

    assert!(
        phase.start_menu().is_none(),
        "no mail flow, the menu closed"
    );
    assert_eq!(phase.lead_for_test().unwrap().held_item(), ItemId::NONE);
}

/// A second flush must not bring the mail back: the lead owns its record, and
/// a flush merges onto it.
#[test]
fn a_later_save_flush_keeps_the_mail_gone() {
    let temp = TempSave::new("take-mail-reflush");
    let mut save_slot = temp.slot();
    let mut phase = phase_with_mail_lead(1, 0, ItemId::WAVE_MAIL);

    select_take(&mut phase, &mut save_slot);
    run_flow(&mut phase, &mut save_slot, &[Answer::Yes]);
    phase.open_synthetic_start_menu();
    // The cursor stayed on TAKE; SAVE is the entry above it.
    phase.advance_start_menu_frame(pressed(Buttons::UP), &mut save_slot);
    assert_eq!(phase.start_menu().unwrap().selected(), StartMenuItem::Save);
    phase.advance_start_menu_frame(pressed(Buttons::A), &mut save_slot);
    for _ in 0..2_000 {
        if phase.start_menu().is_none() {
            break;
        }
        phase.advance_start_menu_frame(pressed(Buttons::A), &mut save_slot);
    }

    assert!(
        temp.slot().load().status.menu_shows_continue(),
        "setup: SAVE (the first entry) wrote the file"
    );
    assert_eq!(phase.save1().player_party[1].mail, MAIL_NONE);
    assert_eq!(phase.lead_record_for_test().unwrap().mail, MAIL_NONE);
}
