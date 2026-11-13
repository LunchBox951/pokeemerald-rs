//! The non-player objects a save records and a continue puts back (`SaveObjectEvents`/
//! `LoadObjectEvents`, `src/load_save.c:180-194`), as opposed to a fresh map entry, which seeds
//! the map's templates ([`super::connections::seed_object_events`]).

use assets::{MapHeaderTable, MapId, MovementType, ObjectEvent};
use engine::event_data::EventData;
use engine::overworld::object_event::{ObjectEventCollection, SavedLiveObject, SavedLiveState};
use engine::overworld::Direction;
use engine::save::{Coords16, SaveBlock1, SavedNpcObject, SavedNpcObjects};

use super::connections::seed_object_events;
use crate::overworld::{oldale_town_npc_reposition, OverworldScene};

fn map_numbers(map: MapId) -> Option<(u8, u8)> {
    let header = MapHeaderTable::new().header(map).ok()?;
    Some((header.group, header.num))
}

fn coords(tile: (i32, i32)) -> Option<Coords16> {
    Some(Coords16 {
        x: i16::try_from(tile.0).ok()?,
        y: i16::try_from(tile.1).ok()?,
    })
}

fn tile(coords: Coords16) -> (i32, i32) {
    (i32::from(coords.x), i32::from(coords.y))
}

/// The snapshot a SAVE records of `objects` standing on `map`, or `None` when it cannot be
/// represented -- more objects than `objectEvents` has slots, or a tile outside an `s16` -- in
/// which case the save carries no snapshot and a continue seeds the templates instead. An
/// object carried in across a connection is recorded under the map its template belongs to.
pub(super) fn capture(map: MapId, objects: &ObjectEventCollection) -> Option<SavedNpcObjects> {
    let (group, num) = map_numbers(map)?;
    let saved = objects
        .saved_objects()
        .into_iter()
        .map(|object| {
            let state = object.state;
            let (map_group, map_num) = map_numbers(object.origin.unwrap_or(map))?;
            Some(SavedNpcObject {
                map_group,
                map_num,
                local_id: object.local_id,
                movement_type: state.movement_type.id(),
                facing_direction: state.facing.to_dir_id(),
                current_elevation: state.elevation,
                previous_elevation: state.previous_elevation,
                initial: coords(state.initial)?,
                current: coords(state.position)?,
                previous: coords(state.previous_position)?,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    SavedNpcObjects::new(group, num, saved)
}

/// The map a record's template belongs to: `None` for `map` itself, else the map its group and
/// number name -- or `Err` when they name none.
fn origin(map: MapId, saved: &SavedNpcObject) -> Result<Option<MapId>, ()> {
    let header = MapHeaderTable::new()
        .get_by_position(saved.map_group, saved.map_num)
        .ok_or(())?;
    Ok((header.id != map).then_some(header.id))
}

/// `origin`'s object `local_id`, as its map's events resolve under `event_data` -- the templates
/// a connection crossing carried the entry with.
fn carried_template(
    event_data: &EventData,
    origin: MapId,
    local_id: u8,
) -> Option<&'static ObjectEvent> {
    oldale_town_npc_reposition::resolve_map_events(origin, event_data)
        .ok()?
        .object_events
        .iter()
        .find(|template| template.local_id == local_id)
}

fn live(map: MapId, saved: &SavedNpcObject) -> Option<SavedLiveObject> {
    Some(SavedLiveObject {
        origin: origin(map, saved).ok()?,
        local_id: saved.local_id,
        state: SavedLiveState {
            position: tile(saved.current),
            previous_position: tile(saved.previous),
            elevation: saved.current_elevation,
            previous_elevation: saved.previous_elevation,
            facing: Direction::from_dir_id(saved.facing_direction)?,
            movement_type: MovementType::from_id(saved.movement_type).ok()?,
            initial: tile(saved.initial),
        },
    })
}

/// The snapshot, when it is of `map`.
fn snapshot_of(map: MapId, saved: Option<&SavedNpcObjects>) -> Option<&SavedNpcObjects> {
    saved.filter(|saved| map_numbers(map) == Some((saved.map_group(), saved.map_num())))
}

/// The templates of the objects an ordinary continue of `map` puts back from another map, for
/// the scene to bind their sprites beside its own events' as a crossing's does
/// ([`crate::overworld::load_room_carrying_from_source`]). A record that names no resolvable
/// object is left out here; [`restore_or_seed`] refuses the snapshot for it.
pub(super) fn carried_templates(map: MapId, block1: &SaveBlock1) -> Vec<&'static ObjectEvent> {
    let event_data = &block1.event_data;
    let Some(saved) = snapshot_of(map, block1.npc_objects.as_ref()) else {
        return Vec::new();
    };
    saved
        .objects()
        .iter()
        .filter_map(|record| {
            let origin = origin(map, record).ok()??;
            carried_template(event_data, origin, record.local_id)
        })
        .collect()
}

/// The collection an ordinary continue of `map` stands in: `scene`'s templates with the saved
/// objects put back active, when `saved` is a snapshot of that map; else (a save without one,
/// or one of another map) the templates seeded as a fresh entry does, for the caller to admit.
///
/// A snapshot with a record the model cannot restore -- an unknown movement type, facing or map,
/// or a local id its map does not declare -- is refused whole, logged, and replaced by the
/// seeded templates: dropping the one object would let the continue succeed with an NPC missing
/// and the next save erase it for good.
pub(super) fn restore_or_seed(
    scene: &OverworldScene,
    map: MapId,
    block1: &SaveBlock1,
    player: (i32, i32),
) -> ObjectEventCollection {
    let mut objects = seed_object_events(scene);
    let event_data = &block1.event_data;
    let Some(saved) = snapshot_of(map, block1.npc_objects.as_ref()) else {
        return objects;
    };
    let live: Option<Vec<SavedLiveObject>> = saved
        .objects()
        .iter()
        .map(|record| live(map, record))
        .collect();
    if let Some(live) = live {
        let restored = objects.restore_saved(&live, player, |origin, local_id| {
            carried_template(event_data, origin, local_id)
        });
        if restored == live.len() {
            return objects;
        }
    }
    eprintln!(
        "continue: the saved object events of {map:?} name an object or state this map cannot \
         restore -- seeding its templates instead"
    );
    seed_object_events(scene)
}
