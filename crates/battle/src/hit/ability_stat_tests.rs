//! Ability stat-modifier regressions for the hit pipeline's damage path:
//! critical-suppressing armor abilities, Huge Power / Pure Power, Guts,
//! Marvel Scale, Thick Fat and Plus / Minus. Accuracy-modifying abilities
//! live in `ability_accuracy_tests`.

use super::test_fixtures::{
    mon, BEST_DAMAGE_DRAW, BULBASAUR, CRITICAL_SUPPRESSED_DRAWS, MAX_IVS,
    ORDINARY_NON_CRITICAL_DRAWS, ORDINARY_NO_CRIT_DRAW, SLASH, SQUIRTLE, TACKLE, WATER_GUN,
};
use super::{damage_before_roll, damage_core, resolve_hit, HitOutcome};
use crate::ability::{suppresses_critical_hits, GUTS, HUGE_POWER, MARVEL_SCALE, PURE_POWER};
use crate::damage::{
    apply_dual_type_effectiveness, base_damage, DamageInput, MoveCategory, Weather,
};
use crate::dex::Dex;
use crate::pokemon::{BattlePokemon, Ivs};
use crate::script_rng::SequenceRng;
use crate::stat_stage::StatStage;
use crate::status1::Status1;
use assets::species::AbilityId;
use assets::{MoveId, SpeciesId, Type};

const ABRA: SpeciesId = SpeciesId(63);

const SHELLDER: SpeciesId = SpeciesId(90);

const MARILL: SpeciesId = SpeciesId(183);

const MEDITITE: SpeciesId = SpeciesId(356);

const ANORITH: SpeciesId = SpeciesId(390);

/// `SPECIES_MAKUHITA`: Guts in ability slot 1 (Thick Fat is slot 0).
const MAKUHITA: SpeciesId = SpeciesId(335);

/// `SPECIES_MILOTIC`: Marvel Scale in its primary (and only) ability slot.
const MILOTIC: SpeciesId = SpeciesId(329);

/// `MOVE_ICICLE_SPEAR` -- `EFFECT_MULTI_HIT`, Ice; its loop reaches Thick Fat
/// through [`damage_before_roll`] alone.
const ICICLE_SPEAR: MoveId = MoveId::ICICLE_SPEAR;

const THICK_FAT: AbilityId = AbilityId::THICK_FAT;

const THICK_FAT_MARILL_TACKLE_DAMAGE: u32 = 3;

const HUGE_POWER_MARILL_TACKLE_DAMAGE: u32 = 5;

const PURE_POWER_MEDITITE_TACKLE_DAMAGE: u32 = 6;

const HUGE_POWER_BEFORE_STAGE_DAMAGE: u32 = 8;

/// L5 Makuhita (12 raw Attack) versus L5 Milotic (14 raw Defense), Tackle,
/// best roll, no boost active on either side.
const HEALTHY_MAKUHITA_TACKLE_DAMAGE: u32 = 4;

/// The same matchup with the Makuhita attacker paralysed or poisoned, so
/// Guts raises its raw Attack from 12 to 18 before the stage multiply.
const GUTS_MAKUHITA_TACKLE_DAMAGE: u32 = 5;

/// The same base matchup with the Milotic defender paralysed or poisoned, so
/// Marvel Scale raises its raw Defense from 14 to 21 before the stage
/// multiply.
const MARVEL_SCALE_MILOTIC_TACKLE_DAMAGE: u32 = 3;

#[test]
fn armor_abilities_skip_the_critical_draw_and_prevent_critical_hits() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 5, vec![SLASH]);

    for armor_species in [ANORITH, SHELLDER] {
        let defender = mon(&dex, armor_species, 5, vec![TACKLE]);
        assert!(suppresses_critical_hits(defender.ability()));
        let mut rng = SequenceRng::new(CRITICAL_SUPPRESSED_DRAWS);

        let resolution = resolve_hit(&dex, SLASH, &attacker, &defender, false, &mut rng).unwrap();

        assert!(matches!(
            resolution.outcome,
            HitOutcome::Hit {
                is_critical: false,
                ..
            }
        ));
        assert_eq!(rng.draws(), CRITICAL_SUPPRESSED_DRAWS.len());
    }
}

#[test]
fn huge_power_in_ability_slot_two_doubles_a_physical_hit() {
    let dex = Dex::new();
    let defender = mon(&dex, SQUIRTLE, 5, vec![TACKLE]);
    let thick_fat = BattlePokemon::new(&dex, MARILL, 5, MAX_IVS, 0, vec![TACKLE])
        .unwrap()
        .with_ability_slot(0);
    let huge_power = BattlePokemon::new(&dex, MARILL, 5, MAX_IVS, 0, vec![TACKLE])
        .unwrap()
        .with_ability_slot(1);
    assert_eq!(thick_fat.ability(), THICK_FAT);
    assert_eq!(huge_power.ability(), HUGE_POWER);

    let mut thick_fat_rng = SequenceRng::new(ORDINARY_NON_CRITICAL_DRAWS);
    assert_eq!(
        resolve_hit(
            &dex,
            TACKLE,
            &thick_fat,
            &defender,
            false,
            &mut thick_fat_rng,
        )
        .unwrap()
        .outcome,
        HitOutcome::Hit {
            damage: THICK_FAT_MARILL_TACKLE_DAMAGE,
            is_critical: false,
        }
    );

    let mut huge_power_rng = SequenceRng::new(ORDINARY_NON_CRITICAL_DRAWS);
    assert_eq!(
        resolve_hit(
            &dex,
            TACKLE,
            &huge_power,
            &defender,
            false,
            &mut huge_power_rng,
        )
        .unwrap()
        .outcome,
        HitOutcome::Hit {
            damage: HUGE_POWER_MARILL_TACKLE_DAMAGE,
            is_critical: false,
        }
    );
}

#[test]
fn pure_power_doubles_a_physical_hit() {
    let dex = Dex::new();
    let attacker = mon(&dex, MEDITITE, 5, vec![TACKLE]);
    let defender = mon(&dex, SQUIRTLE, 5, vec![TACKLE]);
    assert_eq!(attacker.ability(), PURE_POWER);
    let mut rng = SequenceRng::new(ORDINARY_NON_CRITICAL_DRAWS);

    let resolution = resolve_hit(&dex, TACKLE, &attacker, &defender, false, &mut rng).unwrap();

    assert_eq!(
        resolution.outcome,
        HitOutcome::Hit {
            damage: PURE_POWER_MEDITITE_TACKLE_DAMAGE,
            is_critical: false,
        }
    );
}

#[test]
fn huge_power_never_touches_a_special_move() {
    let dex = Dex::new();
    let defender = mon(&dex, SQUIRTLE, 5, vec![TACKLE]);
    let thick_fat = BattlePokemon::new(&dex, MARILL, 5, MAX_IVS, 0, vec![WATER_GUN])
        .unwrap()
        .with_ability_slot(0);
    let huge_power = BattlePokemon::new(&dex, MARILL, 5, MAX_IVS, 0, vec![WATER_GUN])
        .unwrap()
        .with_ability_slot(1);

    let mut thick_fat_rng = SequenceRng::new(ORDINARY_NON_CRITICAL_DRAWS);
    let thick_fat_outcome = resolve_hit(
        &dex,
        WATER_GUN,
        &thick_fat,
        &defender,
        false,
        &mut thick_fat_rng,
    )
    .unwrap();
    let mut huge_power_rng = SequenceRng::new(ORDINARY_NON_CRITICAL_DRAWS);
    let huge_power_outcome = resolve_hit(
        &dex,
        WATER_GUN,
        &huge_power,
        &defender,
        false,
        &mut huge_power_rng,
    )
    .unwrap();

    assert_eq!(thick_fat_outcome.outcome, huge_power_outcome.outcome);
}

#[test]
fn huge_power_doubles_raw_attack_before_stat_stage_scaling() {
    let dex = Dex::new();
    let mut attacker = BattlePokemon::new(&dex, MARILL, 15, MAX_IVS, 0, vec![TACKLE])
        .unwrap()
        .with_ability_slot(1);
    attacker.stages_mut().attack = StatStage::new(-2).unwrap();
    let defender = mon(&dex, ABRA, 15, vec![TACKLE]);
    let mut rng = SequenceRng::new(ORDINARY_NON_CRITICAL_DRAWS);

    let resolution = resolve_hit(&dex, TACKLE, &attacker, &defender, false, &mut rng).unwrap();

    assert_eq!(
        resolution.outcome,
        HitOutcome::Hit {
            damage: HUGE_POWER_BEFORE_STAGE_DAMAGE,
            is_critical: false,
        }
    );
}

#[test]
fn a_statused_guts_attacker_raises_physical_damage() {
    let dex = Dex::new();
    let healthy = BattlePokemon::new(&dex, MAKUHITA, 5, MAX_IVS, 0, vec![TACKLE])
        .unwrap()
        .with_ability_slot(1);
    assert_eq!(healthy.ability(), GUTS);
    let mut statused = healthy.clone();
    statused.set_status1(Status1::Paralysed);
    let defender = mon(&dex, MILOTIC, 5, vec![TACKLE]);

    let mut healthy_rng = SequenceRng::new(ORDINARY_NON_CRITICAL_DRAWS);
    let healthy_outcome =
        resolve_hit(&dex, TACKLE, &healthy, &defender, false, &mut healthy_rng).unwrap();
    assert_eq!(
        healthy_outcome.outcome,
        HitOutcome::Hit {
            damage: HEALTHY_MAKUHITA_TACKLE_DAMAGE,
            is_critical: false,
        }
    );

    let mut statused_rng = SequenceRng::new(ORDINARY_NON_CRITICAL_DRAWS);
    let statused_outcome =
        resolve_hit(&dex, TACKLE, &statused, &defender, false, &mut statused_rng).unwrap();
    assert_eq!(
        statused_outcome.outcome,
        HitOutcome::Hit {
            damage: GUTS_MAKUHITA_TACKLE_DAMAGE,
            is_critical: false,
        }
    );
}

#[test]
fn a_statused_marvel_scale_defender_lowers_physical_damage() {
    let dex = Dex::new();
    let attacker = mon(&dex, MAKUHITA, 5, vec![TACKLE]);
    let healthy_defender = mon(&dex, MILOTIC, 5, vec![TACKLE]);
    assert_eq!(healthy_defender.ability(), MARVEL_SCALE);
    let mut statused_defender = healthy_defender.clone();
    statused_defender.set_status1(Status1::Poisoned);

    let mut healthy_rng = SequenceRng::new(ORDINARY_NON_CRITICAL_DRAWS);
    let healthy_outcome = resolve_hit(
        &dex,
        TACKLE,
        &attacker,
        &healthy_defender,
        false,
        &mut healthy_rng,
    )
    .unwrap();
    assert_eq!(
        healthy_outcome.outcome,
        HitOutcome::Hit {
            damage: HEALTHY_MAKUHITA_TACKLE_DAMAGE,
            is_critical: false,
        }
    );

    let mut statused_rng = SequenceRng::new(ORDINARY_NON_CRITICAL_DRAWS);
    let statused_outcome = resolve_hit(
        &dex,
        TACKLE,
        &attacker,
        &statused_defender,
        false,
        &mut statused_rng,
    )
    .unwrap();
    assert_eq!(
        statused_outcome.outcome,
        HitOutcome::Hit {
            damage: MARVEL_SCALE_MILOTIC_TACKLE_DAMAGE,
            is_critical: false,
        }
    );
}

#[test]
fn guts_never_touches_special_attack() {
    let dex = Dex::new();
    let mut attacker = BattlePokemon::new(&dex, MAKUHITA, 5, MAX_IVS, 0, vec![WATER_GUN])
        .unwrap()
        .with_ability_slot(1);
    assert_eq!(attacker.ability(), GUTS);
    let healthy_special_attack = attacker.attacking_stat(MoveCategory::Special);

    attacker.set_status1(Status1::Paralysed);

    assert_eq!(
        attacker.attacking_stat(MoveCategory::Special),
        healthy_special_attack,
        "Guts must not touch Special Attack even once its holder is statused"
    );
}

#[test]
fn marvel_scale_never_touches_special_defense() {
    let dex = Dex::new();
    let mut defender = mon(&dex, MILOTIC, 5, vec![WATER_GUN]);
    assert_eq!(defender.ability(), MARVEL_SCALE);
    let healthy_special_defense = defender.defending_stat(MoveCategory::Special);

    defender.set_status1(Status1::Poisoned);

    assert_eq!(
        defender.defending_stat(MoveCategory::Special),
        healthy_special_defense,
        "Marvel Scale must not touch Special Defense even once its holder is statused"
    );
}

/// Upstream halves the raw (odd-truncated) Special Attack for Thick Fat before
/// stage scaling (`pokemon.c:3202-3204`), so a +1 stage scales the halved stat.
#[test]
fn thick_fat_halves_raw_special_attack_before_stage_scaling_in_the_raw_damage_input() {
    let dex = Dex::new();
    let mut attacker = mon(&dex, ABRA, 15, vec![ICICLE_SPEAR]);
    attacker.stages_mut().sp_attack = StatStage::new(1).unwrap();
    let raw_special_attack = attacker.stats().sp_attack;
    assert_eq!(
        raw_special_attack % 2,
        1,
        "the stat must be odd to pin truncation"
    );
    let thick_fat = BattlePokemon::new(&dex, MARILL, 5, MAX_IVS, 0, vec![TACKLE])
        .unwrap()
        .with_ability_slot(0);
    assert_eq!(thick_fat.ability(), THICK_FAT);
    let (defense, defense_stage) = thick_fat.defending_stat(MoveCategory::Special);
    let expected_base = base_damage(&DamageInput {
        attacker_level: attacker.level(),
        power: 10,
        move_type: Type::Ice,
        attack_stat: raw_special_attack / 2,
        attack_stage: StatStage::new(1).unwrap(),
        defense_stat: defense,
        defense_stage,
        attacker_burned: false,
        reflect: false,
        light_screen: false,
        weather: Weather::None,
        is_solar_beam: false,
        attacker_pinch_boost: false,
    });
    let mut rng = SequenceRng::new([ORDINARY_NO_CRIT_DRAW]);

    let raw =
        damage_before_roll(&dex, ICICLE_SPEAR, &attacker, &thick_fat, false, &mut rng).unwrap();

    assert_eq!(
        raw.damage,
        apply_dual_type_effectiveness(expected_base, Type::Ice, thick_fat.types())
    );
    assert_eq!(rng.draws(), 1, "Thick Fat spends no extra draw");
}

/// Thick Fat halves raw 7 to 3 and -6 truncates it to 0, which upstream never
/// floors (`pokemon.c:3100-3104`, `:3298-3315`): 2 HP per hit, not 3.
#[test]
fn icicle_spear_into_thick_fat_keeps_a_zero_stage_adjusted_special_attack() {
    const SMEARGLE: SpeciesId = SpeciesId(235);
    const ADAMANT_PERSONALITY: u32 = 3;
    const NAIVE_PERSONALITY: u32 = 14;
    const ZERO_IVS: Ivs = Ivs {
        hp: 0,
        attack: 0,
        defense: 0,
        speed: 0,
        sp_attack: 0,
        sp_defense: 0,
    };
    let dex = Dex::new();
    let mut attacker = BattlePokemon::new(
        &dex,
        SMEARGLE,
        8,
        ZERO_IVS,
        ADAMANT_PERSONALITY,
        vec![ICICLE_SPEAR],
    )
    .unwrap();
    attacker.stages_mut().sp_attack = StatStage::MIN;
    let mut defender =
        BattlePokemon::new(&dex, MAKUHITA, 5, ZERO_IVS, NAIVE_PERSONALITY, vec![TACKLE])
            .unwrap()
            .with_ability_slot(0);
    defender.stages_mut().sp_defense = StatStage::MIN;
    assert_eq!(attacker.stats().sp_attack, 7);
    assert_eq!(defender.stats().sp_defense, 7);
    assert_eq!(defender.ability(), THICK_FAT);
    let mut rng = SequenceRng::new([ORDINARY_NO_CRIT_DRAW, BEST_DAMAGE_DRAW]);

    let outcome = damage_core(&dex, ICICLE_SPEAR, &attacker, &defender, false, &mut rng).unwrap();

    assert_eq!(
        outcome,
        HitOutcome::Hit {
            damage: 2,
            is_critical: false
        }
    );
    assert_eq!(rng.draws(), 2);
}

/// Upstream raises raw Special Attack 150% for Plus against Minus and for
/// Minus against Plus, opponents included, before stage scaling
/// (`pokemon.c:3207-3210`, `battle_util.c:3128-3137`).
fn plus_minus_expected_damage(
    dex: &Dex,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
    boosted: bool,
) -> u32 {
    let raw = attacker.stats().sp_attack;
    let (defense, defense_stage) = defender.defending_stat(MoveCategory::Special);
    let expected_base = base_damage(&DamageInput {
        attacker_level: attacker.level(),
        power: u32::from(dex.move_data(WATER_GUN).unwrap().power),
        move_type: Type::Water,
        attack_stat: if boosted { raw * 150 / 100 } else { raw },
        attack_stage: attacker.stages().sp_attack,
        defense_stat: defense,
        defense_stage,
        attacker_burned: false,
        reflect: false,
        light_screen: false,
        weather: Weather::None,
        is_solar_beam: false,
        attacker_pinch_boost: false,
    });
    apply_dual_type_effectiveness(expected_base, Type::Water, defender.types())
}

#[test]
fn plus_and_minus_boost_special_attack_against_the_complementary_opponent() {
    let dex = Dex::new();
    let mut plusle = mon(&dex, SpeciesId(353), 20, vec![WATER_GUN]);
    let mut minun = mon(&dex, SpeciesId(354), 20, vec![WATER_GUN]);
    assert_eq!(plusle.ability(), AbilityId::PLUS);
    assert_eq!(minun.ability(), AbilityId::MINUS);
    // An odd raw stat at stage +2 separates "boost then stage" from any other order.
    plusle.stages_mut().sp_attack = StatStage::new(2).unwrap();
    minun.stages_mut().sp_attack = StatStage::new(2).unwrap();
    assert_eq!(plusle.stats().sp_attack % 2, 1, "pin truncation");
    assert_eq!(minun.stats().sp_attack % 2, 1, "pin truncation");

    for (attacker, defender) in [(&plusle, &minun), (&minun, &plusle)] {
        let mut rng = SequenceRng::new([ORDINARY_NO_CRIT_DRAW]);
        let raw = damage_before_roll(&dex, WATER_GUN, attacker, defender, false, &mut rng).unwrap();
        assert_eq!(
            raw.damage,
            plus_minus_expected_damage(&dex, attacker, defender, true)
        );
        assert_ne!(
            raw.damage,
            plus_minus_expected_damage(&dex, attacker, defender, false)
        );
    }
}

#[test]
fn plus_without_a_complementary_opponent_gets_no_special_attack_boost() {
    let dex = Dex::new();
    let plusle = mon(&dex, SpeciesId(353), 20, vec![WATER_GUN]);
    let squirtle = mon(&dex, SQUIRTLE, 20, vec![WATER_GUN]);
    let other_plusle = mon(&dex, SpeciesId(353), 20, vec![WATER_GUN]);

    for defender in [&squirtle, &other_plusle] {
        let mut rng = SequenceRng::new([ORDINARY_NO_CRIT_DRAW]);
        let raw = damage_before_roll(&dex, WATER_GUN, &plusle, defender, false, &mut rng).unwrap();
        assert_eq!(
            raw.damage,
            plus_minus_expected_damage(&dex, &plusle, defender, false)
        );
    }
}
