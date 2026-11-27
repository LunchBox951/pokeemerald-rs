//! The TAKE MAIL flow (`CursorCb_TakeMail`, `pokeemerald/src/party_menu.c:3399-3478`)
//! against a fake lead: the prompts, the YES/NO window, and which transfer each
//! answer runs. What the transfers do to the lead and the save is
//! `crate::flow`'s `take_lead_mail_tests`.

use super::test_support::{pressed, FakeTarget, FRAME_BUDGET};
use super::{
    synthetic_start_menu, MailMessage, ReturnMailToBag, SendMailToPc, StartMenu, StartMenuOutcome,
};
use crate::game_save::SaveFileStatus;
use platform::Buttons;

#[derive(Clone, Copy)]
enum Answer {
    Yes,
    NoRow,
    B,
}

struct Run {
    messages: Vec<MailMessage>,
    /// The cursor row each YES/NO window opened on.
    opened_on: Vec<u8>,
    outcome: StartMenuOutcome,
}

fn target() -> FakeTarget {
    FakeTarget::new(SaveFileStatus::Ok, false)
}

fn drive(target: &mut FakeTarget, answers: &[Answer]) -> Run {
    let mut menu = synthetic_start_menu();
    menu.begin_take_mail();
    let mut run = Run {
        messages: Vec::new(),
        opened_on: Vec::new(),
        outcome: StartMenuOutcome::Open,
    };
    let mut answered = 0;
    for _ in 0..FRAME_BUDGET {
        let buttons = if let Some(cursor) = menu.yes_no_cursor() {
            if run.opened_on.len() == answered {
                run.opened_on.push(cursor);
            }
            match answers.get(answered).copied().unwrap_or(Answer::Yes) {
                Answer::NoRow if cursor == 0 => pressed(Buttons::DOWN),
                Answer::Yes | Answer::NoRow => {
                    answered += 1;
                    pressed(Buttons::A)
                }
                Answer::B => {
                    answered += 1;
                    pressed(Buttons::B)
                }
            }
        } else {
            pressed(Buttons::A)
        };
        let outcome = menu.tick(buttons, target);
        let shown = menu.mail_messages_shown();
        if !shown.is_empty() {
            run.messages = shown;
        }
        if outcome == StartMenuOutcome::Closed {
            run.outcome = outcome;
            return run;
        }
    }
    panic!(
        "the TAKE MAIL flow wedged; messages so far: {:?}",
        run.messages
    );
}

#[test]
fn the_first_prompt_is_upstreams_and_its_window_opens_on_yes() {
    let mut fake = target();
    let run = drive(&mut fake, &[Answer::Yes]);
    assert_eq!(run.messages[0], MailMessage::SendMailToPc);
    assert_eq!(
        run.opened_on,
        [0],
        "PartyMenuDisplayYesNoMenu starts on YES"
    );
}

#[test]
fn the_yes_no_window_waits_for_the_prompt_to_finish_printing() {
    let mut menu: StartMenu = synthetic_start_menu();
    menu.begin_take_mail();
    let mut fake = target();
    menu.tick(pressed(Buttons::A), &mut fake);
    assert_eq!(menu.mail_messages_shown(), [MailMessage::SendMailToPc]);
    assert_eq!(menu.yes_no_cursor(), None, "the text is still printing");
    assert!(menu.taking_mail());
}

#[test]
fn yes_sends_the_mail_to_the_pc_and_says_so() {
    let mut fake = target();
    let run = drive(&mut fake, &[Answer::Yes]);
    assert_eq!(
        run.messages,
        [MailMessage::SendMailToPc, MailMessage::SentToPc]
    );
    assert_eq!((fake.sends, fake.returns), (1, 0));
    assert_eq!(run.outcome, StartMenuOutcome::Closed);
}

#[test]
fn a_full_mailbox_is_refused_with_its_own_text() {
    let mut fake = target();
    fake.send_result = SendMailToPc::MailboxFull;
    let run = drive(&mut fake, &[Answer::Yes]);
    assert_eq!(
        run.messages,
        [MailMessage::SendMailToPc, MailMessage::MailboxFull]
    );
    assert_eq!((fake.sends, fake.returns), (1, 0));
}

#[test]
fn nothing_to_send_ends_the_flow_without_inventing_text() {
    let mut fake = target();
    fake.send_result = SendMailToPc::NothingToSend;
    let run = drive(&mut fake, &[Answer::Yes]);
    assert_eq!(run.messages, [MailMessage::SendMailToPc]);
    assert_eq!(run.outcome, StartMenuOutcome::Closed);
}

#[test]
fn no_and_b_both_ask_about_losing_the_message_before_anything_moves() {
    for first in [Answer::NoRow, Answer::B] {
        for second in [Answer::NoRow, Answer::B] {
            let mut fake = target();
            let run = drive(&mut fake, &[first, second]);
            assert_eq!(
                run.messages,
                [MailMessage::SendMailToPc, MailMessage::MessageWillBeLost]
            );
            assert_eq!(run.opened_on, [0, 0]);
            assert_eq!((fake.sends, fake.returns), (0, 0), "both answers were NO");
            assert_eq!(run.outcome, StartMenuOutcome::Closed);
        }
    }
}

#[test]
fn yes_to_losing_the_message_returns_the_mail_to_the_bag() {
    let mut fake = target();
    let run = drive(&mut fake, &[Answer::NoRow, Answer::Yes]);
    assert_eq!(
        run.messages,
        [
            MailMessage::SendMailToPc,
            MailMessage::MessageWillBeLost,
            MailMessage::TakenFromPokemon
        ]
    );
    assert_eq!((fake.sends, fake.returns), (0, 1));
}

#[test]
fn a_full_bag_refuses_the_return_with_the_bag_full_text() {
    let mut fake = target();
    fake.return_result = ReturnMailToBag::BagFull;
    let run = drive(&mut fake, &[Answer::B, Answer::Yes]);
    assert_eq!(
        run.messages,
        [
            MailMessage::SendMailToPc,
            MailMessage::MessageWillBeLost,
            MailMessage::BagFull
        ]
    );
}

#[test]
fn nothing_to_return_ends_the_flow_without_inventing_text() {
    let mut fake = target();
    fake.return_result = ReturnMailToBag::NothingToReturn;
    let run = drive(&mut fake, &[Answer::B, Answer::Yes]);
    assert_eq!(
        run.messages,
        [MailMessage::SendMailToPc, MailMessage::MessageWillBeLost]
    );
}

#[test]
fn a_result_message_holds_the_menu_until_a_fresh_press() {
    let mut menu = synthetic_start_menu();
    menu.begin_take_mail();
    let mut fake = target();
    let mut saw_result = false;
    for _ in 0..FRAME_BUDGET {
        let buttons = if menu.yes_no_cursor().is_some() {
            pressed(Buttons::A)
        } else if saw_result {
            // Nothing pressed: the PAUSE_UNTIL_PRESS wait must not time out.
            platform::ButtonState::new()
        } else {
            pressed(Buttons::A)
        };
        let outcome = menu.tick(buttons, &mut fake);
        if menu.mail_messages_shown().contains(&MailMessage::SentToPc) {
            saw_result = true;
        }
        if saw_result {
            for _ in 0..600 {
                assert_eq!(
                    menu.tick(platform::ButtonState::new(), &mut fake),
                    StartMenuOutcome::Open
                );
            }
            // A press that arrives while the text is still printing speeds it
            // up and, once printed, a fresh press closes it.
            for _ in 0..FRAME_BUDGET {
                if menu.tick(pressed(Buttons::A), &mut fake) == StartMenuOutcome::Closed {
                    return;
                }
            }
            panic!("a fresh A never closed the result message");
        }
        assert_eq!(outcome, StartMenuOutcome::Open);
    }
    panic!("the result message never opened");
}
