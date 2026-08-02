use std::collections::HashMap;

use super::*;
use crate::extract::voicegroups::parser::{DirectSoundMode, Envelope};

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

fn raw_group(label: &str, starting_note: u8, slots: Vec<RawSlot>) -> RawVoiceGroup {
    RawVoiceGroup {
        label: label.to_owned(),
        starting_note,
        slots,
    }
}

fn groups(list: Vec<RawVoiceGroup>) -> HashMap<String, RawVoiceGroup> {
    list.into_iter().map(|g| (g.label.clone(), g)).collect()
}

fn no_keysplit_tables() -> HashMap<String, RawKeySplitTable> {
    HashMap::new()
}

#[test]
fn a_leaf_only_top_level_group_resolves_to_one_group_padded_to_128() {
    let raw = groups(vec![raw_group("solo", 0, vec![direct_sound("a")])]);
    let resolved = resolve_voice_groups("solo", &raw, &no_keysplit_tables()).unwrap();
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].label, "solo");
    assert_eq!(resolved[0].slots.len(), VOICE_SLOT_COUNT);
    assert_eq!(
        resolved[0].slots[0],
        VoiceSlot::DirectSound {
            base_key: 60,
            pan: None,
            sample_id: "audio/sample/direct-sound/a".to_owned(),
            envelope: envelope(),
            mode: DirectSoundMode::Resampled,
        }
    );
    for slot in &resolved[0].slots[1..] {
        assert_eq!(*slot, VoiceSlot::Empty);
    }
}

#[test]
fn a_key_split_slot_resolves_its_table_and_emits_its_child_group() {
    let raw = groups(vec![
        raw_group(
            "top",
            0,
            vec![RawSlot::KeySplit {
                child_label: "child".to_owned(),
                table_label: "demo".to_owned(),
            }],
        ),
        raw_group("child", 0, vec![direct_sound("x")]),
    ]);
    let mut tables = no_keysplit_tables();
    tables.insert(
        "demo".to_owned(),
        RawKeySplitTable {
            starting_note: 36,
            table: vec![0, 0, 1, 1],
        },
    );

    let resolved = resolve_voice_groups("top", &raw, &tables).unwrap();
    assert_eq!(resolved.len(), 2);
    let top = resolved.iter().find(|g| g.label == "top").unwrap();
    assert_eq!(
        top.slots[0],
        VoiceSlot::KeySplit {
            starting_note: 36,
            table: vec![0, 0, 1, 1],
            children_id: "audio/voicegroup/child".to_owned(),
        }
    );
    assert!(resolved.iter().any(|g| g.label == "child"));
}

#[test]
fn a_rhythm_slot_resolves_its_child_group_with_the_starting_note_bias_padded_as_empty() {
    let raw = groups(vec![
        raw_group(
            "top",
            0,
            vec![RawSlot::Rhythm {
                child_label: "drumset".to_owned(),
            }],
        ),
        raw_group(
            "drumset",
            36,
            vec![direct_sound("kick"), direct_sound("snare")],
        ),
    ]);
    let resolved = resolve_voice_groups("top", &raw, &no_keysplit_tables()).unwrap();

    let top = resolved.iter().find(|g| g.label == "top").unwrap();
    assert_eq!(
        top.slots[0],
        VoiceSlot::Rhythm {
            children_id: "audio/voicegroup/drumset".to_owned(),
        }
    );

    let drumset = resolved.iter().find(|g| g.label == "drumset").unwrap();
    assert_eq!(drumset.slots.len(), VOICE_SLOT_COUNT);
    for slot in &drumset.slots[0..36] {
        assert_eq!(
            *slot,
            VoiceSlot::Empty,
            "leading bias slots must stay Empty"
        );
    }
    assert_eq!(
        drumset.slots[36],
        VoiceSlot::DirectSound {
            base_key: 60,
            pan: None,
            sample_id: "audio/sample/direct-sound/kick".to_owned(),
            envelope: envelope(),
            mode: DirectSoundMode::Resampled,
        }
    );
    assert!(matches!(drumset.slots[37], VoiceSlot::DirectSound { .. }));
    // Slot 127 -- the "effective slot 127" every emitted group must
    // explicitly represent (issue #182's 128-slot normalization) -- is
    // trailing padding here, since this synthetic drumset only declares
    // two real entries.
    assert_eq!(drumset.slots[127], VoiceSlot::Empty);
}

#[test]
fn a_chain_of_distinct_groups_is_transitively_resolved() {
    // "top" (the only group allowed key-split/rhythm slots) references
    // "child", a leaf-only group with no further indirection.
    let raw = groups(vec![
        raw_group(
            "top",
            0,
            vec![
                RawSlot::Rhythm {
                    child_label: "child".to_owned(),
                },
                direct_sound("top_own_sample"),
            ],
        ),
        raw_group("child", 0, vec![direct_sound("child_sample")]),
    ]);
    let resolved = resolve_voice_groups("top", &raw, &no_keysplit_tables()).unwrap();
    assert_eq!(resolved.len(), 2);
    assert!(resolved.iter().any(|g| g.label == "top"));
    assert!(resolved.iter().any(|g| g.label == "child"));
}

#[test]
fn a_dangling_voice_group_reference_is_a_hard_error() {
    let raw = groups(vec![raw_group(
        "top",
        0,
        vec![RawSlot::Rhythm {
            child_label: "nowhere".to_owned(),
        }],
    )]);
    let err = resolve_voice_groups("top", &raw, &no_keysplit_tables()).unwrap_err();
    assert_eq!(
        err,
        VoiceGroupError::DanglingVoiceGroupReference {
            referrer: "top".to_owned(),
            target: "nowhere".to_owned(),
        }
    );
}

#[test]
fn a_dangling_top_level_reference_reports_itself_as_referrer() {
    let raw = groups(vec![]);
    let err = resolve_voice_groups("missing", &raw, &no_keysplit_tables()).unwrap_err();
    assert_eq!(
        err,
        VoiceGroupError::DanglingVoiceGroupReference {
            referrer: "missing".to_owned(),
            target: "missing".to_owned(),
        }
    );
}

#[test]
fn a_dangling_key_split_table_reference_is_a_hard_error() {
    let raw = groups(vec![
        raw_group(
            "top",
            0,
            vec![RawSlot::KeySplit {
                child_label: "child".to_owned(),
                table_label: "nowhere".to_owned(),
            }],
        ),
        raw_group("child", 0, vec![direct_sound("x")]),
    ]);
    let err = resolve_voice_groups("top", &raw, &no_keysplit_tables()).unwrap_err();
    assert_eq!(
        err,
        VoiceGroupError::DanglingKeySplitTableReference {
            referrer: "top".to_owned(),
            target: "nowhere".to_owned(),
        }
    );
}

#[test]
fn a_self_referencing_rhythm_slot_is_a_cycle_not_an_infinite_loop() {
    let raw = groups(vec![raw_group(
        "a",
        0,
        vec![RawSlot::Rhythm {
            child_label: "a".to_owned(),
        }],
    )]);
    let err = resolve_voice_groups("a", &raw, &no_keysplit_tables()).unwrap_err();
    assert_eq!(
        err,
        VoiceGroupError::Cycle(vec!["a".to_owned(), "a".to_owned()])
    );
}

#[test]
fn a_child_group_that_itself_carries_an_indirection_slot_is_rejected() {
    // Upstream's `ply_note` aborts a note rather than recursing through a
    // second level of key-split/rhythm indirection -- a child referenced
    // by a top-level group's key-split/rhythm slot must be leaf-only.
    let raw = groups(vec![
        raw_group(
            "top",
            0,
            vec![RawSlot::Rhythm {
                child_label: "child".to_owned(),
            }],
        ),
        raw_group(
            "child",
            0,
            vec![RawSlot::Rhythm {
                child_label: "grandchild".to_owned(),
            }],
        ),
        raw_group("grandchild", 0, vec![direct_sound("x")]),
    ]);
    let err = resolve_voice_groups("top", &raw, &no_keysplit_tables()).unwrap_err();
    assert_eq!(
        err,
        VoiceGroupError::NestedIndirection {
            parent: "child".to_owned(),
            child: "grandchild".to_owned(),
        }
    );
}

#[test]
fn a_starting_note_plus_slot_count_over_128_is_rejected() {
    let slots: Vec<RawSlot> = (0..10).map(|i| direct_sound(&format!("s{i}"))).collect();
    let raw = groups(vec![raw_group("top", 120, slots)]);
    let err = resolve_voice_groups("top", &raw, &no_keysplit_tables()).unwrap_err();
    assert_eq!(
        err,
        VoiceGroupError::TooManySlots {
            group: "top".to_owned(),
            starting_note: 120,
            slot_count: 10,
        }
    );
}

#[test]
fn a_malformed_sample_symbol_is_a_hard_error() {
    let raw = groups(vec![raw_group(
        "top",
        0,
        vec![RawSlot::DirectSound {
            base_key: 60,
            pan: None,
            sample_symbol: "not_a_real_symbol".to_owned(),
            envelope: envelope(),
            mode: DirectSoundMode::Resampled,
        }],
    )]);
    let err = resolve_voice_groups("top", &raw, &no_keysplit_tables()).unwrap_err();
    assert_eq!(
        err,
        VoiceGroupError::MalformedReference {
            group: "top".to_owned(),
            reference: "not_a_real_symbol".to_owned(),
            expected_prefix: "DirectSoundWaveData_",
        }
    );
}

/// The ids here must match `xtask::extract::audio_samples`'s (issue `#183`)
/// byte for byte, or every reference a voicegroup entry carries dangles --
/// see the module docs' "Sample id scheme".
#[test]
fn direct_sound_sample_ids_strip_the_upstream_symbol_prefix() {
    assert_eq!(
        direct_sound_sample_id("DirectSoundWaveData_sc88pro_flute", "g").unwrap(),
        "audio/sample/direct-sound/sc88pro_flute"
    );
}

#[test]
fn programmable_wave_sample_ids_zero_pad_the_symbols_number_to_two_digits() {
    assert_eq!(
        programmable_wave_sample_id("ProgrammableWaveData_2", "g").unwrap(),
        "audio/sample/programmable-wave/02"
    );
    // Already two digits: re-formatting is a no-op, not a second pad.
    assert_eq!(
        programmable_wave_sample_id("ProgrammableWaveData_12", "g").unwrap(),
        "audio/sample/programmable-wave/12"
    );
}

#[test]
fn a_non_numeric_programmable_wave_suffix_is_a_hard_error() {
    // The prefix is right, so `MalformedReference` doesn't fire -- but
    // there is no `<nn>.pcm` source such a symbol could name, so pasting
    // the suffix through would emit an id `#183` never writes.
    let err = programmable_wave_sample_id("ProgrammableWaveData_flute", "g").unwrap_err();
    assert_eq!(
        err,
        VoiceGroupError::MalformedProgrammableWaveIndex {
            group: "g".to_owned(),
            reference: "ProgrammableWaveData_flute".to_owned(),
        }
    );
}

#[test]
fn a_programmable_wave_symbol_without_the_prefix_is_a_hard_error() {
    let err = programmable_wave_sample_id("WaveData_2", "g").unwrap_err();
    assert_eq!(
        err,
        VoiceGroupError::MalformedReference {
            group: "g".to_owned(),
            reference: "WaveData_2".to_owned(),
            expected_prefix: "ProgrammableWaveData_",
        }
    );
}

#[test]
fn each_resolved_group_is_only_emitted_once_even_if_referenced_twice() {
    let raw = groups(vec![
        raw_group(
            "top",
            0,
            vec![
                RawSlot::Rhythm {
                    child_label: "shared".to_owned(),
                },
                RawSlot::KeySplit {
                    child_label: "shared".to_owned(),
                    table_label: "t".to_owned(),
                },
            ],
        ),
        raw_group("shared", 0, vec![direct_sound("x")]),
    ]);
    let mut tables = no_keysplit_tables();
    tables.insert(
        "t".to_owned(),
        RawKeySplitTable {
            starting_note: 0,
            table: vec![0],
        },
    );
    let resolved = resolve_voice_groups("top", &raw, &tables).unwrap();
    assert_eq!(resolved.len(), 2);
    assert_eq!(resolved.iter().filter(|g| g.label == "shared").count(), 1);
}
