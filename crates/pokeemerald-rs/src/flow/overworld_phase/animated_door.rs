//! The animated-door warp decision (issue #851, S-5): upstream `TryDoorWarp`
//! (`pokeemerald/src/field_control_avatar.c:170-178`, `DIR_NORTH` branch at
//! `:833-841`). [`super::step`] sequences the outcome; this module owns
//! whether there is one.
//!
//! # Polled pre-movement, at rest, against the tile the player faces
//!
//! Real animated-door tiles are solid, so a completed-step check can never
//! reach one -- [`engine::overworld::PlayerState::try_start_resolved_step`]'s
//! collision test rejects the step that would land on it. Upstream instead
//! calls `TryDoorWarp` *before* `PlayerStep`, gated on `heldDirection2 &&
//! dpadDirection == playerDirection` (`:170-178`), and reads the facing tile
//! through `GetInFrontOfPlayerPosition` (`:200-210`).
//!
//! # Why there is no drain-frame poll
//!
//! A walked approach enters the door on the call *after* the approach's own
//! walk animation drains, not on the drain call itself, and that is
//! upstream's own timing rather than a rounding of it. Upstream advances a
//! walk in CB2 and reads `tileTransitionState` in CB1, before that frame's
//! CB2 (`pokeemerald/src/main.c:188-195` calls `callback1` then `callback2`;
//! `pokeemerald/src/overworld.c:1438-1454`). `NpcTakeStep` returns TRUE on
//! the very call that applies a walk's last pixel
//! (`pokeemerald/src/event_object_movement.c:8300-8313`, over the 16-entry
//! `sStep1Funcs` at `:8233-8249`), and that return is what sets
//! `heldMovementFinished` (`:5024-5028`), so
//! `UpdatePlayerAvatarTransitionState` can only observe `T_TILE_CENTER` on
//! the *next* frame's CB1 (`pokeemerald/src/field_player_avatar.c:901-917`),
//! and only there does `FieldGetPlayerInput` set the `heldDirection2` this
//! poll is gated on (`field_control_avatar.c:95-112`).
//!
//! [`super::step`]'s pre-movement stage is that CB1; the post-movement
//! remainder of a call is the same frame's CB2, where upstream processes no
//! field input at all. So the drain call has no `TryDoorWarp` to mirror, and
//! polling the door there would accept a held direction one frame before
//! upstream could read it -- entering on a release the player made in time.

use engine::overworld::{
    trigger_animated_door_warp, Direction, MapRuntime, WarpTrigger, ELEVATION_TRANSITION,
};

/// The tile one step from `position` in `direction`, and the elevation to query it at.
///
/// Mirrors `GetInFrontOfPlayerPosition`'s own elevation rule
/// (`pokeemerald/src/field_control_avatar.c:200-210`, the same rule
/// [`engine::overworld::facing_object_event`] applies for object-event
/// interaction lookups): the *standing* tile's own elevation decides whether
/// the facing tile is queried at `elevation` or as an
/// [`ELEVATION_TRANSITION`] wildcard.
fn facing_tile(
    runtime: &MapRuntime<'_>,
    position: (i32, i32),
    elevation: u8,
    direction: Direction,
) -> ((i32, i32), u8) {
    let (dx, dy) = direction.delta();
    let facing = (position.0 + dx, position.1 + dy);
    let facing_elevation = match runtime.metatile_cell(position.0, position.1) {
        Some(cell) if cell.elevation != ELEVATION_TRANSITION => elevation,
        _ => ELEVATION_TRANSITION,
    };
    (facing, facing_elevation)
}

/// The warp an animated door one step ahead in `held_toward_facing` would
/// fire, if that tile is one.
///
/// `held_toward_facing` is the frame's held direction, already required to
/// equal the pre-movement facing by the caller -- upstream sets
/// `heldDirection` and `heldDirection2` from the same condition
/// (`field_control_avatar.c:109-113`), so the arrow poll's own value is this
/// one and no second "held matches facing" test is computed here.
///
/// Whether this frame is *allowed* to reach the door at all -- the arrow
/// check and the A-press interaction lookup both having fallen through, and
/// the player being at rest -- is [`super::step`]'s sequencing, matching
/// upstream's early `return TRUE`s ahead of `TryDoorWarp` (`:164-178`).
pub(super) fn trigger(
    runtime: &MapRuntime<'_>,
    position: (i32, i32),
    elevation: u8,
    held_toward_facing: Direction,
) -> Option<WarpTrigger> {
    let ((x, y), facing_elevation) = facing_tile(runtime, position, elevation, held_toward_facing);
    trigger_animated_door_warp(runtime, x, y, facing_elevation, held_toward_facing)
}

#[cfg(test)]
mod tests {
    use super::trigger;
    use crate::flow::overworld_phase::test_support::{
        littleroot_lab_door_scene, littleroot_runtime,
    };
    use engine::overworld::Direction;

    /// The decision proper: standing south of Littleroot's lab door at
    /// `(7, 16)` and holding North resolves that door's own warp event.
    #[test]
    fn holding_north_below_the_lab_door_resolves_its_warp() {
        let scene = littleroot_lab_door_scene();
        let runtime = littleroot_runtime(&scene);

        assert!(
            trigger(&runtime, (7, 17), 3, Direction::North).is_some(),
            "the tile north of (7, 17) is MB_ANIMATED_DOOR with a warp event"
        );
    }

    /// `TryDoorWarp` only ever opens a door the player is walking *into*
    /// from the south (`field_control_avatar.c:833-841` is its lone
    /// `DIR_NORTH` branch), so the same tile is inert from any other side.
    #[test]
    fn only_a_northward_approach_opens_the_lab_door() {
        let scene = littleroot_lab_door_scene();
        let runtime = littleroot_runtime(&scene);

        for (from, direction) in [
            ((7, 15), Direction::South),
            ((6, 16), Direction::East),
            ((8, 16), Direction::West),
        ] {
            assert!(
                trigger(&runtime, from, 3, direction).is_none(),
                "{direction:?} into the door tile from {from:?} must not warp"
            );
        }
    }

    /// An ordinary tile ahead is not a door, however the player faces it --
    /// the poll reads the *facing* tile, never the one they stand on.
    #[test]
    fn an_ordinary_tile_ahead_resolves_nothing() {
        let scene = littleroot_lab_door_scene();
        let runtime = littleroot_runtime(&scene);

        assert!(
            trigger(&runtime, (7, 18), 3, Direction::North).is_none(),
            "(7, 17) is ordinary ground, so facing it from (7, 18) must not warp"
        );
        assert!(
            trigger(&runtime, (7, 16), 3, Direction::North).is_none(),
            "standing *on* the door tile and facing north reads (7, 15), not the door"
        );
    }
}
