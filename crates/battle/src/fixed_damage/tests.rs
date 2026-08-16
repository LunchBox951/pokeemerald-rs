//! [`crate::fixed_damage`]'s own unit tests: the flat damage, the four
//! things the script skips, the immunity it still honours, and the exact
//! two-draw budget.

use super::{
    ensure_resolvable, fixed_damage_for_effect, is_fixed_damage_effect, resolve_fixed_damage_move,
    EFFECT_DRAGON_RAGE, EFFECT_SONICBOOM,
};
use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::hit::HitOutcome;
use crate::pokemon::{BattlePokemon, Ivs};
use crate::stat_stage::StatStage;
use assets::{MoveId, SpeciesId};

struct SequenceRng {
    values: Vec<u16>,
    index: usize,
}
impl SequenceRng {
    fn new(values: impl IntoIterator<Item = u16>) -> Self {
        Self {
            values: values.into_iter().collect(),
            index: 0,
        }
    }
    fn draws(&self) -> usize {
        self.index
    }
}
impl BattleRng for SequenceRng {
    fn next_u16(&mut self) -> u16 {
        let v = self
            .values
            .get(self.index)
            .copied()
            .expect("SequenceRng exhausted");
        self.index += 1;
        v
    }
}

const SONIC_BOOM: MoveId = MoveId(49);
const DRAGON_RAGE: MoveId = MoveId(82);
const TACKLE: MoveId = MoveId(33);

const MAX_IVS: Ivs = Ivs {
    hp: 31,
    attack: 31,
    defense: 31,
    speed: 31,
    sp_attack: 31,
    sp_defense: 31,
};

fn mon(dex: &Dex, species: u16, level: u8, moves: Vec<MoveId>) -> BattlePokemon {
    BattlePokemon::new(dex, SpeciesId(species), level, MAX_IVS, 0, moves).unwrap()
}

#[test]
fn the_two_effect_ids_and_their_literals_match_the_real_move_table() {
    let dex = Dex::new();
    assert_eq!(dex.move_data(SONIC_BOOM).unwrap().effect, EFFECT_SONICBOOM);
    assert_eq!(
        dex.move_data(DRAGON_RAGE).unwrap().effect,
        EFFECT_DRAGON_RAGE
    );
    assert_eq!(fixed_damage_for_effect(EFFECT_SONICBOOM), Some(20));
    assert_eq!(fixed_damage_for_effect(EFFECT_DRAGON_RAGE), Some(40));
    assert!(!is_fixed_damage_effect(
        dex.move_data(TACKLE).unwrap().effect
    ));

    // Base power **1**, which is the whole reason `power == 0` is not a
    // sufficient filter for the ordinary pipeline (`crate::hit`'s docs).
    assert_eq!(dex.move_data(SONIC_BOOM).unwrap().power, 1);
    assert_eq!(dex.move_data(SONIC_BOOM).unwrap().accuracy, 90);
}

#[test]
fn ensure_resolvable_rejects_ordinary_moves_without_drawing() {
    let dex = Dex::new();
    assert_eq!(ensure_resolvable(&dex, SONIC_BOOM), Ok(()));
    assert_eq!(
        ensure_resolvable(&dex, TACKLE),
        Err(BattleError::UnsupportedMoveEffect(TACKLE))
    );
    let bad = MoveId(60_000);
    assert_eq!(
        ensure_resolvable(&dex, bad),
        Err(BattleError::UnknownMove(bad))
    );
}

/// The headline: exactly 20, exactly two draws, never a crit.
#[test]
fn a_landed_sonic_boom_deals_exactly_twenty_and_draws_twice() {
    let dex = Dex::new();
    let attacker = mon(&dex, 100, 15, vec![SONIC_BOOM]); // Voltorb
    let defender = mon(&dex, 183, 15, vec![TACKLE]); // Marill

    // draw0: accuracy 0 -> roll 1 <= 90 -> hit.
    // draw1: the discarded seteffectwithchance roll.
    // The sequence is exactly 2 long: a crit or damage-roll draw would
    // panic here, which is what pins their absence.
    let mut rng = SequenceRng::new([0, 0]);
    assert_eq!(
        resolve_fixed_damage_move(&dex, SONIC_BOOM, &attacker, &defender, &mut rng).unwrap(),
        HitOutcome::Hit {
            damage: 20,
            is_critical: false,
        }
    );
    assert_eq!(rng.draws(), 2, "accuracy + effect chance, and nothing else");
}

/// Nothing about either battler can move the number: level, stats, stages
/// and STAB all feed a `damagecalc`/`typecalc` result the `setword` throws
/// away one instruction later.
#[test]
fn the_damage_ignores_levels_stats_stages_and_stab() {
    let dex = Dex::new();
    // A level-100 attacker at +6 Attack into a level-1 defender at -6
    // Defense would obliterate anything through the ordinary formula.
    let mut attacker = mon(&dex, 100, 100, vec![SONIC_BOOM]);
    attacker.stages_mut().attack = StatStage::MAX;
    attacker.stages_mut().sp_attack = StatStage::MAX;
    let mut defender = mon(&dex, 183, 1, vec![TACKLE]);
    defender.stages_mut().defense = StatStage::MIN;
    defender.stages_mut().sp_defense = StatStage::MIN;

    let mut rng = SequenceRng::new([0, 0]);
    assert_eq!(
        resolve_fixed_damage_move(&dex, SONIC_BOOM, &attacker, &defender, &mut rng).unwrap(),
        HitOutcome::Hit {
            damage: 20,
            is_critical: false,
        }
    );

    // ...and the mirror: a hopeless attacker deals the same 20.
    let weak = mon(&dex, 129, 1, vec![SONIC_BOOM]); // level-1 Magikarp
    let mut rng = SequenceRng::new([0, 0]);
    assert_eq!(
        resolve_fixed_damage_move(&dex, SONIC_BOOM, &weak, &defender, &mut rng).unwrap(),
        HitOutcome::Hit {
            damage: 20,
            is_critical: false,
        }
    );
}

/// Type effectiveness is thrown away for magnitude but **not** for
/// immunity: `typecalc` still sets `MOVE_RESULT_DOESNT_AFFECT_FOE`, and the
/// `bicbyte` at `:1726` does not clear that bit.
#[test]
fn a_resisted_matchup_still_takes_twenty_but_a_ghost_takes_nothing() {
    let dex = Dex::new();
    let attacker = mon(&dex, 100, 15, vec![SONIC_BOOM]);

    // Normal into Rock (Geodude, 74) is x0.5 -- still exactly 20.
    let rock = mon(&dex, 74, 15, vec![TACKLE]);
    let mut rng = SequenceRng::new([0, 0]);
    assert_eq!(
        resolve_fixed_damage_move(&dex, SONIC_BOOM, &attacker, &rock, &mut rng).unwrap(),
        HitOutcome::Hit {
            damage: 20,
            is_critical: false,
        },
        "a resisted fixed-damage hit is not halved"
    );

    // Normal into Ghost (Gastly, 92) is x0 -- nothing at all, but the two
    // draws are still spent.
    let ghost = mon(&dex, 92, 15, vec![TACKLE]);
    let mut rng = SequenceRng::new([0, 0]);
    assert_eq!(
        resolve_fixed_damage_move(&dex, SONIC_BOOM, &attacker, &ghost, &mut rng).unwrap(),
        HitOutcome::NoEffect
    );
    assert_eq!(
        rng.draws(),
        2,
        "the immunity is found at typecalc, after the accuracy roll, and too late to \
         suppress the effect-chance draw"
    );
}

#[test]
fn a_missed_sonic_boom_costs_only_the_accuracy_draw() {
    let dex = Dex::new();
    let attacker = mon(&dex, 100, 15, vec![SONIC_BOOM]);
    let defender = mon(&dex, 183, 15, vec![TACKLE]);
    // Accuracy 90: draw 90 -> roll 91 > 90 -> miss. Only one value in the
    // sequence, so a stray effect-chance draw after a miss would panic.
    let mut rng = SequenceRng::new([90]);
    assert_eq!(
        resolve_fixed_damage_move(&dex, SONIC_BOOM, &attacker, &defender, &mut rng).unwrap(),
        HitOutcome::Miss
    );
    assert_eq!(rng.draws(), 1);
}

#[test]
fn dragon_rage_is_the_same_script_with_a_different_literal() {
    let dex = Dex::new();
    let attacker = mon(&dex, 395, 30, vec![DRAGON_RAGE]); // Bagon
    let defender = mon(&dex, 183, 30, vec![TACKLE]);
    let mut rng = SequenceRng::new([0, 0]);
    assert_eq!(
        resolve_fixed_damage_move(&dex, DRAGON_RAGE, &attacker, &defender, &mut rng).unwrap(),
        HitOutcome::Hit {
            damage: 40,
            is_critical: false,
        }
    );
    assert_eq!(rng.draws(), 2);
}
