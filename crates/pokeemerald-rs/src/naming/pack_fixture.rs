//! A synthetic pack with every entry the naming screen reads. Each palette
//! bank gets its own colours and each art piece its own palette indices, so
//! one pixel names the art, bank, and tile that drew it.

use std::path::PathBuf;

use assets::AssetPack;
use pack_format::PackEntry;
use rendering::{Bgr555, Rgb888};

use crate::pack_test_support::{pack_bytes, write_scratch_exclusive};

const BIT_DEPTH: u8 = 4;
const GLYPH_DEPTH: u8 = 2;
const TILEMAP_ENTRIES: usize = 32 * 20;
const TILE: usize = 8;

/// The palette id of each naming-screen palette: a bank number for the BG
/// palettes, and distinct ids for the avatars.
pub(super) mod pal {
    pub const MENU: u8 = 0;
    pub const UPPER: u8 = 1;
    pub const LOWER: u8 = 2;
    pub const OTHERS: u8 = 3;
    pub const BUTTONS: u8 = 4;
    pub const CURSOR: u8 = 5;
    pub const BRENDAN: u8 = 8;
    pub const MAY: u8 = 9;
    pub const KEYBOARD: u8 = 10;
    pub const TEXT_WINDOW: u8 = 11;
}

/// Palette indices the art pieces fill themselves with.
pub(super) mod ink {
    /// Menu tile 1: the page marker.
    pub const MARKER: u8 = 5;
    /// Menu tile 2: the background fill.
    pub const BACKGROUND: u8 = 3;
    pub const FRAME: u8 = 6;
    pub const BACK: u8 = 7;
    pub const OK: u8 = 8;
    /// Label art for the upper, lower, and others pages.
    pub const LABELS: [u8; 3] = [9, 10, 11];
    /// The label's two undrawn tiles.
    pub const LABEL_UNDRAWN: u8 = 12;
    pub const BUTTON: u8 = 13;
    pub const UNDERSCORE: u8 = 13;
    pub const ARROW: u8 = 14;
    /// The four drawn tiles of the normal, squished, and filled cursor.
    pub const CURSORS: [[u8; 4]; 3] = [[1, 2, 3, 4], [5, 6, 7, 8], [9, 10, 11, 12]];
    /// The cursor strips' fifth tile, which no sprite reads.
    pub const CURSOR_UNDRAWN: u8 = 15;
    /// The avatar's resting frame (frame 3) and every other frame.
    pub const AVATAR: u8 = 3;
    pub const AVATAR_OTHER_FRAME: u8 = 5;
}

/// The colour palette id `id` gives palette index `index`.
pub(super) const fn pal_color(id: u8, index: u8) -> Rgb888 {
    pal_color_555(id, index).to_rgb888()
}

const fn pal_color_555(id: u8, index: u8) -> Bgr555 {
    Bgr555::from_channels(id * 2, index * 2, 1)
}

fn palette(id: &str, bank_id: u8) -> PackEntry {
    let colors: Vec<u16> = (0..16).map(|i| pal_color_555(bank_id, i).raw()).collect();
    pack_format::palette_entry(id.to_owned(), &colors).unwrap()
}

fn image(id: &str, width: u32, height: u32, pixels: Vec<u8>) -> PackEntry {
    pack_format::image_entry(id.to_owned(), width, height, BIT_DEPTH, pixels).unwrap()
}

fn solid(id: &str, width: u32, height: u32, ink: u8) -> PackEntry {
    image(id, width, height, vec![ink; (width * height) as usize])
}

/// An image of `width` x `height` whose 8x8 tile `n` (row-major) is solid
/// `tiles[n]`.
fn tiled(id: &str, width: u32, height: u32, tiles: &[u8]) -> PackEntry {
    let across = width as usize / TILE;
    let pixels = (0..height as usize)
        .flat_map(|y| (0..width as usize).map(move |x| (x, y)))
        .map(|(x, y)| {
            tiles
                .get(y / TILE * across + x / TILE)
                .copied()
                .unwrap_or(0)
        })
        .collect();
    image(id, width, height, pixels)
}

/// A 32x20 tilemap, every entry `fill`, with `(column, row, entry)`
/// replacements.
fn tilemap(id: &str, fill: u16, replacements: &[(usize, usize, u16)]) -> PackEntry {
    let mut entries = vec![fill; TILEMAP_ENTRIES];
    for &(column, row, entry) in replacements {
        entries[row * 32 + column] = entry;
    }
    let bytes = entries.iter().flat_map(|e| e.to_le_bytes()).collect();
    pack_format::raw_entry(id.to_owned(), bytes)
}

/// `tile` drawn from palette bank `bank`.
const fn map_entry(tile: u16, bank: u8) -> u16 {
    tile | ((bank as u16) << 12)
}

/// Where page `index`'s own marker tile sits: row 3, column `index`.
pub(super) const fn page_marker_cell(index: usize) -> (usize, usize) {
    (index, 3)
}

/// The tile row below the keyboard window both pages draw their frame marker
/// on (`(24, 144)` on screen).
pub(super) const PAGE_FRAME_CELL: (usize, usize) = (3, 18);

/// A font whose every glyph but the space marks its left pixel column with
/// the foreground and its second with the shadow.
fn marked_font(name: &str) -> PackEntry {
    let (width, height) = (
        assets::fonts::SHEET_WIDTH as usize,
        assets::fonts::SHEET_HEIGHT as usize,
    );
    let cell = assets::fonts::GLYPH_SIZE as usize;
    let mut pixels = vec![0u8; width * height];
    for glyph in 1..assets::fonts::GLYPH_COUNT {
        let (left, top) = ((glyph % 16) * cell, (glyph / 16) * cell);
        for y in top..top + cell {
            pixels[y * width + left] = 1;
            pixels[y * width + left + 1] = 2;
        }
    }
    pack_format::image_entry(
        format!("font/{name}/glyphs"),
        assets::fonts::SHEET_WIDTH,
        assets::fonts::SHEET_HEIGHT,
        GLYPH_DEPTH,
        pixels,
    )
    .unwrap()
}

fn walking(who: &str) -> PackEntry {
    let pixels = (0..32usize)
        .flat_map(|_| 0..144usize)
        .map(|x| {
            if (48..64).contains(&x) {
                ink::AVATAR
            } else {
                ink::AVATAR_OTHER_FRAME
            }
        })
        .collect();
    image(&format!("sprite/{who}/walking"), 144, 32, pixels)
}

/// Every entry the naming screen reads.
pub(super) fn entries() -> Vec<PackEntry> {
    let id = |stem: &str| format!("naming-screen/image/{stem}");
    let mut all = vec![
        // Tile 0 is transparent, 1 the page marker, 2 the background fill.
        tiled(&id("menu"), 24, 128, &[0, ink::MARKER, ink::BACKGROUND]),
        solid(&id("pc_icon_off"), 16, 24, 1),
        solid(&id("pc_icon_on"), 16, 24, 1),
        solid(&id("back_button"), 40, 24, ink::BACK),
        solid(&id("ok_button"), 40, 24, ink::OK),
        solid(&id("page_swap_button"), 32, 16, ink::BUTTON),
        solid(&id("input_arrow"), 8, 8, ink::ARROW),
        solid(&id("underscore"), 8, 8, ink::UNDERSCORE),
    ];
    // The frame has a hole where the label and button sit.
    let frame = (0..32usize)
        .flat_map(|y| (0..40usize).map(move |x| (x, y)))
        .map(|(x, y)| {
            if (4..36).contains(&x) && (3..19).contains(&y) {
                0
            } else {
                ink::FRAME
            }
        })
        .collect();
    all.push(image(&id("page_swap_frame"), 40, 32, frame));
    for (stem, ink) in ["page_swap_upper", "page_swap_lower", "page_swap_others"]
        .into_iter()
        .zip(ink::LABELS)
    {
        all.push(tiled(
            &id(stem),
            40,
            8,
            &[ink, ink, ink, ink::LABEL_UNDRAWN, ink::LABEL_UNDRAWN],
        ));
    }
    for (stem, tiles) in ["cursor", "cursor_squished", "cursor_filled"]
        .into_iter()
        .zip(ink::CURSORS)
    {
        let mut strip = tiles.to_vec();
        strip.push(ink::CURSOR_UNDRAWN);
        let pixels = (0..40usize)
            .flat_map(|y| std::iter::repeat_n(strip[y / TILE], 8))
            .collect();
        all.push(image(&id(stem), 8, 40, pixels));
    }
    let palettes = [
        ("menu", pal::MENU),
        ("page_swap_upper", pal::UPPER),
        ("page_swap_lower", pal::LOWER),
        ("page_swap_others", pal::OTHERS),
        ("buttons", pal::BUTTONS),
        ("cursor", pal::CURSOR),
        ("keyboard", pal::KEYBOARD),
    ];
    for (stem, bank_id) in palettes {
        all.push(palette(&format!("naming-screen/palette/{stem}"), bank_id));
    }
    // Background: the fill tile everywhere. Pages: transparent, with a marker
    // in each page's own cell and one both pages draw below the window.
    let raw = |stem: &str| format!("naming-screen/raw/{stem}");
    all.push(tilemap(&raw("background"), map_entry(2, pal::MENU), &[]));
    for (index, (stem, bank_id)) in [
        ("keyboard_upper", pal::UPPER),
        ("keyboard_lower", pal::LOWER),
        ("keyboard_symbols", pal::OTHERS),
    ]
    .into_iter()
    .enumerate()
    {
        let (column, row) = page_marker_cell(index);
        all.push(tilemap(
            &raw(stem),
            0,
            &[
                (column, row, map_entry(1, bank_id)),
                (PAGE_FRAME_CELL.0, PAGE_FRAME_CELL.1, map_entry(1, bank_id)),
            ],
        ));
    }
    all.push(walking("brendan"));
    all.push(walking("may"));
    all.push(palette("sprite/palette/brendan", pal::BRENDAN));
    all.push(palette("sprite/palette/may", pal::MAY));
    all.push(palette("text-window/palette/text_pal2", pal::TEXT_WINDOW));
    all.push(marked_font("normal"));
    all.push(marked_font("small"));
    all
}

/// Removes the scratch pack when dropped, even if the test panics.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Loads `entries` through a real [`AssetPack`] read from a scratch file.
pub(super) fn load(label: &str, entries: Vec<PackEntry>) -> AssetPack {
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-naming-{label}-{}-{:?}.pack",
        std::process::id(),
        std::thread::current().id()
    ));
    write_scratch_exclusive(&path, pack_bytes(entries));
    let _cleanup = Scratch(path.clone());
    AssetPack::load(&path).expect("the synthetic naming pack loads")
}

/// The synthetic pack, fully loaded.
pub(super) fn naming_pack(label: &str) -> AssetPack {
    load(label, entries())
}

/// [`entries`] without the entry whose id is `id`.
pub(super) fn entries_without(id: &str) -> Vec<PackEntry> {
    entries().into_iter().filter(|e| e.id != id).collect()
}
