//! Converts decoded text tokens into frame-driven glyph and control events.
//!
//! [`Printer`] owns its glyph source and render state. The caller supplies
//! [`PrinterInput`] and calls [`Printer::tick`] once per frame. Formatting,
//! audio, and window-buffer controls are consumed without applying effects.
//! Placeholders, dynamic tokens, and keypad icons are skipped because this
//! renderer has no expansion context or corresponding glyphs.

use assets::fonts::{FontId, Glyph, GlyphSource};

use super::Token;

mod ext_ctrl {
    pub const ESCAPE: u8 = super::super::EXT_CTRL_CODE_ESCAPE;
    pub const SHIFT_RIGHT: u8 = super::super::EXT_CTRL_CODE_SHIFT_RIGHT;
    pub const SHIFT_DOWN: u8 = super::super::EXT_CTRL_CODE_SHIFT_DOWN;
    pub const SKIP: u8 = super::super::EXT_CTRL_CODE_SKIP;
    pub const PAUSE: u8 = super::super::EXT_CTRL_CODE_PAUSE;
}

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

/// Text reveal cadence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextSpeed {
    /// Eight frames per glyph.
    Slow,
    /// Four frames per glyph.
    Mid,
    /// One glyph per frame.
    Fast,
    /// No reveal delay.
    Instant,
}

impl TextSpeed {
    /// Returns the configured frame interval between glyph reveals.
    #[must_use]
    pub const fn frames_per_char(self) -> u8 {
        match self {
            Self::Slow => 8,
            Self::Mid => 4,
            Self::Fast => 1,
            Self::Instant => 0,
        }
    }

    const fn wait_frames(self) -> u8 {
        self.frames_per_char().saturating_sub(1)
    }

    fn scroll_pixels_this_frame(self, pixels_remaining: i32) -> i32 {
        match self {
            Self::Slow => pixels_remaining.min(1),
            Self::Mid => pixels_remaining.min(2),
            Self::Fast => pixels_remaining.min(4),
            Self::Instant => pixels_remaining,
        }
    }
}

/// A window-local pixel position.
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
    Pause { frames_remaining: u8 },
    Finished,
}

/// The result of one [`Printer::tick`] call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TickEvent {
    /// The reveal delay consumed this frame.
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
    Cleared,
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
/// than writing a pixel buffer.
#[derive(Debug)]
pub struct Printer<S> {
    tokens: Vec<Token>,
    next_token_index: usize,
    glyphs: S,
    speed: TextSpeed,
    reveal_delay_frames_remaining: u8,
    origin: PixelPos,
    cursor: PixelPos,
    line_height: i32,
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
            glyphs,
            speed,
            reveal_delay_frames_remaining: 0,
            origin,
            cursor: origin,
            state: PrinterState::HandleChar,
            allows_ab_speed_up: false,
            ab_speed_up_latched: false,
        }
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
        self.state = PrinterState::HandleChar;
        self.ab_speed_up_latched = false;
    }

    /// Advance the printer by exactly one frame.
    ///
    /// New button presses confirm clear and scroll prompts. Pressing during a
    /// reveal delay latches A/B speed-up when enabled; later held frames skip
    /// remaining reveal delays.
    pub fn tick(&mut self, input: PrinterInput) -> TickEvent {
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
                    let dy = self.speed.scroll_pixels_this_frame(pixels_remaining);
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
                    self.state = PrinterState::HandleChar;
                    TickEvent::Cleared
                } else {
                    TickEvent::AwaitingClear
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
                    self.cursor.1 += self.line_height;
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
                Token::ExtCtrl { sub, args } => self.apply_extended_control(sub, &args),
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
            self.cursor.0 += i32::from(glyph.advance_width);
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

    fn apply_extended_control(&mut self, subcode: u8, arguments: &[u8]) -> Option<u16> {
        match subcode {
            ext_ctrl::ESCAPE => {
                return arguments
                    .first()
                    .map(|&index| EXTRA_SYMBOL_PAGE_START | u16::from(index));
            }
            ext_ctrl::SHIFT_RIGHT | ext_ctrl::SKIP => {
                if let Some(&offset) = arguments.first() {
                    self.cursor.0 = self.origin.0 + i32::from(offset);
                }
            }
            ext_ctrl::SHIFT_DOWN => {
                if let Some(&offset) = arguments.first() {
                    self.cursor.1 = self.origin.1 + i32::from(offset);
                }
            }
            _ => {}
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assets::fonts::{FontGlyphSheet, FontId, FontImageRef, SHEET_HEIGHT, SHEET_WIDTH};
    use assets::pack::ImageRef;

    const ENCODED_A: u8 = 0xBB;
    const NORMAL_A_ADVANCE_WIDTH: u8 = 6;
    const BARD_DELIMITER_ADVANCE_WIDTH: u8 = 3;
    const PAUSE_DURATION: u8 = 96;

    fn blank_sheet_pixels() -> Vec<u8> {
        vec![0u8; (SHEET_WIDTH * SHEET_HEIGHT) as usize]
    }

    fn synthetic_sheet(pixels: &[u8], font: FontId) -> FontGlyphSheet<'_> {
        let image = ImageRef {
            width: SHEET_WIDTH,
            height: SHEET_HEIGHT,
            bit_depth: 2,
            pixels,
        };
        FontGlyphSheet::new(FontImageRef::new_for_tests(font, image)).unwrap()
    }

    fn decode_tokens(bytes: &[u8]) -> Vec<Token> {
        super::super::decode(bytes).unwrap()
    }

    const fn press_a() -> PrinterInput {
        PrinterInput {
            a_pressed: true,
            b_pressed: false,
            a_held: false,
            b_held: false,
        }
    }

    #[test]
    fn advance_widths_move_the_cursor_by_the_sheets_glyph_width() {
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[ENCODED_A, ENCODED_A, super::super::EOS]);
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 1));

        let TickEvent::Glyph(first) = printer.tick(PrinterInput::none()) else {
            panic!("expected a glyph")
        };
        assert_eq!((first.x, first.y), (0, 1));
        assert_eq!(first.glyph.advance_width, NORMAL_A_ADVANCE_WIDTH);

        let TickEvent::Glyph(second) = printer.tick(PrinterInput::none()) else {
            panic!("expected a glyph")
        };
        assert_eq!((second.x, second.y), (i32::from(NORMAL_A_ADVANCE_WIDTH), 1));
        assert_eq!(printer.cursor(), (i32::from(NORMAL_A_ADVANCE_WIDTH) * 2, 1));
    }

    #[test]
    fn bard_word_delimiter_advances_by_the_blank_glyph_width() {
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[
            ENCODED_A,
            super::super::CHAR_BARD_WORD_DELIMIT,
            ENCODED_A,
            super::super::EOS,
        ]);
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 1));

        let TickEvent::Glyph(first) = printer.tick(PrinterInput::none()) else {
            panic!("expected a glyph")
        };
        assert_eq!((first.x, first.y), (0, 1));

        let TickEvent::Glyph(delim) = printer.tick(PrinterInput::none()) else {
            panic!("expected the delimiter's blank glyph")
        };
        assert_eq!((delim.x, delim.y), (i32::from(NORMAL_A_ADVANCE_WIDTH), 1));
        assert_eq!(delim.glyph.advance_width, BARD_DELIMITER_ADVANCE_WIDTH);

        let TickEvent::Glyph(second) = printer.tick(PrinterInput::none()) else {
            panic!("expected a glyph")
        };
        assert_eq!(
            (second.x, second.y),
            (
                i32::from(NORMAL_A_ADVANCE_WIDTH + BARD_DELIMITER_ADVANCE_WIDTH),
                1
            ),
            "words must stay separated"
        );
    }

    #[test]
    fn every_font_reports_the_upstream_max_letter_height() {
        assert_eq!(max_letter_height(FontId::Small), 12);
        assert_eq!(max_letter_height(FontId::Normal), 16);
        assert_eq!(max_letter_height(FontId::Short), 14);
        assert_eq!(max_letter_height(FontId::Narrow), 16);
        assert_eq!(max_letter_height(FontId::SmallNarrow), 8);
    }

    #[test]
    fn newline_resets_x_and_advances_y_by_line_height_at_instant_speed() {
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[
            ENCODED_A,
            super::super::CHAR_NEWLINE,
            ENCODED_A,
            super::super::EOS,
        ]);
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 1));

        let TickEvent::Glyph(first) = printer.tick(PrinterInput::none()) else {
            panic!("expected a glyph")
        };
        assert_eq!((first.x, first.y), (0, 1));

        let TickEvent::Glyph(second) = printer.tick(PrinterInput::none()) else {
            panic!("expected a glyph after the free newline")
        };
        assert_eq!((second.x, second.y), (0, 1 + 16));
    }

    #[test]
    fn newline_costs_a_reveal_delay_period_at_mid_speed() {
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[
            ENCODED_A,
            super::super::CHAR_NEWLINE,
            ENCODED_A,
            super::super::EOS,
        ]);
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Mid, (0, 0));

        let TickEvent::Glyph(first) = printer.tick(PrinterInput::none()) else {
            panic!("expected the first glyph on frame 0")
        };
        assert_eq!((first.x, first.y), (0, 0));

        for frame in 1..=6 {
            assert_eq!(
                printer.tick(PrinterInput::none()),
                TickEvent::Idle,
                "frame {frame} should be idle"
            );
        }

        let TickEvent::Glyph(second) = printer.tick(PrinterInput::none()) else {
            panic!("expected the second glyph on frame 7")
        };
        assert_eq!((second.x, second.y), (0, 16));
    }

    #[test]
    fn reveal_cadence_matches_frames_per_char_table() {
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[ENCODED_A, ENCODED_A, super::super::EOS]);
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Mid, (0, 0));

        assert!(matches!(
            printer.tick(PrinterInput::none()),
            TickEvent::Glyph(_)
        ));
        for _ in 0..3 {
            assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Idle);
        }
        assert!(matches!(
            printer.tick(PrinterInput::none()),
            TickEvent::Glyph(_)
        ));
    }

    #[test]
    fn fast_speed_reveals_a_glyph_every_frame() {
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[ENCODED_A, ENCODED_A, ENCODED_A, super::super::EOS]);
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Fast, (0, 0));

        for _ in 0..3 {
            assert!(matches!(
                printer.tick(PrinterInput::none()),
                TickEvent::Glyph(_)
            ));
        }
    }

    #[test]
    fn slow_speed_waits_seven_frames_between_glyphs() {
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[ENCODED_A, ENCODED_A, super::super::EOS]);
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Slow, (0, 0));

        assert!(matches!(
            printer.tick(PrinterInput::none()),
            TickEvent::Glyph(_)
        ));
        for _ in 0..7 {
            assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Idle);
        }
        assert!(matches!(
            printer.tick(PrinterInput::none()),
            TickEvent::Glyph(_)
        ));
    }

    #[test]
    fn prompt_scroll_waits_for_confirm_then_animates_the_scroll() {
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[
            ENCODED_A,
            super::super::CHAR_PROMPT_SCROLL,
            ENCODED_A,
            super::super::EOS,
        ]);
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Fast, (0, 1));

        assert!(matches!(
            printer.tick(PrinterInput::none()),
            TickEvent::Glyph(_)
        ));
        assert_eq!(
            printer.tick(PrinterInput::none()),
            TickEvent::AwaitingScroll
        );
        assert_eq!(
            printer.tick(PrinterInput::none()),
            TickEvent::AwaitingScroll
        );
        assert_eq!(printer.tick(press_a()), TickEvent::ScrollStarted);
        for _ in 0..4 {
            assert_eq!(
                printer.tick(PrinterInput::none()),
                TickEvent::Scrolling { dy: 4 }
            );
        }
        assert_eq!(
            printer.tick(PrinterInput::none()),
            TickEvent::ScrollFinished
        );
        let TickEvent::Glyph(g) = printer.tick(PrinterInput::none()) else {
            panic!("expected printing to resume after the scroll")
        };
        assert_eq!((g.x, g.y), (0, 1));
    }

    #[test]
    fn prompt_clear_waits_for_confirm_then_resets_the_cursor_on_page() {
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[
            ENCODED_A,
            super::super::CHAR_NEWLINE,
            super::super::CHAR_PROMPT_CLEAR,
            ENCODED_A,
            super::super::EOS,
        ]);
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Fast, (0, 1));

        assert!(matches!(
            printer.tick(PrinterInput::none()),
            TickEvent::Glyph(_)
        ));
        assert_eq!(printer.tick(PrinterInput::none()), TickEvent::AwaitingClear);
        assert_eq!(printer.tick(PrinterInput::none()), TickEvent::AwaitingClear);
        assert_eq!(printer.tick(press_a()), TickEvent::Cleared);
        assert_eq!(printer.cursor(), (0, 1));
        let TickEvent::Glyph(g) = printer.tick(PrinterInput::none()) else {
            panic!("expected printing to resume on the new page")
        };
        assert_eq!((g.x, g.y), (0, 1));
    }

    #[test]
    fn page_clear_resumes_after_a_reveal_delay_at_mid_speed() {
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[
            ENCODED_A,
            super::super::CHAR_PROMPT_CLEAR,
            ENCODED_A,
            super::super::EOS,
        ]);
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Mid, (0, 0));

        assert!(matches!(
            printer.tick(PrinterInput::none()),
            TickEvent::Glyph(_)
        ));
        for frame in 1..=3 {
            assert_eq!(
                printer.tick(PrinterInput::none()),
                TickEvent::Idle,
                "frame {frame} should be idle"
            );
        }
        assert_eq!(printer.tick(PrinterInput::none()), TickEvent::AwaitingClear);
        assert_eq!(printer.tick(PrinterInput::none()), TickEvent::AwaitingClear);
        assert_eq!(printer.tick(press_a()), TickEvent::Cleared);
        assert_eq!(printer.cursor(), (0, 0));
        for frame in 7..=9 {
            assert_eq!(
                printer.tick(PrinterInput::none()),
                TickEvent::Idle,
                "resume frame {frame} should be idle"
            );
        }
        let TickEvent::Glyph(first_glyph_on_new_page) = printer.tick(PrinterInput::none()) else {
            panic!("expected the new page's glyph on frame 10")
        };
        assert_eq!(
            (first_glyph_on_new_page.x, first_glyph_on_new_page.y),
            (0, 0)
        );
    }

    #[test]
    fn scroll_resumes_after_a_reveal_delay_at_mid_speed() {
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[
            ENCODED_A,
            super::super::CHAR_PROMPT_SCROLL,
            ENCODED_A,
            super::super::EOS,
        ]);
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Mid, (0, 1));

        assert!(matches!(
            printer.tick(PrinterInput::none()),
            TickEvent::Glyph(_)
        ));
        for _ in 0..3 {
            assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Idle);
        }
        assert_eq!(
            printer.tick(PrinterInput::none()),
            TickEvent::AwaitingScroll
        );
        assert_eq!(
            printer.tick(PrinterInput::none()),
            TickEvent::AwaitingScroll
        );
        assert_eq!(printer.tick(press_a()), TickEvent::ScrollStarted);
        for _ in 0..8 {
            assert_eq!(
                printer.tick(PrinterInput::none()),
                TickEvent::Scrolling { dy: 2 }
            );
        }
        assert_eq!(
            printer.tick(PrinterInput::none()),
            TickEvent::ScrollFinished
        );
        for _ in 0..3 {
            assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Idle);
        }
        let TickEvent::Glyph(g) = printer.tick(PrinterInput::none()) else {
            panic!("expected printing to resume after the scroll's reveal delay")
        };
        assert_eq!((g.x, g.y), (0, 1));
    }

    #[test]
    fn finished_is_terminal_and_idempotent() {
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[ENCODED_A, super::super::EOS]);
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 0));

        assert!(matches!(
            printer.tick(PrinterInput::none()),
            TickEvent::Glyph(_)
        ));
        assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Finished);
        assert!(printer.is_finished());
        assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Finished);
    }

    #[test]
    fn extra_symbol_addresses_the_extended_glyph_page() {
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[super::super::CHAR_EXTRA_SYMBOL, 0, super::super::EOS]);
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 0));

        let TickEvent::Glyph(g) = printer.tick(PrinterInput::none()) else {
            panic!("expected a glyph")
        };
        assert_eq!(
            g.glyph.advance_width,
            FontId::Normal.glyph_width(EXTRA_SYMBOL_PAGE_START).unwrap()
        );
    }

    #[test]
    fn unsupported_tokens_are_skipped_without_costing_a_glyph() {
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[
            super::super::PLACEHOLDER_BEGIN,
            super::super::PLACEHOLDER_PLAYER,
            ENCODED_A,
            super::super::EOS,
        ]);
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 0));

        let TickEvent::Glyph(g) = printer.tick(PrinterInput::none()) else {
            panic!("expected the placeholder to be skipped and 'A' drawn")
        };
        assert_eq!((g.x, g.y), (0, 0));
    }

    #[test]
    fn shift_right_extended_control_moves_the_cursor() {
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[
            super::super::EXT_CTRL_CODE_BEGIN,
            ext_ctrl::SHIFT_RIGHT,
            20,
            ENCODED_A,
            super::super::EOS,
        ]);
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 1));

        let TickEvent::Glyph(g) = printer.tick(PrinterInput::none()) else {
            panic!("expected a glyph")
        };
        assert_eq!((g.x, g.y), (20, 1));
    }

    #[test]
    fn restart_reprints_a_new_stream_from_the_top_keeping_the_sheet() {
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[ENCODED_A, super::super::EOS]);
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (3, 1));

        assert!(matches!(
            printer.tick(PrinterInput::none()),
            TickEvent::Glyph(_)
        ));
        assert!(matches!(
            printer.tick(PrinterInput::none()),
            TickEvent::Finished
        ));
        assert!(printer.is_finished());

        printer.restart(decode_tokens(&[ENCODED_A, ENCODED_A, super::super::EOS]));
        assert!(!printer.is_finished(), "a restarted printer prints again");
        assert_eq!(printer.cursor(), (3, 1), "the cursor is back at the origin");

        let TickEvent::Glyph(first) = printer.tick(PrinterInput::none()) else {
            panic!("expected the restarted stream's first glyph")
        };
        assert_eq!((first.x, first.y), (3, 1));
        let TickEvent::Glyph(second) = printer.tick(PrinterInput::none()) else {
            panic!("expected the restarted stream's second glyph")
        };
        assert_eq!(
            (second.x, second.y),
            (3 + i32::from(NORMAL_A_ADVANCE_WIDTH), 1),
            "advanced by the first glyph's width"
        );
        assert!(matches!(
            printer.tick(PrinterInput::none()),
            TickEvent::Finished
        ));
    }

    #[test]
    fn a_printer_over_an_owned_sheet_outlives_the_pack_bytes_it_came_from() {
        let mut printer = {
            let pixels = blank_sheet_pixels();
            let owned = synthetic_sheet(&pixels, FontId::Normal).to_owned_sheet();
            Printer::new(
                decode_tokens(&[ENCODED_A, super::super::EOS]),
                owned,
                TextSpeed::Instant,
                (0, 1),
            )
        };

        let TickEvent::Glyph(g) = printer.tick(PrinterInput::none()) else {
            panic!("expected a glyph from the owned sheet")
        };
        assert_eq!((g.x, g.y), (0, 1));
        assert_eq!(g.glyph.advance_width, NORMAL_A_ADVANCE_WIDTH);
    }

    #[test]
    fn b_alone_advances_a_prompt_clear_exactly_like_a_alone() {
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[
            ENCODED_A,
            super::super::CHAR_PROMPT_CLEAR,
            ENCODED_A,
            super::super::EOS,
        ]);
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Fast, (0, 1));

        assert!(matches!(
            printer.tick(PrinterInput::none()),
            TickEvent::Glyph(_)
        ));
        assert_eq!(printer.tick(PrinterInput::none()), TickEvent::AwaitingClear);
        let b_only = PrinterInput {
            a_pressed: false,
            b_pressed: true,
            a_held: false,
            b_held: false,
        };
        assert_eq!(printer.tick(b_only), TickEvent::Cleared);
        assert_eq!(printer.cursor(), (0, 1));
        let TickEvent::Glyph(g) = printer.tick(PrinterInput::none()) else {
            panic!("expected printing to resume on the new page")
        };
        assert_eq!((g.x, g.y), (0, 1));
    }

    #[test]
    fn pause_control_code_blocks_for_exactly_the_argument_frame_count() {
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[
            ENCODED_A,
            super::super::EXT_CTRL_CODE_BEGIN,
            super::super::EXT_CTRL_CODE_PAUSE,
            PAUSE_DURATION,
            ENCODED_A,
            super::super::EOS,
        ]);
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 1));

        assert!(
            matches!(printer.tick(PrinterInput::none()), TickEvent::Glyph(_)),
            "the first glyph reveals before the pause"
        );

        let mut paused_ticks = 0;
        loop {
            match printer.tick(PrinterInput::none()) {
                TickEvent::Paused => paused_ticks += 1,
                TickEvent::PauseFinished => break,
                other => panic!("expected Paused or PauseFinished, got {other:?}"),
            }
        }
        assert_eq!(
            paused_ticks,
            usize::from(PAUSE_DURATION),
            "pause duration must include the control code's frame"
        );

        let TickEvent::Glyph(second) = printer.tick(PrinterInput::none()) else {
            panic!("expected the second glyph the tick after PauseFinished")
        };
        assert_eq!(
            (second.x, second.y),
            (i32::from(NORMAL_A_ADVANCE_WIDTH), 1),
            "advanced past the first glyph"
        );
    }

    #[test]
    fn mid_speed_pause_resumes_without_an_extra_reveal_delay() {
        const TWO_FRAME_PAUSE: u8 = 2;

        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[
            ENCODED_A,
            super::super::EXT_CTRL_CODE_BEGIN,
            super::super::EXT_CTRL_CODE_PAUSE,
            TWO_FRAME_PAUSE,
            ENCODED_A,
            super::super::EOS,
        ]);
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Mid, (0, 1));

        assert!(matches!(
            printer.tick(PrinterInput::none()),
            TickEvent::Glyph(_)
        ));
        for _ in 0..3 {
            assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Idle);
        }
        for _ in 0..TWO_FRAME_PAUSE {
            assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Paused);
        }
        assert_eq!(printer.tick(PrinterInput::none()), TickEvent::PauseFinished);
        assert!(matches!(
            printer.tick(PrinterInput::none()),
            TickEvent::Glyph(_)
        ));
    }

    #[test]
    fn held_ab_speed_up_only_affects_printers_that_opt_in() {
        let pixels = blank_sheet_pixels();

        let opted_out_sheet = synthetic_sheet(&pixels, FontId::Normal);
        let opted_out_tokens = decode_tokens(&[ENCODED_A, ENCODED_A, ENCODED_A, super::super::EOS]);
        let mut opted_out_printer =
            Printer::new(opted_out_tokens, opted_out_sheet, TextSpeed::Mid, (0, 0));

        let held_a = PrinterInput {
            a_pressed: true,
            b_pressed: false,
            a_held: true,
            b_held: false,
        };
        assert!(
            matches!(opted_out_printer.tick(held_a), TickEvent::Glyph(_)),
            "the first glyph reveals on frame 0 regardless"
        );
        for frame in 1..=3 {
            assert_eq!(
                opted_out_printer.tick(held_a),
                TickEvent::Idle,
                "an opted-out printer's frame {frame} must stay un-accelerated"
            );
        }
        assert!(
            matches!(opted_out_printer.tick(held_a), TickEvent::Glyph(_)),
            "the second glyph still waits out the full MID delay"
        );

        let intro_sheet = synthetic_sheet(&pixels, FontId::Normal);
        let intro_tokens = decode_tokens(&[ENCODED_A, ENCODED_A, ENCODED_A, super::super::EOS]);
        let mut intro_printer = Printer::new(intro_tokens, intro_sheet, TextSpeed::Mid, (0, 0))
            .with_ab_speed_up_print();

        assert!(
            matches!(
                intro_printer.tick(PrinterInput::none()),
                TickEvent::Glyph(_)
            ),
            "the first glyph still reveals on frame 0"
        );
        assert_eq!(intro_printer.tick(held_a), TickEvent::Idle);
        assert!(
            matches!(
                intro_printer.tick(PrinterInput::none()),
                TickEvent::Glyph(_)
            ),
            "the second glyph reveals early once the speed-up latches"
        );
        assert!(
            matches!(intro_printer.tick(held_a), TickEvent::Glyph(_)),
            "the third glyph reveals immediately while A remains held"
        );
    }

    #[test]
    fn restart_clears_a_stuck_pause_state() {
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);

        let tokens = decode_tokens(&[
            super::super::EXT_CTRL_CODE_BEGIN,
            super::super::EXT_CTRL_CODE_PAUSE,
            PAUSE_DURATION,
            ENCODED_A,
            super::super::EOS,
        ]);
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 0));

        for tick in 1..=5 {
            assert_eq!(
                printer.tick(PrinterInput::none()),
                TickEvent::Paused,
                "tick {tick} should still be mid-pause"
            );
        }

        printer.restart(decode_tokens(&[ENCODED_A, super::super::EOS]));
        assert!(!printer.is_finished());
        assert_eq!(printer.cursor(), (0, 0));

        assert!(
            matches!(printer.tick(PrinterInput::none()), TickEvent::Glyph(_)),
            "restart must not leave the printer stuck mid-pause"
        );
    }

    #[test]
    fn restart_clears_the_latched_speed_up_flag() {
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[ENCODED_A, ENCODED_A, super::super::EOS]);
        let mut printer =
            Printer::new(tokens, sheet, TextSpeed::Mid, (0, 0)).with_ab_speed_up_print();

        let press_and_hold_a = PrinterInput {
            a_pressed: true,
            b_pressed: false,
            a_held: true,
            b_held: false,
        };
        let hold_a = PrinterInput {
            a_pressed: false,
            b_pressed: false,
            a_held: true,
            b_held: false,
        };

        assert!(matches!(
            printer.tick(PrinterInput::none()),
            TickEvent::Glyph(_)
        ));
        assert_eq!(printer.tick(press_and_hold_a), TickEvent::Idle);
        assert!(matches!(
            printer.tick(PrinterInput::none()),
            TickEvent::Glyph(_)
        ));

        printer.restart(decode_tokens(&[ENCODED_A, ENCODED_A, super::super::EOS]));
        assert!(matches!(
            printer.tick(PrinterInput::none()),
            TickEvent::Glyph(_)
        ));

        for frame in 1..=3 {
            assert_eq!(
                printer.tick(hold_a),
                TickEvent::Idle,
                "restart frame {frame}: the speed-up latch must not survive"
            );
        }
        assert!(matches!(printer.tick(hold_a), TickEvent::Glyph(_)));
    }

    #[test]
    fn text_speed_frame_tables_match_upstream() {
        assert_eq!(TextSpeed::Slow.frames_per_char(), 8);
        assert_eq!(TextSpeed::Mid.frames_per_char(), 4);
        assert_eq!(TextSpeed::Fast.frames_per_char(), 1);
        assert_eq!(TextSpeed::Instant.frames_per_char(), 0);
        assert_eq!(TextSpeed::Slow.scroll_pixels_this_frame(16), 1);
        assert_eq!(TextSpeed::Mid.scroll_pixels_this_frame(16), 2);
        assert_eq!(TextSpeed::Fast.scroll_pixels_this_frame(16), 4);
    }
}
