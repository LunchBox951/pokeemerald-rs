use super::super::{AssetPack, PackError};
use super::shared::{
    write_synthetic_pack, EXCESS_VOICE_SLOT_COUNT_BYTE, PROGRAMMABLE_WAVE_BYTE_COUNT,
    UNKNOWN_SAMPLE_KIND_TAG,
};
use crate::audio::{Sample, SampleId, SongEvent, VoiceEntry, VoiceGroupId};

#[test]
fn song_accessor_decodes_the_named_entry_through_the_song_schema() {
    let path = write_synthetic_pack("song-ok");
    let pack = AssetPack::load(&path).unwrap();
    let song = pack.song("test_song").unwrap();
    assert_eq!(
        song.voicegroup(),
        &VoiceGroupId("audio/voicegroup/test_group".to_owned())
    );
    assert_eq!(song.priority(), 5);
    assert_eq!(song.reverb(), Some(30));
    assert_eq!(
        song.tracks(),
        [[SongEvent::Voice(2), SongEvent::Wait(4), SongEvent::Fine]]
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn song_accessor_reports_a_missing_entry() {
    let path = write_synthetic_pack("song-missing");
    let pack = AssetPack::load(&path).unwrap();
    assert_eq!(
        pack.song("does_not_exist"),
        Err(PackError::UnknownAsset(
            "audio/song/does_not_exist".to_owned()
        ))
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn song_accessor_reports_the_wrong_kind() {
    let path = write_synthetic_pack("song-wrong-kind");
    let pack = AssetPack::load(&path).unwrap();
    assert!(matches!(
        pack.song("not_raw"),
        Err(PackError::WrongKind {
            expected: "raw blob",
            actual: "image",
            ..
        })
    ));
    let _ = std::fs::remove_file(path);
}

#[test]
fn song_accessor_reports_a_malformed_payload() {
    let path = write_synthetic_pack("song-malformed");
    let pack = AssetPack::load(&path).unwrap();
    assert!(matches!(
        pack.song("malformed"),
        Err(PackError::AudioDecode { id, source: crate::audio::AudioError::Truncated })
            if id == "audio/song/malformed"
    ));
    let _ = std::fs::remove_file(path);
}

#[test]
fn voicegroup_accessor_decodes_the_referenced_entry_through_the_voicegroup_schema() {
    let path = write_synthetic_pack("voicegroup-ok");
    let pack = AssetPack::load(&path).unwrap();
    let id = VoiceGroupId("audio/voicegroup/test_group".to_owned());
    let group = pack.voicegroup(&id).unwrap();
    assert_eq!(group.slots().len(), 2);
    match group.slot(0) {
        Some(VoiceEntry::DirectSound(voice)) => {
            assert_eq!(
                voice.sample,
                SampleId("audio/sample/direct-sound/test_sample".to_owned())
            );
        }
        other => panic!("expected slot 0 to be a DirectSound voice, got {other:?}"),
    }
    assert_eq!(group.slot(1), Some(&VoiceEntry::Empty));
    let _ = std::fs::remove_file(path);
}

#[test]
fn voicegroup_accessor_reports_a_missing_entry() {
    let path = write_synthetic_pack("voicegroup-missing");
    let pack = AssetPack::load(&path).unwrap();
    let id = VoiceGroupId("audio/voicegroup/does_not_exist".to_owned());
    assert_eq!(
        pack.voicegroup(&id),
        Err(PackError::UnknownAsset(id.0.clone()))
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn voicegroup_and_sample_accessors_report_the_wrong_kind() {
    let path = write_synthetic_pack("audio-wrong-kind");
    let pack = AssetPack::load(&path).unwrap();
    assert!(matches!(
        pack.voicegroup(&VoiceGroupId("audio/song/not_raw".to_owned())),
        Err(PackError::WrongKind {
            expected: "raw blob",
            actual: "image",
            ..
        })
    ));
    assert!(matches!(
        pack.sample(&SampleId("audio/song/not_raw".to_owned())),
        Err(PackError::WrongKind {
            expected: "raw blob",
            actual: "image",
            ..
        })
    ));
    let _ = std::fs::remove_file(path);
}

#[test]
fn voicegroup_accessor_reports_a_malformed_payload() {
    let path = write_synthetic_pack("voicegroup-malformed");
    let pack = AssetPack::load(&path).unwrap();
    let id = VoiceGroupId("audio/voicegroup/malformed".to_owned());
    assert!(matches!(
        pack.voicegroup(&id),
        Err(PackError::AudioDecode {
            id: ref got,
            source: crate::audio::AudioError::TooManyVoiceSlots(slot_count),
        }) if got == &id.0 && slot_count == usize::from(EXCESS_VOICE_SLOT_COUNT_BYTE)
    ));
    let _ = std::fs::remove_file(path);
}

#[test]
fn sample_accessor_decodes_direct_sound_and_programmable_wave_entries() {
    let path = write_synthetic_pack("sample-ok");
    let pack = AssetPack::load(&path).unwrap();

    let direct_sound_id = SampleId("audio/sample/direct-sound/test_sample".to_owned());
    let Sample::DirectSound(sample) = pack.sample(&direct_sound_id).unwrap() else {
        panic!("expected a DirectSound sample");
    };
    assert_eq!(sample.base_frequency, 12345);
    assert_eq!(sample.loop_start(), Some(2));
    assert_eq!(sample.data(), &[-1, 0, 1, 2]);

    let wave_id = SampleId("audio/sample/programmable-wave/01".to_owned());
    let Sample::ProgrammableWave(wave) = pack.sample(&wave_id).unwrap() else {
        panic!("expected a ProgrammableWave sample");
    };
    assert_eq!(wave.table, [7; PROGRAMMABLE_WAVE_BYTE_COUNT]);

    let _ = std::fs::remove_file(path);
}

#[test]
fn sample_accessor_reports_a_missing_entry() {
    let path = write_synthetic_pack("sample-missing");
    let pack = AssetPack::load(&path).unwrap();
    let id = SampleId("audio/sample/direct-sound/does_not_exist".to_owned());
    assert_eq!(pack.sample(&id), Err(PackError::UnknownAsset(id.0.clone())));
    let _ = std::fs::remove_file(path);
}

#[test]
fn sample_accessor_reports_a_malformed_payload() {
    let path = write_synthetic_pack("sample-malformed");
    let pack = AssetPack::load(&path).unwrap();
    let id = SampleId("audio/sample/malformed".to_owned());
    assert!(matches!(
        pack.sample(&id),
        Err(PackError::AudioDecode {
            id: ref got,
            source: crate::audio::AudioError::UnknownSampleKind(UNKNOWN_SAMPLE_KIND_TAG),
        }) if got == &id.0
    ));
    let _ = std::fs::remove_file(path);
}
