//! Unit tests for the field start menu: the item list, the cursor, the
//! geometry, and the SAVE flow, driven against a fake [`SaveTarget`] rather
//! than a real save file.
//!
//! `crate::flow::save_continue_tests` and its `save_continue_*` siblings
//! cover the same menu writing a real save and reloading it.

use super::text::SaveMessage;
use super::{
    menu_height, synthetic_start_menu, SaveDialog, SaveDialogOutcome, SaveMode, SaveTarget,
    StartMenu, StartMenuChrome, StartMenuItem, StartMenuOutcome, ITEMS, MENU_TILEMAP_LEFT,
    MENU_TILEMAP_TOP, MENU_WIDTH, YES_NO_TILEMAP_LEFT, YES_NO_TILEMAP_TOP,
};
use crate::game_save::SaveFileStatus;
use crate::overworld::dialog::confirm_printer_input;
use crate::overworld::DialogOutcome;
use engine::text::render::TextSpeed;
use engine::text::Token;
use platform::{ButtonState, Buttons};

fn pressed(button: Buttons) -> ButtonState {
    let mut state = ButtonState::new();
    state.update(button);
    state
}

/// A save medium that records what it was asked to do instead of touching a
/// file.
#[derive(Debug)]
struct FakeTarget {
    boot_status: SaveFileStatus,
    different_save_file: bool,
    write_succeeds: bool,
    /// Every mode `try_saving_data` was called with, in order.
    writes: Vec<SaveMode>,
}

impl FakeTarget {
    fn new(boot_status: SaveFileStatus, different_save_file: bool) -> Self {
        Self {
            boot_status,
            different_save_file,
            write_succeeds: true,
            writes: Vec::new(),
        }
    }
}

impl SaveTarget for FakeTarget {
    fn boot_status(&self) -> SaveFileStatus {
        self.boot_status
    }

    fn different_save_file(&self) -> bool {
        self.different_save_file
    }

    fn player_name(&self) -> Vec<Token> {
        "STU".chars().map(Token::Char).collect()
    }

    /// Fixed at MID: every frame budget in this file assumes MID's cadence.
    /// `crate::flow::save_continue_text_speed_tests` covers text-speed
    /// plumbing against the real `PhaseSaveTarget`.
    fn player_text_speed(&self) -> TextSpeed {
        TextSpeed::Mid
    }

    /// Clears `different_save_file` on an overwrite attempt even when the
    /// write fails, matching [`SaveTarget::try_saving_data`]'s contract.
    fn try_saving_data(&mut self, mode: SaveMode) -> bool {
        self.writes.push(mode);
        if matches!(mode, SaveMode::OverwriteDifferentFile { .. }) {
            self.different_save_file = false;
        }
        self.write_succeeds
    }
}

/// Frames one flow gets before a test calls it wedged -- generous enough for
/// the longest WARNING message at `TextSpeed::Mid`
/// (`crate::flow::save_continue_tests` documents the arithmetic).
const FRAME_BUDGET: usize = 4_000;

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

/// [`drive_reporting_outcome`], keeping only the prompt rows.
fn drive(menu: &mut StartMenu, target: &mut FakeTarget, answers: &[bool]) -> Vec<u8> {
    drive_reporting_outcome(menu, target, answers).0
}

/// Drives `menu` until it closes or its SAVE flow cancels back to the item
/// list, answering Yes/No prompts from `answers` (`true` = YES, defaulting
/// to YES).
///
/// Returns the cursor row each prompt opened on, and how the menu ended:
/// [`StartMenuOutcome::Closed`] when field control returns,
/// [`StartMenuOutcome::Open`] when the SAVE flow canceled back to the item
/// list. The two are not interchangeable, and `saving()` cannot tell them
/// apart: a successful or failed save also closes the menu while its SAVE
/// flow is still installed.
fn drive_reporting_outcome(
    menu: &mut StartMenu,
    target: &mut FakeTarget,
    answers: &[bool],
) -> (Vec<u8>, StartMenuOutcome) {
    let mut opened_on: Vec<u8> = Vec::new();
    let mut answered = 0usize;
    let mut entered = false;
    for _ in 0..FRAME_BUDGET {
        if menu.saving() {
            entered = true;
        } else if entered {
            return (opened_on, StartMenuOutcome::Open);
        }
        let buttons = match menu.yes_no_cursor() {
            Some(cursor) => {
                if opened_on.len() == answered {
                    opened_on.push(cursor);
                }
                let wants_yes = answers.get(answered).copied().unwrap_or(true);
                let desired = u8::from(!wants_yes);
                match cursor.cmp(&desired) {
                    std::cmp::Ordering::Equal => {
                        answered += 1;
                        pressed(Buttons::A)
                    }
                    std::cmp::Ordering::Greater => pressed(Buttons::UP),
                    std::cmp::Ordering::Less => pressed(Buttons::DOWN),
                }
            }
            None => pressed(Buttons::A),
        };
        if menu.tick(buttons, target) == StartMenuOutcome::Closed {
            return (opened_on, StartMenuOutcome::Closed);
        }
    }
    panic!("the save flow must terminate within {FRAME_BUDGET} frames");
}

#[test]
fn the_item_list_is_save_then_exit() {
    assert_eq!(ITEMS, [StartMenuItem::Save, StartMenuItem::Exit]);
    assert_eq!(StartMenuItem::Save.label(), "SAVE");
    assert_eq!(StartMenuItem::Exit.label(), "EXIT");
    assert_eq!(synthetic_start_menu().selected(), StartMenuItem::Save);
}

/// Item-window height is `numActions * 2 + 2` tiles
/// (`pokeemerald/src/menu.c:493`).
#[test]
fn item_window_geometry_matches_upstream() {
    assert_eq!(menu_height(2), 6, "this shell's own two-item menu");
    assert_eq!(menu_height(7), 16, "a larger menu");
    assert_eq!(
        (MENU_TILEMAP_LEFT, MENU_TILEMAP_TOP, MENU_WIDTH),
        (22, 1, 7)
    );
}

#[test]
fn the_item_cursor_wraps_in_both_directions() {
    let mut target = FakeTarget::new(SaveFileStatus::Empty, true);
    let mut menu = synthetic_start_menu();

    menu.tick(pressed(Buttons::UP), &mut target);
    assert_eq!(menu.selected(), StartMenuItem::Exit);
    menu.tick(pressed(Buttons::DOWN), &mut target);
    assert_eq!(menu.selected(), StartMenuItem::Save);
    menu.tick(pressed(Buttons::DOWN), &mut target);
    assert_eq!(menu.selected(), StartMenuItem::Exit);
}

#[test]
fn direction_precedes_a_and_a_precedes_close_buttons_in_the_same_frame() {
    let mut target = FakeTarget::new(SaveFileStatus::Ok, false);

    // DOWN+A off a fresh menu: EXIT is selected but not yet closed.
    let mut menu = synthetic_start_menu();
    assert_eq!(menu.selected(), StartMenuItem::Save);
    assert_eq!(
        menu.tick(pressed(Buttons::DOWN | Buttons::A), &mut target),
        StartMenuOutcome::Open,
        "the A must act on the row DOWN just moved it to, not on SAVE, \
         and EXIT must not have closed the menu yet"
    );
    assert_eq!(menu.selected(), StartMenuItem::Exit);
    assert!(!menu.saving(), "the same-frame A selected EXIT, not SAVE");
    assert_eq!(
        menu.tick(ButtonState::new(), &mut target),
        StartMenuOutcome::Closed,
        "EXIT closes on the next tick, reading no input"
    );

    // UP+A from EXIT: the cursor wraps back to SAVE and the same frame's A
    // starts the save flow.
    let mut menu = synthetic_start_menu();
    menu.tick(pressed(Buttons::DOWN), &mut target);
    assert_eq!(menu.selected(), StartMenuItem::Exit);
    assert_eq!(
        menu.tick(pressed(Buttons::UP | Buttons::A), &mut target),
        StartMenuOutcome::Open
    );
    assert_eq!(menu.selected(), StartMenuItem::Save);
    assert!(menu.saving(), "the same frame's A entered the SAVE flow");

    // A+START: A takes the same-frame selection, so START never closes it.
    let mut menu = synthetic_start_menu();
    assert_eq!(
        menu.tick(pressed(Buttons::A | Buttons::START), &mut target),
        StartMenuOutcome::Open
    );
    assert!(menu.saving());

    assert!(
        target.writes.is_empty(),
        "none of these frames reached the save medium"
    );
}

#[test]
fn exit_closes_next_tick_while_start_and_b_close_immediately_without_writing() {
    let mut target = FakeTarget::new(SaveFileStatus::Ok, false);

    let mut menu = synthetic_start_menu();
    menu.tick(pressed(Buttons::DOWN), &mut target);
    assert_eq!(menu.selected(), StartMenuItem::Exit);
    assert_eq!(
        menu.tick(pressed(Buttons::A), &mut target),
        StartMenuOutcome::Open,
        "A on EXIT only marks it pending this tick"
    );
    assert_eq!(
        menu.tick(ButtonState::new(), &mut target),
        StartMenuOutcome::Closed,
        "the pending EXIT closes the menu on the next tick"
    );

    for close_key in [Buttons::START, Buttons::B] {
        let mut menu = synthetic_start_menu();
        assert_eq!(
            menu.tick(pressed(close_key), &mut target),
            StartMenuOutcome::Closed,
            "{close_key:?} closes the menu"
        );
    }
    assert!(target.writes.is_empty(), "closing the menu writes nothing");
}

/// A new-game session over an empty cartridge is asked only whether to save
/// -- there is provably nothing to overwrite -- and the write records that
/// no prompt stood behind it.
#[test]
fn an_empty_cartridge_skips_the_overwrite_prompt() {
    let mut target = FakeTarget::new(SaveFileStatus::Empty, true);
    let mut menu = synthetic_start_menu();
    let prompts = drive(&mut menu, &mut target, &[]);

    assert_eq!(
        prompts,
        vec![0],
        "only the initial prompt, defaulting to YES"
    );
    assert_eq!(
        target.writes,
        vec![SaveMode::OverwriteDifferentFile { prompted: false }]
    );
}

/// The same shortcut applies to a corrupt cartridge -- a wrecked file is
/// not an adventure worth asking about.
#[test]
fn a_corrupt_cartridge_also_skips_the_overwrite_prompt() {
    let mut target = FakeTarget::new(SaveFileStatus::Corrupt, true);
    let mut menu = synthetic_start_menu();
    assert_eq!(drive(&mut menu, &mut target, &[]).len(), 1);
    assert_eq!(
        target.writes,
        vec![SaveMode::OverwriteDifferentFile { prompted: false }]
    );
}

/// A continued session saving over its own file is asked a second
/// confirmation, defaulting to YES, and writes with [`SaveMode::Normal`].
#[test]
fn a_continued_session_is_asked_twice_and_saves_normally() {
    let mut target = FakeTarget::new(SaveFileStatus::Ok, false);
    let mut menu = synthetic_start_menu();
    let prompts = drive(&mut menu, &mut target, &[]);

    assert_eq!(prompts, vec![0, 0], "both prompts default to YES");
    assert_eq!(target.writes, vec![SaveMode::Normal]);
}

/// A new-game session over someone else's save gets a WARNING prompt whose
/// Yes/No opens on **NO** (`pokeemerald/src/start_menu.c:1049-1053`).
/// Answering NO cancels back to the item list and writes nothing.
#[test]
fn the_different_save_file_warning_defaults_to_no_and_can_be_declined() {
    let mut target = FakeTarget::new(SaveFileStatus::Ok, true);
    let mut menu = synthetic_start_menu();
    let prompts = drive(&mut menu, &mut target, &[true, false]);

    assert_eq!(
        prompts,
        vec![0, 1],
        "the initial prompt defaults to YES; the WARNING to NO"
    );
    assert!(
        target.writes.is_empty(),
        "a declined WARNING writes nothing"
    );
    assert!(
        !menu.saving(),
        "cancellation returns the menu to the item list"
    );
}

/// Answering YES to the WARNING writes with `prompted` set, distinguishing
/// a consented overwrite from the empty-cartridge shortcut.
#[test]
fn answering_the_warning_writes_an_acknowledged_overwrite() {
    let mut target = FakeTarget::new(SaveFileStatus::Ok, true);
    let mut menu = synthetic_start_menu();
    drive(&mut menu, &mut target, &[true, true]);
    assert_eq!(
        target.writes,
        vec![SaveMode::OverwriteDifferentFile { prompted: true }]
    );
}

#[test]
fn declining_the_first_question_never_reaches_the_save_medium() {
    let mut target = FakeTarget::new(SaveFileStatus::Ok, false);
    let mut menu = synthetic_start_menu();
    drive(&mut menu, &mut target, &[false]);
    assert!(target.writes.is_empty());
    assert!(!menu.saving());
}

/// B on a Yes/No prompt always answers NO
/// (`pokeemerald/src/menu.c:1023-1026`), even with the cursor on YES.
#[test]
fn b_on_a_prompt_answers_no_even_with_the_cursor_on_yes() {
    let mut target = FakeTarget::new(SaveFileStatus::Ok, false);
    let mut menu = synthetic_start_menu();

    menu.tick(pressed(Buttons::A), &mut target);
    for _ in 0..FRAME_BUDGET {
        if menu.yes_no_cursor().is_some() {
            break;
        }
        menu.tick(pressed(Buttons::A), &mut target);
    }
    assert_eq!(menu.yes_no_cursor(), Some(0), "the cursor is on YES");

    assert_eq!(
        menu.tick(pressed(Buttons::B), &mut target),
        StartMenuOutcome::Open
    );
    assert!(!menu.saving(), "B cancelled the save");
    assert!(target.writes.is_empty());
}

/// A failed write still closes the start menu -- the player returns to the
/// field knowing the save did not happen, not trapped in a menu.
#[test]
fn a_failed_write_still_closes_the_menu() {
    let mut target = FakeTarget::new(SaveFileStatus::Ok, false);
    target.write_succeeds = false;
    let mut menu = synthetic_start_menu();
    let (_prompts, outcome) = drive_reporting_outcome(&mut menu, &mut target, &[]);
    assert_eq!(target.writes, vec![SaveMode::Normal]);
    assert_eq!(
        outcome,
        StartMenuOutcome::Closed,
        "failure shares success's arm: the menu ends, it does not drop the \
         player back on the item list"
    );
}

/// A failed overwrite still clears `different_save_file`, so the next SAVE
/// in the same session meets the ordinary confirmation (default YES) rather
/// than the WARNING (default NO) all over again.
#[test]
fn a_failed_overwrite_still_retires_the_different_save_file_warning() {
    let mut target = FakeTarget::new(SaveFileStatus::Ok, true);
    target.write_succeeds = false;

    let mut menu = synthetic_start_menu();
    let first_attempt_prompts = drive(&mut menu, &mut target, &[true, true]);
    assert_eq!(
        first_attempt_prompts,
        vec![0, 1],
        "the initial prompt defaults to YES; the WARNING to NO"
    );
    assert_eq!(
        target.writes,
        vec![SaveMode::OverwriteDifferentFile { prompted: true }]
    );
    assert!(
        !target.different_save_file,
        "the overwrite attempt clears the flag even though it failed"
    );

    target.write_succeeds = true;
    let mut menu = synthetic_start_menu();
    let retry_prompts = drive(&mut menu, &mut target, &[]);
    assert_eq!(
        retry_prompts,
        vec![0, 0],
        "the retry asks the ordinary confirmation, which defaults to YES -- \
         a second WARNING would open its Yes/No on NO"
    );
    assert_eq!(
        target.writes,
        vec![
            SaveMode::OverwriteDifferentFile { prompted: true },
            SaveMode::Normal
        ],
        "the retry uses the normal save mode"
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

/// The composed frame really draws something over the map: a start menu
/// that rendered nothing would still pass every state-machine test while
/// being invisible to the player.
#[test]
fn an_open_menu_paints_its_window_and_leaves_the_rest_alone() {
    use rendering::{Framebuffer, Rgb888};

    let marker = Rgb888 {
        r: 10,
        g: 20,
        b: 30,
    };
    let mut base = Framebuffer::new();
    base.fill(marker);
    let composed = synthetic_start_menu().compose_over(base);

    let inside_item_window = composed.pixel(
        usize::try_from(MENU_TILEMAP_LEFT * 8 + 4).unwrap(),
        usize::try_from(MENU_TILEMAP_TOP * 8 + 4).unwrap(),
    );
    assert_ne!(
        inside_item_window,
        Some(marker),
        "the menu window must be painted"
    );
    let far_from_any_window = composed.pixel(4, 150);
    assert_eq!(
        far_from_any_window,
        Some(marker),
        "the overworld behind the menu must still show"
    );
}

/// Selecting SAVE must not blank the screen for a frame: the item window
/// stays up until the SAVE flow has a replacement message ready, which is
/// not until the third tick after the selecting A press (see
/// [`StartMenu::compose_over`]).
#[test]
fn selecting_save_keeps_the_item_window_until_its_message_exists() {
    use rendering::{Framebuffer, Rgb888};

    let marker = Rgb888 {
        r: 10,
        g: 20,
        b: 30,
    };
    let mut base = Framebuffer::new();
    base.fill(marker);
    let item_pixel = (
        usize::try_from(MENU_TILEMAP_LEFT * 8 + 4).unwrap(),
        usize::try_from(MENU_TILEMAP_TOP * 8 + 4).unwrap(),
    );

    let mut menu = synthetic_start_menu();
    let mut target = FakeTarget::new(SaveFileStatus::Ok, false);
    assert_eq!(menu.selected(), StartMenuItem::Save);

    let assert_item_window_remains = |menu: &StartMenu, frame: &str| {
        let dialog = menu.save.as_ref().expect("SAVE installs its dialog");
        assert!(
            dialog.message().is_none(),
            "{frame}: no save message may exist yet"
        );
        assert_ne!(
            menu.compose_over(base.clone())
                .pixel(item_pixel.0, item_pixel.1),
            Some(marker),
            "{frame}: the frame must still draw the item window, not a bare \
             overworld frame"
        );
    };

    assert_eq!(
        menu.tick(pressed(Buttons::A), &mut target),
        StartMenuOutcome::Open
    );
    assert_item_window_remains(&menu, "the tick that selects SAVE");

    assert_eq!(
        menu.tick(ButtonState::new(), &mut target),
        StartMenuOutcome::Open
    );
    assert_item_window_remains(&menu, "the first tick after selecting SAVE");

    assert_eq!(
        menu.tick(ButtonState::new(), &mut target),
        StartMenuOutcome::Open
    );
    assert_item_window_remains(&menu, "the second tick after selecting SAVE");

    assert_eq!(
        menu.tick(ButtonState::new(), &mut target),
        StartMenuOutcome::Open
    );
    assert!(
        menu.save
            .as_ref()
            .expect("still saving")
            .message()
            .is_some(),
        "the third tick after selecting SAVE must have built the confirm message"
    );
    assert_eq!(
        menu.compose_over(base).pixel(item_pixel.0, item_pixel.1),
        Some(marker),
        "once the confirm message exists the item window must be gone"
    );
}

/// The content fill and glyph colours must come from the message-box
/// palette, never the selected standard frame's own palette, which only
/// borders the window (`pokeemerald/src/text_window.c:93-112`). The
/// standard frame's palette is seeded with sentinel colours a correct
/// implementation never reads for content or glyphs, so a regression would
/// show up as a wrong colour here instead of silently matching by
/// coincidence.
#[test]
fn standard_window_uses_message_palette_for_content_and_standard_palette_for_border() {
    use assets::{Glyph, GLYPH_PIXELS};
    use engine::text::render::RevealedGlyph;
    use rendering::{Framebuffer, Rgb888};

    const STD_SENTINEL: Rgb888 = Rgb888 { r: 200, g: 0, b: 0 };
    const STD_BORDER: Rgb888 = Rgb888 { r: 0, g: 0, b: 200 };
    const MSG_FILL: Rgb888 = Rgb888 {
        r: 10,
        g: 200,
        b: 10,
    };
    const MSG_FG: Rgb888 = Rgb888 {
        r: 20,
        g: 20,
        b: 220,
    };
    const MSG_SHADOW: Rgb888 = Rgb888 {
        r: 220,
        g: 220,
        b: 20,
    };

    // The three indices [`StartMenuChrome`] reads content fill, glyph
    // foreground, and glyph shadow colours from.
    const CONTENT_FILL_PALETTE_INDEX: u8 = 1;
    const GLYPH_FOREGROUND_PALETTE_INDEX: u8 = 2;
    const GLYPH_SHADOW_PALETTE_INDEX: u8 = 3;
    const BORDER_PALETTE_INDEX: u8 = 9;
    // The glyph colour-slot order: 0 is transparent, 1 is foreground, 2 is
    // shadow.
    const FOREGROUND_COLOR_SLOT: u8 = 1;
    const SHADOW_COLOR_SLOT: u8 = 2;
    const FOREGROUND_PIXEL: usize = 0; // glyph-local (0, 0)
    const SHADOW_PIXEL: usize = 5; // glyph-local (5, 0)

    let mut chrome = StartMenuChrome::synthetic();
    chrome.std_frame.palette[usize::from(CONTENT_FILL_PALETTE_INDEX)] = STD_SENTINEL;
    chrome.std_frame.palette[usize::from(GLYPH_FOREGROUND_PALETTE_INDEX)] = STD_SENTINEL;
    chrome.std_frame.palette[usize::from(GLYPH_SHADOW_PALETTE_INDEX)] = STD_SENTINEL;
    chrome.std_frame.palette[usize::from(BORDER_PALETTE_INDEX)] = STD_BORDER;
    // Every tile of the border sheet reads this same index, so any drawn
    // border pixel must be [`STD_BORDER`].
    chrome.std_frame.pixels.fill(BORDER_PALETTE_INDEX);
    chrome.message_frame.palette[usize::from(CONTENT_FILL_PALETTE_INDEX)] = MSG_FILL;
    chrome.message_frame.palette[usize::from(GLYPH_FOREGROUND_PALETTE_INDEX)] = MSG_FG;
    chrome.message_frame.palette[usize::from(GLYPH_SHADOW_PALETTE_INDEX)] = MSG_SHADOW;

    let mut fb = Framebuffer::new();
    let (left, top, width, height) = (1, 1, 2, 2);
    chrome.draw_window(&mut fb, left, top, width, height);

    let border_top_left_corner = fb.pixel(0, 0);
    assert_eq!(
        border_top_left_corner,
        Some(STD_BORDER),
        "the border ring must use the standard frame's own palette"
    );
    // Deep inside the content rect, away from where the glyph below draws.
    let content_interior = fb.pixel(20, 20);
    assert_eq!(
        content_interior,
        Some(MSG_FILL),
        "the content fill must use the message-box palette, not the \
         standard frame's"
    );

    let mut pixels = [0u8; GLYPH_PIXELS];
    pixels[FOREGROUND_PIXEL] = FOREGROUND_COLOR_SLOT;
    pixels[SHADOW_PIXEL] = SHADOW_COLOR_SLOT;
    let glyph = RevealedGlyph {
        x: 0,
        y: 0,
        glyph: Glyph {
            advance_width: 8,
            pixels,
        },
    };
    chrome.draw_text(&mut fb, (left, top, width, height), (0, 0), &[glyph]);

    let glyph_foreground_pixel = fb.pixel(8, 8);
    assert_eq!(
        glyph_foreground_pixel,
        Some(MSG_FG),
        "the glyph foreground colour must come from the message-box palette"
    );
    let glyph_shadow_pixel = fb.pixel(13, 8);
    assert_eq!(
        glyph_shadow_pixel,
        Some(MSG_SHADOW),
        "the glyph shadow colour must come from the message-box palette"
    );
    let border_after_drawing_text = fb.pixel(0, 0);
    assert_eq!(
        border_after_drawing_text,
        Some(STD_BORDER),
        "drawing text must not disturb the border on the standard frame's \
         own palette"
    );
}

/// The save's `optionsWindowFrameType` borders the item window and the
/// Yes/No prompt ([`StartMenuChrome::from_pack`] owns the contract).
#[test]
fn a_saved_games_own_window_frame_choice_borders_the_start_menu() {
    use assets::pack::AssetPack;
    use rendering::{Bgr555, Framebuffer};

    const CHOSEN_FRAME: u8 = 5;

    let path = synthetic_pack_path("start-menu-window-frame");
    let _guard = TempPackFile::write(&path, synthetic_start_menu_pack_bytes());

    let pack = AssetPack::load(&path).expect("the fixture pack is well-formed");
    let chrome =
        StartMenuChrome::from_pack(&pack, CHOSEN_FRAME).expect("the fixture pack has frame 5");
    let mut menu = StartMenu::assemble(chrome, 0);

    let frame_5_red = Bgr555::from_channels(31, 0, 0).to_rgb888();

    let item_window_border_corner = menu.compose_over(Framebuffer::new()).pixel(
        usize::try_from((MENU_TILEMAP_LEFT - 1) * 8).unwrap(),
        usize::try_from((MENU_TILEMAP_TOP - 1) * 8).unwrap(),
    );
    assert_eq!(
        item_window_border_corner,
        Some(frame_5_red),
        "a save whose optionsWindowFrameType is {CHOSEN_FRAME} must draw that \
         frame's border around the start menu's item window, not frame 0's"
    );

    let mut target = FakeTarget::new(SaveFileStatus::Ok, false);
    menu.tick(pressed(Buttons::A), &mut target);
    for _ in 0..FRAME_BUDGET {
        if menu.yes_no_cursor().is_some() {
            break;
        }
        menu.tick(pressed(Buttons::A), &mut target);
    }
    assert!(
        menu.yes_no_cursor().is_some(),
        "the save flow must reach a Yes/No prompt within the frame budget"
    );

    let yes_no_window_border_corner = menu.compose_over(Framebuffer::new()).pixel(
        usize::try_from((YES_NO_TILEMAP_LEFT - 1) * 8).unwrap(),
        usize::try_from((YES_NO_TILEMAP_TOP - 1) * 8).unwrap(),
    );
    assert_eq!(
        yes_no_window_border_corner,
        Some(frame_5_red),
        "a save whose optionsWindowFrameType is {CHOSEN_FRAME} must draw that \
         frame's border around the start menu's Yes/No prompt too, not frame 0's"
    );
}

/// A unique scratch path for a synthetic pack fixture.
fn synthetic_pack_path(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "pokeemerald-rs-{label}-{}-{:?}.pack",
        std::process::id(),
        std::thread::current().id()
    ))
}

/// Writes `bytes` to `path` and removes it on drop, so a failed assertion
/// still cleans up the scratch file.
struct TempPackFile {
    path: std::path::PathBuf,
}

impl TempPackFile {
    fn write(path: &std::path::Path, bytes: Vec<u8>) -> Self {
        std::fs::write(path, bytes).expect("the scratch directory is writable");
        Self {
            path: path.to_path_buf(),
        }
    }
}

impl Drop for TempPackFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// The smallest pack [`StartMenuChrome::from_pack`] accepts, carrying two
/// distinguishable selectable standard frames -- frame 0 (green) and frame 5
/// (red) -- plus the fixed message box and the normal font sheet, so which
/// frame a menu drew is readable from a single border pixel.
fn synthetic_start_menu_pack_bytes() -> Vec<u8> {
    use crate::pack_test_support::{image_entry, palette_entry, palette_entry_with_color};
    use rendering::Bgr555;

    const FRAME_SIDE: u32 = 24;
    const FRAME_BIT_DEPTH: u8 = 4;
    const PALETTE_COLOUR_COUNT: u16 = 16;
    const FRAME_BORDER_PALETTE_INDEX: u8 = 1;
    const MESSAGE_BOX_WIDTH: u32 = 56;
    const MESSAGE_BOX_HEIGHT: u32 = 16;
    const FONT_BIT_DEPTH: u8 = 2;

    crate::pack_test_support::pack_bytes(vec![
        image_entry(
            "text-window/image/1",
            FRAME_SIDE,
            FRAME_SIDE,
            FRAME_BIT_DEPTH,
            FRAME_BORDER_PALETTE_INDEX,
        ),
        palette_entry_with_color(
            "text-window/palette/1",
            PALETTE_COLOUR_COUNT,
            FRAME_BORDER_PALETTE_INDEX,
            Bgr555::from_channels(0, 31, 0),
        ),
        image_entry(
            "text-window/image/6",
            FRAME_SIDE,
            FRAME_SIDE,
            FRAME_BIT_DEPTH,
            FRAME_BORDER_PALETTE_INDEX,
        ),
        palette_entry_with_color(
            "text-window/palette/6",
            PALETTE_COLOUR_COUNT,
            FRAME_BORDER_PALETTE_INDEX,
            Bgr555::from_channels(31, 0, 0),
        ),
        image_entry(
            "text-window/image/message_box",
            MESSAGE_BOX_WIDTH,
            MESSAGE_BOX_HEIGHT,
            FRAME_BIT_DEPTH,
            0,
        ),
        palette_entry("text-window/palette/message_box", PALETTE_COLOUR_COUNT),
        image_entry(
            "font/normal/glyphs",
            assets::fonts::SHEET_WIDTH,
            assets::fonts::SHEET_HEIGHT,
            FONT_BIT_DEPTH,
            0,
        ),
    ])
}
