//! Indirection selects the correct leaf, pitch key, and rhythm pan without nested resolution.

use super::test_support::*;
use super::*;
use crate::song::{rhythm_pan_from_pan_sweep, KeySplit, Rhythm, RhythmChild, KEY_SLOTS};

// --- Key-split / rhythm indirection (`TONEDATA_TYPE_SPL`/`_RHY`) -------

#[test]
fn key_split_boundary_selects_the_correct_child() {
    let mut table = [0u8; KEY_SLOTS];
    for slot in table.iter_mut().skip(64) {
        *slot = 1;
    }
    let split = Instrument::KeySplit(KeySplit {
        table,
        children: vec![Some(direct_sound(40)), Some(direct_sound(100))],
    });

    let render = |key: u8| {
        let track = vec![
            Event::Voice(0),
            Event::Note {
                key,
                velocity: 127,
                gate: 8,
            },
            Event::Wait(48),
            Event::Fine,
        ];
        let song = Song::new(vec![split.clone()], vec![track], 150);
        let mut seq = Sequencer::new(song);
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        seq.render_frame(&mut out);
        out
    };

    let key_below_split = render(30);
    let key_at_or_above_split = render(90);
    assert_ne!(
        key_below_split, key_at_or_above_split,
        "the split boundary must select different children"
    );
    let magnitude = |buf: &[f32]| buf.iter().map(|s| s.abs()).sum::<f32>();
    assert!(
        magnitude(&key_at_or_above_split) > magnitude(&key_below_split),
        "key 90 must select the louder (sample 100) child, not the quieter one"
    );
}

#[test]
fn key_split_keeps_the_played_key_for_pitch() {
    let mut table = [0u8; KEY_SLOTS];
    for slot in table.iter_mut().skip(64) {
        *slot = 1;
    }
    let split = Instrument::KeySplit(KeySplit {
        table,
        children: vec![Some(direct_sound(40)), Some(direct_sound(100))],
    });
    for &key in &[30u8, 90u8] {
        let track = vec![
            Event::Voice(0),
            Event::Note {
                key,
                velocity: 127,
                gate: 8,
            },
            Event::Wait(48),
            Event::Fine,
        ];
        let song = Song::new(vec![split.clone()], vec![track], 150);
        let mut seq = Sequencer::new(song);
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        seq.render_frame(&mut out);
        assert_eq!(
            seq.mixer.voices()[0].frequency(),
            pitch::midi_key_to_freq(1 << 20, key, 0),
            "key-split pitch must use the played key {key}, not any child override"
        );
    }
}

#[test]
fn rhythm_indirection_selects_child_by_played_key_directly() {
    let mut children: Vec<Option<RhythmChild>> = vec![None; KEY_SLOTS];
    children[36] = Some(RhythmChild {
        instrument: direct_sound(90),
        base_key: 72,
        pan: None,
    });
    let rhythm = Instrument::Rhythm(Rhythm { children });

    let track_for = |key: u8| {
        vec![
            Event::Voice(0),
            Event::Note {
                key,
                velocity: 127,
                gate: 8,
            },
            Event::Wait(48),
            Event::Fine,
        ]
    };

    let song = Song::new(vec![rhythm.clone()], vec![track_for(36)], 150);
    let mut seq = Sequencer::new(song);
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
    seq.render_frame(&mut out);
    assert_eq!(seq.voice_count(), 1, "a populated rhythm slot must sound");

    let song = Song::new(vec![rhythm], vec![track_for(37)], 150);
    let mut seq = Sequencer::new(song);
    seq.render_frame(&mut out);
    assert_eq!(
        seq.voice_count(),
        0,
        "an unpopulated rhythm slot must produce no note, not panic or fall back"
    );
}

#[test]
fn rhythm_child_base_key_overrides_pitch() {
    let mut children: Vec<Option<RhythmChild>> = vec![None; KEY_SLOTS];
    children[36] = Some(RhythmChild {
        instrument: direct_sound(90),
        base_key: 72,
        pan: None,
    });
    let rhythm = Instrument::Rhythm(Rhythm { children });
    let track = vec![
        Event::Voice(0),
        Event::Note {
            key: 36,
            velocity: 127,
            gate: 8,
        },
        Event::Wait(48),
        Event::Fine,
    ];
    let song = Song::new(vec![rhythm], vec![track], 150);
    let mut seq = Sequencer::new(song);
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
    seq.render_frame(&mut out);
    assert_eq!(
        seq.mixer.voices()[0].frequency(),
        pitch::midi_key_to_freq(1 << 20, 72, 0),
        "rhythm pitch must come from the child's base key, not the played key"
    );
}

#[test]
fn rhythm_child_pan_override_is_applied_when_the_bit_is_set() {
    let mut children: Vec<Option<RhythmChild>> = vec![None; KEY_SLOTS];
    let hard_right_pan_override = rhythm_pan_from_pan_sweep(0xFF);
    children[36] = Some(RhythmChild {
        instrument: direct_sound(90),
        base_key: 36,
        pan: hard_right_pan_override,
    });
    let rhythm = Instrument::Rhythm(Rhythm { children });
    let track = vec![
        Event::Voice(0),
        Event::Note {
            key: 36,
            velocity: 127,
            gate: 8,
        },
        Event::Wait(48),
        Event::Fine,
    ];
    let song = Song::new(vec![rhythm], vec![track], 150);
    let mut seq = Sequencer::new(song);
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
    seq.render_frame(&mut out);
    let (right, left) = seq.mixer.voices()[0].base_volume();
    assert!(
        right > left,
        "a rhythm pan override toward the right must skew the channel volumes ({right} vs {left})"
    );
}

#[test]
fn nested_key_split_or_rhythm_child_produces_no_note() {
    let inner_rhythm = Instrument::Rhythm(Rhythm {
        children: vec![None; KEY_SLOTS],
    });
    let table = [0u8; KEY_SLOTS];
    let split = Instrument::KeySplit(KeySplit {
        table,
        children: vec![Some(inner_rhythm)],
    });
    let track = vec![
        Event::Voice(0),
        Event::Note {
            key: 10,
            velocity: 127,
            gate: 8,
        },
        Event::Wait(48),
        Event::Fine,
    ];
    let song = Song::new(vec![split], vec![track], 150);
    let mut seq = Sequencer::new(song);
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
    seq.render_frame(&mut out);
    assert_eq!(seq.voice_count(), 0, "nested indirection must not sound");
}
