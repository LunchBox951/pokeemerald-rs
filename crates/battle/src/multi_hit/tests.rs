use super::{
    ensure_resolvable, is_multi_hit_effect, resolve_multi_hit, roll_hit_count,
    spend_multi_hit_effect_chance_draw, EFFECT_MULTI_HIT, MAX_HITS, MIN_HITS,
};
use crate::ability::suppresses_critical_hits;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::hit::{damage_core, HitOutcome};
use crate::pokemon::{BattlePokemon, Ivs};
use crate::script_rng::SequenceRng;
use assets::{MoveId, SpeciesId};

/// `MOVE_DOUBLE_SLAP` (power 15, accuracy 85, Normal).
const DOUBLE_SLAP: MoveId = MoveId(3);
/// `MOVE_FURY_ATTACK`, the same effect on a different move.
const FURY_ATTACK: MoveId = MoveId(31);
/// `MOVE_TACKLE`, the plain-hit control.
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
fn only_effect_multi_hit_is_accepted() {
    let dex = Dex::new();
    for move_id in [DOUBLE_SLAP, FURY_ATTACK] {
        assert_eq!(dex.move_data(move_id).unwrap().effect, EFFECT_MULTI_HIT);
        assert!(is_multi_hit_effect(dex.move_data(move_id).unwrap().effect));
        assert_eq!(ensure_resolvable(&dex, move_id), Ok(()));
    }
    assert_eq!(
        ensure_resolvable(&dex, TACKLE),
        Err(BattleError::UnsupportedMoveEffect(TACKLE))
    );
}

/// `Cmd_setmultihitcounter`'s branch, draw for draw:
///
/// ```text
/// gMultiHitCounter = Random() & 3;
/// if (gMultiHitCounter > 1) gMultiHitCounter = (Random() & 3) + 2;
/// else                      gMultiHitCounter += 2;
/// ```
///
/// A first mask of `0`/`1` settles at 2/3 for **one** draw; `2`/`3` redraws
/// and adds 2 for **two**. Sampling the 3/8-3/8-1/8-1/8 distribution
/// directly would produce the same counts with the wrong draw spend half the
/// time, which is exactly the defect this test exists to catch.
#[test]
fn the_hit_count_reproduces_the_two_stage_draw_shape() {
    // One-draw branch: first mask 0 -> 2 hits, 1 -> 3 hits.
    for (first, hits) in [(0u16, 2u8), (1, 3)] {
        let mut rng = SequenceRng::new([first]);
        assert_eq!(roll_hit_count(&mut rng), hits);
        assert_eq!(rng.draws(), 1, "mask {first} must not redraw");
    }
    // Two-draw branch: first mask 2 or 3 discards its own value entirely and
    // the *second* mask decides, `+2`.
    for first in [2u16, 3] {
        for (second, hits) in [(0u16, 2u8), (1, 3), (2, 4), (3, 5)] {
            let mut rng = SequenceRng::new([first, second]);
            assert_eq!(roll_hit_count(&mut rng), hits, "{first} then {second}");
            assert_eq!(rng.draws(), 2);
        }
    }
    // The mask is `& 3`, so only the low two bits of the u16 matter -- a
    // large draw is not a large hit count.
    let mut rng = SequenceRng::new([0xFFFC]);
    assert_eq!(roll_hit_count(&mut rng), 2);
    assert_eq!(rng.draws(), 1);
}

/// Every reachable outcome is within `MIN_HITS..=MAX_HITS`, and the branch
/// really produces upstream's 3/8, 3/8, 1/8, 1/8 shape over the 16 equally
/// likely `(first, second)` low-bit pairs.
#[test]
fn the_hit_count_distribution_is_upstreams() {
    let mut counts = [0u32; 6];
    for first in 0u16..4 {
        for second in 0u16..4 {
            let mut rng = SequenceRng::new([first, second]);
            let hits = roll_hit_count(&mut rng);
            assert!((MIN_HITS..=MAX_HITS).contains(&hits));
            // A one-draw branch ignores `second`, so it is counted once per
            // `second` -- which is exactly the weighting the two-stage draw
            // gives it.
            counts[hits as usize] += 1;
        }
    }
    // 16 equally likely pairs: 2 and 3 land on 6/16 = 3/8 each, 4 and 5 on
    // 2/16 = 1/8 each.
    assert_eq!(counts[2], 6);
    assert_eq!(counts[3], 6);
    assert_eq!(counts[4], 2);
    assert_eq!(counts[5], 2);
}

/// The prologue: one accuracy check for the whole move, then the count roll.
/// A miss ends it at **1** draw; a landing costs **2 or 3**.
#[test]
fn the_prologue_costs_one_draw_on_a_miss_and_two_or_three_otherwise() {
    let dex = Dex::new();
    let attacker = mon(&dex, 1, 5, vec![DOUBLE_SLAP]);
    let defender = mon(&dex, 7, 5, vec![TACKLE]);

    // Double Slap's accuracy is 85: roll = draw % 100 + 1; 85 -> 86 > 85.
    let mut rng = SequenceRng::new([85]);
    assert_eq!(
        resolve_multi_hit(&dex, DOUBLE_SLAP, &attacker, &defender, &mut rng).unwrap(),
        None
    );
    assert_eq!(
        rng.draws(),
        1,
        "a multi-hit move misses all its hits at once"
    );

    // Landing, one-draw count branch: accuracy 0, mask 0 -> 2 hits.
    let mut rng = SequenceRng::new([0, 0]);
    assert_eq!(
        resolve_multi_hit(&dex, DOUBLE_SLAP, &attacker, &defender, &mut rng).unwrap(),
        Some(2)
    );
    assert_eq!(rng.draws(), 2);

    // Landing, two-draw count branch: accuracy 0, mask 3 then mask 3 -> 5.
    let mut rng = SequenceRng::new([0, 3, 3]);
    assert_eq!(
        resolve_multi_hit(&dex, DOUBLE_SLAP, &attacker, &defender, &mut rng).unwrap(),
        Some(5)
    );
    assert_eq!(rng.draws(), 3);
}

/// The trailing `seteffectwithchance` runs **once per move**, not per hit,
/// and is discarded for every `EFFECT_MULTI_HIT` move (none carries a
/// secondary-effect chance).
#[test]
fn the_trailing_effect_chance_draw_is_one_per_move_and_is_discarded() {
    let dex = Dex::new();
    assert_eq!(
        dex.move_data(DOUBLE_SLAP).unwrap().secondary_effect_chance,
        0
    );
    for had_effect in [true, false] {
        for value in [0u16, 99, u16::MAX] {
            let mut rng = SequenceRng::new([value]);
            assert_eq!(
                spend_multi_hit_effect_chance_draw(&dex, DOUBLE_SLAP, had_effect, &mut rng),
                Ok(())
            );
            assert_eq!(rng.draws(), 1);
        }
    }
}

#[test]
fn a_rejected_move_draws_nothing() {
    let dex = Dex::new();
    let attacker = mon(&dex, 1, 5, vec![DOUBLE_SLAP]);
    let defender = mon(&dex, 7, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new([]);
    assert_eq!(
        resolve_multi_hit(&dex, TACKLE, &attacker, &defender, &mut rng),
        Err(BattleError::UnsupportedMoveEffect(TACKLE))
    );
    assert_eq!(rng.draws(), 0);
}

/// The module docs say the per-hit loop lives in the caller, running
/// [`crate::hit::damage_core`] once per landed hit. Battle Armor/Shell
/// Armor's crit suppression (issue #391) lives inside that function, so a
/// Battle Armor defender must drop **every processed hit** by one draw, not
/// just the move as a whole -- this drives the loop the way the turn engine
/// would and pins both shapes side by side.
#[test]
fn a_battle_armor_defender_drops_every_processed_hit_by_one_draw() {
    let dex = Dex::new();
    let attacker = mon(&dex, 1, 5, vec![DOUBLE_SLAP]);

    // Control: an ordinary defender costs 2 draws per hit (crit + damage
    // roll) -- `crate::hit`'s plain shape.
    let plain = mon(&dex, 7, 5, vec![TACKLE]); // Squirtle
                                               // accuracy hit, mask 0 -> 2 hits, then 2x(crit, damage roll).
    let mut rng = SequenceRng::new([0, 0, 1, 0, 1, 0]);
    let hits = resolve_multi_hit(&dex, DOUBLE_SLAP, &attacker, &plain, &mut rng)
        .unwrap()
        .expect("accuracy roll 0 must land");
    assert_eq!(hits, 2);
    for _ in 0..hits {
        damage_core(&dex, DOUBLE_SLAP, &attacker, &plain, false, &mut rng).unwrap();
    }
    assert_eq!(
        rng.draws(),
        2 + 2 * usize::from(hits),
        "plain: 2 draws per hit"
    );

    // Battle Armor: the identical shape costs 1 draw per hit instead of 2,
    // and every hit reports non-critical. The sequence has exactly
    // `2 + hits` values, so a stray crit draw would run it out and panic.
    let armored = mon(&dex, 390, 5, vec![TACKLE]); // Anorith, Battle Armor
    assert!(suppresses_critical_hits(armored.ability()));
    let mut rng = SequenceRng::new([0, 0, 0, 0]); // accuracy, hit count, 2x damage roll
    let hits = resolve_multi_hit(&dex, DOUBLE_SLAP, &attacker, &armored, &mut rng)
        .unwrap()
        .expect("accuracy roll 0 must land");
    assert_eq!(hits, 2);
    for _ in 0..hits {
        let outcome = damage_core(&dex, DOUBLE_SLAP, &attacker, &armored, false, &mut rng).unwrap();
        assert!(
            matches!(
                outcome,
                HitOutcome::Hit {
                    is_critical: false,
                    ..
                }
            ),
            "{outcome:?}"
        );
    }
    assert_eq!(
        rng.draws(),
        2 + usize::from(hits),
        "armored: one draw per hit, not two"
    );
}
