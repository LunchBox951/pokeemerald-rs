//! Shared deterministic PCM/CGB voice constructors and mixer test constants.

use std::sync::Arc;

use super::*;
use crate::cgb_envelope::CgbAdsr;
use crate::cgb_voice::CgbChannelNumber;
use crate::envelope::Adsr;
use crate::pitch::{DIV_FREQ, FRAC_BITS};
use crate::sample::WaveData;

pub(super) const MAX_MASTER_VOLUME: u8 = 15;
pub(super) const FULL_TRACK_VOLUME: u8 = u8::MAX;
pub(super) const MUTED_TRACK_VOLUME: u8 = 0;
pub(super) const TEST_VELOCITY: u8 = 127;
pub(super) const TIED_GATE_TIME: u16 = 0;
pub(super) const SAMPLE_GAIN_DIVISOR: i32 = 256;
pub(super) const OUTPUT_SCALE: f32 = 128.0;
pub(super) const ASSERTION_TOLERANCE: f32 = 1e-6;
pub(super) const FLAT_ENVELOPE_GAIN_AT_MAX_MASTER_VOLUME: i32 = 254;

pub(super) fn unity_freq() -> u32 {
    (1 << FRAC_BITS) / DIV_FREQ
}

pub(super) fn cgb_keyed_voice(track: usize, key: u8) -> CgbVoice {
    cgb_swept_voice(track, key, None)
}

pub(super) fn constant_voice(level: i8, track: usize) -> Voice {
    keyed_voice(level, track, 60, FULL_TRACK_VOLUME, FULL_TRACK_VOLUME)
}

pub(super) fn keyed_voice(
    level: i8,
    track: usize,
    key: u8,
    right_volume: u8,
    left_volume: u8,
) -> Voice {
    let frame_long_samples = vec![level; SAMPLES_PER_FRAME + 4];
    let wave = Arc::new(WaveData::one_shot(0, frame_long_samples));
    Voice::new(
        wave,
        Adsr::flat(),
        unity_freq(),
        right_volume,
        left_volume,
        TEST_VELOCITY,
        TIED_GATE_TIME,
        key,
        track,
        0,
        0,
    )
}

pub(super) fn cgb_swept_voice(track: usize, key: u8, sweep: Option<u8>) -> CgbVoice {
    cgb_square_voice(CgbChannelNumber::Square1, track, key, sweep)
}

pub(super) fn cgb_square_voice(
    channel: CgbChannelNumber,
    track: usize,
    key: u8,
    sweep: Option<u8>,
) -> CgbVoice {
    CgbVoice::square(
        channel,
        2,
        sweep,
        CgbAdsr::flat(),
        key,
        0,
        FULL_TRACK_VOLUME,
        FULL_TRACK_VOLUME,
        TEST_VELOCITY,
        TIED_GATE_TIME,
        key,
        track,
        0,
        0,
        0,
    )
}
