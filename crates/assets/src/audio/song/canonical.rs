//! Wait canonicalization: the one rewrite [`super::Song::new`] applies to
//! every track it is handed.
//!
//! Both backends reach a track's rests by different roads. `tools/mid2agb`
//! breaks every rest into the 49 `Wnn` opcodes the engine defines and
//! chunks anything longer than a whole note (`W96 W04`); a ROM carries
//! exactly that. The checkout compiler never sees those opcodes and emits
//! one `Wait` per gap, but keeps a split wherever a silent controller sat
//! between two rests. Neither chunking is derivable from the other, and
//! neither is audible: `WAIT` deltas are purely additive. So the contract
//! fixes one shape and every backend lands on it:
//!
//! - Adjacent [`SongEvent::Wait`]s merge into one rest.
//! - A rest longer than `255` ticks splits into `255`-tick chunks with the
//!   remainder last.
//! - A zero-length rest emits nothing.
//! - A run never merges across a jump target: a [`SongEvent::Goto`] or
//!   [`SongEvent::MemAccBranch`] that lands between two rests keeps them
//!   apart, because merging would change how long the loop waits on
//!   re-entry.
//!
//! Jump targets are event indices, so they are remapped as the track
//! shrinks. `crates/xtask`'s MIDI compiler mirrors this rewrite on its own
//! private event type, the same way it mirrors the wire encoders; the two
//! are kept identical by hand.

use std::collections::BTreeSet;

use super::SongEvent;

/// Rewrite `track` into the canonical wait shape (module docs).
pub(super) fn canonicalize_waits(track: &[SongEvent]) -> Vec<SongEvent> {
    let targets: BTreeSet<usize> = track
        .iter()
        .filter_map(|event| match event {
            SongEvent::Goto(target) | SongEvent::MemAccBranch { target, .. } => {
                usize::try_from(*target).ok()
            }
            _ => None,
        })
        .collect();

    let mut out = Vec::with_capacity(track.len());
    // `map[old] == new` for every old index, plus one entry for "the end".
    let mut map = Vec::with_capacity(track.len() + 1);
    let mut index = 0;
    while index < track.len() {
        let SongEvent::Wait(first) = track[index] else {
            map.push(out.len());
            out.push(track[index].clone());
            index += 1;
            continue;
        };
        let start = out.len();
        let mut total = u32::from(first);
        let mut end = index + 1;
        while let Some(SongEvent::Wait(ticks)) = track.get(end) {
            if targets.contains(&end) {
                break;
            }
            total += u32::from(*ticks);
            end += 1;
        }
        map.extend(std::iter::repeat_n(start, end - index));
        while total > u32::from(u8::MAX) {
            out.push(SongEvent::Wait(u8::MAX));
            total -= u32::from(u8::MAX);
        }
        if total > 0 {
            #[allow(clippy::cast_possible_truncation, reason = "total <= u8::MAX here")]
            out.push(SongEvent::Wait(total as u8));
        }
        index = end;
    }
    map.push(out.len());

    for event in &mut out {
        if let SongEvent::Goto(target) | SongEvent::MemAccBranch { target, .. } = event {
            if let Some(&new) = usize::try_from(*target).ok().and_then(|old| map.get(old)) {
                *target =
                    u32::try_from(new).expect("a canonical track is no longer than its source");
            }
        }
    }
    out
}
