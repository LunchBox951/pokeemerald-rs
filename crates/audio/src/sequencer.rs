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
//! `MEMACC` is executed too (`ply_memacc`, `m4a.c:1437`..`:1521`): see
//! [`Self::exec_memacc`] for the op table, the accumulator area's ownership
//! (one per [`Sequencer`], [`MemAccArea`]'s docs), and its fail-safe
//! out-of-range handling.
//!
//! Out of scope for this slice (decoded but not executed): the remaining
//! `XCMD` sub-commands (tone overrides, wave swap, portamento wait) and
//! `PORT` — neither is ever emitted by `tools/mid2agb`
//! (`crates/assets/src/audio.rs`'s "Deliberately deferred").

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
/// while `mod_depth` defaults to `0` (`m4a_1.s:1288`).
const DEFAULT_LFO_SPEED: u8 = 22;

/// Max nested `PATT` depth (`track->patternStack`'s capacity, `ply_patt`,
/// `m4a_1.s:851`).
const MAX_PATTERN_DEPTH: usize = 3;

/// The instrument volume scaler `volX` (`0x40`, set at track init).
const VOL_X: u32 = 0x40;

/// Safety bound on commands processed for one track in one tick, so a
/// malformed loop with no `Wait` cannot hang the mixer.
const MAX_COMMANDS_PER_TICK: u32 = 4096;

/// Size of the `MEMACC` accumulator area (`gMPlayMemAccArea[0x10]`,
/// `m4a.c:20`).
const MEM_ACC_LEN: usize = 16;

/// The `MEMACC` accumulator area: 16 byte cells `MEMACC` ops read and write
/// (`ply_memacc`, `m4a.c:1437`..`:1521`).
///
/// # Divergence: one private area per [`Sequencer`], not one shared global
///
/// Upstream's array is a single global (`gMPlayMemAccArea`). `m4aSoundInit`
/// loops over `gMPlayTable` and assigns *that same* pointer to every
/// player's `MusicPlayerInfo::memAccArea` (`m4a.c:88`), so all players alias
/// one 16-byte area and can read cells another player wrote. It is zeroed
/// exactly once, by its static initializer, and never again — it persists
/// across every song any player ever plays for the life of the program.
///
/// This port instead gives each [`Sequencer`] a private area, zeroed at
/// construction. That diverges on two axes rather than matching upstream:
/// concurrent sequencers do not alias one another's cells, and a fresh
/// [`Sequencer`] starts from zero where upstream would carry the previous
/// song's cells forward. It follows from this crate having no standing
/// "player" object that outlives one song (a new [`Sequencer`] is built per
/// song, mirroring [`Song`] itself being a per-song value), so there is no
/// cross-song global to carry the area in.
///
/// # Why the divergence is unobservable
///
/// A sweep of all 530 canonical MIDIs (`sound/songs/midi/*.mid`) for the
/// controllers `tools/mid2agb` turns into `MEMACC` — CC `0x0C`/`0x10` emit
/// the command, CC `0x0D` only latches the op number
/// (`tools/mid2agb/agb.cpp:362`..`:371`) — finds exactly one emitter in the
/// entire soundtrack: `mus_vs_trainer` issues a single `mem_set` of `117`
/// into cell `0` (op `0`, addr `0`, data `117`). `mus_route104` sets CC
/// `0x0D` but never emits a `MEMACC` at all, and no other song touches
/// either controller.
///
/// So canonical data contains one blind write and zero reads: no song reads
/// a cell back, and no branch op (`6..=17`) exists in shipped data at all.
/// Nothing in the shipped soundtrack can observe cross-song, cross-player,
/// or restart-time persistence of these bytes, which is what makes the
/// private-area choice safe rather than merely convenient. The same
/// argument covers the mid-playback sequencer rebuild in
/// `crates/pokeemerald-rs/src/music/player.rs:340`..`:348`: it re-zeroes the
/// area part-way through a song, and again there is no canonical reader to
/// notice.
type MemAccArea = [u8; MEM_ACC_LEN];

/// Per-track runtime state (the mutable half of `struct MusicPlayerTrack`).
#[derive(Clone, Debug)]
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
    /// LFO depth (`MOD`, `track->mod`).
    mod_depth: u8,
    /// LFO target: `0` = pitch, `1` = amplitude (volume), `2` = pan (`MODT`).
    mod_type: u8,
    /// LFO rate (`LFOS`, `track->lfoSpeed`).
    lfo_speed: u8,
    /// Ticks to hold before the LFO starts after a note-on (`LFODL`).
    lfo_delay: u8,
    /// Countdown from `lfo_delay`, reloaded on every note-on.
    lfo_delay_c: u8,
    /// Wrapping triangle-wave phase (`track->lfoSpeedC`).
    lfo_speed_c: u8,
    /// The LFO's current signed output (`track->modM`), folded into pitch or
    /// volume depending on `mod_type`.
    mod_m: i8,
    /// Saved return cursors for nested `PATT` calls (`track->patternStack`).
    pattern_stack: [usize; MAX_PATTERN_DEPTH],
    /// Current `PATT` nesting depth, `0..=MAX_PATTERN_DEPTH` (`track->patternLevel`).
    pattern_level: u8,
    /// `REPT`'s in-progress repeat counter (`track->repN`).
    rep_n: u8,
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
            mod_depth: 0,
            mod_type: 0,
            lfo_speed: DEFAULT_LFO_SPEED,
            lfo_delay: 0,
            lfo_delay_c: 0,
            lfo_speed_c: 0,
            mod_m: 0,
            pattern_stack: [0; MAX_PATTERN_DEPTH],
            pattern_level: 0,
            rep_n: 0,
            pseudo_echo_volume: 0,
            pseudo_echo_length: 0,
            // Zeroed with the rest of a freshly cleared track (`Clear64byte`,
            // `m4a_1.s:1219`); only `PRIO` raises it again.
            priority: 0,
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
            // Zeroed at construction — see `MemAccArea`'s doc comment.
            mem_acc: [0; MEM_ACC_LEN],
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

    /// Apply one decoded event to a track (and the mixer/tempo).
    // A flat command-dispatch table: long by nature, but each arm is trivial
    // (mirrors `sequence::Decoder::dispatch`'s same allowance).
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
                // `ply_fine` releases every still-ON channel this track owns
                // before clearing it, so tied voices and looping waves stop.
                mixer.release_track(track_id);
                track.ended = true;
            }
            Event::Goto(index) => track.cursor = index,
            Event::Voice(v) => track.voice = usize::from(v),
            // Volume/pan set `MPT_FLG_VOLCHG`; pitch commands set
            // `MPT_FLG_PITCHG`. Upstream re-runs `TrkVolPitSet` + per-channel
            // `ChnVolSetAsm`/`MidiKeyToFreq` for the flagged track each tick;
            // here every event executes before the frame renders, so applying
            // the change to the track's live voices immediately is equivalent.
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
            // pack (#404).
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
                track.mod_depth = depth;
                if depth == 0 {
                    Self::clear_mod_m(track, mixer, track_id);
                }
            }
            Event::ModType(kind) => {
                // `ply_modt` only reapplies pitch/volume when the type
                // actually changes (`m4a_1.s:1031`..`:1038`).
                if track.mod_type != kind {
                    track.mod_type = kind;
                    Self::apply_track_volume(track, mixer, track_id);
                    Self::apply_track_pitch(track, mixer, track_id);
                }
            }
            Event::LfoSpeed(speed) => {
                track.lfo_speed = speed;
                if speed == 0 {
                    Self::clear_mod_m(track, mixer, track_id);
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
                // `ply_note` records the raw command key as `track->key`.
                track.key = key;
                // A successful note with an LFO delay is initialized from the
                // reset modulation state. Use a temporary view for allocation
                // so a refused note cannot mutate the real track or its live
                // voices before acceptance is known.
                let mut note_track = track.clone();
                if note_track.lfo_delay != 0 {
                    note_track.mod_m = 0;
                    note_track.lfo_speed_c = 0;
                }
                if Self::note_on(song, &note_track, mixer, track_id, key, velocity, gate) {
                    // Once allocation succeeds, reload the LFO delay. A
                    // nonzero delay also resets the phase and clears any live
                    // modulation, mirroring the inline `clear_modM` call in
                    // `ply_note` (`m4a_1.s:1732`..`:1738`). Refused notes
                    // return before this block and leave that state untouched.
                    track.lfo_delay_c = track.lfo_delay;
                    if track.lfo_delay != 0 {
                        Self::clear_mod_m(track, mixer, track_id);
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
                // `ply_patt`: cap nesting at 3; beyond that, treat it as
                // `FINE` (`ply_patt_done: b ply_fine`, `m4a_1.s:865`).
                if usize::from(track.pattern_level) >= track.pattern_stack.len() {
                    mixer.release_track(track_id);
                    track.ended = true;
                } else {
                    track.pattern_stack[track.pattern_level as usize] = track.cursor;
                    track.pattern_level += 1;
                    track.cursor = target;
                }
            }
            Event::PatternEnd => {
                // `ply_pend`: a stray PEND with nothing on the stack is a
                // no-op (`m4a_1.s:872`).
                if track.pattern_level > 0 {
                    track.pattern_level -= 1;
                    track.cursor = track.pattern_stack[track.pattern_level as usize];
                }
            }
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
            Event::Repeat { count, target } => {
                // `ply_rept`: `count == 0` is an unconditional, uncounted
                // jump (`m4a_1.s:889`..`:893`); otherwise loop `count` times
                // via the shared `rep_n` counter, then fall through once.
                if count == 0 {
                    track.cursor = target;
                } else {
                    track.rep_n = track.rep_n.wrapping_add(1);
                    if track.rep_n < count {
                        track.cursor = target;
                    } else {
                        track.rep_n = 0;
                    }
                }
            }
            Event::MemAcc {
                op,
                addr,
                value,
                target,
            } => Self::exec_memacc(mem_acc, track, op, addr, value, target),
            // Every `XCMD` sub-command other than `xIECV`/`xIECL` (matched
            // above), and `PORT` (portamento), are decoded for stream
            // fidelity but not executed by this slice.
            //
            // That is deliberate deferral, not an absence of behaviour to
            // defer: `ply_port` writes a byte to a CGB sound register
            // selected by its first operand (`m4a_1.s:1056`..`:1068`; see
            // `Event::Port`'s docs, `sequence.rs:87`..`:89`), and
            // `gXcmdTable` (`m4a_tables.c:291`..`:307`) dispatches 12 real
            // handlers behind its two `ply_xxx` stubs. The justification is
            // reachability instead: `tools/mid2agb` emits only `xIECV` and
            // `xIECL` and never a `PORT` or any other `XCMD`
            // (`tools/mid2agb/agb.cpp:338`..`:341`; module docs, and
            // `crates/assets/src/audio.rs`'s "Deliberately deferred"), so
            // these arms are unreachable for canonical pack data.
            Event::Xcmd { .. } | Event::Port { .. } => {}
        }
    }

    /// `ply_memacc` (`m4a.c:1437`..`:1521`): execute one `MEMACC` op against
    /// the shared accumulator area.
    ///
    /// `op` `0..=5` mutate a cell and never branch; `op` `6..=17` compare a
    /// cell and take `target` (like [`Event::Goto`]) when the comparison
    /// holds. `target` is `None` for the `0..=5` (non-branching) ops and
    /// `Some` for `6..=17` — see [`Event::MemAcc`]'s own docs — but this
    /// reads it defensively rather than assuming that pairing, since `op` is
    /// a raw `u8` on the wire rather than a checked enum by the time it
    /// reaches here.
    ///
    /// # Fail-safe out-of-range handling
    ///
    /// Upstream indexes the accumulator with a raw, unchecked byte offset —
    /// an out-of-range `addr` (or, for the `Mem*` cell-vs-cell ops, `data`)
    /// is an actual OOB C read/write, and no canonical song ever produces
    /// one (`tools/mid2agb` only ever emits offsets `0..=15`). This port has
    /// no such thing as an OOB array access to imitate, so it fails safe
    /// instead: an out-of-range index makes the whole instruction a no-op —
    /// no cell is written, and a comparison op falls through as "not taken",
    /// exactly like a `false` comparison.
    fn exec_memacc(
        mem_acc: &mut MemAccArea,
        track: &mut TrackState,
        op: u8,
        addr: u8,
        data: u8,
        target: Option<usize>,
    ) {
        let addr = usize::from(addr);
        let Some(&cell) = mem_acc.get(addr) else {
            return;
        };
        // The `Mem*` (cell-vs-cell) op forms use `data` as a second cell
        // index rather than a literal; read it through the same fail-safe
        // bound check.
        let mem_cell = |mem_acc: &MemAccArea, data: u8| mem_acc.get(usize::from(data)).copied();

        let taken = match op {
            0 => {
                // `mem_set`.
                mem_acc[addr] = data;
                return;
            }
            1 => {
                // `mem_add`.
                mem_acc[addr] = cell.wrapping_add(data);
                return;
            }
            2 => {
                // `mem_sub`.
                mem_acc[addr] = cell.wrapping_sub(data);
                return;
            }
            3 => {
                // `mem_mem_set`.
                let Some(other) = mem_cell(mem_acc, data) else {
                    return;
                };
                mem_acc[addr] = other;
                return;
            }
            4 => {
                // `mem_mem_add`.
                let Some(other) = mem_cell(mem_acc, data) else {
                    return;
                };
                mem_acc[addr] = cell.wrapping_add(other);
                return;
            }
            5 => {
                // `mem_mem_sub`.
                let Some(other) = mem_cell(mem_acc, data) else {
                    return;
                };
                mem_acc[addr] = cell.wrapping_sub(other);
                return;
            }
            6 => cell == data,  // mem_beq
            7 => cell != data,  // mem_bne
            8 => cell > data,   // mem_bhi
            9 => cell >= data,  // mem_bhs
            10 => cell <= data, // mem_bls
            11 => cell < data,  // mem_blo
            12 => {
                // `mem_mem_beq`.
                let Some(other) = mem_cell(mem_acc, data) else {
                    return;
                };
                cell == other
            }
            13 => {
                // `mem_mem_bne`.
                let Some(other) = mem_cell(mem_acc, data) else {
                    return;
                };
                cell != other
            }
            14 => {
                // `mem_mem_bhi`.
                let Some(other) = mem_cell(mem_acc, data) else {
                    return;
                };
                cell > other
            }
            15 => {
                // `mem_mem_bhs`.
                let Some(other) = mem_cell(mem_acc, data) else {
                    return;
                };
                cell >= other
            }
            16 => {
                // `mem_mem_bls`.
                let Some(other) = mem_cell(mem_acc, data) else {
                    return;
                };
                cell <= other
            }
            17 => {
                // `mem_mem_blo`.
                let Some(other) = mem_cell(mem_acc, data) else {
                    return;
                };
                cell < other
            }
            // `ply_memacc`'s `default: return;` (`m4a.c:1508`..`:1509`).
            //
            // The *assets* path cannot reach this arm: the pack decoder
            // rejects any op outside `0..=17`
            // (`MemAccOp`/`MemAccCondition::from_byte`,
            // `crates/assets/src/audio/song.rs`). The raw-ROM decoder is the
            // concrete path that can: `Decoder::decode`'s `MEMACC` arm
            // (`crates/audio/src/sequence.rs:280`..`:315`) passes the op byte
            // through unchecked, and only `6..=17` (`MEMACC_COND_OPS`,
            // `sequence.rs:161`) consumes a jump target — so an op `>= 18` in
            // a malformed or non-mid2agb byte program arrives here with
            // `target: None` and must simply do nothing, exactly as upstream
            // does.
            _ => return,
        };

        if taken {
            // `cond_true`: take the jump exactly like `GOTO`
            // (`ply_goto`, `m4a_1.s:831`..`:849`), reusing the same cursor
            // assignment `Event::Goto` uses above.
            if let Some(target) = target {
                track.cursor = target;
            }
        }
        // `cond_false` (`m4a.c:1519`..`:1520`): upstream skips the 4-byte
        // jump-target operand and falls through to the next command. This
        // decoder already folds that operand into this same `Event::MemAcc`
        // (see `Event::MemAcc`'s docs), so `track.cursor` already points at
        // the next decoded event — doing nothing here *is* that fallthrough.
    }

    /// `clear_modM` (`m4a_1.s:1859`): zero the LFO's phase and output, then
    /// reapply whichever of pitch/volume the current `MODT` targets so the
    /// reset is immediately audible.
    fn clear_mod_m(track: &mut TrackState, mixer: &mut Mixer, track_id: usize) {
        track.mod_m = 0;
        track.lfo_speed_c = 0;
        if track.mod_type == 0 {
            Self::apply_track_pitch(track, mixer, track_id);
        } else {
            Self::apply_track_volume(track, mixer, track_id);
        }
    }

    /// Advance the per-tick LFO triangle wave and, on a change, reapply it
    /// to pitch or volume.
    ///
    /// Behavioural port of `MPlayMain`'s wait-tick tail (`m4a_1.s:1285`..
    /// `:1330`): skips while there is no rate or depth, holds during the
    /// post-note-on delay, then advances a wrapping `0..=255` phase and
    /// derives a signed triangle value from it, scaled by depth `(no-verbatim)`.
    fn apply_lfo(track: &mut TrackState, mixer: &mut Mixer, track_id: usize) {
        if track.lfo_speed == 0 || track.mod_depth == 0 {
            return;
        }
        if track.lfo_delay_c > 0 {
            track.lfo_delay_c -= 1;
            return;
        }

        // The asm adds `lfoSpeed` into `lfoSpeedC` in a wide register, stores
        // only the low byte back (`strb`, `m4a_1.s:1298`..`:1300`), but keeps
        // the FULL sum live in `r1` for the falling-half mirror. Carry that
        // untruncated `u16` sum (up to 510) into `lfo_triangle`.
        let full_sum = u16::from(track.lfo_speed_c) + u16::from(track.lfo_speed);
        #[allow(clippy::cast_possible_truncation)]
        {
            track.lfo_speed_c = full_sum as u8;
        }
        let value = lfo_triangle(full_sum);
        let raw = (i32::from(track.mod_depth) * value) >> 6;
        // `strb` truncates the product to a byte before it is compared and
        // stored, so only the low 8 bits (reinterpreted as signed) survive.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let truncated = raw as u32 as u8;
        let new_mod_m = i8::from_ne_bytes([truncated]);
        if new_mod_m == track.mod_m {
            return;
        }
        track.mod_m = new_mod_m;
        if track.mod_type == 0 {
            Self::apply_track_pitch(track, mixer, track_id);
        } else {
            Self::apply_track_volume(track, mixer, track_id);
        }
    }

    /// Re-run `TrkVolPitSet`'s volume half for the track and push the new
    /// `volMR`/`volML` onto its live voices (`MPT_FLG_VOLCHG`,
    /// `m4a_1.s:1391`..`:1394`).
    fn apply_track_volume(track: &TrackState, mixer: &mut Mixer, track_id: usize) {
        let (vol_mr, vol_ml) = track_volume(track);
        mixer.set_track_volume(track_id, vol_mr, vol_ml);
    }

    /// Re-run `TrkVolPitSet`'s pitch half for the track and push the new
    /// `keyM`/`pitM` onto its live voices (`MPT_FLG_PITCHG`,
    /// `m4a_1.s:1403`..`:1451`).
    fn apply_track_pitch(track: &TrackState, mixer: &mut Mixer, track_id: usize) {
        let (key_m, pit_m) = track_pitch(track);
        mixer.set_track_pitch(track_id, key_m, pit_m);
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

        // pitch_key + keyM, floored at 0 (`bpl _081DDCA0; movs r3, 0`), then
        // the sum is passed to `MidiKeyToFreq`'s `u8 key` param, truncating
        // the register to its low byte — so a sum above 255 wraps modulo
        // 256, it does not saturate (`ldrb r1, [key]; adds r3, r1, r0; ...
        // bl MidiKeyToFreq`). The same clamp-then-truncate feeds
        // `MidiKeyToCgbFreq` for CGB instruments (`m4a_1.s:1760`..`:1766`).
        // `pitch_key` is the played key for a plain/key-split note, or a
        // rhythm child's own base key (`m4a_1.s:1594`..`:1598`).
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
            // `resolve_instrument` never returns an indirection as the leaf:
            // a KeySplit/Rhythm slot whose own resolved child is itself an
            // indirection is treated as "no note", exactly as upstream's
            // `ply_note` aborts on nested indirection (`m4a_1.s:1604`..
            // `:1609`) rather than recursing.
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

/// `TrkVolPitSet`'s volume half: track vol/pan → right/left channel base
/// volumes (`volMR`/`volML`), with `volX = 0x40`, `panX = 0` (`m4a.c:772`).
/// `mod_type == 1` (amplitude/tremolo) scales the volume term by `modM`;
/// `mod_type == 2` (pan) offsets the pan term by it (`m4a.c:774`..`:780`).
fn track_volume(track: &TrackState) -> (u8, u8) {
    let mut x = (u32::from(track.vol) * VOL_X) >> 5;
    if track.mod_type == 1 {
        // `modM + 128` is always `0..=255` since `modM` is a signed byte.
        let factor = u32::try_from(i32::from(track.mod_m) + 128).unwrap_or(0);
        x = (x * factor) >> 7;
    }

    let mut y = 2 * i32::from(track.pan);
    if track.mod_type == 2 {
        y += i32::from(track.mod_m);
    }
    let y = y.clamp(-128, 127);
    // `(y + 128)` and `(127 - y)` are both in `0..=255`.
    let vol_mr = (u32::try_from(y + 128).unwrap_or(0) * x) >> 8;
    let vol_ml = (u32::try_from(127 - y).unwrap_or(0) * x) >> 8;
    // Upstream stores the `>> 8` result straight into the `u8` fields
    // `volMR`/`volML` (`m4a.c:787`..`:788`, `m4a_internal.h:290`..`:291`): the
    // byte store truncates modulo 256, so a tremolo (`modT == 1`) peak past
    // `0xFF` wraps rather than saturating (e.g. a raw 504 stores 248, not 255).
    (
        u8::try_from(vol_mr & 0xFF).unwrap_or(0),
        u8::try_from(vol_ml & 0xFF).unwrap_or(0),
    )
}

/// `TrkVolPitSet`'s pitch half: track key-shift/bend/tune → integer key offset
/// (`keyM`) and 8-bit fine adjust (`pitM`), with `keyShiftX`/`pitX = 0`
/// (`m4a.c:791`). `mod_type == 0` (pitch/vibrato) adds `16 * modM`
/// (`m4a.c:800`..`:801`).
fn track_pitch(track: &TrackState) -> (i32, u8) {
    let bend = i32::from(track.bend) * i32::from(track.bend_range);
    let mut x = (i32::from(track.tune) + bend) * 4 + (i32::from(track.key_shift) << 8);
    if track.mod_type == 0 {
        x += 16 * i32::from(track.mod_m);
    }
    // Hardware stores `keyM = x >> 8` into a `u8` field
    // (`m4a_internal.h:282`) and reads it back with a signed byte load
    // (`ldrsb r0, [keyM]`, `m4a_1.s:1762`): the effective offset is
    // `(s8)((x >> 8) & 0xFF)`, wrapping modulo 256 rather than staying full
    // width. Truncate to a byte, then reinterpret those bits as signed.
    let key_m_byte = u8::try_from((x >> 8) & 0xFF).unwrap_or(0);
    let key_m = i32::from(i8::from_le_bytes([key_m_byte]));
    let pit_m = u8::try_from(x & 0xFF).unwrap_or(0);
    (key_m, pit_m)
}

/// The LFO's triangle-wave shape: given the running phase sum
/// `lfoSpeedC + lfoSpeed` (`m4a_1.s:1298`..`:1300`, an untruncated `u16` up to
/// 510), derive the signed slope value later scaled by depth.
///
/// Behavioural port of `MPlayMain`'s inline triangle computation
/// (`m4a_1.s:1301`..`:1311`). The rising-vs-falling branch keys off the
/// *truncated* 8-bit phase (`full_sum & 0xFF`, the byte written back to
/// `lfoSpeedC`), but the falling half computes `0x80 - r1` where `r1` still
/// holds the *full* pre-`strb` sum (`_081DD96E`, `m4a_1.s:1308`..`:1310`). When
/// `lfoSpeed >= 65` that sum can reach the falling half (truncated phase in
/// `[0x40, 0xBF]`) while exceeding 255, so the mirror term stays wide and the
/// slope drops below `-128`. Reproduced faithfully by returning the full signed
/// slope (an `i32`); the caller scales by depth and truncates the product to a
/// byte with `strb`, exactly as the asm does `(no-verbatim)`.
#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
fn lfo_triangle(full_sum: u16) -> i32 {
    let phase = full_sum as u8;
    if (phase.wrapping_sub(0x40) as i8) >= 0 {
        // Falling half: mirror around 0x80 using the FULL untruncated sum
        // (`movs r0, 0x80; subs r2, r0, r1` with `r1 = lfoSpeedC + lfoSpeed`).
        0x80i32 - i32::from(full_sum)
    } else {
        // Rising half: the truncated phase, reinterpreted as signed
        // (`lsls r2, r1, 24; asrs r2, 24`).
        i32::from(phase as i8)
    }
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
        // A one-note loop: VOICE, [loop:] Note, Wait, Goto(loop).
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
        // Never finishes: the Goto keeps re-triggering the note.
        assert!(!seq.is_finished());
    }

    // --- MEMACC (`ply_memacc`, `m4a.c:1437`..`:1521`) ----------------------
    //
    // The accumulator has no public accessor (upstream has none either —
    // `mplayInfo->memAccArea` is only ever read back by a later `MEMACC`),
    // so every test below observes cell state indirectly, the same way
    // `goto_loops_the_track_forever` observes a `Goto` target: a branch that
    // takes loops the track (a note, a wait, then the branch) forever, so
    // the sequencer never finishes; a branch that doesn't take falls
    // through to `Fine` and the sequencer does finish.

    /// Runs `seq` for enough frames that a genuinely infinite loop would
    /// still be unfinished, and a genuinely finite track would already have
    /// reached `Fine`.
    fn run_for_a_while(seq: &mut Sequencer) {
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        for _ in 0..200 {
            seq.render_frame(&mut out);
        }
    }

    /// Builds a track that: sets up cell state via `prelude`, then loops
    /// `[loop:] Note, Wait, <branch>` where `<branch>` is
    /// `MemAcc { op, addr, value, target: Some(loop) }` — `target` points at
    /// the `Note`, matching `goto_loops_the_track_forever`'s loop shape —
    /// and falls through to `Fine` when the branch doesn't take. Returns
    /// whether the sequencer is still unfinished after running a while
    /// (`true` == branch taken).
    fn memacc_branch_taken(prelude: Vec<Event>, op: u8, addr: u8, value: u8) -> bool {
        // The loop target is the `Note` this pushes right after `prelude`.
        let loop_index = prelude.len() + 1;
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
            addr,
            value,
            target: Some(loop_index),
        });
        track.push(Event::Fine);

        let song = test_song(vec![track], 150);
        let mut seq = Sequencer::new(song);
        run_for_a_while(&mut seq);
        !seq.is_finished()
    }

    #[test]
    fn memacc_set_then_eq_takes_the_branch_when_equal() {
        // mem_set cell0 = 5; mem_beq cell0 == 5 -> taken (loop forever).
        let prelude = vec![Event::MemAcc {
            op: 0,
            addr: 0,
            value: 5,
            target: None,
        }];
        assert!(memacc_branch_taken(prelude, 6, 0, 5));
    }

    #[test]
    fn memacc_set_then_eq_does_not_take_the_branch_when_unequal() {
        // Same cell state, but compared against a different literal: the
        // same `mem_beq` op must now fall through to `Fine`. *Both*
        // directions of "unequal" are checked, because one alone leaves `==`
        // indistinguishable from the inequality that agrees with it on the
        // equal boundary and on that one side (`>=` if only the below-data
        // row existed, `<=` if only the above-data one did).
        let prelude = || {
            vec![Event::MemAcc {
                op: 0,
                addr: 0,
                value: 5,
                target: None,
            }]
        };
        assert!(
            !memacc_branch_taken(prelude(), 6, 0, 6),
            "5 == 6 should not take mem_beq (cell below data; rules out `<=`/`<`)"
        );
        assert!(
            !memacc_branch_taken(prelude(), 6, 0, 4),
            "5 == 4 should not take mem_beq (cell above data; rules out `>=`/`>`)"
        );
    }

    #[test]
    fn memacc_add_wraps_like_a_u8() {
        // mem_set cell0 = 250; mem_add cell0 += 10 -> wraps to 4 (not a
        // saturating/panicking 255). mem_beq cell0 == 4 -> taken.
        let prelude = vec![
            Event::MemAcc {
                op: 0,
                addr: 0,
                value: 250,
                target: None,
            },
            Event::MemAcc {
                op: 1,
                addr: 0,
                value: 10,
                target: None,
            },
        ];
        assert!(memacc_branch_taken(prelude, 6, 0, 4));
    }

    #[test]
    fn memacc_sub_wraps_like_a_u8() {
        // mem_set cell0 = 3; mem_sub cell0 -= 10 -> wraps to 249 (not a
        // saturating/panicking 0). mem_beq cell0 == 249 -> taken.
        let prelude = vec![
            Event::MemAcc {
                op: 0,
                addr: 0,
                value: 3,
                target: None,
            },
            Event::MemAcc {
                op: 2,
                addr: 0,
                value: 10,
                target: None,
            },
        ];
        assert!(memacc_branch_taken(prelude, 6, 0, 249));
    }

    #[test]
    fn memacc_mem_set_copies_another_cell() {
        // mem_set cell1 = 9; mem_mem_set cell0 = cell[1] -> cell0 == 9.
        let prelude = vec![
            Event::MemAcc {
                op: 0,
                addr: 1,
                value: 9,
                target: None,
            },
            Event::MemAcc {
                op: 3,
                addr: 0,
                value: 1,
                target: None,
            },
        ];
        assert!(memacc_branch_taken(prelude, 6, 0, 9));
    }

    #[test]
    fn memacc_mem_add_wraps_like_a_u8() {
        // mem_set cell0 = 10; mem_set cell1 = 250;
        // mem_mem_add cell0 += cell[1] -> 10 + 250 wraps to 4.
        let prelude = vec![
            Event::MemAcc {
                op: 0,
                addr: 0,
                value: 10,
                target: None,
            },
            Event::MemAcc {
                op: 0,
                addr: 1,
                value: 250,
                target: None,
            },
            Event::MemAcc {
                op: 4,
                addr: 0,
                value: 1,
                target: None,
            },
        ];
        assert!(memacc_branch_taken(prelude, 6, 0, 4));
    }

    #[test]
    fn memacc_mem_sub_wraps_like_a_u8() {
        // mem_set cell0 = 3; mem_set cell1 = 250;
        // mem_mem_sub cell0 -= cell[1] -> 3 - 250 wraps to 9.
        let prelude = vec![
            Event::MemAcc {
                op: 0,
                addr: 0,
                value: 3,
                target: None,
            },
            Event::MemAcc {
                op: 0,
                addr: 1,
                value: 250,
                target: None,
            },
            Event::MemAcc {
                op: 5,
                addr: 0,
                value: 1,
                target: None,
            },
        ];
        assert!(memacc_branch_taken(prelude, 6, 0, 9));
    }

    #[test]
    fn memacc_hi_comparison_takes_and_does_not_take() {
        // `mem_bhi` (op 8) is a strict `>`. Three rows: above, equal, and
        // below the data operand. The below-data row is what separates `>`
        // from `!=`, which agrees with it on the other two.
        let prelude = || {
            vec![Event::MemAcc {
                op: 0,
                addr: 0,
                value: 10,
                target: None,
            }]
        };
        assert!(
            memacc_branch_taken(prelude(), 8, 0, 5),
            "10 > 5 should take mem_bhi"
        );
        assert!(
            !memacc_branch_taken(prelude(), 8, 0, 10),
            "10 > 10 should not take mem_bhi (>, not >=)"
        );
        assert!(
            !memacc_branch_taken(prelude(), 8, 0, 11),
            "10 > 11 should not take mem_bhi (cell below data; rules out `!=`)"
        );
    }

    #[test]
    fn memacc_out_of_range_address_is_a_safe_no_op_not_a_panic() {
        // addr 200 is past the 16-cell area. `mem_set` must not panic, must
        // not corrupt any real cell, and must not derail control flow: the
        // track still reaches `Fine` on schedule.
        let track = vec![
            Event::Voice(0),
            Event::MemAcc {
                op: 0,
                addr: 200,
                value: 42,
                target: None,
            },
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 1,
            },
            Event::Wait(24),
            Event::Fine,
        ];
        let song = test_song(vec![track], 150);
        let mut seq = Sequencer::new(song);
        run_for_a_while(&mut seq);
        assert!(
            seq.is_finished(),
            "an out-of-range MEMACC must not hang the track"
        );
    }

    #[test]
    fn memacc_out_of_range_comparison_address_does_not_take_the_branch() {
        // addr 200 on a comparison op: no valid cell to compare, so this
        // must fail safe as "not taken" (fall through to Fine), never panic.
        assert!(!memacc_branch_taken(Vec::new(), 6, 200, 0));
    }

    #[test]
    fn memacc_out_of_range_mem_cell_index_is_a_safe_no_op() {
        // mem_mem_set cell0 = cell[200]: `data` (not `addr`) is the
        // out-of-range index this time. cell0 must be left at its zeroed
        // default rather than reading garbage, so a subsequent `cell0 == 0`
        // comparison still takes.
        let prelude = vec![Event::MemAcc {
            op: 3,
            addr: 0,
            value: 200,
            target: None,
        }];
        assert!(memacc_branch_taken(prelude, 6, 0, 0));
    }

    #[test]
    fn memacc_ne_comparison_takes_and_does_not_take() {
        // `mem_bne` (op 7). The not-taken case is the *equal* boundary, so
        // this fails if `!=` were ever written as `==`. Both unequal
        // directions are taken cases: with only the above-data one, `>`
        // would still pass every row.
        let prelude = || {
            vec![Event::MemAcc {
                op: 0,
                addr: 0,
                value: 10,
                target: None,
            }]
        };
        assert!(
            memacc_branch_taken(prelude(), 7, 0, 5),
            "10 != 5 should take mem_bne"
        );
        assert!(
            memacc_branch_taken(prelude(), 7, 0, 11),
            "10 != 11 should take mem_bne (cell below data; rules out `>`/`>=`)"
        );
        assert!(
            !memacc_branch_taken(prelude(), 7, 0, 10),
            "10 != 10 should not take mem_bne"
        );
    }

    #[test]
    fn memacc_hs_comparison_includes_the_equal_boundary() {
        // `mem_bhs` (op 9) is `>=`. Taking at the equal boundary is what
        // separates it from `mem_bhi`'s `>`; not taking one below separates
        // it from `mem_bls`'s `<=`; taking one *above* separates it from
        // `mem_beq`'s `==`, which agrees with it on the other two rows.
        let prelude = || {
            vec![Event::MemAcc {
                op: 0,
                addr: 0,
                value: 10,
                target: None,
            }]
        };
        assert!(
            memacc_branch_taken(prelude(), 9, 0, 10),
            "10 >= 10 should take mem_bhs (>=, not >)"
        );
        assert!(
            memacc_branch_taken(prelude(), 9, 0, 5),
            "10 >= 5 should take mem_bhs (cell above data; rules out `==`)"
        );
        assert!(
            !memacc_branch_taken(prelude(), 9, 0, 11),
            "10 >= 11 should not take mem_bhs"
        );
    }

    #[test]
    fn memacc_ls_comparison_includes_the_equal_boundary() {
        // `mem_bls` (op 10) is `<=`. Taking at the equal boundary separates
        // it from `mem_blo`'s `<`; not taking one above separates it from
        // `mem_bhs`'s `>=`; taking one *below* separates it from
        // `mem_beq`'s `==`, which agrees with it on the other two rows.
        let prelude = || {
            vec![Event::MemAcc {
                op: 0,
                addr: 0,
                value: 10,
                target: None,
            }]
        };
        assert!(
            memacc_branch_taken(prelude(), 10, 0, 10),
            "10 <= 10 should take mem_bls (<=, not <)"
        );
        assert!(
            memacc_branch_taken(prelude(), 10, 0, 11),
            "10 <= 11 should take mem_bls (cell below data; rules out `==`)"
        );
        assert!(
            !memacc_branch_taken(prelude(), 10, 0, 9),
            "10 <= 9 should not take mem_bls"
        );
    }

    #[test]
    fn memacc_lo_comparison_excludes_the_equal_boundary() {
        // `mem_blo` (op 11) is a strict `<`: the equal boundary must not
        // take, which is what separates it from `mem_bls`'s `<=`. The
        // above-data row separates it from `!=`, which agrees with it on the
        // other two.
        let prelude = || {
            vec![Event::MemAcc {
                op: 0,
                addr: 0,
                value: 10,
                target: None,
            }]
        };
        assert!(
            memacc_branch_taken(prelude(), 11, 0, 11),
            "10 < 11 should take mem_blo"
        );
        assert!(
            !memacc_branch_taken(prelude(), 11, 0, 10),
            "10 < 10 should not take mem_blo (<, not <=)"
        );
        assert!(
            !memacc_branch_taken(prelude(), 11, 0, 5),
            "10 < 5 should not take mem_blo (cell above data; rules out `!=`)"
        );
    }

    #[test]
    fn memacc_cell_vs_cell_comparisons_cover_both_directions() {
        // Ops 12..=17 (`mem_mem_beq`..`mem_mem_blo`) read `data` as a second
        // *cell index* rather than a literal. Each case below sets cell0 to
        // 10 and cell1 to `other`, then compares cell0 against cell1.
        //
        // Every op gets all three orderings of cell0 against cell1 — below,
        // equal, above — because the six relations are pairwise separated
        // only by their full three-row truth vectors: `==` is
        // `(F, T, F)`, `!=` `(T, F, T)`, `>` `(F, F, T)`, `>=` `(F, T, T)`,
        // `<=` `(T, T, F)` and `<` `(T, F, F)` over (below, equal, above).
        // Every pair differs in at least one column, so rewriting any op as
        // any other relation fails at least one row here — checked by
        // mutation, not just argued: all 30 swaps (each of ops 12..=17
        // rewritten as each of the other five relations) fail this test.
        // The literal-operand ops 6..=11 above are pinned the same way,
        // three orderings each, and their 30 swaps all fail too. Two rows
        // per op was *not* enough: it left one survivor per op, since a
        // taken/not-taken pair straddling one boundary cannot tell an op
        // apart from the relation that agrees with it on both rows.
        let prelude = |other: u8| {
            vec![
                Event::MemAcc {
                    op: 0,
                    addr: 0,
                    value: 10,
                    target: None,
                },
                Event::MemAcc {
                    op: 0,
                    addr: 1,
                    value: other,
                    target: None,
                },
            ]
        };
        // (op, cell1, expected-taken, what the row pins down)
        let cases: [(u8, u8, bool, &str); 18] = [
            (12, 11, false, "mem_mem_beq: 10 == 11 does not take"),
            (12, 10, true, "mem_mem_beq: 10 == 10 takes"),
            (
                12,
                5,
                false,
                "mem_mem_beq: 10 == 5 does not take (not `>=`)",
            ),
            (13, 11, true, "mem_mem_bne: 10 != 11 takes"),
            (13, 10, false, "mem_mem_bne: 10 != 10 does not take"),
            (13, 5, true, "mem_mem_bne: 10 != 5 takes (not `<`)"),
            (
                14,
                11,
                false,
                "mem_mem_bhi: 10 > 11 does not take (not `!=`)",
            ),
            (
                14,
                10,
                false,
                "mem_mem_bhi: 10 > 10 does not take (>, not >=)",
            ),
            (14, 5, true, "mem_mem_bhi: 10 > 5 takes"),
            (15, 11, false, "mem_mem_bhs: 10 >= 11 does not take"),
            (15, 10, true, "mem_mem_bhs: 10 >= 10 takes (>=, not >)"),
            (15, 5, true, "mem_mem_bhs: 10 >= 5 takes (not `==`)"),
            (16, 11, true, "mem_mem_bls: 10 <= 11 takes (not `==`)"),
            (16, 10, true, "mem_mem_bls: 10 <= 10 takes (<=, not <)"),
            (16, 9, false, "mem_mem_bls: 10 <= 9 does not take"),
            (17, 11, true, "mem_mem_blo: 10 < 11 takes"),
            (
                17,
                10,
                false,
                "mem_mem_blo: 10 < 10 does not take (<, not <=)",
            ),
            (17, 5, false, "mem_mem_blo: 10 < 5 does not take (not `!=`)"),
        ];
        for (op, other, expected, why) in cases {
            assert_eq!(
                memacc_branch_taken(prelude(other), op, 0, 1),
                expected,
                "{why}"
            );
        }
    }

    #[test]
    fn memacc_out_of_range_mem_cell_index_on_a_comparison_does_not_take() {
        // The `data`-side bound check on the cell-vs-cell *comparison* arms
        // (`memacc_out_of_range_mem_cell_index_is_a_safe_no_op` covers only
        // the writing arm, op 3). cell0 is 10 and cell index 200 is past the
        // 16-cell area, so there is no second operand to compare: every one
        // of ops 12..=17 must fail safe rather than panic, falling through
        // as "not taken", exactly like a `false` comparison (`exec_memacc`'s
        // "Fail-safe out-of-range handling") — so the track reaches `Fine`.
        for op in 12..=17 {
            let prelude = vec![Event::MemAcc {
                op: 0,
                addr: 0,
                value: 10,
                target: None,
            }];
            assert!(
                !memacc_branch_taken(prelude, op, 0, 200),
                "op {op} with an out-of-range cell index must not take"
            );
        }
    }

    #[test]
    fn memacc_op_past_the_last_real_one_is_a_no_op() {
        // `ply_memacc`'s `default: return;` (`m4a.c:1508`..`:1509`).
        //
        // The assets pack decoder rejects ops outside `0..=17`, but the
        // raw-ROM decoder does not: `Decoder::decode`'s `MEMACC` arm
        // (`crates/audio/src/sequence.rs:280`..`:315`) passes the op byte
        // through unchecked and consumes a jump target only for
        // `MEMACC_COND_OPS` (`6..=17`, `sequence.rs:161`), so an op `>= 18`
        // reaches `exec_memacc` with `target: None` — exactly the shape
        // built here. It must write nothing, so a following
        // `mem_beq cell0, 0` still sees the zeroed cell and takes.
        for op in [18, 255] {
            let prelude = vec![Event::MemAcc {
                op,
                addr: 0,
                value: 42,
                target: None,
            }];
            assert!(
                memacc_branch_taken(prelude, 6, 0, 0),
                "op {op} must not write a cell"
            );
        }
    }

    #[test]
    fn memacc_taken_branch_without_a_target_falls_through() {
        // Defensive pairing check. `Event::MemAcc`'s docs pair `6..=17` with
        // `Some(target)`, but `op` arrives as a raw wire byte, so
        // `exec_memacc` reads `target` rather than assuming it. A comparison
        // that *does* hold but carries no target must behave like an ordinary
        // fallthrough: keep going to the next event, not stall or panic.
        let track = vec![
            Event::Voice(0),
            // A freshly zeroed area makes `mem_beq cell0, 0` true.
            Event::MemAcc {
                op: 6,
                addr: 0,
                value: 0,
                target: None,
            },
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 1,
            },
            Event::Wait(24),
            Event::Fine,
        ];
        let mut seq = Sequencer::new(test_song(vec![track], 150));
        run_for_a_while(&mut seq);
        assert!(
            seq.is_finished(),
            "a taken branch with no target must fall through to Fine"
        );
    }

    #[test]
    fn memacc_accumulator_resets_between_sessions() {
        // A first sequencer/song sets cell0 to a nonzero value and finishes.
        let set_track = vec![
            Event::MemAcc {
                op: 0,
                addr: 0,
                value: 42,
                target: None,
            },
            Event::Fine,
        ];
        let mut first = Sequencer::new(test_song(vec![set_track], 150));
        run_for_a_while(&mut first);
        assert!(first.is_finished());

        // A brand-new Sequencer gets a freshly zeroed area, so `first`'s
        // 42 is not carried forward. This is the deliberate divergence
        // documented on `MemAccArea`: upstream's single global
        // `gMPlayMemAccArea` is zeroed only by its static initializer
        // (`m4a.c:20`) and *would* carry the value across songs. No
        // canonical song reads a cell, so nothing can tell the difference —
        // what this pins down is that the port's rule is the zeroing one.
        // mem_beq cell0 == 0 must take on a never-touched accumulator.
        assert!(memacc_branch_taken(Vec::new(), 6, 0, 0));
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
    fn volume_change_updates_a_held_notes_gains() {
        // Start a tied note at full volume, then drop VOL mid-note: the live
        // voice's base gains must follow (before the fix, note-on baked the
        // gains once and later VOL commands never touched the voice).
        let track = vec![
            Event::Voice(0),
            Event::Volume(127),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 0,
            },
            Event::Wait(4),
            Event::Volume(20),
            Event::Wait(4),
            Event::Fine,
        ];
        let mut seq = Sequencer::new(held_note_song(track));
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        seq.render_frame(&mut out); // tick 1: note on at full volume
        let (loud_r, loud_l) = seq.mixer.voices()[0].base_volume();
        // Advance past the mid-note `VOL 20` (fires once the first wait drains).
        for _ in 0..5 {
            seq.render_frame(&mut out);
        }
        let (soft_r, soft_l) = seq.mixer.voices()[0].base_volume();
        assert!(
            soft_r < loud_r && soft_l < loud_l,
            "VOL must lower the held note ({soft_r},{soft_l} vs {loud_r},{loud_l})",
        );
        assert!(soft_r > 0 && soft_l > 0);
    }

    #[test]
    fn bend_changes_a_held_notes_frequency() {
        // Start a tied note, then BEND up mid-note: the live voice's frequency
        // must rise (before the fix, BEND only mutated track state).
        let wave = Arc::new(WaveData::looping(1 << 20, 0, vec![100; SAMPLES_PER_FRAME]));
        let voices = vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))];
        let track = vec![
            Event::Voice(0),
            Event::BendRange(2),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 0,
            },
            Event::Wait(4),
            Event::Bend(63),
            Event::Wait(4),
            Event::Fine,
        ];
        let mut seq = Sequencer::new(Song::new(voices, vec![track], 150));
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        seq.render_frame(&mut out); // tick 1: note on, unbent
        let base_freq = seq.mixer.voices()[0].frequency();
        for _ in 0..5 {
            seq.render_frame(&mut out);
        }
        let bent_freq = seq.mixer.voices()[0].frequency();
        assert!(
            bent_freq > base_freq,
            "BEND up must raise the held note's frequency ({bent_freq} vs {base_freq})",
        );
    }

    #[test]
    fn track_pitch_key_m_stays_full_width_within_a_signed_byte() {
        // In-range key offsets pass through untouched: `keyM = x >> 8` fits a
        // signed byte, so truncation is a no-op.
        let mut track = TrackState::new();
        track.key_shift = 127; // x >> 8 == 127
        assert_eq!(track_pitch(&track).0, 127);
        track.key_shift = -128; // x >> 8 == -128
        assert_eq!(track_pitch(&track).0, -128);
    }

    #[test]
    fn track_pitch_key_m_wraps_through_a_signed_byte() {
        // KEYSH 127 with a positive bend pushes `x >> 8` to 128, one past the
        // signed-byte range. Hardware stores it in a `u8` and reads it back
        // signed, so the effective offset wraps to `(s8)128 == -128`, not +128.
        let mut track = TrackState::new();
        track.key_shift = 127;
        track.bend = 1;
        track.bend_range = 64; // bend == 64; (64*4 + 127*256) >> 8 == 128
        assert_eq!(track_pitch(&track).0, -128);
    }

    #[test]
    fn track_volume_in_range_channels_pass_through() {
        // A plain centred track stays well within a byte, so truncation is a
        // no-op: guards the fix against changing ordinary volumes.
        let track = TrackState::new(); // vol 127, pan 0, modT 0
                                       // x = (127*0x40)>>5 = 254; y = 0.
                                       // volMR = (128*254)>>8 = 127; volML = (127*254)>>8 = 126.
        assert_eq!(track_volume(&track), (127, 126));
    }

    #[test]
    fn track_volume_tremolo_peak_wraps_through_a_byte() {
        // TrkVolPitSet stores `(u32)((y+128)*x)>>8` straight into the u8 fields
        // volMR/volML (m4a.c:787-788, m4a_internal.h:290-291), so a tremolo peak
        // past 0xFF wraps modulo 256 rather than saturating at 255.
        let mut track = TrackState::new();
        track.vol = 127;
        track.mod_type = 1; // amplitude LFO scales the volume term
        track.mod_m = 127; // factor = 127+128 = 255
        track.pan = 63; // hard right: y = 126, y+128 = 254
                        // x = (127*0x40)>>5 = 254; x = (254*255)>>7 = 506.
                        // raw volMR = (254*506)>>8 = 502 -> 502 & 0xFF = 246 (not 255).
                        // raw volML = (1*506)>>8 = 1, unaffected.
        assert_eq!(track_volume(&track), (246, 1));
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
        // A tied note with MOD depth and LFOS set (default MODT = pitch):
        // the held voice's live frequency must eventually diverge from its
        // unmodulated note-on value as the LFO's triangle wave ramps up.
        let wave = Arc::new(WaveData::looping(1 << 20, 0, vec![100; SAMPLES_PER_FRAME]));
        let voices = vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))];
        let track = vec![
            Event::Voice(0),
            Event::Modulation(40),
            Event::LfoSpeed(30),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 0,
            },
            Event::Wait(96),
            Event::Fine,
        ];
        let mut seq = Sequencer::new(Song::new(voices, vec![track], 150));
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        seq.render_frame(&mut out); // tick 1: note on, unmodulated
        let base_freq = seq.mixer.voices()[0].frequency();

        let mut changed = false;
        for _ in 0..40 {
            seq.render_frame(&mut out);
            if seq.mixer.voices()[0].frequency() != base_freq {
                changed = true;
                break;
            }
        }
        assert!(changed, "LFO should eventually bend the held note's pitch");
    }

    #[test]
    fn lfo_measurably_changes_the_rendered_output_vs_no_lfo() {
        // Isolate the LFO's own contribution: an otherwise-identical track
        // with and without MOD/LFOS must diverge in its rendered samples —
        // not merely because the wave keeps playing (both renders share the
        // same starting phase and duration).
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
            track.push(Event::Note {
                key: 60,
                velocity: 127,
                gate: 0,
            });
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
        // LFODL holds the LFO inactive (mod_m stays 0) for the first N
        // ticks after note-on; the frequency should not move until then.
        let wave = Arc::new(WaveData::looping(1 << 20, 0, vec![100; SAMPLES_PER_FRAME]));
        let voices = vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))];
        let track = vec![
            Event::Voice(0),
            Event::Modulation(60),
            Event::LfoSpeed(80),
            Event::LfoDelay(10),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 0,
            },
            Event::Wait(96),
            Event::Fine,
        ];
        let mut seq = Sequencer::new(Song::new(voices, vec![track], 150));
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        seq.render_frame(&mut out); // tick 1: note on
        let base_freq = seq.mixer.voices()[0].frequency();
        for _ in 0..8 {
            seq.render_frame(&mut out);
            assert_eq!(
                seq.mixer.voices()[0].frequency(),
                base_freq,
                "frequency must not move during the LFO delay"
            );
        }
    }

    #[test]
    fn refused_cgb_note_does_not_reset_track_modulation() {
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

        // Track 0 occupies square 1 at a priority track 1 cannot displace.
        seq.tracks[0].voice = 1;
        seq.tracks[0].priority = 10;
        apply_test_event(
            &mut seq,
            0,
            &Event::Note {
                key: 50,
                velocity: 127,
                gate: 0,
            },
        );

        // Give track 1 a live DirectSound voice whose gain exposes any
        // spurious amplitude-modulation reset caused by the refused note.
        seq.tracks[1].voice = 0;
        seq.tracks[1].mod_type = 1;
        seq.tracks[1].mod_m = 64;
        apply_test_event(
            &mut seq,
            1,
            &Event::Note {
                key: 60,
                velocity: 127,
                gate: 0,
            },
        );
        let modulated_volume = seq.mixer.voices()[0].base_volume();

        seq.tracks[1].voice = 1;
        seq.tracks[1].lfo_delay = 7;
        seq.tracks[1].lfo_delay_c = 3;
        seq.tracks[1].lfo_speed_c = 91;
        apply_test_event(
            &mut seq,
            1,
            &Event::Note {
                key: 70,
                velocity: 127,
                gate: 0,
            },
        );

        assert_eq!(seq.tracks[1].key, 70, "the raw track key still updates");
        assert_eq!(seq.tracks[1].lfo_delay_c, 3);
        assert_eq!(seq.tracks[1].lfo_speed_c, 91);
        assert_eq!(seq.tracks[1].mod_m, 64);
        assert_eq!(seq.mixer.voices()[0].base_volume(), modulated_volume);
        let square1 = seq.mixer.cgb_voices()[CgbChannelNumber::Square1.slot()]
            .as_ref()
            .expect("the original square-1 occupant must remain");
        assert_eq!(square1.track(), 0);
        assert_eq!(square1.midi_key(), 50);

        // An accepted note retains the established note-on reset behaviour.
        seq.tracks[1].priority = 20;
        apply_test_event(
            &mut seq,
            1,
            &Event::Note {
                key: 70,
                velocity: 127,
                gate: 0,
            },
        );
        assert_eq!(seq.tracks[1].lfo_delay_c, 7);
        assert_eq!(seq.tracks[1].lfo_speed_c, 0);
        assert_eq!(seq.tracks[1].mod_m, 0);
        let square1 = seq.mixer.cgb_voices()[CgbChannelNumber::Square1.slot()]
            .as_ref()
            .expect("the higher-priority note must replace square 1");
        assert_eq!(square1.track(), 1);
        assert_eq!(square1.midi_key(), 70);
    }

    #[test]
    fn accepted_cgb_sweep_note_uses_reset_pitch_modulation() {
        let instruments = vec![Instrument::CgbSquare1(SquareTone {
            duty: 2,
            sweep: 0x11, // period 1, add, shift 1
            adsr: CgbAdsr::flat(),
            fixed_rate: false,
        })];
        let song = Song::new(instruments, vec![vec![]], 150);
        let mut seq = Sequencer::new(song);

        seq.tracks[0].mod_type = 0;
        seq.tracks[0].mod_m = 127;
        seq.tracks[0].lfo_delay = 7;
        seq.tracks[0].lfo_speed_c = 91;
        apply_test_event(
            &mut seq,
            0,
            &Event::Note {
                key: 48,
                velocity: 127,
                gate: 0,
            },
        );

        assert_eq!(seq.tracks[0].lfo_delay_c, 7);
        assert_eq!(seq.tracks[0].lfo_speed_c, 0);
        assert_eq!(seq.tracks[0].mod_m, 0);
        let square1 = seq.mixer.cgb_voices()[CgbChannelNumber::Square1.slot()]
            .as_ref()
            .expect("the accepted square-1 note must occupy its channel");
        assert!(
            square1.is_active(),
            "the sweep must initialize from reset pitch modulation, without trigger overflow"
        );
    }

    #[test]
    fn refused_direct_sound_note_does_not_reset_track_modulation() {
        let song = Song::new(vec![direct_sound(100)], vec![vec![], vec![]], 150);
        let mut seq = Sequencer::with_config(song, DEFAULT_MASTER_VOLUME, 2);

        // Start a pitch-modulated track-1 voice, then fill the other pool
        // slot with another priority-10 voice.
        seq.tracks[1].priority = 10;
        seq.tracks[1].mod_m = 32;
        apply_test_event(
            &mut seq,
            1,
            &Event::Note {
                key: 60,
                velocity: 127,
                gate: 0,
            },
        );
        seq.tracks[0].priority = 10;
        apply_test_event(
            &mut seq,
            0,
            &Event::Note {
                key: 50,
                velocity: 127,
                gate: 0,
            },
        );
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
        seq.tracks[1].lfo_delay_c = 3;
        seq.tracks[1].lfo_speed_c = 91;
        apply_test_event(
            &mut seq,
            1,
            &Event::Note {
                key: 70,
                velocity: 127,
                gate: 0,
            },
        );

        assert_eq!(seq.tracks[1].key, 70, "the raw track key still updates");
        assert_eq!(seq.tracks[1].lfo_delay_c, 3);
        assert_eq!(seq.tracks[1].lfo_speed_c, 91);
        assert_eq!(seq.tracks[1].mod_m, 32);
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

        // Raising the incoming priority makes allocation succeed and keeps
        // the accepted-note LFO reset covered for DirectSound too.
        seq.tracks[1].priority = 20;
        apply_test_event(
            &mut seq,
            1,
            &Event::Note {
                key: 70,
                velocity: 127,
                gate: 0,
            },
        );
        assert_eq!(seq.tracks[1].lfo_delay_c, 7);
        assert_eq!(seq.tracks[1].lfo_speed_c, 0);
        assert_eq!(seq.tracks[1].mod_m, 0);
        assert!(seq
            .mixer
            .voices()
            .iter()
            .any(|voice| voice.track() == 1 && voice.midi_key() == 70));
    }

    #[test]
    #[allow(clippy::cast_sign_loss)] // mirrors `apply_lfo`'s `strb` truncation
    fn lfo_triangle_falling_half_uses_the_untruncated_phase_sum() {
        // Regression for the wide-register corner at `_081DD96E`
        // (`m4a_1.s:1308`..`:1310`). With `lfoSpeed >= 65` the running phase
        // sum `lfoSpeedC + lfoSpeed` can exceed 255 while its low byte still
        // lands in the falling half (`0x40..=0xBF`). The asm mirrors the FULL
        // pre-`strb` sum (`0x80 - r1`), not the byte written back to lfoSpeedC.
        //
        // Hand computation for the cited corner, full_sum = 400 (0x190),
        // MOD (mod_depth) = 40:
        //   truncated phase = 400 & 0xFF = 0x90 (144) -> in [0x40,0xBF] -> falling
        //   value = 0x80 - r1 = 128 - 400 = -272        (r1 = full sum, not 144)
        //   raw   = (40 * -272) >> 6 = -10880 >> 6 = -170   (muls; asrs r2, #6)
        //   modM  = (i8)(-170 & 0xFF) = (i8)0x56 = +86      (strb truncation)
        // The old truncated-phase port used 0x80 - 144 = -16 instead, giving
        //   raw = (40 * -16) >> 6 = -640 >> 6 = -10, modM = -10 -- the divergence.
        assert_eq!(lfo_triangle(400), -272);

        // Replicate `apply_lfo`'s depth-scale + `strb` truncation on that slope.
        let value = lfo_triangle(400);
        let raw = (i32::from(40u8) * value) >> 6;
        let modm = i8::from_ne_bytes([raw as u32 as u8]);
        assert_eq!(modm, 86, "falling half must mirror the full 16-bit sum");

        // Sanity: sums <= 255 (no wide corner) still behave as before -- the
        // falling half only reaches here for full_sum in [0x140, 0x1BF].
        assert_eq!(lfo_triangle(0x80), 0); // p=0x80 -> 0x80 - 0x80 == 0
        assert_eq!(lfo_triangle(0x20), 0x20); // rising: (i8)0x20
        assert_eq!(lfo_triangle(0xC0), -64); // rising: (i8)0xC0
    }

    // --- Pattern execution (`PATT`/`PEND`/`REPT`) ---------------------------

    #[test]
    fn pattern_call_renders_identically_to_the_unrolled_track() {
        // `PATT` calls into a subroutine track that ends in `PEND`, which
        // returns to the instruction right after the call — a single-pass
        // call/return must sound identical to inlining the same note.
        let with_pattern = vec![
            Event::Voice(0),   // 0
            Event::Pattern(4), // 1: call the body at 4, return to 2
            Event::Wait(48),   // 2
            Event::Fine,       // 3
            Event::Note {
                // 4: pattern body
                key: 60,
                velocity: 127,
                gate: 8,
            },
            Event::PatternEnd, // 5: return to 2
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

        let render = |track: Vec<Event>| {
            let song = test_song(vec![track], 150);
            let mut seq = Sequencer::new(song);
            let mut buf = vec![0.0; Sequencer::FRAME_SAMPLES * 80];
            seq.mix_into(&mut buf);
            buf
        };

        assert_eq!(render(with_pattern), render(unrolled));
    }

    #[test]
    fn nested_pattern_calls_all_return_in_order() {
        // A PATT inside a PATT body: both PENDs must return to their own
        // call site, not just the outermost one.
        let track = vec![
            Event::Voice(0),   // 0
            Event::Pattern(3), // 1: call outer body at 3, return to 2
            Event::Fine,       // 2
            Event::Pattern(6), // 3: outer body — call inner body at 6, return to 4
            Event::Note {
                // 4: runs after the inner call returns
                key: 64,
                velocity: 127,
                gate: 4,
            },
            Event::PatternEnd, // 5: outer body's own return
            Event::Note {
                // 6: inner body
                key: 60,
                velocity: 127,
                gate: 4,
            },
            Event::PatternEnd, // 7: inner return, back to 4
        ];
        let unrolled = vec![
            Event::Voice(0),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 4,
            },
            Event::Note {
                key: 64,
                velocity: 127,
                gate: 4,
            },
            Event::Fine,
        ];

        let render = |track: Vec<Event>| {
            let song = test_song(vec![track], 150);
            let mut seq = Sequencer::new(song);
            let mut buf = vec![0.0; Sequencer::FRAME_SAMPLES * 40];
            seq.mix_into(&mut buf);
            buf
        };

        assert_eq!(render(track), render(unrolled));
    }

    #[test]
    fn rept_repeats_the_body_count_times_not_once_or_forever() {
        let track = |count: u8| {
            vec![
                Event::Voice(0),
                Event::Note {
                    key: 60,
                    velocity: 127,
                    gate: 1,
                },
                Event::Wait(4),
                Event::Repeat { count, target: 1 },
                Event::Wait(4),
                Event::Fine,
            ]
        };
        let frames_to_finish = |count: u8| {
            let song = test_song(vec![track(count)], 150);
            let mut seq = Sequencer::new(song);
            let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
            let mut frames = 0;
            while !seq.is_finished() && frames < 2000 {
                seq.render_frame(&mut out);
                frames += 1;
            }
            frames
        };

        let short = frames_to_finish(1);
        let long = frames_to_finish(5);
        assert!(
            long > short,
            "REPT count=5 should take longer than count=1 ({long} vs {short})"
        );
        assert!(short < 2000, "REPT must fall through and finish, not hang");
    }

    #[test]
    fn rept_with_zero_count_loops_unconditionally() {
        // count == 0 is `ply_rept`'s uncounted, always-taken jump — the
        // track must never reach FINE.
        let track = vec![
            Event::Voice(0),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 1,
            },
            Event::Wait(4),
            Event::Repeat {
                count: 0,
                target: 1,
            },
        ];
        let song = test_song(vec![track], 150);
        let mut seq = Sequencer::new(song);
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        for _ in 0..200 {
            seq.render_frame(&mut out);
        }
        assert!(!seq.is_finished());
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
    /// correction through the sequencer, not merely not crash (PR #276
    /// review): [`cgb_test_track`]'s key 60 lands on the odd register
    /// `0x60B`, which `cgb_dac_correct` rounds to a different playback
    /// rate, so the two renders must diverge. A threading bug that pins the
    /// flag to either constant makes them identical.
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
