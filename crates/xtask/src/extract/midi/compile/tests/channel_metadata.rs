//! Tempo and song metadata placement and which channels emit tracks.

use super::super::{compile, SongEvent};
use super::support::*;

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
