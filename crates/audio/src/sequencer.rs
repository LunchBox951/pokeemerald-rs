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
//! DirectSound or a CGB PSG instrument.
//!
//! Out of scope for this slice (decoded but not executed): memory
//! accumulator (`MEMACC`), extended commands (`XCMD`), priority-based voice
//! stealing.

use crate::cgb_voice::{CgbChannelNumber, CgbVoice};
use crate::pitch::{self, SAMPLES_PER_FRAME};
use crate::psg::WaveChannel;
use crate::sequence::Event;
use crate::song::{Instrument, Song};
use crate::voice::{channel_volume, Voice};
use crate::{Mixer, DEFAULT_MASTER_VOLUME, DEFAULT_MAX_VOICES};

/// Tempo units per tick; `MPlayMain` fires a tick each time `tempoC` crosses
/// `150` (`subs r0, 150`).
const TEMPO_UNIT: u16 = 150;

/// Default track volume when a track plays a note before any `VOL` command.
/// (Upstream leaves `track->vol` at `0`; this crate defaults to full so a
/// minimal hand-authored sequence is audible — a deliberate convenience.)
const DEFAULT_TRACK_VOLUME: u8 = 127;

/// Default pitch-bend range (`track->bendRange = 2`, `m4a_1.s:1223`).
const DEFAULT_BEND_RANGE: u8 = 2;

/// The instrument volume scaler `volX` (`0x40`, set at track init).
const VOL_X: u32 = 0x40;

/// Safety bound on commands processed for one track in one tick, so a
/// malformed loop with no `Wait` cannot hang the mixer.
const MAX_COMMANDS_PER_TICK: u32 = 4096;

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
    /// Saved return cursors for nested `PATT` calls (`track->patternStack`,
    /// max depth 3 — `ply_patt`, `m4a_1.s:851`).
    pattern_stack: [usize; 3],
    /// Current `PATT` nesting depth, `0..=3` (`track->patternLevel`).
    pattern_level: u8,
    /// `REPT`'s in-progress repeat counter (`track->repN`).
    rep_n: u8,
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
            // Upstream track init seeds `lfoSpeed = 0x16` (`m4a_1.s:1226`,
            // `movs r0, 0x16`), a sibling of the `bendRange = 2` / `volX = 0x40`
            // defaults already mirrored above. Modulation still gates on
            // `mod_depth > 0` (`m4a_1.s:1288`), so this default alone never
            // makes a silent track wobble.
            lfo_speed: 22,
            lfo_delay: 0,
            lfo_delay_c: 0,
            lfo_speed_c: 0,
            mod_m: 0,
            pattern_stack: [0; 3],
            pattern_level: 0,
            rep_n: 0,
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
    #[must_use]
    pub fn with_config(song: Song, master_volume: u8, max_voices: usize) -> Self {
        let tracks = (0..song.track_count()).map(|_| TrackState::new()).collect();
        let tempo_i = song.initial_tempo();
        Self {
            song,
            tracks,
            mixer: Mixer::new(master_volume, max_voices),
            tempo_i,
            tempo_c: 0,
        }
    }

    /// Number of voices currently sounding.
    #[must_use]
    pub fn voice_count(&self) -> usize {
        self.mixer.voice_count()
    }

    /// Whether every track has ended and all voices have decayed to silence.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.tracks.iter().all(|t| t.ended) && self.mixer.is_idle()
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
    /// [`Self::FRAME_SAMPLES`]. Deterministic and device-free — the offline
    /// rendering path unit tests assert against.
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
    fn advance_frame(&mut self) {
        self.tempo_c += self.tempo_i;
        while self.tempo_c >= TEMPO_UNIT {
            self.tempo_c -= TEMPO_UNIT;
            self.do_tick();
        }
    }

    /// One sequencer tick: gate countdown, then command processing per track.
    fn do_tick(&mut self) {
        self.mixer.tick_gates();
        // Disjoint field borrows so each track can touch the shared mixer.
        let Self {
            song,
            tracks,
            mixer,
            tempo_i,
            ..
        } = self;
        for (track_id, track) in tracks.iter_mut().enumerate() {
            Self::process_track(song, track, mixer, track_id, tempo_i);
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
                    track.ended = true;
                    break;
                }
                let event = events[track.cursor].clone();
                track.cursor += 1;
                Self::handle_event(song, track, mixer, track_id, tempo_i, &event);
                if track.wait > 0 || track.ended {
                    break;
                }
            }
        }

        if track.wait > 0 {
            track.wait -= 1;
            // MPlayMain runs the LFO update on the same tick it consumes a
            // wait (`m4a_1.s:1279`..`:1330`), for every track still holding
            // a note — not only on the tick a `Wait` command was issued.
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
            Event::Tempo(bpm) => *tempo_i = bpm,
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
            Event::Note {
                key,
                velocity,
                gate,
            } => {
                // `ply_note` records the raw command key as `track->key`.
                track.key = key;
                // Reload the LFO delay on every note-on; a nonzero delay
                // also resets the LFO phase and clears any live modulation,
                // mirroring the inline `clear_modM` call in `ply_note`
                // (`m4a_1.s:1732`..`:1738`).
                track.lfo_delay_c = track.lfo_delay;
                if track.lfo_delay != 0 {
                    Self::clear_mod_m(track, mixer, track_id);
                }
                Self::note_on(song, track, mixer, track_id, key, velocity, gate);
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
            // Decoded but not executed by this slice (see the module docs).
            _ => {}
        }
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
    /// the track's current state (`TrkVolPitSet` + `ChnVolSetAsm`), and
    /// dispatching to either a DirectSound or a CGB PSG voice depending on
    /// the selected instrument's kind.
    fn note_on(
        song: &Song,
        track: &TrackState,
        mixer: &mut Mixer,
        track_id: usize,
        key: u8,
        velocity: u8,
        gate: u8,
    ) {
        let Some(instrument) = song.voice(track.voice) else {
            return;
        };

        let (vol_mr, vol_ml) = track_volume(track);
        let (key_m, pit_m) = track_pitch(track);

        // key + keyM, floored at 0 (`bpl _081DDCA0; movs r3, 0`), then the sum
        // is passed to `MidiKeyToFreq`'s `u8 key` param, truncating the register
        // to its low byte — so a sum above 255 wraps modulo 256, it does not
        // saturate (`ldrb r1, [key]; adds r3, r1, r0; ... bl MidiKeyToFreq`).
        // The same clamp-then-truncate feeds `MidiKeyToCgbFreq` for CGB
        // instruments (`m4a_1.s:1760`..`:1766`).
        let note_key = u8::try_from((i32::from(key) + key_m).max(0) & 0xFF).unwrap_or(0);
        let gate = u16::from(gate);

        match instrument {
            Instrument::DirectSound(tone) => {
                let right = channel_volume(vol_mr, 0x80, velocity);
                let left = channel_volume(vol_ml, 0x7F, velocity);
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
                    0,
                    0,
                );
                mixer.add_voice(voice);
            }
            Instrument::CgbSquare1(sq) => {
                mixer.add_cgb_voice(CgbVoice::square(
                    CgbChannelNumber::Square1,
                    sq.duty,
                    Some(sq.sweep),
                    sq.adsr,
                    note_key,
                    pit_m,
                    vol_mr,
                    vol_ml,
                    velocity,
                    gate,
                    key,
                    track_id,
                ));
            }
            Instrument::CgbSquare2(sq) => {
                mixer.add_cgb_voice(CgbVoice::square(
                    CgbChannelNumber::Square2,
                    sq.duty,
                    None,
                    sq.adsr,
                    note_key,
                    pit_m,
                    vol_mr,
                    vol_ml,
                    velocity,
                    gate,
                    key,
                    track_id,
                ));
            }
            Instrument::CgbWave(w) => {
                let samples = WaveChannel::decode_wave_ram(&w.table);
                mixer.add_cgb_voice(CgbVoice::wave(
                    samples, w.adsr, note_key, pit_m, vol_mr, vol_ml, velocity, gate, key, track_id,
                ));
            }
            Instrument::CgbNoise(n) => {
                mixer.add_cgb_voice(CgbVoice::noise(
                    n.adsr, note_key, n.period, vol_mr, vol_ml, velocity, gate, key, track_id,
                ));
            }
        }
    }
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
    use crate::song::{NoiseTone, SquareTone, ToneData, WaveTone};

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
        // One tick per frame at tempo 150; gate 2 releases after ~2 ticks and
        // the flat envelope (release 0) then retires the voice quickly.
        for _ in 0..8 {
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
        assert!(out.iter().any(|&s| s.abs() > 0.0));
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
        // exhausts. Before the fix, FINE only marked the track ended, leaving
        // the voice sounding forever so `is_finished()` never returned true.
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
        })];
        let song = Song::new(voices, vec![cgb_test_track()], 150);
        let mut seq = Sequencer::new(song);
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        seq.render_frame(&mut out);
        assert!(out.iter().any(|&s| s.abs() > 0.0));
        assert_eq!(seq.voice_count(), 1);
    }

    #[test]
    fn a_cgb_wave_note_produces_sound_through_the_sequencer() {
        let voices = vec![Instrument::CgbWave(WaveTone {
            table: [0xFF; 16],
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
    fn a_cgb_noise_note_produces_sound_through_the_sequencer() {
        let voices = vec![Instrument::CgbNoise(NoiseTone {
            period: 0,
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
            }),
            Instrument::CgbSquare2(SquareTone {
                duty: 1,
                sweep: 0,
                adsr: CgbAdsr::flat(),
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
            period: 0,
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
}
