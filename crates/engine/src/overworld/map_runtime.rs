//! Runtime queries over one map's decoded layout and events.
//!
//! [`MapRuntime`] borrows one map at a time. Callers replace it when entering
//! another map and supply connected-map data through [`ConnectedMapData`].

use assets::{
    CoordEvent, LayoutGrid, MapConnection, MapEvents, MapHeader, MapId, MetatileAttributeTable,
    MetatileCell, ObjectEvent, WarpEvent,
};

use super::direction::Direction;

/// Number of metatiles whose attributes belong to the primary tileset.
pub const NUM_METATILES_IN_PRIMARY: u16 = 512;

/// A resolved transition into a connected map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectionCrossing {
    /// The neighbouring map the crossing enters.
    pub target: MapId,
    /// The landing tile position within `target`'s own coordinate space.
    pub position: (i32, i32),
}

/// Provides geometry and cells for maps connected to the current runtime.
///
/// A dimensions-only implementation may leave [`Self::metatile_cell`] at its
/// default. Player movement then refuses the crossing because it cannot
/// validate the landing cell.
pub trait ConnectedMapData {
    /// Returns `map`'s dimensions in metatiles.
    fn dimensions(&self, map: MapId) -> Option<(u16, u16)>;

    /// Returns the decoded cell at `(x, y)` in `map` when available.
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

/// Borrowed decoded data and events for one map.
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
    /// Binds decoded data and event tables to `id`.
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

    /// Returns the bound map id.
    #[must_use]
    pub const fn id(&self) -> MapId {
        self.id
    }

    /// Returns the bound map header.
    #[must_use]
    pub const fn header(&self) -> &'a MapHeader {
        self.header
    }

    /// Returns the bound map events.
    #[must_use]
    pub const fn events(&self) -> &'a MapEvents {
        self.events
    }

    /// Returns the decoded grid cell at `(x, y)` when it is inside this map.
    #[must_use]
    pub fn metatile_cell(&self, x: i32, y: i32) -> Option<MetatileCell> {
        let x = u16::try_from(x).ok()?;
        let y = u16::try_from(y).ok()?;
        self.grid.cell_at(x, y)
    }

    /// Returns the elevation assigned to a newly placed player at `(x, y)`.
    ///
    /// A multi-level cell becomes [`super::collision::ELEVATION_TRANSITION`].
    /// Upstream leaves a live object's elevation unchanged if either its
    /// current or previous cell is multi-level. A newly spawned player's two
    /// positions are equal and its initial elevation is the transition value,
    /// so this substitution preserves that result
    /// (`event_object_movement.c:1309-1312,7759-7771`).
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

    /// Returns the metatile behavior at `(x, y)` when its cell and attribute
    /// decode successfully.
    ///
    /// Metatile ids below [`NUM_METATILES_IN_PRIMARY`] use the primary
    /// attribute table. Higher ids use the secondary table after rebasing.
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

    /// Returns static object events at `(x, y, elevation)` in declaration
    /// order.
    ///
    /// Either the event or query elevation may be
    /// [`super::collision::ELEVATION_TRANSITION`]. This query does not filter
    /// hidden templates; [`super::object_event::visible_object_event_at`]
    /// composes visibility with the ordered scan.
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

    /// Returns the first warp at `(x, y, elevation)`.
    ///
    /// A stored [`super::collision::ELEVATION_TRANSITION`] matches any query
    /// elevation; a transition query does not match an ordinary stored value.
    #[must_use]
    pub fn warp_event_at(&self, x: i32, y: i32, elevation: u8) -> Option<&'static WarpEvent> {
        self.events.warp_events.iter().find(|w| {
            i32::from(w.x) == x
                && i32::from(w.y) == y
                && (w.elevation == elevation
                    || w.elevation == super::collision::ELEVATION_TRANSITION)
        })
    }

    /// Returns coordinate events at `(x, y, elevation)` in declaration order.
    ///
    /// A stored [`super::collision::ELEVATION_TRANSITION`] matches any query
    /// elevation; a transition query does not match an ordinary stored value.
    /// The iterator exposes every candidate because event evaluation may
    /// continue past weather, immediate, or state-mismatched events before a
    /// later event supplies a script (`field_control_avatar.c:877-916`).
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

    /// Resolves an off-grid step against this map's connections.
    ///
    /// Connection offsets translate the perpendicular axis, and the crossed
    /// axis lands at the target's opposite edge. Upstream delays the map swap
    /// within its camera's padded backing grid; this runtime has neither, so
    /// it resolves as soon as a step leaves the decoded grid
    /// (`fieldmap.c:578-622,691-742`).
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
    use super::super::collision::{ELEVATION_MULTI_LEVEL, ELEVATION_TRANSITION};
    use super::*;
    use assets::{BattleScene, CoordEventKind, CoordWeather, MapType, RegionMapSectionId, Weather};

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

    #[test]
    fn arrival_elevation_substitutes_transition_for_multi_level() {
        const ORDINARY_ELEVATION: u8 = 3;

        let mut bytes = grid_bytes(2, 1, cell(0, 0, ORDINARY_ELEVATION));
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
            Some(ORDINARY_ELEVATION),
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
        const PRIMARY_BEHAVIOR: u8 = 5;
        const SECONDARY_BEHAVIOR: u8 = 9;

        let bytes = {
            let primary_cell = cell(0, 0, ELEVATION_TRANSITION);
            let secondary_cell = cell(NUM_METATILES_IN_PRIMARY, 0, ELEVATION_TRANSITION);
            [primary_cell.to_le_bytes(), secondary_cell.to_le_bytes()].concat()
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

        let primary_attr_bytes = u16::from(PRIMARY_BEHAVIOR).to_le_bytes();
        let secondary_attr_bytes = u16::from(SECONDARY_BEHAVIOR).to_le_bytes();
        let runtime = MapRuntime::new(
            MapId("MAP_B"),
            Box::leak(Box::new(hdr)),
            Box::leak(Box::new(events)),
            grid,
            MetatileAttributeTable::new(&primary_attr_bytes),
            MetatileAttributeTable::new(&secondary_attr_bytes),
        );

        assert_eq!(runtime.metatile_behavior(0, 0), Some(PRIMARY_BEHAVIOR));
        assert_eq!(runtime.metatile_behavior(1, 0), Some(SECONDARY_BEHAVIOR));
    }

    #[test]
    fn warp_event_at_matches_position_and_elevation_wildcard() {
        const ORDINARY_ELEVATION: u8 = 2;
        let hdr = header("MAP_C", &[]);
        let warps: &'static [WarpEvent] = Box::leak(Box::new([
            WarpEvent {
                x: 3,
                y: 4,
                elevation: ELEVATION_TRANSITION,
                dest_map: assets::WarpDestination::Map(MapId("MAP_D")),
                dest_warp_id: assets::WarpId::Fixed(0),
            },
            WarpEvent {
                x: 5,
                y: 5,
                elevation: ORDINARY_ELEVATION,
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
        let bytes = grid_bytes(8, 8, cell(0, 0, ELEVATION_TRANSITION));
        let grid = layout.grid(&bytes).unwrap();
        let runtime = MapRuntime::new(
            MapId("MAP_C"),
            Box::leak(Box::new(hdr)),
            events,
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );

        assert_eq!(
            runtime.warp_event_at(3, 4, 7).map(|w| w.dest_warp_id),
            Some(assets::WarpId::Fixed(0))
        );
        assert_eq!(runtime.warp_event_at(5, 5, 3), None);
        assert_eq!(
            runtime
                .warp_event_at(5, 5, ORDINARY_ELEVATION)
                .map(|w| w.dest_warp_id),
            Some(assets::WarpId::Fixed(1))
        );
        assert_eq!(runtime.warp_event_at(0, 0, ELEVATION_TRANSITION), None);
    }

    #[test]
    fn coord_events_at_uses_only_stored_transition_as_elevation_wildcard() {
        const DIFFERENT_ELEVATION: u8 = 9;
        const ORDINARY_ELEVATION: u8 = 3;
        let hdr = header("MAP_E", &[]);
        let coords: &'static [CoordEvent] = Box::leak(Box::new([
            CoordEvent {
                x: 2,
                y: 2,
                elevation: ELEVATION_TRANSITION,
                kind: CoordEventKind::Weather(CoordWeather::Rain),
            },
            CoordEvent {
                x: 3,
                y: 3,
                elevation: ORDINARY_ELEVATION,
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
        let bytes = grid_bytes(4, 4, cell(0, 0, ELEVATION_TRANSITION));
        let grid = layout.grid(&bytes).unwrap();
        let runtime = MapRuntime::new(
            MapId("MAP_E"),
            Box::leak(Box::new(hdr)),
            events,
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );

        assert!(runtime
            .coord_events_at(2, 2, DIFFERENT_ELEVATION)
            .next()
            .is_some());
        assert!(runtime
            .coord_events_at(2, 3, ELEVATION_TRANSITION)
            .next()
            .is_none());
        assert!(
            runtime
                .coord_events_at(3, 3, ORDINARY_ELEVATION)
                .next()
                .is_some(),
            "an ordinary stored elevation must match the same query elevation"
        );
        assert!(
            runtime
                .coord_events_at(3, 3, ELEVATION_TRANSITION)
                .next()
                .is_none(),
            "a transition query is not a wildcard"
        );
    }

    fn runtime_with_real_events_and_open_grid(
        map: MapId,
        width: u16,
        height: u16,
    ) -> MapRuntime<'static> {
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

    #[test]
    fn littleroot_trigger_stack_is_ordered_and_allows_state_selection() {
        const LITTLEROOT_STATE_AFTER_FIRST_TRIGGER: u16 = 1;
        let runtime = runtime_with_real_events_and_open_grid(MapId("MAP_LITTLEROOT_TOWN"), 20, 20);

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

        let state = LITTLEROOT_STATE_AFTER_FIRST_TRIGGER;
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

    #[test]
    fn jagged_pass_weather_precedes_the_trigger_without_hiding_it() {
        let runtime = runtime_with_real_events_and_open_grid(MapId("MAP_JAGGED_PASS"), 32, 32);

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
                elevation: ELEVATION_TRANSITION,
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
        assert_eq!(ids_at(6, 6, ELEVATION_TRANSITION), vec![1, 2]);
        assert_eq!(ids_at(7, 7, 9), vec![3]);
        assert!(ids_at(0, 0, ELEVATION_TRANSITION).is_empty());
    }

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

        let step_beyond_south_edge = (3, 10);
        let crossing = runtime
            .resolve_connection(
                Direction::South,
                step_beyond_south_edge.0,
                step_beyond_south_edge.1,
                &dims,
            )
            .expect("south connection should resolve");
        assert_eq!(crossing.target, MapId("MAP_SOUTH_NEIGHBOR"));
        assert_eq!(crossing.position, (3, 0));

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

        let step_beyond_east_edge = (6, 5);
        let crossing = runtime
            .resolve_connection(
                Direction::East,
                step_beyond_east_edge.0,
                step_beyond_east_edge.1,
                &dims,
            )
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

        let dims = |_: MapId| Some((5, 5));
        let in_bounds_target_x = 0;
        let out_of_bounds_target_x = 15;
        assert!(runtime
            .resolve_connection(Direction::South, in_bounds_target_x, 5, &dims)
            .is_some());
        assert!(runtime
            .resolve_connection(Direction::South, out_of_bounds_target_x, 5, &dims)
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
