use super::{AssetPack, EntryKind, PackProblem, RequiredAsset};

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
