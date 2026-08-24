//! Wait canonicalization: this compiler's copy of
//! `crates/assets/src/audio/song/canonical.rs`, applied to every track
//! [`super::compile`] emits.
//!
//! Duplicated rather than imported for the same reason [`super::super::encode`]
//! duplicates the wire encoder: this crate never depends on `crates/assets`.
//! The rule is the pack contract's, stated there and mirrored here by hand:
//! adjacent `Wait`s merge, a rest over `255` ticks splits into `255`-tick
//! chunks with the remainder last, a zero rest vanishes, and a run never
//! merges across a `Goto` target. Targets are event indices and move with
//! the events they name.
//!
//! Why this compiler needs it at all: [`super::emit_track`] keeps a split
//! wherever a silent controller sat between two rests, and the ROM keeps
//! `tools/mid2agb`'s `W96 W04` chunking. Neither is musical, and the pack
//! holds one answer so both backends agree byte for byte.

use std::collections::BTreeSet;

use super::super::event::SongEvent;

/// Rewrite `track` into the canonical wait shape (module docs).
pub(super) fn canonicalize_waits(track: &[SongEvent]) -> Vec<SongEvent> {
    let targets: BTreeSet<usize> = track
        .iter()
        .filter_map(|event| match event {
            SongEvent::Goto(target) => usize::try_from(*target).ok(),
            _ => None,
        })
        .collect();

    let mut out = Vec::with_capacity(track.len());
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
        if let SongEvent::Goto(target) = event {
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
/// The running total is `u64`, not `u32`, for the same reason
/// `crates/assets::audio::song::canonical`'s copy of this function is: a
/// run of `u32::MAX` `Wait(255)`s (the per-track event cap the pack
/// contract documents) sums past `u32::MAX` well before the run ends. No
/// current producer's output gets close, but the accumulator has to hold
/// what the type promises.
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
    use super::super::super::event::SongEvent;
    use super::canonicalize_waits;

    #[test]
    fn adjacent_waits_merge_and_long_rests_split_greedily() {
        assert_eq!(
            canonicalize_waits(&[
                SongEvent::Wait(4),
                SongEvent::Wait(255),
                SongEvent::Wait(129),
                SongEvent::Fine,
            ]),
            vec![SongEvent::Wait(255), SongEvent::Wait(133), SongEvent::Fine]
        );
    }

    #[test]
    fn a_zero_rest_vanishes() {
        assert_eq!(
            canonicalize_waits(&[SongEvent::Wait(0), SongEvent::Fine]),
            vec![SongEvent::Fine]
        );
    }

    #[test]
    fn goto_targets_move_with_their_events_and_split_runs() {
        let track = vec![
            SongEvent::Wait(1),
            SongEvent::Wait(1),
            SongEvent::Voice(0),
            SongEvent::Wait(2),
            SongEvent::Wait(3),
            SongEvent::Goto(2),
            SongEvent::Goto(4),
        ];
        assert_eq!(
            canonicalize_waits(&track),
            vec![
                SongEvent::Wait(2),
                SongEvent::Voice(0),
                SongEvent::Wait(2),
                SongEvent::Wait(3),
                SongEvent::Goto(1),
                SongEvent::Goto(3),
            ]
        );
    }
}
