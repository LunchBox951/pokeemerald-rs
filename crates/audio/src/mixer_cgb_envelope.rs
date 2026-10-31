//! `Mixer`-level tests for [`crate::cgb_envelope::CgbEnvelopeCadence`]: one
//! shared clock across CGB channels that survives slot replacement.

use super::*;
use crate::cgb_envelope::CgbAdsr;
use crate::cgb_voice::CgbChannelNumber;

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
    assert_eq!(
        envelope_volume_of(&mixer, CgbChannelNumber::Square1),
        Some(13)
    );
    assert_eq!(
        envelope_volume_of(&mixer, CgbChannelNumber::Square2),
        Some(8)
    );

    mixer.mix_frame(&mut out);
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
    let mut mixer = Mixer::default();
    assert!(mixer.add_cgb_voice(attack_voice(CgbChannelNumber::Square1, 1, 0)));
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    for _ in 0..14 {
        mixer.mix_frame(&mut out);
    }

    // Same track and priority make the occupied slot reusable.
    assert!(mixer.add_cgb_voice(attack_voice(CgbChannelNumber::Square1, 1, 0)));

    // A cadence reset on replacement would leave the volume held at 0.
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

    // Upstream enters the tail past the doubling check (`m4a.c:1087`..`:1103`).
    mixer.mix_frame(&mut out);
    assert!(
        is_occupied(&mixer, CgbChannelNumber::Square1),
        "the tail must still be holding right after entering it on the doubled frame"
    );

    mixer.mix_frame(&mut out);
    assert!(is_occupied(&mixer, CgbChannelNumber::Square1));

    mixer.mix_frame(&mut out);
    assert!(!is_occupied(&mixer, CgbChannelNumber::Square1));
}

#[test]
fn extra_iteration_does_not_double_tick_an_already_active_pseudo_echo_tail() {
    // Upstream ticks an active tail once per frame (`m4a.c:1048`..`:1059`).
    let mut mixer = Mixer::default();
    assert!(mixer.add_cgb_voice(echo_voice(CgbChannelNumber::Square1, 128, 20, 0)));
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];

    // A note-off before the first rendered frame skips the tail
    // (`m4a.c:988`..`:1046`, `:1053`..`:1057`).
    mixer.mix_frame(&mut out);
    mixer.note_off_track(0, 60);
    mixer.mix_frame(&mut out);

    for _ in 0..19 {
        mixer.mix_frame(&mut out);
    }
    assert!(
        is_occupied(&mixer, CgbChannelNumber::Square1),
        "19 ticks (including the doubled 15th frame) must still be one short of a 20-tick tail; \
         a double-ticked frame 15 would have exhausted it by now"
    );

    mixer.mix_frame(&mut out);
    assert!(!is_occupied(&mixer, CgbChannelNumber::Square1));
}

#[test]
fn a_channel_that_retires_on_the_doubled_frame_does_not_panic_or_revive() {
    let mut mixer = Mixer::default();
    assert!(mixer.add_cgb_voice(echo_voice(CgbChannelNumber::Square1, 0, 0, 0)));
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    for _ in 0..14 {
        mixer.mix_frame(&mut out);
    }
    mixer.note_off_track(0, 60);

    // Upstream retires without an echo floor past the doubling check (`m4a.c:1053`).
    mixer.mix_frame(&mut out);
    assert!(!is_occupied(&mixer, CgbChannelNumber::Square1));
    assert_eq!(mixer.voice_count(), 0);
}
