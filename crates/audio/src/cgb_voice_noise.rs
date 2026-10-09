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

const SLOW_NOISE_KEY: u8 = 21;
const MAX_SAMPLES_TO_LATCH_HIGH: usize = 20_000;
const FULL_VOLUME: u8 = 14;
/// The idle frames a slow key (21) needs after a restart before its first
/// LFSR clock fires: 2979 samples at the model's rate, so no clock lands in
/// the first 13 frames and one does in the 14th.
const SLOW_KEY_FIRST_CLOCK_SAMPLE: usize = 2979;

/// A pre-routing contribution for a noise latch `level` resolved against
/// volume `centre` (the render path's DC centre).
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

/// A committed noise voice partway through a frame, its output latch raised
/// (nonzero) by a real LFSR clock.
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

/// The first rendered sample of `replacement` once it inherits `predecessor`.
fn first_sample_after(predecessor: &CgbVoice, mut replacement: CgbVoice) -> i32 {
    replacement.carry_hardware_state_from(predecessor);
    replacement.begin_frame(false);
    let mut acc = [(0i32, 0i32); 1];
    replacement.render(&mut acc, &[]);
    acc[0].0
}

#[test]
fn replacing_a_sounding_noise_voice_keeps_its_output_latch() {
    for width in [WIDE_NOISE, NARROW_NOISE] {
        let predecessor = noise_voice_latched(CgbAdsr::flat(), SLOW_NOISE_KEY, width);
        assert_eq!(predecessor.noise_output_latch(), Some(FULL_VOLUME));
        let mut mixer = crate::mixer::Mixer::default();
        assert!(mixer.add_cgb_voice(predecessor));
        assert!(mixer.add_cgb_voice(slow_noise_replacement(width)));
        assert_mixed(
            first_mixed_sample(&mut mixer),
            expected_mixed(FULL_VOLUME, FULL_VOLUME),
            "a replacement must start on the predecessor's high latch (width {width})",
        );
    }
}

/// The latch is the predecessor's volume-resolved level, not a polarity the
/// replacement's own volume scales (`mgba/src/gb/audio.c:641,782`).
#[test]
fn a_replacement_outputs_the_predecessors_resolved_level_until_its_first_clock() {
    let quiet = sustained_adsr(10);
    let quiet_predecessor = noise_voice_latched(quiet, SLOW_NOISE_KEY, WIDE_NOISE);
    let quiet_level = quiet_predecessor.noise_output_latch().unwrap();
    assert!(
        quiet_level > 0 && quiet_level < FULL_VOLUME,
        "sanity: predecessor latched below full volume, got {quiet_level}"
    );
    let loud_predecessor = noise_voice_latched(CgbAdsr::flat(), SLOW_NOISE_KEY, WIDE_NOISE);

    // Quieter predecessor, full-volume replacement.
    assert_eq!(
        first_sample_after(&quiet_predecessor, slow_noise_replacement(WIDE_NOISE)),
        expected_contribution(quiet_level, quiet_level),
        "the predecessor's level, not the replacement's volume"
    );
    assert_ne!(
        expected_contribution(quiet_level, quiet_level),
        expected_contribution(FULL_VOLUME, FULL_VOLUME),
        "sanity: the two levels render differently"
    );
    // Louder predecessor, quieter replacement.
    assert_eq!(
        first_sample_after(
            &loud_predecessor,
            noise_voice_at(quiet, SLOW_NOISE_KEY, WIDE_NOISE)
        ),
        expected_contribution(FULL_VOLUME, FULL_VOLUME),
    );
}

/// mGBA clocks `ch4.sample = lsb * currentVolume` (`gb/audio.c:641`): a
/// predecessor clocked at hardware volume zero leaves the latch at zero
/// whatever its LFSR polarity reads, and the trigger does not rewrite it.
#[test]
fn replacing_a_zero_volume_noise_voice_starts_low() {
    let adsr = CgbAdsr {
        attack: 4,
        ..CgbAdsr::flat()
    };
    let mut predecessor = noise_voice(adsr, WIDE_NOISE, TestNote::default());
    predecessor.begin_frame(false);
    let mut acc = [(0i32, 0i32); 1];
    for _ in 0..MAX_SAMPLES_TO_LATCH_HIGH {
        if predecessor.noise_lfsr() != Some(0) {
            break;
        }
        predecessor.render(&mut acc, &[]);
    }
    assert!(predecessor.is_active(), "sanity: attack voice still active");
    assert_eq!(
        predecessor.envelope_volume(),
        0,
        "sanity: clocked at volume zero"
    );
    assert_ne!(predecessor.noise_lfsr(), Some(0), "sanity: LFSR clocked");
    assert_eq!(predecessor.noise_output_latch(), Some(0));
    let mut mixer = crate::mixer::Mixer::default();
    assert!(mixer.add_cgb_voice(predecessor));
    assert!(mixer.add_cgb_voice(slow_noise_replacement(WIDE_NOISE)));
    assert_mixed(
        first_mixed_sample(&mut mixer),
        expected_mixed(0, 0),
        "a zero-volume predecessor's latch is zero",
    );
}

/// A direct volume write to zero (the off-write's `NR42`, or an NRx2 zombie
/// write) does not recompute `ch4.sample`; only a clock or envelope step does
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

/// An envelope step re-resolves a raised latch to the stepped volume
/// (`gb/audio.c:730-732`) but never raises a low one.
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

/// Upstream `CgbOscOff` writes `NR42 = 8; NR44 = 0x80` (`m4a.c:873-874`): a
/// zero-volume retrigger that resets the LFSR and phase but leaves `ch4.sample`
/// until the first clock after it (`gb/audio.c:606-641`). A slow key therefore
/// keeps its raised latch through the idle frames.
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
            expected_mixed(FULL_VOLUME, FULL_VOLUME),
            "the latch survives the off-write until a clock (width {width})",
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

/// A new note inside the period resets the phase again, so the latch is still
/// inherited by the note after it (`gb/audio.c:382`).
#[test]
fn a_second_off_write_restarts_the_phase_and_keeps_the_latch() {
    let mut mixer = crate::mixer::Mixer::default();
    assert!(mixer.add_cgb_voice(noise_voice_latched(
        CgbAdsr::flat(),
        SLOW_NOISE_KEY,
        WIDE_NOISE
    )));
    mixer.stop_track(0);
    for _ in 0..10 {
        first_mixed_sample(&mut mixer);
    }
    assert!(mixer.add_cgb_voice(slow_noise_replacement(WIDE_NOISE)));
    first_mixed_sample(&mut mixer);
    mixer.stop_track(0);
    for _ in 0..10 {
        assert!(first_mixed_sample(&mut mixer) > 0.0);
    }
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
            expected_mixed(FULL_VOLUME, FULL_VOLUME),
            "a released note's off-write leaves its latch for the first clock (width {width})",
        );
    }
}

/// A fast key's restarted LFSR clocks inside the idle frames, so the slot is
/// already settled low when the next note arrives.
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

/// A trigger with initial volume zero and a decreasing envelope disables
/// channel 4 (`gb/audio.c:371-372,856-860`): the LFSR stops clocking, so the
/// inherited latch stays held (and output, `gb/audio.c:782`) instead of
/// settling at a clock.
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

/// Track volumes whose centred goal is 16 (`CgbModVol`, `m4a.c:908-911`),
/// which `envelopeVolume << 4` wraps to a zero NR42 volume nibble
/// (`m4a.c:1222`).
const WRAPPING_GOAL_TRACK_VOLUME: u8 = 132;
const WRAPPING_GOAL: u8 = 16;

/// Renders `replacement`'s first frame on `predecessor`'s held latch for
/// twice the slow key's first-clock delay and returns the latch it ends on.
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

/// A decreasing store of `envelopeVolume` 16 writes NR42 `decay | 0x00`
/// (`m4a.c:1158,1222`): a zero volume nibble and clear direction bit, so
/// the DAC test fails (`gb/audio.c:346-352,856-860`) and the LFSR stops,
/// holding the inherited latch however large the unmasked volume is.
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

/// Attack 16 stores NR42 `(16 + 8) & 0xF = 0x08` at volume 0
/// (`m4a.c:1024,1034,1222`): no step time, but bit 3 keeps the DAC on
/// (`gb/audio.c:856-860`), so the LFSR clocks at volume zero and settles
/// the inherited latch low (`gb/audio.c:641`).
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

/// The rendered sample is the held latch's own: a replacement at a lower or
/// higher volume continues the predecessor's last sample until its first
/// clock moves `ch4.sample` (`gb/audio.c:782`).
#[test]
fn a_replacement_continues_the_predecessors_last_rendered_sample() {
    let mut predecessor = noise_voice_latched(sustained_adsr(10), SLOW_NOISE_KEY, WIDE_NOISE);
    let mut acc = [(0i32, 0i32); 1];
    predecessor.render(&mut acc, &[]);
    let last = acc[0].0;
    assert_ne!(last, 0, "sanity: a raised latch renders");
    for replacement_sustain in [3, 10, 15] {
        let replacement = noise_voice_at(
            sustained_adsr(replacement_sustain),
            SLOW_NOISE_KEY,
            WIDE_NOISE,
        );
        assert_eq!(
            first_sample_after(&predecessor, replacement),
            last,
            "replacement sustain {replacement_sustain}"
        );
    }
}

#[test]
fn an_inherited_settled_zero_latch_renders_the_same_across_a_replacement() {
    let mut predecessor = noise_voice_at(CgbAdsr::flat(), SLOW_NOISE_KEY, WIDE_NOISE);
    predecessor.begin_frame(false);
    let mut acc = [(0i32, 0i32); 1];
    // The first slow clock of a fresh LFSR is high; settle it via the off-write.
    for _ in 0..MAX_SAMPLES_TO_LATCH_HIGH {
        if predecessor
            .noise_output_latch()
            .is_some_and(|level| level > 0)
        {
            break;
        }
        predecessor.render(&mut acc, &[]);
    }
    predecessor.apply_hardware_off_write();
    for _ in 0..(2 * SLOW_KEY_FIRST_CLOCK_SAMPLE) {
        predecessor.render(&mut acc, &[]);
    }
    assert_eq!(predecessor.noise_output_latch(), Some(0), "sanity: settled");
    acc[0] = (0, 0);
    predecessor.render(&mut acc, &[]);
    let before = acc[0].0;
    for sustain in [4, 15] {
        let replacement = noise_voice_at(sustained_adsr(sustain), SLOW_NOISE_KEY, WIDE_NOISE);
        assert_eq!(first_sample_after(&predecessor, replacement), before);
    }
}

/// A slow off-write tail is still sounding: the mixer is not idle until the
/// first zero-volume clock settles the latch, so a fade-end `stop_track`
/// does not truncate it.
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

/// A latch a real clock left at zero (at a nonzero volume) is already what
/// the off-write's first zero-volume clock would write, so retiring from it
/// leaves no settling edge and no pending tail (`gb/audio.c:371-383,641`).
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
