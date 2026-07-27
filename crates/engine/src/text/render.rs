//! Glyph renderer: token stream → glyph blits, frame-driven (S-5, issue #124).
//!
//! Behavioural re-implementation `(behavioral-fidelity)` of upstream's
//! per-frame text printer, `pokeemerald/src/text.c`'s `struct TextPrinter` /
//! `RenderText` / `AddTextPrinter` (driven every frame by `RunTextPrinters`).
//! [`Printer`] consumes a decoded [`super::Token`] stream (from
//! [`super::decode`]) and an `assets::fonts::FontGlyphSheet`, and exposes one
//! [`Printer::tick`] call per rendered frame — deterministic, no timers, no
//! global state `(oop-boundaries)`: the caller drives the clock.
//!
//! # Advance widths and glyph identity
//!
//! A decoded [`super::Token::Char`]/[`super::Token::Symbol`] doesn't carry
//! the original Gen-3 byte, but upstream's glyph id *is* that byte (the
//! `DecompressGlyph_*`/`GetGlyphWidth_*` family index
//! `gFont*LatinGlyphWidths[glyphId]` with the raw encoded byte, no
//! translation). [`super::char_to_byte`] / [`super::byte_from_symbol`] are
//! therefore the exact glyph-id lookup this module needs — see
//! [`glyph_id_for_token`].
//!
//! # State machine
//!
//! [`PrinterState`] mirrors upstream's `RENDER_STATE_*` enum, reduced to the
//! v1 path this slice covers:
//!
//! * `HandleChar` (`RENDER_STATE_HANDLE_CHAR`) — the steady state. Consuming
//!   a [`super::Token::Newline`] (`\n`), a colour/font/font-reset extended
//!   control code, or an unsupported placeholder/dynamic/keypad-icon token
//!   costs no frame (upstream `RENDER_REPEAT`: `RenderFont`'s caller loops
//!   `RenderText` again immediately); consuming a glyph costs exactly one
//!   frame and starts the next glyph's reveal delay.
//! * `AwaitingScroll` / `Scrolling` (`RENDER_STATE_SCROLL_START` /
//!   `RENDER_STATE_SCROLL`) — `\l` (`CHAR_PROMPT_SCROLL`): wait for
//!   [`Printer::tick`]'s `confirm_pressed`, then scroll the box up one line
//!   over multiple frames at [`TextSpeed::scroll_px_per_frame`] (upstream
//!   `sWindowVerticalScrollSpeeds`).
//! * `AwaitingClear` (`RENDER_STATE_CLEAR`) — `\p` (`CHAR_PROMPT_CLEAR`):
//!   wait for `confirm_pressed`, then reset the cursor to the box's origin
//!   (a fresh "page") on the same tick — upstream combines the wait check
//!   and the clear in one `TextPrinterWaitWithDownArrow` branch.
//! * `Finished` (`RENDER_FINISH`) — the `0xFF` terminator was consumed.
//!
//! # Deliberately out of scope
//!
//! Per the issue's scope, extended control codes that only affect colour/
//! font selection (`COLOR`, `HIGHLIGHT`, `SHADOW`, `COLOR_HIGHLIGHT_SHADOW`,
//! `PALETTE`, `FONT`, `RESET_FONT`, `MIN_LETTER_SPACING`, `JPN`/`ENG`) are
//! consumed as no-ops: they cost no frame but have no layout effect. Codes
//! needing state this crate doesn't have yet are likewise no-ops:
//! `PAUSE`/`PAUSE_UNTIL_PRESS`/`WAIT_SE` (timed/input pausing beyond
//! `\l`/`\p`), `PLAY_BGM`/`PLAY_SE`/`PAUSE_MUSIC`/`RESUME_MUSIC` (audio),
//! `FILL_WINDOW`/`CLEAR`/`CLEAR_TO` (they clear pixels in a window buffer
//! this module doesn't own — see the module docs on [`Printer`] for the
//! buffer-free design). [`super::Token::Placeholder`], [`super::Token::Dynamic`],
//! and [`super::Token::KeypadIcon`] have no glyph-sheet or buffer backing
//! this slice, so they are skipped (zero-width, no visible glyph) rather
//! than guessed at.

use assets::fonts::{FontGlyphSheet, FontId, Glyph};

use super::Token;

/// Extended control sub-codes this module gives real (non-no-op) layout
/// behaviour, from `pokeemerald/include/constants/characters.h`.
mod ext_ctrl {
    /// `EXT_CTRL_CODE_ESCAPE` — the next byte is a literal glyph id in the
    /// extended symbol page (`currChar | 0x100`), same page
    /// [`super::super::Token::ExtraSymbol`] addresses.
    pub const ESCAPE: u8 = 0x0C;
    /// `EXT_CTRL_CODE_SHIFT_RIGHT` — set the cursor's X to `origin.x + arg`.
    pub const SHIFT_RIGHT: u8 = 0x0D;
    /// `EXT_CTRL_CODE_SHIFT_DOWN` — set the cursor's Y to `origin.y + arg`.
    pub const SHIFT_DOWN: u8 = 0x0E;
    /// `EXT_CTRL_CODE_SKIP` — same cursor effect as `SHIFT_RIGHT` (upstream
    /// keeps them as separate codes; `text.c`'s `RenderText` implements them
    /// identically: `currentX = *currentChar + x`).
    pub const SKIP: u8 = 0x12;
}

/// Per-font line metrics `assets::fonts::FontId` doesn't carry (that crate
/// only has per-glyph advance widths — see its module docs): the vertical
/// distance a `\n` advances the cursor. Transcribed from upstream
/// `sFontInfos[fontId].maxLetterHeight` (`pokeemerald/src/text.c`); this
/// slice always renders with `lineSpacing == 0` (every v1 caller of
/// `AddTextPrinterParameterized2` — e.g. `AddTextPrinterForMessage`,
/// `pokeemerald/src/menu.c` — passes `.lineSpacing = 0` directly rather than
/// the font's own, generally-zero, `gFonts[fontId].lineSpacing`), so line
/// height is exactly `maxLetterHeight`.
#[must_use]
pub const fn max_letter_height(font: FontId) -> i32 {
    match font {
        FontId::Small => 12,
        FontId::Normal | FontId::Narrow => 16,
        FontId::Short => 14,
        FontId::SmallNarrow => 8,
    }
}

/// The three player-selectable text speeds, plus the "render everything on
/// this tick" mode upstream calls `TEXT_SKIP_DRAW`/`speed == 0`
/// (`AddTextPrinter`, `pokeemerald/src/text.c`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextSpeed {
    /// `OPTIONS_TEXT_SPEED_SLOW`.
    Slow,
    /// `OPTIONS_TEXT_SPEED_MID`.
    Mid,
    /// `OPTIONS_TEXT_SPEED_FAST`.
    Fast,
    /// Not a real `OPTIONS_TEXT_SPEED_*` option: upstream's `speed == 0` /
    /// `TEXT_SKIP_DRAW` fast path, reproduced here as "no reveal delay"
    /// rather than upstream's separate one-shot render loop, so the same
    /// [`Printer::tick`]-per-frame API covers it uniformly.
    Instant,
}

impl TextSpeed {
    /// Frames a glyph stays the "next" one before it's revealed (upstream
    /// `sTextSpeedFrameDelays`, `pokeemerald/src/menu.c`, as passed straight
    /// through to `AddTextPrinter`'s `speed` parameter by
    /// `GetPlayerTextSpeedDelay`). A new glyph becomes visible every
    /// `frames_per_char()` frames, including the very first one on frame 0.
    #[must_use]
    pub const fn frames_per_char(self) -> u8 {
        match self {
            Self::Slow => 8,
            Self::Mid => 4,
            Self::Fast => 1,
            Self::Instant => 0,
        }
    }

    /// The per-frame wait budget between two revealed glyphs: upstream
    /// decrements the `speed` argument once at `AddTextPrinter` time
    /// (`--sTempTextPrinter.textSpeed`) before storing it as the delay
    /// counter reload value, so the actual inter-glyph wait is
    /// `frames_per_char() - 1` frames, saturating at 0 (matches upstream's
    /// unsigned `u8` wraparound guard: `speed` is never 0 there except via
    /// the dedicated `TEXT_SKIP_DRAW` branch, which forces `textSpeed = 0`
    /// directly — the same value this saturating subtraction reaches for
    /// [`Self::Instant`]).
    #[must_use]
    const fn wait_frames(self) -> u8 {
        self.frames_per_char().saturating_sub(1)
    }

    /// Pixels the message box scrolls per frame while animating `\l`
    /// (upstream `sWindowVerticalScrollSpeeds`, `pokeemerald/src/text.c`,
    /// indexed by `GetPlayerTextSpeed()`). [`Self::Instant`] has no upstream
    /// table entry (the scroll animation is part of the frame-driven path
    /// upstream's `TEXT_SKIP_DRAW` mode bypasses entirely); modelled here as
    /// "the whole line scrolls in one frame" so [`Printer::tick`] stays
    /// uniform across every speed.
    #[must_use]
    const fn scroll_px_per_frame(self) -> i32 {
        match self {
            Self::Slow => 1,
            Self::Mid => 2,
            Self::Fast => 4,
            Self::Instant => i32::MAX,
        }
    }
}

/// A pixel position, `(x, y)`, relative to the printer's window (upstream
/// `TextPrinterTemplate`'s `x`/`y`/`currentX`/`currentY`, which are
/// themselves window-local, not screen-absolute).
pub type PixelPos = (i32, i32);

/// One glyph made visible on a [`Printer::tick`] call: where to blit it and
/// what to blit (upstream `CopyGlyphToWindow`, fed `gCurGlyph`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RevealedGlyph {
    /// Top-left pixel position, window-local.
    pub x: i32,
    /// Top-left pixel position, window-local.
    pub y: i32,
    /// The glyph's bitmap and advance width.
    pub glyph: Glyph,
}

/// The printer's internal state (upstream `TextPrinter::state`,
/// `RENDER_STATE_*`) — see the module docs for the v1 subset modelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrinterState {
    HandleChar,
    AwaitingScroll,
    Scrolling,
    AwaitingClear,
    Finished,
}

/// The result of one [`Printer::tick`] call.
///
/// [`Self::Glyph`] boxes its payload: [`RevealedGlyph`] embeds a whole
/// [`Glyph`] (256 pixel bytes), which would otherwise force every
/// [`TickEvent`] — even the common zero-payload ones like [`Self::Idle`] —
/// to be sized for it (`clippy::large_enum_variant`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TickEvent {
    /// Nothing new is visible yet (still waiting out the reveal delay).
    Idle,
    /// A glyph was revealed this tick.
    Glyph(Box<RevealedGlyph>),
    /// `\l` was reached; waiting for `confirm_pressed` before scrolling
    /// (upstream `RENDER_STATE_SCROLL_START`, animating the down-arrow
    /// prompt — the arrow's bobbing animation itself is out of scope, see
    /// the module docs).
    AwaitingScroll,
    /// `confirm_pressed` was seen while [`TickEvent::AwaitingScroll`]; the
    /// scroll animation begins next tick (upstream: the confirming tick
    /// only flips `RENDER_STATE_SCROLL_START` → `RENDER_STATE_SCROLL` and
    /// returns `RENDER_UPDATE`, it does not itself scroll any pixels).
    ScrollStarted,
    /// The box scrolled `dy` pixels this tick.
    Scrolling {
        /// Pixels scrolled this tick (positive, upward).
        dy: i32,
    },
    /// The scroll finished; printing resumes next tick.
    ScrollFinished,
    /// `\p` was reached; waiting for `confirm_pressed` before clearing
    /// (upstream `RENDER_STATE_CLEAR`).
    AwaitingClear,
    /// `confirm_pressed` was seen while [`TickEvent::AwaitingClear`]: the
    /// box was cleared and the cursor reset to the printer's origin (a new
    /// page) on this same tick.
    Cleared,
    /// The `0xFF` terminator was consumed; nothing more to print (upstream
    /// `RENDER_FINISH`).
    Finished,
}

/// Map a decoded [`Token`] to the upstream glyph id [`FontGlyphSheet::glyph`]
/// indexes with, or `None` if the token has no glyph-sheet backing (see the
/// module docs for exactly which tokens that is).
///
/// The raw Gen-3 byte *is* the glyph id upstream (`DecompressGlyph_Normal`
/// and its siblings, `pokeemerald/src/text.c`, index
/// `gFont*LatinGlyphWidths[glyphId]` with the encoded byte directly), so
/// this is exactly [`super::char_to_byte`]/[`super::byte_from_symbol`] run
/// backwards from the already-decoded token — see [`super::decode`].
/// [`Token::ExtraSymbol`] addresses the extended symbol page the same way
/// upstream's `CHAR_EXTRA_SYMBOL` case does: `currChar | 0x100`.
#[must_use]
fn glyph_id_for_token(tok: &Token) -> Option<u16> {
    match *tok {
        Token::Char(c) => super::char_to_byte(c).map(u16::from),
        Token::Symbol(s) => Some(u16::from(super::byte_from_symbol(s))),
        Token::ExtraSymbol(idx) => Some(u16::from(idx) | 0x100),
        _ => None,
    }
}

/// A frame-driven glyph printer over a decoded [`Token`] stream (upstream
/// `struct TextPrinter` + `RenderText`, `pokeemerald/src/text.c`).
///
/// Owns no pixel buffer and no font-sheet asset — it borrows a
/// [`FontGlyphSheet`] for the duration and emits [`RevealedGlyph`] blit
/// commands via [`Printer::tick`]; composing those into an actual pixel
/// buffer (upstream's window buffer + VRAM copy) is the platform present
/// path, out of scope here (see the issue). No global mutable state
/// `(oop-boundaries)`: every printer is an independent owned value, and the
/// caller supplies the clock (one [`Printer::tick`] call == one frame) and
/// the confirm-button edge (`confirm_pressed`) instead of this module
/// reading input itself.
pub struct Printer<'a> {
    tokens: Vec<Token>,
    pos: usize,
    sheet: FontGlyphSheet<'a>,
    speed: TextSpeed,
    delay_counter: u8,
    origin: PixelPos,
    cursor: PixelPos,
    line_height: i32,
    scroll_remaining: i32,
    state: PrinterState,
}

impl<'a> Printer<'a> {
    /// Build a printer over an already-decoded token stream (see
    /// [`super::decode`]), ready to print at `origin` (upstream
    /// `TextPrinterTemplate.x`/`.y` — e.g. `(0, 1)` for the standard
    /// overworld dialogue box, `AddTextPrinterForMessage`,
    /// `pokeemerald/src/menu.c`).
    #[must_use]
    pub fn new(
        tokens: Vec<Token>,
        sheet: FontGlyphSheet<'a>,
        speed: TextSpeed,
        origin: PixelPos,
    ) -> Self {
        Self {
            tokens,
            pos: 0,
            line_height: max_letter_height(sheet.font()),
            sheet,
            speed,
            delay_counter: 0,
            origin,
            cursor: origin,
            scroll_remaining: 0,
            state: PrinterState::HandleChar,
        }
    }

    /// The cursor's current window-local pixel position.
    #[must_use]
    pub const fn cursor(&self) -> PixelPos {
        self.cursor
    }

    /// Whether the terminator has been consumed — no further [`Printer::tick`]
    /// call will produce anything but [`TickEvent::Finished`].
    #[must_use]
    pub const fn is_finished(&self) -> bool {
        matches!(self.state, PrinterState::Finished)
    }

    /// Advance the printer by exactly one frame.
    ///
    /// `confirm_pressed` models the confirm button's *just-pressed* edge
    /// this frame (upstream `JOY_NEW(...)` inside
    /// `TextPrinterWaitWithDownArrow`) — only consulted while waiting on
    /// `\l`/`\p` ([`TickEvent::AwaitingScroll`]/[`TickEvent::AwaitingClear`]);
    /// ignored otherwise, matching upstream (the equivalent upstream checks
    /// are gated on those same states).
    pub fn tick(&mut self, confirm_pressed: bool) -> TickEvent {
        match self.state {
            PrinterState::Finished => TickEvent::Finished,
            PrinterState::HandleChar => self.tick_handle_char(),
            PrinterState::AwaitingScroll => {
                if confirm_pressed {
                    self.scroll_remaining = self.line_height;
                    self.cursor.0 = self.origin.0;
                    self.state = PrinterState::Scrolling;
                    TickEvent::ScrollStarted
                } else {
                    TickEvent::AwaitingScroll
                }
            }
            PrinterState::Scrolling => {
                if self.scroll_remaining > 0 {
                    let dy = self.scroll_remaining.min(self.speed.scroll_px_per_frame());
                    self.scroll_remaining -= dy;
                    TickEvent::Scrolling { dy }
                } else {
                    self.state = PrinterState::HandleChar;
                    TickEvent::ScrollFinished
                }
            }
            PrinterState::AwaitingClear => {
                if confirm_pressed {
                    self.cursor = self.origin;
                    self.state = PrinterState::HandleChar;
                    TickEvent::Cleared
                } else {
                    TickEvent::AwaitingClear
                }
            }
        }
    }

    /// `RENDER_STATE_HANDLE_CHAR`: the steady-state loop. Loops internally
    /// over every zero-cost ("`RENDER_REPEAT`") token — newlines, no-op
    /// extended control codes, unsupported placeholder-style tokens — until
    /// it either reveals a glyph, transitions to a waiting state, or hits
    /// the terminator; each of those ends the tick.
    fn tick_handle_char(&mut self) -> TickEvent {
        if self.delay_counter > 0 {
            self.delay_counter -= 1;
            return TickEvent::Idle;
        }
        loop {
            let Some(tok) = self.tokens.get(self.pos).cloned() else {
                self.state = PrinterState::Finished;
                return TickEvent::Finished;
            };
            self.pos += 1;

            let glyph_id = match tok {
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
                Token::ExtCtrl { sub, args } => self.apply_ext_ctrl(sub, &args),
                ref other => glyph_id_for_token(other),
            };

            let Some(id) = glyph_id else { continue };
            let Some(glyph) = self.sheet.glyph(id) else {
                continue;
            };

            let placement = RevealedGlyph {
                x: self.cursor.0,
                y: self.cursor.1,
                glyph,
            };
            self.cursor.0 += i32::from(glyph.advance_width);
            self.delay_counter = self.speed.wait_frames();
            return TickEvent::Glyph(Box::new(placement));
        }
    }

    /// Apply an extended control code's layout effect (see the `ext_ctrl`
    /// submodule and the module docs for exactly which sub-codes do
    /// something here). Returns `Some(glyph_id)` for `ESCAPE`, the one
    /// sub-code that produces a visible glyph; every other handled sub-code
    /// has a cursor side effect but no glyph, so it returns `None` like an
    /// unhandled one.
    fn apply_ext_ctrl(&mut self, sub: u8, args: &[u8]) -> Option<u16> {
        match sub {
            ext_ctrl::ESCAPE => return args.first().map(|&n| u16::from(n) | 0x100),
            ext_ctrl::SHIFT_RIGHT | ext_ctrl::SKIP => {
                if let Some(&n) = args.first() {
                    self.cursor.0 = self.origin.0 + i32::from(n);
                }
            }
            ext_ctrl::SHIFT_DOWN => {
                if let Some(&n) = args.first() {
                    self.cursor.1 = self.origin.1 + i32::from(n);
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
    use assets::fonts::{FontGlyphSheet, FontId, FontImageRef};
    use assets::pack::ImageRef;

    const SHEET_WIDTH: u32 = 256;
    const SHEET_HEIGHT: u32 = 512;

    /// A synthetic, uniformly-`0` glyph sheet the exact real shape
    /// (256x512, see `assets::fonts`' module docs) so [`FontGlyphSheet::new`]
    /// accepts it, mirroring `assets::fonts`'s own tests.
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
        FontGlyphSheet::new(FontImageRef::new(font, image)).unwrap()
    }

    fn decode_tokens(bytes: &[u8]) -> Vec<Token> {
        super::super::decode(bytes).unwrap()
    }

    #[test]
    fn advance_widths_move_the_cursor_by_the_sheets_glyph_width() {
        // FONT_NORMAL 'A' (byte 0xBB) has advance width 6 per
        // gFontNormalLatinGlyphWidths (crates/assets/src/fonts.rs).
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[0xBB, 0xBB, super::super::EOS]); // "AA"
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 1));

        let TickEvent::Glyph(first) = printer.tick(false) else {
            panic!("expected a glyph")
        };
        assert_eq!((first.x, first.y), (0, 1));
        assert_eq!(first.glyph.advance_width, 6);

        let TickEvent::Glyph(second) = printer.tick(false) else {
            panic!("expected a glyph")
        };
        assert_eq!((second.x, second.y), (6, 1));
        assert_eq!(printer.cursor(), (12, 1));
    }

    #[test]
    fn every_font_reports_the_upstream_max_letter_height() {
        // sFontInfos (pokeemerald/src/text.c) ground truth.
        assert_eq!(max_letter_height(FontId::Small), 12);
        assert_eq!(max_letter_height(FontId::Normal), 16);
        assert_eq!(max_letter_height(FontId::Short), 14);
        assert_eq!(max_letter_height(FontId::Narrow), 16);
        assert_eq!(max_letter_height(FontId::SmallNarrow), 8);
    }

    #[test]
    fn newline_resets_x_and_advances_y_by_line_height_for_free() {
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        // "A\nA" -- newline must not cost a tick of its own.
        let tokens = decode_tokens(&[0xBB, super::super::CHAR_NEWLINE, 0xBB, super::super::EOS]);
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 1));

        let TickEvent::Glyph(first) = printer.tick(false) else {
            panic!("expected a glyph")
        };
        assert_eq!((first.x, first.y), (0, 1));

        // Second tick consumes the newline AND draws the next glyph in one
        // frame (newline is RENDER_REPEAT upstream, not a frame of its own).
        let TickEvent::Glyph(second) = printer.tick(false) else {
            panic!("expected a glyph after the free newline")
        };
        assert_eq!((second.x, second.y), (0, 1 + 16));
    }

    #[test]
    fn reveal_cadence_matches_frames_per_char_table() {
        // MID = 4 frames/char (sTextSpeedFrameDelays, src/menu.c): a glyph
        // appears on frame 0, the next not until frame 4.
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[0xBB, 0xBB, super::super::EOS]);
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Mid, (0, 0));

        assert!(matches!(printer.tick(false), TickEvent::Glyph(_)));
        for _ in 0..3 {
            assert_eq!(printer.tick(false), TickEvent::Idle);
        }
        assert!(matches!(printer.tick(false), TickEvent::Glyph(_)));
    }

    #[test]
    fn fast_speed_reveals_a_glyph_every_frame() {
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[0xBB, 0xBB, 0xBB, super::super::EOS]);
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Fast, (0, 0));

        for _ in 0..3 {
            assert!(matches!(printer.tick(false), TickEvent::Glyph(_)));
        }
    }

    #[test]
    fn slow_speed_waits_seven_frames_between_glyphs() {
        // SLOW = 8 frames/char -> 7 idle frames between reveals.
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[0xBB, 0xBB, super::super::EOS]);
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Slow, (0, 0));

        assert!(matches!(printer.tick(false), TickEvent::Glyph(_)));
        for _ in 0..7 {
            assert_eq!(printer.tick(false), TickEvent::Idle);
        }
        assert!(matches!(printer.tick(false), TickEvent::Glyph(_)));
    }

    #[test]
    fn prompt_scroll_waits_for_confirm_then_animates_the_scroll() {
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        // "A\lA": draw 'A', hit \l, wait, scroll, draw 'A' again.
        let tokens = decode_tokens(&[
            0xBB,
            super::super::CHAR_PROMPT_SCROLL,
            0xBB,
            super::super::EOS,
        ]);
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Fast, (0, 1));

        assert!(matches!(printer.tick(false), TickEvent::Glyph(_)));
        assert_eq!(printer.tick(false), TickEvent::AwaitingScroll);
        // Still waiting: no confirm yet.
        assert_eq!(printer.tick(false), TickEvent::AwaitingScroll);
        // Confirm: transitions state but does not itself move any pixels.
        assert_eq!(printer.tick(true), TickEvent::ScrollStarted);
        // FAST = 4px/frame; line height 16 -> 4 frames of Scrolling{4}, then
        // ScrollFinished, then printing resumes.
        for _ in 0..4 {
            assert_eq!(printer.tick(false), TickEvent::Scrolling { dy: 4 });
        }
        assert_eq!(printer.tick(false), TickEvent::ScrollFinished);
        let TickEvent::Glyph(g) = printer.tick(false) else {
            panic!("expected printing to resume after the scroll")
        };
        // X reset to origin by the scroll-start transition; Y unchanged
        // (upstream never touches currentY during a scroll -- the box's
        // visible content shifted instead).
        assert_eq!((g.x, g.y), (0, 1));
    }

    #[test]
    fn prompt_clear_waits_for_confirm_then_resets_the_cursor_on_page() {
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[
            0xBB,
            super::super::CHAR_NEWLINE,
            super::super::CHAR_PROMPT_CLEAR,
            0xBB,
            super::super::EOS,
        ]);
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Fast, (0, 1));

        assert!(matches!(printer.tick(false), TickEvent::Glyph(_)));
        // The free newline plus hitting \p happen in the same tick.
        assert_eq!(printer.tick(false), TickEvent::AwaitingClear);
        assert_eq!(printer.tick(false), TickEvent::AwaitingClear);
        assert_eq!(printer.tick(true), TickEvent::Cleared);
        assert_eq!(printer.cursor(), (0, 1));
        let TickEvent::Glyph(g) = printer.tick(false) else {
            panic!("expected printing to resume on the new page")
        };
        assert_eq!((g.x, g.y), (0, 1));
    }

    #[test]
    fn finished_is_terminal_and_idempotent() {
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[0xBB, super::super::EOS]);
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 0));

        assert!(matches!(printer.tick(false), TickEvent::Glyph(_)));
        assert_eq!(printer.tick(false), TickEvent::Finished);
        assert!(printer.is_finished());
        // Further ticks stay Finished, never panic on the exhausted stream.
        assert_eq!(printer.tick(false), TickEvent::Finished);
    }

    #[test]
    fn extra_symbol_addresses_the_extended_glyph_page() {
        // CHAR_EXTRA_SYMBOL (0xF9) 0x00 -> glyph id 0x100, same as upstream's
        // `currChar | 0x100`.
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[super::super::CHAR_EXTRA_SYMBOL, 0x00, super::super::EOS]);
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 0));

        let TickEvent::Glyph(g) = printer.tick(false) else {
            panic!("expected a glyph")
        };
        assert_eq!(
            g.glyph.advance_width,
            FontId::Normal.glyph_width(0x100).unwrap()
        );
    }

    #[test]
    fn unsupported_tokens_are_skipped_without_costing_a_glyph() {
        // A player-name placeholder has no glyph-sheet backing in this
        // slice (see the module docs); it must be skipped, not panic or
        // draw a stray glyph, and drawing must continue right after it.
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[
            super::super::PLACEHOLDER_BEGIN,
            super::super::PLACEHOLDER_PLAYER,
            0xBB,
            super::super::EOS,
        ]);
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 0));

        let TickEvent::Glyph(g) = printer.tick(false) else {
            panic!("expected the placeholder to be skipped and 'A' drawn")
        };
        assert_eq!((g.x, g.y), (0, 0));
    }

    #[test]
    fn shift_right_extended_control_moves_the_cursor() {
        // EXT_CTRL_CODE_SHIFT_RIGHT (0x0D), arg 20 -> currentX = origin.x + 20.
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[
            super::super::EXT_CTRL_CODE_BEGIN,
            0x0D,
            20,
            0xBB,
            super::super::EOS,
        ]);
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 1));

        let TickEvent::Glyph(g) = printer.tick(false) else {
            panic!("expected a glyph")
        };
        assert_eq!((g.x, g.y), (20, 1));
    }

    #[test]
    fn text_speed_frame_tables_match_upstream() {
        // sTextSpeedFrameDelays (pokeemerald/src/menu.c).
        assert_eq!(TextSpeed::Slow.frames_per_char(), 8);
        assert_eq!(TextSpeed::Mid.frames_per_char(), 4);
        assert_eq!(TextSpeed::Fast.frames_per_char(), 1);
        assert_eq!(TextSpeed::Instant.frames_per_char(), 0);
        // sWindowVerticalScrollSpeeds (pokeemerald/src/text.c).
        assert_eq!(TextSpeed::Slow.scroll_px_per_frame(), 1);
        assert_eq!(TextSpeed::Mid.scroll_px_per_frame(), 2);
        assert_eq!(TextSpeed::Fast.scroll_px_per_frame(), 4);
    }
}
