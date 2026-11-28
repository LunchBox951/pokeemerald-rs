//! `EFFECT_SPECIAL_ATTACK_DOWN_HIT` (Mist Ball) through
//! [`spend_effect_chance_draw`], [`ensure_admissible`] and
//! [`crate::hit::ensure_resolvable`].

use super::{
    ensure_admissible, is_accuracy_down_hit_effect, is_attack_down_hit_effect,
    is_special_attack_down_hit_effect, is_speed_down_hit_effect, spend_effect_chance_draw,
    stat_drop_can_land, SecondaryApplication,
};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::hit::ensure_resolvable;
use crate::pokemon::{BattlePokemon, Ivs};
use crate::script_rng::SequenceRng;
use crate::stat_change::ChangedStat;
use assets::{AbilityId, MoveId, SpeciesId};

/// `EFFECT_SPECIAL_ATTACK_DOWN_HIT` with a 50% `secondaryEffectChance`.
const MIST_BALL: MoveId = MoveId::MIST_BALL;
const MAX_IVS: Ivs = Ivs {
    hp: 31,
    attack: 31,
    defense: 31,
    speed: 31,
    sp_attack: 31,
    sp_defense: 31,
};
const ZIGZAGOON: u16 = 288;
/// Hyper Cutter in its only ability slot.
const PINSIR: u16 = SpeciesId::PINSIR.0;
/// Clear Body in its only ability slot.
const BELDUM: u16 = SpeciesId::BELDUM.0;
/// Shield Dust in its only ability slot.
const WURMPLE: u16 = 290;
const DUNSPARCE: u16 = 206;

fn mon(dex: &Dex, species: SpeciesId) -> BattlePokemon {
    BattlePokemon::new(dex, species, 10, MAX_IVS, 0, vec![MoveId::TACKLE]).unwrap()
}

/// Every defender whose ability silently refuses a secondary Special Attack
/// drop. Hyper Cutter is not among them: it guards Attack only.
fn special_attack_drop_guards(dex: &Dex) -> [BattlePokemon; 3] {
    let torkoal = mon(dex, SpeciesId::TORKOAL);
    let beldum = mon(dex, SpeciesId(BELDUM));
    assert_eq!(torkoal.ability(), AbilityId::WHITE_SMOKE);
    assert_eq!(beldum.ability(), AbilityId::CLEAR_BODY);
    [beldum, mon(dex, SpeciesId(WURMPLE)), torkoal]
}

fn hyper_cutter_holder(dex: &Dex) -> BattlePokemon {
    let pinsir = mon(dex, SpeciesId(PINSIR));
    assert_eq!(pinsir.ability(), AbilityId::HYPER_CUTTER);
    pinsir
}

#[test]
fn only_effect_special_attack_down_hit_is_the_modelled_special_attack_trampoline() {
    let dex = Dex::new();
    let effect = dex.move_data(MIST_BALL).unwrap().effect;
    assert!(is_special_attack_down_hit_effect(effect));
    assert!(!is_attack_down_hit_effect(effect));
    assert!(!is_speed_down_hit_effect(effect));
    assert!(!is_accuracy_down_hit_effect(effect));
    for sibling in [
        MoveId::AURORA_BEAM,
        MoveId::CONSTRICT,
        MoveId::LUSTER_PURGE,
        MoveId::PSYCHIC,
    ] {
        assert!(
            !is_special_attack_down_hit_effect(dex.move_data(sibling).unwrap().effect),
            "{sibling:?}"
        );
    }
}

#[test]
fn mist_ball_lands_only_below_its_fifty_percent_threshold() {
    let dex = Dex::new();
    let defender = mon(&dex, SpeciesId(ZIGZAGOON));
    for (roll, expected) in [
        (0, SecondaryApplication::SpecialAttackDown),
        (49, SecondaryApplication::SpecialAttackDown),
        (50, SecondaryApplication::None),
        (99, SecondaryApplication::None),
        // `Random() % 100` wraps: 149 is a 49 and 150 is a 50.
        (149, SecondaryApplication::SpecialAttackDown),
        (150, SecondaryApplication::None),
    ] {
        let mut rng = SequenceRng::new([roll]);
        assert_eq!(
            spend_effect_chance_draw(&dex, MIST_BALL, true, &defender, &mut rng),
            Ok(expected),
            "roll {roll}"
        );
        rng.assert_exhausted();
    }
}

#[test]
fn an_ineffective_mist_ball_hit_still_spends_one_discarded_draw() {
    let dex = Dex::new();
    let defender = mon(&dex, SpeciesId(ZIGZAGOON));
    let mut rng = SequenceRng::new([0]);
    assert_eq!(
        spend_effect_chance_draw(&dex, MIST_BALL, false, &defender, &mut rng),
        Ok(SecondaryApplication::None)
    );
    rng.assert_exhausted();
}

#[test]
fn clear_body_white_smoke_and_shield_dust_stop_the_drop_but_still_draw() {
    let dex = Dex::new();
    for defender in special_attack_drop_guards(&dex) {
        let mut rng = SequenceRng::new([0]);
        assert_eq!(
            spend_effect_chance_draw(&dex, MIST_BALL, true, &defender, &mut rng),
            Ok(SecondaryApplication::None),
            "{:?}",
            defender.ability()
        );
        rng.assert_exhausted();
    }
}

/// Hyper Cutter guards `STAT_ATK` only (`battle_script_commands.c:7016-7028`),
/// so a Hyper Cutter holder still takes the Special Attack drop.
#[test]
fn hyper_cutter_does_not_protect_special_attack() {
    let dex = Dex::new();
    let pinsir = hyper_cutter_holder(&dex);
    assert!(stat_drop_can_land(ChangedStat::SpAttack, &pinsir));
    assert!(!stat_drop_can_land(ChangedStat::Attack, &pinsir));
    let mut rng = SequenceRng::new([0]);
    assert_eq!(
        spend_effect_chance_draw(&dex, MIST_BALL, true, &pinsir, &mut rng),
        Ok(SecondaryApplication::SpecialAttackDown)
    );
    rng.assert_exhausted();
}

#[test]
fn the_application_names_special_attack_as_the_dropped_stat() {
    assert_eq!(
        SecondaryApplication::SpecialAttackDown.dropped_stat(),
        Some(ChangedStat::SpAttack)
    );
    assert_eq!(
        SecondaryApplication::AttackDown.dropped_stat(),
        Some(ChangedStat::Attack)
    );
}

#[test]
fn serene_grace_is_refused_for_mist_ball_only_where_the_drop_could_land() {
    let dex = Dex::new();
    let attacker = mon(&dex, SpeciesId(DUNSPARCE));
    assert_eq!(attacker.ability(), AbilityId::SERENE_GRACE);
    for open in [mon(&dex, SpeciesId(ZIGZAGOON)), hyper_cutter_holder(&dex)] {
        assert_eq!(
            ensure_admissible(&dex, MIST_BALL, &attacker, &open),
            Err(BattleError::UnportedAbilityInteraction(
                AbilityId::SERENE_GRACE
            )),
            "{:?}",
            open.ability()
        );
    }
    for guard in special_attack_drop_guards(&dex) {
        assert_eq!(
            ensure_admissible(&dex, MIST_BALL, &attacker, &guard),
            Ok(()),
            "{:?}: the drop can never land, so the doubling is unobservable",
            guard.ability()
        );
    }
    let plain_attacker = mon(&dex, SpeciesId(ZIGZAGOON));
    assert_eq!(
        ensure_admissible(
            &dex,
            MIST_BALL,
            &plain_attacker,
            &mon(&dex, SpeciesId(ZIGZAGOON))
        ),
        Ok(())
    );
}

#[test]
fn mist_ball_is_a_resolvable_damaging_move() {
    assert_eq!(ensure_resolvable(&Dex::new(), MIST_BALL), Ok(()));
}

/// Luster Purge's Special Defense sibling is not ported and keeps failing
/// closed after its draw.
#[test]
fn the_special_defense_sibling_stays_refused() {
    let dex = Dex::new();
    let defender = mon(&dex, SpeciesId(ZIGZAGOON));
    let mut rng = SequenceRng::new([0]);
    assert_eq!(
        spend_effect_chance_draw(&dex, MoveId::LUSTER_PURGE, true, &defender, &mut rng),
        Err(BattleError::UnportedSecondaryEffect(MoveId::LUSTER_PURGE))
    );
    rng.assert_exhausted();
    assert_eq!(
        ensure_resolvable(&dex, MoveId::LUSTER_PURGE),
        Err(BattleError::UnsupportedMoveEffect(MoveId::LUSTER_PURGE))
    );
}
