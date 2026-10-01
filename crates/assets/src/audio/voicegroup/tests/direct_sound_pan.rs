use super::super::*;
use super::shared::sample_envelope;

const FIRST_DIRECT_SOUND_PAN_BYTE: usize = 3;

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

fn direct_sound_with_pan(pan: Option<u8>) -> VoiceEntry {
    VoiceEntry::DirectSound(DirectSoundVoice {
        base_key: 60,
        pan,
        sample: SampleId("audio/sample/x".to_owned()),
        envelope: sample_envelope(),
        mode: DirectSoundMode::Resampled,
    })
}

#[test]
fn pan_override_boundaries_are_accepted_by_the_constructor_and_decode() {
    for pan in [1u8, 127] {
        let group = VoiceGroup::new(vec![direct_sound_with_pan(Some(pan))]).unwrap();
        let decoded = VoiceGroup::decode(&group.encode()).unwrap();
        assert_eq!(decoded, group);
        match &decoded.slots()[0] {
            VoiceEntry::DirectSound(v) => assert_eq!(v.pan, Some(pan)),
            other => panic!("expected a DirectSound slot, got {other:?}"),
        }
    }
}

#[test]
fn pan_out_of_range_is_rejected_at_construction() {
    for pan in [128u8, 200, u8::MAX] {
        let err = VoiceGroup::new(vec![direct_sound_with_pan(Some(pan))]).unwrap_err();
        assert_eq!(err, AudioError::PanOverrideOutOfRange(pan));
    }
}

/// A pan byte above `127` aliases an in-domain value once `0x80 | pan` reaches
/// `audio::rhythm_pan_from_pan_sweep` (`Some(200)` pans as `Some(72)`), so decode rejects it.
#[test]
fn decode_rejects_out_of_range_pan() {
    let group = VoiceGroup::new(vec![direct_sound_with_pan(Some(64))]).unwrap();
    let mut bytes = group.encode();
    bytes[FIRST_DIRECT_SOUND_PAN_BYTE] = 128;
    assert_eq!(
        VoiceGroup::decode(&bytes),
        Err(AudioError::PanOverrideOutOfRange(128))
    );
}
