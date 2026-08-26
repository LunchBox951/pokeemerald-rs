//! [`MapRuntime`]: the current map's data, bound together for movement and
//! collision queries (S-5, issue #108).
//!
//! Ported behaviour:
//! - Metatile grid/behavior lookup: `pokeemerald/src/fieldmap.c`'s
//!   `MapGridGetMetatileIdAt` / `MapGridGetCollisionAt` /
//!   `MapGridGetElevationAt` / `GetMetatileAttributesById` /
//!   `MapGridGetMetatileBehaviorAt`.
//! - Connection-edge crossing: `fieldmap.c`'s `IsPosInConnectingMap` /
//!   `IsCoordInConnectingMap` / `SetPositionFromConnection` family (see
//!   [`MapRuntime::resolve_connection`] for the exact correspondence and the
//!   one deliberate simplification against that family).
//! - Object/warp/coord event lookup by position:
//!   `src/event_object_movement.c`'s `GetObjectEventIdByPosition` /
//!   `ObjectEventDoesElevationMatch` (object or query elevation `0` is a
//!   wildcard), and `src/field_control_avatar.c`'s
//!   `GetWarpEventAtPosition` / `GetCoordEventScriptAtPosition` (only the
//!   stored event elevation `0` is a wildcard).
//!
//! **What this type does *not* own.** Per the issue #108 scope, `MapRuntime`
//! binds to exactly one map's already-decoded data; it never loads a pack
//! itself (that's the integration lane's job — see the crate-level
//! `crate::overworld` docs) and it never holds every map in the game at
//! once. Both the current map's [`LayoutGrid`]/[`MetatileAttributeTable`]
//! bytes and a connection target's dimensions/landing cells are supplied by
//! the caller — the latter through the small [`ConnectedMapData`] trait
//! rather than a crate-owned table of every map, so tests can supply a
//! two-entry synthetic map graph instead of the real 518-map table.

use assets::{
    CoordEvent, LayoutGrid, MapConnection, MapEvents, MapHeader, MapId, MetatileAttributeTable,
    MetatileCell, ObjectEvent, WarpEvent,
};

use super::direction::Direction;

/// Upstream `NUM_METATILES_IN_PRIMARY` (`include/fieldmap.h`): metatile ids
/// below this come from the primary tileset's attribute table; ids at or
/// above it come from the secondary tileset's, re-based by subtracting this
/// constant (mirrors `GetMetatileAttributesById`, `fieldmap.c`).
pub const NUM_METATILES_IN_PRIMARY: u16 = 512;

/// Where a connection-edge crossing lands, resolved by
/// [`MapRuntime::resolve_connection`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectionCrossing {
    /// The neighbouring map the crossing enters.
    pub target: MapId,
    /// The landing tile position within `target`'s own coordinate space.
    pub position: (i32, i32),
}

/// Supplies the connected-map data needed to resolve and validate an edge
/// crossing without making [`MapRuntime`] own every map.
///
/// The integration lane implements this by wrapping the real
/// `assets::MapHeaderTable` (to go from a [`MapId`] to its `layout` id) and
/// `assets::LayoutTable` (to go from a layout id to its width/height), plus
/// the destination layout's decoded grid cell. These are caller-owned
/// tables/pack data, so wrapping them here does not violate the "don't load
/// the pack yourself" rule. Tests implement this over a small hand-written
/// map graph (see `crate::overworld::map_runtime::tests`).
///
/// A blanket implementation over `Fn(MapId) -> Option<(u16, u16)>` is
/// provided below for geometry-only users such as
/// [`MapRuntime::resolve_connection`]. Its cell lookup deliberately returns
/// `None`, so [`crate::overworld::player::PlayerState::step`] fails closed
/// rather than entering a connected map whose landing tile was not supplied.
pub trait ConnectedMapData {
    /// `map`'s `(width, height)` in metatiles, or `None` if `map` is not
    /// known to this dimension source.
    fn dimensions(&self, map: MapId) -> Option<(u16, u16)>;

    /// The decoded cell at `(x, y)` in `map`, or `None` if the map/cell data
    /// is unavailable. Player movement requires this to validate collision
    /// and elevation before committing a connection crossing, mirroring the
    /// neighbouring cells in upstream's padded map grid.
    fn metatile_cell(&self, _map: MapId, _x: i32, _y: i32) -> Option<MetatileCell> {
        None
    }
}

impl<F> ConnectedMapData for F
where
    F: Fn(MapId) -> Option<(u16, u16)>,
{
    fn dimensions(&self, map: MapId) -> Option<(u16, u16)> {
        self(map)
    }
}

/// The current map's data, bound together for movement/collision queries.
///
/// Holds exactly one map's already-decoded [`LayoutGrid`] and per-tileset
/// [`MetatileAttributeTable`]s, plus that map's static [`MapHeader`] (for
/// connections) and [`MapEvents`] (for object/warp/coord event lookup).
/// Crossing into a neighbouring map (via [`ConnectionCrossing`] or a warp;
/// see `crate::overworld::warp`) does not mutate a `MapRuntime` in place —
/// the caller builds a new one over the destination map's own data and
/// starts using that instead, exactly the "re-bind" the issue describes.
#[derive(Debug, Clone, Copy)]
pub struct MapRuntime<'a> {
    id: MapId,
    header: &'a MapHeader,
    events: &'a MapEvents,
    grid: LayoutGrid<'a>,
    primary_attrs: MetatileAttributeTable<'a>,
    secondary_attrs: MetatileAttributeTable<'a>,
}

impl<'a> MapRuntime<'a> {
    /// Bind a `MapRuntime` to `id`'s already-decoded data.
    ///
    /// `header`/`events` are typically `&'static` in real use (the
    /// integration lane's `assets::MapHeaderTable`/`assets::MapEventsTable`
    /// hand out `&'static` entries), but this only requires them to outlive
    /// the grid/attribute bytes, so synthetic tests can supply plain local
    /// values without leaking them.
    ///
    /// `primary_attrs`/`secondary_attrs` are `id`'s layout's primary and
    /// secondary tileset attribute tables respectively (see
    /// [`MapRuntime::metatile_behavior`] for how a metatile id picks between
    /// them).
    #[must_use]
    pub const fn new(
        id: MapId,
        header: &'a MapHeader,
        events: &'a MapEvents,
        grid: LayoutGrid<'a>,
        primary_attrs: MetatileAttributeTable<'a>,
        secondary_attrs: MetatileAttributeTable<'a>,
    ) -> Self {
        Self {
            id,
            header,
            events,
            grid,
            primary_attrs,
            secondary_attrs,
        }
    }

    /// This runtime's bound map id.
    #[must_use]
    pub const fn id(&self) -> MapId {
        self.id
    }

    /// This runtime's bound map header (connections, layout id, ...).
    #[must_use]
    pub const fn header(&self) -> &'a MapHeader {
        self.header
    }

    /// This runtime's bound map's object/warp/coord/bg events.
    #[must_use]
    pub const fn events(&self) -> &'a MapEvents {
        self.events
    }

    /// The decoded grid cell at `(x, y)`, or `None` if that position is
    /// outside this map's own grid (mirrors `MapGridGetMetatileIdAt`'s
    /// bounds check, minus the border-block fallback — an out-of-bounds
    /// position is `crate::overworld::map_runtime::MapRuntime::resolve_connection`'s
    /// job to interpret, not a border texture's).
    #[must_use]
    pub fn metatile_cell(&self, x: i32, y: i32) -> Option<MetatileCell> {
        let x = u16::try_from(x).ok()?;
        let y = u16::try_from(y).ok()?;
        self.grid.cell_at(x, y)
    }

    /// The elevation an object event arrives at when placed on the metatile
    /// at `(x, y)` — the landing-tile read inside upstream
    /// `ObjectEventUpdateElevation`
    /// (`pokeemerald/src/event_object_movement.c:7759-7771`), reached via
    /// `UpdateObjectEventElevationAndPriority` on the very first
    /// `DoGroundEffects_OnSpawn` after a warp lands (`:8088-8090`), before
    /// input ever unlocks.
    ///
    /// Returns `None` under the same bound [`MapRuntime::metatile_cell`]
    /// does: `(x, y)` outside this map's own grid.
    ///
    /// **The multi-level rule, folded to a substitution.** Upstream's
    /// `ObjectEventUpdateElevation` reads `curElevation` off the grid
    /// (`:7761`) and returns immediately -- touching neither
    /// `currentElevation` nor `previousElevation` -- when that read comes
    /// back [`super::collision::ELEVATION_MULTI_LEVEL`] (the guard and its
    /// early return, `:7764-7765`), so a multi-level landing tile leaves an
    /// already-live object event's elevation exactly where it was. Every
    /// caller of this method places a *fresh*
    /// [`super::player::PlayerState`], though, whose elevation
    /// upstream's own spawn-time initialization already set to
    /// [`super::collision::ELEVATION_TRANSITION`] before this read would
    /// ever run -- so "leave it alone" and "return
    /// [`super::collision::ELEVATION_TRANSITION`]" are the same outcome here,
    /// and this method returns the latter directly rather than modelling a
    /// no-op against state that was never live.
    ///
    /// **Why one coordinate pair models a two-read guard.** Upstream's
    /// guard tests `previousCoords`' elevation as well as `currentCoords`'
    /// (`prevElevation`, read at `:7762`), and this method takes a single
    /// `(x, y)`. On an *arrival* those two coordinate pairs name the same
    /// cell: a spawning object event seeds `previousCoords` equal to
    /// `currentCoords` (`InitObjectEventStateFromTemplate`,
    /// `event_object_movement.c:1309-1312`), and this read runs on the spawn
    /// frame itself, before any step has moved one of them apart. So the
    /// `prevElevation` half of the guard collapses into the `curElevation`
    /// half here rather than being dropped.
    ///
    /// **The arrival half of the rule, not the whole of it.** This models
    /// `ObjectEventUpdateElevation` where the player is *placed*;
    /// `PlayerState::adopt_elevation`, described in
    /// [`super::player::PlayerState::step`]'s "# Elevation adoption"
    /// section, models the same upstream function per *step*, where the two
    /// coordinate pairs really do differ and both fields are adopted
    /// separately. Between them they cover the calls this port models on the
    /// player; this one is the read shared by
    /// [`super::warp::warp_destination_position`] (a resolved warp event),
    /// `pokeemerald_rs`'s `OverworldPhase::warp_to_position` (an
    /// explicit-coordinate warp, e.g. white-out/first-battle relocation), and
    /// `pokeemerald_rs`'s `saved_tile_placement` (a continued save) -- so the
    /// three cannot drift apart on the same upstream rule again (issue
    /// #379).
    #[must_use]
    pub fn arrival_elevation(&self, x: i32, y: i32) -> Option<u8> {
        let cell = self.metatile_cell(x, y)?;
        Some(
            if cell.elevation == super::collision::ELEVATION_MULTI_LEVEL {
                super::collision::ELEVATION_TRANSITION
            } else {
                cell.elevation
            },
        )
    }

    /// The decoded behavior id (upstream `MB_*`) of the metatile at
    /// `(x, y)`, or `None` if `(x, y)` is out of bounds or its metatile
    /// attribute entry fails to decode (see
    /// `crate::overworld::metatile_behavior`'s module docs for what a
    /// `None` here means downstream: ordinary ground, not a recognized
    /// door/warp).
    ///
    /// Mirrors `GetMetatileAttributesById` + `MapGridGetMetatileBehaviorAt`
    /// (`fieldmap.c`): a metatile id below [`NUM_METATILES_IN_PRIMARY`]
    /// reads the primary tileset's attribute table; at or above it reads
    /// the secondary tileset's, re-based by subtracting
    /// [`NUM_METATILES_IN_PRIMARY`].
    #[must_use]
    pub fn metatile_behavior(&self, x: i32, y: i32) -> Option<u8> {
        let cell = self.metatile_cell(x, y)?;
        let attribute = if cell.metatile_id < NUM_METATILES_IN_PRIMARY {
            self.primary_attrs.attribute_at(cell.metatile_id)
        } else {
            self.secondary_attrs
                .attribute_at(cell.metatile_id - NUM_METATILES_IN_PRIMARY)
        };
        attribute.and_then(Result::ok).map(|a| a.behavior)
    }

    /// Every object event whose static position is `(x, y, elevation)`, in
    /// map.json declaration order — mirroring the *scan* upstream
    /// `GetObjectEventIdByPosition` / `DoesObjectCollideWithObjectAt`
    /// perform over `gObjectEvents` (`event_object_movement.c:2192-2215`,
    /// `:4724-4742`), with `ObjectEventDoesElevationMatch` /
    /// `AreElevationsCompatible` (`:7789-7797`, the same predicate spelled
    /// twice upstream) as the elevation test: position must match exactly;
    /// elevation must match unless either the query or the object uses
    /// [`super::collision::ELEVATION_TRANSITION`] as a wildcard.
    ///
    /// **An iterator, not a single hit, on purpose.** Templates may stack on
    /// one tile with independent hide flags — the Birch lab's three starter
    /// balls all sit at `(6, 8)` — and upstream only ever scans object
    /// events that were actually *spawned*, which a set hide flag prevents
    /// (`TrySpawnObjectEvents`' `!FlagGet(template->flagId)` gate,
    /// `event_object_movement.c:1670-1672`). This port has no spawn step, so
    /// the hide-flag filter has to be applied *inside* the scan rather than
    /// to its first hit; that composition lives one layer up, in
    /// [`super::object_event::visible_object_event_at`], which is what
    /// callers wanting "the object event upstream would find here" should
    /// use. This method deliberately knows nothing about flags: it is the
    /// pure map-data half.
    pub fn object_events_at(
        &self,
        x: i32,
        y: i32,
        elevation: u8,
    ) -> impl Iterator<Item = &'static ObjectEvent> {
        let objects = self.events.object_events;
        objects.iter().filter(move |o| {
            i32::from(o.x) == x
                && i32::from(o.y) == y
                && super::collision::elevations_compatible(o.elevation, elevation)
        })
    }

    /// The warp event at `(x, y, elevation)`, mirroring `GetWarpEventAtPosition`
    /// (`field_control_avatar.c`): position must match exactly, and
    /// `elevation` must equal the warp's own elevation or the warp's
    /// elevation must be [`super::collision::ELEVATION_TRANSITION`] (a
    /// wildcard upstream uses so a warp on a transition tile fires
    /// regardless of which level the player arrived from).
    #[must_use]
    pub fn warp_event_at(&self, x: i32, y: i32, elevation: u8) -> Option<&'static WarpEvent> {
        self.events.warp_events.iter().find(|w| {
            i32::from(w.x) == x
                && i32::from(w.y) == y
                && (w.elevation == elevation
                    || w.elevation == super::collision::ELEVATION_TRANSITION)
        })
    }

    /// Every coord (trigger/weather) event at `(x, y, elevation)`, in
    /// `map.json` declaration order — mirroring the *scan* upstream
    /// `GetCoordEventScriptAtPosition` (`field_control_avatar.c:897-916`)
    /// performs over `mapHeader->events->coordEvents`, same
    /// position-plus-elevation-wildcard matching as
    /// [`MapRuntime::warp_event_at`] (only the stored event's own elevation
    /// `0` is a wildcard, not the query's).
    ///
    /// **An iterator, not a single hit, on purpose — the same shape
    /// [`MapRuntime::object_events_at`] uses for its own stacked case.**
    /// Upstream's loop (`:903-914`) calls `TryRunCoordEventScript`
    /// (`:877-895`) on every positional match in turn, **ends the scan at
    /// the first candidate that yields a script** (`:909-911`), and keeps
    /// going past every candidate that yields `NULL` instead — of which
    /// there are exactly three: a weather event
    /// (`coordEvent->script == NULL`) dispatches `DoCoordEventWeather` and
    /// falls through (`:881-884`); a `TRIGGER_RUN_IMMEDIATELY` event
    /// (`trigger == 0`, `include/constants/vars.h:312`) has its script run
    /// on the spot and *still* returns `NULL` (`:886-889`) — 36 bundled
    /// coord events across five maps are of that kind, 24 of them on Route
    /// 111 alone; and a trigger whose `VarGet(trigger) == (u8)index` check
    /// fails (`:891`) is skipped rather than ending the search — so a later
    /// event stacked on the same tile can still run (Littleroot Town stacks
    /// `LittlerootTown_EventScript_NeedPokemonTriggerRight` and
    /// `LittlerootTown_EventScript_GoSaveBirchTrigger` at `(11, 1)`; Jagged
    /// Pass stacks a `Sunny` weather event under
    /// `JaggedPass_EventScript_OpenMagmaHideout` at `(14, 15)`). This method
    /// returns every positional match in order and leaves candidate
    /// evaluation to its caller — skipping a weather event (no coord-event
    /// weather consumer exists in this port yet) and running the var check
    /// are the caller's — see `pokeemerald_rs`'s
    /// `flow::overworld_phase::first_battle_trigger` for the caller-side
    /// scan this replaced a first-match `Option` with (issue #343).
    pub fn coord_events_at(
        &self,
        x: i32,
        y: i32,
        elevation: u8,
    ) -> impl Iterator<Item = &'static CoordEvent> {
        let coord_events = self.events.coord_events;
        coord_events.iter().filter(move |c| {
            i32::from(c.x) == x
                && i32::from(c.y) == y
                && (c.elevation == elevation
                    || c.elevation == super::collision::ELEVATION_TRANSITION)
        })
    }

    /// Resolve a step in `direction` that lands on `(x, y)` — already
    /// outside this map's own grid — against this map's connections.
    ///
    /// Mirrors the offset math of upstream `IsPosInConnectingMap` /
    /// `IsCoordInConnectingMap` (perpendicular-axis bounds check) and
    /// `SetPositionFromConnection` (landing position), **simplified**: this
    /// port has no camera and no pixel-precision backing-buffer preload
    /// (`InitBackupMapLayoutConnections`'s border-padding scheme,
    /// `MAP_OFFSET` = 7 tiles), so a connection is resolved the instant a
    /// step would land outside the current grid, rather than upstream's
    /// "still within the padding, camera hasn't swapped yet" window. The
    /// landing tile itself is exact: crossing this map's south edge lands
    /// at the neighbour's `y = 0` (its north edge); crossing north lands at
    /// the neighbour's `y = height - 1`; crossing west lands at the
    /// neighbour's `x = width - 1`; crossing east lands at `x = 0`. The
    /// perpendicular coordinate carries across via `coord - connection.offset`
    /// in both directions, matching `IsPosInConnectingMap` exactly.
    #[must_use]
    pub fn resolve_connection(
        &self,
        direction: Direction,
        x: i32,
        y: i32,
        maps: &impl ConnectedMapData,
    ) -> Option<ConnectionCrossing> {
        let wanted = direction.to_connection_direction();
        for connection in self.header.connections {
            if connection.direction != wanted {
                continue;
            }
            let Some((target_width, target_height)) = maps.dimensions(connection.target) else {
                continue;
            };
            let Some(position) =
                landing_position(direction, connection, x, y, target_width, target_height)
            else {
                continue;
            };
            return Some(ConnectionCrossing {
                target: connection.target,
                position,
            });
        }
        None
    }
}

/// The landing tile in a connection's target map for a step in `direction`
/// that left the source map through `connection`, or `None` if `(x, y)`'s
/// perpendicular coordinate falls outside the target's bounds once
/// `connection.offset` is applied (upstream `IsCoordInConnectingMap`).
fn landing_position(
    direction: Direction,
    connection: &MapConnection,
    x: i32,
    y: i32,
    target_width: u16,
    target_height: u16,
) -> Option<(i32, i32)> {
    match direction {
        Direction::South | Direction::North => {
            let target_x = x - connection.offset;
            if target_x < 0 || target_x >= i32::from(target_width) {
                return None;
            }
            let target_y = if matches!(direction, Direction::South) {
                0
            } else {
                i32::from(target_height) - 1
            };
            Some((target_x, target_y))
        }
        Direction::West | Direction::East => {
            let target_y = y - connection.offset;
            if target_y < 0 || target_y >= i32::from(target_height) {
                return None;
            }
            let target_x = if matches!(direction, Direction::East) {
                0
            } else {
                i32::from(target_width) - 1
            };
            Some((target_x, target_y))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assets::{BattleScene, MapType, RegionMapSectionId, Weather};
    use assets::{CoordEventKind, CoordWeather};

    fn cell(metatile_id: u16, collision: u8, elevation: u8) -> u16 {
        MetatileCell {
            metatile_id,
            collision,
            elevation,
        }
        .pack()
    }

    fn grid_bytes(width: u16, height: u16, fill: u16) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(usize::from(width) * usize::from(height) * 2);
        for _ in 0..(u32::from(width) * u32::from(height)) {
            bytes.extend_from_slice(&fill.to_le_bytes());
        }
        bytes
    }

    fn header(id: &'static str, connections: &'static [MapConnection]) -> MapHeader {
        MapHeader {
            id: MapId(id),
            group: 0,
            num: 0,
            name: id,
            layout: assets::LayoutId(id),
            music: assets::MusicId(0),
            region_map_section: RegionMapSectionId("MAPSEC_NONE"),
            requires_flash: false,
            weather: Weather::None,
            map_type: MapType::Route,
            allow_bike: true,
            allow_escape: true,
            allow_run: true,
            show_name: false,
            battle_scene: BattleScene::Normal,
            connections,
        }
    }

    fn empty_events(id: &'static str) -> MapEvents {
        MapEvents {
            id: MapId(id),
            shared_events_map: None,
            object_events: &[],
            warp_events: &[],
            coord_events: &[],
            bg_events: &[],
        }
    }

    #[test]
    fn metatile_cell_reads_in_bounds_and_none_out_of_bounds() {
        let bytes = grid_bytes(4, 4, cell(7, 0, 3));
        let hdr = header("MAP_A", &[]);
        let events = empty_events("MAP_A");
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_A"),
            name: "MapA",
            width: 4,
            height: 4,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&bytes).unwrap();
        let runtime = MapRuntime::new(
            MapId("MAP_A"),
            Box::leak(Box::new(hdr)),
            Box::leak(Box::new(events)),
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );

        let found = runtime.metatile_cell(1, 1).unwrap();
        assert_eq!(found.metatile_id, 7);
        assert_eq!(found.collision, 0);
        assert_eq!(found.elevation, 3);

        assert!(runtime.metatile_cell(-1, 0).is_none());
        assert!(runtime.metatile_cell(4, 0).is_none());
        assert!(runtime.metatile_cell(0, 4).is_none());
    }

    /// [`MapRuntime::arrival_elevation`]'s own substitution (issue #379): an
    /// ordinary cell's elevation passes straight through, an
    /// [`super::super::collision::ELEVATION_MULTI_LEVEL`] (`15`) cell
    /// substitutes [`super::super::collision::ELEVATION_TRANSITION`] (`0`),
    /// and an out-of-bounds position is `None` under the same bound
    /// [`MapRuntime::metatile_cell`] uses -- the one read
    /// [`super::super::warp::warp_destination_position`],
    /// `pokeemerald_rs::flow::overworld_phase::OverworldPhase::warp_to_position`,
    /// and `pokeemerald_rs`'s `saved_tile_placement` all now share.
    #[test]
    fn arrival_elevation_substitutes_transition_for_multi_level() {
        use super::super::collision::{ELEVATION_MULTI_LEVEL, ELEVATION_TRANSITION};

        let mut bytes = grid_bytes(2, 1, cell(0, 0, 3));
        bytes[2..4].copy_from_slice(&cell(1, 0, ELEVATION_MULTI_LEVEL).to_le_bytes());
        let hdr = header("MAP_C", &[]);
        let events = empty_events("MAP_C");
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_C"),
            name: "MapC",
            width: 2,
            height: 1,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&bytes).unwrap();
        let runtime = MapRuntime::new(
            MapId("MAP_C"),
            Box::leak(Box::new(hdr)),
            Box::leak(Box::new(events)),
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );

        assert_eq!(
            runtime.arrival_elevation(0, 0),
            Some(3),
            "an ordinary cell's own elevation passes straight through"
        );
        assert_eq!(
            runtime.arrival_elevation(1, 0),
            Some(ELEVATION_TRANSITION),
            "a multi-level cell substitutes the transition wildcard, never the raw {ELEVATION_MULTI_LEVEL}"
        );
        assert_eq!(
            runtime.arrival_elevation(2, 0),
            None,
            "out of bounds, same as metatile_cell"
        );
    }

    #[test]
    fn metatile_behavior_picks_primary_or_secondary_table_by_id() {
        let bytes = {
            let mut b = Vec::new();
            b.extend_from_slice(&cell(0, 0, 0).to_le_bytes()); // primary id 0
            b.extend_from_slice(&cell(NUM_METATILES_IN_PRIMARY, 0, 0).to_le_bytes()); // secondary id 0
            b
        };
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_B"),
            name: "MapB",
            width: 2,
            height: 1,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&bytes).unwrap();
        let hdr = header("MAP_B", &[]);
        let events = empty_events("MAP_B");

        // Primary attribute id 0 -> behavior 5. Secondary attribute id 0
        // (metatile id NUM_METATILES_IN_PRIMARY) -> behavior 9.
        let primary_attr_bytes = 5u16.to_le_bytes();
        let secondary_attr_bytes = 9u16.to_le_bytes();
        let runtime = MapRuntime::new(
            MapId("MAP_B"),
            Box::leak(Box::new(hdr)),
            Box::leak(Box::new(events)),
            grid,
            MetatileAttributeTable::new(&primary_attr_bytes),
            MetatileAttributeTable::new(&secondary_attr_bytes),
        );

        assert_eq!(runtime.metatile_behavior(0, 0), Some(5));
        assert_eq!(runtime.metatile_behavior(1, 0), Some(9));
    }

    #[test]
    fn warp_event_at_matches_position_and_elevation_wildcard() {
        let hdr = header("MAP_C", &[]);
        let warps: &'static [WarpEvent] = Box::leak(Box::new([
            WarpEvent {
                x: 3,
                y: 4,
                elevation: 0, // ELEVATION_TRANSITION: matches any elevation.
                dest_map: assets::WarpDestination::Map(MapId("MAP_D")),
                dest_warp_id: assets::WarpId::Fixed(0),
            },
            WarpEvent {
                x: 5,
                y: 5,
                elevation: 2,
                dest_map: assets::WarpDestination::Map(MapId("MAP_D")),
                dest_warp_id: assets::WarpId::Fixed(1),
            },
        ]));
        let events: &'static MapEvents = Box::leak(Box::new(MapEvents {
            id: MapId("MAP_C"),
            shared_events_map: None,
            object_events: &[],
            warp_events: warps,
            coord_events: &[],
            bg_events: &[],
        }));
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_C"),
            name: "MapC",
            width: 8,
            height: 8,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let bytes = grid_bytes(8, 8, cell(0, 0, 0));
        let grid = layout.grid(&bytes).unwrap();
        let runtime = MapRuntime::new(
            MapId("MAP_C"),
            Box::leak(Box::new(hdr)),
            events,
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );

        // Transition-elevation warp fires regardless of the traveller's elevation.
        assert_eq!(
            runtime.warp_event_at(3, 4, 7).map(|w| w.dest_warp_id),
            Some(assets::WarpId::Fixed(0))
        );
        // Elevation-specific warp only fires at its own elevation.
        assert_eq!(runtime.warp_event_at(5, 5, 3), None);
        assert_eq!(
            runtime.warp_event_at(5, 5, 2).map(|w| w.dest_warp_id),
            Some(assets::WarpId::Fixed(1))
        );
        assert_eq!(runtime.warp_event_at(0, 0, 0), None);
    }

    #[test]
    fn coord_events_at_matches_position_and_elevation_wildcard() {
        let hdr = header("MAP_E", &[]);
        let coords: &'static [CoordEvent] = Box::leak(Box::new([
            CoordEvent {
                x: 2,
                y: 2,
                elevation: 0, // ELEVATION_TRANSITION: matches any query elevation.
                kind: CoordEventKind::Weather(CoordWeather::Rain),
            },
            CoordEvent {
                x: 3,
                y: 3,
                elevation: 3, // An ordinary elevation: no wildcard of its own.
                kind: CoordEventKind::Weather(CoordWeather::Rain),
            },
        ]));
        let events: &'static MapEvents = Box::leak(Box::new(MapEvents {
            id: MapId("MAP_E"),
            shared_events_map: None,
            object_events: &[],
            warp_events: &[],
            coord_events: coords,
            bg_events: &[],
        }));
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_E"),
            name: "MapE",
            width: 4,
            height: 4,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let bytes = grid_bytes(4, 4, cell(0, 0, 0));
        let grid = layout.grid(&bytes).unwrap();
        let runtime = MapRuntime::new(
            MapId("MAP_E"),
            Box::leak(Box::new(hdr)),
            events,
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );

        // ELEVATION_TRANSITION wildcard: the stored event's elevation `0`
        // matches a query at any other elevation.
        assert!(runtime.coord_events_at(2, 2, 9).next().is_some());
        assert!(runtime.coord_events_at(2, 3, 0).next().is_none());

        // ...and the wildcard is asymmetric. Upstream's test is
        // `coordEvents[i].elevation == elevation || coordEvents[i].elevation
        // == ELEVATION_TRANSITION` (`field_control_avatar.c:907`): only the
        // *stored* elevation has a wildcard arm, never the query's. A step
        // arriving at elevation 0 therefore does not see an elevation-3
        // coord event, which is the half of the rule an implementation could
        // silently symmetrize without any other assertion here noticing.
        assert!(
            runtime.coord_events_at(3, 3, 3).next().is_some(),
            "the stored elevation-3 event must match a query at its own elevation"
        );
        assert!(
            runtime.coord_events_at(3, 3, 0).next().is_none(),
            "a query elevation of 0 is not a wildcard -- only a stored 0 is"
        );
    }

    /// A [`MapRuntime`] over `map`'s real, compiled-in header and events
    /// (`assets::MapHeaderTable`/`assets::MapEventsTable`, no local pack
    /// needed — same premise as `pokeemerald_rs`'s own
    /// `overworld_phase::test_support::runtime_for`) but a synthetic, fully
    /// open `width`x`height` grid: real map layouts are pack-loaded binary
    /// data this crate does not compile in, and
    /// [`MapRuntime::coord_events_at`] never touches the grid, only
    /// `events.coord_events`.
    fn real_events_runtime(map: MapId, width: u16, height: u16) -> MapRuntime<'static> {
        let header = assets::MapHeaderTable::new().header(map).unwrap();
        let events = assets::MapEventsTable::new().resolve(map).unwrap();
        let layout: &'static assets::MapLayout = Box::leak(Box::new(assets::MapLayout {
            id: assets::LayoutId("TEST_OPEN_GRID"),
            name: "TestOpenGrid",
            width,
            height,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        }));
        let bytes: &'static [u8] =
            Box::leak(grid_bytes(width, height, cell(1, 0, 3)).into_boxed_slice());
        let grid = layout.grid(bytes).unwrap();
        MapRuntime::new(
            map,
            header,
            events,
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        )
    }

    /// Littleroot Town stacks two triggers on the same tile
    /// (`crates/assets/src/map_events.rs`'s `MAP_LITTLEROOT_TOWN`
    /// `coord_events`): `NeedPokemonTriggerRight`
    /// (`VAR_LITTLEROOT_TOWN_STATE == 0`) declared first, then
    /// `GoSaveBirchTrigger` (`== 1`), both at `(11, 1, 3)`. Upstream's
    /// `GetCoordEventScriptAtPosition` (`field_control_avatar.c:897-916`)
    /// would visit both in that order and let
    /// `TryRunCoordEventScript`'s var check pick whichever one currently
    /// matches; [`MapRuntime::coord_events_at`] must hand a caller the same
    /// two candidates, in the same order, so that scan is possible at all
    /// (issue #343 — the regression this port's own first-match shortcut
    /// used to prevent).
    #[test]
    fn coord_events_at_yields_the_littleroot_town_trigger_stack_in_declaration_order() {
        let runtime = real_events_runtime(MapId("MAP_LITTLEROOT_TOWN"), 20, 20);

        let candidates: Vec<&CoordEvent> = runtime.coord_events_at(11, 1, 3).collect();
        let scripts: Vec<&str> = candidates
            .iter()
            .map(|c| match c.kind {
                CoordEventKind::Trigger { script, .. } => script,
                CoordEventKind::Weather(_) => "<weather>",
            })
            .collect();
        assert_eq!(
            scripts,
            [
                "LittlerootTown_EventScript_NeedPokemonTriggerRight",
                "LittlerootTown_EventScript_GoSaveBirchTrigger",
            ],
            "the stack must come back in map.json declaration order so a caller at \
             VAR_LITTLEROOT_TOWN_STATE == 1 can walk past the first candidate and pick \
             GoSaveBirchTrigger, mirroring TryRunCoordEventScript's own keep-scanning loop"
        );

        // A caller reproducing TryRunCoordEventScript's var check picks the
        // second candidate once the state var reads `1`.
        let state = 1u16;
        let picked = candidates.into_iter().find(|c| match c.kind {
            CoordEventKind::Trigger { var_value, .. } => var_value == state,
            CoordEventKind::Weather(_) => false,
        });
        assert!(matches!(
            picked.map(|c| c.kind),
            Some(CoordEventKind::Trigger {
                script: "LittlerootTown_EventScript_GoSaveBirchTrigger",
                ..
            })
        ));
    }

    /// Jagged Pass stacks a weather event under a trigger on the same tile
    /// (`crates/assets/src/map_events.rs`'s `MAP_JAGGED_PASS`
    /// `coord_events`): a `Sunny` weather event declared first, then
    /// `JaggedPass_EventScript_OpenMagmaHideout`
    /// (`VAR_JAGGED_PASS_STATE == 1`), both at `(14, 15, 3)`. Upstream's
    /// `TryRunCoordEventScript` never yields a script for the weather event
    /// (`coordEvent->script == NULL` dispatches `DoCoordEventWeather` and
    /// returns `NULL`, `field_control_avatar.c:881-884`) so the scan falls
    /// through to the trigger behind it — the same "weather never blocks a
    /// trigger stacked on top of it" case
    /// [`MapRuntime::coord_events_at`]'s own doc comment cites.
    #[test]
    fn coord_events_at_yields_the_jagged_pass_weather_then_trigger_stack_in_order() {
        let runtime = real_events_runtime(MapId("MAP_JAGGED_PASS"), 32, 32);

        let candidates: Vec<&CoordEvent> = runtime.coord_events_at(14, 15, 3).collect();
        assert_eq!(
            candidates.len(),
            2,
            "expected exactly the weather-then-trigger stack"
        );
        assert!(
            matches!(
                candidates[0].kind,
                CoordEventKind::Weather(CoordWeather::Sunny)
            ),
            "the weather event is declared first and must come back first"
        );
        assert!(
            matches!(
                candidates[1].kind,
                CoordEventKind::Trigger {
                    script: "JaggedPass_EventScript_OpenMagmaHideout",
                    ..
                }
            ),
            "the trigger behind the weather event must still be reachable"
        );
    }

    #[test]
    fn object_events_at_matches_static_position_and_elevation_wildcard() {
        let hdr = header("MAP_F", &[]);
        let objects: &'static [ObjectEvent] = Box::leak(Box::new([
            ObjectEvent {
                local_id: 1,
                graphics_id: "OBJ_EVENT_GFX_KECLEON",
                x: 6,
                y: 6,
                elevation: 4,
                movement_type: assets::MovementType::FaceDown,
                movement_range_x: 0,
                movement_range_y: 0,
                trainer_type: assets::TrainerType::None,
                trainer_sight_or_berry_tree_id: "0",
                script: "0x0",
                flag: "0",
            },
            ObjectEvent {
                local_id: 2,
                graphics_id: "OBJ_EVENT_GFX_KECLEON_BRIDGE_SHADOW",
                x: 6,
                y: 6,
                elevation: 3,
                movement_type: assets::MovementType::FaceDown,
                movement_range_x: 0,
                movement_range_y: 0,
                trainer_type: assets::TrainerType::None,
                trainer_sight_or_berry_tree_id: "0",
                script: "0x0",
                flag: "0",
            },
            ObjectEvent {
                local_id: 3,
                graphics_id: "OBJ_EVENT_GFX_MOM",
                x: 7,
                y: 7,
                elevation: 0,
                movement_type: assets::MovementType::FaceDown,
                movement_range_x: 0,
                movement_range_y: 0,
                trainer_type: assets::TrainerType::None,
                trainer_sight_or_berry_tree_id: "0",
                script: "0x0",
                flag: "0",
            },
        ]));
        let events: &'static MapEvents = Box::leak(Box::new(MapEvents {
            id: MapId("MAP_F"),
            shared_events_map: None,
            object_events: objects,
            warp_events: &[],
            coord_events: &[],
            bg_events: &[],
        }));
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_F"),
            name: "MapF",
            width: 8,
            height: 8,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let bytes = grid_bytes(8, 8, cell(0, 0, 0));
        let grid = layout.grid(&bytes).unwrap();
        let runtime = MapRuntime::new(
            MapId("MAP_F"),
            Box::leak(Box::new(hdr)),
            events,
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );

        let ids_at = |x, y, elevation| -> Vec<u8> {
            runtime
                .object_events_at(x, y, elevation)
                .map(|o| o.local_id)
                .collect()
        };

        assert_eq!(ids_at(6, 6, 4), vec![1]);
        assert_eq!(ids_at(6, 6, 3), vec![2]);
        // The wildcard query matches *both* stacked objects at (6, 6), in
        // declaration order -- the whole reason this returns an iterator
        // (see the method's own docs).
        assert_eq!(ids_at(6, 6, 0), vec![1, 2]);
        assert_eq!(ids_at(7, 7, 9), vec![3]);
        assert!(ids_at(0, 0, 0).is_empty());
    }

    /// Crossing south, matching Route 101 -> Littleroot Town's real
    /// connection shape (`data/maps/Route101/map.json`: a south connection
    /// to Littleroot Town at `offset: 0`), verified against a small
    /// synthetic two-map graph rather than the real 518-map table (per the
    /// "engine tests use synthetic data" convention).
    #[test]
    fn resolve_connection_lands_at_neighbours_opposite_edge() {
        let connections: &'static [MapConnection] = Box::leak(Box::new([MapConnection {
            direction: assets::Direction::South,
            offset: 0,
            target: MapId("MAP_SOUTH_NEIGHBOR"),
        }]));
        let hdr = header("MAP_NORTH", connections);
        let events = empty_events("MAP_NORTH");
        let bytes = grid_bytes(10, 10, cell(0, 0, 0));
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_NORTH"),
            name: "MapNorth",
            width: 10,
            height: 10,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&bytes).unwrap();
        let runtime = MapRuntime::new(
            MapId("MAP_NORTH"),
            Box::leak(Box::new(hdr)),
            Box::leak(Box::new(events)),
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );

        let dims = |map: MapId| -> Option<(u16, u16)> {
            if map == MapId("MAP_SOUTH_NEIGHBOR") {
                Some((10, 12))
            } else {
                None
            }
        };

        // Stepping south off row 10 (one past height-1=9) at x=3 lands at
        // the neighbour's north edge (y=0), same x (offset 0).
        let crossing = runtime
            .resolve_connection(Direction::South, 3, 10, &dims)
            .expect("south connection should resolve");
        assert_eq!(crossing.target, MapId("MAP_SOUTH_NEIGHBOR"));
        assert_eq!(crossing.position, (3, 0));

        // No connection covers north/west/east from this header.
        assert!(runtime
            .resolve_connection(Direction::North, 3, -1, &dims)
            .is_none());
    }

    #[test]
    fn resolve_connection_applies_perpendicular_offset() {
        let connections: &'static [MapConnection] = Box::leak(Box::new([MapConnection {
            direction: assets::Direction::East,
            offset: -2,
            target: MapId("MAP_EAST_NEIGHBOR"),
        }]));
        let hdr = header("MAP_WEST", connections);
        let events = empty_events("MAP_WEST");
        let bytes = grid_bytes(6, 6, cell(0, 0, 0));
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_WEST"),
            name: "MapWest",
            width: 6,
            height: 6,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&bytes).unwrap();
        let runtime = MapRuntime::new(
            MapId("MAP_WEST"),
            Box::leak(Box::new(hdr)),
            Box::leak(Box::new(events)),
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );

        let dims = |map: MapId| -> Option<(u16, u16)> {
            if map == MapId("MAP_EAST_NEIGHBOR") {
                Some((10, 10))
            } else {
                None
            }
        };

        // Stepping east off x=6 (width) at y=5: target_y = y - offset = 5 -
        // (-2) = 7, landing at the neighbour's west edge (x=0).
        let crossing = runtime
            .resolve_connection(Direction::East, 6, 5, &dims)
            .expect("east connection should resolve");
        assert_eq!(crossing.position, (0, 7));
    }

    #[test]
    fn resolve_connection_rejects_position_outside_target_bounds() {
        let connections: &'static [MapConnection] = Box::leak(Box::new([MapConnection {
            direction: assets::Direction::South,
            offset: 0,
            target: MapId("MAP_SOUTH_NEIGHBOR"),
        }]));
        let hdr = header("MAP_NORTH2", connections);
        let events = empty_events("MAP_NORTH2");
        let bytes = grid_bytes(20, 5, cell(0, 0, 0));
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_NORTH2"),
            name: "MapNorth2",
            width: 20,
            height: 5,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&bytes).unwrap();
        let runtime = MapRuntime::new(
            MapId("MAP_NORTH2"),
            Box::leak(Box::new(hdr)),
            Box::leak(Box::new(events)),
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );

        // Neighbour is much narrower than this map: only x in 0..5 lands.
        let dims = |_: MapId| Some((5, 5));
        assert!(runtime
            .resolve_connection(Direction::South, 0, 5, &dims)
            .is_some());
        assert!(runtime
            .resolve_connection(Direction::South, 15, 5, &dims)
            .is_none());
    }

    #[test]
    fn connected_map_data_blanket_impl_accepts_a_dimensions_closure() {
        let dims = |map: MapId| -> Option<(u16, u16)> {
            if map == MapId("MAP_X") {
                Some((4, 4))
            } else {
                None
            }
        };
        assert_eq!(dims.dimensions(MapId("MAP_X")), Some((4, 4)));
        assert_eq!(dims.dimensions(MapId("MAP_Y")), None);
    }
}
