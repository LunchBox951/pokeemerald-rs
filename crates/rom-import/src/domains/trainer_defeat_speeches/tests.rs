//! Trainer defeat-speech reader tests over a synthetic ROM.
//!
//! The fixture plants a distinct stream per context and reads it back through
//! `assets`' own decoder. The failure tests break one root, stream, or witness
//! command at a time.

use assets::pack::TrainerDefeatSpeechError;
use assets::{TrainerDefeatSpeechContext as Context, TrainerDefeatSpeechTable};
use pack_format::{parse_directory, EntryKind, PackWriter};

use super::{
    decode_trainer_defeat_speech_payload, witness_head, write, COMMAND_HEAD_LEN, COMMAND_LEN,
    DEFEAT_POINTER_OFFSET, PACK_ID,
};
use crate::error::ImportError;
use crate::fixture::RomFixture;
use crate::reader::{GbaPtr, ROM_BASE};
use crate::rom::{Rom, ROM_SIZE};
use crate::roots::{Roots, TrainerDefeatSpeechRoot};

/// Where context `id`'s stream is planted.
const fn stream_at(id: usize) -> usize {
    0x2000 + id * 0x40
}

/// A lookalike copy of Isabelle's stream, which a text search would also hit.
const DECOY: usize = 0x3000;

/// The `trainerbattle` command naming Isabelle's text; deliberately unaligned.
const COMMAND: usize = 0x1001;

const ISABELLE: usize = 9;

/// `data/text/trainers.inc`-shaped stream: one distinct byte, then `EOS`.
fn stream_of(id: usize) -> [u8; 2] {
    [0xC0 + u8::try_from(id).unwrap(), 0xFF]
}

fn at(off: usize) -> GbaPtr {
    GbaPtr::at(ROM_BASE + u32::try_from(off).expect("a fixture offset fits in u32"))
}

fn root_of(context: Context) -> TrainerDefeatSpeechRoot {
    let id = usize::from(context.id());
    TrainerDefeatSpeechRoot {
        symbol: context.symbol(),
        addr: at(stream_at(id)),
        script: (id == ISABELLE).then(|| at(COMMAND)),
    }
}

fn roots() -> Vec<TrainerDefeatSpeechRoot> {
    Context::ALL.into_iter().map(root_of).collect()
}

fn leak(roots: Vec<TrainerDefeatSpeechRoot>) -> &'static [TrainerDefeatSpeechRoot] {
    Box::leak(roots.into_boxed_slice())
}

fn rom_of(rom: RomFixture) -> Rom {
    Rom::from_bytes(rom.finish()).expect("the fixture header is valid")
}

fn command(defeat: GbaPtr) -> Vec<u8> {
    let mut bytes = witness_head(Context::Route103Isabelle)
        .expect("Isabelle has a witness")
        .to_vec();
    bytes.extend_from_slice(&0x0800_0100u32.to_le_bytes());
    bytes.extend_from_slice(&defeat.raw().to_le_bytes());
    bytes
}

fn fixture() -> RomFixture {
    let mut rom = RomFixture::new()
        .emerald_header()
        .write(COMMAND, &command(at(stream_at(ISABELLE))))
        .write(DECOY - 1, &[0xFF])
        .write(DECOY, &stream_of(ISABELLE));
    for id in 0..Context::ALL.len() {
        rom = rom
            .write(stream_at(id) - 1, &[0xFF])
            .write(stream_at(id), &stream_of(id));
    }
    rom
}

/// What the domain queued: each entry's id, kind, and payload.
type Written = Vec<(String, EntryKind, Vec<u8>)>;

fn written(
    rom: &Rom,
    trainer_defeat_speeches: &'static [TrainerDefeatSpeechRoot],
) -> Result<Written, ImportError> {
    let roots = Roots {
        trainer_defeat_speeches,
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

    let table = TrainerDefeatSpeechTable::decode(payload).expect("canonical bytes");
    for context in Context::ALL {
        assert_eq!(
            table.speech(context),
            stream_of(usize::from(context.id())),
            "{context:?}"
        );
    }
}

#[test]
fn the_payload_reader_and_the_domain_agree() {
    let rom = rom_of(fixture());
    let payload = decode_trainer_defeat_speech_payload(&rom.reader(), &roots()).expect("a table");
    let entries = written(&rom, leak(roots())).expect("a table");
    assert_eq!(entries[0].2, payload);
}

#[test]
fn only_isabelles_speech_needs_a_witness_and_its_head_is_the_upstream_command() {
    for context in Context::ALL {
        assert_eq!(
            witness_head(context).is_some(),
            context == Context::Route103Isabelle,
            "{context:?}"
        );
    }
    // `SCR_OP_TRAINERBATTLE`, `TRAINER_BATTLE_SINGLE`, `TRAINER_ISABELLE`
    // (736 = 0x02E0), `LOCALID_NONE`.
    assert_eq!(
        witness_head(Context::Route103Isabelle),
        Some([0x5C, 0x00, 0xE0, 0x02, 0x00, 0x00])
    );
    assert_eq!(COMMAND_HEAD_LEN, 6);
    assert_eq!(DEFEAT_POINTER_OFFSET, 10);
    assert_eq!(COMMAND_LEN, 14);
}

#[test]
fn a_profile_without_defeat_speeches_writes_nothing() {
    let rom = rom_of(fixture());
    assert!(written(&rom, &[]).expect("no roots is no work").is_empty());
    let err = decode_trainer_defeat_speech_payload(&rom.reader(), &[]).unwrap_err();
    assert!(mismatch(&err, "trainer defeat speech roots"), "{err}");
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
    let renamed = {
        let mut roots = roots();
        roots[4].symbol = "Route103_Text_Other";
        roots
    };
    let unset = {
        let mut roots = roots();
        roots[4].addr = GbaPtr::AT_BASE;
        roots
    };
    for (bad, expected) in [
        (partial, "trainer defeat speech roots"),
        (surplus, "trainer defeat speech roots"),
        (reordered, "trainer defeat speech symbol"),
        (renamed, "trainer defeat speech symbol"),
        (unset, "trainer defeat speech address"),
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
    let rom = rom_of(fixture().write(ROM_SIZE - 3, &[0xC0, 0xC0, 0xC0]));
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
    let rom = rom_of(fixture().write(stream_at(12), &[0xFC, 0x08, 0xFF]));
    let err = written(&rom, leak(roots())).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::TrainerDefeatSpeech {
                id: PACK_ID,
                source: TrainerDefeatSpeechError::MalformedString { id: 12, .. }
            }
        ),
        "{err}"
    );
}

#[test]
fn the_witness_selects_the_copy_the_script_names() {
    // Two identical copies; the script names the decoy, not the first.
    let rom = rom_of(fixture().write(COMMAND, &command(at(DECOY))));
    let mut decoy = roots();
    decoy[ISABELLE].addr = at(DECOY);
    assert!(written(&rom, leak(decoy)).is_ok());

    // The first copy no longer matches the pointer, though its bytes do.
    let err = written(&rom, leak(roots())).unwrap_err();
    assert!(
        mismatch(&err, "trainer defeat speech text pointer"),
        "{err}"
    );
}

#[test]
fn a_missing_or_stray_witness_is_refused() {
    let rom = rom_of(fixture());
    let mut missing = roots();
    missing[ISABELLE].script = None;
    let err = written(&rom, leak(missing)).unwrap_err();
    assert!(mismatch(&err, "trainer defeat speech script"), "{err}");

    let mut unset = roots();
    unset[ISABELLE].script = Some(GbaPtr::AT_BASE);
    let err = written(&rom, leak(unset)).unwrap_err();
    assert!(mismatch(&err, "trainer defeat speech script"), "{err}");

    let mut stray = roots();
    stray[0].script = Some(at(COMMAND));
    let err = written(&rom, leak(stray)).unwrap_err();
    assert!(mismatch(&err, "trainer defeat speech script"), "{err}");
}

#[test]
fn a_witness_that_is_not_isabelles_trainerbattle_is_refused() {
    let good = command(at(stream_at(ISABELLE)));
    // Opcode, battle type, either trainer byte, either local-ID byte.
    for index in 0..COMMAND_HEAD_LEN {
        let mut bad = good.clone();
        bad[index] ^= 1;
        let rom = rom_of(fixture().write(COMMAND, &bad));
        let err = written(&rom, leak(roots())).unwrap_err();
        assert!(
            mismatch(&err, "trainer defeat speech command"),
            "{index}: {err}"
        );
    }
}

#[test]
fn a_witness_command_off_the_image_or_with_a_bad_pointer_is_refused() {
    // The command is cut by the end of the image.
    let rom = rom_of(fixture().write(ROM_SIZE - 8, &command(at(stream_at(ISABELLE)))[..8]));
    let mut cut = roots();
    cut[ISABELLE].script = Some(at(ROM_SIZE - 8));
    let err = written(&rom, leak(cut)).unwrap_err();
    assert!(matches!(err, ImportError::Truncated { .. }), "{err}");

    // The defeat pointer is not a cartridge address.
    let mut bad = command(at(stream_at(ISABELLE)));
    bad[DEFEAT_POINTER_OFFSET..].copy_from_slice(&0x0200_0000u32.to_le_bytes());
    let rom = rom_of(fixture().write(COMMAND, &bad));
    let err = written(&rom, leak(roots())).unwrap_err();
    assert!(
        matches!(err, ImportError::PointerOutOfRange { at, .. } if at == COMMAND + DEFEAT_POINTER_OFFSET),
        "{err}"
    );
}
