//! The held-direction and single-frame movement mechanics that
//! [`super::OverworldPhase::step`] drives: resolving the held D-pad direction
//! ([`held_direction`]), feeding one input poll to [`PlayerState`] and then
//! advancing its walk-animation timer ([`advance_player_one_frame`]), the
//! preempt-or-move branch around it ([`advance_or_skip_for_preempt`]), and
//! latching a completed step's landing tile for the following `step` call to
//! observe ([`latch_landing`]).

use engine::overworld::metatile_behavior::{is_forced_movement_input_tile, MB_NORMAL};
use engine::overworld::{ConnectedMapData, Direction, PlayerState, StepOutcome, TilePos};
use platform::{ButtonState, Buttons};

/// The held D-pad direction to feed [`PlayerState::step`] this frame, or
/// `None` if no direction is held.
///
/// Exactly one direction is selected, in the priority up, down, left, right,
/// whatever other D-pad bits are held. This is upstream `RunFieldInput`'s
/// `dpadDirection` if/else chain (`pokeemerald/src/field_control_avatar.c:123-129`)
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

/// Whether B is held this frame. Level-triggered, not a press edge, matching
/// upstream's `heldKeys & B_BUTTON` run input (`field_player_avatar.c:658-668`).
pub(super) fn run_held(buttons: ButtonState) -> bool {
    buttons.held().intersects(Buttons::B)
}

/// Apply this frame's movement, unless `movement_preempted` says a warp or an
/// NPC interaction already consumed it.
///
/// A preempted frame still ticks the player's walk animation and releases the
/// run pose, and returns [`StepOutcome::Idle`]. Otherwise the held direction
/// and run input go through [`advance_player_one_frame`], and an
/// [`StepOutcome::Advanced`] landing is latched into `pending_landing`.
///
/// A [`StepOutcome::Crossed`] landing is returned unlatched for
/// [`super::OverworldPhase::step`] to hand to
/// [`super::OverworldPhase::cross_connection`]: the rebind needs
/// `&mut self.scene`, which `step` cannot take while `runtime` still borrows
/// it for the frame's interaction and warp checks.
pub(super) fn advance_or_skip_for_preempt(
    player: &mut PlayerState,
    pending_landing: &mut Option<TilePos>,
    buttons: ButtonState,
    runtime: &engine::overworld::MapRuntime<'_>,
    maps: &impl ConnectedMapData,
    event_data: &engine::event_data::EventData,
    movement_preempted: bool,
) -> StepOutcome {
    if movement_preempted {
        // Preempted frames still tick, as every frame does. The tick is a
        // no-op here: every preempt needs the player at rest when the call
        // began, and `step` consumes any ready landing then ("Warp timing" in
        // `step`'s docs), so nothing is latched and none is added.
        debug_assert!(
            pending_landing.is_none(),
            "a preempted frame implies `pending_landing` is None: every preempt needs the \
             player at rest when the call began, and `step` takes a ready landing there, \
             ahead of this function"
        );
        player.tick();
        player.release_run_pose();
        return StepOutcome::Idle;
    }

    let outcome = advance_player_one_frame(
        player,
        held_direction(buttons),
        run_held(buttons),
        runtime,
        maps,
        event_data,
    );
    latch_landing(pending_landing, outcome);
    outcome
}

/// Split a frame's movement outcome into its two deferred follow-ups: a
/// started turn latches `pending_turn` at the player's tile, and a map-edge
/// crossing is returned for [`super::OverworldPhase::cross_connection`].
pub(super) fn settle_outcome(
    pending_turn: &mut Option<TilePos>,
    player: &PlayerState,
    outcome: StepOutcome,
) -> Option<(assets::MapId, TilePos)> {
    match outcome {
        StepOutcome::Turned(_) => {
            *pending_turn = Some(player.position());
            None
        }
        StepOutcome::Crossed {
            to_map,
            to_position,
        } => Some((to_map, to_position)),
        _ => None,
    }
}

/// The tile this frame's encounter check observes: a completed step's
/// `landed` tile, else a completed stationary turn's. The turn latch is
/// consumed either way, so it cannot outlive its own observation.
pub(super) fn observe_tile(
    pending_turn: &mut Option<TilePos>,
    player: &PlayerState,
    runtime: &engine::overworld::MapRuntime<'_>,
    landed: Option<TilePos>,
) -> Option<TilePos> {
    landed.or(take_completed_turn(pending_turn, player, runtime))
}

/// Take the tile a stationary turn started on, once its busy timer has
/// drained and the player is at rest -- the turn's tile-centre observation
/// (`field_player_avatar.c:901-915` exempts turns from the stationary-animation
/// skip). Always consumed when ready, so a suppressed observation cannot fire
/// on a later idle frame; returned only when the tile is not a forced-movement
/// input tile (`field_control_avatar.c:116-122`).
fn take_completed_turn(
    pending_turn: &mut Option<TilePos>,
    player: &PlayerState,
    runtime: &engine::overworld::MapRuntime<'_>,
) -> Option<TilePos> {
    let (x, y) =
        pending_turn.take_if(|_| !player.in_transit() && player.turn_frames_remaining() == 0)?;
    let behavior = runtime.metatile_behavior(x, y).unwrap_or(MB_NORMAL);
    (!is_forced_movement_input_tile(behavior)).then_some((x, y))
}

/// Latch the tile a just-applied frame of movement started walking onto, if
/// any. This is the `pending_landing` half of upstream's `tookStep`
/// bookkeeping; [`super::OverworldPhase::step`]'s "Frame shape" section owns
/// when the latch is observed.
///
/// [`StepOutcome::Crossed`] is not latched, unlike [`StepOutcome::Advanced`]:
/// its landing tile is in the entered map's coordinates, and only
/// [`super::OverworldPhase::cross_connection`] can confirm that map's data
/// loaded. It latches `pending_landing` once that check passes.
fn latch_landing(pending_landing: &mut Option<TilePos>, outcome: StepOutcome) {
    match outcome {
        StepOutcome::Advanced { to, .. } => *pending_landing = Some(to),
        StepOutcome::Crossed { .. }
        | StepOutcome::Idle
        | StepOutcome::Turned(_)
        | StepOutcome::Blocked { .. } => {}
    }
}

/// Feed one input poll to `player` against `runtime`, then advance its
/// walk-animation timer, whether or not a step began
/// `(behavioral-fidelity)`.
///
/// Upstream applies a step's first frame of movement in the call that starts
/// it (`pokeemerald/src/event_object_movement.c:5354-5358`), so the first
/// rendered frame already has `step_progress() == 1` and a tile crossing takes
/// exactly [`engine::overworld::WALK_FRAMES_PER_TILE`] frames.
///
/// The returned [`StepOutcome`] lets [`super::OverworldPhase::step`] latch an
/// `Advanced` landing for observation after the animation drains.
///
/// `maps` is generic so tests can pass a small synthetic map graph;
/// production always passes [`super::connections::MapConnections`].
pub(super) fn advance_player_one_frame(
    player: &mut PlayerState,
    direction: Option<Direction>,
    run_held: bool,
    runtime: &engine::overworld::MapRuntime<'_>,
    maps: &impl ConnectedMapData,
    event_data: &engine::event_data::EventData,
) -> StepOutcome {
    let outcome = player.step_with_run(direction, run_held, runtime, maps, event_data);
    player.tick();
    outcome
}
