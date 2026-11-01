//! Converts decoded text tokens into frame-driven glyph and control events.
//!
//! [`Printer`] owns its glyph source and render state. The caller supplies
//! [`PrinterInput`] and calls [`Printer::tick`] once per frame.
//!
//! A template line spacing is set with [`Printer::with_line_spacing`] and is
//! zero by default.
//!
//! # Deliberately out of scope
//!
//! Formatting, audio, and window-buffer controls are consumed without applying
//! effects. Placeholders and dynamic tokens are skipped because this renderer
//! has no expansion context. A keypad icon is not skipped: it advances the
//! cursor by the icon's width and is reported through
//! [`Printer::keypad_icon`] for the caller to draw, since this crate owns
//! no icon bitmaps. `CLEAR`,
//! `CLEAR_TO`, and `FILL_WINDOW` are the exceptions: they move the cursor, so
//! `CLEAR`/`CLEAR_TO`'s geometry is reported as a [`ClearedSpan`] and
//! `FILL_WINDOW`'s whole-window equivalent through
//! [`Printer::cleared_window`], both for the caller to erase. Which colour
//! that erase uses stays out of scope with the rest of formatting.

use assets::fonts::{FontId, Glyph, GlyphSource, GlyphSources};

use super::Token;

mod ext_ctrl {
    pub const ESCAPE: u8 = super::super::EXT_CTRL_CODE_ESCAPE;
    pub const SHIFT_RIGHT: u8 = super::super::EXT_CTRL_CODE_SHIFT_RIGHT;
    pub const SHIFT_DOWN: u8 = super::super::EXT_CTRL_CODE_SHIFT_DOWN;
    pub const SKIP: u8 = super::super::EXT_CTRL_CODE_SKIP;
    pub const CLEAR: u8 = super::super::EXT_CTRL_CODE_CLEAR;
    pub const CLEAR_TO: u8 = super::super::EXT_CTRL_CODE_CLEAR_TO;
    pub const FILL_WINDOW: u8 = super::super::EXT_CTRL_CODE_FILL_WINDOW;
    pub const PAUSE: u8 = super::super::EXT_CTRL_CODE_PAUSE;
    pub const PAUSE_UNTIL_PRESS: u8 = super::super::EXT_CTRL_CODE_PAUSE_UNTIL_PRESS;
}

mod control;
mod keypad_icon;
mod speed;

pub use keypad_icon::{keypad_icon_size, KeypadIconPlacement};
pub use speed::TextSpeed;

use control::ControlOutcome;

const EXTRA_SYMBOL_PAGE_START: u16 = 0x100;

/// Returns the vertical cursor advance for a newline in `font`.
#[must_use]
pub const fn max_letter_height(font: FontId) -> i32 {
    match font {
        FontId::Small => 12,
        FontId::Normal | FontId::Narrow => 16,
        FontId::Short => 14,
        FontId::SmallNarrow => 8,
    }
}

/// Returns the number of rows a `CLEAR` or `CLEAR_TO` control erases in
/// `font`.
///
/// `ClearTextSpan` fills `gCurGlyph.height` rows (`pokeemerald/src/text.c`
/// `:649-674`), which is the constant each `DecompressGlyph_*` stores for its
/// Latin glyphs -- Small 13, Narrow 15, `SmallNarrow` 12, Short 14, Normal 15
/// (`text.c:1713`, `:1755`, `:1797`, `:1841`, `:1883`). It is a different
/// metric from [`max_letter_height`], the `\n` advance, so the two tables
/// disagree on every font but Short.
const fn clear_span_height(font: FontId) -> i32 {
    match font {
        FontId::Small => 13,
        FontId::Normal | FontId::Narrow => 15,
        FontId::Short => 14,
        FontId::SmallNarrow => 12,
    }
}

/// The pixels one `CLEAR` or `CLEAR_TO` control erased.
///
/// Upstream's `ClearTextSpan` paints this rectangle into the window's tile
/// buffer (`pokeemerald/src/text.c:649-674`). [`Printer`] owns no such buffer,
/// so it reports the geometry instead and the caller erases; see
/// [`Printer::cleared_span`]. This is a run within one text line, distinct
/// from a `FILL_WINDOW` clear ([`Printer::cleared_window`]) and from the
/// whole-page [`TickEvent::Cleared`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClearedSpan {
    /// Window-local left edge.
    pub x: i32,
    /// Window-local top edge.
    pub y: i32,
    /// Width in pixels; always positive.
    pub width: i32,
    /// Height in pixels: the font's Latin glyph height.
    pub height: i32,
}

/// A window-local pixel position.
///
/// Origins are unrestricted, so every cursor calculation saturates at the
/// `i32` bounds instead of wrapping or panicking.
pub type PixelPos = (i32, i32);

/// A and B button state for one printer frame.
#[expect(
    clippy::struct_excessive_bools,
    reason = "A and B each expose independent pressed and held states"
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PrinterInput {
    /// Whether A was newly pressed.
    pub a_pressed: bool,
    /// Whether B was newly pressed.
    pub b_pressed: bool,
    /// Whether A is held.
    pub a_held: bool,
    /// Whether B is held.
    pub b_held: bool,
}

impl PrinterInput {
    /// Returns input with neither button pressed or held.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            a_pressed: false,
            b_pressed: false,
            a_held: false,
            b_held: false,
        }
    }

    /// Returns whether either button was newly pressed.
    #[must_use]
    pub const fn confirm_pressed(self) -> bool {
        self.a_pressed || self.b_pressed
    }

    /// Returns whether either button is held.
    #[must_use]
    pub const fn confirm_held(self) -> bool {
        self.a_held || self.b_held
    }
}

/// A glyph revealed by [`Printer::tick`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RevealedGlyph {
    /// Window-local left edge.
    pub x: i32,
    /// Window-local top edge.
    pub y: i32,
    /// The glyph's bitmap and advance width.
    pub glyph: Glyph,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrinterState {
    HandleChar,
    AwaitingScroll,
    Scrolling { pixels_remaining: i32 },
    AwaitingClear,
    AwaitingPress,
    Pause { frames_remaining: u8 },
    Finished,
}

/// The result of one [`Printer::tick`] call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TickEvent {
    /// The frame revealed no glyph: either the reveal delay consumed it, a
    /// keypad icon was placed ([`Printer::keypad_icon`]), or a
    /// `CLEAR`/`CLEAR_TO` control erased a span. Upstream ends the frame for
    /// both (`RENDER_UPDATE` and `RENDER_PRINT`, `pokeemerald/src/text.c`
    /// `:352-360`). [`Printer::cleared_span`] distinguishes them. At Slow/Mid
    /// speed a `FILL_WINDOW` control also produces this variant, the same as
    /// any other `RENDER_REPEAT`-class token: its own reveal-delay reload is
    /// spent before the next token starts (`text.c:941-962`); only at
    /// Fast/Instant does its glyph, if any, reach the caller within the same
    /// call. [`Printer::cleared_window`] distinguishes an idle `FILL_WINDOW`
    /// tick from an ordinary delay.
    Idle,
    /// A glyph became visible.
    Glyph(Box<RevealedGlyph>),
    /// A scroll prompt is waiting for a new A or B press.
    AwaitingScroll,
    /// A scroll was confirmed and will move on the next frame.
    ScrollStarted,
    /// The window contents moved upward.
    Scrolling {
        /// Pixels moved this frame.
        dy: i32,
    },
    /// The scroll completed; token handling resumes next frame.
    ScrollFinished,
    /// A page-clear prompt is waiting for a new A or B press.
    AwaitingClear,
    /// The page was confirmed and the cursor returned to its origin.
    /// [`Printer::cleared_window`] is also set on this tick.
    Cleared,
    /// `PAUSE_UNTIL_PRESS` is waiting for a new A or B press.
    AwaitingPress,
    /// The wait was satisfied; token handling resumes next frame.
    PressAccepted,
    /// A pause consumed one frame.
    Paused,
    /// The pause completed; token handling resumes next frame.
    PauseFinished,
    /// The token stream ended.
    Finished,
}

#[must_use]
fn glyph_id_for_token(tok: &Token) -> Option<u16> {
    match *tok {
        Token::Char(c) => super::char_to_byte(c).map(u16::from),
        Token::Symbol(s) => Some(u16::from(super::byte_from_symbol(s))),
        Token::ExtraSymbol(idx) => Some(EXTRA_SYMBOL_PAGE_START | u16::from(idx)),
        Token::BardWordDelimit => Some(u16::from(super::CHAR_BARD_WORD_DELIMIT)),
        _ => None,
    }
}

/// A frame-driven printer over decoded text tokens.
///
/// The printer owns its glyph source and emits blit and layout events rather
/// than writing a pixel buffer. [`Printer::new`] carries that one source;
/// [`Printer::with_sources`] also carries the other modelled Latin sources,
/// retrievable through [`Printer::source`], so a later `FONT` control can
/// select one. Text always renders from the initial source until then.
#[derive(Debug)]
pub struct Printer<S> {
    tokens: Vec<Token>,
    next_token_index: usize,
    glyphs: S,
    /// The modelled sources other than `glyphs`, in [`FontId::ALL`] order.
    /// Empty for a printer built by [`Printer::new`].
    alternate_glyphs: Vec<S>,
    speed: TextSpeed,
    reveal_delay_frames_remaining: u8,
    origin: PixelPos,
    cursor: PixelPos,
    line_height: i32,
    clear_height: i32,
    cleared_span: Option<ClearedSpan>,
    cleared_window: bool,
    keypad_icon: Option<KeypadIconPlacement>,
    state: PrinterState,
    allows_ab_speed_up: bool,
    ab_speed_up_latched: bool,
}

impl<S: GlyphSource> Printer<S> {
    /// Creates a printer at the window-local `origin`.
    ///
    /// A/B speed-up is disabled until [`Self::with_ab_speed_up_print`] is used.
    #[must_use]
    pub fn new(tokens: Vec<Token>, glyphs: S, speed: TextSpeed, origin: PixelPos) -> Self {
        Self {
            tokens,
            next_token_index: 0,
            line_height: max_letter_height(glyphs.font()),
            clear_height: clear_span_height(glyphs.font()),
            cleared_span: None,
            cleared_window: false,
            keypad_icon: None,
            glyphs,
            alternate_glyphs: Vec::new(),
            speed,
            reveal_delay_frames_remaining: 0,
            origin,
            cursor: origin,
            state: PrinterState::HandleChar,
            allows_ab_speed_up: false,
            ab_speed_up_latched: false,
        }
    }

    /// Creates a printer at the window-local `origin` that renders from
    /// `initial`'s source and also carries every other modelled source.
    ///
    /// Behaves exactly as [`Self::new`] over `initial`'s source: nothing
    /// selects an alternate source yet.
    #[must_use]
    pub fn with_sources(
        tokens: Vec<Token>,
        sources: GlyphSources<S>,
        initial: FontId,
        speed: TextSpeed,
        origin: PixelPos,
    ) -> Self {
        let (selected, alternate_glyphs) = sources.split(initial);
        Self {
            alternate_glyphs,
            ..Self::new(tokens, selected, speed, origin)
        }
    }

    /// The font the printer currently renders from.
    #[must_use]
    pub fn selected_font(&self) -> FontId {
        self.glyphs.font()
    }

    /// The source this printer carries for `font`, or `None` if it was built
    /// without that font ([`Self::new`] carries only its one source).
    #[must_use]
    pub fn source(&self, font: FontId) -> Option<&S> {
        std::iter::once(&self.glyphs)
            .chain(&self.alternate_glyphs)
            .find(|source| source.font() == font)
    }

    /// Sets the template line spacing in pixels (`TextPrinterTemplate.lineSpacing`).
    ///
    /// A newline advances the cursor by the font's maximum letter height plus
    /// this spacing (`pokeemerald/src/text.c:970`), and a scroll prompt scrolls
    /// the same distance (`text.c:1184`). Printers default to zero spacing.
    #[must_use]
    pub fn with_line_spacing(mut self, line_spacing: u8) -> Self {
        self.line_height = max_letter_height(self.glyphs.font()) + i32::from(line_spacing);
        self
    }

    /// Enables A/B speed-up for this printer.
    #[must_use]
    pub const fn with_ab_speed_up_print(mut self) -> Self {
        self.allows_ab_speed_up = true;
        self
    }

    /// The cursor's current window-local pixel position.
    #[must_use]
    pub const fn cursor(&self) -> PixelPos {
        self.cursor
    }

    /// The span the most recent [`Self::tick`] erased, if that tick handled a
    /// forward `CLEAR` or `CLEAR_TO`.
    ///
    /// Upstream erases the pixels itself; this printer emits events instead of
    /// owning a window buffer, so a caller that retains revealed glyphs uses
    /// this rectangle to erase them. At most one span exists per tick because
    /// upstream's `RENDER_PRINT` ends the frame (`pokeemerald/src/text.c`
    /// `:352-360`).
    #[must_use]
    pub const fn cleared_span(&self) -> Option<ClearedSpan> {
        self.cleared_span
    }

    /// The keypad icon the most recent [`Self::tick`] placed, if any.
    ///
    /// Upstream draws the icon, advances the cursor by its width, and returns
    /// `RENDER_PRINT`, ending the frame (`pokeemerald/src/text.c:1114-1118`);
    /// that tick returns [`TickEvent::Idle`]. This printer owns no icon
    /// bitmaps, so a caller draws the icon at the reported position. The
    /// printer has no template letter spacing, so the advance is the width
    /// alone.
    #[must_use]
    pub const fn keypad_icon(&self) -> Option<KeypadIconPlacement> {
        self.keypad_icon
    }

    /// Whether the most recent [`Self::tick`] cleared the complete window and
    /// reset the cursor to the origin: set by a forward `FILL_WINDOW`
    /// (`pokeemerald/src/text.c:1052-1056`) or by the page-clear prompt's
    /// confirmation ([`TickEvent::Cleared`]). Like [`Self::cleared_span`], the
    /// caller applies the erase itself; unlike it, `FILL_WINDOW` never ends
    /// upstream's frame (it is unconditionally `RENDER_REPEAT`), so at
    /// Fast/Instant speed a glyph decoded later in the same [`Self::tick`]
    /// call can still be its returned event -- at Slow/Mid that call instead
    /// returns [`TickEvent::Idle`], the same as any other `RENDER_REPEAT`
    /// token's own reveal-delay reload.
    #[must_use]
    pub const fn cleared_window(&self) -> bool {
        self.cleared_window
    }

    /// Returns whether the token stream has ended.
    #[must_use]
    pub const fn is_finished(&self) -> bool {
        matches!(self.state, PrinterState::Finished)
    }

    /// Starts a new token stream with the existing glyph source and settings.
    pub fn restart(&mut self, tokens: Vec<Token>) {
        self.tokens = tokens;
        self.next_token_index = 0;
        self.reveal_delay_frames_remaining = 0;
        self.cursor = self.origin;
        self.cleared_span = None;
        self.cleared_window = false;
        self.keypad_icon = None;
        self.state = PrinterState::HandleChar;
        self.ab_speed_up_latched = false;
    }

    /// Advance the printer by exactly one frame.
    ///
    /// New button presses confirm clear, scroll, and press-wait prompts.
    /// Pressing during a reveal delay latches A/B speed-up when enabled;
    /// later held frames skip remaining reveal delays.
    pub fn tick(&mut self, input: PrinterInput) -> TickEvent {
        self.cleared_span = None;
        self.cleared_window = false;
        self.keypad_icon = None;
        match self.state {
            PrinterState::Finished => TickEvent::Finished,
            PrinterState::HandleChar => self.tick_handle_char(input),
            PrinterState::AwaitingScroll => {
                if input.confirm_pressed() {
                    self.cursor.0 = self.origin.0;
                    self.state = PrinterState::Scrolling {
                        pixels_remaining: self.line_height,
                    };
                    TickEvent::ScrollStarted
                } else {
                    TickEvent::AwaitingScroll
                }
            }
            PrinterState::Scrolling { pixels_remaining } => {
                if pixels_remaining > 0 {
                    let dy = self.speed.scroll_px_per_frame(pixels_remaining);
                    self.state = PrinterState::Scrolling {
                        pixels_remaining: pixels_remaining - dy,
                    };
                    TickEvent::Scrolling { dy }
                } else {
                    self.state = PrinterState::HandleChar;
                    TickEvent::ScrollFinished
                }
            }
            PrinterState::AwaitingClear => {
                if input.confirm_pressed() {
                    self.cursor = self.origin;
                    self.cleared_window = true;
                    self.state = PrinterState::HandleChar;
                    TickEvent::Cleared
                } else {
                    TickEvent::AwaitingClear
                }
            }
            PrinterState::AwaitingPress => {
                if input.confirm_pressed() {
                    self.state = PrinterState::HandleChar;
                    TickEvent::PressAccepted
                } else {
                    TickEvent::AwaitingPress
                }
            }
            PrinterState::Pause { frames_remaining } => self.tick_pause(frames_remaining),
        }
    }

    fn tick_pause(&mut self, frames_remaining: u8) -> TickEvent {
        if frames_remaining != 0 {
            self.state = PrinterState::Pause {
                frames_remaining: frames_remaining - 1,
            };
            TickEvent::Paused
        } else {
            self.state = PrinterState::HandleChar;
            TickEvent::PauseFinished
        }
    }

    fn tick_handle_char(&mut self, input: PrinterInput) -> TickEvent {
        loop {
            if self.consume_reveal_delay_frame(input) {
                return TickEvent::Idle;
            }
            self.reveal_delay_frames_remaining = self.speed.wait_frames();

            let Some(token) = self.tokens.get(self.next_token_index).cloned() else {
                self.state = PrinterState::Finished;
                return TickEvent::Finished;
            };
            self.next_token_index += 1;

            let glyph_id = match token {
                Token::End => {
                    self.state = PrinterState::Finished;
                    return TickEvent::Finished;
                }
                Token::Newline => {
                    self.cursor.0 = self.origin.0;
                    self.cursor.1 = self.cursor.1.saturating_add(self.line_height);
                    continue;
                }
                Token::PromptScroll => {
                    self.state = PrinterState::AwaitingScroll;
                    return TickEvent::AwaitingScroll;
                }
                Token::PromptClear => {
                    self.state = PrinterState::AwaitingClear;
                    return TickEvent::AwaitingClear;
                }
                Token::ExtCtrl { sub, args } if sub == ext_ctrl::PAUSE => {
                    let frames_remaining = args.first().copied().unwrap_or(0);
                    self.reveal_delay_frames_remaining = 0;
                    self.state = PrinterState::Pause { frames_remaining };
                    return self.tick_pause(frames_remaining);
                }
                // Non-auto-scroll `TextPrinterWait` only (`text.c:884-899`);
                // auto-scroll is unmodelled in this port.
                Token::ExtCtrl { sub, .. } if sub == ext_ctrl::PAUSE_UNTIL_PRESS => {
                    self.state = PrinterState::AwaitingPress;
                    return TickEvent::AwaitingPress;
                }
                Token::ExtCtrl { sub, args } => match self.apply_extended_control(sub, &args) {
                    ControlOutcome::Repeat => None,
                    ControlOutcome::Glyph(id) => Some(id),
                    // Upstream's `RENDER_PRINT` copies the erased window to
                    // VRAM and ends the frame (`text.c:352-360`).
                    ControlOutcome::ClearedSpan => return TickEvent::Idle,
                    // `FILL_WINDOW` is also `RENDER_REPEAT` (`text.c`
                    // `:1052-1056`): the caller reads `cleared_window()`
                    // alongside whichever event this call eventually
                    // returns, so token handling keeps going here.
                    ControlOutcome::ClearedWindow => {
                        self.cleared_window = true;
                        None
                    }
                },
                Token::KeypadIcon(id) => {
                    let Some(placement) = keypad_icon::place(id, &mut self.cursor) else {
                        continue;
                    };
                    self.keypad_icon = Some(placement);
                    // `RENDER_PRINT` ends the frame (`text.c:1118`).
                    return TickEvent::Idle;
                }
                ref other => glyph_id_for_token(other),
            };

            let Some(glyph_id) = glyph_id else {
                continue;
            };
            let Some(glyph) = self.glyphs.glyph(glyph_id) else {
                continue;
            };

            let placement = RevealedGlyph {
                x: self.cursor.0,
                y: self.cursor.1,
                glyph,
            };
            self.cursor.0 = self.cursor.0.saturating_add(i32::from(glyph.advance_width));
            return TickEvent::Glyph(Box::new(placement));
        }
    }

    fn consume_reveal_delay_frame(&mut self, input: PrinterInput) -> bool {
        if input.confirm_held() && self.ab_speed_up_latched {
            self.reveal_delay_frames_remaining = 0;
        }
        if self.reveal_delay_frames_remaining == 0 {
            return false;
        }

        self.reveal_delay_frames_remaining -= 1;
        if self.allows_ab_speed_up && input.confirm_pressed() {
            self.ab_speed_up_latched = true;
            self.reveal_delay_frames_remaining = 0;
        }
        true
    }
}

#[cfg(test)]
mod tests;
