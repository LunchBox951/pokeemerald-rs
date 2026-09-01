use super::{
    drain_amount, ensure_resolvable, is_drain_effect, resolve_drain, resolve_drain_move,
    EFFECT_ABSORB,
};
use crate::ability::{suppresses_critical_hits, LIQUID_OOZE, OVERGROW};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::hit::HitOutcome;
use crate::pokemon::{BattlePokemon, Ivs};
use crate::script_rng::SequenceRng;
use crate::stat_stage::StatStage;
use assets::species::AbilityId;
use assets::{MoveId, SpeciesId};

const BULBASAUR: SpeciesId = SpeciesId(1);
const SQUIRTLE: SpeciesId = SpeciesId(7);
const CATERPIE: SpeciesId = SpeciesId(10);
const ANORITH: SpeciesId = SpeciesId(390);

const TACKLE: MoveId = MoveId(33);
const ABSORB: MoveId = MoveId(71);
const MEGA_DRAIN: MoveId = MoveId(72);
const GIGA_DRAIN: MoveId = MoveId(202);
const UNKNOWN_MOVE: MoveId = MoveId(60_000);

const STENCH: AbilityId = AbilityId(1);

const MAX_IVS: Ivs = Ivs {
    hp: 31,
    attack: 31,
    defense: 31,
    speed: 31,
    sp_attack: 31,
    sp_defense: 31,
};

const ACCURACY_HIT_DRAW: u16 = 0;
const MEGA_DRAIN_MISS_DRAW: u16 = 40;
const ORDINARY_NO_CRIT_DRAW: u16 = 1;
const BEST_DAMAGE_DRAW: u16 = 0;
const LANDED_DRAIN_DRAWS: [u16; 3] = [ACCURACY_HIT_DRAW, ORDINARY_NO_CRIT_DRAW, BEST_DAMAGE_DRAW];
const CRITICAL_SUPPRESSED_DRAWS: [u16; 2] = [ACCURACY_HIT_DRAW, BEST_DAMAGE_DRAW];

const ABSORB_DAMAGE_TO_SQUIRTLE: u32 = 8;
const OVERGROW_ABSORB_DAMAGE_TO_SQUIRTLE: u32 = 12;
const ABSORB_DAMAGE_TO_ANORITH: u32 = 4;
const ABSORB_DAMAGE_TO_CATERPIE: u32 = 3;
const BULBASAUR_MAX_HP: u32 = 21;
const OVERGROW_HP_THRESHOLD: u32 = 7;
const SQUIRTLE_MAX_HP: u32 = 20;

fn mon(dex: &Dex, species: SpeciesId, level: u8, moves: Vec<MoveId>) -> BattlePokemon {
    BattlePokemon::new(dex, species, level, MAX_IVS, 0, moves).unwrap()
}

#[test]
fn only_effect_absorb_is_a_drain_effect() {
    let dex = Dex::new();
    for move_id in [ABSORB, MEGA_DRAIN, GIGA_DRAIN] {
        assert_eq!(dex.move_data(move_id).unwrap().effect, EFFECT_ABSORB);
        assert!(is_drain_effect(dex.move_data(move_id).unwrap().effect));
        assert_eq!(ensure_resolvable(&dex, move_id), Ok(()));
    }
    assert!(!is_drain_effect(dex.move_data(TACKLE).unwrap().effect));
    assert_eq!(
        ensure_resolvable(&dex, TACKLE),
        Err(BattleError::UnsupportedMoveEffect(TACKLE))
    );
    assert_eq!(
        ensure_resolvable(&dex, UNKNOWN_MOVE),
        Err(BattleError::UnknownMove(UNKNOWN_MOVE))
    );
}

#[test]
fn drain_amount_halves_actual_hp_loss_then_floors() {
    assert_eq!(
        drain_amount(0),
        0,
        "a move that removed no HP drains nothing"
    );
    assert_eq!(drain_amount(1), 1, "the floor follows the halving");
    assert_eq!(drain_amount(2), 1);
    assert_eq!(drain_amount(3), 1, "integer division truncates");
    assert_eq!(drain_amount(8), 4);
    assert_eq!(drain_amount(999), 499);
}

#[test]
fn liquid_ooze_inverts_the_floored_drain_without_changing_its_magnitude() {
    assert_eq!(resolve_drain(0, LIQUID_OOZE), None);

    let healing = resolve_drain(9, OVERGROW).unwrap();
    let liquid_ooze_damage = resolve_drain(9, LIQUID_OOZE).unwrap();

    assert_eq!(healing.amount, 4);
    assert!(!healing.inverted);
    assert_eq!(liquid_ooze_damage.amount, healing.amount);
    assert!(liquid_ooze_damage.inverted);
    assert_eq!(resolve_drain(1, LIQUID_OOZE).unwrap().amount, 1);
    assert!(!resolve_drain(9, STENCH).unwrap().inverted);
}

#[test]
fn a_landed_drain_move_draws_accuracy_critical_and_damage() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 5, vec![ABSORB]);
    let defender = mon(&dex, SQUIRTLE, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new(LANDED_DRAIN_DRAWS);

    let outcome = resolve_drain_move(&dex, ABSORB, &attacker, &defender, false, &mut rng).unwrap();

    assert_eq!(
        outcome,
        HitOutcome::Hit {
            damage: ABSORB_DAMAGE_TO_SQUIRTLE,
            is_critical: false,
        }
    );
    assert_eq!(rng.draws(), LANDED_DRAIN_DRAWS.len());
}

#[test]
fn a_missed_drain_move_draws_only_for_accuracy() {
    let dex = Dex::new();
    let mut attacker = mon(&dex, BULBASAUR, 5, vec![MEGA_DRAIN]);
    attacker.stages_mut().accuracy = StatStage::MIN;
    let defender = mon(&dex, SQUIRTLE, 5, vec![TACKLE]);
    let miss_draws = [MEGA_DRAIN_MISS_DRAW];
    let mut rng = SequenceRng::new(miss_draws);

    let outcome =
        resolve_drain_move(&dex, MEGA_DRAIN, &attacker, &defender, false, &mut rng).unwrap();

    assert_eq!(outcome, HitOutcome::Miss);
    assert_eq!(rng.draws(), miss_draws.len());
}

#[test]
fn caller_critical_suppression_omits_the_critical_draw() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 5, vec![ABSORB]);
    let defender = mon(&dex, SQUIRTLE, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new(CRITICAL_SUPPRESSED_DRAWS);

    let outcome = resolve_drain_move(&dex, ABSORB, &attacker, &defender, true, &mut rng).unwrap();

    assert_eq!(
        outcome,
        HitOutcome::Hit {
            damage: ABSORB_DAMAGE_TO_SQUIRTLE,
            is_critical: false,
        }
    );
    assert_eq!(rng.draws(), CRITICAL_SUPPRESSED_DRAWS.len());
}

#[test]
fn battle_armor_omits_the_critical_draw() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 5, vec![ABSORB]);
    let defender = mon(&dex, ANORITH, 5, vec![TACKLE]);
    assert!(suppresses_critical_hits(defender.ability()));
    let mut rng = SequenceRng::new(CRITICAL_SUPPRESSED_DRAWS);

    let outcome = resolve_drain_move(&dex, ABSORB, &attacker, &defender, false, &mut rng).unwrap();

    assert_eq!(
        outcome,
        HitOutcome::Hit {
            damage: ABSORB_DAMAGE_TO_ANORITH,
            is_critical: false,
        }
    );
    assert_eq!(rng.draws(), CRITICAL_SUPPRESSED_DRAWS.len());
}

#[test]
fn overgrow_boosts_a_grass_drain_at_one_third_max_hp() {
    let dex = Dex::new();
    let defender = mon(&dex, SQUIRTLE, 5, vec![TACKLE]);

    let healthy = mon(&dex, BULBASAUR, 5, vec![ABSORB]);
    assert_eq!(healthy.ability(), OVERGROW);
    assert_eq!(healthy.stats().max_hp, BULBASAUR_MAX_HP);

    let mut at_threshold = healthy.clone();
    at_threshold.apply_damage(BULBASAUR_MAX_HP - OVERGROW_HP_THRESHOLD);
    assert_eq!(at_threshold.current_hp(), OVERGROW_HP_THRESHOLD);

    let mut rng = SequenceRng::new(LANDED_DRAIN_DRAWS);
    let unboosted = resolve_drain_move(&dex, ABSORB, &healthy, &defender, false, &mut rng).unwrap();
    let mut rng = SequenceRng::new(LANDED_DRAIN_DRAWS);
    let boosted =
        resolve_drain_move(&dex, ABSORB, &at_threshold, &defender, false, &mut rng).unwrap();

    assert_eq!(
        unboosted,
        HitOutcome::Hit {
            damage: ABSORB_DAMAGE_TO_SQUIRTLE,
            is_critical: false,
        }
    );
    assert_eq!(
        boosted,
        HitOutcome::Hit {
            damage: OVERGROW_ABSORB_DAMAGE_TO_SQUIRTLE,
            is_critical: false,
        }
    );

    let mut above_threshold = healthy.clone();
    above_threshold.apply_damage(BULBASAUR_MAX_HP - (OVERGROW_HP_THRESHOLD + 1));
    let mut rng = SequenceRng::new(LANDED_DRAIN_DRAWS);
    assert_eq!(
        resolve_drain_move(&dex, ABSORB, &above_threshold, &defender, false, &mut rng,).unwrap(),
        unboosted
    );
}

#[test]
fn a_resisted_drain_still_uses_all_three_draws() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 5, vec![ABSORB]);
    let defender = mon(&dex, CATERPIE, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new(LANDED_DRAIN_DRAWS);

    let outcome = resolve_drain_move(&dex, ABSORB, &attacker, &defender, false, &mut rng).unwrap();

    assert_eq!(
        outcome,
        HitOutcome::Hit {
            damage: ABSORB_DAMAGE_TO_CATERPIE,
            is_critical: false,
        }
    );
    assert_eq!(rng.draws(), LANDED_DRAIN_DRAWS.len());
}

#[test]
fn drain_amount_uses_hp_removed_instead_of_formula_damage() {
    let formula_damage = 999;
    let target_hp_remaining = 5;
    let target_hp_removed = formula_damage.min(target_hp_remaining);

    assert_eq!(drain_amount(target_hp_removed), 2);
    assert_eq!(drain_amount(formula_damage), 499);

    let dex = Dex::new();
    let squirtle_max_hp = mon(&dex, SQUIRTLE, 5, vec![TACKLE]).stats().max_hp;
    assert_eq!(squirtle_max_hp, SQUIRTLE_MAX_HP);
    assert!(3 * ABSORB_DAMAGE_TO_SQUIRTLE > squirtle_max_hp);
}

#[test]
fn a_rejected_move_draws_nothing() {
    let dex = Dex::new();
    let attacker = mon(&dex, BULBASAUR, 5, vec![ABSORB]);
    let defender = mon(&dex, SQUIRTLE, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new([]);

    assert_eq!(
        resolve_drain_move(&dex, TACKLE, &attacker, &defender, false, &mut rng),
        Err(BattleError::UnsupportedMoveEffect(TACKLE))
    );
    assert_eq!(rng.draws(), 0);
}
