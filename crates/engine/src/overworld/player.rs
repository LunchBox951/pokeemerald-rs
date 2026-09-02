//! On-foot player movement, collision, connection crossings, and tile pacing.

use assets::{MapId, MetatileCell};

use super::collision::{directionally_impassable, elevation_mismatch, Collision};
use super::direction::Direction;
use super::map_runtime::{ConnectedMapData, MapRuntime};
use super::metatile_behavior::MB_NORMAL;
use super::object_event::visible_object_event_at;
use crate::event_data::EventData;

/// Frames required for a normal on-foot step to cross one tile.
pub const WALK_FRAMES_PER_TILE: u8 = 16;

/// A tile position in the active map's coordinate space.
pub type TilePos = (i32, i32);

/// The player's tile position, facing, elevation, and step progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerState {
    position: TilePos,
    collision_elevation: u8,
    render_elevation: u8,
    facing: Direction,
    movement_streak_active: bool,
    transit_frames: Option<u8>,
}

/// The result of one directional-input poll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepOutcome {
    /// No input was applied, or the current tile crossing is still in progress.
    Idle,
    /// The player faced a new direction without changing tiles.
    Turned(Direction),
    /// Collision prevented the attempted step.
    Blocked {
        /// The attempted direction.
        direction: Direction,
        /// Why the attempted step was denied.
        collision: super::collision::Collision,
    },
    /// The player entered an adjacent tile on the current map.
    Advanced {
        /// The tile departed.
        from: TilePos,
        /// The tile entered.
        to: TilePos,
    },
    /// The player entered an adjacent map through a connection.
    ///
    /// The caller must bind its runtime to `to_map` before the next input poll.
    /// The landing cell has already been checked and its elevation adopted.
    Crossed {
        /// The connected map entered.
        to_map: MapId,
        /// The landing tile in the connected map's coordinate space.
        to_position: TilePos,
    },
}

#[derive(Debug, Clone, Copy)]
struct Landing {
    position: TilePos,
    cell: MetatileCell,
    destination_behavior: u8,
    object_events_accessible: bool,
}

impl Landing {
    fn on_current_map(position: TilePos, cell: MetatileCell, destination_behavior: u8) -> Self {
        Self {
            position,
            cell,
            destination_behavior,
            object_events_accessible: true,
        }
    }

    fn across_connection(position: TilePos, cell: MetatileCell) -> Self {
        // ConnectedMapData exposes neighbour cells, but not behaviour
        // attributes or object events.
        Self {
            position,
            cell,
            destination_behavior: MB_NORMAL,
            object_events_accessible: false,
        }
    }
}

impl PlayerState {
    /// Creates a stationary player on `position`.
    ///
    /// Collision and render elevations both start at `elevation`.
    #[must_use]
    pub const fn new(position: TilePos, elevation: u8, facing: Direction) -> Self {
        Self {
            position,
            collision_elevation: elevation,
            render_elevation: elevation,
            facing,
            movement_streak_active: false,
            transit_frames: None,
        }
    }

    /// Returns the current tile position.
    #[must_use]
    pub const fn position(&self) -> TilePos {
        self.position
    }

    /// Returns the elevation used for collision checks.
    #[must_use]
    pub const fn elevation(&self) -> u8 {
        self.collision_elevation
    }

    /// Returns the last non-transition elevation used for render priority.
    #[must_use]
    pub const fn previous_elevation(&self) -> u8 {
        self.render_elevation
    }

    /// Returns the current facing direction.
    #[must_use]
    pub const fn facing(&self) -> Direction {
        self.facing
    }

    /// Changes facing without starting or interrupting a step.
    pub const fn face(&mut self, direction: Direction) {
        self.facing = direction;
    }

    /// Returns frames elapsed in the current tile crossing, or zero at rest.
    #[must_use]
    pub const fn step_progress(&self) -> u8 {
        match self.transit_frames {
            Some(frames) => frames,
            None => 0,
        }
    }

    /// Returns whether a tile crossing is still in progress.
    #[must_use]
    pub const fn in_transit(&self) -> bool {
        self.transit_frames.is_some()
    }

    /// Advances an active tile crossing by one frame; a stationary player remains stationary.
    pub fn tick(&mut self) {
        if let Some(frames) = self.transit_frames.as_mut() {
            *frames += 1;
            if *frames >= WALK_FRAMES_PER_TILE {
                self.transit_frames = None;
            }
        }
    }

    fn adopt_elevation(&mut self, origin_elevation: u8, destination_elevation: u8) {
        if origin_elevation == super::collision::ELEVATION_MULTI_LEVEL
            || destination_elevation == super::collision::ELEVATION_MULTI_LEVEL
        {
            return;
        }
        self.collision_elevation = destination_elevation;
        if destination_elevation != super::collision::ELEVATION_TRANSITION {
            self.render_elevation = destination_elevation;
        }
    }

    /// Applies one directional-input poll.
    ///
    /// Input is ignored during a tile crossing.
    ///
    /// # Turn vs. step
    ///
    /// A direction change turns in place unless a movement streak is active.
    /// Every attempted step starts or continues that streak, including a blocked
    /// attempt. An accepted poll without directional input ends the streak.
    ///
    /// # Collision
    ///
    /// Same-map steps test impassability, elevation, then visible object
    /// occupancy. Connection landings omit neighbouring behaviour attributes and
    /// object events because [`ConnectedMapData`] does not expose them.
    /// A blocked attempt still leaves the player facing the attempted direction.
    ///
    /// # Elevation adoption
    ///
    /// A successful landing adopts its elevation for collision. Render elevation
    /// changes only for a non-transition landing. If either endpoint is
    /// multi-level, both elevation values remain unchanged.
    pub fn step(
        &mut self,
        input: Option<Direction>,
        runtime: &MapRuntime<'_>,
        maps: &impl ConnectedMapData,
        event_data: &EventData,
    ) -> StepOutcome {
        if self.in_transit() {
            return StepOutcome::Idle;
        }

        let Some(direction) = input else {
            self.movement_streak_active = false;
            return StepOutcome::Idle;
        };

        if direction != self.facing && !self.movement_streak_active {
            self.facing = direction;
            return StepOutcome::Turned(direction);
        }

        self.movement_streak_active = true;
        self.facing = direction;

        let (dx, dy) = direction.delta();
        let target = (self.position.0 + dx, self.position.1 + dy);

        let standing_behavior = runtime
            .metatile_behavior(self.position.0, self.position.1)
            .unwrap_or(MB_NORMAL);

        if let Some(cell) = runtime.metatile_cell(target.0, target.1) {
            let target_behavior = runtime
                .metatile_behavior(target.0, target.1)
                .unwrap_or(MB_NORMAL);
            let from = self.position;
            let landing = Landing::on_current_map(target, cell, target_behavior);
            return match self.try_start_resolved_step(
                direction,
                runtime,
                event_data,
                standing_behavior,
                landing,
            ) {
                Ok(()) => StepOutcome::Advanced { from, to: target },
                Err(collision) => StepOutcome::Blocked {
                    direction,
                    collision,
                },
            };
        }

        if let Some(crossing) = runtime.resolve_connection(direction, target.0, target.1, maps) {
            let Some(cell) =
                maps.metatile_cell(crossing.target, crossing.position.0, crossing.position.1)
            else {
                return StepOutcome::Blocked {
                    direction,
                    collision: Collision::Impassable,
                };
            };
            let landing = Landing::across_connection(crossing.position, cell);
            return match self.try_start_resolved_step(
                direction,
                runtime,
                event_data,
                standing_behavior,
                landing,
            ) {
                Ok(()) => StepOutcome::Crossed {
                    to_map: crossing.target,
                    to_position: crossing.position,
                },
                Err(collision) => StepOutcome::Blocked {
                    direction,
                    collision,
                },
            };
        }

        StepOutcome::Blocked {
            direction,
            collision: Collision::Impassable,
        }
    }

    fn try_start_resolved_step(
        &mut self,
        direction: Direction,
        runtime: &MapRuntime<'_>,
        event_data: &EventData,
        standing_behavior: u8,
        landing: Landing,
    ) -> Result<(), Collision> {
        if landing.cell.collision != 0
            || directionally_impassable(standing_behavior, landing.destination_behavior, direction)
        {
            return Err(Collision::Impassable);
        }
        if elevation_mismatch(self.collision_elevation, landing.cell.elevation) {
            return Err(Collision::ElevationMismatch);
        }
        if landing.object_events_accessible
            && visible_object_event_at(
                runtime,
                landing.position.0,
                landing.position.1,
                self.collision_elevation,
                event_data,
            )
            .is_some()
        {
            return Err(Collision::ObjectEvent);
        }

        let origin_elevation = runtime
            .metatile_cell(self.position.0, self.position.1)
            .map_or(self.collision_elevation, |origin_cell| {
                origin_cell.elevation
            });
        self.position = landing.position;
        self.adopt_elevation(origin_elevation, landing.cell.elevation);
        self.transit_frames = Some(0);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overworld::map_runtime::MapRuntime;
    use crate::overworld::metatile_behavior::MB_IMPASSABLE_SOUTH_AND_NORTH;
    use assets::{
        BattleScene, MapConnection, MapEvents, MapHeader, MapType, MetatileAttributeTable,
        MetatileCell, ObjectEvent, RegionMapSectionId, Weather,
    };

    const NO_FLAGS: EventData = EventData::new();

    fn flat_runtime(
        width: u16,
        height: u16,
        collision_at: impl Fn(u16, u16) -> u8,
    ) -> (Vec<u8>, MapHeader, MapEvents) {
        let mut bytes = Vec::with_capacity(usize::from(width) * usize::from(height) * 2);
        for y in 0..height {
            for x in 0..width {
                let raw = MetatileCell {
                    metatile_id: 1,
                    collision: collision_at(x, y),
                    elevation: 3,
                }
                .pack();
                bytes.extend_from_slice(&raw.to_le_bytes());
            }
        }
        let header = MapHeader {
            id: assets::MapId("MAP_TEST"),
            group: 0,
            num: 0,
            name: "MapTest",
            layout: assets::LayoutId("MAP_TEST"),
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
            connections: &[] as &'static [MapConnection],
        };
        let events = MapEvents {
            id: assets::MapId("MAP_TEST"),
            shared_events_map: None,
            object_events: &[],
            warp_events: &[],
            coord_events: &[],
            bg_events: &[],
        };
        (bytes, header, events)
    }

    fn no_connections(_: MapId) -> Option<(u16, u16)> {
        None
    }

    #[derive(Debug, Clone, Copy)]
    struct SingleConnectedMap {
        id: MapId,
        dimensions: (u16, u16),
        landing_position: TilePos,
        landing_cell: MetatileCell,
    }

    impl ConnectedMapData for SingleConnectedMap {
        fn dimensions(&self, map: MapId) -> Option<(u16, u16)> {
            (map == self.id).then_some(self.dimensions)
        }

        fn metatile_cell(&self, map: MapId, x: i32, y: i32) -> Option<MetatileCell> {
            (map == self.id && (x, y) == self.landing_position).then_some(self.landing_cell)
        }
    }

    fn south_connected_runtime() -> MapRuntime<'static> {
        let (bytes, mut header, events) = flat_runtime(5, 5, |_, _| 0);
        header.connections = &[MapConnection {
            direction: assets::Direction::South,
            offset: 0,
            target: MapId("MAP_SOUTH"),
        }];
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 5,
            height: 5,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let bytes = Box::leak(bytes.into_boxed_slice());
        let header = Box::leak(Box::new(header));
        let events = Box::leak(Box::new(events));
        MapRuntime::new(
            MapId("MAP_TEST"),
            header,
            events,
            layout.grid(bytes).unwrap(),
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        )
    }

    #[test]
    fn a_scripted_face_turns_the_player_without_moving_or_disturbing_a_step() {
        let mut player = PlayerState::new((2, 2), 3, Direction::South);
        player.face(Direction::North);
        assert_eq!(player.facing(), Direction::North);
        assert_eq!(player.position(), (2, 2));
        assert!(!player.in_transit());

        let (bytes, header, events) = flat_runtime(5, 5, |_, _| 0);
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 5,
            height: 5,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&bytes).unwrap();
        let runtime = MapRuntime::new(
            assets::MapId("MAP_TEST"),
            &header,
            &events,
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );
        let mut player = PlayerState::new((2, 2), 3, Direction::South);
        player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS);
        let progress = player.step_progress();
        player.face(Direction::West);
        assert_eq!(player.facing(), Direction::West);
        assert_eq!(player.position(), (2, 3), "the committed step is untouched");
        assert!(player.in_transit());
        assert_eq!(player.step_progress(), progress);
    }

    #[test]
    fn fresh_pressing_the_facing_direction_steps_immediately() {
        let (bytes, header, events) = flat_runtime(5, 5, |_, _| 0);
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 5,
            height: 5,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&bytes).unwrap();
        let runtime = MapRuntime::new(
            assets::MapId("MAP_TEST"),
            &header,
            &events,
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );

        let mut player = PlayerState::new((2, 2), 3, Direction::South);
        let outcome = player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS);
        assert_eq!(
            outcome,
            StepOutcome::Advanced {
                from: (2, 2),
                to: (2, 3),
            }
        );
        assert_eq!(player.position(), (2, 3));
        assert!(player.in_transit());
    }

    #[test]
    fn pressing_a_new_direction_from_standstill_turns_without_stepping() {
        let (bytes, header, events) = flat_runtime(5, 5, |_, _| 0);
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 5,
            height: 5,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&bytes).unwrap();
        let runtime = MapRuntime::new(
            assets::MapId("MAP_TEST"),
            &header,
            &events,
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );

        let mut player = PlayerState::new((2, 2), 3, Direction::South);
        let outcome = player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS);
        assert_eq!(outcome, StepOutcome::Turned(Direction::East));
        assert_eq!(
            player.position(),
            (2, 2),
            "a turn must not move the tile position"
        );
        assert_eq!(player.facing(), Direction::East);
        assert!(!player.in_transit());

        let outcome = player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS);
        assert_eq!(
            outcome,
            StepOutcome::Advanced {
                from: (2, 2),
                to: (3, 2),
            }
        );
    }

    #[test]
    fn changing_direction_mid_movement_steps_immediately_without_a_turn_frame() {
        let (bytes, header, events) = flat_runtime(5, 5, |_, _| 0);
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 5,
            height: 5,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&bytes).unwrap();
        let runtime = MapRuntime::new(
            assets::MapId("MAP_TEST"),
            &header,
            &events,
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );

        let mut player = PlayerState::new((2, 2), 3, Direction::South);
        assert!(matches!(
            player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Advanced { .. }
        ));
        for _ in 0..WALK_FRAMES_PER_TILE {
            player.tick();
        }
        assert!(!player.in_transit());

        let outcome = player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS);
        assert_eq!(
            outcome,
            StepOutcome::Advanced {
                from: (2, 3),
                to: (3, 3),
            }
        );
    }

    #[test]
    fn release_during_transit_does_not_end_the_movement_streak() {
        let (bytes, header, events) = flat_runtime(5, 5, |_, _| 0);
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 5,
            height: 5,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&bytes).unwrap();
        let runtime = MapRuntime::new(
            assets::MapId("MAP_TEST"),
            &header,
            &events,
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );

        let mut player = PlayerState::new((2, 2), 3, Direction::South);
        assert!(matches!(
            player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Advanced { .. }
        ));

        for _ in 0..WALK_FRAMES_PER_TILE {
            assert_eq!(
                player.step(None, &runtime, &no_connections, &NO_FLAGS),
                StepOutcome::Idle
            );
            player.tick();
        }

        let outcome = player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS);
        assert_eq!(
            outcome,
            StepOutcome::Advanced {
                from: (2, 3),
                to: (3, 3),
            }
        );
    }

    #[test]
    fn releasing_input_resets_to_not_moving_so_the_next_direction_turns_first() {
        let (bytes, header, events) = flat_runtime(5, 5, |_, _| 0);
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 5,
            height: 5,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&bytes).unwrap();
        let runtime = MapRuntime::new(
            assets::MapId("MAP_TEST"),
            &header,
            &events,
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );

        let mut player = PlayerState::new((2, 2), 3, Direction::South);
        assert!(matches!(
            player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Advanced { .. }
        ));
        for _ in 0..WALK_FRAMES_PER_TILE {
            player.tick();
        }
        assert_eq!(
            player.step(None, &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Idle
        );
        let outcome = player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS);
        assert_eq!(outcome, StepOutcome::Turned(Direction::East));
    }

    #[test]
    fn in_transit_step_calls_are_a_no_op() {
        let (bytes, header, events) = flat_runtime(5, 5, |_, _| 0);
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 5,
            height: 5,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&bytes).unwrap();
        let runtime = MapRuntime::new(
            assets::MapId("MAP_TEST"),
            &header,
            &events,
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );

        let mut player = PlayerState::new((2, 2), 3, Direction::South);
        assert!(matches!(
            player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Advanced { .. }
        ));
        assert_eq!(
            player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Idle
        );
        assert_eq!(
            player.position(),
            (2, 3),
            "position must not change while busy"
        );
    }

    #[test]
    fn collision_bit_blocks_the_step_and_leaves_position_unchanged() {
        let (bytes, header, events) = flat_runtime(5, 5, |_, y| u8::from(y == 3));
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 5,
            height: 5,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&bytes).unwrap();
        let runtime = MapRuntime::new(
            assets::MapId("MAP_TEST"),
            &header,
            &events,
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );

        let mut player = PlayerState::new((2, 2), 3, Direction::South);
        let outcome = player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS);
        assert_eq!(
            outcome,
            StepOutcome::Blocked {
                direction: Direction::South,
                collision: super::super::collision::Collision::Impassable,
            }
        );
        assert_eq!(player.position(), (2, 2));
        assert!(
            !player.in_transit(),
            "a blocked step must not start a transition"
        );
    }

    #[test]
    fn elevation_mismatch_blocks_the_step() {
        let width = 5u16;
        let height = 5u16;
        let mut bytes = Vec::with_capacity(usize::from(width) * usize::from(height) * 2);
        for y in 0..height {
            for x in 0..width {
                let elevation = if x == 2 && y == 3 { 7 } else { 3 };
                let raw = MetatileCell {
                    metatile_id: 1,
                    collision: 0,
                    elevation,
                }
                .pack();
                bytes.extend_from_slice(&raw.to_le_bytes());
            }
        }
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width,
            height,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&bytes).unwrap();
        let header = MapHeader {
            id: assets::MapId("MAP_TEST"),
            group: 0,
            num: 0,
            name: "MapTest",
            layout: assets::LayoutId("MAP_TEST"),
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
            connections: &[] as &'static [MapConnection],
        };
        let events = MapEvents {
            id: assets::MapId("MAP_TEST"),
            shared_events_map: None,
            object_events: &[],
            warp_events: &[],
            coord_events: &[],
            bg_events: &[],
        };
        let runtime = MapRuntime::new(
            assets::MapId("MAP_TEST"),
            &header,
            &events,
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );

        let mut player = PlayerState::new((2, 2), 3, Direction::South);
        let outcome = player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS);
        assert_eq!(
            outcome,
            StepOutcome::Blocked {
                direction: Direction::South,
                collision: super::super::collision::Collision::ElevationMismatch,
            }
        );
        assert_eq!(player.position(), (2, 2));
    }

    fn bed_pillow_runtime(pillow_elevation: u8) -> MapRuntime<'static> {
        const WIDTH: u16 = 3;
        const HEIGHT: u16 = 8;
        const PILLOW: (u16, u16) = (1, 4);

        let mut bytes = Vec::with_capacity(usize::from(WIDTH) * usize::from(HEIGHT) * 2);
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let is_pillow = (x, y) == PILLOW;
                let raw = MetatileCell {
                    metatile_id: u16::from(is_pillow),
                    collision: 0,
                    elevation: if is_pillow { pillow_elevation } else { 3 },
                }
                .pack();
                bytes.extend_from_slice(&raw.to_le_bytes());
            }
        }

        let attrs = [
            u16::from(MB_NORMAL).to_le_bytes(),
            u16::from(MB_IMPASSABLE_SOUTH_AND_NORTH).to_le_bytes(),
        ]
        .concat();

        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: WIDTH,
            height: HEIGHT,
            primary_tileset: "gTileset_Building",
            secondary_tileset: "gTileset_BrendansMaysHouse",
        };
        let (_, header, events) = flat_runtime(1, 1, |_, _| 0);
        let bytes = Box::leak(bytes.into_boxed_slice());
        let attrs = Box::leak(attrs.into_boxed_slice());
        let header = Box::leak(Box::new(header));
        let events = Box::leak(Box::new(events));
        MapRuntime::new(
            assets::MapId("MAP_TEST"),
            header,
            events,
            layout.grid(bytes).unwrap(),
            MetatileAttributeTable::new(attrs),
            MetatileAttributeTable::new(&[]),
        )
    }

    #[test]
    fn the_beds_pillow_tile_cannot_be_left_northward() {
        let runtime = bed_pillow_runtime(3);
        let mut player = PlayerState::new((1, 4), 3, Direction::North);

        let outcome = player.step(Some(Direction::North), &runtime, &no_connections, &NO_FLAGS);
        assert_eq!(
            outcome,
            StepOutcome::Blocked {
                direction: Direction::North,
                collision: super::super::collision::Collision::Impassable,
            }
        );
        assert_eq!(player.position(), (1, 4));
        assert!(
            !player.in_transit(),
            "a blocked step must not start a transition"
        );
    }

    #[test]
    fn the_beds_pillow_tile_cannot_be_entered_from_the_north() {
        let runtime = bed_pillow_runtime(3);
        let mut player = PlayerState::new((1, 3), 3, Direction::South);

        let outcome = player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS);
        assert_eq!(
            outcome,
            StepOutcome::Blocked {
                direction: Direction::South,
                collision: super::super::collision::Collision::Impassable,
            }
        );
        assert_eq!(player.position(), (1, 3));
    }

    #[test]
    fn the_beds_pillow_tile_stays_crossable_east_to_west() {
        let runtime = bed_pillow_runtime(3);

        let mut player = PlayerState::new((0, 4), 3, Direction::East);
        assert_eq!(
            player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Advanced {
                from: (0, 4),
                to: (1, 4),
            }
        );

        let mut player = PlayerState::new((1, 4), 3, Direction::East);
        assert_eq!(
            player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Advanced {
                from: (1, 4),
                to: (2, 4),
            }
        );
    }

    #[test]
    fn directional_impassability_outranks_the_elevation_mismatch() {
        let runtime = bed_pillow_runtime(7);
        let mut player = PlayerState::new((1, 3), 3, Direction::South);

        assert_eq!(
            player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Blocked {
                direction: Direction::South,
                collision: super::super::collision::Collision::Impassable,
            }
        );
    }

    #[test]
    fn transition_tile_lets_the_next_step_cross_between_elevations() {
        let width = 5u16;
        let height = 5u16;
        let mut bytes = Vec::with_capacity(usize::from(width) * usize::from(height) * 2);
        for y in 0..height {
            for x in 0..width {
                let elevation = match (x, y) {
                    (2, 2) => 0,
                    (2, 3) => 5,
                    _ => 3,
                };
                let raw = MetatileCell {
                    metatile_id: 1,
                    collision: 0,
                    elevation,
                }
                .pack();
                bytes.extend_from_slice(&raw.to_le_bytes());
            }
        }
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width,
            height,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&bytes).unwrap();
        let header = MapHeader {
            id: assets::MapId("MAP_TEST"),
            group: 0,
            num: 0,
            name: "MapTest",
            layout: assets::LayoutId("MAP_TEST"),
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
            connections: &[] as &'static [MapConnection],
        };
        let events = MapEvents {
            id: assets::MapId("MAP_TEST"),
            shared_events_map: None,
            object_events: &[],
            warp_events: &[],
            coord_events: &[],
            bg_events: &[],
        };
        let runtime = MapRuntime::new(
            assets::MapId("MAP_TEST"),
            &header,
            &events,
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );

        let mut player = PlayerState::new((2, 1), 3, Direction::South);
        assert_eq!(
            player.previous_elevation(),
            3,
            "the constructor must align collision and render elevations"
        );
        let onto_transition =
            player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS);
        assert!(matches!(onto_transition, StepOutcome::Advanced { .. }));
        assert_eq!(player.elevation(), 0);
        assert_eq!(
            player.previous_elevation(),
            3,
            "previous_elevation must retain the last non-transition elevation \
             while standing on the transition wildcard"
        );
        for _ in 0..WALK_FRAMES_PER_TILE {
            player.tick();
        }
        let onto_upper = player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS);
        assert!(matches!(onto_upper, StepOutcome::Advanced { .. }));
        assert_eq!(player.position(), (2, 3));
        assert_eq!(player.elevation(), 5);
        assert_eq!(
            player.previous_elevation(),
            5,
            "landing on a real (non-transition) elevation updates \
             previous_elevation too"
        );
    }

    #[test]
    fn previous_elevation_survives_a_raised_tile_flanked_by_transitions_on_both_sides() {
        fn step_south(player: &mut PlayerState, runtime: &MapRuntime<'_>) {
            let outcome = player.step(Some(Direction::South), runtime, &no_connections, &NO_FLAGS);
            assert!(
                matches!(outcome, StepOutcome::Advanced { .. }),
                "every step down this column is collision-legal: {outcome:?}"
            );
            for _ in 0..WALK_FRAMES_PER_TILE {
                player.tick();
            }
        }

        let width = 5u16;
        let height = 6u16;
        let mut bytes = Vec::with_capacity(usize::from(width) * usize::from(height) * 2);
        for y in 0..height {
            for x in 0..width {
                let elevation = match (x, y) {
                    (2, 2 | 4) => 0,
                    (2, 3) => 4,
                    _ => 3,
                };
                let raw = MetatileCell {
                    metatile_id: 1,
                    collision: 0,
                    elevation,
                }
                .pack();
                bytes.extend_from_slice(&raw.to_le_bytes());
            }
        }
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width,
            height,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&bytes).unwrap();
        let header = MapHeader {
            id: assets::MapId("MAP_TEST"),
            group: 0,
            num: 0,
            name: "MapTest",
            layout: assets::LayoutId("MAP_TEST"),
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
            connections: &[] as &'static [MapConnection],
        };
        let events = MapEvents {
            id: assets::MapId("MAP_TEST"),
            shared_events_map: None,
            object_events: &[],
            warp_events: &[],
            coord_events: &[],
            bg_events: &[],
        };
        let runtime = MapRuntime::new(
            assets::MapId("MAP_TEST"),
            &header,
            &events,
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );

        let mut player = PlayerState::new((2, 1), 3, Direction::South);

        step_south(&mut player, &runtime);
        assert_eq!((player.elevation(), player.previous_elevation()), (0, 3));

        step_south(&mut player, &runtime);
        assert_eq!((player.elevation(), player.previous_elevation()), (4, 4));

        step_south(&mut player, &runtime);
        assert_eq!(
            (player.elevation(), player.previous_elevation()),
            (0, 4),
            "previous_elevation must still read the raised tile's 4, not \
             reset by standing on the wildcard"
        );

        step_south(&mut player, &runtime);
        assert_eq!((player.elevation(), player.previous_elevation()), (3, 3));
    }

    #[test]
    fn a_multi_level_origin_tile_skips_the_elevation_update_even_though_the_destination_is_ordinary(
    ) {
        let width = 5u16;
        let height = 5u16;
        let mut bytes = Vec::with_capacity(usize::from(width) * usize::from(height) * 2);
        for y in 0..height {
            for x in 0..width {
                let elevation = if (x, y) == (2, 2) {
                    super::super::collision::ELEVATION_MULTI_LEVEL
                } else {
                    7
                };
                let raw = MetatileCell {
                    metatile_id: 1,
                    collision: 0,
                    elevation,
                }
                .pack();
                bytes.extend_from_slice(&raw.to_le_bytes());
            }
        }
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width,
            height,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&bytes).unwrap();
        let header = MapHeader {
            id: assets::MapId("MAP_TEST"),
            group: 0,
            num: 0,
            name: "MapTest",
            layout: assets::LayoutId("MAP_TEST"),
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
            connections: &[] as &'static [MapConnection],
        };
        let events = MapEvents {
            id: assets::MapId("MAP_TEST"),
            shared_events_map: None,
            object_events: &[],
            warp_events: &[],
            coord_events: &[],
            bg_events: &[],
        };
        let runtime = MapRuntime::new(
            assets::MapId("MAP_TEST"),
            &header,
            &events,
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );

        let mut player = PlayerState::new((2, 2), 0, Direction::South);
        let outcome = player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS);
        assert!(
            matches!(outcome, StepOutcome::Advanced { .. }),
            "a multi-level tile is never itself a mismatch source: {outcome:?}"
        );
        assert_eq!(
            (player.elevation(), player.previous_elevation()),
            (0, 0),
            "the origin tile's ELEVATION_MULTI_LEVEL must skip the whole \
             adoption -- both fields stay exactly as they were before this \
             step, even though the destination (7) is perfectly ordinary \
             and would otherwise have been adopted into both"
        );
    }

    #[test]
    fn stepping_off_the_edge_with_a_connection_crosses_maps() {
        let runtime = south_connected_runtime();
        let maps = SingleConnectedMap {
            id: MapId("MAP_SOUTH"),
            dimensions: (5, 5),
            landing_position: (2, 0),
            landing_cell: MetatileCell {
                metatile_id: 1,
                collision: 0,
                elevation: 0,
            },
        };

        let mut player = PlayerState::new((2, 4), 3, Direction::South);
        let outcome = player.step(Some(Direction::South), &runtime, &maps, &NO_FLAGS);
        assert_eq!(
            outcome,
            StepOutcome::Crossed {
                to_map: MapId("MAP_SOUTH"),
                to_position: (2, 0),
            }
        );
        assert_eq!(player.position(), (2, 0));
        assert_eq!(player.elevation(), 0);
    }

    #[test]
    fn connected_map_collision_bit_blocks_crossing() {
        let runtime = south_connected_runtime();
        let maps = SingleConnectedMap {
            id: MapId("MAP_SOUTH"),
            dimensions: (5, 5),
            landing_position: (2, 0),
            landing_cell: MetatileCell {
                metatile_id: 1,
                collision: 1,
                elevation: 3,
            },
        };
        let mut player = PlayerState::new((2, 4), 3, Direction::South);

        assert_eq!(
            player.step(Some(Direction::South), &runtime, &maps, &NO_FLAGS),
            StepOutcome::Blocked {
                direction: Direction::South,
                collision: super::super::collision::Collision::Impassable,
            }
        );
        assert_eq!(player.position(), (2, 4));
        assert_eq!(player.elevation(), 3);
    }

    #[test]
    fn connected_map_elevation_mismatch_blocks_crossing() {
        let runtime = south_connected_runtime();
        let maps = SingleConnectedMap {
            id: MapId("MAP_SOUTH"),
            dimensions: (5, 5),
            landing_position: (2, 0),
            landing_cell: MetatileCell {
                metatile_id: 1,
                collision: 0,
                elevation: 4,
            },
        };
        let mut player = PlayerState::new((2, 4), 3, Direction::South);

        assert_eq!(
            player.step(Some(Direction::South), &runtime, &maps, &NO_FLAGS),
            StepOutcome::Blocked {
                direction: Direction::South,
                collision: super::super::collision::Collision::ElevationMismatch,
            }
        );
        assert_eq!(player.position(), (2, 4));
        assert_eq!(player.elevation(), 3);
    }

    #[test]
    fn connection_collision_bit_outranks_elevation_mismatch() {
        let runtime = south_connected_runtime();
        let maps = SingleConnectedMap {
            id: MapId("MAP_SOUTH"),
            dimensions: (5, 5),
            landing_position: (2, 0),
            landing_cell: MetatileCell {
                metatile_id: 1,
                collision: 1,
                elevation: 4,
            },
        };
        let mut player = PlayerState::new((2, 4), 3, Direction::South);

        assert_eq!(
            player.step(Some(Direction::South), &runtime, &maps, &NO_FLAGS),
            StepOutcome::Blocked {
                direction: Direction::South,
                collision: super::super::collision::Collision::Impassable,
            },
            "collision bits must outrank elevation mismatch on a crossing \
             landing, the same order the same-map branch enforces"
        );
        assert_eq!(player.position(), (2, 4));
        assert_eq!(player.elevation(), 3);
    }

    #[test]
    fn connection_crossing_does_not_check_local_object_events() {
        let (bytes, mut header, _) = flat_runtime(5, 5, |_, _| 0);
        header.connections = &[MapConnection {
            direction: assets::Direction::South,
            offset: 0,
            target: MapId("MAP_SOUTH"),
        }];
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 5,
            height: 5,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let objects: &'static [ObjectEvent] = Box::leak(Box::new([object(1, 2, 0, 3, "0")]));
        let events: &'static MapEvents = Box::leak(Box::new(MapEvents {
            id: assets::MapId("MAP_TEST"),
            shared_events_map: None,
            object_events: objects,
            warp_events: &[],
            coord_events: &[],
            bg_events: &[],
        }));
        let bytes = Box::leak(bytes.into_boxed_slice());
        let header = Box::leak(Box::new(header));
        let runtime = MapRuntime::new(
            MapId("MAP_TEST"),
            header,
            events,
            layout.grid(bytes).unwrap(),
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );
        let maps = SingleConnectedMap {
            id: MapId("MAP_SOUTH"),
            dimensions: (5, 5),
            landing_position: (2, 0),
            landing_cell: MetatileCell {
                metatile_id: 1,
                collision: 0,
                elevation: 3,
            },
        };

        let mut player = PlayerState::new((2, 4), 3, Direction::South);
        assert_eq!(
            player.step(Some(Direction::South), &runtime, &maps, &NO_FLAGS),
            StepOutcome::Crossed {
                to_map: MapId("MAP_SOUTH"),
                to_position: (2, 0),
            },
            "a connection crossing must never consult this map's own object events"
        );
    }

    #[test]
    fn stepping_off_the_edge_without_a_connection_is_blocked() {
        let (bytes, header, events) = flat_runtime(5, 5, |_, _| 0);
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 5,
            height: 5,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&bytes).unwrap();
        let runtime = MapRuntime::new(
            assets::MapId("MAP_TEST"),
            &header,
            &events,
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );

        let mut player = PlayerState::new((2, 4), 3, Direction::South);
        let outcome = player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS);
        assert_eq!(
            outcome,
            StepOutcome::Blocked {
                direction: Direction::South,
                collision: super::super::collision::Collision::Impassable,
            }
        );
        assert_eq!(player.position(), (2, 4));
    }

    fn object(local_id: u8, x: i16, y: i16, elevation: u8, flag: &'static str) -> ObjectEvent {
        ObjectEvent {
            local_id,
            graphics_id: "OBJ_EVENT_GFX_MOM",
            x,
            y,
            elevation,
            movement_type: assets::MovementType::FaceDown,
            movement_range_x: 0,
            movement_range_y: 0,
            trainer_type: assets::TrainerType::None,
            trainer_sight_or_berry_tree_id: "0",
            script: "0x0",
            flag,
        }
    }

    fn runtime_with_events(
        events: &'static MapEvents,
        collision_at: impl Fn(u16, u16) -> u8,
    ) -> MapRuntime<'static> {
        let (bytes, header, _) = flat_runtime(10, 10, collision_at);
        let bytes: &'static [u8] = Box::leak(bytes.into_boxed_slice());
        let header: &'static MapHeader = Box::leak(Box::new(header));
        let layout: &'static assets::MapLayout = Box::leak(Box::new(assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 10,
            height: 10,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        }));
        let grid = layout.grid(bytes).unwrap();
        MapRuntime::new(
            assets::MapId("MAP_TEST"),
            header,
            events,
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        )
    }

    fn runtime_with_objects(object_events: &'static [ObjectEvent]) -> MapRuntime<'static> {
        let events: &'static MapEvents = Box::leak(Box::new(MapEvents {
            id: assets::MapId("MAP_TEST"),
            shared_events_map: None,
            object_events,
            warp_events: &[],
            coord_events: &[],
            bg_events: &[],
        }));
        runtime_with_events(events, |_, _| 0)
    }

    #[test]
    fn a_visible_object_event_blocks_the_step_and_leaves_the_player_facing_it() {
        let objects: &'static [ObjectEvent] = Box::leak(Box::new([object(1, 2, 3, 3, "0")]));
        let runtime = runtime_with_objects(objects);

        let mut player = PlayerState::new((2, 2), 3, Direction::North);
        assert_eq!(
            player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Turned(Direction::South)
        );

        assert_eq!(
            player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Blocked {
                direction: Direction::South,
                collision: super::super::collision::Collision::ObjectEvent,
            }
        );
        assert_eq!(
            player.position(),
            (2, 2),
            "a step into an occupied tile must not move the player"
        );
        assert_eq!(player.facing(), Direction::South);
        assert!(
            !player.in_transit(),
            "a blocked step must not start a transition"
        );

        for _ in 0..4 {
            assert!(matches!(
                player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
                StepOutcome::Blocked {
                    collision: super::super::collision::Collision::ObjectEvent,
                    ..
                }
            ));
            assert_eq!(player.position(), (2, 2));
        }
    }

    #[test]
    fn a_hidden_object_event_does_not_block_the_step() {
        let objects: &'static [ObjectEvent] = Box::leak(Box::new([object(
            1,
            2,
            3,
            3,
            "FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM",
        )]));
        let runtime = runtime_with_objects(objects);
        let mut data = EventData::new();
        let hidden_npc_flag = assets::object_event_flags::resolve(
            "FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM",
        )
        .expect("a bundled hide flag must resolve");
        data.flag_set(hidden_npc_flag).unwrap();

        let mut player = PlayerState::new((2, 2), 3, Direction::South);
        assert_eq!(
            player.step(Some(Direction::South), &runtime, &no_connections, &data),
            StepOutcome::Advanced {
                from: (2, 2),
                to: (2, 3),
            },
            "a hidden template does not occupy the map"
        );
        assert_eq!(player.position(), (2, 3));
    }

    #[test]
    fn object_event_collision_respects_the_elevation_wildcard() {
        let upstairs: &'static [ObjectEvent] = Box::leak(Box::new([object(1, 2, 3, 5, "0")]));
        let mut player = PlayerState::new((2, 2), 3, Direction::South);
        assert!(matches!(
            player.step(
                Some(Direction::South),
                &runtime_with_objects(upstairs),
                &no_connections,
                &NO_FLAGS
            ),
            StepOutcome::Advanced { .. }
        ));

        let transitional: &'static [ObjectEvent] = Box::leak(Box::new([object(
            1,
            2,
            3,
            super::super::collision::ELEVATION_TRANSITION,
            "0",
        )]));
        let mut player = PlayerState::new((2, 2), 3, Direction::South);
        assert_eq!(
            player.step(
                Some(Direction::South),
                &runtime_with_objects(transitional),
                &no_connections,
                &NO_FLAGS
            ),
            StepOutcome::Blocked {
                direction: Direction::South,
                collision: super::super::collision::Collision::ObjectEvent,
            }
        );
    }

    #[test]
    fn a_wall_outranks_an_object_event_on_the_same_tile() {
        let objects: &'static [ObjectEvent] = Box::leak(Box::new([object(1, 2, 3, 3, "0")]));
        let events: &'static MapEvents = Box::leak(Box::new(MapEvents {
            id: assets::MapId("MAP_TEST"),
            shared_events_map: None,
            object_events: objects,
            warp_events: &[],
            coord_events: &[],
            bg_events: &[],
        }));
        let runtime = runtime_with_events(events, |x, y| u8::from(x == 2 && y == 3));

        let mut player = PlayerState::new((2, 2), 3, Direction::South);
        assert_eq!(
            player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Blocked {
                direction: Direction::South,
                collision: super::super::collision::Collision::Impassable,
            }
        );
    }

    #[test]
    fn elevation_mismatch_outranks_an_object_event_on_the_same_tile() {
        let width = 5u16;
        let height = 5u16;
        let mut bytes = Vec::with_capacity(usize::from(width) * usize::from(height) * 2);
        for y in 0..height {
            for x in 0..width {
                let elevation = if x == 2 && y == 3 { 7 } else { 3 };
                let raw = MetatileCell {
                    metatile_id: 1,
                    collision: 0,
                    elevation,
                }
                .pack();
                bytes.extend_from_slice(&raw.to_le_bytes());
            }
        }
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width,
            height,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&bytes).unwrap();
        let header = MapHeader {
            id: assets::MapId("MAP_TEST"),
            group: 0,
            num: 0,
            name: "MapTest",
            layout: assets::LayoutId("MAP_TEST"),
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
            connections: &[] as &'static [MapConnection],
        };
        let objects: &'static [ObjectEvent] = Box::leak(Box::new([object(1, 2, 3, 3, "0")]));
        let events = MapEvents {
            id: assets::MapId("MAP_TEST"),
            shared_events_map: None,
            object_events: objects,
            warp_events: &[],
            coord_events: &[],
            bg_events: &[],
        };
        let runtime = MapRuntime::new(
            assets::MapId("MAP_TEST"),
            &header,
            &events,
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );

        let mut player = PlayerState::new((2, 2), 3, Direction::South);
        assert_eq!(
            player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Blocked {
                direction: Direction::South,
                collision: super::super::collision::Collision::ElevationMismatch,
            },
            "elevation mismatch must outrank an object event on the same \
             tile, the same order GetCollisionAtCoords enforces"
        );
        assert_eq!(player.position(), (2, 2));
    }

    #[test]
    fn a_hidden_first_stack_blocks_only_while_some_template_on_the_tile_is_visible() {
        let objects: &'static [ObjectEvent] = Box::leak(Box::new([
            object(
                1,
                2,
                3,
                3,
                "FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_POKEBALL_CYNDAQUIL",
            ),
            object(
                2,
                2,
                3,
                3,
                "FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_POKEBALL_TOTODILE",
            ),
        ]));
        let runtime = runtime_with_objects(objects);

        let mut data = EventData::new();
        let cyndaquil = assets::object_event_flags::resolve(
            "FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_POKEBALL_CYNDAQUIL",
        )
        .expect("a real FLAG_HIDE_* name must resolve");
        let totodile = assets::object_event_flags::resolve(
            "FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_POKEBALL_TOTODILE",
        )
        .expect("a real FLAG_HIDE_* name must resolve");
        data.flag_set(cyndaquil).unwrap();

        let mut player = PlayerState::new((2, 2), 3, Direction::South);
        assert_eq!(
            player.step(Some(Direction::South), &runtime, &no_connections, &data),
            StepOutcome::Blocked {
                direction: Direction::South,
                collision: super::super::collision::Collision::ObjectEvent,
            },
            "the visible second template occupies the tile even though the \
             first one declared there is hidden"
        );

        data.flag_set(totodile).unwrap();
        let mut player = PlayerState::new((2, 2), 3, Direction::South);
        assert!(matches!(
            player.step(Some(Direction::South), &runtime, &no_connections, &data),
            StepOutcome::Advanced { .. }
        ));
    }

    #[test]
    fn mom_blocks_a_step_into_her_tile_in_brendans_house_1f() {
        let events = assets::MapEventsTable::new()
            .resolve(assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F"))
            .expect("a bundled map must resolve in the generated table");
        let mom = events
            .object_events
            .iter()
            .find(|o| o.graphics_id == "OBJ_EVENT_GFX_MOM")
            .expect("1F's object events include Mom");
        assert_eq!(
            (mom.x, mom.y, mom.elevation),
            (2, 6, 3),
            "fixture precondition: Mom's real map.json position"
        );

        let mut data = EventData::new();
        for &id in assets::RESET_MAP_FLAGS {
            data.flag_set(id).unwrap();
        }
        assert!(
            super::super::object_event::object_event_is_visible(mom, &data),
            "fixture precondition: a fresh save does not hide Mom"
        );

        let runtime = runtime_with_events(events, |_, _| 0);
        let mut player = PlayerState::new((2, 7), 3, Direction::North);
        assert_eq!(
            player.step(Some(Direction::North), &runtime, &no_connections, &data),
            StepOutcome::Blocked {
                direction: Direction::North,
                collision: super::super::collision::Collision::ObjectEvent,
            }
        );
        assert_eq!(
            player.position(),
            (2, 7),
            "the player must stop on the tile adjacent to Mom"
        );
        assert_eq!(player.facing(), Direction::North);
    }
}
