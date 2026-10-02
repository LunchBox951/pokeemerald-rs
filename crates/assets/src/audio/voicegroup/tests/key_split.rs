use super::super::*;

const FIRST_KEY_SPLIT_STARTING_NOTE_BYTE: usize = 2;
const FIRST_KEY_SPLIT_TABLE_LENGTH_BYTE: usize = 3;
const FIRST_KEY_SPLIT_TABLE_ENTRY_BYTE: usize = 4;

#[test]
fn key_split_and_rhythm_indirection_round_trip() {
    let group = VoiceGroup::new(vec![
        VoiceEntry::KeySplit(
            KeySplitVoice::new(
                36,
                vec![0, 0, 1, 1, 2],
                VoiceGroupId("audio/voicegroup/trumpet_keysplit".to_owned()),
            )
            .unwrap(),
        ),
        VoiceEntry::Rhythm(RhythmVoice {
            children: VoiceGroupId("audio/voicegroup/emerald_drumset_1".to_owned()),
        }),
    ])
    .unwrap();
    let decoded = VoiceGroup::decode(&group.encode()).unwrap();
    assert_eq!(decoded, group);
    match &decoded.slots()[0] {
        VoiceEntry::KeySplit(v) => assert_eq!(v.table(), [0, 0, 1, 1, 2]),
        other => panic!("expected a KeySplit slot, got {other:?}"),
    }
}

#[test]
fn the_longest_allowed_key_split_table_round_trips() {
    let table: Vec<u8> = (0..VOICE_SLOT_COUNT)
        .map(|i| u8::try_from(i).expect("i < 128"))
        .collect();
    let group = VoiceGroup::new(vec![VoiceEntry::KeySplit(
        KeySplitVoice::new(
            0,
            table.clone(),
            VoiceGroupId("audio/voicegroup/x".to_owned()),
        )
        .unwrap(),
    )])
    .unwrap();
    match &VoiceGroup::decode(&group.encode()).unwrap().slots()[0] {
        VoiceEntry::KeySplit(v) => assert_eq!(v.table(), table.as_slice()),
        other => panic!("expected a KeySplit slot, got {other:?}"),
    }
}

#[test]
fn an_over_long_key_split_table_is_rejected_by_the_constructor() {
    let table = vec![0u8; VOICE_SLOT_COUNT + 1];
    assert_eq!(
        KeySplitVoice::new(0, table, VoiceGroupId("audio/voicegroup/x".to_owned())),
        Err(AudioError::KeySplitTableTooLong(VOICE_SLOT_COUNT + 1))
    );
}

#[test]
fn decode_rejects_a_declared_key_split_table_length_above_the_maximum() {
    let group = VoiceGroup::new(vec![VoiceEntry::KeySplit(
        KeySplitVoice::new(0, vec![0], VoiceGroupId("audio/voicegroup/x".to_owned())).unwrap(),
    )])
    .unwrap();
    let mut bytes = group.encode();
    let invalid_table_len = u8::try_from(VOICE_SLOT_COUNT + 1).unwrap();
    bytes[FIRST_KEY_SPLIT_TABLE_LENGTH_BYTE] = invalid_table_len;
    assert_eq!(
        VoiceGroup::decode(&bytes),
        Err(AudioError::KeySplitTableTooLong(usize::from(
            invalid_table_len
        )))
    );
}

#[test]
fn a_key_split_table_ending_at_the_last_note_is_accepted() {
    let starting_note = u8::try_from(VOICE_SLOT_COUNT - 1).expect("127 fits a u8");
    let group = VoiceGroup::new(vec![VoiceEntry::KeySplit(
        KeySplitVoice::new(
            starting_note,
            vec![0],
            VoiceGroupId("audio/voicegroup/x".to_owned()),
        )
        .unwrap(),
    )])
    .unwrap();
    match &VoiceGroup::decode(&group.encode()).unwrap().slots()[0] {
        VoiceEntry::KeySplit(v) => {
            assert_eq!(v.starting_note, starting_note);
            assert_eq!(v.table(), [0]);
        }
        other => panic!("expected a KeySplit slot, got {other:?}"),
    }
}

#[test]
fn a_key_split_table_extending_past_the_last_note_is_rejected_by_the_constructor() {
    let starting_note = u8::try_from(VOICE_SLOT_COUNT - 1).expect("127 fits a u8");
    assert_eq!(
        KeySplitVoice::new(
            starting_note,
            vec![0, 0],
            VoiceGroupId("audio/voicegroup/x".to_owned()),
        ),
        Err(AudioError::KeySplitTableNoteOutOfRange {
            starting_note,
            table_len: 2,
        })
    );
}

/// `starting_note` is a public field, so a split built in range can be moved
/// past the last note afterwards; the group constructor is the last check
/// before encoding.
#[test]
fn a_group_rejects_a_key_split_moved_past_the_last_note_after_construction() {
    let mut split =
        KeySplitVoice::new(0, vec![0, 0], VoiceGroupId("audio/voicegroup/x".to_owned())).unwrap();
    split.starting_note = u8::try_from(VOICE_SLOT_COUNT - 1).expect("127 fits a u8");
    assert_eq!(
        VoiceGroup::new(vec![VoiceEntry::KeySplit(split)]),
        Err(AudioError::KeySplitTableNoteOutOfRange {
            starting_note: 127,
            table_len: 2,
        })
    );
}

#[test]
fn an_empty_key_split_table_is_accepted_at_any_starting_note() {
    let group = VoiceGroup::new(vec![VoiceEntry::KeySplit(
        KeySplitVoice::new(
            u8::MAX,
            vec![],
            VoiceGroupId("audio/voicegroup/x".to_owned()),
        )
        .unwrap(),
    )])
    .unwrap();
    assert_eq!(VoiceGroup::decode(&group.encode()).unwrap(), group);
}

#[test]
fn decode_rejects_a_key_split_table_extending_past_the_last_note() {
    let group = VoiceGroup::new(vec![VoiceEntry::KeySplit(
        KeySplitVoice::new(0, vec![0, 0], VoiceGroupId("audio/voicegroup/x".to_owned())).unwrap(),
    )])
    .unwrap();
    let mut bytes = group.encode();
    let out_of_range_start = u8::try_from(VOICE_SLOT_COUNT - 1).expect("127 fits a u8");
    bytes[FIRST_KEY_SPLIT_STARTING_NOTE_BYTE] = out_of_range_start;
    assert_eq!(
        VoiceGroup::decode(&bytes),
        Err(AudioError::KeySplitTableNoteOutOfRange {
            starting_note: out_of_range_start,
            table_len: 2,
        })
    );
}

#[test]
fn the_highest_valid_key_split_table_entry_is_accepted() {
    let highest_slot_index = u8::try_from(VOICE_SLOT_COUNT - 1).expect("127 fits a u8");
    let group = VoiceGroup::new(vec![VoiceEntry::KeySplit(
        KeySplitVoice::new(
            0,
            vec![highest_slot_index],
            VoiceGroupId("audio/voicegroup/x".to_owned()),
        )
        .unwrap(),
    )])
    .unwrap();
    match &VoiceGroup::decode(&group.encode()).unwrap().slots()[0] {
        VoiceEntry::KeySplit(v) => assert_eq!(v.table(), [highest_slot_index]),
        other => panic!("expected a KeySplit slot, got {other:?}"),
    }
}

#[test]
fn a_key_split_table_entry_one_past_the_maximum_is_rejected_by_the_constructor() {
    let impossible_slot_index = u8::try_from(VOICE_SLOT_COUNT).expect("128 fits a u8");
    assert_eq!(
        KeySplitVoice::new(
            0,
            vec![impossible_slot_index],
            VoiceGroupId("audio/voicegroup/x".to_owned()),
        ),
        Err(AudioError::KeySplitTableEntryOutOfRange {
            entry_index: 0,
            slot: impossible_slot_index,
        })
    );
}

#[test]
fn a_key_split_table_entry_out_of_range_names_its_own_position() {
    let impossible_slot_index = u8::MAX;
    assert_eq!(
        KeySplitVoice::new(
            0,
            vec![0, 1, impossible_slot_index],
            VoiceGroupId("audio/voicegroup/x".to_owned()),
        ),
        Err(AudioError::KeySplitTableEntryOutOfRange {
            entry_index: 2,
            slot: impossible_slot_index,
        })
    );
}

#[test]
fn decode_rejects_a_key_split_table_entry_above_the_maximum() {
    let group = VoiceGroup::new(vec![VoiceEntry::KeySplit(
        KeySplitVoice::new(0, vec![0], VoiceGroupId("audio/voicegroup/x".to_owned())).unwrap(),
    )])
    .unwrap();
    let mut bytes = group.encode();
    let impossible_slot_index = u8::try_from(VOICE_SLOT_COUNT).expect("128 fits a u8");
    bytes[FIRST_KEY_SPLIT_TABLE_ENTRY_BYTE] = impossible_slot_index;
    assert_eq!(
        VoiceGroup::decode(&bytes),
        Err(AudioError::KeySplitTableEntryOutOfRange {
            entry_index: 0,
            slot: impossible_slot_index,
        })
    );
}
