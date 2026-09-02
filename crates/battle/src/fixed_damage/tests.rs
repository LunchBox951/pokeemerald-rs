use super::{
    ensure_resolvable, fixed_damage_for_effect, is_fixed_damage_effect, resolve_fixed_damage_move,
    FixedDamage, DRAGON_RAGE_DAMAGE, EFFECT_DRAGON_RAGE, EFFECT_LEVEL_DAMAGE, EFFECT_SONICBOOM,
    FIXED_DAMAGE_EFFECTS, SONIC_BOOM_DAMAGE,
};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::hit::HitOutcome;
use crate::pokemon::{BattlePokemon, Ivs};
use crate::script_rng::SequenceRng;
use assets::{MoveId, SpeciesId};

const BULBASAUR: SpeciesId = SpeciesId(1);
const SQUIRTLE: SpeciesId = SpeciesId(7);
const RATTATA: SpeciesId = SpeciesId(19);
const GASTLY: SpeciesId = SpeciesId(92);

const TACKLE: MoveId = MoveId(33);
const SONIC_BOOM: MoveId = MoveId(49);
const SEISMIC_TOSS: MoveId = MoveId(69);
const DRAGON_RAGE: MoveId = MoveId(82);
const NIGHT_SHADE: MoveId = MoveId(101);

const HARDY_PERSONALITY: u32 = 0;
const ACCURACY_HIT_DRAW: u16 = 0;
const SONIC_BOOM_MISS_DRAW: u16 = 90;
const SONIC_BOOM_LAST_HIT_DRAW: u16 = 89;
const DISCARDED_EFFECT_CHANCE_DRAW: u16 = u16::MAX;
const LANDED_FIXED_DAMAGE_DRAWS: [u16; 2] = [ACCURACY_HIT_DRAW, DISCARDED_EFFECT_CHANCE_DRAW];

const MAX_IVS: Ivs = Ivs {
    hp: 31,
    attack: 31,
    defense: 31,
    speed: 31,
    sp_attack: 31,
    sp_defense: 31,
};

fn mon(dex: &Dex, species: SpeciesId, level: u8, moves: Vec<MoveId>) -> BattlePokemon {
    BattlePokemon::new(dex, species, level, MAX_IVS, HARDY_PERSONALITY, moves).unwrap()
}

#[test]
fn four_moves_map_to_the_three_supported_effects() {
    let dex = Dex::new();
    for (move_id, effect) in [
        (SONIC_BOOM, EFFECT_SONICBOOM),
        (DRAGON_RAGE, EFFECT_DRAGON_RAGE),
        (SEISMIC_TOSS, EFFECT_LEVEL_DAMAGE),
        (NIGHT_SHADE, EFFECT_LEVEL_DAMAGE),
    ] {
        let move_data = dex.move_data(move_id).unwrap();
        assert_eq!(move_data.effect, effect);
        assert_eq!(move_data.power, 1);
        assert!(is_fixed_damage_effect(effect));
        assert_eq!(ensure_resolvable(&dex, move_id), Ok(()));
    }

    assert_eq!(
        ensure_resolvable(&dex, TACKLE),
        Err(BattleError::UnsupportedMoveEffect(TACKLE))
    );
}

#[test]
fn effects_select_literal_or_attacker_level_damage() {
    assert_eq!(
        FIXED_DAMAGE_EFFECTS,
        [
            (EFFECT_DRAGON_RAGE, FixedDamage::Literal(DRAGON_RAGE_DAMAGE)),
            (EFFECT_LEVEL_DAMAGE, FixedDamage::AttackerLevel),
            (EFFECT_SONICBOOM, FixedDamage::Literal(SONIC_BOOM_DAMAGE)),
        ]
    );
    assert_eq!(
        fixed_damage_for_effect(EFFECT_SONICBOOM),
        Some(FixedDamage::Literal(SONIC_BOOM_DAMAGE))
    );
    assert_eq!(
        fixed_damage_for_effect(EFFECT_DRAGON_RAGE),
        Some(FixedDamage::Literal(DRAGON_RAGE_DAMAGE))
    );
    assert_eq!(
        fixed_damage_for_effect(EFFECT_LEVEL_DAMAGE),
        Some(FixedDamage::AttackerLevel)
    );
    assert_eq!(fixed_damage_for_effect(crate::drain::EFFECT_ABSORB), None);

    let literal_damage = 17;
    let attacker_level = 77;
    assert_eq!(
        FixedDamage::Literal(literal_damage).amount(attacker_level),
        literal_damage
    );
    assert_eq!(
        FixedDamage::AttackerLevel.amount(attacker_level),
        u32::from(attacker_level)
    );
    assert_eq!(FixedDamage::AttackerLevel.amount(1), 1);
}

#[test]
fn literal_damage_ignores_attacker_and_defender_stats() {
    let dex = Dex::new();
    let weak_attacker = mon(&dex, BULBASAUR, 5, vec![SONIC_BOOM]);
    let strong_attacker = mon(&dex, BULBASAUR, 100, vec![SONIC_BOOM]);
    let weak_defender = mon(&dex, SQUIRTLE, 5, vec![TACKLE]);
    let strong_defender = mon(&dex, SQUIRTLE, 100, vec![TACKLE]);

    for (attacker, defender) in [
        (&weak_attacker, &weak_defender),
        (&strong_attacker, &strong_defender),
        (&weak_attacker, &strong_defender),
    ] {
        let mut rng = SequenceRng::new(LANDED_FIXED_DAMAGE_DRAWS);
        let outcome =
            resolve_fixed_damage_move(&dex, SONIC_BOOM, attacker, defender, &mut rng).unwrap();

        assert_eq!(
            outcome,
            HitOutcome::Hit {
                damage: SONIC_BOOM_DAMAGE,
                is_critical: false,
            }
        );
        assert_eq!(rng.draws(), LANDED_FIXED_DAMAGE_DRAWS.len());
    }
}

#[test]
fn dragon_rage_uses_its_literal_and_level_damage_uses_the_attacker_level() {
    let dex = Dex::new();
    let attacker_level = 23;
    let attacker = mon(
        &dex,
        BULBASAUR,
        attacker_level,
        vec![DRAGON_RAGE, SEISMIC_TOSS],
    );
    let defender = mon(&dex, SQUIRTLE, 50, vec![TACKLE]);

    let mut dragon_rage_rng = SequenceRng::new(LANDED_FIXED_DAMAGE_DRAWS);
    let dragon_rage = resolve_fixed_damage_move(
        &dex,
        DRAGON_RAGE,
        &attacker,
        &defender,
        &mut dragon_rage_rng,
    )
    .unwrap();
    assert_eq!(
        dragon_rage,
        HitOutcome::Hit {
            damage: DRAGON_RAGE_DAMAGE,
            is_critical: false,
        }
    );
    assert_eq!(dragon_rage_rng.draws(), LANDED_FIXED_DAMAGE_DRAWS.len());

    let mut seismic_toss_rng = SequenceRng::new(LANDED_FIXED_DAMAGE_DRAWS);
    let seismic_toss = resolve_fixed_damage_move(
        &dex,
        SEISMIC_TOSS,
        &attacker,
        &defender,
        &mut seismic_toss_rng,
    )
    .unwrap();
    assert_eq!(
        seismic_toss,
        HitOutcome::Hit {
            damage: u32::from(attacker_level),
            is_critical: false,
        }
    );
    assert_eq!(seismic_toss_rng.draws(), LANDED_FIXED_DAMAGE_DRAWS.len());
}

#[test]
fn type_effectiveness_only_decides_immunity() {
    let dex = Dex::new();
    let attacker_level = 30;
    let attacker = mon(
        &dex,
        BULBASAUR,
        attacker_level,
        vec![SONIC_BOOM, NIGHT_SHADE],
    );
    let ghost_defender = mon(&dex, GASTLY, 30, vec![TACKLE]);
    let normal_defender = mon(&dex, RATTATA, 30, vec![TACKLE]);

    let mut sonic_boom_rng = SequenceRng::new(LANDED_FIXED_DAMAGE_DRAWS);
    let sonic_boom = resolve_fixed_damage_move(
        &dex,
        SONIC_BOOM,
        &attacker,
        &ghost_defender,
        &mut sonic_boom_rng,
    )
    .unwrap();
    assert_eq!(sonic_boom, HitOutcome::NoEffect);
    assert_eq!(sonic_boom_rng.draws(), LANDED_FIXED_DAMAGE_DRAWS.len());

    let mut immune_night_shade_rng = SequenceRng::new(LANDED_FIXED_DAMAGE_DRAWS);
    let immune_night_shade = resolve_fixed_damage_move(
        &dex,
        NIGHT_SHADE,
        &attacker,
        &normal_defender,
        &mut immune_night_shade_rng,
    )
    .unwrap();
    assert_eq!(immune_night_shade, HitOutcome::NoEffect);
    assert_eq!(
        immune_night_shade_rng.draws(),
        LANDED_FIXED_DAMAGE_DRAWS.len()
    );

    let mut super_effective_night_shade_rng = SequenceRng::new(LANDED_FIXED_DAMAGE_DRAWS);
    let super_effective_night_shade = resolve_fixed_damage_move(
        &dex,
        NIGHT_SHADE,
        &attacker,
        &ghost_defender,
        &mut super_effective_night_shade_rng,
    )
    .unwrap();
    assert_eq!(
        super_effective_night_shade,
        HitOutcome::Hit {
            damage: u32::from(attacker_level),
            is_critical: false,
        }
    );
    assert_eq!(
        super_effective_night_shade_rng.draws(),
        LANDED_FIXED_DAMAGE_DRAWS.len()
    );
}

#[test]
fn a_miss_consumes_only_the_accuracy_draw() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 5, vec![SONIC_BOOM]);
    let defender = mon(&dex, SQUIRTLE, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new([SONIC_BOOM_MISS_DRAW]);

    let outcome =
        resolve_fixed_damage_move(&dex, SONIC_BOOM, &attacker, &defender, &mut rng).unwrap();

    assert_eq!(outcome, HitOutcome::Miss);
    assert_eq!(rng.draws(), 1);
}

#[test]
fn sonic_booms_accuracy_boundary_is_inclusive() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 5, vec![SONIC_BOOM]);
    let defender = mon(&dex, SQUIRTLE, 5, vec![TACKLE]);
    let draws = [SONIC_BOOM_LAST_HIT_DRAW, DISCARDED_EFFECT_CHANCE_DRAW];
    let mut rng = SequenceRng::new(draws);

    let outcome =
        resolve_fixed_damage_move(&dex, SONIC_BOOM, &attacker, &defender, &mut rng).unwrap();

    assert!(matches!(outcome, HitOutcome::Hit { .. }));
    assert_eq!(rng.draws(), draws.len());
}

#[test]
fn a_rejected_move_draws_nothing() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 5, vec![SONIC_BOOM]);
    let defender = mon(&dex, SQUIRTLE, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new([]);

    assert_eq!(
        resolve_fixed_damage_move(&dex, TACKLE, &attacker, &defender, &mut rng),
        Err(BattleError::UnsupportedMoveEffect(TACKLE))
    );
    assert_eq!(rng.draws(), 0);
}
