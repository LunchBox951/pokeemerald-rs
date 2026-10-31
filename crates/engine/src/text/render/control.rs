//! Extended control codes that move the cursor, erase, or resolve to a glyph.
//!
//! [`Printer::tick`] hands each `EXT_CTRL_CODE` token it does not handle
//! itself (`PAUSE`, `PAUSE_UNTIL_PRESS`) to [`Printer::apply_extended_control`],
//! which applies the cursor, `CLEAR`, `CLEAR_TO`, and `FILL_WINDOW` effects and
//! reports the [`ControlOutcome`] the frame continues with.

use assets::fonts::GlyphSource;

use super::{ext_ctrl, ClearedSpan, Printer, EXTRA_SYMBOL_PAGE_START};

/// What handling one extended control code leaves for the frame to do.
///
/// These are upstream `RenderText`'s own return values for the control-code
/// cases (`pokeemerald/src/text.c:960-1135`), minus the ones no control in this
/// renderer produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ControlOutcome {
    /// `RENDER_REPEAT`: the control had no glyph of its own, so token handling
    /// continues within the same frame.
    Repeat,
    /// The escape control resolved to an extra-symbol glyph to reveal.
    Glyph(u16),
    /// `RENDER_PRINT`: a span was erased and the frame ends.
    ClearedSpan,
    /// `FILL_WINDOW`'s `RENDER_REPEAT`: the whole window was cleared and the
    /// cursor reset to the origin, but -- unlike [`Self::ClearedSpan`] --
    /// token handling continues within the same frame
    /// (`pokeemerald/src/text.c:1052-1056`).
    ClearedWindow,
}

impl<S: GlyphSource> Printer<S> {
    pub(super) fn apply_extended_control(
        &mut self,
        subcode: u8,
        arguments: &[u8],
    ) -> ControlOutcome {
        match subcode {
            ext_ctrl::ESCAPE => {
                if let Some(&index) = arguments.first() {
                    return ControlOutcome::Glyph(EXTRA_SYMBOL_PAGE_START | u16::from(index));
                }
            }
            ext_ctrl::SHIFT_RIGHT | ext_ctrl::SKIP => {
                if let Some(&offset) = arguments.first() {
                    self.cursor.0 = self.origin.0.saturating_add(i32::from(offset));
                }
            }
            ext_ctrl::SHIFT_DOWN => {
                if let Some(&offset) = arguments.first() {
                    self.cursor.1 = self.origin.1.saturating_add(i32::from(offset));
                }
            }
            // `text.c:1063-1072`: the argument is the width to erase from the
            // cursor, which then advances by it.
            ext_ctrl::CLEAR => {
                let width = arguments.first().map_or(0, |&width| i32::from(width));
                return self.clear_span(width);
            }
            // `text.c:1077-1090`: the argument is an origin-relative column;
            // the width erased is the distance to it, so a column at or behind
            // the cursor erases nothing and never moves the cursor back.
            ext_ctrl::CLEAR_TO => {
                let Some(&column) = arguments.first() else {
                    return ControlOutcome::Repeat;
                };
                let target = self.origin.0.saturating_add(i32::from(column));
                return self.clear_span(target.saturating_sub(self.cursor.0));
            }
            // `text.c:1052-1056`: fills the whole window and resets both
            // cursor axes to the printer origin, then repeats within the
            // same frame.
            ext_ctrl::FILL_WINDOW => {
                self.cursor = self.origin;
                return ControlOutcome::ClearedWindow;
            }
            _ => {}
        }
        ControlOutcome::Repeat
    }

    /// Records `width` pixels erased at the cursor and advances past them.
    ///
    /// Upstream guards both clear controls with `width > 0`
    /// (`pokeemerald/src/text.c:1066`, `:1084`), so a non-positive width
    /// neither erases nor ends the frame.
    ///
    /// The recorded span stops at `i32::MAX`, where the cursor saturates, so
    /// it never reaches a column the cursor cannot represent; a positive width
    /// still ends the frame even when no column remains.
    fn clear_span(&mut self, width: i32) -> ControlOutcome {
        if width <= 0 {
            return ControlOutcome::Repeat;
        }
        let representable = width.min(i32::MAX.saturating_sub(self.cursor.0));
        if representable > 0 {
            self.cleared_span = Some(ClearedSpan {
                x: self.cursor.0,
                y: self.cursor.1,
                width: representable,
                height: self.clear_height,
            });
            self.cursor.0 = self.cursor.0.saturating_add(representable);
        }
        ControlOutcome::ClearedSpan
    }
}
