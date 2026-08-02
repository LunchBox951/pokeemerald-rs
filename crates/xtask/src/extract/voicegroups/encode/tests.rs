use super::*;
use crate::extract::voicegroups::resolve::ResolvedVoiceGroup;

fn envelope(attack: u8, decay: u8, sustain: u8, release: u8) -> Envelope {
    Envelope {
        attack,
        decay,
        sustain,
        release,
    }
}

/// Byte-for-byte pin against `crates/assets/src/audio/voicegroup.rs`'s
/// `VoiceGroup::encode`/`VoiceEntry::write` wire shape (this crate cannot
/// depend on that crate to decode it back -- see the module docs -- so the
/// expected bytes are hand-derived here instead).
#[test]
fn a_direct_sound_slot_with_a_pan_override_matches_the_documented_wire_shape() {
    let group = ResolvedVoiceGroup {
        label: "demo".to_owned(),
        slots: vec![VoiceSlot::DirectSound {
            base_key: 60,
            pan: Some(100),
            sample_id: "audio/sample/x".to_owned(),
            envelope: envelope(255, 0, 255, 0),
            mode: DirectSoundMode::Resampled,
        }],
    };
    let bytes = encode_voice_group(&group);

    let mut expected = vec![1u8]; // slot_count
    expected.push(KIND_DIRECT_SOUND);
    expected.push(60); // base_key
    expected.push(100); // pan override, written verbatim (no OR-0x80 -- see module docs)
    expected.extend_from_slice(&14u16.to_le_bytes()); // "audio/sample/x".len()
    expected.extend_from_slice(b"audio/sample/x");
    expected.extend_from_slice(&[255, 0, 255, 0]); // envelope
    expected.push(MODE_RESAMPLED);

    assert_eq!(bytes, expected);
}

#[test]
fn a_direct_sound_slot_with_no_pan_override_writes_a_zero_byte() {
    let group = ResolvedVoiceGroup {
        label: "demo".to_owned(),
        slots: vec![VoiceSlot::DirectSound {
            base_key: 60,
            pan: None,
            sample_id: "s".to_owned(),
            envelope: envelope(0, 0, 0, 0),
            mode: DirectSoundMode::Fixed,
        }],
    };
    let bytes = encode_voice_group(&group);
    // [0] count, [1] kind, [2] base_key, [3] pan byte.
    assert_eq!(bytes[3], 0);
    assert_eq!(*bytes.last().unwrap(), MODE_FIXED);
}

#[test]
fn an_empty_slot_is_a_single_tag_byte() {
    let group = ResolvedVoiceGroup {
        label: "demo".to_owned(),
        slots: vec![VoiceSlot::Empty, VoiceSlot::Empty],
    };
    let bytes = encode_voice_group(&group);
    assert_eq!(bytes, vec![2, KIND_EMPTY, KIND_EMPTY]);
}

#[test]
fn a_rhythm_slot_writes_its_tag_then_the_children_id_string() {
    let group = ResolvedVoiceGroup {
        label: "demo".to_owned(),
        slots: vec![VoiceSlot::Rhythm {
            children_id: "audio/voicegroup/rs_drumset".to_owned(),
        }],
    };
    let bytes = encode_voice_group(&group);
    let mut expected = vec![1u8, KIND_RHYTHM];
    expected.extend_from_slice(&27u16.to_le_bytes()); // "audio/voicegroup/rs_drumset".len()
    expected.extend_from_slice(b"audio/voicegroup/rs_drumset");
    assert_eq!(bytes, expected);
}

#[test]
fn a_key_split_slot_writes_starting_note_table_len_table_then_children_id() {
    let group = ResolvedVoiceGroup {
        label: "demo".to_owned(),
        slots: vec![VoiceSlot::KeySplit {
            starting_note: 36,
            table: vec![0, 0, 1, 1, 2],
            children_id: "audio/voicegroup/piano_keysplit".to_owned(),
        }],
    };
    let bytes = encode_voice_group(&group);
    let mut expected = vec![1u8, KIND_KEY_SPLIT, 36, 5, 0, 0, 1, 1, 2];
    expected.extend_from_slice(&31u16.to_le_bytes());
    expected.extend_from_slice(b"audio/voicegroup/piano_keysplit");
    assert_eq!(bytes, expected);
}

#[test]
fn square_and_wave_and_noise_slots_carry_fixed_rate_as_a_single_bool_byte() {
    let group = ResolvedVoiceGroup {
        label: "demo".to_owned(),
        slots: vec![
            VoiceSlot::Square1 {
                base_key: 60,
                length: 0,
                sweep: 0,
                duty: 2,
                envelope: envelope(0, 0, 15, 0),
                fixed_rate: true,
            },
            VoiceSlot::Square2 {
                base_key: 60,
                length: 0,
                duty: 3,
                envelope: envelope(0, 0, 15, 0),
                fixed_rate: false,
            },
            VoiceSlot::ProgrammableWave {
                base_key: 60,
                length: 0,
                wave_id: "audio/sample/programmable_wave_1".to_owned(),
                envelope: envelope(0, 7, 15, 1),
                fixed_rate: true,
            },
            VoiceSlot::Noise {
                base_key: 60,
                length: 0,
                period: 1,
                envelope: envelope(0, 0, 15, 0),
                fixed_rate: false,
            },
        ],
    };
    let bytes = encode_voice_group(&group);
    assert_eq!(bytes[0], 4); // slot_count
                             // Square1: [kind, base_key, length, sweep, duty, env x4, fixed_rate] = 9 bytes.
    assert_eq!(&bytes[1..10], &[KIND_SQUARE_1, 60, 0, 0, 2, 0, 0, 15, 0]);
    assert_eq!(bytes[10], 1); // fixed_rate = true
                              // Square2: [kind, base_key, length, duty, env x4, fixed_rate] = 8 bytes.
    assert_eq!(&bytes[11..19], &[KIND_SQUARE_2, 60, 0, 3, 0, 0, 15, 0]);
    assert_eq!(bytes[19], 0); // fixed_rate = false
}
