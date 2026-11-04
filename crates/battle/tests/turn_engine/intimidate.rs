//! Intimidate's queued entry Attack drop (`ABILITYEFFECT_INTIMIDATE1/2`,
//! `pokeemerald/src/battle_util.c:3003`-`:3016`, `:3074`-`:3088`;
//! `BattleScript_IntimidateActivates`, `data/battle_scripts_1.s:4024`-`:4048`)
//! through the public API. Forced switches the AI never chooses run in the
//! crate's private `intimidate_tests`.
//!
//! Every script is exact: [`SequenceRng`] panics on an extra draw and
//! `assert_exhausted` fails on a missing one. Intimidate itself draws nothing.

use crate::common::{max_iv_mon, script, SequenceRng};
use assets::trainers::TrainerId;
use assets::{AbilityId, MoveId, SpeciesId};
use battle::{Battle, BattleEvent, BattlePokemon, Dex, MoveLearnDecision, PlayerAction, StatStage};

const MAY_ROUTE_103_MUDKIP: TrainerId = TrainerId(529);
const SPLASH: MoveId = MoveId::SPLASH;
const TACKLE: MoveId = MoveId::TACKLE;

/// The constructor's turn-number seed; no Speed tie, so no order draw.
const C: [u16; 1] = [0];
/// A public turn's seed draw plus the wild enemy's one-slot move choice.
const W: [u16; 2] = [0, 0];
/// Passing accuracy, non-critical, damage roll, discarded effect chance.
const H: [u16; 4] = [0, 1, 0, 0];

fn splashing(dex: &Dex, species: SpeciesId, level: u8, slot: u8) -> BattlePokemon {
    max_iv_mon(dex, species.0, level, vec![SPLASH]).with_ability_slot(slot)
}

fn dropped(by_player: bool) -> BattleEvent {
    BattleEvent::Intimidated {
        by_player,
        new_stage: StatStage::new(-1).unwrap(),
    }
}

fn is_intimidate_event(event: &BattleEvent) -> bool {
    matches!(
        event,
        BattleEvent::Intimidated { .. } | BattleEvent::IntimidatePrevented { .. }
    )
}

fn attack(mon: &BattlePokemon) -> i8 {
    mon.stages().attack.offset()
}

#[test]
fn initial_holders_resolve_in_battler_order_not_speed_order() {
    let dex = Dex::new();
    let player = splashing(&dex, SpeciesId::MAWILE, 20, 1);
    let enemy = splashing(&dex, SpeciesId::MIGHTYENA, 20, 0);
    assert_eq!(player.ability(), AbilityId::INTIMIDATE);
    assert!(
        player.speed_for_turn_order() < enemy.speed_for_turn_order(),
        "the player is slower, so Speed order would put the enemy first"
    );
    let mut rng = SequenceRng::new(script(&[&C, &W, &W]));
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    assert_eq!(rng.draws(), 1, "construction spends no draw on Intimidate");
    assert_eq!(attack(battle.player()), -1);
    assert_eq!(attack(battle.enemy()), -1);

    let first = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(first.len(), 6, "{first:?}");
    assert_eq!(first[..2], [dropped(true), dropped(false)]);

    let second = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(!second.iter().any(is_intimidate_event), "{second:?}");
    assert_eq!(attack(battle.player()), -1);
    assert_eq!(attack(battle.enemy()), -1);
    rng.assert_exhausted();
}

#[test]
fn clear_body_white_smoke_and_hyper_cutter_prevent_the_drop_on_both_sides() {
    for (species, ability) in [
        (SpeciesId::BELDUM, AbilityId::CLEAR_BODY),
        (SpeciesId::TORKOAL, AbilityId::WHITE_SMOKE),
        (SpeciesId::MAWILE, AbilityId::HYPER_CUTTER),
    ] {
        for holder_is_player in [true, false] {
            let dex = Dex::new();
            let holder = splashing(&dex, SpeciesId::MIGHTYENA, 20, 0);
            let target = splashing(&dex, species, 20, 0);
            assert_eq!(target.ability(), ability);
            assert!(target.speed_for_turn_order() < holder.speed_for_turn_order());
            let (player, enemy) = if holder_is_player {
                (holder, target)
            } else {
                (target, holder)
            };
            let mut rng = SequenceRng::new(script(&[&C, &W]));
            let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();

            let events = battle
                .take_turn(PlayerAction::UseMove(0), &mut rng)
                .unwrap();

            assert_eq!(
                events[0],
                BattleEvent::IntimidatePrevented {
                    by_player: holder_is_player,
                    ability
                }
            );
            assert_eq!(events.iter().filter(|e| is_intimidate_event(e)).count(), 1);
            assert_eq!(attack(battle.player()), 0);
            assert_eq!(attack(battle.enemy()), 0);
            rng.assert_exhausted();
        }
    }
}

#[test]
fn keen_eye_does_not_prevent_the_attack_drop() {
    let dex = Dex::new();
    let player = splashing(&dex, SpeciesId::PIDGEY, 20, 0);
    let enemy = splashing(&dex, SpeciesId::MIGHTYENA, 20, 0);
    assert_eq!(player.ability(), AbilityId::KEEN_EYE);
    let mut rng = SequenceRng::new(script(&[&C, &W]));
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(events[0], dropped(false));
    assert_eq!(attack(battle.player()), -1);
    assert_eq!(attack(battle.enemy()), 0);
    rng.assert_exhausted();
}

#[test]
fn a_benched_holder_waits_then_drops_the_survivors_attack_once() {
    let dex = Dex::new();
    let mut lead = splashing(&dex, SpeciesId::MAGIKARP, 1, 0);
    lead.apply_damage(lead.stats().max_hp - 1);
    let reserve = splashing(&dex, SpeciesId::MIGHTYENA, 50, 0);
    let enemy = max_iv_mon(&dex, SpeciesId::MEOWTH.0, 20, vec![TACKLE]);
    let mut rng = SequenceRng::new(script(&[&C, &W, &H, &W, &H]));
    let mut battle =
        Battle::new_with_player_reserves(dex, lead, vec![reserve], enemy, false, &mut rng).unwrap();
    assert_eq!(attack(battle.enemy()), 0, "a benched holder is silent");

    let knockout = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(rng.draws(), 7);
    assert_eq!(
        knockout[knockout.len() - 3..],
        [
            BattleEvent::Fainted { by_player: true },
            BattleEvent::PlayerSentOut {
                species: SpeciesId::MIGHTYENA,
                reserves_remaining: 0
            },
            dropped(true),
        ]
    );
    assert_eq!(attack(battle.enemy()), -1);

    let next = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(!next.iter().any(is_intimidate_event), "{next:?}");
    assert_eq!(attack(battle.enemy()), -1);
    rng.assert_exhausted();
}

#[test]
fn the_initial_speed_tie_draws_are_unchanged_and_entry_follows_battler_order() {
    let dex = Dex::new();
    let player = splashing(&dex, SpeciesId::MIGHTYENA, 20, 0);
    let enemy = splashing(&dex, SpeciesId::MIGHTYENA, 20, 0);
    assert_eq!(player.speed_for_turn_order(), enemy.speed_for_turn_order());
    let mut rng = SequenceRng::new([0x1234, 1, 0x2222, 0, 1, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    assert_eq!(rng.draws(), 2, "the turn seed and the discarded tie draw");

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(events[..2], [dropped(true), dropped(false)]);
    rng.assert_exhausted();
}

#[test]
fn a_rejected_first_action_keeps_the_opening_events_for_the_next_valid_turn() {
    let dex = Dex::new();
    let player = splashing(&dex, SpeciesId::MAWILE, 20, 1);
    let enemy = splashing(&dex, SpeciesId::MIGHTYENA, 20, 0);
    let mut rng = SequenceRng::new(script(&[&C, &W]));
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();

    let rejected = battle
        .take_turn(PlayerAction::UseMove(3), &mut rng)
        .unwrap_err();
    assert!(rejected.events().is_empty());
    assert_eq!(rng.draws(), 1);

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(events[..2], [dropped(true), dropped(false)]);
    rng.assert_exhausted();
}

#[test]
fn a_replacement_deferred_by_a_move_prompt_intimidates_when_the_answer_arrives() {
    let dex = Dex::new();
    let growth_rate = dex.species(SpeciesId::TORCHIC).unwrap().growth_rate;
    let level_16 = assets::experience_for_level(growth_rate, 16).unwrap();
    let mut player = max_iv_mon(
        &dex,
        SpeciesId::TORCHIC.0,
        15,
        vec![MoveId::SCRATCH, MoveId::GROWL, MoveId::TACKLE, MoveId::LEER],
    );
    assert!(player
        .apply_experience(&dex, level_16 - 1 - player.experience())
        .unwrap()
        .is_none());
    let party = vec![
        max_iv_mon(
            &dex,
            SpeciesId::TREECKO.0,
            5,
            vec![MoveId::POUND, MoveId::LEER],
        ),
        splashing(&dex, SpeciesId::MIGHTYENA, 20, 0),
    ];
    let mut rng = SequenceRng::new([u16::MAX; 128]);
    let mut battle =
        Battle::new_trainer(dex, player, MAY_ROUTE_103_MUDKIP, party, &mut rng).unwrap();
    assert_eq!(attack(battle.player()), 0, "the bench is silent");

    for _ in 0..8 {
        battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();
        if battle.pending_move_learn().is_some() {
            break;
        }
    }
    assert!(battle.pending_move_learn().is_some());
    assert_eq!(
        attack(battle.player()),
        0,
        "the replacement and its drop wait on the answer"
    );

    let answered = battle
        .resolve_move_learn(MoveLearnDecision::Decline, &mut rng)
        .unwrap();

    assert_eq!(
        answered[1..],
        [
            BattleEvent::TrainerSentOut {
                species: SpeciesId::MIGHTYENA,
                bench_remaining: 0
            },
            dropped(false),
        ]
    );
    assert_eq!(attack(battle.player()), -1);
}
