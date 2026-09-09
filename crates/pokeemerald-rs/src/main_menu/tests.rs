use super::{
    darken_outside, highlight_rect, render_label, ItemWindow, MainMenuItem, MainMenuScene,
    MainMenuSceneError, MainMenuType, HEADER_TEXT_BG, HEADER_TEXT_FG,
};
use crate::textbox::{self, Coverage};
use assets::pack::{AssetPack, ImageRef, PackError};
use rendering::{Bgr555, Framebuffer, Rgb888};

const DARKEN_WEIGHT: u8 = 7;
const IMAGE_KIND_TAG: u8 = 0;
const PALETTE_KIND_TAG: u8 = 1;
const FONT_BIT_DEPTH: u8 = 2;
const FRAME_BIT_DEPTH: u8 = 4;
const FRAME_SIDE: u32 = 24;
const PALETTE_COLOUR_COUNT: u16 = 16;
const FRAME_BORDER_PALETTE_INDEX: u8 = 1;
const TRANSPARENT_FONT_INDEX: u8 = 0;
const OPAQUE_FOREGROUND_FONT_INDEX: u8 = 1;

#[test]
fn render_label_reveals_one_glyph_per_character_left_to_right() {
    let pixels = vec![0u8; (assets::fonts::SHEET_WIDTH * assets::fonts::SHEET_HEIGHT) as usize];
    let image = ImageRef {
        width: assets::fonts::SHEET_WIDTH,
        height: assets::fonts::SHEET_HEIGHT,
        bit_depth: FONT_BIT_DEPTH,
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

#[test]
fn is_pack_missing_matches_not_found_only() {
    let err = MainMenuSceneError::Pack(PackError::NotFound(std::path::PathBuf::from("x")));
    assert!(err.is_pack_missing());
    let err = MainMenuSceneError::Pack(PackError::UnknownAsset("x".into()));
    assert!(!err.is_pack_missing());
}

#[test]
fn item_labels_match_upstream_strings() {
    assert_eq!(MainMenuItem::Continue.label(), "CONTINUE");
    assert_eq!(MainMenuItem::NewGame.label(), "NEW GAME");
    assert_eq!(MainMenuItem::Option.label(), "OPTION");
}

#[test]
fn no_saved_game_items_have_the_upstream_order_and_geometry() {
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

pub(super) fn window_of(menu: MainMenuType, item: MainMenuItem) -> ItemWindow {
    menu.window(item).expect("item belongs to this menu type")
}

#[test]
fn highlight_rect_matches_upstream_win0_coords_for_new_game() {
    assert_eq!(
        highlight_rect(window_of(MainMenuType::NoSavedGame, MainMenuItem::NewGame)),
        (9, 1, 231, 31)
    );
}

#[test]
fn highlight_rect_matches_upstream_win0_coords_for_option() {
    assert_eq!(
        highlight_rect(window_of(MainMenuType::NoSavedGame, MainMenuItem::Option)),
        (9, 33, 231, 63)
    );
}

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

    assert_eq!(fb.pixel(10, 10), Some(bright));
    assert_eq!(fb.pixel(19, 19), Some(bright));

    let darkened = rendering::darken(bright, DARKEN_WEIGHT);
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

fn blit_header_glyph_of_index(index: u8) -> Framebuffer {
    let pixels = vec![index; (assets::fonts::SHEET_WIDTH * assets::fonts::SHEET_HEIGHT) as usize];
    let image = ImageRef {
        width: assets::fonts::SHEET_WIDTH,
        height: assets::fonts::SHEET_HEIGHT,
        bit_depth: FONT_BIT_DEPTH,
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
    const BACKGROUND_INDEX: u8 = 0;
    const FOREGROUND_INDEX: u8 = 1;
    const SHADOW_INDEX: u8 = 2;
    const BOX_INDEX: u8 = 3;

    let fg = Bgr555::from_channels(12, 12, 12).to_rgb888();
    let shadow = Bgr555::from_channels(26, 26, 25).to_rgb888();
    assert_ne!(fg, shadow, "the two literals must be distinguishable");

    let fb = blit_header_glyph_of_index(FOREGROUND_INDEX);
    assert_eq!(fb.pixel(0, 0), Some(fg));
    assert_eq!(fb.pixel(7, 7), Some(fg));

    let fb = blit_header_glyph_of_index(SHADOW_INDEX);
    assert_eq!(fb.pixel(0, 0), Some(shadow));
    assert_eq!(fb.pixel(7, 7), Some(shadow));

    for transparent_index in [BACKGROUND_INDEX, BOX_INDEX] {
        let fb = blit_header_glyph_of_index(transparent_index);
        assert!(
            fb.pixels().iter().all(|&p| p == Rgb888::BLACK),
            "font index {transparent_index} must paint nothing"
        );
    }

    assert_eq!(
        HEADER_TEXT_BG,
        Bgr555::from_channels(31, 31, 31).to_rgb888()
    );
}
struct SyntheticPackEntry {
    asset_id: &'static str,
    kind_tag: u8,
    metadata: Vec<u8>,
    payload: Vec<u8>,
}

fn write_synthetic_pack(mut entries: Vec<SyntheticPackEntry>) -> Vec<u8> {
    entries.sort_by(|left, right| left.asset_id.cmp(right.asset_id));

    let header_size = assets::pack::MAGIC.len() + size_of::<u32>() + size_of::<u32>();
    let mut directory_size = 0usize;
    for entry in &entries {
        directory_size += size_of::<u16>()
            + entry.asset_id.len()
            + size_of::<u8>()
            + size_of::<u64>()
            + size_of::<u64>()
            + entry.metadata.len();
    }
    let mut offset = header_size + directory_size;
    let mut payload_offsets = Vec::new();
    for entry in &entries {
        payload_offsets.push(offset);
        offset += entry.payload.len();
    }

    let mut pack_bytes = Vec::new();
    pack_bytes.extend_from_slice(&assets::pack::MAGIC);
    pack_bytes.extend_from_slice(&assets::pack::FORMAT_VERSION.to_le_bytes());
    pack_bytes.extend_from_slice(&u32::try_from(entries.len()).unwrap().to_le_bytes());
    for (entry, &payload_offset) in entries.iter().zip(&payload_offsets) {
        pack_bytes.extend_from_slice(&u16::try_from(entry.asset_id.len()).unwrap().to_le_bytes());
        pack_bytes.extend_from_slice(entry.asset_id.as_bytes());
        pack_bytes.push(entry.kind_tag);
        pack_bytes.extend_from_slice(&u64::try_from(payload_offset).unwrap().to_le_bytes());
        pack_bytes.extend_from_slice(&u64::try_from(entry.payload.len()).unwrap().to_le_bytes());
        pack_bytes.extend_from_slice(&entry.metadata);
    }
    for entry in &entries {
        pack_bytes.extend_from_slice(&entry.payload);
    }
    pack_bytes
}

fn image_metadata(width: u32, height: u32, bit_depth: u8) -> Vec<u8> {
    let mut metadata = Vec::new();
    metadata.extend_from_slice(&width.to_le_bytes());
    metadata.extend_from_slice(&height.to_le_bytes());
    metadata.push(bit_depth);
    metadata
}

fn palette_metadata(color_count: u16) -> Vec<u8> {
    color_count.to_le_bytes().to_vec()
}

fn palette_with_color(index: u8, color: Bgr555) -> Vec<u8> {
    let mut palette = vec![0u8; usize::from(PALETTE_COLOUR_COUNT) * size_of::<u16>()];
    let offset = usize::from(index) * size_of::<u16>();
    palette[offset..offset + size_of::<u16>()].copy_from_slice(&color.raw().to_le_bytes());
    palette
}

fn synthetic_main_menu_pack_bytes(font_index: u8) -> Vec<u8> {
    let frame_pixel_count = usize::try_from(FRAME_SIDE * FRAME_SIDE).unwrap();
    let frame0_pixels = vec![FRAME_BORDER_PALETTE_INDEX; frame_pixel_count];
    let frame0_palette =
        palette_with_color(FRAME_BORDER_PALETTE_INDEX, Bgr555::from_channels(0, 31, 0));
    let frame5_pixels = vec![FRAME_BORDER_PALETTE_INDEX; frame_pixel_count];
    let frame5_palette =
        palette_with_color(FRAME_BORDER_PALETTE_INDEX, Bgr555::from_channels(31, 0, 0));
    let font_pixels =
        vec![font_index; (assets::fonts::SHEET_WIDTH * assets::fonts::SHEET_HEIGHT) as usize];
    let bg_palette = palette_with_color(0, Bgr555::from_channels(4, 4, 16));

    write_synthetic_pack(vec![
        SyntheticPackEntry {
            asset_id: "text-window/image/1",
            kind_tag: IMAGE_KIND_TAG,
            metadata: image_metadata(FRAME_SIDE, FRAME_SIDE, FRAME_BIT_DEPTH),
            payload: frame0_pixels,
        },
        SyntheticPackEntry {
            asset_id: "text-window/palette/1",
            kind_tag: PALETTE_KIND_TAG,
            metadata: palette_metadata(PALETTE_COLOUR_COUNT),
            payload: frame0_palette,
        },
        SyntheticPackEntry {
            asset_id: "text-window/image/6",
            kind_tag: IMAGE_KIND_TAG,
            metadata: image_metadata(FRAME_SIDE, FRAME_SIDE, FRAME_BIT_DEPTH),
            payload: frame5_pixels,
        },
        SyntheticPackEntry {
            asset_id: "text-window/palette/6",
            kind_tag: PALETTE_KIND_TAG,
            metadata: palette_metadata(PALETTE_COLOUR_COUNT),
            payload: frame5_palette,
        },
        SyntheticPackEntry {
            asset_id: "font/normal/glyphs",
            kind_tag: IMAGE_KIND_TAG,
            metadata: image_metadata(
                assets::fonts::SHEET_WIDTH,
                assets::fonts::SHEET_HEIGHT,
                FONT_BIT_DEPTH,
            ),
            payload: font_pixels,
        },
        SyntheticPackEntry {
            asset_id: "interface/palette/main_menu_bg",
            kind_tag: PALETTE_KIND_TAG,
            metadata: palette_metadata(PALETTE_COLOUR_COUNT),
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
    load_synthetic_scene_with_font(TRANSPARENT_FONT_INDEX)
}

pub(super) fn load_synthetic_scene_of(menu_type: MainMenuType) -> MainMenuScene {
    load_synthetic_scene_inner(TRANSPARENT_FONT_INDEX, menu_type, super::FRAME_ID)
}

pub(super) fn load_synthetic_scene_of_with_window_frame(
    menu_type: MainMenuType,
    window_frame: u8,
) -> MainMenuScene {
    load_synthetic_scene_inner(TRANSPARENT_FONT_INDEX, menu_type, window_frame)
}

fn load_synthetic_scene_with_font(font_index: u8) -> MainMenuScene {
    load_synthetic_scene_inner(font_index, MainMenuType::NoSavedGame, super::FRAME_ID)
}

fn load_synthetic_scene_inner(
    font_index: u8,
    menu_type: MainMenuType,
    window_frame: u8,
) -> MainMenuScene {
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-main-menu-test-{}-{:?}-{font_index}-{window_frame}.pack",
        std::process::id(),
        std::thread::current().id()
    ));
    let temp_pack = TempPackGuard::new(path);
    std::fs::write(temp_pack.path(), synthetic_main_menu_pack_bytes(font_index)).unwrap();
    let pack = AssetPack::load(temp_pack.path()).unwrap();
    MainMenuScene::from_pack_with_window_frame(&pack, menu_type, window_frame).unwrap()
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
        std::fs::write(
            temp_pack.path(),
            synthetic_main_menu_pack_bytes(TRANSPARENT_FONT_INDEX),
        )
        .unwrap();
        assert!(temp_pack.path().exists());
        panic!("deliberate panic to exercise temporary pack cleanup");
    });

    assert!(result.is_err(), "the deliberate panic must be observed");
    assert!(!path.exists(), "temporary pack must be removed on unwind");
}

#[test]
fn compose_leaves_the_backdrop_bright_and_darkens_unselected_bg0_pixels() {
    let scene = load_synthetic_scene();
    let fb = scene.compose();

    let extracted_backdrop_color = Bgr555::from_channels(4, 4, 16).to_rgb888();
    let uncovered_backdrop = fb.pixel(5, 130);
    assert_eq!(uncovered_backdrop, Some(extracted_backdrop_color));
    assert_ne!(
        uncovered_backdrop,
        Some(rendering::darken(extracted_backdrop_color, DARKEN_WEIGHT)),
        "the backdrop is not a blend first target and must never darken"
    );

    let unselected_option_content = fb.pixel(18, 42);
    assert_eq!(
        unselected_option_content,
        Some(rendering::darken(HEADER_TEXT_BG, DARKEN_WEIGHT)),
        "BG0's own pixels outside WIN0 are the first target and must darken"
    );
}

#[test]
fn compose_keeps_the_selected_items_header_background_bright() {
    let scene = load_synthetic_scene();
    let fb = scene.compose();

    let selected_new_game_content = fb.pixel(18, 10);
    assert_eq!(selected_new_game_content, Some(HEADER_TEXT_BG));
}

#[test]
fn compose_from_synthetic_pack_darkens_the_unselected_items_content() {
    let scene = load_synthetic_scene();
    let fb = scene.compose();

    let unselected_option_content = fb.pixel(18, 42);
    let darkened_header_background = rendering::darken(HEADER_TEXT_BG, DARKEN_WEIGHT);
    assert_eq!(unselected_option_content, Some(darkened_header_background));
}

#[test]
fn compose_from_synthetic_pack_draws_the_border_from_the_extracted_frame_palette() {
    let scene = load_synthetic_scene();
    let fb = scene.compose();

    let extracted_frame_color = Bgr555::from_channels(0, 31, 0).to_rgb888();
    let selected_new_game_border = fb.pixel(10, 2);
    assert_eq!(selected_new_game_border, Some(extracted_frame_color));

    let unselected_option_border = fb.pixel(10, 34);
    assert_eq!(
        unselected_option_border,
        Some(rendering::darken(extracted_frame_color, DARKEN_WEIGHT))
    );
}

#[test]
fn compose_with_opaque_font_darkens_the_unselected_items_label_glyphs() {
    let scene = load_synthetic_scene_with_font(OPAQUE_FOREGROUND_FONT_INDEX);
    let fb = scene.compose();

    let selected_new_game_label = fb.pixel(17, 11);
    assert_eq!(selected_new_game_label, Some(HEADER_TEXT_FG));

    let unselected_option_label = fb.pixel(17, 43);
    assert_eq!(
        unselected_option_label,
        Some(rendering::darken(HEADER_TEXT_FG, DARKEN_WEIGHT)),
        "an unselected item's label glyphs must darken with its window"
    );
}

#[test]
fn compose_with_opaque_font_keeps_the_1px_text_origin_offset_and_clips_to_the_content_rect() {
    let scene = load_synthetic_scene_with_font(OPAQUE_FOREGROUND_FONT_INDEX);
    let fb = scene.compose();

    let new_game_content_above_text = fb.pixel(17, 8);
    assert_eq!(
        new_game_content_above_text,
        Some(HEADER_TEXT_BG),
        "the content rect's own top row is above the y=1 text origin"
    );
    let new_game_first_glyph_row = fb.pixel(17, 9);
    assert_eq!(
        new_game_first_glyph_row,
        Some(HEADER_TEXT_FG),
        "the first glyph row starts exactly at the y=1 text origin"
    );

    let new_game_last_content_row = fb.pixel(17, 23);
    assert_eq!(
        new_game_last_content_row,
        Some(HEADER_TEXT_FG),
        "the last content row is still glyph-reachable"
    );
    let extracted_frame_color = Bgr555::from_channels(0, 31, 0).to_rgb888();
    let new_game_border_below_content = fb.pixel(17, 24);
    assert_eq!(
        new_game_border_below_content,
        Some(extracted_frame_color),
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

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn checkout_pack_composes_non_blank_deterministic_frames_for_both_selection_states() {
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
