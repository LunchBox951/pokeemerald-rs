//! Tile collision (S-5), ported from the collision-classifying subset of
//! upstream `src/event_object_movement.c`'s `GetCollisionAtCoords` and
//! `src/field_player_avatar.c`'s `CheckForPlayerAvatarCollision`.
//!
//! Upstream's `COLLISION_*` enum (`include/global.fieldmap.h`) has thirteen
//! variants covering object-event traffic, ledges, boulder-pushing,
//! rotating gates, and bike-specific rail/wheelie collisions on top of the
//! ordinary "can I stand here" cases. The ledge/boulder/gate/bike variants
//! are still unreachable in this port (no bike, no scripted objects — see
//! the crate-level scope in `crate::overworld`), so [`Collision`] models the
//! four that matter for a lone on-foot player against static map geometry
//! plus stationary NPCs: passable, blocked, elevation mismatch, and
//! occupied-by-an-object-event.
//!
//! `GetCollisionAtCoords`' own test *order* (`event_object_movement.c:4658-4672`)
//! is part of the ported behaviour, not an implementation detail: the grid's
//! collision bits are tested first, then the elevation mismatch, and only
//! then `DoesObjectCollideWithObjectAt`. A tile that is both walled off and
//! occupied therefore reports [`Collision::Impassable`], never
//! [`Collision::ObjectEvent`] — see
//! [`crate::overworld::player::PlayerState::step`], which applies the checks
//! in that same order.

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
    /// `COLLISION_OBJECT_EVENT`: the tile's geometry is clear, but another
    /// object event stands on it at a compatible elevation — upstream
    /// `DoesObjectCollideWithObjectAt` (`event_object_movement.c:4724-4742`),
    /// whose `AreElevationsCompatible` test (`:7789-7797`) is the same
    /// transition-wildcard rule
    /// [`crate::overworld::map_runtime::MapRuntime::object_events_at`]
    /// applies.
    ///
    /// Upstream scans `gObjectEvents` — the *spawned* objects — so a
    /// template whose hide flag is set never contributes one of these; this
    /// port reproduces that by scanning only visible object events (see
    /// [`crate::overworld::object_event::visible_object_event_at`]).
    ObjectEvent,
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
