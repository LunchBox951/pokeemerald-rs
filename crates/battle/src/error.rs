//! Error types for the `battle` crate.
//!
//! A concrete per-crate enum `(oop-boundaries)` — no `anyhow` in library
//! crates. Lookups that fail inside the underlying `assets` tables surface
//! here as [`BattleError::UnknownSpecies`] / [`BattleError::UnknownMove`],
//! so `battle` callers depend only on this crate's error type.

use assets::{MoveId, SpeciesId};
use std::error::Error;
use std::fmt;

/// An error produced while constructing or querying `battle`-crate data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BattleError {
    /// A stat-stage offset fell outside the upstream range
    /// `MIN_STAT_STAGE..=MAX_STAT_STAGE` (`-6..=+6`,
    /// `pokeemerald/include/constants/pokemon.h`).
    ///
    /// Carries the offending offset.
    ///
    /// [`StatStage`]: crate::stat_stage::StatStage
    StatStageOutOfRange(i8),

    /// A raw `NATURE_*` id did not correspond to any modelled
    /// [`Nature`](crate::nature::Nature).
    ///
    /// Carries the offending id. Upstream defines ids `0..NUM_NATURES`
    /// (`0..25`, `pokeemerald/include/constants/pokemon.h`).
    UnknownNature(u8),

    /// A [`SpeciesId`] fell outside the extracted `gSpeciesInfo` range
    /// (see [`crate::dex::Dex::species`]).
    ///
    /// Carries the offending id.
    UnknownSpecies(SpeciesId),

    /// A [`MoveId`] fell outside the extracted `gBattleMoves` range
    /// (see [`crate::dex::Dex::move_data`]).
    ///
    /// Carries the offending id.
    UnknownMove(MoveId),
}

impl fmt::Display for BattleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StatStageOutOfRange(offset) => {
                write!(f, "stat stage offset `{offset}` outside -6..=6")
            }
            Self::UnknownNature(id) => write!(f, "unknown nature id `{id}`"),
            Self::UnknownSpecies(id) => write!(f, "unknown species id `{}`", id.0),
            Self::UnknownMove(id) => write!(f, "unknown move id `{}`", id.0),
        }
    }
}

impl Error for BattleError {}
