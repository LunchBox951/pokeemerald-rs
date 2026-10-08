use super::{
    cgb3_wave_gain_256, DacCorrection, BIPOLAR_SAMPLE_SCALE, LINEAR_ENVELOPE_SCALE,
    WAVE_SAMPLE_SCALE,
};
use crate::cgb_pitch::{midi_key_to_cgb_freq_reg, midi_key_to_noise_control};
use crate::psg::{NoiseChannel, SquareChannel, WaveChannel};

#[derive(Clone, Debug)]
pub(super) enum Oscillator {
    Square(SquareChannel),
    Wave(WaveChannel),
    Noise(NoiseChannel),
}

impl Oscillator {
    pub(super) fn normalized_sample(&mut self) -> i32 {
        match self {
            Self::Square(square) => i32::from(square.sample()) * BIPOLAR_SAMPLE_SCALE,
            Self::Wave(wave) => i32::from(wave.sample()) * WAVE_SAMPLE_SCALE,
            Self::Noise(noise) => i32::from(noise.sample()) * BIPOLAR_SAMPLE_SCALE,
        }
    }

    /// `hardware_volume` is [`HardwareEnvelopeVolume`](crate::cgb_envelope::HardwareEnvelopeVolume)'s resolved byte; the
    /// Wave arm ignores it and reads `envelope_volume` (0..=31) through its
    /// own `gCgb3Vol` lookup instead (`m4a.c:1211`), since NR32 has no such
    /// register.
    pub(super) fn envelope_gain_256(&self, envelope_volume: u8, hardware_volume: u8) -> u32 {
        match self {
            Self::Wave(_) => cgb3_wave_gain_256(envelope_volume),
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
    /// ([`SquareChannel::apply_hardware_off_write`]'s doc) and the noise
    /// channel's settling to its low latch
    /// ([`NoiseChannel::settle_output_low`]'s doc); a no-op for Wave.
    pub(super) fn apply_hardware_off_write(&mut self) -> bool {
        match self {
            Self::Square(square) => square.apply_hardware_off_write(),
            Self::Noise(noise) => {
                noise.settle_output_low();
                true
            }
            Self::Wave(_) => true,
        }
    }

    pub(super) fn step_sweep_tick(&mut self) -> bool {
        match self {
            Self::Square(square) => square.step_sweep_tick(),
            Self::Wave(_) | Self::Noise(_) => true,
        }
    }

    /// Carries a square oscillator's duty position or a noise oscillator's
    /// output latch forward onto its replacement
    /// ([`SquareChannel::continue_duty_from`]'s and
    /// [`NoiseChannel::continue_output_from`]'s docs); a no-op for Wave.
    /// `previous_volume_zero` is whether the predecessor's hardware volume
    /// was zero, so its noise clocks latched low.
    pub(super) fn carry_hardware_state_from(&mut self, other: &Self, previous_volume_zero: bool) {
        match (self, other) {
            (Self::Square(square), Self::Square(previous)) => square.continue_duty_from(previous),
            (Self::Noise(noise), Self::Noise(previous)) => {
                noise.continue_output_from(previous, previous_volume_zero);
            }
            _ => {}
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
