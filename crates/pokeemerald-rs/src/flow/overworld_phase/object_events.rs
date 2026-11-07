//! The overworld frame's object-event sprite callbacks: hands the live
//! [`engine::overworld::object_event::ObjectEventCollection`] one frame of its
//! scheduler, with the field RNG and the pause state the rest of the phase
//! decides.

use engine::overworld::object_event::FrameContext;

use super::OverworldPhase;

impl OverworldPhase {
    /// Runs one frame of object-event movement, after the frame's input and any
    /// script has run, as CB2's `AnimateSprites` follows CB1
    /// (`src/overworld.c:1465-1469`). Shared by [`Self::step`] and an open start menu's frame.
    ///
    /// Object events are frozen while a message box, a sight-trainer approach, a
    /// start menu, or a battle owns the field: upstream's `lock`/`lockall`,
    /// `FreezeForApproachingTrainers` and `ShowStartMenu` freeze them
    /// (`src/scrcmd.c:2193-2208`, `src/start_menu.c:581`) and unfreeze when the
    /// owner closes (`src/start_menu.c:1421`). An escalator ride freezes them
    /// from its first frame until the destination's objects first run
    /// (`field_effect.c:1580-1586`).
    pub(super) fn advance_object_events(&mut self) {
        let frozen = self.dialog.is_some()
            || self.sight_approach.is_some()
            || self.start_menu.is_some()
            || self.escalator_freezes_objects()
            || self.in_battle();
        self.object_events.run_frame(
            &FrameContext {
                player_position: self.player.position(),
                event_data: &self.save1.event_data,
                frozen,
            },
            &mut self.rng,
        );
    }
}
