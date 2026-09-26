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
    assert_eq!(drumset.slots[127], VoiceSlot::Empty);
}

#[test]
fn a_chain_of_distinct_groups_is_transitively_resolved() {
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
    assert_eq!(
        programmable_wave_sample_id("ProgrammableWaveData_12", "g").unwrap(),
        "audio/sample/programmable-wave/12"
    );
}

#[test]
fn a_non_numeric_programmable_wave_suffix_is_a_hard_error() {
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

#[test]
fn a_link_successor_fills_the_top_level_groups_undeclared_tail() {
    let raw = groups(vec![
        raw_group("top", 0, vec![direct_sound("top_own")]),
        raw_group(
            "next",
            0,
            vec![direct_sound("borrowed"), direct_sound("also_borrowed")],
        ),
    ]);
    let link_successors = vec!["next".to_owned()];
    let resolved = resolve_voice_groups_with_link_successors(
        "top",
        &raw,
        &no_keysplit_tables(),
        &link_successors,
    )
    .unwrap();

    assert_eq!(resolved.len(), 1);
    let top = &resolved[0];
    assert_eq!(
        top.slots[1],
        VoiceSlot::DirectSound {
            base_key: 60,
            pan: None,
            sample_id: "audio/sample/direct-sound/borrowed".to_owned(),
            envelope: envelope(),
            mode: DirectSoundMode::Resampled,
        }
    );
    assert_eq!(
        top.slots[2],
        VoiceSlot::DirectSound {
            base_key: 60,
            pan: None,
            sample_id: "audio/sample/direct-sound/also_borrowed".to_owned(),
            envelope: envelope(),
            mode: DirectSoundMode::Resampled,
        }
    );
    for slot in &top.slots[3..] {
        assert_eq!(*slot, VoiceSlot::Empty);
    }
}

#[test]
fn a_borrowed_indirection_entry_resolves_and_emits_its_child_exactly_like_a_declared_one() {
    let raw = groups(vec![
        raw_group("top", 0, vec![]),
        raw_group(
            "next",
            0,
            vec![RawSlot::Rhythm {
                child_label: "borrowed_child".to_owned(),
            }],
        ),
        raw_group("borrowed_child", 0, vec![direct_sound("hit")]),
    ]);
    let link_successors = vec!["next".to_owned()];
    let resolved = resolve_voice_groups_with_link_successors(
        "top",
        &raw,
        &no_keysplit_tables(),
        &link_successors,
    )
    .unwrap();

    let top = resolved.iter().find(|g| g.label == "top").unwrap();
    assert_eq!(
        top.slots[0],
        VoiceSlot::Rhythm {
            children_id: "audio/voicegroup/borrowed_child".to_owned(),
        }
    );
    assert!(
        resolved.iter().any(|g| g.label == "borrowed_child"),
        "a child referenced only through a borrowed overflow slot must still be emitted"
    );
    assert!(
        !resolved.iter().any(|g| g.label == "next"),
        "the successor file itself is never emitted as its own group"
    );
}

#[test]
fn link_successors_are_walked_in_order_continuing_into_a_second_file_if_the_first_runs_out() {
    let raw = groups(vec![
        raw_group("top", 0, vec![direct_sound("x"); 126]),
        raw_group("first", 0, vec![direct_sound("from_first")]),
        raw_group("second", 0, vec![direct_sound("from_second")]),
    ]);
    let link_successors = vec!["first".to_owned(), "second".to_owned()];
    let resolved = resolve_voice_groups_with_link_successors(
        "top",
        &raw,
        &no_keysplit_tables(),
        &link_successors,
    )
    .unwrap();
    let top = resolved.iter().find(|g| g.label == "top").unwrap();
    match &top.slots[126] {
        VoiceSlot::DirectSound { sample_id, .. } => {
            assert_eq!(sample_id, "audio/sample/direct-sound/from_first");
        }
        other => panic!("expected DirectSound, got {other:?}"),
    }
    match &top.slots[127] {
        VoiceSlot::DirectSound { sample_id, .. } => {
            assert_eq!(sample_id, "audio/sample/direct-sound/from_second");
        }
        other => panic!("expected DirectSound, got {other:?}"),
    }
}

#[test]
fn an_empty_link_successor_list_leaves_the_overflow_tail_empty() {
    let raw = groups(vec![raw_group("top", 0, vec![direct_sound("only_slot")])]);
    let resolved =
        resolve_voice_groups_with_link_successors("top", &raw, &no_keysplit_tables(), &[]).unwrap();
    let top = &resolved[0];
    for slot in &top.slots[1..] {
        assert_eq!(*slot, VoiceSlot::Empty);
    }
}

#[test]
fn a_key_split_rhythm_child_never_gets_link_adjacency_overflow_even_if_under_declared() {
    let raw = groups(vec![
        raw_group(
            "top",
            0,
            vec![RawSlot::Rhythm {
                child_label: "kid".to_owned(),
            }],
        ),
        raw_group("kid", 0, vec![direct_sound("kid_only_slot")]),
        raw_group("successor", 0, vec![direct_sound("must_not_leak_into_kid")]),
    ]);
    let link_successors = vec!["successor".to_owned()];
    let resolved = resolve_voice_groups_with_link_successors(
        "top",
        &raw,
        &no_keysplit_tables(),
        &link_successors,
    )
    .unwrap();
    let kid = resolved.iter().find(|g| g.label == "kid").unwrap();
    for slot in &kid.slots[1..] {
        assert_eq!(*slot, VoiceSlot::Empty);
    }
}

#[test]
fn a_sample_id_too_long_for_the_pack_id_length_field_is_a_hard_error() {
    let raw = groups(vec![raw_group(
        "top",
        0,
        vec![direct_sound(&"x".repeat(usize::from(u16::MAX)))],
    )]);
    let err = resolve_voice_groups("top", &raw, &no_keysplit_tables()).unwrap_err();
    assert_eq!(
        err,
        VoiceGroupError::PackIdTooLong {
            group: "top".to_owned(),
            id_len: "audio/sample/direct-sound/".len() + usize::from(u16::MAX),
        }
    );
}

#[test]
fn a_borrowed_slots_oversize_sample_id_names_the_successor_not_the_borrower() {
    let raw = groups(vec![
        raw_group("top", 0, vec![direct_sound("top_own")]),
        raw_group(
            "next",
            0,
            vec![direct_sound(&"x".repeat(usize::from(u16::MAX)))],
        ),
    ]);
    let link_successors = vec!["next".to_owned()];
    let err = resolve_voice_groups_with_link_successors(
        "top",
        &raw,
        &no_keysplit_tables(),
        &link_successors,
    )
    .unwrap_err();
    assert_eq!(
        err,
        VoiceGroupError::PackIdTooLong {
            group: "next".to_owned(),
            id_len: "audio/sample/direct-sound/".len() + usize::from(u16::MAX),
        }
    );
}

#[test]
fn a_borrowed_indirection_slots_oversize_child_id_names_the_successor_not_the_borrower() {
    let long_child = "c".repeat(usize::from(u16::MAX));
    let raw = groups(vec![
        raw_group("top", 0, vec![]),
        raw_group(
            "next",
            0,
            vec![RawSlot::Rhythm {
                child_label: long_child.clone(),
            }],
        ),
        raw_group(&long_child, 0, vec![direct_sound("hit")]),
    ]);
    let link_successors = vec!["next".to_owned()];
    let err = resolve_voice_groups_with_link_successors(
        "top",
        &raw,
        &no_keysplit_tables(),
        &link_successors,
    )
    .unwrap_err();
    assert_eq!(
        err,
        VoiceGroupError::PackIdTooLong {
            group: "next".to_owned(),
            id_len: "audio/voicegroup/".len() + usize::from(u16::MAX),
        }
    );
}
