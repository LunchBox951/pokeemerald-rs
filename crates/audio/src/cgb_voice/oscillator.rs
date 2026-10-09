use super::{
    cgb3_wave_gain_256, DacCorrection, BIPOLAR_SAMPLE_SCALE, FULL_GAIN_256, LINEAR_ENVELOPE_SCALE,
    SAMPLE_GAIN_BITS, WAVE_SAMPLE_SCALE,
};
use crate::cgb_pitch::{midi_key_to_cgb_freq_reg, midi_key_to_noise_control};
use crate::psg::{NoiseChannel, SquareChannel, WaveChannel};

#[derive(Clone, Debug)]
pub(super) enum Oscillator {
    Square(SquareChannel),
    Wave(WaveChannel),
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
    /// `envelope_volume` selects the Wave arm's NR32 attenuation; Square and
    /// Noise ignore it (their gain is applied after this call).
    pub(super) fn normalized_sample(&mut self, envelope_volume: u8) -> i32 {
        match self {
            Self::Square(square) => i32::from(square.sample()) * BIPOLAR_SAMPLE_SCALE,
            Self::Wave(wave) => nr32_attenuated_sample(wave.sample(), envelope_volume),
            Self::Noise(noise) => i32::from(noise.sample()) * BIPOLAR_SAMPLE_SCALE,
        }
    }

    /// `hardware_volume` is [`HardwareEnvelopeVolume`](crate::cgb_envelope::HardwareEnvelopeVolume)'s resolved byte; the
    /// Wave arm ignores both: NR32 attenuation is an integer shift of the
    /// unsigned nibble, applied per sample by [`nr32_attenuated_sample`].
    pub(super) fn envelope_gain_256(&self, _envelope_volume: u8, hardware_volume: u8) -> u32 {
        match self {
            Self::Wave(_) => FULL_GAIN_256,
            Self::Square(_) | Self::Noise(_) => u32::from(hardware_volume) * LINEAR_ENVELOPE_SCALE,
        }
    }

    pub(super) fn retune(&mut self, note_key: u8, fine_pitch: u8, correction: DacCorrection) {
        match self {
            Self::Square(square) => square
                .set_frequency(correction.apply(midi_key_to_cgb_freq_reg(note_key, fine_pitch))),
            Self::Wave(wave) => {
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
    /// ([`SquareChannel::apply_hardware_off_write`]'s doc); a no-op for
    /// Wave/Noise.
    pub(super) fn apply_hardware_off_write(&mut self) -> bool {
        match self {
            Self::Square(square) => square.apply_hardware_off_write(),
            Self::Wave(_) | Self::Noise(_) => true,
        }
    }

    pub(super) fn step_sweep_tick(&mut self) -> bool {
        match self {
            Self::Square(square) => square.step_sweep_tick(),
            Self::Wave(_) | Self::Noise(_) => true,
        }
    }

    /// Carries a square oscillator's duty position forward onto its replacement
    /// ([`SquareChannel::continue_duty_from`]'s doc); a no-op for Wave/Noise.
    pub(super) fn carry_duty_phase_from(&mut self, other: &Self) {
        if let (Self::Square(square), Self::Square(previous)) = (self, other) {
            square.continue_duty_from(previous);
        }
    }

    /// Re-applies a `CGB_CHANNEL_MO_VOL` volume-write trigger (`m4a.c:1219-1226`)
    /// to the channel's own state and returns whether it still plays.
    pub(super) fn retrigger(&mut self) -> bool {
        match self {
            Self::Square(square) => square.retrigger(),
            Self::Wave(_) => true,
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
        Oscillator::Wave(WaveChannel::new(samples, 0)).normalized_sample(level)
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
