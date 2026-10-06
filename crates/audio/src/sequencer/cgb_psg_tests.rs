//! PSG instruments sound through the sequencer with correct channel and fixed-rate routing.

use super::*;
use crate::cgb_envelope::CgbAdsr;
use crate::song::{NoiseTone, SquareTone, WaveTone};

// --- CGB PSG instruments, wired end-to-end through the sequencer -------

fn cgb_test_track() -> Vec<Event> {
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
}

#[test]
fn a_cgb_square_note_produces_sound_through_the_sequencer() {
    let voices = vec![Instrument::CgbSquare1(SquareTone {
        duty: 2,
        sweep: 0,
        adsr: CgbAdsr::flat(),
        fixed_rate: false,
    })];
    let song = Song::new(voices, vec![cgb_test_track()], 150);
    let mut seq = Sequencer::new(song);
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
    seq.render_frame(&mut out);
    assert!(out.iter().any(|&s| s.abs() > 0.0));
    assert_eq!(seq.voice_count(), 1);
}

#[test]
fn a_fixed_rate_cgb_square_note_still_produces_sound_through_the_sequencer() {
    let voices = vec![Instrument::CgbSquare1(SquareTone {
        duty: 2,
        sweep: 0,
        adsr: CgbAdsr::flat(),
        fixed_rate: true,
    })];
    let song = Song::new(voices, vec![cgb_test_track()], 150);
    let mut seq = Sequencer::new(song);
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
    seq.render_frame(&mut out);
    assert!(out.iter().any(|&s| s.abs() > 0.0));
    assert_eq!(seq.voice_count(), 1);
}

/// Render `frames` frames of a one-instrument song around `instrument`,
/// concatenated -- the comparison buffer for the fixed-rate threading
/// tests below.
fn rendered_cgb_frames(instrument: Instrument, frames: usize) -> Vec<f32> {
    let song = Song::new(vec![instrument], vec![cgb_test_track()], 150);
    let mut seq = Sequencer::new(song);
    let mut all = Vec::with_capacity(frames * Sequencer::FRAME_SAMPLES);
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
    for _ in 0..frames {
        seq.render_frame(&mut out);
        all.extend_from_slice(&out);
    }
    all
}

/// `SquareTone::fixed_rate` must actually reach [`CgbVoice`]'s DAC
/// correction through the sequencer, not merely avoid a crash:
/// [`cgb_test_track`]'s key lands on a register `cgb_dac_correct`
/// rounds to a different playback rate, so the two renders must
/// diverge.
#[expect(
    clippy::float_cmp,
    reason = "the test asserts the rendered frames differ exactly, bit for bit"
)]
#[test]
fn a_fixed_rate_cgb_square_audibly_differs_from_a_plain_one() {
    let tone = |fixed_rate| {
        Instrument::CgbSquare1(SquareTone {
            duty: 2,
            sweep: 0,
            adsr: CgbAdsr::flat(),
            fixed_rate,
        })
    };
    let fixed = rendered_cgb_frames(tone(true), 8);
    let plain = rendered_cgb_frames(tone(false), 8);
    assert!(
        fixed.iter().zip(&plain).any(|(a, b)| a != b),
        "the DAC-corrected register must change the rendered square waveform"
    );
}

/// [`WaveTone::fixed_rate`]'s counterpart to
/// [`a_fixed_rate_cgb_square_audibly_differs_from_a_plain_one`] -- the
/// wave channel threads the same flag through `CgbVoice::wave`.
#[expect(
    clippy::float_cmp,
    reason = "the test asserts the rendered frames differ exactly, bit for bit"
)]
#[test]
fn a_fixed_rate_cgb_wave_audibly_differs_from_a_plain_one() {
    // Decodes to alternating 0/15 samples: a full-swing waveform, so a
    // playback-rate difference is visible (a constant table would
    // render identically at any rate).
    const FULL_SWING_WAVE_RAM: [u8; 16] = [0x0F; 16];

    let tone = |fixed_rate| {
        Instrument::CgbWave(WaveTone {
            table: FULL_SWING_WAVE_RAM,
            adsr: CgbAdsr::flat(),
            fixed_rate,
        })
    };
    let fixed = rendered_cgb_frames(tone(true), 8);
    let plain = rendered_cgb_frames(tone(false), 8);
    assert!(
        fixed.iter().zip(&plain).any(|(a, b)| a != b),
        "the DAC-corrected register must change the rendered wave waveform"
    );
}

#[test]
fn a_cgb_wave_note_produces_sound_through_the_sequencer() {
    let voices = vec![Instrument::CgbWave(WaveTone {
        table: [0xFF; 16],
        adsr: CgbAdsr::flat(),
        fixed_rate: false,
    })];
    let song = Song::new(voices, vec![cgb_test_track()], 150);
    let mut seq = Sequencer::new(song);
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
    seq.render_frame(&mut out);
    assert!(out.iter().any(|&s| s.abs() > 0.0));
    assert_eq!(seq.voice_count(), 1);
}

#[test]
fn a_cgb_noise_note_produces_sound_through_the_sequencer() {
    let voices = vec![Instrument::CgbNoise(NoiseTone {
        lfsr_width_selector: 0,
        adsr: CgbAdsr::flat(),
    })];
    let song = Song::new(voices, vec![cgb_test_track()], 150);
    let mut seq = Sequencer::new(song);
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
    seq.render_frame(&mut out);
    assert!(out.iter().any(|&s| s.abs() > 0.0));
    assert_eq!(seq.voice_count(), 1);
}

#[test]
fn cgb_square1_and_square2_occupy_independent_channel_slots() {
    // Two different tracks each selecting a square instrument must both
    // sound at once — they are different hardware channel numbers, not
    // a shared pool.
    let voices = vec![
        Instrument::CgbSquare1(SquareTone {
            duty: 2,
            sweep: 0,
            adsr: CgbAdsr::flat(),
            fixed_rate: false,
        }),
        Instrument::CgbSquare2(SquareTone {
            duty: 1,
            sweep: 0,
            adsr: CgbAdsr::flat(),
            fixed_rate: false,
        }),
    ];
    let track_a = vec![
        Event::Voice(0),
        Event::Note {
            key: 60,
            velocity: 127,
            gate: 8,
        },
        Event::Wait(48),
        Event::Fine,
    ];
    let track_b = vec![
        Event::Voice(1),
        Event::Note {
            key: 64,
            velocity: 127,
            gate: 8,
        },
        Event::Wait(48),
        Event::Fine,
    ];
    let song = Song::new(voices, vec![track_a, track_b], 150);
    let mut seq = Sequencer::new(song);
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
    seq.render_frame(&mut out);
    assert_eq!(seq.voice_count(), 2);
}

#[test]
fn a_new_note_on_the_same_cgb_channel_replaces_the_old_one() {
    // Two notes on the same track (same instrument -> same hardware
    // channel) in immediate succession: the second retriggers the
    // channel rather than accumulating a second voice.
    let voices = vec![Instrument::CgbNoise(NoiseTone {
        lfsr_width_selector: 0,
        adsr: CgbAdsr::flat(),
    })];
    let track = vec![
        Event::Voice(0),
        Event::Note {
            key: 60,
            velocity: 127,
            gate: 0,
        },
        Event::Note {
            key: 64,
            velocity: 127,
            gate: 0,
        },
        Event::Wait(4),
        Event::Fine,
    ];
    let song = Song::new(voices, vec![track], 150);
    let mut seq = Sequencer::new(song);
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
    seq.render_frame(&mut out);
    assert_eq!(seq.voice_count(), 1);
}
