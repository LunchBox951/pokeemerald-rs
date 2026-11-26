//! Applying Attract: accuracy, then Oblivious, then the gender and
//! reapplication guards.

use super::*;
use battle::battle::LastLandedMove;
use battle::StatStage;

const ATTRACT_PP: u8 = 15;

fn attract_turn(
    attacker: BattlePokemon,
    target: BattlePokemon,
    accuracy_roll: u16,
) -> (Battle, Vec<BattleEvent>) {
    let (battle, events, rng) = wild_turn(attacker, target, &[&PREFIX, &[accuracy_roll]]);
    rng.assert_exhausted();
    (battle, events)
}

fn infatuated_event() -> BattleEvent {
    BattleEvent::Infatuated {
        by_player: true,
        move_id: ATTRACT,
    }
}

fn failed_event() -> BattleEvent {
    BattleEvent::ButItFailed {
        by_player: true,
        move_id: ATTRACT,
    }
}

fn enemy_landed(battle: &Battle) -> LastLandedMove {
    battle.battler_state(BattlerSide::Enemy).last_landed_move()
}

#[test]
fn a_male_attractor_infatuates_a_female_target_and_spends_pp() {
    let (battle, events) = attract_turn(
        mon(MUDKIP, MALE, &[ATTRACT]),
        mon(RATTATA, FEMALE, &[SPLASH]),
        ACCURACY_HITS,
    );

    assert!(events.contains(&infatuated_event()), "{events:?}");
    assert_eq!(
        battle.enemy().volatiles().infatuated_by,
        Some(BattlerSide::Player)
    );
    assert_eq!(battle.player().moves()[0].pp, ATTRACT_PP - 1);
    assert_eq!(enemy_landed(&battle), LastLandedMove::Move(ATTRACT));
}

#[test]
fn a_female_attractor_infatuates_a_male_target() {
    let (battle, events) = attract_turn(
        mon(MUDKIP, FEMALE, &[ATTRACT]),
        mon(RATTATA, MALE, &[SPLASH]),
        ACCURACY_HITS,
    );

    assert!(events.contains(&infatuated_event()), "{events:?}");
    assert_eq!(
        battle.enemy().volatiles().infatuated_by,
        Some(BattlerSide::Player)
    );
}

#[test]
fn the_same_gender_fails_after_spending_pp_and_the_accuracy_draw() {
    for gender in [MALE, FEMALE] {
        let (battle, events) = attract_turn(
            mon(MUDKIP, gender, &[ATTRACT]),
            mon(RATTATA, gender, &[SPLASH]),
            0,
        );

        assert!(events.contains(&failed_event()), "{events:?}");
        assert!(!battle.enemy().volatiles().infatuated());
        assert_eq!(battle.player().moves()[0].pp, ATTRACT_PP - 1);
        assert_eq!(enemy_landed(&battle), LastLandedMove::Unavailable);
    }
}

#[test]
fn a_genderless_attractor_or_target_fails() {
    for (attacker, target) in [
        (
            mon(BELDUM, MALE, &[ATTRACT]),
            mon(RATTATA, FEMALE, &[SPLASH]),
        ),
        (
            mon(MUDKIP, MALE, &[ATTRACT]),
            mon(BELDUM, FEMALE, &[SPLASH]),
        ),
    ] {
        let (battle, events) = attract_turn(attacker, target, 0);

        assert!(events.contains(&failed_event()), "{events:?}");
        assert!(!battle.enemy().volatiles().infatuated());
    }
}

#[test]
fn an_already_infatuated_target_fails_and_keeps_its_original_attractor() {
    let target = infatuated_by(mon(RATTATA, FEMALE, &[SPLASH]), BattlerSide::Enemy);
    let (battle, events, rng) = wild_turn(
        mon(MUDKIP, MALE, &[ATTRACT]),
        target,
        // The infatuated target acts first and spends its own coin.
        &[&PREFIX, &[LOVE_LETS_MOVE_THROUGH], &[0]],
    );

    rng.assert_exhausted();
    assert!(events.contains(&failed_event()), "{events:?}");
    assert_eq!(
        battle.enemy().volatiles().infatuated_by,
        Some(BattlerSide::Enemy)
    );
}

#[test]
fn oblivious_prevents_the_attraction_with_its_own_message() {
    let (battle, events) = attract_turn(
        mon(MUDKIP, MALE, &[ATTRACT]),
        mon(ILLUMISE, FEMALE, &[SPLASH]),
        0,
    );

    assert!(
        events.contains(&BattleEvent::ObliviousPreventsAttraction {
            by_player: true,
            move_id: ATTRACT,
        }),
        "{events:?}"
    );
    assert!(!battle.enemy().volatiles().infatuated());
    assert_eq!(battle.player().moves()[0].pp, ATTRACT_PP - 1);
    assert_eq!(
        enemy_landed(&battle),
        LastLandedMove::Move(ATTRACT),
        "the Oblivious script sets no no-effect result"
    );
}

#[test]
fn oblivious_outranks_the_gender_failure() {
    let (_, events) = attract_turn(
        mon(MUDKIP, FEMALE, &[ATTRACT]),
        mon(ILLUMISE, FEMALE, &[SPLASH]),
        0,
    );

    assert!(
        events
            .iter()
            .any(|event| matches!(event, BattleEvent::ObliviousPreventsAttraction { .. })),
        "{events:?}"
    );
    assert!(!events.contains(&failed_event()), "{events:?}");
}

fn blind_attractor(target: BattlePokemon, accuracy_roll: u16) -> (Battle, Vec<BattleEvent>) {
    let mut attacker = mon(MUDKIP, MALE, &[ATTRACT]);
    attacker.stages_mut().accuracy = StatStage::new(-1).unwrap();
    attract_turn(attacker, target, accuracy_roll)
}

#[test]
fn a_minus_one_accuracy_stage_connects_on_a_raw_74_and_misses_on_75() {
    // Accuracy 100 at -1 is a threshold of 75; rolls are `Random() % 100 + 1`.
    let (hit, events) = blind_attractor(mon(RATTATA, FEMALE, &[SPLASH]), 74);
    assert!(events.contains(&infatuated_event()), "{events:?}");
    assert!(hit.enemy().volatiles().infatuated());

    let (miss, events) = blind_attractor(mon(RATTATA, FEMALE, &[SPLASH]), 75);
    assert!(
        events.contains(&BattleEvent::Missed {
            by_player: true,
            move_id: ATTRACT,
        }),
        "{events:?}"
    );
    assert!(!miss.enemy().volatiles().infatuated());
    assert_eq!(enemy_landed(&miss), LastLandedMove::Unavailable);
}

#[test]
fn a_miss_outranks_oblivious() {
    let (_, events) = blind_attractor(mon(ILLUMISE, FEMALE, &[SPLASH]), 75);

    assert!(
        events.contains(&BattleEvent::Missed {
            by_player: true,
            move_id: ATTRACT,
        }),
        "{events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, BattleEvent::ObliviousPreventsAttraction { .. })),
        "{events:?}"
    );
}
