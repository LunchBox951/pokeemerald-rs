use super::*;

const MIDDLE_C: u8 = 60;
const KEY_SPLIT_START: u8 = 36;
const VOICE_GROUP_SLOT_COUNT_BYTE: usize = 0;
const FIRST_SLOT_KIND_BYTE: usize = 1;
const FIRST_KEY_SPLIT_TABLE_LENGTH_BYTE: usize = 3;

fn sample_envelope() -> Envelope {
    Envelope {
        attack: 255,
        decay: 0,
        sustain: 255,
        release: 0,
    }
}

fn all_voice_kinds() -> Vec<VoiceEntry> {
    vec![
        VoiceEntry::DirectSound(DirectSoundVoice {
            base_key: MIDDLE_C,
            pan: Some(100),
            sample: SampleId("audio/sample/sc88pro_flute".to_owned()),
            envelope: sample_envelope(),
            mode: DirectSoundMode::Resampled,
        }),
        VoiceEntry::Square1(Square1Voice {
            base_key: MIDDLE_C,
            length: 0,
            sweep: 0,
            duty: 2,
            envelope: sample_envelope(),
            fixed_rate: false,
        }),
        VoiceEntry::Square2(Square2Voice {
            base_key: MIDDLE_C,
            length: 4,
            duty: 3,
            envelope: sample_envelope(),
            fixed_rate: true,
        }),
        VoiceEntry::ProgrammableWave(ProgrammableWaveVoice {
            base_key: MIDDLE_C,
            length: 0,
            wave: SampleId("audio/sample/programmable_wave_2".to_owned()),
            envelope: sample_envelope(),
            fixed_rate: true,
        }),
        VoiceEntry::Noise(NoiseVoice {
            base_key: MIDDLE_C,
            length: 0,
            period: 1,
            envelope: sample_envelope(),
            fixed_rate: false,
        }),
        VoiceEntry::KeySplit(
            KeySplitVoice::new(
                KEY_SPLIT_START,
                vec![0, 0, 1, 1, 2],
                VoiceGroupId("audio/voicegroup/trumpet_keysplit".to_owned()),
            )
            .unwrap(),
        ),
        VoiceEntry::Rhythm(RhythmVoice {
            children: VoiceGroupId("audio/voicegroup/emerald_drumset_1".to_owned()),
        }),
        VoiceEntry::Empty,
    ]
}

#[test]
fn direct_sound_round_trips_with_and_without_pan_override() {
    let group = VoiceGroup::new(vec![
        VoiceEntry::DirectSound(DirectSoundVoice {
            base_key: 60,
            pan: None,
            sample: SampleId("audio/sample/sc88pro_flute".to_owned()),
            envelope: sample_envelope(),
            mode: DirectSoundMode::Resampled,
        }),
        VoiceEntry::DirectSound(DirectSoundVoice {
            base_key: 64,
            pan: Some(100),
            sample: SampleId("audio/sample/taiko".to_owned()),
            envelope: Envelope {
                attack: 255,
                decay: 180,
                sustain: 175,
                release: 228,
            },
            mode: DirectSoundMode::Fixed,
        }),
    ])
    .unwrap();
    let bytes = group.encode();
    assert_eq!(VoiceGroup::decode(&bytes).unwrap(), group);
}

#[test]
fn direct_sound_reverse_mode_round_trips() {
    let group = VoiceGroup::new(vec![VoiceEntry::DirectSound(DirectSoundVoice {
        base_key: 60,
        pan: None,
        sample: SampleId("audio/sample/bicycle_bell".to_owned()),
        envelope: sample_envelope(),
        mode: DirectSoundMode::Reverse,
    })])
    .unwrap();
    let bytes = group.encode();
    assert_eq!(VoiceGroup::decode(&bytes).unwrap(), group);
}

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
            envelope: sample_envelope(),
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
fn key_split_and_rhythm_indirection_round_trip() {
    let group = VoiceGroup::new(vec![
        VoiceEntry::KeySplit(
            KeySplitVoice::new(
                36,
                vec![0, 0, 1, 1, 2],
                VoiceGroupId("audio/voicegroup/trumpet_keysplit".to_owned()),
            )
            .unwrap(),
        ),
        VoiceEntry::Rhythm(RhythmVoice {
            children: VoiceGroupId("audio/voicegroup/emerald_drumset_1".to_owned()),
        }),
    ])
    .unwrap();
    let decoded = VoiceGroup::decode(&group.encode()).unwrap();
    assert_eq!(decoded, group);
    match &decoded.slots()[0] {
        VoiceEntry::KeySplit(v) => assert_eq!(v.table(), [0, 0, 1, 1, 2]),
        other => panic!("expected a KeySplit slot, got {other:?}"),
    }
}

#[test]
fn the_longest_allowed_key_split_table_round_trips() {
    let table: Vec<u8> = (0..VOICE_SLOT_COUNT)
        .map(|i| u8::try_from(i).expect("i < 128"))
        .collect();
    let group = VoiceGroup::new(vec![VoiceEntry::KeySplit(
        KeySplitVoice::new(
            0,
            table.clone(),
            VoiceGroupId("audio/voicegroup/x".to_owned()),
        )
        .unwrap(),
    )])
    .unwrap();
    match &VoiceGroup::decode(&group.encode()).unwrap().slots()[0] {
        VoiceEntry::KeySplit(v) => assert_eq!(v.table(), table.as_slice()),
        other => panic!("expected a KeySplit slot, got {other:?}"),
    }
}

#[test]
fn an_over_long_key_split_table_is_rejected_by_the_constructor() {
    let table = vec![0u8; VOICE_SLOT_COUNT + 1];
    assert_eq!(
        KeySplitVoice::new(0, table, VoiceGroupId("audio/voicegroup/x".to_owned())),
        Err(AudioError::KeySplitTableTooLong(VOICE_SLOT_COUNT + 1))
    );
}

#[test]
fn decode_rejects_a_declared_key_split_table_length_above_the_maximum() {
    let group = VoiceGroup::new(vec![VoiceEntry::KeySplit(
        KeySplitVoice::new(0, vec![0], VoiceGroupId("audio/voicegroup/x".to_owned())).unwrap(),
    )])
    .unwrap();
    let mut bytes = group.encode();
    let invalid_table_len = u8::try_from(VOICE_SLOT_COUNT + 1).unwrap();
    bytes[FIRST_KEY_SPLIT_TABLE_LENGTH_BYTE] = invalid_table_len;
    assert_eq!(
        VoiceGroup::decode(&bytes),
        Err(AudioError::KeySplitTableTooLong(usize::from(
            invalid_table_len
        )))
    );
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
        envelope: sample_envelope(),
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
    let slots: Vec<VoiceEntry> = (0..VOICE_SLOT_COUNT)
        .map(|_| {
            VoiceEntry::Square1(Square1Voice {
                base_key: 60,
                length: 0,
                sweep: 0,
                duty: 0,
                envelope: sample_envelope(),
                fixed_rate: false,
            })
        })
        .collect();
    let group = VoiceGroup::new(slots).unwrap();
    assert_eq!(group.slots().len(), VOICE_SLOT_COUNT);
    let bytes = group.encode();
    let decoded = VoiceGroup::decode(&bytes).unwrap();
    assert_eq!(decoded, group);
    assert_eq!(decoded.slot(VOICE_SLOT_COUNT - 1), group.slots().last());
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
fn no_pan_override_round_trips_without_becoming_zero() {
    let group = VoiceGroup::new(vec![VoiceEntry::DirectSound(DirectSoundVoice {
        base_key: 60,
        pan: None,
        sample: SampleId("audio/sample/x".to_owned()),
        envelope: sample_envelope(),
        mode: DirectSoundMode::Resampled,
    })])
    .unwrap();
    let decoded = VoiceGroup::decode(&group.encode()).unwrap();
    match &decoded.slots()[0] {
        VoiceEntry::DirectSound(v) => assert_eq!(v.pan, None),
        other => panic!("expected a DirectSound slot, got {other:?}"),
    }
}

#[test]
fn pan_zero_is_rejected_at_construction() {
    let err = VoiceGroup::new(vec![VoiceEntry::DirectSound(DirectSoundVoice {
        base_key: 60,
        pan: Some(0),
        sample: SampleId("audio/sample/x".to_owned()),
        envelope: sample_envelope(),
        mode: DirectSoundMode::Resampled,
    })])
    .unwrap_err();
    assert_eq!(err, AudioError::PanOverrideZero);
    assert!(
        err.to_string().contains("Some(0)"),
        "Display must explain the sentinel collision: {err}"
    );
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
