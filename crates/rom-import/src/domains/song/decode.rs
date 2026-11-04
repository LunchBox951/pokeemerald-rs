//! Specializes bytecode paths without committing runtime MEMACC decisions.

use std::collections::BTreeMap;

use super::fold::fold_equivalent_contexts;
use super::{step, ImportError, RomReader, SongEvent, SongFault, Step, Track, CMD_VOICE, CMD_WAIT};
use crate::GbaPtr;

const DECODE_LIMIT: usize = 1 << 20;

// MPlayMain, ply_note, and ply_patt/ply_pend carry these fields across
// command-pointer changes (pokeemerald/src/m4a_1.s:832-884,1232-1244,1556-1574).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Context {
    pub(super) at: usize,
    pub(super) running_status: Option<u8>,
    pub(super) key: u8,
    pub(super) velocity: u8,
    pub(super) patterns: Vec<usize>,
}

pub(super) fn decode_track(
    reader: &RomReader<'_>,
    id: &'static str,
    index: usize,
    start: GbaPtr,
) -> Result<Vec<SongEvent>, ImportError> {
    decode_with_limit(reader, id, index, start, DECODE_LIMIT)
}

fn decode_with_limit(
    reader: &RomReader<'_>,
    id: &'static str,
    index: usize,
    start: GbaPtr,
    limit: usize,
) -> Result<Vec<SongEvent>, ImportError> {
    let mut track = Track {
        reader,
        id,
        index,
        events: Vec::new(),
        points: Vec::new(),
        command: (start.offset(), Vec::new()),
        tails: Vec::new(),
        state: Context {
            at: start.offset(),
            running_status: None,
            key: 0,
            velocity: 0,
            patterns: Vec::new(),
        },
    };
    let mut visited = BTreeMap::new();
    decode_reachable(&mut track, &mut visited, limit)?;
    decode_tails(&mut track, &mut visited, limit);
    Ok(fold_equivalent_contexts(&track.events, &track.points))
}

/// Everything execution can reach from the track's current state, every
/// conditional path included.
fn decode_reachable(
    track: &mut Track<'_, '_>,
    visited: &mut BTreeMap<Context, usize>,
    limit: usize,
) -> Result<(), ImportError> {
    let mut pending = Vec::new();
    loop {
        decode_path(track, visited, &mut pending, limit)?;
        loop {
            let Some((branch, state)) = pending.pop() else {
                return Ok(());
            };
            let existing = visited.get(&state).copied();
            let target = existing.unwrap_or(track.events.len());
            let SongEvent::MemAccBranch { target: slot, .. } = &mut track.events[branch] else {
                unreachable!("only conditional branches have pending destinations");
            };
            *slot = event_index(target);
            if existing.is_none() {
                track.state = state;
                break;
            }
        }
    }
}

/// The bytes after each executed `GOTO`, which execution never reaches.
///
/// `tools/mid2agb` writes the loop's trailing rests and `FINE` there and the
/// checkout compiler emits the same, so the pack keeps them. They are decoded
/// as far as they go; bytes that do not decode were never a command stream
/// and leave no events.
fn decode_tails(track: &mut Track<'_, '_>, visited: &mut BTreeMap<Context, usize>, limit: usize) {
    let mut next = 0;
    while let Some(tail) = track.tails.get(next).cloned() {
        next += 1;
        if visited.contains_key(&tail) {
            continue;
        }
        let events = track.events.len();
        let tails = track.tails.len();
        let seen = visited.clone();
        track.state = tail;
        if decode_reachable(track, visited, limit).is_err() {
            track.events.truncate(events);
            track.points.truncate(events);
            track.tails.truncate(tails);
            *visited = seen;
        }
    }
}

fn decode_path(
    track: &mut Track<'_, '_>,
    visited: &mut BTreeMap<Context, usize>,
    pending: &mut Vec<(usize, Context)>,
    limit: usize,
) -> Result<(), ImportError> {
    loop {
        if track.events.len() >= limit {
            return Err(track.fail(track.state.at, SongFault::DecodeLimit));
        }
        if let Some(&target) = visited.get(&track.state) {
            track.push(SongEvent::Goto(event_index(target)));
            return Ok(());
        }
        if visited.len() >= limit {
            return Err(track.fail(track.state.at, SongFault::DecodeLimit));
        }
        visited.insert(track.state.clone(), track.events.len());
        let at = track.state.at;
        track.command = (at, track.state.patterns.clone());
        let byte = track.reader.u8(at)?;
        let (cmd, operands) = if byte < CMD_WAIT {
            let status = track
                .state
                .running_status
                .ok_or_else(|| track.fail(at, SongFault::NoRunningStatus))?;
            (status, at)
        } else {
            if byte >= CMD_VOICE {
                track.state.running_status = Some(byte);
            }
            (byte, at + 1)
        };
        match step(track, cmd, operands)? {
            Step::Next(next) => track.state.at = next,
            Step::Fine => return Ok(()),
            Step::Branch { next, target } => {
                let mut destination = track.state.clone();
                destination.at = target;
                pending.push((track.events.len() - 1, destination));
                track.state.at = next;
            }
        }
    }
}

fn event_index(index: usize) -> u32 {
    u32::try_from(index).expect("the decoder state limit bounds output indices")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ROM_BASE;

    fn decode(bytes: &[u8], limit: usize) -> Result<Vec<SongEvent>, ImportError> {
        decode_with_limit(
            &RomReader::new(bytes),
            "audio/song/limit",
            0,
            GbaPtr::at(ROM_BASE),
            limit,
        )
    }

    /// `decode`d bytes at `ROM_BASE`, with `ptr(n)` the pointer to offset `n`.
    fn ptr(offset: u32) -> [u8; 4] {
        (ROM_BASE + offset).to_le_bytes()
    }

    fn note(key: u8) -> SongEvent {
        SongEvent::Note {
            key,
            velocity: 100,
            gate: 1,
        }
    }

    #[test]
    fn a_loops_second_lap_folds_back_when_it_never_reads_the_state_that_differs() {
        // VOICE 1, then the loop `VOL 5; N01 60 100; W01; GOTO loop; FINE`. The
        // second lap arrives with a note's running status and key the first
        // lacked, but the body sets both before reading them.
        let mut bytes = vec![0xBD, 1, 0xBE, 5, 0xD0, 60, 100, 0x81, 0xB2];
        bytes.extend(ptr(2));
        bytes.push(0xB1);
        assert_eq!(
            decode(&bytes, 64).unwrap(),
            vec![
                SongEvent::Voice(1),
                SongEvent::Volume(5),
                note(60),
                SongEvent::Wait(1),
                SongEvent::Goto(1),
                SongEvent::Fine,
            ]
        );
    }

    #[test]
    fn a_loops_second_lap_stays_apart_when_an_elided_operand_reads_the_state() {
        // The loop opens on a note with no key or velocity: the first lap plays
        // the engine's initial 0/0, the second the key the body ended on.
        let mut bytes = vec![0xBD, 1, 0xD0, 0xD0, 60, 100, 0x81, 0xB2];
        bytes.extend(ptr(2));
        bytes.push(0xB1);
        assert_eq!(
            decode(&bytes, 64).unwrap(),
            vec![
                SongEvent::Voice(1),
                SongEvent::Note {
                    key: 0,
                    velocity: 0,
                    gate: 1
                },
                note(60),
                SongEvent::Wait(1),
                note(60),
                SongEvent::Goto(2),
                SongEvent::Fine,
            ]
        );
    }

    #[test]
    fn a_repeated_phrase_at_another_address_is_not_folded_into_the_first() {
        // `VOL 5; N01 60 100; W01` twice, looping on the second: the two
        // phrases behave alike but are different commands.
        let mut bytes = vec![
            0xBE, 5, 0xD0, 60, 100, 0x81, 0xBE, 5, 0xD0, 60, 100, 0x81, 0xB2,
        ];
        bytes.extend(ptr(6));
        bytes.push(0xB1);
        assert_eq!(
            decode(&bytes, 64).unwrap(),
            vec![
                SongEvent::Volume(5),
                note(60),
                SongEvent::Wait(1),
                SongEvent::Volume(5),
                note(60),
                SongEvent::Wait(1),
                SongEvent::Goto(3),
                SongEvent::Fine,
            ]
        );
    }

    #[test]
    fn bytes_after_a_goto_that_are_not_commands_leave_no_events() {
        let mut bytes = vec![0x81, 0xB2];
        bytes.extend(ptr(0));
        // PORT, a named command this importer does not model.
        bytes.push(0xCC);
        assert_eq!(
            decode(&bytes, 64).unwrap(),
            vec![SongEvent::Wait(1), SongEvent::Goto(0)]
        );
    }

    #[test]
    fn a_control_only_cycle_closes_without_more_contexts() {
        let mut bytes = vec![0xB2];
        bytes.extend(ROM_BASE.to_le_bytes());
        assert_eq!(decode(&bytes, 1).unwrap(), vec![SongEvent::Goto(0)]);
    }

    #[test]
    fn a_pattern_return_cycle_closes_without_emitting_notes() {
        let mut bytes = vec![0xB3];
        bytes.extend((ROM_BASE + 10).to_le_bytes());
        bytes.push(0xB2);
        bytes.extend(ROM_BASE.to_le_bytes());
        bytes.push(0xB4);
        assert_eq!(decode(&bytes, 3).unwrap(), vec![SongEvent::Goto(0)]);
    }

    #[test]
    fn the_context_limit_counts_commands_without_output_events() {
        let err = decode(&[0xB4, 0xB4, 0xB1], 2).unwrap_err();
        assert!(matches!(
            err,
            ImportError::Song {
                at: 2,
                fault: SongFault::DecodeLimit,
                ..
            }
        ));
    }

    #[test]
    fn conditional_paths_share_the_decode_budget() {
        let mut bytes = vec![0xB9, 6, 0, 1];
        bytes.extend((ROM_BASE + 10).to_le_bytes());
        bytes.extend([0x81, 0xB1, 0x82, 0xB1]);
        let err = decode(&bytes, 3).unwrap_err();
        assert!(matches!(
            err,
            ImportError::Song {
                at: 10,
                fault: SongFault::DecodeLimit,
                ..
            }
        ));
        assert_eq!(decode(&bytes, 5).unwrap().len(), 5);
    }

    #[test]
    fn reconnecting_paths_share_the_output_event_budget() {
        let mut bytes = vec![0xBE, 5, 0xB9, 6, 0, 1];
        bytes.extend((ROM_BASE + 12).to_le_bytes());
        bytes.extend([0xBD, 1, 0xBE, 5, 0x81, 0xB1]);
        let err = decode(&bytes, 7).unwrap_err();
        assert!(matches!(
            err,
            ImportError::Song {
                at: 14,
                fault: SongFault::DecodeLimit,
                ..
            }
        ));
        // The budget counts the unfolded stream; the repeated `VOL` then folds away.
        assert_eq!(decode(&bytes, 8).unwrap().len(), 6);
    }
}
