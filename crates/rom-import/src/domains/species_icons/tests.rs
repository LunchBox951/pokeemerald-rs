//! Species-icon reader tests over a synthetic ROM.
//!
//! The whole family is planted under the real `species-icon/` ids at its
//! production shapes: 417 raw sheets, three banks, `gMonIconTable`'s 440
//! pointers, the six-row palette table and the index bytes. The failure tests
//! break one thing at a time and require a typed error with nothing queued.

use assets::species_icons::{
    decode_palette_table, decode_species_sheets, encode_palette_table, palette_pack_id,
    SpeciesIconSheetId, PALETTE_INDICES_ID, PALETTE_TABLE_ID, SHEETS, SPECIES_SHEETS_ID,
    SPECIES_SLOTS,
};
use pack_format::{parse_directory, DirectoryEntry, EntryKind, PackWriter};

use super::write;
use crate::domains::DOMAINS;
use crate::error::ImportError;
use crate::fixture::RomFixture;
use crate::reader::{GbaPtr, ROM_BASE};
use crate::rom::{Rom, ROM_SIZE};
use crate::roots::{BlobRoot, Encoding, ImageRoot, PaletteRoot, Roots, SpeciesIconRoots};

const SHEET_TABLE: usize = 0x8000;
const PALETTE_TABLE: usize = 0x9000;
const BANKS: usize = 0xA000;
const INDICES: usize = 0xB000;
const SHEET_BASE: usize = 0x1_0000;
const SHEET_STRIDE: usize = 0x1000;

const fn at(off: usize) -> GbaPtr {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "fixture offsets are far below u32::MAX"
    )]
    GbaPtr::at(ROM_BASE + off as u32)
}

fn sheet_offset(number: u16) -> usize {
    SHEET_BASE + usize::from(number) * SHEET_STRIDE
}

/// The sheet species slot `species` points at.
fn sheet_of(species: usize) -> u16 {
    match species {
        0 | 1 => 0,
        2..=26 => 5,
        _ => u16::try_from(species % 100 + 10).unwrap(),
    }
}

fn bank_of(species: usize) -> u8 {
    u8::try_from(species % 3).unwrap()
}

/// The tile bytes planted for sheet `number`.
fn tiles(number: u16) -> Vec<u8> {
    let len = if number == 416 { 128 * 32 } else { 32 * 32 };
    (0..len)
        .map(|byte| u8::try_from((usize::from(number) * 13 + byte * 7) % 251).unwrap())
        .collect()
}

fn bank_bytes(bank: u16) -> Vec<u8> {
    (0..16u16)
        .flat_map(|color| (0x100 * (bank + 1) + color).to_le_bytes())
        .collect()
}

/// A ROM with the family planted; `edit` runs on the fixture before sealing.
fn rom_with(edit: impl FnOnce(RomFixture) -> RomFixture) -> Rom {
    let mut fixture = RomFixture::new().emerald_header();
    for number in 0..SHEETS {
        fixture = fixture.write(sheet_offset(number), &tiles(number));
    }
    for species in 0..SPECIES_SLOTS {
        fixture = fixture.write_ptr(
            SHEET_TABLE + species * 4,
            at(sheet_offset(sheet_of(species))),
        );
    }
    for row in 0..6usize {
        fixture = fixture
            .write_ptr(PALETTE_TABLE + row * 8, at(BANKS + row * 32))
            .write(
                PALETTE_TABLE + row * 8 + 4,
                &(56000 + u16::try_from(row).unwrap()).to_le_bytes(),
            );
    }
    for bank in 0..3u16 {
        fixture = fixture.write(BANKS + usize::from(bank) * 32, &bank_bytes(bank));
    }
    let indices: Vec<u8> = (0..SPECIES_SLOTS).map(bank_of).collect();
    fixture = fixture.write(INDICES, &indices);
    Rom::from_bytes(edit(fixture).finish()).expect("the fixture header is valid")
}

fn rom() -> Rom {
    rom_with(|fixture| fixture)
}

fn sheet_roots() -> Vec<ImageRoot> {
    (0..SHEETS)
        .map(|number| {
            let id = SpeciesIconSheetId::new(number).unwrap();
            let (width, height) = id.size();
            ImageRoot {
                id: Box::leak(id.pack_id().into_boxed_str()),
                addr: at(sheet_offset(number)),
                encoding: Encoding::Raw,
                rom_bit_depth: 4,
                pack_bit_depth: 4,
                width,
                height,
                metatile_width: 1,
                metatile_height: 1,
                tile_count: width / 8 * (height / 8),
            }
        })
        .collect()
}

fn palette_roots() -> Vec<PaletteRoot> {
    (0..3u8)
        .map(|bank| PaletteRoot {
            id: Box::leak(palette_pack_id(bank).into_boxed_str()),
            addr: at(BANKS + usize::from(bank) * 32),
            color_count: 16,
            encoding: Encoding::Raw,
        })
        .collect()
}

fn family_with(
    edit: impl FnOnce(&mut Vec<ImageRoot>, &mut Vec<PaletteRoot>, &mut SpeciesIconRoots),
) -> SpeciesIconRoots {
    let mut sheets = sheet_roots();
    let mut palettes = palette_roots();
    let mut family = SpeciesIconRoots {
        sheet_table: at(SHEET_TABLE),
        palette_table: at(PALETTE_TABLE),
        palette_indices: BlobRoot {
            id: PALETTE_INDICES_ID,
            addr: at(INDICES),
            encoding: Encoding::Raw,
            len: 440,
        },
        sheets: &[],
        palettes: &[],
    };
    edit(&mut sheets, &mut palettes, &mut family);
    family.sheets = Vec::leak(sheets);
    family.palettes = Vec::leak(palettes);
    family
}

fn full() -> SpeciesIconRoots {
    family_with(|_, _, _| {})
}

fn roots(species_icons: SpeciesIconRoots) -> Roots {
    Roots {
        species_icons,
        ..Roots::NONE
    }
}

fn written(
    rom: &Rom,
    family: SpeciesIconRoots,
) -> Result<(Vec<u8>, Vec<DirectoryEntry>), ImportError> {
    let mut writer = PackWriter::new();
    write(rom, &roots(family), &mut writer)?;
    let bytes = writer.finish().expect("no duplicate ids");
    let entries = parse_directory(&bytes).expect("a valid pack");
    Ok((bytes, entries))
}

fn payload<'a>(bytes: &'a [u8], entries: &'a [DirectoryEntry], id: &str) -> &'a [u8] {
    let entry = entries
        .iter()
        .find(|entry| entry.id == id)
        .expect("an emitted id");
    &bytes[entry.offset..entry.offset + entry.length]
}

/// The write must fail with nothing queued.
fn refused(rom: &Rom, family: SpeciesIconRoots) -> ImportError {
    let mut writer = PackWriter::new();
    let error = write(rom, &roots(family), &mut writer).unwrap_err();
    assert_eq!(writer.len(), 0, "a failed import queues nothing");
    error
}

#[test]
fn every_entry_is_emitted_with_its_shape_and_bytes() {
    let (bytes, entries) = written(&rom(), full()).expect("the family is well-formed");
    assert_eq!(entries.len(), 423);

    for number in 0..SHEETS {
        let id = SpeciesIconSheetId::new(number).unwrap();
        let (width, height) = id.size();
        let entry = entries.iter().find(|e| e.id == id.pack_id()).unwrap();
        assert_eq!(
            entry.kind,
            EntryKind::Image {
                width,
                height,
                bit_depth: 4
            }
        );
    }
    // Sheet 7: the low nibble is the left pixel, and the fifth tile starts the
    // second tile row of a 32-wide raster.
    let pixels = payload(&bytes, &entries, "species-icon/image/007");
    let planted = tiles(7);
    assert_eq!(pixels[0], planted[0] & 0x0F);
    assert_eq!(pixels[1], planted[0] >> 4);
    assert_eq!(pixels[8 * 32], planted[4 * 32] & 0x0F);
    // The wide sheet is 16 tiles across.
    let wide = payload(&bytes, &entries, "species-icon/image/416");
    assert_eq!(wide.len(), 128 * 64);
    assert_eq!(wide[8 * 128], tiles(416)[16 * 32] & 0x0F);

    for bank in 0..3u16 {
        let id = format!("species-icon/palette/{bank}");
        assert_eq!(
            entries.iter().find(|e| e.id == id).unwrap().kind,
            EntryKind::Palette { color_count: 16 }
        );
        assert_eq!(payload(&bytes, &entries, &id), bank_bytes(bank).as_slice());
    }

    let sheets = decode_species_sheets(payload(&bytes, &entries, SPECIES_SHEETS_ID)).unwrap();
    assert_eq!(sheets.len(), 440);
    for (species, &sheet) in sheets.iter().enumerate() {
        assert_eq!(sheet, sheet_of(species), "species {species}");
    }
    let indices: Vec<u8> = (0..SPECIES_SLOTS).map(bank_of).collect();
    assert_eq!(payload(&bytes, &entries, PALETTE_INDICES_ID), indices);
    assert_eq!(
        payload(&bytes, &entries, PALETTE_TABLE_ID),
        encode_palette_table()
    );
    assert!(decode_palette_table(payload(&bytes, &entries, PALETTE_TABLE_ID)).is_ok());
}

#[test]
fn shared_slots_keep_one_sheet_and_equal_bytes_keep_two() {
    // Sheets 40 and 41 hold identical bytes at different addresses; the
    // pointer, not the bytes, picks between them.
    let rom = rom_with(|fixture| fixture.write(sheet_offset(41), &tiles(40)));
    let (bytes, entries) = written(&rom, full()).unwrap();
    let sheets = decode_species_sheets(payload(&bytes, &entries, SPECIES_SHEETS_ID)).unwrap();
    assert_eq!(sheets[31], 41);
    assert_eq!(sheets[30], 40);
    assert_eq!(
        payload(&bytes, &entries, "species-icon/image/040"),
        payload(&bytes, &entries, "species-icon/image/041")
    );
    // Species 0 and 1 share a sheet and both rows survive.
    assert_eq!((sheets[0], sheets[1]), (0, 0));
    assert!((2..=26).all(|s| sheets[s] == 5));
}

#[test]
fn a_profile_without_species_icons_writes_nothing() {
    let mut writer = PackWriter::new();
    write(&rom(), &Roots::NONE, &mut writer).unwrap();
    assert_eq!(writer.len(), 0);
}

#[test]
fn the_domain_list_dispatches_the_species_icons() {
    let rom = rom();
    let mut writer = PackWriter::new();
    for domain in DOMAINS {
        domain(&rom, &roots(full()), &mut writer).expect("every root is well-formed");
    }
    let entries = parse_directory(&writer.finish().unwrap()).unwrap();
    assert_eq!(
        entries
            .iter()
            .filter(|e| e.id.starts_with("species-icon/"))
            .count(),
        423
    );
}

#[test]
fn the_whole_root_iterators_include_the_family() {
    let all = roots(full());
    assert_eq!(all.images().count(), 417);
    assert_eq!(all.palettes().count(), 3);
}

#[test]
fn a_pointer_naming_no_recorded_sheet_is_refused() {
    // Inside the sheet, off its start.
    let inside = rom_with(|f| f.write_ptr(SHEET_TABLE + 9 * 4, at(sheet_offset(50) + 32)));
    assert!(matches!(
        refused(&inside, full()),
        ImportError::StructMismatch { root, field }
            if root == SPECIES_SHEETS_ID && field == "gMonIconTable"
    ));
    // Deoxys's speed and wide sheets are recorded, but no table row names them.
    for number in [415, 416] {
        let rom = rom_with(|f| f.write_ptr(SHEET_TABLE + 9 * 4, at(sheet_offset(number))));
        assert!(matches!(
            refused(&rom, full()),
            ImportError::StructMismatch { root, .. } if root == SPECIES_SHEETS_ID
        ));
    }
    // The last row, after 439 good ones, still queues nothing.
    let late = rom_with(|f| f.write_ptr(SHEET_TABLE + 439 * 4, at(0x20_0000)));
    assert!(matches!(
        refused(&late, full()),
        ImportError::StructMismatch { .. }
    ));
    // A value outside the cartridge window is a pointer fault.
    let wild = rom_with(|f| f.write(SHEET_TABLE + 4 * 4, &0x0300_0000u32.to_le_bytes()));
    assert!(matches!(
        refused(&wild, full()),
        ImportError::PointerOutOfRange { .. }
    ));
}

#[test]
fn a_sheet_table_running_off_the_rom_is_refused() {
    let family = family_with(|_, _, family| family.sheet_table = at(ROM_SIZE - 2));
    assert!(matches!(
        refused(&rom(), family),
        ImportError::Truncated { .. }
    ));
}

#[test]
fn palette_rows_are_checked_by_pointer_and_tag_and_never_read_as_banks() {
    // Rows 3 to 5 sit past the banks, over ROM filler: the full family loads.
    written(&rom(), full()).unwrap();
    for row in 0..6usize {
        let wrong_pointer =
            rom_with(|f| f.write_ptr(PALETTE_TABLE + row * 8, at(BANKS + row * 32 + 2)));
        assert!(matches!(
            refused(&wrong_pointer, full()),
            ImportError::StructMismatch { field, .. } if field == "SpritePalette.data"
        ));
        let wrong_tag = rom_with(|f| f.write(PALETTE_TABLE + row * 8 + 4, &56007u16.to_le_bytes()));
        assert!(matches!(
            refused(&wrong_tag, full()),
            ImportError::StructMismatch { field, .. } if field == "SpritePalette.tag"
        ));
    }
    let short = family_with(|_, _, family| family.palette_table = at(ROM_SIZE - 4));
    assert!(matches!(
        refused(&rom(), short),
        ImportError::Truncated { .. }
    ));
}

#[test]
fn an_out_of_range_palette_index_is_refused() {
    for bad in [3u8, 255] {
        let rom = rom_with(|f| f.write(INDICES + 439, &[bad]));
        assert!(matches!(
            refused(&rom, full()),
            ImportError::StructMismatch { root, .. } if root == PALETTE_INDICES_ID
        ));
    }
}

#[test]
fn a_family_that_is_not_the_closed_inventory_is_refused() {
    let wrong: Vec<(&str, SpeciesIconRoots)> = vec![
        (
            "missing sheet",
            family_with(|s, _, _| {
                s.pop();
            }),
        ),
        ("extra sheet", family_with(|s, _, _| s.push(s[0]))),
        ("swapped ids", family_with(|s, _, _| s.swap(3, 4))),
        (
            "duplicate address",
            family_with(|s, _, _| s[9].addr = s[8].addr),
        ),
        (
            "lz77 sheet",
            family_with(|s, _, _| s[6].encoding = Encoding::Lz77),
        ),
        ("short sheet", family_with(|s, _, _| s[6].tile_count = 31)),
        (
            "cropped sheet",
            family_with(|s, _, _| {
                s[6].height = 32;
                s[6].tile_count = 16;
            }),
        ),
        (
            "metatile sheet",
            family_with(|s, _, _| s[6].metatile_width = 4),
        ),
        (
            "narrow wide sheet",
            family_with(|s, _, _| {
                s[416].width = 32;
                s[416].tile_count = 32;
            }),
        ),
        (
            "missing bank",
            family_with(|_, p, _| {
                p.pop();
            }),
        ),
        (
            "bank id",
            family_with(|_, p, _| p[1].id = "species-icon/palette/9"),
        ),
        ("bank size", family_with(|_, p, _| p[1].color_count = 15)),
        (
            "bank gap",
            family_with(|_, p, _| p[2].addr = at(BANKS + 0x200)),
        ),
        (
            "index length",
            family_with(|_, _, f| f.palette_indices.len = 439),
        ),
        (
            "index id",
            family_with(|_, _, f| f.palette_indices.id = "species-icon/raw/other"),
        ),
        (
            "lz77 indices",
            family_with(|_, _, f| f.palette_indices.encoding = Encoding::Lz77),
        ),
        ("banks without sheets", family_with(|s, _, _| s.clear())),
    ];
    for (name, family) in wrong {
        assert!(
            matches!(refused(&rom(), family), ImportError::StructMismatch { .. }),
            "{name}"
        );
    }
}

#[test]
fn a_sheet_beyond_the_rom_is_refused_with_nothing_queued() {
    let family = family_with(|s, _, _| s[416].addr = at(ROM_SIZE - 100));
    assert!(matches!(
        refused(&rom(), family),
        ImportError::Truncated { .. }
    ));
}
