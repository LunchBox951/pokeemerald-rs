//! Stateless value scaling ([`scale_volume`], [`centre_relative`],
//! [`convert_ticks`], [`bpm_from_microseconds`]) and the control-change ->
//! [`SongEvent`] mapping ([`translate_controller`]) [`super::compile`]'s
//! pipeline needs — split out of that module to keep it under this crate's
//! ~600-line-per-file guideline (`oop-boundaries`), mirroring
//! `xtask::extract::voicegroups`'s own file-per-concern split.
//!
//! # Reproduced: the dropped wait after CC `0x1E`
//!
//! `agb.cpp:402-405`'s `case 0x1E:` records the extended-command selector
//! (`s_extendedCommand = event.param2;`) and `break`s **without** the
//! `PrintWait(event.time)` every other arm of that switch ends with. Since
//! `event.time` is that event's gap to the *next* event (`CalculateWaits`,
//! `midi.cpp:759-767`), upstream silently swallows that gap: a CC `0x1E`
//! followed by, say, 24 ticks of rest shifts everything after it on that
//! track 24 ticks early. That looks like an oversight next to the `// TODO:
//! loop op` comment sitting on the same arm, not an intended musical
//! effect — but this compiler's job is to reproduce upstream's compiled
//! output, oversight or not, so it does.
//!
//! [`translate_controller`] returns [`ControllerEvent::ExtendedCommandSelect`]
//! for CC `0x1E`: no [`SongEvent`] is emitted (exactly as upstream emits no
//! byte), but unlike every other silent controller it is not simply
//! discarded — `super::compile::compile_track` still records it as a
//! timing-only item so `super::compile::emit_track` can single it out and
//! drop the [`SongEvent::Wait`] that would otherwise follow it, matching
//! upstream's missing `PrintWait(event.time)` exactly. `mus_title.mid`'s own
//! six CC `0x1E` occurrences (two each on the three pseudo-echo tracks,
//! `mus_title_7`/`_8`/`_10`) all sit at tick `0` with a zero gap to the
//! `0x1D`/`0x1F` that consumes them, so this is unobservable there (confirmed
//! against a locally built `tools/mid2agb` oracle — all ten compiled tracks
//! are byte-identical either way) but real for a song with a rest after a CC
//! `0x1E` (`super::compile`'s tests).

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
///
/// Upstream stores the rounded result in a 32-bit `int`
/// (`static_cast<int>`, `agb.cpp:505-507`) and never range-checks it before
/// formatting it into the `.s` output. A Rust `as u16` cast on that same
/// `f32` does not have a 32-bit intermediate to be honest about being
/// wrong in — it saturates straight to `u16::MAX`, so every microseconds
/// value in `1..=915` (all of which round to a BPM the wire schema's `u16`
/// [`super::event::SongEvent::Tempo`] field cannot hold) would silently
/// compile to the *same* `65535` tempo instead of failing. This compiler
/// fails closed instead of reproducing that collapse.
///
/// # Errors
///
/// [`MidiError::TempoOverflow`] if the rounded BPM does not fit a `u16`.
pub(super) fn bpm_from_microseconds(microseconds: u32) -> Result<u16, MidiError> {
    #[expect(
        clippy::cast_precision_loss,
        reason = "upstream's own conversion is f32 arithmetic (agb.cpp:506); \
                  reproducing it is the point"
    )]
    let microseconds_f32 = microseconds as f32;
    let bpm = (60_000_000.0_f32 / microseconds_f32).round();
    if bpm > f32::from(u16::MAX) {
        return Err(MidiError::TempoOverflow(microseconds));
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "bpm is in 0.0..=65535.0 here: microseconds > 0 keeps the \
                  division positive, and the range check above rules out \
                  anything over u16::MAX"
    )]
    let bpm = bpm as u16;
    Ok(bpm)
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

/// What one translated control-change message means to
/// `super::compile::compile_track`: silence (matching upstream's own
/// `default: PrintWait(event.time); break;` no-op — a plain wait, nothing
/// else), a genuine [`SongEvent`], or CC `0x1E`'s own case, which is
/// neither — no [`SongEvent`] is emitted (upstream writes no byte either),
/// but it is not silence: `emit_track` needs a timing-only marker for it so
/// it can drop the wait that follows, reproducing upstream's missing
/// `PrintWait(event.time)` (module docs, "Reproduced: the dropped wait
/// after CC `0x1E`").
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ControllerEvent {
    None,
    Event(SongEvent),
    ExtendedCommandSelect,
}

/// Translate one control-change message into a [`ControllerEvent`],
/// mirroring `agb.cpp:349-415`'s `PrintControllerOp` switch one arm at a
/// time. `extended_command` is per-track state a preceding CC `0x1E` sets,
/// read back by CC `0x1D`/`0x1F` (`super::compile`'s module docs, "Extended
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
) -> Result<ControllerEvent, MidiError> {
    let event = match controller {
        0x01 => ControllerEvent::Event(SongEvent::Modulation(value)),
        0x07 => ControllerEvent::Event(SongEvent::Volume(scale_volume(value, master_volume))),
        0x0A => ControllerEvent::Event(SongEvent::Pan(centre_relative(value))),
        0x0C..=0x11 => return Err(MidiError::UnsupportedMemAccController(controller)),
        0x14 => ControllerEvent::Event(SongEvent::BendRange(value)),
        0x15 => ControllerEvent::Event(SongEvent::LfoSpeed(value)),
        0x16 => ControllerEvent::Event(SongEvent::ModType(value)),
        0x18 => ControllerEvent::Event(SongEvent::Tune(centre_relative(value))),
        0x1A => ControllerEvent::Event(SongEvent::LfoDelay(value)),
        0x1D | 0x1F => match translate_extended_command(*extended_command, value) {
            Some(event) => ControllerEvent::Event(event),
            None => ControllerEvent::None,
        },
        0x1E => {
            *extended_command = Some(value);
            ControllerEvent::ExtendedCommandSelect
        }
        0x21 | 0x27 => ControllerEvent::Event(SongEvent::Priority(value)),
        _ => ControllerEvent::None,
    };
    Ok(event)
}

#[cfg(test)]
mod tests {
    use super::super::error::MidiError;
    use super::{bpm_from_microseconds, translate_controller, ControllerEvent};

    /// `916` microseconds/quarter-note is the smallest value whose rounded
    /// BPM (`65502`) still fits a `u16`; one microsecond less (`915`, the
    /// next test) rounds to `65574`, which does not.
    #[test]
    fn bpm_from_microseconds_boundary_still_representable() {
        assert_eq!(bpm_from_microseconds(916), Ok(65502));
    }

    /// The overflow side of the same boundary: `915` microseconds/quarter
    /// rounds to `65574`, past `u16::MAX`, and must error rather than
    /// saturate (`agb.cpp:505-507` computes into a 32-bit `int` and never
    /// range-checks it; a Rust `as u16` cast would silently saturate here
    /// instead).
    #[test]
    fn bpm_from_microseconds_boundary_overflow() {
        assert_eq!(
            bpm_from_microseconds(915),
            Err(MidiError::TempoOverflow(915))
        );
    }

    /// The extreme case: one microsecond per quarter note rounds to
    /// `60_000_000` BPM, nowhere close to fitting a `u16`.
    #[test]
    fn bpm_from_microseconds_one_microsecond_overflows() {
        assert_eq!(bpm_from_microseconds(1), Err(MidiError::TempoOverflow(1)));
    }

    /// CC `0x1E` sets the extended-command state and reports the
    /// timing-only marker `super::super::compile` needs, matching
    /// `agb.cpp:402-405`'s own no-byte `case 0x1E:` (module docs,
    /// "Reproduced: the dropped wait after CC `0x1E`").
    #[test]
    fn cc_0x1e_selects_without_emitting_an_event() {
        let mut extended_command = None;
        let result = translate_controller(0x1E, 8, 127, &mut extended_command).unwrap();
        assert_eq!(result, ControllerEvent::ExtendedCommandSelect);
        assert_eq!(extended_command, Some(8));
    }
}
