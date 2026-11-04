//! Compound Eyes and Hustle regressions for the hit pipeline: the accuracy
//! threshold each ability moves and Hustle's physical-damage boost.

use super::test_fixtures::{
    mon, BEST_DAMAGE_DRAW, DISCARDED_EFFECT_DRAW, MAX_IVS, ORDINARY_NON_CRITICAL_DRAWS,
    ORDINARY_NO_CRIT_DRAW, SLAM, SQUIRTLE, TACKLE, WATER_GUN,
};
use super::{resolve_hit, HitOutcome};
use crate::damage::{base_damage, DamageInput, MoveCategory, Weather};
use crate::dex::Dex;
use crate::pokemon::BattlePokemon;
use crate::script_rng::SequenceRng;
use assets::species::AbilityId;
use assets::{SpeciesId, Type};

/// `SPECIES_BUTTERFREE`: Compound Eyes in its primary (and only) ability slot.
const BUTTERFREE: SpeciesId = SpeciesId(12);

/// `SPECIES_REMORAID`: Hustle in its primary (and only) ability slot.
const REMORAID: SpeciesId = SpeciesId(223);

/// `SPECIES_DELIBIRD`: Vital Spirit in slot 0, Hustle in slot 1 -- unlike
/// Remoraid, a same-species non-Hustle control is reachable through
/// [`BattlePokemon::with_ability_slot`].
const DELIBIRD: SpeciesId = SpeciesId(225);

/// Slam's plain 75 threshold misses roll 91, but Compound Eyes raises the
/// threshold to `75 * 130 / 100 = 97`, which the same roll clears
/// (`battle_script_commands.c:1152-1153`).
const COMPOUND_EYES_ONLY_HIT_DRAW: u16 = 90;

/// Slam's plain 75 threshold hits roll 66, but Hustle lowers a physical
/// move's threshold to `75 * 80 / 100 = 60`, which the same roll exceeds
/// (`battle_script_commands.c:1156-1157`).
const HUSTLE_ONLY_MISS_DRAW: u16 = 65;

#[test]
fn compound_eyes_raises_the_accuracy_threshold_of_an_executable_move() {
    let dex = Dex::new();
    let attacker = mon(&dex, BUTTERFREE, 10, vec![SLAM]);
    assert_eq!(attacker.ability(), AbilityId::COMPOUND_EYES);
    let defender = mon(&dex, SQUIRTLE, 10, vec![TACKLE]);
    let mut rng = SequenceRng::new([
        COMPOUND_EYES_ONLY_HIT_DRAW,
        ORDINARY_NO_CRIT_DRAW,
        BEST_DAMAGE_DRAW,
        DISCARDED_EFFECT_DRAW,
    ]);

    let resolution = resolve_hit(&dex, SLAM, &attacker, &defender, false, &mut rng).unwrap();

    assert!(
        matches!(resolution.outcome, HitOutcome::Hit { .. }),
        "roll 91 is within Compound Eyes' 97 threshold: {:?}",
        resolution.outcome
    );
    assert_eq!(
        rng.draws(),
        4,
        "a Compound Eyes hit continues through the remaining move draws"
    );
}

#[test]
fn hustle_lowers_the_accuracy_threshold_of_a_physical_executable_move() {
    let dex = Dex::new();
    let attacker = mon(&dex, REMORAID, 10, vec![SLAM]);
    assert_eq!(attacker.ability(), AbilityId::HUSTLE);
    let defender = mon(&dex, SQUIRTLE, 10, vec![TACKLE]);
    let mut rng = SequenceRng::new([HUSTLE_ONLY_MISS_DRAW]);

    let resolution = resolve_hit(&dex, SLAM, &attacker, &defender, false, &mut rng).unwrap();

    assert_eq!(
        resolution.outcome,
        HitOutcome::Miss,
        "roll 66 exceeds Hustle's 60 threshold"
    );
    assert_eq!(rng.draws(), 1, "a miss stops immediately");
}

#[test]
fn hustle_raises_a_physical_attackers_landed_damage() {
    let dex = Dex::new();
    let hustle_attacker = BattlePokemon::new(&dex, DELIBIRD, 10, MAX_IVS, 0, vec![TACKLE])
        .unwrap()
        .with_ability_slot(1);
    assert_eq!(hustle_attacker.ability(), AbilityId::HUSTLE);
    let control = BattlePokemon::new(&dex, DELIBIRD, 10, MAX_IVS, 0, vec![TACKLE])
        .unwrap()
        .with_ability_slot(0);
    assert_ne!(control.ability(), AbilityId::HUSTLE);
    let defender = mon(&dex, SQUIRTLE, 10, vec![TACKLE]);

    let mut control_rng = SequenceRng::new(ORDINARY_NON_CRITICAL_DRAWS);
    let HitOutcome::Hit {
        damage: control_damage,
        ..
    } = resolve_hit(&dex, TACKLE, &control, &defender, false, &mut control_rng)
        .unwrap()
        .outcome
    else {
        panic!("Tackle must land on a Water/Water matchup");
    };

    // Pinned independently through `base_damage` with the control's raw
    // Attack scaled 150%, rather than derived from `control_damage`, since
    // `pokemon.c:3205-3206`'s truncation happens before the stage multiply
    // and would not generally commute with scaling the already-rounded
    // control figure.
    let (raw_attack, attack_stage) = control.attacking_stat(MoveCategory::Physical);
    let (defense_stat, defense_stage) = defender.defending_stat(MoveCategory::Physical);
    let expected_hustle_damage = base_damage(&DamageInput {
        attacker_level: control.level(),
        power: u32::from(dex.move_data(TACKLE).unwrap().power),
        move_type: Type::Normal,
        attack_stat: 150 * raw_attack / 100,
        attack_stage,
        defense_stat,
        defense_stage,
        attacker_burned: false,
        reflect: false,
        light_screen: false,
        weather: Weather::None,
        is_solar_beam: false,
        attacker_pinch_boost: false,
    });
    assert!(
        expected_hustle_damage > control_damage,
        "fixture must be sensitive to the boost: {expected_hustle_damage} vs {control_damage}"
    );

    let mut hustle_rng = SequenceRng::new(ORDINARY_NON_CRITICAL_DRAWS);
    let resolution = resolve_hit(
        &dex,
        TACKLE,
        &hustle_attacker,
        &defender,
        false,
        &mut hustle_rng,
    )
    .unwrap();

    assert_eq!(
        resolution.outcome,
        HitOutcome::Hit {
            damage: expected_hustle_damage,
            is_critical: false,
        },
        "a Hustle attacker's physical damage must carry the 150% raw-Attack boost"
    );
}

#[test]
fn hustle_never_touches_a_special_move() {
    let dex = Dex::new();
    let defender = mon(&dex, SQUIRTLE, 10, vec![TACKLE]);
    let hustle_attacker = BattlePokemon::new(&dex, DELIBIRD, 10, MAX_IVS, 0, vec![WATER_GUN])
        .unwrap()
        .with_ability_slot(1);
    let control = BattlePokemon::new(&dex, DELIBIRD, 10, MAX_IVS, 0, vec![WATER_GUN])
        .unwrap()
        .with_ability_slot(0);
    assert_eq!(hustle_attacker.ability(), AbilityId::HUSTLE);
    assert_ne!(control.ability(), AbilityId::HUSTLE);

    let mut hustle_rng = SequenceRng::new(ORDINARY_NON_CRITICAL_DRAWS);
    let hustle_outcome = resolve_hit(
        &dex,
        WATER_GUN,
        &hustle_attacker,
        &defender,
        false,
        &mut hustle_rng,
    )
    .unwrap();
    let mut control_rng = SequenceRng::new(ORDINARY_NON_CRITICAL_DRAWS);
    let control_outcome = resolve_hit(
        &dex,
        WATER_GUN,
        &control,
        &defender,
        false,
        &mut control_rng,
    )
    .unwrap();

    assert_eq!(hustle_outcome.outcome, control_outcome.outcome);
}
