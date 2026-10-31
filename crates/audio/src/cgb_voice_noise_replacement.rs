//! Replacing a sounding noise voice: the predecessor catch-up latch, resolved level, and cancelled swaps.

use super::*;

/// The replacement's note-on writes (`m4a.c:1020-1022,1197-1202`) clock the
/// predecessor at the width-only rate before the trigger
/// (`mgba/src/gb/audio.c:354-362,371-383,602-644`), so the latch it starts on
/// is the one that catch-up leaves, not the predecessor's last resolved level.
#[test]
fn replacing_a_sounding_noise_voice_inherits_the_latch_its_width_only_write_leaves() {
    for width in [WIDE_NOISE, NARROW_NOISE] {
        let predecessor = noise_voice_latched(CgbAdsr::flat(), SLOW_NOISE_KEY, width);
        assert_eq!(predecessor.noise_output_latch(), Some(FULL_VOLUME));
        let expected = reference_catch_up(&PredecessorRun::latched(width), width);
        let mut mixer = crate::mixer::Mixer::default();
        assert!(mixer.add_cgb_voice(predecessor));
        assert!(mixer.add_cgb_voice(slow_noise_replacement(width)));
        assert_mixed(
            first_mixed_sample(&mut mixer),
            expected_mixed(expected, FULL_VOLUME),
            "a replacement starts on the latch the fast catch-up leaves (width {width})",
        );
    }
}

/// Reference model of a predecessor that has run `samples` from its trigger,
/// then takes the width-only and pitched `NR43` writes of a replacement.
struct PredecessorRun {
    key: u8,
    width: u8,
    samples: usize,
}

impl PredecessorRun {
    /// The run `noise_voice_latched` stops at: the first sample whose clock raises the latch.
    fn latched(width: u8) -> Self {
        Self {
            key: SLOW_NOISE_KEY,
            width,
            samples: usize::try_from(slow_period(SLOW_NOISE_KEY).div_ceil(BASE_TICKS_PER_SAMPLE))
                .unwrap(),
        }
    }
}

const BASE_TICKS_PER_SAMPLE: u64 = 524_288;

fn slow_period(key: u8) -> u64 {
    let control = midi_key_to_noise_control(key);
    let divisor = u64::from(control & 7);
    let ticks = (if divisor == 0 { 1 } else { 2 * divisor }) << (control >> 4);
    ticks * u64::from(crate::pitch::MIXER_RATE)
}

fn lfsr_clock(lfsr: u16, narrow: bool) -> (u16, bool) {
    let feedback_high = (lfsr ^ (lfsr >> 1)) & 1 == 0;
    let bits = 0x4000 | if narrow { 0x40 } else { 0 };
    let next = if feedback_high {
        (lfsr >> 1) | bits
    } else {
        (lfsr >> 1) & !bits
    };
    (next, feedback_high)
}

/// Independent re-derivation of the latch left after the predecessor's own
/// clocks and the fast catch-up under the replacement's width.
fn reference_catch_up(run: &PredecessorRun, new_width: u8) -> u8 {
    let slow_period = slow_period(run.key);
    let (mut lfsr, mut high, mut elapsed) = (0u16, false, 0u64);
    for _ in 0..run.samples {
        elapsed += BASE_TICKS_PER_SAMPLE;
        while elapsed >= slow_period {
            elapsed -= slow_period;
            (lfsr, high) = lfsr_clock(lfsr, run.width == NARROW_NOISE);
        }
    }
    let fast_period = u64::from(crate::pitch::MIXER_RATE);
    for _ in 0..elapsed / fast_period {
        (lfsr, high) = lfsr_clock(lfsr, new_width == NARROW_NOISE);
    }
    if high {
        FULL_VOLUME
    } else {
        0
    }
}

/// `mgba/src/gb/audio.c:641,782`: the latch is the predecessor's resolved level.
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

/// `gb/audio.c:782`.
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

fn first_frame_predecessor(width: u8) -> CgbVoice {
    let mut predecessor = noise_voice(CgbAdsr::flat(), width, TestNote::at_key(SLOW_NOISE_KEY));
    predecessor.begin_frame(false);
    let mut frame = [(0i32, 0i32); crate::SAMPLES_PER_FRAME];
    predecessor.render(&mut frame, &[]);
    predecessor
}

fn latch_after_note_on(predecessor: &CgbVoice, mut replacement: CgbVoice) -> Option<u8> {
    replacement.carry_hardware_state_from(predecessor);
    replacement.begin_frame(false);
    replacement.noise_output_latch()
}

/// After one frame a slow predecessor has made no clock of its own, yet the
/// width-only `NR43` write (`m4a.c:1020-1022`) makes the pitched write
/// (`m4a.c:1197-1202`) clock the whole elapsed frame at the fastest rate and
/// the predecessor's volume (`mgba/src/gb/audio.c:354-362,602-644`).
#[test]
fn a_slow_same_key_replacement_inherits_the_latch_the_fast_catch_up_leaves() {
    let predecessor = first_frame_predecessor(WIDE_NOISE);
    assert_eq!(
        predecessor.noise_output_latch(),
        Some(0),
        "no own clock yet"
    );
    assert_eq!(predecessor.noise_lfsr(), Some(0));
    let expected = reference_catch_up(
        &PredecessorRun {
            key: SLOW_NOISE_KEY,
            width: WIDE_NOISE,
            samples: crate::SAMPLES_PER_FRAME,
        },
        WIDE_NOISE,
    );
    assert_eq!(
        expected, FULL_VOLUME,
        "sanity: the catch-up raises the latch"
    );
    let replacement = noise_voice_at(CgbAdsr::flat(), SLOW_NOISE_KEY, WIDE_NOISE);
    assert_eq!(
        latch_after_note_on(&predecessor, replacement),
        Some(expected)
    );
}

/// The catch-up runs under the replacement's width, which the width-only write
/// has already installed (`mgba/src/gb/audio.c:359-363,630-644`).
#[test]
fn a_changed_width_replacement_catches_up_under_its_own_width() {
    let mut outcomes = Vec::new();
    for new_width in [WIDE_NOISE, NARROW_NOISE] {
        let predecessor = noise_voice_latched(CgbAdsr::flat(), SLOW_NOISE_KEY, WIDE_NOISE);
        let expected = reference_catch_up(&PredecessorRun::latched(WIDE_NOISE), new_width);
        let replacement = noise_voice_at(CgbAdsr::flat(), SLOW_NOISE_KEY, new_width);
        assert_eq!(
            latch_after_note_on(&predecessor, replacement),
            Some(expected),
            "replacement width {new_width}"
        );
        outcomes.push(expected);
    }
    assert_ne!(
        outcomes[0], outcomes[1],
        "sanity: the width decides the latch"
    );
}

/// A replacement that stops before its first pass writes nothing
/// (`m4a.c:1043-1056`), so the predecessor's state is not clocked: the slot
/// keeps the low latch the predecessor had, not the raised one the catch-up
/// would leave.
#[test]
fn a_cancelled_replacement_leaves_the_predecessors_hardware_unclocked() {
    for width in [WIDE_NOISE, NARROW_NOISE] {
        let mut mixer = crate::mixer::Mixer::default();
        assert!(mixer.add_cgb_voice(first_frame_predecessor(width)));
        assert!(mixer.add_cgb_voice(slow_noise_replacement(width)));
        mixer.stop_track(0);
        assert!(mixer.add_cgb_voice(slow_noise_replacement(width)));
        assert_mixed(
            first_mixed_sample(&mut mixer),
            expected_mixed(0, 0),
            "cancelled replacement width {width}",
        );
    }
}
