//! Naming-screen roots expect the right upstream symbol in a linker map.

use std::fmt::Write as _;
use std::path::PathBuf;

use super::super::locate;
use super::fixture::{
    addr, fixture, image_id, palette_id, tilemap_id, IMAGE_AT, IMAGE_SLOT, MENU_TABLE_AT, PALETTES,
    SHEETS, TILEMAPS, TILEMAP_AT,
};
use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::plan::{ReportLine, SymbolExpectation};
use crate::gen_rom_profile::tests::with_context;

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
