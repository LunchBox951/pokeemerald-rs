//! Synchronize's poison reflection at move end, and the guards that make
//! the original attacker immune to its own reflected poison.

use crate::poison_support::*;
use assets::AbilityId;
use battle::{Battle, BattleEvent, Dex, PlayerAction, Status1};

/// `SPECIES_ZANGOOSE`: Immunity in its only ability slot.
const ZANGOOSE: u16 = 380;

#[test]
fn poison_sting_against_a_synchronize_target_reflects_poison_at_move_end() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 5, vec![POISON_STING]);
    let enemy = max_iv_mon(&dex, RALTS, 5, vec![TACKLE]);
    assert_eq!(
        enemy.ability(),
        AbilityId::SYNCHRONIZE,
        "fixture sanity: the primary slot fields Synchronize"
    );

    let mut rng = SequenceRng::new(POISON_STING_LANDS);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .expect("Synchronize is admitted: the reflection is modelled");

    assert!(
        matches!(
            events[0],
            BattleEvent::Hit {
                by_player: true,
                move_id: POISON_STING,
                ..
            }
        ),
        "{events:?}"
    );
    assert_eq!(
        events[1],
        BattleEvent::Poisoned {
            by_player: true,
            move_id: POISON_STING,
        },
        "{events:?}"
    );
    assert_eq!(
        events[2],
        BattleEvent::PoisonedBySynchronize {
            by_player: true,
            move_id: POISON_STING,
        },
        "the target's own Hit and Poisoned events precede \
         MOVEEND_SYNCHRONIZE_TARGET's reflection, which precedes the rest of \
         move-end processing: {events:?}"
    );
    assert!(
        matches!(
            events[3],
            BattleEvent::Hit {
                by_player: false,
                move_id: TACKLE,
                ..
            }
        ),
        "the reflection precedes the rest of move-end processing, so the \
         enemy's own turn still follows: {events:?}"
    );
    assert_eq!(
        events.len(),
        6,
        "both battlers are now poisoned, so each takes its own end-of-turn \
         residual tick after the exchange: {events:?}"
    );
    assert_eq!(battle.enemy().status1(), Status1::Poisoned);
    assert_eq!(
        battle.player().status1(),
        Status1::Poisoned,
        "Synchronize passes the poison back to the battler that inflicted it"
    );
    assert_eq!(rng.draws(), 11, "the reflection consumes no RNG of its own");
}

#[test]
fn an_immunity_original_attacker_blocks_its_own_reflected_poison() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, ZANGOOSE, 5, vec![POISON_STING]);
    assert_eq!(player.ability(), AbilityId::IMMUNITY);
    let enemy = max_iv_mon(&dex, RALTS, 5, vec![TACKLE]);

    // Ralts is not immune to Poison Sting, so the hit lands and reflects;
    // Zangoose's Immunity blocks only the reflection, leaving the draws as in
    // the reflection test.
    let mut rng = SequenceRng::new(POISON_STING_LANDS);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        events.contains(&BattleEvent::SynchronizeImmunityProtected {
            by_player: true,
            move_id: POISON_STING,
        }),
        "{events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, BattleEvent::PoisonedBySynchronize { .. })),
        "{events:?}"
    );
    assert_eq!(battle.enemy().status1(), Status1::Poisoned);
    assert_eq!(
        battle.player().status1(),
        Status1::Healthy,
        "Immunity protects the original attacker from the reflection"
    );
}

#[test]
fn a_poison_typed_original_attacker_blocks_its_own_reflected_poison() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, EKANS, 5, vec![POISON_STING]);
    let enemy = max_iv_mon(&dex, RALTS, 5, vec![TACKLE]);

    // Unlike the initial hit's silent guard against a Poison-type target,
    // the reflection reports Ekans' Poison typing as an event.
    let mut rng = SequenceRng::new(POISON_STING_LANDS);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        events.contains(&BattleEvent::SynchronizePoisonOrSteelTypeProtected {
            by_player: true,
            move_id: POISON_STING,
        }),
        "{events:?}"
    );
    assert_eq!(battle.enemy().status1(), Status1::Poisoned);
    assert_eq!(
        battle.player().status1(),
        Status1::Healthy,
        "Ekans' own Poison typing protects it from the reflection"
    );
}

/// `ppreduce` (`data/battle_scripts_1.s:247`) runs ahead of the reflection,
/// so an original attacker that already carries a primary status leaves the
/// reflection nothing to write (`battle_script_commands.c:2334`-`:2335`).
#[test]
fn an_already_statused_original_attacker_reflects_nothing_new() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, RATTATA, 5, vec![POISON_STING]);
    player.set_status1(Status1::Poisoned);
    let enemy = max_iv_mon(&dex, RALTS, 5, vec![TACKLE]);

    let mut rng = SequenceRng::new(POISON_STING_LANDS);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        !events
            .iter()
            .any(|event| matches!(event, BattleEvent::PoisonedBySynchronize { .. })),
        "an already-statused original attacker's reflection writes nothing: {events:?}"
    );
    assert_eq!(battle.enemy().status1(), Status1::Poisoned);
    assert_eq!(
        battle.player().status1(),
        Status1::Poisoned,
        "the attacker's own pre-existing status is never rewritten by the reflection"
    );
}
