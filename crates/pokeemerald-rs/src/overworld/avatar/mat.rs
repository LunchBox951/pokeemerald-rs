//! The Secret Base mats' avatar poses.
//!
//! The jump mat plays `MOVEMENT_ACTION_JUMP_IN_PLACE_*` twice, whose sprite
//! runs the facing's `sAnim_Go*` and rises by `sJumpY_High`; the spin mat's
//! exit step is `MovementAction_WalkSlow*`, running the same animation over
//! 31 updates (`event_object_movement.c:5140-5155, 6289-6300, 8424-8427`,
//! `object_event_anims.h:202-208`). The spin mat's faces are standing cells.

use engine::overworld::{ForcedMat, PlayerState};

/// Frames each `sAnim_Go*` cell lasts (`ANIMCMD_FRAME(n, 8)`).
const CELL_FRAMES: u8 = 8;

/// Which cell of the four-cell `sAnim_Go*` loop is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GoCell {
    /// A standing cell.
    Stand,
    /// The foot the current walk cycle leads with.
    LeadingFoot,
    /// The other foot, in the loop's third cell.
    OtherFoot,
}

const fn go_cell(elapsed: u8) -> GoCell {
    match elapsed.saturating_sub(1) / CELL_FRAMES % 4 {
        0 => GoCell::LeadingFoot,
        2 => GoCell::OtherFoot,
        _ => GoCell::Stand,
    }
}

/// The cell a mat jump or the spin mat's exit step shows, or `None` for
/// every other pose. `elapsed` counts the frame that started the action as 1,
/// like [`PlayerState::step_progress`].
pub(super) const fn go_cell_for(player: &PlayerState) -> Option<GoCell> {
    if let Some(ForcedMat::Jump { frames, .. }) = player.forced_mat() {
        return Some(go_cell(frames));
    }
    if player.in_transit() && player.transit_walk_slow() {
        return Some(go_cell(player.step_progress()));
    }
    None
}

/// Pixels the avatar sprite is raised this frame.
pub(super) const fn lift(player: &PlayerState) -> u8 {
    match player.forced_mat() {
        Some(mat) => mat.jump_lift(),
        None => 0,
    }
}
