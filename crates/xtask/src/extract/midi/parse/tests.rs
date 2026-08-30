use super::{parse_track, MidiError, RawEvent};

const META_EVENT: u8 = 0xFF;
const END_OF_TRACK_META_EVENT: u8 = 0x2F;
const TEXT_META_EVENT: u8 = 0x01;
const TRACK_NAME_META_EVENT: u8 = 0x03;
const TEMPO_META_EVENT: u8 = 0x51;
const TIME_SIGNATURE_META_EVENT: u8 = 0x58;
const KEY_SIGNATURE_META_EVENT: u8 = 0x59;

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

fn meta_event(meta_type: u8, payload: &[u8]) -> Vec<u8> {
    let mut event = vec![META_EVENT, meta_type];
    event.extend(vlq(
        u32::try_from(payload.len()).expect("test meta payload length fits in u32")
    ));
    event.extend(payload);
    event
}

fn meta_event_with_declared_len(meta_type: u8, declared_len: u32, payload: &[u8]) -> Vec<u8> {
    let mut event = vec![META_EVENT, meta_type];
    event.extend(vlq(declared_len));
    event.extend(payload);
    event
}

fn note_on(channel: u8, key: u8, velocity: u8) -> [u8; 3] {
    [0x90 | channel, key, velocity]
}

fn running_note_on(key: u8, velocity: u8) -> [u8; 2] {
    [key, velocity]
}

fn note_off(channel: u8, key: u8, release_velocity: u8) -> [u8; 3] {
    [0x80 | channel, key, release_velocity]
}

fn control_change(channel: u8, controller: u8, value: u8) -> [u8; 3] {
    [0xB0 | channel, controller, value]
}

fn program_change(channel: u8, program: u8) -> [u8; 2] {
    [0xC0 | channel, program]
}

fn polyphonic_key_pressure(channel: u8, key: u8, pressure: u8) -> [u8; 3] {
    [0xA0 | channel, key, pressure]
}

fn channel_pressure(channel: u8, pressure: u8) -> [u8; 2] {
    [0xD0 | channel, pressure]
}

fn pitch_bend(channel: u8, lsb: u8, msb: u8) -> [u8; 3] {
    [0xE0 | channel, lsb, msb]
}

fn system_exclusive(payload: &[u8]) -> Vec<u8> {
    let mut event = vec![0xF0];
    event.extend(vlq(
        u32::try_from(payload.len()).expect("test system-exclusive payload length fits in u32")
    ));
    event.extend(payload);
    event
}

fn push_timed(body: &mut Vec<u8>, delta: u32, event: impl IntoIterator<Item = u8>) {
    body.extend(vlq(delta));
    body.extend(event);
}

fn terminated_track(mut body: Vec<u8>) -> Vec<u8> {
    push_timed(&mut body, 0, meta_event(END_OF_TRACK_META_EVENT, &[]));
    body
}

#[test]
fn note_on_and_off_with_running_status() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(&mut body, 24, running_note_on(60, 0));

    let parsed = parse_track(&terminated_track(body)).unwrap();
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
    push_timed(&mut body, 0, note_on(1, 64, 80));
    push_timed(&mut body, 10, note_off(1, 64, 0));

    let parsed = parse_track(&terminated_track(body)).unwrap();
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
    let tempo_payload = 500_000u32.to_be_bytes();
    let mut body = Vec::new();
    push_timed(
        &mut body,
        0,
        meta_event(TEMPO_META_EVENT, &tempo_payload[1..]),
    );

    let parsed = parse_track(&terminated_track(body)).unwrap();
    assert_eq!(parsed.events, vec![(0, RawEvent::Tempo(500_000))]);
}

#[test]
fn zero_tempo_is_rejected() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, meta_event(TEMPO_META_EVENT, &[0, 0, 0]));

    let error = parse_track(&terminated_track(body)).unwrap_err();
    assert_eq!(error, MidiError::ZeroTempo);
}

#[test]
fn channel_voice_data_bytes_must_be_seven_bit() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, note_on(0, 60, 128));

    let error = parse_track(&terminated_track(body)).unwrap_err();
    assert_eq!(error, MidiError::InvalidDataByte(128));
}

#[test]
fn all_four_loop_marker_texts_are_recognized() {
    let mut body = Vec::new();
    for (delta, text) in [(0u32, b"[".as_slice()), (1, b"]["), (1, b"]"), (1, b":")] {
        push_timed(&mut body, delta, meta_event(TEXT_META_EVENT, text));
    }

    let parsed = parse_track(&terminated_track(body)).unwrap();
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
    push_timed(&mut body, 0, meta_event(TRACK_NAME_META_EVENT, b"MUS1"));

    let parsed = parse_track(&terminated_track(body)).unwrap();
    assert!(parsed.events.is_empty());
}

#[test]
fn time_signature_parses_and_other_unhandled_meta_events_are_skipped() {
    let mut body = Vec::new();
    push_timed(
        &mut body,
        0,
        meta_event(TIME_SIGNATURE_META_EVENT, &[4, 2, 24, 8]),
    );
    push_timed(&mut body, 3, meta_event(KEY_SIGNATURE_META_EVENT, &[0, 0]));
    push_timed(&mut body, 2, note_on(0, 60, 100));

    let parsed = parse_track(&terminated_track(body)).unwrap();
    assert_eq!(
        parsed.events,
        vec![
            (
                0,
                RawEvent::TimeSignature {
                    numerator: 4,
                    denominator_exponent: 2
                }
            ),
            (
                5,
                RawEvent::NoteOn {
                    channel: 0,
                    key: 60,
                    velocity: 100
                }
            )
        ]
    );
}

#[test]
fn a_malformed_time_signature_fails_closed() {
    let mut body = Vec::new();
    push_timed(
        &mut body,
        0,
        meta_event_with_declared_len(TIME_SIGNATURE_META_EVENT, 3, &[4, 2, 24]),
    );
    assert_eq!(
        parse_track(&terminated_track(body)).unwrap_err(),
        MidiError::BadTimeSignatureLength(3)
    );

    let mut body = Vec::new();
    push_timed(
        &mut body,
        0,
        meta_event(TIME_SIGNATURE_META_EVENT, &[4, 16, 24, 8]),
    );
    assert_eq!(
        parse_track(&terminated_track(body)).unwrap_err(),
        MidiError::BadTimeSignatureDenominator(16)
    );
}

#[test]
fn pitch_bend_keeps_only_the_msb() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, pitch_bend(2, 0, 0x50));

    let parsed = parse_track(&terminated_track(body)).unwrap();
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
    push_timed(&mut body, 0, control_change(3, 7, 100));
    push_timed(&mut body, 0, program_change(3, 46));

    let parsed = parse_track(&terminated_track(body)).unwrap();
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
    push_timed(&mut body, 0, system_exclusive(&[0xAA, 0xBB]));
    push_timed(&mut body, 0, polyphonic_key_pressure(0, 60, 100));
    push_timed(&mut body, 0, channel_pressure(0, 50));

    let parsed = parse_track(&terminated_track(body)).unwrap();
    assert!(parsed.events.is_empty());
}

#[test]
fn a_bare_data_byte_with_no_prior_status_is_an_error() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, [0x10]);

    let error = parse_track(&terminated_track(body)).unwrap_err();
    assert_eq!(error, MidiError::InvalidStatusByte(0x10));
}

#[test]
fn end_of_track_stops_parsing_and_reports_its_own_tick() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, note_on(0, 60, 100));

    let parsed = parse_track(&terminated_track(body)).unwrap();
    assert_eq!(parsed.end_of_track, 0);
    assert_eq!(parsed.events.len(), 1);
}
