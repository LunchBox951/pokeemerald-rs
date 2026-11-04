//! Folds decoder contexts that behave the same back into one event stream.
//!
//! The decoder specializes a command by the running status, key and velocity
//! it runs under, because an elided operand reads them. A loop's second lap
//! usually arrives with different values from its first, though the loop
//! never reads them before setting them, and the unfolded stream then holds
//! the loop body twice. The checkout compiler emits the body once with a
//! `Goto` back to it, and so does this fold.
//!
//! Only events from one command (same address, same pattern return stack) are
//! ever merged, and only when everything that can follow them is identical,
//! so a repeated phrase at a different address stays repeated.

use std::collections::HashMap;

use assets::SongEvent;

/// Where an event's command sits: its address and the pattern return stack it
/// ran under.
pub(super) type Point = (usize, Vec<usize>);

/// One slot of the folded stream.
enum Item {
    Event(usize),
    Jump(usize),
}

/// `events` with every set of equivalent same-`point` events merged into the
/// first of the set.
///
/// `points` is aligned with `events`. Dropped events take their jumps with
/// them: a fall-through that no longer reaches its neighbour becomes an
/// explicit `Goto`, in the same shape as the checkout compiler's loop end.
pub(super) fn fold_equivalent_contexts(events: &[SongEvent], points: &[Point]) -> Vec<SongEvent> {
    let graph = Graph {
        events,
        identity: (0..events.len()).collect(),
    };
    let representative = graph.representatives(points);
    let landing: Vec<usize> = (0..events.len())
        .map(|i| graph.landing(&representative, i))
        .collect();

    let mut kept: Vec<usize> = (0..events.len())
        .filter(|&i| representative[i] == i && !matches!(events[i], SongEvent::Goto(_)))
        .chain(
            landing
                .iter()
                .copied()
                .filter(|&i| matches!(events[i], SongEvent::Goto(_))),
        )
        .collect();
    kept.sort_unstable();
    kept.dedup();

    let mut layout = Vec::with_capacity(kept.len());
    for (position, &event) in kept.iter().enumerate() {
        layout.push(Item::Event(event));
        let falls_through = !matches!(events[event], SongEvent::Fine | SongEvent::Goto(_));
        if let (true, Some(&next)) = (falls_through, landing.get(event + 1)) {
            if kept.get(position + 1) != Some(&next) {
                layout.push(Item::Jump(next));
            }
        }
    }

    let mut position = HashMap::new();
    for (index, item) in layout.iter().enumerate() {
        if let Item::Event(event) = item {
            position.insert(*event, index);
        }
    }
    let at = |original: usize| index(position[&landing[original]]);
    layout
        .into_iter()
        .map(|item| match item {
            Item::Jump(target) => SongEvent::Goto(index(position[&target])),
            Item::Event(event) => match &events[event] {
                SongEvent::Goto(target) => SongEvent::Goto(at(slot(*target))),
                SongEvent::MemAccBranch {
                    condition,
                    address,
                    data,
                    target,
                } => SongEvent::MemAccBranch {
                    condition: *condition,
                    address: *address,
                    data: *data,
                    target: at(slot(*target)),
                },
                other => other.clone(),
            },
        })
        .collect()
}

fn index(position: usize) -> u32 {
    u32::try_from(position).expect("a folded track is no longer than the decoded one")
}

fn slot(target: u32) -> usize {
    usize::try_from(target).expect("an event index fits in usize")
}

/// The decoded events as a control-flow graph.
struct Graph<'a> {
    events: &'a [SongEvent],
    /// Every event standing for itself.
    identity: Vec<usize>,
}

impl Graph<'_> {
    /// Where execution goes after event `i`, fall-through first.
    fn successors(&self, i: usize) -> Vec<usize> {
        match &self.events[i] {
            SongEvent::Fine => Vec::new(),
            SongEvent::Goto(target) => vec![slot(*target)],
            SongEvent::MemAccBranch { target, .. } => vec![i + 1, slot(*target)],
            _ => vec![i + 1],
        }
    }

    /// Skips the `Goto`s from `i` to the event that does something. A cycle of
    /// nothing but `Goto`s lands on its lowest member.
    fn landing(&self, representative: &[usize], i: usize) -> usize {
        let mut chain: Vec<usize> = Vec::new();
        let mut at = i;
        loop {
            at = representative[at];
            let SongEvent::Goto(target) = &self.events[at] else {
                return at;
            };
            if let Some(start) = chain.iter().position(|&seen| seen == at) {
                return chain[start..].iter().copied().min().unwrap_or(at);
            }
            chain.push(at);
            at = slot(*target);
        }
    }

    /// Each event's first behaviourally identical event of the same command.
    fn representatives(&self, points: &[Point]) -> Vec<usize> {
        let mut representative: Vec<usize> = (0..self.events.len()).collect();
        let mut by_point: HashMap<&Point, Vec<usize>> = HashMap::new();
        for (i, point) in points.iter().enumerate() {
            if !matches!(self.events[i], SongEvent::Goto(_)) {
                by_point.entry(point).or_default().push(i);
            }
        }
        for members in by_point.values().filter(|members| members.len() > 1) {
            let mut firsts: Vec<usize> = Vec::new();
            for &member in members {
                match firsts.iter().find(|&&first| self.equivalent(first, member)) {
                    Some(&first) => representative[member] = first,
                    None => firsts.push(member),
                }
            }
        }
        representative
    }

    /// Whether `a` and `b` produce the same events forever (bisimilarity,
    /// checked by merging the pairs they reach until one differs).
    fn equivalent(&self, a: usize, b: usize) -> bool {
        let mut merged: HashMap<usize, usize> = HashMap::new();
        let mut pairs = vec![(
            self.landing(&self.identity, a),
            self.landing(&self.identity, b),
        )];
        while let Some((x, y)) = pairs.pop() {
            let (x, y) = (root(&mut merged, x), root(&mut merged, y));
            if x == y {
                continue;
            }
            if !same_behaviour(&self.events[x], &self.events[y]) {
                return false;
            }
            let (next_x, next_y) = (self.successors(x), self.successors(y));
            if next_x.len() != next_y.len() {
                return false;
            }
            merged.insert(x, y);
            pairs.extend(next_x.into_iter().zip(next_y).map(|(p, q)| {
                (
                    self.landing(&self.identity, p),
                    self.landing(&self.identity, q),
                )
            }));
        }
        true
    }
}

fn root(merged: &mut HashMap<usize, usize>, mut node: usize) -> usize {
    while let Some(&parent) = merged.get(&node) {
        node = parent;
    }
    node
}

/// Equal but for a branch's destination, which the successors carry.
fn same_behaviour(a: &SongEvent, b: &SongEvent) -> bool {
    match (a, b) {
        (SongEvent::Goto(_), SongEvent::Goto(_)) => true,
        (
            SongEvent::MemAccBranch {
                condition: c1,
                address: a1,
                data: d1,
                ..
            },
            SongEvent::MemAccBranch {
                condition: c2,
                address: a2,
                data: d2,
                ..
            },
        ) => (c1, a1, d1) == (c2, a2, d2),
        _ => a == b,
    }
}
