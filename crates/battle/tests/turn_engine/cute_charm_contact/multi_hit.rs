//! Cute Charm on multi-hit contact moves (Double Slap): one roll after every
//! strike that leaves the holder alive.

use super::*;

const DOUBLE_SLAP: MoveId = MoveId::DOUBLE_SLAP;
const HIT_COUNT_TWO: u16 = 0;
const NO_CRIT: u16 = 1;
const DAMAGE_ROLL: u16 = 0;
const DOUBLE_SLAP_EFFECT_CHANCE: u16 = 0;
const ACCURACY_HITS: u16 = 0;

/// Two Double Slap strikes of a surviving holder; each strike is crit, damage,
/// then its contact draws.
fn double_slap_through_two_strikes(first: &[u16], second: &[u16]) -> (Battle, Vec<BattleEvent>) {
    let draws = script(&[
        &SETUP,
        &[ACCURACY_HITS, HIT_COUNT_TWO, NO_CRIT, DAMAGE_ROLL],
        first,
        &[NO_CRIT, DAMAGE_ROLL],
        second,
        &[DOUBLE_SLAP_EFFECT_CHANCE],
    ]);
    player_turn(
        rattata_attacker(MALE, &[DOUBLE_SLAP]),
        skitty_holder(FEMALE),
        &draws,
    )
}

#[test]
fn the_second_strike_rolls_again_after_a_first_roll_that_did_not_trigger() {
    let (battle, events) = double_slap_through_two_strikes(&[CHARM_ONE], &[CHARM_TRIGGERS]);

    assert!(battle.player().volatiles().infatuated(), "{events:?}");
    assert_eq!(
        cute_charm_events(&events),
        vec![&charmed_player(DOUBLE_SLAP)]
    );
    let charmed_at = position(&events, &charmed_player(DOUBLE_SLAP));
    let hits: Vec<usize> = events
        .iter()
        .enumerate()
        .filter(|(_, event)| {
            matches!(
                event,
                BattleEvent::Hit {
                    by_player: true,
                    ..
                }
            )
        })
        .map(|(index, _)| index)
        .collect();
    assert_eq!(hits.len(), 2, "{events:?}");
    assert!(hits[1] < charmed_at, "{events:?}");
}

#[test]
fn a_second_strike_spends_its_roll_even_when_the_first_already_infatuated() {
    let (battle, events) = double_slap_through_two_strikes(&[CHARM_TRIGGERS], &[CHARM_TRIGGERS]);

    assert_eq!(
        battle.player().volatiles().infatuated_by,
        Some(BattlerSide::Enemy),
        "{events:?}"
    );
    assert_eq!(cute_charm_events(&events).len(), 1, "{events:?}");
}

#[test]
fn a_strike_that_faints_the_holder_ends_the_move_without_a_roll() {
    let mut holder = skitty_holder(FEMALE);
    holder.apply_damage(holder.stats().max_hp - 1);
    let draws = script(&[
        &SETUP,
        &[ACCURACY_HITS, HIT_COUNT_TWO, NO_CRIT, DAMAGE_ROLL],
        &[DOUBLE_SLAP_EFFECT_CHANCE],
    ]);
    let (battle, events) = player_turn(rattata_attacker(MALE, &[DOUBLE_SLAP]), holder, &draws);

    assert!(battle.enemy().is_fainted(), "{events:?}");
    assert!(cute_charm_events(&events).is_empty(), "{events:?}");
}
