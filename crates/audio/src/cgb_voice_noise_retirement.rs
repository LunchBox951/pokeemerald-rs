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

fn routed_note(track_right: u8, track_left: u8) -> TestNote {
    TestNote {
        track_right,
        track_left,
        ..TestNote::at_key(SLOW_NOISE_KEY)
    }
}

/// A note whose attack, decay, sustain and echo volume are all zero retires on
/// its first `CgbSound` pass.
const IMMEDIATELY_RETIRING: CgbAdsr = CgbAdsr {
    attack: 0,
    decay: 0,
    sustain: 0,
    release: 0,
};

/// Replace a sounding hard-right noise whose latch is raised with a hard-left
/// `adsr` note, then mix the swap frame and the next one.
fn mix_hard_left_replacement(adsr: CgbAdsr) -> (crate::mixer::Mixer, Vec<f32>) {
    let mut predecessor = noise_voice(
        CgbAdsr::flat(),
        WIDE_NOISE,
        routed_note(FULL_TRACK_VOLUME, 0),
    );
    predecessor.begin_frame(false);
    let mut acc = [(0i32, 0i32); 1];
    while predecessor.noise_output_latch() == Some(0) {
        predecessor.render(&mut acc, &[]);
    }
    let mut mixer = crate::mixer::Mixer::default();
    assert!(mixer.add_cgb_voice(predecessor));
    let replacement = noise_voice(adsr, WIDE_NOISE, routed_note(0, FULL_TRACK_VOLUME));
    assert!(mixer.add_cgb_voice(replacement));
    let mut frames = vec![0.0f32; crate::SAMPLES_PER_FRAME * 2];
    let mut out = vec![0.0f32; crate::SAMPLES_PER_FRAME * 2];
    mixer.mix_frame(&mut frames);
    mixer.mix_frame(&mut out);
    frames.extend(out);
    (mixer, frames)
}

/// A first pass that retires jumps through `oscillator_off` past the
/// `CGB_CHANNEL_MO_VOL` NR51 store (`m4a.c:1053-1056,1205-1208`), so NR51 keeps
/// the predecessor's route. That route is never audible: the retiring note's
/// width-only `NR43` write and `NR42 = 8` off-write leave the channel clocking
/// at volume zero, whose first LFSR clock resolves the raised latch to zero
/// (`m4a.c:873-874,1020-1022`; `mgba/src/gb/audio.c:371-383,602-644`), before the first
/// sample, so no route carries a latch and `CgbVoice` need not carry it.
#[test]
fn a_first_pass_retiring_noise_replacement_leaves_no_latch_on_either_route() {
    let (mixer, out) = mix_hard_left_replacement(IMMEDIATELY_RETIRING);
    assert!(
        mixer.cgb_voices().iter().all(Option::is_none),
        "sanity: the replacement retired on its first pass"
    );
    assert!(
        out.iter().all(|&sample| sample == 0.0),
        "the idle latch plays on a route"
    );
}

/// A replacement that survives its first pass reaches the NR51 store
/// (`m4a.c:1081-1082,1205-1208`), so its own route replaces the predecessor's.
#[test]
fn a_surviving_noise_replacement_commits_its_own_route() {
    let (mixer, out) = mix_hard_left_replacement(CgbAdsr::flat());
    assert!(
        mixer.cgb_voices().iter().any(Option::is_some),
        "sanity: the replacement survived"
    );
    assert!(
        out.chunks_exact(2).any(|pair| pair[0] != 0.0),
        "the replacement sounds on its own left route"
    );
    assert!(
        out.chunks_exact(2).all(|pair| pair[1] == 0.0),
        "the predecessor's right route is gone"
    );
}
