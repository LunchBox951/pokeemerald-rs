//! Field message boxes drawn over the composed overworld frame.
//!
//! A dialog advances its text printer once per frame. Field-script dialogs
//! preserve their final text until a fresh A or B press closes the box.
//!
//! # Held-A/B print speed-up
//!
//! Field messages enable delay skipping when they create their text printer
//! (`pokeemerald/src/field_message_box.c:62-69,109-129`). [`NpcDialog::new`]
//! therefore accelerates printing while either confirm button is held.
//!
//! # Script-level `waitbuttonpress`
//!
//! The field script waits for a fresh A or B press only after printing finishes
//! (`pokeemerald/data/scripts/std_msgbox.inc:1-22` and
//! `pokeemerald/src/scrcmd.c:1323-1336`). [`NpcDialog::with_waitbuttonpress`]
//! models that separate wait without clearing the final printed frame.

use assets::fonts::{FontId, OwnedFontGlyphSheet};
use assets::pack::{AssetPack, PackError};
use engine::text::render::{Printer, PrinterInput, RevealedGlyph, TextSpeed, TickEvent};
use engine::text::window::MessageBoxLayout;
use engine::text::Token;
use platform::{ButtonState, Buttons};
use rendering::Framebuffer;

use crate::textbox::{self, FrameAssets};

const FIELD_SCRIPT_TEXT_SPEED: TextSpeed = TextSpeed::Mid;

/// Maps A/B button edges and holds to printer input.
pub(crate) fn confirm_printer_input(buttons: ButtonState) -> PrinterInput {
    PrinterInput {
        a_pressed: buttons.is_newly_pressed(Buttons::A),
        b_pressed: buttons.is_newly_pressed(Buttons::B),
        a_held: buttons.is_held(Buttons::A),
        b_held: buttons.is_held(Buttons::B),
    }
}

/// An error while loading an [`NpcDialog`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NpcDialogError {
    /// The asset pack could not provide the dialog resources.
    Pack(PackError),
    /// The font glyph sheet could not be decoded.
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

/// Whether an [`NpcDialog`] still owns the current frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DialogOutcome {
    /// Keep routing frames to the dialog.
    Continue,
    /// Drop the dialog and resume its caller.
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DialogState {
    Printing,
    AwaitingButtonPress,
    Closed,
}

/// An open field message box.
#[derive(Debug)]
pub(crate) struct NpcDialog {
    frame: FrameAssets,
    printer: Printer<OwnedFontGlyphSheet>,
    revealed: Vec<RevealedGlyph>,
    state: DialogState,
    wait_for_button_press: bool,
}

impl NpcDialog {
    /// Creates the standard field message box from decoded assets.
    ///
    /// NPC and save messages share this type and supply their own `text_speed`.
    /// Holding A or B accelerates printing.
    pub(crate) fn new(
        sheet: OwnedFontGlyphSheet,
        frame: FrameAssets,
        tokens: Vec<Token>,
        text_speed: TextSpeed,
    ) -> Self {
        let printer = Printer::new(tokens, sheet, text_speed, textbox::STANDARD_PRINTER_ORIGIN)
            .with_ab_speed_up_print();
        Self {
            frame,
            printer,
            revealed: Vec::new(),
            state: DialogState::Printing,
            wait_for_button_press: false,
        }
    }

    /// Keeps the final printed frame visible until a fresh A or B press closes it.
    #[must_use]
    pub(crate) const fn with_waitbuttonpress(mut self) -> Self {
        self.wait_for_button_press = true;
        self
    }

    /// Builds a confirm-to-close dialog from an already-loaded asset pack.
    ///
    /// # Errors
    ///
    /// Returns [`NpcDialogError::Pack`] for missing or malformed pack entries,
    /// and [`NpcDialogError::Font`] when the font sheet cannot be decoded.
    pub(crate) fn from_pack(pack: &AssetPack, tokens: Vec<Token>) -> Result<Self, NpcDialogError> {
        let sheet = OwnedFontGlyphSheet::new(pack.font(FontId::Normal)?)?;
        let frame = FrameAssets::from_handle(pack.message_box()?);
        Ok(Self::new(sheet, frame, tokens, FIELD_SCRIPT_TEXT_SPEED).with_waitbuttonpress())
    }

    /// Loads an asset pack and opens a confirm-to-close dialog.
    ///
    /// # Errors
    ///
    /// Returns [`NpcDialogError::Pack`] when the pack cannot be loaded or read,
    /// and [`NpcDialogError::Font`] when the font sheet cannot be decoded.
    pub(crate) fn open(
        source: crate::pack_source::PackSource,
        tokens: Vec<Token>,
    ) -> Result<Self, NpcDialogError> {
        let pack = source.load()?;
        Self::from_pack(&pack, tokens)
    }

    /// Returns the number of glyphs currently visible on screen.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn revealed_glyph_count(&self) -> usize {
        self.revealed.len()
    }

    /// Advances the dialog by one frame and reports whether it remains open.
    pub(crate) fn tick(&mut self, input: PrinterInput) -> DialogOutcome {
        match self.state {
            DialogState::Closed => return DialogOutcome::Closed,
            DialogState::AwaitingButtonPress => {
                if input.confirm_pressed() {
                    self.state = DialogState::Closed;
                    return DialogOutcome::Closed;
                }
                return DialogOutcome::Continue;
            }
            DialogState::Printing => {}
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
                if self.wait_for_button_press {
                    self.state = DialogState::AwaitingButtonPress;
                } else {
                    self.state = DialogState::Closed;
                }
            }
            TickEvent::Idle
            | TickEvent::AwaitingScroll
            | TickEvent::ScrollStarted
            | TickEvent::ScrollFinished
            | TickEvent::AwaitingClear
            | TickEvent::AwaitingPress
            | TickEvent::PressAccepted
            | TickEvent::Paused
            | TickEvent::PauseFinished => {}
        }

        if self.state == DialogState::Closed {
            DialogOutcome::Closed
        } else {
            DialogOutcome::Continue
        }
    }

    /// Draws the message box and visible glyphs over `base`.
    #[must_use]
    pub(crate) fn compose_over(&self, mut base: Framebuffer) -> Framebuffer {
        let tiles = MessageBoxLayout::STANDARD.frame_tiles();
        textbox::blit_frame_tiles(&mut base, &tiles, self.frame.image(), &self.frame.palette);
        textbox::blit_glyphs(
            &mut base,
            &self.revealed,
            textbox::STANDARD_BOX_SCREEN_ORIGIN,
            textbox::STANDARD_BOX_CONTENT_SIZE_PX,
        );
        base
    }
}

/// Creates an asset-independent open dialog for crate tests.
#[cfg(test)]
pub(crate) fn synthetic_dialog(tokens: Vec<Token>) -> NpcDialog {
    use assets::fonts::FontImageRef;
    use assets::pack::ImageRef;
    use rendering::Rgb888;

    const SHEET_WIDTH: u32 = 256;
    const SHEET_HEIGHT: u32 = 512;
    const SHEET_BIT_DEPTH: u8 = 2;
    const FRAME_WIDTH: u32 = 56;
    const FRAME_HEIGHT: u32 = 16;
    const FRAME_PALETTE_SIZE: usize = 16;

    let pixels = vec![0u8; (SHEET_WIDTH * SHEET_HEIGHT) as usize];
    let image = ImageRef {
        width: SHEET_WIDTH,
        height: SHEET_HEIGHT,
        bit_depth: SHEET_BIT_DEPTH,
        pixels: &pixels,
    };
    let sheet = OwnedFontGlyphSheet::new(FontImageRef::new_for_tests(FontId::Normal, image))
        .expect("this is the exact real glyph-sheet shape");
    let frame = FrameAssets {
        pixels: vec![0u8; (FRAME_WIDTH * FRAME_HEIGHT) as usize],
        width: FRAME_WIDTH,
        height: FRAME_HEIGHT,
        palette: vec![Rgb888::BLACK; FRAME_PALETTE_SIZE],
    };
    NpcDialog::new(sheet, frame, tokens, TextSpeed::Mid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rendering::Rgb888;

    const NO_INPUT: PrinterInput = PrinterInput {
        a_pressed: false,
        b_pressed: false,
        a_held: false,
        b_held: false,
    };

    const A_PRESS: PrinterInput = PrinterInput {
        a_pressed: true,
        b_pressed: false,
        a_held: true,
        b_held: false,
    };

    const A_HELD: PrinterInput = PrinterInput {
        a_pressed: false,
        b_pressed: false,
        a_held: true,
        b_held: false,
    };

    const B_PRESS: PrinterInput = PrinterInput {
        a_pressed: false,
        b_pressed: true,
        a_held: false,
        b_held: true,
    };

    const DIALOG_FRAME_LIMIT: usize = 16;
    const MID_SPEED_PROMPT_READY_FRAMES: usize = 8;
    const OUTSIDE_DIALOG_PIXEL: (usize, usize) = (120, 0);

    fn advance_until_state(dialog: &mut NpcDialog, expected: DialogState) {
        for _ in 0..DIALOG_FRAME_LIMIT {
            if dialog.state == expected {
                return;
            }
            assert_eq!(dialog.tick(NO_INPUT), DialogOutcome::Continue);
        }
        assert_eq!(dialog.state, expected);
    }

    #[test]
    fn tick_reveals_glyphs_and_stays_open_while_printing() {
        let mut dialog = synthetic_dialog(vec![Token::Char('H'), Token::Char('i'), Token::End]);
        assert_eq!(dialog.tick(NO_INPUT), DialogOutcome::Continue);
        assert_eq!(dialog.revealed.len(), 1);
    }

    #[test]
    fn a_message_without_a_trailing_prompt_closes_the_instant_printing_finishes() {
        let mut dialog = synthetic_dialog(vec![Token::Char('A'), Token::End]);
        for _ in 0..DIALOG_FRAME_LIMIT {
            if dialog.tick(NO_INPUT) == DialogOutcome::Closed {
                return;
            }
        }
        panic!("a short, un-prompted message must close on its own");
    }

    #[test]
    fn a_trailing_prompt_clear_waits_for_confirm_then_closes_on_the_next_tick() {
        let mut dialog = synthetic_dialog(vec![Token::Char('A'), Token::PromptClear, Token::End]);
        for _ in 0..MID_SPEED_PROMPT_READY_FRAMES {
            assert!(
                dialog.tick(NO_INPUT) != DialogOutcome::Closed,
                "must not close before a confirm press reaches the trailing prompt"
            );
        }
        assert_eq!(
            dialog.tick(A_PRESS),
            DialogOutcome::Continue,
            "Cleared, not yet Closed"
        );
        for _ in 0..DIALOG_FRAME_LIMIT {
            if dialog.tick(NO_INPUT) == DialogOutcome::Closed {
                return;
            }
        }
        panic!("must close once the post-clear reveal delay drains and Token::End is reached");
    }

    #[test]
    fn waitbuttonpress_holds_every_glyph_until_confirm_then_closes_on_that_same_tick() {
        let mut dialog = synthetic_dialog(vec![Token::Char('H'), Token::Char('i'), Token::End])
            .with_waitbuttonpress();

        advance_until_state(&mut dialog, DialogState::AwaitingButtonPress);
        assert_eq!(dialog.revealed_glyph_count(), 2);

        for _ in 0..DIALOG_FRAME_LIMIT {
            assert_eq!(dialog.tick(NO_INPUT), DialogOutcome::Continue);
            assert_eq!(
                dialog.revealed_glyph_count(),
                2,
                "text must stay on screen while awaiting the confirm press -- \
                 no Cleared, ever, on this path"
            );
        }

        assert_eq!(
            dialog.tick(A_PRESS),
            DialogOutcome::Closed,
            "a confirm edge must close the dialog on this exact tick"
        );
        assert_eq!(
            dialog.revealed_glyph_count(),
            2,
            "the box must close with its text intact, never blanked first"
        );
    }

    #[test]
    fn waitbuttonpress_accepts_a_fresh_b_press_too() {
        let mut dialog =
            synthetic_dialog(vec![Token::Char('A'), Token::End]).with_waitbuttonpress();
        advance_until_state(&mut dialog, DialogState::AwaitingButtonPress);
        assert_eq!(dialog.revealed_glyph_count(), 1);
        assert_eq!(
            dialog.tick(B_PRESS),
            DialogOutcome::Closed,
            "a fresh B edge must close a waitbuttonpress dialog just as A does"
        );
    }

    #[test]
    fn waitbuttonpress_ignores_a_held_button_with_no_fresh_edge() {
        let mut dialog =
            synthetic_dialog(vec![Token::Char('A'), Token::End]).with_waitbuttonpress();
        advance_until_state(&mut dialog, DialogState::AwaitingButtonPress);
        assert_eq!(dialog.revealed_glyph_count(), 1);
        assert_eq!(
            dialog.tick(A_HELD),
            DialogOutcome::Continue,
            "a held-but-not-freshly-pressed button must not close the dialog"
        );
        assert_eq!(
            dialog.tick(A_PRESS),
            DialogOutcome::Closed,
            "a real fresh edge afterward must still close it"
        );
    }

    #[test]
    fn tick_is_idempotent_once_closed() {
        let mut dialog = synthetic_dialog(vec![Token::End]);
        assert_eq!(dialog.tick(NO_INPUT), DialogOutcome::Closed);
        assert_eq!(dialog.tick(NO_INPUT), DialogOutcome::Closed);
        assert_eq!(dialog.tick(A_PRESS), DialogOutcome::Closed);
    }

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
            for tick in 0..DIALOG_FRAME_LIMIT {
                let input = inputs[tick % inputs.len()];
                dialog.tick(input);
                if dialog.revealed_glyph_count() >= 2 {
                    return tick;
                }
            }
            panic!("the second glyph must print within the frame limit");
        };

        let unheld_ticks = ticks_to_second_glyph(&[NO_INPUT]);
        let held_ticks = ticks_to_second_glyph(&[NO_INPUT, A_PRESS, A_HELD]);

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

        assert_eq!(
            composed.pixel(OUTSIDE_DIALOG_PIXEL.0, OUTSIDE_DIALOG_PIXEL.1),
            Some(marker)
        );
    }
}
