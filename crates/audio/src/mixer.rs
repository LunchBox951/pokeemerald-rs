//! The software mixer: owns the active [`Voice`]s, sums them into interleaved
//! stereo `f32`, and clips.
//!
//! Behavioural model of `SoundMainRAM`'s outer channel loop (`m4a_1.s:153`):
//! every active voice steps its envelope once per frame, then adds its
//! contribution to the shared buffer. The hardware sums into 8-bit lanes that
//! wrap; here contributions accumulate in `i32` and are **clipped** (not
//! wrapped) to the `s8` range on the way out, then normalised to `[-1.0, 1.0)`
//! for the `f32` producer `(no-verbatim)`. Clipping rather than wrapping is a
//! deliberate, benign fidelity choice for this slice — the wrap-and-carry
//! behaviour of the packed DMA lanes is a deferred quirk.
//!
//! # Note-on channel allocation (`ply_note`'s priority search)
//!
//! A note-on does not simply append a voice: upstream searches the fixed
//! channel array for a slot to *reuse*, *steal*, or — failing both — refuses
//! the note outright. [`Mixer::add_voice`] reproduces that search over the
//! DirectSound pool (`m4a_1.s:1669`..`:1718`) and [`Mixer::add_cgb_voice`]
//! its single-slot CGB equivalent (`m4a_1.s:1647`..`:1668`). Reading the asm
//! scan, in the order its branches resolve:
//!
//! 1. A channel that is not `SOUND_CHANNEL_SF_ON` (free) ends the scan
//!    immediately and is used as-is (`m4a_1.s:1678`..`:1681`). Because the
//!    scan walks the array from index `0`, *any* free channel wins over
//!    every occupied one, and the one taken is the lowest-indexed free slot.
//! 2. Otherwise a channel flagged `SOUND_CHANNEL_SF_STOP` (already released,
//!    still ringing out) outranks every still-sounding one: the first
//!    stopped channel seen replaces the running candidate *unconditionally*,
//!    ignoring priority entirely, and from then on the scan skips
//!    still-sounding channels altogether (`m4a_1.s:1682`..`:1693`).
//! 3. Among the remaining comparable channels the running candidate starts
//!    at the *new* note's own priority and track, so a channel is only ever
//!    taken when it does not outrank the incoming note (until rule 2
//!    reseeds the candidate from a released channel). Strictly lower
//!    priority wins (`m4a_1.s:1694`..`:1700`); strictly higher is skipped
//!    (`m4a_1.s:1701`..`:1702`).
//! 4. On an exact priority tie the *track* decides: a channel owned by a
//!    strictly later track becomes the candidate (`m4a_1.s:1703`..`:1707`),
//!    a strictly earlier one is skipped (`m4a_1.s:1708`..`:1709`), and an
//!    equal track is taken too — so the *last* such channel in scan order
//!    wins (`m4a_1.s:1710`..`:1711`). Because the candidate seeds with the
//!    incoming note's own track, an equal-priority channel is stolen exactly
//!    when its track index is `>=` the new note's: a track can always
//!    displace itself and any later track, never an earlier one.
//! 5. If nothing was selected the note is refused (`m4a_1.s:1716`..`:1718`).
//!
//! Upstream compares `MusicPlayerTrack *` pointers, which are elements of one
//! array, so pointer order is track-index order; we compare the indices
//! directly `(no-verbatim)`.
//!
//! ## The fixed slot pool
//!
//! Upstream's `maxChans` DirectSound channels are a fixed array whose free
//! slots are distinguished by a cleared `SF_ON` flag, so scan order *is*
//! slot order and a channel that goes silent leaves a hole its neighbours
//! do not close up. The pool here has the same shape: exactly `max_voices`
//! `Option<Voice>` slots, `None` standing for a cleared `SF_ON`.
//! [`Mixer::mix_frame`] clears a slot in place when its voice falls silent,
//! rule 1 lands a note in the lowest free slot because that is the first one
//! the scan reaches, and rules 2-5 compare in slot-index order.
//!
//! Slot order is load-bearing, not bookkeeping: rule 4's
//! equal-priority/equal-track arm is a *take*, so the victim is the **last**
//! matching channel in the scan. Voices that agree on both priority and
//! track are ordinary distinct notes — a chord on one track produces a
//! whole set of them — so picking a different one of them cuts a different
//! note. A pool that compacted on retire would drift out of slot order the
//! moment any voice ended mid-song (upstream refills the freed slot; a
//! compacted list appends at the end) and would then steal the wrong voice;
//! `mixer_priority`'s `a_retired_slot_is_refilled_in_place` pins that case.
//!
//! Where a note lands stays unobservable to the render path: voices sum into
//! one shared `i32` accumulator, so their contributions commute, and empty
//! slots are simply skipped.
//!
//! Before voices mix in, [`crate::reverb::Reverb`] seeds the same
//! accumulator with the master-mix reverb/pseudo-echo stage's wet
//! contribution (S-3, issue #185) — see that module's docs. A disabled
//! [`crate::reverb::Reverb`] (the default) seeds exactly the all-zero reset
//! this mixer always performed, so every pre-reverb test stays unaffected.

use crate::cgb_voice::CgbVoice;
use crate::pitch::SAMPLES_PER_FRAME;
use crate::reverb::Reverb;
use crate::voice::{StereoAcc, Voice};

/// The note-on channel-search tests, kept out of this file so it stays near
/// the repo's per-file size guideline; a child module so they can drive the
/// pool directly (`AGENTS.md`'s one-module-one-concept rule).
#[cfg(test)]
#[path = "mixer_priority.rs"]
mod priority_tests;

/// Default global mix level (`0..=15`). Emerald's `m4aSoundInit` reconfigures
/// the driver the instant it starts, calling `m4aSoundMode` with
/// `12 << SOUND_MODE_MASVOL_SHIFT` (`m4a.c:78`..`:81`); the generic
/// `SoundInit` placeholder of `15` (`m4a.c:383`) never reaches gameplay.
pub const DEFAULT_MASTER_VOLUME: u8 = 12;

/// Default DirectSound voice cap. The hardware allows up to
/// `MAX_DIRECTSOUND_CHANNELS` (12); Emerald's `m4aSoundInit` configures `5`
/// via `5 << SOUND_MODE_MAXCHN_SHIFT` (`m4a.c:78`..`:81`), overriding the
/// generic `SoundInit` placeholder of `8` (`m4a.c:384`). A note-on past the
/// cap reuses or steals a channel per the module docs' priority search, or is
/// refused when every live voice outranks it.
pub const DEFAULT_MAX_VOICES: usize = 5;

/// Owns the playing voices and renders them to interleaved stereo `f32`.
#[derive(Debug)]
pub struct Mixer {
    /// The DirectSound channel pool: exactly `max_voices` fixed slots, `None`
    /// where upstream's array would show a cleared `SF_ON` (module docs).
    /// Indices are stable — a retiring voice clears its own slot and the rest
    /// stay put — so the note-on scan sees upstream's channel order.
    voices: Vec<Option<Voice>>,
    /// The four fixed CGB PSG hardware channels (square 1, square 2, wave,
    /// noise — indexed by [`crate::cgb_voice::CgbChannelNumber::slot`]).
    /// Unlike the pooled DirectSound voices, each of these exists at most
    /// once: a new note on the same channel number may replace whatever was
    /// already sounding there, subject to the priority test in the module
    /// docs (`m4a_1.s:1647`..`:1668`), mirroring `CgbSound`'s loop over fixed
    /// channel slots (`m4a.c:946`).
    cgb_voices: [Option<CgbVoice>; 4],
    master_volume: u8,
    /// Monotonic note-on ordinal handed to each voice as it starts, shared by
    /// the DirectSound pool and the CGB slots. `ply_note` prepends every new
    /// channel of either kind onto one per-track chain (`m4a_1.s:1719`), so a
    /// larger ordinal is unambiguously newer regardless of voice kind; this
    /// lets [`Self::note_off_track`] pick the single newest end-of-tie match
    /// across both collections. Lives on the mixer, not in global state.
    next_seq: u64,
    /// Reusable per-frame accumulator, sized to [`SAMPLES_PER_FRAME`], so
    /// steady-state rendering does not allocate.
    scratch: Vec<StereoAcc>,
    /// The master-mix reverb/pseudo-echo stage (module docs). Disabled
    /// (`level 0`) unless [`Self::with_reverb_level`] is called.
    reverb: Reverb,
}

impl Default for Mixer {
    fn default() -> Self {
        Self::new(DEFAULT_MASTER_VOLUME, DEFAULT_MAX_VOICES)
    }
}

impl Mixer {
    /// A mixer with an explicit master volume and voice cap.
    #[must_use]
    pub fn new(master_volume: u8, max_voices: usize) -> Self {
        Self {
            voices: std::iter::repeat_with(|| None).take(max_voices).collect(),
            cgb_voices: [None, None, None, None],
            master_volume,
            next_seq: 0,
            scratch: vec![(0, 0); SAMPLES_PER_FRAME],
            reverb: Reverb::new(0),
        }
    }

    /// Set this mixer's reverb level (`SongHeader::reverb`, `0..=127`; `0`
    /// disables it) — see [`crate::reverb::Reverb`]. Chainable onto
    /// [`Self::new`]/[`Default::default`], mirroring [`crate::song::ToneData::fixed`]'s
    /// builder shape.
    #[must_use]
    pub(crate) fn with_reverb_level(mut self, level: u8) -> Self {
        self.reverb = Reverb::new(level);
        self
    }

    /// The next shared note-on ordinal, advancing the counter.
    fn take_seq(&mut self) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        seq
    }

    /// Number of voices currently sounding, DirectSound and CGB combined.
    #[must_use]
    pub fn voice_count(&self) -> usize {
        self.live_voices().count() + self.cgb_voices.iter().filter(|v| v.is_some()).count()
    }

    /// Whether any voice (DirectSound or CGB) is active.
    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.voices.iter().all(Option::is_none) && self.cgb_voices.iter().all(Option::is_none)
    }

    /// Whether the master-mix reverb still holds delayed samples that can
    /// produce a wet tail after all voices have stopped.
    pub(crate) fn has_pending_reverb(&self) -> bool {
        self.reverb.has_pending_samples()
    }

    /// The occupied slots, in slot order.
    fn live_voices(&self) -> impl Iterator<Item = &Voice> {
        self.voices.iter().flatten()
    }

    /// Read-only view of the live voices in slot order, for the sequencer's
    /// own tests to inspect mid-note volume/pitch updates. Empty slots are
    /// skipped; use the `voices` field itself to assert on slot positions.
    #[cfg(test)]
    pub(crate) fn voices(&self) -> Vec<&Voice> {
        self.live_voices().collect()
    }

    /// Read-only view of the CGB channel slots, for cross-kind end-of-tie
    /// tests to inspect which slot was released.
    #[cfg(test)]
    pub(crate) fn cgb_voices(&self) -> &[Option<CgbVoice>; 4] {
        &self.cgb_voices
    }

    /// The global mix level.
    #[must_use]
    pub fn master_volume(&self) -> u8 {
        self.master_volume
    }

    /// The DirectSound voice cap — the pool's fixed slot count.
    #[must_use]
    pub fn max_voices(&self) -> usize {
        self.voices.len()
    }

    /// Start a DirectSound voice through `ply_note`'s channel search (module
    /// docs, `m4a_1.s:1669`..`:1718`): take the lowest free slot, else steal
    /// the weakest live voice the newcomer outranks, else refuse. Returns
    /// whether the note started.
    pub fn add_voice(&mut self, mut voice: Voice) -> bool {
        let Some(slot) = self.select_slot(voice.priority(), voice.track()) else {
            return false;
        };
        voice.set_seq(self.take_seq());
        // Upstream overwrites the chosen `SoundChannel` wholesale, so a
        // stolen note is cut dead rather than released (`m4a_1.s:1719`..).
        self.voices[slot] = Some(voice);
        true
    }

    /// Which slot `ply_note`'s scan hands a new note of `priority` on
    /// `track` — a free one, else the voice it steals — or `None` when every
    /// live voice outranks it; see the module docs for each rule's
    /// derivation from the asm.
    ///
    /// The candidate seeds with the *incoming* note's own priority/track
    /// (`m4a_1.s:1669`..`:1671`), which is what makes "refuse" and "steal an
    /// equal-priority later track" fall out of the same comparison. The walk
    /// is in slot-index order, exactly upstream's array walk, so rule 4's
    /// take-on-equal arm settles a full tie on the *last* slot.
    fn select_slot(&self, priority: u8, track: usize) -> Option<usize> {
        let mut best_priority = priority;
        let mut best_track = track;
        let mut best = None;
        let mut saw_stopped = false;

        for (index, slot) in self.voices.iter().enumerate() {
            let Some(voice) = slot else {
                // Rule 1: a free channel ends the scan there and then, so the
                // note lands in the lowest free slot (`m4a_1.s:1678`..`:1681`).
                return Some(index);
            };
            if voice.is_stopping() {
                // Rule 2: the first released voice takes over the candidate
                // outright, whatever its priority (`m4a_1.s:1682`..`:1690`).
                if !saw_stopped {
                    saw_stopped = true;
                    best_priority = voice.priority();
                    best_track = voice.track();
                    best = Some(index);
                    continue;
                }
            } else if saw_stopped {
                // ...and still-sounding voices stop competing once one has
                // (`m4a_1.s:1691`..`:1693`).
                continue;
            }

            // Rules 3-4, the shared comparison both arms fall into
            // (`m4a_1.s:1694`..`:1711`).
            let voice_priority = voice.priority();
            if voice_priority < best_priority {
                best_priority = voice_priority;
                best_track = voice.track();
                best = Some(index);
            } else if voice_priority == best_priority && voice.track() >= best_track {
                best_track = best_track.max(voice.track());
                best = Some(index);
            }
        }

        // Rule 5.
        best
    }

    /// Start a CGB voice on its one fixed hardware channel, applying
    /// `ply_note`'s CGB-arm test to whatever already occupies that slot
    /// (`m4a_1.s:1647`..`:1668`). Returns whether the note started.
    pub fn add_cgb_voice(&mut self, mut voice: CgbVoice) -> bool {
        let slot = voice.channel().slot();
        if let Some(occupant) = &self.cgb_voices[slot] {
            // A released occupant is always reusable; otherwise the newcomer
            // needs a strictly higher priority, or an equal one plus a track
            // index no later than the occupant's.
            let reusable = occupant.is_stopping()
                || occupant.priority() < voice.priority()
                || (occupant.priority() == voice.priority() && occupant.track() >= voice.track());
            if !reusable {
                return false;
            }
        }
        voice.set_seq(self.take_seq());
        self.cgb_voices[slot] = Some(voice);
        true
    }

    /// Tick every voice's note-off gate down by one sequencer tick.
    pub fn tick_gates(&mut self) {
        for voice in self.voices.iter_mut().flatten() {
            voice.tick_gate();
        }
        for voice in self.cgb_voices.iter_mut().flatten() {
            voice.tick_gate();
        }
    }

    /// Release the *newest* still-sounding voice on `track` whose MIDI key is
    /// `key` (end-of-tie), then stop.
    ///
    /// This mirrors `ply_endtie` (`m4a_1.s:1819`): it walks the track's single
    /// channel chain from the head and stops the first channel matching
    /// `track->key`. `ply_note` prepends every new channel — DirectSound *or*
    /// CGB — onto that one shared chain (`m4a_1.s:1719`), so "first in the
    /// chain" is the most recently started voice of *either* kind. The two
    /// kinds live in separate collections here, so we select the single match
    /// with the largest shared note-on ordinal (`next_seq`) across both
    /// and release only it. Scanning one pool fully before the other would
    /// wrongly retire an older same-key voice when the newer match sits in the
    /// other pool (reachable when a `VOICE` change swaps instrument kind
    /// between overlapping same-key ties).
    pub fn note_off_track(&mut self, track: usize, key: u8) {
        // The newest match found so far, as `(seq, index, is_cgb)`.
        let mut best: Option<(u64, usize, bool)> = None;
        let is_newer = |best: &Option<(u64, usize, bool)>, seq: u64| {
            best.is_none_or(|(best_seq, _, _)| seq > best_seq)
        };

        for (i, voice) in self.voices.iter().enumerate() {
            if let Some(voice) = voice {
                if voice.track() == track
                    && !voice.is_stopping()
                    && voice.midi_key() == key
                    && is_newer(&best, voice.seq())
                {
                    best = Some((voice.seq(), i, false));
                }
            }
        }
        for (i, voice) in self.cgb_voices.iter().enumerate() {
            if let Some(voice) = voice {
                if voice.track() == track
                    && !voice.is_stopping()
                    && voice.midi_key() == key
                    && is_newer(&best, voice.seq())
                {
                    best = Some((voice.seq(), i, true));
                }
            }
        }

        match best {
            Some((_, i, false)) => {
                if let Some(voice) = &mut self.voices[i] {
                    voice.note_off();
                }
            }
            Some((_, i, true)) => {
                if let Some(voice) = &mut self.cgb_voices[i] {
                    voice.note_off();
                }
            }
            None => {}
        }
    }

    /// Release every still-sounding voice on `track` (track-end / `FINE`).
    ///
    /// Mirrors `ply_fine` (`m4a_1.s:750`): it walks the track's channel chain
    /// and sets `SOUND_CHANNEL_SF_STOP` on every channel still flagged
    /// `SOUND_CHANNEL_SF_ON`, releasing it, before the track is cleared. Without
    /// this a track owning a tied voice (`gate_time == 0`, never auto-releasing)
    /// or a looping wave would leave that voice sounding forever.
    pub fn release_track(&mut self, track: usize) {
        for voice in self.voices.iter_mut().flatten() {
            if voice.track() == track && !voice.is_stopping() {
                voice.note_off();
            }
        }
        for voice in self.cgb_voices.iter_mut().flatten() {
            if voice.track() == track && !voice.is_stopping() {
                voice.note_off();
            }
        }
    }

    /// Rewrite the live base volumes of `track`'s channels from updated track
    /// `volMR`/`volML` (`MPT_FLG_VOLCHG`).
    ///
    /// Mirrors `MPlayMain`'s per-tick re-run of `ChnVolSetAsm` for each ON
    /// channel of a flagged track (`m4a_1.s:1394`): each voice keeps its own
    /// velocity, so a mid-note `VOL`/`PAN` change is audible on the held note.
    pub fn set_track_volume(&mut self, track: usize, vol_mr: u8, vol_ml: u8) {
        for voice in self.voices.iter_mut().flatten() {
            if voice.track() == track {
                voice.set_track_volume(vol_mr, vol_ml);
            }
        }
        for voice in self.cgb_voices.iter_mut().flatten() {
            if voice.track() == track {
                voice.set_track_volume(vol_mr, vol_ml);
            }
        }
    }

    /// Rewrite the live playback frequency of `track`'s channels from updated
    /// track `keyM`/`pitM` (`MPT_FLG_PITCHG`).
    ///
    /// Mirrors `MPlayMain`'s per-tick pitch re-run for each ON channel of a
    /// flagged track (`m4a_1.s:1403`..`:1451`): it recomputes the played
    /// frequency from the channel's stored key plus the track's updated key/fine
    /// offset via `MidiKeyToFreq`, so a mid-note `BEND`/`BENDR`/`KEYSH`/`TUNE`
    /// bends the held note.
    pub fn set_track_pitch(&mut self, track: usize, key_m: i32, pit_m: u8) {
        for voice in self.voices.iter_mut().flatten() {
            if voice.track() == track {
                voice.set_track_pitch(key_m, pit_m);
            }
        }
        for voice in self.cgb_voices.iter_mut().flatten() {
            if voice.track() == track {
                voice.set_track_pitch(key_m, pit_m);
            }
        }
    }

    /// Render exactly one frame ([`SAMPLES_PER_FRAME`] stereo samples) into
    /// `out`, which must hold `SAMPLES_PER_FRAME * 2` interleaved `f32`s
    /// (`[l, r, l, r, …]`). Retires voices that fell silent.
    ///
    /// # Panics
    ///
    /// Panics if `out.len() != SAMPLES_PER_FRAME * 2`.
    pub fn mix_frame(&mut self, out: &mut [f32]) {
        assert_eq!(
            out.len(),
            SAMPLES_PER_FRAME * 2,
            "mix_frame expects one frame of interleaved stereo",
        );

        // Seed from the reverb stage rather than a plain zero-reset (module
        // docs): a disabled stage seeds exactly the zeroes this loop used to
        // write inline, so this is a no-op change for every song that
        // doesn't set a reverb level.
        self.reverb.seed_frame(&mut self.scratch);

        // A voice that fell silent clears *its own* slot: upstream's array
        // never shifts, and the note-on scan reads slot order (module docs).
        for slot in &mut self.voices {
            if let Some(voice) = slot {
                voice.begin_frame(self.master_volume);
                voice.render(&mut self.scratch);
                if !voice.is_active() {
                    *slot = None;
                }
            }
        }

        for slot in &mut self.cgb_voices {
            if let Some(voice) = slot {
                voice.begin_frame(self.master_volume);
                voice.render(&mut self.scratch);
                if !voice.is_active() {
                    *slot = None;
                }
            }
        }

        // Clip in place first: the reverb delay line stores the same
        // clipped `s8`-range samples upstream's `pcmBuffer` would hold once
        // a frame's mixing finished (`crate::reverb`'s module docs), and
        // `out`'s normalisation reads from that same clipped value.
        for acc in &mut self.scratch {
            *acc = (clip_i32(acc.0), clip_i32(acc.1));
        }
        self.reverb.commit_frame(&self.scratch);

        for (frame, &(l, r)) in self.scratch.iter().enumerate() {
            out[frame * 2] = to_f32(l);
            out[frame * 2 + 1] = to_f32(r);
        }
    }
}

/// Clamp a summed accumulator to the `s8` range.
fn clip_i32(sample: i32) -> i32 {
    sample.clamp(-128, 127)
}

/// Normalise an already-`s8`-clamped sample to `[-1.0, 1.0)`.
fn to_f32(sample: i32) -> f32 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "clamped to [-128, 127] by clip_i32, every value is exactly \
                  representable in f32"
    )]
    let value = sample as f32;
    value / 128.0
}

/// The frame-mixing and note-off tests, split out for the same reason as
/// [`priority_tests`]: this file stays near the repo's per-file size
/// guideline (`AGENTS.md`).
#[cfg(test)]
#[expect(
    clippy::cast_precision_loss,
    clippy::float_cmp,
    reason = "expected values are computed from small integer terms whose casts \
              are exact at these magnitudes, and silence checks compare \
              exactly-representable 0.0/-1.0 values on purpose"
)]
#[path = "mixer_mixing.rs"]
mod tests;
