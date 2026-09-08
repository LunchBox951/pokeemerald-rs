use super::{compile, MidiError, SongEvent};
use crate::extract::midi::cfg::MidiCfgEntry;

const META_EVENT: u8 = 0xFF;
const END_OF_TRACK_META_EVENT: u8 = 0x2F;
const TEXT_META_EVENT: u8 = 0x01;
const TEMPO_META_EVENT: u8 = 0x51;
const TIME_SIGNATURE_META_EVENT: u8 = 0x58;
const MODULATION_CONTROLLER: u8 = 0x01;
const VOLUME_CONTROLLER: u8 = 0x07;
const MEMACC_CONTROLLERS: [u8; 6] = [0x0C, 0x0D, 0x0E, 0x0F, 0x10, 0x11];
const LFO_SPEED_CONTROLLER: u8 = 0x15;
const EXTENDED_COMMAND_TRIGGER: u8 = 0x1D;
const EXTENDED_COMMAND_SELECTOR: u8 = 0x1E;
const ALTERNATE_EXTENDED_COMMAND_TRIGGER: u8 = 0x1F;
const PSEUDO_ECHO_VOLUME_COMMAND: u8 = 8;
const PSEUDO_ECHO_LENGTH_COMMAND: u8 = 9;
const UNKNOWN_CONTROLLER: u8 = 0x50;
const MAX_STANDARD_VLQ: u32 = 0x0FFF_FFFF;
const OVERLONG_ZERO_VLQ: [u8; 5] = [0x90, 0x80, 0x80, 0x80, 0x00];

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

fn note_on(channel: u8, key: u8, velocity: u8) -> [u8; 3] {
    [0x90 | channel, key, velocity]
}

fn note_off(channel: u8, key: u8) -> [u8; 3] {
    [0x80 | channel, key, 0]
}

fn running_note_off(key: u8) -> [u8; 2] {
    [key, 0]
}

fn control_change(channel: u8, controller: u8, value: u8) -> [u8; 3] {
    [0xB0 | channel, controller, value]
}

fn running_control_change(controller: u8, value: u8) -> [u8; 2] {
    [controller, value]
}

fn program_change(channel: u8, program: u8) -> [u8; 2] {
    [0xC0 | channel, program]
}

fn pitch_bend(channel: u8, lsb: u8, msb: u8) -> [u8; 3] {
    [0xE0 | channel, lsb, msb]
}

fn tempo(microseconds_per_quarter_note: u32) -> Vec<u8> {
    let bytes = microseconds_per_quarter_note.to_be_bytes();
    meta_event(TEMPO_META_EVENT, &bytes[1..])
}

fn time_signature(numerator: u8, denominator_exponent: u8) -> Vec<u8> {
    meta_event(
        TIME_SIGNATURE_META_EVENT,
        &[numerator, denominator_exponent, 24, 8],
    )
}

fn text_event(text: &[u8]) -> Vec<u8> {
    meta_event(TEXT_META_EVENT, text)
}

fn push_timed(body: &mut Vec<u8>, delta: u32, event: impl IntoIterator<Item = u8>) {
    body.extend(vlq(delta));
    body.extend(event);
}

fn push_max_delta_empty_text_events(body: &mut Vec<u8>, count: usize) {
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

fn single_track_midi(division: u16, track_body: Vec<u8>) -> Vec<u8> {
    let mut file = mthd(0, 1, division);
    file.extend(mtrk(track_body));
    file
}

fn single_note_midi(division: u16, duration: u32) -> Vec<u8> {
    let mut body = Vec::new();
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(&mut body, duration, running_note_off(60));
    single_track_midi(division, body)
}

fn cfg() -> MidiCfgEntry {
    MidiCfgEntry {
        voicegroup_label: "test".to_owned(),
        priority: 0,
        reverb: None,
        master_volume: 127,
        exact_gate_time: true,
        clocks_per_beat: 1,
    }
}

#[test]
fn a_single_note_compiles_to_the_expected_event_stream() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(&mut body, 24, running_note_off(60));
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    assert_eq!(compiled.tracks.len(), 1);
    assert_eq!(
        compiled.tracks[0],
        vec![
            SongEvent::Volume(127),
            SongEvent::KeyShift(0),
            SongEvent::Note {
                key: 60,
                velocity: 100,
                gate: 24
            },
            SongEvent::Wait(24),
            SongEvent::Fine,
        ]
    );
}

#[test]
fn a_volume_controller_before_the_note_suppresses_the_synthetic_preamble() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, control_change(0, VOLUME_CONTROLLER, 100));
    push_timed(&mut body, 0, note_on(0, 60, 96));
    push_timed(&mut body, 10, note_off(0, 60));
    let midi = single_track_midi(24, body);

    let mut entry = cfg();
    entry.master_volume = 90;
    let compiled = compile(&midi, &entry).unwrap();
    assert_eq!(
        compiled.tracks[0],
        vec![
            SongEvent::KeyShift(0),
            SongEvent::Volume(70),
            SongEvent::Note {
                key: 60,
                velocity: 96,
                gate: 10
            },
            SongEvent::Wait(10),
            SongEvent::Fine,
        ]
    );
}

#[test]
fn a_long_note_is_tie_split_with_an_explicit_end_of_tie_key() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, note_on(0, 67, 80));
    push_timed(&mut body, 120, running_note_off(67));
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    assert_eq!(
        compiled.tracks[0],
        vec![
            SongEvent::Volume(127),
            SongEvent::KeyShift(0),
            SongEvent::Note {
                key: 67,
                velocity: 80,
                gate: 0
            },
            SongEvent::Wait(120),
            SongEvent::EndOfTie { key: 67 },
            SongEvent::Fine,
        ]
    );
}

#[test]
fn a_gap_over_255_ticks_is_split_into_multiple_waits() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(&mut body, 1, running_note_off(60));
    push_timed(&mut body, 300, note_on(0, 62, 100));
    push_timed(&mut body, 1, running_note_off(62));
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    let waits: Vec<u8> = compiled.tracks[0]
        .iter()
        .filter_map(|e| match e {
            SongEvent::Wait(t) => Some(*t),
            _ => None,
        })
        .collect();
    assert!(waits.contains(&255));
    assert_eq!(
        waits.iter().map(|&w| u32::from(w)).sum::<u32>(),
        1 + 300 + 1
    );
}

#[test]
fn loop_markers_compile_to_a_backward_goto() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, text_event(b"["));
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(&mut body, 4, running_note_off(60));
    push_timed(&mut body, 0, text_event(b"]"));
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    assert_eq!(
        compiled.tracks[0],
        vec![
            SongEvent::Volume(127),
            SongEvent::KeyShift(0),
            SongEvent::Note {
                key: 60,
                velocity: 100,
                gate: 4
            },
            SongEvent::Wait(4),
            SongEvent::Goto(2),
            SongEvent::Fine,
        ]
    );
}

#[test]
fn a_loop_end_with_no_matching_loop_begin_is_an_error() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(&mut body, 4, running_note_off(60));
    push_timed(&mut body, 0, text_event(b"]"));
    let midi = single_track_midi(24, body);

    let err = compile(&midi, &cfg()).unwrap_err();
    assert_eq!(err, MidiError::DanglingLoopEnd);
}

#[test]
fn xcmd_pseudo_echo_volume_round_trips_and_unknown_subcommands_are_silent() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(
        &mut body,
        4,
        control_change(0, EXTENDED_COMMAND_SELECTOR, PSEUDO_ECHO_VOLUME_COMMAND),
    );
    push_timed(
        &mut body,
        0,
        control_change(0, EXTENDED_COMMAND_TRIGGER, 10),
    );
    push_timed(
        &mut body,
        0,
        control_change(0, EXTENDED_COMMAND_SELECTOR, 10),
    );
    push_timed(
        &mut body,
        0,
        control_change(0, ALTERNATE_EXTENDED_COMMAND_TRIGGER, 5),
    );
    push_timed(&mut body, 4, note_off(0, 60));
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    assert!(compiled.tracks[0].contains(&SongEvent::PseudoEchoVolume(10)));
    assert_eq!(
        compiled.tracks[0]
            .iter()
            .filter(|e| matches!(
                e,
                SongEvent::PseudoEchoVolume(_) | SongEvent::PseudoEchoLength(_)
            ))
            .count(),
        1
    );
}

#[test]
fn an_extended_command_selector_discards_a_fixed_point_gap() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(
        &mut body,
        4,
        control_change(0, EXTENDED_COMMAND_SELECTOR, PSEUDO_ECHO_VOLUME_COMMAND),
    );
    push_timed(
        &mut body,
        24,
        running_control_change(EXTENDED_COMMAND_TRIGGER, 10),
    );
    push_timed(&mut body, 6, note_off(0, 60));
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    assert_eq!(
        compiled.tracks[0],
        vec![
            SongEvent::Volume(127),
            SongEvent::KeyShift(0),
            SongEvent::Note {
                key: 60,
                velocity: 100,
                gate: 34
            },
            SongEvent::Wait(4),
            SongEvent::PseudoEchoVolume(10),
            SongEvent::Wait(6),
            SongEvent::Fine,
        ]
    );
}

#[test]
fn an_extended_command_selector_keeps_an_off_grid_remainder() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(
        &mut body,
        4,
        control_change(0, EXTENDED_COMMAND_SELECTOR, PSEUDO_ECHO_VOLUME_COMMAND),
    );
    push_timed(
        &mut body,
        27,
        running_control_change(EXTENDED_COMMAND_TRIGGER, 10),
    );
    push_timed(&mut body, 6, note_off(0, 60));
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    assert_eq!(
        compiled.tracks[0],
        vec![
            SongEvent::Volume(127),
            SongEvent::KeyShift(0),
            SongEvent::Note {
                key: 60,
                velocity: 100,
                gate: 37
            },
            SongEvent::Wait(7), // the two rests merge: canonical waits
            SongEvent::PseudoEchoVolume(10),
            SongEvent::Wait(6),
            SongEvent::Fine,
        ]
    );
}

#[test]
fn an_extended_command_selector_gap_stops_at_the_timing_grid() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(&mut body, 4, running_note_off(60));
    push_timed(
        &mut body,
        15,
        control_change(0, EXTENDED_COMMAND_SELECTOR, PSEUDO_ECHO_LENGTH_COMMAND),
    );
    push_timed(
        &mut body,
        100,
        running_control_change(ALTERNATE_EXTENDED_COMMAND_TRIGGER, 12),
    );
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    assert_eq!(
        compiled.tracks[0],
        vec![
            SongEvent::Volume(127),
            SongEvent::KeyShift(0),
            SongEvent::Note {
                key: 60,
                velocity: 100,
                gate: 4
            },
            SongEvent::Wait(43), // the two rests merge: canonical waits
            SongEvent::PseudoEchoLength(12),
            SongEvent::Fine,
        ]
    );
}

#[test]
fn an_extended_command_selector_near_a_grid_line_preserves_later_ticks() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(&mut body, 4, running_note_off(60));
    push_timed(
        &mut body,
        86,
        control_change(0, EXTENDED_COMMAND_SELECTOR, PSEUDO_ECHO_VOLUME_COMMAND),
    );
    push_timed(
        &mut body,
        20,
        running_control_change(EXTENDED_COMMAND_TRIGGER, 10),
    );
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    assert_eq!(
        compiled.tracks[0],
        vec![
            SongEvent::Volume(127),
            SongEvent::KeyShift(0),
            SongEvent::Note {
                key: 60,
                velocity: 100,
                gate: 4
            },
            SongEvent::Wait(104), // the two rests merge: canonical waits
            SongEvent::PseudoEchoVolume(10),
            SongEvent::Fine,
        ]
    );
}

#[test]
fn a_time_signature_rephases_the_extended_command_timing_grid() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, time_signature(2, 2));
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(&mut body, 4, running_note_off(60));
    push_timed(
        &mut body,
        36,
        control_change(0, EXTENDED_COMMAND_SELECTOR, PSEUDO_ECHO_VOLUME_COMMAND),
    );
    push_timed(
        &mut body,
        30,
        running_control_change(EXTENDED_COMMAND_TRIGGER, 10),
    );
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    assert_eq!(
        compiled.tracks[0],
        vec![
            SongEvent::Volume(127),
            SongEvent::KeyShift(0),
            SongEvent::Note {
                key: 60,
                velocity: 100,
                gate: 4
            },
            SongEvent::Wait(62), // the two rests merge: canonical waits
            SongEvent::PseudoEchoVolume(10),
            SongEvent::Fine,
        ]
    );
}

#[test]
fn a_zero_period_time_signature_is_an_error() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, time_signature(1, 7));
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(&mut body, 4, running_note_off(60));
    let midi = single_track_midi(24, body);

    assert_eq!(
        compile(&midi, &cfg()).unwrap_err(),
        MidiError::ZeroTimeSignature
    );
}

#[test]
fn a_silent_controller_after_an_extended_command_selector_keeps_its_wait() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(
        &mut body,
        4,
        control_change(0, EXTENDED_COMMAND_SELECTOR, PSEUDO_ECHO_VOLUME_COMMAND),
    );
    push_timed(&mut body, 10, running_control_change(UNKNOWN_CONTROLLER, 3));
    push_timed(
        &mut body,
        10,
        running_control_change(EXTENDED_COMMAND_TRIGGER, 10),
    );
    push_timed(&mut body, 6, note_off(0, 60));
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    assert_eq!(
        compiled.tracks[0],
        vec![
            SongEvent::Volume(127),
            SongEvent::KeyShift(0),
            SongEvent::Note {
                key: 60,
                velocity: 100,
                gate: 30
            },
            SongEvent::Wait(14), // the two rests merge: canonical waits
            SongEvent::PseudoEchoVolume(10),
            SongEvent::Wait(6),
            SongEvent::Fine,
        ]
    );
}

#[test]
fn waits_not_adjacent_to_an_extended_command_selector_are_unaffected() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(&mut body, 4, running_note_off(60));
    push_timed(&mut body, 10, control_change(0, LFO_SPEED_CONTROLLER, 7));
    push_timed(
        &mut body,
        5,
        running_control_change(EXTENDED_COMMAND_SELECTOR, PSEUDO_ECHO_LENGTH_COMMAND),
    );
    push_timed(
        &mut body,
        40,
        running_control_change(ALTERNATE_EXTENDED_COMMAND_TRIGGER, 12),
    );
    push_timed(
        &mut body,
        300,
        running_control_change(MODULATION_CONTROLLER, 55),
    );
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    assert_eq!(
        compiled.tracks[0],
        vec![
            SongEvent::Volume(127),
            SongEvent::KeyShift(0),
            SongEvent::Note {
                key: 60,
                velocity: 100,
                gate: 4
            },
            SongEvent::Wait(14),
            SongEvent::LfoSpeed(7),
            SongEvent::Wait(5),
            SongEvent::PseudoEchoLength(12),
            SongEvent::Wait(u8::MAX),
            SongEvent::Wait(45),
            SongEvent::Modulation(55),
            SongEvent::Fine,
        ]
    );
}

#[test]
fn memacc_controllers_are_a_hard_error() {
    for controller in MEMACC_CONTROLLERS {
        let mut body = Vec::new();
        push_timed(&mut body, 0, note_on(0, 60, 100));
        push_timed(&mut body, 4, control_change(0, controller, 1));
        push_timed(&mut body, 0, note_off(0, 60));
        let midi = single_track_midi(24, body);
        let err = compile(&midi, &cfg()).unwrap_err();
        assert_eq!(err, MidiError::UnsupportedMemAccController(controller));
    }
}

#[test]
fn an_unterminated_note_is_an_error() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, note_on(0, 60, 100));
    let midi = single_track_midi(24, body);
    let err = compile(&midi, &cfg()).unwrap_err();
    assert_eq!(
        err,
        MidiError::UnterminatedNote {
            channel: 0,
            key: 60
        }
    );
}

#[test]
fn non_exact_gate_time_is_rejected() {
    let midi = single_note_midi(24, 1);
    let mut entry = cfg();
    entry.exact_gate_time = false;
    let err = compile(&midi, &entry).unwrap_err();
    assert_eq!(err, MidiError::NonExactGateTime);
}

#[test]
fn unsupported_clocks_per_beat_is_rejected() {
    let midi = single_note_midi(24, 1);
    let mut entry = cfg();
    entry.clocks_per_beat = 2;
    let err = compile(&midi, &entry).unwrap_err();
    assert_eq!(err, MidiError::UnsupportedClocksPerBeat(2));
}

#[test]
fn tempo_only_reaches_the_first_agb_track() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, tempo(500_000));
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(&mut body, 4, running_note_off(60));
    push_timed(&mut body, 0, note_on(1, 64, 100));
    push_timed(&mut body, 4, note_off(1, 64));
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    assert_eq!(compiled.tracks.len(), 2);
    assert!(compiled.tracks[0].contains(&SongEvent::Tempo(120)));
    assert!(!compiled.tracks[1]
        .iter()
        .any(|e| matches!(e, SongEvent::Tempo(_))));
}

#[test]
fn song_level_metadata_is_carried_from_cfg() {
    let midi = single_note_midi(24, 1);
    let mut entry = cfg();
    entry.voicegroup_label = "title".to_owned();
    entry.priority = 3;
    entry.reverb = Some(50);
    let compiled = compile(&midi, &entry).unwrap();
    assert_eq!(compiled.voicegroup_label, "title");
    assert_eq!(compiled.priority, 3);
    assert_eq!(compiled.reverb, Some(50));
}

#[test]
fn a_channel_of_zero_duration_notes_emits_no_track() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, tempo(500_000));
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(&mut body, 0, note_off(0, 60));
    push_timed(&mut body, 0, note_on(1, 64, 100));
    push_timed(&mut body, 4, note_off(1, 64));
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    assert_eq!(compiled.tracks.len(), 1);
    assert!(compiled.tracks[0].contains(&SongEvent::Tempo(120)));
    assert!(compiled.tracks[0].contains(&SongEvent::Note {
        key: 64,
        velocity: 100,
        gate: 4,
    }));
}

#[test]
fn a_channel_with_no_notes_emits_no_track() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, control_change(0, VOLUME_CONTROLLER, 100));
    push_timed(&mut body, 0, note_on(1, 64, 100));
    push_timed(&mut body, 4, note_off(1, 64));
    let midi = single_track_midi(24, body);

    assert_eq!(compile(&midi, &cfg()).unwrap().tracks.len(), 1);
}

#[test]
fn convert_ticks_evaluates_the_multiply_in_u64() {
    use crate::extract::midi::translate::convert_ticks;

    assert_eq!(
        convert_ticks(MAX_STANDARD_VLQ, 24).unwrap(),
        MAX_STANDARD_VLQ
    );
    assert_eq!(
        convert_ticks(MAX_STANDARD_VLQ, 1).unwrap_err(),
        MidiError::TickOverflow(MAX_STANDARD_VLQ)
    );
}

#[test]
fn a_tick_that_overflows_once_scaled_is_an_error_not_a_panic() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(&mut body, MAX_STANDARD_VLQ, note_off(0, 60));
    let midi = single_track_midi(1, body);

    assert_eq!(
        compile(&midi, &cfg()).unwrap_err(),
        MidiError::TickOverflow(MAX_STANDARD_VLQ)
    );
}

#[test]
fn a_grid_mark_past_u32_max_is_an_error_not_a_hang() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(&mut body, 0x0F, note_off(0, 60));
    push_max_delta_empty_text_events(&mut body, 16);
    push_timed(&mut body, 0, time_signature(4, 2));
    push_timed(&mut body, 0, control_change(0, VOLUME_CONTROLLER, 100));
    let midi = single_track_midi(24, body);

    assert_eq!(
        compile(&midi, &cfg()).unwrap_err(),
        MidiError::TickOverflow(u32::MAX)
    );
}

#[test]
fn a_unit_grid_period_reaches_a_distant_event_without_iterating() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, time_signature(1, 6));
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(&mut body, 0x0F, note_off(0, 60));
    push_max_delta_empty_text_events(&mut body, 16);
    push_timed(&mut body, 0, control_change(0, VOLUME_CONTROLLER, 100));
    let midi = single_track_midi(24, body);

    assert_eq!(
        compile(&midi, &cfg()).unwrap_err(),
        MidiError::TickOverflow(u32::MAX)
    );
}

#[test]
fn malformed_tempo_and_velocity_are_errors_at_the_compile_boundary() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, tempo(0));
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(&mut body, 1, note_off(0, 60));
    let zero_tempo = single_track_midi(24, body);
    assert_eq!(
        compile(&zero_tempo, &cfg()).unwrap_err(),
        MidiError::ZeroTempo
    );

    let mut body = Vec::new();
    push_timed(&mut body, 0, note_on(0, 60, 128));
    push_timed(&mut body, 1, note_off(0, 60));
    let invalid_velocity = single_track_midi(24, body);
    assert_eq!(
        compile(&invalid_velocity, &cfg()).unwrap_err(),
        MidiError::InvalidDataByte(128)
    );
}

#[test]
fn channel_events_are_scaled_to_the_output_timebase() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(&mut body, 12, program_change(0, 3));
    push_timed(&mut body, 12, control_change(0, MODULATION_CONTROLLER, 7));
    push_timed(&mut body, 12, pitch_bend(0, 0, 80));
    push_timed(&mut body, 12, note_off(0, 60));
    let midi = single_track_midi(48, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    assert_eq!(
        compiled.tracks[0],
        vec![
            SongEvent::Volume(127),
            SongEvent::KeyShift(0),
            SongEvent::Note {
                key: 60,
                velocity: 100,
                gate: 24,
            },
            SongEvent::Wait(6),
            SongEvent::Voice(3),
            SongEvent::Wait(6),
            SongEvent::Modulation(7),
            SongEvent::Wait(6),
            SongEvent::Bend(16),
            SongEvent::Wait(6),
            SongEvent::Fine,
        ]
    );
}

#[test]
fn an_over_long_zero_delta_preserves_the_prior_absolute_tick() {
    use crate::extract::midi::parse::{parse_track, RawEvent};

    let mut body = Vec::new();
    push_timed(&mut body, 1, note_on(0, 60, 100));
    body.extend(OVERLONG_ZERO_VLQ);
    body.extend(program_change(0, 5));
    push_timed(&mut body, 24, note_off(0, 60));

    let mut parse_body = body.clone();
    push_timed(&mut parse_body, 0, meta_event(END_OF_TRACK_META_EVENT, &[]));
    let parsed = parse_track(&parse_body).unwrap();
    assert_eq!(
        parsed.events[1],
        (
            1,
            RawEvent::ProgramChange {
                channel: 0,
                program: 5,
            },
        )
    );

    let compiled = compile(&single_track_midi(24, body), &cfg()).unwrap();
    assert_eq!(
        compiled.tracks[0],
        vec![
            SongEvent::Volume(127),
            SongEvent::Wait(1),
            SongEvent::KeyShift(0),
            SongEvent::Voice(5),
            SongEvent::Note {
                key: 60,
                velocity: 100,
                gate: 24,
            },
            SongEvent::Wait(24),
            SongEvent::Fine,
        ]
    );
}
