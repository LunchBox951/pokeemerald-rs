//! Shared song and track fixtures for the sequencer test modules.

// The reciprocal wave-frequency the test song derives narrows to `u32` (well
// within range for these inputs); silence/pan checks compare exact `0.0`.
#![allow(clippy::cast_possible_truncation, clippy::float_cmp)]

use std::sync::Arc;

use super::*;
use crate::envelope::Adsr;
use crate::sample::WaveData;
use crate::song::ToneData;

fn unity_freq() -> u32 {
    (1 << pitch::FRAC_BITS) / pitch::DIV_FREQ
}

/// Reference key this test module builds voices around.
const TEST_REFERENCE_KEY: u8 = 60;

/// Shift used to both probe and invert [`pitch::midi_key_to_freq`]'s
/// fixed-point ratio in [`test_song`], trading ratio precision for
/// staying within its public interface.
const FREQUENCY_PROBE_SHIFT: u32 = 20;

/// A voicegroup with one loud, long, flat-envelope instrument at unity
/// pitch for [`TEST_REFERENCE_KEY`].
pub(super) fn test_song(tracks: Vec<Vec<Event>>, tempo: u16) -> Song {
    let target = unity_freq();
    let ratio60 = pitch::midi_key_to_freq(1 << FREQUENCY_PROBE_SHIFT, TEST_REFERENCE_KEY, 0);
    let freq = ((u64::from(target) << FREQUENCY_PROBE_SHIFT) / u64::from(ratio60)) as u32;
    let wave = Arc::new(WaveData::one_shot(freq, vec![100; SAMPLES_PER_FRAME * 4]));
    let voices = vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))];
    Song::new(voices, tracks, tempo)
}

pub(super) fn apply_test_event(seq: &mut Sequencer, track_id: usize, event: &Event) {
    let Sequencer {
        song,
        tracks,
        mixer,
        tempo_i,
        mem_acc,
        ..
    } = seq;
    Sequencer::handle_event(
        song,
        &mut tracks[track_id],
        mixer,
        track_id,
        tempo_i,
        mem_acc,
        event,
    );
}

pub(super) fn tied_note(key: u8) -> Event {
    Event::Note {
        key,
        velocity: 127,
        gate: 0,
    }
}

pub(super) fn render_frames(seq: &mut Sequencer, frames: usize) {
    let mut output = vec![0.0; Sequencer::FRAME_SAMPLES];
    for _ in 0..frames {
        seq.render_frame(&mut output);
    }
}

pub(super) fn direct_sound(sample: i8) -> Instrument {
    Instrument::DirectSound(ToneData::new(
        Arc::new(WaveData::one_shot(
            1 << 20,
            vec![sample; SAMPLES_PER_FRAME * 4],
        )),
        Adsr::flat(),
    ))
}
