//! The `MAPSEC_*` enumeration's numeric values, the byte a Pokemon's met
//! location stores and `GetCurrentRegionMapSectionId` compares it with
//! (`pokeemerald/src/overworld.c:1391-1394`).
//!
//! Upstream generates `include/constants/region_map_sections.h` as an `enum`
//! over `src/data/region_map/region_map_sections.json`'s `map_sections`, in
//! file order from zero, followed by `MAPSEC_NONE` and `MAPSEC_COUNT`.

use super::RegionMapSectionId;

/// `MAPSEC_NONE`: the value following the last JSON section.
const MAPSEC_NONE: u8 = 213;

/// Every JSON `map_sections` id; a name's position is its `MAPSEC_*` value.
const NAMES: [&str; 213] = [
    "MAPSEC_LITTLEROOT_TOWN",
    "MAPSEC_OLDALE_TOWN",
    "MAPSEC_DEWFORD_TOWN",
    "MAPSEC_LAVARIDGE_TOWN",
    "MAPSEC_FALLARBOR_TOWN",
    "MAPSEC_VERDANTURF_TOWN",
    "MAPSEC_PACIFIDLOG_TOWN",
    "MAPSEC_PETALBURG_CITY",
    "MAPSEC_SLATEPORT_CITY",
    "MAPSEC_MAUVILLE_CITY",
    "MAPSEC_RUSTBORO_CITY",
    "MAPSEC_FORTREE_CITY",
    "MAPSEC_LILYCOVE_CITY",
    "MAPSEC_MOSSDEEP_CITY",
    "MAPSEC_SOOTOPOLIS_CITY",
    "MAPSEC_EVER_GRANDE_CITY",
    "MAPSEC_ROUTE_101",
    "MAPSEC_ROUTE_102",
    "MAPSEC_ROUTE_103",
    "MAPSEC_ROUTE_104",
    "MAPSEC_ROUTE_105",
    "MAPSEC_ROUTE_106",
    "MAPSEC_ROUTE_107",
    "MAPSEC_ROUTE_108",
    "MAPSEC_ROUTE_109",
    "MAPSEC_ROUTE_110",
    "MAPSEC_ROUTE_111",
    "MAPSEC_ROUTE_112",
    "MAPSEC_ROUTE_113",
    "MAPSEC_ROUTE_114",
    "MAPSEC_ROUTE_115",
    "MAPSEC_ROUTE_116",
    "MAPSEC_ROUTE_117",
    "MAPSEC_ROUTE_118",
    "MAPSEC_ROUTE_119",
    "MAPSEC_ROUTE_120",
    "MAPSEC_ROUTE_121",
    "MAPSEC_ROUTE_122",
    "MAPSEC_ROUTE_123",
    "MAPSEC_ROUTE_124",
    "MAPSEC_ROUTE_125",
    "MAPSEC_ROUTE_126",
    "MAPSEC_ROUTE_127",
    "MAPSEC_ROUTE_128",
    "MAPSEC_ROUTE_129",
    "MAPSEC_ROUTE_130",
    "MAPSEC_ROUTE_131",
    "MAPSEC_ROUTE_132",
    "MAPSEC_ROUTE_133",
    "MAPSEC_ROUTE_134",
    "MAPSEC_UNDERWATER_124",
    "MAPSEC_UNDERWATER_126",
    "MAPSEC_UNDERWATER_127",
    "MAPSEC_UNDERWATER_128",
    "MAPSEC_UNDERWATER_SOOTOPOLIS",
    "MAPSEC_GRANITE_CAVE",
    "MAPSEC_MT_CHIMNEY",
    "MAPSEC_SAFARI_ZONE",
    "MAPSEC_BATTLE_FRONTIER",
    "MAPSEC_PETALBURG_WOODS",
    "MAPSEC_RUSTURF_TUNNEL",
    "MAPSEC_ABANDONED_SHIP",
    "MAPSEC_NEW_MAUVILLE",
    "MAPSEC_METEOR_FALLS",
    "MAPSEC_METEOR_FALLS2",
    "MAPSEC_MT_PYRE",
    "MAPSEC_AQUA_HIDEOUT_OLD",
    "MAPSEC_SHOAL_CAVE",
    "MAPSEC_SEAFLOOR_CAVERN",
    "MAPSEC_UNDERWATER_SEAFLOOR_CAVERN",
    "MAPSEC_VICTORY_ROAD",
    "MAPSEC_MIRAGE_ISLAND",
    "MAPSEC_CAVE_OF_ORIGIN",
    "MAPSEC_SOUTHERN_ISLAND",
    "MAPSEC_FIERY_PATH",
    "MAPSEC_FIERY_PATH2",
    "MAPSEC_JAGGED_PASS",
    "MAPSEC_JAGGED_PASS2",
    "MAPSEC_SEALED_CHAMBER",
    "MAPSEC_UNDERWATER_SEALED_CHAMBER",
    "MAPSEC_SCORCHED_SLAB",
    "MAPSEC_ISLAND_CAVE",
    "MAPSEC_DESERT_RUINS",
    "MAPSEC_ANCIENT_TOMB",
    "MAPSEC_INSIDE_OF_TRUCK",
    "MAPSEC_SKY_PILLAR",
    "MAPSEC_SECRET_BASE",
    "MAPSEC_DYNAMIC",
    "MAPSEC_PALLET_TOWN",
    "MAPSEC_VIRIDIAN_CITY",
    "MAPSEC_PEWTER_CITY",
    "MAPSEC_CERULEAN_CITY",
    "MAPSEC_LAVENDER_TOWN",
    "MAPSEC_VERMILION_CITY",
    "MAPSEC_CELADON_CITY",
    "MAPSEC_FUCHSIA_CITY",
    "MAPSEC_CINNABAR_ISLAND",
    "MAPSEC_INDIGO_PLATEAU",
    "MAPSEC_SAFFRON_CITY",
    "MAPSEC_ROUTE_4_POKECENTER",
    "MAPSEC_ROUTE_10_POKECENTER",
    "MAPSEC_ROUTE_1",
    "MAPSEC_ROUTE_2",
    "MAPSEC_ROUTE_3",
    "MAPSEC_ROUTE_4",
    "MAPSEC_ROUTE_5",
    "MAPSEC_ROUTE_6",
    "MAPSEC_ROUTE_7",
    "MAPSEC_ROUTE_8",
    "MAPSEC_ROUTE_9",
    "MAPSEC_ROUTE_10",
    "MAPSEC_ROUTE_11",
    "MAPSEC_ROUTE_12",
    "MAPSEC_ROUTE_13",
    "MAPSEC_ROUTE_14",
    "MAPSEC_ROUTE_15",
    "MAPSEC_ROUTE_16",
    "MAPSEC_ROUTE_17",
    "MAPSEC_ROUTE_18",
    "MAPSEC_ROUTE_19",
    "MAPSEC_ROUTE_20",
    "MAPSEC_ROUTE_21",
    "MAPSEC_ROUTE_22",
    "MAPSEC_ROUTE_23",
    "MAPSEC_ROUTE_24",
    "MAPSEC_ROUTE_25",
    "MAPSEC_VIRIDIAN_FOREST",
    "MAPSEC_MT_MOON",
    "MAPSEC_S_S_ANNE",
    "MAPSEC_UNDERGROUND_PATH",
    "MAPSEC_UNDERGROUND_PATH_2",
    "MAPSEC_DIGLETTS_CAVE",
    "MAPSEC_KANTO_VICTORY_ROAD",
    "MAPSEC_ROCKET_HIDEOUT",
    "MAPSEC_SILPH_CO",
    "MAPSEC_POKEMON_MANSION",
    "MAPSEC_KANTO_SAFARI_ZONE",
    "MAPSEC_POKEMON_LEAGUE",
    "MAPSEC_ROCK_TUNNEL",
    "MAPSEC_SEAFOAM_ISLANDS",
    "MAPSEC_POKEMON_TOWER",
    "MAPSEC_CERULEAN_CAVE",
    "MAPSEC_POWER_PLANT",
    "MAPSEC_ONE_ISLAND",
    "MAPSEC_TWO_ISLAND",
    "MAPSEC_THREE_ISLAND",
    "MAPSEC_FOUR_ISLAND",
    "MAPSEC_FIVE_ISLAND",
    "MAPSEC_SEVEN_ISLAND",
    "MAPSEC_SIX_ISLAND",
    "MAPSEC_KINDLE_ROAD",
    "MAPSEC_TREASURE_BEACH",
    "MAPSEC_CAPE_BRINK",
    "MAPSEC_BOND_BRIDGE",
    "MAPSEC_THREE_ISLE_PORT",
    "MAPSEC_SEVII_ISLE_6",
    "MAPSEC_SEVII_ISLE_7",
    "MAPSEC_SEVII_ISLE_8",
    "MAPSEC_SEVII_ISLE_9",
    "MAPSEC_RESORT_GORGEOUS",
    "MAPSEC_WATER_LABYRINTH",
    "MAPSEC_FIVE_ISLE_MEADOW",
    "MAPSEC_MEMORIAL_PILLAR",
    "MAPSEC_OUTCAST_ISLAND",
    "MAPSEC_GREEN_PATH",
    "MAPSEC_WATER_PATH",
    "MAPSEC_RUIN_VALLEY",
    "MAPSEC_TRAINER_TOWER",
    "MAPSEC_CANYON_ENTRANCE",
    "MAPSEC_SEVAULT_CANYON",
    "MAPSEC_TANOBY_RUINS",
    "MAPSEC_SEVII_ISLE_22",
    "MAPSEC_SEVII_ISLE_23",
    "MAPSEC_SEVII_ISLE_24",
    "MAPSEC_NAVEL_ROCK_FRLG",
    "MAPSEC_MT_EMBER",
    "MAPSEC_BERRY_FOREST",
    "MAPSEC_ICEFALL_CAVE",
    "MAPSEC_ROCKET_WAREHOUSE",
    "MAPSEC_TRAINER_TOWER_2",
    "MAPSEC_DOTTED_HOLE",
    "MAPSEC_LOST_CAVE",
    "MAPSEC_PATTERN_BUSH",
    "MAPSEC_ALTERING_CAVE_FRLG",
    "MAPSEC_TANOBY_CHAMBERS",
    "MAPSEC_THREE_ISLE_PATH",
    "MAPSEC_TANOBY_KEY",
    "MAPSEC_BIRTH_ISLAND_FRLG",
    "MAPSEC_MONEAN_CHAMBER",
    "MAPSEC_LIPTOO_CHAMBER",
    "MAPSEC_WEEPTH_CHAMBER",
    "MAPSEC_DILFORD_CHAMBER",
    "MAPSEC_SCUFIB_CHAMBER",
    "MAPSEC_RIXY_CHAMBER",
    "MAPSEC_VIAPOIS_CHAMBER",
    "MAPSEC_EMBER_SPA",
    "MAPSEC_SPECIAL_AREA",
    "MAPSEC_AQUA_HIDEOUT",
    "MAPSEC_MAGMA_HIDEOUT",
    "MAPSEC_MIRAGE_TOWER",
    "MAPSEC_BIRTH_ISLAND",
    "MAPSEC_FARAWAY_ISLAND",
    "MAPSEC_ARTISAN_CAVE",
    "MAPSEC_MARINE_CAVE",
    "MAPSEC_UNDERWATER_MARINE_CAVE",
    "MAPSEC_TERRA_CAVE",
    "MAPSEC_UNDERWATER_105",
    "MAPSEC_UNDERWATER_125",
    "MAPSEC_UNDERWATER_129",
    "MAPSEC_DESERT_UNDERPASS",
    "MAPSEC_ALTERING_CAVE",
    "MAPSEC_NAVEL_ROCK",
    "MAPSEC_TRAINER_HILL",
];

impl RegionMapSectionId {
    /// Returns the numeric `MAPSEC_*` value, or `None` for a name that is not
    /// an upstream section.
    #[must_use]
    pub fn id(self) -> Option<u8> {
        if self.0 == "MAPSEC_NONE" {
            return Some(MAPSEC_NONE);
        }
        NAMES
            .iter()
            .position(|name| *name == self.0)
            .and_then(|index| u8::try_from(index).ok())
    }
}

#[cfg(test)]
mod tests {
    use super::{MAPSEC_NONE, NAMES};
    use crate::map_headers::{MapHeaderTable, RegionMapSectionId};

    fn id(name: &'static str) -> Option<u8> {
        RegionMapSectionId(name).id()
    }

    /// Values read off `region_map_sections.json`'s order: the first section,
    /// Route 101 (also the starter's met location, `party::starter`), a
    /// dungeon, a late special area, and the last section, then `MAPSEC_NONE`.
    #[test]
    fn sections_take_their_upstream_enum_values() {
        assert_eq!(id("MAPSEC_LITTLEROOT_TOWN"), Some(0));
        assert_eq!(id("MAPSEC_OLDALE_TOWN"), Some(1));
        assert_eq!(id("MAPSEC_ROUTE_101"), Some(16));
        assert_eq!(id("MAPSEC_GRANITE_CAVE"), Some(55));
        assert_eq!(id("MAPSEC_SPECIAL_AREA"), Some(196));
        assert_eq!(id("MAPSEC_TRAINER_HILL"), Some(212));
        assert_eq!(id("MAPSEC_NONE"), Some(MAPSEC_NONE));
        assert_eq!(id("MAPSEC_NOT_A_SECTION"), None);
    }

    #[test]
    fn names_are_distinct_and_fit_the_met_location_byte() {
        let mut sorted = NAMES.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), NAMES.len());
        assert!(NAMES.len() <= usize::from(MAPSEC_NONE));
    }

    /// Every bundled map header's section resolves to a number.
    #[test]
    fn every_map_header_section_resolves() {
        for header in MapHeaderTable::new().iter() {
            assert!(
                header.region_map_section.id().is_some(),
                "{}: {}",
                header.name,
                header.region_map_section.name()
            );
        }
    }
}
