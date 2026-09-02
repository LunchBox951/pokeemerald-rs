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
fn voice_slot_tags_match_the_asset_schema() {
    assert_eq!(
        [
            VoiceSlotTag::DirectSound.byte(),
            VoiceSlotTag::Square1.byte(),
            VoiceSlotTag::Square2.byte(),
            VoiceSlotTag::ProgrammableWave.byte(),
            VoiceSlotTag::Noise.byte(),
            VoiceSlotTag::KeySplit.byte(),
            VoiceSlotTag::Rhythm.byte(),
            VoiceSlotTag::Empty.byte(),
        ],
        [0, 1, 2, 3, 4, 5, 6, 7]
    );
}

#[test]
fn direct_sound_mode_tags_match_the_asset_schema() {
    assert_eq!(
        [
            DirectSoundModeTag::from(DirectSoundMode::Resampled).byte(),
            DirectSoundModeTag::from(DirectSoundMode::Fixed).byte(),
            DirectSoundModeTag::from(DirectSoundMode::Reverse).byte(),
        ],
        [0, 1, 2]
    );
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

    let mut expected = vec![VoiceSlotTag::DirectSound.byte(), base_key, pan];
    push_id(&mut expected, sample_id);
    expected.extend_from_slice(&[
        envelope.attack,
        envelope.decay,
        envelope.sustain,
        envelope.release,
    ]);
    expected.push(DirectSoundModeTag::from(mode).byte());

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

    let mut expected = vec![VoiceSlotTag::DirectSound.byte(), base_key, 0];
    push_id(&mut expected, sample_id);
    expected.extend_from_slice(&[
        envelope.attack,
        envelope.decay,
        envelope.sustain,
        envelope.release,
    ]);
    expected.push(DirectSoundModeTag::from(mode).byte());

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
        VoiceSlotTag::Square1.byte(),
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
        VoiceSlotTag::Square2.byte(),
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

    let mut expected = vec![VoiceSlotTag::ProgrammableWave.byte(), base_key, length];
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
        VoiceSlotTag::Noise.byte(),
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
    let mut expected = vec![VoiceSlotTag::KeySplit.byte(), starting_note, table_len];
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
    let mut expected = vec![VoiceSlotTag::Rhythm.byte()];
    push_id(&mut expected, children_id);

    assert_single_slot_encoding(slot, expected);
}

#[test]
fn empty_encodes_only_its_tag() {
    assert_single_slot_encoding(VoiceSlot::Empty, vec![VoiceSlotTag::Empty.byte()]);
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

    let mut expected = vec![3, VoiceSlotTag::Empty.byte()];
    expected.push(VoiceSlotTag::DirectSound.byte());
    expected.push(60);
    expected.push(0);
    push_id(&mut expected, sample_id);
    expected.extend_from_slice(&[0, 0, 0, 0]);
    expected.push(DirectSoundModeTag::from(DirectSoundMode::Fixed).byte());
    expected.push(VoiceSlotTag::Empty.byte());
    assert_eq!(encode_voice_group(&group).unwrap(), expected);
}

#[test]
fn an_oversize_id_returns_a_typed_extraction_error_instead_of_panicking() {
    let too_long_sample_id = "x".repeat(usize::from(u16::MAX) + 1);
    let group = ResolvedVoiceGroup {
        label: "demo".to_owned(),
        slots: vec![VoiceSlot::DirectSound {
            base_key: 60,
            pan: None,
            sample_id: too_long_sample_id.clone(),
            envelope: envelope(0, 0, 0, 0),
            mode: DirectSoundMode::Resampled,
        }],
    };

    let err = encode_voice_group(&group).unwrap_err();
    assert!(matches!(
        err,
        ExtractError::VoiceGroupIdTooLong { group, actual }
            if group == "demo" && actual == too_long_sample_id.len()
    ));
}
