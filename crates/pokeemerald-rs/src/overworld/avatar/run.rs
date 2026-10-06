//! The running sheet and `sAnim_Run*` timing for a held-B crossing.
//!
//! A running sheet's local cells mirror the walking sheet's one-for-one
//! (`object_event_pic_tables.h:992-1011`, `object_event_anims.h:346-379`).

use engine::overworld::PlayerState;

use super::PlayerCharacter;

/// Rendered frames the leading foot shows before the neutral cell: the
/// first cell of every `sAnim_Run*` lasts five frames and the second three
/// (`ANIMCMD_FRAME(12, 5)`, `ANIMCMD_FRAME(9, 3)`), which exactly fills a
/// [`engine::overworld::RUN_FRAMES_PER_TILE`] crossing.
const RUN_FOOT_FRAMES: u8 = 5;

impl PlayerCharacter {
    pub(in crate::overworld) const fn run_sprite_path(self) -> &'static str {
        match self {
            Self::Brendan => "brendan/running",
            Self::May => "may/running",
        }
    }
}

/// Whether the avatar draws from the running sheet: during a run crossing, and
/// for the neutral cell its last frame holds until the next poll.
pub(super) const fn draws_running_sheet(player: &PlayerState) -> bool {
    (player.in_transit() && player.transit_running()) || player.run_pose_held()
}

/// Whether the run crossing still shows its leading foot.
/// The step progress counts the frame that started the crossing as 1, like the walk
/// animation's own half-crossing split.
pub(super) const fn running_foot_forward(player: &PlayerState) -> bool {
    player.in_transit() && player.transit_running() && player.step_progress() <= RUN_FOOT_FRAMES
}
