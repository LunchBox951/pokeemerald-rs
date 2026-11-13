//! Saving and restoring a collection's active objects (`SaveObjectEvents`/`LoadObjectEvents`,
//! `load_save.c:180-194`), as opposed to seeding it from templates at a map load.
//!
//! A restored object is *active* at once, with the saved state rather than its template's, and
//! with no spawn-window or hide-flag test: upstream's return to the field respawns whatever the
//! save held active (`ReturnToField` -> `TrySpawnObjectEvents` finds them already spawned,
//! `event_object_movement.c:1645-1675`; a flag set since only gates a *later* spawn).

use super::{Lifecycle, LiveObjectEvent, ObjectEventCollection, SavedLiveState};
use crate::overworld::player::TilePos;

/// One active object's saved state with the local id naming it within its map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SavedLiveObject {
    /// The object's `localId` on the map the collection stands in.
    pub local_id: u8,
    /// The state to put back.
    pub state: SavedLiveState,
}

impl ObjectEventCollection {
    /// The active entries of the collection's own map, in sprite-slot order -- the order they
    /// run, and so draw from the shared `Random()` stream, in -- as [`Self::restore_saved`]
    /// takes them back.
    ///
    /// Entries carried in from another map are left out: their coordinates belong to a map
    /// whose templates a continue does not load.
    #[must_use]
    pub fn saved_objects(&self) -> Vec<SavedLiveObject> {
        self.active_indices_in_sprite_slot_order()
            .into_iter()
            .map(|index| &self.entries[index])
            .filter(|entry| entry.carried_from.is_none())
            .filter_map(|entry| {
                let Lifecycle::Active { initial, .. } = entry.lifecycle else {
                    return None;
                };
                let state = &entry.state;
                Some(SavedLiveObject {
                    local_id: entry.template.local_id,
                    state: SavedLiveState {
                        position: state.position(),
                        previous_position: state.previous_position(),
                        elevation: state.elevation(),
                        previous_elevation: state.previous_elevation(),
                        facing: state.facing(),
                        movement_type: state.movement_type(),
                        initial,
                    },
                })
            })
            .collect()
    }

    /// Restores `saved`, as [`Self::saved_objects`] listed them, and returns how many took. A
    /// saved id naming no template of the map, or repeating one already restored, is skipped.
    ///
    /// Each saved object takes the first not-yet-active entry of the collection's own map with
    /// its `local_id`, in sprite-slot order from the lowest free slot, so the relative order the
    /// scheduler runs them in is the one they were saved in. Its callback restarts at step 0
    /// with no delay, as a recreated sprite's data is cleared
    /// (`event_object_movement.c:4895-4901`). The player tile is taken as the one the spawn
    /// window was last reconciled at, so no map-load admission follows: an entry not restored
    /// stays unadmitted until the player moves.
    pub fn restore_saved(&mut self, saved: &[SavedLiveObject], player_position: TilePos) -> usize {
        let mut restored = 0;
        for object in saved {
            let Some(index) = self.entries.iter().position(|entry| {
                entry.carried_from.is_none()
                    && !entry.lifecycle.is_active()
                    && entry.template.local_id == object.local_id
            }) else {
                continue;
            };
            let sprite_slot = self.lowest_free_sprite_slot();
            let entry: &mut LiveObjectEvent = &mut self.entries[index];
            entry.state.restore_live(&object.state);
            entry.lifecycle = Lifecycle::Active {
                sprite_slot,
                initial: object.state.initial,
                type_func_id: 0,
                delay: 0,
                scheduled: object.state.movement_type,
            };
            restored += 1;
        }
        self.reconciled_at = Some(player_position);
        restored
    }
}

#[cfg(test)]
#[path = "object_event_saved_tests.rs"]
mod tests;
