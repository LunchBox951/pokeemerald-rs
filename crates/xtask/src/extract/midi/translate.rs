//! Stateless value scaling ([`scale_volume`], [`centre_relative`],
//! [`convert_ticks`], [`bpm_from_microseconds`]) and the control-change ->
//! [`SongEvent`] mapping ([`translate_controller`]) [`super::compile`]'s
//! pipeline needs — split out of that module to keep it under this crate's
//! ~600-line-per-file guideline (`oop-boundaries`), mirroring
//! `xtask::extract::voicegroups`'s own file-per-concern split.
//!
//! # Known divergence: the trailing wait after CC `0x1E`
//!
//! `agb.cpp:402-405`'s `case 0x1E:` records the extended-command selector
//! (`s_extendedCommand = event.param2;`) and `break`s **without** the
//! `PrintWait(event.time)` every other arm of that switch ends with. Since
//! `event.time` is that event's gap to the *next* event (`CalculateWaits`,
//! `midi.cpp:758-773`), upstream silently swallows that gap: a CC `0x1E`
//! followed by, say, 24 ticks of rest shifts everything after it on that
//! track 24 ticks early. That looks like an oversight next to the `// TODO:
//! loop op` comment sitting on the same arm, not an intended musical
//! effect.
//!
//! This compiler does **not** reproduce it. [`translate_controller`] returns
//! `None` for CC `0x1E` (no [`SongEvent`] is emitted, exactly as upstream
//! emits no byte), but `super::compile`'s `emit_track` still pushes the gap
//! that follows it, so the rest of the track keeps its original timing.
//! That is a real, deliberate behavioural divergence from `tools/mid2agb`,
//! recorded here and in the ledger's own reason rather than buried: it is
//! *unobservable for `mus_title.mid`*, whose six CC `0x1E` occurrences (two
//! each on the three pseudo-echo tracks, `mus_title_7`/`_8`/`_10`) all
//! sit at tick `0` with a zero gap to the `0x1D`/`0x1F` that consumes them
//! (confirmed against a locally built `tools/mid2agb` oracle — all ten
//! compiled tracks are byte-identical either way), but a different song
//! with a rest after a CC `0x1E` would compile to a track this compiler
//! times differently from upstream. Time-preserving is the defensible
//! reading of a desync bug, so the divergence stands; if a future slice
//! ever needs upstream's exact desync, this is the note to revisit.

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
/// [`super::compile::compile`]'s own guard. `division` is nonzero because
/// that entry point rejects a zero header division before calling this
/// private helper.
///
/// Evaluated in `u64`, unlike upstream's `std::uint32_t` expression: a
/// legal 4-byte VLQ reaches `0x0FFF_FFFF`, and `24 * 0x0FFF_FFFF` is
/// already past `u32::MAX`, so doing this multiply in `u32` would wrap
/// (release) or panic (debug) on a tick a hostile — or merely
/// weird — `.mid` file may legitimately encode. Widening keeps the answer
/// exact for every input whose *result* still fits, which is every input
/// with a sane `division`; only the quotient is range-checked
/// ([`MidiError::TickOverflow`]), so the never-panic contract this module
/// and [`super::reader`] share holds for arbitrary bytes.
///
/// # Errors
///
/// [`MidiError::TickOverflow`] if the scaled tick does not fit a `u32` —
/// where upstream would silently wrap. Unreachable for any real file: with
/// the standard `division` of 24 or more the quotient is at most `raw`
/// itself, so this needs both a past-4-byte-VLQ tick and a tiny `division`.
pub(super) fn convert_ticks(raw: u32, division: u16) -> Result<u32, MidiError> {
    let scaled = (24 * u64::from(raw)) / u64::from(division);
    u32::try_from(scaled).map_err(|_| MidiError::TickOverflow(raw))
}

/// `round(60_000_000.0f32 / microseconds)` (`agb.cpp:506`) — an `f32`
/// division and round, not `f64`. `microseconds` is nonzero because
/// [`super::parse::parse_track`] rejects zero-tempo payloads before this
/// private helper is reached. See [`super::compile`]'s module docs on why
/// this compiler stores this real BPM value directly rather than the further
/// `*tbs/2`-scaled wire byte the compiled ROM actually carries.
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
        // No event, matching upstream's own no-byte `case 0x1E:` -- but
        // this compiler keeps the wait that follows it, which upstream
        // drops. See the module docs, "Known divergence".
        0x1E => {
            *extended_command = Some(value);
            None
        }
        0x21 | 0x27 => Some(SongEvent::Priority(value)),
        _ => None,
    };
    Ok(event)
}
