use super::*;

const FIRST_ID_BYTE: usize = size_of::<u16>();
const EMPTY_ID_SONG_METADATA_BYTES: usize = size_of::<u16>() + 4;
const FIRST_EVENT_TAG_BYTE: usize = EMPTY_ID_SONG_METADATA_BYTES + size_of::<u32>();
const FIRST_EVENT_OPERAND_BYTE: usize = FIRST_EVENT_TAG_BYTE + 1;

fn sample_track() -> Vec<SongEvent> {
    vec![
        SongEvent::Voice(0),
        SongEvent::Volume(127),
        SongEvent::Note {
            key: 60,
            velocity: 127,
            gate: 24,
        },
        SongEvent::Wait(24),
        SongEvent::Note {
            key: 62,
            velocity: 100,
            gate: 0,
        },
        SongEvent::EndOfTie { key: None },
        SongEvent::Fine,
    ]
}

fn song(tracks: Vec<Vec<SongEvent>>) -> Song {
    Song::new(
        VoiceGroupId("audio/voicegroup/title".to_owned()),
        0,
        None,
        tracks,
    )
    .expect("test song fits schema bounds")
}

fn single_event_bytes(event: SongEvent) -> Vec<u8> {
    Song::new(VoiceGroupId(String::new()), 0, None, vec![vec![event]])
        .expect("single-event test song fits schema bounds")
        .encode()
}

fn overwrite_trailing_u32(bytes: &mut [u8], value: u32) {
    let start = bytes.len() - size_of::<u32>();
    bytes[start..].copy_from_slice(&value.to_le_bytes());
}

#[test]
fn a_typical_song_round_trips() {
    let song = Song::new(
        VoiceGroupId("audio/voicegroup/title".to_owned()),
        0,
        Some(80),
        vec![sample_track(), sample_track()],
    )
    .unwrap();
    let bytes = song.encode();
    assert_eq!(Song::decode(&bytes).unwrap(), song);
}

#[test]
fn no_reverb_override_round_trips() {
    let song = Song::new(
        VoiceGroupId("audio/voicegroup/dummy".to_owned()),
        5,
        None,
        vec![vec![SongEvent::Fine]],
    )
    .unwrap();
    let bytes = song.encode();
    let decoded = Song::decode(&bytes).unwrap();
    assert_eq!(decoded, song);
    assert_eq!(decoded.reverb(), None);
    assert_eq!(decoded.priority(), 5);
    assert_eq!(
        decoded.voicegroup(),
        &VoiceGroupId("audio/voicegroup/dummy".to_owned())
    );
}

#[test]
fn every_event_variant_round_trips_with_full_width_controller_values() {
    let track = vec![
        SongEvent::Wait(u8::MAX),
        SongEvent::Note {
            key: u8::MIN,
            velocity: u8::MIN,
            gate: u8::MIN,
        },
        SongEvent::Note {
            key: u8::MAX,
            velocity: u8::MAX,
            gate: u8::MAX,
        },
        SongEvent::EndOfTie { key: None },
        SongEvent::EndOfTie { key: Some(u8::MAX) },
        SongEvent::Voice(u8::MIN),
        SongEvent::Voice(u8::MAX),
        SongEvent::Volume(u8::MIN),
        SongEvent::Volume(u8::MAX),
        SongEvent::Pan(i8::MIN),
        SongEvent::Pan(i8::MAX),
        SongEvent::Bend(i8::MIN),
        SongEvent::Bend(i8::MAX),
        SongEvent::BendRange(u8::MIN),
        SongEvent::BendRange(u8::MAX),
        SongEvent::Tune(i8::MIN),
        SongEvent::Tune(i8::MAX),
        SongEvent::KeyShift(i8::MIN),
        SongEvent::KeyShift(i8::MAX),
        SongEvent::Tempo(u16::MIN),
        SongEvent::Tempo(u16::MAX),
        SongEvent::Priority(u8::MIN),
        SongEvent::Priority(u8::MAX),
        SongEvent::LfoSpeed(u8::MIN),
        SongEvent::LfoSpeed(u8::MAX),
        SongEvent::LfoDelay(u8::MIN),
        SongEvent::LfoDelay(u8::MAX),
        SongEvent::Modulation(u8::MIN),
        SongEvent::Modulation(u8::MAX),
        SongEvent::ModType(u8::MIN),
        SongEvent::ModType(u8::MAX),
        SongEvent::PseudoEchoVolume(u8::MIN),
        SongEvent::PseudoEchoVolume(u8::MAX),
        SongEvent::PseudoEchoLength(u8::MIN),
        SongEvent::PseudoEchoLength(u8::MAX),
        SongEvent::MemAcc {
            op: MemAccOp::Set,
            address: u8::MIN,
            data: u8::MAX,
        },
        SongEvent::MemAccBranch {
            condition: MemAccCondition::Eq,
            address: u8::MAX,
            data: u8::MIN,
            target: 0,
        },
        SongEvent::Goto(0),
        SongEvent::Fine,
    ];
    let song = Song::new(
        VoiceGroupId("audio/voicegroup/vs_rayquaza".to_owned()),
        1,
        Some(0),
        vec![track],
    )
    .unwrap();
    let bytes = song.encode();
    assert_eq!(Song::decode(&bytes).unwrap(), song);
}

#[test]
fn event_tags_preserve_the_append_only_wire_identities() {
    let tags = [
        EventTag::Wait,
        EventTag::Note,
        EventTag::EndOfTie,
        EventTag::Voice,
        EventTag::Volume,
        EventTag::Pan,
        EventTag::Bend,
        EventTag::BendRange,
        EventTag::Tune,
        EventTag::KeyShift,
        EventTag::Tempo,
        EventTag::Priority,
        EventTag::LfoSpeed,
        EventTag::LfoDelay,
        EventTag::Modulation,
        EventTag::ModType,
        EventTag::Goto,
        EventTag::Fine,
        EventTag::PseudoEchoVolume,
        EventTag::PseudoEchoLength,
        EventTag::MemAcc,
        EventTag::MemAccBranch,
    ];
    assert_eq!(tags.map(EventTag::byte), (0..=21).collect::<Vec<_>>()[..]);
}

#[test]
fn the_pseudo_echo_pair_round_trips_at_both_ends_of_its_range() {
    let track = vec![
        SongEvent::PseudoEchoVolume(0),
        SongEvent::PseudoEchoVolume(255),
        SongEvent::PseudoEchoLength(0),
        SongEvent::PseudoEchoLength(255),
        SongEvent::Fine,
    ];
    let song = song(vec![track.clone()]);
    let decoded = Song::decode(&song.encode()).unwrap();
    assert_eq!(decoded.tracks()[0], track);
}

#[test]
fn the_pseudo_echo_pair_survives_alongside_the_notes_it_decorates() {
    let track = vec![
        SongEvent::Voice(60),
        SongEvent::PseudoEchoVolume(80),
        SongEvent::PseudoEchoLength(12),
        SongEvent::Note {
            key: 60,
            velocity: 100,
            gate: 24,
        },
        SongEvent::Wait(24),
        SongEvent::Fine,
    ];
    let song = song(vec![track.clone()]);
    assert_eq!(Song::decode(&song.encode()).unwrap().tracks()[0], track);
}

#[test]
fn voice_command_selects_the_highest_slot_127() {
    let song = song(vec![vec![SongEvent::Voice(127), SongEvent::Fine]]);
    let decoded = Song::decode(&song.encode()).unwrap();
    assert_eq!(decoded.tracks()[0][0], SongEvent::Voice(127));
}

#[test]
fn tempo_is_carried_as_a_track_event() {
    let song = song(vec![vec![
        SongEvent::Tempo(144),
        SongEvent::Wait(1),
        SongEvent::Fine,
    ]]);
    let decoded = Song::decode(&song.encode()).unwrap();
    assert_eq!(decoded.tracks()[0][0], SongEvent::Tempo(144));
}

#[test]
fn empty_track_list_round_trips() {
    let song = song(vec![]);
    let bytes = song.encode();
    assert_eq!(Song::decode(&bytes).unwrap(), song);
    assert!(song.tracks().is_empty());
}

#[test]
fn the_maximum_track_count_round_trips() {
    let tracks = vec![vec![SongEvent::Fine]; MAX_TRACKS];
    let song = song(tracks);
    assert_eq!(song.tracks().len(), MAX_TRACKS);
    assert_eq!(Song::decode(&song.encode()).unwrap(), song);
}

#[test]
fn too_many_tracks_is_rejected_by_the_constructor() {
    let tracks = vec![vec![SongEvent::Fine]; MAX_TRACKS + 1];
    assert_eq!(
        Song::new(VoiceGroupId("x".to_owned()), 0, None, tracks),
        Err(AudioError::TooManyTracks(MAX_TRACKS + 1))
    );
}

#[test]
fn an_oversize_voicegroup_id_is_rejected_by_the_constructor() {
    let id = VoiceGroupId("x".repeat(usize::from(u16::MAX) + 1));
    assert_eq!(
        Song::new(id, 0, None, vec![]),
        Err(AudioError::IdTooLong(usize::from(u16::MAX) + 1))
    );
}

#[test]
fn the_longest_encodable_voicegroup_id_is_accepted() {
    let id = VoiceGroupId("x".repeat(usize::from(u16::MAX)));
    let song = Song::new(id.clone(), 0, None, vec![]).unwrap();
    assert_eq!(Song::decode(&song.encode()).unwrap().voicegroup(), &id);
}

#[test]
fn unknown_event_tag_is_rejected() {
    let mut bytes = single_event_bytes(SongEvent::Fine);
    bytes[FIRST_EVENT_TAG_BYTE] = u8::MAX;
    assert_eq!(
        Song::decode(&bytes),
        Err(AudioError::UnknownSongEvent(u8::MAX))
    );
}

#[test]
fn a_tag_just_past_the_last_defined_one_is_rejected() {
    let mut bytes = single_event_bytes(SongEvent::Fine);
    let unknown_tag = EventTag::MemAccBranch.byte() + 1;
    bytes[FIRST_EVENT_TAG_BYTE] = unknown_tag;
    assert_eq!(
        Song::decode(&bytes),
        Err(AudioError::UnknownSongEvent(unknown_tag))
    );
}

#[test]
fn the_memacc_conditional_loop_round_trips() {
    let final_event = 4;
    let track = vec![
        SongEvent::MemAcc {
            op: MemAccOp::Set,
            address: 0,
            data: 0,
        },
        SongEvent::Note {
            key: 60,
            velocity: 100,
            gate: 24,
        },
        SongEvent::MemAccBranch {
            condition: MemAccCondition::Eq,
            address: 0,
            data: 1,
            target: final_event,
        },
        SongEvent::Goto(1),
        SongEvent::Fine,
    ];
    let song = Song::new(
        VoiceGroupId("audio/voicegroup/vs_trainer".to_owned()),
        0,
        Some(50),
        vec![track],
    )
    .unwrap();
    assert_eq!(Song::decode(&song.encode()).unwrap(), song);
}

#[test]
fn every_memacc_op_and_condition_round_trips_on_its_upstream_byte() {
    let ops = [
        (MemAccOp::Set, 0u8),
        (MemAccOp::Add, 1),
        (MemAccOp::Sub, 2),
        (MemAccOp::MemSet, 3),
        (MemAccOp::MemAdd, 4),
        (MemAccOp::MemSub, 5),
    ];
    let conditions = [
        (MemAccCondition::Eq, 6u8),
        (MemAccCondition::Ne, 7),
        (MemAccCondition::Hi, 8),
        (MemAccCondition::Hs, 9),
        (MemAccCondition::Ls, 10),
        (MemAccCondition::Lo, 11),
        (MemAccCondition::MemEq, 12),
        (MemAccCondition::MemNe, 13),
        (MemAccCondition::MemHi, 14),
        (MemAccCondition::MemHs, 15),
        (MemAccCondition::MemLs, 16),
        (MemAccCondition::MemLo, 17),
    ];
    let mut track = Vec::new();
    for (op, byte) in ops {
        assert_eq!(op as u8, byte, "{op:?}: upstream mem_* value");
        track.push(SongEvent::MemAcc {
            op,
            address: byte,
            data: byte.wrapping_add(1),
        });
    }
    for (condition, byte) in conditions {
        assert_eq!(condition as u8, byte, "{condition:?}: upstream mem_* value");
        track.push(SongEvent::MemAccBranch {
            condition,
            address: byte,
            data: byte.wrapping_add(1),
            target: u32::from(byte),
        });
    }
    track.push(SongEvent::Fine);
    let song = song(vec![track]);
    assert_eq!(Song::decode(&song.encode()).unwrap(), song);
}

#[test]
fn an_out_of_range_memacc_op_byte_is_rejected() {
    let mut bytes = single_event_bytes(SongEvent::MemAcc {
        op: MemAccOp::Set,
        address: 0,
        data: 0,
    });
    let unknown_memacc_tag = 18;
    bytes[FIRST_EVENT_OPERAND_BYTE] = unknown_memacc_tag;
    assert_eq!(
        Song::decode(&bytes),
        Err(AudioError::UnknownMemAccOp(unknown_memacc_tag))
    );
}

#[test]
fn decode_rejects_trailing_bytes() {
    let mut bytes = song(vec![sample_track()]).encode();
    bytes.push(0);
    assert_eq!(Song::decode(&bytes), Err(AudioError::TrailingBytes(1)));
}

#[test]
fn a_non_utf8_voicegroup_id_is_rejected() {
    let mut bytes = Song::new(VoiceGroupId("x".to_owned()), 0, None, vec![])
        .unwrap()
        .encode();
    bytes[FIRST_ID_BYTE] = u8::MAX;
    assert_eq!(Song::decode(&bytes), Err(AudioError::InvalidString));
}

#[test]
fn truncated_input_is_rejected() {
    let bytes = Song::new(
        VoiceGroupId("audio/voicegroup/title".to_owned()),
        3,
        Some(10),
        vec![sample_track()],
    )
    .unwrap()
    .encode();
    for cut in 0..bytes.len() {
        assert!(Song::decode(&bytes[..cut]).is_err());
    }
}

mod canonical_waits {
    use super::*;

    fn canon(track: Vec<SongEvent>) -> Vec<SongEvent> {
        song(vec![track]).tracks()[0].clone()
    }

    #[test]
    fn adjacent_waits_merge() {
        assert_eq!(
            canon(vec![
                SongEvent::Wait(96),
                SongEvent::Wait(4),
                SongEvent::Fine
            ]),
            vec![SongEvent::Wait(100), SongEvent::Fine]
        );
    }

    #[test]
    fn a_long_rest_splits_greedily_with_the_remainder_last() {
        assert_eq!(
            canon(vec![
                SongEvent::Wait(96),
                SongEvent::Wait(96),
                SongEvent::Wait(96),
                SongEvent::Wait(48),
            ]),
            vec![SongEvent::Wait(255), SongEvent::Wait(81)]
        );
        assert_eq!(
            canon(vec![SongEvent::Wait(255), SongEvent::Wait(255)]),
            vec![SongEvent::Wait(255), SongEvent::Wait(255)]
        );
    }

    #[test]
    fn a_zero_rest_vanishes() {
        assert_eq!(
            canon(vec![
                SongEvent::Wait(0),
                SongEvent::Voice(1),
                SongEvent::Wait(0)
            ]),
            vec![SongEvent::Voice(1)]
        );
    }

    #[test]
    fn a_canonical_track_is_unchanged() {
        let track = sample_track();
        assert_eq!(canon(track.clone()), track);
    }

    #[test]
    fn jump_targets_follow_the_events_they_name() {
        let voice_before_canonicalization = 4;
        let voice_after_canonicalization = 2;
        let track = vec![
            SongEvent::Voice(0),
            SongEvent::Wait(10),
            SongEvent::Wait(10),
            SongEvent::Wait(10),
            SongEvent::Voice(1),
            SongEvent::Goto(voice_before_canonicalization),
        ];
        assert_eq!(
            canon(track),
            vec![
                SongEvent::Voice(0),
                SongEvent::Wait(30),
                SongEvent::Voice(1),
                SongEvent::Goto(voice_after_canonicalization),
            ]
        );
    }

    #[test]
    fn a_run_does_not_merge_across_a_jump_target() {
        let track = vec![
            SongEvent::Wait(10),
            SongEvent::Wait(20),
            SongEvent::MemAccBranch {
                condition: MemAccCondition::Eq,
                address: 0,
                data: 0,
                target: 1,
            },
            SongEvent::Goto(1),
        ];
        assert_eq!(canon(track.clone()), track);
    }

    #[test]
    fn a_target_past_the_end_is_left_alone() {
        assert_eq!(
            super::super::canonical::canonicalize_waits(&[SongEvent::Goto(99)]),
            vec![SongEvent::Goto(99)]
        );
    }

    #[test]
    fn a_target_at_the_end_stays_at_the_end() {
        assert_eq!(
            canon(vec![
                SongEvent::Wait(1),
                SongEvent::Wait(1),
                SongEvent::Goto(2)
            ]),
            vec![SongEvent::Wait(2), SongEvent::Goto(1)]
        );
    }
}

#[test]
fn the_constructor_rejects_a_jump_target_at_or_past_the_track_length() {
    for target in [2u32, 3, u32::MAX] {
        let goto_track = vec![
            SongEvent::Note {
                key: 60,
                velocity: 127,
                gate: 24,
            },
            SongEvent::Goto(target),
        ];
        let branch_track = vec![
            SongEvent::Note {
                key: 60,
                velocity: 127,
                gate: 24,
            },
            SongEvent::MemAccBranch {
                condition: MemAccCondition::Eq,
                address: 0,
                data: 1,
                target,
            },
        ];
        for track in [goto_track, branch_track] {
            let event_count = u32::try_from(track.len()).unwrap();
            assert_eq!(
                Song::new(
                    VoiceGroupId("audio/voicegroup/title".to_owned()),
                    0,
                    None,
                    vec![track],
                ),
                Err(AudioError::JumpTargetOutOfRange {
                    track_index: 0,
                    event_index: 1,
                    target,
                    event_count,
                })
            );
        }
    }
}

#[test]
fn decode_rejects_a_goto_target_past_the_decoded_track_length() {
    let mut bytes = song(vec![vec![
        SongEvent::Note {
            key: 60,
            velocity: 100,
            gate: 24,
        },
        SongEvent::Goto(0),
    ]])
    .encode();
    overwrite_trailing_u32(&mut bytes, 9_999);
    assert_eq!(
        Song::decode(&bytes),
        Err(AudioError::JumpTargetOutOfRange {
            track_index: 0,
            event_index: 1,
            target: 9_999,
            event_count: 2,
        })
    );
}

#[test]
fn decode_rejects_a_memaccbranch_target_past_the_decoded_track_length() {
    let mut bytes = song(vec![vec![
        SongEvent::Note {
            key: 60,
            velocity: 100,
            gate: 24,
        },
        SongEvent::MemAccBranch {
            condition: MemAccCondition::Eq,
            address: 0,
            data: 1,
            target: 0,
        },
    ]])
    .encode();
    overwrite_trailing_u32(&mut bytes, 9_999);
    assert_eq!(
        Song::decode(&bytes),
        Err(AudioError::JumpTargetOutOfRange {
            track_index: 0,
            event_index: 1,
            target: 9_999,
            event_count: 2,
        })
    );
}

#[test]
fn a_song_with_backward_and_self_jump_targets_round_trips() {
    let first_event = 0;
    let branch_event = 2;
    let track = vec![
        SongEvent::Note {
            key: 60,
            velocity: 100,
            gate: 24,
        },
        SongEvent::Goto(first_event),
        SongEvent::MemAccBranch {
            condition: MemAccCondition::Eq,
            address: 0,
            data: 1,
            target: branch_event,
        },
        SongEvent::Fine,
    ];
    let song = song(vec![track.clone()]);
    assert_eq!(Song::decode(&song.encode()).unwrap().tracks()[0], track);
}
