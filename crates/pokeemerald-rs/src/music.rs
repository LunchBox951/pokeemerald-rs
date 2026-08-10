//! Bridges the asset pack's `audio/song`, `audio/voicegroup`, and
//! `audio/sample` entries (S-4, `#115` children 1-5) into the `audio`
//! crate's runtime [`audio::Song`] (S-3, issue #185) -- the missing link
//! between "a song is in the pack" and "the sequencer can play it".
//!
//! # Why this lives here, not in `crates/audio` or `crates/assets`
//!
//! Mirrors [`crate::title`]'s own precedent: that module converts
//! `assets::AssetPack` entries into `rendering` crate types from *this*
//! integration crate, not from `rendering` itself, keeping the pack format
//! decoupled from the engine types it feeds `(oop-boundaries)`. `audio`
//! stays free of any `assets`/pack dependency; this module is the one place
//! that knows about both.
//!
//! # Resolution shape
//!
//! [`load_song_from_pack`] fetches the named `audio/song/*` entry, resolves
//! its [`assets::Song::voicegroup`] reference into a full 128-slot
//! [`audio::Instrument`] table, and converts its per-track
//! [`assets::SongEvent`] streams into [`audio::sequence::Event`]s.
//!
//! Voicegroup resolution is exactly **two levels deep**, matching the
//! engine's own limit: a top-level slot may be a concrete leaf instrument or
//! a key-split/rhythm indirection (resolved one level, via
//! [`convert_indirection_children`]), but a child slot that is *itself* a
//! further indirection has no upstream meaning either --
//! `crate::audio::sequencer`'s `resolve_instrument` aborts such a note
//! rather than recursing (`pokeemerald/src/m4a_1.s:1604`-`:1609`) -- so this
//! module substitutes [`silent_instrument`]/`None` for it instead of
//! resolving a third level. This bounds recursion depth structurally: no
//! cycle guard is needed even though the pack format itself does not forbid
//! a voicegroup graph from cycling (`assets::audio::voicegroup`'s own docs:
//! resolving that cross-reference is deliberately out of that schema's
//! scope).
//!
//! An empty slot ([`assets::VoiceEntry::Empty`]) becomes
//! [`silent_instrument`] wherever the engine's target type has no "no
//! instrument here" representation of its own (a top-level `VOICE` slot, or
//! a key-split child) -- `Instrument::DirectSound` with a one-sample,
//! never-attacking envelope, which is exactly and permanently silent (see
//! that function's docs). [`audio::Rhythm`]'s children *are* `Option`, so an
//! empty rhythm slot maps to a plain `None` instead.
//!
//! # Fields with no engine representation (pre-existing gaps, not new ones)
//!
//! [`audio::song::ToneData`] carries no per-instrument pan (only
//! [`audio::RhythmChild::pan`] exists, for the rhythm-indirection path
//! upstream itself limits pan overrides to -- see
//! `assets::audio::voicegroup`'s module docs), and none of the CGB leaf
//! kinds ([`audio::song::SquareTone`]/[`audio::song::WaveTone`]/
//! [`audio::song::NoiseTone`]) carry a hardware sound-length counter or a
//! fixed-rate flag -- `crate::audio`'s own module docs already list
//! "SFX priority/interruption" adjacent gaps as this slice's scope
//! boundary. This module drops [`assets::DirectSoundVoice::pan`] outside a
//! rhythm child, and every CGB [`length`](assets::Square1Voice::length)/
//! `fixed_rate` field, rather than inventing a new engine capability inside
//! an audio-*playback* slice.
//!
//! [`assets::DirectSoundMode::Reverse`] (backwards sample playback) has no
//! engine representation at all (`crate::audio`'s own "out of scope" list:
//! "compressed/reversed `DirectSound` waves") and is rare project-wide (two
//! occurrences, neither reachable from `MUS_TITLE`'s own voicegroup tree --
//! see `crate::voicegroup_pack_tests`): [`load_song_from_pack`] fails
//! closed with [`MusicError::UnsupportedReversePlayback`] rather than
//! silently playing it forwards.

use std::sync::Arc;

use assets::{
    AssetPack, DirectSoundMode, DirectSoundVoice, Envelope, KeySplitVoice, NoiseVoice, PackError,
    ProgrammableWaveVoice, RhythmVoice, Sample, Square1Voice, Square2Voice, VoiceEntry, VoiceGroup,
    VoiceGroupId,
};
use audio::sequence::Event;
use audio::song::{NoiseTone, SquareTone, WaveTone};
use audio::{
    Adsr, CgbAdsr, Instrument, KeySplit, Rhythm, RhythmChild, ToneData, WaveData, KEY_SLOTS,
};

mod player;
#[cfg(test)]
mod tests;

pub use player::{MusicPlayer, RING_CAPACITY_FRAMES};

/// `xIECV`'s `XCMD` sub-command number (pseudo-echo volume,
/// `pokeemerald/sound/m4a_tables.c:252`) -- matches the identically-named,
/// crate-private constant `audio::sequencer` decodes against; duplicated
/// here (a two-constant table, not transliterated logic `(no-verbatim)`)
/// because that constant is not part of `audio`'s public surface.
const XCMD_IECV: u8 = 0x08;
/// `xIECL`'s `XCMD` sub-command number (pseudo-echo length,
/// `pokeemerald/sound/m4a_tables.c:253`). See [`XCMD_IECV`].
const XCMD_IECL: u8 = 0x09;

/// A tick fires once [`audio::Sequencer`]'s tempo accumulator reaches `150`
/// (`crate::audio::sequencer`'s `TEMPO_UNIT`) -- seeding [`audio::Song::new`]'s
/// `initial_tempo` at that same value guarantees a tick (and so track 0's
/// leading `Voice`/`Volume`/real `Tempo` commands, conventionally its first
/// events -- `assets::audio::song`'s module docs) fires on frame 0, before
/// any audio renders, regardless of what BPM the song's own `Tempo` event
/// then sets. Mirrors `crate::audio::sequencer`'s own test convention of
/// pinning `tempo: 150` for the same immediate-tick reason.
const INITIAL_TEMPO_SEED: u16 = 150;

/// Why [`load_song_from_pack`] (or the [`MusicPlayer`] built from it) failed.
///
/// Concrete per-crate enum `(oop-boundaries)` -- no `anyhow`.
#[derive(Debug)]
pub enum MusicError {
    /// Loading a pack entry (`audio/song/*`, `audio/voicegroup/*`, or
    /// `audio/sample/*`) failed -- missing, stale, or malformed pack.
    Pack(PackError),
    /// A sample id resolved to the wrong [`Sample`] kind (e.g. a
    /// `ProgrammableWaveVoice` referencing a `DirectSound` sample) -- never
    /// true for a pack `cargo xtask extract` produced; a real, typed
    /// failure mode rather than a panic should the pack ever be corrupt.
    WrongSampleKind {
        /// The sample id that resolved to the wrong kind.
        id: String,
        /// What kind of sample the referencing voice slot expected.
        expected: &'static str,
    },
    /// A `DirectSound` voice slot plays its sample backwards
    /// (`DirectSoundMode::Reverse`) -- unrepresentable in the current
    /// engine (module docs).
    UnsupportedReversePlayback {
        /// The sample id the unsupported slot references.
        id: String,
    },
    /// Opening (or preparing) the audio output device failed.
    Platform(platform::PlatformError),
}

impl std::fmt::Display for MusicError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pack(err) => write!(f, "music: {err}"),
            Self::WrongSampleKind { id, expected } => {
                write!(f, "music: sample `{id}` is not a {expected}")
            }
            Self::UnsupportedReversePlayback { id } => write!(
                f,
                "music: sample `{id}` plays backwards (DirectSoundMode::Reverse), which this \
                 engine does not yet support"
            ),
            Self::Platform(err) => write!(f, "music: {err}"),
        }
    }
}

impl std::error::Error for MusicError {}

impl From<PackError> for MusicError {
    fn from(err: PackError) -> Self {
        Self::Pack(err)
    }
}

impl From<platform::PlatformError> for MusicError {
    fn from(err: platform::PlatformError) -> Self {
        Self::Platform(err)
    }
}

/// Load and resolve `audio/song/<name>` (e.g. `"mus_title"`) from `pack`
/// into a ready-to-play [`audio::Song`] -- see the module docs for the
/// resolution shape and its documented gaps.
///
/// # Errors
///
/// [`MusicError::Pack`] if the song, its voicegroup, or any sample it
/// references is missing or fails to decode; [`MusicError::WrongSampleKind`]
/// or [`MusicError::UnsupportedReversePlayback`] for the documented
/// unrepresentable cases.
pub fn load_song_from_pack(pack: &AssetPack, name: &str) -> Result<audio::Song, MusicError> {
    let packed = pack.song(name)?;
    let voices = convert_top_level_voicegroup(pack, packed.voicegroup())?;
    let tracks: Vec<Vec<Event>> = packed
        .tracks()
        .iter()
        .map(|track| track.iter().map(convert_event).collect())
        .collect();
    Ok(audio::Song::new(voices, tracks, INITIAL_TEMPO_SEED)
        .with_reverb(packed.reverb().unwrap_or(0)))
}

/// Fetch and fully resolve a top-level voicegroup: every one of its (always
/// [`assets::VOICE_SLOT_COUNT`]-long, per the pack's own extraction-time
/// normalization -- `crate::voicegroup_pack_tests`) slots, indirection
/// allowed one level deep (module docs).
fn convert_top_level_voicegroup(
    pack: &AssetPack,
    id: &VoiceGroupId,
) -> Result<Vec<Instrument>, MusicError> {
    let group = pack.voicegroup(id)?;
    group
        .slots()
        .iter()
        .map(|entry| convert_voice_entry(pack, entry, true))
        .collect()
}

/// Convert one voicegroup slot. `allow_indirection` is `true` only for a
/// top-level slot (module docs): a key-split/rhythm entry reached with it
/// `false` (a child slot that is itself an indirection) becomes
/// [`silent_instrument`], matching the engine's own "no nested indirection"
/// limit.
fn convert_voice_entry(
    pack: &AssetPack,
    entry: &VoiceEntry,
    allow_indirection: bool,
) -> Result<Instrument, MusicError> {
    match entry {
        VoiceEntry::DirectSound(v) => convert_direct_sound(pack, v),
        VoiceEntry::Square1(v) => Ok(convert_square1(v)),
        VoiceEntry::Square2(v) => Ok(convert_square2(v)),
        VoiceEntry::ProgrammableWave(v) => convert_programmable_wave(pack, v),
        VoiceEntry::Noise(v) => Ok(convert_noise(v)),
        VoiceEntry::KeySplit(v) if allow_indirection => convert_key_split(pack, v),
        VoiceEntry::Rhythm(v) if allow_indirection => convert_rhythm(pack, v),
        // Empty, and a too-deeply-nested indirection (module docs): both
        // become permanent silence.
        VoiceEntry::Empty | VoiceEntry::KeySplit(_) | VoiceEntry::Rhythm(_) => {
            Ok(silent_instrument())
        }
    }
}

/// A permanently, safely silent [`Instrument`]: a one-sample `DirectSound`
/// wave under an envelope that never leaves its `0`-volume attack phase
/// (`attack: 0` never reaches the `255` threshold `Adsr`'s attack phase
/// requires to advance -- `crate::audio::envelope`'s own module docs).
/// Stands in for [`VoiceEntry::Empty`] (and, per [`convert_voice_entry`], a
/// too-deeply-nested indirection) wherever the target engine type has no
/// dedicated "nothing here" representation of its own.
fn silent_instrument() -> Instrument {
    let wave = Arc::new(WaveData::one_shot(1 << 20, vec![0]));
    Instrument::DirectSound(ToneData::new(
        wave,
        Adsr {
            attack: 0,
            decay: 0,
            sustain: 0,
            release: 0,
        },
    ))
}

fn convert_envelope(e: Envelope) -> Adsr {
    Adsr {
        attack: e.attack,
        decay: e.decay,
        sustain: e.sustain,
        release: e.release,
    }
}

fn convert_cgb_envelope(e: Envelope) -> CgbAdsr {
    CgbAdsr {
        attack: e.attack,
        decay: e.decay,
        sustain: e.sustain,
        release: e.release,
    }
}

/// # Errors
///
/// [`MusicError::Pack`] if `v.sample` is missing; [`MusicError::WrongSampleKind`]
/// if it resolves to a [`Sample::ProgrammableWave`] instead;
/// [`MusicError::UnsupportedReversePlayback`] for `DirectSoundMode::Reverse`
/// (module docs -- [`ToneData::pan`](assets::DirectSoundVoice::pan) is
/// likewise dropped there, a pre-existing engine gap, not a new one).
fn convert_direct_sound(pack: &AssetPack, v: &DirectSoundVoice) -> Result<Instrument, MusicError> {
    if v.mode == DirectSoundMode::Reverse {
        return Err(MusicError::UnsupportedReversePlayback {
            id: v.sample.0.clone(),
        });
    }
    let sample = pack.sample(&v.sample)?;
    let Sample::DirectSound(ds) = sample else {
        return Err(MusicError::WrongSampleKind {
            id: v.sample.0.clone(),
            expected: "DirectSound",
        });
    };
    let wave = Arc::new(match ds.loop_start {
        Some(start) => WaveData::looping(ds.base_frequency, start, ds.data().to_vec()),
        None => WaveData::one_shot(ds.base_frequency, ds.data().to_vec()),
    });
    let tone = ToneData::new(wave, convert_envelope(v.envelope));
    let tone = if v.mode == DirectSoundMode::Fixed {
        tone.fixed()
    } else {
        tone
    };
    Ok(Instrument::DirectSound(tone))
}

fn convert_square1(v: &Square1Voice) -> Instrument {
    Instrument::CgbSquare1(SquareTone {
        duty: v.duty,
        sweep: v.sweep,
        adsr: convert_cgb_envelope(v.envelope),
    })
}

fn convert_square2(v: &Square2Voice) -> Instrument {
    // Square 2 has no hardware sweep register (`SquareTone::sweep`'s own
    // docs): the field is never read for this channel, so `0` is inert.
    Instrument::CgbSquare2(SquareTone {
        duty: v.duty,
        sweep: 0,
        adsr: convert_cgb_envelope(v.envelope),
    })
}

fn convert_noise(v: &NoiseVoice) -> Instrument {
    Instrument::CgbNoise(NoiseTone {
        period: v.period,
        adsr: convert_cgb_envelope(v.envelope),
    })
}

/// # Errors
///
/// [`MusicError::Pack`] if `v.wave` is missing; [`MusicError::WrongSampleKind`]
/// if it resolves to a [`Sample::DirectSound`] instead.
fn convert_programmable_wave(
    pack: &AssetPack,
    v: &ProgrammableWaveVoice,
) -> Result<Instrument, MusicError> {
    let sample = pack.sample(&v.wave)?;
    let Sample::ProgrammableWave(w) = sample else {
        return Err(MusicError::WrongSampleKind {
            id: v.wave.0.clone(),
            expected: "ProgrammableWave",
        });
    };
    Ok(Instrument::CgbWave(WaveTone {
        table: w.table,
        adsr: convert_cgb_envelope(v.envelope),
    }))
}

/// Resolve a key-split indirection's child group into a full
/// [`KeySplit::table`]/[`KeySplit::children`] pair.
///
/// [`KeySplitVoice::table`] is biased by `starting_note` and covers only a
/// subrange; [`KeySplit::table`] is a flat, unbiased `0..=127` lookup (that
/// type's own docs). Keys outside the source subrange -- which upstream
/// leaves genuinely undefined (reading whatever bytes happen to follow the
/// table in ROM, `assets::audio::voicegroup`'s own `KeySplitVoice` docs) --
/// map to an out-of-range sentinel index instead of guessing at a real
/// child, so [`crate::audio::sequencer`]'s `resolve_instrument` cleanly
/// finds no match and plays no note: silence is the honestly-representable
/// choice here, not a guess at upstream's undefined behaviour.
///
/// # Errors
///
/// [`MusicError::Pack`] if `v.children` is missing; propagates any leaf
/// conversion error from the child group's own slots.
fn convert_key_split(pack: &AssetPack, v: &KeySplitVoice) -> Result<Instrument, MusicError> {
    let child_group = pack.voicegroup(&v.children)?;
    let children = convert_indirection_children(pack, &child_group)?;

    // Deliberately out of `0..children.len()`, so `children.get(sentinel)`
    // always misses (module docs).
    let sentinel = u8::try_from(children.len()).unwrap_or(u8::MAX);
    let mut table = [sentinel; KEY_SLOTS];
    for (offset, &child_index) in v.table().iter().enumerate() {
        if let Some(key) = usize::from(v.starting_note).checked_add(offset) {
            if let Some(slot) = table.get_mut(key) {
                *slot = child_index;
            }
        }
    }

    Ok(Instrument::KeySplit(KeySplit { table, children }))
}

/// Resolve a rhythm indirection's child group into [`Rhythm::children`]: one
/// slot per raw played key, each either a concrete leaf instrument (with its
/// own base key/pan substituted for the played note, per
/// [`RhythmChild`]'s own docs) or `None`.
///
/// # Errors
///
/// [`MusicError::Pack`] if `v.children` is missing; propagates any leaf
/// conversion error from the child group's own slots.
fn convert_rhythm(pack: &AssetPack, v: &RhythmVoice) -> Result<Instrument, MusicError> {
    let child_group = pack.voicegroup(&v.children)?;
    let mut children = Vec::with_capacity(child_group.slots().len());
    for entry in child_group.slots() {
        children.push(match entry {
            VoiceEntry::Empty | VoiceEntry::KeySplit(_) | VoiceEntry::Rhythm(_) => None,
            leaf => Some(RhythmChild {
                base_key: base_key_of(leaf),
                pan: rhythm_pan_of(leaf),
                instrument: convert_voice_entry(pack, leaf, false)?,
            }),
        });
    }
    Ok(Instrument::Rhythm(Rhythm { children }))
}

/// Convert a whole child voicegroup's slots into leaf-only [`Instrument`]s
/// (no further indirection -- [`convert_voice_entry`]'s `allow_indirection:
/// false`), the shared body [`convert_key_split`] uses for
/// [`KeySplit::children`].
fn convert_indirection_children(
    pack: &AssetPack,
    group: &VoiceGroup,
) -> Result<Vec<Instrument>, MusicError> {
    group
        .slots()
        .iter()
        .map(|entry| convert_voice_entry(pack, entry, false))
        .collect()
}

/// Every concrete [`VoiceEntry`] leaf kind carries its own base key
/// (`ToneData::key` upstream); `0` for [`VoiceEntry::Empty`]/indirection,
/// which never reach here as `leaf` in [`convert_rhythm`]'s match.
fn base_key_of(entry: &VoiceEntry) -> u8 {
    match entry {
        VoiceEntry::DirectSound(v) => v.base_key,
        VoiceEntry::Square1(v) => v.base_key,
        VoiceEntry::Square2(v) => v.base_key,
        VoiceEntry::ProgrammableWave(v) => v.base_key,
        VoiceEntry::Noise(v) => v.base_key,
        VoiceEntry::Empty | VoiceEntry::KeySplit(_) | VoiceEntry::Rhythm(_) => 0,
    }
}

/// A rhythm child's pan override: only [`VoiceEntry::DirectSound`] carries a
/// pan at all (`assets::audio::voicegroup`'s module docs -- the CGB kinds'
/// same wire byte is a length, not a pan). Reconstructs the raw
/// `pan_sweep` byte (`0x80 | pan`, the override-set sentinel bit) and
/// reuses [`audio::rhythm_pan_from_pan_sweep`] rather than re-deriving its
/// `(pan_sweep - 0xC0) * 2` formula a second time.
fn rhythm_pan_of(entry: &VoiceEntry) -> Option<i8> {
    let VoiceEntry::DirectSound(v) = entry else {
        return None;
    };
    v.pan
        .and_then(|p| audio::rhythm_pan_from_pan_sweep(0x80 | p))
}

/// Convert one normalized [`assets::SongEvent`] into the engine's own
/// [`Event`] -- a 1:1 mapping (module docs' `MemAcc`/`Xcmd` note aside, every
/// arm here is the same command under a different type), so a
/// [`SongEvent::Goto`]/`MemAccBranch` target (an event index into the same
/// track) stays valid after conversion without needing to be re-resolved.
fn convert_event(event: &assets::SongEvent) -> Event {
    use assets::SongEvent as Se;
    match *event {
        Se::Wait(ticks) => Event::Wait(ticks),
        Se::Note {
            key,
            velocity,
            gate,
        } => Event::Note {
            key,
            velocity,
            gate,
        },
        Se::EndOfTie { key } => Event::EndOfTie { key },
        Se::Voice(v) => Event::Voice(v),
        Se::Volume(v) => Event::Volume(v),
        Se::Pan(p) => Event::Pan(p),
        Se::Bend(b) => Event::Bend(b),
        Se::BendRange(r) => Event::BendRange(r),
        Se::Tune(t) => Event::Tune(t),
        Se::KeyShift(k) => Event::KeyShift(k),
        Se::Tempo(bpm) => Event::Tempo(bpm),
        Se::Priority(p) => Event::Priority(p),
        Se::LfoSpeed(s) => Event::LfoSpeed(s),
        Se::LfoDelay(d) => Event::LfoDelay(d),
        Se::Modulation(m) => Event::Modulation(m),
        Se::ModType(t) => Event::ModType(t),
        Se::PseudoEchoVolume(v) => Event::Xcmd {
            kind: XCMD_IECV,
            value: u32::from(v),
        },
        Se::PseudoEchoLength(v) => Event::Xcmd {
            kind: XCMD_IECL,
            value: u32::from(v),
        },
        Se::Goto(target) => Event::Goto(goto_index(target)),
        Se::MemAcc { op, address, data } => Event::MemAcc {
            op: op as u8,
            addr: address,
            value: data,
            target: None,
        },
        Se::MemAccBranch {
            condition,
            address,
            data,
            target,
        } => Event::MemAcc {
            op: condition as u8,
            addr: address,
            value: data,
            target: Some(goto_index(target)),
        },
        Se::Fine => Event::Fine,
    }
}

/// `u32` event index -> `usize`, saturating rather than panicking on a
/// (never-real-upstream) target past `usize::MAX`; harmless even then, since
/// an out-of-range track cursor already ends the track cleanly rather than
/// panicking (`crate::audio::sequencer`'s `process_track`).
fn goto_index(target: u32) -> usize {
    usize::try_from(target).unwrap_or(usize::MAX)
}
