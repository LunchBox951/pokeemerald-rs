//! The Secret Base jump and spin mats' forced sequences.
//!
//! A standing mat tile dispatches `ForcedMovement_MatJump` or
//! `ForcedMovement_MatSpin` ahead of the keypad (`field_player_avatar.c:555-565`),
//! which hand off to `DoPlayerMatJump` and `DoPlayerMatSpin` (`:1544-1644`).
//! Both set `preventStep`, so every poll is consumed until the sequence ends.

use super::{Direction, MapRuntime, PlayerState, StepOutcome, TransitCadence};
use crate::overworld::map_runtime::ConnectedMapData;
use crate::overworld::metatile_behavior::{MB_SECRET_BASE_JUMP_MAT, MB_SECRET_BASE_SPIN_MAT};

/// Frames one in-place jump lasts (`DoJumpSpriteMovement`'s
/// `distanceToTime[JUMP_DISTANCE_IN_PLACE]`, `event_object_movement.c:8454-8482`).
pub const MAT_JUMP_FRAMES: u8 = 16;

/// In-place jumps the jump mat plays (`task->data[1]`, `field_player_avatar.c:1560-1568`).
const MAT_JUMPS: u8 = 2;

/// Faces the spin mat turns through before its exit step: sixteen of the
/// four-direction cycle, then two more to reach the opposite of the initial
/// direction (`PlayerAvatar_SecretBaseMatSpinStep1`, `:1599-1615`).
const SPIN_FACES: u8 = 18;

/// `sJumpY_High`, the in-place jump's height per update
/// (`event_object_movement.c:8424-8427`), negated to pixels raised.
const JUMP_LIFT: [u8; MAT_JUMP_FRAMES as usize] =
    [4, 6, 8, 10, 11, 12, 12, 12, 11, 10, 9, 8, 6, 4, 0, 0];

/// A running Secret Base mat sequence, consuming every keypad poll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForcedMat {
    /// The jump mat's in-place jumps (`PlayerAvatar_DoSecretBaseMatJump`).
    Jump {
        /// Zero-based index of the jump playing.
        jump: u8,
        /// Frames of that jump elapsed, up to [`MAT_JUMP_FRAMES`].
        frames: u8,
    },
    /// The spin mat's facing cycle (`PlayerAvatar_SecretBaseMatSpinStep0-3`).
    Spin {
        /// The movement direction the cycle started from.
        origin: Direction,
        /// Faces already turned, up to the cycle's final face.
        faces_turned: u8,
        /// Frames until the next face, or until the exit step after the last.
        frames_to_next: u8,
    },
}

impl ForcedMat {
    /// Pixels the avatar sprite is raised this frame: the jump mat's hop
    /// (`sJumpY_High`), and zero otherwise. Counts the frame that started
    /// each jump as elapsed once, like [`PlayerState::step_progress`].
    #[must_use]
    pub const fn jump_lift(self) -> u8 {
        match self {
            Self::Jump { frames, .. } => {
                let index = frames.saturating_sub(1) as usize;
                if index < JUMP_LIFT.len() {
                    JUMP_LIFT[index]
                } else {
                    0
                }
            }
            Self::Spin { .. } => 0,
        }
    }
}

/// `directions[objectEvent->movementDirection - 1]` (`field_player_avatar.c:1597`):
/// south turns west, west north, north east, east south.
const fn next_in_cycle(direction: Direction) -> Direction {
    match direction {
        Direction::South => Direction::West,
        Direction::West => Direction::North,
        Direction::North => Direction::East,
        Direction::East => Direction::South,
    }
}

const fn opposite(direction: Direction) -> Direction {
    match direction {
        Direction::South => Direction::North,
        Direction::North => Direction::South,
        Direction::West => Direction::East,
        Direction::East => Direction::West,
    }
}

/// Frames from face `faces_turned` until the next action: the face runs
/// the frame the task sees the previous action finish, then a `Delay`
/// (`MOVEMENT_ACTION_DELAY_1/1/2/4/8` indexed by completed laps,
/// `Step2`, `:1618-1631`) holds the next one back by one frame more than its
/// length. The last face has no delay and is followed by the exit step on
/// the next frame.
const fn frames_after_face(faces_turned: u8) -> u8 {
    match faces_turned {
        1..=7 => 2,
        8..=11 => 3,
        12..=15 => 5,
        16 | 17 => 9,
        _ => 1,
    }
}

/// Whether `behavior` is one of the two mats `dispatch_mat` runs.
pub(super) const fn is_mat(behavior: u8) -> bool {
    matches!(behavior, MB_SECRET_BASE_JUMP_MAT | MB_SECRET_BASE_SPIN_MAT)
}

impl PlayerState {
    /// Returns the running Secret Base mat sequence, or `None`. The avatar
    /// reads it for the jump pose; the keypad is consumed while it is `Some`.
    #[must_use]
    pub const fn forced_mat(&self) -> Option<ForcedMat> {
        self.forced_mat
    }

    /// Starts the standing mat's sequence, as `ForcedMovement_MatJump/Spin`
    /// do. Returns whether `standing_behavior` is a mat.
    pub(super) fn dispatch_mat(&mut self, standing_behavior: u8, keep_foot: bool) -> bool {
        match standing_behavior {
            MB_SECRET_BASE_JUMP_MAT => self.start_mat_jump(0, keep_foot),
            MB_SECRET_BASE_SPIN_MAT => self.turn_mat_spin(self.movement_direction, 0),
            _ => return false,
        }
        true
    }

    /// `ObjectEventSetHeldMovement(GetJumpInPlaceMovementAction(facingDirection))`:
    /// the jump faces the facing direction and alternates the walk cycle.
    fn start_mat_jump(&mut self, jump: u8, keep_foot: bool) {
        self.advance_step_parity(keep_foot);
        self.movement_direction = self.facing;
        self.rest_pose = super::RestPose::Standing;
        self.forced_mat = Some(ForcedMat::Jump { jump, frames: 0 });
    }

    /// Turns to the next face in the cycle from `origin`, `faces_turned`
    /// already done.
    fn turn_mat_spin(&mut self, origin: Direction, faces_turned: u8) {
        self.face(next_in_cycle(self.movement_direction));
        let faces_turned = faces_turned + 1;
        self.forced_mat = Some(ForcedMat::Spin {
            origin,
            faces_turned,
            frames_to_next: frames_after_face(faces_turned),
        });
    }

    /// Advances a running mat sequence by one poll. `None` hands the poll
    /// back to the normal flow: the jump mat has finished and control returns
    /// on this same frame.
    pub(super) fn step_forced_mat(
        &mut self,
        runtime: &MapRuntime<'_>,
        maps: &impl ConnectedMapData,
    ) -> Option<StepOutcome> {
        match self.forced_mat? {
            ForcedMat::Jump { jump, frames } => {
                if frames < MAT_JUMP_FRAMES {
                    return Some(StepOutcome::Idle);
                }
                if jump + 1 < MAT_JUMPS {
                    self.start_mat_jump(jump + 1, false);
                    return Some(StepOutcome::Idle);
                }
                // `PLAYER_AVATAR_FLAG_CONTROLLABLE` is set, so the mat is
                // not dispatched again until the next manual step attempt.
                self.forced_mat = None;
                self.forced_movement_armed = false;
                None
            }
            ForcedMat::Spin {
                origin,
                faces_turned,
                frames_to_next,
            } => {
                if frames_to_next > 0 {
                    return Some(StepOutcome::Idle);
                }
                if faces_turned < SPIN_FACES {
                    self.turn_mat_spin(origin, faces_turned);
                    return Some(StepOutcome::Idle);
                }
                self.forced_mat = None;
                Some(self.start_walk_slow(opposite(origin), runtime, maps))
            }
        }
    }

    /// `MovementAction_WalkSlow*`: one unchecked tile step at
    /// [`super::WALK_SLOW_FRAMES_PER_TILE`], the exit step's facing and movement
    /// direction `direction` (`InitNpcForWalkSlow`, `event_object_movement.c:5140-5155`).
    /// A direction with no tile at all turns without moving.
    fn start_walk_slow(
        &mut self,
        direction: Direction,
        runtime: &MapRuntime<'_>,
        maps: &impl ConnectedMapData,
    ) -> StepOutcome {
        self.facing = direction;
        self.movement_direction = direction;
        let Ok(landing) = self.resolve_landing(direction, runtime, maps) else {
            return StepOutcome::Idle;
        };
        let from = self.position;
        let to = landing.position;
        let to_map = landing.to_map;
        self.commit_resolved_step(direction, runtime, landing, TransitCadence::WALK_SLOW);
        self.advance_step_parity(false);
        match to_map {
            Some(to_map) => StepOutcome::Crossed {
                to_map,
                to_position: to,
            },
            None => StepOutcome::Advanced { from, to },
        }
    }

    /// Drains a running mat sequence's frame counter.
    pub(super) fn tick_forced_mat(&mut self) {
        match &mut self.forced_mat {
            Some(ForcedMat::Jump { frames, .. }) => *frames = (*frames + 1).min(MAT_JUMP_FRAMES),
            Some(ForcedMat::Spin { frames_to_next, .. }) => {
                *frames_to_next = frames_to_next.saturating_sub(1);
            }
            None => {}
        }
    }
}
