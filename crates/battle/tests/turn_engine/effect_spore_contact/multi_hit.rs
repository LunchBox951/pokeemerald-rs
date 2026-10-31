//! Effect Spore on multi-hit contact moves (Double Slap): one roll after every
//! strike, the roll a later strike still spends after an earlier one
//! poisoned, sleep ending the remaining strikes, and the status event landing
//! ahead of the `MultiHit` summary and the holder's faint.

use super::*;

/// Two Double Slap strikes of a surviving holder; each strike is crit, damage,
/// then its contact draws.
fn double_slap_through_two_strikes(first: &[u16], second: &[u16]) -> (Battle, Vec<BattleEvent>) {
    let dex = Dex::new();
    let attacker = rattata(&dex, 60, vec![DOUBLE_SLAP]);
    let holder = shroomish(&dex, 40);
    let draws = script(&[
        &[
            CONSTRUCTION_TURN_NUMBER,
            TURN_START,
            ENEMY_MOVE_SELECTION,
            ACCURACY_HITS,
            HIT_COUNT_TWO,
            NO_CRIT,
            DAMAGE_ROLL,
        ],
        first,
        &[NO_CRIT, DAMAGE_ROLL],
        second,
        &[DOUBLE_SLAP_EFFECT_CHANCE],
        &SLOWER_HITS,
    ]);
    take_player_move(attacker, holder, dex, &draws)
}

#[test]
fn a_multi_hit_contact_move_rolls_effect_spore_after_every_hit() {
    let (battle, events) = double_slap_through_two_strikes(&[SPORE_MISSES], &TRIGGER_POISON);

    assert_eq!(battle.player().status1(), Status1::Poisoned, "{events:?}");
    assert_eq!(
        effect_spore_events(&events),
        vec![&BattleEvent::PoisonedByEffectSpore {
            by_player: true,
            move_id: DOUBLE_SLAP,
        }]
    );
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
    let poisoned_at = events
        .iter()
        .position(|event| matches!(event, BattleEvent::PoisonedByEffectSpore { .. }))
        .unwrap();
    assert!(hits[0] < hits[1] && hits[1] < poisoned_at, "{events:?}");
    assert!(events.contains(&BattleEvent::MultiHit {
        by_player: true,
        move_id: DOUBLE_SLAP,
        hits: 2,
    }));
}

#[test]
fn a_second_strike_spends_its_roll_even_when_the_first_already_poisoned() {
    let (battle, events) = double_slap_through_two_strikes(&TRIGGER_POISON, &TRIGGER_POISON);

    assert_eq!(battle.player().status1(), Status1::Poisoned, "{events:?}");
    assert_eq!(effect_spore_events(&events).len(), 1, "{events:?}");
}

#[test]
fn effect_spore_sleep_ends_the_remaining_strikes_but_reports_the_landed_ones() {
    let dex = Dex::new();
    let attacker = rattata(&dex, 60, vec![DOUBLE_SLAP]);
    let holder = shroomish(&dex, 40);
    // The second strike's crit and damage draws are absent: sleep ends the loop.
    let draws = script(&[
        &[
            CONSTRUCTION_TURN_NUMBER,
            TURN_START,
            ENEMY_MOVE_SELECTION,
            ACCURACY_HITS,
            HIT_COUNT_TWO,
            NO_CRIT,
            DAMAGE_ROLL,
        ],
        &TRIGGER_SLEEP,
        &[DOUBLE_SLAP_EFFECT_CHANCE],
        &SLOWER_HITS,
    ]);
    let (battle, events) = take_player_move(attacker, holder, dex, &draws);

    assert_eq!(
        battle.player().status1(),
        Status1::Asleep { turns: 5 },
        "{events:?}"
    );
    let hits = events
        .iter()
        .filter(|event| {
            matches!(
                event,
                BattleEvent::Hit {
                    by_player: true,
                    ..
                }
            )
        })
        .count();
    assert_eq!(hits, 1, "{events:?}");
    assert!(events.contains(&BattleEvent::MultiHit {
        by_player: true,
        move_id: DOUBLE_SLAP,
        hits: 1,
    }));
}

#[test]
fn a_multi_hit_strike_that_kills_the_holder_publishes_its_status_before_the_faint() {
    let dex = Dex::new();
    let attacker = rattata(&dex, 60, vec![DOUBLE_SLAP]);
    let holder = with_hp(shroomish(&dex, 40), 1);
    let draws = script(&[
        &[
            CONSTRUCTION_TURN_NUMBER,
            TURN_START,
            ENEMY_MOVE_SELECTION,
            ACCURACY_HITS,
            HIT_COUNT_TWO,
            NO_CRIT,
            DAMAGE_ROLL,
        ],
        &TRIGGER_POISON,
        &[DOUBLE_SLAP_EFFECT_CHANCE],
    ]);
    let (battle, events) = take_player_move(attacker, holder, dex, &draws);

    assert_eq!(battle.player().status1(), Status1::Poisoned, "{events:?}");
    let poisoned = position(
        &events,
        &BattleEvent::PoisonedByEffectSpore {
            by_player: true,
            move_id: DOUBLE_SLAP,
        },
    );
    let summary = position(
        &events,
        &BattleEvent::MultiHit {
            by_player: true,
            move_id: DOUBLE_SLAP,
            hits: 1,
        },
    );
    let fainted = position(&events, &BattleEvent::Fainted { by_player: false });
    assert!(poisoned < summary && summary < fainted, "{events:?}");
}
