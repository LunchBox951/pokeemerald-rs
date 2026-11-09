//! Trainer class-name reader tests over a synthetic ROM.
//!
//! The fixture plants the 858-byte fixed-width array and reads it back through
//! `assets`' own decoder. The failure tests break one row, byte, or root at a
//! time.

use assets::pack::TrainerClassNameError;
use assets::{TrainerClass, TrainerClassNameTable};
use pack_format::{parse_directory, EntryKind, PackWriter};

use super::{decode_trainer_class_name_payload, write, PACK_ID, TABLE_BYTES};
use crate::error::ImportError;
use crate::fixture::RomFixture;
use crate::reader::{GbaPtr, ROM_BASE};
use crate::rom::Rom;
use crate::roots::Roots;

/// Where the array is planted; unaligned on purpose.
const TABLE: usize = 0x2001;

/// The rival's name: `{PKMN} TRAINER` (`53 54` are the `PK` and `MN` glyphs).
const RIVAL: [u8; 11] = [
    0x53, 0x54, 0x00, 0xCE, 0xCC, 0xBB, 0xC3, 0xC8, 0xBF, 0xCC, 0xFF,
];

fn at(off: usize) -> GbaPtr {
    GbaPtr::at(ROM_BASE + u32::try_from(off).expect("a fixture offset fits in u32"))
}

/// A distinct stream per class, with the rival's name where upstream has it.
fn stream_of(id: usize) -> Vec<u8> {
    match id {
        0 | 1 | 50 | 65 => RIVAL.to_vec(),
        // A space inside the name is a zero byte, not padding.
        2 => vec![0xBB, 0x00, 0xBC, 0xFF],
        // Exactly fills the row: twelve glyphs and the `EOS`.
        44 => [0xC0; 12].into_iter().chain([0xFF]).collect(),
        _ => vec![0xA1 + u8::try_from(id).unwrap(), 0xFF],
    }
}

fn rows() -> Vec<u8> {
    let mut out = Vec::new();
    for id in 0..66 {
        let stream = stream_of(id);
        out.extend_from_slice(&stream);
        out.resize(out.len() + 13 - stream.len(), 0);
    }
    out
}

fn rom_with(rows: &[u8]) -> Rom {
    let bytes = RomFixture::new()
        .emerald_header()
        .write(TABLE, rows)
        .finish();
    Rom::from_bytes(bytes).expect("the fixture header is valid")
}

fn written(
    rom: &Rom,
    table: Option<GbaPtr>,
) -> Result<Vec<(String, EntryKind, Vec<u8>)>, ImportError> {
    let roots = Roots {
        trainer_class_names: table,
        ..Roots::NONE
    };
    let mut writer = PackWriter::new();
    write(rom, &roots, &mut writer)?;
    let bytes = writer.finish().expect("the queued entries are valid");
    let directory = parse_directory(&bytes).expect("the pack parses");
    Ok(directory
        .into_iter()
        .map(|entry| {
            let payload = bytes[entry.offset..entry.offset + entry.length].to_vec();
            (entry.id, entry.kind, payload)
        })
        .collect())
}

#[test]
fn the_array_is_858_bytes() {
    assert_eq!(TABLE_BYTES, 858);
    assert_eq!(rows().len(), 858);
}

#[test]
fn a_planted_array_yields_the_canonical_table() {
    let rom = rom_with(&rows());
    let entries = written(&rom, Some(at(TABLE))).expect("a consistent table");
    let [(id, kind, payload)] = entries.as_slice() else {
        panic!("one entry, got {}", entries.len())
    };
    assert_eq!(id, PACK_ID);
    assert_eq!(*kind, EntryKind::Raw);

    let table = TrainerClassNameTable::decode(payload).expect("canonical bytes");
    for class in TrainerClass::all() {
        assert_eq!(
            table.name(class).unwrap(),
            stream_of(usize::from(class.0)),
            "{class:?}"
        );
    }
    // The rival decodes to the `PK` and `MN` glyphs, then ` TRAINER`; the
    // padding after the name is gone.
    assert_eq!(table.name(TrainerClass::RIVAL), Some(&RIVAL[..]));
    // The classes sharing a display name keep separate records.
    for class in [
        TrainerClass::PKMN_TRAINER_1,
        TrainerClass::PKMN_TRAINER_2,
        TrainerClass::RS_PROTAG,
    ] {
        assert_eq!(table.name(class), table.name(TrainerClass::RIVAL));
    }
}

#[test]
fn the_payload_reader_and_the_domain_agree() {
    let rom = rom_with(&rows());
    let payload = decode_trainer_class_name_payload(&rom.reader(), at(TABLE)).expect("a table");
    assert_eq!(written(&rom, Some(at(TABLE))).unwrap()[0].2, payload);
}

#[test]
fn a_profile_with_no_root_writes_nothing() {
    let rom = rom_with(&rows());
    assert!(written(&rom, None).unwrap().is_empty());
}

#[test]
fn the_unset_placeholder_is_refused() {
    let rom = rom_with(&rows());
    let err = written(&rom, Some(GbaPtr::AT_BASE)).unwrap_err();
    assert!(
        matches!(err, ImportError::StructMismatch { root: PACK_ID, field } if field == "trainer class name address"),
        "{err:?}"
    );
}

#[test]
fn an_array_running_off_the_image_is_refused() {
    let rom = rom_with(&rows());
    // The last row would end past the image.
    let near_end = crate::rom::ROM_SIZE - TABLE_BYTES + 1;
    assert!(decode_trainer_class_name_payload(&rom.reader(), at(near_end)).is_err());
    let off_image = crate::rom::ROM_SIZE + 0x100;
    assert!(decode_trainer_class_name_payload(&rom.reader(), at(off_image)).is_err());
}

#[test]
fn nonzero_padding_after_a_name_is_refused() {
    let mut bad = rows();
    bad[13 * 7 + 12] = 0x01;
    let err = decode_trainer_class_name_payload(&rom_with(&bad).reader(), at(TABLE)).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::TrainerClassName {
                id: PACK_ID,
                source: TrainerClassNameError::NonzeroPadding { id: 7 }
            }
        ),
        "{err:?}"
    );
}

#[test]
fn a_row_without_a_terminator_does_not_borrow_the_next_rows() {
    let mut bad = rows();
    // Row 9 is all glyphs; row 10 starts with a valid string.
    bad[13 * 9..13 * 10].fill(0xC0);
    let err = decode_trainer_class_name_payload(&rom_with(&bad).reader(), at(TABLE)).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::TrainerClassName {
                source: TrainerClassNameError::MalformedString { id: 9, .. },
                ..
            }
        ),
        "{err:?}"
    );
}

#[test]
fn a_control_sequence_cannot_swallow_its_rows_terminator() {
    let mut bad = rows();
    // `FD` takes the next byte as its argument, so this row has no `EOS`.
    bad[13 * 20..13 * 21].fill(0);
    bad[13 * 20..13 * 20 + 2].copy_from_slice(&[0xFD, 0xFF]);
    let err = decode_trainer_class_name_payload(&rom_with(&bad).reader(), at(TABLE)).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::TrainerClassName {
                source: TrainerClassNameError::MalformedString { id: 20, .. },
                ..
            }
        ),
        "{err:?}"
    );
}

#[test]
fn an_empty_name_or_an_all_zero_row_is_judged_by_its_terminator() {
    // An empty name is just an `EOS` and zero padding: valid.
    let mut ok = rows();
    ok[13 * 3..13 * 4].fill(0);
    ok[13 * 3] = 0xFF;
    decode_trainer_class_name_payload(&rom_with(&ok).reader(), at(TABLE)).unwrap();
    // A row of zeros is twelve... thirteen spaces and no terminator.
    let mut bad = rows();
    bad[13 * 3..13 * 4].fill(0);
    let err = decode_trainer_class_name_payload(&rom_with(&bad).reader(), at(TABLE)).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::TrainerClassName {
                source: TrainerClassNameError::MalformedString { id: 3, .. },
                ..
            }
        ),
        "{err:?}"
    );
}
