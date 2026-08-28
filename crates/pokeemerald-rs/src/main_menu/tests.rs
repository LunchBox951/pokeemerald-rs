//! Unit tests for [`super::MainMenuScene`] and its private helpers.
//!
//! [`compose_from_synthetic_pack_...`]-style tests build a small
//! **synthetic** pack in memory (mirroring `crate::overworld::tests`'
//! fixture style -- CI has no `pokeemerald/` checkout and no real pack) and
//! exercise the full `MainMenuScene::from_pack` + `compose` pipeline
//! against it. The `real_pack_*` test is `#[ignore]`d and needs a real
//! local pack.

use super::{
    darken_outside, highlight_rect, render_label, ItemWindow, MainMenuItem, MainMenuScene,
    MainMenuSceneError, MainMenuType, HEADER_TEXT_BG, HEADER_TEXT_FG,
};
use crate::textbox::{self, Coverage};
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
    assert_eq!(MainMenuItem::Continue.label(), "CONTINUE");
    assert_eq!(MainMenuItem::NewGame.label(), "NEW GAME");
    assert_eq!(MainMenuItem::Option.label(), "OPTION");
}

/// `sWindowTemplates_MainMenu[0]`/`[1]` (`main_menu.c:291-309`): the
/// `HAS_NO_SAVED_GAME` boxes, unchanged by issue #214.
#[test]
fn no_saved_game_item_windows_match_menu_top_win0_and_win1() {
    let menu = MainMenuType::NoSavedGame;
    assert_eq!(menu.items(), [MainMenuItem::NewGame, MainMenuItem::Option]);
    assert_eq!(
        menu.window(MainMenuItem::NewGame),
        Some(ItemWindow { top: 1, height: 2 })
    );
    assert_eq!(
        menu.window(MainMenuItem::Option),
        Some(ItemWindow { top: 5, height: 2 })
    );
    assert_eq!(
        menu.window(MainMenuItem::Continue),
        None,
        "there is nothing to continue in the no-save list"
    );
}

// -- `highlight_rect` (main_menu.c:283-284's `MENU_WIN_HCOORDS`/`MENU_WIN_VCOORDS`) --

pub(super) fn window_of(menu: MainMenuType, item: MainMenuItem) -> ItemWindow {
    menu.window(item).expect("item belongs to this menu type")
}

#[test]
fn highlight_rect_matches_upstream_win0_coords_for_new_game() {
    // MENU_WIN_HCOORDS = WIN_RANGE(9, 231); MENU_WIN_VCOORDS(0) = WIN_RANGE(1, 31).
    assert_eq!(
        highlight_rect(window_of(MainMenuType::NoSavedGame, MainMenuItem::NewGame)),
        (9, 1, 231, 31)
    );
}

#[test]
fn highlight_rect_matches_upstream_win0_coords_for_option() {
    // Same MENU_WIN_HCOORDS; MENU_WIN_VCOORDS(1) = WIN_RANGE(33, 63).
    assert_eq!(
        highlight_rect(window_of(MainMenuType::NoSavedGame, MainMenuItem::Option)),
        (9, 33, 231, 63)
    );
}

// -- `move_up`/`move_down` (main_menu.c:903-925, no wrap) -----------------

#[test]
fn selection_starts_on_new_game_and_moves_without_wrapping() {
    let mut menu = super::synthetic_scene(MainMenuType::NoSavedGame);
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

/// A framebuffer filled `bright` everywhere, with `bg0` recording the
/// `0..40 x 0..40` square as the only BG0-painted region -- so the rest of
/// the frame stands in for the backdrop showing through transparent BG0.
fn darken_fixture(bright: Rgb888) -> (Framebuffer, Coverage) {
    let mut fb = Framebuffer::new();
    fb.fill(bright);
    let mut bg0 = Coverage::recording();
    textbox::fill_rect_tracked(&mut fb, &mut bg0, (0, 0), 40, 40, bright);
    (fb, bg0)
}

#[test]
fn darken_outside_leaves_the_rect_untouched_and_darkens_painted_pixels_outside_it() {
    let bright = Rgb888 {
        r: 200,
        g: 200,
        b: 200,
    };
    let (mut fb, bg0) = darken_fixture(bright);

    darken_outside(&mut fb, &bg0, (10, 10, 20, 20));

    // Inside the rect: untouched.
    assert_eq!(fb.pixel(10, 10), Some(bright));
    assert_eq!(fb.pixel(19, 19), Some(bright));

    // Outside the rect but painted by BG0: darkened by the exact
    // `rendering::darken` formula this module cites (`BLDY` EVY=7).
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

#[test]
fn darken_outside_leaves_unpainted_backdrop_pixels_alone() {
    // `BLDCNT_EFFECT_DARKEN | BLDCNT_TGT1_BG0` (`main_menu.c:751`) names BG0
    // alone as the first target -- never `BLDCNT_TGT1_BD`
    // (`include/gba/io_reg.h:595`) -- so a pixel BG0 never painted keeps the
    // backdrop's own full-brightness colour even outside `WIN0`.
    let bright = Rgb888 {
        r: 200,
        g: 200,
        b: 200,
    };
    let (mut fb, bg0) = darken_fixture(bright);

    darken_outside(&mut fb, &bg0, (10, 10, 20, 20));

    assert_eq!(
        fb.pixel(100, 100),
        Some(bright),
        "an unpainted (transparent-BG0) pixel outside WIN0 must not darken"
    );
    assert_eq!(
        fb.pixel(45, 5),
        Some(bright),
        "an unpainted pixel level with the painted region must not darken either"
    );
}

// -- Header text colours (main_menu.c:758/761/764) -----------------------

/// Render `"A"` from a synthetic `FONT_NORMAL` sheet whose every pixel
/// carries palette `index` -- a known, non-zero glyph pattern -- and blit it
/// through [`super::HEADER_GLYPH_COLORS`] at `(0, 0)` onto an all-black
/// framebuffer, so any painted pixel is unambiguous.
fn blit_header_glyph_of_index(index: u8) -> Framebuffer {
    let pixels = vec![index; (assets::fonts::SHEET_WIDTH * assets::fonts::SHEET_HEIGHT) as usize];
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

    let glyphs = render_label("A", sheet);
    assert_eq!(glyphs.len(), 1, "one character reveals exactly one glyph");
    let mut fb = Framebuffer::new();
    textbox::blit_glyphs_colored(
        &mut fb,
        &glyphs,
        (0, 0),
        (
            i32::try_from(Framebuffer::WIDTH).unwrap(),
            i32::try_from(Framebuffer::HEIGHT).unwrap(),
        ),
        &super::HEADER_GLYPH_COLORS,
    );
    fb
}

#[test]
fn header_glyph_colors_map_each_font_index_to_the_upstream_patched_palette() {
    // The three colours `Task_DisplayMainMenu` patches into bank 15 before
    // the first frame draws, as raw 5-bit `RGB()` literals -- deliberately
    // spelled out here rather than imported from the constants under test:
    // `RGB_WHITE` at 0xA (`main_menu.c:758`), `RGB(12, 12, 12)` at 0xB
    // (`main_menu.c:761`), `RGB(26, 26, 25)` at 0xC (`main_menu.c:764`),
    // read through `sTextColor_Headers`' bg/fg/shadow order
    // (`main_menu.c:409`).
    let fg = rendering::Bgr555::from_channels(12, 12, 12).to_rgb888();
    let shadow = rendering::Bgr555::from_channels(26, 26, 25).to_rgb888();
    assert_ne!(fg, shadow, "the two literals must be distinguishable");

    // Font index 1 (`col[1]`) -> foreground.
    let fb = blit_header_glyph_of_index(1);
    assert_eq!(fb.pixel(0, 0), Some(fg));
    assert_eq!(fb.pixel(7, 7), Some(fg));

    // Font index 2 (`col[2]`) -> shadow.
    let fb = blit_header_glyph_of_index(2);
    assert_eq!(fb.pixel(0, 0), Some(shadow));
    assert_eq!(fb.pixel(7, 7), Some(shadow));

    // Font index 0 (`col[0]`, the glyph cell's own background) and index 3
    // (the unused box colour) are transparent: `draw_item` has already
    // filled the whole content rect `RGB_WHITE`
    // (`FillWindowPixelBuffer(PIXEL_FILL(0xA))`, `main_menu.c:784`) before
    // any glyph draws, so neither may paint anything.
    for transparent_index in [0u8, 3] {
        let fb = blit_header_glyph_of_index(transparent_index);
        assert!(
            fb.pixels().iter().all(|&p| p == Rgb888::BLACK),
            "font index {transparent_index} must paint nothing"
        );
    }

    // ...and the fill it relies on is that same `RGB_WHITE` literal.
    assert_eq!(
        HEADER_TEXT_BG,
        rendering::Bgr555::from_channels(31, 31, 31).to_rgb888()
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
/// both the content fill and the backdrop), a `font/normal/glyphs` sheet
/// whose every pixel is `font_index`, and a `interface/palette/main_menu_bg`
/// whose index 0 is a colour distinct from both the content fill white and
/// the border colour.
///
/// `font_index` picks the fixture flavour: `0` (transparent everywhere)
/// keeps every label pixel showing the *fill* underneath, so the
/// fill/border/backdrop tests can assert those layers without glyph
/// interference; `1` makes every glyph cell a solid block of the header
/// *foreground* colour, so the label-path tests can pin the glyph blit's
/// own darkening, clip, and origin offset (the colour mapping itself is
/// pinned separately by
/// [`header_glyph_colors_map_each_font_index_to_the_upstream_patched_palette`],
/// which drives `textbox::blit_glyphs_colored` directly).
fn synthetic_main_menu_pack_bytes(font_index: u8) -> Vec<u8> {
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

    // Font sheet: every pixel `font_index` (see the doc comment above).
    let font_pixels =
        vec![font_index; (assets::fonts::SHEET_WIDTH * assets::fonts::SHEET_HEIGHT) as usize];

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

struct TempPackGuard {
    path: std::path::PathBuf,
}

impl TempPackGuard {
    fn new(path: std::path::PathBuf) -> Self {
        Self { path }
    }

    fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for TempPackGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn load_synthetic_scene() -> MainMenuScene {
    load_synthetic_scene_with_font(0)
}

/// [`load_synthetic_scene`], for whichever item list is under test (the
/// `HAS_SAVED_GAME` cases live in [`super::saved_game_tests`]).
pub(super) fn load_synthetic_scene_of(menu_type: MainMenuType) -> MainMenuScene {
    load_synthetic_scene_inner(0, menu_type)
}

/// [`load_synthetic_scene`], with the font-sheet flavour spelled out (see
/// [`synthetic_main_menu_pack_bytes`]'s doc comment for what each
/// `font_index` pins).
fn load_synthetic_scene_with_font(font_index: u8) -> MainMenuScene {
    load_synthetic_scene_inner(font_index, MainMenuType::NoSavedGame)
}

fn load_synthetic_scene_inner(font_index: u8, menu_type: MainMenuType) -> MainMenuScene {
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-main-menu-test-{}-{:?}-{font_index}.pack",
        std::process::id(),
        std::thread::current().id()
    ));
    let temp_pack = TempPackGuard::new(path);
    std::fs::write(temp_pack.path(), synthetic_main_menu_pack_bytes(font_index)).unwrap();
    let pack = AssetPack::load(temp_pack.path()).unwrap();
    MainMenuScene::from_pack(&pack, menu_type).unwrap()
}

#[test]
fn temp_pack_cleanup_is_unwind_safe() {
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-main-menu-unwind-test-{}-{:?}.pack",
        std::process::id(),
        std::thread::current().id()
    ));

    let result = std::panic::catch_unwind(|| {
        let temp_pack = TempPackGuard::new(path.clone());
        std::fs::write(temp_pack.path(), synthetic_main_menu_pack_bytes(0)).unwrap();
        assert!(temp_pack.path().exists());
        panic!("deliberate panic to exercise temporary pack cleanup");
    });

    assert!(result.is_err(), "the deliberate panic must be observed");
    assert!(!path.exists(), "temporary pack must be removed on unwind");
}

#[test]
fn compose_from_synthetic_pack_shows_the_extracted_bg_palette_backdrop_undarkened() {
    let scene = load_synthetic_scene();
    let fb = scene.compose();

    let dark_blue = rendering::Bgr555::from_channels(4, 4, 16).to_rgb888();
    // A pixel below both item windows (tile row 8, well past OPTION's own
    // bordered box which ends at tile row 8 -- `MENU_TOP_WIN1` (5) +
    // `MENU_HEIGHT_WIN1` (2) + the border's own 1 tile) shows the raw
    // backdrop colour at *full brightness*, even though it lies outside the
    // selection highlight: BG0 is transparent there (nothing but the two
    // windows is ever drawn into it) and `BLDCNT`'s first-target set is
    // `BLDCNT_TGT1_BG0` without `BLDCNT_TGT1_BD` (`main_menu.c:751`,
    // `include/gba/io_reg.h:595`), so the darken never reaches the backdrop.
    assert_eq!(fb.pixel(5, 130), Some(dark_blue));
    assert_ne!(
        fb.pixel(5, 130),
        Some(rendering::darken(dark_blue, 7)),
        "the backdrop is not a blend first target and must never darken"
    );

    // Contrast: a pixel the *windows* painted outside WIN0 -- OPTION's own
    // content fill, unselected -- is darkened by the same pass.
    assert_eq!(
        fb.pixel(18, 42),
        Some(rendering::darken(HEADER_TEXT_BG, 7)),
        "BG0's own pixels outside WIN0 are the first target and must darken"
    );
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

    // OPTION's own top-left border corner -- tile (1, 4) -> pixel (8, 32),
    // +2 in -- lies *outside* NEW GAME's WIN0 highlight rect (which ends at
    // y=31), so it must be darkened like every other BG0 pixel outside it:
    // the border blit is a `BLDCNT_TGT1_BG0` first target too, not just the
    // content fill.
    assert_eq!(fb.pixel(10, 34), Some(rendering::darken(green, 7)));
}

#[test]
fn compose_with_opaque_font_darkens_the_unselected_items_label_glyphs() {
    let scene = load_synthetic_scene_with_font(1);
    let fb = scene.compose();

    // The opaque fixture turns every glyph cell into a solid block of the
    // header foreground colour (font index 1 -> `HEADER_GLYPH_COLORS[1]`).
    // NEW GAME is selected: a pixel inside its first label glyph (label
    // origin (16, 9), +1 into the cell) stays at full brightness...
    assert_eq!(fb.pixel(17, 11), Some(HEADER_TEXT_FG));

    // ...while OPTION's label (label origin (16, 41)) sits outside WIN0 and
    // must darken along with its fill -- glyph pixels are BG0's own painted
    // pixels, first targets of the same `BLDY` darken.
    assert_eq!(
        fb.pixel(17, 43),
        Some(rendering::darken(HEADER_TEXT_FG, 7)),
        "an unselected item's label glyphs must darken with its window"
    );
}

#[test]
fn compose_with_opaque_font_keeps_the_1px_text_origin_offset_and_clips_to_the_content_rect() {
    let scene = load_synthetic_scene_with_font(1);
    let fb = scene.compose();

    // `AddTextPrinterParameterized3(_, FONT_NORMAL, 0, 1, ...)`'s y=1
    // window-local origin (`main_menu.c:786-787`): NEW GAME's content rect
    // spans y 8..24, so its top row (y=8) is still the plain content fill --
    // the first glyph row lands one pixel down, at y=9.
    assert_eq!(
        fb.pixel(17, 8),
        Some(HEADER_TEXT_BG),
        "the content rect's own top row is above the y=1 text origin"
    );
    assert_eq!(
        fb.pixel(17, 9),
        Some(HEADER_TEXT_FG),
        "the first glyph row starts exactly at the y=1 text origin"
    );

    // The clip (`label_clip`'s `content_size.1 - 1`) lets the glyph reach
    // the content rect's own last row (y=23) and no further: the border row
    // below (y=24) keeps the frame's own colour. Both pixels sit inside NEW
    // GAME's WIN0 highlight, so neither is darkened.
    assert_eq!(
        fb.pixel(17, 23),
        Some(HEADER_TEXT_FG),
        "the last content row is still glyph-reachable"
    );
    let green = rendering::Bgr555::from_channels(0, 31, 0).to_rgb888();
    assert_eq!(
        fb.pixel(17, 24),
        Some(green),
        "the border row below the content rect must never take glyph pixels"
    );
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

/// Loads [`AssetPack::load_repo`] directly rather than
/// [`super::load_default`] (issue #412) -- see [`AssetPack::load_repo`]'s
/// own docs for why a checkout-validation gate must not go through
/// [`AssetPack::default_path`].
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_composes_non_blank_deterministic_frames_for_both_selection_states() {
    let pack = AssetPack::load_repo().expect("run `cargo xtask extract` first");
    let mut scene = MainMenuScene::from_pack(&pack, MainMenuType::NoSavedGame)
        .expect("run `cargo xtask extract` first");

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
