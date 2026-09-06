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

/// A `MEMACC` conditional-loop shape (review finding on #193): set a memory
/// cell, play the body, then conditionally jump on the cell's value. No
/// canonical song has this shape -- `mus_vs_trainer`, the only song carrying
/// a `MEMACC` at all, issues a single unconditional `mem_set` and never
/// branches -- so this exercises the [`SongEvent::MemAccBranch`] half of the
/// pair as raw-ROM/defensive breadth rather than as shipped data.
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
            // Index 4, the trailing `Fine` -- the last valid index in this
            // 5-event track (was `5`, one past the end, before
            // `Song::new`/`Song::decode` validated jump targets, #868).
            target: 4,
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

/// [`SongEvent::Goto`] and [`SongEvent::MemAccBranch`] targets are
/// documented as event indices into their own track (module docs,
/// "Looping"); a target at or past the track's own event count cannot
/// address one. Mirrors
/// [`super::super::sample::tests::constructor_rejects_a_loop_start_at_or_past_the_data_length`].
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

/// Decode is the trust boundary for pack bytes, so it must reject the same
/// out-of-range jump target its constructor does -- as
/// [`super::super::sample::tests::decode_rejects_a_loop_start_at_or_past_the_decoded_data_length`]
/// already does for a sample's loop start. Both target-carrying events sit
/// last in their (only) track, so the trailing 4 bytes are the target.
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
    let target_at = bytes.len() - 4;
    bytes[target_at..].copy_from_slice(&9_999u32.to_le_bytes());
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
    let target_at = bytes.len() - 4;
    bytes[target_at..].copy_from_slice(&9_999u32.to_le_bytes());
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

/// A backward [`SongEvent::Goto`] and a self-targeting
/// [`SongEvent::MemAccBranch`] both address an event strictly within their
/// track (indices below `event_count`), so both remain accepted.
#[test]
fn a_song_with_backward_and_self_jump_targets_round_trips() {
    let track = vec![
        SongEvent::Note {
            key: 60,
            velocity: 100,
            gate: 24,
        },
        SongEvent::Goto(0), // backward: back to the track's first event.
        SongEvent::MemAccBranch {
            condition: MemAccCondition::Eq,
            address: 0,
            data: 1,
            target: 2, // self: this event's own index.
        },
        SongEvent::Fine,
    ];
    let song = song(vec![track.clone()]);
    assert_eq!(Song::decode(&song.encode()).unwrap().tracks()[0], track);
}
