//! Encodes resolved voicegroups into the asset pack's voicegroup schema.
//!
//! `xtask` and `assets` intentionally remain decoupled, so schema changes must
//! be mirrored between their encoders. Each id is UTF-8 prefixed by its `u16`
//! little-endian byte length. A zero `DirectSound` pan byte means no override;
//! the parser never produces an explicit zero override.

use super::parser::{DirectSoundMode, Envelope};
use super::resolve::{ResolvedVoiceGroup, VoiceSlot};

#[derive(Clone, Copy)]
#[repr(u8)]
enum VoiceSlotTag {
    DirectSound = 0,
    Square1 = 1,
    Square2 = 2,
    ProgrammableWave = 3,
    Noise = 4,
    KeySplit = 5,
    Rhythm = 6,
    Empty = 7,
}

impl VoiceSlotTag {
    const fn byte(self) -> u8 {
        self as u8
    }
}

#[derive(Clone, Copy)]
#[repr(u8)]
enum DirectSoundModeTag {
    Resampled = 0,
    Fixed = 1,
    Reverse = 2,
}

impl DirectSoundModeTag {
    const fn byte(self) -> u8 {
        self as u8
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

fn write_id(out: &mut Vec<u8>, id: &str) {
    // resolve::checked_pack_id rejects an over-long id against this same u16
    // prefix while its source group is known; unreachable for resolved slots.
    let byte_len =
        u16::try_from(id.len()).expect("resolver-checked pack id fits the u16 length prefix");
    out.extend_from_slice(&byte_len.to_le_bytes());
    out.extend_from_slice(id.as_bytes());
}

fn write_envelope(out: &mut Vec<u8>, envelope: Envelope) {
    out.extend_from_slice(&[
        envelope.attack,
        envelope.decay,
        envelope.sustain,
        envelope.release,
    ]);
}

#[must_use]
pub(super) fn encode_voice_group(group: &ResolvedVoiceGroup) -> Vec<u8> {
    let mut out = Vec::new();
    // Resolver::resolve_group (via resolve::pad_to_128) normalizes every
    // emitted group to exactly VOICE_SLOT_COUNT (128) slots, well under
    // u8::MAX; unreachable barring a change to VOICE_SLOT_COUNT itself.
    let slot_count = u8::try_from(group.slots.len())
        .expect("resolver-normalized VOICE_SLOT_COUNT is representable as u8");
    out.push(slot_count);
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
            out.push(VoiceSlotTag::DirectSound.byte());
            out.push(*base_key);
            out.push(pan.unwrap_or(0));
            write_id(out, sample_id);
            write_envelope(out, *envelope);
            out.push(DirectSoundModeTag::from(*mode).byte());
        }
        VoiceSlot::Square1 {
            base_key,
            length,
            sweep,
            duty,
            envelope,
            fixed_rate,
        } => {
            out.push(VoiceSlotTag::Square1.byte());
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
            out.push(VoiceSlotTag::Square2.byte());
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
            out.push(VoiceSlotTag::ProgrammableWave.byte());
            out.push(*base_key);
            out.push(*length);
            write_id(out, wave_id);
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
            out.push(VoiceSlotTag::Noise.byte());
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
            out.push(VoiceSlotTag::KeySplit.byte());
            out.push(*starting_note);
            // parser::finish_keysplit_block rejects any expanded key-split
            // table longer than VOICE_SLOT_COUNT (128) before it is ever
            // stored, well under u8::MAX; unreachable barring a change to
            // VOICE_SLOT_COUNT itself.
            let table_len = u8::try_from(table.len())
                .expect("parser-bounded key-split table length is representable as u8");
            out.push(table_len);
            out.extend_from_slice(table);
            write_id(out, children_id);
        }
        VoiceSlot::Rhythm { children_id } => {
            out.push(VoiceSlotTag::Rhythm.byte());
            write_id(out, children_id);
        }
        VoiceSlot::Empty => out.push(VoiceSlotTag::Empty.byte()),
    }
}

#[cfg(test)]
mod tests;
