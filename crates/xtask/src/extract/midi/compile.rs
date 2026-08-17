//! The compiler proper: turns [`parse::parse_track`]'s per-`MTrk` output
//! into one [`event::SongEvent`] stream per playable (MIDI-track,
//! MIDI-channel) pair, reproducing `tools/mid2agb`'s own interpretation —
//! `midi.cpp`'s `ReadMidiTracks`/`MergeEvents`/`ConvertTimes`/`CreateTies`
//! pipeline and `agb.cpp`'s `PrintAgbTrack` translation — without
//! reproducing the parts of that pipeline that only exist to serve a
//! *compiled-byte-code* concern this schema deliberately does not model.
//! Read this file's docs alongside `crates/assets::audio`'s "Deliberately
//! deferred" section, which this module's choices are downstream of.
//!
//! # Pipeline
//!
//! 1. [`compile`] reads `MThd`, splits every `MTrk` chunk
//!    ([`super::reader`]), and parses each one once ([`super::parse`]).
//! 2. Track 0's parse is filtered down to "sequence events" — tempo plus
//!    the four loop-marker text events — matching `ReadSeqEvent`'s own
//!    narrower recognition (`midi.cpp:253-348`; `ReadTrackEvent` recognizes
//!    a disjoint event set, so one parse serves both purposes — see
//!    `super::parse`'s module docs, "One pass, not sixteen").
//! 3. For every `(MTrk chunk, MIDI channel)` pair that carries at least one
//!    *playable* note-on ([`channel_is_playable`]), [`compile_track`] builds
//!    that channel's own [`event::SongEvent`] stream, reproducing
//!    `midi.cpp:916-964`'s nested loop order exactly (`MTrk` chunk outer,
//!    ascending; channel inner, ascending `0..16`) so output track order
//!    matches upstream's `g_agbTrack` numbering.
//!
//! # Which channels become tracks
//!
//! Upstream's gate is `s_minNote != 0xFF` (`midi.cpp:933`), and `s_minNote`
//! is only ever touched inside `ReadTrackEvents`'s note-on arm *under*
//! `if (event.param2 > 0)` (`:484-490`) — `param2` there being the raw MIDI
//! tick duration `FindNoteEnd` has just computed, before any scaling. A
//! channel is therefore playable to upstream when it carries at least one
//! note-on whose note-off lands strictly later than it; a channel whose
//! every note-on is cancelled at the same tick emits no track at all.
//! [`channel_is_playable`] reproduces that gate exactly, rather than the
//! looser "any note-on at all" test an earlier revision of this file used.
//! The difference is unobservable for `mus_title.mid` (10 tracks either
//! way), but it is not cosmetic: an extra emitted track would also shift
//! which track counts as the first one, and so which track receives the
//! song's `Tempo` (`include_tempo`, [`compile_track`]).
//!
//! # Note pairing, not upstream's seek-and-rewind
//!
//! `midi.cpp:434-448`'s `FindNoteEnd` computes a note's duration by saving
//! the file position, scanning forward byte-by-byte for the next matching
//! note-off/note-on-velocity-`0` on the same channel and key, then seeking
//! back. [`find_note_end`] does the equivalent forward search over the
//! already-parsed, already-channel-filtered event list instead — same
//! result (first matching note-off after this note-on, ignoring everything
//! else in between, including a same-key retrigger), no seek needed because
//! nothing here is a stream (`no-verbatim`).
//!
//! # Tick scaling and gate length: exact, not LUT-quantized
//!
//! `midi.cpp:646`: `if (!g_exactGateTime && duration < 96) duration =
//! g_noteDurationLUT[duration];` — the note-duration quantization table
//! (`tables.cpp:23-122`) only ever fires when `-E` is *absent*.
//! `mus_title.mid`'s own `midi.cfg` entry always carries `-E`
//! (`sound/songs/midi/midi.cfg:186`), so this compiler requires it
//! ([`super::error::MidiError::NonExactGateTime`] otherwise) and never
//! implements that table at all — an honest, title-scoped cut, not an
//! oversight: a future compiler slice for a song whose `midi.cfg` entry
//! omits `-E` would need to add it. The same reasoning applies to
//! `-X`/48-clocks-per-beat ([`super::error::MidiError::UnsupportedClocksPerBeat`]):
//! this schema carries no "ticks per quarter note" field a consumer could
//! use to tell a `-X` song's ticks apart from a default song's, so
//! supporting it here would produce ticks a generic consumer can't
//! correctly interpret — a schema gap for a later slice to close, not
//! something this compiler should paper over by guessing.
//!
//! With `-E` required, `agb.cpp:150-158`'s `gtp`
//! (exact-gate-time-parameter) split — which re-derives the *compiled
//! opcode's* coarse duration via the same LUT and stores only the
//! remainder as a second operand — nets out to the exact original tick
//! count once decoded (`ply_note`, `src/m4a_1.s:1538-1576`, adds the two
//! back together): [`event::SongEvent::Note::gate`] is simply that exact
//! value, with no LUT involved on this compiler's side either.
//!
//! # `Wait` is a free tick count, not a 49-value enum
//!
//! `sound/MPlayDef.s:1-49` only defines 49 distinct `WAIT`/`Wnn` opcodes
//! (`0`, `1..=24`, then a coarser tail up to `96`), so `tools/mid2agb`'s
//! `SplitTime` (`midi.cpp:689-730`) — which also drives its whole-note
//! (`WholeNoteMark`) bookkeeping for the pattern-compression pass this
//! schema doesn't model — exists to break any longer or off-grid rest into
//! pieces that opcode set can represent. None of that is a musical
//! constraint: MP2K's `WAIT` deltas are purely additive (no side effects
//! between them), so splitting one 48-tick rest into `W30`+`W18` instead of
//! one `W48` is inaudible. [`event::SongEvent::Wait`] carries a plain `u8`
//! tick count with no such enum to respect, so [`push_wait`] only splits a
//! gap when it doesn't fit in a `u8` (`> 255` ticks) — a wire-format
//! necessity of *this* schema, unrelated to upstream's 49-value opcode set.
//!
//! # No operand elision
//!
//! `tools/mid2agb`'s compression pass (`g_compressionEnabled`, on by
//! default and not disabled by `-N` in `mus_title.mid`'s own entry) elides
//! repeated operands for on-disk size — most visibly,
//! `agb.cpp:222-244`'s `PrintEndOfTieOp` omits `EOT`'s key operand
//! entirely when it matches the immediately preceding note (the engine then
//! matches whichever key the channel currently has sounding). This is a
//! wire-size optimization, not new musical content — the same category as
//! `PATT`/`PEND` (`crates/assets::audio`'s "Deliberately deferred" docs) —
//! so this compiler always writes an explicit key
//! ([`event::SongEvent::EndOfTie`]'s docs), and likewise never elides a
//! `Note`'s key/velocity. Every `Note`/`EndOfTie` this compiler emits is
//! therefore fully self-describing, at the cost of not byte-matching a
//! compressed compile — the same trade-off this schema already makes by not
//! modelling `PATT`/`PEND` at all.
//!
//! # `MEMACC` controllers: unsupported, not silently dropped
//!
//! Controllers `0x0C`/`0x0D`/`0x0E`/`0x0F`/`0x10`/`0x11` — `MEMACC`'s
//! set/branch-op/address/data operands and its `_L<n>:` branch-target label
//! (`agb.cpp:254-329`'s `PrintMemAcc`, `:366-382`'s state-setting cases)
//! — are the one controller family this compiler refuses outright
//! ([`super::error::MidiError::UnsupportedMemAccController`]) rather than
//! translating: `mus_title.mid` carries zero of them (confirmed against a
//! locally built `tools/mid2agb` oracle — the compiled reference has no
//! `MEMACC` byte anywhere), and the schema's one canonical `MEMACC` user,
//! `mus_vs_trainer`, is a different song outside this issue's scope
//! (`crates/assets::audio`'s "Deliberately deferred" docs already model
//! [`event::SongEvent`]'s wire shape for it — `super::event`'s module docs).
//! Failing closed here, instead of adding untested branch-resolution
//! machinery no real input in this slice exercises, is the honest choice;
//! a future slice compiling `mus_vs_trainer` is what should teach this
//! compiler `MEMACC`, informed by a real test case.
//!
//! # Extended commands (`XCMD`): both directions modelled
//!
//! Unlike `MEMACC`, `XCMD` (`agb.cpp:331-347`'s `PrintExtendedOp`) *is*
//! exercised by `mus_title.mid` (three `xIECV` occurrences in the oracle),
//! so both of its recognized sub-commands are modelled
//! ([`event::SongEvent::PseudoEchoVolume`]/[`event::SongEvent::PseudoEchoLength`]);
//! any other extended-command value is a silent no-op, matching upstream's
//! own `default: PrintWait(event.time); break;` (`agb.cpp:343-345`,
//! its own `// TODO: support for other extended commands` comment) — this
//! is upstream's *actual*, exercised behaviour, not a scope cut.
//!
//! # Track preamble order: volume, wait, `KEYSH`
//!
//! [`emit_track`] opens every track with the optional synthetic
//! full-volume byte, then the initial wait, then `KeyShift(0)` — which is
//! `PrintAgbTrack`'s own order: `agb.cpp:441-442`'s conditional
//! `PrintByte("VOL ...")`, then `:444`'s `PrintWait(g_initialWait)`, then
//! `:445`'s `PrintByte("KEYSH ...")`. Reviewed against upstream directly
//! (the wait precedes `KEYSH` there too, and `g_initialWait` is
//! `events[0].time` captured before `CalculateWaits` rewrites each event's
//! time into a gap — `midi.cpp:758-763`), so this is a match, not a latent
//! divergence. It is called out because the ordering *looks* arbitrary: a
//! `KEYSH` is a state write with no duration, so hoisting it ahead of the
//! initial rest would be inaudible, and a reader comparing the two files
//! could reasonably wonder which way round upstream had it. It is this way
//! round.
//!
//! # Sort order
//!
//! [`ItemKind::sort_key`] reproduces `midi.cpp:565-598`'s `EventCompare`
//! exactly enough to matter: `MergeEvents`'s two-pointer merge
//! (`midi.cpp:600-629`) only ever breaks a tie between two events of the
//! *same* concrete type (channel-voice events never collide with the
//! disjoint seq-event types), so concatenating this track's items in any
//! order and running one stable
//! sort by `(time, type-priority, note-for-EndOfTie)` — Rust's `sort_by_key`
//! preserves original relative order for equal keys — reproduces the exact
//! same final order upstream's merge-then-two-sorts (`MergeEvents`, then
//! `std::stable_sort` again after `CreateTies` inserts new `EndOfTie`
//! events) produces, without literally replicating either pass.

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

/// One MIDI-channel's worth of normalized events, tagged by its sort
/// priority relative to `midi.cpp:32-51`'s `EventType` — see the module
/// docs, "Sort order".
#[derive(Debug, Clone)]
enum ItemKind {
    EndOfTie {
        key: u8,
    },
    Label,
    LoopEnd,
    LoopEndBegin,
    LoopBegin,
    Tempo(u16),
    ProgramChange(u8),
    Command(SongEvent),
    /// CC `0x1E`'s own timing-only marker — no [`SongEvent`] is emitted for
    /// it, but [`emit_track`] needs it recorded so it can drop the wait
    /// that follows, reproducing upstream's missing `PrintWait(event.time)`
    /// (`super::translate`'s module docs, "Reproduced: the dropped wait
    /// after CC `0x1E`").
    ExtendedCommandSelect,
    PitchBend(i8),
    Note {
        key: u8,
        velocity: u8,
        gate: u8,
    },
}

impl ItemKind {
    fn sort_key(&self) -> (u32, u32) {
        match *self {
            Self::EndOfTie { key } => (0x01, u32::from(key)),
            Self::Label => (0x11, 0),
            Self::LoopEnd => (0x12, 0),
            Self::LoopEndBegin => (0x13, 0),
            Self::LoopBegin => (0x14, 0),
            Self::Tempo(_) => (0x19, 0),
            Self::ProgramChange(_) => (0x21, 0),
            // Both are upstream's own `EventType::Controller` (`midi.h:45`),
            // so both share its sort priority.
            Self::Command(_) | Self::ExtendedCommandSelect => (0x22, 0),
            Self::PitchBend(_) => (0x23, 0),
            Self::Note { key, .. } => (0x40 + u32::from(key), 0),
        }
    }
}

/// A fully compiled song: `midi.cfg`'s song-level metadata plus one
/// [`event::SongEvent`] stream per playable track, in `g_agbTrack` order.
#[derive(Debug)]
pub(super) struct CompiledSong {
    pub(super) voicegroup_label: String,
    pub(super) priority: u8,
    pub(super) reverb: Option<u8>,
    pub(super) tracks: Vec<Vec<SongEvent>>,
}

/// Find the tick position of the note-off (or velocity-`0` note-on, already
/// folded into [`RawEvent::NoteOff`] by [`parse::parse_track`]) that ends
/// the note-on at `channel_events[from..][0]`'s own `key` — the first
/// matching entry after it, ignoring everything else (module docs, "Note
/// pairing").
///
/// `channel` is not part of the search — `channel_events` is already one
/// channel's slice — and is carried purely to label the error, so a
/// "note doesn't end" diagnostic names the offending channel the way
/// upstream's own `RaiseError` context does. That is deliberate, not a
/// leftover parameter.
///
/// # Errors
///
/// [`MidiError::UnterminatedNote`] if no match is found before the track's
/// end.
fn find_note_end(
    channel_events: &[(u32, RawEvent)],
    channel: u8,
    key: u8,
) -> Result<u32, MidiError> {
    channel_events
        .iter()
        .find_map(|&(t, e)| matches!(e, RawEvent::NoteOff { key: k, .. } if k == key).then_some(t))
        .ok_or(MidiError::UnterminatedNote { channel, key })
}

/// Whether this channel becomes one of the compiled song's tracks:
/// `true` when it carries at least one note-on with a strictly positive
/// *raw* tick duration, reproducing `midi.cpp:484-490`'s `if
/// (event.param2 > 0)` guard around `s_minNote` and `:933`'s
/// `if (s_minNote != 0xFF)` test on it (module docs, "Which channels become
/// tracks").
///
/// # Errors
///
/// [`MidiError::UnterminatedNote`] if a note-on on this channel never ends
/// — the same failure [`push_notes`] would raise, surfaced here because
/// upstream's `FindNoteEnd` likewise runs (and can `RaiseError`) for every
/// note-on before the gate is consulted.
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

/// Pair every note-on in `channel_events` with its ending tick, apply the
/// velocity quantization table, and tie-split any duration over 96 ticks
/// (`midi.cpp:738-749`'s `CreateTies`) — pushing the resulting `Note`/
/// `EndOfTie` items onto `items`.
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
        let mut duration = convert_ticks(raw_duration, division)?;
        if duration == 0 {
            duration = 1; // midi.cpp:643-644
        }
        let velocity = velocity::NOTE_VELOCITY_LUT[usize::from(raw_velocity)];

        if duration > 96 {
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
            #[allow(clippy::cast_possible_truncation)] // duration <= 96 here
            let gate = duration as u8;
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

/// Compile one `(MTrk chunk, MIDI channel)` pair's events into a single
/// track's [`SongEvent`] stream. `include_tempo` is `true` only for the
/// very first track any real `MTrk` chunk produces (`midi.cpp:941-946`
/// strips `Tempo` from the shared sequence-event list right after the
/// first track consumes it).
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
                        items.push((converted, ItemKind::ExtendedCommandSelect));
                    }
                    ControllerEvent::None => {}
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
            RawEvent::LoopBegin => items.push((converted, ItemKind::LoopBegin)),
            RawEvent::LoopEndBegin => items.push((converted, ItemKind::LoopEndBegin)),
            RawEvent::LoopEnd => items.push((converted, ItemKind::LoopEnd)),
            RawEvent::Label => items.push((converted, ItemKind::Label)),
            _ => {}
        }
    }

    items.sort_by_key(|(time, kind)| {
        let (primary, secondary) = kind.sort_key();
        (*time, primary, secondary)
    });

    emit_track(&items, master_volume, final_boundary)
}

/// Append `ticks`' worth of [`SongEvent::Wait`], splitting on `u8::MAX`
/// only because [`SongEvent::Wait`] carries a plain `u8` — a wire
/// constraint of *this* schema, not upstream's 49-value opcode set (module
/// docs).
fn push_wait(out: &mut Vec<SongEvent>, ticks: u32) {
    let mut remaining = ticks;
    while remaining > u32::from(u8::MAX) {
        out.push(SongEvent::Wait(u8::MAX));
        remaining -= u32::from(u8::MAX);
    }
    if remaining > 0 {
        #[allow(clippy::cast_possible_truncation)] // remaining <= u8::MAX here
        let ticks = remaining as u8;
        out.push(SongEvent::Wait(ticks));
    }
}

/// Turn a track's sorted items into the final [`SongEvent`] stream:
/// optional synthetic full-volume preamble, initial wait, `KeyShift(0)`,
/// then each item's own command followed by the wait until the next one (or
/// until `final_boundary` for the last item) — `agb.cpp:417-525`'s
/// `PrintAgbTrack`, ending with `Fine`.
///
/// # Errors
///
/// [`MidiError::DanglingLoopEnd`] if a `LoopEnd`/`LoopEndBegin` item has no
/// preceding `LoopBegin`/`LoopEndBegin`.
fn emit_track(
    items: &[(u32, ItemKind)],
    master_volume: u8,
    final_boundary: u32,
) -> Result<Vec<SongEvent>, MidiError> {
    let mut out = Vec::new();

    // agb.cpp:427-442: scan for the first Note or CC7 (volume); a CC7
    // found first means the track's own content sets an explicit starting
    // volume, so no synthetic full-volume command is needed.
    let found_volume_before_note = items
        .iter()
        .find_map(|(_, kind)| match kind {
            ItemKind::Note { .. } => Some(false),
            ItemKind::Command(SongEvent::Volume(_)) => Some(true),
            _ => None,
        })
        .unwrap_or(false);
    if !found_volume_before_note {
        out.push(SongEvent::Volume(scale_volume(127, master_volume)));
    }

    let initial_wait = items.first().map_or(0, |&(t, _)| t);
    push_wait(&mut out, initial_wait);
    out.push(SongEvent::KeyShift(0));

    let mut loop_target: Option<u32> = None;
    for (index, (time, kind)) in items.iter().enumerate() {
        match kind {
            ItemKind::EndOfTie { key } => out.push(SongEvent::EndOfTie { key: *key }),
            // Label emits nothing, matching upstream (module docs, "Sort
            // order"); ExtendedCommandSelect emits nothing either -- it is
            // purely a timing marker (its doc comment) -- but the two
            // arms are merged for clippy::match_same_arms, not because
            // they mean the same thing.
            ItemKind::Label | ItemKind::ExtendedCommandSelect => {}
            ItemKind::LoopBegin => {
                loop_target = Some(
                    u32::try_from(out.len()).expect("a track has far fewer than u32::MAX events"),
                );
            }
            ItemKind::LoopEndBegin => {
                let target = loop_target.ok_or(MidiError::DanglingLoopEnd)?;
                out.push(SongEvent::Goto(target));
                loop_target = Some(
                    u32::try_from(out.len()).expect("a track has far fewer than u32::MAX events"),
                );
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
        // agb.cpp:402-405: CC 0x1E's own arm is the one `PrintControllerOp`
        // case that `break`s without a trailing `PrintWait(event.time)`, so
        // the gap to the next event is dropped here too, not just the byte
        // (`super::translate`'s module docs, "Reproduced: the dropped wait
        // after CC `0x1E`").
        if !matches!(kind, ItemKind::ExtendedCommandSelect) {
            push_wait(&mut out, gap);
        }
    }

    out.push(SongEvent::Fine);
    Ok(out)
}

/// Compile a whole `.mid` file's bytes into every playable track's
/// [`SongEvent`] stream, per `cfg`'s compile flags — see the module docs
/// for the pipeline and every deliberate scope cut.
///
/// # Errors
///
/// See [`MidiError`]'s variants.
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

    let seq_track = parse::parse_track(track_slices[0])?;
    let seq_events: Vec<(u32, RawEvent)> = seq_track
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
            )
        })
        .collect();

    let mut tracks = Vec::new();
    let mut is_first_agb_track = true;
    for track_data in &track_slices {
        let parsed = parse::parse_track(track_data)?;
        for channel in 0..16u8 {
            let channel_events: Vec<(u32, RawEvent)> = parsed
                .events
                .iter()
                .copied()
                .filter(|(_, e)| e.channel() == Some(channel))
                .collect();
            // Upstream's own `s_minNote != 0xFF` gate, which only a note-on
            // of raw duration > 0 can clear (module docs, "Which channels
            // become tracks").
            if !channel_is_playable(&channel_events, channel)? {
                continue;
            }

            let final_boundary = convert_ticks(
                seq_track.end_of_track.max(parsed.end_of_track),
                header.division,
            )?;
            let track = compile_track(
                channel,
                &channel_events,
                &seq_events,
                is_first_agb_track,
                header.division,
                cfg.master_volume,
                final_boundary,
            )?;
            tracks.push(track);
            is_first_agb_track = false;
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
