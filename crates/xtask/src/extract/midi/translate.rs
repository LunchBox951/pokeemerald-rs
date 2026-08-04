//! Stateless value scaling ([`scale_volume`], [`centre_relative`],
//! [`convert_ticks`], [`bpm_from_microseconds`]) and the control-change ->
//! [`SongEvent`] mapping ([`translate_controller`]) [`super::compile`]'s
//! pipeline needs — split out of that module to keep it under this crate's
//! ~600-line-per-file guideline (`oop-boundaries`), mirroring
//! `xtask::extract::voicegroups`'s own file-per-concern split.

use super::error::MidiError;
use super::event::SongEvent;

/// Scale a raw `0..=127` control-change value against `midi.cfg`'s `-V`
/// master volume, matching `agb.cpp:357`'s assembler-evaluated
/// `<value>*<label>_mvl/mxv` expression (`mxv` = `0x7F` = 127,
/// `sound/MPlayDef.s:128`) — plain truncating integer division, the same
/// arithmetic the GNU assembler performs on that expression.
pub(super) fn scale_volume(raw: u8, master_volume: u8) -> u8 {
    let scaled = (u32::from(raw) * u32::from(master_volume)) / 127;
    #[allow(clippy::cast_possible_truncation)] // raw, master_volume <= 127 => scaled <= 127
    let scaled = scaled as u8;
    scaled
}

/// A raw `0..=127` byte, centred at `64` (`c_v`, `sound/MPlayDef.s:132`),
/// matching `agb.cpp:360`/`:393`/`:513`'s `<value> - 64` (pan, tune, pitch
/// bend).
pub(super) fn centre_relative(raw: u8) -> i8 {
    let v = i16::from(raw) - 64;
    #[allow(clippy::cast_possible_truncation)] // raw in 0..=127 => v in -64..=63
    let v = v as i8;
    v
}

/// `24 * clocks_per_beat * raw / division` (`midi.cpp:635`/`:641`'s
/// `ConvertTimes`), with `clocks_per_beat` pinned to `1` by
/// [`super::compile::compile`]'s own guard.
pub(super) fn convert_ticks(raw: u32, division: u16) -> u32 {
    (24 * raw) / u32::from(division)
}

/// `round(60_000_000.0f32 / microseconds)` (`agb.cpp:506`) — an `f32`
/// division and round, not `f64`; see [`super::compile`]'s module docs on
/// why this compiler stores this real BPM value directly rather than the
/// further `*tbs/2`-scaled wire byte the compiled ROM actually carries.
pub(super) fn bpm_from_microseconds(microseconds: u32) -> u16 {
    #[allow(clippy::cast_precision_loss)]
    let microseconds = microseconds as f32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let bpm = (60_000_000.0_f32 / microseconds).round() as u16;
    bpm
}

/// Translate one extended-command trigger (`agb.cpp:331-347`'s
/// `PrintExtendedOp`) into a [`SongEvent`], given the sub-command
/// `extended_command` a preceding CC `0x1E` selected. Any value other than
/// `8`/`9` is a silent no-op, matching upstream's own `default` branch
/// (`super::compile`'s module docs, "Extended commands").
fn translate_extended_command(extended_command: Option<u8>, value: u8) -> Option<SongEvent> {
    match extended_command {
        Some(0x08) => Some(SongEvent::PseudoEchoVolume(value)),
        Some(0x09) => Some(SongEvent::PseudoEchoLength(value)),
        _ => None,
    }
}

/// Translate one control-change message into a [`SongEvent`], mirroring
/// `agb.cpp:349-415`'s `PrintControllerOp` switch one arm at a time.
/// `extended_command` is per-track state a preceding CC `0x1E` sets, read
/// back by CC `0x1D`/`0x1F` (`super::compile`'s module docs, "Extended
/// commands").
///
/// # Errors
///
/// [`MidiError::UnsupportedMemAccController`] for the `MEMACC` family
/// (`super::compile`'s module docs).
pub(super) fn translate_controller(
    controller: u8,
    value: u8,
    master_volume: u8,
    extended_command: &mut Option<u8>,
) -> Result<Option<SongEvent>, MidiError> {
    let event = match controller {
        0x01 => Some(SongEvent::Modulation(value)),
        0x07 => Some(SongEvent::Volume(scale_volume(value, master_volume))),
        0x0A => Some(SongEvent::Pan(centre_relative(value))),
        0x0C..=0x11 => return Err(MidiError::UnsupportedMemAccController(controller)),
        0x14 => Some(SongEvent::BendRange(value)),
        0x15 => Some(SongEvent::LfoSpeed(value)),
        0x16 => Some(SongEvent::ModType(value)),
        0x18 => Some(SongEvent::Tune(centre_relative(value))),
        0x1A => Some(SongEvent::LfoDelay(value)),
        0x1D | 0x1F => translate_extended_command(*extended_command, value),
        0x1E => {
            *extended_command = Some(value);
            None
        }
        0x21 | 0x27 => Some(SongEvent::Priority(value)),
        _ => None,
    };
    Ok(event)
}
