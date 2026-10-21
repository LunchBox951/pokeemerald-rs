//! Owned signed 8-bit PCM waves for the DirectSound mixer.

#[derive(Clone, Copy, Debug)]
enum Playback {
    OneShot,
    Looping { start: usize },
}

/// An owned, decoded DirectSound sample.
///
/// `samples` may hold one more value than [`Self::logical_len`]: an
/// interpolation guard past the point where a loop wraps or a one-shot retires.
/// The mismatch is upstream's `agbl` override trimming the logical count (see
/// `pokeemerald/src/m4a_1.s:399-407`).
#[derive(Clone, Debug)]
pub struct WaveData {
    base_frequency: u32,
    playback: Playback,
    samples: Vec<i8>,
    logical_len: usize,
}

impl WaveData {
    /// Construct a one-shot wave with every sample logically playable.
    #[must_use]
    pub fn one_shot(base_frequency: u32, samples: Vec<i8>) -> Self {
        let logical_len = samples.len();
        Self {
            base_frequency,
            playback: Playback::OneShot,
            samples,
            logical_len,
        }
    }

    /// Construct a looping wave with every sample logically playable, clamping
    /// its loop start to the final sample or zero when empty.
    #[must_use]
    pub fn looping(base_frequency: u32, loop_start: u32, samples: Vec<i8>) -> Self {
        let requested_start = usize::try_from(loop_start).unwrap_or(usize::MAX);
        let start = requested_start.min(samples.len().saturating_sub(1));
        let logical_len = samples.len();
        Self {
            base_frequency,
            playback: Playback::Looping { start },
            samples,
            logical_len,
        }
    }

    /// Narrow the logical length, keeping the trailing samples as interpolation
    /// guard.
    ///
    /// Clamps to the buffer length and re-clamps a looping start.
    #[must_use]
    pub fn with_logical_len(mut self, logical_len: usize) -> Self {
        self.logical_len = logical_len.min(self.samples.len());
        if let Playback::Looping { start } = &mut self.playback {
            *start = (*start).min(self.logical_len.saturating_sub(1));
        }
        self
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

    /// Return the signed PCM samples, including any retained interpolation
    /// guard past [`Self::logical_len`].
    #[must_use]
    pub fn samples(&self) -> &[i8] {
        &self.samples
    }

    /// Return the buffer length, including any retained interpolation guard.
    #[must_use]
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Return the count where a loop wraps or a one-shot retires, at most
    /// [`Self::len`].
    #[must_use]
    pub fn logical_len(&self) -> usize {
        self.logical_len
    }

    /// Return whether the wave has no logically playable samples.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.logical_len == 0
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
        assert_eq!(w.logical_len(), 4);
        assert!(!w.is_empty());
        assert_eq!(w.samples(), &[0, 1, 2, 3]);
    }

    #[test]
    fn looping_clamps_loop_start_into_range() {
        let w = WaveData::looping(1 << 20, 99, vec![0, 1, 2, 3]);
        assert!(w.is_looping());
        assert_eq!(w.loop_start(), 3);
        assert_eq!(w.logical_len(), 4);
    }

    #[test]
    fn with_logical_len_retains_a_trailing_guard_sample() {
        let w = WaveData::one_shot(1 << 20, vec![0, 1, 2, 3]).with_logical_len(3);
        assert_eq!(w.logical_len(), 3);
        assert_eq!(w.len(), 4);
        assert!(!w.is_empty());
        assert_eq!(w.samples(), &[0, 1, 2, 3]);
    }

    #[test]
    fn with_logical_len_reclamps_a_looping_start_past_the_narrowed_range() {
        let w = WaveData::looping(1 << 20, 3, vec![0, 1, 2, 3]).with_logical_len(2);
        assert_eq!(w.loop_start(), 1);
        assert_eq!(w.logical_len(), 2);
    }

    #[test]
    fn with_logical_len_clamps_past_the_buffer_length() {
        let w = WaveData::one_shot(1 << 20, vec![0, 1]).with_logical_len(99);
        assert_eq!(w.logical_len(), 2);
    }

    #[test]
    fn a_zero_logical_len_is_empty_even_with_a_retained_guard_sample() {
        let w = WaveData::one_shot(1 << 20, vec![5]).with_logical_len(0);
        assert!(w.is_empty());
        assert_eq!(w.len(), 1);
    }
}
