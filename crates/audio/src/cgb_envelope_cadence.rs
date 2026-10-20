//! [`CgbEnvelopeCadence::advance_frame`] and [`CgbEnvelope::step_frame`]
//! tests, run directly against `CgbEnvelope` without a `Mixer`.

use super::*;

#[test]
fn cadence_doubles_on_exactly_the_fifteenth_call_of_every_cycle() {
    const CALLS_PER_CYCLE: usize = 15;
    let mut cadence = CgbEnvelopeCadence::default();
    let results: Vec<bool> = (1..=2 * CALLS_PER_CYCLE)
        .map(|_| cadence.advance_frame())
        .collect();
    let expected: Vec<bool> = (1..=2 * CALLS_PER_CYCLE)
        .map(|call| call % CALLS_PER_CYCLE == 0)
        .collect();
    assert_eq!(results, expected);
}

#[test]
fn extra_iteration_pre_decrements_a_freshly_armed_sustain_counter() {
    // A doubled landing frame decrements the freshly armed sustain counter
    // at once, so a live goal change re-snaps after 6 further frames rather
    // than the 7 an undoubled landing takes.
    let adsr = CgbAdsr {
        attack: 0,
        decay: 0,
        sustain: 8,
        release: 0,
    };
    let mut env = CgbEnvelope::new(adsr, 10, 0, 0);
    env.step_frame(true, 10); // lands sustain_goal = 5
    assert_eq!(env.volume(), 5);

    // A live goal change (10 -> 20, sustain_goal 5 -> 10) waits for the
    // counter boundary.
    for frame in 1..6 {
        env.step_frame(false, 20);
        assert_eq!(
            env.volume(),
            5,
            "must not re-snap before the pre-decremented counter elapses (frame {frame})"
        );
    }
    env.step_frame(false, 20); // 6th further frame
    assert_eq!(
        env.volume(),
        10,
        "a doubled landing frame re-snaps 6 frames later, not 7"
    );
}

#[test]
fn extra_iteration_can_fire_the_first_release_decrement_on_the_note_off_frame() {
    // An undoubled note-off frame only holds the level; a doubled one also
    // fires the first release decrement.
    let adsr = CgbAdsr {
        attack: 0,
        decay: 0,
        sustain: 15,
        release: 1,
    };
    let mut env = CgbEnvelope::new(adsr, 4, 0, 0);
    env.step_frame(false, 4);
    assert_eq!(env.volume(), 4);
    env.note_off();
    env.step_frame(true, 4); // note-off frame, doubled
    assert_eq!(
        env.volume(),
        3,
        "the extra iteration must fire the first release step this same frame"
    );
}

#[test]
fn extra_iteration_is_skipped_the_frame_release_enters_the_pseudo_echo_tail() {
    // A zero release enters the echo tail directly, bypassing the doubling
    // check, so a doubled frame must not also consume a tail frame.
    const GOAL: u8 = 8;
    const HALF_SCALE_ECHO_VOLUME: u8 = 128;
    let adsr = CgbAdsr {
        attack: 0,
        decay: 0,
        sustain: 15,
        release: 0,
    };
    let mut env = CgbEnvelope::new(adsr, GOAL, HALF_SCALE_ECHO_VOLUME, 2);
    env.step_frame(false, GOAL);
    assert_eq!(env.volume(), GOAL);
    env.note_off();
    env.step_frame(true, GOAL); // doubled frame
    assert_eq!(
        env.volume(),
        GOAL / 2,
        "a half-scale echo volume floors the tail at half the goal"
    );
    assert!(
        env.is_active(),
        "an echo floor must hold, not silence outright"
    );
    // A doubled entry frame that also ticked the tail would leave it one
    // frame short.
    env.step_frame(false, GOAL);
    assert!(env.is_active(), "tail must still hold after 1 of 2 ticks");
    env.step_frame(false, GOAL);
    assert!(!env.is_active(), "2-frame tail exhausts on the 2nd tick");
}

#[test]
fn extra_iteration_is_skipped_while_already_in_the_pseudo_echo_tail() {
    // A channel already in its tail takes one decrement per frame, even on
    // the doubled frame.
    let adsr = CgbAdsr {
        attack: 0,
        decay: 0,
        sustain: 15,
        release: 0,
    };
    let mut env = CgbEnvelope::new(adsr, 8, 128, 5);
    env.step_frame(false, 8);
    env.note_off();
    env.step_frame(false, 8); // enters the tail
    assert!(env.is_active());

    env.step_frame(true, 8);
    assert!(
        env.is_active(),
        "one doubled frame must not exhaust a 5-frame tail after just 1 tick"
    );
    // 3 more undoubled ticks leave the tail held.
    for _ in 0..3 {
        env.step_frame(false, 8);
        assert!(env.is_active());
    }
    // The 5th tail tick overall retires it.
    env.step_frame(false, 8);
    assert!(!env.is_active());
}

#[test]
fn a_voice_retired_on_a_doubled_frame_stays_dead() {
    // A zero release with no echo floor retires outright. The retired arm is
    // a no-op either way, so this cannot observe the doubling guard; it pins
    // that a dead envelope neither revives nor mis-steps.
    let adsr = CgbAdsr {
        attack: 0,
        decay: 0,
        sustain: 15,
        release: 0,
    };
    let mut env = CgbEnvelope::new(adsr, 4, 0, 0);
    env.step_frame(false, 4);
    env.note_off();
    env.step_frame(true, 4); // doubled frame
    assert!(!env.is_active());
    assert_eq!(env.volume(), 0);
}
