//! [`PlayerState`]: the player avatar's tile position, facing, and sub-tile
//! step progress (S-5, issue #108).
//!
//! Ported behaviour, from `pokeemerald/src/field_player_avatar.c`:
//! - Turn-vs-step classification: `CheckMovementInputNotOnBike` (the
//!   `NOT_MOVING`/`TURN_DIRECTION`/`MOVING` `runningState` machine).
//! - Collision handling and elevation-adopt-on-arrival:
//!   `PlayerNotOnBikeMoving` + `CheckForPlayerAvatarCollision` (collision
//!   classification itself lives in `crate::overworld::collision`) and
//!   `event_object_movement.c`'s `ObjectEventUpdateElevation`.
//! - Walk speed: `event_object_movement.c`'s `MOVE_SPEED_NORMAL` step table
//!   (`sStep1Funcs`, 16 entries) — see [`WALK_FRAMES_PER_TILE`].
//!
//! **Scope: ordinary on-foot walking only.** No bike (`MovePlayerOnBike`),
//! no running (`PlayerRun`/dash), no forced movement (currents, slopes, ice
//! — `TryDoMetatileBehaviorForcedMovement` and its `ForcedMovement_*`
//! table), no ledges (`ShouldJumpLedge`), no NPC/object-event collision
//! (`DoesObjectCollideWithObjectAt` — there are no other object events to
//! collide with yet). All of upstream's per-frame *animation* timing (the
//! specific pixel offsets `Step1`/`Step2`/... apply) is also out of scope —
//! there is no renderer yet to consume it — but the *pacing* it produces
//! (16 frames to cross one tile at a walk, and no new turn/step decision
//! takes effect until that finishes) is modelled, since the issue calls for
//! "sub-tile step progress" as a first-class concept future rendering work
//! can build on.

use assets::MapId;

use super::collision::elevation_mismatch;
use super::direction::Direction;
use super::map_runtime::{ConnectedMapData, MapRuntime};

/// Frames a normal (on-foot, not running) walk step takes to cross one
/// tile. Mirrors upstream `MOVE_SPEED_NORMAL`'s per-frame step table
/// (`sStep1Funcs`, `event_object_movement.c`): 16 entries, each of which
/// advances the sprite 1 of a tile's 16 pixels. Running (`MOVE_SPEED_FAST_1`,
/// 8 frames) and every faster/bike speed are out of v1 scope (see the
/// module docs).
pub const WALK_FRAMES_PER_TILE: u8 = 16;

/// A tile position, `(x, y)`, in whichever map's coordinate space the
/// current [`MapRuntime`] is bound to.
pub type TilePos = (i32, i32);

/// The upstream `runningState` machine
/// (`include/global.fieldmap.h`'s `NOT_MOVING`/`TURN_DIRECTION`/`MOVING`
/// enum), tracked so a direction change is classified as a turn-in-place
/// only when the avatar was not already mid-movement — see
/// [`PlayerState::step`]'s doc for the exact rule and why it produces the
/// "hold to keep moving even around corners" behaviour real play exhibits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunningState {
    NotMoving,
    Turning,
    Moving,
}

/// The player avatar's on-foot movement state: tile position, elevation,
/// facing, and sub-tile step progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerState {
    position: TilePos,
    elevation: u8,
    facing: Direction,
    running: RunningState,
    /// Frames elapsed of the current step's walk animation, or `None` when
    /// not mid-step. Upstream's combined `tileTransitionState`/anim-active
    /// gate, reduced to just the counter a future renderer needs (see the
    /// module docs).
    transit_frames: Option<u8>,
}

/// The result of one [`PlayerState::step`] call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepOutcome {
    /// No input, or a new step/turn can't begin yet because the previous
    /// step's walk animation hasn't finished (mirrors upstream's
    /// `PlayerIsAnimActive()` gate on `PlayerSetAnimId`).
    Idle,
    /// The avatar turned to face `direction` without moving (upstream
    /// `PlayerNotOnBikeTurningInPlace` / `PlayerTurnInPlace`).
    Turned(Direction),
    /// A step in `direction` was attempted and denied by collision
    /// (upstream `PlayerNotOnBikeCollide`); position is unchanged.
    Blocked {
        /// The direction the step was attempted in.
        direction: Direction,
        /// Why it was denied.
        collision: super::collision::Collision,
    },
    /// The avatar stepped from one tile to an adjacent tile on the same
    /// map.
    Advanced {
        /// The tile stepped from.
        from: TilePos,
        /// The tile stepped to.
        to: TilePos,
    },
    /// The step landed outside the current map's grid, on a tile a
    /// [`super::map_runtime::MapConnection`](assets::MapConnection) covers.
    /// The caller must rebind its [`MapRuntime`] to `to_map`'s data before
    /// the next call. The landing cell has already been checked and its
    /// elevation adopted; `to_position` is expressed in `to_map`'s own
    /// coordinate space (see
    /// [`MapRuntime::resolve_connection`](super::map_runtime::MapRuntime::resolve_connection)).
    Crossed {
        /// The map the crossing entered.
        to_map: MapId,
        /// The landing position, in `to_map`'s coordinate space.
        to_position: TilePos,
    },
}

impl PlayerState {
    /// A freshly placed player: not moving, facing `facing`, standing at
    /// `position` on a tile at `elevation`.
    #[must_use]
    pub const fn new(position: TilePos, elevation: u8, facing: Direction) -> Self {
        Self {
            position,
            elevation,
            facing,
            running: RunningState::NotMoving,
            transit_frames: None,
        }
    }

    /// The avatar's current tile position.
    #[must_use]
    pub const fn position(&self) -> TilePos {
        self.position
    }

    /// The avatar's current elevation (adopted from the tile last arrived
    /// at — see [`PlayerState::step`]).
    #[must_use]
    pub const fn elevation(&self) -> u8 {
        self.elevation
    }

    /// The direction the avatar currently faces.
    #[must_use]
    pub const fn facing(&self) -> Direction {
        self.facing
    }

    /// Frames elapsed of the current step's walk animation, `0` when not
    /// mid-step.
    #[must_use]
    pub const fn step_progress(&self) -> u8 {
        match self.transit_frames {
            Some(frames) => frames,
            None => 0,
        }
    }

    /// Whether a step is still animating (`step_progress()` is less than
    /// [`WALK_FRAMES_PER_TILE`]).
    #[must_use]
    pub const fn in_transit(&self) -> bool {
        self.transit_frames.is_some()
    }

    /// Advance the current step's walk-animation timer by one frame.
    /// Once [`WALK_FRAMES_PER_TILE`] frames have elapsed, [`PlayerState::step`]
    /// accepts a new turn/step again.
    pub fn tick(&mut self) {
        if let Some(frames) = self.transit_frames.as_mut() {
            *frames += 1;
            if *frames >= WALK_FRAMES_PER_TILE {
                self.transit_frames = None;
            }
        }
    }

    /// Process one input poll: `Some(direction)` for a held direction,
    /// `None` for no direction held. Mirrors upstream `PlayerStep` narrowed
    /// to on-foot movement — `MovePlayerAvatarUsingKeypadInput` ->
    /// `MovePlayerNotOnBike` -> `CheckMovementInputNotOnBike` ->
    /// `PlayerNotOnBike{NotMoving,TurningInPlace,Moving}`.
    ///
    /// # Turn vs. step
    ///
    /// Mirrors `CheckMovementInputNotOnBike`'s exact rule: a direction
    /// different from the avatar's facing turns it in place *only* when the
    /// avatar was not already mid-movement (`running != Moving`); once a
    /// movement streak has started, changing direction steps in the new
    /// direction immediately rather than inserting a turn frame — this is
    /// why real play lets you "cut a corner" while holding input, but a
    /// fresh direction press from a standstill always turns first before it
    /// steps.
    ///
    /// # Busy gate
    ///
    /// If [`PlayerState::in_transit`], this call is a no-op returning
    /// [`StepOutcome::Idle`]. In upstream, `TryInterruptObjectEventSpecialAnim`
    /// consumes `DIR_NONE` while the held walk movement is unfinished, before
    /// `CheckMovementInputNotOnBike` can reset `runningState`; preserving the
    /// state here retains the same corner-cut behavior at tile center. Call
    /// [`PlayerState::tick`] each frame to drain the transit timer.
    pub fn step(
        &mut self,
        input: Option<Direction>,
        runtime: &MapRuntime<'_>,
        maps: &impl ConnectedMapData,
    ) -> StepOutcome {
        if self.in_transit() {
            return StepOutcome::Idle;
        }

        let Some(direction) = input else {
            self.running = RunningState::NotMoving;
            return StepOutcome::Idle;
        };

        if direction != self.facing && self.running != RunningState::Moving {
            self.running = RunningState::Turning;
            self.facing = direction;
            return StepOutcome::Turned(direction);
        }

        self.running = RunningState::Moving;
        self.facing = direction;

        let (dx, dy) = direction.delta();
        let target = (self.position.0 + dx, self.position.1 + dy);

        if let Some(cell) = runtime.metatile_cell(target.0, target.1) {
            if cell.collision != 0 {
                return StepOutcome::Blocked {
                    direction,
                    collision: super::collision::Collision::Impassable,
                };
            }
            if elevation_mismatch(self.elevation, cell.elevation) {
                return StepOutcome::Blocked {
                    direction,
                    collision: super::collision::Collision::ElevationMismatch,
                };
            }

            let from = self.position;
            self.position = target;
            // ObjectEventUpdateElevation (event_object_movement.c):
            // `currentElevation` — the field the collision mismatch check
            // consumes — adopts the arrival tile's elevation, including
            // ELEVATION_TRANSITION (0). That 0 is the wildcard that lets
            // the next step cross between levels (stairs/bridge landings).
            // Only multi-level tiles are skipped, mirroring upstream.
            if cell.elevation != super::collision::ELEVATION_MULTI_LEVEL {
                self.elevation = cell.elevation;
            }
            self.transit_frames = Some(0);
            return StepOutcome::Advanced { from, to: target };
        }

        if let Some(crossing) = runtime.resolve_connection(direction, target.0, target.1, maps) {
            let Some(cell) =
                maps.metatile_cell(crossing.target, crossing.position.0, crossing.position.1)
            else {
                return StepOutcome::Blocked {
                    direction,
                    collision: super::collision::Collision::Impassable,
                };
            };
            if cell.collision != 0 {
                return StepOutcome::Blocked {
                    direction,
                    collision: super::collision::Collision::Impassable,
                };
            }
            if elevation_mismatch(self.elevation, cell.elevation) {
                return StepOutcome::Blocked {
                    direction,
                    collision: super::collision::Collision::ElevationMismatch,
                };
            }

            self.position = crossing.position;
            if cell.elevation != super::collision::ELEVATION_MULTI_LEVEL {
                self.elevation = cell.elevation;
            }
            self.transit_frames = Some(0);
            return StepOutcome::Crossed {
                to_map: crossing.target,
                to_position: crossing.position,
            };
        }

        StepOutcome::Blocked {
            direction,
            collision: super::collision::Collision::Impassable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overworld::map_runtime::MapRuntime;
    use assets::{
        BattleScene, MapConnection, MapEvents, MapHeader, MapType, MetatileAttributeTable,
        MetatileCell, RegionMapSectionId, Weather,
    };

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
        let outcome = player.step(Some(Direction::South), &runtime, &no_connections);
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
        // Facing South, tap East: turn only, no step (CheckMovementInputNotOnBike:
        // direction != movementDirection && runningState != MOVING -> TURN_DIRECTION).
        let outcome = player.step(Some(Direction::East), &runtime, &no_connections);
        assert_eq!(outcome, StepOutcome::Turned(Direction::East));
        assert_eq!(
            player.position(),
            (2, 2),
            "a turn must not move the tile position"
        );
        assert_eq!(player.facing(), Direction::East);
        assert!(!player.in_transit());

        // Holding the same (now-facing) direction next poll steps.
        let outcome = player.step(Some(Direction::East), &runtime, &no_connections);
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
        // Start a south step, then let the walk animation finish.
        assert!(matches!(
            player.step(Some(Direction::South), &runtime, &no_connections),
            StepOutcome::Advanced { .. }
        ));
        for _ in 0..WALK_FRAMES_PER_TILE {
            player.tick();
        }
        assert!(!player.in_transit());

        // Still "moving" (held South continuously); now change to East. Per
        // CheckMovementInputNotOnBike, `runningState == MOVING` short-circuits the
        // turn-in-place branch, so this steps East immediately rather than turning.
        let outcome = player.step(Some(Direction::East), &runtime, &no_connections);
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
            player.step(Some(Direction::South), &runtime, &no_connections),
            StepOutcome::Advanced { .. }
        ));

        // TryInterruptObjectEventSpecialAnim consumes DIR_NONE while the
        // current held movement is unfinished, so CheckMovementInputNotOnBike
        // cannot reset runningState to NOT_MOVING during these polls.
        for _ in 0..WALK_FRAMES_PER_TILE {
            assert_eq!(
                player.step(None, &runtime, &no_connections),
                StepOutcome::Idle
            );
            player.tick();
        }

        let outcome = player.step(Some(Direction::East), &runtime, &no_connections);
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
            player.step(Some(Direction::South), &runtime, &no_connections),
            StepOutcome::Advanced { .. }
        ));
        for _ in 0..WALK_FRAMES_PER_TILE {
            player.tick();
        }
        // Release input for one poll.
        assert_eq!(
            player.step(None, &runtime, &no_connections),
            StepOutcome::Idle
        );
        // A new direction now turns first rather than stepping immediately.
        let outcome = player.step(Some(Direction::East), &runtime, &no_connections);
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
            player.step(Some(Direction::South), &runtime, &no_connections),
            StepOutcome::Advanced { .. }
        ));
        // Immediately try to step again before ticking: busy, no-op.
        assert_eq!(
            player.step(Some(Direction::East), &runtime, &no_connections),
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
        // Wall directly south of the player's start.
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
        let outcome = player.step(Some(Direction::South), &runtime, &no_connections);
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
                // Every tile is elevation 3 except the one south of start (elevation 7).
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
        let outcome = player.step(Some(Direction::South), &runtime, &no_connections);
        assert_eq!(
            outcome,
            StepOutcome::Blocked {
                direction: Direction::South,
                collision: super::super::collision::Collision::ElevationMismatch,
            }
        );
        assert_eq!(player.position(), (2, 2));
    }

    /// `ObjectEventUpdateElevation` adopts `currentElevation` from the
    /// arrival tile *including* `ELEVATION_TRANSITION` (0) — that 0 is the
    /// wildcard letting the next step cross between levels (stairs/bridge
    /// landings). A 3 → transition → 5 walk must therefore succeed.
    #[test]
    fn transition_tile_lets_the_next_step_cross_between_elevations() {
        let width = 5u16;
        let height = 5u16;
        let mut bytes = Vec::with_capacity(usize::from(width) * usize::from(height) * 2);
        for y in 0..height {
            for x in 0..width {
                // Column x=2: elevation 3 at y=1, transition (0) at y=2,
                // elevation 5 at y=3; everything else elevation 3.
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
        // Step onto the transition tile: allowed, elevation becomes the
        // wildcard 0.
        let onto_transition = player.step(Some(Direction::South), &runtime, &no_connections);
        assert!(matches!(onto_transition, StepOutcome::Advanced { .. }));
        assert_eq!(player.elevation(), 0);
        for _ in 0..WALK_FRAMES_PER_TILE {
            player.tick();
        }
        // Step from the transition tile onto elevation 5: the wildcard
        // permits it (upstream would block 3 → 5 directly).
        let onto_upper = player.step(Some(Direction::South), &runtime, &no_connections);
        assert!(matches!(onto_upper, StepOutcome::Advanced { .. }));
        assert_eq!(player.position(), (2, 3));
        assert_eq!(player.elevation(), 5);
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
        let outcome = player.step(Some(Direction::South), &runtime, &maps);
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
            player.step(Some(Direction::South), &runtime, &maps),
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
            player.step(Some(Direction::South), &runtime, &maps),
            StepOutcome::Blocked {
                direction: Direction::South,
                collision: super::super::collision::Collision::ElevationMismatch,
            }
        );
        assert_eq!(player.position(), (2, 4));
        assert_eq!(player.elevation(), 3);
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
        let outcome = player.step(Some(Direction::South), &runtime, &no_connections);
        assert_eq!(
            outcome,
            StepOutcome::Blocked {
                direction: Direction::South,
                collision: super::super::collision::Collision::Impassable,
            }
        );
        assert_eq!(player.position(), (2, 4));
    }
}
