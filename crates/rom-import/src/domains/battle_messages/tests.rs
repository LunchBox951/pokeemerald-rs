//! Battle-message reader tests over a synthetic ROM.
//!
//! The fixture plants a 369-pointer table and 74 special templates, each a
//! short stream of plain characters ending in `EOS`, then reads them back
//! through `assets`' own decoder. The failure tests break one root or one
//! stream at a time.

use assets::pack::{
    BattleMessageError, BattleMessageSection, BattleMessageTable, BATTLE_MESSAGE_SLOT_COUNT,
    BATTLE_MESSAGE_SPECIAL_TEMPLATES, BATTLE_MESSAGE_TABLE_START,
};
use pack_format::{parse_directory, EntryKind, PackWriter};

use super::{decode_battle_message_payload, write, PACK_ID};
use crate::error::ImportError;
use crate::fixture::RomFixture;
use crate::reader::{GbaPtr, ROM_BASE};
use crate::rom::{Rom, ROM_SIZE};
use crate::roots::{BattleMessageRoots, Roots, SpecialMessageRoot};

/// Where the fixture plants the pointer table.
const TABLE: usize = 0x1000;
/// Where it plants the table's strings, one per 8-byte cell.
const STRINGS: usize = 0x2000;
/// Where it plants the special templates, one per 8-byte cell.
const SPECIALS: usize = 0x4000;
const CELL: usize = 8;

/// A fixture offset as a bus offset.
fn small(off: usize) -> u32 {
    u32::try_from(off).expect("a fixture offset fits in u32")
}

/// A small fixture index as a byte.
fn byte(index: usize) -> u8 {
    u8::try_from(index).expect("a fixture index fits in u8")
}

fn at(off: usize) -> GbaPtr {
    GbaPtr::at(ROM_BASE + small(off))
}

/// The stream the fixture plants for table slot `index`: two plain
/// characters, so no byte is a control lead or the terminator.
fn table_stream(index: usize) -> Vec<u8> {
    vec![0x01 + byte(index % 200), 0x01 + byte(index / 200), 0xFF]
}

/// The stream planted for special template `index`.
fn special_stream(index: usize) -> Vec<u8> {
    vec![0x02, 0x01 + byte(index), 0xFF]
}

fn fixture() -> RomFixture {
    let mut rom = RomFixture::new().emerald_header();
    for index in 0..BATTLE_MESSAGE_SLOT_COUNT {
        rom = rom
            .write_ptr(TABLE + index * 4, at(STRINGS + index * CELL))
            .write(STRINGS + index * CELL, &table_stream(index));
    }
    for index in 0..BATTLE_MESSAGE_SPECIAL_TEMPLATES.len() {
        rom = rom.write(SPECIALS + index * CELL, &special_stream(index));
    }
    rom
}

fn special_roots() -> &'static [SpecialMessageRoot] {
    let roots: Vec<SpecialMessageRoot> = BATTLE_MESSAGE_SPECIAL_TEMPLATES
        .iter()
        .enumerate()
        .map(|(index, template)| SpecialMessageRoot {
            symbol: template.symbol,
            addr: at(SPECIALS + index * CELL),
        })
        .collect();
    Box::leak(roots.into_boxed_slice())
}

fn family() -> BattleMessageRoots {
    BattleMessageRoots {
        table: at(TABLE),
        specials: special_roots(),
    }
}

fn rom_of(rom: RomFixture) -> Rom {
    Rom::from_bytes(rom.finish()).expect("the fixture header is valid")
}

/// What the domain queued: each entry's id, kind, and payload.
type Written = Vec<(String, EntryKind, Vec<u8>)>;

fn written(rom: &Rom, family: BattleMessageRoots) -> Result<Written, ImportError> {
    let roots = Roots {
        battle_messages: family,
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
fn matching_roots_yield_the_canonical_table() {
    let rom = rom_of(fixture());
    let entries = written(&rom, family()).expect("a consistent table");
    let [(id, kind, payload)] = entries.as_slice() else {
        panic!("one entry, got {}", entries.len())
    };
    assert_eq!(id, PACK_ID);
    assert_eq!(*kind, EntryKind::Raw);

    let table = BattleMessageTable::decode(payload).expect("canonical bytes");
    for index in 0..BATTLE_MESSAGE_SLOT_COUNT {
        let id = BATTLE_MESSAGE_TABLE_START + u16::try_from(index).expect("a slot index");
        assert_eq!(table.string(id).unwrap(), table_stream(index), "slot {id}");
    }
    for (index, template) in BATTLE_MESSAGE_SPECIAL_TEMPLATES.iter().enumerate() {
        assert_eq!(
            table.special_by_symbol(template.symbol),
            Some(special_stream(index).as_slice()),
            "{}",
            template.symbol
        );
    }
}

#[test]
fn the_payload_reader_and_the_domain_agree() {
    let rom = rom_of(fixture());
    let payload = decode_battle_message_payload(&rom.reader(), &family()).expect("a table");
    let entries = written(&rom, family()).expect("a table");
    assert_eq!(entries[0].2, payload);
}

#[test]
fn a_profile_without_battle_messages_writes_nothing() {
    let rom = rom_of(fixture());
    let entries = written(&rom, BattleMessageRoots::NONE).expect("no roots is no work");
    assert!(entries.is_empty());
}

#[test]
fn a_missing_table_root_is_refused() {
    let rom = rom_of(fixture());
    let err = written(
        &rom,
        BattleMessageRoots {
            table: GbaPtr::AT_BASE,
            specials: special_roots(),
        },
    )
    .unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::StructMismatch {
                root: PACK_ID,
                field: "gBattleStringsTable"
            }
        ),
        "{err}"
    );
}

#[test]
fn a_missing_special_root_is_refused() {
    let rom = rom_of(fixture());
    let short = &special_roots()[..special_roots().len() - 1];
    let err = written(
        &rom,
        BattleMessageRoots {
            table: at(TABLE),
            specials: short,
        },
    )
    .unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::StructMismatch {
                root: PACK_ID,
                field: "special roots"
            }
        ),
        "{err}"
    );

    let err = written(
        &rom,
        BattleMessageRoots {
            table: at(TABLE),
            specials: &[],
        },
    )
    .unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::StructMismatch {
                field: "special roots",
                ..
            }
        ),
        "{err}"
    );
}

#[test]
fn special_roots_out_of_inventory_order_are_refused() {
    let rom = rom_of(fixture());
    let mut swapped = special_roots().to_vec();
    swapped.swap(0, 1);
    let err = written(
        &rom,
        BattleMessageRoots {
            table: at(TABLE),
            specials: Box::leak(swapped.into_boxed_slice()),
        },
    )
    .unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::StructMismatch {
                field: "special symbol",
                ..
            }
        ),
        "{err}"
    );
}

#[test]
fn a_table_slot_that_is_not_a_pointer_is_refused() {
    let rom = rom_of(fixture().write(TABLE + 7 * 4, &0u32.to_le_bytes()));
    let err = written(&rom, family()).unwrap_err();
    assert!(
        matches!(err, ImportError::PointerOutOfRange { at, raw: 0 } if at == TABLE + 7 * 4),
        "{err}"
    );
}

#[test]
fn a_table_running_off_the_image_is_refused() {
    // Only the first of 369 pointers fits before the image ends.
    let rom = rom_of(fixture().write_ptr(ROM_SIZE - 4, at(STRINGS)));
    let err = written(
        &rom,
        BattleMessageRoots {
            table: at(ROM_SIZE - 4),
            specials: special_roots(),
        },
    )
    .unwrap_err();
    assert!(
        matches!(err, ImportError::Truncated { at, .. } if at == ROM_SIZE),
        "{err}"
    );
}

#[test]
fn an_unterminated_table_stream_is_refused() {
    // Plain characters to the last byte of the image: no `EOS` ever follows.
    let last = ROM_SIZE - 3;
    let rom = rom_of(
        fixture()
            .write_ptr(TABLE, at(last))
            .write(last, &[0x01, 0x02, 0x03]),
    );
    let err = written(&rom, family()).unwrap_err();
    assert!(
        matches!(err, ImportError::Truncated { at, .. } if at == last),
        "{err}"
    );
}

#[test]
fn an_unterminated_special_stream_is_refused() {
    let last = ROM_SIZE - 3;
    let mut specials = special_roots().to_vec();
    specials[3].addr = at(last);
    let rom = rom_of(fixture().write(last, &[0x01, 0x02, 0x03]));
    let err = written(
        &rom,
        BattleMessageRoots {
            table: at(TABLE),
            specials: Box::leak(specials.into_boxed_slice()),
        },
    )
    .unwrap_err();
    assert!(
        matches!(err, ImportError::Truncated { at, .. } if at == last),
        "{err}"
    );
}

#[test]
fn a_malformed_table_stream_names_its_id() {
    // `0xFC 0x04` leads a control sequence with three argument bytes; the
    // first `0xFF` cuts it short.
    let rom = rom_of(fixture().write(STRINGS + 5 * CELL, &[0xFC, 0x04, 0xFF]));
    let err = written(&rom, family()).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::BattleMessage {
                id: PACK_ID,
                source: BattleMessageError::MalformedString {
                    section: BattleMessageSection::Table,
                    id,
                    ..
                },
            } if id == BATTLE_MESSAGE_TABLE_START + 5
        ),
        "{err}"
    );
}

#[test]
fn a_malformed_special_stream_names_its_index() {
    let rom = rom_of(fixture().write(SPECIALS + 9 * CELL, &[0xFD, 0xFF]));
    let err = written(&rom, family()).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::BattleMessage {
                id: PACK_ID,
                source: BattleMessageError::MalformedString {
                    section: BattleMessageSection::Special,
                    id: 9,
                    ..
                },
            }
        ),
        "{err}"
    );
}
