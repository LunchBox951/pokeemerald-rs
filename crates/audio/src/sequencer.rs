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

/// Default track volume before any `VOL` command. Upstream leaves
/// `track->vol` at `0`; this crate defaults to full so a minimal
/// hand-authored sequence is audible.
const DEFAULT_TRACK_VOLUME: u8 = 127;

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
        self.mixer.tick_gates();
        let Self {
            song,
            tracks,
            mixer,
            tempo_i,
            mem_acc,
            ..
        } = self;
        for (track_id, track) in tracks.iter_mut().enumerate() {
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
// The reciprocal wave-frequency the test song derives narrows to `u32` (well
// within range for these inputs); silence/pan checks compare exact `0.0`.
#[allow(clippy::cast_possible_truncation, clippy::float_cmp)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::cgb_envelope::CgbAdsr;
    use crate::envelope::Adsr;
    use crate::sample::WaveData;
    use crate::sequence::decode_track;
    use crate::song::{
        rhythm_pan_from_pan_sweep, KeySplit, NoiseTone, Rhythm, RhythmChild, SquareTone, ToneData,
        WaveTone, KEY_SLOTS,
    };

    fn unity_freq() -> u32 {
        (1 << pitch::FRAC_BITS) / pitch::DIV_FREQ
    }

    /// Reference key this test module builds voices around.
    const TEST_REFERENCE_KEY: u8 = 60;

    /// Shift used to both probe and invert [`pitch::midi_key_to_freq`]'s
    /// fixed-point ratio in [`test_song`], trading ratio precision for
    /// staying within its public interface.
    const FREQUENCY_PROBE_SHIFT: u32 = 20;

    /// A voicegroup with one loud, long, flat-envelope instrument at unity
    /// pitch for [`TEST_REFERENCE_KEY`].
    fn test_song(tracks: Vec<Vec<Event>>, tempo: u16) -> Song {
        let target = unity_freq();
        let ratio60 = pitch::midi_key_to_freq(1 << FREQUENCY_PROBE_SHIFT, TEST_REFERENCE_KEY, 0);
        let freq = ((u64::from(target) << FREQUENCY_PROBE_SHIFT) / u64::from(ratio60)) as u32;
        let wave = Arc::new(WaveData::one_shot(freq, vec![100; SAMPLES_PER_FRAME * 4]));
        let voices = vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))];
        Song::new(voices, tracks, tempo)
    }

    fn apply_test_event(seq: &mut Sequencer, track_id: usize, event: &Event) {
        let Sequencer {
            song,
            tracks,
            mixer,
            tempo_i,
            mem_acc,
            ..
        } = seq;
        Sequencer::handle_event(
            song,
            &mut tracks[track_id],
            mixer,
            track_id,
            tempo_i,
            mem_acc,
            event,
        );
    }

    fn tied_note(key: u8) -> Event {
        Event::Note {
            key,
            velocity: 127,
            gate: 0,
        }
    }

    fn render_frames(seq: &mut Sequencer, frames: usize) {
        let mut output = vec![0.0; Sequencer::FRAME_SAMPLES];
        for _ in 0..frames {
            seq.render_frame(&mut output);
        }
    }

    #[test]
    fn new_uses_emerald_init_defaults() {
        // `m4aSoundInit` reconfigures the driver away from the generic
        // `SoundInit` placeholders (`m4a.c:78`..`:81`).
        assert_eq!(DEFAULT_MASTER_VOLUME, 12);
        assert_eq!(DEFAULT_MAX_VOICES, 5);
        let seq = Sequencer::new(test_song(vec![vec![Event::Fine]], 150));
        assert_eq!(seq.mixer.master_volume(), 12);
        assert_eq!(seq.mixer.max_voices(), 5);
    }

    #[test]
    fn track_state_new_uses_the_documented_defaults() {
        let track = TrackState::new();
        assert_eq!(track.vol, DEFAULT_TRACK_VOLUME);
        assert_eq!(track.bend_range, DEFAULT_BEND_RANGE);
        assert_eq!(track.lfo_speed, DEFAULT_LFO_SPEED);
        assert_eq!(track.pan, 0);
        assert_eq!(track.priority, 0);
        assert!(!track.ended);
    }

    #[test]
    fn silent_song_renders_zero() {
        let song = test_song(vec![vec![Event::Fine]], 150);
        let mut seq = Sequencer::new(song);
        let mut out = vec![7.0; Sequencer::FRAME_SAMPLES];
        seq.render_frame(&mut out);
        assert!(out.iter().all(|&s| s == 0.0));
        assert!(seq.is_finished());
    }

    #[test]
    fn a_note_produces_sound_then_the_track_ends() {
        let track = vec![
            Event::Voice(0),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 4,
            },
            Event::Wait(48),
            Event::Fine,
        ];
        let song = test_song(vec![track], 150);
        let mut seq = Sequencer::new(song);

        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        seq.render_frame(&mut out);
        assert!(out.iter().any(|&s| s.abs() > 0.0));
        assert_eq!(seq.voice_count(), 1);

        for _ in 0..64 {
            seq.render_frame(&mut out);
        }
        assert!(
            seq.is_finished(),
            "the wait must drain and FINE must end the track"
        );
    }

    #[test]
    fn finite_reverbed_song_finishes_only_after_tail_drains() {
        let track = vec![
            Event::Voice(0),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 1,
            },
            Event::Wait(2),
            Event::Fine,
        ];
        let song = test_song(vec![track], 150).with_reverb(100);
        let mut seq = Sequencer::new(song);
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];

        for _ in 0..32 {
            seq.render_frame(&mut out);
            if seq.tracks.iter().all(|track| track.ended) && seq.mixer.is_idle() {
                break;
            }
        }

        assert!(seq.tracks.iter().all(|track| track.ended));
        assert!(seq.mixer.is_idle());
        assert!(
            seq.mixer.has_pending_reverb(),
            "the dry note must leave delayed samples in the reverb ring"
        );
        assert!(
            !seq.is_finished(),
            "ended tracks and inactive voices are not finished while reverb is pending"
        );

        let mut heard_wet_tail = false;
        for _ in 0..1000 {
            seq.render_frame(&mut out);
            heard_wet_tail |= out.iter().any(|&sample| sample != 0.0);
            if seq.is_finished() {
                break;
            }
        }

        assert!(heard_wet_tail, "the pending reverb must produce wet output");
        assert!(
            seq.is_finished(),
            "a finite reverb tail must eventually decay to silence"
        );
        assert!(!seq.mixer.has_pending_reverb());
    }

    #[test]
    fn with_resolved_reverb_applies_its_explicit_level_over_the_songs_own_header() {
        // Carries a session's previously configured reverb across a header
        // that never set one (`Song::reverb` collapses that case to `0`).
        let track = vec![
            Event::Voice(0),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 1,
            },
            Event::Wait(2),
            Event::Fine,
        ];
        let song = test_song(vec![track], 150);
        assert_eq!(song.reverb_override(), None);
        let mut seq =
            Sequencer::with_resolved_reverb(song, DEFAULT_MASTER_VOLUME, DEFAULT_MAX_VOICES, 100);
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];

        for _ in 0..32 {
            seq.render_frame(&mut out);
            if seq.tracks.iter().all(|track| track.ended) && seq.mixer.is_idle() {
                break;
            }
        }

        assert!(
            seq.mixer.has_pending_reverb(),
            "the resolved reverb level must be the one actually applied to the mixer"
        );
    }

    #[test]
    fn gate_time_releases_the_note() {
        let track = vec![
            Event::Voice(0),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 2,
            },
            Event::Wait(64),
            Event::Fine,
        ];
        let song = test_song(vec![track], 150);
        let mut seq = Sequencer::new(song);
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];

        seq.render_frame(&mut out);
        assert_eq!(
            seq.voice_count(),
            1,
            "the note must start before its gate can release it"
        );

        // Tempo 150 ticks once per frame, so gate 2 releases well within 7.
        for _ in 0..7 {
            seq.render_frame(&mut out);
        }
        assert_eq!(seq.voice_count(), 0);
    }

    #[test]
    fn goto_loops_the_track_forever() {
        let track = vec![
            Event::Voice(0),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 1,
            },
            Event::Wait(24),
            Event::Goto(1),
        ];
        let song = test_song(vec![track], 150);
        let mut seq = Sequencer::new(song);
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        for _ in 0..200 {
            seq.render_frame(&mut out);
        }
        assert!(!seq.is_finished());
    }

    #[test]
    fn wait_goto_and_voice_commands_update_track_control_state() {
        let mut sequencer = Sequencer::new(test_song(vec![vec![Event::Fine]], 150));

        apply_test_event(&mut sequencer, 0, &Event::Wait(24));
        apply_test_event(&mut sequencer, 0, &Event::Goto(17));
        apply_test_event(&mut sequencer, 0, &Event::Voice(3));

        assert_eq!(sequencer.tracks[0].wait, 24);
        assert_eq!(sequencer.tracks[0].cursor, 17);
        assert_eq!(sequencer.tracks[0].voice, 3);
    }

    #[test]
    fn decoded_only_port_and_xcmd_leave_track_state_unchanged() {
        let mut sequencer = Sequencer::new(test_song(vec![vec![Event::Fine]], 150));
        let original = sequencer.tracks[0].clone();

        apply_test_event(
            &mut sequencer,
            0,
            &Event::Port {
                control: 2,
                value: 127,
            },
        );
        apply_test_event(
            &mut sequencer,
            0,
            &Event::Xcmd {
                kind: 0,
                value: 127,
            },
        );

        assert_eq!(sequencer.tracks[0], original);
    }

    // --- MEMACC --------------------------------------------------------

    const MEMACC_BRANCH_TARGET: usize = 37;

    fn apply_memacc(mem_acc: &mut MemAccArea, op: u8, address: u8, operand: u8) {
        Sequencer::exec_memacc(mem_acc, &mut TrackState::new(), op, address, operand, None);
    }

    fn memacc_branch_taken(mem_acc: &mut MemAccArea, op: u8, address: u8, operand: u8) -> bool {
        let mut track = TrackState::new();
        Sequencer::exec_memacc(
            mem_acc,
            &mut track,
            op,
            address,
            operand,
            Some(MEMACC_BRANCH_TARGET),
        );
        track.cursor == MEMACC_BRANCH_TARGET
    }

    // `decode_track` hands `ply_memacc`'s operation byte through unchanged
    // (`sequence.rs:343`..`:346`), so the two tests below drive raw `0..=17`
    // literals: they pin the wire numbering that the `MEMACC_*` constants
    // only name.

    #[test]
    fn raw_memacc_opcodes_select_the_upstream_mutations() {
        let cases = [
            (0_u8, 3_u8, 3_u8, "mem_set cell0 = 3"),
            (1, 3, 13, "mem_add cell0 += 3"),
            (2, 3, 7, "mem_sub cell0 -= 3"),
            (3, 1, 3, "mem_mem_set cell0 = cell1"),
            (4, 1, 13, "mem_mem_add cell0 += cell1"),
            (5, 1, 7, "mem_mem_sub cell0 -= cell1"),
        ];

        for (raw_op, operand, expected_cell_0, operation) in cases {
            let mut mem_acc = MemAccArea::default();
            mem_acc.write(0, 10);
            mem_acc.write(1, 3);

            apply_memacc(&mut mem_acc, raw_op, 0, operand);

            assert_eq!(
                mem_acc.read(0),
                Some(expected_cell_0),
                "raw opcode {raw_op}: {operation}"
            );
        }
    }

    #[test]
    fn raw_memacc_opcodes_select_the_upstream_comparisons() {
        const CELL_0: u8 = 10;
        const OPERANDS: [u8; 3] = [5, 10, 11];

        // Upstream numbers the six orderings `6..=11` against a literal and
        // repeats them at `12..=17` against another cell. Each row lists
        // whether the branch is taken for each of `OPERANDS`.
        let cases = [
            (6_u8, 12_u8, [false, true, false], "=="),
            (7, 13, [true, false, true], "!="),
            (8, 14, [true, false, false], ">"),
            (9, 15, [true, true, false], ">="),
            (10, 16, [false, true, true], "<="),
            (11, 17, [false, false, true], "<"),
        ];

        for (literal_op, cell_op, taken_per_operand, ordering) in cases {
            for (operand, expected) in OPERANDS.into_iter().zip(taken_per_operand) {
                let mut mem_acc = MemAccArea::default();
                mem_acc.write(0, CELL_0);
                mem_acc.write(1, operand);

                assert_eq!(
                    memacc_branch_taken(&mut mem_acc, literal_op, 0, operand),
                    expected,
                    "raw opcode {literal_op}: {CELL_0} {ordering} {operand}"
                );
                assert_eq!(
                    memacc_branch_taken(&mut mem_acc, cell_op, 0, 1),
                    expected,
                    "raw opcode {cell_op}: {CELL_0} {ordering} cell holding {operand}"
                );
            }
        }
    }

    #[test]
    fn memacc_literal_mutations_set_and_wrap_at_u8_bounds() {
        let mut mem_acc = MemAccArea::default();

        apply_memacc(&mut mem_acc, MEMACC_SET, 0, 250);
        assert_eq!(mem_acc.read(0), Some(250));

        apply_memacc(&mut mem_acc, MEMACC_ADD, 0, 10);
        assert_eq!(mem_acc.read(0), Some(4));

        apply_memacc(&mut mem_acc, MEMACC_SUB, 0, 10);
        assert_eq!(mem_acc.read(0), Some(250));
    }

    #[test]
    fn memacc_cell_mutations_copy_and_wrap_at_u8_bounds() {
        let mut mem_acc = MemAccArea::default();
        apply_memacc(&mut mem_acc, MEMACC_SET, 1, 250);

        apply_memacc(&mut mem_acc, MEMACC_COPY, 0, 1);
        assert_eq!(mem_acc.read(0), Some(250));

        apply_memacc(&mut mem_acc, MEMACC_SET, 0, 10);
        apply_memacc(&mut mem_acc, MEMACC_ADD_CELL, 0, 1);
        assert_eq!(mem_acc.read(0), Some(4));

        apply_memacc(&mut mem_acc, MEMACC_SET, 0, 3);
        apply_memacc(&mut mem_acc, MEMACC_SUB_CELL, 0, 1);
        assert_eq!(mem_acc.read(0), Some(9));
    }

    #[test]
    fn memacc_literal_comparisons_cover_every_ordering() {
        let cases = [
            (MEMACC_EQ, 11, false, "10 == 11"),
            (MEMACC_EQ, 10, true, "10 == 10"),
            (MEMACC_EQ, 5, false, "10 == 5"),
            (MEMACC_NE, 11, true, "10 != 11"),
            (MEMACC_NE, 10, false, "10 != 10"),
            (MEMACC_NE, 5, true, "10 != 5"),
            (MEMACC_GT, 11, false, "10 > 11"),
            (MEMACC_GT, 10, false, "10 > 10"),
            (MEMACC_GT, 5, true, "10 > 5"),
            (MEMACC_GE, 11, false, "10 >= 11"),
            (MEMACC_GE, 10, true, "10 >= 10"),
            (MEMACC_GE, 5, true, "10 >= 5"),
            (MEMACC_LE, 11, true, "10 <= 11"),
            (MEMACC_LE, 10, true, "10 <= 10"),
            (MEMACC_LE, 5, false, "10 <= 5"),
            (MEMACC_LT, 11, true, "10 < 11"),
            (MEMACC_LT, 10, false, "10 < 10"),
            (MEMACC_LT, 5, false, "10 < 5"),
        ];
        let mut mem_acc = MemAccArea::default();
        apply_memacc(&mut mem_acc, MEMACC_SET, 0, 10);

        for (op, operand, expected, expression) in cases {
            assert_eq!(
                memacc_branch_taken(&mut mem_acc, op, 0, operand),
                expected,
                "{expression}"
            );
        }
    }

    #[test]
    fn memacc_cell_comparisons_cover_every_ordering() {
        let cases = [
            (MEMACC_CELL_EQ, 11, false, "10 == 11"),
            (MEMACC_CELL_EQ, 10, true, "10 == 10"),
            (MEMACC_CELL_EQ, 5, false, "10 == 5"),
            (MEMACC_CELL_NE, 11, true, "10 != 11"),
            (MEMACC_CELL_NE, 10, false, "10 != 10"),
            (MEMACC_CELL_NE, 5, true, "10 != 5"),
            (MEMACC_CELL_GT, 11, false, "10 > 11"),
            (MEMACC_CELL_GT, 10, false, "10 > 10"),
            (MEMACC_CELL_GT, 5, true, "10 > 5"),
            (MEMACC_CELL_GE, 11, false, "10 >= 11"),
            (MEMACC_CELL_GE, 10, true, "10 >= 10"),
            (MEMACC_CELL_GE, 5, true, "10 >= 5"),
            (MEMACC_CELL_LE, 11, true, "10 <= 11"),
            (MEMACC_CELL_LE, 10, true, "10 <= 10"),
            (MEMACC_CELL_LE, 5, false, "10 <= 5"),
            (MEMACC_CELL_LT, 11, true, "10 < 11"),
            (MEMACC_CELL_LT, 10, false, "10 < 10"),
            (MEMACC_CELL_LT, 5, false, "10 < 5"),
        ];

        for (op, other, expected, expression) in cases {
            let mut mem_acc = MemAccArea::default();
            apply_memacc(&mut mem_acc, MEMACC_SET, 0, 10);
            apply_memacc(&mut mem_acc, MEMACC_SET, 1, other);
            assert_eq!(
                memacc_branch_taken(&mut mem_acc, op, 0, 1),
                expected,
                "{expression}"
            );
        }
    }

    #[test]
    fn memacc_out_of_range_destination_is_a_no_op() {
        let mut mem_acc = MemAccArea::default();

        for op in [MEMACC_SET, MEMACC_ADD, MEMACC_SUB] {
            apply_memacc(&mut mem_acc, op, 200, 42);
        }

        assert_eq!(mem_acc, MemAccArea::default());
        assert!(!memacc_branch_taken(&mut mem_acc, MEMACC_EQ, 200, 0));
    }

    #[test]
    fn memacc_out_of_range_source_is_a_no_op_or_false_comparison() {
        let mut mem_acc = MemAccArea::default();
        apply_memacc(&mut mem_acc, MEMACC_SET, 0, 42);
        let original = mem_acc.clone();

        for op in [MEMACC_COPY, MEMACC_ADD_CELL, MEMACC_SUB_CELL] {
            apply_memacc(&mut mem_acc, op, 0, 200);
            assert_eq!(mem_acc, original);
        }

        for op in [
            MEMACC_CELL_EQ,
            MEMACC_CELL_NE,
            MEMACC_CELL_GT,
            MEMACC_CELL_GE,
            MEMACC_CELL_LE,
            MEMACC_CELL_LT,
        ] {
            assert!(!memacc_branch_taken(&mut mem_acc, op, 0, 200));
        }
    }

    #[test]
    fn memacc_unknown_operation_is_a_no_op() {
        for op in [18, u8::MAX] {
            let mut mem_acc = MemAccArea::default();
            apply_memacc(&mut mem_acc, MEMACC_SET, 0, 42);
            let original = mem_acc.clone();

            apply_memacc(&mut mem_acc, op, 0, 7);

            assert_eq!(mem_acc, original, "operation {op}");
        }
    }

    #[test]
    fn memacc_taken_branch_without_a_target_falls_through() {
        let mut mem_acc = MemAccArea::default();
        apply_memacc(&mut mem_acc, MEMACC_SET, 0, 42);
        let mut track = TrackState::new();

        Sequencer::exec_memacc(&mut mem_acc, &mut track, MEMACC_EQ, 0, 42, None);

        assert_eq!(track.cursor, 0);
    }

    #[test]
    fn memacc_cells_are_private_to_each_sequencer() {
        let song = || test_song(vec![vec![Event::Fine]], 150);
        let mut first = Sequencer::new(song());
        apply_test_event(
            &mut first,
            0,
            &Event::MemAcc {
                op: MEMACC_SET,
                addr: 0,
                value: 42,
                target: None,
            },
        );
        let mut output = vec![0.0; Sequencer::FRAME_SAMPLES];
        first.render_frame(&mut output);
        assert!(first.is_finished());
        assert_eq!(first.mem_acc.read(0), Some(42));

        // Built after `first` already wrote a cell, so a shared area would leak here.
        let second = Sequencer::new(song());

        assert_eq!(second.mem_acc.read(0), Some(0));
    }

    /// Builds `prelude`, then `[loop:] Voice, Note, Wait, MemAcc -> loop`
    /// followed by `Fine`, and renders it: a taken branch loops the track
    /// forever, a fall-through reaches `Fine`.
    fn memacc_track_loops_forever(prelude: Vec<Event>, op: u8, address: u8, operand: u8) -> bool {
        let loop_target = prelude.len() + 1;
        let mut track = prelude;
        track.push(Event::Voice(0));
        track.push(Event::Note {
            key: 60,
            velocity: 127,
            gate: 1,
        });
        track.push(Event::Wait(24));
        track.push(Event::MemAcc {
            op,
            addr: address,
            value: operand,
            target: Some(loop_target),
        });
        track.push(Event::Fine);

        let mut sequencer = Sequencer::new(test_song(vec![track], 150));
        let mut output = vec![0.0; Sequencer::FRAME_SAMPLES];
        for _ in 0..200 {
            sequencer.render_frame(&mut output);
        }
        !sequencer.is_finished()
    }

    #[test]
    fn memacc_mutates_and_branches_through_the_rendered_track() {
        let set_cell_0_to_5 = || {
            vec![Event::MemAcc {
                op: MEMACC_SET,
                addr: 0,
                value: 5,
                target: None,
            }]
        };

        assert!(
            memacc_track_loops_forever(set_cell_0_to_5(), MEMACC_EQ, 0, 5),
            "cell 0 holds 5, so the equal branch loops the rendered track forever"
        );
        assert!(
            !memacc_track_loops_forever(set_cell_0_to_5(), MEMACC_EQ, 0, 6),
            "cell 0 holds 5, so the unequal branch falls through to Fine"
        );
    }

    /// An unconditional `MEMACC` writing `addr`, for use as a prelude.
    fn memacc_set(addr: u8, value: u8) -> Event {
        Event::MemAcc {
            op: MEMACC_SET,
            addr,
            value,
            target: None,
        }
    }

    #[test]
    fn memacc_mutations_take_effect_through_the_rendered_track() {
        let mutate = |op, addr, value| Event::MemAcc {
            op,
            addr,
            value,
            target: None,
        };
        let cases = [
            (
                vec![memacc_set(0, 250), mutate(MEMACC_ADD, 0, 10)],
                4_u8,
                "mem_add wraps past 255",
            ),
            (
                vec![memacc_set(0, 3), mutate(MEMACC_SUB, 0, 10)],
                249,
                "mem_sub wraps below 0",
            ),
            (
                vec![memacc_set(1, 9), mutate(MEMACC_COPY, 0, 1)],
                9,
                "mem_mem_set copies cell 1",
            ),
            (
                vec![
                    memacc_set(0, 10),
                    memacc_set(1, 250),
                    mutate(MEMACC_ADD_CELL, 0, 1),
                ],
                4,
                "mem_mem_add wraps past 255",
            ),
            (
                vec![
                    memacc_set(0, 3),
                    memacc_set(1, 250),
                    mutate(MEMACC_SUB_CELL, 0, 1),
                ],
                9,
                "mem_mem_sub wraps below 0",
            ),
        ];

        for (prelude, expected_cell_0, operation) in cases {
            let neighbour = expected_cell_0.wrapping_add(1);
            assert!(
                memacc_track_loops_forever(prelude.clone(), MEMACC_EQ, 0, expected_cell_0),
                "{operation}: cell 0 should hold {expected_cell_0}"
            );
            assert!(
                !memacc_track_loops_forever(prelude, MEMACC_EQ, 0, neighbour),
                "{operation}: cell 0 should not hold {neighbour}"
            );
        }
    }

    #[test]
    fn memacc_comparisons_branch_through_the_rendered_track() {
        const CELL_0: u8 = 10;
        const OPERANDS: [u8; 3] = [5, 10, 11];

        let cases = [
            (MEMACC_EQ, MEMACC_CELL_EQ, [false, true, false], "=="),
            (MEMACC_NE, MEMACC_CELL_NE, [true, false, true], "!="),
            (MEMACC_GT, MEMACC_CELL_GT, [true, false, false], ">"),
            (MEMACC_GE, MEMACC_CELL_GE, [true, true, false], ">="),
            (MEMACC_LE, MEMACC_CELL_LE, [false, true, true], "<="),
            (MEMACC_LT, MEMACC_CELL_LT, [false, false, true], "<"),
        ];

        for (literal_op, cell_op, taken_per_operand, ordering) in cases {
            for (operand, expected) in OPERANDS.into_iter().zip(taken_per_operand) {
                assert_eq!(
                    memacc_track_loops_forever(vec![memacc_set(0, CELL_0)], literal_op, 0, operand),
                    expected,
                    "{CELL_0} {ordering} {operand}"
                );
                assert_eq!(
                    memacc_track_loops_forever(
                        vec![memacc_set(0, CELL_0), memacc_set(1, operand)],
                        cell_op,
                        0,
                        1
                    ),
                    expected,
                    "{CELL_0} {ordering} cell holding {operand}"
                );
            }
        }
    }

    #[test]
    fn memacc_out_of_range_cells_stay_safe_through_the_rendered_track() {
        const PAST_THE_AREA: u8 = 200;

        assert!(
            memacc_track_loops_forever(vec![memacc_set(PAST_THE_AREA, 42)], MEMACC_EQ, 0, 0),
            "a write past the area must corrupt no real cell"
        );
        assert!(
            !memacc_track_loops_forever(Vec::new(), MEMACC_EQ, PAST_THE_AREA, 0),
            "a comparison addressed past the area must fall through to Fine"
        );
        assert!(
            memacc_track_loops_forever(
                vec![Event::MemAcc {
                    op: MEMACC_COPY,
                    addr: 0,
                    value: PAST_THE_AREA,
                    target: None,
                }],
                MEMACC_EQ,
                0,
                0
            ),
            "a copy sourced past the area must leave cell 0 zeroed"
        );

        for op in [
            MEMACC_CELL_EQ,
            MEMACC_CELL_NE,
            MEMACC_CELL_GT,
            MEMACC_CELL_GE,
            MEMACC_CELL_LE,
            MEMACC_CELL_LT,
        ] {
            assert!(
                !memacc_track_loops_forever(vec![memacc_set(0, 10)], op, 0, PAST_THE_AREA),
                "operation {op} sourced past the area must fall through to Fine"
            );
        }
    }

    #[test]
    fn memacc_unknown_operation_is_a_no_op_through_the_rendered_track() {
        for op in [18, u8::MAX] {
            let prelude = vec![Event::MemAcc {
                op,
                addr: 0,
                value: 42,
                target: None,
            }];
            assert!(
                memacc_track_loops_forever(prelude, MEMACC_EQ, 0, 0),
                "operation {op} must leave cell 0 zeroed"
            );
        }
    }

    #[test]
    fn memacc_event_dispatches_a_taken_conditional_jump() {
        let mut sequencer = Sequencer::new(test_song(vec![vec![Event::Fine]], 150));

        apply_test_event(
            &mut sequencer,
            0,
            &Event::MemAcc {
                op: MEMACC_EQ,
                addr: 0,
                value: 0,
                target: Some(MEMACC_BRANCH_TARGET),
            },
        );

        assert_eq!(sequencer.tracks[0].cursor, MEMACC_BRANCH_TARGET);
    }

    #[test]
    fn faster_tempo_reaches_the_end_in_fewer_frames() {
        let track = || {
            vec![
                Event::Voice(0),
                Event::Note {
                    key: 60,
                    velocity: 127,
                    gate: 8,
                },
                Event::Wait(12),
                Event::Fine,
            ]
        };
        let frames_to_finish = |tempo: u16| {
            let mut seq = Sequencer::new(test_song(vec![track()], tempo));
            let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
            let mut frames = 0;
            while !seq.is_finished() && frames < 1000 {
                seq.render_frame(&mut out);
                frames += 1;
            }
            frames
        };
        assert!(frames_to_finish(300) < frames_to_finish(150));
    }

    #[test]
    fn advance_frame_ticks_only_on_the_frame_that_crosses_tempo_unit() {
        let track = vec![Event::Wait(5), Event::Fine];
        let mut seq = Sequencer::new(test_song(vec![track], 100));

        seq.advance_frame();
        assert_eq!(seq.tempo_c, 100, "one frame below TEMPO_UNIT must not tick");
        assert_eq!(
            seq.tracks[0].wait, 0,
            "no tick fired, so the track hasn't run yet"
        );

        seq.advance_frame();
        assert_eq!(
            seq.tempo_c, 50,
            "the crossing frame ticks once, dropping tempo_c by TEMPO_UNIT"
        );
        assert_eq!(
            seq.tracks[0].wait, 4,
            "the same tick decoded Wait(5) and consumed one unit"
        );
    }

    #[test]
    fn tempo_event_is_clamped_before_it_can_overflow_the_accumulator() {
        // `tempo_c += tempo_i` (`advance_frame`) is unguarded, so a
        // malformed asset pack's out-of-domain `Event::Tempo` must never
        // reach `tempo_i` un-clamped.
        let mut seq = Sequencer::new(test_song(vec![vec![Event::Fine]], 150));
        apply_test_event(&mut seq, 0, &Event::Tempo(u16::MAX));
        assert_eq!(
            seq.tempo_i, MAX_TEMPO_BPM,
            "an out-of-domain Tempo event must clamp to the TEMPO command's real bound"
        );

        // Drive tempo_c to the highest value `advance_frame`'s drain loop
        // ever leaves behind and take one more frame: an unclamped
        // `tempo_i` of `u16::MAX` would overflow this addition, but the
        // clamp above keeps the sum far under `u16::MAX`.
        seq.tempo_c = TEMPO_UNIT - 1;
        seq.advance_frame();
        assert_eq!(seq.tempo_c, (TEMPO_UNIT - 1 + MAX_TEMPO_BPM) % TEMPO_UNIT);
    }

    /// [`Event::Pan`]'s range minimum.
    const HARD_LEFT_PAN: i8 = -64;

    #[test]
    fn panned_note_is_louder_on_one_side() {
        let track = vec![
            Event::Voice(0),
            Event::Pan(HARD_LEFT_PAN),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 8,
            },
            Event::Wait(48),
            Event::Fine,
        ];
        let song = test_song(vec![track], 150);
        let mut seq = Sequencer::new(song);
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        seq.render_frame(&mut out);
        let left: f32 = out.iter().step_by(2).map(|s| s.abs()).sum();
        let right: f32 = out.iter().skip(1).step_by(2).map(|s| s.abs()).sum();
        assert!(left > right, "left {left} should exceed right {right}");
        assert_eq!(right, 0.0);
    }

    #[test]
    fn decoded_bytes_drive_the_engine_end_to_end() {
        // Decode a real byte program and play it: VOICE 0; N04 key60 vel127;
        // W48; FINE.
        let bytes = [0xBD, 0x00, 0xD3, 60, 127, 0xB0, 0xB1];
        let events = decode_track(&bytes).unwrap();
        let song = test_song(vec![events], 150);
        let mut seq = Sequencer::new(song);
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        seq.render_frame(&mut out);
        assert!(out.iter().any(|&s| s.abs() > 0.0));
    }

    #[test]
    fn decoded_pattern_notes_inherit_each_callers_key() {
        let bytes = [
            0xBD, 0, 0xCF, 60, 127, 0xB3, 19, 0, 0, 0, 0xCF, 72, 127, 0xB3, 19, 0, 0, 0, 0xB0,
            0xCF, 0xB4,
        ];
        let events = decode_track(&bytes).unwrap();
        let wave = Arc::new(WaveData::looping(0, 0, vec![100]));
        let voices = vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))];
        let mut sequencer = Sequencer::new(Song::new(voices, vec![events], 150));
        let mut output = vec![0.0; Sequencer::FRAME_SAMPLES];
        sequencer.render_frame(&mut output);

        let mut keys: Vec<_> = sequencer
            .mixer
            .voices()
            .iter()
            .map(|voice| voice.midi_key())
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, vec![60, 60, 72, 72]);
    }

    #[test]
    fn decoded_memory_branch_preserves_each_runtime_paths_note() {
        let bytes = [
            0xBD, 0, 0xCF, 60, 127, 0xB9, 6, 0, 1, 16, 0, 0, 0, 0xCF, 72, 127, 0xCF, 0xB0, 0xB1,
        ];
        let events = decode_track(&bytes).unwrap();
        for (memory, expected) in [(0, vec![60, 72, 72]), (1, vec![60, 60])] {
            let wave = Arc::new(WaveData::looping(0, 0, vec![100]));
            let voices = vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))];
            let mut sequencer = Sequencer::new(Song::new(voices, vec![events.clone()], 150));
            sequencer.mem_acc.write(0, memory);
            let mut output = vec![0.0; Sequencer::FRAME_SAMPLES];
            sequencer.render_frame(&mut output);

            let mut keys: Vec<_> = sequencer
                .mixer
                .voices()
                .iter()
                .map(|voice| voice.midi_key())
                .collect();
            keys.sort_unstable();
            assert_eq!(keys, expected, "memory cell = {memory}");
        }
    }

    #[test]
    fn mix_into_renders_multiple_frames() {
        let track = vec![
            Event::Voice(0),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 90,
            },
            Event::Wait(96),
            Event::Fine,
        ];
        let song = test_song(vec![track], 150);
        let mut seq = Sequencer::new(song);
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES * 3];
        seq.mix_into(&mut out);
        for (i, frame) in out.chunks(Sequencer::FRAME_SAMPLES).enumerate() {
            assert!(
                frame.iter().any(|&s| s.abs() > 0.0),
                "frame {i} of 3 must carry the held note's audio"
            );
        }
    }

    /// A song whose instrument reads a wave at frequency `0`, so a tied voice
    /// never advances through the sample and only stops on an explicit
    /// note-off — isolating end-of-tie behaviour from wave exhaustion.
    fn held_note_song(track: Vec<Event>) -> Song {
        let wave = Arc::new(WaveData::one_shot(0, vec![100; SAMPLES_PER_FRAME]));
        let voices = vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))];
        Song::new(voices, vec![track], 150)
    }

    #[test]
    fn end_of_tie_without_operand_stops_only_the_last_keyed_note() {
        // The omitted-operand `EOT` must resolve to key 64 (the current
        // key), not 60 and not every sounding voice.
        let track = vec![
            Event::Voice(0),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 0,
            },
            Event::Note {
                key: 64,
                velocity: 127,
                gate: 0,
            },
            Event::EndOfTie { key: None },
            Event::Wait(2),
            Event::EndOfTie { key: Some(60) },
            Event::Wait(2),
            Event::Fine,
        ];
        let mut seq = Sequencer::new(held_note_song(track));
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];

        seq.render_frame(&mut out);
        assert_eq!(
            seq.voice_count(),
            1,
            "only the last-keyed note (64) retires"
        );

        for _ in 0..4 {
            seq.render_frame(&mut out);
        }
        assert_eq!(seq.voice_count(), 0, "the key-60 survivor now retires too");
    }

    #[test]
    fn fine_releases_a_tied_voice_and_the_song_finishes() {
        let track = vec![
            Event::Voice(0),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 0,
            },
            Event::Wait(2),
            Event::Fine,
        ];
        let mut seq = Sequencer::new(held_note_song(track));
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        let mut frames = 0;
        while !seq.is_finished() && frames < 500 {
            seq.render_frame(&mut out);
            frames += 1;
        }
        assert!(seq.is_finished(), "FINE must release the tied voice");
    }

    #[test]
    fn eof_without_fine_releases_a_tied_voice_and_the_song_finishes() {
        // VOICE 0; TIE key60 vel127 (gate 0); W02 -- no trailing FINE.
        let bytes = [0xBD, 0x00, 0xCF, 60, 127, 0x82];
        let events = decode_track(&bytes).unwrap();
        assert!(
            !events.contains(&Event::Fine),
            "this stream must end by falling off the end, not by FINE"
        );

        let wave = Arc::new(WaveData::looping(0, 0, vec![100]));
        let voices = vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))];
        let mut seq = Sequencer::new(Song::new(voices, vec![events], 150));
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        let mut frames = 0;
        while !seq.is_finished() && frames < 500 {
            seq.render_frame(&mut out);
            frames += 1;
        }
        assert_eq!(seq.voice_count(), 0);
        assert!(seq.is_finished(), "EOF must release the tied voice");
    }

    #[test]
    fn command_cap_releases_a_tied_voice_and_the_song_finishes() {
        // A self-GOTO with no Wait ends only via the command guard, which
        // must release the tied voice too, or `is_finished()` never returns.
        let track = vec![
            Event::Voice(0),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 0,
            },
            Event::Goto(2),
        ];
        let wave = Arc::new(WaveData::looping(0, 0, vec![100]));
        let voices = vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))];
        let mut seq = Sequencer::new(Song::new(voices, vec![track], 150));
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        let mut frames = 0;
        while !seq.is_finished() && frames < 500 {
            seq.render_frame(&mut out);
            frames += 1;
        }
        assert_eq!(seq.voice_count(), 0, "the command cap must release voices");
        assert!(seq.is_finished(), "the command cap must end the song");
    }

    #[test]
    fn volume_change_updates_a_held_notes_gains() {
        let track = vec![
            Event::Voice(0),
            Event::Volume(127),
            tied_note(60),
            Event::Wait(4),
            Event::Volume(20),
            Event::Wait(4),
            Event::Fine,
        ];
        let mut seq = Sequencer::new(held_note_song(track));
        render_frames(&mut seq, 1);
        let (loud_r, loud_l) = seq.mixer.voices()[0].base_volume();

        render_frames(&mut seq, 5);
        let (soft_r, soft_l) = seq.mixer.voices()[0].base_volume();
        assert!(
            soft_r < loud_r && soft_l < loud_l,
            "VOL must lower the held note ({soft_r},{soft_l} vs {loud_r},{loud_l})",
        );
        assert!(soft_r > 0 && soft_l > 0);
    }

    /// A slow, nonzero release survives past the `Fine` frame so the voice
    /// can still be inspected there, unlike `Adsr::flat()`'s `release: 0`,
    /// which retires (and drops) the voice within that very frame.
    fn slow_release_song(track: Vec<Event>) -> Song {
        slow_release_song_tracks(vec![track])
    }

    /// [`slow_release_song`] with one track per entry, so a track that ended
    /// early can be observed beside one still playing. The wave loops, so a
    /// released voice keeps producing samples while it decays.
    fn slow_release_song_tracks(tracks: Vec<Vec<Event>>) -> Song {
        let adsr = Adsr {
            attack: 255,
            decay: 255,
            sustain: 255,
            release: 250,
        };
        let wave = Arc::new(WaveData::looping(1 << 20, 0, vec![100; SAMPLES_PER_FRAME]));
        let voices = vec![Instrument::DirectSound(ToneData::new(wave, adsr))];
        Song::new(voices, tracks, 150)
    }

    /// `ply_fine` releases a track's voices rather than retiring them, and
    /// `SoundMain` keeps mixing every frame regardless of
    /// `MUSICPLAYER_STATUS_PAUSE` (`m4a_1.s:20`-`:119`), so a voice still in
    /// release when the terminal step stops the surviving tracks rings out
    /// instead of being cut short with them. This song carries no reverb, so
    /// the voice is the only thing that can still sound.
    #[test]
    fn a_paused_sequencer_still_sounds_a_voice_an_ended_track_left_in_release() {
        let ending = vec![Event::Voice(0), tied_note(60), Event::Wait(1), Event::Fine];
        let surviving = vec![Event::Voice(0), tied_note(72), Event::Wait(200)];
        let mut seq = Sequencer::new(slow_release_song_tracks(vec![ending, surviving]));
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];

        seq.render_frame_with_fade(&mut out, None);
        seq.render_frame_with_fade(&mut out, None);
        assert!(
            seq.tracks[0].ended && !seq.tracks[1].ended,
            "sanity: the first track must end via Fine while the second plays on"
        );

        seq.render_frame_with_fade(&mut out, Some(0));
        assert!(
            seq.is_sounding(),
            "the terminal step stops the surviving track, not the voice the ended one released"
        );

        seq.render_frame_with_fade(&mut out, Some(0));
        assert!(
            out.iter().any(|&sample| sample != 0.0),
            "the paused mixer must keep rendering that released voice"
        );
    }

    /// Upstream marks every existing track's `volX` before that frame's
    /// track pass (`FadeOutBody`, `m4a.c:750`-`:757`), but only propagates it
    /// into a channel afterward (`TrkVolPitSet`/`ChnVolSetAsm`,
    /// `m4a_1.s:1361`-`:1400`); `ply_fine` clears a track's flags first
    /// (`m4a_1.s:750`-`:777`), so a track ending via `Fine` on the same tick
    /// as a fade step never receives that step.
    #[test]
    fn a_track_that_ends_via_fine_this_tick_never_receives_that_ticks_fade_step() {
        let track = vec![Event::Voice(0), tied_note(60), Event::Wait(1), Event::Fine];
        let mut seq = Sequencer::new(slow_release_song(track));
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];

        seq.render_frame_with_fade(&mut out, None);
        let before = seq.mixer.voices()[0].base_volume();

        seq.render_frame_with_fade(&mut out, Some(32));
        assert!(
            seq.tracks[0].ended,
            "sanity: Fine must end the track this frame"
        );
        assert_eq!(
            seq.mixer.voices()[0].base_volume(),
            before,
            "a track ending via Fine this tick must not receive that tick's fade step"
        );
    }

    /// Upstream's `ply_pan` only raises `MPT_FLG_VOLCHG`; every volume
    /// recomputation is deferred to the single post-tick `TrkVolPitSet`
    /// (`m4a_1.s:1361`-`:1400`), which `ply_fine`'s flag clear
    /// (`m4a_1.s:750`-`:777`) skips for a track that ended this tick. So an
    /// intervening volume-affecting command must not leak this tick's fade
    /// step into the released voice either. `Pan(0)` here leaves `pan` at its
    /// default 0, so `vol_x` is the only volume input that changed.
    #[test]
    fn an_intervening_pan_must_not_leak_this_ticks_fade_step_into_a_fine_ending_track() {
        let track = vec![
            Event::Voice(0),
            tied_note(60),
            Event::Wait(1),
            Event::Pan(0),
            Event::Fine,
        ];
        let mut seq = Sequencer::new(slow_release_song(track));
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];

        seq.render_frame_with_fade(&mut out, None);
        let before = seq.mixer.voices()[0].base_volume();

        seq.render_frame_with_fade(&mut out, Some(32));
        assert!(
            seq.tracks[0].ended,
            "sanity: Fine must end the track this frame"
        );
        assert_eq!(
            seq.mixer.voices()[0].base_volume(),
            before,
            "an intervening volume command must not propagate this tick's fade step to a track \
             ending via Fine"
        );
    }

    /// `ply_fine` unlinks every channel and zeroes the flags before the
    /// post-tick pass runs (`m4a_1.s:750`-`:777`), so a same-tick `PAN`
    /// never reaches the released voice.
    #[test]
    fn a_pan_command_in_the_same_tick_as_fine_must_not_reach_the_released_voice() {
        let track = vec![
            Event::Voice(0),
            tied_note(60),
            Event::Wait(1),
            Event::Pan(-64),
            Event::Fine,
        ];
        let mut seq = Sequencer::new(slow_release_song(track));
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];

        seq.render_frame(&mut out);
        let before = seq.mixer.voices()[0].base_volume();

        seq.render_frame(&mut out);
        assert!(
            seq.tracks[0].ended,
            "sanity: Fine must end the track this frame"
        );
        assert_eq!(
            seq.mixer.voices()[0].base_volume(),
            before,
            "a PAN reached in the same tick as FINE must not propagate to the released voice"
        );
    }

    /// The pitch-domain twin of the `PAN` case above (`m4a_1.s:994`-`:1005`).
    #[test]
    fn a_bend_command_in_the_same_tick_as_fine_must_not_reach_the_released_voice() {
        let track = vec![
            Event::Voice(0),
            Event::BendRange(2),
            tied_note(60),
            Event::Wait(1),
            Event::Bend(63),
            Event::Fine,
        ];
        let mut seq = Sequencer::new(slow_release_song(track));
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];

        seq.render_frame(&mut out);
        let before = seq.mixer.voices()[0].frequency();

        seq.render_frame(&mut out);
        assert!(
            seq.tracks[0].ended,
            "sanity: Fine must end the track this frame"
        );
        assert_eq!(
            seq.mixer.voices()[0].frequency(),
            before,
            "a BEND reached in the same tick as FINE must not propagate to the released voice"
        );
    }

    /// Propagation runs once per `MPlayMain` call after every tick of the
    /// frame (`m4a_1.s:1169`-`:1175`), so a control from the first of two
    /// same-frame ticks still never reaches a voice the second tick's `Fine`
    /// releases.
    #[test]
    fn a_control_from_an_earlier_tick_in_the_same_frame_as_fine_must_not_reach_the_released_voice()
    {
        let track = vec![
            Event::Voice(0),
            tied_note(60),
            Event::Wait(1),
            Event::Pan(-64),
            Event::Wait(1),
            Event::Fine,
        ];
        let mut seq = Sequencer::new(slow_release_song(track));
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];

        // Frame 1, at the song's default tempo, runs exactly one tick:
        // starts the note and immediately consumes the first `Wait(1)`.
        seq.render_frame(&mut out);
        let before = seq.mixer.voices()[0].base_volume();

        // Frame 2 runs two ticks: the first reaches `Pan`, the second `Fine`.
        seq.tempo_i = 2 * TEMPO_UNIT;
        seq.render_frame(&mut out);
        assert!(
            seq.tracks[0].ended,
            "sanity: Fine must end the track this frame"
        );
        assert_eq!(
            seq.mixer.voices()[0].base_volume(),
            before,
            "a control from an earlier tick in the same frame as FINE must not propagate to the \
             released voice"
        );
    }

    /// A successful `ply_note` applies the track's current controls to the
    /// new voice and masks the flags (`m4a_1.s:1802`-`:1805`), so a same-tick
    /// control never also reaches an older voice on that track.
    #[test]
    fn a_note_started_the_same_tick_as_a_pending_control_consumes_it_before_older_voices_see_it() {
        let wave = Arc::new(WaveData::looping(1 << 20, 0, vec![100; SAMPLES_PER_FRAME]));
        let voices = vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))];
        let track = vec![
            Event::Voice(0),
            tied_note(60),
            Event::Wait(1),
            Event::Pan(-64),
            tied_note(72),
            Event::Wait(4),
            Event::Fine,
        ];
        let mut seq = Sequencer::with_config(
            Song::new(voices, vec![track], 150),
            DEFAULT_MASTER_VOLUME,
            2,
        );
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];

        seq.render_frame(&mut out);
        assert_eq!(
            seq.voice_count(),
            1,
            "sanity: only the first note has started"
        );
        let older_before = seq
            .mixer
            .voices()
            .iter()
            .find(|voice| voice.midi_key() == 60)
            .expect("the first note's voice must exist")
            .base_volume();

        seq.render_frame(&mut out);
        assert_eq!(
            seq.voice_count(),
            2,
            "sanity: the second note must also start"
        );
        let older_after = seq
            .mixer
            .voices()
            .iter()
            .find(|voice| voice.midi_key() == 60)
            .expect("the first note's voice must still exist")
            .base_volume();
        assert_eq!(
            older_after, older_before,
            "a control consumed by a same-tick Note must not reach the track's other voices"
        );
    }

    /// A fade step raises the same flag as a `VOL` command (`m4a.c:753`-`:757`),
    /// so a same-tick `Note` consumes it like any other control.
    #[test]
    fn a_note_started_the_same_tick_as_a_fade_step_consumes_it_before_older_voices_see_it() {
        let wave = Arc::new(WaveData::looping(1 << 20, 0, vec![100; SAMPLES_PER_FRAME]));
        let voices = vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))];
        let track = vec![
            Event::Voice(0),
            tied_note(60),
            Event::Wait(1),
            tied_note(72),
            Event::Wait(4),
            Event::Fine,
        ];
        let mut seq = Sequencer::with_config(
            Song::new(voices, vec![track], 150),
            DEFAULT_MASTER_VOLUME,
            2,
        );
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];

        seq.render_frame(&mut out);
        assert_eq!(
            seq.voice_count(),
            1,
            "sanity: only the first note has started"
        );
        let older_before = seq
            .mixer
            .voices()
            .iter()
            .find(|voice| voice.midi_key() == 60)
            .expect("the first note's voice must exist")
            .base_volume();

        // Frame 2 reaches a new `Note` with a fade step landing the same frame.
        seq.render_frame_with_fade(&mut out, Some(32));
        assert_eq!(
            seq.voice_count(),
            2,
            "sanity: the second note must also start"
        );
        let older_after = seq
            .mixer
            .voices()
            .iter()
            .find(|voice| voice.midi_key() == 60)
            .expect("the first note's voice must still exist")
            .base_volume();
        assert_eq!(
            older_after, older_before,
            "a fade step consumed by a same-tick Note must not reach the track's other voices"
        );
    }

    /// `FadeOutBody`'s terminal step stops every track outright
    /// (`m4a.c:715`-`:743`) and `TrackStop` turns the CGB channel off as it
    /// goes (`m4a_1.s:1490`-`:1493`), so the PSG voice is gone after that
    /// frame -- not merely scaled to zero in the output buffer.
    #[test]
    fn a_terminal_fade_step_retires_a_sustained_cgb_voice() {
        let voices = vec![Instrument::CgbSquare1(SquareTone {
            duty: 2,
            sweep: 0,
            adsr: CgbAdsr::flat(),
            fixed_rate: false,
        })];
        let track = vec![Event::Voice(0), tied_note(60), Event::Wait(200)];
        let mut seq = Sequencer::new(Song::new(voices, vec![track], 150));
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];

        seq.render_frame_with_fade(&mut out, Some(64));
        assert!(
            seq.mixer.cgb_voices()[CgbChannelNumber::Square1.slot()].is_some(),
            "sanity: the CGB voice must be sounding before the terminal step"
        );

        seq.render_frame_with_fade(&mut out, Some(0));
        assert!(
            seq.mixer.cgb_voices()[CgbChannelNumber::Square1.slot()].is_none(),
            "the terminal fade step must retire the CGB voice, not just silence its output"
        );
    }

    #[test]
    fn bend_changes_a_held_notes_frequency() {
        let wave = Arc::new(WaveData::looping(1 << 20, 0, vec![100; SAMPLES_PER_FRAME]));
        let voices = vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))];
        let track = vec![
            Event::Voice(0),
            Event::BendRange(2),
            tied_note(60),
            Event::Wait(4),
            Event::Bend(63),
            Event::Wait(4),
            Event::Fine,
        ];
        let mut seq = Sequencer::new(Song::new(voices, vec![track], 150));
        render_frames(&mut seq, 1);
        let base_freq = seq.mixer.voices()[0].frequency();

        render_frames(&mut seq, 5);
        let bent_freq = seq.mixer.voices()[0].frequency();
        assert!(
            bent_freq > base_freq,
            "BEND up must raise the held note's frequency ({bent_freq} vs {base_freq})",
        );
    }

    #[test]
    fn track_pitch_key_m_stays_full_width_within_a_signed_byte() {
        let mut track = TrackState::new();
        track.key_shift = 127;
        assert_eq!(track_pitch(&track).0, 127);
        track.key_shift = -128;
        assert_eq!(track_pitch(&track).0, -128);
    }

    #[test]
    fn track_pitch_key_m_wraps_through_a_signed_byte() {
        let mut track = TrackState::new();
        track.key_shift = 127;
        track.bend = 1;
        track.bend_range = 64;
        assert_eq!(track_pitch(&track).0, -128);
    }

    #[test]
    fn track_volume_in_range_channels_pass_through() {
        let track = TrackState::new();
        assert_eq!(track_volume(&track), (127, 126));
    }

    #[test]
    fn track_volume_tremolo_peak_wraps_through_a_byte() {
        let mut track = TrackState::new();
        track.vol = 127;
        track.modulation_target = ModulationTarget::AMPLITUDE;
        track.modulation = 127;
        track.pan = 63;
        assert_eq!(track_volume(&track), (246, 1));
    }

    #[test]
    fn modulation_target_selects_pitch_amplitude_or_pan() {
        let mut track = TrackState::new();
        track.modulation = 32;

        track.modulation_target = ModulationTarget::PITCH;
        assert_eq!(track_pitch(&track), (2, 0));
        assert_eq!(track_volume(&track), (127, 126));

        track.modulation_target = ModulationTarget::AMPLITUDE;
        assert_eq!(track_pitch(&track), (0, 0));
        assert_eq!(track_volume(&track), (158, 157));

        track.modulation_target = ModulationTarget::PAN;
        assert_eq!(track_pitch(&track), (0, 0));
        assert_eq!(track_volume(&track), (158, 94));
    }

    #[test]
    #[should_panic(expected = "multiple of FRAME_SAMPLES")]
    fn mix_into_rejects_partial_frames() {
        let song = test_song(vec![vec![Event::Fine]], 150);
        let mut seq = Sequencer::new(song);
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES + 1];
        seq.mix_into(&mut out);
    }

    #[test]
    #[should_panic(expected = "multiple of FRAME_SAMPLES")]
    fn mix_into_rejects_an_empty_buffer() {
        let song = test_song(vec![vec![Event::Fine]], 150);
        let mut seq = Sequencer::new(song);
        let mut out: Vec<f32> = vec![];
        seq.mix_into(&mut out);
    }

    // --- LFO/vibrato -----------------------------------------------------

    #[test]
    fn lfo_pitch_modulation_changes_a_held_notes_frequency_over_time() {
        let wave = Arc::new(WaveData::looping(1 << 20, 0, vec![100; SAMPLES_PER_FRAME]));
        let voices = vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))];
        let track = vec![
            Event::Voice(0),
            Event::Modulation(40),
            Event::LfoSpeed(30),
            tied_note(60),
            Event::Wait(96),
            Event::Fine,
        ];
        let mut seq = Sequencer::new(Song::new(voices, vec![track], 150));
        render_frames(&mut seq, 1);
        let base_freq = seq.mixer.voices()[0].frequency();

        let mut changed = false;
        for _ in 0..40 {
            render_frames(&mut seq, 1);
            if seq.mixer.voices()[0].frequency() != base_freq {
                changed = true;
                break;
            }
        }
        assert!(changed, "LFO should eventually bend the held note's pitch");
    }

    #[test]
    fn lfo_measurably_changes_the_rendered_output_vs_no_lfo() {
        let make_wave = || {
            Arc::new(WaveData::looping(
                1 << 20,
                0,
                vec![100, -100, 50, -50, 30, -30, 10, -10],
            ))
        };
        let make_track = |with_lfo: bool| {
            let mut track = vec![Event::Voice(0)];
            if with_lfo {
                track.push(Event::Modulation(60));
                track.push(Event::LfoSpeed(40));
            }
            track.push(tied_note(60));
            track.push(Event::Wait(96));
            track.push(Event::Fine);
            track
        };
        let render = |with_lfo: bool| {
            let voices = vec![Instrument::DirectSound(ToneData::new(
                make_wave(),
                Adsr::flat(),
            ))];
            let mut seq = Sequencer::new(Song::new(voices, vec![make_track(with_lfo)], 150));
            let mut buf = vec![0.0; Sequencer::FRAME_SAMPLES];
            for _ in 0..25 {
                seq.render_frame(&mut buf);
            }
            buf
        };

        assert_ne!(
            render(false),
            render(true),
            "an active LFO must audibly diverge from the unmodulated render"
        );
    }

    #[test]
    fn lfo_delay_holds_off_modulation_until_it_elapses() {
        let wave = Arc::new(WaveData::looping(1 << 20, 0, vec![100; SAMPLES_PER_FRAME]));
        let voices = vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))];
        let track = vec![
            Event::Voice(0),
            Event::Modulation(60),
            Event::LfoSpeed(80),
            Event::LfoDelay(10),
            tied_note(60),
            Event::Wait(96),
            Event::Fine,
        ];
        let mut seq = Sequencer::new(Song::new(voices, vec![track], 150));
        render_frames(&mut seq, 1);
        let base_freq = seq.mixer.voices()[0].frequency();
        for _ in 0..9 {
            render_frames(&mut seq, 1);
            assert_eq!(
                seq.mixer.voices()[0].frequency(),
                base_freq,
                "frequency must not move during the LFO delay"
            );
        }
        assert_eq!(seq.tracks[0].lfo_delay_remaining, 0);

        render_frames(&mut seq, 1);
        assert_eq!(seq.tracks[0].lfo_phase, 80);
        assert_ne!(seq.mixer.voices()[0].frequency(), base_freq);
    }

    #[test]
    fn cgb_note_resets_modulation_only_after_allocation() {
        let instruments = vec![
            direct_sound(100),
            Instrument::CgbSquare1(SquareTone {
                duty: 2,
                sweep: 0,
                adsr: CgbAdsr::flat(),
                fixed_rate: false,
            }),
        ];
        let song = Song::new(instruments, vec![vec![], vec![]], 150);
        let mut seq = Sequencer::with_config(song, DEFAULT_MASTER_VOLUME, 2);

        seq.tracks[0].voice = 1;
        seq.tracks[0].priority = 10;
        apply_test_event(&mut seq, 0, &tied_note(50));

        seq.tracks[1].voice = 0;
        seq.tracks[1].modulation_target = ModulationTarget::AMPLITUDE;
        seq.tracks[1].modulation = 64;
        apply_test_event(&mut seq, 1, &tied_note(60));
        let modulated_volume = seq.mixer.voices()[0].base_volume();

        seq.tracks[1].voice = 1;
        seq.tracks[1].lfo_delay = 7;
        seq.tracks[1].lfo_delay_remaining = 3;
        seq.tracks[1].lfo_phase = 91;
        apply_test_event(&mut seq, 1, &tied_note(70));

        assert_eq!(seq.tracks[1].key, 70, "the raw track key still updates");
        assert_eq!(seq.tracks[1].lfo_delay_remaining, 3);
        assert_eq!(seq.tracks[1].lfo_phase, 91);
        assert_eq!(seq.tracks[1].modulation, 64);
        assert_eq!(seq.mixer.voices()[0].base_volume(), modulated_volume);
        let square1 = seq.mixer.cgb_voices()[CgbChannelNumber::Square1.slot()]
            .as_ref()
            .expect("the original square-1 occupant must remain");
        assert_eq!(square1.track(), 0);
        assert_eq!(square1.midi_key(), 50);

        seq.tracks[1].priority = 20;
        apply_test_event(&mut seq, 1, &tied_note(70));
        assert_eq!(seq.tracks[1].lfo_delay_remaining, 7);
        assert_eq!(seq.tracks[1].lfo_phase, 0);
        assert_eq!(seq.tracks[1].modulation, 0);
        let square1 = seq.mixer.cgb_voices()[CgbChannelNumber::Square1.slot()]
            .as_ref()
            .expect("the higher-priority note must replace square 1");
        assert_eq!(square1.track(), 1);
        assert_eq!(square1.midi_key(), 70);
    }

    #[test]
    fn accepted_cgb_sweep_note_uses_reset_pitch_modulation() {
        const ADDING_SWEEP_PERIOD_1_SHIFT_1: u8 = 0x11;

        let instruments = vec![Instrument::CgbSquare1(SquareTone {
            duty: 2,
            sweep: ADDING_SWEEP_PERIOD_1_SHIFT_1,
            adsr: CgbAdsr::flat(),
            fixed_rate: false,
        })];
        let song = Song::new(instruments, vec![vec![]], 150);
        let mut seq = Sequencer::new(song);

        seq.tracks[0].modulation_target = ModulationTarget::PITCH;
        seq.tracks[0].modulation = 127;
        seq.tracks[0].lfo_delay = 7;
        seq.tracks[0].lfo_phase = 91;
        apply_test_event(&mut seq, 0, &tied_note(48));

        assert_eq!(seq.tracks[0].lfo_delay_remaining, 7);
        assert_eq!(seq.tracks[0].lfo_phase, 0);
        assert_eq!(seq.tracks[0].modulation, 0);
        let square1 = seq.mixer.cgb_voices()[CgbChannelNumber::Square1.slot()]
            .as_ref()
            .expect("the accepted square-1 note must occupy its channel");
        assert!(
            square1.is_active(),
            "the sweep must initialize from reset pitch modulation, without trigger overflow"
        );
    }

    #[test]
    fn direct_sound_note_resets_modulation_only_after_allocation() {
        let song = Song::new(vec![direct_sound(100)], vec![vec![], vec![]], 150);
        let mut seq = Sequencer::with_config(song, DEFAULT_MASTER_VOLUME, 2);

        seq.tracks[1].priority = 10;
        seq.tracks[1].modulation = 32;
        apply_test_event(&mut seq, 1, &tied_note(60));
        seq.tracks[0].priority = 10;
        apply_test_event(&mut seq, 0, &tied_note(50));
        let modulated_frequency = seq
            .mixer
            .voices()
            .into_iter()
            .find(|voice| voice.track() == Some(1))
            .expect("track 1 must own its original voice")
            .frequency();
        let occupants: Vec<_> = seq
            .mixer
            .voices()
            .into_iter()
            .map(|voice| (voice.track(), voice.midi_key()))
            .collect();

        seq.tracks[1].priority = 0;
        seq.tracks[1].lfo_delay = 7;
        seq.tracks[1].lfo_delay_remaining = 3;
        seq.tracks[1].lfo_phase = 91;
        apply_test_event(&mut seq, 1, &tied_note(70));

        assert_eq!(seq.tracks[1].key, 70, "the raw track key still updates");
        assert_eq!(seq.tracks[1].lfo_delay_remaining, 3);
        assert_eq!(seq.tracks[1].lfo_phase, 91);
        assert_eq!(seq.tracks[1].modulation, 32);
        let voices = seq.mixer.voices();
        assert_eq!(
            voices
                .iter()
                .find(|voice| voice.track() == Some(1))
                .expect("the original track-1 voice must remain")
                .frequency(),
            modulated_frequency
        );
        assert_eq!(
            voices
                .iter()
                .map(|voice| (voice.track(), voice.midi_key()))
                .collect::<Vec<_>>(),
            occupants,
            "a refused note must not replace either pool occupant"
        );

        seq.tracks[1].priority = 20;
        apply_test_event(&mut seq, 1, &tied_note(70));
        assert_eq!(seq.tracks[1].lfo_delay_remaining, 7);
        assert_eq!(seq.tracks[1].lfo_phase, 0);
        assert_eq!(seq.tracks[1].modulation, 0);
        assert!(seq
            .mixer
            .voices()
            .iter()
            .any(|voice| voice.track() == Some(1) && voice.midi_key() == 70));
    }

    #[test]
    fn lfo_triangle_uses_the_wide_phase_sum_before_byte_truncation() {
        let wide_phase_sum = 400;
        let triangle = lfo_triangle(wide_phase_sum);
        assert_eq!(triangle, -272);
        assert_eq!(scale_lfo(40, triangle), 86);

        assert_eq!(lfo_triangle(0x80), 0);
        assert_eq!(lfo_triangle(0x20), 0x20);
        assert_eq!(lfo_triangle(0xC0), -64);
    }

    // --- Pattern execution (`PATT`/`PEND`/`REPT`) ---------------------------

    fn render_track(track: Vec<Event>, frames: usize) -> Vec<f32> {
        let mut sequencer = Sequencer::new(test_song(vec![track], 150));
        let mut output = vec![0.0; Sequencer::FRAME_SAMPLES * frames];
        sequencer.mix_into(&mut output);
        output
    }

    #[test]
    fn pattern_call_renders_identically_to_the_unrolled_track() {
        const PATTERN_BODY: usize = 4;

        let with_pattern = vec![
            Event::Voice(0),
            Event::Pattern(PATTERN_BODY),
            Event::Wait(48),
            Event::Fine,
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 8,
            },
            Event::PatternEnd,
        ];
        let unrolled = vec![
            Event::Voice(0),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 8,
            },
            Event::Wait(48),
            Event::Fine,
        ];

        assert_eq!(render_track(with_pattern, 80), render_track(unrolled, 80));
    }

    #[test]
    fn nested_pattern_calls_render_identically_to_the_unrolled_track() {
        const OUTER_BODY: usize = 4;
        const INNER_BODY: usize = 8;

        let note = |key: u8| Event::Note {
            key,
            velocity: 127,
            gate: 4,
        };
        let nested = vec![
            Event::Voice(0),
            Event::Pattern(OUTER_BODY),
            Event::Wait(12),
            Event::Fine,
            Event::Pattern(INNER_BODY),
            note(64),
            Event::Wait(12),
            Event::PatternEnd,
            note(60),
            Event::Wait(12),
            Event::PatternEnd,
        ];
        let unrolled = vec![
            Event::Voice(0),
            note(60),
            Event::Wait(12),
            note(64),
            Event::Wait(12),
            Event::Wait(12),
            Event::Fine,
        ];

        assert_eq!(render_track(nested, 80), render_track(unrolled, 80));
    }

    #[test]
    fn nested_pattern_calls_return_to_each_call_site() {
        let mut sequencer = Sequencer::new(test_song(vec![vec![Event::Fine]], 150));
        sequencer.tracks[0].cursor = 2;

        apply_test_event(&mut sequencer, 0, &Event::Pattern(10));
        sequencer.tracks[0].cursor = 11;
        apply_test_event(&mut sequencer, 0, &Event::Pattern(20));
        apply_test_event(&mut sequencer, 0, &Event::PatternEnd);
        assert_eq!(sequencer.tracks[0].cursor, 11);

        apply_test_event(&mut sequencer, 0, &Event::PatternEnd);
        assert_eq!(sequencer.tracks[0].cursor, 2);
        assert_eq!(sequencer.tracks[0].pattern_depth, 0);
    }

    #[test]
    fn fourth_nested_pattern_call_ends_the_track() {
        let mut sequencer = Sequencer::new(test_song(vec![vec![Event::Fine]], 150));

        for target in 1..=MAX_PATTERN_DEPTH {
            apply_test_event(&mut sequencer, 0, &Event::Pattern(target));
            assert!(!sequencer.tracks[0].ended);
        }
        apply_test_event(&mut sequencer, 0, &Event::Pattern(99));

        assert!(sequencer.tracks[0].ended);
        assert_eq!(sequencer.tracks[0].pattern_depth, MAX_PATTERN_DEPTH);
    }

    #[test]
    fn pattern_end_without_a_call_is_a_no_op() {
        let mut sequencer = Sequencer::new(test_song(vec![vec![Event::Fine]], 150));
        let original = sequencer.tracks[0].clone();

        apply_test_event(&mut sequencer, 0, &Event::PatternEnd);

        assert_eq!(sequencer.tracks[0], original);
    }

    #[test]
    fn repeat_jumps_until_its_count_is_reached_then_falls_through() {
        const REPEAT_TARGET: usize = 3;
        const FALLTHROUGH: usize = 7;

        let mut sequencer = Sequencer::new(test_song(vec![vec![Event::Fine]], 150));
        for pass in 1..=5 {
            sequencer.tracks[0].cursor = FALLTHROUGH;
            apply_test_event(
                &mut sequencer,
                0,
                &Event::Repeat {
                    count: 5,
                    target: REPEAT_TARGET,
                },
            );
            let expected_cursor = if pass < 5 { REPEAT_TARGET } else { FALLTHROUGH };
            assert_eq!(sequencer.tracks[0].cursor, expected_cursor, "pass {pass}");
        }
        assert_eq!(sequencer.tracks[0].repeat_counter, 0);
    }

    #[test]
    fn repeat_count_one_falls_through_immediately() {
        let mut sequencer = Sequencer::new(test_song(vec![vec![Event::Fine]], 150));
        sequencer.tracks[0].cursor = 7;

        apply_test_event(
            &mut sequencer,
            0,
            &Event::Repeat {
                count: 1,
                target: 3,
            },
        );

        assert_eq!(sequencer.tracks[0].cursor, 7);
        assert_eq!(sequencer.tracks[0].repeat_counter, 0);
    }

    #[test]
    fn repeat_count_zero_jumps_without_incrementing() {
        let mut sequencer = Sequencer::new(test_song(vec![vec![Event::Fine]], 150));
        sequencer.tracks[0].cursor = 7;

        apply_test_event(
            &mut sequencer,
            0,
            &Event::Repeat {
                count: 0,
                target: 3,
            },
        );

        assert_eq!(sequencer.tracks[0].cursor, 3);
        assert_eq!(sequencer.tracks[0].repeat_counter, 0);
    }

    fn repeat_body() -> [Event; 2] {
        [
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 4,
            },
            Event::Wait(12),
        ]
    }

    /// A finite `REPT` track, built so `target` points at the first event of
    /// [`repeat_body`].
    fn repeat_track(count: u8) -> Vec<Event> {
        const REPEAT_TARGET: usize = 1;

        let mut track = vec![Event::Voice(0)];
        track.extend(repeat_body());
        track.push(Event::Repeat {
            count,
            target: REPEAT_TARGET,
        });
        track.push(Event::Fine);
        track
    }

    #[test]
    fn repeat_renders_identically_to_the_unrolled_track() {
        const REPEAT_COUNT: u8 = 3;

        let mut unrolled = vec![Event::Voice(0)];
        for _ in 0..REPEAT_COUNT {
            unrolled.extend(repeat_body());
        }
        unrolled.push(Event::Fine);

        assert_eq!(
            render_track(repeat_track(REPEAT_COUNT), 80),
            render_track(unrolled, 80)
        );
    }

    #[test]
    fn repeat_reaches_fine_through_the_rendered_track() {
        let mut sequencer = Sequencer::new(test_song(vec![repeat_track(3)], 150));
        let mut output = vec![0.0; Sequencer::FRAME_SAMPLES];

        for _ in 0..200 {
            sequencer.render_frame(&mut output);
        }

        assert!(sequencer.is_finished());
    }

    #[test]
    fn repeat_count_zero_loops_the_rendered_track_forever() {
        let mut sequencer = Sequencer::new(test_song(vec![repeat_track(0)], 150));
        let mut output = vec![0.0; Sequencer::FRAME_SAMPLES];

        for _ in 0..200 {
            sequencer.render_frame(&mut output);
        }

        assert!(!sequencer.is_finished());
    }

    // --- CGB PSG instruments, wired end-to-end through the sequencer -------

    fn cgb_test_track() -> Vec<Event> {
        vec![
            Event::Voice(0),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 8,
            },
            Event::Wait(48),
            Event::Fine,
        ]
    }

    #[test]
    fn a_cgb_square_note_produces_sound_through_the_sequencer() {
        let voices = vec![Instrument::CgbSquare1(SquareTone {
            duty: 2,
            sweep: 0,
            adsr: CgbAdsr::flat(),
            fixed_rate: false,
        })];
        let song = Song::new(voices, vec![cgb_test_track()], 150);
        let mut seq = Sequencer::new(song);
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        seq.render_frame(&mut out);
        assert!(out.iter().any(|&s| s.abs() > 0.0));
        assert_eq!(seq.voice_count(), 1);
    }

    #[test]
    fn a_fixed_rate_cgb_square_note_still_produces_sound_through_the_sequencer() {
        let voices = vec![Instrument::CgbSquare1(SquareTone {
            duty: 2,
            sweep: 0,
            adsr: CgbAdsr::flat(),
            fixed_rate: true,
        })];
        let song = Song::new(voices, vec![cgb_test_track()], 150);
        let mut seq = Sequencer::new(song);
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        seq.render_frame(&mut out);
        assert!(out.iter().any(|&s| s.abs() > 0.0));
        assert_eq!(seq.voice_count(), 1);
    }

    /// Render `frames` frames of a one-instrument song around `instrument`,
    /// concatenated -- the comparison buffer for the fixed-rate threading
    /// tests below.
    fn rendered_cgb_frames(instrument: Instrument, frames: usize) -> Vec<f32> {
        let song = Song::new(vec![instrument], vec![cgb_test_track()], 150);
        let mut seq = Sequencer::new(song);
        let mut all = Vec::with_capacity(frames * Sequencer::FRAME_SAMPLES);
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        for _ in 0..frames {
            seq.render_frame(&mut out);
            all.extend_from_slice(&out);
        }
        all
    }

    /// `SquareTone::fixed_rate` must actually reach [`CgbVoice`]'s DAC
    /// correction through the sequencer, not merely avoid a crash:
    /// [`cgb_test_track`]'s key lands on a register `cgb_dac_correct`
    /// rounds to a different playback rate, so the two renders must
    /// diverge.
    #[test]
    fn a_fixed_rate_cgb_square_audibly_differs_from_a_plain_one() {
        let tone = |fixed_rate| {
            Instrument::CgbSquare1(SquareTone {
                duty: 2,
                sweep: 0,
                adsr: CgbAdsr::flat(),
                fixed_rate,
            })
        };
        let fixed = rendered_cgb_frames(tone(true), 8);
        let plain = rendered_cgb_frames(tone(false), 8);
        assert!(
            fixed.iter().zip(&plain).any(|(a, b)| a != b),
            "the DAC-corrected register must change the rendered square waveform"
        );
    }

    /// [`WaveTone::fixed_rate`]'s counterpart to
    /// [`a_fixed_rate_cgb_square_audibly_differs_from_a_plain_one`] -- the
    /// wave channel threads the same flag through `CgbVoice::wave`.
    #[test]
    fn a_fixed_rate_cgb_wave_audibly_differs_from_a_plain_one() {
        // Decodes to alternating 0/15 samples: a full-swing waveform, so a
        // playback-rate difference is visible (a constant table would
        // render identically at any rate).
        const FULL_SWING_WAVE_RAM: [u8; 16] = [0x0F; 16];

        let tone = |fixed_rate| {
            Instrument::CgbWave(WaveTone {
                table: FULL_SWING_WAVE_RAM,
                adsr: CgbAdsr::flat(),
                fixed_rate,
            })
        };
        let fixed = rendered_cgb_frames(tone(true), 8);
        let plain = rendered_cgb_frames(tone(false), 8);
        assert!(
            fixed.iter().zip(&plain).any(|(a, b)| a != b),
            "the DAC-corrected register must change the rendered wave waveform"
        );
    }

    /// [`Sequencer::with_resolved_reverb`] clamps its level to the
    /// `SOUND_MODE_REVERB_VAL` domain (`0..=127`) exactly as
    /// [`Song::with_reverb`] does at the header boundary: an out-of-range
    /// `255` must behave as `127`, never as unclamped comb feedback.
    #[test]
    fn an_out_of_range_resolved_reverb_level_clamps_to_the_canonical_maximum() {
        let track = || {
            vec![
                Event::Voice(0),
                Event::Note {
                    key: 60,
                    velocity: 127,
                    gate: 8,
                },
                Event::Wait(48),
                Event::Fine,
            ]
        };
        let song = || test_song(vec![track()], 150);
        let mut clamped = Sequencer::with_resolved_reverb(song(), 15, 8, 255);
        let mut canonical = Sequencer::with_resolved_reverb(song(), 15, 8, 127);
        let mut a = vec![0.0; Sequencer::FRAME_SAMPLES];
        let mut b = vec![0.0; Sequencer::FRAME_SAMPLES];
        for _ in 0..8 {
            clamped.render_frame(&mut a);
            canonical.render_frame(&mut b);
            assert_eq!(a, b, "255 must render exactly as the clamped 127");
        }
    }

    #[test]
    fn a_cgb_wave_note_produces_sound_through_the_sequencer() {
        let voices = vec![Instrument::CgbWave(WaveTone {
            table: [0xFF; 16],
            adsr: CgbAdsr::flat(),
            fixed_rate: false,
        })];
        let song = Song::new(voices, vec![cgb_test_track()], 150);
        let mut seq = Sequencer::new(song);
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        seq.render_frame(&mut out);
        assert!(out.iter().any(|&s| s.abs() > 0.0));
        assert_eq!(seq.voice_count(), 1);
    }

    #[test]
    fn a_cgb_noise_note_produces_sound_through_the_sequencer() {
        let voices = vec![Instrument::CgbNoise(NoiseTone {
            lfsr_width_selector: 0,
            adsr: CgbAdsr::flat(),
        })];
        let song = Song::new(voices, vec![cgb_test_track()], 150);
        let mut seq = Sequencer::new(song);
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        seq.render_frame(&mut out);
        assert!(out.iter().any(|&s| s.abs() > 0.0));
        assert_eq!(seq.voice_count(), 1);
    }

    #[test]
    fn cgb_square1_and_square2_occupy_independent_channel_slots() {
        // Two different tracks each selecting a square instrument must both
        // sound at once — they are different hardware channel numbers, not
        // a shared pool.
        let voices = vec![
            Instrument::CgbSquare1(SquareTone {
                duty: 2,
                sweep: 0,
                adsr: CgbAdsr::flat(),
                fixed_rate: false,
            }),
            Instrument::CgbSquare2(SquareTone {
                duty: 1,
                sweep: 0,
                adsr: CgbAdsr::flat(),
                fixed_rate: false,
            }),
        ];
        let track_a = vec![
            Event::Voice(0),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 8,
            },
            Event::Wait(48),
            Event::Fine,
        ];
        let track_b = vec![
            Event::Voice(1),
            Event::Note {
                key: 64,
                velocity: 127,
                gate: 8,
            },
            Event::Wait(48),
            Event::Fine,
        ];
        let song = Song::new(voices, vec![track_a, track_b], 150);
        let mut seq = Sequencer::new(song);
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        seq.render_frame(&mut out);
        assert_eq!(seq.voice_count(), 2);
    }

    #[test]
    fn a_new_note_on_the_same_cgb_channel_replaces_the_old_one() {
        // Two notes on the same track (same instrument -> same hardware
        // channel) in immediate succession: the second retriggers the
        // channel rather than accumulating a second voice.
        let voices = vec![Instrument::CgbNoise(NoiseTone {
            lfsr_width_selector: 0,
            adsr: CgbAdsr::flat(),
        })];
        let track = vec![
            Event::Voice(0),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 0,
            },
            Event::Note {
                key: 64,
                velocity: 127,
                gate: 0,
            },
            Event::Wait(4),
            Event::Fine,
        ];
        let song = Song::new(voices, vec![track], 150);
        let mut seq = Sequencer::new(song);
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        seq.render_frame(&mut out);
        assert_eq!(seq.voice_count(), 1);
    }

    // --- Key-split / rhythm indirection (`TONEDATA_TYPE_SPL`/`_RHY`) -------

    fn direct_sound(sample: i8) -> Instrument {
        Instrument::DirectSound(ToneData::new(
            Arc::new(WaveData::one_shot(
                1 << 20,
                vec![sample; SAMPLES_PER_FRAME * 4],
            )),
            Adsr::flat(),
        ))
    }

    #[test]
    fn key_split_boundary_selects_the_correct_child() {
        let mut table = [0u8; KEY_SLOTS];
        for slot in table.iter_mut().skip(64) {
            *slot = 1;
        }
        let split = Instrument::KeySplit(KeySplit {
            table,
            children: vec![Some(direct_sound(40)), Some(direct_sound(100))],
        });

        let render = |key: u8| {
            let track = vec![
                Event::Voice(0),
                Event::Note {
                    key,
                    velocity: 127,
                    gate: 8,
                },
                Event::Wait(48),
                Event::Fine,
            ];
            let song = Song::new(vec![split.clone()], vec![track], 150);
            let mut seq = Sequencer::new(song);
            let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
            seq.render_frame(&mut out);
            out
        };

        let key_below_split = render(30);
        let key_at_or_above_split = render(90);
        assert_ne!(
            key_below_split, key_at_or_above_split,
            "the split boundary must select different children"
        );
        let magnitude = |buf: &[f32]| buf.iter().map(|s| s.abs()).sum::<f32>();
        assert!(
            magnitude(&key_at_or_above_split) > magnitude(&key_below_split),
            "key 90 must select the louder (sample 100) child, not the quieter one"
        );
    }

    #[test]
    fn key_split_keeps_the_played_key_for_pitch() {
        let mut table = [0u8; KEY_SLOTS];
        for slot in table.iter_mut().skip(64) {
            *slot = 1;
        }
        let split = Instrument::KeySplit(KeySplit {
            table,
            children: vec![Some(direct_sound(40)), Some(direct_sound(100))],
        });
        for &key in &[30u8, 90u8] {
            let track = vec![
                Event::Voice(0),
                Event::Note {
                    key,
                    velocity: 127,
                    gate: 8,
                },
                Event::Wait(48),
                Event::Fine,
            ];
            let song = Song::new(vec![split.clone()], vec![track], 150);
            let mut seq = Sequencer::new(song);
            let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
            seq.render_frame(&mut out);
            assert_eq!(
                seq.mixer.voices()[0].frequency(),
                pitch::midi_key_to_freq(1 << 20, key, 0),
                "key-split pitch must use the played key {key}, not any child override"
            );
        }
    }

    #[test]
    fn rhythm_indirection_selects_child_by_played_key_directly() {
        let mut children: Vec<Option<RhythmChild>> = vec![None; KEY_SLOTS];
        children[36] = Some(RhythmChild {
            instrument: direct_sound(90),
            base_key: 72,
            pan: None,
        });
        let rhythm = Instrument::Rhythm(Rhythm { children });

        let track_for = |key: u8| {
            vec![
                Event::Voice(0),
                Event::Note {
                    key,
                    velocity: 127,
                    gate: 8,
                },
                Event::Wait(48),
                Event::Fine,
            ]
        };

        let song = Song::new(vec![rhythm.clone()], vec![track_for(36)], 150);
        let mut seq = Sequencer::new(song);
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        seq.render_frame(&mut out);
        assert_eq!(seq.voice_count(), 1, "a populated rhythm slot must sound");

        let song = Song::new(vec![rhythm], vec![track_for(37)], 150);
        let mut seq = Sequencer::new(song);
        seq.render_frame(&mut out);
        assert_eq!(
            seq.voice_count(),
            0,
            "an unpopulated rhythm slot must produce no note, not panic or fall back"
        );
    }

    #[test]
    fn rhythm_child_base_key_overrides_pitch() {
        let mut children: Vec<Option<RhythmChild>> = vec![None; KEY_SLOTS];
        children[36] = Some(RhythmChild {
            instrument: direct_sound(90),
            base_key: 72,
            pan: None,
        });
        let rhythm = Instrument::Rhythm(Rhythm { children });
        let track = vec![
            Event::Voice(0),
            Event::Note {
                key: 36,
                velocity: 127,
                gate: 8,
            },
            Event::Wait(48),
            Event::Fine,
        ];
        let song = Song::new(vec![rhythm], vec![track], 150);
        let mut seq = Sequencer::new(song);
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        seq.render_frame(&mut out);
        assert_eq!(
            seq.mixer.voices()[0].frequency(),
            pitch::midi_key_to_freq(1 << 20, 72, 0),
            "rhythm pitch must come from the child's base key, not the played key"
        );
    }

    #[test]
    fn rhythm_child_pan_override_is_applied_when_the_bit_is_set() {
        let mut children: Vec<Option<RhythmChild>> = vec![None; KEY_SLOTS];
        let hard_right_pan_override = rhythm_pan_from_pan_sweep(0xFF);
        children[36] = Some(RhythmChild {
            instrument: direct_sound(90),
            base_key: 36,
            pan: hard_right_pan_override,
        });
        let rhythm = Instrument::Rhythm(Rhythm { children });
        let track = vec![
            Event::Voice(0),
            Event::Note {
                key: 36,
                velocity: 127,
                gate: 8,
            },
            Event::Wait(48),
            Event::Fine,
        ];
        let song = Song::new(vec![rhythm], vec![track], 150);
        let mut seq = Sequencer::new(song);
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        seq.render_frame(&mut out);
        let (right, left) = seq.mixer.voices()[0].base_volume();
        assert!(
            right > left,
            "a rhythm pan override toward the right must skew the channel volumes ({right} vs {left})"
        );
    }

    #[test]
    fn nested_key_split_or_rhythm_child_produces_no_note() {
        let inner_rhythm = Instrument::Rhythm(Rhythm {
            children: vec![None; KEY_SLOTS],
        });
        let table = [0u8; KEY_SLOTS];
        let split = Instrument::KeySplit(KeySplit {
            table,
            children: vec![Some(inner_rhythm)],
        });
        let track = vec![
            Event::Voice(0),
            Event::Note {
                key: 10,
                velocity: 127,
                gate: 8,
            },
            Event::Wait(48),
            Event::Fine,
        ];
        let song = Song::new(vec![split], vec![track], 150);
        let mut seq = Sequencer::new(song);
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        seq.render_frame(&mut out);
        assert_eq!(seq.voice_count(), 0, "nested indirection must not sound");
    }

    // --- Fixed-rate DirectSound (`TONEDATA_TYPE_FIX`) -----------------------

    /// A short, varying, looping waveform: a constant wave can't distinguish
    /// "sampled at a different rate" from "sampled at the same rate", since
    /// every source sample reads back the same value regardless of pitch.
    fn varying_wave() -> Arc<WaveData> {
        Arc::new(WaveData::looping(
            1 << 20,
            0,
            vec![100, -100, 50, -50, 30, -30, 10, -10],
        ))
    }

    #[test]
    fn fixed_rate_instrument_renders_identically_regardless_of_played_key() {
        let render = |key: u8| {
            let tone = ToneData::new(varying_wave(), Adsr::flat()).fixed();
            let voices = vec![Instrument::DirectSound(tone)];
            let track = vec![
                Event::Voice(0),
                Event::Note {
                    key,
                    velocity: 127,
                    gate: 90,
                },
                Event::Wait(96),
                Event::Fine,
            ];
            let song = Song::new(voices, vec![track], 150);
            let mut seq = Sequencer::new(song);
            let mut buf = vec![0.0; Sequencer::FRAME_SAMPLES * 3];
            seq.mix_into(&mut buf);
            buf
        };
        assert_eq!(
            render(40),
            render(90),
            "a fixed-rate instrument must ignore the played note's pitch entirely"
        );
    }

    #[test]
    fn non_fixed_instrument_renders_differently_across_keys_for_contrast() {
        let render = |key: u8| {
            let tone = ToneData::new(varying_wave(), Adsr::flat());
            let voices = vec![Instrument::DirectSound(tone)];
            let track = vec![
                Event::Voice(0),
                Event::Note {
                    key,
                    velocity: 127,
                    gate: 90,
                },
                Event::Wait(96),
                Event::Fine,
            ];
            let song = Song::new(voices, vec![track], 150);
            let mut seq = Sequencer::new(song);
            let mut buf = vec![0.0; Sequencer::FRAME_SAMPLES * 3];
            seq.mix_into(&mut buf);
            buf
        };
        assert_ne!(render(40), render(90));
    }

    // --- xIECV/xIECL pseudo-echo XCMDs --------------------------------------

    #[test]
    fn xcmd_iecv_and_iecl_only_affect_subsequently_started_voices() {
        let pre_echo_key = 60;
        let post_echo_key = 64;
        let wave = Arc::new(WaveData::one_shot(0, vec![100; SAMPLES_PER_FRAME]));
        let voices = vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))];
        let track = vec![
            Event::Voice(0),
            Event::Note {
                key: pre_echo_key,
                velocity: 127,
                gate: 0,
            },
            Event::Wait(2),
            Event::Xcmd {
                kind: XCMD_IECV,
                value: 200,
            },
            Event::Xcmd {
                kind: XCMD_IECL,
                value: 5,
            },
            Event::Note {
                key: post_echo_key,
                velocity: 127,
                gate: 0,
            },
            Event::Wait(2),
            Event::EndOfTie {
                key: Some(pre_echo_key),
            },
            Event::EndOfTie {
                key: Some(post_echo_key),
            },
            Event::Wait(64),
            Event::Fine,
        ];
        let mut seq = Sequencer::new(Song::new(voices, vec![track], 150));
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];

        let mut pre_echo_seen = false;
        let mut post_echo_seen = false;
        let mut pre_echo_gone_at = None;
        let mut post_echo_gone_at = None;
        for frame in 0..64 {
            seq.render_frame(&mut out);
            let has_pre_echo = seq
                .mixer
                .voices()
                .iter()
                .any(|v| v.midi_key() == pre_echo_key);
            let has_post_echo = seq
                .mixer
                .voices()
                .iter()
                .any(|v| v.midi_key() == post_echo_key);
            pre_echo_seen |= has_pre_echo;
            post_echo_seen |= has_post_echo;
            if pre_echo_seen && !has_pre_echo && pre_echo_gone_at.is_none() {
                pre_echo_gone_at = Some(frame);
            }
            if post_echo_seen && !has_post_echo && post_echo_gone_at.is_none() {
                post_echo_gone_at = Some(frame);
            }
        }
        let pre_echo_gone = pre_echo_gone_at.expect("the pre-echo voice must eventually retire");
        let post_echo_gone =
            post_echo_gone_at.expect("the post-xIECV voice must eventually retire");
        assert!(
            post_echo_gone > pre_echo_gone,
            "the xIECV/xIECL voice must outlive the voice started before them ({post_echo_gone} vs {pre_echo_gone})"
        );
    }

    #[test]
    fn xcmd_iecv_and_iecl_extend_a_directsound_voices_lifetime() {
        let make_track = |with_echo: bool| {
            let mut track = vec![Event::Voice(0)];
            if with_echo {
                track.push(Event::Xcmd {
                    kind: XCMD_IECV,
                    value: 200,
                });
                track.push(Event::Xcmd {
                    kind: XCMD_IECL,
                    value: 10,
                });
            }
            track.push(Event::Note {
                key: 60,
                velocity: 127,
                gate: 4,
            });
            track.push(Event::Wait(200));
            track.push(Event::Fine);
            track
        };
        let frames_to_silence = |with_echo: bool| {
            let wave = Arc::new(WaveData::one_shot(0, vec![100; SAMPLES_PER_FRAME]));
            let voices = vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))];
            let song = Song::new(voices, vec![make_track(with_echo)], 150);
            let mut seq = Sequencer::new(song);
            let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
            let mut frames = 0;
            loop {
                seq.render_frame(&mut out);
                frames += 1;
                if seq.voice_count() == 0 || frames >= 200 {
                    break;
                }
            }
            frames
        };
        assert!(
            frames_to_silence(true) > frames_to_silence(false),
            "xIECV/xIECL must extend the DirectSound voice's lifetime via its pseudo-echo tail"
        );
    }

    #[test]
    fn xcmd_iecv_and_iecl_extend_a_cgb_voices_lifetime() {
        let make_track = |with_echo: bool| {
            let mut track = vec![Event::Voice(0)];
            if with_echo {
                track.push(Event::Xcmd {
                    kind: XCMD_IECV,
                    value: 200,
                });
                track.push(Event::Xcmd {
                    kind: XCMD_IECL,
                    value: 10,
                });
            }
            track.push(Event::Note {
                key: 60,
                velocity: 127,
                gate: 4,
            });
            track.push(Event::Wait(200));
            track.push(Event::Fine);
            track
        };
        let frames_to_silence = |with_echo: bool| {
            let voices = vec![Instrument::CgbSquare1(SquareTone {
                duty: 2,
                sweep: 0,
                adsr: CgbAdsr {
                    attack: 0,
                    decay: 0,
                    sustain: 15,
                    release: 0,
                },
                fixed_rate: false,
            })];
            let song = Song::new(voices, vec![make_track(with_echo)], 150);
            let mut seq = Sequencer::new(song);
            let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
            let mut frames = 0;
            loop {
                seq.render_frame(&mut out);
                frames += 1;
                if seq.voice_count() == 0 || frames >= 200 {
                    break;
                }
            }
            frames
        };
        assert!(
            frames_to_silence(true) > frames_to_silence(false),
            "xIECV/xIECL must extend the CGB voice's lifetime via its pseudo-echo tail"
        );
    }

    // --- Normalized 128-slot voice banks -------------------------------------

    #[test]
    fn voice_slot_127_is_an_explicit_entry_not_an_adjacent_lookup() {
        let build_voices = |slot127_sample: i8| {
            let mut voices: Vec<Instrument> = (0..127).map(|_| direct_sound(10)).collect();
            voices.push(direct_sound(slot127_sample));
            voices
        };
        assert_eq!(build_voices(0).len(), KEY_SLOTS);

        let render = |slot127_sample: i8| {
            let voices = build_voices(slot127_sample);
            let track = vec![
                Event::Voice(127),
                Event::Note {
                    key: 60,
                    velocity: 127,
                    gate: 8,
                },
                Event::Wait(48),
                Event::Fine,
            ];
            let song = Song::new(voices, vec![track], 150);
            assert!(song.voice(127).is_some(), "slot 127 must be populated");
            assert!(
                song.voice(128).is_none(),
                "index 128 must be out of range, not an adjacent wraparound"
            );
            let mut seq = Sequencer::new(song);
            let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
            seq.render_frame(&mut out);
            out
        };

        assert_ne!(
            render(10),
            render(120),
            "slot 127 must read its own explicit entry, not an adjacent, wrapped, or default slot"
        );
    }

    fn stamped_priority(song_priority: u8, track_priority: u8) -> u8 {
        let track = vec![
            Event::Voice(0),
            Event::Priority(track_priority),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 8,
            },
            Event::Wait(48),
            Event::Fine,
        ];
        let song = test_song(vec![track], 150).with_priority(song_priority);
        let mut seq = Sequencer::new(song);
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        seq.render_frame(&mut out);
        seq.mixer.voices()[0].priority()
    }

    #[test]
    fn note_priority_adds_the_song_and_track_halves() {
        assert_eq!(stamped_priority(0, 0), 0);
        assert_eq!(stamped_priority(30, 0), 30);
        assert_eq!(stamped_priority(0, 40), 40);
        assert_eq!(stamped_priority(30, 40), 70);
    }

    #[test]
    fn note_priority_saturates_instead_of_wrapping() {
        assert_eq!(stamped_priority(200, 100), 255);
        assert_eq!(stamped_priority(255, 1), 255);
        assert_eq!(stamped_priority(255, 255), 255);
    }

    #[test]
    fn a_prio_command_only_affects_later_notes() {
        let track = vec![
            Event::Voice(0),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 8,
            },
            Event::Priority(90),
            Event::Note {
                key: 64,
                velocity: 127,
                gate: 8,
            },
            Event::Wait(48),
            Event::Fine,
        ];
        let mut seq = Sequencer::new(test_song(vec![track], 150).with_priority(5));
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        seq.render_frame(&mut out);
        let stamped: Vec<(u8, u8)> = seq
            .mixer
            .voices()
            .iter()
            .map(|voice| (voice.midi_key(), voice.priority()))
            .collect();
        assert_eq!(stamped, vec![(60, 5), (64, 95)]);
    }

    #[test]
    fn a_low_priority_track_loses_its_note_when_the_pool_is_full() {
        // A refused note finds every pool channel outranking it
        // (`m4a_1.s:1669`..`:1718`).
        let track_with_priority = |priority: u8| {
            vec![
                Event::Voice(0),
                Event::Priority(priority),
                Event::Note {
                    key: 60,
                    velocity: 127,
                    gate: 8,
                },
                Event::Wait(48),
                Event::Fine,
            ]
        };
        let mut tracks: Vec<Vec<Event>> = (0..5).map(|_| track_with_priority(100)).collect();
        tracks.push(track_with_priority(1));
        let mut seq = Sequencer::new(test_song(tracks, 150));
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        seq.render_frame(&mut out);

        assert_eq!(seq.voice_count(), DEFAULT_MAX_VOICES);
        assert!(
            seq.mixer
                .voices()
                .iter()
                .all(|voice| voice.track().is_some_and(|track| track < 5)),
            "the sixth track's weaker note must never have started"
        );
    }
}
