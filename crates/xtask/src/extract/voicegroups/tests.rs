use super::{
    discover_voicegroup_sources, extract_voicegroup_closure, index_link_order,
    index_voicegroup_sources, link_order_successors,
};
use pack_format::PackWriter;

const TITLE_VOICEGROUP_LABEL: &str = "title";
const REFERENCE_VOICEGROUP_SOURCE_COUNT: usize = 195;
const TITLE_DECLARED_SLOT_COUNT: usize = 89;
const RS_DRUMSET_FIRST_DECLARED_SLOT: usize = 36;
const RS_DRUMSET_DECLARED_SLOT_COUNT: usize = 29;
const DUMMY_DECLARED_SLOT_COUNT: usize = 98;
const TITLE_DEPENDENCY_LABELS: [&str; 7] = [
    "french_horn_keysplit",
    "piano_keysplit",
    "rs_drumset",
    "strings_keysplit",
    "title",
    "trumpet_keysplit",
    "tuba_keysplit",
];

#[test]
#[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
fn every_voicegroups_inc_file_parses() {
    assert!(super::super::upstream_present(), "run ./init.sh first");
    let upstream = super::super::repo_root().join("pokeemerald");
    let index = index_voicegroup_sources(&upstream).expect("every real .inc file should parse");
    assert_eq!(
        index.groups_by_label.len(),
        REFERENCE_VOICEGROUP_SOURCE_COUNT
    );
    assert_eq!(
        index.labels_by_relative_path.len(),
        REFERENCE_VOICEGROUP_SOURCE_COUNT
    );
    assert!(index.groups_by_label.contains_key(TITLE_VOICEGROUP_LABEL));
    assert_eq!(
        index
            .labels_by_relative_path
            .get("title.inc")
            .map(String::as_str),
        Some("title")
    );
    assert_eq!(
        index
            .labels_by_relative_path
            .get("drumsets/rs.inc")
            .map(String::as_str),
        Some("rs_drumset")
    );
}

#[test]
#[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
fn title_is_immediately_followed_by_intro_in_the_real_link_order() {
    assert!(super::super::upstream_present(), "run ./init.sh first");
    let upstream = super::super::repo_root().join("pokeemerald");
    let index = index_voicegroup_sources(&upstream).unwrap();
    let successors = link_order_successors(
        &upstream,
        TITLE_VOICEGROUP_LABEL,
        &index.labels_by_relative_path,
    )
    .unwrap();
    assert_eq!(successors.first().map(String::as_str), Some("intro"));
}

#[test]
#[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
fn title_strings_key_split_plays_mus_title_key_33_like_upstream() {
    // mus_title.mid channel 2 plays key 33 at tick 2160 on program 48
    // (title.inc:50, the strings key split). Upstream's strings label sits
    // 36 bytes before its data (asm/macros/m4a.inc:25-32), so key 33 reads
    // piano's final byte (keysplit_tables.inc:13-22): child 3. Strings
    // declares three records (keysplits/strings.inc:1-4) and is linked
    // right before trumpet (voice_groups.inc:11-12), so child 3 is
    // trumpet's first record.
    use super::resolve::VoiceSlot;
    const STRINGS_PROGRAM: usize = 48;
    const PLAYED_KEY: usize = 33;
    assert!(super::super::upstream_present(), "run ./init.sh first");
    let upstream = super::super::repo_root().join("pokeemerald");
    let index = index_voicegroup_sources(&upstream).unwrap();
    let link_order = index_link_order(&upstream, &index.labels_by_relative_path).unwrap();
    let keysplit_text =
        std::fs::read_to_string(upstream.join("sound/keysplit_tables.inc")).unwrap();
    let tables = super::parser::parse_keysplit_tables_with_adjacency(&keysplit_text).unwrap();
    let groups = super::resolve::resolve_voice_groups_with_link_order(
        TITLE_VOICEGROUP_LABEL,
        &index.groups_by_label,
        &tables,
        &link_order,
    )
    .unwrap();
    let title = groups.iter().find(|g| g.label == "title").unwrap();
    let VoiceSlot::KeySplit {
        starting_note,
        table,
        ..
    } = &title.slots[STRINGS_PROGRAM]
    else {
        panic!("title slot 48 should be the strings key split");
    };
    let child_index = PLAYED_KEY
        .checked_sub(usize::from(*starting_note))
        .and_then(|offset| table.get(offset))
        .copied();
    assert_eq!(child_index, Some(3), "key 33 must map like upstream");
    let strings = groups
        .iter()
        .find(|g| g.label == "strings_keysplit")
        .unwrap();
    let VoiceSlot::DirectSound { sample_id, .. } = &strings.slots[3] else {
        panic!("strings_keysplit record 3 must be trumpet_keysplit's first record");
    };
    assert_eq!(sample_id, "audio/sample/direct-sound/sc88pro_trumpet_60");
}

#[test]
#[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
fn mus_titles_full_dependency_tree_resolves_to_expected_groups() {
    assert!(super::super::upstream_present(), "run ./init.sh first");
    let upstream = super::super::repo_root().join("pokeemerald");
    let index = index_voicegroup_sources(&upstream).unwrap();
    let link_order = index_link_order(&upstream, &index.labels_by_relative_path).unwrap();
    let keysplit_text =
        std::fs::read_to_string(upstream.join("sound/keysplit_tables.inc")).unwrap();
    let keysplit_tables =
        super::parser::parse_keysplit_tables_with_adjacency(&keysplit_text).unwrap();

    let groups = super::resolve::resolve_voice_groups_with_link_order(
        TITLE_VOICEGROUP_LABEL,
        &index.groups_by_label,
        &keysplit_tables,
        &link_order,
    )
    .expect("MUS_TITLE's real dependency tree should resolve cleanly");

    let mut labels: Vec<&str> = groups.iter().map(|g| g.label.as_str()).collect();
    labels.sort_unstable();
    assert_eq!(labels, TITLE_DEPENDENCY_LABELS);

    let title = groups.iter().find(|g| g.label == "title").unwrap();
    assert_eq!(title.slots.len(), super::VOICE_SLOT_COUNT);
    assert_eq!(
        index.groups_by_label["title"].slots.len(),
        TITLE_DECLARED_SLOT_COUNT
    );
    assert_eq!(
        title.slots[TITLE_DECLARED_SLOT_COUNT],
        super::resolve::VoiceSlot::Rhythm {
            children_id: "audio/voicegroup/rs_drumset".to_owned(),
        }
    );
    assert_eq!(
        *title.slots.last().unwrap(),
        super::resolve::VoiceSlot::Square1 {
            base_key: 60,
            length: 0,
            sweep: 0,
            duty: 2,
            envelope: super::parser::Envelope {
                attack: 0,
                decay: 0,
                sustain: 15,
                release: 0,
            },
            fixed_rate: false,
        }
    );

    let rs_drumset = groups.iter().find(|g| g.label == "rs_drumset").unwrap();
    assert_eq!(rs_drumset.slots.len(), super::VOICE_SLOT_COUNT);
    let raw_rs_drumset = &index.groups_by_label["rs_drumset"];
    assert_eq!(
        usize::from(raw_rs_drumset.starting_note),
        RS_DRUMSET_FIRST_DECLARED_SLOT
    );
    assert_eq!(raw_rs_drumset.slots.len(), RS_DRUMSET_DECLARED_SLOT_COUNT);
    let after_last_declared_slot = RS_DRUMSET_FIRST_DECLARED_SLOT + RS_DRUMSET_DECLARED_SLOT_COUNT;
    // `voice_group rs_drumset, 36` aliases the 36 records before it:
    // dummy.inc's last 36, linked immediately ahead (sound/voice_groups.inc).
    let dummy = &index.groups_by_label["dummy"];
    assert_eq!(dummy.slots.len(), DUMMY_DECLARED_SLOT_COUNT);
    for slot in &rs_drumset.slots[..RS_DRUMSET_FIRST_DECLARED_SLOT] {
        assert_ne!(*slot, super::resolve::VoiceSlot::Empty);
    }
    assert!(matches!(
        &rs_drumset.slots[11],
        super::resolve::VoiceSlot::ProgrammableWave { wave_id, .. }
            if wave_id == "audio/sample/programmable-wave/01"
    ));
    assert!(matches!(
        &rs_drumset.slots[19],
        super::resolve::VoiceSlot::ProgrammableWave { wave_id, .. }
            if wave_id == "audio/sample/programmable-wave/03"
    ));
    for slot in &rs_drumset.slots[28..RS_DRUMSET_FIRST_DECLARED_SLOT] {
        assert!(matches!(
            slot,
            super::resolve::VoiceSlot::DirectSound { .. }
        ));
    }
    assert_eq!(
        rs_drumset.slots[RS_DRUMSET_FIRST_DECLARED_SLOT - 1],
        super::resolve::VoiceSlot::DirectSound {
            base_key: 60,
            pan: None,
            sample_id: "audio/sample/direct-sound/ethnic_flavours_hyoushigi".to_owned(),
            envelope: super::parser::Envelope {
                attack: 255,
                decay: 0,
                sustain: 255,
                release: 0,
            },
            mode: super::parser::DirectSoundMode::Resampled,
        }
    );
    for slot in &rs_drumset.slots[RS_DRUMSET_FIRST_DECLARED_SLOT..after_last_declared_slot] {
        assert_ne!(*slot, super::resolve::VoiceSlot::Empty);
    }
    for slot in &rs_drumset.slots[after_last_declared_slot..] {
        assert_eq!(*slot, super::resolve::VoiceSlot::Empty);
    }
}

#[test]
#[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
fn extraction_emits_every_expected_voicegroup_pack_entry() {
    assert!(super::super::upstream_present(), "run ./init.sh first");
    let upstream = super::super::repo_root().join("pokeemerald");
    let mut writer = PackWriter::new();
    extract_voicegroup_closure(&upstream, &[TITLE_VOICEGROUP_LABEL.to_owned()], &mut writer)
        .expect("extraction should succeed against a real checkout");
    assert_eq!(writer.len(), TITLE_DEPENDENCY_LABELS.len());
}

#[test]
fn voicegroup_source_discovery_rejects_missing_dir() {
    let err = discover_voicegroup_sources(std::path::Path::new("/does/not/exist")).unwrap_err();
    assert!(matches!(err, crate::extract::ExtractError::ReadFailed(..)));
}

#[test]
fn two_inc_files_declaring_the_same_label_are_rejected() {
    let root = std::env::temp_dir().join(format!(
        "pokeemerald-rs-voicegroup-duplicate-label-test-{}",
        std::process::id()
    ));
    let dir = root.join("sound/voicegroups");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&dir).unwrap();
    let body = "voice_group demo\n\tvoice_square_1 60, 0, 0, 2, 0, 0, 15, 0\n";
    std::fs::write(dir.join("a.inc"), body).unwrap();
    std::fs::write(dir.join("b.inc"), body).unwrap();

    let err = index_voicegroup_sources(&root).unwrap_err();
    std::fs::remove_dir_all(&root).unwrap();

    match err {
        crate::extract::ExtractError::DuplicateVoiceGroupLabel {
            label,
            first_path,
            second_path,
        } => {
            assert_eq!(label, "demo");
            assert_eq!(first_path, dir.join("a.inc"));
            assert_eq!(second_path, dir.join("b.inc"));
        }
        other => panic!("expected DuplicateVoiceGroupLabel, got {other:?}"),
    }
}

#[test]
fn link_order_successors_returns_every_file_after_top_label_in_order() {
    let mut labels_by_relative_path = std::collections::HashMap::new();
    labels_by_relative_path.insert("a.inc".to_owned(), "a".to_owned());
    labels_by_relative_path.insert("b.inc".to_owned(), "b".to_owned());
    labels_by_relative_path.insert("c.inc".to_owned(), "c".to_owned());
    let root = std::env::temp_dir().join(format!(
        "pokeemerald-rs-voicegroup-link-order-test-{}-{}",
        std::process::id(),
        line!()
    ));
    std::fs::create_dir_all(root.join("sound")).unwrap();
    std::fs::write(
        root.join("sound/voice_groups.inc"),
        ".include \"sound/voicegroups/a.inc\"\n.include \"sound/voicegroups/b.inc\"\n.include \"sound/voicegroups/c.inc\"\n",
    )
    .unwrap();

    let successors = link_order_successors(&root, "a", &labels_by_relative_path).unwrap();
    std::fs::remove_dir_all(&root).unwrap();

    assert_eq!(successors, vec!["b".to_owned(), "c".to_owned()]);
}

#[test]
fn link_order_successors_is_empty_when_top_label_is_last() {
    let mut labels_by_relative_path = std::collections::HashMap::new();
    labels_by_relative_path.insert("a.inc".to_owned(), "a".to_owned());
    labels_by_relative_path.insert("b.inc".to_owned(), "b".to_owned());
    let root = std::env::temp_dir().join(format!(
        "pokeemerald-rs-voicegroup-link-order-last-test-{}-{}",
        std::process::id(),
        line!()
    ));
    std::fs::create_dir_all(root.join("sound")).unwrap();
    std::fs::write(
        root.join("sound/voice_groups.inc"),
        ".include \"sound/voicegroups/a.inc\"\n.include \"sound/voicegroups/b.inc\"\n",
    )
    .unwrap();

    let successors = link_order_successors(&root, "b", &labels_by_relative_path).unwrap();
    std::fs::remove_dir_all(&root).unwrap();

    assert_eq!(successors, Vec::<String>::new());
}

#[test]
fn link_order_successors_stop_at_a_foreign_include_barrier() {
    let mut labels_by_relative_path = std::collections::HashMap::new();
    labels_by_relative_path.insert("a.inc".to_owned(), "a".to_owned());
    labels_by_relative_path.insert("b.inc".to_owned(), "b".to_owned());
    labels_by_relative_path.insert("c.inc".to_owned(), "c".to_owned());
    let root = std::env::temp_dir().join(format!(
        "pokeemerald-rs-voicegroup-link-order-barrier-test-{}-{}",
        std::process::id(),
        line!()
    ));
    std::fs::create_dir_all(root.join("sound")).unwrap();
    std::fs::write(
        root.join("sound/voice_groups.inc"),
        ".include \"sound/voicegroups/a.inc\"\n.include \"sound/cry_tables.inc\"\n.include \"sound/voicegroups/b.inc\"\n.include \"sound/voicegroups/c.inc\"\n",
    )
    .unwrap();

    let before_barrier = link_order_successors(&root, "a", &labels_by_relative_path).unwrap();
    let after_barrier = link_order_successors(&root, "b", &labels_by_relative_path).unwrap();
    std::fs::remove_dir_all(&root).unwrap();

    assert_eq!(before_barrier, Vec::<String>::new());
    assert_eq!(after_barrier, vec!["c".to_owned()]);
}

#[test]
fn link_order_successors_is_empty_when_top_label_is_unlinked() {
    let labels_by_relative_path = std::collections::HashMap::new();
    let root = std::env::temp_dir().join(format!(
        "pokeemerald-rs-voicegroup-link-order-unlinked-test-{}-{}",
        std::process::id(),
        line!()
    ));
    std::fs::create_dir_all(root.join("sound")).unwrap();
    std::fs::write(root.join("sound/voice_groups.inc"), "").unwrap();

    let successors = link_order_successors(&root, "solo", &labels_by_relative_path).unwrap();
    std::fs::remove_dir_all(&root).unwrap();

    assert_eq!(successors, Vec::<String>::new());
}

#[test]
fn link_order_successors_rejects_an_unindexed_linked_file() {
    let labels_by_relative_path = std::collections::HashMap::new();
    let root = std::env::temp_dir().join(format!(
        "pokeemerald-rs-voicegroup-link-order-unindexed-test-{}-{}",
        std::process::id(),
        line!()
    ));
    std::fs::create_dir_all(root.join("sound")).unwrap();
    std::fs::write(
        root.join("sound/voice_groups.inc"),
        ".include \"sound/voicegroups/ghost.inc\"\n",
    )
    .unwrap();

    let err = link_order_successors(&root, "ghost", &labels_by_relative_path).unwrap_err();
    std::fs::remove_dir_all(&root).unwrap();

    match err {
        crate::extract::ExtractError::VoiceGroup(
            super::VoiceGroupError::UnindexedLinkOrderFile(path),
        ) => assert_eq!(path, "ghost.inc"),
        other => panic!("expected UnindexedLinkOrderFile, got {other:?}"),
    }
}
