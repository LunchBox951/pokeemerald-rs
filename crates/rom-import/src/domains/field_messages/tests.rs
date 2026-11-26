//! Field-message reader tests over a synthetic ROM.
//!
//! The fixture plants the upstream string `gText_PkmnFainted_FldPsn` and reads
//! it back through `assets`' own decoder. The failure tests break one root or
//! one stream at a time.

use assets::pack::{FieldMessage, FieldMessageError, FieldMessageTable};
use pack_format::{parse_directory, EntryKind, PackWriter};

use super::{decode_field_message_payload, write, PACK_ID};
use crate::error::ImportError;
use crate::fixture::RomFixture;
use crate::reader::{GbaPtr, ROM_BASE};
use crate::rom::{Rom, ROM_SIZE};
use crate::roots::{FieldMessageRoot, Roots};

/// Where the fixture plants the string.
const STRING: usize = 0x2000;

/// `{STR_VAR_1} fainted…\p\n` and its `EOS`, spelled through `charmap.txt`.
const FAINTED: [u8; 14] = [
    0xFD, 0x02, 0x00, 0xDA, 0xD5, 0xDD, 0xE2, 0xE8, 0xD9, 0xD8, 0xB0, 0xFB, 0xFE, 0xFF,
];

fn at(off: usize) -> GbaPtr {
    GbaPtr::at(ROM_BASE + u32::try_from(off).expect("a fixture offset fits in u32"))
}

const ROOTS: &[FieldMessageRoot] = &[FieldMessageRoot {
    symbol: "gText_PkmnFainted_FldPsn",
    addr: GbaPtr::at(ROM_BASE + 0x2000),
}];

fn roots() -> &'static [FieldMessageRoot] {
    ROOTS
}

fn rom_of(rom: RomFixture) -> Rom {
    Rom::from_bytes(rom.finish()).expect("the fixture header is valid")
}

fn fixture() -> RomFixture {
    RomFixture::new()
        .emerald_header()
        .write(STRING - 1, &[0xFF])
        .write(STRING, &FAINTED)
        .write(STRING + FAINTED.len(), &[0xD5, 0xD5, 0xFF])
}

/// What the domain queued: each entry's id, kind, and payload.
type Written = Vec<(String, EntryKind, Vec<u8>)>;

fn written(rom: &Rom, field_messages: &'static [FieldMessageRoot]) -> Result<Written, ImportError> {
    let roots = Roots {
        field_messages,
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
    let entries = written(&rom, roots()).expect("a consistent table");
    let [(id, kind, payload)] = entries.as_slice() else {
        panic!("one entry, got {}", entries.len())
    };
    assert_eq!(id, PACK_ID);
    assert_eq!(*kind, EntryKind::Raw);

    let table = FieldMessageTable::decode(payload).expect("canonical bytes");
    // The stream stops at its own `EOS`; the string planted after it is not read.
    assert_eq!(table.string(FieldMessage::PkmnFaintedFldPsn), FAINTED);
}

#[test]
fn the_payload_reader_and_the_domain_agree() {
    let rom = rom_of(fixture());
    let payload = decode_field_message_payload(&rom.reader(), roots()).expect("a table");
    let entries = written(&rom, roots()).expect("a table");
    assert_eq!(entries[0].2, payload);
}

#[test]
fn a_profile_without_field_messages_writes_nothing() {
    let rom = rom_of(fixture());
    let entries = written(&rom, &[]).expect("no roots is no work");
    assert!(entries.is_empty());
    let err = decode_field_message_payload(&rom.reader(), &[]).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::StructMismatch {
                root: PACK_ID,
                field: "field message roots"
            }
        ),
        "{err}"
    );
}

#[test]
fn roots_outside_the_inventory_are_refused() {
    let rom = rom_of(fixture());
    let one = roots()[0];
    let surplus: &'static [FieldMessageRoot] = Box::leak(Box::new([one, one]));
    let renamed: &'static [FieldMessageRoot] = Box::leak(Box::new([FieldMessageRoot {
        symbol: "gText_Other",
        ..one
    }]));
    let unset: &'static [FieldMessageRoot] = Box::leak(Box::new([FieldMessageRoot {
        addr: GbaPtr::AT_BASE,
        ..one
    }]));
    for (bad, expected) in [
        (surplus, "field message roots"),
        (renamed, "field message symbol"),
        (unset, "field message address"),
    ] {
        let err = written(&rom, bad).unwrap_err();
        assert!(
            matches!(&err, ImportError::StructMismatch { root: PACK_ID, field } if *field == expected),
            "{err}"
        );
    }
}

#[test]
fn an_address_off_the_image_or_an_unterminated_tail_is_refused() {
    let rom = rom_of(fixture());
    let off_image: &'static [FieldMessageRoot] = Box::leak(Box::new([FieldMessageRoot {
        addr: at(ROM_SIZE),
        ..roots()[0]
    }]));
    assert!(written(&rom, off_image).is_err());

    // The last bytes of the image hold a string with no terminator.
    let rom = rom_of(fixture().write(ROM_SIZE - 3, &[0xFD, 0x02, 0x00]));
    let tail: &'static [FieldMessageRoot] = Box::leak(Box::new([FieldMessageRoot {
        addr: at(ROM_SIZE - 3),
        ..roots()[0]
    }]));
    let err = written(&rom, tail).unwrap_err();
    assert!(
        matches!(err, ImportError::Truncated { at, .. } if at == ROM_SIZE - 3),
        "{err}"
    );
}

#[test]
fn a_stream_with_an_unassigned_control_is_a_schema_error() {
    // `0xFC 0x7F` is no extended control the charmap assigns.
    let rom = rom_of(fixture().write(STRING, &[0xFC, 0x7F, 0xFF]));
    let err = written(&rom, roots()).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::FieldMessage {
                id: PACK_ID,
                source: FieldMessageError::MalformedString { .. }
            }
        ),
        "{err}"
    );
}

#[test]
fn a_control_operand_of_ff_is_read_through_to_the_real_eos() {
    // `{PAUSE 255}` then `EOS`, and the same around text; the filler after
    // each is also `0xFF`.
    for planted in [
        &[0xFC, 0x08, 0xFF, 0xFF][..],
        &[0x01, 0xFC, 0x08, 0xFF, 0x02, 0xFF][..],
        &[0x01, 0xFD, 0xFF, 0x02, 0xFF][..],
    ] {
        let rom = rom_of(fixture().write(STRING, planted));
        let entries = written(&rom, roots()).expect("a valid stream with an FF operand");
        let table = FieldMessageTable::decode(&entries[0].2).expect("canonical bytes");
        assert_eq!(
            table.string(FieldMessage::ALL[0]),
            planted,
            "{planted:02X?}"
        );
    }
}

#[test]
fn a_control_sequence_cut_by_the_end_of_the_image_is_refused() {
    for cut in [
        &[0xFC][..],
        &[0xFC, 0x08][..],
        &[0xFC, 0x04, 0xFF, 0xFF][..],
        &[0xFD][..],
    ] {
        let at_end = ROM_SIZE - cut.len();
        let rom = rom_of(fixture().write(at_end, cut));
        let tail: &'static [FieldMessageRoot] = Box::leak(Box::new([FieldMessageRoot {
            addr: at(at_end),
            ..roots()[0]
        }]));
        let err = written(&rom, tail).unwrap_err();
        assert!(
            matches!(err, ImportError::Truncated { at, .. } if at == at_end),
            "{cut:02X?}: {err}"
        );
    }
}
