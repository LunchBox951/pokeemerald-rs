//! Error types for the `battle` crate.
//!
//! A concrete per-crate enum `(oop-boundaries)` — no `anyhow` in library
//! crates. Errors surfaced by the underlying `assets` tables (species/move
//! lookups) are returned as `assets::AssetError` directly rather than
//! wrapped into this type — see [`crate::dex::Dex`]'s doc comment for why.

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
}

impl fmt::Display for BattleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StatStageOutOfRange(offset) => {
                write!(f, "stat stage offset `{offset}` outside -6..=6")
            }
            Self::UnknownNature(id) => write!(f, "unknown nature id `{id}`"),
        }
    }
}

impl Error for BattleError {}
