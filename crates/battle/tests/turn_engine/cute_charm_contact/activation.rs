//! The roll and the infatuation it writes.

use super::*;

#[test]
fn a_zero_roll_infatuates_an_opposite_gender_attacker_with_the_holders_slot() {
    let (battle, events) = attacker_hits_enemy_holder(
        rattata_attacker(MALE, &[TACKLE]),
        skitty_holder(FEMALE),
        &[CHARM_TRIGGERS],
    );

    assert_eq!(
        battle.player().volatiles().infatuated_by,
        Some(BattlerSide::Enemy),
        "{events:?}"
    );
    assert_eq!(battle.enemy().volatiles().infatuated_by, None);
    assert_eq!(cute_charm_events(&events), vec![&charmed_player(TACKLE)]);
    let hit = events
        .iter()
        .position(|event| {
            matches!(
                event,
                BattleEvent::Hit {
                    by_player: true,
                    ..
                }
            )
        })
        .unwrap();
    assert!(
        hit < position(&events, &charmed_player(TACKLE)),
        "{events:?}"
    );
}

#[test]
fn a_female_attacker_is_infatuated_by_a_male_holder_too() {
    let (battle, events) = attacker_hits_enemy_holder(
        rattata_attacker(FEMALE, &[TACKLE]),
        skitty_holder(MALE),
        &[CHARM_TRIGGERS],
    );

    assert!(battle.player().volatiles().infatuated(), "{events:?}");
    assert_eq!(cute_charm_events(&events).len(), 1);
}

#[test]
fn a_nonzero_roll_spends_exactly_one_draw_and_infatuates_nothing() {
    for roll in [CHARM_ONE, CHARM_TWO] {
        let (battle, events) = attacker_hits_enemy_holder(
            rattata_attacker(MALE, &[TACKLE]),
            skitty_holder(FEMALE),
            &[roll],
        );

        assert!(!battle.player().volatiles().infatuated(), "{events:?}");
        assert!(cute_charm_events(&events).is_empty(), "{events:?}");
    }
}

#[test]
fn an_enemy_attacker_is_infatuated_with_the_player_holders_slot() {
    let (battle, events) = enemy_attacker_hits_player_holder(
        rattata_attacker(MALE, &[TACKLE]),
        skitty_holder(FEMALE),
        &[CHARM_TRIGGERS],
    );

    assert_eq!(
        battle.enemy().volatiles().infatuated_by,
        Some(BattlerSide::Player),
        "{events:?}"
    );
    assert_eq!(
        cute_charm_events(&events),
        vec![&BattleEvent::InfatuatedByCuteCharm {
            by_player: false,
            move_id: TACKLE,
        }]
    );
}

/// The state is the one Attract writes: the next action announces the love
/// and spends the `CANCELER_IN_LOVE` coin (`battle_util.c:2200`-`:2219`).
#[test]
fn the_infatuation_cancels_the_attackers_next_action_like_attract() {
    let mut rng = SequenceRng::new(script(&[
        &SETUP,
        &TACKLE_HITS,
        &[CHARM_TRIGGERS],
        // Turn two: turn start, enemy selection, then the infatuated
        // player's coin.
        &[0, 0, LOVE_IMMOBILIZES],
    ]));
    let mut battle = Battle::new(
        Dex::new(),
        rattata_attacker(MALE, &[TACKLE]),
        skitty_holder(FEMALE),
        false,
        &mut rng,
    )
    .unwrap();
    battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    rng.assert_exhausted();

    assert!(events.contains(&BattleEvent::InLove { by_player: true }));
    assert!(events.contains(&BattleEvent::ImmobilizedByLove {
        by_player: true,
        move_id: TACKLE,
    }));
}

#[test]
fn an_odd_love_coin_lets_the_charmed_attackers_move_continue() {
    let mut rng = SequenceRng::new(script(&[
        &SETUP,
        &TACKLE_HITS,
        &[CHARM_TRIGGERS],
        &[0, 0, LOVE_LETS_MOVE_THROUGH],
        &TACKLE_HITS,
        &[CHARM_ONE],
    ]));
    let mut battle = Battle::new(
        Dex::new(),
        rattata_attacker(MALE, &[TACKLE]),
        skitty_holder(FEMALE),
        false,
        &mut rng,
    )
    .unwrap();
    battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    rng.assert_exhausted();

    assert!(events.contains(&BattleEvent::InLove { by_player: true }));
    assert!(events.iter().any(|event| matches!(
        event,
        BattleEvent::Hit {
            by_player: true,
            ..
        }
    )));
}
