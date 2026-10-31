use super::{
    cgb3_wave_gain_256, DacCorrection, BIPOLAR_SAMPLE_SCALE, FULL_GAIN_256, LINEAR_ENVELOPE_SCALE,
    SAMPLE_GAIN_BITS, WAVE_SAMPLE_SCALE,
};
use crate::cgb_pitch::{midi_key_to_cgb_freq_reg, midi_key_to_noise_control};
use crate::psg::{NoiseChannel, SquareChannel, WaveChannel};
use std::sync::Arc;

/// Clocks `noise` once at `hardware_volume` and renders its held latch
/// (`mgba/src/gb/audio.c:782`) centred on the volume it was last resolved against.
/// Only a clock or envelope step moves the level, so a trigger, volume write,
/// or replacement leaves it unchanged.
fn noise_contribution(noise: &mut NoiseChannel, hardware_volume: u8) -> i32 {
    noise.clock_sample(hardware_volume);
    (noise.centred_level() * NOISE_LEVEL_GAIN * BIPOLAR_SAMPLE_SCALE) >> SAMPLE_GAIN_BITS
}

/// [`LINEAR_ENVELOPE_SCALE`] as a signed gain.
const NOISE_LEVEL_GAIN: i32 = LINEAR_ENVELOPE_SCALE.cast_signed();

#[derive(Clone, Debug)]
pub(super) enum Oscillator {
    Square(SquareChannel),
    Wave {
        /// Stable programmable-wave asset identity: wave RAM is reloaded only when it changes.
        wave_id: Arc<str>,
        channel: WaveChannel,
    },
    Noise(NoiseChannel),
}

impl Oscillator {
    /// One sample's pre-routing contribution. Wave ignores `hardware_volume`
    /// and uses `envelope_volume` for NR32; Noise ignores `frame_gain` and
    /// `envelope_volume` ([`noise_contribution`]).
    pub(super) fn frame_contribution(
        &mut self,
        frame_gain: i32,
        hardware_volume: u8,
        envelope_volume: u8,
    ) -> i32 {
        match self {
            Self::Square(square) => {
                (frame_gain * i32::from(square.sample()) * BIPOLAR_SAMPLE_SCALE) >> SAMPLE_GAIN_BITS
            }
            Self::Wave { channel: wave, .. } => {
                let gain = cgb3_wave_gain_256(envelope_volume);
                (frame_gain * wave.sample(gain) * WAVE_SAMPLE_SCALE) >> SAMPLE_GAIN_BITS
            }
            Self::Noise(noise) => noise_contribution(noise, hardware_volume),
        }
    }

    /// A square's contribution re-latched at its current duty position
    /// without stepping it; `None` for Wave and Noise.
    pub(super) fn relatched_square_contribution(&mut self, frame_gain: i32) -> Option<i32> {
        match self {
            Self::Square(square) => Some(
                (frame_gain * i32::from(square.held_level()) * BIPOLAR_SAMPLE_SCALE)
                    >> SAMPLE_GAIN_BITS,
            ),
            Self::Wave { .. } | Self::Noise(_) => None,
        }
    }

    /// The first pass's `NRx1` store, as the byte the length counter loads
    /// from. A square sums its duty into the byte and re-selects the duty from
    /// the result ([`SquareChannel::write_nrx1`]); noise stores the raw length
    /// byte; wave has no such store and returns `None`.
    pub(super) fn write_nrx1(&mut self, length_register: u8) -> Option<u8> {
        match self {
            Self::Square(square) => Some(square.write_nrx1(length_register)),
            Self::Noise(_) => Some(length_register),
            Self::Wave { .. } => None,
        }
    }

    pub(super) fn noise_mut(&mut self) -> Option<&mut NoiseChannel> {
        match self {
            Self::Noise(noise) => Some(noise),
            _ => None,
        }
    }

    /// Wave is always full gain: NR32 attenuation is applied when
    /// [`WaveChannel::sample`] latches.
    pub(super) fn envelope_gain_256(&self, _envelope_volume: u8, hardware_volume: u8) -> u32 {
        match self {
            Self::Wave { .. } => FULL_GAIN_256,
            Self::Square(_) | Self::Noise(_) => u32::from(hardware_volume) * LINEAR_ENVELOPE_SCALE,
        }
    }

    pub(super) fn retune(&mut self, note_key: u8, fine_pitch: u8, correction: DacCorrection) {
        match self {
            Self::Square(square) => square
                .set_frequency(correction.apply(midi_key_to_cgb_freq_reg(note_key, fine_pitch))),
            Self::Wave { channel: wave, .. } => {
                wave.set_frequency(
                    correction.apply(midi_key_to_cgb_freq_reg(note_key, fine_pitch)),
                );
            }
            Self::Noise(noise) => noise.retune(midi_key_to_noise_control(note_key)),
        }
    }

    /// Only a square sweep mutes the hardware, so Wave and Noise are no-ops.
    pub(super) fn advance_silently(&mut self, samples: usize) {
        if let Self::Square(square) = self {
            square.advance_silently(samples);
        }
    }

    pub(super) fn defer_idle_samples(&mut self, samples: usize) {
        if let Self::Square(square) = self {
            square.defer_idle_samples(samples);
        }
    }

    /// The noise off-write restarts phase and LFSR but leaves the latch for
    /// the next clock to settle; a no-op for Wave.
    pub(super) fn apply_hardware_off_write(&mut self) -> bool {
        match self {
            Self::Square(square) => square.apply_hardware_off_write(),
            Self::Noise(noise) => {
                noise.off_write();
                true
            }
            Self::Wave { .. } => true,
        }
    }

    pub(super) fn step_sweep_tick(&mut self) -> bool {
        match self {
            Self::Square(square) => square.step_sweep_tick(),
            Self::Wave { .. } | Self::Noise(_) => true,
        }
    }

    /// Wave keeps its wave-RAM rotation only for the same `wave_id`; a
    /// different asset is a fresh upload (`m4a.c:1003-1012`) that keeps only
    /// the held nibble.
    pub(super) fn carry_hardware_state_from(&mut self, other: &Self) {
        match (self, other) {
            (Self::Square(square), Self::Square(previous)) => square.continue_duty_from(previous),
            (Self::Noise(noise), Self::Noise(previous)) => {
                noise.continue_output_from(previous);
            }
            (
                Self::Wave {
                    wave_id,
                    channel: wave,
                },
                Self::Wave {
                    wave_id: previous_id,
                    channel: previous,
                },
            ) => {
                if wave_id == previous_id {
                    wave.continue_rotation_from(previous);
                } else {
                    wave.continue_latch_from(previous);
                }
            }
            _ => {}
        }
    }

    /// Plays the replacement's note-on `NR43` writes against the noise
    /// predecessor it inherited; a no-op otherwise.
    pub(super) fn commit_initial_writes(&mut self, retires_on_first_pass: bool) {
        if let Self::Noise(noise) = self {
            noise.commit_initial_writes(retires_on_first_pass);
        }
    }

    /// Applies a `CGB_CHANNEL_MO_VOL` trigger (`m4a.c:1219-1226`); returns
    /// whether the channel still plays.
    pub(super) fn retrigger(&mut self) -> bool {
        match self {
            Self::Square(square) => square.retrigger(),
            Self::Wave { .. } => true,
            Self::Noise(noise) => {
                noise.retrigger();
                true
            }
        }
    }
}

#[cfg(test)]
mod nr32_attenuation_tests {
    use super::*;
    use crate::psg::WaveChannel;

    type NibbleLaw = fn(i32) -> i32;

    // Levels selecting mute, quarter, half, three-quarter, full (`m4a_tables.c:168-174`).
    const MODES: [(u8, NibbleLaw); 5] = [
        (0, |_| 0),
        (2, |n| n >> 2),
        (6, |n| n >> 1),
        (10, |n| (n + (n << 1)) >> 2),
        (15, |n| n),
    ];

    fn render(nibble: u8, level: u8) -> i32 {
        let samples = WaveChannel::decode_wave_ram(&[nibble << 4 | nibble; 16]);
        let gain = cgb3_wave_gain_256(level);
        // The cold latch is zero, so the second sample is the first RAM nibble.
        let mut wave = WaveChannel::new(samples, 0x7FF);
        wave.sample(gain);
        wave.sample(gain) * WAVE_SAMPLE_SCALE
    }

    #[test]
    fn quarter_volume_wave_quantizes_adjacent_nibbles_together() {
        assert_eq!(render(4, 2), render(5, 2));
    }

    #[test]
    fn every_nibble_follows_its_nr32_integer_law() {
        for (level, law) in MODES {
            for nibble in 0..16 {
                let expected = (law(i32::from(nibble)) - law(8)) * WAVE_SAMPLE_SCALE;
                assert_eq!(
                    render(nibble, level),
                    expected,
                    "level {level} nibble {nibble}"
                );
            }
        }
    }

    #[test]
    fn three_quarter_volume_collapses_nibbles_hardware_collapses() {
        assert_eq!(render(0, 10), render(1, 10));
        assert_ne!(render(5, 10), render(6, 10));
    }

    #[test]
    fn full_volume_keeps_the_centered_nibble() {
        assert_eq!(render(15, 15), 7 * WAVE_SAMPLE_SCALE);
        assert_eq!(render(0, 31), -8 * WAVE_SAMPLE_SCALE);
    }
}

#[cfg(test)]
mod changed_wave_asset_latch_tests {
    use super::*;
    use crate::psg::WaveChannel;

    /// `mgba/src/gb/audio.c:300-337`.
    #[test]
    fn a_changed_wave_asset_keeps_the_previous_latch_until_the_first_clock() {
        let high = WaveChannel::decode_wave_ram(&[0xFF; 16]);
        let low = WaveChannel::decode_wave_ram(&[0x00; 16]);
        let mut previous = Oscillator::Wave {
            wave_id: Arc::from("old"),
            channel: WaveChannel::new(high, 0x7FF),
        };
        previous.frame_contribution(256, 0, 15);
        assert_eq!(
            previous.frame_contribution(256, 0, 15),
            7 * WAVE_SAMPLE_SCALE
        );
        let mut replacement = Oscillator::Wave {
            wave_id: Arc::from("new"),
            channel: WaveChannel::new(low, 0x7FF),
        };
        replacement.carry_hardware_state_from(&previous);
        let Oscillator::Wave { channel, .. } = &mut replacement else {
            unreachable!()
        };
        assert_eq!(
            channel.sample(FULL_GAIN_256),
            7,
            "old latch before first clock"
        );
        assert_eq!(
            channel.sample(FULL_GAIN_256),
            -8,
            "new waveform after the first clock"
        );
    }

    #[test]
    fn a_replacement_at_a_new_volume_holds_the_old_output_until_the_first_clock() {
        let ram = WaveChannel::decode_wave_ram(&[0xFF; 16]);
        let wave = |id: &str| Oscillator::Wave {
            wave_id: Arc::from(id),
            channel: WaveChannel::new(ram, 0x7FF),
        };
        let mut previous = wave("a");
        previous.frame_contribution(256, 0, 15);
        assert_eq!(
            previous.frame_contribution(256, 0, 15),
            7 * WAVE_SAMPLE_SCALE
        );
        let mut replacement = wave("a");
        replacement.carry_hardware_state_from(&previous);
        assert_eq!(
            replacement.frame_contribution(256, 0, 2),
            7 * WAVE_SAMPLE_SCALE,
            "held output survives the volume change"
        );
        assert_eq!(replacement.frame_contribution(256, 0, 2), WAVE_SAMPLE_SCALE);
    }
}
