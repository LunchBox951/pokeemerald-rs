//! A noise voice retiring while a track volume or pan write is pending: the retiring frame commits no NR51 route.

use super::*;

/// A frame that retires the voice jumps to `oscillator_off` and
/// `channel_complete`, skipping the `CGB_CHANNEL_MO_VOL` NR51 write
/// (`m4a.c:1053-1056,1205-1208`), so a hard-pan volume change landing on the
/// retiring release boundary must not move the route the idle clone keeps.
#[test]
fn a_retiring_frame_does_not_commit_a_pending_pan_change() {
    let mut voice = releasing_latched_noise_voice();
    assert!(
        voice.routing.left_enabled && voice.routing.right_enabled,
        "sanity: centred"
    );
    voice.note_off();
    let retiring_frame = retiring_frame_with_hard_right_write(&voice);
    for _ in 0..retiring_frame {
        voice.begin_frame(false);
    }
    voice.set_track_volume(FULL_TRACK_VOLUME, 0);
    voice.begin_frame(false);
    assert!(!voice.is_active(), "sanity: this is the retiring frame");
    assert!(
        voice.routing.left_enabled && voice.routing.right_enabled,
        "the retiring frame committed the hard-right pan (left {}, right {})",
        voice.routing.left_enabled,
        voice.routing.right_enabled
    );
}

/// The idle slot keeps emitting the raised latch on the route `NR51` still
/// holds, both sides (`m4a.c:1053-1056,1205-1208`).
#[test]
fn a_pending_volume_write_on_the_retiring_frame_keeps_the_idle_latch_on_both_sides() {
    let mut voice = releasing_latched_noise_voice();
    voice.note_off();
    let retiring_frame = retiring_frame_with_hard_right_write(&voice);
    let mut mixer = crate::mixer::Mixer::default();
    assert!(mixer.add_cgb_voice(voice));
    let mut out = vec![0.0f32; crate::SAMPLES_PER_FRAME * 2];
    for _ in 0..retiring_frame {
        mixer.mix_frame(&mut out);
    }
    mixer.set_track_volume(0, FULL_TRACK_VOLUME, 0);
    mixer.mix_frame(&mut out);
    assert!(
        mixer.cgb_voices().iter().all(Option::is_none),
        "sanity: the voice retired on this frame"
    );
    mixer.mix_frame(&mut out);
    let (left, right) = (out[0], out[1]);
    assert!(left > 0.0, "the idle latch still plays on the left: {left}");
    assert!(
        right > 0.0,
        "the idle latch still plays on the right: {right}"
    );
}

fn releasing_latched_noise_voice() -> CgbVoice {
    let releasing = CgbAdsr {
        attack: 0,
        decay: 0,
        sustain: 1,
        release: 1,
    };
    noise_voice_latched(releasing, SLOW_NOISE_KEY, WIDE_NOISE)
}

/// The count of frames before the one that retires `voice`, found by probing
/// with the hard-right volume write applied on each candidate frame.
fn retiring_frame_with_hard_right_write(voice: &CgbVoice) -> usize {
    let mut tracked = voice.clone();
    for frame in 0..200 {
        let mut probe = tracked.clone();
        probe.set_track_volume(FULL_TRACK_VOLUME, 0);
        probe.begin_frame(false);
        if !probe.is_active() {
            return frame;
        }
        tracked.begin_frame(false);
        assert!(
            tracked.is_active(),
            "sanity: the unprobed voice tracks the probe"
        );
    }
    panic!("the voice never retired");
}
