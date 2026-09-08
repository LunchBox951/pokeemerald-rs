//! The sound engine's data: `audio/sample/*` and `audio/voicegroup/*`.
//!
//! Everything `MUS_TITLE` plays through except the song itself. The ROM
//! holds the m4a engine's own structs (`pokeemerald/include/gba/m4a_internal.h`)
//! and this reader turns them into [`assets`]' backend-neutral schemas, then
//! emits them with the same encoders `cargo xtask extract` uses, so the two
//! backends cannot drift on the wire format.
//!
//! # Samples
//!
//! A `DirectSound` sample is a 16-byte `WaveData` header (`type`, `status`,
//! `freq`, `loopStart`, `size`) followed by `size` signed bytes. `freq` is
//! the engine's pre-scaled pitch constant and is carried through unchanged.
//! `status & 0x4000` is the loop flag; a sample without it has no loop, so
//! `loopStart` is not meaningful and is dropped. A `type != 0` sample is
//! DPCM-compressed and refused: the pack stores PCM only, and no instrument
//! in scope is compressed.
//!
//! A programmable wave is the bare 16-byte table CGB channel 3 plays.
//!
//! # Voicegroups
//!
//! A voicegroup is a run of 12-byte `ToneData` slots. `type` selects the
//! kind: `0x00`/`0x08`/`0x10` are `DirectSound` resampled, fixed-rate, and
//! reverse; `0x01..=0x04` the four CGB channels, `| 0x08` for fixed rate;
//! `0x40` a key split and `0x80` a rhythm set, both pointing at another
//! voicegroup. Pointers to samples, voicegroups, and key-split tables become
//! pack ids by looking the address up in the profile; a pointer the profile
//! does not record is [`ImportError::UnresolvedPointer`], never a guess.
//!
//! How many slots a group reads is the one place the ROM and the pack
//! disagree in shape. The pack gives every group 128 slots. The ROM stores
//! only what the `.inc` declared, and the mixer's `voicegroup + voice * 12`
//! fetch is unchecked, so a song can select a slot past that tail and get
//! whatever the linker placed next. The checkout backend models that for
//! the one group a song references directly (issue #201) and pads every
//! other group with `Empty`. This reader does the same: a group some song's
//! header points at reads [`VoicegroupRoot::addressable_slots`] contiguous
//! slots; any other reads its `declared_slots` after `starting_note` empty
//! ones and pads the rest.

use assets::{
    DirectSoundMode, DirectSoundSample, DirectSoundVoice, Envelope, KeySplitVoice, NoiseVoice,
    ProgrammableWave, ProgrammableWaveVoice, RhythmVoice, Sample, SampleId, Square1Voice,
    Square2Voice, VoiceEntry, VoiceGroup, VoiceGroupId,
};
use pack_format::{raw_entry, PackEntry, PackWriter};

use super::len_usize;
use crate::error::ImportError;
use crate::reader::{GbaPtr, RomReader};
use crate::rom::Rom;
use crate::roots::{AudioRoots, Roots, SampleRoot, VoicegroupRoot};

/// `sizeof(struct WaveData)` up to `data`.
const WAVE_HEADER_BYTES: u32 = 16;
/// `WaveData.status`'s loop flag.
const WAVE_STATUS_LOOP: u16 = 0x4000;
/// Offset of `WaveData.type`.
const WAVE_TYPE: usize = 0;
/// Offset of `WaveData.status`.
const WAVE_STATUS: usize = 2;
/// Offset of `WaveData.freq`.
const WAVE_FREQ: usize = 4;
/// Offset of `WaveData.loopStart`.
const WAVE_LOOP_START: usize = 8;
/// Offset of `WaveData.size`.
const WAVE_SIZE: usize = 12;
/// A programmable wave table's length.
const WAVE_TABLE_BYTES: u32 = 16;

/// `sizeof(struct ToneData)`.
const SLOT_BYTES: usize = 12;
/// `ToneData.type`'s channel-kind bits.
const TYPE_CGB_MASK: u8 = 0x07;
/// `ToneData.type`'s fixed-rate bit.
const TYPE_FIXED: u8 = 0x08;
/// `voice_directsound_alt`'s type: play the sample in reverse.
const TYPE_REVERSE: u8 = 0x10;
/// A key-split slot.
const TYPE_KEY_SPLIT: u8 = 0x40;
/// A rhythm slot.
const TYPE_RHYTHM: u8 = 0x80;
/// `ToneData.pan_sweep`'s "pan is set" bit.
const PAN_SET: u8 = 0x80;

/// Write every sample and voicegroup.
///
/// # Errors
///
/// [`ImportError::CompressedSample`] for a DPCM sample;
/// [`ImportError::StructMismatch`] if a `WaveData` header disagrees with
/// the profile; [`ImportError::VoiceType`] and
/// [`ImportError::UnresolvedPointer`] for a slot the profile cannot
/// describe; [`ImportError::Audio`] if the schema rejects a value;
/// [`ImportError::Truncated`] if any root runs past the end of the ROM.
pub(crate) fn write(rom: &Rom, roots: &Roots, writer: &mut PackWriter) -> Result<(), ImportError> {
    let reader = rom.reader();
    let audio = &roots.audio;
    for root in audio.direct_sound {
        writer.push(direct_sound(&reader, root)?);
    }
    for root in audio.programmable_wave {
        writer.push(programmable_wave(&reader, root)?);
    }
    for root in audio.voicegroups {
        writer.push(voicegroup(&reader, audio, root)?);
    }
    Ok(())
}

/// Read one `DirectSound` sample: its `WaveData` header and PCM payload.
pub(crate) fn direct_sound(
    reader: &RomReader<'_>,
    root: &SampleRoot,
) -> Result<PackEntry, ImportError> {
    let id = root.id;
    if root.header_len != WAVE_HEADER_BYTES {
        return Err(ImportError::StructMismatch {
            root: id,
            field: "WaveData",
        });
    }
    let base = root.addr.offset();
    // The whole header is read first so a header that runs off the image
    // is a truncation, whatever its first bytes happen to hold.
    let header = reader.slice_at(base, len_usize(WAVE_HEADER_BYTES))?;
    let u16_at = |off: usize| u16::from_le_bytes([header[off], header[off + 1]]);
    let u32_at = |off: usize| {
        u32::from_le_bytes([
            header[off],
            header[off + 1],
            header[off + 2],
            header[off + 3],
        ])
    };
    if u16_at(WAVE_TYPE) != 0 {
        return Err(ImportError::CompressedSample { id });
    }
    let status = u16_at(WAVE_STATUS);
    let freq = u32_at(WAVE_FREQ);
    let loop_start = u32_at(WAVE_LOOP_START);
    let size = u32_at(WAVE_SIZE);
    if size != root.data_len {
        return Err(ImportError::StructMismatch {
            root: id,
            field: "WaveData.size",
        });
    }
    let data = reader
        .slice_at(base + len_usize(WAVE_HEADER_BYTES), len_usize(size))?
        .iter()
        .map(|&byte| i8::from_le_bytes([byte]))
        .collect();
    let looping = status & WAVE_STATUS_LOOP != 0;
    let sample = DirectSoundSample::new(freq, looping.then_some(loop_start), data)
        .map_err(|source| ImportError::Audio { id, source })?;
    Ok(raw_entry(
        id.to_owned(),
        Sample::DirectSound(sample).encode(),
    ))
}

/// Read one programmable-wave table.
pub(crate) fn programmable_wave(
    reader: &RomReader<'_>,
    root: &SampleRoot,
) -> Result<PackEntry, ImportError> {
    if root.header_len != 0 || root.data_len != WAVE_TABLE_BYTES {
        return Err(ImportError::StructMismatch {
            root: root.id,
            field: "wave table",
        });
    }
    let bytes = reader.slice(root.addr, len_usize(WAVE_TABLE_BYTES))?;
    let mut table = [0u8; 16];
    table.copy_from_slice(bytes);
    Ok(raw_entry(
        root.id.to_owned(),
        Sample::ProgrammableWave(ProgrammableWave { table }).encode(),
    ))
}

/// Read one voicegroup, slot by slot.
pub(crate) fn voicegroup(
    reader: &RomReader<'_>,
    audio: &AudioRoots,
    root: &VoicegroupRoot,
) -> Result<PackEntry, ImportError> {
    let id = root.id;
    let total = usize::from(root.addressable_slots);
    let song_selected = audio.songs.iter().any(|song| song.voicegroup == root.addr);
    // The slots the ROM is read for, as `(first, count)`.
    let (first, count) = if song_selected {
        (0, total)
    } else {
        (
            usize::from(root.starting_note),
            usize::from(root.declared_slots),
        )
    };
    if first.saturating_add(count) > total {
        return Err(ImportError::Length {
            what: "voicegroup slot count",
            value: first.saturating_add(count),
            max: total,
        });
    }

    let mut slots = vec![VoiceEntry::Empty; first];
    for index in first..first + count {
        let bytes = reader.table_entry(id, root.addr, index, total, SLOT_BYTES)?;
        let base = root.addr.offset().saturating_add(index * SLOT_BYTES);
        slots.push(slot(reader, audio, id, index, base, bytes)?);
    }
    slots.resize(total, VoiceEntry::Empty);

    let group = VoiceGroup::new(slots).map_err(|source| ImportError::Audio { id, source })?;
    Ok(raw_entry(id.to_owned(), group.encode()))
}

/// Decode one `ToneData` slot.
///
/// `bytes` is the slot's 12 bytes and `base` its ROM offset, which is only
/// needed to read its pointers through the checked [`RomReader::ptr`].
fn slot(
    reader: &RomReader<'_>,
    audio: &AudioRoots,
    root: &'static str,
    index: usize,
    base: usize,
    bytes: &[u8],
) -> Result<VoiceEntry, ImportError> {
    let kind = bytes[0];
    match kind {
        TYPE_RHYTHM => {
            let children = child_group(reader, audio, root, index, base)?;
            Ok(VoiceEntry::Rhythm(RhythmVoice { children }))
        }
        TYPE_KEY_SPLIT => {
            let children = child_group(reader, audio, root, index, base)?;
            key_split(reader, audio, root, index, base, children)
        }
        _ => leaf(reader, audio, root, index, base, bytes),
    }
}

/// The voicegroup a rhythm or key-split slot's `wav` word points at.
fn child_group(
    reader: &RomReader<'_>,
    audio: &AudioRoots,
    root: &'static str,
    index: usize,
    base: usize,
) -> Result<VoiceGroupId, ImportError> {
    let ptr = reader.ptr(base + 4)?;
    voicegroup_id(audio, ptr).ok_or(ImportError::UnresolvedPointer {
        root,
        slot: index,
        what: "a voicegroup",
        ptr,
    })
}

/// A key-split slot: the table pointer sits where a leaf keeps its
/// envelope, and the table itself is read from the note the profile says
/// it starts at.
fn key_split(
    reader: &RomReader<'_>,
    audio: &AudioRoots,
    root: &'static str,
    index: usize,
    base: usize,
    children: VoiceGroupId,
) -> Result<VoiceEntry, ImportError> {
    let ptr = reader.ptr(base + 8)?;
    let table = audio
        .keysplits
        .iter()
        .find(|table| table.addr == ptr)
        .ok_or(ImportError::UnresolvedPointer {
            root,
            slot: index,
            what: "a key-split table",
            ptr,
        })?;
    let entries = reader
        .slice_at(
            table.addr.offset() + usize::from(table.starting_note),
            usize::from(table.len),
        )?
        .to_vec();
    let voice = KeySplitVoice::new(table.starting_note, entries, children)
        .map_err(|source| ImportError::Audio { id: root, source })?;
    Ok(VoiceEntry::KeySplit(voice))
}

/// A playable slot: `DirectSound` or one of the four CGB channels.
///
/// The CGB macros write their channel byte (`sweep`, `duty`, `period`) at
/// offset 3, where `DirectSound` keeps `pan_sweep`, and square 1's `duty`
/// at offset 4, the first byte of the pointer word
/// (`pokeemerald/asm/macros/music_voice.inc`).
fn leaf(
    reader: &RomReader<'_>,
    audio: &AudioRoots,
    root: &'static str,
    index: usize,
    base: usize,
    bytes: &[u8],
) -> Result<VoiceEntry, ImportError> {
    let kind = bytes[0];
    let base_key = bytes[1];
    let length = bytes[2];
    let channel = bytes[3];
    let envelope = Envelope {
        attack: bytes[8],
        decay: bytes[9],
        sustain: bytes[10],
        release: bytes[11],
    };
    let fixed_rate = kind & TYPE_FIXED != 0;
    let voice_type = ImportError::VoiceType {
        root,
        slot: index,
        kind,
    };
    // Any bit outside the kind and fixed-rate bits is a type the engine
    // would misread too, reverse `DirectSound` aside.
    if kind & !(TYPE_CGB_MASK | TYPE_FIXED) != 0 && kind != TYPE_REVERSE {
        return Err(voice_type);
    }
    let entry = match kind & TYPE_CGB_MASK {
        0 => {
            let mode = match kind {
                0 => DirectSoundMode::Resampled,
                TYPE_FIXED => DirectSoundMode::Fixed,
                TYPE_REVERSE => DirectSoundMode::Reverse,
                _ => return Err(voice_type),
            };
            let ptr = reader.ptr(base + 4)?;
            let sample =
                sample_id(audio.direct_sound, ptr).ok_or(ImportError::UnresolvedPointer {
                    root,
                    slot: index,
                    what: "a DirectSound sample",
                    ptr,
                })?;
            let pan = (channel & PAN_SET != 0).then_some(channel & !PAN_SET);
            VoiceEntry::DirectSound(DirectSoundVoice {
                base_key,
                pan,
                sample,
                envelope,
                mode,
            })
        }
        1 => VoiceEntry::Square1(Square1Voice {
            base_key,
            length,
            sweep: channel,
            duty: bytes[4],
            envelope,
            fixed_rate,
        }),
        2 => VoiceEntry::Square2(Square2Voice {
            base_key,
            length,
            duty: bytes[4],
            envelope,
            fixed_rate,
        }),
        3 => {
            let ptr = reader.ptr(base + 4)?;
            let wave =
                sample_id(audio.programmable_wave, ptr).ok_or(ImportError::UnresolvedPointer {
                    root,
                    slot: index,
                    what: "a programmable wave",
                    ptr,
                })?;
            VoiceEntry::ProgrammableWave(ProgrammableWaveVoice {
                base_key,
                length,
                wave,
                envelope,
                fixed_rate,
            })
        }
        4 => VoiceEntry::Noise(NoiseVoice {
            base_key,
            length,
            period: bytes[4],
            envelope,
            fixed_rate,
        }),
        _ => return Err(voice_type),
    };
    Ok(entry)
}

/// The pack id of the sample whose root sits at `ptr`.
fn sample_id(roots: &[SampleRoot], ptr: GbaPtr) -> Option<SampleId> {
    roots
        .iter()
        .find(|root| root.addr == ptr)
        .map(|root| SampleId(root.id.to_owned()))
}

/// The pack id of the voicegroup whose root sits at `ptr`.
fn voicegroup_id(audio: &AudioRoots, ptr: GbaPtr) -> Option<VoiceGroupId> {
    audio
        .voicegroups
        .iter()
        .find(|root| root.addr == ptr)
        .map(|root| VoiceGroupId(root.id.to_owned()))
}

#[cfg(test)]
mod tests;
