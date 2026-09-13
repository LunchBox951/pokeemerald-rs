use assets::fonts::{
    FontId, FontImageRef, OwnedFontGlyphSheet, GLYPH_COUNT, SHEET_HEIGHT, SHEET_WIDTH,
};
use assets::pack::{AssetPack, ImageRef};
use engine::text::render::{Printer, PrinterInput, TextSpeed, TickEvent};
use pack_format::PackEntry;
use rendering::Rgb888;

use super::{IntroScene, IntroStatus, TraversalRun, NUM_PAGES};
use crate::textbox::{FrameAssets, STANDARD_BOX_SCREEN_ORIGIN, STANDARD_PRINTER_ORIGIN};

const NO_INPUT: PrinterInput = PrinterInput::none();

const PRESS_A: PrinterInput = PrinterInput {
    a_pressed: true,
    b_pressed: false,
    a_held: false,
    b_held: false,
};

const PRESS_B: PrinterInput = PrinterInput {
    a_pressed: false,
    b_pressed: true,
    a_held: false,
    b_held: false,
};

const APP_A_PRESS: PrinterInput = PrinterInput {
    a_pressed: true,
    b_pressed: false,
    a_held: true,
    b_held: false,
};

const TRANSPARENT_PALETTE_INDEX: u8 = 0;
const DARK_GREY_GLYPH_PALETTE_INDEX: u8 = 1;
const SHADOW_GLYPH_PALETTE_INDEX: u8 = 2;
const FONT_BIT_DEPTH: u8 = 2;
const MESSAGE_BOX_WIDTH: u32 = 56;
const MESSAGE_BOX_HEIGHT: u32 = 16;
const MESSAGE_BOX_BIT_DEPTH: u8 = 4;
const MESSAGE_BOX_PALETTE_COLOUR_COUNT: u16 = 16;
const SOLID_FRAME_PALETTE_INDEX: u8 = 1;
const TILE_SIDE: i32 = 8;
const GLYPH_INTERIOR_OFFSET: i32 = 4;
const MESSAGE_BOX_INTERIOR_OFFSET: i32 = 4;
const BACKDROP_PROBE: usize = 2;
const FRAMEBUFFER_WIDTH: usize = 240;
const FRAMEBUFFER_HEIGHT: usize = 160;
const EXPECTED_GLYPH_COUNT: usize = 512;
const MAX_FIRST_PROMPT_TICKS: usize = 200;
const MAX_INTRO_TICKS: usize = 5_000;
const MAX_FIRST_PAGE_TICKS: usize = 500;
const MINIMUM_GLYPHS_BEFORE_CLEAR: usize = 5;
const MAX_TRAVERSAL_TICKS: usize = 20_000;
const REAL_PACK_COMPOSITION_TICKS: usize = 5;
const MINIMUM_REAL_PACK_GLYPHS: usize = 2;

fn transparent_glyph_sheet_pixels() -> Vec<u8> {
    vec![TRANSPARENT_PALETTE_INDEX; (SHEET_WIDTH * SHEET_HEIGHT) as usize]
}

fn dark_grey_glyph_sheet_pixels() -> Vec<u8> {
    vec![DARK_GREY_GLYPH_PALETTE_INDEX; (SHEET_WIDTH * SHEET_HEIGHT) as usize]
}

fn transparent_message_box() -> FrameAssets {
    FrameAssets {
        pixels: vec![TRANSPARENT_PALETTE_INDEX; (MESSAGE_BOX_WIDTH * MESSAGE_BOX_HEIGHT) as usize],
        width: MESSAGE_BOX_WIDTH,
        height: MESSAGE_BOX_HEIGHT,
        palette: vec![Rgb888::BLACK; usize::from(MESSAGE_BOX_PALETTE_COLOUR_COUNT)],
    }
}

fn solid_red_message_box() -> FrameAssets {
    let mut palette = vec![Rgb888::BLACK; usize::from(MESSAGE_BOX_PALETTE_COLOUR_COUNT)];
    palette[usize::from(SOLID_FRAME_PALETTE_INDEX)] = SOLID_FRAME_COLOR;
    FrameAssets {
        pixels: vec![SOLID_FRAME_PALETTE_INDEX; (MESSAGE_BOX_WIDTH * MESSAGE_BOX_HEIGHT) as usize],
        width: MESSAGE_BOX_WIDTH,
        height: MESSAGE_BOX_HEIGHT,
        palette,
    }
}

const SOLID_FRAME_COLOR: Rgb888 = Rgb888 {
    r: 200,
    g: 40,
    b: 40,
};

const DARK_GREY_GLYPH_COLOR: Rgb888 = Rgb888 {
    r: 24,
    g: 24,
    b: 24,
};

const SHADOW_GLYPH_COLOR: Rgb888 = Rgb888 {
    r: 160,
    g: 160,
    b: 160,
};

fn synthetic_sheet(pixels: &[u8]) -> OwnedFontGlyphSheet {
    let image = ImageRef {
        width: SHEET_WIDTH,
        height: SHEET_HEIGHT,
        bit_depth: FONT_BIT_DEPTH,
        pixels,
    };
    OwnedFontGlyphSheet::new(FontImageRef::new_for_tests(FontId::Normal, image)).unwrap()
}

fn synthetic_scene(pixels: &[u8], speed: TextSpeed) -> IntroScene {
    IntroScene::new(synthetic_sheet(pixels), transparent_message_box(), speed)
}

#[test]
fn starts_on_the_first_page_not_finished() {
    let pixels = transparent_glyph_sheet_pixels();
    let scene = synthetic_scene(&pixels, TextSpeed::Mid);
    assert_eq!(scene.page_index(), 0);
    assert!(!scene.is_finished());
    assert_eq!(scene.revealed_glyph_count(), 0);
}

#[test]
fn a_glyph_reveals_on_the_first_tick_at_instant_speed() {
    let pixels = transparent_glyph_sheet_pixels();
    let mut scene = synthetic_scene(&pixels, TextSpeed::Instant);
    let status = scene.tick(NO_INPUT);
    assert_eq!(status, IntroStatus::Continue);
    assert_eq!(
        scene.revealed_glyph_count(),
        1,
        "'H' of \"Hi! Sorry...\" reveals frame 0"
    );
}

#[test]
fn b_mid_speech_does_not_finish_the_intro() {
    let pixels = transparent_glyph_sheet_pixels();
    let mut scene = synthetic_scene(&pixels, TextSpeed::Mid);
    for _ in 0..5 {
        scene.tick(NO_INPUT);
    }
    assert!(!scene.is_finished());

    let status = scene.tick(PRESS_B);
    assert_eq!(
        status,
        IntroStatus::Continue,
        "B mid-speech must not finish the intro"
    );
    assert!(!scene.is_finished());
    assert_eq!(
        scene.page_index(),
        0,
        "B mid-speech must not skip to a later page either"
    );
}

fn reveal_until_the_first_prompt_wait(scene: &mut IntroScene) -> usize {
    let mut previous_glyph_count = scene.revealed_glyph_count();
    for _ in 0..MAX_FIRST_PROMPT_TICKS {
        scene.tick(NO_INPUT);
        let glyph_count = scene.revealed_glyph_count();
        if glyph_count == previous_glyph_count {
            return glyph_count;
        }
        previous_glyph_count = glyph_count;
    }
    panic!("the first prompt wait was not reached");
}

#[test]
fn b_advances_a_prompt_clear_exactly_like_a_does() {
    let pixels = transparent_glyph_sheet_pixels();
    let mut scene = synthetic_scene(&pixels, TextSpeed::Instant);

    let glyphs_before_clear = reveal_until_the_first_prompt_wait(&mut scene);
    assert!(
        glyphs_before_clear > 0,
        "page 0 must have revealed something first"
    );

    let status = scene.tick(PRESS_B);
    assert_eq!(status, IntroStatus::Continue);
    assert_eq!(
        scene.revealed_glyph_count(),
        0,
        "B must clear the accumulator exactly like A does"
    );
}

#[test]
fn once_finished_every_further_tick_stays_finished() {
    let mut scene = super::synthetic_finished_scene();
    assert!(scene.is_finished());
    for _ in 0..5 {
        assert_eq!(scene.tick(PRESS_A), IntroStatus::Finished);
        assert!(scene.is_finished());
    }
}

#[test]
fn confirming_every_frame_advances_through_every_page_to_the_overworld_handoff() {
    let pixels = transparent_glyph_sheet_pixels();
    let mut scene = synthetic_scene(&pixels, TextSpeed::Instant);

    let mut seen_pages = std::collections::BTreeSet::new();
    seen_pages.insert(scene.page_index());
    let mut status = IntroStatus::Continue;
    for _ in 0..MAX_INTRO_TICKS {
        status = scene.tick(PRESS_A);
        seen_pages.insert(scene.page_index());
        if status == IntroStatus::Finished {
            break;
        }
    }

    assert_eq!(status, IntroStatus::Finished);
    assert!(scene.is_finished());
    assert_eq!(scene.page_index(), NUM_PAGES - 1);
    assert_eq!(seen_pages, (0..NUM_PAGES).collect());
}

#[test]
fn a_page_break_clears_the_revealed_glyph_accumulator() {
    let pixels = transparent_glyph_sheet_pixels();
    let mut scene = synthetic_scene(&pixels, TextSpeed::Instant);

    let mut max_revealed_glyphs = 0usize;
    let mut glyph_count_decreased = false;
    for _ in 0..MAX_FIRST_PAGE_TICKS {
        scene.tick(PRESS_A);
        if scene.is_finished() {
            break;
        }
        max_revealed_glyphs = max_revealed_glyphs.max(scene.revealed_glyph_count());
        if max_revealed_glyphs > MINIMUM_GLYPHS_BEFORE_CLEAR
            && scene.revealed_glyph_count() < max_revealed_glyphs
        {
            glyph_count_decreased = true;
            break;
        }
    }
    assert!(
        glyph_count_decreased,
        "expected the glyph accumulator to shrink after a page clear"
    );
}

#[test]
fn compose_returns_native_dimensions_and_paints_the_first_revealed_glyph() {
    let pixels = dark_grey_glyph_sheet_pixels();
    let mut scene = synthetic_scene(&pixels, TextSpeed::Instant);
    scene.tick(NO_INPUT);
    assert_eq!(scene.revealed_glyph_count(), 1);

    let fb = scene.compose();
    assert_eq!(fb.width(), FRAMEBUFFER_WIDTH);
    assert_eq!(fb.height(), FRAMEBUFFER_HEIGHT);

    let (glyph_x, glyph_y) = first_glyph_interior_position();
    assert_eq!(fb.pixel(glyph_x, glyph_y), Some(DARK_GREY_GLYPH_COLOR));
}

#[test]
fn compose_draws_the_dialogue_box_border_even_before_any_glyph_reveals() {
    let pixels = transparent_glyph_sheet_pixels();
    let scene = IntroScene::new(
        synthetic_sheet(&pixels),
        solid_red_message_box(),
        TextSpeed::Mid,
    );

    let fb = scene.compose();
    assert_eq!(fb.width(), FRAMEBUFFER_WIDTH);
    assert_eq!(fb.height(), FRAMEBUFFER_HEIGHT);

    let top_border_x = usize::try_from(STANDARD_BOX_SCREEN_ORIGIN.0).unwrap();
    let top_border_y = usize::try_from(STANDARD_BOX_SCREEN_ORIGIN.1 - TILE_SIDE).unwrap();
    assert_eq!(
        fb.pixel(top_border_x, top_border_y),
        Some(SOLID_FRAME_COLOR)
    );

    assert_eq!(
        fb.pixel(BACKDROP_PROBE, BACKDROP_PROBE),
        Some(Rgb888::BLACK)
    );
}

const _: () = assert!(GLYPH_COUNT == EXPECTED_GLYPH_COUNT);

fn write_pack(path: &std::path::Path, entries: Vec<PackEntry>) {
    std::fs::write(path, crate::pack_test_support::pack_bytes(entries)).unwrap();
}

fn font_entry(fill_palette_index: u8) -> PackEntry {
    crate::pack_test_support::image_entry(
        "font/normal/glyphs",
        SHEET_WIDTH,
        SHEET_HEIGHT,
        FONT_BIT_DEPTH,
        fill_palette_index,
    )
}

fn message_box_entries() -> Vec<PackEntry> {
    vec![
        crate::pack_test_support::image_entry(
            "text-window/image/message_box",
            MESSAGE_BOX_WIDTH,
            MESSAGE_BOX_HEIGHT,
            MESSAGE_BOX_BIT_DEPTH,
            SOLID_FRAME_PALETTE_INDEX,
        ),
        crate::pack_test_support::palette_entry(
            "text-window/palette/message_box",
            MESSAGE_BOX_PALETTE_COLOUR_COUNT,
        ),
    ]
}

fn temp_pack_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "pokeemerald-rs-intro-test-pack-{name}-{}.pack",
        std::process::id()
    ))
}

fn load_scene_from_pack(path: &std::path::Path) -> Result<IntroScene, super::IntroSceneError> {
    let pack = assets::pack::AssetPack::load(path)?;
    IntroScene::from_pack(&pack)
}

fn first_glyph_interior_position() -> (usize, usize) {
    (
        usize::try_from(
            STANDARD_BOX_SCREEN_ORIGIN.0 + STANDARD_PRINTER_ORIGIN.0 + GLYPH_INTERIOR_OFFSET,
        )
        .unwrap(),
        usize::try_from(
            STANDARD_BOX_SCREEN_ORIGIN.1 + STANDARD_PRINTER_ORIGIN.1 + GLYPH_INTERIOR_OFFSET,
        )
        .unwrap(),
    )
}

fn first_glyph_pixel(scene: &IntroScene) -> Option<Rgb888> {
    let fb = scene.compose();
    let (x, y) = first_glyph_interior_position();
    fb.pixel(x, y)
}

#[test]
fn a_pack_missing_message_box_fails_to_build_a_scene() {
    let path = temp_pack_path("no-message-box");
    write_pack(&path, vec![font_entry(TRANSPARENT_PALETTE_INDEX)]);
    let pack = assets::pack::AssetPack::load(&path).unwrap();

    let err = IntroScene::from_pack(&pack).unwrap_err();
    assert!(
        matches!(err, super::IntroSceneError::Pack(_)),
        "a pack with no message_box entry at all must fail with a Pack error, got {err:?}"
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn a_second_load_after_the_pack_is_regenerated_sees_the_new_bytes() {
    let path = temp_pack_path("regenerated");

    let mut entries = message_box_entries();
    entries.push(font_entry(DARK_GREY_GLYPH_PALETTE_INDEX));
    write_pack(&path, entries);

    let mut first =
        load_scene_from_pack(&path).expect("the synthetic pack has both required entries");
    first.tick(NO_INPUT);
    assert_eq!(
        first_glyph_pixel(&first),
        Some(DARK_GREY_GLYPH_COLOR),
        "the first load must render the pack that was on disk then"
    );

    let mut entries = message_box_entries();
    entries.push(font_entry(SHADOW_GLYPH_PALETTE_INDEX));
    write_pack(&path, entries);

    let mut second =
        load_scene_from_pack(&path).expect("the regenerated pack is still well-formed");
    second.tick(NO_INPUT);
    assert_eq!(
        first_glyph_pixel(&second),
        Some(SHADOW_GLYPH_COLOR),
        "a load after the pack changed must render the new bytes, not a cached first-load pack"
    );

    assert_eq!(
        first_glyph_pixel(&first),
        Some(DARK_GREY_GLYPH_COLOR),
        "an already-built scene must keep rendering its own owned bytes"
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn without_a_default_pack_repeated_loads_keep_reporting_missing() {
    if assets::pack::AssetPack::default_path().is_file() {
        return;
    }

    let first = super::load_default().unwrap_err();
    assert!(first.is_pack_missing());
    let second = super::load_default().unwrap_err();
    assert!(second.is_pack_missing());
}

fn derive_traversal_runs() -> Vec<TraversalRun> {
    let pixels = transparent_glyph_sheet_pixels();
    let pages = super::speech::pages();
    let mut printer = Printer::new(
        pages[0].clone(),
        synthetic_sheet(&pixels),
        TextSpeed::Mid,
        STANDARD_PRINTER_ORIGIN,
    )
    .with_ab_speed_up_print();

    let mut runs = Vec::new();
    let mut frames_without_input = 0;
    let mut page_index = 0;
    let mut press_confirm_next_frame = false;
    for _ in 0..MAX_TRAVERSAL_TICKS {
        if press_confirm_next_frame {
            printer.tick(APP_A_PRESS);
            press_confirm_next_frame = false;
            continue;
        }
        let event = printer.tick(NO_INPUT);
        frames_without_input += 1;
        match event {
            TickEvent::AwaitingClear | TickEvent::AwaitingScroll => {
                runs.push(TraversalRun {
                    frames: frames_without_input,
                    confirm_after: true,
                });
                frames_without_input = 0;
                press_confirm_next_frame = true;
            }
            TickEvent::ScrollFinished => {
                runs.push(TraversalRun {
                    frames: frames_without_input,
                    confirm_after: false,
                });
                frames_without_input = 0;
            }
            TickEvent::Finished => {
                runs.push(TraversalRun {
                    frames: frames_without_input,
                    confirm_after: false,
                });
                frames_without_input = 0;
                page_index += 1;
                if page_index == NUM_PAGES {
                    return runs;
                }
                printer.restart(pages[page_index].clone());
            }
            _ => {}
        }
    }
    panic!("the speech never terminated: {} runs so far", runs.len());
}

#[test]
fn traversal_runs_match_the_pinned_table() {
    assert_eq!(derive_traversal_runs(), super::TRAVERSAL_RUNS);
}

#[test]
fn traversal_frames_totals_the_table() {
    let total: usize = super::TRAVERSAL_RUNS
        .iter()
        .map(|run| run.frames as usize + usize::from(run.confirm_after))
        .sum();
    assert_eq!(super::TRAVERSAL_FRAMES, total);
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_composes_a_non_blank_intro_frame() {
    let pack = AssetPack::load_repo().expect("run `cargo xtask extract` first");
    let mut scene = IntroScene::from_pack(&pack).expect("run `cargo xtask extract` first");
    for _ in 0..REAL_PACK_COMPOSITION_TICKS {
        scene.tick(NO_INPUT);
    }
    assert!(scene.revealed_glyph_count() >= MINIMUM_REAL_PACK_GLYPHS);

    let fb = scene.compose();
    assert!(
        fb.pixels().iter().any(|&p| p != Rgb888::BLACK),
        "a few ticks in, the real dialogue box and at least one glyph must have painted something"
    );

    let message_box_interior = fb
        .pixel(
            usize::try_from(STANDARD_BOX_SCREEN_ORIGIN.0 + MESSAGE_BOX_INTERIOR_OFFSET).unwrap(),
            usize::try_from(STANDARD_BOX_SCREEN_ORIGIN.1 + MESSAGE_BOX_INTERIOR_OFFSET).unwrap(),
        )
        .expect("in bounds");
    let empty_backdrop = fb.pixel(BACKDROP_PROBE, BACKDROP_PROBE).expect("in bounds");
    assert_ne!(
        message_box_interior, empty_backdrop,
        "the real dialogue box must look different from the empty backdrop around it"
    );
}
