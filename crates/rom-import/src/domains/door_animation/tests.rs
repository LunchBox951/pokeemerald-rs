//! Door-animation reader tests over a synthetic ROM.
//!
//! The reader's own logic is the table: each row's metatile, sound, size,
//! tile pointer and palette pointer, and the terminator behind the last row.
//! So the cases are a well-formed ROM, which must publish the ROM's own bytes,
//! and a ROM per field that disagrees with the profile or the schema.

use assets::{DoorAnimationRecord, DoorAnimationTable, DoorSize, DoorSound};
use pack_format::{parse_directory, DirectoryEntry, EntryKind, PackWriter};

use super::write;
use crate::domains::DOMAINS;
use crate::error::ImportError;
use crate::fixture::RomFixture;
use crate::reader::{GbaPtr, ROM_BASE};
use crate::rom::{Rom, ROM_SIZE};
use crate::roots::{DoorAnimationRoots, DoorAnimationRow, Encoding, ImageRoot, Roots};

const TABLE: u32 = 0x1_0000;
const LITTLEROOT: u32 = 0x2_0000;
const BIRCHS_LAB: u32 = 0x3_0000;
const MULTI_CORRIDOR: u32 = 0x4_0000;
const LITTLEROOT_BANKS: u32 = 0x5_0000;
const LAB_BANKS: u32 = 0x5_0010;
const LARGE_BANKS: u32 = 0x5_0020;
/// A second copy of the Littleroot sheet, which no row points at.
const STRAY_SHEET: u32 = 0x6_0000;

const fn at(off: u32) -> GbaPtr {
    GbaPtr::at(ROM_BASE + off)
}

fn idx(off: u32) -> usize {
    usize::try_from(off).expect("a ROM offset fits in usize")
}

fn row(metatile: u16, sound: u8, size: u8, tiles: u32, banks: u32) -> Vec<u8> {
    let mut row = metatile.to_le_bytes().to_vec();
    row.extend_from_slice(&[sound, size]);
    row.extend_from_slice(&at(tiles).raw().to_le_bytes());
    row.extend_from_slice(&at(banks).raw().to_le_bytes());
    row
}

/// A sheet's tiles, tile `i` filled with the byte whose nibbles are `i`.
fn tiles(count: u32) -> Vec<u8> {
    (0..count)
        .flat_map(|tile| vec![u8::try_from(tile % 16 * 17).unwrap(); 32])
        .collect()
}

fn fixture() -> RomFixture {
    let mut rows = row(0x248, 0, 1, LITTLEROOT, LITTLEROOT_BANKS);
    rows.extend(row(0x249, 0, 1, BIRCHS_LAB, LAB_BANKS));
    rows.extend(row(0x2AD, 1, 2, MULTI_CORRIDOR, LARGE_BANKS));
    rows.extend([0; 12]);
    RomFixture::new()
        .emerald_header()
        .write(idx(TABLE), &rows)
        .write(idx(LITTLEROOT), &tiles(24))
        .write(idx(STRAY_SHEET), &tiles(24))
        .write(idx(BIRCHS_LAB), &vec![0x55; 24 * 32])
        .write(idx(MULTI_CORRIDOR), &tiles(48))
        .write(idx(LITTLEROOT_BANKS), &[10, 10, 6, 6, 6, 6, 6, 6])
        .write(idx(LAB_BANKS), &[8; 8])
        .write(idx(LARGE_BANKS), &[7; 8])
}

fn rom_of(fixture: RomFixture) -> Rom {
    Rom::from_bytes(fixture.finish()).expect("the fixture header is valid")
}

const fn image(id: &'static str, addr: GbaPtr, width: u32, metatile: (u32, u32)) -> ImageRoot {
    ImageRoot {
        id,
        addr,
        encoding: Encoding::Raw,
        rom_bit_depth: 4,
        pack_bit_depth: 4,
        width,
        height: 96,
        metatile_width: metatile.0,
        metatile_height: metatile.1,
        tile_count: width / 8 * 12,
    }
}

static IMAGES: [ImageRoot; 3] = [
    image(
        "door-animation/image/littleroot",
        at(LITTLEROOT),
        16,
        (1, 1),
    ),
    image(
        "door-animation/image/birchs_lab",
        at(BIRCHS_LAB),
        16,
        (1, 1),
    ),
    image(
        "door-animation/image/battle_tower_multi_corridor",
        at(MULTI_CORRIDOR),
        32,
        (2, 4),
    ),
];

static ROWS: [DoorAnimationRow; 3] = [
    DoorAnimationRow {
        metatile: 0x248,
        sheet: "littleroot",
        palette_slots: at(LITTLEROOT_BANKS),
    },
    DoorAnimationRow {
        metatile: 0x249,
        sheet: "birchs_lab",
        palette_slots: at(LAB_BANKS),
    },
    DoorAnimationRow {
        metatile: 0x2AD,
        sheet: "battle_tower_multi_corridor",
        palette_slots: at(LARGE_BANKS),
    },
];

fn roots() -> Roots {
    Roots {
        door_animation: DoorAnimationRoots {
            table: at(TABLE),
            rows: &ROWS,
            images: &IMAGES,
        },
        ..Roots::NONE
    }
}

fn pack_of(rom: &Rom, roots: &Roots) -> Result<Vec<u8>, ImportError> {
    let mut writer = PackWriter::new();
    write(rom, roots, &mut writer)?;
    Ok(writer.finish().expect("no duplicate ids"))
}

fn entry<'a>(bytes: &'a [u8], id: &str) -> (DirectoryEntry, &'a [u8]) {
    let entry = parse_directory(bytes)
        .expect("a valid pack")
        .into_iter()
        .find(|entry| entry.id == id)
        .unwrap_or_else(|| panic!("no entry {id}"));
    let payload = &bytes[entry.offset..entry.offset + entry.length];
    (entry, payload)
}

fn assert_mismatch(fixture: RomFixture, expected_field: &str) {
    match pack_of(&rom_of(fixture), &roots()) {
        Err(ImportError::StructMismatch { field, .. }) => assert_eq!(field, expected_field),
        other => panic!("expected a {expected_field} mismatch, got {other:?}"),
    }
}

#[test]
fn the_table_publishes_the_roms_own_sound_size_and_banks() {
    let bytes = pack_of(&rom_of(fixture()), &roots()).expect("a well-formed table");
    let ids: Vec<String> = parse_directory(&bytes)
        .unwrap()
        .into_iter()
        .map(|entry| entry.id)
        .collect();
    assert_eq!(
        ids,
        [
            "door-animation/image/battle_tower_multi_corridor",
            "door-animation/image/birchs_lab",
            "door-animation/image/littleroot",
            "door-animation/table",
        ]
    );
    let (_, payload) = entry(&bytes, "door-animation/table");
    let table = DoorAnimationTable::decode(payload).expect("the schema the pack reads");
    let record = |metatile, sound, size, banks, sheet: &str| DoorAnimationRecord {
        metatile,
        sound,
        size,
        palette_slots: banks,
        sheet: sheet.to_owned(),
    };
    assert_eq!(
        table.records(),
        [
            record(
                0x248,
                DoorSound::Normal,
                DoorSize::OneTile,
                [10, 10, 6, 6, 6, 6, 6, 6],
                "littleroot"
            ),
            record(
                0x249,
                DoorSound::Normal,
                DoorSize::OneTile,
                [8; 8],
                "birchs_lab"
            ),
            record(
                0x2AD,
                DoorSound::Sliding,
                DoorSize::Large,
                [7; 8],
                "battle_tower_multi_corridor"
            ),
        ]
    );
}

#[test]
fn the_large_sheet_is_unpacked_in_two_by_four_tile_blocks() {
    let bytes = pack_of(&rom_of(fixture()), &roots()).unwrap();
    let (entry, pixels) = entry(&bytes, "door-animation/image/battle_tower_multi_corridor");
    assert_eq!(
        entry.kind,
        EntryKind::Image {
            width: 32,
            height: 96,
            bit_depth: 4
        }
    );
    // The raster is 4 tiles wide and 12 tall; the ROM stores it as 2x3 blocks
    // of 2x4 tiles, each block row-major inside.
    for (tile_x, tile_y) in [(0u32, 0u32), (1, 0), (0, 1), (3, 5), (2, 11), (3, 11)] {
        let block = (tile_y / 4) * 2 + tile_x / 2;
        let rom_tile = block * 8 + (tile_y % 4) * 2 + tile_x % 2;
        let at = (tile_y * 8 * 32 + tile_x * 8) as usize;
        assert_eq!(
            u32::from(pixels[at]),
            rom_tile % 16,
            "tile ({tile_x}, {tile_y})"
        );
    }
}

#[test]
fn a_row_that_disagrees_with_the_profile_is_a_struct_mismatch() {
    let rows = |edit: &dyn Fn(&mut Vec<u8>)| {
        let mut bytes = row(0x248, 0, 1, LITTLEROOT, LITTLEROOT_BANKS);
        edit(&mut bytes);
        fixture().write(idx(TABLE), &bytes)
    };
    assert_mismatch(rows(&|b| b[0] = 0x47), "DoorGraphics.metatileNum");
    assert_mismatch(rows(&|b| b[2] = 3), "DoorGraphics.sound");
    assert_mismatch(rows(&|b| b[3] = 0), "DoorGraphics.size");
    // A one-tile sheet with a large size class.
    assert_mismatch(rows(&|b| b[3] = 2), "DoorGraphics.size");
    // A pointer at the stray copy of the sheet.
    assert_mismatch(
        rows(&|b| b[4..8].copy_from_slice(&at(STRAY_SHEET).raw().to_le_bytes())),
        "DoorGraphics.tiles",
    );
    assert_mismatch(
        rows(&|b| b[8..12].copy_from_slice(&at(LAB_BANKS).raw().to_le_bytes())),
        "DoorGraphics.palettes",
    );
}

#[test]
fn a_sheet_no_row_names_is_refused() {
    static EXTRA: [ImageRoot; 4] = [
        IMAGES[0],
        IMAGES[1],
        IMAGES[2],
        image("door-animation/image/general", at(STRAY_SHEET), 16, (1, 1)),
    ];
    let mut roots = roots();
    roots.door_animation.images = &EXTRA;
    match pack_of(&rom_of(fixture()), &roots) {
        Err(ImportError::StructMismatch { root, field }) => {
            assert_eq!(
                (root, field),
                ("door-animation/image/general", "DoorGraphics.tiles")
            );
        }
        other => panic!("expected a mismatch, got {other:?}"),
    }
}

#[test]
fn a_bank_past_the_tilesets_sixteen_is_refused() {
    assert_mismatch(
        fixture().write(idx(LITTLEROOT_BANKS), &[10, 10, 6, 6, 6, 6, 6, 16]),
        "DoorGraphics.palettes",
    );
}

#[test]
fn a_table_without_its_terminator_is_refused() {
    assert_mismatch(
        fixture().write(idx(TABLE) + 36, &[1]),
        "DoorGraphics terminator",
    );
}

#[test]
fn a_table_that_runs_off_the_end_of_the_rom_fails_at_the_row_that_crosses_it() {
    // The first two rows fill the last 24 bytes of the ROM; the third begins
    // at the end.
    let base = ROM_SIZE - 24;
    let mut rows = row(0x248, 0, 1, LITTLEROOT, LITTLEROOT_BANKS);
    rows.extend(row(0x249, 0, 1, BIRCHS_LAB, LAB_BANKS));
    let mut roots = roots();
    roots.door_animation.table = at(u32::try_from(base).unwrap());
    let rom = rom_of(fixture().write(base, &rows));
    let err = pack_of(&rom, &roots).unwrap_err();
    assert!(
        matches!(err, ImportError::Truncated { at, .. } if at == ROM_SIZE),
        "{err:?}"
    );
}

#[test]
fn a_failing_row_leaves_the_writer_untouched() {
    let rom = rom_of(fixture().write(idx(LAB_BANKS), &[16; 8]));
    let mut writer = PackWriter::new();
    assert!(write(&rom, &roots(), &mut writer).is_err());
    assert_eq!(writer.len(), 0);
}

#[test]
fn a_profile_without_door_rows_writes_nothing() {
    let rom = rom_of(fixture());
    let mut writer = PackWriter::new();
    write(&rom, &Roots::NONE, &mut writer).expect("nothing to read");
    assert_eq!(writer.len(), 0);
    assert_eq!(Roots::NONE.door_animation, DoorAnimationRoots::NONE);
}

#[test]
fn the_domain_list_dispatches_the_doors_and_the_sheets_are_enumerated() {
    let rom = rom_of(fixture());
    let roots = roots();
    let mut writer = PackWriter::new();
    for domain in DOMAINS {
        domain(&rom, &roots, &mut writer).expect("every root is well-formed");
    }
    assert_eq!(writer.len(), 4);
    let images: Vec<&str> = roots.images().map(|root| root.id).collect();
    assert_eq!(
        images,
        [
            "door-animation/image/littleroot",
            "door-animation/image/birchs_lab",
            "door-animation/image/battle_tower_multi_corridor",
        ]
    );
    assert_eq!(roots.palettes().count(), 0);
}
