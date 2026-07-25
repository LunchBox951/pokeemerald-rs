//! Tile collision (S-5), ported from the collision-classifying subset of
//! upstream `src/event_object_movement.c`'s `GetCollisionAtCoords` and
//! `src/field_player_avatar.c`'s `CheckForPlayerAvatarCollision`.
//!
//! Upstream's `COLLISION_*` enum (`include/global.fieldmap.h`) has thirteen
//! variants covering object-event traffic, ledges, boulder-pushing,
//! rotating gates, and bike-specific rail/wheelie collisions on top of the
//! ordinary "can I stand here" cases. None of the object/NPC/bike variants
//! are reachable in this port yet (no NPCs, no bike, no scripted objects —
//! see the crate-level scope in `crate::overworld`), so [`Collision`] only
//! models the three that matter for a lone on-foot player against static
//! map geometry: passable, blocked, and elevation mismatch.

/// Upstream `ELEVATION_TRANSITION` (`include/global.fieldmap.h`): an
/// elevation of `0` means "compatible with anything" in both directions —
/// bridges, stairs landings, and other tiles that intentionally blend two
/// levels together.
pub const ELEVATION_TRANSITION: u8 = 0;

/// Upstream `ELEVATION_MULTI_LEVEL` (`15`): the map grid's sentinel for "this
/// cell doesn't have a single well-defined elevation" (used sparingly, e.g.
/// multi-level bridge overlaps). Like [`ELEVATION_TRANSITION`], a tile at
/// this elevation never mismatches.
pub const ELEVATION_MULTI_LEVEL: u8 = 15;

/// Whether stepping onto a tile at `target_elevation` from an object
/// currently at `current_elevation` is blocked, mirroring upstream
/// `IsElevationMismatchAt` (`event_object_movement.c`) exactly (that
/// function's own `x`/`y` grid lookup is `crate::overworld::map_runtime`'s
/// job — this is just the elevation-vs-elevation comparison at its core).
#[must_use]
pub const fn elevation_mismatch(current_elevation: u8, target_elevation: u8) -> bool {
    if current_elevation == ELEVATION_TRANSITION {
        return false;
    }
    if target_elevation == ELEVATION_TRANSITION || target_elevation == ELEVATION_MULTI_LEVEL {
        return false;
    }
    current_elevation != target_elevation
}

/// The outcome of checking whether an on-foot object can move onto a tile —
/// the subset of upstream's `COLLISION_*` enum this port models (see the
/// module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Collision {
    /// `COLLISION_NONE`: the tile is clear to step onto.
    None,
    /// `COLLISION_IMPASSABLE`: either the destination grid cell's collision
    /// bits are set (`MapGridGetCollisionAt` non-zero), or the destination
    /// is off the current map's edge with no connection covering it
    /// (upstream `GetMapBorderIdAt(x, y) == CONNECTION_INVALID`).
    Impassable,
    /// `COLLISION_ELEVATION_MISMATCH`: the tile is otherwise clear, but its
    /// elevation is incompatible with the mover's current elevation (see
    /// [`elevation_mismatch`]).
    ElevationMismatch,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_elevation_is_never_a_mismatch() {
        // event_object_movement.c `IsElevationMismatchAt`: `elevation ==
        // ELEVATION_TRANSITION` returns FALSE unconditionally.
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
}
