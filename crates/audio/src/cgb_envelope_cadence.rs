//! [`CgbEnvelopeCadence::advance_frame`] and [`CgbEnvelope::step_frame`]
//! tests, pinned directly against `CgbEnvelope`, no `Mixer` involved — see
//! `mixer_cgb_envelope.rs` for the same mechanism exercised through
//! `Mixer::mix_frame`.

use super::*;

#[test]
fn cadence_doubles_on_exactly_the_fifteenth_call_of_every_cycle() {
    // `c15` counts down `14, 13, .., 1, 0` and wraps (`m4a.c:941`..`:945`);
    // the extra iteration fires only on the call whose `prevC15` (assigned
    // from that result, `m4a.c:984`) is `0` (`m4a.c:1177`..`:1180`) -- the
    // 15th call of each 15-call cycle, never any other.
    let mut cadence = CgbEnvelopeCadence::default();
    let results: Vec<bool> = (1..=30).map(|_| cadence.advance_frame()).collect();
    let expected: Vec<bool> = (1..=30).map(|call| call % 15 == 0).collect();
    assert_eq!(results, expected);
}

#[test]
fn extra_iteration_pre_decrements_a_freshly_armed_sustain_counter() {
    // Landing in sustain arms `envelopeCounter = 7` (`envelope_sustain`,
    // `m4a.c:1112`..`:1114`). If that same call is the doubled 15th
    // frame, the second pass through `envelope_step_repeat` re-checks
    // (and decrements) that freshly-armed counter immediately -- not a
    // bug to special-case away, just what a second pass through the same
    // logic does. Pin it against `cgb_envelope`'s own
    // `sustain_re_snaps_live_goal_within_seven_frames` (a single, undoubled
    // `step()` per call takes 7 further frames to re-snap): here, one
    // doubled `step_frame` should only need 6.
    let adsr = CgbAdsr {
        attack: 0,
        decay: 0,
        sustain: 8,
        release: 0,
    };
    let mut env = CgbEnvelope::new(adsr, 10, 0, 0);
    env.step_frame(true); // lands sustain_goal = 5, counter armed 7, pre-decremented to 6
    assert_eq!(env.volume(), 5);

    env.set_goal(adsr, 20); // sustain_goal 5 -> 10, live goal change

    for frame in 1..6 {
        env.step_frame(false);
        assert_eq!(
            env.volume(),
            5,
            "must not re-snap before the pre-decremented counter elapses (frame {frame})"
        );
    }
    env.step_frame(false); // 6th further frame: the pre-decremented counter reaches 0
    assert_eq!(
        env.volume(),
        10,
        "a doubled landing frame re-snaps 6 frames later, not 7"
    );
}

#[test]
fn extra_iteration_can_fire_the_first_release_decrement_on_the_note_off_frame() {
    // The note-off frame normally only holds the current level, arming
    // `release + 1` and decrementing once with no `-1` yet (`note_off`'s
    // doc). If that same frame is doubled, the second pass re-enters the
    // paced release logic and can fire the first `-1` this same frame
    // (`m4a.c:1176`..`:1180`).
    let adsr = CgbAdsr {
        attack: 0,
        decay: 0,
        sustain: 15,
        release: 1,
    };
    let mut env = CgbEnvelope::new(adsr, 4, 0, 0);
    env.step_frame(false);
    assert_eq!(env.volume(), 4);
    env.note_off();
    env.step_frame(true); // note-off frame, doubled
    assert_eq!(
        env.volume(),
        3,
        "the extra iteration must fire the first release step this same frame"
    );
}

#[test]
fn extra_iteration_is_skipped_the_frame_release_enters_the_pseudo_echo_tail() {
    // `release == 0` sends note-off straight into
    // `enter_echo_or_silence` (`m4a.c:1071` goto `envelope_pseudoecho_start`),
    // a path that bypasses `envelope_step_complete`'s doubling check
    // outright (`m4a.c:1087`..`:1103`) -- so even a doubled 15th frame
    // must not also consume a tail frame the same call.
    let adsr = CgbAdsr {
        attack: 0,
        decay: 0,
        sustain: 15,
        release: 0,
    };
    let mut env = CgbEnvelope::new(adsr, 8, 128, 2);
    env.step_frame(false);
    assert_eq!(env.volume(), 8);
    env.note_off();
    env.step_frame(true); // doubled frame: must enter the tail, not also tick it
                          // floor = (8*128+255)>>8 = 4
    assert_eq!(env.volume(), 4);
    assert!(
        env.is_active(),
        "an echo floor must hold, not silence outright"
    );
    // If the extra iteration had also ticked the tail here, it would
    // already be one frame short of its full 2-frame length.
    env.step_frame(false);
    assert!(env.is_active(), "tail must still hold after 1 of 2 ticks");
    env.step_frame(false);
    assert!(!env.is_active(), "2-frame tail exhausts on the 2nd tick");
}

#[test]
fn extra_iteration_is_skipped_while_already_in_the_pseudo_echo_tail() {
    // The `SOUND_CHANNEL_SF_IEC` branch servicing an ongoing tail jumps
    // straight to `envelope_complete` every frame, regardless of
    // `prevC15` (`m4a.c:1048`..`:1059`) -- so a channel already in its
    // tail must never take a second decrement even on the doubled frame.
    let adsr = CgbAdsr {
        attack: 0,
        decay: 0,
        sustain: 15,
        release: 0,
    };
    let mut env = CgbEnvelope::new(adsr, 8, 128, 5);
    env.step_frame(false);
    env.note_off();
    env.step_frame(false); // enters the tail, echo_length untouched (5)
    assert!(env.is_active());

    // Already in the tail: a doubled frame must consume exactly one tail
    // frame, not two.
    env.step_frame(true);
    assert!(
        env.is_active(),
        "one doubled frame must not exhaust a 5-frame tail after just 1 tick"
    );
    // 3 more undoubled ticks: post-decrement 3, 2, 1 -- still held.
    for _ in 0..3 {
        env.step_frame(false);
        assert!(env.is_active());
    }
    // 5th tail tick overall (post-decrement 0) retires.
    env.step_frame(false);
    assert!(!env.is_active());
}

#[test]
fn extra_iteration_is_skipped_the_frame_the_voice_retires() {
    // A `release == 0` note-off with no echo floor retires outright via
    // `oscillator_off` (`m4a.c:1053`), a goto that -- like the pseudo-echo
    // entry above -- bypasses the doubling check. `step()` already
    // early-returns for an inactive envelope regardless of this gate (see
    // `step_frame`'s body), so this test pins the resulting state -- an
    // already-dead envelope neither revives nor mis-steps -- rather than
    // discriminating the `self.active` check itself.
    let adsr = CgbAdsr {
        attack: 0,
        decay: 0,
        sustain: 15,
        release: 0,
    };
    let mut env = CgbEnvelope::new(adsr, 4, 0, 0);
    env.step_frame(false);
    env.note_off();
    env.step_frame(true); // doubled frame: release==0, no echo -> retires
    assert!(!env.is_active());
    assert_eq!(env.volume(), 0);
}
