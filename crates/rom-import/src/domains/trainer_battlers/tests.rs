//! Trainer battler reader tests over a synthetic ROM.
//!
//! The front half is the portrait reader's, so these cases pin what is new:
//! the back tables are indexed by their own space (0 and 1, not 71 and 72),
//! each back row must hold the address the profile records and the 0x2000
//! size of four frames, and the back palette row must name the very palette
//! stream the trainer's front row names, not another copy of its colours.

use assets::{AssetPack, BattlerDirection, TrainerPicId};
use pack_format::{parse_directory, DirectoryEntry, EntryKind, PackWriter};

use super::{write, BACK_PICTURE_SIZE, BACK_TABLE_ROWS};
use crate::error::ImportError;
use crate::fixture::RomFixture;
use crate::reader::{GbaPtr, ROM_BASE};
use crate::roots::{
    Encoding, ImageRoot, PaletteRoot, Roots, TrainerBackRoot, TrainerBattlerRoots,
    TrainerFrontRoot, TrainerFrontRoots,
};

const BRENDAN_FRONT_PIC: u16 = 71;
const MAY_FRONT_PIC: u16 = 72;
const BRENDAN_BACK_PIC: u16 = 0;
const MAY_BACK_PIC: u16 = 1;

/// ROM offsets of the four tables: front rows are 93 wide, back rows eight.
const FRONT_TABLE: u32 = 0x1_0000;
const FRONT_PALETTE_TABLE: u32 = 0x2_0000;
const BACK_TABLE: u32 = 0x3_0000;
const BACK_PALETTE_TABLE: u32 = 0x4_0000;
/// Where each trainer's art sits.
const BRENDAN_FRONT: u32 = 0x5_0000;
const BRENDAN_BACK: u32 = 0x5_2000;
const BRENDAN_PALETTE: u32 = 0x5_6000;
const MAY_FRONT: u32 = 0x6_0000;
const MAY_BACK: u32 = 0x6_2000;
const MAY_PALETTE: u32 = 0x6_6000;
/// An identical palette stream no row names.
const PALETTE_COPY: u32 = 0x7_0000;
const STRAY: u32 = 0x7_1000;
/// The first index past the eight-row back tables.
const PAST_THE_BACK_TABLE: u16 = 8;

const FRONT_BYTES: usize = 2048;
const FRAME_BYTES: usize = 2048;

const fn idx(off: u32) -> usize {
    off as usize
}

const fn at(off: u32) -> GbaPtr {
    GbaPtr::at(ROM_BASE + off)
}

const fn row_start(table: u32, index: u16) -> u32 {
    table + index as u32 * 8
}

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

/// A front picture of alternating pixels `byte & 0xF`, `byte >> 4`.
fn front_tiles(byte: u8) -> Vec<u8> {
    vec![byte; FRONT_BYTES]
}

/// A four-frame back sheet whose frame `n` holds byte `first + n`.
fn back_tiles(first: u8) -> Vec<u8> {
    (0..4u8)
        .flat_map(|frame| vec![first + frame; FRAME_BYTES])
        .collect()
}

fn colors(first: u16) -> Vec<u8> {
    (0..16u16)
        .flat_map(|step| (first + step).to_le_bytes())
        .collect()
}

fn base_fixture() -> RomFixture {
    RomFixture::new()
        .emerald_header()
        .write(
            idx(row_start(FRONT_TABLE, BRENDAN_FRONT_PIC)),
            &picture_row(at(BRENDAN_FRONT), 0x800, BRENDAN_FRONT_PIC),
        )
        .write(
            idx(row_start(FRONT_TABLE, MAY_FRONT_PIC)),
            &picture_row(at(MAY_FRONT), 0x800, MAY_FRONT_PIC),
        )
        .write(
            idx(row_start(FRONT_PALETTE_TABLE, BRENDAN_FRONT_PIC)),
            &palette_row(at(BRENDAN_PALETTE), BRENDAN_FRONT_PIC),
        )
        .write(
            idx(row_start(FRONT_PALETTE_TABLE, MAY_FRONT_PIC)),
            &palette_row(at(MAY_PALETTE), MAY_FRONT_PIC),
        )
        .write(
            idx(row_start(BACK_TABLE, BRENDAN_BACK_PIC)),
            &picture_row(at(BRENDAN_BACK), BACK_PICTURE_SIZE, BRENDAN_BACK_PIC),
        )
        .write(
            idx(row_start(BACK_TABLE, MAY_BACK_PIC)),
            &picture_row(at(MAY_BACK), BACK_PICTURE_SIZE, MAY_BACK_PIC),
        )
        .write(
            idx(row_start(BACK_PALETTE_TABLE, BRENDAN_BACK_PIC)),
            &palette_row(at(BRENDAN_PALETTE), BRENDAN_BACK_PIC),
        )
        .write(
            idx(row_start(BACK_PALETTE_TABLE, MAY_BACK_PIC)),
            &palette_row(at(MAY_PALETTE), MAY_BACK_PIC),
        )
        .write(idx(BRENDAN_FRONT), &lz77_literals(&front_tiles(0x21)))
        .write(idx(BRENDAN_BACK), &back_tiles(0x12))
        .write(idx(BRENDAN_PALETTE), &lz77_literals(&colors(0x0100)))
        .write(idx(MAY_FRONT), &lz77_literals(&front_tiles(0x43)))
        .write(idx(MAY_BACK), &back_tiles(0x56))
        .write(idx(MAY_PALETTE), &lz77_literals(&colors(0x0200)))
        .write(idx(PALETTE_COPY), &lz77_literals(&colors(0x0100)))
}

fn rom_from(fixture: RomFixture) -> crate::rom::Rom {
    crate::rom::Rom::from_bytes(fixture.finish()).expect("the fixture header is valid")
}

fn rom() -> crate::rom::Rom {
    rom_from(base_fixture())
}

fn broken(edit: impl FnOnce(RomFixture) -> RomFixture) -> crate::rom::Rom {
    rom_from(edit(base_fixture()))
}

const fn front(id: &'static str, addr: u32) -> ImageRoot {
    ImageRoot {
        id,
        addr: at(addr),
        encoding: Encoding::Lz77,
        rom_bit_depth: 4,
        pack_bit_depth: 4,
        width: 64,
        height: 64,
        metatile_width: 8,
        metatile_height: 8,
        tile_count: 64,
    }
}

const fn back(id: &'static str, addr: u32) -> ImageRoot {
    ImageRoot {
        id,
        addr: at(addr),
        encoding: Encoding::Raw,
        rom_bit_depth: 4,
        pack_bit_depth: 4,
        width: 64,
        height: 256,
        metatile_width: 8,
        metatile_height: 32,
        tile_count: 256,
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

const BRENDAN_FRONT_ROOT: TrainerFrontRoot = TrainerFrontRoot {
    trainer_pic: BRENDAN_FRONT_PIC,
    front: front("trainer/brendan/front", BRENDAN_FRONT),
    palette: palette("trainer/brendan/palette", BRENDAN_PALETTE),
};
const MAY_FRONT_ROOT: TrainerFrontRoot = TrainerFrontRoot {
    trainer_pic: MAY_FRONT_PIC,
    front: front("trainer/may/front", MAY_FRONT),
    palette: palette("trainer/may/palette", MAY_PALETTE),
};
const BRENDAN_BACK_ROOT: TrainerBackRoot = TrainerBackRoot {
    trainer_pic: BRENDAN_FRONT_PIC,
    back_pic: BRENDAN_BACK_PIC,
    back: back("trainer/brendan/back", BRENDAN_BACK),
};
const MAY_BACK_ROOT: TrainerBackRoot = TrainerBackRoot {
    trainer_pic: MAY_FRONT_PIC,
    back_pic: MAY_BACK_PIC,
    back: back("trainer/may/back", MAY_BACK),
};

static FRONTS: [TrainerFrontRoot; 2] = [BRENDAN_FRONT_ROOT, MAY_FRONT_ROOT];
static BACKS: [TrainerBackRoot; 2] = [BRENDAN_BACK_ROOT, MAY_BACK_ROOT];

fn family(
    fronts: &'static [TrainerFrontRoot],
    backs: &'static [TrainerBackRoot],
) -> TrainerBattlerRoots {
    TrainerBattlerRoots {
        front: TrainerFrontRoots {
            front_table: at(FRONT_TABLE),
            palette_table: at(FRONT_PALETTE_TABLE),
            trainers: fronts,
        },
        back_table: at(BACK_TABLE),
        back_palette_table: at(BACK_PALETTE_TABLE),
        backs,
    }
}

fn roots_of(family: TrainerBattlerRoots) -> Roots {
    Roots {
        trainer_battlers: family,
        ..Roots::NONE
    }
}

fn run_on(
    rom: &crate::rom::Rom,
    roots: &Roots,
) -> Result<(Vec<u8>, Vec<DirectoryEntry>), ImportError> {
    let mut writer = PackWriter::new();
    write(rom, roots, &mut writer)?;
    let pack = writer.finish()?;
    let entries = parse_directory(&pack).expect("a readable pack");
    Ok((pack, entries))
}

fn run(rom: &crate::rom::Rom) -> Result<(Vec<u8>, Vec<DirectoryEntry>), ImportError> {
    run_on(rom, &roots_of(family(&FRONTS, &BACKS)))
}

fn mismatch(rom: &crate::rom::Rom, root: &str, field: &str) {
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

/// A back sheet's pixels: each 64-row frame holds its two nibbles alternately.
fn sheet_pixels(first: u8) -> Vec<u8> {
    (0..4u8)
        .flat_map(|frame| {
            let byte = first + frame;
            [byte & 0xF, byte >> 4].repeat(FRAME_BYTES)
        })
        .collect()
}

#[test]
fn both_trainers_write_a_front_a_back_and_one_palette_in_id_order() {
    let (pack, entries) = run(&rom()).expect("well-formed tables");
    let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "trainer/brendan/back",
            "trainer/brendan/front",
            "trainer/brendan/palette",
            "trainer/may/back",
            "trainer/may/front",
            "trainer/may/palette",
        ]
    );
    for entry in &entries {
        match entry.kind {
            EntryKind::Palette { color_count } => assert_eq!(color_count, 16),
            EntryKind::Image {
                width,
                height,
                bit_depth,
            } => {
                let expected = if entry.id.ends_with("/back") { 256 } else { 64 };
                assert_eq!(
                    (width, height, bit_depth),
                    (64, expected, 4),
                    "{}",
                    entry.id
                );
            }
            EntryKind::Raw => panic!("unexpected kind for {}", entry.id),
        }
    }
    let alternating = |low: u8, high: u8| [low, high].repeat(64 * 64 / 2);
    assert_eq!(
        payload(&pack, &entries, "trainer/brendan/front"),
        alternating(1, 2)
    );
    assert_eq!(
        payload(&pack, &entries, "trainer/may/front"),
        alternating(3, 4)
    );
    assert_eq!(
        payload(&pack, &entries, "trainer/brendan/back"),
        sheet_pixels(0x12)
    );
    assert_eq!(
        payload(&pack, &entries, "trainer/may/back"),
        sheet_pixels(0x56)
    );
    assert_eq!(
        payload(&pack, &entries, "trainer/brendan/palette"),
        colors(0x0100)
    );
    assert_eq!(
        payload(&pack, &entries, "trainer/may/palette"),
        colors(0x0200)
    );
}

#[test]
fn the_emitted_pack_resolves_both_trainers_in_both_directions() {
    let (pack, _) = run(&rom()).expect("well-formed tables");
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-trainer-battlers-import-{}.pack",
        std::process::id()
    ));
    std::fs::write(&path, pack).unwrap();
    let loaded = AssetPack::load(&path).expect("a loadable pack");
    for (trainer, front_byte, back_first, first_color) in [
        (TrainerPicId::BRENDAN, 0x21_u8, 0x12_u8, 0x0100_u16),
        (TrainerPicId::MAY, 0x43, 0x56, 0x0200),
    ] {
        let front = loaded
            .trainer_battler(trainer, BattlerDirection::Front)
            .expect("a front");
        let back = loaded
            .trainer_battler(trainer, BattlerDirection::Back)
            .expect("a back");
        assert_eq!(front.art.image.pixels[0], front_byte & 0xF);
        assert_eq!((back.art.image.width, back.art.image.height), (64, 256));
        for (frame, rows) in back.art.image.pixels.chunks(64 * 64).enumerate() {
            let byte = back_first + u8::try_from(frame).unwrap();
            assert_eq!(rows[0], byte & 0xF, "{trainer:?} frame {frame}");
        }
        assert_eq!(front.art.palette.color(0), Some(first_color));
        assert_eq!(back.art.palette.color(0), Some(first_color));
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_profile_with_no_trainer_battlers_reads_no_tables() {
    let unusable = GbaPtr::at(ROM_BASE + 0x00FF_FFF0);
    let none = TrainerBattlerRoots {
        front: TrainerFrontRoots {
            front_table: unusable,
            palette_table: unusable,
            trainers: &[],
        },
        back_table: unusable,
        back_palette_table: unusable,
        backs: &[],
    };
    let (_, entries) = run_on(&rom(), &roots_of(none)).expect("nothing to corroborate");
    assert!(entries.is_empty());
}

#[test]
fn the_root_iterators_name_every_battler_entry_once() {
    let roots = roots_of(family(&FRONTS, &BACKS));
    let images: Vec<&str> = roots.images().map(|root| root.id).collect();
    assert_eq!(
        images,
        [
            "trainer/brendan/front",
            "trainer/may/front",
            "trainer/brendan/back",
            "trainer/may/back",
        ]
    );
    let palettes: Vec<&str> = roots.palettes().map(|root| root.id).collect();
    assert_eq!(palettes, ["trainer/brendan/palette", "trainer/may/palette"]);
    assert_eq!(Roots::NONE.images().count(), 0);
    assert_eq!(Roots::NONE.palettes().count(), 0);
}

#[test]
fn a_back_row_naming_another_copy_or_the_wrong_size_or_tag_is_refused() {
    let may_back = idx(row_start(BACK_TABLE, MAY_BACK_PIC));
    mismatch(
        &broken(|f| {
            f.write(
                may_back,
                &picture_row(at(STRAY), BACK_PICTURE_SIZE, MAY_BACK_PIC),
            )
        }),
        "trainer/may/back",
        "CompressedSpriteSheet.data",
    );
    // A single frame's size describes a frame descriptor, not the table row.
    mismatch(
        &broken(|f| f.write(may_back, &picture_row(at(MAY_BACK), 0x800, MAY_BACK_PIC))),
        "trainer/may/back",
        "CompressedSpriteSheet.size",
    );
    mismatch(
        &broken(|f| {
            f.write(
                may_back,
                &picture_row(at(MAY_BACK), BACK_PICTURE_SIZE, MAY_FRONT_PIC),
            )
        }),
        "trainer/may/back",
        "CompressedSpriteSheet.tag",
    );
}

#[test]
fn a_back_palette_row_must_name_the_front_palette_stream() {
    // An identical stream elsewhere carries the same colours but is another copy.
    let brendan = idx(row_start(BACK_PALETTE_TABLE, BRENDAN_BACK_PIC));
    mismatch(
        &broken(|f| f.write(brendan, &palette_row(at(PALETTE_COPY), BRENDAN_BACK_PIC))),
        "trainer/brendan/palette",
        "CompressedSpritePalette.data",
    );
    let may = idx(row_start(BACK_PALETTE_TABLE, MAY_BACK_PIC));
    mismatch(
        &broken(|f| f.write(may, &palette_row(at(MAY_PALETTE), MAY_FRONT_PIC))),
        "trainer/may/palette",
        "CompressedSpritePalette.tag",
    );
}

#[test]
fn the_front_rows_are_still_refused_when_they_disagree() {
    mismatch(
        &broken(|f| {
            f.write(
                idx(row_start(FRONT_TABLE, MAY_FRONT_PIC)),
                &picture_row(at(MAY_FRONT), 0x800, MAY_BACK_PIC),
            )
        }),
        "trainer/may/front",
        "CompressedSpriteSheet.tag",
    );
    mismatch(
        &broken(|f| {
            f.write(
                idx(row_start(FRONT_PALETTE_TABLE, BRENDAN_FRONT_PIC)),
                &palette_row(at(PALETTE_COPY), BRENDAN_FRONT_PIC),
            )
        }),
        "trainer/brendan/palette",
        "CompressedSpritePalette.data",
    );
}

#[test]
fn a_back_index_outside_the_back_tables_is_out_of_range() {
    static PAST_THE_END: [TrainerBackRoot; 1] = [TrainerBackRoot {
        back_pic: PAST_THE_BACK_TABLE,
        ..BRENDAN_BACK_ROOT
    }];
    static FRONT_INDEX_AS_BACK: [TrainerBackRoot; 1] = [TrainerBackRoot {
        back_pic: BRENDAN_FRONT_PIC,
        ..BRENDAN_BACK_ROOT
    }];
    for backs in [&PAST_THE_END, &FRONT_INDEX_AS_BACK] {
        let err = run_on(&rom(), &roots_of(family(&FRONTS, backs))).unwrap_err();
        assert!(
            matches!(
                err,
                ImportError::IndexOutOfRange {
                    table: "gTrainerBackPicTable",
                    count: BACK_TABLE_ROWS,
                    ..
                }
            ),
            "{err}"
        );
    }
}

#[test]
fn a_back_picture_without_its_trainers_front_is_refused() {
    static ONLY_BRENDAN: [TrainerFrontRoot; 1] = [BRENDAN_FRONT_ROOT];
    let err = run_on(&rom(), &roots_of(family(&ONLY_BRENDAN, &BACKS))).unwrap_err();
    assert!(
        matches!(&err, ImportError::StructMismatch { root, field }
            if *root == "trainer/may/back" && *field == "TrainerBackRoot.trainer_pic"),
        "{err}"
    );
}

/// Brendan's back sheet where its 8192 bytes cannot fit.
static SHEET_AT_THE_END: [TrainerBackRoot; 1] = [TrainerBackRoot {
    back: ImageRoot {
        addr: GbaPtr::at(ROM_BASE + 0x00FF_F000),
        ..BRENDAN_BACK_ROOT.back
    },
    ..BRENDAN_BACK_ROOT
}];

#[test]
fn a_truncated_back_sheet_or_corrupt_front_stream_is_refused_and_queues_nothing() {
    let at_the_end = GbaPtr::at(ROM_BASE + 0x00FF_F000);
    let rom = broken(|f| {
        f.write(
            idx(row_start(BACK_TABLE, BRENDAN_BACK_PIC)),
            &picture_row(at_the_end, BACK_PICTURE_SIZE, BRENDAN_BACK_PIC),
        )
    });
    let mut writer = PackWriter::new();
    let err = write(
        &rom,
        &roots_of(family(&FRONTS, &SHEET_AT_THE_END)),
        &mut writer,
    )
    .unwrap_err();
    assert!(matches!(err, ImportError::Truncated { .. }), "{err}");
    assert_eq!(writer.len(), 0);

    let mut writer = PackWriter::new();
    let rom = broken(|f| f.write(idx(MAY_FRONT), &[0x11]));
    let err = write(&rom, &roots_of(family(&FRONTS, &BACKS)), &mut writer).unwrap_err();
    assert!(matches!(err, ImportError::Lz77 { .. }), "{err}");
    assert_eq!(writer.len(), 0);
}
