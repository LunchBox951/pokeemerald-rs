//! Owned signed 8-bit PCM waves for the DirectSound mixer.

#[derive(Clone, Copy, Debug)]
enum Playback {
    OneShot,
    Looping { start: usize },
}

/// An owned, decoded DirectSound sample.
#[derive(Clone, Debug)]
pub struct WaveData {
    base_frequency: u32,
    playback: Playback,
    samples: Vec<i8>,
}

impl WaveData {
    /// Construct a one-shot wave with the given base frequency.
    #[must_use]
    pub fn one_shot(base_frequency: u32, samples: Vec<i8>) -> Self {
        Self {
            base_frequency,
            playback: Playback::OneShot,
            samples,
        }
    }

    /// Construct a looping wave, clamping its loop start to the final sample or
    /// zero when empty.
    #[must_use]
    pub fn looping(base_frequency: u32, loop_start: u32, samples: Vec<i8>) -> Self {
        let requested_start = usize::try_from(loop_start).unwrap_or(usize::MAX);
        let start = requested_start.min(samples.len().saturating_sub(1));
        Self {
            base_frequency,
            playback: Playback::Looping { start },
            samples,
        }
    }

    /// Return the wave's pre-scaled base frequency.
    #[must_use]
    pub fn freq(&self) -> u32 {
        self.base_frequency
    }

    /// Return the loop start, or zero for a one-shot wave.
    #[must_use]
    pub fn loop_start(&self) -> usize {
        match self.playback {
            Playback::OneShot => 0,
            Playback::Looping { start } => start,
        }
    }

    /// Return whether playback loops.
    #[must_use]
    pub fn is_looping(&self) -> bool {
        matches!(self.playback, Playback::Looping { .. })
    }

    /// Return the signed PCM samples.
    #[must_use]
    pub fn samples(&self) -> &[i8] {
        &self.samples
    }

    /// Return the number of samples.
    #[must_use]
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Return whether the wave has no samples.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_shot_has_no_loop() {
        let w = WaveData::one_shot(1 << 20, vec![0, 1, 2, 3]);
        assert!(!w.is_looping());
        assert_eq!(w.len(), 4);
        assert!(!w.is_empty());
        assert_eq!(w.samples(), &[0, 1, 2, 3]);
    }

    #[test]
    fn looping_clamps_loop_start_into_range() {
        let w = WaveData::looping(1 << 20, 99, vec![0, 1, 2, 3]);
        assert!(w.is_looping());
        assert_eq!(w.loop_start(), 3);
    }
}
