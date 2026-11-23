//! XCMD 0x0D seeds each subsequent DirectSound note's source position
//! (`ply_xcmd_0D`, `m4a.c:1639`..`:1653`; `ply_note`, `m4a_1.s:1791`..`:1797`).

use std::sync::Arc;

use super::*;
use crate::cgb_envelope::CgbAdsr;
use crate::envelope::Adsr;
use crate::sample::WaveData;
use crate::song::{SquareTone, ToneData};

use super::test_support::{apply_test_event, render_frames, tied_note};

/// Long enough that no test voice retires while it is inspected.
const WAVE_LEN_U32: u32 = 224 * 4;
const WAVE_LEN: usize = SAMPLES_PER_FRAME * 4;

/// A nonuniform, signed sawtooth with no period inside the first 200 indices,
/// so neighbouring indices rarely hold the same value.
fn ramp() -> Vec<i8> {
    (0..WAVE_LEN)
        .map(|i| i8::try_from(i32::try_from((i * 13) % 201).unwrap() - 100).unwrap())
        .collect()
}

fn sample_progress(value: u32) -> Event {
    Event::Xcmd {
        kind: XCMD_SAMPLE_PROGRESS,
        value,
    }
}

fn direct_sound_song(samples: Vec<i8>, fixed: bool, tracks: Vec<Vec<Event>>) -> Song {
    let mut tone = ToneData::new(Arc::new(WaveData::one_shot(1 << 20, samples)), Adsr::flat());
    if fixed {
        tone = tone.fixed();
    }
    Song::new(vec![Instrument::DirectSound(tone)], tracks, 150)
}

fn source_indices(seq: &Sequencer) -> Vec<usize> {
    seq.mixer
        .voices()
        .iter()
        .map(|v| v.source_index())
        .collect()
}

#[test]
fn the_wave_length_constants_agree() {
    assert_eq!(usize::try_from(WAVE_LEN_U32), Ok(WAVE_LEN));
}

#[test]
fn a_nonzero_value_starts_each_subsequent_note_there_and_never_moves_a_sounding_one() {
    let idle = vec![Event::Wait(96), Event::Goto(0)];
    let song = direct_sound_song(ramp(), true, vec![idle]);
    let mut seq = Sequencer::new(song);

    apply_test_event(&mut seq, 0, &Event::Voice(0));
    apply_test_event(&mut seq, 0, &sample_progress(3));
    apply_test_event(&mut seq, 0, &tied_note(60));
    assert_eq!(source_indices(&seq), [3]);

    apply_test_event(&mut seq, 0, &sample_progress(100));
    assert_eq!(
        source_indices(&seq),
        [3],
        "a later change must not reposition a sounding note"
    );
    apply_test_event(&mut seq, 0, &tied_note(64));
    assert_eq!(source_indices(&seq), [3, 100]);

    // Both fixed-rate voices advance in lockstep from their own starts.
    render_frames(&mut seq, 1);
    let indices = source_indices(&seq);
    assert_eq!(indices[1] - indices[0], 97);
}

#[test]
fn the_start_is_the_default_of_zero_until_the_command_runs_and_zero_restores_it() {
    let song = direct_sound_song(ramp(), true, vec![vec![]]);
    let mut seq = Sequencer::new(song);
    apply_test_event(&mut seq, 0, &Event::Voice(0));
    apply_test_event(&mut seq, 0, &tied_note(60));
    apply_test_event(&mut seq, 0, &sample_progress(9));
    apply_test_event(&mut seq, 0, &tied_note(62));
    apply_test_event(&mut seq, 0, &sample_progress(0));
    apply_test_event(&mut seq, 0, &tied_note(64));
    assert_eq!(source_indices(&seq), [0, 9, 0]);
}

#[test]
fn the_progress_belongs_to_one_track() {
    let song = direct_sound_song(ramp(), true, vec![vec![], vec![]]);
    let mut seq = Sequencer::new(song);
    for track in 0..2 {
        apply_test_event(&mut seq, track, &Event::Voice(0));
    }
    apply_test_event(&mut seq, 0, &sample_progress(7));
    apply_test_event(&mut seq, 1, &tied_note(64));
    apply_test_event(&mut seq, 0, &tied_note(60));
    apply_test_event(&mut seq, 1, &sample_progress(21));
    apply_test_event(&mut seq, 1, &tied_note(65));

    let start_of = |track: usize, key: u8| {
        seq.mixer
            .voices()
            .iter()
            .find(|v| v.track() == Some(track) && v.midi_key() == key)
            .map(|v| v.source_index())
    };
    assert_eq!(start_of(1, 64), Some(0), "track 1 had not run the command");
    assert_eq!(start_of(0, 60), Some(7));
    assert_eq!(start_of(1, 65), Some(21));
}

/// Render `track` against `samples`, concatenating `frames` frames.
fn render_song(samples: Vec<i8>, fixed: bool, track: Vec<Event>, frames: usize) -> Vec<f32> {
    let mut seq = Sequencer::new(direct_sound_song(samples, fixed, vec![track]));
    let mut all = Vec::new();
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
    for _ in 0..frames {
        seq.render_frame(&mut out);
        all.extend_from_slice(&out);
    }
    all
}

fn long_note_track(progress: Option<u32>) -> Vec<Event> {
    let mut track = vec![Event::Voice(0), Event::Volume(127)];
    track.extend(progress.map(sample_progress));
    track.extend([tied_note(60), Event::Wait(96), Event::Fine]);
    track
}

#[test]
fn a_seeded_note_renders_the_tail_of_the_wave_and_retires_with_it() {
    let wave = ramp();
    let progress = 37;
    let tail = wave[progress as usize..].to_vec();
    for fixed in [true, false] {
        // Seeding at `progress` over the full wave must equal an unseeded note
        // over the wave's tail: same PCM, same remaining lifetime.
        let seeded = render_song(wave.clone(), fixed, long_note_track(Some(progress)), 6);
        let tail_only = render_song(tail.clone(), fixed, long_note_track(None), 6);
        let unseeded = render_song(wave.clone(), fixed, long_note_track(None), 6);
        assert_eq!(seeded, tail_only, "fixed_rate: {fixed}");
        assert_ne!(seeded, unseeded, "fixed_rate: {fixed}");
    }
}

#[test]
fn a_value_past_the_wave_never_sounds_and_leaves_the_sounding_note_alone() {
    let song = direct_sound_song(ramp(), true, vec![vec![]]);
    let mut seq = Sequencer::with_config(song, DEFAULT_MASTER_VOLUME, 1);
    apply_test_event(&mut seq, 0, &Event::Voice(0));
    apply_test_event(&mut seq, 0, &tied_note(60));

    for value in [WAVE_LEN_U32, WAVE_LEN_U32 + 1, u32::MAX] {
        apply_test_event(&mut seq, 0, &sample_progress(value));
        apply_test_event(&mut seq, 0, &tied_note(64));
        assert_eq!(seq.voice_count(), 1, "value {value}: no voice was added");
        assert!(
            !seq.mixer.voices()[0].is_stopping(),
            "value {value}: the refused note must not steal the only channel"
        );
        assert_eq!(seq.mixer.voices()[0].midi_key(), 60);
    }

    // The last sample is still supported.
    apply_test_event(&mut seq, 0, &sample_progress(WAVE_LEN_U32 - 1));
    apply_test_event(&mut seq, 0, &tied_note(65));
    assert!(seq
        .mixer
        .voices()
        .iter()
        .any(|v| v.midi_key() == 65 && v.source_index() == WAVE_LEN - 1));
}

#[test]
fn a_decoded_command_sets_the_little_endian_value() {
    let bytes = [0xCD, 0x0D, 0x34, 0x12, 0x00, 0x00, 0xB1];
    let events = crate::decode_track(&bytes).unwrap();
    assert_eq!(events[0], sample_progress(0x1234));
}

#[test]
fn cgb_notes_ignore_the_progress() {
    let square = || {
        Instrument::CgbSquare1(SquareTone {
            duty: 2,
            sweep: 0,
            adsr: CgbAdsr::flat(),
            fixed_rate: false,
            length: 0,
        })
    };
    let render = |progress: Option<u32>| {
        let mut track = vec![Event::Voice(0), Event::Volume(127)];
        track.extend(progress.map(sample_progress));
        track.extend([
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 8,
            },
            Event::Wait(48),
            Event::Fine,
        ]);
        let mut seq = Sequencer::new(Song::new(vec![square()], vec![track], 150));
        let mut all = Vec::new();
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        for _ in 0..3 {
            seq.render_frame(&mut out);
            all.extend_from_slice(&out);
        }
        assert_eq!(seq.voice_count(), 1, "progress {progress:?}");
        all
    };
    let silent_baseline = render(None);
    assert!(silent_baseline.iter().any(|&s| s != 0.0));
    // Even a value no DirectSound wave could honour does not refuse the note.
    assert_eq!(render(Some(5)), silent_baseline);
    assert_eq!(render(Some(u32::MAX)), silent_baseline);
}
