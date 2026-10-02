use super::super::*;
use super::shared::{all_voice_kinds, sample_cgb_envelope, sample_envelope};

const VOICE_GROUP_SLOT_COUNT_BYTE: usize = 0;
const FIRST_SLOT_KIND_BYTE: usize = 1;

#[test]
fn all_voice_entry_kinds_round_trip() {
    let group = VoiceGroup::new(all_voice_kinds()).unwrap();
    let bytes = group.encode();
    assert_eq!(VoiceGroup::decode(&bytes).unwrap(), group);
}

#[test]
fn cgb_sound_length_round_trips_the_entire_byte_range() {
    for length in u8::MIN..=u8::MAX {
        let group = VoiceGroup::new(vec![VoiceEntry::Square1(Square1Voice {
            base_key: 60,
            length,
            sweep: 0,
            duty: 0,
            envelope: sample_cgb_envelope(),
            fixed_rate: false,
        })])
        .unwrap();
        match &VoiceGroup::decode(&group.encode()).unwrap().slots()[0] {
            VoiceEntry::Square1(v) => assert_eq!(v.length, length),
            other => panic!("expected a Square1 slot, got {other:?}"),
        }
    }
}

#[test]
fn an_oversize_pack_id_is_rejected_by_the_constructor() {
    let too_long = "x".repeat(usize::from(u16::MAX) + 1);
    let expected = Err(AudioError::IdTooLong(usize::from(u16::MAX) + 1));

    let direct_sound = VoiceEntry::DirectSound(DirectSoundVoice {
        base_key: 60,
        pan: None,
        sample: SampleId(too_long.clone()),
        envelope: sample_envelope(),
        mode: DirectSoundMode::Resampled,
    });
    assert_eq!(VoiceGroup::new(vec![direct_sound]), expected);

    let wave = VoiceEntry::ProgrammableWave(ProgrammableWaveVoice {
        base_key: 60,
        length: 0,
        wave: SampleId(too_long.clone()),
        envelope: sample_cgb_envelope(),
        fixed_rate: false,
    });
    assert_eq!(VoiceGroup::new(vec![wave]), expected);

    let key_split = VoiceEntry::KeySplit(
        KeySplitVoice::new(0, vec![], VoiceGroupId(too_long.clone())).unwrap(),
    );
    assert_eq!(VoiceGroup::new(vec![key_split]), expected);

    let rhythm = VoiceEntry::Rhythm(RhythmVoice {
        children: VoiceGroupId(too_long),
    });
    assert_eq!(VoiceGroup::new(vec![rhythm]), expected);
}

#[test]
fn maximum_voicegroup_size_round_trips() {
    let highest_slot = VoiceEntry::Rhythm(RhythmVoice {
        children: VoiceGroupId("audio/voicegroup/highest_slot".to_owned()),
    });
    let mut slots: Vec<VoiceEntry> = (0..VOICE_SLOT_COUNT - 1)
        .map(|i| {
            VoiceEntry::Square1(Square1Voice {
                base_key: u8::try_from(i).expect("a slot index below VOICE_SLOT_COUNT fits a u8"),
                length: 0,
                sweep: 0,
                duty: u8::try_from(i % 4).expect("i % 4 < 4"),
                envelope: sample_cgb_envelope(),
                fixed_rate: false,
            })
        })
        .collect();
    slots.push(highest_slot.clone());
    for (i, slot) in slots.iter().enumerate() {
        assert!(
            !slots[..i].contains(slot),
            "slot {i} repeats an earlier payload, so the round trip would not \
             notice one slot's bytes landing in another slot's place"
        );
    }
    let group = VoiceGroup::new(slots).unwrap();
    assert_eq!(group.slots().len(), VOICE_SLOT_COUNT);
    let bytes = group.encode();
    let decoded = VoiceGroup::decode(&bytes).unwrap();
    assert_eq!(decoded, group);
    assert_eq!(decoded.slot(VOICE_SLOT_COUNT - 1), Some(&highest_slot));
    assert!(decoded.slot(VOICE_SLOT_COUNT).is_none());
}

#[test]
fn too_many_slots_is_rejected_by_the_constructor() {
    let slots: Vec<VoiceEntry> = (0..=VOICE_SLOT_COUNT)
        .map(|_| {
            VoiceEntry::Rhythm(RhythmVoice {
                children: VoiceGroupId("audio/voicegroup/dummy".to_owned()),
            })
        })
        .collect();
    assert_eq!(
        VoiceGroup::new(slots),
        Err(AudioError::TooManyVoiceSlots(VOICE_SLOT_COUNT + 1))
    );
}

#[test]
fn decode_rejects_a_declared_slot_count_above_the_maximum() {
    let mut bytes = VoiceGroup::new(vec![]).unwrap().encode();
    let invalid_slot_count = u8::try_from(VOICE_SLOT_COUNT + 1).unwrap();
    bytes[VOICE_GROUP_SLOT_COUNT_BYTE] = invalid_slot_count;
    assert_eq!(
        VoiceGroup::decode(&bytes),
        Err(AudioError::TooManyVoiceSlots(usize::from(
            invalid_slot_count
        )))
    );
}

#[test]
fn empty_voicegroup_round_trips() {
    let group = VoiceGroup::new(vec![]).unwrap();
    let bytes = group.encode();
    assert_eq!(VoiceGroup::decode(&bytes).unwrap(), group);
}

#[test]
fn truncated_input_is_rejected() {
    let bytes = VoiceGroup::new(all_voice_kinds()).unwrap().encode();
    for cut in 0..bytes.len() {
        assert!(
            VoiceGroup::decode(&bytes[..cut]).is_err(),
            "a {cut}-byte prefix decoded successfully"
        );
    }
}

#[test]
fn an_unknown_slot_kind_byte_is_rejected() {
    let mut bytes = VoiceGroup::new(all_voice_kinds()).unwrap().encode();
    bytes[FIRST_SLOT_KIND_BYTE] = u8::MAX;
    assert_eq!(
        VoiceGroup::decode(&bytes),
        Err(AudioError::UnknownVoiceKind(u8::MAX))
    );
}

#[test]
fn a_kind_byte_just_past_the_last_defined_one_is_rejected() {
    let mut bytes = VoiceGroup::new(all_voice_kinds()).unwrap().encode();
    let unknown_kind = VoiceKind::Empty.tag() + 1;
    bytes[FIRST_SLOT_KIND_BYTE] = unknown_kind;
    assert_eq!(
        VoiceGroup::decode(&bytes),
        Err(AudioError::UnknownVoiceKind(unknown_kind))
    );
}

#[test]
fn an_unknown_direct_sound_mode_byte_is_rejected() {
    let group = VoiceGroup::new(vec![VoiceEntry::DirectSound(DirectSoundVoice {
        base_key: 60,
        pan: None,
        sample: SampleId("s".to_owned()),
        envelope: sample_envelope(),
        mode: DirectSoundMode::Resampled,
    })])
    .unwrap();
    let mut bytes = group.encode();
    let unknown_mode = DirectSoundModeTag::Reverse.tag() + 1;
    *bytes.last_mut().unwrap() = unknown_mode;
    assert_eq!(
        VoiceGroup::decode(&bytes),
        Err(AudioError::UnknownDirectSoundMode(unknown_mode))
    );
}

#[test]
fn a_non_utf8_pack_id_is_rejected() {
    let group = VoiceGroup::new(vec![VoiceEntry::Rhythm(RhythmVoice {
        children: VoiceGroupId("audio/voicegroup/emerald_drumset_1".to_owned()),
    })])
    .unwrap();
    let mut bytes = group.encode();
    *bytes.last_mut().unwrap() = u8::MAX;
    assert_eq!(VoiceGroup::decode(&bytes), Err(AudioError::InvalidString));
}

#[test]
fn an_empty_mid_table_slot_keeps_its_position() {
    let drum = |sample: &str| {
        VoiceEntry::DirectSound(DirectSoundVoice {
            base_key: 60,
            pan: None,
            sample: SampleId(sample.to_owned()),
            envelope: sample_envelope(),
            mode: DirectSoundMode::Fixed,
        })
    };
    let group = VoiceGroup::new(vec![
        drum("audio/sample/bass_drum"),
        VoiceEntry::Empty,
        drum("audio/sample/snare"),
    ])
    .unwrap();
    let decoded = VoiceGroup::decode(&group.encode()).unwrap();
    assert_eq!(decoded, group);
    assert_eq!(
        decoded.slots()[1],
        VoiceEntry::Empty,
        "the unused slot must stay at index 1 -- its index is the played key"
    );
    match &decoded.slots()[2] {
        VoiceEntry::DirectSound(v) => assert_eq!(v.sample.0, "audio/sample/snare"),
        other => panic!("the slot after the empty one shifted: {other:?}"),
    }
}

#[test]
fn decode_rejects_trailing_bytes() {
    let group = VoiceGroup::new(vec![VoiceEntry::DirectSound(DirectSoundVoice {
        base_key: 60,
        pan: Some(64),
        sample: SampleId("audio/sample/x".to_owned()),
        envelope: sample_envelope(),
        mode: DirectSoundMode::Resampled,
    })])
    .unwrap();
    let mut bytes = group.encode();
    bytes.push(0);
    assert_eq!(
        VoiceGroup::decode(&bytes).unwrap_err(),
        AudioError::TrailingBytes(1)
    );
}
