//! `MovementType_WanderAround`, `_WanderUpAndDown`, `_WanderLeftAndRight` and
//! `_WalkBackAndForth`: an object that walks inside its movement range, one normal-speed tile at
//! a time, either to a randomly drawn side after a randomly drawn pause (the wanderers) or back
//! and forth along its initial facing (the patrols).
//!
//! The wanderers run the seven-step table of `event_object_movement.c:2566-2629,2897-3029` and
//! the patrols the callback of `:3764-3822`; the callback table aliases all four `Walk*And*`
//! types to `WalkBackAndForth` (`:249-252`). The aliased `WanderDownAndUp` and
//! `WanderRightAndLeft` are not scheduled.
//!
//! A step that queues a movement action returns `TRUE` and the executing step runs in the same
//! callback, so the action's first frame is the frame it was queued (`begin_normal_walk`); the
//! executing step then only advances it on later frames, and returns `FALSE` throughout.

use assets::MovementType;

use crate::overworld::collision::ELEVATION_TRANSITION;
use crate::overworld::direction::Direction;
use crate::overworld::object_event::{
    collision_in_direction, initial_facing_direction, MovementRange, NpcCollision, StepKind,
};
use crate::rng::RandomSource;

use super::{FrameContext, Lifecycle, LiveObjectEvent, Occupant};

/// The data one wandering movement type is built from.
pub(super) struct Wander {
    /// `gStandardDirections`, `gUpAndDownDirections` or `gLeftAndRightDirections`
    /// (`data/object_events/movement_type_func_tables.h:11,45,57`); indexed by `Random() & mask`
    /// with `mask` one less than the length. Both vertical wanderers list south first.
    directions: &'static [Direction],
}

/// `MovementType_WanderAround` (`:2566-2629`).
pub(super) static WANDER_AROUND: Wander = Wander {
    directions: &[
        Direction::South,
        Direction::North,
        Direction::West,
        Direction::East,
    ],
};

/// `MovementType_WanderUpAndDown` (`:2897-2960`).
pub(super) static WANDER_UP_AND_DOWN: Wander = Wander {
    directions: &[Direction::South, Direction::North],
};

/// `MovementType_WanderLeftAndRight` (`:2965-3028`).
pub(super) static WANDER_LEFT_AND_RIGHT: Wander = Wander {
    directions: &[Direction::West, Direction::East],
};

/// The patrol's callback phases (its `sTypeFuncId`): drop any leftover action, pick and start
/// the next leg, and run that leg to its end.
const PATROL_START: u8 = 0;
const PATROL_NEXT_LEG: u8 = 1;
const PATROL_WALKING: u8 = 2;

/// `sMovementDelaysMedium` (`:709`), indexed by `Random() % 4`.
const WANDER_DELAYS: [i16; 4] = [32, 64, 96, 128];

/// `sMovementTypeHasRange` (`:307-349`): the types whose zero authored range is raised to one.
const fn movement_type_has_range(movement_type: MovementType) -> bool {
    use MovementType as M;
    matches!(
        movement_type,
        M::WanderAround
            | M::WanderUpAndDown
            | M::WanderDownAndUp
            | M::WanderLeftAndRight
            | M::WanderRightAndLeft
            | M::WalkUpAndDown
            | M::WalkDownAndUp
            | M::WalkLeftAndRight
            | M::WalkRightAndLeft
            | M::WalkSequenceUpRightLeftDown
            | M::WalkSequenceRightLeftDownUp
            | M::WalkSequenceDownUpRightLeft
            | M::WalkSequenceLeftDownUpRight
            | M::WalkSequenceUpLeftRightDown
            | M::WalkSequenceLeftRightDownUp
            | M::WalkSequenceDownUpLeftRight
            | M::WalkSequenceRightDownUpLeft
            | M::WalkSequenceLeftUpDownRight
            | M::WalkSequenceUpDownRightLeft
            | M::WalkSequenceRightLeftUpDown
            | M::WalkSequenceDownRightLeftUp
            | M::WalkSequenceRightUpDownLeft
            | M::WalkSequenceUpDownLeftRight
            | M::WalkSequenceLeftRightUpDown
            | M::WalkSequenceDownLeftRightUp
            | M::WalkSequenceUpLeftDownRight
            | M::WalkSequenceDownRightUpLeft
            | M::WalkSequenceLeftDownRightUp
            | M::WalkSequenceRightUpLeftDown
            | M::WalkSequenceUpRightDownLeft
            | M::WalkSequenceDownLeftUpRight
            | M::WalkSequenceLeftUpRightDown
            | M::WalkSequenceRightDownLeftUp
            | M::CopyPlayer
            | M::CopyPlayerOpposite
            | M::CopyPlayerCounterclockwise
            | M::CopyPlayerClockwise
            | M::CopyPlayerInGrass
            | M::CopyPlayerOppositeInGrass
            | M::CopyPlayerCounterclockwiseInGrass
            | M::CopyPlayerClockwiseInGrass
    )
}

impl LiveObjectEvent {
    /// `range.rangeX` and `range.rangeY` as `InitObjectEventStateFromTemplate` leaves them
    /// (`:1315-1328`): the template's, with a zero raised to one for a type that has a range.
    pub(super) fn admission_range(&self) -> (u8, u8) {
        let clamp = |range: u8| {
            if range == 0 && movement_type_has_range(self.state.template_movement_type()) {
                1
            } else {
                range
            }
        };
        (
            clamp(self.template.movement_range_x),
            clamp(self.template.movement_range_y),
        )
    }

    /// One call of `MovementType_Wander*_Step0` to `_Step6`; `true` runs the next step this
    /// frame.
    pub(super) fn wander_step(
        &mut self,
        config: &Wander,
        context: &FrameContext<'_>,
        rng: &mut impl RandomSource,
        occupants: &[Occupant],
    ) -> bool {
        match self.type_func_id() {
            0 => {
                self.state.clear_step();
                self.set_type_func_id(1);
                true
            }
            1 => {
                self.set_type_func_id(2);
                true
            }
            2 => {
                // The queued face action completes on its first execution (`FaceDirection`,
                // `:5048-5055`), and this step goes on to the delay wait in the same callback.
                self.state.face_in_place();
                let index = usize::from(rng.next_u16()) % WANDER_DELAYS.len();
                self.set_movement_delay(WANDER_DELAYS[index]);
                self.set_type_func_id(3);
                true
            }
            3 => {
                if self.movement_delay_elapsed() {
                    self.set_type_func_id(4);
                    true
                } else {
                    false
                }
            }
            4 => {
                let index = usize::from(rng.next_u16()) & (config.directions.len() - 1);
                let direction = config.directions[index];
                self.state.face(direction);
                let blocked = self.collision(direction, context, occupants).is_some();
                self.set_type_func_id(if blocked { 1 } else { 5 });
                true
            }
            5 => {
                self.start_walk(self.state.facing(), context);
                self.set_type_func_id(6);
                false
            }
            _ => {
                self.run_action_then(1, context);
                false
            }
        }
    }

    /// One call of `MovementType_WalkBackAndForth` (`:3764-3822`); `true` runs the next phase
    /// this frame.
    ///
    /// The patrol walks from its initial tile toward the facing its movement type starts with,
    /// and turns back at the edge of its range. Only leaving the range turns it back: any other
    /// collision makes it walk in place on that tile and try again. A leg that ends on one frame
    /// is followed by the next leg's first frame on the next.
    pub(super) fn walk_back_and_forth_step(
        &mut self,
        context: &FrameContext<'_>,
        occupants: &[Occupant],
    ) -> bool {
        match self.type_func_id() {
            PATROL_START => {
                self.state.clear_step();
                self.set_type_func_id(PATROL_NEXT_LEG);
                true
            }
            PATROL_NEXT_LEG => {
                let (direction, blocked) = self.next_patrol_leg(context, occupants);
                if blocked {
                    self.state.begin_normal_walk_in_place(direction);
                } else {
                    self.start_walk(direction, context);
                }
                self.set_type_func_id(PATROL_WALKING);
                false
            }
            _ => {
                self.run_action_then(PATROL_NEXT_LEG, context);
                false
            }
        }
    }

    /// The direction the patrol's next leg faces and whether something blocks it. The sequence
    /// index is zero on the outbound leg and counts the times the range has turned it back; a
    /// returning patrol that is back on its initial tile starts outbound again.
    fn next_patrol_leg(
        &mut self,
        context: &FrameContext<'_>,
        occupants: &[Occupant],
    ) -> (Direction, bool) {
        let outbound = initial_facing_direction(self.state.movement_type());
        if self.sequence_index() != 0 && self.is_at_initial_tile() {
            self.set_sequence_index(0);
        }
        let mut direction = if self.sequence_index() == 0 {
            outbound
        } else {
            outbound.opposite()
        };
        let mut collision = self.collision(direction, context, occupants);
        if collision == Some(NpcCollision::OutsideRange) {
            self.set_sequence_index(self.sequence_index().wrapping_add(1));
            direction = direction.opposite();
            collision = self.collision(direction, context, occupants);
        }
        (direction, collision.is_some())
    }

    /// `GetCollisionInDirection` (`:4650-4656`) for this entry against `occupants`, within the
    /// range it was admitted with.
    fn collision(
        &self,
        direction: Direction,
        context: &FrameContext<'_>,
        occupants: &[Occupant],
    ) -> Option<NpcCollision> {
        let Lifecycle::Active {
            initial,
            range_x,
            range_y,
            ..
        } = self.lifecycle
        else {
            return None;
        };
        collision_in_direction(
            &context.terrain,
            context.connected_maps,
            &self.state,
            Some(MovementRange {
                initial,
                x: range_x,
                y: range_y,
            }),
            direction,
            occupants.iter().copied(),
        )
    }

    /// `ObjectEventExecSingleMovementAction` for a running walk or walk in place
    /// (`:5031-5039`): one more frame of it, and on the frame it completes, the table moves to
    /// step `next`. A completed walk has settled its previous tile, so the object's elevation
    /// is updated from its destination.
    fn run_action_then(&mut self, next: u8, context: &FrameContext<'_>) {
        let walking = self
            .state
            .step()
            .is_some_and(|step| step.kind() == StepKind::Walk);
        if self.state.advance_step() {
            if walking {
                self.update_elevation_from_terrain(context);
            }
            self.set_type_func_id(next);
        }
    }

    /// Starts a normal walk toward `direction`; its first frame is this one. Beginning a step
    /// updates the object's elevation (`DoGroundEffects_OnBeginStep` through
    /// `UpdateObjectEventElevationAndPriority`, `:8078-8137`), so a later object's collision
    /// check this frame sees it.
    fn start_walk(&mut self, direction: Direction, context: &FrameContext<'_>) {
        self.state.begin_normal_walk(direction);
        self.update_elevation_from_terrain(context);
    }

    /// `ObjectEventUpdateElevation` (`:7759-7771`) against the cells under the object, the
    /// neighbours' band included. An object on no cell keeps its elevations, and a previous cell
    /// on none reads as a transition.
    fn update_elevation_from_terrain(&mut self, context: &FrameContext<'_>) {
        let cell = |(x, y): (i32, i32)| {
            context
                .terrain
                .metatile_cell_in_band(x, y, context.connected_maps)
                .map(|cell| cell.elevation)
        };
        if let Some(current) = cell(self.state.position()) {
            let previous = cell(self.state.previous_position()).unwrap_or(ELEVATION_TRANSITION);
            self.state.update_elevation(current, previous);
        }
    }

    fn is_at_initial_tile(&self) -> bool {
        match self.lifecycle {
            Lifecycle::Active { initial, .. } => self.state.position() == initial,
            Lifecycle::Unadmitted | Lifecycle::Despawned => false,
        }
    }

    fn sequence_index(&self) -> u8 {
        match self.lifecycle {
            Lifecycle::Active { sequence_index, .. } => sequence_index,
            Lifecycle::Unadmitted | Lifecycle::Despawned => 0,
        }
    }

    fn set_sequence_index(&mut self, next: u8) {
        if let Lifecycle::Active { sequence_index, .. } = &mut self.lifecycle {
            *sequence_index = next;
        }
    }
}

#[cfg(test)]
#[path = "object_event_wandering_tests.rs"]
mod tests;
