//! Cases where Cute Charm draws no roll, and the guards that follow the roll
//! (`battle_util.c:2859`-`:2864`) and so still spend it.

use super::*;
use assets::AbilityId;

fn assert_untouched(battle: &Battle, events: &[BattleEvent]) {
    assert!(!battle.player().volatiles().infatuated(), "{events:?}");
    assert!(cute_charm_events(events).is_empty(), "{events:?}");
}

#[test]
fn a_non_contact_hit_draws_no_roll() {
    let (battle, events) = attacker_hits_enemy_holder(
        rattata_attacker(MALE, &[WATER_GUN]),
        skitty_holder(FEMALE),
        &[],
    );

    assert_untouched(&battle, &events);
}

/// `MOVE_RESULT_NO_EFFECT` (`battle_util.c:2853`): Shadow Punch is a Ghost-type
/// contact move and the Normal-type holder is immune to it.
#[test]
fn a_no_effect_contact_hit_draws_no_roll() {
    let (battle, events) = player_turn(
        rattata_attacker(MALE, &[MoveId::SHADOW_PUNCH]),
        skitty_holder(FEMALE),
        // Accuracy, critical-hit and damage draws; the immunity ends the move
        // before any secondary-effect or contact draw.
        &script(&[&SETUP, &[0, 1, 0]]),
    );

    assert!(
        events
            .iter()
            .any(|event| matches!(event, BattleEvent::NoEffect { .. })),
        "{events:?}"
    );
    assert_untouched(&battle, &events);
}

/// The confusion self-hit (`gProtectStructs.confusionSelfDmg`,
/// `battle_util.c:2855`) never reaches the contact seam, so it draws no roll.
#[test]
fn a_confusion_self_hit_draws_no_roll() {
    let mut attacker = rattata_attacker(MALE, &[TACKLE]);
    attacker.volatiles_mut().set_confusion(2);
    let (battle, events) = player_turn(
        attacker,
        skitty_holder(FEMALE),
        // The confusion coin (residue 0 -> self-hit) and the self-hit damage
        // roll, and nothing after.
        &script(&[&SETUP, &[0, 0]]),
    );

    assert!(
        events.iter().any(|event| matches!(
            event,
            BattleEvent::ConfusionSelfHit {
                by_player: true,
                ..
            }
        )),
        "{events:?}"
    );
    assert_untouched(&battle, &events);
}

/// The holder must still be alive (`battle_util.c:2858`), unlike Static and
/// Poison Point, which still roll for a holder that fainted to the hit.
#[test]
fn a_holder_that_fainted_to_the_hit_draws_no_roll() {
    let mut holder = skitty_holder(FEMALE);
    holder.apply_damage(holder.stats().max_hp - 1);
    let (battle, events) =
        attacker_hits_enemy_holder(rattata_attacker(MALE, &[TACKLE]), holder, &[]);

    assert!(battle.enemy().is_fainted(), "{events:?}");
    assert_untouched(&battle, &events);
}

/// `gBattleMons[gBattlerAttacker].hp != 0` (`battle_util.c:2854`): an attacker
/// that fainted to its own Struggle recoil draws no roll.
#[test]
fn an_attacker_that_fainted_to_its_own_recoil_draws_no_roll() {
    let holder = max_iv_mon_with_personality(&Dex::new(), SKITTY.0, 40, vec![TACKLE], FEMALE);
    let mut attacker = max_iv_mon_with_personality(&Dex::new(), RATTATA.0, 60, vec![TACKLE], MALE);
    attacker.apply_damage(attacker.stats().max_hp - 1);
    for _ in 0..attacker.moves()[0].pp {
        attacker.deduct_pp(0).unwrap();
    }

    // The all-spent enemy's forced Struggle pick skips the selection draw, and
    // its recoil faints it before any contact draw.
    let (battle, events) = player_turn(holder, attacker, &[0, 0, 0, 1, 0]);

    assert!(battle.enemy().is_fainted(), "{events:?}");
    assert!(cute_charm_events(&events).is_empty(), "{events:?}");
    assert!(!battle.player().volatiles().infatuated());
}

#[test]
fn a_same_gender_pair_spends_the_roll_and_is_not_infatuated() {
    for gender in [FEMALE, MALE] {
        for roll in [CHARM_TRIGGERS, CHARM_ONE] {
            let (battle, events) = attacker_hits_enemy_holder(
                rattata_attacker(gender, &[TACKLE]),
                skitty_holder(gender),
                &[roll],
            );

            assert_untouched(&battle, &events);
        }
    }
}

#[test]
fn a_genderless_attacker_spends_the_roll_and_is_not_infatuated() {
    let attacker = mon(SpeciesId::BELDUM, FEMALE, &[TACKLE]);
    assert_eq!(attacker.gender(), battle::Gender::Genderless);
    let (battle, events) =
        attacker_hits_enemy_holder(attacker, skitty_holder(FEMALE), &[CHARM_TRIGGERS]);

    assert_untouched(&battle, &events);
}

/// Illumise is always female, so only Oblivious (`battle_util.c:2859`) stops
/// the male holder's charm.
#[test]
fn an_oblivious_attacker_spends_the_roll_and_is_not_infatuated() {
    let attacker = max_iv_mon_with_personality(
        &Dex::new(),
        SpeciesId::ILLUMISE.0,
        LEVEL + 1,
        vec![TACKLE],
        FEMALE,
    );
    assert_eq!(attacker.ability(), AbilityId::OBLIVIOUS);
    assert_eq!(attacker.gender(), battle::Gender::Female);
    let (battle, events) =
        attacker_hits_enemy_holder(attacker, skitty_holder(MALE), &[CHARM_TRIGGERS]);

    assert_untouched(&battle, &events);
}

/// `!(status2 & STATUS2_INFATUATION)` (`battle_util.c:2861`): the existing
/// infatuation keeps its original attractor and no event is reported.
#[test]
fn an_already_infatuated_attacker_spends_the_roll_and_keeps_its_attractor() {
    let mut attacker = rattata_attacker(MALE, &[TACKLE]);
    // The only attractor in singles is the opposing slot, so a distinct
    // sentinel is the only way to see the original kept.
    attacker
        .volatiles_mut()
        .set_infatuated_by(BattlerSide::Player);
    let (battle, events) = player_turn(
        attacker,
        skitty_holder(FEMALE),
        &script(&[
            &SETUP,
            &[LOVE_LETS_MOVE_THROUGH],
            &TACKLE_HITS,
            &[CHARM_TRIGGERS],
        ]),
    );

    assert_eq!(
        battle.player().volatiles().infatuated_by,
        Some(BattlerSide::Player),
        "{events:?}"
    );
    assert!(cute_charm_events(&events).is_empty(), "{events:?}");
}
