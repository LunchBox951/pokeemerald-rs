//! Frame-mixing tests for [`Mixer`]: envelope scaling, summing, clipping and
//! `EOT` note-off selection across the DirectSound pool and the CGB slots.
//! The note-on channel search has its own sibling, [`super::priority_tests`].

use std::sync::Arc;

use super::*;
use crate::cgb_envelope::CgbAdsr;
use crate::cgb_voice::CgbChannelNumber;
use crate::envelope::Adsr;
use crate::pitch::{DIV_FREQ, FRAC_BITS};
use crate::sample::WaveData;

fn unity_freq() -> u32 {
    (1 << FRAC_BITS) / DIV_FREQ
}

/// A tied square-1 CGB voice on `track` at `key`, for cross-kind
/// end-of-tie tests. Its hardware slot is [`CgbChannelNumber::Square1`].
fn cgb_keyed_voice(track: usize, key: u8) -> CgbVoice {
    CgbVoice::square(
        CgbChannelNumber::Square1,
        2,
        None,
        CgbAdsr::flat(),
        key,
        0,
        0xFF,
        0xFF,
        127,
        0,
        key,
        track,
        0,
        0,
        0,
    )
}

fn constant_voice(level: i8, track: usize) -> Voice {
    keyed_voice(level, track, 60, 0xFF, 0xFF)
}

fn keyed_voice(level: i8, track: usize, key: u8, right: u8, left: u8) -> Voice {
    // A long constant wave so a whole frame renders without ending; a `0`
    // gate makes it tied (it only stops on an explicit note-off).
    let data = vec![level; SAMPLES_PER_FRAME + 4];
    let wave = Arc::new(WaveData::one_shot(0, data));
    Voice::new(
        wave,
        Adsr::flat(),
        unity_freq(),
        right,
        left,
        127,
        0,
        key,
        track,
        0,
        0,
    )
}

#[test]
fn empty_mixer_renders_silence() {
    let mut mixer = Mixer::default();
    let mut out = vec![9.0; SAMPLES_PER_FRAME * 2];
    mixer.mix_frame(&mut out);
    assert!(out.iter().all(|&s| s == 0.0));
    assert!(mixer.is_idle());
}

#[test]
fn single_voice_is_scaled_and_normalised() {
    // Pin master volume to 15 so the documented `254` env gain holds; this
    // test exercises the mixing math, not the Emerald default (12).
    let mut mixer = Mixer::new(15, DEFAULT_MAX_VOICES);
    mixer.add_voice(constant_voice(50, 0));
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    mixer.mix_frame(&mut out);
    // env gain 254, contribution (254*50)>>8 = 49, /128.
    let expected = (((254 * 50) >> 8) as f32) / 128.0;
    assert!((out[0] - expected).abs() < 1e-6);
}

#[test]
fn two_voices_sum() {
    let mut mixer = Mixer::new(15, DEFAULT_MAX_VOICES);
    mixer.add_voice(constant_voice(40, 0));
    mixer.add_voice(constant_voice(30, 1));
    assert_eq!(mixer.voice_count(), 2);
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    mixer.mix_frame(&mut out);
    let a = (254 * 40) >> 8;
    let b = (254 * 30) >> 8;
    let expected = ((a + b) as f32) / 128.0;
    assert!((out[0] - expected).abs() < 1e-6);
}

#[test]
fn loud_sum_clips_to_full_scale() {
    let mut mixer = Mixer::new(15, DEFAULT_MAX_VOICES);
    // Four hard-driven voices sum past the s8 range and must clip.
    for track in 0..4 {
        mixer.add_voice(constant_voice(127, track));
    }
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    mixer.mix_frame(&mut out);
    // Each contributes (254*127)>>8 = 125; 4*125 = 500 clips to 127.
    assert!((out[0] - (127.0 / 128.0)).abs() < 1e-6);
}

#[test]
fn negative_sum_clips_to_minus_one() {
    let mut mixer = Mixer::new(15, DEFAULT_MAX_VOICES);
    for track in 0..4 {
        mixer.add_voice(constant_voice(-128, track));
    }
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    mixer.mix_frame(&mut out);
    assert!((out[0] - (-1.0)).abs() < 1e-6);
}

#[test]
fn note_off_track_stops_only_the_first_matching_voice() {
    // Two overlapping voices share key 60 on track 0; a third holds key 64.
    // `EOT` retires exactly one key-60 voice, leaving the other still
    // sounding (mirrors `ply_endtie`'s break-on-first-match).
    let mut mixer = Mixer::default();
    mixer.add_voice(keyed_voice(50, 0, 60, 0xFF, 0xFF));
    mixer.add_voice(keyed_voice(50, 0, 60, 0xFF, 0xFF));
    mixer.add_voice(keyed_voice(50, 0, 64, 0xFF, 0xFF));
    mixer.note_off_track(0, 60);
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    mixer.mix_frame(&mut out);
    // Only the first key-60 voice was released and retired.
    assert_eq!(mixer.voice_count(), 2);
}

#[test]
fn note_off_track_matches_the_requested_key() {
    // Key 60 is panned hard-left, key 64 hard-right, both on track 0. An
    // `EOT` on key 64 stops only that voice, silencing the right channel
    // while the left keeps sounding.
    let mut mixer = Mixer::default();
    mixer.add_voice(keyed_voice(60, 0, 60, 0x00, 0xFF));
    mixer.add_voice(keyed_voice(60, 0, 64, 0xFF, 0x00));
    mixer.note_off_track(0, 64);
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    mixer.mix_frame(&mut out);
    let left: f32 = out.iter().step_by(2).map(|s| s.abs()).sum();
    let right: f32 = out.iter().skip(1).step_by(2).map(|s| s.abs()).sum();
    assert_eq!(mixer.voice_count(), 1);
    assert!(left > 0.0, "surviving key-60 voice should keep sounding");
    assert_eq!(right, 0.0, "released key-64 voice should be silent");
}

#[test]
fn note_off_track_releases_the_newest_matching_voice() {
    // Upstream `ply_note` prepends each new channel at the head of the
    // track's chain and `ply_endtie` stops the first match — the newest
    // voice. Two key-60 voices overlap on track 0: the older is panned
    // hard-left, the newer hard-right. An `EOT` must retire the newer
    // (right) voice, leaving the left one sounding. Before the fix the scan
    // ran oldest-first and silenced the left channel instead.
    let mut mixer = Mixer::default();
    mixer.add_voice(keyed_voice(60, 0, 60, 0x00, 0xFF)); // older: left only
    mixer.add_voice(keyed_voice(60, 0, 60, 0xFF, 0x00)); // newer: right only
    mixer.note_off_track(0, 60);
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    mixer.mix_frame(&mut out);
    let left: f32 = out.iter().step_by(2).map(|s| s.abs()).sum();
    let right: f32 = out.iter().skip(1).step_by(2).map(|s| s.abs()).sum();
    assert_eq!(mixer.voice_count(), 1);
    assert!(left > 0.0, "older left voice should keep sounding");
    assert_eq!(right, 0.0, "newer right voice should be released");
}

#[test]
fn note_off_releases_newest_match_across_voice_kinds_pcm_then_cgb() {
    // Same track, same key: an older DirectSound voice then a newer CGB
    // voice (as a `VOICE` change between overlapping ties would produce).
    // Upstream chains both kinds together newest-first, so an `EOT` must
    // release the newer CGB voice — not the older PCM one. Before the fix
    // `note_off_track` scanned the whole PCM pool first and wrongly
    // released the older PCM voice.
    let mut mixer = Mixer::default();
    mixer.add_voice(keyed_voice(50, 0, 60, 0xFF, 0xFF)); // older PCM, seq 0
    mixer.add_cgb_voice(cgb_keyed_voice(0, 60)); // newer CGB, seq 1
    mixer.note_off_track(0, 60);
    let cgb = mixer.cgb_voices()[CgbChannelNumber::Square1.slot()]
        .as_ref()
        .expect("cgb voice present");
    assert!(cgb.is_stopping(), "newer CGB voice must be released");
    assert!(
        !mixer.voices()[0].is_stopping(),
        "older PCM voice must keep sounding"
    );
}

#[test]
fn note_off_releases_newest_match_across_voice_kinds_cgb_then_pcm() {
    // The mirror case: an older CGB voice then a newer DirectSound voice.
    // The `EOT` must release the newer PCM voice, leaving the CGB sounding.
    let mut mixer = Mixer::default();
    mixer.add_cgb_voice(cgb_keyed_voice(0, 60)); // older CGB, seq 0
    mixer.add_voice(keyed_voice(50, 0, 60, 0xFF, 0xFF)); // newer PCM, seq 1
    mixer.note_off_track(0, 60);
    assert!(
        mixer.voices()[0].is_stopping(),
        "newer PCM voice must be released"
    );
    let cgb = mixer.cgb_voices()[CgbChannelNumber::Square1.slot()]
        .as_ref()
        .expect("cgb voice present");
    assert!(!cgb.is_stopping(), "older CGB voice must keep sounding");
}

#[test]
fn voice_cap_drops_extra_notes() {
    let mut mixer = Mixer::new(DEFAULT_MASTER_VOLUME, 2);
    assert!(mixer.add_voice(constant_voice(10, 0)));
    assert!(mixer.add_voice(constant_voice(10, 1)));
    assert!(!mixer.add_voice(constant_voice(10, 2)));
    assert_eq!(mixer.voice_count(), 2);
}

/// A square-1 voice with a sweep byte, at the lowest playable frequency:
/// key `0` is below `MidiKeyToCgbFreq`'s floor, so the register clamps to
/// `2048 - 2004 = 44` and a shift-1 sweep gets 8 upward steps (`66`, `99`,
/// `148`, `222`, `333`, `499`, `748`, `1122`) before the 9th tick's
/// look-ahead overflows `0x7FF` and disables the channel.
fn cgb_sweep_voice(sweep_byte: u8) -> CgbVoice {
    CgbVoice::square(
        CgbChannelNumber::Square1,
        2,
        Some(sweep_byte),
        CgbAdsr::flat(),
        0,
        0,
        0xFF,
        0xFF,
        127,
        0,
        0,
        0,
        0,
        0,
        0,
    )
}

/// The square-1 sweep's shadow frequency, or `None` once the slot retired.
fn square1_sweep_frequency(mixer: &Mixer) -> Option<u16> {
    mixer.cgb_voices()[CgbChannelNumber::Square1.slot()]
        .as_ref()
        .map(|voice| {
            voice
                .sweep_frequency()
                .expect("the square-1 slot holds a sweeping voice")
        })
}

#[test]
fn mix_frame_sweeps_at_128hz_across_frame_boundaries() {
    // The mixer's 128 Hz clock is one clock for the whole stream, not one
    // per render buffer: 224 samples per frame is 2.143 sweep ticks, so a
    // frame carries two ticks or — as the accumulator's fraction carries —
    // three, and the count only comes out right if the phase survives from
    // one `mix_frame` to the next (issue #381).
    //
    // Sweep byte `0x71` steps every 7th tick, so the shadow frequency pins
    // the running tick count exactly. 16 frames are 34 ticks (4 steps: 66,
    // 99, 148, 222); the 17th frame crosses tick 35 and takes the 5th step
    // to 333. A clock restarted every frame would tick a flat 2 per frame —
    // 119.5 Hz, not 128 — reaching only 34 ticks by the 17th frame and
    // still sitting at 222.
    let mut mixer = Mixer::default();
    assert!(mixer.add_cgb_voice(cgb_sweep_voice(0x71)));
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];

    for _ in 0..16 {
        mixer.mix_frame(&mut out);
    }
    assert_eq!(
        square1_sweep_frequency(&mixer),
        Some(222),
        "16 frames must be exactly 34 ticks, i.e. 4 period-7 steps"
    );

    mixer.mix_frame(&mut out);
    assert_eq!(
        square1_sweep_frequency(&mixer),
        Some(333),
        "the 17th frame must cross tick 35 and take the 5th step"
    );
}

#[test]
fn mix_frame_retires_an_overflowing_sweep_on_the_hardware_tick_count() {
    // The same clock decides *when* an overflowing sweep retires its
    // channel. `0x71` from register 44 needs 9 steps, i.e. 63 ticks, to
    // overflow: real 128 Hz reaches tick 63 inside the 30th frame, while a
    // per-frame-restarted clock's flat 2 ticks per frame would not get
    // there until the 32nd (issue #381).
    let mut mixer = Mixer::default();
    assert!(mixer.add_cgb_voice(cgb_sweep_voice(0x71)));
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];

    for _ in 0..29 {
        mixer.mix_frame(&mut out);
    }
    assert_eq!(
        square1_sweep_frequency(&mixer),
        Some(1122),
        "29 frames must be 62 ticks: 8 steps, one tick short of the overflow"
    );

    mixer.mix_frame(&mut out);
    assert!(
        square1_sweep_frequency(&mixer).is_none(),
        "the 30th frame must reach tick 63, overflow, and free the slot"
    );
    assert_eq!(mixer.voice_count(), 0);
}

#[test]
fn a_frames_sweep_tick_buffer_never_has_to_grow() {
    // `mix_frame` reuses one preallocated tick buffer so steady-state
    // rendering stays allocation-free (the `sweep_ticks` field). That only
    // holds if `MAX_SWEEP_TICKS_PER_FRAME` really bounds a frame: 224
    // samples span 2.143 ticks, so frames carry two or three, never more.
    assert_eq!(MAX_SWEEP_TICKS_PER_FRAME, 3);

    let mut clock = FrameSequencer128Hz::default();
    let mut ticks = Vec::new();
    let mut seen_three = false;
    for _ in 0..1000 {
        clock.advance_into(SAMPLES_PER_FRAME, &mut ticks);
        assert!(
            ticks.len() <= MAX_SWEEP_TICKS_PER_FRAME,
            "a frame produced {} ticks",
            ticks.len()
        );
        seen_three |= ticks.len() == MAX_SWEEP_TICKS_PER_FRAME;
    }
    assert!(seen_three, "the fractional carry must reach three ticks");
}
