//! [`IntroScene`] flow tests: page advance on confirm, the skip path, and
//! headless composition -- all against a synthetic font sheet + dialogue
//! frame (no local asset pack needed, mirroring `engine::text::render`'s own
//! synthetic-sheet test pattern). A real-pack composition check
//! (`real_pack_composes_a_non_blank_intro_frame`) lives right here, at the
//! bottom of this file, mirroring `main_menu::tests::
//! real_pack_composes_a_non_blank_menu_frame`.

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
/// mirrors `engine::text::render`'s own tests. Every pixel is palette index
/// `0` (`textbox::GLYPH_COLORS[0] == None`, transparent), so a glyph
/// revealed against this sheet never paints anything visible -- fine for
/// tests that only care about state (page index, finished-ness, revealed
/// count), but see [`distinguishable_sheet_pixels`] for tests that need to
/// see an actual painted pixel.
fn blank_sheet_pixels() -> Vec<u8> {
    vec![0u8; (SHEET_WIDTH * SHEET_HEIGHT) as usize]
}

/// A synthetic glyph sheet where every glyph cell is uniformly palette
/// index `1` (`textbox::GLYPH_COLORS[1]`'s opaque dark grey,
/// `Rgb888 { r: 24, g: 24, b: 24 }`) -- unlike [`blank_sheet_pixels`]'s
/// all-transparent fixture, a glyph revealed against this sheet actually
/// paints a recognisable colour, so a test can assert a composed frame's
/// pixels prove a glyph was drawn, not just that [`IntroScene::compose`]
/// ran without panicking.
fn distinguishable_sheet_pixels() -> Vec<u8> {
    vec![1u8; (SHEET_WIDTH * SHEET_HEIGHT) as usize]
}

/// A synthetic dialogue frame the exact real shape
/// (`assets::AssetPack::message_box`'s 7x2 tiles, 56x16px) with a plain
/// 16-colour palette -- `FrameAssets`'s fields are `pub(crate)`, so a test
/// in this crate can build one by hand without a real pack (see
/// `FrameAssets`'s own docs on why the palette is already-converted
/// `Rgb888`, not a pack-only `PaletteRef`). Every tile pixel is index `0`
/// (transparent) and the palette is all-black, so [`crate::textbox::blit_frame_tiles`]
/// never actually paints anything distinguishable from the black backdrop
/// against this fixture -- see [`distinguishable_synthetic_frame`] for a
/// frame a test can use to prove the border/fill was actually drawn.
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

/// A synthetic dialogue frame the same real shape as [`synthetic_frame`],
/// but with every tile pixel set to opaque palette index `1` (a distinct
/// colour from the black backdrop) -- so a test can assert a composed
/// frame's border/fill pixels actually equal that colour, proving
/// [`crate::textbox::blit_frame_tiles`] painted them rather than merely running
/// without panicking.
fn distinguishable_synthetic_frame() -> FrameAssets {
    const WIDTH: u32 = 56;
    const HEIGHT: u32 = 16;
    let mut palette = vec![Rgb888::BLACK; 16];
    palette[1] = FRAME_COLOR;
    FrameAssets {
        pixels: vec![1u8; (WIDTH * HEIGHT) as usize],
        width: WIDTH,
        height: HEIGHT,
        palette,
    }
}

/// [`distinguishable_synthetic_frame`]'s chosen non-black colour.
const FRAME_COLOR: Rgb888 = Rgb888 {
    r: 200,
    g: 40,
    b: 40,
};

/// [`distinguishable_sheet_pixels`]'s chosen colour --
/// `textbox::GLYPH_COLORS[1]`, duplicated here since that table is private
/// to `crate::textbox`.
const GLYPH_COLOR: Rgb888 = Rgb888 {
    r: 24,
    g: 24,
    b: 24,
};

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

/// Senior review round 3 regression: this test used to only assert
/// `fb.width()`/`fb.height()` -- consts on [`rendering::Framebuffer`], true
/// regardless of whether `compose` painted anything at all -- against
/// [`blank_sheet_pixels`]'s all-transparent glyph sheet, which made real
/// pixel colour assertions impossible. Uses [`distinguishable_sheet_pixels`]
/// instead so the revealed glyph's own colour is actually checkable.
#[test]
fn compose_paints_the_revealed_glyphs_pixel_not_just_the_frame_dimensions() {
    let pixels = distinguishable_sheet_pixels();
    let mut scene = synthetic_scene(&pixels, TextSpeed::Instant);
    scene.tick(false, false); // reveals the first glyph ('H' of page 0).

    let fb = scene.compose();
    assert_eq!(fb.width(), 240);
    assert_eq!(fb.height(), 160);

    // The first revealed glyph sits at the printer's own origin
    // (`super::PRINTER_ORIGIN`), offset by the dialogue box's screen
    // position (`super::BOX_SCREEN_ORIGIN`) -- a pixel comfortably inside
    // that 16x16 cell must be the glyph's own opaque colour, proving
    // `compose` actually painted the revealed glyph rather than merely
    // producing a correctly-sized, still-blank framebuffer.
    let x = usize::try_from(super::BOX_SCREEN_ORIGIN.0 + super::PRINTER_ORIGIN.0 + 4).unwrap();
    let y = usize::try_from(super::BOX_SCREEN_ORIGIN.1 + super::PRINTER_ORIGIN.1 + 4).unwrap();
    assert_eq!(fb.pixel(x, y), Some(GLYPH_COLOR));
}

/// Senior review round 3 regression: this test used to only assert
/// `fb.width() == 240` (module docs on the sibling test above) against
/// [`synthetic_frame`]'s all-transparent-over-all-black fixture, which made
/// it impossible for a border pixel to ever differ from the backdrop. Uses
/// [`distinguishable_synthetic_frame`] instead so the border's own colour
/// is actually checkable.
#[test]
fn compose_draws_the_dialogue_box_border_even_before_any_glyph_reveals() {
    let pixels = blank_sheet_pixels();
    let image = ImageRef {
        width: SHEET_WIDTH,
        height: SHEET_HEIGHT,
        bit_depth: 2,
        pixels: &pixels,
    };
    let sheet = FontGlyphSheet::new(FontImageRef::new_for_tests(FontId::Normal, image)).unwrap();
    let scene = IntroScene::new(sheet, distinguishable_synthetic_frame(), TextSpeed::Mid);

    let fb = scene.compose();
    assert_eq!(fb.width(), 240);
    assert_eq!(fb.height(), 160);

    // The dialogue box's own top border row (one tile row above the
    // content rect -- `MessageBoxLayout::frame_tiles`'s top-border push,
    // which this port's `blit_frame_tiles` draws unconditionally, before
    // any glyph is ever revealed) must be the frame's own opaque colour.
    let border_x = usize::try_from(super::BOX_SCREEN_ORIGIN.0).unwrap();
    let border_y = usize::try_from(super::BOX_SCREEN_ORIGIN.1 - 8).unwrap();
    assert_eq!(fb.pixel(border_x, border_y), Some(FRAME_COLOR));

    // A pixel clearly outside the box stays the untouched black backdrop --
    // proof the border is a *distinct*, bounded shape, not a stray fill of
    // the whole screen.
    assert_eq!(fb.pixel(2, 2), Some(Rgb888::BLACK));
}

// Guards `GLYPH_COUNT`'s continued use as this module's synthetic sheet
// shape sanity check (256*32 grid => 512 glyphs), so a future change to the
// font sheet layout constants doesn't silently desync this test file's
// hand-picked `SHEET_WIDTH`/`SHEET_HEIGHT` from the real ones.
const _: () = assert!(GLYPH_COUNT == 512);

/// A hand-built pack, containing only a valid `font/normal/glyphs` entry --
/// no `text-window/image/message_box` / `text-window/palette/message_box`
/// entries at all -- written to a temp path, mirroring `assets::pack`'s own
/// synthetic-pack test fixtures (`crates/assets/src/pack/tests.rs`'s
/// `synthetic_pack`) at the exact byte layout `assets::pack`'s module docs
/// specify. Regression fixture for the finding that `load_default` used to
/// leak a fresh `AssetPack` on every call against a pack like this one
/// (missing `message_box`) -- see `super::required_assets`'s doc comment.
fn write_pack_without_message_box() -> std::path::PathBuf {
    const SHEET_WIDTH: u32 = 256;
    const SHEET_HEIGHT: u32 = 512;
    let id = b"font/normal/glyphs";
    let payload = vec![0u8; (SHEET_WIDTH * SHEET_HEIGHT) as usize];

    let mut meta = Vec::new();
    meta.extend_from_slice(&SHEET_WIDTH.to_le_bytes());
    meta.extend_from_slice(&SHEET_HEIGHT.to_le_bytes());
    meta.push(2); // bit_depth

    let header_size = 8 + 4 + 4;
    let directory_size = 2 + id.len() + 1 + 8 + 8 + meta.len();
    let offset = header_size + directory_size;

    let mut out = Vec::new();
    out.extend_from_slice(&assets::pack::MAGIC);
    out.extend_from_slice(&assets::pack::FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes()); // entry_count
    out.extend_from_slice(&u16::try_from(id.len()).unwrap().to_le_bytes());
    out.extend_from_slice(id);
    out.push(0); // EntryKind::Image's tag (assets::pack's own test fixture convention)
    out.extend_from_slice(&(offset as u64).to_le_bytes());
    out.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    out.extend_from_slice(&meta);
    out.extend_from_slice(&payload);

    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-intro-test-pack-no-message-box-{}.pack",
        std::process::id()
    ));
    std::fs::write(&path, &out).unwrap();
    path
}

#[test]
fn a_pack_missing_message_box_fails_validation_without_leaking() {
    // Finding 4 regression: `required_assets` takes a *borrowed* `&AssetPack`
    // and is what `load_default` runs before its `Box::leak` -- calling it
    // directly here, against a pack that is never leaked or even made
    // `'static`, is itself the structural proof that detecting this failure
    // does not require (and therefore cannot cause) a leak.
    let path = write_pack_without_message_box();
    let pack = assets::pack::AssetPack::load(&path).unwrap();

    let err = super::required_assets(&pack).unwrap_err();
    assert!(
        matches!(err, super::IntroSceneError::Pack(_)),
        "a pack with no message_box entry at all must fail with a Pack error, got {err:?}"
    );

    let _ = std::fs::remove_file(path);
}

/// Regression for the finding that `load_default` leaked a fresh
/// `AssetPack` on every *successful* call, not just failed ones (see
/// `a_pack_missing_message_box_fails_validation_without_leaking` above for
/// that half): repeated calls into `cached_pack` -- and therefore
/// `load_default`, which reads through it -- must reuse the exact same,
/// single `'static` pack instance rather than loading and caching (or
/// leaking) a new one each time. Needs the real pack (`cached_pack` reads
/// from disk on a cache miss).
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn cached_pack_reuses_the_same_pack_across_repeated_calls() {
    let first = super::cached_pack().expect("run `cargo xtask extract` first");
    let second = super::cached_pack().expect("run `cargo xtask extract` first");
    assert!(
        std::ptr::eq(first, second),
        "a second `cached_pack` call must reuse the first call's pack instance, not load a new one"
    );

    // The public entry point routes through the same cache -- building two
    // full scenes must not disturb the cached pack's identity either.
    let _scene_a = super::load_default().expect("run `cargo xtask extract` first");
    let _scene_b = super::load_default().expect("run `cargo xtask extract` first");
    let third = super::cached_pack().expect("run `cargo xtask extract` first");
    assert!(
        std::ptr::eq(first, third),
        "building scenes through `load_default` must not disturb the cached pack identity"
    );
}

/// Companion to the test above, runnable without a local pack: a *failed*
/// `cached_pack` call (module docs' "validate before caching") must not
/// wedge the process-wide cache or panic on a second attempt -- it should
/// keep reporting the same "missing pack" diagnostic every time, leaving
/// room for a later attempt (e.g. after `cargo xtask extract` runs) to
/// actually populate the cache.
#[test]
fn cached_pack_reports_pack_missing_repeatedly_without_panicking() {
    // This crate's own test environment never has a local pack (mirrors
    // `title::tests::load_default_reports_pack_missing_when_no_pack_is_extracted`'s
    // identical guard/rationale) -- step aside rather than asserting the
    // wrong thing if it ever does.
    if assets::pack::AssetPack::default_path().is_file() {
        return;
    }

    let first = super::cached_pack().unwrap_err();
    assert!(first.is_pack_missing());
    let second = super::cached_pack().unwrap_err();
    assert!(second.is_pack_missing());
}

/// Senior review round 3: the one real-pack composition check this module
/// was missing (the `A real-pack composition check lives alongside the
/// other scenes' #[ignore] tests in app.rs` claim this file's own module
/// docs used to make was false -- no such test exists in `app.rs`). Mirrors
/// `main_menu::tests::real_pack_composes_a_non_blank_menu_frame`: build the
/// real scene via [`super::load_default`], tick it forward a few frames (so
/// more than one glyph has actually revealed), and confirm both that
/// *something* painted (not an all-black frame) and that the dialogue box
/// itself is visually distinct from the empty backdrop around it -- not
/// just "some pixel somewhere is non-black," which a stray artifact could
/// also satisfy.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_composes_a_non_blank_intro_frame() {
    let mut scene = super::load_default().expect("run `cargo xtask extract` first");
    for _ in 0..5 {
        scene.tick(false, false);
    }

    let fb = scene.compose();
    assert!(
        fb.pixels().iter().any(|&p| p != Rgb888::BLACK),
        "a few ticks in, the real dialogue box and at least one glyph must have painted something"
    );

    // A pixel inside the dialogue box's own content rect must differ from
    // one clearly outside it (near the top-left corner, well clear of the
    // box -- this scene's own black backdrop, module docs).
    let inside = fb
        .pixel(
            usize::try_from(super::BOX_SCREEN_ORIGIN.0 + 4).unwrap(),
            usize::try_from(super::BOX_SCREEN_ORIGIN.1 + 4).unwrap(),
        )
        .expect("in bounds");
    let outside = fb.pixel(2, 2).expect("in bounds");
    assert_ne!(
        inside, outside,
        "the real dialogue box must look different from the empty backdrop around it"
    );
}
