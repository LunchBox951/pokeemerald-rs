use super::{compile, MidiError, SongEvent};
use crate::extract::midi::cfg::MidiCfgEntry;

fn vlq(mut value: u32) -> Vec<u8> {
    let mut groups = vec![(value & 0x7F) as u8];
    value >>= 7;
    while value > 0 {
        groups.push((value & 0x7F) as u8 | 0x80);
        value >>= 7;
    }
    groups.reverse();
    groups
}

fn mthd(format: u16, track_count: u16, division: u16) -> Vec<u8> {
    let mut out = b"MThd".to_vec();
    out.extend(6u32.to_be_bytes());
    out.extend(format.to_be_bytes());
    out.extend(track_count.to_be_bytes());
    out.extend(division.to_be_bytes());
    out
}

fn mtrk(mut body: Vec<u8>) -> Vec<u8> {
    body.extend([0x00, 0xFF, 0x2F, 0x00]); // end of track
    let mut out = b"MTrk".to_vec();
    let len = u32::try_from(body.len()).expect("test track bodies are tiny");
    out.extend(len.to_be_bytes());
    out.extend(body);
    out
}

/// A one-`MTrk` (format 0) `.mid` file: `track_body` carries both the
/// tempo/loop-marker "sequence" content and the note/controller "track"
/// content on one channel, matching how upstream's own `ReadMidiTracks`
/// treats a single-chunk file (`super::super::parse`'s module docs).
fn single_track_midi(division: u16, track_body: Vec<u8>) -> Vec<u8> {
    let mut file = mthd(0, 1, division);
    file.extend(mtrk(track_body));
    file
}

fn cfg() -> MidiCfgEntry {
    MidiCfgEntry {
        voicegroup_label: "test".to_owned(),
        priority: 0,
        reverb: None,
        master_volume: 127,
        exact_gate_time: true,
        clocks_per_beat: 1,
    }
}

/// A minimal single-note track: `KeyShift(0)`, the synthetic full-volume
/// preamble (no CC7 precedes the note), the note itself, then `Fine`.
#[test]
fn a_single_note_compiles_to_the_expected_event_stream() {
    let mut body = Vec::new();
    body.extend(vlq(0));
    body.extend([0x90, 60, 100]); // note-on, channel 0, key 60, velocity 100
    body.extend(vlq(24));
    body.extend([60, 0]); // note-off (running status), 24 ticks later
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    assert_eq!(compiled.tracks.len(), 1);
    assert_eq!(
        compiled.tracks[0],
        vec![
            SongEvent::Volume(127), // 127 * 127 / 127 = 127 (no CC7 precedes the note)
            SongEvent::KeyShift(0),
            SongEvent::Note {
                key: 60,
                velocity: 100,
                gate: 24
            }, // 100 is already a LUT fixed point
            // Trailing wait until the track's own end-of-track tick (24) --
            // a note's `gate` and the track's own "wait until the next
            // command" are independent axes (module docs, "Sort order"
            // links to `agb.cpp:417-525`'s `PrintAgbTrack`): nothing else
            // happens on this track between the note and end of track, so
            // the sequencer waits out that whole span before `FINE`.
            SongEvent::Wait(24),
            SongEvent::Fine,
        ]
    );
}

/// A volume controller before the note suppresses the synthetic preamble,
/// and is itself scaled by `midi.cfg`'s master volume.
#[test]
fn a_volume_controller_before_the_note_suppresses_the_synthetic_preamble() {
    let mut body = Vec::new();
    body.extend(vlq(0));
    body.extend([0xB0, 7, 100]); // CC7 volume = 100
    body.extend(vlq(0));
    body.extend([0x90, 60, 96]); // velocity 96 is already a LUT fixed point
    body.extend(vlq(10));
    body.extend([0x80, 60, 0]);
    let midi = single_track_midi(24, body);

    let mut entry = cfg();
    entry.master_volume = 90;
    let compiled = compile(&midi, &entry).unwrap();
    assert_eq!(
        compiled.tracks[0],
        vec![
            SongEvent::KeyShift(0),
            SongEvent::Volume(70), // 100 * 90 / 127 = 70 (truncating)
            SongEvent::Note {
                key: 60,
                velocity: 96,
                gate: 10
            },
            SongEvent::Wait(10),
            SongEvent::Fine,
        ]
    );
}

/// A note longer than 96 ticks is split into a tie-start (`gate: 0`) and a
/// later `EndOfTie`, always carrying an explicit key (module docs, "No
/// operand elision").
#[test]
fn a_long_note_is_tie_split_with_an_explicit_end_of_tie_key() {
    let mut body = Vec::new();
    body.extend(vlq(0));
    body.extend([0x90, 67, 80]);
    body.extend(vlq(120)); // > 96 ticks
    body.extend([67, 0]);
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    assert_eq!(
        compiled.tracks[0],
        vec![
            SongEvent::Volume(127),
            SongEvent::KeyShift(0),
            SongEvent::Note {
                key: 67,
                velocity: 80,
                gate: 0
            },
            SongEvent::Wait(120),
            SongEvent::EndOfTie { key: 67 },
            SongEvent::Fine,
        ]
    );
}

/// A rest longer than `u8::MAX` ticks is split into multiple `Wait` events
/// (this schema's own wire constraint — module docs, "`Wait` is a free tick
/// count, not a 49-value enum").
#[test]
fn a_gap_over_255_ticks_is_split_into_multiple_waits() {
    let mut body = Vec::new();
    body.extend(vlq(0));
    body.extend([0x90, 60, 100]);
    body.extend(vlq(1)); // very short first note so its own gate stays small
    body.extend([60, 0]);
    body.extend(vlq(300)); // gap until the next note exceeds u8::MAX
    body.extend([0x90, 62, 100]);
    body.extend(vlq(1));
    body.extend([62, 0]);
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    let waits: Vec<u8> = compiled.tracks[0]
        .iter()
        .filter_map(|e| match e {
            SongEvent::Wait(t) => Some(*t),
            _ => None,
        })
        .collect();
    // Between the two notes: 255 + 45 = 300.
    assert!(waits.contains(&255));
    assert_eq!(
        waits.iter().map(|&w| u32::from(w)).sum::<u32>(),
        1 + 300 + 1
    );
}

/// A loop-begin/loop-end marker pair compiles to a backward `Goto` whose
/// target is the event index right after the loop-begin marker.
#[test]
fn loop_markers_compile_to_a_backward_goto() {
    let mut body = Vec::new();
    body.extend(vlq(0));
    body.extend([0xFF, 0x01, 1]); // "[" -- loop begin
    body.extend(b"[");
    body.extend(vlq(0));
    body.extend([0x90, 60, 100]);
    body.extend(vlq(4));
    body.extend([60, 0]);
    body.extend(vlq(0));
    body.extend([0xFF, 0x01, 1]); // "]" -- loop end
    body.extend(b"]");
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    // The loop-begin marker (t=0) sorts before the note (t=0, same tick --
    // module docs, "Sort order": a marker's type priority is lower than
    // `Note`'s), so it records index 2 (right after `Volume`/`KeyShift`,
    // before the note is pushed) as the loop target. `LoopEnd`'s `Goto`
    // then targets that same index.
    assert_eq!(
        compiled.tracks[0],
        vec![
            SongEvent::Volume(127),
            SongEvent::KeyShift(0),
            SongEvent::Note {
                key: 60,
                velocity: 100,
                gate: 4
            },
            SongEvent::Wait(4),
            SongEvent::Goto(2),
            SongEvent::Fine,
        ]
    );
}

#[test]
fn a_loop_end_with_no_matching_loop_begin_is_an_error() {
    let mut body = Vec::new();
    body.extend(vlq(0));
    body.extend([0x90, 60, 100]);
    body.extend(vlq(4));
    body.extend([60, 0]);
    body.extend(vlq(0));
    body.extend([0xFF, 0x01, 1]);
    body.extend(b"]");
    let midi = single_track_midi(24, body);

    let err = compile(&midi, &cfg()).unwrap_err();
    assert_eq!(err, MidiError::DanglingLoopEnd);
}

/// `XCMD` (`0x1E` selects the sub-command, `0x1D`/`0x1F` triggers it) maps
/// `8` to `PseudoEchoVolume` and any other sub-command to a silent no-op —
/// upstream's own behaviour (module docs, "Extended commands"), not a scope
/// cut.
#[test]
fn xcmd_pseudo_echo_volume_round_trips_and_unknown_subcommands_are_silent() {
    let mut body = Vec::new();
    body.extend(vlq(0));
    body.extend([0x90, 60, 100]);
    body.extend(vlq(4));
    body.extend([0xB0, 0x1E, 8]); // select xIECV
    body.extend(vlq(0));
    body.extend([0xB0, 0x1D, 10]); // trigger with value 10
    body.extend(vlq(0));
    body.extend([0xB0, 0x1E, 200]); // an unrecognized sub-command
    body.extend(vlq(0));
    body.extend([0xB0, 0x1F, 5]); // triggering it is a silent no-op
    body.extend(vlq(4));
    body.extend([0x80, 60, 0]); // explicit status: running status is 0xB0 here
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    assert!(compiled.tracks[0].contains(&SongEvent::PseudoEchoVolume(10)));
    assert_eq!(
        compiled.tracks[0]
            .iter()
            .filter(|e| matches!(
                e,
                SongEvent::PseudoEchoVolume(_) | SongEvent::PseudoEchoLength(_)
            ))
            .count(),
        1
    );
}

/// The `MEMACC` controller family is explicitly unsupported, not silently
/// dropped (module docs, "`MEMACC` controllers").
#[test]
fn memacc_controllers_are_a_hard_error() {
    for controller in [0x0Cu8, 0x0D, 0x0E, 0x0F, 0x10, 0x11] {
        let mut body = Vec::new();
        body.extend(vlq(0));
        body.extend([0x90, 60, 100]);
        body.extend(vlq(4));
        body.extend([0xB0, controller, 1]);
        body.extend(vlq(0));
        body.extend([0x80, 60, 0]); // explicit status: running status is 0xB0 here
        let midi = single_track_midi(24, body);
        let err = compile(&midi, &cfg()).unwrap_err();
        assert_eq!(err, MidiError::UnsupportedMemAccController(controller));
    }
}

#[test]
fn an_unterminated_note_is_an_error() {
    let mut body = Vec::new();
    body.extend(vlq(0));
    body.extend([0x90, 60, 100]); // never followed by a matching note-off
    let midi = single_track_midi(24, body);
    let err = compile(&midi, &cfg()).unwrap_err();
    assert_eq!(
        err,
        MidiError::UnterminatedNote {
            channel: 0,
            key: 60
        }
    );
}

#[test]
fn non_exact_gate_time_is_rejected() {
    let midi = single_track_midi(24, {
        let mut b = Vec::new();
        b.extend(vlq(0));
        b.extend([0x90, 60, 100]);
        b.extend(vlq(1));
        b.extend([60, 0]);
        b
    });
    let mut entry = cfg();
    entry.exact_gate_time = false;
    let err = compile(&midi, &entry).unwrap_err();
    assert_eq!(err, MidiError::NonExactGateTime);
}

#[test]
fn unsupported_clocks_per_beat_is_rejected() {
    let midi = single_track_midi(24, {
        let mut b = Vec::new();
        b.extend(vlq(0));
        b.extend([0x90, 60, 100]);
        b.extend(vlq(1));
        b.extend([60, 0]);
        b
    });
    let mut entry = cfg();
    entry.clocks_per_beat = 2;
    let err = compile(&midi, &entry).unwrap_err();
    assert_eq!(err, MidiError::UnsupportedClocksPerBeat(2));
}

/// Two channels each carrying a note become two tracks, in ascending
/// channel order, and only the first gets the seq track's `Tempo`
/// (`midi.cpp:941-946`).
#[test]
fn tempo_only_reaches_the_first_agb_track() {
    let mut body = Vec::new();
    body.extend(vlq(0));
    body.extend([0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20]); // 500_000 us/qn = 120 BPM
    body.extend(vlq(0));
    body.extend([0x90, 60, 100]); // channel 0
    body.extend(vlq(4));
    body.extend([60, 0]);
    body.extend(vlq(0));
    body.extend([0x91, 64, 100]); // channel 1
    body.extend(vlq(4));
    body.extend([0x81, 64, 0]);
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    assert_eq!(compiled.tracks.len(), 2);
    assert!(compiled.tracks[0].contains(&SongEvent::Tempo(120)));
    assert!(!compiled.tracks[1]
        .iter()
        .any(|e| matches!(e, SongEvent::Tempo(_))));
}

#[test]
fn song_level_metadata_is_carried_from_cfg() {
    let midi = single_track_midi(24, {
        let mut b = Vec::new();
        b.extend(vlq(0));
        b.extend([0x90, 60, 100]);
        b.extend(vlq(1));
        b.extend([60, 0]);
        b
    });
    let mut entry = cfg();
    entry.voicegroup_label = "title".to_owned();
    entry.priority = 3;
    entry.reverb = Some(50);
    let compiled = compile(&midi, &entry).unwrap();
    assert_eq!(compiled.voicegroup_label, "title");
    assert_eq!(compiled.priority, 3);
    assert_eq!(compiled.reverb, Some(50));
}
