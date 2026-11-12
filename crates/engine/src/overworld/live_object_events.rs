//! A map's live object-event state, seeded once from its authored templates.
//!
//! Upstream spawns templates conditionally as the camera moves
//! (`TrySpawnObjectEvents`, `event_object_movement.c:1645-1673`, delegating
//! to `InitObjectEventStateFromTemplate`, `:1287-1330`).
//! [`ObjectEventCollection`] instead seeds every template unconditionally
//! and once, as an unadmitted entry.
//!
//! A seamless map-connection crossing does not reset the collection:
//! [`ObjectEventCollection::carry_across_connection`] keeps the departed visit's active entries, shifted into the entered
//! map's coordinates, and seeds the entered map's templates beside them for the scheduler to admit.
//!
//! The per-frame scheduler ([`ObjectEventCollection::run_frame`]) is the one place that applies
//! upstream's spawn window, the template hide flag at spawn time, and the `frozen` gate: an entry
//! is *active* once the scheduler admits it, and only active entries run their movement type and
//! answer position queries ([`ObjectEventCollection::object_events_at`]). A flag that changes
//! afterwards neither admits nor retires an entry.

use assets::{MapId, ObjectEvent};

use crate::overworld::collision::elevations_compatible;

use super::ObjectEventState;
use crate::overworld::player::TilePos;

#[path = "object_event_scheduler.rs"]
mod scheduler;
pub use scheduler::FrameContext;
use scheduler::Lifecycle;

#[path = "object_event_collision.rs"]
mod object_collision;
pub use object_collision::{collision_in_direction, MovementRange, NpcCollision, Occupant};

/// One authored object-event template paired with its live, mutable state.
#[derive(Debug, Clone, Copy)]
pub struct LiveObjectEvent {
    template: &'static ObjectEvent,
    state: ObjectEventState,
    lifecycle: Lifecycle,
    /// The map whose template this entry was seeded from, once a connection crossing has carried
    /// it out of that map; `None` for an entry of the collection's own current map.
    carried_from: Option<MapId>,
    id: u32,
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

    /// Returns whether the scheduler has admitted this entry and not since retired it, the
    /// counterpart of upstream's spawned `active` object with a sprite.
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.lifecycle.is_active()
    }

    /// Returns the map this entry's template belongs to when that is not the map the collection
    /// currently stands in, i.e. the entry survived a connection crossing; `None` for an entry of
    /// the current map.
    ///
    /// A local id names an object only within its own map (`GetObjectEventIdByLocalIdAndMap`,
    /// `event_object_movement.c:1263-1285`), so `(carried_from, template().local_id)` is the
    /// entry's stable identity across crossings; [`ObjectEventCollection::find`] resolves it.
    #[must_use]
    pub const fn carried_from(&self) -> Option<MapId> {
        self.carried_from
    }

    /// Returns this entry's identity, unique among every entry a chain of connection crossings
    /// has ever held, which survives the removal of other entries and a crossing's carry.
    ///
    /// A collection index is not an identity: removing a carried entry compacts the entries
    /// after it, and a crossing seeds the entered map's templates ahead of the carried ones.
    /// Upstream's field-effect sprites name their owner by (`localId`, `mapNum`, `mapGroup`)
    /// (`field_effect_helpers.c:305-307`); a consumer that must follow one object across
    /// removals and crossings keys on this and resolves it with [`ObjectEventCollection::by_id`].
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Moves this entry into the coordinates a camera update puts its map in: the current,
    /// previous, and template-snapshot tiles, and the admission tile `RemoveObjectEventIfOutsideView`
    /// also tests (`initialCoords`). The authored template itself is never written.
    fn translate(&mut self, shift: (i32, i32)) {
        self.state.translate(shift);
        if let Lifecycle::Active { initial, .. } = &mut self.lifecycle {
            *initial = (
                initial.0.saturating_add(shift.0),
                initial.1.saturating_add(shift.1),
            );
        }
    }
}

/// One map's live object-event state, one entry per authored template, in
/// declaration order.
#[derive(Debug, Clone)]
pub struct ObjectEventCollection {
    entries: Vec<LiveObjectEvent>,
    /// The player tile the scheduler last reconciled the spawn window at; `None` before the
    /// map-load admission pass.
    reconciled_at: Option<TilePos>,
    /// The identity [`LiveObjectEvent::id`] gives the next entry seeded. A crossing continues the
    /// departed collection's count, so no entry of the entered collection reuses a carried one's.
    next_id: u32,
}

impl ObjectEventCollection {
    /// Seeds one live entry per template, in declaration order.
    #[must_use]
    pub fn from_templates(templates: &'static [ObjectEvent]) -> Self {
        let entries: Vec<LiveObjectEvent> = templates
            .iter()
            .zip(0..)
            .map(|(template, id)| LiveObjectEvent {
                template,
                state: ObjectEventState::from_template(template),
                lifecycle: Lifecycle::Unadmitted,
                carried_from: None,
                id,
            })
            .collect();
        let next_id = u32::try_from(entries.len()).unwrap_or(u32::MAX);
        Self {
            entries,
            reconciled_at: None,
            next_id,
        }
    }

    /// Iterates entries in declaration order.
    pub fn iter(&self) -> impl Iterator<Item = &LiveObjectEvent> {
        self.entries.iter()
    }

    /// Iterates entries mutably in declaration order, visiting each entry even when a local id
    /// repeats.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut LiveObjectEvent> {
        self.entries.iter_mut()
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

    /// Returns the entry [`LiveObjectEvent::id`] names, or `None` once it has been removed.
    #[must_use]
    pub fn by_id(&self, id: u32) -> Option<&LiveObjectEvent> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    /// Returns the entry [`LiveObjectEvent::carried_from`] and `local_id` name, unlike
    /// [`Self::get`], which resolves a bare local id to the first entry declaring it.
    #[must_use]
    pub fn find(&self, carried_from: Option<MapId>, local_id: u8) -> Option<&LiveObjectEvent> {
        self.entries
            .iter()
            .find(|entry| entry.carried_from == carried_from && entry.template.local_id == local_id)
    }

    /// Returns mutable access to the entry [`Self::find`] would return.
    pub fn find_mut(
        &mut self,
        carried_from: Option<MapId>,
        local_id: u8,
    ) -> Option<&mut LiveObjectEvent> {
        self.entries
            .iter_mut()
            .find(|entry| entry.carried_from == carried_from && entry.template.local_id == local_id)
    }

    /// Builds the collection a seamless connection crossing from `departed` into `entered` leaves
    /// behind, without touching this one, so a crossing that is then refused costs nothing.
    ///
    /// Upstream reloads the entered map's templates but leaves the active objects in place
    /// (`LoadMapFromCameraTransition`, `overworld.c:784-825`); the camera update that follows
    /// shifts each into the entered map's coordinates, spawns the templates the view reaches, then
    /// removes the objects outside it (`UpdateObjectEventCoordsForCameraUpdate`,
    /// `TrySpawnObjectEvents`, `RemoveObjectEventIfOutsideView`,
    /// `event_object_movement.c:1677-1713,2167-2189,2217-2222`). Here every active entry is
    /// shifted by `shift` (departed to entered tile coordinates) in its current, previous,
    /// template, and initial tiles; an entry the departed visit never admitted, or already
    /// retired, has no object upstream and is dropped. Each keeps its template, its sprite slot and
    /// its movement state, and still runs its callback on the crossing frame.
    ///
    /// The spawn and the removal are the scheduler's: the departed collection's last reconciled
    /// player tile is shifted along, so the next [`Self::run_frame`] sees a camera move, runs the
    /// callbacks, then admits before it removes, and an outgoing object still holds its sprite
    /// slot while a newcomer takes one.
    ///
    /// The entered map's `templates` follow in declaration order, unadmitted: the scheduler admits
    /// one only when the view and its hide flag allow, as at any map load, so a crossing never
    /// activates one by itself. A template whose object is already live, because an entry
    /// originally of `entered` survived the trip out and back, is that entry rather than a second
    /// one (`GetAvailableObjectEventId`, `:1362-1381`), and it takes that template for its next
    /// spawn, as upstream's reloaded template array serves it (`TrySpawnObjectEvents`,
    /// `:1666-1672`). Entries still carried from another map
    /// follow the templates, so the entered map's own indices never move.
    #[must_use]
    pub fn carry_across_connection(
        &self,
        departed: MapId,
        entered: MapId,
        templates: &'static [ObjectEvent],
        shift: (i32, i32),
    ) -> Self {
        let mut carried: Vec<LiveObjectEvent> = self
            .entries
            .iter()
            .filter(|entry| entry.is_active())
            .map(|entry| {
                let mut entry = *entry;
                entry.translate(shift);
                entry.carried_from.get_or_insert(departed);
                entry
            })
            .collect();
        let mut entries = Vec::with_capacity(templates.len() + carried.len());
        let mut next_id = self.next_id;
        for template in templates {
            let returned = carried.iter().position(|entry| {
                entry.carried_from == Some(entered) && entry.template.local_id == template.local_id
            });
            entries.push(if let Some(index) = returned {
                let mut entry = carried.remove(index);
                entry.carried_from = None;
                // The entered map's templates were reloaded, and its on-transition script may
                // have moved this one: the live object keeps its state, but its next spawn
                // reads the reloaded template.
                entry.template = template;
                entry.state.reload_template_snapshots(template);
                entry
            } else {
                let id = next_id;
                next_id = next_id.saturating_add(1);
                LiveObjectEvent {
                    template,
                    state: ObjectEventState::from_template(template),
                    lifecycle: Lifecycle::Unadmitted,
                    carried_from: None,
                    id,
                }
            });
        }
        entries.extend(carried);
        Self {
            entries,
            reconciled_at: self
                .reconciled_at
                .map(|(x, y)| (x.saturating_add(shift.0), y.saturating_add(shift.1))),
            next_id,
        }
    }

    /// Iterates the *active* entries at `(x, y, elevation)` by *live* position, in
    /// declaration order: an entry the scheduler has not admitted, or has retired, has no
    /// object, as `GetObjectEventIdByPosition` scans only active `gObjectEvents` slots.
    /// A hide flag is not read here; the scheduler applied it at admission.
    ///
    /// [`crate::overworld::map_runtime::MapRuntime::object_events_at`]
    /// delegates here when a collection is bound; unbound, it scans authored
    /// template coordinates instead. Either side's elevation may be
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
            entry.is_active()
                && ex == x
                && ey == y
                && elevations_compatible(entry.state.elevation(), elevation)
        })
    }

    /// Iterates admitted entries that hold `(x, y, elevation)` for collision, by their *live*
    /// current or previous tile, in declaration order.
    ///
    /// An unadmitted or despawned entry is skipped: upstream's scan visits only `active` objects
    /// (`DoesObjectCollideWithObjectAt`, `event_object_movement.c:4730-4733`).
    ///
    /// A walker holds the tile it is leaving until its step settles, so this is wider than
    /// [`Self::object_events_at`], which identifies the object standing on a tile by its current
    /// tile alone. Elevation matches as there (`DoesObjectCollideWithObjectAt`,
    /// `event_object_movement.c:4724-4741`).
    pub fn collision_objects_at(
        &self,
        x: i32,
        y: i32,
        elevation: u8,
    ) -> impl Iterator<Item = &LiveObjectEvent> {
        self.entries.iter().filter(move |entry| {
            let state = &entry.state;
            entry.is_active()
                && (state.position() == (x, y) || state.previous_position() == (x, y))
                && elevations_compatible(state.elevation(), elevation)
        })
    }

    /// Marks every entry admitted without running the scheduler, for tests that build a
    /// collection by hand and then place its objects.
    #[cfg(test)]
    pub(crate) fn admit_all_for_test(&mut self) {
        for entry in &mut self.entries {
            entry.lifecycle = Lifecycle::Active {
                sprite_slot: 0,
                initial: entry.state.template_position(),
                type_func_id: 0,
                delay: 0,
                scheduled: entry.state.movement_type(),
            };
        }
    }

    /// Iterates every entry in declaration order, the order upstream's
    /// `gObjectEvents` slot scan visits them (`trainer_see.c:196-219`).
    pub fn entries(&self) -> impl Iterator<Item = &LiveObjectEvent> {
        self.entries.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overworld::direction::Direction;
    use crate::overworld::object_event_harness::{admitted, template, Rig};
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
            script: Some(assets::ScriptRef::new("SomeScript")),
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
        let collection = admitted(templates, (5, 5));

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
        let mut collection = admitted(templates, (2, 2));

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

    const HIDE: &str = "FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_MOM";

    fn hide_flag() -> u16 {
        assets::object_event_flags::resolve(HIDE).unwrap()
    }

    fn ids_at(rig: &Rig, x: i32, y: i32) -> Vec<u8> {
        rig.collection
            .object_events_at(x, y, 3)
            .map(|entry| entry.template().local_id)
            .collect()
    }

    #[test]
    fn object_events_at_skips_entries_the_scheduler_has_not_admitted_or_has_retired() {
        let mut rig = Rig::new(vec![template(1, 2, 2, MovementType::FaceUp, "0")], (50, 50));
        assert_eq!(ids_at(&rig, 2, 2), Vec::<u8>::new(), "seeded, never ticked");
        rig.tick(1);
        assert_eq!(
            ids_at(&rig, 2, 2),
            Vec::<u8>::new(),
            "out of the spawn window"
        );

        rig.player = (2, 2);
        rig.tick(1);
        assert_eq!(ids_at(&rig, 2, 2), vec![1], "admitted once in the window");

        rig.player = (50, 50);
        rig.tick(1);
        assert!(!rig.collection.get(1).unwrap().is_active());
        assert_eq!(
            ids_at(&rig, 2, 2),
            Vec::<u8>::new(),
            "retired outside the view"
        );
    }

    #[test]
    fn a_hide_flag_cleared_while_the_camera_is_still_leaves_the_entry_out_of_the_query() {
        let mut rig = Rig::new(vec![template(1, 2, 2, MovementType::FaceUp, HIDE)], (2, 2));
        rig.event_data.flag_set(hide_flag()).unwrap();
        rig.tick(1);
        rig.event_data.flag_clear(hide_flag()).unwrap();
        rig.tick(10);
        assert_eq!(ids_at(&rig, 2, 2), Vec::<u8>::new());

        rig.player = (3, 2);
        rig.tick(1);
        assert_eq!(
            ids_at(&rig, 2, 2),
            vec![1],
            "the next camera update admits it"
        );
    }

    #[test]
    fn a_hide_flag_set_after_admission_leaves_the_entry_in_the_query() {
        let mut rig = Rig::new(vec![template(1, 2, 2, MovementType::FaceUp, HIDE)], (2, 2));
        rig.tick(1);
        rig.event_data.flag_set(hide_flag()).unwrap();
        rig.tick(10);
        assert_eq!(ids_at(&rig, 2, 2), vec![1]);
    }

    #[test]
    fn map_load_admission_makes_in_window_entries_queryable_before_any_frame_runs() {
        let templates = leaked(vec![object(1, 2, 2, 3), object(2, 40, 40, 3)]);
        let mut collection = ObjectEventCollection::from_templates(templates);
        assert!(collection.object_events_at(2, 2, 3).next().is_none());

        let _ = collection.admit_on_map_load((2, 2), &crate::event_data::EventData::new());
        assert_eq!(
            collection
                .object_events_at(2, 2, 3)
                .map(|entry| entry.template().local_id)
                .collect::<Vec<_>>(),
            vec![1]
        );
        assert!(
            collection.object_events_at(40, 40, 3).next().is_none(),
            "outside the spawn window"
        );
    }

    #[test]
    fn a_script_removal_retires_an_admitted_entry_whatever_its_flag() {
        let mut rig = Rig::new(vec![template(1, 2, 2, MovementType::FaceUp, HIDE)], (2, 2));
        rig.tick(1);
        assert_eq!(ids_at(&rig, 2, 2), vec![1]);

        assert!(rig.collection.remove_by_local_id(1));
        assert_eq!(ids_at(&rig, 2, 2), Vec::<u8>::new());
        assert!(!rig.collection.remove_by_local_id(1), "already retired");
        assert!(!rig.collection.remove_by_local_id(9), "no such local id");

        rig.event_data.flag_set(hide_flag()).unwrap();
        rig.player = (3, 2);
        rig.tick(1);
        assert_eq!(
            ids_at(&rig, 2, 2),
            Vec::<u8>::new(),
            "the set flag keeps it gone"
        );
    }

    mod crossing {
        use super::*;
        use crate::event_data::EventData;
        use crate::overworld::object_event_harness::{leaked, template, Rig, ScriptedRng};

        const DEPARTED: MapId = MapId("MAP_DEPARTED");
        const ENTERED: MapId = MapId("MAP_ENTERED");
        /// The departed map's tile `(x, y)` is `(x, y + 20)` in the entered map: a step north off
        /// the departed map's top edge into a 20-tall neighbour.
        const SHIFT: (i32, i32) = (0, 20);
        const NEVER_HIDDEN: &str = "0";
        const HIDDEN: &str = "FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_MOM";

        fn fixed(local_id: u8, x: i16, y: i16) -> ObjectEvent {
            template(local_id, x, y, MovementType::FaceDown, NEVER_HIDDEN)
        }

        /// `from` carried into `ENTERED` over `templates`.
        fn cross(
            from: &ObjectEventCollection,
            templates: Vec<ObjectEvent>,
        ) -> ObjectEventCollection {
            from.carry_across_connection(DEPARTED, ENTERED, leaked(templates), SHIFT)
        }

        /// One overworld frame with the player at `player`.
        fn frame(collection: &mut ObjectEventCollection, player: TilePos) {
            let terrain = crate::overworld::object_event_harness::Terrain::flat();
            collection.run_frame(
                &FrameContext {
                    player_position: player,
                    event_data: &EventData::new(),
                    frozen: false,
                    player_dashing: false,
                    terrain: terrain.runtime(),
                    player_previous_position: player,
                    player_elevation: crate::overworld::object_event_harness::FLOOR_ELEVATION,
                },
                &mut ScriptedRng::exhausted(),
            );
        }

        fn slot_of(entry: &LiveObjectEvent) -> Option<usize> {
            match entry.lifecycle {
                Lifecycle::Active { sprite_slot, .. } => Some(sprite_slot),
                Lifecycle::Unadmitted | Lifecycle::Despawned => None,
            }
        }

        fn initial_of(entry: &LiveObjectEvent) -> TilePos {
            match entry.lifecycle {
                Lifecycle::Active { initial, .. } => initial,
                other => panic!("expected an active entry, found {other:?}"),
            }
        }

        #[test]
        fn a_survivor_keeps_its_template_and_translates_current_previous_and_initial_tiles() {
            let mut rig = Rig::new(vec![fixed(4, 5, 5)], (5, 5));
            rig.tick(1);
            rig.collection
                .get_mut(4)
                .unwrap()
                .state_mut()
                .walk(Direction::South);
            let before = *rig.collection.get(4).unwrap();
            assert_eq!(before.state().position(), (5, 6));
            assert_eq!(before.state().previous_position(), (5, 5));
            assert_eq!(initial_of(&before), (5, 5));

            let carried = cross(&rig.collection, vec![]);

            let entry = carried
                .find(Some(DEPARTED), 4)
                .expect("the survivor is retained");
            assert!(entry.is_active());
            assert_eq!(entry.state().position(), (5, 26));
            assert_eq!(entry.state().previous_position(), (5, 25));
            assert_eq!(entry.state().template_position(), (5, 25));
            assert_eq!(initial_of(entry), (5, 25));
            assert_eq!(entry.state().facing(), before.state().facing());
            assert!(
                std::ptr::eq(entry.template(), before.template()),
                "the immutable template is the same one"
            );
            assert_eq!(
                (entry.template().x, entry.template().y),
                (5, 5),
                "the template stays in the departed map's coordinates"
            );
            assert_eq!(entry.carried_from(), Some(DEPARTED));
            assert_eq!(
                slot_of(entry),
                Some(0),
                "the survivor keeps the sprite slot it held"
            );
        }

        #[test]
        fn carrying_leaves_the_source_collection_untouched() {
            let mut rig = Rig::new(vec![fixed(4, 5, 5)], (5, 5));
            rig.tick(1);
            let before = *rig.collection.get(4).unwrap();

            let _ = cross(&rig.collection, vec![fixed(4, 0, 0)]);

            let after = rig.collection.get(4).unwrap();
            assert_eq!(after.state(), before.state());
            assert_eq!(after.lifecycle, before.lifecycle);
            assert_eq!(rig.collection.iter().count(), 1);
        }

        #[test]
        fn an_entry_outside_the_entered_view_is_removed_and_one_at_its_edge_stays() {
            // The crossing step carries the player one tile past the departed view's far edge
            // (`SPAWN_MAX_Y_OFFSET` = 9 south of the player).
            let mut rig = Rig::new(vec![fixed(1, 5, 13), fixed(2, 5, 14)], (5, 5));
            rig.tick(1);
            assert!(rig.collection.iter().all(LiveObjectEvent::is_active));

            let mut carried = cross(&rig.collection, vec![]);
            assert!(
                carried.find(Some(DEPARTED), 2).is_some(),
                "the crossing itself removes nothing: the scheduler does, on the camera move"
            );
            frame(&mut carried, (5, 24));

            assert!(
                carried.find(Some(DEPARTED), 1).is_some(),
                "nine rows below stays"
            );
            assert!(
                carried.find(Some(DEPARTED), 2).is_none(),
                "ten rows below is outside the view and is removed"
            );
        }

        #[test]
        fn an_entry_whose_initial_tile_is_in_view_survives_having_wandered_out() {
            let mut rig = Rig::new(vec![fixed(1, 5, 13)], (5, 5));
            rig.tick(1);
            for _ in 0..3 {
                rig.collection
                    .get_mut(1)
                    .unwrap()
                    .state_mut()
                    .walk(Direction::South);
            }
            // Entered coordinates: current (5, 36) is twelve rows below a player at (5, 24), but
            // the initial tile (5, 33) is nine rows below.
            let mut carried = cross(&rig.collection, vec![]);
            frame(&mut carried, (5, 24));
            let entry = carried
                .find(Some(DEPARTED), 1)
                .expect("the initial tile is in view");
            assert_eq!(entry.state().position(), (5, 36));

            let mut carried = cross(&rig.collection, vec![]);
            frame(&mut carried, (5, 22));
            assert!(carried.find(Some(DEPARTED), 1).is_none(), "both tiles out");
        }

        #[test]
        fn only_admitted_entries_are_carried() {
            let mut rig = Rig::new(
                vec![
                    fixed(1, 5, 6),
                    template(2, 5, 7, MovementType::FaceDown, HIDDEN),
                    fixed(3, 40, 6),
                ],
                (5, 5),
            );
            rig.event_data
                .flag_set(assets::object_event_flags::resolve(HIDDEN).expect("a bundled flag"))
                .unwrap();
            rig.tick(1);
            assert_eq!(
                rig.collection
                    .iter()
                    .filter(|entry| entry.is_active())
                    .count(),
                1
            );

            let carried = cross(&rig.collection, vec![]);

            assert_eq!(
                carried
                    .iter()
                    .map(|entry| entry.template().local_id)
                    .collect::<Vec<_>>(),
                vec![1],
                "a hidden and an out-of-window entry have no object upstream to carry"
            );
        }

        #[test]
        fn entered_templates_are_seeded_unadmitted_before_the_carried_entries() {
            let mut rig = Rig::new(vec![fixed(1, 5, 6)], (5, 5));
            rig.tick(1);

            let mut carried = cross(&rig.collection, vec![fixed(1, 5, 25), fixed(2, 5, 26)]);

            let order: Vec<_> = carried
                .iter()
                .map(|entry| {
                    (
                        entry.carried_from(),
                        entry.template().local_id,
                        entry.is_active(),
                    )
                })
                .collect();
            assert_eq!(
                order,
                vec![(None, 1, false), (None, 2, false), (Some(DEPARTED), 1, true)],
                "the entered map's own templates are not carried and are not admitted by the crossing"
            );
            assert_eq!(
                carried.get(1).unwrap().carried_from(),
                None,
                "a bare local id is the entered map's"
            );
            assert_eq!(
                carried.find(Some(DEPARTED), 1).unwrap().state().position(),
                (5, 26)
            );

            // The scheduler admits them on the camera move's frame, behind the survivor's slot.
            frame(&mut carried, (5, 24));
            assert!(carried.iter().all(LiveObjectEvent::is_active));
            let slots: Vec<_> = carried.iter().map(slot_of).collect();
            assert_eq!(slots, vec![Some(1), Some(2), Some(0)]);
        }

        #[test]
        fn a_carried_entry_removed_later_is_gone_and_never_respawns() {
            let mut rig = Rig::new(vec![fixed(1, 5, 6)], (5, 5));
            rig.tick(1);
            let mut carried = cross(&rig.collection, vec![fixed(2, 5, 25)]);
            frame(&mut carried, (5, 24));
            assert_eq!(carried.iter().count(), 2);

            frame(&mut carried, (5, 60));
            assert!(
                carried.find(Some(DEPARTED), 1).is_none(),
                "the survivor left the view and is removed"
            );
            assert!(
                carried.get(2).is_some(),
                "the entered map's own entry stays for respawn"
            );
            assert!(!carried.get(2).unwrap().is_active());

            // Walking back into where it stood spawns nothing for the departed map's template.
            frame(&mut carried, (5, 26));
            assert!(carried.find(Some(DEPARTED), 1).is_none());
            assert_eq!(carried.iter().count(), 1);
        }

        #[test]
        fn an_entrys_id_survives_the_removal_of_an_earlier_entry_and_a_crossing() {
            let mut rig = Rig::new(vec![fixed(1, 5, 9), fixed(2, 5, 4)], (5, 5));
            rig.tick(1);
            let [first, second] = [1, 2].map(|local_id| rig.collection.get(local_id).unwrap().id());
            assert_ne!(first, second);

            let mut carried = cross(&rig.collection, vec![fixed(3, 5, 25)]);
            let entered = carried.get(3).unwrap().id();
            assert!(
                [first, second].iter().all(|&id| id != entered),
                "the entered map's entry takes an id no carried entry holds"
            );
            // The first leaves the view, so the entries after it are compacted down an index.
            frame(&mut carried, (5, 19));
            frame(&mut carried, (5, 20));
            assert!(carried.by_id(first).is_none(), "the earlier entry is gone");
            assert_eq!(
                carried.by_id(second).map(|entry| entry.template().local_id),
                Some(2),
                "the later entry is still found by its own id"
            );
            assert_eq!(carried.by_id(entered).unwrap().template().local_id, 3);
        }

        #[test]
        fn a_returning_survivor_keeps_the_id_it_had_before_leaving() {
            let templates = leaked(vec![fixed(1, 5, 6)]);
            let mut rig = Rig::new(vec![], (5, 5));
            rig.collection = ObjectEventCollection::from_templates(templates);
            rig.tick(1);
            let id = rig.collection.get(1).unwrap().id();
            let away =
                rig.collection
                    .carry_across_connection(DEPARTED, ENTERED, leaked(vec![]), SHIFT);
            let back =
                away.carry_across_connection(ENTERED, DEPARTED, templates, (-SHIFT.0, -SHIFT.1));
            assert_eq!(back.get(1).unwrap().id(), id);
        }

        #[test]
        fn a_survivor_that_returns_to_its_own_map_is_its_template_entry_not_a_second_one() {
            let templates = leaked(vec![fixed(1, 5, 6), fixed(2, 5, 7)]);
            let mut rig = Rig::new(vec![], (5, 5));
            rig.collection = ObjectEventCollection::from_templates(templates);
            rig.tick(1);
            rig.collection
                .get_mut(2)
                .unwrap()
                .state_mut()
                .walk(Direction::East);

            let away =
                rig.collection
                    .carry_across_connection(DEPARTED, ENTERED, leaked(vec![]), SHIFT);
            let back =
                away.carry_across_connection(ENTERED, DEPARTED, templates, (-SHIFT.0, -SHIFT.1));

            assert_eq!(
                back.iter().count(),
                2,
                "no duplicate of an object already live"
            );
            assert!(back
                .iter()
                .all(|entry| entry.carried_from().is_none() && entry.is_active()));
            let walked = back.get(2).unwrap();
            assert_eq!(
                walked.state().position(),
                (6, 7),
                "its live tile came back with it"
            );
            assert!(std::ptr::eq(walked.template(), &raw const templates[1]));
        }

        #[test]
        fn a_survivor_that_returns_takes_the_reloaded_template_for_its_next_respawn() {
            // Upstream's crossing reloads the entered map's templates from its header and reruns
            // its on-transition script (`LoadObjEventTemplatesFromHeader`,
            // `RunOnTransitionMapScript`, `overworld.c:796,807`), so a placement such as Oldale
            // Town's flag-gated `setobjectxyperm` can differ from the one the survivor spawned
            // from. A live object keeps its tile; its next spawn reads the reloaded template
            // (`TrySpawnObjectEvents`, `event_object_movement.c:1666-1672`).
            let before = leaked(vec![fixed(1, 5, 6)]);
            let mut rig = Rig::new(vec![], (5, 5));
            rig.collection = ObjectEventCollection::from_templates(before);
            rig.tick(1);
            let away =
                rig.collection
                    .carry_across_connection(DEPARTED, ENTERED, leaked(vec![]), SHIFT);
            let after = leaked(vec![template(
                1,
                5,
                9,
                MovementType::FaceLeft,
                NEVER_HIDDEN,
            )]);
            let mut back =
                away.carry_across_connection(ENTERED, DEPARTED, after, (-SHIFT.0, -SHIFT.1));

            let returned = back.get(1).unwrap();
            assert!(returned.is_active());
            assert_eq!(returned.state().position(), (5, 6), "the live tile is kept");
            assert!(std::ptr::eq(returned.template(), &raw const after[0]));
            assert_eq!(returned.state().template_position(), (5, 9));
            assert_eq!(
                returned.state().template_movement_type(),
                MovementType::FaceLeft
            );

            frame(&mut back, (5, 60));
            assert!(!back.get(1).unwrap().is_active(), "out of view, removed");
            frame(&mut back, (5, 9));
            let respawned = back.get(1).unwrap();
            assert!(respawned.is_active());
            assert_eq!(
                respawned.state().position(),
                (5, 9),
                "respawned at the reloaded template's tile"
            );
            assert_eq!(respawned.state().movement_type(), MovementType::FaceLeft);
        }

        #[test]
        fn a_second_crossing_keeps_naming_the_first_departed_map() {
            let mut rig = Rig::new(vec![fixed(1, 5, 6)], (5, 5));
            rig.tick(1);
            let once = cross(&rig.collection, vec![fixed(1, 0, 0)]);
            let twice =
                once.carry_across_connection(ENTERED, MapId("MAP_THIRD"), leaked(vec![]), SHIFT);
            assert_eq!(
                twice
                    .iter()
                    .map(LiveObjectEvent::carried_from)
                    .collect::<Vec<_>>(),
                vec![Some(DEPARTED)],
                "the unadmitted ENTERED entry was dropped; the survivor is still the first map's"
            );
        }

        #[test]
        fn a_newcomer_admitted_on_the_crossing_frame_does_not_take_an_outgoing_objects_slot() {
            // Upstream's crossing frame runs every live callback, then the camera update spawns
            // (`TrySpawnObjectEvents`) before it removes (`RemoveObjectEventsOutsideView`,
            // `event_object_movement.c:2217-2222`): the outgoing object still holds slot 0 when
            // the newcomer is spawned, so the newcomer takes slot 2.
            let mut rig = Rig::new(vec![fixed(1, 5, 14), fixed(2, 5, 13)], (5, 5));
            rig.tick(1);
            assert_eq!(slot_of(rig.collection.get(1).unwrap()), Some(0));
            assert_eq!(slot_of(rig.collection.get(2).unwrap()), Some(1));

            let mut carried = cross(&rig.collection, vec![fixed(3, 5, 25)]);
            frame(&mut carried, (5, 24));

            assert!(
                carried.find(Some(DEPARTED), 1).is_none(),
                "outgoing removed"
            );
            assert_eq!(slot_of(carried.find(Some(DEPARTED), 2).unwrap()), Some(1));
            assert_eq!(
                slot_of(carried.get(3).unwrap()),
                Some(2),
                "the newcomer is spawned while the outgoing object still holds slot 0"
            );
        }
    }
}
