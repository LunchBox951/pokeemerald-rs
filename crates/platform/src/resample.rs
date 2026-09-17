//! A minimal linear-interpolation resampler: the bridge used whenever
//! the audio device does not support the GBA's nominal mixing rate directly.
//! Since real devices run at 44.1/48 kHz and virtually never advertise the
//! 13379 Hz M4A mixer rate, this is the *common* path, not a rare one (see
//! `crate::audio`).
//!
//! [`Resampler`] drains the source frames one output chunk needs in **one**
//! [`crate::ring::Consumer::fill`] call into a preallocated scratch buffer,
//! then linearly interpolates from that scratch. A callback within the
//! constructor's advertised bound is one such chunk, so it is one bulk drain;
//! an oversized callback (see [`Self::fill`]) is split into several bounded
//! chunks rather than growing scratch, so it is several. Either way `fill`
//! never drains the ring one source frame at a time, so the resampled path
//! is no more contended than the direct path, and underrun accounting
//! (counted inside `Consumer::fill`) is identical whether or not resampling
//! is in play.
//!
//! A source frame is pulled only when an output frame actually consumes it:
//! the per-frame advance runs *before* producing each frame (never as a
//! trailing step after the last one), so the lookahead frame the *next*
//! callback needs is deferred to that callback — when the producer has had
//! time to refill. Pulling it eagerly at end-of-callback would latch a
//! spurious underrun/silence into `next` whenever the producer was momentarily
//! one frame short, a glitch the direct path never produces.

use crate::ring::Consumer;

/// Bound (output frames) for pre-sizing the source scratch buffer: the
/// fallback when the device advertises no concrete callback-size range, and
/// the ceiling [`scratch_layout`] caps a concrete advertised maximum at (a
/// device may advertise an unconstrained range as an enormous concrete
/// number instead of `Unknown`; cpal's ALSA backend reports `u32::MAX` for
/// exactly this). [`Resampler::fill`] already chunks a callback past this
/// scale, so capping the preallocation loses nothing but reserved-but-unused
/// memory.
const DEFAULT_MAX_OUTPUT_FRAMES: usize = 8192;

/// Ceiling on the source-frame scratch buffer itself: scratch scales with
/// `bound * step`, and a device can pair a capped `bound` with an extreme
/// rate ratio (an advertised near-zero device rate against the fixed M4A
/// source rate), which the frame-count cap alone does not bound. Comfortably
/// above the ~13,700 source frames the most extreme realistic device rate
/// (8 kHz) needs at the capped output bound, so no real device's chunking
/// granularity changes.
const MAX_SCRATCH_SOURCE_FRAMES: usize = 32_768;

/// The `(chunk_frames, scratch_frames)` pair [`Resampler::new`] sizes
/// against: `max_output_frames` capped at [`DEFAULT_MAX_OUTPUT_FRAMES`],
/// then re-derived downward if `* step` would still exceed
/// [`MAX_SCRATCH_SOURCE_FRAMES`]. Pure so both caps are unit-testable
/// without allocating the buffer they size.
fn scratch_layout(max_output_frames: usize, step: f64) -> (usize, usize) {
    let bound = if max_output_frames == 0 {
        DEFAULT_MAX_OUTPUT_FRAMES
    } else {
        max_output_frames.min(DEFAULT_MAX_OUTPUT_FRAMES)
    };

    // Re-derive `bound` downward when `bound * step` would still exceed the
    // source-frame ceiling: `bound` alone only caps output frames, and
    // `step` (source_rate / device_rate) is unbounded when a device
    // advertises an extreme rate.
    #[allow(clippy::cast_precision_loss)]
    let source_cap = MAX_SCRATCH_SOURCE_FRAMES as f64;
    #[allow(clippy::cast_precision_loss)]
    let bound_frames = bound as f64;
    let bound = if step > 0.0 && bound_frames * step > source_cap {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let step_limited = (source_cap / step).floor().max(1.0) as usize;
        bound.min(step_limited)
    } else {
        bound
    };

    #[allow(clippy::cast_precision_loss)]
    let crossings = (bound as f64 * step).ceil();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let scratch_frames = 2 + crossings as usize + 1;
    (bound, scratch_frames)
}

/// Linear-interpolation resampler bridging a [`Consumer`]'s nominal sample
/// rate (what the `audio` crate renders at) and an audio device's actual
/// negotiated rate.
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
    /// Preallocated interleaved source-frame scratch, bulk-drained in one
    /// non-blocking call per chunk. Fixed at construction; [`Self::fill`]
    /// splits an oversized `out` into `chunk_frames`-bounded chunks instead
    /// of ever growing this.
    scratch: Vec<f32>,
    /// The most output frames one chunk of [`Self::fill`] may cover without
    /// risking a `scratch` overrun — the same `max_output_frames` bound
    /// `scratch` was sized against.
    chunk_frames: usize,
    primed: bool,
    /// Whether the next output frame should first advance the interpolation
    /// cursor. `false` only for the very first frame ever produced (which sits
    /// exactly on the primed `prev`); `true` forever after. Advancing *before*
    /// each frame — rather than after — is what defers the trailing lookahead
    /// pull to the next callback (see the module docs).
    need_advance: bool,
}

impl Resampler {
    /// Build a resampler pulling `channels`-wide interleaved frames from
    /// `consumer`, nominally produced at `source_rate` Hz, to be emitted at
    /// `device_rate` Hz.
    ///
    /// `max_output_frames` is the device's largest advertised callback size
    /// in frames (`0` if the device advertises none); [`scratch_layout`]
    /// caps it (and the rate ratio it combines with) before it bounds, and
    /// lets the constructor preallocate, the per-chunk source scratch
    /// buffer, and bounds each chunk [`Self::fill`] processes an oversized
    /// callback in, so the real-time `fill` never drains per frame and
    /// never allocates, however large a callback the device hands it.
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

        // Upper bound on the source frames one chunk can consume: two priming
        // frames plus one per source-frame boundary the interpolation cursor
        // crosses over `bound` output frames. `frac` starts below 1, so
        // `ceil(frames * step) + 1` safely covers the crossings with one
        // frame of slack. `Self::fill` never processes more than `bound`
        // output frames per chunk (splitting a larger `out` instead of
        // growing `scratch`), so this bound is never exceeded.
        let (bound, scratch_frames) = scratch_layout(max_output_frames, step);

        Self {
            consumer,
            channels,
            step,
            frac: 0.0,
            prev: vec![0.0; channels],
            next: vec![0.0; channels],
            scratch: vec![0.0; scratch_frames * channels],
            chunk_frames: bound,
            primed: false,
            need_advance: false,
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
    /// A zero-length `out` is a no-op: it does not drain the ring buffer,
    /// prime the interpolator, or count an underrun.
    ///
    /// Splits `out` into `chunk_frames`-bounded chunks (one, for any callback
    /// within the constructor's advertised bound) and drains the ring buffer
    /// exactly once per chunk: all the source frames a chunk needs are
    /// bulk-drained into the fixed `scratch` up front, then the interpolation
    /// loop reads them from scratch without touching the ring again. Chunking
    /// this way — rather than draining `out`'s full demand into a `scratch`
    /// grown to fit — is what keeps `scratch` fixed after construction, so an
    /// oversized callback still never allocates.
    pub fn fill(&mut self, out: &mut [f32]) {
        // A zero-length fill must not move the stream: no priming frames
        // drained, no queue occupancy change, no underrun accounted. Every
        // step below assumes at least one output frame is being produced, so
        // bail before any of it runs rather than special-casing `steps == 0`
        // partway through.
        if out.is_empty() {
            return;
        }

        // Saturating: `chunk_frames` is caller-supplied via `Self::new`'s
        // `max_output_frames` with no documented upper bound. Saturating to
        // `usize::MAX` rather than overflow-panicking just disables chunking
        // for such a pathological bound — `out` (a real caller's buffer)
        // stays far smaller than `usize::MAX` regardless, so it is still
        // processed as one `chunks_mut` chunk, correctly.
        let chunk_samples = self.chunk_frames.saturating_mul(self.channels);
        for chunk in out.chunks_mut(chunk_samples) {
            self.fill_chunk(chunk);
        }
    }

    /// Fill one chunk (`out.len() <= chunk_frames * channels` samples) of
    /// [`Self::fill`]'s output. See [`Self::fill`] and the module docs; the
    /// per-chunk bound is exactly what `scratch` was sized against, so this
    /// never grows it.
    fn fill_chunk(&mut self, out: &mut [f32]) {
        // Number of interpolation steps == number of `chunks_mut` iterations
        // below (a trailing partial frame still advances the cursor).
        let steps = out.len().div_ceil(self.channels);

        // Source frames this call consumes: 2 to prime the `prev`/`next`
        // lookahead on the very first call, plus one per source-frame boundary
        // the interpolation cursor crosses. The advance runs *before* each
        // frame (skipped only for the very first frame ever), so the trailing
        // lookahead pull is deferred to the next callback — see the module
        // docs. Count the crossings with a dry run that mirrors the loop below
        // exactly: same `need_advance` gate, same `frac`/`step` f64 arithmetic
        // in the same order. That parity is what makes the bulk drain match
        // consumption precisely — never relying on `floor` agreeing with
        // incremental accumulation at an exact boundary (a mismatch would drop
        // a source frame or index past the scratch).
        let prime = if self.primed { 0 } else { 2 };
        let mut frac_probe = self.frac;
        let mut advance_probe = self.need_advance;
        let mut crossings = 0usize;
        for _ in 0..steps {
            if advance_probe {
                frac_probe += self.step;
                while frac_probe >= 1.0 {
                    frac_probe -= 1.0;
                    crossings += 1;
                }
            }
            advance_probe = true;
        }
        let source_frames = prime + crossings;
        let needed = source_frames * self.channels;

        // Bulk-drain every needed source frame in ONE non-blocking call, into
        // the fixed `scratch`. `out.len() <= chunk_frames * channels` (see
        // `Self::fill`) is exactly the bound `scratch` was sized against, so
        // `needed` never exceeds it here.
        debug_assert!(
            needed <= self.scratch.len(),
            "a chunk bounded by chunk_frames must never demand more source \
             frames than scratch was sized for"
        );
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
            // Advance to this frame's position first, pulling the source
            // frame(s) it needs. Skipped only for the first frame ever, which
            // sits exactly on the primed `prev`/`next` pair at `frac == 0`.
            if self.need_advance {
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
            self.need_advance = true;

            // `self.frac` is a loop invariant always in `[0.0, 1.0)` (see
            // the `while` above), so narrowing to `f32` here never
            // truncates meaningfully — audio-rate linear interpolation
            // doesn't need `f64`'s extra precision either way.
            #[allow(clippy::cast_possible_truncation)]
            let frac = self.frac as f32;
            for ((sample, &prev), &next) in frame.iter_mut().zip(&self.prev).zip(&self.next) {
                *sample = prev.mul_add(1.0 - frac, next * frac);
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
    use crate::ring::{ring_buffer, Producer};

    #[test]
    fn identity_ratio_passes_frames_through_unchanged() {
        // With the deferred lookahead pull, priming plus 4 output frames pulls
        // only 5 source frames within this call (the 6th is deferred to a next
        // callback that never comes here); push a comfortable margin so the
        // pass-through values are exercised without a tail-end underrun
        // (production code never hits this identity case: `AudioOutput::open`
        // only builds a `Resampler` when the rates actually differ).
        let (producer, consumer) = ring_buffer(16);
        assert_eq!(producer.push(&[0.0, 10.0, 20.0, 30.0, 40.0, 50.0]), 6);
        let mut resampler = Resampler::new(consumer, 1, 100, 100, 16);

        let mut out = [0.0; 4];
        resampler.fill(&mut out);
        assert_eq!(out, [0.0, 10.0, 20.0, 30.0]);
        assert_eq!(resampler.underruns(), 0);
    }

    #[test]
    fn a_callback_larger_than_the_advertised_bound_is_chunked_not_grown() {
        // `max_output_frames = 4` bounds every chunk `fill` processes
        // internally to 4 output frames, so a single 20-frame call must
        // split into 5 such chunks. That must produce exactly what 5
        // separate 4-frame calls would (state carries across a chunk
        // boundary exactly as it carries across a callback boundary), and
        // must never grow `scratch` doing it — the oversized-callback path
        // regressed by this fix.
        #[expect(
            clippy::cast_precision_loss,
            reason = "i is 0..30, exactly representable in f32"
        )]
        let source: Vec<f32> = (0..30_i32).map(|i| i as f32).collect();

        let (producer_a, consumer_a) = ring_buffer(64);
        assert_eq!(producer_a.push(&source), 30);
        let mut oversized = Resampler::new(consumer_a, 1, 1, 1, 4);
        let scratch_capacity = oversized.scratch.len();

        let mut big_out = [0.0; 20];
        oversized.fill(&mut big_out);
        assert_eq!(
            oversized.scratch.len(),
            scratch_capacity,
            "an oversized single call must never grow scratch"
        );

        let (producer_b, consumer_b) = ring_buffer(64);
        assert_eq!(producer_b.push(&source), 30);
        let mut chunked = Resampler::new(consumer_b, 1, 1, 1, 4);
        let mut small_out = [0.0; 20];
        for chunk in small_out.chunks_mut(4) {
            chunked.fill(chunk);
        }

        assert_eq!(big_out, small_out);
        assert_eq!(oversized.underruns(), chunked.underruns());
    }

    #[test]
    fn an_extreme_advertised_bound_is_capped_at_the_realistic_callback_scale() {
        // `scratch_layout` is what `Resampler::new` actually preallocates
        // against (see its doc for why an advertised maximum needs this);
        // pin the cap without allocating the buffer an uncapped `u32::MAX`
        // bound would imply.
        let huge = usize::try_from(u32::MAX).unwrap();
        assert_eq!(scratch_layout(huge, 1.0).0, DEFAULT_MAX_OUTPUT_FRAMES);
        assert_eq!(scratch_layout(0, 1.0).0, DEFAULT_MAX_OUTPUT_FRAMES);
        assert_eq!(scratch_layout(4, 1.0).0, 4);
    }

    #[test]
    fn an_extreme_rate_ratio_is_capped_independently_of_the_frame_bound() {
        // A device can pair an already-capped `max_output_frames` with an
        // extreme rate ratio (see `MAX_SCRATCH_SOURCE_FRAMES`'s docs), which
        // `DEFAULT_MAX_OUTPUT_FRAMES` alone does not bound: pin that
        // `scratch_layout` shrinks `bound` further rather than letting
        // `bound * step` size scratch unbounded.
        let step = 13_379.0; // source_rate=13379 Hz over device_rate=1 Hz
        let (bound, scratch_frames) = scratch_layout(DEFAULT_MAX_OUTPUT_FRAMES, step);
        assert!(
            bound < DEFAULT_MAX_OUTPUT_FRAMES,
            "an extreme rate ratio must shrink the output-frame bound: got {bound}"
        );
        assert!(
            scratch_frames <= MAX_SCRATCH_SOURCE_FRAMES + 4,
            "scratch_frames must stay at MAX_SCRATCH_SOURCE_FRAMES's scale: got {scratch_frames}"
        );
    }

    #[test]
    fn constructing_with_an_extreme_bound_and_rate_ratio_stays_cheap() {
        // Both caps compose: an advertised `u32::MAX` buffer maximum and an
        // advertised near-zero device rate (13379/1 -- cpal's ALSA backend
        // can report either) must not multiply into an unbounded
        // allocation.
        let huge_bound = usize::try_from(u32::MAX).unwrap();
        let (_producer, consumer) = ring_buffer(16);
        let resampler = Resampler::new(consumer, 2, 13_379, 1, huge_bound);
        assert!(resampler.scratch.len() <= (MAX_SCRATCH_SOURCE_FRAMES + 4) * 2);
    }

    #[test]
    fn a_bound_above_the_cap_still_chunks_an_oversized_fill_correctly() {
        // A bound comfortably above `DEFAULT_MAX_OUTPUT_FRAMES` must still
        // have its preallocation capped (`chunk_frames ==
        // DEFAULT_MAX_OUTPUT_FRAMES`, not the raw advertised value), and an
        // oversized fill against it must still chunk into exactly the
        // output an equivalent in-cap bound produces -- the cap changes
        // chunking granularity, never correctness.
        let above_cap = DEFAULT_MAX_OUTPUT_FRAMES * 4;

        #[expect(
            clippy::cast_precision_loss,
            reason = "i is 0..30, exactly representable in f32"
        )]
        let source: Vec<f32> = (0..30_i32).map(|i| i as f32).collect();

        let (producer_a, consumer_a) = ring_buffer(64);
        assert_eq!(producer_a.push(&source), 30);
        let mut capped = Resampler::new(consumer_a, 1, 1, 1, above_cap);
        assert_eq!(capped.chunk_frames, DEFAULT_MAX_OUTPUT_FRAMES);

        let mut big_out = [0.0; 20];
        capped.fill(&mut big_out);

        let (producer_b, consumer_b) = ring_buffer(64);
        assert_eq!(producer_b.push(&source), 30);
        let mut reference = Resampler::new(consumer_b, 1, 1, 1, 4);
        let mut small_out = [0.0; 20];
        for chunk in small_out.chunks_mut(4) {
            reference.fill(chunk);
        }

        assert_eq!(big_out, small_out);
        assert_eq!(capped.underruns(), reference.underruns());
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
        // No underrun: this call consumes exactly the 3 queued frames; the
        // lookahead pull that would run past them is deferred to the next
        // callback (see the module docs).
        assert_eq!(resampler.underruns(), 0);
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
        // No underrun: 2 output frames consume 4 of the 5 queued source frames
        // (the deferred lookahead frame would be the 5th).
        assert_eq!(resampler.underruns(), 0);
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

    #[test]
    fn deferred_pull_avoids_glitch_when_producer_catches_up_between_callbacks() {
        // step = 0.5. Callback 1 is given exactly the source frames it needs
        // (0, 10); the eager-pull code would have pulled a 3rd frame past the
        // queue end, recording an underrun and latching silence into `next`.
        // With the pull deferred, callback 1 underruns 0 and leaves `next`
        // pointing at real data. The producer then supplies the next frame
        // (20) *before* callback 2, which must interpolate 10 -> 20 cleanly
        // (no silence blend) with underruns still 0.
        let (producer, consumer) = ring_buffer(16);
        assert_eq!(producer.push(&[0.0, 10.0]), 2);
        let mut resampler = Resampler::new(consumer, 1, 1, 2, 8);

        let mut cb1 = [0.0; 2];
        resampler.fill(&mut cb1);
        assert_eq!(cb1, [0.0, 5.0]);
        assert_eq!(resampler.underruns(), 0);

        // Producer catches up between callbacks.
        assert_eq!(producer.push(&[20.0]), 1);

        let mut cb2 = [0.0; 2];
        resampler.fill(&mut cb2);
        // Interpolates against the real frame 20, not stale silence.
        assert_eq!(cb2, [10.0, 15.0]);
        assert_eq!(resampler.underruns(), 0);
    }

    #[test]
    fn empty_fill_before_priming_does_not_touch_queue_or_prime() {
        // Regression: on a never-primed resampler, a `fill` that computed
        // its 2-frame priming drain from `self.primed` alone would run that
        // drain for an empty `out` too. The queue here is deliberately
        // starved to 1 sample (fewer than the 2 priming frames such a drain
        // would take), so both symptoms are directly observable:
        // `Consumer::fill`'s shortfall padding would count 1 underrun on a
        // call that produced no output, and the queue would lose its 1
        // queued sample even though nothing was emitted.
        let (producer, consumer) = ring_buffer(16);
        assert_eq!(producer.push(&[7.0]), 1);
        let mut resampler = Resampler::new(consumer, 1, 100, 100, 16);

        resampler.fill(&mut []);
        assert_eq!(producer.available_space(), 16 - 1);
        assert_eq!(resampler.underruns(), 0);
        assert!(!resampler.primed);

        // Such a drain would also have latched `primed = true` with
        // `prev = 7.0` (the one real sample it grabbed) and `next = 0.0`
        // (silence, padded for the shortfall) — corrupting the interpolator
        // before any real output existed. Queue the rest of the stream and
        // run a real fill: with the empty fill correctly a no-op, priming
        // still happens here, against the real, un-poisoned data.
        assert_eq!(producer.push(&[8.0, 9.0, 10.0]), 3);
        let mut out = [0.0; 2];
        resampler.fill(&mut out);
        assert_eq!(out, [7.0, 8.0]);
        assert_eq!(resampler.underruns(), 0);
    }

    #[test]
    fn empty_fill_after_priming_leaves_interpolator_state_untouched() {
        // The post-priming counterpart to
        // `empty_fill_before_priming_does_not_touch_queue_or_prime`: it pins
        // the `out.is_empty()` guard's contract — no queue/underrun/cursor
        // change — for the primed path as well, so a change to this method
        // can't introduce a cost or side effect here without failing a
        // test. Two identically-seeded resamplers are driven through the
        // same real fills; only one gets an empty fill spliced in between,
        // and every field plus the next real fill's output must still match
        // the one that never saw an empty fill.
        fn make() -> (Producer, Resampler) {
            let (producer, consumer) = ring_buffer(16);
            assert_eq!(producer.push(&[0.0, 10.0]), 2);
            (producer, Resampler::new(consumer, 1, 1, 2, 8))
        }

        let (producer_a, mut baseline) = make();
        let (producer_b, mut with_empty) = make();

        let mut first_a = [0.0; 2];
        let mut first_b = [0.0; 2];
        baseline.fill(&mut first_a);
        with_empty.fill(&mut first_b);
        assert_eq!(first_a, first_b);
        assert!(baseline.primed);
        // Both queues are fully drained by the first fill (2 pushed, 2
        // consumed for priming, 0 crossings) -- an absolute checkpoint, not
        // just a relative one, before the empty fill is spliced in.
        assert_eq!(producer_a.available_space(), 16);
        assert_eq!(producer_b.available_space(), 16);

        // Only `with_empty` gets the empty fill.
        with_empty.fill(&mut []);
        assert_eq!(producer_b.available_space(), 16);
        assert_eq!(producer_a.available_space(), producer_b.available_space());
        assert_eq!(baseline.underruns(), with_empty.underruns());
        assert_eq!(baseline.primed, with_empty.primed);
        assert_eq!(baseline.need_advance, with_empty.need_advance);
        assert_eq!(baseline.frac, with_empty.frac);
        assert_eq!(baseline.prev, with_empty.prev);
        assert_eq!(baseline.next, with_empty.next);

        // Feed both queues identically, then confirm the next real fill
        // still matches: proof the spliced-in empty fill changed nothing
        // that later output depends on.
        assert_eq!(producer_a.push(&[20.0]), 1);
        assert_eq!(producer_b.push(&[20.0]), 1);

        let mut second_a = [0.0; 2];
        let mut second_b = [0.0; 2];
        baseline.fill(&mut second_a);
        with_empty.fill(&mut second_b);
        assert_eq!(second_a, [10.0, 15.0]);
        assert_eq!(second_a, second_b);
        assert_eq!(baseline.underruns(), with_empty.underruns());
    }
}
