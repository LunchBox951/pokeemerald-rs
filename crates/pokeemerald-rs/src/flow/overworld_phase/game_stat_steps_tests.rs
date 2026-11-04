//! `GAME_STAT_STEPS` (`include/constants/game_stat.h:9`): one saturating
//! increment per completed, non-forced field step, taken ahead of every
//! step-based consumer (`src/field_control_avatar.c:89-122,155-163`).
//!
//! Route 101's coordinate event and the wild-encounter ordering pins live
//! in `first_battle_trigger_tests::game_stat_steps`.

use super::test_support::*;
use super::OverworldPhase;
use assets::MapId;
use engine::overworld::metatile_behavior::{MB_ANIMATED_DOOR, MB_CAVE, MB_MUDDY_SLOPE};
use engine::overworld::{
    Direction, PlayerState, RUN_FRAMES_PER_TILE, TURN_IN_PLACE_FRAMES, WALK_FRAMES_PER_TILE,
};
use engine::rng::Rng;
use engine::save::GameStat;
use platform::{ButtonState, Buttons};

const FLAG_SYS_B_DASH: u16 = 0x8C0;

fn steps(phase: &OverworldPhase) -> u32 {
    phase.save1.game_stats.get(GameStat::Steps)
}

/// A walk bumps the counter on the call after its animation drains, the
/// call upstream runs `tookStep` on, and never again for the same tile.
#[test]
fn an_ordinary_walk_increments_once_after_its_animation_drains() {
    let mut phase = synthetic_phase(PlayerState::new((4, 6), 3, Direction::East), None);
    assert_eq!(steps(&phase), 0, "fixture precondition: a fresh game");

    for frame in 1..=u32::from(WALK_FRAMES_PER_TILE) {
        phase.step(held(Buttons::RIGHT));
        assert_eq!(
            steps(&phase),
            0,
            "frame {frame}: the step is still draining"
        );
    }
    assert_eq!(phase.player.position(), (5, 6));
    assert!(!phase.player.in_transit());

    phase.step(ButtonState::new());
    assert_eq!(steps(&phase), 1, "the landing call counts the step");

    for _ in 0..4 {
        phase.step(ButtonState::new());
    }
    assert_eq!(steps(&phase), 1, "a landing is counted exactly once");
}

/// Held B with the running shoes crosses in `RUN_FRAMES_PER_TILE` calls and
/// counts the same single step.
#[test]
fn an_ordinary_run_increments_once_after_its_animation_drains() {
    let mut phase = OverworldPhase::for_test(
        crate::overworld::test_support::synthetic_scene(20, 20),
        MapId("MAP_LITTLEROOT_TOWN"),
        PlayerState::new((10, 10), 3, Direction::East),
        None,
    );
    phase.save1.event_data.flag_set(FLAG_SYS_B_DASH).unwrap();

    for frame in 1..=u32::from(RUN_FRAMES_PER_TILE) {
        phase.step(held(Buttons::B | Buttons::RIGHT));
        assert_eq!(steps(&phase), 0, "frame {frame}: the run is still draining");
    }
    assert_eq!(phase.player.position(), (11, 10));
    assert!(!phase.player.in_transit());

    phase.step(ButtonState::new());
    assert_eq!(steps(&phase), 1);
    phase.step(ButtonState::new());
    assert_eq!(steps(&phase), 1);
}

#[test]
fn idle_frames_do_not_increment() {
    let mut phase = synthetic_phase(PlayerState::new((4, 6), 3, Direction::East), None);
    for _ in 0..40 {
        phase.step(ButtonState::new());
    }
    assert_eq!(steps(&phase), 0);
}

/// Walking into the map edge plays the bump and never reaches
/// `runningState == MOVING` (`field_control_avatar.c:116-118`).
#[test]
fn a_blocked_attempt_does_not_increment() {
    let mut phase = synthetic_phase(PlayerState::new((0, 5), 3, Direction::West), None);
    for _ in 0..3 * u32::from(WALK_FRAMES_PER_TILE) {
        phase.step(held(Buttons::LEFT));
    }
    phase.step(ButtonState::new());
    assert_eq!(
        phase.player.position(),
        (0, 5),
        "fixture precondition: the edge blocked every attempt"
    );
    assert_eq!(steps(&phase), 0);
}

/// A turn in place moves nothing, so nothing lands.
#[test]
fn a_stationary_turn_does_not_increment() {
    let mut phase = synthetic_phase(PlayerState::new((4, 6), 3, Direction::West), None);
    for _ in 0..TURN_IN_PLACE_FRAMES {
        phase.step(held(Buttons::RIGHT));
    }
    for _ in 0..4 {
        phase.step(ButtonState::new());
    }
    assert_eq!(
        phase.player.position(),
        (4, 6),
        "fixture precondition: the player only turned"
    );
    assert_eq!(phase.player.facing(), Direction::East);
    assert_eq!(steps(&phase), 0);
}

/// Landing on a forced-movement tile never sets `tookStep`
/// (`field_control_avatar.c:116-122`). The player is walked onto a muddy
/// slope and the call that observes that landing must leave the counter alone.
#[test]
fn a_forced_landing_does_not_increment() {
    let scene = crate::overworld::test_support::synthetic_scene_with_special_tile(
        10,
        10,
        (5, 6),
        MB_MUDDY_SLOPE,
    );
    let mut phase = OverworldPhase::for_test(
        scene,
        ONE_F,
        PlayerState::new((4, 6), 3, Direction::East),
        None,
    );
    for _ in 0..WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::RIGHT));
    }
    assert_eq!(phase.player.position(), (5, 6));
    assert!(
        phase.player.field_input_suppressed(),
        "fixture precondition: the slope's landing call is suppressed"
    );

    phase.step(ButtonState::new());
    assert_eq!(steps(&phase), 0);
}

/// A door-shaped warp claims the frame its landing triggers on, yet the step
/// onto the door tile is already counted: `tookStep` is set in
/// `FieldGetPlayerInput` before `TryStartStepBasedScript` runs
/// (`field_control_avatar.c:116-118,155-163`).
#[test]
fn a_landing_that_triggers_a_door_warp_still_increments_first() {
    const CAVE: MapId = MapId("MAP_GRANITE_CAVE_B1F");
    const FLOOR: (u16, u16) = (7, 5);
    const DOOR: (u16, u16) = (8, 5);

    let mut phase = OverworldPhase::for_test(
        crate::overworld::test_support::synthetic_scene_with_special_tiles_at_elevations(
            10,
            10,
            &[(FLOOR, MB_CAVE, 3), (DOOR, MB_ANIMATED_DOOR, 0)],
        ),
        CAVE,
        PlayerState::new((6, 5), 3, Direction::East),
        None,
    );
    phase.rng = Rng::new(IMMUNITY_SEED);
    phase.wild_table_screen = Some((CAVE, true));

    for _ in 0..WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::RIGHT));
    }
    assert_eq!(steps(&phase), 0);
    // The floor tile's landing call counts and starts the door crossing.
    phase.step(held(Buttons::RIGHT));
    assert_eq!(steps(&phase), 1);
    assert_eq!(phase.player.position(), (8, 5));
    for _ in 0..WALK_FRAMES_PER_TILE - 1 {
        phase.step(held(Buttons::RIGHT));
    }
    assert_eq!(steps(&phase), 1, "the door crossing is still draining");

    // The door tile's landing call: its warp claims the frame (the roll never
    // records the tile), and the step onto it is counted all the same.
    phase.step(held(Buttons::RIGHT));
    assert_eq!(
        phase.wild.prev_metatile_behavior(),
        MB_CAVE,
        "fixture precondition: the door warp claimed the landing frame"
    );
    assert_eq!(steps(&phase), 2);
}
