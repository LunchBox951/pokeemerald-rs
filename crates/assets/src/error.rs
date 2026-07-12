//! Error types for the `assets` crate.
//!
//! A concrete per-crate enum `(oop-boundaries)` — no `anyhow` in library crates.

use std::error::Error;
use std::fmt;

/// An error produced while accessing or decoding extracted game data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetError {
    /// A raw type identifier did not correspond to any battle [`Type`].
    ///
    /// Carries the offending id. Upstream defines ids `0..=17` (with `9`,
    /// `TYPE_MYSTERY`, reserved for the non-combat "???" type); anything else
    /// is unknown.
    ///
    /// [`Type`]: crate::type_chart::Type
    UnknownType(u8),

    /// A raw move id fell outside the `gBattleMoves` table.
    ///
    /// Carries the offending id. Upstream defines ids `0..MOVES_COUNT`
    /// (`0..355`); anything else has no battle-data entry.
    ///
    /// [`MoveId`]: crate::battle_moves::MoveId
    UnknownMove(u16),
}

impl fmt::Display for AssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownType(id) => write!(f, "unknown battle type id `{id}`"),
            Self::UnknownMove(id) => write!(f, "unknown move id `{id}`"),
        }
    }
}

impl Error for AssetError {}
