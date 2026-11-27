//! Sight-trainer intro-speech reader tests over a synthetic ROM.
//!
//! The fixture plants a distinct stream per script, a `trainerbattle` command
//! naming each (deliberately unaligned), and a verbatim copy of Amy's stream
//! that a rematch command names. It reads them back through `assets`' own
//! decoder. The failure tests break one root, stream, or witness at a time.

use assets::pack::SightTrainerIntroSpeechError;
use assets::{SightTrainerIntroSpeechTable, SIGHT_TRAINER_INTRO_SCRIPTS};
use pack_format::{parse_directory, EntryKind, PackWriter};

use super::{
    decode_sight_trainer_intro_speech_payload, witness_head, write, COMMAND_HEAD_LEN, COMMAND_LEN,
    INTRO_POINTER_OFFSET, PACK_ID,
};
use crate::error::ImportError;
use crate::fixture::RomFixture;
use crate::reader::{GbaPtr, ROM_BASE};
use crate::rom::{Rom, ROM_SIZE};
use crate::roots::{Roots, SightTrainerIntroSpeechRoot};

const AMY: usize = 0;
const LIV: usize = 4;
/// A verbatim copy of Amy's stream, which a rematch command names.
const COPY: usize = 0x3000;

const fn stream_at(position: usize) -> usize {
    0x2000 + position * 0x40
}

const fn command_at(position: usize) -> usize {
    0x1001 + position * 0x20
}

fn stream_of(position: usize) -> [u8; 2] {
    [0xC0 + u8::try_from(position).unwrap(), 0xFF]
}

fn at(off: usize) -> GbaPtr {
    GbaPtr::at(ROM_BASE + u32::try_from(off).expect("a fixture offset fits in u32"))
}

fn roots() -> Vec<SightTrainerIntroSpeechRoot> {
    SIGHT_TRAINER_INTRO_SCRIPTS
        .into_iter()
        .enumerate()
        .map(|(position, symbol)| SightTrainerIntroSpeechRoot {
            symbol,
            addr: at(stream_at(position)),
            script: at(command_at(position)),
        })
        .collect()
}

fn leak(roots: Vec<SightTrainerIntroSpeechRoot>) -> &'static [SightTrainerIntroSpeechRoot] {
    Box::leak(roots.into_boxed_slice())
}

fn rom_of(rom: RomFixture) -> Rom {
    Rom::from_bytes(rom.finish()).expect("the fixture header is valid")
}

fn command(position: usize, intro: GbaPtr) -> Vec<u8> {
    let mut bytes = witness_head(SIGHT_TRAINER_INTRO_SCRIPTS[position])
        .expect("every inventory script has a witness")
        .to_vec();
    bytes.extend_from_slice(&intro.raw().to_le_bytes());
    bytes.extend_from_slice(&0x0800_0100u32.to_le_bytes());
    bytes
}

fn fixture() -> RomFixture {
    let mut rom = RomFixture::new()
        .emerald_header()
        .write(COPY - 1, &[0xFF])
        .write(COPY, &stream_of(AMY));
    for position in 0..SIGHT_TRAINER_INTRO_SCRIPTS.len() {
        rom = rom
            .write(stream_at(position) - 1, &[0xFF])
            .write(stream_at(position), &stream_of(position))
            .write(
                command_at(position),
                &command(position, at(stream_at(position))),
            );
    }
    rom
}

/// What the domain queued: each entry's id, kind, and payload.
type Written = Vec<(String, EntryKind, Vec<u8>)>;

fn written(
    rom: &Rom,
    sight_trainer_intro_speeches: &'static [SightTrainerIntroSpeechRoot],
) -> Result<Written, ImportError> {
    let roots = Roots {
        sight_trainer_intro_speeches,
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

fn mismatch(err: &ImportError, expected: &str) -> bool {
    matches!(err, ImportError::StructMismatch { root: PACK_ID, field } if *field == expected)
}

#[test]
fn matching_roots_yield_the_canonical_table() {
    let rom = rom_of(fixture());
    let entries = written(&rom, leak(roots())).expect("a consistent table");
    let [(id, kind, payload)] = entries.as_slice() else {
        panic!("one entry, got {}", entries.len())
    };
    assert_eq!(id, PACK_ID);
    assert_eq!(*kind, EntryKind::Raw);

    let table = SightTrainerIntroSpeechTable::decode(payload).expect("canonical bytes");
    for (position, script) in SIGHT_TRAINER_INTRO_SCRIPTS.into_iter().enumerate() {
        assert_eq!(
            table.speech(script),
            Some(stream_of(position).as_slice()),
            "{script}"
        );
    }
}

#[test]
fn the_payload_reader_and_the_domain_agree() {
    let rom = rom_of(fixture());
    let payload =
        decode_sight_trainer_intro_speech_payload(&rom.reader(), &roots()).expect("a table");
    let entries = written(&rom, leak(roots())).expect("a table");
    assert_eq!(entries[0].2, payload);
}

#[test]
fn every_script_has_a_witness_head() {
    for script in SIGHT_TRAINER_INTRO_SCRIPTS {
        assert!(witness_head(script).is_some(), "{script}");
    }
    assert_eq!(witness_head("Route103_EventScript_Nobody"), None);
    assert_eq!(COMMAND_HEAD_LEN, 6);
    assert_eq!(INTRO_POINTER_OFFSET, 6);
    assert_eq!(COMMAND_LEN, 10);
}

#[test]
fn witness_heads_are_the_upstream_commands() {
    // `SCR_OP_TRAINERBATTLE`, the battle type, the trainer (little-endian) and
    // `LOCALID_NONE`. Amy and Liv share a trainer and a type.
    let expected = [
        ("Route103_EventScript_Amy", [0x5C, 6, 0xE1, 0x01, 0, 0]),
        ("Route103_EventScript_Liv", [0x5C, 6, 0xE1, 0x01, 0, 0]),
        ("Route103_EventScript_Daisy", [0x5C, 0, 36, 0, 0, 0]),
        ("Route103_EventScript_Andrew", [0x5C, 0, 0x50, 0x01, 0, 0]),
        ("Route103_EventScript_Miguel", [0x5C, 2, 0x25, 0x01, 0, 0]),
        ("Route103_EventScript_Marcos", [0x5C, 0, 0xBE, 0x02, 0, 0]),
        ("Route103_EventScript_Rhett", [0x5C, 0, 0xBF, 0x02, 0, 0]),
        ("Route103_EventScript_Pete", [0x5C, 0, 0xDF, 0x02, 0, 0]),
        ("Route103_EventScript_Isabelle", [0x5C, 0, 0xE0, 0x02, 0, 0]),
    ];
    for (script, head) in expected {
        assert_eq!(witness_head(script), Some(head), "{script}");
    }
}

#[test]
fn a_profile_without_intro_speeches_writes_nothing() {
    let rom = rom_of(fixture());
    assert!(written(&rom, &[]).expect("no roots is no work").is_empty());
    let err = decode_sight_trainer_intro_speech_payload(&rom.reader(), &[]).unwrap_err();
    assert!(mismatch(&err, "sight trainer intro speech roots"), "{err}");
}

#[test]
fn roots_outside_the_inventory_are_refused() {
    let rom = rom_of(fixture());
    let partial = {
        let mut roots = roots();
        roots.pop();
        roots
    };
    let surplus = {
        let mut roots = roots();
        roots.push(roots[0]);
        roots
    };
    let reordered = {
        let mut roots = roots();
        roots.swap(0, 1);
        roots
    };
    let duplicated = {
        let mut roots = roots();
        roots[1] = roots[0];
        roots
    };
    let renamed = {
        let mut roots = roots();
        roots[4].symbol = "Route103_EventScript_Other";
        roots
    };
    let unset_text = {
        let mut roots = roots();
        roots[4].addr = GbaPtr::AT_BASE;
        roots
    };
    let unset_script = {
        let mut roots = roots();
        roots[4].script = GbaPtr::AT_BASE;
        roots
    };
    for (bad, expected) in [
        (partial, "sight trainer intro speech roots"),
        (surplus, "sight trainer intro speech roots"),
        (reordered, "sight trainer intro speech symbol"),
        (duplicated, "sight trainer intro speech symbol"),
        (renamed, "sight trainer intro speech symbol"),
        (unset_text, "sight trainer intro speech address"),
        (unset_script, "sight trainer intro speech script"),
    ] {
        let err = written(&rom, leak(bad)).unwrap_err();
        assert!(mismatch(&err, expected), "{err}");
    }
}

#[test]
fn an_address_off_the_image_or_an_unterminated_tail_is_refused() {
    let rom = rom_of(fixture());
    let mut off_image = roots();
    off_image[0].addr = at(ROM_SIZE);
    assert!(written(&rom, leak(off_image)).is_err());

    // The last bytes of the image hold a string with no terminator.
    let rom = rom_of(
        fixture()
            .write(ROM_SIZE - 3, &[0xC0, 0xC0, 0xC0])
            .write(command_at(0), &command(0, at(ROM_SIZE - 3))),
    );
    let mut tail = roots();
    tail[0].addr = at(ROM_SIZE - 3);
    let err = written(&rom, leak(tail)).unwrap_err();
    assert!(
        matches!(err, ImportError::Truncated { at, .. } if at == ROM_SIZE - 3),
        "{err}"
    );
}

#[test]
fn a_stream_cut_inside_a_control_sequence_is_a_schema_error() {
    // `{PAUSE}` takes an argument byte; here `EOS` ends the stream first.
    let rom = rom_of(fixture().write(stream_at(7), &[0xFC, 0x08, 0xFF]));
    let err = written(&rom, leak(roots())).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::SightTrainerIntroSpeech {
                id: PACK_ID,
                source: SightTrainerIntroSpeechError::MalformedString { position: 7, .. }
            }
        ),
        "{err}"
    );
}

#[test]
fn the_command_selects_the_copy_it_names() {
    // Two identical copies of Amy's stream; her command names the copy.
    let rom = rom_of(fixture().write(command_at(AMY), &command(AMY, at(COPY))));
    let mut copy = roots();
    copy[AMY].addr = at(COPY);
    assert!(written(&rom, leak(copy)).is_ok());

    // The first copy no longer matches the pointer, though its bytes do.
    let err = written(&rom, leak(roots())).unwrap_err();
    assert!(
        mismatch(&err, "sight trainer intro speech text pointer"),
        "{err}"
    );
}

#[test]
fn a_twin_cannot_borrow_its_siblings_command() {
    // Amy's root naming Liv's command: same trainer and type, other text.
    let rom = rom_of(fixture());
    let mut swapped = roots();
    swapped[AMY].script = at(command_at(LIV));
    let err = written(&rom, leak(swapped)).unwrap_err();
    assert!(
        mismatch(&err, "sight trainer intro speech text pointer"),
        "{err}"
    );
}

#[test]
fn a_command_that_is_not_the_scripts_trainerbattle_is_refused() {
    let good = command(5, at(stream_at(5)));
    // Opcode, battle type, either trainer byte, either local-ID byte.
    for index in 0..COMMAND_HEAD_LEN {
        let mut bad = good.clone();
        bad[index] ^= 1;
        let rom = rom_of(fixture().write(command_at(5), &bad));
        let err = written(&rom, leak(roots())).unwrap_err();
        assert!(
            mismatch(&err, "sight trainer intro speech command"),
            "{index}: {err}"
        );
    }
}

#[test]
fn a_rematch_command_is_not_the_scripts_command() {
    let mut rematch = command(AMY, at(COPY));
    rematch[1] = 7;
    let rom = rom_of(fixture().write(command_at(AMY), &rematch));
    let mut copy = roots();
    copy[AMY].addr = at(COPY);
    let err = written(&rom, leak(copy)).unwrap_err();
    assert!(
        mismatch(&err, "sight trainer intro speech command"),
        "{err}"
    );
}

#[test]
fn a_command_off_the_image_or_with_a_bad_pointer_is_refused() {
    // The command is cut by the end of the image.
    let rom = rom_of(fixture().write(ROM_SIZE - 8, &command(2, at(stream_at(2)))[..8]));
    let mut cut = roots();
    cut[2].script = at(ROM_SIZE - 8);
    let err = written(&rom, leak(cut)).unwrap_err();
    assert!(matches!(err, ImportError::Truncated { .. }), "{err}");

    // The intro pointer is not a cartridge address.
    let mut bad = command(2, at(stream_at(2)));
    bad[INTRO_POINTER_OFFSET..COMMAND_LEN].copy_from_slice(&0x0200_0000u32.to_le_bytes());
    let rom = rom_of(fixture().write(command_at(2), &bad));
    let err = written(&rom, leak(roots())).unwrap_err();
    assert!(
        matches!(err, ImportError::PointerOutOfRange { at, .. } if at == command_at(2) + INTRO_POINTER_OFFSET),
        "{err}"
    );
}
