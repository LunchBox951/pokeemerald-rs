use super::{AssetPack, ConsumerConstraint, EntryKind, PackProblem, RequiredAsset};

struct ScratchPack(std::path::PathBuf);

impl ScratchPack {
    fn new(bytes: &[u8]) -> Self {
        let path = std::env::temp_dir().join(format!(
            "pack-readiness-{}-{:?}.pack",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::write(&path, bytes).unwrap();
        Self(path)
    }
}

impl Drop for ScratchPack {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn current_format_without_required_content_is_incomplete() {
    let bytes = pack_format::PackWriter::new().finish().unwrap();
    let pack = ScratchPack::new(&bytes);
    let required = [RequiredAsset {
        id: "title/image/rayquaza".into(),
        kind: EntryKind::Image {
            width: 8,
            height: 8,
            bit_depth: 4,
        },
        byte_len: None,
        consumer: None,
    }];
    let error = AssetPack::load_ready(&pack.0, &required).unwrap_err();
    assert_eq!(
        error.problem(),
        &PackProblem::MissingAsset("title/image/rayquaza".into())
    );
    assert_eq!(std::fs::read(&pack.0).unwrap(), bytes);
}

fn pack_with(entries: Vec<pack_format::PackEntry>) -> Vec<u8> {
    let mut writer = pack_format::PackWriter::new();
    for entry in entries {
        writer.push(entry);
    }
    writer.finish().unwrap()
}

#[test]
fn compatible_pack_and_additional_entries_are_accepted_unchanged() {
    let bytes = pack_with(vec![
        pack_format::raw_entry("layout/test/map".into(), vec![0, 0]),
        pack_format::raw_entry("future/optional".into(), vec![1, 2, 3]),
    ]);
    let pack = ScratchPack::new(&bytes);
    let required = [RequiredAsset {
        id: "layout/test/map".into(),
        kind: EntryKind::Raw,
        byte_len: Some(2),
        consumer: None,
    }];
    let accepted = AssetPack::load_ready(&pack.0, &required).unwrap();
    assert_eq!(accepted.bytes(), bytes);
    assert_eq!(std::fs::read(&pack.0).unwrap(), bytes);
}

#[test]
fn stale_format_reports_found_and_supported_versions_without_rewriting() {
    let mut bytes = pack_with(vec![]);
    bytes[8..12].copy_from_slice(&0u32.to_le_bytes());
    let pack = ScratchPack::new(&bytes);
    let error = AssetPack::load_ready(&pack.0, &[]).unwrap_err();
    assert_eq!(
        error.problem(),
        &PackProblem::UnsupportedFormat {
            found: 0,
            supported: pack_format::FORMAT_VERSION,
        }
    );
    assert_eq!(std::fs::read(&pack.0).unwrap(), bytes);
}

#[test]
fn malformed_container_reports_the_parser_failure_without_rewriting() {
    for bytes in [b"not a pack".to_vec(), pack_with(vec![])[..12].to_vec()] {
        let pack = ScratchPack::new(&bytes);
        let error = AssetPack::load_ready(&pack.0, &[]).unwrap_err();
        assert!(matches!(error.problem(), PackProblem::Malformed(_)));
        assert_eq!(std::fs::read(&pack.0).unwrap(), bytes);
    }
}

#[test]
fn incorrect_required_kind_shape_and_length_are_rejected() {
    let bytes = pack_with(vec![pack_format::raw_entry(
        "layout/test/map".into(),
        vec![0, 0],
    )]);
    let pack = ScratchPack::new(&bytes);
    for (kind, byte_len) in [
        (EntryKind::Palette { color_count: 1 }, None),
        (EntryKind::Raw, Some(4)),
    ] {
        let required = [RequiredAsset {
            id: "layout/test/map".into(),
            kind,
            byte_len,
            consumer: None,
        }];
        assert!(
            matches!(AssetPack::load_ready(&pack.0, &required).unwrap_err().problem(), PackProblem::InvalidAsset { id, .. } if id == "layout/test/map")
        );
    }
    let bytes = pack_with(vec![pack_format::image_entry(
        "test/image".into(),
        8,
        8,
        4,
        vec![0; 64],
    )
    .unwrap()]);
    let pack = ScratchPack::new(&bytes);
    let required = [RequiredAsset {
        id: "test/image".into(),
        kind: EntryKind::Image {
            width: 16,
            height: 8,
            bit_depth: 4,
        },
        byte_len: None,
        consumer: None,
    }];
    assert!(
        matches!(AssetPack::load_ready(&pack.0, &required).unwrap_err().problem(), PackProblem::InvalidAsset { id, .. } if id == "test/image")
    );
}

#[test]
fn malformed_payload_is_rejected_even_outside_the_required_inventory() {
    let bytes = pack_with(vec![pack_format::raw_entry(
        "audio/song/optional".into(),
        vec![0],
    )]);
    let pack = ScratchPack::new(&bytes);
    let error = AssetPack::load_ready(&pack.0, &[]).unwrap_err();
    assert!(
        matches!(error.problem(), PackProblem::InvalidAsset { id, .. } if id == "audio/song/optional")
    );
    assert_eq!(std::fs::read(&pack.0).unwrap(), bytes);
}

#[test]
fn missing_referenced_voicegroup_is_incomplete() {
    let song = crate::Song::new(
        crate::VoiceGroupId("audio/voicegroup/missing".into()),
        crate::MusicPlayerId(0),
        0,
        None,
        vec![],
    )
    .unwrap();
    let pack = ScratchPack::new(&pack_with(vec![pack_format::raw_entry(
        "audio/song/test".into(),
        song.encode(),
    )]));
    assert_eq!(
        AssetPack::load_ready(&pack.0, &[]).unwrap_err().problem(),
        &PackProblem::MissingAsset("audio/voicegroup/missing".into())
    );
}

#[test]
fn referenced_payloads_are_decoded_even_outside_their_usual_id_family() {
    let song = crate::Song::new(
        crate::VoiceGroupId("extension/instrument".into()),
        crate::MusicPlayerId(0),
        0,
        None,
        vec![],
    )
    .unwrap();
    let pack = ScratchPack::new(&pack_with(vec![
        pack_format::raw_entry("audio/song/test".into(), song.encode()),
        pack_format::raw_entry("extension/instrument".into(), vec![]),
    ]));
    assert!(
        matches!(AssetPack::load_ready(&pack.0, &[]).unwrap_err().problem(), PackProblem::InvalidAsset { id, .. } if id == "extension/instrument")
    );
}

#[test]
fn a_present_but_incomplete_graphics_group_is_rejected() {
    let bytes = pack_with(vec![pack_format::raw_entry(
        "starter-chooser/unknown".into(),
        vec![],
    )]);
    let pack = ScratchPack::new(&bytes);
    assert!(matches!(
        AssetPack::load_ready(&pack.0, &[]).unwrap_err().problem(),
        PackProblem::MissingAsset(_)
    ));
}

#[test]
fn a_cry_table_cannot_reference_a_missing_sample() {
    let id = crate::SampleId("audio/sample/direct-sound/missing".into());
    let table = crate::audio::CryTable::new(vec![id.clone(); crate::audio::CRY_TABLE_LEN]).unwrap();
    let bytes = pack_with(vec![pack_format::raw_entry(
        crate::audio::CRY_TABLE_PACK_ID.into(),
        table.encode(),
    )]);
    let pack = ScratchPack::new(&bytes);
    assert_eq!(
        AssetPack::load_ready(&pack.0, &[]).unwrap_err().problem(),
        &PackProblem::MissingAsset(id.0)
    );
}

#[test]
fn missing_and_unreadable_packs_are_distinguishable() {
    let path =
        std::env::temp_dir().join(format!("absent-pack-readiness-{}.pack", std::process::id()));
    assert_eq!(
        AssetPack::load_ready(&path, &[]).unwrap_err().problem(),
        &PackProblem::Missing
    );
    assert!(!path.exists());
    let unreadable = AssetPack::load_ready(&std::env::temp_dir(), &[]).unwrap_err();
    assert!(matches!(unreadable.problem(), PackProblem::Unreadable(_)));
    assert!(unreadable.to_string().contains("--import-rom <rom>"));
    assert!(unreadable.to_string().contains("Check file permissions"));
}

#[test]
fn recovery_guidance_cannot_emit_control_characters_from_paths_or_asset_ids() {
    let error = super::PackReadinessError::new(
        std::path::Path::new("pack\n\u{1b}[2K"),
        PackProblem::MissingAsset("asset\n\u{1b}[2K".into()),
    );
    let text = error.to_string();
    assert!(text.contains("--import-rom <rom>"));
    assert!(text.contains("cargo xtask extract"));
    assert!(!text.contains('\n'));
    assert!(!text.contains('\u{1b}'));
}

fn sheet_requirement(consumer: Option<ConsumerConstraint>) -> RequiredAsset {
    RequiredAsset {
        id: "test/sheet".into(),
        kind: EntryKind::Image {
            width: 8,
            height: 8,
            bit_depth: 8,
        },
        byte_len: None,
        consumer,
    }
}

fn sheet_with_first_pixel(pixel: u8) -> ScratchPack {
    let mut pixels = vec![0; 64];
    pixels[0] = pixel;
    ScratchPack::new(&pack_with(vec![pack_format::image_entry(
        "test/sheet".into(),
        8,
        8,
        8,
        pixels,
    )
    .unwrap()]))
}

#[test]
fn a_consumer_narrower_than_the_declared_depth_rejects_indices_it_cannot_read() {
    let narrow = Some(ConsumerConstraint::ImagePixelDepth(4));
    for pixel in 16..=u8::MAX {
        let pack = sheet_with_first_pixel(pixel);
        let error = AssetPack::load_ready(&pack.0, &[sheet_requirement(narrow)]).unwrap_err();
        assert!(
            matches!(error.problem(), PackProblem::InvalidAsset { id, .. } if id == "test/sheet"),
            "pixel {pixel}: {error}"
        );
        assert!(error.to_string().contains("--import-rom <rom>"));
        // The declared depth alone still admits the value.
        assert!(AssetPack::load_ready(&pack.0, &[sheet_requirement(None)]).is_ok());
    }
    let pack = sheet_with_first_pixel(15);
    assert!(AssetPack::load_ready(&pack.0, &[sheet_requirement(narrow)]).is_ok());
}

fn playable_song_pack(
    voices: Vec<crate::VoiceEntry>,
    children: Vec<crate::VoiceEntry>,
) -> ScratchPack {
    let song = crate::Song::new(
        crate::VoiceGroupId("audio/voicegroup/title".into()),
        crate::MusicPlayerId(0),
        0,
        None,
        vec![],
    )
    .unwrap();
    let sample = crate::Sample::DirectSound(
        crate::audio::DirectSoundSample::new(1, None, 1, vec![0, 0]).unwrap(),
    );
    ScratchPack::new(&pack_with(vec![
        pack_format::raw_entry("audio/song/title".into(), song.encode()),
        pack_format::raw_entry(
            "audio/voicegroup/title".into(),
            crate::VoiceGroup::new(voices).unwrap().encode(),
        ),
        pack_format::raw_entry(
            "audio/voicegroup/children".into(),
            crate::VoiceGroup::new(children).unwrap().encode(),
        ),
        pack_format::raw_entry("audio/sample/direct-sound/s".into(), sample.encode()),
    ]))
}

fn direct_sound(mode: crate::audio::DirectSoundMode) -> crate::VoiceEntry {
    crate::VoiceEntry::DirectSound(crate::audio::DirectSoundVoice {
        base_key: 60,
        pan: None,
        sample: crate::SampleId("audio/sample/direct-sound/s".into()),
        envelope: crate::Envelope {
            attack: 255,
            decay: 0,
            sustain: 255,
            release: 0,
        },
        mode,
    })
}

fn playable_song_requirement() -> RequiredAsset {
    RequiredAsset {
        id: "audio/song/title".into(),
        kind: EntryKind::Raw,
        byte_len: None,
        consumer: Some(ConsumerConstraint::PlayableSong),
    }
}

fn key_split_to_children() -> crate::VoiceEntry {
    crate::VoiceEntry::KeySplit(
        crate::audio::KeySplitVoice::new(
            0,
            vec![0],
            crate::VoiceGroupId("audio/voicegroup/children".into()),
        )
        .unwrap(),
    )
}

fn rhythm_to_children() -> crate::VoiceEntry {
    crate::VoiceEntry::Rhythm(crate::audio::RhythmVoice {
        children: crate::VoiceGroupId("audio/voicegroup/children".into()),
    })
}

#[test]
fn a_reverse_direct_sound_voice_the_song_converts_is_not_playable() {
    use crate::audio::DirectSoundMode::{Fixed, Resampled, Reverse};
    let ok = playable_song_pack(vec![direct_sound(Resampled), direct_sound(Fixed)], vec![]);
    assert!(AssetPack::load_ready(&ok.0, &[playable_song_requirement()]).is_ok());

    let top = playable_song_pack(vec![direct_sound(Resampled), direct_sound(Reverse)], vec![]);
    let error = AssetPack::load_ready(&top.0, &[playable_song_requirement()]).unwrap_err();
    assert!(
        matches!(error.problem(), PackProblem::InvalidAsset { id, reason }
            if id == "audio/voicegroup/title" && reason.contains("reverse")),
        "{error}"
    );
    assert!(error.to_string().contains("--import-rom <rom>"));
    // Without the playback constraint the same pack stays schema-valid.
    assert!(AssetPack::load_ready(&top.0, &[]).is_ok());

    for parent in [key_split_to_children(), rhythm_to_children()] {
        let nested = playable_song_pack(vec![parent.clone()], vec![direct_sound(Reverse)]);
        let error = AssetPack::load_ready(&nested.0, &[playable_song_requirement()]).unwrap_err();
        assert!(
            matches!(error.problem(), PackProblem::InvalidAsset { id, .. } if id == "audio/voicegroup/children"),
            "{error}"
        );
        let fine = playable_song_pack(vec![parent], vec![direct_sound(Fixed)]);
        assert!(AssetPack::load_ready(&fine.0, &[playable_song_requirement()]).is_ok());
    }
}

#[test]
fn a_reverse_voice_outside_the_songs_converted_voices_does_not_block() {
    use crate::audio::DirectSoundMode::Reverse;
    // Unreferenced by the song.
    let unreferenced = playable_song_pack(vec![], vec![direct_sound(Reverse)]);
    assert!(AssetPack::load_ready(&unreferenced.0, &[playable_song_requirement()]).is_ok());
    // A key-split child that is itself an indirection resolves silent without
    // converting its own children, so a reverse leaf behind it is never reached.
    let deeper = playable_song_pack(vec![key_split_to_children()], vec![rhythm_to_children()]);
    assert!(AssetPack::load_ready(&deeper.0, &[playable_song_requirement()]).is_ok());
}

#[test]
fn an_unsupported_consumer_depth_is_a_typed_failure_not_a_panic() {
    let pack = sheet_with_first_pixel(0);
    for depth in [0, 9, u8::MAX] {
        let error = AssetPack::load_ready(
            &pack.0,
            &[sheet_requirement(Some(
                ConsumerConstraint::ImagePixelDepth(depth),
            ))],
        )
        .unwrap_err();
        assert!(
            matches!(error.problem(), PackProblem::InvalidAsset { id, .. } if id == "test/sheet"),
            "depth {depth}: {error}"
        );
    }
}
