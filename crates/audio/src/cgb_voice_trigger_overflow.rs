//! A sweep overflow at a trigger stops the channel but re-latches its level
//! from the duty position and the reset initial volume
//! (`mgba/src/gb/audio.c:168-193,745-754,856-860,916-918`).
use super::fixture::*;
use super::*;

const OVERFLOW_KEY: u8 = 120;
/// Two full-scale track sides centre to envelope level 15, so the hardware
/// volume is 15 and a high duty step mixes `240 * 32 >> 8` per side.
const HIGH: (i32, i32) = (30, 30);
const LOW: (i32, i32) = (-30, -30);
const FULL_LEVEL_GAIN: i32 = 240;

fn full_level_note(key: u8) -> TestNote {
    TestNote {
        track_right: MAX_TRACK_VOLUME,
        track_left: MAX_TRACK_VOLUME,
        ..TestNote::at_key(key)
    }
}

fn overflowing_voice(duty: u8, sweep: u8, adsr: CgbAdsr) -> CgbVoice {
    let note = full_level_note(OVERFLOW_KEY);
    CgbVoice::square(
        CgbChannelNumber::Square1,
        duty,
        Some(sweep),
        adsr,
        note.note_key,
        note.fine_pitch,
        note.track_right,
        note.track_left,
        note.velocity,
        note.gate_time,
        note.played_key,
        note.track,
        note.rhythm_pan,
        note.echo_volume,
        note.echo_length,
    )
}

fn render(voice: &mut CgbVoice, samples: usize, ticks: &[usize]) -> Vec<(i32, i32)> {
    let mut acc = vec![(0i32, 0i32); samples];
    voice.render(&mut acc, ticks);
    acc
}

fn assert_held(voice: &CgbVoice, acc: &[(i32, i32)], level: (i32, i32)) {
    assert!(acc.iter().all(|&sample| sample == level), "{acc:?}");
    assert_eq!(voice.last_contribution, level.0);
    assert_eq!(voice.hardware_envelope_volume.volume(), 15);
    assert_eq!(voice.frame_gain, FULL_LEVEL_GAIN);
    assert!(voice.is_active(), "the software voice keeps its slot");
    assert!(voice.hardware_muted && voice.sweep_held);
}

/// A note-on overflow holds the duty level at the cold index, scaled by the
/// initial volume, instead of rendering silence.
#[test]
fn a_note_on_trigger_overflow_holds_the_duty_level_at_the_initial_volume() {
    // Key 120 plays register 2032; one shift-1 step reaches 3048 > 2047.
    for (duty, level) in [(2, HIGH), (0, LOW)] {
        for sweep in [upward_sweep(0, 1), upward_sweep(3, 1)] {
            let mut voice = overflowing_voice(duty, sweep, CgbAdsr::flat());
            voice.begin_frame(false);
            let acc = render(&mut voice, 8, &[]);
            assert_held(&voice, &acc, level);
            let again = {
                voice.begin_frame(false);
                render(&mut voice, 8, &[])
            };
            assert_held(&voice, &again, level);
        }
    }
}

/// The reset volume is the one the trigger wrote, and a stopped envelope does
/// not step it afterwards (`mgba/src/gb/audio.c:711-717,856-860`).
#[test]
fn a_trigger_hold_keeps_the_initial_volume_while_the_software_envelope_decays() {
    let adsr = CgbAdsr {
        attack: 0,
        decay: 3,
        sustain: 0,
        release: 0,
    };
    let mut voice = overflowing_voice(2, upward_sweep(1, 1), adsr);
    for extra_iteration in [false, true, false, true] {
        voice.begin_frame(extra_iteration);
        let acc = render(&mut voice, 16, &[3]);
        assert_held(&voice, &acc, HIGH);
    }
}

fn playing_voice() -> CgbVoice {
    // Register 0x400 -> 1536 after one shift-1 step: safe.
    let mut voice = square_voice(
        CgbChannelNumber::Square1,
        Some(upward_sweep(1, 1)),
        full_level_note(0),
    )
    .with_square_frequency_register(0x400);
    voice.begin_frame(false);
    assert!(!voice.hardware_muted, "sanity: the first trigger is safe");
    voice
}

fn retrigger_into_overflow(voice: &mut CgbVoice) {
    voice.set_track_pitch(i32::from(OVERFLOW_KEY), 0);
    voice.set_track_volume(MAX_TRACK_VOLUME, MAX_TRACK_VOLUME);
    voice.begin_frame(false);
}

/// An unsafe retrigger replaces the level a playing square held with the duty
/// level at the new position: index 4 of the half duty is low.
#[test]
fn an_unsafe_retrigger_replaces_a_playing_squares_level() {
    let mut voice = playing_voice();
    assert_eq!(render(&mut voice, 1, &[]), [HIGH]);
    retrigger_into_overflow(&mut voice);
    let acc = render(&mut voice, 8, &[]);
    assert_held(&voice, &acc, LOW);
}

/// The retrigger after a pitch write is the later latch: the pitch write's
/// pre-high-byte latch (+1 at index 0) must not survive it.
#[test]
fn an_unsafe_retrigger_replaces_a_sweep_held_squares_level() {
    let mut voice = playing_voice();
    let acc = render(&mut voice, 1, &[0]);
    assert_eq!(acc, [HIGH], "the tick overflows the look-ahead and holds");
    assert!(voice.sweep_held);
    assert_eq!(voice.square_frequency(), Some(1536));
    retrigger_into_overflow(&mut voice);
    let acc = render(&mut voice, 8, &[]);
    assert_held(&voice, &acc, LOW);
}

/// The off-write is a trigger at volume zero from a low frequency: it ends the
/// hold rather than carrying the level into the idle slot.
#[test]
fn the_off_write_ends_a_trigger_hold() {
    let mut voice = overflowing_voice(2, upward_sweep(1, 1), CgbAdsr::flat());
    voice.begin_frame(false);
    assert!(voice.sweep_held);
    voice.apply_hardware_off_write();
    assert!(!voice.hardware_muted && !voice.sweep_held);
}
