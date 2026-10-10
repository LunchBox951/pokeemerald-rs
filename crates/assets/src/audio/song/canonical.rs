//! Wait canonicalization: the one rewrite [`super::Song::new`] applies to
//! every track, so ROM and checkout rests land on one shape. Contract:
//! `pokeemerald/sound/MPlayDef.s:1-49` (the `Wnn` wait opcodes).
//!
//! The xtask MIDI compiler mirrors this by hand in
//! `crates/xtask/src/extract/midi/compile/canonical.rs`; change both together.

use std::collections::BTreeSet;

use super::SongEvent;

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
    let mut source_to_canonical = Vec::with_capacity(track.len() + 1);
    let mut index = 0;
    while index < track.len() {
        if !matches!(track[index], SongEvent::Wait(_)) {
            source_to_canonical.push(out.len());
            out.push(track[index].clone());
            index += 1;
            continue;
        }
        let canonical_run_start = out.len();
        let run_start_is_jump_target = targets.contains(&index);
        let mut run_end = index + 1;
        while let Some(SongEvent::Wait(_)) = track.get(run_end) {
            if targets.contains(&run_end) {
                break;
            }
            run_end += 1;
        }
        source_to_canonical.extend(std::iter::repeat_n(canonical_run_start, run_end - index));
        out.extend(wait_run_chunks(track[index..run_end].iter().map(|event| {
            let SongEvent::Wait(ticks) = event else {
                unreachable!("the scan above only ever advances across Wait events")
            };
            *ticks
        })));
        if run_start_is_jump_target && out.len() == canonical_run_start {
            out.push(SongEvent::Wait(0));
        }
        index = run_end;
    }
    // The source-end index is also a valid jump target.
    source_to_canonical.push(out.len());

    for event in &mut out {
        if let SongEvent::Goto(target) | SongEvent::MemAccBranch { target, .. } = event {
            if let Some(&new) = usize::try_from(*target)
                .ok()
                .and_then(|old| source_to_canonical.get(old))
            {
                *target =
                    u32::try_from(new).expect("a canonical track is no longer than its source");
            }
        }
    }
    out
}

/// Chunk a run of adjacent waits as `255`-tick steps, remainder last.
fn wait_run_chunks(ticks: impl Iterator<Item = u8>) -> impl Iterator<Item = SongEvent> {
    // A run of up to `u32::MAX` events (`Song::new`) can sum past `u32::MAX`.
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
    use super::{wait_run_chunks, SongEvent};

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
