//! The Hoenn region-map section grid: the 28 by 15 table that maps a
//! region-map cursor cell to the map section (`MAPSEC_*`) drawn there.
//!
//! Source: `sRegionMap_MapSectionLayout` (`src/data/region_map/region_map_layout.h`),
//! sized by `MAP_WIDTH` (28) and `MAP_HEIGHT` (15) in `src/region_map.c` and
//! read there as `[y][x]`, so row `y` holds the `x` columns. Each cell is a
//! [`RegionMapSectionId`] from [`crate::map_headers`]; a cell with no location
//! holds [`MAPSEC_NONE`]. Data only: the region-map screen, cursor movement,
//! zoom, the Fly map, map-section names, the player-position correction for
//! sub-areas, and region-map graphics are not ported here.
//!
//! The grid is indexed from `0`. Upstream's cursor is offset by
//! `MAPCURSOR_X_MIN`/`MAPCURSOR_Y_MIN` before it reads the grid
//! (`GetMapSecIdAt`); that offset belongs to the cursor and is not applied here.

use std::error::Error;
use std::fmt;

use crate::map_headers::RegionMapSectionId;

/// The number of columns in the grid (`MAP_WIDTH`).
pub const REGION_MAP_WIDTH: usize = 28;

/// The number of rows in the grid (`MAP_HEIGHT`).
pub const REGION_MAP_HEIGHT: usize = 15;

/// The map section of a cell with no location (`MAPSEC_NONE`).
pub const MAPSEC_NONE: RegionMapSectionId = RegionMapSectionId("MAPSEC_NONE");

/// A failed lookup in the region-map section grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionMapLayoutError {
    /// The column is not below [`REGION_MAP_WIDTH`] or the row is not below
    /// [`REGION_MAP_HEIGHT`].
    OutOfRange {
        /// The requested column (`x`).
        column: usize,
        /// The requested row (`y`).
        row: usize,
    },
}

impl fmt::Display for RegionMapLayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfRange { column, row } => write!(
                f,
                "region-map cell (column {column}, row {row}) is outside the \
                 {REGION_MAP_WIDTH} by {REGION_MAP_HEIGHT} grid"
            ),
        }
    }
}

impl Error for RegionMapLayoutError {}

/// Builds a row of [`RegionMapSectionId`]s from the part of each `MAPSEC_*`
/// name after the prefix.
macro_rules! sections {
    ($($name:ident),+ $(,)?) => {
        [$(RegionMapSectionId(concat!("MAPSEC_", stringify!($name)))),+]
    };
}

/// `sRegionMap_MapSectionLayout`, one array per row (`[y][x]`).
#[rustfmt::skip]
static MAP_SECTION_LAYOUT: [[RegionMapSectionId; REGION_MAP_WIDTH]; REGION_MAP_HEIGHT] = [
    sections![NONE, ROUTE_114, ROUTE_114, FALLARBOR_TOWN, ROUTE_113, ROUTE_113, ROUTE_113, ROUTE_113, ROUTE_111, NONE, NONE, ROUTE_119, FORTREE_CITY, ROUTE_120, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE],
    sections![NONE, ROUTE_114, NONE, NONE, NONE, NONE, MT_CHIMNEY, MT_CHIMNEY, ROUTE_111, NONE, NONE, ROUTE_119, NONE, ROUTE_120, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE],
    sections![ROUTE_115, ROUTE_114, NONE, NONE, NONE, NONE, MT_CHIMNEY, MT_CHIMNEY, ROUTE_111, NONE, NONE, ROUTE_119, NONE, ROUTE_120, NONE, NONE, SAFARI_ZONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE],
    sections![ROUTE_115, NONE, NONE, NONE, NONE, LAVARIDGE_TOWN, ROUTE_112, ROUTE_112, ROUTE_111, NONE, NONE, ROUTE_119, NONE, ROUTE_120, ROUTE_121, ROUTE_121, ROUTE_121, ROUTE_121, LILYCOVE_CITY, LILYCOVE_CITY, ROUTE_124, ROUTE_124, ROUTE_124, ROUTE_124, ROUTE_125, ROUTE_125, NONE, NONE],
    sections![ROUTE_115, NONE, NONE, NONE, NONE, NONE, NONE, NONE, ROUTE_111, NONE, NONE, ROUTE_119, NONE, NONE, NONE, NONE, ROUTE_122, NONE, NONE, NONE, ROUTE_124, ROUTE_124, ROUTE_124, ROUTE_124, ROUTE_125, ROUTE_125, NONE, NONE],
    sections![RUSTBORO_CITY, ROUTE_116, ROUTE_116, ROUTE_116, ROUTE_116, NONE, NONE, NONE, ROUTE_111, NONE, NONE, ROUTE_119, NONE, NONE, NONE, NONE, ROUTE_122, NONE, NONE, NONE, ROUTE_124, ROUTE_124, ROUTE_124, ROUTE_124, MOSSDEEP_CITY, MOSSDEEP_CITY, NONE, NONE],
    sections![RUSTBORO_CITY, NONE, NONE, NONE, VERDANTURF_TOWN, ROUTE_117, ROUTE_117, ROUTE_117, MAUVILLE_CITY, MAUVILLE_CITY, ROUTE_118, ROUTE_118, ROUTE_123, ROUTE_123, ROUTE_123, ROUTE_123, ROUTE_123, NONE, NONE, NONE, ROUTE_126, ROUTE_126, ROUTE_126, ROUTE_127, ROUTE_127, ROUTE_127, NONE, NONE],
    sections![ROUTE_104, NONE, NONE, NONE, NONE, NONE, NONE, NONE, ROUTE_110, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, ROUTE_126, SOOTOPOLIS_CITY, ROUTE_126, ROUTE_127, ROUTE_127, ROUTE_127, NONE, NONE],
    sections![ROUTE_104, NONE, NONE, NONE, ROUTE_103, ROUTE_103, ROUTE_103, ROUTE_103, ROUTE_110, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, ROUTE_126, ROUTE_126, ROUTE_126, ROUTE_127, ROUTE_127, ROUTE_127, NONE, EVER_GRANDE_CITY],
    sections![ROUTE_104, PETALBURG_CITY, ROUTE_102, ROUTE_102, OLDALE_TOWN, NONE, NONE, NONE, ROUTE_110, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, ROUTE_128, ROUTE_128, ROUTE_128, ROUTE_128, EVER_GRANDE_CITY],
    sections![ROUTE_105, NONE, NONE, NONE, ROUTE_101, NONE, NONE, NONE, SLATEPORT_CITY, ROUTE_134, ROUTE_134, ROUTE_134, ROUTE_133, ROUTE_133, ROUTE_133, ROUTE_132, ROUTE_132, PACIFIDLOG_TOWN, ROUTE_131, ROUTE_131, ROUTE_131, ROUTE_130, ROUTE_130, ROUTE_130, ROUTE_129, ROUTE_129, NONE, NONE],
    sections![ROUTE_105, NONE, NONE, NONE, LITTLEROOT_TOWN, NONE, NONE, NONE, SLATEPORT_CITY, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE],
    sections![ROUTE_105, NONE, NONE, NONE, NONE, NONE, NONE, NONE, ROUTE_109, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, BATTLE_FRONTIER, NONE, NONE, NONE, NONE, NONE],
    sections![ROUTE_106, ROUTE_106, ROUTE_106, NONE, NONE, NONE, NONE, NONE, ROUTE_109, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE],
    sections![NONE, NONE, DEWFORD_TOWN, ROUTE_107, ROUTE_107, ROUTE_107, ROUTE_108, ROUTE_108, ROUTE_109, NONE, NONE, NONE, SOUTHERN_ISLAND, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE],
];

/// Returns the map section at `column` (`x`) and `row` (`y`), in the upstream
/// `[y][x]` order.
///
/// A cell with no location returns [`MAPSEC_NONE`].
///
/// # Errors
///
/// Returns [`RegionMapLayoutError::OutOfRange`] when `column` is not below
/// [`REGION_MAP_WIDTH`] or `row` is not below [`REGION_MAP_HEIGHT`].
pub fn map_section_at(
    column: usize,
    row: usize,
) -> Result<RegionMapSectionId, RegionMapLayoutError> {
    MAP_SECTION_LAYOUT
        .get(row)
        .and_then(|cells| cells.get(column))
        .copied()
        .ok_or(RegionMapLayoutError::OutOfRange { column, row })
}

/// Returns the whole grid, one array per row (`[y][x]`).
#[must_use]
pub fn map_section_layout() -> &'static [[RegionMapSectionId; REGION_MAP_WIDTH]; REGION_MAP_HEIGHT]
{
    &MAP_SECTION_LAYOUT
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `sRegionMap_MapSectionLayout`, row by row, as the full `MAPSEC_*` names
    /// listed in `src/data/region_map/region_map_layout.h`.
    #[rustfmt::skip]
    const EXPECTED: [[&str; 28]; 15] = [
        [
            "MAPSEC_NONE", "MAPSEC_ROUTE_114", "MAPSEC_ROUTE_114", "MAPSEC_FALLARBOR_TOWN",
            "MAPSEC_ROUTE_113", "MAPSEC_ROUTE_113", "MAPSEC_ROUTE_113", "MAPSEC_ROUTE_113",
            "MAPSEC_ROUTE_111", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_ROUTE_119",
            "MAPSEC_FORTREE_CITY", "MAPSEC_ROUTE_120", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
        ],
        [
            "MAPSEC_NONE", "MAPSEC_ROUTE_114", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_MT_CHIMNEY", "MAPSEC_MT_CHIMNEY",
            "MAPSEC_ROUTE_111", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_ROUTE_119",
            "MAPSEC_NONE", "MAPSEC_ROUTE_120", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
        ],
        [
            "MAPSEC_ROUTE_115", "MAPSEC_ROUTE_114", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_MT_CHIMNEY", "MAPSEC_MT_CHIMNEY",
            "MAPSEC_ROUTE_111", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_ROUTE_119",
            "MAPSEC_NONE", "MAPSEC_ROUTE_120", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_SAFARI_ZONE", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
        ],
        [
            "MAPSEC_ROUTE_115", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_NONE", "MAPSEC_LAVARIDGE_TOWN", "MAPSEC_ROUTE_112", "MAPSEC_ROUTE_112",
            "MAPSEC_ROUTE_111", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_ROUTE_119",
            "MAPSEC_NONE", "MAPSEC_ROUTE_120", "MAPSEC_ROUTE_121", "MAPSEC_ROUTE_121",
            "MAPSEC_ROUTE_121", "MAPSEC_ROUTE_121", "MAPSEC_LILYCOVE_CITY", "MAPSEC_LILYCOVE_CITY",
            "MAPSEC_ROUTE_124", "MAPSEC_ROUTE_124", "MAPSEC_ROUTE_124", "MAPSEC_ROUTE_124",
            "MAPSEC_ROUTE_125", "MAPSEC_ROUTE_125", "MAPSEC_NONE", "MAPSEC_NONE",
        ],
        [
            "MAPSEC_ROUTE_115", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_ROUTE_111", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_ROUTE_119",
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_ROUTE_122", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_ROUTE_124", "MAPSEC_ROUTE_124", "MAPSEC_ROUTE_124", "MAPSEC_ROUTE_124",
            "MAPSEC_ROUTE_125", "MAPSEC_ROUTE_125", "MAPSEC_NONE", "MAPSEC_NONE",
        ],
        [
            "MAPSEC_RUSTBORO_CITY", "MAPSEC_ROUTE_116", "MAPSEC_ROUTE_116", "MAPSEC_ROUTE_116",
            "MAPSEC_ROUTE_116", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_ROUTE_111", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_ROUTE_119",
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_ROUTE_122", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_ROUTE_124", "MAPSEC_ROUTE_124", "MAPSEC_ROUTE_124", "MAPSEC_ROUTE_124",
            "MAPSEC_MOSSDEEP_CITY", "MAPSEC_MOSSDEEP_CITY", "MAPSEC_NONE", "MAPSEC_NONE",
        ],
        [
            "MAPSEC_RUSTBORO_CITY", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_VERDANTURF_TOWN", "MAPSEC_ROUTE_117", "MAPSEC_ROUTE_117", "MAPSEC_ROUTE_117",
            "MAPSEC_MAUVILLE_CITY", "MAPSEC_MAUVILLE_CITY", "MAPSEC_ROUTE_118", "MAPSEC_ROUTE_118",
            "MAPSEC_ROUTE_123", "MAPSEC_ROUTE_123", "MAPSEC_ROUTE_123", "MAPSEC_ROUTE_123",
            "MAPSEC_ROUTE_123", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_ROUTE_126", "MAPSEC_ROUTE_126", "MAPSEC_ROUTE_126", "MAPSEC_ROUTE_127",
            "MAPSEC_ROUTE_127", "MAPSEC_ROUTE_127", "MAPSEC_NONE", "MAPSEC_NONE",
        ],
        [
            "MAPSEC_ROUTE_104", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_ROUTE_110", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_ROUTE_126", "MAPSEC_SOOTOPOLIS_CITY", "MAPSEC_ROUTE_126", "MAPSEC_ROUTE_127",
            "MAPSEC_ROUTE_127", "MAPSEC_ROUTE_127", "MAPSEC_NONE", "MAPSEC_NONE",
        ],
        [
            "MAPSEC_ROUTE_104", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_ROUTE_103", "MAPSEC_ROUTE_103", "MAPSEC_ROUTE_103", "MAPSEC_ROUTE_103",
            "MAPSEC_ROUTE_110", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_ROUTE_126", "MAPSEC_ROUTE_126", "MAPSEC_ROUTE_126", "MAPSEC_ROUTE_127",
            "MAPSEC_ROUTE_127", "MAPSEC_ROUTE_127", "MAPSEC_NONE", "MAPSEC_EVER_GRANDE_CITY",
        ],
        [
            "MAPSEC_ROUTE_104", "MAPSEC_PETALBURG_CITY", "MAPSEC_ROUTE_102", "MAPSEC_ROUTE_102",
            "MAPSEC_OLDALE_TOWN", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_ROUTE_110", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_ROUTE_128",
            "MAPSEC_ROUTE_128", "MAPSEC_ROUTE_128", "MAPSEC_ROUTE_128", "MAPSEC_EVER_GRANDE_CITY",
        ],
        [
            "MAPSEC_ROUTE_105", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_ROUTE_101", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_SLATEPORT_CITY", "MAPSEC_ROUTE_134", "MAPSEC_ROUTE_134", "MAPSEC_ROUTE_134",
            "MAPSEC_ROUTE_133", "MAPSEC_ROUTE_133", "MAPSEC_ROUTE_133", "MAPSEC_ROUTE_132",
            "MAPSEC_ROUTE_132", "MAPSEC_PACIFIDLOG_TOWN", "MAPSEC_ROUTE_131", "MAPSEC_ROUTE_131",
            "MAPSEC_ROUTE_131", "MAPSEC_ROUTE_130", "MAPSEC_ROUTE_130", "MAPSEC_ROUTE_130",
            "MAPSEC_ROUTE_129", "MAPSEC_ROUTE_129", "MAPSEC_NONE", "MAPSEC_NONE",
        ],
        [
            "MAPSEC_ROUTE_105", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_LITTLEROOT_TOWN", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_SLATEPORT_CITY", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
        ],
        [
            "MAPSEC_ROUTE_105", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_ROUTE_109", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_BATTLE_FRONTIER", "MAPSEC_NONE",
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
        ],
        [
            "MAPSEC_ROUTE_106", "MAPSEC_ROUTE_106", "MAPSEC_ROUTE_106", "MAPSEC_NONE",
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_ROUTE_109", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
        ],
        [
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_DEWFORD_TOWN", "MAPSEC_ROUTE_107",
            "MAPSEC_ROUTE_107", "MAPSEC_ROUTE_107", "MAPSEC_ROUTE_108", "MAPSEC_ROUTE_108",
            "MAPSEC_ROUTE_109", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_SOUTHERN_ISLAND", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
            "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE", "MAPSEC_NONE",
        ],
    ];

    #[test]
    fn dimensions_match_upstream() {
        assert_eq!(REGION_MAP_WIDTH, 28);
        assert_eq!(REGION_MAP_HEIGHT, 15);
        assert_eq!(map_section_layout().len(), 15);
        assert!(map_section_layout().iter().all(|row| row.len() == 28));
        assert_eq!(EXPECTED.len(), 15);
        assert!(EXPECTED.iter().all(|row| row.len() == 28));
    }

    #[test]
    fn every_cell_matches_upstream() {
        let mut checked = 0;
        for (row, expected_row) in EXPECTED.iter().enumerate() {
            for (column, expected) in expected_row.iter().enumerate() {
                let section = map_section_at(column, row).expect("cell is inside the grid");
                assert_eq!(
                    section.name(),
                    *expected,
                    "cell at column {column}, row {row}"
                );
                checked += 1;
            }
        }
        assert_eq!(checked, 420);
    }

    #[test]
    fn whole_grid_matches_upstream_row_by_row() {
        for (row, (actual_row, expected_row)) in
            map_section_layout().iter().zip(EXPECTED.iter()).enumerate()
        {
            let names: Vec<&str> = actual_row.iter().map(|section| section.name()).collect();
            assert_eq!(names, expected_row.to_vec(), "row {row}");
        }
    }

    #[test]
    fn lookup_takes_column_then_row() {
        // The grid is read as `[y][x]`: row 0 starts MAPSEC_NONE, MAPSEC_ROUTE_114,
        // while column 0 of row 2 is MAPSEC_ROUTE_115.
        assert_eq!(map_section_at(1, 0).unwrap().name(), "MAPSEC_ROUTE_114");
        assert_eq!(map_section_at(0, 2).unwrap().name(), "MAPSEC_ROUTE_115");
        assert_eq!(
            map_section_at(27, 8).unwrap().name(),
            "MAPSEC_EVER_GRANDE_CITY"
        );
        assert_eq!(map_section_at(8, 14).unwrap().name(), "MAPSEC_ROUTE_109");
        assert_eq!(
            map_section_at(12, 14).unwrap().name(),
            "MAPSEC_SOUTHERN_ISLAND"
        );
    }

    #[test]
    fn empty_cells_return_mapsec_none() {
        assert_eq!(MAPSEC_NONE.name(), "MAPSEC_NONE");
        assert_eq!(map_section_at(0, 0), Ok(MAPSEC_NONE));
        assert_eq!(map_section_at(27, 14), Ok(MAPSEC_NONE));

        let expected_none = EXPECTED
            .iter()
            .flatten()
            .filter(|name| **name == "MAPSEC_NONE")
            .count();
        let actual_none = map_section_layout()
            .iter()
            .flatten()
            .filter(|section| **section == MAPSEC_NONE)
            .count();
        assert_eq!(actual_none, expected_none);
        assert!(actual_none > 0 && actual_none < 420);
    }

    #[test]
    fn named_sections_are_distinct_from_mapsec_none() {
        for (row, expected_row) in EXPECTED.iter().enumerate() {
            for (column, expected) in expected_row.iter().enumerate() {
                let section = map_section_at(column, row).unwrap();
                assert_eq!(
                    section == MAPSEC_NONE,
                    *expected == "MAPSEC_NONE",
                    "cell at column {column}, row {row}"
                );
            }
        }
    }

    #[test]
    fn lookup_rejects_coordinates_outside_the_grid() {
        for (column, row) in [
            (28, 0),
            (0, 15),
            (28, 15),
            (usize::MAX, 0),
            (0, usize::MAX),
            (15, 15),
        ] {
            assert_eq!(
                map_section_at(column, row),
                Err(RegionMapLayoutError::OutOfRange { column, row }),
                "column {column}, row {row}"
            );
        }
        // A cell valid only with the axes swapped is still rejected.
        assert!(map_section_at(5, 20).is_err());
        assert!(map_section_at(20, 5).is_ok());
    }

    #[test]
    fn error_names_the_rejected_coordinates() {
        let error = map_section_at(28, 3).unwrap_err();
        assert_eq!(
            error.to_string(),
            "region-map cell (column 28, row 3) is outside the 28 by 15 grid"
        );
    }
}
