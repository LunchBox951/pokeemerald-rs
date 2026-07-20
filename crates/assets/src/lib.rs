//! Assets subsystem (S-4): extraction pipeline + typed access.
//!
//! Exposes extracted upstream game data as idiomatic, owned Rust types
//! `(oop-boundaries)`. Extracted so far: the battle type-effectiveness chart
//! (see [`type_chart`]), the per-move battle-data table (see [`battle_moves`]),
//! the species base-stats table (see [`species`]), the item table (see
//! [`items`]), the per-species evolution table (see [`evolution`]), the
//! per-species TM/HM learnsets (see [`tmhm_learnsets`]), the per-species
//! level-up learnsets (see [`level_up_learnsets`]), the per-species
//! egg-move lists (see [`egg_moves`]), the species/move/ability display text
//! (see [`species_names`], [`move_names`], [`abilities`]), the experience
//! growth curves (see [`experience`]), the per-map wild encounter tables (see
//! [`wild_encounters`]), the trainer roster and parties (see [`trainers`]),
//! the map layout geometry (see [`map_layouts`]), and the map headers,
//! groups, and connections (see [`map_headers`]); more data follows as the
//! crate fills out.

pub mod abilities;
pub mod battle_moves;
pub mod egg_moves;
pub mod error;
pub mod evolution;
pub mod experience;
pub mod items;
pub mod level_up_learnsets;
pub mod map_headers;
pub mod map_layouts;
pub mod move_names;
pub mod species;
pub mod species_names;
pub mod tmhm_learnsets;
pub mod trainers;
pub mod type_chart;
pub mod wild_encounters;

pub use abilities::{Abilities, AbilityData, ABILITIES_COUNT};
pub use battle_moves::{
    MoveData, MoveEffect, MoveFlags, MoveId, MoveTable, MoveTarget, MoveType, MOVES_COUNT,
};
pub use egg_moves::{
    EggMoveList, EggMoveTable, EGG_MOVES_SPECIES_OFFSET, EGG_MOVES_TERMINATOR,
    EGG_MOVE_SPECIES_COUNT,
};
pub use error::AssetError;
pub use evolution::{EvoMethod, Evolution, EvolutionTable};
pub use experience::{experience_for_level, MAX_LEVEL};
pub use level_up_learnsets::{LevelUpLearnsets, LevelUpMove, SPECIES_COUNT};
pub use map_headers::{
    BattleScene, Direction, MapConnection, MapGroup, MapHeader, MapHeaderTable, MapType, MusicId,
    RegionMapSectionId, Weather, MAP_COUNT, MAP_GROUP_COUNT,
};
pub use map_layouts::{
    LayoutId, LayoutTable, MapLayout, MetatileCell, BORDER_CELLS, BORDER_HEIGHT, BORDER_WIDTH,
    LAYOUT_COUNT,
};

// `items::ItemId` is the full item-table identifier; it is intentionally *not*
// re-exported at the crate root to avoid clashing with `species::ItemId` (the
// lightweight held-item reference used by the species table). Reach it via the
// `items` module path.
pub use items::{BattleUsage, HoldEffect, ItemData, ItemTable, ItemType, Pocket, ITEMS_COUNT};
pub use move_names::MoveNames;
pub use species::{
    AbilityId, BaseStats, BodyColor, EggGroup, EvYield, GenderRatio, GrowthRate, ItemId, SpeciesId,
    SpeciesTable,
};
pub use species_names::SpeciesNames;
pub use tmhm_learnsets::{TmHmLearnsets, TmHmSlot};
pub use trainers::{
    AiFlags, EncounterMusic, TrainerClass, TrainerData, TrainerId, TrainerMonItemCustomMoves,
    TrainerMonItemDefaultMoves, TrainerMonNoItemCustomMoves, TrainerMonNoItemDefaultMoves,
    TrainerParty, TrainerPicId, TrainerTable, MAX_TRAINER_ITEMS, TRAINERS_COUNT,
};
pub use type_chart::{Effectiveness, Type, TypeChart};
pub use wild_encounters::{
    FishingEncounters, FishingRod, LandEncounters, MapId, RockSmashEncounters, WaterEncounters,
    WildEncounterHeader, WildEncounterTable, WildPokemon, FISHING_SLOTS, LAND_SLOTS,
    MAP_HEADER_COUNT, ROCK_SMASH_SLOTS, WATER_SLOTS,
};
