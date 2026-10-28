//! Where a keypad icon lands and how far it moves the cursor.
//!
//! A `CHAR_KEYPAD_ICON` token names one of the thirteen icons upstream keeps in
//! `sKeypadIcons`, each with a fixed pixel size (`pokeemerald/src/text.c`
//! `:98-115`). Upstream's `RenderText` blits the icon at the cursor, advances
//! the cursor by the icon's width, and returns `RENDER_PRINT`, which ends the
//! frame (`text.c:1114-1118`).
//!
//! This module owns the size table, the [`KeypadIconPlacement`] a caller reads
//! to draw the icon, and [`place`], the per-token step [`super::Printer::tick`]
//! delegates to. It owns no icon bitmaps, so drawing (`DrawKeypadIcon`,
//! `text.c:1609-1623`) stays with the caller. The printer has no template
//! letter spacing, so the cursor advance is the icon width alone.

use super::PixelPos;

/// Width and height in pixels of keypad icon `id`, or `None` for an id
/// outside the thirteen icons the source defines.
///
/// Mirrors the per-icon size table behind `DrawKeypadIcon`
/// (`pokeemerald/src/text.c:98-115`, `:1609-1623`).
#[must_use]
pub const fn keypad_icon_size(id: u8) -> Option<(u8, u8)> {
    match id {
        2 | 3 => Some((16, 12)),
        4 | 5 => Some((24, 12)),
        0..=12 => Some((8, 12)),
        _ => None,
    }
}

/// A keypad icon the most recent [`super::Printer::tick`] placed.
///
/// Upstream blits the icon into the window itself; this printer owns no icon
/// bitmaps, so it reports the placement and the caller draws it. See
/// [`super::Printer::keypad_icon`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeypadIconPlacement {
    /// Window-local left edge.
    pub x: i32,
    /// Window-local top edge.
    pub y: i32,
    /// The icon id carried by the token (`CHAR_A_BUTTON` and its siblings).
    pub id: u8,
    /// Icon width in pixels; the cursor advanced by this much.
    pub width: u8,
    /// Icon height in pixels.
    pub height: u8,
}

/// Places keypad icon `id` at `cursor` and advances `cursor` past it.
///
/// Returns `None`, leaving `cursor` untouched, for an id outside the table.
pub(super) fn place(id: u8, cursor: &mut PixelPos) -> Option<KeypadIconPlacement> {
    let (width, height) = keypad_icon_size(id)?;
    let placement = KeypadIconPlacement {
        x: cursor.0,
        y: cursor.1,
        id,
        width,
        height,
    };
    cursor.0 = cursor.0.saturating_add(i32::from(width));
    Some(placement)
}

#[cfg(test)]
mod tests;
