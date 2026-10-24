//! Match `crates/assets/src/audio/song/canonical.rs` without importing the assets canonicalizer.
//! A shared wait shape lets checkout and ROM backends encode identical packs.
//!
//! Adjacent `Wait`s merge, a rest over `255` ticks splits into `255`-tick chunks with the
//! remainder last, a zero rest vanishes unless a `Goto` targets it (then `Wait(0)` stays as
//! its anchor), and a run never merges across a `Goto` target. `Goto` targets are event
//! indices and follow the events they name.

use std::collections::BTreeSet;

use super::super::event::SongEvent;

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
        let run_is_targeted = targets.contains(&index);
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
        if run_is_targeted && out.len() == start {
            // Preserve an addressable event for jumps to an otherwise empty rest.
            out.push(SongEvent::Wait(0));
        }
        index = end;
    }
    map.push(out.len());

    for event in &mut out {
        if let SongEvent::Goto(target) = event {
            if let Some(&canonical_target) = usize::try_from(*target)
                .ok()
                .and_then(|source_target| map.get(source_target))
            {
                *target = u32::try_from(canonical_target)
                    .expect("a canonical track is no longer than its source");
            }
        }
    }
    out
}

/// Sum a run of adjacent `Wait` ticks and lazily split the total into canonical chunks.
///
/// The total is `u64`: a pack-valid track can hold up to `u32::MAX` waits,
/// whose combined ticks exceed `u32::MAX`.
fn wait_run_chunks(ticks: impl Iterator<Item = u8>) -> impl Iterator<Item = SongEvent> {
    let mut total: u64 = ticks.map(u64::from).sum();
    std::iter::from_fn(move || {
        if total > u64::from(u8::MAX) {
            total -= u64::from(u8::MAX);
            Some(SongEvent::Wait(u8::MAX))
        } else if total > 0 {
            #[expect(clippy::cast_possible_truncation, reason = "total <= u8::MAX here")]
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
    fn a_targeted_terminal_zero_rest_keeps_an_anchor() {
        assert_eq!(
            canonicalize_waits(&[SongEvent::Goto(1), SongEvent::Wait(0)]),
            vec![SongEvent::Goto(1), SongEvent::Wait(0)]
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
