use super::{CgbAdsr, CgbChannelNumber, CgbVoice};
use crate::psg::WaveChannel;

pub(super) const TEST_KEY: u8 = 60;
pub(super) const FULL_TRACK_VOLUME: u8 = u8::MAX;
pub(super) const FULL_VELOCITY: u8 = 127;
pub(super) const MAX_ADSR_LEVEL: u8 = 15;
pub(super) const HALF_DUTY: u8 = 2;
pub(super) const NARROW_NOISE: u8 = 1;
pub(super) const WIDE_NOISE: u8 = 0;
pub(super) const SWEEP_PERIOD_SHIFT: u32 = 4;
pub(super) const FULL_SWING_WAVE_BYTE: u8 = 0x0F;

#[derive(Clone, Copy)]
pub(super) struct TestNote {
    pub(super) note_key: u8,
    pub(super) fine_pitch: u8,
    pub(super) track_right: u8,
    pub(super) track_left: u8,
    pub(super) velocity: u8,
    pub(super) gate_time: u16,
    pub(super) played_key: u8,
    pub(super) track: usize,
    pub(super) rhythm_pan: i8,
    pub(super) echo_volume: u8,
    pub(super) echo_length: u8,
}

impl TestNote {
    pub(super) fn at_key(key: u8) -> Self {
        Self {
            note_key: key,
            played_key: key,
            ..Self::default()
        }
    }
}

impl Default for TestNote {
    fn default() -> Self {
        Self {
            note_key: TEST_KEY,
            fine_pitch: 0,
            track_right: FULL_TRACK_VOLUME,
            track_left: FULL_TRACK_VOLUME,
            velocity: FULL_VELOCITY,
            gate_time: 0,
            played_key: TEST_KEY,
            track: 0,
            rhythm_pan: 0,
            echo_volume: 0,
            echo_length: 0,
        }
    }
}

pub(super) fn square_voice_with_adsr(
    channel: CgbChannelNumber,
    sweep: Option<u8>,
    adsr: CgbAdsr,
    note: TestNote,
) -> CgbVoice {
    CgbVoice::square(
        channel,
        HALF_DUTY,
        sweep,
        adsr,
        note.note_key,
        note.fine_pitch,
        note.track_right,
        note.track_left,
        note.velocity,
        note.gate_time,
        note.played_key,
        note.track,
        note.rhythm_pan,
        note.echo_volume,
        note.echo_length,
    )
}

pub(super) fn square_voice(
    channel: CgbChannelNumber,
    sweep: Option<u8>,
    note: TestNote,
) -> CgbVoice {
    square_voice_with_adsr(channel, sweep, CgbAdsr::flat(), note)
}

pub(super) fn fixed_square_voice(
    channel: CgbChannelNumber,
    sweep: Option<u8>,
    note: TestNote,
) -> CgbVoice {
    CgbVoice::square_with_fixed_rate(
        channel,
        HALF_DUTY,
        sweep,
        CgbAdsr::flat(),
        true,
        note.note_key,
        note.fine_pitch,
        note.track_right,
        note.track_left,
        note.velocity,
        note.gate_time,
        note.played_key,
        note.track,
        note.rhythm_pan,
        note.echo_volume,
        note.echo_length,
    )
}

pub(super) fn wave_voice(fixed_rate: bool, note: TestNote) -> CgbVoice {
    CgbVoice::wave(
        full_swing_wave(),
        CgbAdsr::flat(),
        fixed_rate,
        note.note_key,
        note.fine_pitch,
        note.track_right,
        note.track_left,
        note.velocity,
        note.gate_time,
        note.played_key,
        note.track,
        note.rhythm_pan,
        note.echo_volume,
        note.echo_length,
    )
}

pub(super) fn noise_voice(adsr: CgbAdsr, width_selector: u8, note: TestNote) -> CgbVoice {
    CgbVoice::noise(
        adsr,
        note.note_key,
        width_selector,
        note.track_right,
        note.track_left,
        note.velocity,
        note.gate_time,
        note.played_key,
        note.track,
        note.rhythm_pan,
        note.echo_volume,
        note.echo_length,
    )
}

pub(super) fn upward_sweep(period_ticks: u8, shift: u8) -> u8 {
    (period_ticks << SWEEP_PERIOD_SHIFT) | shift
}

pub(super) fn full_swing_wave() -> [i8; 32] {
    WaveChannel::decode_wave_ram(&[FULL_SWING_WAVE_BYTE; 16])
}
