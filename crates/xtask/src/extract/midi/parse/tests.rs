use super::{parse_track, RawEvent};

/// Encode a variable-length quantity the same way a real `.mid` file would
/// (big-endian 7-bit groups, continuation bit set on every group but the
/// last) — the inverse of `MidiReader::vlq`, used only by these tests to
/// build crafted track bytes by hand.
fn vlq(mut value: u32) -> Vec<u8> {
    let mut groups = vec![(value & 0x7F) as u8];
    value >>= 7;
    while value > 0 {
        groups.push((value & 0x7F) as u8 | 0x80);
        value >>= 7;
    }
    groups.reverse();
    groups
}

/// A minimal well-formed `MTrk` body: a caller-supplied delta-time-prefixed
/// event stream, terminated with a standard `0x00 0xFF 0x2F 0x00`
/// end-of-track meta event.
fn track(mut body: Vec<u8>) -> Vec<u8> {
    body.extend_from_slice(&[0x00, 0xFF, 0x2F, 0x00]);
    body
}

#[test]
fn note_on_and_off_with_running_status() {
    // delta 0, note-on ch0 key60 vel100; delta 24 (running status, no
    // status byte repeated), note-off key60 vel0.
    let mut body = Vec::new();
    body.extend(vlq(0));
    body.extend([0x90, 60, 100]);
    body.extend(vlq(24));
    body.extend([60, 0]); // running status 0x90, velocity 0 -> NoteOff
    let parsed = parse_track(&track(body)).unwrap();
    assert_eq!(
        parsed.events,
        vec![
            (
                0,
                RawEvent::NoteOn {
                    channel: 0,
                    key: 60,
                    velocity: 100
                }
            ),
            (
                24,
                RawEvent::NoteOff {
                    channel: 0,
                    key: 60
                }
            ),
        ]
    );
    assert_eq!(parsed.end_of_track, 24);
}

#[test]
fn explicit_note_off_status_is_recognized() {
    let mut body = Vec::new();
    body.extend(vlq(0));
    body.extend([0x91, 64, 80]); // note-on channel 1
    body.extend(vlq(10));
    body.extend([0x81, 64, 0]); // explicit note-off, channel 1
    let parsed = parse_track(&track(body)).unwrap();
    assert_eq!(
        parsed.events,
        vec![
            (
                0,
                RawEvent::NoteOn {
                    channel: 1,
                    key: 64,
                    velocity: 80
                }
            ),
            (
                10,
                RawEvent::NoteOff {
                    channel: 1,
                    key: 64
                }
            ),
        ]
    );
}

#[test]
fn tempo_meta_event_reads_the_24_bit_microseconds_value() {
    let mut body = Vec::new();
    body.extend(vlq(0));
    body.extend([0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20]); // 500000 us/qn
    let parsed = parse_track(&track(body)).unwrap();
    assert_eq!(parsed.events, vec![(0, RawEvent::Tempo(500_000))]);
}

#[test]
fn all_four_loop_marker_texts_are_recognized() {
    let mut body = Vec::new();
    for (delta, text) in [(0u32, "["), (1, "]["), (1, "]"), (1, ":")] {
        body.extend(vlq(delta));
        body.push(0xFF);
        body.push(0x01); // text meta type
        body.push(u8::try_from(text.len()).expect("test marker text is tiny"));
        body.extend(text.as_bytes());
    }
    let parsed = parse_track(&track(body)).unwrap();
    assert_eq!(
        parsed.events,
        vec![
            (0, RawEvent::LoopBegin),
            (1, RawEvent::LoopEndBegin),
            (2, RawEvent::LoopEnd),
            (3, RawEvent::Label),
        ]
    );
}

#[test]
fn unrecognized_text_meta_yields_no_event() {
    let mut body = Vec::new();
    body.extend(vlq(0));
    body.extend([0xFF, 0x03, 0x04]); // track-name meta, length 4
    body.extend(b"MUS1");
    let parsed = parse_track(&track(body)).unwrap();
    assert!(parsed.events.is_empty());
}

#[test]
fn time_signature_and_other_unhandled_meta_events_are_skipped_without_error() {
    let mut body = Vec::new();
    body.extend(vlq(0));
    body.extend([0xFF, 0x58, 0x04, 4, 2, 24, 8]); // time signature, ignored
    body.extend(vlq(5));
    body.extend([0x90, 60, 100]); // a real note-on should still parse after it
    let parsed = parse_track(&track(body)).unwrap();
    assert_eq!(
        parsed.events,
        vec![(
            5,
            RawEvent::NoteOn {
                channel: 0,
                key: 60,
                velocity: 100
            }
        )]
    );
}

#[test]
fn pitch_bend_keeps_only_the_msb() {
    let mut body = Vec::new();
    body.extend(vlq(0));
    body.extend([0xE2, 0x00, 0x50]); // channel 2, lsb=0, msb=0x50
    let parsed = parse_track(&track(body)).unwrap();
    assert_eq!(
        parsed.events,
        vec![(
            0,
            RawEvent::PitchBend {
                channel: 2,
                msb: 0x50
            }
        )]
    );
}

#[test]
fn controller_and_program_change_are_recognized() {
    let mut body = Vec::new();
    body.extend(vlq(0));
    body.extend([0xB3, 7, 100]); // channel 3, controller 7 (volume) = 100
    body.extend(vlq(0));
    body.extend([0xC3, 46]); // program change, channel 3
    let parsed = parse_track(&track(body)).unwrap();
    assert_eq!(
        parsed.events,
        vec![
            (
                0,
                RawEvent::Controller {
                    channel: 3,
                    controller: 7,
                    value: 100
                }
            ),
            (
                0,
                RawEvent::ProgramChange {
                    channel: 3,
                    program: 46
                }
            ),
        ]
    );
}

#[test]
fn sysex_and_aftertouch_are_skipped() {
    let mut body = Vec::new();
    body.extend(vlq(0));
    body.extend([0xF0, 0x02, 0xAA, 0xBB]); // sysex, 2 data bytes
    body.extend(vlq(0));
    body.extend([0xA0, 60, 100]); // poly aftertouch, channel 0
    body.extend(vlq(0));
    body.extend([0xD0, 50]); // channel aftertouch
    let parsed = parse_track(&track(body)).unwrap();
    assert!(parsed.events.is_empty());
}

#[test]
fn a_bare_data_byte_with_no_prior_status_is_an_error() {
    let mut body = Vec::new();
    body.extend(vlq(0));
    body.push(0x10); // data byte, but no running status has been set yet
    let err = parse_track(&track(body)).unwrap_err();
    assert!(matches!(
        err,
        super::super::error::MidiError::InvalidStatusByte(0x10)
    ));
}

#[test]
fn end_of_track_stops_parsing_and_reports_its_own_tick() {
    let mut body = Vec::new();
    body.extend(vlq(0));
    body.extend([0x90, 60, 100]);
    let bytes = track(body);
    let parsed = parse_track(&bytes).unwrap();
    assert_eq!(parsed.end_of_track, 0);
    assert_eq!(parsed.events.len(), 1);
}
