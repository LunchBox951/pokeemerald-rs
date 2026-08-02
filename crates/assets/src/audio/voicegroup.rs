//! [`VoiceGroup`]: up to 128 instrument slots, selected by a song's `VOICE`
//! command.
//!
//! Mirrors upstream `struct ToneData`
//! (`pokeemerald/include/gba/m4a_internal.h`) — one slot per instrument,
//! `type` discriminating a concrete leaf channel kind from the two
//! indirection kinds. Every `.inc` source under
//! `pokeemerald/sound/voicegroups/` (195 in the reference checkout: 180 at
//! top level plus 15 under `drumsets/` and `keysplits/`) assembles to this
//! same 12-byte-per-slot shape via the `voice_directsound`/`voice_square_1`/
//! `voice_square_2`/`voice_programmable_wave`/`voice_noise`/`voice_keysplit`/
//! `voice_keysplit_all` macros (`pokeemerald/asm/macros/music_voice.inc`) —
//! cited here for provenance, not depended on: this schema is what a
//! backend *produces* from that source, not a re-implementation of the
//! assembler macros.
//!
//! # Leaf kinds vs. indirection
//!
//! [`VoiceEntry::DirectSound`]/[`Square1`](VoiceEntry::Square1)/
//! [`Square2`](VoiceEntry::Square2)/
//! [`ProgrammableWave`](VoiceEntry::ProgrammableWave)/[`Noise`](VoiceEntry::Noise)
//! are concrete, playable channel kinds. [`VoiceEntry::KeySplit`] and
//! [`VoiceEntry::Rhythm`] are indirections: rather than embedding another
//! `VoiceGroup`'s worth of leaf entries inline, each carries a
//! [`VoiceGroupId`] — the stable pack id of *another* `VoiceGroup` entry to
//! resolve through (matching upstream's own pointer indirection: e.g.
//! `voice_keysplit voicegroup_trumpet_keysplit, keysplit_trumpet` points at
//! a small, separately-defined three-slot group). Several such child groups
//! (e.g. `trumpet_keysplit`) are shared by many different songs' top-level
//! voicegroups, so this keeps the pack from duplicating them.
//!
//! Resolving a `KeySplit`/`Rhythm` reference into a concrete leaf instrument
//! — including upstream's own runtime rule that a key-split/rhythm child
//! must not itself be another key-split/rhythm (`ply_note` aborts the note
//! rather than recursing) — is `#115` child 3's job (the voicegroup
//! resolver), not this schema's: nothing here rejects a `VoiceGroupId` that
//! happens to point at a group full of further indirections, the same way
//! nothing in [`crate::map_events`] validates that a warp's destination map
//! actually exists.
//!
//! # Pan is a `DirectSound`-only field
//!
//! Only [`DirectSoundVoice`] has a `pan`, because only `DirectSound` voices
//! have one upstream. The two are laid out differently despite sharing the
//! `pan` *operand* name in `asm/macros/music_voice.inc`:
//!
//! - `_voice_directsound` writes `.byte 0` at `ToneData` offset 2
//!   (`length`) and the `0x80 | pan` byte at offset 3 (`pan_sweep`) — a
//!   real pan field.
//! - `_voice_square_1`, `_voice_square_2`, `_voice_programmable_wave`, and
//!   `_voice_noise` write the `0x80 | pan` byte at offset **2**, which is
//!   `ToneData::length`, not `pan_sweep`. Nothing ever reads it as pan:
//!   `ply_note`'s CGB branch (`src/m4a_1.s`) copies `ToneData_length` into
//!   `CgbChannel_length`, and `m4a.c`'s `CgbSound` writes that straight to
//!   the channel's `NRx1` register as the hardware sound-length counter.
//!   The macro operand is misleadingly named; the field is a duration.
//!
//! So the CGB kinds carry [`Square1Voice::length`] and its siblings — a
//! plain `u8`, not an `Option`, since a sound-length counter has no
//! "override vs. inherit" question. Every CGB voice in the reference
//! checkout's `sound/voicegroups/` passes `0` for that operand, so naming
//! the field what it is loses nothing.
//!
//! [`DirectSoundVoice::pan`] stays `Option<u8>`: `None` means "no
//! per-instrument override" (inherit the track's pan), `Some(1..=127)` an
//! explicit override. That mirrors a real limitation of the upstream wire
//! format rather than an invented one — the macro writes `0` when the
//! operand is `0` and `0x80 | pan` otherwise, so upstream itself cannot
//! distinguish "no override" from "override to exactly 0", and `Some(0)` is
//! consequently never a value [`VoiceEntry`] decode produces. Upstream only
//! *honours* `ToneData::pan_sweep` as pan through the rhythm indirection,
//! where `ply_note` latches it into `SoundChannel::rhythmPan` for the child
//! voice it selects.

use super::cursor::{check_id_len, Reader, Writer};
use super::error::AudioError;
use super::sample::SampleId;

/// The maximum number of slots a [`VoiceGroup`] may declare: every raw
/// `VOICE`/key-split-table command byte addresses a slot in `0..=127`
/// (`MUS_TITLE`'s own MIDI source selects slot 127 on one channel — see
/// `super`'s module docs), so 128 is not a soft convenience limit but the
/// full addressable range.
pub const VOICE_SLOT_COUNT: usize = 128;

/// A voicegroup's stable pack id — the normalized asset id a song or a
/// [`KeySplitVoice`]/[`RhythmVoice`] references (e.g.
/// `"audio/voicegroup/trumpet_keysplit"`), not the group's contents.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VoiceGroupId(pub String);

/// An instrument's attack/decay/sustain/release envelope (upstream
/// `ToneData`'s trailing four bytes).
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

/// Which of the three on-disk `DirectSound` forms a [`DirectSoundVoice`] is.
///
/// # Why these are exclusive
///
/// Upstream's `ToneData::type` is a bit field, not an enum
/// (`TONEDATA_TYPE_FIX` `0x08`, `TONEDATA_TYPE_REV` `0x10`,
/// `TONEDATA_TYPE_CMP` `0x20`, `constants/m4a_constants.inc`), so a voice
/// could in principle set several at once. Narrowing to three exclusive
/// modes is deliberate and checked: every `DirectSound` voice in
/// `sound/voicegroups/` comes from exactly one of the three
/// `voice_directsound*` macros, emitting `0`, `8`, or `16`
/// (`asm/macros/music_voice.inc`) — no voicegroup combines the bits, and no
/// macro could. `sound/cry_tables.inc` *does* combine them (`cry` is `0x20`,
/// `cry_reverse` `0x20 | 0x10`), but cries are a separate species-indexed
/// table outside these schemas' scope — see `super`'s module docs. This enum
/// is the one place to widen should a backend ever need a combined form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectSoundMode {
    /// `voice_directsound` (type tag `0`): resample the sample to the
    /// played note's pitch — the common case.
    Resampled,
    /// `voice_directsound_no_resample` (type tag `8`, upstream
    /// `TONEDATA_TYPE_FIX`): play the sample back unpitched, at its own
    /// recorded rate, ignoring the played note's key entirely.
    Fixed,
    /// `voice_directsound_alt` (type tag `0x10`, upstream
    /// `TONEDATA_TYPE_REV`): play the sample **backwards**.
    ///
    /// `SoundMainRAM_Unk1` (`pokeemerald/src/m4a_1.s`) branches on
    /// `TONEDATA_TYPE_REV` to start the channel's read pointer at the end of
    /// the wave and walk it towards the start — a distinct audible
    /// behaviour, not an unexplained on-disk variant. Rare in voicegroups (2
    /// occurrences project-wide, both in `sound/voicegroups/rs_sfx_2.inc`),
    /// though it is also the bit `gCryTable_Reverse` sets over every cry.
    Reverse,
}

/// A `DirectSound` (PCM sample playback) voice slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectSoundVoice {
    pub base_key: u8,
    /// Per-instrument pan override (upstream `ToneData::pan_sweep`), or
    /// `None` to inherit the track's pan. The only voice kind with a pan at
    /// all — see the module docs, including the note that upstream honours
    /// it via the rhythm indirection's `rhythmPan`.
    ///
    /// `Some(0)` is invalid: the wire's `0` byte is the no-override
    /// sentinel (as it is upstream — mid2agb writes `0` for none and
    /// `0x80 | pan` otherwise), so [`VoiceGroup::new`] rejects it with
    /// [`AudioError::PanOverrideZero`] rather than let it silently decay
    /// to `None` across a round trip.
    pub pan: Option<u8>,
    /// The [`super::sample::Sample::DirectSound`] this voice plays.
    pub sample: SampleId,
    pub envelope: Envelope,
    pub mode: DirectSoundMode,
}

/// A CGB square-channel-1 voice slot (has a hardware sweep register;
/// channel 2 does not — see [`Square2Voice`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Square1Voice {
    pub base_key: u8,
    /// Hardware sound-length counter (upstream `ToneData::length` → the
    /// channel's `NRx1`). Not a pan, despite the source macro's operand
    /// name — see the module docs.
    pub length: u8,
    /// Raw `NR10`-style sweep byte.
    pub sweep: u8,
    /// Duty cycle selector, `0..=3`.
    pub duty: u8,
    pub envelope: Envelope,
    /// The `_alt` on-disk suffix (type tag `+8`, `TONEDATA_TYPE_FIX`).
    pub fixed_rate: bool,
}

/// A CGB square-channel-2 voice slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Square2Voice {
    pub base_key: u8,
    /// Hardware sound-length counter — see [`Square1Voice::length`].
    pub length: u8,
    pub duty: u8,
    pub envelope: Envelope,
    pub fixed_rate: bool,
}

/// A CGB programmable-wave (hardware channel 3) voice slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgrammableWaveVoice {
    pub base_key: u8,
    /// Hardware sound-length counter — see [`Square1Voice::length`].
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
    /// Hardware sound-length counter — see [`Square1Voice::length`].
    pub length: u8,
    /// LFSR width selector (`period & 1`): narrow (7-bit) when set, wide
    /// (15-bit) periodic otherwise.
    pub period: u8,
    pub envelope: Envelope,
    pub fixed_rate: bool,
}

/// A key-split indirection (`voice_keysplit`, upstream `TONEDATA_TYPE_SPL`):
/// a played note in `starting_note..starting_note + table.len()` looks up
/// `table[key - starting_note]` to select a slot in the [`VoiceGroupId`]
/// group; the played key otherwise keeps being used for pitch (upstream
/// `ply_note`, `m4a_1.s`). Notes outside that range have no defined mapping
/// in this slot — a real limit of the upstream format, not an invented one:
/// `sound/keysplit_tables.inc:1-11` documents mks4agb biasing a table's label
/// backwards by its starting note because tables cover only a subrange, and
/// `src/m4a_1.s:1589-1591` indexes `table[raw key]` with no offset — so
/// [`starting_note`](Self::starting_note) honestly un-biases that format.
///
/// Build one with [`KeySplitVoice::new`]: the table's `0..=`[`VOICE_SLOT_COUNT`]
/// length bound is what makes [`VoiceGroup::encode`]'s `u8` table-length
/// field total, so the table is private and the bound is enforced at
/// construction rather than left to a panic at encode time. [`VoiceGroup::decode`]
/// enforces the same bound on untrusted input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeySplitVoice {
    pub starting_note: u8,
    table: Vec<u8>,
    pub children: VoiceGroupId,
}

impl KeySplitVoice {
    /// Build a key-split indirection.
    ///
    /// `table[i]` selects a slot index in `children` for note
    /// `starting_note + i`.
    ///
    /// # Errors
    ///
    /// [`AudioError::KeySplitTableTooLong`] if `table.len() > `[`VOICE_SLOT_COUNT`]
    /// — a table entry names a slot in `children`, which itself holds at
    /// most [`VOICE_SLOT_COUNT`] slots.
    pub fn new(
        starting_note: u8,
        table: Vec<u8>,
        children: VoiceGroupId,
    ) -> Result<Self, AudioError> {
        if table.len() > VOICE_SLOT_COUNT {
            return Err(AudioError::KeySplitTableTooLong(table.len()));
        }
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
}

/// A rhythm indirection (`voice_keysplit_all`, upstream `TONEDATA_TYPE_RHY`):
/// the played note key selects a slot in the [`VoiceGroupId`] group
/// *directly* (no separate key-split table) — a drum-kit-style mapping
/// where each key triggers an independently-pitched hit, substituting the
/// child slot's own base key for the played note (upstream `ply_note`).
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
}

const KIND_DIRECT_SOUND: u8 = 0;
const KIND_SQUARE_1: u8 = 1;
const KIND_SQUARE_2: u8 = 2;
const KIND_PROGRAMMABLE_WAVE: u8 = 3;
const KIND_NOISE: u8 = 4;
const KIND_KEY_SPLIT: u8 = 5;
const KIND_RHYTHM: u8 = 6;

const MODE_RESAMPLED: u8 = 0;
const MODE_FIXED: u8 = 1;
const MODE_REVERSE: u8 = 2;

fn write_pan(w: &mut Writer, pan: Option<u8>) {
    w.u8(pan.unwrap_or(0));
}

fn read_pan(r: &mut Reader<'_>) -> Result<Option<u8>, AudioError> {
    Ok(match r.u8()? {
        0 => None,
        other => Some(other),
    })
}

impl VoiceEntry {
    fn write(&self, w: &mut Writer) {
        match self {
            Self::DirectSound(v) => {
                w.u8(KIND_DIRECT_SOUND);
                w.u8(v.base_key);
                write_pan(w, v.pan);
                w.string(&v.sample.0);
                v.envelope.write(w);
                w.u8(match v.mode {
                    DirectSoundMode::Resampled => MODE_RESAMPLED,
                    DirectSoundMode::Fixed => MODE_FIXED,
                    DirectSoundMode::Reverse => MODE_REVERSE,
                });
            }
            Self::Square1(v) => {
                w.u8(KIND_SQUARE_1);
                w.u8(v.base_key);
                w.u8(v.length);
                w.u8(v.sweep);
                w.u8(v.duty);
                v.envelope.write(w);
                w.bool(v.fixed_rate);
            }
            Self::Square2(v) => {
                w.u8(KIND_SQUARE_2);
                w.u8(v.base_key);
                w.u8(v.length);
                w.u8(v.duty);
                v.envelope.write(w);
                w.bool(v.fixed_rate);
            }
            Self::ProgrammableWave(v) => {
                w.u8(KIND_PROGRAMMABLE_WAVE);
                w.u8(v.base_key);
                w.u8(v.length);
                w.string(&v.wave.0);
                v.envelope.write(w);
                w.bool(v.fixed_rate);
            }
            Self::Noise(v) => {
                w.u8(KIND_NOISE);
                w.u8(v.base_key);
                w.u8(v.length);
                w.u8(v.period);
                v.envelope.write(w);
                w.bool(v.fixed_rate);
            }
            Self::KeySplit(v) => {
                w.u8(KIND_KEY_SPLIT);
                w.u8(v.starting_note);
                // Total by construction: `KeySplitVoice::new` and
                // `VoiceGroup::decode` both cap the table at
                // `VOICE_SLOT_COUNT` (128), and the field is private.
                let len = u8::try_from(v.table.len())
                    .expect("KeySplitVoice::new enforces table.len() <= VOICE_SLOT_COUNT (128)");
                w.u8(len);
                w.bytes(&v.table);
                w.string(&v.children.0);
            }
            Self::Rhythm(v) => {
                w.u8(KIND_RHYTHM);
                w.string(&v.children.0);
            }
        }
    }

    fn read(r: &mut Reader<'_>) -> Result<Self, AudioError> {
        match r.u8()? {
            KIND_DIRECT_SOUND => {
                let base_key = r.u8()?;
                let pan = read_pan(r)?;
                let sample = SampleId(r.string()?);
                let envelope = Envelope::read(r)?;
                let mode = match r.u8()? {
                    MODE_RESAMPLED => DirectSoundMode::Resampled,
                    MODE_FIXED => DirectSoundMode::Fixed,
                    MODE_REVERSE => DirectSoundMode::Reverse,
                    other => return Err(AudioError::UnknownDirectSoundMode(other)),
                };
                Ok(Self::DirectSound(DirectSoundVoice {
                    base_key,
                    pan,
                    sample,
                    envelope,
                    mode,
                }))
            }
            KIND_SQUARE_1 => {
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
            KIND_SQUARE_2 => {
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
            KIND_PROGRAMMABLE_WAVE => {
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
            KIND_NOISE => {
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
            KIND_KEY_SPLIT => {
                let starting_note = r.u8()?;
                // The read side does not trust the encoded length: the
                // documented `VOICE_SLOT_COUNT` bound is enforced here too,
                // not only in `KeySplitVoice::new`.
                let table_len = usize::from(r.u8()?);
                if table_len > VOICE_SLOT_COUNT {
                    return Err(AudioError::KeySplitTableTooLong(table_len));
                }
                let table = r.bytes(table_len)?;
                let children = VoiceGroupId(r.string()?);
                Ok(Self::KeySplit(KeySplitVoice {
                    starting_note,
                    table,
                    children,
                }))
            }
            KIND_RHYTHM => {
                let children = VoiceGroupId(r.string()?);
                Ok(Self::Rhythm(RhythmVoice { children }))
            }
            other => Err(AudioError::UnknownVoiceKind(other)),
        }
    }
}

/// Up to [`VOICE_SLOT_COUNT`] instrument slots, selected by a song's `VOICE`
/// command. See the module docs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceGroup {
    slots: Vec<VoiceEntry>,
}

impl VoiceGroup {
    /// Build a voicegroup from `slots`, slot `0` first.
    ///
    /// # Errors
    ///
    /// [`AudioError::TooManyVoiceSlots`] if `slots.len() > `[`VOICE_SLOT_COUNT`];
    /// [`AudioError::IdTooLong`] if any slot references a [`SampleId`] or
    /// [`VoiceGroupId`] too long for the `u16` length prefix the encoding
    /// writes for it. Both bounds are what make [`Self::encode`] total.
    pub fn new(slots: Vec<VoiceEntry>) -> Result<Self, AudioError> {
        if slots.len() > VOICE_SLOT_COUNT {
            return Err(AudioError::TooManyVoiceSlots(slots.len()));
        }
        for slot in &slots {
            match slot {
                VoiceEntry::DirectSound(v) => {
                    check_id_len(&v.sample.0)?;
                    // The wire's `0` byte is the no-override sentinel
                    // (`write_pan`/`read_pan`), so `Some(0)` would silently
                    // decay to `None` across a round trip -- rejected here,
                    // where it is still reportable (review finding on
                    // #193). Upstream cannot express it either: mid2agb's
                    // pan operand is `0` for none, `0x80 | pan` otherwise.
                    if v.pan == Some(0) {
                        return Err(AudioError::PanOverrideZero);
                    }
                }
                VoiceEntry::ProgrammableWave(v) => check_id_len(&v.wave.0)?,
                VoiceEntry::KeySplit(v) => check_id_len(&v.children.0)?,
                VoiceEntry::Rhythm(v) => check_id_len(&v.children.0)?,
                VoiceEntry::Square1(_) | VoiceEntry::Square2(_) | VoiceEntry::Noise(_) => {}
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

    /// Encode to this schema's binary form. Staleness is gated by
    /// [`crate::pack::FORMAT_VERSION`] — see `super`'s module docs,
    /// "Versioning".
    ///
    /// # Panics
    ///
    /// Unreachable: [`Self::new`] is the only way to build a `VoiceGroup`,
    /// and it enforces `slots.len() <= `[`VOICE_SLOT_COUNT`] (128) and every
    /// referenced id's length, returning [`AudioError`] where a caller can
    /// see it. [`KeySplitVoice::new`] likewise bounds the table length each
    /// key-split slot writes.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        let count = u8::try_from(self.slots.len())
            .expect("VoiceGroup::new enforces slots.len() <= VOICE_SLOT_COUNT (128)");
        w.u8(count);
        for slot in &self.slots {
            slot.write(&mut w);
        }
        w.into_bytes()
    }

    /// Decode from [`encode`](Self::encode)'s binary form.
    ///
    /// Structural decode only: a [`VoiceGroupId`]/[`SampleId`] this group
    /// references is not resolved or checked for existence, and a
    /// key-split/rhythm child that is itself another indirection is not
    /// rejected — that is `#115` child 3's job (see the module docs).
    ///
    /// # Errors
    ///
    /// [`AudioError::Truncated`] if `bytes` is shorter than the format
    /// requires; [`AudioError::TooManyVoiceSlots`] if the declared slot count
    /// exceeds [`VOICE_SLOT_COUNT`] and [`AudioError::KeySplitTableTooLong`]
    /// if a declared key-split table length does (the read side does not
    /// trust encoded counts, same discipline as `crate::pack`'s directory
    /// parser); [`AudioError::InvalidString`] for a non-UTF-8 id;
    /// [`AudioError::UnknownVoiceKind`] / [`AudioError::UnknownDirectSoundMode`]
    /// for an unrecognized tag byte.
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
