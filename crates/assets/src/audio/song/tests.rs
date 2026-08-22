//! [`Song`]/[`SongEvent`] round-trip and validation tests, split out of
//! `song.rs` itself to keep that file under this crate's ~600-line-per-file
//! guideline (`oop-boundaries`) — mirrors `super::super::voicegroup`'s own
//! `mod tests;` split.

use super::*;

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
    .expect("within every documented bound")
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
fn every_controller_and_note_event_kind_round_trips() {
    let track = vec![
        SongEvent::Wait(96),
        SongEvent::Note {
            key: 0,
            velocity: 0,
            gate: 0,
        },
        SongEvent::EndOfTie { key: Some(72) },
        SongEvent::Voice(127),
        SongEvent::Volume(0),
        SongEvent::Pan(-64),
        SongEvent::Pan(63),
        SongEvent::Bend(0),
        SongEvent::BendRange(2),
        SongEvent::Tune(-64),
        SongEvent::KeyShift(12),
        SongEvent::Tempo(300),
        SongEvent::Priority(255),
        SongEvent::LfoSpeed(16),
        SongEvent::LfoDelay(0),
        SongEvent::Modulation(127),
        SongEvent::ModType(1),
        SongEvent::PseudoEchoVolume(127),
        SongEvent::PseudoEchoLength(0),
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
fn the_pseudo_echo_pair_round_trips_at_both_ends_of_its_range() {
    // `XCMD xIECV`/`xIECL` (m4a.c `ply_xiecv`/`ply_xiecl`) are the only two
    // extended commands mid2agb emits, and they carry the CGB decay tail --
    // audible content, so pin both operand extremes explicitly rather than
    // relying on the mixed-event test above.
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
    // The shape mid2agb actually emits: the echo settings precede the notes
    // they apply to, and `ply_note` copies the track's current values onto
    // each new channel -- so their position in the stream is meaningful and
    // must survive a round trip unreordered.
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
    // MUS_TITLE's own MIDI source selects instrument 127 on one channel
    // -- pin that a `Voice` event carrying that value round-trips.
    let song = song(vec![vec![SongEvent::Voice(127), SongEvent::Fine]]);
    let decoded = Song::decode(&song.encode()).unwrap();
    assert_eq!(decoded.tracks()[0][0], SongEvent::Voice(127));
}

#[test]
fn tempo_is_carried_as_a_track_event() {
    // Upstream's `SongHeader` has no tempo field; tempo is a `TEMPO`
    // command in a track's own stream (see the module docs), so this is
    // where a song's starting tempo lives.
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
    // The documented `MAX_TRACKS` bound is what makes `encode`'s `u8` track
    // count total -- it must be an error at construction, not a panic
    // inside `encode`.
    let tracks = vec![vec![SongEvent::Fine]; MAX_TRACKS + 1];
    assert_eq!(
        Song::new(VoiceGroupId("x".to_owned()), 0, None, tracks),
        Err(AudioError::TooManyTracks(MAX_TRACKS + 1))
    );
}

#[test]
fn an_oversize_voicegroup_id_is_rejected_by_the_constructor() {
    // `Writer::string`'s `u16` length prefix is the bound; reaching it must
    // be an `AudioError`, not a panic from safe code.
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
    let mut bytes = song(vec![vec![SongEvent::Fine]]).encode();
    let last = bytes.len() - 1;
    bytes[last] = 0xFF; // the lone event's tag byte
    assert_eq!(
        Song::decode(&bytes),
        Err(AudioError::UnknownSongEvent(0xFF))
    );
}

#[test]
fn a_tag_just_past_the_last_defined_one_is_rejected() {
    // Guards the additive tag space: `TAG_MEM_ACC_BRANCH` (21) is the
    // highest defined tag, so 22 must still be rejected rather than read as
    // some neighbouring variant.
    let mut bytes = song(vec![vec![SongEvent::Fine]]).encode();
    let last = bytes.len() - 1;
    bytes[last] = TAG_MEM_ACC_BRANCH + 1;
    assert_eq!(
        Song::decode(&bytes),
        Err(AudioError::UnknownSongEvent(TAG_MEM_ACC_BRANCH + 1))
    );
}

/// `mus_vs_trainer`'s own conditional-loop shape (review finding on #193):
/// mid2agb's generated loop sets a memory cell, plays the body, and
/// conditionally jumps on the cell's value -- the one canonical song the
/// [`SongEvent::MemAcc`]/[`SongEvent::MemAccBranch`] pair exists for.
#[test]
fn the_memacc_conditional_loop_round_trips() {
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
            target: 5,
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

/// Every [`MemAccOp`]/[`MemAccCondition`] discriminant round-trips, and
/// each wire byte is upstream's own `mem_*` value (`sound/MPlayDef.s`) --
/// transcribed literals, not read back from the enums under test.
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

/// A MEMACC op byte outside `0..=5` (or a branch condition outside
/// `6..=17`) is structurally invalid, not a neighbouring variant.
#[test]
fn an_out_of_range_memacc_op_byte_is_rejected() {
    // A track of exactly one MemAcc event: corrupt its op byte (the byte
    // right after the event tag, which is the last-but-2 byte: tag, op,
    // address, data).
    let mut bytes = song(vec![vec![SongEvent::MemAcc {
        op: MemAccOp::Set,
        address: 0,
        data: 0,
    }]])
    .encode();
    let op_at = bytes.len() - 3;
    bytes[op_at] = 18; // first byte past mem_mem_blo -- valid for neither.
    assert_eq!(Song::decode(&bytes), Err(AudioError::UnknownMemAccOp(18)));
}

/// Review finding on #193: a decoder that ignores trailing bytes validates
/// a corrupt (or newer-producer) payload as its own prefix.
#[test]
fn decode_rejects_trailing_bytes() {
    let mut bytes = song(vec![sample_track()]).encode();
    bytes.push(0);
    assert_eq!(Song::decode(&bytes), Err(AudioError::TrailingBytes(1)));
}

#[test]
fn a_non_utf8_voicegroup_id_is_rejected() {
    let mut bytes = song(vec![]).encode();
    // The id's bytes start right after its 2-byte length prefix.
    bytes[2] = 0xFF;
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
    //! `Song::new`'s wait rewrite (`super::super::canonical`).

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
        // `Goto(4)` names `Voice(1)`, which moves to index 2 once the three
        // rests before it merge.
        let track = vec![
            SongEvent::Voice(0),
            SongEvent::Wait(10),
            SongEvent::Wait(10),
            SongEvent::Wait(10),
            SongEvent::Voice(1),
            SongEvent::Goto(4),
        ];
        assert_eq!(
            canon(track),
            vec![
                SongEvent::Voice(0),
                SongEvent::Wait(30),
                SongEvent::Voice(1),
                SongEvent::Goto(2),
            ]
        );
    }

    #[test]
    fn a_run_does_not_merge_across_a_jump_target() {
        // The loop re-enters between the two rests, so they stay apart and
        // the target keeps naming the second one.
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
        assert_eq!(canon(vec![SongEvent::Goto(99)]), vec![SongEvent::Goto(99)]);
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
