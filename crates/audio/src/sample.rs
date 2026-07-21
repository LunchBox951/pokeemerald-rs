//! A DirectSound wave: a block of signed 8-bit PCM plus the metadata the
//! mixer needs to pitch and loop it.
//!
//! Behavioural model of upstream `struct WaveData` (`m4a_internal.h:39`). The
//! GBA packs `type`/`status`/`freq`/`loopStart`/`size` ahead of an inline
//! `s8 data[]`; here the loop metadata is normalised into explicit fields and
//! the samples live in an owned `Vec<i8>` `(oop-boundaries)`. The compressed
//! (`TONEDATA_TYPE_CMP`) and reversed (`TONEDATA_TYPE_REV`) encodings are out
//! of scope for this slice.

/// An owned, decoded DirectSound sample.
#[derive(Clone, Debug)]
pub struct WaveData {
    /// Pre-scaled base pitch constant (`WaveData::freq`), fed to
    /// [`crate::pitch::midi_key_to_freq`] as `wav_freq`.
    freq: u32,
    /// First sample index of the loop region; ignored unless [`Self::looping`]
    /// is set.
    loop_start: u32,
    /// Whether playback wraps to [`Self::loop_start`] on reaching the end
    /// (`WAVE_DATA_FLAG_LOOP`, `m4a_1.s:198`).
    looping: bool,
    /// The signed 8-bit PCM samples.
    data: Vec<i8>,
}

impl WaveData {
    /// A one-shot (non-looping) wave with base pitch `freq`.
    #[must_use]
    pub fn one_shot(freq: u32, data: Vec<i8>) -> Self {
        Self {
            freq,
            loop_start: 0,
            looping: false,
            data,
        }
    }

    /// A looping wave that wraps to `loop_start` after its last sample.
    ///
    /// `loop_start` is clamped into range; a loop region must contain at least
    /// one sample for playback to make progress.
    #[must_use]
    pub fn looping(freq: u32, loop_start: u32, data: Vec<i8>) -> Self {
        let max_start = data.len().saturating_sub(1);
        let loop_start = loop_start.min(u32::try_from(max_start).unwrap_or(u32::MAX));
        Self {
            freq,
            loop_start,
            looping: true,
            data,
        }
    }

    /// The wave's pre-scaled base pitch constant.
    #[must_use]
    pub fn freq(&self) -> u32 {
        self.freq
    }

    /// The loop start index (only meaningful when [`Self::is_looping`]).
    #[must_use]
    pub fn loop_start(&self) -> usize {
        self.loop_start as usize
    }

    /// Whether the wave loops.
    #[must_use]
    pub fn is_looping(&self) -> bool {
        self.looping
    }

    /// The raw sample slice.
    #[must_use]
    pub fn samples(&self) -> &[i8] {
        &self.data
    }

    /// Number of samples in the wave.
    #[must_use]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Whether the wave carries no samples.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
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
        // Clamped to the last valid index.
        assert_eq!(w.loop_start(), 3);
    }
}
