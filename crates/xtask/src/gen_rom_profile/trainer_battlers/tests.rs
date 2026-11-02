//! Locator tests over a synthetic ROM: both trainers' battlers on top of the
//! portraits' plan, the two back tables with their own index space, and every
//! way a battler entry or a back table can fail to corroborate.

use pack_format::{image_entry_from_tiles, palette_entry, raw_entry, PackEntry};
use rom_import::fixture::RomFixture;
use rom_import::Encoding;

use super::{locate, BACK_PICTURE_SIZE, BACK_TABLE_ROWS};
use crate::extract::scope::{trainer_battler_ids, TRAINER_BATTLERS, TRAINER_FRONT_PICS};
use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::inventory::require_trainer_battlers;
use crate::gen_rom_profile::plan::{ReportLine, SymbolExpectation, TrainerBattlerPlan};
use crate::gen_rom_profile::tests::with_context;
use crate::gen_rom_profile::trainer_front;

const BUS: u32 = 0x0800_0000;
const FRONT_TABLE: u32 = 0x10_0000;
const FRONT_PALETTE_TABLE: u32 = 0x11_0000;
const BACK_TABLE: u32 = 0x12_0000;
const BACK_PALETTE_TABLE: u32 = 0x13_0000;
/// A second back table, frame descriptors, and an unreferenced palette copy.
const OTHER_BACK_TABLE: u32 = 0x14_0000;
const FRAME_ROWS: u32 = 0x15_0000;
const PALETTE_COPY: u32 = 0x18_0000;
/// Each trainer's front, palette and back sheet sit `0x4000` apart per slot.
const ART: u32 = 0x20_0000;

const FRONT_SIZE: u16 = 0x800;

fn lz77_literals(data: &[u8]) -> Vec<u8> {
    let size = u32::try_from(data.len()).expect("small").to_le_bytes();
    let mut out = vec![0x10, size[0], size[1], size[2]];
    for chunk in data.chunks(8) {
        out.push(0);
        out.extend_from_slice(chunk);
    }
    out
}

fn front_at(index: usize) -> u32 {
    ART + u32::try_from(index).unwrap() * 0x4000
}

fn palette_at(index: usize) -> u32 {
    front_at(index) + 0x1000
}

fn back_at(index: usize) -> u32 {
    front_at(index) + 0x2000
}

fn tiles(seed: usize, count: usize) -> Vec<u8> {
    (0..count * 32)
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

fn front_entry(id: &str, seed: usize) -> PackEntry {
    image_entry_from_tiles(id.to_owned(), &tiles(seed, 64), 4, 64, 64, None).expect("image")
}

/// The portraits' entries and the battlers' entries, for both trainers.
fn entries() -> Vec<PackEntry> {
    let mut entries = Vec::new();
    for (index, trainer) in TRAINER_BATTLERS.iter().enumerate() {
        entries.push(front_entry(trainer.pic.image_id, index));
        entries.push(
            palette_entry(trainer.pic.palette_id.to_owned(), &colors(index)).expect("palette"),
        );
        entries.push(front_entry(trainer.front_id, index));
        entries
            .push(palette_entry(trainer.palette_id.to_owned(), &colors(index)).expect("palette"));
        entries.push(
            image_entry_from_tiles(
                trainer.back_id.to_owned(),
                &tiles(index + 10, 256),
                4,
                64,
                256,
                None,
            )
            .expect("image"),
        );
    }
    entries
}

fn picture_row(data: u32, size: u16, tag: u16) -> Vec<u8> {
    let mut row = data.to_le_bytes().to_vec();
    row.extend_from_slice(&size.to_le_bytes());
    row.extend_from_slice(&tag.to_le_bytes());
    row
}

fn palette_row(data: u32, tag: u16) -> Vec<u8> {
    let mut row = data.to_le_bytes().to_vec();
    row.extend_from_slice(&tag.to_le_bytes());
    row.extend_from_slice(&[0, 0]);
    row
}

fn front_pic(index: usize) -> u16 {
    u16::from(TRAINER_BATTLERS[index].pic.trainer_pic.index())
}

fn back_pic(index: usize) -> u16 {
    TRAINER_BATTLERS[index]
        .pic
        .trainer_pic
        .battler_back_index()
        .unwrap()
}

fn row(table: u32, index: u16) -> usize {
    (table + u32::from(index) * 8) as usize
}

/// A ROM with the four tables and all six streams at their canonical places.
fn base_fixture() -> RomFixture {
    let mut rom = RomFixture::new().emerald_header();
    for index in 0..TRAINER_BATTLERS.len() {
        rom = rom
            .write(
                row(FRONT_TABLE, front_pic(index)),
                &picture_row(BUS + front_at(index), FRONT_SIZE, front_pic(index)),
            )
            .write(
                row(FRONT_PALETTE_TABLE, front_pic(index)),
                &palette_row(BUS + palette_at(index), front_pic(index)),
            )
            .write(
                row(BACK_TABLE, back_pic(index)),
                &picture_row(BUS + back_at(index), BACK_PICTURE_SIZE, back_pic(index)),
            )
            .write(
                row(BACK_PALETTE_TABLE, back_pic(index)),
                &palette_row(BUS + palette_at(index), back_pic(index)),
            )
            .write(front_at(index) as usize, &lz77_literals(&tiles(index, 64)))
            .write(
                palette_at(index) as usize,
                &lz77_literals(&colour_bytes(index)),
            )
            .write(back_at(index) as usize, &tiles(index + 10, 256));
    }
    rom
}

fn rom() -> Vec<u8> {
    base_fixture().finish()
}

type Located = Result<(TrainerBattlerPlan, Vec<ReportLine>), GenRomProfileError>;

fn located(label: &str, rom: &[u8], pack: Vec<PackEntry>) -> Located {
    with_context(label, rom, pack, |ctx| {
        let mut report = Vec::new();
        let portraits = trainer_front::locate(ctx, &mut report)?;
        locate(ctx, &portraits, &mut report).map(|plan| (plan, report))
    })
}

fn assert_struct_mismatch(result: Located, id: &str) {
    let err = result.unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::StructMismatch { id: found, .. } if found == id),
        "{err}"
    );
}

#[test]
fn both_trainers_and_both_back_tables_are_found() {
    let (plan, report) = located("trainer-battlers", &rom(), entries()).expect("located");
    assert_eq!(plan.front.front_table, BUS + FRONT_TABLE);
    assert_eq!(plan.front.palette_table, BUS + FRONT_PALETTE_TABLE);
    assert_eq!(plan.back_table, BUS + BACK_TABLE);
    assert_eq!(plan.back_palette_table, BUS + BACK_PALETTE_TABLE);
    assert_eq!(plan.front.trainers.len(), 2);
    assert_eq!(plan.backs.len(), 2);
    for (index, trainer) in TRAINER_BATTLERS.iter().enumerate() {
        let front = &plan.front.trainers[index];
        assert_eq!(front.trainer_pic, 71 + u16::try_from(index).unwrap());
        assert_eq!(front.front.id, trainer.front_id);
        assert_eq!(front.front.addr, BUS + front_at(index));
        assert_eq!(front.front.encoding, Encoding::Lz77);
        assert_eq!(front.palette.id, trainer.palette_id);
        assert_eq!(front.palette.addr, BUS + palette_at(index));

        let back = &plan.backs[index];
        assert_eq!(back.trainer_pic, front.trainer_pic);
        assert_eq!(back.back_pic, u16::try_from(index).unwrap());
        assert_eq!(back.back.id, trainer.back_id);
        assert_eq!(back.back.addr, BUS + back_at(index));
        assert_eq!(back.back.encoding, Encoding::Raw);
        assert_eq!(
            (
                back.back.rom_bit_depth,
                back.back.pack_bit_depth,
                back.back.width,
                back.back.height,
                back.back.tile_count
            ),
            (4, 4, 64, 256, 256)
        );
    }
    // The portraits' four lines and two tables, then the battlers' two
    // repeated pictures and palettes, two back sheets, and two back tables.
    assert_eq!(report.len(), 6 + 4 + 2 + 2);
    let table = report
        .iter()
        .find(|line| line.id == "gTrainerBackPicTable")
        .expect("a table line");
    assert_eq!(table.len, BACK_TABLE_ROWS * 8);
}

#[test]
fn the_battler_lines_repeat_the_portraits_addresses_and_name_each_symbol() {
    let (_, report) = located("trainer-battlers-symbols", &rom(), entries()).expect("located");
    let line = |id: &str| report.iter().find(|line| line.id == id).expect(id);
    let exact = |name: &str| SymbolExpectation::Exact(name.to_owned());
    for (index, (battler, portrait)) in TRAINER_BATTLERS.iter().zip(TRAINER_FRONT_PICS).enumerate()
    {
        assert_eq!(line(battler.front_id).addr, line(portrait.image_id).addr);
        assert_eq!(
            line(battler.palette_id).addr,
            line(portrait.palette_id).addr
        );
        assert_eq!(line(battler.back_id).addr, BUS + back_at(index));
    }
    assert_eq!(
        line("trainer/brendan/front").symbol,
        exact("gTrainerFrontPic_Brendan")
    );
    assert_eq!(
        line("trainer/may/palette").symbol,
        exact("gTrainerPalette_May")
    );
    assert_eq!(
        line("trainer/brendan/back").symbol,
        exact("gTrainerBackPic_Brendan")
    );
    assert_eq!(
        line("trainer/may/back").symbol,
        exact("gTrainerBackPic_May")
    );
    assert_eq!(
        line("gTrainerBackPicTable").symbol,
        exact("gTrainerBackPicTable")
    );
    assert_eq!(
        line("gTrainerBackPicPaletteTable").symbol,
        exact("gTrainerBackPicPaletteTable")
    );
}

#[test]
fn a_battler_entry_that_differs_from_its_portrait_is_refused() {
    let replaced = |id: &str, entry: PackEntry| {
        let mut pack = entries();
        pack.retain(|existing| existing.id != id);
        pack.push(entry);
        pack
    };
    assert_struct_mismatch(
        located(
            "trainer-battlers-front-differs",
            &rom(),
            replaced("trainer/may/front", front_entry("trainer/may/front", 9)),
        ),
        "trainer/may/front",
    );
    assert_struct_mismatch(
        located(
            "trainer-battlers-palette-differs",
            &rom(),
            replaced(
                "trainer/brendan/palette",
                palette_entry("trainer/brendan/palette".to_owned(), &colors(9)).unwrap(),
            ),
        ),
        "trainer/brendan/palette",
    );
}

#[test]
fn a_back_row_with_the_wrong_size_or_tag_pins_no_back_table() {
    let may = row(BACK_TABLE, back_pic(1));
    // A frame's size, and the front index used as the back tag.
    for (label, bytes) in [
        ("size", picture_row(BUS + back_at(1), 0x800, back_pic(1))),
        (
            "tag",
            picture_row(BUS + back_at(1), BACK_PICTURE_SIZE, front_pic(1)),
        ),
    ] {
        let rom = base_fixture().write(may, &bytes).finish();
        assert_struct_mismatch(
            located(&format!("trainer-battlers-bad-{label}"), &rom, entries()),
            "gTrainerBackPicTable",
        );
    }
}

#[test]
fn a_back_palette_row_must_point_at_the_front_palette_not_a_copy() {
    let rom = base_fixture()
        .write(PALETTE_COPY as usize, &lz77_literals(&colour_bytes(1)))
        .write(
            row(BACK_PALETTE_TABLE, back_pic(1)),
            &palette_row(BUS + PALETTE_COPY, back_pic(1)),
        )
        .finish();
    assert_struct_mismatch(
        located("trainer-battlers-palette-copy", &rom, entries()),
        "gTrainerBackPicPaletteTable",
    );
    let rom = base_fixture()
        .write(
            row(BACK_PALETTE_TABLE, back_pic(1)),
            &palette_row(BUS + palette_at(1), front_pic(1)),
        )
        .finish();
    assert_struct_mismatch(
        located("trainer-battlers-palette-tag", &rom, entries()),
        "gTrainerBackPicPaletteTable",
    );
}

#[test]
fn frame_descriptors_pointing_at_the_sheets_are_not_a_back_table() {
    // `src/data.c` lists each frame of a sheet with the frame's own size; none
    // of those rows is a table row, and the real table is still the one found.
    let mut fixture = base_fixture();
    for index in 0..TRAINER_BATTLERS.len() {
        for frame in 0..4_u32 {
            fixture = fixture.write(
                (FRAME_ROWS + (u32::try_from(index).unwrap() * 4 + frame) * 8) as usize,
                &picture_row(BUS + back_at(index) + frame * 0x800, FRONT_SIZE, 0),
            );
        }
    }
    let (plan, _) = located("trainer-battlers-frames", &fixture.finish(), entries())
        .expect("located despite the lookalike rows");
    assert_eq!(plan.back_table, BUS + BACK_TABLE);
}

#[test]
fn a_second_back_table_holding_the_same_rows_is_ambiguous_not_first_wins() {
    let mut fixture = base_fixture();
    for index in 0..TRAINER_BATTLERS.len() {
        fixture = fixture.write(
            row(OTHER_BACK_TABLE, back_pic(index)),
            &picture_row(BUS + back_at(index), BACK_PICTURE_SIZE, back_pic(index)),
        );
    }
    assert_struct_mismatch(
        located("trainer-battlers-two-tables", &fixture.finish(), entries()),
        "gTrainerBackPicTable",
    );
}

#[test]
fn a_back_sheet_the_rom_lacks_is_not_found() {
    let mut bytes = rom();
    let start = back_at(1) as usize;
    bytes[start..start + 4].fill(0xFF);
    let err = located("trainer-battlers-missing-back", &bytes, entries()).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::NotFound { id } if id == "trainer/may/back"),
        "{err}"
    );
}

#[test]
fn a_pack_without_a_battler_entry_is_refused_before_any_report_line() {
    for id in trainer_battler_ids() {
        let mut pack = entries();
        pack.retain(|entry| entry.id != id);
        let rom = rom();
        with_context("trainer-battlers-pack-gap", &rom, pack, |ctx| {
            let portraits = {
                let mut report = Vec::new();
                trainer_front::locate(ctx, &mut report).expect("portraits")
            };
            let mut report = Vec::new();
            let err = locate(ctx, &portraits, &mut report).unwrap_err();
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
    let surplus = "trainer/wally/front";
    let mut pack = entries();
    pack.push(raw_entry(surplus.to_owned(), vec![0]));
    let rom = rom();
    with_context("trainer-battlers-surplus", &rom, pack, |ctx| {
        let err = require_trainer_battlers(ctx.pack).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == surplus),
            "{err}"
        );
    });
}

#[test]
fn the_inventory_is_each_trainers_front_back_then_palette() {
    assert_eq!(
        trainer_battler_ids(),
        [
            "trainer/brendan/front",
            "trainer/brendan/back",
            "trainer/brendan/palette",
            "trainer/may/front",
            "trainer/may/back",
            "trainer/may/palette",
        ]
    );
}
