use super::fixture::*;
use super::*;

#[test]
fn echo_volume_and_length_are_copied_into_the_envelope_at_construction() {
    let echo_note = TestNote {
        echo_volume: 128,
        echo_length: 3,
        ..TestNote::default()
    };
    let mut voice = noise_voice(
        CgbAdsr {
            attack: 0,
            decay: 0,
            sustain: MAX_ADSR_LEVEL,
            release: 0,
        },
        WIDE_NOISE,
        echo_note,
    );
    voice.begin_frame(false);
    voice.note_off();
    voice.begin_frame(false);
    assert!(
        voice.is_active(),
        "a nonzero echo_volume must hold the channel in its pseudo-echo tail"
    );
}

#[test]
fn a_live_volume_write_leaves_the_pseudo_echo_floor_on_the_latched_goal() {
    // `ChnVolSetAsm` writes only the side volumes; `chan->envelopeGoal` is
    // recomputed by `CgbModVol` alone, which runs at note-on and at every
    // `envelopeCounter == 0` boundary (`m4a.c:903-923`, `:994`, `:1084`).
    // A zero-release note-off jumps straight to `envelope_pseudoecho_start`,
    // past that recompute (`m4a.c:1060-1073`), so its floor scales the goal
    // latched before the live write.
    let echo_note = TestNote {
        track_right: 128,
        track_left: 128,
        echo_volume: 128,
        echo_length: 4,
        ..TestNote::default()
    };
    let mut voice =
        square_voice_with_adsr(CgbChannelNumber::Square1, None, CgbAdsr::flat(), echo_note);
    voice.begin_frame(false);
    assert_eq!(
        voice.envelope.volume(),
        15,
        "sanity: the note sustains at the goal CgbModVol latched at note-on"
    );

    // One tick's `VOL` then `EOT`, with no envelope boundary between them.
    voice.set_track_volume(32, 32);
    voice.note_off();
    voice.begin_frame(false);

    assert_eq!(
        voice.envelope.volume(),
        8,
        "the pseudo-echo floor must scale the still-latched goal (15 -> 8), not the goal \
         the live volume write would recompute (3 -> 2)"
    );
}

#[test]
fn a_live_volume_write_before_the_first_pass_still_latches_at_note_on() {
    // `CgbModVol` runs unconditionally in the `SOUND_CHANNEL_SF_START`
    // branch, before upstream even reads the attack period
    // (`m4a.c:988-995`), so a live write that lands before the first
    // `CgbSound` pass for this channel is already reflected the first
    // time the goal latches -- unlike a write after that first pass
    // (`a_live_volume_write_leaves_the_pseudo_echo_floor_on_the_latched_goal`),
    // which the note-on latch has already passed.
    let echo_note = TestNote {
        track_right: 128,
        track_left: 128,
        echo_volume: 128,
        echo_length: 4,
        ..TestNote::default()
    };
    let paced_attack = CgbAdsr {
        attack: 2,
        decay: 0,
        sustain: 15,
        release: 0,
    };
    let mut voice =
        square_voice_with_adsr(CgbChannelNumber::Square1, None, paced_attack, echo_note);

    // Lands before the note's first `begin_frame`, so the note-on latch
    // itself must see it.
    voice.set_track_volume(32, 32);

    voice.begin_frame(false); // note-on's CgbModVol latches goal 3
    voice.note_off(); // live (mid-attack): a real release transition
    voice.begin_frame(false); // release == 0: straight to the tail

    assert_eq!(
        voice.envelope.volume(),
        2,
        "the note-on latch must see the live write (goal 15 -> 3), not the goal captured \
         at construction, so the pseudo-echo floor is 2, not 8"
    );
}

#[test]
fn note_off_before_the_first_envelope_pass_produces_no_audible_samples() {
    // Note-off before any `begin_frame`: `CgbEnvelope`'s upstream
    // short-circuit retires it at once, skipping the pseudo-echo tail.
    let echo_note = TestNote {
        echo_volume: 128,
        echo_length: 3,
        ..TestNote::default()
    };
    let mut voice = noise_voice(
        CgbAdsr {
            attack: 0,
            decay: 0,
            sustain: MAX_ADSR_LEVEL,
            release: 0,
        },
        WIDE_NOISE,
        echo_note,
    );

    voice.note_off();
    voice.begin_frame(false);
    let mut acc = vec![(0i32, 0i32); 8];
    voice.render(&mut acc, &[]);

    assert!(
        !voice.is_active(),
        "must retire immediately, not hold a pseudo-echo tail"
    );
    assert!(
        acc.iter().all(|&(l, r)| l == 0 && r == 0),
        "must produce no audible samples"
    );
}
