use super::fixture::*;
use super::*;
use crate::psg::FrameSequencer128Hz;

/// A hardware-muted square keeps its duty position running, so the note
/// that later replaces it continues from where hardware would be
/// ([`SquareChannel::advance_silently`]'s doc).
#[test]
fn a_hardware_muted_square_keeps_its_duty_position_running() {
    const FRAME_SAMPLES: usize = 37;
    let mut muted = square_voice(
        CgbChannelNumber::Square1,
        Some(upward_sweep(3, 1)),
        TestNote::at_key(120),
    );
    let mut free_running = muted.square_oscillator().expect("a square voice").clone();
    muted.begin_frame(false);
    let mut acc = vec![(0i32, 0i32); FRAME_SAMPLES];
    muted.render(&mut acc, &[]);
    assert!(
        acc.iter().all(|&(l, r)| l == 0 && r == 0),
        "the overflow must have muted the channel",
    );
    for _ in 0..FRAME_SAMPLES {
        free_running.sample();
    }
    assert_eq!(
        muted
            .square_oscillator()
            .expect("a square voice")
            .duty_phase(),
        free_running.duty_phase(),
        "a muted frame must advance the duty position as a sounding one does",
    );
}

#[test]
fn square1_upward_sweep_overflow_is_born_muted_and_revives_on_a_safe_trigger() {
    // A note-on overflow clears the hardware channel-enable bit and nothing
    // else: upstream leaves SOUND_CHANNEL_SF_ON set and keeps running the
    // envelope, so the next volume write can bring the note back
    // (`m4a.c:1055-1058`, `mgba/src/gb/audio.c:180-186`).
    let high_key = 120;
    let safe_key = 48;
    for sweep_period in [0, 3] {
        let sweep = upward_sweep(sweep_period, 1);
        let mut muted = square_voice(
            CgbChannelNumber::Square1,
            Some(sweep),
            TestNote::at_key(high_key),
        );
        assert!(
            muted.is_active(),
            "the overflow mutes the hardware channel; the software voice keeps its slot"
        );
        let mut acc = vec![(0i32, 0i32); 8];
        muted.begin_frame(false);
        muted.render(&mut acc, &[]);
        assert!(
            acc.iter().all(|&(l, r)| l == 0 && r == 0),
            "a born-muted channel must be silent from frame 0"
        );

        muted.set_track_pitch(i32::from(safe_key) - i32::from(high_key), 0);
        muted.set_track_volume(FULL_TRACK_VOLUME, FULL_TRACK_VOLUME);
        muted.begin_frame(false);
        let mut revived = vec![(0i32, 0i32); 8];
        muted.render(&mut revived, &[]);
        assert!(
            revived.iter().any(|&(l, r)| l != 0 || r != 0),
            "a later safe trigger must revive the born-muted note"
        );
    }

    let mut normal = square_voice(
        CgbChannelNumber::Square1,
        Some(upward_sweep(0, 1)),
        TestNote::at_key(safe_key),
    );
    let mut acc = vec![(0i32, 0i32); 8];
    normal.begin_frame(false);
    normal.render(&mut acc, &[]);
    assert!(
        acc.iter().any(|&(l, r)| l != 0 || r != 0),
        "a normal-frequency sweep note is audible from frame 0"
    );
}

#[test]
#[should_panic(expected = "square voice requires Square1 or Square2")]
fn square_voice_rejects_a_wave_channel() {
    let _ = square_voice(CgbChannelNumber::Wave, None, TestNote::default());
}

#[test]
#[should_panic(expected = "square voice requires Square1 or Square2")]
fn square_voice_rejects_a_noise_channel() {
    let _ = square_voice(CgbChannelNumber::Noise, None, TestNote::default());
}

#[test]
fn a_square2_voice_never_carries_a_sweep() {
    let sweeping = square_voice(
        CgbChannelNumber::Square1,
        Some(upward_sweep(1, 1)),
        TestNote::at_key(0),
    );
    assert!(
        sweeping.sweep_frequency().is_some(),
        "sanity: channel 1 does take the sweep byte"
    );

    let square2 = square_voice(
        CgbChannelNumber::Square2,
        Some(upward_sweep(1, 1)),
        TestNote::at_key(0),
    );
    assert_eq!(
        square2.sweep_frequency(),
        None,
        "a Square2 voice must ignore a channel-1 sweep byte"
    );
}

fn low_freq_sweep_voice(sweep_byte: u8) -> CgbVoice {
    square_voice(
        CgbChannelNumber::Square1,
        Some(sweep_byte),
        TestNote::at_key(0),
    )
}

#[test]
fn idle_duty_retunes_at_each_sweep_tick_then_freezes_on_overflow() {
    const FRAME_SAMPLES: usize = 300;
    const TICK: usize = 100;
    const MAX_FRAMES_TO_OVERFLOW: usize = 400;

    let start = square_voice(
        CgbChannelNumber::Square1,
        Some(upward_sweep(1, 1)),
        TestNote::at_key(0),
    );
    let phase = |voice: &CgbVoice| voice.square_oscillator().expect("square").duty_phase();

    // The tick retunes the rate the whole deferred span is then rated at.
    let mut idle = start.clone();
    idle.advance_idle_duty(FRAME_SAMPLES, &[TICK]);
    let mut deferred = start.clone();
    assert!(deferred.oscillator.step_sweep_tick());
    deferred.oscillator.advance_silently(FRAME_SAMPLES);
    assert_eq!(phase(&idle), phase(&deferred));
    let mut eager = start.clone();
    eager.oscillator.advance_silently(TICK);
    assert!(eager.oscillator.step_sweep_tick());
    eager.oscillator.advance_silently(FRAME_SAMPLES - TICK);
    assert_ne!(
        phase(&idle),
        phase(&eager),
        "the catch-up must not be rated at each intermediate frequency"
    );
    let mut unswept = start.clone();
    unswept.advance_idle_duty(FRAME_SAMPLES, &[]);
    assert_ne!(
        phase(&idle),
        phase(&unswept),
        "the tick must retune the rate"
    );

    // An overflow mutes the channel; its frequency then stays frozen.
    let mut overflowing = start;
    let mut frames = 0;
    while !overflowing.hardware_muted {
        assert!(frames < MAX_FRAMES_TO_OVERFLOW, "the sweep must overflow");
        overflowing.advance_idle_duty(FRAME_SAMPLES, &[TICK]);
        frames += 1;
    }
    let frozen_frequency = overflowing.sweep_frequency();
    let mut free_running = overflowing.clone();
    free_running.oscillator.advance_silently(FRAME_SAMPLES);
    overflowing.advance_idle_duty(FRAME_SAMPLES, &[TICK]);
    assert_eq!(overflowing.sweep_frequency(), frozen_frequency);
    assert_eq!(phase(&overflowing), phase(&free_running));
}

fn sweep_frequency_after(sweep_byte: u8, len: usize, schedule: &[usize]) -> u16 {
    let mut voice = low_freq_sweep_voice(sweep_byte);
    voice.begin_frame(false);
    let ticks: Vec<usize> = schedule.iter().copied().filter(|&t| t < len).collect();
    let mut acc = vec![(0i32, 0i32); len];
    voice.render(&mut acc, &ticks);
    voice
        .sweep_frequency()
        .expect("still a square voice with a sweep configured")
}

fn sweep_steps(sweep_byte: u8, total: usize, schedule: &[usize]) -> Vec<(usize, u16)> {
    let mut steps = Vec::new();
    let mut previous = sweep_frequency_after(sweep_byte, 0, schedule);
    for len in 1..=total {
        let frequency = sweep_frequency_after(sweep_byte, len, schedule);
        if frequency != previous {
            steps.push((len - 1, frequency));
            previous = frequency;
        }
    }
    steps
}

#[test]
fn square1_sweep_period_1_steps_once_per_scheduled_128hz_tick() {
    let mut clock = FrameSequencer128Hz::default();
    let schedule = clock.advance(600);
    assert_eq!(schedule, vec![104, 209, 313, 418, 522]);

    assert_eq!(
        sweep_steps(upward_sweep(1, 1), 600, &schedule),
        vec![(104, 66), (209, 99), (313, 148), (418, 222), (522, 333)],
    );
}

#[test]
fn square1_sweep_period_2_steps_once_per_second_scheduled_tick() {
    let mut clock = FrameSequencer128Hz::default();
    let schedule = clock.advance(1200);
    assert_eq!(
        schedule,
        vec![104, 209, 313, 418, 522, 627, 731, 836, 940, 1045, 1149]
    );

    assert_eq!(
        sweep_steps(upward_sweep(2, 1), 1200, &schedule),
        vec![(209, 66), (418, 99), (627, 148), (836, 222), (1045, 333)],
    );
}

#[test]
fn cgb_voice_render_is_chunk_boundary_invariant() {
    let make_voice = || low_freq_sweep_voice(upward_sweep(1, 1));

    let mut whole_voice = make_voice();
    whole_voice.begin_frame(false);
    let mut whole_clock = FrameSequencer128Hz::default();
    let whole_ticks = whole_clock.advance(600);
    let mut whole_acc = vec![(0i32, 0i32); 600];
    whole_voice.render(&mut whole_acc, &whole_ticks);

    let mut split_voice = make_voice();
    split_voice.begin_frame(false);
    let mut split_clock = FrameSequencer128Hz::default();
    let first_ticks = split_clock.advance(300);
    let mut first_half = vec![(0i32, 0i32); 300];
    split_voice.render(&mut first_half, &first_ticks);
    let second_ticks = split_clock.advance(300);
    let mut second_half = vec![(0i32, 0i32); 300];
    split_voice.render(&mut second_half, &second_ticks);
    let mut split_acc = first_half;
    split_acc.extend(second_half);

    assert_eq!(whole_acc, split_acc);
    assert!(
        whole_acc.iter().any(|&(l, r)| l != 0 || r != 0),
        "sanity: the sweeping voice must actually be audible"
    );
}

#[test]
fn square1_sweep_overflow_mutes_the_voice_mid_buffer_until_the_next_safe_trigger() {
    // A running sweep's overflow clears the channel-enable bit exactly as
    // a trigger-time overflow does, and leaves the same later trigger able
    // to revive it (`mgba/src/gb/audio.c:667-672`, `:180-186`).
    let safe_key = 48;
    let mut voice = square_voice(
        CgbChannelNumber::Square1,
        Some(upward_sweep(1, 1)),
        TestNote::at_key(safe_key),
    );
    assert!(
        voice.is_active(),
        "not born dead: the trigger check alone doesn't overflow"
    );
    voice.begin_frame(false);

    let mut clock = FrameSequencer128Hz::default();
    let ticks = clock.advance(300);
    let first_tick = ticks[0];
    let mut acc = vec![(0i32, 0i32); 300];
    voice.render(&mut acc, &ticks);

    assert!(
        acc[..first_tick].iter().any(|&(l, r)| l != 0 || r != 0),
        "samples before the overflowing tick must still be audible"
    );
    assert!(
        acc[first_tick..].iter().all(|&(l, r)| l == 0 && r == 0),
        "samples from the overflowing tick onward must be silent, not just \
         at the buffer end"
    );
    assert!(
        voice.is_active(),
        "the overflow mutes the hardware channel; the software voice lives on"
    );
    let mut still_muted = vec![(0i32, 0i32); 8];
    voice.begin_frame(false);
    voice.render(&mut still_muted, &[]);
    assert!(
        still_muted.iter().all(|&(l, r)| l == 0 && r == 0),
        "the mute holds across frames until a trigger rechecks the sweep"
    );

    voice.set_track_pitch(0, 0);
    voice.set_track_volume(FULL_TRACK_VOLUME, FULL_TRACK_VOLUME);
    voice.begin_frame(false);
    let mut revived = vec![(0i32, 0i32); 8];
    voice.render(&mut revived, &[]);
    assert!(
        revived.iter().any(|&(l, r)| l != 0 || r != 0),
        "a later safe trigger must revive the muted voice, not find it retired"
    );
}

#[test]
fn attack_to_decay_volume_write_retriggers_channel1_sweep_from_the_current_frequency() {
    // The attack-to-decay transition is a volume write that reloads
    // channel 1's sweep from the channel's *current* played frequency,
    // not a pitch bend's stale shadow (`m4a.c:1150-1158,1219-1226`;
    // `mgba/src/gb/audio.c:180-186`).
    let safe_key: u8 = 48;
    let overflow_prone_key: u8 = 120; // shares its overflow fixture with
                                      // `square1_upward_sweep_overflow_is_born_muted_and_revives_on_a_safe_trigger`.
    let mut voice = CgbVoice::square(
        CgbChannelNumber::Square1,
        HALF_DUTY,
        Some(upward_sweep(0, 1)),
        CgbAdsr {
            attack: 0,
            decay: 1,
            sustain: MAX_ADSR_LEVEL,
            release: 0,
        },
        safe_key,
        0,
        FULL_TRACK_VOLUME,
        FULL_TRACK_VOLUME,
        FULL_VELOCITY,
        0,
        safe_key,
        0,
        0,
        0,
        0,
    );
    assert!(
        voice.is_active(),
        "constructed at a safe frequency, the sweep must not be born dead"
    );

    voice.set_track_pitch(i32::from(overflow_prone_key) - i32::from(safe_key), 0);
    assert!(
        voice.is_active(),
        "a pitch bend alone must not retrigger the sweep"
    );

    voice.begin_frame(false); // attack==0 -> decay!=0: retriggers

    assert!(
        voice.is_active(),
        "an overflowing trigger mutes the hardware channel, not the software voice"
    );
    let mut acc = vec![(0i32, 0i32); 8];
    voice.render(&mut acc, &[]);
    assert!(
        acc.iter().all(|&(l, r)| l == 0 && r == 0),
        "the attack-to-decay volume write must retrigger channel 1, reloading the sweep \
         shadow from the bent frequency and finding it overflows"
    );
}

#[test]
fn live_track_volume_update_retriggers_channel1_sweep_from_the_current_frequency() {
    // `set_track_volume` is itself a volume-write trigger, distinct from
    // an envelope transition (`m4a_1.s:1391-1400`, applied at `m4a.c:1219-1226`).
    let safe_key: u8 = 48;
    let overflow_prone_key: u8 = 120;
    let mut voice = square_voice(
        CgbChannelNumber::Square1,
        Some(upward_sweep(0, 1)),
        TestNote::at_key(safe_key),
    );
    // Settle `CgbAdsr::flat()`'s own instant retrigger first, so this
    // frame isolates `set_track_volume`'s retrigger below.
    voice.begin_frame(false);
    assert!(
        voice.is_active(),
        "the settling frame must not itself overflow"
    );

    voice.set_track_pitch(i32::from(overflow_prone_key) - i32::from(safe_key), 0);
    assert!(
        voice.is_active(),
        "a pitch bend alone must not retrigger the sweep"
    );

    voice.set_track_volume(FULL_TRACK_VOLUME, FULL_TRACK_VOLUME);
    assert!(
        voice.is_active(),
        "sanity: set_track_volume must not retrigger before the next begin_frame"
    );

    voice.begin_frame(false); // steady in sustain now, so only
                              // set_track_volume's retrigger explains
                              // the mute below

    assert!(
        voice.is_active(),
        "an overflowing trigger mutes the hardware channel, not the software voice"
    );
    let mut acc = vec![(0i32, 0i32); 8];
    voice.render(&mut acc, &[]);
    assert!(
        acc.iter().all(|&(l, r)| l == 0 && r == 0),
        "a live volume update must retrigger channel 1, reloading the sweep shadow from \
         the bent frequency and finding it overflows"
    );
}

#[test]
fn a_trigger_time_sweep_overflow_only_mutes_the_channel_until_the_next_safe_trigger() {
    let safe_key = 48;
    let overflowing_key = 120;
    let mut voice = square_voice(
        CgbChannelNumber::Square1,
        Some(upward_sweep(0, 1)),
        TestNote::at_key(safe_key),
    );
    assert!(voice.is_active(), "sanity: the note is born playing");

    // The volume write's trigger, after this bend, rechecks the sweep
    // and overflows (`mgba/src/gb/audio.c:180-196`).
    voice.set_track_pitch(i32::from(overflowing_key - safe_key), 0);
    voice.set_track_volume(FULL_TRACK_VOLUME, FULL_TRACK_VOLUME);
    voice.begin_frame(false);
    let mut muted = vec![(0i32, 0i32); 8];
    voice.render(&mut muted, &[]);
    assert!(
        muted.iter().all(|&(l, r)| l == 0 && r == 0),
        "the overflowing trigger must silence the hardware channel"
    );

    // The next trigger reruns the same recheck and finds no overflow
    // (`m4a.c:1053-1056`).
    voice.set_track_pitch(0, 0);
    voice.set_track_volume(FULL_TRACK_VOLUME, FULL_TRACK_VOLUME);
    voice.begin_frame(false);
    let mut revived = vec![(0i32, 0i32); 8];
    voice.render(&mut revived, &[]);
    assert!(
        revived.iter().any(|&(l, r)| l != 0 || r != 0),
        "a later safe trigger must revive the muted voice, not find it retired"
    );
}

/// A zero-volume square holds hardware's `dead == 2` envelope, so its duty
/// catch-up is deferred like an idle slot's and rated at the frequency the
/// sweep finally reaches (`mgba/src/gb/audio.c:493-501,946-950`).
#[test]
fn a_zero_volume_square_defers_its_duty_catch_up_across_sweep_ticks() {
    const FRAME_SAMPLES: usize = 300;
    const TICK: usize = 100;
    let mut silent = square_voice(
        CgbChannelNumber::Square1,
        Some(upward_sweep(1, 1)),
        TestNote {
            track_right: 0,
            track_left: 0,
            ..TestNote::at_key(0)
        },
    );
    let start = silent.clone();
    silent.begin_frame(false);
    assert!(silent.is_active(), "nonzero sustain keeps the voice active");
    assert_eq!(silent.envelope.volume(), 0, "the zero goal holds volume 0");
    let mut acc = vec![(0i32, 0i32); FRAME_SAMPLES];
    silent.render(&mut acc, &[TICK]);
    assert!(acc.iter().all(|&(l, r)| l == 0 && r == 0));

    let phase = |voice: &CgbVoice| voice.square_oscillator().expect("square").duty_phase();
    let mut deferred = start;
    assert!(deferred.oscillator.step_sweep_tick());
    deferred.oscillator.advance_silently(FRAME_SAMPLES);
    assert_eq!(
        phase(&silent),
        phase(&deferred),
        "a dead == 2 channel's silence must be rated at the swept frequency, not each intermediate one"
    );
}

/// The deferral survives frame boundaries: a later sweep tick retunes the
/// still-unsettled silence, which is rated once at the final frequency.
#[test]
fn a_zero_volume_square_keeps_deferring_across_frames() {
    const FRAME_SAMPLES: usize = 300;
    let mut silent = square_voice(
        CgbChannelNumber::Square1,
        Some(upward_sweep(1, 1)),
        TestNote {
            track_right: 0,
            track_left: 0,
            ..TestNote::at_key(0)
        },
    );
    let mut deferred = silent.clone();
    for _ in 0..2 {
        silent.begin_frame(false);
        let mut acc = vec![(0i32, 0i32); FRAME_SAMPLES];
        silent.render(&mut acc, &[100]);
        assert!(deferred.oscillator.step_sweep_tick());
    }
    deferred.oscillator.advance_silently(2 * FRAME_SAMPLES);
    let phase = |voice: &CgbVoice| voice.square_oscillator().expect("square").duty_phase();
    assert_eq!(phase(&silent), phase(&deferred));
}

/// A dead square inherits a nonzero duty remainder; silent sweep ticks leave
/// it unrated and settlement retimes it once at the final frequency
/// (`mgba/src/gb/audio.c:493-503,975-979`).
#[test]
fn a_dead_square_preserves_its_inherited_remainder_across_silent_sweep_ticks() {
    const FIRST_FREQUENCY: u16 = 0x400;
    const FINAL_FREQUENCY: u16 = 0x640;
    const SWEEP_BYTE: u8 = 0x12;
    const FRAME_SAMPLES: usize = 17;

    let mut silent = square_voice(
        CgbChannelNumber::Square1,
        Some(SWEEP_BYTE),
        TestNote {
            track_right: 0,
            track_left: 0,
            ..TestNote::at_key(0)
        },
    );
    silent.begin_frame(false);
    assert!(silent.is_dead_at_zero());

    let sweep = crate::psg::Sweep::from_byte(SWEEP_BYTE, FIRST_FREQUENCY);
    let mut inherited = SquareChannel::new(HALF_DUTY, FIRST_FREQUENCY, Some(sweep));
    for _ in 0..13 {
        let _ = inherited.sample();
    }
    silent.oscillator = Oscillator::Square(inherited.clone());

    let mut expected = SquareChannel::new(HALF_DUTY, FINAL_FREQUENCY, None);
    expected.continue_duty_from(&inherited);
    expected.advance_silently(FRAME_SAMPLES);

    let mut acc = vec![(0i32, 0i32); FRAME_SAMPLES];
    silent.render(&mut acc, &[4, 12]);
    assert!(acc.iter().all(|&(left, right)| left == 0 && right == 0));
    assert_eq!(silent.sweep_frequency(), Some(FINAL_FREQUENCY));
    assert_eq!(
        silent.square_oscillator().expect("square").duty_phase(),
        expected.duty_phase(),
    );
}
