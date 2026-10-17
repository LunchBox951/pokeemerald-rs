use super::*;

const A: Buttons = Buttons::A;
const B: Buttons = Buttons::B;
const SELECT: Buttons = Buttons::SELECT;
const START: Buttons = Buttons::START;
const UP: Buttons = Buttons::UP;
const DOWN: Buttons = Buttons::DOWN;
const LEFT: Buttons = Buttons::LEFT;
const RIGHT: Buttons = Buttons::RIGHT;

const PAGES: [KeyboardPage; 3] = [
    KeyboardPage::Upper,
    KeyboardPage::Lower,
    KeyboardPage::Symbols,
];

/// A model driven by synthetic per-frame button sets.
struct Trace {
    model: NamingScreenModel,
    buttons: ButtonState,
}

impl Trace {
    fn new() -> Self {
        Self::with(NamingConfig::player("DEST"))
    }

    fn with(config: NamingConfig) -> Self {
        Self {
            model: NamingScreenModel::new(config),
            buttons: ButtonState::new(),
        }
    }

    fn at(page: KeyboardPage, column: u8, row: u8) -> Self {
        let mut trace = Self::new();
        trace.model.page = page;
        trace.model.cursor = KeyboardCursor { column, row };
        trace
    }

    fn frame(&mut self, held: Buttons) -> Option<NamingEvent> {
        self.buttons.update(held);
        self.model.step(&self.buttons)
    }

    /// Holds `held` for one frame, then releases for one frame.
    fn tap(&mut self, held: Buttons) -> Option<NamingEvent> {
        let event = self.frame(held);
        assert_eq!(self.frame(Buttons::NONE), None, "release frame is quiet");
        event
    }

    fn cursor(&self) -> (u8, u8) {
        (self.model.cursor.column, self.model.cursor.row)
    }

    fn text(&self) -> String {
        self.model.entry.iter().map_while(|slot| *slot).collect()
    }

    fn type_chars(&mut self, count: usize) {
        for _ in 0..count {
            assert_eq!(self.tap(A), Some(NamingEvent::CharacterAdded));
        }
    }
}

#[test]
fn page_tables_match_upstream_key_for_key() {
    let expected = [
        (
            KeyboardPage::Upper,
            8,
            ["ABCDEF .", "GHIJKL ,", "MNOPQRS ", "TUVWXYZ "],
        ),
        (
            KeyboardPage::Lower,
            8,
            ["abcdef .", "ghijkl ,", "mnopqrs ", "tuvwxyz "],
        ),
        (
            KeyboardPage::Symbols,
            6,
            ["01234", "56789", "!?♂♀/-", "…“”‘'"],
        ),
    ];
    for (page, columns, rows) in expected {
        assert_eq!(page.column_count(), columns, "{page:?} columns");
        for (row, text) in (0u8..).zip(rows) {
            for (column, ch) in (0u8..).zip(text.chars()) {
                assert_eq!(
                    page.character_at(column, row),
                    Some(ch),
                    "{page:?} {column},{row}"
                );
            }
            assert_eq!(
                page.character_at(columns, row),
                None,
                "{page:?} button column"
            );
        }
        assert_eq!(page.character_at(0, KEY_ROWS), None);
    }
}

#[test]
fn page_swap_cycles_upper_lower_symbols() {
    let mut page = KeyboardPage::Upper;
    let mut seen = Vec::new();
    for _ in 0..4 {
        page = page.next();
        seen.push(page);
    }
    assert_eq!(
        seen,
        [
            KeyboardPage::Lower,
            KeyboardPage::Symbols,
            KeyboardPage::Upper,
            KeyboardPage::Lower
        ]
    );
}

#[test]
fn key_role_lookup_covers_keys_and_button_column() {
    for page in PAGES {
        let button_column = page.column_count();
        for row in 0..KEY_ROWS {
            for column in 0..button_column {
                assert_eq!(
                    KeyboardCursor { column, row }.role(page),
                    KeyRole::Character
                );
            }
        }
        for (row, role) in [
            (BUTTON_PAGE, KeyRole::PageSwap),
            (BUTTON_BACK, KeyRole::Backspace),
            (BUTTON_OK, KeyRole::Ok),
        ] {
            assert_eq!(
                KeyboardCursor {
                    column: button_column,
                    row
                }
                .role(page),
                role
            );
        }
    }
}

#[test]
fn new_model_opens_on_upper_page_at_first_key() {
    let snapshot = Trace::new().model.snapshot();
    assert_eq!(snapshot.page, KeyboardPage::Upper);
    assert_eq!(snapshot.cursor, KeyboardCursor { column: 0, row: 0 });
    assert_eq!(snapshot.role, KeyRole::Character);
    assert_eq!(snapshot.entry, vec![None; 7]);
    assert_eq!(snapshot.entry_position, 0);
    assert_eq!(snapshot.pending, None);
}

#[test]
fn cursor_wraps_and_crosses_the_button_column() {
    // (page, start, press, expected)
    let cases = [
        // Vertical wrap on keys.
        (KeyboardPage::Upper, (3, 0), UP, (3, 3)),
        (KeyboardPage::Upper, (3, 3), DOWN, (3, 0)),
        // Horizontal movement inside the grid.
        (KeyboardPage::Upper, (3, 1), RIGHT, (4, 1)),
        (KeyboardPage::Upper, (3, 1), LEFT, (2, 1)),
        // Left from column 0 wraps onto the button column.
        (KeyboardPage::Upper, (0, 0), LEFT, (8, BUTTON_PAGE)),
        (KeyboardPage::Upper, (0, 1), LEFT, (8, BUTTON_BACK)),
        (KeyboardPage::Upper, (0, 2), LEFT, (8, BUTTON_BACK)),
        (KeyboardPage::Upper, (0, 3), LEFT, (8, BUTTON_OK)),
        // Right from the last column enters the button column.
        (KeyboardPage::Lower, (7, 3), RIGHT, (8, BUTTON_OK)),
        (KeyboardPage::Symbols, (5, 0), RIGHT, (6, BUTTON_PAGE)),
        (KeyboardPage::Symbols, (0, 2), LEFT, (6, BUTTON_BACK)),
        // Leaving the button column from the top and bottom buttons.
        (KeyboardPage::Upper, (8, BUTTON_PAGE), RIGHT, (0, 0)),
        (KeyboardPage::Upper, (8, BUTTON_OK), LEFT, (7, 3)),
        (KeyboardPage::Symbols, (6, BUTTON_OK), RIGHT, (0, 3)),
        // The button column wraps over three rows.
        (KeyboardPage::Upper, (8, BUTTON_PAGE), UP, (8, BUTTON_OK)),
        (
            KeyboardPage::Symbols,
            (6, BUTTON_OK),
            DOWN,
            (6, BUTTON_PAGE),
        ),
    ];
    for (page, start, press, expected) in cases {
        let mut trace = Trace::at(page, start.0, start.1);
        trace.frame(press);
        assert_eq!(trace.cursor(), expected, "{page:?} {start:?} + {press:?}");
    }
}

#[test]
fn middle_button_returns_to_the_remembered_key_row() {
    // Entering from key row 2 remembers row 2, which maps to the middle button.
    let mut trace = Trace::at(KeyboardPage::Upper, 7, 2);
    trace.tap(RIGHT);
    assert_eq!(trace.cursor(), (8, BUTTON_BACK));
    trace.tap(LEFT);
    assert_eq!(trace.cursor(), (7, 2));
    // Row 1 round-trips too.
    let mut trace = Trace::at(KeyboardPage::Upper, 0, 1);
    trace.tap(LEFT);
    trace.tap(RIGHT);
    assert_eq!(trace.cursor(), (0, 1));
}

#[test]
fn resting_on_a_button_overwrites_the_remembered_row() {
    // Upstream stores BUTTON_BACK while on the top button, BUTTON_OK on the
    // bottom one, so walking up to the middle button returns to row 1 or 2.
    let mut trace = Trace::at(KeyboardPage::Upper, 7, 2);
    trace.tap(RIGHT);
    trace.tap(UP);
    assert_eq!(trace.cursor(), (8, BUTTON_PAGE));
    trace.tap(DOWN);
    assert_eq!(trace.cursor(), (8, BUTTON_BACK));
    trace.tap(LEFT);
    assert_eq!(trace.cursor(), (7, 1), "top button stored BUTTON_BACK");

    let mut trace = Trace::at(KeyboardPage::Upper, 7, 0);
    trace.tap(RIGHT);
    trace.tap(DOWN);
    trace.tap(DOWN);
    assert_eq!(trace.cursor(), (8, BUTTON_OK));
    trace.tap(UP);
    trace.tap(LEFT);
    assert_eq!(trace.cursor(), (7, 2), "bottom button stored BUTTON_OK");
}

#[test]
fn simultaneous_directions_resolve_up_down_left_right() {
    let cases = [
        (UP | DOWN, (3, 2)),
        (UP | LEFT, (2, 1)),
        (DOWN | LEFT, (2, 1)),
        (LEFT | RIGHT, (4, 1)),
        (UP | RIGHT, (4, 1)),
        (UP | DOWN | LEFT | RIGHT, (4, 1)),
    ];
    for (held, expected) in cases {
        let mut trace = Trace::at(KeyboardPage::Upper, 3, 1);
        trace.frame(held);
        assert_eq!(trace.cursor(), expected, "{held:?}");
    }
}

#[test]
fn fresh_press_precedence_is_a_b_select_start() {
    // Cursor on the OK button so A, B, SELECT, and START are all distinct.
    let cases = [
        (
            A | B | SELECT | START,
            NamingEvent::Confirmed("DEST".into()),
        ),
        (B | SELECT | START, NamingEvent::CharacterDeleted),
        (SELECT | START, NamingEvent::PageSwapStarted),
    ];
    for (held, expected) in cases {
        let mut trace = Trace::at(KeyboardPage::Upper, 8, BUTTON_OK);
        assert_eq!(trace.frame(held | RIGHT), Some(expected), "{held:?}");
        assert_eq!(
            trace.cursor(),
            (8, BUTTON_OK),
            "fresh press suppresses the d-pad"
        );
    }
    let mut trace = Trace::at(KeyboardPage::Upper, 0, 0);
    assert_eq!(trace.frame(START | DOWN), Some(NamingEvent::CursorMoved));
    assert_eq!(trace.cursor(), (8, BUTTON_OK));
}

#[test]
fn held_buttons_act_once() {
    let mut trace = Trace::new();
    assert_eq!(trace.frame(A), Some(NamingEvent::CharacterAdded));
    for _ in 0..40 {
        assert_eq!(trace.frame(A), None);
    }
    assert_eq!(trace.text(), "A");

    // A held while the d-pad is fresh still moves the cursor.
    assert_eq!(trace.frame(A | RIGHT), Some(NamingEvent::CursorMoved));
}

#[test]
fn dpad_repeats_after_sixteen_frames_then_every_five() {
    let mut trace = Trace::new();
    let moves: Vec<u32> = (0..32).filter(|_| trace.frame(RIGHT).is_some()).collect();
    assert_eq!(moves, [0, 16, 21, 26, 31]);
    assert_eq!(trace.cursor(), (5, 0));
}

#[test]
fn changing_the_held_set_restarts_the_repeat_delay() {
    let mut trace = Trace::new();
    for _ in 0..10 {
        trace.frame(RIGHT);
    }
    // UP is fresh on frame 10 and restarts the counter for the whole set.
    assert_eq!(trace.frame(RIGHT | UP), Some(NamingEvent::CursorMoved));
    assert_eq!(
        trace.cursor(),
        (1, 3),
        "fresh UP moves; held RIGHT does not repeat"
    );
    for frame in 1..16 {
        assert_eq!(
            trace.frame(RIGHT | UP),
            None,
            "frame {frame} after the change"
        );
    }
    assert_eq!(trace.frame(RIGHT | UP), Some(NamingEvent::CursorMoved));
    assert_eq!(trace.cursor(), (2, 3), "Right wins the repeat");

    // Releasing everything restarts it as well.
    trace.frame(Buttons::NONE);
    assert_eq!(trace.frame(RIGHT), Some(NamingEvent::CursorMoved));
    assert_eq!(trace.frame(RIGHT), None);
}

#[test]
fn repeat_counter_runs_while_input_is_locked() {
    let mut trace = Trace::new();
    assert_eq!(
        trace.frame(SELECT | RIGHT),
        Some(NamingEvent::PageSwapStarted)
    );
    // Frames 1..=15 count down; the frame-16 repeat lands while the swap
    // animation has input disabled.
    for _ in 1..=16 {
        assert_eq!(trace.frame(SELECT | RIGHT), None);
    }
    assert_eq!(trace.cursor(), (0, 0));
    assert_eq!(
        trace.model.complete_transition(),
        Some(NamingEvent::PageSwapped(KeyboardPage::Lower))
    );
    for frame in 17..21 {
        assert_eq!(trace.frame(SELECT | RIGHT), None, "frame {frame}");
    }
    assert_eq!(trace.frame(SELECT | RIGHT), Some(NamingEvent::CursorMoved));
    assert_eq!(trace.cursor(), (1, 0));
}

#[test]
fn select_swaps_pages_from_every_focus() {
    for (column, row) in [(2, 1), (8, BUTTON_PAGE), (8, BUTTON_BACK), (8, BUTTON_OK)] {
        let mut trace = Trace::at(KeyboardPage::Upper, column, row);
        assert_eq!(
            trace.tap(SELECT),
            Some(NamingEvent::PageSwapStarted),
            "{column},{row}"
        );
        assert_eq!(
            trace.model.pending,
            Some(PendingTransition::PageSwapAnimating)
        );
    }
}

#[test]
fn a_on_the_page_button_swaps_pages() {
    let mut trace = Trace::at(KeyboardPage::Upper, 8, BUTTON_PAGE);
    assert_eq!(trace.tap(A), Some(NamingEvent::PageSwapStarted));
    assert_eq!(
        trace.model.complete_transition(),
        Some(NamingEvent::PageSwapped(KeyboardPage::Lower))
    );
    assert_eq!(trace.model.page, KeyboardPage::Lower);
    assert_eq!(trace.model.pending, None);
}

#[test]
fn page_swap_clamps_the_cursor_to_the_new_page() {
    // (start page, start cursor, expected cursor after one swap)
    let cases = [
        (KeyboardPage::Lower, (7, 2), (5, 2)),
        (KeyboardPage::Lower, (6, 0), (5, 0)),
        (KeyboardPage::Lower, (4, 3), (4, 3)),
        (KeyboardPage::Lower, (8, BUTTON_OK), (6, BUTTON_OK)),
        (KeyboardPage::Symbols, (5, 1), (5, 1)),
        (KeyboardPage::Symbols, (6, BUTTON_BACK), (8, BUTTON_BACK)),
    ];
    for (page, start, expected) in cases {
        let mut trace = Trace::at(page, start.0, start.1);
        trace.tap(SELECT);
        trace.model.complete_transition();
        assert_eq!(trace.cursor(), expected, "{page:?} {start:?}");
        assert_eq!(trace.model.page, page.next());
    }
}

#[test]
fn page_swap_locks_editing_until_it_completes() {
    let mut trace = Trace::at(KeyboardPage::Upper, 1, 1);
    trace.tap(SELECT);
    // Animating: nothing responds, including fresh A, B, START, and the d-pad.
    for held in [A, B, SELECT, START, RIGHT, DOWN] {
        assert_eq!(trace.tap(held), None, "{held:?}");
    }
    assert_eq!(trace.cursor(), (1, 1));
    assert_eq!(trace.text(), "");
    assert_eq!(
        trace.model.complete_transition(),
        Some(NamingEvent::PageSwapped(KeyboardPage::Lower))
    );
    // The lock lifts afterwards.
    assert_eq!(trace.tap(A), Some(NamingEvent::CharacterAdded));
    assert_eq!(trace.text(), "h");
}

#[test]
fn queued_page_swap_frame_still_moves_the_cursor() {
    let mut trace = Trace::at(KeyboardPage::Upper, 1, 1);
    assert_eq!(trace.frame(SELECT), Some(NamingEvent::PageSwapStarted));
    assert_eq!(trace.model.pending, Some(PendingTransition::PageSwapQueued));
    // Completion before the queued frame has run does nothing.
    assert_eq!(trace.model.complete_transition(), None);
    // The next frame lets the d-pad move but drops fresh button presses.
    assert_eq!(trace.frame(RIGHT), Some(NamingEvent::CursorMoved));
    assert_eq!(trace.cursor(), (2, 1));
    assert_eq!(
        trace.model.pending,
        Some(PendingTransition::PageSwapAnimating)
    );

    let mut trace = Trace::at(KeyboardPage::Upper, 1, 1);
    trace.frame(SELECT);
    assert_eq!(trace.frame(A | B), None);
    assert_eq!(trace.text(), "");

    // A fresh press on the queued frame is dropped and suppresses the d-pad.
    let mut trace = Trace::at(KeyboardPage::Upper, 1, 1);
    trace.frame(SELECT);
    assert_eq!(trace.frame(A | RIGHT), None);
    assert_eq!(trace.cursor(), (1, 1));
}

#[test]
fn a_on_a_key_enters_its_character() {
    let mut trace = Trace::at(KeyboardPage::Upper, 1, 1);
    assert_eq!(trace.tap(A), Some(NamingEvent::CharacterAdded));
    trace.model.cursor = KeyboardCursor { column: 6, row: 0 };
    trace.tap(A); // the blank key inserts a space
    trace.model.page = KeyboardPage::Symbols;
    trace.model.cursor = KeyboardCursor { column: 2, row: 2 };
    trace.tap(A);
    assert_eq!(trace.text(), "H ♂");
    assert_eq!(trace.model.snapshot().entry_position, 3);
}

#[test]
fn name_length_limit_squishes_then_jumps_to_ok() {
    let mut trace = Trace::new();
    trace.type_chars(6);
    assert_eq!(trace.model.pending, None);
    assert_eq!(trace.tap(A), Some(NamingEvent::CharacterAdded));
    assert_eq!(trace.text(), "AAAAAAA");
    assert_eq!(trace.model.pending, Some(PendingTransition::CursorSquish));
    assert_eq!(trace.model.snapshot().entry_position, 6);

    // Squish lock: no editing and no movement until it completes.
    for held in [A, B, SELECT, START, RIGHT] {
        assert_eq!(trace.tap(held), None, "{held:?}");
    }
    assert_eq!(trace.text(), "AAAAAAA");
    assert_eq!(trace.cursor(), (0, 0));

    assert_eq!(
        trace.model.complete_transition(),
        Some(NamingEvent::CursorMoved)
    );
    assert_eq!(trace.cursor(), (8, BUTTON_OK));
    assert_eq!(trace.model.pending, None);
}

#[test]
fn full_name_overwrites_the_last_character() {
    let mut trace = Trace::new();
    trace.type_chars(7);
    trace.model.complete_transition();
    trace.model.cursor = KeyboardCursor { column: 1, row: 0 };
    assert_eq!(trace.tap(A), Some(NamingEvent::CharacterAdded));
    assert_eq!(trace.text(), "AAAAAAB");
    assert_eq!(trace.model.pending, Some(PendingTransition::CursorSquish));
}

#[test]
fn b_deletes_from_every_focus() {
    for (column, row) in [(0, 0), (8, BUTTON_PAGE), (8, BUTTON_BACK), (8, BUTTON_OK)] {
        let mut trace = Trace::new();
        trace.type_chars(3);
        trace.model.cursor = KeyboardCursor { column, row };
        assert_eq!(
            trace.tap(B),
            Some(NamingEvent::CharacterDeleted),
            "{column},{row}"
        );
        assert_eq!(trace.text(), "AA");
    }
}

#[test]
fn backspace_button_deletes_on_a() {
    let mut trace = Trace::new();
    trace.type_chars(2);
    trace.model.cursor = KeyboardCursor {
        column: 8,
        row: BUTTON_BACK,
    };
    assert_eq!(trace.tap(A), Some(NamingEvent::CharacterDeleted));
    assert_eq!(trace.text(), "A");
}

#[test]
fn backspace_unwinds_a_full_name_and_tolerates_empty() {
    let mut trace = Trace::new();
    trace.type_chars(7);
    trace.model.complete_transition();
    for expected in ["AAAAAA", "AAAAA", "AAAA", "AAA", "AA", "A", "", ""] {
        assert_eq!(trace.tap(B), Some(NamingEvent::CharacterDeleted));
        assert_eq!(trace.text(), expected);
    }
    assert_eq!(trace.model.snapshot().entry_position, 0);
}

#[test]
fn start_jumps_to_ok_without_touching_the_remembered_row() {
    let mut trace = Trace::at(KeyboardPage::Symbols, 2, 1);
    assert_eq!(trace.tap(START), Some(NamingEvent::CursorMoved));
    assert_eq!(trace.cursor(), (6, BUTTON_OK));
    assert_eq!(trace.tap(START), None, "already on OK");
    // The next resting frame stored BUTTON_OK, as `HandleDpadMovement` does.
    trace.tap(UP);
    trace.tap(LEFT);
    assert_eq!(trace.cursor(), (5, BUTTON_OK));
}

#[test]
fn start_still_works_while_the_name_is_full_and_squish_done() {
    let mut trace = Trace::new();
    trace.type_chars(7);
    trace.model.complete_transition();
    trace.model.cursor = KeyboardCursor { column: 2, row: 2 };
    assert_eq!(trace.tap(START), Some(NamingEvent::CursorMoved));
    assert_eq!(trace.cursor(), (8, BUTTON_OK));
}

#[test]
fn ok_confirms_a_nonblank_name() {
    let mut trace = Trace::new();
    trace.type_chars(2);
    trace.model.cursor = KeyboardCursor {
        column: 8,
        row: BUTTON_OK,
    };
    assert_eq!(trace.tap(A), Some(NamingEvent::Confirmed("AA".into())));
    assert_eq!(trace.model.confirmed(), Some("AA"));
    // A finished model ignores further input.
    assert_eq!(trace.tap(B), None);
    assert_eq!(trace.model.confirmed(), Some("AA"));
}

#[test]
fn name_with_a_blank_among_letters_is_nonblank() {
    let mut trace = Trace::new();
    trace.model.cursor = KeyboardCursor { column: 6, row: 0 };
    trace.tap(A);
    trace.model.cursor = KeyboardCursor { column: 0, row: 0 };
    trace.tap(A);
    trace.model.cursor = KeyboardCursor {
        column: 8,
        row: BUTTON_OK,
    };
    assert_eq!(trace.tap(A), Some(NamingEvent::Confirmed(" A".into())));
}

#[test]
fn ok_with_only_blanks_or_nothing_keeps_the_seeded_destination() {
    // Empty entry.
    let mut trace = Trace::new();
    trace.model.cursor = KeyboardCursor {
        column: 8,
        row: BUTTON_OK,
    };
    assert_eq!(trace.tap(A), Some(NamingEvent::Confirmed("DEST".into())));

    // Spaces only.
    let mut trace = Trace::new();
    trace.model.cursor = KeyboardCursor { column: 6, row: 0 };
    trace.type_chars(3);
    assert_eq!(trace.text(), "   ");
    trace.model.cursor = KeyboardCursor {
        column: 8,
        row: BUTTON_OK,
    };
    assert_eq!(trace.tap(A), Some(NamingEvent::Confirmed("DEST".into())));
    assert_eq!(trace.model.confirmed(), Some("DEST"));
}

#[test]
fn copy_existing_seeds_the_entry_up_to_the_limit() {
    let mut trace = Trace::with(NamingConfig {
        max_chars: 5,
        copy_existing: true,
        destination: "ABCDEFGH".into(),
    });
    assert_eq!(trace.text(), "ABCDE");
    assert_eq!(trace.model.snapshot().entry_position, 4);
    assert_eq!(trace.tap(B), Some(NamingEvent::CharacterDeleted));
    assert_eq!(trace.text(), "ABCD");
    assert_eq!(trace.model.snapshot().entry_position, 4);
}
