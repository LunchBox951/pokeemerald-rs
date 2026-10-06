//! Square duty-phase continuity through replacement, retirement, and idle sweep.

use super::test_support::*;
use super::*;
use crate::cgb_envelope::CgbAdsr;
use crate::cgb_voice::CgbChannelNumber;

/// A note taking over an occupied square slot must pick the duty phase up
/// where the note it replaced left it (`SquareChannel::continue_duty_from`'s doc).
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

    // Same track, same key, same priority: the slot is reusable, so this note
    // replaces the sounding one for the next frame.
    assert!(retriggered.add_cgb_voice(cgb_keyed_voice(TRACK, KEY)));

    retriggered.mix_frame(&mut retriggered_out);
    sustaining.mix_frame(&mut sustaining_out);
    assert_eq!(
        retriggered_out, sustaining_out,
        "a square note-on must carry the channel's duty phase forward instead \
         of restarting the duty table at index zero",
    );
}

/// A note landing on a square slot that `stop_track` vacated must continue
/// the duty index the slot's free-running register would have reached over
/// that silence, not restart at zero (`CgbVoice::advance_idle_duty`'s doc).
#[test]
fn a_square_note_on_a_slot_stop_track_vacated_continues_the_idle_duty_phase() {
    const TRACK: usize = 0;
    const KEY: u8 = 60;
    const IDLE_FRAMES: usize = 3;

    let mut occupied = Mixer::new(MAX_MASTER_VOLUME, 1);
    let mut vacated = Mixer::new(MAX_MASTER_VOLUME, 1);
    // A slot's off-write truncates its frequency the instant it idles
    // (`CgbVoice::apply_hardware_off_write`'s doc); a slot rated the same
    // way and left occupied for the same span is the oracle.
    let mut idle_rate_reference = cgb_keyed_voice(TRACK, KEY);
    idle_rate_reference.apply_hardware_off_write();
    assert!(occupied.add_cgb_voice(idle_rate_reference));
    assert!(vacated.add_cgb_voice(cgb_keyed_voice(TRACK, KEY)));
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

/// The same continuation, this time through natural envelope retirement
/// instead of `stop_track`.
#[test]
fn a_square_note_on_a_naturally_retired_slot_continues_the_idle_duty_phase() {
    const TRACK: usize = 0;
    const KEY: u8 = 60;
    const IDLE_FRAMES: usize = 3;
    const MAX_FRAMES_TO_RETIRE: usize = 32;

    let mut occupied = Mixer::new(MAX_MASTER_VOLUME, 1);
    let mut retired = Mixer::new(MAX_MASTER_VOLUME, 1);
    assert!(occupied.add_cgb_voice(cgb_keyed_voice(TRACK, KEY)));
    assert!(retired.add_cgb_voice(cgb_keyed_voice(TRACK, KEY)));
    retired.note_off_track(TRACK, KEY);

    let mut occupied_out = vec![0.0; SAMPLES_PER_FRAME * 2];
    let mut retired_out = vec![0.0; SAMPLES_PER_FRAME * 2];
    let mut frames_to_retire = 0;
    while retired.cgb_voices()[CgbChannelNumber::Square1.slot()].is_some() {
        retired.mix_frame(&mut retired_out);
        // The instant a slot idles, its off-write truncates the rate its
        // duty catch-up measures by; switch the still-sounding oracle to
        // that same rate, at the same instant, to track it.
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

// Overflows at trigger, muting the hardware from frame 0 while the
// envelope keeps running (`square1_upward_sweep_overflow_is_born_muted_and_revives_on_a_safe_trigger`,
// cgb_voice.rs).
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

    let mut occupied = Mixer::new(MAX_MASTER_VOLUME, 1);
    let mut retired = Mixer::new(MAX_MASTER_VOLUME, 1);
    assert!(retired.add_cgb_voice(cgb_muted_at_trigger_voice(TRACK, OVERFLOW_KEY)));
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
            // The oracle idles through `stop_track`, which counts the frame
            // once, at the same vacated instant.
            assert!(occupied.add_cgb_voice(cgb_muted_at_trigger_voice(TRACK, OVERFLOW_KEY)));
            occupied.stop_track(TRACK);
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

    assert!(occupied.add_cgb_voice(cgb_keyed_voice(TRACK, OVERFLOW_KEY)));
    assert!(retired.add_cgb_voice(cgb_keyed_voice(TRACK, OVERFLOW_KEY)));

    occupied.mix_frame(&mut occupied_out);
    retired.mix_frame(&mut retired_out);
    assert_eq!(
        occupied_out, retired_out,
        "a note on a slot a muted voice vacated must not double-count the \
         retirement frame's silence",
    );
}

/// Mixes `idle_frames` over a Square1 slot `stop_track` vacated from a voice
/// carrying `sweep`, then plays an unswept note on it and returns that frame.
fn frame_after_idling_a_stopped_square1(
    sweep: Option<u8>,
    key: u8,
    idle_frames: usize,
) -> Vec<f32> {
    const TRACK: usize = 0;

    let mut mixer = Mixer::new(MAX_MASTER_VOLUME, 1);
    assert!(mixer.add_cgb_voice(cgb_swept_voice(TRACK, key, sweep)));
    mixer.stop_track(TRACK);
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    for _ in 0..idle_frames {
        mixer.mix_frame(&mut out);
    }
    assert!(mixer.add_cgb_voice(cgb_swept_voice(TRACK, key, None)));
    mixer.mix_frame(&mut out);
    out
}

/// A vacated Square1 slot's hardware sweep keeps ticking and retuning while
/// it idles, so the next note inherits a different duty phase than an
/// unswept slot's (`CgbVoice::advance_idle_duty`'s doc).
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
