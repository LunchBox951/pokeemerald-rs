//! Poké Ball reader tests over a synthetic ROM.
//!
//! The reader's own logic is the corroboration of the `BALL_POKE` row in the
//! two tables, so the cases are a well-formed ROM and, per table, a row naming
//! a different copy, carrying the wrong size or tag, or lying past the table.
//! The images and palette go through the shared codecs and are covered there.

use pack_format::{parse_directory, DirectoryEntry, EntryKind, PackWriter};

use super::{write, POKE_BALL_TAG, SHEET_BYTES};
use crate::domains::DOMAINS;
use crate::error::{ImportError, Lz77Fault};
use crate::fixture::RomFixture;
use crate::reader::{GbaPtr, ROM_BASE};
use crate::rom::{Rom, ROM_SIZE};
use crate::roots::{Encoding, ImageRoot, PaletteRoot, PokeBallRoot, PokeBallRoots, Roots};

const SHEET_TABLE: u32 = 0x1_0000;
const PALETTE_TABLE: u32 = 0x2_0000;
const SHEET: u32 = 0x3_0000;
const PALETTE: u32 = 0x4_0000;
const OPEN_SHEET: u32 = 0x5_0000;
/// A second, unreferenced copy of the sheet.
const STRAY_SHEET: u32 = 0x6_0000;

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

fn sheet_tiles() -> Vec<u8> {
    vec![0x21; 384]
}

fn open_tiles() -> Vec<u8> {
    vec![0x43; 128]
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
            idx(SHEET_TABLE),
            &sheet_row(at(SHEET), SHEET_BYTES, POKE_BALL_TAG),
        )
        .write(idx(PALETTE_TABLE), &palette_row(at(PALETTE), POKE_BALL_TAG))
        .write(idx(SHEET), &lz77_literals(&sheet_tiles()))
        .write(idx(STRAY_SHEET), &lz77_literals(&sheet_tiles()))
        .write(idx(PALETTE), &lz77_literals(&colors()))
        .write(idx(OPEN_SHEET), &lz77_literals(&open_tiles()))
}

fn rom_of(fixture: RomFixture) -> Rom {
    Rom::from_bytes(fixture.finish()).expect("the fixture header is valid")
}

const fn image(id: &'static str, addr: GbaPtr, height: u32, tile_count: u32) -> ImageRoot {
    ImageRoot {
        id,
        addr,
        encoding: Encoding::Lz77,
        rom_bit_depth: 4,
        pack_bit_depth: 4,
        width: 16,
        height,
        metatile_width: 1,
        metatile_height: 1,
        tile_count,
    }
}

const BALL: PokeBallRoot = PokeBallRoot {
    sheet: image("poke-ball/image/poke", at(SHEET), 48, 12),
    palette: PaletteRoot {
        id: "poke-ball/palette/poke",
        addr: at(PALETTE),
        encoding: Encoding::Lz77,
        color_count: 16,
    },
    open_sheet: image("poke-ball/image/open", at(OPEN_SHEET), 16, 4),
};

fn roots_of(ball: PokeBallRoot) -> Roots {
    Roots {
        poke_ball: PokeBallRoots {
            sheet_table: at(SHEET_TABLE),
            palette_table: at(PALETTE_TABLE),
            poke_ball: Some(ball),
        },
        ..Roots::NONE
    }
}

fn written(rom: &Rom, roots: &Roots) -> Result<Vec<DirectoryEntry>, ImportError> {
    let mut writer = PackWriter::new();
    write(rom, roots, &mut writer)?;
    let bytes = writer.finish().expect("no duplicate ids");
    Ok(parse_directory(&bytes).expect("a valid pack"))
}

fn assert_mismatch(result: Result<Vec<DirectoryEntry>, ImportError>, expected_field: &str) {
    match result {
        Err(ImportError::StructMismatch { field, .. }) => assert_eq!(field, expected_field),
        other => panic!("expected a {expected_field} mismatch, got {other:?}"),
    }
}

#[test]
fn the_record_emits_its_sheet_palette_and_open_sheet() {
    let entries = written(&rom_of(fixture()), &roots_of(BALL)).expect("a well-formed record");
    let ids: Vec<&str> = entries.iter().map(|entry| entry.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "poke-ball/image/open",
            "poke-ball/image/poke",
            "poke-ball/palette/poke"
        ]
    );
    assert_eq!(
        entries[0].kind,
        EntryKind::Image {
            width: 16,
            height: 16,
            bit_depth: 4
        }
    );
    assert_eq!(
        entries[1].kind,
        EntryKind::Image {
            width: 16,
            height: 48,
            bit_depth: 4
        }
    );
    assert_eq!(entries[2].kind, EntryKind::Palette { color_count: 16 });
}

#[test]
fn the_domain_list_dispatches_the_poke_ball() {
    let rom = rom_of(fixture());
    let roots = roots_of(BALL);
    let mut writer = PackWriter::new();
    for domain in DOMAINS {
        domain(&rom, &roots, &mut writer).expect("every root is well-formed");
    }
    let bytes = writer.finish().expect("no duplicate ids");
    assert_eq!(parse_directory(&bytes).expect("a valid pack").len(), 3);
}

#[test]
fn a_profile_without_the_record_writes_nothing() {
    assert!(written(&rom_of(fixture()), &Roots::NONE)
        .expect("nothing to read")
        .is_empty());
    assert_eq!(Roots::NONE.poke_ball, PokeBallRoots::NONE);
}

#[test]
fn the_record_is_enumerated_as_two_images_and_one_palette() {
    let roots = roots_of(BALL);
    let images: Vec<&str> = roots.images().map(|root| root.id).collect();
    let palettes: Vec<&str> = roots.palettes().map(|root| root.id).collect();
    assert_eq!(images, ["poke-ball/image/poke", "poke-ball/image/open"]);
    assert_eq!(palettes, ["poke-ball/palette/poke"]);
}

#[test]
fn a_sheet_row_that_names_another_copy_is_a_mismatch() {
    let ball = PokeBallRoot {
        sheet: image("poke-ball/image/poke", at(STRAY_SHEET), 48, 12),
        ..BALL
    };
    assert_mismatch(
        written(&rom_of(fixture()), &roots_of(ball)),
        "CompressedSpriteSheet.data",
    );
}

#[test]
fn a_sheet_row_with_the_wrong_size_or_tag_is_a_mismatch() {
    let wrong_size = fixture().write(
        idx(SHEET_TABLE),
        &sheet_row(at(SHEET), SHEET_BYTES + 32, POKE_BALL_TAG),
    );
    assert_mismatch(
        written(&rom_of(wrong_size), &roots_of(BALL)),
        "CompressedSpriteSheet.size",
    );
    let wrong_tag = fixture().write(
        idx(SHEET_TABLE),
        &sheet_row(at(SHEET), SHEET_BYTES, POKE_BALL_TAG + 1),
    );
    assert_mismatch(
        written(&rom_of(wrong_tag), &roots_of(BALL)),
        "CompressedSpriteSheet.tag",
    );
}

#[test]
fn a_palette_row_that_names_another_copy_or_tag_is_a_mismatch() {
    let ball = PokeBallRoot {
        palette: PaletteRoot {
            addr: at(OPEN_SHEET),
            ..BALL.palette
        },
        ..BALL
    };
    assert_mismatch(
        written(&rom_of(fixture()), &roots_of(ball)),
        "CompressedSpritePalette.data",
    );
    let wrong_tag = fixture().write(
        idx(PALETTE_TABLE),
        &palette_row(at(PALETTE), POKE_BALL_TAG + 1),
    );
    assert_mismatch(
        written(&rom_of(wrong_tag), &roots_of(BALL)),
        "CompressedSpritePalette.tag",
    );
}

#[test]
fn a_table_past_the_rom_fails_closed() {
    let mut roots = roots_of(BALL);
    let past_the_end = u32::try_from(ROM_SIZE).expect("the ROM fits a bus address") - 4;
    roots.poke_ball.sheet_table = at(past_the_end);
    let err = written(&rom_of(fixture()), &roots).unwrap_err();
    assert!(matches!(err, ImportError::Truncated { .. }), "{err}");
}

#[test]
fn a_malformed_stream_is_a_typed_error_and_queues_nothing() {
    // The opened sheet's root points at the palette table, which is no LZ77 stream.
    let ball = PokeBallRoot {
        open_sheet: image("poke-ball/image/open", at(PALETTE_TABLE), 16, 4),
        ..BALL
    };
    let rom = rom_of(fixture());
    let roots = roots_of(ball);
    let mut writer = PackWriter::new();
    let err = write(&rom, &roots, &mut writer).unwrap_err();
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
    let bytes = writer.finish().expect("an empty pack");
    assert!(parse_directory(&bytes).expect("a valid pack").is_empty());
}
