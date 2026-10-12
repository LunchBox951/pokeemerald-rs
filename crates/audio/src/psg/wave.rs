use super::common::{phase_delta, register_frequency_hz, PHASE_ONE};

const WAVE_CLOCK_HZ: f64 = 65_536.0;
const WAVE_STEPS_PER_CYCLE: f64 = 32.0;
const WAVE_RAM_BYTES: usize = 16;
const WAVE_SAMPLES: usize = WAVE_RAM_BYTES * 2;
const NIBBLE_ZERO: i8 = 8;

/// CGB channel 3 generator over 32 decoded wave-RAM samples.
///
/// Samples remain unscaled because M4A applies `gCgb3Vol` through `NR32`
/// (`pokeemerald/src/m4a.c:1205-1212`).
#[derive(Clone, Debug)]
pub struct WaveChannel {
    samples: [i8; WAVE_SAMPLES],
    phase: u32,
    step_delta: u32,
}

impl WaveChannel {
    /// Decodes two high-nibble-first samples per wave-RAM byte, centered on zero.
    #[must_use]
    pub fn decode_wave_ram(bytes: &[u8; WAVE_RAM_BYTES]) -> [i8; WAVE_SAMPLES] {
        let mut samples = [0i8; WAVE_SAMPLES];
        for (i, &byte) in bytes.iter().enumerate() {
            #[expect(clippy::cast_possible_wrap, reason = "a nibble fits in i8")]
            let high = (byte >> 4) as i8 - NIBBLE_ZERO;
            #[expect(clippy::cast_possible_wrap, reason = "a nibble fits in i8")]
            let low = (byte & 0x0F) as i8 - NIBBLE_ZERO;
            samples[i * 2] = high;
            samples[i * 2 + 1] = low;
        }
        samples
    }

    /// Creates a wave channel from decoded samples and a frequency register value.
    #[must_use]
    pub fn new(samples: [i8; WAVE_SAMPLES], freq_reg: u16) -> Self {
        let mut chan = Self {
            samples,
            phase: 0,
            step_delta: 0,
        };
        chan.set_frequency(freq_reg);
        chan
    }

    /// Retunes the channel from an 11-bit frequency register value, keeping
    /// the sample index but starting a fresh period at the new rate
    /// (`mgba/src/gb/audio.c:331-335`).
    pub fn set_frequency(&mut self, freq_reg: u16) {
        let hz = register_frequency_hz(freq_reg, WAVE_CLOCK_HZ);
        self.step_delta = phase_delta(hz, WAVE_STEPS_PER_CYCLE);
        self.phase &= !(PHASE_ONE - 1);
    }

    /// Produces the next unscaled decoded sample in `-8..=7`.
    pub fn sample(&mut self) -> i8 {
        let step = (self.phase / PHASE_ONE) as usize % self.samples.len();
        self.phase = self.phase.wrapping_add(self.step_delta);
        self.samples[step]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wave_ram_decodes_high_nibble_first() {
        let bytes = [0xF0, 0x08, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let samples = WaveChannel::decode_wave_ram(&bytes);
        assert_eq!(&samples[..4], &[7, -8, -8, 0]);
    }

    #[test]
    fn wave_channel_emits_raw_decoded_nibbles() {
        let full_amplitude_wave = WaveChannel::decode_wave_ram(&[0xF0; WAVE_RAM_BYTES]);
        let mut wave = WaveChannel::new(full_amplitude_wave, 0);
        wave.step_delta = 0;
        assert_eq!(wave.sample(), 7);
    }

    fn samples_until_next_wave_step(mut chan: WaveChannel) -> u32 {
        let index = chan.phase / PHASE_ONE;
        (1..=PHASE_ONE)
            .find(|_| {
                chan.sample();
                chan.phase / PHASE_ONE != index
            })
            .expect("a wave channel always reaches its next step")
    }

    /// A retune mid-step keeps the sample index but discards the fraction
    /// of the old step already elapsed, so the new rate's first step lands
    /// one new period after the retune itself, not after the step it
    /// interrupted (`WaveChannel::set_frequency`'s doc).
    #[test]
    fn a_wave_retune_mid_step_lands_the_next_step_one_new_period_after_the_retune() {
        const FIRST_FREQUENCY: u16 = 0x400;
        const RETUNED_FREQUENCY: u16 = 0x200;
        const SAMPLES_BEFORE_RETUNE: u32 = 3;

        let wave_ram = WaveChannel::decode_wave_ram(&[0xF0; WAVE_RAM_BYTES]);
        let mut wave = WaveChannel::new(wave_ram, FIRST_FREQUENCY);
        // Walk into the middle of a step so a remainder is actually carried.
        while wave.phase / PHASE_ONE == 0 {
            wave.sample();
        }
        let last_step_index = wave.phase / PHASE_ONE;
        for _ in 0..SAMPLES_BEFORE_RETUNE {
            wave.sample();
        }
        assert_eq!(
            wave.phase / PHASE_ONE,
            last_step_index,
            "the retune case must stay within the same step",
        );

        wave.set_frequency(RETUNED_FREQUENCY);
        assert_eq!(
            wave.phase % PHASE_ONE,
            0,
            "the retune must discard the elapsed fraction of the old step",
        );
        assert_eq!(
            wave.phase / PHASE_ONE,
            last_step_index,
            "the retune must keep the sample index",
        );
        let retuned_period = f64::from(PHASE_ONE) / f64::from(wave.step_delta);

        let next_step = f64::from(samples_until_next_wave_step(wave));
        assert!(
            (next_step - retuned_period).abs() <= 1.0,
            "the retuned wave steps after {next_step} samples, expected about {retuned_period}",
        );
    }
}
