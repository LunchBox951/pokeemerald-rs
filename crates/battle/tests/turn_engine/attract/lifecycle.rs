//! Infatuation ends with either battler's time on the field, and an
//! infatuated Oblivious holder is admitted and cured at its first move end.

use super::*;
use assets::AbilityId;
use battle::{BattleOutcome, CuredCondition};

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

fn cured_oblivious() -> BattleEvent {
    BattleEvent::AbilityCuredStatus {
        by_player: false,
        ability: AbilityId::OBLIVIOUS,
        condition: CuredCondition::Infatuation,
    }
}

fn splash(by_player: bool) -> [BattleEvent; 2] {
    [
        BattleEvent::UsedMove {
            by_player,
            move_id: SPLASH,
        },
        BattleEvent::NothingHappened {
            by_player,
            move_id: SPLASH,
        },
    ]
}

#[test]
fn an_infatuated_oblivious_holder_is_cured_once_its_move_ends() {
    let holder = infatuated_by(mon(ILLUMISE, FEMALE, &[SPLASH]), BattlerSide::Player);

    let (battle, events, rng) = wild_turn(
        mon(MUDKIP, MALE, &[SPLASH]),
        holder,
        &[&PREFIX, &[LOVE_LETS_MOVE_THROUGH]],
    );

    rng.assert_exhausted();
    assert_eq!(
        events,
        [
            vec![in_love(false)],
            splash(false).to_vec(),
            vec![cured_oblivious()],
            splash(true).to_vec(),
        ]
        .concat()
    );
    assert!(!battle.enemy().volatiles().infatuated());
}

#[test]
fn an_oblivious_holder_immobilized_by_love_is_cured_after_the_cancelled_move() {
    let holder = infatuated_by(mon(ILLUMISE, FEMALE, &[SPLASH]), BattlerSide::Player);

    let (battle, events, rng) = wild_turn(
        mon(MUDKIP, MALE, &[SPLASH]),
        holder,
        &[&PREFIX, &[LOVE_IMMOBILIZES]],
    );

    rng.assert_exhausted();
    assert_eq!(
        events,
        [
            vec![
                in_love(false),
                immobilized(false, SPLASH),
                cured_oblivious()
            ],
            splash(true).to_vec(),
        ]
        .concat()
    );
    assert!(!battle.enemy().volatiles().infatuated());
}
