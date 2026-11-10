//! The overworld frame's object-event sprite callbacks: hands the live
//! [`engine::overworld::object_event::ObjectEventCollection`] one frame of its
//! scheduler, with the field RNG and the pause state the rest of the phase
//! decides.

use engine::overworld::object_event::FrameContext;

use super::OverworldPhase;

impl OverworldPhase {
    /// Admits the objects already in view before the frame's input moves the player, as map
    /// load's `TrySpawnObjectEvents` does ahead of the first frame (`src/overworld.c:2163`), so
    /// the player's first step collides with them as with any active object
    /// (`DoesObjectCollideWithObjectAt`, `src/event_object_movement.c:4730-4733`). A frozen
    /// scheduler frame admits and moves nothing, and does nothing once the collection has
    /// reconciled at the player's tile.
    pub(super) fn admit_object_events_in_view(&mut self) {
        let context = FrameContext {
            player_position: self.player.position(),
            event_data: &self.save1.event_data,
            frozen: true,
            player_dashing: self.player.dashing(),
            terrain: self.scene.runtime(),
            player_previous_position: self.player.previous_position(),
            player_elevation: self.player.elevation(),
        };
        self.object_events.run_frame(&context, &mut self.rng);
    }

    /// Runs one frame of object-event movement, after the frame's input and any
    /// script has run, as CB2's `AnimateSprites` follows CB1
    /// (`src/overworld.c:1465-1469`). Shared by [`Self::step`] and an open start menu's frame.
    ///
    /// Object events are frozen while a message box, a sight-trainer approach, a
    /// start menu, or a battle owns the field: upstream's `lock`/`lockall`,
    /// `FreezeForApproachingTrainers` and `ShowStartMenu` freeze them
    /// (`src/scrcmd.c:2193-2208`, `src/start_menu.c:581`) and unfreeze when the
    /// owner closes (`src/start_menu.c:1421`).
    pub(super) fn advance_object_events(&mut self) {
        let frozen = self.dialog.is_some()
            || self.sight_approach.is_some()
            || self.start_menu.is_some()
            || self.in_battle();
        // Each object's spawn-time ground-effect pass opens its sprite
        // callback (`UpdateObjectEventCurrentMovement`, `:4931`), ahead of the
        // movement. Map load's admission happens before the first callback, so
        // that frame's pass follows the scheduler's.
        let first_frame = self.scene.take_object_admission_pending();
        self.scene
            .seed_object_tall_grass(&self.object_events, &self.save1.event_data);
        let context = FrameContext {
            player_position: self.player.position(),
            event_data: &self.save1.event_data,
            frozen,
            player_dashing: self.player.dashing(),
            terrain: self.scene.runtime(),
            player_previous_position: self.player.previous_position(),
            player_elevation: self.player.elevation(),
        };
        #[cfg(test)]
        if let Some(scratch) = &mut self.object_event_rng {
            self.object_events.run_frame(&context, scratch);
        } else {
            self.object_events.run_frame(&context, &mut self.rng);
        }
        #[cfg(not(test))]
        self.object_events.run_frame(&context, &mut self.rng);
        if first_frame {
            self.scene
                .seed_object_tall_grass(&self.object_events, &self.save1.event_data);
        }
        self.scene
            .object_steps_began(&self.object_events, &self.save1.event_data);
    }
}
