//! The rematch step counter (`IncrementRematchStepCounter`,
//! `pokeemerald/src/battle_setup.c:1776-1803`), split out of [`super::step`]
//! beside [`super::field_friendship`].

use engine::overworld::{TilePos, WarpTrigger};

use super::OverworldPhase;

/// `FLAG_BADGE01_GET` through `FLAG_BADGE08_GET`
/// (`pokeemerald/include/constants/flags.h:1359-1366`).
const BADGE_FLAGS: [u16; 8] = [0x867, 0x868, 0x869, 0x86A, 0x86B, 0x86C, 0x86D, 0x86E];
/// Badges `HasAtLeastFiveBadges` needs (`battle_setup.c:1776-1790`).
const BADGES_REQUIRED: usize = 5;
/// `STEP_COUNTER_MAX` (`battle_setup.c:1792`).
const STEP_COUNTER_MAX: u16 = 255;

impl OverworldPhase {
    /// `IncrementRematchStepCounter` (`battle_setup.c:1795-1803`), reached from
    /// `TryStartStepCountScript` (`field_control_avatar.c:536-545`).
    pub(super) fn increment_rematch_step_counter(
        &mut self,
        landed: Option<TilePos>,
        door_warp: Option<WarpTrigger>,
    ) {
        if !self.step_count_script_reached(landed, door_warp) {
            return;
        }
        let badges = BADGE_FLAGS
            .into_iter()
            .filter(|&flag| self.save1.event_data.flag_get(flag).unwrap_or(false))
            .count();
        if badges < BADGES_REQUIRED {
            return;
        }
        let counter = &mut self.save1.trainer_rematch_step_counter;
        *counter = counter.saturating_add(1).min(STEP_COUNTER_MAX);
    }
}
