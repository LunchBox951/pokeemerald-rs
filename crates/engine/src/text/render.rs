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
//! therefore the exact glyph-id lookup this module needs — see the private
//! `glyph_id_for_token` helper.
//!
//! # State machine
//!
//! The printer's internal state mirrors upstream's `RENDER_STATE_*` enum,
//! reduced to the v1 path this slice covers:
//!
//! * `HandleChar` (`RENDER_STATE_HANDLE_CHAR`) — the steady state. Upstream
//!   reloads `delayCounter = textSpeed` *unconditionally* before consuming
//!   each char, and the `if (delayCounter && textSpeed)` guard at the top of
//!   the state burns it back down one frame at a time. A `RENDER_REPEAT`-class
//!   token — a [`super::Token::Newline`] (`\n`), a colour/font/font-reset
//!   extended control code, or an unsupported placeholder/dynamic/keypad-icon
//!   token — reloads that counter and loops straight back to the guard, so at
//!   Slow/Mid (`textSpeed != 0`) it costs one full reveal-delay period of
//!   `TextSpeed::frames_per_char - 1` idle frames before the next glyph; at
//!   Fast/Instant (`textSpeed == 0`) the counter stays 0 and such tokens
//!   remain free. Consuming a glyph likewise reloads the counter, so the next
//!   glyph's reveal delay follows. Concretely, `"A\nB"` at Mid draws `A` on
//!   frame 0 and `B` on frame 7 (`A`, three idle frames for `A`'s reveal
//!   delay, the `\n` consumed on frame 4 with three more idle frames, `B` on
//!   frame 7) — the `\n` is *not* frame-free at Slow/Mid.
//! * `AwaitingScroll` / `Scrolling` (`RENDER_STATE_SCROLL_START` /
//!   `RENDER_STATE_SCROLL`) — `\l` (`CHAR_PROMPT_SCROLL`): wait for
//!   [`Printer::tick`]'s `confirm_pressed`, then scroll the box up one line
//!   over multiple frames at the private `TextSpeed::scroll_px_per_frame`
//!   rate (upstream `sWindowVerticalScrollSpeeds`). The `\l` char reloaded
//!   `delayCounter = textSpeed` when it was consumed and the scroll never
//!   touches it, so at Slow/Mid the first glyph of the next page resumes
//!   after a full reveal-delay period, not immediately.
//! * `AwaitingClear` (`RENDER_STATE_CLEAR`) — `\p` (`CHAR_PROMPT_CLEAR`):
//!   wait for `confirm_pressed`, then reset the cursor to the box's origin
//!   (a fresh "page") on the same tick — upstream combines the wait check
//!   and the clear in one `TextPrinterWaitWithDownArrow` branch. Like `\l`,
//!   the `\p` char reloaded `delayCounter` when consumed, so the first glyph
//!   after the clear resumes after a reveal-delay period at Slow/Mid.
//! * `Pause` (`RENDER_STATE_PAUSE`) — `{PAUSE n}` (`EXT_CTRL_CODE_PAUSE`):
//!   count down `n` frames with nothing new revealed, then resume
//!   `HandleChar` (`text.c:1013-1017`, `text.c:1215-1220`). Unlike `\l`/`\p`,
//!   upstream's own `RENDER_REPEAT` return from the `HANDLE_CHAR` case
//!   re-enters `RenderText` a second time *on the same upstream frame*,
//!   landing straight in `RENDER_STATE_PAUSE` and burning the first of the
//!   `n` frames immediately — so the consuming tick is already the pause's
//!   first counted frame, not a free transition tick the way `\l`/`\p`'s own
//!   waiting states are. [`Printer::tick`]'s private `tick_pause` helper is
//!   called directly from the token match for exactly this reason: getting
//!   this same-frame chain wrong (deferring the first decrement to the next
//!   external tick, the way `\l`/`\p` do) undercounts by one frame in the
//!   other direction — an `{PAUSE n}` control code then blocks for `n + 1`
//!   frames instead of exactly `n`.
//! * `Finished` (`RENDER_FINISH`) — the `0xFF` terminator was consumed.
//!
//! # Held-A/B print speed-up
//!
//! Upstream's held-A/B reveal speed-up (`RENDER_STATE_HANDLE_CHAR`,
//! `text.c:943-953`) is opt-in per printer via [`Printer::with_ab_speed_up_print`]
//! (upstream `gTextFlags.canABSpeedUpPrint`, set per-message by
//! `AddTextPrinterForMessage`'s own `allowSkippingDelayWithButtonPress`
//! argument): a fresh *press* of A/B while a glyph's reveal delay is
//! counting down latches `has_print_been_sped_up` and zeroes that delay
//! immediately; from then on (upstream `subStruct->hasPrintBeenSpedUp`,
//! reset on [`Printer::restart`] like every other per-message render state),
//! simply *holding* A/B forces every further delay to zero too, without
//! needing another fresh press. [`Printer::new`] defaults this off — a bare
//! printer, opted into nothing, matching neither of upstream's two concrete
//! `allowSkippingDelayWithButtonPress` shapes until a caller picks one.
//!
//! The standard field message box is *not* one of the false ones: both
//! `ShowFieldMessage` and `ShowFieldMessageFromBuffer`
//! (`pokeemerald/src/field_message_box.c:62-69,109-129`) call
//! `AddTextPrinterForMessage(TRUE)`, same as the intro's Birch speech
//! (`pokeemerald/src/main_menu.c:1339`) — `InitFieldMessageBox`'s own
//! `gTextFlags.canABSpeedUpPrint = FALSE` (`field_message_box.c:17`) is only
//! the module's dormant-box reset, overwritten `TRUE` the moment a message
//! actually prints. `pokeemerald-rs`'s own field message box
//! (`crate::overworld::dialog::NpcDialog`, in the `pokeemerald-rs` crate)
//! opts in via [`Printer::with_ab_speed_up_print`] for exactly that reason;
//! only the fixed-label printers that never opted in (`pokeemerald-rs`'s
//! start-menu chrome and main menu, whose instant-speed label printers have
//! no reveal delay for a hold to shorten in the first place) stay on this
//! module's own off-by-default [`Printer::new`].
//!
//! # Deliberately out of scope
//!
//! Per the issue's scope, extended control codes that only affect colour/
//! font selection (`COLOR`, `HIGHLIGHT`, `SHADOW`, `COLOR_HIGHLIGHT_SHADOW`,
//! `PALETTE`, `FONT`, `RESET_FONT`, `MIN_LETTER_SPACING`, `JPN`/`ENG`) are
//! consumed as no-ops: they cost no frame but have no layout effect. Codes
//! needing state this crate doesn't have yet are likewise no-ops:
//! `PAUSE_UNTIL_PRESS`/`WAIT_SE` (input/audio pausing beyond `\l`/`\p`/
//! `PAUSE`), `PLAY_BGM`/`PLAY_SE`/`PAUSE_MUSIC`/`RESUME_MUSIC` (audio),
//! `FILL_WINDOW`/`CLEAR`/`CLEAR_TO` (they clear pixels in a window buffer
//! this module doesn't own — see the module docs on [`Printer`] for the
//! buffer-free design). [`super::Token::Placeholder`], [`super::Token::Dynamic`],
//! and [`super::Token::KeypadIcon`] have no glyph-sheet or buffer backing
//! this slice, so they are skipped (zero-width, no visible glyph) rather
//! than guessed at.

use assets::fonts::{FontId, Glyph, GlyphSource};

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
    /// `EXT_CTRL_CODE_PAUSE` — pause for the trailing argument's frame count
    /// before resuming (`PrinterState::Pause`, module docs' "State machine"
    /// section). Restated from [`super::super::EXT_CTRL_CODE_PAUSE`] rather
    /// than a duplicated bare literal, since that constant is also this
    /// crate's public authoring surface for the same sub-code (e.g.
    /// `pokeemerald_rs::intro::speech`'s `{PAUSE n}` marker).
    pub const PAUSE: u8 = super::super::EXT_CTRL_CODE_PAUSE;
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

/// Per-frame printer input: the A/B buttons' newly-pressed and held state
/// this frame (upstream `JOY_NEW`/`JOY_HELD`, both gated on `A_BUTTON |
/// B_BUTTON` together — `text.c:874-879`'s `TextPrinterWaitWithDownArrow`
/// and `text.c:943-953`'s held print speed-up never distinguish which of
/// the two buttons did it, so neither does this struct's own
/// [`Self::confirm_pressed`]/[`Self::confirm_held`]).
///
/// A plain bool-carrying struct, not a wrapper around `platform::ButtonState`:
/// `engine` does not depend on `platform` (`assets` is this crate's only
/// dependency — its own `Cargo.toml`), so the caller (a `pokeemerald-rs`
/// scene) is the one that reads a real `ButtonState` and narrows it down to
/// these four bits.
#[expect(
    clippy::struct_excessive_bools,
    reason = "Four independent flags -- two buttons, each with its own \
              pressed/held edge, matching upstream's own separate \
              `JOY_NEW`/`JOY_HELD` checks (module docs) -- they don't share \
              enough structure to collapse into a state machine or enum, \
              mirroring `assets::map_headers::MapHeader`'s identical \
              five-bool shape and its own exception."
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PrinterInput {
    /// A's just-pressed edge this frame (upstream `JOY_NEW(A_BUTTON)`).
    pub a_pressed: bool,
    /// B's just-pressed edge this frame (upstream `JOY_NEW(B_BUTTON)`).
    pub b_pressed: bool,
    /// Whether A is currently held down (upstream `JOY_HELD(A_BUTTON)`).
    pub a_held: bool,
    /// Whether B is currently held down (upstream `JOY_HELD(B_BUTTON)`).
    pub b_held: bool,
}

impl PrinterInput {
    /// No buttons pressed or held — the shape a non-interactive tick (a
    /// throwaway printer rendering a menu label to completion, a test that
    /// doesn't care about input) can pass.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            a_pressed: false,
            b_pressed: false,
            a_held: false,
            b_held: false,
        }
    }

    /// Upstream `JOY_NEW(A_BUTTON | B_BUTTON)` — either button's just-pressed
    /// edge this frame. The only input [`PrinterState::AwaitingScroll`]/
    /// [`PrinterState::AwaitingClear`] consult.
    #[must_use]
    pub const fn confirm_pressed(self) -> bool {
        self.a_pressed || self.b_pressed
    }

    /// Upstream `JOY_HELD(A_BUTTON | B_BUTTON)` — either button currently
    /// held. Only consulted by [`Printer::tick`]'s held print speed-up
    /// (module docs), and only once a printer has opted in via
    /// [`Printer::with_ab_speed_up_print`].
    #[must_use]
    pub const fn confirm_held(self) -> bool {
        self.a_held || self.b_held
    }
}

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
    Pause,
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
    /// A `{PAUSE n}` control code is counting down; nothing new is visible
    /// this tick (upstream `RENDER_STATE_PAUSE`, `text.c:1215-1220`).
    /// Returned for exactly `n` ticks — including the one that consumed the
    /// control code itself, module docs' "State machine" section on the
    /// same-frame `RENDER_REPEAT` chain.
    Paused,
    /// The pause's countdown reached zero this tick; printing resumes next
    /// tick, not this one (mirrors [`TickEvent::ScrollFinished`]'s identical
    /// one-tick-late shape).
    PauseFinished,
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
        // Upstream `RenderText` has no case for `CHAR_BARD_WORD_DELIMIT`:
        // the byte falls through to the glyph-draw path and advances by
        // glyph `0x37`'s width (a blank 3px cell in the Latin sheets), so
        // adjacent easy-chat words stay separated. Dropping it instead
        // would concatenate them.
        Token::BardWordDelimit => Some(u16::from(super::CHAR_BARD_WORD_DELIMIT)),
        _ => None,
    }
}

/// A frame-driven glyph printer over a decoded [`Token`] stream (upstream
/// `struct TextPrinter` + `RenderText`, `pokeemerald/src/text.c`).
///
/// Owns no pixel buffer — it emits [`RevealedGlyph`] blit commands via
/// [`Printer::tick`]; composing those into an actual pixel buffer
/// (upstream's window buffer + VRAM copy) is the platform present path, out
/// of scope here (see the issue). No global mutable state
/// `(oop-boundaries)`: every printer is an independent owned value, and the
/// caller supplies the clock (one [`Printer::tick`] call == one frame) and
/// the confirm-button edge (`confirm_pressed`) instead of this module
/// reading input itself.
///
/// # Sheet ownership
///
/// A printer *holds* its glyph source `S` for as long as it is printing, so
/// it is generic over [`GlyphSource`] rather than pinned to one sheet
/// flavour. A caller that drives a throwaway printer to completion inside
/// a single function, while the pack is still open, passes a borrowed
/// `assets::fonts::FontGlyphSheet<'_>`; a caller that keeps a printer alive
/// across frames — outliving the pack its glyphs came from — passes an
/// owned `assets::fonts::OwnedFontGlyphSheet` instead, and needs no
/// `'static` borrow, leak, or self-referential struct to do it.
#[derive(Debug)]
pub struct Printer<S> {
    tokens: Vec<Token>,
    pos: usize,
    sheet: S,
    speed: TextSpeed,
    delay_counter: u8,
    origin: PixelPos,
    cursor: PixelPos,
    line_height: i32,
    scroll_remaining: i32,
    state: PrinterState,
    can_ab_speed_up_print: bool,
    has_print_been_sped_up: bool,
}

impl<S: GlyphSource> Printer<S> {
    /// Build a printer over an already-decoded token stream (see
    /// [`super::decode`]), ready to print at `origin` (upstream
    /// `TextPrinterTemplate.x`/`.y` — e.g. `(0, 1)` for the standard
    /// overworld dialogue box, `AddTextPrinterForMessage`,
    /// `pokeemerald/src/menu.c`).
    ///
    /// Held-A/B print speed-up starts disabled — a bare, unopinionated
    /// printer, matching neither of upstream's two concrete
    /// `AddTextPrinterForMessage` argument shapes until a caller opts in
    /// (module docs' "Held-A/B print speed-up" section on why the standard
    /// field message box is one of the ones that does); see
    /// [`Printer::with_ab_speed_up_print`].
    #[must_use]
    pub fn new(tokens: Vec<Token>, sheet: S, speed: TextSpeed, origin: PixelPos) -> Self {
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
            can_ab_speed_up_print: false,
            has_print_been_sped_up: false,
        }
    }

    /// Enable upstream's held-A/B print speed-up on this printer (upstream
    /// `gTextFlags.canABSpeedUpPrint = TRUE`, `AddTextPrinterForMessage(TRUE)`
    /// — the intro's Birch speech, `pokeemerald/src/main_menu.c:1339`; module
    /// docs' "Held-A/B print speed-up" section for the exact semantics).
    /// Builder-style so the standard field message box's own call site
    /// (`pokeemerald-rs`'s `NpcDialog::new`, which does opt in — module
    /// docs) doesn't need its [`Printer::new`] call site to change shape;
    /// the other two call sites in this port (the start-menu chrome's and
    /// the main menu's fixed-label printers, both `TextSpeed::Instant` with
    /// no reveal delay for a hold to shorten) simply never call this.
    #[must_use]
    pub const fn with_ab_speed_up_print(mut self) -> Self {
        self.can_ab_speed_up_print = true;
        self
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

    /// Re-arm this printer over a fresh `tokens` stream, keeping its glyph
    /// source, speed, origin and [`Printer::with_ab_speed_up_print`]
    /// capability — the same printer starting a new message, exactly as
    /// upstream reuses one `struct TextPrinter` slot for consecutive
    /// `AddTextPrinter` calls into the same window rather than allocating a
    /// new one per message (`pokeemerald/src/text.c`).
    ///
    /// Resets every scrap of per-message state (stream position, reveal
    /// delay, cursor, in-flight scroll, render state, pause countdown, and
    /// the latched print-speed-up flag — upstream's own
    /// `subStruct->hasPrintBeenSpedUp`, zeroed fresh by every new
    /// `AddTextPrinter` call the same way the rest of `TextPrinterSubStruct`
    /// is), so the result is indistinguishable from [`Printer::new`] (plus
    /// [`Printer::with_ab_speed_up_print`] if this printer had it) with the
    /// same arguments — without needing to hand the sheet back in, which
    /// matters when the sheet is an owned one the caller cannot cheaply
    /// duplicate.
    pub fn restart(&mut self, tokens: Vec<Token>) {
        self.tokens = tokens;
        self.pos = 0;
        self.delay_counter = 0;
        self.cursor = self.origin;
        self.scroll_remaining = 0;
        self.state = PrinterState::HandleChar;
        self.has_print_been_sped_up = false;
    }

    /// Advance the printer by exactly one frame.
    ///
    /// `input.confirm_pressed()` models the confirm button's *just-pressed*
    /// edge this frame (upstream `JOY_NEW(A_BUTTON | B_BUTTON)` inside
    /// `TextPrinterWaitWithDownArrow`) — only consulted while waiting on
    /// `\l`/`\p` ([`TickEvent::AwaitingScroll`]/[`TickEvent::AwaitingClear`]);
    /// ignored otherwise, matching upstream (the equivalent upstream checks
    /// are gated on those same states). `input.confirm_held()` feeds the
    /// held-A/B print speed-up (module docs), only observable once this
    /// printer opted in via [`Printer::with_ab_speed_up_print`].
    pub fn tick(&mut self, input: PrinterInput) -> TickEvent {
        match self.state {
            PrinterState::Finished => TickEvent::Finished,
            PrinterState::HandleChar => self.tick_handle_char(input),
            PrinterState::AwaitingScroll => {
                if input.confirm_pressed() {
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
                if input.confirm_pressed() {
                    self.cursor = self.origin;
                    self.state = PrinterState::HandleChar;
                    TickEvent::Cleared
                } else {
                    TickEvent::AwaitingClear
                }
            }
            PrinterState::Pause => self.tick_pause(),
        }
    }

    /// `RENDER_STATE_PAUSE` (`text.c:1215-1220`): count `delay_counter` down
    /// to zero, one frame at a time, then resume `HandleChar` -- on the
    /// *next* tick, not this one (upstream returns `RENDER_UPDATE`, not
    /// `RENDER_REPEAT`, the frame the counter reaches zero, so nothing new
    /// prints until a further tick — the same one-tick-late shape
    /// [`TickEvent::ScrollFinished`] already has). Called both from the
    /// dispatcher above (every external tick while already paused) and
    /// directly from [`Printer::tick_handle_char`]'s own `{PAUSE n}` branch
    /// (the same-frame `RENDER_REPEAT` chain, module docs), so the first of
    /// the `n` counted-down frames lands on the tick that consumed the
    /// control code, not a tick after it.
    fn tick_pause(&mut self) -> TickEvent {
        if self.delay_counter != 0 {
            self.delay_counter -= 1;
            TickEvent::Paused
        } else {
            self.state = PrinterState::HandleChar;
            TickEvent::PauseFinished
        }
    }

    /// `RENDER_STATE_HANDLE_CHAR`: the steady-state loop. Loops internally
    /// over every `RENDER_REPEAT`-class token — newlines, no-op extended
    /// control codes, unsupported placeholder-style tokens — until it either
    /// reveals a glyph, transitions to a waiting state, or hits the
    /// terminator; each of those ends the tick.
    ///
    /// Faithful to upstream's `RENDER_STATE_HANDLE_CHAR` (`text.c:943-1017`):
    ///
    /// * `text.c:944-945` — if this printer opted into held-A/B speed-up
    ///   ([`Printer::with_ab_speed_up_print`]) and this message has already
    ///   been sped up once (`has_print_been_sped_up`), *holding* A/B forces
    ///   `delay_counter` to zero before the wait guard even runs, every loop
    ///   iteration — including `RENDER_REPEAT` re-entries within one tick.
    /// * `text.c:945-961` — the reveal-delay counter is reloaded to
    ///   [`TextSpeed::wait_frames`] (`delayCounter = textSpeed`)
    ///   *unconditionally* before consuming each token, and the
    ///   `delay_counter > 0` guard at the top of the loop — upstream's
    ///   `if (delayCounter && textSpeed)` — burns it back down one frame per
    ///   tick. While burning it down, a fresh *press* of A/B (only checked
    ///   here, not while merely held) latches `has_print_been_sped_up` and
    ///   zeroes `delay_counter` outright — the char this delay was gating
    ///   still doesn't reveal until the counter is checked again next tick
    ///   (upstream returns `RENDER_UPDATE` either way on this branch).
    /// * A `RENDER_REPEAT` token reloads the counter and loops back to the
    ///   guard, so at Slow/Mid (`wait_frames != 0`) it costs a full
    ///   reveal-delay period; at Fast/Instant (`wait_frames == 0`) the
    ///   counter stays 0 and those tokens remain free. The counter starts at
    ///   0, so the very first glyph still reveals on frame 0.
    fn tick_handle_char(&mut self, input: PrinterInput) -> TickEvent {
        loop {
            // `text.c:944-945`.
            if input.confirm_held() && self.has_print_been_sped_up {
                self.delay_counter = 0;
            }

            if self.delay_counter > 0 {
                self.delay_counter -= 1;
                // `text.c:948-952`: only while `can_ab_speed_up_print` is set
                // ([`Printer::with_ab_speed_up_print`]) does a fresh press
                // during a pending delay latch the speed-up.
                if self.can_ab_speed_up_print && input.confirm_pressed() {
                    self.has_print_been_sped_up = true;
                    self.delay_counter = 0;
                }
                return TickEvent::Idle;
            }
            // Upstream reloads `delayCounter = textSpeed` unconditionally here,
            // before consuming the next char. For a glyph this becomes the
            // reveal delay; for a RENDER_REPEAT token the `continue` returns to
            // the guard above and drains it (one idle period at Slow/Mid).
            self.delay_counter = self.speed.wait_frames();

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
                Token::ExtCtrl { sub, args } if sub == ext_ctrl::PAUSE => {
                    // `text.c:1013-1017` (`EXT_CTRL_CODE_PAUSE`): reload the
                    // delay counter to the control code's own argument, flip
                    // to `RENDER_STATE_PAUSE`, then upstream returns
                    // `RENDER_REPEAT` — which `RenderFont`'s `while` loop
                    // re-enters immediately, on this same upstream frame,
                    // landing straight in `RENDER_STATE_PAUSE` and burning
                    // the pause's first frame right here. Calling
                    // `tick_pause` directly (module docs' "State machine"
                    // section) mirrors that same-frame chain precisely.
                    self.delay_counter = args.first().copied().unwrap_or(0);
                    self.state = PrinterState::Pause;
                    return self.tick_pause();
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
            // `delay_counter` was already reloaded above, so the next glyph's
            // reveal delay is set; upstream leaves it untouched on this path.
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
        FontGlyphSheet::new(FontImageRef::new_for_tests(font, image)).unwrap()
    }

    fn decode_tokens(bytes: &[u8]) -> Vec<Token> {
        super::super::decode(bytes).unwrap()
    }

    /// A's just-pressed edge, nothing else held or pressed -- the shape
    /// every pre-existing test in this module used a bare `true` for, back
    /// when [`Printer::tick`] took a single `confirm_pressed: bool`.
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
        // FONT_NORMAL 'A' (byte 0xBB) has advance width 6 per
        // gFontNormalLatinGlyphWidths (crates/assets/src/fonts.rs).
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[0xBB, 0xBB, super::super::EOS]); // "AA"
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 1));

        let TickEvent::Glyph(first) = printer.tick(PrinterInput::none()) else {
            panic!("expected a glyph")
        };
        assert_eq!((first.x, first.y), (0, 1));
        assert_eq!(first.glyph.advance_width, 6);

        let TickEvent::Glyph(second) = printer.tick(PrinterInput::none()) else {
            panic!("expected a glyph")
        };
        assert_eq!((second.x, second.y), (6, 1));
        assert_eq!(printer.cursor(), (12, 1));
    }

    #[test]
    fn bard_word_delimiter_advances_by_the_blank_glyph_width() {
        // CHAR_BARD_WORD_DELIMIT (0x37) has no RenderText case upstream: it
        // falls through to the glyph-draw path and advances by glyph 0x37's
        // width (3px blank cell in gFontNormalLatinGlyphWidths), keeping
        // adjacent easy-chat words separated.
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[0xBB, 0x37, 0xBB, super::super::EOS]); // "A<delim>A"
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 1));

        let TickEvent::Glyph(first) = printer.tick(PrinterInput::none()) else {
            panic!("expected a glyph")
        };
        assert_eq!((first.x, first.y), (0, 1));

        let TickEvent::Glyph(delim) = printer.tick(PrinterInput::none()) else {
            panic!("expected the delimiter's blank glyph")
        };
        assert_eq!((delim.x, delim.y), (6, 1));
        assert_eq!(delim.glyph.advance_width, 3);

        let TickEvent::Glyph(second) = printer.tick(PrinterInput::none()) else {
            panic!("expected a glyph")
        };
        assert_eq!((second.x, second.y), (9, 1), "words must stay separated");
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
    fn newline_resets_x_and_advances_y_by_line_height_at_instant_speed() {
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        // "A\nA" -- at Instant (textSpeed == 0) the newline reloads a
        // zero-frame reveal delay, so it stays free: it costs no tick of its
        // own. (At Slow/Mid it would cost a reveal-delay period -- see
        // `newline_costs_a_reveal_delay_period_at_mid_speed`.)
        let tokens = decode_tokens(&[0xBB, super::super::CHAR_NEWLINE, 0xBB, super::super::EOS]);
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 1));

        let TickEvent::Glyph(first) = printer.tick(PrinterInput::none()) else {
            panic!("expected a glyph")
        };
        assert_eq!((first.x, first.y), (0, 1));

        // Second tick consumes the newline AND draws the next glyph in one
        // frame (RENDER_REPEAT with a zero-frame reload at Instant).
        let TickEvent::Glyph(second) = printer.tick(PrinterInput::none()) else {
            panic!("expected a glyph after the free newline")
        };
        assert_eq!((second.x, second.y), (0, 1 + 16));
    }

    #[test]
    fn newline_costs_a_reveal_delay_period_at_mid_speed() {
        // Upstream reloads `delayCounter = textSpeed` before consuming the
        // newline, then the RENDER_REPEAT re-entry burns it down. For "A\nB"
        // at MID (textSpeed = 3): A on frame 0, then A's 3-frame reveal delay
        // (frames 1-3), the newline consumed on frame 4 followed by 3 more
        // idle frames (frames 4-6), and B on frame 7 -- NOT frame 4.
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[0xBB, super::super::CHAR_NEWLINE, 0xBB, super::super::EOS]);
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Mid, (0, 0));

        // Frame 0: A drawn immediately (first glyph, delay_counter starts 0).
        let TickEvent::Glyph(a) = printer.tick(PrinterInput::none()) else {
            panic!("expected 'A' on frame 0")
        };
        assert_eq!((a.x, a.y), (0, 0));

        // Frames 1-6: six idle frames (A's reveal delay + the delay the
        // newline reloads). The newline is consumed on frame 4 but yields no
        // visible glyph and leaves the counter mid-drain.
        for frame in 1..=6 {
            assert_eq!(
                printer.tick(PrinterInput::none()),
                TickEvent::Idle,
                "frame {frame} should be idle"
            );
        }

        // Frame 7: B finally revealed, on the next line.
        let TickEvent::Glyph(b) = printer.tick(PrinterInput::none()) else {
            panic!("expected 'B' on frame 7")
        };
        assert_eq!((b.x, b.y), (0, 16));
    }

    #[test]
    fn reveal_cadence_matches_frames_per_char_table() {
        // MID = 4 frames/char (sTextSpeedFrameDelays, src/menu.c): a glyph
        // appears on frame 0, the next not until frame 4.
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[0xBB, 0xBB, super::super::EOS]);
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
        let tokens = decode_tokens(&[0xBB, 0xBB, 0xBB, super::super::EOS]);
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
        // SLOW = 8 frames/char -> 7 idle frames between reveals.
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[0xBB, 0xBB, super::super::EOS]);
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
        // "A\lA": draw 'A', hit \l, wait, scroll, draw 'A' again.
        let tokens = decode_tokens(&[
            0xBB,
            super::super::CHAR_PROMPT_SCROLL,
            0xBB,
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
        // Still waiting: no confirm yet.
        assert_eq!(
            printer.tick(PrinterInput::none()),
            TickEvent::AwaitingScroll
        );
        // Confirm: transitions state but does not itself move any pixels.
        assert_eq!(printer.tick(press_a()), TickEvent::ScrollStarted);
        // FAST = 4px/frame; line height 16 -> 4 frames of Scrolling{4}, then
        // ScrollFinished, then printing resumes.
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

        assert!(matches!(
            printer.tick(PrinterInput::none()),
            TickEvent::Glyph(_)
        ));
        // The free newline plus hitting \p happen in the same tick.
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
        // The `\p` char reloads `delayCounter = textSpeed` when consumed, and
        // the clear/wait states never touch it, so at MID the first glyph of
        // the new page resumes after a full 3-frame reveal delay (NOT frame 0).
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[
            0xBB,
            super::super::CHAR_PROMPT_CLEAR,
            0xBB,
            super::super::EOS,
        ]);
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Mid, (0, 0));

        // Frame 0: 'A'. Frames 1-3: A's reveal delay.
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
        // Frame 4: \p consumed -> AwaitingClear (reloads delay_counter = 3).
        assert_eq!(printer.tick(PrinterInput::none()), TickEvent::AwaitingClear);
        // Frame 5: still waiting for confirm.
        assert_eq!(printer.tick(PrinterInput::none()), TickEvent::AwaitingClear);
        // Frame 6: confirm clears the page on the same tick.
        assert_eq!(printer.tick(press_a()), TickEvent::Cleared);
        assert_eq!(printer.cursor(), (0, 0));
        // Frames 7-9: the reveal delay reloaded by \p drains -- resume is NOT
        // immediate at MID.
        for frame in 7..=9 {
            assert_eq!(
                printer.tick(PrinterInput::none()),
                TickEvent::Idle,
                "resume frame {frame} should be idle"
            );
        }
        // Frame 10: the new page's first glyph.
        let TickEvent::Glyph(b) = printer.tick(PrinterInput::none()) else {
            panic!("expected the new page's glyph on frame 10")
        };
        assert_eq!((b.x, b.y), (0, 0));
    }

    #[test]
    fn scroll_resumes_after_a_reveal_delay_at_mid_speed() {
        // Same reload semantics for `\l`: the scroll animation leaves
        // delay_counter at textSpeed, so the first glyph after the scroll
        // waits out a reveal-delay period at MID.
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[
            0xBB,
            super::super::CHAR_PROMPT_SCROLL,
            0xBB,
            super::super::EOS,
        ]);
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Mid, (0, 1));

        // 'A', then its 3-frame reveal delay.
        assert!(matches!(
            printer.tick(PrinterInput::none()),
            TickEvent::Glyph(_)
        ));
        for _ in 0..3 {
            assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Idle);
        }
        // \l consumed -> AwaitingScroll (reloads delay_counter = 3).
        assert_eq!(
            printer.tick(PrinterInput::none()),
            TickEvent::AwaitingScroll
        );
        assert_eq!(
            printer.tick(PrinterInput::none()),
            TickEvent::AwaitingScroll
        );
        assert_eq!(printer.tick(press_a()), TickEvent::ScrollStarted);
        // MID scrolls 2px/frame over the 16px line height -> 8 scroll frames.
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
        // The reveal delay reloaded when \l was consumed still has to drain:
        // 3 idle frames, THEN the next glyph.
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
        let tokens = decode_tokens(&[0xBB, super::super::EOS]);
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 0));

        assert!(matches!(
            printer.tick(PrinterInput::none()),
            TickEvent::Glyph(_)
        ));
        assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Finished);
        assert!(printer.is_finished());
        // Further ticks stay Finished, never panic on the exhausted stream.
        assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Finished);
    }

    #[test]
    fn extra_symbol_addresses_the_extended_glyph_page() {
        // CHAR_EXTRA_SYMBOL (0xF9) 0x00 -> glyph id 0x100, same as upstream's
        // `currChar | 0x100`.
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[super::super::CHAR_EXTRA_SYMBOL, 0x00, super::super::EOS]);
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 0));

        let TickEvent::Glyph(g) = printer.tick(PrinterInput::none()) else {
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

        let TickEvent::Glyph(g) = printer.tick(PrinterInput::none()) else {
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

        let TickEvent::Glyph(g) = printer.tick(PrinterInput::none()) else {
            panic!("expected a glyph")
        };
        assert_eq!((g.x, g.y), (20, 1));
    }

    /// [`Printer::restart`] must leave the printer exactly as a freshly
    /// built one over the new stream — same glyphs, same cursor, same
    /// non-finished state — no leftover position, delay or scroll from the
    /// message it just finished.
    #[test]
    fn restart_reprints_a_new_stream_from_the_top_keeping_the_sheet() {
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[0xBB, super::super::EOS]); // "A"
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

        printer.restart(decode_tokens(&[0xBB, 0xBB, super::super::EOS])); // "AA"
        assert!(!printer.is_finished(), "a restarted printer prints again");
        assert_eq!(printer.cursor(), (3, 1), "the cursor is back at the origin");

        let TickEvent::Glyph(first) = printer.tick(PrinterInput::none()) else {
            panic!("expected the restarted stream's first glyph")
        };
        assert_eq!((first.x, first.y), (3, 1));
        let TickEvent::Glyph(second) = printer.tick(PrinterInput::none()) else {
            panic!("expected the restarted stream's second glyph")
        };
        assert_eq!((second.x, second.y), (9, 1), "advanced by 'A''s 6px width");
        assert!(matches!(
            printer.tick(PrinterInput::none()),
            TickEvent::Finished
        ));
    }

    /// An owned glyph source drives the printer identically to a borrowed
    /// one (the whole point of the [`GlyphSource`] seam — see the
    /// [`Printer`] docs' "Sheet ownership"): a printer over an owned sheet
    /// stays usable after the buffer the sheet was copied from is gone.
    #[test]
    fn a_printer_over_an_owned_sheet_outlives_the_pack_bytes_it_came_from() {
        let mut printer = {
            let pixels = blank_sheet_pixels();
            let owned = synthetic_sheet(&pixels, FontId::Normal).to_owned_sheet();
            Printer::new(
                decode_tokens(&[0xBB, super::super::EOS]),
                owned,
                TextSpeed::Instant,
                (0, 1),
            )
        };

        let TickEvent::Glyph(g) = printer.tick(PrinterInput::none()) else {
            panic!("expected a glyph from the owned sheet")
        };
        assert_eq!((g.x, g.y), (0, 1));
        assert_eq!(g.glyph.advance_width, 6);
    }

    /// Issue #393: either A or B must advance a `\p`/`\l` wait identically --
    /// upstream never distinguishes which button did it
    /// (`JOY_NEW(A_BUTTON | B_BUTTON)`, `text.c:874-879`). Drives the same
    /// `AwaitingClear` wait with `b_pressed` alone (no `a_pressed`) and
    /// confirms it clears exactly like [`prompt_clear_waits_for_confirm_then_resets_the_cursor_on_page`]'s
    /// `a_pressed`-only case does.
    #[test]
    fn b_alone_advances_a_prompt_clear_exactly_like_a_alone() {
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[
            0xBB,
            super::super::CHAR_PROMPT_CLEAR,
            0xBB,
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

    /// Issue #393: `{PAUSE n}` must block for *exactly* `n` frames -- not
    /// `n + 1` (the off-by-one this module's own "State machine" docs warn
    /// about, `text.c:1013-1017`'s same-frame `RENDER_REPEAT` chain into
    /// `RENDER_STATE_PAUSE`, `text.c:1215-1220`). Runs at `Instant` speed so
    /// every glyph is frame-free and the only source of `Idle`-shaped ticks
    /// in this stream is the pause itself, isolating its own count exactly.
    #[test]
    fn pause_control_code_blocks_for_exactly_the_argument_frame_count() {
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[
            0xBB, // 'A'
            super::super::EXT_CTRL_CODE_BEGIN,
            super::super::EXT_CTRL_CODE_PAUSE,
            96,
            0xBB, // 'B'
            super::super::EOS,
        ]);
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 1));

        assert!(
            matches!(printer.tick(PrinterInput::none()), TickEvent::Glyph(_)),
            "'A' reveals before the pause"
        );

        // The pause control code is consumed on the very next tick, and
        // upstream's same-frame `RENDER_REPEAT` chain means that tick is
        // *already* the pause's first counted frame -- 96 `Paused` ticks
        // total, this one included.
        let mut paused_ticks = 0;
        loop {
            match printer.tick(PrinterInput::none()) {
                TickEvent::Paused => paused_ticks += 1,
                TickEvent::PauseFinished => break,
                other => panic!("expected Paused or PauseFinished, got {other:?}"),
            }
        }
        assert_eq!(
            paused_ticks, 96,
            "{{PAUSE 96}} must block for exactly 96 frames, not 95 or 97"
        );

        // Printing does not resume on the same tick `PauseFinished` was
        // returned -- one more tick, matching `ScrollFinished`'s identical
        // one-tick-late shape.
        let TickEvent::Glyph(b) = printer.tick(PrinterInput::none()) else {
            panic!("expected 'B' to resume the tick after PauseFinished")
        };
        assert_eq!((b.x, b.y), (6, 1), "advanced past 'A''s 6px width");
    }

    /// Issue #393: held-A/B print speed-up only applies to a printer that
    /// opted in via [`Printer::with_ab_speed_up_print`] -- a bare
    /// [`Printer::new`] with no such opt-in must keep printing at its own
    /// `TextSpeed` regardless of how long A/B is held. This is a property of
    /// the flag itself, not a claim about which of this port's call sites
    /// leave it unset -- module docs' "Held-A/B print speed-up" section for
    /// which ones actually do (the standard field message box is not one of
    /// them, matching upstream's own `AddTextPrinterForMessage(TRUE)`).
    #[test]
    fn held_ab_speed_up_only_affects_printers_that_opt_in() {
        let pixels = blank_sheet_pixels();

        // Three glyphs at MID (4 frames/char): without speed-up, 'B' and 'C'
        // only reveal after their own full reveal-delay periods, no matter
        // what is held.
        let opted_out_sheet = synthetic_sheet(&pixels, FontId::Normal);
        let opted_out_tokens = decode_tokens(&[0xBB, 0xBB, 0xBB, super::super::EOS]);
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
            "'A' reveals on frame 0 regardless"
        );
        // Continuing to hold A must NOT accelerate this printer: three idle
        // frames (MID's own wait_frames), matching
        // `reveal_cadence_matches_frames_per_char_table`'s un-accelerated
        // cadence exactly, even though A is held (and was freshly pressed)
        // throughout.
        for frame in 1..=3 {
            assert_eq!(
                opted_out_printer.tick(held_a),
                TickEvent::Idle,
                "an opted-out printer's frame {frame} must stay un-accelerated"
            );
        }
        assert!(
            matches!(opted_out_printer.tick(held_a), TickEvent::Glyph(_)),
            "'B' still waits out the full MID delay"
        );

        // The intro's own printer, same tokens/speed, opted into speed-up:
        // pressing A while a delay is pending latches the speed-up and
        // zeroes that delay outright.
        let intro_sheet = synthetic_sheet(&pixels, FontId::Normal);
        let intro_tokens = decode_tokens(&[0xBB, 0xBB, 0xBB, super::super::EOS]);
        let mut intro_printer = Printer::new(intro_tokens, intro_sheet, TextSpeed::Mid, (0, 0))
            .with_ab_speed_up_print();

        assert!(
            matches!(
                intro_printer.tick(PrinterInput::none()),
                TickEvent::Glyph(_)
            ),
            "'A' still reveals on frame 0"
        );
        // A fresh press while the delay is pending latches the speed-up and
        // zeroes the counter on this same tick (still no glyph this tick --
        // upstream returns RENDER_UPDATE either way here).
        assert_eq!(intro_printer.tick(held_a), TickEvent::Idle);
        // The very next tick already reveals 'B' -- the latch fired.
        assert!(
            matches!(
                intro_printer.tick(PrinterInput::none()),
                TickEvent::Glyph(_)
            ),
            "'B' reveals early once the speed-up latches"
        );
        // With the latch set, merely holding A (no fresh press needed this
        // time) forces 'C''s delay to zero too.
        assert!(
            matches!(intro_printer.tick(held_a), TickEvent::Glyph(_)),
            "'C' reveals immediately: held A alone, once already sped up"
        );
    }

    /// Issue #393: [`Printer::restart`] must clear a printer stuck mid-pause
    /// -- upstream's own `RENDER_STATE_PAUSE`/`delayCounter` do not survive
    /// a fresh `AddTextPrinter` call into the reused window slot.
    #[test]
    fn restart_clears_a_stuck_pause_state() {
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);

        // Instant speed: the only source of a non-`Glyph`/`Finished` tick in
        // this stream is the pause itself (no reveal-delay idle frames to
        // account for), so five ticks land squarely mid-pause.
        let tokens = decode_tokens(&[
            super::super::EXT_CTRL_CODE_BEGIN,
            super::super::EXT_CTRL_CODE_PAUSE,
            96,
            0xBB,
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

        printer.restart(decode_tokens(&[0xBB, super::super::EOS])); // "A"
        assert!(!printer.is_finished());
        assert_eq!(printer.cursor(), (0, 0));

        // The pause state is gone: the restarted stream's first glyph
        // reveals immediately, not another `Paused`/`PauseFinished`.
        assert!(
            matches!(printer.tick(PrinterInput::none()), TickEvent::Glyph(_)),
            "restart must not leave the printer stuck mid-pause"
        );
    }

    /// Issue #393: [`Printer::restart`] must clear the latched print-speed-up
    /// flag -- upstream's own `subStruct->hasPrintBeenSpedUp` does not
    /// survive a fresh `AddTextPrinter` call into the reused window slot
    /// either, so a later message on the same printer pays the full
    /// reveal-delay period again until it earns its own speed-up.
    #[test]
    fn restart_clears_the_latched_speed_up_flag() {
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[0xBB, 0xBB, super::super::EOS]); // "AA"
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
        )); // 'A'
        assert_eq!(printer.tick(press_and_hold_a), TickEvent::Idle); // latches the speed-up
        assert!(matches!(
            printer.tick(PrinterInput::none()),
            TickEvent::Glyph(_)
        )); // second 'A', sped up

        printer.restart(decode_tokens(&[0xBB, 0xBB, super::super::EOS])); // "AA"
        assert!(matches!(
            printer.tick(PrinterInput::none()),
            TickEvent::Glyph(_)
        )); // 'A'

        // The latch is gone: merely holding A (with no fresh press on this
        // restarted stream) must NOT force the second glyph's delay to zero
        // -- the full MID reveal-delay period must be paid again, exactly as
        // a freshly-built printer would.
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
