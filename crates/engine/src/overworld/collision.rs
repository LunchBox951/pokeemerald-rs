//! Collision rules for on-foot movement.
//!
//! Map impassability takes precedence over elevation mismatch, which takes
//! precedence over object-event occupancy.

use super::direction::Direction;
use super::metatile_behavior::{
    is_east_blocked, is_north_blocked, is_south_blocked, is_west_blocked,
};

/// An elevation compatible with every map cell and object.
pub const ELEVATION_TRANSITION: u8 = 0;

/// A map-cell elevation compatible with objects at every elevation.
pub const ELEVATION_MULTI_LEVEL: u8 = 15;

/// Returns whether an object's elevation is incompatible with a map cell.
#[must_use]
pub const fn elevation_mismatch(object_elevation: u8, cell_elevation: u8) -> bool {
    object_elevation != ELEVATION_TRANSITION
        && cell_elevation != ELEVATION_TRANSITION
        && cell_elevation != ELEVATION_MULTI_LEVEL
        && object_elevation != cell_elevation
}

/// Returns whether two objects may occupy the same cell based on elevation.
#[must_use]
pub const fn elevations_compatible(
    first_object_elevation: u8,
    second_object_elevation: u8,
) -> bool {
    first_object_elevation == ELEVATION_TRANSITION
        || second_object_elevation == ELEVATION_TRANSITION
        || first_object_elevation == second_object_elevation
}

/// Returns whether the standing tile blocks exit or the target tile blocks entry.
///
/// The asymmetric edge pairing follows upstream `IsMetatileDirectionallyImpassable`
/// (`src/event_object_movement.c:4715-4722`).
#[must_use]
pub const fn directionally_impassable(
    standing_behavior: u8,
    target_behavior: u8,
    direction: Direction,
) -> bool {
    match direction {
        Direction::South => {
            is_south_blocked(standing_behavior) || is_north_blocked(target_behavior)
        }
        Direction::North => {
            is_north_blocked(standing_behavior) || is_south_blocked(target_behavior)
        }
        Direction::West => is_west_blocked(standing_behavior) || is_east_blocked(target_behavior),
        Direction::East => is_east_blocked(standing_behavior) || is_west_blocked(target_behavior),
    }
}

/// The result of checking an on-foot movement destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Collision {
    /// Movement can proceed.
    None,
    /// Map collision bits, a closed directional edge, or the map boundary blocks movement.
    Impassable,
    /// The map cell's elevation is incompatible with the mover.
    ElevationMismatch,
    /// A visible object at a compatible elevation occupies the map cell.
    ObjectEvent,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_elevation_is_never_a_mismatch() {
        assert!(!elevation_mismatch(ELEVATION_TRANSITION, 5));
        assert!(!elevation_mismatch(
            ELEVATION_TRANSITION,
            ELEVATION_MULTI_LEVEL
        ));
    }

    #[test]
    fn transition_or_multi_level_target_is_never_a_mismatch() {
        assert!(!elevation_mismatch(3, ELEVATION_TRANSITION));
        assert!(!elevation_mismatch(3, ELEVATION_MULTI_LEVEL));
    }

    #[test]
    fn matching_elevations_are_compatible() {
        assert!(!elevation_mismatch(3, 3));
    }

    #[test]
    fn differing_ordinary_elevations_mismatch() {
        assert!(elevation_mismatch(3, 4));
        assert!(elevation_mismatch(1, 2));
    }

    #[test]
    fn object_elevations_are_compatible_only_on_a_match_or_a_transition() {
        assert!(elevations_compatible(3, 3));
        assert!(elevations_compatible(ELEVATION_TRANSITION, 5));
        assert!(elevations_compatible(5, ELEVATION_TRANSITION));
        assert!(!elevations_compatible(3, 4));
    }

    #[test]
    fn multi_level_is_a_cell_wildcard_but_not_an_object_one() {
        assert!(!elevation_mismatch(3, ELEVATION_MULTI_LEVEL));
        assert!(!elevations_compatible(3, ELEVATION_MULTI_LEVEL));
    }

    use super::super::metatile_behavior::{
        MB_IMPASSABLE_NORTH, MB_IMPASSABLE_NORTHEAST, MB_IMPASSABLE_SOUTH,
        MB_IMPASSABLE_SOUTH_AND_NORTH, MB_IMPASSABLE_WEST, MB_IMPASSABLE_WEST_AND_EAST, MB_NORMAL,
    };

    #[test]
    fn plain_ground_is_directionally_passable_in_every_direction() {
        for direction in [
            Direction::North,
            Direction::South,
            Direction::East,
            Direction::West,
        ] {
            assert!(!directionally_impassable(MB_NORMAL, MB_NORMAL, direction));
        }
    }

    #[test]
    fn standing_tile_blocks_exit_through_its_closed_edges() {
        assert!(directionally_impassable(
            MB_IMPASSABLE_SOUTH_AND_NORTH,
            MB_NORMAL,
            Direction::North
        ));
        assert!(directionally_impassable(
            MB_IMPASSABLE_SOUTH_AND_NORTH,
            MB_NORMAL,
            Direction::South
        ));
        assert!(!directionally_impassable(
            MB_IMPASSABLE_SOUTH_AND_NORTH,
            MB_NORMAL,
            Direction::East
        ));
        assert!(!directionally_impassable(
            MB_IMPASSABLE_SOUTH_AND_NORTH,
            MB_NORMAL,
            Direction::West
        ));
    }

    #[test]
    fn target_tile_blocks_entry_through_its_closed_edges() {
        assert!(directionally_impassable(
            MB_NORMAL,
            MB_IMPASSABLE_SOUTH_AND_NORTH,
            Direction::South
        ));
        assert!(directionally_impassable(
            MB_NORMAL,
            MB_IMPASSABLE_SOUTH_AND_NORTH,
            Direction::North
        ));
        assert!(!directionally_impassable(
            MB_NORMAL,
            MB_IMPASSABLE_SOUTH_AND_NORTH,
            Direction::East
        ));
        assert!(!directionally_impassable(
            MB_NORMAL,
            MB_IMPASSABLE_SOUTH_AND_NORTH,
            Direction::West
        ));
    }

    #[test]
    fn one_sided_behaviors_block_one_entry_and_one_exit() {
        assert!(directionally_impassable(
            MB_IMPASSABLE_NORTH,
            MB_NORMAL,
            Direction::North
        ));
        assert!(!directionally_impassable(
            MB_IMPASSABLE_NORTH,
            MB_NORMAL,
            Direction::South
        ));
        assert!(directionally_impassable(
            MB_NORMAL,
            MB_IMPASSABLE_NORTH,
            Direction::South
        ));
        assert!(!directionally_impassable(
            MB_NORMAL,
            MB_IMPASSABLE_NORTH,
            Direction::North
        ));
        assert!(directionally_impassable(
            MB_IMPASSABLE_WEST,
            MB_NORMAL,
            Direction::West
        ));
        assert!(directionally_impassable(
            MB_NORMAL,
            MB_IMPASSABLE_WEST_AND_EAST,
            Direction::East
        ));
        assert!(directionally_impassable(
            MB_IMPASSABLE_NORTHEAST,
            MB_NORMAL,
            Direction::North
        ));
        assert!(directionally_impassable(
            MB_IMPASSABLE_NORTHEAST,
            MB_NORMAL,
            Direction::East
        ));
        assert!(!directionally_impassable(
            MB_IMPASSABLE_NORTHEAST,
            MB_IMPASSABLE_SOUTH,
            Direction::West
        ));
    }
}
