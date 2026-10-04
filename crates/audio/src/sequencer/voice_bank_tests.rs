//! Normalized voice slots and priority-based allocation preserve bank and gate-order semantics.

// The reciprocal wave-frequency the test song derives narrows to `u32` (well
// within range for these inputs); silence/pan checks compare exact `0.0`.
#![allow(clippy::cast_possible_truncation, clippy::float_cmp)]

use super::test_support::*;
use super::*;
use crate::cgb_envelope::CgbAdsr;
use crate::song::{SquareTone, KEY_SLOTS};

// --- Normalized 128-slot voice banks -------------------------------------

#[test]
fn voice_slot_127_is_an_explicit_entry_not_an_adjacent_lookup() {
    let build_voices = |slot127_sample: i8| {
        let mut voices: Vec<Instrument> = (0..127).map(|_| direct_sound(10)).collect();
        voices.push(direct_sound(slot127_sample));
        voices
    };
    assert_eq!(build_voices(0).len(), KEY_SLOTS);

    let render = |slot127_sample: i8| {
        let voices = build_voices(slot127_sample);
        let track = vec![
            Event::Voice(127),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 8,
            },
            Event::Wait(48),
            Event::Fine,
        ];
        let song = Song::new(voices, vec![track], 150);
        assert!(song.voice(127).is_some(), "slot 127 must be populated");
        assert!(
            song.voice(128).is_none(),
            "index 128 must be out of range, not an adjacent wraparound"
        );
        let mut seq = Sequencer::new(song);
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        seq.render_frame(&mut out);
        out
    };

    assert_ne!(
        render(10),
        render(120),
        "slot 127 must read its own explicit entry, not an adjacent, wrapped, or default slot"
    );
}

fn stamped_priority(song_priority: u8, track_priority: u8) -> u8 {
    let track = vec![
        Event::Voice(0),
        Event::Priority(track_priority),
        Event::Note {
            key: 60,
            velocity: 127,
            gate: 8,
        },
        Event::Wait(48),
        Event::Fine,
    ];
    let song = test_song(vec![track], 150).with_priority(song_priority);
    let mut seq = Sequencer::new(song);
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
    seq.render_frame(&mut out);
    seq.mixer.voices()[0].priority()
}

#[test]
fn note_priority_adds_the_song_and_track_halves() {
    assert_eq!(stamped_priority(0, 0), 0);
    assert_eq!(stamped_priority(30, 0), 30);
    assert_eq!(stamped_priority(0, 40), 40);
    assert_eq!(stamped_priority(30, 40), 70);
}

#[test]
fn note_priority_saturates_instead_of_wrapping() {
    assert_eq!(stamped_priority(200, 100), 255);
    assert_eq!(stamped_priority(255, 1), 255);
    assert_eq!(stamped_priority(255, 255), 255);
}

#[test]
fn a_prio_command_only_affects_later_notes() {
    let track = vec![
        Event::Voice(0),
        Event::Note {
            key: 60,
            velocity: 127,
            gate: 8,
        },
        Event::Priority(90),
        Event::Note {
            key: 64,
            velocity: 127,
            gate: 8,
        },
        Event::Wait(48),
        Event::Fine,
    ];
    let mut seq = Sequencer::new(test_song(vec![track], 150).with_priority(5));
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
    seq.render_frame(&mut out);
    let stamped: Vec<(u8, u8)> = seq
        .mixer
        .voices()
        .iter()
        .map(|voice| (voice.midi_key(), voice.priority()))
        .collect();
    assert_eq!(stamped, vec![(60, 5), (64, 95)]);
}

#[test]
fn a_low_priority_track_loses_its_note_when_the_pool_is_full() {
    // A refused note finds every pool channel outranking it
    // (`m4a_1.s:1669`..`:1718`).
    let track_with_priority = |priority: u8| {
        vec![
            Event::Voice(0),
            Event::Priority(priority),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 8,
            },
            Event::Wait(48),
            Event::Fine,
        ]
    };
    let mut tracks: Vec<Vec<Event>> = (0..5).map(|_| track_with_priority(100)).collect();
    tracks.push(track_with_priority(1));
    let mut seq = Sequencer::new(test_song(tracks, 150));
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
    seq.render_frame(&mut out);

    assert_eq!(seq.voice_count(), DEFAULT_MAX_VOICES);
    assert!(
        seq.mixer
            .voices()
            .iter()
            .all(|voice| voice.track().is_some_and(|track| track < 5)),
        "the sixth track's weaker note must never have started"
    );
}

#[test]
fn later_track_gate_expiry_does_not_change_earlier_track_slot_selection() {
    // Upstream expires a track's gates during that track's own pass
    // (`m4a_1.s:1191`-`:1212`), so track 1's gate-1 voice is still live
    // when track 0 allocates: track 0 steals its own weaker tied voice.
    let tracks = vec![
        vec![
            Event::Priority(10),
            tied_note(50),
            Event::Wait(1),
            Event::Priority(20),
            tied_note(64),
            Event::Wait(8),
        ],
        vec![
            Event::Priority(20),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 1,
            },
            Event::Wait(8),
        ],
    ];
    let mut seq = Sequencer::with_config(test_song(tracks, 150), DEFAULT_MASTER_VOLUME, 2);
    seq.do_tick();
    assert_eq!(seq.mixer.voices()[0].midi_key(), 50);
    assert_eq!(seq.mixer.voices()[1].midi_key(), 60);
    assert!(!seq.mixer.voices()[1].is_stopping());

    seq.do_tick();
    let occupants: Vec<_> = seq
        .mixer
        .voices()
        .iter()
        .map(|v| (v.track(), v.midi_key(), v.priority(), v.is_stopping()))
        .collect();
    assert_eq!(
        occupants,
        vec![(Some(0), 64, 20, false), (Some(1), 60, 20, true)]
    );
}

#[test]
fn higher_priority_later_track_voice_survives_earlier_track_allocation_on_expiry_tick() {
    let tracks = vec![
        vec![
            Event::Priority(10),
            Event::Wait(1),
            tied_note(64),
            Event::Wait(8),
        ],
        vec![
            Event::Priority(20),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 1,
            },
            Event::Wait(8),
        ],
    ];
    let mut seq = Sequencer::with_config(test_song(tracks, 150), DEFAULT_MASTER_VOLUME, 1);
    seq.do_tick();
    seq.do_tick();
    let occupants: Vec<_> = seq
        .mixer
        .voices()
        .iter()
        .map(|v| (v.track(), v.midi_key()))
        .collect();
    assert_eq!(occupants, vec![(Some(1), 60)]);
}

#[test]
fn higher_priority_later_track_cgb_voice_survives_earlier_track_allocation_on_expiry_tick() {
    let voices = vec![Instrument::CgbSquare1(SquareTone {
        duty: 2,
        sweep: 0,
        adsr: CgbAdsr::flat(),
        fixed_rate: false,
    })];
    let tracks = vec![
        vec![
            Event::Voice(0),
            Event::Priority(10),
            Event::Wait(1),
            tied_note(64),
            Event::Wait(8),
        ],
        vec![
            Event::Voice(0),
            Event::Priority(20),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 1,
            },
            Event::Wait(8),
        ],
    ];
    let mut seq = Sequencer::new(Song::new(voices, tracks, 150));
    seq.do_tick();
    seq.do_tick();
    let square1 = seq.mixer.cgb_voices()[CgbChannelNumber::Square1.slot()]
        .as_ref()
        .expect("track 1's voice must still occupy Square1");
    assert_eq!((square1.track(), square1.midi_key()), (1, 60));
    assert!(square1.is_stopping());
}
