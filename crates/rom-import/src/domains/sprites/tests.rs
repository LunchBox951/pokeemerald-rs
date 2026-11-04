//! Sprite reader tests over a synthetic ROM.
//!
//! The reader's own logic is the walk of `sObjectEventSpritePalettes`, so
//! the cases are a table that names every palette, one that names a
//! different copy, one with no terminator, and one addressed past the
//! image. The sheets go through the shared image reader and are covered
//! there. The field-effect sheet and palette, which the table does not
//! name, are written without consulting it.

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
/// Where the five-frame field-effect sheet sits.
const FIELD_SHEET: usize = 0x4000;
/// The address of that sheet.
const FIELD_SHEET_AT: GbaPtr = GbaPtr::at(ROM_BASE + 0x4000);
/// Where the field-effect bank sits: absent from the palette table.
const FIELD_BANK: usize = 0x5000;
/// The address of that bank.
const FIELD_BANK_AT: GbaPtr = GbaPtr::at(ROM_BASE + 0x5000);
/// Tiles in the field-effect sheet: five frames of 2x2 tiles.
const FIELD_TILES: u32 = 20;
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

static FIELD_SHEETS: [ImageRoot; 2] = [
    SHEETS[0],
    ImageRoot {
        id: "field-effect/image/test",
        addr: FIELD_SHEET_AT,
        encoding: Encoding::Raw,
        rom_bit_depth: 4,
        pack_bit_depth: 4,
        width: 80,
        height: 16,
        metatile_width: 2,
        metatile_height: 2,
        tile_count: FIELD_TILES,
    },
];

/// A bank unlike [`BANK`]: blue, then fifteen zeros.
const FIELD_BANK_COLORS: [u8; 32] = {
    let mut bank = [0u8; 32];
    bank[1] = 0x7C;
    bank
};

static FIELD_PALETTES: [PaletteRoot; 1] = [PaletteRoot {
    id: "field-effect/palette/test",
    addr: FIELD_BANK_AT,
    encoding: Encoding::Raw,
    color_count: 16,
}];

static PALETTES: [PaletteRoot; 1] = [PaletteRoot {
    id: "sprite/palette/test",
    addr: NAMED_AT,
    encoding: Encoding::Raw,
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

/// The field-effect sheet as the ROM stores it: tile `k` has its first
/// pixel at `k % 15 + 1` and its second at `k / 15`, and no other pixel set.
fn field_sheet_tiles() -> Vec<u8> {
    let tile_count = usize::try_from(FIELD_TILES).expect("a small count");
    let mut tiles = vec![0u8; tile_count * 32];
    for tile in 0..tile_count {
        let first = u8::try_from(tile % 15 + 1).expect("a nibble");
        let second = u8::try_from(tile / 15).expect("a nibble");
        tiles[tile * 32] = first | (second << 4);
    }
    tiles
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
        .write(FIELD_SHEET, &field_sheet_tiles())
        .write(FIELD_BANK, &FIELD_BANK_COLORS)
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
            field_effect_palettes: &[],
        },
        ..Roots::NONE
    }
}

/// [`roots`] plus the field-effect sheet and bank.
fn field_effect_roots(table: GbaPtr, palettes: &'static [PaletteRoot]) -> Roots {
    Roots {
        sprites: SpriteRoots {
            sheets: &FIELD_SHEETS,
            field_effect_palettes: &FIELD_PALETTES,
            ..roots(table, palettes).sprites
        },
        ..Roots::NONE
    }
}

/// Run the domain and hand back the finished pack.
fn run_pack(roots: &Roots) -> Result<Vec<u8>, ImportError> {
    let rom = rom();
    let mut writer = PackWriter::new();
    write(&rom, roots, &mut writer)?;
    Ok(writer.finish()?)
}

/// Run the domain and hand back the sorted directory it wrote.
fn run(roots: &Roots) -> Result<Vec<DirectoryEntry>, ImportError> {
    let pack = run_pack(roots)?;
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
fn the_field_effect_sheet_and_bank_are_written_without_the_palette_table() {
    // The table address is garbage: the field-effect bank is not in it.
    let roots = field_effect_roots(PAST_THE_ROM, &[]);
    let pack = run_pack(&roots).expect("nothing consults the table");
    let entries = parse_directory(&pack).expect("a readable pack");
    let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "field-effect/image/test",
            "field-effect/palette/test",
            "sprite/test"
        ]
    );
    let payload = |index: usize| &pack[entries[index].offset..][..entries[index].length];

    assert_eq!(
        entries[0].kind,
        EntryKind::Image {
            width: 80,
            height: 16,
            bit_depth: 4
        }
    );
    // Row-major palette indices, one byte per pixel.
    let image = payload(0);
    assert_eq!(image.len(), 80 * 16);
    let mut expected = vec![0u8; 80 * 16];
    for frame in 0..5 {
        for row in 0..2 {
            for column in 0..2 {
                let tile = frame * 4 + row * 2 + column;
                let (x, y) = (frame * 16 + column * 8, row * 8);
                expected[y * 80 + x] = u8::try_from(tile % 15 + 1).expect("a nibble");
                expected[y * 80 + x + 1] = u8::try_from(tile / 15).expect("a nibble");
            }
        }
    }
    assert_eq!(image, expected);

    assert_eq!(entries[1].kind, EntryKind::Palette { color_count: 16 });
    assert_eq!(payload(1), FIELD_BANK_COLORS);
}

#[test]
fn a_field_effect_bank_does_not_excuse_an_ordinary_one_the_table_omits() {
    let roots = field_effect_roots(TABLE_AT, &STRAY_PALETTES);
    let err = run(&roots).unwrap_err();
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

static PAST_THE_ROM_FIELD_PALETTES: [PaletteRoot; 1] = [PaletteRoot {
    addr: PAST_THE_ROM,
    ..FIELD_PALETTES[0]
}];

#[test]
fn a_field_effect_bank_past_the_image_is_truncated() {
    let roots = Roots {
        sprites: SpriteRoots {
            field_effect_palettes: &PAST_THE_ROM_FIELD_PALETTES,
            ..field_effect_roots(TABLE_AT, &PALETTES).sprites
        },
        ..Roots::NONE
    };
    let err = run(&roots).unwrap_err();
    assert!(matches!(err, ImportError::Truncated { .. }), "{err}");
}

#[test]
fn the_aggregate_palette_iterator_includes_the_field_effect_bank() {
    let roots = field_effect_roots(TABLE_AT, &PALETTES);
    let palettes: Vec<&str> = roots.palettes().map(|root| root.id).collect();
    assert_eq!(
        palettes,
        ["sprite/palette/test", "field-effect/palette/test"]
    );
}

#[test]
fn a_record_is_eight_bytes() {
    assert_eq!(record(NAMED_AT, 1).len(), RECORD_BYTES);
}
