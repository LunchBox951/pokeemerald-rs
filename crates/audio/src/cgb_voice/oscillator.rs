use super::{
    cgb3_wave_gain_256, DacCorrection, BIPOLAR_SAMPLE_SCALE, FULL_GAIN_256, LINEAR_ENVELOPE_SCALE,
    SAMPLE_GAIN_BITS, WAVE_SAMPLE_SCALE,
};
use crate::cgb_pitch::{midi_key_to_cgb_freq_reg, midi_key_to_noise_control};
use crate::psg::{NoiseChannel, SquareChannel, WaveChannel};
use std::sync::Arc;

/// Clocks `noise` one sample at `hardware_volume` and maps its held latch
/// into the centred render path. The DAC level is output directly
/// (`mgba/src/gb/audio.c:782`); like the other channels the render path
/// removes the DC offset by centring, here on the volume the latch was last
/// resolved against ([`NoiseChannel::centred_level`]). A clock at volume `v`
/// reproduces the usual `+-v` swing, and nothing but a clock or envelope step
/// moves the rendered level, so a trigger, volume write, or replacement leaves
/// it as it was and a settled latch is silent.
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

/// NR32 shifts the unsigned nibble, so neighbouring nibbles can collapse
/// (`mgba/src/gb/audio.c:519-533,567-574`; `m4a.c:1205-1211`).
fn nr32_attenuated_sample(centered_nibble: i8, envelope_volume: u8) -> i32 {
    const NIBBLE_ZERO: i32 = 8;
    let gain = i32::try_from(cgb3_wave_gain_256(envelope_volume)).unwrap_or(0);
    let attenuate = |nibble: i32| (nibble * gain) >> SAMPLE_GAIN_BITS;
    let nibble = i32::from(centered_nibble) + NIBBLE_ZERO;
    (attenuate(nibble) - attenuate(NIBBLE_ZERO)) * WAVE_SAMPLE_SCALE
}

impl Oscillator {
    /// One sample's pre-routing contribution. Square scales its waveform by
    /// `frame_gain`; Wave attenuates each nibble through NR32 from
    /// `envelope_volume` and its `frame_gain` is full scale; Noise ignores
    /// both and renders its resolved output latch directly
    /// ([`noise_contribution`]).
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
                (frame_gain * nr32_attenuated_sample(wave.sample(), envelope_volume))
                    >> SAMPLE_GAIN_BITS
            }
            Self::Noise(noise) => noise_contribution(noise, hardware_volume),
        }
    }

    /// The retained noise latch, for the idle-slot and completion paths.
    pub(super) fn noise_mut(&mut self) -> Option<&mut NoiseChannel> {
        match self {
            Self::Noise(noise) => Some(noise),
            _ => None,
        }
    }

    /// `hardware_volume` is [`HardwareEnvelopeVolume`](crate::cgb_envelope::HardwareEnvelopeVolume)'s resolved byte; the
    /// Wave arm ignores both: NR32 attenuation is an integer shift of the
    /// unsigned nibble, applied per sample by [`nr32_attenuated_sample`].
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

    /// Keeps a hardware-muted square's duty position running
    /// ([`SquareChannel::advance_silently`]'s doc). Only a square sweep ever
    /// mutes the hardware, so Wave/Noise never reach this.
    pub(super) fn advance_silently(&mut self, samples: usize) {
        if let Self::Square(square) = self {
            square.advance_silently(samples);
        }
    }

    /// Defers an idle square slot's duty catch-up
    /// ([`SquareChannel::defer_idle_samples`]'s doc); a no-op for Wave/Noise.
    pub(super) fn defer_idle_samples(&mut self, samples: usize) {
        if let Self::Square(square) = self {
            square.defer_idle_samples(samples);
        }
    }

    /// Applies the hardware off-write's frequency truncation
    /// ([`SquareChannel::apply_hardware_off_write`]'s doc) and the noise
    /// channel's zero-volume retrigger, which restarts its phase and LFSR but
    /// leaves the latch for the next clock to settle; a no-op for Wave.
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

    /// Carries a square oscillator's duty position or a noise oscillator's
    /// output latch forward onto its replacement
    /// ([`SquareChannel::continue_duty_from`]'s and
    /// [`NoiseChannel::continue_output_from`]'s docs). A wave oscillator
    /// always keeps the held output nibble, and keeps its retained wave-RAM
    /// rotation only when the replacement names the same wave asset; a
    /// different asset is a fresh upload (`m4a.c:1003-1012`) that the next
    /// wave clock latches from.
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

    /// Re-applies a `CGB_CHANNEL_MO_VOL` volume-write trigger (`m4a.c:1219-1226`)
    /// to the channel's own state and returns whether it still plays.
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
        nr32_attenuated_sample(WaveChannel::new(samples, 0).sample(), level)
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
        // (n + 2n) >> 2: nibbles 0 and 1 both give 0; 5 and 6 give 3 and 4.
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

    /// A changed wave asset uploads new RAM but leaves the held channel-3
    /// output sample alone; only the next wave clock latches from the new
    /// RAM (`mgba/src/gb/audio.c:300-337`).
    #[test]
    fn a_changed_wave_asset_keeps_the_previous_latch_until_the_first_clock() {
        let high = WaveChannel::decode_wave_ram(&[0xFF; 16]);
        let low = WaveChannel::decode_wave_ram(&[0x00; 16]);
        let previous = Oscillator::Wave {
            wave_id: Arc::from("old"),
            channel: WaveChannel::new(high, 0x7FF),
        };
        let mut replacement = Oscillator::Wave {
            wave_id: Arc::from("new"),
            channel: WaveChannel::new(low, 0x7FF),
        };
        replacement.carry_hardware_state_from(&previous);
        let Oscillator::Wave { channel, .. } = &mut replacement else {
            unreachable!()
        };
        assert_eq!(channel.sample(), 7, "old latch before first clock");
        // 0x7FF clocks the wave every sample, so the new RAM follows.
        assert_eq!(channel.sample(), -8, "new waveform after the first clock");
    }
}
