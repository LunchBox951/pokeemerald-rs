//! Naming-screen locator tests over a synthetic ROM and pack.

use std::fmt::Write as _;
use std::path::PathBuf;

use pack_format::{image_entry_from_tiles, palette_entry, raw_entry, PackEntry};
use rom_import::fixture::RomFixture;
use rom_import::Encoding;

use super::locate;
use crate::extract::scope::naming_screen_ids;
use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::plan::{ReportLine, Resolution, SymbolExpectation};
use crate::gen_rom_profile::tests::{distinctive_tiles, lz77_literals, with_context};

const IMAGE_AT: usize = 0x10_0000;
const IMAGE_SLOT: usize = 0x1000;
const MENU_TABLE_AT: usize = 0x20_0000;
const KEYBOARD_AT: usize = 0x21_0000;
const TILEMAP_AT: usize = 0x30_0000;
/// Free ROM space for a second copy of a root.
const SPARE_AT: usize = 0x40_0000;
const TILEMAP_LEN: usize = 1280;

/// `(stem, pack width, pack height, stored tiles, upstream symbol fragment)`
/// for every sheet. `None` is a `static` (`naming_screen.c:185-186`).
const SHEETS: [(&str, u32, u32, usize, Option<&str>); 15] = [
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
const PALETTES: [&str; 7] = [
    "menu",
    "page_swap_upper",
    "page_swap_lower",
    "page_swap_others",
    "buttons",
    "cursor",
    "keyboard",
];
const TILEMAPS: [(&str, &str); 4] = [
    ("background", "gNamingScreenBackground_Tilemap"),
    ("keyboard_upper", "gNamingScreenKeyboardUpper_Tilemap"),
    ("keyboard_lower", "gNamingScreenKeyboardLower_Tilemap"),
    ("keyboard_symbols", "gNamingScreenKeyboardSymbols_Tilemap"),
];

fn image_id(stem: &str) -> String {
    format!("naming-screen/image/{stem}")
}
fn palette_id(stem: &str) -> String {
    format!("naming-screen/palette/{stem}")
}
fn tilemap_id(stem: &str) -> String {
    format!("naming-screen/raw/{stem}")
}

/// Tiles unique to sheet `index`, so no two sheets share a run.
fn sheet_tiles(index: usize) -> Vec<u8> {
    let seed = u8::try_from(index + 1).expect("small");
    distinctive_tiles(SHEETS[index].3)
        .into_iter()
        .map(|byte| byte ^ seed)
        .collect()
}

/// A 16-colour bank as values and ROM bytes.
fn bank(seed: u16) -> (Vec<u16>, Vec<u8>) {
    let colors: Vec<u16> = (0..16).map(|n| (seed + n * 0x21) & 0x7FFF).collect();
    let bytes = colors.iter().flat_map(|c| c.to_le_bytes()).collect();
    (colors, bytes)
}

fn palette_seed(index: usize) -> u16 {
    0x0100 * u16::try_from(index + 1).expect("small")
}

fn tilemap(index: usize) -> Vec<u8> {
    (0..TILEMAP_LEN)
        .map(|n| u8::try_from((n * 13 + index * 29) % 251).expect("below 251"))
        .collect()
}

/// A ROM and pack holding all 26 roots; `seeds` picks each palette's colours.
fn fixture_with(seeds: [u16; 7]) -> (Vec<u8>, Vec<PackEntry>) {
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

fn default_seeds() -> [u16; 7] {
    std::array::from_fn(palette_seed)
}

fn fixture() -> (Vec<u8>, Vec<PackEntry>) {
    fixture_with(default_seeds())
}

fn without(mut entries: Vec<PackEntry>, id: &str) -> Vec<PackEntry> {
    entries.retain(|entry| entry.id != id);
    entries
}

/// Plant `bytes` over the spare area of `rom`.
fn with_spare_copy(mut rom: Vec<u8>, bytes: &[u8]) -> Vec<u8> {
    rom[SPARE_AT..SPARE_AT + bytes.len()].copy_from_slice(bytes);
    rom
}

fn table_bytes(seeds: [u16; 7]) -> Vec<u8> {
    seeds[..6].iter().flat_map(|&seed| bank(seed).1).collect()
}

fn addr(offset: usize) -> u32 {
    0x0800_0000 + u32::try_from(offset).expect("small")
}

#[test]
fn the_fixed_inventory_is_fifteen_sheets_seven_palettes_and_four_tilemaps() {
    let ids = naming_screen_ids();
    assert_eq!(ids.len(), 26);
    assert_eq!(
        ids.iter().filter(|id| id.contains("/image/")).count(),
        SHEETS.len()
    );
    assert_eq!(
        ids.iter().filter(|id| id.contains("/palette/")).count(),
        PALETTES.len()
    );
    assert_eq!(
        ids.iter().filter(|id| id.contains("/raw/")).count(),
        TILEMAPS.len()
    );
}

#[test]
fn every_root_is_located_at_its_planted_address_in_its_own_encoding() {
    let (rom, entries) = fixture();
    with_context("naming-screen-plan", &rom, entries, |ctx| {
        let mut report = Vec::new();
        let plan = locate(ctx, &mut report).expect("located");
        assert_eq!(plan.images.len(), 15);
        assert_eq!(plan.palettes.len(), 7);
        assert_eq!(plan.tilemaps.len(), 4);
        assert_eq!(report.len(), 26);

        for (index, &(stem, width, height, tiles, _)) in SHEETS.iter().enumerate() {
            let image = plan
                .images
                .iter()
                .find(|image| image.id == image_id(stem))
                .expect(stem);
            let encoding = if stem == "menu" {
                Encoding::Lz77
            } else {
                Encoding::Raw
            };
            assert_eq!(image.addr, addr(IMAGE_AT + index * IMAGE_SLOT), "{stem}");
            assert_eq!(image.encoding, encoding, "{stem}");
            assert_eq!(
                (image.rom_bit_depth, image.pack_bit_depth),
                (4, 4),
                "{stem}"
            );
            assert_eq!((image.width, image.height), (width, height), "{stem}");
            assert_eq!(usize::try_from(image.tile_count).unwrap(), tiles, "{stem}");
        }
        for (index, stem) in PALETTES.iter().enumerate() {
            let palette = plan
                .palettes
                .iter()
                .find(|palette| palette.id == palette_id(stem))
                .expect(stem);
            let at = if index < 6 {
                MENU_TABLE_AT + index * 32
            } else {
                KEYBOARD_AT
            };
            assert_eq!(palette.addr, addr(at), "{stem}");
            assert_eq!((palette.encoding, palette.color_count), (Encoding::Raw, 16));
        }
        for (index, (stem, _)) in TILEMAPS.iter().enumerate() {
            let map = plan
                .tilemaps
                .iter()
                .find(|map| map.id == tilemap_id(stem))
                .expect(stem);
            assert_eq!(map.addr, addr(TILEMAP_AT + index * 0x1000), "{stem}");
            assert_eq!((map.encoding, map.len), (Encoding::Lz77, 1280), "{stem}");
        }
        assert!(
            report
                .iter()
                .all(|line| line.resolution != Resolution::ArbitraryAmongIdentical),
            "no naming-screen root may be chosen among copies"
        );
    });
}

#[test]
fn a_cursor_is_the_five_stored_tiles_even_with_a_nonzero_sixth_beside_it() {
    let (mut rom, entries) = fixture();
    let cursor = 7;
    let after = IMAGE_AT + cursor * IMAGE_SLOT + 5 * 32;
    rom[after..after + 32].fill(0xFF);
    with_context("naming-screen-cursor", &rom, entries, |ctx| {
        let plan = locate(ctx, &mut Vec::new()).expect("located");
        for stem in ["cursor", "cursor_squished", "cursor_filled"] {
            let image = plan
                .images
                .iter()
                .find(|image| image.id == image_id(stem))
                .expect(stem);
            assert_eq!(
                (image.width, image.height, image.tile_count),
                (8, 40, 5),
                "{stem}"
            );
        }
    });
}

#[test]
fn the_six_menu_rows_are_the_array_the_keyboard_bank_and_the_statics_are_local() {
    let (rom, entries) = fixture();
    with_context("naming-screen-symbols", &rom, entries, |ctx| {
        let mut report = Vec::new();
        locate(ctx, &mut report).expect("located");
        let symbol = |id: String| {
            report
                .iter()
                .find(|line| line.id == id)
                .expect("reported")
                .symbol
                .clone()
        };
        assert_eq!(
            symbol(palette_id("menu")),
            SymbolExpectation::Exact("gNamingScreenMenu_Pal".to_owned())
        );
        for stem in &PALETTES[1..6] {
            assert_eq!(symbol(palette_id(stem)), SymbolExpectation::Interior);
        }
        assert_eq!(
            symbol(palette_id("keyboard")),
            SymbolExpectation::Local("sKeyboard_Pal".to_owned())
        );
        assert_eq!(
            symbol(image_id("pc_icon_off")),
            SymbolExpectation::Local("sPCIconOff_Gfx".to_owned())
        );
        assert_eq!(
            symbol(image_id("pc_icon_on")),
            SymbolExpectation::Local("sPCIconOn_Gfx".to_owned())
        );
        assert_eq!(
            symbol(image_id("ok_button")),
            SymbolExpectation::Contains(vec!["gNamingScreenOKButton_Gfx".to_owned()])
        );
        for (stem, name) in TILEMAPS {
            assert_eq!(
                symbol(tilemap_id(stem)),
                SymbolExpectation::Exact(name.to_owned())
            );
        }
        let count = |wanted: fn(&SymbolExpectation) -> bool| {
            report.iter().filter(|line| wanted(&line.symbol)).count()
        };
        // Thirteen global sheets, the menu array base, and four tilemaps.
        assert_eq!(
            count(|s| matches!(
                s,
                SymbolExpectation::Exact(_) | SymbolExpectation::Contains(_)
            )),
            18
        );
        assert_eq!(count(|s| matches!(s, SymbolExpectation::Local(_))), 3);
        assert_eq!(count(|s| matches!(s, SymbolExpectation::Interior)), 5);
    });
}

/// Write a GNU-ld-shaped map naming `symbols`, then cross-check `lines`.
fn cross_check_against_map(
    name: &str,
    symbols: &[(String, u32)],
    lines: &mut [ReportLine],
) -> Result<(), GenRomProfileError> {
    let path: PathBuf =
        std::env::temp_dir().join(format!("pokeemerald-rs-{name}-{}.map", std::process::id()));
    let mut map = String::new();
    for (symbol, addr) in symbols {
        let _ = writeln!(map, "{:15}0x{addr:016x}                {symbol}", "");
    }
    std::fs::write(&path, map).expect("write map");
    let result = crate::gen_rom_profile::cross_check(Some(&path), lines);
    let _ = std::fs::remove_file(&path);
    result.map(|_| ())
}

#[test]
fn a_map_of_the_global_symbols_confirms_the_family_and_each_omission_is_named() {
    let (rom, entries) = fixture();
    let report = with_context("naming-screen-map", &rom, entries, |ctx| {
        let mut report = Vec::new();
        locate(ctx, &mut report).expect("located");
        report
    });
    let mut symbols: Vec<(String, u32)> = Vec::new();
    for (index, &(_, _, _, _, name)) in SHEETS.iter().enumerate() {
        if let Some(name) = name {
            symbols.push((name.to_owned(), addr(IMAGE_AT + index * IMAGE_SLOT)));
        }
    }
    symbols.push(("gNamingScreenMenu_Pal".to_owned(), addr(MENU_TABLE_AT)));
    for (index, (_, name)) in TILEMAPS.iter().enumerate() {
        symbols.push(((*name).to_owned(), addr(TILEMAP_AT + index * 0x1000)));
    }
    assert_eq!(symbols.len(), 18);
    // The statics and interior rows are absent from a linker map and not asserted.
    cross_check_against_map("naming-screen-all", &symbols, &mut report.clone())
        .expect("every global symbol is named");

    for omitted in 0..symbols.len() {
        let mut kept = symbols.clone();
        let (_, at) = kept.remove(omitted);
        let err = cross_check_against_map("naming-screen-one-short", &kept, &mut report.clone())
            .unwrap_err();
        let GenRomProfileError::MapMismatch { generated, .. } = err else {
            panic!("expected a map mismatch, got {err}");
        };
        assert_eq!(generated, at, "{omitted}");
    }

    let mut misspelled = symbols;
    let ok = misspelled
        .iter_mut()
        .find(|(name, _)| name == "gNamingScreenOKButton_Gfx")
        .expect("planted");
    ok.0 = "gNamingScreenOkButton_Gfx".to_owned();
    let err =
        cross_check_against_map("naming-screen-ok", &misspelled, &mut report.clone()).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::MapMismatch { id, .. } if *id == image_id("ok_button")),
        "{err}"
    );
}

#[test]
fn a_bank_copied_below_the_table_does_not_move_a_menu_row() {
    let (rom, entries) = fixture();
    // A lone copy of row 4 at the spare area, after the table.
    let rom = with_spare_copy(rom, &bank(palette_seed(4)).1);
    with_context("naming-screen-decoy", &rom, entries, |ctx| {
        let plan = locate(ctx, &mut Vec::new()).expect("the table decides, not a lone bank");
        let buttons = plan
            .palettes
            .iter()
            .find(|palette| palette.id == palette_id("buttons"))
            .expect("located");
        assert_eq!(buttons.addr, addr(MENU_TABLE_AT + 4 * 32));
    });
}

#[test]
fn equal_rows_stay_ordinal_at_their_own_stride() {
    // `page_swap_upper` and `page_swap_lower` carrying the same colours.
    let mut seeds = default_seeds();
    seeds[2] = seeds[1];
    let (rom, entries) = fixture_with(seeds);
    with_context("naming-screen-equal-rows", &rom, entries, |ctx| {
        let mut report = Vec::new();
        let plan = locate(ctx, &mut report).expect("the array settles equal rows");
        let at = |stem: &str| {
            plan.palettes
                .iter()
                .find(|palette| palette.id == palette_id(stem))
                .expect(stem)
                .addr
        };
        assert_eq!(at("page_swap_upper"), addr(MENU_TABLE_AT + 32));
        assert_eq!(at("page_swap_lower"), addr(MENU_TABLE_AT + 64));
        assert!(report
            .iter()
            .filter(|line| line.id.starts_with("naming-screen/palette/"))
            .all(|line| line.resolution != Resolution::ArbitraryAmongIdentical));
    });
}

#[test]
fn a_menu_table_missing_a_row_or_present_twice_is_refused() {
    // Row 3 overwritten: no complete array remains.
    let (mut rom, entries) = fixture();
    let row = MENU_TABLE_AT + 3 * 32;
    rom[row..row + 32].fill(0x55);
    with_context("naming-screen-broken-table", &rom, entries, |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::NotFound { id } if *id == palette_id("menu")),
            "{err}"
        );
    });

    let (rom, entries) = fixture();
    let rom = with_spare_copy(rom, &table_bytes(default_seeds()));
    with_context("naming-screen-two-tables", &rom, entries, |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::Ambiguous { id, .. } if *id == palette_id("menu")),
            "{err}"
        );
    });
}

#[test]
fn a_sheet_or_the_keyboard_bank_present_twice_is_ambiguous_not_a_first_match() {
    let mut copies: Vec<(String, Vec<u8>)> = vec![
        (image_id("ok_button"), sheet_tiles(3)),
        (image_id("cursor"), sheet_tiles(7)),
        (image_id("pc_icon_on"), sheet_tiles(14)),
        (image_id("underscore"), sheet_tiles(12)),
        (image_id("menu"), lz77_literals(&sheet_tiles(0))),
        (palette_id("keyboard"), bank(palette_seed(6)).1),
    ];
    copies.push((tilemap_id("background"), lz77_literals(&tilemap(0))));
    for (id, copy) in copies {
        let (rom, entries) = fixture();
        let rom = with_spare_copy(rom, &copy);
        with_context("naming-screen-twice", &rom, entries, |ctx| {
            let err = locate(ctx, &mut Vec::new()).unwrap_err();
            assert!(
                matches!(&err, GenRomProfileError::Ambiguous { id: named, .. } if *named == id),
                "{id}: {err}"
            );
        });
    }
}

#[test]
fn each_root_must_be_stored_in_its_own_encoding() {
    // The menu sheet stored raw, a raw sheet stored compressed, and a tilemap
    // stored raw each leave the root unfound.
    let cases: [(String, usize, Vec<u8>, usize); 3] = [
        (
            image_id("menu"),
            IMAGE_AT,
            sheet_tiles(0),
            lz77_literals(&sheet_tiles(0)).len(),
        ),
        (
            image_id("input_arrow"),
            IMAGE_AT + 11 * IMAGE_SLOT,
            lz77_literals(&sheet_tiles(11)),
            sheet_tiles(11).len(),
        ),
        (
            tilemap_id("background"),
            TILEMAP_AT,
            tilemap(0),
            lz77_literals(&tilemap(0)).len(),
        ),
    ];
    for (id, at, replacement, original_len) in cases {
        let (mut rom, entries) = fixture();
        rom[at..at + original_len.max(replacement.len())].fill(0);
        rom[at..at + replacement.len()].copy_from_slice(&replacement);
        with_context("naming-screen-encoding", &rom, entries, |ctx| {
            let err = locate(ctx, &mut Vec::new()).unwrap_err();
            assert!(
                matches!(&err, GenRomProfileError::NotFound { id: missing } if *missing == id),
                "{id}: {err}"
            );
        });
    }
}

#[test]
fn a_missing_entry_or_family_is_refused_before_any_report_line() {
    for id in naming_screen_ids() {
        let (rom, entries) = fixture();
        with_context(
            "naming-screen-missing",
            &rom,
            without(entries, &id),
            |ctx| {
                let mut report = Vec::new();
                let err = locate(ctx, &mut report).unwrap_err();
                assert!(
                    matches!(&err, GenRomProfileError::MissingPackEntry(missing) if *missing == id),
                    "{err}"
                );
                assert!(report.is_empty());
            },
        );
    }
    let (rom, _) = fixture();
    with_context("naming-screen-no-family", &rom, Vec::new(), |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert!(
            matches!(err, GenRomProfileError::MissingPackEntry(_)),
            "{err}"
        );
    });
}

#[test]
fn a_surplus_entry_is_refused_before_any_report_line() {
    let (rom, mut entries) = fixture();
    entries.push(raw_entry("naming-screen/raw/extra".into(), vec![0]));
    with_context("naming-screen-surplus", &rom, entries, |ctx| {
        let mut report = Vec::new();
        let err = locate(ctx, &mut report).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == "naming-screen/raw/extra"),
            "{err}"
        );
        assert!(report.is_empty());
    });
}

#[test]
fn a_wrong_kind_entry_is_refused() {
    for (id, wrong) in [
        (
            image_id("menu"),
            palette_entry(image_id("menu"), &[0; 16]).expect("palette"),
        ),
        (
            palette_id("keyboard"),
            raw_entry(palette_id("keyboard"), vec![0; 32]),
        ),
        (
            tilemap_id("background"),
            palette_entry(tilemap_id("background"), &[0; 16]).expect("palette"),
        ),
    ] {
        let (rom, entries) = fixture();
        let mut entries = without(entries, &id);
        entries.push(wrong);
        with_context("naming-screen-wrong-kind", &rom, entries, |ctx| {
            let err = locate(ctx, &mut Vec::new()).unwrap_err();
            assert!(
                matches!(err, GenRomProfileError::WrongPackEntryKind { .. }),
                "{id}: {err}"
            );
        });
    }
}

#[test]
fn a_valid_entry_of_the_wrong_production_shape_is_refused_by_name() {
    let (_, cursor_bytes) = (0, sheet_tiles(7));
    let wrong_cursor = {
        // 16x24 holds six tiles: the whole-sheet cursor upstream does not store.
        let mut tiles = cursor_bytes;
        tiles.extend_from_slice(&[0; 32]);
        image_entry_from_tiles(image_id("cursor"), &tiles, 4, 16, 24, None).expect("entry")
    };
    let cases = [
        (image_id("cursor"), wrong_cursor),
        (
            palette_id("cursor"),
            palette_entry(palette_id("cursor"), &[0x0421; 8]).expect("palette"),
        ),
        (
            tilemap_id("background"),
            raw_entry(tilemap_id("background"), vec![0; 640]),
        ),
    ];
    for (id, wrong) in cases {
        let (rom, entries) = fixture();
        let mut entries = without(entries, &id);
        entries.push(wrong);
        with_context("naming-screen-wrong-shape", &rom, entries, |ctx| {
            let mut report = Vec::new();
            let err = locate(ctx, &mut report).unwrap_err();
            assert!(
                matches!(&err, GenRomProfileError::EntryShape { id: named, .. } if *named == id),
                "{id}: {err}"
            );
            assert!(report.is_empty());
        });
    }
}
