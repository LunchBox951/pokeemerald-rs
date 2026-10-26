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
fn a_live_noise_retune_carries_elapsed_time_across_full_frames() {
    // `mgba/src/gb/audio.c:354-358,602-607,630-644`: `lastEvent` survives an NR43 write.
    assert_eq!(crate::SAMPLES_PER_FRAME, 224);
    assert_eq!(midi_key_to_noise_control(47), 0x75);
    assert_eq!(midi_key_to_noise_control(51), 0x65);
    let mut voice = noise_voice(CgbAdsr::flat(), WIDE_NOISE, TestNote::at_key(47));
    voice.begin_frame(false);
    assert_eq!(voice.hardware_volume(), FULL_VOLUME);
    let mut first = [(0i32, 0i32); crate::SAMPLES_PER_FRAME];
    voice.render(&mut first, &[]);
    assert_eq!(voice.noise_lfsr(), Some(0x7E00));
    let held_latch = voice.noise_output_latch();

    voice.set_track_pitch(4, 0);
    assert_eq!(voice.noise_is_narrow(), Some(false));
    assert_eq!(voice.noise_lfsr(), Some(0x7E00));
    assert_eq!(voice.noise_output_latch(), held_latch);
    voice.begin_frame(false);
    let mut contiguous = voice.clone();
    let mut second = [(0i32, 0i32); crate::SAMPLES_PER_FRAME];
    voice.render(&mut second[..1], &[]);
    assert_eq!(voice.noise_lfsr(), Some(0x7F00));
    voice.render(&mut second[1..4], &[]);
    assert_eq!(voice.noise_lfsr(), Some(0x7F00));
    voice.render(&mut second[4..5], &[]);
    assert_eq!(voice.noise_lfsr(), Some(0x7F80));
    voice.render(&mut second[5..], &[]);
    assert_eq!(voice.noise_lfsr(), Some(0x7EFF));

    let mut whole = [(0i32, 0i32); crate::SAMPLES_PER_FRAME];
    contiguous.render(&mut whole, &[]);
    assert_eq!(whole, second);
    for (index, &sample) in second.iter().enumerate() {
        let level = if (118..135).contains(&index) {
            0
        } else {
            FULL_VOLUME
        };
        let expected = expected_contribution(level, FULL_VOLUME);
        assert_eq!(sample, (expected, expected), "sample {}", index + 1);
    }
}

#[test]
fn a_trigger_after_an_accelerating_retune_settles_the_owed_clocks_first() {
    // Channel 4 runs before every NR42/NR44 write (`mgba/src/gb/audio.c:347,362,602-644`),
    // so owed clocks land before the trigger resets the LFSR (`audio.c:374`).
    assert_eq!(midi_key_to_noise_control(47), 0x75);
    assert_eq!(midi_key_to_noise_control(55), 0x55);
    let mut voice = noise_voice(CgbAdsr::flat(), WIDE_NOISE, TestNote::at_key(47));
    voice.begin_frame(false);
    let mut first = [(0i32, 0i32); 30];
    voice.render(&mut first, &[]);
    assert_eq!(voice.noise_lfsr(), Some(0));
    assert_eq!(voice.noise_output_latch(), Some(0));

    // 30 samples at 0x75 cover three whole 0x55 periods: LFSR 0 -> 0x4000 -> 0x6000 -> 0x7000.
    voice.set_track_pitch(8, 0);
    voice.set_track_volume(FULL_TRACK_VOLUME, FULL_TRACK_VOLUME);
    voice.begin_frame(false);
    assert_eq!(voice.noise_lfsr(), Some(0));
    assert_eq!(voice.noise_output_latch(), Some(FULL_VOLUME));
    let mut second = [(0i32, 0i32); 1];
    voice.render(&mut second, &[]);
    let expected = expected_contribution(FULL_VOLUME, FULL_VOLUME);
    assert_eq!(second[0], (expected, expected));
}

#[test]
fn release_start_volume_write_retriggers_the_noise_lfsr() {
    // `m4a.c:1060-1069,1219-1226`; `mgba/src/gb/audio.c:374`.
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

const SLOW_NOISE_KEY: u8 = 21;
const MAX_SAMPLES_TO_LATCH_HIGH: usize = 20_000;
const FULL_VOLUME: u8 = 14;
/// Samples before key 21's first LFSR clock after a restart: none lands in
/// the first 13 frames, one does in the 14th.
const SLOW_KEY_FIRST_CLOCK_SAMPLE: usize = 2979;

/// Pre-routing contribution of a noise latch `level` centred on volume `centre`.
fn expected_contribution(level: u8, centre: u8) -> i32 {
    ((2 * i32::from(level) - i32::from(centre)) * 16 * 127) >> 8
}

fn expected_mixed(level: u8, centre: u8) -> f32 {
    let clipped = expected_contribution(level, centre).clamp(-128, 127);
    #[expect(clippy::cast_precision_loss, reason = "within [-128, 127]")]
    let value = clipped as f32;
    value / 128.0
}

/// Mixed samples are exact multiples of 1/128, so bit equality is the check.
#[track_caller]
fn assert_mixed(actual: f32, expected: f32, context: &str) {
    assert_eq!(
        actual.to_bits(),
        expected.to_bits(),
        "{context}: {actual} vs {expected}"
    );
}

fn sustained_adsr(sustain: u8) -> CgbAdsr {
    CgbAdsr {
        sustain,
        ..CgbAdsr::flat()
    }
}

fn noise_voice_at(adsr: CgbAdsr, key: u8, width_selector: u8) -> CgbVoice {
    noise_voice(adsr, width_selector, TestNote::at_key(key))
}

/// A committed noise voice mid-frame whose latch a real LFSR clock raised.
fn noise_voice_latched(adsr: CgbAdsr, key: u8, width_selector: u8) -> CgbVoice {
    let mut voice = noise_voice_at(adsr, key, width_selector);
    voice.begin_frame(false);
    let mut acc = [(0i32, 0i32); 1];
    for _ in 0..MAX_SAMPLES_TO_LATCH_HIGH {
        if voice.noise_output_latch().is_some_and(|level| level > 0) {
            return voice;
        }
        voice.render(&mut acc, &[]);
    }
    panic!("noise latch never rose");
}

fn noise_voice_latched_high(width_selector: u8) -> CgbVoice {
    noise_voice_latched(CgbAdsr::flat(), TEST_KEY, width_selector)
}

fn slow_noise_replacement(width_selector: u8) -> CgbVoice {
    noise_voice_at(CgbAdsr::flat(), SLOW_NOISE_KEY, width_selector)
}

fn first_mixed_sample(mixer: &mut crate::mixer::Mixer) -> f32 {
    let mut out = vec![0.0f32; crate::SAMPLES_PER_FRAME * 2];
    mixer.mix_frame(&mut out);
    out[0]
}

fn first_sample_after(predecessor: &CgbVoice, mut replacement: CgbVoice) -> i32 {
    replacement.carry_hardware_state_from(predecessor);
    replacement.begin_frame(false);
    let mut acc = [(0i32, 0i32); 1];
    replacement.render(&mut acc, &[]);
    acc[0].0
}

/// A direct NR42 volume write to zero leaves `ch4.sample` alone
/// (`gb/audio.c:641,730-732,890-913`).
#[test]
fn a_volume_write_to_zero_keeps_the_nonzero_latch() {
    for width in [WIDE_NOISE, NARROW_NOISE] {
        let mut predecessor = noise_voice_latched(CgbAdsr::flat(), SLOW_NOISE_KEY, width);
        predecessor.apply_hardware_off_write();
        assert_eq!(predecessor.hardware_volume(), 0);
        assert_eq!(predecessor.noise_output_latch(), Some(FULL_VOLUME));
        assert_eq!(
            first_sample_after(&predecessor, slow_noise_replacement(width)),
            expected_contribution(FULL_VOLUME, FULL_VOLUME),
            "width {width}"
        );
    }
}

/// `gb/audio.c:730-732`.
#[test]
fn an_envelope_step_follows_a_raised_latch_and_ignores_a_low_one() {
    let decaying = CgbAdsr {
        attack: 0,
        decay: 1,
        sustain: 4,
        release: 0,
    };
    let mut voice = noise_voice_latched(decaying, SLOW_NOISE_KEY, WIDE_NOISE);
    let mut acc = [(0i32, 0i32); 1];
    let mut previous = voice.hardware_volume();
    for _ in 0..40 {
        voice.begin_frame(false);
        let volume = voice.hardware_volume();
        if volume != previous {
            assert_eq!(
                voice.noise_output_latch(),
                Some(volume),
                "a raised latch follows the stepped volume"
            );
            return;
        }
        previous = volume;
        voice.render(&mut acc, &[]);
    }
    panic!("the envelope never stepped");
}

/// `CgbOscOff` writes `NR42 = 8; NR44 = 0x80` (`m4a.c:873-874`), which leaves
/// `ch4.sample` until the next clock (`gb/audio.c:606-641`).
#[test]
fn a_noise_note_after_retirement_keeps_a_slow_latch_until_the_first_clock() {
    for width in [WIDE_NOISE, NARROW_NOISE] {
        let mut mixer = crate::mixer::Mixer::default();
        assert!(mixer.add_cgb_voice(noise_voice_latched(CgbAdsr::flat(), SLOW_NOISE_KEY, width)));
        mixer.stop_track(0);
        for _ in 0..3 {
            assert!(
                first_mixed_sample(&mut mixer) > 0.0,
                "the idle slot keeps emitting its latch (width {width})"
            );
        }
        assert!(mixer.add_cgb_voice(slow_noise_replacement(width)));
        assert_mixed(
            first_mixed_sample(&mut mixer),
            expected_mixed(0, 0),
            "the replacement's width-only write clocks the idle slot's elapsed time at the \
             off-write's zero volume (width {width})",
        );
    }
}

/// With no time between the off-write and the next note-on, no catch-up clock
/// is owed, so the latch survives (`gb/audio.c:371-383,602-644`).
#[test]
fn a_noise_note_added_at_the_off_write_keeps_the_latch() {
    for width in [WIDE_NOISE, NARROW_NOISE] {
        let mut mixer = crate::mixer::Mixer::default();
        assert!(mixer.add_cgb_voice(noise_voice_latched(CgbAdsr::flat(), SLOW_NOISE_KEY, width)));
        mixer.stop_track(0);
        assert!(mixer.add_cgb_voice(slow_noise_replacement(width)));
        assert_mixed(
            first_mixed_sample(&mut mixer),
            expected_mixed(FULL_VOLUME, FULL_VOLUME),
            "no elapsed time, no catch-up clock (width {width})",
        );
    }
}

#[test]
fn an_idle_noise_slot_settles_its_latch_at_the_first_clock_after_the_off_write() {
    let frames_before_clock = SLOW_KEY_FIRST_CLOCK_SAMPLE / crate::SAMPLES_PER_FRAME;
    for width in [WIDE_NOISE, NARROW_NOISE] {
        let mut mixer = crate::mixer::Mixer::default();
        assert!(mixer.add_cgb_voice(noise_voice_latched(CgbAdsr::flat(), SLOW_NOISE_KEY, width)));
        mixer.stop_track(0);
        for frame in 0..frames_before_clock {
            assert!(
                first_mixed_sample(&mut mixer) > 0.0,
                "frame {frame} precedes the first clock (width {width})"
            );
        }
        let mut out = vec![0.0f32; crate::SAMPLES_PER_FRAME * 2];
        mixer.mix_frame(&mut out);
        assert!(out[0] > 0.0, "the clock lands mid-frame");
        assert_eq!(
            out[crate::SAMPLES_PER_FRAME * 2 - 2].to_bits(),
            0.0f32.to_bits(),
            "the clock settles the zero-volume latch (width {width})"
        );
        assert!(mixer.add_cgb_voice(slow_noise_replacement(width)));
        assert_mixed(
            first_mixed_sample(&mut mixer),
            expected_mixed(0, 0),
            "a settled slot starts the next note low (width {width})",
        );
    }
}

/// `gb/audio.c:382`.
#[test]
fn a_second_off_write_restarts_the_phase_and_keeps_the_latch() {
    let mut mixer = crate::mixer::Mixer::default();
    assert!(mixer.add_cgb_voice(noise_voice_latched(
        CgbAdsr::flat(),
        SLOW_NOISE_KEY,
        WIDE_NOISE
    )));
    mixer.stop_track(0);
    assert!(mixer.add_cgb_voice(slow_noise_replacement(WIDE_NOISE)));
    // Stay short of the restarted phase's first clock, then off-write again.
    for _ in 0..(SLOW_KEY_FIRST_CLOCK_SAMPLE / crate::SAMPLES_PER_FRAME - 1) {
        assert!(first_mixed_sample(&mut mixer) > 0.0);
    }
    mixer.stop_track(0);
    assert!(mixer.add_cgb_voice(slow_noise_replacement(WIDE_NOISE)));
    assert_mixed(
        first_mixed_sample(&mut mixer),
        expected_mixed(FULL_VOLUME, FULL_VOLUME),
        "second off-write keeps the latch",
    );
}

#[test]
fn a_noise_note_after_release_retirement_keeps_a_slow_latch_until_the_first_clock() {
    for width in [WIDE_NOISE, NARROW_NOISE] {
        let mut mixer = crate::mixer::Mixer::default();
        assert!(mixer.add_cgb_voice(noise_voice_latched(CgbAdsr::flat(), SLOW_NOISE_KEY, width)));
        mixer.note_off_track(0, SLOW_NOISE_KEY);
        assert!(first_mixed_sample(&mut mixer) > 0.0);
        assert!(mixer.add_cgb_voice(slow_noise_replacement(width)));
        assert_mixed(
            first_mixed_sample(&mut mixer),
            expected_mixed(0, 0),
            "the frame after a released note's off-write owes fast catch-up clocks at volume 0 \
             (width {width})",
        );
    }
}

#[test]
fn a_noise_note_after_fast_retirement_and_idle_frames_starts_low() {
    for width in [WIDE_NOISE, NARROW_NOISE] {
        let mut mixer = crate::mixer::Mixer::default();
        assert!(mixer.add_cgb_voice(noise_voice_latched_high(width)));
        mixer.stop_track(0);
        for _ in 0..3 {
            first_mixed_sample(&mut mixer);
        }
        assert!(mixer.add_cgb_voice(slow_noise_replacement(width)));
        assert_mixed(
            first_mixed_sample(&mut mixer),
            expected_mixed(0, 0),
            "the off-write's zero-volume clocking leaves the latch low (width {width})",
        );
    }
}

#[test]
fn a_noise_note_after_fast_release_retirement_starts_low() {
    for width in [WIDE_NOISE, NARROW_NOISE] {
        let mut mixer = crate::mixer::Mixer::default();
        assert!(mixer.add_cgb_voice(noise_voice_latched_high(width)));
        mixer.note_off_track(0, TEST_KEY);
        first_mixed_sample(&mut mixer);
        assert!(mixer.add_cgb_voice(slow_noise_replacement(width)));
        assert_mixed(
            first_mixed_sample(&mut mixer),
            expected_mixed(0, 0),
            "a released note's off-write leaves the latch low (width {width})",
        );
    }
}

/// Zero initial volume with a decreasing envelope disables channel 4
/// (`gb/audio.c:371-372,856-860,782`).
#[test]
fn a_dac_disabled_replacement_holds_the_inherited_latch() {
    let predecessor = noise_voice_latched(CgbAdsr::flat(), SLOW_NOISE_KEY, WIDE_NOISE);
    let disabled = CgbAdsr {
        attack: 8,
        ..CgbAdsr::flat()
    };
    let mut replacement = noise_voice_at(disabled, SLOW_NOISE_KEY, WIDE_NOISE);
    replacement.carry_hardware_state_from(&predecessor);
    replacement.begin_frame(false);
    assert_eq!(
        replacement.hardware_volume(),
        0,
        "sanity: volume-zero trigger"
    );
    let mut acc = [(0i32, 0i32); 1];
    for _ in 0..(2 * SLOW_KEY_FIRST_CLOCK_SAMPLE) {
        replacement.render(&mut acc, &[]);
        assert_eq!(
            acc[0].0,
            expected_contribution(FULL_VOLUME, FULL_VOLUME),
            "the disabled channel keeps emitting the held latch"
        );
        acc[0] = (0, 0);
    }
    assert_eq!(replacement.noise_output_latch(), Some(FULL_VOLUME));
}

/// Centred goal 16 (`m4a.c:908-911`) wraps to a zero NR42 volume nibble
/// (`m4a.c:1222`).
const WRAPPING_GOAL_TRACK_VOLUME: u8 = 132;
const WRAPPING_GOAL: u8 = 16;

fn latch_after_slow_clock(adsr: CgbAdsr, note: TestNote) -> (CgbVoice, Option<u8>) {
    let predecessor = noise_voice_latched(CgbAdsr::flat(), SLOW_NOISE_KEY, WIDE_NOISE);
    let mut replacement = noise_voice(adsr, WIDE_NOISE, note);
    replacement.carry_hardware_state_from(&predecessor);
    replacement.begin_frame(false);
    let mut acc = [(0i32, 0i32); 1];
    for _ in 0..(2 * SLOW_KEY_FIRST_CLOCK_SAMPLE) {
        replacement.render(&mut acc, &[]);
    }
    let latch = replacement.noise_output_latch();
    (replacement, latch)
}

/// `m4a.c:1158,1222`: NR42 `decay | 0x00` fails the DAC test
/// (`gb/audio.c:346-352,856-860`).
#[test]
fn a_wrapped_zero_volume_decreasing_store_stops_clocking() {
    let decaying = CgbAdsr {
        attack: 0,
        decay: 1,
        ..CgbAdsr::flat()
    };
    let note = TestNote {
        track_right: WRAPPING_GOAL_TRACK_VOLUME,
        track_left: WRAPPING_GOAL_TRACK_VOLUME,
        ..TestNote::at_key(SLOW_NOISE_KEY)
    };
    let (replacement, latch) = latch_after_slow_clock(decaying, note);
    assert_eq!(
        replacement.envelope_volume(),
        WRAPPING_GOAL,
        "sanity: unmasked volume 16"
    );
    assert_eq!(
        replacement.hardware_volume(),
        0,
        "sanity: nibble wraps to 0"
    );
    assert_eq!(
        latch,
        Some(FULL_VOLUME),
        "the disabled LFSR holds the latch"
    );
}

/// Attack 16 stores NR42 `(16 + 8) & 0xF = 0x08` (`m4a.c:1024,1034,1222`);
/// bit 3 keeps the DAC on (`gb/audio.c:856-860,641`).
#[test]
fn a_zero_volume_store_with_the_direction_bit_keeps_clocking() {
    let attacking = CgbAdsr {
        attack: 16,
        ..CgbAdsr::flat()
    };
    let (replacement, latch) = latch_after_slow_clock(attacking, TestNote::at_key(SLOW_NOISE_KEY));
    assert_eq!(
        replacement.hardware_volume(),
        0,
        "sanity: volume-zero trigger"
    );
    assert_eq!(latch, Some(0), "a zero-volume clock settles the latch low");
}

/// The mixer is not idle until the first zero-volume clock settles the latch.
#[test]
fn a_vacated_slow_noise_slot_is_not_idle_until_its_latch_settles() {
    let frames_before_clock = SLOW_KEY_FIRST_CLOCK_SAMPLE / crate::SAMPLES_PER_FRAME;
    let mut mixer = crate::mixer::Mixer::default();
    assert!(mixer.add_cgb_voice(noise_voice_latched(
        CgbAdsr::flat(),
        SLOW_NOISE_KEY,
        WIDE_NOISE
    )));
    mixer.stop_track(0);
    for frame in 0..=frames_before_clock {
        assert!(!mixer.is_idle(), "frame {frame}: the latch is unsettled");
        first_mixed_sample(&mut mixer);
    }
    assert!(mixer.is_idle(), "the first zero-volume clock settled it");
}

/// `gb/audio.c:371-383,641`.
#[test]
fn retiring_from_a_low_clock_leaves_no_idle_tail() {
    let mut voice = noise_voice_at(CgbAdsr::flat(), TEST_KEY, WIDE_NOISE);
    voice.begin_frame(false);
    let mut acc = [(0i32, 0i32); 1];
    let mut raised = false;
    for _ in 0..MAX_SAMPLES_TO_LATCH_HIGH {
        match voice.noise_output_latch() {
            Some(level) if level > 0 => raised = true,
            Some(0) if raised => break,
            _ => {}
        }
        voice.render(&mut acc, &[]);
    }
    assert_eq!(voice.noise_output_latch(), Some(0), "sanity: low clock");
    let mut mixer = crate::mixer::Mixer::default();
    assert!(mixer.add_cgb_voice(voice));
    mixer.stop_track(0);
    assert!(mixer.is_idle(), "a zero latch has no settling tail");
    assert_mixed(
        first_mixed_sample(&mut mixer),
        expected_mixed(0, 0),
        "no settling edge after retirement",
    );
}

/// A note that retires on its first pass still wrote the width-only `NR43`
/// in the start block (`m4a.c:1020-1022`) but jumps to `oscillator_off` past the
/// pitched `NR43` (`m4a.c:1197-1202`), so `CgbOscOff` restarts channel 4 at the
/// width-only rate and the first idle sample clocks the latch low at volume 0.
#[test]
fn an_immediately_retiring_noise_note_leaves_the_width_only_rate() {
    let retiring = CgbAdsr {
        attack: 0,
        decay: 0,
        sustain: 0,
        release: 0,
    };
    for width in [WIDE_NOISE, NARROW_NOISE] {
        let mut mixer = crate::mixer::Mixer::default();
        assert!(mixer.add_cgb_voice(noise_voice_latched(CgbAdsr::flat(), SLOW_NOISE_KEY, width)));
        mixer.stop_track(0);
        assert!(mixer.add_cgb_voice(noise_voice_at(retiring, SLOW_NOISE_KEY, width)));
        let mut out = vec![0.0f32; crate::SAMPLES_PER_FRAME * 2];
        mixer.mix_frame(&mut out);
        assert_eq!(
            mixer.voice_count(),
            0,
            "sanity: the note retired on its first pass"
        );
        assert_mixed(
            out[crate::SAMPLES_PER_FRAME * 2 - 2],
            expected_mixed(0, 0),
            &format!("the width-only rate settles the off-write latch (width {width})"),
        );
    }
}

#[path = "cgb_voice_noise_replacement.rs"]
mod replacement_tests;

#[path = "cgb_voice_noise_retirement.rs"]
mod retirement_tests;
