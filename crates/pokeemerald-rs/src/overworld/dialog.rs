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

/// Default cadence for a caller not yet threading a saved [`TextSpeed`]
/// through [`NpcDialog::open`]/[`NpcDialog::from_pack`]. Upstream always
/// paces field text via `GetPlayerTextSpeedDelay()`
/// (`pokeemerald/src/menu.c:191-196,481-488`); see
/// [`NpcDialog::open_at_speed`].
const FIELD_SCRIPT_TEXT_SPEED: TextSpeed = TextSpeed::Mid;

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
    /// NPC and save messages share this type because both use the standard field
    /// message resources. `text_speed` stays caller-supplied: save messages and
    /// the ordinary field-dialog path ([`Self::open_at_speed`]/
    /// [`Self::from_pack_at_speed`]) pass the saved, normalized option, while a
    /// caller still on [`Self::open`]/[`Self::from_pack`] gets
    /// [`FIELD_SCRIPT_TEXT_SPEED`]. Holding A or B accelerates printing.
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

    /// Builds a confirm-to-close dialog from an already-loaded asset pack, at
    /// the default [`FIELD_SCRIPT_TEXT_SPEED`] cadence.
    ///
    /// # Errors
    ///
    /// Returns [`NpcDialogError::Pack`] for missing or malformed pack entries,
    /// and [`NpcDialogError::Font`] when the font sheet cannot be decoded.
    pub(crate) fn from_pack(pack: &AssetPack, tokens: Vec<Token>) -> Result<Self, NpcDialogError> {
        Self::from_pack_at_speed(pack, tokens, FIELD_SCRIPT_TEXT_SPEED)
    }

    /// [`Self::from_pack`], at a caller-chosen `text_speed` rather than the
    /// [`FIELD_SCRIPT_TEXT_SPEED`] default -- the saved, normalized option for
    /// an ordinary field dialog (issue #1392), mirroring how upstream's
    /// `AddTextPrinterForMessage` always paces field text through
    /// `GetPlayerTextSpeedDelay()` (`pokeemerald/src/menu.c:191-196,481-488`).
    ///
    /// # Errors
    ///
    /// Returns [`NpcDialogError::Pack`] for missing or malformed pack entries,
    /// and [`NpcDialogError::Font`] when the font sheet cannot be decoded.
    pub(crate) fn from_pack_at_speed(
        pack: &AssetPack,
        tokens: Vec<Token>,
        text_speed: TextSpeed,
    ) -> Result<Self, NpcDialogError> {
        let sheet = OwnedFontGlyphSheet::new(pack.font(FontId::Normal)?)?;
        let frame = FrameAssets::from_handle(pack.message_box()?);
        Ok(Self::new(sheet, frame, tokens, text_speed).with_waitbuttonpress())
    }

    /// Loads an asset pack and opens a confirm-to-close dialog, at the
    /// default [`FIELD_SCRIPT_TEXT_SPEED`] cadence.
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

    /// [`Self::open`], at a caller-chosen `text_speed` -- see
    /// [`Self::from_pack_at_speed`].
    ///
    /// # Errors
    ///
    /// Returns [`NpcDialogError::Pack`] when the pack cannot be loaded or read,
    /// and [`NpcDialogError::Font`] when the font sheet cannot be decoded.
    pub(crate) fn open_at_speed(
        source: crate::pack_source::PackSource,
        tokens: Vec<Token>,
        text_speed: TextSpeed,
    ) -> Result<Self, NpcDialogError> {
        let pack = source.load()?;
        Self::from_pack_at_speed(&pack, tokens, text_speed)
    }

    /// Returns the number of glyphs currently visible on screen.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn revealed_glyph_count(&self) -> usize {
        self.revealed.len()
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
            self.revealed.clear();
        }
        match event {
            TickEvent::Glyph(g) => self.revealed.push(*g),
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
    synthetic_dialog_at_speed(tokens, TextSpeed::Mid)
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
    let sheet = OwnedFontGlyphSheet::new(FontImageRef::new_for_tests(FontId::Normal, image))
        .expect("this is the exact real glyph-sheet shape");
    let frame = FrameAssets {
        pixels: vec![0u8; (FRAME_WIDTH * FRAME_HEIGHT) as usize],
        width: FRAME_WIDTH,
        height: FRAME_HEIGHT,
        palette: vec![Rgb888::BLACK; FRAME_PALETTE_SIZE],
    };
    NpcDialog::new(sheet, frame, tokens, speed)
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
        assert_eq!(dialog.revealed.len(), 1);
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
        let only_glyph = dialog.revealed[0];
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

    /// The smallest pack [`NpcDialog::from_pack_at_speed`] accepts: the
    /// standard message box frame and the normal font sheet. Mirrors
    /// `crate::start_menu::tests::synthetic_start_menu_pack_bytes`'s own
    /// fixture.
    fn synthetic_field_dialog_pack_bytes() -> Vec<u8> {
        use crate::pack_test_support::{image_entry, pack_bytes, palette_entry};

        const MESSAGE_BOX_WIDTH: u32 = 56;
        const MESSAGE_BOX_HEIGHT: u32 = 16;
        const FRAME_BIT_DEPTH: u8 = 4;
        const FONT_BIT_DEPTH: u8 = 2;
        const PALETTE_COLOUR_COUNT: u16 = 16;

        pack_bytes(vec![
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

    /// Issue #1392 regression: `AddTextPrinterForMessage` -- the printer
    /// every ordinary field message goes through
    /// (`pokeemerald/src/field_message_box.c:117-128`) -- paces at
    /// `GetPlayerTextSpeedDelay()` (`pokeemerald/src/menu.c:191-196`), which
    /// reads the saved `optionsTextSpeed` (`:481-488`). A field dialog
    /// opened through [`NpcDialog::from_pack_at_speed`] (the entry point
    /// `super::OverworldPhase::field_dialog_text_speed` feeds) must
    /// therefore take its cadence from the caller-supplied, normalized
    /// speed rather than the fixed [`FIELD_SCRIPT_TEXT_SPEED`] default, so a
    /// session saved FAST prints the same message in fewer frames than one
    /// saved SLOW (`sTextSpeedFrameDelays`: 1 vs 8 frames a glyph).
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

        let path = std::env::temp_dir().join(format!(
            "pokeemerald-rs-field-dialog-text-speed-{}-{:?}.pack",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::write(&path, synthetic_field_dialog_pack_bytes())
            .expect("the scratch directory is writable");
        let pack = AssetPack::load(&path).expect("the synthetic field-dialog pack must load");
        let _ = std::fs::remove_file(&path);

        let mut fast_dialog =
            NpcDialog::from_pack_at_speed(&pack, tokens(), TextSpeed::from_raw_option(2))
                .expect("the synthetic pack carries the dialog's font and message box");
        let mut slow_dialog =
            NpcDialog::from_pack_at_speed(&pack, tokens(), TextSpeed::from_raw_option(0))
                .expect("the synthetic pack carries the dialog's font and message box");

        let fast = frames_to_reveal(&mut fast_dialog, MESSAGE_GLYPHS);
        let slow = frames_to_reveal(&mut slow_dialog, MESSAGE_GLYPHS);

        assert!(
            fast < slow,
            "a field dialog given the FAST saved option must print in fewer frames than one \
             given SLOW, but they took {fast} and {slow} frames"
        );
    }

    /// [`NpcDialog::open`]/[`NpcDialog::from_pack`] must keep defaulting to
    /// [`FIELD_SCRIPT_TEXT_SPEED`] -- the sight-trainer intro speech
    /// (`crate::flow::overworld_phase::sight_trainer_approach`) still calls
    /// the two-argument form and must not change cadence out from under it.
    #[test]
    fn from_pack_still_defaults_to_the_field_script_text_speed() {
        const MESSAGE_GLYPHS: usize = 1;

        let path = std::env::temp_dir().join(format!(
            "pokeemerald-rs-field-dialog-default-speed-{}-{:?}.pack",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::write(&path, synthetic_field_dialog_pack_bytes())
            .expect("the scratch directory is writable");
        let pack = AssetPack::load(&path).expect("the synthetic field-dialog pack must load");
        let _ = std::fs::remove_file(&path);

        let mut default_dialog = NpcDialog::from_pack(&pack, vec![Token::Char('H'), Token::End])
            .expect("the synthetic pack carries the dialog's font and message box");
        let mut mid_dialog = NpcDialog::from_pack_at_speed(
            &pack,
            vec![Token::Char('H'), Token::End],
            FIELD_SCRIPT_TEXT_SPEED,
        )
        .expect("the synthetic pack carries the dialog's font and message box");

        assert_eq!(
            frames_to_reveal(&mut default_dialog, MESSAGE_GLYPHS),
            frames_to_reveal(&mut mid_dialog, MESSAGE_GLYPHS),
            "from_pack's default speed must still be FIELD_SCRIPT_TEXT_SPEED"
        );
    }
}
