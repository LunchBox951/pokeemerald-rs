//! Typed wild encounters indexed by their canonical map and base labels.

use crate::error::AssetError;
use crate::species::SpeciesId;

/// Number of slots in every land encounter table.
pub const LAND_SLOTS: usize = 12;
/// Number of slots in every water encounter table.
pub const WATER_SLOTS: usize = 5;
/// Number of slots in every rock-smash encounter table.
pub const ROCK_SMASH_SLOTS: usize = 5;
/// Number of slots in every fishing encounter table.
pub const FISHING_SLOTS: usize = 10;

/// Number of encounter headers, including all nine Altering Cave variants.
pub const MAP_HEADER_COUNT: usize = 124;

/// A symbolic `MAP_*` identity shared with map-header data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MapId(pub &'static str);

impl MapId {
    /// Returns the symbolic `MAP_*` name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        self.0
    }
}

/// A species and inclusive level range for one encounter slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WildPokemon {
    /// Lowest generated level.
    pub min_level: u8,
    /// Highest generated level.
    pub max_level: u8,
    /// Generated species.
    pub species: SpeciesId,
}

/// Land encounters for one map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LandEncounters {
    /// Stored land encounter rate.
    pub encounter_rate: u8,
    /// Slots in selection order.
    pub mons: [WildPokemon; LAND_SLOTS],
}

/// Water encounters for one map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaterEncounters {
    /// Stored water encounter rate.
    pub encounter_rate: u8,
    /// Slots in selection order.
    pub mons: [WildPokemon; WATER_SLOTS],
}

/// Rock-smash encounters for one map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RockSmashEncounters {
    /// Stored rock-smash encounter rate.
    pub encounter_rate: u8,
    /// Slots in selection order.
    pub mons: [WildPokemon; ROCK_SMASH_SLOTS],
}

/// Fishing encounters for one map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FishingEncounters {
    /// Stored fishing encounter rate.
    pub encounter_rate: u8,
    /// Slots partitioned by [`FishingRod::slots`].
    pub mons: [WildPokemon; FISHING_SLOTS],
}

impl FishingEncounters {
    /// Returns the slots available to `rod`, in selection order.
    pub fn mons_for_rod(&self, rod: FishingRod) -> impl Iterator<Item = WildPokemon> + '_ {
        rod.slots().iter().map(|&i| self.mons[i])
    }
}

/// Fishing-rod tiers that partition the ten fishing slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FishingRod {
    /// Old Rod tier.
    Old,
    /// Good Rod tier.
    Good,
    /// Super Rod tier.
    Super,
}

impl FishingRod {
    /// Returns this rod's partition of [`FishingEncounters::mons`].
    #[must_use]
    pub const fn slots(self) -> &'static [usize] {
        match self {
            FishingRod::Old => &[0, 1],
            FishingRod::Good => &[2, 3, 4],
            FishingRod::Super => &[5, 6, 7, 8, 9],
        }
    }
}

/// Wild encounters for one canonical base label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WildEncounterHeader {
    /// Map identity. Altering Cave deliberately has nine headers with this ID.
    pub map: MapId,
    /// Unique base label, such as `"gRoute101"`.
    pub label: &'static str,
    /// Land encounters, when present.
    pub land: Option<LandEncounters>,
    /// Water encounters, when present.
    pub water: Option<WaterEncounters>,
    /// Rock-smash encounters, when present.
    pub rock_smash: Option<RockSmashEncounters>,
    /// Fishing encounters, when present.
    pub fishing: Option<FishingEncounters>,
}

macro_rules! wild {
    (levels: $min_level:literal..=$max_level:literal, species: $species:expr) => {
        WildPokemon {
            min_level: $min_level,
            max_level: $max_level,
            species: $species,
        }
    };
}

const GROUTE101_LAND: LandEncounters = LandEncounters {
    encounter_rate: 20,
    mons: [
        wild!(levels: 2..=2, species: SpeciesId::WURMPLE),
        wild!(levels: 2..=2, species: SpeciesId::POOCHYENA),
        wild!(levels: 2..=2, species: SpeciesId::WURMPLE),
        wild!(levels: 3..=3, species: SpeciesId::WURMPLE),
        wild!(levels: 3..=3, species: SpeciesId::POOCHYENA),
        wild!(levels: 3..=3, species: SpeciesId::POOCHYENA),
        wild!(levels: 3..=3, species: SpeciesId::WURMPLE),
        wild!(levels: 3..=3, species: SpeciesId::POOCHYENA),
        wild!(levels: 2..=2, species: SpeciesId::ZIGZAGOON),
        wild!(levels: 2..=2, species: SpeciesId::ZIGZAGOON),
        wild!(levels: 3..=3, species: SpeciesId::ZIGZAGOON),
        wild!(levels: 3..=3, species: SpeciesId::ZIGZAGOON),
    ],
};

const GROUTE102_LAND: LandEncounters = LandEncounters {
    encounter_rate: 20,
    mons: [
        wild!(levels: 3..=3, species: SpeciesId::POOCHYENA),
        wild!(levels: 3..=3, species: SpeciesId::WURMPLE),
        wild!(levels: 4..=4, species: SpeciesId::POOCHYENA),
        wild!(levels: 4..=4, species: SpeciesId::WURMPLE),
        wild!(levels: 3..=3, species: SpeciesId::LOTAD),
        wild!(levels: 4..=4, species: SpeciesId::LOTAD),
        wild!(levels: 3..=3, species: SpeciesId::ZIGZAGOON),
        wild!(levels: 3..=3, species: SpeciesId::ZIGZAGOON),
        wild!(levels: 4..=4, species: SpeciesId::ZIGZAGOON),
        wild!(levels: 4..=4, species: SpeciesId::RALTS),
        wild!(levels: 4..=4, species: SpeciesId::ZIGZAGOON),
        wild!(levels: 3..=3, species: SpeciesId::SEEDOT),
    ],
};

const GROUTE102_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 20..=30, species: SpeciesId::MARILL),
        wild!(levels: 10..=20, species: SpeciesId::MARILL),
        wild!(levels: 30..=35, species: SpeciesId::MARILL),
        wild!(levels: 5..=10, species: SpeciesId::MARILL),
        wild!(levels: 20..=30, species: SpeciesId::GOLDEEN),
    ],
};

const GROUTE102_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::GOLDEEN),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::GOLDEEN),
        wild!(levels: 10..=30, species: SpeciesId::CORPHISH),
        wild!(levels: 25..=30, species: SpeciesId::CORPHISH),
        wild!(levels: 30..=35, species: SpeciesId::CORPHISH),
        wild!(levels: 20..=25, species: SpeciesId::CORPHISH),
        wild!(levels: 35..=40, species: SpeciesId::CORPHISH),
        wild!(levels: 40..=45, species: SpeciesId::CORPHISH),
    ],
};

const GROUTE103_LAND: LandEncounters = LandEncounters {
    encounter_rate: 20,
    mons: [
        wild!(levels: 2..=2, species: SpeciesId::POOCHYENA),
        wild!(levels: 3..=3, species: SpeciesId::POOCHYENA),
        wild!(levels: 3..=3, species: SpeciesId::POOCHYENA),
        wild!(levels: 4..=4, species: SpeciesId::POOCHYENA),
        wild!(levels: 2..=2, species: SpeciesId::WINGULL),
        wild!(levels: 3..=3, species: SpeciesId::ZIGZAGOON),
        wild!(levels: 3..=3, species: SpeciesId::ZIGZAGOON),
        wild!(levels: 4..=4, species: SpeciesId::ZIGZAGOON),
        wild!(levels: 3..=3, species: SpeciesId::WINGULL),
        wild!(levels: 3..=3, species: SpeciesId::WINGULL),
        wild!(levels: 2..=2, species: SpeciesId::WINGULL),
        wild!(levels: 4..=4, species: SpeciesId::WINGULL),
    ],
};

const GROUTE103_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WINGULL),
        wild!(levels: 15..=25, species: SpeciesId::WINGULL),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
    ],
};

const GROUTE103_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WAILMER),
        wild!(levels: 30..=35, species: SpeciesId::SHARPEDO),
        wild!(levels: 30..=35, species: SpeciesId::WAILMER),
        wild!(levels: 25..=30, species: SpeciesId::WAILMER),
        wild!(levels: 35..=40, species: SpeciesId::WAILMER),
        wild!(levels: 40..=45, species: SpeciesId::WAILMER),
    ],
};

const GROUTE104_LAND: LandEncounters = LandEncounters {
    encounter_rate: 20,
    mons: [
        wild!(levels: 4..=4, species: SpeciesId::POOCHYENA),
        wild!(levels: 4..=4, species: SpeciesId::WURMPLE),
        wild!(levels: 5..=5, species: SpeciesId::POOCHYENA),
        wild!(levels: 5..=5, species: SpeciesId::MARILL),
        wild!(levels: 4..=4, species: SpeciesId::MARILL),
        wild!(levels: 5..=5, species: SpeciesId::POOCHYENA),
        wild!(levels: 4..=4, species: SpeciesId::TAILLOW),
        wild!(levels: 5..=5, species: SpeciesId::TAILLOW),
        wild!(levels: 4..=4, species: SpeciesId::WINGULL),
        wild!(levels: 4..=4, species: SpeciesId::WINGULL),
        wild!(levels: 3..=3, species: SpeciesId::WINGULL),
        wild!(levels: 5..=5, species: SpeciesId::WINGULL),
    ],
};

const GROUTE104_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 10..=30, species: SpeciesId::WINGULL),
        wild!(levels: 15..=25, species: SpeciesId::WINGULL),
        wild!(levels: 15..=25, species: SpeciesId::WINGULL),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
    ],
};

const GROUTE104_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 25..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 30..=35, species: SpeciesId::MAGIKARP),
        wild!(levels: 20..=25, species: SpeciesId::MAGIKARP),
        wild!(levels: 35..=40, species: SpeciesId::MAGIKARP),
        wild!(levels: 40..=45, species: SpeciesId::MAGIKARP),
    ],
};

const GROUTE105_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WINGULL),
        wild!(levels: 15..=25, species: SpeciesId::WINGULL),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
    ],
};

const GROUTE105_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WAILMER),
        wild!(levels: 25..=30, species: SpeciesId::WAILMER),
        wild!(levels: 30..=35, species: SpeciesId::WAILMER),
        wild!(levels: 20..=25, species: SpeciesId::WAILMER),
        wild!(levels: 35..=40, species: SpeciesId::WAILMER),
        wild!(levels: 40..=45, species: SpeciesId::WAILMER),
    ],
};

const GROUTE110_LAND: LandEncounters = LandEncounters {
    encounter_rate: 20,
    mons: [
        wild!(levels: 12..=12, species: SpeciesId::POOCHYENA),
        wild!(levels: 12..=12, species: SpeciesId::ELECTRIKE),
        wild!(levels: 12..=12, species: SpeciesId::GULPIN),
        wild!(levels: 13..=13, species: SpeciesId::ELECTRIKE),
        wild!(levels: 13..=13, species: SpeciesId::MINUN),
        wild!(levels: 13..=13, species: SpeciesId::ODDISH),
        wild!(levels: 13..=13, species: SpeciesId::MINUN),
        wild!(levels: 13..=13, species: SpeciesId::GULPIN),
        wild!(levels: 12..=12, species: SpeciesId::WINGULL),
        wild!(levels: 12..=12, species: SpeciesId::WINGULL),
        wild!(levels: 12..=12, species: SpeciesId::PLUSLE),
        wild!(levels: 13..=13, species: SpeciesId::PLUSLE),
    ],
};

const GROUTE110_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WINGULL),
        wild!(levels: 15..=25, species: SpeciesId::WINGULL),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
    ],
};

const GROUTE110_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WAILMER),
        wild!(levels: 25..=30, species: SpeciesId::WAILMER),
        wild!(levels: 30..=35, species: SpeciesId::WAILMER),
        wild!(levels: 20..=25, species: SpeciesId::WAILMER),
        wild!(levels: 35..=40, species: SpeciesId::WAILMER),
        wild!(levels: 40..=45, species: SpeciesId::WAILMER),
    ],
};

const GROUTE111_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 20..=20, species: SpeciesId::SANDSHREW),
        wild!(levels: 20..=20, species: SpeciesId::TRAPINCH),
        wild!(levels: 21..=21, species: SpeciesId::SANDSHREW),
        wild!(levels: 21..=21, species: SpeciesId::TRAPINCH),
        wild!(levels: 19..=19, species: SpeciesId::BALTOY),
        wild!(levels: 21..=21, species: SpeciesId::BALTOY),
        wild!(levels: 19..=19, species: SpeciesId::SANDSHREW),
        wild!(levels: 19..=19, species: SpeciesId::TRAPINCH),
        wild!(levels: 20..=20, species: SpeciesId::BALTOY),
        wild!(levels: 20..=20, species: SpeciesId::CACNEA),
        wild!(levels: 22..=22, species: SpeciesId::CACNEA),
        wild!(levels: 22..=22, species: SpeciesId::CACNEA),
    ],
};

const GROUTE111_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 20..=30, species: SpeciesId::MARILL),
        wild!(levels: 10..=20, species: SpeciesId::MARILL),
        wild!(levels: 30..=35, species: SpeciesId::MARILL),
        wild!(levels: 5..=10, species: SpeciesId::MARILL),
        wild!(levels: 20..=30, species: SpeciesId::GOLDEEN),
    ],
};

const GROUTE111_ROCKSMASH: RockSmashEncounters = RockSmashEncounters {
    encounter_rate: 20,
    mons: [
        wild!(levels: 10..=15, species: SpeciesId::GEODUDE),
        wild!(levels: 5..=10, species: SpeciesId::GEODUDE),
        wild!(levels: 15..=20, species: SpeciesId::GEODUDE),
        wild!(levels: 15..=20, species: SpeciesId::GEODUDE),
        wild!(levels: 15..=20, species: SpeciesId::GEODUDE),
    ],
};

const GROUTE111_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::GOLDEEN),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::GOLDEEN),
        wild!(levels: 10..=30, species: SpeciesId::BARBOACH),
        wild!(levels: 25..=30, species: SpeciesId::BARBOACH),
        wild!(levels: 30..=35, species: SpeciesId::BARBOACH),
        wild!(levels: 20..=25, species: SpeciesId::BARBOACH),
        wild!(levels: 35..=40, species: SpeciesId::BARBOACH),
        wild!(levels: 40..=45, species: SpeciesId::BARBOACH),
    ],
};

const GROUTE112_LAND: LandEncounters = LandEncounters {
    encounter_rate: 20,
    mons: [
        wild!(levels: 15..=15, species: SpeciesId::NUMEL),
        wild!(levels: 15..=15, species: SpeciesId::NUMEL),
        wild!(levels: 15..=15, species: SpeciesId::MARILL),
        wild!(levels: 14..=14, species: SpeciesId::NUMEL),
        wild!(levels: 14..=14, species: SpeciesId::NUMEL),
        wild!(levels: 14..=14, species: SpeciesId::MARILL),
        wild!(levels: 16..=16, species: SpeciesId::NUMEL),
        wild!(levels: 16..=16, species: SpeciesId::MARILL),
        wild!(levels: 16..=16, species: SpeciesId::NUMEL),
        wild!(levels: 16..=16, species: SpeciesId::NUMEL),
        wild!(levels: 16..=16, species: SpeciesId::NUMEL),
        wild!(levels: 16..=16, species: SpeciesId::NUMEL),
    ],
};

const GROUTE113_LAND: LandEncounters = LandEncounters {
    encounter_rate: 20,
    mons: [
        wild!(levels: 15..=15, species: SpeciesId::SPINDA),
        wild!(levels: 15..=15, species: SpeciesId::SPINDA),
        wild!(levels: 15..=15, species: SpeciesId::SLUGMA),
        wild!(levels: 14..=14, species: SpeciesId::SPINDA),
        wild!(levels: 14..=14, species: SpeciesId::SPINDA),
        wild!(levels: 14..=14, species: SpeciesId::SLUGMA),
        wild!(levels: 16..=16, species: SpeciesId::SPINDA),
        wild!(levels: 16..=16, species: SpeciesId::SLUGMA),
        wild!(levels: 16..=16, species: SpeciesId::SPINDA),
        wild!(levels: 16..=16, species: SpeciesId::SKARMORY),
        wild!(levels: 16..=16, species: SpeciesId::SPINDA),
        wild!(levels: 16..=16, species: SpeciesId::SKARMORY),
    ],
};

const GROUTE114_LAND: LandEncounters = LandEncounters {
    encounter_rate: 20,
    mons: [
        wild!(levels: 16..=16, species: SpeciesId::SWABLU),
        wild!(levels: 16..=16, species: SpeciesId::LOTAD),
        wild!(levels: 17..=17, species: SpeciesId::SWABLU),
        wild!(levels: 15..=15, species: SpeciesId::SWABLU),
        wild!(levels: 15..=15, species: SpeciesId::LOTAD),
        wild!(levels: 16..=16, species: SpeciesId::LOMBRE),
        wild!(levels: 16..=16, species: SpeciesId::LOMBRE),
        wild!(levels: 18..=18, species: SpeciesId::LOMBRE),
        wild!(levels: 17..=17, species: SpeciesId::SEVIPER),
        wild!(levels: 15..=15, species: SpeciesId::SEVIPER),
        wild!(levels: 17..=17, species: SpeciesId::SEVIPER),
        wild!(levels: 15..=15, species: SpeciesId::NUZLEAF),
    ],
};

const GROUTE114_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 20..=30, species: SpeciesId::MARILL),
        wild!(levels: 10..=20, species: SpeciesId::MARILL),
        wild!(levels: 30..=35, species: SpeciesId::MARILL),
        wild!(levels: 5..=10, species: SpeciesId::MARILL),
        wild!(levels: 20..=30, species: SpeciesId::GOLDEEN),
    ],
};

const GROUTE114_ROCKSMASH: RockSmashEncounters = RockSmashEncounters {
    encounter_rate: 20,
    mons: [
        wild!(levels: 10..=15, species: SpeciesId::GEODUDE),
        wild!(levels: 5..=10, species: SpeciesId::GEODUDE),
        wild!(levels: 15..=20, species: SpeciesId::GEODUDE),
        wild!(levels: 15..=20, species: SpeciesId::GEODUDE),
        wild!(levels: 15..=20, species: SpeciesId::GEODUDE),
    ],
};

const GROUTE114_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::GOLDEEN),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::GOLDEEN),
        wild!(levels: 10..=30, species: SpeciesId::BARBOACH),
        wild!(levels: 25..=30, species: SpeciesId::BARBOACH),
        wild!(levels: 30..=35, species: SpeciesId::BARBOACH),
        wild!(levels: 20..=25, species: SpeciesId::BARBOACH),
        wild!(levels: 35..=40, species: SpeciesId::BARBOACH),
        wild!(levels: 40..=45, species: SpeciesId::BARBOACH),
    ],
};

const GROUTE116_LAND: LandEncounters = LandEncounters {
    encounter_rate: 20,
    mons: [
        wild!(levels: 6..=6, species: SpeciesId::POOCHYENA),
        wild!(levels: 6..=6, species: SpeciesId::WHISMUR),
        wild!(levels: 6..=6, species: SpeciesId::NINCADA),
        wild!(levels: 7..=7, species: SpeciesId::ABRA),
        wild!(levels: 7..=7, species: SpeciesId::NINCADA),
        wild!(levels: 6..=6, species: SpeciesId::TAILLOW),
        wild!(levels: 7..=7, species: SpeciesId::TAILLOW),
        wild!(levels: 8..=8, species: SpeciesId::TAILLOW),
        wild!(levels: 7..=7, species: SpeciesId::POOCHYENA),
        wild!(levels: 8..=8, species: SpeciesId::POOCHYENA),
        wild!(levels: 7..=7, species: SpeciesId::SKITTY),
        wild!(levels: 8..=8, species: SpeciesId::SKITTY),
    ],
};

const GROUTE117_LAND: LandEncounters = LandEncounters {
    encounter_rate: 20,
    mons: [
        wild!(levels: 13..=13, species: SpeciesId::POOCHYENA),
        wild!(levels: 13..=13, species: SpeciesId::ODDISH),
        wild!(levels: 14..=14, species: SpeciesId::POOCHYENA),
        wild!(levels: 14..=14, species: SpeciesId::ODDISH),
        wild!(levels: 13..=13, species: SpeciesId::MARILL),
        wild!(levels: 13..=13, species: SpeciesId::ODDISH),
        wild!(levels: 13..=13, species: SpeciesId::ILLUMISE),
        wild!(levels: 13..=13, species: SpeciesId::ILLUMISE),
        wild!(levels: 14..=14, species: SpeciesId::ILLUMISE),
        wild!(levels: 14..=14, species: SpeciesId::ILLUMISE),
        wild!(levels: 13..=13, species: SpeciesId::VOLBEAT),
        wild!(levels: 13..=13, species: SpeciesId::SEEDOT),
    ],
};

const GROUTE117_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 20..=30, species: SpeciesId::MARILL),
        wild!(levels: 10..=20, species: SpeciesId::MARILL),
        wild!(levels: 30..=35, species: SpeciesId::MARILL),
        wild!(levels: 5..=10, species: SpeciesId::MARILL),
        wild!(levels: 20..=30, species: SpeciesId::GOLDEEN),
    ],
};

const GROUTE117_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::GOLDEEN),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::GOLDEEN),
        wild!(levels: 10..=30, species: SpeciesId::CORPHISH),
        wild!(levels: 25..=30, species: SpeciesId::CORPHISH),
        wild!(levels: 30..=35, species: SpeciesId::CORPHISH),
        wild!(levels: 20..=25, species: SpeciesId::CORPHISH),
        wild!(levels: 35..=40, species: SpeciesId::CORPHISH),
        wild!(levels: 40..=45, species: SpeciesId::CORPHISH),
    ],
};

const GROUTE118_LAND: LandEncounters = LandEncounters {
    encounter_rate: 20,
    mons: [
        wild!(levels: 24..=24, species: SpeciesId::ZIGZAGOON),
        wild!(levels: 24..=24, species: SpeciesId::ELECTRIKE),
        wild!(levels: 26..=26, species: SpeciesId::ZIGZAGOON),
        wild!(levels: 26..=26, species: SpeciesId::ELECTRIKE),
        wild!(levels: 26..=26, species: SpeciesId::LINOONE),
        wild!(levels: 26..=26, species: SpeciesId::MANECTRIC),
        wild!(levels: 25..=25, species: SpeciesId::WINGULL),
        wild!(levels: 25..=25, species: SpeciesId::WINGULL),
        wild!(levels: 26..=26, species: SpeciesId::WINGULL),
        wild!(levels: 26..=26, species: SpeciesId::WINGULL),
        wild!(levels: 27..=27, species: SpeciesId::WINGULL),
        wild!(levels: 25..=25, species: SpeciesId::KECLEON),
    ],
};

const GROUTE118_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WINGULL),
        wild!(levels: 15..=25, species: SpeciesId::WINGULL),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
    ],
};

const GROUTE118_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::CARVANHA),
        wild!(levels: 30..=35, species: SpeciesId::SHARPEDO),
        wild!(levels: 30..=35, species: SpeciesId::CARVANHA),
        wild!(levels: 20..=25, species: SpeciesId::CARVANHA),
        wild!(levels: 35..=40, species: SpeciesId::CARVANHA),
        wild!(levels: 40..=45, species: SpeciesId::CARVANHA),
    ],
};

const GROUTE124_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WINGULL),
        wild!(levels: 15..=25, species: SpeciesId::WINGULL),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
    ],
};

const GROUTE124_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WAILMER),
        wild!(levels: 30..=35, species: SpeciesId::SHARPEDO),
        wild!(levels: 30..=35, species: SpeciesId::WAILMER),
        wild!(levels: 25..=30, species: SpeciesId::WAILMER),
        wild!(levels: 35..=40, species: SpeciesId::WAILMER),
        wild!(levels: 40..=45, species: SpeciesId::WAILMER),
    ],
};

const GPETALBURGWOODS_LAND: LandEncounters = LandEncounters {
    encounter_rate: 20,
    mons: [
        wild!(levels: 5..=5, species: SpeciesId::POOCHYENA),
        wild!(levels: 5..=5, species: SpeciesId::WURMPLE),
        wild!(levels: 5..=5, species: SpeciesId::SHROOMISH),
        wild!(levels: 6..=6, species: SpeciesId::POOCHYENA),
        wild!(levels: 5..=5, species: SpeciesId::SILCOON),
        wild!(levels: 5..=5, species: SpeciesId::CASCOON),
        wild!(levels: 6..=6, species: SpeciesId::WURMPLE),
        wild!(levels: 6..=6, species: SpeciesId::SHROOMISH),
        wild!(levels: 5..=5, species: SpeciesId::TAILLOW),
        wild!(levels: 5..=5, species: SpeciesId::SLAKOTH),
        wild!(levels: 6..=6, species: SpeciesId::TAILLOW),
        wild!(levels: 6..=6, species: SpeciesId::SLAKOTH),
    ],
};

const GRUSTURFTUNNEL_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 6..=6, species: SpeciesId::WHISMUR),
        wild!(levels: 7..=7, species: SpeciesId::WHISMUR),
        wild!(levels: 6..=6, species: SpeciesId::WHISMUR),
        wild!(levels: 6..=6, species: SpeciesId::WHISMUR),
        wild!(levels: 7..=7, species: SpeciesId::WHISMUR),
        wild!(levels: 7..=7, species: SpeciesId::WHISMUR),
        wild!(levels: 5..=5, species: SpeciesId::WHISMUR),
        wild!(levels: 8..=8, species: SpeciesId::WHISMUR),
        wild!(levels: 5..=5, species: SpeciesId::WHISMUR),
        wild!(levels: 8..=8, species: SpeciesId::WHISMUR),
        wild!(levels: 5..=5, species: SpeciesId::WHISMUR),
        wild!(levels: 8..=8, species: SpeciesId::WHISMUR),
    ],
};

const GGRANITECAVE_1F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 7..=7, species: SpeciesId::ZUBAT),
        wild!(levels: 8..=8, species: SpeciesId::MAKUHITA),
        wild!(levels: 7..=7, species: SpeciesId::MAKUHITA),
        wild!(levels: 8..=8, species: SpeciesId::ZUBAT),
        wild!(levels: 9..=9, species: SpeciesId::MAKUHITA),
        wild!(levels: 8..=8, species: SpeciesId::ABRA),
        wild!(levels: 10..=10, species: SpeciesId::MAKUHITA),
        wild!(levels: 6..=6, species: SpeciesId::MAKUHITA),
        wild!(levels: 7..=7, species: SpeciesId::GEODUDE),
        wild!(levels: 8..=8, species: SpeciesId::GEODUDE),
        wild!(levels: 6..=6, species: SpeciesId::GEODUDE),
        wild!(levels: 9..=9, species: SpeciesId::GEODUDE),
    ],
};

const GGRANITECAVE_B1F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 9..=9, species: SpeciesId::ZUBAT),
        wild!(levels: 10..=10, species: SpeciesId::ARON),
        wild!(levels: 9..=9, species: SpeciesId::ARON),
        wild!(levels: 11..=11, species: SpeciesId::ARON),
        wild!(levels: 10..=10, species: SpeciesId::ZUBAT),
        wild!(levels: 9..=9, species: SpeciesId::ABRA),
        wild!(levels: 10..=10, species: SpeciesId::MAKUHITA),
        wild!(levels: 11..=11, species: SpeciesId::MAKUHITA),
        wild!(levels: 10..=10, species: SpeciesId::SABLEYE),
        wild!(levels: 10..=10, species: SpeciesId::SABLEYE),
        wild!(levels: 9..=9, species: SpeciesId::SABLEYE),
        wild!(levels: 11..=11, species: SpeciesId::SABLEYE),
    ],
};

const GMTPYRE_1F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 27..=27, species: SpeciesId::SHUPPET),
        wild!(levels: 28..=28, species: SpeciesId::SHUPPET),
        wild!(levels: 26..=26, species: SpeciesId::SHUPPET),
        wild!(levels: 25..=25, species: SpeciesId::SHUPPET),
        wild!(levels: 29..=29, species: SpeciesId::SHUPPET),
        wild!(levels: 24..=24, species: SpeciesId::SHUPPET),
        wild!(levels: 23..=23, species: SpeciesId::SHUPPET),
        wild!(levels: 22..=22, species: SpeciesId::SHUPPET),
        wild!(levels: 29..=29, species: SpeciesId::SHUPPET),
        wild!(levels: 24..=24, species: SpeciesId::SHUPPET),
        wild!(levels: 29..=29, species: SpeciesId::SHUPPET),
        wild!(levels: 24..=24, species: SpeciesId::SHUPPET),
    ],
};

const GVICTORYROAD_1F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 40..=40, species: SpeciesId::GOLBAT),
        wild!(levels: 40..=40, species: SpeciesId::HARIYAMA),
        wild!(levels: 40..=40, species: SpeciesId::LAIRON),
        wild!(levels: 40..=40, species: SpeciesId::LOUDRED),
        wild!(levels: 36..=36, species: SpeciesId::ZUBAT),
        wild!(levels: 36..=36, species: SpeciesId::MAKUHITA),
        wild!(levels: 38..=38, species: SpeciesId::GOLBAT),
        wild!(levels: 38..=38, species: SpeciesId::HARIYAMA),
        wild!(levels: 36..=36, species: SpeciesId::ARON),
        wild!(levels: 36..=36, species: SpeciesId::WHISMUR),
        wild!(levels: 36..=36, species: SpeciesId::ARON),
        wild!(levels: 36..=36, species: SpeciesId::WHISMUR),
    ],
};

const GSAFARIZONE_SOUTH_LAND: LandEncounters = LandEncounters {
    encounter_rate: 25,
    mons: [
        wild!(levels: 25..=25, species: SpeciesId::ODDISH),
        wild!(levels: 27..=27, species: SpeciesId::ODDISH),
        wild!(levels: 25..=25, species: SpeciesId::GIRAFARIG),
        wild!(levels: 27..=27, species: SpeciesId::GIRAFARIG),
        wild!(levels: 25..=25, species: SpeciesId::NATU),
        wild!(levels: 25..=25, species: SpeciesId::DODUO),
        wild!(levels: 25..=25, species: SpeciesId::GLOOM),
        wild!(levels: 27..=27, species: SpeciesId::WOBBUFFET),
        wild!(levels: 25..=25, species: SpeciesId::PIKACHU),
        wild!(levels: 27..=27, species: SpeciesId::WOBBUFFET),
        wild!(levels: 27..=27, species: SpeciesId::PIKACHU),
        wild!(levels: 29..=29, species: SpeciesId::WOBBUFFET),
    ],
};

const GUNDERWATER_ROUTE126_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 20..=30, species: SpeciesId::CLAMPERL),
        wild!(levels: 20..=30, species: SpeciesId::CHINCHOU),
        wild!(levels: 30..=35, species: SpeciesId::CLAMPERL),
        wild!(levels: 30..=35, species: SpeciesId::RELICANTH),
        wild!(levels: 30..=35, species: SpeciesId::RELICANTH),
    ],
};

const GABANDONEDSHIP_ROOMS_B1F_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 30..=35, species: SpeciesId::TENTACRUEL),
    ],
};

const GABANDONEDSHIP_ROOMS_B1F_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 20,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 25..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 30..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 30..=35, species: SpeciesId::TENTACRUEL),
        wild!(levels: 25..=30, species: SpeciesId::TENTACRUEL),
        wild!(levels: 20..=25, species: SpeciesId::TENTACRUEL),
    ],
};

const GGRANITECAVE_B2F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 10..=10, species: SpeciesId::ZUBAT),
        wild!(levels: 11..=11, species: SpeciesId::ARON),
        wild!(levels: 10..=10, species: SpeciesId::ARON),
        wild!(levels: 11..=11, species: SpeciesId::ZUBAT),
        wild!(levels: 12..=12, species: SpeciesId::ARON),
        wild!(levels: 10..=10, species: SpeciesId::ABRA),
        wild!(levels: 10..=10, species: SpeciesId::SABLEYE),
        wild!(levels: 11..=11, species: SpeciesId::SABLEYE),
        wild!(levels: 12..=12, species: SpeciesId::SABLEYE),
        wild!(levels: 10..=10, species: SpeciesId::SABLEYE),
        wild!(levels: 12..=12, species: SpeciesId::SABLEYE),
        wild!(levels: 10..=10, species: SpeciesId::SABLEYE),
    ],
};

const GGRANITECAVE_B2F_ROCKSMASH: RockSmashEncounters = RockSmashEncounters {
    encounter_rate: 20,
    mons: [
        wild!(levels: 10..=15, species: SpeciesId::GEODUDE),
        wild!(levels: 10..=20, species: SpeciesId::NOSEPASS),
        wild!(levels: 5..=10, species: SpeciesId::GEODUDE),
        wild!(levels: 15..=20, species: SpeciesId::GEODUDE),
        wild!(levels: 15..=20, species: SpeciesId::GEODUDE),
    ],
};

const GFIERYPATH_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 15..=15, species: SpeciesId::NUMEL),
        wild!(levels: 15..=15, species: SpeciesId::KOFFING),
        wild!(levels: 16..=16, species: SpeciesId::NUMEL),
        wild!(levels: 15..=15, species: SpeciesId::MACHOP),
        wild!(levels: 15..=15, species: SpeciesId::TORKOAL),
        wild!(levels: 15..=15, species: SpeciesId::SLUGMA),
        wild!(levels: 16..=16, species: SpeciesId::KOFFING),
        wild!(levels: 16..=16, species: SpeciesId::MACHOP),
        wild!(levels: 14..=14, species: SpeciesId::TORKOAL),
        wild!(levels: 16..=16, species: SpeciesId::TORKOAL),
        wild!(levels: 14..=14, species: SpeciesId::GRIMER),
        wild!(levels: 14..=14, species: SpeciesId::GRIMER),
    ],
};

const GMETEORFALLS_B1F_2R_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 33..=33, species: SpeciesId::GOLBAT),
        wild!(levels: 35..=35, species: SpeciesId::GOLBAT),
        wild!(levels: 30..=30, species: SpeciesId::BAGON),
        wild!(levels: 35..=35, species: SpeciesId::SOLROCK),
        wild!(levels: 35..=35, species: SpeciesId::BAGON),
        wild!(levels: 37..=37, species: SpeciesId::SOLROCK),
        wild!(levels: 25..=25, species: SpeciesId::BAGON),
        wild!(levels: 39..=39, species: SpeciesId::SOLROCK),
        wild!(levels: 38..=38, species: SpeciesId::GOLBAT),
        wild!(levels: 40..=40, species: SpeciesId::GOLBAT),
        wild!(levels: 38..=38, species: SpeciesId::GOLBAT),
        wild!(levels: 40..=40, species: SpeciesId::GOLBAT),
    ],
};

const GMETEORFALLS_B1F_2R_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 30..=35, species: SpeciesId::GOLBAT),
        wild!(levels: 30..=35, species: SpeciesId::GOLBAT),
        wild!(levels: 25..=35, species: SpeciesId::SOLROCK),
        wild!(levels: 15..=25, species: SpeciesId::SOLROCK),
        wild!(levels: 5..=15, species: SpeciesId::SOLROCK),
    ],
};

const GMETEORFALLS_B1F_2R_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::GOLDEEN),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::GOLDEEN),
        wild!(levels: 10..=30, species: SpeciesId::BARBOACH),
        wild!(levels: 25..=30, species: SpeciesId::BARBOACH),
        wild!(levels: 30..=35, species: SpeciesId::BARBOACH),
        wild!(levels: 30..=35, species: SpeciesId::WHISCASH),
        wild!(levels: 35..=40, species: SpeciesId::WHISCASH),
        wild!(levels: 40..=45, species: SpeciesId::WHISCASH),
    ],
};

const GJAGGEDPASS_LAND: LandEncounters = LandEncounters {
    encounter_rate: 20,
    mons: [
        wild!(levels: 21..=21, species: SpeciesId::NUMEL),
        wild!(levels: 21..=21, species: SpeciesId::NUMEL),
        wild!(levels: 21..=21, species: SpeciesId::MACHOP),
        wild!(levels: 20..=20, species: SpeciesId::NUMEL),
        wild!(levels: 20..=20, species: SpeciesId::SPOINK),
        wild!(levels: 20..=20, species: SpeciesId::MACHOP),
        wild!(levels: 21..=21, species: SpeciesId::SPOINK),
        wild!(levels: 22..=22, species: SpeciesId::MACHOP),
        wild!(levels: 22..=22, species: SpeciesId::NUMEL),
        wild!(levels: 22..=22, species: SpeciesId::SPOINK),
        wild!(levels: 22..=22, species: SpeciesId::NUMEL),
        wild!(levels: 22..=22, species: SpeciesId::SPOINK),
    ],
};

const GROUTE106_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WINGULL),
        wild!(levels: 15..=25, species: SpeciesId::WINGULL),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
    ],
};

const GROUTE106_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WAILMER),
        wild!(levels: 25..=30, species: SpeciesId::WAILMER),
        wild!(levels: 30..=35, species: SpeciesId::WAILMER),
        wild!(levels: 20..=25, species: SpeciesId::WAILMER),
        wild!(levels: 35..=40, species: SpeciesId::WAILMER),
        wild!(levels: 40..=45, species: SpeciesId::WAILMER),
    ],
};

const GROUTE107_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WINGULL),
        wild!(levels: 15..=25, species: SpeciesId::WINGULL),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
    ],
};

const GROUTE107_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WAILMER),
        wild!(levels: 25..=30, species: SpeciesId::WAILMER),
        wild!(levels: 30..=35, species: SpeciesId::WAILMER),
        wild!(levels: 20..=25, species: SpeciesId::WAILMER),
        wild!(levels: 35..=40, species: SpeciesId::WAILMER),
        wild!(levels: 40..=45, species: SpeciesId::WAILMER),
    ],
};

const GROUTE108_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WINGULL),
        wild!(levels: 15..=25, species: SpeciesId::WINGULL),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
    ],
};

const GROUTE108_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WAILMER),
        wild!(levels: 25..=30, species: SpeciesId::WAILMER),
        wild!(levels: 30..=35, species: SpeciesId::WAILMER),
        wild!(levels: 20..=25, species: SpeciesId::WAILMER),
        wild!(levels: 35..=40, species: SpeciesId::WAILMER),
        wild!(levels: 40..=45, species: SpeciesId::WAILMER),
    ],
};

const GROUTE109_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WINGULL),
        wild!(levels: 15..=25, species: SpeciesId::WINGULL),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
    ],
};

const GROUTE109_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WAILMER),
        wild!(levels: 25..=30, species: SpeciesId::WAILMER),
        wild!(levels: 30..=35, species: SpeciesId::WAILMER),
        wild!(levels: 20..=25, species: SpeciesId::WAILMER),
        wild!(levels: 35..=40, species: SpeciesId::WAILMER),
        wild!(levels: 40..=45, species: SpeciesId::WAILMER),
    ],
};

const GROUTE115_LAND: LandEncounters = LandEncounters {
    encounter_rate: 20,
    mons: [
        wild!(levels: 23..=23, species: SpeciesId::SWABLU),
        wild!(levels: 23..=23, species: SpeciesId::TAILLOW),
        wild!(levels: 25..=25, species: SpeciesId::SWABLU),
        wild!(levels: 24..=24, species: SpeciesId::TAILLOW),
        wild!(levels: 25..=25, species: SpeciesId::TAILLOW),
        wild!(levels: 25..=25, species: SpeciesId::SWELLOW),
        wild!(levels: 24..=24, species: SpeciesId::JIGGLYPUFF),
        wild!(levels: 25..=25, species: SpeciesId::JIGGLYPUFF),
        wild!(levels: 24..=24, species: SpeciesId::WINGULL),
        wild!(levels: 24..=24, species: SpeciesId::WINGULL),
        wild!(levels: 26..=26, species: SpeciesId::WINGULL),
        wild!(levels: 25..=25, species: SpeciesId::WINGULL),
    ],
};

const GROUTE115_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WINGULL),
        wild!(levels: 15..=25, species: SpeciesId::WINGULL),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
    ],
};

const GROUTE115_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WAILMER),
        wild!(levels: 25..=30, species: SpeciesId::WAILMER),
        wild!(levels: 30..=35, species: SpeciesId::WAILMER),
        wild!(levels: 20..=25, species: SpeciesId::WAILMER),
        wild!(levels: 35..=40, species: SpeciesId::WAILMER),
        wild!(levels: 40..=45, species: SpeciesId::WAILMER),
    ],
};

const GNEWMAUVILLE_INSIDE_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 24..=24, species: SpeciesId::VOLTORB),
        wild!(levels: 24..=24, species: SpeciesId::MAGNEMITE),
        wild!(levels: 25..=25, species: SpeciesId::VOLTORB),
        wild!(levels: 25..=25, species: SpeciesId::MAGNEMITE),
        wild!(levels: 23..=23, species: SpeciesId::VOLTORB),
        wild!(levels: 23..=23, species: SpeciesId::MAGNEMITE),
        wild!(levels: 26..=26, species: SpeciesId::VOLTORB),
        wild!(levels: 26..=26, species: SpeciesId::MAGNEMITE),
        wild!(levels: 22..=22, species: SpeciesId::VOLTORB),
        wild!(levels: 22..=22, species: SpeciesId::MAGNEMITE),
        wild!(levels: 26..=26, species: SpeciesId::ELECTRODE),
        wild!(levels: 26..=26, species: SpeciesId::MAGNETON),
    ],
};

const GROUTE119_LAND: LandEncounters = LandEncounters {
    encounter_rate: 15,
    mons: [
        wild!(levels: 25..=25, species: SpeciesId::ZIGZAGOON),
        wild!(levels: 25..=25, species: SpeciesId::LINOONE),
        wild!(levels: 27..=27, species: SpeciesId::ZIGZAGOON),
        wild!(levels: 25..=25, species: SpeciesId::ODDISH),
        wild!(levels: 27..=27, species: SpeciesId::LINOONE),
        wild!(levels: 26..=26, species: SpeciesId::ODDISH),
        wild!(levels: 27..=27, species: SpeciesId::ODDISH),
        wild!(levels: 24..=24, species: SpeciesId::ODDISH),
        wild!(levels: 25..=25, species: SpeciesId::TROPIUS),
        wild!(levels: 26..=26, species: SpeciesId::TROPIUS),
        wild!(levels: 27..=27, species: SpeciesId::TROPIUS),
        wild!(levels: 25..=25, species: SpeciesId::KECLEON),
    ],
};

const GROUTE119_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WINGULL),
        wild!(levels: 15..=25, species: SpeciesId::WINGULL),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
    ],
};

const GROUTE119_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::CARVANHA),
        wild!(levels: 25..=30, species: SpeciesId::CARVANHA),
        wild!(levels: 30..=35, species: SpeciesId::CARVANHA),
        wild!(levels: 20..=25, species: SpeciesId::CARVANHA),
        wild!(levels: 35..=40, species: SpeciesId::CARVANHA),
        wild!(levels: 40..=45, species: SpeciesId::CARVANHA),
    ],
};

const GROUTE120_LAND: LandEncounters = LandEncounters {
    encounter_rate: 20,
    mons: [
        wild!(levels: 25..=25, species: SpeciesId::POOCHYENA),
        wild!(levels: 25..=25, species: SpeciesId::MIGHTYENA),
        wild!(levels: 27..=27, species: SpeciesId::MIGHTYENA),
        wild!(levels: 25..=25, species: SpeciesId::ODDISH),
        wild!(levels: 25..=25, species: SpeciesId::MARILL),
        wild!(levels: 26..=26, species: SpeciesId::ODDISH),
        wild!(levels: 27..=27, species: SpeciesId::ODDISH),
        wild!(levels: 27..=27, species: SpeciesId::MARILL),
        wild!(levels: 25..=25, species: SpeciesId::ABSOL),
        wild!(levels: 27..=27, species: SpeciesId::ABSOL),
        wild!(levels: 25..=25, species: SpeciesId::KECLEON),
        wild!(levels: 25..=25, species: SpeciesId::SEEDOT),
    ],
};

const GROUTE120_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 20..=30, species: SpeciesId::MARILL),
        wild!(levels: 10..=20, species: SpeciesId::MARILL),
        wild!(levels: 30..=35, species: SpeciesId::MARILL),
        wild!(levels: 5..=10, species: SpeciesId::MARILL),
        wild!(levels: 20..=30, species: SpeciesId::GOLDEEN),
    ],
};

const GROUTE120_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::GOLDEEN),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::GOLDEEN),
        wild!(levels: 10..=30, species: SpeciesId::BARBOACH),
        wild!(levels: 25..=30, species: SpeciesId::BARBOACH),
        wild!(levels: 30..=35, species: SpeciesId::BARBOACH),
        wild!(levels: 20..=25, species: SpeciesId::BARBOACH),
        wild!(levels: 35..=40, species: SpeciesId::BARBOACH),
        wild!(levels: 40..=45, species: SpeciesId::BARBOACH),
    ],
};

const GROUTE121_LAND: LandEncounters = LandEncounters {
    encounter_rate: 20,
    mons: [
        wild!(levels: 26..=26, species: SpeciesId::POOCHYENA),
        wild!(levels: 26..=26, species: SpeciesId::SHUPPET),
        wild!(levels: 26..=26, species: SpeciesId::MIGHTYENA),
        wild!(levels: 28..=28, species: SpeciesId::SHUPPET),
        wild!(levels: 28..=28, species: SpeciesId::MIGHTYENA),
        wild!(levels: 26..=26, species: SpeciesId::ODDISH),
        wild!(levels: 28..=28, species: SpeciesId::ODDISH),
        wild!(levels: 28..=28, species: SpeciesId::GLOOM),
        wild!(levels: 26..=26, species: SpeciesId::WINGULL),
        wild!(levels: 27..=27, species: SpeciesId::WINGULL),
        wild!(levels: 28..=28, species: SpeciesId::WINGULL),
        wild!(levels: 25..=25, species: SpeciesId::KECLEON),
    ],
};

const GROUTE121_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WINGULL),
        wild!(levels: 15..=25, species: SpeciesId::WINGULL),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
    ],
};

const GROUTE121_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WAILMER),
        wild!(levels: 25..=30, species: SpeciesId::WAILMER),
        wild!(levels: 30..=35, species: SpeciesId::WAILMER),
        wild!(levels: 20..=25, species: SpeciesId::WAILMER),
        wild!(levels: 35..=40, species: SpeciesId::WAILMER),
        wild!(levels: 40..=45, species: SpeciesId::WAILMER),
    ],
};

const GROUTE122_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WINGULL),
        wild!(levels: 15..=25, species: SpeciesId::WINGULL),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
    ],
};

const GROUTE122_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WAILMER),
        wild!(levels: 30..=35, species: SpeciesId::SHARPEDO),
        wild!(levels: 30..=35, species: SpeciesId::WAILMER),
        wild!(levels: 25..=30, species: SpeciesId::WAILMER),
        wild!(levels: 35..=40, species: SpeciesId::WAILMER),
        wild!(levels: 40..=45, species: SpeciesId::WAILMER),
    ],
};

const GROUTE123_LAND: LandEncounters = LandEncounters {
    encounter_rate: 20,
    mons: [
        wild!(levels: 26..=26, species: SpeciesId::POOCHYENA),
        wild!(levels: 26..=26, species: SpeciesId::SHUPPET),
        wild!(levels: 26..=26, species: SpeciesId::MIGHTYENA),
        wild!(levels: 28..=28, species: SpeciesId::SHUPPET),
        wild!(levels: 28..=28, species: SpeciesId::MIGHTYENA),
        wild!(levels: 26..=26, species: SpeciesId::ODDISH),
        wild!(levels: 28..=28, species: SpeciesId::ODDISH),
        wild!(levels: 28..=28, species: SpeciesId::GLOOM),
        wild!(levels: 26..=26, species: SpeciesId::WINGULL),
        wild!(levels: 27..=27, species: SpeciesId::WINGULL),
        wild!(levels: 28..=28, species: SpeciesId::WINGULL),
        wild!(levels: 25..=25, species: SpeciesId::KECLEON),
    ],
};

const GROUTE123_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WINGULL),
        wild!(levels: 15..=25, species: SpeciesId::WINGULL),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
    ],
};

const GROUTE123_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WAILMER),
        wild!(levels: 25..=30, species: SpeciesId::WAILMER),
        wild!(levels: 30..=35, species: SpeciesId::WAILMER),
        wild!(levels: 20..=25, species: SpeciesId::WAILMER),
        wild!(levels: 35..=40, species: SpeciesId::WAILMER),
        wild!(levels: 40..=45, species: SpeciesId::WAILMER),
    ],
};

const GMTPYRE_2F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 27..=27, species: SpeciesId::SHUPPET),
        wild!(levels: 28..=28, species: SpeciesId::SHUPPET),
        wild!(levels: 26..=26, species: SpeciesId::SHUPPET),
        wild!(levels: 25..=25, species: SpeciesId::SHUPPET),
        wild!(levels: 29..=29, species: SpeciesId::SHUPPET),
        wild!(levels: 24..=24, species: SpeciesId::SHUPPET),
        wild!(levels: 23..=23, species: SpeciesId::SHUPPET),
        wild!(levels: 22..=22, species: SpeciesId::SHUPPET),
        wild!(levels: 29..=29, species: SpeciesId::SHUPPET),
        wild!(levels: 24..=24, species: SpeciesId::SHUPPET),
        wild!(levels: 29..=29, species: SpeciesId::SHUPPET),
        wild!(levels: 24..=24, species: SpeciesId::SHUPPET),
    ],
};

const GMTPYRE_3F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 27..=27, species: SpeciesId::SHUPPET),
        wild!(levels: 28..=28, species: SpeciesId::SHUPPET),
        wild!(levels: 26..=26, species: SpeciesId::SHUPPET),
        wild!(levels: 25..=25, species: SpeciesId::SHUPPET),
        wild!(levels: 29..=29, species: SpeciesId::SHUPPET),
        wild!(levels: 24..=24, species: SpeciesId::SHUPPET),
        wild!(levels: 23..=23, species: SpeciesId::SHUPPET),
        wild!(levels: 22..=22, species: SpeciesId::SHUPPET),
        wild!(levels: 29..=29, species: SpeciesId::SHUPPET),
        wild!(levels: 24..=24, species: SpeciesId::SHUPPET),
        wild!(levels: 29..=29, species: SpeciesId::SHUPPET),
        wild!(levels: 24..=24, species: SpeciesId::SHUPPET),
    ],
};

const GMTPYRE_4F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 27..=27, species: SpeciesId::SHUPPET),
        wild!(levels: 28..=28, species: SpeciesId::SHUPPET),
        wild!(levels: 26..=26, species: SpeciesId::SHUPPET),
        wild!(levels: 25..=25, species: SpeciesId::SHUPPET),
        wild!(levels: 29..=29, species: SpeciesId::SHUPPET),
        wild!(levels: 24..=24, species: SpeciesId::SHUPPET),
        wild!(levels: 23..=23, species: SpeciesId::SHUPPET),
        wild!(levels: 22..=22, species: SpeciesId::SHUPPET),
        wild!(levels: 27..=27, species: SpeciesId::DUSKULL),
        wild!(levels: 27..=27, species: SpeciesId::DUSKULL),
        wild!(levels: 25..=25, species: SpeciesId::DUSKULL),
        wild!(levels: 29..=29, species: SpeciesId::DUSKULL),
    ],
};

const GMTPYRE_5F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 27..=27, species: SpeciesId::SHUPPET),
        wild!(levels: 28..=28, species: SpeciesId::SHUPPET),
        wild!(levels: 26..=26, species: SpeciesId::SHUPPET),
        wild!(levels: 25..=25, species: SpeciesId::SHUPPET),
        wild!(levels: 29..=29, species: SpeciesId::SHUPPET),
        wild!(levels: 24..=24, species: SpeciesId::SHUPPET),
        wild!(levels: 23..=23, species: SpeciesId::SHUPPET),
        wild!(levels: 22..=22, species: SpeciesId::SHUPPET),
        wild!(levels: 27..=27, species: SpeciesId::DUSKULL),
        wild!(levels: 27..=27, species: SpeciesId::DUSKULL),
        wild!(levels: 25..=25, species: SpeciesId::DUSKULL),
        wild!(levels: 29..=29, species: SpeciesId::DUSKULL),
    ],
};

const GMTPYRE_6F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 27..=27, species: SpeciesId::SHUPPET),
        wild!(levels: 28..=28, species: SpeciesId::SHUPPET),
        wild!(levels: 26..=26, species: SpeciesId::SHUPPET),
        wild!(levels: 25..=25, species: SpeciesId::SHUPPET),
        wild!(levels: 29..=29, species: SpeciesId::SHUPPET),
        wild!(levels: 24..=24, species: SpeciesId::SHUPPET),
        wild!(levels: 23..=23, species: SpeciesId::SHUPPET),
        wild!(levels: 22..=22, species: SpeciesId::SHUPPET),
        wild!(levels: 27..=27, species: SpeciesId::DUSKULL),
        wild!(levels: 27..=27, species: SpeciesId::DUSKULL),
        wild!(levels: 25..=25, species: SpeciesId::DUSKULL),
        wild!(levels: 29..=29, species: SpeciesId::DUSKULL),
    ],
};

const GMTPYRE_EXTERIOR_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 27..=27, species: SpeciesId::SHUPPET),
        wild!(levels: 27..=27, species: SpeciesId::SHUPPET),
        wild!(levels: 28..=28, species: SpeciesId::SHUPPET),
        wild!(levels: 29..=29, species: SpeciesId::SHUPPET),
        wild!(levels: 29..=29, species: SpeciesId::VULPIX),
        wild!(levels: 27..=27, species: SpeciesId::VULPIX),
        wild!(levels: 29..=29, species: SpeciesId::VULPIX),
        wild!(levels: 25..=25, species: SpeciesId::VULPIX),
        wild!(levels: 27..=27, species: SpeciesId::WINGULL),
        wild!(levels: 27..=27, species: SpeciesId::WINGULL),
        wild!(levels: 26..=26, species: SpeciesId::WINGULL),
        wild!(levels: 28..=28, species: SpeciesId::WINGULL),
    ],
};

const GMTPYRE_SUMMIT_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 28..=28, species: SpeciesId::SHUPPET),
        wild!(levels: 29..=29, species: SpeciesId::SHUPPET),
        wild!(levels: 27..=27, species: SpeciesId::SHUPPET),
        wild!(levels: 26..=26, species: SpeciesId::SHUPPET),
        wild!(levels: 30..=30, species: SpeciesId::SHUPPET),
        wild!(levels: 25..=25, species: SpeciesId::SHUPPET),
        wild!(levels: 24..=24, species: SpeciesId::SHUPPET),
        wild!(levels: 28..=28, species: SpeciesId::DUSKULL),
        wild!(levels: 26..=26, species: SpeciesId::DUSKULL),
        wild!(levels: 30..=30, species: SpeciesId::DUSKULL),
        wild!(levels: 28..=28, species: SpeciesId::CHIMECHO),
        wild!(levels: 28..=28, species: SpeciesId::CHIMECHO),
    ],
};

const GGRANITECAVE_STEVENSROOM_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 7..=7, species: SpeciesId::ZUBAT),
        wild!(levels: 8..=8, species: SpeciesId::MAKUHITA),
        wild!(levels: 7..=7, species: SpeciesId::MAKUHITA),
        wild!(levels: 8..=8, species: SpeciesId::ZUBAT),
        wild!(levels: 9..=9, species: SpeciesId::MAKUHITA),
        wild!(levels: 8..=8, species: SpeciesId::ABRA),
        wild!(levels: 10..=10, species: SpeciesId::MAKUHITA),
        wild!(levels: 6..=6, species: SpeciesId::MAKUHITA),
        wild!(levels: 7..=7, species: SpeciesId::ARON),
        wild!(levels: 8..=8, species: SpeciesId::ARON),
        wild!(levels: 7..=7, species: SpeciesId::ARON),
        wild!(levels: 8..=8, species: SpeciesId::ARON),
    ],
};

const GROUTE125_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WINGULL),
        wild!(levels: 15..=25, species: SpeciesId::WINGULL),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
    ],
};

const GROUTE125_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WAILMER),
        wild!(levels: 30..=35, species: SpeciesId::SHARPEDO),
        wild!(levels: 30..=35, species: SpeciesId::WAILMER),
        wild!(levels: 25..=30, species: SpeciesId::WAILMER),
        wild!(levels: 35..=40, species: SpeciesId::WAILMER),
        wild!(levels: 40..=45, species: SpeciesId::WAILMER),
    ],
};

const GROUTE126_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WINGULL),
        wild!(levels: 15..=25, species: SpeciesId::WINGULL),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
    ],
};

const GROUTE126_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WAILMER),
        wild!(levels: 30..=35, species: SpeciesId::SHARPEDO),
        wild!(levels: 30..=35, species: SpeciesId::WAILMER),
        wild!(levels: 25..=30, species: SpeciesId::WAILMER),
        wild!(levels: 35..=40, species: SpeciesId::WAILMER),
        wild!(levels: 40..=45, species: SpeciesId::WAILMER),
    ],
};

const GROUTE127_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WINGULL),
        wild!(levels: 15..=25, species: SpeciesId::WINGULL),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
    ],
};

const GROUTE127_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WAILMER),
        wild!(levels: 30..=35, species: SpeciesId::SHARPEDO),
        wild!(levels: 30..=35, species: SpeciesId::WAILMER),
        wild!(levels: 25..=30, species: SpeciesId::WAILMER),
        wild!(levels: 35..=40, species: SpeciesId::WAILMER),
        wild!(levels: 40..=45, species: SpeciesId::WAILMER),
    ],
};

const GROUTE128_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WINGULL),
        wild!(levels: 15..=25, species: SpeciesId::WINGULL),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
    ],
};

const GROUTE128_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::LUVDISC),
        wild!(levels: 10..=30, species: SpeciesId::WAILMER),
        wild!(levels: 30..=35, species: SpeciesId::LUVDISC),
        wild!(levels: 30..=35, species: SpeciesId::WAILMER),
        wild!(levels: 30..=35, species: SpeciesId::CORSOLA),
        wild!(levels: 35..=40, species: SpeciesId::WAILMER),
        wild!(levels: 40..=45, species: SpeciesId::WAILMER),
    ],
};

const GROUTE129_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WINGULL),
        wild!(levels: 15..=25, species: SpeciesId::WINGULL),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
        wild!(levels: 25..=30, species: SpeciesId::WAILORD),
    ],
};

const GROUTE129_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WAILMER),
        wild!(levels: 30..=35, species: SpeciesId::SHARPEDO),
        wild!(levels: 30..=35, species: SpeciesId::WAILMER),
        wild!(levels: 25..=30, species: SpeciesId::WAILMER),
        wild!(levels: 35..=40, species: SpeciesId::WAILMER),
        wild!(levels: 40..=45, species: SpeciesId::WAILMER),
    ],
};

const GROUTE130_LAND: LandEncounters = LandEncounters {
    encounter_rate: 20,
    mons: [
        wild!(levels: 30..=30, species: SpeciesId::WYNAUT),
        wild!(levels: 35..=35, species: SpeciesId::WYNAUT),
        wild!(levels: 25..=25, species: SpeciesId::WYNAUT),
        wild!(levels: 40..=40, species: SpeciesId::WYNAUT),
        wild!(levels: 20..=20, species: SpeciesId::WYNAUT),
        wild!(levels: 45..=45, species: SpeciesId::WYNAUT),
        wild!(levels: 15..=15, species: SpeciesId::WYNAUT),
        wild!(levels: 50..=50, species: SpeciesId::WYNAUT),
        wild!(levels: 10..=10, species: SpeciesId::WYNAUT),
        wild!(levels: 5..=5, species: SpeciesId::WYNAUT),
        wild!(levels: 10..=10, species: SpeciesId::WYNAUT),
        wild!(levels: 5..=5, species: SpeciesId::WYNAUT),
    ],
};

const GROUTE130_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WINGULL),
        wild!(levels: 15..=25, species: SpeciesId::WINGULL),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
    ],
};

const GROUTE130_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WAILMER),
        wild!(levels: 30..=35, species: SpeciesId::SHARPEDO),
        wild!(levels: 30..=35, species: SpeciesId::WAILMER),
        wild!(levels: 25..=30, species: SpeciesId::WAILMER),
        wild!(levels: 35..=40, species: SpeciesId::WAILMER),
        wild!(levels: 40..=45, species: SpeciesId::WAILMER),
    ],
};

const GROUTE131_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WINGULL),
        wild!(levels: 15..=25, species: SpeciesId::WINGULL),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
    ],
};

const GROUTE131_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WAILMER),
        wild!(levels: 30..=35, species: SpeciesId::SHARPEDO),
        wild!(levels: 30..=35, species: SpeciesId::WAILMER),
        wild!(levels: 25..=30, species: SpeciesId::WAILMER),
        wild!(levels: 35..=40, species: SpeciesId::WAILMER),
        wild!(levels: 40..=45, species: SpeciesId::WAILMER),
    ],
};

const GROUTE132_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WINGULL),
        wild!(levels: 15..=25, species: SpeciesId::WINGULL),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
    ],
};

const GROUTE132_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WAILMER),
        wild!(levels: 30..=35, species: SpeciesId::SHARPEDO),
        wild!(levels: 30..=35, species: SpeciesId::WAILMER),
        wild!(levels: 25..=30, species: SpeciesId::HORSEA),
        wild!(levels: 35..=40, species: SpeciesId::WAILMER),
        wild!(levels: 40..=45, species: SpeciesId::WAILMER),
    ],
};

const GROUTE133_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WINGULL),
        wild!(levels: 15..=25, species: SpeciesId::WINGULL),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
    ],
};

const GROUTE133_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WAILMER),
        wild!(levels: 30..=35, species: SpeciesId::SHARPEDO),
        wild!(levels: 30..=35, species: SpeciesId::WAILMER),
        wild!(levels: 25..=30, species: SpeciesId::HORSEA),
        wild!(levels: 35..=40, species: SpeciesId::WAILMER),
        wild!(levels: 40..=45, species: SpeciesId::WAILMER),
    ],
};

const GROUTE134_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WINGULL),
        wild!(levels: 15..=25, species: SpeciesId::WINGULL),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
    ],
};

const GROUTE134_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WAILMER),
        wild!(levels: 30..=35, species: SpeciesId::SHARPEDO),
        wild!(levels: 30..=35, species: SpeciesId::WAILMER),
        wild!(levels: 25..=30, species: SpeciesId::HORSEA),
        wild!(levels: 35..=40, species: SpeciesId::WAILMER),
        wild!(levels: 40..=45, species: SpeciesId::WAILMER),
    ],
};

const GABANDONEDSHIP_HIDDENFLOORCORRIDORS_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 30..=35, species: SpeciesId::TENTACRUEL),
    ],
};

const GABANDONEDSHIP_HIDDENFLOORCORRIDORS_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 20,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 25..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 30..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 30..=35, species: SpeciesId::TENTACRUEL),
        wild!(levels: 25..=30, species: SpeciesId::TENTACRUEL),
        wild!(levels: 20..=25, species: SpeciesId::TENTACRUEL),
    ],
};

const GSEAFLOORCAVERN_ROOM1_LAND: LandEncounters = LandEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 30..=30, species: SpeciesId::ZUBAT),
        wild!(levels: 31..=31, species: SpeciesId::ZUBAT),
        wild!(levels: 32..=32, species: SpeciesId::ZUBAT),
        wild!(levels: 33..=33, species: SpeciesId::ZUBAT),
        wild!(levels: 28..=28, species: SpeciesId::ZUBAT),
        wild!(levels: 29..=29, species: SpeciesId::ZUBAT),
        wild!(levels: 34..=34, species: SpeciesId::ZUBAT),
        wild!(levels: 35..=35, species: SpeciesId::ZUBAT),
        wild!(levels: 34..=34, species: SpeciesId::GOLBAT),
        wild!(levels: 35..=35, species: SpeciesId::GOLBAT),
        wild!(levels: 33..=33, species: SpeciesId::GOLBAT),
        wild!(levels: 36..=36, species: SpeciesId::GOLBAT),
    ],
};

const GSEAFLOORCAVERN_ROOM2_LAND: LandEncounters = LandEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 30..=30, species: SpeciesId::ZUBAT),
        wild!(levels: 31..=31, species: SpeciesId::ZUBAT),
        wild!(levels: 32..=32, species: SpeciesId::ZUBAT),
        wild!(levels: 33..=33, species: SpeciesId::ZUBAT),
        wild!(levels: 28..=28, species: SpeciesId::ZUBAT),
        wild!(levels: 29..=29, species: SpeciesId::ZUBAT),
        wild!(levels: 34..=34, species: SpeciesId::ZUBAT),
        wild!(levels: 35..=35, species: SpeciesId::ZUBAT),
        wild!(levels: 34..=34, species: SpeciesId::GOLBAT),
        wild!(levels: 35..=35, species: SpeciesId::GOLBAT),
        wild!(levels: 33..=33, species: SpeciesId::GOLBAT),
        wild!(levels: 36..=36, species: SpeciesId::GOLBAT),
    ],
};

const GSEAFLOORCAVERN_ROOM3_LAND: LandEncounters = LandEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 30..=30, species: SpeciesId::ZUBAT),
        wild!(levels: 31..=31, species: SpeciesId::ZUBAT),
        wild!(levels: 32..=32, species: SpeciesId::ZUBAT),
        wild!(levels: 33..=33, species: SpeciesId::ZUBAT),
        wild!(levels: 28..=28, species: SpeciesId::ZUBAT),
        wild!(levels: 29..=29, species: SpeciesId::ZUBAT),
        wild!(levels: 34..=34, species: SpeciesId::ZUBAT),
        wild!(levels: 35..=35, species: SpeciesId::ZUBAT),
        wild!(levels: 34..=34, species: SpeciesId::GOLBAT),
        wild!(levels: 35..=35, species: SpeciesId::GOLBAT),
        wild!(levels: 33..=33, species: SpeciesId::GOLBAT),
        wild!(levels: 36..=36, species: SpeciesId::GOLBAT),
    ],
};

const GSEAFLOORCAVERN_ROOM4_LAND: LandEncounters = LandEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 30..=30, species: SpeciesId::ZUBAT),
        wild!(levels: 31..=31, species: SpeciesId::ZUBAT),
        wild!(levels: 32..=32, species: SpeciesId::ZUBAT),
        wild!(levels: 33..=33, species: SpeciesId::ZUBAT),
        wild!(levels: 28..=28, species: SpeciesId::ZUBAT),
        wild!(levels: 29..=29, species: SpeciesId::ZUBAT),
        wild!(levels: 34..=34, species: SpeciesId::ZUBAT),
        wild!(levels: 35..=35, species: SpeciesId::ZUBAT),
        wild!(levels: 34..=34, species: SpeciesId::GOLBAT),
        wild!(levels: 35..=35, species: SpeciesId::GOLBAT),
        wild!(levels: 33..=33, species: SpeciesId::GOLBAT),
        wild!(levels: 36..=36, species: SpeciesId::GOLBAT),
    ],
};

const GSEAFLOORCAVERN_ROOM5_LAND: LandEncounters = LandEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 30..=30, species: SpeciesId::ZUBAT),
        wild!(levels: 31..=31, species: SpeciesId::ZUBAT),
        wild!(levels: 32..=32, species: SpeciesId::ZUBAT),
        wild!(levels: 33..=33, species: SpeciesId::ZUBAT),
        wild!(levels: 28..=28, species: SpeciesId::ZUBAT),
        wild!(levels: 29..=29, species: SpeciesId::ZUBAT),
        wild!(levels: 34..=34, species: SpeciesId::ZUBAT),
        wild!(levels: 35..=35, species: SpeciesId::ZUBAT),
        wild!(levels: 34..=34, species: SpeciesId::GOLBAT),
        wild!(levels: 35..=35, species: SpeciesId::GOLBAT),
        wild!(levels: 33..=33, species: SpeciesId::GOLBAT),
        wild!(levels: 36..=36, species: SpeciesId::GOLBAT),
    ],
};

const GSEAFLOORCAVERN_ROOM6_LAND: LandEncounters = LandEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 30..=30, species: SpeciesId::ZUBAT),
        wild!(levels: 31..=31, species: SpeciesId::ZUBAT),
        wild!(levels: 32..=32, species: SpeciesId::ZUBAT),
        wild!(levels: 33..=33, species: SpeciesId::ZUBAT),
        wild!(levels: 28..=28, species: SpeciesId::ZUBAT),
        wild!(levels: 29..=29, species: SpeciesId::ZUBAT),
        wild!(levels: 34..=34, species: SpeciesId::ZUBAT),
        wild!(levels: 35..=35, species: SpeciesId::ZUBAT),
        wild!(levels: 34..=34, species: SpeciesId::GOLBAT),
        wild!(levels: 35..=35, species: SpeciesId::GOLBAT),
        wild!(levels: 33..=33, species: SpeciesId::GOLBAT),
        wild!(levels: 36..=36, species: SpeciesId::GOLBAT),
    ],
};

const GSEAFLOORCAVERN_ROOM6_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 5..=35, species: SpeciesId::ZUBAT),
        wild!(levels: 30..=35, species: SpeciesId::ZUBAT),
        wild!(levels: 30..=35, species: SpeciesId::GOLBAT),
        wild!(levels: 30..=35, species: SpeciesId::GOLBAT),
    ],
};

const GSEAFLOORCAVERN_ROOM6_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WAILMER),
        wild!(levels: 25..=30, species: SpeciesId::WAILMER),
        wild!(levels: 30..=35, species: SpeciesId::WAILMER),
        wild!(levels: 20..=25, species: SpeciesId::WAILMER),
        wild!(levels: 35..=40, species: SpeciesId::WAILMER),
        wild!(levels: 40..=45, species: SpeciesId::WAILMER),
    ],
};

const GSEAFLOORCAVERN_ROOM7_LAND: LandEncounters = LandEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 30..=30, species: SpeciesId::ZUBAT),
        wild!(levels: 31..=31, species: SpeciesId::ZUBAT),
        wild!(levels: 32..=32, species: SpeciesId::ZUBAT),
        wild!(levels: 33..=33, species: SpeciesId::ZUBAT),
        wild!(levels: 28..=28, species: SpeciesId::ZUBAT),
        wild!(levels: 29..=29, species: SpeciesId::ZUBAT),
        wild!(levels: 34..=34, species: SpeciesId::ZUBAT),
        wild!(levels: 35..=35, species: SpeciesId::ZUBAT),
        wild!(levels: 34..=34, species: SpeciesId::GOLBAT),
        wild!(levels: 35..=35, species: SpeciesId::GOLBAT),
        wild!(levels: 33..=33, species: SpeciesId::GOLBAT),
        wild!(levels: 36..=36, species: SpeciesId::GOLBAT),
    ],
};

const GSEAFLOORCAVERN_ROOM7_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 5..=35, species: SpeciesId::ZUBAT),
        wild!(levels: 30..=35, species: SpeciesId::ZUBAT),
        wild!(levels: 30..=35, species: SpeciesId::GOLBAT),
        wild!(levels: 30..=35, species: SpeciesId::GOLBAT),
    ],
};

const GSEAFLOORCAVERN_ROOM7_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WAILMER),
        wild!(levels: 25..=30, species: SpeciesId::WAILMER),
        wild!(levels: 30..=35, species: SpeciesId::WAILMER),
        wild!(levels: 20..=25, species: SpeciesId::WAILMER),
        wild!(levels: 35..=40, species: SpeciesId::WAILMER),
        wild!(levels: 40..=45, species: SpeciesId::WAILMER),
    ],
};

const GSEAFLOORCAVERN_ROOM8_LAND: LandEncounters = LandEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 30..=30, species: SpeciesId::ZUBAT),
        wild!(levels: 31..=31, species: SpeciesId::ZUBAT),
        wild!(levels: 32..=32, species: SpeciesId::ZUBAT),
        wild!(levels: 33..=33, species: SpeciesId::ZUBAT),
        wild!(levels: 28..=28, species: SpeciesId::ZUBAT),
        wild!(levels: 29..=29, species: SpeciesId::ZUBAT),
        wild!(levels: 34..=34, species: SpeciesId::ZUBAT),
        wild!(levels: 35..=35, species: SpeciesId::ZUBAT),
        wild!(levels: 34..=34, species: SpeciesId::GOLBAT),
        wild!(levels: 35..=35, species: SpeciesId::GOLBAT),
        wild!(levels: 33..=33, species: SpeciesId::GOLBAT),
        wild!(levels: 36..=36, species: SpeciesId::GOLBAT),
    ],
};

const GSEAFLOORCAVERN_ENTRANCE_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 5..=35, species: SpeciesId::ZUBAT),
        wild!(levels: 30..=35, species: SpeciesId::ZUBAT),
        wild!(levels: 30..=35, species: SpeciesId::GOLBAT),
        wild!(levels: 30..=35, species: SpeciesId::GOLBAT),
    ],
};

const GSEAFLOORCAVERN_ENTRANCE_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WAILMER),
        wild!(levels: 25..=30, species: SpeciesId::WAILMER),
        wild!(levels: 30..=35, species: SpeciesId::WAILMER),
        wild!(levels: 20..=25, species: SpeciesId::WAILMER),
        wild!(levels: 35..=40, species: SpeciesId::WAILMER),
        wild!(levels: 40..=45, species: SpeciesId::WAILMER),
    ],
};

const GCAVEOFORIGIN_ENTRANCE_LAND: LandEncounters = LandEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 30..=30, species: SpeciesId::ZUBAT),
        wild!(levels: 31..=31, species: SpeciesId::ZUBAT),
        wild!(levels: 32..=32, species: SpeciesId::ZUBAT),
        wild!(levels: 33..=33, species: SpeciesId::ZUBAT),
        wild!(levels: 28..=28, species: SpeciesId::ZUBAT),
        wild!(levels: 29..=29, species: SpeciesId::ZUBAT),
        wild!(levels: 34..=34, species: SpeciesId::ZUBAT),
        wild!(levels: 35..=35, species: SpeciesId::ZUBAT),
        wild!(levels: 34..=34, species: SpeciesId::GOLBAT),
        wild!(levels: 35..=35, species: SpeciesId::GOLBAT),
        wild!(levels: 33..=33, species: SpeciesId::GOLBAT),
        wild!(levels: 36..=36, species: SpeciesId::GOLBAT),
    ],
};

const GCAVEOFORIGIN_1F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 30..=30, species: SpeciesId::ZUBAT),
        wild!(levels: 31..=31, species: SpeciesId::ZUBAT),
        wild!(levels: 32..=32, species: SpeciesId::ZUBAT),
        wild!(levels: 30..=30, species: SpeciesId::SABLEYE),
        wild!(levels: 32..=32, species: SpeciesId::SABLEYE),
        wild!(levels: 34..=34, species: SpeciesId::SABLEYE),
        wild!(levels: 33..=33, species: SpeciesId::ZUBAT),
        wild!(levels: 34..=34, species: SpeciesId::ZUBAT),
        wild!(levels: 34..=34, species: SpeciesId::GOLBAT),
        wild!(levels: 35..=35, species: SpeciesId::GOLBAT),
        wild!(levels: 33..=33, species: SpeciesId::GOLBAT),
        wild!(levels: 36..=36, species: SpeciesId::GOLBAT),
    ],
};

const GCAVEOFORIGIN_UNUSEDRUBYSAPPHIREMAP1_LAND: LandEncounters = LandEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 30..=30, species: SpeciesId::ZUBAT),
        wild!(levels: 31..=31, species: SpeciesId::ZUBAT),
        wild!(levels: 32..=32, species: SpeciesId::ZUBAT),
        wild!(levels: 30..=30, species: SpeciesId::SABLEYE),
        wild!(levels: 32..=32, species: SpeciesId::SABLEYE),
        wild!(levels: 34..=34, species: SpeciesId::SABLEYE),
        wild!(levels: 33..=33, species: SpeciesId::ZUBAT),
        wild!(levels: 34..=34, species: SpeciesId::ZUBAT),
        wild!(levels: 34..=34, species: SpeciesId::GOLBAT),
        wild!(levels: 35..=35, species: SpeciesId::GOLBAT),
        wild!(levels: 33..=33, species: SpeciesId::GOLBAT),
        wild!(levels: 36..=36, species: SpeciesId::GOLBAT),
    ],
};

const GCAVEOFORIGIN_UNUSEDRUBYSAPPHIREMAP2_LAND: LandEncounters = LandEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 30..=30, species: SpeciesId::ZUBAT),
        wild!(levels: 31..=31, species: SpeciesId::ZUBAT),
        wild!(levels: 32..=32, species: SpeciesId::ZUBAT),
        wild!(levels: 30..=30, species: SpeciesId::SABLEYE),
        wild!(levels: 32..=32, species: SpeciesId::SABLEYE),
        wild!(levels: 34..=34, species: SpeciesId::SABLEYE),
        wild!(levels: 33..=33, species: SpeciesId::ZUBAT),
        wild!(levels: 34..=34, species: SpeciesId::ZUBAT),
        wild!(levels: 34..=34, species: SpeciesId::GOLBAT),
        wild!(levels: 35..=35, species: SpeciesId::GOLBAT),
        wild!(levels: 33..=33, species: SpeciesId::GOLBAT),
        wild!(levels: 36..=36, species: SpeciesId::GOLBAT),
    ],
};

const GCAVEOFORIGIN_UNUSEDRUBYSAPPHIREMAP3_LAND: LandEncounters = LandEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 30..=30, species: SpeciesId::ZUBAT),
        wild!(levels: 31..=31, species: SpeciesId::ZUBAT),
        wild!(levels: 32..=32, species: SpeciesId::ZUBAT),
        wild!(levels: 30..=30, species: SpeciesId::SABLEYE),
        wild!(levels: 32..=32, species: SpeciesId::SABLEYE),
        wild!(levels: 34..=34, species: SpeciesId::SABLEYE),
        wild!(levels: 33..=33, species: SpeciesId::ZUBAT),
        wild!(levels: 34..=34, species: SpeciesId::ZUBAT),
        wild!(levels: 34..=34, species: SpeciesId::GOLBAT),
        wild!(levels: 35..=35, species: SpeciesId::GOLBAT),
        wild!(levels: 33..=33, species: SpeciesId::GOLBAT),
        wild!(levels: 36..=36, species: SpeciesId::GOLBAT),
    ],
};

const GNEWMAUVILLE_ENTRANCE_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 24..=24, species: SpeciesId::VOLTORB),
        wild!(levels: 24..=24, species: SpeciesId::MAGNEMITE),
        wild!(levels: 25..=25, species: SpeciesId::VOLTORB),
        wild!(levels: 25..=25, species: SpeciesId::MAGNEMITE),
        wild!(levels: 23..=23, species: SpeciesId::VOLTORB),
        wild!(levels: 23..=23, species: SpeciesId::MAGNEMITE),
        wild!(levels: 26..=26, species: SpeciesId::VOLTORB),
        wild!(levels: 26..=26, species: SpeciesId::MAGNEMITE),
        wild!(levels: 22..=22, species: SpeciesId::VOLTORB),
        wild!(levels: 22..=22, species: SpeciesId::MAGNEMITE),
        wild!(levels: 22..=22, species: SpeciesId::VOLTORB),
        wild!(levels: 22..=22, species: SpeciesId::MAGNEMITE),
    ],
};

const GSAFARIZONE_SOUTHWEST_LAND: LandEncounters = LandEncounters {
    encounter_rate: 25,
    mons: [
        wild!(levels: 25..=25, species: SpeciesId::ODDISH),
        wild!(levels: 27..=27, species: SpeciesId::ODDISH),
        wild!(levels: 25..=25, species: SpeciesId::GIRAFARIG),
        wild!(levels: 27..=27, species: SpeciesId::GIRAFARIG),
        wild!(levels: 25..=25, species: SpeciesId::NATU),
        wild!(levels: 27..=27, species: SpeciesId::DODUO),
        wild!(levels: 25..=25, species: SpeciesId::GLOOM),
        wild!(levels: 27..=27, species: SpeciesId::WOBBUFFET),
        wild!(levels: 25..=25, species: SpeciesId::PIKACHU),
        wild!(levels: 27..=27, species: SpeciesId::WOBBUFFET),
        wild!(levels: 27..=27, species: SpeciesId::PIKACHU),
        wild!(levels: 29..=29, species: SpeciesId::WOBBUFFET),
    ],
};

const GSAFARIZONE_SOUTHWEST_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 9,
    mons: [
        wild!(levels: 20..=30, species: SpeciesId::PSYDUCK),
        wild!(levels: 20..=30, species: SpeciesId::PSYDUCK),
        wild!(levels: 30..=35, species: SpeciesId::PSYDUCK),
        wild!(levels: 30..=35, species: SpeciesId::PSYDUCK),
        wild!(levels: 30..=35, species: SpeciesId::PSYDUCK),
    ],
};

const GSAFARIZONE_SOUTHWEST_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 35,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::GOLDEEN),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=25, species: SpeciesId::GOLDEEN),
        wild!(levels: 10..=30, species: SpeciesId::GOLDEEN),
        wild!(levels: 25..=30, species: SpeciesId::GOLDEEN),
        wild!(levels: 30..=35, species: SpeciesId::GOLDEEN),
        wild!(levels: 30..=35, species: SpeciesId::SEAKING),
        wild!(levels: 35..=40, species: SpeciesId::SEAKING),
        wild!(levels: 25..=30, species: SpeciesId::SEAKING),
    ],
};

const GSAFARIZONE_NORTH_LAND: LandEncounters = LandEncounters {
    encounter_rate: 25,
    mons: [
        wild!(levels: 27..=27, species: SpeciesId::PHANPY),
        wild!(levels: 27..=27, species: SpeciesId::ODDISH),
        wild!(levels: 29..=29, species: SpeciesId::PHANPY),
        wild!(levels: 29..=29, species: SpeciesId::ODDISH),
        wild!(levels: 27..=27, species: SpeciesId::NATU),
        wild!(levels: 29..=29, species: SpeciesId::GLOOM),
        wild!(levels: 31..=31, species: SpeciesId::GLOOM),
        wild!(levels: 29..=29, species: SpeciesId::NATU),
        wild!(levels: 29..=29, species: SpeciesId::XATU),
        wild!(levels: 27..=27, species: SpeciesId::HERACROSS),
        wild!(levels: 31..=31, species: SpeciesId::XATU),
        wild!(levels: 29..=29, species: SpeciesId::HERACROSS),
    ],
};

const GSAFARIZONE_NORTH_ROCKSMASH: RockSmashEncounters = RockSmashEncounters {
    encounter_rate: 25,
    mons: [
        wild!(levels: 10..=15, species: SpeciesId::GEODUDE),
        wild!(levels: 5..=10, species: SpeciesId::GEODUDE),
        wild!(levels: 15..=20, species: SpeciesId::GEODUDE),
        wild!(levels: 20..=25, species: SpeciesId::GEODUDE),
        wild!(levels: 25..=30, species: SpeciesId::GEODUDE),
    ],
};

const GSAFARIZONE_NORTHWEST_LAND: LandEncounters = LandEncounters {
    encounter_rate: 25,
    mons: [
        wild!(levels: 27..=27, species: SpeciesId::RHYHORN),
        wild!(levels: 27..=27, species: SpeciesId::ODDISH),
        wild!(levels: 29..=29, species: SpeciesId::RHYHORN),
        wild!(levels: 29..=29, species: SpeciesId::ODDISH),
        wild!(levels: 27..=27, species: SpeciesId::DODUO),
        wild!(levels: 29..=29, species: SpeciesId::GLOOM),
        wild!(levels: 31..=31, species: SpeciesId::GLOOM),
        wild!(levels: 29..=29, species: SpeciesId::DODUO),
        wild!(levels: 29..=29, species: SpeciesId::DODRIO),
        wild!(levels: 27..=27, species: SpeciesId::PINSIR),
        wild!(levels: 31..=31, species: SpeciesId::DODRIO),
        wild!(levels: 29..=29, species: SpeciesId::PINSIR),
    ],
};

const GSAFARIZONE_NORTHWEST_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 9,
    mons: [
        wild!(levels: 20..=30, species: SpeciesId::PSYDUCK),
        wild!(levels: 20..=30, species: SpeciesId::PSYDUCK),
        wild!(levels: 30..=35, species: SpeciesId::PSYDUCK),
        wild!(levels: 30..=35, species: SpeciesId::GOLDUCK),
        wild!(levels: 25..=40, species: SpeciesId::GOLDUCK),
    ],
};

const GSAFARIZONE_NORTHWEST_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 35,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::GOLDEEN),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=25, species: SpeciesId::GOLDEEN),
        wild!(levels: 10..=30, species: SpeciesId::GOLDEEN),
        wild!(levels: 25..=30, species: SpeciesId::GOLDEEN),
        wild!(levels: 30..=35, species: SpeciesId::GOLDEEN),
        wild!(levels: 30..=35, species: SpeciesId::SEAKING),
        wild!(levels: 35..=40, species: SpeciesId::SEAKING),
        wild!(levels: 25..=30, species: SpeciesId::SEAKING),
    ],
};

const GVICTORYROAD_B1F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 40..=40, species: SpeciesId::GOLBAT),
        wild!(levels: 40..=40, species: SpeciesId::HARIYAMA),
        wild!(levels: 40..=40, species: SpeciesId::LAIRON),
        wild!(levels: 40..=40, species: SpeciesId::LAIRON),
        wild!(levels: 38..=38, species: SpeciesId::GOLBAT),
        wild!(levels: 38..=38, species: SpeciesId::HARIYAMA),
        wild!(levels: 42..=42, species: SpeciesId::GOLBAT),
        wild!(levels: 42..=42, species: SpeciesId::HARIYAMA),
        wild!(levels: 42..=42, species: SpeciesId::LAIRON),
        wild!(levels: 38..=38, species: SpeciesId::MAWILE),
        wild!(levels: 42..=42, species: SpeciesId::LAIRON),
        wild!(levels: 38..=38, species: SpeciesId::MAWILE),
    ],
};

const GVICTORYROAD_B1F_ROCKSMASH: RockSmashEncounters = RockSmashEncounters {
    encounter_rate: 20,
    mons: [
        wild!(levels: 30..=40, species: SpeciesId::GRAVELER),
        wild!(levels: 30..=40, species: SpeciesId::GEODUDE),
        wild!(levels: 35..=40, species: SpeciesId::GRAVELER),
        wild!(levels: 35..=40, species: SpeciesId::GRAVELER),
        wild!(levels: 35..=40, species: SpeciesId::GRAVELER),
    ],
};

const GVICTORYROAD_B2F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 40..=40, species: SpeciesId::GOLBAT),
        wild!(levels: 40..=40, species: SpeciesId::SABLEYE),
        wild!(levels: 40..=40, species: SpeciesId::LAIRON),
        wild!(levels: 40..=40, species: SpeciesId::LAIRON),
        wild!(levels: 42..=42, species: SpeciesId::GOLBAT),
        wild!(levels: 42..=42, species: SpeciesId::SABLEYE),
        wild!(levels: 44..=44, species: SpeciesId::GOLBAT),
        wild!(levels: 44..=44, species: SpeciesId::SABLEYE),
        wild!(levels: 42..=42, species: SpeciesId::LAIRON),
        wild!(levels: 42..=42, species: SpeciesId::MAWILE),
        wild!(levels: 44..=44, species: SpeciesId::LAIRON),
        wild!(levels: 44..=44, species: SpeciesId::MAWILE),
    ],
};

const GVICTORYROAD_B2F_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 30..=35, species: SpeciesId::GOLBAT),
        wild!(levels: 25..=30, species: SpeciesId::GOLBAT),
        wild!(levels: 35..=40, species: SpeciesId::GOLBAT),
        wild!(levels: 35..=40, species: SpeciesId::GOLBAT),
        wild!(levels: 35..=40, species: SpeciesId::GOLBAT),
    ],
};

const GVICTORYROAD_B2F_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::GOLDEEN),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::GOLDEEN),
        wild!(levels: 10..=30, species: SpeciesId::BARBOACH),
        wild!(levels: 25..=30, species: SpeciesId::BARBOACH),
        wild!(levels: 30..=35, species: SpeciesId::BARBOACH),
        wild!(levels: 30..=35, species: SpeciesId::WHISCASH),
        wild!(levels: 35..=40, species: SpeciesId::WHISCASH),
        wild!(levels: 40..=45, species: SpeciesId::WHISCASH),
    ],
};

const GMETEORFALLS_1F_1R_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 16..=16, species: SpeciesId::ZUBAT),
        wild!(levels: 17..=17, species: SpeciesId::ZUBAT),
        wild!(levels: 18..=18, species: SpeciesId::ZUBAT),
        wild!(levels: 15..=15, species: SpeciesId::ZUBAT),
        wild!(levels: 14..=14, species: SpeciesId::ZUBAT),
        wild!(levels: 16..=16, species: SpeciesId::SOLROCK),
        wild!(levels: 18..=18, species: SpeciesId::SOLROCK),
        wild!(levels: 14..=14, species: SpeciesId::SOLROCK),
        wild!(levels: 19..=19, species: SpeciesId::ZUBAT),
        wild!(levels: 20..=20, species: SpeciesId::ZUBAT),
        wild!(levels: 19..=19, species: SpeciesId::ZUBAT),
        wild!(levels: 20..=20, species: SpeciesId::ZUBAT),
    ],
};

const GMETEORFALLS_1F_1R_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::ZUBAT),
        wild!(levels: 30..=35, species: SpeciesId::ZUBAT),
        wild!(levels: 25..=35, species: SpeciesId::SOLROCK),
        wild!(levels: 15..=25, species: SpeciesId::SOLROCK),
        wild!(levels: 5..=15, species: SpeciesId::SOLROCK),
    ],
};

const GMETEORFALLS_1F_1R_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::GOLDEEN),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::GOLDEEN),
        wild!(levels: 10..=30, species: SpeciesId::BARBOACH),
        wild!(levels: 25..=30, species: SpeciesId::BARBOACH),
        wild!(levels: 30..=35, species: SpeciesId::BARBOACH),
        wild!(levels: 20..=25, species: SpeciesId::BARBOACH),
        wild!(levels: 35..=40, species: SpeciesId::BARBOACH),
        wild!(levels: 40..=45, species: SpeciesId::BARBOACH),
    ],
};

const GMETEORFALLS_1F_2R_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 33..=33, species: SpeciesId::GOLBAT),
        wild!(levels: 35..=35, species: SpeciesId::GOLBAT),
        wild!(levels: 33..=33, species: SpeciesId::GOLBAT),
        wild!(levels: 35..=35, species: SpeciesId::SOLROCK),
        wild!(levels: 33..=33, species: SpeciesId::SOLROCK),
        wild!(levels: 37..=37, species: SpeciesId::SOLROCK),
        wild!(levels: 35..=35, species: SpeciesId::GOLBAT),
        wild!(levels: 39..=39, species: SpeciesId::SOLROCK),
        wild!(levels: 38..=38, species: SpeciesId::GOLBAT),
        wild!(levels: 40..=40, species: SpeciesId::GOLBAT),
        wild!(levels: 38..=38, species: SpeciesId::GOLBAT),
        wild!(levels: 40..=40, species: SpeciesId::GOLBAT),
    ],
};

const GMETEORFALLS_1F_2R_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 30..=35, species: SpeciesId::GOLBAT),
        wild!(levels: 30..=35, species: SpeciesId::GOLBAT),
        wild!(levels: 25..=35, species: SpeciesId::SOLROCK),
        wild!(levels: 15..=25, species: SpeciesId::SOLROCK),
        wild!(levels: 5..=15, species: SpeciesId::SOLROCK),
    ],
};

const GMETEORFALLS_1F_2R_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::GOLDEEN),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::GOLDEEN),
        wild!(levels: 10..=30, species: SpeciesId::BARBOACH),
        wild!(levels: 25..=30, species: SpeciesId::BARBOACH),
        wild!(levels: 30..=35, species: SpeciesId::BARBOACH),
        wild!(levels: 30..=35, species: SpeciesId::WHISCASH),
        wild!(levels: 35..=40, species: SpeciesId::WHISCASH),
        wild!(levels: 40..=45, species: SpeciesId::WHISCASH),
    ],
};

const GMETEORFALLS_B1F_1R_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 33..=33, species: SpeciesId::GOLBAT),
        wild!(levels: 35..=35, species: SpeciesId::GOLBAT),
        wild!(levels: 33..=33, species: SpeciesId::GOLBAT),
        wild!(levels: 35..=35, species: SpeciesId::SOLROCK),
        wild!(levels: 33..=33, species: SpeciesId::SOLROCK),
        wild!(levels: 37..=37, species: SpeciesId::SOLROCK),
        wild!(levels: 35..=35, species: SpeciesId::GOLBAT),
        wild!(levels: 39..=39, species: SpeciesId::SOLROCK),
        wild!(levels: 38..=38, species: SpeciesId::GOLBAT),
        wild!(levels: 40..=40, species: SpeciesId::GOLBAT),
        wild!(levels: 38..=38, species: SpeciesId::GOLBAT),
        wild!(levels: 40..=40, species: SpeciesId::GOLBAT),
    ],
};

const GMETEORFALLS_B1F_1R_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 30..=35, species: SpeciesId::GOLBAT),
        wild!(levels: 30..=35, species: SpeciesId::GOLBAT),
        wild!(levels: 25..=35, species: SpeciesId::SOLROCK),
        wild!(levels: 15..=25, species: SpeciesId::SOLROCK),
        wild!(levels: 5..=15, species: SpeciesId::SOLROCK),
    ],
};

const GMETEORFALLS_B1F_1R_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 30,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::GOLDEEN),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::GOLDEEN),
        wild!(levels: 10..=30, species: SpeciesId::BARBOACH),
        wild!(levels: 25..=30, species: SpeciesId::BARBOACH),
        wild!(levels: 30..=35, species: SpeciesId::BARBOACH),
        wild!(levels: 30..=35, species: SpeciesId::WHISCASH),
        wild!(levels: 35..=40, species: SpeciesId::WHISCASH),
        wild!(levels: 40..=45, species: SpeciesId::WHISCASH),
    ],
};

const GSHOALCAVE_LOWTIDESTAIRSROOM_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 26..=26, species: SpeciesId::ZUBAT),
        wild!(levels: 26..=26, species: SpeciesId::SPHEAL),
        wild!(levels: 28..=28, species: SpeciesId::ZUBAT),
        wild!(levels: 28..=28, species: SpeciesId::SPHEAL),
        wild!(levels: 30..=30, species: SpeciesId::ZUBAT),
        wild!(levels: 30..=30, species: SpeciesId::SPHEAL),
        wild!(levels: 32..=32, species: SpeciesId::ZUBAT),
        wild!(levels: 32..=32, species: SpeciesId::SPHEAL),
        wild!(levels: 32..=32, species: SpeciesId::GOLBAT),
        wild!(levels: 32..=32, species: SpeciesId::SPHEAL),
        wild!(levels: 32..=32, species: SpeciesId::GOLBAT),
        wild!(levels: 32..=32, species: SpeciesId::SPHEAL),
    ],
};

const GSHOALCAVE_LOWTIDELOWERROOM_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 26..=26, species: SpeciesId::ZUBAT),
        wild!(levels: 26..=26, species: SpeciesId::SPHEAL),
        wild!(levels: 28..=28, species: SpeciesId::ZUBAT),
        wild!(levels: 28..=28, species: SpeciesId::SPHEAL),
        wild!(levels: 30..=30, species: SpeciesId::ZUBAT),
        wild!(levels: 30..=30, species: SpeciesId::SPHEAL),
        wild!(levels: 32..=32, species: SpeciesId::ZUBAT),
        wild!(levels: 32..=32, species: SpeciesId::SPHEAL),
        wild!(levels: 32..=32, species: SpeciesId::GOLBAT),
        wild!(levels: 32..=32, species: SpeciesId::SPHEAL),
        wild!(levels: 32..=32, species: SpeciesId::GOLBAT),
        wild!(levels: 32..=32, species: SpeciesId::SPHEAL),
    ],
};

const GSHOALCAVE_LOWTIDEINNERROOM_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 26..=26, species: SpeciesId::ZUBAT),
        wild!(levels: 26..=26, species: SpeciesId::SPHEAL),
        wild!(levels: 28..=28, species: SpeciesId::ZUBAT),
        wild!(levels: 28..=28, species: SpeciesId::SPHEAL),
        wild!(levels: 30..=30, species: SpeciesId::ZUBAT),
        wild!(levels: 30..=30, species: SpeciesId::SPHEAL),
        wild!(levels: 32..=32, species: SpeciesId::ZUBAT),
        wild!(levels: 32..=32, species: SpeciesId::SPHEAL),
        wild!(levels: 32..=32, species: SpeciesId::GOLBAT),
        wild!(levels: 32..=32, species: SpeciesId::SPHEAL),
        wild!(levels: 32..=32, species: SpeciesId::GOLBAT),
        wild!(levels: 32..=32, species: SpeciesId::SPHEAL),
    ],
};

const GSHOALCAVE_LOWTIDEINNERROOM_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 5..=35, species: SpeciesId::ZUBAT),
        wild!(levels: 25..=30, species: SpeciesId::SPHEAL),
        wild!(levels: 25..=30, species: SpeciesId::SPHEAL),
        wild!(levels: 25..=35, species: SpeciesId::SPHEAL),
    ],
};

const GSHOALCAVE_LOWTIDEINNERROOM_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WAILMER),
        wild!(levels: 25..=30, species: SpeciesId::WAILMER),
        wild!(levels: 30..=35, species: SpeciesId::WAILMER),
        wild!(levels: 20..=25, species: SpeciesId::WAILMER),
        wild!(levels: 35..=40, species: SpeciesId::WAILMER),
        wild!(levels: 40..=45, species: SpeciesId::WAILMER),
    ],
};

const GSHOALCAVE_LOWTIDEENTRANCEROOM_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 26..=26, species: SpeciesId::ZUBAT),
        wild!(levels: 26..=26, species: SpeciesId::SPHEAL),
        wild!(levels: 28..=28, species: SpeciesId::ZUBAT),
        wild!(levels: 28..=28, species: SpeciesId::SPHEAL),
        wild!(levels: 30..=30, species: SpeciesId::ZUBAT),
        wild!(levels: 30..=30, species: SpeciesId::SPHEAL),
        wild!(levels: 32..=32, species: SpeciesId::ZUBAT),
        wild!(levels: 32..=32, species: SpeciesId::SPHEAL),
        wild!(levels: 32..=32, species: SpeciesId::GOLBAT),
        wild!(levels: 32..=32, species: SpeciesId::SPHEAL),
        wild!(levels: 32..=32, species: SpeciesId::GOLBAT),
        wild!(levels: 32..=32, species: SpeciesId::SPHEAL),
    ],
};

const GSHOALCAVE_LOWTIDEENTRANCEROOM_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 5..=35, species: SpeciesId::ZUBAT),
        wild!(levels: 25..=30, species: SpeciesId::SPHEAL),
        wild!(levels: 25..=30, species: SpeciesId::SPHEAL),
        wild!(levels: 25..=35, species: SpeciesId::SPHEAL),
    ],
};

const GSHOALCAVE_LOWTIDEENTRANCEROOM_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WAILMER),
        wild!(levels: 25..=30, species: SpeciesId::WAILMER),
        wild!(levels: 30..=35, species: SpeciesId::WAILMER),
        wild!(levels: 20..=25, species: SpeciesId::WAILMER),
        wild!(levels: 35..=40, species: SpeciesId::WAILMER),
        wild!(levels: 40..=45, species: SpeciesId::WAILMER),
    ],
};

const GLILYCOVECITY_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WINGULL),
        wild!(levels: 15..=25, species: SpeciesId::WINGULL),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
    ],
};

const GLILYCOVECITY_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WAILMER),
        wild!(levels: 25..=30, species: SpeciesId::WAILMER),
        wild!(levels: 30..=35, species: SpeciesId::WAILMER),
        wild!(levels: 25..=30, species: SpeciesId::STARYU),
        wild!(levels: 35..=40, species: SpeciesId::WAILMER),
        wild!(levels: 40..=45, species: SpeciesId::WAILMER),
    ],
};

const GDEWFORDTOWN_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WINGULL),
        wild!(levels: 15..=25, species: SpeciesId::WINGULL),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
    ],
};

const GDEWFORDTOWN_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WAILMER),
        wild!(levels: 25..=30, species: SpeciesId::WAILMER),
        wild!(levels: 30..=35, species: SpeciesId::WAILMER),
        wild!(levels: 20..=25, species: SpeciesId::WAILMER),
        wild!(levels: 35..=40, species: SpeciesId::WAILMER),
        wild!(levels: 40..=45, species: SpeciesId::WAILMER),
    ],
};

const GSLATEPORTCITY_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WINGULL),
        wild!(levels: 15..=25, species: SpeciesId::WINGULL),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
    ],
};

const GSLATEPORTCITY_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WAILMER),
        wild!(levels: 25..=30, species: SpeciesId::WAILMER),
        wild!(levels: 30..=35, species: SpeciesId::WAILMER),
        wild!(levels: 20..=25, species: SpeciesId::WAILMER),
        wild!(levels: 35..=40, species: SpeciesId::WAILMER),
        wild!(levels: 40..=45, species: SpeciesId::WAILMER),
    ],
};

const GMOSSDEEPCITY_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WINGULL),
        wild!(levels: 15..=25, species: SpeciesId::WINGULL),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
    ],
};

const GMOSSDEEPCITY_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WAILMER),
        wild!(levels: 30..=35, species: SpeciesId::SHARPEDO),
        wild!(levels: 30..=35, species: SpeciesId::WAILMER),
        wild!(levels: 25..=30, species: SpeciesId::WAILMER),
        wild!(levels: 35..=40, species: SpeciesId::WAILMER),
        wild!(levels: 40..=45, species: SpeciesId::WAILMER),
    ],
};

const GPACIFIDLOGTOWN_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WINGULL),
        wild!(levels: 15..=25, species: SpeciesId::WINGULL),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
    ],
};

const GPACIFIDLOGTOWN_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WAILMER),
        wild!(levels: 30..=35, species: SpeciesId::SHARPEDO),
        wild!(levels: 30..=35, species: SpeciesId::WAILMER),
        wild!(levels: 25..=30, species: SpeciesId::WAILMER),
        wild!(levels: 35..=40, species: SpeciesId::WAILMER),
        wild!(levels: 40..=45, species: SpeciesId::WAILMER),
    ],
};

const GEVERGRANDECITY_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::WINGULL),
        wild!(levels: 15..=25, species: SpeciesId::WINGULL),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
        wild!(levels: 25..=30, species: SpeciesId::PELIPPER),
    ],
};

const GEVERGRANDECITY_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::LUVDISC),
        wild!(levels: 10..=30, species: SpeciesId::WAILMER),
        wild!(levels: 30..=35, species: SpeciesId::LUVDISC),
        wild!(levels: 30..=35, species: SpeciesId::WAILMER),
        wild!(levels: 30..=35, species: SpeciesId::CORSOLA),
        wild!(levels: 35..=40, species: SpeciesId::WAILMER),
        wild!(levels: 40..=45, species: SpeciesId::WAILMER),
    ],
};

const GPETALBURGCITY_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 1,
    mons: [
        wild!(levels: 20..=30, species: SpeciesId::MARILL),
        wild!(levels: 10..=20, species: SpeciesId::MARILL),
        wild!(levels: 30..=35, species: SpeciesId::MARILL),
        wild!(levels: 5..=10, species: SpeciesId::MARILL),
        wild!(levels: 5..=10, species: SpeciesId::MARILL),
    ],
};

const GPETALBURGCITY_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::GOLDEEN),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::GOLDEEN),
        wild!(levels: 10..=30, species: SpeciesId::CORPHISH),
        wild!(levels: 25..=30, species: SpeciesId::CORPHISH),
        wild!(levels: 30..=35, species: SpeciesId::CORPHISH),
        wild!(levels: 20..=25, species: SpeciesId::CORPHISH),
        wild!(levels: 35..=40, species: SpeciesId::CORPHISH),
        wild!(levels: 40..=45, species: SpeciesId::CORPHISH),
    ],
};

const GUNDERWATER_ROUTE124_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 4,
    mons: [
        wild!(levels: 20..=30, species: SpeciesId::CLAMPERL),
        wild!(levels: 20..=30, species: SpeciesId::CHINCHOU),
        wild!(levels: 30..=35, species: SpeciesId::CLAMPERL),
        wild!(levels: 30..=35, species: SpeciesId::RELICANTH),
        wild!(levels: 30..=35, species: SpeciesId::RELICANTH),
    ],
};

const GSHOALCAVE_LOWTIDEICEROOM_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 26..=26, species: SpeciesId::ZUBAT),
        wild!(levels: 26..=26, species: SpeciesId::SPHEAL),
        wild!(levels: 28..=28, species: SpeciesId::ZUBAT),
        wild!(levels: 28..=28, species: SpeciesId::SPHEAL),
        wild!(levels: 30..=30, species: SpeciesId::ZUBAT),
        wild!(levels: 30..=30, species: SpeciesId::SPHEAL),
        wild!(levels: 26..=26, species: SpeciesId::SNORUNT),
        wild!(levels: 32..=32, species: SpeciesId::SPHEAL),
        wild!(levels: 30..=30, species: SpeciesId::GOLBAT),
        wild!(levels: 28..=28, species: SpeciesId::SNORUNT),
        wild!(levels: 32..=32, species: SpeciesId::GOLBAT),
        wild!(levels: 30..=30, species: SpeciesId::SNORUNT),
    ],
};

const GSKYPILLAR_1F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 33..=33, species: SpeciesId::SABLEYE),
        wild!(levels: 34..=34, species: SpeciesId::GOLBAT),
        wild!(levels: 35..=35, species: SpeciesId::GOLBAT),
        wild!(levels: 34..=34, species: SpeciesId::SABLEYE),
        wild!(levels: 36..=36, species: SpeciesId::CLAYDOL),
        wild!(levels: 37..=37, species: SpeciesId::BANETTE),
        wild!(levels: 38..=38, species: SpeciesId::BANETTE),
        wild!(levels: 36..=36, species: SpeciesId::CLAYDOL),
        wild!(levels: 37..=37, species: SpeciesId::CLAYDOL),
        wild!(levels: 38..=38, species: SpeciesId::CLAYDOL),
        wild!(levels: 37..=37, species: SpeciesId::CLAYDOL),
        wild!(levels: 38..=38, species: SpeciesId::CLAYDOL),
    ],
};

const GSOOTOPOLISCITY_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 1,
    mons: [
        wild!(levels: 5..=35, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 15..=25, species: SpeciesId::MAGIKARP),
        wild!(levels: 25..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 25..=30, species: SpeciesId::MAGIKARP),
    ],
};

const GSOOTOPOLISCITY_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 5..=10, species: SpeciesId::MAGIKARP),
        wild!(levels: 5..=10, species: SpeciesId::TENTACOOL),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 10..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 30..=35, species: SpeciesId::MAGIKARP),
        wild!(levels: 30..=35, species: SpeciesId::MAGIKARP),
        wild!(levels: 35..=40, species: SpeciesId::GYARADOS),
        wild!(levels: 35..=45, species: SpeciesId::GYARADOS),
        wild!(levels: 5..=45, species: SpeciesId::GYARADOS),
    ],
};

const GSKYPILLAR_3F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 33..=33, species: SpeciesId::SABLEYE),
        wild!(levels: 34..=34, species: SpeciesId::GOLBAT),
        wild!(levels: 35..=35, species: SpeciesId::GOLBAT),
        wild!(levels: 34..=34, species: SpeciesId::SABLEYE),
        wild!(levels: 36..=36, species: SpeciesId::CLAYDOL),
        wild!(levels: 37..=37, species: SpeciesId::BANETTE),
        wild!(levels: 38..=38, species: SpeciesId::BANETTE),
        wild!(levels: 36..=36, species: SpeciesId::CLAYDOL),
        wild!(levels: 37..=37, species: SpeciesId::CLAYDOL),
        wild!(levels: 38..=38, species: SpeciesId::CLAYDOL),
        wild!(levels: 37..=37, species: SpeciesId::CLAYDOL),
        wild!(levels: 38..=38, species: SpeciesId::CLAYDOL),
    ],
};

const GSKYPILLAR_5F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 33..=33, species: SpeciesId::SABLEYE),
        wild!(levels: 34..=34, species: SpeciesId::GOLBAT),
        wild!(levels: 35..=35, species: SpeciesId::GOLBAT),
        wild!(levels: 34..=34, species: SpeciesId::SABLEYE),
        wild!(levels: 36..=36, species: SpeciesId::CLAYDOL),
        wild!(levels: 37..=37, species: SpeciesId::BANETTE),
        wild!(levels: 38..=38, species: SpeciesId::BANETTE),
        wild!(levels: 36..=36, species: SpeciesId::CLAYDOL),
        wild!(levels: 37..=37, species: SpeciesId::CLAYDOL),
        wild!(levels: 38..=38, species: SpeciesId::ALTARIA),
        wild!(levels: 39..=39, species: SpeciesId::ALTARIA),
        wild!(levels: 39..=39, species: SpeciesId::ALTARIA),
    ],
};

const GSAFARIZONE_SOUTHEAST_LAND: LandEncounters = LandEncounters {
    encounter_rate: 25,
    mons: [
        wild!(levels: 33..=33, species: SpeciesId::SUNKERN),
        wild!(levels: 34..=34, species: SpeciesId::MAREEP),
        wild!(levels: 35..=35, species: SpeciesId::SUNKERN),
        wild!(levels: 36..=36, species: SpeciesId::MAREEP),
        wild!(levels: 34..=34, species: SpeciesId::AIPOM),
        wild!(levels: 33..=33, species: SpeciesId::SPINARAK),
        wild!(levels: 35..=35, species: SpeciesId::HOOTHOOT),
        wild!(levels: 34..=34, species: SpeciesId::SNUBBULL),
        wild!(levels: 36..=36, species: SpeciesId::STANTLER),
        wild!(levels: 37..=37, species: SpeciesId::GLIGAR),
        wild!(levels: 39..=39, species: SpeciesId::STANTLER),
        wild!(levels: 40..=40, species: SpeciesId::GLIGAR),
    ],
};

const GSAFARIZONE_SOUTHEAST_WATER: WaterEncounters = WaterEncounters {
    encounter_rate: 9,
    mons: [
        wild!(levels: 25..=30, species: SpeciesId::WOOPER),
        wild!(levels: 25..=30, species: SpeciesId::MARILL),
        wild!(levels: 25..=30, species: SpeciesId::MARILL),
        wild!(levels: 30..=35, species: SpeciesId::MARILL),
        wild!(levels: 35..=40, species: SpeciesId::QUAGSIRE),
    ],
};

const GSAFARIZONE_SOUTHEAST_FISHING: FishingEncounters = FishingEncounters {
    encounter_rate: 35,
    mons: [
        wild!(levels: 25..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 25..=30, species: SpeciesId::GOLDEEN),
        wild!(levels: 25..=30, species: SpeciesId::MAGIKARP),
        wild!(levels: 25..=30, species: SpeciesId::GOLDEEN),
        wild!(levels: 30..=35, species: SpeciesId::REMORAID),
        wild!(levels: 25..=30, species: SpeciesId::GOLDEEN),
        wild!(levels: 25..=30, species: SpeciesId::REMORAID),
        wild!(levels: 30..=35, species: SpeciesId::REMORAID),
        wild!(levels: 30..=35, species: SpeciesId::REMORAID),
        wild!(levels: 35..=40, species: SpeciesId::OCTILLERY),
    ],
};

const GSAFARIZONE_NORTHEAST_LAND: LandEncounters = LandEncounters {
    encounter_rate: 25,
    mons: [
        wild!(levels: 33..=33, species: SpeciesId::AIPOM),
        wild!(levels: 34..=34, species: SpeciesId::TEDDIURSA),
        wild!(levels: 35..=35, species: SpeciesId::AIPOM),
        wild!(levels: 36..=36, species: SpeciesId::TEDDIURSA),
        wild!(levels: 34..=34, species: SpeciesId::SUNKERN),
        wild!(levels: 33..=33, species: SpeciesId::LEDYBA),
        wild!(levels: 35..=35, species: SpeciesId::HOOTHOOT),
        wild!(levels: 34..=34, species: SpeciesId::PINECO),
        wild!(levels: 36..=36, species: SpeciesId::HOUNDOUR),
        wild!(levels: 37..=37, species: SpeciesId::MILTANK),
        wild!(levels: 39..=39, species: SpeciesId::HOUNDOUR),
        wild!(levels: 40..=40, species: SpeciesId::MILTANK),
    ],
};

const GSAFARIZONE_NORTHEAST_ROCKSMASH: RockSmashEncounters = RockSmashEncounters {
    encounter_rate: 25,
    mons: [
        wild!(levels: 25..=30, species: SpeciesId::SHUCKLE),
        wild!(levels: 20..=25, species: SpeciesId::SHUCKLE),
        wild!(levels: 30..=35, species: SpeciesId::SHUCKLE),
        wild!(levels: 30..=35, species: SpeciesId::SHUCKLE),
        wild!(levels: 35..=40, species: SpeciesId::SHUCKLE),
    ],
};

const GMAGMAHIDEOUT_1F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 27..=27, species: SpeciesId::GEODUDE),
        wild!(levels: 28..=28, species: SpeciesId::TORKOAL),
        wild!(levels: 28..=28, species: SpeciesId::GEODUDE),
        wild!(levels: 30..=30, species: SpeciesId::TORKOAL),
        wild!(levels: 29..=29, species: SpeciesId::GEODUDE),
        wild!(levels: 30..=30, species: SpeciesId::GEODUDE),
        wild!(levels: 30..=30, species: SpeciesId::GEODUDE),
        wild!(levels: 30..=30, species: SpeciesId::GRAVELER),
        wild!(levels: 30..=30, species: SpeciesId::GRAVELER),
        wild!(levels: 31..=31, species: SpeciesId::GRAVELER),
        wild!(levels: 32..=32, species: SpeciesId::GRAVELER),
        wild!(levels: 33..=33, species: SpeciesId::GRAVELER),
    ],
};

const GMAGMAHIDEOUT_2F_1R_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 27..=27, species: SpeciesId::GEODUDE),
        wild!(levels: 28..=28, species: SpeciesId::TORKOAL),
        wild!(levels: 28..=28, species: SpeciesId::GEODUDE),
        wild!(levels: 30..=30, species: SpeciesId::TORKOAL),
        wild!(levels: 29..=29, species: SpeciesId::GEODUDE),
        wild!(levels: 30..=30, species: SpeciesId::GEODUDE),
        wild!(levels: 30..=30, species: SpeciesId::GEODUDE),
        wild!(levels: 30..=30, species: SpeciesId::GRAVELER),
        wild!(levels: 30..=30, species: SpeciesId::GRAVELER),
        wild!(levels: 31..=31, species: SpeciesId::GRAVELER),
        wild!(levels: 32..=32, species: SpeciesId::GRAVELER),
        wild!(levels: 33..=33, species: SpeciesId::GRAVELER),
    ],
};

const GMAGMAHIDEOUT_2F_2R_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 27..=27, species: SpeciesId::GEODUDE),
        wild!(levels: 28..=28, species: SpeciesId::TORKOAL),
        wild!(levels: 28..=28, species: SpeciesId::GEODUDE),
        wild!(levels: 30..=30, species: SpeciesId::TORKOAL),
        wild!(levels: 29..=29, species: SpeciesId::GEODUDE),
        wild!(levels: 30..=30, species: SpeciesId::GEODUDE),
        wild!(levels: 30..=30, species: SpeciesId::GEODUDE),
        wild!(levels: 30..=30, species: SpeciesId::GRAVELER),
        wild!(levels: 30..=30, species: SpeciesId::GRAVELER),
        wild!(levels: 31..=31, species: SpeciesId::GRAVELER),
        wild!(levels: 32..=32, species: SpeciesId::GRAVELER),
        wild!(levels: 33..=33, species: SpeciesId::GRAVELER),
    ],
};

const GMAGMAHIDEOUT_3F_1R_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 27..=27, species: SpeciesId::GEODUDE),
        wild!(levels: 28..=28, species: SpeciesId::TORKOAL),
        wild!(levels: 28..=28, species: SpeciesId::GEODUDE),
        wild!(levels: 30..=30, species: SpeciesId::TORKOAL),
        wild!(levels: 29..=29, species: SpeciesId::GEODUDE),
        wild!(levels: 30..=30, species: SpeciesId::GEODUDE),
        wild!(levels: 30..=30, species: SpeciesId::GEODUDE),
        wild!(levels: 30..=30, species: SpeciesId::GRAVELER),
        wild!(levels: 30..=30, species: SpeciesId::GRAVELER),
        wild!(levels: 31..=31, species: SpeciesId::GRAVELER),
        wild!(levels: 32..=32, species: SpeciesId::GRAVELER),
        wild!(levels: 33..=33, species: SpeciesId::GRAVELER),
    ],
};

const GMAGMAHIDEOUT_3F_2R_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 27..=27, species: SpeciesId::GEODUDE),
        wild!(levels: 28..=28, species: SpeciesId::TORKOAL),
        wild!(levels: 28..=28, species: SpeciesId::GEODUDE),
        wild!(levels: 30..=30, species: SpeciesId::TORKOAL),
        wild!(levels: 29..=29, species: SpeciesId::GEODUDE),
        wild!(levels: 30..=30, species: SpeciesId::GEODUDE),
        wild!(levels: 30..=30, species: SpeciesId::GEODUDE),
        wild!(levels: 30..=30, species: SpeciesId::GRAVELER),
        wild!(levels: 30..=30, species: SpeciesId::GRAVELER),
        wild!(levels: 31..=31, species: SpeciesId::GRAVELER),
        wild!(levels: 32..=32, species: SpeciesId::GRAVELER),
        wild!(levels: 33..=33, species: SpeciesId::GRAVELER),
    ],
};

const GMAGMAHIDEOUT_4F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 27..=27, species: SpeciesId::GEODUDE),
        wild!(levels: 28..=28, species: SpeciesId::TORKOAL),
        wild!(levels: 28..=28, species: SpeciesId::GEODUDE),
        wild!(levels: 30..=30, species: SpeciesId::TORKOAL),
        wild!(levels: 29..=29, species: SpeciesId::GEODUDE),
        wild!(levels: 30..=30, species: SpeciesId::GEODUDE),
        wild!(levels: 30..=30, species: SpeciesId::GEODUDE),
        wild!(levels: 30..=30, species: SpeciesId::GRAVELER),
        wild!(levels: 30..=30, species: SpeciesId::GRAVELER),
        wild!(levels: 31..=31, species: SpeciesId::GRAVELER),
        wild!(levels: 32..=32, species: SpeciesId::GRAVELER),
        wild!(levels: 33..=33, species: SpeciesId::GRAVELER),
    ],
};

const GMAGMAHIDEOUT_3F_3R_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 27..=27, species: SpeciesId::GEODUDE),
        wild!(levels: 28..=28, species: SpeciesId::TORKOAL),
        wild!(levels: 28..=28, species: SpeciesId::GEODUDE),
        wild!(levels: 30..=30, species: SpeciesId::TORKOAL),
        wild!(levels: 29..=29, species: SpeciesId::GEODUDE),
        wild!(levels: 30..=30, species: SpeciesId::GEODUDE),
        wild!(levels: 30..=30, species: SpeciesId::GEODUDE),
        wild!(levels: 30..=30, species: SpeciesId::GRAVELER),
        wild!(levels: 30..=30, species: SpeciesId::GRAVELER),
        wild!(levels: 31..=31, species: SpeciesId::GRAVELER),
        wild!(levels: 32..=32, species: SpeciesId::GRAVELER),
        wild!(levels: 33..=33, species: SpeciesId::GRAVELER),
    ],
};

const GMAGMAHIDEOUT_2F_3R_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 27..=27, species: SpeciesId::GEODUDE),
        wild!(levels: 28..=28, species: SpeciesId::TORKOAL),
        wild!(levels: 28..=28, species: SpeciesId::GEODUDE),
        wild!(levels: 30..=30, species: SpeciesId::TORKOAL),
        wild!(levels: 29..=29, species: SpeciesId::GEODUDE),
        wild!(levels: 30..=30, species: SpeciesId::GEODUDE),
        wild!(levels: 30..=30, species: SpeciesId::GEODUDE),
        wild!(levels: 30..=30, species: SpeciesId::GRAVELER),
        wild!(levels: 30..=30, species: SpeciesId::GRAVELER),
        wild!(levels: 31..=31, species: SpeciesId::GRAVELER),
        wild!(levels: 32..=32, species: SpeciesId::GRAVELER),
        wild!(levels: 33..=33, species: SpeciesId::GRAVELER),
    ],
};

const GMIRAGETOWER_1F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 21..=21, species: SpeciesId::SANDSHREW),
        wild!(levels: 21..=21, species: SpeciesId::TRAPINCH),
        wild!(levels: 20..=20, species: SpeciesId::SANDSHREW),
        wild!(levels: 20..=20, species: SpeciesId::TRAPINCH),
        wild!(levels: 20..=20, species: SpeciesId::SANDSHREW),
        wild!(levels: 20..=20, species: SpeciesId::TRAPINCH),
        wild!(levels: 22..=22, species: SpeciesId::SANDSHREW),
        wild!(levels: 22..=22, species: SpeciesId::TRAPINCH),
        wild!(levels: 23..=23, species: SpeciesId::SANDSHREW),
        wild!(levels: 23..=23, species: SpeciesId::TRAPINCH),
        wild!(levels: 24..=24, species: SpeciesId::SANDSHREW),
        wild!(levels: 24..=24, species: SpeciesId::TRAPINCH),
    ],
};

const GMIRAGETOWER_2F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 21..=21, species: SpeciesId::SANDSHREW),
        wild!(levels: 21..=21, species: SpeciesId::TRAPINCH),
        wild!(levels: 20..=20, species: SpeciesId::SANDSHREW),
        wild!(levels: 20..=20, species: SpeciesId::TRAPINCH),
        wild!(levels: 20..=20, species: SpeciesId::SANDSHREW),
        wild!(levels: 20..=20, species: SpeciesId::TRAPINCH),
        wild!(levels: 22..=22, species: SpeciesId::SANDSHREW),
        wild!(levels: 22..=22, species: SpeciesId::TRAPINCH),
        wild!(levels: 23..=23, species: SpeciesId::SANDSHREW),
        wild!(levels: 23..=23, species: SpeciesId::TRAPINCH),
        wild!(levels: 24..=24, species: SpeciesId::SANDSHREW),
        wild!(levels: 24..=24, species: SpeciesId::TRAPINCH),
    ],
};

const GMIRAGETOWER_3F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 21..=21, species: SpeciesId::SANDSHREW),
        wild!(levels: 21..=21, species: SpeciesId::TRAPINCH),
        wild!(levels: 20..=20, species: SpeciesId::SANDSHREW),
        wild!(levels: 20..=20, species: SpeciesId::TRAPINCH),
        wild!(levels: 20..=20, species: SpeciesId::SANDSHREW),
        wild!(levels: 20..=20, species: SpeciesId::TRAPINCH),
        wild!(levels: 22..=22, species: SpeciesId::SANDSHREW),
        wild!(levels: 22..=22, species: SpeciesId::TRAPINCH),
        wild!(levels: 23..=23, species: SpeciesId::SANDSHREW),
        wild!(levels: 23..=23, species: SpeciesId::TRAPINCH),
        wild!(levels: 24..=24, species: SpeciesId::SANDSHREW),
        wild!(levels: 24..=24, species: SpeciesId::TRAPINCH),
    ],
};

const GMIRAGETOWER_4F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 21..=21, species: SpeciesId::SANDSHREW),
        wild!(levels: 21..=21, species: SpeciesId::TRAPINCH),
        wild!(levels: 20..=20, species: SpeciesId::SANDSHREW),
        wild!(levels: 20..=20, species: SpeciesId::TRAPINCH),
        wild!(levels: 20..=20, species: SpeciesId::SANDSHREW),
        wild!(levels: 20..=20, species: SpeciesId::TRAPINCH),
        wild!(levels: 22..=22, species: SpeciesId::SANDSHREW),
        wild!(levels: 22..=22, species: SpeciesId::TRAPINCH),
        wild!(levels: 23..=23, species: SpeciesId::SANDSHREW),
        wild!(levels: 23..=23, species: SpeciesId::TRAPINCH),
        wild!(levels: 24..=24, species: SpeciesId::SANDSHREW),
        wild!(levels: 24..=24, species: SpeciesId::TRAPINCH),
    ],
};

const GDESERTUNDERPASS_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 38..=38, species: SpeciesId::DITTO),
        wild!(levels: 35..=35, species: SpeciesId::WHISMUR),
        wild!(levels: 40..=40, species: SpeciesId::DITTO),
        wild!(levels: 40..=40, species: SpeciesId::LOUDRED),
        wild!(levels: 41..=41, species: SpeciesId::DITTO),
        wild!(levels: 36..=36, species: SpeciesId::WHISMUR),
        wild!(levels: 38..=38, species: SpeciesId::LOUDRED),
        wild!(levels: 42..=42, species: SpeciesId::DITTO),
        wild!(levels: 38..=38, species: SpeciesId::WHISMUR),
        wild!(levels: 43..=43, species: SpeciesId::DITTO),
        wild!(levels: 44..=44, species: SpeciesId::LOUDRED),
        wild!(levels: 45..=45, species: SpeciesId::DITTO),
    ],
};

const GARTISANCAVE_B1F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 40..=40, species: SpeciesId::SMEARGLE),
        wild!(levels: 41..=41, species: SpeciesId::SMEARGLE),
        wild!(levels: 42..=42, species: SpeciesId::SMEARGLE),
        wild!(levels: 43..=43, species: SpeciesId::SMEARGLE),
        wild!(levels: 44..=44, species: SpeciesId::SMEARGLE),
        wild!(levels: 45..=45, species: SpeciesId::SMEARGLE),
        wild!(levels: 46..=46, species: SpeciesId::SMEARGLE),
        wild!(levels: 47..=47, species: SpeciesId::SMEARGLE),
        wild!(levels: 48..=48, species: SpeciesId::SMEARGLE),
        wild!(levels: 49..=49, species: SpeciesId::SMEARGLE),
        wild!(levels: 50..=50, species: SpeciesId::SMEARGLE),
        wild!(levels: 50..=50, species: SpeciesId::SMEARGLE),
    ],
};

const GARTISANCAVE_1F_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 40..=40, species: SpeciesId::SMEARGLE),
        wild!(levels: 41..=41, species: SpeciesId::SMEARGLE),
        wild!(levels: 42..=42, species: SpeciesId::SMEARGLE),
        wild!(levels: 43..=43, species: SpeciesId::SMEARGLE),
        wild!(levels: 44..=44, species: SpeciesId::SMEARGLE),
        wild!(levels: 45..=45, species: SpeciesId::SMEARGLE),
        wild!(levels: 46..=46, species: SpeciesId::SMEARGLE),
        wild!(levels: 47..=47, species: SpeciesId::SMEARGLE),
        wild!(levels: 48..=48, species: SpeciesId::SMEARGLE),
        wild!(levels: 49..=49, species: SpeciesId::SMEARGLE),
        wild!(levels: 50..=50, species: SpeciesId::SMEARGLE),
        wild!(levels: 50..=50, species: SpeciesId::SMEARGLE),
    ],
};

const GALTERINGCAVE1_LAND: LandEncounters = LandEncounters {
    encounter_rate: 7,
    mons: [
        wild!(levels: 10..=10, species: SpeciesId::ZUBAT),
        wild!(levels: 12..=12, species: SpeciesId::ZUBAT),
        wild!(levels: 8..=8, species: SpeciesId::ZUBAT),
        wild!(levels: 14..=14, species: SpeciesId::ZUBAT),
        wild!(levels: 10..=10, species: SpeciesId::ZUBAT),
        wild!(levels: 12..=12, species: SpeciesId::ZUBAT),
        wild!(levels: 16..=16, species: SpeciesId::ZUBAT),
        wild!(levels: 6..=6, species: SpeciesId::ZUBAT),
        wild!(levels: 8..=8, species: SpeciesId::ZUBAT),
        wild!(levels: 14..=14, species: SpeciesId::ZUBAT),
        wild!(levels: 8..=8, species: SpeciesId::ZUBAT),
        wild!(levels: 14..=14, species: SpeciesId::ZUBAT),
    ],
};

const GALTERINGCAVE2_LAND: LandEncounters = LandEncounters {
    encounter_rate: 7,
    mons: [
        wild!(levels: 7..=7, species: SpeciesId::MAREEP),
        wild!(levels: 9..=9, species: SpeciesId::MAREEP),
        wild!(levels: 5..=5, species: SpeciesId::MAREEP),
        wild!(levels: 11..=11, species: SpeciesId::MAREEP),
        wild!(levels: 7..=7, species: SpeciesId::MAREEP),
        wild!(levels: 9..=9, species: SpeciesId::MAREEP),
        wild!(levels: 13..=13, species: SpeciesId::MAREEP),
        wild!(levels: 3..=3, species: SpeciesId::MAREEP),
        wild!(levels: 5..=5, species: SpeciesId::MAREEP),
        wild!(levels: 11..=11, species: SpeciesId::MAREEP),
        wild!(levels: 5..=5, species: SpeciesId::MAREEP),
        wild!(levels: 11..=11, species: SpeciesId::MAREEP),
    ],
};

const GALTERINGCAVE3_LAND: LandEncounters = LandEncounters {
    encounter_rate: 7,
    mons: [
        wild!(levels: 23..=23, species: SpeciesId::PINECO),
        wild!(levels: 25..=25, species: SpeciesId::PINECO),
        wild!(levels: 22..=22, species: SpeciesId::PINECO),
        wild!(levels: 27..=27, species: SpeciesId::PINECO),
        wild!(levels: 23..=23, species: SpeciesId::PINECO),
        wild!(levels: 25..=25, species: SpeciesId::PINECO),
        wild!(levels: 29..=29, species: SpeciesId::PINECO),
        wild!(levels: 19..=19, species: SpeciesId::PINECO),
        wild!(levels: 21..=21, species: SpeciesId::PINECO),
        wild!(levels: 27..=27, species: SpeciesId::PINECO),
        wild!(levels: 21..=21, species: SpeciesId::PINECO),
        wild!(levels: 27..=27, species: SpeciesId::PINECO),
    ],
};

const GALTERINGCAVE4_LAND: LandEncounters = LandEncounters {
    encounter_rate: 7,
    mons: [
        wild!(levels: 16..=16, species: SpeciesId::HOUNDOUR),
        wild!(levels: 18..=18, species: SpeciesId::HOUNDOUR),
        wild!(levels: 14..=14, species: SpeciesId::HOUNDOUR),
        wild!(levels: 20..=20, species: SpeciesId::HOUNDOUR),
        wild!(levels: 16..=16, species: SpeciesId::HOUNDOUR),
        wild!(levels: 18..=18, species: SpeciesId::HOUNDOUR),
        wild!(levels: 22..=22, species: SpeciesId::HOUNDOUR),
        wild!(levels: 12..=12, species: SpeciesId::HOUNDOUR),
        wild!(levels: 14..=14, species: SpeciesId::HOUNDOUR),
        wild!(levels: 20..=20, species: SpeciesId::HOUNDOUR),
        wild!(levels: 14..=14, species: SpeciesId::HOUNDOUR),
        wild!(levels: 20..=20, species: SpeciesId::HOUNDOUR),
    ],
};

const GALTERINGCAVE5_LAND: LandEncounters = LandEncounters {
    encounter_rate: 7,
    mons: [
        wild!(levels: 10..=10, species: SpeciesId::TEDDIURSA),
        wild!(levels: 12..=12, species: SpeciesId::TEDDIURSA),
        wild!(levels: 8..=8, species: SpeciesId::TEDDIURSA),
        wild!(levels: 14..=14, species: SpeciesId::TEDDIURSA),
        wild!(levels: 10..=10, species: SpeciesId::TEDDIURSA),
        wild!(levels: 12..=12, species: SpeciesId::TEDDIURSA),
        wild!(levels: 16..=16, species: SpeciesId::TEDDIURSA),
        wild!(levels: 6..=6, species: SpeciesId::TEDDIURSA),
        wild!(levels: 8..=8, species: SpeciesId::TEDDIURSA),
        wild!(levels: 14..=14, species: SpeciesId::TEDDIURSA),
        wild!(levels: 8..=8, species: SpeciesId::TEDDIURSA),
        wild!(levels: 14..=14, species: SpeciesId::TEDDIURSA),
    ],
};

const GALTERINGCAVE6_LAND: LandEncounters = LandEncounters {
    encounter_rate: 7,
    mons: [
        wild!(levels: 22..=22, species: SpeciesId::AIPOM),
        wild!(levels: 24..=24, species: SpeciesId::AIPOM),
        wild!(levels: 20..=20, species: SpeciesId::AIPOM),
        wild!(levels: 26..=26, species: SpeciesId::AIPOM),
        wild!(levels: 22..=22, species: SpeciesId::AIPOM),
        wild!(levels: 24..=24, species: SpeciesId::AIPOM),
        wild!(levels: 28..=28, species: SpeciesId::AIPOM),
        wild!(levels: 18..=18, species: SpeciesId::AIPOM),
        wild!(levels: 20..=20, species: SpeciesId::AIPOM),
        wild!(levels: 26..=26, species: SpeciesId::AIPOM),
        wild!(levels: 20..=20, species: SpeciesId::AIPOM),
        wild!(levels: 26..=26, species: SpeciesId::AIPOM),
    ],
};

const GALTERINGCAVE7_LAND: LandEncounters = LandEncounters {
    encounter_rate: 7,
    mons: [
        wild!(levels: 22..=22, species: SpeciesId::SHUCKLE),
        wild!(levels: 24..=24, species: SpeciesId::SHUCKLE),
        wild!(levels: 20..=20, species: SpeciesId::SHUCKLE),
        wild!(levels: 26..=26, species: SpeciesId::SHUCKLE),
        wild!(levels: 22..=22, species: SpeciesId::SHUCKLE),
        wild!(levels: 24..=24, species: SpeciesId::SHUCKLE),
        wild!(levels: 28..=28, species: SpeciesId::SHUCKLE),
        wild!(levels: 18..=18, species: SpeciesId::SHUCKLE),
        wild!(levels: 20..=20, species: SpeciesId::SHUCKLE),
        wild!(levels: 26..=26, species: SpeciesId::SHUCKLE),
        wild!(levels: 20..=20, species: SpeciesId::SHUCKLE),
        wild!(levels: 26..=26, species: SpeciesId::SHUCKLE),
    ],
};

const GALTERINGCAVE8_LAND: LandEncounters = LandEncounters {
    encounter_rate: 7,
    mons: [
        wild!(levels: 22..=22, species: SpeciesId::STANTLER),
        wild!(levels: 24..=24, species: SpeciesId::STANTLER),
        wild!(levels: 20..=20, species: SpeciesId::STANTLER),
        wild!(levels: 26..=26, species: SpeciesId::STANTLER),
        wild!(levels: 22..=22, species: SpeciesId::STANTLER),
        wild!(levels: 24..=24, species: SpeciesId::STANTLER),
        wild!(levels: 28..=28, species: SpeciesId::STANTLER),
        wild!(levels: 18..=18, species: SpeciesId::STANTLER),
        wild!(levels: 20..=20, species: SpeciesId::STANTLER),
        wild!(levels: 26..=26, species: SpeciesId::STANTLER),
        wild!(levels: 20..=20, species: SpeciesId::STANTLER),
        wild!(levels: 26..=26, species: SpeciesId::STANTLER),
    ],
};

const GALTERINGCAVE9_LAND: LandEncounters = LandEncounters {
    encounter_rate: 7,
    mons: [
        wild!(levels: 22..=22, species: SpeciesId::SMEARGLE),
        wild!(levels: 24..=24, species: SpeciesId::SMEARGLE),
        wild!(levels: 20..=20, species: SpeciesId::SMEARGLE),
        wild!(levels: 26..=26, species: SpeciesId::SMEARGLE),
        wild!(levels: 22..=22, species: SpeciesId::SMEARGLE),
        wild!(levels: 24..=24, species: SpeciesId::SMEARGLE),
        wild!(levels: 28..=28, species: SpeciesId::SMEARGLE),
        wild!(levels: 18..=18, species: SpeciesId::SMEARGLE),
        wild!(levels: 20..=20, species: SpeciesId::SMEARGLE),
        wild!(levels: 26..=26, species: SpeciesId::SMEARGLE),
        wild!(levels: 20..=20, species: SpeciesId::SMEARGLE),
        wild!(levels: 26..=26, species: SpeciesId::SMEARGLE),
    ],
};

const GMETEORFALLS_STEVENSCAVE_LAND: LandEncounters = LandEncounters {
    encounter_rate: 10,
    mons: [
        wild!(levels: 33..=33, species: SpeciesId::GOLBAT),
        wild!(levels: 35..=35, species: SpeciesId::GOLBAT),
        wild!(levels: 33..=33, species: SpeciesId::GOLBAT),
        wild!(levels: 35..=35, species: SpeciesId::SOLROCK),
        wild!(levels: 33..=33, species: SpeciesId::SOLROCK),
        wild!(levels: 37..=37, species: SpeciesId::SOLROCK),
        wild!(levels: 35..=35, species: SpeciesId::GOLBAT),
        wild!(levels: 39..=39, species: SpeciesId::SOLROCK),
        wild!(levels: 38..=38, species: SpeciesId::GOLBAT),
        wild!(levels: 40..=40, species: SpeciesId::GOLBAT),
        wild!(levels: 38..=38, species: SpeciesId::GOLBAT),
        wild!(levels: 40..=40, species: SpeciesId::GOLBAT),
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

/// Read-only access to every wild-encounter header.
#[derive(Debug, Clone, Copy)]
pub struct WildEncounterTable {
    headers: &'static [WildEncounterHeader; MAP_HEADER_COUNT],
}

impl WildEncounterTable {
    /// Number of entries in the table.
    pub const LEN: usize = MAP_HEADER_COUNT;

    /// Returns the canonical encounter table.
    #[must_use]
    pub const fn new() -> Self {
        Self { headers: &HEADERS }
    }

    /// Returns the entry with `label`, if present.
    #[must_use]
    pub fn get_by_label(&self, label: &str) -> Option<&'static WildEncounterHeader> {
        self.headers.iter().find(|h| h.label == label)
    }

    /// Returns the entry with `label`.
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

    /// Returns the first entry for `map`, if present.
    ///
    /// Altering Cave has nine entries; use [`Self::all_by_map`] to access all
    /// of its variants.
    #[must_use]
    pub fn get_by_map(&self, map: MapId) -> Option<&'static WildEncounterHeader> {
        self.headers.iter().find(|h| h.map == map)
    }

    /// Returns the first entry for `map`.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownMap`] if no entry names that map.
    pub fn by_map(&self, map: MapId) -> Result<&'static WildEncounterHeader, AssetError> {
        self.get_by_map(map).ok_or(AssetError::UnknownMap(map.0))
    }

    /// Returns every entry for `map`, in table order.
    pub fn all_by_map(&self, map: MapId) -> impl Iterator<Item = &'static WildEncounterHeader> {
        self.headers.iter().filter(move |h| h.map == map)
    }

    /// Iterates over every entry in canonical table order.
    pub fn iter(&self) -> impl Iterator<Item = &'static WildEncounterHeader> {
        self.headers.iter()
    }

    /// Returns the number of entries.
    #[must_use]
    pub const fn len(&self) -> usize {
        MAP_HEADER_COUNT
    }

    /// Returns `false`; the fixed canonical table is never empty.
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
        FishingRod, MapId, WildEncounterTable, WildPokemon, FISHING_SLOTS, LAND_SLOTS,
        MAP_HEADER_COUNT, ROCK_SMASH_SLOTS, WATER_SLOTS,
    };
    use crate::error::AssetError;
    use crate::species::{SpeciesId, SPECIES_COUNT};

    #[test]
    fn table_has_every_canonical_header() {
        let table = WildEncounterTable::new();
        assert_eq!(MAP_HEADER_COUNT, 124);
        assert_eq!(table.len(), 124);
        assert_eq!(WildEncounterTable::LEN, 124);
        assert_eq!(table.iter().count(), 124);
        assert!(!table.is_empty());
    }

    #[test]
    fn every_present_kind_has_the_fixed_slot_count() {
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
    fn fishing_rods_partition_all_slots() {
        assert_eq!(FishingRod::Old.slots(), &[0, 1]);
        assert_eq!(FishingRod::Good.slots(), &[2, 3, 4]);
        assert_eq!(FishingRod::Super.slots(), &[5, 6, 7, 8, 9]);

        let slots: Vec<_> = [FishingRod::Old, FishingRod::Good, FishingRod::Super]
            .into_iter()
            .flat_map(FishingRod::slots)
            .copied()
            .collect();
        assert_eq!(slots, (0..FISHING_SLOTS).collect::<Vec<_>>());
    }

    #[test]
    fn every_header_and_encounter_slot_is_structurally_valid() {
        let table = WildEncounterTable::new();
        let mut map_counts = std::collections::HashMap::new();

        for header in table.iter() {
            assert!(header.map.name().starts_with("MAP_"));
            assert!(header.label.starts_with('g'));
            *map_counts.entry(header.map).or_insert(0) += 1;

            let kinds: [Option<(u8, &[WildPokemon])>; 4] = [
                header
                    .land
                    .as_ref()
                    .map(|encounters| (encounters.encounter_rate, encounters.mons.as_slice())),
                header
                    .water
                    .as_ref()
                    .map(|encounters| (encounters.encounter_rate, encounters.mons.as_slice())),
                header
                    .rock_smash
                    .as_ref()
                    .map(|encounters| (encounters.encounter_rate, encounters.mons.as_slice())),
                header
                    .fishing
                    .as_ref()
                    .map(|encounters| (encounters.encounter_rate, encounters.mons.as_slice())),
            ];
            assert!(kinds.iter().any(Option::is_some));

            for (encounter_rate, slots) in kinds.into_iter().flatten() {
                assert!(encounter_rate > 0);
                for slot in slots {
                    assert!(slot.min_level > 0);
                    assert!(slot.min_level <= slot.max_level);
                    assert_ne!(slot.species, SpeciesId::NONE);
                    assert!((slot.species.index() as usize) < SPECIES_COUNT);
                }
            }
        }

        assert_eq!(map_counts.get(&MapId("MAP_ALTERING_CAVE")), Some(&9));
        assert!(map_counts.iter().all(|(map, count)| {
            *count == 1 || (*map == MapId("MAP_ALTERING_CAVE") && *count == 9)
        }));
    }

    #[test]
    fn route_101_has_only_its_canonical_land_encounters() {
        let table = WildEncounterTable::new();
        let h = table.by_map(MapId("MAP_ROUTE101")).unwrap();
        assert_eq!(h.label, "gRoute101");
        assert!(h.water.is_none());
        assert!(h.rock_smash.is_none());
        assert!(h.fishing.is_none());
        let land = h.land.unwrap();
        assert_eq!(land.encounter_rate, 20);
        let expected: &[(u8, u8, SpeciesId)] = &[
            (2, 2, SpeciesId::WURMPLE),
            (2, 2, SpeciesId::POOCHYENA),
            (2, 2, SpeciesId::WURMPLE),
            (3, 3, SpeciesId::WURMPLE),
            (3, 3, SpeciesId::POOCHYENA),
            (3, 3, SpeciesId::POOCHYENA),
            (3, 3, SpeciesId::WURMPLE),
            (3, 3, SpeciesId::POOCHYENA),
            (2, 2, SpeciesId::ZIGZAGOON),
            (2, 2, SpeciesId::ZIGZAGOON),
            (3, 3, SpeciesId::ZIGZAGOON),
            (3, 3, SpeciesId::ZIGZAGOON),
        ];
        for (slot, &(min, max, species)) in land.mons.iter().zip(expected) {
            assert_eq!(slot.min_level, min);
            assert_eq!(slot.max_level, max);
            assert_eq!(slot.species, species);
        }
    }

    #[test]
    fn route_102_has_its_canonical_water_and_fishing_encounters() {
        let table = WildEncounterTable::new();
        let h = table.by_map(MapId("MAP_ROUTE102")).unwrap();
        assert_eq!(h.label, "gRoute102");
        assert!(h.rock_smash.is_none());

        let water = h.water.unwrap();
        assert_eq!(water.encounter_rate, 4);
        let expected_water: &[(u8, u8, SpeciesId)] = &[
            (20, 30, SpeciesId::MARILL),
            (10, 20, SpeciesId::MARILL),
            (30, 35, SpeciesId::MARILL),
            (5, 10, SpeciesId::MARILL),
            (20, 30, SpeciesId::GOLDEEN),
        ];
        for (slot, &(min, max, species)) in water.mons.iter().zip(expected_water) {
            assert_eq!(slot.min_level, min);
            assert_eq!(slot.max_level, max);
            assert_eq!(slot.species, species);
        }

        let fishing = h.fishing.unwrap();
        assert_eq!(fishing.encounter_rate, 30);
        let expected_fishing: &[(u8, u8, SpeciesId)] = &[
            (5, 10, SpeciesId::MAGIKARP),
            (5, 10, SpeciesId::GOLDEEN),
            (10, 30, SpeciesId::MAGIKARP),
            (10, 30, SpeciesId::GOLDEEN),
            (10, 30, SpeciesId::CORPHISH),
            (25, 30, SpeciesId::CORPHISH),
            (30, 35, SpeciesId::CORPHISH),
            (20, 25, SpeciesId::CORPHISH),
            (35, 40, SpeciesId::CORPHISH),
            (40, 45, SpeciesId::CORPHISH),
        ];
        for (slot, &(min, max, species)) in fishing.mons.iter().zip(expected_fishing) {
            assert_eq!(slot.min_level, min);
            assert_eq!(slot.max_level, max);
            assert_eq!(slot.species, species);
        }

        let old_rod: Vec<_> = fishing.mons_for_rod(FishingRod::Old).collect();
        assert_eq!(old_rod.len(), 2);
        assert_eq!(old_rod[0].species, SpeciesId::MAGIKARP);
        assert_eq!(old_rod[1].species, SpeciesId::GOLDEEN);
        let super_rod: Vec<_> = fishing.mons_for_rod(FishingRod::Super).collect();
        assert_eq!(super_rod.len(), 5);
        assert!(super_rod
            .iter()
            .all(|encounter| encounter.species == SpeciesId::CORPHISH));
    }

    #[test]
    fn route_111_has_its_canonical_rock_smash_encounters() {
        let table = WildEncounterTable::new();
        let h = table.by_map(MapId("MAP_ROUTE111")).unwrap();
        let rock_smash = h.rock_smash.unwrap();
        assert_eq!(rock_smash.encounter_rate, 20);
        let expected: &[(u8, u8, SpeciesId)] = &[
            (10, 15, SpeciesId::GEODUDE),
            (5, 10, SpeciesId::GEODUDE),
            (15, 20, SpeciesId::GEODUDE),
            (15, 20, SpeciesId::GEODUDE),
            (15, 20, SpeciesId::GEODUDE),
        ];
        for (slot, &(min, max, species)) in rock_smash.mons.iter().zip(expected) {
            assert_eq!(slot.min_level, min);
            assert_eq!(slot.max_level, max);
            assert_eq!(slot.species, species);
        }
    }

    #[test]
    fn altering_cave_variants_have_distinct_labels_and_species() {
        let table = WildEncounterTable::new();
        let entries: Vec<_> = table.all_by_map(MapId("MAP_ALTERING_CAVE")).collect();
        let expected = [
            ("gAlteringCave1", SpeciesId::ZUBAT),
            ("gAlteringCave2", SpeciesId::MAREEP),
            ("gAlteringCave3", SpeciesId::PINECO),
            ("gAlteringCave4", SpeciesId::HOUNDOUR),
            ("gAlteringCave5", SpeciesId::TEDDIURSA),
            ("gAlteringCave6", SpeciesId::AIPOM),
            ("gAlteringCave7", SpeciesId::SHUCKLE),
            ("gAlteringCave8", SpeciesId::STANTLER),
            ("gAlteringCave9", SpeciesId::SMEARGLE),
        ];

        for (entry, (label, species)) in entries.iter().zip(expected) {
            assert_eq!(entry.label, label);
            assert_eq!(entry.land.unwrap().mons[0].species, species);
        }

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
