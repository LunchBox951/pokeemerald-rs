//! Shared Route 101 fixtures: the trigger tile constants and the synthetic phases.

use super::super::test_support::*;
use super::super::OverworldPhase;
use assets::{MapId, MoveId};
use engine::overworld::PlayerState;

/// First Route 101 rescue coord event (`Route101/map.json`, elevation 3).
pub(super) const ROUTE_101_TRIGGER_TILE: (i32, i32) = (10, 19);

pub(super) const ROUTE_101_TRIGGER_ELEVATION: u8 = 3;

/// Treecko's `MOVE_PURSUIT`: `EFFECT_PURSUIT` has no resolver, so `validate_player_move`
/// refuses it before any draw (`crates/battle/src/battle.rs:415`).
pub(super) const UNEXECUTABLE_MOVE: MoveId = MoveId::PURSUIT;

/// Open 25x25 synthetic grid at elevation 3 with `MAP_ROUTE101`'s real generated events.
pub(super) fn route_101_trigger_phase(player: PlayerState) -> OverworldPhase {
    OverworldPhase::for_test(
        crate::overworld::test_support::synthetic_scene(25, 25),
        MapId("MAP_ROUTE101"),
        player,
        None,
    )
}

pub(super) fn route_101_trigger_phase_with_special_tiles(
    player: PlayerState,
    specials: &[((u16, u16), u8)],
) -> OverworldPhase {
    OverworldPhase::for_test(
        crate::overworld::test_support::synthetic_scene_with_special_tiles(25, 25, specials),
        MapId("MAP_ROUTE101"),
        player,
        None,
    )
}

pub(super) fn trigger_tile_cell() -> (u16, u16) {
    let (x, y) = ROUTE_101_TRIGGER_TILE;
    (
        u16::try_from(x).expect("the trigger tile is inside the fixture"),
        u16::try_from(y).expect("the trigger tile is inside the fixture"),
    )
}

pub(super) fn walk_east(phase: &mut OverworldPhase, tiles: usize) {
    for _ in 0..tiles {
        walk_one_tile_east(phase);
    }
}
