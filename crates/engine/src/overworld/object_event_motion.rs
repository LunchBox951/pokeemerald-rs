//! The normal-speed movement action of a live object event: a one-tile walk or a walk in place,
//! sixteen callback frames long.
//!
//! The action's first frame is the callback that starts it: a walk commits its destination tile
//! and moves the sprite one pixel (`InitMovementNormal`, `UpdateMovementNormal`,
//! `event_object_movement.c:5081-5124`). The sixteenth frame moves the last pixel, shifts the
//! previous tile onto the current one (`ShiftStillObjectEventCoords`, `:2162-2164`) and reports
//! the action done in that same callback (`ObjectEventExecSingleMovementAction`, `:5031-5039`),
//! so a movement type queues its next action on the following frame.

use crate::overworld::direction::Direction;

/// `sStepTimes` for normal speed: the frames a normal walk, and a normal walk in place, take.
pub const NORMAL_STEP_FRAMES: u8 = 16;

const TILE_PIXELS: i32 = 16;

/// Which normal-speed action is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepKind {
    /// `MovementAction_WalkNormal*`: the object moves to the next tile.
    Walk,
    /// `MovementAction_WalkInPlaceNormal*`: the object animates on its tile.
    WalkInPlace,
}

/// A running normal-speed action and how many of its frames have elapsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepAction {
    kind: StepKind,
    direction: Direction,
    frames_elapsed: u8,
}

impl StepAction {
    /// An action whose first frame has just run.
    pub(super) const fn begin(kind: StepKind, direction: Direction) -> Self {
        Self {
            kind,
            direction,
            frames_elapsed: 1,
        }
    }

    /// Whether the object walks to the next tile or animates in place.
    #[must_use]
    pub const fn kind(&self) -> StepKind {
        self.kind
    }

    /// The direction the action was started in; later facing changes do not alter it.
    #[must_use]
    pub const fn direction(&self) -> Direction {
        self.direction
    }

    /// Frames run so far, `1` through [`NORMAL_STEP_FRAMES`].
    #[must_use]
    pub const fn frames_elapsed(&self) -> u8 {
        self.frames_elapsed
    }

    /// Runs one more frame; `true` on the frame that completes the action.
    pub(super) const fn advance(&mut self) -> bool {
        self.frames_elapsed += 1;
        self.frames_elapsed >= NORMAL_STEP_FRAMES
    }

    /// Where the sprite is drawn relative to the tile it has already been committed to, in
    /// pixels: a walk starts a full tile behind the destination and closes one pixel a frame.
    #[must_use]
    pub const fn pixel_offset(&self) -> (i32, i32) {
        match self.kind {
            StepKind::WalkInPlace => (0, 0),
            StepKind::Walk => {
                let behind = TILE_PIXELS - self.frames_elapsed as i32;
                let (dx, dy) = self.direction.delta();
                (-dx * behind, -dy * behind)
            }
        }
    }
}

#[cfg(test)]
#[path = "object_event_motion_tests.rs"]
mod tests;
