//! A minimal linear-interpolation resampler (S-1): the fallback path for
//! when the audio device does not support the GBA's nominal mixing rate
//! directly (see `crate::audio` for when this is and is not used).
//!
//! [`Resampler`] pulls interleaved frames from a [`crate::ring::Consumer`]
//! one at a time via [`crate::ring::Consumer::pop_or_silence`] — the exact
//! same underrun-safe primitive the no-resampling direct path uses — so
//! underrun accounting behaves identically whether or not resampling is in
//! play.

use crate::ring::Consumer;

/// Pull one interleaved frame (`into.len()` samples) from `consumer`.
fn pull_frame(consumer: &Consumer, into: &mut [f32]) {
    for sample in into {
        *sample = consumer.pop_or_silence();
    }
}

/// Linear-interpolation resampler bridging a [`Consumer`]'s nominal sample
/// rate (what the future `audio` crate renders at) and an audio device's
/// actual negotiated rate.
///
/// Not a general-purpose DSP resampler — linear interpolation is cheap and
/// good enough for the rare case a device refuses the GBA-native rate; it is
/// explicitly out of scope to do anything fancier (band-limited
/// interpolation, etc) per the `cpal` dependency's approved scope (no
/// decoding, no effects).
pub struct Resampler {
    consumer: Consumer,
    channels: usize,
    /// Source frames advanced per output frame (`source_rate / device_rate`).
    step: f64,
    /// Fractional position between `prev` and `next`, in `[0.0, 1.0)`.
    frac: f64,
    prev: Vec<f32>,
    next: Vec<f32>,
    primed: bool,
}

impl Resampler {
    /// Build a resampler pulling `channels`-wide interleaved frames from
    /// `consumer`, nominally produced at `source_rate` Hz, to be emitted at
    /// `device_rate` Hz.
    #[must_use]
    pub fn new(consumer: Consumer, channels: u16, source_rate: u32, device_rate: u32) -> Self {
        let channels = usize::from(channels.max(1));
        Self {
            consumer,
            channels,
            step: f64::from(source_rate) / f64::from(device_rate.max(1)),
            frac: 0.0,
            prev: vec![0.0; channels],
            next: vec![0.0; channels],
            primed: false,
        }
    }

    /// Total samples the underlying [`Consumer`] has filled with silence due
    /// to underrun so far.
    #[must_use]
    pub fn underruns(&self) -> u64 {
        self.consumer.underruns()
    }

    /// Fill `out` (interleaved frames, `channels`-wide) with resampled
    /// audio, advancing internal state across calls.
    ///
    /// `out.len()` should be a multiple of `channels`; a short trailing
    /// partial frame is filled as far as it goes and otherwise ignored.
    pub fn fill(&mut self, out: &mut [f32]) {
        if !self.primed {
            pull_frame(&self.consumer, &mut self.prev);
            pull_frame(&self.consumer, &mut self.next);
            self.primed = true;
        }

        for frame in out.chunks_mut(self.channels) {
            // `self.frac` is a loop invariant always in `[0.0, 1.0)` (see
            // the `while` below), so narrowing to `f32` here never
            // truncates meaningfully — audio-rate linear interpolation
            // doesn't need `f64`'s extra precision either way.
            #[allow(clippy::cast_possible_truncation)]
            let frac = self.frac as f32;
            for ((sample, &prev), &next) in frame.iter_mut().zip(&self.prev).zip(&self.next) {
                *sample = prev.mul_add(1.0 - frac, next * frac);
            }

            self.frac += self.step;
            while self.frac >= 1.0 {
                self.frac -= 1.0;
                std::mem::swap(&mut self.prev, &mut self.next);
                pull_frame(&self.consumer, &mut self.next);
            }
        }
    }
}

#[cfg(test)]
// Tests compare interpolated sample arrays for exact equality on purpose:
// the expected values below are hand-computed exact results of the linear
// interpolation formula (`prev*(1-frac) + next*frac`) at frac values (0.0,
// 0.5) chosen to land exactly, not tolerances on accumulated math.
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::ring::ring_buffer;

    #[test]
    fn identity_ratio_passes_frames_through_unchanged() {
        // `Resampler` always keeps one frame of lookahead (`prev`/`next`),
        // so priming plus 4 output frames pulls 6 source frames total; push
        // exactly that many so the pass-through values are exercised
        // without also tripping the tail-end underrun that lookahead
        // otherwise causes once the source runs out (production code never
        // hits this: `AudioOutput::open` only builds a `Resampler` when the
        // rates actually differ, never for this identity case).
        let (producer, consumer) = ring_buffer(16);
        assert_eq!(producer.push(&[0.0, 10.0, 20.0, 30.0, 40.0, 50.0]), 6);
        let mut resampler = Resampler::new(consumer, 1, 100, 100);

        let mut out = [0.0; 4];
        resampler.fill(&mut out);
        assert_eq!(out, [0.0, 10.0, 20.0, 30.0]);
        assert_eq!(resampler.underruns(), 0);
    }

    #[test]
    fn upsampling_interpolates_between_frames() {
        // device_rate = 2 * source_rate -> step = 0.5: each source frame
        // pair is stretched into two output frames, the second an exact
        // midpoint.
        let (producer, consumer) = ring_buffer(16);
        assert_eq!(producer.push(&[0.0, 10.0, 20.0]), 3);
        let mut resampler = Resampler::new(consumer, 1, 1, 2);

        let mut out = [0.0; 4];
        resampler.fill(&mut out);
        assert_eq!(out, [0.0, 5.0, 10.0, 15.0]);
        // The 4th output frame's "next" pull ran past the 3 queued samples.
        assert_eq!(resampler.underruns(), 1);
    }

    #[test]
    fn downsampling_skips_source_frames() {
        // device_rate = source_rate / 2 -> step = 2.0: an exact integer
        // step, so frac always returns to exactly 0.0 and every other
        // source frame is emitted verbatim with no interpolation blending.
        let (producer, consumer) = ring_buffer(16);
        assert_eq!(producer.push(&[0.0, 10.0, 20.0, 30.0, 40.0]), 5);
        let mut resampler = Resampler::new(consumer, 1, 2, 1);

        let mut out = [0.0; 2];
        resampler.fill(&mut out);
        assert_eq!(out, [0.0, 20.0]);
        assert_eq!(resampler.underruns(), 1);
    }

    #[test]
    fn stereo_frames_interpolate_each_channel_independently() {
        let (producer, consumer) = ring_buffer(16);
        // Two stereo frames: (0, 100) and (10, 200).
        assert_eq!(producer.push(&[0.0, 100.0, 10.0, 200.0]), 4);
        let mut resampler = Resampler::new(consumer, 2, 1, 2);

        let mut out = [0.0; 4]; // 2 stereo frames
        resampler.fill(&mut out);
        assert_eq!(out, [0.0, 100.0, 5.0, 150.0]);
    }
}
