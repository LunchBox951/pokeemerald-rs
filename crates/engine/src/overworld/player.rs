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
#[cfg(test)]
use super::{collision, metatile_behavior, object_event};
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

/// Frames a blocked step's slow in-place walk lasts
/// (`MovementAction_WalkInPlaceSlow*_Step0` starts its counter at 32,
/// `event_object_movement.c:5724-5735`).
pub const BUMP_IN_PLACE_FRAMES: u8 = 32;

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
    /// Frames left in a blocked step's interruptible slow in-place walk;
    /// zero when none is held.
    bump_frames_remaining: u8,
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
    /// Collision and render elevations both start at `elevation`. Only a
    /// manual MOVING attempt (landed or blocked) arms
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
            bump_frames_remaining: 0,
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
        self.bump_frames_remaining = 0;
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
        self.bump_frames_remaining = 0;
    }

    /// Returns whether the standing tile is in upstream's forced-movement
    /// dispatch set, `sForcedMovementTestFuncs`
    /// (`field_player_avatar.c:412-427`). `supported_forced_mover`
    /// dispatches a `MB_WALK_*`/`MB_SLIDE_*` tile as real movement; every
    /// other armed tile still refuses manual steps. Armed only by a manual
    /// step attempt (including a blocked one), never by placement.
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

    /// Drops a finished run's held neutral pose, as a field lock does
    /// (`ScriptContext` and battle starts reset the avatar through
    /// `SetPlayerAvatarTransitionFlags`, `field_player_avatar.c:150-162`).
    pub fn release_run_pose(&mut self) {
        if self.run_pose_held() {
            self.rest_pose = RestPose::Standing;
        }
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

    /// Returns whether a blocked step's slow in-place walk is held. Polls
    /// that do not interrupt it are swallowed (`TryInterruptObjectEventSpecialAnim`,
    /// `field_player_avatar.c:353-382`).
    #[must_use]
    pub const fn bump_active(&self) -> bool {
        self.bump_frames_remaining > 0
    }

    /// Drops a held bump, as a field lock does (`PlayerFreeze` forces a face
    /// action over it, `field_player_avatar.c:1039-1046`).
    pub const fn cancel_bump(&mut self) {
        self.bump_frames_remaining = 0;
    }

    /// Returns whether the held bump shows its forward foot: the first half
    /// of [`BUMP_IN_PLACE_FRAMES`], of the animation's two foot cells and two
    /// standing cells (`sAnim_Go*`, `object_event_anims.h:202-235`).
    #[must_use]
    pub const fn bump_foot_forward(&self) -> bool {
        self.bump_frames_remaining >= BUMP_IN_PLACE_FRAMES / 2
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
        self.bump_frames_remaining = self.bump_frames_remaining.saturating_sub(1);
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
    /// `dispatch_forced_mover` for a
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

        // `TryInterruptObjectEventSpecialAnim` runs ahead of forced movement
        // and the keypad.
        let mut keep_foot = self.slide_pose_held();
        if self.bump_active() {
            match self.interrupt_bump(input, standing_behavior, runtime, maps, event_data) {
                Some(forward_foot) => keep_foot |= forward_foot,
                None => return StepOutcome::Idle,
            }
        }

        // Dispatched ahead of the keypad, even on a `None` poll; see
        // `step`'s "Forced movement" section for the contract.
        if self.forced_movement_armed {
            if let Some(mover) = supported_mover {
                if let Some(outcome) = self.dispatch_forced_mover(
                    mover,
                    standing_behavior,
                    keep_foot,
                    runtime,
                    maps,
                    event_data,
                ) {
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
        let slide_pose_held = keep_foot;
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
        // Upstream enters MOVING before collision resolves, so a blocked attempt
        // still arms the standing tile (`field_player_avatar.c:401-405`, `:583-595`).
        self.forced_movement_armed |= supported_forced_mover(standing_behavior).is_some();

        self.attempt_manual_step(
            direction,
            run_held,
            slide_pose_held,
            standing_behavior,
            runtime,
            maps,
            event_data,
        )
    }

    /// Resolves and starts the step a keypad poll asked for, or starts the
    /// collision bump when it is denied.
    #[expect(
        clippy::too_many_arguments,
        reason = "one blocked-step attempt reads every input the collision mover does"
    )]
    fn attempt_manual_step(
        &mut self,
        direction: Direction,
        run_held: bool,
        slide_pose_held: bool,
        standing_behavior: u8,
        runtime: &MapRuntime<'_>,
        maps: &impl ConnectedMapData,
        event_data: &EventData,
    ) -> StepOutcome {
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
                    Err(collision) => {
                        self.start_bump(slide_pose_held);
                        StepOutcome::Blocked {
                            direction,
                            collision,
                        }
                    }
                }
            }
            Err(collision) => {
                self.start_bump(slide_pose_held);
                StepOutcome::Blocked {
                    direction,
                    collision,
                }
            }
        }
    }

    /// Resolves a poll against the held bump: a neutral poll and a repeat of
    /// the still-blocked direction are swallowed (`None`); any other poll
    /// cancels it, reporting whether the forward foot showed so the next
    /// action keeps that foot (`field_player_avatar.c:353-382`).
    fn interrupt_bump(
        &mut self,
        input: Option<Direction>,
        standing_behavior: u8,
        runtime: &MapRuntime<'_>,
        maps: &impl ConnectedMapData,
        event_data: &EventData,
    ) -> Option<bool> {
        let direction = input?;
        if direction == self.movement_direction
            && self.step_blocked(direction, standing_behavior, runtime, maps, event_data)
        {
            return None;
        }
        let forward_foot = self.bump_foot_forward();
        self.bump_frames_remaining = 0;
        Some(forward_foot)
    }

    /// Starts the slow in-place walk a collision plays
    /// (`PlayerNotOnBikeCollide`, `field_player_avatar.c:1011-1015`), facing
    /// the attempted direction already set by the caller.
    fn start_bump(&mut self, keep_foot: bool) {
        self.advance_step_parity(keep_foot);
        self.bump_frames_remaining = BUMP_IN_PLACE_FRAMES;
    }

    /// Returns whether a step in `direction` would be denied, read-only: the
    /// collision check without a collision's side effects
    /// (`CheckForPlayerAvatarStaticCollision`, `field_player_avatar.c:716-727`).
    fn step_blocked(
        &self,
        direction: Direction,
        standing_behavior: u8,
        runtime: &MapRuntime<'_>,
        maps: &impl ConnectedMapData,
        event_data: &EventData,
    ) -> bool {
        self.forced_direction_blocked(direction, runtime, maps, event_data, standing_behavior)
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
        self.bump_frames_remaining = 0;
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
        keep_foot: bool,
        runtime: &MapRuntime<'_>,
        maps: &impl ConnectedMapData,
        event_data: &EventData,
    ) -> Option<StepOutcome> {
        let direction = mover.direction;

        let landing = self.resolve_landing(direction, runtime, maps).ok()?;
        let from = self.position;
        let to = landing.position;
        let to_map = landing.to_map;
        self.try_start_resolved_step(
            direction,
            runtime,
            event_data,
            standing_behavior,
            landing,
            mover.cadence,
        )
        .ok()?;
        self.advance_step_parity(keep_foot);

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
    use crate::overworld::metatile_behavior::MB_SLIDE_EAST;
    use assets::{
        BattleScene, MapConnection, MapEvents, MapHeader, MapType, MetatileAttributeTable,
        MetatileCell, ObjectEvent, RegionMapSectionId, Weather,
    };

    /// Table-driven forced-mover dispatch, cadence, and fallback
    /// regressions over every `MB_WALK_*`/`MB_SLIDE_*` tile.
    mod forced_movement_tests;

    /// Forced-movement landings that suppress or yield to the keypad.
    mod forced_keypad_tests;

    /// Input handling, same-map collision, and elevation adoption.
    mod input_collision_tests;

    /// Directional impassability and elevation transitions.
    mod elevation_tests;

    /// Map-connection crossing refusals and landings.
    mod connection_crossing_tests;

    /// Object-event collision and its precedence against walls and elevation.
    mod object_collision_tests;

    /// Leading-foot parity across steps, turns, and rejected polls.
    mod step_parity_tests;

    mod bump_tests;
    /// Held-B running: its gates, cadence, and preserved interactions.
    mod running_tests;

    /// Shared map and player fixtures.
    mod support;
    use support::*;
}
