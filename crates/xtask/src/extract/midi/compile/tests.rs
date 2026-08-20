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
    body.extend([0xB0, 0x1E, 10]); // an unrecognized sub-command
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

/// A `Wnn`-fixed-point gap after CC `0x1E` is dropped whole, not emitted
/// as a `Wait` -- `agb.cpp:402-405`'s `case 0x1E:` is the one
/// `PrintControllerOp` arm that `break`s without a trailing
/// `PrintWait(event.time)`, `SplitTime` inserts no `TimeSplit` into a
/// 24-tick gap (`g_noteDurationLUT[24] == 24`), and this compiler
/// reproduces both (`super::super::translate`'s module docs, "Reproduced:
/// the dropped wait after CC `0x1E`"). If the 24-tick gap between the
/// selector and the trigger had survived, the waits between `Note` and
/// `Fine` would sum to `34` (`4 + 24 + 6`), not `10`.
#[test]
fn a_nonzero_gap_after_cc_0x1e_is_dropped_not_emitted() {
    let mut body = Vec::new();
    body.extend(vlq(0));
    body.extend([0x90, 60, 100]); // note-on, t=0
    body.extend(vlq(4));
    body.extend([0xB0, 0x1E, 8]); // select xIECV, t=4
    body.extend(vlq(24)); // this gap must not surface as a Wait
    body.extend([0x1D, 10]); // trigger (running status 0xB0), t=28
    body.extend(vlq(6));
    body.extend([0x80, 60, 0]); // note-off, t=34
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    assert_eq!(
        compiled.tracks[0],
        vec![
            SongEvent::Volume(127),
            SongEvent::KeyShift(0),
            SongEvent::Note {
                key: 60,
                velocity: 100,
                gate: 34
            },
            SongEvent::Wait(4),
            SongEvent::PseudoEchoVolume(10),
            SongEvent::Wait(6),
            SongEvent::Fine,
        ]
    );
}

/// An off-grid gap after CC `0x1E` loses only its `g_noteDurationLUT`
/// floor, not the whole gap: `SplitTime` (`midi.cpp:714-722`) puts a
/// `TimeSplit` at the 27-tick gap's LUT floor (`g_noteDurationLUT[27] ==
/// 24`, `tables.cpp:52`), so the selector's own dropped wait is `24` and
/// the `TimeSplit` prints the remaining `W03` (`agb.cpp:518-520`).
/// Dropping the whole gap here would shift everything after the selector
/// `3` extra ticks early.
#[test]
fn an_off_grid_gap_after_cc_0x1e_keeps_its_remainder_past_the_lut_floor() {
    let mut body = Vec::new();
    body.extend(vlq(0));
    body.extend([0x90, 60, 100]); // note-on, t=0
    body.extend(vlq(4));
    body.extend([0xB0, 0x1E, 8]); // select xIECV, t=4
    body.extend(vlq(27)); // dropped only up to the LUT floor (24)
    body.extend([0x1D, 10]); // trigger (running status 0xB0), t=31
    body.extend(vlq(6));
    body.extend([0x80, 60, 0]); // note-off, t=37
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    assert_eq!(
        compiled.tracks[0],
        vec![
            SongEvent::Volume(127),
            SongEvent::KeyShift(0),
            SongEvent::Note {
                key: 60,
                velocity: 100,
                gate: 37
            },
            SongEvent::Wait(4),
            SongEvent::Wait(3),
            SongEvent::PseudoEchoVolume(10),
            SongEvent::Wait(6),
            SongEvent::Fine,
        ]
    );
}

/// A gap spanning a whole-note grid line after CC `0x1E` loses only the
/// stretch up to that line's LUT floor: `InsertTimingEvents`
/// (`midi.cpp:653-686`) put a wait-printing timing mark at absolute tick
/// `96`, so the 100-tick gap from the selector at `19` is cut at `77` --
/// then `SplitTime` floors that off-grid `77` to `g_noteDurationLUT[77] ==
/// 76`, making `76` the selector's own dropped wait. The `TimeSplit` at
/// `95` prints `W01` and the mark at `96` prints `W23`: `24` ticks
/// survive. The pre-grid-walk revision of `emit_track` dropped `96` here.
#[test]
fn a_gap_spanning_a_grid_line_after_cc_0x1e_is_cut_at_the_grid_lut_floor() {
    let mut body = Vec::new();
    body.extend(vlq(0));
    body.extend([0x90, 60, 100]); // note-on, t=0
    body.extend(vlq(4));
    body.extend([60, 0]); // note-off (running status), t=4
    body.extend(vlq(15));
    body.extend([0xB0, 0x1E, 9]); // select xIECL, t=19
    body.extend(vlq(100)); // dropped only up to LUT[96 - 19] == 76
    body.extend([0x1F, 12]); // trigger xIECL (running status 0xB0), t=119
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
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
            SongEvent::Wait(19),
            SongEvent::Wait(24),
            SongEvent::PseudoEchoLength(12),
            SongEvent::Fine,
        ]
    );
}

/// A CC `0x1E` sitting just before a whole-note grid line loses only the
/// ticks up to that line, whatever the gap: the timing mark at absolute
/// tick `96` is `6` ticks after the selector at `90`, `g_noteDurationLUT[6]
/// == 6`, so `6` is dropped and the mark prints the remaining `W14` --
/// even though the raw 20-tick gap is itself a `Wnn` fixed point. Using
/// the raw gap here would drop all `20`.
#[test]
fn cc_0x1e_near_a_grid_line_loses_only_the_ticks_up_to_it() {
    let mut body = Vec::new();
    body.extend(vlq(0));
    body.extend([0x90, 60, 100]); // note-on, t=0
    body.extend(vlq(4));
    body.extend([60, 0]); // note-off (running status), t=4
    body.extend(vlq(86));
    body.extend([0xB0, 0x1E, 8]); // select xIECV, t=90
    body.extend(vlq(20)); // dropped only up to the grid line at 96
    body.extend([0x1D, 10]); // trigger (running status 0xB0), t=110
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
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
            SongEvent::Wait(90),
            SongEvent::Wait(14),
            SongEvent::PseudoEchoVolume(10),
            SongEvent::Fine,
        ]
    );
}

/// A file time-signature event re-phases the whole-note grid that bounds
/// the CC `0x1E` drop (`midi.cpp:671-679`): a `2/4` signature at tick `0`
/// puts the next timing mark at `48`, so a selector at `40` with a 30-tick
/// gap loses only the `8` ticks to that mark (`g_noteDurationLUT[8] == 8`)
/// and `W22` survives. Under the default `96` grid the whole `30` would
/// have been a fixed-point drop.
#[test]
fn a_time_signature_re_phases_the_grid_bounding_the_cc_0x1e_drop() {
    let mut body = Vec::new();
    body.extend(vlq(0));
    body.extend([0xFF, 0x58, 0x04, 2, 2, 24, 8]); // 2/4 time signature, t=0
    body.extend(vlq(0));
    body.extend([0x90, 60, 100]); // note-on, t=0
    body.extend(vlq(4));
    body.extend([60, 0]); // note-off (running status), t=4
    body.extend(vlq(36));
    body.extend([0xB0, 0x1E, 8]); // select xIECV, t=40
    body.extend(vlq(30)); // dropped only up to the re-phased mark at 48
    body.extend([0x1D, 10]); // trigger (running status 0xB0), t=70
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
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
            SongEvent::Wait(40),
            SongEvent::Wait(22),
            SongEvent::PseudoEchoVolume(10),
            SongEvent::Fine,
        ]
    );
}

/// A time signature whose whole-note grid period works out to zero ticks
/// (`1/128`: `96 * 1 >> 7 == 0`) fails closed -- upstream's own
/// `timeSig <= 0` guard (`midi.cpp:333-334`), enforced here at compile
/// time where `clocks_per_beat` is known, since a zero period would stall
/// `emit_track`'s timing-mark walk.
#[test]
fn a_zero_period_time_signature_is_an_error() {
    let mut body = Vec::new();
    body.extend(vlq(0));
    body.extend([0xFF, 0x58, 0x04, 1, 7, 24, 8]); // 1/128 time signature
    body.extend(vlq(0));
    body.extend([0x90, 60, 100]); // note-on, t=0
    body.extend(vlq(4));
    body.extend([60, 0]); // note-off (running status), t=4
    let midi = single_track_midi(24, body);

    assert_eq!(
        compile(&midi, &cfg()).unwrap_err(),
        MidiError::ZeroTimeSignature
    );
}

/// A silent controller (upstream's bare-`PrintWait` arms) between a CC
/// `0x1E` and the next emitted event keeps its own wait: upstream drops
/// only the selector's own gap (`agb.cpp:402-405`), while the unknown CC's
/// `default:` arm still prints its wait. Dropping silent controllers from
/// the item list entirely would fold both gaps into one suppressed wait,
/// shifting everything after the selector `10` extra ticks early here.
#[test]
fn a_silent_controller_after_cc_0x1e_keeps_its_own_wait() {
    let mut body = Vec::new();
    body.extend(vlq(0));
    body.extend([0x90, 60, 100]); // note-on, t=0
    body.extend(vlq(4));
    body.extend([0xB0, 0x1E, 8]); // select xIECV, t=4
    body.extend(vlq(10)); // the selector's own gap: dropped
    body.extend([0x50, 3]); // unknown CC 0x50 (running status 0xB0), t=14
    body.extend(vlq(10)); // the silent controller's gap: kept
    body.extend([0x1D, 10]); // trigger, t=24
    body.extend(vlq(6));
    body.extend([0x80, 60, 0]); // note-off, t=30
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    assert_eq!(
        compiled.tracks[0],
        vec![
            SongEvent::Volume(127),
            SongEvent::KeyShift(0),
            SongEvent::Note {
                key: 60,
                velocity: 100,
                gate: 30
            },
            SongEvent::Wait(4),
            SongEvent::Wait(10),
            SongEvent::PseudoEchoVolume(10),
            SongEvent::Wait(6),
            SongEvent::Fine,
        ]
    );
}

/// Waits that do not immediately follow a CC `0x1E` are unaffected by the
/// drop above -- both the ordinary wait leading up to the selector and an
/// ordinary (`> u8::MAX`, so still split) wait well after the triggered
/// command survive untouched; only the one gap `agb.cpp:402-405` itself
/// swallows disappears.
#[test]
fn waits_not_adjacent_to_cc_0x1e_are_unaffected() {
    let mut body = Vec::new();
    body.extend(vlq(0));
    body.extend([0x90, 60, 100]); // note-on, t=0
    body.extend(vlq(4));
    body.extend([60, 0]); // note-off (running status), t=4
    body.extend(vlq(10));
    body.extend([0xB0, 0x15, 7]); // LFOS 7, t=14 -- ordinary wait before it
    body.extend(vlq(5));
    body.extend([0x1E, 9]); // select xIECL (running status 0xB0), t=19
    body.extend(vlq(40)); // this gap must not surface as a Wait
    body.extend([0x1F, 12]); // trigger xIECL, t=59
    body.extend(vlq(300)); // ordinary wait, still split at u8::MAX
    body.extend([0x01, 55]); // MOD 55, t=359
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
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
            SongEvent::Wait(14),
            SongEvent::LfoSpeed(7),
            SongEvent::Wait(5),
            SongEvent::PseudoEchoLength(12),
            SongEvent::Wait(u8::MAX),
            SongEvent::Wait(45),
            SongEvent::Modulation(55),
            SongEvent::Fine,
        ]
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

/// A channel whose every note-on is cancelled at the very same tick is not
/// playable and emits no track at all — `midi.cpp:484-490` only lets a
/// note-on touch `s_minNote` `if (event.param2 > 0)` (raw tick duration),
/// and `:933` gates the whole track on `s_minNote != 0xFF` (module docs,
/// "Which channels become tracks").
///
/// The consequence this pins is not just the missing track: channel 0's
/// phantom track would have been the *first* one, and so would have taken
/// the song's `Tempo` with it (`include_tempo`), leaving the one real track
/// tempo-less. Both halves are asserted.
#[test]
fn a_channel_of_zero_duration_notes_emits_no_track() {
    let mut body = Vec::new();
    body.extend(vlq(0));
    body.extend([0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20]); // 500_000 us/qn = 120 BPM
    body.extend(vlq(0));
    body.extend([0x90, 60, 100]); // channel 0 note-on ...
    body.extend(vlq(0));
    body.extend([0x80, 60, 0]); // ... cancelled at the same tick
    body.extend(vlq(0));
    body.extend([0x91, 64, 100]); // channel 1, a real note
    body.extend(vlq(4));
    body.extend([0x81, 64, 0]);
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    assert_eq!(compiled.tracks.len(), 1);
    assert!(compiled.tracks[0].contains(&SongEvent::Tempo(120)));
    assert!(compiled.tracks[0].contains(&SongEvent::Note {
        key: 64,
        velocity: 100,
        gate: 4,
    }));
}

/// A channel that carries only controllers and no note-on at all is not a
/// track either — the same gate, from the other direction.
#[test]
fn a_channel_with_no_notes_emits_no_track() {
    let mut body = Vec::new();
    body.extend(vlq(0));
    body.extend([0xB0, 7, 100]); // channel 0: a volume controller, no notes
    body.extend(vlq(0));
    body.extend([0x91, 64, 100]);
    body.extend(vlq(4));
    body.extend([0x81, 64, 0]);
    let midi = single_track_midi(24, body);

    assert_eq!(compile(&midi, &cfg()).unwrap().tracks.len(), 1);
}

/// `convert_ticks` evaluates `24 * raw` in `u64`: `24 * 0x0FFF_FFFF` is
/// `0x1_7FFF_FFE8`, past `u32::MAX`, so the old `u32` expression panicked
/// in debug and wrapped in release on a tick a *legal* four-byte VLQ can
/// carry. Widened, the quotient is exact whenever it fits, and a
/// [`MidiError::TickOverflow`] when it does not — never a panic, never a
/// silent wrap (`super::super::translate::convert_ticks`'s docs).
#[test]
fn convert_ticks_evaluates_the_multiply_in_u64() {
    use crate::extract::midi::translate::convert_ticks;

    assert_eq!(convert_ticks(0x0FFF_FFFF, 24).unwrap(), 0x0FFF_FFFF);
    assert_eq!(
        convert_ticks(0x0FFF_FFFF, 1).unwrap_err(),
        MidiError::TickOverflow(0x0FFF_FFFF)
    );
}

/// The end-to-end regression for the same overflow: a crafted `.mid` whose
/// note-off sits a full four-byte VLQ (`0x0FFF_FFFF` ticks, the largest a
/// standard MIDI file can encode) after its note-on, at a `division` of `1`
/// so the scaled result cannot fit a `u32`. This must be a returned error,
/// not a panic — the never-panic contract `super::super::reader` and
/// `super::super::translate` share.
#[test]
fn a_tick_that_overflows_once_scaled_is_an_error_not_a_panic() {
    let mut body = Vec::new();
    body.extend(vlq(0));
    body.extend([0x90, 60, 100]);
    body.extend(vlq(0x0FFF_FFFF));
    body.extend([0x80, 60, 0]);
    let midi = single_track_midi(1, body);

    assert_eq!(
        compile(&midi, &cfg()).unwrap_err(),
        MidiError::TickOverflow(0x0FFF_FFFF)
    );
}

/// The timing-grid walk once saturated `next_mark` at `u32::MAX`: a mark
/// that can never exceed a `u32::MAX` item time left `while next_mark <=
/// time` spinning forever. Reachable with division 24 (converted time ==
/// raw tick) and legal VLQ deltas summing to `u32::MAX`, most immediately
/// via a time-signature event at that tick. The walk now fails closed with
/// [`MidiError::TickOverflow`] instead of hanging extraction.
#[test]
fn a_grid_mark_past_u32_max_is_an_error_not_a_hang() {
    let mut body = Vec::new();
    body.extend(vlq(0));
    body.extend([0x90, 60, 100]); // note-on so the channel emits a track
    body.extend(vlq(0x0F));
    body.extend([0x80, 60, 0]); // note-off at tick 0x0F
    for _ in 0..16 {
        // 16 maximal four-byte VLQs: 0x0F + 16 * 0x0FFF_FFFF == u32::MAX.
        // Text metas carry the deltas without emitting items of their own.
        body.extend(vlq(0x0FFF_FFFF));
        body.extend([0xFF, 0x01, 0x00]);
    }
    body.extend(vlq(0)); // time signature at exactly u32::MAX
    body.extend([0xFF, 0x58, 0x04, 0x04, 0x02, 0x18, 0x08]);
    body.extend(vlq(0));
    body.extend([0xB0, 7, 100]); // an item at u32::MAX after the saturated mark
    let midi = single_track_midi(24, body);

    assert_eq!(
        compile(&midi, &cfg()).unwrap_err(),
        MidiError::TickOverflow(u32::MAX)
    );
}

/// The mark walk must be arithmetic, not iterative: a valid `1/64` time
/// signature makes the grid period `1`, so an event near `u32::MAX` ticks
/// (legal VLQ deltas, division 24) would otherwise advance the mark
/// billions of times before reaching the event or the overflow report.
/// This input must fail closed immediately — wall-clock, not eventually.
#[test]
fn a_unit_grid_period_reaches_a_distant_event_without_iterating() {
    let mut body = Vec::new();
    body.extend(vlq(0));
    body.extend([0xFF, 0x58, 0x04, 0x01, 0x06, 0x18, 0x08]); // 1/64: period 1
    body.extend(vlq(0));
    body.extend([0x90, 60, 100]);
    body.extend(vlq(0x0F));
    body.extend([0x80, 60, 0]);
    for _ in 0..16 {
        // 0x0F + 16 * 0x0FFF_FFFF == u32::MAX (text metas carry the deltas).
        body.extend(vlq(0x0FFF_FFFF));
        body.extend([0xFF, 0x01, 0x00]);
    }
    body.extend(vlq(0));
    body.extend([0xB0, 7, 100]); // an item at u32::MAX, 2^32 marks away
    let midi = single_track_midi(24, body);

    assert_eq!(
        compile(&midi, &cfg()).unwrap_err(),
        MidiError::TickOverflow(u32::MAX)
    );
}

#[test]
fn malformed_tempo_and_velocity_are_errors_at_the_compile_boundary() {
    let zero_tempo = single_track_midi(
        24,
        vec![
            0x00, 0xFF, 0x51, 0x03, 0x00, 0x00, 0x00, // zero microseconds/qn
            0x00, 0x90, 60, 100, // note-on
            0x01, 0x80, 60, 0, // note-off
        ],
    );
    assert_eq!(
        compile(&zero_tempo, &cfg()).unwrap_err(),
        MidiError::ZeroTempo
    );

    let invalid_velocity = single_track_midi(
        24,
        vec![
            0x00, 0x90, 60, 128, // velocity has its status bit set
            0x01, 0x80, 60, 0,
        ],
    );
    assert_eq!(
        compile(&invalid_velocity, &cfg()).unwrap_err(),
        MidiError::InvalidDataByte(128)
    );
}

#[test]
fn channel_events_are_scaled_to_the_output_timebase() {
    let mut body = Vec::new();
    body.extend(vlq(0));
    body.extend([0x90, 60, 100]); // note at output tick 0
    body.extend(vlq(12));
    body.extend([0xC0, 3]); // program change at output tick 6
    body.extend(vlq(12));
    body.extend([0xB0, 1, 7]); // modulation at output tick 12
    body.extend(vlq(12));
    body.extend([0xE0, 0, 80]); // bend +16 at output tick 18
    body.extend(vlq(12));
    body.extend([0x80, 60, 0]); // note-off/final boundary at output tick 24
    let midi = single_track_midi(48, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    assert_eq!(
        compiled.tracks[0],
        vec![
            SongEvent::Volume(127),
            SongEvent::KeyShift(0),
            SongEvent::Note {
                key: 60,
                velocity: 100,
                gate: 24,
            },
            SongEvent::Wait(6),
            SongEvent::Voice(3),
            SongEvent::Wait(6),
            SongEvent::Modulation(7),
            SongEvent::Wait(6),
            SongEvent::Bend(16),
            SongEvent::Wait(6),
            SongEvent::Fine,
        ]
    );
}

#[test]
fn an_over_long_zero_delta_preserves_the_prior_absolute_tick() {
    use crate::extract::midi::parse::{parse_track, RawEvent};

    let mut body = Vec::new();
    body.extend(vlq(1));
    body.extend([0x90, 60, 100]);
    body.extend([0x90, 0x80, 0x80, 0x80, 0x00]);
    body.extend([0xC0, 5]);
    body.extend(vlq(24));
    body.extend([0x80, 60, 0]);

    let mut parse_body = body.clone();
    parse_body.extend([0x00, 0xFF, 0x2F, 0x00]);
    let parsed = parse_track(&parse_body).unwrap();
    assert_eq!(
        parsed.events[1],
        (
            1,
            RawEvent::ProgramChange {
                channel: 0,
                program: 5,
            },
        )
    );

    let compiled = compile(&single_track_midi(24, body), &cfg()).unwrap();
    assert_eq!(
        compiled.tracks[0],
        vec![
            SongEvent::Volume(127),
            SongEvent::Wait(1),
            SongEvent::KeyShift(0),
            SongEvent::Voice(5),
            SongEvent::Note {
                key: 60,
                velocity: 100,
                gate: 24,
            },
            SongEvent::Wait(24),
            SongEvent::Fine,
        ]
    );
}
