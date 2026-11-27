//! `EFFECT_ATTACK_DOWN_HIT` (Aurora Beam) through [`spend_effect_chance_draw`],
//! [`ensure_admissible`] and [`crate::hit::ensure_resolvable`].

use super::{
    ensure_admissible, is_accuracy_down_hit_effect, is_attack_down_hit_effect,
    is_speed_down_hit_effect, spend_effect_chance_draw, stat_drop_can_land, SecondaryApplication,
};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::hit::ensure_resolvable;
use crate::pokemon::{BattlePokemon, Ivs};
use crate::script_rng::SequenceRng;
use crate::stat_change::ChangedStat;
use assets::{AbilityId, MoveId, SpeciesId};

/// `EFFECT_ATTACK_DOWN_HIT` with a 10% `secondaryEffectChance`.
const AURORA_BEAM: MoveId = MoveId::AURORA_BEAM;
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
/// Keen Eye in its only ability slot.
const HITMONCHAN: u16 = SpeciesId::HITMONCHAN.0;
/// Clear Body in its first ability slot.
const TENTACOOL: u16 = 72;
/// Shield Dust in its only ability slot.
const WURMPLE: u16 = 290;
const DUNSPARCE: u16 = 206;

fn mon(dex: &Dex, species: SpeciesId) -> BattlePokemon {
    BattlePokemon::new(dex, species, 10, MAX_IVS, 0, vec![MoveId::TACKLE]).unwrap()
}

fn white_smoke_holder(dex: &Dex) -> BattlePokemon {
    let torkoal = mon(dex, SpeciesId::TORKOAL);
    assert_eq!(torkoal.ability(), AbilityId::WHITE_SMOKE);
    torkoal
}

fn hyper_cutter_holder(dex: &Dex) -> BattlePokemon {
    let pinsir = mon(dex, SpeciesId(PINSIR));
    assert_eq!(pinsir.ability(), AbilityId::HYPER_CUTTER);
    pinsir
}

/// Every defender whose ability silently refuses a secondary Attack drop.
fn attack_drop_guards(dex: &Dex) -> [BattlePokemon; 4] {
    [
        mon(dex, SpeciesId(TENTACOOL)),
        mon(dex, SpeciesId(WURMPLE)),
        white_smoke_holder(dex),
        hyper_cutter_holder(dex),
    ]
}

#[test]
fn only_effect_attack_down_hit_is_the_modelled_attack_trampoline() {
    let dex = Dex::new();
    let effect = dex.move_data(AURORA_BEAM).unwrap().effect;
    assert!(is_attack_down_hit_effect(effect));
    assert!(!is_speed_down_hit_effect(effect));
    assert!(!is_accuracy_down_hit_effect(effect));
    for sibling in [
        MoveId::CONSTRICT,
        MoveId::MUD_SLAP,
        MoveId::MIST_BALL,
        MoveId::CRUNCH,
    ] {
        assert!(
            !is_attack_down_hit_effect(dex.move_data(sibling).unwrap().effect),
            "{sibling:?}"
        );
    }
}

#[test]
fn aurora_beam_lands_only_below_its_ten_percent_threshold() {
    let dex = Dex::new();
    let defender = mon(&dex, SpeciesId(ZIGZAGOON));
    for (roll, expected) in [
        (0, SecondaryApplication::AttackDown),
        (9, SecondaryApplication::AttackDown),
        (10, SecondaryApplication::None),
        (99, SecondaryApplication::None),
        // `Random() % 100` wraps: 109 is a 9 and 110 is a 10.
        (109, SecondaryApplication::AttackDown),
        (110, SecondaryApplication::None),
    ] {
        let mut rng = SequenceRng::new([roll]);
        assert_eq!(
            spend_effect_chance_draw(&dex, AURORA_BEAM, true, &defender, &mut rng),
            Ok(expected),
            "roll {roll}"
        );
        rng.assert_exhausted();
    }
}

#[test]
fn an_ineffective_aurora_beam_hit_still_spends_one_discarded_draw() {
    let dex = Dex::new();
    let defender = mon(&dex, SpeciesId(ZIGZAGOON));
    let mut rng = SequenceRng::new([0]);
    assert_eq!(
        spend_effect_chance_draw(&dex, AURORA_BEAM, false, &defender, &mut rng),
        Ok(SecondaryApplication::None)
    );
    rng.assert_exhausted();
}

#[test]
fn clear_body_white_smoke_shield_dust_and_hyper_cutter_stop_the_drop_but_still_draw() {
    let dex = Dex::new();
    for defender in attack_drop_guards(&dex) {
        let mut rng = SequenceRng::new([0]);
        assert_eq!(
            spend_effect_chance_draw(&dex, AURORA_BEAM, true, &defender, &mut rng),
            Ok(SecondaryApplication::None),
            "{:?}",
            defender.ability()
        );
        rng.assert_exhausted();
    }
}

/// Hyper Cutter and Keen Eye each guard exactly one stat: Hyper Cutter does not
/// stop a Speed or Accuracy drop, and Keen Eye does not stop the Attack drop.
#[test]
fn hyper_cutter_and_keen_eye_guard_only_their_own_stat() {
    let dex = Dex::new();
    let pinsir = hyper_cutter_holder(&dex);
    for (move_id, expected) in [
        (MoveId::CONSTRICT, SecondaryApplication::SpeedDown),
        (MoveId::MUD_SLAP, SecondaryApplication::AccuracyDown),
    ] {
        let mut rng = SequenceRng::new([0]);
        assert_eq!(
            spend_effect_chance_draw(&dex, move_id, true, &pinsir, &mut rng),
            Ok(expected),
            "{move_id:?}"
        );
    }
    let hitmonchan = mon(&dex, SpeciesId(HITMONCHAN));
    assert_eq!(hitmonchan.ability(), AbilityId::KEEN_EYE);
    let mut rng = SequenceRng::new([0]);
    assert_eq!(
        spend_effect_chance_draw(&dex, AURORA_BEAM, true, &hitmonchan, &mut rng),
        Ok(SecondaryApplication::AttackDown)
    );
}

/// A defender whose `ability()` is `ABILITY_NONE`, as a saved single-ability
/// species on ability slot 1 holds upstream. `BattlePokemon::new` cannot build
/// one in this tree, so the state is reached through the crate-private
/// `trace_ability`, which makes `ability()` read `NONE` exactly as a native
/// `ABILITY_NONE` slot would.
fn none_ability_defender(dex: &Dex) -> BattlePokemon {
    let mut defender = mon(dex, SpeciesId(ZIGZAGOON));
    defender.trace_ability(AbilityId::NONE);
    assert_eq!(defender.ability(), AbilityId::NONE);
    defender
}

#[test]
fn a_defender_holding_ability_none_takes_the_speed_and_attack_drops() {
    let dex = Dex::new();
    let defender = none_ability_defender(&dex);
    for stat in [ChangedStat::Speed, ChangedStat::Attack] {
        assert!(stat_drop_can_land(stat, &defender), "{stat:?}");
    }
    for (move_id, expected) in [
        (MoveId::CONSTRICT, SecondaryApplication::SpeedDown),
        (AURORA_BEAM, SecondaryApplication::AttackDown),
    ] {
        let mut rng = SequenceRng::new([0]);
        assert_eq!(
            spend_effect_chance_draw(&dex, move_id, true, &defender, &mut rng),
            Ok(expected),
            "{move_id:?}"
        );
    }
}

#[test]
fn serene_grace_is_refused_for_aurora_beam_only_where_the_drop_could_land() {
    let dex = Dex::new();
    let attacker = mon(&dex, SpeciesId(DUNSPARCE));
    assert_eq!(attacker.ability(), AbilityId::SERENE_GRACE);
    for open in [
        mon(&dex, SpeciesId(ZIGZAGOON)),
        mon(&dex, SpeciesId(HITMONCHAN)),
    ] {
        assert_eq!(
            ensure_admissible(&dex, AURORA_BEAM, &attacker, &open),
            Err(BattleError::UnportedAbilityInteraction(
                AbilityId::SERENE_GRACE
            )),
            "{:?}",
            open.ability()
        );
    }
    for guard in attack_drop_guards(&dex) {
        assert_eq!(
            ensure_admissible(&dex, AURORA_BEAM, &attacker, &guard),
            Ok(()),
            "{:?}: the drop can never land, so the doubling is unobservable",
            guard.ability()
        );
    }
    let plain_attacker = mon(&dex, SpeciesId(ZIGZAGOON));
    assert_eq!(
        ensure_admissible(
            &dex,
            AURORA_BEAM,
            &plain_attacker,
            &mon(&dex, SpeciesId(ZIGZAGOON))
        ),
        Ok(())
    );
}

#[test]
fn aurora_beam_is_a_resolvable_damaging_move() {
    assert_eq!(ensure_resolvable(&Dex::new(), AURORA_BEAM), Ok(()));
}

/// A sibling whose effect is not ported keeps failing closed after its draw.
#[test]
fn an_unported_sibling_stat_drop_effect_stays_refused() {
    let dex = Dex::new();
    let defender = mon(&dex, SpeciesId(ZIGZAGOON));
    let mut rng = SequenceRng::new([0]);
    assert_eq!(
        spend_effect_chance_draw(&dex, MoveId::MIST_BALL, true, &defender, &mut rng),
        Err(BattleError::UnportedSecondaryEffect(MoveId::MIST_BALL))
    );
    rng.assert_exhausted();
    assert_eq!(
        ensure_resolvable(&dex, MoveId::MIST_BALL),
        Err(BattleError::UnsupportedMoveEffect(MoveId::MIST_BALL))
    );
}
