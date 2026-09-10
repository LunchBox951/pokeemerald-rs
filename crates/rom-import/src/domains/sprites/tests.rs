//! Sprite reader tests over a synthetic ROM.
//!
//! The reader's own logic is the walk of `sObjectEventSpritePalettes`, so
//! the cases are a table that names every palette, one that names a
//! different copy, one with no terminator, and one addressed past the
//! image. The sheets go through the shared image reader and are covered
//! there.

use pack_format::{parse_directory, DirectoryEntry, EntryKind, PackWriter};

use super::{write, RECORD_BYTES};
use crate::error::ImportError;
use crate::fixture::RomFixture;
use crate::reader::{GbaPtr, ROM_BASE};
use crate::rom::Rom;
use crate::roots::{Encoding, ImageRoot, PaletteRoot, Roots, SpriteRoots};

/// Where the fixture's palette table sits.
const TABLE: usize = 0x1000;
/// The address of that table.
const TABLE_AT: GbaPtr = GbaPtr::at(ROM_BASE + 0x1000);
/// Where a bank the table names sits.
const NAMED: usize = 0x2000;
/// The address of that bank.
const NAMED_AT: GbaPtr = GbaPtr::at(ROM_BASE + 0x2000);
/// Where an identical bank the table does not name sits.
const STRAY: usize = 0x2100;
/// The address of that bank.
const STRAY_AT: GbaPtr = GbaPtr::at(ROM_BASE + 0x2100);
/// Where the sheet sits.
const SHEET: usize = 0x3000;
/// The address of that sheet.
const SHEET_AT: GbaPtr = GbaPtr::at(ROM_BASE + 0x3000);
/// A table with no terminator, ending in the image's last bytes.
const UNTERMINATED: usize = 0x1_0000;
/// The address of that table.
const UNTERMINATED_AT: GbaPtr = GbaPtr::at(ROM_BASE + 0x1_0000);
/// An address inside the cartridge window but past a 16 MiB image.
const PAST_THE_ROM: GbaPtr = GbaPtr::at(ROM_BASE + 0x00FF_FFF0);

/// One bank: red, then green, then fourteen zeros.
const BANK: [u8; 32] = {
    let mut bank = [0u8; 32];
    bank[0] = 0x1F;
    bank[2] = 0xE0;
    bank[3] = 0x03;
    bank
};

static SHEETS: [ImageRoot; 1] = [ImageRoot {
    id: "sprite/test",
    addr: SHEET_AT,
    encoding: Encoding::Raw,
    rom_bit_depth: 4,
    pack_bit_depth: 4,
    width: 8,
    height: 8,
    metatile_width: 1,
    metatile_height: 1,
    tile_count: 1,
}];

static PALETTES: [PaletteRoot; 1] = [PaletteRoot {
    id: "sprite/palette/test",
    addr: NAMED_AT,
    color_count: 16,
}];

/// The same bank, recorded at the copy the table does not name.
static STRAY_PALETTES: [PaletteRoot; 1] = [PaletteRoot {
    addr: STRAY_AT,
    ..PALETTES[0]
}];

/// A `{data, tag}` record.
fn record(data: GbaPtr, tag: u16) -> Vec<u8> {
    let mut bytes = data.raw().to_le_bytes().to_vec();
    bytes.extend_from_slice(&tag.to_le_bytes());
    bytes.extend_from_slice(&[0, 0]);
    bytes
}

fn rom() -> Rom {
    let mut table = record(NAMED_AT, 0x1103);
    // The retail terminator: an all-zero record.
    table.extend([0u8; RECORD_BYTES]);
    // Live records all the way to the end of the image.
    let mut unterminated = Vec::new();
    while unterminated.len() < crate::rom::ROM_SIZE - UNTERMINATED {
        unterminated.extend(record(NAMED_AT, 0x1103));
    }
    unterminated.truncate(crate::rom::ROM_SIZE - UNTERMINATED);
    let bytes = RomFixture::new()
        .emerald_header()
        .write(TABLE, &table)
        .write(NAMED, &BANK)
        .write(STRAY, &BANK)
        .write(SHEET, &[0x10, 0x32, 0x54, 0x76])
        .write(UNTERMINATED, &unterminated)
        .finish();
    Rom::from_bytes(bytes).expect("the fixture header is valid")
}

fn roots(table: GbaPtr, palettes: &'static [PaletteRoot]) -> Roots {
    Roots {
        sprites: SpriteRoots {
            palette_table: table,
            sheets: &SHEETS,
            palettes,
        },
        ..Roots::NONE
    }
}

/// Run the domain and hand back the sorted directory it wrote.
fn run(roots: &Roots) -> Result<Vec<DirectoryEntry>, ImportError> {
    let rom = rom();
    let mut writer = PackWriter::new();
    write(&rom, roots, &mut writer)?;
    let pack = writer.finish()?;
    Ok(parse_directory(&pack).expect("a readable pack"))
}

#[test]
fn a_named_palette_and_its_sheet_are_written() {
    let entries = run(&roots(TABLE_AT, &PALETTES)).expect("a well-formed table");
    let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, ["sprite/palette/test", "sprite/test"]);
    assert_eq!(entries[0].kind, EntryKind::Palette { color_count: 16 });
}

#[test]
fn a_palette_the_table_does_not_name_is_refused() {
    let err = run(&roots(TABLE_AT, &STRAY_PALETTES)).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::StructMismatch {
                root: "sprite/palette/test",
                field: "SpritePalette.data",
            }
        ),
        "{err}"
    );
}

#[test]
fn a_table_without_a_terminator_is_refused() {
    let err = run(&roots(UNTERMINATED_AT, &PALETTES)).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::StructMismatch {
                root: "sObjectEventSpritePalettes",
                field: "SpritePalette.data",
            }
        ) || matches!(err, ImportError::Truncated { .. }),
        "{err}"
    );
}

#[test]
fn a_table_past_the_image_is_truncated() {
    let err = run(&roots(PAST_THE_ROM, &PALETTES)).unwrap_err();
    assert!(matches!(err, ImportError::Truncated { .. }), "{err}");
}

#[test]
fn a_profile_with_no_palettes_reads_no_table() {
    // The table address is garbage and never read.
    let entries = run(&roots(PAST_THE_ROM, &[])).expect("no table to check");
    assert_eq!(entries.len(), 1);
}

#[test]
fn a_record_is_eight_bytes() {
    assert_eq!(record(NAMED_AT, 1).len(), RECORD_BYTES);
}
