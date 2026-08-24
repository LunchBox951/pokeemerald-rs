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
        if !matches!(track[index], SongEvent::Wait(_)) {
            map.push(out.len());
            out.push(track[index].clone());
            index += 1;
            continue;
        }
        let start = out.len();
        // Find the run's extent first; the tick sum is a separate pass
        // below, wide enough that it cannot overflow no matter how long
        // the run gets.
        let mut end = index + 1;
        while let Some(SongEvent::Wait(_)) = track.get(end) {
            if targets.contains(&end) {
                break;
            }
            end += 1;
        }
        map.extend(std::iter::repeat_n(start, end - index));
        out.extend(wait_run_chunks(track[index..end].iter().map(|event| {
            let SongEvent::Wait(ticks) = event else {
                unreachable!("the scan above only ever advances across Wait events")
            };
            *ticks
        })));
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

/// Sum a run of adjacent `Wait` tick counts and lazily split the total into
/// canonical chunks: `255`-tick steps with the remainder last, nothing for a
/// zero total (module docs).
///
/// The running total is `u64`, not `u32`: [`super::Song::new`] documents
/// accepting a track of up to [`u32::MAX`] events, and a run that long, made
/// entirely of `Wait(255)`, sums past `u32::MAX` well before the run ends --
/// a `u32` accumulator would panic in a debug build and wrap in a release
/// one. No current producer's output gets close (rom-import caps a track at
/// `1 << 20` events and a tick at `96`), but the accumulator has to hold
/// what the type promises, not just what today's producers send it.
fn wait_run_chunks(ticks: impl Iterator<Item = u8>) -> impl Iterator<Item = SongEvent> {
    let mut total: u64 = ticks.map(u64::from).sum();
    std::iter::from_fn(move || {
        if total > u64::from(u8::MAX) {
            total -= u64::from(u8::MAX);
            Some(SongEvent::Wait(u8::MAX))
        } else if total > 0 {
            #[allow(clippy::cast_possible_truncation, reason = "total <= u8::MAX here")]
            let chunk = SongEvent::Wait(total as u8);
            total = 0;
            Some(chunk)
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{wait_run_chunks, SongEvent};

    /// [`super::super::Song::new`] documents accepting up to `u32::MAX`
    /// events per track; a track that is nothing but adjacent `Wait(255)`s
    /// pushes the run's accumulated total past `u32::MAX` well before a
    /// track could actually hold that many events -- `16_843_010` of them,
    /// each worth `255` ticks, already sums to `4_294_967_550`. Drives
    /// `wait_run_chunks` straight off a `u8` tick iterator rather than a
    /// materialized `Vec<SongEvent>`, so this stays a millisecond-scale
    /// check, not a multi-hundred-megabyte one. No current producer reaches
    /// this (rom-import caps a track at `1 << 20` events and a tick at
    /// `96`) -- this hardens the documented contract, not a live bug.
    #[test]
    fn a_run_past_u32_max_ticks_chunks_without_overflow() {
        let run_len: u64 = 16_843_010;
        let total = run_len * u64::from(u8::MAX);
        assert_eq!(total, 4_294_967_550);

        let count = usize::try_from(run_len).expect("fits usize on any real target");
        let mut produced = 0u64;
        for chunk in wait_run_chunks(std::iter::repeat_n(u8::MAX, count)) {
            assert_eq!(chunk, SongEvent::Wait(u8::MAX));
            produced += 1;
        }
        assert_eq!(produced, run_len);
    }
}
