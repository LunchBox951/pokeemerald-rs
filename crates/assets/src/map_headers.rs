//! Map headers, groups, and connections (S-4): the `gMapGroups` /
//! `gMapHeaders` / `gMapConnections` tables.
//!
//! Ports every map's header metadata and its inter-map connections from the
//! upstream reference `pokeemerald/data/maps/map_groups.json` (the
//! `MAP_GROUP`/`MAP_NUM` position table, 34 groups / 518 maps) plus each
//! map's own `pokeemerald/data/maps/<Name>/map.json` (header fields +
//! `connections`; the `struct MapHeader` layout is
//! `pokeemerald/include/global.h`). Object/warp/coord/bg *events* — the rest
//! of `map.json` — are explicitly **not** extracted here; that is issue
//! #77's scope, and the module docs make no claim over that data.
//!
//! **`(group, num)`, honestly derived.** `wild_encounters`' [`MapId`]
//! (reused here rather than redefined — see below) deferred a numeric
//! `mapGroup`/`mapNum` pair because deriving it required this exact
//! group/position table, which didn't exist in this workspace yet. It does
//! now: `map_groups.json`'s `group_order` array gives the upstream
//! `MAP_GROUP(map)` index, and each named group's array gives
//! `MAP_NUM(map)` (its position within that group). [`MapHeader::group`] /
//! [`MapHeader::num`] carry these, cross-checked at extraction time against
//! every map directory appearing in exactly one group exactly once (518 in,
//! 518 out, no duplicates — verified by a structural test below).
//!
//! **Reusing [`MapId`].** Map headers and `gWildMonHeaders` entries name the
//! same `MAP_*` id space, so this module imports
//! [`wild_encounters::MapId`](crate::wild_encounters::MapId) rather than
//! defining a second, incompatible "map id" type.
//!
//! **Fields without an extracted owning table get an opaque id, not a
//! number.** Three fields reference tables this workspace hasn't extracted
//! yet:
//! - `music`: [`MusicId`] wraps the *numeric* `MUS_*` id, resolved from
//!   `pokeemerald/include/constants/songs.h` at extraction time (that header
//!   is self-contained and small enough to resolve safely without owning
//!   the full audio-song table this crate doesn't have yet).
//! - `region_map_section`: [`RegionMapSectionId`] wraps the `MAPSEC_*`
//!   *name* — no `constants/region_map_sections.h` exists in this reference
//!   checkout (the ids live in `src/data/region_map/region_map_sections.json`,
//!   a separate region-map slice), so, per the same reasoning as
//!   [`MapId`](crate::wild_encounters::MapId), the symbolic name is
//!   transcribed rather than a guessed number.
//! - `layout`: [`crate::map_layouts::LayoutId`] (already a name-wrapping
//!   newtype for the same reason — see that module).
//!
//! **Fields with a complete, self-contained upstream enum become real Rust
//! enums**, not opaque ids: [`Weather`] (`constants/weather.h`), [`MapType`]
//! / [`BattleScene`] (`constants/map_types.h`), and [`Direction`]
//! (`CONNECTION_*`, `constants/global.h`). Each has a `from_id`/`id` pair
//! mirroring [`type_chart::Type`](crate::type_chart::Type).
//!
//! **Field renames.** Upstream's `allow_cycling` / `allow_escaping` /
//! `allow_running` / `show_map_name` JSON keys are transcribed as
//! `allow_bike` / `allow_escape` / `allow_run` / `show_name` (the issue's
//! own wording) — same booleans, no upstream `struct MapHeader` field name
//! to preserve exactly since Porymap's `map.json` is JSON, not the compiled
//! struct.
//!
//! **`shared_events_map` / `shared_scripts_map` are events/scripts
//! metadata**, not header metadata — 50 maps have one — and are left for
//! issue #77 alongside the event lists themselves, exactly like
//! `object_events`/`warp_events`/`coord_events`/`bg_events`.
//!
//! **Re-running the extraction.** As with `map_layouts` (and, before it,
//! `wild_encounters`/`trainers`), this module's transcribed table was
//! produced by a development-time-only script, not checked into the
//! workspace. To regenerate: walk `map_groups.json`'s `group_order`, and for
//! each named group's map list (in array order) open that map's
//! `data/maps/<Name>/map.json` and transcribe `id`, `name`, `layout`,
//! `music` (resolved against `songs.h`), `region_map_section`,
//! `requires_flash`, `weather`, `map_type`, `allow_cycling`/`allow_escaping`/
//! `allow_running`/`show_map_name` (renamed per above), `battle_scene`, and
//! `connections` (each entry's `direction` string mapped `up`->`North`,
//! `down`->`South`, `left`->`West`, `right`->`East`, `dive`->`Dive`,
//! `emerge`->`Emerge`, matching `CONNECTION_*`) into one [`MapHeader`]
//! literal, plus a [`MapGroup`] listing per group — both emitted in
//! `group_order` / group-array order, giving [`MapHeader::group`] /
//! [`MapHeader::num`] their meaning directly from position.
//!
//! The upstream-tie tests at the bottom pin Petalburg City (music id,
//! connections, flags), Route 101's connections (Oldale Town north,
//! Littleroot Town south — *not* a direct Route 103 connection; the two
//! only meet by way of Oldale Town, see the test doc comment), and a
//! structural round-trip of the whole group/position table.

use crate::error::AssetError;
use crate::map_layouts::LayoutId;
use crate::wild_encounters::MapId;

/// The number of per-map header entries (one per `data/maps/*/map.json`).
pub const MAP_COUNT: usize = 518;
/// The number of upstream `gMapGroup_*` arrays (`map_groups.json`'s
/// `group_order` length).
pub const MAP_GROUP_COUNT: usize = 34;

/// A background music track id — the upstream numeric `MUS_*` value
/// (resolved from `constants/songs.h`; see the module docs for why this is
/// a number rather than the [`MapId`]-style name wrapper).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MusicId(pub u16);

impl MusicId {
    /// The raw upstream `MUS_*` id.
    #[must_use]
    pub const fn id(self) -> u16 {
        self.0
    }
}

/// A region-map section id — the upstream `MAPSEC_*` name (see the module
/// docs for why this wraps the symbolic name, not a number).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RegionMapSectionId(pub &'static str);

impl RegionMapSectionId {
    /// The upstream `MAPSEC_*` name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        self.0
    }
}

/// A map's persistent weather effect, matching the upstream `WEATHER_*`
/// identifiers (`pokeemerald/include/constants/weather.h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Weather {
    None = 0,
    SunnyClouds = 1,
    Sunny = 2,
    Rain = 3,
    Snow = 4,
    RainThunderstorm = 5,
    FogHorizontal = 6,
    VolcanicAsh = 7,
    Sandstorm = 8,
    FogDiagonal = 9,
    Underwater = 10,
    Shade = 11,
    Drought = 12,
    Downpour = 13,
    UnderwaterBubbles = 14,
    Abnormal = 15,
    Route119Cycle = 20,
    Route123Cycle = 21,
}

impl Weather {
    /// The upstream `WEATHER_*` id for this weather.
    #[must_use]
    pub const fn id(self) -> u8 {
        self as u8
    }

    /// Resolve an upstream `WEATHER_*` id into a [`Weather`].
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownWeather`] if `id` is not one of the
    /// eighteen modelled values.
    pub const fn from_id(id: u8) -> Result<Self, AssetError> {
        match id {
            0 => Ok(Self::None),
            1 => Ok(Self::SunnyClouds),
            2 => Ok(Self::Sunny),
            3 => Ok(Self::Rain),
            4 => Ok(Self::Snow),
            5 => Ok(Self::RainThunderstorm),
            6 => Ok(Self::FogHorizontal),
            7 => Ok(Self::VolcanicAsh),
            8 => Ok(Self::Sandstorm),
            9 => Ok(Self::FogDiagonal),
            10 => Ok(Self::Underwater),
            11 => Ok(Self::Shade),
            12 => Ok(Self::Drought),
            13 => Ok(Self::Downpour),
            14 => Ok(Self::UnderwaterBubbles),
            15 => Ok(Self::Abnormal),
            20 => Ok(Self::Route119Cycle),
            21 => Ok(Self::Route123Cycle),
            other => Err(AssetError::UnknownWeather(other)),
        }
    }
}

/// A map's terrain/UI classification, matching the upstream `MAP_TYPE_*`
/// identifiers (`pokeemerald/include/constants/map_types.h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum MapType {
    None = 0,
    Town = 1,
    City = 2,
    Route = 3,
    Underground = 4,
    Underwater = 5,
    OceanRoute = 6,
    /// `MAP_TYPE_UNKNOWN` — defined upstream but not used by any map.
    Unknown = 7,
    Indoor = 8,
    SecretBase = 9,
}

impl MapType {
    /// The upstream `MAP_TYPE_*` id for this type.
    #[must_use]
    pub const fn id(self) -> u8 {
        self as u8
    }

    /// Resolve an upstream `MAP_TYPE_*` id into a [`MapType`].
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownMapType`] if `id` is not one of the ten
    /// modelled values.
    pub const fn from_id(id: u8) -> Result<Self, AssetError> {
        match id {
            0 => Ok(Self::None),
            1 => Ok(Self::Town),
            2 => Ok(Self::City),
            3 => Ok(Self::Route),
            4 => Ok(Self::Underground),
            5 => Ok(Self::Underwater),
            6 => Ok(Self::OceanRoute),
            7 => Ok(Self::Unknown),
            8 => Ok(Self::Indoor),
            9 => Ok(Self::SecretBase),
            other => Err(AssetError::UnknownMapType(other)),
        }
    }
}

/// A trainer-battle backdrop override for battles fought on this map,
/// matching the upstream `MAP_BATTLE_SCENE_*` identifiers
/// (`pokeemerald/include/constants/map_types.h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum BattleScene {
    Normal = 0,
    Gym = 1,
    Magma = 2,
    Aqua = 3,
    Sidney = 4,
    Phoebe = 5,
    Glacia = 6,
    Drake = 7,
    Frontier = 8,
}

impl BattleScene {
    /// The upstream `MAP_BATTLE_SCENE_*` id for this scene.
    #[must_use]
    pub const fn id(self) -> u8 {
        self as u8
    }

    /// Resolve an upstream `MAP_BATTLE_SCENE_*` id into a [`BattleScene`].
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownBattleScene`] if `id` is not one of the
    /// nine modelled values.
    pub const fn from_id(id: u8) -> Result<Self, AssetError> {
        match id {
            0 => Ok(Self::Normal),
            1 => Ok(Self::Gym),
            2 => Ok(Self::Magma),
            3 => Ok(Self::Aqua),
            4 => Ok(Self::Sidney),
            5 => Ok(Self::Phoebe),
            6 => Ok(Self::Glacia),
            7 => Ok(Self::Drake),
            8 => Ok(Self::Frontier),
            other => Err(AssetError::UnknownBattleScene(other)),
        }
    }
}

/// The direction a [`MapConnection`] joins two maps in, matching the
/// upstream `CONNECTION_*` identifiers (`pokeemerald/include/constants/global.h`;
/// `CONNECTION_NONE`/`CONNECTION_INVALID` are sentinels with no connection
/// entry to represent and are not modelled).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Direction {
    South = 1,
    North = 2,
    West = 3,
    East = 4,
    Dive = 5,
    Emerge = 6,
}

impl Direction {
    /// The upstream `CONNECTION_*` id for this direction.
    #[must_use]
    pub const fn id(self) -> u8 {
        self as u8
    }

    /// Resolve an upstream `CONNECTION_*` id into a [`Direction`].
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownConnectionDirection`] if `id` is not one
    /// of the six modelled directions.
    pub const fn from_id(id: u8) -> Result<Self, AssetError> {
        match id {
            1 => Ok(Self::South),
            2 => Ok(Self::North),
            3 => Ok(Self::West),
            4 => Ok(Self::East),
            5 => Ok(Self::Dive),
            6 => Ok(Self::Emerge),
            other => Err(AssetError::UnknownConnectionDirection(other)),
        }
    }
}

/// One connection from a map to a neighbour — the owned form of upstream
/// `struct MapConnection`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapConnection {
    /// Which edge of the map this connection joins from (upstream
    /// `direction`).
    pub direction: Direction,
    /// The neighbour's offset, in metatiles, along the shared edge
    /// (upstream `offset`); can be negative.
    pub offset: i32,
    /// The neighbouring map (upstream `mapGroup`/`mapNum`, resolved here to
    /// the target's [`MapId`]).
    pub target: MapId,
}

/// One `gMapGroup_*` array: the ordered list of maps in one upstream map
/// group. Position within [`MAP_GROUPS`] is `MAP_GROUP(map)`; position
/// within [`maps`](MapGroup::maps) is `MAP_NUM(map)`.
#[derive(Debug, Clone, Copy)]
pub struct MapGroup {
    /// The upstream `gMapGroup_*` symbol (e.g. `"gMapGroup_TownsAndRoutes"`).
    pub label: &'static str,
    /// The maps in this group, in `MAP_NUM` order.
    pub maps: &'static [MapId],
}

/// One map's header metadata and connections — the owned form of upstream
/// `struct MapHeader` (object/warp/coord/bg events excluded; see the module
/// docs).
// Five independent flags, matching upstream's own `struct MapHeader`
// (`requiresFlash`/`allowCycling`/`allowEscaping`/`allowRunning`/
// `showMapName` are five separate bitfields there too) — they don't share
// enough structure to collapse into a state machine or enum.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapHeader {
    /// This map's id (upstream `MAP_*` name).
    pub id: MapId,
    /// This map's `MAP_GROUP` index: position in [`MAP_GROUPS`].
    pub group: u8,
    /// This map's `MAP_NUM` index: position in its group's
    /// [`MapGroup::maps`].
    pub num: u8,
    /// The upstream Porymap label (upstream `name`, e.g. `"PetalburgCity"`).
    pub name: &'static str,
    /// This map's layout (upstream `layout`).
    pub layout: LayoutId,
    /// This map's background music (upstream `music`).
    pub music: MusicId,
    /// This map's region-map section (upstream `regionMapSectionId`).
    pub region_map_section: RegionMapSectionId,
    /// Whether the map requires Flash to see (upstream `requiresFlash`).
    pub requires_flash: bool,
    /// This map's persistent weather (upstream `weather`).
    pub weather: Weather,
    /// This map's terrain/UI classification (upstream `mapType`).
    pub map_type: MapType,
    /// Whether the player may use the bike here (upstream `allowCycling`).
    pub allow_bike: bool,
    /// Whether the player may use Fly/Dig/Escape Rope here (upstream
    /// `allowEscaping`).
    pub allow_escape: bool,
    /// Whether the player may run here (upstream `allowRunning`).
    pub allow_run: bool,
    /// Whether entering the map pops up its name banner (upstream
    /// `showMapName`).
    pub show_name: bool,
    /// The battle backdrop override for battles on this map (upstream
    /// `battleType`/`mapBattleScene`).
    pub battle_scene: BattleScene,
    /// This map's connections to neighbouring maps (upstream `connections`;
    /// empty when the map has none, e.g. most indoor maps).
    pub connections: &'static [MapConnection],
}

// --- GENERATED: transcribed from pokeemerald/data/maps/map_groups.json and
// pokeemerald/data/maps/*/map.json ---

static MAP_GROUPS: [MapGroup; 34] = [
    MapGroup {
        label: "gMapGroup_TownsAndRoutes",
        maps: &[
            MapId("MAP_PETALBURG_CITY"),
            MapId("MAP_SLATEPORT_CITY"),
            MapId("MAP_MAUVILLE_CITY"),
            MapId("MAP_RUSTBORO_CITY"),
            MapId("MAP_FORTREE_CITY"),
            MapId("MAP_LILYCOVE_CITY"),
            MapId("MAP_MOSSDEEP_CITY"),
            MapId("MAP_SOOTOPOLIS_CITY"),
            MapId("MAP_EVER_GRANDE_CITY"),
            MapId("MAP_LITTLEROOT_TOWN"),
            MapId("MAP_OLDALE_TOWN"),
            MapId("MAP_DEWFORD_TOWN"),
            MapId("MAP_LAVARIDGE_TOWN"),
            MapId("MAP_FALLARBOR_TOWN"),
            MapId("MAP_VERDANTURF_TOWN"),
            MapId("MAP_PACIFIDLOG_TOWN"),
            MapId("MAP_ROUTE101"),
            MapId("MAP_ROUTE102"),
            MapId("MAP_ROUTE103"),
            MapId("MAP_ROUTE104"),
            MapId("MAP_ROUTE105"),
            MapId("MAP_ROUTE106"),
            MapId("MAP_ROUTE107"),
            MapId("MAP_ROUTE108"),
            MapId("MAP_ROUTE109"),
            MapId("MAP_ROUTE110"),
            MapId("MAP_ROUTE111"),
            MapId("MAP_ROUTE112"),
            MapId("MAP_ROUTE113"),
            MapId("MAP_ROUTE114"),
            MapId("MAP_ROUTE115"),
            MapId("MAP_ROUTE116"),
            MapId("MAP_ROUTE117"),
            MapId("MAP_ROUTE118"),
            MapId("MAP_ROUTE119"),
            MapId("MAP_ROUTE120"),
            MapId("MAP_ROUTE121"),
            MapId("MAP_ROUTE122"),
            MapId("MAP_ROUTE123"),
            MapId("MAP_ROUTE124"),
            MapId("MAP_ROUTE125"),
            MapId("MAP_ROUTE126"),
            MapId("MAP_ROUTE127"),
            MapId("MAP_ROUTE128"),
            MapId("MAP_ROUTE129"),
            MapId("MAP_ROUTE130"),
            MapId("MAP_ROUTE131"),
            MapId("MAP_ROUTE132"),
            MapId("MAP_ROUTE133"),
            MapId("MAP_ROUTE134"),
            MapId("MAP_UNDERWATER_ROUTE124"),
            MapId("MAP_UNDERWATER_ROUTE126"),
            MapId("MAP_UNDERWATER_ROUTE127"),
            MapId("MAP_UNDERWATER_ROUTE128"),
            MapId("MAP_UNDERWATER_ROUTE129"),
            MapId("MAP_UNDERWATER_ROUTE105"),
            MapId("MAP_UNDERWATER_ROUTE125"),
        ],
    },
    MapGroup {
        label: "gMapGroup_IndoorLittleroot",
        maps: &[
            MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F"),
            MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F"),
            MapId("MAP_LITTLEROOT_TOWN_MAYS_HOUSE_1F"),
            MapId("MAP_LITTLEROOT_TOWN_MAYS_HOUSE_2F"),
            MapId("MAP_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB"),
        ],
    },
    MapGroup {
        label: "gMapGroup_IndoorOldale",
        maps: &[
            MapId("MAP_OLDALE_TOWN_HOUSE1"),
            MapId("MAP_OLDALE_TOWN_HOUSE2"),
            MapId("MAP_OLDALE_TOWN_POKEMON_CENTER_1F"),
            MapId("MAP_OLDALE_TOWN_POKEMON_CENTER_2F"),
            MapId("MAP_OLDALE_TOWN_MART"),
        ],
    },
    MapGroup {
        label: "gMapGroup_IndoorDewford",
        maps: &[
            MapId("MAP_DEWFORD_TOWN_HOUSE1"),
            MapId("MAP_DEWFORD_TOWN_POKEMON_CENTER_1F"),
            MapId("MAP_DEWFORD_TOWN_POKEMON_CENTER_2F"),
            MapId("MAP_DEWFORD_TOWN_GYM"),
            MapId("MAP_DEWFORD_TOWN_HALL"),
            MapId("MAP_DEWFORD_TOWN_HOUSE2"),
        ],
    },
    MapGroup {
        label: "gMapGroup_IndoorLavaridge",
        maps: &[
            MapId("MAP_LAVARIDGE_TOWN_HERB_SHOP"),
            MapId("MAP_LAVARIDGE_TOWN_GYM_1F"),
            MapId("MAP_LAVARIDGE_TOWN_GYM_B1F"),
            MapId("MAP_LAVARIDGE_TOWN_HOUSE"),
            MapId("MAP_LAVARIDGE_TOWN_MART"),
            MapId("MAP_LAVARIDGE_TOWN_POKEMON_CENTER_1F"),
            MapId("MAP_LAVARIDGE_TOWN_POKEMON_CENTER_2F"),
        ],
    },
    MapGroup {
        label: "gMapGroup_IndoorFallarbor",
        maps: &[
            MapId("MAP_FALLARBOR_TOWN_MART"),
            MapId("MAP_FALLARBOR_TOWN_BATTLE_TENT_LOBBY"),
            MapId("MAP_FALLARBOR_TOWN_BATTLE_TENT_CORRIDOR"),
            MapId("MAP_FALLARBOR_TOWN_BATTLE_TENT_BATTLE_ROOM"),
            MapId("MAP_FALLARBOR_TOWN_POKEMON_CENTER_1F"),
            MapId("MAP_FALLARBOR_TOWN_POKEMON_CENTER_2F"),
            MapId("MAP_FALLARBOR_TOWN_COZMOS_HOUSE"),
            MapId("MAP_FALLARBOR_TOWN_MOVE_RELEARNERS_HOUSE"),
        ],
    },
    MapGroup {
        label: "gMapGroup_IndoorVerdanturf",
        maps: &[
            MapId("MAP_VERDANTURF_TOWN_BATTLE_TENT_LOBBY"),
            MapId("MAP_VERDANTURF_TOWN_BATTLE_TENT_CORRIDOR"),
            MapId("MAP_VERDANTURF_TOWN_BATTLE_TENT_BATTLE_ROOM"),
            MapId("MAP_VERDANTURF_TOWN_MART"),
            MapId("MAP_VERDANTURF_TOWN_POKEMON_CENTER_1F"),
            MapId("MAP_VERDANTURF_TOWN_POKEMON_CENTER_2F"),
            MapId("MAP_VERDANTURF_TOWN_WANDAS_HOUSE"),
            MapId("MAP_VERDANTURF_TOWN_FRIENDSHIP_RATERS_HOUSE"),
            MapId("MAP_VERDANTURF_TOWN_HOUSE"),
        ],
    },
    MapGroup {
        label: "gMapGroup_IndoorPacifidlog",
        maps: &[
            MapId("MAP_PACIFIDLOG_TOWN_POKEMON_CENTER_1F"),
            MapId("MAP_PACIFIDLOG_TOWN_POKEMON_CENTER_2F"),
            MapId("MAP_PACIFIDLOG_TOWN_HOUSE1"),
            MapId("MAP_PACIFIDLOG_TOWN_HOUSE2"),
            MapId("MAP_PACIFIDLOG_TOWN_HOUSE3"),
            MapId("MAP_PACIFIDLOG_TOWN_HOUSE4"),
            MapId("MAP_PACIFIDLOG_TOWN_HOUSE5"),
        ],
    },
    MapGroup {
        label: "gMapGroup_IndoorPetalburg",
        maps: &[
            MapId("MAP_PETALBURG_CITY_WALLYS_HOUSE"),
            MapId("MAP_PETALBURG_CITY_GYM"),
            MapId("MAP_PETALBURG_CITY_HOUSE1"),
            MapId("MAP_PETALBURG_CITY_HOUSE2"),
            MapId("MAP_PETALBURG_CITY_POKEMON_CENTER_1F"),
            MapId("MAP_PETALBURG_CITY_POKEMON_CENTER_2F"),
            MapId("MAP_PETALBURG_CITY_MART"),
        ],
    },
    MapGroup {
        label: "gMapGroup_IndoorSlateport",
        maps: &[
            MapId("MAP_SLATEPORT_CITY_STERNS_SHIPYARD_1F"),
            MapId("MAP_SLATEPORT_CITY_STERNS_SHIPYARD_2F"),
            MapId("MAP_SLATEPORT_CITY_BATTLE_TENT_LOBBY"),
            MapId("MAP_SLATEPORT_CITY_BATTLE_TENT_CORRIDOR"),
            MapId("MAP_SLATEPORT_CITY_BATTLE_TENT_BATTLE_ROOM"),
            MapId("MAP_SLATEPORT_CITY_NAME_RATERS_HOUSE"),
            MapId("MAP_SLATEPORT_CITY_POKEMON_FAN_CLUB"),
            MapId("MAP_SLATEPORT_CITY_OCEANIC_MUSEUM_1F"),
            MapId("MAP_SLATEPORT_CITY_OCEANIC_MUSEUM_2F"),
            MapId("MAP_SLATEPORT_CITY_HARBOR"),
            MapId("MAP_SLATEPORT_CITY_HOUSE"),
            MapId("MAP_SLATEPORT_CITY_POKEMON_CENTER_1F"),
            MapId("MAP_SLATEPORT_CITY_POKEMON_CENTER_2F"),
            MapId("MAP_SLATEPORT_CITY_MART"),
        ],
    },
    MapGroup {
        label: "gMapGroup_IndoorMauville",
        maps: &[
            MapId("MAP_MAUVILLE_CITY_GYM"),
            MapId("MAP_MAUVILLE_CITY_BIKE_SHOP"),
            MapId("MAP_MAUVILLE_CITY_HOUSE1"),
            MapId("MAP_MAUVILLE_CITY_GAME_CORNER"),
            MapId("MAP_MAUVILLE_CITY_HOUSE2"),
            MapId("MAP_MAUVILLE_CITY_POKEMON_CENTER_1F"),
            MapId("MAP_MAUVILLE_CITY_POKEMON_CENTER_2F"),
            MapId("MAP_MAUVILLE_CITY_MART"),
        ],
    },
    MapGroup {
        label: "gMapGroup_IndoorRustboro",
        maps: &[
            MapId("MAP_RUSTBORO_CITY_DEVON_CORP_1F"),
            MapId("MAP_RUSTBORO_CITY_DEVON_CORP_2F"),
            MapId("MAP_RUSTBORO_CITY_DEVON_CORP_3F"),
            MapId("MAP_RUSTBORO_CITY_GYM"),
            MapId("MAP_RUSTBORO_CITY_POKEMON_SCHOOL"),
            MapId("MAP_RUSTBORO_CITY_POKEMON_CENTER_1F"),
            MapId("MAP_RUSTBORO_CITY_POKEMON_CENTER_2F"),
            MapId("MAP_RUSTBORO_CITY_MART"),
            MapId("MAP_RUSTBORO_CITY_FLAT1_1F"),
            MapId("MAP_RUSTBORO_CITY_FLAT1_2F"),
            MapId("MAP_RUSTBORO_CITY_HOUSE1"),
            MapId("MAP_RUSTBORO_CITY_CUTTERS_HOUSE"),
            MapId("MAP_RUSTBORO_CITY_HOUSE2"),
            MapId("MAP_RUSTBORO_CITY_FLAT2_1F"),
            MapId("MAP_RUSTBORO_CITY_FLAT2_2F"),
            MapId("MAP_RUSTBORO_CITY_FLAT2_3F"),
            MapId("MAP_RUSTBORO_CITY_HOUSE3"),
        ],
    },
    MapGroup {
        label: "gMapGroup_IndoorFortree",
        maps: &[
            MapId("MAP_FORTREE_CITY_HOUSE1"),
            MapId("MAP_FORTREE_CITY_GYM"),
            MapId("MAP_FORTREE_CITY_POKEMON_CENTER_1F"),
            MapId("MAP_FORTREE_CITY_POKEMON_CENTER_2F"),
            MapId("MAP_FORTREE_CITY_MART"),
            MapId("MAP_FORTREE_CITY_HOUSE2"),
            MapId("MAP_FORTREE_CITY_HOUSE3"),
            MapId("MAP_FORTREE_CITY_HOUSE4"),
            MapId("MAP_FORTREE_CITY_HOUSE5"),
            MapId("MAP_FORTREE_CITY_DECORATION_SHOP"),
        ],
    },
    MapGroup {
        label: "gMapGroup_IndoorLilycove",
        maps: &[
            MapId("MAP_LILYCOVE_CITY_COVE_LILY_MOTEL_1F"),
            MapId("MAP_LILYCOVE_CITY_COVE_LILY_MOTEL_2F"),
            MapId("MAP_LILYCOVE_CITY_LILYCOVE_MUSEUM_1F"),
            MapId("MAP_LILYCOVE_CITY_LILYCOVE_MUSEUM_2F"),
            MapId("MAP_LILYCOVE_CITY_CONTEST_LOBBY"),
            MapId("MAP_LILYCOVE_CITY_CONTEST_HALL"),
            MapId("MAP_LILYCOVE_CITY_POKEMON_CENTER_1F"),
            MapId("MAP_LILYCOVE_CITY_POKEMON_CENTER_2F"),
            MapId("MAP_LILYCOVE_CITY_UNUSED_MART"),
            MapId("MAP_LILYCOVE_CITY_POKEMON_TRAINER_FAN_CLUB"),
            MapId("MAP_LILYCOVE_CITY_HARBOR"),
            MapId("MAP_LILYCOVE_CITY_MOVE_DELETERS_HOUSE"),
            MapId("MAP_LILYCOVE_CITY_HOUSE1"),
            MapId("MAP_LILYCOVE_CITY_HOUSE2"),
            MapId("MAP_LILYCOVE_CITY_HOUSE3"),
            MapId("MAP_LILYCOVE_CITY_HOUSE4"),
            MapId("MAP_LILYCOVE_CITY_DEPARTMENT_STORE_1F"),
            MapId("MAP_LILYCOVE_CITY_DEPARTMENT_STORE_2F"),
            MapId("MAP_LILYCOVE_CITY_DEPARTMENT_STORE_3F"),
            MapId("MAP_LILYCOVE_CITY_DEPARTMENT_STORE_4F"),
            MapId("MAP_LILYCOVE_CITY_DEPARTMENT_STORE_5F"),
            MapId("MAP_LILYCOVE_CITY_DEPARTMENT_STORE_ROOFTOP"),
            MapId("MAP_LILYCOVE_CITY_DEPARTMENT_STORE_ELEVATOR"),
        ],
    },
    MapGroup {
        label: "gMapGroup_IndoorMossdeep",
        maps: &[
            MapId("MAP_MOSSDEEP_CITY_GYM"),
            MapId("MAP_MOSSDEEP_CITY_HOUSE1"),
            MapId("MAP_MOSSDEEP_CITY_HOUSE2"),
            MapId("MAP_MOSSDEEP_CITY_POKEMON_CENTER_1F"),
            MapId("MAP_MOSSDEEP_CITY_POKEMON_CENTER_2F"),
            MapId("MAP_MOSSDEEP_CITY_MART"),
            MapId("MAP_MOSSDEEP_CITY_HOUSE3"),
            MapId("MAP_MOSSDEEP_CITY_STEVENS_HOUSE"),
            MapId("MAP_MOSSDEEP_CITY_HOUSE4"),
            MapId("MAP_MOSSDEEP_CITY_SPACE_CENTER_1F"),
            MapId("MAP_MOSSDEEP_CITY_SPACE_CENTER_2F"),
            MapId("MAP_MOSSDEEP_CITY_GAME_CORNER_1F"),
            MapId("MAP_MOSSDEEP_CITY_GAME_CORNER_B1F"),
        ],
    },
    MapGroup {
        label: "gMapGroup_IndoorSootopolis",
        maps: &[
            MapId("MAP_SOOTOPOLIS_CITY_GYM_1F"),
            MapId("MAP_SOOTOPOLIS_CITY_GYM_B1F"),
            MapId("MAP_SOOTOPOLIS_CITY_POKEMON_CENTER_1F"),
            MapId("MAP_SOOTOPOLIS_CITY_POKEMON_CENTER_2F"),
            MapId("MAP_SOOTOPOLIS_CITY_MART"),
            MapId("MAP_SOOTOPOLIS_CITY_HOUSE1"),
            MapId("MAP_SOOTOPOLIS_CITY_HOUSE2"),
            MapId("MAP_SOOTOPOLIS_CITY_HOUSE3"),
            MapId("MAP_SOOTOPOLIS_CITY_HOUSE4"),
            MapId("MAP_SOOTOPOLIS_CITY_HOUSE5"),
            MapId("MAP_SOOTOPOLIS_CITY_HOUSE6"),
            MapId("MAP_SOOTOPOLIS_CITY_HOUSE7"),
            MapId("MAP_SOOTOPOLIS_CITY_LOTAD_AND_SEEDOT_HOUSE"),
            MapId("MAP_SOOTOPOLIS_CITY_MYSTERY_EVENTS_HOUSE_1F"),
            MapId("MAP_SOOTOPOLIS_CITY_MYSTERY_EVENTS_HOUSE_B1F"),
        ],
    },
    MapGroup {
        label: "gMapGroup_IndoorEverGrande",
        maps: &[
            MapId("MAP_EVER_GRANDE_CITY_SIDNEYS_ROOM"),
            MapId("MAP_EVER_GRANDE_CITY_PHOEBES_ROOM"),
            MapId("MAP_EVER_GRANDE_CITY_GLACIAS_ROOM"),
            MapId("MAP_EVER_GRANDE_CITY_DRAKES_ROOM"),
            MapId("MAP_EVER_GRANDE_CITY_CHAMPIONS_ROOM"),
            MapId("MAP_EVER_GRANDE_CITY_HALL1"),
            MapId("MAP_EVER_GRANDE_CITY_HALL2"),
            MapId("MAP_EVER_GRANDE_CITY_HALL3"),
            MapId("MAP_EVER_GRANDE_CITY_HALL4"),
            MapId("MAP_EVER_GRANDE_CITY_HALL5"),
            MapId("MAP_EVER_GRANDE_CITY_POKEMON_LEAGUE_1F"),
            MapId("MAP_EVER_GRANDE_CITY_HALL_OF_FAME"),
            MapId("MAP_EVER_GRANDE_CITY_POKEMON_CENTER_1F"),
            MapId("MAP_EVER_GRANDE_CITY_POKEMON_CENTER_2F"),
            MapId("MAP_EVER_GRANDE_CITY_POKEMON_LEAGUE_2F"),
        ],
    },
    MapGroup {
        label: "gMapGroup_IndoorRoute104",
        maps: &[
            MapId("MAP_ROUTE104_MR_BRINEYS_HOUSE"),
            MapId("MAP_ROUTE104_PRETTY_PETAL_FLOWER_SHOP"),
        ],
    },
    MapGroup {
        label: "gMapGroup_IndoorRoute111",
        maps: &[
            MapId("MAP_ROUTE111_WINSTRATE_FAMILYS_HOUSE"),
            MapId("MAP_ROUTE111_OLD_LADYS_REST_STOP"),
        ],
    },
    MapGroup {
        label: "gMapGroup_IndoorRoute112",
        maps: &[
            MapId("MAP_ROUTE112_CABLE_CAR_STATION"),
            MapId("MAP_MT_CHIMNEY_CABLE_CAR_STATION"),
        ],
    },
    MapGroup {
        label: "gMapGroup_IndoorRoute114",
        maps: &[
            MapId("MAP_ROUTE114_FOSSIL_MANIACS_HOUSE"),
            MapId("MAP_ROUTE114_FOSSIL_MANIACS_TUNNEL"),
            MapId("MAP_ROUTE114_LANETTES_HOUSE"),
        ],
    },
    MapGroup {
        label: "gMapGroup_IndoorRoute116",
        maps: &[MapId("MAP_ROUTE116_TUNNELERS_REST_HOUSE")],
    },
    MapGroup {
        label: "gMapGroup_IndoorRoute117",
        maps: &[MapId("MAP_ROUTE117_POKEMON_DAY_CARE")],
    },
    MapGroup {
        label: "gMapGroup_IndoorRoute121",
        maps: &[MapId("MAP_ROUTE121_SAFARI_ZONE_ENTRANCE")],
    },
    MapGroup {
        label: "gMapGroup_Dungeons",
        maps: &[
            MapId("MAP_METEOR_FALLS_1F_1R"),
            MapId("MAP_METEOR_FALLS_1F_2R"),
            MapId("MAP_METEOR_FALLS_B1F_1R"),
            MapId("MAP_METEOR_FALLS_B1F_2R"),
            MapId("MAP_RUSTURF_TUNNEL"),
            MapId("MAP_UNDERWATER_SOOTOPOLIS_CITY"),
            MapId("MAP_DESERT_RUINS"),
            MapId("MAP_GRANITE_CAVE_1F"),
            MapId("MAP_GRANITE_CAVE_B1F"),
            MapId("MAP_GRANITE_CAVE_B2F"),
            MapId("MAP_GRANITE_CAVE_STEVENS_ROOM"),
            MapId("MAP_PETALBURG_WOODS"),
            MapId("MAP_MT_CHIMNEY"),
            MapId("MAP_JAGGED_PASS"),
            MapId("MAP_FIERY_PATH"),
            MapId("MAP_MT_PYRE_1F"),
            MapId("MAP_MT_PYRE_2F"),
            MapId("MAP_MT_PYRE_3F"),
            MapId("MAP_MT_PYRE_4F"),
            MapId("MAP_MT_PYRE_5F"),
            MapId("MAP_MT_PYRE_6F"),
            MapId("MAP_MT_PYRE_EXTERIOR"),
            MapId("MAP_MT_PYRE_SUMMIT"),
            MapId("MAP_AQUA_HIDEOUT_1F"),
            MapId("MAP_AQUA_HIDEOUT_B1F"),
            MapId("MAP_AQUA_HIDEOUT_B2F"),
            MapId("MAP_UNDERWATER_SEAFLOOR_CAVERN"),
            MapId("MAP_SEAFLOOR_CAVERN_ENTRANCE"),
            MapId("MAP_SEAFLOOR_CAVERN_ROOM1"),
            MapId("MAP_SEAFLOOR_CAVERN_ROOM2"),
            MapId("MAP_SEAFLOOR_CAVERN_ROOM3"),
            MapId("MAP_SEAFLOOR_CAVERN_ROOM4"),
            MapId("MAP_SEAFLOOR_CAVERN_ROOM5"),
            MapId("MAP_SEAFLOOR_CAVERN_ROOM6"),
            MapId("MAP_SEAFLOOR_CAVERN_ROOM7"),
            MapId("MAP_SEAFLOOR_CAVERN_ROOM8"),
            MapId("MAP_SEAFLOOR_CAVERN_ROOM9"),
            MapId("MAP_CAVE_OF_ORIGIN_ENTRANCE"),
            MapId("MAP_CAVE_OF_ORIGIN_1F"),
            MapId("MAP_CAVE_OF_ORIGIN_UNUSED_RUBY_SAPPHIRE_MAP1"),
            MapId("MAP_CAVE_OF_ORIGIN_UNUSED_RUBY_SAPPHIRE_MAP2"),
            MapId("MAP_CAVE_OF_ORIGIN_UNUSED_RUBY_SAPPHIRE_MAP3"),
            MapId("MAP_CAVE_OF_ORIGIN_B1F"),
            MapId("MAP_VICTORY_ROAD_1F"),
            MapId("MAP_VICTORY_ROAD_B1F"),
            MapId("MAP_VICTORY_ROAD_B2F"),
            MapId("MAP_SHOAL_CAVE_LOW_TIDE_ENTRANCE_ROOM"),
            MapId("MAP_SHOAL_CAVE_LOW_TIDE_INNER_ROOM"),
            MapId("MAP_SHOAL_CAVE_LOW_TIDE_STAIRS_ROOM"),
            MapId("MAP_SHOAL_CAVE_LOW_TIDE_LOWER_ROOM"),
            MapId("MAP_SHOAL_CAVE_HIGH_TIDE_ENTRANCE_ROOM"),
            MapId("MAP_SHOAL_CAVE_HIGH_TIDE_INNER_ROOM"),
            MapId("MAP_NEW_MAUVILLE_ENTRANCE"),
            MapId("MAP_NEW_MAUVILLE_INSIDE"),
            MapId("MAP_ABANDONED_SHIP_DECK"),
            MapId("MAP_ABANDONED_SHIP_CORRIDORS_1F"),
            MapId("MAP_ABANDONED_SHIP_ROOMS_1F"),
            MapId("MAP_ABANDONED_SHIP_CORRIDORS_B1F"),
            MapId("MAP_ABANDONED_SHIP_ROOMS_B1F"),
            MapId("MAP_ABANDONED_SHIP_ROOMS2_B1F"),
            MapId("MAP_ABANDONED_SHIP_UNDERWATER1"),
            MapId("MAP_ABANDONED_SHIP_ROOM_B1F"),
            MapId("MAP_ABANDONED_SHIP_ROOMS2_1F"),
            MapId("MAP_ABANDONED_SHIP_CAPTAINS_OFFICE"),
            MapId("MAP_ABANDONED_SHIP_UNDERWATER2"),
            MapId("MAP_ABANDONED_SHIP_HIDDEN_FLOOR_CORRIDORS"),
            MapId("MAP_ABANDONED_SHIP_HIDDEN_FLOOR_ROOMS"),
            MapId("MAP_ISLAND_CAVE"),
            MapId("MAP_ANCIENT_TOMB"),
            MapId("MAP_UNDERWATER_ROUTE134"),
            MapId("MAP_UNDERWATER_SEALED_CHAMBER"),
            MapId("MAP_SEALED_CHAMBER_OUTER_ROOM"),
            MapId("MAP_SEALED_CHAMBER_INNER_ROOM"),
            MapId("MAP_SCORCHED_SLAB"),
            MapId("MAP_AQUA_HIDEOUT_UNUSED_RUBY_MAP1"),
            MapId("MAP_AQUA_HIDEOUT_UNUSED_RUBY_MAP2"),
            MapId("MAP_AQUA_HIDEOUT_UNUSED_RUBY_MAP3"),
            MapId("MAP_SKY_PILLAR_ENTRANCE"),
            MapId("MAP_SKY_PILLAR_OUTSIDE"),
            MapId("MAP_SKY_PILLAR_1F"),
            MapId("MAP_SKY_PILLAR_2F"),
            MapId("MAP_SKY_PILLAR_3F"),
            MapId("MAP_SKY_PILLAR_4F"),
            MapId("MAP_SHOAL_CAVE_LOW_TIDE_ICE_ROOM"),
            MapId("MAP_SKY_PILLAR_5F"),
            MapId("MAP_SKY_PILLAR_TOP"),
            MapId("MAP_MAGMA_HIDEOUT_1F"),
            MapId("MAP_MAGMA_HIDEOUT_2F_1R"),
            MapId("MAP_MAGMA_HIDEOUT_2F_2R"),
            MapId("MAP_MAGMA_HIDEOUT_3F_1R"),
            MapId("MAP_MAGMA_HIDEOUT_3F_2R"),
            MapId("MAP_MAGMA_HIDEOUT_4F"),
            MapId("MAP_MAGMA_HIDEOUT_3F_3R"),
            MapId("MAP_MAGMA_HIDEOUT_2F_3R"),
            MapId("MAP_MIRAGE_TOWER_1F"),
            MapId("MAP_MIRAGE_TOWER_2F"),
            MapId("MAP_MIRAGE_TOWER_3F"),
            MapId("MAP_MIRAGE_TOWER_4F"),
            MapId("MAP_DESERT_UNDERPASS"),
            MapId("MAP_ARTISAN_CAVE_B1F"),
            MapId("MAP_ARTISAN_CAVE_1F"),
            MapId("MAP_UNDERWATER_MARINE_CAVE"),
            MapId("MAP_MARINE_CAVE_ENTRANCE"),
            MapId("MAP_MARINE_CAVE_END"),
            MapId("MAP_TERRA_CAVE_ENTRANCE"),
            MapId("MAP_TERRA_CAVE_END"),
            MapId("MAP_ALTERING_CAVE"),
            MapId("MAP_METEOR_FALLS_STEVENS_CAVE"),
        ],
    },
    MapGroup {
        label: "gMapGroup_IndoorDynamic",
        maps: &[
            MapId("MAP_SECRET_BASE_RED_CAVE1"),
            MapId("MAP_SECRET_BASE_BROWN_CAVE1"),
            MapId("MAP_SECRET_BASE_BLUE_CAVE1"),
            MapId("MAP_SECRET_BASE_YELLOW_CAVE1"),
            MapId("MAP_SECRET_BASE_TREE1"),
            MapId("MAP_SECRET_BASE_SHRUB1"),
            MapId("MAP_SECRET_BASE_RED_CAVE2"),
            MapId("MAP_SECRET_BASE_BROWN_CAVE2"),
            MapId("MAP_SECRET_BASE_BLUE_CAVE2"),
            MapId("MAP_SECRET_BASE_YELLOW_CAVE2"),
            MapId("MAP_SECRET_BASE_TREE2"),
            MapId("MAP_SECRET_BASE_SHRUB2"),
            MapId("MAP_SECRET_BASE_RED_CAVE3"),
            MapId("MAP_SECRET_BASE_BROWN_CAVE3"),
            MapId("MAP_SECRET_BASE_BLUE_CAVE3"),
            MapId("MAP_SECRET_BASE_YELLOW_CAVE3"),
            MapId("MAP_SECRET_BASE_TREE3"),
            MapId("MAP_SECRET_BASE_SHRUB3"),
            MapId("MAP_SECRET_BASE_RED_CAVE4"),
            MapId("MAP_SECRET_BASE_BROWN_CAVE4"),
            MapId("MAP_SECRET_BASE_BLUE_CAVE4"),
            MapId("MAP_SECRET_BASE_YELLOW_CAVE4"),
            MapId("MAP_SECRET_BASE_TREE4"),
            MapId("MAP_SECRET_BASE_SHRUB4"),
            MapId("MAP_BATTLE_COLOSSEUM_2P"),
            MapId("MAP_TRADE_CENTER"),
            MapId("MAP_RECORD_CORNER"),
            MapId("MAP_BATTLE_COLOSSEUM_4P"),
            MapId("MAP_CONTEST_HALL"),
            MapId("MAP_UNUSED_CONTEST_HALL1"),
            MapId("MAP_UNUSED_CONTEST_HALL2"),
            MapId("MAP_UNUSED_CONTEST_HALL3"),
            MapId("MAP_UNUSED_CONTEST_HALL4"),
            MapId("MAP_UNUSED_CONTEST_HALL5"),
            MapId("MAP_UNUSED_CONTEST_HALL6"),
            MapId("MAP_CONTEST_HALL_BEAUTY"),
            MapId("MAP_CONTEST_HALL_TOUGH"),
            MapId("MAP_CONTEST_HALL_COOL"),
            MapId("MAP_CONTEST_HALL_SMART"),
            MapId("MAP_CONTEST_HALL_CUTE"),
            MapId("MAP_INSIDE_OF_TRUCK"),
            MapId("MAP_SS_TIDAL_CORRIDOR"),
            MapId("MAP_SS_TIDAL_LOWER_DECK"),
            MapId("MAP_SS_TIDAL_ROOMS"),
            MapId("MAP_BATTLE_PYRAMID_SQUARE01"),
            MapId("MAP_BATTLE_PYRAMID_SQUARE02"),
            MapId("MAP_BATTLE_PYRAMID_SQUARE03"),
            MapId("MAP_BATTLE_PYRAMID_SQUARE04"),
            MapId("MAP_BATTLE_PYRAMID_SQUARE05"),
            MapId("MAP_BATTLE_PYRAMID_SQUARE06"),
            MapId("MAP_BATTLE_PYRAMID_SQUARE07"),
            MapId("MAP_BATTLE_PYRAMID_SQUARE08"),
            MapId("MAP_BATTLE_PYRAMID_SQUARE09"),
            MapId("MAP_BATTLE_PYRAMID_SQUARE10"),
            MapId("MAP_BATTLE_PYRAMID_SQUARE11"),
            MapId("MAP_BATTLE_PYRAMID_SQUARE12"),
            MapId("MAP_BATTLE_PYRAMID_SQUARE13"),
            MapId("MAP_BATTLE_PYRAMID_SQUARE14"),
            MapId("MAP_BATTLE_PYRAMID_SQUARE15"),
            MapId("MAP_BATTLE_PYRAMID_SQUARE16"),
            MapId("MAP_UNION_ROOM"),
        ],
    },
    MapGroup {
        label: "gMapGroup_SpecialArea",
        maps: &[
            MapId("MAP_SAFARI_ZONE_NORTHWEST"),
            MapId("MAP_SAFARI_ZONE_NORTH"),
            MapId("MAP_SAFARI_ZONE_SOUTHWEST"),
            MapId("MAP_SAFARI_ZONE_SOUTH"),
            MapId("MAP_BATTLE_FRONTIER_OUTSIDE_WEST"),
            MapId("MAP_BATTLE_FRONTIER_BATTLE_TOWER_LOBBY"),
            MapId("MAP_BATTLE_FRONTIER_BATTLE_TOWER_ELEVATOR"),
            MapId("MAP_BATTLE_FRONTIER_BATTLE_TOWER_CORRIDOR"),
            MapId("MAP_BATTLE_FRONTIER_BATTLE_TOWER_BATTLE_ROOM"),
            MapId("MAP_SOUTHERN_ISLAND_EXTERIOR"),
            MapId("MAP_SOUTHERN_ISLAND_INTERIOR"),
            MapId("MAP_SAFARI_ZONE_REST_HOUSE"),
            MapId("MAP_SAFARI_ZONE_NORTHEAST"),
            MapId("MAP_SAFARI_ZONE_SOUTHEAST"),
            MapId("MAP_BATTLE_FRONTIER_OUTSIDE_EAST"),
            MapId("MAP_BATTLE_FRONTIER_BATTLE_TOWER_MULTI_PARTNER_ROOM"),
            MapId("MAP_BATTLE_FRONTIER_BATTLE_TOWER_MULTI_CORRIDOR"),
            MapId("MAP_BATTLE_FRONTIER_BATTLE_TOWER_MULTI_BATTLE_ROOM"),
            MapId("MAP_BATTLE_FRONTIER_BATTLE_DOME_LOBBY"),
            MapId("MAP_BATTLE_FRONTIER_BATTLE_DOME_CORRIDOR"),
            MapId("MAP_BATTLE_FRONTIER_BATTLE_DOME_PRE_BATTLE_ROOM"),
            MapId("MAP_BATTLE_FRONTIER_BATTLE_DOME_BATTLE_ROOM"),
            MapId("MAP_BATTLE_FRONTIER_BATTLE_PALACE_LOBBY"),
            MapId("MAP_BATTLE_FRONTIER_BATTLE_PALACE_CORRIDOR"),
            MapId("MAP_BATTLE_FRONTIER_BATTLE_PALACE_BATTLE_ROOM"),
            MapId("MAP_BATTLE_FRONTIER_BATTLE_PYRAMID_LOBBY"),
            MapId("MAP_BATTLE_FRONTIER_BATTLE_PYRAMID_FLOOR"),
            MapId("MAP_BATTLE_FRONTIER_BATTLE_PYRAMID_TOP"),
            MapId("MAP_BATTLE_FRONTIER_BATTLE_ARENA_LOBBY"),
            MapId("MAP_BATTLE_FRONTIER_BATTLE_ARENA_CORRIDOR"),
            MapId("MAP_BATTLE_FRONTIER_BATTLE_ARENA_BATTLE_ROOM"),
            MapId("MAP_BATTLE_FRONTIER_BATTLE_FACTORY_LOBBY"),
            MapId("MAP_BATTLE_FRONTIER_BATTLE_FACTORY_PRE_BATTLE_ROOM"),
            MapId("MAP_BATTLE_FRONTIER_BATTLE_FACTORY_BATTLE_ROOM"),
            MapId("MAP_BATTLE_FRONTIER_BATTLE_PIKE_LOBBY"),
            MapId("MAP_BATTLE_FRONTIER_BATTLE_PIKE_CORRIDOR"),
            MapId("MAP_BATTLE_FRONTIER_BATTLE_PIKE_THREE_PATH_ROOM"),
            MapId("MAP_BATTLE_FRONTIER_BATTLE_PIKE_ROOM_NORMAL"),
            MapId("MAP_BATTLE_FRONTIER_BATTLE_PIKE_ROOM_FINAL"),
            MapId("MAP_BATTLE_FRONTIER_BATTLE_PIKE_ROOM_WILD_MONS"),
            MapId("MAP_BATTLE_FRONTIER_RANKING_HALL"),
            MapId("MAP_BATTLE_FRONTIER_LOUNGE1"),
            MapId("MAP_BATTLE_FRONTIER_EXCHANGE_SERVICE_CORNER"),
            MapId("MAP_BATTLE_FRONTIER_LOUNGE2"),
            MapId("MAP_BATTLE_FRONTIER_LOUNGE3"),
            MapId("MAP_BATTLE_FRONTIER_LOUNGE4"),
            MapId("MAP_BATTLE_FRONTIER_SCOTTS_HOUSE"),
            MapId("MAP_BATTLE_FRONTIER_LOUNGE5"),
            MapId("MAP_BATTLE_FRONTIER_LOUNGE6"),
            MapId("MAP_BATTLE_FRONTIER_LOUNGE7"),
            MapId("MAP_BATTLE_FRONTIER_RECEPTION_GATE"),
            MapId("MAP_BATTLE_FRONTIER_LOUNGE8"),
            MapId("MAP_BATTLE_FRONTIER_LOUNGE9"),
            MapId("MAP_BATTLE_FRONTIER_POKEMON_CENTER_1F"),
            MapId("MAP_BATTLE_FRONTIER_POKEMON_CENTER_2F"),
            MapId("MAP_BATTLE_FRONTIER_MART"),
            MapId("MAP_FARAWAY_ISLAND_ENTRANCE"),
            MapId("MAP_FARAWAY_ISLAND_INTERIOR"),
            MapId("MAP_BIRTH_ISLAND_EXTERIOR"),
            MapId("MAP_BIRTH_ISLAND_HARBOR"),
            MapId("MAP_TRAINER_HILL_ENTRANCE"),
            MapId("MAP_TRAINER_HILL_1F"),
            MapId("MAP_TRAINER_HILL_2F"),
            MapId("MAP_TRAINER_HILL_3F"),
            MapId("MAP_TRAINER_HILL_4F"),
            MapId("MAP_TRAINER_HILL_ROOF"),
            MapId("MAP_NAVEL_ROCK_EXTERIOR"),
            MapId("MAP_NAVEL_ROCK_HARBOR"),
            MapId("MAP_NAVEL_ROCK_ENTRANCE"),
            MapId("MAP_NAVEL_ROCK_B1F"),
            MapId("MAP_NAVEL_ROCK_FORK"),
            MapId("MAP_NAVEL_ROCK_UP1"),
            MapId("MAP_NAVEL_ROCK_UP2"),
            MapId("MAP_NAVEL_ROCK_UP3"),
            MapId("MAP_NAVEL_ROCK_UP4"),
            MapId("MAP_NAVEL_ROCK_TOP"),
            MapId("MAP_NAVEL_ROCK_DOWN01"),
            MapId("MAP_NAVEL_ROCK_DOWN02"),
            MapId("MAP_NAVEL_ROCK_DOWN03"),
            MapId("MAP_NAVEL_ROCK_DOWN04"),
            MapId("MAP_NAVEL_ROCK_DOWN05"),
            MapId("MAP_NAVEL_ROCK_DOWN06"),
            MapId("MAP_NAVEL_ROCK_DOWN07"),
            MapId("MAP_NAVEL_ROCK_DOWN08"),
            MapId("MAP_NAVEL_ROCK_DOWN09"),
            MapId("MAP_NAVEL_ROCK_DOWN10"),
            MapId("MAP_NAVEL_ROCK_DOWN11"),
            MapId("MAP_NAVEL_ROCK_BOTTOM"),
            MapId("MAP_TRAINER_HILL_ELEVATOR"),
        ],
    },
    MapGroup {
        label: "gMapGroup_IndoorRoute104Prototype",
        maps: &[
            MapId("MAP_ROUTE104_PROTOTYPE"),
            MapId("MAP_ROUTE104_PROTOTYPE_PRETTY_PETAL_FLOWER_SHOP"),
        ],
    },
    MapGroup {
        label: "gMapGroup_IndoorRoute109",
        maps: &[MapId("MAP_ROUTE109_SEASHORE_HOUSE")],
    },
    MapGroup {
        label: "gMapGroup_IndoorRoute110",
        maps: &[
            MapId("MAP_ROUTE110_TRICK_HOUSE_ENTRANCE"),
            MapId("MAP_ROUTE110_TRICK_HOUSE_END"),
            MapId("MAP_ROUTE110_TRICK_HOUSE_CORRIDOR"),
            MapId("MAP_ROUTE110_TRICK_HOUSE_PUZZLE1"),
            MapId("MAP_ROUTE110_TRICK_HOUSE_PUZZLE2"),
            MapId("MAP_ROUTE110_TRICK_HOUSE_PUZZLE3"),
            MapId("MAP_ROUTE110_TRICK_HOUSE_PUZZLE4"),
            MapId("MAP_ROUTE110_TRICK_HOUSE_PUZZLE5"),
            MapId("MAP_ROUTE110_TRICK_HOUSE_PUZZLE6"),
            MapId("MAP_ROUTE110_TRICK_HOUSE_PUZZLE7"),
            MapId("MAP_ROUTE110_TRICK_HOUSE_PUZZLE8"),
            MapId("MAP_ROUTE110_SEASIDE_CYCLING_ROAD_SOUTH_ENTRANCE"),
            MapId("MAP_ROUTE110_SEASIDE_CYCLING_ROAD_NORTH_ENTRANCE"),
        ],
    },
    MapGroup {
        label: "gMapGroup_IndoorRoute113",
        maps: &[MapId("MAP_ROUTE113_GLASS_WORKSHOP")],
    },
    MapGroup {
        label: "gMapGroup_IndoorRoute123",
        maps: &[MapId("MAP_ROUTE123_BERRY_MASTERS_HOUSE")],
    },
    MapGroup {
        label: "gMapGroup_IndoorRoute119",
        maps: &[
            MapId("MAP_ROUTE119_WEATHER_INSTITUTE_1F"),
            MapId("MAP_ROUTE119_WEATHER_INSTITUTE_2F"),
            MapId("MAP_ROUTE119_HOUSE"),
        ],
    },
    MapGroup {
        label: "gMapGroup_IndoorRoute124",
        maps: &[MapId("MAP_ROUTE124_DIVING_TREASURE_HUNTERS_HOUSE")],
    },
];

static HEADERS: [MapHeader; MAP_COUNT] = [
    MapHeader {
        id: MapId("MAP_PETALBURG_CITY"),
        group: 0,
        num: 0,
        name: "PetalburgCity",
        layout: LayoutId("LAYOUT_PETALBURG_CITY"),
        music: MusicId(362), // MUS_PETALBURG
        region_map_section: RegionMapSectionId("MAPSEC_PETALBURG_CITY"),
        requires_flash: false,
        weather: Weather::Sunny, // WEATHER_SUNNY
        map_type: MapType::City, // MAP_TYPE_CITY
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::West,
                offset: -50,
                target: MapId("MAP_ROUTE104"),
            }, // left
            MapConnection {
                direction: Direction::East,
                offset: 10,
                target: MapId("MAP_ROUTE102"),
            }, // right
        ],
    },
    MapHeader {
        id: MapId("MAP_SLATEPORT_CITY"),
        group: 0,
        num: 1,
        name: "SlateportCity",
        layout: LayoutId("LAYOUT_SLATEPORT_CITY"),
        music: MusicId(433), // MUS_SLATEPORT
        region_map_section: RegionMapSectionId("MAPSEC_SLATEPORT_CITY"),
        requires_flash: false,
        weather: Weather::Sunny, // WEATHER_SUNNY
        map_type: MapType::City, // MAP_TYPE_CITY
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::North,
                offset: 0,
                target: MapId("MAP_ROUTE110"),
            }, // up
            MapConnection {
                direction: Direction::South,
                offset: 0,
                target: MapId("MAP_ROUTE109"),
            }, // down
            MapConnection {
                direction: Direction::East,
                offset: 0,
                target: MapId("MAP_ROUTE134"),
            }, // right
        ],
    },
    MapHeader {
        id: MapId("MAP_MAUVILLE_CITY"),
        group: 0,
        num: 2,
        name: "MauvilleCity",
        layout: LayoutId("LAYOUT_MAUVILLE_CITY"),
        music: MusicId(399), // MUS_RUSTBORO
        region_map_section: RegionMapSectionId("MAPSEC_MAUVILLE_CITY"),
        requires_flash: false,
        weather: Weather::Sunny, // WEATHER_SUNNY
        map_type: MapType::City, // MAP_TYPE_CITY
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::North,
                offset: 0,
                target: MapId("MAP_ROUTE111"),
            }, // up
            MapConnection {
                direction: Direction::South,
                offset: 0,
                target: MapId("MAP_ROUTE110"),
            }, // down
            MapConnection {
                direction: Direction::West,
                offset: 0,
                target: MapId("MAP_ROUTE117"),
            }, // left
            MapConnection {
                direction: Direction::East,
                offset: 0,
                target: MapId("MAP_ROUTE118"),
            }, // right
        ],
    },
    MapHeader {
        id: MapId("MAP_RUSTBORO_CITY"),
        group: 0,
        num: 3,
        name: "RustboroCity",
        layout: LayoutId("LAYOUT_RUSTBORO_CITY"),
        music: MusicId(399), // MUS_RUSTBORO
        region_map_section: RegionMapSectionId("MAPSEC_RUSTBORO_CITY"),
        requires_flash: false,
        weather: Weather::Sunny, // WEATHER_SUNNY
        map_type: MapType::City, // MAP_TYPE_CITY
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::North,
                offset: 0,
                target: MapId("MAP_ROUTE115"),
            }, // up
            MapConnection {
                direction: Direction::South,
                offset: 0,
                target: MapId("MAP_ROUTE104"),
            }, // down
            MapConnection {
                direction: Direction::East,
                offset: 0,
                target: MapId("MAP_ROUTE116"),
            }, // right
        ],
    },
    MapHeader {
        id: MapId("MAP_FORTREE_CITY"),
        group: 0,
        num: 4,
        name: "FortreeCity",
        layout: LayoutId("LAYOUT_FORTREE_CITY"),
        music: MusicId(382), // MUS_FORTREE
        region_map_section: RegionMapSectionId("MAPSEC_FORTREE_CITY"),
        requires_flash: false,
        weather: Weather::Sunny, // WEATHER_SUNNY
        map_type: MapType::City, // MAP_TYPE_CITY
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::West,
                offset: 0,
                target: MapId("MAP_ROUTE119"),
            }, // left
            MapConnection {
                direction: Direction::East,
                offset: 0,
                target: MapId("MAP_ROUTE120"),
            }, // right
        ],
    },
    MapHeader {
        id: MapId("MAP_LILYCOVE_CITY"),
        group: 0,
        num: 5,
        name: "LilycoveCity",
        layout: LayoutId("LAYOUT_LILYCOVE_CITY"),
        music: MusicId(408), // MUS_LILYCOVE
        region_map_section: RegionMapSectionId("MAPSEC_LILYCOVE_CITY"),
        requires_flash: false,
        weather: Weather::Sunny, // WEATHER_SUNNY
        map_type: MapType::City, // MAP_TYPE_CITY
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::West,
                offset: 10,
                target: MapId("MAP_ROUTE121"),
            }, // left
            MapConnection {
                direction: Direction::East,
                offset: -10,
                target: MapId("MAP_ROUTE124"),
            }, // right
        ],
    },
    MapHeader {
        id: MapId("MAP_MOSSDEEP_CITY"),
        group: 0,
        num: 6,
        name: "MossdeepCity",
        layout: LayoutId("LAYOUT_MOSSDEEP_CITY"),
        music: MusicId(399), // MUS_RUSTBORO
        region_map_section: RegionMapSectionId("MAPSEC_MOSSDEEP_CITY"),
        requires_flash: false,
        weather: Weather::Sunny, // WEATHER_SUNNY
        map_type: MapType::City, // MAP_TYPE_CITY
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::North,
                offset: 0,
                target: MapId("MAP_ROUTE125"),
            }, // up
            MapConnection {
                direction: Direction::South,
                offset: 0,
                target: MapId("MAP_ROUTE127"),
            }, // down
            MapConnection {
                direction: Direction::West,
                offset: -40,
                target: MapId("MAP_ROUTE124"),
            }, // left
        ],
    },
    MapHeader {
        id: MapId("MAP_SOOTOPOLIS_CITY"),
        group: 0,
        num: 7,
        name: "SootopolisCity",
        layout: LayoutId("LAYOUT_SOOTOPOLIS_CITY"),
        music: MusicId(445), // MUS_SOOTOPOLIS
        region_map_section: RegionMapSectionId("MAPSEC_SOOTOPOLIS_CITY"),
        requires_flash: false,
        weather: Weather::Sunny, // WEATHER_SUNNY
        map_type: MapType::City, // MAP_TYPE_CITY
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_EVER_GRANDE_CITY"),
        group: 0,
        num: 8,
        name: "EverGrandeCity",
        layout: LayoutId("LAYOUT_EVER_GRANDE_CITY"),
        music: MusicId(422), // MUS_EVER_GRANDE
        region_map_section: RegionMapSectionId("MAPSEC_EVER_GRANDE_CITY"),
        requires_flash: false,
        weather: Weather::Sunny, // WEATHER_SUNNY
        map_type: MapType::City, // MAP_TYPE_CITY
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::West,
                offset: 40,
                target: MapId("MAP_ROUTE128"),
            }, // left
        ],
    },
    MapHeader {
        id: MapId("MAP_LITTLEROOT_TOWN"),
        group: 0,
        num: 9,
        name: "LittlerootTown",
        layout: LayoutId("LAYOUT_LITTLEROOT_TOWN"),
        music: MusicId(405), // MUS_LITTLEROOT
        region_map_section: RegionMapSectionId("MAPSEC_LITTLEROOT_TOWN"),
        requires_flash: false,
        weather: Weather::Sunny, // WEATHER_SUNNY
        map_type: MapType::Town, // MAP_TYPE_TOWN
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::North,
                offset: 0,
                target: MapId("MAP_ROUTE101"),
            }, // up
        ],
    },
    MapHeader {
        id: MapId("MAP_OLDALE_TOWN"),
        group: 0,
        num: 10,
        name: "OldaleTown",
        layout: LayoutId("LAYOUT_OLDALE_TOWN"),
        music: MusicId(363), // MUS_OLDALE
        region_map_section: RegionMapSectionId("MAPSEC_OLDALE_TOWN"),
        requires_flash: false,
        weather: Weather::Sunny, // WEATHER_SUNNY
        map_type: MapType::Town, // MAP_TYPE_TOWN
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::North,
                offset: 0,
                target: MapId("MAP_ROUTE103"),
            }, // up
            MapConnection {
                direction: Direction::South,
                offset: 0,
                target: MapId("MAP_ROUTE101"),
            }, // down
            MapConnection {
                direction: Direction::West,
                offset: 0,
                target: MapId("MAP_ROUTE102"),
            }, // left
        ],
    },
    MapHeader {
        id: MapId("MAP_DEWFORD_TOWN"),
        group: 0,
        num: 11,
        name: "DewfordTown",
        layout: LayoutId("LAYOUT_DEWFORD_TOWN"),
        music: MusicId(427), // MUS_DEWFORD
        region_map_section: RegionMapSectionId("MAPSEC_DEWFORD_TOWN"),
        requires_flash: false,
        weather: Weather::Sunny, // WEATHER_SUNNY
        map_type: MapType::Town, // MAP_TYPE_TOWN
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::North,
                offset: -60,
                target: MapId("MAP_ROUTE106"),
            }, // up
            MapConnection {
                direction: Direction::East,
                offset: 0,
                target: MapId("MAP_ROUTE107"),
            }, // right
        ],
    },
    MapHeader {
        id: MapId("MAP_LAVARIDGE_TOWN"),
        group: 0,
        num: 12,
        name: "LavaridgeTown",
        layout: LayoutId("LAYOUT_LAVARIDGE_TOWN"),
        music: MusicId(363), // MUS_OLDALE
        region_map_section: RegionMapSectionId("MAPSEC_LAVARIDGE_TOWN"),
        requires_flash: false,
        weather: Weather::Sunny, // WEATHER_SUNNY
        map_type: MapType::Town, // MAP_TYPE_TOWN
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::East,
                offset: -40,
                target: MapId("MAP_ROUTE112"),
            }, // right
        ],
    },
    MapHeader {
        id: MapId("MAP_FALLARBOR_TOWN"),
        group: 0,
        num: 13,
        name: "FallarborTown",
        layout: LayoutId("LAYOUT_FALLARBOR_TOWN"),
        music: MusicId(437), // MUS_FALLARBOR
        region_map_section: RegionMapSectionId("MAPSEC_FALLARBOR_TOWN"),
        requires_flash: false,
        weather: Weather::Sunny, // WEATHER_SUNNY
        map_type: MapType::Town, // MAP_TYPE_TOWN
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::West,
                offset: 0,
                target: MapId("MAP_ROUTE114"),
            }, // left
            MapConnection {
                direction: Direction::East,
                offset: 0,
                target: MapId("MAP_ROUTE113"),
            }, // right
        ],
    },
    MapHeader {
        id: MapId("MAP_VERDANTURF_TOWN"),
        group: 0,
        num: 14,
        name: "VerdanturfTown",
        layout: LayoutId("LAYOUT_VERDANTURF_TOWN"),
        music: MusicId(398), // MUS_VERDANTURF
        region_map_section: RegionMapSectionId("MAPSEC_VERDANTURF_TOWN"),
        requires_flash: false,
        weather: Weather::Sunny, // WEATHER_SUNNY
        map_type: MapType::Town, // MAP_TYPE_TOWN
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::North,
                offset: -80,
                target: MapId("MAP_ROUTE116"),
            }, // up
            MapConnection {
                direction: Direction::East,
                offset: 0,
                target: MapId("MAP_ROUTE117"),
            }, // right
        ],
    },
    MapHeader {
        id: MapId("MAP_PACIFIDLOG_TOWN"),
        group: 0,
        num: 15,
        name: "PacifidlogTown",
        layout: LayoutId("LAYOUT_PACIFIDLOG_TOWN"),
        music: MusicId(408), // MUS_LILYCOVE
        region_map_section: RegionMapSectionId("MAPSEC_PACIFIDLOG_TOWN"),
        requires_flash: false,
        weather: Weather::Sunny, // WEATHER_SUNNY
        map_type: MapType::Town, // MAP_TYPE_TOWN
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::West,
                offset: 0,
                target: MapId("MAP_ROUTE132"),
            }, // left
            MapConnection {
                direction: Direction::East,
                offset: 0,
                target: MapId("MAP_ROUTE131"),
            }, // right
        ],
    },
    MapHeader {
        id: MapId("MAP_ROUTE101"),
        group: 0,
        num: 16,
        name: "Route101",
        layout: LayoutId("LAYOUT_ROUTE101"),
        music: MusicId(359), // MUS_ROUTE101
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_101"),
        requires_flash: false,
        weather: Weather::Sunny,  // WEATHER_SUNNY
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::North,
                offset: 0,
                target: MapId("MAP_OLDALE_TOWN"),
            }, // up
            MapConnection {
                direction: Direction::South,
                offset: 0,
                target: MapId("MAP_LITTLEROOT_TOWN"),
            }, // down
        ],
    },
    MapHeader {
        id: MapId("MAP_ROUTE102"),
        group: 0,
        num: 17,
        name: "Route102",
        layout: LayoutId("LAYOUT_ROUTE102"),
        music: MusicId(359), // MUS_ROUTE101
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_102"),
        requires_flash: false,
        weather: Weather::Sunny,  // WEATHER_SUNNY
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::West,
                offset: -10,
                target: MapId("MAP_PETALBURG_CITY"),
            }, // left
            MapConnection {
                direction: Direction::East,
                offset: 0,
                target: MapId("MAP_OLDALE_TOWN"),
            }, // right
        ],
    },
    MapHeader {
        id: MapId("MAP_ROUTE103"),
        group: 0,
        num: 18,
        name: "Route103",
        layout: LayoutId("LAYOUT_ROUTE103"),
        music: MusicId(359), // MUS_ROUTE101
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_103"),
        requires_flash: false,
        weather: Weather::Sunny,  // WEATHER_SUNNY
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::South,
                offset: 0,
                target: MapId("MAP_OLDALE_TOWN"),
            }, // down
            MapConnection {
                direction: Direction::East,
                offset: -60,
                target: MapId("MAP_ROUTE110"),
            }, // right
        ],
    },
    MapHeader {
        id: MapId("MAP_ROUTE104"),
        group: 0,
        num: 19,
        name: "Route104",
        layout: LayoutId("LAYOUT_ROUTE104"),
        music: MusicId(401), // MUS_ROUTE104
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_104"),
        requires_flash: false,
        weather: Weather::Sunny,  // WEATHER_SUNNY
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::North,
                offset: 0,
                target: MapId("MAP_RUSTBORO_CITY"),
            }, // up
            MapConnection {
                direction: Direction::South,
                offset: 0,
                target: MapId("MAP_ROUTE105"),
            }, // down
            MapConnection {
                direction: Direction::East,
                offset: 50,
                target: MapId("MAP_PETALBURG_CITY"),
            }, // right
        ],
    },
    MapHeader {
        id: MapId("MAP_ROUTE105"),
        group: 0,
        num: 20,
        name: "Route105",
        layout: LayoutId("LAYOUT_ROUTE105"),
        music: MusicId(401), // MUS_ROUTE104
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_105"),
        requires_flash: false,
        weather: Weather::Sunny,  // WEATHER_SUNNY
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::North,
                offset: 0,
                target: MapId("MAP_ROUTE104"),
            }, // up
            MapConnection {
                direction: Direction::South,
                offset: 0,
                target: MapId("MAP_ROUTE106"),
            }, // down
            MapConnection {
                direction: Direction::Dive,
                offset: 0,
                target: MapId("MAP_UNDERWATER_ROUTE105"),
            }, // dive
        ],
    },
    MapHeader {
        id: MapId("MAP_ROUTE106"),
        group: 0,
        num: 21,
        name: "Route106",
        layout: LayoutId("LAYOUT_ROUTE106"),
        music: MusicId(401), // MUS_ROUTE104
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_106"),
        requires_flash: false,
        weather: Weather::Sunny,  // WEATHER_SUNNY
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::North,
                offset: 0,
                target: MapId("MAP_ROUTE105"),
            }, // up
            MapConnection {
                direction: Direction::South,
                offset: 60,
                target: MapId("MAP_DEWFORD_TOWN"),
            }, // down
        ],
    },
    MapHeader {
        id: MapId("MAP_ROUTE107"),
        group: 0,
        num: 22,
        name: "Route107",
        layout: LayoutId("LAYOUT_ROUTE107"),
        music: MusicId(401), // MUS_ROUTE104
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_107"),
        requires_flash: false,
        weather: Weather::Sunny,  // WEATHER_SUNNY
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::West,
                offset: 0,
                target: MapId("MAP_DEWFORD_TOWN"),
            }, // left
            MapConnection {
                direction: Direction::East,
                offset: 0,
                target: MapId("MAP_ROUTE108"),
            }, // right
        ],
    },
    MapHeader {
        id: MapId("MAP_ROUTE108"),
        group: 0,
        num: 23,
        name: "Route108",
        layout: LayoutId("LAYOUT_ROUTE108"),
        music: MusicId(401), // MUS_ROUTE104
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_108"),
        requires_flash: false,
        weather: Weather::Sunny,  // WEATHER_SUNNY
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::West,
                offset: 0,
                target: MapId("MAP_ROUTE107"),
            }, // left
            MapConnection {
                direction: Direction::East,
                offset: -40,
                target: MapId("MAP_ROUTE109"),
            }, // right
        ],
    },
    MapHeader {
        id: MapId("MAP_ROUTE109"),
        group: 0,
        num: 24,
        name: "Route109",
        layout: LayoutId("LAYOUT_ROUTE109"),
        music: MusicId(401), // MUS_ROUTE104
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_109"),
        requires_flash: false,
        weather: Weather::Sunny,  // WEATHER_SUNNY
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::North,
                offset: 0,
                target: MapId("MAP_SLATEPORT_CITY"),
            }, // up
            MapConnection {
                direction: Direction::West,
                offset: 40,
                target: MapId("MAP_ROUTE108"),
            }, // left
        ],
    },
    MapHeader {
        id: MapId("MAP_ROUTE110"),
        group: 0,
        num: 25,
        name: "Route110",
        layout: LayoutId("LAYOUT_ROUTE110"),
        music: MusicId(360), // MUS_ROUTE110
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_110"),
        requires_flash: false,
        weather: Weather::Sunny,  // WEATHER_SUNNY
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::North,
                offset: 0,
                target: MapId("MAP_MAUVILLE_CITY"),
            }, // up
            MapConnection {
                direction: Direction::South,
                offset: 0,
                target: MapId("MAP_SLATEPORT_CITY"),
            }, // down
            MapConnection {
                direction: Direction::West,
                offset: 60,
                target: MapId("MAP_ROUTE103"),
            }, // left
        ],
    },
    MapHeader {
        id: MapId("MAP_ROUTE111"),
        group: 0,
        num: 26,
        name: "Route111",
        layout: LayoutId("LAYOUT_ROUTE111"),
        music: MusicId(360), // MUS_ROUTE110
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_111"),
        requires_flash: false,
        weather: Weather::Sunny,  // WEATHER_SUNNY
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::South,
                offset: 0,
                target: MapId("MAP_MAUVILLE_CITY"),
            }, // down
            MapConnection {
                direction: Direction::West,
                offset: 0,
                target: MapId("MAP_ROUTE113"),
            }, // left
            MapConnection {
                direction: Direction::West,
                offset: 20,
                target: MapId("MAP_ROUTE112"),
            }, // left
        ],
    },
    MapHeader {
        id: MapId("MAP_ROUTE112"),
        group: 0,
        num: 27,
        name: "Route112",
        layout: LayoutId("LAYOUT_ROUTE112"),
        music: MusicId(360), // MUS_ROUTE110
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_112"),
        requires_flash: false,
        weather: Weather::Sunny,  // WEATHER_SUNNY
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::North,
                offset: -60,
                target: MapId("MAP_ROUTE113"),
            }, // up
            MapConnection {
                direction: Direction::West,
                offset: 40,
                target: MapId("MAP_LAVARIDGE_TOWN"),
            }, // left
            MapConnection {
                direction: Direction::East,
                offset: -20,
                target: MapId("MAP_ROUTE111"),
            }, // right
        ],
    },
    MapHeader {
        id: MapId("MAP_ROUTE113"),
        group: 0,
        num: 28,
        name: "Route113",
        layout: LayoutId("LAYOUT_ROUTE113"),
        music: MusicId(418), // MUS_ROUTE113
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_113"),
        requires_flash: false,
        weather: Weather::Sunny,  // WEATHER_SUNNY
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::South,
                offset: 60,
                target: MapId("MAP_ROUTE112"),
            }, // down
            MapConnection {
                direction: Direction::West,
                offset: 0,
                target: MapId("MAP_FALLARBOR_TOWN"),
            }, // left
            MapConnection {
                direction: Direction::East,
                offset: 0,
                target: MapId("MAP_ROUTE111"),
            }, // right
        ],
    },
    MapHeader {
        id: MapId("MAP_ROUTE114"),
        group: 0,
        num: 29,
        name: "Route114",
        layout: LayoutId("LAYOUT_ROUTE114"),
        music: MusicId(360), // MUS_ROUTE110
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_114"),
        requires_flash: false,
        weather: Weather::Sunny,  // WEATHER_SUNNY
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::West,
                offset: 40,
                target: MapId("MAP_ROUTE115"),
            }, // left
            MapConnection {
                direction: Direction::East,
                offset: 0,
                target: MapId("MAP_FALLARBOR_TOWN"),
            }, // right
        ],
    },
    MapHeader {
        id: MapId("MAP_ROUTE115"),
        group: 0,
        num: 30,
        name: "Route115",
        layout: LayoutId("LAYOUT_ROUTE115"),
        music: MusicId(401), // MUS_ROUTE104
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_115"),
        requires_flash: false,
        weather: Weather::Sunny,  // WEATHER_SUNNY
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::South,
                offset: 0,
                target: MapId("MAP_RUSTBORO_CITY"),
            }, // down
            MapConnection {
                direction: Direction::East,
                offset: -40,
                target: MapId("MAP_ROUTE114"),
            }, // right
        ],
    },
    MapHeader {
        id: MapId("MAP_ROUTE116"),
        group: 0,
        num: 31,
        name: "Route116",
        layout: LayoutId("LAYOUT_ROUTE116"),
        music: MusicId(401), // MUS_ROUTE104
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_116"),
        requires_flash: false,
        weather: Weather::Sunny,  // WEATHER_SUNNY
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::South,
                offset: 80,
                target: MapId("MAP_VERDANTURF_TOWN"),
            }, // down
            MapConnection {
                direction: Direction::West,
                offset: 0,
                target: MapId("MAP_RUSTBORO_CITY"),
            }, // left
        ],
    },
    MapHeader {
        id: MapId("MAP_ROUTE117"),
        group: 0,
        num: 32,
        name: "Route117",
        layout: LayoutId("LAYOUT_ROUTE117"),
        music: MusicId(360), // MUS_ROUTE110
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_117"),
        requires_flash: false,
        weather: Weather::Sunny,  // WEATHER_SUNNY
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::West,
                offset: 0,
                target: MapId("MAP_VERDANTURF_TOWN"),
            }, // left
            MapConnection {
                direction: Direction::East,
                offset: 0,
                target: MapId("MAP_MAUVILLE_CITY"),
            }, // right
        ],
    },
    MapHeader {
        id: MapId("MAP_ROUTE118"),
        group: 0,
        num: 33,
        name: "Route118",
        layout: LayoutId("LAYOUT_ROUTE118"),
        music: MusicId(32767), // MUS_ROUTE118
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_118"),
        requires_flash: false,
        weather: Weather::Sunny,  // WEATHER_SUNNY
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::North,
                offset: 40,
                target: MapId("MAP_ROUTE119"),
            }, // up
            MapConnection {
                direction: Direction::West,
                offset: 0,
                target: MapId("MAP_MAUVILLE_CITY"),
            }, // left
            MapConnection {
                direction: Direction::East,
                offset: 0,
                target: MapId("MAP_ROUTE123"),
            }, // right
        ],
    },
    MapHeader {
        id: MapId("MAP_ROUTE119"),
        group: 0,
        num: 34,
        name: "Route119",
        layout: LayoutId("LAYOUT_ROUTE119"),
        music: MusicId(402), // MUS_ROUTE119
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_119"),
        requires_flash: false,
        weather: Weather::Sunny,  // WEATHER_SUNNY
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::South,
                offset: -40,
                target: MapId("MAP_ROUTE118"),
            }, // down
            MapConnection {
                direction: Direction::East,
                offset: 0,
                target: MapId("MAP_FORTREE_CITY"),
            }, // right
        ],
    },
    MapHeader {
        id: MapId("MAP_ROUTE120"),
        group: 0,
        num: 35,
        name: "Route120",
        layout: LayoutId("LAYOUT_ROUTE120"),
        music: MusicId(361), // MUS_ROUTE120
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_120"),
        requires_flash: false,
        weather: Weather::Sunny,  // WEATHER_SUNNY
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::West,
                offset: 0,
                target: MapId("MAP_FORTREE_CITY"),
            }, // left
            MapConnection {
                direction: Direction::East,
                offset: 80,
                target: MapId("MAP_ROUTE121"),
            }, // right
        ],
    },
    MapHeader {
        id: MapId("MAP_ROUTE121"),
        group: 0,
        num: 36,
        name: "Route121",
        layout: LayoutId("LAYOUT_ROUTE121"),
        music: MusicId(361), // MUS_ROUTE120
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_121"),
        requires_flash: false,
        weather: Weather::Sunny,  // WEATHER_SUNNY
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::South,
                offset: 20,
                target: MapId("MAP_ROUTE122"),
            }, // down
            MapConnection {
                direction: Direction::West,
                offset: -80,
                target: MapId("MAP_ROUTE120"),
            }, // left
            MapConnection {
                direction: Direction::East,
                offset: -10,
                target: MapId("MAP_LILYCOVE_CITY"),
            }, // right
        ],
    },
    MapHeader {
        id: MapId("MAP_ROUTE122"),
        group: 0,
        num: 37,
        name: "Route122",
        layout: LayoutId("LAYOUT_ROUTE122"),
        music: MusicId(374), // MUS_ROUTE122
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_122"),
        requires_flash: false,
        weather: Weather::Sunny,  // WEATHER_SUNNY
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::North,
                offset: -20,
                target: MapId("MAP_ROUTE121"),
            }, // up
            MapConnection {
                direction: Direction::South,
                offset: -100,
                target: MapId("MAP_ROUTE123"),
            }, // down
        ],
    },
    MapHeader {
        id: MapId("MAP_ROUTE123"),
        group: 0,
        num: 38,
        name: "Route123",
        layout: LayoutId("LAYOUT_ROUTE123"),
        music: MusicId(374), // MUS_ROUTE122
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_123"),
        requires_flash: false,
        weather: Weather::Sunny,  // WEATHER_SUNNY
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::North,
                offset: 100,
                target: MapId("MAP_ROUTE122"),
            }, // up
            MapConnection {
                direction: Direction::West,
                offset: 0,
                target: MapId("MAP_ROUTE118"),
            }, // left
        ],
    },
    MapHeader {
        id: MapId("MAP_ROUTE124"),
        group: 0,
        num: 39,
        name: "Route124",
        layout: LayoutId("LAYOUT_ROUTE124"),
        music: MusicId(361), // MUS_ROUTE120
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_124"),
        requires_flash: false,
        weather: Weather::Sunny,       // WEATHER_SUNNY
        map_type: MapType::OceanRoute, // MAP_TYPE_OCEAN_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::South,
                offset: 0,
                target: MapId("MAP_ROUTE126"),
            }, // down
            MapConnection {
                direction: Direction::West,
                offset: 10,
                target: MapId("MAP_LILYCOVE_CITY"),
            }, // left
            MapConnection {
                direction: Direction::East,
                offset: 0,
                target: MapId("MAP_ROUTE125"),
            }, // right
            MapConnection {
                direction: Direction::East,
                offset: 40,
                target: MapId("MAP_MOSSDEEP_CITY"),
            }, // right
            MapConnection {
                direction: Direction::Dive,
                offset: 0,
                target: MapId("MAP_UNDERWATER_ROUTE124"),
            }, // dive
        ],
    },
    MapHeader {
        id: MapId("MAP_ROUTE125"),
        group: 0,
        num: 40,
        name: "Route125",
        layout: LayoutId("LAYOUT_ROUTE125"),
        music: MusicId(361), // MUS_ROUTE120
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_125"),
        requires_flash: false,
        weather: Weather::Sunny,       // WEATHER_SUNNY
        map_type: MapType::OceanRoute, // MAP_TYPE_OCEAN_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::South,
                offset: 0,
                target: MapId("MAP_MOSSDEEP_CITY"),
            }, // down
            MapConnection {
                direction: Direction::West,
                offset: 0,
                target: MapId("MAP_ROUTE124"),
            }, // left
            MapConnection {
                direction: Direction::Dive,
                offset: 0,
                target: MapId("MAP_UNDERWATER_ROUTE125"),
            }, // dive
        ],
    },
    MapHeader {
        id: MapId("MAP_ROUTE126"),
        group: 0,
        num: 41,
        name: "Route126",
        layout: LayoutId("LAYOUT_ROUTE126"),
        music: MusicId(361), // MUS_ROUTE120
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_126"),
        requires_flash: false,
        weather: Weather::Sunny,       // WEATHER_SUNNY
        map_type: MapType::OceanRoute, // MAP_TYPE_OCEAN_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::North,
                offset: 0,
                target: MapId("MAP_ROUTE124"),
            }, // up
            MapConnection {
                direction: Direction::East,
                offset: 0,
                target: MapId("MAP_ROUTE127"),
            }, // right
            MapConnection {
                direction: Direction::Dive,
                offset: 0,
                target: MapId("MAP_UNDERWATER_ROUTE126"),
            }, // dive
        ],
    },
    MapHeader {
        id: MapId("MAP_ROUTE127"),
        group: 0,
        num: 42,
        name: "Route127",
        layout: LayoutId("LAYOUT_ROUTE127"),
        music: MusicId(361), // MUS_ROUTE120
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_127"),
        requires_flash: false,
        weather: Weather::Sunny,       // WEATHER_SUNNY
        map_type: MapType::OceanRoute, // MAP_TYPE_OCEAN_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::North,
                offset: 0,
                target: MapId("MAP_MOSSDEEP_CITY"),
            }, // up
            MapConnection {
                direction: Direction::South,
                offset: 0,
                target: MapId("MAP_ROUTE128"),
            }, // down
            MapConnection {
                direction: Direction::West,
                offset: 0,
                target: MapId("MAP_ROUTE126"),
            }, // left
            MapConnection {
                direction: Direction::Dive,
                offset: 0,
                target: MapId("MAP_UNDERWATER_ROUTE127"),
            }, // dive
        ],
    },
    MapHeader {
        id: MapId("MAP_ROUTE128"),
        group: 0,
        num: 43,
        name: "Route128",
        layout: LayoutId("LAYOUT_ROUTE128"),
        music: MusicId(361), // MUS_ROUTE120
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_128"),
        requires_flash: false,
        weather: Weather::Sunny,       // WEATHER_SUNNY
        map_type: MapType::OceanRoute, // MAP_TYPE_OCEAN_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::North,
                offset: 0,
                target: MapId("MAP_ROUTE127"),
            }, // up
            MapConnection {
                direction: Direction::South,
                offset: 0,
                target: MapId("MAP_ROUTE129"),
            }, // down
            MapConnection {
                direction: Direction::East,
                offset: -40,
                target: MapId("MAP_EVER_GRANDE_CITY"),
            }, // right
            MapConnection {
                direction: Direction::Dive,
                offset: 0,
                target: MapId("MAP_UNDERWATER_ROUTE128"),
            }, // dive
        ],
    },
    MapHeader {
        id: MapId("MAP_ROUTE129"),
        group: 0,
        num: 44,
        name: "Route129",
        layout: LayoutId("LAYOUT_ROUTE129"),
        music: MusicId(402), // MUS_ROUTE119
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_129"),
        requires_flash: false,
        weather: Weather::Sunny,       // WEATHER_SUNNY
        map_type: MapType::OceanRoute, // MAP_TYPE_OCEAN_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::North,
                offset: 0,
                target: MapId("MAP_ROUTE128"),
            }, // up
            MapConnection {
                direction: Direction::West,
                offset: 0,
                target: MapId("MAP_ROUTE130"),
            }, // left
            MapConnection {
                direction: Direction::Dive,
                offset: 0,
                target: MapId("MAP_UNDERWATER_ROUTE129"),
            }, // dive
        ],
    },
    MapHeader {
        id: MapId("MAP_ROUTE130"),
        group: 0,
        num: 45,
        name: "Route130",
        layout: LayoutId("LAYOUT_ROUTE130"),
        music: MusicId(402), // MUS_ROUTE119
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_130"),
        requires_flash: false,
        weather: Weather::Sunny,       // WEATHER_SUNNY
        map_type: MapType::OceanRoute, // MAP_TYPE_OCEAN_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::West,
                offset: 0,
                target: MapId("MAP_ROUTE131"),
            }, // left
            MapConnection {
                direction: Direction::East,
                offset: 0,
                target: MapId("MAP_ROUTE129"),
            }, // right
        ],
    },
    MapHeader {
        id: MapId("MAP_ROUTE131"),
        group: 0,
        num: 46,
        name: "Route131",
        layout: LayoutId("LAYOUT_ROUTE131"),
        music: MusicId(402), // MUS_ROUTE119
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_131"),
        requires_flash: false,
        weather: Weather::Sunny,       // WEATHER_SUNNY
        map_type: MapType::OceanRoute, // MAP_TYPE_OCEAN_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::West,
                offset: 0,
                target: MapId("MAP_PACIFIDLOG_TOWN"),
            }, // left
            MapConnection {
                direction: Direction::East,
                offset: 0,
                target: MapId("MAP_ROUTE130"),
            }, // right
        ],
    },
    MapHeader {
        id: MapId("MAP_ROUTE132"),
        group: 0,
        num: 47,
        name: "Route132",
        layout: LayoutId("LAYOUT_ROUTE132"),
        music: MusicId(402), // MUS_ROUTE119
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_132"),
        requires_flash: false,
        weather: Weather::Sunny,       // WEATHER_SUNNY
        map_type: MapType::OceanRoute, // MAP_TYPE_OCEAN_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::West,
                offset: 0,
                target: MapId("MAP_ROUTE133"),
            }, // left
            MapConnection {
                direction: Direction::East,
                offset: 0,
                target: MapId("MAP_PACIFIDLOG_TOWN"),
            }, // right
        ],
    },
    MapHeader {
        id: MapId("MAP_ROUTE133"),
        group: 0,
        num: 48,
        name: "Route133",
        layout: LayoutId("LAYOUT_ROUTE133"),
        music: MusicId(402), // MUS_ROUTE119
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_133"),
        requires_flash: false,
        weather: Weather::Sunny,       // WEATHER_SUNNY
        map_type: MapType::OceanRoute, // MAP_TYPE_OCEAN_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::West,
                offset: 0,
                target: MapId("MAP_ROUTE134"),
            }, // left
            MapConnection {
                direction: Direction::East,
                offset: 0,
                target: MapId("MAP_ROUTE132"),
            }, // right
        ],
    },
    MapHeader {
        id: MapId("MAP_ROUTE134"),
        group: 0,
        num: 49,
        name: "Route134",
        layout: LayoutId("LAYOUT_ROUTE134"),
        music: MusicId(402), // MUS_ROUTE119
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_134"),
        requires_flash: false,
        weather: Weather::Sunny,       // WEATHER_SUNNY
        map_type: MapType::OceanRoute, // MAP_TYPE_OCEAN_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::West,
                offset: 0,
                target: MapId("MAP_SLATEPORT_CITY"),
            }, // left
            MapConnection {
                direction: Direction::East,
                offset: 0,
                target: MapId("MAP_ROUTE133"),
            }, // right
        ],
    },
    MapHeader {
        id: MapId("MAP_UNDERWATER_ROUTE124"),
        group: 0,
        num: 50,
        name: "Underwater_Route124",
        layout: LayoutId("LAYOUT_UNDERWATER_ROUTE124"),
        music: MusicId(411), // MUS_UNDERWATER
        region_map_section: RegionMapSectionId("MAPSEC_UNDERWATER_124"),
        requires_flash: false,
        weather: Weather::UnderwaterBubbles, // WEATHER_UNDERWATER_BUBBLES
        map_type: MapType::Underwater,       // MAP_TYPE_UNDERWATER
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::South,
                offset: 0,
                target: MapId("MAP_UNDERWATER_ROUTE126"),
            }, // down
            MapConnection {
                direction: Direction::Emerge,
                offset: 0,
                target: MapId("MAP_ROUTE124"),
            }, // emerge
        ],
    },
    MapHeader {
        id: MapId("MAP_UNDERWATER_ROUTE126"),
        group: 0,
        num: 51,
        name: "Underwater_Route126",
        layout: LayoutId("LAYOUT_UNDERWATER_ROUTE126"),
        music: MusicId(411), // MUS_UNDERWATER
        region_map_section: RegionMapSectionId("MAPSEC_UNDERWATER_126"),
        requires_flash: false,
        weather: Weather::UnderwaterBubbles, // WEATHER_UNDERWATER_BUBBLES
        map_type: MapType::Underwater,       // MAP_TYPE_UNDERWATER
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::North,
                offset: 0,
                target: MapId("MAP_UNDERWATER_ROUTE124"),
            }, // up
            MapConnection {
                direction: Direction::East,
                offset: 0,
                target: MapId("MAP_UNDERWATER_ROUTE127"),
            }, // right
            MapConnection {
                direction: Direction::Emerge,
                offset: 0,
                target: MapId("MAP_ROUTE126"),
            }, // emerge
        ],
    },
    MapHeader {
        id: MapId("MAP_UNDERWATER_ROUTE127"),
        group: 0,
        num: 52,
        name: "Underwater_Route127",
        layout: LayoutId("LAYOUT_UNDERWATER_ROUTE127"),
        music: MusicId(411), // MUS_UNDERWATER
        region_map_section: RegionMapSectionId("MAPSEC_UNDERWATER_127"),
        requires_flash: false,
        weather: Weather::UnderwaterBubbles, // WEATHER_UNDERWATER_BUBBLES
        map_type: MapType::Underwater,       // MAP_TYPE_UNDERWATER
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::Emerge,
                offset: 0,
                target: MapId("MAP_ROUTE127"),
            }, // emerge
            MapConnection {
                direction: Direction::West,
                offset: 0,
                target: MapId("MAP_UNDERWATER_ROUTE126"),
            }, // left
            MapConnection {
                direction: Direction::South,
                offset: 0,
                target: MapId("MAP_UNDERWATER_ROUTE128"),
            }, // down
        ],
    },
    MapHeader {
        id: MapId("MAP_UNDERWATER_ROUTE128"),
        group: 0,
        num: 53,
        name: "Underwater_Route128",
        layout: LayoutId("LAYOUT_UNDERWATER_ROUTE128"),
        music: MusicId(411), // MUS_UNDERWATER
        region_map_section: RegionMapSectionId("MAPSEC_UNDERWATER_128"),
        requires_flash: false,
        weather: Weather::UnderwaterBubbles, // WEATHER_UNDERWATER_BUBBLES
        map_type: MapType::Underwater,       // MAP_TYPE_UNDERWATER
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::North,
                offset: 0,
                target: MapId("MAP_UNDERWATER_ROUTE127"),
            }, // up
            MapConnection {
                direction: Direction::Emerge,
                offset: 0,
                target: MapId("MAP_ROUTE128"),
            }, // emerge
        ],
    },
    MapHeader {
        id: MapId("MAP_UNDERWATER_ROUTE129"),
        group: 0,
        num: 54,
        name: "Underwater_Route129",
        layout: LayoutId("LAYOUT_UNDERWATER_ROUTE129"),
        music: MusicId(411), // MUS_UNDERWATER
        region_map_section: RegionMapSectionId("MAPSEC_UNDERWATER_129"),
        requires_flash: false,
        weather: Weather::UnderwaterBubbles, // WEATHER_UNDERWATER_BUBBLES
        map_type: MapType::Underwater,       // MAP_TYPE_UNDERWATER
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::Emerge,
                offset: 0,
                target: MapId("MAP_ROUTE129"),
            }, // emerge
        ],
    },
    MapHeader {
        id: MapId("MAP_UNDERWATER_ROUTE105"),
        group: 0,
        num: 55,
        name: "Underwater_Route105",
        layout: LayoutId("LAYOUT_UNDERWATER_ROUTE105"),
        music: MusicId(411), // MUS_UNDERWATER
        region_map_section: RegionMapSectionId("MAPSEC_UNDERWATER_105"),
        requires_flash: false,
        weather: Weather::UnderwaterBubbles, // WEATHER_UNDERWATER_BUBBLES
        map_type: MapType::Underwater,       // MAP_TYPE_UNDERWATER
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::Emerge,
                offset: 0,
                target: MapId("MAP_ROUTE105"),
            }, // emerge
        ],
    },
    MapHeader {
        id: MapId("MAP_UNDERWATER_ROUTE125"),
        group: 0,
        num: 56,
        name: "Underwater_Route125",
        layout: LayoutId("LAYOUT_UNDERWATER_ROUTE125"),
        music: MusicId(411), // MUS_UNDERWATER
        region_map_section: RegionMapSectionId("MAPSEC_UNDERWATER_125"),
        requires_flash: false,
        weather: Weather::UnderwaterBubbles, // WEATHER_UNDERWATER_BUBBLES
        map_type: MapType::Underwater,       // MAP_TYPE_UNDERWATER
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::Emerge,
                offset: 0,
                target: MapId("MAP_ROUTE125"),
            }, // emerge
        ],
    },
    MapHeader {
        id: MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F"),
        group: 1,
        num: 0,
        name: "LittlerootTown_BrendansHouse_1F",
        layout: LayoutId("LAYOUT_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F"),
        music: MusicId(405), // MUS_LITTLEROOT
        region_map_section: RegionMapSectionId("MAPSEC_LITTLEROOT_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F"),
        group: 1,
        num: 1,
        name: "LittlerootTown_BrendansHouse_2F",
        layout: LayoutId("LAYOUT_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F"),
        music: MusicId(405), // MUS_LITTLEROOT
        region_map_section: RegionMapSectionId("MAPSEC_LITTLEROOT_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_LITTLEROOT_TOWN_MAYS_HOUSE_1F"),
        group: 1,
        num: 2,
        name: "LittlerootTown_MaysHouse_1F",
        layout: LayoutId("LAYOUT_LITTLEROOT_TOWN_MAYS_HOUSE_1F"),
        music: MusicId(405), // MUS_LITTLEROOT
        region_map_section: RegionMapSectionId("MAPSEC_LITTLEROOT_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_LITTLEROOT_TOWN_MAYS_HOUSE_2F"),
        group: 1,
        num: 3,
        name: "LittlerootTown_MaysHouse_2F",
        layout: LayoutId("LAYOUT_LITTLEROOT_TOWN_MAYS_HOUSE_2F"),
        music: MusicId(405), // MUS_LITTLEROOT
        region_map_section: RegionMapSectionId("MAPSEC_LITTLEROOT_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB"),
        group: 1,
        num: 4,
        name: "LittlerootTown_ProfessorBirchsLab",
        layout: LayoutId("LAYOUT_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB"),
        music: MusicId(383), // MUS_BIRCH_LAB
        region_map_section: RegionMapSectionId("MAPSEC_LITTLEROOT_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_OLDALE_TOWN_HOUSE1"),
        group: 2,
        num: 0,
        name: "OldaleTown_House1",
        layout: LayoutId("LAYOUT_HOUSE1"),
        music: MusicId(363), // MUS_OLDALE
        region_map_section: RegionMapSectionId("MAPSEC_OLDALE_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_OLDALE_TOWN_HOUSE2"),
        group: 2,
        num: 1,
        name: "OldaleTown_House2",
        layout: LayoutId("LAYOUT_HOUSE2"),
        music: MusicId(363), // MUS_OLDALE
        region_map_section: RegionMapSectionId("MAPSEC_OLDALE_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_OLDALE_TOWN_POKEMON_CENTER_1F"),
        group: 2,
        num: 2,
        name: "OldaleTown_PokemonCenter_1F",
        layout: LayoutId("LAYOUT_POKEMON_CENTER_1F"),
        music: MusicId(400), // MUS_POKE_CENTER
        region_map_section: RegionMapSectionId("MAPSEC_OLDALE_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_OLDALE_TOWN_POKEMON_CENTER_2F"),
        group: 2,
        num: 3,
        name: "OldaleTown_PokemonCenter_2F",
        layout: LayoutId("LAYOUT_POKEMON_CENTER_2F"),
        music: MusicId(400), // MUS_POKE_CENTER
        region_map_section: RegionMapSectionId("MAPSEC_OLDALE_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_OLDALE_TOWN_MART"),
        group: 2,
        num: 4,
        name: "OldaleTown_Mart",
        layout: LayoutId("LAYOUT_MART"),
        music: MusicId(404), // MUS_POKE_MART
        region_map_section: RegionMapSectionId("MAPSEC_OLDALE_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_DEWFORD_TOWN_HOUSE1"),
        group: 3,
        num: 0,
        name: "DewfordTown_House1",
        layout: LayoutId("LAYOUT_HOUSE3"),
        music: MusicId(427), // MUS_DEWFORD
        region_map_section: RegionMapSectionId("MAPSEC_DEWFORD_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_DEWFORD_TOWN_POKEMON_CENTER_1F"),
        group: 3,
        num: 1,
        name: "DewfordTown_PokemonCenter_1F",
        layout: LayoutId("LAYOUT_POKEMON_CENTER_1F"),
        music: MusicId(400), // MUS_POKE_CENTER
        region_map_section: RegionMapSectionId("MAPSEC_DEWFORD_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_DEWFORD_TOWN_POKEMON_CENTER_2F"),
        group: 3,
        num: 2,
        name: "DewfordTown_PokemonCenter_2F",
        layout: LayoutId("LAYOUT_POKEMON_CENTER_2F"),
        music: MusicId(400), // MUS_POKE_CENTER
        region_map_section: RegionMapSectionId("MAPSEC_DEWFORD_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_DEWFORD_TOWN_GYM"),
        group: 3,
        num: 3,
        name: "DewfordTown_Gym",
        layout: LayoutId("LAYOUT_DEWFORD_TOWN_GYM"),
        music: MusicId(364), // MUS_GYM
        region_map_section: RegionMapSectionId("MAPSEC_DEWFORD_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Gym, // MAP_BATTLE_SCENE_GYM
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_DEWFORD_TOWN_HALL"),
        group: 3,
        num: 4,
        name: "DewfordTown_Hall",
        layout: LayoutId("LAYOUT_DEWFORD_TOWN_HALL"),
        music: MusicId(427), // MUS_DEWFORD
        region_map_section: RegionMapSectionId("MAPSEC_DEWFORD_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_DEWFORD_TOWN_HOUSE2"),
        group: 3,
        num: 5,
        name: "DewfordTown_House2",
        layout: LayoutId("LAYOUT_HOUSE4"),
        music: MusicId(427), // MUS_DEWFORD
        region_map_section: RegionMapSectionId("MAPSEC_DEWFORD_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_LAVARIDGE_TOWN_HERB_SHOP"),
        group: 4,
        num: 0,
        name: "LavaridgeTown_HerbShop",
        layout: LayoutId("LAYOUT_LAVARIDGE_TOWN_HERB_SHOP"),
        music: MusicId(363), // MUS_OLDALE
        region_map_section: RegionMapSectionId("MAPSEC_LAVARIDGE_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_LAVARIDGE_TOWN_GYM_1F"),
        group: 4,
        num: 1,
        name: "LavaridgeTown_Gym_1F",
        layout: LayoutId("LAYOUT_LAVARIDGE_TOWN_GYM_1F"),
        music: MusicId(364), // MUS_GYM
        region_map_section: RegionMapSectionId("MAPSEC_LAVARIDGE_TOWN"),
        requires_flash: false,
        weather: Weather::FogHorizontal, // WEATHER_FOG_HORIZONTAL
        map_type: MapType::Indoor,       // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Gym, // MAP_BATTLE_SCENE_GYM
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_LAVARIDGE_TOWN_GYM_B1F"),
        group: 4,
        num: 2,
        name: "LavaridgeTown_Gym_B1F",
        layout: LayoutId("LAYOUT_LAVARIDGE_TOWN_GYM_B1F"),
        music: MusicId(364), // MUS_GYM
        region_map_section: RegionMapSectionId("MAPSEC_LAVARIDGE_TOWN"),
        requires_flash: false,
        weather: Weather::FogHorizontal, // WEATHER_FOG_HORIZONTAL
        map_type: MapType::Indoor,       // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Gym, // MAP_BATTLE_SCENE_GYM
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_LAVARIDGE_TOWN_HOUSE"),
        group: 4,
        num: 3,
        name: "LavaridgeTown_House",
        layout: LayoutId("LAYOUT_HOUSE3"),
        music: MusicId(363), // MUS_OLDALE
        region_map_section: RegionMapSectionId("MAPSEC_LAVARIDGE_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_LAVARIDGE_TOWN_MART"),
        group: 4,
        num: 4,
        name: "LavaridgeTown_Mart",
        layout: LayoutId("LAYOUT_MART"),
        music: MusicId(404), // MUS_POKE_MART
        region_map_section: RegionMapSectionId("MAPSEC_LAVARIDGE_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_LAVARIDGE_TOWN_POKEMON_CENTER_1F"),
        group: 4,
        num: 5,
        name: "LavaridgeTown_PokemonCenter_1F",
        layout: LayoutId("LAYOUT_LAVARIDGE_TOWN_POKEMON_CENTER_1F"),
        music: MusicId(400), // MUS_POKE_CENTER
        region_map_section: RegionMapSectionId("MAPSEC_LAVARIDGE_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_LAVARIDGE_TOWN_POKEMON_CENTER_2F"),
        group: 4,
        num: 6,
        name: "LavaridgeTown_PokemonCenter_2F",
        layout: LayoutId("LAYOUT_POKEMON_CENTER_2F"),
        music: MusicId(400), // MUS_POKE_CENTER
        region_map_section: RegionMapSectionId("MAPSEC_LAVARIDGE_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_FALLARBOR_TOWN_MART"),
        group: 5,
        num: 0,
        name: "FallarborTown_Mart",
        layout: LayoutId("LAYOUT_MART"),
        music: MusicId(404), // MUS_POKE_MART
        region_map_section: RegionMapSectionId("MAPSEC_FALLARBOR_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_FALLARBOR_TOWN_BATTLE_TENT_LOBBY"),
        group: 5,
        num: 1,
        name: "FallarborTown_BattleTentLobby",
        layout: LayoutId("LAYOUT_BATTLE_TENT_LOBBY"),
        music: MusicId(384), // MUS_B_TOWER_RS
        region_map_section: RegionMapSectionId("MAPSEC_FALLARBOR_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_FALLARBOR_TOWN_BATTLE_TENT_CORRIDOR"),
        group: 5,
        num: 2,
        name: "FallarborTown_BattleTentCorridor",
        layout: LayoutId("LAYOUT_BATTLE_TENT_CORRIDOR"),
        music: MusicId(384), // MUS_B_TOWER_RS
        region_map_section: RegionMapSectionId("MAPSEC_FALLARBOR_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_FALLARBOR_TOWN_BATTLE_TENT_BATTLE_ROOM"),
        group: 5,
        num: 3,
        name: "FallarborTown_BattleTentBattleRoom",
        layout: LayoutId("LAYOUT_BATTLE_TENT_BATTLE_ROOM"),
        music: MusicId(384), // MUS_B_TOWER_RS
        region_map_section: RegionMapSectionId("MAPSEC_FALLARBOR_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_FALLARBOR_TOWN_POKEMON_CENTER_1F"),
        group: 5,
        num: 4,
        name: "FallarborTown_PokemonCenter_1F",
        layout: LayoutId("LAYOUT_POKEMON_CENTER_1F"),
        music: MusicId(400), // MUS_POKE_CENTER
        region_map_section: RegionMapSectionId("MAPSEC_FALLARBOR_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_FALLARBOR_TOWN_POKEMON_CENTER_2F"),
        group: 5,
        num: 5,
        name: "FallarborTown_PokemonCenter_2F",
        layout: LayoutId("LAYOUT_POKEMON_CENTER_2F"),
        music: MusicId(400), // MUS_POKE_CENTER
        region_map_section: RegionMapSectionId("MAPSEC_FALLARBOR_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_FALLARBOR_TOWN_COZMOS_HOUSE"),
        group: 5,
        num: 6,
        name: "FallarborTown_CozmosHouse",
        layout: LayoutId("LAYOUT_HOUSE1"),
        music: MusicId(437), // MUS_FALLARBOR
        region_map_section: RegionMapSectionId("MAPSEC_FALLARBOR_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_FALLARBOR_TOWN_MOVE_RELEARNERS_HOUSE"),
        group: 5,
        num: 7,
        name: "FallarborTown_MoveRelearnersHouse",
        layout: LayoutId("LAYOUT_HOUSE2"),
        music: MusicId(437), // MUS_FALLARBOR
        region_map_section: RegionMapSectionId("MAPSEC_FALLARBOR_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_VERDANTURF_TOWN_BATTLE_TENT_LOBBY"),
        group: 6,
        num: 0,
        name: "VerdanturfTown_BattleTentLobby",
        layout: LayoutId("LAYOUT_BATTLE_TENT_LOBBY"),
        music: MusicId(384), // MUS_B_TOWER_RS
        region_map_section: RegionMapSectionId("MAPSEC_VERDANTURF_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_VERDANTURF_TOWN_BATTLE_TENT_CORRIDOR"),
        group: 6,
        num: 1,
        name: "VerdanturfTown_BattleTentCorridor",
        layout: LayoutId("LAYOUT_BATTLE_TENT_CORRIDOR"),
        music: MusicId(384), // MUS_B_TOWER_RS
        region_map_section: RegionMapSectionId("MAPSEC_VERDANTURF_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_VERDANTURF_TOWN_BATTLE_TENT_BATTLE_ROOM"),
        group: 6,
        num: 2,
        name: "VerdanturfTown_BattleTentBattleRoom",
        layout: LayoutId("LAYOUT_VERDANTURF_TOWN_BATTLE_TENT_BATTLE_ROOM"),
        music: MusicId(384), // MUS_B_TOWER_RS
        region_map_section: RegionMapSectionId("MAPSEC_VERDANTURF_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_VERDANTURF_TOWN_MART"),
        group: 6,
        num: 3,
        name: "VerdanturfTown_Mart",
        layout: LayoutId("LAYOUT_MART"),
        music: MusicId(404), // MUS_POKE_MART
        region_map_section: RegionMapSectionId("MAPSEC_VERDANTURF_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_VERDANTURF_TOWN_POKEMON_CENTER_1F"),
        group: 6,
        num: 4,
        name: "VerdanturfTown_PokemonCenter_1F",
        layout: LayoutId("LAYOUT_POKEMON_CENTER_1F"),
        music: MusicId(400), // MUS_POKE_CENTER
        region_map_section: RegionMapSectionId("MAPSEC_VERDANTURF_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_VERDANTURF_TOWN_POKEMON_CENTER_2F"),
        group: 6,
        num: 5,
        name: "VerdanturfTown_PokemonCenter_2F",
        layout: LayoutId("LAYOUT_POKEMON_CENTER_2F"),
        music: MusicId(400), // MUS_POKE_CENTER
        region_map_section: RegionMapSectionId("MAPSEC_VERDANTURF_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_VERDANTURF_TOWN_WANDAS_HOUSE"),
        group: 6,
        num: 6,
        name: "VerdanturfTown_WandasHouse",
        layout: LayoutId("LAYOUT_VERDANTURF_TOWN_WANDAS_HOUSE"),
        music: MusicId(398), // MUS_VERDANTURF
        region_map_section: RegionMapSectionId("MAPSEC_VERDANTURF_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_VERDANTURF_TOWN_FRIENDSHIP_RATERS_HOUSE"),
        group: 6,
        num: 7,
        name: "VerdanturfTown_FriendshipRatersHouse",
        layout: LayoutId("LAYOUT_HOUSE2"),
        music: MusicId(398), // MUS_VERDANTURF
        region_map_section: RegionMapSectionId("MAPSEC_VERDANTURF_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_VERDANTURF_TOWN_HOUSE"),
        group: 6,
        num: 8,
        name: "VerdanturfTown_House",
        layout: LayoutId("LAYOUT_HOUSE1"),
        music: MusicId(398), // MUS_VERDANTURF
        region_map_section: RegionMapSectionId("MAPSEC_VERDANTURF_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_PACIFIDLOG_TOWN_POKEMON_CENTER_1F"),
        group: 7,
        num: 0,
        name: "PacifidlogTown_PokemonCenter_1F",
        layout: LayoutId("LAYOUT_POKEMON_CENTER_1F"),
        music: MusicId(400), // MUS_POKE_CENTER
        region_map_section: RegionMapSectionId("MAPSEC_PACIFIDLOG_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_PACIFIDLOG_TOWN_POKEMON_CENTER_2F"),
        group: 7,
        num: 1,
        name: "PacifidlogTown_PokemonCenter_2F",
        layout: LayoutId("LAYOUT_POKEMON_CENTER_2F"),
        music: MusicId(400), // MUS_POKE_CENTER
        region_map_section: RegionMapSectionId("MAPSEC_PACIFIDLOG_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_PACIFIDLOG_TOWN_HOUSE1"),
        group: 7,
        num: 2,
        name: "PacifidlogTown_House1",
        layout: LayoutId("LAYOUT_PACIFIDLOG_TOWN_HOUSE1"),
        music: MusicId(408), // MUS_LILYCOVE
        region_map_section: RegionMapSectionId("MAPSEC_PACIFIDLOG_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_PACIFIDLOG_TOWN_HOUSE2"),
        group: 7,
        num: 3,
        name: "PacifidlogTown_House2",
        layout: LayoutId("LAYOUT_PACIFIDLOG_TOWN_HOUSE2"),
        music: MusicId(408), // MUS_LILYCOVE
        region_map_section: RegionMapSectionId("MAPSEC_PACIFIDLOG_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_PACIFIDLOG_TOWN_HOUSE3"),
        group: 7,
        num: 4,
        name: "PacifidlogTown_House3",
        layout: LayoutId("LAYOUT_PACIFIDLOG_TOWN_HOUSE1"),
        music: MusicId(408), // MUS_LILYCOVE
        region_map_section: RegionMapSectionId("MAPSEC_PACIFIDLOG_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_PACIFIDLOG_TOWN_HOUSE4"),
        group: 7,
        num: 5,
        name: "PacifidlogTown_House4",
        layout: LayoutId("LAYOUT_PACIFIDLOG_TOWN_HOUSE2"),
        music: MusicId(408), // MUS_LILYCOVE
        region_map_section: RegionMapSectionId("MAPSEC_PACIFIDLOG_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_PACIFIDLOG_TOWN_HOUSE5"),
        group: 7,
        num: 6,
        name: "PacifidlogTown_House5",
        layout: LayoutId("LAYOUT_PACIFIDLOG_TOWN_HOUSE1"),
        music: MusicId(408), // MUS_LILYCOVE
        region_map_section: RegionMapSectionId("MAPSEC_PACIFIDLOG_TOWN"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_PETALBURG_CITY_WALLYS_HOUSE"),
        group: 8,
        num: 0,
        name: "PetalburgCity_WallysHouse",
        layout: LayoutId("LAYOUT_HOUSE2"),
        music: MusicId(362), // MUS_PETALBURG
        region_map_section: RegionMapSectionId("MAPSEC_PETALBURG_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_PETALBURG_CITY_GYM"),
        group: 8,
        num: 1,
        name: "PetalburgCity_Gym",
        layout: LayoutId("LAYOUT_PETALBURG_CITY_GYM"),
        music: MusicId(364), // MUS_GYM
        region_map_section: RegionMapSectionId("MAPSEC_PETALBURG_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Gym, // MAP_BATTLE_SCENE_GYM
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_PETALBURG_CITY_HOUSE1"),
        group: 8,
        num: 2,
        name: "PetalburgCity_House1",
        layout: LayoutId("LAYOUT_HOUSE1"),
        music: MusicId(362), // MUS_PETALBURG
        region_map_section: RegionMapSectionId("MAPSEC_PETALBURG_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_PETALBURG_CITY_HOUSE2"),
        group: 8,
        num: 3,
        name: "PetalburgCity_House2",
        layout: LayoutId("LAYOUT_HOUSE_WITH_BED"),
        music: MusicId(362), // MUS_PETALBURG
        region_map_section: RegionMapSectionId("MAPSEC_PETALBURG_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_PETALBURG_CITY_POKEMON_CENTER_1F"),
        group: 8,
        num: 4,
        name: "PetalburgCity_PokemonCenter_1F",
        layout: LayoutId("LAYOUT_POKEMON_CENTER_1F"),
        music: MusicId(400), // MUS_POKE_CENTER
        region_map_section: RegionMapSectionId("MAPSEC_PETALBURG_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_PETALBURG_CITY_POKEMON_CENTER_2F"),
        group: 8,
        num: 5,
        name: "PetalburgCity_PokemonCenter_2F",
        layout: LayoutId("LAYOUT_POKEMON_CENTER_2F"),
        music: MusicId(400), // MUS_POKE_CENTER
        region_map_section: RegionMapSectionId("MAPSEC_PETALBURG_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_PETALBURG_CITY_MART"),
        group: 8,
        num: 6,
        name: "PetalburgCity_Mart",
        layout: LayoutId("LAYOUT_MART"),
        music: MusicId(404), // MUS_POKE_MART
        region_map_section: RegionMapSectionId("MAPSEC_PETALBURG_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SLATEPORT_CITY_STERNS_SHIPYARD_1F"),
        group: 9,
        num: 0,
        name: "SlateportCity_SternsShipyard_1F",
        layout: LayoutId("LAYOUT_SLATEPORT_CITY_STERNS_SHIPYARD_1F"),
        music: MusicId(433), // MUS_SLATEPORT
        region_map_section: RegionMapSectionId("MAPSEC_SLATEPORT_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SLATEPORT_CITY_STERNS_SHIPYARD_2F"),
        group: 9,
        num: 1,
        name: "SlateportCity_SternsShipyard_2F",
        layout: LayoutId("LAYOUT_SLATEPORT_CITY_STERNS_SHIPYARD_2F"),
        music: MusicId(433), // MUS_SLATEPORT
        region_map_section: RegionMapSectionId("MAPSEC_SLATEPORT_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SLATEPORT_CITY_BATTLE_TENT_LOBBY"),
        group: 9,
        num: 2,
        name: "SlateportCity_BattleTentLobby",
        layout: LayoutId("LAYOUT_BATTLE_TENT_LOBBY"),
        music: MusicId(384), // MUS_B_TOWER_RS
        region_map_section: RegionMapSectionId("MAPSEC_SLATEPORT_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SLATEPORT_CITY_BATTLE_TENT_CORRIDOR"),
        group: 9,
        num: 3,
        name: "SlateportCity_BattleTentCorridor",
        layout: LayoutId("LAYOUT_BATTLE_TENT_CORRIDOR"),
        music: MusicId(384), // MUS_B_TOWER_RS
        region_map_section: RegionMapSectionId("MAPSEC_SLATEPORT_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SLATEPORT_CITY_BATTLE_TENT_BATTLE_ROOM"),
        group: 9,
        num: 4,
        name: "SlateportCity_BattleTentBattleRoom",
        layout: LayoutId("LAYOUT_BATTLE_TENT_BATTLE_ROOM"),
        music: MusicId(384), // MUS_B_TOWER_RS
        region_map_section: RegionMapSectionId("MAPSEC_SLATEPORT_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SLATEPORT_CITY_NAME_RATERS_HOUSE"),
        group: 9,
        num: 5,
        name: "SlateportCity_NameRatersHouse",
        layout: LayoutId("LAYOUT_HOUSE_WITH_BED"),
        music: MusicId(433), // MUS_SLATEPORT
        region_map_section: RegionMapSectionId("MAPSEC_SLATEPORT_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SLATEPORT_CITY_POKEMON_FAN_CLUB"),
        group: 9,
        num: 6,
        name: "SlateportCity_PokemonFanClub",
        layout: LayoutId("LAYOUT_SLATEPORT_CITY_POKEMON_FAN_CLUB"),
        music: MusicId(433), // MUS_SLATEPORT
        region_map_section: RegionMapSectionId("MAPSEC_SLATEPORT_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SLATEPORT_CITY_OCEANIC_MUSEUM_1F"),
        group: 9,
        num: 7,
        name: "SlateportCity_OceanicMuseum_1F",
        layout: LayoutId("LAYOUT_SLATEPORT_CITY_OCEANIC_MUSEUM_1F"),
        music: MusicId(375), // MUS_OCEANIC_MUSEUM
        region_map_section: RegionMapSectionId("MAPSEC_SLATEPORT_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SLATEPORT_CITY_OCEANIC_MUSEUM_2F"),
        group: 9,
        num: 8,
        name: "SlateportCity_OceanicMuseum_2F",
        layout: LayoutId("LAYOUT_SLATEPORT_CITY_OCEANIC_MUSEUM_2F"),
        music: MusicId(375), // MUS_OCEANIC_MUSEUM
        region_map_section: RegionMapSectionId("MAPSEC_SLATEPORT_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SLATEPORT_CITY_HARBOR"),
        group: 9,
        num: 9,
        name: "SlateportCity_Harbor",
        layout: LayoutId("LAYOUT_HARBOR"),
        music: MusicId(433), // MUS_SLATEPORT
        region_map_section: RegionMapSectionId("MAPSEC_SLATEPORT_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SLATEPORT_CITY_HOUSE"),
        group: 9,
        num: 10,
        name: "SlateportCity_House",
        layout: LayoutId("LAYOUT_HOUSE2"),
        music: MusicId(433), // MUS_SLATEPORT
        region_map_section: RegionMapSectionId("MAPSEC_SLATEPORT_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SLATEPORT_CITY_POKEMON_CENTER_1F"),
        group: 9,
        num: 11,
        name: "SlateportCity_PokemonCenter_1F",
        layout: LayoutId("LAYOUT_POKEMON_CENTER_1F"),
        music: MusicId(400), // MUS_POKE_CENTER
        region_map_section: RegionMapSectionId("MAPSEC_SLATEPORT_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SLATEPORT_CITY_POKEMON_CENTER_2F"),
        group: 9,
        num: 12,
        name: "SlateportCity_PokemonCenter_2F",
        layout: LayoutId("LAYOUT_POKEMON_CENTER_2F"),
        music: MusicId(400), // MUS_POKE_CENTER
        region_map_section: RegionMapSectionId("MAPSEC_SLATEPORT_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SLATEPORT_CITY_MART"),
        group: 9,
        num: 13,
        name: "SlateportCity_Mart",
        layout: LayoutId("LAYOUT_MART"),
        music: MusicId(404), // MUS_POKE_MART
        region_map_section: RegionMapSectionId("MAPSEC_SLATEPORT_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MAUVILLE_CITY_GYM"),
        group: 10,
        num: 0,
        name: "MauvilleCity_Gym",
        layout: LayoutId("LAYOUT_MAUVILLE_CITY_GYM"),
        music: MusicId(364), // MUS_GYM
        region_map_section: RegionMapSectionId("MAPSEC_MAUVILLE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Gym, // MAP_BATTLE_SCENE_GYM
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MAUVILLE_CITY_BIKE_SHOP"),
        group: 10,
        num: 1,
        name: "MauvilleCity_BikeShop",
        layout: LayoutId("LAYOUT_MAUVILLE_CITY_BIKE_SHOP"),
        music: MusicId(399), // MUS_RUSTBORO
        region_map_section: RegionMapSectionId("MAPSEC_MAUVILLE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MAUVILLE_CITY_HOUSE1"),
        group: 10,
        num: 2,
        name: "MauvilleCity_House1",
        layout: LayoutId("LAYOUT_HOUSE2"),
        music: MusicId(399), // MUS_RUSTBORO
        region_map_section: RegionMapSectionId("MAPSEC_MAUVILLE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MAUVILLE_CITY_GAME_CORNER"),
        group: 10,
        num: 3,
        name: "MauvilleCity_GameCorner",
        layout: LayoutId("LAYOUT_MAUVILLE_CITY_GAME_CORNER"),
        music: MusicId(426), // MUS_GAME_CORNER
        region_map_section: RegionMapSectionId("MAPSEC_MAUVILLE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MAUVILLE_CITY_HOUSE2"),
        group: 10,
        num: 4,
        name: "MauvilleCity_House2",
        layout: LayoutId("LAYOUT_HOUSE1"),
        music: MusicId(399), // MUS_RUSTBORO
        region_map_section: RegionMapSectionId("MAPSEC_MAUVILLE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MAUVILLE_CITY_POKEMON_CENTER_1F"),
        group: 10,
        num: 5,
        name: "MauvilleCity_PokemonCenter_1F",
        layout: LayoutId("LAYOUT_POKEMON_CENTER_1F"),
        music: MusicId(400), // MUS_POKE_CENTER
        region_map_section: RegionMapSectionId("MAPSEC_MAUVILLE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MAUVILLE_CITY_POKEMON_CENTER_2F"),
        group: 10,
        num: 6,
        name: "MauvilleCity_PokemonCenter_2F",
        layout: LayoutId("LAYOUT_POKEMON_CENTER_2F"),
        music: MusicId(400), // MUS_POKE_CENTER
        region_map_section: RegionMapSectionId("MAPSEC_MAUVILLE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MAUVILLE_CITY_MART"),
        group: 10,
        num: 7,
        name: "MauvilleCity_Mart",
        layout: LayoutId("LAYOUT_MART"),
        music: MusicId(404), // MUS_POKE_MART
        region_map_section: RegionMapSectionId("MAPSEC_MAUVILLE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_RUSTBORO_CITY_DEVON_CORP_1F"),
        group: 11,
        num: 0,
        name: "RustboroCity_DevonCorp_1F",
        layout: LayoutId("LAYOUT_RUSTBORO_CITY_DEVON_CORP_1F"),
        music: MusicId(399), // MUS_RUSTBORO
        region_map_section: RegionMapSectionId("MAPSEC_RUSTBORO_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_RUSTBORO_CITY_DEVON_CORP_2F"),
        group: 11,
        num: 1,
        name: "RustboroCity_DevonCorp_2F",
        layout: LayoutId("LAYOUT_RUSTBORO_CITY_DEVON_CORP_2F"),
        music: MusicId(399), // MUS_RUSTBORO
        region_map_section: RegionMapSectionId("MAPSEC_RUSTBORO_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_RUSTBORO_CITY_DEVON_CORP_3F"),
        group: 11,
        num: 2,
        name: "RustboroCity_DevonCorp_3F",
        layout: LayoutId("LAYOUT_RUSTBORO_CITY_DEVON_CORP_3F"),
        music: MusicId(399), // MUS_RUSTBORO
        region_map_section: RegionMapSectionId("MAPSEC_RUSTBORO_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_RUSTBORO_CITY_GYM"),
        group: 11,
        num: 3,
        name: "RustboroCity_Gym",
        layout: LayoutId("LAYOUT_RUSTBORO_CITY_GYM"),
        music: MusicId(364), // MUS_GYM
        region_map_section: RegionMapSectionId("MAPSEC_RUSTBORO_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Gym, // MAP_BATTLE_SCENE_GYM
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_RUSTBORO_CITY_POKEMON_SCHOOL"),
        group: 11,
        num: 4,
        name: "RustboroCity_PokemonSchool",
        layout: LayoutId("LAYOUT_RUSTBORO_CITY_POKEMON_SCHOOL"),
        music: MusicId(435), // MUS_SCHOOL
        region_map_section: RegionMapSectionId("MAPSEC_RUSTBORO_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_RUSTBORO_CITY_POKEMON_CENTER_1F"),
        group: 11,
        num: 5,
        name: "RustboroCity_PokemonCenter_1F",
        layout: LayoutId("LAYOUT_POKEMON_CENTER_1F"),
        music: MusicId(400), // MUS_POKE_CENTER
        region_map_section: RegionMapSectionId("MAPSEC_RUSTBORO_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_RUSTBORO_CITY_POKEMON_CENTER_2F"),
        group: 11,
        num: 6,
        name: "RustboroCity_PokemonCenter_2F",
        layout: LayoutId("LAYOUT_POKEMON_CENTER_2F"),
        music: MusicId(400), // MUS_POKE_CENTER
        region_map_section: RegionMapSectionId("MAPSEC_RUSTBORO_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_RUSTBORO_CITY_MART"),
        group: 11,
        num: 7,
        name: "RustboroCity_Mart",
        layout: LayoutId("LAYOUT_MART"),
        music: MusicId(404), // MUS_POKE_MART
        region_map_section: RegionMapSectionId("MAPSEC_RUSTBORO_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_RUSTBORO_CITY_FLAT1_1F"),
        group: 11,
        num: 8,
        name: "RustboroCity_Flat1_1F",
        layout: LayoutId("LAYOUT_RUSTBORO_CITY_FLAT1_1F"),
        music: MusicId(399), // MUS_RUSTBORO
        region_map_section: RegionMapSectionId("MAPSEC_RUSTBORO_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_RUSTBORO_CITY_FLAT1_2F"),
        group: 11,
        num: 9,
        name: "RustboroCity_Flat1_2F",
        layout: LayoutId("LAYOUT_RUSTBORO_CITY_FLAT1_2F"),
        music: MusicId(399), // MUS_RUSTBORO
        region_map_section: RegionMapSectionId("MAPSEC_RUSTBORO_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_RUSTBORO_CITY_HOUSE1"),
        group: 11,
        num: 10,
        name: "RustboroCity_House1",
        layout: LayoutId("LAYOUT_RUSTBORO_CITY_HOUSE1"),
        music: MusicId(399), // MUS_RUSTBORO
        region_map_section: RegionMapSectionId("MAPSEC_RUSTBORO_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_RUSTBORO_CITY_CUTTERS_HOUSE"),
        group: 11,
        num: 11,
        name: "RustboroCity_CuttersHouse",
        layout: LayoutId("LAYOUT_RUSTBORO_CITY_CUTTERS_HOUSE"),
        music: MusicId(399), // MUS_RUSTBORO
        region_map_section: RegionMapSectionId("MAPSEC_RUSTBORO_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_RUSTBORO_CITY_HOUSE2"),
        group: 11,
        num: 12,
        name: "RustboroCity_House2",
        layout: LayoutId("LAYOUT_RUSTBORO_CITY_HOUSE"),
        music: MusicId(399), // MUS_RUSTBORO
        region_map_section: RegionMapSectionId("MAPSEC_RUSTBORO_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_RUSTBORO_CITY_FLAT2_1F"),
        group: 11,
        num: 13,
        name: "RustboroCity_Flat2_1F",
        layout: LayoutId("LAYOUT_RUSTBORO_CITY_FLAT2_1F"),
        music: MusicId(399), // MUS_RUSTBORO
        region_map_section: RegionMapSectionId("MAPSEC_RUSTBORO_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_RUSTBORO_CITY_FLAT2_2F"),
        group: 11,
        num: 14,
        name: "RustboroCity_Flat2_2F",
        layout: LayoutId("LAYOUT_RUSTBORO_CITY_FLAT2_2F"),
        music: MusicId(399), // MUS_RUSTBORO
        region_map_section: RegionMapSectionId("MAPSEC_RUSTBORO_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_RUSTBORO_CITY_FLAT2_3F"),
        group: 11,
        num: 15,
        name: "RustboroCity_Flat2_3F",
        layout: LayoutId("LAYOUT_RUSTBORO_CITY_FLAT2_3F"),
        music: MusicId(399), // MUS_RUSTBORO
        region_map_section: RegionMapSectionId("MAPSEC_RUSTBORO_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_RUSTBORO_CITY_HOUSE3"),
        group: 11,
        num: 16,
        name: "RustboroCity_House3",
        layout: LayoutId("LAYOUT_RUSTBORO_CITY_HOUSE"),
        music: MusicId(399), // MUS_RUSTBORO
        region_map_section: RegionMapSectionId("MAPSEC_RUSTBORO_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_FORTREE_CITY_HOUSE1"),
        group: 12,
        num: 0,
        name: "FortreeCity_House1",
        layout: LayoutId("LAYOUT_FORTREE_CITY_HOUSE1"),
        music: MusicId(382), // MUS_FORTREE
        region_map_section: RegionMapSectionId("MAPSEC_FORTREE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_FORTREE_CITY_GYM"),
        group: 12,
        num: 1,
        name: "FortreeCity_Gym",
        layout: LayoutId("LAYOUT_FORTREE_CITY_GYM"),
        music: MusicId(364), // MUS_GYM
        region_map_section: RegionMapSectionId("MAPSEC_FORTREE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Gym, // MAP_BATTLE_SCENE_GYM
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_FORTREE_CITY_POKEMON_CENTER_1F"),
        group: 12,
        num: 2,
        name: "FortreeCity_PokemonCenter_1F",
        layout: LayoutId("LAYOUT_POKEMON_CENTER_1F"),
        music: MusicId(400), // MUS_POKE_CENTER
        region_map_section: RegionMapSectionId("MAPSEC_FORTREE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_FORTREE_CITY_POKEMON_CENTER_2F"),
        group: 12,
        num: 3,
        name: "FortreeCity_PokemonCenter_2F",
        layout: LayoutId("LAYOUT_POKEMON_CENTER_2F"),
        music: MusicId(400), // MUS_POKE_CENTER
        region_map_section: RegionMapSectionId("MAPSEC_FORTREE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_FORTREE_CITY_MART"),
        group: 12,
        num: 4,
        name: "FortreeCity_Mart",
        layout: LayoutId("LAYOUT_MART"),
        music: MusicId(404), // MUS_POKE_MART
        region_map_section: RegionMapSectionId("MAPSEC_FORTREE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_FORTREE_CITY_HOUSE2"),
        group: 12,
        num: 5,
        name: "FortreeCity_House2",
        layout: LayoutId("LAYOUT_FORTREE_CITY_HOUSE2"),
        music: MusicId(382), // MUS_FORTREE
        region_map_section: RegionMapSectionId("MAPSEC_FORTREE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_FORTREE_CITY_HOUSE3"),
        group: 12,
        num: 6,
        name: "FortreeCity_House3",
        layout: LayoutId("LAYOUT_FORTREE_CITY_HOUSE1"),
        music: MusicId(382), // MUS_FORTREE
        region_map_section: RegionMapSectionId("MAPSEC_FORTREE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_FORTREE_CITY_HOUSE4"),
        group: 12,
        num: 7,
        name: "FortreeCity_House4",
        layout: LayoutId("LAYOUT_FORTREE_CITY_HOUSE2"),
        music: MusicId(382), // MUS_FORTREE
        region_map_section: RegionMapSectionId("MAPSEC_FORTREE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_FORTREE_CITY_HOUSE5"),
        group: 12,
        num: 8,
        name: "FortreeCity_House5",
        layout: LayoutId("LAYOUT_FORTREE_CITY_HOUSE1"),
        music: MusicId(382), // MUS_FORTREE
        region_map_section: RegionMapSectionId("MAPSEC_FORTREE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_FORTREE_CITY_DECORATION_SHOP"),
        group: 12,
        num: 9,
        name: "FortreeCity_DecorationShop",
        layout: LayoutId("LAYOUT_FORTREE_CITY_DECORATION_SHOP"),
        music: MusicId(382), // MUS_FORTREE
        region_map_section: RegionMapSectionId("MAPSEC_FORTREE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_LILYCOVE_CITY_COVE_LILY_MOTEL_1F"),
        group: 13,
        num: 0,
        name: "LilycoveCity_CoveLilyMotel_1F",
        layout: LayoutId("LAYOUT_LILYCOVE_CITY_COVE_LILY_MOTEL_1F"),
        music: MusicId(408), // MUS_LILYCOVE
        region_map_section: RegionMapSectionId("MAPSEC_LILYCOVE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_LILYCOVE_CITY_COVE_LILY_MOTEL_2F"),
        group: 13,
        num: 1,
        name: "LilycoveCity_CoveLilyMotel_2F",
        layout: LayoutId("LAYOUT_LILYCOVE_CITY_COVE_LILY_MOTEL_2F"),
        music: MusicId(408), // MUS_LILYCOVE
        region_map_section: RegionMapSectionId("MAPSEC_LILYCOVE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_LILYCOVE_CITY_LILYCOVE_MUSEUM_1F"),
        group: 13,
        num: 2,
        name: "LilycoveCity_LilycoveMuseum_1F",
        layout: LayoutId("LAYOUT_LILYCOVE_CITY_LILYCOVE_MUSEUM_1F"),
        music: MusicId(373), // MUS_LILYCOVE_MUSEUM
        region_map_section: RegionMapSectionId("MAPSEC_LILYCOVE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_LILYCOVE_CITY_LILYCOVE_MUSEUM_2F"),
        group: 13,
        num: 3,
        name: "LilycoveCity_LilycoveMuseum_2F",
        layout: LayoutId("LAYOUT_LILYCOVE_CITY_LILYCOVE_MUSEUM_2F"),
        music: MusicId(373), // MUS_LILYCOVE_MUSEUM
        region_map_section: RegionMapSectionId("MAPSEC_LILYCOVE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_LILYCOVE_CITY_CONTEST_LOBBY"),
        group: 13,
        num: 4,
        name: "LilycoveCity_ContestLobby",
        layout: LayoutId("LAYOUT_LILYCOVE_CITY_CONTEST_LOBBY"),
        music: MusicId(452), // MUS_CONTEST_LOBBY
        region_map_section: RegionMapSectionId("MAPSEC_LILYCOVE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_LILYCOVE_CITY_CONTEST_HALL"),
        group: 13,
        num: 5,
        name: "LilycoveCity_ContestHall",
        layout: LayoutId("LAYOUT_LILYCOVE_CITY_CONTEST_HALL"),
        music: MusicId(452), // MUS_CONTEST_LOBBY
        region_map_section: RegionMapSectionId("MAPSEC_LILYCOVE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_LILYCOVE_CITY_POKEMON_CENTER_1F"),
        group: 13,
        num: 6,
        name: "LilycoveCity_PokemonCenter_1F",
        layout: LayoutId("LAYOUT_POKEMON_CENTER_1F"),
        music: MusicId(400), // MUS_POKE_CENTER
        region_map_section: RegionMapSectionId("MAPSEC_LILYCOVE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_LILYCOVE_CITY_POKEMON_CENTER_2F"),
        group: 13,
        num: 7,
        name: "LilycoveCity_PokemonCenter_2F",
        layout: LayoutId("LAYOUT_POKEMON_CENTER_2F"),
        music: MusicId(400), // MUS_POKE_CENTER
        region_map_section: RegionMapSectionId("MAPSEC_LILYCOVE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_LILYCOVE_CITY_UNUSED_MART"),
        group: 13,
        num: 8,
        name: "LilycoveCity_UnusedMart",
        layout: LayoutId("LAYOUT_MART"),
        music: MusicId(404), // MUS_POKE_MART
        region_map_section: RegionMapSectionId("MAPSEC_LILYCOVE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_LILYCOVE_CITY_POKEMON_TRAINER_FAN_CLUB"),
        group: 13,
        num: 9,
        name: "LilycoveCity_PokemonTrainerFanClub",
        layout: LayoutId("LAYOUT_LILYCOVE_CITY_POKEMON_TRAINER_FAN_CLUB"),
        music: MusicId(408), // MUS_LILYCOVE
        region_map_section: RegionMapSectionId("MAPSEC_LILYCOVE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_LILYCOVE_CITY_HARBOR"),
        group: 13,
        num: 10,
        name: "LilycoveCity_Harbor",
        layout: LayoutId("LAYOUT_HARBOR"),
        music: MusicId(408), // MUS_LILYCOVE
        region_map_section: RegionMapSectionId("MAPSEC_LILYCOVE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_LILYCOVE_CITY_MOVE_DELETERS_HOUSE"),
        group: 13,
        num: 11,
        name: "LilycoveCity_MoveDeletersHouse",
        layout: LayoutId("LAYOUT_HOUSE2"),
        music: MusicId(408), // MUS_LILYCOVE
        region_map_section: RegionMapSectionId("MAPSEC_LILYCOVE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_LILYCOVE_CITY_HOUSE1"),
        group: 13,
        num: 12,
        name: "LilycoveCity_House1",
        layout: LayoutId("LAYOUT_HOUSE1"),
        music: MusicId(408), // MUS_LILYCOVE
        region_map_section: RegionMapSectionId("MAPSEC_LILYCOVE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_LILYCOVE_CITY_HOUSE2"),
        group: 13,
        num: 13,
        name: "LilycoveCity_House2",
        layout: LayoutId("LAYOUT_LILYCOVE_CITY_HOUSE2"),
        music: MusicId(408), // MUS_LILYCOVE
        region_map_section: RegionMapSectionId("MAPSEC_LILYCOVE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_LILYCOVE_CITY_HOUSE3"),
        group: 13,
        num: 14,
        name: "LilycoveCity_House3",
        layout: LayoutId("LAYOUT_HOUSE2"),
        music: MusicId(408), // MUS_LILYCOVE
        region_map_section: RegionMapSectionId("MAPSEC_LILYCOVE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_LILYCOVE_CITY_HOUSE4"),
        group: 13,
        num: 15,
        name: "LilycoveCity_House4",
        layout: LayoutId("LAYOUT_HOUSE1"),
        music: MusicId(408), // MUS_LILYCOVE
        region_map_section: RegionMapSectionId("MAPSEC_LILYCOVE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_LILYCOVE_CITY_DEPARTMENT_STORE_1F"),
        group: 13,
        num: 16,
        name: "LilycoveCity_DepartmentStore_1F",
        layout: LayoutId("LAYOUT_LILYCOVE_CITY_DEPARTMENT_STORE_1F"),
        music: MusicId(408), // MUS_LILYCOVE
        region_map_section: RegionMapSectionId("MAPSEC_LILYCOVE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_LILYCOVE_CITY_DEPARTMENT_STORE_2F"),
        group: 13,
        num: 17,
        name: "LilycoveCity_DepartmentStore_2F",
        layout: LayoutId("LAYOUT_LILYCOVE_CITY_DEPARTMENT_STORE_2F"),
        music: MusicId(404), // MUS_POKE_MART
        region_map_section: RegionMapSectionId("MAPSEC_LILYCOVE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_LILYCOVE_CITY_DEPARTMENT_STORE_3F"),
        group: 13,
        num: 18,
        name: "LilycoveCity_DepartmentStore_3F",
        layout: LayoutId("LAYOUT_LILYCOVE_CITY_DEPARTMENT_STORE_3F"),
        music: MusicId(404), // MUS_POKE_MART
        region_map_section: RegionMapSectionId("MAPSEC_LILYCOVE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_LILYCOVE_CITY_DEPARTMENT_STORE_4F"),
        group: 13,
        num: 19,
        name: "LilycoveCity_DepartmentStore_4F",
        layout: LayoutId("LAYOUT_LILYCOVE_CITY_DEPARTMENT_STORE_4F"),
        music: MusicId(404), // MUS_POKE_MART
        region_map_section: RegionMapSectionId("MAPSEC_LILYCOVE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_LILYCOVE_CITY_DEPARTMENT_STORE_5F"),
        group: 13,
        num: 20,
        name: "LilycoveCity_DepartmentStore_5F",
        layout: LayoutId("LAYOUT_LILYCOVE_CITY_DEPARTMENT_STORE_5F"),
        music: MusicId(404), // MUS_POKE_MART
        region_map_section: RegionMapSectionId("MAPSEC_LILYCOVE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_LILYCOVE_CITY_DEPARTMENT_STORE_ROOFTOP"),
        group: 13,
        num: 21,
        name: "LilycoveCity_DepartmentStoreRooftop",
        layout: LayoutId("LAYOUT_LILYCOVE_CITY_DEPARTMENT_STORE_ROOFTOP"),
        music: MusicId(408), // MUS_LILYCOVE
        region_map_section: RegionMapSectionId("MAPSEC_LILYCOVE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_LILYCOVE_CITY_DEPARTMENT_STORE_ELEVATOR"),
        group: 13,
        num: 22,
        name: "LilycoveCity_DepartmentStoreElevator",
        layout: LayoutId("LAYOUT_LILYCOVE_CITY_DEPARTMENT_STORE_ELEVATOR"),
        music: MusicId(408), // MUS_LILYCOVE
        region_map_section: RegionMapSectionId("MAPSEC_LILYCOVE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MOSSDEEP_CITY_GYM"),
        group: 14,
        num: 0,
        name: "MossdeepCity_Gym",
        layout: LayoutId("LAYOUT_MOSSDEEP_CITY_GYM"),
        music: MusicId(364), // MUS_GYM
        region_map_section: RegionMapSectionId("MAPSEC_MOSSDEEP_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Gym, // MAP_BATTLE_SCENE_GYM
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MOSSDEEP_CITY_HOUSE1"),
        group: 14,
        num: 1,
        name: "MossdeepCity_House1",
        layout: LayoutId("LAYOUT_HOUSE2"),
        music: MusicId(399), // MUS_RUSTBORO
        region_map_section: RegionMapSectionId("MAPSEC_MOSSDEEP_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MOSSDEEP_CITY_HOUSE2"),
        group: 14,
        num: 2,
        name: "MossdeepCity_House2",
        layout: LayoutId("LAYOUT_HOUSE1"),
        music: MusicId(399), // MUS_RUSTBORO
        region_map_section: RegionMapSectionId("MAPSEC_MOSSDEEP_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MOSSDEEP_CITY_POKEMON_CENTER_1F"),
        group: 14,
        num: 3,
        name: "MossdeepCity_PokemonCenter_1F",
        layout: LayoutId("LAYOUT_POKEMON_CENTER_1F"),
        music: MusicId(400), // MUS_POKE_CENTER
        region_map_section: RegionMapSectionId("MAPSEC_MOSSDEEP_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MOSSDEEP_CITY_POKEMON_CENTER_2F"),
        group: 14,
        num: 4,
        name: "MossdeepCity_PokemonCenter_2F",
        layout: LayoutId("LAYOUT_POKEMON_CENTER_2F"),
        music: MusicId(400), // MUS_POKE_CENTER
        region_map_section: RegionMapSectionId("MAPSEC_MOSSDEEP_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MOSSDEEP_CITY_MART"),
        group: 14,
        num: 5,
        name: "MossdeepCity_Mart",
        layout: LayoutId("LAYOUT_MART"),
        music: MusicId(404), // MUS_POKE_MART
        region_map_section: RegionMapSectionId("MAPSEC_MOSSDEEP_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MOSSDEEP_CITY_HOUSE3"),
        group: 14,
        num: 6,
        name: "MossdeepCity_House3",
        layout: LayoutId("LAYOUT_HOUSE2"),
        music: MusicId(399), // MUS_RUSTBORO
        region_map_section: RegionMapSectionId("MAPSEC_MOSSDEEP_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MOSSDEEP_CITY_STEVENS_HOUSE"),
        group: 14,
        num: 7,
        name: "MossdeepCity_StevensHouse",
        layout: LayoutId("LAYOUT_MOSSDEEP_CITY_STEVENS_HOUSE"),
        music: MusicId(399), // MUS_RUSTBORO
        region_map_section: RegionMapSectionId("MAPSEC_MOSSDEEP_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MOSSDEEP_CITY_HOUSE4"),
        group: 14,
        num: 8,
        name: "MossdeepCity_House4",
        layout: LayoutId("LAYOUT_HOUSE_WITH_BED"),
        music: MusicId(399), // MUS_RUSTBORO
        region_map_section: RegionMapSectionId("MAPSEC_MOSSDEEP_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MOSSDEEP_CITY_SPACE_CENTER_1F"),
        group: 14,
        num: 9,
        name: "MossdeepCity_SpaceCenter_1F",
        layout: LayoutId("LAYOUT_MOSSDEEP_CITY_SPACE_CENTER_1F"),
        music: MusicId(399), // MUS_RUSTBORO
        region_map_section: RegionMapSectionId("MAPSEC_MOSSDEEP_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MOSSDEEP_CITY_SPACE_CENTER_2F"),
        group: 14,
        num: 10,
        name: "MossdeepCity_SpaceCenter_2F",
        layout: LayoutId("LAYOUT_MOSSDEEP_CITY_SPACE_CENTER_2F"),
        music: MusicId(399), // MUS_RUSTBORO
        region_map_section: RegionMapSectionId("MAPSEC_MOSSDEEP_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MOSSDEEP_CITY_GAME_CORNER_1F"),
        group: 14,
        num: 11,
        name: "MossdeepCity_GameCorner_1F",
        layout: LayoutId("LAYOUT_MOSSDEEP_CITY_GAME_CORNER_1F"),
        music: MusicId(399), // MUS_RUSTBORO
        region_map_section: RegionMapSectionId("MAPSEC_MOSSDEEP_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MOSSDEEP_CITY_GAME_CORNER_B1F"),
        group: 14,
        num: 12,
        name: "MossdeepCity_GameCorner_B1F",
        layout: LayoutId("LAYOUT_MOSSDEEP_CITY_GAME_CORNER_B1F"),
        music: MusicId(399), // MUS_RUSTBORO
        region_map_section: RegionMapSectionId("MAPSEC_MOSSDEEP_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SOOTOPOLIS_CITY_GYM_1F"),
        group: 15,
        num: 0,
        name: "SootopolisCity_Gym_1F",
        layout: LayoutId("LAYOUT_SOOTOPOLIS_CITY_GYM_1F"),
        music: MusicId(364), // MUS_GYM
        region_map_section: RegionMapSectionId("MAPSEC_SOOTOPOLIS_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Gym, // MAP_BATTLE_SCENE_GYM
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SOOTOPOLIS_CITY_GYM_B1F"),
        group: 15,
        num: 1,
        name: "SootopolisCity_Gym_B1F",
        layout: LayoutId("LAYOUT_SOOTOPOLIS_CITY_GYM_B1F"),
        music: MusicId(364), // MUS_GYM
        region_map_section: RegionMapSectionId("MAPSEC_SOOTOPOLIS_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Gym, // MAP_BATTLE_SCENE_GYM
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SOOTOPOLIS_CITY_POKEMON_CENTER_1F"),
        group: 15,
        num: 2,
        name: "SootopolisCity_PokemonCenter_1F",
        layout: LayoutId("LAYOUT_POKEMON_CENTER_1F"),
        music: MusicId(400), // MUS_POKE_CENTER
        region_map_section: RegionMapSectionId("MAPSEC_SOOTOPOLIS_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SOOTOPOLIS_CITY_POKEMON_CENTER_2F"),
        group: 15,
        num: 3,
        name: "SootopolisCity_PokemonCenter_2F",
        layout: LayoutId("LAYOUT_POKEMON_CENTER_2F"),
        music: MusicId(400), // MUS_POKE_CENTER
        region_map_section: RegionMapSectionId("MAPSEC_SOOTOPOLIS_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SOOTOPOLIS_CITY_MART"),
        group: 15,
        num: 4,
        name: "SootopolisCity_Mart",
        layout: LayoutId("LAYOUT_MART"),
        music: MusicId(404), // MUS_POKE_MART
        region_map_section: RegionMapSectionId("MAPSEC_SOOTOPOLIS_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SOOTOPOLIS_CITY_HOUSE1"),
        group: 15,
        num: 5,
        name: "SootopolisCity_House1",
        layout: LayoutId("LAYOUT_SOOTOPOLIS_CITY_HOUSE1"),
        music: MusicId(445), // MUS_SOOTOPOLIS
        region_map_section: RegionMapSectionId("MAPSEC_SOOTOPOLIS_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SOOTOPOLIS_CITY_HOUSE2"),
        group: 15,
        num: 6,
        name: "SootopolisCity_House2",
        layout: LayoutId("LAYOUT_SOOTOPOLIS_CITY_HOUSE2"),
        music: MusicId(445), // MUS_SOOTOPOLIS
        region_map_section: RegionMapSectionId("MAPSEC_SOOTOPOLIS_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SOOTOPOLIS_CITY_HOUSE3"),
        group: 15,
        num: 7,
        name: "SootopolisCity_House3",
        layout: LayoutId("LAYOUT_SOOTOPOLIS_CITY_HOUSE3"),
        music: MusicId(445), // MUS_SOOTOPOLIS
        region_map_section: RegionMapSectionId("MAPSEC_SOOTOPOLIS_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SOOTOPOLIS_CITY_HOUSE4"),
        group: 15,
        num: 8,
        name: "SootopolisCity_House4",
        layout: LayoutId("LAYOUT_SOOTOPOLIS_CITY_HOUSE1"),
        music: MusicId(445), // MUS_SOOTOPOLIS
        region_map_section: RegionMapSectionId("MAPSEC_SOOTOPOLIS_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SOOTOPOLIS_CITY_HOUSE5"),
        group: 15,
        num: 9,
        name: "SootopolisCity_House5",
        layout: LayoutId("LAYOUT_SOOTOPOLIS_CITY_HOUSE2"),
        music: MusicId(445), // MUS_SOOTOPOLIS
        region_map_section: RegionMapSectionId("MAPSEC_SOOTOPOLIS_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SOOTOPOLIS_CITY_HOUSE6"),
        group: 15,
        num: 10,
        name: "SootopolisCity_House6",
        layout: LayoutId("LAYOUT_SOOTOPOLIS_CITY_HOUSE3"),
        music: MusicId(445), // MUS_SOOTOPOLIS
        region_map_section: RegionMapSectionId("MAPSEC_SOOTOPOLIS_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SOOTOPOLIS_CITY_HOUSE7"),
        group: 15,
        num: 11,
        name: "SootopolisCity_House7",
        layout: LayoutId("LAYOUT_SOOTOPOLIS_CITY_HOUSE1"),
        music: MusicId(445), // MUS_SOOTOPOLIS
        region_map_section: RegionMapSectionId("MAPSEC_SOOTOPOLIS_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SOOTOPOLIS_CITY_LOTAD_AND_SEEDOT_HOUSE"),
        group: 15,
        num: 12,
        name: "SootopolisCity_LotadAndSeedotHouse",
        layout: LayoutId("LAYOUT_SOOTOPOLIS_CITY_LOTAD_AND_SEEDOT_HOUSE"),
        music: MusicId(445), // MUS_SOOTOPOLIS
        region_map_section: RegionMapSectionId("MAPSEC_SOOTOPOLIS_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SOOTOPOLIS_CITY_MYSTERY_EVENTS_HOUSE_1F"),
        group: 15,
        num: 13,
        name: "SootopolisCity_MysteryEventsHouse_1F",
        layout: LayoutId("LAYOUT_SOOTOPOLIS_CITY_MYSTERY_EVENTS_HOUSE_1F"),
        music: MusicId(445), // MUS_SOOTOPOLIS
        region_map_section: RegionMapSectionId("MAPSEC_SOOTOPOLIS_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SOOTOPOLIS_CITY_MYSTERY_EVENTS_HOUSE_B1F"),
        group: 15,
        num: 14,
        name: "SootopolisCity_MysteryEventsHouse_B1F",
        layout: LayoutId("LAYOUT_SOOTOPOLIS_CITY_MYSTERY_EVENTS_HOUSE_B1F"),
        music: MusicId(445), // MUS_SOOTOPOLIS
        region_map_section: RegionMapSectionId("MAPSEC_SOOTOPOLIS_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_EVER_GRANDE_CITY_SIDNEYS_ROOM"),
        group: 16,
        num: 0,
        name: "EverGrandeCity_SidneysRoom",
        layout: LayoutId("LAYOUT_EVER_GRANDE_CITY_SIDNEYS_ROOM"),
        music: MusicId(429), // MUS_VICTORY_ROAD
        region_map_section: RegionMapSectionId("MAPSEC_EVER_GRANDE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Sidney, // MAP_BATTLE_SCENE_SIDNEY
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_EVER_GRANDE_CITY_PHOEBES_ROOM"),
        group: 16,
        num: 1,
        name: "EverGrandeCity_PhoebesRoom",
        layout: LayoutId("LAYOUT_EVER_GRANDE_CITY_PHOEBES_ROOM"),
        music: MusicId(429), // MUS_VICTORY_ROAD
        region_map_section: RegionMapSectionId("MAPSEC_EVER_GRANDE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Phoebe, // MAP_BATTLE_SCENE_PHOEBE
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_EVER_GRANDE_CITY_GLACIAS_ROOM"),
        group: 16,
        num: 2,
        name: "EverGrandeCity_GlaciasRoom",
        layout: LayoutId("LAYOUT_EVER_GRANDE_CITY_GLACIAS_ROOM"),
        music: MusicId(429), // MUS_VICTORY_ROAD
        region_map_section: RegionMapSectionId("MAPSEC_EVER_GRANDE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Glacia, // MAP_BATTLE_SCENE_GLACIA
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_EVER_GRANDE_CITY_DRAKES_ROOM"),
        group: 16,
        num: 3,
        name: "EverGrandeCity_DrakesRoom",
        layout: LayoutId("LAYOUT_EVER_GRANDE_CITY_DRAKES_ROOM"),
        music: MusicId(429), // MUS_VICTORY_ROAD
        region_map_section: RegionMapSectionId("MAPSEC_EVER_GRANDE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Drake, // MAP_BATTLE_SCENE_DRAKE
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_EVER_GRANDE_CITY_CHAMPIONS_ROOM"),
        group: 16,
        num: 4,
        name: "EverGrandeCity_ChampionsRoom",
        layout: LayoutId("LAYOUT_EVER_GRANDE_CITY_CHAMPIONS_ROOM"),
        music: MusicId(429), // MUS_VICTORY_ROAD
        region_map_section: RegionMapSectionId("MAPSEC_EVER_GRANDE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_EVER_GRANDE_CITY_HALL1"),
        group: 16,
        num: 5,
        name: "EverGrandeCity_Hall1",
        layout: LayoutId("LAYOUT_EVER_GRANDE_CITY_SHORT_HALL"),
        music: MusicId(429), // MUS_VICTORY_ROAD
        region_map_section: RegionMapSectionId("MAPSEC_EVER_GRANDE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_EVER_GRANDE_CITY_HALL2"),
        group: 16,
        num: 6,
        name: "EverGrandeCity_Hall2",
        layout: LayoutId("LAYOUT_EVER_GRANDE_CITY_SHORT_HALL"),
        music: MusicId(429), // MUS_VICTORY_ROAD
        region_map_section: RegionMapSectionId("MAPSEC_EVER_GRANDE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_EVER_GRANDE_CITY_HALL3"),
        group: 16,
        num: 7,
        name: "EverGrandeCity_Hall3",
        layout: LayoutId("LAYOUT_EVER_GRANDE_CITY_SHORT_HALL"),
        music: MusicId(429), // MUS_VICTORY_ROAD
        region_map_section: RegionMapSectionId("MAPSEC_EVER_GRANDE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_EVER_GRANDE_CITY_HALL4"),
        group: 16,
        num: 8,
        name: "EverGrandeCity_Hall4",
        layout: LayoutId("LAYOUT_EVER_GRANDE_CITY_HALL4"),
        music: MusicId(429), // MUS_VICTORY_ROAD
        region_map_section: RegionMapSectionId("MAPSEC_EVER_GRANDE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_EVER_GRANDE_CITY_HALL5"),
        group: 16,
        num: 9,
        name: "EverGrandeCity_Hall5",
        layout: LayoutId("LAYOUT_EVER_GRANDE_CITY_SHORT_HALL"),
        music: MusicId(429), // MUS_VICTORY_ROAD
        region_map_section: RegionMapSectionId("MAPSEC_EVER_GRANDE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_EVER_GRANDE_CITY_POKEMON_LEAGUE_1F"),
        group: 16,
        num: 10,
        name: "EverGrandeCity_PokemonLeague_1F",
        layout: LayoutId("LAYOUT_EVER_GRANDE_CITY_POKEMON_LEAGUE_1F"),
        music: MusicId(400), // MUS_POKE_CENTER
        region_map_section: RegionMapSectionId("MAPSEC_EVER_GRANDE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_EVER_GRANDE_CITY_HALL_OF_FAME"),
        group: 16,
        num: 11,
        name: "EverGrandeCity_HallOfFame",
        layout: LayoutId("LAYOUT_EVER_GRANDE_CITY_HALL_OF_FAME"),
        music: MusicId(447), // MUS_HALL_OF_FAME_ROOM
        region_map_section: RegionMapSectionId("MAPSEC_EVER_GRANDE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_EVER_GRANDE_CITY_POKEMON_CENTER_1F"),
        group: 16,
        num: 12,
        name: "EverGrandeCity_PokemonCenter_1F",
        layout: LayoutId("LAYOUT_POKEMON_CENTER_1F"),
        music: MusicId(400), // MUS_POKE_CENTER
        region_map_section: RegionMapSectionId("MAPSEC_EVER_GRANDE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_EVER_GRANDE_CITY_POKEMON_CENTER_2F"),
        group: 16,
        num: 13,
        name: "EverGrandeCity_PokemonCenter_2F",
        layout: LayoutId("LAYOUT_POKEMON_CENTER_2F"),
        music: MusicId(400), // MUS_POKE_CENTER
        region_map_section: RegionMapSectionId("MAPSEC_EVER_GRANDE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_EVER_GRANDE_CITY_POKEMON_LEAGUE_2F"),
        group: 16,
        num: 14,
        name: "EverGrandeCity_PokemonLeague_2F",
        layout: LayoutId("LAYOUT_POKEMON_CENTER_2F"),
        music: MusicId(400), // MUS_POKE_CENTER
        region_map_section: RegionMapSectionId("MAPSEC_EVER_GRANDE_CITY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ROUTE104_MR_BRINEYS_HOUSE"),
        group: 17,
        num: 0,
        name: "Route104_MrBrineysHouse",
        layout: LayoutId("LAYOUT_ROUTE104_MR_BRINEYS_HOUSE"),
        music: MusicId(362), // MUS_PETALBURG
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_104"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ROUTE104_PRETTY_PETAL_FLOWER_SHOP"),
        group: 17,
        num: 1,
        name: "Route104_PrettyPetalFlowerShop",
        layout: LayoutId("LAYOUT_ROUTE104_PRETTY_PETAL_FLOWER_SHOP"),
        music: MusicId(362), // MUS_PETALBURG
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_104"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ROUTE111_WINSTRATE_FAMILYS_HOUSE"),
        group: 18,
        num: 0,
        name: "Route111_WinstrateFamilysHouse",
        layout: LayoutId("LAYOUT_HOUSE2"),
        music: MusicId(399), // MUS_RUSTBORO
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_111"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ROUTE111_OLD_LADYS_REST_STOP"),
        group: 18,
        num: 1,
        name: "Route111_OldLadysRestStop",
        layout: LayoutId("LAYOUT_HOUSE3"),
        music: MusicId(399), // MUS_RUSTBORO
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_111"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ROUTE112_CABLE_CAR_STATION"),
        group: 19,
        num: 0,
        name: "Route112_CableCarStation",
        layout: LayoutId("LAYOUT_CABLE_CAR_STATION"),
        music: MusicId(360), // MUS_ROUTE110
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_112"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MT_CHIMNEY_CABLE_CAR_STATION"),
        group: 19,
        num: 1,
        name: "MtChimney_CableCarStation",
        layout: LayoutId("LAYOUT_CABLE_CAR_STATION"),
        music: MusicId(360), // MUS_ROUTE110
        region_map_section: RegionMapSectionId("MAPSEC_MT_CHIMNEY"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ROUTE114_FOSSIL_MANIACS_HOUSE"),
        group: 20,
        num: 0,
        name: "Route114_FossilManiacsHouse",
        layout: LayoutId("LAYOUT_ROUTE114_FOSSIL_MANIACS_HOUSE"),
        music: MusicId(437), // MUS_FALLARBOR
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_114"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ROUTE114_FOSSIL_MANIACS_TUNNEL"),
        group: 20,
        num: 1,
        name: "Route114_FossilManiacsTunnel",
        layout: LayoutId("LAYOUT_ROUTE114_FOSSIL_MANIACS_TUNNEL"),
        music: MusicId(437), // MUS_FALLARBOR
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_114"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ROUTE114_LANETTES_HOUSE"),
        group: 20,
        num: 2,
        name: "Route114_LanettesHouse",
        layout: LayoutId("LAYOUT_ROUTE114_LANETTES_HOUSE"),
        music: MusicId(437), // MUS_FALLARBOR
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_114"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ROUTE116_TUNNELERS_REST_HOUSE"),
        group: 21,
        num: 0,
        name: "Route116_TunnelersRestHouse",
        layout: LayoutId("LAYOUT_ROUTE116_TUNNELERS_REST_HOUSE"),
        music: MusicId(399), // MUS_RUSTBORO
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_116"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ROUTE117_POKEMON_DAY_CARE"),
        group: 22,
        num: 0,
        name: "Route117_PokemonDayCare",
        layout: LayoutId("LAYOUT_ROUTE117_POKEMON_DAY_CARE"),
        music: MusicId(399), // MUS_RUSTBORO
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_117"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ROUTE121_SAFARI_ZONE_ENTRANCE"),
        group: 23,
        num: 0,
        name: "Route121_SafariZoneEntrance",
        layout: LayoutId("LAYOUT_ROUTE121_SAFARI_ZONE_ENTRANCE"),
        music: MusicId(382), // MUS_FORTREE
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_121"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_METEOR_FALLS_1F_1R"),
        group: 24,
        num: 0,
        name: "MeteorFalls_1F_1R",
        layout: LayoutId("LAYOUT_METEOR_FALLS_1F_1R"),
        music: MusicId(386), // MUS_CAVE_OF_ORIGIN
        region_map_section: RegionMapSectionId("MAPSEC_METEOR_FALLS"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_METEOR_FALLS_1F_2R"),
        group: 24,
        num: 1,
        name: "MeteorFalls_1F_2R",
        layout: LayoutId("LAYOUT_METEOR_FALLS_1F_2R"),
        music: MusicId(386), // MUS_CAVE_OF_ORIGIN
        region_map_section: RegionMapSectionId("MAPSEC_METEOR_FALLS"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_METEOR_FALLS_B1F_1R"),
        group: 24,
        num: 2,
        name: "MeteorFalls_B1F_1R",
        layout: LayoutId("LAYOUT_METEOR_FALLS_B1F_1R"),
        music: MusicId(386), // MUS_CAVE_OF_ORIGIN
        region_map_section: RegionMapSectionId("MAPSEC_METEOR_FALLS"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_METEOR_FALLS_B1F_2R"),
        group: 24,
        num: 3,
        name: "MeteorFalls_B1F_2R",
        layout: LayoutId("LAYOUT_METEOR_FALLS_B1F_2R"),
        music: MusicId(386), // MUS_CAVE_OF_ORIGIN
        region_map_section: RegionMapSectionId("MAPSEC_METEOR_FALLS"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_RUSTURF_TUNNEL"),
        group: 24,
        num: 4,
        name: "RusturfTunnel",
        layout: LayoutId("LAYOUT_RUSTURF_TUNNEL"),
        music: MusicId(366), // MUS_PETALBURG_WOODS
        region_map_section: RegionMapSectionId("MAPSEC_RUSTURF_TUNNEL"),
        requires_flash: false,
        weather: Weather::FogHorizontal, // WEATHER_FOG_HORIZONTAL
        map_type: MapType::Underground,  // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_UNDERWATER_SOOTOPOLIS_CITY"),
        group: 24,
        num: 5,
        name: "Underwater_SootopolisCity",
        layout: LayoutId("LAYOUT_UNDERWATER_SOOTOPOLIS_CITY"),
        music: MusicId(411), // MUS_UNDERWATER
        region_map_section: RegionMapSectionId("MAPSEC_UNDERWATER_SOOTOPOLIS"),
        requires_flash: false,
        weather: Weather::UnderwaterBubbles, // WEATHER_UNDERWATER_BUBBLES
        map_type: MapType::Underwater,       // MAP_TYPE_UNDERWATER
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_DESERT_RUINS"),
        group: 24,
        num: 6,
        name: "DesertRuins",
        layout: LayoutId("LAYOUT_DESERT_RUINS"),
        music: MusicId(438), // MUS_SEALED_CHAMBER
        region_map_section: RegionMapSectionId("MAPSEC_DESERT_RUINS"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_GRANITE_CAVE_1F"),
        group: 24,
        num: 7,
        name: "GraniteCave_1F",
        layout: LayoutId("LAYOUT_GRANITE_CAVE_1F"),
        music: MusicId(366), // MUS_PETALBURG_WOODS
        region_map_section: RegionMapSectionId("MAPSEC_GRANITE_CAVE"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_GRANITE_CAVE_B1F"),
        group: 24,
        num: 8,
        name: "GraniteCave_B1F",
        layout: LayoutId("LAYOUT_GRANITE_CAVE_B1F"),
        music: MusicId(366), // MUS_PETALBURG_WOODS
        region_map_section: RegionMapSectionId("MAPSEC_GRANITE_CAVE"),
        requires_flash: true,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_GRANITE_CAVE_B2F"),
        group: 24,
        num: 9,
        name: "GraniteCave_B2F",
        layout: LayoutId("LAYOUT_GRANITE_CAVE_B2F"),
        music: MusicId(366), // MUS_PETALBURG_WOODS
        region_map_section: RegionMapSectionId("MAPSEC_GRANITE_CAVE"),
        requires_flash: true,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_GRANITE_CAVE_STEVENS_ROOM"),
        group: 24,
        num: 10,
        name: "GraniteCave_StevensRoom",
        layout: LayoutId("LAYOUT_GRANITE_CAVE_STEVENS_ROOM"),
        music: MusicId(366), // MUS_PETALBURG_WOODS
        region_map_section: RegionMapSectionId("MAPSEC_GRANITE_CAVE"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_PETALBURG_WOODS"),
        group: 24,
        num: 11,
        name: "PetalburgWoods",
        layout: LayoutId("LAYOUT_PETALBURG_WOODS"),
        music: MusicId(366), // MUS_PETALBURG_WOODS
        region_map_section: RegionMapSectionId("MAPSEC_PETALBURG_WOODS"),
        requires_flash: false,
        weather: Weather::Shade,  // WEATHER_SHADE
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MT_CHIMNEY"),
        group: 24,
        num: 12,
        name: "MtChimney",
        layout: LayoutId("LAYOUT_MT_CHIMNEY"),
        music: MusicId(406), // MUS_MT_CHIMNEY
        region_map_section: RegionMapSectionId("MAPSEC_MT_CHIMNEY"),
        requires_flash: false,
        weather: Weather::VolcanicAsh, // WEATHER_VOLCANIC_ASH
        map_type: MapType::Route,      // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_JAGGED_PASS"),
        group: 24,
        num: 13,
        name: "JaggedPass",
        layout: LayoutId("LAYOUT_JAGGED_PASS"),
        music: MusicId(366), // MUS_PETALBURG_WOODS
        region_map_section: RegionMapSectionId("MAPSEC_JAGGED_PASS"),
        requires_flash: false,
        weather: Weather::None,   // WEATHER_NONE
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_FIERY_PATH"),
        group: 24,
        num: 14,
        name: "FieryPath",
        layout: LayoutId("LAYOUT_FIERY_PATH"),
        music: MusicId(366), // MUS_PETALBURG_WOODS
        region_map_section: RegionMapSectionId("MAPSEC_FIERY_PATH"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MT_PYRE_1F"),
        group: 24,
        num: 15,
        name: "MtPyre_1F",
        layout: LayoutId("LAYOUT_MT_PYRE_1F"),
        music: MusicId(432), // MUS_MT_PYRE
        region_map_section: RegionMapSectionId("MAPSEC_MT_PYRE"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MT_PYRE_2F"),
        group: 24,
        num: 16,
        name: "MtPyre_2F",
        layout: LayoutId("LAYOUT_MT_PYRE_2F"),
        music: MusicId(432), // MUS_MT_PYRE
        region_map_section: RegionMapSectionId("MAPSEC_MT_PYRE"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MT_PYRE_3F"),
        group: 24,
        num: 17,
        name: "MtPyre_3F",
        layout: LayoutId("LAYOUT_MT_PYRE_3F"),
        music: MusicId(432), // MUS_MT_PYRE
        region_map_section: RegionMapSectionId("MAPSEC_MT_PYRE"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MT_PYRE_4F"),
        group: 24,
        num: 18,
        name: "MtPyre_4F",
        layout: LayoutId("LAYOUT_MT_PYRE_4F"),
        music: MusicId(432), // MUS_MT_PYRE
        region_map_section: RegionMapSectionId("MAPSEC_MT_PYRE"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MT_PYRE_5F"),
        group: 24,
        num: 19,
        name: "MtPyre_5F",
        layout: LayoutId("LAYOUT_MT_PYRE_5F"),
        music: MusicId(432), // MUS_MT_PYRE
        region_map_section: RegionMapSectionId("MAPSEC_MT_PYRE"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MT_PYRE_6F"),
        group: 24,
        num: 20,
        name: "MtPyre_6F",
        layout: LayoutId("LAYOUT_MT_PYRE_6F"),
        music: MusicId(432), // MUS_MT_PYRE
        region_map_section: RegionMapSectionId("MAPSEC_MT_PYRE"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MT_PYRE_EXTERIOR"),
        group: 24,
        num: 21,
        name: "MtPyre_Exterior",
        layout: LayoutId("LAYOUT_MT_PYRE_EXTERIOR"),
        music: MusicId(434), // MUS_MT_PYRE_EXTERIOR
        region_map_section: RegionMapSectionId("MAPSEC_MT_PYRE"),
        requires_flash: false,
        weather: Weather::None,   // WEATHER_NONE
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MT_PYRE_SUMMIT"),
        group: 24,
        num: 22,
        name: "MtPyre_Summit",
        layout: LayoutId("LAYOUT_MT_PYRE_SUMMIT"),
        music: MusicId(434), // MUS_MT_PYRE_EXTERIOR
        region_map_section: RegionMapSectionId("MAPSEC_MT_PYRE"),
        requires_flash: false,
        weather: Weather::FogHorizontal, // WEATHER_FOG_HORIZONTAL
        map_type: MapType::Route,        // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_AQUA_HIDEOUT_1F"),
        group: 24,
        num: 23,
        name: "AquaHideout_1F",
        layout: LayoutId("LAYOUT_AQUA_HIDEOUT_1F"),
        music: MusicId(430), // MUS_AQUA_MAGMA_HIDEOUT
        region_map_section: RegionMapSectionId("MAPSEC_AQUA_HIDEOUT"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: true,
        battle_scene: BattleScene::Aqua, // MAP_BATTLE_SCENE_AQUA
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_AQUA_HIDEOUT_B1F"),
        group: 24,
        num: 24,
        name: "AquaHideout_B1F",
        layout: LayoutId("LAYOUT_AQUA_HIDEOUT_B1F"),
        music: MusicId(430), // MUS_AQUA_MAGMA_HIDEOUT
        region_map_section: RegionMapSectionId("MAPSEC_AQUA_HIDEOUT"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: true,
        battle_scene: BattleScene::Aqua, // MAP_BATTLE_SCENE_AQUA
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_AQUA_HIDEOUT_B2F"),
        group: 24,
        num: 25,
        name: "AquaHideout_B2F",
        layout: LayoutId("LAYOUT_AQUA_HIDEOUT_B2F"),
        music: MusicId(430), // MUS_AQUA_MAGMA_HIDEOUT
        region_map_section: RegionMapSectionId("MAPSEC_AQUA_HIDEOUT"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: true,
        battle_scene: BattleScene::Aqua, // MAP_BATTLE_SCENE_AQUA
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_UNDERWATER_SEAFLOOR_CAVERN"),
        group: 24,
        num: 26,
        name: "Underwater_SeafloorCavern",
        layout: LayoutId("LAYOUT_UNDERWATER_SEAFLOOR_CAVERN"),
        music: MusicId(411), // MUS_UNDERWATER
        region_map_section: RegionMapSectionId("MAPSEC_UNDERWATER_SEAFLOOR_CAVERN"),
        requires_flash: false,
        weather: Weather::UnderwaterBubbles, // WEATHER_UNDERWATER_BUBBLES
        map_type: MapType::Underwater,       // MAP_TYPE_UNDERWATER
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SEAFLOOR_CAVERN_ENTRANCE"),
        group: 24,
        num: 27,
        name: "SeafloorCavern_Entrance",
        layout: LayoutId("LAYOUT_SEAFLOOR_CAVERN_ENTRANCE"),
        music: MusicId(406), // MUS_MT_CHIMNEY
        region_map_section: RegionMapSectionId("MAPSEC_SEAFLOOR_CAVERN"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SEAFLOOR_CAVERN_ROOM1"),
        group: 24,
        num: 28,
        name: "SeafloorCavern_Room1",
        layout: LayoutId("LAYOUT_SEAFLOOR_CAVERN_ROOM1"),
        music: MusicId(406), // MUS_MT_CHIMNEY
        region_map_section: RegionMapSectionId("MAPSEC_SEAFLOOR_CAVERN"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SEAFLOOR_CAVERN_ROOM2"),
        group: 24,
        num: 29,
        name: "SeafloorCavern_Room2",
        layout: LayoutId("LAYOUT_SEAFLOOR_CAVERN_ROOM2"),
        music: MusicId(406), // MUS_MT_CHIMNEY
        region_map_section: RegionMapSectionId("MAPSEC_SEAFLOOR_CAVERN"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SEAFLOOR_CAVERN_ROOM3"),
        group: 24,
        num: 30,
        name: "SeafloorCavern_Room3",
        layout: LayoutId("LAYOUT_SEAFLOOR_CAVERN_ROOM3"),
        music: MusicId(406), // MUS_MT_CHIMNEY
        region_map_section: RegionMapSectionId("MAPSEC_SEAFLOOR_CAVERN"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SEAFLOOR_CAVERN_ROOM4"),
        group: 24,
        num: 31,
        name: "SeafloorCavern_Room4",
        layout: LayoutId("LAYOUT_SEAFLOOR_CAVERN_ROOM4"),
        music: MusicId(406), // MUS_MT_CHIMNEY
        region_map_section: RegionMapSectionId("MAPSEC_SEAFLOOR_CAVERN"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SEAFLOOR_CAVERN_ROOM5"),
        group: 24,
        num: 32,
        name: "SeafloorCavern_Room5",
        layout: LayoutId("LAYOUT_SEAFLOOR_CAVERN_ROOM5"),
        music: MusicId(406), // MUS_MT_CHIMNEY
        region_map_section: RegionMapSectionId("MAPSEC_SEAFLOOR_CAVERN"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SEAFLOOR_CAVERN_ROOM6"),
        group: 24,
        num: 33,
        name: "SeafloorCavern_Room6",
        layout: LayoutId("LAYOUT_SEAFLOOR_CAVERN_ROOM6"),
        music: MusicId(406), // MUS_MT_CHIMNEY
        region_map_section: RegionMapSectionId("MAPSEC_SEAFLOOR_CAVERN"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SEAFLOOR_CAVERN_ROOM7"),
        group: 24,
        num: 34,
        name: "SeafloorCavern_Room7",
        layout: LayoutId("LAYOUT_SEAFLOOR_CAVERN_ROOM7"),
        music: MusicId(406), // MUS_MT_CHIMNEY
        region_map_section: RegionMapSectionId("MAPSEC_SEAFLOOR_CAVERN"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SEAFLOOR_CAVERN_ROOM8"),
        group: 24,
        num: 35,
        name: "SeafloorCavern_Room8",
        layout: LayoutId("LAYOUT_SEAFLOOR_CAVERN_ROOM8"),
        music: MusicId(406), // MUS_MT_CHIMNEY
        region_map_section: RegionMapSectionId("MAPSEC_SEAFLOOR_CAVERN"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SEAFLOOR_CAVERN_ROOM9"),
        group: 24,
        num: 36,
        name: "SeafloorCavern_Room9",
        layout: LayoutId("LAYOUT_SEAFLOOR_CAVERN_ROOM9"),
        music: MusicId(406), // MUS_MT_CHIMNEY
        region_map_section: RegionMapSectionId("MAPSEC_SEAFLOOR_CAVERN"),
        requires_flash: false,
        weather: Weather::FogHorizontal, // WEATHER_FOG_HORIZONTAL
        map_type: MapType::Underground,  // MAP_TYPE_UNDERGROUND
        allow_bike: false,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_CAVE_OF_ORIGIN_ENTRANCE"),
        group: 24,
        num: 37,
        name: "CaveOfOrigin_Entrance",
        layout: LayoutId("LAYOUT_CAVE_OF_ORIGIN_ENTRANCE"),
        music: MusicId(386), // MUS_CAVE_OF_ORIGIN
        region_map_section: RegionMapSectionId("MAPSEC_CAVE_OF_ORIGIN"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_CAVE_OF_ORIGIN_1F"),
        group: 24,
        num: 38,
        name: "CaveOfOrigin_1F",
        layout: LayoutId("LAYOUT_CAVE_OF_ORIGIN_1F"),
        music: MusicId(386), // MUS_CAVE_OF_ORIGIN
        region_map_section: RegionMapSectionId("MAPSEC_CAVE_OF_ORIGIN"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_CAVE_OF_ORIGIN_UNUSED_RUBY_SAPPHIRE_MAP1"),
        group: 24,
        num: 39,
        name: "CaveOfOrigin_UnusedRubySapphireMap1",
        layout: LayoutId("LAYOUT_CAVE_OF_ORIGIN_UNUSED_RUBY_SAPPHIRE_MAP1"),
        music: MusicId(386), // MUS_CAVE_OF_ORIGIN
        region_map_section: RegionMapSectionId("MAPSEC_CAVE_OF_ORIGIN"),
        requires_flash: true,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_CAVE_OF_ORIGIN_UNUSED_RUBY_SAPPHIRE_MAP2"),
        group: 24,
        num: 40,
        name: "CaveOfOrigin_UnusedRubySapphireMap2",
        layout: LayoutId("LAYOUT_CAVE_OF_ORIGIN_UNUSED_RUBY_SAPPHIRE_MAP2"),
        music: MusicId(386), // MUS_CAVE_OF_ORIGIN
        region_map_section: RegionMapSectionId("MAPSEC_CAVE_OF_ORIGIN"),
        requires_flash: true,
        weather: Weather::FogHorizontal, // WEATHER_FOG_HORIZONTAL
        map_type: MapType::Underground,  // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_CAVE_OF_ORIGIN_UNUSED_RUBY_SAPPHIRE_MAP3"),
        group: 24,
        num: 41,
        name: "CaveOfOrigin_UnusedRubySapphireMap3",
        layout: LayoutId("LAYOUT_CAVE_OF_ORIGIN_UNUSED_RUBY_SAPPHIRE_MAP3"),
        music: MusicId(386), // MUS_CAVE_OF_ORIGIN
        region_map_section: RegionMapSectionId("MAPSEC_CAVE_OF_ORIGIN"),
        requires_flash: true,
        weather: Weather::FogHorizontal, // WEATHER_FOG_HORIZONTAL
        map_type: MapType::Underground,  // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_CAVE_OF_ORIGIN_B1F"),
        group: 24,
        num: 42,
        name: "CaveOfOrigin_B1F",
        layout: LayoutId("LAYOUT_CAVE_OF_ORIGIN_B1F"),
        music: MusicId(65535), // MUS_NONE
        region_map_section: RegionMapSectionId("MAPSEC_CAVE_OF_ORIGIN"),
        requires_flash: false,
        weather: Weather::FogHorizontal, // WEATHER_FOG_HORIZONTAL
        map_type: MapType::Underground,  // MAP_TYPE_UNDERGROUND
        allow_bike: false,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_VICTORY_ROAD_1F"),
        group: 24,
        num: 43,
        name: "VictoryRoad_1F",
        layout: LayoutId("LAYOUT_VICTORY_ROAD_1F"),
        music: MusicId(429), // MUS_VICTORY_ROAD
        region_map_section: RegionMapSectionId("MAPSEC_VICTORY_ROAD"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_VICTORY_ROAD_B1F"),
        group: 24,
        num: 44,
        name: "VictoryRoad_B1F",
        layout: LayoutId("LAYOUT_VICTORY_ROAD_B1F"),
        music: MusicId(429), // MUS_VICTORY_ROAD
        region_map_section: RegionMapSectionId("MAPSEC_VICTORY_ROAD"),
        requires_flash: true,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_VICTORY_ROAD_B2F"),
        group: 24,
        num: 45,
        name: "VictoryRoad_B2F",
        layout: LayoutId("LAYOUT_VICTORY_ROAD_B2F"),
        music: MusicId(429), // MUS_VICTORY_ROAD
        region_map_section: RegionMapSectionId("MAPSEC_VICTORY_ROAD"),
        requires_flash: true,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SHOAL_CAVE_LOW_TIDE_ENTRANCE_ROOM"),
        group: 24,
        num: 46,
        name: "ShoalCave_LowTideEntranceRoom",
        layout: LayoutId("LAYOUT_SHOAL_CAVE_LOW_TIDE_ENTRANCE_ROOM"),
        music: MusicId(432), // MUS_MT_PYRE
        region_map_section: RegionMapSectionId("MAPSEC_SHOAL_CAVE"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SHOAL_CAVE_LOW_TIDE_INNER_ROOM"),
        group: 24,
        num: 47,
        name: "ShoalCave_LowTideInnerRoom",
        layout: LayoutId("LAYOUT_SHOAL_CAVE_LOW_TIDE_INNER_ROOM"),
        music: MusicId(432), // MUS_MT_PYRE
        region_map_section: RegionMapSectionId("MAPSEC_SHOAL_CAVE"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SHOAL_CAVE_LOW_TIDE_STAIRS_ROOM"),
        group: 24,
        num: 48,
        name: "ShoalCave_LowTideStairsRoom",
        layout: LayoutId("LAYOUT_SHOAL_CAVE_LOW_TIDE_STAIRS_ROOM"),
        music: MusicId(432), // MUS_MT_PYRE
        region_map_section: RegionMapSectionId("MAPSEC_SHOAL_CAVE"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SHOAL_CAVE_LOW_TIDE_LOWER_ROOM"),
        group: 24,
        num: 49,
        name: "ShoalCave_LowTideLowerRoom",
        layout: LayoutId("LAYOUT_SHOAL_CAVE_LOW_TIDE_LOWER_ROOM"),
        music: MusicId(432), // MUS_MT_PYRE
        region_map_section: RegionMapSectionId("MAPSEC_SHOAL_CAVE"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SHOAL_CAVE_HIGH_TIDE_ENTRANCE_ROOM"),
        group: 24,
        num: 50,
        name: "ShoalCave_HighTideEntranceRoom",
        layout: LayoutId("LAYOUT_SHOAL_CAVE_HIGH_TIDE_ENTRANCE_ROOM"),
        music: MusicId(432), // MUS_MT_PYRE
        region_map_section: RegionMapSectionId("MAPSEC_SHOAL_CAVE"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SHOAL_CAVE_HIGH_TIDE_INNER_ROOM"),
        group: 24,
        num: 51,
        name: "ShoalCave_HighTideInnerRoom",
        layout: LayoutId("LAYOUT_SHOAL_CAVE_HIGH_TIDE_INNER_ROOM"),
        music: MusicId(432), // MUS_MT_PYRE
        region_map_section: RegionMapSectionId("MAPSEC_SHOAL_CAVE"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_NEW_MAUVILLE_ENTRANCE"),
        group: 24,
        num: 52,
        name: "NewMauville_Entrance",
        layout: LayoutId("LAYOUT_NEW_MAUVILLE_ENTRANCE"),
        music: MusicId(432), // MUS_MT_PYRE
        region_map_section: RegionMapSectionId("MAPSEC_NEW_MAUVILLE"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_NEW_MAUVILLE_INSIDE"),
        group: 24,
        num: 53,
        name: "NewMauville_Inside",
        layout: LayoutId("LAYOUT_NEW_MAUVILLE_INSIDE"),
        music: MusicId(432), // MUS_MT_PYRE
        region_map_section: RegionMapSectionId("MAPSEC_NEW_MAUVILLE"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ABANDONED_SHIP_DECK"),
        group: 24,
        num: 54,
        name: "AbandonedShip_Deck",
        layout: LayoutId("LAYOUT_ABANDONED_SHIP_DECK"),
        music: MusicId(381), // MUS_ABANDONED_SHIP
        region_map_section: RegionMapSectionId("MAPSEC_ABANDONED_SHIP"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ABANDONED_SHIP_CORRIDORS_1F"),
        group: 24,
        num: 55,
        name: "AbandonedShip_Corridors_1F",
        layout: LayoutId("LAYOUT_ABANDONED_SHIP_CORRIDORS_1F"),
        music: MusicId(381), // MUS_ABANDONED_SHIP
        region_map_section: RegionMapSectionId("MAPSEC_ABANDONED_SHIP"),
        requires_flash: false,
        weather: Weather::Shade,        // WEATHER_SHADE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ABANDONED_SHIP_ROOMS_1F"),
        group: 24,
        num: 56,
        name: "AbandonedShip_Rooms_1F",
        layout: LayoutId("LAYOUT_ABANDONED_SHIP_ROOMS_1F"),
        music: MusicId(381), // MUS_ABANDONED_SHIP
        region_map_section: RegionMapSectionId("MAPSEC_ABANDONED_SHIP"),
        requires_flash: false,
        weather: Weather::Shade,        // WEATHER_SHADE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ABANDONED_SHIP_CORRIDORS_B1F"),
        group: 24,
        num: 57,
        name: "AbandonedShip_Corridors_B1F",
        layout: LayoutId("LAYOUT_ABANDONED_SHIP_CORRIDORS_B1F"),
        music: MusicId(381), // MUS_ABANDONED_SHIP
        region_map_section: RegionMapSectionId("MAPSEC_ABANDONED_SHIP"),
        requires_flash: false,
        weather: Weather::Shade,        // WEATHER_SHADE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ABANDONED_SHIP_ROOMS_B1F"),
        group: 24,
        num: 58,
        name: "AbandonedShip_Rooms_B1F",
        layout: LayoutId("LAYOUT_ABANDONED_SHIP_ROOMS_B1F"),
        music: MusicId(381), // MUS_ABANDONED_SHIP
        region_map_section: RegionMapSectionId("MAPSEC_ABANDONED_SHIP"),
        requires_flash: false,
        weather: Weather::Shade,        // WEATHER_SHADE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ABANDONED_SHIP_ROOMS2_B1F"),
        group: 24,
        num: 59,
        name: "AbandonedShip_Rooms2_B1F",
        layout: LayoutId("LAYOUT_ABANDONED_SHIP_ROOMS2_B1F"),
        music: MusicId(381), // MUS_ABANDONED_SHIP
        region_map_section: RegionMapSectionId("MAPSEC_ABANDONED_SHIP"),
        requires_flash: false,
        weather: Weather::Shade,        // WEATHER_SHADE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ABANDONED_SHIP_UNDERWATER1"),
        group: 24,
        num: 60,
        name: "AbandonedShip_Underwater1",
        layout: LayoutId("LAYOUT_ABANDONED_SHIP_UNDERWATER1"),
        music: MusicId(411), // MUS_UNDERWATER
        region_map_section: RegionMapSectionId("MAPSEC_ABANDONED_SHIP"),
        requires_flash: false,
        weather: Weather::UnderwaterBubbles, // WEATHER_UNDERWATER_BUBBLES
        map_type: MapType::Underwater,       // MAP_TYPE_UNDERWATER
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ABANDONED_SHIP_ROOM_B1F"),
        group: 24,
        num: 61,
        name: "AbandonedShip_Room_B1F",
        layout: LayoutId("LAYOUT_ABANDONED_SHIP_ROOM_B1F"),
        music: MusicId(381), // MUS_ABANDONED_SHIP
        region_map_section: RegionMapSectionId("MAPSEC_ABANDONED_SHIP"),
        requires_flash: false,
        weather: Weather::Shade,        // WEATHER_SHADE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ABANDONED_SHIP_ROOMS2_1F"),
        group: 24,
        num: 62,
        name: "AbandonedShip_Rooms2_1F",
        layout: LayoutId("LAYOUT_ABANDONED_SHIP_ROOMS2_1F"),
        music: MusicId(381), // MUS_ABANDONED_SHIP
        region_map_section: RegionMapSectionId("MAPSEC_ABANDONED_SHIP"),
        requires_flash: false,
        weather: Weather::Shade,        // WEATHER_SHADE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ABANDONED_SHIP_CAPTAINS_OFFICE"),
        group: 24,
        num: 63,
        name: "AbandonedShip_CaptainsOffice",
        layout: LayoutId("LAYOUT_ABANDONED_SHIP_CAPTAINS_OFFICE"),
        music: MusicId(381), // MUS_ABANDONED_SHIP
        region_map_section: RegionMapSectionId("MAPSEC_ABANDONED_SHIP"),
        requires_flash: false,
        weather: Weather::Shade,        // WEATHER_SHADE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ABANDONED_SHIP_UNDERWATER2"),
        group: 24,
        num: 64,
        name: "AbandonedShip_Underwater2",
        layout: LayoutId("LAYOUT_ABANDONED_SHIP_UNDERWATER2"),
        music: MusicId(411), // MUS_UNDERWATER
        region_map_section: RegionMapSectionId("MAPSEC_ABANDONED_SHIP"),
        requires_flash: false,
        weather: Weather::UnderwaterBubbles, // WEATHER_UNDERWATER_BUBBLES
        map_type: MapType::Underwater,       // MAP_TYPE_UNDERWATER
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ABANDONED_SHIP_HIDDEN_FLOOR_CORRIDORS"),
        group: 24,
        num: 65,
        name: "AbandonedShip_HiddenFloorCorridors",
        layout: LayoutId("LAYOUT_ABANDONED_SHIP_HIDDEN_FLOOR_CORRIDORS"),
        music: MusicId(381), // MUS_ABANDONED_SHIP
        region_map_section: RegionMapSectionId("MAPSEC_ABANDONED_SHIP"),
        requires_flash: false,
        weather: Weather::Shade,        // WEATHER_SHADE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ABANDONED_SHIP_HIDDEN_FLOOR_ROOMS"),
        group: 24,
        num: 66,
        name: "AbandonedShip_HiddenFloorRooms",
        layout: LayoutId("LAYOUT_ABANDONED_SHIP_HIDDEN_FLOOR_ROOMS"),
        music: MusicId(381), // MUS_ABANDONED_SHIP
        region_map_section: RegionMapSectionId("MAPSEC_ABANDONED_SHIP"),
        requires_flash: false,
        weather: Weather::Shade,        // WEATHER_SHADE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ISLAND_CAVE"),
        group: 24,
        num: 67,
        name: "IslandCave",
        layout: LayoutId("LAYOUT_ISLAND_CAVE"),
        music: MusicId(438), // MUS_SEALED_CHAMBER
        region_map_section: RegionMapSectionId("MAPSEC_ISLAND_CAVE"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ANCIENT_TOMB"),
        group: 24,
        num: 68,
        name: "AncientTomb",
        layout: LayoutId("LAYOUT_ANCIENT_TOMB"),
        music: MusicId(438), // MUS_SEALED_CHAMBER
        region_map_section: RegionMapSectionId("MAPSEC_ANCIENT_TOMB"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_UNDERWATER_ROUTE134"),
        group: 24,
        num: 69,
        name: "Underwater_Route134",
        layout: LayoutId("LAYOUT_UNDERWATER_ROUTE134"),
        music: MusicId(411), // MUS_UNDERWATER
        region_map_section: RegionMapSectionId("MAPSEC_UNDERWATER_SEALED_CHAMBER"),
        requires_flash: false,
        weather: Weather::UnderwaterBubbles, // WEATHER_UNDERWATER_BUBBLES
        map_type: MapType::Underwater,       // MAP_TYPE_UNDERWATER
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_UNDERWATER_SEALED_CHAMBER"),
        group: 24,
        num: 70,
        name: "Underwater_SealedChamber",
        layout: LayoutId("LAYOUT_UNDERWATER_SEALED_CHAMBER"),
        music: MusicId(411), // MUS_UNDERWATER
        region_map_section: RegionMapSectionId("MAPSEC_UNDERWATER_SEALED_CHAMBER"),
        requires_flash: false,
        weather: Weather::UnderwaterBubbles, // WEATHER_UNDERWATER_BUBBLES
        map_type: MapType::Underwater,       // MAP_TYPE_UNDERWATER
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SEALED_CHAMBER_OUTER_ROOM"),
        group: 24,
        num: 71,
        name: "SealedChamber_OuterRoom",
        layout: LayoutId("LAYOUT_SEALED_CHAMBER_OUTER_ROOM"),
        music: MusicId(438), // MUS_SEALED_CHAMBER
        region_map_section: RegionMapSectionId("MAPSEC_SEALED_CHAMBER"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SEALED_CHAMBER_INNER_ROOM"),
        group: 24,
        num: 72,
        name: "SealedChamber_InnerRoom",
        layout: LayoutId("LAYOUT_SEALED_CHAMBER_INNER_ROOM"),
        music: MusicId(438), // MUS_SEALED_CHAMBER
        region_map_section: RegionMapSectionId("MAPSEC_SEALED_CHAMBER"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SCORCHED_SLAB"),
        group: 24,
        num: 73,
        name: "ScorchedSlab",
        layout: LayoutId("LAYOUT_SCORCHED_SLAB"),
        music: MusicId(366), // MUS_PETALBURG_WOODS
        region_map_section: RegionMapSectionId("MAPSEC_SCORCHED_SLAB"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_AQUA_HIDEOUT_UNUSED_RUBY_MAP1"),
        group: 24,
        num: 74,
        name: "AquaHideout_UnusedRubyMap1",
        layout: LayoutId("LAYOUT_AQUA_HIDEOUT_UNUSED_RUBY_MAP1"),
        music: MusicId(430), // MUS_AQUA_MAGMA_HIDEOUT
        region_map_section: RegionMapSectionId("MAPSEC_AQUA_HIDEOUT"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Magma, // MAP_BATTLE_SCENE_MAGMA
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_AQUA_HIDEOUT_UNUSED_RUBY_MAP2"),
        group: 24,
        num: 75,
        name: "AquaHideout_UnusedRubyMap2",
        layout: LayoutId("LAYOUT_AQUA_HIDEOUT_UNUSED_RUBY_MAP2"),
        music: MusicId(430), // MUS_AQUA_MAGMA_HIDEOUT
        region_map_section: RegionMapSectionId("MAPSEC_AQUA_HIDEOUT"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Magma, // MAP_BATTLE_SCENE_MAGMA
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_AQUA_HIDEOUT_UNUSED_RUBY_MAP3"),
        group: 24,
        num: 76,
        name: "AquaHideout_UnusedRubyMap3",
        layout: LayoutId("LAYOUT_AQUA_HIDEOUT_UNUSED_RUBY_MAP3"),
        music: MusicId(430), // MUS_AQUA_MAGMA_HIDEOUT
        region_map_section: RegionMapSectionId("MAPSEC_AQUA_HIDEOUT"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Magma, // MAP_BATTLE_SCENE_MAGMA
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SKY_PILLAR_ENTRANCE"),
        group: 24,
        num: 77,
        name: "SkyPillar_Entrance",
        layout: LayoutId("LAYOUT_SKY_PILLAR_ENTRANCE"),
        music: MusicId(406), // MUS_MT_CHIMNEY
        region_map_section: RegionMapSectionId("MAPSEC_SKY_PILLAR"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SKY_PILLAR_OUTSIDE"),
        group: 24,
        num: 78,
        name: "SkyPillar_Outside",
        layout: LayoutId("LAYOUT_SKY_PILLAR_OUTSIDE"),
        music: MusicId(406), // MUS_MT_CHIMNEY
        region_map_section: RegionMapSectionId("MAPSEC_SKY_PILLAR"),
        requires_flash: false,
        weather: Weather::None,   // WEATHER_NONE
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SKY_PILLAR_1F"),
        group: 24,
        num: 79,
        name: "SkyPillar_1F",
        layout: LayoutId("LAYOUT_SKY_PILLAR_1F"),
        music: MusicId(406), // MUS_MT_CHIMNEY
        region_map_section: RegionMapSectionId("MAPSEC_SKY_PILLAR"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SKY_PILLAR_2F"),
        group: 24,
        num: 80,
        name: "SkyPillar_2F",
        layout: LayoutId("LAYOUT_SKY_PILLAR_2F"),
        music: MusicId(406), // MUS_MT_CHIMNEY
        region_map_section: RegionMapSectionId("MAPSEC_SKY_PILLAR"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SKY_PILLAR_3F"),
        group: 24,
        num: 81,
        name: "SkyPillar_3F",
        layout: LayoutId("LAYOUT_SKY_PILLAR_3F"),
        music: MusicId(406), // MUS_MT_CHIMNEY
        region_map_section: RegionMapSectionId("MAPSEC_SKY_PILLAR"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SKY_PILLAR_4F"),
        group: 24,
        num: 82,
        name: "SkyPillar_4F",
        layout: LayoutId("LAYOUT_SKY_PILLAR_4F"),
        music: MusicId(406), // MUS_MT_CHIMNEY
        region_map_section: RegionMapSectionId("MAPSEC_SKY_PILLAR"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SHOAL_CAVE_LOW_TIDE_ICE_ROOM"),
        group: 24,
        num: 83,
        name: "ShoalCave_LowTideIceRoom",
        layout: LayoutId("LAYOUT_SHOAL_CAVE_LOW_TIDE_ICE_ROOM"),
        music: MusicId(432), // MUS_MT_PYRE
        region_map_section: RegionMapSectionId("MAPSEC_SHOAL_CAVE"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SKY_PILLAR_5F"),
        group: 24,
        num: 84,
        name: "SkyPillar_5F",
        layout: LayoutId("LAYOUT_SKY_PILLAR_5F"),
        music: MusicId(406), // MUS_MT_CHIMNEY
        region_map_section: RegionMapSectionId("MAPSEC_SKY_PILLAR"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SKY_PILLAR_TOP"),
        group: 24,
        num: 85,
        name: "SkyPillar_Top",
        layout: LayoutId("LAYOUT_SKY_PILLAR_TOP"),
        music: MusicId(406), // MUS_MT_CHIMNEY
        region_map_section: RegionMapSectionId("MAPSEC_SKY_PILLAR"),
        requires_flash: false,
        weather: Weather::None,   // WEATHER_NONE
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MAGMA_HIDEOUT_1F"),
        group: 24,
        num: 86,
        name: "MagmaHideout_1F",
        layout: LayoutId("LAYOUT_MAGMA_HIDEOUT_1F"),
        music: MusicId(430), // MUS_AQUA_MAGMA_HIDEOUT
        region_map_section: RegionMapSectionId("MAPSEC_MAGMA_HIDEOUT"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MAGMA_HIDEOUT_2F_1R"),
        group: 24,
        num: 87,
        name: "MagmaHideout_2F_1R",
        layout: LayoutId("LAYOUT_MAGMA_HIDEOUT_2F_1R"),
        music: MusicId(430), // MUS_AQUA_MAGMA_HIDEOUT
        region_map_section: RegionMapSectionId("MAPSEC_MAGMA_HIDEOUT"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MAGMA_HIDEOUT_2F_2R"),
        group: 24,
        num: 88,
        name: "MagmaHideout_2F_2R",
        layout: LayoutId("LAYOUT_MAGMA_HIDEOUT_2F_2R"),
        music: MusicId(430), // MUS_AQUA_MAGMA_HIDEOUT
        region_map_section: RegionMapSectionId("MAPSEC_MAGMA_HIDEOUT"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MAGMA_HIDEOUT_3F_1R"),
        group: 24,
        num: 89,
        name: "MagmaHideout_3F_1R",
        layout: LayoutId("LAYOUT_MAGMA_HIDEOUT_3F_1R"),
        music: MusicId(430), // MUS_AQUA_MAGMA_HIDEOUT
        region_map_section: RegionMapSectionId("MAPSEC_MAGMA_HIDEOUT"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MAGMA_HIDEOUT_3F_2R"),
        group: 24,
        num: 90,
        name: "MagmaHideout_3F_2R",
        layout: LayoutId("LAYOUT_MAGMA_HIDEOUT_3F_2R"),
        music: MusicId(430), // MUS_AQUA_MAGMA_HIDEOUT
        region_map_section: RegionMapSectionId("MAPSEC_MAGMA_HIDEOUT"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MAGMA_HIDEOUT_4F"),
        group: 24,
        num: 91,
        name: "MagmaHideout_4F",
        layout: LayoutId("LAYOUT_MAGMA_HIDEOUT_4F"),
        music: MusicId(430), // MUS_AQUA_MAGMA_HIDEOUT
        region_map_section: RegionMapSectionId("MAPSEC_MAGMA_HIDEOUT"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MAGMA_HIDEOUT_3F_3R"),
        group: 24,
        num: 92,
        name: "MagmaHideout_3F_3R",
        layout: LayoutId("LAYOUT_MAGMA_HIDEOUT_3F_3R"),
        music: MusicId(430), // MUS_AQUA_MAGMA_HIDEOUT
        region_map_section: RegionMapSectionId("MAPSEC_MAGMA_HIDEOUT"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MAGMA_HIDEOUT_2F_3R"),
        group: 24,
        num: 93,
        name: "MagmaHideout_2F_3R",
        layout: LayoutId("LAYOUT_MAGMA_HIDEOUT_2F_3R"),
        music: MusicId(430), // MUS_AQUA_MAGMA_HIDEOUT
        region_map_section: RegionMapSectionId("MAPSEC_MAGMA_HIDEOUT"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MIRAGE_TOWER_1F"),
        group: 24,
        num: 94,
        name: "MirageTower_1F",
        layout: LayoutId("LAYOUT_MIRAGE_TOWER_1F"),
        music: MusicId(406), // MUS_MT_CHIMNEY
        region_map_section: RegionMapSectionId("MAPSEC_MIRAGE_TOWER"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MIRAGE_TOWER_2F"),
        group: 24,
        num: 95,
        name: "MirageTower_2F",
        layout: LayoutId("LAYOUT_MIRAGE_TOWER_2F"),
        music: MusicId(406), // MUS_MT_CHIMNEY
        region_map_section: RegionMapSectionId("MAPSEC_MIRAGE_TOWER"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MIRAGE_TOWER_3F"),
        group: 24,
        num: 96,
        name: "MirageTower_3F",
        layout: LayoutId("LAYOUT_MIRAGE_TOWER_3F"),
        music: MusicId(406), // MUS_MT_CHIMNEY
        region_map_section: RegionMapSectionId("MAPSEC_MIRAGE_TOWER"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MIRAGE_TOWER_4F"),
        group: 24,
        num: 97,
        name: "MirageTower_4F",
        layout: LayoutId("LAYOUT_MIRAGE_TOWER_4F"),
        music: MusicId(406), // MUS_MT_CHIMNEY
        region_map_section: RegionMapSectionId("MAPSEC_MIRAGE_TOWER"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_DESERT_UNDERPASS"),
        group: 24,
        num: 98,
        name: "DesertUnderpass",
        layout: LayoutId("LAYOUT_DESERT_UNDERPASS"),
        music: MusicId(406), // MUS_MT_CHIMNEY
        region_map_section: RegionMapSectionId("MAPSEC_DESERT_UNDERPASS"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ARTISAN_CAVE_B1F"),
        group: 24,
        num: 99,
        name: "ArtisanCave_B1F",
        layout: LayoutId("LAYOUT_ARTISAN_CAVE_B1F"),
        music: MusicId(366), // MUS_PETALBURG_WOODS
        region_map_section: RegionMapSectionId("MAPSEC_ARTISAN_CAVE"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ARTISAN_CAVE_1F"),
        group: 24,
        num: 100,
        name: "ArtisanCave_1F",
        layout: LayoutId("LAYOUT_ARTISAN_CAVE_1F"),
        music: MusicId(366), // MUS_PETALBURG_WOODS
        region_map_section: RegionMapSectionId("MAPSEC_ARTISAN_CAVE"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_UNDERWATER_MARINE_CAVE"),
        group: 24,
        num: 101,
        name: "Underwater_MarineCave",
        layout: LayoutId("LAYOUT_UNDERWATER_MARINE_CAVE"),
        music: MusicId(366), // MUS_PETALBURG_WOODS
        region_map_section: RegionMapSectionId("MAPSEC_UNDERWATER_MARINE_CAVE"),
        requires_flash: false,
        weather: Weather::UnderwaterBubbles, // WEATHER_UNDERWATER_BUBBLES
        map_type: MapType::Underwater,       // MAP_TYPE_UNDERWATER
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MARINE_CAVE_ENTRANCE"),
        group: 24,
        num: 102,
        name: "MarineCave_Entrance",
        layout: LayoutId("LAYOUT_MARINE_CAVE_ENTRANCE"),
        music: MusicId(366), // MUS_PETALBURG_WOODS
        region_map_section: RegionMapSectionId("MAPSEC_MARINE_CAVE"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: false,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_MARINE_CAVE_END"),
        group: 24,
        num: 103,
        name: "MarineCave_End",
        layout: LayoutId("LAYOUT_MARINE_CAVE_END"),
        music: MusicId(366), // MUS_PETALBURG_WOODS
        region_map_section: RegionMapSectionId("MAPSEC_MARINE_CAVE"),
        requires_flash: false,
        weather: Weather::FogHorizontal, // WEATHER_FOG_HORIZONTAL
        map_type: MapType::Underground,  // MAP_TYPE_UNDERGROUND
        allow_bike: false,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_TERRA_CAVE_ENTRANCE"),
        group: 24,
        num: 104,
        name: "TerraCave_Entrance",
        layout: LayoutId("LAYOUT_TERRA_CAVE_ENTRANCE"),
        music: MusicId(366), // MUS_PETALBURG_WOODS
        region_map_section: RegionMapSectionId("MAPSEC_TERRA_CAVE"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_TERRA_CAVE_END"),
        group: 24,
        num: 105,
        name: "TerraCave_End",
        layout: LayoutId("LAYOUT_TERRA_CAVE_END"),
        music: MusicId(366), // MUS_PETALBURG_WOODS
        region_map_section: RegionMapSectionId("MAPSEC_TERRA_CAVE"),
        requires_flash: false,
        weather: Weather::FogHorizontal, // WEATHER_FOG_HORIZONTAL
        map_type: MapType::Underground,  // MAP_TYPE_UNDERGROUND
        allow_bike: false,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ALTERING_CAVE"),
        group: 24,
        num: 106,
        name: "AlteringCave",
        layout: LayoutId("LAYOUT_ALTERING_CAVE"),
        music: MusicId(543), // MUS_RG_SEVII_CAVE
        region_map_section: RegionMapSectionId("MAPSEC_ALTERING_CAVE"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_METEOR_FALLS_STEVENS_CAVE"),
        group: 24,
        num: 107,
        name: "MeteorFalls_StevensCave",
        layout: LayoutId("LAYOUT_METEOR_FALLS_STEVENS_CAVE"),
        music: MusicId(386), // MUS_CAVE_OF_ORIGIN
        region_map_section: RegionMapSectionId("MAPSEC_METEOR_FALLS"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SECRET_BASE_RED_CAVE1"),
        group: 25,
        num: 0,
        name: "SecretBase_RedCave1",
        layout: LayoutId("LAYOUT_SECRET_BASE_RED_CAVE1"),
        music: MusicId(382), // MUS_FORTREE
        region_map_section: RegionMapSectionId("MAPSEC_SECRET_BASE"),
        requires_flash: false,
        weather: Weather::None,        // WEATHER_NONE
        map_type: MapType::SecretBase, // MAP_TYPE_SECRET_BASE
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Frontier, // MAP_BATTLE_SCENE_FRONTIER
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SECRET_BASE_BROWN_CAVE1"),
        group: 25,
        num: 1,
        name: "SecretBase_BrownCave1",
        layout: LayoutId("LAYOUT_SECRET_BASE_BROWN_CAVE1"),
        music: MusicId(382), // MUS_FORTREE
        region_map_section: RegionMapSectionId("MAPSEC_SECRET_BASE"),
        requires_flash: false,
        weather: Weather::None,        // WEATHER_NONE
        map_type: MapType::SecretBase, // MAP_TYPE_SECRET_BASE
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Frontier, // MAP_BATTLE_SCENE_FRONTIER
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SECRET_BASE_BLUE_CAVE1"),
        group: 25,
        num: 2,
        name: "SecretBase_BlueCave1",
        layout: LayoutId("LAYOUT_SECRET_BASE_BLUE_CAVE1"),
        music: MusicId(382), // MUS_FORTREE
        region_map_section: RegionMapSectionId("MAPSEC_SECRET_BASE"),
        requires_flash: false,
        weather: Weather::None,        // WEATHER_NONE
        map_type: MapType::SecretBase, // MAP_TYPE_SECRET_BASE
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Frontier, // MAP_BATTLE_SCENE_FRONTIER
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SECRET_BASE_YELLOW_CAVE1"),
        group: 25,
        num: 3,
        name: "SecretBase_YellowCave1",
        layout: LayoutId("LAYOUT_SECRET_BASE_YELLOW_CAVE1"),
        music: MusicId(382), // MUS_FORTREE
        region_map_section: RegionMapSectionId("MAPSEC_SECRET_BASE"),
        requires_flash: false,
        weather: Weather::None,        // WEATHER_NONE
        map_type: MapType::SecretBase, // MAP_TYPE_SECRET_BASE
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Frontier, // MAP_BATTLE_SCENE_FRONTIER
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SECRET_BASE_TREE1"),
        group: 25,
        num: 4,
        name: "SecretBase_Tree1",
        layout: LayoutId("LAYOUT_SECRET_BASE_TREE1"),
        music: MusicId(382), // MUS_FORTREE
        region_map_section: RegionMapSectionId("MAPSEC_SECRET_BASE"),
        requires_flash: false,
        weather: Weather::None,        // WEATHER_NONE
        map_type: MapType::SecretBase, // MAP_TYPE_SECRET_BASE
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Frontier, // MAP_BATTLE_SCENE_FRONTIER
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SECRET_BASE_SHRUB1"),
        group: 25,
        num: 5,
        name: "SecretBase_Shrub1",
        layout: LayoutId("LAYOUT_SECRET_BASE_SHRUB1"),
        music: MusicId(382), // MUS_FORTREE
        region_map_section: RegionMapSectionId("MAPSEC_SECRET_BASE"),
        requires_flash: false,
        weather: Weather::None,        // WEATHER_NONE
        map_type: MapType::SecretBase, // MAP_TYPE_SECRET_BASE
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Frontier, // MAP_BATTLE_SCENE_FRONTIER
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SECRET_BASE_RED_CAVE2"),
        group: 25,
        num: 6,
        name: "SecretBase_RedCave2",
        layout: LayoutId("LAYOUT_SECRET_BASE_RED_CAVE2"),
        music: MusicId(382), // MUS_FORTREE
        region_map_section: RegionMapSectionId("MAPSEC_SECRET_BASE"),
        requires_flash: false,
        weather: Weather::None,        // WEATHER_NONE
        map_type: MapType::SecretBase, // MAP_TYPE_SECRET_BASE
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Frontier, // MAP_BATTLE_SCENE_FRONTIER
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SECRET_BASE_BROWN_CAVE2"),
        group: 25,
        num: 7,
        name: "SecretBase_BrownCave2",
        layout: LayoutId("LAYOUT_SECRET_BASE_BROWN_CAVE2"),
        music: MusicId(382), // MUS_FORTREE
        region_map_section: RegionMapSectionId("MAPSEC_SECRET_BASE"),
        requires_flash: false,
        weather: Weather::None,        // WEATHER_NONE
        map_type: MapType::SecretBase, // MAP_TYPE_SECRET_BASE
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Frontier, // MAP_BATTLE_SCENE_FRONTIER
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SECRET_BASE_BLUE_CAVE2"),
        group: 25,
        num: 8,
        name: "SecretBase_BlueCave2",
        layout: LayoutId("LAYOUT_SECRET_BASE_BLUE_CAVE2"),
        music: MusicId(382), // MUS_FORTREE
        region_map_section: RegionMapSectionId("MAPSEC_SECRET_BASE"),
        requires_flash: false,
        weather: Weather::None,        // WEATHER_NONE
        map_type: MapType::SecretBase, // MAP_TYPE_SECRET_BASE
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Frontier, // MAP_BATTLE_SCENE_FRONTIER
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SECRET_BASE_YELLOW_CAVE2"),
        group: 25,
        num: 9,
        name: "SecretBase_YellowCave2",
        layout: LayoutId("LAYOUT_SECRET_BASE_YELLOW_CAVE2"),
        music: MusicId(382), // MUS_FORTREE
        region_map_section: RegionMapSectionId("MAPSEC_SECRET_BASE"),
        requires_flash: false,
        weather: Weather::None,        // WEATHER_NONE
        map_type: MapType::SecretBase, // MAP_TYPE_SECRET_BASE
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Frontier, // MAP_BATTLE_SCENE_FRONTIER
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SECRET_BASE_TREE2"),
        group: 25,
        num: 10,
        name: "SecretBase_Tree2",
        layout: LayoutId("LAYOUT_SECRET_BASE_TREE2"),
        music: MusicId(382), // MUS_FORTREE
        region_map_section: RegionMapSectionId("MAPSEC_SECRET_BASE"),
        requires_flash: false,
        weather: Weather::None,        // WEATHER_NONE
        map_type: MapType::SecretBase, // MAP_TYPE_SECRET_BASE
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Frontier, // MAP_BATTLE_SCENE_FRONTIER
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SECRET_BASE_SHRUB2"),
        group: 25,
        num: 11,
        name: "SecretBase_Shrub2",
        layout: LayoutId("LAYOUT_SECRET_BASE_SHRUB2"),
        music: MusicId(382), // MUS_FORTREE
        region_map_section: RegionMapSectionId("MAPSEC_SECRET_BASE"),
        requires_flash: false,
        weather: Weather::None,        // WEATHER_NONE
        map_type: MapType::SecretBase, // MAP_TYPE_SECRET_BASE
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Frontier, // MAP_BATTLE_SCENE_FRONTIER
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SECRET_BASE_RED_CAVE3"),
        group: 25,
        num: 12,
        name: "SecretBase_RedCave3",
        layout: LayoutId("LAYOUT_SECRET_BASE_RED_CAVE3"),
        music: MusicId(382), // MUS_FORTREE
        region_map_section: RegionMapSectionId("MAPSEC_SECRET_BASE"),
        requires_flash: false,
        weather: Weather::None,        // WEATHER_NONE
        map_type: MapType::SecretBase, // MAP_TYPE_SECRET_BASE
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Frontier, // MAP_BATTLE_SCENE_FRONTIER
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SECRET_BASE_BROWN_CAVE3"),
        group: 25,
        num: 13,
        name: "SecretBase_BrownCave3",
        layout: LayoutId("LAYOUT_SECRET_BASE_BROWN_CAVE3"),
        music: MusicId(382), // MUS_FORTREE
        region_map_section: RegionMapSectionId("MAPSEC_SECRET_BASE"),
        requires_flash: false,
        weather: Weather::None,        // WEATHER_NONE
        map_type: MapType::SecretBase, // MAP_TYPE_SECRET_BASE
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Frontier, // MAP_BATTLE_SCENE_FRONTIER
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SECRET_BASE_BLUE_CAVE3"),
        group: 25,
        num: 14,
        name: "SecretBase_BlueCave3",
        layout: LayoutId("LAYOUT_SECRET_BASE_BLUE_CAVE3"),
        music: MusicId(382), // MUS_FORTREE
        region_map_section: RegionMapSectionId("MAPSEC_SECRET_BASE"),
        requires_flash: false,
        weather: Weather::None,        // WEATHER_NONE
        map_type: MapType::SecretBase, // MAP_TYPE_SECRET_BASE
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Frontier, // MAP_BATTLE_SCENE_FRONTIER
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SECRET_BASE_YELLOW_CAVE3"),
        group: 25,
        num: 15,
        name: "SecretBase_YellowCave3",
        layout: LayoutId("LAYOUT_SECRET_BASE_YELLOW_CAVE3"),
        music: MusicId(382), // MUS_FORTREE
        region_map_section: RegionMapSectionId("MAPSEC_SECRET_BASE"),
        requires_flash: false,
        weather: Weather::None,        // WEATHER_NONE
        map_type: MapType::SecretBase, // MAP_TYPE_SECRET_BASE
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Frontier, // MAP_BATTLE_SCENE_FRONTIER
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SECRET_BASE_TREE3"),
        group: 25,
        num: 16,
        name: "SecretBase_Tree3",
        layout: LayoutId("LAYOUT_SECRET_BASE_TREE3"),
        music: MusicId(382), // MUS_FORTREE
        region_map_section: RegionMapSectionId("MAPSEC_SECRET_BASE"),
        requires_flash: false,
        weather: Weather::None,        // WEATHER_NONE
        map_type: MapType::SecretBase, // MAP_TYPE_SECRET_BASE
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Frontier, // MAP_BATTLE_SCENE_FRONTIER
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SECRET_BASE_SHRUB3"),
        group: 25,
        num: 17,
        name: "SecretBase_Shrub3",
        layout: LayoutId("LAYOUT_SECRET_BASE_SHRUB3"),
        music: MusicId(382), // MUS_FORTREE
        region_map_section: RegionMapSectionId("MAPSEC_SECRET_BASE"),
        requires_flash: false,
        weather: Weather::None,        // WEATHER_NONE
        map_type: MapType::SecretBase, // MAP_TYPE_SECRET_BASE
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Frontier, // MAP_BATTLE_SCENE_FRONTIER
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SECRET_BASE_RED_CAVE4"),
        group: 25,
        num: 18,
        name: "SecretBase_RedCave4",
        layout: LayoutId("LAYOUT_SECRET_BASE_RED_CAVE4"),
        music: MusicId(382), // MUS_FORTREE
        region_map_section: RegionMapSectionId("MAPSEC_SECRET_BASE"),
        requires_flash: false,
        weather: Weather::None,        // WEATHER_NONE
        map_type: MapType::SecretBase, // MAP_TYPE_SECRET_BASE
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Frontier, // MAP_BATTLE_SCENE_FRONTIER
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SECRET_BASE_BROWN_CAVE4"),
        group: 25,
        num: 19,
        name: "SecretBase_BrownCave4",
        layout: LayoutId("LAYOUT_SECRET_BASE_BROWN_CAVE4"),
        music: MusicId(382), // MUS_FORTREE
        region_map_section: RegionMapSectionId("MAPSEC_SECRET_BASE"),
        requires_flash: false,
        weather: Weather::None,        // WEATHER_NONE
        map_type: MapType::SecretBase, // MAP_TYPE_SECRET_BASE
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Frontier, // MAP_BATTLE_SCENE_FRONTIER
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SECRET_BASE_BLUE_CAVE4"),
        group: 25,
        num: 20,
        name: "SecretBase_BlueCave4",
        layout: LayoutId("LAYOUT_SECRET_BASE_BLUE_CAVE4"),
        music: MusicId(382), // MUS_FORTREE
        region_map_section: RegionMapSectionId("MAPSEC_SECRET_BASE"),
        requires_flash: false,
        weather: Weather::None,        // WEATHER_NONE
        map_type: MapType::SecretBase, // MAP_TYPE_SECRET_BASE
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Frontier, // MAP_BATTLE_SCENE_FRONTIER
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SECRET_BASE_YELLOW_CAVE4"),
        group: 25,
        num: 21,
        name: "SecretBase_YellowCave4",
        layout: LayoutId("LAYOUT_SECRET_BASE_YELLOW_CAVE4"),
        music: MusicId(382), // MUS_FORTREE
        region_map_section: RegionMapSectionId("MAPSEC_SECRET_BASE"),
        requires_flash: false,
        weather: Weather::None,        // WEATHER_NONE
        map_type: MapType::SecretBase, // MAP_TYPE_SECRET_BASE
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Frontier, // MAP_BATTLE_SCENE_FRONTIER
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SECRET_BASE_TREE4"),
        group: 25,
        num: 22,
        name: "SecretBase_Tree4",
        layout: LayoutId("LAYOUT_SECRET_BASE_TREE4"),
        music: MusicId(382), // MUS_FORTREE
        region_map_section: RegionMapSectionId("MAPSEC_SECRET_BASE"),
        requires_flash: false,
        weather: Weather::None,        // WEATHER_NONE
        map_type: MapType::SecretBase, // MAP_TYPE_SECRET_BASE
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Frontier, // MAP_BATTLE_SCENE_FRONTIER
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SECRET_BASE_SHRUB4"),
        group: 25,
        num: 23,
        name: "SecretBase_Shrub4",
        layout: LayoutId("LAYOUT_SECRET_BASE_SHRUB4"),
        music: MusicId(382), // MUS_FORTREE
        region_map_section: RegionMapSectionId("MAPSEC_SECRET_BASE"),
        requires_flash: false,
        weather: Weather::None,        // WEATHER_NONE
        map_type: MapType::SecretBase, // MAP_TYPE_SECRET_BASE
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Frontier, // MAP_BATTLE_SCENE_FRONTIER
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_COLOSSEUM_2P"),
        group: 25,
        num: 24,
        name: "BattleColosseum_2P",
        layout: LayoutId("LAYOUT_BATTLE_COLOSSEUM_2P"),
        music: MusicId(422), // MUS_EVER_GRANDE
        region_map_section: RegionMapSectionId("MAPSEC_DYNAMIC"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Frontier, // MAP_BATTLE_SCENE_FRONTIER
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_TRADE_CENTER"),
        group: 25,
        num: 25,
        name: "TradeCenter",
        layout: LayoutId("LAYOUT_TRADE_CENTER"),
        music: MusicId(422), // MUS_EVER_GRANDE
        region_map_section: RegionMapSectionId("MAPSEC_DYNAMIC"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_RECORD_CORNER"),
        group: 25,
        num: 26,
        name: "RecordCorner",
        layout: LayoutId("LAYOUT_RECORD_CORNER"),
        music: MusicId(422), // MUS_EVER_GRANDE
        region_map_section: RegionMapSectionId("MAPSEC_DYNAMIC"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_COLOSSEUM_4P"),
        group: 25,
        num: 27,
        name: "BattleColosseum_4P",
        layout: LayoutId("LAYOUT_BATTLE_COLOSSEUM_4P"),
        music: MusicId(422), // MUS_EVER_GRANDE
        region_map_section: RegionMapSectionId("MAPSEC_DYNAMIC"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Frontier, // MAP_BATTLE_SCENE_FRONTIER
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_CONTEST_HALL"),
        group: 25,
        num: 28,
        name: "ContestHall",
        layout: LayoutId("LAYOUT_CONTEST_HALL"),
        music: MusicId(440), // MUS_CONTEST
        region_map_section: RegionMapSectionId("MAPSEC_DYNAMIC"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_UNUSED_CONTEST_HALL1"),
        group: 25,
        num: 29,
        name: "UnusedContestHall1",
        layout: LayoutId("LAYOUT_UNUSED_CONTEST_HALL1"),
        music: MusicId(357), // MUS_GSC_PEWTER
        region_map_section: RegionMapSectionId("MAPSEC_DYNAMIC"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_UNUSED_CONTEST_HALL2"),
        group: 25,
        num: 30,
        name: "UnusedContestHall2",
        layout: LayoutId("LAYOUT_UNUSED_CONTEST_HALL2"),
        music: MusicId(357), // MUS_GSC_PEWTER
        region_map_section: RegionMapSectionId("MAPSEC_DYNAMIC"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_UNUSED_CONTEST_HALL3"),
        group: 25,
        num: 31,
        name: "UnusedContestHall3",
        layout: LayoutId("LAYOUT_UNUSED_CONTEST_HALL3"),
        music: MusicId(357), // MUS_GSC_PEWTER
        region_map_section: RegionMapSectionId("MAPSEC_DYNAMIC"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_UNUSED_CONTEST_HALL4"),
        group: 25,
        num: 32,
        name: "UnusedContestHall4",
        layout: LayoutId("LAYOUT_UNUSED_CONTEST_HALL4"),
        music: MusicId(357), // MUS_GSC_PEWTER
        region_map_section: RegionMapSectionId("MAPSEC_DYNAMIC"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_UNUSED_CONTEST_HALL5"),
        group: 25,
        num: 33,
        name: "UnusedContestHall5",
        layout: LayoutId("LAYOUT_UNUSED_CONTEST_HALL5"),
        music: MusicId(357), // MUS_GSC_PEWTER
        region_map_section: RegionMapSectionId("MAPSEC_DYNAMIC"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_UNUSED_CONTEST_HALL6"),
        group: 25,
        num: 34,
        name: "UnusedContestHall6",
        layout: LayoutId("LAYOUT_UNUSED_CONTEST_HALL6"),
        music: MusicId(357), // MUS_GSC_PEWTER
        region_map_section: RegionMapSectionId("MAPSEC_DYNAMIC"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_CONTEST_HALL_BEAUTY"),
        group: 25,
        num: 35,
        name: "ContestHallBeauty",
        layout: LayoutId("LAYOUT_CONTEST_HALL_BEAUTY"),
        music: MusicId(440), // MUS_CONTEST
        region_map_section: RegionMapSectionId("MAPSEC_DYNAMIC"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_CONTEST_HALL_TOUGH"),
        group: 25,
        num: 36,
        name: "ContestHallTough",
        layout: LayoutId("LAYOUT_CONTEST_HALL_TOUGH"),
        music: MusicId(440), // MUS_CONTEST
        region_map_section: RegionMapSectionId("MAPSEC_DYNAMIC"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_CONTEST_HALL_COOL"),
        group: 25,
        num: 37,
        name: "ContestHallCool",
        layout: LayoutId("LAYOUT_CONTEST_HALL_COOL"),
        music: MusicId(440), // MUS_CONTEST
        region_map_section: RegionMapSectionId("MAPSEC_DYNAMIC"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_CONTEST_HALL_SMART"),
        group: 25,
        num: 38,
        name: "ContestHallSmart",
        layout: LayoutId("LAYOUT_CONTEST_HALL_SMART"),
        music: MusicId(440), // MUS_CONTEST
        region_map_section: RegionMapSectionId("MAPSEC_DYNAMIC"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_CONTEST_HALL_CUTE"),
        group: 25,
        num: 39,
        name: "ContestHallCute",
        layout: LayoutId("LAYOUT_CONTEST_HALL_CUTE"),
        music: MusicId(440), // MUS_CONTEST
        region_map_section: RegionMapSectionId("MAPSEC_DYNAMIC"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_INSIDE_OF_TRUCK"),
        group: 25,
        num: 40,
        name: "InsideOfTruck",
        layout: LayoutId("LAYOUT_INSIDE_OF_TRUCK"),
        music: MusicId(65535), // MUS_NONE
        region_map_section: RegionMapSectionId("MAPSEC_INSIDE_OF_TRUCK"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SS_TIDAL_CORRIDOR"),
        group: 25,
        num: 41,
        name: "SSTidalCorridor",
        layout: LayoutId("LAYOUT_SS_TIDAL_CORRIDOR"),
        music: MusicId(431), // MUS_SAILING
        region_map_section: RegionMapSectionId("MAPSEC_DYNAMIC"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SS_TIDAL_LOWER_DECK"),
        group: 25,
        num: 42,
        name: "SSTidalLowerDeck",
        layout: LayoutId("LAYOUT_SS_TIDAL_LOWER_DECK"),
        music: MusicId(431), // MUS_SAILING
        region_map_section: RegionMapSectionId("MAPSEC_DYNAMIC"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SS_TIDAL_ROOMS"),
        group: 25,
        num: 43,
        name: "SSTidalRooms",
        layout: LayoutId("LAYOUT_SS_TIDAL_ROOMS"),
        music: MusicId(431), // MUS_SAILING
        region_map_section: RegionMapSectionId("MAPSEC_DYNAMIC"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_PYRAMID_SQUARE01"),
        group: 25,
        num: 44,
        name: "BattlePyramidSquare01",
        layout: LayoutId("LAYOUT_BATTLE_PYRAMID_SQUARE01"),
        music: MusicId(65535), // MUS_NONE
        region_map_section: RegionMapSectionId("MAPSEC_DYNAMIC"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Gym, // MAP_BATTLE_SCENE_GYM
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_PYRAMID_SQUARE02"),
        group: 25,
        num: 45,
        name: "BattlePyramidSquare02",
        layout: LayoutId("LAYOUT_BATTLE_PYRAMID_SQUARE02"),
        music: MusicId(65535), // MUS_NONE
        region_map_section: RegionMapSectionId("MAPSEC_DYNAMIC"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Gym, // MAP_BATTLE_SCENE_GYM
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_PYRAMID_SQUARE03"),
        group: 25,
        num: 46,
        name: "BattlePyramidSquare03",
        layout: LayoutId("LAYOUT_BATTLE_PYRAMID_SQUARE03"),
        music: MusicId(65535), // MUS_NONE
        region_map_section: RegionMapSectionId("MAPSEC_DYNAMIC"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Gym, // MAP_BATTLE_SCENE_GYM
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_PYRAMID_SQUARE04"),
        group: 25,
        num: 47,
        name: "BattlePyramidSquare04",
        layout: LayoutId("LAYOUT_BATTLE_PYRAMID_SQUARE04"),
        music: MusicId(65535), // MUS_NONE
        region_map_section: RegionMapSectionId("MAPSEC_DYNAMIC"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Gym, // MAP_BATTLE_SCENE_GYM
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_PYRAMID_SQUARE05"),
        group: 25,
        num: 48,
        name: "BattlePyramidSquare05",
        layout: LayoutId("LAYOUT_BATTLE_PYRAMID_SQUARE05"),
        music: MusicId(65535), // MUS_NONE
        region_map_section: RegionMapSectionId("MAPSEC_DYNAMIC"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Gym, // MAP_BATTLE_SCENE_GYM
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_PYRAMID_SQUARE06"),
        group: 25,
        num: 49,
        name: "BattlePyramidSquare06",
        layout: LayoutId("LAYOUT_BATTLE_PYRAMID_SQUARE06"),
        music: MusicId(65535), // MUS_NONE
        region_map_section: RegionMapSectionId("MAPSEC_DYNAMIC"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Gym, // MAP_BATTLE_SCENE_GYM
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_PYRAMID_SQUARE07"),
        group: 25,
        num: 50,
        name: "BattlePyramidSquare07",
        layout: LayoutId("LAYOUT_BATTLE_PYRAMID_SQUARE07"),
        music: MusicId(65535), // MUS_NONE
        region_map_section: RegionMapSectionId("MAPSEC_DYNAMIC"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Gym, // MAP_BATTLE_SCENE_GYM
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_PYRAMID_SQUARE08"),
        group: 25,
        num: 51,
        name: "BattlePyramidSquare08",
        layout: LayoutId("LAYOUT_BATTLE_PYRAMID_SQUARE08"),
        music: MusicId(65535), // MUS_NONE
        region_map_section: RegionMapSectionId("MAPSEC_DYNAMIC"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Gym, // MAP_BATTLE_SCENE_GYM
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_PYRAMID_SQUARE09"),
        group: 25,
        num: 52,
        name: "BattlePyramidSquare09",
        layout: LayoutId("LAYOUT_BATTLE_PYRAMID_SQUARE09"),
        music: MusicId(65535), // MUS_NONE
        region_map_section: RegionMapSectionId("MAPSEC_DYNAMIC"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Gym, // MAP_BATTLE_SCENE_GYM
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_PYRAMID_SQUARE10"),
        group: 25,
        num: 53,
        name: "BattlePyramidSquare10",
        layout: LayoutId("LAYOUT_BATTLE_PYRAMID_SQUARE10"),
        music: MusicId(65535), // MUS_NONE
        region_map_section: RegionMapSectionId("MAPSEC_DYNAMIC"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Gym, // MAP_BATTLE_SCENE_GYM
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_PYRAMID_SQUARE11"),
        group: 25,
        num: 54,
        name: "BattlePyramidSquare11",
        layout: LayoutId("LAYOUT_BATTLE_PYRAMID_SQUARE11"),
        music: MusicId(65535), // MUS_NONE
        region_map_section: RegionMapSectionId("MAPSEC_DYNAMIC"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Gym, // MAP_BATTLE_SCENE_GYM
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_PYRAMID_SQUARE12"),
        group: 25,
        num: 55,
        name: "BattlePyramidSquare12",
        layout: LayoutId("LAYOUT_BATTLE_PYRAMID_SQUARE12"),
        music: MusicId(65535), // MUS_NONE
        region_map_section: RegionMapSectionId("MAPSEC_DYNAMIC"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Gym, // MAP_BATTLE_SCENE_GYM
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_PYRAMID_SQUARE13"),
        group: 25,
        num: 56,
        name: "BattlePyramidSquare13",
        layout: LayoutId("LAYOUT_BATTLE_PYRAMID_SQUARE13"),
        music: MusicId(65535), // MUS_NONE
        region_map_section: RegionMapSectionId("MAPSEC_DYNAMIC"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Gym, // MAP_BATTLE_SCENE_GYM
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_PYRAMID_SQUARE14"),
        group: 25,
        num: 57,
        name: "BattlePyramidSquare14",
        layout: LayoutId("LAYOUT_BATTLE_PYRAMID_SQUARE14"),
        music: MusicId(65535), // MUS_NONE
        region_map_section: RegionMapSectionId("MAPSEC_DYNAMIC"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Gym, // MAP_BATTLE_SCENE_GYM
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_PYRAMID_SQUARE15"),
        group: 25,
        num: 58,
        name: "BattlePyramidSquare15",
        layout: LayoutId("LAYOUT_BATTLE_PYRAMID_SQUARE15"),
        music: MusicId(65535), // MUS_NONE
        region_map_section: RegionMapSectionId("MAPSEC_DYNAMIC"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Gym, // MAP_BATTLE_SCENE_GYM
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_PYRAMID_SQUARE16"),
        group: 25,
        num: 59,
        name: "BattlePyramidSquare16",
        layout: LayoutId("LAYOUT_BATTLE_PYRAMID_SQUARE16"),
        music: MusicId(65535), // MUS_NONE
        region_map_section: RegionMapSectionId("MAPSEC_DYNAMIC"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Gym, // MAP_BATTLE_SCENE_GYM
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_UNION_ROOM"),
        group: 25,
        num: 60,
        name: "UnionRoom",
        layout: LayoutId("LAYOUT_UNION_ROOM"),
        music: MusicId(422), // MUS_EVER_GRANDE
        region_map_section: RegionMapSectionId("MAPSEC_DYNAMIC"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Frontier, // MAP_BATTLE_SCENE_FRONTIER
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SAFARI_ZONE_NORTHWEST"),
        group: 26,
        num: 0,
        name: "SafariZone_Northwest",
        layout: LayoutId("LAYOUT_SAFARI_ZONE_NORTHWEST"),
        music: MusicId(428), // MUS_SAFARI_ZONE
        region_map_section: RegionMapSectionId("MAPSEC_SAFARI_ZONE"),
        requires_flash: false,
        weather: Weather::None,   // WEATHER_NONE
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::East,
                offset: 0,
                target: MapId("MAP_SAFARI_ZONE_NORTH"),
            }, // right
            MapConnection {
                direction: Direction::South,
                offset: 0,
                target: MapId("MAP_SAFARI_ZONE_SOUTHWEST"),
            }, // down
        ],
    },
    MapHeader {
        id: MapId("MAP_SAFARI_ZONE_NORTH"),
        group: 26,
        num: 1,
        name: "SafariZone_North",
        layout: LayoutId("LAYOUT_SAFARI_ZONE_NORTH"),
        music: MusicId(428), // MUS_SAFARI_ZONE
        region_map_section: RegionMapSectionId("MAPSEC_SAFARI_ZONE"),
        requires_flash: false,
        weather: Weather::None,   // WEATHER_NONE
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::West,
                offset: 0,
                target: MapId("MAP_SAFARI_ZONE_NORTHWEST"),
            }, // left
            MapConnection {
                direction: Direction::South,
                offset: 0,
                target: MapId("MAP_SAFARI_ZONE_SOUTH"),
            }, // down
            MapConnection {
                direction: Direction::East,
                offset: 0,
                target: MapId("MAP_SAFARI_ZONE_NORTHEAST"),
            }, // right
        ],
    },
    MapHeader {
        id: MapId("MAP_SAFARI_ZONE_SOUTHWEST"),
        group: 26,
        num: 2,
        name: "SafariZone_Southwest",
        layout: LayoutId("LAYOUT_SAFARI_ZONE_SOUTHWEST"),
        music: MusicId(428), // MUS_SAFARI_ZONE
        region_map_section: RegionMapSectionId("MAPSEC_SAFARI_ZONE"),
        requires_flash: false,
        weather: Weather::None,   // WEATHER_NONE
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::North,
                offset: 0,
                target: MapId("MAP_SAFARI_ZONE_NORTHWEST"),
            }, // up
            MapConnection {
                direction: Direction::East,
                offset: 0,
                target: MapId("MAP_SAFARI_ZONE_SOUTH"),
            }, // right
        ],
    },
    MapHeader {
        id: MapId("MAP_SAFARI_ZONE_SOUTH"),
        group: 26,
        num: 3,
        name: "SafariZone_South",
        layout: LayoutId("LAYOUT_SAFARI_ZONE_SOUTH"),
        music: MusicId(428), // MUS_SAFARI_ZONE
        region_map_section: RegionMapSectionId("MAPSEC_SAFARI_ZONE"),
        requires_flash: false,
        weather: Weather::None,   // WEATHER_NONE
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::North,
                offset: 0,
                target: MapId("MAP_SAFARI_ZONE_NORTH"),
            }, // up
            MapConnection {
                direction: Direction::West,
                offset: 0,
                target: MapId("MAP_SAFARI_ZONE_SOUTHWEST"),
            }, // left
            MapConnection {
                direction: Direction::East,
                offset: 0,
                target: MapId("MAP_SAFARI_ZONE_SOUTHEAST"),
            }, // right
        ],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_OUTSIDE_WEST"),
        group: 26,
        num: 4,
        name: "BattleFrontier_OutsideWest",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_OUTSIDE_WEST"),
        music: MusicId(457), // MUS_B_FRONTIER
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,   // WEATHER_NONE
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::East,
                offset: 0,
                target: MapId("MAP_BATTLE_FRONTIER_OUTSIDE_EAST"),
            }, // right
        ],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_BATTLE_TOWER_LOBBY"),
        group: 26,
        num: 5,
        name: "BattleFrontier_BattleTowerLobby",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_TOWER_LOBBY"),
        music: MusicId(465), // MUS_B_TOWER
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_BATTLE_TOWER_ELEVATOR"),
        group: 26,
        num: 6,
        name: "BattleFrontier_BattleTowerElevator",
        layout: LayoutId("LAYOUT_BATTLE_ELEVATOR"),
        music: MusicId(465), // MUS_B_TOWER
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_BATTLE_TOWER_CORRIDOR"),
        group: 26,
        num: 7,
        name: "BattleFrontier_BattleTowerCorridor",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_TOWER_CORRIDOR"),
        music: MusicId(465), // MUS_B_TOWER
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_BATTLE_TOWER_BATTLE_ROOM"),
        group: 26,
        num: 8,
        name: "BattleFrontier_BattleTowerBattleRoom",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_TOWER_BATTLE_ROOM"),
        music: MusicId(465), // MUS_B_TOWER
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Frontier, // MAP_BATTLE_SCENE_FRONTIER
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SOUTHERN_ISLAND_EXTERIOR"),
        group: 26,
        num: 9,
        name: "SouthernIsland_Exterior",
        layout: LayoutId("LAYOUT_SOUTHERN_ISLAND_EXTERIOR"),
        music: MusicId(381), // MUS_ABANDONED_SHIP
        region_map_section: RegionMapSectionId("MAPSEC_SOUTHERN_ISLAND"),
        requires_flash: false,
        weather: Weather::None,   // WEATHER_NONE
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SOUTHERN_ISLAND_INTERIOR"),
        group: 26,
        num: 10,
        name: "SouthernIsland_Interior",
        layout: LayoutId("LAYOUT_SOUTHERN_ISLAND_INTERIOR"),
        music: MusicId(381), // MUS_ABANDONED_SHIP
        region_map_section: RegionMapSectionId("MAPSEC_SOUTHERN_ISLAND"),
        requires_flash: false,
        weather: Weather::Shade,  // WEATHER_SHADE
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SAFARI_ZONE_REST_HOUSE"),
        group: 26,
        num: 11,
        name: "SafariZone_RestHouse",
        layout: LayoutId("LAYOUT_SAFARI_ZONE_REST_HOUSE"),
        music: MusicId(428), // MUS_SAFARI_ZONE
        region_map_section: RegionMapSectionId("MAPSEC_SAFARI_ZONE"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_SAFARI_ZONE_NORTHEAST"),
        group: 26,
        num: 12,
        name: "SafariZone_Northeast",
        layout: LayoutId("LAYOUT_SAFARI_ZONE_NORTHEAST"),
        music: MusicId(428), // MUS_SAFARI_ZONE
        region_map_section: RegionMapSectionId("MAPSEC_SAFARI_ZONE"),
        requires_flash: false,
        weather: Weather::None,   // WEATHER_NONE
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::West,
                offset: 0,
                target: MapId("MAP_SAFARI_ZONE_NORTH"),
            }, // left
            MapConnection {
                direction: Direction::South,
                offset: 0,
                target: MapId("MAP_SAFARI_ZONE_SOUTHEAST"),
            }, // down
        ],
    },
    MapHeader {
        id: MapId("MAP_SAFARI_ZONE_SOUTHEAST"),
        group: 26,
        num: 13,
        name: "SafariZone_Southeast",
        layout: LayoutId("LAYOUT_SAFARI_ZONE_SOUTHEAST"),
        music: MusicId(428), // MUS_SAFARI_ZONE
        region_map_section: RegionMapSectionId("MAPSEC_SAFARI_ZONE"),
        requires_flash: false,
        weather: Weather::None,   // WEATHER_NONE
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::West,
                offset: 0,
                target: MapId("MAP_SAFARI_ZONE_SOUTH"),
            }, // left
            MapConnection {
                direction: Direction::North,
                offset: 0,
                target: MapId("MAP_SAFARI_ZONE_NORTHEAST"),
            }, // up
        ],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_OUTSIDE_EAST"),
        group: 26,
        num: 14,
        name: "BattleFrontier_OutsideEast",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_OUTSIDE_EAST"),
        music: MusicId(457), // MUS_B_FRONTIER
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,   // WEATHER_NONE
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[
            MapConnection {
                direction: Direction::West,
                offset: 0,
                target: MapId("MAP_BATTLE_FRONTIER_OUTSIDE_WEST"),
            }, // left
        ],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_BATTLE_TOWER_MULTI_PARTNER_ROOM"),
        group: 26,
        num: 15,
        name: "BattleFrontier_BattleTowerMultiPartnerRoom",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_TOWER_MULTI_PARTNER_ROOM"),
        music: MusicId(465), // MUS_B_TOWER
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_BATTLE_TOWER_MULTI_CORRIDOR"),
        group: 26,
        num: 16,
        name: "BattleFrontier_BattleTowerMultiCorridor",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_TOWER_MULTI_CORRIDOR"),
        music: MusicId(465), // MUS_B_TOWER
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_BATTLE_TOWER_MULTI_BATTLE_ROOM"),
        group: 26,
        num: 17,
        name: "BattleFrontier_BattleTowerMultiBattleRoom",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_TOWER_BATTLE_ROOM"),
        music: MusicId(465), // MUS_B_TOWER
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Frontier, // MAP_BATTLE_SCENE_FRONTIER
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_BATTLE_DOME_LOBBY"),
        group: 26,
        num: 18,
        name: "BattleFrontier_BattleDomeLobby",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_DOME_LOBBY"),
        music: MusicId(473), // MUS_B_DOME_LOBBY
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_BATTLE_DOME_CORRIDOR"),
        group: 26,
        num: 19,
        name: "BattleFrontier_BattleDomeCorridor",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_DOME_CORRIDOR"),
        music: MusicId(473), // MUS_B_DOME_LOBBY
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_BATTLE_DOME_PRE_BATTLE_ROOM"),
        group: 26,
        num: 20,
        name: "BattleFrontier_BattleDomePreBattleRoom",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_DOME_PRE_BATTLE_ROOM"),
        music: MusicId(467), // MUS_B_DOME
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_BATTLE_DOME_BATTLE_ROOM"),
        group: 26,
        num: 21,
        name: "BattleFrontier_BattleDomeBattleRoom",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_DOME_BATTLE_ROOM"),
        music: MusicId(467), // MUS_B_DOME
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_BATTLE_PALACE_LOBBY"),
        group: 26,
        num: 22,
        name: "BattleFrontier_BattlePalaceLobby",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_PALACE_LOBBY"),
        music: MusicId(463), // MUS_B_PALACE
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_BATTLE_PALACE_CORRIDOR"),
        group: 26,
        num: 23,
        name: "BattleFrontier_BattlePalaceCorridor",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_PALACE_CORRIDOR"),
        music: MusicId(463), // MUS_B_PALACE
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_BATTLE_PALACE_BATTLE_ROOM"),
        group: 26,
        num: 24,
        name: "BattleFrontier_BattlePalaceBattleRoom",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_PALACE_BATTLE_ROOM"),
        music: MusicId(463), // MUS_B_PALACE
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_BATTLE_PYRAMID_LOBBY"),
        group: 26,
        num: 25,
        name: "BattleFrontier_BattlePyramidLobby",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_PYRAMID_LOBBY"),
        music: MusicId(461), // MUS_B_PYRAMID
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_BATTLE_PYRAMID_FLOOR"),
        group: 26,
        num: 26,
        name: "BattleFrontier_BattlePyramidFloor",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_PYRAMID_FLOOR"),
        music: MusicId(65535), // MUS_NONE
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_BATTLE_PYRAMID_TOP"),
        group: 26,
        num: 27,
        name: "BattleFrontier_BattlePyramidTop",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_PYRAMID_TOP"),
        music: MusicId(65535), // MUS_NONE
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_BATTLE_ARENA_LOBBY"),
        group: 26,
        num: 28,
        name: "BattleFrontier_BattleArenaLobby",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_ARENA_LOBBY"),
        music: MusicId(458), // MUS_B_ARENA
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_BATTLE_ARENA_CORRIDOR"),
        group: 26,
        num: 29,
        name: "BattleFrontier_BattleArenaCorridor",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_ARENA_CORRIDOR"),
        music: MusicId(458), // MUS_B_ARENA
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_BATTLE_ARENA_BATTLE_ROOM"),
        group: 26,
        num: 30,
        name: "BattleFrontier_BattleArenaBattleRoom",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_ARENA_BATTLE_ROOM"),
        music: MusicId(458), // MUS_B_ARENA
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_BATTLE_FACTORY_LOBBY"),
        group: 26,
        num: 31,
        name: "BattleFrontier_BattleFactoryLobby",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_FACTORY_LOBBY"),
        music: MusicId(469), // MUS_B_FACTORY
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_BATTLE_FACTORY_PRE_BATTLE_ROOM"),
        group: 26,
        num: 32,
        name: "BattleFrontier_BattleFactoryPreBattleRoom",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_FACTORY_PRE_BATTLE_ROOM"),
        music: MusicId(469), // MUS_B_FACTORY
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_BATTLE_FACTORY_BATTLE_ROOM"),
        group: 26,
        num: 33,
        name: "BattleFrontier_BattleFactoryBattleRoom",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_FACTORY_BATTLE_ROOM"),
        music: MusicId(469), // MUS_B_FACTORY
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_BATTLE_PIKE_LOBBY"),
        group: 26,
        num: 34,
        name: "BattleFrontier_BattlePikeLobby",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_PIKE_LOBBY"),
        music: MusicId(468), // MUS_B_PIKE
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_BATTLE_PIKE_CORRIDOR"),
        group: 26,
        num: 35,
        name: "BattleFrontier_BattlePikeCorridor",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_PIKE_CORRIDOR"),
        music: MusicId(468), // MUS_B_PIKE
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_BATTLE_PIKE_THREE_PATH_ROOM"),
        group: 26,
        num: 36,
        name: "BattleFrontier_BattlePikeThreePathRoom",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_PIKE_THREE_PATH_ROOM"),
        music: MusicId(468), // MUS_B_PIKE
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_BATTLE_PIKE_ROOM_NORMAL"),
        group: 26,
        num: 37,
        name: "BattleFrontier_BattlePikeRoomNormal",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_PIKE_ROOM_NORMAL"),
        music: MusicId(468), // MUS_B_PIKE
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_BATTLE_PIKE_ROOM_FINAL"),
        group: 26,
        num: 38,
        name: "BattleFrontier_BattlePikeRoomFinal",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_PIKE_ROOM_FINAL"),
        music: MusicId(468), // MUS_B_PIKE
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_BATTLE_PIKE_ROOM_WILD_MONS"),
        group: 26,
        num: 39,
        name: "BattleFrontier_BattlePikeRoomWildMons",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_BATTLE_PIKE_ROOM_WILD_MONS"),
        music: MusicId(468), // MUS_B_PIKE
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_RANKING_HALL"),
        group: 26,
        num: 40,
        name: "BattleFrontier_RankingHall",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_RANKING_HALL"),
        music: MusicId(373), // MUS_LILYCOVE_MUSEUM
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_LOUNGE1"),
        group: 26,
        num: 41,
        name: "BattleFrontier_Lounge1",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_LOUNGE2"),
        music: MusicId(384), // MUS_B_TOWER_RS
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_EXCHANGE_SERVICE_CORNER"),
        group: 26,
        num: 42,
        name: "BattleFrontier_ExchangeServiceCorner",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_EXCHANGE_SERVICE_CORNER"),
        music: MusicId(384), // MUS_B_TOWER_RS
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_LOUNGE2"),
        group: 26,
        num: 43,
        name: "BattleFrontier_Lounge2",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_LOUNGE1"),
        music: MusicId(384), // MUS_B_TOWER_RS
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_LOUNGE3"),
        group: 26,
        num: 44,
        name: "BattleFrontier_Lounge3",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_LOUNGE2"),
        music: MusicId(384), // MUS_B_TOWER_RS
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_LOUNGE4"),
        group: 26,
        num: 45,
        name: "BattleFrontier_Lounge4",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_LOUNGE2"),
        music: MusicId(384), // MUS_B_TOWER_RS
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_SCOTTS_HOUSE"),
        group: 26,
        num: 46,
        name: "BattleFrontier_ScottsHouse",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_SCOTTS_HOUSE"),
        music: MusicId(384), // MUS_B_TOWER_RS
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_LOUNGE5"),
        group: 26,
        num: 47,
        name: "BattleFrontier_Lounge5",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_LOUNGE1"),
        music: MusicId(384), // MUS_B_TOWER_RS
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_LOUNGE6"),
        group: 26,
        num: 48,
        name: "BattleFrontier_Lounge6",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_LOUNGE2"),
        music: MusicId(384), // MUS_B_TOWER_RS
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_LOUNGE7"),
        group: 26,
        num: 49,
        name: "BattleFrontier_Lounge7",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_LOUNGE2"),
        music: MusicId(384), // MUS_B_TOWER_RS
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_RECEPTION_GATE"),
        group: 26,
        num: 50,
        name: "BattleFrontier_ReceptionGate",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_RECEPTION_GATE"),
        music: MusicId(384), // MUS_B_TOWER_RS
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_LOUNGE8"),
        group: 26,
        num: 51,
        name: "BattleFrontier_Lounge8",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_LOUNGE2"),
        music: MusicId(384), // MUS_B_TOWER_RS
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_LOUNGE9"),
        group: 26,
        num: 52,
        name: "BattleFrontier_Lounge9",
        layout: LayoutId("LAYOUT_BATTLE_FRONTIER_LOUNGE2"),
        music: MusicId(384), // MUS_B_TOWER_RS
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_POKEMON_CENTER_1F"),
        group: 26,
        num: 53,
        name: "BattleFrontier_PokemonCenter_1F",
        layout: LayoutId("LAYOUT_POKEMON_CENTER_1F"),
        music: MusicId(400), // MUS_POKE_CENTER
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_POKEMON_CENTER_2F"),
        group: 26,
        num: 54,
        name: "BattleFrontier_PokemonCenter_2F",
        layout: LayoutId("LAYOUT_POKEMON_CENTER_2F"),
        music: MusicId(400), // MUS_POKE_CENTER
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BATTLE_FRONTIER_MART"),
        group: 26,
        num: 55,
        name: "BattleFrontier_Mart",
        layout: LayoutId("LAYOUT_MART"),
        music: MusicId(404), // MUS_POKE_MART
        region_map_section: RegionMapSectionId("MAPSEC_BATTLE_FRONTIER"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_FARAWAY_ISLAND_ENTRANCE"),
        group: 26,
        num: 56,
        name: "FarawayIsland_Entrance",
        layout: LayoutId("LAYOUT_FARAWAY_ISLAND_ENTRANCE"),
        music: MusicId(381), // MUS_ABANDONED_SHIP
        region_map_section: RegionMapSectionId("MAPSEC_FARAWAY_ISLAND"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_FARAWAY_ISLAND_INTERIOR"),
        group: 26,
        num: 57,
        name: "FarawayIsland_Interior",
        layout: LayoutId("LAYOUT_FARAWAY_ISLAND_INTERIOR"),
        music: MusicId(381), // MUS_ABANDONED_SHIP
        region_map_section: RegionMapSectionId("MAPSEC_FARAWAY_ISLAND"),
        requires_flash: false,
        weather: Weather::Shade,   // WEATHER_SHADE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BIRTH_ISLAND_EXTERIOR"),
        group: 26,
        num: 58,
        name: "BirthIsland_Exterior",
        layout: LayoutId("LAYOUT_BIRTH_ISLAND_EXTERIOR"),
        music: MusicId(65535), // MUS_NONE
        region_map_section: RegionMapSectionId("MAPSEC_BIRTH_ISLAND"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_BIRTH_ISLAND_HARBOR"),
        group: 26,
        num: 59,
        name: "BirthIsland_Harbor",
        layout: LayoutId("LAYOUT_ISLAND_HARBOR"),
        music: MusicId(65535), // MUS_NONE
        region_map_section: RegionMapSectionId("MAPSEC_BIRTH_ISLAND"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_TRAINER_HILL_ENTRANCE"),
        group: 26,
        num: 60,
        name: "TrainerHill_Entrance",
        layout: LayoutId("LAYOUT_TRAINER_HILL_ENTRANCE"),
        music: MusicId(384), // MUS_B_TOWER_RS
        region_map_section: RegionMapSectionId("MAPSEC_TRAINER_HILL"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_TRAINER_HILL_1F"),
        group: 26,
        num: 61,
        name: "TrainerHill_1F",
        layout: LayoutId("LAYOUT_TRAINER_HILL_1F"),
        music: MusicId(384), // MUS_B_TOWER_RS
        region_map_section: RegionMapSectionId("MAPSEC_TRAINER_HILL"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_TRAINER_HILL_2F"),
        group: 26,
        num: 62,
        name: "TrainerHill_2F",
        layout: LayoutId("LAYOUT_TRAINER_HILL_2F"),
        music: MusicId(384), // MUS_B_TOWER_RS
        region_map_section: RegionMapSectionId("MAPSEC_TRAINER_HILL"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_TRAINER_HILL_3F"),
        group: 26,
        num: 63,
        name: "TrainerHill_3F",
        layout: LayoutId("LAYOUT_TRAINER_HILL_3F"),
        music: MusicId(384), // MUS_B_TOWER_RS
        region_map_section: RegionMapSectionId("MAPSEC_TRAINER_HILL"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_TRAINER_HILL_4F"),
        group: 26,
        num: 64,
        name: "TrainerHill_4F",
        layout: LayoutId("LAYOUT_TRAINER_HILL_4F"),
        music: MusicId(384), // MUS_B_TOWER_RS
        region_map_section: RegionMapSectionId("MAPSEC_TRAINER_HILL"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_TRAINER_HILL_ROOF"),
        group: 26,
        num: 65,
        name: "TrainerHill_Roof",
        layout: LayoutId("LAYOUT_TRAINER_HILL_ROOF"),
        music: MusicId(384), // MUS_B_TOWER_RS
        region_map_section: RegionMapSectionId("MAPSEC_TRAINER_HILL"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_NAVEL_ROCK_EXTERIOR"),
        group: 26,
        num: 66,
        name: "NavelRock_Exterior",
        layout: LayoutId("LAYOUT_NAVEL_ROCK_EXTERIOR"),
        music: MusicId(545), // MUS_RG_SEVII_ROUTE
        region_map_section: RegionMapSectionId("MAPSEC_NAVEL_ROCK"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: true,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_NAVEL_ROCK_HARBOR"),
        group: 26,
        num: 67,
        name: "NavelRock_Harbor",
        layout: LayoutId("LAYOUT_ISLAND_HARBOR"),
        music: MusicId(545), // MUS_RG_SEVII_ROUTE
        region_map_section: RegionMapSectionId("MAPSEC_NAVEL_ROCK"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_NAVEL_ROCK_ENTRANCE"),
        group: 26,
        num: 68,
        name: "NavelRock_Entrance",
        layout: LayoutId("LAYOUT_NAVEL_ROCK_ENTRANCE"),
        music: MusicId(543), // MUS_RG_SEVII_CAVE
        region_map_section: RegionMapSectionId("MAPSEC_NAVEL_ROCK"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_NAVEL_ROCK_B1F"),
        group: 26,
        num: 69,
        name: "NavelRock_B1F",
        layout: LayoutId("LAYOUT_NAVEL_ROCK_B1F"),
        music: MusicId(543), // MUS_RG_SEVII_CAVE
        region_map_section: RegionMapSectionId("MAPSEC_NAVEL_ROCK"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_NAVEL_ROCK_FORK"),
        group: 26,
        num: 70,
        name: "NavelRock_Fork",
        layout: LayoutId("LAYOUT_NAVEL_ROCK_FORK"),
        music: MusicId(543), // MUS_RG_SEVII_CAVE
        region_map_section: RegionMapSectionId("MAPSEC_NAVEL_ROCK"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_NAVEL_ROCK_UP1"),
        group: 26,
        num: 71,
        name: "NavelRock_Up1",
        layout: LayoutId("LAYOUT_NAVEL_ROCK_LADDER_ROOM1"),
        music: MusicId(543), // MUS_RG_SEVII_CAVE
        region_map_section: RegionMapSectionId("MAPSEC_NAVEL_ROCK"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_NAVEL_ROCK_UP2"),
        group: 26,
        num: 72,
        name: "NavelRock_Up2",
        layout: LayoutId("LAYOUT_NAVEL_ROCK_LADDER_ROOM2"),
        music: MusicId(543), // MUS_RG_SEVII_CAVE
        region_map_section: RegionMapSectionId("MAPSEC_NAVEL_ROCK"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_NAVEL_ROCK_UP3"),
        group: 26,
        num: 73,
        name: "NavelRock_Up3",
        layout: LayoutId("LAYOUT_NAVEL_ROCK_LADDER_ROOM1"),
        music: MusicId(543), // MUS_RG_SEVII_CAVE
        region_map_section: RegionMapSectionId("MAPSEC_NAVEL_ROCK"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_NAVEL_ROCK_UP4"),
        group: 26,
        num: 74,
        name: "NavelRock_Up4",
        layout: LayoutId("LAYOUT_NAVEL_ROCK_LADDER_ROOM2"),
        music: MusicId(543), // MUS_RG_SEVII_CAVE
        region_map_section: RegionMapSectionId("MAPSEC_NAVEL_ROCK"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_NAVEL_ROCK_TOP"),
        group: 26,
        num: 75,
        name: "NavelRock_Top",
        layout: LayoutId("LAYOUT_NAVEL_ROCK_TOP"),
        music: MusicId(543), // MUS_RG_SEVII_CAVE
        region_map_section: RegionMapSectionId("MAPSEC_NAVEL_ROCK"),
        requires_flash: false,
        weather: Weather::Shade,        // WEATHER_SHADE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_NAVEL_ROCK_DOWN01"),
        group: 26,
        num: 76,
        name: "NavelRock_Down01",
        layout: LayoutId("LAYOUT_NAVEL_ROCK_LADDER_ROOM1"),
        music: MusicId(543), // MUS_RG_SEVII_CAVE
        region_map_section: RegionMapSectionId("MAPSEC_NAVEL_ROCK"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_NAVEL_ROCK_DOWN02"),
        group: 26,
        num: 77,
        name: "NavelRock_Down02",
        layout: LayoutId("LAYOUT_NAVEL_ROCK_LADDER_ROOM2"),
        music: MusicId(543), // MUS_RG_SEVII_CAVE
        region_map_section: RegionMapSectionId("MAPSEC_NAVEL_ROCK"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_NAVEL_ROCK_DOWN03"),
        group: 26,
        num: 78,
        name: "NavelRock_Down03",
        layout: LayoutId("LAYOUT_NAVEL_ROCK_LADDER_ROOM1"),
        music: MusicId(543), // MUS_RG_SEVII_CAVE
        region_map_section: RegionMapSectionId("MAPSEC_NAVEL_ROCK"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_NAVEL_ROCK_DOWN04"),
        group: 26,
        num: 79,
        name: "NavelRock_Down04",
        layout: LayoutId("LAYOUT_NAVEL_ROCK_LADDER_ROOM2"),
        music: MusicId(543), // MUS_RG_SEVII_CAVE
        region_map_section: RegionMapSectionId("MAPSEC_NAVEL_ROCK"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_NAVEL_ROCK_DOWN05"),
        group: 26,
        num: 80,
        name: "NavelRock_Down05",
        layout: LayoutId("LAYOUT_NAVEL_ROCK_LADDER_ROOM1"),
        music: MusicId(543), // MUS_RG_SEVII_CAVE
        region_map_section: RegionMapSectionId("MAPSEC_NAVEL_ROCK"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_NAVEL_ROCK_DOWN06"),
        group: 26,
        num: 81,
        name: "NavelRock_Down06",
        layout: LayoutId("LAYOUT_NAVEL_ROCK_LADDER_ROOM2"),
        music: MusicId(543), // MUS_RG_SEVII_CAVE
        region_map_section: RegionMapSectionId("MAPSEC_NAVEL_ROCK"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_NAVEL_ROCK_DOWN07"),
        group: 26,
        num: 82,
        name: "NavelRock_Down07",
        layout: LayoutId("LAYOUT_NAVEL_ROCK_LADDER_ROOM1"),
        music: MusicId(543), // MUS_RG_SEVII_CAVE
        region_map_section: RegionMapSectionId("MAPSEC_NAVEL_ROCK"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_NAVEL_ROCK_DOWN08"),
        group: 26,
        num: 83,
        name: "NavelRock_Down08",
        layout: LayoutId("LAYOUT_NAVEL_ROCK_LADDER_ROOM2"),
        music: MusicId(543), // MUS_RG_SEVII_CAVE
        region_map_section: RegionMapSectionId("MAPSEC_NAVEL_ROCK"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_NAVEL_ROCK_DOWN09"),
        group: 26,
        num: 84,
        name: "NavelRock_Down09",
        layout: LayoutId("LAYOUT_NAVEL_ROCK_LADDER_ROOM1"),
        music: MusicId(543), // MUS_RG_SEVII_CAVE
        region_map_section: RegionMapSectionId("MAPSEC_NAVEL_ROCK"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_NAVEL_ROCK_DOWN10"),
        group: 26,
        num: 85,
        name: "NavelRock_Down10",
        layout: LayoutId("LAYOUT_NAVEL_ROCK_LADDER_ROOM2"),
        music: MusicId(543), // MUS_RG_SEVII_CAVE
        region_map_section: RegionMapSectionId("MAPSEC_NAVEL_ROCK"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_NAVEL_ROCK_DOWN11"),
        group: 26,
        num: 86,
        name: "NavelRock_Down11",
        layout: LayoutId("LAYOUT_NAVEL_ROCK_LADDER_ROOM1"),
        music: MusicId(543), // MUS_RG_SEVII_CAVE
        region_map_section: RegionMapSectionId("MAPSEC_NAVEL_ROCK"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_NAVEL_ROCK_BOTTOM"),
        group: 26,
        num: 87,
        name: "NavelRock_Bottom",
        layout: LayoutId("LAYOUT_NAVEL_ROCK_BOTTOM"),
        music: MusicId(543), // MUS_RG_SEVII_CAVE
        region_map_section: RegionMapSectionId("MAPSEC_NAVEL_ROCK"),
        requires_flash: false,
        weather: Weather::None,         // WEATHER_NONE
        map_type: MapType::Underground, // MAP_TYPE_UNDERGROUND
        allow_bike: false,
        allow_escape: false,
        allow_run: true,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_TRAINER_HILL_ELEVATOR"),
        group: 26,
        num: 88,
        name: "TrainerHill_Elevator",
        layout: LayoutId("LAYOUT_BATTLE_ELEVATOR"),
        music: MusicId(384), // MUS_B_TOWER_RS
        region_map_section: RegionMapSectionId("MAPSEC_TRAINER_HILL"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ROUTE104_PROTOTYPE"),
        group: 27,
        num: 0,
        name: "Route104_Prototype",
        layout: LayoutId("LAYOUT_ROUTE104_PROTOTYPE"),
        music: MusicId(401), // MUS_ROUTE104
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_104"),
        requires_flash: false,
        weather: Weather::Sunny,  // WEATHER_SUNNY
        map_type: MapType::Route, // MAP_TYPE_ROUTE
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ROUTE104_PROTOTYPE_PRETTY_PETAL_FLOWER_SHOP"),
        group: 27,
        num: 1,
        name: "Route104_PrototypePrettyPetalFlowerShop",
        layout: LayoutId("LAYOUT_ROUTE104_PRETTY_PETAL_FLOWER_SHOP"),
        music: MusicId(401), // MUS_ROUTE104
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_104"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: true,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ROUTE109_SEASHORE_HOUSE"),
        group: 28,
        num: 0,
        name: "Route109_SeashoreHouse",
        layout: LayoutId("LAYOUT_ROUTE109_SEASHORE_HOUSE"),
        music: MusicId(427), // MUS_DEWFORD
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_109"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ROUTE110_TRICK_HOUSE_ENTRANCE"),
        group: 29,
        num: 0,
        name: "Route110_TrickHouseEntrance",
        layout: LayoutId("LAYOUT_ROUTE110_TRICK_HOUSE_ENTRANCE"),
        music: MusicId(448), // MUS_TRICK_HOUSE
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_110"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ROUTE110_TRICK_HOUSE_END"),
        group: 29,
        num: 1,
        name: "Route110_TrickHouseEnd",
        layout: LayoutId("LAYOUT_ROUTE110_TRICK_HOUSE_END"),
        music: MusicId(448), // MUS_TRICK_HOUSE
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_110"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ROUTE110_TRICK_HOUSE_CORRIDOR"),
        group: 29,
        num: 2,
        name: "Route110_TrickHouseCorridor",
        layout: LayoutId("LAYOUT_ROUTE110_TRICK_HOUSE_CORRIDOR"),
        music: MusicId(448), // MUS_TRICK_HOUSE
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_110"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ROUTE110_TRICK_HOUSE_PUZZLE1"),
        group: 29,
        num: 3,
        name: "Route110_TrickHousePuzzle1",
        layout: LayoutId("LAYOUT_ROUTE110_TRICK_HOUSE_PUZZLE1"),
        music: MusicId(448), // MUS_TRICK_HOUSE
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_110"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ROUTE110_TRICK_HOUSE_PUZZLE2"),
        group: 29,
        num: 4,
        name: "Route110_TrickHousePuzzle2",
        layout: LayoutId("LAYOUT_ROUTE110_TRICK_HOUSE_PUZZLE2"),
        music: MusicId(448), // MUS_TRICK_HOUSE
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_110"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ROUTE110_TRICK_HOUSE_PUZZLE3"),
        group: 29,
        num: 5,
        name: "Route110_TrickHousePuzzle3",
        layout: LayoutId("LAYOUT_ROUTE110_TRICK_HOUSE_PUZZLE3"),
        music: MusicId(448), // MUS_TRICK_HOUSE
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_110"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ROUTE110_TRICK_HOUSE_PUZZLE4"),
        group: 29,
        num: 6,
        name: "Route110_TrickHousePuzzle4",
        layout: LayoutId("LAYOUT_ROUTE110_TRICK_HOUSE_PUZZLE4"),
        music: MusicId(448), // MUS_TRICK_HOUSE
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_110"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ROUTE110_TRICK_HOUSE_PUZZLE5"),
        group: 29,
        num: 7,
        name: "Route110_TrickHousePuzzle5",
        layout: LayoutId("LAYOUT_ROUTE110_TRICK_HOUSE_PUZZLE5"),
        music: MusicId(448), // MUS_TRICK_HOUSE
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_110"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ROUTE110_TRICK_HOUSE_PUZZLE6"),
        group: 29,
        num: 8,
        name: "Route110_TrickHousePuzzle6",
        layout: LayoutId("LAYOUT_ROUTE110_TRICK_HOUSE_PUZZLE6"),
        music: MusicId(448), // MUS_TRICK_HOUSE
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_110"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ROUTE110_TRICK_HOUSE_PUZZLE7"),
        group: 29,
        num: 9,
        name: "Route110_TrickHousePuzzle7",
        layout: LayoutId("LAYOUT_ROUTE110_TRICK_HOUSE_PUZZLE7"),
        music: MusicId(448), // MUS_TRICK_HOUSE
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_110"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ROUTE110_TRICK_HOUSE_PUZZLE8"),
        group: 29,
        num: 10,
        name: "Route110_TrickHousePuzzle8",
        layout: LayoutId("LAYOUT_ROUTE110_TRICK_HOUSE_PUZZLE8"),
        music: MusicId(448), // MUS_TRICK_HOUSE
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_110"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ROUTE110_SEASIDE_CYCLING_ROAD_SOUTH_ENTRANCE"),
        group: 29,
        num: 11,
        name: "Route110_SeasideCyclingRoadSouthEntrance",
        layout: LayoutId("LAYOUT_ROUTE110_SEASIDE_CYCLING_ROAD_ENTRANCE"),
        music: MusicId(433), // MUS_SLATEPORT
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_110"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: true,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ROUTE110_SEASIDE_CYCLING_ROAD_NORTH_ENTRANCE"),
        group: 29,
        num: 12,
        name: "Route110_SeasideCyclingRoadNorthEntrance",
        layout: LayoutId("LAYOUT_ROUTE110_SEASIDE_CYCLING_ROAD_ENTRANCE"),
        music: MusicId(433), // MUS_SLATEPORT
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_110"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: true,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ROUTE113_GLASS_WORKSHOP"),
        group: 30,
        num: 0,
        name: "Route113_GlassWorkshop",
        layout: LayoutId("LAYOUT_HOUSE4"),
        music: MusicId(399), // MUS_RUSTBORO
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_113"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ROUTE123_BERRY_MASTERS_HOUSE"),
        group: 31,
        num: 0,
        name: "Route123_BerryMastersHouse",
        layout: LayoutId("LAYOUT_HOUSE2"),
        music: MusicId(399), // MUS_RUSTBORO
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_123"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ROUTE119_WEATHER_INSTITUTE_1F"),
        group: 32,
        num: 0,
        name: "Route119_WeatherInstitute_1F",
        layout: LayoutId("LAYOUT_ROUTE119_WEATHER_INSTITUTE_1F"),
        music: MusicId(399), // MUS_RUSTBORO
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_119"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ROUTE119_WEATHER_INSTITUTE_2F"),
        group: 32,
        num: 1,
        name: "Route119_WeatherInstitute_2F",
        layout: LayoutId("LAYOUT_ROUTE119_WEATHER_INSTITUTE_2F"),
        music: MusicId(399), // MUS_RUSTBORO
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_119"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ROUTE119_HOUSE"),
        group: 32,
        num: 2,
        name: "Route119_House",
        layout: LayoutId("LAYOUT_HOUSE1"),
        music: MusicId(399), // MUS_RUSTBORO
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_119"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
    MapHeader {
        id: MapId("MAP_ROUTE124_DIVING_TREASURE_HUNTERS_HOUSE"),
        group: 33,
        num: 0,
        name: "Route124_DivingTreasureHuntersHouse",
        layout: LayoutId("LAYOUT_ROUTE124_DIVING_TREASURE_HUNTERS_HOUSE"),
        music: MusicId(408), // MUS_LILYCOVE
        region_map_section: RegionMapSectionId("MAPSEC_ROUTE_124"),
        requires_flash: false,
        weather: Weather::None,    // WEATHER_NONE
        map_type: MapType::Indoor, // MAP_TYPE_INDOOR
        allow_bike: false,
        allow_escape: false,
        allow_run: false,
        show_name: false,
        battle_scene: BattleScene::Normal, // MAP_BATTLE_SCENE_NORMAL
        connections: &[],
    },
];

// --- end generated ---

/// The map-header table: owned, read-only access to every map's header
/// metadata and connections with typed lookup `(oop-boundaries)`.
#[derive(Debug, Clone, Copy)]
pub struct MapHeaderTable {
    headers: &'static [MapHeader; MAP_COUNT],
    groups: &'static [MapGroup; MAP_GROUP_COUNT],
}

impl MapHeaderTable {
    /// The number of entries in the table ([`MAP_COUNT`]).
    pub const LEN: usize = MAP_COUNT;

    /// Build the table over the extracted upstream data.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            headers: &HEADERS,
            groups: &MAP_GROUPS,
        }
    }

    /// The header for `id`, or `None` if no entry names that map.
    #[must_use]
    pub fn get(&self, id: MapId) -> Option<&'static MapHeader> {
        self.headers.iter().find(|h| h.id == id)
    }

    /// The header for `id`.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownMapHeader`] if no entry names that map.
    pub fn header(&self, id: MapId) -> Result<&'static MapHeader, AssetError> {
        self.get(id).ok_or(AssetError::UnknownMapHeader(id.0))
    }

    /// The header at the given `(group, num)` position, or `None` if out of
    /// range.
    #[must_use]
    pub fn get_by_position(&self, group: u8, num: u8) -> Option<&'static MapHeader> {
        self.headers
            .iter()
            .find(|h| h.group == group && h.num == num)
    }

    /// The map group at index `group` (`MAP_GROUP` index into
    /// [`MAP_GROUPS`]), or `None` if out of range.
    #[must_use]
    pub fn group(&self, group: u8) -> Option<&'static MapGroup> {
        self.groups.get(usize::from(group))
    }

    /// Every map group, in `group_order` order.
    pub fn groups(&self) -> impl Iterator<Item = &'static MapGroup> {
        self.groups.iter()
    }

    /// Iterate over every header, in upstream group/position order.
    pub fn iter(&self) -> impl Iterator<Item = &'static MapHeader> {
        self.headers.iter()
    }

    /// The number of entries in the table (`MAP_COUNT`).
    #[must_use]
    pub const fn len(&self) -> usize {
        MAP_COUNT
    }

    /// Always `false` — the table is never empty. Present for API convention.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }
}

impl Default for MapHeaderTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BattleScene, Direction, MapHeaderTable, MapType, MusicId, Weather, MAP_COUNT,
        MAP_GROUP_COUNT,
    };
    use crate::error::AssetError;
    use crate::wild_encounters::MapId;

    #[test]
    fn table_length_matches_map_count() {
        let table = MapHeaderTable::new();
        assert_eq!(MAP_COUNT, 518);
        assert_eq!(table.len(), 518);
        assert_eq!(MapHeaderTable::LEN, 518);
        assert_eq!(table.iter().count(), 518);
        assert!(!table.is_empty());
        assert_eq!(MAP_GROUP_COUNT, 34);
        assert_eq!(table.groups().count(), 34);
    }

    #[test]
    fn upstream_tie_petalburg_city() {
        let table = MapHeaderTable::new();
        let h = table.header(MapId("MAP_PETALBURG_CITY")).unwrap();
        assert_eq!(h.name, "PetalburgCity");
        assert_eq!(h.music, MusicId(362)); // MUS_PETALBURG
        assert_eq!(h.region_map_section.name(), "MAPSEC_PETALBURG_CITY");
        assert!(!h.requires_flash);
        assert_eq!(h.weather, Weather::Sunny);
        assert_eq!(h.map_type, MapType::City);
        assert!(h.allow_bike);
        assert!(!h.allow_escape);
        assert!(h.allow_run);
        assert!(h.show_name);
        assert_eq!(h.battle_scene, BattleScene::Normal);
        assert_eq!(h.connections.len(), 2);
        assert_eq!(h.connections[0].direction, Direction::West);
        assert_eq!(h.connections[0].offset, -50);
        assert_eq!(h.connections[0].target, MapId("MAP_ROUTE104"));
        assert_eq!(h.connections[1].direction, Direction::East);
        assert_eq!(h.connections[1].offset, 10);
        assert_eq!(h.connections[1].target, MapId("MAP_ROUTE102"));
    }

    #[test]
    fn upstream_tie_route_101_connections() {
        // Route 101 connects north to Oldale Town and south to Littleroot
        // Town. It does *not* connect directly to Route 103 (both only
        // border Oldale Town, one to its west/north and the other to its
        // north) — verified against the extracted data, not assumed.
        let table = MapHeaderTable::new();
        let h = table.header(MapId("MAP_ROUTE101")).unwrap();
        assert_eq!(h.connections.len(), 2);
        assert_eq!(h.connections[0].direction, Direction::North);
        assert_eq!(h.connections[0].target, MapId("MAP_OLDALE_TOWN"));
        assert_eq!(h.connections[1].direction, Direction::South);
        assert_eq!(h.connections[1].target, MapId("MAP_LITTLEROOT_TOWN"));
        assert!(!h
            .connections
            .iter()
            .any(|c| c.target == MapId("MAP_ROUTE103")));

        let route103 = table.header(MapId("MAP_ROUTE103")).unwrap();
        assert!(route103
            .connections
            .iter()
            .any(|c| c.target == MapId("MAP_OLDALE_TOWN")));
    }

    #[test]
    fn group_and_num_round_trip_through_position_lookup() {
        // Every header's (group, num) resolves back to itself, and every
        // group's map list is exactly MAP_COUNT long in total with no
        // duplicates or gaps.
        let table = MapHeaderTable::new();
        let mut seen = std::collections::HashSet::new();
        for h in table.iter() {
            assert_eq!(table.get_by_position(h.group, h.num), Some(h));
            let group = table.group(h.group).expect("group in range");
            assert_eq!(group.maps[h.num as usize], h.id);
            assert!(seen.insert((h.group, h.num)), "duplicate (group, num)");
        }
        let total: usize = table.groups().map(|g| g.maps.len()).sum();
        assert_eq!(total, MAP_COUNT);
    }

    #[test]
    fn every_map_id_is_unique() {
        let table = MapHeaderTable::new();
        let ids: Vec<_> = table.iter().map(|h| h.id).collect();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(ids.len(), unique.len(), "duplicate MapId in table");
    }

    #[test]
    fn unknown_map_header_is_an_error() {
        let table = MapHeaderTable::new();
        assert_eq!(table.get(MapId("MAP_NOT_REAL")), None);
        assert_eq!(
            table.header(MapId("MAP_NOT_REAL")),
            Err(AssetError::UnknownMapHeader("MAP_NOT_REAL")),
        );
    }

    #[test]
    fn weather_id_round_trips() {
        for id in [
            0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 20, 21,
        ] {
            let w = Weather::from_id(id).unwrap();
            assert_eq!(w.id(), id);
        }
        assert_eq!(Weather::from_id(16), Err(AssetError::UnknownWeather(16)));
    }

    #[test]
    fn map_type_id_round_trips() {
        for id in 0u8..=9 {
            let t = MapType::from_id(id).unwrap();
            assert_eq!(t.id(), id);
        }
        assert_eq!(MapType::from_id(10), Err(AssetError::UnknownMapType(10)));
    }

    #[test]
    fn battle_scene_id_round_trips() {
        for id in 0u8..=8 {
            let s = BattleScene::from_id(id).unwrap();
            assert_eq!(s.id(), id);
        }
        assert_eq!(
            BattleScene::from_id(9),
            Err(AssetError::UnknownBattleScene(9))
        );
    }

    #[test]
    fn direction_id_round_trips() {
        for id in 1u8..=6 {
            let d = Direction::from_id(id).unwrap();
            assert_eq!(d.id(), id);
        }
        assert_eq!(
            Direction::from_id(0),
            Err(AssetError::UnknownConnectionDirection(0))
        );
    }

    #[test]
    fn every_header_weather_map_type_battle_scene_are_in_range() {
        // Structural guard: every transcribed enum value round-trips its own
        // id (i.e. was constructed from a value `from_id` also accepts).
        let table = MapHeaderTable::new();
        for h in table.iter() {
            assert!(Weather::from_id(h.weather.id()).is_ok());
            assert!(MapType::from_id(h.map_type.id()).is_ok());
            assert!(BattleScene::from_id(h.battle_scene.id()).is_ok());
            for c in h.connections {
                assert!(Direction::from_id(c.direction.id()).is_ok());
            }
        }
    }
}
