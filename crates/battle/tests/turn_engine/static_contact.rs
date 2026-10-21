//! Static's contact paralysis (`ABILITYEFFECT_MOVE_END`, `battle_util.c:2821`).

use crate::poison_support::*;
use assets::{AbilityId, MoveId};
use battle::{Battle, BattleEvent, BattlePokemon, Dex, PlayerAction, Status1};

/// `SPECIES_PIKACHU`: Static, and faster than [`RATTATA`] at equal level.
const PIKACHU: u16 = 25;
const WATER_GUN: MoveId = MoveId::WATER_GUN;

const CONSTRUCTION_TURN_NUMBER: u16 = 0;
const TURN_START: u16 = 0;
const ENEMY_MOVE_SELECTION: u16 = 0;
const ACCURACY_HITS: u16 = 0;
const NO_CRIT: u16 = 1;
const DAMAGE_ROLL: u16 = 0;
const DISCARDED_SECONDARY_EFFECT: u16 = 0;

/// Static paralyses on a zero `Random() % 3` roll.
const STATIC_TRIGGERS: u16 = 0;
const STATIC_MISSES: u16 = 1;

const PLAYER_ACTS_ALONE: [u16; 7] = [
    CONSTRUCTION_TURN_NUMBER,
    TURN_START,
    ENEMY_MOVE_SELECTION,
    ACCURACY_HITS,
    NO_CRIT,
    DAMAGE_ROLL,
    DISCARDED_SECONDARY_EFFECT,
];

const BOTH_ACT: [u16; 11] = [
    CONSTRUCTION_TURN_NUMBER,
    TURN_START,
    ENEMY_MOVE_SELECTION,
    ACCURACY_HITS,
    NO_CRIT,
    DAMAGE_ROLL,
    DISCARDED_SECONDARY_EFFECT,
    ACCURACY_HITS,
    NO_CRIT,
    DAMAGE_ROLL,
    DISCARDED_SECONDARY_EFFECT,
];

fn pikachu(dex: &Dex, level: u8, moves: Vec<MoveId>) -> BattlePokemon {
    let mon = max_iv_mon(dex, PIKACHU, level, moves);
    assert_eq!(mon.ability(), AbilityId::STATIC);
    mon
}

fn script_with_roll(turn: &[u16], roll: u16) -> SequenceRng {
    let mut draws = turn.to_vec();
    draws.push(roll);
    SequenceRng::new(draws)
}

fn static_events(events: &[BattleEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, BattleEvent::ParalyzedByStatic { .. }))
        .count()
}

#[test]
fn a_contact_hit_into_an_enemy_static_holder_paralyses_the_player_on_zero() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]);
    let enemy = pikachu(&dex, 5, vec![TACKLE]);

    let mut rng = script_with_roll(&BOTH_ACT, STATIC_TRIGGERS);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    rng.assert_exhausted();

    assert_eq!(battle.player().status1(), Status1::Paralysed, "{events:?}");
    assert_eq!(battle.enemy().status1(), Status1::Healthy);
    assert_eq!(
        events.last(),
        Some(&BattleEvent::ParalyzedByStatic {
            by_player: true,
            move_id: TACKLE,
        }),
        "{events:?}"
    );
}

#[test]
fn a_contact_hit_into_a_player_static_holder_paralyses_the_enemy_on_zero() {
    let dex = Dex::new();
    let player = pikachu(&dex, 5, vec![TACKLE]);
    let enemy = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]);

    let mut rng = script_with_roll(&BOTH_ACT, STATIC_TRIGGERS);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    rng.assert_exhausted();

    assert_eq!(battle.enemy().status1(), Status1::Paralysed, "{events:?}");
    assert_eq!(battle.player().status1(), Status1::Healthy);
    assert_eq!(
        events.last(),
        Some(&BattleEvent::ParalyzedByStatic {
            by_player: false,
            move_id: TACKLE,
        }),
        "{events:?}"
    );
}

#[test]
fn a_failed_static_roll_consumes_its_draw_and_leaves_the_attacker_healthy() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]);
    let enemy = pikachu(&dex, 5, vec![TACKLE]);

    let mut rng = script_with_roll(&BOTH_ACT, STATIC_MISSES);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    rng.assert_exhausted();

    assert_eq!(battle.player().status1(), Status1::Healthy, "{events:?}");
    assert_eq!(static_events(&events), 0, "{events:?}");
}

#[test]
fn a_non_contact_hit_into_a_static_holder_draws_no_roll() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 5, vec![WATER_GUN]);
    let enemy = pikachu(&dex, 5, vec![TACKLE]);

    let mut rng = SequenceRng::new(BOTH_ACT);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    rng.assert_exhausted();

    assert_eq!(battle.player().status1(), Status1::Healthy, "{events:?}");
    assert_eq!(static_events(&events), 0, "{events:?}");
}

#[test]
fn a_fainted_static_holder_still_rolls_and_paralyses_the_attacker() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 50, vec![TACKLE]);
    let enemy = pikachu(&dex, 5, vec![TACKLE]);

    let mut rng = script_with_roll(&PLAYER_ACTS_ALONE, STATIC_TRIGGERS);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    rng.assert_exhausted();

    assert!(battle.enemy().is_fainted(), "{events:?}");
    assert_eq!(battle.player().status1(), Status1::Paralysed, "{events:?}");
}

#[test]
fn a_multi_hit_contact_move_rolls_static_after_every_hit() {
    const HIT_COUNT_TWO: u16 = 0;
    const FIRST_HIT_STATIC_ROLL: u16 = STATIC_MISSES;
    const SECOND_HIT_STATIC_ROLL: u16 = STATIC_TRIGGERS;
    const DOUBLE_SLAP_EFFECT_CHANCE: u16 = 0;
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 60, vec![MoveId::DOUBLE_SLAP]);
    let enemy = pikachu(&dex, 40, vec![TACKLE]);

    let mut rng = SequenceRng::new([
        CONSTRUCTION_TURN_NUMBER,
        TURN_START,
        ENEMY_MOVE_SELECTION,
        ACCURACY_HITS,
        HIT_COUNT_TWO,
        NO_CRIT,
        DAMAGE_ROLL,
        FIRST_HIT_STATIC_ROLL,
        NO_CRIT,
        DAMAGE_ROLL,
        SECOND_HIT_STATIC_ROLL,
        DOUBLE_SLAP_EFFECT_CHANCE,
        ACCURACY_HITS,
        NO_CRIT,
        DAMAGE_ROLL,
        DISCARDED_SECONDARY_EFFECT,
    ]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    rng.assert_exhausted();

    assert_eq!(battle.player().status1(), Status1::Paralysed, "{events:?}");
    assert_eq!(static_events(&events), 1, "{events:?}");
}

#[test]
fn a_synchronize_attacker_reflects_static_paralysis_onto_the_surviving_holder() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RALTS, 10, vec![TACKLE]);
    assert_eq!(player.ability(), AbilityId::SYNCHRONIZE);
    let enemy = pikachu(&dex, 10, vec![TACKLE]);

    let mut rng = script_with_roll(&BOTH_ACT, STATIC_TRIGGERS);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    rng.assert_exhausted();

    assert_eq!(battle.player().status1(), Status1::Paralysed, "{events:?}");
    assert_eq!(battle.enemy().status1(), Status1::Paralysed, "{events:?}");
    assert_eq!(
        events.last(),
        Some(&BattleEvent::ParalyzedByAttackerSynchronize {
            by_player: true,
            move_id: TACKLE,
        }),
        "{events:?}"
    );
}
