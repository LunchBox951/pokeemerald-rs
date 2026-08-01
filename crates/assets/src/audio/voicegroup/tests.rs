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
fn direct_sound_alt_mode_round_trips() {
    let group = VoiceGroup::new(vec![VoiceEntry::DirectSound(DirectSoundVoice {
        base_key: 60,
        pan: None,
        sample: SampleId("audio/sample/bicycle_bell".to_owned()),
        envelope: sample_envelope(),
        mode: DirectSoundMode::Alt,
    })])
    .unwrap();
    let bytes = group.encode();
    assert_eq!(VoiceGroup::decode(&bytes).unwrap(), group);
}

#[test]
fn square_and_wave_and_noise_voices_round_trip() {
    let group = VoiceGroup::new(vec![
        VoiceEntry::Square1(Square1Voice {
            base_key: 60,
            pan: None,
            sweep: 0,
            duty: 2,
            envelope: Envelope {
                attack: 0,
                decay: 0,
                sustain: 15,
                release: 0,
            },
            fixed_rate: false,
        }),
        VoiceEntry::Square2(Square2Voice {
            base_key: 60,
            pan: Some(64),
            duty: 3,
            envelope: Envelope {
                attack: 0,
                decay: 1,
                sustain: 6,
                release: 2,
            },
            fixed_rate: true,
        }),
        VoiceEntry::ProgrammableWave(ProgrammableWaveVoice {
            base_key: 60,
            pan: None,
            wave: SampleId("audio/sample/programmable_wave_2".to_owned()),
            envelope: Envelope {
                attack: 0,
                decay: 7,
                sustain: 15,
                release: 0,
            },
            fixed_rate: true,
        }),
        VoiceEntry::Noise(NoiseVoice {
            base_key: 60,
            pan: None,
            period: 1,
            envelope: sample_envelope(),
            fixed_rate: false,
        }),
    ])
    .unwrap();
    let bytes = group.encode();
    assert_eq!(VoiceGroup::decode(&bytes).unwrap(), group);
}

#[test]
fn key_split_and_rhythm_indirection_round_trip() {
    let group = VoiceGroup::new(vec![
        VoiceEntry::KeySplit(KeySplitVoice {
            starting_note: 36,
            table: vec![0, 0, 1, 1, 2],
            children: VoiceGroupId("audio/voicegroup/trumpet_keysplit".to_owned()),
        }),
        VoiceEntry::Rhythm(RhythmVoice {
            children: VoiceGroupId("audio/voicegroup/emerald_drumset_1".to_owned()),
        }),
    ])
    .unwrap();
    let bytes = group.encode();
    assert_eq!(VoiceGroup::decode(&bytes).unwrap(), group);
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
                pan: None,
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
    bytes[4] = 129; // the slot-count byte, right after the 4-byte version
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
fn unsupported_version_is_rejected() {
    let mut bytes = VoiceGroup::new(vec![]).unwrap().encode();
    bytes[0] = 0xFF;
    assert_eq!(
        VoiceGroup::decode(&bytes),
        Err(AudioError::UnsupportedVersion {
            schema: "voicegroup",
            found: u32::from_le_bytes([0xFF, 0, 0, 0]),
        })
    );
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
        _ => unreachable!(),
    }
}
