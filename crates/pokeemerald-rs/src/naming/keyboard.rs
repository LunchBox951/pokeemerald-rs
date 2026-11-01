//! Keyboard pages, key roles, and cursor geometry
//! (`pokeemerald/src/naming_screen.c:278-310`).

/// Rows of character keys on every page.
pub const KEY_ROWS: u8 = 4;
/// Rows in the button column (page swap, backspace, OK).
pub const BUTTON_ROWS: u8 = 3;
/// Button-column row of the page-swap button.
pub const BUTTON_PAGE: u8 = 0;
/// Button-column row of the backspace button.
pub const BUTTON_BACK: u8 = 1;
/// Button-column row of the OK button.
pub const BUTTON_OK: u8 = 2;

const LETTER_COLUMNS: u8 = 8;
const SYMBOL_COLUMNS: u8 = 6;

/// A keyboard page. Swapping cycles upper, lower, symbols, upper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardPage {
    /// `KBPAGE_LETTERS_UPPER`, the page every naming screen opens on.
    Upper,
    /// `KBPAGE_LETTERS_LOWER`.
    Lower,
    /// `KBPAGE_SYMBOLS`.
    Symbols,
}

impl KeyboardPage {
    /// The page a swap leads to (`MainState_WaitPageSwap`).
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Upper => Self::Lower,
            Self::Lower => Self::Symbols,
            Self::Symbols => Self::Upper,
        }
    }

    /// Character columns on this page. The button column sits at this index.
    #[must_use]
    pub const fn column_count(self) -> u8 {
        match self {
            Self::Upper | Self::Lower => LETTER_COLUMNS,
            Self::Symbols => SYMBOL_COLUMNS,
        }
    }

    /// The character a key inserts (`sKeyboardChars`), or `None` outside the
    /// page's character grid.
    #[must_use]
    pub fn character_at(self, column: u8, row: u8) -> Option<char> {
        if column >= self.column_count() || row >= KEY_ROWS {
            return None;
        }
        self.rows()[usize::from(row)]
            .chars()
            .nth(usize::from(column))
    }

    pub(super) const fn rows(self) -> [&'static str; KEY_ROWS as usize] {
        match self {
            Self::Lower => ["abcdef .", "ghijkl ,", "mnopqrs ", "tuvwxyz "],
            Self::Upper => ["ABCDEF .", "GHIJKL ,", "MNOPQRS ", "TUVWXYZ "],
            Self::Symbols => ["01234   ", "56789   ", "!?♂♀/-  ", "…“”‘'   "],
        }
    }
}

/// What activating the key under the cursor does (`sKeyboardKeyHandlers`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyRole {
    /// Inserts the key's character.
    Character,
    /// Swaps the keyboard page.
    PageSwap,
    /// Deletes the last character.
    Backspace,
    /// Confirms the name.
    Ok,
}

/// A cursor cell. `column == page.column_count()` selects the button column,
/// where `row` indexes [`BUTTON_PAGE`], [`BUTTON_BACK`], and [`BUTTON_OK`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyboardCursor {
    /// Column index; the button column is one past the last character column.
    pub column: u8,
    /// Row index within the key grid or the button column.
    pub row: u8,
}

impl KeyboardCursor {
    /// The cursor's key role on `page` (`GetKeyRoleAtCursorPos`).
    #[must_use]
    pub fn role(self, page: KeyboardPage) -> KeyRole {
        if self.column < page.column_count() {
            return KeyRole::Character;
        }
        match self.row {
            BUTTON_PAGE => KeyRole::PageSwap,
            BUTTON_BACK => KeyRole::Backspace,
            _ => KeyRole::Ok,
        }
    }
}
