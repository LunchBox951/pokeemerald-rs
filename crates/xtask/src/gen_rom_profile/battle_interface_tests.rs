//! Battle-interface locator tests over a synthetic ROM and pack.

use std::fmt::Write as _;
use std::path::PathBuf;

use pack_format::{image_entry_from_tiles, palette_entry, raw_entry, PackEntry};
use rom_import::fixture::RomFixture;
use rom_import::Encoding;

use super::locate;
use crate::extract::scope::{battle_interface_ids, healthbox_element_ids};
use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::plan::{ReportLine, Resolution, SymbolExpectation};
use crate::gen_rom_profile::tests::{distinctive_tiles, lz77_literals, with_context};

const SHEET_AT: usize = 0x10_0000;
const TEXTBOX_PALETTE_AT: usize = 0x11_0000;
const WINDOW_TEXT_AT: usize = 0x12_0000;
const PP_TEXT_AT: usize = 0x13_0000;
const TILEMAP_AT: usize = 0x14_0000;
/// Free ROM space for a second copy of a root.
const SPARE_AT: usize = 0x15_0000;

const TEXTBOX_ID: &str = "battle-interface/image/textbox";
const TEXTBOX_PALETTE_ID: &str = "battle-interface/palette/textbox";
const WINDOW_TEXT_ID: &str = "battle-interface/palette/window_text";
const PP_TEXT_ID: &str = "battle-interface/palette/pp_text";
const TILEMAP_ID: &str = "battle-interface/raw/textbox_map";

/// `count` BGR555 colours starting from `seed`, as values and ROM bytes.
fn palette_bytes(seed: u16, count: u16) -> (Vec<u16>, Vec<u8>) {
    let colors: Vec<u16> = (0..count).map(|n| (seed + n * 0x21) & 0x7FFF).collect();
    let bytes = colors.iter().flat_map(|c| c.to_le_bytes()).collect();
    (colors, bytes)
}

fn tilemap() -> Vec<u8> {
    (0..4096u32)
        .map(|n| u8::try_from(n * 13 % 251).expect("below 251"))
        .collect()
}

/// A ROM and pack holding all five roots, one distinct copy each.
fn fixture() -> (Vec<u8>, Vec<PackEntry>) {
    let tiles = distinctive_tiles(256);
    let (textbox_colors, textbox_bytes) = palette_bytes(0x1000, 32);
    let (window_colors, window_bytes) = palette_bytes(0x2000, 16);
    let (pp_colors, pp_bytes) = palette_bytes(0x3000, 16);
    let map = tilemap();
    let rom = RomFixture::new()
        .emerald_header()
        .write(SHEET_AT, &lz77_literals(&tiles))
        .write(TEXTBOX_PALETTE_AT, &lz77_literals(&textbox_bytes))
        .write(WINDOW_TEXT_AT, &lz77_literals(&window_bytes))
        .write(PP_TEXT_AT, &pp_bytes)
        .write(TILEMAP_AT, &lz77_literals(&map))
        .finish();
    let entries = vec![
        image_entry_from_tiles(TEXTBOX_ID.to_owned(), &tiles, 4, 128, 128, None).expect("sheet"),
        palette_entry(TEXTBOX_PALETTE_ID.to_owned(), &textbox_colors).expect("palette"),
        palette_entry(WINDOW_TEXT_ID.to_owned(), &window_colors).expect("palette"),
        palette_entry(PP_TEXT_ID.to_owned(), &pp_colors).expect("palette"),
        raw_entry(TILEMAP_ID.to_owned(), map),
    ];
    (rom, entries)
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

#[test]
fn the_fixed_inventory_is_one_sheet_three_palettes_and_one_tilemap() {
    assert_eq!(
        battle_interface_ids(),
        [
            TEXTBOX_ID,
            TEXTBOX_PALETTE_ID,
            WINDOW_TEXT_ID,
            PP_TEXT_ID,
            TILEMAP_ID
        ]
    );
}

#[test]
fn the_health_box_records_beside_the_textbox_are_left_unclaimed() {
    let (rom, mut entries) = fixture();
    let health_box = healthbox_element_ids();
    assert_eq!(health_box.len(), 14);
    entries.extend(health_box.into_iter().map(|id| raw_entry(id, vec![0; 32])));
    with_context("battle-interface-health-box", &rom, entries, |ctx| {
        let mut report = Vec::new();
        let plan = locate(ctx, &mut report).expect("located beside the health-box records");
        let mut located: Vec<&str> = plan
            .images
            .iter()
            .map(|i| i.id.as_str())
            .chain(plan.palettes.iter().map(|p| p.id.as_str()))
            .chain(plan.tilemaps.iter().map(|t| t.id.as_str()))
            .collect();
        located.sort_unstable();
        let mut expected = battle_interface_ids();
        expected.sort();
        assert_eq!(located, expected);
    });
}

#[test]
fn every_family_is_located_with_its_own_encoding() {
    let (rom, entries) = fixture();
    with_context("battle-interface-families", &rom, entries, |ctx| {
        let mut report = Vec::new();
        let plan = locate(ctx, &mut report).expect("located");
        let images: Vec<_> = plan
            .images
            .iter()
            .map(|i| {
                (
                    i.id.as_str(),
                    i.addr,
                    i.encoding,
                    (i.rom_bit_depth, i.pack_bit_depth),
                    (i.width, i.height, i.tile_count),
                )
            })
            .collect();
        assert_eq!(
            images,
            [(
                TEXTBOX_ID,
                0x0810_0000,
                Encoding::Lz77,
                (4, 4),
                (128, 128, 256)
            )]
        );
        let palettes: Vec<_> = plan
            .palettes
            .iter()
            .map(|p| (p.id.as_str(), p.addr, p.encoding, p.color_count))
            .collect();
        assert_eq!(
            palettes,
            [
                (PP_TEXT_ID, 0x0813_0000, Encoding::Raw, 16),
                (TEXTBOX_PALETTE_ID, 0x0811_0000, Encoding::Lz77, 32),
                (WINDOW_TEXT_ID, 0x0812_0000, Encoding::Lz77, 16),
            ]
        );
        let tilemaps: Vec<_> = plan
            .tilemaps
            .iter()
            .map(|t| (t.id.as_str(), t.addr, t.encoding, t.len))
            .collect();
        assert_eq!(tilemaps, [(TILEMAP_ID, 0x0814_0000, Encoding::Lz77, 4096)]);
        assert_eq!(report.len(), 5);
        assert!(report
            .iter()
            .all(|line| line.resolution == Resolution::UniqueSignature));
    });
}

#[test]
fn every_root_expects_its_global_upstream_symbol() {
    let (rom, entries) = fixture();
    with_context("battle-interface-symbols", &rom, entries, |ctx| {
        let mut report = Vec::new();
        locate(ctx, &mut report).expect("located");
        let expected: Vec<(&str, SymbolExpectation)> = report
            .iter()
            .map(|line| (line.id.as_str(), line.symbol.clone()))
            .collect();
        let exact = |name: &str| SymbolExpectation::Exact(name.to_owned());
        assert_eq!(
            expected,
            [
                (
                    TEXTBOX_ID,
                    SymbolExpectation::Contains(vec!["gBattleTextboxTiles".to_owned()])
                ),
                (PP_TEXT_ID, exact("gPPTextPalette")),
                (TEXTBOX_PALETTE_ID, exact("gBattleTextboxPalette")),
                (WINDOW_TEXT_ID, exact("gBattleWindowTextPalette")),
                (TILEMAP_ID, exact("gBattleTextboxTilemap")),
            ]
        );
    });
}

/// Write a GNU-ld-shaped map naming `symbols` at `lines`' addresses, then
/// cross-check `lines` against it.
fn cross_check_against_map(
    name: &str,
    symbols: &[(&str, u32)],
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
fn a_map_naming_all_five_symbols_confirms_them_and_a_missing_one_names_its_id() {
    let (rom, entries) = fixture();
    let report = with_context("battle-interface-map", &rom, entries, |ctx| {
        let mut report = Vec::new();
        locate(ctx, &mut report).expect("located");
        report
    });
    let symbols = [
        ("gBattleTextboxTiles", 0x0810_0000),
        ("gBattleTextboxPalette", 0x0811_0000),
        ("gBattleWindowTextPalette", 0x0812_0000),
        ("gPPTextPalette", 0x0813_0000),
        ("gBattleTextboxTilemap", 0x0814_0000),
    ];
    cross_check_against_map("battle-interface-all", &symbols, &mut report.clone())
        .expect("every symbol is named");

    for omitted in 0..symbols.len() {
        let mut kept = symbols.to_vec();
        kept.remove(omitted);
        let err = cross_check_against_map("battle-interface-one-short", &kept, &mut report.clone())
            .unwrap_err();
        let GenRomProfileError::MapMismatch { generated, .. } = err else {
            panic!("expected a map mismatch, got {err}");
        };
        assert_eq!(generated, symbols[omitted].1, "{omitted}");
    }

    let mut renamed = symbols;
    renamed[3].0 = "gPPTextPaletteX";
    let err = cross_check_against_map("battle-interface-renamed", &renamed, &mut report.clone())
        .unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::MapMismatch { id, .. } if id == PP_TEXT_ID),
        "{err}"
    );
}

#[test]
fn a_missing_entry_or_family_is_refused_before_any_report_line() {
    for id in battle_interface_ids() {
        let (rom, entries) = fixture();
        with_context(
            "battle-interface-missing",
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
    with_context("battle-interface-no-family", &rom, Vec::new(), |ctx| {
        let mut report = Vec::new();
        let err = locate(ctx, &mut report).unwrap_err();
        assert!(
            matches!(err, GenRomProfileError::MissingPackEntry(_)),
            "{err}"
        );
        assert!(report.is_empty());
    });
}

#[test]
fn a_surplus_entry_is_refused_before_any_report_line() {
    let (rom, mut entries) = fixture();
    entries.push(raw_entry("battle-interface/raw/extra".into(), vec![0]));
    with_context("battle-interface-surplus", &rom, entries, |ctx| {
        let mut report = Vec::new();
        let err = locate(ctx, &mut report).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == "battle-interface/raw/extra"),
            "{err}"
        );
        assert!(report.is_empty());
    });
}

#[test]
fn a_wrong_kind_entry_is_refused() {
    for (id, wrong) in [
        (
            TEXTBOX_PALETTE_ID,
            raw_entry(TEXTBOX_PALETTE_ID.into(), vec![0; 64]),
        ),
        (
            TILEMAP_ID,
            palette_entry(TILEMAP_ID.into(), &[0; 16]).expect("palette"),
        ),
        (
            TEXTBOX_ID,
            palette_entry(TEXTBOX_ID.into(), &[0; 16]).expect("palette"),
        ),
    ] {
        let (rom, entries) = fixture();
        let mut entries = without(entries, id);
        entries.push(wrong);
        with_context("battle-interface-wrong-kind", &rom, entries, |ctx| {
            let err = locate(ctx, &mut Vec::new()).unwrap_err();
            assert!(
                matches!(err, GenRomProfileError::WrongPackEntryKind { .. }),
                "{id}: {err}"
            );
        });
    }
}

#[test]
fn each_root_must_be_stored_in_its_own_encoding() {
    // Compressed palettes stored raw, the raw palette stored compressed, and
    // the tilemap stored raw each leave the root unfound.
    let (_, textbox_bytes) = palette_bytes(0x1000, 32);
    let (_, window_bytes) = palette_bytes(0x2000, 16);
    let (_, pp_bytes) = palette_bytes(0x3000, 16);
    let cases: [(&str, usize, Vec<u8>, Vec<u8>); 4] = [
        (
            TEXTBOX_PALETTE_ID,
            TEXTBOX_PALETTE_AT,
            textbox_bytes.clone(),
            lz77_literals(&textbox_bytes),
        ),
        (
            WINDOW_TEXT_ID,
            WINDOW_TEXT_AT,
            window_bytes.clone(),
            lz77_literals(&window_bytes),
        ),
        (
            PP_TEXT_ID,
            PP_TEXT_AT,
            lz77_literals(&pp_bytes),
            pp_bytes.clone(),
        ),
        (TILEMAP_ID, TILEMAP_AT, tilemap(), lz77_literals(&tilemap())),
    ];
    for (id, at, replacement, original) in cases {
        let (mut rom, entries) = fixture();
        rom[at..at + original.len()].fill(0);
        rom[at..at + replacement.len()].copy_from_slice(&replacement);
        with_context("battle-interface-encoding", &rom, entries, |ctx| {
            let err = locate(ctx, &mut Vec::new()).unwrap_err();
            assert!(
                matches!(&err, GenRomProfileError::NotFound { id: missing, .. } if missing == id),
                "{id}: {err}"
            );
        });
    }
}

#[test]
fn a_compressed_palette_or_tilemap_present_twice_is_ambiguous() {
    let (_, textbox_bytes) = palette_bytes(0x1000, 32);
    let (_, window_bytes) = palette_bytes(0x2000, 16);
    for (id, copy) in [
        (TEXTBOX_PALETTE_ID, lz77_literals(&textbox_bytes)),
        (WINDOW_TEXT_ID, lz77_literals(&window_bytes)),
        (TILEMAP_ID, lz77_literals(&tilemap())),
    ] {
        let (rom, entries) = fixture();
        let rom = with_spare_copy(rom, &copy);
        with_context("battle-interface-twice", &rom, entries, |ctx| {
            let err = locate(ctx, &mut Vec::new()).unwrap_err();
            assert!(
                matches!(&err, GenRomProfileError::Ambiguous { id: named, .. } if named == id),
                "{id}: {err}"
            );
        });
    }
}
