//! The owned sequencer: the tick engine that walks each track's decoded
//! events, turns them into note-ons/offs, and drives the [`Mixer`].
//!
//! Behavioural port of `MPlayMain` (`m4a_1.s:1129`) plus the volume/pitch
//! resolution in `TrkVolPitSet` (`m4a.c:765`) and `ChnVolSetAsm`
//! (`m4a_1.s:1508`). Tempo accumulates at `TEMPO_UNIT` per tick; each tick
//! counts note-off gates down, then processes commands until every track is
//! blocked on a `Wait`. Rendering happens one frame at a time, matching
//! `SoundMain`'s once-per-V-blank cadence.
//!
//! This slice also executes pattern control flow (`PATT`/`PEND`/`REPT`,
//! `m4a_1.s:851`..`:910`) and per-tick LFO/vibrato (`MPlayMain`'s wait-tick
//! tail, `m4a_1.s:1285`..`:1330`), and dispatches `VOICE` to either a
//! DirectSound or a CGB PSG instrument — resolving key-split/rhythm
//! indirection first, and the `xIECV`/`xIECL` pseudo-echo `XCMD`s
//! (`ply_note`, `m4a_1.s:1580`..`:1609`, `:1757`..`:1758`; `ply_xiecv`/
//! `ply_xiecl`, `m4a.c:1591`..`:1600`).
//!
//! `PRIO` is executed too: the track priority it sets combines with the song
//! header's into each note's effective note-on priority (see
//! [`Sequencer::note_priority`]), which drives [`crate::mixer::Mixer`]'s
//! channel reuse/steal/refuse search.
//!
//! `MEMACC` is executed too (`ply_memacc`, `m4a.c:1437`..`:1521`), using a
//! private accumulator area for each [`Sequencer`] and safe no-ops for
//! out-of-range cell addresses.
//!
//! Out of scope for this slice (decoded but not executed): the remaining
//! `XCMD` sub-commands (tone overrides, wave swap, portamento wait) and
//! `PORT` — neither is ever emitted by `tools/mid2agb`
//! (`crates/assets/src/audio.rs`'s "Deferred commands").

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

/// Tempo units per tick; `MPlayMain` fires a tick each time `tempoC` crosses
/// `150` (`subs r0, 150`).
const TEMPO_UNIT: u16 = 150;

/// Default track volume when a track plays a note before any `VOL` command.
/// (Upstream leaves `track->vol` at `0`; this crate defaults to full so a
/// minimal hand-authored sequence is audible — a deliberate convenience.)
const DEFAULT_TRACK_VOLUME: u8 = 127;

/// Default pitch-bend range (`track->bendRange = 2`, `m4a_1.s:1223`).
const DEFAULT_BEND_RANGE: u8 = 2;

/// Default LFO rate (`track->lfoSpeed = 0x16`, `m4a_1.s:1226`); harmless
/// while `lfo_depth` defaults to `0` (`m4a_1.s:1288`).
const DEFAULT_LFO_SPEED: u8 = 22;

/// Max nested `PATT` depth (`track->patternStack`'s capacity, `ply_patt`,
/// `m4a_1.s:851`).
const MAX_PATTERN_DEPTH: usize = 3;

/// Fixed `volX` input to `TrkVolPitSet` (`m4a.c:772`).
const TRACK_VOLUME_SCALE: u32 = 0x40;

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

/// One [`Sequencer`]'s zero-initialized `MEMACC` cells.
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

/// Per-track runtime state (the mutable half of `struct MusicPlayerTrack`).
#[derive(Clone, Debug, Eq, PartialEq)]
struct TrackState {
    cursor: usize,
    wait: u16,
    ended: bool,
    voice: usize,
    vol: u8,
    pan: i8,
    bend: i8,
    bend_range: u8,
    tune: i8,
    key_shift: i8,
    /// The track's current MIDI key (`track->key`): the raw key of the last
    /// note, reused when an `EOT` omits its key operand (`m4a_1.s:1830`).
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
    /// Active pseudo-echo volume (`xIECV`, `track->pseudoEchoVolume`);
    /// applies only to voices started after it last changed (`m4a_1.s:1757`..`:1758`).
    pseudo_echo_volume: u8,
    /// Active pseudo-echo length (`xIECL`, `track->pseudoEchoLength`); see
    /// [`Self::pseudo_echo_volume`].
    pseudo_echo_length: u8,
    /// This track's own note priority (`PRIO`, `track->priority`); combined
    /// into each new note's effective priority by [`Sequencer::note_priority`].
    priority: u8,
}

impl TrackState {
    fn new() -> Self {
        Self {
            cursor: 0,
            wait: 0,
            ended: false,
            voice: 0,
            vol: DEFAULT_TRACK_VOLUME,
            pan: 0,
            bend: 0,
            bend_range: DEFAULT_BEND_RANGE,
            tune: 0,
            key_shift: 0,
            // Upstream zeroes `track->key` at init; no `EOT` should fire before
            // a note sets it in real data.
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
            // Zeroed with the rest of a freshly cleared track (`Clear64byte`,
            // `m4a_1.s:1219`); only `PRIO` raises it again.
            priority: 0,
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
    /// Tempo increment per frame (BPM); accumulates into `tempo_c`.
    tempo_i: u16,
    tempo_c: u16,
    /// `MEMACC`'s accumulator area (see [`MemAccArea`]).
    mem_acc: MemAccArea,
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
    /// `reverb_level` clamps to `0..=127` (`SOUND_MODE_REVERB_VAL`,
    /// `m4a_internal.h:12`), the same bound [`Song::with_reverb`] enforces
    /// at the header-ingest boundary; upstream itself masks the byte
    /// (`soundInfo->reverb = temp & SOUND_MODE_REVERB_VAL`, `m4a.c:445`).
    #[must_use]
    pub fn with_resolved_reverb(
        song: Song,
        master_volume: u8,
        max_voices: usize,
        reverb_level: u8,
    ) -> Self {
        let tracks = (0..song.track_count()).map(|_| TrackState::new()).collect();
        let tempo_i = song.initial_tempo();
        let mixer = Mixer::new(master_volume, max_voices).with_reverb_level(reverb_level.min(127));
        Self {
            song,
            tracks,
            mixer,
            tempo_i,
            tempo_c: 0,
            mem_acc: MemAccArea::default(),
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
        self.tracks.iter().all(|t| t.ended)
            && self.mixer.is_idle()
            && !self.mixer.has_pending_reverb()
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

    /// Run the tempo accumulator for one frame, firing ticks as it crosses
    /// [`TEMPO_UNIT`] (`m4a_1.s:1169`..`:1359`).
    ///
    /// # Overflow
    ///
    /// `tempo_c += tempo_i` never overflows its `u16`s: the loop below always
    /// leaves `tempo_c < TEMPO_UNIT` (149 at most), and every `tempo_i`
    /// ingestion point ([`Song::new`]'s initial tempo, [`Event::Tempo`]'s
    /// runtime assignment in [`Self::handle_event`]) clamps to
    /// [`MAX_TEMPO_BPM`] (510) — `149 + 510` stays far under `u16::MAX`.
    fn advance_frame(&mut self) {
        debug_assert!(self.tempo_c < TEMPO_UNIT);
        debug_assert!(self.tempo_i <= MAX_TEMPO_BPM);
        self.tempo_c += self.tempo_i;
        while self.tempo_c >= TEMPO_UNIT {
            self.tempo_c -= TEMPO_UNIT;
            self.do_tick();
        }
    }

    fn do_tick(&mut self) {
        self.mixer.tick_gates();
        // Disjoint field borrows so each track can touch the shared mixer
        // and the shared MEMACC accumulator area.
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

    /// Process one track for one tick: run commands until it blocks on a
    /// `Wait`, then consume one tick of that wait.
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
                    // A guarded end still honors `ply_fine`'s voice-release
                    // cleanup, like every other end path.
                    mixer.release_track(track_id);
                    track.ended = true;
                    break;
                }
                if track.cursor >= events.len() {
                    // A stream lacking `FINE` still ends cleanly
                    // (`decode_track` allows it), but must still honor `ply_fine`'s voice-release cleanup.
                    mixer.release_track(track_id);
                    track.ended = true;
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
            Self::apply_lfo(track, mixer, track_id);
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
                Self::apply_track_volume(track, mixer, track_id);
            }
            Event::Pan(p) => {
                track.pan = p;
                Self::apply_track_volume(track, mixer, track_id);
            }
            // `bpm` already carries `clamp_tempo`'s bound when it came from
            // `decode_track`'s `TEMPO` arm, but this also runs on an
            // `Event::Tempo` converted from the normalized asset-pack schema
            // (`assets::audio::song::SongEvent::Tempo`), which round-trips an
            // unbounded on-disk `u16` -- so the clamp is re-applied here
            // too, guarding `tempo_c`'s accumulation against a malformed
            // pack.
            Event::Tempo(bpm) => *tempo_i = clamp_tempo(bpm),
            Event::KeyShift(k) => {
                track.key_shift = k;
                Self::apply_track_pitch(track, mixer, track_id);
            }
            Event::Bend(b) => {
                track.bend = b;
                Self::apply_track_pitch(track, mixer, track_id);
            }
            Event::BendRange(r) => {
                track.bend_range = r;
                Self::apply_track_pitch(track, mixer, track_id);
            }
            Event::Tune(t) => {
                track.tune = t;
                Self::apply_track_pitch(track, mixer, track_id);
            }
            Event::Modulation(depth) => {
                track.lfo_depth = depth;
                if depth == 0 {
                    Self::reset_lfo(track, mixer, track_id);
                }
            }
            Event::ModType(kind) => {
                let target = ModulationTarget::from_command(kind);
                if track.modulation_target != target {
                    track.modulation_target = target;
                    Self::apply_track_volume(track, mixer, track_id);
                    Self::apply_track_pitch(track, mixer, track_id);
                }
            }
            Event::LfoSpeed(speed) => {
                track.lfo_speed = speed;
                if speed == 0 {
                    Self::reset_lfo(track, mixer, track_id);
                }
            }
            Event::LfoDelay(delay) => track.lfo_delay = delay,
            // `ply_prio` stores the operand on the track
            // (`m4a_1.s:912`..`:917`); it takes effect only for notes
            // started afterwards, since `ply_note` reads it when it stamps
            // a channel (`m4a_1.s:1628`..`:1633`).
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
                        Self::reset_lfo(track, mixer, track_id);
                    }
                }
            }
            Event::EndOfTie { key } => {
                // With an operand, `ply_endtie` stores it as the new `track->key`
                // and matches on it; without one, it matches the current key.
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
            } => {
                // `ply_xiecv` (`m4a.c:1591`): stores the raw byte on the
                // track; only subsequently started voices pick it up (see
                // `note_on`), matching upstream's note-on-time copy
                // (`m4a_1.s:1757`..`:1758`).
                track.pseudo_echo_volume = u8::try_from(value).unwrap_or(0);
            }
            Event::Xcmd {
                kind: XCMD_IECL,
                value,
            } => {
                // `ply_xiecl` (`m4a.c:1597`).
                track.pseudo_echo_length = u8::try_from(value).unwrap_or(0);
            }
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

    fn reset_lfo(track: &mut TrackState, mixer: &mut Mixer, track_id: usize) {
        track.reset_lfo();
        Self::apply_modulation_target(track, mixer, track_id);
    }

    fn apply_modulation_target(track: &TrackState, mixer: &mut Mixer, track_id: usize) {
        if track.modulation_target.is_pitch() {
            Self::apply_track_pitch(track, mixer, track_id);
        } else {
            Self::apply_track_volume(track, mixer, track_id);
        }
    }

    fn apply_lfo(track: &mut TrackState, mixer: &mut Mixer, track_id: usize) {
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
        Self::apply_modulation_target(track, mixer, track_id);
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
    // A flat per-instrument-kind dispatch: long by construction (five leaf
    // kinds, each threading the same handful of resolved values), not
    // logically complex -- mirrors `handle_event`'s same allowance.
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

        // Floored at 0 (`m4a_1.s:1760`..`:1766`), then passed as
        // `MidiKeyToFreq`/`MidiKeyToCgbFreq`'s `u8 key` parameter
        // (`m4a.c:23`, `:810`), so it wraps modulo 256 rather than saturating.
        let note_key = u8::try_from((i32::from(pitch_key) + key_m).max(0) & 0xFF).unwrap_or(0);
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
            // `resolve_instrument` never returns an indirection as the leaf.
            Instrument::KeySplit(_) | Instrument::Rhythm(_) => false,
        }
    }

    /// A new note's effective priority: the song header's priority
    /// (`MusicPlayerInfo::priority`) plus the sounding track's own `PRIO`,
    /// saturated rather than wrapped at `0xFF` (`m4a_1.s:1628`..`:1633` --
    /// upstream adds in a wide register and clamps the sum before storing
    /// the byte). [`crate::mixer::Mixer`]'s channel search ranks note-ons by
    /// this value.
    fn note_priority(song: &Song, track: &TrackState) -> u8 {
        song.priority().saturating_add(track.priority)
    }
}

/// Resolve `instrument` against the played `key` to a concrete leaf
/// instrument plus its pitch/pan context, following MP2K's key-split
/// (`TONEDATA_TYPE_SPL`) and rhythm (`TONEDATA_TYPE_RHY`) indirection exactly
/// as `ply_note` does before allocating a channel (`m4a_1.s:1580`..`:1609`).
///
/// Returns the resolved `(leaf, pitch_key, rhythm_pan)`, or `None` when the
/// table/rhythm slot has nothing for `key`, or when the resolved child is
/// itself a key-split/rhythm instrument — upstream aborts the note rather
/// than supporting nested indirection (`_081DDB80`..`b _081DDCEA`,
/// `m4a_1.s:1604`..`:1609`).
fn resolve_instrument(instrument: &Instrument, key: u8) -> Option<(&Instrument, u8, i8)> {
    let resolved = match instrument {
        Instrument::KeySplit(split) => {
            // `keySplitTable[key]` selects the child; pitch/pan still use
            // the played key untouched (`m4a_1.s:1589`, `:1598`).
            let &child_index = split.table.get(usize::from(key))?;
            let leaf = split.children.get(usize::from(child_index))?;
            (leaf, key, 0)
        }
        Instrument::Rhythm(rhythm) => {
            // The played key indexes `children` directly (no split table);
            // the child's own base key/pan replace the played note's
            // (`m4a_1.s:1580`..`:1609`).
            let child = rhythm.children.get(usize::from(key))?.as_ref()?;
            (&child.instrument, child.base_key, child.pan.unwrap_or(0))
        }
        leaf => (leaf, key, 0),
    };
    if matches!(resolved.0, Instrument::KeySplit(_) | Instrument::Rhythm(_)) {
        return None;
    }
    Some(resolved)
}

fn track_volume(track: &TrackState) -> (u8, u8) {
    let mut volume = (u32::from(track.vol) * TRACK_VOLUME_SCALE) >> 5;
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
        u8::try_from(right & 0xFF).unwrap_or(0),
        u8::try_from(left & 0xFF).unwrap_or(0),
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
    let key_offset = u8::try_from((pitch >> 8) & 0xFF).unwrap_or(0);
    let key_offset = i32::from(i8::from_le_bytes([key_offset]));
    let fine_adjust = u8::try_from(pitch & 0xFF).unwrap_or(0);
    (key_offset, fine_adjust)
}

/// `MPlayMain` selects the triangle half from the stored phase byte, but mirrors
/// its falling half against the full pre-store sum (`m4a_1.s:1298`..`:1310`).
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "MPlayMain selects the slope from the stored phase byte"
)]
fn lfo_triangle(phase_sum: u16) -> i32 {
    let stored_phase = phase_sum as u8;
    if (stored_phase.wrapping_sub(0x40) as i8) >= 0 {
        0x80i32 - i32::from(phase_sum)
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

    /// A voicegroup with one loud, long, flat-envelope instrument at unity
    /// pitch for the reference key 60.
    fn test_song(tracks: Vec<Vec<Event>>, tempo: u16) -> Song {
        // Choose wave freq so key 60 renders near unity: pick freq that makes
        // midi_key_to_freq(freq, 60, 0) close to the unity frequency.
        let target = unity_freq();
        // midi_key_to_freq(freq, 60, 0) ~= freq * ratio60 >> 32; invert roughly.
        let ratio60 = pitch::midi_key_to_freq(1 << 20, 60, 0);
        let freq = ((u64::from(target) << 20) / u64::from(ratio60)) as u32;
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
        // `m4aSoundInit` reconfigures the driver to master volume 12 and 5
        // DirectSound channels (`m4a.c:78`..`:81`), not the generic `SoundInit`
        // placeholders (15/8).
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
        // VOICE 0; note key 60 vel 127 gate ~ (N04 -> 4 ticks); W48; FINE.
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
        // The note is on this frame: audible output.
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
        // A song whose header never set a reverb level (`Song::reverb`
        // collapses that to `0`) must still get a pending reverb tail when
        // the caller supplies an explicit resolved level — this is how the
        // player crate carries a session's previously configured level
        // across such a header.
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

        // One tick per frame at tempo 150; gate 2 releases after ~2 ticks and
        // the flat envelope (release 0) then retires the voice quickly.
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

    // --- MEMACC (`ply_memacc`, `m4a.c:1437`..`:1521`) ----------------------

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

        // Built only once the first song has written a cell and finished, so
        // a process- or session-carried area would surface here.
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
        // The same track at double tempo crosses TEMPO_UNIT twice as often, so
        // it processes its waits — and reaches FINE — in fewer frames.
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
        // Tempo 100 needs two frames to cross TEMPO_UNIT (150): the first
        // leaves tempo_c short with no tick fired, so the pending Wait(5) is
        // still undecoded; the second crosses it, decodes the Wait, and
        // immediately consumes one tick of it.
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
        // `tempo_c += tempo_i` (`advance_frame`) is unguarded, so an
        // out-of-domain `Event::Tempo` -- one a malformed asset pack could
        // carry, since `SongEvent::Tempo` round-trips an unbounded on-disk
        // `u16` -- must never reach `tempo_i` un-clamped.
        let mut seq = Sequencer::new(test_song(vec![vec![Event::Fine]], 150));
        apply_test_event(&mut seq, 0, &Event::Tempo(u16::MAX));
        assert_eq!(
            seq.tempo_i, MAX_TEMPO_BPM,
            "an out-of-domain Tempo event must clamp to the TEMPO command's real bound"
        );

        // Drive tempo_c to the highest value `advance_frame`'s drain loop
        // ever leaves behind (`TEMPO_UNIT - 1`) and take one more frame: an
        // unclamped `tempo_i` of `u16::MAX` would overflow this addition,
        // but the clamp above keeps the sum (`149 + 510 = 659`) nowhere
        // near `u16::MAX`.
        seq.tempo_c = TEMPO_UNIT - 1;
        seq.advance_frame();
        assert_eq!(seq.tempo_c, (TEMPO_UNIT - 1 + MAX_TEMPO_BPM) % TEMPO_UNIT);
    }

    #[test]
    fn panned_note_is_louder_on_one_side() {
        // Hard-left pan: left channel should carry more energy than right.
        let track = vec![
            Event::Voice(0),
            Event::Pan(-64),
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
        // Two overlapping tied notes (keys 60 then 64). An `EOT` with no
        // operand resolves to the track's current key (64, the last note),
        // retiring only that voice; the key-60 note keeps sounding. A later
        // `EOT` naming key 60 then retires the survivor — proving the omitted
        // operand resolved to 64, not 60 and not "every voice".
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
        // Exactly one voice retired: the last-keyed note (64). Before the fix
        // this stopped both voices, leaving zero.
        assert_eq!(seq.voice_count(), 1);

        // Advance until the `EOT{Some(60)}` fires; the survivor was key 60, so
        // it is now retired too.
        for _ in 0..4 {
            seq.render_frame(&mut out);
        }
        assert_eq!(seq.voice_count(), 0);
    }

    #[test]
    fn fine_releases_a_tied_voice_and_the_song_finishes() {
        // A tied note (gate 0) never auto-releases; the freq-0 wave never
        // exhausts, so only FINE's explicit voice release can let
        // `is_finished()` return true.
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
        // Decode a real byte program with no trailing FINE: VOICE 0; TIE
        // key60 vel127 (gate 0, tied); W02 -- then the stream simply runs
        // out. `decode_track` accepts this as a clean end (no `Event::Fine`
        // appears at all), so that acceptance must still release the tied
        // voice the same way `Event::Fine` does.
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
        // `0` is a multiple of FRAME_SAMPLES arithmetically, but the
        // contract demands a POSITIVE multiple: an empty buffer must panic
        // too, not silently render nothing.
        let song = test_song(vec![vec![Event::Fine]], 150);
        let mut seq = Sequencer::new(song);
        let mut out: Vec<f32> = vec![];
        seq.mix_into(&mut out);
    }

    // --- LFO/vibrato -------------------------------------------------------

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
            .find(|voice| voice.track() == 1)
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
                .find(|voice| voice.track() == 1)
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
            .any(|voice| voice.track() == 1 && voice.midi_key() == 70));
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
        // A `PATT` inside a `PATT` body: each `PEND` returns to its own call
        // site, so the inner note sounds before the outer body resumes.
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
        // Plumbing check: `SquareTone::fixed_rate` must reach `CgbVoice`
        // without breaking playback — the DAC correction math itself is
        // pinned by `cgb_voice`'s own tests.
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
    /// [`cgb_test_track`]'s key 60 lands on the odd register `0x60B`, which
    /// `cgb_dac_correct` rounds to a different playback rate, so the two
    /// renders must diverge. A threading bug that pins the flag to either
    /// constant would make them identical.
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
        let tone = |fixed_rate| {
            Instrument::CgbWave(WaveTone {
                // `0x0F` decodes to alternating 0/15 samples -- a full-swing
                // waveform, so a playback-rate difference is visible (a
                // constant table would render identically at any rate).
                table: [0x0F; 16],
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
        // keySplitTable maps keys < 64 to child 0, >= 64 to child 1 -- two
        // otherwise-identical DirectSound children distinguished only by
        // their wave's constant sample value.
        let mut table = [0u8; KEY_SLOTS];
        for slot in table.iter_mut().skip(64) {
            *slot = 1;
        }
        let split = Instrument::KeySplit(KeySplit {
            table,
            children: vec![direct_sound(40), direct_sound(100)],
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

        let low = render(30); // < 64 -> child 0 (sample 40)
        let high = render(90); // >= 64 -> child 1 (sample 100)
        assert_ne!(
            low, high,
            "the split boundary must select different children"
        );
        let magnitude = |buf: &[f32]| buf.iter().map(|s| s.abs()).sum::<f32>();
        assert!(
            magnitude(&high) > magnitude(&low),
            "key 90 must select the louder (sample 100) child, not the quieter one"
        );
    }

    #[test]
    fn key_split_keeps_the_played_key_for_pitch() {
        // Pitch resolution must keep using the PLAYED key even though the
        // split table swaps the underlying instrument (`m4a_1.s:1589`,
        // `:1598`): a key-split note's frequency is exactly
        // `MidiKeyToFreq(child.wave.freq(), played_key, 0)`.
        let mut table = [0u8; KEY_SLOTS];
        for slot in table.iter_mut().skip(64) {
            *slot = 1;
        }
        let split = Instrument::KeySplit(KeySplit {
            table,
            children: vec![direct_sound(40), direct_sound(100)],
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
        // No split table: the played key indexes `children` directly. Key 36
        // (a typical MP2K kick-drum trigger) is populated; an unpopulated key
        // produces no note at all.
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
        // The rhythm child's own base key (72) replaces the played key (36)
        // for pitch resolution (`ply_note`, `m4a_1.s:1594`).
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
        // pan_sweep 0xFF has the 0x80 override bit set -> a hard-right pan.
        children[36] = Some(RhythmChild {
            instrument: direct_sound(90),
            base_key: 36,
            pan: rhythm_pan_from_pan_sweep(0xFF),
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
        // A child that is itself a KeySplit/Rhythm is unsupported nested
        // indirection; upstream aborts the note rather than recursing
        // (`m4a_1.s:1604`..`:1609`).
        let inner_rhythm = Instrument::Rhythm(Rhythm {
            children: vec![None; KEY_SLOTS],
        });
        let table = [0u8; KEY_SLOTS]; // every key -> child 0
        let split = Instrument::KeySplit(KeySplit {
            table,
            children: vec![inner_rhythm],
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
        // Isolates the previous test's guarantee: without `.fixed()`, the
        // SAME song rendered at two different keys must actually diverge, so
        // the equality assertion above is meaningful and not a vacuous no-op.
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
        // A tied note (key 60) starts before any xIECV/xIECL; a second tied
        // note (key 64) starts after they are set. Releasing both together
        // must retire the pre-echo voice quickly while the post-xIECV one
        // lingers in its pseudo-echo tail -- voices already started keep
        // whatever they captured at their own note-on.
        let wave = Arc::new(WaveData::one_shot(0, vec![100; SAMPLES_PER_FRAME]));
        let voices = vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))];
        let track = vec![
            Event::Voice(0),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 0,
            },
            Event::Wait(2),
            Event::Xcmd {
                kind: 0x08,
                value: 200,
            }, // xIECV
            Event::Xcmd {
                kind: 0x09,
                value: 5,
            }, // xIECL
            Event::Note {
                key: 64,
                velocity: 127,
                gate: 0,
            },
            Event::Wait(2),
            Event::EndOfTie { key: Some(60) },
            Event::EndOfTie { key: Some(64) },
            Event::Wait(64),
            Event::Fine,
        ];
        let mut seq = Sequencer::new(Song::new(voices, vec![track], 150));
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];

        let mut key60_seen = false;
        let mut key64_seen = false;
        let mut key60_gone_at = None;
        let mut key64_gone_at = None;
        for frame in 0..64 {
            seq.render_frame(&mut out);
            let has60 = seq.mixer.voices().iter().any(|v| v.midi_key() == 60);
            let has64 = seq.mixer.voices().iter().any(|v| v.midi_key() == 64);
            key60_seen |= has60;
            key64_seen |= has64;
            if key60_seen && !has60 && key60_gone_at.is_none() {
                key60_gone_at = Some(frame);
            }
            if key64_seen && !has64 && key64_gone_at.is_none() {
                key64_gone_at = Some(frame);
            }
        }
        let g60 = key60_gone_at.expect("the pre-echo voice must eventually retire");
        let g64 = key64_gone_at.expect("the post-xIECV voice must eventually retire");
        assert!(
            g64 > g60,
            "the xIECV/xIECL voice must outlive the voice started before them ({g64} vs {g60})"
        );
    }

    #[test]
    fn xcmd_iecv_and_iecl_extend_a_directsound_voices_lifetime() {
        let make_track = |with_echo: bool| {
            let mut track = vec![Event::Voice(0)];
            if with_echo {
                track.push(Event::Xcmd {
                    kind: 0x08,
                    value: 200,
                });
                track.push(Event::Xcmd {
                    kind: 0x09,
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
                    kind: 0x08,
                    value: 200,
                });
                track.push(Event::Xcmd {
                    kind: 0x09,
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

        // Two songs differing ONLY in slot 127's instrument must render
        // differently -- proving `VOICE 127` reads slot 127's own explicit
        // entry, not some other (adjacent, wrapped, or default) slot.
        assert_ne!(render(10), render(120));
    }

    /// The priority a note-on stamps on its channel after one leading
    /// `PRIO` operand, for a song of header priority `song_priority`.
    fn stamped_priority(song_priority: u8, prio: u8) -> u8 {
        let track = vec![
            Event::Voice(0),
            Event::Priority(prio),
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
        // `ply_note` sums `MusicPlayerInfo::priority` and the track's own
        // `PRIO` (`m4a_1.s:1628`..`:1631`); either half alone still reaches
        // the channel, so neither can quietly be dropped.
        assert_eq!(stamped_priority(0, 0), 0);
        assert_eq!(stamped_priority(30, 0), 30);
        assert_eq!(stamped_priority(0, 40), 40);
        assert_eq!(stamped_priority(30, 40), 70);
    }

    #[test]
    fn note_priority_saturates_instead_of_wrapping() {
        // The sum is clamped, not truncated (`m4a_1.s:1632`..`:1633`): the
        // byte-wrapped answers would be 44 and 0, both far *below* the
        // unsaturated halves rather than above them.
        assert_eq!(stamped_priority(200, 100), 255);
        assert_eq!(stamped_priority(255, 1), 255);
        assert_eq!(stamped_priority(255, 255), 255);
    }

    #[test]
    fn a_prio_command_only_affects_later_notes() {
        // `ply_prio` just stores the operand; a channel keeps whatever
        // priority it was stamped with at its own note-on.
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
        // End to end: five tracks fill the default five-channel pool at
        // priority 100, so a sixth track's `PRIO 1` note finds every channel
        // outranking it and is refused (`m4a_1.s:1716`..`:1718`).
        let loud = |prio: u8| {
            vec![
                Event::Voice(0),
                Event::Priority(prio),
                Event::Note {
                    key: 60,
                    velocity: 127,
                    gate: 8,
                },
                Event::Wait(48),
                Event::Fine,
            ]
        };
        let mut tracks: Vec<Vec<Event>> = (0..5).map(|_| loud(100)).collect();
        tracks.push(loud(1));
        let mut seq = Sequencer::new(test_song(tracks, 150));
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        seq.render_frame(&mut out);

        assert_eq!(seq.voice_count(), DEFAULT_MAX_VOICES);
        assert!(
            seq.mixer.voices().iter().all(|voice| voice.track() < 5),
            "the sixth track's weaker note must never have started"
        );
    }
}
