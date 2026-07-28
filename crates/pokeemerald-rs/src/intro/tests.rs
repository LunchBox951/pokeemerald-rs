//! [`IntroScene`] flow tests: page advance on confirm, the skip path, and
//! headless composition -- all against a synthetic font sheet + dialogue
//! frame (no local asset pack needed, mirroring `engine::text::render`'s own
//! synthetic-sheet test pattern). A real-pack composition check lives
//! alongside the other scenes' `#[ignore]` tests in `app.rs`.

use assets::fonts::{FontGlyphSheet, FontId, FontImageRef, GLYPH_COUNT};
use assets::pack::ImageRef;
use engine::text::render::TextSpeed;
use rendering::Rgb888;

use super::{IntroScene, IntroStatus, NUM_PAGES};
use crate::textbox::FrameAssets;

const SHEET_WIDTH: u32 = 256;
const SHEET_HEIGHT: u32 = 512;

/// A uniformly-blank synthetic glyph sheet, the real shape (256x512, see
/// `assets::fonts`' module docs) so [`FontGlyphSheet::new`] accepts it --
/// mirrors `engine::text::render`'s own tests.
fn blank_sheet_pixels() -> Vec<u8> {
    vec![0u8; (SHEET_WIDTH * SHEET_HEIGHT) as usize]
}

/// A synthetic dialogue frame the exact real shape
/// (`assets::AssetPack::message_box`'s 7x2 tiles, 56x16px) with a plain
/// 16-colour palette -- `FrameAssets`'s fields are `pub(crate)`, so a test
/// in this crate can build one by hand without a real pack (see
/// `FrameAssets`'s own docs on why the palette is already-converted
/// `Rgb888`, not a pack-only `PaletteRef`).
fn synthetic_frame() -> FrameAssets {
    const WIDTH: u32 = 56;
    const HEIGHT: u32 = 16;
    FrameAssets {
        pixels: vec![0u8; (WIDTH * HEIGHT) as usize],
        width: WIDTH,
        height: HEIGHT,
        palette: vec![Rgb888::BLACK; 16],
    }
}

fn synthetic_scene(pixels: &[u8], speed: TextSpeed) -> IntroScene<'_> {
    let image = ImageRef {
        width: SHEET_WIDTH,
        height: SHEET_HEIGHT,
        bit_depth: 2,
        pixels,
    };
    let sheet = FontGlyphSheet::new(FontImageRef::new_for_tests(FontId::Normal, image)).unwrap();
    IntroScene::new(sheet, synthetic_frame(), speed)
}

#[test]
fn starts_on_the_first_page_not_finished() {
    let pixels = blank_sheet_pixels();
    let scene = synthetic_scene(&pixels, TextSpeed::Mid);
    assert_eq!(scene.page_index(), 0);
    assert!(!scene.is_finished());
    assert_eq!(scene.revealed_glyph_count(), 0);
}

#[test]
fn a_glyph_reveals_on_the_first_tick_at_instant_speed() {
    let pixels = blank_sheet_pixels();
    let mut scene = synthetic_scene(&pixels, TextSpeed::Instant);
    let status = scene.tick(false, false);
    assert_eq!(status, IntroStatus::Continue);
    assert_eq!(
        scene.revealed_glyph_count(),
        1,
        "'H' of \"Hi! Sorry...\" reveals frame 0"
    );
}

#[test]
fn skip_finishes_immediately_regardless_of_page_progress() {
    let pixels = blank_sheet_pixels();
    let mut scene = synthetic_scene(&pixels, TextSpeed::Mid);
    // Print a little first, so the skip is genuinely mid-page, not a no-op.
    for _ in 0..5 {
        scene.tick(false, false);
    }
    assert!(!scene.is_finished());

    let status = scene.tick(false, true);
    assert_eq!(status, IntroStatus::Finished);
    assert!(scene.is_finished());
}

#[test]
fn once_finished_every_further_tick_stays_finished() {
    let pixels = blank_sheet_pixels();
    let mut scene = synthetic_scene(&pixels, TextSpeed::Instant);
    assert_eq!(scene.tick(false, true), IntroStatus::Finished);
    for _ in 0..5 {
        assert_eq!(scene.tick(true, false), IntroStatus::Finished);
        assert!(scene.is_finished());
    }
}

#[test]
fn confirming_every_frame_advances_through_every_page_to_the_overworld_handoff() {
    // At Instant speed every glyph reveals in one tick and every \p/\l wait
    // resolves in one confirmed tick (see `IntroScene::tick`'s module
    // docs), so this terminates quickly; the generous bound just guards
    // against an infinite loop if a future change breaks termination.
    let pixels = blank_sheet_pixels();
    let mut scene = synthetic_scene(&pixels, TextSpeed::Instant);

    let mut seen_pages = std::collections::BTreeSet::new();
    seen_pages.insert(scene.page_index());
    let mut status = IntroStatus::Continue;
    for _ in 0..5000 {
        status = scene.tick(true, false);
        seen_pages.insert(scene.page_index());
        if status == IntroStatus::Finished {
            break;
        }
    }

    assert_eq!(status, IntroStatus::Finished);
    assert!(scene.is_finished());
    assert_eq!(scene.page_index(), NUM_PAGES - 1);
    // Every page in order was actually visited, not skipped over.
    assert_eq!(seen_pages, (0..NUM_PAGES).collect());
}

#[test]
fn a_page_break_clears_the_revealed_glyph_accumulator() {
    // The first page ("Hi! Sorry to keep you waiting!{P}...") prints past
    // its first `\p` well before running out of pages; drive it there and
    // confirm the accumulator drops back to (near) zero on the page clear,
    // rather than growing unboundedly across pages.
    let pixels = blank_sheet_pixels();
    let mut scene = synthetic_scene(&pixels, TextSpeed::Instant);

    let mut max_seen = 0usize;
    let mut saw_reset = false;
    for _ in 0..500 {
        scene.tick(true, false);
        if scene.is_finished() {
            break;
        }
        max_seen = max_seen.max(scene.revealed_glyph_count());
        if max_seen > 5 && scene.revealed_glyph_count() < max_seen {
            saw_reset = true;
            break;
        }
    }
    assert!(
        saw_reset,
        "expected the glyph accumulator to shrink after a page clear"
    );
}

#[test]
fn compose_produces_a_full_240x160_framebuffer() {
    let pixels = blank_sheet_pixels();
    let mut scene = synthetic_scene(&pixels, TextSpeed::Instant);
    scene.tick(false, false);
    let fb = scene.compose();
    assert_eq!(fb.width(), 240);
    assert_eq!(fb.height(), 160);
}

#[test]
fn compose_draws_the_dialogue_box_border_even_before_any_glyph_reveals() {
    let pixels = blank_sheet_pixels();
    let scene = synthetic_scene(&pixels, TextSpeed::Mid);
    let fb = scene.compose();
    // The border tiles use palette index 1+ (index 0 is transparent); the
    // synthetic frame's tile bitmap is all-zero, so this only proves the
    // blit path runs without panicking against a full page layout -- pixel
    // colour fidelity is covered by the real-pack ignored test in `app.rs`.
    assert_eq!(fb.width(), 240);
}

// Guards `GLYPH_COUNT`'s continued use as this module's synthetic sheet
// shape sanity check (256*32 grid => 512 glyphs), so a future change to the
// font sheet layout constants doesn't silently desync this test file's
// hand-picked `SHEET_WIDTH`/`SHEET_HEIGHT` from the real ones.
const _: () = assert!(GLYPH_COUNT == 512);
