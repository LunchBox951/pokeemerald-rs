//! The species cries: `audio/cry/table` and `audio/sample/direct-sound/cry/*`.
//!
//! `gCryTable` and `gCryTable_Reverse` (`pokeemerald/sound/cry_tables.inc`)
//! are two runs of [`CRY_TABLE_LEN`] 12-byte `ToneData` records. A `cry`
//! record is `0x20, 60, 0, 0, <wav>, 0xff, 0, 0xff, 0` and a `cry_reverse`
//! record the same with type `0x30`
//! (`pokeemerald/asm/macros/music_voice.inc:142-152`), so the two tables name
//! the same sample at every position and differ only in the reverse flag. The
//! pack keeps one table of sample ids, in position order, and
//! `assets::CryTable` derives the reverse flag from the playback mode.
//!
//! # Samples
//!
//! Each cry is a `WaveData` built with `wav2agb -b -c -l 1 --no-pad`
//! (`pokeemerald/audio_rules.mk:19-22`): `type` 1, no loop, `size` the decoded
//! sample count, followed by DPCM blocks. A block is 64 samples in 33 bytes:
//! the first sample, then 32 bytes of 4-bit indexes into
//! `gDeltaEncodingTable` (`pokeemerald/src/m4a_tables.c:47`). The mixer's
//! decoder (`SoundMainRAM_Unk2`, `pokeemerald/src/m4a_1.s:671-700`) takes only
//! the low nibble of a block's first index byte, then the high and low nibble
//! of each later byte, adding each delta to the previous level with 8-bit
//! wraparound. The level restarts at every block.
//!
//! # Guard sample
//!
//! The pack retains the sample the mixer interpolates toward past `size`
//! (the module docs of [`super`] own that contract). The mixer reads it by
//! decoding index `size` of the block that holds it, so this reader decodes
//! `size + 1` samples from the ROM bytes the mixer would read. `--no-pad`
//! omits the last block's unused tail, so that read can end in a byte another
//! object follows; it is taken from the ROM as the mixer takes it, and a read
//! past the image is [`ImportError::Truncated`], never a zero.

use std::collections::BTreeMap;

use assets::audio::{CryTable, CRY_TABLE_LEN, CRY_TABLE_PACK_ID};
use assets::{DirectSoundSample, Sample, SampleId};
use pack_format::{raw_entry, PackEntry};

use super::{WAVE_FREQ, WAVE_HEADER_BYTES, WAVE_SIZE, WAVE_STATUS, WAVE_TYPE};
use crate::domains::len_usize;
use crate::error::ImportError;
use crate::reader::RomReader;
use crate::roots::{CryRoots, SampleRoot};

/// `gCryTable`'s symbol, for messages.
const NORMAL_TABLE: &str = "gCryTable";
/// `gCryTable_Reverse`'s symbol, for messages.
const REVERSE_TABLE: &str = "gCryTable_Reverse";
/// `sizeof(struct ToneData)`.
const RECORD_BYTES: usize = 12;
/// Offset of `ToneData.wav`.
const RECORD_WAV: usize = 4;
/// The `cry` macro's `ToneData.type`.
const TYPE_NORMAL: u8 = 0x20;
/// The `cry_reverse` macro's `ToneData.type`.
const TYPE_REVERSE: u8 = 0x30;
/// The `ToneData` bytes outside `type` and `wav` a cry record holds, as
/// `(offset, field, value)`.
const FIXED_BYTES: [(usize, &str, u8); 7] = [
    (1, "key", 60),
    (2, "length", 0),
    (3, "pan_sweep", 0),
    (8, "attack", 0xFF),
    (9, "decay", 0),
    (10, "sustain", 0xFF),
    (11, "release", 0),
];
/// `WaveData.type` of a DPCM-compressed sample.
const WAVE_TYPE_DPCM: u16 = 1;
/// Samples in one DPCM block.
const BLOCK_SAMPLES: usize = 64;
/// Bytes in one DPCM block: the first sample, then two indexes per byte.
const BLOCK_BYTES: usize = 33;
/// `gDeltaEncodingTable`.
const DELTAS: [i8; 16] = [
    0, 1, 4, 9, 16, 25, 36, 49, -64, -49, -36, -25, -16, -9, -4, -1,
];

/// Read both cry tables and every sample they name.
///
/// Returns the `audio/cry/table` entry, then one entry per sample in table
/// order. Shared by [`super::write`] and the profile generator, so the
/// generator checks the bytes the importer would emit.
///
/// # Errors
///
/// [`ImportError::StructMismatch`] for a root list that is not
/// [`CRY_TABLE_LEN`] distinct samples with 16-byte headers, or a `WaveData`
/// header that disagrees with its root; [`ImportError::Truncated`] and
/// [`ImportError::PointerOutOfRange`] for a table, header, or sample that runs
/// off the image; [`ImportError::CryToneData`] for a record that is not the
/// table's cry form; [`ImportError::UnresolvedPointer`],
/// [`ImportError::CryPointerMismatch`], and [`ImportError::RepeatedCrySample`]
/// for a record whose sample is not the one recorded for its position;
/// [`ImportError::CryWaveType`] for a sample that is not DPCM;
/// [`ImportError::Audio`] if the schema rejects a value.
pub fn decode_cry_payloads(
    reader: &RomReader<'_>,
    cry: &CryRoots,
) -> Result<Vec<PackEntry>, ImportError> {
    check_roots(cry.samples)?;
    check_tables(reader, cry)?;

    let table = CryTable::new(
        cry.samples
            .iter()
            .map(|root| SampleId(root.id.to_owned()))
            .collect(),
    )
    .map_err(|source| ImportError::Audio {
        id: CRY_TABLE_PACK_ID,
        source,
    })?;
    let mut entries = Vec::with_capacity(1 + cry.samples.len());
    entries.push(raw_entry(CRY_TABLE_PACK_ID.to_owned(), table.encode()));
    for root in cry.samples {
        entries.push(sample(reader, root)?);
    }
    Ok(entries)
}

/// Require the sample list to be one distinct, headed sample per position.
fn check_roots(samples: &[SampleRoot]) -> Result<(), ImportError> {
    let mismatch = |field| ImportError::StructMismatch {
        root: CRY_TABLE_PACK_ID,
        field,
    };
    if samples.len() != CRY_TABLE_LEN {
        return Err(mismatch("CryRoots.samples"));
    }
    let mut ids = std::collections::BTreeSet::new();
    for root in samples {
        if root.header_len != WAVE_HEADER_BYTES {
            return Err(mismatch("CryRoots.samples.header_len"));
        }
        if !ids.insert(root.id) {
            return Err(mismatch("CryRoots.samples.id"));
        }
    }
    Ok(())
}

/// Validate both tables against the recorded samples.
///
/// Each table's whole span is read before any record is, so a table that runs
/// off the image is one [`ImportError::Truncated`] whatever its records hold.
fn check_tables(reader: &RomReader<'_>, cry: &CryRoots) -> Result<(), ImportError> {
    let span = CRY_TABLE_LEN * RECORD_BYTES;
    let normal = reader.slice(cry.normal, span)?;
    let reverse = reader.slice(cry.reverse, span)?;
    let mut seen: BTreeMap<u32, usize> = BTreeMap::new();
    let mut normal_ptrs = Vec::with_capacity(CRY_TABLE_LEN);
    for (slot, root) in cry.samples.iter().enumerate() {
        let record = &normal[slot * RECORD_BYTES..][..RECORD_BYTES];
        check_record(NORMAL_TABLE, TYPE_NORMAL, slot, record)?;
        let ptr = reader.ptr(cry.normal.offset() + slot * RECORD_BYTES + RECORD_WAV)?;
        if let Some(&first) = seen.get(&ptr.raw()) {
            return Err(ImportError::RepeatedCrySample { first, slot, ptr });
        }
        seen.insert(ptr.raw(), slot);
        if !cry.samples.iter().any(|candidate| candidate.addr == ptr) {
            return Err(ImportError::UnresolvedPointer {
                root: NORMAL_TABLE,
                slot,
                what: "a cry sample",
                ptr,
            });
        }
        if ptr != root.addr {
            return Err(ImportError::CryPointerMismatch {
                table: NORMAL_TABLE,
                slot,
                expected: root.addr,
                actual: ptr,
            });
        }
        normal_ptrs.push(ptr);
    }
    for (slot, &expected) in normal_ptrs.iter().enumerate() {
        let record = &reverse[slot * RECORD_BYTES..][..RECORD_BYTES];
        check_record(REVERSE_TABLE, TYPE_REVERSE, slot, record)?;
        let actual = reader.ptr(cry.reverse.offset() + slot * RECORD_BYTES + RECORD_WAV)?;
        if actual != expected {
            return Err(ImportError::CryPointerMismatch {
                table: REVERSE_TABLE,
                slot,
                expected,
                actual,
            });
        }
    }
    Ok(())
}

/// Require a record's `type` and every other fixed byte to be a cry's.
fn check_record(
    table: &'static str,
    kind: u8,
    slot: usize,
    record: &[u8],
) -> Result<(), ImportError> {
    let wrong = |field, expected, actual| ImportError::CryToneData {
        table,
        slot,
        field,
        expected,
        actual,
    };
    if record[0] != kind {
        return Err(wrong("type", kind, record[0]));
    }
    for (offset, field, expected) in FIXED_BYTES {
        if record[offset] != expected {
            return Err(wrong(field, expected, record[offset]));
        }
    }
    Ok(())
}

/// Read one cry sample: its `WaveData` header and DPCM payload.
fn sample(reader: &RomReader<'_>, root: &SampleRoot) -> Result<PackEntry, ImportError> {
    let id = root.id;
    let base = root.addr.offset();
    // The whole header is read first so a header that runs off the image is
    // a truncation, whatever its first bytes hold.
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
    let kind = u16_at(WAVE_TYPE);
    if kind != WAVE_TYPE_DPCM {
        return Err(ImportError::CryWaveType { id, kind });
    }
    let mismatch = |field| ImportError::StructMismatch { root: id, field };
    if u16_at(WAVE_STATUS) != 0 {
        return Err(mismatch("WaveData.status"));
    }
    let size = u32_at(WAVE_SIZE);
    if size != root.data_len {
        return Err(mismatch("WaveData.size"));
    }
    let data = decode_dpcm(reader, base + len_usize(WAVE_HEADER_BYTES), len_usize(size))?;
    let sample = DirectSoundSample::new(u32_at(WAVE_FREQ), None, size, data)
        .map_err(|source| ImportError::Audio { id, source })?;
    Ok(raw_entry(
        id.to_owned(),
        Sample::DirectSound(sample).encode(),
    ))
}

/// The bytes a decode of samples `0..=count` reads from the start of the
/// payload: `count` is the last index, the guard.
fn payload_len(count: usize) -> usize {
    let (blocks, within) = (count / BLOCK_SAMPLES, count % BLOCK_SAMPLES);
    // Index 0 is the block's first byte; index `n >= 1` is in byte `n / 2 + 1`.
    let last = if within == 0 { 1 } else { within / 2 + 2 };
    blocks * BLOCK_BYTES + last
}

/// Decode the `count` logical samples at `payload` and the guard after them.
fn decode_dpcm(
    reader: &RomReader<'_>,
    payload: usize,
    count: usize,
) -> Result<Vec<i8>, ImportError> {
    // The stored bytes are checked before the output is sized from the header.
    let bytes = reader.slice_at(payload, payload_len(count))?;
    let mut samples = Vec::with_capacity(count + 1);
    let mut level = 0i8;
    for index in 0..=count {
        let (block, within) = (index / BLOCK_SAMPLES, index % BLOCK_SAMPLES);
        let start = block * BLOCK_BYTES;
        if within == 0 {
            level = i8::from_le_bytes([bytes[start]]);
        } else {
            let byte = bytes[start + within / 2 + 1];
            let nibble = if within % 2 == 1 {
                byte & 0xF
            } else {
                byte >> 4
            };
            level = level.wrapping_add(DELTAS[usize::from(nibble)]);
        }
        samples.push(level);
    }
    Ok(samples)
}

#[cfg(test)]
mod tests;
