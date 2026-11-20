//! The summary screen's move and contest tables.

use crate::reader::GbaPtr;

/// The four tables behind the summary screen's move page.
///
/// Each is a single array, so one address apiece; the strings the two pointer
/// tables name are reached through their pointers, and the struct tables are
/// read at the strides the retail compiler gave them (see
/// `domains::summary_moves`). A profile records all four or none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SummaryMoveRoots {
    /// `gMoveDescriptionPointers`: 354 pointers indexed by move minus one.
    pub descriptions: GbaPtr,
    /// `gContestMoves`: 355 `ContestMove` rows indexed by move.
    pub contest_moves: GbaPtr,
    /// `gContestEffects`: 48 `ContestEffect` rows.
    pub contest_effects: GbaPtr,
    /// `gContestEffectDescriptionPointers`: 48 pointers indexed by effect.
    pub effect_descriptions: GbaPtr,
}

impl SummaryMoveRoots {
    /// No summary-move roots recorded: the domain writes nothing.
    pub const NONE: Self = Self {
        descriptions: GbaPtr::AT_BASE,
        contest_moves: GbaPtr::AT_BASE,
        contest_effects: GbaPtr::AT_BASE,
        effect_descriptions: GbaPtr::AT_BASE,
    };
}
