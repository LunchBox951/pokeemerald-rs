//! End-tie selection across keys, note ages, voice kinds, and pseudo-echo tails.

use super::test_support::*;
use super::*;
use crate::cgb_envelope::CgbAdsr;
use crate::cgb_voice::CgbChannelNumber;

#[test]
fn note_off_track_stops_only_the_newest_matching_voice() {
    const TRACK: usize = 0;
    const REPEATED_KEY: u8 = 60;
    const OTHER_KEY: u8 = 64;

    let mut mixer = Mixer::default();
    // Panned apart so the assertions can tell WHICH matching voice stopped.
    mixer.add_voice(keyed_voice(
        50,
        TRACK,
        REPEATED_KEY,
        MUTED_TRACK_VOLUME,
        FULL_TRACK_VOLUME,
    ));
    mixer.add_voice(keyed_voice(
        50,
        TRACK,
        REPEATED_KEY,
        FULL_TRACK_VOLUME,
        MUTED_TRACK_VOLUME,
    ));
    mixer.add_voice(keyed_voice(
        50,
        TRACK,
        OTHER_KEY,
        MUTED_TRACK_VOLUME,
        MUTED_TRACK_VOLUME,
    ));
    mixer.note_off_track(TRACK, REPEATED_KEY);
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    mixer.mix_frame(&mut out);
    let left_energy: f32 = out.iter().step_by(2).map(|sample| sample.abs()).sum();
    let right_energy: f32 = out
        .iter()
        .skip(1)
        .step_by(2)
        .map(|sample| sample.abs())
        .sum();
    assert_eq!(mixer.voice_count(), 2);
    assert!(left_energy > 0.0, "the older matching voice keeps sounding");
    assert_eq!(
        right_energy, 0.0,
        "the newest matching voice is the one that stops"
    );
}

#[test]
fn note_off_track_matches_the_requested_key() {
    const TRACK: usize = 0;
    const LEFT_KEY: u8 = 60;
    const RIGHT_KEY: u8 = 64;

    let mut mixer = Mixer::default();
    mixer.add_voice(keyed_voice(
        60,
        TRACK,
        LEFT_KEY,
        MUTED_TRACK_VOLUME,
        FULL_TRACK_VOLUME,
    ));
    mixer.add_voice(keyed_voice(
        60,
        TRACK,
        RIGHT_KEY,
        FULL_TRACK_VOLUME,
        MUTED_TRACK_VOLUME,
    ));
    mixer.note_off_track(TRACK, RIGHT_KEY);
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    mixer.mix_frame(&mut out);
    let left_energy: f32 = out.iter().step_by(2).map(|sample| sample.abs()).sum();
    let right_energy: f32 = out
        .iter()
        .skip(1)
        .step_by(2)
        .map(|sample| sample.abs())
        .sum();
    assert_eq!(mixer.voice_count(), 1);
    assert!(
        left_energy > 0.0,
        "the voice on the unrequested key must keep sounding"
    );
    assert_eq!(
        right_energy, 0.0,
        "the voice on the requested key must stop"
    );
}

#[test]
fn note_off_track_releases_the_newest_matching_voice() {
    const TRACK: usize = 0;
    const KEY: u8 = 60;

    let mut mixer = Mixer::default();
    let older_left_voice = keyed_voice(60, TRACK, KEY, MUTED_TRACK_VOLUME, FULL_TRACK_VOLUME);
    let newer_right_voice = keyed_voice(60, TRACK, KEY, FULL_TRACK_VOLUME, MUTED_TRACK_VOLUME);
    mixer.add_voice(older_left_voice);
    mixer.add_voice(newer_right_voice);
    mixer.note_off_track(TRACK, KEY);
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    mixer.mix_frame(&mut out);
    let left_energy: f32 = out.iter().step_by(2).map(|sample| sample.abs()).sum();
    let right_energy: f32 = out
        .iter()
        .skip(1)
        .step_by(2)
        .map(|sample| sample.abs())
        .sum();
    assert_eq!(mixer.voice_count(), 1);
    assert!(left_energy > 0.0, "the older voice must keep sounding");
    assert_eq!(right_energy, 0.0, "the newer voice must be released");
}

#[test]
fn note_off_releases_newest_match_across_voice_kinds_pcm_then_cgb() {
    const TRACK: usize = 0;
    const KEY: u8 = 60;

    let mut mixer = Mixer::default();
    let older_direct_sound_voice =
        keyed_voice(50, TRACK, KEY, FULL_TRACK_VOLUME, FULL_TRACK_VOLUME);
    let newer_cgb_voice = cgb_keyed_voice(TRACK, KEY);
    mixer.add_voice(older_direct_sound_voice);
    mixer.add_cgb_voice(newer_cgb_voice);
    mixer.note_off_track(TRACK, KEY);
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
    const TRACK: usize = 0;
    const KEY: u8 = 60;

    let mut mixer = Mixer::default();
    let older_cgb_voice = cgb_keyed_voice(TRACK, KEY);
    let newer_direct_sound_voice =
        keyed_voice(50, TRACK, KEY, FULL_TRACK_VOLUME, FULL_TRACK_VOLUME);
    mixer.add_cgb_voice(older_cgb_voice);
    mixer.add_voice(newer_direct_sound_voice);
    mixer.note_off_track(TRACK, KEY);
    assert!(
        mixer.voices()[0].is_stopping(),
        "newer PCM voice must be released"
    );
    let cgb = mixer.cgb_voices()[CgbChannelNumber::Square1.slot()]
        .as_ref()
        .expect("cgb voice present");
    assert!(!cgb.is_stopping(), "older CGB voice must keep sounding");
}

/// A square-channel voice on `channel` with the given envelope and
/// pseudo-echo tail, on track 0 and key 60 like [`cgb_keyed_voice`].
fn cgb_endtie_voice(
    channel: CgbChannelNumber,
    adsr: CgbAdsr,
    echo_volume: u8,
    echo_length: u8,
) -> CgbVoice {
    CgbVoice::square(
        channel,
        2,
        None,
        adsr,
        60,
        0,
        FULL_TRACK_VOLUME,
        FULL_TRACK_VOLUME,
        TEST_VELOCITY,
        TIED_GATE_TIME,
        60,
        0,
        0,
        echo_volume,
        echo_length,
    )
}

#[test]
fn end_tie_skips_a_cgb_channel_in_its_automatic_pseudo_echo_tail() {
    // A zero-sustain CGB envelope completes on its own into the automatic
    // pseudo-echo tail that `CgbEnvelope::is_end_tie_eligible`'s doc excludes.
    const TRACK: usize = 0;
    const KEY: u8 = 60;

    let mut mixer = Mixer::default();
    let older_sustaining_voice = cgb_endtie_voice(CgbChannelNumber::Square2, CgbAdsr::flat(), 0, 0);
    let newer_echo_voice = cgb_endtie_voice(
        CgbChannelNumber::Square1,
        CgbAdsr {
            attack: 0,
            decay: 0,
            sustain: 0,
            release: 0,
        },
        128,
        60,
    );
    assert!(mixer.add_cgb_voice(older_sustaining_voice));
    assert!(mixer.add_cgb_voice(newer_echo_voice));

    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    mixer.mix_frame(&mut out);
    assert!(
        mixer.cgb_voices()[CgbChannelNumber::Square1.slot()].is_some(),
        "the zero-sustain voice must have entered its pseudo-echo tail, not retired"
    );

    mixer.note_off_track(TRACK, KEY);

    let echo = mixer.cgb_voices()[CgbChannelNumber::Square1.slot()]
        .as_ref()
        .expect("echo voice present");
    assert!(
        !echo.is_stopping(),
        "the end-tie must walk past the IEC-only pseudo-echo channel"
    );
    let older = mixer.cgb_voices()[CgbChannelNumber::Square2.slot()]
        .as_ref()
        .expect("older voice present");
    assert!(
        older.is_stopping(),
        "the older same-key channel must be the one the end-tie stops"
    );
}
