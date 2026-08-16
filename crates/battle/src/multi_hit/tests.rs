//! [`crate::multi_hit`]'s own unit tests: the two-stage hit-count draw and
//! the once-per-move accuracy check.

use super::{
    ensure_resolvable, is_multi_hit_effect, resolve_multi_hit, roll_hit_count,
    spend_effect_chance_draw, EFFECT_MULTI_HIT, MAX_HITS, MIN_HITS,
};
use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::error::BattleError;
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

const DOUBLE_SLAP: MoveId = MoveId(3);
const ARM_THRUST: MoveId = MoveId(292);
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
fn both_multi_hit_moves_share_one_effect_id() {
    let dex = Dex::new();
    for move_id in [DOUBLE_SLAP, ARM_THRUST] {
        assert_eq!(dex.move_data(move_id).unwrap().effect, EFFECT_MULTI_HIT);
        assert_eq!(ensure_resolvable(&dex, move_id), Ok(()));
    }
    assert!(!is_multi_hit_effect(dex.move_data(TACKLE).unwrap().effect));
    assert_eq!(
        ensure_resolvable(&dex, TACKLE),
        Err(BattleError::UnsupportedMoveEffect(TACKLE))
    );
}

/// `Cmd_setmultihitcounter`'s branch, both arms, with the **draw counts**
/// that distinguish it from sampling the 3/8-3/8-1/8-1/8 distribution
/// directly: a first draw of 0 or 1 settles in one draw, 2 or 3 redraws.
#[test]
fn the_hit_count_roll_reproduces_the_two_stage_branch() {
    // First draw 0 -> 0 + 2 = 2, one draw.
    let mut rng = SequenceRng::new([0]);
    assert_eq!(roll_hit_count(&mut rng), 2);
    assert_eq!(rng.draws(), 1);

    // First draw 1 -> 1 + 2 = 3, one draw.
    let mut rng = SequenceRng::new([1]);
    assert_eq!(roll_hit_count(&mut rng), 3);
    assert_eq!(rng.draws(), 1);

    // First draw 2 -> redraw. Second draw 3 -> 3 + 2 = 5, two draws.
    let mut rng = SequenceRng::new([2, 3]);
    assert_eq!(roll_hit_count(&mut rng), 5);
    assert_eq!(rng.draws(), 2);

    // First draw 3 -> redraw. Second draw 0 -> 0 + 2 = 2, two draws --
    // the case a one-draw model gets *both* wrong: the value happens to
    // agree with `first = 0` but the stream position does not.
    let mut rng = SequenceRng::new([3, 0]);
    assert_eq!(roll_hit_count(&mut rng), 2);
    assert_eq!(rng.draws(), 2);
}

/// The mask is `& 3`, so only the low two bits matter -- a draw of `0xFFFF`
/// is `3`, not something out of range.
#[test]
fn the_hit_count_is_always_two_through_five_whatever_the_draw() {
    for first in [0u16, 1, 2, 3, 4, 17, 0x8000, 0xFFFF] {
        for second in [0u16, 1, 2, 3, 0xFFFF] {
            let mut rng = SequenceRng::new([first, second]);
            let hits = roll_hit_count(&mut rng);
            assert!(
                (MIN_HITS..=MAX_HITS).contains(&hits),
                "first={first} second={second} -> {hits}"
            );
            let expected_draws = if (first & 3) > 1 { 2 } else { 1 };
            assert_eq!(rng.draws(), expected_draws, "first={first}");
        }
    }
}

/// The accuracy check runs **once for the whole move**, so a missed
/// multi-hit costs exactly one draw and never reaches the hit-count roll.
#[test]
fn a_missed_multi_hit_costs_one_draw_and_rolls_no_hit_count() {
    let dex = Dex::new();
    let attacker = mon(&dex, 315, 15, vec![DOUBLE_SLAP]); // Skitty
    let defender = mon(&dex, 183, 15, vec![TACKLE]);
    // Double Slap is 85 accuracy: draw 85 -> roll 86 > 85 -> miss. Only one
    // value, so a stray hit-count draw would panic.
    let mut rng = SequenceRng::new([85]);
    assert_eq!(
        resolve_multi_hit(&dex, DOUBLE_SLAP, &attacker, &defender, &mut rng).unwrap(),
        None
    );
    assert_eq!(rng.draws(), 1);
}

#[test]
fn a_landed_multi_hit_rolls_the_count_right_after_the_one_accuracy_check() {
    let dex = Dex::new();
    let attacker = mon(&dex, 335, 15, vec![ARM_THRUST]); // Makuhita
    let defender = mon(&dex, 183, 15, vec![TACKLE]);
    // draw0: accuracy 0 -> hit (Arm Thrust is 100). draw1: 1 -> 3 hits.
    let mut rng = SequenceRng::new([0, 1]);
    assert_eq!(
        resolve_multi_hit(&dex, ARM_THRUST, &attacker, &defender, &mut rng).unwrap(),
        Some(3)
    );
    assert_eq!(rng.draws(), 2, "accuracy + a one-draw hit count");
}

/// The whole documented budget for a 3-hit Double Slap, assembled from this
/// module's two halves plus `crate::hit::damage_core`'s two draws per hit:
/// 1 accuracy + 1 count + 3 x 2 + 1 effect chance = 9.
#[test]
fn a_three_hit_sequence_costs_the_nine_draws_the_module_docs_claim() {
    let dex = Dex::new();
    let attacker = mon(&dex, 335, 15, vec![ARM_THRUST]);
    let defender = mon(&dex, 183, 50, vec![TACKLE]); // high level: survives

    let mut rng = SequenceRng::new([
        0, // accuracy -> hit
        1, // hit count -> 3, one draw
        1, 0, // hit 1: crit, damage roll
        1, 0, // hit 2
        1, 0, // hit 3
        0, // seteffectwithchance
    ]);
    let hits = resolve_multi_hit(&dex, ARM_THRUST, &attacker, &defender, &mut rng)
        .unwrap()
        .expect("Arm Thrust at 100 accuracy must land");
    assert_eq!(hits, 3);
    for _ in 0..hits {
        crate::hit::damage_core(&dex, ARM_THRUST, &attacker, &defender, false, &mut rng).unwrap();
    }
    spend_effect_chance_draw(&mut rng);
    assert_eq!(rng.draws(), 9);
}

/// `jumpifmovehadnoeffect` (`data/battle_scripts_1.s:623`) sits **between**
/// `typecalc` (`:622`) and `adjustnormaldamage` (`:624`), so a type-immune
/// iteration spends the crit draw and then jumps **past the damage roll** --
/// the only damaging script in the engine that skips it.
///
/// The sequence is exactly three long (accuracy, hit count, crit), so a
/// fourth draw would panic. Reproduced here at the level the loop actually
/// runs at, since `crate::battle::Battle` owns the loop itself.
#[test]
fn an_immune_iteration_spends_the_crit_draw_but_not_the_damage_roll() {
    let dex = Dex::new();
    // Double Slap is Normal; Gastly (92) is Ghost/Poison, so every hit is
    // `MOVE_RESULT_DOESNT_AFFECT_FOE`.
    let attacker = mon(&dex, 315, 30, vec![DOUBLE_SLAP]); // Skitty
    let ghost = mon(&dex, 92, 30, vec![TACKLE]);

    let mut rng = SequenceRng::new([0, 0, 1]);
    let hits = resolve_multi_hit(&dex, DOUBLE_SLAP, &attacker, &ghost, &mut rng)
        .unwrap()
        .expect("draw 0 is a hit against 85 accuracy");
    assert_eq!(hits, 2, "draw 0 -> 0 + 2");

    let (damage, _) =
        crate::hit::damage_before_roll(&dex, DOUBLE_SLAP, &attacker, &ghost, false, &mut rng)
            .unwrap();
    assert_eq!(
        damage, 0,
        "Normal into Ghost is MOVE_RESULT_DOESNT_AFFECT_FOE"
    );
    assert_eq!(
        rng.draws(),
        3,
        "accuracy + hit count + the iteration's crit roll, and NOT its \
         damage roll -- upstream jumps past adjustnormaldamage at :623"
    );

    // The control: against a target the move *can* hurt, the same iteration
    // reaches `damage_core` and does spend both.
    let marill = mon(&dex, 183, 30, vec![TACKLE]);
    let mut rng = SequenceRng::new([0, 0, 1, 0]);
    let _ = resolve_multi_hit(&dex, DOUBLE_SLAP, &attacker, &marill, &mut rng).unwrap();
    let _ =
        crate::hit::damage_core(&dex, DOUBLE_SLAP, &attacker, &marill, false, &mut rng).unwrap();
    assert_eq!(rng.draws(), 4, "crit *and* damage roll on a landing hit");
}
