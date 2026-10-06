//! On-foot player movement, collision, connection crossings, and tile pacing.

use assets::{MapId, MetatileCell};

use super::collision::{directionally_impassable, elevation_mismatch, Collision};
use super::direction::Direction;
use super::map_runtime::{ConnectedMapData, MapRuntime};
use super::metatile_behavior::{
    is_forced_movement, is_forced_movement_input_tile, MB_EASTWARD_CURRENT, MB_ICE, MB_LONG_GRASS,
    MB_MUDDY_SLOPE, MB_NORMAL, MB_NORTHWARD_CURRENT, MB_SLIDE_EAST, MB_SLIDE_NORTH, MB_SLIDE_SOUTH,
    MB_SLIDE_WEST, MB_SOUTHWARD_CURRENT, MB_TRICK_HOUSE_PUZZLE_8_FLOOR, MB_WALK_EAST,
    MB_WALK_NORTH, MB_WALK_SOUTH, MB_WALK_WEST, MB_WATERFALL, MB_WESTWARD_CURRENT,
};
use super::object_event::visible_object_event_at;
use crate::event_data::EventData;

/// Frames required for a normal on-foot step to cross one tile: also the
/// pace `ForcedMovement_WalkSouth/North/West/East`'s `PlayerWalkNormal`
/// mover crosses a standing `MB_WALK_*` tile at
/// (`field_player_avatar.c:486-503, 975-977`; `sStepTimes[MOVE_SPEED_NORMAL]
/// == ARRAY_COUNT(sStep1Funcs) == 16`, `event_object_movement.c:8233-8298`).
pub const WALK_FRAMES_PER_TILE: u8 = 16;

/// Frames a standing `MB_SLIDE_*` tile's `ForcedMovement_Slide` mover takes
/// to cross one tile, via `PlayerWalkFast`
/// (`field_player_avatar.c:526-552, 979-982`; `sStepTimes[MOVE_SPEED_FAST_1]
/// == ARRAY_COUNT(sStep2Funcs) == 8`, `event_object_movement.c:8233-8298`).
pub const SLIDE_FRAMES_PER_TILE: u8 = 8;

/// Frames a held-B run crosses one tile in: upstream's `PlayerRun` mover
/// initialises `MOVE_SPEED_FAST_1`, so `sStepTimes` gives 8 two-pixel updates
/// (`field_player_avatar.c:995-998`, `event_object_movement.c:5110-5124,
/// 8233-8298`).
pub const RUN_FRAMES_PER_TILE: u8 = 8;

/// `FLAG_SYS_B_DASH` (`SYSTEM_FLAGS + 0x60`): the running shoes were received
/// (`include/constants/flags.h:1462`).
const FLAG_SYS_B_DASH: u16 = 0x8C0;

const MB_NO_RUNNING: u8 = 0x0A;
const MB_HOT_SPRINGS: u8 = 0x28;
const MB_PACIFIDLOG_VERTICAL_LOG_TOP: u8 = 0x74;
const MB_PACIFIDLOG_HORIZONTAL_LOG_RIGHT: u8 = 0x77;
const MB_FORTREE_BRIDGE: u8 = 0x78;

/// `MetatileBehavior_IsRunningDisallowed` plus the Fortree bridge's
/// even-elevation rule from `IsRunningDisallowedByMetatile`
/// (`metatile_behavior.c:1258-1266`, `bike.c:1056-1062`).
const fn running_disallowed_by_metatile(behavior: u8, elevation: u8) -> bool {
    matches!(behavior, MB_NO_RUNNING | MB_LONG_GRASS | MB_HOT_SPRINGS)
        || (behavior >= MB_PACIFIDLOG_VERTICAL_LOG_TOP
            && behavior <= MB_PACIFIDLOG_HORIZONTAL_LOG_RIGHT)
        || (behavior == MB_FORTREE_BRIDGE && elevation & 1 == 0)
}

/// Frames a standstill turn busies movement input for: upstream's
/// `WALK_IN_PLACE_FAST` action (`event_object_movement.c:5704-5721`).
pub const TURN_IN_PLACE_FRAMES: u8 = 8;

/// A tile position in the active map's coordinate space.
pub type TilePos = (i32, i32);

/// A tile crossing's per-crossing pace, fixed at the frame it started:
/// its total frame count and whether its walk animation is paused on the
/// forward-foot cell for the whole crossing rather than switching to the
/// standing pose at the halfway point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TransitCadence {
    /// [`WALK_FRAMES_PER_TILE`] for an ordinary step or a dispatched walk
    /// tile, [`SLIDE_FRAMES_PER_TILE`] for a dispatched slide tile.
    duration: u8,
    /// `ForcedMovement_Slide` sets `disableAnim` before moving
    /// (`field_player_avatar.c:526-532`), so the go-fast animation never
    /// reaches its standing half; the four `ForcedMovement_Walk*` and every
    /// ordinary step never set it.
    animation_disabled: bool,
    /// Whether the crossing plays `sAnim_Run*` on the running sheet rather
    /// than `sAnim_Go*` on the walking sheet (`event_object_movement.c:5120-5124`).
    running: bool,
}

impl TransitCadence {
    /// An ordinary step's pace, and a dispatched walk tile's.
    const WALK: Self = Self {
        duration: WALK_FRAMES_PER_TILE,
        animation_disabled: false,
        running: false,
    };

    /// A dispatched slide tile's pace.
    const SLIDE: Self = Self {
        duration: SLIDE_FRAMES_PER_TILE,
        animation_disabled: true,
        running: false,
    };

    /// A held-B run's pace: fast, with the walk animation left running.
    const RUN: Self = Self {
        duration: RUN_FRAMES_PER_TILE,
        animation_disabled: false,
        running: true,
    };
}

/// Which of a walk animation's two forward-foot cells leads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LeadingFoot {
    /// Fresh command 0, before any step, turn, or face. A direct first step
    /// or turn leads with the first foot; an idle face, a field lock, or a
    /// scripted face moves it to `First`.
    Fresh,
    First,
    Second,
}

/// The sprite pose the player holds at rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RestPose {
    Standing,
    /// A finished slide crossing's animation-paused forward foot.
    SlidePaused,
    /// A finished run crossing's neutral running cell, held until the next
    /// keypad poll (`MovementAction_PlayerRun*_Step1` pauses the animation
    /// at the step's end, `event_object_movement.c:6020-6081`).
    RunPaused,
}

/// The player's tile position, facing, elevation, and step progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerState {
    position: TilePos,
    collision_elevation: u8,
    render_elevation: u8,
    facing: Direction,
    movement_direction: Direction,
    movement_streak_active: bool,
    transit_frames: Option<u8>,
    /// The active crossing's pace. Meaningless while `transit_frames` is
    /// `None`.
    transit_cadence: TransitCadence,
    turn_frames_remaining: u8,
    transit_direction: Option<Direction>,
    /// The elevation of the cell the active crossing lands on, re-adopted
    /// when the crossing finishes. Stale at rest.
    landing_elevation: u8,
    forced_movement_armed: bool,
    forced_input_tile_center: bool,
    rest_pose: RestPose,
    /// The current walk-cycle phase (`sAnim_Go*` alternate cells,
    /// `object_event_anims.h:202-272`).
    leading_foot: LeadingFoot,
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
    /// The connected map entered, or `None` for a same-map landing.
    to_map: Option<MapId>,
}

impl Landing {
    fn on_current_map(position: TilePos, cell: MetatileCell, destination_behavior: u8) -> Self {
        Self {
            position,
            cell,
            destination_behavior,
            object_events_accessible: true,
            to_map: None,
        }
    }

    fn across_connection(
        to_map: MapId,
        position: TilePos,
        cell: MetatileCell,
        destination_behavior: u8,
    ) -> Self {
        // `ConnectedMapData` exposes the neighbour's cells and behaviors, but
        // not its object events.
        Self {
            position,
            cell,
            destination_behavior,
            object_events_accessible: false,
            to_map: Some(to_map),
        }
    }
}

impl PlayerState {
    /// Creates a stationary player on `position`.
    ///
    /// Collision and render elevations both start at `elevation`. Only an
    /// observed landing arms
    /// [`forced_movement_armed`](Self::forced_movement_armed), so a placement
    /// onto a forced-movement tile is never trapped there.
    #[must_use]
    pub const fn new(position: TilePos, elevation: u8, facing: Direction) -> Self {
        Self {
            position,
            collision_elevation: elevation,
            render_elevation: elevation,
            facing,
            movement_direction: facing,
            movement_streak_active: false,
            transit_frames: None,
            transit_cadence: TransitCadence::WALK,
            turn_frames_remaining: 0,
            transit_direction: None,
            landing_elevation: elevation,
            forced_movement_armed: false,
            forced_input_tile_center: false,
            rest_pose: RestPose::Standing,
            leading_foot: LeadingFoot::Fresh,
        }
    }

    /// Creates a stationary player carrying a saved elevation pair.
    ///
    /// Upstream's continue restores the player object's persisted
    /// `currentElevation`/`previousElevation` verbatim
    /// (`LoadObjectEvents`, `src/load_save.c:188-193`) rather than deriving
    /// both from the landing tile as [`Self::new`] does; the two differ on
    /// multi-level and transition cells, where
    /// `ObjectEventUpdateElevation` retains history.
    #[must_use]
    pub const fn with_saved_elevations(
        position: TilePos,
        current_elevation: u8,
        previous_elevation: u8,
        facing: Direction,
    ) -> Self {
        let mut player = Self::new(position, current_elevation, facing);
        player.render_elevation = previous_elevation;
        player
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

    /// Returns the current movement direction, which a slide leaves on its
    /// own direction while facing stays locked, until the next poll off
    /// the tile resynchronises the two ([`step`](Self::step)'s "Forced
    /// movement"), as `GetPlayerMovementDirection` reads
    /// `movementDirection` (`field_player_avatar.c:429-440, 526-532`).
    #[must_use]
    pub const fn movement_direction(&self) -> Direction {
        self.movement_direction
    }

    /// Changes facing without starting or interrupting a step, carrying the
    /// movement direction with it as upstream's `SetObjectEventDirection`
    /// does (`event_object_movement.c:2361-2371`). Callers model a face
    /// movement action, whose `FaceDirection` normalises a fresh walk cycle
    /// (`event_object_movement.c:5048-5054`).
    pub const fn face(&mut self, direction: Direction) {
        self.normalise_fresh_step_parity();
        self.rest_pose = RestPose::Standing;
        self.facing = direction;
        self.movement_direction = direction;
    }

    /// Ends a standstill turn's busy window early, as `PlayerFreeze` does
    /// the instant the field lock engages (`field_player_avatar.c:1039-1046`).
    /// Its forced face normalises a fresh walk cycle, so an owner that claims
    /// the first frame before any idle poll still leaves the second foot to
    /// lead the next step.
    pub const fn clear_turn_lock(&mut self) {
        self.normalise_fresh_step_parity();
        self.rest_pose = RestPose::Standing;
        self.turn_frames_remaining = 0;
    }

    /// Returns whether the standing tile is in upstream's forced-movement
    /// dispatch set, `sForcedMovementTestFuncs`
    /// (`field_player_avatar.c:412-427`). [`supported_forced_mover`]
    /// dispatches a `MB_WALK_*`/`MB_SLIDE_*` tile as real movement; every
    /// other armed tile still refuses manual steps. Armed only by a step
    /// this state committed, never by placement.
    #[must_use]
    pub const fn forced_movement_armed(&self) -> bool {
        self.forced_movement_armed
    }

    /// Returns whether this frame is a forced landing's `T_TILE_CENTER`, the
    /// only state in which `FieldGetPlayerInput` skips its button block; its
    /// `T_NOT_MOVING` arm admits input (`field_control_avatar.c:93-113`).
    #[must_use]
    pub const fn field_input_suppressed(&self) -> bool {
        self.forced_input_tile_center
    }

    /// Returns frames elapsed in the current tile crossing, or zero at rest.
    #[must_use]
    pub const fn step_progress(&self) -> u8 {
        match self.transit_frames {
            Some(frames) => frames,
            None => 0,
        }
    }

    /// Returns the active tile crossing's total frame count --
    /// [`WALK_FRAMES_PER_TILE`] for an ordinary step or a dispatched walk
    /// tile, [`SLIDE_FRAMES_PER_TILE`] for a dispatched slide tile. Only
    /// meaningful alongside [`step_progress`](Self::step_progress) while
    /// [`step_direction`](Self::step_direction) is `Some`; stale (the
    /// last crossing's duration) at rest.
    #[must_use]
    pub const fn transit_duration(&self) -> u8 {
        self.transit_cadence.duration
    }

    /// Returns whether the active tile crossing pauses its walk animation
    /// on the forward-foot cell for the whole crossing, as a dispatched
    /// slide tile's `disableAnim` does. Only meaningful while
    /// [`step_direction`](Self::step_direction) is `Some`; stale at rest.
    #[must_use]
    pub const fn transit_animation_disabled(&self) -> bool {
        self.transit_cadence.animation_disabled
    }

    /// Returns whether a finished slide still holds its paused forward-foot
    /// pose until the next keypad poll or field lock (`event_object_movement.c:7302-7306`).
    #[must_use]
    pub const fn slide_pose_held(&self) -> bool {
        matches!(self.rest_pose, RestPose::SlidePaused)
    }

    /// Returns whether the active tile crossing is a held-B run, drawn from
    /// the running sheet. Only meaningful while
    /// [`step_direction`](Self::step_direction) is `Some`; stale at rest.
    #[must_use]
    pub const fn transit_running(&self) -> bool {
        self.transit_cadence.running
    }

    /// Returns whether a finished run still holds its neutral running pose
    /// until the next keypad poll or field lock.
    #[must_use]
    pub const fn run_pose_held(&self) -> bool {
        matches!(self.rest_pose, RestPose::RunPaused)
    }

    /// Returns whether the current walk-cycle phase is the second foot.
    #[must_use]
    pub const fn second_foot_leads(&self) -> bool {
        matches!(self.leading_foot, LeadingFoot::Second)
    }

    /// A fresh command 0 faces at command 1 (`SetStepAnim` seeks
    /// `animPos[0]`, `event_object_movement.c:4613-4617, 5048-5054`), so the
    /// next step or turn selects command 2, the second foot. A started cycle,
    /// held slide foot included, is kept. Every face upstream runs, the idle
    /// poll's, `PlayerFreeze`'s, and a scripted face's, goes through it.
    const fn normalise_fresh_step_parity(&mut self) {
        if matches!(self.leading_foot, LeadingFoot::Fresh) {
            self.leading_foot = LeadingFoot::First;
        }
    }

    /// `SetStepAnimHandleAlternation` leaves a paused foot command unchanged
    /// (`event_object_movement.c:4582-4598`), so a held slide pose keeps its foot.
    fn advance_step_parity(&mut self, slide_pose_held: bool) {
        if !slide_pose_held {
            self.leading_foot = match self.leading_foot {
                LeadingFoot::First => LeadingFoot::Second,
                LeadingFoot::Fresh | LeadingFoot::Second => LeadingFoot::First,
            };
        }
    }

    /// Returns whether a tile crossing is still in progress.
    #[must_use]
    pub const fn in_transit(&self) -> bool {
        self.transit_frames.is_some()
    }

    /// Returns frames left in a standstill turn's busy window, or zero when no
    /// turn is in flight.
    #[must_use]
    pub const fn turn_frames_remaining(&self) -> u8 {
        self.turn_frames_remaining
    }

    /// Returns the direction committed for the active tile crossing, or `None` at rest.
    ///
    /// Unlike [`facing`](Self::facing), this is fixed for the crossing's whole
    /// duration: [`face`](Self::face) may change facing mid-step, but never this.
    #[must_use]
    pub const fn step_direction(&self) -> Option<Direction> {
        self.transit_direction
    }

    /// Advances an active tile crossing by one frame; a stationary player remains stationary.
    ///
    /// A crossing drains at the per-tile pace fixed when it started, so a
    /// dispatched slide tile ([`SLIDE_FRAMES_PER_TILE`]) drains twice as
    /// fast as an ordinary step or a dispatched walk tile
    /// ([`WALK_FRAMES_PER_TILE`]).
    ///
    /// Also drains [`TURN_IN_PLACE_FRAMES`], independently of tile transit.
    pub fn tick(&mut self) {
        let crossing = self.transit_frames.is_some();
        if let Some(frames) = self.transit_frames.as_mut() {
            *frames += 1;
            if *frames >= self.transit_cadence.duration {
                self.transit_frames = None;
                self.transit_direction = None;
                // Upstream's finished step shifts previous coords onto
                // current (`ShiftStillObjectEventCoords`,
                // `event_object_movement.c:2162-2165`) and re-runs
                // `ObjectEventUpdateElevation` over the landing cell alone
                // (`DoGroundEffects_OnFinishStep`, `:8085-8104`), so a
                // multi-level origin's deferred adoption lands here.
                self.adopt_elevation(self.landing_elevation, self.landing_elevation);
                if self.transit_cadence.animation_disabled {
                    self.rest_pose = RestPose::SlidePaused;
                } else if self.transit_cadence.running {
                    self.rest_pose = RestPose::RunPaused;
                }
            }
        }
        // The landing's tile-center frame is the first whole frame it spends
        // at rest; from the next one upstream reads `T_NOT_MOVING`.
        if !crossing {
            self.forced_input_tile_center = false;
        }
        self.turn_frames_remaining = self.turn_frames_remaining.saturating_sub(1);
    }

    /// `ObjectEventUpdateElevation` (`event_object_movement.c:7725-7737`):
    /// skipped whole while either end is multi-level, and a transition
    /// destination keeps the render elevation.
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
    /// # Forced movement
    ///
    /// An armed tile dispatches before the keypad, even on a `None` poll,
    /// winning over whatever direction the caller polled
    /// (`field_player_avatar.c:332-349`). See
    /// [`dispatch_forced_mover`](Self::dispatch_forced_mover) for a
    /// supported tile's own dispatch and blocked-route fallback, and
    /// [`forced_movement_armed`](Self::forced_movement_armed) for the
    /// dispatched-vs-deferred split.
    ///
    /// # Turn vs. step
    ///
    /// A direction change turns in place unless a movement streak is active.
    /// Every attempted step starts or continues that streak, including a blocked
    /// attempt. An accepted poll without directional input ends the streak.
    /// A turn also busies every poll, including the new direction's own, for
    /// [`TURN_IN_PLACE_FRAMES`].
    ///
    /// # Collision
    ///
    /// Same-map steps test impassability, elevation, then visible object
    /// occupancy. A connection landing is classified through
    /// [`ConnectedMapData::metatile_behavior`], failing closed on `None`;
    /// only the neighbour's object events stay unavailable.
    ///
    /// A blocked attempt leaves the player facing the attempted direction.
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
        self.step_with_run(input, false, runtime, maps, event_data)
    }

    /// [`Self::step`] with the held state of B.
    ///
    /// # Running
    ///
    /// A step that clears collision runs, at [`RUN_FRAMES_PER_TILE`], when
    /// `run_held` is set, `FLAG_SYS_B_DASH` is set in `event_data`, the
    /// map header allows running, and the departure tile's behavior permits
    /// it (`field_player_avatar.c:658-668`, `bike.c:1056-1062`). The
    /// permission is read at each eligible poll rather than cached, and a
    /// crossing already under way keeps the pace it started with. Turns,
    /// blocked attempts, and forced movement are never run.
    pub fn step_with_run(
        &mut self,
        input: Option<Direction>,
        run_held: bool,
        runtime: &MapRuntime<'_>,
        maps: &impl ConnectedMapData,
        event_data: &EventData,
    ) -> StepOutcome {
        if self.in_transit() || self.turn_frames_remaining > 0 {
            return StepOutcome::Idle;
        }

        let standing_behavior = runtime
            .metatile_behavior(self.position.0, self.position.1)
            .unwrap_or(MB_NORMAL);
        let supported_mover = supported_forced_mover(standing_behavior);

        // Dispatched ahead of the keypad, even on a `None` poll; see
        // `step`'s "Forced movement" section for the contract.
        if self.forced_movement_armed {
            if let Some(mover) = supported_mover {
                if let Some(outcome) =
                    self.dispatch_forced_mover(mover, standing_behavior, runtime, maps, event_data)
                {
                    return outcome;
                }
                // Blocked: the guard stays set (see `dispatch_forced_mover`'s
                // doc), so keypad handling below runs on the original poll,
                // skipping the deferred-behavior check below -- this tile is
                // a supported mover, not one of those behaviors.
            }
        }

        // `ForcedMovement_None`, reached off a forced tile or through a
        // blocked `DoForcedMovement`, clears a slide's facing lock and
        // resets `movementDirection` to the locked facing
        // (`field_player_avatar.c:429-440, 449-451`). A no-op unless a slide
        // left the two apart; deferred armed tiles keep theirs.
        if !self.forced_movement_armed || supported_mover.is_some() {
            self.movement_direction = self.facing;
        }

        // Any poll that reaches the keypad, idle included, restarts the sprite
        // animation: `ForcedMovement_None` sets `enableAnim`, and a no-input poll
        // faces the standing cell (`field_player_avatar.c:429-440, 588-600`).
        let slide_pose_held = self.slide_pose_held();
        self.rest_pose = RestPose::Standing;
        let Some(direction) = input else {
            self.normalise_fresh_step_parity();
            self.movement_streak_active = false;
            return StepOutcome::Idle;
        };

        // Checked before the turn branch; see `step`'s doc for the contract.
        // `supported_mover.is_none()` excludes a tile already handled above.
        if self.forced_movement_armed && supported_mover.is_none() {
            let forced_step_blocked =
                forced_movement_direction(standing_behavior, self.movement_direction).is_some_and(
                    |forced_direction| {
                        self.forced_direction_blocked(
                            forced_direction,
                            runtime,
                            maps,
                            event_data,
                            standing_behavior,
                        )
                    },
                );
            if !forced_step_blocked {
                self.movement_streak_active = true;
                return StepOutcome::Blocked {
                    direction,
                    collision: Collision::Impassable,
                };
            }
        }

        if direction != self.facing && !self.movement_streak_active {
            self.facing = direction;
            self.movement_direction = direction;
            self.turn_frames_remaining = TURN_IN_PLACE_FRAMES;
            self.advance_step_parity(slide_pose_held);
            return StepOutcome::Turned(direction);
        }

        self.movement_streak_active = true;
        self.facing = direction;
        self.movement_direction = direction;

        match self.resolve_landing(direction, runtime, maps) {
            Ok(landing) => {
                let from = self.position;
                let to = landing.position;
                let to_map = landing.to_map;
                let running = run_held
                    && event_data.flag_get(FLAG_SYS_B_DASH).unwrap_or(false)
                    && runtime.header().allow_run
                    && !running_disallowed_by_metatile(standing_behavior, self.render_elevation);
                let started = self.try_start_resolved_step(
                    direction,
                    runtime,
                    event_data,
                    standing_behavior,
                    landing,
                    if running {
                        TransitCadence::RUN
                    } else {
                        TransitCadence::WALK
                    },
                );
                match started {
                    Ok(()) => {
                        self.advance_step_parity(slide_pose_held);
                        match to_map {
                            Some(to_map) => StepOutcome::Crossed {
                                to_map,
                                to_position: to,
                            },
                            None => StepOutcome::Advanced { from, to },
                        }
                    }
                    Err(collision) => StepOutcome::Blocked {
                        direction,
                        collision,
                    },
                }
            }
            Err(collision) => StepOutcome::Blocked {
                direction,
                collision,
            },
        }
    }

    /// Resolves `direction`'s destination tile, read-only: a same-map cell,
    /// a connected map's landing cell, or [`Collision::Impassable`] when
    /// neither exists (an unconnected map edge, or a connection whose
    /// landing cell [`ConnectedMapData`] cannot supply).
    fn resolve_landing(
        &self,
        direction: Direction,
        runtime: &MapRuntime<'_>,
        maps: &impl ConnectedMapData,
    ) -> Result<Landing, Collision> {
        let (dx, dy) = direction.delta();
        let target = (self.position.0 + dx, self.position.1 + dy);

        if let Some(cell) = runtime.metatile_cell(target.0, target.1) {
            let target_behavior = runtime
                .metatile_behavior(target.0, target.1)
                .unwrap_or(MB_NORMAL);
            return Ok(Landing::on_current_map(target, cell, target_behavior));
        }

        let crossing = runtime
            .resolve_connection(direction, target.0, target.1, maps)
            .ok_or(Collision::Impassable)?;
        let cell = maps
            .metatile_cell(crossing.target, crossing.position.0, crossing.position.1)
            .ok_or(Collision::Impassable)?;
        let destination_behavior = maps
            .metatile_behavior(crossing.target, crossing.position.0, crossing.position.1)
            .ok_or(Collision::Impassable)?;
        Ok(Landing::across_connection(
            crossing.target,
            crossing.position,
            cell,
            destination_behavior,
        ))
    }

    /// Returns why `landing` would deny a step in `direction`, or `None` if
    /// it would succeed. Read-only, so the mutating commit and the
    /// forced-direction probe cannot disagree about what collides.
    fn landing_collision(
        &self,
        direction: Direction,
        standing_behavior: u8,
        landing: &Landing,
        runtime: &MapRuntime<'_>,
        event_data: &EventData,
    ) -> Option<Collision> {
        if landing.cell.collision != 0
            || directionally_impassable(standing_behavior, landing.destination_behavior, direction)
        {
            return Some(Collision::Impassable);
        }
        if elevation_mismatch(self.collision_elevation, landing.cell.elevation) {
            return Some(Collision::ElevationMismatch);
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
            return Some(Collision::ObjectEvent);
        }
        None
    }

    /// `cadence` is the crossing's own pace -- the caller's, not the
    /// destination's: an ordinary step and a dispatched walk tile both use
    /// [`TransitCadence::WALK`], a dispatched slide tile its own faster,
    /// animation-paused pace.
    fn try_start_resolved_step(
        &mut self,
        direction: Direction,
        runtime: &MapRuntime<'_>,
        event_data: &EventData,
        standing_behavior: u8,
        landing: Landing,
        cadence: TransitCadence,
    ) -> Result<(), Collision> {
        if let Some(collision) =
            self.landing_collision(direction, standing_behavior, &landing, runtime, event_data)
        {
            return Err(collision);
        }

        let origin_elevation = runtime
            .metatile_cell(self.position.0, self.position.1)
            .map_or(self.collision_elevation, |origin_cell| {
                origin_cell.elevation
            });
        self.position = landing.position;
        self.adopt_elevation(origin_elevation, landing.cell.elevation);
        self.landing_elevation = landing.cell.elevation;
        self.transit_direction = Some(direction);
        self.transit_frames = Some(0);
        self.transit_cadence = cadence;
        self.rest_pose = RestPose::Standing;
        // The dispatch set guards movement, the wider input set holds field
        // input (`field_player_avatar.c:144-164`, `metatile_behavior.c:338-351`).
        self.forced_movement_armed = is_forced_movement(landing.destination_behavior);
        self.forced_input_tile_center = is_forced_movement_input_tile(landing.destination_behavior);
        Ok(())
    }

    /// Dispatches a supported standing forced tile's mover before the keypad
    /// is read (`field_player_avatar.c:443-470`). A blocked route returns
    /// `None` and leaves the guard armed and `runningState` untouched, so the
    /// keypad falls through as upstream's `FALSE` arm does (`:344-348`).
    fn dispatch_forced_mover(
        &mut self,
        mover: ForcedMover,
        standing_behavior: u8,
        runtime: &MapRuntime<'_>,
        maps: &impl ConnectedMapData,
        event_data: &EventData,
    ) -> Option<StepOutcome> {
        let direction = mover.direction;

        let landing = self.resolve_landing(direction, runtime, maps).ok()?;
        let from = self.position;
        let to = landing.position;
        let to_map = landing.to_map;
        let was_slide_paused = self.slide_pose_held();
        self.try_start_resolved_step(
            direction,
            runtime,
            event_data,
            standing_behavior,
            landing,
            mover.cadence,
        )
        .ok()?;
        self.advance_step_parity(was_slide_paused);

        self.movement_streak_active = true;
        self.movement_direction = direction;
        if !mover.locks_facing {
            self.facing = direction;
        }
        Some(match to_map {
            Some(to_map) => StepOutcome::Crossed {
                to_map,
                to_position: to,
            },
            None => StepOutcome::Advanced { from, to },
        })
    }

    /// Returns whether a step in `direction` would be collision-blocked,
    /// without mutating state, as `DoForcedMovement` tests before moving
    /// (`field_player_avatar.c:443-470`).
    fn forced_direction_blocked(
        &self,
        direction: Direction,
        runtime: &MapRuntime<'_>,
        maps: &impl ConnectedMapData,
        event_data: &EventData,
        standing_behavior: u8,
    ) -> bool {
        match self.resolve_landing(direction, runtime, maps) {
            Ok(landing) => self
                .landing_collision(direction, standing_behavior, &landing, runtime, event_data)
                .is_some(),
            Err(_) => true,
        }
    }
}

/// A supported standing forced tile's fixed direction, crossing pace, and
/// whether it locks the sprite's rendered facing, as dispatched by
/// [`supported_forced_mover`].
#[derive(Debug, Clone, Copy)]
struct ForcedMover {
    direction: Direction,
    cadence: TransitCadence,
    /// Whether the mover locks facing to its pre-dispatch value rather than
    /// adopting `direction`: `ForcedMovement_Slide` sets
    /// `facingDirectionLocked` before moving, so `SetObjectEventDirection`
    /// updates only `movementDirection`; the four `ForcedMovement_Walk*`
    /// never set that lock (`field_player_avatar.c:486-503, 526-532`;
    /// `event_object_movement.c:2361-2370`).
    locks_facing: bool,
}

/// Returns the walk or slide mover a standing behavior dispatches
/// (`field_player_avatar.c:486-552, 975-982`), or `None` for every other
/// behavior [`is_forced_movement`] arms -- ice, currents, the Trick House
/// puzzle floor, the waterfall, the muddy slope, and the Secret Base mats,
/// which stay deferred.
#[must_use]
fn supported_forced_mover(standing_behavior: u8) -> Option<ForcedMover> {
    // `locks_facing` and `cadence.animation_disabled` coincide for every
    // mover this slice supports: both are set together, only by
    // `ForcedMovement_Slide` (`field_player_avatar.c:526-532`).
    let (direction, cadence, locks_facing) = match standing_behavior {
        MB_WALK_EAST => (Direction::East, TransitCadence::WALK, false),
        MB_WALK_WEST => (Direction::West, TransitCadence::WALK, false),
        MB_WALK_NORTH => (Direction::North, TransitCadence::WALK, false),
        MB_WALK_SOUTH => (Direction::South, TransitCadence::WALK, false),
        MB_SLIDE_EAST => (Direction::East, TransitCadence::SLIDE, true),
        MB_SLIDE_WEST => (Direction::West, TransitCadence::SLIDE, true),
        MB_SLIDE_NORTH => (Direction::North, TransitCadence::SLIDE, true),
        MB_SLIDE_SOUTH => (Direction::South, TransitCadence::SLIDE, true),
        _ => return None,
    };
    Some(ForcedMover {
        direction,
        cadence,
        locks_facing,
    })
}

/// Returns the direction a standing *deferred* behavior's `ForcedMovement_*`
/// handler collision-tests (`field_player_avatar.c:486-580`),
/// `movement_direction` for the two `ForcedMovement_Slip` tiles
/// (`:473-484`) and `None` for the Secret Base mats, which never
/// collision-check (`:555-565`). `MB_WALK_*` and `MB_SLIDE_*` are dispatched
/// by [`supported_forced_mover`] before `step` ever reaches this deferred
/// path.
fn forced_movement_direction(
    standing_behavior: u8,
    movement_direction: Direction,
) -> Option<Direction> {
    match standing_behavior {
        MB_EASTWARD_CURRENT => Some(Direction::East),
        MB_WESTWARD_CURRENT => Some(Direction::West),
        MB_NORTHWARD_CURRENT => Some(Direction::North),
        MB_SOUTHWARD_CURRENT | MB_WATERFALL | MB_MUDDY_SLOPE => Some(Direction::South),
        MB_ICE | MB_TRICK_HOUSE_PUZZLE_8_FLOOR => Some(movement_direction),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overworld::map_runtime::MapRuntime;
    use crate::overworld::metatile_behavior::{MB_IMPASSABLE_SOUTH_AND_NORTH, MB_SLIDE_EAST};
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

    /// A `width` x `height` map of plain ground whose only variation is the
    /// collision bits `collision_at` assigns per cell.
    fn flat_map_runtime(
        width: u16,
        height: u16,
        collision_at: impl Fn(u16, u16) -> u8,
    ) -> MapRuntime<'static> {
        let (bytes, header, events) = flat_runtime(width, height, collision_at);
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width,
            height,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let bytes: &'static [u8] = Box::leak(bytes.into_boxed_slice());
        MapRuntime::new(
            assets::MapId("MAP_TEST"),
            Box::leak(Box::new(header)),
            Box::leak(Box::new(events)),
            layout.grid(bytes).unwrap(),
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        )
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
        landing_behavior: u8,
    }

    impl ConnectedMapData for SingleConnectedMap {
        fn dimensions(&self, map: MapId) -> Option<(u16, u16)> {
            (map == self.id).then_some(self.dimensions)
        }

        fn metatile_cell(&self, map: MapId, x: i32, y: i32) -> Option<MetatileCell> {
            (map == self.id && (x, y) == self.landing_position).then_some(self.landing_cell)
        }

        fn metatile_behavior(&self, map: MapId, x: i32, y: i32) -> Option<u8> {
            (map == self.id && (x, y) == self.landing_position).then_some(self.landing_behavior)
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
        assert_eq!(player.step_direction(), None);

        let runtime = flat_map_runtime(5, 5, |_, _| 0);
        let mut player = PlayerState::new((2, 2), 3, Direction::South);
        player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS);
        let progress = player.step_progress();
        assert_eq!(player.step_direction(), Some(Direction::South));
        player.face(Direction::West);
        assert_eq!(player.facing(), Direction::West);
        assert_eq!(player.position(), (2, 3), "the committed step is untouched");
        assert!(player.in_transit());
        assert_eq!(player.step_progress(), progress);
        assert_eq!(
            player.step_direction(),
            Some(Direction::South),
            "facing does not replace the committed step direction"
        );
    }

    #[test]
    fn fresh_pressing_the_facing_direction_steps_immediately() {
        let runtime = flat_map_runtime(5, 5, |_, _| 0);

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
        let runtime = flat_map_runtime(5, 5, |_, _| 0);

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
        player.tick();

        // The turn frame itself spends one of TURN_IN_PLACE_FRAMES, so the
        // held direction stays swallowed through frames 2..=8.
        assert_eq!(player.turn_frames_remaining(), TURN_IN_PLACE_FRAMES - 1);
        for frame in 2..=8 {
            let outcome = player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS);
            assert_eq!(
                outcome,
                StepOutcome::Idle,
                "frame {frame} is still inside the turn's busy window"
            );
            assert_eq!(player.position(), (2, 2), "frame {frame} must not move");
            player.tick();
            assert_eq!(
                player.turn_frames_remaining(),
                TURN_IN_PLACE_FRAMES - frame,
                "frame {frame} of the window leaves the rest of it to run"
            );
        }

        let outcome = player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS);
        assert_eq!(
            outcome,
            StepOutcome::Advanced {
                from: (2, 2),
                to: (3, 2),
            },
            "the step begins only once the turn's busy window has drained"
        );
    }

    #[test]
    fn changing_direction_mid_movement_steps_immediately_without_a_turn_frame() {
        let runtime = flat_map_runtime(5, 5, |_, _| 0);

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
        let runtime = flat_map_runtime(5, 5, |_, _| 0);

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
        let runtime = flat_map_runtime(5, 5, |_, _| 0);

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
        let runtime = flat_map_runtime(5, 5, |_, _| 0);

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
        let runtime = flat_map_runtime(5, 5, |_, y| u8::from(y == 3));

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
    fn a_multi_level_origin_defers_the_elevation_update_to_the_finished_step() {
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
            "the begin-step pass skips the whole adoption while the origin is \
             ELEVATION_MULTI_LEVEL, even though the destination (7) is ordinary"
        );
        for _ in 0..WALK_FRAMES_PER_TILE {
            player.tick();
        }
        assert!(!player.in_transit());
        assert_eq!(
            (player.elevation(), player.previous_elevation()),
            (7, 7),
            "the finish-step pass runs over the landing cell alone, so both \
             fields adopt the ordinary destination once the crossing ends"
        );
    }

    /// A multi-level cell crossed between a transition cell and an ordinary
    /// one leaves the settled pair on the landing cell's elevation, the pair
    /// a save then holds and a continue accepts on an ordinary cell.
    #[test]
    fn settled_elevation_follows_the_landing_cell_after_crossing_multi_level() {
        // Column x=2: y=1 elev 3, y=2 elev 0, y=3 elev 15, y=4 elev 7.
        let (_, header, events) = flat_runtime(5, 6, |_, _| 0);
        let mut bytes = Vec::new();
        for y in 0..6u16 {
            for x in 0..5u16 {
                let elevation = match (x, y) {
                    (2, 2) => 0,
                    (2, 3) => super::super::collision::ELEVATION_MULTI_LEVEL,
                    (2, 4) => 7,
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
            width: 5,
            height: 6,
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
        let mut player = PlayerState::new((2, 1), 3, Direction::South);
        let mut settled = Vec::new();
        for _ in 0..3 {
            assert!(matches!(
                player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
                StepOutcome::Advanced { .. }
            ));
            for _ in 0..WALK_FRAMES_PER_TILE {
                player.tick();
            }
            settled.push((player.elevation(), player.previous_elevation()));
        }
        assert_eq!(player.position(), (2, 4));
        assert!(!player.in_transit());
        assert_eq!(
            settled,
            vec![(0, 3), (0, 3), (7, 7)],
            "transition keeps the retained 3, multi-level retains both, and the \
             ordinary landing settles both halves on 7"
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
            landing_behavior: MB_NORMAL,
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
            landing_behavior: MB_NORMAL,
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
            landing_behavior: MB_NORMAL,
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
            landing_behavior: MB_NORMAL,
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
            landing_behavior: MB_NORMAL,
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
        let runtime = flat_map_runtime(5, 5, |_, _| 0);

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
        player.tick();

        // Drain the turn's busy window (`TURN_IN_PLACE_FRAMES` doc comment)
        // before the held direction can be attempted as a step at all.
        for _ in 2..=8 {
            assert_eq!(
                player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
                StepOutcome::Idle,
                "still inside the turn's busy window"
            );
            player.tick();
        }

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

    fn slide_east_runtime() -> MapRuntime<'static> {
        let mut bytes = Vec::new();
        for y in 0..5u16 {
            for x in 0..5u16 {
                let raw = MetatileCell {
                    metatile_id: u16::from((x, y) == (2, 2)),
                    collision: 0,
                    elevation: 3,
                }
                .pack();
                bytes.extend_from_slice(&raw.to_le_bytes());
            }
        }
        let attrs = [
            u16::from(MB_NORMAL).to_le_bytes(),
            u16::from(MB_SLIDE_EAST).to_le_bytes(),
        ]
        .concat();
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 5,
            height: 5,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
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

    /// Upstream dispatches forced movement from the metatile the player is
    /// already standing on before it ever reads the keypad, and
    /// `DoForcedMovement` moves in that tile's own direction on a clear
    /// route regardless of the caller's held direction
    /// (`field_player_avatar.c:342-347`, `:407-470`) `(behavioral-fidelity)`.
    #[test]
    fn a_forced_movement_tile_does_not_honour_the_callers_direction() {
        let runtime = slide_east_runtime();

        let mut player = PlayerState::new((1, 2), 3, Direction::East);
        assert_eq!(
            player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Advanced {
                from: (1, 2),
                to: (2, 2),
            },
            "fixture precondition: the slide tile is entered like ordinary ground"
        );
        for _ in 0..WALK_FRAMES_PER_TILE {
            player.tick();
        }

        assert_eq!(
            player.step(Some(Direction::West), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Advanced {
                from: (2, 2),
                to: (3, 2),
            },
            "the slide tile dispatches its own eastward direction, never the \
             caller's held westward one"
        );
        assert_eq!(
            player.facing(),
            Direction::East,
            "the tile's own direction is what actually moved the player, not \
             the caller's West poll"
        );
        assert_eq!(player.position(), (3, 2));
    }

    /// A blocked forced route falls through to the keypad on the very poll
    /// that found it blocked -- `DoForcedMovement`'s collision branch never
    /// denies the poll outright, it just skips `moveFunc` and lets
    /// `MovePlayerAvatarUsingKeypadInput` run (`field_player_avatar.c:344-349`,
    /// `:443-462`) -- but the guard itself stays armed, since
    /// `GetForcedMovementByMetatileBehavior` re-reads the standing tile
    /// fresh every poll rather than latching a one-time verdict (`:409-426`):
    /// only leaving the tile or a successful dispatch consumes it, so a
    /// later poll retries the same dispatch. A blocked dispatch attempt
    /// leaves `runningState` (this port's movement streak) untouched, same
    /// as `DoForcedMovement`'s collision branch (`:443-462`), so the retry
    /// still turns in place first rather than stepping immediately
    /// `(behavioral-fidelity)`.
    #[test]
    fn a_blocked_forced_movement_tile_stays_armed_and_still_turns_after_an_idle_poll() {
        let runtime = blocked_slide_east_runtime();

        let mut player = PlayerState::new((1, 2), 3, Direction::East);
        assert_eq!(
            player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Advanced {
                from: (1, 2),
                to: (2, 2),
            },
            "fixture precondition: the slide tile is entered like ordinary ground, \
             which is what arms the guard"
        );
        for _ in 0..WALK_FRAMES_PER_TILE {
            player.tick();
        }
        assert!(
            player.forced_movement_armed(),
            "fixture precondition: the slide tile arms the guard on landing"
        );

        assert_eq!(
            player.step(None, &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Idle,
            "the forced eastward route is blocked at (3, 2), so the no-input \
             dispatch attempt fails and idles rather than moving"
        );
        assert!(
            player.forced_movement_armed(),
            "a blocked forced route must not clear the guard while the player \
             still stands on the same tile, so a later poll can retry once \
             whatever blocked it no longer does"
        );

        assert_eq!(
            player.step(Some(Direction::West), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Turned(Direction::West),
            "the retried dispatch is still blocked and never touched the \
             movement streak the idle poll ended, so this poll turns in \
             place like any other direction change from standstill"
        );
        assert_eq!(player.position(), (2, 2));
    }

    /// A dispatched forced tile's own direction sets the avatar's facing --
    /// not the direction it happened to be entered from, and not the
    /// caller's held poll -- `ForcedMovement_WalkEast` never sets
    /// `facingDirectionLocked`, so `SetObjectEventDirection` adopts the
    /// dispatched direction (`field_player_avatar.c:486-503`,
    /// `event_object_movement.c:2361-2370`). Entering from the north
    /// (facing South) and dispatching a `MB_WALK_EAST` proves both
    /// independently `(behavioral-fidelity)`.
    #[test]
    fn a_dispatched_walk_tile_faces_its_own_direction_not_the_entrants() {
        let runtime = walk_east_runtime();

        let mut player = PlayerState::new((2, 1), 3, Direction::South);
        assert_eq!(
            player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Advanced {
                from: (2, 1),
                to: (2, 2),
            },
            "fixture precondition: the walk tile is entered like ordinary \
             ground, from the north this time"
        );
        for _ in 0..WALK_FRAMES_PER_TILE {
            player.tick();
        }
        assert_eq!(
            player.facing(),
            Direction::South,
            "fixture precondition: the entry step still faces the entered direction"
        );

        assert_eq!(
            player.step(Some(Direction::West), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Advanced {
                from: (2, 2),
                to: (3, 2),
            },
            "the walk tile's own eastward direction dispatches regardless of \
             the entry direction or the caller's held West"
        );
        assert_eq!(
            player.facing(),
            Direction::East,
            "the dispatched mover's own direction sets facing, not the \
             direction the player entered from or the caller's held poll"
        );
    }

    /// Unlike a walk tile, `ForcedMovement_Slide` sets
    /// `facingDirectionLocked` before moving, so `SetObjectEventDirection`
    /// updates only `movementDirection`, leaving the sprite's rendered
    /// facing exactly where it was (`field_player_avatar.c:526-532`,
    /// `event_object_movement.c:2361-2370`). Entering from the north
    /// (facing South) and dispatching a `MB_SLIDE_EAST` proves the avatar
    /// moves east while still rendering as facing south
    /// `(behavioral-fidelity)`.
    #[test]
    fn a_dispatched_slide_tile_keeps_the_entrants_facing_locked() {
        let runtime = slide_east_runtime();

        let mut player = PlayerState::new((2, 1), 3, Direction::South);
        assert_eq!(
            player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Advanced {
                from: (2, 1),
                to: (2, 2),
            },
            "fixture precondition: the slide tile is entered like ordinary \
             ground, from the north this time"
        );
        for _ in 0..WALK_FRAMES_PER_TILE {
            player.tick();
        }
        assert_eq!(
            player.facing(),
            Direction::South,
            "fixture precondition: the entry step still faces the entered direction"
        );

        assert_eq!(
            player.step(Some(Direction::West), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Advanced {
                from: (2, 2),
                to: (3, 2),
            },
            "the slide tile's own eastward direction dispatches regardless of \
             the entry direction or the caller's held West"
        );
        assert_eq!(
            player.facing(),
            Direction::South,
            "a slide locks the rendered facing to its pre-dispatch value: the \
             avatar moves east while still facing south"
        );
    }

    /// `ForcedMovement_Slide`'s `disableAnim` leaves the sprite paused after
    /// the crossing (`field_player_avatar.c:525-531`); the next keypad poll or
    /// scripted facing restarts the animation
    /// (`event_object_movement.c:7302-7313`) `(behavioral-fidelity)`.
    #[test]
    fn a_finished_slide_holds_its_pose_until_the_next_movement() {
        let runtime = slide_east_runtime();
        let mut player = PlayerState::new((2, 1), 3, Direction::South);
        player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS);
        for _ in 0..WALK_FRAMES_PER_TILE {
            player.tick();
        }
        player.step(None, &runtime, &no_connections, &NO_FLAGS);
        assert!(!player.slide_pose_held(), "the pose is held only at rest");
        for _ in 0..SLIDE_FRAMES_PER_TILE {
            player.tick();
        }
        assert!(!player.in_transit());
        assert!(player.slide_pose_held());
        player.tick();
        assert!(player.slide_pose_held(), "ticking alone keeps the pause");

        let mut idle = player;
        idle.step(None, &runtime, &no_connections, &NO_FLAGS);
        assert!(
            !idle.slide_pose_held(),
            "an idle poll faces the standing cell"
        );

        let mut stepping = player;
        stepping.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS);
        assert!(!stepping.slide_pose_held(), "a step restarts the animation");

        let mut turning = player;
        turning.step(Some(Direction::North), &runtime, &no_connections, &NO_FLAGS);
        assert!(!turning.slide_pose_held(), "a turn restarts the animation");

        let mut frozen = player;
        frozen.clear_turn_lock();
        assert!(
            !frozen.slide_pose_held(),
            "a field lock faces the standing cell"
        );

        let mut faced = player;
        faced.face(Direction::West);
        assert!(!faced.slide_pose_held(), "a scripted facing restarts it");
    }

    /// A walk-east tile, for proving facing follows dispatch on a family
    /// that never locks it (unlike [`slide_east_runtime`]).
    fn walk_east_runtime() -> MapRuntime<'static> {
        let mut bytes = Vec::new();
        for y in 0..5u16 {
            for x in 0..5u16 {
                let raw = MetatileCell {
                    metatile_id: u16::from((x, y) == (2, 2)),
                    collision: 0,
                    elevation: 3,
                }
                .pack();
                bytes.extend_from_slice(&raw.to_le_bytes());
            }
        }
        let attrs = [
            u16::from(MB_NORMAL).to_le_bytes(),
            u16::from(MB_WALK_EAST).to_le_bytes(),
        ]
        .concat();
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 5,
            height: 5,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
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

    /// A slide-east tile whose eastward destination is impassable.
    fn blocked_slide_east_runtime() -> MapRuntime<'static> {
        let mut bytes = Vec::new();
        for y in 0..5u16 {
            for x in 0..5u16 {
                let raw = MetatileCell {
                    metatile_id: u16::from((x, y) == (2, 2)),
                    collision: u8::from((x, y) == (3, 2)),
                    elevation: 3,
                }
                .pack();
                bytes.extend_from_slice(&raw.to_le_bytes());
            }
        }
        let attrs = [
            u16::from(MB_NORMAL).to_le_bytes(),
            u16::from(MB_SLIDE_EAST).to_le_bytes(),
        ]
        .concat();
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 5,
            height: 5,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
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

    /// A collision-blocked forced step falls through to the keypad
    /// (`field_player_avatar.c:344-348`, `:443-462`), so a slide-east tile
    /// with an impassable neighbour stays steerable `(behavioral-fidelity)`.
    #[test]
    fn a_collision_blocked_forced_direction_still_honours_manual_input() {
        let runtime = blocked_slide_east_runtime();
        let mut player = PlayerState::new((1, 2), 3, Direction::East);
        assert_eq!(
            player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Advanced {
                from: (1, 2),
                to: (2, 2),
            },
            "fixture precondition: the slide tile is entered like ordinary ground, \
             which is what arms the guard (issue #926's placement/resume finding)"
        );
        for _ in 0..WALK_FRAMES_PER_TILE {
            player.tick();
        }

        assert_eq!(
            player.step(Some(Direction::West), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Advanced {
                from: (2, 2),
                to: (1, 2),
            },
            "the forced eastward step is blocked by the impassable tile at (3, 2), \
             so upstream falls through to the keypad and the westward poll moves \
             the player off the slide tile"
        );
        assert_eq!(player.position(), (1, 2));
    }

    /// Table-driven forced-mover dispatch, cadence, and fallback
    /// regressions over every `MB_WALK_*`/`MB_SLIDE_*` tile.
    mod forced_movement_tests;

    /// Leading-foot parity across steps, turns, and rejected polls.
    mod step_parity_tests;

    /// Held-B running: its gates, cadence, and preserved interactions.
    mod running_tests;

    /// `forcedMove` closes only the `T_TILE_CENTER` arm of
    /// `FieldGetPlayerInput`'s gate, so input suppression lasts the landing's
    /// tile-center frame and `T_NOT_MOVING` admits the keypad from the next
    /// one (`field_control_avatar.c:93-113`) `(behavioral-fidelity)`.
    #[test]
    fn forced_input_suppression_lasts_only_the_landings_tile_center_frame() {
        let runtime = slide_east_runtime();
        let mut player = PlayerState::new((1, 2), 3, Direction::East);
        assert!(
            !player.field_input_suppressed(),
            "setup: a placed player is at rest, never at a landing's tile centre"
        );

        let _ = player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS);
        for _ in 0..WALK_FRAMES_PER_TILE {
            player.tick();
        }
        assert!(
            player.field_input_suppressed(),
            "the frame the crossing drains into is this port's T_TILE_CENTER, \
             where upstream skips the whole button block"
        );

        player.tick();
        assert!(
            !player.field_input_suppressed(),
            "T_NOT_MOVING from the next frame on: a player standing on a \
             forced tile must still reach START, A, and the warp polls"
        );
        assert!(
            player.forced_movement_armed(),
            "the movement guard is unaffected -- it still refuses manual steps \
             on the tile"
        );
    }

    /// A placement never arms the guard ([`PlayerState::new`]'s own doc), so
    /// a warp arrival or resumed save onto a forced tile whose direction is
    /// passable is controllable from its first poll `(behavioral-fidelity)`.
    #[test]
    fn a_player_placed_on_a_forced_movement_tile_is_not_immobile_forever() {
        let runtime = slide_east_runtime();
        let mut player = PlayerState::new((2, 2), 3, Direction::East);

        for _ in 0..60 {
            let _ = player.step(Some(Direction::West), &runtime, &no_connections, &NO_FLAGS);
            player.tick();
        }

        assert_ne!(
            player.position(),
            (2, 2),
            "a player placed on a forced-movement tile must not stay stuck on it \
             for a whole second of polls"
        );
    }

    /// Ice slips along the player's own movement direction
    /// (`ForcedMovement_Slip`, `field_player_avatar.c:473-484`), so an ice
    /// tile blocked that way still yields to the keypad
    /// `(behavioral-fidelity)`.
    #[test]
    fn ice_yields_to_the_keypad_when_the_players_own_facing_direction_is_blocked() {
        let mut bytes = Vec::new();
        for y in 0..5u16 {
            for x in 0..5u16 {
                let raw = MetatileCell {
                    metatile_id: u16::from((x, y) == (2, 2)),
                    collision: u8::from((x, y) == (3, 2)),
                    elevation: 3,
                }
                .pack();
                bytes.extend_from_slice(&raw.to_le_bytes());
            }
        }
        let attrs = [
            u16::from(MB_NORMAL).to_le_bytes(),
            u16::from(MB_ICE).to_le_bytes(),
        ]
        .concat();
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 5,
            height: 5,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let (_, header, events) = flat_runtime(1, 1, |_, _| 0);
        let bytes = Box::leak(bytes.into_boxed_slice());
        let attrs = Box::leak(attrs.into_boxed_slice());
        let header = Box::leak(Box::new(header));
        let events = Box::leak(Box::new(events));
        let runtime = MapRuntime::new(
            assets::MapId("MAP_TEST"),
            header,
            events,
            layout.grid(bytes).unwrap(),
            MetatileAttributeTable::new(attrs),
            MetatileAttributeTable::new(&[]),
        );

        let mut player = PlayerState::new((1, 2), 3, Direction::East);
        assert_eq!(
            player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Advanced {
                from: (1, 2),
                to: (2, 2),
            },
            "fixture precondition: the ice tile is entered like ordinary ground, \
             which is what arms the guard"
        );
        for _ in 0..WALK_FRAMES_PER_TILE {
            player.tick();
        }
        assert_eq!(
            player.step(None, &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Idle,
            "fixture precondition: releasing input ends the movement streak"
        );

        // The blocked forced direction reaches ordinary keypad handling, so
        // this direction change turns in place like any other.
        assert_eq!(
            player.step(Some(Direction::West), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Turned(Direction::West),
            "the player's own eastward movement direction is the ice tile's forced \
             direction and it is blocked at (3, 2), so this poll must reach \
             ordinary keypad handling and turn instead of failing closed without \
             turning"
        );
    }

    /// A scripted look moves ice's forced direction with it, so facing the
    /// sibling test's clear direction closes the keypad fallback it exercises
    /// `(behavioral-fidelity)`.
    #[test]
    fn a_scripted_face_carries_ices_forced_direction_with_it() {
        let mut bytes = Vec::new();
        for y in 0..5u16 {
            for x in 0..5u16 {
                let raw = MetatileCell {
                    metatile_id: u16::from((x, y) == (2, 2)),
                    collision: u8::from((x, y) == (3, 2)),
                    elevation: 3,
                }
                .pack();
                bytes.extend_from_slice(&raw.to_le_bytes());
            }
        }
        let attrs = [
            u16::from(MB_NORMAL).to_le_bytes(),
            u16::from(MB_ICE).to_le_bytes(),
        ]
        .concat();
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 5,
            height: 5,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let (_, header, events) = flat_runtime(1, 1, |_, _| 0);
        let bytes = Box::leak(bytes.into_boxed_slice());
        let attrs = Box::leak(attrs.into_boxed_slice());
        let header = Box::leak(Box::new(header));
        let events = Box::leak(Box::new(events));
        let runtime = MapRuntime::new(
            assets::MapId("MAP_TEST"),
            header,
            events,
            layout.grid(bytes).unwrap(),
            MetatileAttributeTable::new(attrs),
            MetatileAttributeTable::new(&[]),
        );

        let mut player = PlayerState::new((1, 2), 3, Direction::East);
        assert_eq!(
            player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Advanced {
                from: (1, 2),
                to: (2, 2),
            },
            "fixture precondition: the ice tile is entered like ordinary ground, \
             which is what arms the guard"
        );
        for _ in 0..WALK_FRAMES_PER_TILE {
            player.tick();
        }
        assert_eq!(
            player.step(None, &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Idle,
            "fixture precondition: releasing input ends the movement streak"
        );

        player.face(Direction::North);
        assert_eq!(
            player.facing(),
            Direction::North,
            "setup: the scripted look takes effect"
        );

        assert_eq!(
            player.step(Some(Direction::West), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Blocked {
                direction: Direction::West,
                collision: Collision::Impassable,
            },
            "the scripted look carried movement_direction north with it, and \
             north is clear, so the forced direction is no longer blocked and \
             the guard must refuse this poll -- not fall through to the keypad \
             on the pre-face east"
        );
        assert_eq!(
            player.facing(),
            Direction::North,
            "a refused poll on an armed forced tile leaves facing where the \
             script put it"
        );
    }

    /// The two Secret Base mats never collision-check, returning TRUE
    /// unconditionally (`field_player_avatar.c:555-565`), so they refuse every
    /// manual poll even with every direction clear `(behavioral-fidelity)`.
    fn assert_secret_base_mat_never_yields_to_the_keypad(mat_behavior: u8) {
        let mut bytes = Vec::new();
        for y in 0..5u16 {
            for x in 0..5u16 {
                let raw = MetatileCell {
                    metatile_id: u16::from((x, y) == (2, 2)),
                    collision: 0,
                    elevation: 3,
                }
                .pack();
                bytes.extend_from_slice(&raw.to_le_bytes());
            }
        }
        let attrs = [
            u16::from(MB_NORMAL).to_le_bytes(),
            u16::from(mat_behavior).to_le_bytes(),
        ]
        .concat();
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 5,
            height: 5,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let (_, header, events) = flat_runtime(1, 1, |_, _| 0);
        let bytes = Box::leak(bytes.into_boxed_slice());
        let attrs = Box::leak(attrs.into_boxed_slice());
        let header = Box::leak(Box::new(header));
        let events = Box::leak(Box::new(events));
        let runtime = MapRuntime::new(
            assets::MapId("MAP_TEST"),
            header,
            events,
            layout.grid(bytes).unwrap(),
            MetatileAttributeTable::new(attrs),
            MetatileAttributeTable::new(&[]),
        );

        let mut player = PlayerState::new((1, 2), 3, Direction::East);
        assert_eq!(
            player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Advanced {
                from: (1, 2),
                to: (2, 2),
            },
            "fixture precondition: the mat tile is entered like ordinary ground, \
             which is what arms the guard"
        );
        for _ in 0..WALK_FRAMES_PER_TILE {
            player.tick();
        }

        assert_eq!(
            player.step(Some(Direction::West), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Blocked {
                direction: Direction::West,
                collision: super::super::collision::Collision::Impassable,
            },
            "the mat has no derivable forced direction to collision-test, so it \
             must never fall through to the keypad"
        );
        assert_eq!(player.position(), (2, 2));
    }

    #[test]
    fn the_secret_base_spin_mat_never_yields_to_the_keypad_even_when_every_direction_is_clear() {
        assert_secret_base_mat_never_yields_to_the_keypad(
            super::super::metatile_behavior::MB_SECRET_BASE_SPIN_MAT,
        );
    }

    #[test]
    fn the_secret_base_jump_mat_never_yields_to_the_keypad_even_when_every_direction_is_clear() {
        assert_secret_base_mat_never_yields_to_the_keypad(
            super::super::metatile_behavior::MB_SECRET_BASE_JUMP_MAT,
        );
    }

    /// `MAP_SOUTH`'s landing tile carries a southward current (Route
    /// 132/133/134's connected water does at the seam), which upstream
    /// classifies on the standing tile every `PlayerStep`, crossing or not
    /// (`field_player_avatar.c:332-349`) `(behavioral-fidelity)`.
    fn southward_current_landing_runtime() -> MapRuntime<'static> {
        let mut bytes = Vec::new();
        for y in 0..5u16 {
            for x in 0..5u16 {
                let raw = MetatileCell {
                    metatile_id: u16::from((x, y) == (2, 0)),
                    collision: 0,
                    elevation: 3,
                }
                .pack();
                bytes.extend_from_slice(&raw.to_le_bytes());
            }
        }
        let attrs = [
            u16::from(MB_NORMAL).to_le_bytes(),
            u16::from(MB_SOUTHWARD_CURRENT).to_le_bytes(),
        ]
        .concat();
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_SOUTH"),
            name: "MapSouth",
            width: 5,
            height: 5,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let (_, mut header, events) = flat_runtime(1, 1, |_, _| 0);
        header.id = MapId("MAP_SOUTH");
        let bytes = Box::leak(bytes.into_boxed_slice());
        let attrs = Box::leak(attrs.into_boxed_slice());
        let header = Box::leak(Box::new(header));
        let events = Box::leak(Box::new(events));
        MapRuntime::new(
            MapId("MAP_SOUTH"),
            header,
            events,
            layout.grid(bytes).unwrap(),
            MetatileAttributeTable::new(attrs),
            MetatileAttributeTable::new(&[]),
        )
    }

    #[test]
    fn crossing_a_connection_onto_a_current_tile_still_refuses_the_keypad() {
        let runtime = south_connected_runtime();
        let maps = SingleConnectedMap {
            id: MapId("MAP_SOUTH"),
            dimensions: (5, 5),
            landing_position: (2, 0),
            landing_cell: MetatileCell {
                metatile_id: 1,
                collision: 0,
                elevation: 3,
            },
            landing_behavior: MB_SOUTHWARD_CURRENT,
        };

        let mut player = PlayerState::new((2, 4), 3, Direction::South);
        assert_eq!(
            player.step(Some(Direction::South), &runtime, &maps, &NO_FLAGS),
            StepOutcome::Crossed {
                to_map: MapId("MAP_SOUTH"),
                to_position: (2, 0),
            }
        );
        assert!(
            player.forced_movement_armed(),
            "the tile the crossing landed on is a southward current"
        );

        let landed = southward_current_landing_runtime();
        for _ in 0..WALK_FRAMES_PER_TILE {
            player.tick();
        }

        assert_eq!(
            player.step(Some(Direction::East), &landed, &no_connections, &NO_FLAGS),
            StepOutcome::Blocked {
                direction: Direction::East,
                collision: Collision::Impassable,
            },
            "the keypad must not steer off a current tile reached by a crossing"
        );
        assert_eq!(player.position(), (2, 0));
    }

    /// A resolver that can decode a landing cell but not classify it has
    /// broken `ConnectedMapData`'s contract; the crossing fails closed.
    #[derive(Debug, Clone, Copy)]
    struct UnclassifiedConnectedMap(SingleConnectedMap);

    impl ConnectedMapData for UnclassifiedConnectedMap {
        fn dimensions(&self, map: MapId) -> Option<(u16, u16)> {
            self.0.dimensions(map)
        }

        fn metatile_cell(&self, map: MapId, x: i32, y: i32) -> Option<MetatileCell> {
            self.0.metatile_cell(map, x, y)
        }

        fn metatile_behavior(&self, _map: MapId, _x: i32, _y: i32) -> Option<u8> {
            None
        }
    }

    #[test]
    fn a_crossing_whose_landing_tile_cannot_be_classified_is_refused() {
        let runtime = south_connected_runtime();
        let maps = UnclassifiedConnectedMap(SingleConnectedMap {
            id: MapId("MAP_SOUTH"),
            dimensions: (5, 5),
            landing_position: (2, 0),
            landing_cell: MetatileCell {
                metatile_id: 1,
                collision: 0,
                elevation: 3,
            },
            landing_behavior: MB_NORMAL,
        });

        let mut player = PlayerState::new((2, 4), 3, Direction::South);
        assert_eq!(
            player.step(Some(Direction::South), &runtime, &maps, &NO_FLAGS),
            StepOutcome::Blocked {
                direction: Direction::South,
                collision: Collision::Impassable,
            }
        );
        assert_eq!(player.position(), (2, 4));
    }
}
