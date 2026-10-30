//! Battler reader tests over a synthetic ROM.
//!
//! The reader's own logic is the corroboration of each species' row in the
//! four tables, so the cases are a well-formed ROM and, per table, a row
//! naming a different copy, carrying the wrong size or tag, or lying past
//! the table. The images and palettes go through the shared codecs and are
//! covered there.

use pack_format::{parse_directory, DirectoryEntry, EntryKind, PackWriter};

use super::{write, RECORD_BYTES, SHINY_TAG, TABLE_ROWS};
use crate::error::ImportError;
use crate::fixture::RomFixture;
use crate::reader::{GbaPtr, ROM_BASE};
use crate::rom::Rom;
use crate::roots::{BattlerRoot, BattlerRoots, Encoding, ImageRoot, PaletteRoot, Roots};

/// The first species index the four tables do not hold.
const FIRST_PAST_THE_TABLES: u16 = 440;

/// The two species the fixture carries.
const FIRST: u16 = 3;
const SECOND: u16 = 7;

/// ROM offsets of the four tables, each 440 rows of eight bytes.
const FRONT_TABLE: u32 = 0x1_0000;
const BACK_TABLE: u32 = 0x2_0000;
const PALETTE_TABLE: u32 = 0x3_0000;
const SHINY_TABLE: u32 = 0x4_0000;
/// Where each species' four assets sit: `ART + species * 0x400`, then
/// front, back, normal, shiny at `0x100` steps.
const ART: u32 = 0x5_0000;

/// A ROM offset as the fixture's index.
fn idx(off: u32) -> usize {
    usize::try_from(off).expect("a ROM offset fits in usize")
}

/// [`RECORD_BYTES`] as an address delta.
const RECORD_BYTES_U32: u32 = 8;

const fn at(off: u32) -> GbaPtr {
    GbaPtr::at(ROM_BASE + off)
}

fn art_offset(species: u16, slot: u32) -> u32 {
    ART + u32::from(species) * 0x400 + slot * 0x100
}

/// An LZ77 type `0x10` stream of `payload`, all literals.
fn lz77_literals(payload: &[u8]) -> Vec<u8> {
    let len = u32::try_from(payload.len()).expect("a test payload fits in 24 bits");
    let size = len.to_le_bytes();
    let mut stream = vec![0x10, size[0], size[1], size[2]];
    for chunk in payload.chunks(8) {
        stream.push(0x00);
        stream.extend_from_slice(chunk);
    }
    stream
}

fn picture_row(data: GbaPtr, size: u16, tag: u16) -> Vec<u8> {
    let mut row = data.raw().to_le_bytes().to_vec();
    row.extend_from_slice(&size.to_le_bytes());
    row.extend_from_slice(&tag.to_le_bytes());
    row
}

fn palette_row(data: GbaPtr, tag: u16) -> Vec<u8> {
    let mut row = data.raw().to_le_bytes().to_vec();
    row.extend_from_slice(&tag.to_le_bytes());
    row.extend_from_slice(&[0, 0]);
    row
}

/// One tile with a distinct byte per species so a swapped read is visible.
fn tile(species: u16) -> Vec<u8> {
    vec![u8::try_from(species).unwrap() | 0x10; 32]
}

/// A 16-colour palette whose first colour is `first`.
fn colors(first: u16) -> Vec<u8> {
    let mut bank = vec![0u8; 32];
    bank[..2].copy_from_slice(&first.to_le_bytes());
    bank
}

fn first_color(species: u16, shiny: bool) -> u16 {
    species * 2 + u16::from(shiny) + 1
}

fn base_fixture() -> RomFixture {
    let mut rom = RomFixture::new().emerald_header();
    for species in [FIRST, SECOND] {
        let row = u32::from(species) * RECORD_BYTES_U32;
        rom = rom
            .write(
                idx(FRONT_TABLE + row),
                &picture_row(at(art_offset(species, 0)), 0x800, species),
            )
            .write(
                idx(BACK_TABLE + row),
                &picture_row(at(art_offset(species, 1)), 0x800, species),
            )
            .write(
                idx(PALETTE_TABLE + row),
                &palette_row(at(art_offset(species, 2)), species),
            )
            .write(
                idx(SHINY_TABLE + row),
                &palette_row(at(art_offset(species, 3)), species + SHINY_TAG),
            )
            .write(idx(art_offset(species, 0)), &lz77_literals(&tile(species)))
            .write(
                idx(art_offset(species, 1)),
                &lz77_literals(&tile(species + 100)),
            )
            .write(
                idx(art_offset(species, 2)),
                &lz77_literals(&colors(first_color(species, false))),
            )
            .write(
                idx(art_offset(species, 3)),
                &lz77_literals(&colors(first_color(species, true))),
            );
    }
    rom
}

fn rom() -> Rom {
    Rom::from_bytes(base_fixture().finish()).expect("the fixture header is valid")
}

const fn image(id: &'static str, species: u16, slot: u32) -> ImageRoot {
    ImageRoot {
        id,
        addr: at(ART + species as u32 * 0x400 + slot * 0x100),
        encoding: Encoding::Lz77,
        rom_bit_depth: 4,
        pack_bit_depth: 4,
        width: 64,
        height: 64,
        metatile_width: 1,
        metatile_height: 1,
        tile_count: 1,
    }
}

const fn palette(id: &'static str, species: u16, slot: u32) -> PaletteRoot {
    PaletteRoot {
        id,
        addr: at(ART + species as u32 * 0x400 + slot * 0x100),
        encoding: Encoding::Lz77,
        color_count: 16,
    }
}

static SPECIES: [BattlerRoot; 2] = [
    BattlerRoot {
        species: FIRST,
        front: image("pokemon/first/front", FIRST, 0),
        back: image("pokemon/first/back", FIRST, 1),
        normal: palette("pokemon/first/palette/normal", FIRST, 2),
        shiny: palette("pokemon/first/palette/shiny", FIRST, 3),
    },
    BattlerRoot {
        species: SECOND,
        front: image("pokemon/second/front", SECOND, 0),
        back: image("pokemon/second/back", SECOND, 1),
        normal: palette("pokemon/second/palette/normal", SECOND, 2),
        shiny: palette("pokemon/second/palette/shiny", SECOND, 3),
    },
];

fn roots(species: &'static [BattlerRoot]) -> Roots {
    Roots {
        battlers: BattlerRoots {
            front_table: at(FRONT_TABLE),
            back_table: at(BACK_TABLE),
            palette_table: at(PALETTE_TABLE),
            shiny_palette_table: at(SHINY_TABLE),
            species,
        },
        ..Roots::NONE
    }
}

fn run_on(rom: &Rom, roots: &Roots) -> Result<(Vec<u8>, Vec<DirectoryEntry>), ImportError> {
    let mut writer = PackWriter::new();
    write(rom, roots, &mut writer)?;
    let pack = writer.finish()?;
    let entries = parse_directory(&pack).expect("a readable pack");
    Ok((pack, entries))
}

fn run(rom: &Rom) -> Result<(Vec<u8>, Vec<DirectoryEntry>), ImportError> {
    run_on(rom, &roots(&SPECIES))
}

fn broken(edit: impl FnOnce(RomFixture) -> RomFixture) -> Rom {
    Rom::from_bytes(edit(base_fixture()).finish()).expect("the fixture header is valid")
}

fn mismatch(rom: &Rom, root: &str, field: &str) {
    let err = run(rom).unwrap_err();
    assert!(
        matches!(&err, ImportError::StructMismatch { root: r, field: f } if *r == root && *f == field),
        "{err}"
    );
}

#[test]
fn every_species_writes_two_images_and_two_palettes() {
    let (pack, entries) = run(&rom()).expect("well-formed tables");
    let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "pokemon/first/back",
            "pokemon/first/front",
            "pokemon/first/palette/normal",
            "pokemon/first/palette/shiny",
            "pokemon/second/back",
            "pokemon/second/front",
            "pokemon/second/palette/normal",
            "pokemon/second/palette/shiny",
        ]
    );
    for entry in &entries {
        match entry.kind {
            EntryKind::Palette { color_count } => assert_eq!(color_count, 16),
            EntryKind::Image { width, height, .. } => assert_eq!((width, height), (64, 64)),
            EntryKind::Raw => panic!("unexpected kind for {}", entry.id),
        }
    }
    let payload = |id: &str| {
        let entry = entries.iter().find(|e| e.id == id).unwrap();
        pack[entry.offset..entry.offset + entry.length].to_vec()
    };
    // The decompressed colours, not the stream, reach the pack.
    let shiny = payload("pokemon/second/palette/shiny");
    assert_eq!(
        u16::from_le_bytes([shiny[0], shiny[1]]),
        first_color(SECOND, true)
    );
    let normal = payload("pokemon/first/palette/normal");
    assert_eq!(
        u16::from_le_bytes([normal[0], normal[1]]),
        first_color(FIRST, false)
    );
}

#[test]
fn a_profile_with_no_battlers_reads_no_tables() {
    let mut roots = roots(&[]);
    roots.battlers.front_table = GbaPtr::at(ROM_BASE + 0x00FF_FFF0);
    let (_, entries) = run_on(&rom(), &roots).expect("nothing to corroborate");
    assert_eq!(entries.len(), 0);
}

#[test]
fn a_row_naming_another_copy_is_refused_in_each_table() {
    let stray = at(0x6_0000);
    let row = u32::from(FIRST) * RECORD_BYTES_U32;
    mismatch(
        &broken(|f| f.write(idx(FRONT_TABLE + row), &picture_row(stray, 0x800, FIRST))),
        "pokemon/first/front",
        "CompressedSpriteSheet.data",
    );
    mismatch(
        &broken(|f| f.write(idx(BACK_TABLE + row), &picture_row(stray, 0x800, FIRST))),
        "pokemon/first/back",
        "CompressedSpriteSheet.data",
    );
    mismatch(
        &broken(|f| f.write(idx(PALETTE_TABLE + row), &palette_row(stray, FIRST))),
        "pokemon/first/palette/normal",
        "CompressedSpritePalette.data",
    );
    mismatch(
        &broken(|f| {
            f.write(
                idx(SHINY_TABLE + row),
                &palette_row(stray, FIRST + SHINY_TAG),
            )
        }),
        "pokemon/first/palette/shiny",
        "CompressedSpritePalette.data",
    );
}

#[test]
fn a_row_with_the_wrong_size_or_tag_is_refused() {
    let row = u32::from(SECOND) * RECORD_BYTES_U32;
    let front = at(art_offset(SECOND, 0));
    mismatch(
        &broken(|f| f.write(idx(FRONT_TABLE + row), &picture_row(front, 0x400, SECOND))),
        "pokemon/second/front",
        "CompressedSpriteSheet.size",
    );
    mismatch(
        &broken(|f| f.write(idx(FRONT_TABLE + row), &picture_row(front, 0x800, FIRST))),
        "pokemon/second/front",
        "CompressedSpriteSheet.tag",
    );
    let normal = at(art_offset(SECOND, 2));
    mismatch(
        &broken(|f| f.write(idx(PALETTE_TABLE + row), &palette_row(normal, FIRST))),
        "pokemon/second/palette/normal",
        "CompressedSpritePalette.tag",
    );
    // A shiny row carrying the plain tag is the normal palette's row.
    let shiny = at(art_offset(SECOND, 3));
    mismatch(
        &broken(|f| f.write(idx(SHINY_TABLE + row), &palette_row(shiny, SECOND))),
        "pokemon/second/palette/shiny",
        "CompressedSpritePalette.tag",
    );
}

#[test]
fn a_palette_stream_of_the_wrong_length_is_refused() {
    let rom = broken(|f| f.write(idx(art_offset(FIRST, 2)), &lz77_literals(&colors(1)[..30])));
    let err = run(&rom).unwrap_err();
    assert!(matches!(err, ImportError::Lz77 { .. }), "{err}");
}

#[test]
fn a_species_past_the_end_of_a_table_is_out_of_range() {
    static PAST: [BattlerRoot; 1] = [BattlerRoot {
        species: FIRST_PAST_THE_TABLES,
        ..SPECIES[0]
    }];
    assert_eq!(usize::from(FIRST_PAST_THE_TABLES), TABLE_ROWS);
    let err = run_on(&rom(), &roots(&PAST)).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::IndexOutOfRange {
                table: "gMonStillFrontPicTable",
                ..
            }
        ),
        "{err}"
    );
}

#[test]
fn a_failing_species_leaves_nothing_queued() {
    let rom = broken(|f| {
        f.write(
            idx(SHINY_TABLE + u32::from(SECOND) * RECORD_BYTES_U32),
            &palette_row(at(art_offset(SECOND, 3)), 0),
        )
    });
    let mut writer = PackWriter::new();
    assert!(write(&rom, &roots(&SPECIES), &mut writer).is_err());
    assert_eq!(writer.len(), 0);
}

#[test]
fn the_roots_expose_every_battler_image_and_palette() {
    let roots = roots(&SPECIES);
    assert_eq!(roots.images().count(), 4);
    assert_eq!(roots.palettes().count(), 4);
}

#[test]
fn a_record_is_eight_bytes() {
    assert_eq!(picture_row(at(0), 0, 0).len(), RECORD_BYTES);
    assert_eq!(palette_row(at(0), 0).len(), RECORD_BYTES);
}
