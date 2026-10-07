//! The approach cutscene's tile-by-tile timing, lock handoff, and icon
//! countdown, through the real
//! [`OverworldPhase::step`](super::super::OverworldPhase::step).

use engine::overworld::{Direction, PlayerState, WALK_FRAMES_PER_TILE};
use platform::{ButtonState, Buttons};

use crate::flow::tests::held;

use super::support::*;

// -- The approach sequence (S-5, issue #300) ---------------------------------
//
// Same honest cut as `support::seed_battle`, one stage earlier: no real Route
// 103 sight trainer's party constructs today, so
// `begin_sight_trainer_approach_if_seen` can never *reach* the approach with
// a real party -- but the sequence it would run is real, and so is the
// object event it runs on. `seed_approach` therefore builds the approach the
// trigger would build, from Rhett's own real extracted object event, around
// the same proven-constructible stand-in battle.

/// The approach's tile-by-tile timing, through the real
/// [`OverworldPhase::step`], with a direction held down the whole way: the
/// icon holds for sixty frames, the walked tile is committed at its own
/// *start* (`InitNpcForMovement`) and takes sixteen frames, and the player
/// cannot move for any of it -- upstream's `lockall`/`FreezeObjectEvents`,
/// expressed as frame ownership.
#[test]
fn the_approach_owns_every_frame_and_walks_one_tile_per_sixteen() {
    let (rx, ry) = RHETT_TILE;
    // Two tiles south of Rhett: `approachDistance` 2, so one walked tile.
    let start = (rx, ry + 2);
    let mut phase = route_103_phase(PlayerState::new(start, 3, Direction::South));
    seed_approach(&mut phase, 1);

    // Frames 1..=63: the handoff frame, the icon animation and its dispatch
    // frames, less the one frame that commits; frame 64 commits.
    for frame in 1..=EXCLAMATION_ICON_FRAMES + ICON_DISPATCH_FRAMES {
        phase.step(held(Buttons::DOWN));
        assert_eq!(
            approaching_trainer(&phase).position(),
            RHETT_TILE,
            "frame {frame}: the trainer stands still through the lock handoff and its icon"
        );
        assert_eq!(
            phase.player.position(),
            start,
            "frame {frame}: a held direction must not move the player mid-cutscene"
        );
        assert!(
            !phase.player.in_transit(),
            "frame {frame}: no step even started"
        );
    }

    // The trainer task's observation of the icon's removal (the sixty-fourth
    // frame after the seeded trigger) is the first walked tile's own start.
    phase.step(held(Buttons::DOWN));
    assert_eq!(approaching_trainer(&phase).position(), (rx, ry + 1));
    assert_eq!(
        approaching_trainer(&phase).previous_position(),
        RHETT_TILE,
        "the vacated tile is retained for the length of the animation"
    );

    for frame in 1..usize::from(WALK_FRAMES_PER_TILE) {
        phase.step(held(Buttons::DOWN));
        assert_eq!(
            approaching_trainer(&phase).position(),
            (rx, ry + 1),
            "walk frame {frame}: one tile takes sixteen frames"
        );
        assert_eq!(phase.player.position(), start);
    }

    assert!(
        !phase.is_sight_trainer_battle_active(),
        "no battle before the approach finishes"
    );
    assert!(
        phase.party_lead.is_some(),
        "the lead stays in the party for the whole approach -- a save taken mid-cutscene \
         must persist an honest pre-battle overworld"
    );
}

/// `PlayerFaceApproachingTrainer` (`trainer_see.c:508-528`), end to end: the
/// trainer stops on the tile *beside* the player, both turn to face each
/// other, and the trainer's own template is rewritten so a later respawn
/// keeps the stopping tile and facing.
#[test]
fn the_trainer_stops_beside_the_player_and_both_turn_to_face_each_other() {
    let (rx, ry) = RHETT_TILE;
    let start = (rx, ry + 2);
    // Facing *away* from the approaching trainer, so the turn is visible.
    let mut phase = route_103_phase(PlayerState::new(start, 3, Direction::South));
    seed_approach(&mut phase, 1);

    // The lock handoff, sixty icon frames, the icon's dispatch frames, sixteen
    // walk frames, one frame for the trainer's own `MOVEMENT_ACTION_FACE_PLAYER`,
    // then the stop itself.
    for _ in 0..=LOCK_HANDOFF_AT_REST
        + EXCLAMATION_ICON_FRAMES
        + ICON_DISPATCH_FRAMES
        + usize::from(WALK_FRAMES_PER_TILE)
    {
        phase.step(ButtonState::new());
    }
    assert_eq!(
        phase.player.facing(),
        Direction::North,
        "the player turns to the opposite of the trainer's facing"
    );
    assert_eq!(
        phase.player.position(),
        start,
        "and turning is not stepping"
    );

    let trainer = approaching_trainer(&phase);
    assert_eq!(
        trainer.position(),
        (rx, ry + 1),
        "the trainer stops on the tile beside the player, never on it"
    );
    assert_eq!(trainer.facing(), Direction::South);
    assert_eq!(
        trainer.movement_type(),
        assets::MovementType::FaceDown,
        "SetTrainerMovementType pins the stopped facing instead of resuming the patrol"
    );
    assert_eq!(
        trainer.template_position(),
        (rx, ry + 1),
        "OverrideTemplateCoordsForObjectEvent: a respawn uses the stopping tile"
    );
    assert_eq!(
        trainer.template_movement_type(),
        assets::MovementType::FaceDown,
        "TryOverrideTemplateCoordsForObjectEvent: ...and the stopping facing"
    );
}

/// `TRSEE_PLAYER_FACE_WAIT` (`trainer_see.c:531-539`,
/// `ApproachStage::PlayerFaceWait`'s own docs): the turn frame is not the
/// speech frame. Pack-independent: the box opening or the no-pack battle
/// fallback starting are both "the speech has opened"
/// (`advance_intro_message`'s own docs).
#[test]
fn the_speech_does_not_open_on_the_frame_the_player_is_turned() {
    let (rx, ry) = RHETT_TILE;
    // Adjacent already (`walk_tiles` 0), facing away, so the turn frame is
    // unambiguous.
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 1), 3, Direction::South));
    seed_approach(&mut phase, 0);

    let mut frames = 0;
    while phase.player.facing() == Direction::South {
        phase.step(ButtonState::new());
        frames += 1;
        assert!(frames < 200, "the trainer must eventually turn the player");
    }
    assert_eq!(
        phase.player.facing(),
        Direction::North,
        "setup: this is the turning frame"
    );
    assert!(
        phase.dialog.is_none() && !phase.is_sight_trainer_battle_active(),
        "setup: the turning frame itself never opens the speech"
    );

    // `TRSEE_PLAYER_FACE_WAIT`.
    phase.step(ButtonState::new());
    assert!(
        phase.dialog.is_none() && !phase.is_sight_trainer_battle_active(),
        "the frame after the turn is upstream's `TRSEE_PLAYER_FACE_WAIT`, which only sets the \
         task's followup func -- the intro speech cannot open until at least the frame after \
         that (trainer_see.c:531-539)"
    );

    // The intro stage itself, the frame after that.
    phase.step(ButtonState::new());
    assert!(
        phase.dialog.is_some() || phase.is_sight_trainer_battle_active(),
        "the speech (or its no-pack battle fallback) must open on the frame after the wait"
    );
}

/// `PlayerFaceApproachingTrainer`'s own guard (`trainer_see.c:522-523`): a
/// step the player committed on the very frame the cone reached them (so
/// [`engine::overworld::PlayerState::in_transit`] is still `true` when the
/// approach starts owning the frame) must be allowed to finish -- ticked
/// while the icon countdown holds, the same as any other spawned object
/// event's held movement would keep animating upstream while
/// `lockfortrainer` waits -- before the trainer turns them around. Turning
/// a player who is still mid-tile would spin them in place under an
/// animation upstream never lets get that far.
#[test]
fn the_players_in_flight_step_finishes_before_the_trainer_turns_them() {
    let (rx, ry) = RHETT_TILE;
    // Two tiles further south than Rhett's own stopping tile, so a full
    // three-tile walk-up (`walk_tiles = 2`) leaves the trainer adjacent to
    // where the player ends up below -- realistic geometry, not just a
    // timing fixture.
    let start = (rx, ry + 2);
    let mut phase = route_103_phase(PlayerState::new(start, 3, Direction::South));

    // Commit one ordinary step *before* the approach exists -- mirroring
    // the frame order `begin_sight_trainer_approach_if_seen`'s own docs
    // describe: `PlayerState::position` already reflects the just-stepped
    // tile a frame before the cone check can see it, so the approach can
    // start with the player still mid-transit (finding's own probe: this
    // is genuinely `(67, 8)` with `step_progress() == 1`).
    phase.step(held(Buttons::DOWN));
    assert!(
        phase.player.in_transit(),
        "fixture precondition: the step must still be animating when the approach starts"
    );
    assert_eq!(phase.player.step_progress(), 1, "fixture precondition");
    assert_eq!(phase.player.position(), (rx, ry + 3));

    seed_approach(&mut phase, 2);
    let original_facing = phase.player.facing();

    // Run every frame from the icon to the moment the trainer turns the
    // player, asserting the invariant `PlayerFaceApproachingTrainer` itself
    // enforces: the player's own facing must never change while their step
    // is still in flight.
    let mut frames = 0;
    while phase.player.facing() == original_facing {
        phase.step(ButtonState::new());
        frames += 1;
        assert!(
            frames < 200,
            "the trainer must eventually turn the player -- the approach is stuck"
        );
    }
    assert!(
        !phase.player.in_transit(),
        "the player must not be turned while still mid-step -- upstream blocks \
         `PlayerFaceApproachingTrainer` on `ObjectEventClearHeldMovementIfFinished` until the \
         held walk is done (trainer_see.c:522-523)"
    );
    assert_eq!(
        phase.player.step_progress(),
        0,
        "the step must have fully drained, not merely stopped mid-count"
    );
    assert_eq!(
        phase.player.position(),
        (rx, ry + 3),
        "the committed step's destination tile is unaffected by the turn"
    );
    assert_eq!(
        phase.player.facing(),
        Direction::North,
        "the player turns to the opposite of the trainer's facing"
    );
}

/// The other half of that in-flight step (PR #407 review): it drains under
/// the lock, and the tile it drains onto is owed nothing afterwards.
///
/// Upstream reads `input->tookStep`/`input->checkStandardWildEncounter` off
/// the *current* frame's `gPlayerAvatar.tileTransitionState`
/// (`field_control_avatar.c:116-121`) -- neither is latched -- and their one
/// reader, `ProcessPlayerFieldInput`, is skipped entirely while the
/// approach's `LockPlayerFieldControls` holds (`overworld.c:1445-1455`),
/// even though `UpdatePlayerAvatarTransitionState` keeps draining that state
/// ahead of the lock check (`:1442`, `field_player_avatar.c:901-917`). So
/// the single `T_TILE_CENTER` frame passes with nobody looking, and that
/// tile's coordinate event, door warp and wild-encounter roll are genuinely
/// skipped -- `UnlockPlayerFieldControls` gives nothing back.
///
/// This port's `pending_landing` is the latch upstream does not have, so it
/// has to be dropped here rather than survive into the first ordinary frame
/// after the fight -- where it would either fire that tile's events a whole
/// cutscene late (no direction held) or be silently overwritten by the next
/// step's landing (a direction held), and would break
/// `advance_or_skip_for_preempt`'s "at rest implies no latched landing"
/// invariant either way.
#[test]
fn a_step_draining_under_the_lock_leaves_its_tile_owed_nothing() {
    let (rx, ry) = RHETT_TILE;
    let start = (rx, ry + 2);
    let mut phase = route_103_phase(PlayerState::new(start, 3, Direction::South));

    // Same frame order as the test above: one ordinary step committed a
    // frame before the cone reaches the player, so the approach starts with
    // the landing tile already latched and its walk still animating.
    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.pending_landing,
        Some((rx, ry + 3)),
        "fixture precondition: the ordinary step latched its landing tile"
    );
    assert!(
        phase.player.in_transit(),
        "fixture precondition: with the walk still to drain"
    );

    seed_approach(&mut phase, 2);

    for frame in 0..usize::from(WALK_FRAMES_PER_TILE) {
        phase.step(held(Buttons::DOWN));
        assert!(
            phase.sight_approach.is_some(),
            "frame {frame}: the approach is still running (the icon alone outlasts the walk)"
        );
        assert!(
            phase.pending_landing.is_none(),
            "frame {frame}: a landing whose completion frame falls under the lock is never \
             observed upstream, so it must not be held open here"
        );
    }

    assert!(
        !phase.player.in_transit(),
        "the held walk drained under the icon, exactly as it would with no trainer watching"
    );
    assert_eq!(
        phase.player.position(),
        (rx, ry + 3),
        "the player really is standing on that tile -- only its step events are skipped"
    );
    assert!(
        !phase.mid_step(),
        "and nothing is left owing it, so the first ordinary frame after the cutscene starts \
         from a clean at-rest stance"
    );
}

/// The icon countdown waits for the player (PR #407 review): upstream's
/// `lockfortrainer` blocks the script on `IsFreezeObjectAndPlayerFinished`
/// until `IsPlayerStandingStill()` (`scrcmd.c:2193-2208`,
/// `event_object_lock.c:130-147`), and only then does
/// `EventScript_TrainerApproach` reach `DoTrainerApproach`'s
/// `FieldEffectStart` (`trainer_battle.inc:1-7`). A cone that catches the
/// player mid-step must therefore spend the *full* sixty icon frames after
/// the step drains -- overlapping the two would start the walk-up early by
/// however many frames the step had left.
#[test]
fn the_icon_countdown_holds_until_the_players_step_drains() {
    let (rx, ry) = RHETT_TILE;
    let start = (rx, ry + 2);
    let mut phase = route_103_phase(PlayerState::new(start, 3, Direction::South));

    // One ordinary step committed the frame before the cone check, so the
    // approach starts with the walk still animating (the two tests above).
    phase.step(held(Buttons::DOWN));
    assert!(phase.player.in_transit(), "fixture precondition");
    seed_approach(&mut phase, 2);

    let mut drain_frames = 0;
    while phase.player.in_transit() {
        phase.step(held(Buttons::DOWN));
        drain_frames += 1;
        assert_eq!(
            approaching_trainer(&phase).position(),
            RHETT_TILE,
            "drain frame {drain_frames}: the trainer must not start walking while \
             `lockfortrainer` would still be waiting on the player"
        );
        assert!(
            drain_frames <= usize::from(WALK_FRAMES_PER_TILE),
            "the held step must drain within one tile's animation"
        );
    }

    // The drain-completing frame is not an icon frame, and neither are the
    // two frames the freeze task and the native poll spend after it, so the
    // first walked tile commits once the icon's removal is seen -- the
    // `LOCK_HANDOFF_AFTER_DRAIN + EXCLAMATION_ICON_FRAMES + ICON_DISPATCH_FRAMES`th
    // after the drain.
    for frame in 1..LOCK_HANDOFF_AFTER_DRAIN + EXCLAMATION_ICON_FRAMES + ICON_DISPATCH_FRAMES {
        phase.step(held(Buttons::DOWN));
        assert_eq!(
            approaching_trainer(&phase).position(),
            RHETT_TILE,
            "frame {frame} after the drain: no icon frame has expired, so the walk-up must \
             not start early"
        );
    }
    phase.step(held(Buttons::DOWN));
    assert_eq!(
        approaching_trainer(&phase).position(),
        (rx, ry + 1),
        "the frame the task sees the icon removed, two handoff frames after the drain, \
         commits the first walked tile"
    );
}

/// The at-rest counterpart: the trigger frame runs the freeze task, the
/// next frame is the native poll, and only the frame after is the icon's
/// first -- so a standing player's approach starts no earlier than
/// `LOCK_HANDOFF_AT_REST` frames after the trigger.
#[test]
fn an_at_rest_approach_spends_a_handoff_frame_before_the_icon() {
    let (rx, ry) = RHETT_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 2), 3, Direction::South));
    seed_approach(&mut phase, 1);

    for _ in 0..LOCK_HANDOFF_AT_REST + EXCLAMATION_ICON_FRAMES + ICON_DISPATCH_FRAMES - 1 {
        phase.step(ButtonState::new());
        assert_eq!(approaching_trainer(&phase).position(), RHETT_TILE);
    }
    phase.step(ButtonState::new());
    assert_eq!(approaching_trainer(&phase).position(), (rx, ry + 1));
}

/// `Task_FreezeObjectAndPlayer` runs `PlayerFreeze` the frame after a
/// caught run drains (`event_object_lock.c:130-146`,
/// `field_player_avatar.c:1039-1046`), so the running sheet's paused cell
/// must not survive into the exclamation icon.
#[test]
fn a_run_caught_by_a_cone_stands_once_the_lock_sees_it_settle() {
    let (rx, ry) = RHETT_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 2), 3, Direction::South));
    phase.save1.event_data.flag_set(0x8C0).unwrap();
    let mut run = ButtonState::new();
    run.update(Buttons::B | Buttons::DOWN);
    run.update(Buttons::B | Buttons::DOWN);

    phase.step(run);
    assert!(
        phase.player.in_transit() && phase.player.transit_running(),
        "fixture precondition: a held-B run is in flight"
    );
    seed_approach(&mut phase, 2);

    while phase.player.in_transit() {
        phase.step(run);
    }
    assert!(
        phase.player.run_pose_held(),
        "the drain frame still shows the finished run's paused cell"
    );

    phase.step(run);
    assert!(
        !phase.player.run_pose_held(),
        "the freeze task's `PlayerFreeze` stands the player up on the first settled frame"
    );
    assert_eq!(
        approaching_trainer(&phase).position(),
        RHETT_TILE,
        "and does so before the approach leaves the lock handoff"
    );
}

/// [`OverworldPhase::tick_player_under_approach_lock`]'s own two-part
/// contract, pinned directly: one frame of the player's held walk really
/// runs, and the latched landing is dropped with it.
///
/// The frame that *starts* an approach ([`OverworldPhase::step`]'s early
/// return on `SightTrainerOutcome::owns_frame`) is a locked frame like
/// every other one and goes through this same method: upstream's lock gates
/// CB1's `ProcessPlayerFieldInput`/`PlayerStep` only
/// (`overworld.c:1445-1455`), while the held movement runs from CB2's
/// `AnimateSprites` afterwards (`:1469`, `main.c:188-195`), so skipping that
/// frame's tick stalls the walk animation by exactly one frame (PR #407
/// review). That call site itself cannot be driven from a test today -- no
/// real Route 103 sight trainer's party constructs, so `step` never reaches
/// its `ApproachStarted` arm at all
/// (`sight_trainer_trigger::tests::every_sight_trainers_real_party_fails_to_construct_for_exactly_these_reasons`)
/// -- so the method both frames now share is what gets pinned.
#[test]
fn a_locked_frame_advances_the_players_walk_and_drops_its_latched_landing() {
    let (rx, ry) = RHETT_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 2), 3, Direction::South));

    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.player.step_progress(),
        1,
        "fixture precondition: one ordinary frame of an in-flight step"
    );
    assert_eq!(
        phase.pending_landing,
        Some((rx, ry + 3)),
        "fixture precondition: with its landing tile latched"
    );

    phase.tick_player_under_approach_lock();

    assert_eq!(
        phase.player.step_progress(),
        2,
        "a locked frame still animates the held walk -- the lock stops input, not animation"
    );
    assert!(
        phase.pending_landing.is_none(),
        "and the landing whose completion frame the lock eats goes with it"
    );
}

/// A standing player's held wall bump ends on a locked frame, as the lock's
/// forced face action does (`PlayerFreeze`, `field_player_avatar.c:1039-1046`).
#[test]
fn a_locked_frame_cancels_a_standing_players_wall_bump() {
    let (rx, ry) = RHETT_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 1), 3, Direction::North));
    phase.step(held(Buttons::UP));
    assert!(phase.player.bump_active(), "setup: Rhett blocks the step");

    phase.tick_player_under_approach_lock();

    assert!(!phase.player.bump_active());
}

/// The trigger frame itself drains the last tick of the player's step: the
/// drain frame is `step.rs`'s trigger path, not the approach driver, so the
/// two after-drain handoff frames must still be spent before the icon.
#[test]
fn a_trigger_frame_that_drains_the_step_still_spends_the_after_drain_handoff() {
    let (rx, ry) = RHETT_TILE;
    // Same geometry as `the_icon_countdown_holds_until_the_players_step_drains`.
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 2), 3, Direction::South));
    phase.synthetic_sight_trainer = Some(assets::trainers::TrainerId(STAND_IN_TRAINER));
    assert!(
        phase.party_lead.is_none(),
        "setup: the cone refuses while walking"
    );

    phase.step(held(Buttons::DOWN));
    assert!(phase.player.in_transit(), "setup: step committed");
    while phase.player.step_progress() < WALK_FRAMES_PER_TILE - 1 {
        phase.step(ButtonState::new());
        assert!(phase.sight_approach.is_none());
    }
    assert!(phase.player.in_transit(), "setup: one tick left");

    phase.rng = engine::rng::Rng::new(7);
    phase.party_lead = Some(overwhelming_lead());
    phase.step(ButtonState::new()); // trigger frame == drain frame
    assert!(
        phase.sight_approach.is_some(),
        "setup: the trigger claimed the frame"
    );
    assert!(
        !phase.player.in_transit(),
        "setup: the trigger frame drained the step"
    );

    let mut frames_after_drain = 0;
    while format!("{:?}", phase.sight_approach).contains("LockHandoff") {
        phase.step(ButtonState::new());
        frames_after_drain += 1;
    }
    assert_eq!(
        frames_after_drain, LOCK_HANDOFF_AFTER_DRAIN,
        "the icon must start only after both after-drain handoff frames"
    );
}

/// The production counterpart of `EXCLAMATION_DISPATCH_FRAMES`: frames between
/// the icon's sixtieth animation frame and the first walked tile.
const ICON_DISPATCH_FRAMES: usize = 3;

/// The trigger-to-first-tile count in production order: the real cone, the
/// real trigger frame and the pre-icon handoff, no seeded approach. Frame 0
/// is the trigger frame; the icon's own sixty frames start after the handoff,
/// and the first tile lands only once the icon has been removed and the task
/// has seen it.
#[test]
fn a_standing_trigger_walks_its_first_tile_after_the_icon_dispatch_frames() {
    let (rx, ry) = RHETT_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 2), 3, Direction::South));
    phase.rng = engine::rng::Rng::new(7);
    phase.party_lead = Some(overwhelming_lead());
    phase.synthetic_sight_trainer = Some(assets::trainers::TrainerId(STAND_IN_TRAINER));
    phase.step(ButtonState::new());
    assert!(phase.sight_approach.is_some(), "setup: frame 0 triggers");
    let rng_after_trigger = phase.rng.state();

    let first_tile_frame = LOCK_HANDOFF_AT_REST + EXCLAMATION_ICON_FRAMES + ICON_DISPATCH_FRAMES;
    for frame in 1..first_tile_frame {
        phase.step(ButtonState::new());
        assert_eq!(
            approaching_trainer(&phase).position(),
            RHETT_TILE,
            "frame {frame}: the icon is still up or not yet seen removed"
        );
    }
    phase.step(ButtonState::new());
    assert_eq!(
        approaching_trainer(&phase).position(),
        (rx, ry + 1),
        "frame {first_tile_frame} commits the first tile"
    );
    assert_eq!(
        phase.rng.state(),
        rng_after_trigger,
        "the approach draws no RNG"
    );
}
