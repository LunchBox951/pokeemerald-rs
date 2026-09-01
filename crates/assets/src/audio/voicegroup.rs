//! Typed instrument slots stored in voicegroup asset-pack entries.
//!
//! Key-split and rhythm slots retain their child [`VoiceGroupId`] references;
//! decoding validates structure without resolving those references.

use super::cursor::{check_id_len, Reader, Writer};
use super::error::AudioError;
use super::sample::SampleId;

/// The number of instrument slots addressable by M4A `VOICE` commands.
pub const VOICE_SLOT_COUNT: usize = 128;

/// A voicegroup's stable asset-pack identifier.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VoiceGroupId(pub String);

/// An instrument's attack, decay, sustain, and release envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Envelope {
    pub attack: u8,
    pub decay: u8,
    pub sustain: u8,
    pub release: u8,
}

impl Envelope {
    fn write(self, w: &mut Writer) {
        w.u8(self.attack);
        w.u8(self.decay);
        w.u8(self.sustain);
        w.u8(self.release);
    }

    fn read(r: &mut Reader<'_>) -> Result<Self, AudioError> {
        Ok(Self {
            attack: r.u8()?,
            decay: r.u8()?,
            sustain: r.u8()?,
            release: r.u8()?,
        })
    }
}

/// The playback mode for a [`DirectSoundVoice`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectSoundMode {
    /// Resample the recording to the played note's pitch.
    Resampled,
    /// Play the recording at its native rate, independent of the played note.
    Fixed,
    /// Play the recording backwards.
    Reverse,
}

/// A `DirectSound` (PCM sample playback) voice slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectSoundVoice {
    pub base_key: u8,
    /// Overrides the track pan. `None` inherits it; zero is not encodable as an override.
    pub pan: Option<u8>,
    /// The [`super::sample::Sample::DirectSound`] this voice plays.
    pub sample: SampleId,
    pub envelope: Envelope,
    pub mode: DirectSoundMode,
}

/// A CGB square-channel-1 voice slot with hardware frequency sweep.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Square1Voice {
    pub base_key: u8,
    /// Hardware sound-length counter.
    pub length: u8,
    /// Raw `NR10`-style sweep byte.
    pub sweep: u8,
    /// Duty cycle selector, `0..=3`.
    pub duty: u8,
    pub envelope: Envelope,
    /// Whether playback ignores the played note's pitch.
    pub fixed_rate: bool,
}

/// A CGB square-channel-2 voice slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Square2Voice {
    pub base_key: u8,
    /// Hardware sound-length counter.
    pub length: u8,
    pub duty: u8,
    pub envelope: Envelope,
    pub fixed_rate: bool,
}

/// A CGB programmable-wave (hardware channel 3) voice slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgrammableWaveVoice {
    pub base_key: u8,
    /// Hardware sound-length counter.
    pub length: u8,
    /// The [`super::sample::Sample::ProgrammableWave`] this voice plays.
    pub wave: SampleId,
    pub envelope: Envelope,
    pub fixed_rate: bool,
}

/// A CGB noise (hardware channel 4) voice slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoiseVoice {
    pub base_key: u8,
    /// Hardware sound-length counter.
    pub length: u8,
    /// LFSR width selector (`period & 1`): narrow (7-bit) when set, wide
    /// (15-bit) periodic otherwise.
    pub period: u8,
    pub envelope: Envelope,
    pub fixed_rate: bool,
}

/// Selects a child voice by mapping each note from `starting_note` through `table`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeySplitVoice {
    pub starting_note: u8,
    table: Vec<u8>,
    pub children: VoiceGroupId,
}

fn check_key_split_table_len(len: usize) -> Result<(), AudioError> {
    if len > VOICE_SLOT_COUNT {
        return Err(AudioError::KeySplitTableTooLong(len));
    }
    Ok(())
}

impl KeySplitVoice {
    /// Builds a key split whose table entries select slots in `children`.
    ///
    /// # Errors
    ///
    /// Returns [`AudioError::KeySplitTableTooLong`] when the table exceeds
    /// [`VOICE_SLOT_COUNT`] entries.
    pub fn new(
        starting_note: u8,
        table: Vec<u8>,
        children: VoiceGroupId,
    ) -> Result<Self, AudioError> {
        check_key_split_table_len(table.len())?;
        Ok(Self {
            starting_note,
            table,
            children,
        })
    }

    /// The key-split table: `table()[i]` selects a slot index in
    /// [`children`](Self::children) for note
    /// [`starting_note`](Self::starting_note)` + i`.
    #[must_use]
    pub fn table(&self) -> &[u8] {
        &self.table
    }

    fn encoded_table_len(&self) -> u8 {
        u8::try_from(self.table.len()).expect("KeySplitVoice preserves its validated table length")
    }
}

/// Selects the played note's slot directly from a child voicegroup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RhythmVoice {
    pub children: VoiceGroupId,
}

/// One voicegroup slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceEntry {
    DirectSound(DirectSoundVoice),
    Square1(Square1Voice),
    Square2(Square2Voice),
    ProgrammableWave(ProgrammableWaveVoice),
    Noise(NoiseVoice),
    KeySplit(KeySplitVoice),
    Rhythm(RhythmVoice),
    /// An unassigned slot whose position remains addressable.
    Empty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum VoiceKind {
    DirectSound = 0,
    Square1 = 1,
    Square2 = 2,
    ProgrammableWave = 3,
    Noise = 4,
    KeySplit = 5,
    Rhythm = 6,
    Empty = 7,
}

impl VoiceKind {
    const fn tag(self) -> u8 {
        self as u8
    }

    fn from_tag(tag: u8) -> Result<Self, AudioError> {
        match tag {
            tag if tag == Self::DirectSound.tag() => Ok(Self::DirectSound),
            tag if tag == Self::Square1.tag() => Ok(Self::Square1),
            tag if tag == Self::Square2.tag() => Ok(Self::Square2),
            tag if tag == Self::ProgrammableWave.tag() => Ok(Self::ProgrammableWave),
            tag if tag == Self::Noise.tag() => Ok(Self::Noise),
            tag if tag == Self::KeySplit.tag() => Ok(Self::KeySplit),
            tag if tag == Self::Rhythm.tag() => Ok(Self::Rhythm),
            tag if tag == Self::Empty.tag() => Ok(Self::Empty),
            tag => Err(AudioError::UnknownVoiceKind(tag)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum DirectSoundModeTag {
    Resampled = 0,
    Fixed = 1,
    Reverse = 2,
}

impl DirectSoundModeTag {
    const fn tag(self) -> u8 {
        self as u8
    }

    fn from_tag(tag: u8) -> Result<Self, AudioError> {
        match tag {
            tag if tag == Self::Resampled.tag() => Ok(Self::Resampled),
            tag if tag == Self::Fixed.tag() => Ok(Self::Fixed),
            tag if tag == Self::Reverse.tag() => Ok(Self::Reverse),
            tag => Err(AudioError::UnknownDirectSoundMode(tag)),
        }
    }
}

impl From<DirectSoundMode> for DirectSoundModeTag {
    fn from(mode: DirectSoundMode) -> Self {
        match mode {
            DirectSoundMode::Resampled => Self::Resampled,
            DirectSoundMode::Fixed => Self::Fixed,
            DirectSoundMode::Reverse => Self::Reverse,
        }
    }
}

impl From<DirectSoundModeTag> for DirectSoundMode {
    fn from(tag: DirectSoundModeTag) -> Self {
        match tag {
            DirectSoundModeTag::Resampled => Self::Resampled,
            DirectSoundModeTag::Fixed => Self::Fixed,
            DirectSoundModeTag::Reverse => Self::Reverse,
        }
    }
}

const NO_PAN_OVERRIDE: u8 = 0;

fn write_pan(w: &mut Writer, pan: Option<u8>) {
    w.u8(pan.unwrap_or(NO_PAN_OVERRIDE));
}

fn read_pan(r: &mut Reader<'_>) -> Result<Option<u8>, AudioError> {
    Ok(match r.u8()? {
        NO_PAN_OVERRIDE => None,
        other => Some(other),
    })
}

fn check_pan_override(pan: Option<u8>) -> Result<(), AudioError> {
    if pan == Some(NO_PAN_OVERRIDE) {
        return Err(AudioError::PanOverrideZero);
    }
    Ok(())
}

impl VoiceEntry {
    fn write(&self, w: &mut Writer) {
        match self {
            Self::DirectSound(v) => {
                w.u8(VoiceKind::DirectSound.tag());
                w.u8(v.base_key);
                write_pan(w, v.pan);
                w.string(&v.sample.0);
                v.envelope.write(w);
                w.u8(DirectSoundModeTag::from(v.mode).tag());
            }
            Self::Square1(v) => {
                w.u8(VoiceKind::Square1.tag());
                w.u8(v.base_key);
                w.u8(v.length);
                w.u8(v.sweep);
                w.u8(v.duty);
                v.envelope.write(w);
                w.bool(v.fixed_rate);
            }
            Self::Square2(v) => {
                w.u8(VoiceKind::Square2.tag());
                w.u8(v.base_key);
                w.u8(v.length);
                w.u8(v.duty);
                v.envelope.write(w);
                w.bool(v.fixed_rate);
            }
            Self::ProgrammableWave(v) => {
                w.u8(VoiceKind::ProgrammableWave.tag());
                w.u8(v.base_key);
                w.u8(v.length);
                w.string(&v.wave.0);
                v.envelope.write(w);
                w.bool(v.fixed_rate);
            }
            Self::Noise(v) => {
                w.u8(VoiceKind::Noise.tag());
                w.u8(v.base_key);
                w.u8(v.length);
                w.u8(v.period);
                v.envelope.write(w);
                w.bool(v.fixed_rate);
            }
            Self::KeySplit(v) => {
                w.u8(VoiceKind::KeySplit.tag());
                w.u8(v.starting_note);
                w.u8(v.encoded_table_len());
                w.bytes(&v.table);
                w.string(&v.children.0);
            }
            Self::Rhythm(v) => {
                w.u8(VoiceKind::Rhythm.tag());
                w.string(&v.children.0);
            }
            Self::Empty => w.u8(VoiceKind::Empty.tag()),
        }
    }

    fn read(r: &mut Reader<'_>) -> Result<Self, AudioError> {
        match VoiceKind::from_tag(r.u8()?)? {
            VoiceKind::DirectSound => {
                let base_key = r.u8()?;
                let pan = read_pan(r)?;
                let sample = SampleId(r.string()?);
                let envelope = Envelope::read(r)?;
                let mode = DirectSoundModeTag::from_tag(r.u8()?)?.into();
                Ok(Self::DirectSound(DirectSoundVoice {
                    base_key,
                    pan,
                    sample,
                    envelope,
                    mode,
                }))
            }
            VoiceKind::Square1 => {
                let base_key = r.u8()?;
                let length = r.u8()?;
                let sweep = r.u8()?;
                let duty = r.u8()?;
                let envelope = Envelope::read(r)?;
                let fixed_rate = r.bool()?;
                Ok(Self::Square1(Square1Voice {
                    base_key,
                    length,
                    sweep,
                    duty,
                    envelope,
                    fixed_rate,
                }))
            }
            VoiceKind::Square2 => {
                let base_key = r.u8()?;
                let length = r.u8()?;
                let duty = r.u8()?;
                let envelope = Envelope::read(r)?;
                let fixed_rate = r.bool()?;
                Ok(Self::Square2(Square2Voice {
                    base_key,
                    length,
                    duty,
                    envelope,
                    fixed_rate,
                }))
            }
            VoiceKind::ProgrammableWave => {
                let base_key = r.u8()?;
                let length = r.u8()?;
                let wave = SampleId(r.string()?);
                let envelope = Envelope::read(r)?;
                let fixed_rate = r.bool()?;
                Ok(Self::ProgrammableWave(ProgrammableWaveVoice {
                    base_key,
                    length,
                    wave,
                    envelope,
                    fixed_rate,
                }))
            }
            VoiceKind::Noise => {
                let base_key = r.u8()?;
                let length = r.u8()?;
                let period = r.u8()?;
                let envelope = Envelope::read(r)?;
                let fixed_rate = r.bool()?;
                Ok(Self::Noise(NoiseVoice {
                    base_key,
                    length,
                    period,
                    envelope,
                    fixed_rate,
                }))
            }
            VoiceKind::KeySplit => {
                let starting_note = r.u8()?;
                let table_len = usize::from(r.u8()?);
                check_key_split_table_len(table_len)?;
                let table = r.bytes(table_len)?;
                let children = VoiceGroupId(r.string()?);
                Ok(Self::KeySplit(KeySplitVoice::new(
                    starting_note,
                    table,
                    children,
                )?))
            }
            VoiceKind::Rhythm => {
                let children = VoiceGroupId(r.string()?);
                Ok(Self::Rhythm(RhythmVoice { children }))
            }
            VoiceKind::Empty => Ok(Self::Empty),
        }
    }
}

/// Up to [`VOICE_SLOT_COUNT`] instrument slots in `VOICE`-index order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceGroup {
    slots: Vec<VoiceEntry>,
}

impl VoiceGroup {
    /// Builds a voicegroup from slots in `VOICE`-index order.
    ///
    /// # Errors
    ///
    /// Returns [`AudioError::TooManyVoiceSlots`] when the group exceeds
    /// [`VOICE_SLOT_COUNT`], [`AudioError::IdTooLong`] when a referenced id
    /// cannot be encoded, or [`AudioError::PanOverrideZero`] for `Some(0)` pan.
    pub fn new(slots: Vec<VoiceEntry>) -> Result<Self, AudioError> {
        if slots.len() > VOICE_SLOT_COUNT {
            return Err(AudioError::TooManyVoiceSlots(slots.len()));
        }
        for slot in &slots {
            match slot {
                VoiceEntry::DirectSound(v) => {
                    check_id_len(&v.sample.0)?;
                    check_pan_override(v.pan)?;
                }
                VoiceEntry::ProgrammableWave(v) => check_id_len(&v.wave.0)?,
                VoiceEntry::KeySplit(v) => check_id_len(&v.children.0)?,
                VoiceEntry::Rhythm(v) => check_id_len(&v.children.0)?,
                VoiceEntry::Square1(_)
                | VoiceEntry::Square2(_)
                | VoiceEntry::Noise(_)
                | VoiceEntry::Empty => {}
            }
        }
        Ok(Self { slots })
    }

    /// Every slot, in `VOICE`-index order.
    #[must_use]
    pub fn slots(&self) -> &[VoiceEntry] {
        &self.slots
    }

    /// The slot a `VOICE index` command selects, if this group defines one.
    #[must_use]
    pub fn slot(&self, index: usize) -> Option<&VoiceEntry> {
        self.slots.get(index)
    }

    fn encoded_slot_count(&self) -> u8 {
        u8::try_from(self.slots.len()).expect("VoiceGroup preserves its validated slot count")
    }

    /// Encodes this voicegroup for the asset pack.
    ///
    /// # Panics
    ///
    /// Panics only if this type's private validated length invariants are broken.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u8(self.encoded_slot_count());
        for slot in &self.slots {
            slot.write(&mut w);
        }
        w.into_bytes()
    }

    /// Decodes one complete asset-pack voicegroup without resolving referenced ids.
    ///
    /// # Errors
    ///
    /// Returns an [`AudioError`] for malformed data, invalid tags or ids,
    /// out-of-range counts, or trailing bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, AudioError> {
        let mut r = Reader::new(bytes);
        let count = usize::from(r.u8()?);
        if count > VOICE_SLOT_COUNT {
            return Err(AudioError::TooManyVoiceSlots(count));
        }
        let mut slots = Vec::with_capacity(count);
        for _ in 0..count {
            slots.push(VoiceEntry::read(&mut r)?);
        }
        r.expect_eof()?;
        Ok(Self { slots })
    }
}

#[cfg(test)]
mod tests;
