//! Where a continued save puts the player: the elevation and facing
//! [`super::OverworldPhase::from_saved`] places
//! [`engine::overworld::PlayerState`] with.
//!
//! The saved player object's elevation pair wins when the save holds a
//! trustworthy one ([`saved_elevations`]); otherwise the elevation comes from
//! the destination tile ([`engine::overworld::MapRuntime::arrival_elevation`],
//! shared with [`engine::overworld::warp_destination_position`] and
//! [`super::OverworldPhase::warp_to_position`] so every placement path applies
//! `ObjectEventUpdateElevation`'s rule alike). The facing comes from the save
//! ([`saved_facing`]) and falls back to the destination tile's only when the
//! save holds none.

use engine::overworld::{warp_in_facing, Direction, TilePos, ELEVATION_MULTI_LEVEL};
use engine::save::SaveBlock1;

use crate::new_game;
use crate::overworld::OverworldScene;

/// The direction the saved player object event holds, or `fallback` when it
/// holds none (`LoadObjectEvents`, `src/load_save.c:188-194`).
///
/// A zeroed [`SaveBlock1`], or an image written before the object-event field
/// was modelled, reads back `0`, which [`Direction::from_dir_id`] refuses, so
/// the tile-derived `fallback` applies. Bike-only diagonals are refused too: a
/// player on foot never holds one.
pub(super) fn saved_facing(block1: &SaveBlock1, fallback: Direction) -> Direction {
    Direction::from_dir_id(block1.player_object_event.facing_direction).unwrap_or(fallback)
}

/// The `(current, previous)` elevation pair the saved player object holds, or
/// `(tile_elevation, tile_elevation)` when the save cannot be trusted to hold
/// one for this tile (`LoadObjectEvents`, `src/load_save.c:188-193`).
///
/// The pair is trusted only when the decoded `active` is set (upstream's bit
/// plus a marker only this port's writer sets, which rejects an image re-saved
/// by an older writer) and the saved current elevation is one upstream could
/// hold on the landing cell: the cell's own, or any on a multi-level cell
/// ([`ELEVATION_MULTI_LEVEL`]; `ObjectEventUpdateElevation`,
/// `src/event_object_movement.c:7759-7771`).
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

/// The raw grid-cell elevation at `position` on `scene`'s map, or `None` when
/// the cell does not decode. It is not
/// [`saved_tile_placement`]'s `arrival_elevation`, which substitutes for
/// multi-level cells.
pub(super) fn saved_cell_elevation(scene: &OverworldScene, position: TilePos) -> Option<u8> {
    let runtime = scene.runtime();
    Some(runtime.metatile_cell(position.0, position.1)?.elevation)
}

/// The `(elevation, facing)` the destination tile gives a continued save's
/// player at `position` on `map_id`. These are the fallbacks for a save whose
/// player object holds none ([`saved_elevations`], [`saved_facing`]).
///
/// Elevation is
/// [`engine::overworld::MapRuntime::arrival_elevation`] of the grid cell and
/// facing comes from the tile's metatile behavior (`GetAdjustedInitialDirection`). When the
/// map's header or events are missing from the generated tables, or the
/// coordinates fall outside the grid, the result is
/// [`new_game::SPAWN_ELEVATION`] and [`new_game::SPAWN_FACING`]
/// (`DIR_SOUTH`), which is `GetAdjustedInitialDirection`'s own fallthrough
/// (`src/overworld.c:951`).
pub(super) fn saved_tile_placement(
    scene: &OverworldScene,
    position: TilePos,
) -> (u8, engine::overworld::Direction) {
    let fallback = (new_game::SPAWN_ELEVATION, new_game::SPAWN_FACING);
    let runtime = scene.runtime();
    let Some(elevation) = runtime.arrival_elevation(position.0, position.1) else {
        return fallback;
    };
    let facing = runtime
        .metatile_behavior(position.0, position.1)
        .map_or(new_game::SPAWN_FACING, warp_in_facing);
    (elevation, facing)
}
