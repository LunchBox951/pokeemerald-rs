//! Battle-effect locator tests over a synthetic ROM, pack and checkout.
//!
//! Sprites 3 and 7 are the catalog's closure. They share one sheet, as three
//! upstream tags share the glowy orb, and the ROM holds an unreferenced second
//! copy of it, so only the registration tables can say which copy is theirs.
//! Sprite 7's palette likewise has a stray copy below the one its row names.
//! Each failure test then breaks one fact.

use pack_format::{image_entry, PackEntry};
use rom_import::fixture::RomFixture;

use super::fixture::{checkout_files, color_bytes, entries, pair, tiles, with_checkout};
use super::locate;
use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::plan::{BattleEffectPlan, ReportLine, Resolution, SymbolExpectation};
use crate::gen_rom_profile::tests::lz77_literals;

const SPRITES: [u16; 2] = [3, 7];
const SHEET_TABLE: usize = 0x1000;
const PALETTE_TABLE: usize = 0x2000;
const SHEET: usize = 0x3000;
const STRAY_SHEET: usize = 0x3800;
const STRAY_PALETTE: usize = 0x3F00;
const PALETTE_3: usize = 0x4000;
const PALETTE_7: usize = 0x4100;
/// Sprite 3 and 7's sheets are the same art: sprite 3's.
const SHEET_SALT: u8 = 3;

fn bus(offset: usize) -> u32 {
    0x0800_0000 + u32::try_from(offset).expect("a fixture offset fits a bus address")
}

/// The size the checkout registers for `sprite`: not the tile data's 64 bytes.
fn registered(sprite: u16) -> u16 {
    0x0100 + sprite * 0x10
}

fn tag(sprite: u16) -> u16 {
    10_000 + sprite
}

fn sheet_row(data: usize, size: u16, tag: u16) -> Vec<u8> {
    let mut row = bus(data).to_le_bytes().to_vec();
    row.extend_from_slice(&size.to_le_bytes());
    row.extend_from_slice(&tag.to_le_bytes());
    row
}

fn palette_row(data: usize, tag: u16) -> Vec<u8> {
    let mut row = bus(data).to_le_bytes().to_vec();
    row.extend_from_slice(&tag.to_le_bytes());
    row.extend_from_slice(&[0, 0]);
    row
}

fn rom() -> RomFixture {
    RomFixture::new()
        .emerald_header()
        .write(SHEET, &lz77_literals(&tiles(SHEET_SALT)))
        .write(STRAY_SHEET, &lz77_literals(&tiles(SHEET_SALT)))
        .write(STRAY_PALETTE, &lz77_literals(&color_bytes(7)))
        .write(PALETTE_3, &lz77_literals(&color_bytes(3)))
        .write(PALETTE_7, &lz77_literals(&color_bytes(7)))
        .write(
            SHEET_TABLE + 3 * 8,
            &sheet_row(SHEET, registered(3), tag(3)),
        )
        .write(
            SHEET_TABLE + 7 * 8,
            &sheet_row(SHEET, registered(7), tag(7)),
        )
        .write(PALETTE_TABLE + 3 * 8, &palette_row(PALETTE_3, tag(3)))
        .write(PALETTE_TABLE + 7 * 8, &palette_row(PALETTE_7, tag(7)))
}

/// The pack: the catalog, and a pair per sprite whose sheets share art.
fn pack() -> Vec<PackEntry> {
    let mut pack = entries(&SPRITES);
    pack.retain(|entry| entry.id != "battle-effect/007/image");
    pack.push(
        image_entry(
            "battle-effect/007/image".to_owned(),
            8,
            16,
            4,
            super::fixture::pixels(SHEET_SALT),
        )
        .expect("an image"),
    );
    pack
}

fn checkout() -> [(&'static str, String); 2] {
    checkout_files(8, registered)
}

type Located = Result<(BattleEffectPlan, Vec<ReportLine>), GenRomProfileError>;

fn locate_in(
    name: &str,
    rom: &RomFixture,
    pack: Vec<PackEntry>,
    files: &[(&str, String)],
) -> Located {
    with_checkout(name, rom, pack, files, |ctx| {
        let mut report = Vec::new();
        locate(ctx, &mut report).map(|plan| (plan, report))
    })
}

fn located(name: &str) -> (BattleEffectPlan, Vec<ReportLine>) {
    locate_in(name, &rom(), pack(), &checkout()).expect("a well-formed ROM locates")
}

fn refused(result: Located) -> GenRomProfileError {
    result.expect_err("the locator must refuse")
}

fn assert_table_mismatch(error: &GenRomProfileError, table: &str) {
    assert!(
        matches!(error, GenRomProfileError::StructMismatch { id, .. } if id == table),
        "{error:?}"
    );
}

#[test]
fn both_tables_and_every_record_are_located() {
    let (plan, _) = located("plan");
    assert_eq!(plan.sheet_table, bus(SHEET_TABLE));
    assert_eq!(plan.palette_table, bus(PALETTE_TABLE));
    let sprites: Vec<u16> = plan.records.iter().map(|record| record.sprite).collect();
    assert_eq!(sprites, SPRITES);
    for record in &plan.records {
        assert_eq!(record.sheet_bytes, registered(record.sprite));
        assert_eq!(
            record.sheet.id,
            format!("battle-effect/{:03}/image", record.sprite)
        );
        assert_eq!(record.sheet.addr, bus(SHEET));
        assert_eq!(
            (
                record.sheet.width,
                record.sheet.height,
                record.sheet.tile_count
            ),
            (8, 16, 2)
        );
        assert_eq!(
            (record.sheet.metatile_width, record.sheet.metatile_height),
            (1, 1)
        );
        assert_eq!(record.palette.color_count, 16);
    }
    assert_eq!(plan.records[0].palette.addr, bus(PALETTE_3));
    // The row names the copy above the stray one, so the address is not the lowest.
    assert_eq!(plan.records[1].palette.addr, bus(PALETTE_7));
}

#[test]
fn the_report_names_the_tables_and_the_symbols_their_rows_register() {
    let (_, report) = located("report");
    let line = |id: &str| {
        report
            .iter()
            .find(|line| line.id == id)
            .unwrap_or_else(|| panic!("no report line for {id}: {report:?}"))
    };
    assert_eq!(
        line("gBattleAnimPicTable").symbol,
        SymbolExpectation::Exact("gBattleAnimPicTable".to_owned())
    );
    assert_eq!(line("gBattleAnimPaletteTable").addr, bus(PALETTE_TABLE));
    assert_eq!(
        line("battle-effect/003/image").symbol,
        SymbolExpectation::Exact("gBattleAnimSpriteGfx_T3".to_owned())
    );
    let palette = line("battle-effect/007/palette");
    assert_eq!(
        palette.symbol,
        SymbolExpectation::Exact("gBattleAnimSpritePal_T7".to_owned())
    );
    assert_eq!(palette.resolution, Resolution::StructDerived);
    assert_eq!(report.len(), 2 + 2 * SPRITES.len());
}

#[test]
fn a_registered_size_the_rom_row_lacks_is_refused() {
    let rom = rom().write(
        SHEET_TABLE + 7 * 8,
        &sheet_row(SHEET, registered(7) + 1, tag(7)),
    );
    let error = refused(locate_in("size", &rom, pack(), &checkout()));
    assert_table_mismatch(&error, "gBattleAnimPicTable");
}

#[test]
fn a_tag_the_rom_row_lacks_is_refused_in_either_table() {
    let sheet = rom().write(
        SHEET_TABLE + 3 * 8,
        &sheet_row(SHEET, registered(3), tag(3) + 1),
    );
    let error = refused(locate_in("sheet-tag", &sheet, pack(), &checkout()));
    assert_table_mismatch(&error, "gBattleAnimPicTable");
    let palette = rom().write(PALETTE_TABLE + 7 * 8, &palette_row(PALETTE_7, tag(7) - 1));
    let error = refused(locate_in("palette-tag", &palette, pack(), &checkout()));
    assert_table_mismatch(&error, "gBattleAnimPaletteTable");
}

#[test]
fn a_row_that_names_other_art_is_refused() {
    let rom = rom().write(PALETTE_TABLE + 3 * 8, &palette_row(PALETTE_7, tag(3)));
    let error = refused(locate_in("other-art", &rom, pack(), &checkout()));
    assert_table_mismatch(&error, "gBattleAnimPaletteTable");
}

#[test]
fn a_table_that_stops_before_a_selected_row_is_refused() {
    // Row 7 of the sheet table is zeroed, as if the table ended at row 6.
    let rom = rom().write(SHEET_TABLE + 7 * 8, &[0; 8]);
    let error = refused(locate_in("short-table", &rom, pack(), &checkout()));
    assert_table_mismatch(&error, "gBattleAnimPicTable");
}

#[test]
fn two_tables_that_both_fit_are_refused_not_chosen_between() {
    let rom = rom()
        .write(
            PALETTE_TABLE + 0x800 + 3 * 8,
            &palette_row(PALETTE_3, tag(3)),
        )
        .write(
            PALETTE_TABLE + 0x800 + 7 * 8,
            &palette_row(PALETTE_7, tag(7)),
        );
    let error = refused(locate_in("two-tables", &rom, pack(), &checkout()));
    assert_table_mismatch(&error, "gBattleAnimPaletteTable");
}

#[test]
fn art_absent_from_the_rom_is_not_found() {
    let rom = RomFixture::new()
        .emerald_header()
        .write(SHEET, &lz77_literals(&tiles(SHEET_SALT)))
        .write(PALETTE_3, &lz77_literals(&color_bytes(3)));
    let error = refused(locate_in("absent", &rom, pack(), &checkout()));
    assert!(
        matches!(&error, GenRomProfileError::NotFound { id } if id == "battle-effect/007/palette"),
        "{error:?}"
    );
}

#[test]
fn a_checkout_row_at_another_index_or_tag_is_refused() {
    let [constants, tables] = checkout();
    let moved = tables.1.replace("ANIM_TAG_T3}", "ANIM_TAG_T4}");
    let error = refused(locate_in(
        "moved-row",
        &rom(),
        pack(),
        &[constants.clone(), (tables.0, moved)],
    ));
    assert!(
        matches!(&error, GenRomProfileError::UpstreamSource { reason, .. } if reason.contains("row 3")),
        "{error:?}"
    );
    let short = checkout_files(5, registered);
    let error = refused(locate_in("short-checkout", &rom(), pack(), &short));
    assert!(
        matches!(&error, GenRomProfileError::UpstreamSource { reason, .. } if reason.contains("no row 7")),
        "{error:?}"
    );
}

/// The pack refusals happen before any search: the ROM holds no art at all, so
/// reaching the search would be `NotFound`, and nothing is reported.
fn assert_refused_before_searching(name: &str, pack: Vec<PackEntry>) -> GenRomProfileError {
    let rom = RomFixture::new().emerald_header();
    with_checkout(name, &rom, pack, &checkout(), |ctx| {
        let mut report = Vec::new();
        let error = locate(ctx, &mut report).expect_err("the pack must be refused");
        assert!(report.is_empty(), "{report:?}");
        error
    })
}

fn without(id: &str) -> Vec<PackEntry> {
    let mut pack = pack();
    pack.retain(|entry| entry.id != id);
    pack
}

#[test]
fn a_missing_half_of_a_pair_is_refused() {
    for id in ["battle-effect/007/image", "battle-effect/003/palette"] {
        let error = assert_refused_before_searching("missing", without(id));
        assert!(
            matches!(&error, GenRomProfileError::MissingPackEntry(missing) if missing == id),
            "{error:?}"
        );
    }
}

#[test]
fn a_missing_family_is_refused() {
    let mut pack = pack();
    pack.retain(|entry| !entry.id.starts_with("battle-effect/"));
    let error = assert_refused_before_searching("no-family", pack);
    assert!(
        matches!(&error, GenRomProfileError::MissingPackEntry(id) if id == "battle-effect/003/palette"),
        "{error:?}"
    );
}

#[test]
fn a_catalog_that_loads_no_effect_sprite_is_refused() {
    let error = assert_refused_before_searching("empty", entries(&[]));
    assert!(
        matches!(&error, GenRomProfileError::MissingPackEntry(id) if id == "battle-effect/*"),
        "{error:?}"
    );
}

#[test]
fn a_surplus_effect_entry_is_refused() {
    let mut pack = pack();
    pack.extend(pair(200, 9));
    let error = assert_refused_before_searching("surplus", pack);
    assert!(
        matches!(&error, GenRomProfileError::StructMismatch { id, .. } if id == "battle-effect/200/image"),
        "{error:?}"
    );
}

#[test]
fn an_entry_of_the_wrong_kind_is_refused() {
    let mut pack = without("battle-effect/003/image");
    pack.push(pack_format::raw_entry(
        "battle-effect/003/image".to_owned(),
        vec![0],
    ));
    let error = assert_refused_before_searching("wrong-kind", pack);
    assert!(
        matches!(&error, GenRomProfileError::WrongPackEntryKind { id, .. } if id == "battle-effect/003/image"),
        "{error:?}"
    );
}

#[test]
fn a_malformed_sheet_or_palette_is_refused() {
    let replaced = |entry: PackEntry| {
        let mut pack = without(&entry.id);
        pack.push(entry);
        pack
    };
    let ragged =
        image_entry("battle-effect/003/image".to_owned(), 12, 8, 4, vec![0; 96]).expect("an image");
    let eight_bit =
        image_entry("battle-effect/003/image".to_owned(), 8, 8, 8, vec![0; 64]).expect("an image");
    let ten_colors = pack_format::palette_entry("battle-effect/003/palette".to_owned(), &[0; 10])
        .expect("a palette");
    for (name, entry) in [
        ("ragged", ragged),
        ("eight-bit", eight_bit),
        ("ten", ten_colors),
    ] {
        let id = entry.id.clone();
        let error = assert_refused_before_searching(name, replaced(entry));
        assert!(
            matches!(&error, GenRomProfileError::EntryShape { id: bad, .. } if *bad == id),
            "{name}: {error:?}"
        );
    }
}
