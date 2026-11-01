//! Poison Point's contact poison (`ABILITYEFFECT_ON_DAMAGE`, `battle_util.c:2806`).

use crate::poison_support::*;
use assets::{AbilityId, MoveId, SpeciesId};
use battle::{Battle, BattleEvent, BattlePokemon, Dex, PlayerAction, Status1};

/// Poison Point holder, slower than [`RATTATA`] at equal level.
const NIDORAN_F: SpeciesId = SpeciesId::NIDORAN_F;
const WATER_GUN: MoveId = MoveId::WATER_GUN;

const CONSTRUCTION_TURN_NUMBER: u16 = 0;
const TURN_START: u16 = 0;
const ENEMY_MOVE_SELECTION: u16 = 0;
const ACCURACY_HITS: u16 = 0;
const NO_CRIT: u16 = 1;
const DAMAGE_ROLL: u16 = 0;
const DISCARDED_SECONDARY_EFFECT: u16 = 0;

const FASTER_HITS: [u16; 7] = [
    CONSTRUCTION_TURN_NUMBER,
    TURN_START,
    ENEMY_MOVE_SELECTION,
    ACCURACY_HITS,
    NO_CRIT,
    DAMAGE_ROLL,
    DISCARDED_SECONDARY_EFFECT,
];

const SLOWER_HITS: [u16; 4] = [
    ACCURACY_HITS,
    NO_CRIT,
    DAMAGE_ROLL,
    DISCARDED_SECONDARY_EFFECT,
];

/// The Poison Point roll is `Random() % 3`: zero poisons.
const POISON_POINT_TRIGGERS: u16 = 0;
const POISON_POINT_MISSES: u16 = 1;

fn nidoran(dex: &Dex, level: u8) -> BattlePokemon {
    let mon = max_iv_mon(dex, NIDORAN_F, level, vec![TACKLE]);
    assert_eq!(mon.ability(), AbilityId::POISON_POINT);
    mon
}

fn script(prefix: &[u16], roll: u16, suffix: &[u16]) -> SequenceRng {
    let mut draws = prefix.to_vec();
    draws.push(roll);
    draws.extend_from_slice(suffix);
    SequenceRng::new(draws)
}

fn poison_point_events(events: &[BattleEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, BattleEvent::PoisonedByPoisonPoint { .. }))
        .count()
}

#[test]
fn a_contact_hit_into_an_enemy_poison_point_holder_poisons_the_player_on_zero() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]);
    let enemy = nidoran(&dex, 5);

    let mut rng = script(&FASTER_HITS, POISON_POINT_TRIGGERS, &SLOWER_HITS);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    rng.assert_exhausted();

    assert_eq!(battle.player().status1(), Status1::Poisoned, "{events:?}");
    assert_eq!(battle.enemy().status1(), Status1::Healthy);
    assert!(
        events.contains(&BattleEvent::PoisonedByPoisonPoint {
            by_player: true,
            move_id: TACKLE,
        }),
        "{events:?}"
    );
}

#[test]
fn a_contact_hit_into_a_player_poison_point_holder_poisons_the_enemy_on_zero() {
    let dex = Dex::new();
    let player = nidoran(&dex, 5);
    let enemy = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]);

    let mut rng = script(&FASTER_HITS, POISON_POINT_TRIGGERS, &SLOWER_HITS);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    rng.assert_exhausted();

    assert_eq!(battle.enemy().status1(), Status1::Poisoned, "{events:?}");
    assert_eq!(battle.player().status1(), Status1::Healthy);
    assert!(
        events.contains(&BattleEvent::PoisonedByPoisonPoint {
            by_player: false,
            move_id: TACKLE,
        }),
        "{events:?}"
    );
}

#[test]
fn a_failed_poison_point_roll_consumes_its_draw_and_leaves_the_attacker_healthy() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]);
    let enemy = nidoran(&dex, 5);

    let mut rng = script(&FASTER_HITS, POISON_POINT_MISSES, &SLOWER_HITS);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    rng.assert_exhausted();

    assert_eq!(battle.player().status1(), Status1::Healthy, "{events:?}");
    assert_eq!(poison_point_events(&events), 0, "{events:?}");
}

#[test]
fn a_non_contact_hit_into_a_poison_point_holder_draws_no_roll() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 5, vec![WATER_GUN]);
    let enemy = nidoran(&dex, 5);

    let mut draws = FASTER_HITS.to_vec();
    draws.extend_from_slice(&SLOWER_HITS);
    let mut rng = SequenceRng::new(draws);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    rng.assert_exhausted();

    assert_eq!(battle.player().status1(), Status1::Healthy, "{events:?}");
    assert_eq!(poison_point_events(&events), 0, "{events:?}");
}

#[test]
fn a_fainted_poison_point_holder_still_rolls_and_poisons_the_attacker() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 50, vec![TACKLE]);
    let enemy = nidoran(&dex, 5);

    let mut rng = script(&FASTER_HITS, POISON_POINT_TRIGGERS, &[]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    rng.assert_exhausted();

    assert!(battle.enemy().is_fainted(), "{events:?}");
    assert_eq!(battle.player().status1(), Status1::Poisoned, "{events:?}");
}

#[test]
fn a_poison_typed_attacker_is_not_poisoned_but_still_spends_the_roll() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, SpeciesId::ZUBAT, 5, vec![TACKLE]);
    let enemy = nidoran(&dex, 5);

    let mut rng = script(&FASTER_HITS, POISON_POINT_TRIGGERS, &SLOWER_HITS);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    rng.assert_exhausted();

    assert_eq!(battle.player().status1(), Status1::Healthy, "{events:?}");
    assert_eq!(poison_point_events(&events), 0, "{events:?}");
}

#[test]
fn an_immunity_attacker_is_not_poisoned_but_still_spends_the_roll() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, SpeciesId::ZANGOOSE, 5, vec![TACKLE]);
    assert_eq!(player.ability(), AbilityId::IMMUNITY);
    let enemy = nidoran(&dex, 5);

    let mut rng = script(&FASTER_HITS, POISON_POINT_TRIGGERS, &SLOWER_HITS);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    rng.assert_exhausted();

    assert_eq!(battle.player().status1(), Status1::Healthy, "{events:?}");
    assert_eq!(poison_point_events(&events), 0, "{events:?}");
}

#[test]
fn an_already_poisoned_attacker_spends_the_roll_without_a_second_poison() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]);
    player.set_status1(Status1::Poisoned);
    let enemy = nidoran(&dex, 5);

    let mut rng = script(&FASTER_HITS, POISON_POINT_TRIGGERS, &SLOWER_HITS);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    rng.assert_exhausted();

    assert_eq!(poison_point_events(&events), 0, "{events:?}");
}

#[test]
fn a_multi_hit_contact_move_rolls_poison_point_after_every_hit() {
    const HIT_COUNT_TWO: u16 = 0;
    const FIRST_HIT_POISON_POINT_ROLL: u16 = POISON_POINT_MISSES;
    const SECOND_HIT_POISON_POINT_ROLL: u16 = POISON_POINT_TRIGGERS;
    const DOUBLE_SLAP_EFFECT_CHANCE: u16 = 0;
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 60, vec![MoveId::DOUBLE_SLAP]);
    let enemy = nidoran(&dex, 40);

    let mut rng = SequenceRng::new([
        CONSTRUCTION_TURN_NUMBER,
        TURN_START,
        ENEMY_MOVE_SELECTION,
        ACCURACY_HITS,
        HIT_COUNT_TWO,
        NO_CRIT,
        DAMAGE_ROLL,
        FIRST_HIT_POISON_POINT_ROLL,
        NO_CRIT,
        DAMAGE_ROLL,
        SECOND_HIT_POISON_POINT_ROLL,
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

    assert_eq!(battle.player().status1(), Status1::Poisoned, "{events:?}");
    assert_eq!(poison_point_events(&events), 1, "{events:?}");
}

#[test]
fn a_synchronize_attacker_is_blocked_by_the_poison_typed_holder_when_reflecting() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RALTS, 14, vec![TACKLE]);
    assert_eq!(player.ability(), AbilityId::SYNCHRONIZE);
    let enemy = nidoran(&dex, 10);

    let mut rng = script(&FASTER_HITS, POISON_POINT_TRIGGERS, &SLOWER_HITS);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    rng.assert_exhausted();

    assert_eq!(battle.player().status1(), Status1::Poisoned, "{events:?}");
    assert_eq!(battle.enemy().status1(), Status1::Healthy, "{events:?}");
    assert!(
        events.contains(
            &BattleEvent::AttackerSynchronizePoisonOrSteelTypeProtected {
                by_player: true,
                move_id: TACKLE,
            }
        ),
        "{events:?}"
    );
}

#[test]
fn a_synchronize_attacker_reflects_poison_point_poison_onto_a_non_poison_holder() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RALTS, 14, vec![TACKLE]);
    assert_eq!(player.ability(), AbilityId::SYNCHRONIZE);
    // Poison Point holder that is not Poison-typed.
    let enemy = max_iv_mon(&dex, SpeciesId::SEADRA, 5, vec![TACKLE]);
    assert_eq!(enemy.ability(), AbilityId::POISON_POINT);

    let mut rng = script(&FASTER_HITS, POISON_POINT_TRIGGERS, &SLOWER_HITS);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    rng.assert_exhausted();

    assert_eq!(battle.player().status1(), Status1::Poisoned, "{events:?}");
    assert_eq!(battle.enemy().status1(), Status1::Poisoned, "{events:?}");
    assert!(
        events.contains(&BattleEvent::PoisonedByAttackerSynchronize {
            by_player: true,
            move_id: TACKLE,
        }),
        "{events:?}"
    );
}
