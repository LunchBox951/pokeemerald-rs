//! A map-scoped collection of live object-event state, keyed by `local_id`
//! and seeded in declaration order from a map's authored templates.
//!
//! Upstream builds this once per map lifetime with a loop over its authored
//! `objectEventTemplates`, in array order (`TrySpawnObjectEvents`,
//! `event_object_movement.c:1645-1673`), delegating each slot's position,
//! previous position, elevation, facing, and movement initialization to
//! `InitObjectEventStateFromTemplate` (`event_object_movement.c:1287-1330`).
//! [`ObjectEventCollection`] models that non-sprite half: every authored
//! template gets one live [`ObjectEventState`] up front, regardless of hide
//! flag or camera spawn radius -- both stay query-time concerns of
//! [`super::visible_object_event_at`] and [`super::object_event_is_in_view`],
//! not this collection (see its own boundary note).

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
    /// Returns the immutable authored metadata (graphics, script, hide flag,
    /// trainer type/range, declaration position) this entry was seeded from.
    #[must_use]
    pub const fn template(&self) -> &'static ObjectEvent {
        self.template
    }

    /// Returns this entry's live position, elevation, facing, and movement
    /// state.
    #[must_use]
    pub const fn state(&self) -> &ObjectEventState {
        &self.state
    }

    /// Returns mutable access to this entry's live state; the template
    /// metadata above is never written back to.
    pub fn state_mut(&mut self) -> &mut ObjectEventState {
        &mut self.state
    }
}

/// One map's live object-event state, keyed by `local_id` and seeded in
/// declaration order from its authored templates.
///
/// Models the non-sprite half of upstream's per-map template loop
/// (`TrySpawnObjectEvents`, `event_object_movement.c:1645-1673`), which repeatedly, as the camera
/// moves, spawns each template not yet spawned and still in range and unhidden. This collection
/// instead seeds every template exactly once, unconditional on hide flag or camera radius, into a
/// fixed, map-lifetime `Vec` with exactly one live entry per authored template -- sprite/graphics
/// setup, the camera-radius spawn/despawn lifecycle, and object-event slot reuse are not modelled.
/// Hide-flag visibility and camera admission remain the query-time concerns
/// [`super::visible_object_event_at`] and [`super::object_event_is_in_view`] already implement over
/// the static templates; nothing in this module consumes them yet, and this collection is not yet
/// wired into any map runtime, rendering, collision, interaction, or sight query.
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

    /// Returns the live entry whose template declares `local_id`, following
    /// upstream's linear `localId` scan (`GetObjectEventIdByLocalId`,
    /// `event_object_movement.c:1275-1285`).
    #[must_use]
    pub fn get(&self, local_id: u8) -> Option<&LiveObjectEvent> {
        self.entries
            .iter()
            .find(|entry| entry.template.local_id == local_id)
    }

    /// Returns mutable access to the live entry whose template declares
    /// `local_id`.
    pub fn get_mut(&mut self, local_id: u8) -> Option<&mut LiveObjectEvent> {
        self.entries
            .iter_mut()
            .find(|entry| entry.template.local_id == local_id)
    }

    /// Iterates entries at `(x, y, elevation)` by *live* position, in
    /// declaration order.
    ///
    /// Either the entry's or the query's elevation may be
    /// [`crate::overworld::collision::ELEVATION_TRANSITION`]. Mirrors upstream's
    /// `GetObjectEventIdByPosition`/`ObjectEventDoesElevationMatch`
    /// (`event_object_movement.c:2192-2215`), which scan live
    /// `currentCoords`/`currentElevation` -- unlike
    /// [`crate::overworld::map_runtime::MapRuntime::object_events_at`], which scans the
    /// static templates' authored coordinates instead.
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
        // Declared in the opposite order from how they end up stacked, so a
        // result of [1, 2] proves the scan follows declaration order rather
        // than incidentally matching insertion or walk order.
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

        // Walk local_id 2 onto local_id 1's live tile to stack them.
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
