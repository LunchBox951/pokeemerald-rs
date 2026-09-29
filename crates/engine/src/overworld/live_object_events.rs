//! A map's live object-event state, seeded once from its authored templates.
//!
//! Upstream spawns templates conditionally as the camera moves
//! (`TrySpawnObjectEvents`, `event_object_movement.c:1645-1673`, delegating
//! to `InitObjectEventStateFromTemplate`, `:1287-1330`).
//! [`ObjectEventCollection`] instead seeds every template unconditionally
//! and once; hide-flag and spawn-window admission stay query-time concerns
//! of [`super::visible_object_event_at`] and [`super::object_event_is_in_view`].

use assets::ObjectEvent;

use crate::overworld::collision::elevations_compatible;

use super::ObjectEventState;

/// One authored object-event template paired with its live, mutable state.
#[derive(Debug, Clone, Copy)]
pub struct LiveObjectEvent {
    template: &'static ObjectEvent,
    state: ObjectEventState,
}

impl LiveObjectEvent {
    /// Returns the authored template this entry was seeded from.
    #[must_use]
    pub const fn template(&self) -> &'static ObjectEvent {
        self.template
    }

    /// Returns this entry's live state.
    #[must_use]
    pub const fn state(&self) -> &ObjectEventState {
        &self.state
    }

    /// Returns mutable access to this entry's live state; the template
    /// above is never written back to.
    pub fn state_mut(&mut self) -> &mut ObjectEventState {
        &mut self.state
    }
}

/// One map's live object-event state, one entry per authored template, in
/// declaration order.
#[derive(Debug, Clone)]
pub struct ObjectEventCollection {
    entries: Vec<LiveObjectEvent>,
}

impl ObjectEventCollection {
    /// Seeds one live entry per template, in declaration order.
    #[must_use]
    pub fn from_templates(templates: &'static [ObjectEvent]) -> Self {
        Self {
            entries: templates
                .iter()
                .map(|template| LiveObjectEvent {
                    template,
                    state: ObjectEventState::from_template(template),
                })
                .collect(),
        }
    }

    /// Returns the entry whose template declares `local_id`, or the first
    /// such entry in declaration order if `local_id` repeats, matching
    /// upstream's linear scan (`GetObjectEventIdByLocalId`,
    /// `event_object_movement.c:1275-1285`).
    #[must_use]
    pub fn get(&self, local_id: u8) -> Option<&LiveObjectEvent> {
        self.entries
            .iter()
            .find(|entry| entry.template.local_id == local_id)
    }

    /// Returns mutable access to the entry [`Self::get`] would return.
    pub fn get_mut(&mut self, local_id: u8) -> Option<&mut LiveObjectEvent> {
        self.entries
            .iter_mut()
            .find(|entry| entry.template.local_id == local_id)
    }

    /// Iterates entries at `(x, y, elevation)` by *live* position, in
    /// declaration order.
    ///
    /// Unlike [`crate::overworld::map_runtime::MapRuntime::object_events_at`],
    /// which scans authored template coordinates, this scans live position
    /// and elevation. Either side's elevation may be
    /// [`crate::overworld::collision::ELEVATION_TRANSITION`] to match the
    /// other, mirroring upstream's `GetObjectEventIdByPosition`/
    /// `ObjectEventDoesElevationMatch` (`event_object_movement.c:2192-2215`).
    pub fn object_events_at(
        &self,
        x: i32,
        y: i32,
        elevation: u8,
    ) -> impl Iterator<Item = &LiveObjectEvent> {
        self.entries.iter().filter(move |entry| {
            let (ex, ey) = entry.state.position();
            ex == x && ey == y && elevations_compatible(entry.state.elevation(), elevation)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overworld::direction::Direction;
    use assets::{MovementType, TrainerType};

    fn object(local_id: u8, x: i16, y: i16, elevation: u8) -> ObjectEvent {
        ObjectEvent {
            local_id,
            graphics_id: "OBJ_EVENT_GFX_MOM",
            x,
            y,
            elevation,
            movement_type: MovementType::FaceRight,
            movement_range_x: 0,
            movement_range_y: 0,
            trainer_type: TrainerType::None,
            trainer_sight_or_berry_tree_id: "0",
            script: "SomeScript",
            flag: "0",
        }
    }

    fn leaked(events: Vec<ObjectEvent>) -> &'static [ObjectEvent] {
        Box::leak(events.into_boxed_slice())
    }

    #[test]
    fn collection_seeds_entries_and_looks_them_up_by_local_id() {
        let templates = leaked(vec![
            object(1, 2, 3, 0),
            object(2, 5, 6, 3),
            object(3, 9, 9, 3),
        ]);
        let collection = ObjectEventCollection::from_templates(templates);

        let first = collection
            .get(1)
            .expect("local_id 1 was seeded from the first template");
        assert_eq!(first.template().x, 2);
        assert_eq!(first.state().position(), (2, 3));
        assert_eq!(first.state().elevation(), 0);

        let second = collection
            .get(2)
            .expect("local_id 2 was seeded from the second template");
        assert_eq!(second.template().local_id, 2);
        assert_eq!(second.template().x, 5);
        assert_eq!(second.state().position(), (5, 6));
        assert_eq!(second.state().previous_position(), (5, 6));
        assert_eq!(second.state().elevation(), 3);

        let third = collection
            .get(3)
            .expect("local_id 3 was seeded from the third template");
        assert_eq!(third.template().x, 9);
        assert_eq!(third.state().position(), (9, 9));
        assert_eq!(third.state().elevation(), 3);

        assert!(
            collection.get(4).is_none(),
            "no template declares local_id 4"
        );
    }

    #[test]
    fn an_empty_template_slice_seeds_no_entries() {
        let collection = ObjectEventCollection::from_templates(&[]);

        assert!(collection.get(1).is_none());
        assert!(collection.object_events_at(0, 0, 0).next().is_none());
    }

    #[test]
    fn object_events_at_applies_the_elevation_transition_wildcard_on_either_side() {
        let templates = leaked(vec![
            object(1, 5, 5, 3),
            object(2, 6, 6, crate::overworld::collision::ELEVATION_TRANSITION),
        ]);
        let collection = ObjectEventCollection::from_templates(templates);

        assert!(
            collection.object_events_at(5, 5, 4).next().is_none(),
            "a genuine elevation mismatch must not match"
        );
        assert_eq!(
            collection
                .object_events_at(5, 5, crate::overworld::collision::ELEVATION_TRANSITION)
                .map(|entry| entry.template().local_id)
                .collect::<Vec<_>>(),
            vec![1],
            "a transition query elevation must match an ordinary entry elevation"
        );
        assert_eq!(
            collection
                .object_events_at(6, 6, 4)
                .map(|entry| entry.template().local_id)
                .collect::<Vec<_>>(),
            vec![2],
            "a transition entry elevation must match an ordinary query elevation"
        );
    }

    #[test]
    fn object_events_at_uses_live_positions_and_preserves_declaration_order() {
        let templates = leaked(vec![object(1, 0, 0, 3), object(2, 5, 5, 3)]);
        let mut collection = ObjectEventCollection::from_templates(templates);

        for _ in 0..5 {
            collection
                .get_mut(1)
                .unwrap()
                .state_mut()
                .walk(Direction::East);
        }
        for _ in 0..5 {
            collection
                .get_mut(2)
                .unwrap()
                .state_mut()
                .walk(Direction::West);
        }
        assert_eq!(
            (
                collection.get(1).unwrap().state().position(),
                collection.get(2).unwrap().state().position(),
            ),
            ((5, 0), (0, 5)),
            "fixture precondition: the two entries walked to each other's template tiles"
        );

        assert!(
            collection.object_events_at(0, 0, 3).next().is_none(),
            "local_id 1's vacated template tile must no longer report it"
        );
        assert!(
            collection.object_events_at(5, 5, 3).next().is_none(),
            "local_id 2's vacated template tile must no longer report it"
        );

        for _ in 0..5 {
            collection
                .get_mut(2)
                .unwrap()
                .state_mut()
                .walk(Direction::South);
        }
        assert_eq!(collection.get(2).unwrap().state().position(), (0, 10));
        for _ in 0..5 {
            collection
                .get_mut(2)
                .unwrap()
                .state_mut()
                .walk(Direction::East);
        }
        assert_eq!(collection.get(2).unwrap().state().position(), (5, 10));
        for _ in 0..10 {
            collection
                .get_mut(1)
                .unwrap()
                .state_mut()
                .walk(Direction::South);
        }
        assert_eq!(
            collection.get(1).unwrap().state().position(),
            (5, 10),
            "fixture precondition: both live entries now occupy the same tile"
        );

        let stacked: Vec<u8> = collection
            .object_events_at(5, 10, 3)
            .map(|entry| entry.template().local_id)
            .collect();
        assert_eq!(
            stacked,
            vec![1, 2],
            "a live-position stack must resolve in declaration order, matching the \
             static-template stacking rule this collection's siblings already prove"
        );
    }

    #[test]
    fn walking_a_collection_entry_updates_current_and_previous_positions() {
        let templates = leaked(vec![object(1, 5, 5, 3)]);
        let mut collection = ObjectEventCollection::from_templates(templates);

        collection
            .get_mut(1)
            .unwrap()
            .state_mut()
            .walk(Direction::South);

        let entry = collection.get(1).unwrap();
        assert_eq!(entry.state().position(), (5, 6));
        assert_eq!(entry.state().previous_position(), (5, 5));
        assert_eq!(entry.state().facing(), Direction::South);
        assert_eq!(
            entry.template().y,
            5,
            "the template must stay untouched by a live walk"
        );
    }

    #[test]
    fn facing_a_collection_entry_changes_facing_without_moving() {
        let templates = leaked(vec![object(1, 5, 5, 3)]);
        let mut collection = ObjectEventCollection::from_templates(templates);

        collection
            .get_mut(1)
            .unwrap()
            .state_mut()
            .face(Direction::West);

        let entry = collection.get(1).unwrap();
        assert_eq!(entry.state().facing(), Direction::West);
        assert_eq!(entry.state().position(), (5, 5));
        assert_eq!(entry.state().previous_position(), (5, 5));
    }

    #[test]
    fn coordinate_override_changes_only_the_live_entrys_template_position() {
        let templates = leaked(vec![object(1, 5, 5, 3)]);
        let mut collection = ObjectEventCollection::from_templates(templates);

        {
            let entry = collection.get_mut(1).unwrap();
            entry.state_mut().walk(Direction::South);
            entry.state_mut().walk(Direction::South);
            entry.state_mut().override_template_coords();
        }

        let entry = collection.get(1).unwrap();
        assert_eq!(entry.state().template_position(), (5, 7));
        assert_eq!(
            entry.template().y,
            5,
            "the override affects only the live state's own template snapshot, \
             never the authored template this entry was seeded from"
        );
    }

    #[test]
    fn movement_override_changes_only_the_live_entrys_template_movement_type() {
        let templates = leaked(vec![object(1, 5, 5, 3)]);
        let mut collection = ObjectEventCollection::from_templates(templates);

        collection
            .get_mut(1)
            .unwrap()
            .state_mut()
            .override_template_movement_type(MovementType::FaceUp);

        let entry = collection.get(1).unwrap();
        assert_eq!(entry.state().template_movement_type(), MovementType::FaceUp);
        assert_eq!(
            entry.template().movement_type,
            MovementType::FaceRight,
            "the override affects only the live state's own template snapshot, \
             never the authored template this entry was seeded from"
        );
    }
}
