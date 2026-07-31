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
//! consulted while awaiting a scroll/clear" contract -- just single-message
//! (no paging) and composited on top of a live overworld frame instead of a
//! blank black one, since [`MessageBoxLayout::STANDARD`]'s own frame tiles
//! already include their interior fill (`crate::textbox::blit_frame_tiles`'s
//! own "last write wins" docs), so nothing behind the box needs erasing
//! first.

use assets::fonts::{FontId, OwnedFontGlyphSheet};
use assets::pack::{AssetPack, PackError};
use engine::text::render::{Printer, RevealedGlyph, TextSpeed, TickEvent};
use engine::text::window::MessageBoxLayout;
use engine::text::Token;
use rendering::Framebuffer;

use crate::textbox::{self, FrameAssets};

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
}

impl NpcDialog {
    /// Build a dialog over an already-decoded font `sheet` and dialogue
    /// `frame`, printing `tokens` at [`TextSpeed::Mid`] (upstream's own
    /// new-game default -- see [`crate::intro::IntroScene::from_pack`]'s
    /// identical doc comment).
    fn new(sheet: OwnedFontGlyphSheet, frame: FrameAssets, tokens: Vec<Token>) -> Self {
        let printer = Printer::new(tokens, sheet, TextSpeed::Mid, PRINTER_ORIGIN);
        Self {
            frame,
            printer,
            revealed: Vec::new(),
            finished: false,
        }
    }

    /// Copy the dialog's two required pack entries (the normal-weight font
    /// sheet and the dialogue frame) out of an already-loaded `pack` and
    /// open a dialog printing `tokens` -- mirrors
    /// [`crate::intro::IntroScene::from_pack`].
    ///
    /// # Errors
    ///
    /// [`NpcDialogError::Pack`] if `pack` is missing its `font/normal/glyphs`
    /// or message-box entries (or either is malformed);
    /// [`NpcDialogError::Font`] if the font sheet doesn't decode.
    pub(crate) fn from_pack(pack: &AssetPack, tokens: Vec<Token>) -> Result<Self, NpcDialogError> {
        let sheet = OwnedFontGlyphSheet::new(pack.font(FontId::Normal)?)?;
        let frame = FrameAssets::from_handle(pack.message_box()?);
        Ok(Self::new(sheet, frame, tokens))
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

    /// Advance the dialog by exactly one frame. `confirm_pressed` is the
    /// just-pressed edge of *either* confirm button -- upstream's message
    /// box waits on `JOY_NEW(A_BUTTON | B_BUTTON)`
    /// (`TextPrinterWaitWithDownArrow`, `src/text.c:865-882`), not A alone;
    /// combining the two edges is the caller's job
    /// (`crate::flow::overworld_phase::OverworldPhase::step`). Forwarded
    /// straight to the current [`Printer::tick`] (only consulted while
    /// awaiting a scroll/clear -- that method's own doc comment).
    pub(crate) fn tick(&mut self, confirm_pressed: bool) -> DialogOutcome {
        if self.finished {
            return DialogOutcome::Closed;
        }
        match self.printer.tick(confirm_pressed) {
            TickEvent::Glyph(g) => self.revealed.push(*g),
            TickEvent::Cleared => self.revealed.clear(),
            TickEvent::Scrolling { dy } => {
                for g in &mut self.revealed {
                    g.y -= dy;
                }
            }
            TickEvent::Finished => self.finished = true,
            TickEvent::Idle
            | TickEvent::AwaitingScroll
            | TickEvent::ScrollStarted
            | TickEvent::ScrollFinished
            | TickEvent::AwaitingClear => {}
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

    #[test]
    fn tick_reveals_glyphs_and_stays_open_while_printing() {
        let mut dialog = synthetic_dialog(vec![Token::Char('H'), Token::Char('i'), Token::End]);
        // TextSpeed::Mid: a glyph every 4th frame, starting at frame 0.
        assert_eq!(dialog.tick(false), DialogOutcome::Continue);
        assert_eq!(dialog.revealed.len(), 1);
    }

    #[test]
    fn a_message_without_a_trailing_prompt_closes_the_instant_printing_finishes() {
        let mut dialog = synthetic_dialog(vec![Token::Char('A'), Token::End]);
        for _ in 0..8 {
            if dialog.tick(false) == DialogOutcome::Closed {
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
                dialog.tick(false) != DialogOutcome::Closed,
                "must not close before a confirm press reaches the trailing prompt"
            );
        }
        // Confirm: clears the box (`Cleared`, not yet closed -- `Printer`
        // reloaded a reveal-delay counter when `PromptClear` was consumed,
        // so a few more `Idle` ticks drain before `Token::End` is finally
        // reached, mirroring `engine::text::render`'s own
        // `page_clear_resumes_after_a_reveal_delay_at_mid_speed` test).
        assert_eq!(
            dialog.tick(true),
            DialogOutcome::Continue,
            "Cleared, not yet Closed"
        );
        for _ in 0..8 {
            if dialog.tick(false) == DialogOutcome::Closed {
                return;
            }
        }
        panic!("must close once the post-clear reveal delay drains and Token::End is reached");
    }

    #[test]
    fn tick_is_idempotent_once_closed() {
        let mut dialog = synthetic_dialog(vec![Token::End]);
        assert_eq!(dialog.tick(false), DialogOutcome::Closed);
        assert_eq!(dialog.tick(false), DialogOutcome::Closed);
        assert_eq!(dialog.tick(true), DialogOutcome::Closed);
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
