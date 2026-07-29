//! The new-game intro: Birch's speech (I-3, issue #149), transcribed from
//! upstream's `Task_NewGameBirchSpeech_*` chain
//! (`pokeemerald/src/main_menu.c:1279-1755`).
//!
//! [`speech`] holds the actual dialogue text (module docs there for the
//! exact upstream strings and the naming/gender-selection deviations);
//! [`IntroScene`] drives it, one [`engine::text::render::Printer`] per page,
//! through the same [`crate::textbox`] pixel-blit path
//! [`crate::main_menu::MainMenuScene`] uses for its own text window.
//!
//! # Reduced cinematic -- what's rendered and what's deferred
//!
//! In scope: the real speech text, paged with upstream's own `\p`/`\l`
//! pacing (wait for a button press, then clear/scroll -- see
//! [`engine::text::render::Printer`]'s module docs for the exact frame
//! timing this reuses unmodified), inside the standard dialogue box
//! (`engine::text::window::MessageBoxLayout::STANDARD`, the same layout a
//! future NPC-interaction slice will reuse). Out of scope, matching the
//! issue's own "reduced but faithful subset" allowance:
//!
//! - **No Birch/Lotad/player sprites, no platform background, no palette
//!   fades/slides.** Upstream's task chain spends most of its state
//!   machine animating `AddBirchSpeechObjects`' sprites in and out
//!   (`Task_NewGameBirchSpeech_WaitToShowBirch` through
//!   `Task_NewGameBirchSpeech_FadePlayerToWhite`) around the dialogue --
//!   none of that is rendered here. `graphics/birch_speech` (the shadow/map
//!   background graphics) stays untouched in the coverage ledger for
//!   exactly this reason.
//! - **No gender-select menu, no naming screen.** See
//!   `crate::new_game`'s module docs -- the speech pages that would
//!   normally frame those UI steps ([`speech::pages`]'s pages 4/5, "And you
//!   are?"/"What's your name?") still print, just with nothing interactive
//!   in between.
//! - **No music, no sound effects.** `PlayBGM(MUS_ROUTE122)` and every
//!   `PlaySE` call in the task chain are silent here (no audio wiring in
//!   this slice).
//!
//! # Advance and skip
//!
//! [`IntroScene::tick`] takes two per-frame button edges: `confirm_pressed`
//! (upstream's A-button `JOY_NEW` check inside
//! `TextPrinterWaitWithDownArrow`, which both `\p` and `\l` wait on) and
//! `skip_pressed` -- a v1-only convenience with **no upstream analogue**
//! (real Emerald's intro cannot be skipped outright; the closest it gets is
//! `WhatsYourName`'s own wait state also accepting B,
//! `main_menu.c:1590`). Since this slice already elides the naming/gender
//! UI, a full-intro skip does not skip anything upstream would have made
//! interactive -- it only lets a player (or a test) reach the overworld
//! without re-confirming all eight pages. Wired to the B button in
//! [`crate::app`].

mod speech;

use assets::fonts::{FontGlyphSheet, FontId};
use assets::pack::{AssetPack, PackError};
use engine::text::render::{Printer, RevealedGlyph, TextSpeed, TickEvent};
use engine::text::window::MessageBoxLayout;
use engine::text::Token;
use rendering::{Framebuffer, Rgb888};

use crate::textbox::{self, FrameAssets};

pub use speech::NUM_PAGES;

/// The dialogue box's window-local text origin (a small inset from the
/// content rect's own top-left corner, matching the margin
/// `AddTextPrinterForMessage`'s standard field message box uses -- upstream
/// `sTextFlags`/`x = 1` in spirit, not transcribed byte-for-byte).
const PRINTER_ORIGIN: (i32, i32) = (2, 2);

/// `MessageBoxLayout::STANDARD`'s content rect, converted to absolute
/// screen pixels (tile -> 8px), for [`textbox::blit_glyphs`]'s `origin`.
const BOX_SCREEN_ORIGIN: (i32, i32) = (
    engine::text::window::STANDARD_TILEMAP_LEFT * 8,
    engine::text::window::STANDARD_TILEMAP_TOP * 8,
);

/// Why building [`IntroScene`] failed.
///
/// Concrete per-crate-boundary enum `(oop-boundaries)`, mirroring
/// [`crate::main_menu::MainMenuSceneError`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntroSceneError {
    /// Loading or reading the asset pack failed -- most commonly
    /// [`PackError::NotFound`] (see [`IntroSceneError::is_pack_missing`]).
    Pack(PackError),
    /// The font glyph sheet fetched from the pack didn't decode.
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
    /// Whether this is specifically the "no pack on disk" diagnostic --
    /// mirrors [`crate::title::TitleSceneError::is_pack_missing`].
    #[must_use]
    pub const fn is_pack_missing(&self) -> bool {
        matches!(self, Self::Pack(PackError::NotFound(_)))
    }
}

/// Whether the intro is still running or has handed off to the overworld
/// (module docs' "Advance and skip").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntroStatus {
    /// More pages remain (or the current one is still printing/paging).
    Continue,
    /// Every page finished, or [`IntroScene::tick`] was called with
    /// `skip_pressed`. The caller should transition to the overworld.
    Finished,
}

/// Birch's speech, one paged [`engine::text::render::Printer`] at a time
/// (module docs).
pub struct IntroScene<'a> {
    frame: FrameAssets,
    sheet: FontGlyphSheet<'a>,
    speed: TextSpeed,
    pages: [Vec<Token>; NUM_PAGES],
    page_index: usize,
    printer: Printer<'a>,
    revealed: Vec<RevealedGlyph>,
    finished: bool,
}

impl<'a> IntroScene<'a> {
    /// Build a fresh intro over an already-decoded font `sheet` and dialogue
    /// `frame`, printing at `speed`.
    ///
    /// A plain constructor rather than `from_pack` (contrast
    /// [`crate::main_menu::MainMenuScene::from_pack`]): `sheet` must
    /// outlive every [`IntroScene::tick`] call (the printer persists across
    /// frames, unlike [`crate::main_menu::MainMenuScene`]'s throwaway,
    /// drive-to-completion-once printer), so [`load_default`] leaks its
    /// [`AssetPack`] to get a `'static` `sheet` rather than borrowing one
    /// transiently -- see [`load_default`]'s doc comment.
    ///
    /// `pub(crate)`, not `pub`: [`FrameAssets`] (its `frame` parameter) is
    /// itself crate-private (see its own docs), and every real caller goes
    /// through [`load_default`] anyway -- this constructor exists as a seam
    /// for [`load_default`] and this crate's own synthetic-fixture tests.
    #[must_use]
    pub(crate) fn new(sheet: FontGlyphSheet<'a>, frame: FrameAssets, speed: TextSpeed) -> Self {
        let pages = speech::pages();
        let printer = Printer::new(pages[0].clone(), sheet, speed, PRINTER_ORIGIN);
        Self {
            frame,
            sheet,
            speed,
            pages,
            page_index: 0,
            printer,
            revealed: Vec::new(),
            finished: false,
        }
    }

    /// The current page index (`0..NUM_PAGES`), for tests/diagnostics.
    #[must_use]
    pub const fn page_index(&self) -> usize {
        self.page_index
    }

    /// Whether every page has finished (or [`IntroScene::tick`] was ever
    /// called with `skip_pressed`).
    #[must_use]
    pub const fn is_finished(&self) -> bool {
        self.finished
    }

    /// How many glyphs are currently visible on screen (cleared on a `\p`
    /// page break, shifted -- not cleared -- by a `\l` scroll). Exposed for
    /// tests; [`IntroScene::compose`] is the production consumer.
    #[must_use]
    pub fn revealed_glyph_count(&self) -> usize {
        self.revealed.len()
    }

    /// Advance the intro by exactly one frame.
    ///
    /// `confirm_pressed` is the A-button just-pressed edge, forwarded
    /// straight to the current page's [`Printer::tick`] (module docs).
    /// `skip_pressed` is the v1-only whole-intro skip (module docs) -- once
    /// [`IntroStatus::Finished`] is returned, every further call returns it
    /// again without doing anything (mirrors
    /// [`engine::text::render::Printer::is_finished`]'s own terminal
    /// contract).
    pub fn tick(&mut self, confirm_pressed: bool, skip_pressed: bool) -> IntroStatus {
        if self.finished {
            return IntroStatus::Finished;
        }
        if skip_pressed {
            self.finished = true;
            return IntroStatus::Finished;
        }

        match self.printer.tick(confirm_pressed) {
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
            | TickEvent::AwaitingClear => {}
        }

        if self.finished {
            IntroStatus::Finished
        } else {
            IntroStatus::Continue
        }
    }

    /// Move to the next page's fresh [`Printer`], or mark the intro
    /// finished if [`Self::page_index`] was already the last page.
    fn advance_page(&mut self) {
        if self.page_index + 1 < NUM_PAGES {
            self.page_index += 1;
            self.revealed.clear();
            self.printer = Printer::new(
                self.pages[self.page_index].clone(),
                self.sheet,
                self.speed,
                PRINTER_ORIGIN,
            );
        } else {
            self.finished = true;
        }
    }

    /// Composite the dialogue box and every currently-revealed glyph into a
    /// fresh [`Framebuffer`].
    #[must_use]
    pub fn compose(&self) -> Framebuffer {
        let mut fb = Framebuffer::new();
        fb.fill(Rgb888::BLACK);

        let tiles = MessageBoxLayout::STANDARD.frame_tiles();
        textbox::blit_frame_tiles(&mut fb, &tiles, self.frame.image(), &self.frame.palette);
        textbox::blit_glyphs(&mut fb, &self.revealed, BOX_SCREEN_ORIGIN);

        fb
    }

    /// [`compose`](Self::compose), converted to `platform`'s
    /// presentation-ready pixel format -- mirrors
    /// [`crate::title::TitleScene::compose_frame`].
    #[must_use]
    pub fn compose_frame(&self) -> Box<platform::Frame> {
        crate::frame::to_platform_frame(&self.compose())
    }
}

/// Fetch [`IntroScene`]'s two required entries -- the normal-weight font
/// sheet and the dialogue frame -- out of an already-loaded `pack`, without
/// leaking anything. Shared by [`load_default`] for both its pre-leak
/// validation pass and its real, post-leak build (that doc comment's
/// "validate before leak" section) -- a borrowed `pack` here, not the
/// `'static` one [`IntroScene`] itself ultimately needs, so this alone can
/// never be the thing that leaks.
///
/// # Errors
///
/// [`IntroSceneError::Pack`] if `pack` is missing its `font/normal/glyphs`
/// or message-box entries (or either is malformed); [`IntroSceneError::Font`]
/// if the font sheet fetched doesn't decode.
fn required_assets(pack: &AssetPack) -> Result<(FontGlyphSheet<'_>, FrameAssets), IntroSceneError> {
    let font_image = pack.font(FontId::Normal)?;
    let sheet = FontGlyphSheet::new(font_image)?;
    let frame = FrameAssets::from_handle(pack.message_box()?);
    Ok((sheet, frame))
}

/// Load the pack from its default location and build the intro out of it in
/// one step -- mirrors [`crate::title::load_default`].
///
/// Leaks the loaded [`AssetPack`] (`Box::leak`) to obtain the `'static`
/// [`FontGlyphSheet`] [`IntroScene`]'s persistent, per-frame
/// [`engine::text::render::Printer`] needs to borrow across
/// [`IntroScene::tick`] calls -- unlike [`crate::title::TitleScene`]/
/// [`crate::overworld::OverworldScene`], which copy every pack byte they
/// need into owned buffers up front, a live `Printer` cannot be rebuilt from
/// scratch every frame without losing its mid-page position. A one-time
/// leak of a single, small (~150KiB), load-once-per-process pack is the
/// pragmatic tradeoff, not a per-frame or unbounded leak -- **provided** the
/// pack is actually usable.
///
/// **Validate before leak.** This function is `pub`, and
/// [`crate::flow::advance_scene`]'s `MainMenu` arm calls it again on every
/// fresh A press for as long as the player keeps retrying against a
/// still-broken pack (its own "log-or-ignore" policy) -- so every required
/// entry ([`required_assets`]) is fetched against the freshly loaded, *not
/// yet* leaked `pack` first. Only once that succeeds does this commit `pack`
/// to its `'static` leak; a malformed pack (module docs' own example, a
/// missing `message_box` entry) returns an error here without ever leaking,
/// instead of leaking a fresh, immediately unusable [`AssetPack`] on every
/// such call.
///
/// # Errors
///
/// [`IntroSceneError::Pack`] with [`IntroSceneError::is_pack_missing`] true
/// if no pack has been extracted yet; see [`required_assets`] for the other
/// (real-pack-only) error cases.
///
/// # Panics
///
/// Never in practice: the second [`required_assets`] call re-reads the exact
/// same, now-`'static`, bytes the first call (against the same pack, still
/// owned locally at that point) already validated successfully -- nothing
/// about leaking a value changes its contents.
pub fn load_default() -> Result<IntroScene<'static>, IntroSceneError> {
    let pack = AssetPack::load_default()?;
    required_assets(&pack)?;

    let pack: &'static AssetPack = Box::leak(Box::new(pack));
    let (sheet, frame) =
        required_assets(pack).expect("already validated identically against the same bytes above");
    Ok(IntroScene::new(sheet, frame, TextSpeed::Mid))
}

/// Test-only: an [`IntroScene`] already at [`IntroStatus::Finished`] (via
/// the skip path), built the same synthetic way [`tests`]'s own fixtures
/// are -- a blank glyph sheet plus a blank dialogue frame, no local pack
/// needed -- but leaked to `'static` so [`crate::flow`]'s own tests can put
/// one straight into an [`crate::flow::AppScene::Intro`], the same shape
/// [`load_default`] would hand [`crate::flow::advance_scene`]. A tiny,
/// one-time, `#[cfg(test)]`-only leak -- never reached in production.
#[cfg(test)]
pub(crate) fn synthetic_finished_scene() -> IntroScene<'static> {
    use assets::fonts::FontImageRef;
    use assets::pack::ImageRef;

    const SHEET_WIDTH: u32 = 256;
    const SHEET_HEIGHT: u32 = 512;
    let pixels: &'static [u8] =
        Box::leak(vec![0u8; (SHEET_WIDTH * SHEET_HEIGHT) as usize].into_boxed_slice());
    let image = ImageRef {
        width: SHEET_WIDTH,
        height: SHEET_HEIGHT,
        bit_depth: 2,
        pixels,
    };
    let sheet = FontGlyphSheet::new(FontImageRef::new_for_tests(FontId::Normal, image))
        .expect("this is the exact real glyph-sheet shape");
    let frame = FrameAssets {
        pixels: vec![0u8; 56 * 16],
        width: 56,
        height: 16,
        palette: vec![Rgb888::BLACK; 16],
    };
    let mut scene = IntroScene::new(sheet, frame, TextSpeed::Instant);
    let status = scene.tick(false, true); // the skip path -> immediately Finished.
    debug_assert_eq!(status, IntroStatus::Finished);
    scene
}

#[cfg(test)]
mod tests;
