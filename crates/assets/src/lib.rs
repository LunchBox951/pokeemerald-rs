//! Assets subsystem (S-4): extraction pipeline + typed access.
//!
//! Exposes extracted upstream game data as idiomatic, owned Rust types
//! `(oop-boundaries)`. Extracted so far: the battle type-effectiveness chart
//! (see [`type_chart`]), the per-move battle-data table (see [`battle_moves`]),
//! and the species base-stats table (see [`species`]); more data follows as the
//! crate fills out.

pub mod battle_moves;
pub mod error;
pub mod species;
pub mod type_chart;

pub use battle_moves::{
    MoveData, MoveEffect, MoveFlags, MoveId, MoveTable, MoveTarget, MoveType, MOVES_COUNT,
};
pub use error::AssetError;
pub use species::{
    AbilityId, BaseStats, BodyColor, EggGroup, EvYield, GenderRatio, GrowthRate, ItemId, SpeciesId,
    SpeciesTable,
};
pub use type_chart::{Effectiveness, Type, TypeChart};
