//! The Pokénav city-map selector catalog.
//!
//! Upstream's `sPokenavCityMaps` (`pokeemerald/src/data/region_map/city_map_entries.h`)
//! lists the 22 zoomed city maps, each keyed by a region-map section and a
//! position index (`NUM_CITY_MAPS`, `pokeemerald/src/pokenav_region_map.c:23`).
//! `DrawCityMap` picks the entry whose section and position both match
//! (`pokeemerald/src/pokenav_region_map.c:637-643`); a city drawn as two maps
//! has one entry per position.
//!
//! Only the selector metadata lives here. [`CityMapId`] names the tilemap an
//! entry points at; the tilemap bytes (`city_map_tilemaps.h`) are not part of
//! this table, so an id claims no loaded graphics.

use crate::map_headers::RegionMapSectionId;

/// Names the upstream city-map tilemap an entry selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CityMapId(pub &'static str);

impl CityMapId {
    /// Returns the upstream tilemap symbol name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        self.0
    }
}

/// One selector record: a section and position that show a city map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CityMapEntry {
    /// The region-map section the map belongs to.
    pub section: RegionMapSectionId,
    /// Which of the section's maps this is; `0` unless the city is split.
    pub position: u8,
    /// The tilemap this entry selects.
    pub map: CityMapId,
}

const fn entry(section: &'static str, position: u8, map: &'static str) -> CityMapEntry {
    CityMapEntry {
        section: RegionMapSectionId(section),
        position,
        map: CityMapId(map),
    }
}

/// `NUM_CITY_MAPS`.
pub const CITY_MAP_COUNT: usize = 22;

static ENTRIES: [CityMapEntry; CITY_MAP_COUNT] = [
    entry("MAPSEC_LITTLEROOT_TOWN", 0, "gPokenavCityMap_Littleroot_0"),
    entry("MAPSEC_OLDALE_TOWN", 0, "gPokenavCityMap_Oldale_0"),
    entry("MAPSEC_DEWFORD_TOWN", 0, "gPokenavCityMap_Dewford_0"),
    entry("MAPSEC_LAVARIDGE_TOWN", 0, "gPokenavCityMap_Lavaridge_0"),
    entry("MAPSEC_FALLARBOR_TOWN", 0, "gPokenavCityMap_Fallarbor_0"),
    entry("MAPSEC_VERDANTURF_TOWN", 0, "gPokenavCityMap_Verdanturf_0"),
    entry("MAPSEC_PACIFIDLOG_TOWN", 0, "gPokenavCityMap_Pacifidlog_0"),
    entry("MAPSEC_PETALBURG_CITY", 0, "gPokenavCityMap_Petalburg_0"),
    entry("MAPSEC_SLATEPORT_CITY", 0, "gPokenavCityMap_Slateport_0"),
    entry("MAPSEC_SLATEPORT_CITY", 1, "gPokenavCityMap_Slateport_1"),
    entry("MAPSEC_MAUVILLE_CITY", 0, "gPokenavCityMap_Mauville_0"),
    entry("MAPSEC_MAUVILLE_CITY", 1, "gPokenavCityMap_Mauville_1"),
    entry("MAPSEC_RUSTBORO_CITY", 0, "gPokenavCityMap_Rustboro_0"),
    entry("MAPSEC_RUSTBORO_CITY", 1, "gPokenavCityMap_Rustboro_1"),
    entry("MAPSEC_FORTREE_CITY", 0, "gPokenavCityMap_Fortree_0"),
    entry("MAPSEC_LILYCOVE_CITY", 0, "gPokenavCityMap_Lilycove_0"),
    entry("MAPSEC_LILYCOVE_CITY", 1, "gPokenavCityMap_Lilycove_1"),
    entry("MAPSEC_MOSSDEEP_CITY", 0, "gPokenavCityMap_Mossdeep_0"),
    entry("MAPSEC_MOSSDEEP_CITY", 1, "gPokenavCityMap_Mossdeep_1"),
    entry("MAPSEC_SOOTOPOLIS_CITY", 0, "gPokenavCityMap_Sootopolis_0"),
    entry("MAPSEC_EVER_GRANDE_CITY", 0, "gPokenavCityMap_EverGrande_0"),
    entry("MAPSEC_EVER_GRANDE_CITY", 1, "gPokenavCityMap_EverGrande_1"),
];

/// Read-only access to the city-map selector entries.
#[derive(Debug, Clone, Copy, Default)]
pub struct CityMapCatalog;

impl CityMapCatalog {
    /// Returns the catalog.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Returns every entry in upstream order.
    #[must_use]
    pub fn entries(&self) -> &'static [CityMapEntry] {
        &ENTRIES
    }

    /// Returns the entry whose section and position both match, if any.
    #[must_use]
    pub fn find(&self, section: RegionMapSectionId, position: u8) -> Option<&'static CityMapEntry> {
        ENTRIES
            .iter()
            .find(|e| e.section == section && e.position == position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `(section, position, tilemap)` in upstream order, transcribed from
    /// `city_map_entries.h` independently of `ENTRIES`.
    const EXPECTED: [(&str, u8, &str); 22] = [
        ("MAPSEC_LITTLEROOT_TOWN", 0, "gPokenavCityMap_Littleroot_0"),
        ("MAPSEC_OLDALE_TOWN", 0, "gPokenavCityMap_Oldale_0"),
        ("MAPSEC_DEWFORD_TOWN", 0, "gPokenavCityMap_Dewford_0"),
        ("MAPSEC_LAVARIDGE_TOWN", 0, "gPokenavCityMap_Lavaridge_0"),
        ("MAPSEC_FALLARBOR_TOWN", 0, "gPokenavCityMap_Fallarbor_0"),
        ("MAPSEC_VERDANTURF_TOWN", 0, "gPokenavCityMap_Verdanturf_0"),
        ("MAPSEC_PACIFIDLOG_TOWN", 0, "gPokenavCityMap_Pacifidlog_0"),
        ("MAPSEC_PETALBURG_CITY", 0, "gPokenavCityMap_Petalburg_0"),
        ("MAPSEC_SLATEPORT_CITY", 0, "gPokenavCityMap_Slateport_0"),
        ("MAPSEC_SLATEPORT_CITY", 1, "gPokenavCityMap_Slateport_1"),
        ("MAPSEC_MAUVILLE_CITY", 0, "gPokenavCityMap_Mauville_0"),
        ("MAPSEC_MAUVILLE_CITY", 1, "gPokenavCityMap_Mauville_1"),
        ("MAPSEC_RUSTBORO_CITY", 0, "gPokenavCityMap_Rustboro_0"),
        ("MAPSEC_RUSTBORO_CITY", 1, "gPokenavCityMap_Rustboro_1"),
        ("MAPSEC_FORTREE_CITY", 0, "gPokenavCityMap_Fortree_0"),
        ("MAPSEC_LILYCOVE_CITY", 0, "gPokenavCityMap_Lilycove_0"),
        ("MAPSEC_LILYCOVE_CITY", 1, "gPokenavCityMap_Lilycove_1"),
        ("MAPSEC_MOSSDEEP_CITY", 0, "gPokenavCityMap_Mossdeep_0"),
        ("MAPSEC_MOSSDEEP_CITY", 1, "gPokenavCityMap_Mossdeep_1"),
        ("MAPSEC_SOOTOPOLIS_CITY", 0, "gPokenavCityMap_Sootopolis_0"),
        ("MAPSEC_EVER_GRANDE_CITY", 0, "gPokenavCityMap_EverGrande_0"),
        ("MAPSEC_EVER_GRANDE_CITY", 1, "gPokenavCityMap_EverGrande_1"),
    ];

    fn section(name: &'static str) -> RegionMapSectionId {
        RegionMapSectionId(name)
    }

    #[test]
    fn entries_match_upstream_in_order() {
        let actual: Vec<_> = CityMapCatalog::new()
            .entries()
            .iter()
            .map(|e| (e.section.name(), e.position, e.map.name()))
            .collect();
        assert_eq!(actual, EXPECTED);
    }

    #[test]
    fn find_returns_each_entry_by_its_section_and_position() {
        let catalog = CityMapCatalog::new();
        for (name, position, map) in EXPECTED {
            let found = catalog
                .find(section(name), position)
                .unwrap_or_else(|| panic!("{name}/{position} has no entry"));
            assert_eq!(found.map.name(), map, "{name}/{position}");
        }
    }

    #[test]
    fn find_tells_apart_both_maps_of_a_split_city() {
        let catalog = CityMapCatalog::new();
        for (name, first, second) in [
            ("MAPSEC_SLATEPORT_CITY", "Slateport_0", "Slateport_1"),
            ("MAPSEC_MAUVILLE_CITY", "Mauville_0", "Mauville_1"),
            ("MAPSEC_RUSTBORO_CITY", "Rustboro_0", "Rustboro_1"),
            ("MAPSEC_LILYCOVE_CITY", "Lilycove_0", "Lilycove_1"),
            ("MAPSEC_MOSSDEEP_CITY", "Mossdeep_0", "Mossdeep_1"),
            ("MAPSEC_EVER_GRANDE_CITY", "EverGrande_0", "EverGrande_1"),
        ] {
            let at = |p| catalog.find(section(name), p).map(|e| e.map.name());
            assert_eq!(at(0), Some(format!("gPokenavCityMap_{first}").as_str()));
            assert_eq!(at(1), Some(format!("gPokenavCityMap_{second}").as_str()));
        }
    }

    #[test]
    fn find_refuses_pairs_with_no_entry() {
        let catalog = CityMapCatalog::new();
        // Unsplit city with a second position.
        assert_eq!(catalog.find(section("MAPSEC_LITTLEROOT_TOWN"), 1), None);
        assert_eq!(catalog.find(section("MAPSEC_FORTREE_CITY"), 1), None);
        // Split city past its last position.
        assert_eq!(catalog.find(section("MAPSEC_SLATEPORT_CITY"), 2), None);
        assert_eq!(
            catalog.find(section("MAPSEC_EVER_GRANDE_CITY"), u8::MAX),
            None
        );
        // A section with no city map, and one that is not a section at all.
        assert_eq!(catalog.find(section("MAPSEC_ROUTE_101"), 0), None);
        assert_eq!(catalog.find(section("Slateport"), 0), None);
    }

    #[test]
    fn every_entry_section_is_used_by_a_map_header() {
        use crate::map_headers::MapHeaderTable;
        let headers = MapHeaderTable::new();
        for e in CityMapCatalog::new().entries() {
            assert!(
                headers.iter().any(|h| h.region_map_section == e.section),
                "{} names no map header section",
                e.section.name()
            );
        }
    }
}
