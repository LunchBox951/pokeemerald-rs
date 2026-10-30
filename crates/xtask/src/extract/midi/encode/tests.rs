use super::*;

/// A non-zero player so a dropped or misplaced field cannot pass as the default.
const PLAYER: u16 = 0x0102;

/// Bytes before the first event of a one-track `song_with_tracks` song: the
/// `audio/voicegroup/title` id (`u16` length prefix, 22 bytes), the `u16` player,
/// priority, reverb flag and value, the track count, and the track's `u32` event count.
const SINGLE_TRACK_HEADER_LEN: usize = 2 + 22 + 2 + 4 + 4;

fn song_with_tracks(tracks: Vec<Vec<SongEvent>>) -> CompiledSong {
    CompiledSong {
        voicegroup_label: "title".to_owned(),
        priority: 3,
        reverb: Some(50),
        tracks,
    }
}

fn song_with_label(voicegroup_label: String) -> CompiledSong {
    CompiledSong {
        voicegroup_label,
        priority: 0,
        reverb: None,
        tracks: Vec::new(),
    }
}

/// The event bytes `event` encodes to when it is followed by `Fine` (tag 17), which
/// also gives a `Goto` a valid target.
fn encoded_event(event: &SongEvent) -> Vec<u8> {
    let song = song_with_tracks(vec![vec![event.clone(), SongEvent::Fine]]);
    let bytes = encode_song(&song, PLAYER).unwrap();
    let (event_bytes, fine) =
        bytes[SINGLE_TRACK_HEADER_LEN..].split_at(bytes.len() - 1 - SINGLE_TRACK_HEADER_LEN);
    assert_eq!(fine, [17]);
    event_bytes.to_vec()
}

/// The schema's tags are the hand-written literals below, so a renumbered tag in
/// `assets` fails here instead of silently changing the pack.
#[test]
fn every_event_variant_encodes_its_tag_and_payload() {
    let cases = [
        (SongEvent::Wait(48), vec![0, 48]),
        (
            SongEvent::Note {
                key: 57,
                velocity: 120,
                gate: 48,
            },
            vec![1, 57, 120, 48],
        ),
        (SongEvent::EndOfTie { key: 53 }, vec![2, 1, 53]),
        (SongEvent::Voice(14), vec![3, 14]),
        (SongEvent::Volume(122), vec![4, 122]),
        (SongEvent::Pan(-32), vec![5, (-32_i8).to_le_bytes()[0]]),
        (SongEvent::Bend(3), vec![6, 3]),
        (SongEvent::BendRange(2), vec![7, 2]),
        (SongEvent::Tune(-5), vec![8, (-5_i8).to_le_bytes()[0]]),
        (SongEvent::KeyShift(-1), vec![9, (-1_i8).to_le_bytes()[0]]),
        (SongEvent::Tempo(144), vec![10, 144, 0]),
        (SongEvent::Priority(1), vec![11, 1]),
        (SongEvent::LfoSpeed(44), vec![12, 44]),
        (SongEvent::LfoDelay(6), vec![13, 6]),
        (SongEvent::Modulation(7), vec![14, 7]),
        (SongEvent::ModType(0), vec![15, 0]),
        (SongEvent::Goto(1), vec![16, 1, 0, 0, 0]),
        (SongEvent::PseudoEchoVolume(10), vec![18, 10]),
        (SongEvent::PseudoEchoLength(20), vec![19, 20]),
    ];

    for (event, expected) in cases {
        assert_eq!(encoded_event(&event), expected, "{event:?}");
    }
}

#[test]
fn song_encoding_frames_metadata_and_each_track() {
    let song = song_with_tracks(vec![
        vec![SongEvent::KeyShift(0), SongEvent::Fine],
        vec![SongEvent::Wait(1), SongEvent::Tempo(144), SongEvent::Fine],
    ]);

    let mut expected = Vec::new();
    expected.extend_from_slice(&22_u16.to_le_bytes());
    expected.extend_from_slice(b"audio/voicegroup/title");
    expected.extend_from_slice(&PLAYER.to_le_bytes());
    expected.extend_from_slice(&[3, 1, 50, 2]);
    expected.extend_from_slice(&2_u32.to_le_bytes());
    expected.extend_from_slice(&[9, 0, 17]);
    expected.extend_from_slice(&3_u32.to_le_bytes());
    expected.extend_from_slice(&[0, 1, 10, 144, 0, 17]);

    assert_eq!(encode_song(&song, PLAYER).unwrap(), expected);
}

#[test]
fn absent_reverb_encodes_a_false_flag_and_zero_value() {
    let song = song_with_label("title".to_owned());

    let mut expected = Vec::new();
    expected.extend_from_slice(&22_u16.to_le_bytes());
    expected.extend_from_slice(b"audio/voicegroup/title");
    expected.extend_from_slice(&PLAYER.to_le_bytes());
    expected.extend_from_slice(&[0, 0, 0, 0]);

    assert_eq!(encode_song(&song, PLAYER).unwrap(), expected);
}

/// The compiler leaves wait shape to the schema, so a raw track must encode as `assets`
/// canonicalizes it: zero rests vanish, adjacent rests merge and split at 255 ticks, a
/// rest a `Goto` targets stays addressable, and `Goto` targets follow the events they
/// name. These bytes were the output of the former hand-written canonicalizer and
/// encoder.
#[test]
fn raw_waits_encode_in_the_canonical_shape() {
    let raw = vec![
        SongEvent::Wait(0),
        SongEvent::Tempo(150),
        SongEvent::Wait(200),
        SongEvent::Wait(200),
        SongEvent::Note {
            key: 60,
            velocity: 100,
            gate: 0,
        },
        SongEvent::Wait(0),
        SongEvent::EndOfTie { key: 60 },
        SongEvent::Pan(-5),
        SongEvent::Goto(5),
        SongEvent::Fine,
    ];
    let song = song_with_tracks(vec![raw, vec![SongEvent::Fine]]);

    let mut expected = Vec::new();
    expected.extend_from_slice(&22_u16.to_le_bytes());
    expected.extend_from_slice(b"audio/voicegroup/title");
    expected.extend_from_slice(&PLAYER.to_le_bytes());
    expected.extend_from_slice(&[3, 1, 50, 2]);
    expected.extend_from_slice(&9_u32.to_le_bytes());
    expected.extend_from_slice(&[10, 150, 0]);
    expected.extend_from_slice(&[0, 255, 0, 145]);
    expected.extend_from_slice(&[1, 60, 100, 0]);
    expected.extend_from_slice(&[0, 0]);
    expected.extend_from_slice(&[2, 1, 60]);
    expected.extend_from_slice(&[5, (-5_i8).to_le_bytes()[0]]);
    expected.extend_from_slice(&[16, 4, 0, 0, 0]);
    expected.extend_from_slice(&[17]);
    expected.extend_from_slice(&1_u32.to_le_bytes());
    expected.extend_from_slice(&[17]);

    assert_eq!(encode_song(&song, PLAYER).unwrap(), expected);
}

#[test]
fn track_count_must_fit_the_wire_field() {
    let maximum_track_count = usize::from(u8::MAX);
    let first_unrepresentable_track_count = maximum_track_count + 1;

    assert!(encode_song(
        &song_with_tracks(vec![Vec::new(); maximum_track_count]),
        PLAYER
    )
    .is_ok());
    assert_eq!(
        encode_song(
            &song_with_tracks(vec![Vec::new(); first_unrepresentable_track_count]),
            PLAYER
        )
        .unwrap_err(),
        AudioError::TooManyTracks(first_unrepresentable_track_count)
    );
}

/// `midi.cfg`'s `-G` label is unbounded (`super::super::cfg::apply_flag`).
#[test]
fn voicegroup_pack_id_length_must_fit_the_wire_field() {
    let longest_label_len = usize::from(u16::MAX) - VOICEGROUP_PACK_ID_PREFIX.len();

    assert!(encode_song(&song_with_label("v".repeat(longest_label_len)), PLAYER).is_ok());
    assert_eq!(
        encode_song(&song_with_label("v".repeat(longest_label_len + 1)), PLAYER).unwrap_err(),
        AudioError::IdTooLong(usize::from(u16::MAX) + 1)
    );
}

#[test]
fn a_jump_outside_its_track_is_rejected_by_the_schema() {
    let song = song_with_tracks(vec![vec![SongEvent::Goto(2), SongEvent::Fine]]);

    assert_eq!(
        encode_song(&song, PLAYER).unwrap_err(),
        AudioError::JumpTargetOutOfRange {
            track_index: 0,
            event_index: 0,
            target: 2,
            event_count: 2,
        }
    );
}
