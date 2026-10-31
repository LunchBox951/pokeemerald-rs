use super::common::{
    phase_delta, register_frequency_hz, FREQUENCY_REGISTER_RANGE, MAX_FREQUENCY_REGISTER, PHASE_ONE,
};

const WAVE_CLOCK_HZ: f64 = 65_536.0;
const WAVE_STEPS_PER_CYCLE: f64 = 32.0;
const WAVE_RAM_BYTES: usize = 16;
const WAVE_SAMPLES: usize = WAVE_RAM_BYTES * 2;
const NIBBLE_ZERO: i8 = 8;
const GAIN_BITS: u32 = 8;
const FIRST_CLOCK_DELAY_CYCLES: f64 = 6.0;
const CYCLES_PER_REGISTER_STEP: f64 = 2.0;

/// CGB channel 3 generator over 32 decoded wave-RAM samples.
///
/// M4A applies `gCgb3Vol` through `NR32` (`pokeemerald/src/m4a.c:1205-1212`).
/// The volume shift applies when a sample is latched, not while it is held
/// (`mgba/src/gb/audio.c:549-575,768-774`).
#[derive(Clone, Debug)]
pub struct WaveChannel {
    samples: [i8; WAVE_SAMPLES],
    phase: u32,
    step_delta: u32,
    /// Phase still owed before the first clock after a start or retune, from
    /// the six extra cycles in `mgba/src/gb/audio.c:331-335`.
    first_clock_delay: u32,
    /// The held output, shifted by the NR32 gain it was latched under and
    /// centered on zero. `None` until the first `sample`, since creation does
    /// not know the volume.
    latch: Option<i32>,
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
            first_clock_delay: 0,
            latch: None,
        };
        chan.set_frequency(freq_reg);
        chan
    }

    /// Retunes from an 11-bit frequency register value, keeping the sample
    /// index and starting a fresh period that lasts six cycles longer than
    /// later ones (`mgba/src/gb/audio.c:331-335`).
    pub fn set_frequency(&mut self, freq_reg: u16) {
        let hz = register_frequency_hz(freq_reg, WAVE_CLOCK_HZ);
        self.step_delta = phase_delta(hz, WAVE_STEPS_PER_CYCLE);
        self.phase &= !(PHASE_ONE - 1);
        let period_cycles = CYCLES_PER_REGISTER_STEP
            * (f64::from(FREQUENCY_REGISTER_RANGE)
                - f64::from(freq_reg.min(MAX_FREQUENCY_REGISTER)));
        // Rounded up: `step_delta` is truncated, so a truncated delay could
        // let the delayed clock land one sample early (register 0x0AA).
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "the delay is a positive fraction of at most three phase steps"
        )]
        {
            self.first_clock_delay =
                (FIRST_CLOCK_DELAY_CYCLES / period_cycles * f64::from(PHASE_ONE)).ceil() as u32;
        }
    }

    /// Carries `previous`'s wave-RAM head and latch onto a freshly triggered
    /// channel; the step fraction restarts (`mgba/src/gb/audio.c:549-568`).
    pub(crate) fn continue_rotation_from(&mut self, previous: &Self) {
        self.samples = previous.samples;
        self.phase = previous.phase & !(PHASE_ONE - 1);
        self.latch = previous.latch;
    }

    /// Carries only `previous`'s held output onto a freshly uploaded channel
    /// (`mgba/src/gb/audio.c:300-337`).
    pub(crate) fn continue_latch_from(&mut self, previous: &Self) {
        self.latch = previous.latch;
    }

    fn head(&self) -> usize {
        (self.phase / PHASE_ONE) as usize % self.samples.len()
    }

    /// Shifts the unsigned nibble by `gain_256` (an NR32 gain out of 256) and
    /// re-centers it (`mgba/src/gb/audio.c:519-533,567-574`).
    fn resolve(sample: i8, gain_256: u32) -> i32 {
        let gain = i32::try_from(gain_256).unwrap_or(0);
        let shift = |nibble: i32| (nibble * gain) >> GAIN_BITS;
        let zero = i32::from(NIBBLE_ZERO);
        shift(i32::from(sample) + zero) - shift(zero)
    }

    /// Returns the held output, then advances time, so a call on a phase
    /// crossing yields the pre-clock value and stores the crossed nibble for
    /// the next call. Each wave clock latches the nibble it removes from the head at
    /// `gain_256` (`mgba/src/gb/audio.c:557-568,768-774`); a volume change
    /// alone leaves the held output (`mgba/src/gba/io.c:633-637`). A cold
    /// channel holds a zero nibble until its first clock (`audio.c:92-95,513-517`).
    pub fn sample(&mut self, gain_256: u32) -> i32 {
        let output = match self.latch {
            Some(held) => held,
            None => Self::resolve(-NIBBLE_ZERO, gain_256),
        };
        self.latch = Some(output);
        let old_clock = self.phase / PHASE_ONE;
        let absorbed = self.first_clock_delay.min(self.step_delta);
        self.first_clock_delay -= absorbed;
        self.phase = self.phase.wrapping_add(self.step_delta - absorbed);
        if self.phase / PHASE_ONE != old_clock {
            let outgoing = (self.head() + self.samples.len() - 1) % self.samples.len();
            self.latch = Some(Self::resolve(self.samples[outgoing], gain_256));
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: u32 = 256;

    /// Uploaded RAM does not reach the output before the first wave clock;
    /// once clocked the outputs diverge (`mgba/src/gb/audio.c:92-95,513-517`).
    #[test]
    fn cold_start_output_ignores_wave_ram_until_the_first_wave_clock() {
        const LOW_FREQUENCY: u16 = 0x400;
        let mut loud = WaveChannel::new(WaveChannel::decode_wave_ram(&[0xF0; 16]), LOW_FREQUENCY);
        let mut quiet = WaveChannel::new(WaveChannel::decode_wave_ram(&[0x00; 16]), LOW_FREQUENCY);
        while loud.phase / PHASE_ONE == 0 {
            assert_eq!(loud.sample(FULL), quiet.sample(FULL));
        }
        assert_ne!(loud.sample(FULL), quiet.sample(FULL));
    }

    /// For 0x764 the steady period is 312 cycles and one mixer sample spans
    /// about 313.5, so the first clock at 318 cycles lands at the end of the second sample
    /// interval; later clocks step by the ordinary period
    /// (`mgba/src/gb/audio.c:331-335,513-575`).
    #[test]
    fn first_wave_clock_includes_mgba_six_cycle_delay() {
        const REGISTER: u16 = 0x764;
        let ram = WaveChannel::decode_wave_ram(&[0xF0; WAVE_RAM_BYTES]);
        let mut wave = WaveChannel::new(ram, REGISTER);
        assert_eq!(wave.sample(FULL), -8);
        assert_eq!(wave.head(), 0, "no clock inside the first interval");
        assert_eq!(
            wave.sample(FULL),
            -8,
            "the second sample still holds the cold latch"
        );
        assert_eq!(
            wave.head(),
            1,
            "the delayed clock lands by the end of the second interval"
        );
        assert_eq!(
            wave.sample(FULL),
            i32::from(ram[0]),
            "the first clock latched the head"
        );
        assert_eq!(
            wave.head(),
            2,
            "the next clock (cycle 630) keeps the ordinary period"
        );

        let mut retuned = WaveChannel::new(ram, 0x400);
        retuned.set_frequency(REGISTER);
        assert_eq!(retuned.sample(FULL), -8);
        assert_eq!(retuned.head(), 0, "a retune delays its first clock too");
        assert_eq!(retuned.sample(FULL), -8);
        assert_eq!(retuned.head(), 1);
    }

    /// Register 0x0AA's delayed first clock falls at cycle 3762, just after
    /// twelve mixer intervals (3761.989 cycles), so it belongs to the
    /// thirteenth sample (`mgba/src/gb/audio.c:331-335`). Twelve truncated
    /// steps of 5470 less a truncated 104-unit delay would reach one whole
    /// step exactly and latch a sample early, under the wrong NR32 gain.
    #[test]
    fn first_wave_clock_delay_rounds_up_at_the_sample_boundary() {
        const REGISTER: u16 = 0x0AA;
        const HALF: u32 = 128;
        let ram = WaveChannel::decode_wave_ram(&[0xF0; WAVE_RAM_BYTES]);
        let mut wave = WaveChannel::new(ram, REGISTER);
        for _ in 0..12 {
            assert_eq!(wave.sample(FULL), -8);
        }
        assert_eq!(wave.head(), 0, "no clock inside the first twelve intervals");
        assert_eq!(
            wave.sample(HALF),
            -8,
            "the cold latch holds through call 13"
        );
        assert_eq!(
            wave.head(),
            1,
            "the delayed clock lands in the thirteenth interval"
        );
        assert_eq!(
            wave.sample(FULL),
            WaveChannel::resolve(ram[0], HALF),
            "the clock latched the head under the gain in effect when it fired"
        );
    }

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
        wave.step_delta = PHASE_ONE;
        wave.first_clock_delay = 0;
        wave.sample(FULL);
        assert_eq!(wave.sample(FULL), 7);
    }

    fn samples_until_next_wave_step(mut chan: WaveChannel) -> u32 {
        let index = chan.phase / PHASE_ONE;
        (1..=PHASE_ONE)
            .find(|_| {
                chan.sample(FULL);
                chan.phase / PHASE_ONE != index
            })
            .expect("a wave channel always reaches its next step")
    }

    /// A mid-step retune discards the elapsed step fraction.
    #[test]
    fn a_wave_retune_mid_step_lands_the_next_step_one_new_period_after_the_retune() {
        const FIRST_FREQUENCY: u16 = 0x400;
        const RETUNED_FREQUENCY: u16 = 0x200;
        const SAMPLES_BEFORE_RETUNE: u32 = 3;

        let wave_ram = WaveChannel::decode_wave_ram(&[0xF0; WAVE_RAM_BYTES]);
        let mut wave = WaveChannel::new(wave_ram, FIRST_FREQUENCY);
        while wave.phase / PHASE_ONE == 0 {
            wave.sample(FULL);
        }
        let last_step_index = wave.phase / PHASE_ONE;
        for _ in 0..SAMPLES_BEFORE_RETUNE {
            wave.sample(FULL);
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

    #[test]
    fn wave_holds_the_nibble_removed_by_the_last_clock() {
        let wave_ram = WaveChannel::decode_wave_ram(&[0xF0; WAVE_RAM_BYTES]);
        let mut wave = WaveChannel::new(wave_ram, 0);
        wave.step_delta = PHASE_ONE;
        wave.first_clock_delay = 0;
        // The cold latch holds a zero nibble (-8); the first clock removes the
        // head (7), and only the second clock reaches the low nibble (-8).
        assert_eq!(wave.sample(FULL), -8);
        assert_eq!(wave.sample(FULL), 7);
        assert_eq!(wave.sample(FULL), -8);
        assert_eq!(wave.sample(FULL), 7);
    }

    #[test]
    fn after_a_wave_clock_the_output_is_the_nibble_removed_from_the_old_head() {
        let wave_ram = WaveChannel::decode_wave_ram(&[0xF0; WAVE_RAM_BYTES]);
        let mut wave = WaveChannel::new(wave_ram, 0x400);
        while wave.phase / PHASE_ONE == 0 {
            wave.sample(FULL);
        }
        assert_eq!(wave.phase / PHASE_ONE, 1, "exactly one wave clock elapsed");
        assert_eq!(
            wave.sample(FULL),
            i32::from(wave_ram[0]),
            "latched nibble is the removed head (7)"
        );
    }

    #[test]
    fn a_retune_keeps_the_head_and_the_held_latch() {
        let wave_ram = WaveChannel::decode_wave_ram(&[0xF0; WAVE_RAM_BYTES]);
        let mut wave = WaveChannel::new(wave_ram, 0x400);
        while wave.phase / PHASE_ONE == 0 {
            wave.sample(FULL);
        }
        wave.set_frequency(0x200);
        assert_eq!(wave.head(), 1);
        assert_eq!(wave.latch, Some(i32::from(wave_ram[0])));
        assert_eq!(
            wave.sample(FULL),
            i32::from(wave_ram[0]),
            "the old latch holds mid-period"
        );
        assert_eq!(
            wave.sample(FULL),
            i32::from(wave_ram[0]),
            "still held until the next clock"
        );
    }

    #[test]
    fn continuing_the_rotation_carries_the_latch_but_not_the_fraction() {
        let wave_ram = WaveChannel::decode_wave_ram(&[0xF0; WAVE_RAM_BYTES]);
        let mut previous = WaveChannel::new(wave_ram, 0x400);
        while previous.phase / PHASE_ONE == 0 {
            previous.sample(FULL);
        }
        let mut next = WaveChannel::new(wave_ram, 0x400);
        next.continue_rotation_from(&previous);
        assert_eq!(next.head(), 1);
        assert_eq!(next.latch, Some(i32::from(wave_ram[0])));
        assert_eq!(next.phase % PHASE_ONE, 0);
    }

    /// Only the next wave clock latches under a new volume (`mgba/src/gba/io.c:633-637`).
    #[test]
    fn a_volume_change_mid_hold_waits_for_the_next_wave_clock() {
        const QUARTER: u32 = 64;
        let wave_ram = WaveChannel::decode_wave_ram(&[0xFF; WAVE_RAM_BYTES]);
        let mut wave = WaveChannel::new(wave_ram, 0x400);
        assert_eq!(wave.sample(FULL), -8, "first call latches the cold nibble");
        // Every call below holds until the clock-crossing call, which still
        // returns the old latch while latching the next one at quarter volume.
        let mut held = Vec::new();
        while wave.phase / PHASE_ONE == 0 {
            held.push(wave.sample(QUARTER));
        }
        assert!(held.len() > 1, "the hold must span several samples");
        assert!(
            held.iter().all(|&out| out == -8),
            "held output rescaled: {held:?}"
        );
        assert_eq!(wave.sample(FULL), 1, "next clock latched at quarter volume");
    }
}
