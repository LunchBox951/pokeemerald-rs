//! Square duty-phase continuity through replacement, retirement, and idle sweep.

use super::test_support::*;
use super::*;
use crate::cgb_envelope::CgbAdsr;
use crate::cgb_voice::CgbChannelNumber;

fn committed_voice(mut voice: CgbVoice) -> CgbVoice {
    voice.begin_frame(false);
    voice
}

#[test]
fn a_square_note_on_continues_the_duty_phase_of_the_note_it_replaces() {
    const TRACK: usize = 0;
    const KEY: u8 = 60;

    let mut retriggered = Mixer::new(MAX_MASTER_VOLUME, 1);
    let mut sustaining = Mixer::new(MAX_MASTER_VOLUME, 1);
    assert!(retriggered.add_cgb_voice(cgb_keyed_voice(TRACK, KEY)));
    assert!(sustaining.add_cgb_voice(cgb_keyed_voice(TRACK, KEY)));

    let mut retriggered_out = vec![0.0; SAMPLES_PER_FRAME * 2];
    let mut sustaining_out = vec![0.0; SAMPLES_PER_FRAME * 2];
    retriggered.mix_frame(&mut retriggered_out);
    sustaining.mix_frame(&mut sustaining_out);
    assert_eq!(
        retriggered_out, sustaining_out,
        "the two mixers must still be identical before the re-attack",
    );

    assert!(retriggered.add_cgb_voice(cgb_keyed_voice(TRACK, KEY)));

    retriggered.mix_frame(&mut retriggered_out);
    sustaining.mix_frame(&mut sustaining_out);
    assert_eq!(
        retriggered_out, sustaining_out,
        "a square note-on must carry the channel's duty phase forward instead \
         of restarting the duty table at index zero",
    );
}

#[test]
fn a_square_note_on_a_slot_stop_track_vacated_continues_the_idle_duty_phase() {
    const TRACK: usize = 0;
    const KEY: u8 = 60;
    const IDLE_FRAMES: usize = 3;

    let mut occupied = Mixer::new(MAX_MASTER_VOLUME, 1);
    let mut vacated = Mixer::new(MAX_MASTER_VOLUME, 1);
    // Oracle: an occupied slot already at the idle rate.
    let mut idle_rate_reference = cgb_keyed_voice(TRACK, KEY);
    idle_rate_reference.apply_hardware_off_write();
    assert!(occupied.add_cgb_voice(idle_rate_reference));
    assert!(vacated.add_cgb_voice(committed_voice(cgb_keyed_voice(TRACK, KEY))));
    vacated.stop_track(TRACK);

    let mut occupied_out = vec![0.0; SAMPLES_PER_FRAME * 2];
    let mut vacated_out = vec![0.0; SAMPLES_PER_FRAME * 2];
    for frame in 0..IDLE_FRAMES {
        occupied.mix_frame(&mut occupied_out);
        vacated.mix_frame(&mut vacated_out);
        assert!(
            vacated_out.iter().all(|&sample| sample == 0.0),
            "frame {frame}: a vacated slot must render silence"
        );
    }

    assert!(occupied.add_cgb_voice(cgb_keyed_voice(TRACK, KEY)));
    assert!(vacated.add_cgb_voice(cgb_keyed_voice(TRACK, KEY)));

    occupied.mix_frame(&mut occupied_out);
    vacated.mix_frame(&mut vacated_out);
    assert_eq!(
        occupied_out, vacated_out,
        "a note on a stop_track-vacated slot must continue the duty index \
         the slot's free-running register would have reached, not reset it \
         to zero",
    );
}

#[test]
fn a_square_note_on_a_naturally_retired_slot_continues_the_idle_duty_phase() {
    const TRACK: usize = 0;
    const KEY: u8 = 60;
    const IDLE_FRAMES: usize = 3;
    const MAX_FRAMES_TO_RETIRE: usize = 32;

    let mut occupied = Mixer::new(MAX_MASTER_VOLUME, 1);
    let mut retired = Mixer::new(MAX_MASTER_VOLUME, 1);
    assert!(occupied.add_cgb_voice(cgb_keyed_voice(TRACK, KEY)));
    assert!(retired.add_cgb_voice(committed_voice(cgb_keyed_voice(TRACK, KEY))));
    retired.note_off_track(TRACK, KEY);

    let mut occupied_out = vec![0.0; SAMPLES_PER_FRAME * 2];
    let mut retired_out = vec![0.0; SAMPLES_PER_FRAME * 2];
    let mut frames_to_retire = 0;
    while retired.cgb_voices()[CgbChannelNumber::Square1.slot()].is_some() {
        retired.mix_frame(&mut retired_out);
        // Switch the oracle to the idle rate at the instant the slot idles.
        if retired.cgb_voices()[CgbChannelNumber::Square1.slot()].is_none() {
            let mut idle_rate_reference = cgb_keyed_voice(TRACK, KEY);
            idle_rate_reference.apply_hardware_off_write();
            assert!(occupied.add_cgb_voice(idle_rate_reference));
        }
        occupied.mix_frame(&mut occupied_out);
        frames_to_retire += 1;
        assert!(
            frames_to_retire < MAX_FRAMES_TO_RETIRE,
            "test setup must retire well within this bound"
        );
    }

    for frame in 0..IDLE_FRAMES {
        occupied.mix_frame(&mut occupied_out);
        retired.mix_frame(&mut retired_out);
        assert!(
            retired_out.iter().all(|&sample| sample == 0.0),
            "frame {frame}: a retired slot must render silence while idle"
        );
    }

    assert!(occupied.add_cgb_voice(cgb_keyed_voice(TRACK, KEY)));
    assert!(retired.add_cgb_voice(cgb_keyed_voice(TRACK, KEY)));

    occupied.mix_frame(&mut occupied_out);
    retired.mix_frame(&mut retired_out);
    assert_eq!(
        occupied_out, retired_out,
        "a note on a naturally retired slot must continue the duty index \
         the slot's free-running register would have reached, not reset it \
         to zero",
    );
}

/// Sweep that overflows at trigger: hardware muted from frame 0, envelope running.
const OVERFLOWING_SWEEP_AT_TRIGGER: u8 = 0x31;
const OVERFLOW_KEY: u8 = 120;

fn cgb_muted_at_trigger_voice(track: usize, key: u8) -> CgbVoice {
    CgbVoice::square(
        CgbChannelNumber::Square1,
        2,
        Some(OVERFLOWING_SWEEP_AT_TRIGGER),
        CgbAdsr::flat(),
        key,
        0,
        FULL_TRACK_VOLUME,
        FULL_TRACK_VOLUME,
        TEST_VELOCITY,
        TIED_GATE_TIME,
        key,
        track,
        0,
        0,
        0,
    )
}

/// A hardware-muted voice's unconditional duty catch-up in `render` must
/// not also double-count the frame its envelope retires on top of the
/// idle-duty loop's own advance for that same frame.
#[test]
fn a_note_on_a_slot_a_muted_voice_vacated_does_not_double_count_the_retirement_frame() {
    const TRACK: usize = 0;
    const IDLE_FRAMES: usize = 3;
    const MAX_FRAMES_TO_RETIRE: usize = 32;

    // A trigger stopped before its first frame is cancelled and retires no hardware.
    let slot = CgbChannelNumber::Square1.slot();
    let muted = committed_voice(cgb_muted_at_trigger_voice(TRACK, OVERFLOW_KEY));
    assert!(muted.initial_trigger_committed());
    assert!(muted.is_active());

    let mut occupied = Mixer::new(MAX_MASTER_VOLUME, 1);
    let mut retired = Mixer::new(MAX_MASTER_VOLUME, 1);
    assert!(retired.add_cgb_voice(muted.clone()));
    retired.note_off_track(TRACK, OVERFLOW_KEY);

    let mut occupied_out = vec![0.0; SAMPLES_PER_FRAME * 2];
    let mut retired_out = vec![0.0; SAMPLES_PER_FRAME * 2];
    let mut frames_to_retire = 0;
    while retired.cgb_voices()[CgbChannelNumber::Square1.slot()].is_some() {
        retired.mix_frame(&mut retired_out);
        assert!(
            retired_out.iter().all(|&sample| sample == 0.0),
            "a muted voice must render silence while it still holds the slot"
        );
        if retired.cgb_voices()[CgbChannelNumber::Square1.slot()].is_none() {
            assert!(occupied.add_cgb_voice(muted.clone()));
            occupied.stop_track(TRACK);
            assert!(occupied.cgb_voices()[slot].is_none());
            for mixer in [&occupied, &retired] {
                let hardware = mixer.idle_cgb_hardware[slot].as_ref().unwrap();
                assert!(hardware.initial_trigger_committed());
            }
        }
        occupied.mix_frame(&mut occupied_out);
        frames_to_retire += 1;
        assert!(
            frames_to_retire < MAX_FRAMES_TO_RETIRE,
            "test setup must retire well within this bound"
        );
    }

    for frame in 0..IDLE_FRAMES {
        occupied.mix_frame(&mut occupied_out);
        retired.mix_frame(&mut retired_out);
        assert!(
            retired_out.iter().all(|&sample| sample == 0.0),
            "frame {frame}: a retired slot must render silence while idle"
        );
        assert!(occupied.idle_cgb_hardware[slot].is_some());
        assert!(retired.idle_cgb_hardware[slot].is_some());
    }

    assert!(occupied.add_cgb_voice(cgb_keyed_voice(TRACK, OVERFLOW_KEY)));
    assert!(retired.add_cgb_voice(cgb_keyed_voice(TRACK, OVERFLOW_KEY)));

    occupied.mix_frame(&mut occupied_out);
    retired.mix_frame(&mut retired_out);
    assert!(occupied_out.iter().any(|&sample| sample != 0.0));
    assert_eq!(
        occupied_out, retired_out,
        "a note on a slot a muted voice vacated must not double-count the \
         retirement frame's silence",
    );
}

fn frame_after_idling_a_stopped_square1(
    sweep: Option<u8>,
    key: u8,
    idle_frames: usize,
) -> Vec<f32> {
    const TRACK: usize = 0;

    let mut mixer = Mixer::new(MAX_MASTER_VOLUME, 1);
    assert!(mixer.add_cgb_voice(committed_voice(cgb_swept_voice(TRACK, key, sweep))));
    mixer.stop_track(TRACK);
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    for _ in 0..idle_frames {
        mixer.mix_frame(&mut out);
    }
    assert!(mixer.add_cgb_voice(cgb_swept_voice(TRACK, key, None)));
    mixer.mix_frame(&mut out);
    out
}

#[test]
fn verify_idle_square1_sweep_keeps_ticking_after_stop_track() {
    const KEY: u8 = 60;
    const DOWNWARD_PERIOD_1_SHIFT_1: u8 = 0x19;

    assert_ne!(
        frame_after_idling_a_stopped_square1(Some(DOWNWARD_PERIOD_1_SHIFT_1), KEY, 3),
        frame_after_idling_a_stopped_square1(None, KEY, 3),
        "an idle slot's sweep must retune the duty rate the next note continues",
    );
}

/// Upstream START|STOP jumps straight to oscillator-off, so a replacement
/// cancelled before its first frame leaves the established note's hardware
/// to take the off-write the next audible note inherits.
#[test]
fn cancelled_pending_square_preserves_committed_hardware() {
    for (channel, old_sweep, pending_sweep) in [
        (CgbChannelNumber::Square1, Some(0x00), Some(0x00)),
        (CgbChannelNumber::Square1, Some(0x70), Some(0x19)),
        (CgbChannelNumber::Square2, None, None),
    ] {
        let slot = channel.slot();
        let mut cancelled = Mixer::new(MAX_MASTER_VOLUME, 1);
        let mut stopped = Mixer::new(MAX_MASTER_VOLUME, 1);
        let mut actual = vec![0.0; SAMPLES_PER_FRAME * 2];
        let mut expected = actual.clone();
        assert!(cancelled.add_cgb_voice(cgb_square_voice(channel, 0, 60, old_sweep)));
        assert!(stopped.add_cgb_voice(cgb_square_voice(channel, 0, 60, old_sweep)));
        cancelled.mix_frame(&mut actual);
        stopped.mix_frame(&mut expected);
        assert_eq!(actual, expected);
        assert!(actual.iter().any(|&sample| sample != 0.0));

        assert!(cancelled.add_cgb_voice(cgb_square_voice(channel, 0, 61, pending_sweep)));
        cancelled.note_off_track(0, 61);
        stopped.stop_track(0);
        for _ in 0..4 {
            cancelled.mix_frame(&mut actual);
            stopped.mix_frame(&mut expected);
            assert!(cancelled.cgb_slots[slot].is_none());
            assert!(actual.iter().all(|&sample| sample == 0.0));
            assert_eq!(actual, expected);
        }

        assert!(cancelled.add_cgb_voice(cgb_square_voice(channel, 0, 60, None)));
        assert!(stopped.add_cgb_voice(cgb_square_voice(channel, 0, 60, None)));
        for _ in 0..4 {
            cancelled.mix_frame(&mut actual);
            stopped.mix_frame(&mut expected);
            assert!(actual.iter().any(|&sample| sample != 0.0));
            assert_eq!(
                actual, expected,
                "later audible note must inherit identical duty"
            );
        }
    }
}

/// An immediately retiring first pass jumps to `oscillator_off` past the
/// `NRx3` pitch write (`pokeemerald/src/m4a.c:1053-1056,1090-1102,1197-1202`),
/// so the slot keeps the predecessor's low frequency byte for the next note.
#[test]
fn an_immediately_retiring_square_replacement_never_writes_its_pitch() {
    const TRACK: usize = 0;
    for (channel, sweep) in [
        (CgbChannelNumber::Square1, Some(0x00)),
        (CgbChannelNumber::Square2, None),
    ] {
        let slot = channel.slot();
        let retiring = CgbVoice::square(
            channel,
            2,
            sweep,
            CgbAdsr {
                attack: 0,
                decay: 0,
                sustain: 0,
                release: 0,
            },
            72,
            0,
            FULL_TRACK_VOLUME,
            FULL_TRACK_VOLUME,
            TEST_VELOCITY,
            TIED_GATE_TIME,
            72,
            TRACK,
            0,
            0,
            0,
        );
        let mut replaced = Mixer::new(MAX_MASTER_VOLUME, 1);
        let mut stopped = Mixer::new(MAX_MASTER_VOLUME, 1);
        let mut actual = vec![0.0; SAMPLES_PER_FRAME * 2];
        let mut expected = actual.clone();
        assert!(replaced.add_cgb_voice(cgb_square_voice(channel, TRACK, 60, sweep)));
        assert!(stopped.add_cgb_voice(cgb_square_voice(channel, TRACK, 60, sweep)));
        replaced.mix_frame(&mut actual);
        stopped.mix_frame(&mut expected);
        assert_eq!(actual, expected);
        assert!(actual.iter().any(|&sample| sample != 0.0));

        assert!(replaced.add_cgb_voice(retiring));
        stopped.stop_track(TRACK);
        for _ in 0..4 {
            replaced.mix_frame(&mut actual);
            stopped.mix_frame(&mut expected);
            assert!(replaced.cgb_slots[slot].is_none());
            assert!(actual.iter().all(|&sample| sample == 0.0));
            assert_eq!(
                replaced.idle_cgb_hardware[slot]
                    .as_ref()
                    .and_then(CgbVoice::square_frequency),
                stopped.idle_cgb_hardware[slot]
                    .as_ref()
                    .and_then(CgbVoice::square_frequency),
                "the retained frequency must be the predecessor's low byte",
            );
        }

        assert!(replaced.add_cgb_voice(cgb_square_voice(channel, TRACK, 60, None)));
        assert!(stopped.add_cgb_voice(cgb_square_voice(channel, TRACK, 60, None)));
        for _ in 0..4 {
            replaced.mix_frame(&mut actual);
            stopped.mix_frame(&mut expected);
            assert!(actual.iter().any(|&sample| sample != 0.0));
            assert_eq!(
                actual, expected,
                "the next note must inherit identical duty"
            );
        }
    }
}

/// A swept predecessor whose frequency left its note's `NR14` high bits:
/// upstream's retiring first pass goes straight from the swept register to
/// `CgbOscOff`'s `NR14 = 0x80` (`pokeemerald/src/m4a.c:1053-1056`), never
/// through the predecessor's stale high bits.
#[test]
fn a_swept_predecessors_retiring_replacement_keeps_the_next_notes_duty() {
    const TRACK: usize = 0;
    const SWEEP_DOWN: u8 = 0x1F;
    let channel = CgbChannelNumber::Square1;
    let slot = channel.slot();
    let retiring = CgbVoice::square(
        channel,
        2,
        Some(SWEEP_DOWN),
        CgbAdsr {
            attack: 0,
            decay: 0,
            sustain: 0,
            release: 0,
        },
        72,
        0,
        FULL_TRACK_VOLUME,
        FULL_TRACK_VOLUME,
        TEST_VELOCITY,
        TIED_GATE_TIME,
        72,
        TRACK,
        0,
        0,
        0,
    );
    let mut replaced = Mixer::new(MAX_MASTER_VOLUME, 1);
    let mut stopped = Mixer::new(MAX_MASTER_VOLUME, 1);
    let mut actual = vec![0.0; SAMPLES_PER_FRAME * 2];
    let mut expected = actual.clone();
    assert!(replaced.add_cgb_voice(cgb_square_voice(channel, TRACK, 72, Some(SWEEP_DOWN))));
    assert!(stopped.add_cgb_voice(cgb_square_voice(channel, TRACK, 72, Some(SWEEP_DOWN))));
    replaced.mix_frame(&mut actual);
    stopped.mix_frame(&mut expected);
    assert_eq!(actual, expected);
    let swept = replaced.cgb_slots[slot]
        .as_ref()
        .and_then(CgbVoice::square_frequency)
        .expect("the predecessor is a playing square");
    assert_ne!(
        swept & 0x700,
        0x700,
        "the sweep must cross the high-byte boundary"
    );

    assert!(replaced.add_cgb_voice(retiring));
    stopped.stop_track(TRACK);
    replaced.mix_frame(&mut actual);
    stopped.mix_frame(&mut expected);
    assert!(replaced.cgb_slots[slot].is_none());

    assert!(replaced.add_cgb_voice(cgb_square_voice(channel, TRACK, 60, None)));
    assert!(stopped.add_cgb_voice(cgb_square_voice(channel, TRACK, 60, None)));
    for frame in 0..4 {
        replaced.mix_frame(&mut actual);
        stopped.mix_frame(&mut expected);
        assert!(actual.iter().any(|&sample| sample != 0.0));
        assert_eq!(
            actual, expected,
            "frame {frame}: the next note must inherit the duty upstream leaves",
        );
    }
}

/// Retires `voice` after `settled_frames` of sound, applying
/// `retiring_key_shift` on the retiring frame, and returns the idle-tail frames
/// (plus a replacement note's first frame for a square). Upstream discards that
/// pitch write (`pokeemerald/src/m4a.c:1053-1056`, `:1196-1203`).
fn retire_with_key_shift(
    voice: &CgbVoice,
    slot: usize,
    settled_frames: usize,
    retiring_key_shift: Option<i32>,
    replacement: Option<CgbVoice>,
) -> Vec<f32> {
    const TRACK: usize = 0;
    const IDLE_FRAMES: usize = 40;
    const MAX_FRAMES_TO_RETIRE: usize = 32;
    let mut mixer = Mixer::new(MAX_MASTER_VOLUME, 1);
    assert!(mixer.add_cgb_voice(voice.clone()));
    let mut frame = vec![0.0; SAMPLES_PER_FRAME * 2];
    let mut rendered = Vec::new();
    for _ in 0..settled_frames {
        mixer.mix_frame(&mut frame);
    }
    mixer.note_off_track(TRACK, 60);
    if let Some(shift) = retiring_key_shift {
        mixer.set_track_pitch(TRACK, shift, 0);
    }
    for _ in 0..MAX_FRAMES_TO_RETIRE {
        mixer.mix_frame(&mut frame);
        rendered.extend_from_slice(&frame);
        if mixer.cgb_voices()[slot].is_none() {
            break;
        }
    }
    assert!(mixer.cgb_voices()[slot].is_none(), "the voice must retire");
    for _ in 0..IDLE_FRAMES {
        mixer.mix_frame(&mut frame);
        rendered.extend_from_slice(&frame);
    }
    if let Some(next) = replacement {
        assert!(mixer.add_cgb_voice(next));
        mixer.mix_frame(&mut frame);
        rendered.extend_from_slice(&frame);
    }
    rendered
}

#[test]
fn a_pitch_write_on_the_retiring_frame_never_reaches_the_retained_square() {
    const TRACK: usize = 0;
    const KEY: u8 = 60;
    let voice = committed_voice(cgb_keyed_voice(TRACK, KEY));
    let slot = CgbChannelNumber::Square1.slot();
    let replacement = || Some(cgb_keyed_voice(TRACK, KEY));
    assert_eq!(
        retire_with_key_shift(&voice, slot, 0, None, replacement()),
        retire_with_key_shift(&voice, slot, 0, Some(7), replacement()),
        "a retiring square's discarded pitch write must not move the duty \
         phase the next note continues from",
    );
}

#[test]
fn a_pitch_write_on_the_retiring_frame_never_reaches_the_retained_noise_latch() {
    const TRACK: usize = 0;
    const KEY: u8 = 60;
    const SLOW_KEY_SHIFT: i32 = -39;
    const SETTLE_FRAME_RANGE: std::ops::Range<usize> = 0..24;
    let voice = CgbVoice::noise(
        CgbAdsr::flat(),
        KEY,
        0,
        FULL_TRACK_VOLUME,
        FULL_TRACK_VOLUME,
        TEST_VELOCITY,
        TIED_GATE_TIME,
        KEY,
        TRACK,
        0,
        0,
        0,
    );
    let slot = CgbChannelNumber::Noise.slot();
    // The latch depends on the clock phase at retirement.
    for settled_frames in SETTLE_FRAME_RANGE {
        let plain = retire_with_key_shift(&voice, slot, settled_frames, None, None);
        let shifted =
            retire_with_key_shift(&voice, slot, settled_frames, Some(SLOW_KEY_SHIFT), None);
        assert_eq!(
            plain, shifted,
            "after {settled_frames} frames a retiring noise voice must \
             off-write its committed clock, not the discarded pitch",
        );
    }
}

/// `MPlayExtender` writes `NR12`/`NR22` = 0x08 and `NR14`/`NR24` = 0x80
/// (`pokeemerald/src/m4a.c:268-273`), triggering both squares at frequency 0
/// before any note; their duty index then free-runs, and the first note's
/// register write catches it up (`mgba/src/gb/audio.c:162-171,493-510`).
const SILENT_LEAD_IN_FRAMES: usize = 3;

fn first_square_frame_after_lead_in(channel: CgbChannelNumber, lead_in: usize) -> Vec<f32> {
    const TRACK: usize = 0;
    const KEY: u8 = 60;

    let mut mixer = Mixer::new(MAX_MASTER_VOLUME, 1);
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    for _ in 0..lead_in {
        mixer.mix_frame(&mut out);
        assert!(out.iter().all(|&sample| sample == 0.0));
    }
    assert!(mixer.add_cgb_voice(cgb_square_voice(channel, TRACK, KEY, None)));
    mixer.mix_frame(&mut out);
    out
}

/// The initialized-hardware oracle: the same square channel explicitly triggered
/// at frequency 0 on the slot, committed, then idled for `lead_in` frames.
fn first_square_frame_after_explicit_init(channel: CgbChannelNumber, lead_in: usize) -> Vec<f32> {
    const TRACK: usize = 0;
    const KEY: u8 = 60;

    let mut mixer = Mixer::new(MAX_MASTER_VOLUME, 1);
    let init = CgbVoice::square(
        channel,
        0,
        None,
        CgbAdsr::flat(),
        0,
        0,
        FULL_TRACK_VOLUME,
        FULL_TRACK_VOLUME,
        TEST_VELOCITY,
        TIED_GATE_TIME,
        0,
        TRACK,
        0,
        0,
        0,
    )
    .with_square_frequency_register(0);
    mixer.idle_cgb_hardware[channel.slot()] = Some(committed_voice(init));
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    for _ in 0..lead_in {
        mixer.mix_frame(&mut out);
    }
    assert!(mixer.add_cgb_voice(cgb_square_voice(channel, TRACK, KEY, None)));
    mixer.mix_frame(&mut out);
    out
}

#[test]
fn a_first_square_note_after_silent_frames_inherits_the_elapsed_idle_duty_phase() {
    for channel in [CgbChannelNumber::Square1, CgbChannelNumber::Square2] {
        assert_ne!(
            first_square_frame_after_lead_in(channel, SILENT_LEAD_IN_FRAMES),
            first_square_frame_after_lead_in(channel, 0),
            "{channel:?}: the silent lead-in must advance the initialized duty index",
        );
    }
}

#[test]
fn a_first_square_note_matches_an_explicitly_initialized_hardware_oracle() {
    for channel in [CgbChannelNumber::Square1, CgbChannelNumber::Square2] {
        for lead_in in [0, 1, SILENT_LEAD_IN_FRAMES] {
            assert_eq!(
                first_square_frame_after_lead_in(channel, lead_in),
                first_square_frame_after_explicit_init(channel, lead_in),
                "{channel:?} after {lead_in} silent frames",
            );
        }
    }
}
