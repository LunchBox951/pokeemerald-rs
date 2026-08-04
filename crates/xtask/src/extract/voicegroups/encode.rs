//! Serializes a [`ResolvedVoiceGroup`] to the exact binary shape
//! `crates/assets/src/audio/voicegroup.rs`'s `VoiceGroup::encode`/
//! `VoiceEntry::write` produce -- duplicated rather than shared, since this
//! crate never depends on `crates/assets` (`crate::extract::pack`'s module
//! docs: the two crates stay decoupled the same way the pack container
//! format itself is duplicated on both sides). Any change to that schema's
//! wire shape must be mirrored here by hand.
//!
//! Wire shape (one group): `slot_count: u8`, then `slot_count` slots, each
//! tagged by a `kind: u8` byte:
//!
//! | kind | tag | payload |
//! |---|---|---|
//! | `DirectSound` | 0 | `base_key: u8`, `pan_or_zero: u8`, `sample_id: string`, `envelope: [u8; 4]`, `mode: u8` |
//! | `Square1` | 1 | `base_key: u8`, `length: u8`, `sweep: u8`, `duty: u8`, `envelope: [u8; 4]`, `fixed_rate: bool` |
//! | `Square2` | 2 | `base_key: u8`, `length: u8`, `duty: u8`, `envelope: [u8; 4]`, `fixed_rate: bool` |
//! | `ProgrammableWave` | 3 | `base_key: u8`, `length: u8`, `wave_id: string`, `envelope: [u8; 4]`, `fixed_rate: bool` |
//! | `Noise` | 4 | `base_key: u8`, `length: u8`, `period: u8`, `envelope: [u8; 4]`, `fixed_rate: bool` |
//! | `KeySplit` | 5 | `starting_note: u8`, `table_len: u8`, `table: [u8; table_len]`, `children_id: string` |
//! | `Rhythm` | 6 | `children_id: string` |
//! | `Empty` | 7 | (none) |
//!
//! `string` is a `u16` little-endian length prefix followed by that many
//! UTF-8 bytes (`crates/assets/src/audio/cursor.rs`'s `Writer::string`);
//! `bool` is a single `0`/`1` byte; `pan_or_zero` is the raw `Option<u8>`
//! value with `None` written as `0` (the wire's own "no override" sentinel
//! -- `VoiceGroup::new` rejects `Some(0)` upstream, and this pipeline's
//! parser never produces it, since a `0` pan operand is read as `None` at
//! parse time -- see `super::parser`'s `op_pan`).

use super::parser::DirectSoundMode;
use super::parser::Envelope;
use super::resolve::{ResolvedVoiceGroup, VoiceSlot};

const KIND_DIRECT_SOUND: u8 = 0;
const KIND_SQUARE_1: u8 = 1;
const KIND_SQUARE_2: u8 = 2;
const KIND_PROGRAMMABLE_WAVE: u8 = 3;
const KIND_NOISE: u8 = 4;
const KIND_KEY_SPLIT: u8 = 5;
const KIND_RHYTHM: u8 = 6;
const KIND_EMPTY: u8 = 7;

const MODE_RESAMPLED: u8 = 0;
const MODE_FIXED: u8 = 1;
const MODE_REVERSE: u8 = 2;

fn write_string(out: &mut Vec<u8>, value: &str) {
    let len = u16::try_from(value.len())
        .expect("every pack id this pipeline generates is far shorter than u16::MAX");
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(value.as_bytes());
}

fn write_envelope(out: &mut Vec<u8>, envelope: Envelope) {
    out.push(envelope.attack);
    out.push(envelope.decay);
    out.push(envelope.sustain);
    out.push(envelope.release);
}

/// Encode one resolved group to the wire shape documented above.
///
/// # Panics
///
/// Unreachable in practice: [`super::resolve::resolve_voice_groups`] always
/// produces exactly [`super::VOICE_SLOT_COUNT`] (128, fits a `u8`) slots per
/// group, and every key-split table it attaches is bounded to the same
/// 128-entry maximum ([`super::parser::VoiceGroupError::KeySplitTableTooLong`]
/// is returned well before an over-long table would reach here).
#[must_use]
pub(super) fn encode_voice_group(group: &ResolvedVoiceGroup) -> Vec<u8> {
    let mut out = Vec::new();
    let count = u8::try_from(group.slots.len())
        .expect("resolve::pad_to_128 always produces exactly VOICE_SLOT_COUNT (128) slots");
    out.push(count);
    for slot in &group.slots {
        encode_slot(&mut out, slot);
    }
    out
}

fn encode_slot(out: &mut Vec<u8>, slot: &VoiceSlot) {
    match slot {
        VoiceSlot::DirectSound {
            base_key,
            pan,
            sample_id,
            envelope,
            mode,
        } => {
            out.push(KIND_DIRECT_SOUND);
            out.push(*base_key);
            out.push(pan.unwrap_or(0));
            write_string(out, sample_id);
            write_envelope(out, *envelope);
            out.push(match mode {
                DirectSoundMode::Resampled => MODE_RESAMPLED,
                DirectSoundMode::Fixed => MODE_FIXED,
                DirectSoundMode::Reverse => MODE_REVERSE,
            });
        }
        VoiceSlot::Square1 {
            base_key,
            length,
            sweep,
            duty,
            envelope,
            fixed_rate,
        } => {
            out.push(KIND_SQUARE_1);
            out.push(*base_key);
            out.push(*length);
            out.push(*sweep);
            out.push(*duty);
            write_envelope(out, *envelope);
            out.push(u8::from(*fixed_rate));
        }
        VoiceSlot::Square2 {
            base_key,
            length,
            duty,
            envelope,
            fixed_rate,
        } => {
            out.push(KIND_SQUARE_2);
            out.push(*base_key);
            out.push(*length);
            out.push(*duty);
            write_envelope(out, *envelope);
            out.push(u8::from(*fixed_rate));
        }
        VoiceSlot::ProgrammableWave {
            base_key,
            length,
            wave_id,
            envelope,
            fixed_rate,
        } => {
            out.push(KIND_PROGRAMMABLE_WAVE);
            out.push(*base_key);
            out.push(*length);
            write_string(out, wave_id);
            write_envelope(out, *envelope);
            out.push(u8::from(*fixed_rate));
        }
        VoiceSlot::Noise {
            base_key,
            length,
            period,
            envelope,
            fixed_rate,
        } => {
            out.push(KIND_NOISE);
            out.push(*base_key);
            out.push(*length);
            out.push(*period);
            write_envelope(out, *envelope);
            out.push(u8::from(*fixed_rate));
        }
        VoiceSlot::KeySplit {
            starting_note,
            table,
            children_id,
        } => {
            out.push(KIND_KEY_SPLIT);
            out.push(*starting_note);
            let len = u8::try_from(table.len()).expect(
                "resolve_voice_groups rejects a key-split table longer than VOICE_SLOT_COUNT \
                 (128) before it ever reaches here",
            );
            out.push(len);
            out.extend_from_slice(table);
            write_string(out, children_id);
        }
        VoiceSlot::Rhythm { children_id } => {
            out.push(KIND_RHYTHM);
            write_string(out, children_id);
        }
        VoiceSlot::Empty => out.push(KIND_EMPTY),
    }
}

#[cfg(test)]
mod tests;
