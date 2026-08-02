//! [`VoiceGroup`]/[`VoiceEntry`] round-trip and validation tests, split out
//! of `voicegroup.rs` itself to keep that file under this crate's
//! ~600-line-per-file guideline (`oop-boundaries`) — mirrors
//! `crate::pack`'s own `mod tests;` split.

use super::*;

fn sample_envelope() -> Envelope {
    Envelope {
        attack: 255,
        decay: 0,
        sustain: 255,
        release: 0,
    }
}

/// One slot of every kind, so a test that walks or corrupts an encoding
/// covers every branch of [`VoiceEntry`]'s write/read pair.
fn one_of_every_kind() -> Vec<VoiceEntry> {
    vec![
        VoiceEntry::DirectSound(DirectSoundVoice {
            base_key: 60,
            pan: Some(100),
            sample: SampleId("audio/sample/sc88pro_flute".to_owned()),
            envelope: sample_envelope(),
            mode: DirectSoundMode::Resampled,
        }),
        VoiceEntry::Square1(Square1Voice {
            base_key: 60,
            length: 0,
            sweep: 0,
            duty: 2,
            envelope: sample_envelope(),
            fixed_rate: false,
        }),
        VoiceEntry::Square2(Square2Voice {
            base_key: 60,
            length: 4,
            duty: 3,
            envelope: sample_envelope(),
            fixed_rate: true,
        }),
        VoiceEntry::ProgrammableWave(ProgrammableWaveVoice {
            base_key: 60,
            length: 0,
            wave: SampleId("audio/sample/programmable_wave_2".to_owned()),
            envelope: sample_envelope(),
            fixed_rate: true,
        }),
        VoiceEntry::Noise(NoiseVoice {
            base_key: 60,
            length: 0,
            period: 1,
            envelope: sample_envelope(),
            fixed_rate: false,
        }),
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
    // `voice_directsound_alt` is `TONEDATA_TYPE_REV` (0x10): reverse
    // playback, two occurrences project-wide in `rs_sfx_2.inc`.
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
fn square_and_wave_and_noise_voices_round_trip() {
    let group = VoiceGroup::new(one_of_every_kind()).unwrap();
    let bytes = group.encode();
    assert_eq!(VoiceGroup::decode(&bytes).unwrap(), group);
}

#[test]
fn a_cgb_voices_length_is_a_plain_byte_not_an_optional_pan() {
    // `_voice_square_1` and friends write their `pan` operand at `ToneData`
    // offset 2, which is `length` -- the NRx1 sound-length counter, never
    // read as pan (see the module docs). Pin that the whole byte range
    // survives, including the `0` an `Option<u8>` pan encoding would have
    // collapsed into "no override".
    for length in [0u8, 1, 0x80, 0xFF] {
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
    // The documented `0..=VOICE_SLOT_COUNT` bound is what makes the encoded
    // `u8` table length total -- it must be an error at construction, not a
    // panic inside `encode`.
    let table = vec![0u8; VOICE_SLOT_COUNT + 1];
    assert_eq!(
        KeySplitVoice::new(0, table, VoiceGroupId("audio/voicegroup/x".to_owned())),
        Err(AudioError::KeySplitTableTooLong(VOICE_SLOT_COUNT + 1))
    );
}

#[test]
fn decode_rejects_a_declared_key_split_table_length_above_the_maximum() {
    // The read side must enforce the same bound as the constructor.
    let group = VoiceGroup::new(vec![VoiceEntry::KeySplit(
        KeySplitVoice::new(0, vec![0], VoiceGroupId("audio/voicegroup/x".to_owned())).unwrap(),
    )])
    .unwrap();
    let mut bytes = group.encode();
    // [0] slot count, [1] kind, [2] starting note, [3] table length.
    bytes[3] = 200;
    assert_eq!(
        VoiceGroup::decode(&bytes),
        Err(AudioError::KeySplitTableTooLong(200))
    );
}

#[test]
fn an_oversize_pack_id_is_rejected_by_the_constructor() {
    // `Writer::string`'s `u16` length prefix is the bound; reaching it must
    // be an `AudioError`, not a panic from safe code. Checked for each of
    // the id-bearing slot kinds.
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

    let rhythm = VoiceEntry::Rhythm(RhythmVoice {
        children: VoiceGroupId(too_long),
    });
    assert_eq!(VoiceGroup::new(vec![rhythm]), expected);
}

#[test]
fn all_128_slots_round_trip_including_slot_127() {
    // `MUS_TITLE`'s MIDI source selects instrument 127 on one channel
    // (see `super`'s module docs) -- pin that the full slot range,
    // including the last slot, is representable and round-trips.
    let slots: Vec<VoiceEntry> = (0..VOICE_SLOT_COUNT)
        .map(|i| {
            VoiceEntry::Square1(Square1Voice {
                base_key: 60,
                length: 0,
                sweep: 0,
                duty: u8::try_from(i % 4).expect("i % 4 < 4"),
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
    assert_eq!(decoded.slot(127), group.slots().last());
    assert!(decoded.slot(128).is_none());
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
    // A hand-corrupted count byte (129) must be rejected by the decoder
    // itself, not just the constructor -- the read side must not trust
    // encoded content (mirrors `crate::pack`'s directory parser).
    let mut bytes = VoiceGroup::new(vec![]).unwrap().encode();
    bytes[0] = 129; // the leading slot-count byte
    assert_eq!(
        VoiceGroup::decode(&bytes),
        Err(AudioError::TooManyVoiceSlots(129))
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
    // Every prefix of a group holding one slot of every kind: no cut may
    // panic, and none may decode as a valid (shorter) group.
    let bytes = VoiceGroup::new(one_of_every_kind()).unwrap().encode();
    for cut in 0..bytes.len() {
        assert!(
            VoiceGroup::decode(&bytes[..cut]).is_err(),
            "a {cut}-byte prefix decoded successfully"
        );
    }
}

#[test]
fn an_unknown_slot_kind_byte_is_rejected() {
    let mut bytes = VoiceGroup::new(one_of_every_kind()).unwrap().encode();
    bytes[1] = 0xFF; // the first slot's kind tag, right after the count
    assert_eq!(
        VoiceGroup::decode(&bytes),
        Err(AudioError::UnknownVoiceKind(0xFF))
    );
}

#[test]
fn a_kind_byte_just_past_the_last_defined_one_is_rejected() {
    // `KIND_EMPTY` (7) is the highest defined slot kind; 8 must not be
    // read as some neighbouring kind.
    let mut bytes = VoiceGroup::new(one_of_every_kind()).unwrap().encode();
    bytes[1] = KIND_EMPTY + 1;
    assert_eq!(
        VoiceGroup::decode(&bytes),
        Err(AudioError::UnknownVoiceKind(KIND_EMPTY + 1))
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
    // The mode byte is the last one a `DirectSound` slot writes.
    let last = bytes.len() - 1;
    bytes[last] = MODE_REVERSE + 1;
    assert_eq!(
        VoiceGroup::decode(&bytes),
        Err(AudioError::UnknownDirectSoundMode(MODE_REVERSE + 1))
    );
}

#[test]
fn a_non_utf8_pack_id_is_rejected() {
    let group = VoiceGroup::new(vec![VoiceEntry::Rhythm(RhythmVoice {
        children: VoiceGroupId("audio/voicegroup/emerald_drumset_1".to_owned()),
    })])
    .unwrap();
    let mut bytes = group.encode();
    // [0] slot count, [1] kind, [2..4] the id's u16 length prefix, then the
    // id's own bytes.
    bytes[4] = 0xFF;
    assert_eq!(VoiceGroup::decode(&bytes), Err(AudioError::InvalidString));
}

#[test]
fn pan_zero_is_never_produced_by_decode() {
    // The upstream wire format cannot distinguish "no override" from
    // "override to 0" (see the module docs) -- confirm the round trip
    // for `pan: None` never comes back as `Some(0)`.
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

/// The construction half of the sentinel contract
/// ([`pan_zero_is_never_produced_by_decode`]'s doc comment): `Some(0)`
/// would silently decay to `None` across a round trip, so
/// [`VoiceGroup::new`] must reject it outright (review finding on #193).
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

/// Review finding on #193: an unused slot in the middle of a group — the
/// normal state of a rhythm child table, where most raw keys have no drum
/// — must keep its position across a round trip, because a slot's index
/// *is* its meaning (the played key). Omitting it would shift every later
/// slot; a zeroed `DirectSound` stand-in would invent a sample id.
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

/// Review finding on #193: a decoder that ignores trailing bytes validates
/// a corrupt (or newer-producer) payload as its own prefix. The reader
/// must be exhausted when decoding succeeds.
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
