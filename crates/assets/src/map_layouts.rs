//! Typed map-layout metadata and decoded metatile grids.
//!
//! [`MapLayout`] stores canonical dimensions and tileset identities while map
//! and border bytes remain in the caller-owned asset pack under Discussion
//! #71 policy A. [`LayoutId`] retains the `LAYOUT_*` name from
//! `data/layouts/layouts.json`; this reference checkout has no
//! `constants/layouts.h` from which to recover its numeric table position.
//!
//! Grid and border cells are row-major little-endian `u16` values with the
//! fields defined by `include/global.fieldmap.h`. Several unused layouts and
//! `LAYOUT_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB_WITH_TABLE` contain trailing
//! `map.bin` padding beyond their declared dimensions. [`LayoutGrid`] accepts
//! that surplus but exposes only the declared cells.
//!
//! [`BorderGrid::cell_at`] preserves `src/fieldmap.c`'s `GetBorderBlockAt`
//! rule: each world coordinate is shifted by one and masked into the repeating
//! two-by-two border. Wrapping the shift keeps the same parity at `i32` limits.

use crate::error::AssetError;
use std::mem::size_of;

const BYTES_PER_METATILE_CELL: usize = size_of::<u16>();
const METATILE_ID_MASK: u16 = 0x03FF;
const COLLISION_MASK: u16 = 0b11;
const COLLISION_SHIFT: u32 = 10;
const ELEVATION_MASK: u16 = 0b1111;
const ELEVATION_SHIFT: u32 = 12;
const NO_TILESET: &str = "0";
const BORDER_REPEAT_OFFSET: i32 = 1;
const BORDER_COORDINATE_MASK: i32 = 1;

/// Number of canonical map layouts.
pub const LAYOUT_COUNT: usize = 441;

/// Width of every repeating border grid, in cells.
pub const BORDER_WIDTH: usize = 2;
/// Height of every repeating border grid, in cells.
pub const BORDER_HEIGHT: usize = 2;
/// Number of cells in every border grid.
pub const BORDER_CELLS: usize = BORDER_WIDTH * BORDER_HEIGHT;
const BORDER_BYTE_LEN: usize = BORDER_CELLS * BYTES_PER_METATILE_CELL;

/// A symbolic `LAYOUT_*` map-layout identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LayoutId(pub &'static str);

impl LayoutId {
    /// Returns the symbolic `LAYOUT_*` name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        self.0
    }
}

/// A decoded cell from a packed map or border grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetatileCell {
    /// Metatile identity stored in bits 0 through 9.
    pub metatile_id: u16,
    /// Collision value stored in bits 10 and 11. Zero is passable; other
    /// values obstruct movement.
    pub collision: u8,
    /// Elevation value stored in bits 12 through 15.
    pub elevation: u8,
}

impl MetatileCell {
    /// Decodes one packed grid value.
    #[must_use]
    pub const fn from_raw(raw: u16) -> Self {
        Self {
            metatile_id: raw & METATILE_ID_MASK,
            collision: ((raw >> COLLISION_SHIFT) & COLLISION_MASK) as u8,
            elevation: ((raw >> ELEVATION_SHIFT) & ELEVATION_MASK) as u8,
        }
    }

    /// Packs the cell into its 16-bit grid representation.
    #[must_use]
    pub const fn pack(self) -> u16 {
        (self.metatile_id & METATILE_ID_MASK)
            | (((self.collision as u16) & COLLISION_MASK) << COLLISION_SHIFT)
            | (((self.elevation as u16) & ELEVATION_MASK) << ELEVATION_SHIFT)
    }
}

/// Canonical metadata for one map layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapLayout {
    /// Symbolic layout identity.
    pub id: LayoutId,
    /// Unique Porymap layout label, distinct from a display name.
    pub name: &'static str,
    /// Grid width in metatiles.
    pub width: u16,
    /// Grid height in metatiles.
    pub height: u16,
    /// Symbolic primary tileset identity.
    pub primary_tileset: &'static str,
    /// Symbolic secondary tileset identity.
    pub secondary_tileset: &'static str,
}

impl MapLayout {
    /// Returns the grid's cell count.
    #[must_use]
    pub fn cell_count(&self) -> usize {
        usize::from(self.width) * usize::from(self.height)
    }

    /// Decodes caller-supplied grid bytes for this layout.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::LayoutGridTooShort`] when the buffer does not
    /// contain every declared cell.
    pub fn grid<'a>(&self, bytes: &'a [u8]) -> Result<LayoutGrid<'a>, AssetError> {
        LayoutGrid::new(self, bytes)
    }
}

/// A validated, borrowed view of a row-major little-endian metatile grid.
#[derive(Debug, Clone, Copy)]
pub struct LayoutGrid<'a> {
    width: u16,
    height: u16,
    bytes: &'a [u8],
}

impl<'a> LayoutGrid<'a> {
    /// Builds a grid over the cells declared by `layout`.
    ///
    /// Trailing bytes are accepted but remain outside the grid.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::LayoutGridTooShort`] when the buffer does not
    /// contain every declared cell.
    pub fn new(layout: &MapLayout, bytes: &'a [u8]) -> Result<Self, AssetError> {
        let expected = layout.cell_count() * BYTES_PER_METATILE_CELL;
        if bytes.len() < expected {
            return Err(AssetError::LayoutGridTooShort(
                layout.id.0,
                expected,
                bytes.len(),
            ));
        }
        Ok(Self {
            width: layout.width,
            height: layout.height,
            bytes,
        })
    }

    /// Returns the grid's cell count.
    #[must_use]
    pub fn cell_count(&self) -> usize {
        usize::from(self.width) * usize::from(self.height)
    }

    /// Returns the grid width in metatiles.
    #[must_use]
    pub const fn width(&self) -> u16 {
        self.width
    }

    /// Returns the grid height in metatiles.
    #[must_use]
    pub const fn height(&self) -> u16 {
        self.height
    }

    /// Returns the decoded cell at `(x, y)`, or `None` when out of bounds.
    #[must_use]
    pub fn cell_at(&self, x: u16, y: u16) -> Option<MetatileCell> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let index = usize::from(y) * usize::from(self.width) + usize::from(x);
        let offset = index * BYTES_PER_METATILE_CELL;
        let bytes = self.bytes.get(offset..offset + BYTES_PER_METATILE_CELL)?;
        Some(decode_metatile_cell(bytes))
    }

    /// Iterates over the declared cells in row-major order.
    pub fn cells(&self) -> impl Iterator<Item = MetatileCell> + '_ {
        self.bytes
            .chunks_exact(BYTES_PER_METATILE_CELL)
            .take(self.cell_count())
            .map(decode_metatile_cell)
    }
}

/// A validated, borrowed view of a repeating two-dimensional border grid.
#[derive(Debug, Clone, Copy)]
pub struct BorderGrid<'a> {
    bytes: &'a [u8],
}

impl<'a> BorderGrid<'a> {
    /// Builds a border grid from its exact encoded byte length.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::LayoutBorderWrongSize`] when the buffer length
    /// differs from [`BORDER_CELLS`] packed values.
    pub fn new(bytes: &'a [u8]) -> Result<Self, AssetError> {
        if bytes.len() != BORDER_BYTE_LEN {
            return Err(AssetError::LayoutBorderWrongSize(bytes.len()));
        }
        Ok(Self { bytes })
    }

    /// Returns the border cell repeated over world position `(x, y)`.
    #[must_use]
    pub fn cell_at(&self, x: i32, y: i32) -> MetatileCell {
        let column = repeating_border_coordinate(x);
        let row = repeating_border_coordinate(y);
        let offset = (row * BORDER_WIDTH + column) * BYTES_PER_METATILE_CELL;
        decode_metatile_cell(&self.bytes[offset..offset + BYTES_PER_METATILE_CELL])
    }

    /// Iterates over the border cells in row-major order.
    pub fn cells(&self) -> impl Iterator<Item = MetatileCell> + '_ {
        self.bytes
            .chunks_exact(BYTES_PER_METATILE_CELL)
            .map(decode_metatile_cell)
    }
}

fn repeating_border_coordinate(world_coordinate: i32) -> usize {
    usize::try_from(world_coordinate.wrapping_add(BORDER_REPEAT_OFFSET) & BORDER_COORDINATE_MASK)
        .expect("repeating border coordinate is zero or one")
}

fn decode_metatile_cell(bytes: &[u8]) -> MetatileCell {
    MetatileCell::from_raw(u16::from_le_bytes([bytes[0], bytes[1]]))
}

static LAYOUTS: [MapLayout; LAYOUT_COUNT] = [
    MapLayout {
        id: LayoutId("LAYOUT_PETALBURG_CITY"),
        name: "PetalburgCity_Layout",
        width: 30,
        height: 30,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Petalburg",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SLATEPORT_CITY"),
        name: "SlateportCity_Layout",
        width: 40,
        height: 60,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Slateport",
    },
    MapLayout {
        id: LayoutId("LAYOUT_MAUVILLE_CITY"),
        name: "MauvilleCity_Layout",
        width: 40,
        height: 20,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Mauville",
    },
    MapLayout {
        id: LayoutId("LAYOUT_RUSTBORO_CITY"),
        name: "RustboroCity_Layout",
        width: 40,
        height: 60,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Rustboro",
    },
    MapLayout {
        id: LayoutId("LAYOUT_FORTREE_CITY"),
        name: "FortreeCity_Layout",
        width: 40,
        height: 20,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Fortree",
    },
    MapLayout {
        id: LayoutId("LAYOUT_LILYCOVE_CITY"),
        name: "LilycoveCity_Layout",
        width: 80,
        height: 40,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Lilycove",
    },
    MapLayout {
        id: LayoutId("LAYOUT_MOSSDEEP_CITY"),
        name: "MossdeepCity_Layout",
        width: 80,
        height: 40,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Mossdeep",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SOOTOPOLIS_CITY"),
        name: "SootopolisCity_Layout",
        width: 60,
        height: 60,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Sootopolis",
    },
    MapLayout {
        id: LayoutId("LAYOUT_EVER_GRANDE_CITY"),
        name: "EverGrandeCity_Layout",
        width: 40,
        height: 80,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_EverGrande",
    },
    MapLayout {
        id: LayoutId("LAYOUT_LITTLEROOT_TOWN"),
        name: "LittlerootTown_Layout",
        width: 20,
        height: 20,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Petalburg",
    },
    MapLayout {
        id: LayoutId("LAYOUT_OLDALE_TOWN"),
        name: "OldaleTown_Layout",
        width: 20,
        height: 20,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Petalburg",
    },
    MapLayout {
        id: LayoutId("LAYOUT_DEWFORD_TOWN"),
        name: "DewfordTown_Layout",
        width: 20,
        height: 20,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Dewford",
    },
    MapLayout {
        id: LayoutId("LAYOUT_LAVARIDGE_TOWN"),
        name: "LavaridgeTown_Layout",
        width: 20,
        height: 20,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Lavaridge",
    },
    MapLayout {
        id: LayoutId("LAYOUT_FALLARBOR_TOWN"),
        name: "FallarborTown_Layout",
        width: 20,
        height: 20,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Fallarbor",
    },
    MapLayout {
        id: LayoutId("LAYOUT_VERDANTURF_TOWN"),
        name: "VerdanturfTown_Layout",
        width: 20,
        height: 20,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Mauville",
    },
    MapLayout {
        id: LayoutId("LAYOUT_PACIFIDLOG_TOWN"),
        name: "PacifidlogTown_Layout",
        width: 20,
        height: 40,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Pacifidlog",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE101"),
        name: "Route101_Layout",
        width: 20,
        height: 20,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Petalburg",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE102"),
        name: "Route102_Layout",
        width: 50,
        height: 20,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Petalburg",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE103"),
        name: "Route103_Layout",
        width: 80,
        height: 22,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Petalburg",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE104"),
        name: "Route104_Layout",
        width: 40,
        height: 80,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Rustboro",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE105"),
        name: "Route105_Layout",
        width: 40,
        height: 80,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Dewford",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE106"),
        name: "Route106_Layout",
        width: 80,
        height: 20,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Dewford",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE107"),
        name: "Route107_Layout",
        width: 60,
        height: 20,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Dewford",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE108"),
        name: "Route108_Layout",
        width: 60,
        height: 20,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Slateport",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE109"),
        name: "Route109_Layout",
        width: 40,
        height: 63,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Slateport",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE110"),
        name: "Route110_Layout",
        width: 40,
        height: 100,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Mauville",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE111"),
        name: "Route111_Layout",
        width: 40,
        height: 140,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Mauville",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE112"),
        name: "Route112_Layout",
        width: 40,
        height: 60,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Lavaridge",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE113"),
        name: "Route113_Layout",
        width: 100,
        height: 20,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Fallarbor",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE114"),
        name: "Route114_Layout",
        width: 40,
        height: 80,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Fallarbor",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE115"),
        name: "Route115_Layout",
        width: 40,
        height: 80,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Fallarbor",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE116"),
        name: "Route116_Layout",
        width: 100,
        height: 20,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Rustboro",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE117"),
        name: "Route117_Layout",
        width: 60,
        height: 20,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Mauville",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE118"),
        name: "Route118_Layout",
        width: 80,
        height: 20,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Mauville",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE119"),
        name: "Route119_Layout",
        width: 40,
        height: 140,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Fortree",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE120"),
        name: "Route120_Layout",
        width: 40,
        height: 100,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Fortree",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE121"),
        name: "Route121_Layout",
        width: 80,
        height: 20,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Lilycove",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE122"),
        name: "Route122_Layout",
        width: 40,
        height: 40,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Lilycove",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE123"),
        name: "Route123_Layout",
        width: 140,
        height: 20,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Lilycove",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE124"),
        name: "Route124_Layout",
        width: 80,
        height: 80,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Mossdeep",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE125"),
        name: "Route125_Layout",
        width: 80,
        height: 40,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Mossdeep",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE126"),
        name: "Route126_Layout",
        width: 80,
        height: 80,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Mossdeep",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE127"),
        name: "Route127_Layout",
        width: 80,
        height: 80,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Mossdeep",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE128"),
        name: "Route128_Layout",
        width: 120,
        height: 40,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Mossdeep",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE129"),
        name: "Route129_Layout",
        width: 80,
        height: 40,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Mossdeep",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE130_MIRAGE_ISLAND"),
        name: "Route130_MirageIsland_Layout",
        width: 80,
        height: 40,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE131"),
        name: "Route131_Layout",
        width: 60,
        height: 40,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Pacifidlog",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE132"),
        name: "Route132_Layout",
        width: 80,
        height: 40,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Pacifidlog",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE133"),
        name: "Route133_Layout",
        width: 80,
        height: 40,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Pacifidlog",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE134"),
        name: "Route134_Layout",
        width: 80,
        height: 40,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Pacifidlog",
    },
    MapLayout {
        id: LayoutId("LAYOUT_UNDERWATER_ROUTE126"),
        name: "Underwater_Route126_Layout",
        width: 80,
        height: 80,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Underwater",
    },
    MapLayout {
        id: LayoutId("LAYOUT_UNDERWATER_ROUTE127"),
        name: "Underwater_Route127_Layout",
        width: 80,
        height: 80,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Underwater",
    },
    MapLayout {
        id: LayoutId("LAYOUT_UNDERWATER_ROUTE128"),
        name: "Underwater_Route128_Layout",
        width: 120,
        height: 40,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Underwater",
    },
    MapLayout {
        id: LayoutId("LAYOUT_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F"),
        name: "LittlerootTown_BrendansHouse_1F_Layout",
        width: 11,
        height: 9,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BrendansMaysHouse",
    },
    MapLayout {
        id: LayoutId("LAYOUT_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F"),
        name: "LittlerootTown_BrendansHouse_2F_Layout",
        width: 9,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BrendansMaysHouse",
    },
    MapLayout {
        id: LayoutId("LAYOUT_LITTLEROOT_TOWN_MAYS_HOUSE_1F"),
        name: "LittlerootTown_MaysHouse_1F_Layout",
        width: 11,
        height: 9,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BrendansMaysHouse",
    },
    MapLayout {
        id: LayoutId("LAYOUT_LITTLEROOT_TOWN_MAYS_HOUSE_2F"),
        name: "LittlerootTown_MaysHouse_2F_Layout",
        width: 9,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BrendansMaysHouse",
    },
    MapLayout {
        id: LayoutId("LAYOUT_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB"),
        name: "LittlerootTown_ProfessorBirchsLab_Layout",
        width: 13,
        height: 13,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_Lab",
    },
    MapLayout {
        id: LayoutId("LAYOUT_HOUSE1"),
        name: "House1_Layout",
        width: 10,
        height: 9,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_GenericBuilding",
    },
    MapLayout {
        id: LayoutId("LAYOUT_HOUSE2"),
        name: "House2_Layout",
        width: 11,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_GenericBuilding",
    },
    MapLayout {
        id: LayoutId("LAYOUT_POKEMON_CENTER_1F"),
        name: "PokemonCenter_1F_Layout",
        width: 14,
        height: 9,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_PokemonCenter",
    },
    MapLayout {
        id: LayoutId("LAYOUT_POKEMON_CENTER_2F"),
        name: "PokemonCenter_2F_Layout",
        width: 14,
        height: 10,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_PokemonCenter",
    },
    MapLayout {
        id: LayoutId("LAYOUT_MART"),
        name: "Mart_Layout",
        width: 11,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_Shop",
    },
    MapLayout {
        id: LayoutId("LAYOUT_HOUSE3"),
        name: "House3_Layout",
        width: 10,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_GenericBuilding",
    },
    MapLayout {
        id: LayoutId("LAYOUT_DEWFORD_TOWN_GYM"),
        name: "DewfordTown_Gym_Layout",
        width: 18,
        height: 28,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_DewfordGym",
    },
    MapLayout {
        id: LayoutId("LAYOUT_DEWFORD_TOWN_HALL"),
        name: "DewfordTown_Hall_Layout",
        width: 17,
        height: 9,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_GenericBuilding",
    },
    MapLayout {
        id: LayoutId("LAYOUT_HOUSE4"),
        name: "House4_Layout",
        width: 10,
        height: 9,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_GenericBuilding",
    },
    MapLayout {
        id: LayoutId("LAYOUT_LAVARIDGE_TOWN_HERB_SHOP"),
        name: "LavaridgeTown_HerbShop_Layout",
        width: 11,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_Shop",
    },
    MapLayout {
        id: LayoutId("LAYOUT_LAVARIDGE_TOWN_GYM_1F"),
        name: "LavaridgeTown_Gym_1F_Layout",
        width: 17,
        height: 19,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_LavaridgeGym",
    },
    MapLayout {
        id: LayoutId("LAYOUT_LAVARIDGE_TOWN_GYM_B1F"),
        name: "LavaridgeTown_Gym_B1F_Layout",
        width: 17,
        height: 19,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_LavaridgeGym",
    },
    MapLayout {
        id: LayoutId("LAYOUT_LAVARIDGE_TOWN_POKEMON_CENTER_1F"),
        name: "LavaridgeTown_PokemonCenter_1F_Layout",
        width: 14,
        height: 9,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_PokemonCenter",
    },
    MapLayout {
        id: LayoutId("LAYOUT_FALLARBOR_TOWN_LEFTOVER_RSCONTEST_LOBBY"),
        name: "FallarborTown_LeftoverRSContestLobby_Layout",
        width: 15,
        height: 7,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_Contest",
    },
    MapLayout {
        id: LayoutId("LAYOUT_FALLARBOR_TOWN_LEFTOVER_RSCONTEST_HALL"),
        name: "FallarborTown_LeftoverRSContestHall_Layout",
        width: 21,
        height: 18,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_Contest",
    },
    MapLayout {
        id: LayoutId("LAYOUT_LILYCOVE_CITY_HOUSE2"),
        name: "LilycoveCity_House2_Layout",
        width: 8,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_GenericBuilding",
    },
    MapLayout {
        id: LayoutId("LAYOUT_UNUSED_CONTEST_ROOM1"),
        name: "UnusedContestRoom1_Layout",
        width: 1,
        height: 1,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_Contest",
    },
    MapLayout {
        id: LayoutId("LAYOUT_VERDANTURF_TOWN_WANDAS_HOUSE"),
        name: "VerdanturfTown_WandasHouse_Layout",
        width: 17,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_GenericBuilding",
    },
    MapLayout {
        id: LayoutId("LAYOUT_PACIFIDLOG_TOWN_HOUSE1"),
        name: "PacifidlogTown_House1_Layout",
        width: 10,
        height: 9,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_GenericBuilding",
    },
    MapLayout {
        id: LayoutId("LAYOUT_PACIFIDLOG_TOWN_HOUSE2"),
        name: "PacifidlogTown_House2_Layout",
        width: 10,
        height: 9,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_GenericBuilding",
    },
    MapLayout {
        id: LayoutId("LAYOUT_PETALBURG_CITY_GYM"),
        name: "PetalburgCity_Gym_Layout",
        width: 9,
        height: 112,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_PetalburgGym",
    },
    MapLayout {
        id: LayoutId("LAYOUT_HOUSE_WITH_BED"),
        name: "HouseWithBed_Layout",
        width: 10,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_GenericBuilding",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SLATEPORT_CITY_STERNS_SHIPYARD_1F"),
        name: "SlateportCity_SternsShipyard_1F_Layout",
        width: 21,
        height: 15,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Facility",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SLATEPORT_CITY_STERNS_SHIPYARD_2F"),
        name: "SlateportCity_SternsShipyard_2F_Layout",
        width: 17,
        height: 15,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Facility",
    },
    MapLayout {
        id: LayoutId("LAYOUT_UNUSED_CONTEST_ROOM2"),
        name: "UnusedContestRoom2_Layout",
        width: 1,
        height: 1,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_Contest",
    },
    MapLayout {
        id: LayoutId("LAYOUT_UNUSED_CONTEST_ROOM3"),
        name: "UnusedContestRoom3_Layout",
        width: 1,
        height: 1,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_Contest",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SLATEPORT_CITY_POKEMON_FAN_CLUB"),
        name: "SlateportCity_PokemonFanClub_Layout",
        width: 14,
        height: 11,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_PokemonFanClub",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SLATEPORT_CITY_OCEANIC_MUSEUM_1F"),
        name: "SlateportCity_OceanicMuseum_1F_Layout",
        width: 20,
        height: 9,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_OceanicMuseum",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SLATEPORT_CITY_OCEANIC_MUSEUM_2F"),
        name: "SlateportCity_OceanicMuseum_2F_Layout",
        width: 20,
        height: 9,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_OceanicMuseum",
    },
    MapLayout {
        id: LayoutId("LAYOUT_HARBOR"),
        name: "Harbor_Layout",
        width: 24,
        height: 15,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Facility",
    },
    MapLayout {
        id: LayoutId("LAYOUT_MAUVILLE_CITY_GYM"),
        name: "MauvilleCity_Gym_Layout",
        width: 10,
        height: 21,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_MauvilleGym",
    },
    MapLayout {
        id: LayoutId("LAYOUT_MAUVILLE_CITY_BIKE_SHOP"),
        name: "MauvilleCity_BikeShop_Layout",
        width: 12,
        height: 9,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_BikeShop",
    },
    MapLayout {
        id: LayoutId("LAYOUT_MAUVILLE_CITY_GAME_CORNER"),
        name: "MauvilleCity_GameCorner_Layout",
        width: 22,
        height: 11,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_MauvilleGameCorner",
    },
    MapLayout {
        id: LayoutId("LAYOUT_RUSTBORO_CITY_DEVON_CORP_1F"),
        name: "RustboroCity_DevonCorp_1F_Layout",
        width: 19,
        height: 9,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_Facility",
    },
    MapLayout {
        id: LayoutId("LAYOUT_RUSTBORO_CITY_DEVON_CORP_2F"),
        name: "RustboroCity_DevonCorp_2F_Layout",
        width: 19,
        height: 9,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Facility",
    },
    MapLayout {
        id: LayoutId("LAYOUT_RUSTBORO_CITY_GYM"),
        name: "RustboroCity_Gym_Layout",
        width: 11,
        height: 20,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_RustboroGym",
    },
    MapLayout {
        id: LayoutId("LAYOUT_RUSTBORO_CITY_POKEMON_SCHOOL"),
        name: "RustboroCity_PokemonSchool_Layout",
        width: 12,
        height: 11,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_PokemonSchool",
    },
    MapLayout {
        id: LayoutId("LAYOUT_RUSTBORO_CITY_HOUSE"),
        name: "RustboroCity_House_Layout",
        width: 12,
        height: 9,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_GenericBuilding",
    },
    MapLayout {
        id: LayoutId("LAYOUT_RUSTBORO_CITY_HOUSE1"),
        name: "RustboroCity_House1_Layout",
        width: 13,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_GenericBuilding",
    },
    MapLayout {
        id: LayoutId("LAYOUT_RUSTBORO_CITY_CUTTERS_HOUSE"),
        name: "RustboroCity_CuttersHouse_Layout",
        width: 11,
        height: 9,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_GenericBuilding",
    },
    MapLayout {
        id: LayoutId("LAYOUT_FORTREE_CITY_HOUSE1"),
        name: "FortreeCity_House1_Layout",
        width: 8,
        height: 6,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_GenericBuilding",
    },
    MapLayout {
        id: LayoutId("LAYOUT_FORTREE_CITY_GYM"),
        name: "FortreeCity_Gym_Layout",
        width: 20,
        height: 25,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_FortreeGym",
    },
    MapLayout {
        id: LayoutId("LAYOUT_FORTREE_CITY_HOUSE2"),
        name: "FortreeCity_House2_Layout",
        width: 8,
        height: 6,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_GenericBuilding",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE104_MR_BRINEYS_HOUSE"),
        name: "Route104_MrBrineysHouse_Layout",
        width: 12,
        height: 9,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_GenericBuilding",
    },
    MapLayout {
        id: LayoutId("LAYOUT_LILYCOVE_CITY_LILYCOVE_MUSEUM_1F"),
        name: "LilycoveCity_LilycoveMuseum_1F_Layout",
        width: 21,
        height: 14,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_LilycoveMuseum",
    },
    MapLayout {
        id: LayoutId("LAYOUT_LILYCOVE_CITY_LILYCOVE_MUSEUM_2F"),
        name: "LilycoveCity_LilycoveMuseum_2F_Layout",
        width: 22,
        height: 13,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_LilycoveMuseum",
    },
    MapLayout {
        id: LayoutId("LAYOUT_LILYCOVE_CITY_CONTEST_LOBBY"),
        name: "LilycoveCity_ContestLobby_Layout",
        width: 31,
        height: 12,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_Contest",
    },
    MapLayout {
        id: LayoutId("LAYOUT_LILYCOVE_CITY_CONTEST_HALL"),
        name: "LilycoveCity_ContestHall_Layout",
        width: 51,
        height: 33,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_Contest",
    },
    MapLayout {
        id: LayoutId("LAYOUT_LILYCOVE_CITY_POKEMON_TRAINER_FAN_CLUB"),
        name: "LilycoveCity_PokemonTrainerFanClub_Layout",
        width: 12,
        height: 14,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_GenericBuilding",
    },
    MapLayout {
        id: LayoutId("LAYOUT_MOSSDEEP_CITY_GYM"),
        name: "MossdeepCity_Gym_Layout",
        width: 26,
        height: 36,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_MossdeepGym",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SOOTOPOLIS_CITY_GYM_1F"),
        name: "SootopolisCity_Gym_1F_Layout",
        width: 17,
        height: 26,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_SootopolisGym",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SOOTOPOLIS_CITY_GYM_B1F"),
        name: "SootopolisCity_Gym_B1F_Layout",
        width: 17,
        height: 26,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_SootopolisGym",
    },
    MapLayout {
        id: LayoutId("LAYOUT_EVER_GRANDE_CITY_SIDNEYS_ROOM"),
        name: "EverGrandeCity_SidneysRoom_Layout",
        width: 13,
        height: 14,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_EliteFour",
    },
    MapLayout {
        id: LayoutId("LAYOUT_EVER_GRANDE_CITY_PHOEBES_ROOM"),
        name: "EverGrandeCity_PhoebesRoom_Layout",
        width: 13,
        height: 14,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_EliteFour",
    },
    MapLayout {
        id: LayoutId("LAYOUT_EVER_GRANDE_CITY_GLACIAS_ROOM"),
        name: "EverGrandeCity_GlaciasRoom_Layout",
        width: 13,
        height: 14,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_EliteFour",
    },
    MapLayout {
        id: LayoutId("LAYOUT_EVER_GRANDE_CITY_DRAKES_ROOM"),
        name: "EverGrandeCity_DrakesRoom_Layout",
        width: 13,
        height: 14,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_EliteFour",
    },
    MapLayout {
        id: LayoutId("LAYOUT_EVER_GRANDE_CITY_CHAMPIONS_ROOM"),
        name: "EverGrandeCity_ChampionsRoom_Layout",
        width: 13,
        height: 13,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_EliteFour",
    },
    MapLayout {
        id: LayoutId("LAYOUT_EVER_GRANDE_CITY_SHORT_HALL"),
        name: "EverGrandeCity_ShortHall_Layout",
        width: 11,
        height: 13,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_EliteFour",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE104_PRETTY_PETAL_FLOWER_SHOP"),
        name: "Route104_PrettyPetalFlowerShop_Layout",
        width: 15,
        height: 9,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_PrettyPetalFlowerShop",
    },
    MapLayout {
        id: LayoutId("LAYOUT_CABLE_CAR_STATION"),
        name: "CableCarStation_Layout",
        width: 13,
        height: 12,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Facility",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE114_FOSSIL_MANIACS_HOUSE"),
        name: "Route114_FossilManiacsHouse_Layout",
        width: 10,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_GenericBuilding",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE114_FOSSIL_MANIACS_TUNNEL"),
        name: "Route114_FossilManiacsTunnel_Layout",
        width: 13,
        height: 26,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Fallarbor",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE114_LANETTES_HOUSE"),
        name: "Route114_LanettesHouse_Layout",
        width: 11,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_Lab",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE116_TUNNELERS_REST_HOUSE"),
        name: "Route116_TunnelersRestHouse_Layout",
        width: 10,
        height: 9,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_GenericBuilding",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE117_POKEMON_DAY_CARE"),
        name: "Route117_PokemonDayCare_Layout",
        width: 12,
        height: 9,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_PokemonDayCare",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE121_SAFARI_ZONE_ENTRANCE"),
        name: "Route121_SafariZoneEntrance_Layout",
        width: 18,
        height: 14,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_Shop",
    },
    MapLayout {
        id: LayoutId("LAYOUT_METEOR_FALLS_1F_1R"),
        name: "MeteorFalls_1F_1R_Layout",
        width: 30,
        height: 42,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_MeteorFalls",
    },
    MapLayout {
        id: LayoutId("LAYOUT_METEOR_FALLS_1F_2R"),
        name: "MeteorFalls_1F_2R_Layout",
        width: 30,
        height: 32,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_MeteorFalls",
    },
    MapLayout {
        id: LayoutId("LAYOUT_METEOR_FALLS_B1F_1R"),
        name: "MeteorFalls_B1F_1R_Layout",
        width: 29,
        height: 38,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_MeteorFalls",
    },
    MapLayout {
        id: LayoutId("LAYOUT_METEOR_FALLS_B1F_2R"),
        name: "MeteorFalls_B1F_2R_Layout",
        width: 11,
        height: 18,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_MeteorFalls",
    },
    MapLayout {
        id: LayoutId("LAYOUT_RUSTURF_TUNNEL"),
        name: "RusturfTunnel_Layout",
        width: 36,
        height: 24,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_RusturfTunnel",
    },
    MapLayout {
        id: LayoutId("LAYOUT_UNDERWATER_SOOTOPOLIS_CITY"),
        name: "Underwater_SootopolisCity_Layout",
        width: 20,
        height: 10,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Underwater",
    },
    MapLayout {
        id: LayoutId("LAYOUT_DESERT_RUINS"),
        name: "DesertRuins_Layout",
        width: 17,
        height: 33,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_GRANITE_CAVE_1F"),
        name: "GraniteCave_1F_Layout",
        width: 42,
        height: 15,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_GRANITE_CAVE_B1F"),
        name: "GraniteCave_B1F_Layout",
        width: 32,
        height: 26,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_GRANITE_CAVE_B2F"),
        name: "GraniteCave_B2F_Layout",
        width: 32,
        height: 26,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_PETALBURG_WOODS"),
        name: "PetalburgWoods_Layout",
        width: 48,
        height: 44,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Rustboro",
    },
    MapLayout {
        id: LayoutId("LAYOUT_MT_CHIMNEY"),
        name: "MtChimney_Layout",
        width: 40,
        height: 47,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Lavaridge",
    },
    MapLayout {
        id: LayoutId("LAYOUT_MT_PYRE_1F"),
        name: "MtPyre_1F_Layout",
        width: 22,
        height: 19,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Facility",
    },
    MapLayout {
        id: LayoutId("LAYOUT_MT_PYRE_2F"),
        name: "MtPyre_2F_Layout",
        width: 13,
        height: 13,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Facility",
    },
    MapLayout {
        id: LayoutId("LAYOUT_MT_PYRE_3F"),
        name: "MtPyre_3F_Layout",
        width: 13,
        height: 13,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Facility",
    },
    MapLayout {
        id: LayoutId("LAYOUT_MT_PYRE_4F"),
        name: "MtPyre_4F_Layout",
        width: 13,
        height: 13,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Facility",
    },
    MapLayout {
        id: LayoutId("LAYOUT_MT_PYRE_5F"),
        name: "MtPyre_5F_Layout",
        width: 13,
        height: 13,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Facility",
    },
    MapLayout {
        id: LayoutId("LAYOUT_MT_PYRE_6F"),
        name: "MtPyre_6F_Layout",
        width: 13,
        height: 13,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Facility",
    },
    MapLayout {
        id: LayoutId("LAYOUT_AQUA_HIDEOUT_1F"),
        name: "AquaHideout_1F_Layout",
        width: 28,
        height: 30,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Facility",
    },
    MapLayout {
        id: LayoutId("LAYOUT_AQUA_HIDEOUT_B1F"),
        name: "AquaHideout_B1F_Layout",
        width: 51,
        height: 24,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Facility",
    },
    MapLayout {
        id: LayoutId("LAYOUT_AQUA_HIDEOUT_B2F"),
        name: "AquaHideout_B2F_Layout",
        width: 34,
        height: 24,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Facility",
    },
    MapLayout {
        id: LayoutId("LAYOUT_UNDERWATER_SEAFLOOR_CAVERN"),
        name: "Underwater_SeafloorCavern_Layout",
        width: 14,
        height: 9,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Underwater",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SEAFLOOR_CAVERN_ENTRANCE"),
        name: "SeafloorCavern_Entrance_Layout",
        width: 20,
        height: 20,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SEAFLOOR_CAVERN_ROOM1"),
        name: "SeafloorCavern_Room1_Layout",
        width: 20,
        height: 21,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SEAFLOOR_CAVERN_ROOM2"),
        name: "SeafloorCavern_Room2_Layout",
        width: 18,
        height: 12,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SEAFLOOR_CAVERN_ROOM3"),
        name: "SeafloorCavern_Room3_Layout",
        width: 16,
        height: 17,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SEAFLOOR_CAVERN_ROOM4"),
        name: "SeafloorCavern_Room4_Layout",
        width: 18,
        height: 19,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SEAFLOOR_CAVERN_ROOM5"),
        name: "SeafloorCavern_Room5_Layout",
        width: 20,
        height: 20,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SEAFLOOR_CAVERN_ROOM6"),
        name: "SeafloorCavern_Room6_Layout",
        width: 24,
        height: 23,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Pacifidlog",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SEAFLOOR_CAVERN_ROOM7"),
        name: "SeafloorCavern_Room7_Layout",
        width: 23,
        height: 25,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Pacifidlog",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SEAFLOOR_CAVERN_ROOM8"),
        name: "SeafloorCavern_Room8_Layout",
        width: 11,
        height: 14,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SEAFLOOR_CAVERN_ROOM9"),
        name: "SeafloorCavern_Room9_Layout",
        width: 27,
        height: 46,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_CAVE_OF_ORIGIN_ENTRANCE"),
        name: "CaveOfOrigin_Entrance_Layout",
        width: 19,
        height: 26,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_CAVE_OF_ORIGIN_1F"),
        name: "CaveOfOrigin_1F_Layout",
        width: 23,
        height: 23,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_CAVE_OF_ORIGIN_UNUSED_RUBY_SAPPHIRE_MAP1"),
        name: "CaveOfOrigin_UnusedRubySapphireMap1_Layout",
        width: 23,
        height: 23,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_CAVE_OF_ORIGIN_UNUSED_RUBY_SAPPHIRE_MAP2"),
        name: "CaveOfOrigin_UnusedRubySapphireMap2_Layout",
        width: 21,
        height: 21,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_CAVE_OF_ORIGIN_UNUSED_RUBY_SAPPHIRE_MAP3"),
        name: "CaveOfOrigin_UnusedRubySapphireMap3_Layout",
        width: 19,
        height: 21,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_CAVE_OF_ORIGIN_B1F"),
        name: "CaveOfOrigin_B1F_Layout",
        width: 19,
        height: 19,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_VICTORY_ROAD_1F"),
        name: "VictoryRoad_1F_Layout",
        width: 46,
        height: 45,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SHOAL_CAVE_LOW_TIDE_ENTRANCE_ROOM"),
        name: "ShoalCave_LowTideEntranceRoom_Layout",
        width: 35,
        height: 35,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SHOAL_CAVE_LOW_TIDE_INNER_ROOM"),
        name: "ShoalCave_LowTideInnerRoom_Layout",
        width: 46,
        height: 38,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SHOAL_CAVE_LOW_TIDE_STAIRS_ROOM"),
        name: "ShoalCave_LowTideStairsRoom_Layout",
        width: 21,
        height: 15,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SHOAL_CAVE_LOW_TIDE_LOWER_ROOM"),
        name: "ShoalCave_LowTideLowerRoom_Layout",
        width: 31,
        height: 14,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SHOAL_CAVE_HIGH_TIDE_ENTRANCE_ROOM"),
        name: "ShoalCave_HighTideEntranceRoom_Layout",
        width: 35,
        height: 35,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SHOAL_CAVE_HIGH_TIDE_INNER_ROOM"),
        name: "ShoalCave_HighTideInnerRoom_Layout",
        width: 46,
        height: 38,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_UNUSED_CAVE1"),
        name: "UnusedCave1_Layout",
        width: 1,
        height: 1,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_UNUSED_CAVE2"),
        name: "UnusedCave2_Layout",
        width: 1,
        height: 1,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_UNUSED_CAVE3"),
        name: "UnusedCave3_Layout",
        width: 1,
        height: 1,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_UNUSED_CAVE4"),
        name: "UnusedCave4_Layout",
        width: 1,
        height: 1,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_UNUSED_CAVE5"),
        name: "UnusedCave5_Layout",
        width: 1,
        height: 1,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_UNUSED_CAVE6"),
        name: "UnusedCave6_Layout",
        width: 1,
        height: 1,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_UNUSED_CAVE7"),
        name: "UnusedCave7_Layout",
        width: 1,
        height: 1,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_UNUSED_CAVE8"),
        name: "UnusedCave8_Layout",
        width: 1,
        height: 1,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_UNUSED_CAVE9"),
        name: "UnusedCave9_Layout",
        width: 1,
        height: 1,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_UNUSED_CAVE10"),
        name: "UnusedCave10_Layout",
        width: 1,
        height: 1,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_UNUSED_CAVE11"),
        name: "UnusedCave11_Layout",
        width: 1,
        height: 1,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_UNUSED_CAVE12"),
        name: "UnusedCave12_Layout",
        width: 1,
        height: 1,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_UNUSED_CAVE13"),
        name: "UnusedCave13_Layout",
        width: 1,
        height: 1,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_UNUSED_CAVE14"),
        name: "UnusedCave14_Layout",
        width: 1,
        height: 1,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_NEW_MAUVILLE_ENTRANCE"),
        name: "NewMauville_Entrance_Layout",
        width: 9,
        height: 9,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Facility",
    },
    MapLayout {
        id: LayoutId("LAYOUT_NEW_MAUVILLE_INSIDE"),
        name: "NewMauville_Inside_Layout",
        width: 41,
        height: 41,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_BikeShop",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ABANDONED_SHIP_DECK"),
        name: "AbandonedShip_Deck_Layout",
        width: 23,
        height: 21,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Facility",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ABANDONED_SHIP_CORRIDORS_1F"),
        name: "AbandonedShip_Corridors_1F_Layout",
        width: 18,
        height: 12,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_InsideShip",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ABANDONED_SHIP_ROOMS_1F"),
        name: "AbandonedShip_Rooms_1F_Layout",
        width: 18,
        height: 17,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_InsideShip",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ABANDONED_SHIP_CORRIDORS_B1F"),
        name: "AbandonedShip_Corridors_B1F_Layout",
        width: 13,
        height: 10,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_InsideShip",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ABANDONED_SHIP_ROOMS_B1F"),
        name: "AbandonedShip_Rooms_B1F_Layout",
        width: 27,
        height: 8,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_InsideShip",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ABANDONED_SHIP_ROOMS2_B1F"),
        name: "AbandonedShip_Rooms2_B1F_Layout",
        width: 18,
        height: 8,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_InsideShip",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ABANDONED_SHIP_UNDERWATER1"),
        name: "AbandonedShip_Underwater1_Layout",
        width: 8,
        height: 8,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_InsideShip",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ABANDONED_SHIP_ROOM_B1F"),
        name: "AbandonedShip_Room_B1F_Layout",
        width: 9,
        height: 8,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_InsideShip",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ABANDONED_SHIP_ROOMS2_1F"),
        name: "AbandonedShip_Rooms2_1F_Layout",
        width: 9,
        height: 17,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_InsideShip",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ABANDONED_SHIP_CAPTAINS_OFFICE"),
        name: "AbandonedShip_CaptainsOffice_Layout",
        width: 9,
        height: 7,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Facility",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ABANDONED_SHIP_UNDERWATER2"),
        name: "AbandonedShip_Underwater2_Layout",
        width: 21,
        height: 7,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_InsideShip",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SECRET_BASE_RED_CAVE1"),
        name: "SecretBase_RedCave1_Layout",
        width: 11,
        height: 9,
        primary_tileset: "gTileset_SecretBase",
        secondary_tileset: "gTileset_SecretBaseRedCave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SECRET_BASE_BROWN_CAVE1"),
        name: "SecretBase_BrownCave1_Layout",
        width: 11,
        height: 9,
        primary_tileset: "gTileset_SecretBase",
        secondary_tileset: "gTileset_SecretBaseBrownCave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SECRET_BASE_BLUE_CAVE1"),
        name: "SecretBase_BlueCave1_Layout",
        width: 11,
        height: 9,
        primary_tileset: "gTileset_SecretBase",
        secondary_tileset: "gTileset_SecretBaseBlueCave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SECRET_BASE_YELLOW_CAVE1"),
        name: "SecretBase_YellowCave1_Layout",
        width: 11,
        height: 9,
        primary_tileset: "gTileset_SecretBase",
        secondary_tileset: "gTileset_SecretBaseYellowCave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SECRET_BASE_TREE1"),
        name: "SecretBase_Tree1_Layout",
        width: 11,
        height: 9,
        primary_tileset: "gTileset_SecretBase",
        secondary_tileset: "gTileset_SecretBaseTree",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SECRET_BASE_SHRUB1"),
        name: "SecretBase_Shrub1_Layout",
        width: 11,
        height: 9,
        primary_tileset: "gTileset_SecretBase",
        secondary_tileset: "gTileset_SecretBaseShrub",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SECRET_BASE_RED_CAVE2"),
        name: "SecretBase_RedCave2_Layout",
        width: 7,
        height: 16,
        primary_tileset: "gTileset_SecretBase",
        secondary_tileset: "gTileset_SecretBaseRedCave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SECRET_BASE_BROWN_CAVE2"),
        name: "SecretBase_BrownCave2_Layout",
        width: 14,
        height: 9,
        primary_tileset: "gTileset_SecretBase",
        secondary_tileset: "gTileset_SecretBaseBrownCave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SECRET_BASE_BLUE_CAVE2"),
        name: "SecretBase_BlueCave2_Layout",
        width: 15,
        height: 7,
        primary_tileset: "gTileset_SecretBase",
        secondary_tileset: "gTileset_SecretBaseBlueCave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SECRET_BASE_YELLOW_CAVE2"),
        name: "SecretBase_YellowCave2_Layout",
        width: 14,
        height: 9,
        primary_tileset: "gTileset_SecretBase",
        secondary_tileset: "gTileset_SecretBaseYellowCave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SECRET_BASE_TREE2"),
        name: "SecretBase_Tree2_Layout",
        width: 7,
        height: 16,
        primary_tileset: "gTileset_SecretBase",
        secondary_tileset: "gTileset_SecretBaseTree",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SECRET_BASE_SHRUB2"),
        name: "SecretBase_Shrub2_Layout",
        width: 15,
        height: 7,
        primary_tileset: "gTileset_SecretBase",
        secondary_tileset: "gTileset_SecretBaseShrub",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SECRET_BASE_RED_CAVE3"),
        name: "SecretBase_RedCave3_Layout",
        width: 15,
        height: 8,
        primary_tileset: "gTileset_SecretBase",
        secondary_tileset: "gTileset_SecretBaseRedCave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SECRET_BASE_BROWN_CAVE3"),
        name: "SecretBase_BrownCave3_Layout",
        width: 15,
        height: 11,
        primary_tileset: "gTileset_SecretBase",
        secondary_tileset: "gTileset_SecretBaseBrownCave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SECRET_BASE_BLUE_CAVE3"),
        name: "SecretBase_BlueCave3_Layout",
        width: 10,
        height: 17,
        primary_tileset: "gTileset_SecretBase",
        secondary_tileset: "gTileset_SecretBaseBlueCave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SECRET_BASE_YELLOW_CAVE3"),
        name: "SecretBase_YellowCave3_Layout",
        width: 12,
        height: 11,
        primary_tileset: "gTileset_SecretBase",
        secondary_tileset: "gTileset_SecretBaseYellowCave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SECRET_BASE_TREE3"),
        name: "SecretBase_Tree3_Layout",
        width: 17,
        height: 8,
        primary_tileset: "gTileset_SecretBase",
        secondary_tileset: "gTileset_SecretBaseTree",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SECRET_BASE_SHRUB3"),
        name: "SecretBase_Shrub3_Layout",
        width: 13,
        height: 11,
        primary_tileset: "gTileset_SecretBase",
        secondary_tileset: "gTileset_SecretBaseShrub",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SECRET_BASE_RED_CAVE4"),
        name: "SecretBase_RedCave4_Layout",
        width: 9,
        height: 15,
        primary_tileset: "gTileset_SecretBase",
        secondary_tileset: "gTileset_SecretBaseRedCave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SECRET_BASE_BROWN_CAVE4"),
        name: "SecretBase_BrownCave4_Layout",
        width: 14,
        height: 12,
        primary_tileset: "gTileset_SecretBase",
        secondary_tileset: "gTileset_SecretBaseBrownCave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SECRET_BASE_BLUE_CAVE4"),
        name: "SecretBase_BlueCave4_Layout",
        width: 9,
        height: 17,
        primary_tileset: "gTileset_SecretBase",
        secondary_tileset: "gTileset_SecretBaseBlueCave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SECRET_BASE_YELLOW_CAVE4"),
        name: "SecretBase_YellowCave4_Layout",
        width: 13,
        height: 14,
        primary_tileset: "gTileset_SecretBase",
        secondary_tileset: "gTileset_SecretBaseYellowCave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SECRET_BASE_TREE4"),
        name: "SecretBase_Tree4_Layout",
        width: 14,
        height: 14,
        primary_tileset: "gTileset_SecretBase",
        secondary_tileset: "gTileset_SecretBaseTree",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SECRET_BASE_SHRUB4"),
        name: "SecretBase_Shrub4_Layout",
        width: 14,
        height: 11,
        primary_tileset: "gTileset_SecretBase",
        secondary_tileset: "gTileset_SecretBaseShrub",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_COLOSSEUM_2P"),
        name: "BattleColosseum_2P_Layout",
        width: 14,
        height: 9,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_CableClub",
    },
    MapLayout {
        id: LayoutId("LAYOUT_TRADE_CENTER"),
        name: "TradeCenter_Layout",
        width: 12,
        height: 9,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_CableClub",
    },
    MapLayout {
        id: LayoutId("LAYOUT_RECORD_CORNER"),
        name: "RecordCorner_Layout",
        width: 20,
        height: 10,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_CableClub",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_COLOSSEUM_4P"),
        name: "BattleColosseum_4P_Layout",
        width: 14,
        height: 9,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_CableClub",
    },
    MapLayout {
        id: LayoutId("LAYOUT_CONTEST_HALL"),
        name: "ContestHall_Layout",
        width: 15,
        height: 11,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Contest",
    },
    MapLayout {
        id: LayoutId("LAYOUT_UNUSED_CONTEST_HALL1"),
        name: "UnusedContestHall1_Layout",
        width: 1,
        height: 1,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_Contest",
    },
    MapLayout {
        id: LayoutId("LAYOUT_UNUSED_CONTEST_HALL2"),
        name: "UnusedContestHall2_Layout",
        width: 1,
        height: 1,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_Contest",
    },
    MapLayout {
        id: LayoutId("LAYOUT_UNUSED_CONTEST_HALL3"),
        name: "UnusedContestHall3_Layout",
        width: 1,
        height: 1,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_Contest",
    },
    MapLayout {
        id: LayoutId("LAYOUT_UNUSED_CONTEST_HALL4"),
        name: "UnusedContestHall4_Layout",
        width: 1,
        height: 1,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_Contest",
    },
    MapLayout {
        id: LayoutId("LAYOUT_UNUSED_CONTEST_HALL5"),
        name: "UnusedContestHall5_Layout",
        width: 1,
        height: 1,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_Contest",
    },
    MapLayout {
        id: LayoutId("LAYOUT_UNUSED_CONTEST_HALL6"),
        name: "UnusedContestHall6_Layout",
        width: 1,
        height: 1,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_Contest",
    },
    MapLayout {
        id: LayoutId("LAYOUT_CONTEST_HALL_BEAUTY"),
        name: "ContestHallBeauty_Layout",
        width: 15,
        height: 11,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Contest",
    },
    MapLayout {
        id: LayoutId("LAYOUT_CONTEST_HALL_TOUGH"),
        name: "ContestHallTough_Layout",
        width: 15,
        height: 11,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Contest",
    },
    MapLayout {
        id: LayoutId("LAYOUT_CONTEST_HALL_COOL"),
        name: "ContestHallCool_Layout",
        width: 15,
        height: 11,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Contest",
    },
    MapLayout {
        id: LayoutId("LAYOUT_CONTEST_HALL_SMART"),
        name: "ContestHallSmart_Layout",
        width: 15,
        height: 11,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Contest",
    },
    MapLayout {
        id: LayoutId("LAYOUT_CONTEST_HALL_CUTE"),
        name: "ContestHallCute_Layout",
        width: 15,
        height: 11,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Contest",
    },
    MapLayout {
        id: LayoutId("LAYOUT_INSIDE_OF_TRUCK"),
        name: "InsideOfTruck_Layout",
        width: 5,
        height: 5,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_InsideOfTruck",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SAFARI_ZONE_NORTHWEST"),
        name: "SafariZone_Northwest_Layout",
        width: 40,
        height: 40,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Lilycove",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SAFARI_ZONE_NORTH"),
        name: "SafariZone_North_Layout",
        width: 40,
        height: 40,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Lilycove",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SAFARI_ZONE_SOUTHWEST"),
        name: "SafariZone_Southwest_Layout",
        width: 40,
        height: 40,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Lilycove",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SAFARI_ZONE_SOUTH"),
        name: "SafariZone_South_Layout",
        width: 40,
        height: 40,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Lilycove",
    },
    MapLayout {
        id: LayoutId("LAYOUT_UNUSED_OUTDOOR_AREA"),
        name: "UnusedOutdoorArea_Layout",
        width: 58,
        height: 26,
        primary_tileset: "gTileset_General",
        secondary_tileset: NO_TILESET,
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE109_SEASHORE_HOUSE"),
        name: "Route109_SeashoreHouse_Layout",
        width: 15,
        height: 10,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_SeashoreHouse",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE110_TRICK_HOUSE_ENTRANCE"),
        name: "Route110_TrickHouseEntrance_Layout",
        width: 12,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_GenericBuilding",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE110_TRICK_HOUSE_END"),
        name: "Route110_TrickHouseEnd_Layout",
        width: 12,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_GenericBuilding",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE110_TRICK_HOUSE_CORRIDOR"),
        name: "Route110_TrickHouseCorridor_Layout",
        width: 15,
        height: 24,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_GenericBuilding",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE110_TRICK_HOUSE_PUZZLE1"),
        name: "Route110_TrickHousePuzzle1_Layout",
        width: 15,
        height: 22,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_TrickHousePuzzle",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE110_TRICK_HOUSE_PUZZLE2"),
        name: "Route110_TrickHousePuzzle2_Layout",
        width: 15,
        height: 22,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_TrickHousePuzzle",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE110_TRICK_HOUSE_PUZZLE3"),
        name: "Route110_TrickHousePuzzle3_Layout",
        width: 15,
        height: 22,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_TrickHousePuzzle",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE110_TRICK_HOUSE_PUZZLE4"),
        name: "Route110_TrickHousePuzzle4_Layout",
        width: 15,
        height: 22,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_TrickHousePuzzle",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE110_TRICK_HOUSE_PUZZLE5"),
        name: "Route110_TrickHousePuzzle5_Layout",
        width: 15,
        height: 22,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_TrickHousePuzzle",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE110_TRICK_HOUSE_PUZZLE6"),
        name: "Route110_TrickHousePuzzle6_Layout",
        width: 15,
        height: 22,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_TrickHousePuzzle",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE110_TRICK_HOUSE_PUZZLE7"),
        name: "Route110_TrickHousePuzzle7_Layout",
        width: 15,
        height: 22,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_TrickHousePuzzle",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE110_TRICK_HOUSE_PUZZLE8"),
        name: "Route110_TrickHousePuzzle8_Layout",
        width: 15,
        height: 22,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_TrickHousePuzzle",
    },
    MapLayout {
        id: LayoutId("LAYOUT_FORTREE_CITY_DECORATION_SHOP"),
        name: "FortreeCity_DecorationShop_Layout",
        width: 8,
        height: 6,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_GenericBuilding",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE110_SEASIDE_CYCLING_ROAD_ENTRANCE"),
        name: "Route110_SeasideCyclingRoadEntrance_Layout",
        width: 15,
        height: 6,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_Shop",
    },
    MapLayout {
        id: LayoutId("LAYOUT_LILYCOVE_CITY_DEPARTMENT_STORE_1F"),
        name: "LilycoveCity_DepartmentStore_1F_Layout",
        width: 18,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_Shop",
    },
    MapLayout {
        id: LayoutId("LAYOUT_LILYCOVE_CITY_DEPARTMENT_STORE_2F"),
        name: "LilycoveCity_DepartmentStore_2F_Layout",
        width: 18,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_Shop",
    },
    MapLayout {
        id: LayoutId("LAYOUT_LILYCOVE_CITY_DEPARTMENT_STORE_3F"),
        name: "LilycoveCity_DepartmentStore_3F_Layout",
        width: 18,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_Shop",
    },
    MapLayout {
        id: LayoutId("LAYOUT_LILYCOVE_CITY_DEPARTMENT_STORE_4F"),
        name: "LilycoveCity_DepartmentStore_4F_Layout",
        width: 18,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_Shop",
    },
    MapLayout {
        id: LayoutId("LAYOUT_LILYCOVE_CITY_DEPARTMENT_STORE_5F"),
        name: "LilycoveCity_DepartmentStore_5F_Layout",
        width: 18,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_Shop",
    },
    MapLayout {
        id: LayoutId("LAYOUT_LILYCOVE_CITY_DEPARTMENT_STORE_ROOFTOP"),
        name: "LilycoveCity_DepartmentStoreRooftop_Layout",
        width: 18,
        height: 12,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_Shop",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE130"),
        name: "Route130_Layout",
        width: 80,
        height: 40,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_TOWER_LOBBY"),
        name: "BattleFrontier_BattleTowerLobby_Layout",
        width: 25,
        height: 10,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattleFrontier",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_FRONTIER_OUTSIDE_WEST"),
        name: "BattleFrontier_OutsideWest_Layout",
        width: 56,
        height: 72,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_BattleFrontierOutsideWest",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_ELEVATOR"),
        name: "BattleElevator_Layout",
        width: 5,
        height: 7,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattleFrontier",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_TOWER_CORRIDOR"),
        name: "BattleFrontier_BattleTowerCorridor_Layout",
        width: 17,
        height: 5,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattleFrontier",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_TOWER_BATTLE_ROOM"),
        name: "BattleFrontier_BattleTowerBattleRoom_Layout",
        width: 10,
        height: 9,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattleFrontier",
    },
    MapLayout {
        id: LayoutId("LAYOUT_RUSTBORO_CITY_DEVON_CORP_3F"),
        name: "RustboroCity_DevonCorp_3F_Layout",
        width: 19,
        height: 9,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_Facility",
    },
    MapLayout {
        id: LayoutId("LAYOUT_EVER_GRANDE_CITY_POKEMON_LEAGUE_1F"),
        name: "EverGrandeCity_PokemonLeague_1F_Layout",
        width: 19,
        height: 12,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_PokemonCenter",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE119_WEATHER_INSTITUTE_1F"),
        name: "Route119_WeatherInstitute_1F_Layout",
        width: 20,
        height: 13,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_Lab",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE119_WEATHER_INSTITUTE_2F"),
        name: "Route119_WeatherInstitute_2F_Layout",
        width: 20,
        height: 11,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_Lab",
    },
    MapLayout {
        id: LayoutId("LAYOUT_LILYCOVE_CITY_DEPARTMENT_STORE_ELEVATOR"),
        name: "LilycoveCity_DepartmentStoreElevator_Layout",
        width: 5,
        height: 6,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattleFrontier",
    },
    MapLayout {
        id: LayoutId("LAYOUT_UNDERWATER_ROUTE124"),
        name: "Underwater_Route124_Layout",
        width: 80,
        height: 80,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Underwater",
    },
    MapLayout {
        id: LayoutId("LAYOUT_MOSSDEEP_CITY_SPACE_CENTER_1F"),
        name: "MossdeepCity_SpaceCenter_1F_Layout",
        width: 16,
        height: 10,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Facility",
    },
    MapLayout {
        id: LayoutId("LAYOUT_MOSSDEEP_CITY_SPACE_CENTER_2F"),
        name: "MossdeepCity_SpaceCenter_2F_Layout",
        width: 16,
        height: 10,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Facility",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SS_TIDAL_CORRIDOR"),
        name: "SSTidalCorridor_Layout",
        width: 18,
        height: 13,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_InsideShip",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SS_TIDAL_LOWER_DECK"),
        name: "SSTidalLowerDeck_Layout",
        width: 17,
        height: 13,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_InsideShip",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SS_TIDAL_ROOMS"),
        name: "SSTidalRooms_Layout",
        width: 36,
        height: 18,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_InsideShip",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ISLAND_CAVE"),
        name: "IslandCave_Layout",
        width: 17,
        height: 33,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ANCIENT_TOMB"),
        name: "AncientTomb_Layout",
        width: 17,
        height: 33,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_UNDERWATER_ROUTE134"),
        name: "Underwater_Route134_Layout",
        width: 18,
        height: 10,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Underwater",
    },
    MapLayout {
        id: LayoutId("LAYOUT_UNDERWATER_SEALED_CHAMBER"),
        name: "Underwater_SealedChamber_Layout",
        width: 22,
        height: 48,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Underwater",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SEALED_CHAMBER_OUTER_ROOM"),
        name: "SealedChamber_OuterRoom_Layout",
        width: 21,
        height: 23,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_VICTORY_ROAD_B1F"),
        name: "VictoryRoad_B1F_Layout",
        width: 46,
        height: 31,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_VICTORY_ROAD_B2F"),
        name: "VictoryRoad_B2F_Layout",
        width: 46,
        height: 31,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE104_PROTOTYPE"),
        name: "Route104_Prototype_Layout",
        width: 1,
        height: 1,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Rustboro",
    },
    MapLayout {
        id: LayoutId("LAYOUT_GRANITE_CAVE_STEVENS_ROOM"),
        name: "GraniteCave_StevensRoom_Layout",
        width: 15,
        height: 14,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ABANDONED_SHIP_HIDDEN_FLOOR_CORRIDORS"),
        name: "AbandonedShip_HiddenFloorCorridors_Layout",
        width: 13,
        height: 11,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_InsideShip",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SOUTHERN_ISLAND_EXTERIOR"),
        name: "SouthernIsland_Exterior_Layout",
        width: 33,
        height: 30,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Rustboro",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SOUTHERN_ISLAND_INTERIOR"),
        name: "SouthernIsland_Interior_Layout",
        width: 27,
        height: 24,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Rustboro",
    },
    MapLayout {
        id: LayoutId("LAYOUT_JAGGED_PASS"),
        name: "JaggedPass_Layout",
        width: 30,
        height: 46,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Lavaridge",
    },
    MapLayout {
        id: LayoutId("LAYOUT_FIERY_PATH"),
        name: "FieryPath_Layout",
        width: 35,
        height: 38,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Lavaridge",
    },
    MapLayout {
        id: LayoutId("LAYOUT_RUSTBORO_CITY_FLAT2_1F"),
        name: "RustboroCity_Flat2_1F_Layout",
        width: 14,
        height: 9,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_GenericBuilding",
    },
    MapLayout {
        id: LayoutId("LAYOUT_RUSTBORO_CITY_FLAT2_2F"),
        name: "RustboroCity_Flat2_2F_Layout",
        width: 14,
        height: 9,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_GenericBuilding",
    },
    MapLayout {
        id: LayoutId("LAYOUT_RUSTBORO_CITY_FLAT2_3F"),
        name: "RustboroCity_Flat2_3F_Layout",
        width: 14,
        height: 9,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_GenericBuilding",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SOOTOPOLIS_CITY_LOTAD_AND_SEEDOT_HOUSE"),
        name: "SootopolisCity_LotadAndSeedotHouse_Layout",
        width: 8,
        height: 7,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_GenericBuilding",
    },
    MapLayout {
        id: LayoutId("LAYOUT_EVER_GRANDE_CITY_HALL_OF_FAME"),
        name: "EverGrandeCity_HallOfFame_Layout",
        width: 15,
        height: 17,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_CableClub",
    },
    MapLayout {
        id: LayoutId("LAYOUT_LILYCOVE_CITY_COVE_LILY_MOTEL_1F"),
        name: "LilycoveCity_CoveLilyMotel_1F_Layout",
        width: 12,
        height: 9,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_GenericBuilding",
    },
    MapLayout {
        id: LayoutId("LAYOUT_LILYCOVE_CITY_COVE_LILY_MOTEL_2F"),
        name: "LilycoveCity_CoveLilyMotel_2F_Layout",
        width: 12,
        height: 9,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_GenericBuilding",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE124_DIVING_TREASURE_HUNTERS_HOUSE"),
        name: "Route124_DivingTreasureHuntersHouse_Layout",
        width: 10,
        height: 9,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_GenericBuilding",
    },
    MapLayout {
        id: LayoutId("LAYOUT_MT_PYRE_EXTERIOR"),
        name: "MtPyre_Exterior_Layout",
        width: 38,
        height: 51,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Facility",
    },
    MapLayout {
        id: LayoutId("LAYOUT_MT_PYRE_SUMMIT"),
        name: "MtPyre_Summit_Layout",
        width: 50,
        height: 37,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Facility",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SEALED_CHAMBER_INNER_ROOM"),
        name: "SealedChamber_InnerRoom_Layout",
        width: 21,
        height: 23,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_MOSSDEEP_CITY_GAME_CORNER_1F"),
        name: "MossdeepCity_GameCorner_1F_Layout",
        width: 12,
        height: 10,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_MossdeepGameCorner",
    },
    MapLayout {
        id: LayoutId("LAYOUT_MOSSDEEP_CITY_GAME_CORNER_B1F"),
        name: "MossdeepCity_GameCorner_B1F_Layout",
        width: 12,
        height: 9,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_GenericBuilding",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SOOTOPOLIS_CITY_HOUSE1"),
        name: "SootopolisCity_House1_Layout",
        width: 8,
        height: 7,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_GenericBuilding",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SOOTOPOLIS_CITY_HOUSE2"),
        name: "SootopolisCity_House2_Layout",
        width: 8,
        height: 7,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_GenericBuilding",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SOOTOPOLIS_CITY_HOUSE3"),
        name: "SootopolisCity_House3_Layout",
        width: 8,
        height: 7,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_GenericBuilding",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ABANDONED_SHIP_HIDDEN_FLOOR_ROOMS"),
        name: "AbandonedShip_HiddenFloorRooms_Layout",
        width: 44,
        height: 15,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_InsideShip",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SCORCHED_SLAB"),
        name: "ScorchedSlab_Layout",
        width: 15,
        height: 20,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_CAVE_OF_ORIGIN_UNUSED_B4F_LAVA"),
        name: "CaveOfOrigin_Unused_B4F_Lava_Layout",
        width: 19,
        height: 19,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_RUSTBORO_CITY_FLAT1_1F"),
        name: "RustboroCity_Flat1_1F_Layout",
        width: 14,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_GenericBuilding",
    },
    MapLayout {
        id: LayoutId("LAYOUT_RUSTBORO_CITY_FLAT1_2F"),
        name: "RustboroCity_Flat1_2F_Layout",
        width: 14,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_GenericBuilding",
    },
    MapLayout {
        id: LayoutId("LAYOUT_EVER_GRANDE_CITY_HALL4"),
        name: "EverGrandeCity_Hall4_Layout",
        width: 11,
        height: 34,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_EliteFour",
    },
    MapLayout {
        id: LayoutId("LAYOUT_AQUA_HIDEOUT_UNUSED_RUBY_MAP1"),
        name: "AquaHideout_UnusedRubyMap1_Layout",
        width: 28,
        height: 30,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Facility",
    },
    MapLayout {
        id: LayoutId("LAYOUT_AQUA_HIDEOUT_UNUSED_RUBY_MAP2"),
        name: "AquaHideout_UnusedRubyMap2_Layout",
        width: 62,
        height: 24,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Facility",
    },
    MapLayout {
        id: LayoutId("LAYOUT_AQUA_HIDEOUT_UNUSED_RUBY_MAP3"),
        name: "AquaHideout_UnusedRubyMap3_Layout",
        width: 34,
        height: 24,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Facility",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE131_SKY_PILLAR"),
        name: "Route131_SkyPillar_Layout",
        width: 60,
        height: 40,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Pacifidlog",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SKY_PILLAR_ENTRANCE"),
        name: "SkyPillar_Entrance_Layout",
        width: 18,
        height: 18,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SKY_PILLAR_OUTSIDE"),
        name: "SkyPillar_Outside_Layout",
        width: 28,
        height: 23,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Pacifidlog",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SKY_PILLAR_1F"),
        name: "SkyPillar_1F_Layout",
        width: 14,
        height: 14,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Pacifidlog",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SKY_PILLAR_2F"),
        name: "SkyPillar_2F_Layout",
        width: 14,
        height: 14,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Pacifidlog",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SKY_PILLAR_3F"),
        name: "SkyPillar_3F_Layout",
        width: 14,
        height: 14,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Pacifidlog",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SKY_PILLAR_4F"),
        name: "SkyPillar_4F_Layout",
        width: 14,
        height: 14,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Pacifidlog",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SEAFLOOR_CAVERN_ROOM9_LAVA"),
        name: "SeafloorCavern_Room9_Lava_Layout",
        width: 27,
        height: 46,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_MOSSDEEP_CITY_STEVENS_HOUSE"),
        name: "MossdeepCity_StevensHouse_Layout",
        width: 11,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_GenericBuilding",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SHOAL_CAVE_LOW_TIDE_ICE_ROOM"),
        name: "ShoalCave_LowTideIceRoom_Layout",
        width: 20,
        height: 30,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SAFARI_ZONE_REST_HOUSE"),
        name: "SafariZone_RestHouse_Layout",
        width: 10,
        height: 9,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_GenericBuilding",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SKY_PILLAR_5F"),
        name: "SkyPillar_5F_Layout",
        width: 14,
        height: 14,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Pacifidlog",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SKY_PILLAR_TOP"),
        name: "SkyPillar_Top_Layout",
        width: 27,
        height: 24,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Pacifidlog",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_DOME_LOBBY"),
        name: "BattleFrontier_BattleDomeLobby_Layout",
        width: 23,
        height: 17,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattleDome",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_DOME_CORRIDOR"),
        name: "BattleFrontier_BattleDomeCorridor_Layout",
        width: 48,
        height: 7,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattleDome",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_DOME_PRE_BATTLE_ROOM"),
        name: "BattleFrontier_BattleDomePreBattleRoom_Layout",
        width: 9,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattleDome",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_DOME_BATTLE_ROOM"),
        name: "BattleFrontier_BattleDomeBattleRoom_Layout",
        width: 20,
        height: 10,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattleDome",
    },
    MapLayout {
        id: LayoutId("LAYOUT_MAGMA_HIDEOUT_1F"),
        name: "MagmaHideout_1F_Layout",
        width: 37,
        height: 38,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Lavaridge",
    },
    MapLayout {
        id: LayoutId("LAYOUT_MAGMA_HIDEOUT_2F_1R"),
        name: "MagmaHideout_2F_1R_Layout",
        width: 33,
        height: 39,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Lavaridge",
    },
    MapLayout {
        id: LayoutId("LAYOUT_MAGMA_HIDEOUT_2F_2R"),
        name: "MagmaHideout_2F_2R_Layout",
        width: 49,
        height: 28,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Lavaridge",
    },
    MapLayout {
        id: LayoutId("LAYOUT_MAGMA_HIDEOUT_3F_1R"),
        name: "MagmaHideout_3F_1R_Layout",
        width: 28,
        height: 24,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Lavaridge",
    },
    MapLayout {
        id: LayoutId("LAYOUT_MAGMA_HIDEOUT_3F_2R"),
        name: "MagmaHideout_3F_2R_Layout",
        width: 24,
        height: 17,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Lavaridge",
    },
    MapLayout {
        id: LayoutId("LAYOUT_MAGMA_HIDEOUT_4F"),
        name: "MagmaHideout_4F_Layout",
        width: 59,
        height: 28,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Lavaridge",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_PALACE_LOBBY"),
        name: "BattleFrontier_BattlePalaceLobby_Layout",
        width: 25,
        height: 12,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattlePalace",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_PALACE_CORRIDOR"),
        name: "BattleFrontier_BattlePalaceCorridor_Layout",
        width: 17,
        height: 14,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_BattlePalace",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_PALACE_BATTLE_ROOM"),
        name: "BattleFrontier_BattlePalaceBattleRoom_Layout",
        width: 15,
        height: 10,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_BattlePalace",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_FRONTIER_OUTSIDE_EAST"),
        name: "BattleFrontier_OutsideEast_Layout",
        width: 72,
        height: 72,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_BattleFrontierOutsideEast",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_FACTORY_LOBBY"),
        name: "BattleFrontier_BattleFactoryLobby_Layout",
        width: 19,
        height: 12,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattleFactory",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_FACTORY_PRE_BATTLE_ROOM"),
        name: "BattleFrontier_BattleFactoryPreBattleRoom_Layout",
        width: 17,
        height: 14,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattleFactory",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_FACTORY_BATTLE_ROOM"),
        name: "BattleFrontier_BattleFactoryBattleRoom_Layout",
        width: 13,
        height: 12,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattleFactory",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_PIKE_LOBBY"),
        name: "BattleFrontier_BattlePikeLobby_Layout",
        width: 11,
        height: 13,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattlePike",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_PIKE_CORRIDOR"),
        name: "BattleFrontier_BattlePikeCorridor_Layout",
        width: 14,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattlePike",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_PIKE_THREE_PATH_ROOM"),
        name: "BattleFrontier_BattlePikeThreePathRoom_Layout",
        width: 13,
        height: 11,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattlePike",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_PIKE_ROOM_NORMAL"),
        name: "BattleFrontier_BattlePikeRoomNormal_Layout",
        width: 9,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattlePike",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_PIKE_ROOM_FINAL"),
        name: "BattleFrontier_BattlePikeRoomFinal_Layout",
        width: 5,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattlePike",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_ARENA_LOBBY"),
        name: "BattleFrontier_BattleArenaLobby_Layout",
        width: 16,
        height: 13,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattleArena",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_ARENA_CORRIDOR"),
        name: "BattleFrontier_BattleArenaCorridor_Layout",
        width: 18,
        height: 14,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattleArena",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_ARENA_BATTLE_ROOM"),
        name: "BattleFrontier_BattleArenaBattleRoom_Layout",
        width: 16,
        height: 11,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattleArena",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SOOTOPOLIS_CITY_LEGENDS_BATTLE"),
        name: "SootopolisCity_LegendsBattle_Layout",
        width: 60,
        height: 60,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Sootopolis",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_PIKE_ROOM_WILD_MONS"),
        name: "BattleFrontier_BattlePikeRoomWildMons_Layout",
        width: 9,
        height: 20,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattlePike",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_PIKE_ROOM_UNUSED"),
        name: "BattleFrontier_BattlePikeRoomUnused_Layout",
        width: 1,
        height: 1,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattlePike",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_PYRAMID_LOBBY"),
        name: "BattleFrontier_BattlePyramidLobby_Layout",
        width: 15,
        height: 18,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattlePyramid",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_PYRAMID_FLOOR"),
        name: "BattleFrontier_BattlePyramidFloor_Layout",
        width: 8,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattlePyramid",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_PYRAMID_SQUARE01"),
        name: "BattlePyramidSquare01_Layout",
        width: 8,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattlePyramid",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_PYRAMID_SQUARE02"),
        name: "BattlePyramidSquare02_Layout",
        width: 8,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattlePyramid",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_PYRAMID_SQUARE03"),
        name: "BattlePyramidSquare03_Layout",
        width: 8,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattlePyramid",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_PYRAMID_SQUARE04"),
        name: "BattlePyramidSquare04_Layout",
        width: 8,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattlePyramid",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_PYRAMID_SQUARE05"),
        name: "BattlePyramidSquare05_Layout",
        width: 8,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattlePyramid",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_PYRAMID_SQUARE06"),
        name: "BattlePyramidSquare06_Layout",
        width: 8,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattlePyramid",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_PYRAMID_SQUARE07"),
        name: "BattlePyramidSquare07_Layout",
        width: 8,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattlePyramid",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_PYRAMID_SQUARE08"),
        name: "BattlePyramidSquare08_Layout",
        width: 8,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattlePyramid",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_PYRAMID_SQUARE09"),
        name: "BattlePyramidSquare09_Layout",
        width: 8,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattlePyramid",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_PYRAMID_SQUARE10"),
        name: "BattlePyramidSquare10_Layout",
        width: 8,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattlePyramid",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_PYRAMID_SQUARE11"),
        name: "BattlePyramidSquare11_Layout",
        width: 8,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattlePyramid",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_PYRAMID_SQUARE12"),
        name: "BattlePyramidSquare12_Layout",
        width: 8,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattlePyramid",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_PYRAMID_SQUARE13"),
        name: "BattlePyramidSquare13_Layout",
        width: 8,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattlePyramid",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_PYRAMID_SQUARE14"),
        name: "BattlePyramidSquare14_Layout",
        width: 8,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattlePyramid",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_PYRAMID_SQUARE15"),
        name: "BattlePyramidSquare15_Layout",
        width: 8,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattlePyramid",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_PYRAMID_SQUARE16"),
        name: "BattlePyramidSquare16_Layout",
        width: 8,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattlePyramid",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_PYRAMID_TOP"),
        name: "BattleFrontier_BattlePyramidTop_Layout",
        width: 34,
        height: 23,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattlePyramid",
    },
    MapLayout {
        id: LayoutId("LAYOUT_MAGMA_HIDEOUT_3F_3R"),
        name: "MagmaHideout_3F_3R_Layout",
        width: 33,
        height: 24,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Lavaridge",
    },
    MapLayout {
        id: LayoutId("LAYOUT_MAGMA_HIDEOUT_2F_3R"),
        name: "MagmaHideout_2F_3R_Layout",
        width: 60,
        height: 19,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Lavaridge",
    },
    MapLayout {
        id: LayoutId("LAYOUT_MIRAGE_TOWER_1F"),
        name: "MirageTower_1F_Layout",
        width: 21,
        height: 17,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_MirageTower",
    },
    MapLayout {
        id: LayoutId("LAYOUT_MIRAGE_TOWER_2F"),
        name: "MirageTower_2F_Layout",
        width: 21,
        height: 17,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_MirageTower",
    },
    MapLayout {
        id: LayoutId("LAYOUT_MIRAGE_TOWER_3F"),
        name: "MirageTower_3F_Layout",
        width: 21,
        height: 17,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_MirageTower",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_TENT_LOBBY"),
        name: "BattleTentLobby_Layout",
        width: 13,
        height: 10,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattleTent",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_TENT_CORRIDOR"),
        name: "BattleTentCorridor_Layout",
        width: 5,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattleTent",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_TENT_BATTLE_ROOM"),
        name: "BattleTentBattleRoom_Layout",
        width: 10,
        height: 9,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattleTent",
    },
    MapLayout {
        id: LayoutId("LAYOUT_VERDANTURF_TOWN_BATTLE_TENT_BATTLE_ROOM"),
        name: "VerdanturfTown_BattleTentBattleRoom_Layout",
        width: 13,
        height: 9,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_BattleTent",
    },
    MapLayout {
        id: LayoutId("LAYOUT_MIRAGE_TOWER_4F"),
        name: "MirageTower_4F_Layout",
        width: 13,
        height: 10,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_MirageTower",
    },
    MapLayout {
        id: LayoutId("LAYOUT_DESERT_UNDERPASS"),
        name: "DesertUnderpass_Layout",
        width: 139,
        height: 23,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_TOWER_MULTI_PARTNER_ROOM"),
        name: "BattleFrontier_BattleTowerMultiPartnerRoom_Layout",
        width: 21,
        height: 15,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattleFrontier",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_TOWER_MULTI_CORRIDOR"),
        name: "BattleFrontier_BattleTowerMultiCorridor_Layout",
        width: 17,
        height: 5,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattleFrontier",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ROUTE111_NO_MIRAGE_TOWER"),
        name: "Route111_NoMirageTower_Layout",
        width: 40,
        height: 140,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Mauville",
    },
    MapLayout {
        id: LayoutId("LAYOUT_UNION_ROOM"),
        name: "UnionRoom_Layout",
        width: 15,
        height: 12,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_UnionRoom",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SAFARI_ZONE_NORTHEAST"),
        name: "SafariZone_Northeast_Layout",
        width: 40,
        height: 40,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Lilycove",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SAFARI_ZONE_SOUTHEAST"),
        name: "SafariZone_Southeast_Layout",
        width: 40,
        height: 40,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Lilycove",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_FRONTIER_RANKING_HALL"),
        name: "BattleFrontier_RankingHall_Layout",
        width: 53,
        height: 15,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattleFrontierRankingHall",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_FRONTIER_LOUNGE1"),
        name: "BattleFrontier_Lounge1_Layout",
        width: 13,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattleFrontier",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_FRONTIER_EXCHANGE_SERVICE_CORNER"),
        name: "BattleFrontier_ExchangeServiceCorner_Layout",
        width: 15,
        height: 11,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattleFrontier",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_FRONTIER_RECEPTION_GATE"),
        name: "BattleFrontier_ReceptionGate_Layout",
        width: 9,
        height: 14,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_BattleFrontier",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ARTISAN_CAVE_B1F"),
        name: "ArtisanCave_B1F_Layout",
        width: 46,
        height: 54,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ARTISAN_CAVE_1F"),
        name: "ArtisanCave_1F_Layout",
        width: 21,
        height: 22,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_FARAWAY_ISLAND_ENTRANCE"),
        name: "FarawayIsland_Entrance_Layout",
        width: 34,
        height: 46,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Rustboro",
    },
    MapLayout {
        id: LayoutId("LAYOUT_FARAWAY_ISLAND_INTERIOR"),
        name: "FarawayIsland_Interior_Layout",
        width: 29,
        height: 26,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Fortree",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BIRTH_ISLAND_EXTERIOR"),
        name: "BirthIsland_Exterior_Layout",
        width: 30,
        height: 30,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Dewford",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ISLAND_HARBOR"),
        name: "IslandHarbor_Layout",
        width: 17,
        height: 13,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_IslandHarbor",
    },
    MapLayout {
        id: LayoutId("LAYOUT_UNDERWATER_MARINE_CAVE"),
        name: "Underwater_MarineCave_Layout",
        width: 20,
        height: 10,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Underwater",
    },
    MapLayout {
        id: LayoutId("LAYOUT_MARINE_CAVE_ENTRANCE"),
        name: "MarineCave_Entrance_Layout",
        width: 20,
        height: 20,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_TERRA_CAVE_ENTRANCE"),
        name: "TerraCave_Entrance_Layout",
        width: 20,
        height: 20,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_TERRA_CAVE_END"),
        name: "TerraCave_End_Layout",
        width: 27,
        height: 30,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_UNDERWATER_ROUTE105"),
        name: "Underwater_Route105_Layout",
        width: 40,
        height: 80,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Underwater",
    },
    MapLayout {
        id: LayoutId("LAYOUT_UNDERWATER_ROUTE125"),
        name: "Underwater_Route125_Layout",
        width: 80,
        height: 40,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Underwater",
    },
    MapLayout {
        id: LayoutId("LAYOUT_UNDERWATER_ROUTE129"),
        name: "Underwater_Route129_Layout",
        width: 80,
        height: 40,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Underwater",
    },
    MapLayout {
        id: LayoutId("LAYOUT_MARINE_CAVE_END"),
        name: "MarineCave_End_Layout",
        width: 27,
        height: 30,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_TRAINER_HILL_ENTRANCE"),
        name: "TrainerHill_Entrance_Layout",
        width: 19,
        height: 17,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_TrainerHill",
    },
    MapLayout {
        id: LayoutId("LAYOUT_TRAINER_HILL_1F"),
        name: "TrainerHill_1F_Layout",
        width: 16,
        height: 21,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_TrainerHill",
    },
    MapLayout {
        id: LayoutId("LAYOUT_TRAINER_HILL_2F"),
        name: "TrainerHill_2F_Layout",
        width: 16,
        height: 21,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_TrainerHill",
    },
    MapLayout {
        id: LayoutId("LAYOUT_TRAINER_HILL_3F"),
        name: "TrainerHill_3F_Layout",
        width: 16,
        height: 21,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_TrainerHill",
    },
    MapLayout {
        id: LayoutId("LAYOUT_TRAINER_HILL_4F"),
        name: "TrainerHill_4F_Layout",
        width: 16,
        height: 21,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_TrainerHill",
    },
    MapLayout {
        id: LayoutId("LAYOUT_TRAINER_HILL_ROOF"),
        name: "TrainerHill_Roof_Layout",
        width: 25,
        height: 16,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_TrainerHill",
    },
    MapLayout {
        id: LayoutId("LAYOUT_ALTERING_CAVE"),
        name: "AlteringCave_Layout",
        width: 32,
        height: 24,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Cave",
    },
    MapLayout {
        id: LayoutId("LAYOUT_NAVEL_ROCK_EXTERIOR"),
        name: "NavelRock_Exterior_Layout",
        width: 21,
        height: 24,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Dewford",
    },
    MapLayout {
        id: LayoutId("LAYOUT_NAVEL_ROCK_ENTRANCE"),
        name: "NavelRock_Entrance_Layout",
        width: 21,
        height: 32,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_NavelRock",
    },
    MapLayout {
        id: LayoutId("LAYOUT_NAVEL_ROCK_TOP"),
        name: "NavelRock_Top_Layout",
        width: 25,
        height: 28,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_NavelRock",
    },
    MapLayout {
        id: LayoutId("LAYOUT_NAVEL_ROCK_BOTTOM"),
        name: "NavelRock_Bottom_Layout",
        width: 22,
        height: 22,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_NavelRock",
    },
    MapLayout {
        id: LayoutId("LAYOUT_NAVEL_ROCK_LADDER_ROOM1"),
        name: "NavelRock_LadderRoom1_Layout",
        width: 9,
        height: 8,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_NavelRock",
    },
    MapLayout {
        id: LayoutId("LAYOUT_NAVEL_ROCK_LADDER_ROOM2"),
        name: "NavelRock_LadderRoom2_Layout",
        width: 9,
        height: 8,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_NavelRock",
    },
    MapLayout {
        id: LayoutId("LAYOUT_NAVEL_ROCK_B1F"),
        name: "NavelRock_B1F_Layout",
        width: 23,
        height: 11,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_NavelRock",
    },
    MapLayout {
        id: LayoutId("LAYOUT_NAVEL_ROCK_FORK"),
        name: "NavelRock_Fork_Layout",
        width: 27,
        height: 86,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_NavelRock",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_FRONTIER_LOUNGE2"),
        name: "BattleFrontier_Lounge2_Layout",
        width: 9,
        height: 10,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattleFrontier",
    },
    MapLayout {
        id: LayoutId("LAYOUT_BATTLE_FRONTIER_SCOTTS_HOUSE"),
        name: "BattleFrontier_ScottsHouse_Layout",
        width: 6,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BattleFrontier",
    },
    MapLayout {
        id: LayoutId("LAYOUT_METEOR_FALLS_STEVENS_CAVE"),
        name: "MeteorFalls_StevensCave_Layout",
        width: 30,
        height: 32,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_MeteorFalls",
    },
    MapLayout {
        id: LayoutId("LAYOUT_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB_WITH_TABLE"),
        name: "LittlerootTown_ProfessorBirchsLabWithTable_Layout",
        width: 13,
        height: 13,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_Lab",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SKY_PILLAR_1F_CLEAN"),
        name: "SkyPillar_1F_Clean_Layout",
        width: 14,
        height: 14,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Pacifidlog",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SKY_PILLAR_2F_CLEAN"),
        name: "SkyPillar_2F_Clean_Layout",
        width: 14,
        height: 14,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Pacifidlog",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SKY_PILLAR_3F_CLEAN"),
        name: "SkyPillar_3F_Clean_Layout",
        width: 14,
        height: 14,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Pacifidlog",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SKY_PILLAR_4F_CLEAN"),
        name: "SkyPillar_4F_Clean_Layout",
        width: 14,
        height: 14,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Pacifidlog",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SKY_PILLAR_5F_CLEAN"),
        name: "SkyPillar_5F_Clean_Layout",
        width: 14,
        height: 14,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Pacifidlog",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SKY_PILLAR_TOP_CLEAN"),
        name: "SkyPillar_Top_Clean_Layout",
        width: 27,
        height: 24,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_Pacifidlog",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SOOTOPOLIS_CITY_MYSTERY_EVENTS_HOUSE_1F"),
        name: "SootopolisCity_MysteryEventsHouse_1F_Layout",
        width: 11,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_MysteryEventsHouse",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SOOTOPOLIS_CITY_MYSTERY_EVENTS_HOUSE_B1F"),
        name: "SootopolisCity_MysteryEventsHouse_B1F_Layout",
        width: 12,
        height: 9,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_MysteryEventsHouse",
    },
    MapLayout {
        id: LayoutId("LAYOUT_SOOTOPOLIS_CITY_MYSTERY_EVENTS_HOUSE_1F_STAIRS_UNBLOCKED"),
        name: "SootopolisCity_MysteryEventsHouse_1F_StairsUnblocked_Layout",
        width: 11,
        height: 8,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_MysteryEventsHouse",
    },
];

/// Read-only access to every canonical map layout.
#[derive(Debug, Clone, Copy)]
pub struct LayoutTable {
    layouts: &'static [MapLayout; LAYOUT_COUNT],
}

impl LayoutTable {
    /// Number of entries in the table.
    pub const LEN: usize = LAYOUT_COUNT;

    /// Returns the canonical layout table.
    #[must_use]
    pub const fn new() -> Self {
        Self { layouts: &LAYOUTS }
    }

    /// Returns the layout with `id`, if present.
    #[must_use]
    pub fn get(&self, id: LayoutId) -> Option<&'static MapLayout> {
        self.layouts.iter().find(|layout| layout.id == id)
    }

    /// Returns the layout with `id`.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownLayout`] when `id` is absent.
    pub fn layout(&self, id: LayoutId) -> Result<&'static MapLayout, AssetError> {
        self.get(id).ok_or(AssetError::UnknownLayout(id.0))
    }

    /// Iterates over layouts in canonical order.
    pub fn iter(&self) -> impl Iterator<Item = &'static MapLayout> {
        self.layouts.iter()
    }

    /// Returns the number of layouts.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.layouts.len()
    }

    /// Returns whether the table contains no layouts.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.layouts.is_empty()
    }
}

impl Default for LayoutTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BorderGrid, LayoutGrid, LayoutId, LayoutTable, MapLayout, MetatileCell, BORDER_BYTE_LEN,
        BYTES_PER_METATILE_CELL, LAYOUT_COUNT, NO_TILESET,
    };
    use crate::error::AssetError;
    use std::collections::HashSet;
    use std::fmt::Write as _;

    const TEST_LAYOUT_ID: LayoutId = LayoutId("LAYOUT_TEST");
    const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    const CANONICAL_LAYOUTS_FINGERPRINT: u64 = 0xbc08_5cb0_ec14_7ac3;

    fn test_layout(width: u16, height: u16) -> MapLayout {
        MapLayout {
            id: TEST_LAYOUT_ID,
            name: "Test_Layout",
            width,
            height,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        }
    }

    const fn cell(metatile_id: u16, collision: u8, elevation: u8) -> MetatileCell {
        MetatileCell {
            metatile_id,
            collision,
            elevation,
        }
    }

    fn encode_cells(cells: &[MetatileCell]) -> Vec<u8> {
        cells
            .iter()
            .flat_map(|cell| cell.pack().to_le_bytes())
            .collect()
    }

    fn layout_table_fingerprint(table: LayoutTable) -> u64 {
        let mut rendered = String::new();
        for layout in table.iter() {
            writeln!(
                rendered,
                "{}|{}|{}x{}|{}|{}",
                layout.id.name(),
                layout.name,
                layout.width,
                layout.height,
                layout.primary_tileset,
                layout.secondary_tileset
            )
            .unwrap();
        }
        rendered.bytes().fold(FNV_OFFSET_BASIS, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(FNV_PRIME)
        })
    }

    #[test]
    fn layout_table_has_the_canonical_shape() {
        let table = LayoutTable::new();
        assert_eq!(LAYOUT_COUNT, 441);
        assert_eq!(table.len(), LAYOUT_COUNT);
        assert_eq!(LayoutTable::LEN, LAYOUT_COUNT);
        assert_eq!(table.iter().count(), LAYOUT_COUNT);
        assert!(!table.is_empty());

        let mut ids = HashSet::new();
        let mut names = HashSet::new();
        for layout in table.iter() {
            assert!(layout.id.name().starts_with("LAYOUT_"));
            assert!(
                ids.insert(layout.id),
                "duplicate layout id: {:?}",
                layout.id
            );
            assert!(layout.name.ends_with("_Layout"));
            assert!(
                names.insert(layout.name),
                "duplicate layout name: {}",
                layout.name
            );
            assert!(layout.width > 0, "zero-width layout: {}", layout.name);
            assert!(layout.height > 0, "zero-height layout: {}", layout.name);
            assert!(layout.primary_tileset.starts_with("gTileset_"));
            assert!(
                layout.secondary_tileset == NO_TILESET
                    || layout.secondary_tileset.starts_with("gTileset_")
            );
        }
    }

    #[test]
    fn whole_layout_table_matches_the_canonical_fingerprint() {
        assert_eq!(
            layout_table_fingerprint(LayoutTable::new()),
            CANONICAL_LAYOUTS_FINGERPRINT
        );
    }

    #[test]
    fn littleroot_town_layout_matches_canonical_metadata() {
        let table = LayoutTable::new();
        let layout = table.layout(LayoutId("LAYOUT_LITTLEROOT_TOWN")).unwrap();
        assert_eq!(layout.name, "LittlerootTown_Layout");
        assert_eq!(layout.width, 20);
        assert_eq!(layout.height, 20);
        assert_eq!(layout.primary_tileset, "gTileset_General");
        assert_eq!(layout.secondary_tileset, "gTileset_Petalburg");
        assert_eq!(layout.cell_count(), 400);
    }

    #[test]
    fn petalburg_city_layout_matches_canonical_metadata() {
        let table = LayoutTable::new();
        let layout = table.layout(LayoutId("LAYOUT_PETALBURG_CITY")).unwrap();
        assert_eq!(layout.width, 30);
        assert_eq!(layout.height, 30);
        assert_eq!(layout.secondary_tileset, "gTileset_Petalburg");
    }

    #[test]
    fn unknown_layout_is_an_error() {
        let table = LayoutTable::new();
        assert_eq!(table.get(LayoutId("LAYOUT_NOT_REAL")), None);
        assert_eq!(
            table.layout(LayoutId("LAYOUT_NOT_REAL")),
            Err(AssetError::UnknownLayout("LAYOUT_NOT_REAL")),
        );
    }

    #[test]
    fn metatile_cell_round_trips_through_pack() {
        for packed in u16::MIN..=u16::MAX {
            assert_eq!(MetatileCell::from_raw(packed).pack(), packed);
        }
    }

    #[test]
    fn layout_grid_decodes_row_major_little_endian_cells() {
        let layout = test_layout(2, 2);
        let bytes = [0x01, 0x10, 0x02, 0x24, 0x03, 0x0C, 0xFF, 0xF3];
        let expected = [
            cell(1, 0, 1),
            cell(2, 1, 2),
            cell(3, 3, 0),
            cell(1023, 0, 15),
        ];
        let grid = LayoutGrid::new(&layout, &bytes).unwrap();
        assert_eq!(grid.cell_count(), expected.len());
        assert_eq!(grid.cells().collect::<Vec<_>>(), expected);
        assert_eq!(grid.cell_at(0, 0), Some(expected[0]));
        assert_eq!(grid.cell_at(1, 0), Some(expected[1]));
        assert_eq!(grid.cell_at(0, 1), Some(expected[2]));
        assert_eq!(grid.cell_at(1, 1), Some(expected[3]));
    }

    #[test]
    fn layout_grid_cell_at_out_of_bounds_is_none() {
        let layout = test_layout(2, 2);
        let bytes = encode_cells(&[cell(0, 0, 0); 4]);
        let grid = LayoutGrid::new(&layout, &bytes).unwrap();
        assert_eq!(grid.cell_at(2, 0), None);
        assert_eq!(grid.cell_at(0, 2), None);
    }

    #[test]
    fn layout_grid_accepts_trailing_padding() {
        let layout = test_layout(1, 1);
        let expected_cell = cell(5, 0, 0);
        let mut bytes = encode_cells(&[expected_cell]);
        bytes.extend_from_slice(&[u8::MAX; BYTES_PER_METATILE_CELL * 2]);
        let grid = LayoutGrid::new(&layout, &bytes).unwrap();
        assert_eq!(grid.cell_count(), 1);
        assert_eq!(grid.cells().count(), 1);
        assert_eq!(grid.cell_at(0, 0), Some(expected_cell));
    }

    #[test]
    fn layout_grid_rejects_short_buffer() {
        let layout = test_layout(2, 2);
        let expected = layout.cell_count() * BYTES_PER_METATILE_CELL;
        let bytes = vec![0; expected - 1];
        let err = LayoutGrid::new(&layout, &bytes).unwrap_err();
        assert_eq!(
            err,
            AssetError::LayoutGridTooShort(TEST_LAYOUT_ID.name(), expected, bytes.len())
        );
    }

    #[test]
    fn layout_grid_exact_length_is_accepted() {
        let layout = test_layout(3, 2);
        let bytes = vec![0; layout.cell_count() * BYTES_PER_METATILE_CELL];
        assert!(LayoutGrid::new(&layout, &bytes).is_ok());
    }

    #[test]
    fn border_grid_repeats_its_shifted_two_by_two_pattern() {
        let expected = [cell(1, 0, 0), cell(2, 0, 0), cell(3, 0, 0), cell(4, 0, 0)];
        let bytes = encode_cells(&expected);
        let grid = BorderGrid::new(&bytes).unwrap();
        let cells: Vec<_> = grid.cells().collect();
        assert_eq!(cells, expected);
        assert_eq!(grid.cell_at(0, 0), cells[3]);
        assert_eq!(grid.cell_at(1, 0), cells[2]);
        assert_eq!(grid.cell_at(0, 1), cells[1]);
        assert_eq!(grid.cell_at(1, 1), cells[0]);
    }

    #[test]
    fn border_grid_cell_at_does_not_overflow_at_extreme_coordinates() {
        let expected = [cell(1, 0, 0), cell(2, 0, 0), cell(3, 0, 0), cell(4, 0, 0)];
        let bytes = encode_cells(&expected);
        let grid = BorderGrid::new(&bytes).unwrap();
        let equivalent_coordinates = [(i32::MIN, 0), (i32::MAX, 1)];

        for (x, equivalent_x) in equivalent_coordinates {
            for (y, equivalent_y) in equivalent_coordinates {
                assert_eq!(grid.cell_at(x, y), grid.cell_at(equivalent_x, equivalent_y));
            }
        }
    }

    #[test]
    fn border_grid_rejects_wrong_size() {
        for byte_len in [0, BORDER_BYTE_LEN - 1, BORDER_BYTE_LEN + 1] {
            let bytes = vec![0; byte_len];
            assert_eq!(
                BorderGrid::new(&bytes).unwrap_err(),
                AssetError::LayoutBorderWrongSize(byte_len)
            );
        }
    }

    #[test]
    fn border_grid_exact_byte_length_is_accepted() {
        assert!(BorderGrid::new(&[0u8; BORDER_BYTE_LEN]).is_ok());
    }

    #[test]
    fn map_layout_grid_convenience_method_matches_layout_grid_new() {
        let layout = test_layout(2, 1);
        let bytes = encode_cells(&[cell(1, 0, 0), cell(2, 0, 0)]);
        let via_method: Vec<_> = layout.grid(&bytes).unwrap().cells().collect();
        let via_ctor: Vec<_> = LayoutGrid::new(&layout, &bytes).unwrap().cells().collect();
        assert_eq!(via_method, via_ctor);
    }
}
