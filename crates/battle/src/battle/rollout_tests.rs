//! Lock states no public action reaches: a series carried into an attempt a
//! type block cancels, and into a sleeping action that does not.

use assets::{MoveId, SpeciesId};

use super::battler_state::MoveLock;
use super::trainer::fixed_ivs;
use super::{Battle, BattleEvent, BattlerSide, PlayerAction};
use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::pokemon::BattlePokemon;
use crate::status1::Status1;

const RATTATA: SpeciesId = SpeciesId(19);
const SHEDINJA: SpeciesId = SpeciesId(303);

/// Replays zero for every draw and counts them.
struct CountingRng(usize);

impl BattleRng for CountingRng {
    fn next_u16(&mut self) -> u16 {
        self.0 += 1;
        0
    }
}

fn mon(species: SpeciesId, moves: &[MoveId]) -> BattlePokemon {
    BattlePokemon::new(&Dex::new(), species, 50, fixed_ivs(255), 0, moves.to_vec())
        .expect("dex-resident")
}

fn locked_battle(enemy: BattlePokemon, moves: &[MoveId], rng: &mut CountingRng) -> Battle {
    let mut battle = Battle::new(Dex::new(), mon(RATTATA, moves), enemy, false, rng)
        .expect("a valid wild battle");
    battle.battler_states[BattlerSide::Player.index()].set_move_lock(Some(MoveLock {
        move_id: moves[0],
        attempts_left: 3,
    }));
    battle
}

#[test]
fn a_type_block_in_a_later_attempt_cancels_the_series_after_one_draw() {
    let mut rng = CountingRng(0);
    let mut battle = locked_battle(
        mon(SHEDINJA, &[MoveId::SPLASH]),
        &[MoveId::ICE_BALL, MoveId::SPLASH],
        &mut rng,
    );
    let before = rng.0;

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .expect("a legal turn");

    assert!(events.contains(&BattleEvent::WonderGuardBlocked {
        by_player: true,
        move_id: MoveId::ICE_BALL,
    }));
    assert_eq!(
        battle.battler_state(BattlerSide::Player).locked_move(),
        None
    );
    assert_eq!(
        battle.player().moves()[0].pp,
        20,
        "a locked attempt is free"
    );
    assert_eq!(
        rng.0 - before,
        1 + 1 + 1,
        "turn start, the wild pick, then the accuracy roll alone"
    );
}

#[test]
fn the_sleep_canceller_neither_ends_nor_advances_the_series() {
    let mut rng = CountingRng(0);
    let mut battle = locked_battle(
        mon(SHEDINJA, &[MoveId::SPLASH]),
        &[MoveId::ROLLOUT, MoveId::SPLASH],
        &mut rng,
    );
    battle.player.set_status1(Status1::Asleep { turns: 3 });

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .expect("a legal turn");

    assert!(events.contains(&BattleEvent::FastAsleep {
        by_player: true,
        move_id: MoveId::ROLLOUT,
    }));
    let state = battle.battler_state(BattlerSide::Player);
    assert_eq!(state.locked_move(), Some(MoveId::ROLLOUT));
    assert_eq!(state.lock_attempts_left(), Some(3));
}
