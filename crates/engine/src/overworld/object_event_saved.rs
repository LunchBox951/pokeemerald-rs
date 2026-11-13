//! Saving and restoring a collection's active objects (`SaveObjectEvents`/`LoadObjectEvents`,
//! `load_save.c:180-194`), as opposed to seeding it from templates at a map load.
//!
//! A restored object is *active* at once, with the saved state rather than its template's, and
//! with no spawn-window or hide-flag test: upstream's return to the field respawns whatever the
//! save held active (`ReturnToField` -> `TrySpawnObjectEvents` finds them already spawned,
//! `event_object_movement.c:1645-1675`; a flag set since only gates a *later* spawn).

use super::{Lifecycle, LiveObjectEvent, ObjectEventCollection, ObjectEventState, SavedLiveState};
use crate::overworld::player::TilePos;
use assets::{MapId, ObjectEvent};

/// One active object's saved state with the identity naming it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SavedLiveObject {
    /// The map the object's template belongs to when it is not the map the collection stands in,
    /// i.e. the object was carried in across a connection
    /// ([`super::LiveObjectEvent::carried_from`]); `None` for the collection's own map.
    pub origin: Option<MapId>,
    /// The object's `localId` on that map.
    pub local_id: u8,
    /// The state to put back, in the collection's current map's coordinates.
    pub state: SavedLiveState,
}

impl ObjectEventCollection {
    /// The active entries, in sprite-slot order -- the order they run, and so draw from the
    /// shared `Random()` stream, in -- as [`Self::restore_saved`] takes them back. An entry
    /// carried in from another map is included with its origin: upstream's `SaveObjectEvents`
    /// copies every active object (`load_save.c:180-186`), wherever its template came from.
    #[must_use]
    pub fn saved_objects(&self) -> Vec<SavedLiveObject> {
        self.active_indices_in_sprite_slot_order()
            .into_iter()
            .map(|index| &self.entries[index])
            .filter_map(|entry| {
                let Lifecycle::Active { initial, .. } = entry.lifecycle else {
                    return None;
                };
                let state = &entry.state;
                Some(SavedLiveObject {
                    origin: entry.carried_from,
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

    /// Restores `saved`, as [`Self::saved_objects`] listed them, and returns how many took.
    ///
    /// An object of the collection's own map takes the first not-yet-active entry with its
    /// `local_id`. An object carried in from `origin` becomes a new entry beside them, after the
    /// map's own, whose template `carried_template` names (`None` if the origin's events have
    /// no such object): the shape [`Self::carry_across_connection`] leaves, so a later crossing
    /// finds it by `(carried_from, local_id)`. A saved object naming no template, or repeating
    /// one already restored, is skipped.
    ///
    /// Each takes the lowest free sprite slot in turn, so the relative order the scheduler runs
    /// them in is the one they were saved in. Its callback restarts at step 0 with no delay, as
    /// a recreated sprite's data is cleared (`event_object_movement.c:4895-4901`). The player
    /// tile is taken as the one the spawn window was last reconciled at, so no map-load
    /// admission follows: an entry not restored stays unadmitted until the player moves.
    pub fn restore_saved(
        &mut self,
        saved: &[SavedLiveObject],
        player_position: TilePos,
        carried_template: impl Fn(MapId, u8) -> Option<&'static ObjectEvent>,
    ) -> usize {
        let mut restored = 0;
        for object in saved {
            let sprite_slot = self.lowest_free_sprite_slot();
            let lifecycle = Lifecycle::Active {
                sprite_slot,
                initial: object.state.initial,
                type_func_id: 0,
                delay: 0,
                scheduled: object.state.movement_type,
            };
            let Some(origin) = object.origin else {
                let Some(index) = self.entries.iter().position(|entry| {
                    entry.carried_from.is_none()
                        && !entry.lifecycle.is_active()
                        && entry.template.local_id == object.local_id
                }) else {
                    continue;
                };
                let entry: &mut LiveObjectEvent = &mut self.entries[index];
                entry.state.restore_live(&object.state);
                entry.lifecycle = lifecycle;
                restored += 1;
                continue;
            };
            if self.find(Some(origin), object.local_id).is_some() {
                continue;
            }
            let Some(template) = carried_template(origin, object.local_id) else {
                continue;
            };
            let mut state = ObjectEventState::from_template(template);
            state.restore_live(&object.state);
            state.rebase_template_position(object.state.initial);
            let id = self.next_id;
            self.next_id = self.next_id.saturating_add(1);
            self.entries.push(LiveObjectEvent {
                template,
                state,
                lifecycle,
                carried_from: Some(origin),
                id,
            });
            restored += 1;
        }
        self.reconciled_at = Some(player_position);
        restored
    }
}

#[cfg(test)]
#[path = "object_event_saved_tests.rs"]
mod tests;
