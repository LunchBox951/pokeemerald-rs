//! Compiles parsed MIDI chunks into explicit [`SongEvent`] streams.
//!
//! Each playable `(chunk, channel)` pair becomes one output track, ordered by
//! chunk and then channel. A channel is playable only when a note-on has a
//! later same-key note-off in that channel. Track zero supplies tempo, timing,
//! and loop events, and only the first output track receives tempo. These rules
//! reproduce `midi.cpp:434-490,916-964` without its seek-based parsing.
//!
//! Exact gates and the default clock scale are required because [`SongEvent`]
//! has no clock-scale metadata. Notes and tie ends keep every operand explicit;
//! pattern compression and operand elision do not change musical behavior and
//! are not represented. `MEMACC` controllers fail closed because their branch
//! state is not represented either.
//!
//! [`SongEvent::Wait`] stores a free `u8` tick count rather than upstream's
//! restricted `Wnn` opcode set. The compiler therefore preserves total delays
//! without reproducing ordinary wait chunking. It does reproduce the one
//! observable exception: CC `0x1E` omits its own first wait chunk after
//! upstream inserts whole-note timing marks and off-grid splits
//! (`midi.cpp:653-730`, `agb.cpp:349-405`).
//!
//! Same-tick items use upstream's semantic type order, with note key as the
//! note and tie-end tiebreaker (`midi.cpp:565-598`). A stable sort preserves
//! source order when those keys are equal.

use super::cfg::MidiCfgEntry;
use super::error::MidiError;
use super::event::SongEvent;
use super::parse::{self, RawEvent};
use super::reader;
use super::translate::{
    bpm_from_microseconds, centre_relative, convert_ticks, scale_volume, translate_controller,
    ControllerEvent,
};
use super::velocity;

const MIDI_CHANNEL_COUNT: u8 = 16;
const MAX_UNTIED_NOTE_TICKS: u32 = 96;
const DEFAULT_WHOLE_NOTE_TICKS: u32 = 96;
const FULL_MIDI_VALUE: u8 = 127;

#[derive(Debug, Clone)]
enum ItemKind {
    EndOfTie { key: u8 },
    SilentLabel,
    LoopEnd,
    LoopEndBegin,
    LoopBegin,
    Tempo(u16),
    TimingGridChange(u32),
    ProgramChange(u8),
    Command(SongEvent),
    ExtendedCommandSelector,
    SilentController,
    PitchBend(i8),
    Note { key: u8, velocity: u8, gate: u8 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ItemPriority {
    EndOfTie,
    SilentLabel,
    LoopEnd,
    LoopEndBegin,
    LoopBegin,
    TimingGridChange,
    Tempo,
    ProgramChange,
    Controller,
    PitchBend,
    Note,
}

impl ItemKind {
    fn sort_key(&self) -> (ItemPriority, u8) {
        match *self {
            Self::EndOfTie { key } => (ItemPriority::EndOfTie, key),
            Self::SilentLabel => (ItemPriority::SilentLabel, 0),
            Self::LoopEnd => (ItemPriority::LoopEnd, 0),
            Self::LoopEndBegin => (ItemPriority::LoopEndBegin, 0),
            Self::LoopBegin => (ItemPriority::LoopBegin, 0),
            Self::TimingGridChange(_) => (ItemPriority::TimingGridChange, 0),
            Self::Tempo(_) => (ItemPriority::Tempo, 0),
            Self::ProgramChange(_) => (ItemPriority::ProgramChange, 0),
            Self::Command(_) | Self::ExtendedCommandSelector | Self::SilentController => {
                (ItemPriority::Controller, 0)
            }
            Self::PitchBend(_) => (ItemPriority::PitchBend, 0),
            Self::Note { key, .. } => (ItemPriority::Note, key),
        }
    }
}

/// Song metadata and tracks ready for asset encoding.
#[derive(Debug)]
pub(super) struct CompiledSong {
    pub(super) voicegroup_label: String,
    pub(super) priority: u8,
    pub(super) reverb: Option<u8>,
    pub(super) tracks: Vec<Vec<SongEvent>>,
}

fn find_note_end(
    channel_events: &[(u32, RawEvent)],
    error_channel: u8,
    key: u8,
) -> Result<u32, MidiError> {
    channel_events
        .iter()
        .find_map(|&(t, e)| matches!(e, RawEvent::NoteOff { key: k, .. } if k == key).then_some(t))
        .ok_or(MidiError::UnterminatedNote {
            channel: error_channel,
            key,
        })
}

fn channel_is_playable(channel_events: &[(u32, RawEvent)], channel: u8) -> Result<bool, MidiError> {
    for index in 0..channel_events.len() {
        let (time, event) = channel_events[index];
        let RawEvent::NoteOn { key, .. } = event else {
            continue;
        };
        let off_time = find_note_end(&channel_events[index + 1..], channel, key)?;
        if off_time > time {
            return Ok(true);
        }
    }
    Ok(false)
}

fn push_notes(
    items: &mut Vec<(u32, ItemKind)>,
    channel_events: &[(u32, RawEvent)],
    channel: u8,
    division: u16,
) -> Result<(), MidiError> {
    for index in 0..channel_events.len() {
        let (time, event) = channel_events[index];
        let RawEvent::NoteOn {
            key,
            velocity: raw_velocity,
            ..
        } = event
        else {
            continue;
        };
        let off_time = find_note_end(&channel_events[index + 1..], channel, key)?;
        let raw_duration = off_time.checked_sub(time).ok_or(MidiError::Truncated)?;

        let start = convert_ticks(time, division)?;
        let duration = convert_ticks(raw_duration, division)?.max(1);
        let velocity = velocity::NOTE_VELOCITY_LUT[usize::from(raw_velocity)];

        if duration > MAX_UNTIED_NOTE_TICKS {
            items.push((
                start,
                ItemKind::Note {
                    key,
                    velocity,
                    gate: 0,
                },
            ));
            let end = start
                .checked_add(duration)
                .ok_or(MidiError::TickOverflow(off_time))?;
            items.push((end, ItemKind::EndOfTie { key }));
        } else {
            let gate = u8::try_from(duration).expect("untied note duration fits in u8");
            items.push((
                start,
                ItemKind::Note {
                    key,
                    velocity,
                    gate,
                },
            ));
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn compile_track(
    channel: u8,
    channel_events: &[(u32, RawEvent)],
    seq_events: &[(u32, RawEvent)],
    include_tempo: bool,
    division: u16,
    master_volume: u8,
    final_boundary: u32,
) -> Result<Vec<SongEvent>, MidiError> {
    let mut items: Vec<(u32, ItemKind)> = Vec::new();

    push_notes(&mut items, channel_events, channel, division)?;

    let mut extended_command: Option<u8> = None;
    for &(time, event) in channel_events {
        match event {
            RawEvent::ProgramChange { program, .. } => {
                let converted = convert_ticks(time, division)?;
                items.push((converted, ItemKind::ProgramChange(program)));
            }
            RawEvent::PitchBend { msb, .. } => {
                let converted = convert_ticks(time, division)?;
                items.push((converted, ItemKind::PitchBend(centre_relative(msb))));
            }
            RawEvent::Controller {
                controller, value, ..
            } => {
                match translate_controller(controller, value, master_volume, &mut extended_command)?
                {
                    ControllerEvent::Event(song_event) => {
                        let converted = convert_ticks(time, division)?;
                        items.push((converted, ItemKind::Command(song_event)));
                    }
                    ControllerEvent::ExtendedCommandSelect => {
                        let converted = convert_ticks(time, division)?;
                        items.push((converted, ItemKind::ExtendedCommandSelector));
                    }
                    ControllerEvent::None => {
                        let converted = convert_ticks(time, division)?;
                        items.push((converted, ItemKind::SilentController));
                    }
                }
            }
            _ => {}
        }
    }

    for &(time, event) in seq_events {
        let converted = convert_ticks(time, division)?;
        match event {
            RawEvent::Tempo(microseconds) => {
                if include_tempo {
                    items.push((
                        converted,
                        ItemKind::Tempo(bpm_from_microseconds(microseconds)?),
                    ));
                }
            }
            RawEvent::TimeSignature {
                numerator,
                denominator_exponent,
            } => {
                let period =
                    (DEFAULT_WHOLE_NOTE_TICKS * u32::from(numerator)) >> denominator_exponent;
                if period == 0 {
                    return Err(MidiError::ZeroTimeSignature);
                }
                items.push((converted, ItemKind::TimingGridChange(period)));
            }
            RawEvent::LoopBegin => items.push((converted, ItemKind::LoopBegin)),
            RawEvent::LoopEndBegin => items.push((converted, ItemKind::LoopEndBegin)),
            RawEvent::LoopEnd => items.push((converted, ItemKind::LoopEnd)),
            RawEvent::Label => items.push((converted, ItemKind::SilentLabel)),
            _ => {}
        }
    }

    items.sort_by_key(|(time, kind)| {
        let (primary, secondary) = kind.sort_key();
        (*time, primary, secondary)
    });

    emit_track(&items, master_volume, final_boundary)
}

/// The distinct `Wnn` durations represented by `g_noteDurationLUT`
/// (`tables.cpp:23-122`), in ascending order.
const ENCODABLE_WAIT_TICKS: &[u8] = &[
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 28,
    30, 32, 36, 40, 42, 44, 48, 52, 54, 56, 60, 64, 66, 68, 72, 76, 78, 80, 84, 88, 90, 92, 96,
];

fn first_encodable_wait_chunk(gap: u32) -> u32 {
    if gap > DEFAULT_WHOLE_NOTE_TICKS {
        return DEFAULT_WHOLE_NOTE_TICKS;
    }

    ENCODABLE_WAIT_TICKS
        .iter()
        .rev()
        .copied()
        .map(u32::from)
        .find(|&ticks| ticks <= gap)
        .expect("encodable waits include zero")
}

fn push_wait(out: &mut Vec<SongEvent>, ticks: u32) {
    let mut remaining = ticks;
    while remaining > u32::from(u8::MAX) {
        out.push(SongEvent::Wait(u8::MAX));
        remaining -= u32::from(u8::MAX);
    }
    if remaining > 0 {
        let ticks = u8::try_from(remaining).expect("remaining wait fits in u8");
        out.push(SongEvent::Wait(ticks));
    }
}

struct TimingGrid {
    period: u32,
    next_mark: u32,
}

impl TimingGrid {
    fn new() -> Self {
        Self {
            period: DEFAULT_WHOLE_NOTE_TICKS,
            next_mark: 0,
        }
    }

    fn update_for_item(&mut self, time: u32, kind: &ItemKind) -> Result<(), MidiError> {
        if let ItemKind::TimingGridChange(period) = kind {
            self.period = *period;
            self.next_mark = time
                .checked_add(*period)
                .ok_or(MidiError::TickOverflow(time))?;
        } else if self.next_mark <= time {
            let elapsed_periods = (time - self.next_mark) / self.period + 1;
            self.next_mark = elapsed_periods
                .checked_mul(self.period)
                .and_then(|elapsed| self.next_mark.checked_add(elapsed))
                .ok_or(MidiError::TickOverflow(time))?;
        }
        Ok(())
    }

    fn ticks_until_next_mark(&self, time: u32) -> u32 {
        self.next_mark
            .checked_sub(time)
            .expect("timing grid advances beyond each item")
    }
}

fn output_index(out: &[SongEvent]) -> u32 {
    u32::try_from(out.len()).expect("track event count fits in u32")
}

fn emit_track(
    items: &[(u32, ItemKind)],
    master_volume: u8,
    final_boundary: u32,
) -> Result<Vec<SongEvent>, MidiError> {
    let mut out = Vec::new();

    let has_initial_volume = items
        .iter()
        .find_map(|(_, kind)| match kind {
            ItemKind::Note { .. } => Some(false),
            ItemKind::Command(SongEvent::Volume(_)) => Some(true),
            _ => None,
        })
        .unwrap_or(false);
    if !has_initial_volume {
        out.push(SongEvent::Volume(scale_volume(
            FULL_MIDI_VALUE,
            master_volume,
        )));
    }

    let initial_wait = items.first().map_or(0, |&(t, _)| t);
    push_wait(&mut out, initial_wait);
    out.push(SongEvent::KeyShift(0));

    let mut loop_target: Option<u32> = None;
    let mut timing_grid = TimingGrid::new();
    for (index, (time, kind)) in items.iter().enumerate() {
        timing_grid.update_for_item(*time, kind)?;
        match kind {
            ItemKind::EndOfTie { key } => out.push(SongEvent::EndOfTie { key: *key }),
            ItemKind::SilentLabel
            | ItemKind::ExtendedCommandSelector
            | ItemKind::SilentController
            | ItemKind::TimingGridChange(_) => {}
            ItemKind::LoopBegin => {
                loop_target = Some(output_index(&out));
            }
            ItemKind::LoopEndBegin => {
                let target = loop_target.ok_or(MidiError::DanglingLoopEnd)?;
                out.push(SongEvent::Goto(target));
                loop_target = Some(output_index(&out));
            }
            ItemKind::LoopEnd => {
                let target = loop_target.ok_or(MidiError::DanglingLoopEnd)?;
                out.push(SongEvent::Goto(target));
            }
            ItemKind::Tempo(bpm) => out.push(SongEvent::Tempo(*bpm)),
            ItemKind::ProgramChange(program) => out.push(SongEvent::Voice(*program)),
            ItemKind::Command(event) => out.push(event.clone()),
            ItemKind::PitchBend(bend) => out.push(SongEvent::Bend(*bend)),
            ItemKind::Note {
                key,
                velocity,
                gate,
            } => {
                out.push(SongEvent::Note {
                    key: *key,
                    velocity: *velocity,
                    gate: *gate,
                });
            }
        }
        let next_time = items.get(index + 1).map_or(final_boundary, |&(t, _)| t);
        let gap = next_time.checked_sub(*time).ok_or(MidiError::Truncated)?;
        if matches!(kind, ItemKind::ExtendedCommandSelector) {
            let bounded_gap = gap.min(timing_grid.ticks_until_next_mark(*time));
            push_wait(&mut out, gap - first_encodable_wait_chunk(bounded_gap));
        } else {
            push_wait(&mut out, gap);
        }
    }

    out.push(SongEvent::Fine);
    Ok(out)
}

/// Compiles a MIDI file according to its extraction configuration.
///
/// # Errors
///
/// Returns [`MidiError`] when the file is malformed or requires an unsupported
/// compilation feature.
pub(super) fn compile(midi_bytes: &[u8], cfg: &MidiCfgEntry) -> Result<CompiledSong, MidiError> {
    if !cfg.exact_gate_time {
        return Err(MidiError::NonExactGateTime);
    }
    if cfg.clocks_per_beat != 1 {
        return Err(MidiError::UnsupportedClocksPerBeat(cfg.clocks_per_beat));
    }

    let header = reader::read_header(midi_bytes)?;
    if header.division == 0 {
        return Err(MidiError::ZeroTimeDivision);
    }
    let track_slices = reader::split_tracks(midi_bytes, header.track_count)?;

    let sequence_track = parse::parse_track(track_slices[0])?;
    let sequence_events: Vec<(u32, RawEvent)> = sequence_track
        .events
        .iter()
        .copied()
        .filter(|(_, e)| {
            matches!(
                e,
                RawEvent::Tempo(_)
                    | RawEvent::LoopBegin
                    | RawEvent::LoopEndBegin
                    | RawEvent::LoopEnd
                    | RawEvent::Label
                    | RawEvent::TimeSignature { .. }
            )
        })
        .collect();

    let mut tracks = Vec::new();
    let mut include_tempo = true;
    for track_data in &track_slices {
        let parsed = parse::parse_track(track_data)?;
        for channel in 0..MIDI_CHANNEL_COUNT {
            let channel_events: Vec<(u32, RawEvent)> = parsed
                .events
                .iter()
                .copied()
                .filter(|(_, e)| e.channel() == Some(channel))
                .collect();
            if !channel_is_playable(&channel_events, channel)? {
                continue;
            }

            let final_boundary = convert_ticks(
                sequence_track.end_of_track.max(parsed.end_of_track),
                header.division,
            )?;
            let track = compile_track(
                channel,
                &channel_events,
                &sequence_events,
                include_tempo,
                header.division,
                cfg.master_volume,
                final_boundary,
            )?;
            tracks.push(track);
            include_tempo = false;
        }
    }

    Ok(CompiledSong {
        voicegroup_label: cfg.voicegroup_label.clone(),
        priority: cfg.priority,
        reverb: cfg.reverb,
        tracks,
    })
}

#[cfg(test)]
mod tests;
