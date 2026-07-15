//! Assets subsystem (S-4): extraction pipeline + typed access.
//!
//! Exposes extracted upstream game data as idiomatic, owned Rust types
//! `(oop-boundaries)`. Extracted so far: the battle type-effectiveness chart
//! (see [`type_chart`]), the per-move battle-data table (see [`battle_moves`]),
//! the species base-stats table (see [`species`]), the per-species evolution
//! table (see [`evolution`]), the per-species TM/HM learnsets (see
//! [`tmhm_learnsets`]), and per-species display names (see [`species_names`]);
//! more data follows as the crate fills out.

pub mod battle_moves;
pub mod error;
pub mod evolution;
pub mod items;
pub mod species;
pub mod species_names;
pub mod tmhm_learnsets;
pub mod type_chart;

pub use battle_moves::{
    MoveData, MoveEffect, MoveFlags, MoveId, MoveTable, MoveTarget, MoveType, MOVES_COUNT,
};
pub use error::AssetError;
pub use evolution::{EvoMethod, Evolution, EvolutionTable};
// `items::ItemId` is the full item-table identifier; it is intentionally *not*
// re-exported at the crate root to avoid clashing with `species::ItemId` (the
// lightweight held-item reference used by the species table). Reach it via the
// `items` module path.
pub use items::{BattleUsage, HoldEffect, ItemData, ItemTable, ItemType, Pocket, ITEMS_COUNT};
pub use species::{
    AbilityId, BaseStats, BodyColor, EggGroup, EvYield, GenderRatio, GrowthRate, ItemId, SpeciesId,
    SpeciesTable,
};
pub use species_names::SpeciesNames;
pub use tmhm_learnsets::{TmHmLearnsets, TmHmSlot};
pub use type_chart::{Effectiveness, Type, TypeChart};
