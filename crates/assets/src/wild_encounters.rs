//! Wild encounter tables (S-4): the `gWildMonHeaders` table.
//!
//! Ports the per-map wild encounter data from the upstream reference
//! `pokeemerald/src/data/wild_encounters.json` (`wild_encounter_groups[0]`,
//! label `gWildMonHeaders`) — 124 `MAP_*` entries, each with up to four
//! encounter kinds (land, water, rock smash, fishing). `gWildMonHeaders`
//! itself is not a checked-in file: it is generated from that JSON by the
//! build's Jinja-style templating (`wild_encounters.json.txt`) into
//! `data/wild_encounters.h`, which `src/wild_encounter.c` `#include`s. This
//! module transcribes straight from the JSON, the canonical source, not from
//! any generated header.
//!
//! The upstream shape (`pokeemerald/include/wild_encounter.h`) is
//! `struct WildPokemon {minLevel, maxLevel, species}`,
//! `struct WildPokemonInfo {encounterRate, wildPokemon}`, and
//! `struct WildPokemonHeader {mapGroup, mapNum, landMonsInfo, waterMonsInfo,
//! rockSmashMonsInfo, fishingMonsInfo}`. Re-expressed idiomatically here
//! `(no-verbatim)`: the four `*MonsInfo` pointers (each `NULL` when that
//! encounter kind is absent for a map) become `Option` fields on
//! [`WildEncounterHeader`], and each present kind is an owned, fixed-size
//! [`WildPokemon`] array rather than a raw pointer.
//!
//! **`mapGroup`/`mapNum` and [`MapId`].** Upstream's `mapGroup`/`mapNum` come
//! from the `MAP_GROUP(map)`/`MAP_NUM(map)` macros, which derive numeric
//! values from a map's *position* in `data/maps/map_groups.json` — a whole
//! map/location layout system that does not exist in this workspace yet and
//! is out of scope here (a future maps/tilesets slice). Modelling fabricated
//! numeric group/num pairs would be dishonest, so [`MapId`] instead wraps the
//! upstream `MAP_*` name directly (e.g. `MapId("MAP_ROUTE101")`) — the same
//! identifier the JSON's `map` field and the game's map-header data already
//! use elsewhere, with no invented numbering.
//!
//! **Duplicate map ids.** 124 JSON entries name only 116 distinct maps: nine
//! entries all share `MAP_ALTERING_CAVE` (`gAlteringCave1`..`gAlteringCave9`),
//! reflecting the real game's rotating Altering Cave species (the cave's
//! wild table is swapped at runtime by logic outside this data). Because a
//! [`MapId`] is therefore not always a unique key, [`WildEncounterHeader`]
//! also carries the upstream `base_label` (unique across all 124 entries,
//! e.g. `"gRoute101"` / `"gAlteringCave3"`) as [`label`](WildEncounterHeader::label),
//! and [`WildEncounterTable`] exposes both a by-label lookup (always unique)
//! and a by-map lookup (returns the first matching entry, plus an
//! `all_by_map` iterator for the Altering Cave case).
//!
//! **Fixed slot counts.** `pokeemerald/include/constants/wild_encounter.h`
//! (cited by the originating issue as the source of the per-kind slot counts)
//! no longer exists in this reference checkout; the invariants are instead
//! derived from the JSON itself, where every present `land_mons`/`water_mons`/
//! `rock_smash_mons`/`fishing_mons` block's `mons` array has a fixed length:
//! land 12, water 5, rock smash 5, fishing 10 ([`LAND_SLOTS`], [`WATER_SLOTS`],
//! [`ROCK_SMASH_SLOTS`], [`FISHING_SLOTS`]). A structural test below checks
//! this holds for every present block across the whole table.
//!
//! **Fishing rods.** The JSON's top-level `fields` metadata fixes how a rod
//! indexes into the 10 flat `fishing_mons` slots: `old_rod` -> `[0, 1]`,
//! `good_rod` -> `[2, 3, 4]`, `super_rod` -> `[5, 6, 7, 8, 9]`. This grouping
//! is the same for every map (it is metadata about the encounter-kind shape,
//! not per-map data), so it is transcribed once as [`FishingRod::slots`]
//! rather than repeated per entry.
//!
//! The upstream-tie tests at the bottom pin Route 101 (land only, the first
//! route reachable from Littleroot — needed for I-4), Route 102 (water +
//! fishing) and Route 111 (rock smash), plus the table length and per-kind
//! slot-count invariants.

use crate::error::AssetError;
use crate::species::SpeciesId;

/// The number of wild-Pokémon slots in a land encounter table
/// (`land_mons.mons` length in every present JSON block).
pub const LAND_SLOTS: usize = 12;
/// The number of wild-Pokémon slots in a water encounter table
/// (`water_mons.mons` length in every present JSON block).
pub const WATER_SLOTS: usize = 5;
/// The number of wild-Pokémon slots in a rock-smash encounter table
/// (`rock_smash_mons.mons` length in every present JSON block).
pub const ROCK_SMASH_SLOTS: usize = 5;
/// The number of wild-Pokémon slots in a fishing encounter table
/// (`fishing_mons.mons` length in every present JSON block).
pub const FISHING_SLOTS: usize = 10;

/// The number of map entries in `gWildMonHeaders`
/// (`wild_encounter_groups[0].encounters.length` in the upstream JSON).
///
/// Not the number of *distinct* maps — see the module docs on duplicate map
/// ids (Altering Cave).
pub const MAP_HEADER_COUNT: usize = 124;

/// A map identifier — the upstream `MAP_*` name, e.g. `MapId("MAP_ROUTE101")`.
///
/// See the module docs for why this wraps the symbolic name rather than a
/// numeric `(map_group, map_num)` pair: deriving upstream's actual
/// `mapGroup`/`mapNum` values requires the map-layout system
/// (`data/maps/map_groups.json`), which is out of scope for this slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MapId(pub &'static str);

impl MapId {
    /// The upstream `MAP_*` name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        self.0
    }
}

/// One wild-Pokémon slot — the owned form of upstream `struct WildPokemon`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WildPokemon {
    /// The lowest level this slot can generate (upstream `minLevel`).
    pub min_level: u8,
    /// The highest level this slot can generate (upstream `maxLevel`).
    pub max_level: u8,
    /// The species this slot generates.
    pub species: SpeciesId,
}

/// A land encounter table — the owned form of upstream `struct
/// WildPokemonInfo` for `landMonsInfo` (always [`LAND_SLOTS`] slots).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LandEncounters {
    /// The chance (out of upstream's internal scale) of a land encounter
    /// triggering at all on this map (upstream `encounterRate`).
    pub encounter_rate: u8,
    /// The fixed-size slot table, in upstream order.
    pub mons: [WildPokemon; LAND_SLOTS],
}

/// A water encounter table — the owned form of upstream `struct
/// WildPokemonInfo` for `waterMonsInfo` (always [`WATER_SLOTS`] slots).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaterEncounters {
    /// Upstream `encounterRate` for water encounters on this map.
    pub encounter_rate: u8,
    /// The fixed-size slot table, in upstream order.
    pub mons: [WildPokemon; WATER_SLOTS],
}

/// A rock-smash encounter table — the owned form of upstream `struct
/// WildPokemonInfo` for `rockSmashMonsInfo` (always [`ROCK_SMASH_SLOTS`]
/// slots).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RockSmashEncounters {
    /// Upstream `encounterRate` for rock-smash encounters on this map.
    pub encounter_rate: u8,
    /// The fixed-size slot table, in upstream order.
    pub mons: [WildPokemon; ROCK_SMASH_SLOTS],
}

/// A fishing encounter table — the owned form of upstream `struct
/// WildPokemonInfo` for `fishingMonsInfo` (always [`FISHING_SLOTS`] slots,
/// grouped into rod tiers by [`FishingRod::slots`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FishingEncounters {
    /// Upstream `encounterRate` for fishing encounters on this map.
    pub encounter_rate: u8,
    /// The fixed-size slot table, in upstream order.
    pub mons: [WildPokemon; FISHING_SLOTS],
}

impl FishingEncounters {
    /// The slots usable with `rod`, in upstream order.
    pub fn mons_for_rod(&self, rod: FishingRod) -> impl Iterator<Item = WildPokemon> + '_ {
        rod.slots().iter().map(|&i| self.mons[i])
    }
}

/// The three fishing-rod tiers, matching the upstream `groups` object in
/// `wild_encounters.json`'s `fishing_mons` field metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FishingRod {
    /// The Old Rod (`old_rod`): slots `[0, 1]`.
    Old,
    /// The Good Rod (`good_rod`): slots `[2, 3, 4]`.
    Good,
    /// The Super Rod (`super_rod`): slots `[5, 6, 7, 8, 9]`.
    Super,
}

impl FishingRod {
    /// The fixed slot indices into a [`FishingEncounters::mons`] array usable
    /// with this rod, per the upstream `groups` metadata (the same for every
    /// map).
    #[must_use]
    pub const fn slots(self) -> &'static [usize] {
        match self {
            FishingRod::Old => &[0, 1],
            FishingRod::Good => &[2, 3, 4],
            FishingRod::Super => &[5, 6, 7, 8, 9],
        }
    }
}

/// One `gWildMonHeaders` entry — the owned form of upstream `struct
/// WildPokemonHeader`, keyed by [`map`](WildEncounterHeader::map) and a
/// unique [`label`](WildEncounterHeader::label).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WildEncounterHeader {
    /// The map this header describes (upstream `mapGroup`/`mapNum`, see the
    /// module docs on [`MapId`]). Not always unique across the table — see
    /// the Altering Cave note in the module docs.
    pub map: MapId,
    /// The upstream `base_label` (e.g. `"gRoute101"`), unique across all
    /// [`MAP_HEADER_COUNT`] entries.
    pub label: &'static str,
    /// Land encounters, if this map has any (upstream `landMonsInfo`, `NULL`
    /// when absent).
    pub land: Option<LandEncounters>,
    /// Water encounters, if this map has any (upstream `waterMonsInfo`,
    /// `NULL` when absent).
    pub water: Option<WaterEncounters>,
    /// Rock-smash encounters, if this map has any (upstream
    /// `rockSmashMonsInfo`, `NULL` when absent).
    pub rock_smash: Option<RockSmashEncounters>,
    /// Fishing encounters, if this map has any (upstream `fishingMonsInfo`,
    /// `NULL` when absent).
    pub fishing: Option<FishingEncounters>,
}

// --- GENERATED: transcribed from pokeemerald/src/data/wild_encounters.json ---

const fn w(min_level: u8, max_level: u8, species: SpeciesId) -> WildPokemon {
    WildPokemon {
        min_level,
        max_level,
        species,
    }
}

const GROUTE101_LAND: LandEncounters = LandEncounters {
    encounter_rate: 20,
    mons: [
        w(2, 2, SpeciesId(290)), /* SPECIES_WURMPLE */
        w(2, 2, SpeciesId(286)), /* SPECIES_POOCHYENA */
        w(2, 2, SpeciesId(290)), /* SPECIES_WURMPLE */
        w(3, 3, SpeciesId(290)), /* SPECIES_WURMPLE */
        w(3, 3, SpeciesId(286)), /* SPECIES_POOCHYENA */
        w(3, 3, SpeciesId(286)), /* SPECIES_POOCHYENA */
        w(3, 3, SpeciesId(290)), /* SPECIES_WURMPLE */
        w(3, 3, SpeciesId(286)), /* SPECIES_POOCHYENA */
        w(2, 2, SpeciesId(288)), /* SPECIES_ZIGZAGOON */
        w(2, 2, SpeciesId(288)), /* SPECIES_ZIGZAGOON */
        w(3, 3, SpeciesId(288)), /* SPECIES_ZIGZAGOON */
        w(3, 3, SpeciesId(288)), /* SPECIES_ZIGZAGOON */
    ],
};

const GROUTE102_LAND: LandEncounters = LandEncounters {
    encounter_rate: 20,
    mons: [
        w(3, 3, SpeciesId(286)), /* SPECIES_POOCHYENA */
        w(3, 3, SpeciesId(290)), /* SPECIES_WURMPLE */
        w(4, 4, SpeciesId(286)), /* SPECIES_POOCHYENA */
        w(4, 4, SpeciesId(290)), /* SPECIES_WURMPLE */
        w(3, 3, SpeciesId(295)), /* SPECIES_LOTAD */
        w(4, 4, SpeciesId(295)), /* SPECIES_LOTAD */
        w(3, 3, SpeciesId(288)), /* SPECIES_ZIGZAGOON */
        w(3, 3, SpeciesId(288)), /* SPECIES_ZIGZAGOON */
        w(4, 4, SpeciesId(288)), /* SPECIES_ZIGZAGOON */
        w(4, 4, SpeciesId(392)), /* SPECIES_RALTS */
        w(4, 4, SpeciesId(288)), /* SPECIES_ZIGZAGOON */
        w(3, 3, SpeciesId(298)), /* SPECIES_SEEDOT */
    ],
};

const GROUTE102_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(20, 30, SpeciesId(183)), /* SPECIES_MARILL */
        w(10, 20, SpeciesId(183)), /* SPECIES_MARILL */
        w(30, 35, SpeciesId(183)), /* SPECIES_MARILL */
        w(5, 10, SpeciesId(183)),  /* SPECIES_MARILL */
        w(20, 30, SpeciesId(118)), /* SPECIES_GOLDEEN */
    ],
};

const GROUTE102_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(118)),  /* SPECIES_GOLDEEN */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(118)), /* SPECIES_GOLDEEN */
        w(10, 30, SpeciesId(326)), /* SPECIES_CORPHISH */
        w(25, 30, SpeciesId(326)), /* SPECIES_CORPHISH */
        w(30, 35, SpeciesId(326)), /* SPECIES_CORPHISH */
        w(20, 25, SpeciesId(326)), /* SPECIES_CORPHISH */
        w(35, 40, SpeciesId(326)), /* SPECIES_CORPHISH */
        w(40, 45, SpeciesId(326)), /* SPECIES_CORPHISH */
    ],
};

const GROUTE103_LAND: LandEncounters = LandEncounters {
    encounter_rate: 20,
    mons: [
        w(2, 2, SpeciesId(286)), /* SPECIES_POOCHYENA */
        w(3, 3, SpeciesId(286)), /* SPECIES_POOCHYENA */
        w(3, 3, SpeciesId(286)), /* SPECIES_POOCHYENA */
        w(4, 4, SpeciesId(286)), /* SPECIES_POOCHYENA */
        w(2, 2, SpeciesId(309)), /* SPECIES_WINGULL */
        w(3, 3, SpeciesId(288)), /* SPECIES_ZIGZAGOON */
        w(3, 3, SpeciesId(288)), /* SPECIES_ZIGZAGOON */
        w(4, 4, SpeciesId(288)), /* SPECIES_ZIGZAGOON */
        w(3, 3, SpeciesId(309)), /* SPECIES_WINGULL */
        w(3, 3, SpeciesId(309)), /* SPECIES_WINGULL */
        w(2, 2, SpeciesId(309)), /* SPECIES_WINGULL */
        w(4, 4, SpeciesId(309)), /* SPECIES_WINGULL */
    ],
};

const GROUTE103_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(5, 35, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(309)), /* SPECIES_WINGULL */
        w(15, 25, SpeciesId(309)), /* SPECIES_WINGULL */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
    ],
};

const GROUTE103_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(30, 35, SpeciesId(331)), /* SPECIES_SHARPEDO */
        w(30, 35, SpeciesId(313)), /* SPECIES_WAILMER */
        w(25, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(35, 40, SpeciesId(313)), /* SPECIES_WAILMER */
        w(40, 45, SpeciesId(313)), /* SPECIES_WAILMER */
    ],
};

const GROUTE104_LAND: LandEncounters = LandEncounters {
    encounter_rate: 20,
    mons: [
        w(4, 4, SpeciesId(286)), /* SPECIES_POOCHYENA */
        w(4, 4, SpeciesId(290)), /* SPECIES_WURMPLE */
        w(5, 5, SpeciesId(286)), /* SPECIES_POOCHYENA */
        w(5, 5, SpeciesId(183)), /* SPECIES_MARILL */
        w(4, 4, SpeciesId(183)), /* SPECIES_MARILL */
        w(5, 5, SpeciesId(286)), /* SPECIES_POOCHYENA */
        w(4, 4, SpeciesId(304)), /* SPECIES_TAILLOW */
        w(5, 5, SpeciesId(304)), /* SPECIES_TAILLOW */
        w(4, 4, SpeciesId(309)), /* SPECIES_WINGULL */
        w(4, 4, SpeciesId(309)), /* SPECIES_WINGULL */
        w(3, 3, SpeciesId(309)), /* SPECIES_WINGULL */
        w(5, 5, SpeciesId(309)), /* SPECIES_WINGULL */
    ],
};

const GROUTE104_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(10, 30, SpeciesId(309)), /* SPECIES_WINGULL */
        w(15, 25, SpeciesId(309)), /* SPECIES_WINGULL */
        w(15, 25, SpeciesId(309)), /* SPECIES_WINGULL */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
    ],
};

const GROUTE104_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(25, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(30, 35, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(20, 25, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(35, 40, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(40, 45, SpeciesId(129)), /* SPECIES_MAGIKARP */
    ],
};

const GROUTE105_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(5, 35, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(309)), /* SPECIES_WINGULL */
        w(15, 25, SpeciesId(309)), /* SPECIES_WINGULL */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
    ],
};

const GROUTE105_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(25, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(30, 35, SpeciesId(313)), /* SPECIES_WAILMER */
        w(20, 25, SpeciesId(313)), /* SPECIES_WAILMER */
        w(35, 40, SpeciesId(313)), /* SPECIES_WAILMER */
        w(40, 45, SpeciesId(313)), /* SPECIES_WAILMER */
    ],
};

const GROUTE110_LAND: LandEncounters = LandEncounters {
    encounter_rate: 20,
    mons: [
        w(12, 12, SpeciesId(286)), /* SPECIES_POOCHYENA */
        w(12, 12, SpeciesId(337)), /* SPECIES_ELECTRIKE */
        w(12, 12, SpeciesId(367)), /* SPECIES_GULPIN */
        w(13, 13, SpeciesId(337)), /* SPECIES_ELECTRIKE */
        w(13, 13, SpeciesId(354)), /* SPECIES_MINUN */
        w(13, 13, SpeciesId(43)),  /* SPECIES_ODDISH */
        w(13, 13, SpeciesId(354)), /* SPECIES_MINUN */
        w(13, 13, SpeciesId(367)), /* SPECIES_GULPIN */
        w(12, 12, SpeciesId(309)), /* SPECIES_WINGULL */
        w(12, 12, SpeciesId(309)), /* SPECIES_WINGULL */
        w(12, 12, SpeciesId(353)), /* SPECIES_PLUSLE */
        w(13, 13, SpeciesId(353)), /* SPECIES_PLUSLE */
    ],
};

const GROUTE110_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(5, 35, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(309)), /* SPECIES_WINGULL */
        w(15, 25, SpeciesId(309)), /* SPECIES_WINGULL */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
    ],
};

const GROUTE110_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(25, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(30, 35, SpeciesId(313)), /* SPECIES_WAILMER */
        w(20, 25, SpeciesId(313)), /* SPECIES_WAILMER */
        w(35, 40, SpeciesId(313)), /* SPECIES_WAILMER */
        w(40, 45, SpeciesId(313)), /* SPECIES_WAILMER */
    ],
};

const GROUTE111_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(20, 20, SpeciesId(27)),  /* SPECIES_SANDSHREW */
        w(20, 20, SpeciesId(332)), /* SPECIES_TRAPINCH */
        w(21, 21, SpeciesId(27)),  /* SPECIES_SANDSHREW */
        w(21, 21, SpeciesId(332)), /* SPECIES_TRAPINCH */
        w(19, 19, SpeciesId(318)), /* SPECIES_BALTOY */
        w(21, 21, SpeciesId(318)), /* SPECIES_BALTOY */
        w(19, 19, SpeciesId(27)),  /* SPECIES_SANDSHREW */
        w(19, 19, SpeciesId(332)), /* SPECIES_TRAPINCH */
        w(20, 20, SpeciesId(318)), /* SPECIES_BALTOY */
        w(20, 20, SpeciesId(344)), /* SPECIES_CACNEA */
        w(22, 22, SpeciesId(344)), /* SPECIES_CACNEA */
        w(22, 22, SpeciesId(344)), /* SPECIES_CACNEA */
    ],
};

const GROUTE111_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(20, 30, SpeciesId(183)), /* SPECIES_MARILL */
        w(10, 20, SpeciesId(183)), /* SPECIES_MARILL */
        w(30, 35, SpeciesId(183)), /* SPECIES_MARILL */
        w(5, 10, SpeciesId(183)),  /* SPECIES_MARILL */
        w(20, 30, SpeciesId(118)), /* SPECIES_GOLDEEN */
    ],
};

const GROUTE111_ROCKSMASH: RockSmashEncounters = RockSmashEncounters {
    encounter_rate: 20,
    mons: [
        w(10, 15, SpeciesId(74)), /* SPECIES_GEODUDE */
        w(5, 10, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(15, 20, SpeciesId(74)), /* SPECIES_GEODUDE */
        w(15, 20, SpeciesId(74)), /* SPECIES_GEODUDE */
        w(15, 20, SpeciesId(74)), /* SPECIES_GEODUDE */
    ],
};

const GROUTE111_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(118)),  /* SPECIES_GOLDEEN */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(118)), /* SPECIES_GOLDEEN */
        w(10, 30, SpeciesId(323)), /* SPECIES_BARBOACH */
        w(25, 30, SpeciesId(323)), /* SPECIES_BARBOACH */
        w(30, 35, SpeciesId(323)), /* SPECIES_BARBOACH */
        w(20, 25, SpeciesId(323)), /* SPECIES_BARBOACH */
        w(35, 40, SpeciesId(323)), /* SPECIES_BARBOACH */
        w(40, 45, SpeciesId(323)), /* SPECIES_BARBOACH */
    ],
};

const GROUTE112_LAND: LandEncounters = LandEncounters {
    encounter_rate: 20,
    mons: [
        w(15, 15, SpeciesId(339)), /* SPECIES_NUMEL */
        w(15, 15, SpeciesId(339)), /* SPECIES_NUMEL */
        w(15, 15, SpeciesId(183)), /* SPECIES_MARILL */
        w(14, 14, SpeciesId(339)), /* SPECIES_NUMEL */
        w(14, 14, SpeciesId(339)), /* SPECIES_NUMEL */
        w(14, 14, SpeciesId(183)), /* SPECIES_MARILL */
        w(16, 16, SpeciesId(339)), /* SPECIES_NUMEL */
        w(16, 16, SpeciesId(183)), /* SPECIES_MARILL */
        w(16, 16, SpeciesId(339)), /* SPECIES_NUMEL */
        w(16, 16, SpeciesId(339)), /* SPECIES_NUMEL */
        w(16, 16, SpeciesId(339)), /* SPECIES_NUMEL */
        w(16, 16, SpeciesId(339)), /* SPECIES_NUMEL */
    ],
};

const GROUTE113_LAND: LandEncounters = LandEncounters {
    encounter_rate: 20,
    mons: [
        w(15, 15, SpeciesId(308)), /* SPECIES_SPINDA */
        w(15, 15, SpeciesId(308)), /* SPECIES_SPINDA */
        w(15, 15, SpeciesId(218)), /* SPECIES_SLUGMA */
        w(14, 14, SpeciesId(308)), /* SPECIES_SPINDA */
        w(14, 14, SpeciesId(308)), /* SPECIES_SPINDA */
        w(14, 14, SpeciesId(218)), /* SPECIES_SLUGMA */
        w(16, 16, SpeciesId(308)), /* SPECIES_SPINDA */
        w(16, 16, SpeciesId(218)), /* SPECIES_SLUGMA */
        w(16, 16, SpeciesId(308)), /* SPECIES_SPINDA */
        w(16, 16, SpeciesId(227)), /* SPECIES_SKARMORY */
        w(16, 16, SpeciesId(308)), /* SPECIES_SPINDA */
        w(16, 16, SpeciesId(227)), /* SPECIES_SKARMORY */
    ],
};

const GROUTE114_LAND: LandEncounters = LandEncounters {
    encounter_rate: 20,
    mons: [
        w(16, 16, SpeciesId(358)), /* SPECIES_SWABLU */
        w(16, 16, SpeciesId(295)), /* SPECIES_LOTAD */
        w(17, 17, SpeciesId(358)), /* SPECIES_SWABLU */
        w(15, 15, SpeciesId(358)), /* SPECIES_SWABLU */
        w(15, 15, SpeciesId(295)), /* SPECIES_LOTAD */
        w(16, 16, SpeciesId(296)), /* SPECIES_LOMBRE */
        w(16, 16, SpeciesId(296)), /* SPECIES_LOMBRE */
        w(18, 18, SpeciesId(296)), /* SPECIES_LOMBRE */
        w(17, 17, SpeciesId(379)), /* SPECIES_SEVIPER */
        w(15, 15, SpeciesId(379)), /* SPECIES_SEVIPER */
        w(17, 17, SpeciesId(379)), /* SPECIES_SEVIPER */
        w(15, 15, SpeciesId(299)), /* SPECIES_NUZLEAF */
    ],
};

const GROUTE114_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(20, 30, SpeciesId(183)), /* SPECIES_MARILL */
        w(10, 20, SpeciesId(183)), /* SPECIES_MARILL */
        w(30, 35, SpeciesId(183)), /* SPECIES_MARILL */
        w(5, 10, SpeciesId(183)),  /* SPECIES_MARILL */
        w(20, 30, SpeciesId(118)), /* SPECIES_GOLDEEN */
    ],
};

const GROUTE114_ROCKSMASH: RockSmashEncounters = RockSmashEncounters {
    encounter_rate: 20,
    mons: [
        w(10, 15, SpeciesId(74)), /* SPECIES_GEODUDE */
        w(5, 10, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(15, 20, SpeciesId(74)), /* SPECIES_GEODUDE */
        w(15, 20, SpeciesId(74)), /* SPECIES_GEODUDE */
        w(15, 20, SpeciesId(74)), /* SPECIES_GEODUDE */
    ],
};

const GROUTE114_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(118)),  /* SPECIES_GOLDEEN */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(118)), /* SPECIES_GOLDEEN */
        w(10, 30, SpeciesId(323)), /* SPECIES_BARBOACH */
        w(25, 30, SpeciesId(323)), /* SPECIES_BARBOACH */
        w(30, 35, SpeciesId(323)), /* SPECIES_BARBOACH */
        w(20, 25, SpeciesId(323)), /* SPECIES_BARBOACH */
        w(35, 40, SpeciesId(323)), /* SPECIES_BARBOACH */
        w(40, 45, SpeciesId(323)), /* SPECIES_BARBOACH */
    ],
};

const GROUTE116_LAND: LandEncounters = LandEncounters {
    encounter_rate: 20,
    mons: [
        w(6, 6, SpeciesId(286)), /* SPECIES_POOCHYENA */
        w(6, 6, SpeciesId(370)), /* SPECIES_WHISMUR */
        w(6, 6, SpeciesId(301)), /* SPECIES_NINCADA */
        w(7, 7, SpeciesId(63)),  /* SPECIES_ABRA */
        w(7, 7, SpeciesId(301)), /* SPECIES_NINCADA */
        w(6, 6, SpeciesId(304)), /* SPECIES_TAILLOW */
        w(7, 7, SpeciesId(304)), /* SPECIES_TAILLOW */
        w(8, 8, SpeciesId(304)), /* SPECIES_TAILLOW */
        w(7, 7, SpeciesId(286)), /* SPECIES_POOCHYENA */
        w(8, 8, SpeciesId(286)), /* SPECIES_POOCHYENA */
        w(7, 7, SpeciesId(315)), /* SPECIES_SKITTY */
        w(8, 8, SpeciesId(315)), /* SPECIES_SKITTY */
    ],
};

const GROUTE117_LAND: LandEncounters = LandEncounters {
    encounter_rate: 20,
    mons: [
        w(13, 13, SpeciesId(286)), /* SPECIES_POOCHYENA */
        w(13, 13, SpeciesId(43)),  /* SPECIES_ODDISH */
        w(14, 14, SpeciesId(286)), /* SPECIES_POOCHYENA */
        w(14, 14, SpeciesId(43)),  /* SPECIES_ODDISH */
        w(13, 13, SpeciesId(183)), /* SPECIES_MARILL */
        w(13, 13, SpeciesId(43)),  /* SPECIES_ODDISH */
        w(13, 13, SpeciesId(387)), /* SPECIES_ILLUMISE */
        w(13, 13, SpeciesId(387)), /* SPECIES_ILLUMISE */
        w(14, 14, SpeciesId(387)), /* SPECIES_ILLUMISE */
        w(14, 14, SpeciesId(387)), /* SPECIES_ILLUMISE */
        w(13, 13, SpeciesId(386)), /* SPECIES_VOLBEAT */
        w(13, 13, SpeciesId(298)), /* SPECIES_SEEDOT */
    ],
};

const GROUTE117_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(20, 30, SpeciesId(183)), /* SPECIES_MARILL */
        w(10, 20, SpeciesId(183)), /* SPECIES_MARILL */
        w(30, 35, SpeciesId(183)), /* SPECIES_MARILL */
        w(5, 10, SpeciesId(183)),  /* SPECIES_MARILL */
        w(20, 30, SpeciesId(118)), /* SPECIES_GOLDEEN */
    ],
};

const GROUTE117_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(118)),  /* SPECIES_GOLDEEN */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(118)), /* SPECIES_GOLDEEN */
        w(10, 30, SpeciesId(326)), /* SPECIES_CORPHISH */
        w(25, 30, SpeciesId(326)), /* SPECIES_CORPHISH */
        w(30, 35, SpeciesId(326)), /* SPECIES_CORPHISH */
        w(20, 25, SpeciesId(326)), /* SPECIES_CORPHISH */
        w(35, 40, SpeciesId(326)), /* SPECIES_CORPHISH */
        w(40, 45, SpeciesId(326)), /* SPECIES_CORPHISH */
    ],
};

const GROUTE118_LAND: LandEncounters = LandEncounters {
    encounter_rate: 20,
    mons: [
        w(24, 24, SpeciesId(288)), /* SPECIES_ZIGZAGOON */
        w(24, 24, SpeciesId(337)), /* SPECIES_ELECTRIKE */
        w(26, 26, SpeciesId(288)), /* SPECIES_ZIGZAGOON */
        w(26, 26, SpeciesId(337)), /* SPECIES_ELECTRIKE */
        w(26, 26, SpeciesId(289)), /* SPECIES_LINOONE */
        w(26, 26, SpeciesId(338)), /* SPECIES_MANECTRIC */
        w(25, 25, SpeciesId(309)), /* SPECIES_WINGULL */
        w(25, 25, SpeciesId(309)), /* SPECIES_WINGULL */
        w(26, 26, SpeciesId(309)), /* SPECIES_WINGULL */
        w(26, 26, SpeciesId(309)), /* SPECIES_WINGULL */
        w(27, 27, SpeciesId(309)), /* SPECIES_WINGULL */
        w(25, 25, SpeciesId(317)), /* SPECIES_KECLEON */
    ],
};

const GROUTE118_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(5, 35, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(309)), /* SPECIES_WINGULL */
        w(15, 25, SpeciesId(309)), /* SPECIES_WINGULL */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
    ],
};

const GROUTE118_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(330)), /* SPECIES_CARVANHA */
        w(30, 35, SpeciesId(331)), /* SPECIES_SHARPEDO */
        w(30, 35, SpeciesId(330)), /* SPECIES_CARVANHA */
        w(20, 25, SpeciesId(330)), /* SPECIES_CARVANHA */
        w(35, 40, SpeciesId(330)), /* SPECIES_CARVANHA */
        w(40, 45, SpeciesId(330)), /* SPECIES_CARVANHA */
    ],
};

const GROUTE124_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(5, 35, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(309)), /* SPECIES_WINGULL */
        w(15, 25, SpeciesId(309)), /* SPECIES_WINGULL */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
    ],
};

const GROUTE124_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(30, 35, SpeciesId(331)), /* SPECIES_SHARPEDO */
        w(30, 35, SpeciesId(313)), /* SPECIES_WAILMER */
        w(25, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(35, 40, SpeciesId(313)), /* SPECIES_WAILMER */
        w(40, 45, SpeciesId(313)), /* SPECIES_WAILMER */
    ],
};

const GPETALBURGWOODS_LAND: LandEncounters = LandEncounters {
    encounter_rate: 20,
    mons: [
        w(5, 5, SpeciesId(286)), /* SPECIES_POOCHYENA */
        w(5, 5, SpeciesId(290)), /* SPECIES_WURMPLE */
        w(5, 5, SpeciesId(306)), /* SPECIES_SHROOMISH */
        w(6, 6, SpeciesId(286)), /* SPECIES_POOCHYENA */
        w(5, 5, SpeciesId(291)), /* SPECIES_SILCOON */
        w(5, 5, SpeciesId(293)), /* SPECIES_CASCOON */
        w(6, 6, SpeciesId(290)), /* SPECIES_WURMPLE */
        w(6, 6, SpeciesId(306)), /* SPECIES_SHROOMISH */
        w(5, 5, SpeciesId(304)), /* SPECIES_TAILLOW */
        w(5, 5, SpeciesId(364)), /* SPECIES_SLAKOTH */
        w(6, 6, SpeciesId(304)), /* SPECIES_TAILLOW */
        w(6, 6, SpeciesId(364)), /* SPECIES_SLAKOTH */
    ],
};

const GRUSTURFTUNNEL_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(6, 6, SpeciesId(370)), /* SPECIES_WHISMUR */
        w(7, 7, SpeciesId(370)), /* SPECIES_WHISMUR */
        w(6, 6, SpeciesId(370)), /* SPECIES_WHISMUR */
        w(6, 6, SpeciesId(370)), /* SPECIES_WHISMUR */
        w(7, 7, SpeciesId(370)), /* SPECIES_WHISMUR */
        w(7, 7, SpeciesId(370)), /* SPECIES_WHISMUR */
        w(5, 5, SpeciesId(370)), /* SPECIES_WHISMUR */
        w(8, 8, SpeciesId(370)), /* SPECIES_WHISMUR */
        w(5, 5, SpeciesId(370)), /* SPECIES_WHISMUR */
        w(8, 8, SpeciesId(370)), /* SPECIES_WHISMUR */
        w(5, 5, SpeciesId(370)), /* SPECIES_WHISMUR */
        w(8, 8, SpeciesId(370)), /* SPECIES_WHISMUR */
    ],
};

const GGRANITECAVE_1F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(7, 7, SpeciesId(41)),    /* SPECIES_ZUBAT */
        w(8, 8, SpeciesId(335)),   /* SPECIES_MAKUHITA */
        w(7, 7, SpeciesId(335)),   /* SPECIES_MAKUHITA */
        w(8, 8, SpeciesId(41)),    /* SPECIES_ZUBAT */
        w(9, 9, SpeciesId(335)),   /* SPECIES_MAKUHITA */
        w(8, 8, SpeciesId(63)),    /* SPECIES_ABRA */
        w(10, 10, SpeciesId(335)), /* SPECIES_MAKUHITA */
        w(6, 6, SpeciesId(335)),   /* SPECIES_MAKUHITA */
        w(7, 7, SpeciesId(74)),    /* SPECIES_GEODUDE */
        w(8, 8, SpeciesId(74)),    /* SPECIES_GEODUDE */
        w(6, 6, SpeciesId(74)),    /* SPECIES_GEODUDE */
        w(9, 9, SpeciesId(74)),    /* SPECIES_GEODUDE */
    ],
};

const GGRANITECAVE_B1F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(9, 9, SpeciesId(41)),    /* SPECIES_ZUBAT */
        w(10, 10, SpeciesId(382)), /* SPECIES_ARON */
        w(9, 9, SpeciesId(382)),   /* SPECIES_ARON */
        w(11, 11, SpeciesId(382)), /* SPECIES_ARON */
        w(10, 10, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(9, 9, SpeciesId(63)),    /* SPECIES_ABRA */
        w(10, 10, SpeciesId(335)), /* SPECIES_MAKUHITA */
        w(11, 11, SpeciesId(335)), /* SPECIES_MAKUHITA */
        w(10, 10, SpeciesId(322)), /* SPECIES_SABLEYE */
        w(10, 10, SpeciesId(322)), /* SPECIES_SABLEYE */
        w(9, 9, SpeciesId(322)),   /* SPECIES_SABLEYE */
        w(11, 11, SpeciesId(322)), /* SPECIES_SABLEYE */
    ],
};

const GMTPYRE_1F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(27, 27, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(28, 28, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(26, 26, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(25, 25, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(29, 29, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(24, 24, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(23, 23, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(22, 22, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(29, 29, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(24, 24, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(29, 29, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(24, 24, SpeciesId(377)), /* SPECIES_SHUPPET */
    ],
};

const GVICTORYROAD_1F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(40, 40, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(40, 40, SpeciesId(336)), /* SPECIES_HARIYAMA */
        w(40, 40, SpeciesId(383)), /* SPECIES_LAIRON */
        w(40, 40, SpeciesId(371)), /* SPECIES_LOUDRED */
        w(36, 36, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(36, 36, SpeciesId(335)), /* SPECIES_MAKUHITA */
        w(38, 38, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(38, 38, SpeciesId(336)), /* SPECIES_HARIYAMA */
        w(36, 36, SpeciesId(382)), /* SPECIES_ARON */
        w(36, 36, SpeciesId(370)), /* SPECIES_WHISMUR */
        w(36, 36, SpeciesId(382)), /* SPECIES_ARON */
        w(36, 36, SpeciesId(370)), /* SPECIES_WHISMUR */
    ],
};

const GSAFARIZONE_SOUTH_LAND: LandEncounters = LandEncounters {
    encounter_rate: 25,
    mons: [
        w(25, 25, SpeciesId(43)),  /* SPECIES_ODDISH */
        w(27, 27, SpeciesId(43)),  /* SPECIES_ODDISH */
        w(25, 25, SpeciesId(203)), /* SPECIES_GIRAFARIG */
        w(27, 27, SpeciesId(203)), /* SPECIES_GIRAFARIG */
        w(25, 25, SpeciesId(177)), /* SPECIES_NATU */
        w(25, 25, SpeciesId(84)),  /* SPECIES_DODUO */
        w(25, 25, SpeciesId(44)),  /* SPECIES_GLOOM */
        w(27, 27, SpeciesId(202)), /* SPECIES_WOBBUFFET */
        w(25, 25, SpeciesId(25)),  /* SPECIES_PIKACHU */
        w(27, 27, SpeciesId(202)), /* SPECIES_WOBBUFFET */
        w(27, 27, SpeciesId(25)),  /* SPECIES_PIKACHU */
        w(29, 29, SpeciesId(202)), /* SPECIES_WOBBUFFET */
    ],
};

const GUNDERWATER_ROUTE126_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(20, 30, SpeciesId(373)), /* SPECIES_CLAMPERL */
        w(20, 30, SpeciesId(170)), /* SPECIES_CHINCHOU */
        w(30, 35, SpeciesId(373)), /* SPECIES_CLAMPERL */
        w(30, 35, SpeciesId(381)), /* SPECIES_RELICANTH */
        w(30, 35, SpeciesId(381)), /* SPECIES_RELICANTH */
    ],
};

const GABANDONEDSHIP_ROOMS_B1F_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(5, 35, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(5, 35, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(5, 35, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(5, 35, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(30, 35, SpeciesId(73)), /* SPECIES_TENTACRUEL */
    ],
};

const GABANDONEDSHIP_ROOMS_B1F_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 20,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(25, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(30, 35, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(30, 35, SpeciesId(73)),  /* SPECIES_TENTACRUEL */
        w(25, 30, SpeciesId(73)),  /* SPECIES_TENTACRUEL */
        w(20, 25, SpeciesId(73)),  /* SPECIES_TENTACRUEL */
    ],
};

const GGRANITECAVE_B2F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(10, 10, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(11, 11, SpeciesId(382)), /* SPECIES_ARON */
        w(10, 10, SpeciesId(382)), /* SPECIES_ARON */
        w(11, 11, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(12, 12, SpeciesId(382)), /* SPECIES_ARON */
        w(10, 10, SpeciesId(63)),  /* SPECIES_ABRA */
        w(10, 10, SpeciesId(322)), /* SPECIES_SABLEYE */
        w(11, 11, SpeciesId(322)), /* SPECIES_SABLEYE */
        w(12, 12, SpeciesId(322)), /* SPECIES_SABLEYE */
        w(10, 10, SpeciesId(322)), /* SPECIES_SABLEYE */
        w(12, 12, SpeciesId(322)), /* SPECIES_SABLEYE */
        w(10, 10, SpeciesId(322)), /* SPECIES_SABLEYE */
    ],
};

const GGRANITECAVE_B2F_ROCKSMASH: RockSmashEncounters = RockSmashEncounters {
    encounter_rate: 20,
    mons: [
        w(10, 15, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(10, 20, SpeciesId(320)), /* SPECIES_NOSEPASS */
        w(5, 10, SpeciesId(74)),   /* SPECIES_GEODUDE */
        w(15, 20, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(15, 20, SpeciesId(74)),  /* SPECIES_GEODUDE */
    ],
};

const GFIERYPATH_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(15, 15, SpeciesId(339)), /* SPECIES_NUMEL */
        w(15, 15, SpeciesId(109)), /* SPECIES_KOFFING */
        w(16, 16, SpeciesId(339)), /* SPECIES_NUMEL */
        w(15, 15, SpeciesId(66)),  /* SPECIES_MACHOP */
        w(15, 15, SpeciesId(321)), /* SPECIES_TORKOAL */
        w(15, 15, SpeciesId(218)), /* SPECIES_SLUGMA */
        w(16, 16, SpeciesId(109)), /* SPECIES_KOFFING */
        w(16, 16, SpeciesId(66)),  /* SPECIES_MACHOP */
        w(14, 14, SpeciesId(321)), /* SPECIES_TORKOAL */
        w(16, 16, SpeciesId(321)), /* SPECIES_TORKOAL */
        w(14, 14, SpeciesId(88)),  /* SPECIES_GRIMER */
        w(14, 14, SpeciesId(88)),  /* SPECIES_GRIMER */
    ],
};

const GMETEORFALLS_B1F_2R_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(33, 33, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(35, 35, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(30, 30, SpeciesId(395)), /* SPECIES_BAGON */
        w(35, 35, SpeciesId(349)), /* SPECIES_SOLROCK */
        w(35, 35, SpeciesId(395)), /* SPECIES_BAGON */
        w(37, 37, SpeciesId(349)), /* SPECIES_SOLROCK */
        w(25, 25, SpeciesId(395)), /* SPECIES_BAGON */
        w(39, 39, SpeciesId(349)), /* SPECIES_SOLROCK */
        w(38, 38, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(40, 40, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(38, 38, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(40, 40, SpeciesId(42)),  /* SPECIES_GOLBAT */
    ],
};

const GMETEORFALLS_B1F_2R_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(30, 35, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(30, 35, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(25, 35, SpeciesId(349)), /* SPECIES_SOLROCK */
        w(15, 25, SpeciesId(349)), /* SPECIES_SOLROCK */
        w(5, 15, SpeciesId(349)),  /* SPECIES_SOLROCK */
    ],
};

const GMETEORFALLS_B1F_2R_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(118)),  /* SPECIES_GOLDEEN */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(118)), /* SPECIES_GOLDEEN */
        w(10, 30, SpeciesId(323)), /* SPECIES_BARBOACH */
        w(25, 30, SpeciesId(323)), /* SPECIES_BARBOACH */
        w(30, 35, SpeciesId(323)), /* SPECIES_BARBOACH */
        w(30, 35, SpeciesId(324)), /* SPECIES_WHISCASH */
        w(35, 40, SpeciesId(324)), /* SPECIES_WHISCASH */
        w(40, 45, SpeciesId(324)), /* SPECIES_WHISCASH */
    ],
};

const GJAGGEDPASS_LAND: LandEncounters = LandEncounters {
    encounter_rate: 20,
    mons: [
        w(21, 21, SpeciesId(339)), /* SPECIES_NUMEL */
        w(21, 21, SpeciesId(339)), /* SPECIES_NUMEL */
        w(21, 21, SpeciesId(66)),  /* SPECIES_MACHOP */
        w(20, 20, SpeciesId(339)), /* SPECIES_NUMEL */
        w(20, 20, SpeciesId(351)), /* SPECIES_SPOINK */
        w(20, 20, SpeciesId(66)),  /* SPECIES_MACHOP */
        w(21, 21, SpeciesId(351)), /* SPECIES_SPOINK */
        w(22, 22, SpeciesId(66)),  /* SPECIES_MACHOP */
        w(22, 22, SpeciesId(339)), /* SPECIES_NUMEL */
        w(22, 22, SpeciesId(351)), /* SPECIES_SPOINK */
        w(22, 22, SpeciesId(339)), /* SPECIES_NUMEL */
        w(22, 22, SpeciesId(351)), /* SPECIES_SPOINK */
    ],
};

const GROUTE106_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(5, 35, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(309)), /* SPECIES_WINGULL */
        w(15, 25, SpeciesId(309)), /* SPECIES_WINGULL */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
    ],
};

const GROUTE106_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(25, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(30, 35, SpeciesId(313)), /* SPECIES_WAILMER */
        w(20, 25, SpeciesId(313)), /* SPECIES_WAILMER */
        w(35, 40, SpeciesId(313)), /* SPECIES_WAILMER */
        w(40, 45, SpeciesId(313)), /* SPECIES_WAILMER */
    ],
};

const GROUTE107_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(5, 35, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(309)), /* SPECIES_WINGULL */
        w(15, 25, SpeciesId(309)), /* SPECIES_WINGULL */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
    ],
};

const GROUTE107_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(25, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(30, 35, SpeciesId(313)), /* SPECIES_WAILMER */
        w(20, 25, SpeciesId(313)), /* SPECIES_WAILMER */
        w(35, 40, SpeciesId(313)), /* SPECIES_WAILMER */
        w(40, 45, SpeciesId(313)), /* SPECIES_WAILMER */
    ],
};

const GROUTE108_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(5, 35, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(309)), /* SPECIES_WINGULL */
        w(15, 25, SpeciesId(309)), /* SPECIES_WINGULL */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
    ],
};

const GROUTE108_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(25, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(30, 35, SpeciesId(313)), /* SPECIES_WAILMER */
        w(20, 25, SpeciesId(313)), /* SPECIES_WAILMER */
        w(35, 40, SpeciesId(313)), /* SPECIES_WAILMER */
        w(40, 45, SpeciesId(313)), /* SPECIES_WAILMER */
    ],
};

const GROUTE109_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(5, 35, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(309)), /* SPECIES_WINGULL */
        w(15, 25, SpeciesId(309)), /* SPECIES_WINGULL */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
    ],
};

const GROUTE109_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(25, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(30, 35, SpeciesId(313)), /* SPECIES_WAILMER */
        w(20, 25, SpeciesId(313)), /* SPECIES_WAILMER */
        w(35, 40, SpeciesId(313)), /* SPECIES_WAILMER */
        w(40, 45, SpeciesId(313)), /* SPECIES_WAILMER */
    ],
};

const GROUTE115_LAND: LandEncounters = LandEncounters {
    encounter_rate: 20,
    mons: [
        w(23, 23, SpeciesId(358)), /* SPECIES_SWABLU */
        w(23, 23, SpeciesId(304)), /* SPECIES_TAILLOW */
        w(25, 25, SpeciesId(358)), /* SPECIES_SWABLU */
        w(24, 24, SpeciesId(304)), /* SPECIES_TAILLOW */
        w(25, 25, SpeciesId(304)), /* SPECIES_TAILLOW */
        w(25, 25, SpeciesId(305)), /* SPECIES_SWELLOW */
        w(24, 24, SpeciesId(39)),  /* SPECIES_JIGGLYPUFF */
        w(25, 25, SpeciesId(39)),  /* SPECIES_JIGGLYPUFF */
        w(24, 24, SpeciesId(309)), /* SPECIES_WINGULL */
        w(24, 24, SpeciesId(309)), /* SPECIES_WINGULL */
        w(26, 26, SpeciesId(309)), /* SPECIES_WINGULL */
        w(25, 25, SpeciesId(309)), /* SPECIES_WINGULL */
    ],
};

const GROUTE115_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(5, 35, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(309)), /* SPECIES_WINGULL */
        w(15, 25, SpeciesId(309)), /* SPECIES_WINGULL */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
    ],
};

const GROUTE115_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(25, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(30, 35, SpeciesId(313)), /* SPECIES_WAILMER */
        w(20, 25, SpeciesId(313)), /* SPECIES_WAILMER */
        w(35, 40, SpeciesId(313)), /* SPECIES_WAILMER */
        w(40, 45, SpeciesId(313)), /* SPECIES_WAILMER */
    ],
};

const GNEWMAUVILLE_INSIDE_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(24, 24, SpeciesId(100)), /* SPECIES_VOLTORB */
        w(24, 24, SpeciesId(81)),  /* SPECIES_MAGNEMITE */
        w(25, 25, SpeciesId(100)), /* SPECIES_VOLTORB */
        w(25, 25, SpeciesId(81)),  /* SPECIES_MAGNEMITE */
        w(23, 23, SpeciesId(100)), /* SPECIES_VOLTORB */
        w(23, 23, SpeciesId(81)),  /* SPECIES_MAGNEMITE */
        w(26, 26, SpeciesId(100)), /* SPECIES_VOLTORB */
        w(26, 26, SpeciesId(81)),  /* SPECIES_MAGNEMITE */
        w(22, 22, SpeciesId(100)), /* SPECIES_VOLTORB */
        w(22, 22, SpeciesId(81)),  /* SPECIES_MAGNEMITE */
        w(26, 26, SpeciesId(101)), /* SPECIES_ELECTRODE */
        w(26, 26, SpeciesId(82)),  /* SPECIES_MAGNETON */
    ],
};

const GROUTE119_LAND: LandEncounters = LandEncounters {
    encounter_rate: 15,
    mons: [
        w(25, 25, SpeciesId(288)), /* SPECIES_ZIGZAGOON */
        w(25, 25, SpeciesId(289)), /* SPECIES_LINOONE */
        w(27, 27, SpeciesId(288)), /* SPECIES_ZIGZAGOON */
        w(25, 25, SpeciesId(43)),  /* SPECIES_ODDISH */
        w(27, 27, SpeciesId(289)), /* SPECIES_LINOONE */
        w(26, 26, SpeciesId(43)),  /* SPECIES_ODDISH */
        w(27, 27, SpeciesId(43)),  /* SPECIES_ODDISH */
        w(24, 24, SpeciesId(43)),  /* SPECIES_ODDISH */
        w(25, 25, SpeciesId(369)), /* SPECIES_TROPIUS */
        w(26, 26, SpeciesId(369)), /* SPECIES_TROPIUS */
        w(27, 27, SpeciesId(369)), /* SPECIES_TROPIUS */
        w(25, 25, SpeciesId(317)), /* SPECIES_KECLEON */
    ],
};

const GROUTE119_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(5, 35, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(309)), /* SPECIES_WINGULL */
        w(15, 25, SpeciesId(309)), /* SPECIES_WINGULL */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
    ],
};

const GROUTE119_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(330)), /* SPECIES_CARVANHA */
        w(25, 30, SpeciesId(330)), /* SPECIES_CARVANHA */
        w(30, 35, SpeciesId(330)), /* SPECIES_CARVANHA */
        w(20, 25, SpeciesId(330)), /* SPECIES_CARVANHA */
        w(35, 40, SpeciesId(330)), /* SPECIES_CARVANHA */
        w(40, 45, SpeciesId(330)), /* SPECIES_CARVANHA */
    ],
};

const GROUTE120_LAND: LandEncounters = LandEncounters {
    encounter_rate: 20,
    mons: [
        w(25, 25, SpeciesId(286)), /* SPECIES_POOCHYENA */
        w(25, 25, SpeciesId(287)), /* SPECIES_MIGHTYENA */
        w(27, 27, SpeciesId(287)), /* SPECIES_MIGHTYENA */
        w(25, 25, SpeciesId(43)),  /* SPECIES_ODDISH */
        w(25, 25, SpeciesId(183)), /* SPECIES_MARILL */
        w(26, 26, SpeciesId(43)),  /* SPECIES_ODDISH */
        w(27, 27, SpeciesId(43)),  /* SPECIES_ODDISH */
        w(27, 27, SpeciesId(183)), /* SPECIES_MARILL */
        w(25, 25, SpeciesId(376)), /* SPECIES_ABSOL */
        w(27, 27, SpeciesId(376)), /* SPECIES_ABSOL */
        w(25, 25, SpeciesId(317)), /* SPECIES_KECLEON */
        w(25, 25, SpeciesId(298)), /* SPECIES_SEEDOT */
    ],
};

const GROUTE120_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(20, 30, SpeciesId(183)), /* SPECIES_MARILL */
        w(10, 20, SpeciesId(183)), /* SPECIES_MARILL */
        w(30, 35, SpeciesId(183)), /* SPECIES_MARILL */
        w(5, 10, SpeciesId(183)),  /* SPECIES_MARILL */
        w(20, 30, SpeciesId(118)), /* SPECIES_GOLDEEN */
    ],
};

const GROUTE120_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(118)),  /* SPECIES_GOLDEEN */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(118)), /* SPECIES_GOLDEEN */
        w(10, 30, SpeciesId(323)), /* SPECIES_BARBOACH */
        w(25, 30, SpeciesId(323)), /* SPECIES_BARBOACH */
        w(30, 35, SpeciesId(323)), /* SPECIES_BARBOACH */
        w(20, 25, SpeciesId(323)), /* SPECIES_BARBOACH */
        w(35, 40, SpeciesId(323)), /* SPECIES_BARBOACH */
        w(40, 45, SpeciesId(323)), /* SPECIES_BARBOACH */
    ],
};

const GROUTE121_LAND: LandEncounters = LandEncounters {
    encounter_rate: 20,
    mons: [
        w(26, 26, SpeciesId(286)), /* SPECIES_POOCHYENA */
        w(26, 26, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(26, 26, SpeciesId(287)), /* SPECIES_MIGHTYENA */
        w(28, 28, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(28, 28, SpeciesId(287)), /* SPECIES_MIGHTYENA */
        w(26, 26, SpeciesId(43)),  /* SPECIES_ODDISH */
        w(28, 28, SpeciesId(43)),  /* SPECIES_ODDISH */
        w(28, 28, SpeciesId(44)),  /* SPECIES_GLOOM */
        w(26, 26, SpeciesId(309)), /* SPECIES_WINGULL */
        w(27, 27, SpeciesId(309)), /* SPECIES_WINGULL */
        w(28, 28, SpeciesId(309)), /* SPECIES_WINGULL */
        w(25, 25, SpeciesId(317)), /* SPECIES_KECLEON */
    ],
};

const GROUTE121_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(5, 35, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(309)), /* SPECIES_WINGULL */
        w(15, 25, SpeciesId(309)), /* SPECIES_WINGULL */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
    ],
};

const GROUTE121_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(25, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(30, 35, SpeciesId(313)), /* SPECIES_WAILMER */
        w(20, 25, SpeciesId(313)), /* SPECIES_WAILMER */
        w(35, 40, SpeciesId(313)), /* SPECIES_WAILMER */
        w(40, 45, SpeciesId(313)), /* SPECIES_WAILMER */
    ],
};

const GROUTE122_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(5, 35, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(309)), /* SPECIES_WINGULL */
        w(15, 25, SpeciesId(309)), /* SPECIES_WINGULL */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
    ],
};

const GROUTE122_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(30, 35, SpeciesId(331)), /* SPECIES_SHARPEDO */
        w(30, 35, SpeciesId(313)), /* SPECIES_WAILMER */
        w(25, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(35, 40, SpeciesId(313)), /* SPECIES_WAILMER */
        w(40, 45, SpeciesId(313)), /* SPECIES_WAILMER */
    ],
};

const GROUTE123_LAND: LandEncounters = LandEncounters {
    encounter_rate: 20,
    mons: [
        w(26, 26, SpeciesId(286)), /* SPECIES_POOCHYENA */
        w(26, 26, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(26, 26, SpeciesId(287)), /* SPECIES_MIGHTYENA */
        w(28, 28, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(28, 28, SpeciesId(287)), /* SPECIES_MIGHTYENA */
        w(26, 26, SpeciesId(43)),  /* SPECIES_ODDISH */
        w(28, 28, SpeciesId(43)),  /* SPECIES_ODDISH */
        w(28, 28, SpeciesId(44)),  /* SPECIES_GLOOM */
        w(26, 26, SpeciesId(309)), /* SPECIES_WINGULL */
        w(27, 27, SpeciesId(309)), /* SPECIES_WINGULL */
        w(28, 28, SpeciesId(309)), /* SPECIES_WINGULL */
        w(25, 25, SpeciesId(317)), /* SPECIES_KECLEON */
    ],
};

const GROUTE123_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(5, 35, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(309)), /* SPECIES_WINGULL */
        w(15, 25, SpeciesId(309)), /* SPECIES_WINGULL */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
    ],
};

const GROUTE123_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(25, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(30, 35, SpeciesId(313)), /* SPECIES_WAILMER */
        w(20, 25, SpeciesId(313)), /* SPECIES_WAILMER */
        w(35, 40, SpeciesId(313)), /* SPECIES_WAILMER */
        w(40, 45, SpeciesId(313)), /* SPECIES_WAILMER */
    ],
};

const GMTPYRE_2F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(27, 27, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(28, 28, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(26, 26, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(25, 25, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(29, 29, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(24, 24, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(23, 23, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(22, 22, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(29, 29, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(24, 24, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(29, 29, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(24, 24, SpeciesId(377)), /* SPECIES_SHUPPET */
    ],
};

const GMTPYRE_3F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(27, 27, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(28, 28, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(26, 26, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(25, 25, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(29, 29, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(24, 24, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(23, 23, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(22, 22, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(29, 29, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(24, 24, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(29, 29, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(24, 24, SpeciesId(377)), /* SPECIES_SHUPPET */
    ],
};

const GMTPYRE_4F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(27, 27, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(28, 28, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(26, 26, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(25, 25, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(29, 29, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(24, 24, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(23, 23, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(22, 22, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(27, 27, SpeciesId(361)), /* SPECIES_DUSKULL */
        w(27, 27, SpeciesId(361)), /* SPECIES_DUSKULL */
        w(25, 25, SpeciesId(361)), /* SPECIES_DUSKULL */
        w(29, 29, SpeciesId(361)), /* SPECIES_DUSKULL */
    ],
};

const GMTPYRE_5F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(27, 27, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(28, 28, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(26, 26, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(25, 25, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(29, 29, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(24, 24, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(23, 23, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(22, 22, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(27, 27, SpeciesId(361)), /* SPECIES_DUSKULL */
        w(27, 27, SpeciesId(361)), /* SPECIES_DUSKULL */
        w(25, 25, SpeciesId(361)), /* SPECIES_DUSKULL */
        w(29, 29, SpeciesId(361)), /* SPECIES_DUSKULL */
    ],
};

const GMTPYRE_6F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(27, 27, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(28, 28, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(26, 26, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(25, 25, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(29, 29, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(24, 24, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(23, 23, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(22, 22, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(27, 27, SpeciesId(361)), /* SPECIES_DUSKULL */
        w(27, 27, SpeciesId(361)), /* SPECIES_DUSKULL */
        w(25, 25, SpeciesId(361)), /* SPECIES_DUSKULL */
        w(29, 29, SpeciesId(361)), /* SPECIES_DUSKULL */
    ],
};

const GMTPYRE_EXTERIOR_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(27, 27, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(27, 27, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(28, 28, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(29, 29, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(29, 29, SpeciesId(37)),  /* SPECIES_VULPIX */
        w(27, 27, SpeciesId(37)),  /* SPECIES_VULPIX */
        w(29, 29, SpeciesId(37)),  /* SPECIES_VULPIX */
        w(25, 25, SpeciesId(37)),  /* SPECIES_VULPIX */
        w(27, 27, SpeciesId(309)), /* SPECIES_WINGULL */
        w(27, 27, SpeciesId(309)), /* SPECIES_WINGULL */
        w(26, 26, SpeciesId(309)), /* SPECIES_WINGULL */
        w(28, 28, SpeciesId(309)), /* SPECIES_WINGULL */
    ],
};

const GMTPYRE_SUMMIT_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(28, 28, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(29, 29, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(27, 27, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(26, 26, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(30, 30, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(25, 25, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(24, 24, SpeciesId(377)), /* SPECIES_SHUPPET */
        w(28, 28, SpeciesId(361)), /* SPECIES_DUSKULL */
        w(26, 26, SpeciesId(361)), /* SPECIES_DUSKULL */
        w(30, 30, SpeciesId(361)), /* SPECIES_DUSKULL */
        w(28, 28, SpeciesId(411)), /* SPECIES_CHIMECHO */
        w(28, 28, SpeciesId(411)), /* SPECIES_CHIMECHO */
    ],
};

const GGRANITECAVE_STEVENSROOM_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(7, 7, SpeciesId(41)),    /* SPECIES_ZUBAT */
        w(8, 8, SpeciesId(335)),   /* SPECIES_MAKUHITA */
        w(7, 7, SpeciesId(335)),   /* SPECIES_MAKUHITA */
        w(8, 8, SpeciesId(41)),    /* SPECIES_ZUBAT */
        w(9, 9, SpeciesId(335)),   /* SPECIES_MAKUHITA */
        w(8, 8, SpeciesId(63)),    /* SPECIES_ABRA */
        w(10, 10, SpeciesId(335)), /* SPECIES_MAKUHITA */
        w(6, 6, SpeciesId(335)),   /* SPECIES_MAKUHITA */
        w(7, 7, SpeciesId(382)),   /* SPECIES_ARON */
        w(8, 8, SpeciesId(382)),   /* SPECIES_ARON */
        w(7, 7, SpeciesId(382)),   /* SPECIES_ARON */
        w(8, 8, SpeciesId(382)),   /* SPECIES_ARON */
    ],
};

const GROUTE125_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(5, 35, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(309)), /* SPECIES_WINGULL */
        w(15, 25, SpeciesId(309)), /* SPECIES_WINGULL */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
    ],
};

const GROUTE125_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(30, 35, SpeciesId(331)), /* SPECIES_SHARPEDO */
        w(30, 35, SpeciesId(313)), /* SPECIES_WAILMER */
        w(25, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(35, 40, SpeciesId(313)), /* SPECIES_WAILMER */
        w(40, 45, SpeciesId(313)), /* SPECIES_WAILMER */
    ],
};

const GROUTE126_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(5, 35, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(309)), /* SPECIES_WINGULL */
        w(15, 25, SpeciesId(309)), /* SPECIES_WINGULL */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
    ],
};

const GROUTE126_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(30, 35, SpeciesId(331)), /* SPECIES_SHARPEDO */
        w(30, 35, SpeciesId(313)), /* SPECIES_WAILMER */
        w(25, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(35, 40, SpeciesId(313)), /* SPECIES_WAILMER */
        w(40, 45, SpeciesId(313)), /* SPECIES_WAILMER */
    ],
};

const GROUTE127_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(5, 35, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(309)), /* SPECIES_WINGULL */
        w(15, 25, SpeciesId(309)), /* SPECIES_WINGULL */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
    ],
};

const GROUTE127_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(30, 35, SpeciesId(331)), /* SPECIES_SHARPEDO */
        w(30, 35, SpeciesId(313)), /* SPECIES_WAILMER */
        w(25, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(35, 40, SpeciesId(313)), /* SPECIES_WAILMER */
        w(40, 45, SpeciesId(313)), /* SPECIES_WAILMER */
    ],
};

const GROUTE128_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(5, 35, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(309)), /* SPECIES_WINGULL */
        w(15, 25, SpeciesId(309)), /* SPECIES_WINGULL */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
    ],
};

const GROUTE128_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(325)), /* SPECIES_LUVDISC */
        w(10, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(30, 35, SpeciesId(325)), /* SPECIES_LUVDISC */
        w(30, 35, SpeciesId(313)), /* SPECIES_WAILMER */
        w(30, 35, SpeciesId(222)), /* SPECIES_CORSOLA */
        w(35, 40, SpeciesId(313)), /* SPECIES_WAILMER */
        w(40, 45, SpeciesId(313)), /* SPECIES_WAILMER */
    ],
};

const GROUTE129_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(5, 35, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(309)), /* SPECIES_WINGULL */
        w(15, 25, SpeciesId(309)), /* SPECIES_WINGULL */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
        w(25, 30, SpeciesId(314)), /* SPECIES_WAILORD */
    ],
};

const GROUTE129_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(30, 35, SpeciesId(331)), /* SPECIES_SHARPEDO */
        w(30, 35, SpeciesId(313)), /* SPECIES_WAILMER */
        w(25, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(35, 40, SpeciesId(313)), /* SPECIES_WAILMER */
        w(40, 45, SpeciesId(313)), /* SPECIES_WAILMER */
    ],
};

const GROUTE130_LAND: LandEncounters = LandEncounters {
    encounter_rate: 20,
    mons: [
        w(30, 30, SpeciesId(360)), /* SPECIES_WYNAUT */
        w(35, 35, SpeciesId(360)), /* SPECIES_WYNAUT */
        w(25, 25, SpeciesId(360)), /* SPECIES_WYNAUT */
        w(40, 40, SpeciesId(360)), /* SPECIES_WYNAUT */
        w(20, 20, SpeciesId(360)), /* SPECIES_WYNAUT */
        w(45, 45, SpeciesId(360)), /* SPECIES_WYNAUT */
        w(15, 15, SpeciesId(360)), /* SPECIES_WYNAUT */
        w(50, 50, SpeciesId(360)), /* SPECIES_WYNAUT */
        w(10, 10, SpeciesId(360)), /* SPECIES_WYNAUT */
        w(5, 5, SpeciesId(360)),   /* SPECIES_WYNAUT */
        w(10, 10, SpeciesId(360)), /* SPECIES_WYNAUT */
        w(5, 5, SpeciesId(360)),   /* SPECIES_WYNAUT */
    ],
};

const GROUTE130_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(5, 35, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(309)), /* SPECIES_WINGULL */
        w(15, 25, SpeciesId(309)), /* SPECIES_WINGULL */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
    ],
};

const GROUTE130_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(30, 35, SpeciesId(331)), /* SPECIES_SHARPEDO */
        w(30, 35, SpeciesId(313)), /* SPECIES_WAILMER */
        w(25, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(35, 40, SpeciesId(313)), /* SPECIES_WAILMER */
        w(40, 45, SpeciesId(313)), /* SPECIES_WAILMER */
    ],
};

const GROUTE131_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(5, 35, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(309)), /* SPECIES_WINGULL */
        w(15, 25, SpeciesId(309)), /* SPECIES_WINGULL */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
    ],
};

const GROUTE131_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(30, 35, SpeciesId(331)), /* SPECIES_SHARPEDO */
        w(30, 35, SpeciesId(313)), /* SPECIES_WAILMER */
        w(25, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(35, 40, SpeciesId(313)), /* SPECIES_WAILMER */
        w(40, 45, SpeciesId(313)), /* SPECIES_WAILMER */
    ],
};

const GROUTE132_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(5, 35, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(309)), /* SPECIES_WINGULL */
        w(15, 25, SpeciesId(309)), /* SPECIES_WINGULL */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
    ],
};

const GROUTE132_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(30, 35, SpeciesId(331)), /* SPECIES_SHARPEDO */
        w(30, 35, SpeciesId(313)), /* SPECIES_WAILMER */
        w(25, 30, SpeciesId(116)), /* SPECIES_HORSEA */
        w(35, 40, SpeciesId(313)), /* SPECIES_WAILMER */
        w(40, 45, SpeciesId(313)), /* SPECIES_WAILMER */
    ],
};

const GROUTE133_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(5, 35, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(309)), /* SPECIES_WINGULL */
        w(15, 25, SpeciesId(309)), /* SPECIES_WINGULL */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
    ],
};

const GROUTE133_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(30, 35, SpeciesId(331)), /* SPECIES_SHARPEDO */
        w(30, 35, SpeciesId(313)), /* SPECIES_WAILMER */
        w(25, 30, SpeciesId(116)), /* SPECIES_HORSEA */
        w(35, 40, SpeciesId(313)), /* SPECIES_WAILMER */
        w(40, 45, SpeciesId(313)), /* SPECIES_WAILMER */
    ],
};

const GROUTE134_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(5, 35, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(309)), /* SPECIES_WINGULL */
        w(15, 25, SpeciesId(309)), /* SPECIES_WINGULL */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
    ],
};

const GROUTE134_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(30, 35, SpeciesId(331)), /* SPECIES_SHARPEDO */
        w(30, 35, SpeciesId(313)), /* SPECIES_WAILMER */
        w(25, 30, SpeciesId(116)), /* SPECIES_HORSEA */
        w(35, 40, SpeciesId(313)), /* SPECIES_WAILMER */
        w(40, 45, SpeciesId(313)), /* SPECIES_WAILMER */
    ],
};

const GABANDONEDSHIP_HIDDENFLOORCORRIDORS_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(5, 35, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(5, 35, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(5, 35, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(5, 35, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(30, 35, SpeciesId(73)), /* SPECIES_TENTACRUEL */
    ],
};

const GABANDONEDSHIP_HIDDENFLOORCORRIDORS_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 20,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(25, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(30, 35, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(30, 35, SpeciesId(73)),  /* SPECIES_TENTACRUEL */
        w(25, 30, SpeciesId(73)),  /* SPECIES_TENTACRUEL */
        w(20, 25, SpeciesId(73)),  /* SPECIES_TENTACRUEL */
    ],
};

const GSEAFLOORCAVERN_ROOM1_LAND: LandEncounters = LandEncounters {
    encounter_rate: 4,
    mons: [
        w(30, 30, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(31, 31, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(32, 32, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(33, 33, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(28, 28, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(29, 29, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(34, 34, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(35, 35, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(34, 34, SpeciesId(42)), /* SPECIES_GOLBAT */
        w(35, 35, SpeciesId(42)), /* SPECIES_GOLBAT */
        w(33, 33, SpeciesId(42)), /* SPECIES_GOLBAT */
        w(36, 36, SpeciesId(42)), /* SPECIES_GOLBAT */
    ],
};

const GSEAFLOORCAVERN_ROOM2_LAND: LandEncounters = LandEncounters {
    encounter_rate: 4,
    mons: [
        w(30, 30, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(31, 31, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(32, 32, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(33, 33, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(28, 28, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(29, 29, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(34, 34, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(35, 35, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(34, 34, SpeciesId(42)), /* SPECIES_GOLBAT */
        w(35, 35, SpeciesId(42)), /* SPECIES_GOLBAT */
        w(33, 33, SpeciesId(42)), /* SPECIES_GOLBAT */
        w(36, 36, SpeciesId(42)), /* SPECIES_GOLBAT */
    ],
};

const GSEAFLOORCAVERN_ROOM3_LAND: LandEncounters = LandEncounters {
    encounter_rate: 4,
    mons: [
        w(30, 30, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(31, 31, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(32, 32, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(33, 33, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(28, 28, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(29, 29, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(34, 34, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(35, 35, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(34, 34, SpeciesId(42)), /* SPECIES_GOLBAT */
        w(35, 35, SpeciesId(42)), /* SPECIES_GOLBAT */
        w(33, 33, SpeciesId(42)), /* SPECIES_GOLBAT */
        w(36, 36, SpeciesId(42)), /* SPECIES_GOLBAT */
    ],
};

const GSEAFLOORCAVERN_ROOM4_LAND: LandEncounters = LandEncounters {
    encounter_rate: 4,
    mons: [
        w(30, 30, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(31, 31, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(32, 32, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(33, 33, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(28, 28, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(29, 29, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(34, 34, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(35, 35, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(34, 34, SpeciesId(42)), /* SPECIES_GOLBAT */
        w(35, 35, SpeciesId(42)), /* SPECIES_GOLBAT */
        w(33, 33, SpeciesId(42)), /* SPECIES_GOLBAT */
        w(36, 36, SpeciesId(42)), /* SPECIES_GOLBAT */
    ],
};

const GSEAFLOORCAVERN_ROOM5_LAND: LandEncounters = LandEncounters {
    encounter_rate: 4,
    mons: [
        w(30, 30, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(31, 31, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(32, 32, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(33, 33, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(28, 28, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(29, 29, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(34, 34, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(35, 35, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(34, 34, SpeciesId(42)), /* SPECIES_GOLBAT */
        w(35, 35, SpeciesId(42)), /* SPECIES_GOLBAT */
        w(33, 33, SpeciesId(42)), /* SPECIES_GOLBAT */
        w(36, 36, SpeciesId(42)), /* SPECIES_GOLBAT */
    ],
};

const GSEAFLOORCAVERN_ROOM6_LAND: LandEncounters = LandEncounters {
    encounter_rate: 4,
    mons: [
        w(30, 30, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(31, 31, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(32, 32, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(33, 33, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(28, 28, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(29, 29, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(34, 34, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(35, 35, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(34, 34, SpeciesId(42)), /* SPECIES_GOLBAT */
        w(35, 35, SpeciesId(42)), /* SPECIES_GOLBAT */
        w(33, 33, SpeciesId(42)), /* SPECIES_GOLBAT */
        w(36, 36, SpeciesId(42)), /* SPECIES_GOLBAT */
    ],
};

const GSEAFLOORCAVERN_ROOM6_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(5, 35, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(5, 35, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(30, 35, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(30, 35, SpeciesId(42)), /* SPECIES_GOLBAT */
        w(30, 35, SpeciesId(42)), /* SPECIES_GOLBAT */
    ],
};

const GSEAFLOORCAVERN_ROOM6_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 10,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(25, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(30, 35, SpeciesId(313)), /* SPECIES_WAILMER */
        w(20, 25, SpeciesId(313)), /* SPECIES_WAILMER */
        w(35, 40, SpeciesId(313)), /* SPECIES_WAILMER */
        w(40, 45, SpeciesId(313)), /* SPECIES_WAILMER */
    ],
};

const GSEAFLOORCAVERN_ROOM7_LAND: LandEncounters = LandEncounters {
    encounter_rate: 4,
    mons: [
        w(30, 30, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(31, 31, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(32, 32, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(33, 33, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(28, 28, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(29, 29, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(34, 34, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(35, 35, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(34, 34, SpeciesId(42)), /* SPECIES_GOLBAT */
        w(35, 35, SpeciesId(42)), /* SPECIES_GOLBAT */
        w(33, 33, SpeciesId(42)), /* SPECIES_GOLBAT */
        w(36, 36, SpeciesId(42)), /* SPECIES_GOLBAT */
    ],
};

const GSEAFLOORCAVERN_ROOM7_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(5, 35, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(5, 35, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(30, 35, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(30, 35, SpeciesId(42)), /* SPECIES_GOLBAT */
        w(30, 35, SpeciesId(42)), /* SPECIES_GOLBAT */
    ],
};

const GSEAFLOORCAVERN_ROOM7_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 10,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(25, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(30, 35, SpeciesId(313)), /* SPECIES_WAILMER */
        w(20, 25, SpeciesId(313)), /* SPECIES_WAILMER */
        w(35, 40, SpeciesId(313)), /* SPECIES_WAILMER */
        w(40, 45, SpeciesId(313)), /* SPECIES_WAILMER */
    ],
};

const GSEAFLOORCAVERN_ROOM8_LAND: LandEncounters = LandEncounters {
    encounter_rate: 4,
    mons: [
        w(30, 30, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(31, 31, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(32, 32, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(33, 33, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(28, 28, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(29, 29, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(34, 34, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(35, 35, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(34, 34, SpeciesId(42)), /* SPECIES_GOLBAT */
        w(35, 35, SpeciesId(42)), /* SPECIES_GOLBAT */
        w(33, 33, SpeciesId(42)), /* SPECIES_GOLBAT */
        w(36, 36, SpeciesId(42)), /* SPECIES_GOLBAT */
    ],
};

const GSEAFLOORCAVERN_ENTRANCE_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(5, 35, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(5, 35, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(30, 35, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(30, 35, SpeciesId(42)), /* SPECIES_GOLBAT */
        w(30, 35, SpeciesId(42)), /* SPECIES_GOLBAT */
    ],
};

const GSEAFLOORCAVERN_ENTRANCE_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 10,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(25, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(30, 35, SpeciesId(313)), /* SPECIES_WAILMER */
        w(20, 25, SpeciesId(313)), /* SPECIES_WAILMER */
        w(35, 40, SpeciesId(313)), /* SPECIES_WAILMER */
        w(40, 45, SpeciesId(313)), /* SPECIES_WAILMER */
    ],
};

const GCAVEOFORIGIN_ENTRANCE_LAND: LandEncounters = LandEncounters {
    encounter_rate: 4,
    mons: [
        w(30, 30, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(31, 31, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(32, 32, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(33, 33, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(28, 28, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(29, 29, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(34, 34, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(35, 35, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(34, 34, SpeciesId(42)), /* SPECIES_GOLBAT */
        w(35, 35, SpeciesId(42)), /* SPECIES_GOLBAT */
        w(33, 33, SpeciesId(42)), /* SPECIES_GOLBAT */
        w(36, 36, SpeciesId(42)), /* SPECIES_GOLBAT */
    ],
};

const GCAVEOFORIGIN_1F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 4,
    mons: [
        w(30, 30, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(31, 31, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(32, 32, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(30, 30, SpeciesId(322)), /* SPECIES_SABLEYE */
        w(32, 32, SpeciesId(322)), /* SPECIES_SABLEYE */
        w(34, 34, SpeciesId(322)), /* SPECIES_SABLEYE */
        w(33, 33, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(34, 34, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(34, 34, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(35, 35, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(33, 33, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(36, 36, SpeciesId(42)),  /* SPECIES_GOLBAT */
    ],
};

const GCAVEOFORIGIN_UNUSEDRUBYSAPPHIREMAP1_LAND: LandEncounters = LandEncounters {
    encounter_rate: 4,
    mons: [
        w(30, 30, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(31, 31, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(32, 32, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(30, 30, SpeciesId(322)), /* SPECIES_SABLEYE */
        w(32, 32, SpeciesId(322)), /* SPECIES_SABLEYE */
        w(34, 34, SpeciesId(322)), /* SPECIES_SABLEYE */
        w(33, 33, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(34, 34, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(34, 34, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(35, 35, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(33, 33, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(36, 36, SpeciesId(42)),  /* SPECIES_GOLBAT */
    ],
};

const GCAVEOFORIGIN_UNUSEDRUBYSAPPHIREMAP2_LAND: LandEncounters = LandEncounters {
    encounter_rate: 4,
    mons: [
        w(30, 30, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(31, 31, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(32, 32, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(30, 30, SpeciesId(322)), /* SPECIES_SABLEYE */
        w(32, 32, SpeciesId(322)), /* SPECIES_SABLEYE */
        w(34, 34, SpeciesId(322)), /* SPECIES_SABLEYE */
        w(33, 33, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(34, 34, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(34, 34, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(35, 35, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(33, 33, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(36, 36, SpeciesId(42)),  /* SPECIES_GOLBAT */
    ],
};

const GCAVEOFORIGIN_UNUSEDRUBYSAPPHIREMAP3_LAND: LandEncounters = LandEncounters {
    encounter_rate: 4,
    mons: [
        w(30, 30, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(31, 31, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(32, 32, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(30, 30, SpeciesId(322)), /* SPECIES_SABLEYE */
        w(32, 32, SpeciesId(322)), /* SPECIES_SABLEYE */
        w(34, 34, SpeciesId(322)), /* SPECIES_SABLEYE */
        w(33, 33, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(34, 34, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(34, 34, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(35, 35, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(33, 33, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(36, 36, SpeciesId(42)),  /* SPECIES_GOLBAT */
    ],
};

const GNEWMAUVILLE_ENTRANCE_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(24, 24, SpeciesId(100)), /* SPECIES_VOLTORB */
        w(24, 24, SpeciesId(81)),  /* SPECIES_MAGNEMITE */
        w(25, 25, SpeciesId(100)), /* SPECIES_VOLTORB */
        w(25, 25, SpeciesId(81)),  /* SPECIES_MAGNEMITE */
        w(23, 23, SpeciesId(100)), /* SPECIES_VOLTORB */
        w(23, 23, SpeciesId(81)),  /* SPECIES_MAGNEMITE */
        w(26, 26, SpeciesId(100)), /* SPECIES_VOLTORB */
        w(26, 26, SpeciesId(81)),  /* SPECIES_MAGNEMITE */
        w(22, 22, SpeciesId(100)), /* SPECIES_VOLTORB */
        w(22, 22, SpeciesId(81)),  /* SPECIES_MAGNEMITE */
        w(22, 22, SpeciesId(100)), /* SPECIES_VOLTORB */
        w(22, 22, SpeciesId(81)),  /* SPECIES_MAGNEMITE */
    ],
};

const GSAFARIZONE_SOUTHWEST_LAND: LandEncounters = LandEncounters {
    encounter_rate: 25,
    mons: [
        w(25, 25, SpeciesId(43)),  /* SPECIES_ODDISH */
        w(27, 27, SpeciesId(43)),  /* SPECIES_ODDISH */
        w(25, 25, SpeciesId(203)), /* SPECIES_GIRAFARIG */
        w(27, 27, SpeciesId(203)), /* SPECIES_GIRAFARIG */
        w(25, 25, SpeciesId(177)), /* SPECIES_NATU */
        w(27, 27, SpeciesId(84)),  /* SPECIES_DODUO */
        w(25, 25, SpeciesId(44)),  /* SPECIES_GLOOM */
        w(27, 27, SpeciesId(202)), /* SPECIES_WOBBUFFET */
        w(25, 25, SpeciesId(25)),  /* SPECIES_PIKACHU */
        w(27, 27, SpeciesId(202)), /* SPECIES_WOBBUFFET */
        w(27, 27, SpeciesId(25)),  /* SPECIES_PIKACHU */
        w(29, 29, SpeciesId(202)), /* SPECIES_WOBBUFFET */
    ],
};

const GSAFARIZONE_SOUTHWEST_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 9,
    mons: [
        w(20, 30, SpeciesId(54)), /* SPECIES_PSYDUCK */
        w(20, 30, SpeciesId(54)), /* SPECIES_PSYDUCK */
        w(30, 35, SpeciesId(54)), /* SPECIES_PSYDUCK */
        w(30, 35, SpeciesId(54)), /* SPECIES_PSYDUCK */
        w(30, 35, SpeciesId(54)), /* SPECIES_PSYDUCK */
    ],
};

const GSAFARIZONE_SOUTHWEST_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 35,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(118)),  /* SPECIES_GOLDEEN */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 25, SpeciesId(118)), /* SPECIES_GOLDEEN */
        w(10, 30, SpeciesId(118)), /* SPECIES_GOLDEEN */
        w(25, 30, SpeciesId(118)), /* SPECIES_GOLDEEN */
        w(30, 35, SpeciesId(118)), /* SPECIES_GOLDEEN */
        w(30, 35, SpeciesId(119)), /* SPECIES_SEAKING */
        w(35, 40, SpeciesId(119)), /* SPECIES_SEAKING */
        w(25, 30, SpeciesId(119)), /* SPECIES_SEAKING */
    ],
};

const GSAFARIZONE_NORTH_LAND: LandEncounters = LandEncounters {
    encounter_rate: 25,
    mons: [
        w(27, 27, SpeciesId(231)), /* SPECIES_PHANPY */
        w(27, 27, SpeciesId(43)),  /* SPECIES_ODDISH */
        w(29, 29, SpeciesId(231)), /* SPECIES_PHANPY */
        w(29, 29, SpeciesId(43)),  /* SPECIES_ODDISH */
        w(27, 27, SpeciesId(177)), /* SPECIES_NATU */
        w(29, 29, SpeciesId(44)),  /* SPECIES_GLOOM */
        w(31, 31, SpeciesId(44)),  /* SPECIES_GLOOM */
        w(29, 29, SpeciesId(177)), /* SPECIES_NATU */
        w(29, 29, SpeciesId(178)), /* SPECIES_XATU */
        w(27, 27, SpeciesId(214)), /* SPECIES_HERACROSS */
        w(31, 31, SpeciesId(178)), /* SPECIES_XATU */
        w(29, 29, SpeciesId(214)), /* SPECIES_HERACROSS */
    ],
};

const GSAFARIZONE_NORTH_ROCKSMASH: RockSmashEncounters = RockSmashEncounters {
    encounter_rate: 25,
    mons: [
        w(10, 15, SpeciesId(74)), /* SPECIES_GEODUDE */
        w(5, 10, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(15, 20, SpeciesId(74)), /* SPECIES_GEODUDE */
        w(20, 25, SpeciesId(74)), /* SPECIES_GEODUDE */
        w(25, 30, SpeciesId(74)), /* SPECIES_GEODUDE */
    ],
};

const GSAFARIZONE_NORTHWEST_LAND: LandEncounters = LandEncounters {
    encounter_rate: 25,
    mons: [
        w(27, 27, SpeciesId(111)), /* SPECIES_RHYHORN */
        w(27, 27, SpeciesId(43)),  /* SPECIES_ODDISH */
        w(29, 29, SpeciesId(111)), /* SPECIES_RHYHORN */
        w(29, 29, SpeciesId(43)),  /* SPECIES_ODDISH */
        w(27, 27, SpeciesId(84)),  /* SPECIES_DODUO */
        w(29, 29, SpeciesId(44)),  /* SPECIES_GLOOM */
        w(31, 31, SpeciesId(44)),  /* SPECIES_GLOOM */
        w(29, 29, SpeciesId(84)),  /* SPECIES_DODUO */
        w(29, 29, SpeciesId(85)),  /* SPECIES_DODRIO */
        w(27, 27, SpeciesId(127)), /* SPECIES_PINSIR */
        w(31, 31, SpeciesId(85)),  /* SPECIES_DODRIO */
        w(29, 29, SpeciesId(127)), /* SPECIES_PINSIR */
    ],
};

const GSAFARIZONE_NORTHWEST_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 9,
    mons: [
        w(20, 30, SpeciesId(54)), /* SPECIES_PSYDUCK */
        w(20, 30, SpeciesId(54)), /* SPECIES_PSYDUCK */
        w(30, 35, SpeciesId(54)), /* SPECIES_PSYDUCK */
        w(30, 35, SpeciesId(55)), /* SPECIES_GOLDUCK */
        w(25, 40, SpeciesId(55)), /* SPECIES_GOLDUCK */
    ],
};

const GSAFARIZONE_NORTHWEST_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 35,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(118)),  /* SPECIES_GOLDEEN */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 25, SpeciesId(118)), /* SPECIES_GOLDEEN */
        w(10, 30, SpeciesId(118)), /* SPECIES_GOLDEEN */
        w(25, 30, SpeciesId(118)), /* SPECIES_GOLDEEN */
        w(30, 35, SpeciesId(118)), /* SPECIES_GOLDEEN */
        w(30, 35, SpeciesId(119)), /* SPECIES_SEAKING */
        w(35, 40, SpeciesId(119)), /* SPECIES_SEAKING */
        w(25, 30, SpeciesId(119)), /* SPECIES_SEAKING */
    ],
};

const GVICTORYROAD_B1F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(40, 40, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(40, 40, SpeciesId(336)), /* SPECIES_HARIYAMA */
        w(40, 40, SpeciesId(383)), /* SPECIES_LAIRON */
        w(40, 40, SpeciesId(383)), /* SPECIES_LAIRON */
        w(38, 38, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(38, 38, SpeciesId(336)), /* SPECIES_HARIYAMA */
        w(42, 42, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(42, 42, SpeciesId(336)), /* SPECIES_HARIYAMA */
        w(42, 42, SpeciesId(383)), /* SPECIES_LAIRON */
        w(38, 38, SpeciesId(355)), /* SPECIES_MAWILE */
        w(42, 42, SpeciesId(383)), /* SPECIES_LAIRON */
        w(38, 38, SpeciesId(355)), /* SPECIES_MAWILE */
    ],
};

const GVICTORYROAD_B1F_ROCKSMASH: RockSmashEncounters = RockSmashEncounters {
    encounter_rate: 20,
    mons: [
        w(30, 40, SpeciesId(75)), /* SPECIES_GRAVELER */
        w(30, 40, SpeciesId(74)), /* SPECIES_GEODUDE */
        w(35, 40, SpeciesId(75)), /* SPECIES_GRAVELER */
        w(35, 40, SpeciesId(75)), /* SPECIES_GRAVELER */
        w(35, 40, SpeciesId(75)), /* SPECIES_GRAVELER */
    ],
};

const GVICTORYROAD_B2F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(40, 40, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(40, 40, SpeciesId(322)), /* SPECIES_SABLEYE */
        w(40, 40, SpeciesId(383)), /* SPECIES_LAIRON */
        w(40, 40, SpeciesId(383)), /* SPECIES_LAIRON */
        w(42, 42, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(42, 42, SpeciesId(322)), /* SPECIES_SABLEYE */
        w(44, 44, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(44, 44, SpeciesId(322)), /* SPECIES_SABLEYE */
        w(42, 42, SpeciesId(383)), /* SPECIES_LAIRON */
        w(42, 42, SpeciesId(355)), /* SPECIES_MAWILE */
        w(44, 44, SpeciesId(383)), /* SPECIES_LAIRON */
        w(44, 44, SpeciesId(355)), /* SPECIES_MAWILE */
    ],
};

const GVICTORYROAD_B2F_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(30, 35, SpeciesId(42)), /* SPECIES_GOLBAT */
        w(25, 30, SpeciesId(42)), /* SPECIES_GOLBAT */
        w(35, 40, SpeciesId(42)), /* SPECIES_GOLBAT */
        w(35, 40, SpeciesId(42)), /* SPECIES_GOLBAT */
        w(35, 40, SpeciesId(42)), /* SPECIES_GOLBAT */
    ],
};

const GVICTORYROAD_B2F_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(118)),  /* SPECIES_GOLDEEN */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(118)), /* SPECIES_GOLDEEN */
        w(10, 30, SpeciesId(323)), /* SPECIES_BARBOACH */
        w(25, 30, SpeciesId(323)), /* SPECIES_BARBOACH */
        w(30, 35, SpeciesId(323)), /* SPECIES_BARBOACH */
        w(30, 35, SpeciesId(324)), /* SPECIES_WHISCASH */
        w(35, 40, SpeciesId(324)), /* SPECIES_WHISCASH */
        w(40, 45, SpeciesId(324)), /* SPECIES_WHISCASH */
    ],
};

const GMETEORFALLS_1F_1R_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(16, 16, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(17, 17, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(18, 18, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(15, 15, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(14, 14, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(16, 16, SpeciesId(349)), /* SPECIES_SOLROCK */
        w(18, 18, SpeciesId(349)), /* SPECIES_SOLROCK */
        w(14, 14, SpeciesId(349)), /* SPECIES_SOLROCK */
        w(19, 19, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(20, 20, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(19, 19, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(20, 20, SpeciesId(41)),  /* SPECIES_ZUBAT */
    ],
};

const GMETEORFALLS_1F_1R_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(5, 35, SpeciesId(41)),   /* SPECIES_ZUBAT */
        w(30, 35, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(25, 35, SpeciesId(349)), /* SPECIES_SOLROCK */
        w(15, 25, SpeciesId(349)), /* SPECIES_SOLROCK */
        w(5, 15, SpeciesId(349)),  /* SPECIES_SOLROCK */
    ],
};

const GMETEORFALLS_1F_1R_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(118)),  /* SPECIES_GOLDEEN */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(118)), /* SPECIES_GOLDEEN */
        w(10, 30, SpeciesId(323)), /* SPECIES_BARBOACH */
        w(25, 30, SpeciesId(323)), /* SPECIES_BARBOACH */
        w(30, 35, SpeciesId(323)), /* SPECIES_BARBOACH */
        w(20, 25, SpeciesId(323)), /* SPECIES_BARBOACH */
        w(35, 40, SpeciesId(323)), /* SPECIES_BARBOACH */
        w(40, 45, SpeciesId(323)), /* SPECIES_BARBOACH */
    ],
};

const GMETEORFALLS_1F_2R_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(33, 33, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(35, 35, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(33, 33, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(35, 35, SpeciesId(349)), /* SPECIES_SOLROCK */
        w(33, 33, SpeciesId(349)), /* SPECIES_SOLROCK */
        w(37, 37, SpeciesId(349)), /* SPECIES_SOLROCK */
        w(35, 35, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(39, 39, SpeciesId(349)), /* SPECIES_SOLROCK */
        w(38, 38, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(40, 40, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(38, 38, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(40, 40, SpeciesId(42)),  /* SPECIES_GOLBAT */
    ],
};

const GMETEORFALLS_1F_2R_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(30, 35, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(30, 35, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(25, 35, SpeciesId(349)), /* SPECIES_SOLROCK */
        w(15, 25, SpeciesId(349)), /* SPECIES_SOLROCK */
        w(5, 15, SpeciesId(349)),  /* SPECIES_SOLROCK */
    ],
};

const GMETEORFALLS_1F_2R_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(118)),  /* SPECIES_GOLDEEN */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(118)), /* SPECIES_GOLDEEN */
        w(10, 30, SpeciesId(323)), /* SPECIES_BARBOACH */
        w(25, 30, SpeciesId(323)), /* SPECIES_BARBOACH */
        w(30, 35, SpeciesId(323)), /* SPECIES_BARBOACH */
        w(30, 35, SpeciesId(324)), /* SPECIES_WHISCASH */
        w(35, 40, SpeciesId(324)), /* SPECIES_WHISCASH */
        w(40, 45, SpeciesId(324)), /* SPECIES_WHISCASH */
    ],
};

const GMETEORFALLS_B1F_1R_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(33, 33, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(35, 35, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(33, 33, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(35, 35, SpeciesId(349)), /* SPECIES_SOLROCK */
        w(33, 33, SpeciesId(349)), /* SPECIES_SOLROCK */
        w(37, 37, SpeciesId(349)), /* SPECIES_SOLROCK */
        w(35, 35, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(39, 39, SpeciesId(349)), /* SPECIES_SOLROCK */
        w(38, 38, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(40, 40, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(38, 38, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(40, 40, SpeciesId(42)),  /* SPECIES_GOLBAT */
    ],
};

const GMETEORFALLS_B1F_1R_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(30, 35, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(30, 35, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(25, 35, SpeciesId(349)), /* SPECIES_SOLROCK */
        w(15, 25, SpeciesId(349)), /* SPECIES_SOLROCK */
        w(5, 15, SpeciesId(349)),  /* SPECIES_SOLROCK */
    ],
};

const GMETEORFALLS_B1F_1R_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(118)),  /* SPECIES_GOLDEEN */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(118)), /* SPECIES_GOLDEEN */
        w(10, 30, SpeciesId(323)), /* SPECIES_BARBOACH */
        w(25, 30, SpeciesId(323)), /* SPECIES_BARBOACH */
        w(30, 35, SpeciesId(323)), /* SPECIES_BARBOACH */
        w(30, 35, SpeciesId(324)), /* SPECIES_WHISCASH */
        w(35, 40, SpeciesId(324)), /* SPECIES_WHISCASH */
        w(40, 45, SpeciesId(324)), /* SPECIES_WHISCASH */
    ],
};

const GSHOALCAVE_LOWTIDESTAIRSROOM_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(26, 26, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(26, 26, SpeciesId(341)), /* SPECIES_SPHEAL */
        w(28, 28, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(28, 28, SpeciesId(341)), /* SPECIES_SPHEAL */
        w(30, 30, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(30, 30, SpeciesId(341)), /* SPECIES_SPHEAL */
        w(32, 32, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(32, 32, SpeciesId(341)), /* SPECIES_SPHEAL */
        w(32, 32, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(32, 32, SpeciesId(341)), /* SPECIES_SPHEAL */
        w(32, 32, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(32, 32, SpeciesId(341)), /* SPECIES_SPHEAL */
    ],
};

const GSHOALCAVE_LOWTIDELOWERROOM_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(26, 26, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(26, 26, SpeciesId(341)), /* SPECIES_SPHEAL */
        w(28, 28, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(28, 28, SpeciesId(341)), /* SPECIES_SPHEAL */
        w(30, 30, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(30, 30, SpeciesId(341)), /* SPECIES_SPHEAL */
        w(32, 32, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(32, 32, SpeciesId(341)), /* SPECIES_SPHEAL */
        w(32, 32, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(32, 32, SpeciesId(341)), /* SPECIES_SPHEAL */
        w(32, 32, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(32, 32, SpeciesId(341)), /* SPECIES_SPHEAL */
    ],
};

const GSHOALCAVE_LOWTIDEINNERROOM_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(26, 26, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(26, 26, SpeciesId(341)), /* SPECIES_SPHEAL */
        w(28, 28, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(28, 28, SpeciesId(341)), /* SPECIES_SPHEAL */
        w(30, 30, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(30, 30, SpeciesId(341)), /* SPECIES_SPHEAL */
        w(32, 32, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(32, 32, SpeciesId(341)), /* SPECIES_SPHEAL */
        w(32, 32, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(32, 32, SpeciesId(341)), /* SPECIES_SPHEAL */
        w(32, 32, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(32, 32, SpeciesId(341)), /* SPECIES_SPHEAL */
    ],
};

const GSHOALCAVE_LOWTIDEINNERROOM_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(5, 35, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(5, 35, SpeciesId(41)),   /* SPECIES_ZUBAT */
        w(25, 30, SpeciesId(341)), /* SPECIES_SPHEAL */
        w(25, 30, SpeciesId(341)), /* SPECIES_SPHEAL */
        w(25, 35, SpeciesId(341)), /* SPECIES_SPHEAL */
    ],
};

const GSHOALCAVE_LOWTIDEINNERROOM_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 10,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(25, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(30, 35, SpeciesId(313)), /* SPECIES_WAILMER */
        w(20, 25, SpeciesId(313)), /* SPECIES_WAILMER */
        w(35, 40, SpeciesId(313)), /* SPECIES_WAILMER */
        w(40, 45, SpeciesId(313)), /* SPECIES_WAILMER */
    ],
};

const GSHOALCAVE_LOWTIDEENTRANCEROOM_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(26, 26, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(26, 26, SpeciesId(341)), /* SPECIES_SPHEAL */
        w(28, 28, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(28, 28, SpeciesId(341)), /* SPECIES_SPHEAL */
        w(30, 30, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(30, 30, SpeciesId(341)), /* SPECIES_SPHEAL */
        w(32, 32, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(32, 32, SpeciesId(341)), /* SPECIES_SPHEAL */
        w(32, 32, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(32, 32, SpeciesId(341)), /* SPECIES_SPHEAL */
        w(32, 32, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(32, 32, SpeciesId(341)), /* SPECIES_SPHEAL */
    ],
};

const GSHOALCAVE_LOWTIDEENTRANCEROOM_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(5, 35, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(5, 35, SpeciesId(41)),   /* SPECIES_ZUBAT */
        w(25, 30, SpeciesId(341)), /* SPECIES_SPHEAL */
        w(25, 30, SpeciesId(341)), /* SPECIES_SPHEAL */
        w(25, 35, SpeciesId(341)), /* SPECIES_SPHEAL */
    ],
};

const GSHOALCAVE_LOWTIDEENTRANCEROOM_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 10,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(25, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(30, 35, SpeciesId(313)), /* SPECIES_WAILMER */
        w(20, 25, SpeciesId(313)), /* SPECIES_WAILMER */
        w(35, 40, SpeciesId(313)), /* SPECIES_WAILMER */
        w(40, 45, SpeciesId(313)), /* SPECIES_WAILMER */
    ],
};

const GLILYCOVECITY_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(5, 35, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(309)), /* SPECIES_WINGULL */
        w(15, 25, SpeciesId(309)), /* SPECIES_WINGULL */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
    ],
};

const GLILYCOVECITY_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 10,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(25, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(30, 35, SpeciesId(313)), /* SPECIES_WAILMER */
        w(25, 30, SpeciesId(120)), /* SPECIES_STARYU */
        w(35, 40, SpeciesId(313)), /* SPECIES_WAILMER */
        w(40, 45, SpeciesId(313)), /* SPECIES_WAILMER */
    ],
};

const GDEWFORDTOWN_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(5, 35, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(309)), /* SPECIES_WINGULL */
        w(15, 25, SpeciesId(309)), /* SPECIES_WINGULL */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
    ],
};

const GDEWFORDTOWN_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 10,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(25, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(30, 35, SpeciesId(313)), /* SPECIES_WAILMER */
        w(20, 25, SpeciesId(313)), /* SPECIES_WAILMER */
        w(35, 40, SpeciesId(313)), /* SPECIES_WAILMER */
        w(40, 45, SpeciesId(313)), /* SPECIES_WAILMER */
    ],
};

const GSLATEPORTCITY_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(5, 35, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(309)), /* SPECIES_WINGULL */
        w(15, 25, SpeciesId(309)), /* SPECIES_WINGULL */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
    ],
};

const GSLATEPORTCITY_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 10,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(25, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(30, 35, SpeciesId(313)), /* SPECIES_WAILMER */
        w(20, 25, SpeciesId(313)), /* SPECIES_WAILMER */
        w(35, 40, SpeciesId(313)), /* SPECIES_WAILMER */
        w(40, 45, SpeciesId(313)), /* SPECIES_WAILMER */
    ],
};

const GMOSSDEEPCITY_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(5, 35, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(309)), /* SPECIES_WINGULL */
        w(15, 25, SpeciesId(309)), /* SPECIES_WINGULL */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
    ],
};

const GMOSSDEEPCITY_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 10,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(30, 35, SpeciesId(331)), /* SPECIES_SHARPEDO */
        w(30, 35, SpeciesId(313)), /* SPECIES_WAILMER */
        w(25, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(35, 40, SpeciesId(313)), /* SPECIES_WAILMER */
        w(40, 45, SpeciesId(313)), /* SPECIES_WAILMER */
    ],
};

const GPACIFIDLOGTOWN_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(5, 35, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(309)), /* SPECIES_WINGULL */
        w(15, 25, SpeciesId(309)), /* SPECIES_WINGULL */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
    ],
};

const GPACIFIDLOGTOWN_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 10,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(72)),  /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(30, 35, SpeciesId(331)), /* SPECIES_SHARPEDO */
        w(30, 35, SpeciesId(313)), /* SPECIES_WAILMER */
        w(25, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(35, 40, SpeciesId(313)), /* SPECIES_WAILMER */
        w(40, 45, SpeciesId(313)), /* SPECIES_WAILMER */
    ],
};

const GEVERGRANDECITY_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(5, 35, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(309)), /* SPECIES_WINGULL */
        w(15, 25, SpeciesId(309)), /* SPECIES_WINGULL */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
        w(25, 30, SpeciesId(310)), /* SPECIES_PELIPPER */
    ],
};

const GEVERGRANDECITY_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 10,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(325)), /* SPECIES_LUVDISC */
        w(10, 30, SpeciesId(313)), /* SPECIES_WAILMER */
        w(30, 35, SpeciesId(325)), /* SPECIES_LUVDISC */
        w(30, 35, SpeciesId(313)), /* SPECIES_WAILMER */
        w(30, 35, SpeciesId(222)), /* SPECIES_CORSOLA */
        w(35, 40, SpeciesId(313)), /* SPECIES_WAILMER */
        w(40, 45, SpeciesId(313)), /* SPECIES_WAILMER */
    ],
};

const GPETALBURGCITY_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 1,
    mons: [
        w(20, 30, SpeciesId(183)), /* SPECIES_MARILL */
        w(10, 20, SpeciesId(183)), /* SPECIES_MARILL */
        w(30, 35, SpeciesId(183)), /* SPECIES_MARILL */
        w(5, 10, SpeciesId(183)),  /* SPECIES_MARILL */
        w(5, 10, SpeciesId(183)),  /* SPECIES_MARILL */
    ],
};

const GPETALBURGCITY_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 10,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(118)),  /* SPECIES_GOLDEEN */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(118)), /* SPECIES_GOLDEEN */
        w(10, 30, SpeciesId(326)), /* SPECIES_CORPHISH */
        w(25, 30, SpeciesId(326)), /* SPECIES_CORPHISH */
        w(30, 35, SpeciesId(326)), /* SPECIES_CORPHISH */
        w(20, 25, SpeciesId(326)), /* SPECIES_CORPHISH */
        w(35, 40, SpeciesId(326)), /* SPECIES_CORPHISH */
        w(40, 45, SpeciesId(326)), /* SPECIES_CORPHISH */
    ],
};

const GUNDERWATER_ROUTE124_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        w(20, 30, SpeciesId(373)), /* SPECIES_CLAMPERL */
        w(20, 30, SpeciesId(170)), /* SPECIES_CHINCHOU */
        w(30, 35, SpeciesId(373)), /* SPECIES_CLAMPERL */
        w(30, 35, SpeciesId(381)), /* SPECIES_RELICANTH */
        w(30, 35, SpeciesId(381)), /* SPECIES_RELICANTH */
    ],
};

const GSHOALCAVE_LOWTIDEICEROOM_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(26, 26, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(26, 26, SpeciesId(341)), /* SPECIES_SPHEAL */
        w(28, 28, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(28, 28, SpeciesId(341)), /* SPECIES_SPHEAL */
        w(30, 30, SpeciesId(41)),  /* SPECIES_ZUBAT */
        w(30, 30, SpeciesId(341)), /* SPECIES_SPHEAL */
        w(26, 26, SpeciesId(346)), /* SPECIES_SNORUNT */
        w(32, 32, SpeciesId(341)), /* SPECIES_SPHEAL */
        w(30, 30, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(28, 28, SpeciesId(346)), /* SPECIES_SNORUNT */
        w(32, 32, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(30, 30, SpeciesId(346)), /* SPECIES_SNORUNT */
    ],
};

const GSKYPILLAR_1F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(33, 33, SpeciesId(322)), /* SPECIES_SABLEYE */
        w(34, 34, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(35, 35, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(34, 34, SpeciesId(322)), /* SPECIES_SABLEYE */
        w(36, 36, SpeciesId(319)), /* SPECIES_CLAYDOL */
        w(37, 37, SpeciesId(378)), /* SPECIES_BANETTE */
        w(38, 38, SpeciesId(378)), /* SPECIES_BANETTE */
        w(36, 36, SpeciesId(319)), /* SPECIES_CLAYDOL */
        w(37, 37, SpeciesId(319)), /* SPECIES_CLAYDOL */
        w(38, 38, SpeciesId(319)), /* SPECIES_CLAYDOL */
        w(37, 37, SpeciesId(319)), /* SPECIES_CLAYDOL */
        w(38, 38, SpeciesId(319)), /* SPECIES_CLAYDOL */
    ],
};

const GSOOTOPOLISCITY_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 1,
    mons: [
        w(5, 35, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(15, 25, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(25, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(25, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
    ],
};

const GSOOTOPOLISCITY_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 10,
    mons: [
        w(5, 10, SpeciesId(129)),  /* SPECIES_MAGIKARP */
        w(5, 10, SpeciesId(72)),   /* SPECIES_TENTACOOL */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(10, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(30, 35, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(30, 35, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(35, 40, SpeciesId(130)), /* SPECIES_GYARADOS */
        w(35, 45, SpeciesId(130)), /* SPECIES_GYARADOS */
        w(5, 45, SpeciesId(130)),  /* SPECIES_GYARADOS */
    ],
};

const GSKYPILLAR_3F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(33, 33, SpeciesId(322)), /* SPECIES_SABLEYE */
        w(34, 34, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(35, 35, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(34, 34, SpeciesId(322)), /* SPECIES_SABLEYE */
        w(36, 36, SpeciesId(319)), /* SPECIES_CLAYDOL */
        w(37, 37, SpeciesId(378)), /* SPECIES_BANETTE */
        w(38, 38, SpeciesId(378)), /* SPECIES_BANETTE */
        w(36, 36, SpeciesId(319)), /* SPECIES_CLAYDOL */
        w(37, 37, SpeciesId(319)), /* SPECIES_CLAYDOL */
        w(38, 38, SpeciesId(319)), /* SPECIES_CLAYDOL */
        w(37, 37, SpeciesId(319)), /* SPECIES_CLAYDOL */
        w(38, 38, SpeciesId(319)), /* SPECIES_CLAYDOL */
    ],
};

const GSKYPILLAR_5F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(33, 33, SpeciesId(322)), /* SPECIES_SABLEYE */
        w(34, 34, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(35, 35, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(34, 34, SpeciesId(322)), /* SPECIES_SABLEYE */
        w(36, 36, SpeciesId(319)), /* SPECIES_CLAYDOL */
        w(37, 37, SpeciesId(378)), /* SPECIES_BANETTE */
        w(38, 38, SpeciesId(378)), /* SPECIES_BANETTE */
        w(36, 36, SpeciesId(319)), /* SPECIES_CLAYDOL */
        w(37, 37, SpeciesId(319)), /* SPECIES_CLAYDOL */
        w(38, 38, SpeciesId(359)), /* SPECIES_ALTARIA */
        w(39, 39, SpeciesId(359)), /* SPECIES_ALTARIA */
        w(39, 39, SpeciesId(359)), /* SPECIES_ALTARIA */
    ],
};

const GSAFARIZONE_SOUTHEAST_LAND: LandEncounters = LandEncounters {
    encounter_rate: 25,
    mons: [
        w(33, 33, SpeciesId(191)), /* SPECIES_SUNKERN */
        w(34, 34, SpeciesId(179)), /* SPECIES_MAREEP */
        w(35, 35, SpeciesId(191)), /* SPECIES_SUNKERN */
        w(36, 36, SpeciesId(179)), /* SPECIES_MAREEP */
        w(34, 34, SpeciesId(190)), /* SPECIES_AIPOM */
        w(33, 33, SpeciesId(167)), /* SPECIES_SPINARAK */
        w(35, 35, SpeciesId(163)), /* SPECIES_HOOTHOOT */
        w(34, 34, SpeciesId(209)), /* SPECIES_SNUBBULL */
        w(36, 36, SpeciesId(234)), /* SPECIES_STANTLER */
        w(37, 37, SpeciesId(207)), /* SPECIES_GLIGAR */
        w(39, 39, SpeciesId(234)), /* SPECIES_STANTLER */
        w(40, 40, SpeciesId(207)), /* SPECIES_GLIGAR */
    ],
};

const GSAFARIZONE_SOUTHEAST_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 9,
    mons: [
        w(25, 30, SpeciesId(194)), /* SPECIES_WOOPER */
        w(25, 30, SpeciesId(183)), /* SPECIES_MARILL */
        w(25, 30, SpeciesId(183)), /* SPECIES_MARILL */
        w(30, 35, SpeciesId(183)), /* SPECIES_MARILL */
        w(35, 40, SpeciesId(195)), /* SPECIES_QUAGSIRE */
    ],
};

const GSAFARIZONE_SOUTHEAST_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 35,
    mons: [
        w(25, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(25, 30, SpeciesId(118)), /* SPECIES_GOLDEEN */
        w(25, 30, SpeciesId(129)), /* SPECIES_MAGIKARP */
        w(25, 30, SpeciesId(118)), /* SPECIES_GOLDEEN */
        w(30, 35, SpeciesId(223)), /* SPECIES_REMORAID */
        w(25, 30, SpeciesId(118)), /* SPECIES_GOLDEEN */
        w(25, 30, SpeciesId(223)), /* SPECIES_REMORAID */
        w(30, 35, SpeciesId(223)), /* SPECIES_REMORAID */
        w(30, 35, SpeciesId(223)), /* SPECIES_REMORAID */
        w(35, 40, SpeciesId(224)), /* SPECIES_OCTILLERY */
    ],
};

const GSAFARIZONE_NORTHEAST_LAND: LandEncounters = LandEncounters {
    encounter_rate: 25,
    mons: [
        w(33, 33, SpeciesId(190)), /* SPECIES_AIPOM */
        w(34, 34, SpeciesId(216)), /* SPECIES_TEDDIURSA */
        w(35, 35, SpeciesId(190)), /* SPECIES_AIPOM */
        w(36, 36, SpeciesId(216)), /* SPECIES_TEDDIURSA */
        w(34, 34, SpeciesId(191)), /* SPECIES_SUNKERN */
        w(33, 33, SpeciesId(165)), /* SPECIES_LEDYBA */
        w(35, 35, SpeciesId(163)), /* SPECIES_HOOTHOOT */
        w(34, 34, SpeciesId(204)), /* SPECIES_PINECO */
        w(36, 36, SpeciesId(228)), /* SPECIES_HOUNDOUR */
        w(37, 37, SpeciesId(241)), /* SPECIES_MILTANK */
        w(39, 39, SpeciesId(228)), /* SPECIES_HOUNDOUR */
        w(40, 40, SpeciesId(241)), /* SPECIES_MILTANK */
    ],
};

const GSAFARIZONE_NORTHEAST_ROCKSMASH: RockSmashEncounters = RockSmashEncounters {
    encounter_rate: 25,
    mons: [
        w(25, 30, SpeciesId(213)), /* SPECIES_SHUCKLE */
        w(20, 25, SpeciesId(213)), /* SPECIES_SHUCKLE */
        w(30, 35, SpeciesId(213)), /* SPECIES_SHUCKLE */
        w(30, 35, SpeciesId(213)), /* SPECIES_SHUCKLE */
        w(35, 40, SpeciesId(213)), /* SPECIES_SHUCKLE */
    ],
};

const GMAGMAHIDEOUT_1F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(27, 27, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(28, 28, SpeciesId(321)), /* SPECIES_TORKOAL */
        w(28, 28, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(30, 30, SpeciesId(321)), /* SPECIES_TORKOAL */
        w(29, 29, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(30, 30, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(30, 30, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(30, 30, SpeciesId(75)),  /* SPECIES_GRAVELER */
        w(30, 30, SpeciesId(75)),  /* SPECIES_GRAVELER */
        w(31, 31, SpeciesId(75)),  /* SPECIES_GRAVELER */
        w(32, 32, SpeciesId(75)),  /* SPECIES_GRAVELER */
        w(33, 33, SpeciesId(75)),  /* SPECIES_GRAVELER */
    ],
};

const GMAGMAHIDEOUT_2F_1R_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(27, 27, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(28, 28, SpeciesId(321)), /* SPECIES_TORKOAL */
        w(28, 28, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(30, 30, SpeciesId(321)), /* SPECIES_TORKOAL */
        w(29, 29, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(30, 30, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(30, 30, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(30, 30, SpeciesId(75)),  /* SPECIES_GRAVELER */
        w(30, 30, SpeciesId(75)),  /* SPECIES_GRAVELER */
        w(31, 31, SpeciesId(75)),  /* SPECIES_GRAVELER */
        w(32, 32, SpeciesId(75)),  /* SPECIES_GRAVELER */
        w(33, 33, SpeciesId(75)),  /* SPECIES_GRAVELER */
    ],
};

const GMAGMAHIDEOUT_2F_2R_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(27, 27, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(28, 28, SpeciesId(321)), /* SPECIES_TORKOAL */
        w(28, 28, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(30, 30, SpeciesId(321)), /* SPECIES_TORKOAL */
        w(29, 29, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(30, 30, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(30, 30, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(30, 30, SpeciesId(75)),  /* SPECIES_GRAVELER */
        w(30, 30, SpeciesId(75)),  /* SPECIES_GRAVELER */
        w(31, 31, SpeciesId(75)),  /* SPECIES_GRAVELER */
        w(32, 32, SpeciesId(75)),  /* SPECIES_GRAVELER */
        w(33, 33, SpeciesId(75)),  /* SPECIES_GRAVELER */
    ],
};

const GMAGMAHIDEOUT_3F_1R_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(27, 27, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(28, 28, SpeciesId(321)), /* SPECIES_TORKOAL */
        w(28, 28, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(30, 30, SpeciesId(321)), /* SPECIES_TORKOAL */
        w(29, 29, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(30, 30, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(30, 30, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(30, 30, SpeciesId(75)),  /* SPECIES_GRAVELER */
        w(30, 30, SpeciesId(75)),  /* SPECIES_GRAVELER */
        w(31, 31, SpeciesId(75)),  /* SPECIES_GRAVELER */
        w(32, 32, SpeciesId(75)),  /* SPECIES_GRAVELER */
        w(33, 33, SpeciesId(75)),  /* SPECIES_GRAVELER */
    ],
};

const GMAGMAHIDEOUT_3F_2R_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(27, 27, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(28, 28, SpeciesId(321)), /* SPECIES_TORKOAL */
        w(28, 28, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(30, 30, SpeciesId(321)), /* SPECIES_TORKOAL */
        w(29, 29, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(30, 30, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(30, 30, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(30, 30, SpeciesId(75)),  /* SPECIES_GRAVELER */
        w(30, 30, SpeciesId(75)),  /* SPECIES_GRAVELER */
        w(31, 31, SpeciesId(75)),  /* SPECIES_GRAVELER */
        w(32, 32, SpeciesId(75)),  /* SPECIES_GRAVELER */
        w(33, 33, SpeciesId(75)),  /* SPECIES_GRAVELER */
    ],
};

const GMAGMAHIDEOUT_4F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(27, 27, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(28, 28, SpeciesId(321)), /* SPECIES_TORKOAL */
        w(28, 28, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(30, 30, SpeciesId(321)), /* SPECIES_TORKOAL */
        w(29, 29, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(30, 30, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(30, 30, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(30, 30, SpeciesId(75)),  /* SPECIES_GRAVELER */
        w(30, 30, SpeciesId(75)),  /* SPECIES_GRAVELER */
        w(31, 31, SpeciesId(75)),  /* SPECIES_GRAVELER */
        w(32, 32, SpeciesId(75)),  /* SPECIES_GRAVELER */
        w(33, 33, SpeciesId(75)),  /* SPECIES_GRAVELER */
    ],
};

const GMAGMAHIDEOUT_3F_3R_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(27, 27, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(28, 28, SpeciesId(321)), /* SPECIES_TORKOAL */
        w(28, 28, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(30, 30, SpeciesId(321)), /* SPECIES_TORKOAL */
        w(29, 29, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(30, 30, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(30, 30, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(30, 30, SpeciesId(75)),  /* SPECIES_GRAVELER */
        w(30, 30, SpeciesId(75)),  /* SPECIES_GRAVELER */
        w(31, 31, SpeciesId(75)),  /* SPECIES_GRAVELER */
        w(32, 32, SpeciesId(75)),  /* SPECIES_GRAVELER */
        w(33, 33, SpeciesId(75)),  /* SPECIES_GRAVELER */
    ],
};

const GMAGMAHIDEOUT_2F_3R_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(27, 27, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(28, 28, SpeciesId(321)), /* SPECIES_TORKOAL */
        w(28, 28, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(30, 30, SpeciesId(321)), /* SPECIES_TORKOAL */
        w(29, 29, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(30, 30, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(30, 30, SpeciesId(74)),  /* SPECIES_GEODUDE */
        w(30, 30, SpeciesId(75)),  /* SPECIES_GRAVELER */
        w(30, 30, SpeciesId(75)),  /* SPECIES_GRAVELER */
        w(31, 31, SpeciesId(75)),  /* SPECIES_GRAVELER */
        w(32, 32, SpeciesId(75)),  /* SPECIES_GRAVELER */
        w(33, 33, SpeciesId(75)),  /* SPECIES_GRAVELER */
    ],
};

const GMIRAGETOWER_1F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(21, 21, SpeciesId(27)),  /* SPECIES_SANDSHREW */
        w(21, 21, SpeciesId(332)), /* SPECIES_TRAPINCH */
        w(20, 20, SpeciesId(27)),  /* SPECIES_SANDSHREW */
        w(20, 20, SpeciesId(332)), /* SPECIES_TRAPINCH */
        w(20, 20, SpeciesId(27)),  /* SPECIES_SANDSHREW */
        w(20, 20, SpeciesId(332)), /* SPECIES_TRAPINCH */
        w(22, 22, SpeciesId(27)),  /* SPECIES_SANDSHREW */
        w(22, 22, SpeciesId(332)), /* SPECIES_TRAPINCH */
        w(23, 23, SpeciesId(27)),  /* SPECIES_SANDSHREW */
        w(23, 23, SpeciesId(332)), /* SPECIES_TRAPINCH */
        w(24, 24, SpeciesId(27)),  /* SPECIES_SANDSHREW */
        w(24, 24, SpeciesId(332)), /* SPECIES_TRAPINCH */
    ],
};

const GMIRAGETOWER_2F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(21, 21, SpeciesId(27)),  /* SPECIES_SANDSHREW */
        w(21, 21, SpeciesId(332)), /* SPECIES_TRAPINCH */
        w(20, 20, SpeciesId(27)),  /* SPECIES_SANDSHREW */
        w(20, 20, SpeciesId(332)), /* SPECIES_TRAPINCH */
        w(20, 20, SpeciesId(27)),  /* SPECIES_SANDSHREW */
        w(20, 20, SpeciesId(332)), /* SPECIES_TRAPINCH */
        w(22, 22, SpeciesId(27)),  /* SPECIES_SANDSHREW */
        w(22, 22, SpeciesId(332)), /* SPECIES_TRAPINCH */
        w(23, 23, SpeciesId(27)),  /* SPECIES_SANDSHREW */
        w(23, 23, SpeciesId(332)), /* SPECIES_TRAPINCH */
        w(24, 24, SpeciesId(27)),  /* SPECIES_SANDSHREW */
        w(24, 24, SpeciesId(332)), /* SPECIES_TRAPINCH */
    ],
};

const GMIRAGETOWER_3F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(21, 21, SpeciesId(27)),  /* SPECIES_SANDSHREW */
        w(21, 21, SpeciesId(332)), /* SPECIES_TRAPINCH */
        w(20, 20, SpeciesId(27)),  /* SPECIES_SANDSHREW */
        w(20, 20, SpeciesId(332)), /* SPECIES_TRAPINCH */
        w(20, 20, SpeciesId(27)),  /* SPECIES_SANDSHREW */
        w(20, 20, SpeciesId(332)), /* SPECIES_TRAPINCH */
        w(22, 22, SpeciesId(27)),  /* SPECIES_SANDSHREW */
        w(22, 22, SpeciesId(332)), /* SPECIES_TRAPINCH */
        w(23, 23, SpeciesId(27)),  /* SPECIES_SANDSHREW */
        w(23, 23, SpeciesId(332)), /* SPECIES_TRAPINCH */
        w(24, 24, SpeciesId(27)),  /* SPECIES_SANDSHREW */
        w(24, 24, SpeciesId(332)), /* SPECIES_TRAPINCH */
    ],
};

const GMIRAGETOWER_4F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(21, 21, SpeciesId(27)),  /* SPECIES_SANDSHREW */
        w(21, 21, SpeciesId(332)), /* SPECIES_TRAPINCH */
        w(20, 20, SpeciesId(27)),  /* SPECIES_SANDSHREW */
        w(20, 20, SpeciesId(332)), /* SPECIES_TRAPINCH */
        w(20, 20, SpeciesId(27)),  /* SPECIES_SANDSHREW */
        w(20, 20, SpeciesId(332)), /* SPECIES_TRAPINCH */
        w(22, 22, SpeciesId(27)),  /* SPECIES_SANDSHREW */
        w(22, 22, SpeciesId(332)), /* SPECIES_TRAPINCH */
        w(23, 23, SpeciesId(27)),  /* SPECIES_SANDSHREW */
        w(23, 23, SpeciesId(332)), /* SPECIES_TRAPINCH */
        w(24, 24, SpeciesId(27)),  /* SPECIES_SANDSHREW */
        w(24, 24, SpeciesId(332)), /* SPECIES_TRAPINCH */
    ],
};

const GDESERTUNDERPASS_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(38, 38, SpeciesId(132)), /* SPECIES_DITTO */
        w(35, 35, SpeciesId(370)), /* SPECIES_WHISMUR */
        w(40, 40, SpeciesId(132)), /* SPECIES_DITTO */
        w(40, 40, SpeciesId(371)), /* SPECIES_LOUDRED */
        w(41, 41, SpeciesId(132)), /* SPECIES_DITTO */
        w(36, 36, SpeciesId(370)), /* SPECIES_WHISMUR */
        w(38, 38, SpeciesId(371)), /* SPECIES_LOUDRED */
        w(42, 42, SpeciesId(132)), /* SPECIES_DITTO */
        w(38, 38, SpeciesId(370)), /* SPECIES_WHISMUR */
        w(43, 43, SpeciesId(132)), /* SPECIES_DITTO */
        w(44, 44, SpeciesId(371)), /* SPECIES_LOUDRED */
        w(45, 45, SpeciesId(132)), /* SPECIES_DITTO */
    ],
};

const GARTISANCAVE_B1F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(40, 40, SpeciesId(235)), /* SPECIES_SMEARGLE */
        w(41, 41, SpeciesId(235)), /* SPECIES_SMEARGLE */
        w(42, 42, SpeciesId(235)), /* SPECIES_SMEARGLE */
        w(43, 43, SpeciesId(235)), /* SPECIES_SMEARGLE */
        w(44, 44, SpeciesId(235)), /* SPECIES_SMEARGLE */
        w(45, 45, SpeciesId(235)), /* SPECIES_SMEARGLE */
        w(46, 46, SpeciesId(235)), /* SPECIES_SMEARGLE */
        w(47, 47, SpeciesId(235)), /* SPECIES_SMEARGLE */
        w(48, 48, SpeciesId(235)), /* SPECIES_SMEARGLE */
        w(49, 49, SpeciesId(235)), /* SPECIES_SMEARGLE */
        w(50, 50, SpeciesId(235)), /* SPECIES_SMEARGLE */
        w(50, 50, SpeciesId(235)), /* SPECIES_SMEARGLE */
    ],
};

const GARTISANCAVE_1F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(40, 40, SpeciesId(235)), /* SPECIES_SMEARGLE */
        w(41, 41, SpeciesId(235)), /* SPECIES_SMEARGLE */
        w(42, 42, SpeciesId(235)), /* SPECIES_SMEARGLE */
        w(43, 43, SpeciesId(235)), /* SPECIES_SMEARGLE */
        w(44, 44, SpeciesId(235)), /* SPECIES_SMEARGLE */
        w(45, 45, SpeciesId(235)), /* SPECIES_SMEARGLE */
        w(46, 46, SpeciesId(235)), /* SPECIES_SMEARGLE */
        w(47, 47, SpeciesId(235)), /* SPECIES_SMEARGLE */
        w(48, 48, SpeciesId(235)), /* SPECIES_SMEARGLE */
        w(49, 49, SpeciesId(235)), /* SPECIES_SMEARGLE */
        w(50, 50, SpeciesId(235)), /* SPECIES_SMEARGLE */
        w(50, 50, SpeciesId(235)), /* SPECIES_SMEARGLE */
    ],
};

const GALTERINGCAVE1_LAND: LandEncounters = LandEncounters {
    encounter_rate: 7,
    mons: [
        w(10, 10, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(12, 12, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(8, 8, SpeciesId(41)),   /* SPECIES_ZUBAT */
        w(14, 14, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(10, 10, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(12, 12, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(16, 16, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(6, 6, SpeciesId(41)),   /* SPECIES_ZUBAT */
        w(8, 8, SpeciesId(41)),   /* SPECIES_ZUBAT */
        w(14, 14, SpeciesId(41)), /* SPECIES_ZUBAT */
        w(8, 8, SpeciesId(41)),   /* SPECIES_ZUBAT */
        w(14, 14, SpeciesId(41)), /* SPECIES_ZUBAT */
    ],
};

const GALTERINGCAVE2_LAND: LandEncounters = LandEncounters {
    encounter_rate: 7,
    mons: [
        w(7, 7, SpeciesId(179)),   /* SPECIES_MAREEP */
        w(9, 9, SpeciesId(179)),   /* SPECIES_MAREEP */
        w(5, 5, SpeciesId(179)),   /* SPECIES_MAREEP */
        w(11, 11, SpeciesId(179)), /* SPECIES_MAREEP */
        w(7, 7, SpeciesId(179)),   /* SPECIES_MAREEP */
        w(9, 9, SpeciesId(179)),   /* SPECIES_MAREEP */
        w(13, 13, SpeciesId(179)), /* SPECIES_MAREEP */
        w(3, 3, SpeciesId(179)),   /* SPECIES_MAREEP */
        w(5, 5, SpeciesId(179)),   /* SPECIES_MAREEP */
        w(11, 11, SpeciesId(179)), /* SPECIES_MAREEP */
        w(5, 5, SpeciesId(179)),   /* SPECIES_MAREEP */
        w(11, 11, SpeciesId(179)), /* SPECIES_MAREEP */
    ],
};

const GALTERINGCAVE3_LAND: LandEncounters = LandEncounters {
    encounter_rate: 7,
    mons: [
        w(23, 23, SpeciesId(204)), /* SPECIES_PINECO */
        w(25, 25, SpeciesId(204)), /* SPECIES_PINECO */
        w(22, 22, SpeciesId(204)), /* SPECIES_PINECO */
        w(27, 27, SpeciesId(204)), /* SPECIES_PINECO */
        w(23, 23, SpeciesId(204)), /* SPECIES_PINECO */
        w(25, 25, SpeciesId(204)), /* SPECIES_PINECO */
        w(29, 29, SpeciesId(204)), /* SPECIES_PINECO */
        w(19, 19, SpeciesId(204)), /* SPECIES_PINECO */
        w(21, 21, SpeciesId(204)), /* SPECIES_PINECO */
        w(27, 27, SpeciesId(204)), /* SPECIES_PINECO */
        w(21, 21, SpeciesId(204)), /* SPECIES_PINECO */
        w(27, 27, SpeciesId(204)), /* SPECIES_PINECO */
    ],
};

const GALTERINGCAVE4_LAND: LandEncounters = LandEncounters {
    encounter_rate: 7,
    mons: [
        w(16, 16, SpeciesId(228)), /* SPECIES_HOUNDOUR */
        w(18, 18, SpeciesId(228)), /* SPECIES_HOUNDOUR */
        w(14, 14, SpeciesId(228)), /* SPECIES_HOUNDOUR */
        w(20, 20, SpeciesId(228)), /* SPECIES_HOUNDOUR */
        w(16, 16, SpeciesId(228)), /* SPECIES_HOUNDOUR */
        w(18, 18, SpeciesId(228)), /* SPECIES_HOUNDOUR */
        w(22, 22, SpeciesId(228)), /* SPECIES_HOUNDOUR */
        w(12, 12, SpeciesId(228)), /* SPECIES_HOUNDOUR */
        w(14, 14, SpeciesId(228)), /* SPECIES_HOUNDOUR */
        w(20, 20, SpeciesId(228)), /* SPECIES_HOUNDOUR */
        w(14, 14, SpeciesId(228)), /* SPECIES_HOUNDOUR */
        w(20, 20, SpeciesId(228)), /* SPECIES_HOUNDOUR */
    ],
};

const GALTERINGCAVE5_LAND: LandEncounters = LandEncounters {
    encounter_rate: 7,
    mons: [
        w(10, 10, SpeciesId(216)), /* SPECIES_TEDDIURSA */
        w(12, 12, SpeciesId(216)), /* SPECIES_TEDDIURSA */
        w(8, 8, SpeciesId(216)),   /* SPECIES_TEDDIURSA */
        w(14, 14, SpeciesId(216)), /* SPECIES_TEDDIURSA */
        w(10, 10, SpeciesId(216)), /* SPECIES_TEDDIURSA */
        w(12, 12, SpeciesId(216)), /* SPECIES_TEDDIURSA */
        w(16, 16, SpeciesId(216)), /* SPECIES_TEDDIURSA */
        w(6, 6, SpeciesId(216)),   /* SPECIES_TEDDIURSA */
        w(8, 8, SpeciesId(216)),   /* SPECIES_TEDDIURSA */
        w(14, 14, SpeciesId(216)), /* SPECIES_TEDDIURSA */
        w(8, 8, SpeciesId(216)),   /* SPECIES_TEDDIURSA */
        w(14, 14, SpeciesId(216)), /* SPECIES_TEDDIURSA */
    ],
};

const GALTERINGCAVE6_LAND: LandEncounters = LandEncounters {
    encounter_rate: 7,
    mons: [
        w(22, 22, SpeciesId(190)), /* SPECIES_AIPOM */
        w(24, 24, SpeciesId(190)), /* SPECIES_AIPOM */
        w(20, 20, SpeciesId(190)), /* SPECIES_AIPOM */
        w(26, 26, SpeciesId(190)), /* SPECIES_AIPOM */
        w(22, 22, SpeciesId(190)), /* SPECIES_AIPOM */
        w(24, 24, SpeciesId(190)), /* SPECIES_AIPOM */
        w(28, 28, SpeciesId(190)), /* SPECIES_AIPOM */
        w(18, 18, SpeciesId(190)), /* SPECIES_AIPOM */
        w(20, 20, SpeciesId(190)), /* SPECIES_AIPOM */
        w(26, 26, SpeciesId(190)), /* SPECIES_AIPOM */
        w(20, 20, SpeciesId(190)), /* SPECIES_AIPOM */
        w(26, 26, SpeciesId(190)), /* SPECIES_AIPOM */
    ],
};

const GALTERINGCAVE7_LAND: LandEncounters = LandEncounters {
    encounter_rate: 7,
    mons: [
        w(22, 22, SpeciesId(213)), /* SPECIES_SHUCKLE */
        w(24, 24, SpeciesId(213)), /* SPECIES_SHUCKLE */
        w(20, 20, SpeciesId(213)), /* SPECIES_SHUCKLE */
        w(26, 26, SpeciesId(213)), /* SPECIES_SHUCKLE */
        w(22, 22, SpeciesId(213)), /* SPECIES_SHUCKLE */
        w(24, 24, SpeciesId(213)), /* SPECIES_SHUCKLE */
        w(28, 28, SpeciesId(213)), /* SPECIES_SHUCKLE */
        w(18, 18, SpeciesId(213)), /* SPECIES_SHUCKLE */
        w(20, 20, SpeciesId(213)), /* SPECIES_SHUCKLE */
        w(26, 26, SpeciesId(213)), /* SPECIES_SHUCKLE */
        w(20, 20, SpeciesId(213)), /* SPECIES_SHUCKLE */
        w(26, 26, SpeciesId(213)), /* SPECIES_SHUCKLE */
    ],
};

const GALTERINGCAVE8_LAND: LandEncounters = LandEncounters {
    encounter_rate: 7,
    mons: [
        w(22, 22, SpeciesId(234)), /* SPECIES_STANTLER */
        w(24, 24, SpeciesId(234)), /* SPECIES_STANTLER */
        w(20, 20, SpeciesId(234)), /* SPECIES_STANTLER */
        w(26, 26, SpeciesId(234)), /* SPECIES_STANTLER */
        w(22, 22, SpeciesId(234)), /* SPECIES_STANTLER */
        w(24, 24, SpeciesId(234)), /* SPECIES_STANTLER */
        w(28, 28, SpeciesId(234)), /* SPECIES_STANTLER */
        w(18, 18, SpeciesId(234)), /* SPECIES_STANTLER */
        w(20, 20, SpeciesId(234)), /* SPECIES_STANTLER */
        w(26, 26, SpeciesId(234)), /* SPECIES_STANTLER */
        w(20, 20, SpeciesId(234)), /* SPECIES_STANTLER */
        w(26, 26, SpeciesId(234)), /* SPECIES_STANTLER */
    ],
};

const GALTERINGCAVE9_LAND: LandEncounters = LandEncounters {
    encounter_rate: 7,
    mons: [
        w(22, 22, SpeciesId(235)), /* SPECIES_SMEARGLE */
        w(24, 24, SpeciesId(235)), /* SPECIES_SMEARGLE */
        w(20, 20, SpeciesId(235)), /* SPECIES_SMEARGLE */
        w(26, 26, SpeciesId(235)), /* SPECIES_SMEARGLE */
        w(22, 22, SpeciesId(235)), /* SPECIES_SMEARGLE */
        w(24, 24, SpeciesId(235)), /* SPECIES_SMEARGLE */
        w(28, 28, SpeciesId(235)), /* SPECIES_SMEARGLE */
        w(18, 18, SpeciesId(235)), /* SPECIES_SMEARGLE */
        w(20, 20, SpeciesId(235)), /* SPECIES_SMEARGLE */
        w(26, 26, SpeciesId(235)), /* SPECIES_SMEARGLE */
        w(20, 20, SpeciesId(235)), /* SPECIES_SMEARGLE */
        w(26, 26, SpeciesId(235)), /* SPECIES_SMEARGLE */
    ],
};

const GMETEORFALLS_STEVENSCAVE_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        w(33, 33, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(35, 35, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(33, 33, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(35, 35, SpeciesId(349)), /* SPECIES_SOLROCK */
        w(33, 33, SpeciesId(349)), /* SPECIES_SOLROCK */
        w(37, 37, SpeciesId(349)), /* SPECIES_SOLROCK */
        w(35, 35, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(39, 39, SpeciesId(349)), /* SPECIES_SOLROCK */
        w(38, 38, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(40, 40, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(38, 38, SpeciesId(42)),  /* SPECIES_GOLBAT */
        w(40, 40, SpeciesId(42)),  /* SPECIES_GOLBAT */
    ],
};

pub(crate) static HEADERS: [WildEncounterHeader; MAP_HEADER_COUNT] = [
    WildEncounterHeader {
        map: MapId("MAP_ROUTE101"),
        label: "gRoute101",
        land: Some(GROUTE101_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_ROUTE102"),
        label: "gRoute102",
        land: Some(GROUTE102_LAND),
        water: Some(GROUTE102_WATER),
        rock_smash: None,
        fishing: Some(GROUTE102_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_ROUTE103"),
        label: "gRoute103",
        land: Some(GROUTE103_LAND),
        water: Some(GROUTE103_WATER),
        rock_smash: None,
        fishing: Some(GROUTE103_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_ROUTE104"),
        label: "gRoute104",
        land: Some(GROUTE104_LAND),
        water: Some(GROUTE104_WATER),
        rock_smash: None,
        fishing: Some(GROUTE104_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_ROUTE105"),
        label: "gRoute105",
        land: None,
        water: Some(GROUTE105_WATER),
        rock_smash: None,
        fishing: Some(GROUTE105_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_ROUTE110"),
        label: "gRoute110",
        land: Some(GROUTE110_LAND),
        water: Some(GROUTE110_WATER),
        rock_smash: None,
        fishing: Some(GROUTE110_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_ROUTE111"),
        label: "gRoute111",
        land: Some(GROUTE111_LAND),
        water: Some(GROUTE111_WATER),
        rock_smash: Some(GROUTE111_ROCKSMASH),
        fishing: Some(GROUTE111_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_ROUTE112"),
        label: "gRoute112",
        land: Some(GROUTE112_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_ROUTE113"),
        label: "gRoute113",
        land: Some(GROUTE113_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_ROUTE114"),
        label: "gRoute114",
        land: Some(GROUTE114_LAND),
        water: Some(GROUTE114_WATER),
        rock_smash: Some(GROUTE114_ROCKSMASH),
        fishing: Some(GROUTE114_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_ROUTE116"),
        label: "gRoute116",
        land: Some(GROUTE116_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_ROUTE117"),
        label: "gRoute117",
        land: Some(GROUTE117_LAND),
        water: Some(GROUTE117_WATER),
        rock_smash: None,
        fishing: Some(GROUTE117_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_ROUTE118"),
        label: "gRoute118",
        land: Some(GROUTE118_LAND),
        water: Some(GROUTE118_WATER),
        rock_smash: None,
        fishing: Some(GROUTE118_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_ROUTE124"),
        label: "gRoute124",
        land: None,
        water: Some(GROUTE124_WATER),
        rock_smash: None,
        fishing: Some(GROUTE124_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_PETALBURG_WOODS"),
        label: "gPetalburgWoods",
        land: Some(GPETALBURGWOODS_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_RUSTURF_TUNNEL"),
        label: "gRusturfTunnel",
        land: Some(GRUSTURFTUNNEL_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_GRANITE_CAVE_1F"),
        label: "gGraniteCave_1F",
        land: Some(GGRANITECAVE_1F_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_GRANITE_CAVE_B1F"),
        label: "gGraniteCave_B1F",
        land: Some(GGRANITECAVE_B1F_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_MT_PYRE_1F"),
        label: "gMtPyre_1F",
        land: Some(GMTPYRE_1F_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_VICTORY_ROAD_1F"),
        label: "gVictoryRoad_1F",
        land: Some(GVICTORYROAD_1F_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_SAFARI_ZONE_SOUTH"),
        label: "gSafariZone_South",
        land: Some(GSAFARIZONE_SOUTH_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_UNDERWATER_ROUTE126"),
        label: "gUnderwater_Route126",
        land: None,
        water: Some(GUNDERWATER_ROUTE126_WATER),
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_ABANDONED_SHIP_ROOMS_B1F"),
        label: "gAbandonedShip_Rooms_B1F",
        land: None,
        water: Some(GABANDONEDSHIP_ROOMS_B1F_WATER),
        rock_smash: None,
        fishing: Some(GABANDONEDSHIP_ROOMS_B1F_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_GRANITE_CAVE_B2F"),
        label: "gGraniteCave_B2F",
        land: Some(GGRANITECAVE_B2F_LAND),
        water: None,
        rock_smash: Some(GGRANITECAVE_B2F_ROCKSMASH),
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_FIERY_PATH"),
        label: "gFieryPath",
        land: Some(GFIERYPATH_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_METEOR_FALLS_B1F_2R"),
        label: "gMeteorFalls_B1F_2R",
        land: Some(GMETEORFALLS_B1F_2R_LAND),
        water: Some(GMETEORFALLS_B1F_2R_WATER),
        rock_smash: None,
        fishing: Some(GMETEORFALLS_B1F_2R_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_JAGGED_PASS"),
        label: "gJaggedPass",
        land: Some(GJAGGEDPASS_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_ROUTE106"),
        label: "gRoute106",
        land: None,
        water: Some(GROUTE106_WATER),
        rock_smash: None,
        fishing: Some(GROUTE106_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_ROUTE107"),
        label: "gRoute107",
        land: None,
        water: Some(GROUTE107_WATER),
        rock_smash: None,
        fishing: Some(GROUTE107_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_ROUTE108"),
        label: "gRoute108",
        land: None,
        water: Some(GROUTE108_WATER),
        rock_smash: None,
        fishing: Some(GROUTE108_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_ROUTE109"),
        label: "gRoute109",
        land: None,
        water: Some(GROUTE109_WATER),
        rock_smash: None,
        fishing: Some(GROUTE109_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_ROUTE115"),
        label: "gRoute115",
        land: Some(GROUTE115_LAND),
        water: Some(GROUTE115_WATER),
        rock_smash: None,
        fishing: Some(GROUTE115_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_NEW_MAUVILLE_INSIDE"),
        label: "gNewMauville_Inside",
        land: Some(GNEWMAUVILLE_INSIDE_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_ROUTE119"),
        label: "gRoute119",
        land: Some(GROUTE119_LAND),
        water: Some(GROUTE119_WATER),
        rock_smash: None,
        fishing: Some(GROUTE119_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_ROUTE120"),
        label: "gRoute120",
        land: Some(GROUTE120_LAND),
        water: Some(GROUTE120_WATER),
        rock_smash: None,
        fishing: Some(GROUTE120_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_ROUTE121"),
        label: "gRoute121",
        land: Some(GROUTE121_LAND),
        water: Some(GROUTE121_WATER),
        rock_smash: None,
        fishing: Some(GROUTE121_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_ROUTE122"),
        label: "gRoute122",
        land: None,
        water: Some(GROUTE122_WATER),
        rock_smash: None,
        fishing: Some(GROUTE122_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_ROUTE123"),
        label: "gRoute123",
        land: Some(GROUTE123_LAND),
        water: Some(GROUTE123_WATER),
        rock_smash: None,
        fishing: Some(GROUTE123_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_MT_PYRE_2F"),
        label: "gMtPyre_2F",
        land: Some(GMTPYRE_2F_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_MT_PYRE_3F"),
        label: "gMtPyre_3F",
        land: Some(GMTPYRE_3F_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_MT_PYRE_4F"),
        label: "gMtPyre_4F",
        land: Some(GMTPYRE_4F_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_MT_PYRE_5F"),
        label: "gMtPyre_5F",
        land: Some(GMTPYRE_5F_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_MT_PYRE_6F"),
        label: "gMtPyre_6F",
        land: Some(GMTPYRE_6F_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_MT_PYRE_EXTERIOR"),
        label: "gMtPyre_Exterior",
        land: Some(GMTPYRE_EXTERIOR_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_MT_PYRE_SUMMIT"),
        label: "gMtPyre_Summit",
        land: Some(GMTPYRE_SUMMIT_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_GRANITE_CAVE_STEVENS_ROOM"),
        label: "gGraniteCave_StevensRoom",
        land: Some(GGRANITECAVE_STEVENSROOM_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_ROUTE125"),
        label: "gRoute125",
        land: None,
        water: Some(GROUTE125_WATER),
        rock_smash: None,
        fishing: Some(GROUTE125_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_ROUTE126"),
        label: "gRoute126",
        land: None,
        water: Some(GROUTE126_WATER),
        rock_smash: None,
        fishing: Some(GROUTE126_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_ROUTE127"),
        label: "gRoute127",
        land: None,
        water: Some(GROUTE127_WATER),
        rock_smash: None,
        fishing: Some(GROUTE127_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_ROUTE128"),
        label: "gRoute128",
        land: None,
        water: Some(GROUTE128_WATER),
        rock_smash: None,
        fishing: Some(GROUTE128_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_ROUTE129"),
        label: "gRoute129",
        land: None,
        water: Some(GROUTE129_WATER),
        rock_smash: None,
        fishing: Some(GROUTE129_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_ROUTE130"),
        label: "gRoute130",
        land: Some(GROUTE130_LAND),
        water: Some(GROUTE130_WATER),
        rock_smash: None,
        fishing: Some(GROUTE130_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_ROUTE131"),
        label: "gRoute131",
        land: None,
        water: Some(GROUTE131_WATER),
        rock_smash: None,
        fishing: Some(GROUTE131_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_ROUTE132"),
        label: "gRoute132",
        land: None,
        water: Some(GROUTE132_WATER),
        rock_smash: None,
        fishing: Some(GROUTE132_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_ROUTE133"),
        label: "gRoute133",
        land: None,
        water: Some(GROUTE133_WATER),
        rock_smash: None,
        fishing: Some(GROUTE133_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_ROUTE134"),
        label: "gRoute134",
        land: None,
        water: Some(GROUTE134_WATER),
        rock_smash: None,
        fishing: Some(GROUTE134_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_ABANDONED_SHIP_HIDDEN_FLOOR_CORRIDORS"),
        label: "gAbandonedShip_HiddenFloorCorridors",
        land: None,
        water: Some(GABANDONEDSHIP_HIDDENFLOORCORRIDORS_WATER),
        rock_smash: None,
        fishing: Some(GABANDONEDSHIP_HIDDENFLOORCORRIDORS_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_SEAFLOOR_CAVERN_ROOM1"),
        label: "gSeafloorCavern_Room1",
        land: Some(GSEAFLOORCAVERN_ROOM1_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_SEAFLOOR_CAVERN_ROOM2"),
        label: "gSeafloorCavern_Room2",
        land: Some(GSEAFLOORCAVERN_ROOM2_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_SEAFLOOR_CAVERN_ROOM3"),
        label: "gSeafloorCavern_Room3",
        land: Some(GSEAFLOORCAVERN_ROOM3_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_SEAFLOOR_CAVERN_ROOM4"),
        label: "gSeafloorCavern_Room4",
        land: Some(GSEAFLOORCAVERN_ROOM4_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_SEAFLOOR_CAVERN_ROOM5"),
        label: "gSeafloorCavern_Room5",
        land: Some(GSEAFLOORCAVERN_ROOM5_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_SEAFLOOR_CAVERN_ROOM6"),
        label: "gSeafloorCavern_Room6",
        land: Some(GSEAFLOORCAVERN_ROOM6_LAND),
        water: Some(GSEAFLOORCAVERN_ROOM6_WATER),
        rock_smash: None,
        fishing: Some(GSEAFLOORCAVERN_ROOM6_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_SEAFLOOR_CAVERN_ROOM7"),
        label: "gSeafloorCavern_Room7",
        land: Some(GSEAFLOORCAVERN_ROOM7_LAND),
        water: Some(GSEAFLOORCAVERN_ROOM7_WATER),
        rock_smash: None,
        fishing: Some(GSEAFLOORCAVERN_ROOM7_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_SEAFLOOR_CAVERN_ROOM8"),
        label: "gSeafloorCavern_Room8",
        land: Some(GSEAFLOORCAVERN_ROOM8_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_SEAFLOOR_CAVERN_ENTRANCE"),
        label: "gSeafloorCavern_Entrance",
        land: None,
        water: Some(GSEAFLOORCAVERN_ENTRANCE_WATER),
        rock_smash: None,
        fishing: Some(GSEAFLOORCAVERN_ENTRANCE_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_CAVE_OF_ORIGIN_ENTRANCE"),
        label: "gCaveOfOrigin_Entrance",
        land: Some(GCAVEOFORIGIN_ENTRANCE_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_CAVE_OF_ORIGIN_1F"),
        label: "gCaveOfOrigin_1F",
        land: Some(GCAVEOFORIGIN_1F_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_CAVE_OF_ORIGIN_UNUSED_RUBY_SAPPHIRE_MAP1"),
        label: "gCaveOfOrigin_UnusedRubySapphireMap1",
        land: Some(GCAVEOFORIGIN_UNUSEDRUBYSAPPHIREMAP1_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_CAVE_OF_ORIGIN_UNUSED_RUBY_SAPPHIRE_MAP2"),
        label: "gCaveOfOrigin_UnusedRubySapphireMap2",
        land: Some(GCAVEOFORIGIN_UNUSEDRUBYSAPPHIREMAP2_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_CAVE_OF_ORIGIN_UNUSED_RUBY_SAPPHIRE_MAP3"),
        label: "gCaveOfOrigin_UnusedRubySapphireMap3",
        land: Some(GCAVEOFORIGIN_UNUSEDRUBYSAPPHIREMAP3_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_NEW_MAUVILLE_ENTRANCE"),
        label: "gNewMauville_Entrance",
        land: Some(GNEWMAUVILLE_ENTRANCE_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_SAFARI_ZONE_SOUTHWEST"),
        label: "gSafariZone_Southwest",
        land: Some(GSAFARIZONE_SOUTHWEST_LAND),
        water: Some(GSAFARIZONE_SOUTHWEST_WATER),
        rock_smash: None,
        fishing: Some(GSAFARIZONE_SOUTHWEST_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_SAFARI_ZONE_NORTH"),
        label: "gSafariZone_North",
        land: Some(GSAFARIZONE_NORTH_LAND),
        water: None,
        rock_smash: Some(GSAFARIZONE_NORTH_ROCKSMASH),
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_SAFARI_ZONE_NORTHWEST"),
        label: "gSafariZone_Northwest",
        land: Some(GSAFARIZONE_NORTHWEST_LAND),
        water: Some(GSAFARIZONE_NORTHWEST_WATER),
        rock_smash: None,
        fishing: Some(GSAFARIZONE_NORTHWEST_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_VICTORY_ROAD_B1F"),
        label: "gVictoryRoad_B1F",
        land: Some(GVICTORYROAD_B1F_LAND),
        water: None,
        rock_smash: Some(GVICTORYROAD_B1F_ROCKSMASH),
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_VICTORY_ROAD_B2F"),
        label: "gVictoryRoad_B2F",
        land: Some(GVICTORYROAD_B2F_LAND),
        water: Some(GVICTORYROAD_B2F_WATER),
        rock_smash: None,
        fishing: Some(GVICTORYROAD_B2F_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_METEOR_FALLS_1F_1R"),
        label: "gMeteorFalls_1F_1R",
        land: Some(GMETEORFALLS_1F_1R_LAND),
        water: Some(GMETEORFALLS_1F_1R_WATER),
        rock_smash: None,
        fishing: Some(GMETEORFALLS_1F_1R_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_METEOR_FALLS_1F_2R"),
        label: "gMeteorFalls_1F_2R",
        land: Some(GMETEORFALLS_1F_2R_LAND),
        water: Some(GMETEORFALLS_1F_2R_WATER),
        rock_smash: None,
        fishing: Some(GMETEORFALLS_1F_2R_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_METEOR_FALLS_B1F_1R"),
        label: "gMeteorFalls_B1F_1R",
        land: Some(GMETEORFALLS_B1F_1R_LAND),
        water: Some(GMETEORFALLS_B1F_1R_WATER),
        rock_smash: None,
        fishing: Some(GMETEORFALLS_B1F_1R_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_SHOAL_CAVE_LOW_TIDE_STAIRS_ROOM"),
        label: "gShoalCave_LowTideStairsRoom",
        land: Some(GSHOALCAVE_LOWTIDESTAIRSROOM_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_SHOAL_CAVE_LOW_TIDE_LOWER_ROOM"),
        label: "gShoalCave_LowTideLowerRoom",
        land: Some(GSHOALCAVE_LOWTIDELOWERROOM_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_SHOAL_CAVE_LOW_TIDE_INNER_ROOM"),
        label: "gShoalCave_LowTideInnerRoom",
        land: Some(GSHOALCAVE_LOWTIDEINNERROOM_LAND),
        water: Some(GSHOALCAVE_LOWTIDEINNERROOM_WATER),
        rock_smash: None,
        fishing: Some(GSHOALCAVE_LOWTIDEINNERROOM_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_SHOAL_CAVE_LOW_TIDE_ENTRANCE_ROOM"),
        label: "gShoalCave_LowTideEntranceRoom",
        land: Some(GSHOALCAVE_LOWTIDEENTRANCEROOM_LAND),
        water: Some(GSHOALCAVE_LOWTIDEENTRANCEROOM_WATER),
        rock_smash: None,
        fishing: Some(GSHOALCAVE_LOWTIDEENTRANCEROOM_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_LILYCOVE_CITY"),
        label: "gLilycoveCity",
        land: None,
        water: Some(GLILYCOVECITY_WATER),
        rock_smash: None,
        fishing: Some(GLILYCOVECITY_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_DEWFORD_TOWN"),
        label: "gDewfordTown",
        land: None,
        water: Some(GDEWFORDTOWN_WATER),
        rock_smash: None,
        fishing: Some(GDEWFORDTOWN_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_SLATEPORT_CITY"),
        label: "gSlateportCity",
        land: None,
        water: Some(GSLATEPORTCITY_WATER),
        rock_smash: None,
        fishing: Some(GSLATEPORTCITY_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_MOSSDEEP_CITY"),
        label: "gMossdeepCity",
        land: None,
        water: Some(GMOSSDEEPCITY_WATER),
        rock_smash: None,
        fishing: Some(GMOSSDEEPCITY_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_PACIFIDLOG_TOWN"),
        label: "gPacifidlogTown",
        land: None,
        water: Some(GPACIFIDLOGTOWN_WATER),
        rock_smash: None,
        fishing: Some(GPACIFIDLOGTOWN_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_EVER_GRANDE_CITY"),
        label: "gEverGrandeCity",
        land: None,
        water: Some(GEVERGRANDECITY_WATER),
        rock_smash: None,
        fishing: Some(GEVERGRANDECITY_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_PETALBURG_CITY"),
        label: "gPetalburgCity",
        land: None,
        water: Some(GPETALBURGCITY_WATER),
        rock_smash: None,
        fishing: Some(GPETALBURGCITY_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_UNDERWATER_ROUTE124"),
        label: "gUnderwater_Route124",
        land: None,
        water: Some(GUNDERWATER_ROUTE124_WATER),
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_SHOAL_CAVE_LOW_TIDE_ICE_ROOM"),
        label: "gShoalCave_LowTideIceRoom",
        land: Some(GSHOALCAVE_LOWTIDEICEROOM_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_SKY_PILLAR_1F"),
        label: "gSkyPillar_1F",
        land: Some(GSKYPILLAR_1F_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_SOOTOPOLIS_CITY"),
        label: "gSootopolisCity",
        land: None,
        water: Some(GSOOTOPOLISCITY_WATER),
        rock_smash: None,
        fishing: Some(GSOOTOPOLISCITY_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_SKY_PILLAR_3F"),
        label: "gSkyPillar_3F",
        land: Some(GSKYPILLAR_3F_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_SKY_PILLAR_5F"),
        label: "gSkyPillar_5F",
        land: Some(GSKYPILLAR_5F_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_SAFARI_ZONE_SOUTHEAST"),
        label: "gSafariZone_Southeast",
        land: Some(GSAFARIZONE_SOUTHEAST_LAND),
        water: Some(GSAFARIZONE_SOUTHEAST_WATER),
        rock_smash: None,
        fishing: Some(GSAFARIZONE_SOUTHEAST_FISHING),
    },
    WildEncounterHeader {
        map: MapId("MAP_SAFARI_ZONE_NORTHEAST"),
        label: "gSafariZone_Northeast",
        land: Some(GSAFARIZONE_NORTHEAST_LAND),
        water: None,
        rock_smash: Some(GSAFARIZONE_NORTHEAST_ROCKSMASH),
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_MAGMA_HIDEOUT_1F"),
        label: "gMagmaHideout_1F",
        land: Some(GMAGMAHIDEOUT_1F_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_MAGMA_HIDEOUT_2F_1R"),
        label: "gMagmaHideout_2F_1R",
        land: Some(GMAGMAHIDEOUT_2F_1R_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_MAGMA_HIDEOUT_2F_2R"),
        label: "gMagmaHideout_2F_2R",
        land: Some(GMAGMAHIDEOUT_2F_2R_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_MAGMA_HIDEOUT_3F_1R"),
        label: "gMagmaHideout_3F_1R",
        land: Some(GMAGMAHIDEOUT_3F_1R_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_MAGMA_HIDEOUT_3F_2R"),
        label: "gMagmaHideout_3F_2R",
        land: Some(GMAGMAHIDEOUT_3F_2R_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_MAGMA_HIDEOUT_4F"),
        label: "gMagmaHideout_4F",
        land: Some(GMAGMAHIDEOUT_4F_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_MAGMA_HIDEOUT_3F_3R"),
        label: "gMagmaHideout_3F_3R",
        land: Some(GMAGMAHIDEOUT_3F_3R_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_MAGMA_HIDEOUT_2F_3R"),
        label: "gMagmaHideout_2F_3R",
        land: Some(GMAGMAHIDEOUT_2F_3R_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_MIRAGE_TOWER_1F"),
        label: "gMirageTower_1F",
        land: Some(GMIRAGETOWER_1F_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_MIRAGE_TOWER_2F"),
        label: "gMirageTower_2F",
        land: Some(GMIRAGETOWER_2F_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_MIRAGE_TOWER_3F"),
        label: "gMirageTower_3F",
        land: Some(GMIRAGETOWER_3F_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_MIRAGE_TOWER_4F"),
        label: "gMirageTower_4F",
        land: Some(GMIRAGETOWER_4F_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_DESERT_UNDERPASS"),
        label: "gDesertUnderpass",
        land: Some(GDESERTUNDERPASS_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_ARTISAN_CAVE_B1F"),
        label: "gArtisanCave_B1F",
        land: Some(GARTISANCAVE_B1F_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_ARTISAN_CAVE_1F"),
        label: "gArtisanCave_1F",
        land: Some(GARTISANCAVE_1F_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_ALTERING_CAVE"),
        label: "gAlteringCave1",
        land: Some(GALTERINGCAVE1_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_ALTERING_CAVE"),
        label: "gAlteringCave2",
        land: Some(GALTERINGCAVE2_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_ALTERING_CAVE"),
        label: "gAlteringCave3",
        land: Some(GALTERINGCAVE3_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_ALTERING_CAVE"),
        label: "gAlteringCave4",
        land: Some(GALTERINGCAVE4_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_ALTERING_CAVE"),
        label: "gAlteringCave5",
        land: Some(GALTERINGCAVE5_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_ALTERING_CAVE"),
        label: "gAlteringCave6",
        land: Some(GALTERINGCAVE6_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_ALTERING_CAVE"),
        label: "gAlteringCave7",
        land: Some(GALTERINGCAVE7_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_ALTERING_CAVE"),
        label: "gAlteringCave8",
        land: Some(GALTERINGCAVE8_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_ALTERING_CAVE"),
        label: "gAlteringCave9",
        land: Some(GALTERINGCAVE9_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
    WildEncounterHeader {
        map: MapId("MAP_METEOR_FALLS_STEVENS_CAVE"),
        label: "gMeteorFalls_StevensCave",
        land: Some(GMETEORFALLS_STEVENSCAVE_LAND),
        water: None,
        rock_smash: None,
        fishing: None,
    },
];

/// The `gWildMonHeaders` table: owned, read-only access to every map's wild
/// encounter data with typed lookup `(oop-boundaries)`.
#[derive(Debug, Clone, Copy)]
pub struct WildEncounterTable {
    headers: &'static [WildEncounterHeader; MAP_HEADER_COUNT],
}

impl WildEncounterTable {
    /// The number of entries in the table ([`MAP_HEADER_COUNT`]).
    pub const LEN: usize = MAP_HEADER_COUNT;

    /// Build the table over the extracted upstream data.
    #[must_use]
    pub const fn new() -> Self {
        Self { headers: &HEADERS }
    }

    /// The entry with the given unique `label` (upstream `base_label`, e.g.
    /// `"gRoute101"`), or `None` if no entry has that label.
    #[must_use]
    pub fn get_by_label(&self, label: &str) -> Option<&'static WildEncounterHeader> {
        self.headers.iter().find(|h| h.label == label)
    }

    /// The entry with the given unique `label`.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownMap`] if no entry has that label.
    pub fn by_label(
        &self,
        label: &'static str,
    ) -> Result<&'static WildEncounterHeader, AssetError> {
        self.get_by_label(label)
            .ok_or(AssetError::UnknownMap(label))
    }

    /// The first entry for `map`, in table order, or `None` if no entry
    /// names that map.
    ///
    /// Most maps have exactly one entry, so this is the primary accessor;
    /// see [`WildEncounterTable::all_by_map`] for maps with more than one
    /// (currently only `MAP_ALTERING_CAVE`).
    #[must_use]
    pub fn get_by_map(&self, map: MapId) -> Option<&'static WildEncounterHeader> {
        self.headers.iter().find(|h| h.map == map)
    }

    /// The first entry for `map`, in table order.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownMap`] if no entry names that map.
    pub fn by_map(&self, map: MapId) -> Result<&'static WildEncounterHeader, AssetError> {
        self.get_by_map(map).ok_or(AssetError::UnknownMap(map.0))
    }

    /// Every entry naming `map`, in table order (more than one only for
    /// `MAP_ALTERING_CAVE`, whose nine variants the game swaps between at
    /// runtime).
    pub fn all_by_map(&self, map: MapId) -> impl Iterator<Item = &'static WildEncounterHeader> {
        self.headers.iter().filter(move |h| h.map == map)
    }

    /// Iterate over every entry, in upstream table order.
    pub fn iter(&self) -> impl Iterator<Item = &'static WildEncounterHeader> {
        self.headers.iter()
    }

    /// The number of entries in the table ([`MAP_HEADER_COUNT`]).
    #[must_use]
    pub const fn len(&self) -> usize {
        MAP_HEADER_COUNT
    }

    /// Always `false` — the table is never empty. Present for API convention.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }
}

impl Default for WildEncounterTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FishingRod, MapId, WildEncounterTable, FISHING_SLOTS, LAND_SLOTS, MAP_HEADER_COUNT,
        ROCK_SMASH_SLOTS, WATER_SLOTS,
    };
    use crate::error::AssetError;
    use crate::species::SpeciesId;

    // Raw upstream species ids used below (from `include/constants/species.h`).
    const WURMPLE: u16 = 290;
    const POOCHYENA: u16 = 286;
    const ZIGZAGOON: u16 = 288;
    const MARILL: u16 = 183;
    const GOLDEEN: u16 = 118;
    const MAGIKARP: u16 = 129;
    const CORPHISH: u16 = 326;
    const GEODUDE: u16 = 74;

    #[test]
    fn table_length_matches_upstream_encounter_count() {
        // Structural anchor: wild_encounter_groups[0].encounters has 124
        // entries in the upstream JSON.
        let table = WildEncounterTable::new();
        assert_eq!(MAP_HEADER_COUNT, 124);
        assert_eq!(table.len(), 124);
        assert_eq!(WildEncounterTable::LEN, 124);
        assert_eq!(table.iter().count(), 124);
        assert!(!table.is_empty());
    }

    #[test]
    fn every_present_kind_has_the_fixed_slot_count() {
        // Slot-count invariants derived from the JSON itself (see module
        // docs: constants/wild_encounter.h no longer exists upstream).
        let table = WildEncounterTable::new();
        for h in table.iter() {
            if let Some(land) = h.land {
                assert_eq!(land.mons.len(), LAND_SLOTS);
            }
            if let Some(water) = h.water {
                assert_eq!(water.mons.len(), WATER_SLOTS);
            }
            if let Some(rock_smash) = h.rock_smash {
                assert_eq!(rock_smash.mons.len(), ROCK_SMASH_SLOTS);
            }
            if let Some(fishing) = h.fishing {
                assert_eq!(fishing.mons.len(), FISHING_SLOTS);
            }
        }
    }

    #[test]
    fn upstream_tie_route_101_land_only() {
        // gRoute101: land mons only, straight from wild_encounters.json.
        let table = WildEncounterTable::new();
        let h = table.by_map(MapId("MAP_ROUTE101")).unwrap();
        assert_eq!(h.label, "gRoute101");
        assert!(h.water.is_none());
        assert!(h.rock_smash.is_none());
        assert!(h.fishing.is_none());
        let land = h.land.unwrap();
        assert_eq!(land.encounter_rate, 20);
        let expected: &[(u8, u8, u16)] = &[
            (2, 2, WURMPLE),
            (2, 2, POOCHYENA),
            (2, 2, WURMPLE),
            (3, 3, WURMPLE),
            (3, 3, POOCHYENA),
            (3, 3, POOCHYENA),
            (3, 3, WURMPLE),
            (3, 3, POOCHYENA),
            (2, 2, ZIGZAGOON),
            (2, 2, ZIGZAGOON),
            (3, 3, ZIGZAGOON),
            (3, 3, ZIGZAGOON),
        ];
        for (slot, &(min, max, sp)) in land.mons.iter().zip(expected) {
            assert_eq!(slot.min_level, min);
            assert_eq!(slot.max_level, max);
            assert_eq!(slot.species, SpeciesId(sp));
        }
    }

    #[test]
    fn upstream_tie_route_102_water_and_fishing() {
        // gRoute102: land + water + fishing, straight from wild_encounters.json.
        let table = WildEncounterTable::new();
        let h = table.by_map(MapId("MAP_ROUTE102")).unwrap();
        assert_eq!(h.label, "gRoute102");
        assert!(h.rock_smash.is_none());

        let water = h.water.unwrap();
        assert_eq!(water.encounter_rate, 4);
        let expected_water: &[(u8, u8, u16)] = &[
            (20, 30, MARILL),
            (10, 20, MARILL),
            (30, 35, MARILL),
            (5, 10, MARILL),
            (20, 30, GOLDEEN),
        ];
        for (slot, &(min, max, sp)) in water.mons.iter().zip(expected_water) {
            assert_eq!(slot.min_level, min);
            assert_eq!(slot.max_level, max);
            assert_eq!(slot.species, SpeciesId(sp));
        }

        let fishing = h.fishing.unwrap();
        assert_eq!(fishing.encounter_rate, 30);
        let expected_fishing: &[(u8, u8, u16)] = &[
            (5, 10, MAGIKARP),
            (5, 10, GOLDEEN),
            (10, 30, MAGIKARP),
            (10, 30, GOLDEEN),
            (10, 30, CORPHISH),
            (25, 30, CORPHISH),
            (30, 35, CORPHISH),
            (20, 25, CORPHISH),
            (35, 40, CORPHISH),
            (40, 45, CORPHISH),
        ];
        for (slot, &(min, max, sp)) in fishing.mons.iter().zip(expected_fishing) {
            assert_eq!(slot.min_level, min);
            assert_eq!(slot.max_level, max);
            assert_eq!(slot.species, SpeciesId(sp));
        }

        // Old rod only reaches the first two (weakest) slots.
        let old_rod: Vec<_> = fishing.mons_for_rod(FishingRod::Old).collect();
        assert_eq!(old_rod.len(), 2);
        assert_eq!(old_rod[0].species, SpeciesId(MAGIKARP));
        assert_eq!(old_rod[1].species, SpeciesId(GOLDEEN));
        // Super rod reaches the last five slots (the Corphish tier).
        let super_rod: Vec<_> = fishing.mons_for_rod(FishingRod::Super).collect();
        assert_eq!(super_rod.len(), 5);
        assert!(super_rod.iter().all(|m| m.species == SpeciesId(CORPHISH)));
    }

    #[test]
    fn upstream_tie_route_111_rock_smash() {
        // gRoute111: rock-smash slots, straight from wild_encounters.json.
        let table = WildEncounterTable::new();
        let h = table.by_map(MapId("MAP_ROUTE111")).unwrap();
        let rock_smash = h.rock_smash.unwrap();
        assert_eq!(rock_smash.encounter_rate, 20);
        let expected: &[(u8, u8, u16)] = &[
            (10, 15, GEODUDE),
            (5, 10, GEODUDE),
            (15, 20, GEODUDE),
            (15, 20, GEODUDE),
            (15, 20, GEODUDE),
        ];
        for (slot, &(min, max, sp)) in rock_smash.mons.iter().zip(expected) {
            assert_eq!(slot.min_level, min);
            assert_eq!(slot.max_level, max);
            assert_eq!(slot.species, SpeciesId(sp));
        }
    }

    #[test]
    fn altering_cave_has_nine_distinct_labelled_entries() {
        // The game's rotating Altering Cave: nine entries share MAP_ALTERING_CAVE
        // but have distinct base_labels and distinct first-slot species.
        let table = WildEncounterTable::new();
        let entries: Vec<_> = table.all_by_map(MapId("MAP_ALTERING_CAVE")).collect();
        assert_eq!(entries.len(), 9);
        let labels: std::collections::HashSet<_> = entries.iter().map(|h| h.label).collect();
        assert_eq!(labels.len(), 9, "altering cave labels must be distinct");
        // The first (gAlteringCave1) still resolves via the primary by-map
        // accessor (first match in table order).
        let first = table.by_map(MapId("MAP_ALTERING_CAVE")).unwrap();
        assert_eq!(first.label, "gAlteringCave1");
    }

    #[test]
    fn every_label_is_unique() {
        let table = WildEncounterTable::new();
        let labels: Vec<_> = table.iter().map(|h| h.label).collect();
        let unique: std::collections::HashSet<_> = labels.iter().collect();
        assert_eq!(labels.len(), unique.len(), "duplicate base_label in table");
    }

    #[test]
    fn unknown_map_is_an_error() {
        let table = WildEncounterTable::new();
        assert_eq!(table.get_by_map(MapId("MAP_NOT_REAL")), None);
        assert_eq!(
            table.by_map(MapId("MAP_NOT_REAL")),
            Err(AssetError::UnknownMap("MAP_NOT_REAL")),
        );
        assert_eq!(table.get_by_label("not_a_real_label"), None);
        assert_eq!(
            table.by_label("not_a_real_label"),
            Err(AssetError::UnknownMap("not_a_real_label")),
        );
    }
}
