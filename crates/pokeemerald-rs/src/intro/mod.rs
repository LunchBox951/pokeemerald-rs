//! The new-game intro: Birch's speech (I-3, issue #149), transcribed from
//! upstream's `Task_NewGameBirchSpeech_*` chain
//! (`pokeemerald/src/main_menu.c:1279-1755`).
//!
//! [`speech`] holds the actual dialogue text (module docs there for the
//! exact upstream strings and the naming/gender-selection deviations);
//! [`IntroScene`] drives it through a single
//! [`engine::text::render::Printer`], re-armed
//! ([`engine::text::render::Printer::restart`]) once per page, and paints it
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
//!   are?"/"What's your name?") still print and still wait for a button
//!   press before continuing (upstream holds both on screen too --
//!   `Task_NewGameBirchSpeech_WaitPressBeforeNameChoice`,
//!   `main_menu.c:1590`, for "What's your name?"; "And you are?" is
//!   gated behind the unmodeled Birch/Lotad platform-fade sequence instead
//!   of an explicit button-wait task state, `main_menu.c:1410-1501`, but a
//!   press-wait reproduces upstream's actual observable pacing -- the page
//!   stays up until *something* advances it -- without modeling that
//!   animation) -- just with no gender-select menu or naming UI rendered
//!   in between.
//! - **No name-confirmation Yes/No menu.** [`speech::pages`]'s page 6
//!   ("So it's ...?") is upstream's name confirmation prompt: it holds on
//!   screen while `Task_NewGameBirchSpeech_ProcessNameYesNoMenu`
//!   (`main_menu.c:1626`) waits on a real Yes/No menu. No menu is rendered
//!   here, so that page too waits on a plain button press instead (see
//!   `speech`'s `so_its_player` doc comment) -- answering "No" (which
//!   upstream loops back to the naming step) has nothing to loop back to in
//!   this slice.
//! - **No music, no sound effects.** `PlayBGM(MUS_ROUTE122)` and every
//!   `PlaySE` call in the task chain are silent here (no audio wiring in
//!   this slice).
//!
//! # Advance (issue #393)
//!
//! [`IntroScene::tick`] takes one [`PrinterInput`] per frame -- newly-pressed
//! and held A/B, forwarded straight to the underlying
//! [`Printer::tick`](engine::text::render::Printer::tick) -- and no separate
//! skip input. Real Emerald's intro cannot be skipped outright: B is an
//! ordinary dialogue-advance button, not a whole-intro shortcut (upstream
//! `JOY_NEW(A_BUTTON | B_BUTTON)` inside `TextPrinterWaitWithDownArrow`,
//! `pokeemerald/src/text.c:874-879`, which both `\p` and `\l` wait on --
//! either button clears/scrolls a page identically, module docs on
//! [`engine::text::render::Printer::tick`]). An earlier pre-1.0 revision of
//! this scene wired B to a `skip_pressed` shortcut with no upstream
//! analogue at all; issue #393 deleted it once `V-7`/`H-1`
//! (`docs/acceptance/v1.md`) flagged it as a dev convenience that had
//! survived into production code. The closest real Emerald gets to a B
//! shortcut is `WhatsYourName`'s own wait state also accepting B
//! (`main_menu.c:1590`) -- ordinary dialogue-advance, exactly what this
//! scene now does.
//!
//! The intro's own [`Printer`] opts into upstream's held-A/B print
//! speed-up (`AddTextPrinterForMessage(TRUE)`, `main_menu.c:1339`) via
//! [`Printer::with_ab_speed_up_print`] -- see that method's docs and
//! [`engine::text::render`]'s own "Held-A/B print speed-up" module docs for
//! the exact semantics.
//!
//! # Traversal pacing
//!
//! [`TRAVERSAL_RUNS`] pins, frame by frame, how long a full read of the
//! speech takes at [`TextSpeed::Mid`] when the player never holds a button:
//! thirty-six runs of no input, twenty-four of them ended by a single
//! confirm press at a `\p`/`\l` wait, the other twelve (four scroll
//! animations, eight page-terminator drains) needing no input at all.
//! [`TRAVERSAL_FRAMES`] is their total.
//!
//! That table is a *derived* measurement, re-computed from the real
//! [`Printer`] on every CI run by this module's own pack-free
//! `traversal_runs_match_the_pinned_table` test -- not a set of magic
//! numbers. `xtask`'s `boot-to-first-fight` scenario reads Birch's whole
//! speech, so its authored script needs exactly these counts; publishing
//! them here (rather than re-typing them into the script) keeps one
//! machine-checked source for the intro's pacing.

mod speech;

use assets::fonts::{FontId, OwnedFontGlyphSheet};
use assets::pack::{AssetPack, PackError};
use engine::text::render::{Printer, PrinterInput, RevealedGlyph, TextSpeed, TickEvent};
use engine::text::window::MessageBoxLayout;
use engine::text::Token;
use rendering::{Framebuffer, Rgb888};

use crate::textbox::{self, FrameAssets};

pub use speech::NUM_PAGES;

/// One uninterrupted run of *no* input while reading Birch's speech
/// (module docs' "Traversal pacing" section) -- how many frames printing,
/// scrolling or draining takes before the next thing happens, and whether
/// that next thing is a single confirm press.
///
/// Plain data with public fields `(oop-boundaries)`: this is a measurement,
/// not an object with behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraversalRun {
    /// Frames of no input in this run, counting the frame the run's own
    /// terminating event fires on (the `\p`/`\l` wait being reached, the
    /// scroll animation finishing, or the page's terminator being
    /// consumed).
    pub frames: u32,
    /// Whether exactly one confirm-press frame follows this run: true for
    /// a run that ended on a `\p`/`\l` wait, false for a scroll-animation
    /// drain or a page's own terminator drain, which need no input.
    pub confirm_after: bool,
}

/// Every [`TraversalRun`] of a full, never-held read of Birch's whole
/// eight-page speech at [`TextSpeed::Mid`]
/// ([`IntroScene::from_pack`]'s own speed), in order -- the intro's
/// frame-level pacing contract (module docs' "Traversal pacing" section).
///
/// **Derived, not hand-copied.** `tests::traversal_runs_match_the_pinned_table`
/// re-derives this whole table every CI run by driving [`speech::pages`]
/// through a real [`Printer`] over a synthetic glyph sheet (no asset pack:
/// only the compiled-in advance-width table, not sheet pixels, affects
/// *when* a wait is reached), so any change to the printer's state machine,
/// to a speech page's text, or to the reveal-delay timing fails that test
/// instead of silently re-pacing the intro.
///
/// `xtask`'s `boot-to-first-fight` scenario script is the consumer: its
/// intro block presses A or B on exactly the frames this table says a wait
/// is reached, and its own tests assert the authored script against this
/// table frame for frame.
pub const TRAVERSAL_RUNS: &[TraversalRun] = &[
    // --- page 0: WELCOME ---
    run(121, true), // prompt 1 (\p)
    run(132, true), // prompt 2 (\p)
    run(72, true),  // prompt 3 (\p)
    run(179, true), // prompt 4 (\p)
    run(4, false),  // reveal-delay drain, then the page terminator
    // --- page 1: THIS_IS_A_POKEMON (its {PAUSE 96} is inside this run) ---
    run(230, true), // prompt 1 (\p)
    run(4, false),  // page terminator drain
    // --- page 2: MAIN_SPEECH ---
    run(244, true), // prompt 1 (\p)
    run(279, true), // prompt 2 (\l)
    run(9, false),  // scroll animation, no input needed
    run(140, true), // prompt 3 (\p)
    run(235, true), // prompt 4 (\p)
    run(267, true), // prompt 5 (\p)
    run(235, true), // prompt 6 (\p)
    run(247, true), // prompt 7 (\l)
    run(9, false),  // scroll animation
    run(72, true),  // prompt 8 (\p)
    run(4, false),  // page terminator drain
    // --- page 3: AND_YOU_ARE ---
    run(49, true), // prompt 1 (\p)
    run(4, false), // page terminator drain
    // --- page 4: WHATS_YOUR_NAME ---
    run(112, true), // prompt 1 (\p)
    run(4, false),  // page terminator drain
    // --- page 5: so_its_player ---
    run(49, true), // prompt 1 (\p)
    run(4, false), // page terminator drain
    // --- page 6: youre_player ---
    run(37, true),  // prompt 1 (\p)
    run(215, true), // prompt 2 (\l)
    run(9, false),  // scroll animation
    run(56, true),  // prompt 3 (\p)
    run(4, false),  // page terminator drain
    // --- page 7: ARE_YOU_READY ---
    run(101, true), // prompt 1 (\p)
    run(175, true), // prompt 2 (\p)
    run(251, true), // prompt 3 (\l)
    run(9, false),  // scroll animation
    run(136, true), // prompt 4 (\p)
    run(263, true), // prompt 5 (\p)
    run(4, false),  // the last drain: its final frame hands off to the overworld
];

/// [`TraversalRun`]'s own terser constructor, so [`TRAVERSAL_RUNS`] reads
/// as a table of numbers rather than thirty-six struct literals.
const fn run(frames: u32, confirm_after: bool) -> TraversalRun {
    TraversalRun {
        frames,
        confirm_after,
    }
}

/// Total frames a full, never-held read of Birch's speech takes: every
/// [`TRAVERSAL_RUNS`] entry's own frames plus the single confirm-press
/// frame after each run that needs one.
pub const TRAVERSAL_FRAMES: usize = traversal_frames(TRAVERSAL_RUNS);

/// [`TRAVERSAL_FRAMES`]'s sum, as a `const fn` so the total stays derived
/// from the table instead of being a second number to keep in step.
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

/// `MessageBoxLayout::STANDARD`'s content rect size, converted to pixels
/// (tile -> 8px), for [`textbox::blit_glyphs`]'s `content_size` -- clips a
/// scrolled-past-the-top-edge glyph (module docs there) to this box instead
/// of letting it paint anywhere else in the framebuffer.
const BOX_CONTENT_SIZE_PX: (i32, i32) = (
    engine::text::window::STANDARD_CONTENT_WIDTH * 8,
    engine::text::window::STANDARD_CONTENT_HEIGHT * 8,
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
/// (module docs' "Advance" section).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntroStatus {
    /// More pages remain (or the current one is still printing/paging).
    Continue,
    /// Every page finished. The caller should transition to the overworld.
    Finished,
}

/// Birch's speech, one page at a time (module docs).
///
/// # Ownership
///
/// Owns every byte it renders -- the decoded glyph sheet
/// ([`OwnedFontGlyphSheet`], held inside the [`Printer`]) and the dialogue
/// frame ([`FrameAssets`]) -- exactly like
/// [`crate::title::TitleScene`]/[`crate::overworld::OverworldScene`]/
/// [`crate::main_menu::MainMenuScene`] do. Nothing here borrows from an
/// [`AssetPack`], so the pack a scene was built from is dropped the moment
/// [`from_pack`](Self::from_pack) returns and every fresh intro reads
/// whatever is on disk *then* -- a regenerated pack (`cargo xtask extract`
/// re-run mid-session, an embedding host restarting the game) is picked up
/// like any other scene's, and there is no process-global state to make one
/// session's pack outlive it `(oop-boundaries)`.
///
/// The intro is the one scene whose [`Printer`] stays alive *across* frames
/// (the others drive a throwaway one to completion inside a single
/// function), which is why the printer holds an owned glyph source rather
/// than a pack-borrowed [`assets::fonts::FontGlyphSheet`]: see
/// [`Printer`]'s own "Sheet ownership" docs.
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
    /// Build a fresh intro over an already-decoded font `sheet` and dialogue
    /// `frame`, printing at `speed`.
    ///
    /// `pub(crate)`, not `pub`: [`FrameAssets`] (its `frame` parameter) is
    /// itself crate-private (see its own docs), and every real caller goes
    /// through [`from_pack`](Self::from_pack)/[`load_default`] -- this
    /// constructor exists as a seam for those and for this crate's own
    /// synthetic-fixture tests.
    #[must_use]
    pub(crate) fn new(sheet: OwnedFontGlyphSheet, frame: FrameAssets, speed: TextSpeed) -> Self {
        let pages = speech::pages();
        // Upstream's `AddTextPrinterForMessage(TRUE)` (`main_menu.c:1339`):
        // Birch's speech is the one printer in this port with held-A/B
        // speed-up enabled (module docs' "Advance" section).
        let printer =
            Printer::new(pages[0].clone(), sheet, speed, PRINTER_ORIGIN).with_ab_speed_up_print();
        Self {
            frame,
            pages,
            page_index: 0,
            printer,
            revealed: Vec::new(),
            finished: false,
        }
    }

    /// Copy [`IntroScene`]'s two required entries -- the normal-weight font
    /// sheet and the dialogue frame -- out of an already-loaded `pack` into
    /// owned storage (struct docs), mirroring
    /// [`crate::main_menu::MainMenuScene::from_pack`]. `pack` is only
    /// borrowed for this call; the returned scene does not reference it.
    ///
    /// Prints at [`TextSpeed::Mid`], upstream's own new-game default
    /// (`SetDefaultOptions`'s `optionsTextSpeed = OPTIONS_TEXT_SPEED_MID`,
    /// `pokeemerald/src/new_game.c:91-93`) -- nothing yet models the
    /// player-selectable text-speed option.
    ///
    /// # Errors
    ///
    /// [`IntroSceneError::Pack`] if `pack` is missing its
    /// `font/normal/glyphs` or message-box entries (or either is malformed);
    /// [`IntroSceneError::Font`] if the font sheet doesn't decode.
    pub fn from_pack(pack: &AssetPack) -> Result<Self, IntroSceneError> {
        let sheet = OwnedFontGlyphSheet::new(pack.font(FontId::Normal)?)?;
        let frame = FrameAssets::from_handle(pack.message_box()?);
        Ok(Self::new(sheet, frame, TextSpeed::Mid))
    }

    /// The current page index (`0..NUM_PAGES`), for tests/diagnostics.
    #[must_use]
    pub const fn page_index(&self) -> usize {
        self.page_index
    }

    /// Whether every page has finished.
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
    /// `input` is this frame's A/B edges and holds, forwarded straight to
    /// the current page's [`Printer::tick`] (module docs' "Advance"
    /// section) -- once [`IntroStatus::Finished`] is returned, every further
    /// call returns it again without doing anything (mirrors
    /// [`engine::text::render::Printer::is_finished`]'s own terminal
    /// contract).
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
            | TickEvent::Paused
            | TickEvent::PauseFinished => {}
        }

        if self.finished {
            IntroStatus::Finished
        } else {
            IntroStatus::Continue
        }
    }

    /// Re-arm the printer over the next page ([`Printer::restart`] -- same
    /// printer, same owned glyph sheet, fresh token stream), or mark the
    /// intro finished if [`Self::page_index`] was already the last page.
    fn advance_page(&mut self) {
        if self.page_index + 1 < NUM_PAGES {
            self.page_index += 1;
            self.revealed.clear();
            self.printer.restart(self.pages[self.page_index].clone());
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
        textbox::blit_glyphs(
            &mut fb,
            &self.revealed,
            BOX_SCREEN_ORIGIN,
            BOX_CONTENT_SIZE_PX,
        );

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

/// Load the pack from its default location, build the intro out of it, and
/// drop the pack again -- mirrors [`crate::main_menu::load_default`] /
/// [`crate::title::load_default`].
///
/// Reads from disk on every call, by design: an intro built here owns every
/// byte it renders ([`IntroScene`]'s struct docs), so a second call after
/// the pack on disk changed builds a scene from the *new* bytes. Nothing is
/// cached process-wide and nothing is leaked.
///
/// # Errors
///
/// [`IntroSceneError::Pack`] with [`IntroSceneError::is_pack_missing`] true
/// if no pack has been extracted yet; see [`IntroScene::from_pack`] for the
/// other (real-pack-only) error cases.
pub fn load_default() -> Result<IntroScene, IntroSceneError> {
    let pack = AssetPack::load_default()?;
    IntroScene::from_pack(&pack)
}

/// Test-only: an [`IntroScene`] already at [`IntroStatus::Finished`], built
/// the same synthetic way [`tests`]'s own fixtures are -- a blank glyph
/// sheet plus a blank dialogue frame, no local pack needed -- so
/// [`crate::flow`]'s own tests can put one straight into an
/// [`crate::flow::AppScene::Intro`], the same shape [`load_default`] would
/// hand [`crate::flow::advance_scene`]. Fully owned, like every real scene
/// ([`IntroScene`]'s struct docs): nothing leaked, nothing `'static`.
///
/// Drives the real page-by-page advance to completion (module docs'
/// "Advance" section -- there is no shortcut past it since issue #393
/// deleted the pre-1.0 whole-intro B-skip this helper used to take): at
/// [`TextSpeed::Instant`] every glyph reveals in one tick and every
/// `\p`/`\l` wait resolves on the very next confirmed one, so this
/// terminates in well under the bound, mirroring
/// [`tests::confirming_every_frame_advances_through_every_page_to_the_overworld_handoff`]'s
/// identical loop shape.
#[cfg(test)]
pub(crate) fn synthetic_finished_scene() -> IntroScene {
    use assets::fonts::FontImageRef;
    use assets::pack::ImageRef;

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
    let mut scene = IntroScene::new(sheet, frame, TextSpeed::Instant);
    let confirm_a = PrinterInput {
        a_pressed: true,
        b_pressed: false,
        a_held: true,
        b_held: false,
    };
    let mut status = IntroStatus::Continue;
    for _ in 0..5000 {
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
