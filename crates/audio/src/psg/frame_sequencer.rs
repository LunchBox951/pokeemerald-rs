use crate::pitch::MIXER_RATE;

/// The sample offsets at which one buffer's frame-sequencer steps clock the
/// sweep and the length counters, plus the step in effect when it began.
#[derive(Clone, Debug)]
pub struct FrameClocks {
    /// The last step the sequencer completed before this buffer; a register
    /// write at the buffer's start sees this step's parity
    /// (`mgba/src/gb/audio.c:174,189`).
    pub start_step: u8,
    /// Ascending offsets of the 128 Hz sweep clocks (steps 2 and 6).
    pub sweep: Vec<usize>,
    /// Ascending offsets of the 256 Hz length clocks (steps 0, 2, 4 and 6).
    pub length: Vec<usize>,
}

impl FrameClocks {
    /// An empty set of clocks with room for one frame of each.
    #[must_use]
    pub fn with_capacity(sweep: usize, length: usize) -> Self {
        Self {
            start_step: FrameSequencer::INITIAL_STEP,
            sweep: Vec::with_capacity(sweep),
            length: Vec::with_capacity(length),
        }
    }
}

/// Schedules the CGB frame sequencer's 512 Hz steps without quantizing them to
/// render buffers: sweep on steps 2 and 6 (128 Hz) and length on the even
/// steps (256 Hz) (`mgba/src/gb/audio.c:649-707`).
#[derive(Clone, Copy, Debug)]
pub struct FrameSequencer {
    tick_accumulator: u32,
    /// The step most recently completed, 0..=7.
    step: u8,
}

impl Default for FrameSequencer {
    fn default() -> Self {
        Self {
            tick_accumulator: 0,
            step: Self::INITIAL_STEP,
        }
    }
}

impl FrameSequencer {
    /// The step completed before the first sample. mGBA's boot phase
    /// (`mgba/src/gb/audio.c:116,466-468`) is not represented here; step 6 is
    /// the epoch that puts the first sweep clock at the established offset 104,
    /// four steps (26 samples apiece) on.
    pub const INITIAL_STEP: u8 = 6;
    const STEP_HZ: u32 = 512;
    const STEPS: u8 = 8;

    /// The step most recently completed.
    #[must_use]
    pub fn step(&self) -> u8 {
        self.step
    }

    /// A sequencer whose last completed step is `step` (0..=7), as a test seeds one.
    #[must_use]
    pub fn at_step(step: u8) -> Self {
        Self {
            tick_accumulator: 0,
            step: step % Self::STEPS,
        }
    }

    /// Whether an NRx4 write now precedes a step that does not clock length
    /// (`!(frame & 1)`, `mgba/src/gb/audio.c:174,189`).
    #[must_use]
    pub fn next_step_skips_length(step: u8) -> bool {
        step & 1 == 0
    }

    fn advance_steps(&mut self, samples: usize, mut on_step: impl FnMut(usize, u8)) {
        for sample_offset in 0..samples {
            self.tick_accumulator += Self::STEP_HZ;
            if self.tick_accumulator >= MIXER_RATE {
                self.tick_accumulator -= MIXER_RATE;
                self.step = (self.step + 1) % Self::STEPS;
                on_step(sample_offset, self.step);
            }
        }
    }

    /// Replaces `clocks` with the sweep and length offsets for `samples`
    /// output samples.
    pub fn advance_clocks_into(&mut self, samples: usize, clocks: &mut FrameClocks) {
        clocks.start_step = self.step;
        clocks.sweep.clear();
        clocks.length.clear();
        self.advance_steps(samples, |offset, step| {
            if step & 1 == 0 {
                clocks.length.push(offset);
            }
            if step == 2 || step == 6 {
                clocks.sweep.push(offset);
            }
        });
    }

    /// Replaces `ticks` with the ascending sample offsets where sweep ticks occur.
    pub fn advance_into(&mut self, samples: usize, ticks: &mut Vec<usize>) {
        ticks.clear();
        self.advance_steps(samples, |offset, step| {
            if step == 2 || step == 6 {
                ticks.push(offset);
            }
        });
    }

    /// Returns the sweep-tick offsets for `samples` output samples.
    #[must_use]
    pub fn advance(&mut self, samples: usize) -> Vec<usize> {
        let mut ticks = Vec::new();
        self.advance_into(samples, &mut ticks);
        ticks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_sequencer_128hz_pins_the_hardware_tick_offsets() {
        let mut clock = FrameSequencer::default();
        let ticks = clock.advance(1200);
        assert_eq!(
            ticks,
            vec![104, 209, 313, 418, 522, 627, 731, 836, 940, 1045, 1149]
        );
        let spacings: Vec<usize> = ticks
            .windows(2)
            .map(|window| window[1] - window[0])
            .collect();
        let floor_spacing = usize::try_from(MIXER_RATE / 128).expect("sample spacing fits usize");
        assert!(
            spacings
                .iter()
                .all(|&spacing| spacing == floor_spacing || spacing == floor_spacing + 1),
            "unexpected tick spacing: {spacings:?}"
        );
    }

    #[test]
    fn frame_sequencer_128hz_does_not_drift_over_long_runs() {
        let one_second = usize::try_from(MIXER_RATE).expect("MIXER_RATE fits a usize");

        let mut one_second_clock = FrameSequencer::default();
        assert_eq!(one_second_clock.advance(one_second).len(), 128);

        let mut ten_second_clock = FrameSequencer::default();
        assert_eq!(ten_second_clock.advance(10 * one_second).len(), 1280);
    }

    #[test]
    fn frame_sequencer_128hz_chunk_boundary_invariance() {
        let mut whole = FrameSequencer::default();
        let whole_ticks = whole.advance(600);

        let mut split = FrameSequencer::default();
        let first = split.advance(300);
        let second = split.advance(300);
        let mut combined = first;
        combined.extend(second.into_iter().map(|t| t + 300));

        assert_eq!(whole_ticks, combined);
    }

    #[test]
    fn the_default_epoch_keeps_the_established_sweep_offsets_and_adds_length_clocks() {
        let mut clock = FrameSequencer::default();
        let mut clocks = FrameClocks::with_capacity(0, 0);
        clock.advance_clocks_into(1200, &mut clocks);
        assert_eq!(clocks.start_step, 6);
        assert_eq!(
            clocks.sweep,
            vec![104, 209, 313, 418, 522, 627, 731, 836, 940, 1045, 1149],
            "the 128 Hz view is unchanged"
        );
        assert_eq!(
            clocks.length[..8],
            [52, 104, 156, 209, 261, 313, 365, 418],
            "length clocks fall on steps 0, 2, 4 and 6"
        );
        assert_eq!(clock.step(), 3, "45 steps from step 6 land on step 3");
    }
}
