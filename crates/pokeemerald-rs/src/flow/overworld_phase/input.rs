//! The held-direction and single-frame movement mechanics
//! [`super::OverworldPhase::step`] drives (module split of
//! [`crate::flow::overworld_phase::step`], issue #210, `oop-boundaries`):
//! resolving this frame's held D-pad direction ([`held_direction`]),
//! feeding one input poll to [`PlayerState`] and unconditionally advancing
//! its walk-animation timer ([`advance_player_one_frame`]), the
//! preempt-or-move branch around it
//! ([`advance_or_skip_for_preempt`]), and latching a completed step's
//! landing tile for [`super::step`]'s drain-frame warp check
//! ([`latch_landing`]). Pulled out of `step` itself purely to keep both
//! files under the `oop-boundaries` size guideline -- these are still one
//! concept with [`super::OverworldPhase::step`], just not one file.

use engine::overworld::{
    ConnectedMapData, Direction, PlayerState, StepOutcome, TilePos, WarpTrigger,
};
use platform::{ButtonState, Buttons};

/// The held D-pad direction to feed [`PlayerState::step`] this frame, or
/// `None` if no direction is held. Priority order (first held wins)
/// transcribes upstream `RunFieldInput`'s own `dpadDirection` resolution
/// exactly: `if (heldKeys & DPAD_UP) ... else if (DPAD_DOWN) ... else if
/// (DPAD_LEFT) ... else if (DPAD_RIGHT)`
/// (`pokeemerald/src/field_control_avatar.c:123-129`) -- up, then down,
/// then left, then right, with only one cardinal direction ever selected
/// per call regardless of which other D-pad bits also happen to be held
/// `(behavioral-fidelity)`.
pub(super) fn held_direction(buttons: ButtonState) -> Option<Direction> {
    let held = buttons.held();
    if held.intersects(Buttons::UP) {
        Some(Direction::North)
    } else if held.intersects(Buttons::DOWN) {
        Some(Direction::South)
    } else if held.intersects(Buttons::LEFT) {
        Some(Direction::West)
    } else if held.intersects(Buttons::RIGHT) {
        Some(Direction::East)
    } else {
        None
    }
}

/// Apply this frame's movement, unless `preempting_arrow_trigger` already
/// consumed it -- [`super::OverworldPhase::step`]'s movement branch,
/// pulled out (as its own free function, disjointly borrowing `player`/
/// `pending_landing`/`event_data` rather than all of `self` -- see that
/// call site's own comment) to keep that method under clippy's
/// `too_many_lines` limit. Returns a [`StepOutcome::Crossed`] landing (issue
/// #177) for [`super::OverworldPhase::step`] to hand to
/// [`super::OverworldPhase::cross_connection`] once
/// `runtime`'s borrow there has ended: that rebind needs `&mut self.scene`,
/// which can't happen while `runtime` -- an immutable borrow of the same
/// field -- is still in use for this frame's interaction/warp checks, so it
/// can't be applied directly from here either.
pub(super) fn advance_or_skip_for_preempt(
    player: &mut PlayerState,
    pending_landing: &mut Option<TilePos>,
    direction: Option<Direction>,
    runtime: &engine::overworld::MapRuntime<'_>,
    maps: &impl ConnectedMapData,
    event_data: &engine::event_data::EventData,
    preempting_arrow_trigger: Option<WarpTrigger>,
) -> Option<(assets::MapId, TilePos)> {
    if preempting_arrow_trigger.is_some() {
        // The walk-animation tick still advances every frame, even one a
        // warp preempts movement on (module docs on
        // `advance_player_one_frame`) -- a no-op here since the caller only
        // reaches this arm when the player was already at rest, but called
        // anyway so that contract stays unconditional. This path latches
        // nothing of its own, so "at rest => no latched landing" has to hold
        // on its own here rather than being restored by anything downstream:
        // `step`'s own drain-frame `take_if` does run on a preempted frame
        // (the player is at rest, so its `in_transit` guard passes), but the
        // preempting warp claims the frame before either the door check or
        // the wild-encounter roll can look at what it returned.
        debug_assert!(
            pending_landing.is_none(),
            "at rest implies `pending_landing` is None: a landing latched here would be taken \
             on a frame the preempting warp has already claimed, and so would never reach a \
             door check or an encounter roll"
        );
        player.tick();
        return None;
    }

    let outcome = advance_player_one_frame(player, direction, runtime, maps, event_data);
    latch_landing(pending_landing, outcome);
    match outcome {
        StepOutcome::Crossed {
            to_map,
            to_position,
        } => Some((to_map, to_position)),
        _ => None,
    }
}

/// Latch the tile a just-applied frame of movement started walking onto, if
/// any -- the `pending_landing` half of
/// [`super::OverworldPhase::step`]'s `tookStep` bookkeeping, factored
/// out of that method's movement branch (which is at clippy's
/// `too_many_lines` limit) rather than inlined there.
///
/// See [`super::OverworldPhase::step`]'s "Warp timing" section for why
/// the landing is latched at step *start* and only tested for a warp once
/// its walk animation has drained, a later frame.
///
/// [`StepOutcome::Crossed`] (issue #177) is deliberately *not* latched
/// here, unlike [`StepOutcome::Advanced`]: its landing tile is expressed in
/// the map the crossing entered, not whichever map `pending_landing` still
/// means at this point, and this function has no way to confirm that map's
/// data actually loaded before committing to it.
/// [`super::OverworldPhase::cross_connection`] (called
/// separately, after this function returns -- see
/// [`super::OverworldPhase::step`]'s own "Map-edge connection
/// crossing" comment for why it can't happen here) does that check and
/// latches `pending_landing` onto the entered map's coordinate space only
/// once it has passed.
fn latch_landing(pending_landing: &mut Option<TilePos>, outcome: StepOutcome) {
    match outcome {
        StepOutcome::Advanced { to, .. } => *pending_landing = Some(to),
        StepOutcome::Crossed { .. }
        | StepOutcome::Idle
        | StepOutcome::Turned(_)
        | StepOutcome::Blocked { .. } => {}
    }
}

/// Feed one input poll to `player` against `runtime`, then unconditionally
/// advance its walk-animation timer -- upstream's own per-frame shape,
/// reproduced exactly `(behavioral-fidelity)`.
///
/// Every `MOVE_SPEED_NORMAL` walk direction's `Step0` handler both starts
/// *and* applies the first frame of movement in the same call: e.g.
/// `MovementAction_WalkNormalDown_Step0`
/// (`pokeemerald/src/event_object_movement.c:5354-5358`) calls
/// `InitMovementNormal` (which zeroes the sprite's step timer,
/// `sTimer = 0`) and then immediately falls through to
/// `MovementAction_WalkNormalDown_Step1` -> `UpdateMovementNormal` ->
/// `NpcTakeStep`, which applies `sStep1Funcs[0]`'s 1px offset and advances
/// the timer to `1` -- all before that frame is ever drawn. So the very
/// first rendered frame of a step is already 1px into the tile crossing
/// (`step_progress() == 1`, not `0`), and a full tile crossing takes
/// exactly [`engine::overworld::WALK_FRAMES_PER_TILE`] (16) *rendered*
/// frames, matching `sStepTimes[MOVE_SPEED_NORMAL] ==
/// ARRAY_COUNT(sStep1Funcs) == 16` (`event_object_movement.c`'s
/// `sStep1Funcs`/`sStepTimes` tables).
///
/// A prior version of this function skipped the tick on the frame a step
/// began, on the theory that [`crate::overworld::viewport::build_tilemaps`]'s
/// scroll-lag math needed a `0`-progress frame rendered first to "cancel"
/// [`PlayerState::position`]'s one-tile logical jump. Reviewed and reverted:
/// that reasoning didn't match upstream (verified above) and, empirically,
/// made a held direction take 17 rendered frames to cross one tile instead
/// of 16, plus duplicated a camera position at every tile boundary (a
/// one-frame stutter of its own) -- see this function's own tests for the
/// corrected contract.
/// The returned [`StepOutcome`] is fed back to the caller (issue #163):
/// [`super::OverworldPhase::step`] latches an `Advanced` step's
/// landing tile and, once the 16-frame walk animation above has drained,
/// checks it via
/// [`engine::overworld::trigger_door_warp`]/[`super::OverworldPhase::warp_to`]
/// -- so walking onto the bedroom's stair warp at `(7, 1)` (the map's only
/// warp event, the same one [`crate::new_game`]'s `SPAWN_*` derives the
/// spawn from) transitions to `MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F` on the
/// frame the step *finishes*, matching upstream's `tookStep` gate (that
/// method's own doc comment).
///
/// `maps` (issue #177) is generic, not hardcoded to
/// [`super::connections::MapConnections`], so this function stays directly
/// unit-testable against a small synthetic map graph the way
/// [`engine::overworld::player`]'s own tests already are (this module's own
/// tests do exactly that for the coordinate-translation edge cases);
/// [`super::OverworldPhase::step`] is the only production call site,
/// and it always passes
/// `&`[`super::connections::MapConnections`]. Indoor maps carry no edge
/// connections in the generated [`assets::MapHeaderTable`] at all, so
/// `resolve_connection` never finds a candidate there regardless of which
/// `maps` is passed; an outdoor map's own connections (Littleroot Town's
/// north edge to Route 101, the early playable slice's own crossing) resolve
/// for real
/// against [`super::connections::MapConnections`].
pub(super) fn advance_player_one_frame(
    player: &mut PlayerState,
    direction: Option<Direction>,
    runtime: &engine::overworld::MapRuntime<'_>,
    maps: &impl ConnectedMapData,
    event_data: &engine::event_data::EventData,
) -> StepOutcome {
    let outcome = player.step(direction, runtime, maps, event_data);
    player.tick();
    outcome
}
