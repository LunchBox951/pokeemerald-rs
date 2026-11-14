//! `GAME_STAT_STEPS` against the Route 101 coordinate event and the wild roll:
//! `tookStep` is set before `TryStartStepBasedScript` and
//! `CheckStandardWildEncounter` run, so a landing that triggers either still
//! counts (`src/field_control_avatar.c:116-118,155-163`).

use super::super::test_support::*;
use super::support::*;
use crate::new_game;
use engine::overworld::metatile_behavior::MB_TALL_GRASS;
use engine::overworld::{Direction, PlayerState};
use engine::rng::Rng;
use engine::save::GameStat;

#[test]
fn the_landing_that_starts_the_coordinate_event_is_counted_before_it_claims_input() {
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    let mut phase = route_101_trigger_phase(PlayerState::new(
        (tx - 3, ty),
        ROUTE_101_TRIGGER_ELEVATION,
        Direction::East,
    ));
    phase.set_fresh_lead_for_test(new_game::provisional_starter());

    walk_east(&mut phase, 2);
    assert!(!phase.is_first_battle_active());
    assert_eq!(phase.save1.game_stats.get(GameStat::Steps), 2);

    walk_one_tile_east(&mut phase);
    assert_eq!(phase.player.position(), (tx, ty));
    assert!(
        phase.is_first_battle_active(),
        "the coordinate event claimed the landing frame"
    );
    assert_eq!(
        phase.save1.game_stats.get(GameStat::Steps),
        3,
        "and the step that triggered it was counted first"
    );
}

#[test]
fn the_landing_that_rolls_a_wild_encounter_is_counted() {
    // The first rolled grass step encounters with this seed (see
    // `precedence::the_route_101_trigger_suppresses_the_wild_encounter_roll_on_its_own_tile`).
    const SEED: u32 = 17;
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    let mut phase = route_101_trigger_phase_with_special_tiles(
        PlayerState::new((tx - 5, ty), ROUTE_101_TRIGGER_ELEVATION, Direction::East),
        &[(trigger_tile_cell(), MB_TALL_GRASS)],
    );
    phase
        .save1
        .event_data
        .var_set(VAR_ROUTE101_STATE, 2)
        .expect("VAR_ROUTE101_STATE is an ordinary var id");
    phase.rng = Rng::new(SEED);
    phase.set_fresh_lead_for_test(new_game::provisional_starter());

    walk_east(&mut phase, 4);
    assert!(!phase.is_wild_battle_active());
    assert_eq!(phase.save1.game_stats.get(GameStat::Steps), 4);

    walk_one_tile_east(&mut phase);
    assert!(
        phase.is_wild_battle_active(),
        "fixture precondition: the fifth step rolls the encounter"
    );
    assert_eq!(phase.save1.game_stats.get(GameStat::Steps), 5);
}
