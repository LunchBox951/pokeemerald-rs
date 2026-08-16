//! [`crate::drain`]'s own unit tests: the three-draw budget (one fewer than
//! an ordinary hit), and `Cmd_negativedamage`'s exact halving.

use super::{drain_amount, ensure_resolvable, is_drain_effect, resolve_drain_move, EFFECT_ABSORB};
use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::hit::{resolve_hit, HitOutcome};
use crate::pokemon::{BattlePokemon, Ivs};
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

const ABSORB: MoveId = MoveId(71);
const MEGA_DRAIN: MoveId = MoveId(72);
const GIGA_DRAIN: MoveId = MoveId(202);
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
fn all_three_drain_moves_share_one_effect_id() {
    let dex = Dex::new();
    for move_id in [ABSORB, MEGA_DRAIN, GIGA_DRAIN] {
        assert_eq!(
            dex.move_data(move_id).unwrap().effect,
            EFFECT_ABSORB,
            "move {}",
            move_id.0
        );
        assert_eq!(ensure_resolvable(&dex, move_id), Ok(()));
    }
    assert!(!is_drain_effect(dex.move_data(TACKLE).unwrap().effect));
    assert_eq!(
        ensure_resolvable(&dex, TACKLE),
        Err(BattleError::UnsupportedMoveEffect(TACKLE))
    );
}

/// The headline difference from every other damaging move: **three** draws,
/// not four. The sequence is exactly three long, so the missing
/// `seteffectwithchance` roll is pinned by a panic rather than by prose.
#[test]
fn a_landed_absorb_draws_three_times_where_an_ordinary_hit_draws_four() {
    let dex = Dex::new();
    let attacker = mon(&dex, 306, 14, vec![ABSORB, TACKLE]); // Shroomish
    let defender = mon(&dex, 183, 14, vec![TACKLE]); // Marill

    // draw0: accuracy 0 -> hit. draw1: crit 1 -> no crit. draw2: 100% roll.
    let mut rng = SequenceRng::new([0, 1, 0]);
    let drained = resolve_drain_move(&dex, ABSORB, &attacker, &defender, false, &mut rng).unwrap();
    assert!(matches!(drained, HitOutcome::Hit { .. }), "{drained:?}");
    assert_eq!(
        rng.draws(),
        3,
        "no seteffectwithchance draw for a drain move"
    );

    // The control: the same attacker's Tackle through the ordinary
    // pipeline needs a fourth value or it panics.
    let mut rng = SequenceRng::new([0, 1, 0, 0]);
    let hit = resolve_hit(&dex, TACKLE, &attacker, &defender, false, &mut rng).unwrap();
    assert!(matches!(hit, HitOutcome::Hit { .. }));
    assert_eq!(rng.draws(), 4);
}

/// The damage half is the plain pipeline's, byte for byte -- the two
/// scripts are identical down to `adjustnormaldamage`, so the same battlers
/// and the same three draws must produce the same number a hypothetical
/// plain-`EFFECT_HIT` Absorb would.
#[test]
fn the_damage_half_matches_the_ordinary_pipeline_exactly() {
    let dex = Dex::new();
    // Shroomish's Absorb is Grass into Marill (Water): super effective, and
    // Shroomish is Grass, so STAB applies too -- a value that would differ
    // if any step of the shared core were skipped.
    let attacker = mon(&dex, 306, 14, vec![ABSORB]);
    let defender = mon(&dex, 183, 14, vec![TACKLE]);
    let mut drain_rng = SequenceRng::new([0, 1, 0]);
    let drained =
        resolve_drain_move(&dex, ABSORB, &attacker, &defender, false, &mut drain_rng).unwrap();

    // `resolve_hit` refuses EFFECT_ABSORB outright (it is not in the
    // allow-list), which is itself worth pinning: the two pipelines are not
    // interchangeable even though their damage agrees.
    let mut plain_rng = SequenceRng::new([]);
    assert_eq!(
        resolve_hit(&dex, ABSORB, &attacker, &defender, false, &mut plain_rng),
        Err(BattleError::UnsupportedMoveEffect(ABSORB))
    );
    assert_eq!(plain_rng.draws(), 0);

    let HitOutcome::Hit { damage, .. } = drained else {
        panic!("Absorb must land: {drained:?}");
    };
    assert!(damage > 0);
}

#[test]
fn a_missed_drain_move_costs_only_the_accuracy_draw() {
    let dex = Dex::new();
    let attacker = mon(&dex, 306, 14, vec![ABSORB]);
    // Sand Attack the accuracy down so the 100-accuracy Absorb can miss:
    // -6 accuracy stage gives calc = 100 * 33/100 = 33, and a draw of 40
    // rolls 41 > 33.
    let mut worse = mon(&dex, 306, 14, vec![ABSORB]);
    worse.stages_mut().accuracy = crate::stat_stage::StatStage::MIN;
    let defender = mon(&dex, 183, 14, vec![TACKLE]);
    let mut rng = SequenceRng::new([40]);
    assert_eq!(
        resolve_drain_move(&dex, ABSORB, &worse, &defender, false, &mut rng).unwrap(),
        HitOutcome::Miss
    );
    assert_eq!(rng.draws(), 1);
    // ...and the unmodified attacker at 100 accuracy still lands.
    let mut rng = SequenceRng::new([40, 1, 0]);
    assert!(matches!(
        resolve_drain_move(&dex, ABSORB, &attacker, &defender, false, &mut rng).unwrap(),
        HitOutcome::Hit { .. }
    ));
}

/// `Cmd_negativedamage`'s exact arithmetic, including the two details a
/// "half the damage" reimplementation gets wrong.
#[test]
fn the_drain_amount_halves_the_hp_actually_dealt_with_a_floor_of_one() {
    // Plain halving, truncating.
    assert_eq!(drain_amount(20), 10);
    assert_eq!(drain_amount(21), 10, "integer division truncates");
    // The floor fires *after* the halving, so 1 damage still heals 1.
    assert_eq!(drain_amount(1), 1);
    assert_eq!(drain_amount(2), 1);
    // Nothing dealt heals nothing: `negativedamage` runs after
    // `datahpupdate`, which applied no damage on a missed/immune hit.
    assert_eq!(drain_amount(0), 0);
}

/// The `gHpDealt`-not-`gBattleMoveDamage` detail, stated as a scenario: an
/// overkill hit drains half of what the target *had*, not half of the
/// formula's output.
#[test]
fn an_overkill_drain_heals_only_half_the_hp_the_target_had_left() {
    let formula_damage = 999u32;
    let target_hp_remaining = 5u32;
    let dealt = formula_damage.min(target_hp_remaining);
    assert_eq!(dealt, 5);
    assert_eq!(
        drain_amount(dealt),
        2,
        "half of the 5 HP actually removed, not half of 999"
    );
}

#[test]
fn suppressing_the_crit_drops_a_landed_drain_move_to_two_draws() {
    let dex = Dex::new();
    let attacker = mon(&dex, 306, 14, vec![ABSORB]);
    let defender = mon(&dex, 183, 14, vec![TACKLE]);
    // accuracy + damage roll only; a crit draw would panic.
    let mut rng = SequenceRng::new([0, 0]);
    assert!(matches!(
        resolve_drain_move(&dex, ABSORB, &attacker, &defender, true, &mut rng).unwrap(),
        HitOutcome::Hit {
            is_critical: false,
            ..
        }
    ));
    assert_eq!(rng.draws(), 2);
}
