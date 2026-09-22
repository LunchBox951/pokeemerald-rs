use super::{decode_track, SongEvent};
use crate::{GbaPtr, RomReader, ROM_BASE};

fn pointer(bytes: &mut Vec<u8>, target: usize) {
    bytes.extend((ROM_BASE + u32::try_from(target).unwrap()).to_le_bytes());
}

fn decode(bytes: &[u8]) -> Vec<SongEvent> {
    let events = decode_track(
        &RomReader::new(bytes),
        "audio/song/state",
        0,
        GbaPtr::at(ROM_BASE),
    )
    .unwrap();
    let song = assets::Song::new(
        assets::VoiceGroupId("audio/voicegroup/g".into()),
        0,
        None,
        vec![events],
    )
    .unwrap();
    assets::Song::decode(&song.encode()).unwrap().tracks()[0].clone()
}

fn trace(events: &[SongEvent], count: usize, branch_taken: bool) -> Vec<SongEvent> {
    let mut pc = 0;
    let mut output = Vec::new();
    for _ in 0..100 {
        match &events[pc] {
            SongEvent::Goto(target) => {
                pc = usize::try_from(*target).unwrap();
                continue;
            }
            SongEvent::MemAccBranch {
                condition,
                address,
                data,
                target,
            } => {
                assert_eq!(*condition, assets::audio::MemAccCondition::Eq);
                assert_eq!((*address, *data), (0, 1));
                if branch_taken {
                    pc = usize::try_from(*target).unwrap();
                    continue;
                }
            }
            SongEvent::Fine => return output,
            event => output.push(event.clone()),
        }
        if output.len() == count {
            return output;
        }
        pc += 1;
    }
    panic!("track did not reach FINE or the requested trace length")
}

fn note(key: u8, velocity: u8, gate: u8) -> SongEvent {
    SongEvent::Note {
        key,
        velocity,
        gate,
    }
}

#[test]
fn backward_jump_inherits_the_previous_iteration_note() {
    let mut bytes = vec![0xCF, 60, 80, 0xCF, 0x81, 0xCF, 72, 100, 0x81, 0xB2];
    pointer(&mut bytes, 3);
    bytes.push(0xB1);
    assert_eq!(
        trace(&decode(&bytes), 9, false),
        vec![
            note(60, 80, 0),
            note(60, 80, 0),
            SongEvent::Wait(1),
            note(72, 100, 0),
            SongEvent::Wait(1),
            note(72, 100, 0),
            SongEvent::Wait(1),
            note(72, 100, 0),
            SongEvent::Wait(1),
        ]
    );
}

#[test]
fn forward_jump_uses_executed_running_status_and_skips_invalid_fallthrough() {
    let mut bytes = vec![0xD0, 60, 80, 0xB2];
    pointer(&mut bytes, 10);
    bytes.extend([0xBE, 127, 72, 100, 0x81, 0xB1]);
    assert_eq!(
        trace(&decode(&bytes), 3, false),
        vec![note(60, 80, 1), note(72, 100, 1), SongEvent::Wait(1),]
    );
    bytes[8] = 0xCC;
    assert_eq!(
        trace(&decode(&bytes), 3, false),
        vec![note(60, 80, 1), note(72, 100, 1), SongEvent::Wait(1),]
    );
}

#[test]
fn conditional_branch_decodes_both_instruction_widths_at_a_shared_target() {
    let mut bytes = vec![0xBD, 12, 0xB9, 6, 0, 1];
    pointer(&mut bytes, 14);
    bytes.extend([0xCF, 72, 127, 0x81, 60, 100, 5, 0x81, 0xB1]);
    let events = decode(&bytes);
    assert_eq!(
        trace(&events, 8, true),
        vec![
            SongEvent::Voice(12),
            SongEvent::Voice(60),
            SongEvent::Voice(100),
            SongEvent::Voice(5),
            SongEvent::Wait(1),
        ]
    );
    assert_eq!(
        trace(&events, 8, false),
        vec![
            SongEvent::Voice(12),
            note(72, 127, 0),
            SongEvent::Wait(1),
            note(60, 100, 5),
            SongEvent::Wait(1),
        ]
    );
}

#[test]
fn a_branch_inside_each_pattern_keeps_its_callers_state_and_return() {
    let mut bytes = vec![0xCF, 60, 80, 0xB3];
    pointer(&mut bytes, 17);
    bytes.extend([0xCF, 72, 100, 0xB3]);
    pointer(&mut bytes, 17);
    bytes.extend([0xB1, 0xB9, 6, 0, 1]);
    pointer(&mut bytes, 29);
    bytes.extend([0xCF, 64, 90, 0xB4, 0xCF, 0xB4]);
    let events = decode(&bytes);
    assert_eq!(
        trace(&events, 8, true),
        vec![
            note(60, 80, 0),
            note(60, 80, 0),
            note(72, 100, 0),
            note(72, 100, 0),
        ]
    );
    assert_eq!(
        trace(&events, 8, false),
        vec![
            note(60, 80, 0),
            note(64, 90, 0),
            note(72, 100, 0),
            note(64, 90, 0),
        ]
    );
}

#[test]
fn an_elided_tie_end_uses_the_key_from_the_taken_branch() {
    let mut bytes = vec![0xCF, 60, 80, 0xB9, 6, 0, 1];
    pointer(&mut bytes, 14);
    bytes.extend([0xCF, 72, 100, 0xCE, 0x81, 0xB1]);
    let events = decode(&bytes);
    assert_eq!(
        trace(&events, 8, true),
        vec![
            note(60, 80, 0),
            SongEvent::EndOfTie { key: Some(60) },
            SongEvent::Wait(1),
        ]
    );
    assert_eq!(
        trace(&events, 8, false),
        vec![
            note(60, 80, 0),
            note(72, 100, 0),
            SongEvent::EndOfTie { key: Some(72) },
            SongEvent::Wait(1),
        ]
    );
}

#[test]
fn the_existing_pattern_loop_preserves_both_branch_paths() {
    let rom = super::rom();
    let events = decode_track(&rom.reader(), "audio/song/s", 1, super::at(super::TRACK_B)).unwrap();
    let expected = vec![
        SongEvent::Voice(1),
        SongEvent::Volume(127),
        note(60, 100, 1),
        SongEvent::Wait(1),
        note(60, 100, 1),
        SongEvent::Wait(1),
        SongEvent::Wait(2),
        SongEvent::Volume(127),
        note(60, 100, 1),
        SongEvent::Wait(1),
        note(60, 100, 1),
        SongEvent::Wait(1),
        SongEvent::Wait(2),
    ];
    for taken in [false, true] {
        assert_eq!(trace(&events, expected.len(), taken), expected);
    }
}
