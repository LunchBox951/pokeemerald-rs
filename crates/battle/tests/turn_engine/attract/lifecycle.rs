//! Infatuation ends with either battler's time on the field, and an
//! infatuated Oblivious holder is refused at admission.

use super::*;
use assets::AbilityId;
use battle::{BattleError, BattleOutcome};

/// A landing Tackle: accuracy, non-critical, damage roll, discarded
/// secondary-effect draw.
const TACKLE_HIT: [u16; 4] = [0, 1, 0, 0];

fn at_one_hp(mut mon: BattlePokemon) -> BattlePokemon {
    mon.apply_damage(mon.current_hp() - 1);
    mon
}

#[test]
fn the_attractor_fainting_frees_the_battler_it_infatuated() {
    // The faster infatuated player gets through its coin and knocks out the
    // enemy that attracted it.
    let player = infatuated_by(mon(RATTATA, MALE, &[TACKLE]), BattlerSide::Enemy);
    let enemy = at_one_hp(mon(ZIGZAGOON, FEMALE, &[SPLASH]));

    let (battle, events, rng) = wild_turn(
        player,
        enemy,
        &[&PREFIX, &[LOVE_LETS_MOVE_THROUGH], &TACKLE_HIT],
    );

    rng.assert_exhausted();
    assert!(
        events.contains(&BattleEvent::Fainted { by_player: false }),
        "{events:?}"
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
    assert!(!battle.player().volatiles().infatuated());
}

#[test]
fn an_infatuated_battler_that_faints_keeps_nothing() {
    let player = at_one_hp(infatuated_by(
        mon(ZIGZAGOON, MALE, &[TACKLE]),
        BattlerSide::Enemy,
    ));
    let enemy = mon(RATTATA, FEMALE, &[TACKLE]);

    let (battle, events, rng) = wild_turn(player, enemy, &[&PREFIX, &TACKLE_HIT]);

    rng.assert_exhausted();
    assert!(
        events.contains(&BattleEvent::Fainted { by_player: true }),
        "the faster enemy knocks it out before its coin: {events:?}"
    );
    assert!(!battle.player().volatiles().infatuated());
}

#[test]
fn an_infatuated_oblivious_holder_is_refused_before_any_draw() {
    let holder = infatuated_by(mon(ILLUMISE, FEMALE, &[SPLASH]), BattlerSide::Player);
    let mut rng = SequenceRng::new([]);

    let result = Battle::new(
        Dex::new(),
        mon(MUDKIP, MALE, &[SPLASH]),
        holder,
        false,
        &mut rng,
    );

    assert!(matches!(
        result,
        Err(BattleError::UnportedAbilityInteraction(
            AbilityId::OBLIVIOUS
        ))
    ));
    assert_eq!(rng.draws(), 0);
}
