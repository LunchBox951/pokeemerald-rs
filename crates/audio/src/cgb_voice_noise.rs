use super::fixture::*;
use super::*;

#[test]
fn noise_control_byte_sets_width_bit_only_for_odd_period() {
    assert_eq!(midi_key_to_noise_control(TEST_KEY) & NOISE_WIDTH_BIT, 0);
    assert_eq!(
        noise_control_byte(TEST_KEY, NARROW_NOISE) & NOISE_WIDTH_BIT,
        NOISE_WIDTH_BIT
    );
    assert_eq!(
        noise_control_byte(TEST_KEY, WIDE_NOISE) & NOISE_WIDTH_BIT,
        0
    );
}

#[test]
fn noise_period_bit_drives_narrow_lfsr_mode() {
    let narrow = noise_voice(CgbAdsr::flat(), NARROW_NOISE, TestNote::default());
    assert_eq!(narrow.noise_is_narrow(), Some(true));
    let wide = noise_voice(CgbAdsr::flat(), WIDE_NOISE, TestNote::default());
    assert_eq!(wide.noise_is_narrow(), Some(false));
}

#[test]
fn noise_retune_preserves_the_width_bit() {
    let mut narrow = noise_voice(CgbAdsr::flat(), NARROW_NOISE, TestNote::default());
    let octave_up = 12;
    narrow.set_track_pitch(octave_up, 0);
    assert_eq!(narrow.noise_is_narrow(), Some(true));
}

#[test]
fn release_start_volume_write_retriggers_the_noise_lfsr() {
    // Release start is a volume write; upstream's channel-4 trigger
    // resets the LFSR (`m4a.c:1060-1069,1219-1226`; `mgba/src/gb/audio.c:374`).
    let adsr = CgbAdsr {
        attack: 0,
        decay: 0,
        sustain: MAX_ADSR_LEVEL,
        release: 4,
    };
    let mut voice = noise_voice(adsr, WIDE_NOISE, TestNote::default());
    let at_note_on = voice.noise_lfsr();
    assert_eq!(
        at_note_on,
        Some(0),
        "a trigger resets the LFSR without clocking"
    );

    let mut acc = vec![(0i32, 0i32); 64];
    for _ in 0..3 {
        voice.begin_frame(false);
        voice.render(&mut acc, &[]);
    }
    let walked_away = voice.noise_lfsr();
    assert_ne!(
        walked_away, at_note_on,
        "sanity: a sustained note must have walked the LFSR away from note-on"
    );

    voice.note_off();
    assert_eq!(
        voice.noise_lfsr(),
        walked_away,
        "sanity: note_off must not retrigger before the next begin_frame -- upstream applies \
         a tick's writes inside the following CgbSound call, not synchronously"
    );
    voice.begin_frame(false);
    assert_eq!(voice.noise_lfsr(), Some(0));
    assert_eq!(
        voice.noise_lfsr(),
        at_note_on,
        "note_off's release-start volume write (release != 0) must retrigger channel 4 at \
         the next begin_frame, clearing the LFSR"
    );
}

const LATCH_HIGH: i8 = 1;
const SLOW_NOISE_KEY: u8 = 21;
const MAX_SAMPLES_TO_LATCH_HIGH: usize = 256;

/// A committed noise voice partway through a frame, its output latch high.
fn noise_voice_latched_high(width_selector: u8) -> CgbVoice {
    let mut voice = noise_voice(CgbAdsr::flat(), width_selector, TestNote::default());
    voice.begin_frame(false);
    let mut acc = [(0i32, 0i32); 1];
    for _ in 0..MAX_SAMPLES_TO_LATCH_HIGH {
        if voice.noise_output_latch() == Some(LATCH_HIGH) {
            return voice;
        }
        voice.render(&mut acc, &[]);
    }
    panic!("noise latch never reached high");
}

fn slow_noise_replacement(width_selector: u8) -> CgbVoice {
    noise_voice(
        CgbAdsr::flat(),
        width_selector,
        TestNote::at_key(SLOW_NOISE_KEY),
    )
}

fn first_mixed_sample(mixer: &mut crate::mixer::Mixer) -> f32 {
    let mut out = vec![0.0f32; crate::SAMPLES_PER_FRAME * 2];
    mixer.mix_frame(&mut out);
    out[0]
}

#[test]
fn replacing_a_sounding_noise_voice_keeps_its_output_latch() {
    for width in [WIDE_NOISE, NARROW_NOISE] {
        let mut mixer = crate::mixer::Mixer::default();
        assert!(mixer.add_cgb_voice(noise_voice_latched_high(width)));
        assert!(mixer.add_cgb_voice(slow_noise_replacement(width)));
        assert!(
            first_mixed_sample(&mut mixer) > 0.0,
            "a replacement must start on the predecessor's high latch (width {width})"
        );
    }
}

/// Upstream `CgbOscOff` writes `NR42 = 8; NR44 = 0x80` (`m4a.c:873-874`): a
/// zero-volume retrigger that keeps the LFSR clocking, and mGBA rewrites
/// `ch4.sample = lsb * currentVolume` (`gb/audio.c:641`) on every clock, so an
/// idled noise slot settles at the low latch before the next note.
#[test]
fn a_noise_note_after_retirement_and_idle_frames_starts_low() {
    for width in [WIDE_NOISE, NARROW_NOISE] {
        let mut mixer = crate::mixer::Mixer::default();
        assert!(mixer.add_cgb_voice(noise_voice_latched_high(width)));
        mixer.stop_track(0);
        for _ in 0..3 {
            first_mixed_sample(&mut mixer);
        }
        assert!(mixer.add_cgb_voice(slow_noise_replacement(width)));
        assert!(
            first_mixed_sample(&mut mixer) < 0.0,
            "the off-write's zero-volume clocking leaves the latch low (width {width})"
        );
    }
}

#[test]
fn a_noise_note_after_release_retirement_starts_low() {
    for width in [WIDE_NOISE, NARROW_NOISE] {
        let mut mixer = crate::mixer::Mixer::default();
        assert!(mixer.add_cgb_voice(noise_voice_latched_high(width)));
        mixer.note_off_track(0, TEST_KEY);
        first_mixed_sample(&mut mixer);
        assert!(mixer.add_cgb_voice(slow_noise_replacement(width)));
        assert!(
            first_mixed_sample(&mut mixer) < 0.0,
            "a released note's off-write leaves the latch low (width {width})"
        );
    }
}
