//! Defaults, tempo, rendering, pan, and resolved reverb obey sequencer contracts.

// The reciprocal wave-frequency the test song derives narrows to `u32` (well
// within range for these inputs); silence/pan checks compare exact `0.0`.
#![allow(clippy::cast_possible_truncation, clippy::float_cmp)]

use super::test_support::*;
use super::*;
use crate::sequence::decode_track;

#[test]
fn new_uses_emerald_init_defaults() {
    // `m4aSoundInit` reconfigures the driver away from the generic
    // `SoundInit` placeholders (`m4a.c:78`..`:81`).
    assert_eq!(DEFAULT_MASTER_VOLUME, 12);
    assert_eq!(DEFAULT_MAX_VOICES, 5);
    let seq = Sequencer::new(test_song(vec![vec![Event::Fine]], 150));
    assert_eq!(seq.mixer.master_volume(), 12);
    assert_eq!(seq.mixer.max_voices(), 5);
}

#[test]
fn track_state_new_uses_the_documented_defaults() {
    let track = TrackState::new();
    assert_eq!(track.vol, DEFAULT_TRACK_VOLUME);
    assert_eq!(track.bend_range, DEFAULT_BEND_RANGE);
    assert_eq!(track.lfo_speed, DEFAULT_LFO_SPEED);
    assert_eq!(track.pan, 0);
    assert_eq!(track.priority, 0);
    assert!(!track.ended);
}

#[test]
fn silent_song_renders_zero() {
    let song = test_song(vec![vec![Event::Fine]], 150);
    let mut seq = Sequencer::new(song);
    let mut out = vec![7.0; Sequencer::FRAME_SAMPLES];
    seq.render_frame(&mut out);
    assert!(out.iter().all(|&s| s == 0.0));
    assert!(seq.is_finished());
}

#[test]
fn a_note_produces_sound_then_the_track_ends() {
    let track = vec![
        Event::Voice(0),
        Event::Note {
            key: 60,
            velocity: 127,
            gate: 4,
        },
        Event::Wait(48),
        Event::Fine,
    ];
    let song = test_song(vec![track], 150);
    let mut seq = Sequencer::new(song);

    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
    seq.render_frame(&mut out);
    assert!(out.iter().any(|&s| s.abs() > 0.0));
    assert_eq!(seq.voice_count(), 1);

    for _ in 0..64 {
        seq.render_frame(&mut out);
    }
    assert!(
        seq.is_finished(),
        "the wait must drain and FINE must end the track"
    );
}

#[test]
fn finite_reverbed_song_finishes_only_after_tail_drains() {
    let track = vec![
        Event::Voice(0),
        Event::Note {
            key: 60,
            velocity: 127,
            gate: 1,
        },
        Event::Wait(2),
        Event::Fine,
    ];
    let song = test_song(vec![track], 150).with_reverb(100);
    let mut seq = Sequencer::new(song);
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];

    for _ in 0..32 {
        seq.render_frame(&mut out);
        if seq.tracks.iter().all(|track| track.ended) && seq.mixer.is_idle() {
            break;
        }
    }

    assert!(seq.tracks.iter().all(|track| track.ended));
    assert!(seq.mixer.is_idle());
    assert!(
        seq.mixer.has_pending_reverb(),
        "the dry note must leave delayed samples in the reverb ring"
    );
    assert!(
        !seq.is_finished(),
        "ended tracks and inactive voices are not finished while reverb is pending"
    );

    let mut heard_wet_tail = false;
    for _ in 0..1000 {
        seq.render_frame(&mut out);
        heard_wet_tail |= out.iter().any(|&sample| sample != 0.0);
        if seq.is_finished() {
            break;
        }
    }

    assert!(heard_wet_tail, "the pending reverb must produce wet output");
    assert!(
        seq.is_finished(),
        "a finite reverb tail must eventually decay to silence"
    );
    assert!(!seq.mixer.has_pending_reverb());
}

#[test]
fn with_resolved_reverb_applies_its_explicit_level_over_the_songs_own_header() {
    // Carries a session's previously configured reverb across a header
    // that never set one (`Song::reverb` collapses that case to `0`).
    let track = vec![
        Event::Voice(0),
        Event::Note {
            key: 60,
            velocity: 127,
            gate: 1,
        },
        Event::Wait(2),
        Event::Fine,
    ];
    let song = test_song(vec![track], 150);
    assert_eq!(song.reverb_override(), None);
    let mut seq =
        Sequencer::with_resolved_reverb(song, DEFAULT_MASTER_VOLUME, DEFAULT_MAX_VOICES, 100);
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];

    for _ in 0..32 {
        seq.render_frame(&mut out);
        if seq.tracks.iter().all(|track| track.ended) && seq.mixer.is_idle() {
            break;
        }
    }

    assert!(
        seq.mixer.has_pending_reverb(),
        "the resolved reverb level must be the one actually applied to the mixer"
    );
}

#[test]
fn gate_time_releases_the_note() {
    let track = vec![
        Event::Voice(0),
        Event::Note {
            key: 60,
            velocity: 127,
            gate: 2,
        },
        Event::Wait(64),
        Event::Fine,
    ];
    let song = test_song(vec![track], 150);
    let mut seq = Sequencer::new(song);
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];

    seq.render_frame(&mut out);
    assert_eq!(
        seq.voice_count(),
        1,
        "the note must start before its gate can release it"
    );

    // Tempo 150 ticks once per frame, so gate 2 releases well within 7.
    for _ in 0..7 {
        seq.render_frame(&mut out);
    }
    assert_eq!(seq.voice_count(), 0);
}

#[test]
fn goto_loops_the_track_forever() {
    let track = vec![
        Event::Voice(0),
        Event::Note {
            key: 60,
            velocity: 127,
            gate: 1,
        },
        Event::Wait(24),
        Event::Goto(1),
    ];
    let song = test_song(vec![track], 150);
    let mut seq = Sequencer::new(song);
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
    for _ in 0..200 {
        seq.render_frame(&mut out);
    }
    assert!(!seq.is_finished());
}

#[test]
fn wait_goto_and_voice_commands_update_track_control_state() {
    let mut sequencer = Sequencer::new(test_song(vec![vec![Event::Fine]], 150));

    apply_test_event(&mut sequencer, 0, &Event::Wait(24));
    apply_test_event(&mut sequencer, 0, &Event::Goto(17));
    apply_test_event(&mut sequencer, 0, &Event::Voice(3));

    assert_eq!(sequencer.tracks[0].wait, 24);
    assert_eq!(sequencer.tracks[0].cursor, 17);
    assert_eq!(sequencer.tracks[0].voice, 3);
}

#[test]
fn decoded_only_port_and_xcmd_leave_track_state_unchanged() {
    let mut sequencer = Sequencer::new(test_song(vec![vec![Event::Fine]], 150));
    let original = sequencer.tracks[0].clone();

    apply_test_event(
        &mut sequencer,
        0,
        &Event::Port {
            control: 2,
            value: 127,
        },
    );
    apply_test_event(
        &mut sequencer,
        0,
        &Event::Xcmd {
            kind: 0,
            value: 127,
        },
    );

    assert_eq!(sequencer.tracks[0], original);
}

#[test]
fn faster_tempo_reaches_the_end_in_fewer_frames() {
    let track = || {
        vec![
            Event::Voice(0),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 8,
            },
            Event::Wait(12),
            Event::Fine,
        ]
    };
    let frames_to_finish = |tempo: u16| {
        let mut seq = Sequencer::new(test_song(vec![track()], tempo));
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        let mut frames = 0;
        while !seq.is_finished() && frames < 1000 {
            seq.render_frame(&mut out);
            frames += 1;
        }
        frames
    };
    assert!(frames_to_finish(300) < frames_to_finish(150));
}

#[test]
fn advance_frame_ticks_only_on_the_frame_that_crosses_tempo_unit() {
    let track = vec![Event::Wait(5), Event::Fine];
    let mut seq = Sequencer::new(test_song(vec![track], 100));

    seq.advance_frame();
    assert_eq!(seq.tempo_c, 100, "one frame below TEMPO_UNIT must not tick");
    assert_eq!(
        seq.tracks[0].wait, 0,
        "no tick fired, so the track hasn't run yet"
    );

    seq.advance_frame();
    assert_eq!(
        seq.tempo_c, 50,
        "the crossing frame ticks once, dropping tempo_c by TEMPO_UNIT"
    );
    assert_eq!(
        seq.tracks[0].wait, 4,
        "the same tick decoded Wait(5) and consumed one unit"
    );
}

#[test]
fn tempo_event_is_clamped_before_it_can_overflow_the_accumulator() {
    // `tempo_c += tempo_i` (`advance_frame`) is unguarded, so a
    // malformed asset pack's out-of-domain `Event::Tempo` must never
    // reach `tempo_i` un-clamped.
    let mut seq = Sequencer::new(test_song(vec![vec![Event::Fine]], 150));
    apply_test_event(&mut seq, 0, &Event::Tempo(u16::MAX));
    assert_eq!(
        seq.tempo_i, MAX_TEMPO_BPM,
        "an out-of-domain Tempo event must clamp to the TEMPO command's real bound"
    );

    // Drive tempo_c to the highest value `advance_frame`'s drain loop
    // ever leaves behind and take one more frame: an unclamped
    // `tempo_i` of `u16::MAX` would overflow this addition, but the
    // clamp above keeps the sum far under `u16::MAX`.
    seq.tempo_c = TEMPO_UNIT - 1;
    seq.advance_frame();
    assert_eq!(seq.tempo_c, (TEMPO_UNIT - 1 + MAX_TEMPO_BPM) % TEMPO_UNIT);
}

/// [`Event::Pan`]'s range minimum.
const HARD_LEFT_PAN: i8 = -64;

#[test]
fn panned_note_is_louder_on_one_side() {
    let track = vec![
        Event::Voice(0),
        Event::Pan(HARD_LEFT_PAN),
        Event::Note {
            key: 60,
            velocity: 127,
            gate: 8,
        },
        Event::Wait(48),
        Event::Fine,
    ];
    let song = test_song(vec![track], 150);
    let mut seq = Sequencer::new(song);
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
    seq.render_frame(&mut out);
    let left: f32 = out.iter().step_by(2).map(|s| s.abs()).sum();
    let right: f32 = out.iter().skip(1).step_by(2).map(|s| s.abs()).sum();
    assert!(left > right, "left {left} should exceed right {right}");
    assert_eq!(right, 0.0);
}

#[test]
fn decoded_bytes_drive_the_engine_end_to_end() {
    // Decode a real byte program and play it: VOICE 0; N04 key60 vel127;
    // W48; FINE.
    let bytes = [0xBD, 0x00, 0xD3, 60, 127, 0xB0, 0xB1];
    let events = decode_track(&bytes).unwrap();
    let song = test_song(vec![events], 150);
    let mut seq = Sequencer::new(song);
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
    seq.render_frame(&mut out);
    assert!(out.iter().any(|&s| s.abs() > 0.0));
}

#[test]
fn mix_into_renders_multiple_frames() {
    let track = vec![
        Event::Voice(0),
        Event::Note {
            key: 60,
            velocity: 127,
            gate: 90,
        },
        Event::Wait(96),
        Event::Fine,
    ];
    let song = test_song(vec![track], 150);
    let mut seq = Sequencer::new(song);
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES * 3];
    seq.mix_into(&mut out);
    for (i, frame) in out.chunks(Sequencer::FRAME_SAMPLES).enumerate() {
        assert!(
            frame.iter().any(|&s| s.abs() > 0.0),
            "frame {i} of 3 must carry the held note's audio"
        );
    }
}

#[test]
#[should_panic(expected = "multiple of FRAME_SAMPLES")]
fn mix_into_rejects_partial_frames() {
    let song = test_song(vec![vec![Event::Fine]], 150);
    let mut seq = Sequencer::new(song);
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES + 1];
    seq.mix_into(&mut out);
}

#[test]
#[should_panic(expected = "multiple of FRAME_SAMPLES")]
fn mix_into_rejects_an_empty_buffer() {
    let song = test_song(vec![vec![Event::Fine]], 150);
    let mut seq = Sequencer::new(song);
    let mut out: Vec<f32> = vec![];
    seq.mix_into(&mut out);
}

/// [`Sequencer::with_resolved_reverb`] clamps its level to the
/// `SOUND_MODE_REVERB_VAL` domain (`0..=127`) exactly as
/// [`Song::with_reverb`] does at the header boundary: an out-of-range
/// `255` must behave as `127`, never as unclamped comb feedback.
#[test]
fn an_out_of_range_resolved_reverb_level_clamps_to_the_canonical_maximum() {
    let track = || {
        vec![
            Event::Voice(0),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 8,
            },
            Event::Wait(48),
            Event::Fine,
        ]
    };
    let song = || test_song(vec![track()], 150);
    let mut clamped = Sequencer::with_resolved_reverb(song(), 15, 8, 255);
    let mut canonical = Sequencer::with_resolved_reverb(song(), 15, 8, 127);
    let mut a = vec![0.0; Sequencer::FRAME_SAMPLES];
    let mut b = vec![0.0; Sequencer::FRAME_SAMPLES];
    for _ in 0..8 {
        clamped.render_frame(&mut a);
        canonical.render_frame(&mut b);
        assert_eq!(a, b, "255 must render exactly as the clamped 127");
    }
}
