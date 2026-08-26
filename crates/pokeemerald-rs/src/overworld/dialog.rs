//! NPC dialog (I-3, issue #161): a single message box, opened when the
//! player interacts with a facing object event whose script
//! [`super::npc_scripts::script_text`] recognizes, drawn over the
//! already-composed overworld frame and closed once the player confirms
//! through it.
//!
//! Reuses the exact same [`Printer`] + standard-dialogue-box compositing
//! path [`crate::intro::IntroScene`] already established (issue #149) --
//! same font/window pack entries, same [`MessageBoxLayout::STANDARD`]
//! geometry, same "one `Printer::tick` per frame, `confirm_pressed` only
//! consulted while awaiting a scroll/clear (or, past `Token::End`, the
//! script-level button wait below)" contract -- just single-message (no
//! paging) and composited on top of a live overworld frame instead of a
//! blank black one, since [`MessageBoxLayout::STANDARD`]'s own frame tiles
//! already include their interior fill (`crate::textbox::blit_frame_tiles`'s
//! own "last write wins" docs), so nothing behind the box needs erasing
//! first.
//!
//! # Held-A/B print speed-up (issue #393)
//!
//! This *is* upstream's standard field message box (`ShowFieldMessage`/
//! `ShowFieldMessageFromBuffer`, both routing through
//! `AddTextPrinterForMessage(TRUE)` --
//! `pokeemerald/src/field_message_box.c:62-69,109-129`), so it opts into
//! held-A/B print speed-up the same way [`crate::intro::IntroScene`] does --
//! [`Printer::with_ab_speed_up_print`], built in [`NpcDialog::new`]. An
//! earlier revision of this module left it off and claimed that matched
//! upstream's own default; it did not -- `InitFieldMessageBox`'s
//! `gTextFlags.canABSpeedUpPrint = FALSE` (`field_message_box.c:17`) is only
//! the dormant-box reset, overwritten `TRUE` the instant either function
//! above actually shows a message. [`engine::text::render`]'s own
//! "Held-A/B print speed-up" module docs have the exact latch semantics.
//!
//! # Script-level `waitbuttonpress` (issue #410)
//!
//! Upstream's `waitbuttonpress` script command
//! (`ScrCmd_waitbuttonpress`/`WaitForAorBPress`,
//! `pokeemerald/src/scrcmd.c:1323-1336`) polls `JOY_NEW(A_BUTTON |
//! B_BUTTON)` and nothing else -- no window state, no printer state, no
//! clear. `Std_MsgboxNPC`/`Std_MsgboxDefault`/`Std_MsgboxSign`
//! (`pokeemerald/data/scripts/std_msgbox.inc:1-22`) all reach it only
//! *after* `waitmessage` has already confirmed the printer itself finished,
//! and none of them clear the box before `return`/`release(all)`. That is a
//! script-level wait, not a text control code, so it does not belong in
//! [`Printer`]/[`TickEvent`] alongside `\l`/`\p` -- [`NpcDialog::tick`]
//! models it as a second, later gate a message can opt into with
//! [`NpcDialog::with_waitbuttonpress`] (applied automatically by
//! [`NpcDialog::from_pack`]/[`NpcDialog::open_default`], the two
//! constructors that build a real field NPC's dialog): once
//! [`TickEvent::Finished`] is reached, the box holds its last frame of text
//! exactly as printed -- no [`TickEvent::Cleared`], no post-clear reveal
//! delay -- until a confirm edge lands, closing on that same tick.
//!
//! Before this, the only way a caller could hold a finished box open for a
//! confirm press was to append a synthetic trailing `{P}`
//! ([`engine::text::Token::PromptClear`]) to the text itself -- which
//! *does* clear the box (`TickEvent::Cleared`) and then pays a full
//! reveal-delay period before `Token::End` is finally reached
//! ([`Printer`]'s own module docs), showing several blank-box frames
//! upstream never does. [`Printer`] is untouched by this fix: a genuine
//! mid-message `{P}` (a real page break, more text follows) keeps its exact
//! existing wait-then-clear-then-resume behaviour -- only a *trailing* one
//! used purely to fake a wait was ever wrong, and no caller does that
//! anymore ([`super::npc_scripts::script_text`]'s Mom message, issue #410).

use assets::fonts::{FontId, OwnedFontGlyphSheet};
use assets::pack::{AssetPack, PackError};
use engine::text::render::{Printer, PrinterInput, RevealedGlyph, TextSpeed, TickEvent};
use engine::text::window::MessageBoxLayout;
use engine::text::Token;
use platform::{ButtonState, Buttons};
use rendering::Framebuffer;

use crate::textbox::{self, FrameAssets};

/// Narrow a frame's real [`ButtonState`] down to the four bits
/// [`NpcDialog::tick`] needs (same seam `crate::flow::intro_printer_input`
/// crosses for [`crate::intro::IntroScene::tick`], and the same shape: A and
/// B both count, pressed or held alike -- neither
/// `TextPrinterWaitWithDownArrow`'s confirm-edge wait nor
/// `RENDER_STATE_HANDLE_CHAR`'s held speed-up distinguish which button did
/// it, so this doesn't either). Shared by every caller that ticks an
/// [`NpcDialog`] with real input -- `OverworldPhase::advance_dialog_frame`
/// for NPC/field dialogs and `SaveDialog::run` for `ShowSaveMessage`'s own
/// box (both `pub(super)`/crate-private to their own modules, so no
/// intra-doc link reaches either from here) -- so the two don't drift.
pub(crate) fn confirm_printer_input(buttons: ButtonState) -> PrinterInput {
    PrinterInput {
        a_pressed: buttons.is_newly_pressed(Buttons::A),
        b_pressed: buttons.is_newly_pressed(Buttons::B),
        a_held: buttons.is_held(Buttons::A),
        b_held: buttons.is_held(Buttons::B),
    }
}

/// The dialogue box's window-local text origin -- mirrors
/// [`crate::intro`]'s identical constant for the same standard field message
/// box.
const PRINTER_ORIGIN: (i32, i32) = (2, 2);

/// [`MessageBoxLayout::STANDARD`]'s content rect, converted to absolute
/// screen pixels, for [`textbox::blit_glyphs`]'s `origin` -- mirrors
/// [`crate::intro`]'s identical constant.
const BOX_SCREEN_ORIGIN: (i32, i32) = (
    engine::text::window::STANDARD_TILEMAP_LEFT * 8,
    engine::text::window::STANDARD_TILEMAP_TOP * 8,
);

/// [`MessageBoxLayout::STANDARD`]'s content rect size in pixels, for
/// [`textbox::blit_glyphs`]'s `content_size` -- mirrors [`crate::intro`]'s
/// identical constant.
const BOX_CONTENT_SIZE_PX: (i32, i32) = (
    engine::text::window::STANDARD_CONTENT_WIDTH * 8,
    engine::text::window::STANDARD_CONTENT_HEIGHT * 8,
);

/// Why opening an [`NpcDialog`] failed.
///
/// Concrete per-crate-boundary enum `(oop-boundaries)`, mirroring
/// [`crate::intro::IntroSceneError`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NpcDialogError {
    /// Loading or reading the asset pack failed -- most commonly
    /// [`PackError::NotFound`].
    Pack(PackError),
    /// The font glyph sheet fetched from the pack didn't decode.
    Font(assets::AssetError),
}

impl std::fmt::Display for NpcDialogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pack(err) => write!(f, "npc dialog: {err}"),
            Self::Font(err) => write!(f, "npc dialog: {err}"),
        }
    }
}

impl std::error::Error for NpcDialogError {}

impl From<PackError> for NpcDialogError {
    fn from(err: PackError) -> Self {
        Self::Pack(err)
    }
}

impl From<assets::AssetError> for NpcDialogError {
    fn from(err: assets::AssetError) -> Self {
        Self::Font(err)
    }
}

/// What [`NpcDialog::tick`] did this frame -- whether the caller should keep
/// routing input to the dialog, or the message finished and control should
/// return to ordinary overworld movement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DialogOutcome {
    /// The message is still printing, or awaiting a confirm press.
    Continue,
    /// The message finished (the terminating `Token::End` was reached, the
    /// tick after the trailing `{P}`'s confirm cleared the box -- see
    /// module docs) -- the caller should drop this dialog and resume
    /// ordinary overworld control.
    Closed,
}

/// One open NPC message box (module docs).
#[derive(Debug)]
pub(crate) struct NpcDialog {
    frame: FrameAssets,
    printer: Printer<OwnedFontGlyphSheet>,
    revealed: Vec<RevealedGlyph>,
    finished: bool,
    /// Set by [`Self::with_waitbuttonpress`]: once [`TickEvent::Finished`]
    /// is reached, hold the box open for one more confirm edge instead of
    /// closing outright (module docs' "Script-level `waitbuttonpress`"
    /// section).
    waitbuttonpress: bool,
    /// `true` once the printer has reached [`TickEvent::Finished`] while
    /// [`Self::waitbuttonpress`] is set and [`Self::finished`] isn't yet --
    /// [`Self::tick`] is now waiting on a confirm edge alone, without
    /// calling [`Printer::tick`] again (so no [`TickEvent::Cleared`], no
    /// post-clear reveal delay).
    awaiting_button_press: bool,
}

impl NpcDialog {
    /// Build a dialog over an already-decoded font `sheet` and dialogue
    /// `frame`, printing `tokens` at [`TextSpeed::Mid`] (upstream's own
    /// new-game default -- see [`crate::intro::IntroScene::from_pack`]'s
    /// identical doc comment).
    ///
    /// `pub(crate)` because this box is not only an NPC's: upstream's
    /// standard field message window is a single window
    /// (`sStandardTextBox_WindowTemplates[0]`, `src/menu.c:84-96`) that
    /// every field message prints into, and the start menu's save flow
    /// (`ShowSaveMessage`, `src/start_menu.c:902-909`) prints into that same
    /// one. [`crate::start_menu`] already holds a decoded sheet and
    /// message-box frame, so it builds boxes here directly instead of
    /// re-reading the pack once per message.
    ///
    /// Opts into held-A/B print speed-up (module docs' "Held-A/B print
    /// speed-up" section): every caller of this constructor is one of
    /// upstream's `AddTextPrinterForMessage(TRUE)` sites, so every box built
    /// here -- an NPC's or `ShowSaveMessage`'s alike -- gets it.
    pub(crate) fn new(sheet: OwnedFontGlyphSheet, frame: FrameAssets, tokens: Vec<Token>) -> Self {
        let printer =
            Printer::new(tokens, sheet, TextSpeed::Mid, PRINTER_ORIGIN).with_ab_speed_up_print();
        Self {
            frame,
            printer,
            revealed: Vec::new(),
            finished: false,
            waitbuttonpress: false,
            awaiting_button_press: false,
        }
    }

    /// Opt into upstream's `waitbuttonpress` script command (module docs'
    /// "Script-level `waitbuttonpress`" section, issue #410): once
    /// [`Self::tick`] reaches [`TickEvent::Finished`], hold the box open --
    /// final text untouched, no [`TickEvent::Cleared`], no post-clear
    /// reveal delay -- for one more confirm edge before actually closing.
    /// Builder-style, like [`Printer::with_ab_speed_up_print`]: every
    /// [`Self::from_pack`] call already applies it, since every real field
    /// NPC message this port opens ends its upstream script with exactly
    /// this command (`Std_MsgboxNPC`/`Std_MsgboxDefault`/`Std_MsgboxSign`,
    /// `pokeemerald/data/scripts/std_msgbox.inc:1-22`).
    #[must_use]
    pub(crate) const fn with_waitbuttonpress(mut self) -> Self {
        self.waitbuttonpress = true;
        self
    }

    /// Copy the dialog's two required pack entries (the normal-weight font
    /// sheet and the dialogue frame) out of an already-loaded `pack` and
    /// open a dialog printing `tokens` -- mirrors
    /// [`crate::intro::IntroScene::from_pack`].
    ///
    /// Opts into [`Self::with_waitbuttonpress`] (that method's own doc
    /// comment): this is the constructor real field NPC scripts open
    /// through ([`Self::open_default`], `crate::flow::overworld_phase`'s own
    /// A-press interaction path), and every one of them ends with upstream's
    /// `waitbuttonpress`, not an auto-close.
    ///
    /// # Errors
    ///
    /// [`NpcDialogError::Pack`] if `pack` is missing its `font/normal/glyphs`
    /// or message-box entries (or either is malformed);
    /// [`NpcDialogError::Font`] if the font sheet doesn't decode.
    pub(crate) fn from_pack(pack: &AssetPack, tokens: Vec<Token>) -> Result<Self, NpcDialogError> {
        let sheet = OwnedFontGlyphSheet::new(pack.font(FontId::Normal)?)?;
        let frame = FrameAssets::from_handle(pack.message_box()?);
        Ok(Self::new(sheet, frame, tokens).with_waitbuttonpress())
    }

    /// Load the pack from its default location and open a dialog printing
    /// `tokens` -- mirrors [`crate::intro::load_default`]. Reads from disk
    /// on every call, by design (module docs on [`crate::intro::IntroScene`]'s
    /// identical "owns every byte it renders" shape): a dialog only ever
    /// opens for the single frame the player presses A facing an NPC, so the
    /// small extra pack read is not a per-frame cost.
    ///
    /// # Errors
    ///
    /// [`NpcDialogError::Pack`] if no pack has been extracted yet, or is
    /// missing the entries [`Self::from_pack`] needs;
    /// [`NpcDialogError::Font`] if the font sheet doesn't decode.
    pub(crate) fn open_default(tokens: Vec<Token>) -> Result<Self, NpcDialogError> {
        let pack = AssetPack::load_default()?;
        Self::from_pack(&pack, tokens)
    }

    /// How many glyphs are currently visible on screen -- mirrors
    /// [`crate::intro::IntroScene::revealed_glyph_count`]'s identical
    /// test-facing accessor. `#[cfg(test)]`-only (unlike that one): `NpcDialog`
    /// itself is `pub(crate)`, not exported outside this crate the way
    /// `IntroScene` is, so nothing but this crate's own tests -- see
    /// `crate::flow::overworld_phase`'s real-pack dialog test -- could ever
    /// call it; [`Self::compose_over`] is the production consumer of the
    /// same underlying state.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn revealed_glyph_count(&self) -> usize {
        self.revealed.len()
    }

    /// Advance the dialog by exactly one frame. `input`'s pressed/held A and
    /// B bits are forwarded straight to the current [`Printer::tick`] --
    /// both the confirm edge it consults while awaiting a scroll/clear (that
    /// method's own doc comment) and, since this box opted into held-A/B
    /// print speed-up ([`Self::new`]), the press/hold pair its
    /// [`engine::text::render`] module docs describe. Upstream's message box
    /// never distinguishes which button did either
    /// (`TextPrinterWaitWithDownArrow`'s `JOY_NEW(A_BUTTON | B_BUTTON)`,
    /// `src/text.c:865-882`, and `RENDER_STATE_HANDLE_CHAR`'s
    /// `JOY_HELD(A_BUTTON | B_BUTTON)`, `:943-953`), so callers build `input`
    /// with [`confirm_printer_input`] rather than picking one button.
    ///
    /// Once [`Self::waitbuttonpress`] is set and the printer has reached
    /// [`TickEvent::Finished`], `input`'s confirm edge is consulted directly
    /// here instead (module docs' "Script-level `waitbuttonpress`" section):
    /// [`Printer::tick`] is not called again, so the box's last printed
    /// frame stays exactly as it was -- no [`TickEvent::Cleared`], no
    /// post-clear reveal delay -- until that edge lands, closing on the
    /// very same tick.
    pub(crate) fn tick(&mut self, input: PrinterInput) -> DialogOutcome {
        if self.finished {
            return DialogOutcome::Closed;
        }
        if self.awaiting_button_press {
            if input.confirm_pressed() {
                self.finished = true;
            }
            return if self.finished {
                DialogOutcome::Closed
            } else {
                DialogOutcome::Continue
            };
        }
        match self.printer.tick(input) {
            TickEvent::Glyph(g) => self.revealed.push(*g),
            TickEvent::Cleared => self.revealed.clear(),
            TickEvent::Scrolling { dy } => {
                for g in &mut self.revealed {
                    g.y -= dy;
                }
            }
            TickEvent::Finished => {
                if self.waitbuttonpress {
                    self.awaiting_button_press = true;
                } else {
                    self.finished = true;
                }
            }
            TickEvent::Idle
            | TickEvent::AwaitingScroll
            | TickEvent::ScrollStarted
            | TickEvent::ScrollFinished
            | TickEvent::AwaitingClear
            | TickEvent::Paused
            | TickEvent::PauseFinished => {}
        }

        if self.finished {
            DialogOutcome::Closed
        } else {
            DialogOutcome::Continue
        }
    }

    /// Composite this dialog's box and every currently-revealed glyph on top
    /// of `base` (an already-composed overworld frame -- see the module
    /// docs on why no interior-erase pass is needed first).
    #[must_use]
    pub(crate) fn compose_over(&self, mut base: Framebuffer) -> Framebuffer {
        let tiles = MessageBoxLayout::STANDARD.frame_tiles();
        textbox::blit_frame_tiles(&mut base, &tiles, self.frame.image(), &self.frame.palette);
        textbox::blit_glyphs(
            &mut base,
            &self.revealed,
            BOX_SCREEN_ORIGIN,
            BOX_CONTENT_SIZE_PX,
        );
        base
    }
}

/// A dialog over a blank glyph sheet plus a blank dialogue frame, with no
/// local pack needed -- mirrors [`crate::intro::synthetic_finished_scene`]'s
/// identical fixture shape.
///
/// `pub(crate)`: `crate::flow::overworld_phase`'s own headless tests need an
/// *open* dialog to prove `OverworldPhase::step` freezes movement while one
/// is up, and [`NpcDialog::new`] is private to this module.
#[cfg(test)]
pub(crate) fn synthetic_dialog(tokens: Vec<Token>) -> NpcDialog {
    use assets::fonts::FontImageRef;
    use assets::pack::ImageRef;
    use rendering::Rgb888;

    const SHEET_WIDTH: u32 = 256;
    const SHEET_HEIGHT: u32 = 512;

    let pixels = vec![0u8; (SHEET_WIDTH * SHEET_HEIGHT) as usize];
    let image = ImageRef {
        width: SHEET_WIDTH,
        height: SHEET_HEIGHT,
        bit_depth: 2,
        pixels: &pixels,
    };
    let sheet = OwnedFontGlyphSheet::new(FontImageRef::new_for_tests(FontId::Normal, image))
        .expect("this is the exact real glyph-sheet shape");
    let frame = FrameAssets {
        pixels: vec![0u8; 56 * 16],
        width: 56,
        height: 16,
        palette: vec![Rgb888::BLACK; 16],
    };
    NpcDialog::new(sheet, frame, tokens)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rendering::Rgb888;

    /// No buttons pressed or held this frame -- shorthand for the many
    /// no-input ticks these tests drive, mirroring
    /// [`crate::intro::tests`]'s identical `NONE` constant.
    const NONE: PrinterInput = PrinterInput {
        a_pressed: false,
        b_pressed: false,
        a_held: false,
        b_held: false,
    };

    /// A one-frame confirm press, exactly as [`confirm_printer_input`]
    /// produces it on the frame a button goes down: `a_pressed` and
    /// `a_held` both true (mirrors [`crate::intro::tests`]'s identical
    /// `CONFIRM` constant and its own doc comment on why the real input
    /// shape sets both).
    const CONFIRM: PrinterInput = PrinterInput {
        a_pressed: true,
        b_pressed: false,
        a_held: true,
        b_held: false,
    };

    /// Still held, one frame after [`CONFIRM`] -- the edge is gone
    /// (`a_pressed: false`) but the button has not been released.
    const HELD: PrinterInput = PrinterInput {
        a_pressed: false,
        b_pressed: false,
        a_held: true,
        b_held: false,
    };

    #[test]
    fn tick_reveals_glyphs_and_stays_open_while_printing() {
        let mut dialog = synthetic_dialog(vec![Token::Char('H'), Token::Char('i'), Token::End]);
        // TextSpeed::Mid: a glyph every 4th frame, starting at frame 0.
        assert_eq!(dialog.tick(NONE), DialogOutcome::Continue);
        assert_eq!(dialog.revealed.len(), 1);
    }

    #[test]
    fn a_message_without_a_trailing_prompt_closes_the_instant_printing_finishes() {
        let mut dialog = synthetic_dialog(vec![Token::Char('A'), Token::End]);
        for _ in 0..8 {
            if dialog.tick(NONE) == DialogOutcome::Closed {
                return;
            }
        }
        panic!("a short, un-prompted message must close on its own");
    }

    #[test]
    fn a_trailing_prompt_clear_waits_for_confirm_then_closes_on_the_next_tick() {
        let mut dialog = synthetic_dialog(vec![Token::Char('A'), Token::PromptClear, Token::End]);
        // Drain the reveal delay to reach AwaitingClear.
        for _ in 0..8 {
            assert!(
                dialog.tick(NONE) != DialogOutcome::Closed,
                "must not close before a confirm press reaches the trailing prompt"
            );
        }
        // Confirm: clears the box (`Cleared`, not yet closed -- `Printer`
        // reloaded a reveal-delay counter when `PromptClear` was consumed,
        // so a few more `Idle` ticks drain before `Token::End` is finally
        // reached, mirroring `engine::text::render`'s own
        // `page_clear_resumes_after_a_reveal_delay_at_mid_speed` test).
        assert_eq!(
            dialog.tick(CONFIRM),
            DialogOutcome::Continue,
            "Cleared, not yet Closed"
        );
        for _ in 0..8 {
            if dialog.tick(NONE) == DialogOutcome::Closed {
                return;
            }
        }
        panic!("must close once the post-clear reveal delay drains and Token::End is reached");
    }

    /// Issue #410: a dialog opted into [`NpcDialog::with_waitbuttonpress`]
    /// holds its final printed frame on screen -- unlike the old
    /// trailing-`{P}` trick (`a_trailing_prompt_clear_waits_for_confirm_then_closes_on_the_next_tick`
    /// just above), nothing is ever cleared, so the glyph count never drops
    /// before the dialog closes, and closing itself needs only the one
    /// confirm tick -- no extra reveal-delay drain afterward.
    #[test]
    fn waitbuttonpress_holds_every_glyph_until_confirm_then_closes_on_that_same_tick() {
        let mut dialog = synthetic_dialog(vec![Token::Char('H'), Token::Char('i'), Token::End])
            .with_waitbuttonpress();

        // Print both glyphs, never closing on its own.
        let mut fully_printed = false;
        for _ in 0..16 {
            assert_eq!(
                dialog.tick(NONE),
                DialogOutcome::Continue,
                "must not close on its own once printing finishes -- \
                 waitbuttonpress needs an explicit confirm"
            );
            if dialog.revealed_glyph_count() == 2 {
                fully_printed = true;
                break;
            }
        }
        assert!(fully_printed, "both glyphs must have printed");

        // Idle a while longer, past the point a genuine `{P}` would have
        // long since cleared and resumed: the glyph count must never move.
        for _ in 0..8 {
            assert_eq!(dialog.tick(NONE), DialogOutcome::Continue);
            assert_eq!(
                dialog.revealed_glyph_count(),
                2,
                "text must stay on screen while awaiting the confirm press -- \
                 no Cleared, ever, on this path"
            );
        }

        // Confirm: closes immediately, same tick -- no intervening
        // Cleared-then-drain tick the old {P} trick needed.
        assert_eq!(
            dialog.tick(CONFIRM),
            DialogOutcome::Closed,
            "a confirm edge must close the dialog on this exact tick"
        );
        assert_eq!(
            dialog.revealed_glyph_count(),
            2,
            "the box must close with its text intact, never blanked first"
        );
    }

    /// Issue #410, upstream `WaitForAorBPress`
    /// (`pokeemerald/src/scrcmd.c:1323-1330`): either button's fresh edge
    /// satisfies the script-level wait, mirroring
    /// [`Printer`]'s own `\l`/`\p` confirm handling
    /// (`crate::overworld::dialog::tests` -- see this module's B-alone
    /// coverage at the `flow::overworld_phase` level for the full wiring).
    #[test]
    fn waitbuttonpress_accepts_a_fresh_b_press_too() {
        const B_PRESS: PrinterInput = PrinterInput {
            a_pressed: false,
            b_pressed: true,
            a_held: false,
            b_held: true,
        };

        let mut dialog =
            synthetic_dialog(vec![Token::Char('A'), Token::End]).with_waitbuttonpress();
        // Drain printing (the one glyph, then the reveal delay and
        // Token::End) all the way to the script-level wait.
        for _ in 0..8 {
            assert_eq!(
                dialog.tick(NONE),
                DialogOutcome::Continue,
                "must not close on its own -- waitbuttonpress needs an explicit confirm"
            );
        }
        assert_eq!(
            dialog.revealed_glyph_count(),
            1,
            "the one glyph must have printed"
        );
        assert_eq!(
            dialog.tick(B_PRESS),
            DialogOutcome::Closed,
            "a fresh B edge must close a waitbuttonpress dialog just as A does"
        );
    }

    /// Issue #410: merely *holding* a button, with no fresh edge, must not
    /// satisfy the script-level wait -- `WaitForAorBPress` reads `JOY_NEW`,
    /// not `JOY_HELD` (`pokeemerald/src/scrcmd.c:1323-1330`), unlike
    /// [`Printer`]'s own held-A/B print speed-up.
    #[test]
    fn waitbuttonpress_ignores_a_held_button_with_no_fresh_edge() {
        let mut dialog =
            synthetic_dialog(vec![Token::Char('A'), Token::End]).with_waitbuttonpress();
        for _ in 0..8 {
            assert_eq!(
                dialog.tick(NONE),
                DialogOutcome::Continue,
                "must not close on its own -- waitbuttonpress needs an explicit confirm"
            );
        }
        assert_eq!(
            dialog.revealed_glyph_count(),
            1,
            "the one glyph must have printed"
        );
        assert_eq!(
            dialog.tick(HELD),
            DialogOutcome::Continue,
            "a held-but-not-freshly-pressed button must not close the dialog"
        );
        assert_eq!(
            dialog.tick(CONFIRM),
            DialogOutcome::Closed,
            "a real fresh edge afterward must still close it"
        );
    }

    #[test]
    fn tick_is_idempotent_once_closed() {
        let mut dialog = synthetic_dialog(vec![Token::End]);
        assert_eq!(dialog.tick(NONE), DialogOutcome::Closed);
        assert_eq!(dialog.tick(NONE), DialogOutcome::Closed);
        assert_eq!(dialog.tick(CONFIRM), DialogOutcome::Closed);
    }

    /// Issue #393: this box opts into held-A/B print speed-up
    /// ([`NpcDialog::new`]'s doc comment) -- upstream's ordinary field
    /// message box is one of the `AddTextPrinterForMessage(TRUE)` sites,
    /// not the `FALSE` one an earlier revision of this module wrongly
    /// claimed it matched. A press landing while the second glyph's reveal
    /// delay is pending latches the speed-up and reaches it in fewer ticks
    /// than never pressing anything at all -- mirrors
    /// [`crate::intro::tests`]'s own printer-level proof of the same latch,
    /// one level up (through [`NpcDialog::tick`] rather than
    /// [`Printer::tick`] directly), so a regression here means a real NPC
    /// conversation stopped accelerating, not just the underlying printer.
    #[test]
    fn held_confirm_reaches_the_next_glyph_in_fewer_ticks_than_unheld() {
        let tokens = || {
            vec![
                Token::Char('A'),
                Token::Char('B'),
                Token::Char('C'),
                Token::End,
            ]
        };

        let ticks_to_second_glyph = |inputs: &[PrinterInput]| {
            let mut dialog = synthetic_dialog(tokens());
            for (tick, &input) in inputs.iter().cycle().enumerate() {
                dialog.tick(input);
                if dialog.revealed_glyph_count() >= 2 {
                    return tick;
                }
            }
            unreachable!("the cycling iterator never ends");
        };

        // Unheld: MID's own cadence, no acceleration -- 'A' on tick 0, 'B'
        // only after a full reveal-delay period.
        let unheld_ticks = ticks_to_second_glyph(&[NONE]);
        // Held: a press on tick 1 (mid reveal-delay for 'B') latches the
        // speed-up and zeroes the delay outright; still held afterward.
        let held_ticks = ticks_to_second_glyph(&[NONE, CONFIRM, HELD]);

        assert!(
            held_ticks < unheld_ticks,
            "held confirm must reach the second glyph faster ({held_ticks} ticks) than \
             unheld ({unheld_ticks} ticks)"
        );
    }

    #[test]
    fn compose_over_leaves_the_base_frame_visible_outside_the_box() {
        let dialog = synthetic_dialog(vec![Token::End]);
        let mut base = Framebuffer::new();
        let marker = Rgb888 {
            r: 10,
            g: 20,
            b: 30,
        };
        base.fill(marker);

        let composed = dialog.compose_over(base);

        // Far above the standard dialogue box (which sits near the bottom
        // of the screen): must still show the caller's own backdrop.
        assert_eq!(composed.pixel(120, 0), Some(marker));
    }
}
