//! Item selection, input precedence, and SAVE outcomes against a fake save medium.

use super::test_support::{pressed, FakeTarget, FRAME_BUDGET};
use super::{
    menu_height, synthetic_start_menu, SaveMode, StartMenu, StartMenuItem, StartMenuOutcome, ITEMS,
    MENU_TILEMAP_LEFT, MENU_TILEMAP_TOP, MENU_WIDTH,
};
use crate::game_save::SaveFileStatus;
use platform::{ButtonState, Buttons};

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
