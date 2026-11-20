use super::fixture::*;
use super::*;

/// Hold a square voice's envelope at `expected_level` (the ADSR's zero
/// attack and non-zero decay latch `volume` to the goal on entering decay,
/// before any decay step consumes it) and return its peak render sample.
fn square_peak_at_level(track_volume: u8, expected_level: u8) -> i32 {
    let held_at_goal = CgbAdsr {
        attack: 0,
        decay: 8,
        sustain: 15,
        release: 0,
    };
    let note = TestNote {
        track_right: track_volume,
        track_left: track_volume,
        ..TestNote::default()
    };
    let mut voice = square_voice_with_adsr(CgbChannelNumber::Square1, None, held_at_goal, note);
    voice.begin_frame(false);
    assert_eq!(
        voice.envelope_volume(),
        expected_level,
        "test setup must hold the envelope at the level under test"
    );
    let mut acc = vec![(0i32, 0i32); 64];
    voice.render(&mut acc, &[]);
    acc.iter()
        .map(|&(left, right)| left.abs().max(right.abs()))
        .max()
        .unwrap_or(0)
}

#[test]
fn square_envelope_levels_above_fifteen_render_through_the_nrx2_low_nibble() {
    let level_14_peak = square_peak_at_level(121, 14);
    let level_15_peak = square_peak_at_level(130, 15);

    // One hardware volume unit swings +-2 DirectSound accumulator units
    // (`PSG_LEVEL_FIFO_UNITS`'s derivation): +-28 and +-30.
    assert_eq!(level_14_peak, 28);
    assert_eq!(level_15_peak, 30);
    assert_eq!(square_peak_at_level(131, 16), 0);
    assert_eq!(square_peak_at_level(243, 30), level_14_peak);
    assert_eq!(square_peak_at_level(255, 31), level_15_peak);
}

fn centred_goal_thirty_one_note() -> TestNote {
    TestNote {
        track_right: u8::MAX,
        track_left: u8::MAX,
        ..TestNote::default()
    }
}

/// The first phase-7 envelope clock of the shared sequencer's default epoch.
const FIRST_ENVELOPE_EVENT: usize = 26;

/// The frame's samples, each as the larger of its two channel magnitudes.
fn magnitudes(acc: &[StereoAcc]) -> Vec<i32> {
    acc.iter()
        .map(|&(left, right)| left.abs().max(right.abs()))
        .collect()
}

/// Hardware attack 17 decodes to an upward period of one event.
const CLIMBING_EVERY_EVENT: CgbAdsr = CgbAdsr {
    attack: 17,
    decay: 0,
    sustain: 15,
    release: 0,
};

/// Peak render sample of each of `frames` consecutive frames of a square
/// voice, counting from its note-on. Square gain is the hardware nibble
/// alone (`Oscillator::envelope_gain_256`), so the series reads the
/// hardware envelope out directly.
fn square_peaks_from_note_on(adsr: CgbAdsr, frames: usize) -> Vec<i32> {
    let mut clock = SharedClock::new();
    let mut voice = square_voice_with_adsr(
        CgbChannelNumber::Square1,
        None,
        adsr,
        centred_goal_thirty_one_note(),
    );
    (0..frames)
        .map(|_| peak_magnitude(&clock.frame(&mut voice, false)))
        .collect()
}

#[test]
fn note_on_arms_the_hardware_timer_from_the_decoded_nrx2_period() {
    // The note-on write programs NRx2 with the attack's decoded nibble, so a
    // fresh voice's timer starts from the decoded period, not the raw attack
    // byte.
    //
    // Attack 17 writes `(17 + CGB_NRx2_ENV_DIR_INC) & 0xF == 9`: period
    // 1, upward. One level lands per phase-7 event: the events at 26 of the
    // first frame, 11 and 220 of the second, and 205 of the third.
    assert_eq!(
        square_peaks_from_note_on(CLIMBING_EVERY_EVENT, 4),
        [2, 6, 8, 10],
        "a decoded period of 1 climbs a nibble per event, two units a nibble"
    );
}

#[test]
fn a_hardware_step_takes_effect_at_its_event_inside_the_render_frame() {
    for channel in [CgbChannelNumber::Square1, CgbChannelNumber::Square2] {
        let mut clock = SharedClock::new();
        let mut voice = square_voice_with_adsr(
            channel,
            None,
            CLIMBING_EVERY_EVENT,
            centred_goal_thirty_one_note(),
        );
        let heard = magnitudes(&clock.frame(&mut voice, false));

        assert_eq!(voice.hardware_volume(), 1, "{channel:?}");
        assert!(
            heard[..FIRST_ENVELOPE_EVENT].iter().all(|&m| m == 0),
            "{channel:?}: silent before the event"
        );
        assert!(
            heard[FIRST_ENVELOPE_EVENT..].iter().all(|&m| m == 2),
            "{channel:?}: one level from the event's own sample on"
        );
    }
}

#[test]
fn a_note_on_write_leaves_the_hardware_unstepped_until_the_first_event() {
    // Upstream stores NRx2 once per `CgbSound` pass, after the doubling
    // re-entry (`m4a.c:1176-1226`), so a doubled and an ordinary note-on pass
    // both leave the nibble where the store latched it and the sequencer's
    // next event takes the first step.
    for extra_iteration in [false, true] {
        let mut clock = SharedClock::new();
        let mut voice = square_voice_with_adsr(
            CgbChannelNumber::Square1,
            None,
            CLIMBING_EVERY_EVENT,
            centred_goal_thirty_one_note(),
        );
        let heard = magnitudes(&clock.frame(&mut voice, extra_iteration));
        assert!(
            heard[..FIRST_ENVELOPE_EVENT].iter().all(|&m| m == 0),
            "doubled: {extra_iteration}"
        );
        assert_eq!(heard[FIRST_ENVELOPE_EVENT], 2, "doubled: {extra_iteration}");
    }
}

#[test]
fn a_downward_note_on_write_at_level_zero_leaves_hardware_dead() {
    // The note-on timer takes the same saturation check as any NRx2 write.
    // Attack 9 writes
    // `(9 + CGB_NRx2_ENV_DIR_INC) & 0xF == 1`: period 1, downward, and
    // the note-on level of zero is already saturated in that direction,
    // so the write leaves hardware dead rather than stepping the nibble
    // below zero.
    let dead = square_peaks_from_note_on(
        CgbAdsr {
            attack: 9,
            decay: 0,
            sustain: 15,
            release: 0,
        },
        12,
    );
    assert_eq!(
        dead,
        vec![0; 12],
        "a downward note-on write at level zero is dead on arrival"
    );
}

#[test]
fn centred_goal_thirty_one_decay_falls_with_the_hardware_envelope_between_writes() {
    let level_15_peak = square_peak_at_level(130, 15);
    let level_14_peak = square_peak_at_level(121, 14);

    let paced_decay = CgbAdsr {
        attack: 0,
        decay: 1,
        sustain: 0,
        release: 0,
    };
    let mut clock = SharedClock::new();
    let mut voice = square_voice_with_adsr(
        CgbChannelNumber::Square1,
        None,
        paced_decay,
        centred_goal_thirty_one_note(),
    );

    let heard = magnitudes(&clock.frame(&mut voice, false));
    assert_eq!(
        voice.envelope_volume(),
        31,
        "the software envelope does not step inside a render frame"
    );
    assert_eq!(voice.hardware_volume(), 14);
    assert!(
        heard[..FIRST_ENVELOPE_EVENT]
            .iter()
            .all(|&m| m == level_15_peak),
        "the write latches the nibble of the raw goal until the event"
    );
    assert!(
        heard[FIRST_ENVELOPE_EVENT..]
            .iter()
            .all(|&m| m == level_14_peak),
        "hardware's own envelope timer is one step quieter from the event on"
    );
}

#[test]
fn live_volume_write_mid_decay_restarts_the_hardware_envelope_timer() {
    // A live volume write retriggers NRx4 (`m4a.c:1223`), which reloads the
    // hardware envelope timer (`mgba/src/gb/audio.c:856-859`) while the
    // software counter keeps its phase and the sequencer its own.
    let level_15_peak = square_peak_at_level(130, 15);
    let level_14_peak = square_peak_at_level(121, 14);
    assert_ne!(
        level_15_peak, level_14_peak,
        "sanity: the two levels differ"
    );

    // Hardware period 7: from the note-on write, the seventh event is the one
    // at sample 1280, frame 5 offset 160; software holds its level through
    // frame 6.
    let paced_decay = CgbAdsr {
        attack: 0,
        decay: 7,
        sustain: 0,
        release: 0,
    };
    let goal_fifteen_note = TestNote {
        track_right: 130,
        track_left: 130,
        ..TestNote::default()
    };
    let mut written_clock = SharedClock::new();
    let mut control_clock = SharedClock::new();
    let mut written = square_voice_with_adsr(
        CgbChannelNumber::Square1,
        None,
        paced_decay,
        goal_fifteen_note,
    );
    let mut control = written.clone();
    let mut frames = Vec::new();
    for frame in 0..10 {
        if frame == 3 {
            written.set_track_volume(130, 130);
        }
        frames.push((
            magnitudes(&written_clock.frame(&mut written, false)),
            magnitudes(&control_clock.frame(&mut control, false)),
        ));
    }

    let (written_frame_5, control_frame_5) = &frames[5];
    assert!(
        control_frame_5[..160].iter().all(|&m| m == level_15_peak)
            && control_frame_5[160..].iter().all(|&m| m == level_14_peak),
        "the untouched voice steps at the seventh event after its note-on write"
    );
    assert!(
        written_frame_5.iter().all(|&m| m == level_15_peak),
        "the live write at frame 3 restarted the countdown, so the sequencer's \
         seventh event has not yet counted out"
    );
    // The restarted countdown ends at sample 2116, frame 9 offset 100.
    let written_frame_9 = &frames[9].0;
    assert!(
        written_frame_9[..100].iter().all(|&m| m == level_15_peak)
            && written_frame_9[100..].iter().all(|&m| m == level_14_peak),
        "the restarted countdown steps at the seventh event after the write"
    );
}

#[test]
fn centred_goal_thirty_one_attack_climbs_with_the_hardware_envelope_then_freezes_at_fifteen() {
    let paced_attack = CgbAdsr {
        attack: 1,
        decay: 1,
        sustain: 15,
        release: 0,
    };
    let mut clock = SharedClock::new();
    let mut voice = square_voice_with_adsr(
        CgbChannelNumber::Square1,
        None,
        paced_attack,
        centred_goal_thirty_one_note(),
    );

    let level_15_peak = square_peak_at_level(130, 15);
    let mut previous_peak = -1i32;
    let mut frames_pinned_above_fifteen = 0;
    for _ in 0..40 {
        let peak = peak_magnitude(&clock.frame(&mut voice, false));
        let level = voice.envelope_volume();
        assert!(
            peak >= previous_peak,
            "loudness must not fall while the hardware climbs"
        );
        if level > 15 && previous_peak == level_15_peak {
            assert_eq!(
                peak, level_15_peak,
                "hardware must stay pinned at level 15's loudness once the level exceeds it"
            );
            frames_pinned_above_fifteen += 1;
        }
        previous_peak = peak;
        if level >= 31 {
            break;
        }
    }
    assert_eq!(previous_peak, level_15_peak);
    assert!(
        frames_pinned_above_fifteen > 0,
        "test setup must run the software attack past level 15"
    );
}

#[test]
fn centred_goal_thirty_one_decay_freezes_at_zero_before_software_reaches_it() {
    let paced_decay_to_silence = CgbAdsr {
        attack: 0,
        decay: 1,
        sustain: 0,
        release: 0,
    };
    let mut clock = SharedClock::new();
    let mut voice = square_voice_with_adsr(
        CgbChannelNumber::Square1,
        None,
        paced_decay_to_silence,
        centred_goal_thirty_one_note(),
    );

    let mut saw_silence_while_still_in_range = false;
    for _ in 0..40 {
        let peak = peak_magnitude(&clock.frame(&mut voice, false));
        let level = voice.envelope_volume();
        if level > 0 && level <= 15 && peak == 0 {
            saw_silence_while_still_in_range = true;
        }
        if level == 0 {
            assert_eq!(peak, 0, "hardware must be silent once it has bottomed out");
            break;
        }
    }
    assert!(
        saw_silence_while_still_in_range,
        "hardware must freeze at silence before the software level itself reaches zero"
    );
}

#[test]
fn live_track_volume_drop_during_sustain_does_not_silence_the_channel() {
    // A sustain refresh never writes hardware (`m4a.c:1128-1137`), so its
    // unwritten snap to a lower goal must not read as envelope motion.
    let held_at_full_sustain = CgbAdsr {
        attack: 0,
        decay: 0,
        sustain: MAX_ADSR_LEVEL,
        release: 0,
    };
    let mut voice = square_voice_with_adsr(
        CgbChannelNumber::Square1,
        None,
        held_at_full_sustain,
        centred_goal_thirty_one_note(),
    );
    voice.begin_frame(false); // enters sustain at its write frame

    let mut sanity_acc = vec![(0i32, 0i32); 8];
    voice.render(&mut sanity_acc, &[]);
    assert!(
        sanity_acc.iter().any(|&(l, r)| l != 0 || r != 0),
        "sanity: the note starts audible in sustain"
    );

    let near_silent_track_volume = 1u8;
    voice.set_track_volume(near_silent_track_volume, near_silent_track_volume);

    for frame in 0..8 {
        voice.begin_frame(false);
        let mut acc = vec![(0i32, 0i32); 8];
        voice.render(&mut acc, &[]);
        assert!(
            acc.iter().any(|&(l, r)| l != 0 || r != 0),
            "frame {frame}: hardware holds its last explicit write through the unwritten \
             sustain refresh; it must not go silent before another real trigger"
        );
    }
}

#[test]
fn square_envelope_gain_at_the_nibble_ceiling_matches_the_wave_arms_full_gain() {
    // This is the internal Q8 envelope gain, not rendered amplitude: the
    // rendered PSG swing is scaled to DirectSound units after it
    // (`BIPOLAR_SAMPLE_SCALE`), so a loud PSG channel stays far below the s8 clip.
    // `envelope_gain_256`'s square/noise arm must reach a ceiling
    // comparable to the wave arm's full-scale gain (`cgb3_wave_gain_256`)
    // once both read the same 0..=15 nibble domain. The bound
    // allows one hardware step (`LINEAR_ENVELOPE_SCALE`, the finest step
    // this linear law can represent) short of that ceiling.
    let square = Oscillator::Square(SquareChannel::new(HALF_DUTY, 0, None));
    let square_ceiling = square.envelope_gain_256(15, 15);
    assert!(
        square_ceiling >= FULL_GAIN_256 - LINEAR_ENVELOPE_SCALE,
        "the loudest square/noise nibble ({square_ceiling}/256) must land within one \
         hardware step of the wave arm's full-scale gain ({FULL_GAIN_256}/256)"
    );
}
