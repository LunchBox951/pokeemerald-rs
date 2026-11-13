//! `MovementType_LookAround` and `MovementType_FaceDownAndRight`: an object that turns on the
//! spot to a randomly drawn facing after a randomly drawn delay, never walking.
//!
//! Both run the same five-step table (`event_object_movement.c:2844-2892,3434-3481`), differing
//! only in the delay table, the facing table, and how a running player nearby picks the facing.

use assets::TrainerType;

use crate::overworld::direction::Direction;
use crate::overworld::player::TilePos;
use crate::rng::RandomSource;

use super::{FrameContext, LiveObjectEvent};

/// The data one random-facing movement type is built from.
pub(super) struct RandomFacing {
    /// `sMovementDelaysMedium` or `sMovementDelaysShort` (`:709,711`); indexed by `Random() % 4`.
    delays: [i16; 4],
    /// `gStandardDirections` or `gDownAndRightDirections`
    /// (`data/object_events/movement_type_func_tables.h:11,127`); indexed by `Random() & mask`
    /// with `mask` one less than the length.
    directions: &'static [Direction],
    /// `gGetVectorDirectionFuncs[RUNFOLLOW_*]`, the facing a running player nearby forces.
    encounter: fn(dx: i32, dy: i32) -> Direction,
}

/// `MovementType_LookAround` (`:2844-2892`).
pub(super) static LOOK_AROUND: RandomFacing = RandomFacing {
    delays: [32, 64, 96, 128],
    directions: &[
        Direction::South,
        Direction::North,
        Direction::West,
        Direction::East,
    ],
    encounter: vector_direction,
};

/// `MovementType_FaceDownAndRight` (`:3434-3481`).
pub(super) static FACE_DOWN_AND_RIGHT: RandomFacing = RandomFacing {
    delays: [32, 48, 64, 80],
    directions: &[Direction::South, Direction::East],
    encounter: east_south_vector_direction,
};

impl LiveObjectEvent {
    /// One call of `MovementType_{LookAround,FaceDownAndRight}_Step0` to `_Step4`; `true` runs the
    /// next step this frame.
    ///
    /// Step 2's face action completes on its first execution, since a single movement action is
    /// never held (`ObjectEventSetSingleMovement`, `:5042-5046`, and `FaceDirection`,
    /// `:5048-5055`), so the first callback frame draws the delay and a turn frame draws the
    /// facing then the next delay.
    pub(super) fn random_facing_step(
        &mut self,
        config: &RandomFacing,
        context: &FrameContext<'_>,
        rng: &mut impl RandomSource,
    ) -> bool {
        match self.type_func_id() {
            0 => {
                self.set_type_func_id(1);
                true
            }
            1 => {
                self.set_type_func_id(2);
                true
            }
            2 => {
                self.state.face_in_place();
                let index = usize::from(rng.next_u16()) % config.delays.len();
                self.set_movement_delay(config.delays[index]);
                self.set_type_func_id(3);
                false
            }
            3 => {
                if self.movement_delay_elapsed() || self.trainer_is_close_to_running_player(context)
                {
                    self.set_type_func_id(4);
                    true
                } else {
                    false
                }
            }
            _ => {
                let direction = self
                    .trainer_encounter_direction(config, context)
                    .unwrap_or_else(|| {
                        let index = usize::from(rng.next_u16()) & (config.directions.len() - 1);
                        config.directions[index]
                    });
                self.state.face(direction);
                self.set_type_func_id(1);
                true
            }
        }
    }

    /// `ObjectEventIsTrainerAndCloseToPlayer` (`:2632-2660`): a dashing player on a tile within
    /// the trainer's range square, for a `Normal` or `Buried` trainer. Alignment, line of sight,
    /// elevation, and whether the trainer was beaten are not consulted.
    fn trainer_is_close_to_running_player(&self, context: &FrameContext<'_>) -> bool {
        if !context.player_dashing {
            return false;
        }
        let template = self.template;
        if !matches!(
            template.trainer_type,
            TrainerType::Normal | TrainerType::Buried
        ) {
            return false;
        }
        let Ok(range) = template.trainer_sight_or_berry_tree_id.parse::<u8>() else {
            return false;
        };
        let (dx, dy) = offset_to_player(self.state.position(), context.player_position);
        let range = i32::from(range);
        dx.abs() <= range && dy.abs() <= range
    }

    /// `TryGetTrainerEncounterDirection` (`:2822-2841`): the facing toward a running player
    /// nearby, or `None` when there is none.
    fn trainer_encounter_direction(
        &self,
        config: &RandomFacing,
        context: &FrameContext<'_>,
    ) -> Option<Direction> {
        if !self.trainer_is_close_to_running_player(context) {
            return None;
        }
        let (dx, dy) = offset_to_player(self.state.position(), context.player_position);
        Some((config.encounter)(dx, dy))
    }
}

fn offset_to_player(object: TilePos, player: TilePos) -> (i32, i32) {
    (player.0 - object.0, player.1 - object.1)
}

/// `GetVectorDirection` (`:2663-2679`): the dominant axis, vertical on a tie, with south for a
/// zero offset on the chosen axis (east on the horizontal axis).
fn vector_direction(dx: i32, dy: i32) -> Direction {
    if dx.abs() > dy.abs() {
        if dx < 0 {
            Direction::West
        } else {
            Direction::East
        }
    } else if dy < 0 {
        Direction::North
    } else {
        Direction::South
    }
}

/// `GetLimitedVectorDirection_EastSouth` (`:2762-2779`): `GetVectorDirection`, with a north
/// result turned to east or south by the horizontal sign and a west result turned to south.
fn east_south_vector_direction(dx: i32, dy: i32) -> Direction {
    match vector_direction(dx, dy) {
        Direction::North if dx < 0 => Direction::South,
        Direction::North => Direction::East,
        Direction::West => Direction::South,
        direction => direction,
    }
}

#[cfg(test)]
#[path = "object_event_random_facing_tests.rs"]
mod tests;
