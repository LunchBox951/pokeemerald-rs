//! The walking friendship counter (`UpdateFriendshipStepCounter`,
//! `pokeemerald/src/field_control_avatar.c:614-630`), split out of
//! [`super::step`] beside [`super::field_poison`].

use engine::overworld::{TilePos, WarpTrigger};

use super::OverworldPhase;

/// `VAR_FRIENDSHIP_STEP_COUNTER` (`pokeemerald/include/constants/vars.h:60`).
const VAR_FRIENDSHIP_STEP_COUNTER: u16 = 0x402A;
/// The counter's modulus: the walking gain rolls on every 128th step
/// (`field_control_avatar.c:621`).
const FRIENDSHIP_STEP_PERIOD: u16 = 128;

impl OverworldPhase {
    /// `UpdateFriendshipStepCounter` for a completed step that reaches
    /// `TryStartStepCountScript` (`field_control_avatar.c:536-545`): advances
    /// [`VAR_FRIENDSHIP_STEP_COUNTER`] modulo 128 and, on the wrap, runs the
    /// walking `AdjustFriendship` over all six party slots in order.
    ///
    /// Unlike the poison counter it runs on every such step, forced-movement
    /// flag or not, and in a Secret Base; only `InUnionRoom` skips it
    /// (`union_room.c:4307-4311`). The counter is a saved event var, so Save
    /// and Continue keep it mid-cycle. Each increment wraps as `u16`'s, like
    /// the C `(*ptr)++`.
    pub(super) fn update_friendship_step_counter(
        &mut self,
        landed: Option<TilePos>,
        door_warp: Option<WarpTrigger>,
    ) {
        if !self.step_count_script_reached(landed, door_warp) {
            return;
        }
        let counter = self
            .save1
            .event_data
            .var_get(VAR_FRIENDSHIP_STEP_COUNTER)
            .unwrap_or(0)
            .wrapping_add(1)
            % FRIENDSHIP_STEP_PERIOD;
        let _ = self
            .save1
            .event_data
            .var_set(VAR_FRIENDSHIP_STEP_COUNTER, counter);
        if counter == 0 {
            self.adjust_party_friendship_walking();
        }
    }

    /// Whether a landing reaches `TryStartStepCountScript`
    /// (`field_control_avatar.c:483-490,536-541`).
    pub(super) fn step_count_script_reached(
        &self,
        landed: Option<TilePos>,
        door_warp: Option<WarpTrigger>,
    ) -> bool {
        landed.is_some() && door_warp.is_none() && self.map_id != assets::MapId("MAP_UNION_ROOM")
    }

    /// `AdjustFriendship(FRIENDSHIP_EVENT_WALKING)` on all six saved slots,
    /// every slot visited regardless of the stored count, as the C loop does.
    ///
    /// The live lead's backing record is merged into the save first and
    /// reloaded after, so the lead's cached record carries the new
    /// friendship and a later flush cannot write the old one back.
    fn adjust_party_friendship_walking(&mut self) {
        let has_lead = self.lead.as_ref().is_some_and(|lead| !lead.is_lent());
        if has_lead {
            self.flush_lead_to_save();
        }
        // `GetCurrentRegionMapSectionId` reads the current map's header
        // (`overworld.c:1391-1394`).
        let section = assets::MapHeaderTable::new()
            .get(self.map_id)
            .and_then(|header| header.region_map_section.id());
        for record in &mut self.save1.player_party {
            crate::party::adjust_friendship_walking(record, section, &mut self.rng);
        }
        if has_lead {
            self.load_lead_from_save();
        }
    }
}
