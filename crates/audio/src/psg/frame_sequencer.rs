use crate::pitch::MIXER_RATE;

/// Schedules channel-1 sweep ticks without quantizing them to render buffers.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameSequencer128Hz {
    tick_accumulator: u32,
}

impl FrameSequencer128Hz {
    // A 512 Hz CGB frame sequencer clocks sweep on phases 2 and 6
    // (mgba/src/gb/audio.c:659-668).
    const TICK_HZ: u32 = 128;

    /// Replaces `ticks` with the ascending sample offsets where sweep ticks occur.
    pub fn advance_into(&mut self, samples: usize, ticks: &mut Vec<usize>) {
        ticks.clear();
        for sample_offset in 0..samples {
            self.tick_accumulator += Self::TICK_HZ;
            if self.tick_accumulator >= MIXER_RATE {
                self.tick_accumulator -= MIXER_RATE;
                ticks.push(sample_offset);
            }
        }
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
        let mut clock = FrameSequencer128Hz::default();
        let ticks = clock.advance(1200);
        assert_eq!(
            ticks,
            vec![104, 209, 313, 418, 522, 627, 731, 836, 940, 1045, 1149]
        );
        let spacings: Vec<usize> = ticks
            .windows(2)
            .map(|window| window[1] - window[0])
            .collect();
        let floor_spacing = usize::try_from(MIXER_RATE / FrameSequencer128Hz::TICK_HZ)
            .expect("sample spacing fits usize");
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

        let mut one_second_clock = FrameSequencer128Hz::default();
        assert_eq!(one_second_clock.advance(one_second).len(), 128);

        let mut ten_second_clock = FrameSequencer128Hz::default();
        assert_eq!(ten_second_clock.advance(10 * one_second).len(), 1280);
    }

    #[test]
    fn frame_sequencer_128hz_chunk_boundary_invariance() {
        let mut whole = FrameSequencer128Hz::default();
        let whole_ticks = whole.advance(600);

        let mut split = FrameSequencer128Hz::default();
        let first = split.advance(300);
        let second = split.advance(300);
        let mut combined = first;
        combined.extend(second.into_iter().map(|t| t + 300));

        assert_eq!(whole_ticks, combined);
    }
}
