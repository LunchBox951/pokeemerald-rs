use super::*;

fn envelope(attack: u8, decay: u8, sustain: u8, release: u8) -> Envelope {
    Envelope {
        attack,
        decay,
        sustain,
        release,
    }
}

#[test]
fn parses_a_simple_group_with_a_leaf_slot_of_every_kind() {
    let text = "\
voice_group demo
\tvoice_directsound 60, 0, DirectSoundWaveData_sc88pro_flute, 255, 0, 255, 165
\tvoice_directsound_no_resample 60, 64, DirectSoundWaveData_bass_drum, 255, 0, 255, 242
\tvoice_directsound_alt 60, 0, DirectSoundWaveData_bicycle_bell, 255, 0, 255, 165
\tvoice_square_1 60, 0, 0, 2, 0, 0, 15, 0
\tvoice_square_1_alt 60, 0, 38, 2, 1, 0, 0, 0
\tvoice_square_2 60, 0, 2, 0, 0, 9, 2
\tvoice_square_2_alt 60, 0, 1, 1, 1, 6, 2
\tvoice_programmable_wave 60, 0, ProgrammableWaveData_1, 0, 7, 15, 1
\tvoice_programmable_wave_alt 60, 0, ProgrammableWaveData_2, 0, 7, 15, 0
\tvoice_noise 60, 0, 0, 0, 0, 15, 0
\tvoice_noise_alt 60, 0, 1, 0, 0, 15, 0
";
    let group = parse_voice_group(text).unwrap();
    assert_eq!(group.label, "demo");
    assert_eq!(group.starting_note, 0);
    assert_eq!(group.slots.len(), 11);

    assert_eq!(
        group.slots[0],
        RawSlot::DirectSound {
            base_key: 60,
            pan: None,
            sample_symbol: "DirectSoundWaveData_sc88pro_flute".to_owned(),
            envelope: envelope(255, 0, 255, 165),
            mode: DirectSoundMode::Resampled,
        }
    );
    match &group.slots[1] {
        RawSlot::DirectSound { pan, mode, .. } => {
            assert_eq!(*pan, Some(64));
            assert!(matches!(mode, DirectSoundMode::Fixed));
        }
        other => panic!("expected DirectSound, got {other:?}"),
    }
    match &group.slots[2] {
        RawSlot::DirectSound { mode, .. } => assert!(matches!(mode, DirectSoundMode::Reverse)),
        other => panic!("expected DirectSound, got {other:?}"),
    }
    match &group.slots[3] {
        RawSlot::Square1 {
            fixed_rate, duty, ..
        } => {
            assert!(!fixed_rate);
            assert_eq!(*duty, 2);
        }
        other => panic!("expected Square1, got {other:?}"),
    }
    match &group.slots[4] {
        RawSlot::Square1 { fixed_rate, .. } => assert!(fixed_rate),
        other => panic!("expected Square1, got {other:?}"),
    }
    match &group.slots[5] {
        RawSlot::Square2 { fixed_rate, .. } => assert!(!fixed_rate),
        other => panic!("expected Square2, got {other:?}"),
    }
    match &group.slots[6] {
        RawSlot::Square2 { fixed_rate, .. } => assert!(fixed_rate),
        other => panic!("expected Square2, got {other:?}"),
    }
    match &group.slots[7] {
        RawSlot::ProgrammableWave {
            wave_symbol,
            fixed_rate,
            ..
        } => {
            assert_eq!(wave_symbol, "ProgrammableWaveData_1");
            assert!(!fixed_rate);
        }
        other => panic!("expected ProgrammableWave, got {other:?}"),
    }
    match &group.slots[9] {
        RawSlot::Noise {
            fixed_rate, period, ..
        } => {
            assert!(!fixed_rate);
            assert_eq!(*period, 0);
        }
        other => panic!("expected Noise, got {other:?}"),
    }
}

#[test]
fn parses_a_starting_note_bias_on_the_declaration_line() {
    let text = "voice_group rs_drumset, 36\n\tvoice_square_1 60, 0, 0, 2, 0, 0, 15, 0\n";
    let group = parse_voice_group(text).unwrap();
    assert_eq!(group.label, "rs_drumset");
    assert_eq!(group.starting_note, 36);
    assert_eq!(group.slots.len(), 1);
}

#[test]
fn parses_a_key_split_slot() {
    let text = "voice_group title\n\tvoice_keysplit voicegroup_piano_keysplit, keysplit_piano\n";
    let group = parse_voice_group(text).unwrap();
    assert_eq!(
        group.slots[0],
        RawSlot::KeySplit {
            child_label: "piano_keysplit".to_owned(),
            table_label: "piano".to_owned(),
        }
    );
}

#[test]
fn parses_a_rhythm_slot() {
    let text = "voice_group title\n\tvoice_keysplit_all voicegroup_rs_drumset\n";
    let group = parse_voice_group(text).unwrap();
    assert_eq!(
        group.slots[0],
        RawSlot::Rhythm {
            child_label: "rs_drumset".to_owned(),
        }
    );
}

#[test]
fn blank_lines_and_comments_are_ignored() {
    let text = "\n@ a comment\nvoice_group demo\n\n@ another comment\n\tvoice_square_1 60, 0, 0, 2, 0, 0, 15, 0\n\n";
    let group = parse_voice_group(text).unwrap();
    assert_eq!(group.label, "demo");
    assert_eq!(group.slots.len(), 1);
}

#[test]
fn a_file_with_no_declaration_is_rejected() {
    let text = "@ just a comment\n\n";
    assert_eq!(
        parse_voice_group(text),
        Err(VoiceGroupError::MissingDeclaration)
    );
}

#[test]
fn a_non_voice_group_first_line_is_rejected() {
    let text = "voice_square_1 60, 0, 0, 2, 0, 0, 15, 0\n";
    assert_eq!(
        parse_voice_group(text),
        Err(VoiceGroupError::MissingDeclaration)
    );
}

#[test]
fn a_bad_starting_note_is_rejected() {
    let text = "voice_group demo, not_a_number\n";
    assert_eq!(
        parse_voice_group(text),
        Err(VoiceGroupError::BadStartingNote)
    );
}

#[test]
fn a_line_with_too_few_operands_is_malformed() {
    let text = "voice_group demo\n\tvoice_square_1 60, 0\n";
    assert_eq!(
        parse_voice_group(text),
        Err(VoiceGroupError::MalformedLine {
            group: "demo".to_owned(),
            line: "voice_square_1 60, 0".to_owned(),
        })
    );
}

#[test]
fn a_non_numeric_operand_is_malformed() {
    let text = "voice_group demo\n\tvoice_square_1 sixty, 0, 0, 2, 0, 0, 15, 0\n";
    assert!(matches!(
        parse_voice_group(text),
        Err(VoiceGroupError::MalformedLine { .. })
    ));
}

#[test]
fn an_unrecognized_macro_is_rejected() {
    let text = "voice_group demo\n\tvoice_bogus 1, 2, 3\n";
    assert_eq!(
        parse_voice_group(text),
        Err(VoiceGroupError::UnknownMacro {
            group: "demo".to_owned(),
            macro_name: "voice_bogus".to_owned(),
        })
    );
}

#[test]
fn a_keysplit_reference_missing_its_prefix_is_rejected() {
    let text = "voice_group demo\n\tvoice_keysplit piano_keysplit, keysplit_piano\n";
    assert_eq!(
        parse_voice_group(text),
        Err(VoiceGroupError::MalformedReference {
            group: "demo".to_owned(),
            reference: "piano_keysplit".to_owned(),
            expected_prefix: "voicegroup_",
        })
    );
}

#[test]
fn a_keysplit_table_reference_missing_its_prefix_is_rejected() {
    let text = "voice_group demo\n\tvoice_keysplit voicegroup_piano_keysplit, piano\n";
    assert_eq!(
        parse_voice_group(text),
        Err(VoiceGroupError::MalformedReference {
            group: "demo".to_owned(),
            reference: "piano".to_owned(),
            expected_prefix: "keysplit_",
        })
    );
}

#[test]
fn a_rhythm_reference_missing_its_prefix_is_rejected() {
    let text = "voice_group demo\n\tvoice_keysplit_all rs_drumset\n";
    assert_eq!(
        parse_voice_group(text),
        Err(VoiceGroupError::MalformedReference {
            group: "demo".to_owned(),
            reference: "rs_drumset".to_owned(),
            expected_prefix: "voicegroup_",
        })
    );
}

// ── keysplit_tables.inc ────────────────────────────────────────────────

const KEYSPLIT_SAMPLE: &str = "\
keysplit piano, 36
\tsplit 0, 55
\tsplit 1, 70
\tsplit 2, 91
\tsplit 3, 108

keysplit tuba, 24
\tsplit 0, 42
\tsplit 1, 108
";

#[test]
fn parses_every_keysplit_block_with_the_documented_starting_note_bias() {
    let tables = parse_keysplit_tables(KEYSPLIT_SAMPLE).unwrap();
    assert_eq!(tables.len(), 2);

    let piano = &tables["piano"];
    assert_eq!(piano.starting_note, 36);
    // 108 - 36 = 72 entries: notes 36..=54 -> 0 (19), 55..=69 -> 1 (15),
    // 70..=90 -> 2 (21), 91..=107 -> 3 (17).
    assert_eq!(piano.table.len(), 72);
    assert_eq!(piano.table[0], 0);
    assert_eq!(piano.table[18], 0);
    assert_eq!(piano.table[19], 1);
    assert_eq!(piano.table[33], 1);
    assert_eq!(piano.table[34], 2);
    assert_eq!(piano.table[54], 2);
    assert_eq!(piano.table[55], 3);
    assert_eq!(piano.table[71], 3);

    let tuba = &tables["tuba"];
    assert_eq!(tuba.starting_note, 24);
    assert_eq!(tuba.table.len(), 84);
    assert_eq!(tuba.table[0], 0);
    assert_eq!(tuba.table[17], 0);
    assert_eq!(tuba.table[18], 1);
    assert_eq!(tuba.table[83], 1);
}

#[test]
fn a_keysplit_block_with_no_starting_note_defaults_to_zero() {
    let text = "keysplit demo\n\tsplit 0, 5\n";
    let tables = parse_keysplit_tables(text).unwrap();
    let demo = &tables["demo"];
    assert_eq!(demo.starting_note, 0);
    assert_eq!(demo.table, vec![0, 0, 0, 0, 0]);
}

#[test]
fn a_split_before_any_keysplit_declaration_is_rejected() {
    let text = "\tsplit 0, 5\n";
    assert_eq!(
        parse_keysplit_tables(text),
        Err(VoiceGroupError::SplitOutsideKeySplit)
    );
}

#[test]
fn a_split_earlier_than_the_running_cursor_is_rejected() {
    let text = "keysplit demo, 36\n\tsplit 0, 40\n\tsplit 1, 38\n";
    assert_eq!(
        parse_keysplit_tables(text),
        Err(VoiceGroupError::SplitOutOfOrder("demo".to_owned()))
    );
}

#[test]
fn a_duplicate_keysplit_label_is_rejected() {
    let text = "keysplit demo, 0\n\tsplit 0, 1\nkeysplit demo, 0\n\tsplit 0, 1\n";
    assert_eq!(
        parse_keysplit_tables(text),
        Err(VoiceGroupError::DuplicateKeySplitTable("demo".to_owned()))
    );
}

#[test]
fn a_keysplit_header_missing_its_label_is_rejected() {
    let text = "keysplit\n\tsplit 0, 1\n";
    assert_eq!(
        parse_keysplit_tables(text),
        Err(VoiceGroupError::MalformedKeySplitTableHeader)
    );
}

#[test]
fn an_unrecognized_keysplit_macro_is_rejected() {
    // Crafted fixture: the real `keysplit_tables.inc` contains only
    // `keysplit`/`split` lines, but a silently-skipped third macro would be
    // a silently-truncated table, so this fails closed the same way
    // `parse_slot_line`'s `UnknownMacro` does.
    let text = "keysplit demo, 0\n\tsplit 0, 4\n\tsplit_all 1\n";
    assert_eq!(
        parse_keysplit_tables(text),
        Err(VoiceGroupError::UnknownKeySplitMacro(
            "split_all".to_owned()
        ))
    );
}

#[test]
fn a_keysplit_table_longer_than_128_entries_is_rejected() {
    // One `split` spanning notes 0..200 expands to 200 table entries, past
    // the 128 slots any table entry can address (`VOICE_SLOT_COUNT`).
    let text = "keysplit demo, 0\n\tsplit 0, 200\n";
    assert_eq!(
        parse_keysplit_tables(text),
        Err(VoiceGroupError::KeySplitTableTooLong {
            label: "demo".to_owned(),
            len: 200,
        })
    );
}

#[test]
fn parse_link_order_keeps_only_voicegroups_includes_in_file_order() {
    // Mirrors `sound/voice_groups.inc:66-67,136`'s own shape: a comment
    // line, two voicegroup includes back to back, then an unrelated
    // `sound/cry_tables.inc` include that must not appear in the output.
    let text = "\
@ drumsets
.include \"sound/voicegroups/title.inc\"
.include \"sound/voicegroups/intro.inc\"
.include \"sound/cry_tables.inc\"
.include \"sound/voicegroups/drumsets/rs.inc\" @ trailing comment
";
    assert_eq!(
        parse_link_order(text),
        vec![
            "title.inc".to_owned(),
            "intro.inc".to_owned(),
            "drumsets/rs.inc".to_owned(),
        ]
    );
}

#[test]
fn parse_link_order_on_empty_text_is_empty() {
    assert_eq!(parse_link_order(""), Vec::<String>::new());
}
