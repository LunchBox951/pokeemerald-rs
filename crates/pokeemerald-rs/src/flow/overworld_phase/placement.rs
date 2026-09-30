//! Where a continued save puts the player (module split of
//! [`crate::flow::overworld_phase`], `oop-boundaries`): the elevation and
//! facing [`super::OverworldPhase::from_saved`] places
//! [`engine::overworld::PlayerState`] with.
//!
//! The saved player object's elevation pair wins when the save holds one
//! ([`saved_elevations`]); otherwise the elevation comes from the
//! destination tile's own grid cell
//! ([`engine::overworld::MapRuntime::arrival_elevation`], this port's shared
//! home for `ObjectEventUpdateElevation`'s landing-tile read -- also used by
//! [`engine::overworld::warp_destination_position`] for a resolved warp and
//! [`super::OverworldPhase::warp_to_position`] for an explicit-coordinate
//! warp, so the three placement paths cannot drift apart on the same
//! upstream rule (issue #379)), while the facing comes from the *save*
//! (`LoadObjectEvents`, `src/load_save.c:188-194`) and only falls back to
//! the tile when the save holds none. See
//! [`super::OverworldPhase::from_saved`]'s "What a continue restores" for the
//! whole account.

use engine::overworld::{warp_in_facing, Direction, TilePos, ELEVATION_MULTI_LEVEL};
use engine::save::SaveBlock1;

use crate::new_game;
use crate::overworld::OverworldScene;

/// `LoadObjectEvents`' player-facing half (`src/load_save.c:188-194`): the
/// direction the saved player object event holds, or `fallback` when it
/// holds none.
///
/// `DIR_NONE` is the only "none" there is: a save block whose object-event
/// array was never written -- a zeroed [`SaveBlock1`], or an image written
/// before this port modelled the field at all -- reads back as `0`, which
/// [`Direction::from_dir_id`] refuses. Falling back to the *tile-derived*
/// direction rather than to a literal `DIR_SOUTH` keeps such a save
/// behaving exactly as it did before this slice
/// ([`super::OverworldPhase::from_saved`]'s "Facing"). The bike-only diagonals are
/// likewise refused; no upstream player object event on foot can hold one.
pub(super) fn saved_facing(block1: &SaveBlock1, fallback: Direction) -> Direction {
    Direction::from_dir_id(block1.player_object_event.facing_direction).unwrap_or(fallback)
}

/// `LoadObjectEvents`' elevation half (`src/load_save.c:188-193`): the
/// `(current, previous)` elevation pair the saved player object holds, or
/// `(tile_elevation, tile_elevation)` when the save cannot be trusted to hold
/// one for this tile.
///
/// The pair is trusted only when the decoded `active` is set (upstream's bit
/// plus this port's marker, which only this port's writer sets) *and* the saved
/// current elevation is one upstream could hold on the landing cell. `ObjectEventUpdateElevation`
/// (`src/event_object_movement.c:7759-7771`) sets current elevation to the
/// cell's own -- 0 on a transition cell -- and retains it on a multi-level
/// (15) cell. An upstream-origin image re-saved by an older writer keeps its
/// `active` bit and elevation byte while the position moves on; the missing
/// marker rejects it, and a current elevation the cell forbids is a second net. A settled player never
/// holds a mismatch on an ordinary cell: a finished step sets previous coords
/// to current (`ShiftStillObjectEventCoords`, `event_object_movement.c:2162`)
/// before `DoGroundEffects_OnFinishStep` re-runs the update (`:8120-8130`), so
/// the mid-step multi-level early return does not survive to a save.
pub(super) fn saved_elevations(
    block1: &SaveBlock1,
    cell_elevation: Option<u8>,
    tile_elevation: u8,
) -> (u8, u8) {
    let event = block1.player_object_event;
    let holdable = cell_elevation
        .is_some_and(|cell| cell == ELEVATION_MULTI_LEVEL || event.current_elevation == cell);
    if event.active && holdable {
        (event.current_elevation, event.previous_elevation)
    } else {
        (tile_elevation, tile_elevation)
    }
}

/// The raw grid-cell elevation at `position` on `map_id`, before
/// [`saved_tile_placement`]'s multi-level substitution, or `None` when the
/// cell will not decode.
pub(super) fn saved_cell_elevation(
    scene: &OverworldScene,
    map_id: assets::MapId,
    position: TilePos,
) -> Option<u8> {
    let header = assets::MapHeaderTable::new().header(map_id).ok()?;
    let events = assets::MapEventsTable::new().resolve(map_id).ok()?;
    let runtime = scene.runtime(map_id, header, events);
    Some(runtime.metatile_cell(position.0, position.1)?.elevation)
}

/// The `(elevation, facing)` a continued save's player is placed with at
/// `position` on `map_id` -- see
/// [`super::OverworldPhase::from_saved`]'s "What a continue restores". The facing
/// half is only the *fallback* since issue #232 ([`saved_facing`]).
///
/// Both are the *fallback* for a save whose player object holds none
/// ([`saved_elevations`], [`saved_facing`]); upstream reads the destination grid cell for elevation
/// (`ObjectEventUpdateElevation`, via
/// [`engine::overworld::MapRuntime::arrival_elevation`]) and the destination
/// metatile's behavior for direction (`GetAdjustedInitialDirection`). A tile
/// that will not decode -- the map's header/events missing from the generated tables, or
/// coordinates outside the grid -- yields the new-game spawn elevation and
/// `DIR_SOUTH`, which is `GetAdjustedInitialDirection`'s own fallthrough
/// (`src/overworld.c:951`) rather than an invented default.
pub(super) fn saved_tile_placement(
    scene: &OverworldScene,
    map_id: assets::MapId,
    position: TilePos,
) -> (u8, engine::overworld::Direction) {
    let fallback = (new_game::SPAWN_ELEVATION, new_game::SPAWN_FACING);
    let Ok(header) = assets::MapHeaderTable::new().header(map_id) else {
        return fallback;
    };
    let Ok(events) = assets::MapEventsTable::new().resolve(map_id) else {
        return fallback;
    };
    let runtime = scene.runtime(map_id, header, events);
    let Some(elevation) = runtime.arrival_elevation(position.0, position.1) else {
        return fallback;
    };
    let facing = runtime
        .metatile_behavior(position.0, position.1)
        .map_or(new_game::SPAWN_FACING, warp_in_facing);
    (elevation, facing)
}
