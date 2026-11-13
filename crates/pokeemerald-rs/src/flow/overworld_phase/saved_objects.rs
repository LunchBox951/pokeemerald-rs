//! The non-player objects a save records and a continue puts back (`SaveObjectEvents`/
//! `LoadObjectEvents`, `src/load_save.c:180-194`), as opposed to a fresh map entry, which seeds
//! the map's templates ([`super::connections::seed_object_events`]).

use assets::{MapHeaderTable, MapId, MovementType};
use engine::overworld::object_event::{ObjectEventCollection, SavedLiveObject, SavedLiveState};
use engine::overworld::Direction;
use engine::save::{Coords16, SavedNpcObject, SavedNpcObjects};

use super::connections::seed_object_events;
use crate::overworld::OverworldScene;

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
/// which case the save carries no snapshot and a continue seeds the templates instead.
pub(super) fn capture(map: MapId, objects: &ObjectEventCollection) -> Option<SavedNpcObjects> {
    let (group, num) = map_numbers(map)?;
    let saved = objects
        .saved_objects()
        .into_iter()
        .map(|object| {
            let state = object.state;
            Some(SavedNpcObject {
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

fn live(saved: &SavedNpcObject) -> Option<SavedLiveObject> {
    Some(SavedLiveObject {
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

/// The collection an ordinary continue of `map` stands in: `scene`'s templates with the saved
/// objects put back active, when `saved` is a snapshot of that map; else (a save without one,
/// or one of another map) the templates seeded as a fresh entry does, for the caller to admit.
///
/// A snapshot with a record the model cannot restore -- an unknown movement type or facing, or
/// a local id the map does not declare -- is refused whole, logged, and replaced by the seeded
/// templates: dropping the one object would let the continue succeed with an NPC missing and the
/// next save erase it for good.
pub(super) fn restore_or_seed(
    scene: &OverworldScene,
    map: MapId,
    saved: Option<&SavedNpcObjects>,
    player: (i32, i32),
) -> ObjectEventCollection {
    let mut objects = seed_object_events(scene);
    let Some(saved) =
        saved.filter(|saved| map_numbers(map) == Some((saved.map_group(), saved.map_num())))
    else {
        return objects;
    };
    let live: Option<Vec<SavedLiveObject>> = saved.objects().iter().map(live).collect();
    if let Some(live) = live {
        if objects.restore_saved(&live, player) == live.len() {
            return objects;
        }
    }
    eprintln!(
        "continue: the saved object events of {map:?} name an object or state this map cannot \
         restore -- seeding its templates instead"
    );
    seed_object_events(scene)
}
