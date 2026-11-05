//! The party list's cursor in the single layout
//! (`UpdatePartySelectionSingleLayout`, `pokeemerald/src/party_menu.c:1523-1585`).
//!
//! Slot 0 is the large panel on the left and slots 1 to 5 stack on the right,
//! so LEFT and RIGHT cross between them instead of stepping through the list,
//! and the CANCEL button sits below the last member. The double and multi
//! layouts and the choose-half CONFIRM button are not reachable from the
//! field or from the battle callers modelled here.

/// Where the party list's cursor is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PartyCursor {
    /// A party member's panel.
    Mon(usize),
    /// The CANCEL button (`PARTY_SIZE + 1`).
    Cancel,
}

/// A direction the party list's cursor can be moved in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Direction {
    Up,
    Down,
    Left,
    Right,
}

impl PartyCursor {
    /// The cursor after one `direction` over a party of `count`.
    /// `side` is `lastSelectedSlot`, the right-hand panel LEFT came from and
    /// RIGHT returns to; it starts at 0, which RIGHT reads as slot 1.
    pub(super) fn moved(self, direction: Direction, count: usize, side: &mut usize) -> Self {
        let last = count.checked_sub(1);
        match (direction, self) {
            (Direction::Up, Self::Mon(0)) => Self::Cancel,
            (Direction::Up, Self::Mon(slot)) => Self::Mon(slot - 1),
            (Direction::Up, Self::Cancel) => last.map_or(Self::Cancel, Self::Mon),
            (Direction::Down, Self::Cancel) => {
                if count == 0 {
                    Self::Cancel
                } else {
                    Self::Mon(0)
                }
            }
            (Direction::Down, Self::Mon(slot)) if Some(slot) == last => Self::Cancel,
            (Direction::Down, Self::Mon(slot)) => Self::Mon(slot + 1),
            (Direction::Right, Self::Mon(0)) if count != 1 => {
                Self::Mon(if *side == 0 { 1 } else { *side })
            }
            (Direction::Left, Self::Mon(slot)) if slot != 0 => {
                *side = slot;
                Self::Mon(0)
            }
            (Direction::Right | Direction::Left, cursor) => cursor,
        }
    }

    /// The cursor clamped into a party of `count`: a retained cursor from a
    /// larger party falls back to the first member, or to CANCEL when the
    /// party is empty.
    pub(super) fn within(self, count: usize) -> Self {
        match self {
            Self::Mon(slot) if slot >= count => {
                if count == 0 {
                    Self::Cancel
                } else {
                    Self::Mon(0)
                }
            }
            cursor => cursor,
        }
    }
}
