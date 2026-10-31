//! Battle-interface locator tests over the whole inventory: the located set,
//! report lines, upstream symbols and map cross-check, and the pack
//! validation refusals; the fixture they extend is in the sibling `tests`
//! module.

use std::fmt::Write as _;
use std::path::PathBuf;

use pack_format::{image_entry_from_tiles, palette_entry, raw_entry, PackEntry};
use rom_import::Encoding;

use super::frame_tests::{
    BALL_DISPLAY_ID, BALL_STATUS_ID, OPPONENT, OPPONENT_ID, PLAYER, PLAYER_ID, SHADOW_ID,
};
use super::locate;
use super::tests::{
    fixture, PP_TEXT_ID, TEXTBOX_ID, TEXTBOX_PALETTE_ID, TILEMAP_ID, WINDOW_TEXT_ID,
};
use crate::extract::scope::{
    battle_interface_ids, healthbox_element_ids, healthbox_frame_ids, healthbox_table_ids,
};
use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::plan::{ReportLine, Resolution, SymbolExpectation};
use crate::gen_rom_profile::tests::with_context;

fn without(mut entries: Vec<PackEntry>, id: &str) -> Vec<PackEntry> {
    entries.retain(|entry| entry.id != id);
    entries
}

#[test]
fn the_health_box_and_frame_records_are_located_with_the_textbox() {
    let (rom, entries) = fixture();
    with_context("battle-interface-health-box", &rom, entries, |ctx| {
        let mut report = Vec::new();
        let plan = locate(ctx, &mut report).expect("located");
        let mut located: Vec<&str> = plan
            .images
            .iter()
            .map(|i| i.id.as_str())
            .chain(plan.palettes.iter().map(|p| p.id.as_str()))
            .chain(plan.tilemaps.iter().map(|t| t.id.as_str()))
            .chain(plan.healthbox_elements.iter().map(|t| t.id.as_str()))
            .chain([plan.battle_bar.id.as_str()])
            .collect();
        located.sort_unstable();
        let mut expected = battle_interface_ids();
        expected.extend(healthbox_element_ids());
        expected.extend(healthbox_frame_ids());
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
            [
                (SHADOW_ID, 0x081A_0000, Encoding::Lz77, (4, 4), (32, 8, 4)),
                (
                    OPPONENT_ID,
                    0x0819_0000,
                    Encoding::Lz77,
                    (4, 4),
                    (128, 32, 64)
                ),
                (
                    PLAYER_ID,
                    0x0818_0000,
                    Encoding::Lz77,
                    (4, 4),
                    (128, 64, 128)
                ),
                (
                    TEXTBOX_ID,
                    0x0810_0000,
                    Encoding::Lz77,
                    (4, 4),
                    (128, 128, 256)
                ),
            ]
        );
        let palettes: Vec<_> = plan
            .palettes
            .iter()
            .map(|p| (p.id.as_str(), p.addr, p.encoding, p.color_count))
            .collect();
        assert_eq!(
            palettes,
            [
                (BALL_DISPLAY_ID, 0x081C_0000, Encoding::Raw, 16),
                (BALL_STATUS_ID, 0x081B_0000, Encoding::Raw, 16),
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
        assert_eq!(report.len(), 24);
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
        let contains = |name: &str| SymbolExpectation::Contains(vec![name.to_owned()]);
        assert_eq!(
            expected,
            [
                (SHADOW_ID, contains("gEnemyMonShadow_Gfx")),
                (OPPONENT_ID, contains(OPPONENT.symbol)),
                (PLAYER_ID, contains(PLAYER.symbol)),
                (TEXTBOX_ID, contains("gBattleTextboxTiles")),
                (BALL_DISPLAY_ID, exact("gBattleInterface_BallDisplayPal")),
                (BALL_STATUS_ID, exact("gBattleInterface_BallStatusBarPal")),
                (PP_TEXT_ID, exact("gPPTextPalette")),
                (TEXTBOX_PALETTE_ID, exact("gBattleTextboxPalette")),
                (WINDOW_TEXT_ID, exact("gBattleWindowTextPalette")),
                (TILEMAP_ID, exact("gBattleTextboxTilemap")),
            ]
            .into_iter()
            .chain(table_symbols())
            .chain([(
                "battle-interface/raw/battle_bar",
                exact("gBattleInterfaceGfx_BattleBar")
            )])
            .collect::<Vec<_>>()
        );
    });
}

/// The table's rows: the first names the table, the rest are inside it.
fn table_symbols() -> Vec<(&'static str, SymbolExpectation)> {
    let ids: Vec<&'static str> = healthbox_table_ids()
        .into_iter()
        .map(|id| &*Box::leak(id.into_boxed_str()))
        .collect();
    ids.into_iter()
        .enumerate()
        .map(|(index, id)| {
            let symbol = if index == 0 {
                SymbolExpectation::Exact("gHealthboxElementsGfxTable".to_owned())
            } else {
                SymbolExpectation::Interior
            };
            (id, symbol)
        })
        .collect()
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
fn a_map_naming_every_symbol_confirms_them_and_a_missing_one_names_its_id() {
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
        ("gHealthboxElementsGfxTable", 0x0816_0000),
        ("gBattleInterfaceGfx_BattleBar", 0x0817_0000),
        ("gHealthboxSinglesPlayerGfx", 0x0818_0000),
        ("gHealthboxSinglesOpponentGfx", 0x0819_0000),
        ("gEnemyMonShadow_Gfx", 0x081A_0000),
        ("gBattleInterface_BallStatusBarPal", 0x081B_0000),
        ("gBattleInterface_BallDisplayPal", 0x081C_0000),
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
    for id in battle_interface_ids()
        .into_iter()
        .chain(healthbox_element_ids())
        .chain(healthbox_frame_ids())
    {
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
        (
            "battle-interface/raw/healthbox_element/hpbar",
            palette_entry(
                "battle-interface/raw/healthbox_element/hpbar".into(),
                &[0; 16],
            )
            .expect("palette"),
        ),
        (
            "battle-interface/raw/battle_bar",
            palette_entry("battle-interface/raw/battle_bar".into(), &[0; 16]).expect("palette"),
        ),
        (
            PLAYER_ID,
            palette_entry(PLAYER_ID.into(), &[0; 16]).expect("palette"),
        ),
        (SHADOW_ID, raw_entry(SHADOW_ID.into(), vec![0; 32])),
        (
            BALL_STATUS_ID,
            raw_entry(BALL_STATUS_ID.into(), vec![0; 32]),
        ),
        (
            BALL_DISPLAY_ID,
            image_entry_from_tiles(BALL_DISPLAY_ID.into(), &[0; 32], 4, 8, 8, None).expect("image"),
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
