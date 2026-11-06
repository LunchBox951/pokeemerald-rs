use super::{
    cgb3_wave_gain_256, DacCorrection, BIPOLAR_SAMPLE_SCALE, FULL_GAIN_256, LINEAR_ENVELOPE_SCALE,
    SAMPLE_GAIN_BITS, WAVE_SAMPLE_SCALE,
};
use crate::cgb_pitch::{midi_key_to_cgb_freq_reg, midi_key_to_noise_control};
use crate::psg::{NoiseChannel, SquareChannel, WaveChannel};
use std::sync::Arc;

/// One sample's pre-routing PSG output as the port renders it: `signal` is
/// centred on the channel's midpoint, and `offset` is that removed midpoint, so
/// `signal + offset` is the unipolar value mGBA's channel produces
/// (`mgba/src/gb/audio.c:543-574,641,916-918`). The saturation in
/// `mgba/src/gba/audio.c:343-351` acts on the unipolar sum, so the mixer needs
/// both (`Mixer::mix_frame`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Contribution {
    pub(super) signal: i32,
    pub(super) offset: i32,
}

impl Contribution {
    /// A silent channel: neither signal nor midpoint.
    pub(super) const SILENT: Self = Self {
        signal: 0,
        offset: 0,
    };
}

/// Clocks `noise` once at `hardware_volume` and renders its held latch
/// (`mgba/src/gb/audio.c:782`) centred on the volume it was last resolved against.
/// Only a clock or envelope step moves the level, so a trigger, volume write,
/// or replacement leaves it unchanged. The unipolar output is `4 * level`
/// (`mgba/src/gb/audio.c:641`), so the midpoint is the part of it the centred
/// level `2 * level - centre` does not carry.
fn noise_contribution(noise: &mut NoiseChannel, hardware_volume: u8) -> Contribution {
    noise.clock_sample(hardware_volume);
    let scale = |level: i32| (level * NOISE_LEVEL_GAIN * BIPOLAR_SAMPLE_SCALE) >> SAMPLE_GAIN_BITS;
    let signal = scale(noise.centred_level());
    Contribution {
        signal,
        offset: scale(2 * i32::from(noise.level())) - signal,
    }
}

/// [`LINEAR_ENVELOPE_SCALE`] as a signed gain.
const NOISE_LEVEL_GAIN: i32 = LINEAR_ENVELOPE_SCALE.cast_signed();

/// A square's output at its centred `level` (`+-1`). The unipolar duty is
/// `0` or `volume` (`mgba/src/gb/audio.c:916-918`), so the midpoint is the
/// contribution of a `+1` level: the same `frame_gain` and scale on half the
/// swing, exact for odd volumes.
fn square_contribution(frame_gain: i32, level: i8) -> Contribution {
    let scale = |level: i32| (frame_gain * level * BIPOLAR_SAMPLE_SCALE) >> SAMPLE_GAIN_BITS;
    Contribution {
        signal: scale(i32::from(level)),
        offset: scale(1),
    }
}

/// A wave latch's output: `level` was centred on `midpoint` nibble units when
/// it latched (`mgba/src/gb/audio.c:567-574`).
fn wave_contribution(frame_gain: i32, level: i32, midpoint: i32) -> Contribution {
    let scale = |level: i32| (frame_gain * level * WAVE_SAMPLE_SCALE) >> SAMPLE_GAIN_BITS;
    Contribution {
        signal: scale(level),
        offset: scale(midpoint),
    }
}

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
    ) -> Contribution {
        match self {
            Self::Square(square) => square_contribution(frame_gain, square.sample()),
            Self::Wave { channel: wave, .. } => {
                let gain = cgb3_wave_gain_256(envelope_volume);
                let (level, midpoint) = wave.sample_with_midpoint(gain);
                wave_contribution(frame_gain, level, midpoint)
            }
            Self::Noise(noise) => noise_contribution(noise, hardware_volume),
        }
    }

    /// A square's contribution at a `level` its register write latched,
    /// without stepping it; `None` for Wave and Noise.
    pub(super) fn relatched_square_contribution(
        &self,
        level: i8,
        frame_gain: i32,
    ) -> Option<Contribution> {
        match self {
            Self::Square(_) => Some(square_contribution(frame_gain, level)),
            Self::Wave { .. } | Self::Noise(_) => None,
        }
    }

    /// The first pass's `NRx1` store, as the byte the length counter loads
    /// from. A square sums its duty into the byte and re-selects the duty from
    /// the result ([`SquareChannel::write_nrx1`]); noise stores the raw length
    /// byte; wave stores the whole byte as the 8-bit `NR31` length, with no
    /// duty bits to re-select.
    pub(super) fn write_nrx1(&mut self, length_register: u8) -> u8 {
        match self {
            Self::Square(square) => square.write_nrx1(length_register),
            Self::Noise(_) | Self::Wave { .. } => length_register,
        }
    }

    /// A stopped wave channel's held output at full envelope gain, with no
    /// clock (`mgba/src/gb/audio.c:246-253,768-774`); `None` for the other
    /// channels, whose idle outputs are accounted separately.
    pub(super) fn held_wave_contribution(&self) -> Option<Contribution> {
        match self {
            Self::Wave { channel, .. } => {
                let (level, midpoint) = channel.held_output_with_midpoint();
                Some(wave_contribution(
                    i32::try_from(FULL_GAIN_256).unwrap_or(i32::MAX),
                    level,
                    midpoint,
                ))
            }
            Self::Square(_) | Self::Noise(_) => None,
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

    /// Retunes the channel; a square returns the level its `NR14` write's
    /// catch-up latched before the high-byte store
    /// ([`SquareChannel::set_frequency_latching`]).
    pub(super) fn retune(
        &mut self,
        note_key: u8,
        fine_pitch: u8,
        correction: DacCorrection,
    ) -> Option<i8> {
        match self {
            Self::Square(square) => Some(square.set_frequency_latching(
                correction.apply(midi_key_to_cgb_freq_reg(note_key, fine_pitch)),
            )),
            Self::Wave { channel: wave, .. } => {
                wave.set_frequency(
                    correction.apply(midi_key_to_cgb_freq_reg(note_key, fine_pitch)),
                );
                None
            }
            Self::Noise(noise) => {
                noise.retune(midi_key_to_noise_control(note_key));
                None
            }
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
    /// the next clock to settle; Wave's `NR30` off-write stops its clock and
    /// leaves the latch (`mgba/src/gb/audio.c:246-253`).
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
            (Self::Square(square), Self::Square(previous)) => {
                square.inherit_hardware_from(previous);
            }
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

    /// Plays the replacement's note-on writes against the noise or square
    /// predecessor it inherited; a no-op otherwise.
    pub(super) fn commit_initial_writes(&mut self, retires_on_first_pass: bool) {
        match self {
            Self::Square(square) => square.commit_initial_writes(retires_on_first_pass),
            Self::Noise(noise) => noise.commit_initial_writes(retires_on_first_pass),
            Self::Wave { .. } => {}
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
            previous.frame_contribution(256, 0, 15).signal,
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
            previous.frame_contribution(256, 0, 15).signal,
            7 * WAVE_SAMPLE_SCALE
        );
        let mut replacement = wave("a");
        replacement.carry_hardware_state_from(&previous);
        assert_eq!(
            replacement.frame_contribution(256, 0, 2).signal,
            7 * WAVE_SAMPLE_SCALE,
            "held output survives the volume change"
        );
        assert_eq!(
            replacement.frame_contribution(256, 0, 2).signal,
            WAVE_SAMPLE_SCALE
        );
    }
}
