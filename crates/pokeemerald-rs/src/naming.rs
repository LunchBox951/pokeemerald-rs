//! Rendering-independent naming-screen model (I-3).
//!
//! Reproduces the keyboard, cursor, and name-editing behaviour of
//! `pokeemerald/src/naming_screen.c`: `HandleKeyboardEvent` and its key
//! handlers (`:1451-1527`), `Input_Enabled` and `HandleDpadMovement`
//! (`:1530-1698`), and the text routines (`:1784-1863`). The d-pad uses the
//! screen-local 16-frame initial repeat delay set in `NamingScreen_Init`
//! (`:465-484`) and the 5-frame continued delay of `ReadKeys`
//! (`pokeemerald/src/main.c:250-290`).
//!
//! [`NamingScene`] composes the pack-backed keyboard pages, controls, and
//! entry from a [`PageSelection`] and [`NamingVisuals`] without holding any
//! input state. Page-swap and cursor-squish animations belong to presentation.
//! The model enters a [`PendingTransition`], locks input, and waits for the caller to
//! call [`NamingScreenModel::complete_transition`]. Presentation, sound,
//! preset names, and save writes live outside this module. The `L=A` button
//! mode that `ReadKeys` remaps is not modelled.

mod compositor;
mod keyboard;
#[cfg(test)]
mod pack_fixture;
mod page_spec;
#[cfg(test)]
mod tests;

pub use compositor::{
    CursorVariant, FeedbackPose, KeyboardLayer, NamingAvatar, NamingScene, NamingSceneError,
    NamingVisuals, PageSelection, PLAYER_TITLE,
};
pub use keyboard::{
    KeyRole, KeyboardCursor, KeyboardPage, BUTTON_BACK, BUTTON_OK, BUTTON_PAGE, BUTTON_ROWS,
    KEY_ROWS,
};
use platform::input::{ButtonState, Buttons};

/// `PLAYER_NAME_LENGTH`.
const PLAYER_NAME_LENGTH: usize = 7;
/// `gKeyRepeatStartDelay` while the naming screen runs.
const REPEAT_START_DELAY: u8 = 16;
/// `gKeyRepeatContinueDelay`.
const REPEAT_CONTINUE_DELAY: u8 = 5;
const BUTTON_COLUMN_MIDDLE_ROW: u8 = BUTTON_BACK;
/// `sKeyRowToButtonRow`.
const KEY_ROW_TO_BUTTON_ROW: [u8; KEY_ROWS as usize] =
    [BUTTON_PAGE, BUTTON_BACK, BUTTON_BACK, BUTTON_OK];
/// `sButtonRowToKeyRow`; the middle entry is replaced by the remembered row.
const BUTTON_ROW_TO_KEY_ROW: [u8; BUTTON_ROWS as usize] = [0, 0, KEY_ROWS - 1];

/// The template fields that shape a naming session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamingConfig {
    /// `maxChars`.
    pub max_chars: usize,
    /// `copyExistingString`: start with the destination's text in the entry.
    pub copy_existing: bool,
    /// The caller's seeded destination (`destBuffer`).
    pub destination: String,
}

impl NamingConfig {
    /// `sPlayerNamingScreenTemplate`.
    #[must_use]
    pub fn player(destination: &str) -> Self {
        Self {
            max_chars: PLAYER_NAME_LENGTH,
            copy_existing: false,
            destination: destination.to_owned(),
        }
    }
}

/// A transition the model waits on before editing resumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingTransition {
    /// `STATE_START_PAGE_SWAP`: one more frame before the animation starts;
    /// the d-pad still moves the cursor on that frame.
    PageSwapQueued,
    /// `STATE_WAIT_PAGE_SWAP`: input is disabled until the animation ends.
    PageSwapAnimating,
    /// `STATE_MOVE_TO_OK_BUTTON`: the full-name cursor squish is playing.
    CursorSquish,
}

/// What a frame changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NamingEvent {
    /// The cursor moved.
    CursorMoved,
    /// A character was written (`AddTextCharacter`).
    CharacterAdded,
    /// The last character was removed (`DeleteTextCharacter`).
    CharacterDeleted,
    /// A page swap began; see [`PendingTransition`].
    PageSwapStarted,
    /// A page swap finished and the keyboard now shows this page.
    PageSwapped(KeyboardPage),
    /// OK was pressed. Carries the destination's value after `SaveInputText`.
    Confirmed(String),
}

/// What a scene draws from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamingSnapshot {
    /// The visible keyboard page.
    pub page: KeyboardPage,
    /// The cursor cell.
    pub cursor: KeyboardCursor,
    /// The key role under the cursor.
    pub role: KeyRole,
    /// One slot per entry position; `None` is `EOS`.
    pub entry: Vec<Option<char>>,
    /// `GetTextEntryPosition`: where the next character lands.
    pub entry_position: usize,
    /// The transition input is waiting on.
    pub pending: Option<PendingTransition>,
}

/// A fresh press, in `Input_Enabled` precedence order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyEvent {
    A,
    B,
    Select,
    Start,
}

/// `ReadKeys`' repeat counter, kept per screen because the platform input
/// carries only held and fresh buttons.
#[derive(Debug)]
struct KeyRepeat {
    counter: u8,
    previous_held: Buttons,
}

impl KeyRepeat {
    /// `gMain.newAndRepeatedKeys` for this frame.
    fn advance(&mut self, buttons: ButtonState) -> Buttons {
        let held = buttons.held();
        let mut repeated = buttons.newly_pressed();
        if held != Buttons::NONE && held == self.previous_held {
            self.counter -= 1;
            if self.counter == 0 {
                repeated = held;
                self.counter = REPEAT_CONTINUE_DELAY;
            }
        } else {
            self.counter = REPEAT_START_DELAY;
        }
        self.previous_held = held;
        repeated
    }
}

/// One step forward or backward around a cycle of `len` cells.
fn step_wrapping(value: u8, len: u8, forward: bool) -> u8 {
    if forward {
        (value + 1) % len
    } else {
        (value + len - 1) % len
    }
}

#[derive(Debug, Clone, Copy)]
enum Direction {
    Up,
    Down,
    Left,
    Right,
}

impl Direction {
    /// Sequential Up, Down, Left, Right tests; the last match wins.
    fn from_repeated(repeated: Buttons) -> Option<Self> {
        [
            (Buttons::UP, Self::Up),
            (Buttons::DOWN, Self::Down),
            (Buttons::LEFT, Self::Left),
            (Buttons::RIGHT, Self::Right),
        ]
        .into_iter()
        .rev()
        .find_map(|(button, direction)| repeated.intersects(button).then_some(direction))
    }

    /// Whether the direction is horizontal, and whether it moves forward.
    const fn axis(self) -> (bool, bool) {
        match self {
            Self::Up => (false, false),
            Self::Down => (false, true),
            Self::Left => (true, false),
            Self::Right => (true, true),
        }
    }
}

/// The naming screen's keyboard, cursor, and name buffer.
#[derive(Debug)]
pub struct NamingScreenModel {
    page: KeyboardPage,
    cursor: KeyboardCursor,
    /// The input task's `tButtonId`: the key row to return to when leaving
    /// the button column from its middle row.
    remembered_key_row: u8,
    entry: Vec<Option<char>>,
    destination: String,
    pending: Option<PendingTransition>,
    confirmed: Option<String>,
    repeat: KeyRepeat,
}

impl NamingScreenModel {
    /// Starts on the upper page with the cursor on the first key.
    #[must_use]
    pub fn new(config: NamingConfig) -> Self {
        let mut entry = vec![None; config.max_chars];
        if config.copy_existing {
            for (slot, ch) in entry.iter_mut().zip(config.destination.chars()) {
                *slot = Some(ch);
            }
        }
        Self {
            page: KeyboardPage::Upper,
            cursor: KeyboardCursor { column: 0, row: 0 },
            remembered_key_row: 0,
            entry,
            destination: config.destination,
            pending: None,
            confirmed: None,
            repeat: KeyRepeat {
                counter: REPEAT_START_DELAY,
                previous_held: Buttons::NONE,
            },
        }
    }

    /// Runs one frame of `buttons`. The repeat counter advances every frame,
    /// including locked ones, like `ReadKeys`.
    pub fn step(&mut self, buttons: &ButtonState) -> Option<NamingEvent> {
        let repeated = self.repeat.advance(*buttons);
        if self.confirmed.is_some() {
            return None;
        }
        match self.pending {
            Some(PendingTransition::PageSwapAnimating | PendingTransition::CursorSquish) => None,
            Some(PendingTransition::PageSwapQueued) => {
                // Fresh presses are read but dropped; they still suppress the d-pad.
                let moved = Self::fresh_key_event(*buttons).is_none() && self.read_dpad(repeated);
                self.pending = Some(PendingTransition::PageSwapAnimating);
                moved.then_some(NamingEvent::CursorMoved)
            }
            None => match Self::fresh_key_event(*buttons) {
                Some(key) => self.handle_key_event(key),
                None => self.read_dpad(repeated).then_some(NamingEvent::CursorMoved),
            },
        }
    }

    /// Reports that the animation behind the pending transition has ended.
    /// Does nothing while no animation is running.
    pub fn complete_transition(&mut self) -> Option<NamingEvent> {
        match self.pending? {
            PendingTransition::PageSwapQueued => None,
            PendingTransition::PageSwapAnimating => {
                self.pending = None;
                self.swap_page();
                Some(NamingEvent::PageSwapped(self.page))
            }
            PendingTransition::CursorSquish => {
                self.pending = None;
                self.move_cursor_to_ok().then_some(NamingEvent::CursorMoved)
            }
        }
    }

    /// The state a scene draws.
    #[must_use]
    pub fn snapshot(&self) -> NamingSnapshot {
        NamingSnapshot {
            page: self.page,
            cursor: self.cursor,
            role: self.cursor.role(self.page),
            entry: self.entry.clone(),
            entry_position: self.text_entry_position(),
            pending: self.pending,
        }
    }

    /// The destination's value once OK was pressed, else `None`.
    #[must_use]
    pub fn confirmed(&self) -> Option<&str> {
        self.confirmed.as_deref()
    }

    fn fresh_key_event(buttons: ButtonState) -> Option<KeyEvent> {
        [
            (Buttons::A, KeyEvent::A),
            (Buttons::B, KeyEvent::B),
            (Buttons::SELECT, KeyEvent::Select),
            (Buttons::START, KeyEvent::Start),
        ]
        .into_iter()
        .find_map(|(button, key)| buttons.is_newly_pressed(button).then_some(key))
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Option<NamingEvent> {
        match key {
            KeyEvent::Select => Some(self.start_page_swap()),
            KeyEvent::B => Some(self.delete_character()),
            KeyEvent::Start => self.move_cursor_to_ok().then_some(NamingEvent::CursorMoved),
            KeyEvent::A => match self.cursor.role(self.page) {
                KeyRole::Character => Some(self.add_character()),
                KeyRole::PageSwap => Some(self.start_page_swap()),
                KeyRole::Backspace => Some(self.delete_character()),
                KeyRole::Ok => Some(self.confirm()),
            },
        }
    }

    fn start_page_swap(&mut self) -> NamingEvent {
        self.pending = Some(PendingTransition::PageSwapQueued);
        NamingEvent::PageSwapStarted
    }

    /// `MainState_WaitPageSwap`: the button column stays selected; any other
    /// column clamps to the new page's last character column.
    fn swap_page(&mut self) {
        let on_button_column = self.cursor.column == self.page.column_count();
        self.page = self.page.next();
        let columns = self.page.column_count();
        self.cursor.column = if on_button_column {
            columns
        } else {
            self.cursor.column.min(columns - 1)
        };
    }

    /// `MoveCursorToOKButton`. Leaves `remembered_key_row` untouched.
    fn move_cursor_to_ok(&mut self) -> bool {
        let target = KeyboardCursor {
            column: self.page.column_count(),
            row: BUTTON_OK,
        };
        let moved = self.cursor != target;
        self.cursor = target;
        moved
    }

    /// `HandleDpadMovement`. Runs every enabled frame without a fresh press,
    /// including frames with no direction, because it also refreshes
    /// `remembered_key_row` while the cursor rests on the button column.
    fn read_dpad(&mut self, repeated: Buttons) -> bool {
        let previous = self.cursor;
        let button_column = self.page.column_count();
        let mut cursor = previous;

        if let Some(direction) = Direction::from_repeated(repeated) {
            let (horizontal, forward) = direction.axis();
            if horizontal {
                // The button column is one extra column in the wrap.
                cursor.column = step_wrapping(cursor.column, button_column + 1, forward);
                if cursor.column == button_column {
                    self.remembered_key_row = cursor.row;
                    cursor.row = KEY_ROW_TO_BUTTON_ROW[usize::from(cursor.row)];
                } else if previous.column == button_column {
                    cursor.row = if cursor.row == BUTTON_COLUMN_MIDDLE_ROW {
                        self.remembered_key_row
                    } else {
                        BUTTON_ROW_TO_KEY_ROW[usize::from(cursor.row)]
                    };
                }
            } else {
                let rows = if cursor.column == button_column {
                    BUTTON_ROWS
                } else {
                    KEY_ROWS
                };
                cursor.row = step_wrapping(cursor.row, rows, forward);
            }
        }

        if cursor.column == button_column {
            // Upstream records BUTTON_BACK for the top button and BUTTON_OK
            // for the bottom one.
            match cursor.row {
                BUTTON_PAGE => self.remembered_key_row = BUTTON_BACK,
                BUTTON_OK => self.remembered_key_row = BUTTON_OK,
                _ => {}
            }
        }

        self.cursor = cursor;
        cursor != previous
    }

    /// `GetTextEntryPosition`.
    fn text_entry_position(&self) -> usize {
        self.entry
            .iter()
            .position(Option::is_none)
            .unwrap_or(self.entry.len().saturating_sub(1))
    }

    /// `GetPreviousTextCaretPosition`.
    fn previous_caret_position(&self) -> usize {
        (1..self.entry.len())
            .rev()
            .find(|&index| self.entry[index].is_some())
            .unwrap_or(0)
    }

    /// `DeleteTextCharacter`. Deleting from an empty entry still reports.
    fn delete_character(&mut self) -> NamingEvent {
        let index = self.previous_caret_position();
        if let Some(slot) = self.entry.get_mut(index) {
            *slot = None;
        }
        NamingEvent::CharacterDeleted
    }

    /// `KeyboardKeyHandler_Character`. A full entry overwrites its last
    /// character, then waits for the cursor squish before jumping to OK.
    fn add_character(&mut self) -> NamingEvent {
        let character = self.page.character_at(self.cursor.column, self.cursor.row);
        let index = self.text_entry_position();
        if let Some(slot) = self.entry.get_mut(index) {
            *slot = character;
        }
        if self.previous_caret_position() == self.entry.len().saturating_sub(1) {
            self.pending = Some(PendingTransition::CursorSquish);
        }
        NamingEvent::CharacterAdded
    }

    /// `SaveInputText`: the entry replaces the destination unless every
    /// entered character is blank.
    fn confirm(&mut self) -> NamingEvent {
        if self.entry.iter().flatten().any(|&ch| ch != ' ') {
            self.destination = self.entry.iter().map_while(|slot| *slot).collect();
        }
        self.confirmed = Some(self.destination.clone());
        NamingEvent::Confirmed(self.destination.clone())
    }
}
