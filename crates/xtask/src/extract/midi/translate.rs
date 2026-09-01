//! MIDI control-change and timing value translation.
//!
//! # Extended-command selector timing
//!
//! Upstream's extended-command selector omits the trailing wait emitted by
//! every other controller arm (`tools/mid2agb/agb.cpp:402-405`). By then,
//! timing marks and split points have divided that wait into chunks.
//! [`ControllerEvent::ExtendedCommandSelect`] preserves the selector as a
//! timing marker so [`super::compile`] suppresses only the first chunk. Other
//! silent controllers retain their complete wait.

use super::error::MidiError;
use super::event::SongEvent;

const MIDI_CONTROL_MAX: u8 = 127;
const MIDI_CONTROL_CENTER: i16 = 64;
const SONG_TICKS_PER_BEAT: u64 = 24;
const MICROSECONDS_PER_MINUTE: f32 = 60_000_000.0;

const MODULATION_CONTROLLER: u8 = 0x01;
const VOLUME_CONTROLLER: u8 = 0x07;
const PAN_CONTROLLER: u8 = 0x0A;
const MEMACC_FIRST_CONTROLLER: u8 = 0x0C;
const MEMACC_LAST_CONTROLLER: u8 = 0x11;
const BEND_RANGE_CONTROLLER: u8 = 0x14;
const LFO_SPEED_CONTROLLER: u8 = 0x15;
const MODULATION_TYPE_CONTROLLER: u8 = 0x16;
const TUNE_CONTROLLER: u8 = 0x18;
const LFO_DELAY_CONTROLLER: u8 = 0x1A;
const EXTENDED_COMMAND_CONTROLLER: u8 = 0x1D;
const EXTENDED_COMMAND_SELECT_CONTROLLER: u8 = 0x1E;
const ALTERNATE_EXTENDED_COMMAND_CONTROLLER: u8 = 0x1F;
const PRIORITY_CONTROLLER: u8 = 0x21;
const ALTERNATE_PRIORITY_CONTROLLER: u8 = 0x27;

const PSEUDO_ECHO_VOLUME_COMMAND: u8 = 0x08;
const PSEUDO_ECHO_LENGTH_COMMAND: u8 = 0x09;

/// Scales a MIDI control value by the configured master volume using
/// truncating integer division.
pub(super) fn scale_volume(raw: u8, master_volume: u8) -> u8 {
    let scaled = (u32::from(raw) * u32::from(master_volume)) / u32::from(MIDI_CONTROL_MAX);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a MIDI control value is at most 127, so the result cannot exceed master_volume"
    )]
    let scaled = scaled as u8;
    scaled
}

/// Converts a MIDI control value to its signed, center-relative value.
pub(super) fn centre_relative(raw: u8) -> i8 {
    let centered = i16::from(raw) - MIDI_CONTROL_CENTER;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "centering a MIDI control value gives -64..=63"
    )]
    let centered = centered as i8;
    centered
}

/// Converts raw MIDI ticks to the song's 24-ticks-per-beat scale.
///
/// The widened multiplication prevents input overflow before division; only
/// the final quotient must fit the song's `u32` tick representation.
///
/// # Errors
///
/// Returns [`MidiError::TickOverflow`] if the scaled tick exceeds `u32`.
///
/// # Panics
///
/// Panics if `division` is zero. The MIDI compiler rejects a zero time
/// division before translating ticks.
pub(super) fn convert_ticks(raw: u32, division: u16) -> Result<u32, MidiError> {
    let scaled = (SONG_TICKS_PER_BEAT * u64::from(raw)) / u64::from(division);
    u32::try_from(scaled).map_err(|_| MidiError::TickOverflow(raw))
}

/// Converts microseconds per quarter note to `f32`-rounded beats per minute.
///
/// The `f32` calculation preserves the source compiler's rounding
/// (`tools/mid2agb/agb.cpp:505-507`). Values outside the song's `u16` tempo
/// representation fail instead of saturating.
///
/// # Errors
///
/// Returns [`MidiError::TempoOverflow`] if the rounded tempo exceeds `u16`.
pub(super) fn bpm_from_microseconds(microseconds: u32) -> Result<u16, MidiError> {
    #[expect(
        clippy::cast_precision_loss,
        reason = "tools/mid2agb uses f32 arithmetic for this conversion"
    )]
    let microseconds_f32 = microseconds as f32;
    let bpm = (MICROSECONDS_PER_MINUTE / microseconds_f32).round();
    if bpm > f32::from(u16::MAX) {
        return Err(MidiError::TempoOverflow(microseconds));
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the positive BPM is bounded by u16::MAX above"
    )]
    let bpm = bpm as u16;
    Ok(bpm)
}

fn translate_extended_command(selected_command: Option<u8>, value: u8) -> Option<SongEvent> {
    match selected_command {
        Some(PSEUDO_ECHO_VOLUME_COMMAND) => Some(SongEvent::PseudoEchoVolume(value)),
        Some(PSEUDO_ECHO_LENGTH_COMMAND) => Some(SongEvent::PseudoEchoLength(value)),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ControllerEvent {
    /// Preserves controller timing without emitting a song event.
    None,
    /// Emits the translated song event and preserves controller timing.
    Event(SongEvent),
    /// Updates extended-command state and invokes the selector timing quirk.
    ExtendedCommandSelect,
}

/// Translates a control change and maintains its track's extended-command
/// selection.
///
/// # Errors
///
/// Returns [`MidiError::UnsupportedMemAccController`] for a `MEMACC`
/// controller.
pub(super) fn translate_controller(
    controller: u8,
    value: u8,
    master_volume: u8,
    extended_command: &mut Option<u8>,
) -> Result<ControllerEvent, MidiError> {
    let event = match controller {
        MODULATION_CONTROLLER => ControllerEvent::Event(SongEvent::Modulation(value)),
        VOLUME_CONTROLLER => {
            ControllerEvent::Event(SongEvent::Volume(scale_volume(value, master_volume)))
        }
        PAN_CONTROLLER => ControllerEvent::Event(SongEvent::Pan(centre_relative(value))),
        MEMACC_FIRST_CONTROLLER..=MEMACC_LAST_CONTROLLER => {
            return Err(MidiError::UnsupportedMemAccController(controller));
        }
        BEND_RANGE_CONTROLLER => ControllerEvent::Event(SongEvent::BendRange(value)),
        LFO_SPEED_CONTROLLER => ControllerEvent::Event(SongEvent::LfoSpeed(value)),
        MODULATION_TYPE_CONTROLLER => ControllerEvent::Event(SongEvent::ModType(value)),
        TUNE_CONTROLLER => ControllerEvent::Event(SongEvent::Tune(centre_relative(value))),
        LFO_DELAY_CONTROLLER => ControllerEvent::Event(SongEvent::LfoDelay(value)),
        EXTENDED_COMMAND_CONTROLLER | ALTERNATE_EXTENDED_COMMAND_CONTROLLER => {
            match translate_extended_command(*extended_command, value) {
                Some(event) => ControllerEvent::Event(event),
                None => ControllerEvent::None,
            }
        }
        EXTENDED_COMMAND_SELECT_CONTROLLER => {
            *extended_command = Some(value);
            ControllerEvent::ExtendedCommandSelect
        }
        PRIORITY_CONTROLLER | ALTERNATE_PRIORITY_CONTROLLER => {
            ControllerEvent::Event(SongEvent::Priority(value))
        }
        _ => ControllerEvent::None,
    };
    Ok(event)
}

#[cfg(test)]
mod tests {
    use super::super::error::MidiError;
    use super::{
        bpm_from_microseconds, translate_controller, ControllerEvent,
        EXTENDED_COMMAND_SELECT_CONTROLLER, MIDI_CONTROL_MAX, PSEUDO_ECHO_VOLUME_COMMAND,
    };

    #[test]
    fn bpm_accepts_smallest_representable_microsecond_interval() {
        let smallest_representable = 916;
        assert_eq!(bpm_from_microseconds(smallest_representable), Ok(65502));
    }

    #[test]
    fn bpm_rejects_first_overflowing_microsecond_interval() {
        let first_overflowing = 915;
        assert_eq!(
            bpm_from_microseconds(first_overflowing),
            Err(MidiError::TempoOverflow(first_overflowing))
        );
    }

    #[test]
    fn bpm_rejects_one_microsecond_interval() {
        assert_eq!(bpm_from_microseconds(1), Err(MidiError::TempoOverflow(1)));
    }

    #[test]
    fn extended_command_selector_updates_state_without_song_event() {
        let mut extended_command = None;
        let result = translate_controller(
            EXTENDED_COMMAND_SELECT_CONTROLLER,
            PSEUDO_ECHO_VOLUME_COMMAND,
            MIDI_CONTROL_MAX,
            &mut extended_command,
        )
        .unwrap();
        assert_eq!(result, ControllerEvent::ExtendedCommandSelect);
        assert_eq!(extended_command, Some(PSEUDO_ECHO_VOLUME_COMMAND));
    }
}
