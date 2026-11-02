//! Trainer front picture reader tests over a synthetic ROM.
//!
//! The reader's own logic is the corroboration of each picture's row in the
//! two tables, so the cases are a well-formed ROM and, per table, a row
//! naming a different copy, carrying the wrong size or tag, or lying past
//! the table. The compressed image and palette payloads go through the
//! shared codecs; the cases here pin that their failures reach this domain.

use pack_format::{parse_directory, DirectoryEntry, EntryKind, PackWriter};

use super::{write, RECORD_BYTES, TABLE_ROWS};
use crate::error::{ImportError, Lz77Fault};
use crate::fixture::RomFixture;
use crate::reader::{GbaPtr, ROM_BASE};
use crate::rom::{Rom, ROM_SIZE};
use crate::roots::{Encoding, ImageRoot, PaletteRoot, Roots, TrainerFrontRoot, TrainerFrontRoots};

const BRENDAN: u16 = 71;
const MAY: u16 = 72;

/// ROM offsets of the two tables, each 93 rows of eight bytes.
const FRONT_TABLE: u32 = 0x1_0000;
const PALETTE_TABLE: u32 = 0x2_0000;
/// Where each picture's image and palette sit.
const BRENDAN_IMAGE: u32 = 0x3_0000;
const BRENDAN_PALETTE: u32 = 0x3_1000;
const MAY_IMAGE: u32 = 0x3_2000;
const MAY_PALETTE: u32 = 0x3_3000;
/// Leaves room for an LZ77 header and one flag byte, and nothing after.
const FIVE_BYTES_FROM_THE_END: u32 = 0x00FF_FFFB;
/// A valid stream no table row names.
const STRAY: u32 = 0x4_0000;

/// A 64x64 4bpp picture: 64 tiles of 32 bytes.
const IMAGE_BYTES: usize = 2048;
const PALETTE_BYTES: usize = 32;

const fn idx(off: u32) -> usize {
    off as usize
}

const fn at(off: u32) -> GbaPtr {
    GbaPtr::at(ROM_BASE + off)
}

/// [`RECORD_BYTES`] as an address delta.
const RECORD_BYTES_U32: u32 = 8;

const fn row_start(table: u32, trainer_pic: u16) -> u32 {
    table + trainer_pic as u32 * RECORD_BYTES_U32
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

/// 64 tiles whose every byte is `byte`: a picture of alternating pixels
/// `byte & 0xF`, `byte >> 4`.
fn tiles(byte: u8) -> Vec<u8> {
    vec![byte; IMAGE_BYTES]
}

/// Sixteen colours counting up from `first`.
fn colors(first: u16) -> Vec<u8> {
    (0..16u16)
        .flat_map(|step| (first + step).to_le_bytes())
        .collect()
}

fn base_fixture() -> RomFixture {
    RomFixture::new()
        .emerald_header()
        .write(
            idx(row_start(FRONT_TABLE, BRENDAN)),
            &picture_row(at(BRENDAN_IMAGE), 0x800, BRENDAN),
        )
        .write(
            idx(row_start(FRONT_TABLE, MAY)),
            &picture_row(at(MAY_IMAGE), 0x800, MAY),
        )
        .write(
            idx(row_start(PALETTE_TABLE, BRENDAN)),
            &palette_row(at(BRENDAN_PALETTE), BRENDAN),
        )
        .write(
            idx(row_start(PALETTE_TABLE, MAY)),
            &palette_row(at(MAY_PALETTE), MAY),
        )
        .write(idx(BRENDAN_IMAGE), &lz77_literals(&tiles(0x21)))
        .write(idx(BRENDAN_PALETTE), &lz77_literals(&colors(0x0100)))
        .write(idx(MAY_IMAGE), &lz77_literals(&tiles(0x43)))
        .write(idx(MAY_PALETTE), &lz77_literals(&colors(0x0200)))
}

fn rom_from(fixture: RomFixture) -> Rom {
    Rom::from_bytes(fixture.finish()).expect("the fixture header is valid")
}

fn rom() -> Rom {
    rom_from(base_fixture())
}

fn broken(edit: impl FnOnce(RomFixture) -> RomFixture) -> Rom {
    rom_from(edit(base_fixture()))
}

const fn image(id: &'static str, addr: u32) -> ImageRoot {
    ImageRoot {
        id,
        addr: at(addr),
        encoding: Encoding::Lz77,
        rom_bit_depth: 4,
        pack_bit_depth: 4,
        width: 64,
        height: 64,
        metatile_width: 1,
        metatile_height: 1,
        tile_count: 64,
    }
}

const fn palette(id: &'static str, addr: u32) -> PaletteRoot {
    PaletteRoot {
        id,
        addr: at(addr),
        encoding: Encoding::Lz77,
        color_count: 16,
    }
}

const BRENDAN_ROOT: TrainerFrontRoot = TrainerFrontRoot {
    trainer_pic: BRENDAN,
    front: image("trainer-pic/brendan/front", BRENDAN_IMAGE),
    palette: palette("trainer-pic/brendan/palette", BRENDAN_PALETTE),
};

const MAY_ROOT: TrainerFrontRoot = TrainerFrontRoot {
    trainer_pic: MAY,
    front: image("trainer-pic/may/front", MAY_IMAGE),
    palette: palette("trainer-pic/may/palette", MAY_PALETTE),
};

static TRAINERS: [TrainerFrontRoot; 2] = [BRENDAN_ROOT, MAY_ROOT];
/// Brendan's image where only a stream header can fit.
static IMAGE_AT_THE_END: [TrainerFrontRoot; 1] = [TrainerFrontRoot {
    front: ImageRoot {
        addr: at(FIVE_BYTES_FROM_THE_END),
        ..BRENDAN_ROOT.front
    },
    ..BRENDAN_ROOT
}];
/// Brendan's palette where only a stream header can fit.
static PALETTE_AT_THE_END: [TrainerFrontRoot; 1] = [TrainerFrontRoot {
    palette: PaletteRoot {
        addr: at(FIVE_BYTES_FROM_THE_END),
        ..BRENDAN_ROOT.palette
    },
    ..BRENDAN_ROOT
}];

fn roots_at(
    front_table: GbaPtr,
    palette_table: GbaPtr,
    trainers: &'static [TrainerFrontRoot],
) -> Roots {
    Roots {
        trainer_front: TrainerFrontRoots {
            front_table,
            palette_table,
            trainers,
        },
        ..Roots::NONE
    }
}

fn roots(trainers: &'static [TrainerFrontRoot]) -> Roots {
    roots_at(at(FRONT_TABLE), at(PALETTE_TABLE), trainers)
}

fn run_on(rom: &Rom, roots: &Roots) -> Result<(Vec<u8>, Vec<DirectoryEntry>), ImportError> {
    let mut writer = PackWriter::new();
    write(rom, roots, &mut writer)?;
    let pack = writer.finish()?;
    let entries = parse_directory(&pack).expect("a readable pack");
    Ok((pack, entries))
}

fn run(rom: &Rom) -> Result<(Vec<u8>, Vec<DirectoryEntry>), ImportError> {
    run_on(rom, &roots(&TRAINERS))
}

fn mismatch(rom: &Rom, root: &str, field: &str) {
    let err = run(rom).unwrap_err();
    assert!(
        matches!(&err, ImportError::StructMismatch { root: r, field: f } if *r == root && *f == field),
        "{err}"
    );
}

fn payload<'a>(pack: &'a [u8], entries: &[DirectoryEntry], id: &str) -> &'a [u8] {
    let entry = entries.iter().find(|e| e.id == id).expect("an entry");
    &pack[entry.offset..entry.offset + entry.length]
}

#[test]
fn both_pictures_write_an_image_and_a_palette_with_their_decompressed_payloads() {
    let (pack, entries) = run(&rom()).expect("well-formed tables");
    let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "trainer-pic/brendan/front",
            "trainer-pic/brendan/palette",
            "trainer-pic/may/front",
            "trainer-pic/may/palette",
        ]
    );
    for entry in &entries {
        match entry.kind {
            EntryKind::Palette { color_count } => assert_eq!(color_count, 16),
            EntryKind::Image {
                width,
                height,
                bit_depth,
            } => assert_eq!((width, height, bit_depth), (64, 64, 4)),
            EntryKind::Raw => panic!("unexpected kind for {}", entry.id),
        }
    }
    let alternating = |low: u8, high: u8| [low, high].repeat(64 * 64 / 2);
    assert_eq!(
        payload(&pack, &entries, "trainer-pic/brendan/front"),
        alternating(1, 2)
    );
    assert_eq!(
        payload(&pack, &entries, "trainer-pic/may/front"),
        alternating(3, 4)
    );
    assert_eq!(
        payload(&pack, &entries, "trainer-pic/brendan/palette"),
        colors(0x0100)
    );
    assert_eq!(
        payload(&pack, &entries, "trainer-pic/may/palette"),
        colors(0x0200)
    );
}

#[test]
fn a_profile_with_no_trainer_pictures_reads_no_tables() {
    let unusable = GbaPtr::at(ROM_BASE + 0x00FF_FFF0);
    let (_, entries) =
        run_on(&rom(), &roots_at(unusable, unusable, &[])).expect("nothing to corroborate");
    assert!(entries.is_empty());
}

#[test]
fn a_row_naming_another_copy_is_refused_in_each_table() {
    let stray = at(STRAY);
    mismatch(
        &broken(|f| {
            f.write(
                idx(row_start(FRONT_TABLE, BRENDAN)),
                &picture_row(stray, 0x800, BRENDAN),
            )
        }),
        "trainer-pic/brendan/front",
        "CompressedSpriteSheet.data",
    );
    mismatch(
        &broken(|f| f.write(idx(row_start(PALETTE_TABLE, MAY)), &palette_row(stray, MAY))),
        "trainer-pic/may/palette",
        "CompressedSpritePalette.data",
    );
}

#[test]
fn a_row_pointing_outside_the_cartridge_is_a_pointer_fault_not_a_mismatch() {
    let outside_the_cartridge = 0x0200_0000_u32.to_le_bytes();
    let rom = broken(|f| f.write(idx(row_start(FRONT_TABLE, BRENDAN)), &outside_the_cartridge));
    let err = run(&rom).unwrap_err();
    assert!(
        matches!(err, ImportError::PointerOutOfRange { .. }),
        "{err}"
    );
}

#[test]
fn a_row_with_the_wrong_size_or_tag_is_refused() {
    mismatch(
        &broken(|f| {
            f.write(
                idx(row_start(FRONT_TABLE, MAY)),
                &picture_row(at(MAY_IMAGE), 0x400, MAY),
            )
        }),
        "trainer-pic/may/front",
        "CompressedSpriteSheet.size",
    );
    mismatch(
        &broken(|f| {
            f.write(
                idx(row_start(FRONT_TABLE, MAY)),
                &picture_row(at(MAY_IMAGE), 0x800, BRENDAN),
            )
        }),
        "trainer-pic/may/front",
        "CompressedSpriteSheet.tag",
    );
    mismatch(
        &broken(|f| {
            f.write(
                idx(row_start(PALETTE_TABLE, MAY)),
                &palette_row(at(MAY_PALETTE), BRENDAN),
            )
        }),
        "trainer-pic/may/palette",
        "CompressedSpritePalette.tag",
    );
}

#[test]
fn a_table_row_past_the_end_of_the_rom_is_truncated() {
    let last_word = ROM_SIZE - 4;
    let base_for_row_at_the_end = |trainer_pic: u16| {
        GbaPtr::at(ROM_BASE + u32::try_from(last_word).unwrap() - row_start(0, trainer_pic))
    };
    let err = run_on(
        &rom(),
        &roots_at(
            base_for_row_at_the_end(BRENDAN),
            at(PALETTE_TABLE),
            &TRAINERS,
        ),
    )
    .unwrap_err();
    assert!(
        matches!(err, ImportError::Truncated { at, len } if at == last_word && len == RECORD_BYTES),
        "{err}"
    );
    let err = run_on(
        &rom(),
        &roots_at(at(FRONT_TABLE), base_for_row_at_the_end(BRENDAN), &TRAINERS),
    )
    .unwrap_err();
    assert!(matches!(err, ImportError::Truncated { .. }), "{err}");
}

#[test]
fn a_stream_of_the_wrong_type_is_refused_for_the_image_and_the_palette() {
    for (addr, name) in [(BRENDAN_IMAGE, "image"), (MAY_PALETTE, "palette")] {
        let rom = broken(|f| f.write(idx(addr), &[0x11]));
        let err = run(&rom).unwrap_err();
        assert!(
            matches!(
                err,
                ImportError::Lz77 {
                    fault: Lz77Fault::WrongType,
                    ..
                }
            ),
            "{name}: {err}"
        );
    }
}

#[test]
fn a_back_reference_before_any_output_is_refused() {
    let stream = [0x10, 0x00, 0x08, 0x00, 0x80, 0x00, 0x00];
    let err = run(&broken(|f| f.write(idx(MAY_IMAGE), &stream))).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::Lz77 {
                fault: Lz77Fault::BackReferenceTooFar {
                    displacement: 1,
                    produced: 0
                },
                ..
            }
        ),
        "{err}"
    );
}

#[test]
fn streams_ending_at_the_end_of_the_rom_are_unexpected_ends() {
    // Only the type, the size and one flag byte fit before the ROM ends.
    let header_only = |decoded_len: usize| {
        let size = u32::try_from(decoded_len).unwrap().to_le_bytes();
        [0x10, size[0], size[1], size[2], 0x00]
    };
    let image_rom = broken(|f| {
        f.write(
            idx(row_start(FRONT_TABLE, BRENDAN)),
            &picture_row(at(FIVE_BYTES_FROM_THE_END), 0x800, BRENDAN),
        )
        .write(idx(FIVE_BYTES_FROM_THE_END), &header_only(IMAGE_BYTES))
    });
    let palette_rom = broken(|f| {
        f.write(
            idx(row_start(PALETTE_TABLE, BRENDAN)),
            &palette_row(at(FIVE_BYTES_FROM_THE_END), BRENDAN),
        )
        .write(idx(FIVE_BYTES_FROM_THE_END), &header_only(PALETTE_BYTES))
    });
    for (rom, trainers) in [
        (&image_rom, &IMAGE_AT_THE_END),
        (&palette_rom, &PALETTE_AT_THE_END),
    ] {
        let err = run_on(rom, &roots(trainers)).unwrap_err();
        assert!(
            matches!(
                err,
                ImportError::Lz77 {
                    fault: Lz77Fault::UnexpectedEnd,
                    ..
                }
            ),
            "{err}"
        );
    }
}

#[test]
fn a_stream_decoding_to_the_wrong_length_is_refused() {
    let sized = |at_offset: u32, bytes: Vec<u8>| {
        broken(|f| f.write(idx(at_offset), &lz77_literals(&bytes)))
    };
    for (rom, expected, actual) in [
        (
            sized(BRENDAN_IMAGE, vec![0; IMAGE_BYTES - 1]),
            IMAGE_BYTES,
            IMAGE_BYTES - 1,
        ),
        (
            sized(MAY_IMAGE, vec![0; IMAGE_BYTES + 1]),
            IMAGE_BYTES,
            IMAGE_BYTES + 1,
        ),
        (
            sized(BRENDAN_PALETTE, vec![0; PALETTE_BYTES - 2]),
            PALETTE_BYTES,
            PALETTE_BYTES - 2,
        ),
        (
            sized(MAY_PALETTE, vec![0; PALETTE_BYTES + 2]),
            PALETTE_BYTES,
            PALETTE_BYTES + 2,
        ),
    ] {
        let err = run(&rom).unwrap_err();
        assert!(
            matches!(
                err,
                ImportError::Lz77 {
                    fault: Lz77Fault::SizeMismatch { expected: e, actual: a },
                    ..
                } if e == expected && a == actual
            ),
            "{err}"
        );
    }
}

#[test]
fn a_picture_past_the_end_of_a_table_is_out_of_range() {
    static PAST: [TrainerFrontRoot; 1] = [TrainerFrontRoot {
        trainer_pic: 93,
        ..BRENDAN_ROOT
    }];
    assert_eq!(TABLE_ROWS, 93);
    let err = run_on(&rom(), &roots(&PAST)).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::IndexOutOfRange {
                table: "gTrainerFrontPicTable",
                index: 93,
                count: 93
            }
        ),
        "{err}"
    );
}

#[test]
fn a_failing_picture_leaves_nothing_queued() {
    let rom = broken(|f| f.write(idx(MAY_PALETTE), &[0x11]));
    let mut writer = PackWriter::new();
    assert!(write(&rom, &roots(&TRAINERS), &mut writer).is_err());
    assert_eq!(writer.len(), 0);
}

#[test]
fn the_roots_expose_every_trainer_image_and_palette() {
    let roots = roots(&TRAINERS);
    let images: Vec<&str> = roots.images().map(|root| root.id).collect();
    let palettes: Vec<&str> = roots.palettes().map(|root| root.id).collect();
    assert_eq!(
        images,
        ["trainer-pic/brendan/front", "trainer-pic/may/front"]
    );
    assert_eq!(
        palettes,
        ["trainer-pic/brendan/palette", "trainer-pic/may/palette"]
    );
}
