//! Song decoder tests over a synthetic ROM.
//!
//! The fixture plants `gSongTable`, one `SongHeader`, and hand-assembled
//! tracks that exercise each rule the engine's `MPlayMain` applies:
//! running status, operand elision on notes, `PATT` expansion, `GOTO`
//! and branch pointers, the centred operands, and every fault the decoder
//! refuses.

use assets::audio::{MemAccCondition, MemAccOp};
use assets::{Song, SongEvent, VoiceGroupId};

use super::{decode_track, song, write};
use crate::error::{ImportError, SongFault};
use crate::fixture::RomFixture;
use crate::reader::{GbaPtr, ROM_BASE};
use crate::rom::Rom;
use crate::roots::{AudioRoots, Roots, SongRoot, VoicegroupRoot};

const fn at(off: u32) -> GbaPtr {
    GbaPtr::at(ROM_BASE + off)
}

const SONG_TABLE: u32 = 0x1000;
const HEADER: u32 = 0x1100;
const VOICEGROUP: u32 = 0x1200;
/// Track 0: the preamble, running status, notes with elided operands.
const TRACK_A: u32 = 0x2000;
/// Track 1: a pattern, a loop, and a branch.
const TRACK_B: u32 = 0x2100;
const FAULTS: u32 = 0x3000;

/// Track A, as `mid2agb` would print it.
fn track_a() -> Vec<u8> {
    vec![
        0xBC, 0x00, // KEYSH 0
        0xBB, 0x48, // TEMPO 72 (144 BPM)
        0xBD, 0x0E, // VOICE 14
        0xBF, 0x68, // PAN c_v+40
        0x20, // running status: PAN c_v-32
        0xC0, 0x40, // BEND c_v
        0xC8, 0x3F, // TUNE c_v-1
        0xBE, 0x56, // VOL 86
        0x98, // W24
        0xEF, 0x39, 0x78, // N48 key 57 velocity 120
        0xA0, // W48
        0x35, // running status: N48 key 53, velocity elided
        0xA0, // W48 (an elided operand needs a command byte after it)
        0x3C, 0x70, 0x02, // running status: N48 key 60 velocity 112, gate +2
        0xA0, // W48
        0xE7, // N24, key and velocity elided
        0xCF, 0x40, // TIE key 64
        0xCE, // EOT, key elided: the last key, 64
        0xCE, 0x40, // EOT key 64
        0xCD, 0x08, 0x10, // XCMD xIECV 16
        0xCD, 0x09, 0x04, // XCMD xIECL 4
        0xBA, 0x02, // PRIO 2
        0xC1, 0x0C, // BENDR 12
        0xC2, 0x2C, // LFOS 44
        0xC3, 0x01, // LFODL 1
        0xC4, 0x05, // MOD 5
        0xC5, 0x01, // MODT 1
        0xB9, 0x00, 0x01, 0x07, // MEMACC mem_set 1 7
        0xB1, // FINE
    ]
}

/// The events track A decodes to, before canonicalization.
fn track_a_events() -> Vec<SongEvent> {
    vec![
        SongEvent::KeyShift(0),
        SongEvent::Tempo(144),
        SongEvent::Voice(14),
        SongEvent::Pan(40),
        SongEvent::Pan(-32),
        SongEvent::Bend(0),
        SongEvent::Tune(-1),
        SongEvent::Volume(86),
        SongEvent::Wait(24),
        SongEvent::Note {
            key: 57,
            velocity: 120,
            gate: 48,
        },
        SongEvent::Wait(48),
        SongEvent::Note {
            key: 53,
            velocity: 120,
            gate: 48,
        },
        SongEvent::Wait(48),
        SongEvent::Note {
            key: 60,
            velocity: 112,
            gate: 50,
        },
        SongEvent::Wait(48),
        SongEvent::Note {
            key: 60,
            velocity: 112,
            gate: 24,
        },
        SongEvent::Note {
            key: 64,
            velocity: 112,
            gate: 0,
        },
        SongEvent::EndOfTie { key: Some(64) },
        SongEvent::EndOfTie { key: Some(64) },
        SongEvent::PseudoEchoVolume(16),
        SongEvent::PseudoEchoLength(4),
        SongEvent::Priority(2),
        SongEvent::BendRange(12),
        SongEvent::LfoSpeed(44),
        SongEvent::LfoDelay(1),
        SongEvent::Modulation(5),
        SongEvent::ModType(1),
        SongEvent::MemAcc {
            op: MemAccOp::Set,
            address: 1,
            data: 7,
        },
        SongEvent::Fine,
    ]
}

/// Track B: a pattern body played inline then called again, a loop back to
/// the label after the preamble, and a branch to the same place.
fn track_b() -> Vec<u8> {
    let body = at(TRACK_B + 4).raw().to_le_bytes();
    let loop_target = at(TRACK_B + 2).raw().to_le_bytes();
    let mut bytes = vec![
        0xBD, 0x01, // VOICE 1                      (offset 0)
        0xBE, 0x7F, // VOL 127                      (offset 2: loop target)
        0xD0, 0x3C, 0x64, // N01 60 100             (offset 4: pattern body)
        0x81, // W01
        0xB4, // PEND, a no-op at top level
        0xB3, body[0], body[1], body[2], body[3], // PATT body
        0x82,    // W02
    ];
    bytes.extend([0xB9, 0x06, 0x00, 0x01]); // MEMACC mem_beq 0 1
    bytes.extend(loop_target);
    bytes.push(0xB2); // GOTO
    bytes.extend(loop_target);
    bytes.push(0xB1); // FINE
    bytes
}

fn track_b_events() -> Vec<SongEvent> {
    let note = SongEvent::Note {
        key: 60,
        velocity: 100,
        gate: 1,
    };
    vec![
        SongEvent::Voice(1),
        SongEvent::Volume(127),
        note.clone(),
        SongEvent::Wait(1),
        note,
        SongEvent::Wait(1),
        SongEvent::Wait(2),
        SongEvent::MemAccBranch {
            condition: MemAccCondition::Eq,
            address: 0,
            data: 1,
            target: 1,
        },
        SongEvent::Goto(1),
        SongEvent::Fine,
    ]
}

/// Byte streams that each trip one fault, at `FAULTS + 0x10 * n`.
const FAULT_STREAMS: [&[u8]; 6] = [
    &[0x3C, 0xB1],                         // an operand with no running status
    &[0xB6, 0xB1],                         // an unused jump-table entry
    &[0xCD, 0x0D, 0x00, 0xB1],             // an unmodelled XCMD
    &[0xB9, 0x12, 0, 0, 0xB1],             // MEMACC op 18
    &[0xB5, 0x00, 0, 0, 0, 0],             // REPT
    &[0xB2, 0x00, 0x00, 0x00, 0x09, 0xB1], // GOTO to another track
];

fn rom() -> Rom {
    // Entry 0 is some other song's header; entry 1 is ours.
    let mut table = at(TRACK_A).raw().to_le_bytes().to_vec();
    table.extend([0, 0, 0, 0]);
    table.extend(at(HEADER).raw().to_le_bytes());
    table.extend([0, 0, 0, 0]);
    let mut header = vec![2, 0, 5, 0x80 | 0x32];
    header.extend(at(VOICEGROUP).raw().to_le_bytes());
    header.extend(at(TRACK_A).raw().to_le_bytes());
    header.extend(at(TRACK_B).raw().to_le_bytes());
    // A pattern nested three deep, for the depth check.
    let deep = at(FAULTS + 0x60).raw().to_le_bytes();
    let mut nested = vec![0xB3];
    nested.extend(deep);
    let mut fixture = RomFixture::new()
        .emerald_header()
        .write(SONG_TABLE as usize, &table)
        .write(HEADER as usize, &header)
        .write(TRACK_A as usize, &track_a())
        .write(TRACK_B as usize, &track_b())
        .write(FAULTS as usize + 0x60, &nested);
    for (n, stream) in FAULT_STREAMS.iter().enumerate() {
        fixture = fixture.write(FAULTS as usize + 0x10 * n, stream);
    }
    Rom::from_bytes(fixture.finish()).expect("the fixture header is valid")
}

static VOICEGROUPS: [VoicegroupRoot; 1] = [VoicegroupRoot {
    id: "audio/voicegroup/g",
    label: "g",
    addr: at(VOICEGROUP),
    starting_note: 0,
    declared_slots: 1,
    addressable_slots: 1,
}];
static SONGS: [SongRoot; 1] = [SongRoot {
    id: "audio/song/s",
    index: 1,
    header: at(HEADER),
    track_count: 2,
    voicegroup: at(VOICEGROUP),
}];

fn audio() -> AudioRoots {
    AudioRoots {
        song_table: at(SONG_TABLE),
        songs: &SONGS,
        voicegroups: &VOICEGROUPS,
        keysplits: &[],
        direct_sound: &[],
        programmable_wave: &[],
    }
}

#[test]
fn a_track_decodes_to_the_engines_reading_of_it() {
    let rom = rom();
    let events = decode_track(&rom.reader(), "audio/song/s", 0, at(TRACK_A)).unwrap();
    assert_eq!(events, track_a_events());
}

#[test]
fn patterns_expand_inline_and_jumps_become_indices() {
    let rom = rom();
    let events = decode_track(&rom.reader(), "audio/song/s", 1, at(TRACK_B)).unwrap();
    assert_eq!(events, track_b_events());
}

#[test]
fn the_song_carries_its_header_and_canonical_tracks() {
    let rom = rom();
    let entry = song(&rom.reader(), &audio(), &SONGS[0]).expect("a well-formed song");
    let decoded = Song::decode(&entry.payload).unwrap();
    assert_eq!(
        decoded.voicegroup(),
        &VoiceGroupId("audio/voicegroup/g".into())
    );
    assert_eq!(decoded.priority(), 5);
    assert_eq!(decoded.reverb(), Some(50));
    let expected = Song::new(
        VoiceGroupId("audio/voicegroup/g".into()),
        5,
        Some(50),
        vec![track_a_events(), track_b_events()],
    )
    .unwrap();
    assert_eq!(decoded, expected);
    // Canonicalization merged track B's two rests.
    assert_eq!(decoded.tracks()[1][5], SongEvent::Wait(3));
}

#[test]
fn the_domain_writes_one_entry_per_song() {
    let rom = rom();
    let roots = Roots {
        audio: audio(),
        ..Roots::NONE
    };
    let mut writer = pack_format::PackWriter::new();
    write(&rom, &roots, &mut writer).expect("a well-formed table");
    assert_eq!(writer.len(), 1);
}

#[test]
fn a_table_entry_or_header_that_disagrees_is_refused() {
    let rom = rom();
    let wrong_index = SongRoot {
        index: 0,
        ..SONGS[0]
    };
    let err = song(&rom.reader(), &audio(), &wrong_index).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::StructMismatch {
                field: "Song.header",
                ..
            }
        ),
        "{err}"
    );
    let wrong_count = SongRoot {
        track_count: 3,
        ..SONGS[0]
    };
    let err = song(&rom.reader(), &audio(), &wrong_count).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::StructMismatch {
                field: "SongHeader.trackCount",
                ..
            }
        ),
        "{err}"
    );
}

#[test]
fn each_fault_is_named() {
    let rom = rom();
    let expected = [
        SongFault::NoRunningStatus,
        SongFault::UnknownCommand(0xB6),
        SongFault::UnknownExtendedCommand(0x0D),
        SongFault::UnknownMemAccOp(18),
        SongFault::Repeat,
        SongFault::JumpOutsideTrack,
    ];
    for (n, fault) in expected.into_iter().enumerate() {
        let start = at(FAULTS + 0x10 * u32::try_from(n).unwrap());
        let err = decode_track(&rom.reader(), "audio/song/s", 0, start).unwrap_err();
        assert!(
            matches!(err, ImportError::Song { fault: got, .. } if got == fault),
            "stream {n}: {err}"
        );
    }
}

#[test]
fn a_pattern_nested_past_the_engines_stack_is_refused() {
    let rom = rom();
    let err = decode_track(&rom.reader(), "audio/song/s", 0, at(FAULTS + 0x60)).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::Song {
                fault: SongFault::PatternTooDeep,
                ..
            }
        ),
        "{err}"
    );
}

#[test]
fn a_track_with_no_fine_is_refused() {
    // The fixture's unwritten space reads as rests, so a track started
    // there walks to the event limit without ever meeting `FINE`.
    let rom = rom();
    let err = decode_track(&rom.reader(), "audio/song/s", 0, at(0x8000)).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::Song {
                fault: SongFault::NoFine,
                ..
            }
        ),
        "{err}"
    );
}
