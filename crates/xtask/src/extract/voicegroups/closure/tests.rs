use std::collections::HashMap;

use super::*;
use crate::extract::voicegroups::parser::{
    DirectSoundMode, Envelope, RawKeySplitTable, RawSlot, RawVoiceGroup,
};

fn envelope() -> Envelope {
    Envelope {
        attack: 255,
        decay: 0,
        sustain: 255,
        release: 0,
    }
}

fn direct_sound(sample: &str) -> RawSlot {
    RawSlot::DirectSound {
        base_key: 60,
        pan: None,
        sample_symbol: format!("DirectSoundWaveData_{sample}"),
        envelope: envelope(),
        mode: DirectSoundMode::Resampled,
    }
}

fn wave(number: u32) -> RawSlot {
    RawSlot::ProgrammableWave {
        base_key: 60,
        length: 0,
        wave_symbol: format!("ProgrammableWaveData_{number}"),
        envelope: envelope(),
        fixed_rate: false,
    }
}

fn rhythm(child: &str) -> RawSlot {
    RawSlot::Rhythm {
        child_label: child.to_owned(),
    }
}

fn raw_group(label: &str, slots: Vec<RawSlot>) -> RawVoiceGroup {
    RawVoiceGroup {
        label: label.to_owned(),
        starting_note: 0,
        slots,
    }
}

fn index(list: Vec<RawVoiceGroup>) -> HashMap<String, RawVoiceGroup> {
    list.into_iter().map(|g| (g.label.clone(), g)).collect()
}

fn linked(labels: &[&str]) -> Vec<IndexedLinkOrderItem> {
    labels
        .iter()
        .map(|label| IndexedLinkOrderItem::VoiceGroup((*label).to_owned()))
        .collect()
}

fn roots(labels: &[&str]) -> Vec<String> {
    labels.iter().map(|label| (*label).to_owned()).collect()
}

fn resolve(
    raw: &HashMap<String, RawVoiceGroup>,
    link_order: &[IndexedLinkOrderItem],
    root_labels: &[&str],
) -> Result<Vec<ResolvedVoiceGroup>, VoiceGroupError> {
    resolve_closure(&roots(root_labels), raw, &HashMap::new(), link_order)
}

fn labels(groups: &[ResolvedVoiceGroup]) -> Vec<&str> {
    groups.iter().map(|group| group.label.as_str()).collect()
}

fn sample_slot(sample: &str) -> VoiceSlot {
    VoiceSlot::DirectSound {
        base_key: 60,
        pan: None,
        sample_id: format!("audio/sample/direct-sound/{sample}"),
        envelope: envelope(),
        mode: DirectSoundMode::Resampled,
    }
}

#[test]
fn every_root_is_emitted_once_in_label_order() {
    let raw = index(vec![
        raw_group("zeta", vec![direct_sound("z")]),
        raw_group("alpha", vec![direct_sound("a")]),
    ]);
    let groups = resolve(&raw, &[], &["zeta", "alpha", "zeta"]).unwrap();
    assert_eq!(labels(&groups), ["alpha", "zeta"]);
}

#[test]
fn a_child_shared_by_two_roots_is_emitted_once() {
    let raw = index(vec![
        raw_group("one", vec![rhythm("drums")]),
        raw_group("two", vec![rhythm("drums")]),
        raw_group("drums", vec![direct_sound("kick")]),
    ]);
    let groups = resolve(&raw, &[], &["one", "two"]).unwrap();
    assert_eq!(labels(&groups), ["drums", "one", "two"]);
}

#[test]
fn a_group_that_is_a_child_of_one_root_and_a_root_itself_keeps_its_borrowed_slots() {
    // `drums` is `one`'s rhythm child, which never borrows, and `two`'s root, which does.
    let raw = index(vec![
        raw_group("one", vec![rhythm("drums")]),
        raw_group("drums", vec![direct_sound("kick")]),
        raw_group("next", vec![direct_sound("snare")]),
    ]);
    let link_order = linked(&["drums", "next"]);
    let as_child_only = resolve(&raw, &link_order, &["one"]).unwrap();
    let both = resolve(&raw, &link_order, &["one", "drums"]).unwrap();

    let child = |groups: &[ResolvedVoiceGroup]| {
        groups
            .iter()
            .find(|group| group.label == "drums")
            .unwrap()
            .slots
            .clone()
    };
    assert_eq!(child(&as_child_only)[1], VoiceSlot::Empty);
    assert_eq!(child(&both)[1], sample_slot("snare"));
    assert_eq!(child(&both)[0], sample_slot("kick"));
}

#[test]
fn the_closure_does_not_depend_on_root_order() {
    let raw = index(vec![
        raw_group("one", vec![rhythm("drums")]),
        raw_group("drums", vec![direct_sound("kick")]),
        raw_group("next", vec![direct_sound("snare")]),
    ]);
    let link_order = linked(&["drums", "next"]);
    assert_eq!(
        resolve(&raw, &link_order, &["one", "drums"]).unwrap(),
        resolve(&raw, &link_order, &["drums", "one"]).unwrap()
    );
}

#[test]
fn a_missing_root_label_fails_closed() {
    let raw = index(vec![raw_group("here", vec![direct_sound("a")])]);
    assert_eq!(
        resolve(&raw, &[], &["here", "gone"]),
        Err(VoiceGroupError::DanglingVoiceGroupReference {
            referrer: "gone".to_owned(),
            target: "gone".to_owned(),
        })
    );
}

#[test]
fn a_missing_child_label_fails_closed() {
    let raw = index(vec![raw_group("top", vec![rhythm("gone")])]);
    assert_eq!(
        resolve(&raw, &[], &["top"]),
        Err(VoiceGroupError::DanglingVoiceGroupReference {
            referrer: "top".to_owned(),
            target: "gone".to_owned(),
        })
    );
}

#[test]
fn a_missing_key_split_table_fails_closed() {
    let raw = index(vec![
        raw_group(
            "top",
            vec![RawSlot::KeySplit {
                child_label: "child".to_owned(),
                table_label: "nowhere".to_owned(),
            }],
        ),
        raw_group("child", vec![direct_sound("a")]),
    ]);
    let tables: HashMap<String, RawKeySplitTable> = HashMap::new();
    assert_eq!(
        resolve_closure(&roots(&["top"]), &raw, &tables, &[]),
        Err(VoiceGroupError::DanglingKeySplitTableReference {
            referrer: "top".to_owned(),
            target: "nowhere".to_owned(),
        })
    );
}

fn group_with_slot(label: &str, slot: VoiceSlot) -> ResolvedVoiceGroup {
    let mut slots = vec![VoiceSlot::Empty; super::super::VOICE_SLOT_COUNT];
    slots[3] = slot;
    ResolvedVoiceGroup {
        label: label.to_owned(),
        slots,
    }
}

#[test]
fn merging_fills_an_empty_slot_and_keeps_an_equal_one() {
    let mut existing = group_with_slot("shared", VoiceSlot::Empty);
    merge_group(
        &mut existing,
        group_with_slot("shared", sample_slot("a")),
        "r",
    )
    .unwrap();
    assert_eq!(existing.slots[3], sample_slot("a"));
    merge_group(
        &mut existing,
        group_with_slot("shared", sample_slot("a")),
        "r",
    )
    .unwrap();
    merge_group(
        &mut existing,
        group_with_slot("shared", VoiceSlot::Empty),
        "r",
    )
    .unwrap();
    assert_eq!(existing.slots[3], sample_slot("a"));
}

#[test]
fn two_different_records_at_one_slot_fail_closed() {
    let mut existing = group_with_slot("shared", sample_slot("a"));
    let error = merge_group(
        &mut existing,
        group_with_slot("shared", sample_slot("b")),
        "late",
    )
    .unwrap_err();
    assert_eq!(
        error,
        VoiceGroupError::ConflictingClosureSlot {
            group: "shared".to_owned(),
            slot: 3,
            root: "late".to_owned(),
        }
    );
    assert_eq!(existing.slots[3], sample_slot("a"));
}

#[test]
fn a_child_edge_outside_the_closure_fails_closed() {
    let orphan = group_with_slot(
        "orphan",
        VoiceSlot::Rhythm {
            children_id: "audio/voicegroup/nowhere".to_owned(),
        },
    );
    assert_eq!(
        check_closed(&[orphan]),
        Err(VoiceGroupError::UnresolvedClosureEdge {
            group: "orphan".to_owned(),
            target: "audio/voicegroup/nowhere".to_owned(),
        })
    );
}

#[test]
fn referenced_samples_are_the_direct_sound_and_wave_ids_without_duplicates() {
    let raw = index(vec![
        raw_group("one", vec![direct_sound("b"), wave(5), direct_sound("a")]),
        raw_group("two", vec![direct_sound("a"), wave(2), rhythm("one")]),
    ]);
    let groups = resolve(&raw, &[], &["two"]).unwrap();
    let samples: Vec<String> = referenced_sample_ids(&groups).into_iter().collect();
    assert_eq!(
        samples,
        [
            "audio/sample/direct-sound/a",
            "audio/sample/direct-sound/b",
            "audio/sample/programmable-wave/02",
            "audio/sample/programmable-wave/05",
        ]
    );
}

mod real_checkout {
    use std::collections::{BTreeMap, BTreeSet};

    use assets::{Sample, Song, VoiceEntry, VoiceGroup};
    use pack_format::PackWriter;

    use crate::extract::{
        audio_samples, midi, repo_root, upstream_present, voicegroups, ExtractError,
    };

    const SONG_PREFIX: &str = "audio/song/";
    const VOICEGROUP_PREFIX: &str = "audio/voicegroup/";
    const SAMPLE_PREFIX: &str = "audio/sample/";
    /// The distinct voicegroups and samples (88 `DirectSound`, 8 programmable-wave) the pinned
    /// checkout's checkpoint songs reach.
    const CHECKPOINT_VOICEGROUP_COUNT: usize = 35;
    const CHECKPOINT_SAMPLE_COUNT: usize = 96;

    /// The audio entries the extractor writes for the checkpoint manifest.
    fn extract_audio_pack() -> Result<Vec<u8>, ExtractError> {
        let upstream = repo_root().join("pokeemerald");
        let mut writer = PackWriter::new();
        let sample_ids = voicegroups::extract_voicegroups(&upstream, &mut writer)?;
        audio_samples::extract_audio_samples(&upstream, &sample_ids, &mut writer)?;
        midi::extract_songs(&upstream, &mut writer)?;
        Ok(writer.finish()?)
    }

    fn payloads(pack: &[u8]) -> BTreeMap<String, &[u8]> {
        pack_format::parse_directory(pack)
            .expect("the extracted audio pack should parse")
            .into_iter()
            .map(|entry| {
                let payload = &pack[entry.offset..entry.offset + entry.length];
                (entry.id, payload)
            })
            .collect()
    }

    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn every_manifest_song_resolves_its_voicegroups_and_samples() {
        assert!(upstream_present(), "run ./init.sh first");
        let pack = extract_audio_pack().expect("the audio pack should extract");
        let entries = payloads(&pack);

        let emitted_groups: BTreeSet<&str> = entries
            .keys()
            .filter(|id| id.starts_with(VOICEGROUP_PREFIX))
            .map(String::as_str)
            .collect();
        assert_eq!(emitted_groups.len(), CHECKPOINT_VOICEGROUP_COUNT);
        let emitted_samples: BTreeSet<&str> = entries
            .keys()
            .filter(|id| id.starts_with(SAMPLE_PREFIX))
            .map(String::as_str)
            .collect();

        assert_eq!(emitted_samples.len(), CHECKPOINT_SAMPLE_COUNT);
        let mut song_groups = BTreeSet::new();
        let mut song_count = 0;
        for (id, bytes) in entries.iter().filter(|(id, _)| id.starts_with(SONG_PREFIX)) {
            let song = Song::decode(bytes).unwrap_or_else(|e| panic!("`{id}` should decode: {e}"));
            song_groups.insert(song.voicegroup().0.clone());
            song_count += 1;
        }
        assert!(song_count > 1, "the manifest songs were not extracted");

        let manifest_roots: BTreeSet<String> =
            midi::manifest_voicegroup_labels(&repo_root().join("pokeemerald"))
                .unwrap()
                .iter()
                .map(|label| format!("{VOICEGROUP_PREFIX}{label}"))
                .collect();
        assert_eq!(song_groups, manifest_roots);

        let mut pending: Vec<String> = song_groups.into_iter().collect();
        let mut reached_groups = BTreeSet::new();
        let mut reached_samples = BTreeSet::new();
        while let Some(group_id) = pending.pop() {
            if !reached_groups.insert(group_id.clone()) {
                continue;
            }
            let bytes = entries
                .get(group_id.as_str())
                .unwrap_or_else(|| panic!("`{group_id}` is referenced but not emitted"));
            let group = VoiceGroup::decode(bytes).unwrap();
            assert_eq!(
                group.slots().len(),
                voicegroups::VOICE_SLOT_COUNT,
                "{group_id}"
            );
            for slot in group.slots() {
                match slot {
                    VoiceEntry::DirectSound(voice) => {
                        reached_samples.insert(voice.sample.0.clone());
                    }
                    VoiceEntry::ProgrammableWave(voice) => {
                        reached_samples.insert(voice.wave.0.clone());
                    }
                    VoiceEntry::KeySplit(voice) => pending.push(voice.children.0.clone()),
                    VoiceEntry::Rhythm(voice) => pending.push(voice.children.0.clone()),
                    _ => {}
                }
            }
        }
        for sample_id in &reached_samples {
            let bytes = entries
                .get(sample_id.as_str())
                .unwrap_or_else(|| panic!("`{sample_id}` is referenced but not emitted"));
            Sample::decode(bytes).unwrap_or_else(|e| panic!("`{sample_id}` should decode: {e}"));
        }

        let reached_groups: BTreeSet<&str> = reached_groups.iter().map(String::as_str).collect();
        let reached_samples: BTreeSet<&str> = reached_samples.iter().map(String::as_str).collect();
        assert_eq!(
            reached_groups, emitted_groups,
            "voicegroups no song reaches"
        );
        assert_eq!(
            reached_samples, emitted_samples,
            "samples no voicegroup reaches"
        );
    }

    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn the_audio_closure_is_deterministic() {
        assert!(upstream_present(), "run ./init.sh first");
        assert_eq!(extract_audio_pack().unwrap(), extract_audio_pack().unwrap());
    }

    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn the_other_songs_leave_the_title_closure_bytes_unchanged() {
        assert!(upstream_present(), "run ./init.sh first");
        let upstream = repo_root().join("pokeemerald");
        let mut title_only = PackWriter::new();
        voicegroups::extract_voicegroup_closure(&upstream, &["title".to_owned()], &mut title_only)
            .unwrap();
        let title_only = title_only.finish().unwrap();
        let everything = extract_audio_pack().unwrap();
        let everything = payloads(&everything);

        let title_only_groups = payloads(&title_only);
        assert_eq!(title_only_groups.len(), 7);
        for (id, bytes) in title_only_groups {
            assert_eq!(
                everything[&id], bytes,
                "`{id}` changed with the wider closure"
            );
        }
    }
}
