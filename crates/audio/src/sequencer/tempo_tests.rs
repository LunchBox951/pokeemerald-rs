//! The player tempo multiplier (`m4aMPlayTempoControl`, `m4a.c:1234`-`:1242`)
//! and its interaction with the `TEMPO` command (`m4a_1.s:920`-`:929`).

use super::test_support::*;
use super::*;
use crate::sequence::MAX_TEMPO_BPM;

/// One track that waits as long as a `Wait` allows, so each consumed tick
/// shows as one unit off `tracks[0].wait`.
fn long_wait_track() -> Vec<Event> {
    vec![Event::Wait(u8::MAX), Event::Fine]
}

/// Ticks each of `frames` frames runs, read off the track's wait counter.
fn ticks_per_frame(seq: &mut Sequencer, frames: usize) -> Vec<u16> {
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
    let mut previous = u16::from(u8::MAX);
    let mut ticks = Vec::new();
    for _ in 0..frames {
        seq.render_frame(&mut out);
        let wait = seq.tracks[0].wait;
        // Until the first tick decodes `Wait`, the counter reads zero.
        if wait == 0 && previous == u16::from(u8::MAX) {
            ticks.push(0);
        } else {
            ticks.push(previous - wait);
            previous = wait;
        }
    }
    ticks
}

fn sequencer_at(base: u16, multiplier: u16) -> Sequencer {
    let mut seq = Sequencer::new(test_song(vec![long_wait_track()], base));
    seq.set_tempo_multiplier(multiplier);
    seq
}

#[test]
fn a_new_sequencer_starts_at_the_unity_multiplier() {
    let seq = sequencer_at(150, 0x100);
    assert_eq!(seq.tempo_multiplier(), 256);
    assert_eq!(seq.base_tempo(), 150);
    assert_eq!(seq.effective_tempo(), 150);
    let fresh = Sequencer::new(test_song(vec![long_wait_track()], 150));
    assert_eq!(fresh.tempo_multiplier(), 256, "the default is 0x100");
}

#[test]
fn the_multiplier_scales_the_base_tempo_by_the_shifted_product() {
    for (multiplier, effective) in [(128, 75), (256, 150), (384, 225)] {
        let seq = sequencer_at(150, multiplier);
        assert_eq!(seq.base_tempo(), 150, "the base tempo is kept apart");
        assert_eq!(seq.effective_tempo(), effective, "multiplier {multiplier}");
    }
}

#[test]
fn a_half_multiplier_ticks_every_other_frame() {
    let mut seq = sequencer_at(150, 128);
    assert_eq!(ticks_per_frame(&mut seq, 6), [0, 1, 0, 1, 0, 1]);
}

#[test]
fn the_unity_multiplier_ticks_once_per_frame() {
    let mut seq = sequencer_at(150, 256);
    assert_eq!(ticks_per_frame(&mut seq, 6), [1; 6]);
}

#[test]
fn a_three_halves_multiplier_alternates_one_and_two_ticks() {
    let mut seq = sequencer_at(150, 384);
    assert_eq!(ticks_per_frame(&mut seq, 6), [1, 2, 1, 2, 1, 2]);
}

#[test]
fn a_tempo_command_changes_the_base_and_keeps_the_multiplier() {
    let mut seq = sequencer_at(150, 384);
    apply_test_event(&mut seq, 0, &Event::Tempo(100));
    assert_eq!(seq.base_tempo(), 100);
    assert_eq!(seq.tempo_multiplier(), 384, "TEMPO leaves the multiplier");
    assert_eq!(seq.effective_tempo(), 150, "(100 * 384) >> 8");
}

#[test]
fn a_tempo_command_in_the_stream_keeps_the_multiplier_for_later_ticks() {
    let track = vec![Event::Tempo(100), Event::Wait(u8::MAX), Event::Fine];
    let mut seq = Sequencer::new(test_song(vec![track], 150));
    seq.set_tempo_multiplier(384);
    // Frame 1 runs the TEMPO command on its first tick; the accumulator
    // then runs at the recomputed 150 from frame 2 on.
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
    seq.render_frame(&mut out);
    assert_eq!(seq.effective_tempo(), 150);
    assert_eq!(seq.tempo_multiplier(), 384);
    let after_first = seq.tracks[0].wait;
    seq.render_frame(&mut out);
    assert_eq!(
        after_first - seq.tracks[0].wait,
        1,
        "150 per frame: one tick"
    );
}

#[test]
fn a_multiplier_change_after_a_tempo_command_uses_the_new_base() {
    let mut seq = sequencer_at(150, 128);
    apply_test_event(&mut seq, 0, &Event::Tempo(200));
    assert_eq!(seq.effective_tempo(), 100);
    seq.set_tempo_multiplier(256);
    assert_eq!(seq.effective_tempo(), 200);
}

#[test]
fn the_effective_tempo_is_the_same_whichever_order_sets_it() {
    let mut multiplier_first = sequencer_at(150, 300);
    apply_test_event(&mut multiplier_first, 0, &Event::Tempo(77));
    let mut tempo_first = sequencer_at(150, 256);
    apply_test_event(&mut tempo_first, 0, &Event::Tempo(77));
    tempo_first.set_tempo_multiplier(300);
    assert_eq!(multiplier_first.effective_tempo(), (77 * 300) >> 8);
    assert_eq!(
        multiplier_first.effective_tempo(),
        tempo_first.effective_tempo()
    );
}

#[test]
fn two_sequencers_do_not_share_a_multiplier() {
    let mut fast = sequencer_at(150, 512);
    let slow = sequencer_at(150, 128);
    assert_eq!(fast.effective_tempo(), 300);
    assert_eq!(slow.effective_tempo(), 75);
    fast.set_tempo_multiplier(256);
    assert_eq!(slow.tempo_multiplier(), 128);
    assert_eq!(slow.effective_tempo(), 75);
    let untouched = Sequencer::new(test_song(vec![long_wait_track()], 150));
    assert_eq!(untouched.tempo_multiplier(), 256);
}

#[test]
fn a_zero_multiplier_freezes_sequence_progress_but_not_the_mixer() {
    let track = vec![
        Event::Voice(0),
        Event::Volume(127),
        tied_note(60),
        Event::Wait(u8::MAX),
        Event::Fine,
    ];
    let mut seq = Sequencer::new(test_song(vec![track], 150));
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
    seq.render_frame(&mut out);
    assert_eq!(seq.voice_count(), 1, "the tied note is sounding");

    seq.set_tempo_multiplier(0);
    assert_eq!(seq.effective_tempo(), 0);
    let wait = seq.tracks[0].wait;
    let cursor = seq.tracks[0].cursor;
    let accumulator = seq.tempo_c;
    // The test voice's one-shot sample lasts four frames.
    for _ in 0..2 {
        seq.render_frame(&mut out);
        assert!(
            out.iter().any(|sample| *sample != 0.0),
            "the mixer keeps sounding the existing voice"
        );
    }
    assert_eq!(seq.tracks[0].wait, wait, "no tick consumed the wait");
    assert_eq!(seq.tracks[0].cursor, cursor, "no command ran");
    assert_eq!(seq.tempo_c, accumulator, "the accumulator is not reset");
    assert_eq!(seq.voice_count(), 1);

    seq.set_tempo_multiplier(256);
    seq.render_frame(&mut out);
    assert_eq!(seq.tracks[0].wait, wait - 1, "progress resumes");
}

#[test]
fn restoring_the_multiplier_resumes_from_the_held_accumulator() {
    // Base 100 leaves tempo_c at 100 after one frame, 50 short of a tick.
    let mut seq = sequencer_at(100, 256);
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
    seq.render_frame(&mut out);
    assert_eq!(seq.tempo_c, 100);
    seq.set_tempo_multiplier(0);
    seq.render_frame(&mut out);
    assert_eq!(seq.tempo_c, 100, "a zero multiplier adds nothing");
    assert_eq!(seq.tracks[0].wait, 0);
    seq.set_tempo_multiplier(256);
    seq.render_frame(&mut out);
    assert_eq!(seq.tempo_c, 50, "100 + 100 crosses one unit of 150");
    assert_eq!(seq.tracks[0].wait, u16::from(u8::MAX) - 1);
}

#[test]
fn an_effective_tempo_above_the_command_bound_is_not_clamped() {
    let mut seq = sequencer_at(MAX_TEMPO_BPM, 512);
    assert_eq!(seq.effective_tempo(), 1020, "510 * 2 exceeds the bound");
    // 1020 / 150 = 6.8: six ticks on the first frame, the remainder carried.
    assert_eq!(ticks_per_frame(&mut seq, 1), [6]);
    assert_eq!(seq.tempo_c, 1020 % 150);
}

#[test]
fn the_widest_multiplier_stays_a_full_width_product() {
    let mut seq = sequencer_at(MAX_TEMPO_BPM, u16::MAX);
    // (510 * 65535) >> 8 = 130558; tempoI keeps the low 16 bits.
    assert_eq!(seq.effective_tempo(), 65_022);
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
    for _ in 0..3 {
        seq.render_frame(&mut out);
    }
    assert!(
        seq.tracks[0].ended,
        "the track's wait drains in one frame at this tempo"
    );
}

#[test]
fn an_ordinary_tempo_command_still_clamps_to_its_bound() {
    let mut seq = sequencer_at(150, 512);
    apply_test_event(&mut seq, 0, &Event::Tempo(u16::MAX));
    assert_eq!(seq.base_tempo(), MAX_TEMPO_BPM);
    assert_eq!(seq.effective_tempo(), 2 * MAX_TEMPO_BPM);
}

/// A track whose first tick runs more commands than one frame's budget, so
/// that tick yields and resumes on the next frame.
fn yielding_track() -> Vec<Event> {
    let mut track = vec![Event::Volume(127); TRACK_COMMANDS_PER_TICK as usize + 10];
    track.extend([Event::Wait(u8::MAX), Event::Fine]);
    track
}

#[test]
fn a_suspended_tick_resumes_under_a_large_effective_tempo() {
    let mut seq = Sequencer::new(test_song(vec![yielding_track()], MAX_TEMPO_BPM));
    seq.set_tempo_multiplier(u16::MAX);
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
    seq.render_frame(&mut out);
    assert!(seq.suspended_tick.is_some(), "the first tick yielded");
    assert_eq!(
        seq.tempo_c,
        65_022 - u32::from(TEMPO_UNIT),
        "the yielded tick leaves the rest of a 16-bit effective tempo unspent"
    );
    seq.render_frame(&mut out);
    assert!(seq.suspended_tick.is_none(), "the next frame resumed it");
    assert!(seq.tempo_c < u32::from(TEMPO_UNIT), "the backlog drained");
}

#[test]
fn a_zero_multiplier_still_finishes_the_tick_a_frame_already_paid_for() {
    // The frame that yielded had its tempo added already, so the backlog it
    // left is owed ticks (`m4a_1.s:1169`-`:1175`); a zero multiplier stops
    // only the tempo later frames add.
    let mut seq = Sequencer::new(test_song(vec![yielding_track()], MAX_TEMPO_BPM));
    seq.set_tempo_multiplier(512);
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
    seq.render_frame(&mut out);
    assert!(seq.suspended_tick.is_some());
    assert_eq!(seq.tempo_c, 1020 - u32::from(TEMPO_UNIT));

    seq.set_tempo_multiplier(0);
    seq.render_frame(&mut out);
    assert!(seq.suspended_tick.is_none());
    assert_eq!(seq.tempo_c, 1020 % u32::from(TEMPO_UNIT), "no tempo added");
    let wait = seq.tracks[0].wait;

    seq.render_frame(&mut out);
    assert_eq!(seq.tempo_c, 1020 % u32::from(TEMPO_UNIT));
    assert_eq!(seq.tracks[0].wait, wait, "the frozen sequencer stays put");
}

#[test]
fn the_multiplier_leaves_frame_size_and_sustained_pitch_unchanged() {
    let track = || {
        vec![
            Event::Voice(0),
            Event::Volume(127),
            tied_note(60),
            Event::Wait(u8::MAX),
            Event::Fine,
        ]
    };
    let render = |multiplier: u16| {
        let mut seq = Sequencer::new(test_song(vec![track()], 150));
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        seq.render_frame(&mut out);
        seq.set_tempo_multiplier(multiplier);
        let mut frames = Vec::new();
        // The test voice's one-shot sample lasts four frames.
        for _ in 0..2 {
            seq.render_frame(&mut out);
            assert_eq!(out.len(), Sequencer::FRAME_SAMPLES);
            frames.push(out.clone());
        }
        (frames, seq.mixer.voices()[0].base_volume())
    };
    let (slow, slow_volume) = render(128);
    let (unity, unity_volume) = render(256);
    let (fast, fast_volume) = render(384);
    assert!(unity.iter().flatten().any(|sample| *sample != 0.0));
    assert_eq!(
        slow, unity,
        "the sustained waveform does not depend on tempo"
    );
    assert_eq!(fast, unity);
    assert_eq!(slow_volume, unity_volume);
    assert_eq!(fast_volume, unity_volume);
}
