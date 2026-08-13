//! Unit tests for the field start menu (I-6, issue #232) -- the item list,
//! the cursor, the geometry, and the whole `sSaveDialogCallback` chain,
//! driven against a fake [`SaveTarget`] rather than a real save file.
//!
//! The *integration* half -- the same menu writing a real save through an
//! `crate::flow::overworld_phase::OverworldPhase` and reloading it -- lives
//! in `crate::flow::save_continue_tests`.

use super::{
    menu_height, synthetic_start_menu, SaveMode, SaveTarget, StartMenu, StartMenuItem,
    StartMenuOutcome, ITEMS, MENU_TILEMAP_LEFT, MENU_TILEMAP_TOP, MENU_WIDTH,
};
use crate::game_save::SaveFileStatus;
use engine::text::Token;
use platform::{ButtonState, Buttons};

fn pressed(button: Buttons) -> ButtonState {
    let mut state = ButtonState::new();
    state.update(button);
    state
}

/// A save medium that records what it was asked to do instead of touching
/// a file -- upstream's `gSaveFileStatus`/`gDifferentSaveFile` globals and
/// `TrySavingData`, as a value the test owns.
#[derive(Debug)]
struct FakeTarget {
    boot_status: SaveFileStatus,
    different_save_file: bool,
    /// Whether `TrySavingData` should report `SAVE_STATUS_OK`.
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

    /// `SaveDoSaveCallback`'s dispatch (`start_menu.c:1093-1096`): the
    /// `gDifferentSaveFile = FALSE` on `:1096` is inside the
    /// `SAVE_OVERWRITE_DIFFERENT_FILE` branch and runs before `saveStatus`
    /// is looked at, so a *failed* overwrite clears the flag too. The real
    /// implementor
    /// (`crate::flow::overworld_phase::start_menu`'s `PhaseSaveTarget`)
    /// does the same; this fake would be a second, wrong opinion if it
    /// gated the clear on success.
    fn try_saving_data(&mut self, mode: SaveMode) -> bool {
        self.writes.push(mode);
        if matches!(mode, SaveMode::OverwriteDifferentFile { .. }) {
            self.different_save_file = false;
        }
        self.write_succeeds
    }
}

/// Frames one flow gets before the fixture calls it wedged -- generous
/// enough for `gText_DifferentSaveFile`'s four-page WARNING at
/// `TextSpeed::Mid` (`crate::flow::save_continue_tests` documents the
/// arithmetic).
const FRAME_BUDGET: usize = 4_000;

/// [`drive_reporting_outcome`], keeping only the prompt rows.
fn drive(menu: &mut StartMenu, target: &mut FakeTarget, answers: &[bool]) -> Vec<u8> {
    drive_reporting_outcome(menu, target, answers).0
}

/// Drive `menu` until it closes or its SAVE flow cancels back to the item
/// list, answering Yes/No prompts from `answers` (`true` = YES, defaulting
/// to YES).
///
/// Returns the cursor row each prompt opened on, and how the menu ended:
/// [`StartMenuOutcome::Closed`] for `HideStartMenu` giving field control
/// back, [`StartMenuOutcome::Open`] for `SAVE_CANCELED` putting the item
/// list back up. The two are not interchangeable, and `saving()` cannot
/// tell them apart -- `SaveCallback`'s `SAVE_SUCCESS`/`SAVE_ERROR` arm
/// closes the menu with the save flow still installed on it
/// (`start_menu.c:828-834`).
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

/// `BuildNormalStartMenu` order, restricted to this shell's two items
/// (module docs): `SAVE` before `EXIT`, and `SAVE` first so a fresh menu
/// opens on it.
#[test]
fn the_item_list_is_save_then_exit() {
    assert_eq!(ITEMS, [StartMenuItem::Save, StartMenuItem::Exit]);
    assert_eq!(StartMenuItem::Save.label(), "SAVE");
    assert_eq!(StartMenuItem::Exit.label(), "EXIT");
    assert_eq!(synthetic_start_menu().selected(), StartMenuItem::Save);
}

/// `AddStartMenuWindow`'s `(numActions * 2) + 2` height
/// (`pokeemerald/src/menu.c:493`), and the window it lands in -- the
/// geometry a two-item menu draws with.
#[test]
fn the_window_geometry_is_upstreams() {
    assert_eq!(menu_height(2), 6);
    assert_eq!(menu_height(7), 16, "upstream's own full normal menu");
    assert_eq!(
        (MENU_TILEMAP_LEFT, MENU_TILEMAP_TOP, MENU_WIDTH),
        (22, 1, 7)
    );
}

/// `Menu_MoveCursor` wraps at both ends (`src/menu.c:948-962`).
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

/// `HandleStartMenuInput`'s D-pad and A tests are three independent `if`s
/// (`start_menu.c:595`, `:601`, `:607`), not a chain: a frame that reports
/// DOWN and A together moves the cursor *and then* selects the item it
/// landed on. The A branch's own `return FALSE` (`:626`) is why a START on
/// that same frame does not also close the menu behind the selection.
#[test]
fn a_dpad_press_and_a_in_one_frame_move_the_cursor_then_select() {
    let mut target = FakeTarget::new(SaveFileStatus::Ok, false);

    // DOWN+A off a fresh menu: EXIT, chosen on the frame it was reached.
    let mut menu = synthetic_start_menu();
    assert_eq!(menu.selected(), StartMenuItem::Save);
    assert_eq!(
        menu.tick(pressed(Buttons::DOWN | Buttons::A), &mut target),
        StartMenuOutcome::Closed,
        "the A must act on the row DOWN just moved it to, not on SAVE"
    );

    // UP+A from EXIT: `Menu_MoveCursor` wraps back to SAVE and the same
    // frame's A starts the save flow.
    let mut menu = synthetic_start_menu();
    menu.tick(pressed(Buttons::DOWN), &mut target);
    assert_eq!(menu.selected(), StartMenuItem::Exit);
    assert_eq!(
        menu.tick(pressed(Buttons::UP | Buttons::A), &mut target),
        StartMenuOutcome::Open
    );
    assert_eq!(menu.selected(), StartMenuItem::Save);
    assert!(menu.saving(), "the same frame's A entered the SAVE flow");

    // A+START: the A branch returns first, so START never closes anything.
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

/// `StartMenuExitCallback` (`start_menu.c:750-757`) and
/// `HandleStartMenuInput`'s `JOY_NEW(START_BUTTON | B_BUTTON)` close
/// (`:628-633`) -- none of the three writes anything.
#[test]
fn exit_start_and_b_all_close_without_writing() {
    let mut target = FakeTarget::new(SaveFileStatus::Ok, false);

    let mut menu = synthetic_start_menu();
    menu.tick(pressed(Buttons::DOWN), &mut target);
    assert_eq!(menu.selected(), StartMenuItem::Exit);
    assert_eq!(
        menu.tick(pressed(Buttons::A), &mut target),
        StartMenuOutcome::Closed
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

/// `SaveConfirmInputCallback`'s shortcut (`start_menu.c:1008-1019`): a
/// new-game session over an empty cartridge is asked only whether to save
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
        "gText_ConfirmSave only, defaulting to YES"
    );
    assert_eq!(
        target.writes,
        vec![SaveMode::OverwriteDifferentFile { prompted: false }]
    );
}

/// The same shortcut applies to `SAVE_STATUS_CORRUPT`
/// (`start_menu.c:1011-1012`) -- a wrecked file is not an adventure worth
/// asking about.
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

/// A *continued* session saving over its own file gets Emerald's famous
/// second question -- `gText_AlreadySavedFile`, defaulting to YES
/// (`SaveConfirmOverwriteCallback`, `start_menu.c:1056-1061`) -- and writes
/// `SAVE_NORMAL`.
#[test]
fn a_continued_session_is_asked_twice_and_saves_normally() {
    let mut target = FakeTarget::new(SaveFileStatus::Ok, false);
    let mut menu = synthetic_start_menu();
    let prompts = drive(&mut menu, &mut target, &[]);

    assert_eq!(prompts, vec![0, 0], "both prompts default to YES");
    assert_eq!(target.writes, vec![SaveMode::Normal]);
}

/// A new-game session over someone else's save gets the WARNING, whose
/// Yes/No opens on **NO** (`DisplayYesNoMenuWithDefault(1)`,
/// `start_menu.c:1049-1053`). Answering NO cancels back to the item list
/// and writes nothing.
#[test]
fn the_different_save_file_warning_defaults_to_no_and_can_be_declined() {
    let mut target = FakeTarget::new(SaveFileStatus::Ok, true);
    let mut menu = synthetic_start_menu();
    let prompts = drive(&mut menu, &mut target, &[true, false]);

    assert_eq!(
        prompts,
        vec![0, 1],
        "gText_ConfirmSave defaults to YES; the WARNING to NO"
    );
    assert!(
        target.writes.is_empty(),
        "a declined WARNING writes nothing"
    );
    assert!(
        !menu.saving(),
        "SAVE_CANCELED puts the item list back (SaveCallback, :822-827)"
    );
}

/// Answering YES to the WARNING writes -- with `prompted` set, which is
/// what distinguishes a consented overwrite from the empty-cartridge
/// shortcut.
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

/// `SaveConfirmInputCallback`'s NO/`MENU_B_PRESSED` arm
/// (`start_menu.c:1026-1030`): the flow reports `SAVE_CANCELED` before ever
/// reaching the medium.
#[test]
fn declining_the_first_question_never_reaches_the_save_medium() {
    let mut target = FakeTarget::new(SaveFileStatus::Ok, false);
    let mut menu = synthetic_start_menu();
    drive(&mut menu, &mut target, &[false]);
    assert!(target.writes.is_empty());
    assert!(!menu.saving());
}

/// B on a Yes/No prompt is `MENU_B_PRESSED`, which every caller here reads
/// as NO (`Menu_ProcessInputNoWrap`, `src/menu.c:1023-1026`) -- even with
/// the cursor sitting on YES.
#[test]
fn b_on_a_prompt_answers_no_even_with_the_cursor_on_yes() {
    let mut target = FakeTarget::new(SaveFileStatus::Ok, false);
    let mut menu = synthetic_start_menu();

    // Into the SAVE flow, then forward until the first prompt is waiting.
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

/// A failed write shows `gText_SaveError` and still closes the start menu
/// (`SaveCallback`'s shared `SAVE_SUCCESS`/`SAVE_ERROR` arm,
/// `start_menu.c:828-834`) -- the player is returned to the field knowing
/// the save did not happen, not trapped in a menu.
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
        "SAVE_ERROR shares SAVE_SUCCESS's arm: the menu ends, it does not \
         drop the player back on the item list"
    );
}

/// `SaveDoSaveCallback` clears `gDifferentSaveFile` on the statement right
/// after `TrySavingData(SAVE_OVERWRITE_DIFFERENT_FILE)`, inside the branch
/// and before `saveStatus` is ever read (`start_menu.c:1093-1096`) -- so a
/// *failed* overwrite clears it too, and the next SAVE in the same session
/// meets the ordinary `gText_AlreadySavedFile` question (default YES)
/// rather than `gText_DifferentSaveFile`'s WARNING (default NO) all over
/// again. Clearing only on success would re-prompt where upstream does not.
#[test]
fn a_failed_overwrite_still_retires_the_different_save_file_warning() {
    let mut target = FakeTarget::new(SaveFileStatus::Ok, true);
    target.write_succeeds = false;

    // First SAVE: the WARNING, consented to, and the write fails.
    let mut menu = synthetic_start_menu();
    let prompts = drive(&mut menu, &mut target, &[true, true]);
    assert_eq!(
        prompts,
        vec![0, 1],
        "gText_ConfirmSave defaults to YES; the WARNING to NO"
    );
    assert_eq!(
        target.writes,
        vec![SaveMode::OverwriteDifferentFile { prompted: true }]
    );
    assert!(
        !target.different_save_file,
        "gDifferentSaveFile is cleared by the dispatch, not by the status"
    );

    // Second SAVE, same session, this time the medium cooperates.
    target.write_succeeds = true;
    let mut menu = synthetic_start_menu();
    let prompts = drive(&mut menu, &mut target, &[]);
    assert_eq!(
        prompts,
        vec![0, 0],
        "the retry asks gText_AlreadySavedFile, which defaults to YES -- a \
         second WARNING would open its Yes/No on NO"
    );
    assert_eq!(
        target.writes,
        vec![
            SaveMode::OverwriteDifferentFile { prompted: true },
            SaveMode::Normal
        ],
        "the retry dispatches SAVE_NORMAL, exactly as upstream's own \
         gDifferentSaveFile == FALSE branch does (start_menu.c:1098-1101)"
    );
}

/// The composed frame really draws something over the map: a start menu
/// that rendered nothing would pass every state-machine test above while
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

    // Inside the item window's content rect (tile 22,1, 7x6 tiles): the
    // fill replaced the caller's backdrop.
    let inside = composed.pixel(
        usize::try_from(MENU_TILEMAP_LEFT * 8 + 4).unwrap(),
        usize::try_from(MENU_TILEMAP_TOP * 8 + 4).unwrap(),
    );
    assert_ne!(inside, Some(marker), "the menu window must be painted");
    // Far from the window -- the overworld behind it still shows.
    assert_eq!(composed.pixel(4, 150), Some(marker));
}

/// Pins the palette split every window here draws with (correctness issue
/// caught in review): the content fill and `FONT_NORMAL` glyph colours must
/// come from the message-box palette (bank 15,
/// `LoadMessageBoxAndBorderGfx`, `pokeemerald/src/text_window.c:93-112`),
/// while only the border ring uses the selected standard frame's own
/// palette (bank 14). The standard frame's palette is seeded with sentinel
/// colours a correct implementation never reads for content or glyphs, so a
/// regression back to reading `std_frame.palette` for either would show up
/// as a wrong colour here instead of silently matching by coincidence.
#[test]
fn standard_window_uses_message_palette_for_content_and_standard_palette_for_border() {
    use super::StartMenuChrome;
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

    let mut chrome = StartMenuChrome::synthetic();
    chrome.std_frame.palette[1] = STD_SENTINEL;
    chrome.std_frame.palette[2] = STD_SENTINEL;
    chrome.std_frame.palette[3] = STD_SENTINEL;
    chrome.std_frame.palette[9] = STD_BORDER;
    // Every tile of the border sheet reads palette index 9, so any drawn
    // border pixel must be [`STD_BORDER`].
    chrome.std_frame.pixels.fill(9);
    chrome.message_frame.palette[1] = MSG_FILL;
    chrome.message_frame.palette[2] = MSG_FG;
    chrome.message_frame.palette[3] = MSG_SHADOW;

    let mut fb = Framebuffer::new();
    let (left, top, width, height) = (1, 1, 2, 2);
    chrome.draw_window(&mut fb, left, top, width, height);

    // The border's top-left corner tile, at tilemap (0, 0) -- pixel (0, 0).
    assert_eq!(
        fb.pixel(0, 0),
        Some(STD_BORDER),
        "the border ring must use the standard frame's own palette"
    );
    // Deep inside the content rect (tiles 1..3, i.e. pixels 8..24), away
    // from where the glyph below will draw.
    assert_eq!(
        fb.pixel(20, 20),
        Some(MSG_FILL),
        "the content fill must use the message-box palette, not the \
         standard frame's"
    );

    let mut pixels = [0u8; GLYPH_PIXELS];
    pixels[0] = 1; // fg, glyph-local (0, 0)
    pixels[5] = 2; // shadow, glyph-local (5, 0)
    let glyph = RevealedGlyph {
        x: 0,
        y: 0,
        glyph: Glyph {
            advance_width: 8,
            pixels,
        },
    };
    chrome.draw_text(&mut fb, (left, top, width, height), (0, 0), &[glyph]);

    assert_eq!(
        fb.pixel(8, 8),
        Some(MSG_FG),
        "FONT_NORMAL's foreground colour must come from the message-box \
         palette"
    );
    assert_eq!(
        fb.pixel(13, 8),
        Some(MSG_SHADOW),
        "FONT_NORMAL's shadow colour must come from the message-box palette"
    );
    // Drawing text must not disturb the border already on the standard
    // frame's own palette.
    assert_eq!(fb.pixel(0, 0), Some(STD_BORDER));
}
