//! The immutable keyboard page specification rendering reads: column
//! positions, fill colours, display spacing, and the page-to-page
//! relationships (`pokeemerald/src/naming_screen.c:278-310`, `:584-603`,
//! `:1927-1959`; `text_input_strings.c:10-21`).
//!
//! The characters and column counts stay in [`keyboard`](super::keyboard)
//! (`sKeyboardChars`, `sPageColumnCounts`); this file adds only what drawing a
//! page needs. Nothing here is mutable input state.

use super::keyboard::{KeyboardCursor, KeyboardPage, KEY_ROWS};

/// `sPageColumnXPos` for the two letter pages.
const LETTER_COLUMN_X: [u8; 8] = [0, 12, 24, 56, 68, 80, 92, 123];
/// `sPageColumnXPos` for the symbols page.
const SYMBOL_COLUMN_X: [u8; 6] = [0, 22, 44, 66, 88, 110];

/// `SetCursorPos`' x origin: `sPageColumnXPos[column] + 38`.
const CURSOR_ORIGIN_X: u8 = 38;
/// `SetCursorPos`' y origin: `row * 16 + 88`.
const CURSOR_ORIGIN_Y: u8 = 88;
/// Pixels between key rows (`i * 16 + 1` in `PrintKeyboardKeys`).
pub(super) const KEY_ROW_HEIGHT: u8 = 16;

/// The `{CLEAR n}` before each glyph of a displayed keyboard row
/// (`gText_NamingScreenKeyboard_*`), indexed by row.
type RowGaps = [&'static [u8]; KEY_ROWS as usize];

const LOWER_GAPS: RowGaps = [
    &[11, 6, 6, 26, 6, 6, 6, 30],
    &[11, 6, 7, 27, 7, 7, 7, 30],
    &[11, 6, 6, 26, 6, 7, 6, 27],
    &[11, 6, 6, 26, 6, 6, 6, 26],
];
const UPPER_GAPS: RowGaps = [
    &[11, 6, 6, 26, 6, 6, 6, 30],
    &[11, 6, 6, 26, 6, 6, 6, 30],
    &[11, 6, 6, 26, 6, 6, 6, 26],
    &[11, 6, 6, 26, 6, 6, 6, 26],
];
const SYMBOL_GAPS: RowGaps = [
    &[11, 16, 16, 16, 16, 16],
    &[11, 16, 16, 16, 16, 16],
    &[12, 17, 16, 16, 16, 17],
    &[11, 16, 16, 18, 19, 18],
];

impl KeyboardPage {
    /// The page's character columns' x offsets from the keyboard window's left
    /// edge (`sPageColumnXPos`), one per [`column_count`](Self::column_count).
    #[must_use]
    pub const fn column_x_offsets(self) -> &'static [u8] {
        match self {
            Self::Upper | Self::Lower => &LETTER_COLUMN_X,
            Self::Symbols => &SYMBOL_COLUMN_X,
        }
    }

    /// The palette index that fills this page's window and is its text
    /// background (`sFillValues`, `sTextColorStruct`): background bank 10,
    /// colour 13 for upper, 14 for lower, 15 for symbols.
    #[must_use]
    pub const fn fill_color_index(self) -> u8 {
        match self {
            Self::Upper => 13,
            Self::Lower => 14,
            Self::Symbols => 15,
        }
    }

    /// The screen position of the cursor sprite's centre over the key at
    /// `cursor` (`SetCursorPos`), or `None` for the button column, where
    /// upstream hides the cursor (`SpriteCB_Cursor`).
    #[must_use]
    pub fn cursor_center(self, cursor: KeyboardCursor) -> Option<(u8, u8)> {
        if cursor.column >= self.column_count() || cursor.row >= KEY_ROWS {
            return None;
        }
        let x = *self.column_x_offsets().get(usize::from(cursor.column))?;
        Some((
            CURSOR_ORIGIN_X + x,
            CURSOR_ORIGIN_Y + cursor.row * KEY_ROW_HEIGHT,
        ))
    }

    /// The `{CLEAR n}` width before each displayed glyph of `row`.
    pub(super) const fn clear_gaps(self, row: usize) -> &'static [u8] {
        let table = match self {
            Self::Upper => &UPPER_GAPS,
            Self::Lower => &LOWER_GAPS,
            Self::Symbols => &SYMBOL_GAPS,
        };
        table[row]
    }
}
