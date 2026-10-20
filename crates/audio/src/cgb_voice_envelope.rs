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

    assert!(level_14_peak > 0);
    assert!(level_15_peak > 0);
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

/// Peak render sample of each of `frames` consecutive frames of a square
/// voice, counting from its note-on. Square gain is the hardware nibble
/// alone (`Oscillator::envelope_gain_256`), so the series reads the
/// hardware envelope out directly.
fn square_peaks_from_note_on(adsr: CgbAdsr, frames: usize) -> Vec<i32> {
    let mut voice = square_voice_with_adsr(
        CgbChannelNumber::Square1,
        None,
        adsr,
        centred_goal_thirty_one_note(),
    );
    (0..frames)
        .map(|_| {
            voice.begin_frame(false);
            let mut acc = vec![(0i32, 0i32); 64];
            voice.render(&mut acc, &[]);
            acc.iter()
                .map(|&(left, right)| left.abs().max(right.abs()))
                .max()
                .unwrap_or(0)
        })
        .collect()
}

#[test]
fn note_on_arms_the_hardware_timer_from_the_decoded_nrx2_period() {
    // The note-on write programs NRx2 with the attack's decoded nibble, so a
    // fresh voice's timer starts from the decoded period, not the raw attack
    // byte.
    //
    // Attack 17 writes `(17 + CGB_NRx2_ENV_DIR_INC) & 0xF == 9`: period
    // 1, upward. The nibble leaves zero on the second frame, not on the
    // raw byte's eighteenth.
    let climbing = square_peaks_from_note_on(
        CgbAdsr {
            attack: 17,
            decay: 0,
            sustain: 15,
            release: 0,
        },
        4,
    );
    assert_eq!(
        climbing[0], 0,
        "the note-on write still owes one iteration on its own frame"
    );
    assert!(
        climbing[1] > 0 && climbing[2] > climbing[1] && climbing[3] > climbing[2],
        "a decoded period of 1 climbs a nibble per frame, got {climbing:?}"
    );
}

#[test]
fn a_doubled_note_on_frame_still_owes_its_write_until_the_frame_ends() {
    // Upstream stores NRx2 once per `CgbSound` pass, after the doubling
    // re-entry (`m4a.c:1176-1226`), so the hardware timer stays unarmed
    // through the whole note-on frame.
    let adsr = CgbAdsr {
        attack: 17,
        decay: 0,
        sustain: 15,
        release: 0,
    };
    let mut voice = square_voice_with_adsr(
        CgbChannelNumber::Square1,
        None,
        adsr,
        centred_goal_thirty_one_note(),
    );
    let peak_of = |voice: &mut CgbVoice, extra_envelope_iteration: bool| {
        voice.begin_frame(extra_envelope_iteration);
        let mut acc = vec![(0i32, 0i32); 64];
        voice.render(&mut acc, &[]);
        acc.iter()
            .map(|&(left, right)| left.abs().max(right.abs()))
            .max()
            .unwrap_or(0)
    };

    let note_on_frame = peak_of(&mut voice, true);
    let next_frame = peak_of(&mut voice, false);

    assert_eq!(
        note_on_frame, 0,
        "a doubled note-on frame owes its write until the pass ends, so the \
         hardware nibble must not step during it"
    );
    assert!(
        next_frame > 0,
        "the frame after the note-on write must show the first hardware step, got {next_frame}"
    );
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
    // The frame right after a write is already one hardware step quieter,
    // not still holding the write frame's loudness.
    let level_15_peak = square_peak_at_level(130, 15);
    let level_14_peak = square_peak_at_level(121, 14);

    let paced_decay = CgbAdsr {
        attack: 0,
        decay: 1,
        sustain: 0,
        release: 0,
    };
    let mut voice = square_voice_with_adsr(
        CgbChannelNumber::Square1,
        None,
        paced_decay,
        centred_goal_thirty_one_note(),
    );

    voice.begin_frame(false);
    assert_eq!(
        voice.envelope_volume(),
        31,
        "write frame must hold the raw goal"
    );
    let mut write_frame_acc = vec![(0i32, 0i32); 64];
    voice.render(&mut write_frame_acc, &[]);
    let write_frame_peak = write_frame_acc
        .iter()
        .map(|&(left, right)| left.abs().max(right.abs()))
        .max()
        .unwrap_or(0);
    assert_eq!(write_frame_peak, level_15_peak);

    voice.begin_frame(false);
    assert_eq!(
        voice.envelope_volume(),
        30,
        "one paced decay step without a write must have run"
    );
    let mut next_frame_acc = vec![(0i32, 0i32); 64];
    voice.render(&mut next_frame_acc, &[]);
    let next_frame_peak = next_frame_acc
        .iter()
        .map(|&(left, right)| left.abs().max(right.abs()))
        .max()
        .unwrap_or(0);
    assert_eq!(
        next_frame_peak, level_14_peak,
        "hardware's own envelope timer must already be one step quieter"
    );
}

/// Render one frame and return its peak sample magnitude.
fn frame_peak(voice: &mut CgbVoice) -> i32 {
    voice.begin_frame(false);
    let mut acc = vec![(0i32, 0i32); 64];
    voice.render(&mut acc, &[]);
    acc.iter()
        .map(|&(left, right)| left.abs().max(right.abs()))
        .max()
        .unwrap_or(0)
}

#[test]
fn live_volume_write_mid_decay_restarts_the_hardware_envelope_timer() {
    // A live volume write retriggers NRx4 (`m4a.c:1222`), which reloads the
    // hardware envelope timer (`mgba/src/gb/audio.c:856-859`) while the
    // software counter keeps its phase.
    let level_15_peak = square_peak_at_level(130, 15);
    let level_14_peak = square_peak_at_level(121, 14);
    assert_ne!(
        level_15_peak, level_14_peak,
        "sanity: the two levels differ"
    );

    let paced_decay = CgbAdsr {
        attack: 0,
        decay: 4,
        sustain: 0,
        release: 0,
    };
    let goal_fifteen_note = TestNote {
        track_right: 130,
        track_left: 130,
        ..TestNote::default()
    };
    let mut voice = square_voice_with_adsr(
        CgbChannelNumber::Square1,
        None,
        paced_decay,
        goal_fifteen_note,
    );

    assert_eq!(
        frame_peak(&mut voice),
        level_15_peak,
        "sanity: the attack-to-decay write latches level 15"
    );
    for frame in 0..2 {
        assert_eq!(
            frame_peak(&mut voice),
            level_15_peak,
            "sanity: frame {frame} is still inside the first decay period"
        );
    }

    // Two iterations short of the software counter's next step.
    voice.set_track_volume(130, 130);
    assert_eq!(
        frame_peak(&mut voice),
        level_15_peak,
        "sanity: the live volume write relatches the same level 15"
    );

    for frame in 0..(paced_decay.decay - 1) {
        assert_eq!(
            frame_peak(&mut voice),
            level_15_peak,
            "frame {frame} after the live volume write: the trigger restarted the hardware \
             envelope timer, so a full step time must elapse before the next hardware step"
        );
    }
    assert_eq!(
        frame_peak(&mut voice),
        level_14_peak,
        "one full step time after the trigger, the hardware envelope steps once"
    );
}

#[test]
fn centred_goal_thirty_one_attack_climbs_with_the_hardware_envelope_then_freezes_at_fifteen() {
    // The hardware envelope climbs while the level is in range and stays at
    // level 15's loudness once the level exceeds it.
    let paced_attack = CgbAdsr {
        attack: 1,
        decay: 1,
        sustain: 15,
        release: 0,
    };
    let mut voice = square_voice_with_adsr(
        CgbChannelNumber::Square1,
        None,
        paced_attack,
        centred_goal_thirty_one_note(),
    );

    let mut previous_peak = -1i32;
    let mut peak_at_fifteen = None;
    for _ in 0..40 {
        voice.begin_frame(false);
        let level = voice.envelope_volume();
        let mut acc = vec![(0i32, 0i32); 64];
        voice.render(&mut acc, &[]);
        let peak = acc
            .iter()
            .map(|&(left, right)| left.abs().max(right.abs()))
            .max()
            .unwrap_or(0);
        if level <= 15 {
            assert!(
                peak >= previous_peak,
                "loudness must not fall while the level is climbing in range"
            );
            if level == 15 {
                peak_at_fifteen = Some(peak);
            }
        } else {
            assert_eq!(
                Some(peak),
                peak_at_fifteen,
                "hardware must stay pinned at level 15's loudness once the level exceeds it"
            );
        }
        previous_peak = peak;
        if level >= 31 {
            break;
        }
    }
    assert!(
        peak_at_fifteen.is_some(),
        "test setup must run the attack far enough to reach level 15"
    );
}

#[test]
fn centred_goal_thirty_one_decay_freezes_at_zero_before_software_reaches_it() {
    // Hardware goes silent well before the software envelope itself reaches
    // zero.
    let paced_decay_to_silence = CgbAdsr {
        attack: 0,
        decay: 1,
        sustain: 0,
        release: 0,
    };
    let mut voice = square_voice_with_adsr(
        CgbChannelNumber::Square1,
        None,
        paced_decay_to_silence,
        centred_goal_thirty_one_note(),
    );

    let mut saw_silence_while_still_in_range = false;
    for _ in 0..40 {
        voice.begin_frame(false);
        let level = voice.envelope_volume();
        let mut acc = vec![(0i32, 0i32); 64];
        voice.render(&mut acc, &[]);
        let peak = acc
            .iter()
            .map(|&(left, right)| left.abs().max(right.abs()))
            .max()
            .unwrap_or(0);
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
    // Sustain's periodic refresh (`CgbEnvelope::sustain_step`, every
    // `SUSTAIN_REFRESH_FRAMES`) only updates software bookkeeping and
    // never sets a hardware write -- upstream's own sustain-refresh
    // branch never raises `CGB_CHANNEL_MO_VOL` (`m4a.c:1128-1137`).
    // `set_track_volume` itself retriggers immediately, but that write
    // lands before the software goal has caught up (still the stale
    // value), so it changes nothing audible; the *later*, unwritten
    // refresh is the one that actually snaps to the new low goal.
    // `HardwareEnvelopeVolume::track` must not read that unwritten snap
    // as gradual hardware envelope motion and silence the channel.
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
fn square_gain_at_the_nibble_ceiling_reaches_near_full_scale_like_the_wave_arm() {
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
