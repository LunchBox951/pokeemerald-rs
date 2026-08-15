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
//! collision bits and [`directionally_impassable`] are tested first — they
//! share a single `else if` arm upstream, so both report
//! [`Collision::Impassable`] — then the elevation mismatch, and only then
//! `DoesObjectCollideWithObjectAt`. A tile that is both walled off and
//! occupied therefore reports [`Collision::Impassable`], never
//! [`Collision::ObjectEvent`] — see
//! [`crate::overworld::player::PlayerState::step`], which applies the checks
//! in that same order.

use super::direction::Direction;
use super::metatile_behavior::{
    is_east_blocked, is_north_blocked, is_south_blocked, is_west_blocked,
};

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

/// Whether two *objects'* elevations may share a tile, mirroring upstream
/// `AreElevationsCompatible` (`event_object_movement.c:7789-7797`) exactly:
/// [`ELEVATION_TRANSITION`] on either side is a wildcard, and otherwise the
/// two must be equal.
///
/// **Not the same rule as [`elevation_mismatch`]**, and that difference is
/// why both exist: `IsElevationMismatchAt` compares an object against a *map
/// cell* and additionally lets [`ELEVATION_MULTI_LEVEL`] through, because a
/// multi-level cell genuinely has no single elevation to disagree with. Two
/// objects always do, so `DoesObjectCollideWithObjectAt`'s own test has no
/// such leniency — an object standing at elevation `15` is at elevation
/// `15`.
#[must_use]
pub const fn elevations_compatible(a: u8, b: u8) -> bool {
    a == ELEVATION_TRANSITION || b == ELEVATION_TRANSITION || a == b
}

/// Whether a step in `direction` is blocked by an invisible one-sided wall
/// carried by either tile's *behavior*, mirroring upstream
/// `IsMetatileDirectionallyImpassable` (`event_object_movement.c:4715-4722`)
/// exactly (that function's own grid lookups are
/// `crate::overworld::map_runtime`'s job — this is the behavior-vs-behavior
/// decision at its core).
///
/// `standing_behavior` is the behavior of the tile the mover currently
/// occupies (upstream `objectEvent->currentMetatileBehavior`, kept fresh by
/// `ObjectEventUpdateMetatileBehaviors`, `:7428-7432`, from
/// `MapGridGetMetatileBehaviorAt(currentCoords)` — this port re-reads the
/// grid at the mover's position instead of caching a field, which is the
/// same value by construction). `target_behavior` is the behavior of the
/// tile being stepped onto (upstream's own `MapGridGetMetatileBehaviorAt(x, y)`).
///
/// **Both sides are checked, with different pairings** — this is the part
/// that is easy to get backwards, so it is spelled out here. Upstream keeps
/// two function tables (`:893-905`), both indexed by `direction - 1`:
///
/// - `gOppositeDirectionBlockedMetatileFuncs` is applied to the **standing**
///   tile and pairs each direction with its *own* predicate: moving north
///   consults [`is_north_blocked`], south [`is_south_blocked`], and so on.
///   A mover standing on a north-blocked tile cannot leave northward.
/// - `gDirectionBlockedMetatileFuncs` is applied to the **target** tile and
///   pairs each direction with the *opposite* predicate: moving north
///   consults [`is_south_blocked`], south [`is_north_blocked`], west
///   [`is_east_blocked`], east [`is_west_blocked`]. A mover cannot enter a
///   tile through an edge that tile walls off.
///
/// (Upstream's two array *names* read the other way round from the pairings
/// they hold; the pairings above are what the arrays literally contain.)
///
/// Worked example, the issue #218 defect: the protagonist's bed carries
/// [`MB_IMPASSABLE_SOUTH_AND_NORTH`](super::metatile_behavior::MB_IMPASSABLE_SOUTH_AND_NORTH)
/// on its center pillow tile. Standing on the pillow and pressing north is
/// blocked by the *standing*-side test (`is_north_blocked(0xC0)`); standing
/// on the plain floor tile above it and pressing south is blocked by the
/// *target*-side test (`is_north_blocked(0xC0)` again, the opposite pairing
/// for a southward move). Either way the player cannot cross the bed
/// lengthwise, while east/west traffic across it stays free.
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

    /// `AreElevationsCompatible` is symmetric in its transition wildcard and
    /// strict everywhere else.
    #[test]
    fn object_elevations_are_compatible_only_on_a_match_or_a_transition() {
        assert!(elevations_compatible(3, 3));
        assert!(elevations_compatible(ELEVATION_TRANSITION, 5));
        assert!(elevations_compatible(5, ELEVATION_TRANSITION));
        assert!(!elevations_compatible(3, 4));
    }

    /// The one place `AreElevationsCompatible` and `IsElevationMismatchAt`
    /// genuinely disagree, and the reason both primitives exist:
    /// [`ELEVATION_MULTI_LEVEL`] is a wildcard for a *map cell* only, never
    /// for another object.
    #[test]
    fn multi_level_is_a_cell_wildcard_but_not_an_object_one() {
        assert!(!elevation_mismatch(3, ELEVATION_MULTI_LEVEL));
        assert!(!elevations_compatible(3, ELEVATION_MULTI_LEVEL));
    }

    use super::super::metatile_behavior::{
        MB_IMPASSABLE_NORTH, MB_IMPASSABLE_NORTHEAST, MB_IMPASSABLE_SOUTH,
        MB_IMPASSABLE_SOUTH_AND_NORTH, MB_IMPASSABLE_WEST, MB_IMPASSABLE_WEST_AND_EAST, MB_NORMAL,
    };

    /// Two ordinary-ground tiles are never directionally impassable, in any
    /// direction — the overwhelmingly common case, and the one that would
    /// break every other movement test in the workspace if the pairing were
    /// wrong.
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

    /// The **standing**-tile half (upstream
    /// `gOppositeDirectionBlockedMetatileFuncs[direction - 1]` applied to
    /// `currentMetatileBehavior`): each direction consults its *own*
    /// predicate, so a mover on the bed's pillow cannot leave north or
    /// south but can still leave east or west.
    #[test]
    fn the_standing_tile_blocks_its_own_direction() {
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

    /// The **target**-tile half (upstream
    /// `gDirectionBlockedMetatileFuncs[direction - 1]` applied to
    /// `MapGridGetMetatileBehaviorAt(x, y)`): each direction consults the
    /// *opposite* predicate, so the same `0xC0` pillow tile blocks entry
    /// from the north (a southward move) and from the south (a northward
    /// move) while staying enterable from either side.
    #[test]
    fn the_target_tile_blocks_the_opposite_direction() {
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

    /// A one-sided id is one-sided in *both* halves, and the two halves do
    /// not leak into each other: `MB_IMPASSABLE_NORTH` on the standing tile
    /// blocks only a northward exit, and on the target tile blocks only a
    /// southward entry (never the reverse, which is the mistake a swapped
    /// pairing would produce).
    #[test]
    fn one_sided_ids_block_exactly_one_move_per_side() {
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

        // The east/west axis, and a diagonal id that belongs to two sets.
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
