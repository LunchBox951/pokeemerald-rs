//! Battle-effect reader tests over a synthetic ROM.
//!
//! The reader's own logic is the corroboration of each record's row in the two
//! tables, so the cases are a well-formed ROM and, per record, a row naming a
//! different copy, carrying the wrong size or tag, or lying past the table. The
//! family is all-or-nothing: a malformed stream in the last pair queues no
//! entry for the pairs before it. Images and palettes go through the shared
//! codecs and are covered there.

use pack_format::{parse_directory, DirectoryEntry, EntryKind, PackEntry, PackWriter};

use super::write;
use crate::domains::DOMAINS;
use crate::error::{ImportError, Lz77Fault};
use crate::fixture::RomFixture;
use crate::reader::{GbaPtr, ROM_BASE};
use crate::rom::{Rom, ROM_SIZE};
use crate::roots::{BattleEffectRoot, BattleEffectRoots, Encoding, ImageRoot, PaletteRoot, Roots};

/// `Impact` and `Pokeblock`: one `loadspritegfx` operand, one task-loaded pair.
const IMPACT: u16 = 135;
const POKEBLOCK: u16 = 269;
const SHEET_TABLE: u32 = 0x1_0000;
const PALETTE_TABLE: u32 = 0x2_0000;
const IMPACT_SHEET: u32 = 0x3_0000;
const IMPACT_PALETTE: u32 = 0x3_1000;
const POKEBLOCK_SHEET: u32 = 0x3_2000;
const POKEBLOCK_PALETTE: u32 = 0x3_3000;
/// A second copy of Impact's sheet that no table row names.
const STRAY_SHEET: u32 = 0x3_4000;
/// The registered size of both sheets: one 4bpp tile.
const SHEET_BYTES: u16 = 32;

const fn at(off: u32) -> GbaPtr {
    GbaPtr::at(ROM_BASE + off)
}

fn idx(off: u32) -> usize {
    usize::try_from(off).expect("a ROM offset fits in usize")
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

/// A stream whose first token reaches back before any byte was produced.
fn lz77_back_reference_too_far() -> Vec<u8> {
    vec![0x10, 32, 0, 0, 0x80, 0x00, 0x01]
}

fn sheet_row(data: GbaPtr, size: u16, tag: u16) -> Vec<u8> {
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

fn row(table: u32, sprite: u16) -> usize {
    idx(table) + usize::from(sprite) * 8
}

fn tag(sprite: u16) -> u16 {
    10_000 + sprite
}

fn tiles() -> Vec<u8> {
    vec![0x21; 32]
}

fn colors() -> Vec<u8> {
    let mut bank = vec![0u8; 32];
    bank[..2].copy_from_slice(&0x7FFFu16.to_le_bytes());
    bank
}

fn fixture() -> RomFixture {
    RomFixture::new()
        .emerald_header()
        .write(
            row(SHEET_TABLE, IMPACT),
            &sheet_row(at(IMPACT_SHEET), SHEET_BYTES, tag(IMPACT)),
        )
        .write(
            row(SHEET_TABLE, POKEBLOCK),
            &sheet_row(at(POKEBLOCK_SHEET), SHEET_BYTES, tag(POKEBLOCK)),
        )
        .write(
            row(PALETTE_TABLE, IMPACT),
            &palette_row(at(IMPACT_PALETTE), tag(IMPACT)),
        )
        .write(
            row(PALETTE_TABLE, POKEBLOCK),
            &palette_row(at(POKEBLOCK_PALETTE), tag(POKEBLOCK)),
        )
        .write(idx(IMPACT_SHEET), &lz77_literals(&tiles()))
        .write(idx(STRAY_SHEET), &lz77_literals(&tiles()))
        .write(idx(POKEBLOCK_SHEET), &lz77_literals(&tiles()))
        .write(idx(IMPACT_PALETTE), &lz77_literals(&colors()))
        .write(idx(POKEBLOCK_PALETTE), &lz77_literals(&colors()))
}

fn rom_of(fixture: RomFixture) -> Rom {
    Rom::from_bytes(fixture.finish()).expect("the fixture header is valid")
}

const fn sheet(id: &'static str, addr: GbaPtr) -> ImageRoot {
    ImageRoot {
        id,
        addr,
        encoding: Encoding::Lz77,
        rom_bit_depth: 4,
        pack_bit_depth: 4,
        width: 8,
        height: 8,
        metatile_width: 1,
        metatile_height: 1,
        tile_count: 1,
    }
}

const fn palette(id: &'static str, addr: GbaPtr) -> PaletteRoot {
    PaletteRoot {
        id,
        addr,
        color_count: 16,
        encoding: Encoding::Lz77,
    }
}

const IMPACT_RECORD: BattleEffectRoot = BattleEffectRoot {
    sprite: IMPACT,
    sheet_bytes: SHEET_BYTES,
    sheet: sheet("battle-effect/135/image", at(IMPACT_SHEET)),
    palette: palette("battle-effect/135/palette", at(IMPACT_PALETTE)),
};

const POKEBLOCK_RECORD: BattleEffectRoot = BattleEffectRoot {
    sprite: POKEBLOCK,
    sheet_bytes: SHEET_BYTES,
    sheet: sheet("battle-effect/269/image", at(POKEBLOCK_SHEET)),
    palette: palette("battle-effect/269/palette", at(POKEBLOCK_PALETTE)),
};

fn roots_of(records: &'static [BattleEffectRoot]) -> Roots {
    Roots {
        battle_effects: BattleEffectRoots {
            sheet_table: at(SHEET_TABLE),
            palette_table: at(PALETTE_TABLE),
            records,
        },
        ..Roots::NONE
    }
}

fn both() -> Roots {
    roots_of(&[IMPACT_RECORD, POKEBLOCK_RECORD])
}

fn pack_of(writer: PackWriter) -> (Vec<u8>, Vec<DirectoryEntry>) {
    let bytes = writer.finish().expect("no duplicate ids");
    let entries = parse_directory(&bytes).expect("a valid pack");
    (bytes, entries)
}

fn written(rom: &Rom, roots: &Roots) -> Result<Vec<DirectoryEntry>, ImportError> {
    let mut writer = PackWriter::new();
    write(rom, roots, &mut writer)?;
    Ok(pack_of(writer).1)
}

fn assert_mismatch(result: Result<Vec<DirectoryEntry>, ImportError>, expected_field: &str) {
    match result {
        Err(ImportError::StructMismatch { field, .. }) => assert_eq!(field, expected_field),
        other => panic!("expected a {expected_field} mismatch, got {other:?}"),
    }
}

fn sentinel() -> PackEntry {
    pack_format::raw_entry("sentinel".to_owned(), vec![7])
}

#[test]
fn the_records_emit_exactly_their_sheets_and_palettes() {
    let mut writer = PackWriter::new();
    write(&rom_of(fixture()), &both(), &mut writer).expect("well-formed records");
    let (bytes, entries) = pack_of(writer);
    let ids: Vec<&str> = entries.iter().map(|entry| entry.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "battle-effect/135/image",
            "battle-effect/135/palette",
            "battle-effect/269/image",
            "battle-effect/269/palette",
        ]
    );
    assert_eq!(
        entries[0].kind,
        EntryKind::Image {
            width: 8,
            height: 8,
            bit_depth: 4
        }
    );
    assert_eq!(entries[1].kind, EntryKind::Palette { color_count: 16 });
    let sheet = &bytes[entries[0].offset..entries[0].offset + entries[0].length];
    assert_eq!(&sheet[..4], [1, 2, 1, 2], "low nibble first");
}

#[test]
fn only_the_selected_rows_are_read() {
    let entries = written(&rom_of(fixture()), &roots_of(&[POKEBLOCK_RECORD])).expect("one record");
    let ids: Vec<&str> = entries.iter().map(|entry| entry.id.as_str()).collect();
    assert_eq!(
        ids,
        ["battle-effect/269/image", "battle-effect/269/palette"]
    );
}

#[test]
fn the_domain_list_dispatches_the_battle_effects() {
    let rom = rom_of(fixture());
    let roots = both();
    let mut writer = PackWriter::new();
    for domain in DOMAINS {
        domain(&rom, &roots, &mut writer).expect("every root is well-formed");
    }
    assert_eq!(pack_of(writer).1.len(), 4);
}

#[test]
fn a_profile_without_records_writes_nothing() {
    assert!(written(&rom_of(fixture()), &Roots::NONE)
        .expect("nothing to read")
        .is_empty());
    assert_eq!(Roots::NONE.battle_effects, BattleEffectRoots::NONE);
}

#[test]
fn the_records_are_enumerated_as_images_and_palettes() {
    let roots = both();
    let images: Vec<&str> = roots.images().map(|root| root.id).collect();
    let palettes: Vec<&str> = roots.palettes().map(|root| root.id).collect();
    assert_eq!(
        images,
        ["battle-effect/135/image", "battle-effect/269/image"]
    );
    assert_eq!(
        palettes,
        ["battle-effect/135/palette", "battle-effect/269/palette"]
    );
}

#[test]
fn a_sheet_row_that_names_another_copy_is_a_mismatch() {
    const RECORDS: &[BattleEffectRoot] = &[BattleEffectRoot {
        sheet: sheet("battle-effect/135/image", at(STRAY_SHEET)),
        ..IMPACT_RECORD
    }];
    assert_mismatch(
        written(&rom_of(fixture()), &roots_of(RECORDS)),
        "CompressedSpriteSheet.data",
    );
}

#[test]
fn a_sheet_row_with_the_wrong_size_or_tag_is_a_mismatch() {
    let wrong_size = fixture().write(
        row(SHEET_TABLE, POKEBLOCK),
        &sheet_row(at(POKEBLOCK_SHEET), SHEET_BYTES + 32, tag(POKEBLOCK)),
    );
    assert_mismatch(
        written(&rom_of(wrong_size), &both()),
        "CompressedSpriteSheet.size",
    );
    let wrong_tag = fixture().write(
        row(SHEET_TABLE, IMPACT),
        &sheet_row(at(IMPACT_SHEET), SHEET_BYTES, tag(IMPACT) + 1),
    );
    assert_mismatch(
        written(&rom_of(wrong_tag), &both()),
        "CompressedSpriteSheet.tag",
    );
}

#[test]
fn a_palette_row_that_names_another_copy_or_tag_is_a_mismatch() {
    const RECORDS: &[BattleEffectRoot] = &[BattleEffectRoot {
        palette: palette("battle-effect/135/palette", at(POKEBLOCK_PALETTE)),
        ..IMPACT_RECORD
    }];
    assert_mismatch(
        written(&rom_of(fixture()), &roots_of(RECORDS)),
        "CompressedSpritePalette.data",
    );
    let wrong_tag = fixture().write(
        row(PALETTE_TABLE, POKEBLOCK),
        &palette_row(at(POKEBLOCK_PALETTE), tag(POKEBLOCK) - 1),
    );
    assert_mismatch(
        written(&rom_of(wrong_tag), &both()),
        "CompressedSpritePalette.tag",
    );
}

#[test]
fn a_row_must_be_the_one_the_sprite_index_selects() {
    // Impact's pointers sit in Pokeblock's row: the table is read by index, not searched.
    let swapped = fixture().write(
        row(SHEET_TABLE, POKEBLOCK),
        &sheet_row(at(IMPACT_SHEET), SHEET_BYTES, tag(POKEBLOCK)),
    );
    assert_mismatch(
        written(&rom_of(swapped), &both()),
        "CompressedSpriteSheet.data",
    );
}

#[test]
fn a_truncated_table_fails_closed() {
    let past_the_end = u32::try_from(ROM_SIZE).expect("the ROM fits a bus address") - 4;
    for (sheet_table, palette_table) in [
        (at(past_the_end), at(PALETTE_TABLE)),
        (at(SHEET_TABLE), at(past_the_end)),
    ] {
        let mut roots = both();
        roots.battle_effects.sheet_table = sheet_table;
        roots.battle_effects.palette_table = palette_table;
        let err = written(&rom_of(fixture()), &roots).unwrap_err();
        assert!(matches!(err, ImportError::Truncated { .. }), "{err}");
    }
}

#[test]
fn a_sprite_past_the_tables_is_refused() {
    const RECORDS: &[BattleEffectRoot] = &[BattleEffectRoot {
        sprite: 289,
        ..IMPACT_RECORD
    }];
    let err = written(&rom_of(fixture()), &roots_of(RECORDS)).unwrap_err();
    assert!(
        matches!(err, ImportError::IndexOutOfRange { index: 289, .. }),
        "{err}"
    );
}

#[test]
fn ids_that_do_not_follow_the_sprite_index_are_refused() {
    const WRONG_SHEET: &[BattleEffectRoot] = &[BattleEffectRoot {
        sheet: sheet("battle-effect/269/image", at(IMPACT_SHEET)),
        ..IMPACT_RECORD
    }];
    assert_mismatch(
        written(&rom_of(fixture()), &roots_of(WRONG_SHEET)),
        "BattleEffectRoot.sheet.id",
    );
    const WRONG_PALETTE: &[BattleEffectRoot] = &[BattleEffectRoot {
        palette: palette("battle-effect/135/image", at(IMPACT_PALETTE)),
        ..IMPACT_RECORD
    }];
    assert_mismatch(
        written(&rom_of(fixture()), &roots_of(WRONG_PALETTE)),
        "BattleEffectRoot.palette.id",
    );
}

#[test]
fn a_sprite_recorded_twice_is_refused() {
    assert_mismatch(
        written(
            &rom_of(fixture()),
            &roots_of(&[IMPACT_RECORD, IMPACT_RECORD]),
        ),
        "BattleEffectRoot.sprite",
    );
}

/// The writer after `write` failed on a writer seeded with a sentinel.
fn failed_leaving_only_the_sentinel(rom: &Rom, roots: &Roots) -> ImportError {
    let mut writer = PackWriter::new();
    writer.push(sentinel());
    let err = write(rom, roots, &mut writer).unwrap_err();
    let (_, entries) = pack_of(writer);
    let ids: Vec<&str> = entries.iter().map(|entry| entry.id.as_str()).collect();
    assert_eq!(ids, ["sentinel"], "a failed import queued entries");
    err
}

#[test]
fn a_malformed_stream_in_the_last_pair_queues_nothing() {
    let wrong_type = fixture().write(idx(POKEBLOCK_PALETTE), &[0x11, 32, 0, 0, 0]);
    let err = failed_leaving_only_the_sentinel(&rom_of(wrong_type), &both());
    assert!(
        matches!(
            err,
            ImportError::Lz77 {
                fault: Lz77Fault::WrongType,
                ..
            }
        ),
        "{err}"
    );

    let back_reference = fixture().write(idx(POKEBLOCK_SHEET), &lz77_back_reference_too_far());
    let err = failed_leaving_only_the_sentinel(&rom_of(back_reference), &both());
    assert!(
        matches!(
            err,
            ImportError::Lz77 {
                fault: Lz77Fault::BackReferenceTooFar { .. },
                ..
            }
        ),
        "{err}"
    );
}

#[test]
fn a_row_mismatch_in_the_last_pair_queues_nothing() {
    let wrong_tag = fixture().write(
        row(PALETTE_TABLE, POKEBLOCK),
        &palette_row(at(POKEBLOCK_PALETTE), tag(POKEBLOCK) + 1),
    );
    let err = failed_leaving_only_the_sentinel(&rom_of(wrong_tag), &both());
    assert!(
        matches!(
            err,
            ImportError::StructMismatch {
                field: "CompressedSpritePalette.tag",
                ..
            }
        ),
        "{err}"
    );
}
