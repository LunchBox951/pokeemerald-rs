//! Synthetic MIDI byte builders and default compilation configuration.

use crate::extract::midi::cfg::MidiCfgEntry;

const META_EVENT: u8 = 0xFF;
pub(super) const END_OF_TRACK_META_EVENT: u8 = 0x2F;
const TEXT_META_EVENT: u8 = 0x01;
const TEMPO_META_EVENT: u8 = 0x51;
const TIME_SIGNATURE_META_EVENT: u8 = 0x58;
pub(super) const MODULATION_CONTROLLER: u8 = 0x01;
pub(super) const VOLUME_CONTROLLER: u8 = 0x07;
pub(super) const MEMACC_CONTROLLERS: [u8; 6] = [0x0C, 0x0D, 0x0E, 0x0F, 0x10, 0x11];
pub(super) const LFO_SPEED_CONTROLLER: u8 = 0x15;
pub(super) const EXTENDED_COMMAND_TRIGGER: u8 = 0x1D;
pub(super) const EXTENDED_COMMAND_SELECTOR: u8 = 0x1E;
pub(super) const ALTERNATE_EXTENDED_COMMAND_TRIGGER: u8 = 0x1F;
pub(super) const PSEUDO_ECHO_VOLUME_COMMAND: u8 = 8;
pub(super) const PSEUDO_ECHO_LENGTH_COMMAND: u8 = 9;
pub(super) const UNKNOWN_CONTROLLER: u8 = 0x50;
pub(super) const MAX_STANDARD_VLQ: u32 = 0x0FFF_FFFF;
pub(super) const OVERLONG_ZERO_VLQ: [u8; 5] = [0x90, 0x80, 0x80, 0x80, 0x00];

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

pub(super) fn meta_event(meta_type: u8, payload: &[u8]) -> Vec<u8> {
    let mut event = vec![META_EVENT, meta_type];
    event.extend(vlq(
        u32::try_from(payload.len()).expect("test meta payload length fits in u32")
    ));
    event.extend(payload);
    event
}

pub(super) fn note_on(channel: u8, key: u8, velocity: u8) -> [u8; 3] {
    [0x90 | channel, key, velocity]
}

pub(super) fn note_off(channel: u8, key: u8) -> [u8; 3] {
    [0x80 | channel, key, 0]
}

pub(super) fn running_note_off(key: u8) -> [u8; 2] {
    [key, 0]
}

pub(super) fn control_change(channel: u8, controller: u8, value: u8) -> [u8; 3] {
    [0xB0 | channel, controller, value]
}

pub(super) fn running_control_change(controller: u8, value: u8) -> [u8; 2] {
    [controller, value]
}

pub(super) fn program_change(channel: u8, program: u8) -> [u8; 2] {
    [0xC0 | channel, program]
}

pub(super) fn pitch_bend(channel: u8, lsb: u8, msb: u8) -> [u8; 3] {
    [0xE0 | channel, lsb, msb]
}

pub(super) fn tempo(microseconds_per_quarter_note: u32) -> Vec<u8> {
    let bytes = microseconds_per_quarter_note.to_be_bytes();
    meta_event(TEMPO_META_EVENT, &bytes[1..])
}

pub(super) fn time_signature(numerator: u8, denominator_exponent: u8) -> Vec<u8> {
    meta_event(
        TIME_SIGNATURE_META_EVENT,
        &[numerator, denominator_exponent, 24, 8],
    )
}

pub(super) fn text_event(text: &[u8]) -> Vec<u8> {
    meta_event(TEXT_META_EVENT, text)
}

pub(super) fn push_timed(body: &mut Vec<u8>, delta: u32, event: impl IntoIterator<Item = u8>) {
    body.extend(vlq(delta));
    body.extend(event);
}

pub(super) fn push_max_delta_empty_text_events(body: &mut Vec<u8>, count: usize) {
    for _ in 0..count {
        push_timed(body, MAX_STANDARD_VLQ, text_event(b""));
    }
}

fn mthd(format: u16, track_count: u16, division: u16) -> Vec<u8> {
    let mut out = b"MThd".to_vec();
    out.extend(6u32.to_be_bytes());
    out.extend(format.to_be_bytes());
    out.extend(track_count.to_be_bytes());
    out.extend(division.to_be_bytes());
    out
}

fn mtrk(mut body: Vec<u8>) -> Vec<u8> {
    push_timed(&mut body, 0, meta_event(END_OF_TRACK_META_EVENT, &[]));
    let mut out = b"MTrk".to_vec();
    let len = u32::try_from(body.len()).expect("test track bodies are tiny");
    out.extend(len.to_be_bytes());
    out.extend(body);
    out
}

pub(super) fn single_track_midi(division: u16, track_body: Vec<u8>) -> Vec<u8> {
    let mut file = mthd(0, 1, division);
    file.extend(mtrk(track_body));
    file
}

pub(super) fn single_note_midi(division: u16, duration: u32) -> Vec<u8> {
    let mut body = Vec::new();
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(&mut body, duration, running_note_off(60));
    single_track_midi(division, body)
}

pub(super) fn cfg() -> MidiCfgEntry {
    MidiCfgEntry {
        voicegroup_label: "test".to_owned(),
        priority: 0,
        reverb: None,
        master_volume: 127,
        exact_gate_time: true,
        clocks_per_beat: 1,
    }
}
