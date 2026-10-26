//! A synthetic pack with every entry the bag screen reads, and scratch-file
//! loading for it. Colours are chosen so a single pixel names what drew it.

use assets::AssetPack;
use pack_format::PackEntry;
use rendering::{Bgr555, Rgb888};

use crate::pack_test_support::{image_entry, pack_bytes, palette_entry_with_colors};

const BIT_DEPTH: u8 = 4;
const GLYPH_DEPTH: u8 = 2;
const SHEET_TILES_ACROSS: usize = 16;
const SCREEN_WIDTH: u32 = 128;
const SCREEN_HEIGHT: u32 = 32;
const TILEMAP_BYTES: usize = 2048;
const BAG_SIDE: u32 = 64;
const BAG_FRAMES: u32 = 6;
/// The local row of frame 0 that [`CLOSED_BAG`] fills.
pub(super) const CLOSED_BAG_ROW: usize = 10;
const CLOSED_MARKER_INDEX: u8 = 6;
const BALL_SIDE: u32 = 16;
const BALL_A_AT: (usize, usize) = (12, 8);
const BALL_B_AT: (usize, usize) = (8, 2);

/// Colour the message-box palette gives text and window content.
pub(super) const TEXT: Rgb888 = rgb(0, 0, 31);
pub(super) const WINDOW_FILL: Rgb888 = rgb(31, 31, 31);
pub(super) const WINDOW_BORDER: Rgb888 = rgb(0, 31, 0);
/// Screen palette bank 1 colours drawn by the two pocket-indicator tiles.
pub(super) const INDICATOR_IDLE: Rgb888 = rgb(10, 0, 0);
pub(super) const INDICATOR_CURRENT: Rgb888 = rgb(20, 0, 0);
/// The tile-0 colour, drawn by the tilemap's all-zero entries.
/// Bag-window text: screen palette bank 1, index 1 (also the idle indicator).
pub(super) const BAG_TEXT: Rgb888 = INDICATOR_IDLE;
/// Screen palette bank 1 index 4, drawn only by the HM mark in the list.
pub(super) const HM_MARK: Rgb888 = rgb(5, 5, 5);
/// Screen palette bank 1 index 3, the shadow of list and description text.
pub(super) const NORMAL_SHADOW: Rgb888 = rgb(0, 20, 0);
/// Screen palette bank 1 index 3 is also the gray cursor's text colour, and
/// index 6 is its shadow (`COLORID_GRAY_CURSOR`, `item_menu.c:388`).
pub(super) const GRAY_CURSOR_TEXT: Rgb888 = NORMAL_SHADOW;
pub(super) const GRAY_CURSOR_SHADOW: Rgb888 = rgb(0, 0, 20);
/// The closed bag's marker row, the only thing frame 0 of the sheet draws.
pub(super) const CLOSED_BAG: Rgb888 = CLOSED_BAG_555.to_rgb888();
const CLOSED_BAG_555: Bgr555 = bgr(15, 15, 0);
/// The rotating ball's two markers: A is 4 right of its centre, B 6 above it.
pub(super) const BALL_A: Rgb888 = BALL_A_555.to_rgb888();
pub(super) const BALL_B: Rgb888 = BALL_B_555.to_rgb888();
const BALL_A_555: Bgr555 = bgr(31, 0, 31);
const BALL_B_555: Bgr555 = bgr(0, 31, 31);
/// The list-wipe tiles (11 and 17 of the screen sheet, bank 0): the dark one
/// that covers the list and the one that restores it a row at a time.
pub(super) const WIPE_COVER: Rgb888 = WIPE_COVER_555.to_rgb888();
pub(super) const WIPE_RESTORED: Rgb888 = WIPE_RESTORED_555.to_rgb888();
const WIPE_COVER_555: Bgr555 = bgr(12, 0, 12);
const WIPE_RESTORED_555: Bgr555 = bgr(0, 12, 12);
pub(super) const BACKGROUND: Rgb888 = rgb(0, 10, 10);

const fn rgb(r: u8, g: u8, b: u8) -> Rgb888 {
    bgr(r, g, b).to_rgb888()
}

const fn bgr(r: u8, g: u8, b: u8) -> Bgr555 {
    Bgr555::from_channels(r, g, b)
}

/// The icon colour of an item icon palette stem.
pub(super) const fn icon_color(stem: &str) -> Rgb888 {
    match stem.as_bytes() {
        b"potion" => rgb(31, 0, 0),
        b"antidote" => rgb(0, 31, 0),
        b"poke_ball" => rgb(0, 0, 31),
        b"return_to_field_arrow" => rgb(31, 31, 0),
        _ => rgb(1, 1, 1),
    }
}

/// The colour of bag picture frame `frame` (1-5 per pocket; 0 is clear).
pub(super) fn bag_frame_color(frame: u8) -> Rgb888 {
    rgb(frame * 4, 5, 5)
}

const ICON_STEMS: [&str; 10] = [
    "potion",
    "great_ball",
    "antidote",
    "poke_ball",
    "return_to_field_arrow",
    "cheri_berry",
    "mach_bike",
    "fighting_tm_hm",
    "normal_tm_hm",
    "pokeblock_case",
];
const ICON_IMAGES: [&str; 10] = [
    "potion",
    "great_ball",
    "antidote",
    "poke_ball",
    "return_to_field_arrow",
    "cheri_berry",
    "mach_bike",
    "tm",
    "hm",
    "pokeblock_case",
];

fn leaked(id: String) -> &'static str {
    Box::leak(id.into_boxed_str())
}

/// Every glyph is solid text (index 1) with its odd columns shadow (index 2).
fn glyph_sheet(font: &str) -> PackEntry {
    let (width, height) = (assets::fonts::SHEET_WIDTH, assets::fonts::SHEET_HEIGHT);
    let pixels = (0..width * height)
        .map(|i| if i % 2 == 1 { 2 } else { 1 })
        .collect();
    pack_format::image_entry(
        format!("font/{font}/glyphs"),
        width,
        height,
        GLYPH_DEPTH,
        pixels,
    )
    .unwrap()
}

/// A sheet whose only opaque pixels are the text-coloured left column of each
/// glyph in `marked`, so a drawn glyph shows as one pixel column at the x it
/// was placed.
fn marked_glyph_sheet(font: &str, marked: &[usize]) -> PackEntry {
    let (width, height) = (
        assets::fonts::SHEET_WIDTH as usize,
        assets::fonts::SHEET_HEIGHT as usize,
    );
    let cell = assets::fonts::GLYPH_SIZE as usize;
    let mut pixels = vec![0u8; width * height];
    for &glyph in marked {
        let (left, top) = (
            (glyph % SHEET_TILES_ACROSS) * cell,
            (glyph / SHEET_TILES_ACROSS) * cell,
        );
        for y in top..top + cell {
            pixels[y * width + left] = 1;
        }
    }
    pack_format::image_entry(
        format!("font/{font}/glyphs"),
        assets::fonts::SHEET_WIDTH,
        assets::fonts::SHEET_HEIGHT,
        GLYPH_DEPTH,
        pixels,
    )
    .unwrap()
}

fn window_entries(normal: PackEntry, narrow: PackEntry) -> Vec<PackEntry> {
    vec![
        image_entry("text-window/image/1", 24, 24, BIT_DEPTH, 1),
        palette_entry_with_colors("text-window/palette/1", 16, &[(1, WINDOW_BORDER_555)]),
        image_entry("text-window/image/message_box", 56, 16, BIT_DEPTH, 0),
        palette_entry_with_colors(
            "text-window/palette/message_box",
            16,
            &[(1, WINDOW_FILL_555), (2, TEXT_555), (3, TEXT_555)],
        ),
        normal,
        narrow,
    ]
}

const WINDOW_BORDER_555: Bgr555 = Bgr555::from_channels(0, 31, 0);
const WINDOW_FILL_555: Bgr555 = Bgr555::from_channels(31, 31, 31);
const TEXT_555: Bgr555 = Bgr555::from_channels(0, 0, 31);

fn screen_entries() -> Vec<PackEntry> {
    let mut sheet = vec![0u8; (SCREEN_WIDTH * SCREEN_HEIGHT) as usize];
    for (tile, index) in [(0x17usize, 1u8), (0x2B, 2)] {
        let (tile_x, tile_y) = (tile % SHEET_TILES_ACROSS, tile / SHEET_TILES_ACROSS);
        for y in 0..8 {
            for x in 0..8 {
                sheet[(tile_y * 8 + y) * SCREEN_WIDTH as usize + tile_x * 8 + x] = index;
            }
        }
    }
    for (tile, index) in [(11usize, 1u8), (17, 2)] {
        let (tile_x, tile_y) = (tile % SHEET_TILES_ACROSS, tile / SHEET_TILES_ACROSS);
        for y in 0..8 {
            for x in 0..8 {
                sheet[(tile_y * 8 + y) * SCREEN_WIDTH as usize + tile_x * 8 + x] = index;
            }
        }
    }
    // Tile 0 draws index 3 in bank 0, so the all-zero tilemap paints BACKGROUND.
    for y in 0..8 {
        for x in 0..8 {
            sheet[y * SCREEN_WIDTH as usize + x] = 3;
        }
    }
    let tilemap = vec![0u8; TILEMAP_BYTES];
    let palette = [
        (1u8, WIPE_COVER_555),
        (2, WIPE_RESTORED_555),
        (3, Bgr555::from_channels(0, 10, 10)),
        (17, Bgr555::from_channels(10, 0, 0)),
        (18, Bgr555::from_channels(20, 0, 0)),
        (19, Bgr555::from_channels(0, 20, 0)),
        (20, Bgr555::from_channels(5, 5, 5)),
        (22, Bgr555::from_channels(0, 0, 20)),
    ];
    vec![
        pack_format::image_entry(
            "bag/image/menu".into(),
            SCREEN_WIDTH,
            SCREEN_HEIGHT,
            BIT_DEPTH,
            sheet,
        )
        .unwrap(),
        palette_entry_with_colors("bag/palette/menu/male", 32, &palette),
        palette_entry_with_colors("bag/palette/menu/female", 32, &palette),
        pack_format::raw_entry("bag/raw/menu".into(), tilemap),
    ]
}

fn picture_entries() -> Vec<PackEntry> {
    let mut pixels = Vec::new();
    for frame in 0..BAG_FRAMES {
        pixels.extend(std::iter::repeat_n(
            u8::try_from(frame).unwrap(),
            (BAG_SIDE * BAG_SIDE) as usize,
        ));
    }
    // The closed bag draws only a marker row.
    let closed_row = CLOSED_BAG_ROW * BAG_SIDE as usize;
    pixels[..(BAG_SIDE * BAG_SIDE) as usize].fill(0);
    pixels[closed_row..closed_row + BAG_SIDE as usize].fill(CLOSED_MARKER_INDEX);
    let mut colors: Vec<(u8, Bgr555)> = (1u8..6)
        .map(|frame| (frame, Bgr555::from_channels(frame * 4, 5, 5)))
        .collect();
    colors.push((CLOSED_MARKER_INDEX, CLOSED_BAG_555));
    let mut ball = vec![0u8; (BALL_SIDE * BALL_SIDE) as usize];
    for ((x, y), index) in [(BALL_A_AT, 1u8), (BALL_B_AT, 2)] {
        ball[y * BALL_SIDE as usize + x] = index;
    }
    let picture = |id: &'static str| {
        pack_format::image_entry(
            id.into(),
            BAG_SIDE,
            BAG_SIDE * BAG_FRAMES,
            BIT_DEPTH,
            pixels.clone(),
        )
        .unwrap()
    };
    vec![
        picture("bag/image/male"),
        picture("bag/image/female"),
        palette_entry_with_colors("bag/palette/bag", 16, &colors),
        pack_format::image_entry(
            "bag/image/rotating_ball".into(),
            BALL_SIDE,
            BALL_SIDE,
            BIT_DEPTH,
            ball,
        )
        .unwrap(),
        palette_entry_with_colors(
            "bag/palette/rotating_ball",
            16,
            &[(1, BALL_A_555), (2, BALL_B_555)],
        ),
        image_entry("bag/image/select_button", 24, 16, BIT_DEPTH, 0),
        image_entry("bag/image/hm", 16, 16, BIT_DEPTH, 4),
    ]
}

fn icon_entries() -> Vec<PackEntry> {
    let mut entries = Vec::new();
    for image in ICON_IMAGES {
        entries.push(image_entry(
            leaked(format!("bag/item-icon/image/{image}")),
            24,
            24,
            BIT_DEPTH,
            1,
        ));
    }
    for stem in ICON_STEMS {
        let color = icon_color(stem);
        entries.push(palette_entry_with_colors(
            leaked(format!("bag/item-icon/palette/{stem}")),
            16,
            &[(
                1,
                Bgr555::from_channels(color.r >> 3, color.g >> 3, color.b >> 3),
            )],
        ));
    }
    entries
}

/// The pack with every entry the screen reads.
pub(super) fn entries() -> Vec<PackEntry> {
    entries_with_narrow(glyph_sheet("narrow"))
}

/// [`entries`], but the narrow font draws only the left pixel column of the
/// glyph ids in `marked`, and the normal font, which draws the list cursor,
/// draws nothing.
pub(super) fn entries_with_marked_narrow_glyphs(marked: &[usize]) -> Vec<PackEntry> {
    entries_with_fonts(
        marked_glyph_sheet("normal", &[]),
        marked_glyph_sheet("narrow", marked),
    )
}

/// [`entries`], but only the normal font draws, and only the left pixel column
/// of the glyph ids in `marked`.
pub(super) fn entries_with_marked_normal_glyphs(marked: &[usize]) -> Vec<PackEntry> {
    entries_with_fonts(
        marked_glyph_sheet("normal", marked),
        marked_glyph_sheet("narrow", &[]),
    )
}

fn entries_with_narrow(narrow: PackEntry) -> Vec<PackEntry> {
    entries_with_fonts(glyph_sheet("normal"), narrow)
}

fn entries_with_fonts(normal: PackEntry, narrow: PackEntry) -> Vec<PackEntry> {
    let mut all = window_entries(normal, narrow);
    all.extend(screen_entries());
    all.extend(picture_entries());
    all.extend(icon_entries());
    all
}

/// Loads `entries` through a real [`AssetPack`] read from a scratch file.
pub(super) fn load(label: &str, entries: Vec<PackEntry>) -> AssetPack {
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-bag-menu-{label}-{}-{:?}.pack",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::write(&path, pack_bytes(entries)).expect("the scratch directory is writable");
    let pack = AssetPack::load(&path);
    let _ = std::fs::remove_file(&path);
    pack.expect("the synthetic bag pack loads")
}

/// The synthetic pack, fully loaded.
pub(super) fn bag_pack(label: &str) -> AssetPack {
    load(label, entries())
}
