//! `Mixer`-level tests for [`crate::cgb_envelope::CgbEnvelopeCadence`]
//! (issue #453): that it is one shared clock across CGB channels and
//! survives slot replacement. Per-transition doubling/bypass mechanics have
//! their own sibling, `crate::cgb_envelope`'s `step_frame` tests. Split out
//! for the same reason as [`super::priority_tests`].

use super::*;
use crate::cgb_envelope::CgbAdsr;
use crate::cgb_voice::CgbChannelNumber;

/// A square-channel voice with an `attack`-period, full-sustain envelope and
/// a wide-open centred pan (`vol_mr`/`vol_ml` both `0xFF`), so its goal
/// clears `31` -- comfortably above `15` and never reached across the frame
/// counts these tests pace through, keeping every step a plain `+1`.
fn attack_voice(channel: CgbChannelNumber, attack: u8, track: usize) -> CgbVoice {
    CgbVoice::square(
        channel,
        2,
        None,
        CgbAdsr {
            attack,
            decay: 0,
            sustain: 15,
            release: 0,
        },
        60,
        0,
        0xFF,
        0xFF,
        127,
        0,
        60,
        track,
        0,
        0,
        0,
    )
}

/// A square-channel voice with an instant (`release == 0`), full-sustain
/// envelope and a configured pseudo-echo tail, for the doubled-frame
/// echo-entry/echo-tick tests below.
fn echo_voice(
    channel: CgbChannelNumber,
    echo_volume: u8,
    echo_length: u8,
    track: usize,
) -> CgbVoice {
    CgbVoice::square(
        channel,
        2,
        None,
        CgbAdsr {
            attack: 0,
            decay: 0,
            sustain: 15,
            release: 0,
        },
        60,
        0,
        0xFF,
        0xFF,
        127,
        0,
        60,
        track,
        0,
        echo_volume,
        echo_length,
    )
}

fn envelope_volume_of(mixer: &Mixer, channel: CgbChannelNumber) -> Option<u8> {
    mixer.cgb_voices()[channel.slot()]
        .as_ref()
        .map(CgbVoice::envelope_volume)
}

fn is_occupied(mixer: &Mixer, channel: CgbChannelNumber) -> bool {
    mixer.cgb_voices()[channel.slot()].is_some()
}

#[test]
fn mix_frame_runs_sixteen_envelope_iterations_across_fifteen_frames() {
    // 14 undoubled frames pace one `+1` per frame after the held note-on
    // frame (`attack == 1`: counter armed to 2, so the first call holds and
    // every call from the 2nd onward fires); the 15th frame's extra
    // iteration must add a *second* `+1` that same call.
    let mut mixer = Mixer::default();
    assert!(mixer.add_cgb_voice(attack_voice(CgbChannelNumber::Square1, 1, 0)));
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];

    for _ in 0..14 {
        mixer.mix_frame(&mut out);
    }
    assert_eq!(
        envelope_volume_of(&mixer, CgbChannelNumber::Square1),
        Some(13),
        "14 undoubled frames must pace one +1 per frame after the held note-on frame"
    );

    mixer.mix_frame(&mut out);
    assert_eq!(
        envelope_volume_of(&mixer, CgbChannelNumber::Square1),
        Some(15),
        "the 15th frame's extra iteration must add a second +1 this same frame \
         (16 iterations over 15 frames, not 15)"
    );
}

#[test]
fn the_shared_cadence_doubles_every_active_channel_on_the_same_mixer_frame() {
    // Square 2 starts 5 mixer frames later than square 1, so its own local
    // frame count is offset from square 1's -- but the cadence is one clock
    // for the whole mixer, not per voice (module docs), so both must double
    // on the same 15th *mixer* frame regardless of each one's own age.
    let mut mixer = Mixer::default();
    assert!(mixer.add_cgb_voice(attack_voice(CgbChannelNumber::Square1, 1, 0)));
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    for _ in 0..5 {
        mixer.mix_frame(&mut out);
    }
    assert!(mixer.add_cgb_voice(attack_voice(CgbChannelNumber::Square2, 1, 1)));

    for _ in 0..9 {
        mixer.mix_frame(&mut out);
    }
    // Mixer frame 14: square 1's own 14th frame, square 2's own 9th.
    assert_eq!(
        envelope_volume_of(&mixer, CgbChannelNumber::Square1),
        Some(13)
    );
    assert_eq!(
        envelope_volume_of(&mixer, CgbChannelNumber::Square2),
        Some(8)
    );

    mixer.mix_frame(&mut out); // mixer frame 15
    assert_eq!(
        envelope_volume_of(&mixer, CgbChannelNumber::Square1),
        Some(15),
        "square 1's own 15th frame doubles"
    );
    assert_eq!(
        envelope_volume_of(&mixer, CgbChannelNumber::Square2),
        Some(10),
        "square 2, only on its own local 10th frame, must ALSO double here -- \
         a per-voice cadence (restarting at each note-on) would leave it at 9"
    );
}

#[test]
fn the_cadence_phase_survives_a_cgb_slot_being_replaced() {
    // The cadence lives on the Mixer, not the voice: replacing whatever
    // occupies square 1's slot must not reset which mixer frame is "the
    // 15th".
    let mut mixer = Mixer::default();
    assert!(mixer.add_cgb_voice(attack_voice(CgbChannelNumber::Square1, 1, 0)));
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    for _ in 0..14 {
        mixer.mix_frame(&mut out);
    }

    // Replace square 1's occupant with a fresh attack-1 note in the same
    // slot: equal priority and an equal-or-later occupant track is reusable
    // (`Mixer::add_cgb_voice`'s doc), so the same track number is enough.
    assert!(mixer.add_cgb_voice(attack_voice(CgbChannelNumber::Square1, 1, 0)));

    // This is the mixer's 15th frame -- the shared cadence's extra
    // iteration -- even though the replacement voice is on its own very
    // first frame. A cadence reset on replacement would leave it held at 0
    // (an ordinary, undoubled note-on frame); surviving the replacement
    // means this one frame both holds (iteration 1) and fires the first
    // `+1` (iteration 2), landing on 1.
    mixer.mix_frame(&mut out);
    assert_eq!(
        envelope_volume_of(&mixer, CgbChannelNumber::Square1),
        Some(1),
        "the replacement's very first frame must still get the doubled iteration"
    );
}

#[test]
fn extra_iteration_does_not_shorten_a_tail_entered_on_the_doubled_frame() {
    let mut mixer = Mixer::default();
    assert!(mixer.add_cgb_voice(echo_voice(CgbChannelNumber::Square1, 128, 2, 0)));
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    for _ in 0..14 {
        mixer.mix_frame(&mut out);
    }
    mixer.note_off_track(0, 60);

    // Frame 15: release == 0 sends note-off straight into the pseudo-echo
    // tail via a goto that bypasses the doubling check outright
    // (`m4a.c:1087`..`:1103`) -- the extra iteration on this same frame must
    // not also consume a tail tick.
    mixer.mix_frame(&mut out);
    assert!(
        is_occupied(&mixer, CgbChannelNumber::Square1),
        "the tail must still be holding right after entering it on the doubled frame"
    );

    mixer.mix_frame(&mut out); // tail tick 1 of 2
    assert!(is_occupied(&mixer, CgbChannelNumber::Square1));

    mixer.mix_frame(&mut out); // tail tick 2 of 2 -> retires
    assert!(!is_occupied(&mixer, CgbChannelNumber::Square1));
}

#[test]
fn extra_iteration_does_not_double_tick_an_already_active_pseudo_echo_tail() {
    // The `SOUND_CHANNEL_SF_IEC` branch servicing an ongoing tail jumps
    // straight to `envelope_complete` every frame regardless of `prevC15`
    // (`m4a.c:1048`..`:1059`) -- so a tail already running well before the
    // doubled 15th frame must still take exactly 20 ticks to exhaust a
    // 20-frame `echo_length`, not 19 (which a frame-15 double-tick bug
    // would produce).
    let mut mixer = Mixer::default();
    assert!(mixer.add_cgb_voice(echo_voice(CgbChannelNumber::Square1, 128, 20, 0)));
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];

    // Render one ordinary frame before the note-off: `SOUND_CHANNEL_SF_START`
    // still being set when `SOUND_CHANNEL_SF_STOP` arrives routes straight to
    // `oscillator_off`, bypassing pseudo-echo entirely (`m4a.c:988`..`:1046`,
    // `:1053`..`:1057`) -- a distinct, pre-existing edge this cadence test
    // isn't about, so establish the voice first the same way every other
    // test here does.
    mixer.mix_frame(&mut out); // frame 1: establishes the voice
    mixer.note_off_track(0, 60);
    mixer.mix_frame(&mut out); // frame 2: release == 0 enters the tail immediately

    for _ in 0..19 {
        mixer.mix_frame(&mut out); // frames 3..=21: 19 ordinary ticks (global frame 15 among them)
    }
    assert!(
        is_occupied(&mixer, CgbChannelNumber::Square1),
        "19 ticks (including the doubled 15th frame) must still be one short of a 20-tick tail; \
         a double-ticked frame 15 would have exhausted it by now"
    );

    mixer.mix_frame(&mut out); // frame 22: the 20th tick exhausts the tail
    assert!(!is_occupied(&mixer, CgbChannelNumber::Square1));
}

#[test]
fn a_channel_that_retires_on_the_doubled_frame_does_not_panic_or_revive() {
    let mut mixer = Mixer::default();
    // No echo floor configured: `echo_volume == 0` retires outright instead
    // of holding a tail.
    assert!(mixer.add_cgb_voice(echo_voice(CgbChannelNumber::Square1, 0, 0, 0)));
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    for _ in 0..14 {
        mixer.mix_frame(&mut out);
    }
    mixer.note_off_track(0, 60);

    // Frame 15: release == 0, no echo floor -> retires outright via a goto
    // that likewise bypasses the doubling check (`m4a.c:1053`); the extra
    // iteration must be a safe no-op on an already-retired voice, not a
    // panic or a revival.
    mixer.mix_frame(&mut out);
    assert!(!is_occupied(&mixer, CgbChannelNumber::Square1));
    assert_eq!(mixer.voice_count(), 0);
}
