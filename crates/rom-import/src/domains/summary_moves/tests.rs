//! Summary-move reader tests over a synthetic ROM.
//!
//! The fixture plants the two pointer tables with their strings, a 355-row
//! `gContestMoves` at its 8-byte stride and a 48-row `gContestEffects` at its
//! 4-byte stride, with filler where upstream's padding and combo fields sit.
//! It reads them back through `assets`' own decoder. The failure tests break
//! one root, row, or stream at a time.

use assets::pack::{
    ContestCategory, ContestEffectId, ContestEffectType, SummaryMoveError, SummaryMoveSection,
    SummaryMoveTable, CONTEST_EFFECT_COUNT, CONTEST_MOVE_COUNT, MOVE_DESCRIPTION_COUNT,
};
use assets::MoveId;
use pack_format::{parse_directory, EntryKind, PackWriter};

use super::{
    decode_summary_move_payload, write, CONTEST_EFFECT_BYTES, CONTEST_MOVE_BYTES, PACK_ID,
};
use crate::error::ImportError;
use crate::fixture::RomFixture;
use crate::reader::{GbaPtr, ROM_BASE};
use crate::rom::{Rom, ROM_SIZE};
use crate::roots::{Roots, SummaryMoveRoots};

/// Where the fixture plants each table.
const DESCRIPTIONS: usize = 0x1000;
const DESCRIPTION_STRINGS: usize = 0x2000;
const CONTEST_MOVES: usize = 0x6000;
const CONTEST_EFFECTS: usize = 0x8000;
const EFFECT_DESCRIPTIONS: usize = 0x8800;
const EFFECT_STRINGS: usize = 0x9000;
/// Strings sit one per 8-byte cell.
const CELL: usize = 8;
/// Filler for bytes the schema must never carry: combo fields and padding.
const FILLER: u8 = 0xEE;
/// The move whose description is a bare `EOS`.
const EMPTY_SLOT: usize = 9;

fn small(off: usize) -> u32 {
    u32::try_from(off).expect("a fixture offset fits in u32")
}

fn byte(index: usize) -> u8 {
    u8::try_from(index).expect("a fixture index fits in u8")
}

fn at(off: usize) -> GbaPtr {
    GbaPtr::at(ROM_BASE + small(off))
}

fn description(slot: usize) -> Vec<u8> {
    if slot == EMPTY_SLOT {
        vec![0xFF]
    } else {
        vec![0x01 + byte(slot % 200), 0x01 + byte(slot / 200), 0xFF]
    }
}

fn effect_text(id: usize) -> Vec<u8> {
    vec![0x03, 0x01 + byte(id), 0xFF]
}

/// The type of effect `id`: upstream's values skip 7.
fn effect_type(id: usize) -> u8 {
    [0, 1, 2, 3, 4, 5, 6, 8][id % 8]
}

fn move_effect(index: usize) -> u8 {
    byte(index % CONTEST_EFFECT_COUNT)
}

fn move_category(index: usize) -> u8 {
    byte(index % 5)
}

fn fixture() -> RomFixture {
    let mut rom = RomFixture::new().emerald_header();
    for slot in 0..MOVE_DESCRIPTION_COUNT {
        rom = rom
            .write_ptr(
                DESCRIPTIONS + slot * 4,
                at(DESCRIPTION_STRINGS + slot * CELL),
            )
            .write(DESCRIPTION_STRINGS + slot * CELL, &description(slot));
    }
    for index in 0..CONTEST_MOVE_COUNT {
        // effect, category in the low three bits of a byte whose high bits are
        // padding, comboStarterId, four combo moves, one padding byte.
        let row = [
            move_effect(index),
            0xF8 | move_category(index),
            FILLER,
            FILLER,
            FILLER,
            FILLER,
            FILLER,
            FILLER,
        ];
        rom = rom.write(CONTEST_MOVES + index * CONTEST_MOVE_BYTES, &row);
    }
    for id in 0..CONTEST_EFFECT_COUNT {
        let row = [effect_type(id), byte(id * 5), byte(id), FILLER];
        rom = rom
            .write(CONTEST_EFFECTS + id * CONTEST_EFFECT_BYTES, &row)
            .write_ptr(EFFECT_DESCRIPTIONS + id * 4, at(EFFECT_STRINGS + id * CELL))
            .write(EFFECT_STRINGS + id * CELL, &effect_text(id));
    }
    rom
}

fn family() -> SummaryMoveRoots {
    SummaryMoveRoots {
        descriptions: at(DESCRIPTIONS),
        contest_moves: at(CONTEST_MOVES),
        contest_effects: at(CONTEST_EFFECTS),
        effect_descriptions: at(EFFECT_DESCRIPTIONS),
    }
}

fn rom_of(rom: RomFixture) -> Rom {
    Rom::from_bytes(rom.finish()).expect("the fixture header is valid")
}

/// What the domain queued: each entry's id, kind, and payload.
type Written = Vec<(String, EntryKind, Vec<u8>)>;

fn written(rom: &Rom, family: SummaryMoveRoots) -> Result<Written, ImportError> {
    let roots = Roots {
        summary_moves: family,
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
fn strides_are_the_word_rounded_struct_sizes() {
    // Seven declared bytes round up to eight, three to four.
    assert_eq!(CONTEST_MOVE_BYTES, 8);
    assert_eq!(CONTEST_EFFECT_BYTES, 4);
}

#[test]
fn matching_roots_yield_the_canonical_table() {
    let rom = rom_of(fixture());
    let entries = written(&rom, family()).expect("consistent tables");
    let [(id, kind, payload)] = entries.as_slice() else {
        panic!("one entry, got {}", entries.len())
    };
    assert_eq!(id, PACK_ID);
    assert_eq!(*kind, EntryKind::Raw);

    let table = SummaryMoveTable::decode(payload).expect("canonical bytes");
    for slot in 0..MOVE_DESCRIPTION_COUNT {
        let move_id = MoveId(u16::try_from(slot + 1).unwrap());
        assert_eq!(
            table.move_description(move_id),
            Some(description(slot).as_slice()),
            "slot {slot}"
        );
    }
    // The empty entry keeps its slot and does not shift its neighbours.
    assert_eq!(
        table.move_description(MoveId(u16::try_from(EMPTY_SLOT + 1).unwrap())),
        Some([0xFF].as_slice())
    );
    for index in 0..CONTEST_MOVE_COUNT {
        let row = table
            .contest_move(MoveId(u16::try_from(index).unwrap()))
            .unwrap();
        assert_eq!(row.effect.index(), move_effect(index), "move {index}");
        assert_eq!(row.category.raw(), move_category(index), "move {index}");
    }
    for id in 0..CONTEST_EFFECT_COUNT {
        let effect = table.contest_effect(ContestEffectId::new(byte(id)).unwrap());
        assert_eq!(effect.effect_type.raw(), effect_type(id), "effect {id}");
        assert_eq!((effect.appeal, effect.jam), (byte(id * 5), byte(id)));
        assert_eq!(effect.description, effect_text(id));
    }
    // The last rows would land on the wrong bytes at a seven- or three-byte stride.
    let last = table.contest_move(MoveId(354)).unwrap();
    assert_eq!(
        last.category,
        ContestCategory::from_raw(move_category(354)).unwrap()
    );
    assert_eq!(
        table
            .contest_effect(ContestEffectId::new(47).unwrap())
            .effect_type,
        ContestEffectType::Unknown
    );
}

#[test]
fn the_payload_reader_and_the_domain_agree() {
    let rom = rom_of(fixture());
    let payload = decode_summary_move_payload(&rom.reader(), &family()).expect("a table");
    let entries = written(&rom, family()).expect("a table");
    assert_eq!(entries[0].2, payload);
}

#[test]
fn padding_and_combo_bytes_never_reach_the_payload() {
    let a = decode_summary_move_payload(&rom_of(fixture()).reader(), &family()).unwrap();
    let rewritten = fixture()
        .write(
            CONTEST_MOVES + 5 * CONTEST_MOVE_BYTES + 2,
            &[1, 2, 3, 4, 5, 6],
        )
        .write(CONTEST_EFFECTS + 5 * CONTEST_EFFECT_BYTES + 3, &[9]);
    let b = decode_summary_move_payload(&rom_of(rewritten).reader(), &family()).unwrap();
    assert_eq!(a, b);
}

#[test]
fn a_profile_without_summary_moves_writes_nothing() {
    let rom = rom_of(fixture());
    let entries = written(&rom, SummaryMoveRoots::NONE).expect("no roots is no work");
    assert!(entries.is_empty());
}

type Clear = fn(&mut SummaryMoveRoots);

#[test]
fn a_missing_root_is_refused() {
    let rom = rom_of(fixture());
    let cases: [(&str, Clear); 4] = [
        ("gMoveDescriptionPointers", |r| {
            r.descriptions = GbaPtr::AT_BASE;
        }),
        ("gContestMoves", |r| r.contest_moves = GbaPtr::AT_BASE),
        ("gContestEffects", |r| r.contest_effects = GbaPtr::AT_BASE),
        ("gContestEffectDescriptionPointers", |r| {
            r.effect_descriptions = GbaPtr::AT_BASE;
        }),
    ];
    for (field, clear) in cases {
        let mut roots = family();
        clear(&mut roots);
        let err = written(&rom, roots).unwrap_err();
        assert!(
            matches!(err, ImportError::StructMismatch { root: PACK_ID, field: f } if f == field),
            "{field}: {err}"
        );
    }
}

#[test]
fn a_table_slot_that_is_not_a_pointer_is_refused() {
    let rom = rom_of(fixture().write(DESCRIPTIONS + 7 * 4, &0u32.to_le_bytes()));
    let err = written(&rom, family()).unwrap_err();
    assert!(
        matches!(err, ImportError::PointerOutOfRange { at, raw: 0 } if at == DESCRIPTIONS + 7 * 4),
        "{err}"
    );

    let rom = rom_of(fixture().write(EFFECT_DESCRIPTIONS + 47 * 4, &0u32.to_le_bytes()));
    let err = written(&rom, family()).unwrap_err();
    assert!(
        matches!(err, ImportError::PointerOutOfRange { at, raw: 0 } if at == EFFECT_DESCRIPTIONS + 47 * 4),
        "{err}"
    );
}

#[test]
fn tables_running_off_the_image_are_refused() {
    // Only the first row of each struct table fits before the image ends.
    for (field, roots) in [
        (
            "gContestMoves",
            SummaryMoveRoots {
                contest_moves: at(ROM_SIZE - CONTEST_MOVE_BYTES),
                ..family()
            },
        ),
        (
            "gContestEffects",
            SummaryMoveRoots {
                contest_effects: at(ROM_SIZE - CONTEST_EFFECT_BYTES),
                ..family()
            },
        ),
    ] {
        let err = written(&rom_of(fixture()), roots).unwrap_err();
        assert!(
            matches!(err, ImportError::Truncated { .. }),
            "{field}: {err}"
        );
    }
}

#[test]
fn an_unterminated_stream_is_refused() {
    let last = ROM_SIZE - 3;
    let rom = rom_of(
        fixture()
            .write_ptr(DESCRIPTIONS, at(last))
            .write(last, &[0x01, 0x02, 0x03]),
    );
    let err = written(&rom, family()).unwrap_err();
    assert!(
        matches!(err, ImportError::Truncated { at, .. } if at == last),
        "{err}"
    );
}

#[test]
fn a_malformed_description_names_its_table_and_slot() {
    let rom = rom_of(fixture().write(DESCRIPTION_STRINGS + 5 * CELL, &[0xFC, 0x04, 0xFF]));
    let err = written(&rom, family()).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::SummaryMove {
                id: PACK_ID,
                source: SummaryMoveError::MalformedString {
                    section: SummaryMoveSection::Description,
                    id: 5,
                    ..
                },
            }
        ),
        "{err}"
    );

    let rom = rom_of(fixture().write(EFFECT_STRINGS + 3 * CELL, &[0xFD, 0xFF]));
    let err = written(&rom, family()).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::SummaryMove {
                source: SummaryMoveError::MalformedString {
                    section: SummaryMoveSection::ContestEffect,
                    id: 3,
                    ..
                },
                ..
            }
        ),
        "{err}"
    );
}

#[test]
fn a_dangling_effect_or_category_is_refused() {
    let rom = rom_of(fixture().write(CONTEST_MOVES + 12 * CONTEST_MOVE_BYTES, &[48]));
    let err = written(&rom, family()).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::SummaryMove {
                source: SummaryMoveError::EffectOutOfRange {
                    move_id: 12,
                    effect: 48
                },
                ..
            }
        ),
        "{err}"
    );

    let rom = rom_of(fixture().write(CONTEST_MOVES + 13 * CONTEST_MOVE_BYTES + 1, &[0xF8 | 5]));
    let err = written(&rom, family()).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::SummaryMove {
                source: SummaryMoveError::CategoryOutOfRange {
                    move_id: 13,
                    category: 5
                },
                ..
            }
        ),
        "{err}"
    );

    let rom = rom_of(fixture().write(CONTEST_EFFECTS + 4 * CONTEST_EFFECT_BYTES, &[7]));
    let err = written(&rom, family()).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::SummaryMove {
                source: SummaryMoveError::EffectTypeInvalid {
                    id: 4,
                    effect_type: 7
                },
                ..
            }
        ),
        "{err}"
    );
}
