//! Extracts the voicegroups used by `MUS_TITLE` into the asset pack.
//!
//! `sound/songs/midi/midi.cfg` assigns the song to `voicegroup_title`, whose
//! source declares 89 of 128 addressable slots. `sound/voice_groups.inc` links
//! `intro.inc` immediately after `title.inc`, and the unchecked lookup in
//! `src/m4a_1.s::ply_voice` reads the undeclared title slots from that adjacent
//! data. The root group therefore borrows known linked-successor slots. A
//! foreign include, missing root, final root, or exhausted successor stops the
//! borrowing and leaves the remaining slots empty `(behavioral-fidelity)`.
//!
//! All voicegroup sources are parsed and indexed, so malformed files and
//! duplicate labels fail extraction. Traversing the title group rejects
//! dangling labels in its transitive key-split and rhythm dependencies; only
//! those reachable groups are emitted. Sample payload extraction belongs to
//! `super::audio_samples`.

mod encode;
mod error;
mod parser;
mod resolve;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::pack::{PackEntry, PackKind, PackWriter};
use super::{read_text, ExtractError};
use parser::RawVoiceGroup;

pub(crate) use error::VoiceGroupError;

/// Number of addressable slots in the asset-pack voicegroup schema.
pub(super) const VOICE_SLOT_COUNT: usize = 128;

const TITLE_VOICEGROUP_LABEL: &str = "title";

#[derive(Debug)]
struct VoiceGroupSourceIndex {
    groups_by_label: HashMap<String, RawVoiceGroup>,
    labels_by_relative_path: HashMap<String, String>,
}

enum IndexedLinkOrderItem {
    VoiceGroup(String),
    ForeignInclude,
}

fn discover_voicegroup_sources(dir: &Path) -> Result<Vec<PathBuf>, ExtractError> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                walk(&path, out)?;
            } else if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("inc"))
            {
                out.push(path);
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(dir, &mut out).map_err(|e| ExtractError::ReadFailed(dir.to_path_buf(), e.to_string()))?;
    out.sort();
    Ok(out)
}

fn index_voicegroup_sources(upstream: &Path) -> Result<VoiceGroupSourceIndex, ExtractError> {
    let source_root = upstream.join("sound/voicegroups");
    let mut groups_by_label = HashMap::new();
    let mut source_paths_by_label = HashMap::new();
    let mut labels_by_relative_path = HashMap::new();

    for path in discover_voicegroup_sources(&source_root)? {
        let text = read_text(&path)?;
        let raw = parser::parse_voice_group(&text)
            .map_err(|e| ExtractError::VoiceGroupFile(path.clone(), e))?;
        if let Some(first_path) = source_paths_by_label.insert(raw.label.clone(), path.clone()) {
            return Err(ExtractError::DuplicateVoiceGroupLabel {
                label: raw.label,
                first_path,
                second_path: path,
            });
        }
        let relative = path
            .strip_prefix(&source_root)
            .expect("discovered voicegroup source must be under its source root")
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        labels_by_relative_path.insert(relative, raw.label.clone());
        groups_by_label.insert(raw.label.clone(), raw);
    }
    Ok(VoiceGroupSourceIndex {
        groups_by_label,
        labels_by_relative_path,
    })
}

fn link_order_successors(
    upstream: &Path,
    root_label: &str,
    labels_by_relative_path: &HashMap<String, String>,
) -> Result<Vec<String>, ExtractError> {
    let path = upstream.join("sound/voice_groups.inc");
    let text = read_text(&path)?;

    let mut link_order = Vec::new();
    for item in parser::parse_link_order(&text) {
        match item {
            parser::LinkOrderItem::VoiceGroup(relative) => {
                let label = labels_by_relative_path.get(&relative).ok_or_else(|| {
                    ExtractError::VoiceGroup(VoiceGroupError::UnindexedLinkOrderFile(
                        relative.clone(),
                    ))
                })?;
                link_order.push(IndexedLinkOrderItem::VoiceGroup(label.clone()));
            }
            parser::LinkOrderItem::Foreign => link_order.push(IndexedLinkOrderItem::ForeignInclude),
        }
    }

    let Some(index) = link_order.iter().position(
        |item| matches!(item, IndexedLinkOrderItem::VoiceGroup(label) if label == root_label),
    ) else {
        return Ok(Vec::new());
    };
    Ok(link_order
        .into_iter()
        .skip(index + 1)
        .map_while(|item| match item {
            IndexedLinkOrderItem::VoiceGroup(label) => Some(label),
            IndexedLinkOrderItem::ForeignInclude => None,
        })
        .collect())
}

/// Extracts `MUS_TITLE`'s transitive voicegroup dependencies.
pub(super) fn extract_voicegroups(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let source_index = index_voicegroup_sources(upstream)?;
    let link_successors = link_order_successors(
        upstream,
        TITLE_VOICEGROUP_LABEL,
        &source_index.labels_by_relative_path,
    )?;

    let keysplit_path = upstream.join("sound/keysplit_tables.inc");
    let keysplit_text = read_text(&keysplit_path)?;
    let keysplit_tables = parser::parse_keysplit_tables(&keysplit_text)
        .map_err(|e| ExtractError::VoiceGroupFile(keysplit_path, e))?;

    let resolved_groups = resolve::resolve_voice_groups_with_link_successors(
        TITLE_VOICEGROUP_LABEL,
        &source_index.groups_by_label,
        &keysplit_tables,
        &link_successors,
    )
    .map_err(ExtractError::VoiceGroup)?;

    for group in &resolved_groups {
        writer.push(PackEntry {
            id: resolve::voice_group_pack_id(&group.label),
            kind: PackKind::Raw,
            payload: encode::encode_voice_group(group),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        discover_voicegroup_sources, extract_voicegroups, index_voicegroup_sources,
        link_order_successors, TITLE_VOICEGROUP_LABEL,
    };
    use crate::extract::pack::PackWriter;

    const REFERENCE_VOICEGROUP_SOURCE_COUNT: usize = 195;
    const TITLE_DECLARED_SLOT_COUNT: usize = 89;
    const RS_DRUMSET_FIRST_DECLARED_SLOT: usize = 36;
    const RS_DRUMSET_DECLARED_SLOT_COUNT: usize = 29;
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
    fn mus_titles_full_dependency_tree_resolves_to_expected_groups() {
        assert!(super::super::upstream_present(), "run ./init.sh first");
        let upstream = super::super::repo_root().join("pokeemerald");
        let index = index_voicegroup_sources(&upstream).unwrap();
        let link_successors = link_order_successors(
            &upstream,
            TITLE_VOICEGROUP_LABEL,
            &index.labels_by_relative_path,
        )
        .unwrap();
        let keysplit_text =
            std::fs::read_to_string(upstream.join("sound/keysplit_tables.inc")).unwrap();
        let keysplit_tables = super::parser::parse_keysplit_tables(&keysplit_text).unwrap();

        let groups = super::resolve::resolve_voice_groups_with_link_successors(
            TITLE_VOICEGROUP_LABEL,
            &index.groups_by_label,
            &keysplit_tables,
            &link_successors,
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
        let after_last_declared_slot =
            RS_DRUMSET_FIRST_DECLARED_SLOT + RS_DRUMSET_DECLARED_SLOT_COUNT;
        for slot in &rs_drumset.slots[..RS_DRUMSET_FIRST_DECLARED_SLOT] {
            assert_eq!(*slot, super::resolve::VoiceSlot::Empty);
        }
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
        extract_voicegroups(&upstream, &mut writer)
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
}
