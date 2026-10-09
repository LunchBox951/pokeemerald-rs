//! An owned M4A sequencer: decodes each track's events, applies them to
//! per-track state, and drives the [`Mixer`] one V-blank frame at a time
//! (`MPlayMain`, `m4a_1.s:1129`).
//!
//! `PORT` and every `XCMD` besides the pseudo-echo pair (`xIECV`/`xIECL`)
//! decode but never execute.

use crate::cgb_voice::{CgbChannelNumber, CgbVoice};
use crate::pitch::{self, SAMPLES_PER_FRAME};
use crate::psg::WaveChannel;
use crate::sequence::{clamp_tempo, Event, MAX_TEMPO_BPM};
use crate::song::{Instrument, Song};
use crate::voice::{channel_volume, pan_terms, Voice};
use crate::{Mixer, DEFAULT_MASTER_VOLUME, DEFAULT_MAX_VOICES};

/// `XCMD` sub-command number for `xIECV` (pseudo-echo volume,
/// `m4a_tables.c:252`).
const XCMD_IECV: u8 = 0x08;
/// `XCMD` sub-command number for `xIECL` (pseudo-echo length,
/// `m4a_tables.c:253`).
const XCMD_IECL: u8 = 0x09;

/// Ticks fire each time the accumulator crosses this threshold
/// (`subs r0, 150`, `m4a_1.s:1169`).
const TEMPO_UNIT: u16 = 150;

/// Track volume before any `VOL` command: `MPlayStart` clears the track and
/// restores no `vol` field (`m4a_1.s:1214`-`:1230`), so the resolved channel
/// volume is zero until the sequence sets one (`m4a.c:772`).
const DEFAULT_TRACK_VOLUME: u8 = 0;

/// Default pitch-bend range (`track->bendRange = 2`, `m4a_1.s:1223`).
const DEFAULT_BEND_RANGE: u8 = 2;

/// Default LFO rate (`track->lfoSpeed = 0x16`, `m4a_1.s:1226`).
const DEFAULT_LFO_SPEED: u8 = 22;

/// Max nested `PATT` depth (`track->patternStack`'s capacity, `ply_patt`,
/// `m4a_1.s:851`).
const MAX_PATTERN_DEPTH: usize = 3;

/// Default `volX` input to `TrkVolPitSet` absent an active fade
/// (`m4a.c:772`).
const TRACK_VOLUME_SCALE: u8 = 0x40;

/// Upper bound of `SOUND_MODE_REVERB_VAL`, the byte upstream masks the
/// resolved reverb level into (`m4a_internal.h:12`, `m4a.c:445`).
const MAX_REVERB_LEVEL: u8 = 127;

/// Safety bound on commands processed for one track in one tick, so a
/// malformed loop with no `Wait` cannot hang the mixer.
const MAX_COMMANDS_PER_TICK: u32 = 4096;

/// Size of the `MEMACC` accumulator area (`gMPlayMemAccArea[0x10]`,
/// `m4a.c:20`).
const MEM_ACC_LEN: usize = 16;

// `ply_memacc` owns this exact command ordering (`m4a.c:1437`..`:1521`).
const MEMACC_SET: u8 = 0;
const MEMACC_ADD: u8 = 1;
const MEMACC_SUB: u8 = 2;
const MEMACC_COPY: u8 = 3;
const MEMACC_ADD_CELL: u8 = 4;
const MEMACC_SUB_CELL: u8 = 5;
const MEMACC_EQ: u8 = 6;
const MEMACC_NE: u8 = 7;
const MEMACC_GT: u8 = 8;
const MEMACC_GE: u8 = 9;
const MEMACC_LE: u8 = 10;
const MEMACC_LT: u8 = 11;
const MEMACC_CELL_EQ: u8 = 12;
const MEMACC_CELL_NE: u8 = 13;
const MEMACC_CELL_GT: u8 = 14;
const MEMACC_CELL_GE: u8 = 15;
const MEMACC_CELL_LE: u8 = 16;
const MEMACC_CELL_LT: u8 = 17;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct MemAccArea {
    cells: [u8; MEM_ACC_LEN],
}

impl MemAccArea {
    fn read(&self, address: u8) -> Option<u8> {
        self.cells.get(usize::from(address)).copied()
    }

    fn write(&mut self, address: u8, value: u8) {
        if let Some(cell) = self.cells.get_mut(usize::from(address)) {
            *cell = value;
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ModulationTarget(u8);

impl ModulationTarget {
    const PITCH: Self = Self(0);
    const AMPLITUDE: Self = Self(1);
    const PAN: Self = Self(2);

    fn from_command(value: u8) -> Self {
        Self(value)
    }

    fn is_pitch(self) -> bool {
        self == Self::PITCH
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TrackState {
    cursor: usize,
    wait: u16,
    ended: bool,
    voice: usize,
    vol: u8,
    /// This track's `volX` input to `TrkVolPitSet` (`m4a.c:772`).
    vol_x: u8,
    pan: i8,
    bend: i8,
    bend_range: u8,
    tune: i8,
    key_shift: i8,
    /// Reused as the match key when [`Event::EndOfTie`] omits its key
    /// operand (`m4a_1.s:1830`).
    key: u8,
    lfo_depth: u8,
    modulation_target: ModulationTarget,
    lfo_speed: u8,
    lfo_delay: u8,
    lfo_delay_remaining: u8,
    lfo_phase: u8,
    modulation: i8,
    pattern_return_stack: [usize; MAX_PATTERN_DEPTH],
    pattern_depth: usize,
    repeat_counter: u8,
    /// Applies only to voices started after it last changed
    /// (`m4a_1.s:1757`-`:1758`).
    pseudo_echo_volume: u8,
    /// Same snapshot-timing rule as [`Self::pseudo_echo_volume`].
    pseudo_echo_length: u8,
    /// Combined into each new note's priority by
    /// [`Sequencer::note_priority`].
    priority: u8,
    /// Set when a volume input changed since the last
    /// [`Sequencer::propagate_dirty_tracks`] pass (`MPT_FLG_VOLCHG`,
    /// `m4a_internal.h:266`).
    vol_dirty: bool,
    /// [`Self::vol_dirty`]'s counterpart for the pitch-affecting commands
    /// and a pitch-target LFO step (`MPT_FLG_PITCHG`, `m4a_internal.h:268`).
    pitch_dirty: bool,
}

impl TrackState {
    fn new() -> Self {
        Self {
            cursor: 0,
            wait: 0,
            ended: false,
            voice: 0,
            vol: DEFAULT_TRACK_VOLUME,
            vol_x: TRACK_VOLUME_SCALE,
            pan: 0,
            bend: 0,
            bend_range: DEFAULT_BEND_RANGE,
            tune: 0,
            key_shift: 0,
            key: 0,
            lfo_depth: 0,
            modulation_target: ModulationTarget::PITCH,
            lfo_speed: DEFAULT_LFO_SPEED,
            lfo_delay: 0,
            lfo_delay_remaining: 0,
            lfo_phase: 0,
            modulation: 0,
            pattern_return_stack: [0; MAX_PATTERN_DEPTH],
            pattern_depth: 0,
            repeat_counter: 0,
            pseudo_echo_volume: 0,
            pseudo_echo_length: 0,
            priority: 0,
            vol_dirty: false,
            pitch_dirty: false,
        }
    }

    fn reset_lfo(&mut self) {
        self.modulation = 0;
        self.lfo_phase = 0;
    }

    /// Saves the return cursor and calls `target`; upstream ends the track at
    /// the fourth nested `PATT` (`ply_patt`, `m4a_1.s:851`..`:869`).
    fn call_pattern(&mut self, target: usize) -> bool {
        let Some(return_cursor) = self.pattern_return_stack.get_mut(self.pattern_depth) else {
            return false;
        };
        *return_cursor = self.cursor;
        self.pattern_depth += 1;
        self.cursor = target;
        true
    }

    fn return_from_pattern(&mut self) {
        let Some(pattern_depth) = self.pattern_depth.checked_sub(1) else {
            return;
        };
        self.pattern_depth = pattern_depth;
        self.cursor = self.pattern_return_stack[pattern_depth];
    }

    /// Count zero jumps without incrementing; positive counts increment first
    /// and reset after the final pass (`ply_rept`, `m4a_1.s:880`..`:910`).
    fn repeat(&mut self, count: u8, target: usize) {
        if count == 0 {
            self.cursor = target;
            return;
        }

        self.repeat_counter = self.repeat_counter.wrapping_add(1);
        if self.repeat_counter < count {
            self.cursor = target;
        } else {
            self.repeat_counter = 0;
        }
    }
}

/// An owned M4A sequencer + mixer. Construct from a [`Song`], then pull audio
/// one frame at a time with [`Self::render_frame`] (or many frames with
/// [`Self::mix_into`]).
#[derive(Debug)]
pub struct Sequencer {
    song: Song,
    tracks: Vec<TrackState>,
    mixer: Mixer,
    tempo_i: u16,
    tempo_c: u16,
    mem_acc: MemAccArea,
    paused: bool,
}

impl Sequencer {
    /// Interleaved-stereo samples one [`Self::render_frame`] produces
    /// (`SAMPLES_PER_FRAME * 2`).
    pub const FRAME_SAMPLES: usize = SAMPLES_PER_FRAME * 2;

    /// Build a sequencer for `song` with the default mixer configuration.
    #[must_use]
    pub fn new(song: Song) -> Self {
        Self::with_config(song, DEFAULT_MASTER_VOLUME, DEFAULT_MAX_VOICES)
    }

    /// Build a sequencer with an explicit master volume and voice cap.
    ///
    /// Uses `song`'s own header reverb (`Song::reverb`, which collapses "no
    /// override" to `0`); [`Self::with_resolved_reverb`] instead lets a
    /// caller supply a session-carried level for a header that left reverb
    /// unset.
    #[must_use]
    pub fn with_config(song: Song, master_volume: u8, max_voices: usize) -> Self {
        let reverb_level = song.reverb();
        Self::with_resolved_reverb(song, master_volume, max_voices, reverb_level)
    }

    /// Build a sequencer with an explicit master volume, voice cap, and
    /// resolved reverb level, overriding `song`'s own header value (the SET
    /// bit in `SongHeader::reverb`, `m4a_internal.h:12`..`:13`;
    /// `m4a.c:661`..`:662`).
    ///
    /// `reverb_level` clamps to the same `SOUND_MODE_REVERB_VAL` bound
    /// [`Song::with_reverb`] enforces at the header-ingest boundary.
    #[must_use]
    pub fn with_resolved_reverb(
        song: Song,
        master_volume: u8,
        max_voices: usize,
        reverb_level: u8,
    ) -> Self {
        let tracks = (0..song.track_count()).map(|_| TrackState::new()).collect();
        let tempo_i = song.initial_tempo();
        let mixer = Mixer::new(master_volume, max_voices)
            .with_reverb_level(reverb_level.min(MAX_REVERB_LEVEL));
        Self {
            song,
            tracks,
            mixer,
            tempo_i,
            tempo_c: 0,
            mem_acc: MemAccArea::default(),
            paused: false,
        }
    }

    /// Number of voices currently sounding.
    #[must_use]
    pub fn voice_count(&self) -> usize {
        self.mixer.voice_count()
    }

    /// Whether every track has ended, all voices have decayed to silence, and
    /// the master-mix reverb tail has drained.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.tracks.iter().all(|t| t.ended) && !self.is_sounding()
    }

    /// Whether mixing another frame still produces sound with no track
    /// ticking: a voice an ended track left in release, or delayed samples
    /// the master-mix reverb ring still holds.
    #[must_use]
    pub fn is_sounding(&self) -> bool {
        !self.mixer.is_idle() || self.mixer.has_pending_reverb()
    }

    /// Whether a fade's terminal step has paused this sequencer
    /// (`MUSICPLAYER_STATUS_PAUSE`, `m4a.c:740`). A paused sequencer stops
    /// ticking for good; only its mixer keeps running.
    #[must_use]
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Advance the sequencer by one V-blank frame and render its audio into
    /// `out`, which must hold exactly [`Self::FRAME_SAMPLES`] interleaved
    /// stereo `f32`s.
    ///
    /// # Panics
    ///
    /// Panics if `out.len() != Self::FRAME_SAMPLES`.
    pub fn render_frame(&mut self, out: &mut [f32]) {
        self.advance_frame();
        self.mixer.mix_frame(out);
    }

    /// Render whole frames into `out`, whose length must be a multiple of
    /// [`Self::FRAME_SAMPLES`]. Deterministic and device-free: no wall-clock
    /// or host-audio dependency.
    ///
    /// # Panics
    ///
    /// Panics if `out.len()` is not a positive multiple of
    /// [`Self::FRAME_SAMPLES`].
    pub fn mix_into(&mut self, out: &mut [f32]) {
        assert!(
            !out.is_empty() && out.len().is_multiple_of(Self::FRAME_SAMPLES),
            "mix_into length must be a multiple of FRAME_SAMPLES",
        );
        for frame in out.chunks_mut(Self::FRAME_SAMPLES) {
            self.render_frame(frame);
        }
    }

    /// Like [`Self::render_frame`], but stages an active fade's `volX` before
    /// this frame's tick, where upstream's `FadeOutBody` runs
    /// (`m4a_1.s:1152`-`:1169`). The fade raises the same dirty flag as a
    /// `VOL` command (`m4a.c:753`-`:757`), so this tick's `Fine` or `Note`
    /// consumes it and the post-tick propagation pass applies the rest.
    ///
    /// The terminal step (`volX == 0`) instead pauses the sequencer: every
    /// track stops and no tick runs again, while the mixer keeps rendering
    /// the tail.
    pub fn render_frame_with_fade(&mut self, out: &mut [f32], fade_vol_x: Option<u8>) {
        match fade_vol_x {
            Some(0) => self.pause(),
            Some(vol_x) => {
                self.stage_fade_volume(vol_x);
                self.advance_frame();
            }
            None => self.advance_frame(),
        }
        self.mixer.mix_frame(out);
    }

    /// Stops every not-yet-ended track's voices outright and latches
    /// `paused`, matching `TrackStop` (`m4a_1.s:1469`-`:1506`) and
    /// `MUSICPLAYER_STATUS_PAUSE` (`m4a.c:720`-`:740`).
    fn pause(&mut self) {
        if self.paused {
            return;
        }
        self.paused = true;
        let Self { tracks, mixer, .. } = self;
        for (track_id, track) in tracks.iter().enumerate() {
            if !track.ended {
                mixer.stop_track(track_id);
            }
        }
    }

    /// Returns every live track's `volX` to full scale, so a fade the caller
    /// abandoned before its terminal step stops attenuating the song. The
    /// change raises the same dirty flag a fade step does, so the next
    /// rendered frame's post-tick pass applies it to sounding voices. A
    /// sequencer the terminal step already paused stays paused.
    pub fn restore_full_volume(&mut self) {
        self.stage_fade_volume(TRACK_VOLUME_SCALE);
    }

    fn stage_fade_volume(&mut self, vol_x: u8) {
        for track in &mut self.tracks {
            if !track.ended && track.vol_x != vol_x {
                track.vol_x = vol_x;
                track.vol_dirty = true;
            }
        }
    }

    /// Run the tempo accumulator for one frame, firing ticks as it crosses
    /// [`TEMPO_UNIT`] (`m4a_1.s:1169`..`:1359`).
    ///
    /// # Overflow
    ///
    /// `tempo_c += tempo_i` never overflows: the loop below always leaves
    /// `tempo_c < TEMPO_UNIT`, and every `tempo_i` ingestion point
    /// ([`Song::new`]'s initial tempo, [`Event::Tempo`]'s runtime assignment
    /// in [`Self::handle_event`]) clamps to [`MAX_TEMPO_BPM`], so their sum
    /// stays far under `u16::MAX`.
    fn advance_frame(&mut self) {
        if self.paused {
            return;
        }
        debug_assert!(self.tempo_c < TEMPO_UNIT);
        debug_assert!(self.tempo_i <= MAX_TEMPO_BPM);
        self.tempo_c += self.tempo_i;
        while self.tempo_c >= TEMPO_UNIT {
            self.tempo_c -= TEMPO_UNIT;
            self.do_tick();
        }
        // Once per frame after every tick, matching `MPlayMain`'s single
        // pass after its tempo loop (`m4a_1.s:1169`-`:1175`, `:1341`-`:1360`).
        self.propagate_dirty_tracks();
    }

    fn do_tick(&mut self) {
        let Self {
            song,
            tracks,
            mixer,
            tempo_i,
            mem_acc,
            ..
        } = self;
        for (track_id, track) in tracks.iter_mut().enumerate() {
            // Per-track gate expiry precedes that track's commands, as in
            // `MPlayMain`'s per-track chain walk (`m4a_1.s:1191`-`:1212`).
            if !track.ended {
                mixer.tick_gates(track_id);
            }
            Self::process_track(song, track, mixer, track_id, tempo_i, mem_acc);
        }
    }

    /// The deferred recompute behind `MPT_FLG_VOLCHG`/`MPT_FLG_PITCHG`:
    /// pushes each dirty, live track's volume and pitch into its voices and
    /// clears both flags (`m4a_1.s:1361`-`:1441`). A track ended by `Fine`
    /// this frame has no flags left to apply.
    fn propagate_dirty_tracks(&mut self) {
        let Self { tracks, mixer, .. } = self;
        for (track_id, track) in tracks.iter_mut().enumerate() {
            if track.ended {
                continue;
            }
            if track.vol_dirty {
                Self::apply_track_volume(track, mixer, track_id);
                track.vol_dirty = false;
            }
            if track.pitch_dirty {
                Self::apply_track_pitch(track, mixer, track_id);
                track.pitch_dirty = false;
            }
        }
    }

    fn process_track(
        song: &Song,
        track: &mut TrackState,
        mixer: &mut Mixer,
        track_id: usize,
        tempo_i: &mut u16,
        mem_acc: &mut MemAccArea,
    ) {
        if track.ended {
            return;
        }
        let events = &song.tracks()[track_id];

        if track.wait == 0 {
            let mut guard = 0;
            loop {
                guard += 1;
                if guard > MAX_COMMANDS_PER_TICK {
                    Self::finish_track(track, mixer, track_id);
                    break;
                }
                if track.cursor >= events.len() {
                    // `decode_track` allows a stream with no trailing `FINE`.
                    Self::finish_track(track, mixer, track_id);
                    break;
                }
                let event = events[track.cursor].clone();
                track.cursor += 1;
                Self::handle_event(song, track, mixer, track_id, tempo_i, mem_acc, &event);
                if track.wait > 0 || track.ended {
                    break;
                }
            }
        }

        if track.wait > 0 {
            track.wait -= 1;
            // LFO fires every wait-consuming tick, not only when `Wait` was
            // first issued (`m4a_1.s:1279`..`:1330`).
            Self::apply_lfo(track);
        }
    }

    /// Applies one decoded event to its track, shared mixer, or tempo.
    #[allow(clippy::too_many_lines)]
    fn handle_event(
        song: &Song,
        track: &mut TrackState,
        mixer: &mut Mixer,
        track_id: usize,
        tempo_i: &mut u16,
        mem_acc: &mut MemAccArea,
        event: &Event,
    ) {
        match *event {
            Event::Wait(ticks) => track.wait = u16::from(ticks),
            Event::Fine => {
                Self::finish_track(track, mixer, track_id);
            }
            Event::Goto(index) => track.cursor = index,
            Event::Voice(v) => track.voice = usize::from(v),
            Event::Volume(v) => {
                track.vol = v;
                track.vol_dirty = true;
            }
            Event::Pan(p) => {
                track.pan = p;
                track.vol_dirty = true;
            }
            // Also reached from an `Event::Tempo` converted from the
            // asset-pack schema, which round-trips an unbounded on-disk
            // `u16`; re-applying `clamp_tempo` here guards `tempo_c`'s
            // accumulation against a malformed pack even though
            // `decode_track`'s own `TEMPO` arm already clamped it.
            Event::Tempo(bpm) => *tempo_i = clamp_tempo(bpm),
            Event::KeyShift(k) => {
                track.key_shift = k;
                track.pitch_dirty = true;
            }
            Event::Bend(b) => {
                track.bend = b;
                track.pitch_dirty = true;
            }
            Event::BendRange(r) => {
                track.bend_range = r;
                track.pitch_dirty = true;
            }
            Event::Tune(t) => {
                track.tune = t;
                track.pitch_dirty = true;
            }
            Event::Modulation(depth) => {
                track.lfo_depth = depth;
                if depth == 0 {
                    Self::reset_lfo(track);
                }
            }
            // Raises both flags, not just one, since whichever domain the
            // old target drove needs its modulation cleared too
            // (`ply_modt`, `m4a_1.s:1027`-`:1041`).
            Event::ModType(kind) => {
                let target = ModulationTarget::from_command(kind);
                if track.modulation_target != target {
                    track.modulation_target = target;
                    track.vol_dirty = true;
                    track.pitch_dirty = true;
                }
            }
            Event::LfoSpeed(speed) => {
                track.lfo_speed = speed;
                if speed == 0 {
                    Self::reset_lfo(track);
                }
            }
            Event::LfoDelay(delay) => track.lfo_delay = delay,
            // Takes effect only for notes started afterwards; an
            // already-sounding voice keeps the priority it was stamped
            // with (`ply_prio`, `m4a_1.s:912`..`:917`).
            Event::Priority(priority) => track.priority = priority,
            Event::Note {
                key,
                velocity,
                gate,
            } => {
                track.key = key;
                let mut note_track = track.clone();
                if note_track.lfo_delay != 0 {
                    note_track.reset_lfo();
                }
                if Self::note_on(song, &note_track, mixer, track_id, key, velocity, gate) {
                    track.lfo_delay_remaining = track.lfo_delay;
                    if track.lfo_delay != 0 {
                        Self::reset_lfo(track);
                    }
                    // Allocation already applied the current volume and
                    // pitch to the new voice, so both flags are consumed
                    // here rather than left for the next propagation pass
                    // (`m4a_1.s:1802`-`:1805`).
                    track.vol_dirty = false;
                    track.pitch_dirty = false;
                }
            }
            Event::EndOfTie { key } => {
                // An operand becomes the track's new persistent key, not
                // just this call's match target (`ply_endtie`).
                let match_key = match key {
                    Some(k) => {
                        track.key = k;
                        k
                    }
                    None => track.key,
                };
                mixer.note_off_track(track_id, match_key);
            }
            Event::Pattern(target) => {
                if !track.call_pattern(target) {
                    Self::finish_track(track, mixer, track_id);
                }
            }
            Event::PatternEnd => track.return_from_pattern(),
            Event::Xcmd {
                kind: XCMD_IECV,
                value,
            } => track.pseudo_echo_volume = u8::try_from(value).unwrap_or(0),
            Event::Xcmd {
                kind: XCMD_IECL,
                value,
            } => track.pseudo_echo_length = u8::try_from(value).unwrap_or(0),
            Event::Repeat { count, target } => track.repeat(count, target),
            Event::MemAcc {
                op,
                addr,
                value,
                target,
            } => Self::exec_memacc(mem_acc, track, op, addr, value, target),
            // Canonical `mid2agb` songs emit neither `PORT` nor other `XCMD`s
            // (`tools/mid2agb/agb.cpp:338`..`:341`); they remain decode-only.
            Event::Xcmd { .. } | Event::Port { .. } => {}
        }
    }

    fn finish_track(track: &mut TrackState, mixer: &mut Mixer, track_id: usize) {
        mixer.release_track(track_id);
        track.ended = true;
        // `ply_fine` zeroes the whole flags byte (`m4a_1.s:771`-`:773`).
        track.vol_dirty = false;
        track.pitch_dirty = false;
    }

    /// Executes `ply_memacc`'s mutation and conditional-jump table
    /// (`m4a.c:1437`..`:1521`). This port treats invalid operations, cell
    /// addresses, and missing jump targets as safe no-ops.
    fn exec_memacc(
        mem_acc: &mut MemAccArea,
        track: &mut TrackState,
        op: u8,
        addr: u8,
        data: u8,
        target: Option<usize>,
    ) {
        let Some(cell) = mem_acc.read(addr) else {
            return;
        };

        let taken = match op {
            MEMACC_SET => {
                mem_acc.write(addr, data);
                return;
            }
            MEMACC_ADD => {
                mem_acc.write(addr, cell.wrapping_add(data));
                return;
            }
            MEMACC_SUB => {
                mem_acc.write(addr, cell.wrapping_sub(data));
                return;
            }
            MEMACC_COPY => {
                let Some(other) = mem_acc.read(data) else {
                    return;
                };
                mem_acc.write(addr, other);
                return;
            }
            MEMACC_ADD_CELL => {
                let Some(other) = mem_acc.read(data) else {
                    return;
                };
                mem_acc.write(addr, cell.wrapping_add(other));
                return;
            }
            MEMACC_SUB_CELL => {
                let Some(other) = mem_acc.read(data) else {
                    return;
                };
                mem_acc.write(addr, cell.wrapping_sub(other));
                return;
            }
            MEMACC_EQ => cell == data,
            MEMACC_NE => cell != data,
            MEMACC_GT => cell > data,
            MEMACC_GE => cell >= data,
            MEMACC_LE => cell <= data,
            MEMACC_LT => cell < data,
            MEMACC_CELL_EQ => {
                let Some(other) = mem_acc.read(data) else {
                    return;
                };
                cell == other
            }
            MEMACC_CELL_NE => {
                let Some(other) = mem_acc.read(data) else {
                    return;
                };
                cell != other
            }
            MEMACC_CELL_GT => {
                let Some(other) = mem_acc.read(data) else {
                    return;
                };
                cell > other
            }
            MEMACC_CELL_GE => {
                let Some(other) = mem_acc.read(data) else {
                    return;
                };
                cell >= other
            }
            MEMACC_CELL_LE => {
                let Some(other) = mem_acc.read(data) else {
                    return;
                };
                cell <= other
            }
            MEMACC_CELL_LT => {
                let Some(other) = mem_acc.read(data) else {
                    return;
                };
                cell < other
            }
            _ => return,
        };

        if taken {
            if let Some(target) = target {
                track.cursor = target;
            }
        }
    }

    /// `clear_modM`'s counterpart (`m4a_1.s:1859`-`:1874`): zeroes the
    /// modulation state and raises the dirty flag matching whichever domain
    /// it had been driving, deferred like every other volume/pitch input.
    fn reset_lfo(track: &mut TrackState) {
        track.reset_lfo();
        Self::mark_modulation_dirty(track);
    }

    /// Marks whichever domain `track.modulation` currently drives as dirty
    /// (`clear_modM`/`ply_modt`'s own flag choice: pitch for target `0`,
    /// volume otherwise).
    fn mark_modulation_dirty(track: &mut TrackState) {
        if track.modulation_target.is_pitch() {
            track.pitch_dirty = true;
        } else {
            track.vol_dirty = true;
        }
    }

    /// The wait-tail LFO update (`m4a_1.s:1279`..`:1330`): only stores the
    /// new `modM` and marks the driven domain dirty when it actually
    /// changes, deferring the recompute like every other command here.
    fn apply_lfo(track: &mut TrackState) {
        if track.lfo_speed == 0 || track.lfo_depth == 0 {
            return;
        }
        if track.lfo_delay_remaining > 0 {
            track.lfo_delay_remaining -= 1;
            return;
        }

        let phase_sum = u16::from(track.lfo_phase) + u16::from(track.lfo_speed);
        #[allow(clippy::cast_possible_truncation)]
        {
            track.lfo_phase = phase_sum as u8;
        }
        let modulation = scale_lfo(track.lfo_depth, lfo_triangle(phase_sum));
        if modulation == track.modulation {
            return;
        }
        track.modulation = modulation;
        Self::mark_modulation_dirty(track);
    }

    fn apply_track_volume(track: &TrackState, mixer: &mut Mixer, track_id: usize) {
        let (right, left) = track_volume(track);
        mixer.set_track_volume(track_id, right, left);
    }

    fn apply_track_pitch(track: &TrackState, mixer: &mut Mixer, track_id: usize) {
        let (key_offset, fine_adjust) = track_pitch(track);
        mixer.set_track_pitch(track_id, key_offset, fine_adjust);
    }

    /// Allocate a voice for a note, resolving its stereo volume and pitch from
    /// the track's current state (`TrkVolPitSet` + `ChnVolSetAsm`), resolving
    /// any key-split/rhythm indirection to a concrete leaf instrument first,
    /// and dispatching to either a DirectSound or a CGB PSG voice depending
    /// on that leaf's kind.
    // Long by construction, one match arm per instrument kind, not by
    // complexity.
    #[allow(clippy::too_many_lines)]
    fn note_on(
        song: &Song,
        track: &TrackState,
        mixer: &mut Mixer,
        track_id: usize,
        key: u8,
        velocity: u8,
        gate: u8,
    ) -> bool {
        let Some(instrument) = song.voice(track.voice) else {
            return false;
        };
        let Some((instrument, pitch_key, rhythm_pan)) = resolve_instrument(instrument, key) else {
            return false;
        };

        let (vol_mr, vol_ml) = track_volume(track);
        let (key_m, pit_m) = track_pitch(track);
        let (pan_right, pan_left) = pan_terms(rhythm_pan);

        // Floored at 0 (`m4a_1.s:1760`..`:1766`), then wrapped modulo 256 by
        // `MidiKeyToFreq`/`MidiKeyToCgbFreq`'s `u8 key` (`m4a.c:23`, `:810`).
        let note_key =
            u8::try_from((i32::from(pitch_key) + key_m).max(0) & i32::from(u8::MAX)).unwrap_or(0);
        let gate = u16::from(gate);
        let echo_volume = track.pseudo_echo_volume;
        let echo_length = track.pseudo_echo_length;
        let priority = Self::note_priority(song, track);

        match instrument {
            Instrument::DirectSound(tone) => {
                let right = channel_volume(vol_mr, pan_right, velocity);
                let left = channel_volume(vol_ml, pan_left, velocity);
                let freq = pitch::midi_key_to_freq(tone.wave.freq(), note_key, pit_m);
                let voice = Voice::new(
                    tone.wave.clone(),
                    tone.adsr,
                    freq,
                    right,
                    left,
                    velocity,
                    gate,
                    key,
                    track_id,
                    echo_volume,
                    echo_length,
                )
                .with_pitch_key(pitch_key)
                .with_rhythm_pan(rhythm_pan)
                .fixed_rate(tone.is_fixed_rate())
                .with_priority(priority);
                // A refused note simply never sounds -- upstream's `ply_note`
                // returns without touching any channel (`m4a_1.s:1806`).
                mixer.add_voice(voice)
            }
            Instrument::CgbSquare1(sq) => mixer.add_cgb_voice(
                CgbVoice::square_with_fixed_rate(
                    CgbChannelNumber::Square1,
                    sq.duty,
                    Some(sq.sweep),
                    sq.adsr,
                    sq.fixed_rate,
                    note_key,
                    pit_m,
                    vol_mr,
                    vol_ml,
                    velocity,
                    gate,
                    key,
                    track_id,
                    rhythm_pan,
                    echo_volume,
                    echo_length,
                )
                .with_pitch_key(pitch_key)
                .with_priority(priority),
            ),
            Instrument::CgbSquare2(sq) => mixer.add_cgb_voice(
                CgbVoice::square_with_fixed_rate(
                    CgbChannelNumber::Square2,
                    sq.duty,
                    None,
                    sq.adsr,
                    sq.fixed_rate,
                    note_key,
                    pit_m,
                    vol_mr,
                    vol_ml,
                    velocity,
                    gate,
                    key,
                    track_id,
                    rhythm_pan,
                    echo_volume,
                    echo_length,
                )
                .with_pitch_key(pitch_key)
                .with_priority(priority),
            ),
            Instrument::CgbWave(w) => {
                let samples = WaveChannel::decode_wave_ram(&w.table);
                mixer.add_cgb_voice(
                    CgbVoice::wave(
                        samples,
                        w.adsr,
                        w.fixed_rate,
                        note_key,
                        pit_m,
                        vol_mr,
                        vol_ml,
                        velocity,
                        gate,
                        key,
                        track_id,
                        rhythm_pan,
                        echo_volume,
                        echo_length,
                    )
                    .with_pitch_key(pitch_key)
                    .with_priority(priority),
                )
            }
            Instrument::CgbNoise(n) => mixer.add_cgb_voice(
                CgbVoice::noise(
                    n.adsr,
                    note_key,
                    n.lfsr_width_selector,
                    vol_mr,
                    vol_ml,
                    velocity,
                    gate,
                    key,
                    track_id,
                    rhythm_pan,
                    echo_volume,
                    echo_length,
                )
                .with_pitch_key(pitch_key)
                .with_priority(priority),
            ),
            Instrument::KeySplit(_) | Instrument::Rhythm(_) => false,
        }
    }

    /// `ply_note` sums the song and track halves in a wide register and
    /// clamps before storing the byte (`m4a_1.s:1628`..`:1633`).
    fn note_priority(song: &Song, track: &TrackState) -> u8 {
        song.priority().saturating_add(track.priority)
    }
}

/// `ply_note` refuses (rather than recurses into) an indirect key-split or
/// rhythm child (`m4a_1.s:1580`..`:1609`).
fn resolve_instrument(instrument: &Instrument, played_key: u8) -> Option<(&Instrument, u8, i8)> {
    let (leaf, pitch_key, rhythm_pan) = match instrument {
        Instrument::KeySplit(split) => {
            let &child_index = split.table.get(usize::from(played_key))?;
            let leaf = split.children.get(usize::from(child_index))?.as_ref()?;
            (leaf, played_key, 0)
        }
        Instrument::Rhythm(rhythm) => {
            let child = rhythm.children.get(usize::from(played_key))?.as_ref()?;
            (&child.instrument, child.base_key, child.pan.unwrap_or(0))
        }
        leaf => (leaf, played_key, 0),
    };
    if matches!(leaf, Instrument::KeySplit(_) | Instrument::Rhythm(_)) {
        return None;
    }
    Some((leaf, pitch_key, rhythm_pan))
}

fn track_volume(track: &TrackState) -> (u8, u8) {
    let mut volume = (u32::from(track.vol) * u32::from(track.vol_x)) >> 5;
    if track.modulation_target == ModulationTarget::AMPLITUDE {
        let modulation_scale = u32::try_from(i32::from(track.modulation) + 128).unwrap_or(0);
        volume = (volume * modulation_scale) >> 7;
    }

    let mut pan = 2 * i32::from(track.pan);
    if track.modulation_target == ModulationTarget::PAN {
        pan += i32::from(track.modulation);
    }
    let pan = pan.clamp(-128, 127);
    let right = (u32::try_from(pan + 128).unwrap_or(0) * volume) >> 8;
    let left = (u32::try_from(127 - pan).unwrap_or(0) * volume) >> 8;

    // `TrkVolPitSet` stores these wide products into byte fields, so peaks wrap
    // instead of saturating (`m4a.c:787`..`:788`).
    (
        u8::try_from(right & u32::from(u8::MAX)).unwrap_or(0),
        u8::try_from(left & u32::from(u8::MAX)).unwrap_or(0),
    )
}

fn track_pitch(track: &TrackState) -> (i32, u8) {
    let bend = i32::from(track.bend) * i32::from(track.bend_range);
    let mut pitch = (i32::from(track.tune) + bend) * 4 + (i32::from(track.key_shift) << 8);
    if track.modulation_target.is_pitch() {
        pitch += 16 * i32::from(track.modulation);
    }

    // `TrkVolPitSet` stores the key offset and fine adjustment in byte fields
    // (`m4a.c:803`..`:804`); the former is later loaded as signed.
    let key_offset = u8::try_from((pitch >> 8) & i32::from(u8::MAX)).unwrap_or(0);
    let key_offset = i32::from(i8::from_le_bytes([key_offset]));
    let fine_adjust = u8::try_from(pitch & i32::from(u8::MAX)).unwrap_or(0);
    (key_offset, fine_adjust)
}

/// Quarter of the 8-bit LFO phase's wraparound period; the falling half of
/// the triangle wave spans `[LFO_QUARTER_PERIOD, 3 * LFO_QUARTER_PERIOD)`.
const LFO_QUARTER_PERIOD: u8 = 0x40;

/// Half of the 8-bit LFO phase's wraparound period.
const LFO_HALF_PERIOD: i32 = 0x80;

/// `MPlayMain` selects the triangle half from the stored phase byte, but mirrors
/// its falling half against the full pre-store sum (`m4a_1.s:1298`..`:1310`).
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "MPlayMain selects the slope from the stored phase byte"
)]
fn lfo_triangle(phase_sum: u16) -> i32 {
    let stored_phase = phase_sum as u8;
    if (stored_phase.wrapping_sub(LFO_QUARTER_PERIOD) as i8) >= 0 {
        LFO_HALF_PERIOD - i32::from(phase_sum)
    } else {
        i32::from(stored_phase as i8)
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "MPlayMain stores the scaled LFO result through strb"
)]
fn scale_lfo(depth: u8, triangle: i32) -> i8 {
    let scaled = (i32::from(depth) * triangle) >> 6;
    i8::from_ne_bytes([scaled as u32 as u8])
}

#[cfg(test)]
mod cgb_psg_tests;
#[cfg(test)]
mod core_tests;
#[cfg(test)]
mod fixed_rate_tests;
#[cfg(test)]
mod key_split_rhythm_tests;
#[cfg(test)]
mod lfo_tests;
#[cfg(test)]
mod lifecycle_tests;
#[cfg(test)]
mod memacc_tests;
#[cfg(test)]
mod pattern_tests;
#[cfg(test)]
mod pseudo_echo_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod voice_bank_tests;
