//! Pickup after a won sight-trainer battle (`Cmd_pickup`, reached through
//! `BattleScript_LocalBattleWonReward`,
//! `pokeemerald/data/battle_scripts_1.s:2936-2943`).

use assets::items::ItemId;
use battle::BattleOutcome;
use engine::overworld::{Direction, PlayerState};

use super::super::OverworldPhase;
use super::support::*;

/// Plays Rhett's stand-in battle for a level-50 Zigzagoon lead holding `item`.
fn sight_battle(item: ItemId, seed: u32) -> OverworldPhase {
    let (rx, ry) = RHETT_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 1), 3, Direction::North));
    // Zigzagoon's ability is Pickup; `SLASH` is move 163.
    seed_battle(
        &mut phase,
        TRAINER_RHETT,
        lead(288, 50, 163).with_held_item(item),
        seed,
    );
    let outcome = play_out_sight_battle(&mut phase, 64);
    assert_eq!(outcome, Some(BattleOutcome::PlayerWon));
    phase
}

fn after_draws(rng: &engine::rng::Rng, draws: usize) -> u32 {
    let mut rng = rng.clone();
    for _ in 0..draws {
        rng.next_u16();
    }
    rng.state()
}

/// A holder of a Potion is ineligible, so that run's stream is where Pickup
/// starts in the other, identical battle. Seed 12 ends on a stream whose next
/// two draws are 1320 (a hit) and 34005 (percentile 5): level 50 is tier 4,
/// first step, Repel.
#[test]
fn winning_a_sight_trainer_battle_rolls_pickup_for_the_lead() {
    let without = sight_battle(ItemId::POTION, 12);
    let with = sight_battle(ItemId::NONE, 12);

    assert_eq!(with.rng.state(), after_draws(&without.rng, 2));
    assert_eq!(with.lead_for_test().unwrap().held_item(), ItemId::REPEL);
    assert_eq!(without.lead_for_test().unwrap().held_item(), ItemId::POTION);
}
