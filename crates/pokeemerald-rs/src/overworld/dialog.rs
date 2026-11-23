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
//! # Page-prompt down arrow
//!
//! A `\p` or `\l` prompt shows upstream's down arrow at the printer cursor
//! while it waits for A or B, and removes it once the press advances the page
//! (`pokeemerald/src/text.c:787-882`); [`textbox::PromptArrow`] owns that
//! pacing and records the arrow among the dialog's window ops.
//!
//! # Script-level `waitbuttonpress`
//!
//! The field script waits for a fresh A or B press only after printing finishes
//! (`pokeemerald/data/scripts/std_msgbox.inc:1-22` and
//! `pokeemerald/src/scrcmd.c:1323-1336`). [`NpcDialog::with_waitbuttonpress`]
//! models that separate wait without clearing the final printed frame.

use assets::fonts::{FontId, GlyphSources, OwnedFontGlyphSheet, OwnedGlyphSources};
use assets::pack::{AssetPack, PackError};
use assets::KeypadIconSheet;
use engine::text::render::{Printer, PrinterInput, TextColors, TextSpeed, TickEvent};
use engine::text::window::MessageBoxLayout;
use engine::text::Token;
use platform::{ButtonState, Buttons};
use rendering::Framebuffer;

use crate::textbox::{self, DownArrowSheet, FrameAssets, PromptArrow, WindowOp};

/// Maps A/B button edges and holds to the shared printer input shape.
///
/// Fresh edges advance prompts and close waits. Held states accelerate printing.
/// Keeping the mapping here prevents field and save dialogs from drifting.
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
    /// The font glyph sheet or keypad-icon sheet could not be decoded.
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
    icons: KeypadIconSheet,
    down_arrow: DownArrowSheet,
    prompt_arrow: PromptArrow,
    printer: Printer<OwnedFontGlyphSheet>,
    ops: Vec<WindowOp>,
    /// The colors the latest whole-window clear erased with
    /// (`Printer::cleared_window_colors`); `None` before any.
    window_fill: Option<TextColors>,
    state: DialogState,
    wait_for_button_press: bool,
}

impl NpcDialog {
    /// Creates the standard field message box from decoded assets.
    ///
    /// NPC and save messages share this type because both use the standard field
    /// message resources. Holding A or B accelerates printing.
    pub(crate) fn new(
        sources: OwnedGlyphSources,
        frame: FrameAssets,
        icons: KeypadIconSheet,
        down_arrow: DownArrowSheet,
        tokens: Vec<Token>,
        text_speed: TextSpeed,
    ) -> Self {
        let printer = Printer::with_sources(
            tokens,
            sources,
            FontId::Normal,
            text_speed,
            textbox::STANDARD_PRINTER_ORIGIN,
        )
        .with_ab_speed_up_print();
        Self {
            frame,
            icons,
            down_arrow,
            prompt_arrow: PromptArrow::default(),
            printer,
            ops: Vec::new(),
            window_fill: None,
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

    /// Builds a confirm-to-close dialog from an already-loaded asset pack, at
    /// a caller-chosen `text_speed`.
    ///
    /// # Errors
    ///
    /// Returns [`NpcDialogError::Pack`] for missing or malformed pack entries,
    /// and [`NpcDialogError::Font`] when the font or keypad-icon sheet cannot
    /// be decoded.
    pub(crate) fn from_pack_at_speed(
        pack: &AssetPack,
        tokens: Vec<Token>,
        text_speed: TextSpeed,
    ) -> Result<Self, NpcDialogError> {
        let sources = GlyphSources::try_from_fn(|font| {
            Ok::<_, NpcDialogError>(OwnedFontGlyphSheet::new(pack.font(font)?)?)
        })?;
        let frame = FrameAssets::from_handle(pack.message_box()?);
        let icons = KeypadIconSheet::new(pack.keypad_icons()?)?;
        let down_arrow = DownArrowSheet::new(&pack.text_printer_down_arrow()?);
        Ok(Self::new(sources, frame, icons, down_arrow, tokens, text_speed).with_waitbuttonpress())
    }

    /// Loads an asset pack and opens a confirm-to-close dialog, at a
    /// caller-chosen `text_speed` -- see [`Self::from_pack_at_speed`].
    ///
    /// # Errors
    ///
    /// Returns [`NpcDialogError::Pack`] when the pack cannot be loaded or read,
    /// and [`NpcDialogError::Font`] when the font or keypad-icon sheet cannot be
    /// decoded.
    pub(crate) fn open_at_speed(
        source: &crate::pack_source::PackSource,
        tokens: Vec<Token>,
        text_speed: TextSpeed,
    ) -> Result<Self, NpcDialogError> {
        let pack = source.load()?;
        Self::from_pack_at_speed(&pack, tokens, text_speed)
    }

    /// Returns the number of glyphs currently retained (revealed, and not
    /// since dropped by a page or window clear).
    #[cfg(test)]
    #[must_use]
    pub(crate) fn revealed_glyph_count(&self) -> usize {
        self.ops
            .iter()
            .filter(|op| matches!(op, WindowOp::Glyph(_)))
            .count()
    }

    /// Advances the dialog by one frame and reports whether it remains open.
    ///
    /// Fresh A/B edges advance printer prompts and the final script wait. Held A/B
    /// states accelerate printing without satisfying that final wait.
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

        let event = self.printer.tick(input);
        // Checked ahead of `event` so a same-tick `FILL_WINDOW` glyph
        // survives the clear it follows; see `Printer::cleared_window`.
        if self.printer.cleared_window() {
            self.ops.clear();
            self.window_fill = self.printer.cleared_window_colors();
        }
        // A prompt wait owns its frame: no glyph, span, or icon competes with
        // the arrow draw or erase it makes here (`PromptArrow::follow`).
        self.prompt_arrow.follow(
            &self.printer,
            &event,
            textbox::STANDARD_BOX_CONTENT_SIZE_PX.1,
            &self.down_arrow,
            &mut self.ops,
        );
        match event {
            TickEvent::Glyph(g) => self.push_clipped(WindowOp::Glyph(g)),
            // `ScrollWindow` fills the exposed rows with the background in
            // force; no token runs while a scroll is in flight.
            TickEvent::Scrolling { dy } => textbox::scroll_window_ops(
                &mut self.ops,
                dy,
                self.printer.colors(),
                textbox::STANDARD_BOX_CONTENT_SIZE_PX,
            ),
            TickEvent::Finished => {
                if self.wait_for_button_press {
                    self.state = DialogState::AwaitingButtonPress;
                } else {
                    self.state = DialogState::Closed;
                }
            }
            TickEvent::Idle
            | TickEvent::Cleared
            | TickEvent::AwaitingScroll
            | TickEvent::ScrollStarted
            | TickEvent::ScrollFinished
            | TickEvent::AwaitingClear
            | TickEvent::AwaitingPress
            | TickEvent::PressAccepted
            | TickEvent::Paused
            | TickEvent::PauseFinished => {}
        }
        // Polled right after `tick`. Upstream draws a glyph before clearing the
        // minimum-spacing gap behind it (`text.c:1146-1156`), so the span goes
        // on after the glyph the same tick pushed; a positive `CLEAR`/`CLEAR_TO`
        // ends its frame as `TickEvent::Idle` with no glyph (`Printer::cleared_span`).
        if let Some(span) = self.printer.cleared_span() {
            self.push_clipped(WindowOp::ClearSpan(span));
        }
        // A keypad icon also ends its frame as `TickEvent::Idle`
        // (`Printer::keypad_icon`), so it never competes with a glyph either.
        if let Some(icon) = self.printer.keypad_icon() {
            self.push_clipped(WindowOp::KeypadIcon(icon));
        }

        if self.state == DialogState::Closed {
            DialogOutcome::Closed
        } else {
            DialogOutcome::Continue
        }
    }

    /// Retains `op` with the rows below the window removed, once, at reveal
    /// time; see [`WindowOp::clipped_below`].
    fn push_clipped(&mut self, op: WindowOp) {
        textbox::push_clipped(&mut self.ops, op, textbox::STANDARD_BOX_CONTENT_SIZE_PX.1);
    }

    /// Draws the message box and visible glyphs over `base`.
    #[must_use]
    pub(crate) fn compose_over(&self, mut base: Framebuffer) -> Framebuffer {
        // A transparent (index 0) fill or scroll fill shows what lies under the box.
        let scene = textbox::needs_scene(&self.ops, self.window_fill).then(|| base.clone());
        let tiles = MessageBoxLayout::STANDARD.frame_tiles();
        textbox::blit_frame_tiles(&mut base, &tiles, self.frame.image(), &self.frame.palette);
        textbox::compose_window_ops(
            &mut base,
            &self.ops,
            textbox::STANDARD_BOX_SCREEN_ORIGIN,
            textbox::STANDARD_BOX_CONTENT_SIZE_PX,
            &self.icons,
            &self.frame.palette,
            textbox::WindowBackdrop::message_box(self.window_fill, scene.as_ref()),
        );
        base
    }
}

/// Creates an asset-independent open dialog for crate tests.
#[cfg(test)]
pub(crate) fn synthetic_dialog(tokens: Vec<Token>) -> NpcDialog {
    synthetic_dialog_at_speed(tokens, TextSpeed::Mid)
}

/// [`synthetic_dialog`] whose message-box frame draws opaque `color`
/// tiles, where the plain fixture's frame draws nothing.
#[cfg(test)]
pub(crate) fn synthetic_dialog_with_frame_color(
    tokens: Vec<Token>,
    color: rendering::Rgb888,
) -> NpcDialog {
    const OPAQUE_INDEX: u8 = 1;
    let mut dialog = synthetic_dialog(tokens);
    dialog.frame.pixels.fill(OPAQUE_INDEX);
    dialog.frame.palette[usize::from(OPAQUE_INDEX)] = color;
    dialog
}

/// [`synthetic_dialog`], but at a caller-chosen [`TextSpeed`].
///
/// Only used within this module's own tests, which need [`TextSpeed::Instant`]
/// to observe same-tick behaviour (e.g. `FILL_WINDOW`'s `RENDER_REPEAT`).
#[cfg(test)]
fn synthetic_dialog_at_speed(tokens: Vec<Token>, speed: TextSpeed) -> NpcDialog {
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
    let sources = GlyphSources::try_from_fn(|font| {
        OwnedFontGlyphSheet::new(FontImageRef::new_for_tests(font, image))
    })
    .expect("this is the exact real glyph-sheet shape");
    let frame = FrameAssets {
        pixels: vec![0u8; (FRAME_WIDTH * FRAME_HEIGHT) as usize],
        width: FRAME_WIDTH,
        height: FRAME_HEIGHT,
        palette: vec![Rgb888::BLACK; FRAME_PALETTE_SIZE],
    };
    NpcDialog::new(
        sources,
        frame,
        textbox::test_keypad_icons(&[]),
        textbox::test_blank_down_arrow(),
        tokens,
        speed,
    )
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

    const ONE_GLYPH_PRINT_FRAME_LIMIT: usize = 8;
    const TWO_GLYPH_PRINT_FRAME_LIMIT: usize = 16;
    const MID_SPEED_PROMPT_READY_FRAMES: usize = 8;
    const POST_CLEAR_CLOSE_FRAME_LIMIT: usize = 8;
    const WAIT_STATE_STABILITY_FRAMES: usize = 8;
    const ACCELERATION_TEST_FRAME_LIMIT: usize = 16;
    const OUTSIDE_DIALOG_PIXEL: (usize, usize) = (120, 0);

    fn advance_until_state(dialog: &mut NpcDialog, expected: DialogState, frame_limit: usize) {
        for _ in 0..frame_limit {
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
        assert_eq!(dialog.ops.len(), 1);
    }

    #[test]
    fn fill_window_drops_stale_glyphs_but_keeps_the_glyph_printed_after_it() {
        // `0x0F` is `EXT_CTRL_CODE_FILL_WINDOW` (`pokeemerald/src/text.c`
        // `:1052-1056`), zero arguments per `charmap.txt:427`.
        let mut dialog = synthetic_dialog_at_speed(
            vec![
                Token::Char('A'),
                Token::Char('B'),
                Token::ExtCtrl {
                    sub: 0x0F,
                    args: vec![],
                },
                Token::Char('C'),
                Token::End,
            ],
            TextSpeed::Instant,
        );

        assert_eq!(dialog.tick(NO_INPUT), DialogOutcome::Continue);
        assert_eq!(dialog.tick(NO_INPUT), DialogOutcome::Continue);
        assert_eq!(
            dialog.revealed_glyph_count(),
            2,
            "both glyphs printed before FILL_WINDOW should be on screen"
        );

        assert_eq!(dialog.tick(NO_INPUT), DialogOutcome::Continue);
        assert_eq!(
            dialog.revealed_glyph_count(),
            1,
            "FILL_WINDOW must drop the stale glyphs, but not the one printed after it \
             in the same tick"
        );
        let WindowOp::Glyph(only_glyph) = &dialog.ops[0] else {
            panic!("expected the surviving op to be a glyph")
        };
        assert_eq!(
            (only_glyph.x, only_glyph.y),
            textbox::STANDARD_PRINTER_ORIGIN,
            "the surviving glyph must be placed at the reset cursor, not the stale one"
        );
    }

    #[test]
    fn a_message_without_a_trailing_prompt_closes_the_instant_printing_finishes() {
        let mut dialog = synthetic_dialog(vec![Token::Char('A'), Token::End]);
        for _ in 0..ONE_GLYPH_PRINT_FRAME_LIMIT {
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
        for _ in 0..POST_CLEAR_CLOSE_FRAME_LIMIT {
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

        advance_until_state(
            &mut dialog,
            DialogState::AwaitingButtonPress,
            TWO_GLYPH_PRINT_FRAME_LIMIT,
        );
        assert_eq!(dialog.revealed_glyph_count(), 2);

        for _ in 0..WAIT_STATE_STABILITY_FRAMES {
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
        advance_until_state(
            &mut dialog,
            DialogState::AwaitingButtonPress,
            ONE_GLYPH_PRINT_FRAME_LIMIT,
        );
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
        advance_until_state(
            &mut dialog,
            DialogState::AwaitingButtonPress,
            ONE_GLYPH_PRINT_FRAME_LIMIT,
        );
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
            for tick in 0..ACCELERATION_TEST_FRAME_LIMIT {
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

    /// A dialog whose message-box palette and font sheet are fully controlled:
    /// every glyph pixel is `glyph_index`, and palette slot 2/3 are distinct.
    fn dialog_with_palette(glyph_index: u8, palette: Vec<Rgb888>) -> NpcDialog {
        use assets::fonts::FontImageRef;
        use assets::pack::ImageRef;

        const SHEET_WIDTH: u32 = 256;
        const SHEET_HEIGHT: u32 = 512;
        const FONT_SHEET_BIT_DEPTH: u8 = 2;
        const FRAME_WIDTH: u32 = 56;
        const FRAME_HEIGHT: u32 = 16;

        let pixels = vec![glyph_index; (SHEET_WIDTH * SHEET_HEIGHT) as usize];
        let image = ImageRef {
            width: SHEET_WIDTH,
            height: SHEET_HEIGHT,
            bit_depth: FONT_SHEET_BIT_DEPTH,
            pixels: &pixels,
        };
        let sources = GlyphSources::try_from_fn(|font| {
            OwnedFontGlyphSheet::new(FontImageRef::new_for_tests(font, image))
        })
        .expect("this is the exact real glyph-sheet shape");
        let frame = FrameAssets {
            // Every frame pixel is palette index 0, i.e. transparent, so only
            // glyph pixels reach the framebuffer.
            pixels: vec![0u8; (FRAME_WIDTH * FRAME_HEIGHT) as usize],
            width: FRAME_WIDTH,
            height: FRAME_HEIGHT,
            palette,
        };
        NpcDialog::new(
            sources,
            frame,
            textbox::test_keypad_icons(&[]),
            textbox::test_blank_down_arrow(),
            vec![Token::Char('A'), Token::End],
            TextSpeed::Mid,
        )
    }

    /// Upstream's palette-index choice (`pokeemerald/src/menu.c:191-195`);
    /// see [`FrameAssets::glyph_colors`] for the full contract this asserts.
    #[test]
    fn dialog_glyphs_take_their_colors_from_the_loaded_message_box_palette() {
        const FOREGROUND_FONT_INDEX: u8 = 1;
        const SHADOW_FONT_INDEX: u8 = 2;
        const MESSAGE_BOX_FOREGROUND_SLOT: usize = 2;
        const MESSAGE_BOX_SHADOW_SLOT: usize = 3;
        const PALETTE_SIZE: usize = 16;

        let foreground = Rgb888 {
            r: 96,
            g: 96,
            b: 96,
        };
        let shadow = Rgb888 {
            r: 208,
            g: 208,
            b: 200,
        };
        let mut palette = vec![Rgb888::BLACK; PALETTE_SIZE];
        palette[MESSAGE_BOX_FOREGROUND_SLOT] = foreground;
        palette[MESSAGE_BOX_SHADOW_SLOT] = shadow;

        let first_glyph_pixel = (
            usize::try_from(
                textbox::STANDARD_BOX_SCREEN_ORIGIN.0 + textbox::STANDARD_PRINTER_ORIGIN.0,
            )
            .unwrap(),
            usize::try_from(
                textbox::STANDARD_BOX_SCREEN_ORIGIN.1 + textbox::STANDARD_PRINTER_ORIGIN.1,
            )
            .unwrap(),
        );

        for (font_index, expected) in [
            (FOREGROUND_FONT_INDEX, foreground),
            (SHADOW_FONT_INDEX, shadow),
        ] {
            let mut dialog = dialog_with_palette(font_index, palette.clone());
            assert_eq!(dialog.tick(NO_INPUT), DialogOutcome::Continue);
            assert_eq!(dialog.revealed_glyph_count(), 1);

            let composed = dialog.compose_over(Framebuffer::new());

            assert_eq!(
                composed.pixel(first_glyph_pixel.0, first_glyph_pixel.1),
                Some(expected),
                "font index {font_index} must paint the loaded palette's slot color"
            );
        }
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

    const OPAQUE_GLYPH_PALETTE_INDEX: u8 = 1;
    const FRAME_INTERIOR_PALETTE_INDEX: u8 = 1;
    // The palette index upstream selects as the message-box glyph
    // foreground (`TEXT_COLOR_DARK_GRAY`, `pokeemerald/src/menu.c:191-201`);
    // mirrors `textbox::MESSAGE_BOX_FOREGROUND_PALETTE_INDEX`.
    const GLYPH_FOREGROUND_PALETTE_INDEX: u8 = 2;
    const NORMAL_A_ADVANCE_WIDTH: i32 = 6;

    const OPAQUE_GLYPH_COLOR: Rgb888 = Rgb888 {
        r: 24,
        g: 24,
        b: 24,
    };

    const FRAME_INTERIOR_COLOR: Rgb888 = Rgb888 {
        r: 200,
        g: 40,
        b: 40,
    };

    const ICON_ACCENT_PALETTE_INDEX: u8 = 3;
    const ICON_ACCENT_COLOR: Rgb888 = Rgb888 {
        r: 30,
        g: 60,
        b: 220,
    };

    /// A dialog whose glyph sheet and message box are solid, distinguishable
    /// colors, so a cleared span's pixels can be told apart from an unerased
    /// glyph's -- `synthetic_dialog`'s all-transparent fixture cannot.
    fn opaque_synthetic_dialog(tokens: Vec<Token>) -> NpcDialog {
        opaque_synthetic_dialog_with_icons(tokens, textbox::test_keypad_icons(&[]))
    }

    /// [`opaque_synthetic_dialog`] over a caller-supplied keypad-icon sheet.
    fn opaque_synthetic_dialog_with_icons(tokens: Vec<Token>, icons: KeypadIconSheet) -> NpcDialog {
        use assets::fonts::FontImageRef;
        use assets::pack::ImageRef;

        const SHEET_WIDTH: u32 = 256;
        const SHEET_HEIGHT: u32 = 512;
        const SHEET_BIT_DEPTH: u8 = 2;
        const FRAME_WIDTH: u32 = 56;
        const FRAME_HEIGHT: u32 = 16;
        const FRAME_PALETTE_SIZE: usize = 16;

        let pixels = vec![OPAQUE_GLYPH_PALETTE_INDEX; (SHEET_WIDTH * SHEET_HEIGHT) as usize];
        let image = ImageRef {
            width: SHEET_WIDTH,
            height: SHEET_HEIGHT,
            bit_depth: SHEET_BIT_DEPTH,
            pixels: &pixels,
        };
        let sources = GlyphSources::try_from_fn(|font| {
            OwnedFontGlyphSheet::new(FontImageRef::new_for_tests(font, image))
        })
        .expect("this is the exact real glyph-sheet shape");

        let mut palette = vec![Rgb888::BLACK; FRAME_PALETTE_SIZE];
        palette[usize::from(FRAME_INTERIOR_PALETTE_INDEX)] = FRAME_INTERIOR_COLOR;
        palette[usize::from(GLYPH_FOREGROUND_PALETTE_INDEX)] = OPAQUE_GLYPH_COLOR;
        palette[usize::from(ICON_ACCENT_PALETTE_INDEX)] = ICON_ACCENT_COLOR;
        let frame = FrameAssets {
            pixels: vec![FRAME_INTERIOR_PALETTE_INDEX; (FRAME_WIDTH * FRAME_HEIGHT) as usize],
            width: FRAME_WIDTH,
            height: FRAME_HEIGHT,
            palette,
        };
        NpcDialog::new(
            sources,
            frame,
            icons,
            textbox::test_blank_down_arrow(),
            tokens,
            TextSpeed::Instant,
        )
    }

    /// Screen pixel for a window-local content coordinate in the standard box.
    fn box_pixel(content: (i32, i32)) -> (usize, usize) {
        let x = textbox::STANDARD_BOX_SCREEN_ORIGIN.0 + content.0;
        let y = textbox::STANDARD_BOX_SCREEN_ORIGIN.1 + content.1;
        (usize::try_from(x).unwrap(), usize::try_from(y).unwrap())
    }

    /// `[KEYPAD_ICON A_BUTTON, 'A']` prints the icon where the printer places
    /// it, then the glyph past the icon's 8-pixel width. The icon's pixels are
    /// window-palette indices (`pokeemerald/src/text.c:1609-1623`), so index 1
    /// stays the window's slot 1 rather than the glyph foreground role.
    #[test]
    fn a_keypad_icon_draws_at_the_printers_placement_and_the_next_glyph_follows_it() {
        const ICON_A_ID: u8 = 0;
        const ICON_A_WIDTH: i32 = 8;
        // Sheet pixels of icon A (sheet origin (0, 0)): window slots 2, 3, 1.
        let icons = textbox::test_keypad_icons(&[(0, 0, 2), (3, 5, 3), (5, 0, 1)]);
        let mut dialog = opaque_synthetic_dialog_with_icons(
            vec![Token::KeypadIcon(ICON_A_ID), Token::Char('A'), Token::End],
            icons,
        );
        let (origin_x, origin_y) = textbox::STANDARD_PRINTER_ORIGIN;

        assert_eq!(dialog.tick(NO_INPUT), DialogOutcome::Continue);
        let placement = dialog
            .printer
            .keypad_icon()
            .expect("the first tick places the icon");
        assert_eq!((placement.x, placement.y), (origin_x, origin_y));
        assert_eq!(i32::from(placement.width), ICON_A_WIDTH);

        let fb = dialog.compose_over(Framebuffer::new());
        let at = |dx: i32, dy: i32| {
            let (x, y) = box_pixel((placement.x + dx, placement.y + dy));
            fb.pixel(x, y)
        };
        assert_eq!(at(0, 0), Some(OPAQUE_GLYPH_COLOR), "icon index 2");
        assert_eq!(at(3, 5), Some(ICON_ACCENT_COLOR), "icon index 3");
        assert_eq!(
            at(5, 0),
            Some(FRAME_INTERIOR_COLOR),
            "icon index 1 keeps slot 1"
        );
        assert_eq!(
            at(1, 0),
            Some(FRAME_INTERIOR_COLOR),
            "icon index 0 is transparent"
        );
        assert_eq!(
            at(ICON_A_WIDTH, 0),
            Some(FRAME_INTERIOR_COLOR),
            "the glyph has not printed yet"
        );

        assert_eq!(dialog.tick(NO_INPUT), DialogOutcome::Continue);
        assert_eq!(
            dialog.printer.keypad_icon(),
            None,
            "the glyph tick has no icon"
        );
        let fb = dialog.compose_over(Framebuffer::new());
        let (glyph_x, glyph_y) = box_pixel((origin_x + ICON_A_WIDTH, origin_y));
        assert_eq!(
            fb.pixel(glyph_x, glyph_y),
            Some(OPAQUE_GLYPH_COLOR),
            "the glyph starts at the icon's right edge"
        );
        let (icon_x, icon_y) = box_pixel((origin_x + 3, origin_y + 5));
        assert_eq!(
            fb.pixel(icon_x, icon_y),
            Some(ICON_ACCENT_COLOR),
            "the icon stays after the glyph prints"
        );
    }

    #[test]
    fn clear_paints_the_erased_span_with_the_windows_background() {
        const CLEARED_WIDTH: u8 = 4;
        // `0x0D`/`0x11` are `EXT_CTRL_CODE_SHIFT_RIGHT`/`EXT_CTRL_CODE_CLEAR`
        // (`pokeemerald/src/text.c:1063-1072`), one argument byte each per
        // `charmap.txt`. `SHIFT_RIGHT 0` moves the cursor back over the `A`
        // just printed, so `CLEAR` erases `CLEARED_WIDTH` columns, fewer than the glyph's
        // advance width, leaving its right-hand columns visible.
        let mut dialog = opaque_synthetic_dialog(vec![
            Token::Char('A'),
            Token::ExtCtrl {
                sub: 0x0D,
                args: vec![0],
            },
            Token::ExtCtrl {
                sub: 0x11,
                args: vec![CLEARED_WIDTH],
            },
            Token::End,
        ]);

        assert_eq!(
            dialog.tick(NO_INPUT),
            DialogOutcome::Continue,
            "reveals 'A'"
        );
        assert_eq!(
            dialog.tick(NO_INPUT),
            DialogOutcome::Continue,
            "SHIFT_RIGHT is free; CLEAR ends the frame"
        );

        let fb = dialog.compose_over(Framebuffer::new());

        let (cleared_x, cleared_y) = box_pixel((
            textbox::STANDARD_PRINTER_ORIGIN.0 + 2,
            textbox::STANDARD_PRINTER_ORIGIN.1 + 8,
        ));
        assert_eq!(
            fb.pixel(cleared_x, cleared_y),
            Some(FRAME_INTERIOR_COLOR),
            "the erased span must show the window's background, not the glyph"
        );

        let (still_glyph_x, still_glyph_y) = box_pixel((
            textbox::STANDARD_PRINTER_ORIGIN.0 + NORMAL_A_ADVANCE_WIDTH - 1,
            textbox::STANDARD_PRINTER_ORIGIN.1 + 8,
        ));
        assert_eq!(
            fb.pixel(still_glyph_x, still_glyph_y),
            Some(OPAQUE_GLYPH_COLOR),
            "pixels past the cleared width belong to the same glyph and must stay visible"
        );
    }

    #[test]
    fn clear_to_paints_only_up_to_its_target_column() {
        // `0x13` is `EXT_CTRL_CODE_CLEAR_TO` (`pokeemerald/src/text.c`
        // `:1077-1090`); the argument is an origin-relative column, so this
        // erases fewer pixels than the glyph's full advance width.
        const CLEAR_TO_COLUMN: u8 = 4;

        let mut dialog = opaque_synthetic_dialog(vec![
            Token::Char('A'),
            Token::ExtCtrl {
                sub: 0x0D,
                args: vec![0],
            },
            Token::ExtCtrl {
                sub: 0x13,
                args: vec![CLEAR_TO_COLUMN],
            },
            Token::End,
        ]);

        assert_eq!(
            dialog.tick(NO_INPUT),
            DialogOutcome::Continue,
            "reveals 'A'"
        );
        assert_eq!(
            dialog.tick(NO_INPUT),
            DialogOutcome::Continue,
            "SHIFT_RIGHT is free; CLEAR_TO ends the frame"
        );

        let fb = dialog.compose_over(Framebuffer::new());

        let (cleared_x, cleared_y) = box_pixel((
            textbox::STANDARD_PRINTER_ORIGIN.0 + 1,
            textbox::STANDARD_PRINTER_ORIGIN.1 + 8,
        ));
        assert_eq!(
            fb.pixel(cleared_x, cleared_y),
            Some(FRAME_INTERIOR_COLOR),
            "the span up to the target column must show the window's background"
        );

        let (still_glyph_x, still_glyph_y) = box_pixel((
            textbox::STANDARD_PRINTER_ORIGIN.0 + NORMAL_A_ADVANCE_WIDTH - 1,
            textbox::STANDARD_PRINTER_ORIGIN.1 + 8,
        ));
        assert_eq!(
            fb.pixel(still_glyph_x, still_glyph_y),
            Some(OPAQUE_GLYPH_COLOR),
            "pixels past the target column belong to the same glyph and must stay visible"
        );
    }

    #[test]
    fn min_letter_spacing_erases_the_post_glyph_gap_after_the_glyph_draws() {
        // `0x14` is `EXT_CTRL_CODE_MIN_LETTER_SPACING`; upstream copies the
        // glyph and only then clears the gap up to the minimum
        // (`pokeemerald/src/text.c:1146-1156`), so the opaque glyph's pixels
        // inside the gap must show the window's background.
        const MINIMUM: u8 = 12;
        let mut dialog = opaque_synthetic_dialog(vec![
            Token::ExtCtrl {
                sub: 0x14,
                args: vec![MINIMUM],
            },
            Token::Char('A'),
            Token::End,
        ]);

        assert_eq!(
            dialog.tick(NO_INPUT),
            DialogOutcome::Continue,
            "reveals 'A'"
        );
        assert!(
            matches!(
                dialog.ops.as_slice(),
                [WindowOp::Glyph(_), WindowOp::ClearSpan(_)]
            ),
            "the gap span follows the glyph in the same tick"
        );

        let fb = dialog.compose_over(Framebuffer::new());
        let probe = |x: i32| {
            let (x, y) = box_pixel((
                textbox::STANDARD_PRINTER_ORIGIN.0 + x,
                textbox::STANDARD_PRINTER_ORIGIN.1 + 8,
            ));
            fb.pixel(x, y)
        };
        assert_eq!(
            probe(2),
            Some(OPAQUE_GLYPH_COLOR),
            "the glyph's own advance stays"
        );
        assert_eq!(
            probe(NORMAL_A_ADVANCE_WIDTH + 2),
            Some(FRAME_INTERIOR_COLOR),
            "the gap up to the minimum is erased after the glyph draws"
        );
        // The glyph's cell holds nothing past its advance width on screen
        // (`CopyGlyphToWindow` copies `gCurGlyph.width` columns,
        // `text.c:608-609`), so the gap's far end is the frame interior whether
        // or not the clear reached it; the last copied glyph column is what
        // stays observable.
        assert_eq!(
            probe(NORMAL_A_ADVANCE_WIDTH - 1),
            Some(OPAQUE_GLYPH_COLOR),
            "the glyph's last copied column stays"
        );
        assert_eq!(
            probe(i32::from(MINIMUM) + 1),
            Some(FRAME_INTERIOR_COLOR),
            "nothing is drawn past the glyph's width or the minimum"
        );
    }

    #[test]
    fn min_letter_spacing_gap_clear_erases_an_earlier_glyph_inside_the_gap() {
        // An earlier opaque 'A' at column 8 is overlapped by the gap that a
        // later 'A' at column 0 clears up to the minimum of 12
        // (`text.c:1146-1156`), so only the gap's replay of the pre-glyph
        // background can erase the seeded pixels; column 13 lies past the gap
        // and keeps the seeded glyph.
        const SKIP_TO_COLUMN: u8 = 0x12;
        const MINIMUM: u8 = 12;
        let mut dialog = opaque_synthetic_dialog(vec![
            Token::ExtCtrl {
                sub: SKIP_TO_COLUMN,
                args: vec![8],
            },
            Token::Char('A'),
            Token::ExtCtrl {
                sub: SKIP_TO_COLUMN,
                args: vec![0],
            },
            Token::ExtCtrl {
                sub: 0x14,
                args: vec![MINIMUM],
            },
            Token::Char('A'),
            Token::End,
        ]);

        for _ in 0..8 {
            if dialog
                .ops
                .iter()
                .any(|op| matches!(op, WindowOp::ClearSpan(_)))
            {
                break;
            }
            dialog.tick(NO_INPUT);
        }
        assert!(
            dialog
                .ops
                .iter()
                .any(|op| matches!(op, WindowOp::ClearSpan(_))),
            "the second glyph queues its gap span"
        );

        let fb = dialog.compose_over(Framebuffer::new());
        let probe = |x: i32| {
            let (x, y) = box_pixel((
                textbox::STANDARD_PRINTER_ORIGIN.0 + x,
                textbox::STANDARD_PRINTER_ORIGIN.1 + 8,
            ));
            fb.pixel(x, y)
        };
        assert_eq!(
            probe(13),
            Some(OPAQUE_GLYPH_COLOR),
            "the seeded glyph past the gap stays"
        );
        assert_eq!(
            probe(9),
            Some(FRAME_INTERIOR_COLOR),
            "the seeded glyph inside the gap is erased"
        );
    }

    /// The smallest pack [`NpcDialog::from_pack_at_speed`] accepts: the
    /// standard message box frame and the normal font sheet. Mirrors
    /// `crate::start_menu::pack_fixture::synthetic_start_menu_pack_bytes`'s own
    /// fixture.
    fn synthetic_field_dialog_pack_bytes() -> Vec<u8> {
        use crate::pack_test_support::{image_entry, pack_bytes_with_fonts, palette_entry};

        const MESSAGE_BOX_WIDTH: u32 = 56;
        const MESSAGE_BOX_HEIGHT: u32 = 16;
        const FRAME_BIT_DEPTH: u8 = 4;
        const FONT_BIT_DEPTH: u8 = 2;
        const PALETTE_COLOUR_COUNT: u16 = 16;

        pack_bytes_with_fonts(vec![
            image_entry(
                "text-window/image/message_box",
                MESSAGE_BOX_WIDTH,
                MESSAGE_BOX_HEIGHT,
                FRAME_BIT_DEPTH,
                0,
            ),
            palette_entry("text-window/palette/message_box", PALETTE_COLOUR_COUNT),
            image_entry(
                "font/normal/glyphs",
                assets::fonts::SHEET_WIDTH,
                assets::fonts::SHEET_HEIGHT,
                FONT_BIT_DEPTH,
                0,
            ),
        ])
    }

    /// Ticks `dialog` until `glyph_count` glyphs are revealed, or panics past
    /// `PRINT_FRAME_BUDGET` frames.
    fn frames_to_reveal(dialog: &mut NpcDialog, glyph_count: usize) -> usize {
        const PRINT_FRAME_BUDGET: usize = 64;
        for frame in 1..=PRINT_FRAME_BUDGET {
            dialog.tick(NO_INPUT);
            if dialog.revealed_glyph_count() == glyph_count {
                return frame;
            }
        }
        panic!("a message of {glyph_count} glyphs must print within {PRINT_FRAME_BUDGET} frames");
    }

    fn field_dialog_scratch_path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "pokeemerald-rs-field-dialog-text-speed-{}-{:?}.pack",
            std::process::id(),
            std::thread::current().id()
        ))
    }

    /// Writes the synthetic field-dialog pack to its scratch path and
    /// returns that path; the pacing test and its planted-symlink regression
    /// share this one writer.
    fn write_field_dialog_scratch_pack() -> std::path::PathBuf {
        let path = field_dialog_scratch_path();
        crate::overworld::test_support::write_scratch_exclusive(
            &path,
            synthetic_field_dialog_pack_bytes(),
        );
        path
    }

    #[cfg(unix)]
    #[test]
    fn field_dialog_scratch_pack_refuses_a_planted_symlink() {
        crate::overworld::test_support::assert_planted_symlink_untouched(
            &field_dialog_scratch_path(),
            || {
                write_field_dialog_scratch_pack();
            },
        );
    }

    /// A dialog opened at a given speed paces at it: FAST prints the same
    /// message in fewer frames than SLOW (`sTextSpeedFrameDelays`,
    /// `pokeemerald/src/menu.c:77-82`, `:191-196`).
    #[test]
    fn field_dialogs_pace_at_the_speed_they_are_given() {
        const MESSAGE_GLYPHS: usize = 3;
        let tokens = || {
            vec![
                Token::Char('H'),
                Token::Char('i'),
                Token::Char('!'),
                Token::End,
            ]
        };

        let pack = crate::overworld::test_support::with_scratch_pack(
            field_dialog_scratch_path(),
            synthetic_field_dialog_pack_bytes(),
            |pack| pack,
        );

        let mut fast_dialog =
            NpcDialog::from_pack_at_speed(&pack, tokens(), TextSpeed::from_raw_option(2))
                .expect("the synthetic pack carries the dialog's font and message box");
        let mut slow_dialog =
            NpcDialog::from_pack_at_speed(&pack, tokens(), TextSpeed::from_raw_option(0))
                .expect("the synthetic pack carries the dialog's font and message box");

        crate::pack_test_support::assert_carries_every_font(&fast_dialog.printer);
        let fast = frames_to_reveal(&mut fast_dialog, MESSAGE_GLYPHS);
        let slow = frames_to_reveal(&mut slow_dialog, MESSAGE_GLYPHS);

        assert!(
            fast < slow,
            "a field dialog given the FAST saved option must print in fewer frames than one \
             given SLOW, but they took {fast} and {slow} frames"
        );
    }

    // -- Page-prompt down arrow (`pokeemerald/src/text.c:787-882`) ----------

    const ARROW_WIDTH_PX: i32 = 8;
    const ARROW_HEIGHT_PX: i32 = 16;
    /// `downArrowDelay` reloads to eight after a draw, so a waiting prompt
    /// redraws every ninth frame (`text.c:831`).
    const ARROW_REDRAW_INTERVAL: usize = 9;
    const ARROW_FRAMES: usize = 4;

    /// [`opaque_synthetic_dialog`] drawing a [`textbox::test_down_arrow`]
    /// whose frame `n` marks only row `n` of column 0 with window slot 2.
    fn arrow_dialog(tokens: Vec<Token>) -> NpcDialog {
        let mut dialog = opaque_synthetic_dialog(tokens);
        dialog.down_arrow = textbox::test_down_arrow(GLYPH_FOREGROUND_PALETTE_INDEX);
        dialog
    }

    /// Ticks `dialog` with no input through the frame its printer first
    /// reports a page prompt.
    fn tick_until_prompt(dialog: &mut NpcDialog) {
        for _ in 0..ONE_GLYPH_PRINT_FRAME_LIMIT {
            assert_eq!(dialog.tick(NO_INPUT), DialogOutcome::Continue);
            if dialog.prompt_arrow != PromptArrow::default() {
                return;
            }
        }
        panic!("the printer never reached its prompt");
    }

    fn has_arrow_op(dialog: &NpcDialog) -> bool {
        dialog
            .ops
            .iter()
            .any(|op| matches!(op, WindowOp::DownArrow(_)))
    }

    /// Which of the arrow rectangle's frame-marker rows is drawn, if the
    /// rectangle holds a drawn arrow. Any pixel that is neither the window's
    /// interior nor the marker fails the test: the arrow's own erase must
    /// have cleared the glyph pixels under it.
    fn drawn_arrow_frame(dialog: &NpcDialog) -> Option<usize> {
        let fb = dialog.compose_over(Framebuffer::new());
        let (cursor_x, cursor_y) = dialog.printer.cursor();
        let mut marked = Vec::new();
        for dy in 0..ARROW_HEIGHT_PX {
            for dx in 0..ARROW_WIDTH_PX {
                let (x, y) = box_pixel((cursor_x + dx, cursor_y + dy));
                match fb.pixel(x, y) {
                    Some(color) if color == OPAQUE_GLYPH_COLOR && dx == 0 => {
                        marked.push(usize::try_from(dy).unwrap());
                    }
                    Some(color) if color == FRAME_INTERIOR_COLOR => {}
                    other => panic!("unexpected pixel {other:?} at arrow offset ({dx}, {dy})"),
                }
            }
        }
        match marked.as_slice() {
            [] => None,
            [row] => Some(*row),
            rows => panic!("more than one frame marker drawn: {rows:?}"),
        }
    }

    #[test]
    fn a_page_clear_prompt_draws_the_arrow_a_frame_after_it_starts_waiting() {
        let mut dialog = arrow_dialog(vec![
            Token::Char('A'),
            Token::PromptClear,
            Token::Char('B'),
            Token::End,
        ]);
        tick_until_prompt(&mut dialog);
        // The tick that reached the `\p` only sets the state
        // (`text.c:1102-1109`); `TextPrinterWaitWithDownArrow` first runs on
        // the next one.
        assert!(!has_arrow_op(&dialog), "the arrow has not drawn yet");
        assert_eq!(dialog.tick(NO_INPUT), DialogOutcome::Continue);
        assert_eq!(drawn_arrow_frame(&dialog), Some(0));
    }

    #[test]
    fn the_arrow_bobs_to_its_next_frame_every_ninth_waiting_frame_and_wraps_at_four() {
        let mut dialog = arrow_dialog(vec![
            Token::Char('A'),
            Token::PromptClear,
            Token::Char('B'),
            Token::End,
        ]);
        tick_until_prompt(&mut dialog);
        assert_eq!(dialog.tick(NO_INPUT), DialogOutcome::Continue);

        let mut seen = Vec::new();
        for waiting_frame in 0..ARROW_REDRAW_INTERVAL * (ARROW_FRAMES + 1) {
            if waiting_frame % ARROW_REDRAW_INTERVAL == 0 {
                seen.push(drawn_arrow_frame(&dialog).expect("the arrow stays up while waiting"));
            } else {
                assert_eq!(
                    drawn_arrow_frame(&dialog),
                    seen.last().copied(),
                    "the frame holds between redraws (waiting frame {waiting_frame})"
                );
            }
            assert_eq!(dialog.tick(NO_INPUT), DialogOutcome::Continue);
        }
        assert_eq!(seen, [0, 1, 2, 3, 0]);
    }

    #[test]
    fn a_waiting_arrow_keeps_one_op_however_long_the_prompt_waits() {
        let mut dialog = arrow_dialog(vec![Token::Char('A'), Token::PromptClear, Token::End]);
        tick_until_prompt(&mut dialog);
        for _ in 0..ARROW_REDRAW_INTERVAL * 20 {
            let _ = dialog.tick(NO_INPUT);
        }
        let arrows = dialog
            .ops
            .iter()
            .filter(|op| matches!(op, WindowOp::DownArrow(_)))
            .count();
        assert_eq!(arrows, 1);
    }

    #[test]
    fn confirming_a_page_clear_with_a_or_b_removes_the_arrow() {
        for press in [A_PRESS, B_PRESS] {
            let mut dialog = arrow_dialog(vec![
                Token::Char('A'),
                Token::PromptClear,
                Token::Char('B'),
                Token::End,
            ]);
            tick_until_prompt(&mut dialog);
            let waiting_cursor = dialog.printer.cursor();
            assert_eq!(dialog.tick(NO_INPUT), DialogOutcome::Continue);
            assert_eq!(drawn_arrow_frame(&dialog), Some(0));

            assert_eq!(dialog.tick(press), DialogOutcome::Continue);
            assert!(!has_arrow_op(&dialog), "the page clear drops the arrow");
            let fb = dialog.compose_over(Framebuffer::new());
            for dy in 0..ARROW_HEIGHT_PX {
                for dx in 0..ARROW_WIDTH_PX {
                    let (x, y) = box_pixel((waiting_cursor.0 + dx, waiting_cursor.1 + dy));
                    assert_eq!(fb.pixel(x, y), Some(FRAME_INTERIOR_COLOR));
                }
            }
        }
    }

    #[test]
    fn a_scroll_prompt_draws_the_arrow_and_confirming_it_erases_the_arrow_and_the_glyph_under_it() {
        for press in [A_PRESS, B_PRESS] {
            let mut dialog = arrow_dialog(vec![
                Token::Char('A'),
                Token::PromptScroll,
                Token::Char('B'),
                Token::End,
            ]);
            tick_until_prompt(&mut dialog);
            let waiting_cursor = dialog.printer.cursor();
            assert!(!has_arrow_op(&dialog), "the arrow has not drawn yet");
            assert_eq!(dialog.tick(NO_INPUT), DialogOutcome::Continue);
            assert_eq!(drawn_arrow_frame(&dialog), Some(0));

            // `TextPrinterClearDownArrow` fills the rectangle at the cursor
            // the wait drew at, before the cursor returns to the margin
            // (`text.c:1180-1188`).
            assert_eq!(dialog.tick(press), DialogOutcome::Continue);
            assert_eq!(
                dialog.printer.cursor().0,
                textbox::STANDARD_PRINTER_ORIGIN.0
            );
            let fb = dialog.compose_over(Framebuffer::new());
            for dy in 0..ARROW_HEIGHT_PX {
                for dx in 0..ARROW_WIDTH_PX {
                    let (x, y) = box_pixel((waiting_cursor.0 + dx, waiting_cursor.1 + dy));
                    assert_eq!(fb.pixel(x, y), Some(FRAME_INTERIOR_COLOR));
                }
            }
        }
    }

    #[test]
    fn a_second_prompt_starts_its_own_arrow_from_frame_zero() {
        let mut dialog = arrow_dialog(vec![
            Token::Char('A'),
            Token::PromptClear,
            Token::Char('B'),
            Token::PromptClear,
            Token::End,
        ]);
        tick_until_prompt(&mut dialog);
        for _ in 0..=ARROW_REDRAW_INTERVAL {
            let _ = dialog.tick(NO_INPUT);
        }
        assert_eq!(drawn_arrow_frame(&dialog), Some(1));
        let _ = dialog.tick(A_PRESS);

        tick_until_prompt(&mut dialog);
        assert!(!has_arrow_op(&dialog), "the arrow has not drawn yet");
        assert_eq!(dialog.tick(NO_INPUT), DialogOutcome::Continue);
        assert_eq!(drawn_arrow_frame(&dialog), Some(0));
    }

    #[test]
    fn drawing_the_arrow_adds_no_frames_to_a_prompt() {
        // Instant speed: tick 1 prints 'A', tick 2 reaches the `\p`, tick 3
        // is the first waiting frame -- the one the arrow first draws on --
        // and its fresh A press already confirms. 'B' prints on tick 4 and
        // the stream ends on tick 5, exactly as without an arrow.
        let mut dialog = arrow_dialog(vec![
            Token::Char('A'),
            Token::PromptClear,
            Token::Char('B'),
            Token::End,
        ]);
        let mut closed_on = None;
        for frame in 1..=8 {
            let input = if frame == 3 { A_PRESS } else { NO_INPUT };
            if dialog.tick(input) == DialogOutcome::Closed {
                closed_on = Some(frame);
                break;
            }
        }
        assert_eq!(closed_on, Some(5));
    }

    #[test]
    fn a_press_on_the_tick_that_reaches_the_prompt_does_not_confirm_it() {
        let mut dialog = arrow_dialog(vec![
            Token::Char('A'),
            Token::PromptClear,
            Token::Char('B'),
            Token::End,
        ]);
        assert_eq!(dialog.tick(NO_INPUT), DialogOutcome::Continue);
        assert_eq!(dialog.tick(A_PRESS), DialogOutcome::Continue);
        assert_eq!(dialog.printer.cursor().0, NORMAL_A_ADVANCE_WIDTH);
        assert_eq!(dialog.revealed_glyph_count(), 1, "the page is still up");
    }

    /// The pack path end to end: a dialog opened from a pack draws the pack's
    /// own arrow sheet, and its bob follows `sDownArrowYCoords`' `0, 1, 2, 1`
    /// source rows (`text.c:75`), which move a marker at sheet row 3 to
    /// window rows 3, 2, 1, 2.
    #[test]
    fn a_dialog_opened_from_a_pack_bobs_the_packs_down_arrow() {
        use crate::pack_test_support::{
            image_entry, pack_bytes_with_fonts, palette_entry_with_color,
        };
        use rendering::Bgr555;

        const MARKER_SHEET_ROW: usize = 3;
        const ARROW_SHEET_WIDTH: u32 = 8;
        const ARROW_SHEET_HEIGHT: u32 = 48;
        const MARKER_COLOR: Bgr555 = Bgr555::from_raw(0x001F);

        let mut arrow_pixels = vec![0u8; (ARROW_SHEET_WIDTH * ARROW_SHEET_HEIGHT) as usize];
        arrow_pixels[MARKER_SHEET_ROW * ARROW_SHEET_WIDTH as usize] = 2;
        let bytes = pack_bytes_with_fonts(vec![
            image_entry("text-window/image/message_box", 56, 16, 4, 0),
            palette_entry_with_color("text-window/palette/message_box", 16, 2, MARKER_COLOR),
            pack_format::image_entry(
                "interface/image/down_arrow".into(),
                ARROW_SHEET_WIDTH,
                ARROW_SHEET_HEIGHT,
                4,
                arrow_pixels,
            )
            .unwrap(),
        ]);
        let path = std::env::temp_dir().join(format!(
            "pokeemerald-rs-field-dialog-down-arrow-{}-{:?}.pack",
            std::process::id(),
            std::thread::current().id()
        ));
        let pack = crate::overworld::test_support::with_scratch_pack(path, bytes, |pack| pack);

        let mut dialog = NpcDialog::from_pack_at_speed(
            &pack,
            vec![Token::PromptClear, Token::End],
            TextSpeed::Instant,
        )
        .expect("the synthetic pack carries every dialog asset");
        let mut white = Framebuffer::new();
        white.fill(Rgb888 {
            r: 255,
            g: 255,
            b: 255,
        });
        let marker = MARKER_COLOR.to_rgb888();

        let mut rows = Vec::new();
        for _ in 0..2 {
            assert_eq!(dialog.tick(NO_INPUT), DialogOutcome::Continue);
        }
        for _ in 0..ARROW_FRAMES {
            let fb = dialog.compose_over(white.clone());
            let (cursor_x, cursor_y) = dialog.printer.cursor();
            let rows_hit: Vec<i32> = (0..ARROW_HEIGHT_PX)
                .filter(|dy| {
                    let (x, y) = box_pixel((cursor_x, cursor_y + dy));
                    fb.pixel(x, y) == Some(marker)
                })
                .collect();
            assert_eq!(
                rows_hit.len(),
                1,
                "one marker pixel in column 0: {rows_hit:?}"
            );
            rows.push(rows_hit[0]);
            for _ in 0..ARROW_REDRAW_INTERVAL {
                assert_eq!(dialog.tick(NO_INPUT), DialogOutcome::Continue);
            }
        }
        assert_eq!(rows, [3, 2, 1, 2]);
    }

    /// Palette entries the colour-control regressions tell apart.
    const COLOR_BLUE_PALETTE_INDEX: usize = 8;
    const HIGHLIGHT_PALETTE_INDEX: usize = 4;
    const BLUE_COLOR: Rgb888 = Rgb888 { r: 0, g: 0, b: 240 };
    const HIGHLIGHT_COLOR: Rgb888 = Rgb888 {
        r: 240,
        g: 240,
        b: 0,
    };

    /// Runs a decoded stream to its end in an [`opaque_synthetic_dialog`]
    /// whose palette also holds `BLUE_COLOR` and `HIGHLIGHT_COLOR`, and
    /// composes it.
    fn compose_decoded(bytes: &[u8]) -> Framebuffer {
        let mut dialog = opaque_synthetic_dialog(engine::text::decode(bytes).unwrap());
        dialog.frame.palette[COLOR_BLUE_PALETTE_INDEX] = BLUE_COLOR;
        dialog.frame.palette[HIGHLIGHT_PALETTE_INDEX] = HIGHLIGHT_COLOR;
        for _ in 0..16 {
            dialog.tick(NO_INPUT);
        }
        dialog.compose_over(Framebuffer::new())
    }

    #[test]
    fn a_decoded_color_blue_stream_paints_its_glyph_with_palette_index_8() {
        const ENCODED_A: u8 = 0xBB;
        // `{COLOR BLUE}` is FC 01 08 (`pokeemerald/src/strings.c:1234`).
        let plain = compose_decoded(&[ENCODED_A, 0xFF]);
        let blue = compose_decoded(&[0xFC, 0x01, 0x08, ENCODED_A, 0xFF]);
        let (x, y) = box_pixel((
            textbox::STANDARD_PRINTER_ORIGIN.0 + 2,
            textbox::STANDARD_PRINTER_ORIGIN.1 + 8,
        ));
        assert_eq!(plain.pixel(x, y), Some(OPAQUE_GLYPH_COLOR));
        assert_eq!(blue.pixel(x, y), Some(BLUE_COLOR));
    }

    #[test]
    fn a_decoded_highlight_changes_the_color_a_clear_erases_with() {
        const ENCODED_A: u8 = 0xBB;
        // A, HIGHLIGHT 4, SHIFT_RIGHT 0, CLEAR 6: the erase back over the A.
        let stream = |highlight: &[u8]| {
            let mut bytes = vec![ENCODED_A];
            bytes.extend_from_slice(highlight);
            bytes.extend_from_slice(&[0xFC, 0x0D, 0x00, 0xFC, 0x11, 0x06, 0xFF]);
            bytes
        };
        let (x, y) = box_pixel((
            textbox::STANDARD_PRINTER_ORIGIN.0 + 2,
            textbox::STANDARD_PRINTER_ORIGIN.1 + 8,
        ));
        let default = compose_decoded(&stream(&[]));
        let highlighted = compose_decoded(&stream(&[0xFC, 0x02, 0x04]));
        assert_eq!(default.pixel(x, y), Some(FRAME_INTERIOR_COLOR));
        assert_eq!(highlighted.pixel(x, y), Some(HIGHLIGHT_COLOR));
    }

    #[test]
    fn a_decoded_fill_window_erases_the_whole_window_with_the_highlight_in_force() {
        const ENCODED_A: u8 = 0xBB;
        let composed = compose_decoded(&[ENCODED_A, 0xFC, 0x02, 0x04, 0xFC, 0x0F, 0xFF]);
        let (x, y) = box_pixel((40, 20));
        assert_eq!(composed.pixel(x, y), Some(HIGHLIGHT_COLOR));
        let (glyph_x, glyph_y) = box_pixel((
            textbox::STANDARD_PRINTER_ORIGIN.0 + 2,
            textbox::STANDARD_PRINTER_ORIGIN.1 + 8,
        ));
        assert_eq!(composed.pixel(glyph_x, glyph_y), Some(HIGHLIGHT_COLOR));
    }

    #[test]
    fn a_scroll_after_highlight_fills_the_exposed_rows_with_the_highlight() {
        // `ScrollWindow(..., PIXEL_FILL(bgColor))` (`pokeemerald/src/text.c:1196-1201`).
        let mut dialog = opaque_synthetic_dialog(vec![
            Token::Char('A'),
            Token::ExtCtrl {
                sub: 0x02,
                args: vec![4],
            },
            Token::PromptScroll,
            Token::End,
        ]);
        dialog.frame.palette[HIGHLIGHT_PALETTE_INDEX] = HIGHLIGHT_COLOR;
        for _ in 0..8 {
            dialog.tick(NO_INPUT);
        }
        dialog.tick(A_PRESS);
        for _ in 0..16 {
            dialog.tick(NO_INPUT);
        }
        let fb = dialog.compose_over(Framebuffer::new());
        let (x, y) = box_pixel((150, textbox::STANDARD_BOX_CONTENT_SIZE_PX.1 - 2));
        assert_eq!(fb.pixel(x, y), Some(HIGHLIGHT_COLOR));
    }

    #[test]
    fn a_scroll_after_highlight_zero_fills_the_exposed_rows_transparently() {
        // `ScrollWindow(..., PIXEL_FILL(0))` writes palette index 0, which a
        // window shows as transparent (`pokeemerald/src/text.c:1196-1201`), so
        // the scene beneath the box shows through the rows the scroll exposed.
        const SCENE_COLOR: Rgb888 = Rgb888 {
            r: 10,
            g: 200,
            b: 10,
        };
        let mut dialog = opaque_synthetic_dialog(vec![
            Token::Char('A'),
            Token::ExtCtrl {
                sub: 0x02,
                args: vec![0],
            },
            Token::PromptScroll,
            Token::End,
        ]);
        for _ in 0..8 {
            dialog.tick(NO_INPUT);
        }
        dialog.tick(A_PRESS);
        for _ in 0..16 {
            dialog.tick(NO_INPUT);
        }
        let mut base = Framebuffer::new();
        base.fill(SCENE_COLOR);
        let fb = dialog.compose_over(base);
        let (x, y) = box_pixel((150, textbox::STANDARD_BOX_CONTENT_SIZE_PX.1 - 2));
        assert_eq!(fb.pixel(x, y), Some(SCENE_COLOR));
    }

    /// `{SHIFT_DOWN n}A\l` then a long pause, as an open dialog: the scroll's
    /// 16 pixels finish within the pause.
    fn shifted_glyph_then_scroll(shift_down: u8) -> NpcDialog {
        opaque_synthetic_dialog(vec![
            Token::ExtCtrl {
                sub: 0x0E,
                args: vec![shift_down],
            },
            Token::Char('A'),
            Token::PromptScroll,
            Token::ExtCtrl {
                sub: 0x08,
                args: vec![u8::MAX],
            },
            Token::End,
        ])
    }

    /// Runs `dialog` to the prompt, answers it, and lets the scroll run out.
    fn scroll_past_the_prompt(dialog: &mut NpcDialog) {
        const FRAMES_AFTER_THE_ANSWER: usize = 8;
        dialog.tick(NO_INPUT);
        dialog.tick(NO_INPUT);
        dialog.tick(A_PRESS);
        for _ in 0..FRAMES_AFTER_THE_ANSWER {
            dialog.tick(NO_INPUT);
        }
    }

    /// The glyph-wide strip of the standard box's content, one colour per
    /// window row.
    fn glyph_column_rows(dialog: &NpcDialog) -> Vec<Vec<Option<Rgb888>>> {
        let composed = dialog.compose_over(Framebuffer::new());
        (0..textbox::STANDARD_BOX_CONTENT_SIZE_PX.1)
            .map(|row| {
                (0..i32::try_from(assets::fonts::GLYPH_SIZE).unwrap())
                    .map(|column| {
                        let (x, y) = box_pixel((column, row));
                        composed.pixel(x, y)
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn a_glyph_clipped_below_the_window_stays_hidden_after_a_scroll() {
        let background = glyph_column_rows(&opaque_synthetic_dialog(vec![Token::End]));
        // A lands at window y = 32, wholly below the 32-pixel window.
        let mut dialog = shifted_glyph_then_scroll(31);
        dialog.tick(NO_INPUT);
        assert_eq!(glyph_column_rows(&dialog), background, "before scroll");

        scroll_past_the_prompt(&mut dialog);

        assert_eq!(glyph_column_rows(&dialog), background, "after scroll");
    }

    #[test]
    fn a_glyph_straddling_the_bottom_edge_keeps_only_its_revealed_rows_after_a_scroll() {
        const LAST_WINDOW_ROW: usize = 31;
        const SCROLL_PX: usize = 16;
        let background = glyph_column_rows(&opaque_synthetic_dialog(vec![Token::End]));
        // A lands at window y = 31: only its top row is inside the window.
        let mut dialog = shifted_glyph_then_scroll(30);
        dialog.tick(NO_INPUT);
        let before = glyph_column_rows(&dialog);
        for (row, pixels) in before.iter().enumerate() {
            let expected = if row == LAST_WINDOW_ROW {
                assert_ne!(pixels, &background[row], "the revealed row is drawn");
                pixels
            } else {
                &background[row]
            };
            assert_eq!(pixels, expected, "before scroll row {row}");
        }

        // Answering the prompt erases the arrow's 8-pixel-wide rectangle at the
        // cursor first (`TextPrinterClearDownArrow`, `text.c:838-849`), which
        // overlaps the part of this synthetic glyph drawn past its advance.
        let arrow_left = usize::try_from(dialog.printer.cursor().0).unwrap();
        let mut moved_row = before[LAST_WINDOW_ROW].clone();
        for (column, pixel) in moved_row.iter_mut().enumerate().skip(arrow_left).take(8) {
            *pixel = background[LAST_WINDOW_ROW - SCROLL_PX][column];
        }
        assert_ne!(moved_row, background[LAST_WINDOW_ROW - SCROLL_PX]);

        scroll_past_the_prompt(&mut dialog);

        // The scroll moved the revealed row up; rows clipped at reveal never
        // come back.
        let after = glyph_column_rows(&dialog);
        for (row, pixels) in after.iter().enumerate() {
            let expected = if row == LAST_WINDOW_ROW - SCROLL_PX {
                &moved_row
            } else {
                &background[row]
            };
            assert_eq!(pixels, expected, "after scroll row {row}");
        }
    }

    // -- Auto-scroll waits draw no arrow (`text.c:865-884`) -------------------

    /// `EXT_CTRL_CODE_SKIP` and `EXT_CTRL_CODE_PAUSE_UNTIL_PRESS`
    /// (`pokeemerald/include/characters.h`; one argument byte and none).
    const SKIP: u8 = 0x12;
    const PAUSE_UNTIL_PRESS: u8 = 0x09;
    /// Waiting calls after the arming frame that leave an auto-scroll prompt
    /// waiting; the next one releases it (`TextPrinterWaitAutoMode`,
    /// `text.c:850-863`).
    const AUTO_WAITING_CALLS: usize = 49;

    /// [`arrow_dialog`] whose printer waits in auto-scroll mode.
    fn auto_arrow_dialog(tokens: Vec<Token>) -> NpcDialog {
        let dialog = arrow_dialog(tokens);
        NpcDialog {
            printer: dialog.printer.with_auto_scroll(),
            ..dialog
        }
    }

    /// A glyph, a wait, a second glyph: one stream per wait the printer has.
    fn wait_streams() -> [(&'static str, Vec<Token>); 3] {
        let stream = |wait| vec![Token::Char('A'), wait, Token::Char('B'), Token::End];
        [
            ("clear", stream(Token::PromptClear)),
            ("scroll", stream(Token::PromptScroll)),
            (
                "pause until press",
                stream(Token::ExtCtrl {
                    sub: PAUSE_UNTIL_PRESS,
                    args: vec![],
                }),
            ),
        ]
    }

    /// Ticks until the message closes, asserting no arrow on the way, and
    /// returns how many frames that took.
    fn frames_to_close_without_arrow(dialog: &mut NpcDialog) -> usize {
        for frame in 1..=ONE_GLYPH_PRINT_FRAME_LIMIT * 2 {
            let outcome = dialog.tick(NO_INPUT);
            assert!(!has_arrow_op(dialog), "an arrow after the release");
            if outcome == DialogOutcome::Closed {
                return frame;
            }
        }
        panic!("the message never closed");
    }

    #[test]
    fn an_auto_scroll_wait_draws_no_arrow_on_any_of_its_fifty_frames() {
        for (name, tokens) in wait_streams() {
            for input in [NO_INPUT, A_PRESS, B_PRESS] {
                let mut dialog = auto_arrow_dialog(tokens.clone());
                // The glyph, then the frame that reaches the wait.
                for _ in 0..2 {
                    assert_eq!(dialog.tick(NO_INPUT), DialogOutcome::Continue);
                    assert!(!has_arrow_op(&dialog), "{name}: arrow before the wait");
                }
                let waiting_cursor = dialog.printer.cursor();
                for call in 1..=AUTO_WAITING_CALLS {
                    assert_eq!(dialog.tick(input), DialogOutcome::Continue);
                    assert!(
                        !has_arrow_op(&dialog),
                        "{name}: arrow on waiting call {call}"
                    );
                    assert_eq!(dialog.revealed_glyph_count(), 1, "{name}: page up");
                    assert_eq!(dialog.printer.cursor(), waiting_cursor, "{name}: cursor");
                }
                assert_eq!(dialog.tick(input), DialogOutcome::Continue);
                assert!(
                    !has_arrow_op(&dialog),
                    "{name}: arrow on the releasing call"
                );
                let released = (dialog.revealed_glyph_count(), dialog.printer.cursor());
                let auto_rest = frames_to_close_without_arrow(&mut dialog);

                // A manual dialog confirmed on its first waiting frame ends up
                // in the same place and closes after the same frames.
                let mut manual = arrow_dialog(tokens.clone());
                for _ in 0..2 {
                    assert_eq!(manual.tick(NO_INPUT), DialogOutcome::Continue);
                }
                assert_eq!(manual.tick(A_PRESS), DialogOutcome::Continue);
                assert_eq!(
                    (manual.revealed_glyph_count(), manual.printer.cursor()),
                    released,
                    "{name}: the page advances as in manual mode"
                );
                let manual_rest = (1..=ONE_GLYPH_PRINT_FRAME_LIMIT * 2)
                    .find(|_| manual.tick(NO_INPUT) == DialogOutcome::Closed);
                assert_eq!(manual_rest, Some(auto_rest), "{name}: frames to close");
            }
        }
    }

    /// A scroll prompt whose cursor sits over the glyph before it:
    /// `TextPrinterClearDownArrow` fills the rectangle at the cursor on
    /// release even when no arrow was drawn (`text.c:1180-1188`), so pixels
    /// right of the cursor go and those left of it stay, in either mode.
    #[test]
    fn an_auto_scroll_release_erases_the_cursor_rectangle_as_manual_mode_does() {
        const CURSOR_X: i32 = 2;
        let tokens = vec![
            Token::Char('A'),
            Token::ExtCtrl {
                sub: SKIP,
                args: vec![2],
            },
            Token::PromptScroll,
            Token::Char('B'),
            Token::End,
        ];
        let pixel = |dialog: &NpcDialog, x: i32| {
            let (px, py) = box_pixel((x, 4));
            dialog.compose_over(Framebuffer::new()).pixel(px, py)
        };

        let mut auto = auto_arrow_dialog(tokens.clone());
        for _ in 0..2 {
            let _ = auto.tick(NO_INPUT);
        }
        assert_eq!(auto.printer.cursor().0, CURSOR_X);
        for _ in 0..AUTO_WAITING_CALLS {
            let _ = auto.tick(NO_INPUT);
            assert_eq!(pixel(&auto, CURSOR_X + 1), Some(OPAQUE_GLYPH_COLOR));
        }
        let _ = auto.tick(NO_INPUT);

        let mut manual = arrow_dialog(tokens);
        for _ in 0..2 {
            let _ = manual.tick(NO_INPUT);
        }
        let _ = manual.tick(A_PRESS);

        for dialog in [&auto, &manual] {
            assert_eq!(pixel(dialog, CURSOR_X - 1), Some(OPAQUE_GLYPH_COLOR));
            assert_eq!(pixel(dialog, CURSOR_X + 1), Some(FRAME_INTERIOR_COLOR));
        }
        let window = |dialog: &NpcDialog| {
            let fb = dialog.compose_over(Framebuffer::new());
            let (width, height) = textbox::STANDARD_BOX_CONTENT_SIZE_PX;
            (0..height)
                .flat_map(|y| (0..width).map(move |x| box_pixel((x, y))))
                .map(|(x, y)| fb.pixel(x, y))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            window(&auto),
            window(&manual),
            "the released window holds the same pixels in both modes"
        );
    }
}
