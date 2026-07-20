//! A minimal linear-interpolation resampler (S-1): the bridge used whenever
//! the audio device does not support the GBA's nominal mixing rate directly.
//! Since real devices run at 44.1/48 kHz and virtually never advertise the
//! 13379 Hz M4A mixer rate, this is the *common* path, not a rare one (see
//! `crate::audio`).
//!
//! [`Resampler`] drains all the source frames a callback needs in **one**
//! [`crate::ring::Consumer::fill`] call into a preallocated scratch buffer,
//! then linearly interpolates from that scratch. That honours `crate::ring`'s
//! invariant — one queue-lock acquisition per callback, never one per source
//! frame — so the resampled path locks no more often than the direct path,
//! and underrun accounting (counted inside `Consumer::fill`) is identical
//! whether or not resampling is in play.

use crate::ring::Consumer;

/// Fallback bound (output frames) for pre-sizing the source scratch buffer
/// when the device advertises no concrete callback-size range. Generously
/// larger than any realistic callback so the constructor's preallocation
/// still spares the real-time thread an allocation.
const DEFAULT_MAX_OUTPUT_FRAMES: usize = 8192;

/// Linear-interpolation resampler bridging a [`Consumer`]'s nominal sample
/// rate (what the future `audio` crate renders at) and an audio device's
/// actual negotiated rate.
///
/// Not a general-purpose DSP resampler — linear interpolation is cheap and
/// good enough for bridging the sample-rate mismatch; it is explicitly out of
/// scope to do anything fancier (band-limited interpolation, etc) per the
/// `cpal` dependency's approved scope (no decoding, no effects).
pub struct Resampler {
    consumer: Consumer,
    channels: usize,
    /// Source frames advanced per output frame (`source_rate / device_rate`).
    step: f64,
    /// Fractional position between `prev` and `next`, in `[0.0, 1.0)`.
    frac: f64,
    prev: Vec<f32>,
    next: Vec<f32>,
    /// Preallocated interleaved source-frame scratch, bulk-drained under one
    /// lock per callback (grown off the hot path only if a callback is larger
    /// than the constructor's estimate).
    scratch: Vec<f32>,
    primed: bool,
}

impl Resampler {
    /// Build a resampler pulling `channels`-wide interleaved frames from
    /// `consumer`, nominally produced at `source_rate` Hz, to be emitted at
    /// `device_rate` Hz.
    ///
    /// `max_output_frames` is the device's largest advertised callback size
    /// in frames (`0` if the device advertises none); it bounds — and lets
    /// the constructor preallocate — the per-callback source scratch buffer
    /// so the real-time [`Self::fill`] never locks per frame and, in steady
    /// state, never allocates.
    #[must_use]
    pub fn new(
        consumer: Consumer,
        channels: u16,
        source_rate: u32,
        device_rate: u32,
        max_output_frames: usize,
    ) -> Self {
        let channels = usize::from(channels.max(1));
        let step = f64::from(source_rate) / f64::from(device_rate.max(1));

        // Upper bound on the source frames one callback can consume: two
        // priming frames plus one per source-frame boundary the interpolation
        // cursor crosses over `max_output_frames` output frames. `frac` starts
        // below 1, so `ceil(frames * step) + 1` safely covers the crossings;
        // `Self::fill` still resizes as a last resort for a larger callback.
        let bound = if max_output_frames == 0 {
            DEFAULT_MAX_OUTPUT_FRAMES
        } else {
            max_output_frames
        };
        #[allow(clippy::cast_precision_loss)]
        let crossings = (bound as f64 * step).ceil();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let scratch_frames = 2 + crossings as usize + 1;

        Self {
            consumer,
            channels,
            step,
            frac: 0.0,
            prev: vec![0.0; channels],
            next: vec![0.0; channels],
            scratch: vec![0.0; scratch_frames * channels],
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
    ///
    /// Locks the ring buffer exactly once: all the source frames this call
    /// needs are bulk-drained up front (see the module docs), then the
    /// interpolation loop reads them from scratch without touching the queue.
    pub fn fill(&mut self, out: &mut [f32]) {
        // Number of interpolation steps == number of `chunks_mut` iterations
        // below (a trailing partial frame still advances the cursor).
        let steps = out.len().div_ceil(self.channels);

        // Source frames this call consumes: 2 to prime the `prev`/`next`
        // lookahead on the first call, plus one per source-frame boundary the
        // interpolation cursor crosses. Count the crossings with a dry run
        // using the *identical* f64 arithmetic the loop below performs, so the
        // bulk drain matches consumption exactly — never relying on `floor`
        // agreeing with incremental accumulation at an exact boundary (a
        // mismatch would drop a source frame or index past the scratch).
        let prime = if self.primed { 0 } else { 2 };
        let mut frac_probe = self.frac;
        let mut crossings = 0usize;
        for _ in 0..steps {
            frac_probe += self.step;
            while frac_probe >= 1.0 {
                frac_probe -= 1.0;
                crossings += 1;
            }
        }
        let source_frames = prime + crossings;
        let needed = source_frames * self.channels;

        // Bulk-drain every needed source frame under ONE lock. Grow only here,
        // off the per-frame hot loop, for a larger-than-estimated callback.
        if self.scratch.len() < needed {
            self.scratch.resize(needed, 0.0);
        }
        self.consumer.fill(&mut self.scratch[..needed]);

        // Cursor into the drained scratch, in frames.
        let mut src = 0;
        if !self.primed {
            self.prev.copy_from_slice(&self.scratch[..self.channels]);
            self.next
                .copy_from_slice(&self.scratch[self.channels..2 * self.channels]);
            src = 2;
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
                let base = src * self.channels;
                self.next
                    .copy_from_slice(&self.scratch[base..base + self.channels]);
                src += 1;
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
        let mut resampler = Resampler::new(consumer, 1, 100, 100, 16);

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
        let mut resampler = Resampler::new(consumer, 1, 1, 2, 16);

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
        let mut resampler = Resampler::new(consumer, 1, 2, 1, 16);

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
        let mut resampler = Resampler::new(consumer, 2, 1, 2, 16);

        let mut out = [0.0; 4]; // 2 stereo frames
        resampler.fill(&mut out);
        assert_eq!(out, [0.0, 100.0, 5.0, 150.0]);
    }
}
