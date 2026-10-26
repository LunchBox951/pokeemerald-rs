//! Shared synthetic ROM and pack for the naming-screen locator tests.

use pack_format::{image_entry_from_tiles, palette_entry, raw_entry, PackEntry};
use rom_import::fixture::RomFixture;

use crate::gen_rom_profile::tests::{distinctive_tiles, lz77_literals};

pub(super) const IMAGE_AT: usize = 0x10_0000;
pub(super) const IMAGE_SLOT: usize = 0x1000;
pub(super) const MENU_TABLE_AT: usize = 0x20_0000;
pub(super) const KEYBOARD_AT: usize = 0x21_0000;
pub(super) const TILEMAP_AT: usize = 0x30_0000;
/// Free ROM space for a second copy of a root.
pub(super) const SPARE_AT: usize = 0x40_0000;
pub(super) const TILEMAP_LEN: usize = 1280;

/// `(stem, pack width, pack height, stored tiles, upstream symbol fragment)`
/// for every sheet. `None` is a `static` (`naming_screen.c:185-186`).
pub(super) const SHEETS: [(&str, u32, u32, usize, Option<&str>); 15] = [
    ("menu", 24, 128, 48, Some("gNamingScreenMenu_Gfx")),
    (
        "page_swap_frame",
        40,
        32,
        20,
        Some("gNamingScreenPageSwapFrame_Gfx"),
    ),
    (
        "back_button",
        40,
        24,
        15,
        Some("gNamingScreenBackButton_Gfx"),
    ),
    ("ok_button", 40, 24, 15, Some("gNamingScreenOKButton_Gfx")),
    (
        "page_swap_upper",
        40,
        8,
        5,
        Some("gNamingScreenPageSwapUpper_Gfx"),
    ),
    (
        "page_swap_lower",
        40,
        8,
        5,
        Some("gNamingScreenPageSwapLower_Gfx"),
    ),
    (
        "page_swap_others",
        40,
        8,
        5,
        Some("gNamingScreenPageSwapOthers_Gfx"),
    ),
    ("cursor", 8, 40, 5, Some("gNamingScreenCursor_Gfx")),
    (
        "cursor_squished",
        8,
        40,
        5,
        Some("gNamingScreenCursorSquished_Gfx"),
    ),
    (
        "cursor_filled",
        8,
        40,
        5,
        Some("gNamingScreenCursorFilled_Gfx"),
    ),
    (
        "page_swap_button",
        32,
        16,
        8,
        Some("gNamingScreenPageSwapButton_Gfx"),
    ),
    ("input_arrow", 8, 8, 1, Some("gNamingScreenInputArrow_Gfx")),
    ("underscore", 8, 8, 1, Some("gNamingScreenUnderscore_Gfx")),
    ("pc_icon_off", 16, 24, 6, None),
    ("pc_icon_on", 16, 24, 6, None),
];

/// The six `gNamingScreenMenu_Pal` rows in array order, then `sKeyboard_Pal`.
pub(super) const PALETTES: [&str; 7] = [
    "menu",
    "page_swap_upper",
    "page_swap_lower",
    "page_swap_others",
    "buttons",
    "cursor",
    "keyboard",
];
pub(super) const TILEMAPS: [(&str, &str); 4] = [
    ("background", "gNamingScreenBackground_Tilemap"),
    ("keyboard_upper", "gNamingScreenKeyboardUpper_Tilemap"),
    ("keyboard_lower", "gNamingScreenKeyboardLower_Tilemap"),
    ("keyboard_symbols", "gNamingScreenKeyboardSymbols_Tilemap"),
];

pub(super) fn image_id(stem: &str) -> String {
    format!("naming-screen/image/{stem}")
}
pub(super) fn palette_id(stem: &str) -> String {
    format!("naming-screen/palette/{stem}")
}
pub(super) fn tilemap_id(stem: &str) -> String {
    format!("naming-screen/raw/{stem}")
}

/// Tiles unique to sheet `index`, so no two sheets share a run.
pub(super) fn sheet_tiles(index: usize) -> Vec<u8> {
    let seed = u8::try_from(index + 1).expect("small");
    distinctive_tiles(SHEETS[index].3)
        .into_iter()
        .map(|byte| byte ^ seed)
        .collect()
}

/// A 16-colour bank as values and ROM bytes.
pub(super) fn bank(seed: u16) -> (Vec<u16>, Vec<u8>) {
    let colors: Vec<u16> = (0..16).map(|n| (seed + n * 0x21) & 0x7FFF).collect();
    let bytes = colors.iter().flat_map(|c| c.to_le_bytes()).collect();
    (colors, bytes)
}

pub(super) fn palette_seed(index: usize) -> u16 {
    0x0100 * u16::try_from(index + 1).expect("small")
}

pub(super) fn tilemap(index: usize) -> Vec<u8> {
    (0..TILEMAP_LEN)
        .map(|n| u8::try_from((n * 13 + index * 29) % 251).expect("below 251"))
        .collect()
}

/// A ROM and pack holding all 26 roots; `seeds` picks each palette's colours.
pub(super) fn fixture_with(seeds: [u16; 7]) -> (Vec<u8>, Vec<PackEntry>) {
    let mut rom = RomFixture::new().emerald_header();
    let mut entries = Vec::new();
    for (index, &(stem, width, height, _, _)) in SHEETS.iter().enumerate() {
        let tiles = sheet_tiles(index);
        let stored = if stem == "menu" {
            lz77_literals(&tiles)
        } else {
            tiles.clone()
        };
        rom = rom.write(IMAGE_AT + index * IMAGE_SLOT, &stored);
        entries.push(
            image_entry_from_tiles(image_id(stem), &tiles, 4, width, height, None).expect("sheet"),
        );
    }
    for (index, stem) in PALETTES.iter().enumerate() {
        let (colors, bytes) = bank(seeds[index]);
        let at = if index < 6 {
            MENU_TABLE_AT + index * 32
        } else {
            KEYBOARD_AT
        };
        rom = rom.write(at, &bytes);
        entries.push(palette_entry(palette_id(stem), &colors).expect("palette"));
    }
    for (index, (stem, _)) in TILEMAPS.iter().enumerate() {
        let map = tilemap(index);
        rom = rom.write(TILEMAP_AT + index * 0x1000, &lz77_literals(&map));
        entries.push(raw_entry(tilemap_id(stem), map));
    }
    (rom.finish(), entries)
}

pub(super) fn default_seeds() -> [u16; 7] {
    std::array::from_fn(palette_seed)
}

pub(super) fn fixture() -> (Vec<u8>, Vec<PackEntry>) {
    fixture_with(default_seeds())
}

pub(super) fn without(mut entries: Vec<PackEntry>, id: &str) -> Vec<PackEntry> {
    entries.retain(|entry| entry.id != id);
    entries
}

/// Plant `bytes` over the spare area of `rom`.
pub(super) fn with_spare_copy(mut rom: Vec<u8>, bytes: &[u8]) -> Vec<u8> {
    rom[SPARE_AT..SPARE_AT + bytes.len()].copy_from_slice(bytes);
    rom
}

pub(super) fn table_bytes(seeds: [u16; 7]) -> Vec<u8> {
    seeds[..6].iter().flat_map(|&seed| bank(seed).1).collect()
}

pub(super) fn addr(offset: usize) -> u32 {
    0x0800_0000 + u32::try_from(offset).expect("small")
}
