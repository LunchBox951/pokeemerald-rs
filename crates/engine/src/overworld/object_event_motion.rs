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

/// How many callback frames each command of a Standard `Go*` walk animation shows
/// (`ANIMCMD_FRAME(n, 8)`, `object_event_anims.h:202-235`).
pub const GO_COMMAND_FRAMES: u8 = 8;

/// The number of commands in a Standard `Go*` cycle: a foot, the neutral pose, the other foot
/// and the neutral pose again (`sAnim_GoSouth`, `object_event_anims.h:202-209`).
pub const GO_COMMANDS: u8 = 4;

/// The command a step starts on, given the command the sprite's animation sits at when the step
/// begins: the neutral commands 1 and 3 move on to the foot that follows, 0 and 2 stay
/// (`SetStepAnimHandleAlternation` with `sStepAnimTables`' `{1, 3, 0, 2}`,
/// `event_object_movement.c:4582-4598`, `object_event_anims.h:1146-1155`).
#[must_use]
pub const fn alternate_go_command(command: u8) -> u8 {
    match command {
        1 => 2,
        3 => 0,
        other => other,
    }
}

/// The neutral command a face action leaves the animation on: commands 0 and 1 give 1, and 2
/// and 3 give 3, so the foot that follows is kept (`SetStepAnim`, `:4601-4618`).
#[must_use]
pub const fn face_go_command(command: u8) -> u8 {
    if command <= 1 {
        1
    } else {
        3
    }
}

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
    start_command: u8,
}

impl StepAction {
    /// An action whose first frame has just run, with its `Go*` animation started on
    /// `start_command`.
    pub(super) const fn begin(kind: StepKind, direction: Direction, start_command: u8) -> Self {
        Self {
            kind,
            direction,
            frames_elapsed: 1,
            start_command,
        }
    }

    /// The command of the `Go*` animation cycle the sprite shows now, `0` to `3`. A step seeks
    /// its start command on its first frame, so that command holds for [`GO_COMMAND_FRAMES`]
    /// frames and the neutral pose after it holds for the rest
    /// (`SeekSpriteAnim`, `sprite.c:1359-1370`; `ContinueAnim`, `:943-966`).
    #[must_use]
    pub const fn go_command(&self) -> u8 {
        (self.start_command + (self.frames_elapsed - 1) / GO_COMMAND_FRAMES) % GO_COMMANDS
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
