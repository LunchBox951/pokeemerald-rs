//! The overworld frame's object-event sprite callbacks: hands the live
//! [`engine::overworld::object_event::ObjectEventCollection`] one frame of its
//! scheduler, with the field RNG and the pause state the rest of the phase
//! decides.

use engine::event_data::EventData;
use engine::overworld::object_event::FrameContext;
use engine::overworld::{MapRuntime, PlayerState};

use super::OverworldPhase;

/// What one overworld frame lets the object-event scheduler see of the player and the flags.
fn frame_context<'a>(
    player: &PlayerState,
    event_data: &'a EventData,
    terrain: MapRuntime<'a>,
    frozen: bool,
) -> FrameContext<'a> {
    FrameContext {
        player_position: player.position(),
        event_data,
        frozen,
        player_dashing: player.dashing(),
        terrain,
        player_previous_position: player.previous_position(),
        player_elevation: player.elevation(),
    }
}

impl OverworldPhase {
    /// Admits the objects already in view when the map was entered, before the frame's input can
    /// end the visit: map load's `TrySpawnObjectEvents` runs ahead of the first frame
    /// (`src/overworld.c:2163`), so a connection crossing on the first input frame finds them
    /// live. Does nothing once the collection has been admitted.
    ///
    /// The admitted objects also get their spawn-time ground-effect pass here, while the scene
    /// they stand on is still the one that loaded them: that pass opens each object's first
    /// sprite callback (`DoGroundEffects_OnSpawn`, `:4931`), and a crossing before it would
    /// carry no effect for the object to bring along. No object has moved yet, so the pass finds
    /// the tile it would find after the frame's input.
    pub(super) fn admit_object_events_on_map_load(&mut self) {
        // A warp, a continue, or a new game has already admitted the collection as it seeded it
        // (`connections::admit_object_events_at_map_load`), so this call's own admission is not
        // the only sign of a first frame: the scene's pending first object frame is the other.
        let admitted_now = self
            .object_events
            .admit_on_map_load(self.player.position(), &self.save1.event_data);
        if admitted_now || self.scene.object_admission_pending() {
            self.scene
                .seed_object_tall_grass(&self.object_events, &self.save1.event_data);
        }
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
        // movement. A scene's first frame also seeds again after the scheduler, so an object
        // its camera update admits on that frame, as a crossing's does, gets its pass then.
        let first_frame = self.scene.take_object_admission_pending();
        self.scene
            .seed_object_tall_grass(&self.object_events, &self.save1.event_data);
        let context = frame_context(
            &self.player,
            &self.save1.event_data,
            self.scene.runtime(),
            frozen,
        );
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
