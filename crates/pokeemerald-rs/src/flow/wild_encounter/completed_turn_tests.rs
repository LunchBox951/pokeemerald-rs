//! A completed stationary turn is a tile-centre observation upstream's
//! `ProcessPlayerFieldInput` polls `checkStandardWildEncounter` on
//! (`field_control_avatar.c:116-122`, `:162-163`; turns are exempt from the
//! stationary-animation skip at `field_player_avatar.c:901-915`). These pin
//! the port: the observation lands when the turn's busy timer has drained,
//! once, on the encounter gate alone.

use super::test_support::{held, player_mon, route_101_phase, ENCOUNTER_SEED, ROUTE_101};
use crate::flow::overworld_phase::OverworldPhase;
use assets::MoveId;
use engine::overworld::metatile_behavior::{MB_CRACKED_FLOOR, MB_NORMAL, MB_TALL_GRASS};
use engine::overworld::{Direction, PlayerState, TURN_IN_PLACE_FRAMES};
use engine::rng::Rng;
use platform::{ButtonState, Buttons};

const TILE: (i32, i32) = (5, 5);
const TILE_CELL: (u16, u16) = (5, 5);

fn grass_phase() -> OverworldPhase {
    route_101_phase(PlayerState::new(TILE, 3, Direction::East), TILE_CELL)
}

fn neutral(phase: &mut OverworldPhase) {
    phase.step(ButtonState::new());
}

/// Start a turn toward `button` and run the neutral calls that drain its
/// timer, leaving the phase one call short of the observation.
fn start_and_drain_turn(phase: &mut OverworldPhase, button: Buttons) {
    phase.step(held(button));
    for _ in 1..TURN_IN_PLACE_FRAMES {
        neutral(phase);
    }
    assert_eq!(phase.player.turn_frames_remaining(), 0);
}

#[test]
fn a_completed_turn_observes_the_tile_once_after_the_timer_drains() {
    let mut phase = grass_phase();
    phase.rng = Rng::new(ENCOUNTER_SEED);
    let rng_before = phase.rng.state();

    phase.step(held(Buttons::DOWN));
    for _ in 1..TURN_IN_PLACE_FRAMES {
        assert_eq!(phase.wild.immunity_steps(), 0);
        assert_eq!(phase.wild.prev_metatile_behavior(), MB_NORMAL);
        neutral(&mut phase);
    }
    assert_eq!(phase.player.turn_frames_remaining(), 0);
    assert_eq!(phase.wild.immunity_steps(), 0);
    assert_eq!(phase.rng.state(), rng_before);
    assert_eq!(phase.player.position(), TILE);
    assert!(!phase.player.in_transit());

    neutral(&mut phase);
    assert_eq!(phase.wild.immunity_steps(), 1);
    assert_eq!(phase.wild.prev_metatile_behavior(), MB_TALL_GRASS);

    for _ in 0..3 {
        neutral(&mut phase);
    }
    assert_eq!(phase.wild.immunity_steps(), 1, "one observation per turn");
    assert_eq!(phase.rng.state(), rng_before);
}

#[test]
fn a_completed_turn_on_ordinary_ground_remembers_its_behavior() {
    let mut phase = route_101_phase(PlayerState::new(TILE, 3, Direction::East), (1, 1));
    start_and_drain_turn(&mut phase, Buttons::DOWN);
    assert_eq!(phase.wild.immunity_steps(), 0);
    neutral(&mut phase);
    assert_eq!(phase.wild.immunity_steps(), 1);
    assert_eq!(phase.wild.prev_metatile_behavior(), MB_NORMAL);
}

/// The fifth observation is the first to roll, and a same-behavior grass
/// check draws no new-metatile permission first, so `Rng::new(0)` (first draw
/// 0) passes the rate check immediately.
#[test]
fn the_fifth_completed_turn_on_grass_rolls_after_immunity() {
    let mut phase = grass_phase();
    phase.rng = Rng::new(0);
    phase.party_lead = Some(player_mon(277, 50, vec![MoveId::POUND]));
    let rng_before = phase.rng.state();
    let directions = [Buttons::DOWN, Buttons::UP, Buttons::DOWN, Buttons::UP];

    for (turns, button) in directions.into_iter().enumerate() {
        start_and_drain_turn(&mut phase, button);
        neutral(&mut phase);
        assert_eq!(usize::from(phase.wild.immunity_steps()), turns + 1);
        assert_eq!(phase.rng.state(), rng_before, "immunity turns draw nothing");
        assert!(!phase.is_wild_battle_active());
    }

    start_and_drain_turn(&mut phase, Buttons::DOWN);
    assert!(!phase.is_wild_battle_active(), "not before completion");
    assert_eq!(phase.rng.state(), rng_before);
    neutral(&mut phase);
    assert!(phase.is_wild_battle_active());
    assert_eq!(phase.wild.immunity_steps(), 0, "a roll resets immunity");
    assert_ne!(phase.rng.state(), rng_before);
}

#[test]
fn a_failed_turn_roll_keeps_immunity_exhausted() {
    let mut phase = grass_phase();
    phase.rng = Rng::new(ENCOUNTER_SEED);
    phase.party_lead = Some(player_mon(277, 50, vec![MoveId::POUND]));
    // Seed 17's first draw (24107) fails the same-behavior rate check.
    let mut expected = Rng::new(ENCOUNTER_SEED);
    assert!(u32::from(expected.next_u16()) % 2880 >= 320);

    for button in [Buttons::DOWN, Buttons::UP, Buttons::DOWN, Buttons::UP] {
        start_and_drain_turn(&mut phase, button);
        neutral(&mut phase);
    }
    start_and_drain_turn(&mut phase, Buttons::DOWN);
    neutral(&mut phase);
    assert!(!phase.is_wild_battle_active());
    assert_eq!(phase.rng.state(), expected.state(), "exactly one draw");
    assert_eq!(phase.wild.immunity_steps(), 4);
}

#[test]
fn a_completed_turn_has_no_positional_side_effects() {
    let mut phase = grass_phase();
    start_and_drain_turn(&mut phase, Buttons::DOWN);
    neutral(&mut phase);
    assert!(!phase.mid_step());
    assert_eq!(phase.player.position(), TILE);
}

#[test]
fn a_completed_turn_on_a_forced_tile_never_observes() {
    let mut phase = OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene_with_special_tiles(
            10,
            10,
            &[(TILE_CELL, MB_CRACKED_FLOOR)],
        ),
        ROUTE_101,
        PlayerState::new(TILE, 3, Direction::East),
        None,
    );
    phase.rng = Rng::new(ENCOUNTER_SEED);
    let rng_before = phase.rng.state();
    start_and_drain_turn(&mut phase, Buttons::DOWN);
    for _ in 0..3 {
        neutral(&mut phase);
    }
    assert_eq!(phase.wild.immunity_steps(), 0);
    assert_eq!(phase.wild.prev_metatile_behavior(), MB_NORMAL);
    assert_eq!(phase.rng.state(), rng_before);
}
