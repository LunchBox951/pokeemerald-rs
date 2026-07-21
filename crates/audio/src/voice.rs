//! A single playing DirectSound voice: a wave read at a pitch-scaled rate,
//! shaped by an [`Envelope`], and panned into left/right accumulators.
//!
//! Behavioural port of one `struct SoundChannel` (`m4a_internal.h:130`) as
//! rendered by `SoundMainRAM`'s interpolating inner loop (`m4a_1.s:396`..
//! `:437`). The hardware packs four `s8` output samples per word with a rotate
//! trick and lets adjacent lanes carry into one another; here each output
//! sample accumulates independently into an `i32` `(no-verbatim)` — the
//! packed cross-lane carry bleed is a deferred hardware quirk. Fixed-rate
//! (`TONEDATA_TYPE_FIX`) and compressed voices are also out of scope for this
//! slice; every voice interpolates.

use std::sync::Arc;

use crate::envelope::{Adsr, Envelope};
use crate::pitch::{self, FRAC_MASK};
use crate::sample::WaveData;

/// One stereo output sample as signed pre-clip accumulators (`left`, `right`).
pub type StereoAcc = (i32, i32);

/// A live voice owned by the mixer.
#[derive(Clone, Debug)]
pub struct Voice {
    wave: Arc<WaveData>,
    envelope: Envelope,
    /// Base right/left channel volume from `ChnVolSetAsm` (`m4a_1.s:1508`),
    /// before the envelope scales it per frame.
    base_right: u8,
    base_left: u8,
    /// The note's velocity (`SoundChannel::velocity`), retained so a mid-note
    /// track volume change can re-run `ChnVolSetAsm` against this channel.
    velocity: u8,
    /// Per-frame envelope-scaled right/left gain (`envelopeVolumeRight/Left`).
    env_right: i32,
    env_left: i32,
    /// Voice frequency (`MidiKeyToFreq` output); scaled by `DIV_FREQ` per
    /// output sample to get the `.23` phase step.
    frequency: u32,
    /// Integer source-sample read index.
    index: usize,
    /// `.23` fractional read position within the current source sample.
    frac: u32,
    /// Note-off countdown in sequencer ticks; `0` means tied (never
    /// auto-releases).
    gate_time: u16,
    /// The track command's MIDI key, for tie/end-of-tie matching.
    midi_key: u8,
    /// Owning track index, so per-track note-off can find its voices.
    track: usize,
}

impl Voice {
    /// Start a voice for `wave` at `frequency`, with base stereo volume
    /// `(right, left)` and an ADSR envelope. `velocity` is the note's velocity
    /// (kept for mid-note volume re-runs). `gate_time` is the note-off delay in
    /// ticks (`0` = tied). `echo_volume`/`echo_length` drive the pseudo-echo
    /// tail (both `0` for a plain note).
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        wave: Arc<WaveData>,
        adsr: Adsr,
        frequency: u32,
        right: u8,
        left: u8,
        velocity: u8,
        gate_time: u16,
        midi_key: u8,
        track: usize,
        echo_volume: u8,
        echo_length: u8,
    ) -> Self {
        Self {
            wave,
            envelope: Envelope::new(adsr, echo_volume, echo_length),
            base_right: right,
            base_left: left,
            velocity,
            env_right: 0,
            env_left: 0,
            frequency,
            index: 0,
            frac: 0,
            gate_time,
            midi_key,
            track,
        }
    }

    /// Whether the voice is still producing sound.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.envelope.is_active()
    }

    /// The owning track index.
    #[must_use]
    pub fn track(&self) -> usize {
        self.track
    }

    /// The MIDI key this voice was started on.
    #[must_use]
    pub fn midi_key(&self) -> u8 {
        self.midi_key
    }

    /// The current integer source-sample read index (for inspection/testing).
    #[must_use]
    pub fn source_index(&self) -> usize {
        self.index
    }

    /// Whether the note has already been released.
    #[must_use]
    pub fn is_stopping(&self) -> bool {
        self.envelope.is_stopping()
    }

    /// Tick the note-off gate down by one sequencer tick, releasing the
    /// envelope when a non-tied gate reaches zero (`m4a_1.s:1196`).
    pub fn tick_gate(&mut self) {
        if self.gate_time > 0 {
            self.gate_time -= 1;
            if self.gate_time == 0 {
                self.envelope.note_off();
            }
        }
    }

    /// Force note-off (tie termination / explicit stop).
    pub fn note_off(&mut self) {
        self.envelope.note_off();
    }

    /// Re-run `ChnVolSetAsm` (`m4a_1.s:1508`) against this live channel from
    /// updated track `volMR`/`volML`, folding in the channel's own retained
    /// velocity. Applied when the owning track's volume/pan changes mid-note
    /// (`MPT_FLG_VOLCHG`).
    pub fn set_track_volume(&mut self, vol_mr: u8, vol_ml: u8) {
        self.base_right = channel_volume(vol_mr, 0x80, self.velocity);
        self.base_left = channel_volume(vol_ml, 0x7F, self.velocity);
    }

    /// Recompute this live channel's playback frequency from updated track
    /// `keyM`/`pitM`, mirroring `MidiKeyToFreq(wav, (key + keyM).max(0), pitM)`
    /// in `MPlayMain`'s per-tick pitch re-run (`m4a_1.s:1446`). Uses the
    /// channel's stored key exactly as [`note_on`](crate::sequencer) did for the
    /// initial pitch, so a mid-note bend tracks the same math.
    pub fn set_track_pitch(&mut self, key_m: i32, pit_m: u8) {
        let note_key = u8::try_from((i32::from(self.midi_key) + key_m).max(0) & 0xFF).unwrap_or(0);
        self.frequency = pitch::midi_key_to_freq(self.wave.freq(), note_key, pit_m);
    }

    /// The current effective playback frequency (for inspection/testing).
    #[must_use]
    pub fn frequency(&self) -> u32 {
        self.frequency
    }

    /// The current base right/left channel volumes (for inspection/testing).
    #[must_use]
    pub fn base_volume(&self) -> (u8, u8) {
        (self.base_right, self.base_left)
    }

    /// Advance the envelope one frame and recompute the frame's stereo gains.
    ///
    /// `master_volume` is the global `0..=15` mix level; upstream forms the
    /// effective volume as `((masterVolume + 1) * envelopeVolume) >> 4`
    /// (`m4a_1.s:265`) then scales the base channel volumes by it.
    pub fn begin_frame(&mut self, master_volume: u8) {
        self.envelope.step();
        let effective = ((u32::from(master_volume) + 1) * u32::from(self.envelope.volume())) >> 4;
        self.env_right =
            i32::try_from((u32::from(self.base_right) * effective) >> 8).unwrap_or(i32::MAX);
        self.env_left =
            i32::try_from((u32::from(self.base_left) * effective) >> 8).unwrap_or(i32::MAX);
    }

    /// Render this voice's contribution across a frame, accumulating into
    /// `acc` (one entry per output sample). [`Self::begin_frame`] must have run
    /// for this frame first.
    pub fn render(&mut self, acc: &mut [StereoAcc]) {
        let step = pitch::phase_step(self.frequency);
        // Cheap Arc clone so the immutable sample borrow does not clash with
        // the `&mut self` cursor/envelope updates below.
        let wave = Arc::clone(&self.wave);
        let samples = wave.samples();
        let len = samples.len();
        if len == 0 {
            self.envelope.note_off();
            return;
        }

        for slot in acc.iter_mut() {
            if !self.envelope.is_active() {
                break;
            }
            if !self.wrap_or_end(&wave, len) {
                break;
            }

            let s0 = i32::from(samples[self.index]);
            let s1 = i32::from(self.next_sample(&wave, len));
            // `s0 + ((frac * (s1 - s0)) >> 23)` with an arithmetic shift
            // (`add lr, r0, lr, asr 23`). The shifted term is bounded by the
            // inter-sample delta (`< 256`), so it always fits an `i32`.
            let frac_delta = (i64::from(self.frac) * i64::from(s1 - s0)) >> pitch::FRAC_BITS;
            let interp = s0 + i32::try_from(frac_delta).unwrap_or(0);

            slot.1 += (self.env_right * interp) >> 8;
            slot.0 += (self.env_left * interp) >> 8;

            // The hardware phase accumulator wraps mod 2^32 (`add r9,r9,r4`);
            // `step` (a `wrapping_mul` result) can land near `u32::MAX`, so a
            // checked add would overflow-panic in debug builds.
            self.frac = self.frac.wrapping_add(step);
            self.index += usize::try_from(self.frac >> pitch::FRAC_BITS).unwrap_or(0);
            self.frac &= FRAC_MASK;
        }
    }

    /// Keep `index` in range: wrap looping voices back into their loop region,
    /// retire one-shot voices past their end. Returns `false` if the voice has
    /// ended and rendering should stop.
    fn wrap_or_end(&mut self, wave: &WaveData, len: usize) -> bool {
        if self.index < len {
            return true;
        }
        if wave.is_looping() {
            let loop_start = wave.loop_start().min(len - 1);
            let loop_len = len - loop_start;
            self.index = loop_start + (self.index - loop_start) % loop_len;
            true
        } else {
            self.envelope.retire();
            false
        }
    }

    /// The sample following `index`, honouring loop wrap-around at the end.
    fn next_sample(&self, wave: &WaveData, len: usize) -> i8 {
        let samples = wave.samples();
        let next = self.index + 1;
        if next < len {
            samples[next]
        } else if wave.is_looping() {
            samples[wave.loop_start().min(len - 1)]
        } else {
            samples[self.index]
        }
    }
}

/// `ChnVolSetAsm`: fold a per-side base volume with the pan term and velocity,
/// clamped to `255` (`m4a_1.s:1508`). `pan_term` is `0x80` for the right
/// channel, `0x7F` for the left (rhythm pan is `0` for this slice). Shared by
/// the sequencer's note-on path and [`Voice::set_track_volume`].
#[must_use]
pub(crate) fn channel_volume(vol_side: u8, pan_term: u32, velocity: u8) -> u8 {
    let scaled = (u32::from(vol_side) * (pan_term * u32::from(velocity))) >> 14;
    u8::try_from(scaled.min(255)).unwrap_or(255)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wave(freq: u32, data: Vec<i8>) -> Arc<WaveData> {
        Arc::new(WaveData::one_shot(freq, data))
    }

    /// A voice whose phase step is exactly one source sample per output sample.
    /// A step of ~1 source sample per output sample (not exactly integral,
    /// because `DIV_FREQ` does not divide `2^23`).
    fn unity_freq() -> u32 {
        (1 << pitch::FRAC_BITS) / pitch::DIV_FREQ
    }

    #[test]
    fn flat_envelope_applies_gain_exactly_on_a_constant_wave() {
        // A constant wave removes interpolation, isolating the gain path:
        // eff = 16*0xFF>>4 = 0xFF, env = 0xFF*0xFF>>8 = 254, contribution
        // = 254*50>>8, on every output sample.
        let w = wave(0, vec![50; 16]);
        let mut v = Voice::new(
            w,
            Adsr::flat(),
            unity_freq(),
            0xFF,
            0xFF,
            127,
            0,
            60,
            0,
            0,
            0,
        );
        let mut acc = vec![(0, 0); 8];
        v.begin_frame(15);
        v.render(&mut acc);
        let expected = (254 * 50) >> 8;
        for slot in &acc {
            assert_eq!(*slot, (expected, expected));
        }
    }

    #[test]
    fn frac0_first_output_is_the_raw_first_sample() {
        // At frac 0 there is no interpolation, so output 0 is exact.
        let w = wave(0, vec![-100, 0, 0, 0]);
        let mut v = Voice::new(
            w,
            Adsr::flat(),
            unity_freq(),
            0xFF,
            0xFF,
            127,
            0,
            60,
            0,
            0,
            0,
        );
        let mut acc = vec![(0, 0); 1];
        v.begin_frame(15);
        v.render(&mut acc);
        assert_eq!(acc[0].0, (254 * -100) >> 8);
    }

    #[test]
    fn faster_frequency_advances_further_through_the_wave() {
        let w = wave(0, vec![0; 512]);
        let mut slow = Voice::new(
            w.clone(),
            Adsr::flat(),
            unity_freq(),
            0xFF,
            0xFF,
            127,
            0,
            60,
            0,
            0,
            0,
        );
        let mut fast = Voice::new(
            w,
            Adsr::flat(),
            unity_freq() * 2,
            0xFF,
            0xFF,
            127,
            0,
            60,
            0,
            0,
            0,
        );
        let mut acc = vec![(0, 0); 100];
        slow.begin_frame(15);
        slow.render(&mut acc);
        fast.begin_frame(15);
        fast.render(&mut acc);
        // ~100 vs ~200 source samples consumed.
        assert!((95..=105).contains(&slow.source_index()));
        assert!((195..=205).contains(&fast.source_index()));
    }

    #[test]
    fn half_frequency_interpolates_between_samples() {
        // Half the unity step -> a fractional midpoint between samples.
        let w = wave(0, vec![0, 100, 0, 0]);
        let mut v = Voice::new(
            w,
            Adsr::flat(),
            unity_freq() / 2,
            0xFF,
            0xFF,
            127,
            0,
            60,
            0,
            0,
            0,
        );
        let mut acc = vec![(0, 0); 2];
        v.begin_frame(15);
        v.render(&mut acc);
        // First output at frac 0 -> sample[0] = 0.
        assert_eq!(acc[0].0, 0);
        // Second output at frac 0.5 between 0 and 100 -> ~50.
        let interp = acc[1].0 * 256 / 254;
        assert!((48..=52).contains(&interp), "midpoint interp was {interp}");
    }

    #[test]
    fn one_shot_voice_retires_at_end_of_wave() {
        let w = wave(0, vec![10, 20]);
        let mut v = Voice::new(
            w,
            Adsr::flat(),
            unity_freq(),
            0xFF,
            0xFF,
            127,
            0,
            60,
            0,
            0,
            0,
        );
        let mut acc = vec![(0, 0); 8];
        v.begin_frame(15);
        v.render(&mut acc);
        assert!(!v.is_active());
        // First output (frac 0) is the raw first sample; once the two-sample
        // wave is exhausted the voice retires and later slots stay silent.
        assert_eq!(acc[0].0, (254 * 10) >> 8);
        assert_eq!(acc[7], (0, 0));
    }

    #[test]
    fn looping_voice_wraps_and_keeps_playing() {
        // A single-sample loop region (`5`) so wrap-around output is exact
        // regardless of the fractional phase.
        let w = Arc::new(WaveData::looping(0, 1, vec![99, 5]));
        let mut v = Voice::new(
            w,
            Adsr::flat(),
            unity_freq(),
            0xFF,
            0xFF,
            127,
            0,
            60,
            0,
            0,
            0,
        );
        let mut acc = vec![(0, 0); 32];
        v.begin_frame(15);
        v.render(&mut acc);
        // First output (frac 0) is sample 0; the tail settles on the looped 5.
        assert_eq!(acc[0].0, (254 * 99) >> 8);
        assert_eq!(acc[31].0, (254 * 5) >> 8);
        // A looping voice never retires on its own.
        assert!(v.is_active());
    }

    #[test]
    fn panned_voice_splits_left_and_right() {
        let w = wave(0, vec![100, 0, 0, 0]);
        // Hard-right base volume: right 0xFF, left 0.
        let mut v = Voice::new(w, Adsr::flat(), unity_freq(), 0xFF, 0, 127, 0, 60, 0, 0, 0);
        let mut acc = vec![(0, 0); 1];
        v.begin_frame(15);
        v.render(&mut acc);
        assert_eq!(acc[0].0, 0); // left silent
        assert_eq!(acc[0].1, (254 * 100) >> 8); // right full
    }

    #[test]
    fn near_max_phase_step_wraps_without_panicking() {
        // `frequency == u32::MAX` makes `phase_step = 627 * u32::MAX (mod 2^32)`
        // land in the top of the `u32` range, so the phase accumulator crosses
        // `2^32` within a couple of output samples. The hardware `add r9,r9,r4`
        // wraps; a checked `frac += step` would overflow-panic in debug. A
        // looping wave keeps the voice alive across the whole render.
        let w = Arc::new(WaveData::looping(0, 0, vec![10, -10, 10, -10]));
        let mut v = Voice::new(w, Adsr::flat(), u32::MAX, 0xFF, 0xFF, 127, 0, 60, 0, 0, 0);
        let mut acc = vec![(0, 0); 8];
        v.begin_frame(15);
        v.render(&mut acc); // must not panic
        assert!(v.is_active());
    }

    #[test]
    fn set_track_volume_rewrites_base_from_velocity() {
        // Re-running `ChnVolSetAsm` against a live channel with new track
        // volMR/volML changes the base volumes, folding in the stored velocity.
        let w = wave(0, vec![50; 16]);
        let mut v = Voice::new(w, Adsr::flat(), unity_freq(), 0, 0, 127, 0, 60, 0, 0, 0);
        v.set_track_volume(0x40, 0x40);
        let (right, left) = v.base_volume();
        assert_eq!(right, channel_volume(0x40, 0x80, 127));
        assert_eq!(left, channel_volume(0x40, 0x7F, 127));
        assert!(right > 0 && left > 0);
    }

    #[test]
    fn set_track_pitch_recomputes_frequency_from_stored_key() {
        // A positive key/fine offset raises the played frequency, matching a
        // fresh `MidiKeyToFreq(wav, key + keyM, pitM)` on the channel's key.
        let w = wave(1 << 20, vec![50; 16]);
        let mut v = Voice::new(
            w,
            Adsr::flat(),
            unity_freq(),
            0xFF,
            0xFF,
            127,
            0,
            60,
            0,
            0,
            0,
        );
        v.set_track_pitch(0, 0);
        assert_eq!(v.frequency(), pitch::midi_key_to_freq(1 << 20, 60, 0));
        v.set_track_pitch(12, 0); // one octave up doubles the frequency
        assert_eq!(v.frequency(), pitch::midi_key_to_freq(1 << 20, 72, 0));
    }

    #[test]
    fn gate_expiry_releases_the_envelope() {
        let w = wave(0, vec![50, 50, 50, 50]);
        let mut v = Voice::new(
            w,
            Adsr::flat(),
            unity_freq(),
            0xFF,
            0xFF,
            127,
            2,
            60,
            0,
            0,
            0,
        );
        assert!(!v.is_stopping());
        v.tick_gate();
        assert!(!v.is_stopping());
        v.tick_gate(); // gate hits 0 -> note-off
        assert!(v.is_stopping());
    }
}
