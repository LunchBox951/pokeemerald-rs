//! Specializes bytecode paths without committing runtime MEMACC decisions.

use std::collections::BTreeMap;

use super::{step, ImportError, RomReader, SongEvent, SongFault, Step, Track, CMD_VOICE, CMD_WAIT};
use crate::GbaPtr;

const DECODE_LIMIT: usize = 1 << 20;

// MPlayMain, ply_note, and ply_patt/ply_pend carry these fields across
// command-pointer changes (pokeemerald/src/m4a_1.s:832-884,1232-1244,1556-1574).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Context {
    at: usize,
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
        state: Context {
            at: start.offset(),
            running_status: None,
            key: 0,
            velocity: 0,
            patterns: Vec::new(),
        },
    };
    let mut visited = BTreeMap::new();
    let mut pending = Vec::new();
    loop {
        decode_path(&mut track, &mut visited, &mut pending, limit)?;
        loop {
            let Some((branch, state)) = pending.pop() else {
                return Ok(track.events);
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
        assert_eq!(decode(&bytes, 8).unwrap().len(), 8);
    }
}
