//! Locator tests over a synthetic ROM: both trainer pictures, both tables,
//! and every way the art or a table can fail to corroborate.

use pack_format::{image_entry_from_tiles, palette_entry, raw_entry, PackEntry};
use rom_import::fixture::RomFixture;
use rom_import::Encoding;

use super::{locate, PALETTE_TAG_FIELD, PICTURE_SIZE, PICTURE_SIZE_FIELD, PICTURE_TAG_FIELD};
use super::{RECORD_BYTES, TABLE_ROWS};
use crate::extract::scope::{trainer_front_ids, TRAINER_FRONT_PICS};
use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::inventory::require_trainer_front;
use crate::gen_rom_profile::plan::{ReportLine, Resolution, SymbolExpectation};
use crate::gen_rom_profile::tests::with_context;

const BUS: u32 = 0x0800_0000;
const FRONT_TABLE: u32 = 0x10_0000;
const PALETTE_TABLE: u32 = 0x11_0000;
/// Each picture's image and palette sit `0x4000` apart from here, `0x1000` per slot.
const ART: u32 = 0x20_0000;
/// A second table at another base, and an unreferenced palette copy.
const OTHER_TABLE: u32 = 0x12_0000;
const STRAY_PALETTE: u32 = 0x18_0000;

fn lz77_literals(data: &[u8]) -> Vec<u8> {
    let size = u32::try_from(data.len()).expect("small").to_le_bytes();
    let mut out = vec![0x10, size[0], size[1], size[2]];
    for chunk in data.chunks(8) {
        out.push(0);
        out.extend_from_slice(chunk);
    }
    out
}

fn image_at(index: usize) -> u32 {
    ART + u32::try_from(index).unwrap() * 0x4000
}

fn palette_at(index: usize) -> u32 {
    image_at(index) + 0x1000
}

/// 64 tiles that differ per picture.
fn tiles(seed: usize) -> Vec<u8> {
    (0..64 * 32)
        .map(|index| u8::try_from((index * 37 + seed * 11 + 5) % 251).unwrap())
        .collect()
}

fn colors(seed: usize) -> Vec<u16> {
    (0..16u16)
        .map(|index| index * 0x0421 + u16::try_from(seed).unwrap() * 0x0101 + 0x0123)
        .collect()
}

fn colour_bytes(seed: usize) -> Vec<u8> {
    colors(seed).iter().flat_map(|c| c.to_le_bytes()).collect()
}

fn entries() -> Vec<PackEntry> {
    let mut entries = Vec::new();
    for (index, pic) in TRAINER_FRONT_PICS.iter().enumerate() {
        entries.push(
            image_entry_from_tiles(pic.image_id.to_owned(), &tiles(index), 4, 64, 64, None)
                .expect("image"),
        );
        entries.push(palette_entry(pic.palette_id.to_owned(), &colors(index)).expect("palette"));
    }
    entries
}

fn picture_row(data: u32, tag: u16) -> Vec<u8> {
    let mut row = data.to_le_bytes().to_vec();
    row.extend_from_slice(&PICTURE_SIZE.to_le_bytes());
    row.extend_from_slice(&tag.to_le_bytes());
    row
}

fn palette_row(data: u32, tag: u16) -> Vec<u8> {
    let mut row = data.to_le_bytes().to_vec();
    row.extend_from_slice(&tag.to_le_bytes());
    row.extend_from_slice(&[0, 0]);
    row
}

fn row_offset(table: u32, index: usize) -> usize {
    let trainer_pic = u32::from(TRAINER_FRONT_PICS[index].trainer_pic.index());
    (table + trainer_pic * RECORD_BYTES) as usize
}

/// A ROM with both tables and all four streams at their canonical places.
fn base_fixture() -> RomFixture {
    let mut rom = RomFixture::new().emerald_header();
    for (index, pic) in TRAINER_FRONT_PICS.iter().enumerate() {
        let trainer_pic = u16::from(pic.trainer_pic.index());
        rom = rom
            .write(
                row_offset(FRONT_TABLE, index),
                &picture_row(BUS + image_at(index), trainer_pic),
            )
            .write(
                row_offset(PALETTE_TABLE, index),
                &palette_row(BUS + palette_at(index), trainer_pic),
            )
            .write(image_at(index) as usize, &lz77_literals(&tiles(index)))
            .write(
                palette_at(index) as usize,
                &lz77_literals(&colour_bytes(index)),
            );
    }
    rom
}

fn rom() -> Vec<u8> {
    base_fixture().finish()
}

fn located(
    label: &str,
    rom: &[u8],
    pack: Vec<PackEntry>,
) -> Result<Vec<ReportLine>, GenRomProfileError> {
    with_context(label, rom, pack, |ctx| {
        let mut report = Vec::new();
        locate(ctx, &mut report).map(|_| report)
    })
}

fn assert_struct_mismatch(result: Result<Vec<ReportLine>, GenRomProfileError>, table: &str) {
    let err = result.unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == table),
        "{err}"
    );
}

#[test]
fn both_pictures_and_both_tables_are_found_from_the_art() {
    let rom = rom();
    with_context("trainer-front", &rom, entries(), |ctx| {
        let mut report = Vec::new();
        let plan = locate(ctx, &mut report).expect("located");
        assert_eq!(plan.front_table, BUS + FRONT_TABLE);
        assert_eq!(plan.palette_table, BUS + PALETTE_TABLE);
        assert_eq!(plan.trainers.len(), 2);
        for (index, trainer) in plan.trainers.iter().enumerate() {
            let pic = TRAINER_FRONT_PICS[index];
            assert_eq!(trainer.trainer_pic, u16::from(pic.trainer_pic.index()));
            assert_eq!(trainer.front.id, pic.image_id);
            assert_eq!(trainer.front.addr, BUS + image_at(index));
            assert_eq!(trainer.front.encoding, Encoding::Lz77);
            assert_eq!(
                (
                    trainer.front.rom_bit_depth,
                    trainer.front.pack_bit_depth,
                    trainer.front.width,
                    trainer.front.height,
                    trainer.front.tile_count
                ),
                (4, 4, 64, 64, 64)
            );
            assert_eq!(trainer.palette.id, pic.palette_id);
            assert_eq!(trainer.palette.addr, BUS + palette_at(index));
            assert_eq!(trainer.palette.encoding, Encoding::Lz77);
            assert_eq!(trainer.palette.color_count, 16);
        }
        assert_eq!(plan.trainers[0].trainer_pic, 71);
        assert_eq!(plan.trainers[1].trainer_pic, 72);
        // Four payload roots and two tables.
        assert_eq!(report.len(), 6);
        let table = report
            .iter()
            .find(|line| line.id == "gTrainerFrontPicTable")
            .expect("a table line");
        assert_eq!(table.len, TABLE_ROWS * RECORD_BYTES);
        assert_eq!(table.resolution, Resolution::StructDerived);
    });
}

#[test]
fn a_linker_map_must_name_each_root_after_its_trainer() {
    let rom = rom();
    with_context("trainer-front-symbols", &rom, entries(), |ctx| {
        let mut report = Vec::new();
        locate(ctx, &mut report).expect("located");
        let symbol = |id: &str| {
            report
                .iter()
                .find(|line| line.id == id)
                .map(|line| line.symbol.clone())
        };
        let exact = |name: &str| Some(SymbolExpectation::Exact(name.to_owned()));
        assert_eq!(
            symbol("trainer-pic/brendan/front"),
            exact("gTrainerFrontPic_Brendan")
        );
        assert_eq!(
            symbol("trainer-pic/may/front"),
            exact("gTrainerFrontPic_May")
        );
        assert_eq!(
            symbol("trainer-pic/brendan/palette"),
            exact("gTrainerPalette_Brendan")
        );
        assert_eq!(
            symbol("trainer-pic/may/palette"),
            exact("gTrainerPalette_May")
        );
        assert_eq!(
            symbol("gTrainerFrontPicTable"),
            exact("gTrainerFrontPicTable")
        );
        assert_eq!(
            symbol("gTrainerFrontPicPaletteTable"),
            exact("gTrainerFrontPicPaletteTable")
        );
    });
}

#[test]
fn a_picture_row_with_the_wrong_size_or_tag_pins_no_front_table() {
    let may_row = row_offset(FRONT_TABLE, 1);
    for (label, field, value) in [
        ("size", PICTURE_SIZE_FIELD, 0x400_u16),
        ("tag", PICTURE_TAG_FIELD, 71),
    ] {
        let rom = base_fixture()
            .write(may_row + field as usize, &value.to_le_bytes())
            .finish();
        assert_struct_mismatch(
            located(&format!("trainer-front-bad-{label}"), &rom, entries()),
            "gTrainerFrontPicTable",
        );
    }
}

#[test]
fn a_palette_row_with_the_wrong_tag_pins_no_palette_table() {
    let rom = base_fixture()
        .write(
            row_offset(PALETTE_TABLE, 1) + PALETTE_TAG_FIELD as usize,
            &71_u16.to_le_bytes(),
        )
        .finish();
    assert_struct_mismatch(
        located("trainer-front-bad-palette-tag", &rom, entries()),
        "gTrainerFrontPicPaletteTable",
    );
}

#[test]
fn a_second_table_holding_the_same_rows_is_ambiguous_not_first_wins() {
    let mut fixture = base_fixture();
    for (index, pic) in TRAINER_FRONT_PICS.iter().enumerate() {
        fixture = fixture.write(
            row_offset(OTHER_TABLE, index),
            &picture_row(BUS + image_at(index), u16::from(pic.trainer_pic.index())),
        );
    }
    assert_struct_mismatch(
        located("trainer-front-two-tables", &fixture.finish(), entries()),
        "gTrainerFrontPicTable",
    );
}

#[test]
fn a_palette_stored_twice_takes_the_copy_its_table_row_names() {
    // The stray copies sit at lower addresses than the ones the table names.
    let mut fixture = base_fixture();
    for index in 0..TRAINER_FRONT_PICS.len() {
        fixture = fixture.write(
            STRAY_PALETTE as usize + index * 0x100,
            &lz77_literals(&colour_bytes(index)),
        );
    }
    let rom = fixture.finish();
    let report = located("trainer-front-stray", &rom, entries()).expect("located");
    for (index, pic) in TRAINER_FRONT_PICS.iter().enumerate() {
        let line = report
            .iter()
            .find(|line| line.id == pic.palette_id)
            .expect("a report line");
        assert_eq!(line.addr, BUS + palette_at(index));
        assert_eq!(line.resolution, Resolution::StructDerived);
    }
}

#[test]
fn art_the_rom_lacks_is_not_found() {
    for (label, id, addr) in [
        ("image", TRAINER_FRONT_PICS[0].image_id, image_at(0)),
        ("palette", TRAINER_FRONT_PICS[1].palette_id, palette_at(1)),
    ] {
        let mut bytes = rom();
        bytes[addr as usize..addr as usize + 4].fill(0xFF);
        let err =
            located(&format!("trainer-front-missing-{label}"), &bytes, entries()).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::NotFound { id: missing } if missing == id),
            "{err}"
        );
    }
}

#[test]
fn a_pack_without_a_trainer_entry_is_refused_before_any_report_line() {
    for id in trainer_front_ids() {
        let mut pack = entries();
        pack.retain(|entry| entry.id != id);
        let rom = rom();
        with_context("trainer-front-pack-gap", &rom, pack, |ctx| {
            let mut report = Vec::new();
            let err = locate(ctx, &mut report).unwrap_err();
            assert!(
                matches!(&err, GenRomProfileError::MissingPackEntry(missing) if *missing == id),
                "{err}"
            );
            assert!(report.is_empty(), "{report:?}");
        });
    }
}

#[test]
fn a_surplus_trainer_entry_is_refused_by_id() {
    let surplus = "trainer-pic/hiker/front";
    let mut pack = entries();
    pack.push(raw_entry(surplus.to_owned(), vec![0]));
    let rom = rom();
    with_context("trainer-front-surplus", &rom, pack, |ctx| {
        let err = require_trainer_front(ctx.pack).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == surplus),
            "{err}"
        );
        let mut report = Vec::new();
        let err = locate(ctx, &mut report).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == surplus),
            "{err}"
        );
        assert!(report.is_empty(), "{report:?}");
    });
}

#[test]
fn the_inventory_is_each_trainers_image_then_palette() {
    assert_eq!(
        trainer_front_ids(),
        [
            "trainer-pic/brendan/front",
            "trainer-pic/brendan/palette",
            "trainer-pic/may/front",
            "trainer-pic/may/palette",
        ]
    );
}
