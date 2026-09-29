use super::*;

fn encoded_event(event: &SongEvent) -> Vec<u8> {
    let mut bytes = Vec::new();
    write_event(&mut bytes, event);
    bytes
}

fn song_with_tracks(tracks: Vec<Vec<SongEvent>>) -> CompiledSong {
    CompiledSong {
        voicegroup_label: "title".to_owned(),
        priority: 3,
        reverb: Some(50),
        tracks,
    }
}

fn push_expected_string(out: &mut Vec<u8>, value: &str) {
    let byte_len = u16::try_from(value.len()).unwrap();
    out.extend_from_slice(&byte_len.to_le_bytes());
    out.extend_from_slice(value.as_bytes());
}

fn push_expected_track(out: &mut Vec<u8>, events: &[(SongEvent, Vec<u8>)]) {
    let event_count = u32::try_from(events.len()).unwrap();
    out.extend_from_slice(&event_count.to_le_bytes());
    for (_, encoded) in events {
        out.extend_from_slice(encoded);
    }
}

#[test]
fn event_tags_match_the_asset_schema() {
    assert_eq!(
        [
            EventTag::Wait.byte(),
            EventTag::Note.byte(),
            EventTag::EndOfTie.byte(),
            EventTag::Voice.byte(),
            EventTag::Volume.byte(),
            EventTag::Pan.byte(),
            EventTag::Bend.byte(),
            EventTag::BendRange.byte(),
            EventTag::Tune.byte(),
            EventTag::KeyShift.byte(),
            EventTag::Tempo.byte(),
            EventTag::Priority.byte(),
            EventTag::LfoSpeed.byte(),
            EventTag::LfoDelay.byte(),
            EventTag::Modulation.byte(),
            EventTag::ModType.byte(),
            EventTag::Goto.byte(),
            EventTag::Fine.byte(),
            EventTag::PseudoEchoVolume.byte(),
            EventTag::PseudoEchoLength.byte(),
        ],
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19,]
    );
}

#[test]
fn every_event_variant_encodes_its_tag_and_payload() {
    let cases = [
        (SongEvent::Wait(48), vec![EventTag::Wait.byte(), 48]),
        (
            SongEvent::Note {
                key: 57,
                velocity: 120,
                gate: 48,
            },
            vec![EventTag::Note.byte(), 57, 120, 48],
        ),
        (
            SongEvent::EndOfTie { key: 53 },
            vec![EventTag::EndOfTie.byte(), u8::from(true), 53],
        ),
        (SongEvent::Voice(14), vec![EventTag::Voice.byte(), 14]),
        (SongEvent::Volume(122), vec![EventTag::Volume.byte(), 122]),
        (
            SongEvent::Pan(-32),
            vec![EventTag::Pan.byte(), (-32_i8).to_le_bytes()[0]],
        ),
        (
            SongEvent::Bend(3),
            vec![EventTag::Bend.byte(), 3_i8.to_le_bytes()[0]],
        ),
        (SongEvent::BendRange(2), vec![EventTag::BendRange.byte(), 2]),
        (
            SongEvent::Tune(-5),
            vec![EventTag::Tune.byte(), (-5_i8).to_le_bytes()[0]],
        ),
        (
            SongEvent::KeyShift(-1),
            vec![EventTag::KeyShift.byte(), (-1_i8).to_le_bytes()[0]],
        ),
        (
            SongEvent::Tempo(144),
            [vec![EventTag::Tempo.byte()], 144_u16.to_le_bytes().to_vec()].concat(),
        ),
        (SongEvent::Priority(1), vec![EventTag::Priority.byte(), 1]),
        (SongEvent::LfoSpeed(44), vec![EventTag::LfoSpeed.byte(), 44]),
        (SongEvent::LfoDelay(6), vec![EventTag::LfoDelay.byte(), 6]),
        (
            SongEvent::Modulation(7),
            vec![EventTag::Modulation.byte(), 7],
        ),
        (SongEvent::ModType(0), vec![EventTag::ModType.byte(), 0]),
        (
            SongEvent::Goto(3),
            [vec![EventTag::Goto.byte()], 3_u32.to_le_bytes().to_vec()].concat(),
        ),
        (SongEvent::Fine, vec![EventTag::Fine.byte()]),
        (
            SongEvent::PseudoEchoVolume(10),
            vec![EventTag::PseudoEchoVolume.byte(), 10],
        ),
        (
            SongEvent::PseudoEchoLength(20),
            vec![EventTag::PseudoEchoLength.byte(), 20],
        ),
    ];

    for (event, expected) in cases {
        assert_eq!(encoded_event(&event), expected);
    }
}

#[test]
fn song_encoding_frames_metadata_and_each_track() {
    let first_track = [
        (SongEvent::KeyShift(0), vec![EventTag::KeyShift.byte(), 0]),
        (SongEvent::Fine, vec![EventTag::Fine.byte()]),
    ];
    let second_track = [
        (SongEvent::Wait(1), vec![EventTag::Wait.byte(), 1]),
        (
            SongEvent::Tempo(144),
            [vec![EventTag::Tempo.byte()], 144_u16.to_le_bytes().to_vec()].concat(),
        ),
        (SongEvent::Fine, vec![EventTag::Fine.byte()]),
    ];
    let song = song_with_tracks(vec![
        first_track.iter().map(|(event, _)| event.clone()).collect(),
        second_track
            .iter()
            .map(|(event, _)| event.clone())
            .collect(),
    ]);

    let mut expected = Vec::new();
    push_expected_string(&mut expected, "audio/voicegroup/title");
    expected.extend_from_slice(&[song.priority, u8::from(true), 50, 2]);
    push_expected_track(&mut expected, &first_track);
    push_expected_track(&mut expected, &second_track);

    assert_eq!(encode_song(&song).unwrap(), expected);
}

#[test]
fn absent_reverb_encodes_a_false_flag_and_zero_value() {
    let song = CompiledSong {
        voicegroup_label: "title".to_owned(),
        priority: 0,
        reverb: None,
        tracks: Vec::new(),
    };
    let mut expected = Vec::new();
    push_expected_string(&mut expected, "audio/voicegroup/title");
    expected.extend_from_slice(&[song.priority, u8::from(false), 0, 0]);

    assert_eq!(encode_song(&song).unwrap(), expected);
}

#[test]
fn track_count_must_fit_the_wire_field() {
    let maximum_track_count = usize::from(u8::MAX);
    let first_unrepresentable_track_count = maximum_track_count + 1;

    assert!(encode_song(&song_with_tracks(vec![Vec::new(); maximum_track_count])).is_ok());
    assert_eq!(
        encode_song(&song_with_tracks(vec![
            Vec::new();
            first_unrepresentable_track_count
        ]))
        .unwrap_err(),
        MidiError::TooManyTracks(first_unrepresentable_track_count)
    );
}

#[test]
fn track_event_count_must_fit_the_wire_field() {
    let largest_representable_count = usize::try_from(u32::MAX).unwrap();
    assert_eq!(event_count_field(largest_representable_count), Ok(u32::MAX));

    if let Some(unrepresentable_count) = largest_representable_count.checked_add(1) {
        assert_eq!(
            event_count_field(unrepresentable_count),
            Err(MidiError::TooManyTrackEvents(unrepresentable_count))
        );
    }
}

/// `midi.cfg`'s `-G` label is unbounded (`super::super::cfg::apply_flag`).
#[test]
fn voicegroup_pack_id_length_must_fit_the_wire_field() {
    let song_with_label = |voicegroup_label: String| CompiledSong {
        voicegroup_label,
        priority: 0,
        reverb: None,
        tracks: Vec::new(),
    };
    let longest_label_len = usize::from(u16::MAX) - "audio/voicegroup/".len();

    assert!(encode_song(&song_with_label("v".repeat(longest_label_len))).is_ok());
    assert_eq!(
        encode_song(&song_with_label("v".repeat(longest_label_len + 1))).unwrap_err(),
        MidiError::VoiceGroupPackIdTooLong(usize::from(u16::MAX) + 1)
    );
}
