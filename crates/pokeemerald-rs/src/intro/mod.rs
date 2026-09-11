//! Birch's new-game introduction.
//!
//! # Reduced cinematic
//!
//! [`IntroScene`] renders the dialogue and standard message box. The scene
//! omits the character animation, gender and naming screens, confirmation
//! menu, and audio. Questions held on screen by those omitted transitions
//! wait for A or B before the scene continues.
//!
//! # Advance
//!
//! A and B advance printer waits, and holding either button accelerates text.
//!
//! # Traversal pacing
//!
//! [`TRAVERSAL_RUNS`] records the printer-derived timing used by scripted runs.

mod speech;

use assets::fonts::{FontId, OwnedFontGlyphSheet};
use assets::pack::{AssetPack, PackError};
use engine::text::render::{Printer, PrinterInput, RevealedGlyph, TextSpeed, TickEvent};
use engine::text::window::MessageBoxLayout;
use engine::text::Token;
use rendering::{Framebuffer, Rgb888};

use crate::textbox::{self, FrameAssets};

pub use speech::NUM_PAGES;

/// A consecutive span of frames without input while reading Birch's speech.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraversalRun {
    /// Frames elapsed before the printer waits or continues automatically.
    pub frames: u32,
    /// Whether the next frame must contain an A or B press.
    pub confirm_after: bool,
}

/// Printer-derived timing for a full read at [`TextSpeed::Mid`] without held input.
pub const TRAVERSAL_RUNS: &[TraversalRun] = &[
    waits_for_confirmation(121),
    waits_for_confirmation(132),
    waits_for_confirmation(72),
    waits_for_confirmation(179),
    advances_automatically(4),
    waits_for_confirmation(230),
    advances_automatically(4),
    waits_for_confirmation(244),
    waits_for_confirmation(279),
    advances_automatically(9),
    waits_for_confirmation(140),
    waits_for_confirmation(235),
    waits_for_confirmation(267),
    waits_for_confirmation(235),
    waits_for_confirmation(247),
    advances_automatically(9),
    waits_for_confirmation(72),
    advances_automatically(4),
    waits_for_confirmation(49),
    advances_automatically(4),
    waits_for_confirmation(112),
    advances_automatically(4),
    waits_for_confirmation(49),
    advances_automatically(4),
    waits_for_confirmation(37),
    waits_for_confirmation(215),
    advances_automatically(9),
    waits_for_confirmation(56),
    advances_automatically(4),
    waits_for_confirmation(101),
    waits_for_confirmation(175),
    waits_for_confirmation(251),
    advances_automatically(9),
    waits_for_confirmation(136),
    waits_for_confirmation(263),
    advances_automatically(4),
];

const fn waits_for_confirmation(frames: u32) -> TraversalRun {
    TraversalRun {
        frames,
        confirm_after: true,
    }
}

const fn advances_automatically(frames: u32) -> TraversalRun {
    TraversalRun {
        frames,
        confirm_after: false,
    }
}

/// Total frames for [`TRAVERSAL_RUNS`], including required confirmation frames.
pub const TRAVERSAL_FRAMES: usize = traversal_frames(TRAVERSAL_RUNS);

const fn traversal_frames(runs: &[TraversalRun]) -> usize {
    let mut total = 0;
    let mut i = 0;
    while i < runs.len() {
        total += runs[i].frames as usize;
        if runs[i].confirm_after {
            total += 1;
        }
        i += 1;
    }
    total
}

/// An error while building an [`IntroScene`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntroSceneError {
    /// The asset pack could not be loaded or read.
    Pack(PackError),
    /// The font glyph sheet could not be decoded.
    Font(assets::AssetError),
}

impl std::fmt::Display for IntroSceneError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pack(err) => write!(f, "intro: {err}"),
            Self::Font(err) => write!(f, "intro: {err}"),
        }
    }
}

impl std::error::Error for IntroSceneError {}

impl From<PackError> for IntroSceneError {
    fn from(err: PackError) -> Self {
        Self::Pack(err)
    }
}

impl From<assets::AssetError> for IntroSceneError {
    fn from(err: assets::AssetError) -> Self {
        Self::Font(err)
    }
}

impl IntroSceneError {
    /// Returns whether the asset pack was not found.
    #[must_use]
    pub const fn is_pack_missing(&self) -> bool {
        matches!(self, Self::Pack(PackError::NotFound(_)))
    }
}

/// The introduction's progress towards the overworld handoff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntroStatus {
    /// The current page or a later page still needs processing.
    Continue,
    /// Every page has finished.
    Finished,
}

/// Birch's introduction speech, rendered one page at a time.
#[derive(Debug)]
pub struct IntroScene {
    frame: FrameAssets,
    pages: [Vec<Token>; NUM_PAGES],
    page_index: usize,
    printer: Printer<OwnedFontGlyphSheet>,
    revealed: Vec<RevealedGlyph>,
    finished: bool,
}

impl IntroScene {
    /// Creates an introduction from decoded rendering assets at `speed`.
    #[must_use]
    pub(crate) fn new(sheet: OwnedFontGlyphSheet, frame: FrameAssets, speed: TextSpeed) -> Self {
        let pages = speech::pages();
        // The upstream speech printer enables held-A/B acceleration (src/main_menu.c:1339).
        let printer = Printer::new(
            pages[0].clone(),
            sheet,
            speed,
            textbox::STANDARD_PRINTER_ORIGIN,
        )
        .with_ab_speed_up_print();
        Self {
            frame,
            pages,
            page_index: 0,
            printer,
            revealed: Vec::new(),
            finished: false,
        }
    }

    /// Loads owned rendering assets from `pack` at the default text speed.
    ///
    /// # Errors
    ///
    /// Returns [`IntroSceneError::Pack`] if an asset is missing or malformed,
    /// or [`IntroSceneError::Font`] if the font sheet cannot be decoded.
    pub fn from_pack(pack: &AssetPack) -> Result<Self, IntroSceneError> {
        let sheet = OwnedFontGlyphSheet::new(pack.font(FontId::Normal)?)?;
        let frame = FrameAssets::from_handle(pack.message_box()?);
        Ok(Self::new(sheet, frame, TextSpeed::Mid))
    }

    /// Returns the current page index in `0..NUM_PAGES`.
    #[must_use]
    pub const fn page_index(&self) -> usize {
        self.page_index
    }

    /// Returns whether every page has finished.
    #[must_use]
    pub const fn is_finished(&self) -> bool {
        self.finished
    }

    /// Returns the number of glyphs currently visible.
    #[must_use]
    pub fn revealed_glyph_count(&self) -> usize {
        self.revealed.len()
    }

    /// Advances the introduction by one frame.
    ///
    /// Once this returns [`IntroStatus::Finished`], later calls also return it.
    pub fn tick(&mut self, input: PrinterInput) -> IntroStatus {
        if self.finished {
            return IntroStatus::Finished;
        }

        match self.printer.tick(input) {
            TickEvent::Glyph(g) => self.revealed.push(*g),
            TickEvent::Cleared => self.revealed.clear(),
            TickEvent::Scrolling { dy } => {
                for g in &mut self.revealed {
                    g.y -= dy;
                }
            }
            TickEvent::Finished => self.advance_page(),
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

        if self.finished {
            IntroStatus::Finished
        } else {
            IntroStatus::Continue
        }
    }

    fn advance_page(&mut self) {
        if self.page_index + 1 < NUM_PAGES {
            self.page_index += 1;
            self.revealed.clear();
            self.printer.restart(self.pages[self.page_index].clone());
        } else {
            self.finished = true;
        }
    }

    /// Renders the dialogue box and visible glyphs into a new framebuffer.
    #[must_use]
    pub fn compose(&self) -> Framebuffer {
        let mut fb = Framebuffer::new();
        fb.fill(Rgb888::BLACK);

        let tiles = MessageBoxLayout::STANDARD.frame_tiles();
        textbox::blit_frame_tiles(&mut fb, &tiles, self.frame.image(), &self.frame.palette);
        textbox::blit_glyphs(
            &mut fb,
            &self.revealed,
            textbox::STANDARD_BOX_SCREEN_ORIGIN,
            textbox::STANDARD_BOX_CONTENT_SIZE_PX,
        );

        fb
    }

    /// Renders a presentation-ready frame.
    #[must_use]
    pub fn compose_frame(&self) -> Box<platform::Frame> {
        crate::frame::to_platform_frame(&self.compose())
    }
}

/// Loads an introduction from the default asset pack.
///
/// # Errors
///
/// Returns an error if the pack cannot be loaded or its rendering assets
/// cannot be decoded.
pub fn load_default() -> Result<IntroScene, IntroSceneError> {
    load(crate::pack_source::PackSource::Runtime)
}

/// Loads an introduction from `source`.
///
/// # Errors
///
/// Returns an error if the pack cannot be loaded or its rendering assets
/// cannot be decoded.
pub(crate) fn load(source: crate::pack_source::PackSource) -> Result<IntroScene, IntroSceneError> {
    let pack = source.load()?;
    IntroScene::from_pack(&pack)
}

/// Returns a synthetic, finished introduction for flow tests.
#[cfg(test)]
pub(crate) fn synthetic_finished_scene() -> IntroScene {
    use assets::fonts::FontImageRef;
    use assets::pack::ImageRef;

    const SHEET_WIDTH: u32 = 256;
    const SHEET_HEIGHT: u32 = 512;
    const FRAME_WIDTH: u32 = 56;
    const FRAME_HEIGHT: u32 = 16;
    const MAX_TICKS_TO_FINISH: usize = 5_000;
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
        pixels: vec![0u8; (FRAME_WIDTH * FRAME_HEIGHT) as usize],
        width: FRAME_WIDTH,
        height: FRAME_HEIGHT,
        palette: vec![Rgb888::BLACK; 16],
    };
    let mut scene = IntroScene::new(sheet, frame, TextSpeed::Instant);
    let confirm_a = PrinterInput {
        a_pressed: true,
        b_pressed: false,
        a_held: true,
        b_held: false,
    };
    let mut status = IntroStatus::Continue;
    for _ in 0..MAX_TICKS_TO_FINISH {
        status = scene.tick(confirm_a);
        if status == IntroStatus::Finished {
            break;
        }
    }
    debug_assert_eq!(status, IntroStatus::Finished);
    scene
}

#[cfg(test)]
mod tests;
