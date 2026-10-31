//! The overworld layouts and tilesets the extraction pipeline bundles.
//!
//! `cargo xtask extract` and the committed ROM profile write map grids and
//! tileset sheets only for the layouts and tilesets named here, so anything
//! that walks, draws or counts "bundled" maps selects through this module
//! instead of keeping its own list. `xtask` pins both tables against its
//! extraction manifest, which makes a manifest change fail a test until this
//! module follows it.

use crate::map_headers::MapHeaderTable;
use crate::map_layouts::LayoutId;
use crate::wild_encounters::MapId;

/// A tileset the extraction pipeline bundles, with the pack name it is
/// stored under (`tileset/<pack_name>/...`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BundledTileset {
    /// The `gTileset_*` symbol a [`MapLayout`](crate::map_layouts::MapLayout)
    /// names it by.
    pub upstream_symbol: &'static str,
    /// The tileset's name inside the asset pack.
    pub pack_name: &'static str,
}

/// Every layout whose map and border grids are bundled, in extraction order.
pub const BUNDLED_LAYOUT_IDS: &[LayoutId] = &[
    LayoutId("LAYOUT_LITTLEROOT_TOWN"),
    LayoutId("LAYOUT_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F"),
    LayoutId("LAYOUT_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F"),
    LayoutId("LAYOUT_LITTLEROOT_TOWN_MAYS_HOUSE_1F"),
    LayoutId("LAYOUT_LITTLEROOT_TOWN_MAYS_HOUSE_2F"),
    LayoutId("LAYOUT_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB"),
    LayoutId("LAYOUT_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB_WITH_TABLE"),
    LayoutId("LAYOUT_ROUTE101"),
    LayoutId("LAYOUT_OLDALE_TOWN"),
    LayoutId("LAYOUT_ROUTE103"),
    LayoutId("LAYOUT_INSIDE_OF_TRUCK"),
    LayoutId("LAYOUT_HOUSE1"),
    LayoutId("LAYOUT_HOUSE2"),
    LayoutId("LAYOUT_MART"),
    LayoutId("LAYOUT_POKEMON_CENTER_1F"),
    LayoutId("LAYOUT_POKEMON_CENTER_2F"),
];

/// Every tileset whose sheets and metatile tables are bundled.
pub const BUNDLED_TILESETS: &[BundledTileset] = &[
    BundledTileset {
        upstream_symbol: "gTileset_General",
        pack_name: "general",
    },
    BundledTileset {
        upstream_symbol: "gTileset_Building",
        pack_name: "building",
    },
    BundledTileset {
        upstream_symbol: "gTileset_Petalburg",
        pack_name: "petalburg",
    },
    BundledTileset {
        upstream_symbol: "gTileset_BrendansMaysHouse",
        pack_name: "brendans_mays_house",
    },
    BundledTileset {
        upstream_symbol: "gTileset_Lab",
        pack_name: "lab",
    },
    BundledTileset {
        upstream_symbol: "gTileset_GenericBuilding",
        pack_name: "generic_building",
    },
    BundledTileset {
        upstream_symbol: "gTileset_PokemonCenter",
        pack_name: "pokemon_center",
    },
    BundledTileset {
        upstream_symbol: "gTileset_Shop",
        pack_name: "shop",
    },
    BundledTileset {
        upstream_symbol: "gTileset_InsideOfTruck",
        pack_name: "inside_of_truck",
    },
];

/// Returns the pack name of the bundled tileset `upstream_symbol` names, or
/// `None` for a tileset that is not bundled.
#[must_use]
pub fn bundled_tileset_pack_name(upstream_symbol: &str) -> Option<&'static str> {
    BUNDLED_TILESETS
        .iter()
        .find(|tileset| tileset.upstream_symbol == upstream_symbol)
        .map(|tileset| tileset.pack_name)
}

/// Returns every map whose header names a bundled layout, in header order.
///
/// A bundled layout is shared by every map that reuses it (each town's Pokémon
/// Center, for one), so this is the set of maps the bundled grids can build,
/// not the set of maps a scenario reaches.
#[must_use]
pub fn maps_with_bundled_layouts() -> Vec<MapId> {
    MapHeaderTable::new()
        .iter()
        .filter(|header| BUNDLED_LAYOUT_IDS.contains(&header.layout))
        .map(|header| header.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_layouts::LayoutTable;
    use std::collections::BTreeSet;

    #[test]
    fn bundled_layouts_are_unique_known_layouts() {
        let unique: BTreeSet<_> = BUNDLED_LAYOUT_IDS.iter().map(|id| id.name()).collect();
        assert_eq!(unique.len(), BUNDLED_LAYOUT_IDS.len(), "duplicate layout");
        let table = LayoutTable::new();
        for id in BUNDLED_LAYOUT_IDS {
            assert!(table.layout(*id).is_ok(), "{id:?} is not a known layout");
        }
    }

    #[test]
    fn bundled_tilesets_have_unique_symbols_and_pack_names() {
        let symbols: BTreeSet<_> = BUNDLED_TILESETS.iter().map(|t| t.upstream_symbol).collect();
        let names: BTreeSet<_> = BUNDLED_TILESETS.iter().map(|t| t.pack_name).collect();
        assert_eq!(symbols.len(), BUNDLED_TILESETS.len(), "duplicate symbol");
        assert_eq!(names.len(), BUNDLED_TILESETS.len(), "duplicate pack name");
    }

    #[test]
    fn bundled_tilesets_are_exactly_those_the_bundled_layouts_use() {
        let table = LayoutTable::new();
        let used: BTreeSet<&str> = BUNDLED_LAYOUT_IDS
            .iter()
            .flat_map(|id| {
                let layout = table.layout(*id).unwrap();
                [layout.primary_tileset, layout.secondary_tileset]
            })
            .collect();
        let bundled: BTreeSet<&str> = BUNDLED_TILESETS.iter().map(|t| t.upstream_symbol).collect();
        assert_eq!(bundled, used);
    }

    #[test]
    fn the_bundled_layouts_build_seventy_six_maps() {
        let maps = maps_with_bundled_layouts();
        // The lab's `_WITH_TABLE` layout has no header of its own: it is an
        // alternate layout for a map already counted.
        assert_eq!(maps.len(), 76);
        assert!(maps.contains(&MapId("MAP_INSIDE_OF_TRUCK")));
        assert!(maps.contains(&MapId("MAP_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB")));
    }

    #[test]
    fn an_unbundled_tileset_has_no_pack_name() {
        assert_eq!(bundled_tileset_pack_name("gTileset_Underwater"), None);
        assert_eq!(bundled_tileset_pack_name("gTileset_Shop"), Some("shop"),);
    }
}
