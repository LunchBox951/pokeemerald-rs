//! `EFFECT_ACCURACY_DOWN_HIT` (Mud-Slap, Octazooka) through
//! [`spend_effect_chance_draw`], [`ensure_admissible`] and
//! [`crate::hit::ensure_resolvable`].

use super::{
    ensure_admissible, is_accuracy_down_hit_effect, is_speed_down_hit_effect,
    spend_effect_chance_draw, SecondaryApplication,
};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::hit::ensure_resolvable;
use crate::pokemon::{BattlePokemon, Ivs};
use crate::script_rng::SequenceRng;
use assets::{AbilityId, MoveId, SpeciesId};

const MUD_SLAP: MoveId = MoveId::MUD_SLAP;
/// `EFFECT_ACCURACY_DOWN_HIT` with a 50% `secondaryEffectChance`.
const OCTAZOOKA: MoveId = MoveId::OCTAZOOKA;
const MAX_IVS: Ivs = Ivs {
    hp: 31,
    attack: 31,
    defense: 31,
    speed: 31,
    sp_attack: 31,
    sp_defense: 31,
};
const ZIGZAGOON: u16 = 288;
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

#[test]
fn only_effect_accuracy_down_hit_is_the_modelled_accuracy_trampoline() {
    let dex = Dex::new();
    for move_id in [MUD_SLAP, OCTAZOOKA] {
        let effect = dex.move_data(move_id).unwrap().effect;
        assert!(is_accuracy_down_hit_effect(effect), "{move_id:?}");
        assert!(!is_speed_down_hit_effect(effect), "{move_id:?}");
    }
    assert!(!is_accuracy_down_hit_effect(
        dex.move_data(MoveId::CONSTRICT).unwrap().effect
    ));
}

#[test]
fn mud_slap_reports_an_accuracy_drop_on_every_roll_and_always_draws_once() {
    let dex = Dex::new();
    let defender = mon(&dex, SpeciesId(ZIGZAGOON));
    for roll in [0, 99] {
        let mut rng = SequenceRng::new([roll]);
        assert_eq!(
            spend_effect_chance_draw(&dex, MUD_SLAP, true, &defender, &mut rng),
            Ok(SecondaryApplication::AccuracyDown),
            "roll {roll}: a 100% chance is not the draw-free certain branch"
        );
        rng.assert_exhausted();
    }
    let mut rng = SequenceRng::new([0]);
    assert_eq!(
        spend_effect_chance_draw(&dex, MUD_SLAP, false, &defender, &mut rng),
        Ok(SecondaryApplication::None),
        "an ineffective hit never carries the secondary"
    );
    rng.assert_exhausted();
}

#[test]
fn octazooka_lands_only_under_its_fifty_percent_chance() {
    let dex = Dex::new();
    let defender = mon(&dex, SpeciesId(ZIGZAGOON));
    for (roll, expected) in [
        (49, SecondaryApplication::AccuracyDown),
        (50, SecondaryApplication::None),
    ] {
        let mut rng = SequenceRng::new([roll]);
        assert_eq!(
            spend_effect_chance_draw(&dex, OCTAZOOKA, true, &defender, &mut rng),
            Ok(expected),
            "roll {roll}"
        );
        rng.assert_exhausted();
    }
}

#[test]
fn keen_eye_clear_body_white_smoke_and_shield_dust_stop_the_drop_but_still_draw() {
    let dex = Dex::new();
    let guards = [
        mon(&dex, SpeciesId(HITMONCHAN)),
        mon(&dex, SpeciesId(TENTACOOL)),
        mon(&dex, SpeciesId(WURMPLE)),
        white_smoke_holder(&dex),
    ];
    for defender in guards {
        let mut rng = SequenceRng::new([0]);
        assert_eq!(
            spend_effect_chance_draw(&dex, MUD_SLAP, true, &defender, &mut rng),
            Ok(SecondaryApplication::None),
            "{:?}",
            defender.ability()
        );
        rng.assert_exhausted();
    }
}

/// Keen Eye guards only Accuracy: the same defender still takes a Speed drop.
#[test]
fn keen_eye_does_not_stop_the_speed_drop() {
    let dex = Dex::new();
    let mut rng = SequenceRng::new([0]);
    assert_eq!(
        spend_effect_chance_draw(
            &dex,
            MoveId::CONSTRICT,
            true,
            &mon(&dex, SpeciesId(HITMONCHAN)),
            &mut rng
        ),
        Ok(SecondaryApplication::SpeedDown)
    );
}

#[test]
fn serene_grace_is_exempt_for_mud_slap_but_refused_for_octazooka_where_it_could_land() {
    let dex = Dex::new();
    let attacker = mon(&dex, SpeciesId(DUNSPARCE));
    assert_eq!(attacker.ability(), AbilityId::SERENE_GRACE);
    let zigzagoon = mon(&dex, SpeciesId(ZIGZAGOON));
    assert_eq!(
        ensure_admissible(&dex, MUD_SLAP, &attacker, &zigzagoon),
        Ok(()),
        "200% and 100% clear the same comparison in the same drawing branch"
    );
    assert_eq!(
        ensure_admissible(&dex, OCTAZOOKA, &attacker, &zigzagoon),
        Err(BattleError::UnportedAbilityInteraction(
            AbilityId::SERENE_GRACE
        ))
    );
    for guard in [
        mon(&dex, SpeciesId(HITMONCHAN)),
        mon(&dex, SpeciesId(TENTACOOL)),
        mon(&dex, SpeciesId(WURMPLE)),
        white_smoke_holder(&dex),
    ] {
        assert_eq!(
            ensure_admissible(&dex, OCTAZOOKA, &attacker, &guard),
            Ok(()),
            "{:?}: the drop can never land, so the doubling is unobservable",
            guard.ability()
        );
    }
}

#[test]
fn the_accuracy_down_hit_moves_are_resolvable_damaging_moves() {
    let dex = Dex::new();
    for move_id in [MUD_SLAP, OCTAZOOKA] {
        assert_eq!(ensure_resolvable(&dex, move_id), Ok(()), "{move_id:?}");
    }
}
