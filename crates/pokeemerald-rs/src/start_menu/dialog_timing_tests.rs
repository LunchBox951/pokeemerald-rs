//! Frame-exact SAVE dispatch and result transitions against a reference printer.

use super::test_support::{pressed, FakeTarget, FRAME_BUDGET};
use super::text::SaveMessage;
use super::{SaveDialog, SaveDialogOutcome, SaveMode, SaveTarget, StartMenuChrome};
use crate::game_save::SaveFileStatus;
use crate::overworld::dialog::confirm_printer_input;
use crate::overworld::DialogOutcome;
use engine::text::Token;
use platform::{ButtonState, Buttons};

fn held(button: Buttons) -> ButtonState {
    let mut state = pressed(button);
    state.update(button);
    state
}

/// Ticks `dialog` and a plain reference `NpcDialog` copy of the same message
/// in lockstep, pinning the exact tick the message finishes printing without
/// hard-coding its frame count. Calls `before_finish` on every tick before
/// the one the reference closes on.
fn finish_message_in_lockstep(
    dialog: &mut SaveDialog,
    chrome: &StartMenuChrome,
    target: &mut FakeTarget,
    tokens: Vec<Token>,
    buttons: ButtonState,
    mut before_finish: impl FnMut(&SaveDialog, &FakeTarget),
) {
    let mut reference = chrome.message_box(tokens, target.player_text_speed());
    for _ in 0..FRAME_BUDGET {
        let reference_finished =
            reference.tick(confirm_printer_input(buttons)) == DialogOutcome::Closed;
        assert_eq!(
            dialog.run(buttons, chrome, target),
            SaveDialogOutcome::InProgress
        );
        if reference_finished {
            return;
        }
        before_finish(dialog, target);
    }
    panic!("the save message must finish within {FRAME_BUDGET} frames");
}

/// Drives a fresh [`SaveDialog`] up to its first Yes/No window.
fn open_initial_choice(dialog: &mut SaveDialog, chrome: &StartMenuChrome, target: &mut FakeTarget) {
    assert_eq!(
        dialog.run(ButtonState::new(), chrome, target),
        SaveDialogOutcome::InProgress
    );
    for _ in 0..FRAME_BUDGET {
        assert_eq!(
            dialog.run(pressed(Buttons::A), chrome, target),
            SaveDialogOutcome::InProgress
        );
        if dialog.yes_no().is_some() {
            return;
        }
    }
    panic!("the initial choice must open within {FRAME_BUDGET} frames");
}

/// Drives a fresh [`SaveDialog`] through the empty-cartridge shortcut up to
/// its saving message just starting to print, with no overwrite prompt in
/// between.
fn begin_unprompted_saving_message(
    dialog: &mut SaveDialog,
    chrome: &StartMenuChrome,
    target: &mut FakeTarget,
) {
    open_initial_choice(dialog, chrome, target);
    assert_eq!(
        dialog.run(pressed(Buttons::A), chrome, target),
        SaveDialogOutcome::InProgress
    );
    assert_eq!(
        dialog.run(ButtonState::new(), chrome, target),
        SaveDialogOutcome::InProgress
    );
}

#[test]
fn initial_yes_no_opens_on_the_confirm_message_finish_tick() {
    let chrome = StartMenuChrome::synthetic();
    let mut target = FakeTarget::new(SaveFileStatus::Ok, false);
    let mut dialog = SaveDialog::new();

    assert_eq!(
        dialog.run(ButtonState::new(), &chrome, &mut target),
        SaveDialogOutcome::InProgress
    );
    assert!(
        dialog.message().is_none(),
        "the install tick builds no message"
    );
    assert_eq!(
        dialog.run(ButtonState::new(), &chrome, &mut target),
        SaveDialogOutcome::InProgress
    );
    assert!(
        dialog.message().is_none(),
        "the init tick builds no message"
    );
    assert_eq!(
        dialog.run(ButtonState::new(), &chrome, &mut target),
        SaveDialogOutcome::InProgress
    );
    assert!(
        dialog.message().is_some(),
        "the third tick must have built the confirm message"
    );
    finish_message_in_lockstep(
        &mut dialog,
        &chrome,
        &mut target,
        SaveMessage::ConfirmSave.tokens(),
        ButtonState::new(),
        |dialog, _| assert!(dialog.yes_no().is_none(), "nothing is queued yet"),
    );

    assert!(
        dialog.yes_no().is_some(),
        "the Yes/No window must be open on the tick the message finished"
    );
}

#[test]
fn overwrite_yes_no_opens_on_the_overwrite_message_finish_tick() {
    let chrome = StartMenuChrome::synthetic();
    let mut target = FakeTarget::new(SaveFileStatus::Ok, false);
    let mut dialog = SaveDialog::new();

    open_initial_choice(&mut dialog, &chrome, &mut target);
    assert_eq!(
        dialog.run(pressed(Buttons::A), &chrome, &mut target),
        SaveDialogOutcome::InProgress,
        "YES on the first prompt queues the overwrite prompt"
    );
    assert_eq!(
        dialog.run(ButtonState::new(), &chrome, &mut target),
        SaveDialogOutcome::InProgress,
        "this tick only starts the overwrite message printing"
    );
    finish_message_in_lockstep(
        &mut dialog,
        &chrome,
        &mut target,
        SaveMessage::AlreadySavedFile.tokens(),
        ButtonState::new(),
        |dialog, _| assert!(dialog.yes_no().is_none(), "nothing is queued yet"),
    );

    assert_eq!(
        dialog.yes_no().map(|menu| menu.cursor),
        Some(0),
        "the overwrite Yes/No window (defaulting to YES) must be open on \
         the tick the message finished"
    );
}

#[test]
fn store_runs_on_the_saving_message_finish_tick() {
    let chrome = StartMenuChrome::synthetic();
    let mut target = FakeTarget::new(SaveFileStatus::Empty, true);
    let mut dialog = SaveDialog::new();

    begin_unprompted_saving_message(&mut dialog, &chrome, &mut target);
    finish_message_in_lockstep(
        &mut dialog,
        &chrome,
        &mut target,
        SaveMessage::Saving.tokens(),
        ButtonState::new(),
        |_, target| assert!(target.writes.is_empty(), "nothing is stored yet"),
    );

    assert_eq!(
        target.writes,
        vec![SaveMode::OverwriteDifferentFile { prompted: false }],
        "the store must have run on the tick the message finished"
    );
}

/// Unlike the prompt and store dispatches above, the result path (see
/// [`SaveDialog::run`]) does not read input on the message's finish tick, so
/// a held A carried into that tick must never fire success or error early.
///
/// The drive up to and through the result message uses fresh A presses (a
/// page-break message still needs one to turn the page); only the
/// completion check below holds A without a fresh edge, the shape of an
/// already-held button.
#[test]
fn held_a_does_not_skip_the_result_transition_on_the_finish_tick() {
    for write_succeeds in [true, false] {
        let chrome = StartMenuChrome::synthetic();
        let mut target = FakeTarget::new(SaveFileStatus::Empty, true);
        target.write_succeeds = write_succeeds;
        let mut dialog = SaveDialog::new();

        begin_unprompted_saving_message(&mut dialog, &chrome, &mut target);
        for _ in 0..FRAME_BUDGET {
            assert_eq!(
                dialog.run(pressed(Buttons::A), &chrome, &mut target),
                SaveDialogOutcome::InProgress
            );
            if !target.writes.is_empty() {
                break;
            }
        }
        assert_eq!(
            target.writes.len(),
            1,
            "the store must have run exactly once"
        );

        let result_tokens: Vec<Token> = if write_succeeds {
            "STU saved the game."
                .chars()
                .map(Token::Char)
                .chain(std::iter::once(Token::End))
                .collect()
        } else {
            SaveMessage::SaveError.tokens()
        };
        // `finish_message_in_lockstep` asserts `InProgress` on every tick up
        // to and including the finish tick, which is the load-bearing check
        // that the dismissal transition never fires early.
        finish_message_in_lockstep(
            &mut dialog,
            &chrome,
            &mut target,
            result_tokens,
            pressed(Buttons::A),
            |_, _| {},
        );

        if write_succeeds {
            assert_eq!(
                dialog.run(held(Buttons::A), &chrome, &mut target),
                SaveDialogOutcome::Success,
                "the tick after completion must honor the already-held A"
            );
        } else {
            let mut outcome = SaveDialogOutcome::InProgress;
            for _ in 0..FRAME_BUDGET {
                outcome = dialog.run(held(Buttons::A), &chrome, &mut target);
                if outcome != SaveDialogOutcome::InProgress {
                    break;
                }
            }
            assert_eq!(
                outcome,
                SaveDialogOutcome::Error,
                "the error countdown must still resolve once it drains"
            );
        }
    }
}
