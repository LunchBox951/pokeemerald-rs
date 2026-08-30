//! DirectSound voice playback and stereo accumulation.
//!
//! `SoundMainRAM` mixes four packed sample lanes and permits carry between
//! adjacent lanes (`m4a_1.s:396..437`). This native mixer accumulates each
//! output sample independently in an `i32`, so it does not reproduce that
//! cross-lane bleed. Fixed-rate voices advance by exactly one source sample,
//! which also preserves their raw, un-interpolated playback.

use std::sync::Arc;

use crate::envelope::{Adsr, Envelope};
use crate::pitch::{self, FRAC_MASK};
use crate::sample::WaveData;

const MASTER_VOLUME_BITS: u32 = 4;
const SAMPLE_GAIN_BITS: u32 = 8;
const CHANNEL_VOLUME_BITS: u32 = 14;
const CENTRED_RIGHT_PAN: i32 = 128;
const CENTRED_LEFT_PAN: i32 = 127;
const MIDI_KEY_COUNT: i32 = 256;

/// Signed pre-clip accumulators for one `(left, right)` output sample.
pub type StereoAcc = (i32, i32);

#[derive(Clone, Copy, Debug)]
struct SourcePosition {
    sample_index: usize,
    fractional_phase: u32,
}

impl SourcePosition {
    fn start() -> Self {
        Self {
            sample_index: 0,
            fractional_phase: 0,
        }
    }

    fn normalize(&mut self, wave: &WaveData) -> bool {
        if self.sample_index < wave.len() {
            return true;
        }
        if !wave.is_looping() {
            return false;
        }

        let loop_start = wave.loop_start();
        let loop_len = wave.len() - loop_start;
        self.sample_index = loop_start + (self.sample_index - loop_start) % loop_len;
        true
    }

    fn interpolated_sample(&self, wave: &WaveData) -> i32 {
        let samples = wave.samples();
        let current = i32::from(samples[self.sample_index]);
        let next_index = self.sample_index + 1;
        let next = if next_index < samples.len() {
            samples[next_index]
        } else if wave.is_looping() {
            samples[wave.loop_start()]
        } else {
            samples[self.sample_index]
        };
        // `SoundMainRAM` multiplies and arithmetically shifts the signed delta
        // before adding the current sample (`m4a_1.s:396..407`).
        let weighted_delta = (i64::from(self.fractional_phase)
            * i64::from(i32::from(next) - current))
            >> pitch::FRAC_BITS;
        current + i32::try_from(weighted_delta).unwrap_or(0)
    }

    fn advance_wrapping(&mut self, phase_step: u32) {
        // `SoundMainRAM` advances its 32-bit phase with wrapping addition
        // (`m4a_1.s:413..418`).
        self.fractional_phase = self.fractional_phase.wrapping_add(phase_step);
        self.sample_index +=
            usize::try_from(self.fractional_phase >> pitch::FRAC_BITS).unwrap_or(0);
        self.fractional_phase &= FRAC_MASK;
    }
}

#[derive(Clone, Copy, Debug)]
enum Gate {
    Tied,
    TicksRemaining(u16),
    Expired,
}

impl Gate {
    fn new(gate_time: u16) -> Self {
        if gate_time == 0 {
            Self::Tied
        } else {
            Self::TicksRemaining(gate_time)
        }
    }

    fn tick(&mut self) -> bool {
        match *self {
            Self::TicksRemaining(1) => {
                *self = Self::Expired;
                true
            }
            Self::TicksRemaining(remaining) => {
                *self = Self::TicksRemaining(remaining - 1);
                false
            }
            Self::Tied | Self::Expired => false,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum PlaybackRate {
    PitchScaled,
    Fixed,
}

impl PlaybackRate {
    fn phase_step(self, frequency: u32) -> u32 {
        match self {
            Self::PitchScaled => pitch::phase_step(frequency),
            Self::Fixed => pitch::FRAC_ONE,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct ChannelVolume {
    right: u8,
    left: u8,
    velocity: u8,
    rhythm_pan: i8,
}

impl ChannelVolume {
    fn new(right: u8, left: u8, velocity: u8) -> Self {
        Self {
            right,
            left,
            velocity,
            rhythm_pan: 0,
        }
    }

    fn update_from_track(&mut self, track_right: u8, track_left: u8) {
        let (pan_right, pan_left) = pan_terms(self.rhythm_pan);
        self.right = channel_volume(track_right, pan_right, self.velocity);
        self.left = channel_volume(track_left, pan_left, self.velocity);
    }

    fn frame_gain(self, master_volume: u8, envelope_volume: u8) -> StereoGain {
        let effective_volume =
            ((u32::from(master_volume) + 1) * u32::from(envelope_volume)) >> MASTER_VOLUME_BITS;
        StereoGain {
            right: i32::try_from((u32::from(self.right) * effective_volume) >> SAMPLE_GAIN_BITS)
                .unwrap_or(i32::MAX),
            left: i32::try_from((u32::from(self.left) * effective_volume) >> SAMPLE_GAIN_BITS)
                .unwrap_or(i32::MAX),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct StereoGain {
    right: i32,
    left: i32,
}

impl StereoGain {
    fn silent() -> Self {
        Self { right: 0, left: 0 }
    }

    fn accumulate(self, sample: i32, output: &mut StereoAcc) {
        output.0 += (self.left * sample) >> SAMPLE_GAIN_BITS;
        output.1 += (self.right * sample) >> SAMPLE_GAIN_BITS;
    }
}

#[derive(Clone, Copy, Debug)]
struct VoiceIdentity {
    track: usize,
    played_key: u8,
    pitch_key: u8,
    note_on_ordinal: u64,
    priority: u8,
}

impl VoiceIdentity {
    fn new(track: usize, played_key: u8) -> Self {
        Self {
            track,
            played_key,
            pitch_key: played_key,
            note_on_ordinal: 0,
            priority: 0,
        }
    }
}

/// A live voice owned by the mixer.
#[derive(Clone, Debug)]
pub struct Voice {
    wave: Arc<WaveData>,
    envelope: Envelope,
    channel_volume: ChannelVolume,
    frame_gain: StereoGain,
    frequency: u32,
    source_position: SourcePosition,
    gate: Gate,
    playback_rate: PlaybackRate,
    identity: VoiceIdentity,
}

impl Voice {
    /// Start a voice. A zero `gate_time` creates a tied note.
    #[must_use]
    #[expect(
        clippy::too_many_arguments,
        reason = "a voice starts from one decoded note and its instrument state"
    )]
    pub fn new(
        wave: Arc<WaveData>,
        adsr: Adsr,
        frequency: u32,
        right: u8,
        left: u8,
        velocity: u8,
        gate_time: u16,
        midi_key: u8,
        track: usize,
        echo_volume: u8,
        echo_length: u8,
    ) -> Self {
        Self {
            wave,
            envelope: Envelope::new(adsr, echo_volume, echo_length),
            channel_volume: ChannelVolume::new(right, left, velocity),
            frame_gain: StereoGain::silent(),
            frequency,
            source_position: SourcePosition::start(),
            gate: Gate::new(gate_time),
            playback_rate: PlaybackRate::PitchScaled,
            identity: VoiceIdentity::new(track, midi_key),
        }
    }

    #[must_use]
    pub(crate) fn with_priority(mut self, priority: u8) -> Self {
        self.identity.priority = priority;
        self
    }

    #[must_use]
    pub(crate) fn priority(&self) -> u8 {
        self.identity.priority
    }

    #[must_use]
    pub(crate) fn with_pitch_key(mut self, pitch_key: u8) -> Self {
        // `ply_note` pitches a rhythm voice from its child key, while
        // `ply_endtie` matches the played track key (`m4a_1.s:1594,1819`).
        self.identity.pitch_key = pitch_key;
        self
    }

    #[must_use]
    pub(crate) fn with_rhythm_pan(mut self, rhythm_pan: i8) -> Self {
        self.channel_volume.rhythm_pan = rhythm_pan;
        self
    }

    #[must_use]
    pub(crate) fn fixed_rate(mut self, fixed_rate: bool) -> Self {
        self.playback_rate = if fixed_rate {
            PlaybackRate::Fixed
        } else {
            PlaybackRate::PitchScaled
        };
        self
    }

    pub(crate) fn set_seq(&mut self, seq: u64) {
        self.identity.note_on_ordinal = seq;
    }

    #[must_use]
    pub(crate) fn seq(&self) -> u64 {
        self.identity.note_on_ordinal
    }

    /// Return whether the voice can still produce sound.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.envelope.is_active()
    }

    /// Return the owning track index.
    #[must_use]
    pub fn track(&self) -> usize {
        self.identity.track
    }

    /// Return the played MIDI key used for tie matching.
    #[must_use]
    pub fn midi_key(&self) -> u8 {
        self.identity.played_key
    }

    /// Return the current source-sample index.
    #[must_use]
    pub fn source_index(&self) -> usize {
        self.source_position.sample_index
    }

    /// Return whether note-off has been requested.
    #[must_use]
    pub fn is_stopping(&self) -> bool {
        self.envelope.is_stopping()
    }

    /// Advance the note-off gate by one sequencer tick.
    pub fn tick_gate(&mut self) {
        if self.gate.tick() {
            self.envelope.note_off();
        }
    }

    /// Request note-off immediately.
    pub fn note_off(&mut self) {
        self.envelope.note_off();
    }

    /// Update base channel volumes from the owning track.
    pub fn set_track_volume(&mut self, vol_mr: u8, vol_ml: u8) {
        self.channel_volume.update_from_track(vol_mr, vol_ml);
    }

    /// Update the stored frequency from track pitch. Fixed-rate playback keeps
    /// advancing one source sample regardless of this value.
    pub fn set_track_pitch(&mut self, key_m: i32, pit_m: u8) {
        let translated_key = (i32::from(self.identity.pitch_key) + key_m).max(0) % MIDI_KEY_COUNT;
        let note_key = u8::try_from(translated_key).unwrap_or(0);
        self.frequency = pitch::midi_key_to_freq(self.wave.freq(), note_key, pit_m);
    }

    /// Return the stored playback frequency.
    #[must_use]
    pub fn frequency(&self) -> u32 {
        self.frequency
    }

    /// Return the base `(right, left)` channel volumes.
    #[must_use]
    pub fn base_volume(&self) -> (u8, u8) {
        (self.channel_volume.right, self.channel_volume.left)
    }

    /// Advance the envelope and prepare stereo gain for one render frame.
    pub fn begin_frame(&mut self, master_volume: u8) {
        self.envelope.step();
        self.frame_gain = self
            .channel_volume
            .frame_gain(master_volume, self.envelope.volume());
    }

    /// Accumulate this voice into one frame after [`Self::begin_frame`].
    pub fn render(&mut self, acc: &mut [StereoAcc]) {
        let phase_step = self.playback_rate.phase_step(self.frequency);
        if self.wave.is_empty() {
            self.envelope.note_off();
            return;
        }

        for output in acc.iter_mut() {
            if !self.envelope.is_active() {
                break;
            }
            if !self.source_position.normalize(&self.wave) {
                self.envelope.retire();
                break;
            }

            let sample = self.source_position.interpolated_sample(&self.wave);
            self.frame_gain.accumulate(sample, output);
            self.source_position.advance_wrapping(phase_step);
        }
    }
}

#[must_use]
pub(crate) fn channel_volume(vol_side: u8, pan_term: u32, velocity: u8) -> u8 {
    let scaled = (u32::from(vol_side) * (pan_term * u32::from(velocity))) >> CHANNEL_VOLUME_BITS;
    u8::try_from(scaled.min(u32::from(u8::MAX))).unwrap_or(u8::MAX)
}

#[must_use]
pub(crate) fn pan_terms(rhythm_pan: i8) -> (u32, u32) {
    let pan = i32::from(rhythm_pan);
    let right = u32::try_from(CENTRED_RIGHT_PAN + pan).unwrap_or(0);
    let left = u32::try_from(CENTRED_LEFT_PAN - pan).unwrap_or(0);
    (right, left)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL_SCALE_FRAME_GAIN: i32 = 254;
    const TEST_VELOCITY: u8 = 127;
    const TEST_KEY: u8 = 60;

    fn wave(freq: u32, data: Vec<i8>) -> Arc<WaveData> {
        Arc::new(WaveData::one_shot(freq, data))
    }

    fn approximately_unity_frequency() -> u32 {
        (1 << pitch::FRAC_BITS) / pitch::DIV_FREQ
    }

    fn voice(wave: Arc<WaveData>, frequency: u32, right: u8, left: u8, gate_time: u16) -> Voice {
        Voice::new(
            wave,
            Adsr::flat(),
            frequency,
            right,
            left,
            TEST_VELOCITY,
            gate_time,
            TEST_KEY,
            0,
            0,
            0,
        )
    }

    #[test]
    fn flat_envelope_applies_gain_exactly_on_a_constant_wave() {
        let constant_sample = 50;
        let mut voice = voice(
            wave(0, vec![constant_sample; 16]),
            approximately_unity_frequency(),
            u8::MAX,
            u8::MAX,
            0,
        );
        let mut acc = vec![(0, 0); 8];
        voice.begin_frame(15);
        voice.render(&mut acc);
        let expected = (FULL_SCALE_FRAME_GAIN * i32::from(constant_sample)) >> SAMPLE_GAIN_BITS;
        for output in &acc {
            assert_eq!(*output, (expected, expected));
        }
    }

    #[test]
    fn zero_fraction_first_output_is_the_raw_first_sample() {
        let first_sample = -100;
        let mut voice = voice(
            wave(0, vec![first_sample, 0, 0, 0]),
            approximately_unity_frequency(),
            u8::MAX,
            u8::MAX,
            0,
        );
        let mut acc = vec![(0, 0); 1];
        voice.begin_frame(15);
        voice.render(&mut acc);
        assert_eq!(
            acc[0].0,
            (FULL_SCALE_FRAME_GAIN * i32::from(first_sample)) >> SAMPLE_GAIN_BITS
        );
    }

    #[test]
    fn faster_frequency_advances_further_through_the_wave() {
        let wave = wave(0, vec![0; 512]);
        let unity_frequency = approximately_unity_frequency();
        let mut slow = voice(wave.clone(), unity_frequency, u8::MAX, u8::MAX, 0);
        let mut fast = voice(wave, unity_frequency * 2, u8::MAX, u8::MAX, 0);
        let mut acc = vec![(0, 0); 100];
        slow.begin_frame(15);
        slow.render(&mut acc);
        fast.begin_frame(15);
        fast.render(&mut acc);
        assert!((95..=105).contains(&slow.source_index()));
        assert!((195..=205).contains(&fast.source_index()));
    }

    #[test]
    fn half_frequency_interpolates_between_samples() {
        let mut voice = voice(
            wave(0, vec![0, 100, 0, 0]),
            approximately_unity_frequency() / 2,
            u8::MAX,
            u8::MAX,
            0,
        );
        let mut acc = vec![(0, 0); 2];
        voice.begin_frame(15);
        voice.render(&mut acc);
        assert_eq!(acc[0].0, 0);
        let interpolated_sample = acc[1].0 * 256 / FULL_SCALE_FRAME_GAIN;
        assert!(
            (48..=52).contains(&interpolated_sample),
            "midpoint interpolation was {interpolated_sample}"
        );
    }

    #[test]
    fn one_shot_voice_retires_at_end_of_wave() {
        let first_sample = 10;
        let mut voice = voice(
            wave(0, vec![first_sample, 20]),
            approximately_unity_frequency(),
            u8::MAX,
            u8::MAX,
            0,
        );
        let mut acc = vec![(0, 0); 8];
        voice.begin_frame(15);
        voice.render(&mut acc);
        assert!(!voice.is_active());
        assert_eq!(
            acc[0].0,
            (FULL_SCALE_FRAME_GAIN * i32::from(first_sample)) >> SAMPLE_GAIN_BITS
        );
        assert_eq!(acc[7], (0, 0));
    }

    #[test]
    fn looping_voice_wraps_and_keeps_playing() {
        let first_sample = 99;
        let loop_sample = 5;
        let wave = Arc::new(WaveData::looping(0, 1, vec![first_sample, loop_sample]));
        let mut voice = voice(wave, approximately_unity_frequency(), u8::MAX, u8::MAX, 0);
        let mut acc = vec![(0, 0); 32];
        voice.begin_frame(15);
        voice.render(&mut acc);
        assert_eq!(
            acc[0].0,
            (FULL_SCALE_FRAME_GAIN * i32::from(first_sample)) >> SAMPLE_GAIN_BITS
        );
        assert_eq!(
            acc[31].0,
            (FULL_SCALE_FRAME_GAIN * i32::from(loop_sample)) >> SAMPLE_GAIN_BITS
        );
        assert!(voice.is_active());
    }

    #[test]
    fn panned_voice_splits_left_and_right() {
        let first_sample = 100;
        let mut voice = voice(
            wave(0, vec![first_sample, 0, 0, 0]),
            approximately_unity_frequency(),
            u8::MAX,
            0,
            0,
        );
        let mut acc = vec![(0, 0); 1];
        voice.begin_frame(15);
        voice.render(&mut acc);
        assert_eq!(acc[0].0, 0);
        assert_eq!(
            acc[0].1,
            (FULL_SCALE_FRAME_GAIN * i32::from(first_sample)) >> SAMPLE_GAIN_BITS
        );
    }

    #[test]
    fn near_max_phase_step_wraps_without_panicking() {
        let wave = Arc::new(WaveData::looping(0, 0, vec![10, -10, 10, -10]));
        let mut voice = voice(wave, u32::MAX, u8::MAX, u8::MAX, 0);
        let mut acc = vec![(0, 0); 8];
        voice.begin_frame(15);
        voice.render(&mut acc);
        assert!(voice.is_active());
    }

    #[test]
    fn set_track_volume_rewrites_base_from_velocity() {
        let track_volume = 0x40;
        let mut voice = voice(
            wave(0, vec![50; 16]),
            approximately_unity_frequency(),
            0,
            0,
            0,
        );
        voice.set_track_volume(track_volume, track_volume);
        let (right, left) = voice.base_volume();
        assert_eq!(
            right,
            channel_volume(track_volume, CENTRED_RIGHT_PAN as u32, TEST_VELOCITY)
        );
        assert_eq!(
            left,
            channel_volume(track_volume, CENTRED_LEFT_PAN as u32, TEST_VELOCITY)
        );
        assert!(right > 0 && left > 0);
    }

    #[test]
    fn set_track_pitch_recomputes_frequency_from_stored_key() {
        let wave_frequency = 1 << 20;
        let mut voice = voice(
            wave(wave_frequency, vec![50; 16]),
            approximately_unity_frequency(),
            u8::MAX,
            u8::MAX,
            0,
        );
        voice.set_track_pitch(0, 0);
        assert_eq!(
            voice.frequency(),
            pitch::midi_key_to_freq(wave_frequency, TEST_KEY, 0)
        );
        let one_octave = 12_u8;
        voice.set_track_pitch(i32::from(one_octave), 0);
        assert_eq!(
            voice.frequency(),
            pitch::midi_key_to_freq(wave_frequency, TEST_KEY + one_octave, 0)
        );
    }

    #[test]
    fn with_pitch_key_overrides_the_base_used_for_mid_note_bend() {
        let wave_frequency = 1 << 20;
        let rhythm_child_key = 72;
        let mut voice = voice(
            wave(wave_frequency, vec![50; 16]),
            approximately_unity_frequency(),
            u8::MAX,
            u8::MAX,
            0,
        )
        .with_pitch_key(rhythm_child_key);
        assert_eq!(
            voice.midi_key(),
            TEST_KEY,
            "tie identity stays the played key"
        );
        voice.set_track_pitch(0, 0);
        assert_eq!(
            voice.frequency(),
            pitch::midi_key_to_freq(wave_frequency, rhythm_child_key, 0)
        );
    }

    #[test]
    fn with_rhythm_pan_shifts_the_stereo_split_on_volume_reruns() {
        let rhythm_pan = 63;
        let track_volume = 0x40;
        let mut voice = voice(
            wave(0, vec![50; 16]),
            approximately_unity_frequency(),
            0,
            0,
            0,
        )
        .with_rhythm_pan(rhythm_pan);
        voice.set_track_volume(track_volume, track_volume);
        let (right, left) = voice.base_volume();
        let (pan_right, pan_left) = pan_terms(rhythm_pan);
        assert_eq!(
            right,
            channel_volume(track_volume, pan_right, TEST_VELOCITY)
        );
        assert_eq!(left, channel_volume(track_volume, pan_left, TEST_VELOCITY));
        assert!(
            right > left,
            "a positive rhythm pan should favour the right channel"
        );
    }

    #[test]
    fn pan_terms_reduces_to_the_plain_split_when_rhythm_pan_is_zero() {
        assert_eq!(
            pan_terms(0),
            (CENTRED_RIGHT_PAN as u32, CENTRED_LEFT_PAN as u32)
        );
    }

    #[test]
    fn fixed_rate_voice_consumes_one_source_sample_per_output_sample() {
        let wave = wave(0, vec![0; 512]);
        let unity_frequency = approximately_unity_frequency();
        let mut slow = voice(wave.clone(), unity_frequency, u8::MAX, u8::MAX, 0).fixed_rate(true);
        let mut fast = voice(wave, unity_frequency * 5, u8::MAX, u8::MAX, 0).fixed_rate(true);
        let mut acc = vec![(0, 0); 100];
        slow.begin_frame(15);
        slow.render(&mut acc);
        fast.begin_frame(15);
        fast.render(&mut acc);
        assert_eq!(slow.source_index(), 100);
        assert_eq!(fast.source_index(), 100);
    }

    #[test]
    fn fixed_rate_voice_ignores_a_mid_note_bend() {
        let mut voice = voice(
            wave(0, vec![0; 512]),
            approximately_unity_frequency(),
            u8::MAX,
            u8::MAX,
            0,
        )
        .fixed_rate(true);
        let mut acc = vec![(0, 0); 50];
        voice.begin_frame(15);
        voice.render(&mut acc);
        let unbent_index = voice.source_index();

        let large_upward_bend = 48;
        voice.set_track_pitch(large_upward_bend, 0);
        let mut acc2 = vec![(0, 0); 50];
        voice.begin_frame(15);
        voice.render(&mut acc2);
        assert_eq!(
            voice.source_index(),
            unbent_index * 2,
            "playback rate must stay exactly one sample per output sample after the bend"
        );
    }

    #[test]
    fn fixed_rate_voice_reads_raw_samples_with_no_interpolation() {
        let second_sample = 100;
        let mut voice = voice(
            wave(0, vec![0, second_sample, 0, 0]),
            approximately_unity_frequency() / 2,
            u8::MAX,
            u8::MAX,
            0,
        )
        .fixed_rate(true);
        let mut acc = vec![(0, 0); 2];
        voice.begin_frame(15);
        voice.render(&mut acc);
        assert_eq!(acc[0].0, 0);
        assert_eq!(
            acc[1].0,
            (FULL_SCALE_FRAME_GAIN * i32::from(second_sample)) >> SAMPLE_GAIN_BITS,
            "sample 1 exactly, no blend toward sample 2"
        );
    }

    #[test]
    fn gate_expiry_releases_the_envelope() {
        let mut voice = voice(
            wave(0, vec![50, 50, 50, 50]),
            approximately_unity_frequency(),
            u8::MAX,
            u8::MAX,
            2,
        );
        assert!(!voice.is_stopping());
        voice.tick_gate();
        assert!(!voice.is_stopping());
        voice.tick_gate();
        assert!(voice.is_stopping());
    }
}
