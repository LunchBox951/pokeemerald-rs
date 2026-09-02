//! Owns active DirectSound and CGB voices and renders interleaved stereo frames.
//!
//! DirectSound admission preserves the fixed channel-array scan from
//! `ply_note` (`m4a_1.s:1669..1718`). The first free slot wins. If the pool is
//! full, a released voice outranks every sounding candidate; otherwise the
//! weakest voice that the incoming priority and track can displace wins. At
//! equal priority, the incoming note displaces its own or a later track; a full
//! tie displaces the last slot.
//! CGB voices apply the same priority and track test to their one fixed hardware
//! slot (`m4a_1.s:1647..1668`).
//!
//! Reverb seeds an `i32` accumulator before voices render. The finished samples
//! are clipped to the signed 8-bit range before entering the reverb delay and
//! being normalised to `f32`. Upstream instead sums packed 8-bit lanes with
//! wrapping carry between adjacent samples (`m4a_1.s:396..437`).

use crate::cgb_envelope::CgbEnvelopeCadence;
use crate::cgb_voice::CgbVoice;
use crate::pitch::SAMPLES_PER_FRAME;
use crate::psg::FrameSequencer128Hz;
use crate::reverb::Reverb;
use crate::voice::{StereoAcc, Voice};

#[cfg(test)]
#[path = "mixer_priority.rs"]
mod priority_tests;

#[cfg(test)]
#[path = "mixer_cgb_envelope.rs"]
mod cgb_envelope_cadence_tests;

/// Emerald's default global mix level, on a scale from 0 to 15.
pub const DEFAULT_MASTER_VOLUME: u8 = 12;

/// Emerald's default DirectSound voice cap.
pub const DEFAULT_MAX_VOICES: usize = 5;

const MAX_SWEEP_TICKS_PER_FRAME: usize =
    (SAMPLES_PER_FRAME * 128).div_ceil(crate::pitch::MIXER_RATE as usize);

#[derive(Clone, Copy, Debug)]
enum VoiceSlot {
    DirectSound(usize),
    Cgb(usize),
}

/// Owns the playing voices and renders them to interleaved stereo `f32`.
#[derive(Debug)]
pub struct Mixer {
    direct_sound_slots: Vec<Option<Voice>>,
    cgb_slots: [Option<CgbVoice>; 4],
    master_volume: u8,
    next_note_on_ordinal: u64,
    mix_buffer: Vec<StereoAcc>,
    reverb: Reverb,
    sweep_clock: FrameSequencer128Hz,
    sweep_ticks: Vec<usize>,
    cgb_envelope_cadence: CgbEnvelopeCadence,
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
            direct_sound_slots: std::iter::repeat_with(|| None).take(max_voices).collect(),
            cgb_slots: [None, None, None, None],
            master_volume,
            next_note_on_ordinal: 0,
            mix_buffer: vec![(0, 0); SAMPLES_PER_FRAME],
            reverb: Reverb::new(0),
            sweep_clock: FrameSequencer128Hz::default(),
            sweep_ticks: Vec::with_capacity(MAX_SWEEP_TICKS_PER_FRAME),
            cgb_envelope_cadence: CgbEnvelopeCadence::default(),
        }
    }

    /// Set the song-header reverb level. Zero disables reverb.
    #[must_use]
    pub(crate) fn with_reverb_level(mut self, level: u8) -> Self {
        self.reverb = Reverb::new(level);
        self
    }

    fn take_note_on_ordinal(&mut self) -> u64 {
        let ordinal = self.next_note_on_ordinal;
        self.next_note_on_ordinal += 1;
        ordinal
    }

    /// Number of voices currently sounding, DirectSound and CGB combined.
    #[must_use]
    pub fn voice_count(&self) -> usize {
        self.live_direct_sound_voices().count()
            + self.cgb_slots.iter().filter(|slot| slot.is_some()).count()
    }

    /// Whether any voice (DirectSound or CGB) is active.
    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.direct_sound_slots.iter().all(Option::is_none)
            && self.cgb_slots.iter().all(Option::is_none)
    }

    /// Whether the master-mix reverb still holds delayed samples that can
    /// produce a wet tail after all voices have stopped.
    pub(crate) fn has_pending_reverb(&self) -> bool {
        self.reverb.has_pending_samples()
    }

    fn live_direct_sound_voices(&self) -> impl Iterator<Item = &Voice> {
        self.direct_sound_slots.iter().flatten()
    }

    #[cfg(test)]
    pub(crate) fn voices(&self) -> Vec<&Voice> {
        self.live_direct_sound_voices().collect()
    }

    #[cfg(test)]
    pub(crate) fn cgb_voices(&self) -> &[Option<CgbVoice>; 4] {
        &self.cgb_slots
    }

    /// The global mix level.
    #[must_use]
    pub fn master_volume(&self) -> u8 {
        self.master_volume
    }

    /// The DirectSound voice cap.
    #[must_use]
    pub fn max_voices(&self) -> usize {
        self.direct_sound_slots.len()
    }

    /// Start a DirectSound voice if a free, released, or lower-ranked slot is available.
    pub fn add_voice(&mut self, mut voice: Voice) -> bool {
        let Some(slot) = self.select_direct_sound_slot(voice.priority(), voice.track()) else {
            return false;
        };
        voice.set_seq(self.take_note_on_ordinal());
        self.direct_sound_slots[slot] = Some(voice);
        true
    }

    fn select_direct_sound_slot(&self, priority: u8, track: usize) -> Option<usize> {
        let mut candidate_priority = priority;
        let mut candidate_track = track;
        let mut candidate_slot = None;
        let mut only_released_voices_compete = false;

        for (index, slot) in self.direct_sound_slots.iter().enumerate() {
            let Some(voice) = slot else {
                return Some(index);
            };
            if voice.is_stopping() {
                if !only_released_voices_compete {
                    only_released_voices_compete = true;
                    candidate_priority = voice.priority();
                    candidate_track = voice.track();
                    candidate_slot = Some(index);
                    continue;
                }
            } else if only_released_voices_compete {
                continue;
            }

            let voice_priority = voice.priority();
            if voice_priority < candidate_priority
                || (voice_priority == candidate_priority && voice.track() >= candidate_track)
            {
                candidate_priority = voice_priority;
                candidate_track = voice.track();
                candidate_slot = Some(index);
            }
        }

        candidate_slot
    }

    /// Start a CGB voice if its fixed hardware slot is reusable.
    pub fn add_cgb_voice(&mut self, mut voice: CgbVoice) -> bool {
        let slot = voice.channel().slot();
        if let Some(occupant) = &self.cgb_slots[slot] {
            let reusable = occupant.is_stopping()
                || occupant.priority() < voice.priority()
                || (occupant.priority() == voice.priority() && occupant.track() >= voice.track());
            if !reusable {
                return false;
            }
        }
        voice.set_seq(self.take_note_on_ordinal());
        self.cgb_slots[slot] = Some(voice);
        true
    }

    /// Tick every voice's note-off gate down by one sequencer tick.
    pub fn tick_gates(&mut self) {
        for voice in self.direct_sound_slots.iter_mut().flatten() {
            voice.tick_gate();
        }
        for voice in self.cgb_slots.iter_mut().flatten() {
            voice.tick_gate();
        }
    }

    /// Release the newest voice on `track` with the given MIDI `key`.
    pub fn note_off_track(&mut self, track: usize, key: u8) {
        let direct_sound_matches = self
            .direct_sound_slots
            .iter()
            .enumerate()
            .filter_map(|(index, voice)| voice.as_ref().map(|voice| (index, voice)))
            .filter(|(_, voice)| {
                voice.track() == track && !voice.is_stopping() && voice.midi_key() == key
            })
            .map(|(index, voice)| (voice.seq(), VoiceSlot::DirectSound(index)));
        let cgb_matches = self
            .cgb_slots
            .iter()
            .enumerate()
            .filter_map(|(index, voice)| voice.as_ref().map(|voice| (index, voice)))
            .filter(|(_, voice)| {
                voice.track() == track && !voice.is_stopping() && voice.midi_key() == key
            })
            .map(|(index, voice)| (voice.seq(), VoiceSlot::Cgb(index)));

        let newest_match = direct_sound_matches
            .chain(cgb_matches)
            .max_by_key(|(ordinal, _)| *ordinal)
            .map(|(_, slot)| slot);

        match newest_match {
            Some(VoiceSlot::DirectSound(index)) => {
                if let Some(voice) = &mut self.direct_sound_slots[index] {
                    voice.note_off();
                }
            }
            Some(VoiceSlot::Cgb(index)) => {
                if let Some(voice) = &mut self.cgb_slots[index] {
                    voice.note_off();
                }
            }
            None => {}
        }
    }

    /// Release every voice on `track`.
    pub fn release_track(&mut self, track: usize) {
        for voice in self.direct_sound_slots.iter_mut().flatten() {
            if voice.track() == track && !voice.is_stopping() {
                voice.note_off();
            }
        }
        for voice in self.cgb_slots.iter_mut().flatten() {
            if voice.track() == track && !voice.is_stopping() {
                voice.note_off();
            }
        }
    }

    /// Apply updated track volume and panning to every live voice on `track`.
    pub fn set_track_volume(&mut self, track: usize, vol_mr: u8, vol_ml: u8) {
        for voice in self.direct_sound_slots.iter_mut().flatten() {
            if voice.track() == track {
                voice.set_track_volume(vol_mr, vol_ml);
            }
        }
        for voice in self.cgb_slots.iter_mut().flatten() {
            if voice.track() == track {
                voice.set_track_volume(vol_mr, vol_ml);
            }
        }
    }

    /// Apply updated track pitch to every live voice on `track`.
    pub fn set_track_pitch(&mut self, track: usize, key_m: i32, pit_m: u8) {
        for voice in self.direct_sound_slots.iter_mut().flatten() {
            if voice.track() == track {
                voice.set_track_pitch(key_m, pit_m);
            }
        }
        for voice in self.cgb_slots.iter_mut().flatten() {
            if voice.track() == track {
                voice.set_track_pitch(key_m, pit_m);
            }
        }
    }

    /// Render one frame of interleaved stereo samples and retire silent voices.
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

        self.reverb.seed_frame(&mut self.mix_buffer);

        for slot in &mut self.direct_sound_slots {
            if let Some(voice) = slot {
                voice.begin_frame(self.master_volume);
                voice.render(&mut self.mix_buffer);
                if !voice.is_active() {
                    *slot = None;
                }
            }
        }

        self.sweep_clock
            .advance_into(self.mix_buffer.len(), &mut self.sweep_ticks);
        debug_assert!(
            self.sweep_ticks.len() <= MAX_SWEEP_TICKS_PER_FRAME,
            "a frame's tick buffer must never have to grow",
        );
        let extra_envelope_iteration = self.cgb_envelope_cadence.advance_frame();
        for slot in &mut self.cgb_slots {
            if let Some(voice) = slot {
                voice.begin_frame(self.master_volume, extra_envelope_iteration);
                voice.render(&mut self.mix_buffer, &self.sweep_ticks);
                if !voice.is_active() {
                    *slot = None;
                }
            }
        }

        for sample in &mut self.mix_buffer {
            *sample = (clip_to_s8(sample.0), clip_to_s8(sample.1));
        }
        self.reverb.commit_frame(&self.mix_buffer);

        for (frame, &(left, right)) in self.mix_buffer.iter().enumerate() {
            out[frame * 2] = normalise_s8(left);
            out[frame * 2 + 1] = normalise_s8(right);
        }
    }
}

fn clip_to_s8(sample: i32) -> i32 {
    sample.clamp(-128, 127)
}

fn normalise_s8(sample: i32) -> f32 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "clamped to [-128, 127] by clip_to_s8, every value is exactly \
                  representable in f32"
    )]
    let value = sample as f32;
    value / 128.0
}

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
