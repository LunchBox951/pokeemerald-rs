//! DirectSound fixed-rate playback ignores played-key frequency changes.

// The reciprocal wave-frequency the test song derives narrows to `u32` (well
// within range for these inputs); silence/pan checks compare exact `0.0`.
#![allow(clippy::cast_possible_truncation, clippy::float_cmp)]

use std::sync::Arc;

use super::*;
use crate::envelope::Adsr;
use crate::sample::WaveData;
use crate::song::ToneData;

// --- Fixed-rate DirectSound (`TONEDATA_TYPE_FIX`) -----------------------

/// A short, varying, looping waveform: a constant wave can't distinguish
/// "sampled at a different rate" from "sampled at the same rate", since
/// every source sample reads back the same value regardless of pitch.
fn varying_wave() -> Arc<WaveData> {
    Arc::new(WaveData::looping(
        1 << 20,
        0,
        vec![100, -100, 50, -50, 30, -30, 10, -10],
    ))
}

#[test]
fn fixed_rate_instrument_renders_identically_regardless_of_played_key() {
    let render = |key: u8| {
        let tone = ToneData::new(varying_wave(), Adsr::flat()).fixed();
        let voices = vec![Instrument::DirectSound(tone)];
        let track = vec![
            Event::Voice(0),
            Event::Note {
                key,
                velocity: 127,
                gate: 90,
            },
            Event::Wait(96),
            Event::Fine,
        ];
        let song = Song::new(voices, vec![track], 150);
        let mut seq = Sequencer::new(song);
        let mut buf = vec![0.0; Sequencer::FRAME_SAMPLES * 3];
        seq.mix_into(&mut buf);
        buf
    };
    assert_eq!(
        render(40),
        render(90),
        "a fixed-rate instrument must ignore the played note's pitch entirely"
    );
}

#[test]
fn non_fixed_instrument_renders_differently_across_keys_for_contrast() {
    let render = |key: u8| {
        let tone = ToneData::new(varying_wave(), Adsr::flat());
        let voices = vec![Instrument::DirectSound(tone)];
        let track = vec![
            Event::Voice(0),
            Event::Note {
                key,
                velocity: 127,
                gate: 90,
            },
            Event::Wait(96),
            Event::Fine,
        ];
        let song = Song::new(voices, vec![track], 150);
        let mut seq = Sequencer::new(song);
        let mut buf = vec![0.0; Sequencer::FRAME_SAMPLES * 3];
        seq.mix_into(&mut buf);
        buf
    };
    assert_ne!(render(40), render(90));
}
