//! Unit tests for [`super::MainMenuScene`] and its private helpers.
//!
//! [`compose_from_synthetic_pack_...`]-style tests build a small
//! **synthetic** pack in memory (mirroring `crate::overworld::tests`'
//! fixture style -- CI has no `pokeemerald/` checkout and no real pack) and
//! exercise the full `MainMenuScene::from_pack` + `compose` pipeline
//! against it. The `real_pack_*` test is `#[ignore]`d and needs a real
//! local pack.

use super::{
    darken_outside, highlight_rect, render_label, MainMenuItem, MainMenuScene, MainMenuSceneError,
    HEADER_TEXT_BG,
};
use assets::pack::{AssetPack, ImageRef, PackError};
use rendering::{Framebuffer, Rgb888};

// -- `render_label` -----------------------------------------------------

#[test]
fn render_label_reveals_one_glyph_per_character_left_to_right() {
    let pixels = vec![0u8; (assets::fonts::SHEET_WIDTH * assets::fonts::SHEET_HEIGHT) as usize];
    let image = ImageRef {
        width: assets::fonts::SHEET_WIDTH,
        height: assets::fonts::SHEET_HEIGHT,
        bit_depth: 2,
        pixels: &pixels,
    };
    let sheet = assets::fonts::FontGlyphSheet::new(assets::fonts::FontImageRef::new_for_tests(
        assets::fonts::FontId::Normal,
        image,
    ))
    .unwrap();

    let glyphs = render_label("AB", sheet);
    assert_eq!(glyphs.len(), 2);
    assert_eq!(glyphs[0].x, 0);
    assert!(
        glyphs[1].x > glyphs[0].x,
        "glyphs must advance left to right"
    );
}

// -- `MainMenuSceneError` ------------------------------------------------

#[test]
fn is_pack_missing_matches_not_found_only() {
    let err = MainMenuSceneError::Pack(PackError::NotFound(std::path::PathBuf::from("x")));
    assert!(err.is_pack_missing());
    let err = MainMenuSceneError::Pack(PackError::UnknownAsset("x".into()));
    assert!(!err.is_pack_missing());
}

// -- `MainMenuItem` geometry (main_menu.c:259-309) -----------------------

#[test]
fn item_labels_match_upstream_strings() {
    assert_eq!(MainMenuItem::NewGame.label(), "NEW GAME");
    assert_eq!(MainMenuItem::Option.label(), "OPTION");
}

#[test]
fn item_top_tiles_match_menu_top_win0_and_win1() {
    assert_eq!(MainMenuItem::NewGame.top_tile(), 1); // MENU_TOP_WIN0
    assert_eq!(MainMenuItem::Option.top_tile(), 5); // MENU_TOP_WIN1
}

// -- `highlight_rect` (main_menu.c:283-284's `MENU_WIN_HCOORDS`/`MENU_WIN_VCOORDS`) --

#[test]
fn highlight_rect_matches_upstream_win0_coords_for_new_game() {
    // MENU_WIN_HCOORDS = WIN_RANGE(9, 231); MENU_WIN_VCOORDS(0) = WIN_RANGE(1, 31).
    assert_eq!(
        highlight_rect(MainMenuItem::NewGame.top_tile()),
        (9, 1, 231, 31)
    );
}

#[test]
fn highlight_rect_matches_upstream_win0_coords_for_option() {
    // Same MENU_WIN_HCOORDS; MENU_WIN_VCOORDS(1) = WIN_RANGE(33, 63).
    assert_eq!(
        highlight_rect(MainMenuItem::Option.top_tile()),
        (9, 33, 231, 63)
    );
}

// -- `move_up`/`move_down` (main_menu.c:903-925, no wrap) -----------------

#[test]
fn selection_starts_on_new_game_and_moves_without_wrapping() {
    let mut menu = super::synthetic_scene();
    assert_eq!(menu.selected(), MainMenuItem::NewGame);

    menu.move_up();
    assert_eq!(
        menu.selected(),
        MainMenuItem::NewGame,
        "DPAD_UP on the first item must not wrap"
    );

    menu.move_down();
    assert_eq!(menu.selected(), MainMenuItem::Option);

    menu.move_down();
    assert_eq!(
        menu.selected(),
        MainMenuItem::Option,
        "DPAD_DOWN on the last item must not wrap"
    );

    menu.move_up();
    assert_eq!(menu.selected(), MainMenuItem::NewGame);
}

// -- `darken_outside` (main_menu.c:745-753's WIN0+BLDCNT+BLDY) ------------

#[test]
fn darken_outside_leaves_the_rect_untouched_and_darkens_everything_else() {
    let bright = Rgb888 {
        r: 200,
        g: 200,
        b: 200,
    };
    let mut fb = Framebuffer::new();
    fb.fill(bright);

    darken_outside(&mut fb, (10, 10, 20, 20));

    // Inside the rect: untouched.
    assert_eq!(fb.pixel(10, 10), Some(bright));
    assert_eq!(fb.pixel(19, 19), Some(bright));

    // Outside the rect: darkened by the exact `rendering::darken` formula
    // this module cites (`BLDY` EVY=7).
    let darkened = rendering::darken(bright, 7);
    assert_ne!(
        darkened, bright,
        "the fixture's darken weight must be visible"
    );
    assert_eq!(fb.pixel(0, 0), Some(darkened));
    assert_eq!(
        fb.pixel(20, 20),
        Some(darkened),
        "the rect's own far edge is excluded (half-open)"
    );
}

// -- End-to-end against a synthetic pack -----------------------------------

/// One directory entry for [`write_synthetic_pack`], mirroring
/// `crate::overworld::tests`' own fixture-building style (that module's
/// helper is private to `overworld::tests`, so this is a small independent
/// copy rather than a shared one).
struct Entry {
    id: &'static str,
    kind_tag: u8,
    meta: Vec<u8>,
    payload: Vec<u8>,
}

fn write_synthetic_pack(mut entries: Vec<Entry>) -> Vec<u8> {
    entries.sort_by(|a, b| a.id.cmp(b.id));

    let header_size = 8 + 4 + 4;
    let mut directory_size = 0usize;
    for e in &entries {
        directory_size += 2 + e.id.len() + 1 + 8 + 8 + e.meta.len();
    }
    let mut offset = header_size + directory_size;
    let mut offsets = Vec::new();
    for e in &entries {
        offsets.push(offset);
        offset += e.payload.len();
    }

    let mut out = Vec::new();
    out.extend_from_slice(&assets::pack::MAGIC);
    out.extend_from_slice(&assets::pack::FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&u32::try_from(entries.len()).unwrap().to_le_bytes());
    for (e, &off) in entries.iter().zip(&offsets) {
        out.extend_from_slice(&u16::try_from(e.id.len()).unwrap().to_le_bytes());
        out.extend_from_slice(e.id.as_bytes());
        out.push(e.kind_tag);
        out.extend_from_slice(&u64::try_from(off).unwrap().to_le_bytes());
        out.extend_from_slice(&u64::try_from(e.payload.len()).unwrap().to_le_bytes());
        out.extend_from_slice(&e.meta);
    }
    for e in &entries {
        out.extend_from_slice(&e.payload);
    }
    out
}

fn image_meta(width: u32, height: u32, bit_depth: u8) -> Vec<u8> {
    let mut m = Vec::new();
    m.extend_from_slice(&width.to_le_bytes());
    m.extend_from_slice(&height.to_le_bytes());
    m.push(bit_depth);
    m
}

fn palette_meta(color_count: u16) -> Vec<u8> {
    color_count.to_le_bytes().to_vec()
}

/// A minimal pack covering exactly what [`super::MainMenuScene::from_pack`]
/// needs: a 24x24 (3x3-tile) selectable window frame (every ring tile
/// opaque, palette index 1, so the border is trivially distinguishable from
/// both the content fill and the darkened backdrop), a blank
/// `font/normal/glyphs` sheet (transparent everywhere -- this fixture only
/// needs the label glyphs to *exist*, not to be visually legible; label
/// colour mapping is pinned directly against `textbox::blit_glyphs_colored`
/// instead), and a `interface/palette/main_menu_bg` whose index 0 is a
/// colour distinct from both the content fill white and the border colour.
fn synthetic_main_menu_pack_bytes() -> Vec<u8> {
    // 24x24 frame sheet: every ring tile (the 8 border cells `border_tiles`
    // draws) opaque, palette index 1. Filling the whole sheet with index 1
    // is simplest and correct here: `border_tiles` only ever draws pixels
    // from within the sheet's own tile cells, and this fixture never reads
    // interior (non-ring) tiles.
    let frame_pixels = vec![1u8; 24 * 24];

    // Palette bank: index 0 transparent (unused by `blit_frame_tiles`),
    // index 1 a distinct bright green, rest black.
    let mut frame_palette = vec![0u8; 32];
    let green = rendering::Bgr555::from_channels(0, 31, 0).raw();
    frame_palette[2..4].copy_from_slice(&green.to_le_bytes());

    // Blank font sheet (module docs): every pixel index 0 (transparent).
    let font_pixels =
        vec![0u8; (assets::fonts::SHEET_WIDTH * assets::fonts::SHEET_HEIGHT) as usize];

    // Background palette: index 0 a distinct dark blue, rest black.
    let mut bg_palette = vec![0u8; 32];
    let dark_blue = rendering::Bgr555::from_channels(4, 4, 16).raw();
    bg_palette[0..2].copy_from_slice(&dark_blue.to_le_bytes());

    write_synthetic_pack(vec![
        Entry {
            id: "text-window/image/1",
            kind_tag: 0,
            meta: image_meta(24, 24, 4),
            payload: frame_pixels,
        },
        Entry {
            id: "text-window/palette/1",
            kind_tag: 1,
            meta: palette_meta(16),
            payload: frame_palette,
        },
        Entry {
            id: "font/normal/glyphs",
            kind_tag: 0,
            meta: image_meta(assets::fonts::SHEET_WIDTH, assets::fonts::SHEET_HEIGHT, 2),
            payload: font_pixels,
        },
        Entry {
            id: "interface/palette/main_menu_bg",
            kind_tag: 1,
            meta: palette_meta(16),
            payload: bg_palette,
        },
    ])
}

fn load_synthetic_scene() -> MainMenuScene {
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-main-menu-test-{}-{:?}.pack",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::write(&path, synthetic_main_menu_pack_bytes()).unwrap();
    let pack = AssetPack::load(&path).unwrap();
    let scene = MainMenuScene::from_pack(&pack).unwrap();
    let _ = std::fs::remove_file(path);
    scene
}

#[test]
fn compose_from_synthetic_pack_fills_the_backdrop_from_the_extracted_bg_palette() {
    let scene = load_synthetic_scene();
    let fb = scene.compose();

    let dark_blue = rendering::Bgr555::from_channels(4, 4, 16).to_rgb888();
    // A pixel below both item windows (tile row 8, well past OPTION's own
    // bordered box which ends at tile row 8 -- `MENU_TOP_WIN1` (5) +
    // `MENU_HEIGHT_WIN1` (2) + the border's own 1 tile) must still show the
    // raw backdrop colour, darkened (it's outside the selection highlight).
    let expected = rendering::darken(dark_blue, 7);
    assert_eq!(fb.pixel(5, 130), Some(expected));
}

#[test]
fn compose_from_synthetic_pack_fills_selected_items_content_with_the_upstream_header_bg_and_leaves_it_undarkened(
) {
    let scene = load_synthetic_scene();
    let fb = scene.compose();

    // NEW GAME is selected by default; a pixel well inside its content rect
    // (tile (2,1) -> pixel (16,8), +2 to clear the border ring) must be the
    // exact upstream `HEADER_TEXT_BG` (`RGB_WHITE` post-patch), completely
    // undarkened.
    assert_eq!(fb.pixel(18, 10), Some(HEADER_TEXT_BG));
}

#[test]
fn compose_from_synthetic_pack_darkens_the_unselected_items_content() {
    let scene = load_synthetic_scene();
    let fb = scene.compose();

    // OPTION (tile (2,5) -> pixel (16,40), +2 to clear the border ring) is
    // not selected -- its own content fill must be `HEADER_TEXT_BG`,
    // darkened.
    let expected = rendering::darken(HEADER_TEXT_BG, 7);
    assert_eq!(fb.pixel(18, 42), Some(expected));
}

#[test]
fn compose_from_synthetic_pack_draws_the_border_from_the_extracted_frame_palette() {
    let scene = load_synthetic_scene();
    let fb = scene.compose();

    // NEW GAME's own top-left border corner cell: tile (1, 0) (one tile
    // left/up of the content rect) -- must show the frame's palette index 1
    // colour (bright green, module docs), undarkened (inside WIN0's own
    // highlight rect, which starts one row above the content rect too).
    // Tile (1, 0) -> pixel (8, 0), +2 into the corner tile's own body.
    let green = rendering::Bgr555::from_channels(0, 31, 0).to_rgb888();
    assert_eq!(fb.pixel(10, 2), Some(green));
}

#[test]
fn compose_from_synthetic_pack_is_deterministic_and_selection_changes_the_frame() {
    let mut scene = load_synthetic_scene();

    let first = scene.compose();
    let second = scene.compose();
    assert_eq!(
        first.pixels(),
        second.pixels(),
        "composing the same selection twice must be deterministic"
    );

    scene.move_down();
    let after_move = scene.compose();
    assert_ne!(
        first.pixels(),
        after_move.pixels(),
        "moving the selection must change the composed highlight"
    );
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_composes_non_blank_deterministic_frames_for_both_selection_states() {
    let mut scene = super::load_default().expect("run `cargo xtask extract` first");

    let new_game_first = scene.compose();
    let new_game_second = scene.compose();
    assert_eq!(
        new_game_first.pixels(),
        new_game_second.pixels(),
        "composing the same selection twice must be deterministic"
    );
    assert!(
        new_game_first.pixels().iter().any(|&p| p != Rgb888::BLACK),
        "the NEW GAME frame must be non-blank"
    );

    scene.move_down();
    assert_eq!(scene.selected(), MainMenuItem::Option);
    let option_first = scene.compose();
    let option_second = scene.compose();
    assert_eq!(
        option_first.pixels(),
        option_second.pixels(),
        "composing the OPTION selection twice must be deterministic"
    );
    assert!(
        option_first.pixels().iter().any(|&p| p != Rgb888::BLACK),
        "the OPTION frame must be non-blank"
    );

    assert_ne!(
        new_game_first.pixels(),
        option_first.pixels(),
        "the two selection states must render a different highlight"
    );

    scene.move_down();
    assert_eq!(
        scene.selected(),
        MainMenuItem::Option,
        "DPAD_DOWN on the last item must not wrap"
    );
    scene.move_up();
    assert_eq!(scene.selected(), MainMenuItem::NewGame);
    scene.move_up();
    assert_eq!(
        scene.selected(),
        MainMenuItem::NewGame,
        "DPAD_UP on the first item must not wrap"
    );
}
