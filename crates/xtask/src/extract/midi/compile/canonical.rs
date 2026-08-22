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
        if let SongEvent::Goto(target) = event {
            if let Some(&new) = usize::try_from(*target).ok().and_then(|old| map.get(old)) {
                *target =
                    u32::try_from(new).expect("a canonical track is no longer than its source");
            }
        }
    }
    out
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
