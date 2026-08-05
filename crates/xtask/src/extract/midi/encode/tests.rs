use super::encode_song;
use crate::extract::midi::compile::CompiledSong;
use crate::extract::midi::error::MidiError;
use crate::extract::midi::event::SongEvent;

fn sample_track() -> Vec<SongEvent> {
    vec![
        SongEvent::KeyShift(0),
        SongEvent::Voice(14),
        SongEvent::Volume(122),
        SongEvent::Pan(-32),
        SongEvent::Note {
            key: 57,
            velocity: 120,
            gate: 48,
        },
        SongEvent::Wait(48),
        SongEvent::Note {
            key: 53,
            velocity: 112,
            gate: 0,
        },
        SongEvent::EndOfTie { key: 53 },
        SongEvent::Bend(3),
        SongEvent::BendRange(2),
        SongEvent::Tune(-5),
        SongEvent::Tempo(144),
        SongEvent::Priority(1),
        SongEvent::LfoSpeed(44),
        SongEvent::LfoDelay(6),
        SongEvent::Modulation(7),
        SongEvent::ModType(0),
        SongEvent::PseudoEchoVolume(10),
        SongEvent::PseudoEchoLength(20),
        SongEvent::Goto(3),
        SongEvent::Fine,
    ]
}

/// Pins the wire format byte-for-byte against
/// `crates/assets::audio::song::Song::encode`'s documented layout — the two
/// encoders are deliberately not shared code (module docs), so each side
/// pins its own understanding of the format. The cross-crate half of this
/// pin (decoding a real extracted pack's `audio/song/mus_title` payload back
/// through `Song::decode`) lives in `crates/assets/src/pack/tests.rs`.
#[test]
fn encode_song_matches_the_documented_wire_format() {
    let song = CompiledSong {
        voicegroup_label: "title".to_owned(),
        priority: 3,
        reverb: Some(50),
        tracks: vec![sample_track()],
    };
    let bytes =
        encode_song(&song).expect("the sample song is well inside the u8 track-count bound");

    let mut expected = Vec::new();
    let id = "audio/voicegroup/title";
    let id_len = u16::try_from(id.len()).expect("test id is tiny");
    expected.extend_from_slice(&id_len.to_le_bytes());
    expected.extend_from_slice(id.as_bytes());
    expected.push(3); // priority
    expected.push(1); // has reverb
    expected.push(50); // reverb
    expected.push(1); // track count
    expected.extend_from_slice(&21u32.to_le_bytes()); // event count
    expected.extend_from_slice(&[9, 0]); // KeyShift(0)
    expected.extend_from_slice(&[3, 14]); // Voice(14)
    expected.extend_from_slice(&[4, 122]); // Volume(122)
    expected.extend_from_slice(&[5, (-32i8).to_ne_bytes()[0]]); // Pan(-32)
    expected.extend_from_slice(&[1, 57, 120, 48]); // Note
    expected.extend_from_slice(&[0, 48]); // Wait(48)
    expected.extend_from_slice(&[1, 53, 112, 0]); // Note (tie start)
    expected.extend_from_slice(&[2, 1, 53]); // EndOfTie { key: Some(53) }
    expected.extend_from_slice(&[6, 3u8.to_ne_bytes()[0]]); // Bend(3)
    expected.extend_from_slice(&[7, 2]); // BendRange(2)
    expected.extend_from_slice(&[8, (-5i8).to_ne_bytes()[0]]); // Tune(-5)
    expected.push(10); // Tempo tag
    expected.extend_from_slice(&144u16.to_le_bytes());
    expected.extend_from_slice(&[11, 1]); // Priority(1)
    expected.extend_from_slice(&[12, 44]); // LfoSpeed(44)
    expected.extend_from_slice(&[13, 6]); // LfoDelay(6)
    expected.extend_from_slice(&[14, 7]); // Modulation(7)
    expected.extend_from_slice(&[15, 0]); // ModType(0)
    expected.extend_from_slice(&[18, 10]); // PseudoEchoVolume(10)
    expected.extend_from_slice(&[19, 20]); // PseudoEchoLength(20)
    expected.push(16); // Goto tag
    expected.extend_from_slice(&3u32.to_le_bytes());
    expected.push(17); // Fine

    assert_eq!(bytes, expected);
}

#[test]
fn no_reverb_writes_a_false_flag_and_zero_byte() {
    let song = CompiledSong {
        voicegroup_label: "title".to_owned(),
        priority: 0,
        reverb: None,
        tracks: vec![vec![SongEvent::Fine]],
    };
    let bytes =
        encode_song(&song).expect("the sample song is well inside the u8 track-count bound");
    let id_len = "audio/voicegroup/title".len();
    // id (2 + id_len bytes), priority (1), has_reverb (1), reverb (1)
    let reverb_flag_offset = 2 + id_len + 1;
    assert_eq!(bytes[reverb_flag_offset], 0);
    assert_eq!(bytes[reverb_flag_offset + 1], 0);
}

#[test]
fn multiple_tracks_each_carry_their_own_event_count() {
    let song = CompiledSong {
        voicegroup_label: "title".to_owned(),
        priority: 0,
        reverb: None,
        tracks: vec![
            vec![SongEvent::Fine],
            vec![SongEvent::Wait(1), SongEvent::Fine],
        ],
    };
    let bytes =
        encode_song(&song).expect("the sample song is well inside the u8 track-count bound");
    let id_len = "audio/voicegroup/title".len();
    let track_count_offset = 2 + id_len + 1 + 1 + 1;
    assert_eq!(bytes[track_count_offset], 2);
}

/// More tracks than the `u8` track-count field can describe is a returned
/// error, not a panic. 255 tracks is the last encodable count; 256 is the
/// first that is not. Reachable only from a format-1 file with seventeen or
/// more note-carrying `MTrk` chunks (16 channels each), which is why the
/// bound is the *wire format's*, not one upstream imposes — but the caller
/// still gets a diagnostic it can attach a path to.
#[test]
fn more_tracks_than_the_u8_count_can_describe_is_an_error() {
    let song = |count: usize| CompiledSong {
        voicegroup_label: "title".to_owned(),
        priority: 0,
        reverb: None,
        tracks: vec![vec![SongEvent::Fine]; count],
    };
    assert!(encode_song(&song(255)).is_ok());
    assert_eq!(
        encode_song(&song(256)).unwrap_err(),
        MidiError::TooManyTracks(256)
    );
}
