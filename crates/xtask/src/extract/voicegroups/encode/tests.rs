use super::*;
use crate::extract::voicegroups::resolve::ResolvedVoiceGroup;

// Wire tags spelled out as literals: these tests pin that each slot reaches
// `assets`' encoder with its fields on the right schema positions, so they
// must not borrow the encoder's own constants.
const TAG_DIRECT_SOUND: u8 = 0;
const TAG_SQUARE1: u8 = 1;
const TAG_SQUARE2: u8 = 2;
const TAG_PROGRAMMABLE_WAVE: u8 = 3;
const TAG_NOISE: u8 = 4;
const TAG_KEY_SPLIT: u8 = 5;
const TAG_RHYTHM: u8 = 6;
const TAG_EMPTY: u8 = 7;

const fn mode_byte(mode: DirectSoundMode) -> u8 {
    match mode {
        DirectSoundMode::Resampled => 0,
        DirectSoundMode::Fixed => 1,
        DirectSoundMode::Reverse => 2,
    }
}

fn envelope(attack: u8, decay: u8, sustain: u8, release: u8) -> Envelope {
    Envelope {
        attack,
        decay,
        sustain,
        release,
    }
}

fn assert_single_slot_encoding(slot: VoiceSlot, mut expected_slot: Vec<u8>) {
    let group = ResolvedVoiceGroup {
        label: "demo".to_owned(),
        slots: vec![slot],
    };
    let mut expected_group = vec![1];
    expected_group.append(&mut expected_slot);
    assert_eq!(encode_voice_group(&group).unwrap(), expected_group);
}

fn push_id(out: &mut Vec<u8>, id: &str) {
    let byte_len = u16::try_from(id.len()).unwrap();
    out.extend_from_slice(&byte_len.to_le_bytes());
    out.extend_from_slice(id.as_bytes());
}

#[test]
fn direct_sound_encodes_every_field_in_schema_order() {
    let base_key = 60;
    let pan = 100;
    let sample_id = "audio/sample/direct-sound/x";
    let envelope = envelope(255, 0, 255, 0);
    let mode = DirectSoundMode::Resampled;
    let slot = VoiceSlot::DirectSound {
        base_key,
        pan: Some(pan),
        sample_id: sample_id.to_owned(),
        envelope,
        mode,
    };

    let mut expected = vec![TAG_DIRECT_SOUND, base_key, pan];
    push_id(&mut expected, sample_id);
    expected.extend_from_slice(&[
        envelope.attack,
        envelope.decay,
        envelope.sustain,
        envelope.release,
    ]);
    expected.push(mode_byte(mode));

    assert_single_slot_encoding(slot, expected);
}

#[test]
fn direct_sound_without_a_pan_override_encodes_the_zero_sentinel() {
    let base_key = 60;
    let sample_id = "s";
    let envelope = envelope(0, 0, 0, 0);
    let mode = DirectSoundMode::Fixed;
    let slot = VoiceSlot::DirectSound {
        base_key,
        pan: None,
        sample_id: sample_id.to_owned(),
        envelope,
        mode,
    };

    let mut expected = vec![TAG_DIRECT_SOUND, base_key, 0];
    push_id(&mut expected, sample_id);
    expected.extend_from_slice(&[
        envelope.attack,
        envelope.decay,
        envelope.sustain,
        envelope.release,
    ]);
    expected.push(mode_byte(mode));

    assert_single_slot_encoding(slot, expected);
}

#[test]
fn square_one_encodes_every_field_in_schema_order() {
    let base_key = 60;
    let length = 0;
    let sweep = 0;
    let duty = 2;
    let envelope = envelope(0, 0, 15, 0);
    let fixed_rate = true;
    let slot = VoiceSlot::Square1 {
        base_key,
        length,
        sweep,
        duty,
        envelope,
        fixed_rate,
    };
    let expected = vec![
        TAG_SQUARE1,
        base_key,
        length,
        sweep,
        duty,
        envelope.attack,
        envelope.decay,
        envelope.sustain,
        envelope.release,
        u8::from(fixed_rate),
    ];

    assert_single_slot_encoding(slot, expected);
}

#[test]
fn square_two_encodes_every_field_in_schema_order() {
    let base_key = 60;
    let length = 0;
    let duty = 3;
    let envelope = envelope(0, 0, 15, 0);
    let fixed_rate = false;
    let slot = VoiceSlot::Square2 {
        base_key,
        length,
        duty,
        envelope,
        fixed_rate,
    };
    let expected = vec![
        TAG_SQUARE2,
        base_key,
        length,
        duty,
        envelope.attack,
        envelope.decay,
        envelope.sustain,
        envelope.release,
        u8::from(fixed_rate),
    ];

    assert_single_slot_encoding(slot, expected);
}

#[test]
fn programmable_wave_encodes_every_field_in_schema_order() {
    let base_key = 60;
    let length = 0;
    let wave_id = "audio/sample/programmable-wave/01";
    let envelope = envelope(0, 7, 15, 1);
    let fixed_rate = true;
    let slot = VoiceSlot::ProgrammableWave {
        base_key,
        length,
        wave_id: wave_id.to_owned(),
        envelope,
        fixed_rate,
    };

    let mut expected = vec![TAG_PROGRAMMABLE_WAVE, base_key, length];
    push_id(&mut expected, wave_id);
    expected.extend_from_slice(&[
        envelope.attack,
        envelope.decay,
        envelope.sustain,
        envelope.release,
        u8::from(fixed_rate),
    ]);

    assert_single_slot_encoding(slot, expected);
}

#[test]
fn noise_encodes_every_field_in_schema_order() {
    let base_key = 60;
    let length = 0;
    let period = 1;
    let envelope = envelope(0, 0, 15, 0);
    let fixed_rate = false;
    let slot = VoiceSlot::Noise {
        base_key,
        length,
        period,
        envelope,
        fixed_rate,
    };
    let expected = vec![
        TAG_NOISE,
        base_key,
        length,
        period,
        envelope.attack,
        envelope.decay,
        envelope.sustain,
        envelope.release,
        u8::from(fixed_rate),
    ];

    assert_single_slot_encoding(slot, expected);
}

#[test]
fn key_split_encodes_every_field_in_schema_order() {
    let starting_note = 36;
    let table = vec![0, 0, 1, 1, 2];
    let children_id = "audio/voicegroup/piano_keysplit";
    let slot = VoiceSlot::KeySplit {
        starting_note,
        table: table.clone(),
        children_id: children_id.to_owned(),
    };

    let table_len = u8::try_from(table.len()).unwrap();
    let mut expected = vec![TAG_KEY_SPLIT, starting_note, table_len];
    expected.extend_from_slice(&table);
    push_id(&mut expected, children_id);

    assert_single_slot_encoding(slot, expected);
}

#[test]
fn rhythm_encodes_its_tag_and_children_id() {
    let children_id = "audio/voicegroup/rs_drumset";
    let slot = VoiceSlot::Rhythm {
        children_id: children_id.to_owned(),
    };
    let mut expected = vec![TAG_RHYTHM];
    push_id(&mut expected, children_id);

    assert_single_slot_encoding(slot, expected);
}

#[test]
fn empty_encodes_only_its_tag() {
    assert_single_slot_encoding(VoiceSlot::Empty, vec![TAG_EMPTY]);
}

#[test]
fn a_multi_slot_group_writes_the_count_then_each_slot_in_order() {
    let sample_id = "s";
    let group = ResolvedVoiceGroup {
        label: "demo".to_owned(),
        slots: vec![
            VoiceSlot::Empty,
            VoiceSlot::DirectSound {
                base_key: 60,
                pan: None,
                sample_id: sample_id.to_owned(),
                envelope: envelope(0, 0, 0, 0),
                mode: DirectSoundMode::Fixed,
            },
            VoiceSlot::Empty,
        ],
    };

    let mut expected = vec![3, TAG_EMPTY];
    expected.push(TAG_DIRECT_SOUND);
    expected.push(60);
    expected.push(0);
    push_id(&mut expected, sample_id);
    expected.extend_from_slice(&[0, 0, 0, 0]);
    expected.push(mode_byte(DirectSoundMode::Fixed));
    expected.push(TAG_EMPTY);
    assert_eq!(encode_voice_group(&group).unwrap(), expected);
}

/// Source text -> parse -> resolve -> encode parity with the byte
/// `rom-import` reads back into the same field
/// (`crates/rom-import/src/domains/audio.rs:312-315`), across all four CGB
/// families.
#[test]
fn cgb_length_operands_round_trip_the_way_the_assembler_encodes_them() {
    use crate::extract::voicegroups::parser::{parse_voice_group, RawKeySplitTable};
    use crate::extract::voicegroups::resolve::resolve_voice_groups;
    use std::collections::HashMap;

    // [0] slot count, [1] slot tag, [2] base key, [3] length.
    const FIRST_SLOT_LENGTH_BYTE: usize = 3;

    fn encoded_cgb_length(operand: u8) -> u8 {
        0x80 | operand
    }

    let families = [
        "voice_square_1 60, {length}, 0, 0, 0, 0, 0, 0",
        "voice_square_2 60, {length}, 0, 0, 0, 0, 0",
        "voice_programmable_wave 60, {length}, ProgrammableWaveData_1, 0, 0, 0, 0",
        "voice_noise 60, {length}, 0, 0, 0, 0, 0",
    ];

    for family in families {
        for (length_operand, expected_byte) in [(0_u8, 0_u8), (54, encoded_cgb_length(54))] {
            let slot_line = family.replace("{length}", &length_operand.to_string());
            let text = format!("voice_group demo\n\t{slot_line}\n");
            let raw = parse_voice_group(&text).unwrap();
            let raw_groups = HashMap::from([(raw.label.clone(), raw)]);
            let no_keysplit_tables: HashMap<String, RawKeySplitTable> = HashMap::new();
            let resolved = resolve_voice_groups("demo", &raw_groups, &no_keysplit_tables).unwrap();
            assert_eq!(
                encode_voice_group(&resolved[0]).unwrap()[FIRST_SLOT_LENGTH_BYTE],
                expected_byte,
                "{slot_line}"
            );
        }

        let rejected_line = family.replace("{length}", "128");
        let text = format!("voice_group demo\n\t{rejected_line}\n");
        assert!(
            parse_voice_group(&text).is_err(),
            "expected an out-of-domain CGB length operand to be rejected before resolve/encode: \
             {rejected_line}"
        );
    }
}

#[test]
fn every_direct_sound_mode_reaches_its_schema_tag() {
    for mode in [
        DirectSoundMode::Resampled,
        DirectSoundMode::Fixed,
        DirectSoundMode::Reverse,
    ] {
        let slot = VoiceSlot::DirectSound {
            base_key: 60,
            pan: None,
            sample_id: "s".to_owned(),
            envelope: envelope(0, 0, 0, 0),
            mode,
        };
        let mut expected = vec![TAG_DIRECT_SOUND, 60, 0];
        push_id(&mut expected, "s");
        expected.extend_from_slice(&[0, 0, 0, 0, mode_byte(mode)]);
        assert_single_slot_encoding(slot, expected);
    }
}

#[test]
fn an_encoded_group_decodes_through_the_asset_schema() {
    let group = ResolvedVoiceGroup {
        label: "demo".to_owned(),
        slots: vec![
            VoiceSlot::DirectSound {
                base_key: 60,
                pan: Some(100),
                sample_id: "audio/sample/direct-sound/x".to_owned(),
                envelope: envelope(255, 0, 255, 0),
                mode: DirectSoundMode::Reverse,
            },
            VoiceSlot::KeySplit {
                starting_note: 36,
                table: vec![0, 1, 2],
                children_id: "audio/voicegroup/piano_keysplit".to_owned(),
            },
            VoiceSlot::Empty,
        ],
    };

    let decoded = assets::VoiceGroup::decode(&encode_voice_group(&group).unwrap()).unwrap();

    assert_eq!(decoded.slots().len(), 3);
    assert!(matches!(
        &decoded.slots()[0],
        assets::VoiceEntry::DirectSound(v)
            if v.pan == Some(100)
                && v.mode == assets::DirectSoundMode::Reverse
                && v.sample.0 == "audio/sample/direct-sound/x"
    ));
    assert!(matches!(
        &decoded.slots()[1],
        assets::VoiceEntry::KeySplit(v) if v.starting_note == 36 && v.table() == [0, 1, 2]
    ));
    assert_eq!(decoded.slots()[2], assets::VoiceEntry::Empty);
}

#[test]
fn a_slot_outside_the_asset_schema_is_a_typed_error_naming_the_group() {
    let group = ResolvedVoiceGroup {
        label: "demo".to_owned(),
        slots: vec![VoiceSlot::DirectSound {
            base_key: 60,
            pan: Some(0),
            sample_id: "s".to_owned(),
            envelope: envelope(0, 0, 0, 0),
            mode: DirectSoundMode::Fixed,
        }],
    };

    assert_eq!(
        encode_voice_group(&group),
        Err(VoiceGroupError::InvalidAssetSchema {
            group: "demo".to_owned(),
            source: assets::AudioError::PanOverrideZero,
        })
    );
}

#[test]
fn a_key_split_outside_the_asset_schema_is_a_typed_error_naming_the_group() {
    let group = ResolvedVoiceGroup {
        label: "demo".to_owned(),
        slots: vec![VoiceSlot::KeySplit {
            starting_note: 120,
            table: vec![0; 16],
            children_id: "c".to_owned(),
        }],
    };

    assert!(matches!(
        encode_voice_group(&group),
        Err(VoiceGroupError::InvalidAssetSchema { group, .. }) if group == "demo"
    ));
}
