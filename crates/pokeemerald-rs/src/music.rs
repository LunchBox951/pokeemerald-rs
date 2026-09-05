//! Resolves packed songs, voicegroups, and samples into playable [`audio::Song`] values.
//!
//! Key-split and rhythm instruments resolve one child voicegroup. Nested
//! indirection plays silence because the sequencer supports only that depth
//! (`pokeemerald/src/m4a_1.s:1604`-`:1609`). Conversion also omits source
//! fields the runtime cannot represent: direct-sound pan outside rhythm
//! children, CGB sound lengths, and noise fixed-rate mode. Reverse direct-sound
//! playback fails instead of silently playing the sample forwards.

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

pub use player::{
    MusicContext, MusicPlayer, DEVICE_TAIL_FLOOR_FRAMES, RING_CAPACITY_FRAMES, TITLE_FADE_OUT_SPEED,
};

// `m4a_tables.c:252` and `:253` assign these XCMD subcommand IDs.
const XCMD_PSEUDO_ECHO_VOLUME: u8 = 0x08;
const XCMD_PSEUDO_ECHO_LENGTH: u8 = 0x09;

// Starting at the sequencer's private tempo threshold makes the first tick
// process the song's setup events before rendering audio.
const IMMEDIATE_TICK_TEMPO: u16 = 150;
const UNUSED_SQUARE_2_SWEEP: u8 = 0;
const NON_LEAF_BASE_KEY: u8 = 0;
const RHYTHM_PAN_OVERRIDE_FLAG: u8 = 0x80;

/// Why loading or playing music failed.
#[derive(Debug)]
pub enum MusicError {
    /// A required pack entry was missing or malformed.
    Pack(PackError),
    /// A voice referenced an incompatible sample kind.
    WrongSampleKind {
        /// Referenced sample ID.
        id: String,
        /// Required sample kind.
        expected: &'static str,
    },
    /// A direct-sound voice requested unsupported reverse playback.
    UnsupportedReversePlayback {
        /// Referenced sample ID.
        id: String,
    },
    /// The audio output could not be opened or prepared.
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

/// Loads and resolves `audio/song/<name>` from an asset pack.
///
/// # Errors
///
/// Returns [`MusicError`] when a required entry is missing, malformed, or
/// unsupported by the runtime representation.
pub fn load_song_from_pack(pack: &AssetPack, name: &str) -> Result<audio::Song, MusicError> {
    let packed = pack.song(name)?;
    let voices = convert_top_level_voicegroup(pack, packed.voicegroup())?;
    let tracks: Vec<Vec<Event>> = packed
        .tracks()
        .iter()
        .map(|track| track.iter().map(convert_event).collect())
        .collect();
    let song =
        audio::Song::new(voices, tracks, IMMEDIATE_TICK_TEMPO).with_priority(packed.priority());
    Ok(match packed.reverb() {
        Some(level) => song.with_reverb(level),
        None => song,
    })
}

fn convert_top_level_voicegroup(
    pack: &AssetPack,
    id: &VoiceGroupId,
) -> Result<Vec<Instrument>, MusicError> {
    let group = pack.voicegroup(id)?;
    group
        .slots()
        .iter()
        .map(|entry| convert_top_level_voice_entry(pack, entry))
        .collect()
}

fn convert_top_level_voice_entry(
    pack: &AssetPack,
    entry: &VoiceEntry,
) -> Result<Instrument, MusicError> {
    match entry {
        VoiceEntry::KeySplit(v) => convert_key_split(pack, v),
        VoiceEntry::Rhythm(v) => convert_rhythm(pack, v),
        leaf => convert_leaf_voice_entry(pack, leaf),
    }
}

fn convert_leaf_voice_entry(
    pack: &AssetPack,
    entry: &VoiceEntry,
) -> Result<Instrument, MusicError> {
    match entry {
        VoiceEntry::DirectSound(v) => convert_direct_sound(pack, v),
        VoiceEntry::Square1(v) => Ok(convert_square1(v)),
        VoiceEntry::Square2(v) => Ok(convert_square2(v)),
        VoiceEntry::ProgrammableWave(v) => convert_programmable_wave(pack, v),
        VoiceEntry::Noise(v) => Ok(convert_noise(v)),
        VoiceEntry::Empty | VoiceEntry::KeySplit(_) | VoiceEntry::Rhythm(_) => {
            Ok(silent_instrument())
        }
    }
}

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
    let wave = Arc::new(match ds.loop_start() {
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
        fixed_rate: v.fixed_rate,
    })
}

fn convert_square2(v: &Square2Voice) -> Instrument {
    Instrument::CgbSquare2(SquareTone {
        duty: v.duty,
        sweep: UNUSED_SQUARE_2_SWEEP,
        adsr: convert_cgb_envelope(v.envelope),
        fixed_rate: v.fixed_rate,
    })
}

fn convert_noise(v: &NoiseVoice) -> Instrument {
    Instrument::CgbNoise(NoiseTone {
        lfsr_width_selector: v.period,
        adsr: convert_cgb_envelope(v.envelope),
    })
}

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
        fixed_rate: v.fixed_rate,
    }))
}

const MISSING_CHILD_INDEX: u8 = u8::MAX;

fn convert_key_split(pack: &AssetPack, v: &KeySplitVoice) -> Result<Instrument, MusicError> {
    let child_group = pack.voicegroup(&v.children)?;
    let mut children = convert_indirection_children(pack, &child_group)?;
    children.truncate(usize::from(MISSING_CHILD_INDEX));

    let mut table = [MISSING_CHILD_INDEX; KEY_SLOTS];
    for (offset, &child_index) in v.table().iter().enumerate() {
        if let Some(key) = usize::from(v.starting_note).checked_add(offset) {
            if let Some(slot) = table.get_mut(key) {
                *slot = child_index;
            }
        }
    }

    Ok(Instrument::KeySplit(KeySplit { table, children }))
}

fn convert_rhythm(pack: &AssetPack, v: &RhythmVoice) -> Result<Instrument, MusicError> {
    let child_group = pack.voicegroup(&v.children)?;
    let mut children = Vec::with_capacity(child_group.slots().len());
    for entry in child_group.slots() {
        children.push(match entry {
            VoiceEntry::Empty | VoiceEntry::KeySplit(_) | VoiceEntry::Rhythm(_) => None,
            leaf => Some(RhythmChild {
                base_key: base_key_of(leaf),
                pan: rhythm_pan_of(leaf),
                instrument: convert_leaf_voice_entry(pack, leaf)?,
            }),
        });
    }
    Ok(Instrument::Rhythm(Rhythm { children }))
}

fn convert_indirection_children(
    pack: &AssetPack,
    group: &VoiceGroup,
) -> Result<Vec<Instrument>, MusicError> {
    group
        .slots()
        .iter()
        .map(|entry| convert_leaf_voice_entry(pack, entry))
        .collect()
}

fn base_key_of(entry: &VoiceEntry) -> u8 {
    match entry {
        VoiceEntry::DirectSound(v) => v.base_key,
        VoiceEntry::Square1(v) => v.base_key,
        VoiceEntry::Square2(v) => v.base_key,
        VoiceEntry::ProgrammableWave(v) => v.base_key,
        VoiceEntry::Noise(v) => v.base_key,
        VoiceEntry::Empty | VoiceEntry::KeySplit(_) | VoiceEntry::Rhythm(_) => NON_LEAF_BASE_KEY,
    }
}

fn rhythm_pan_of(entry: &VoiceEntry) -> Option<i8> {
    let VoiceEntry::DirectSound(v) = entry else {
        return None;
    };
    v.pan
        .and_then(|pan| audio::rhythm_pan_from_pan_sweep(RHYTHM_PAN_OVERRIDE_FLAG | pan))
}

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
            kind: XCMD_PSEUDO_ECHO_VOLUME,
            value: u32::from(v),
        },
        Se::PseudoEchoLength(v) => Event::Xcmd {
            kind: XCMD_PSEUDO_ECHO_LENGTH,
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

fn goto_index(target: u32) -> usize {
    usize::try_from(target).unwrap_or(usize::MAX)
}
