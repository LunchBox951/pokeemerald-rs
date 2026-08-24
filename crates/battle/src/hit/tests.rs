use super::{ensure_resolvable, is_ordinary_hit_effect, resolve_hit, HitOutcome};
use crate::ability::{suppresses_critical_hits, HUGE_POWER, PURE_POWER};
use crate::accuracy::always_hits;
use crate::damage::{BattleRng, STRUGGLE};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::{BattlePokemon, Ivs};
use crate::stat_stage::StatStage;
use assets::species::AbilityId;
use assets::{MoveId, SpeciesId};

/// A `BattleRng` fed from a fixed sequence, for pinning exact draw
/// order/count in a multi-draw pipeline.
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

/// Max Gen-3 individual values (per-stat rolls, `MAX_IV_MASK` = 31 --
/// *not* a cryptographic initialization vector; see [`Ivs`]).
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
fn a_miss_draws_exactly_once() {
    let dex = Dex::new();
    let attacker = mon(&dex, 1, 5, vec![MoveId(33)]); // Bulbasaur/Tackle
    let defender = mon(&dex, 7, 5, vec![MoveId(33)]); // Squirtle
                                                      // Tackle accuracy 95: roll = draw%100+1. draw=95 -> roll=96 > 95 -> miss.
    let mut rng = SequenceRng::new([95]);
    let outcome = resolve_hit(&dex, MoveId(33), &attacker, &defender, false, &mut rng).unwrap();
    assert_eq!(outcome, HitOutcome::Miss);
    assert_eq!(rng.draws(), 1);
}

#[test]
fn a_hit_draws_exactly_four_times() {
    let dex = Dex::new();
    let attacker = mon(&dex, 1, 5, vec![MoveId(33)]);
    let defender = mon(&dex, 7, 5, vec![MoveId(33)]);
    // draw0: accuracy roll 0 -> roll=1 <= 95 -> hit.
    // draw1: crit roll 1 -> 1%16 != 0 -> no crit.
    // draw2: damage roll 0 -> best (100%) roll.
    // draw3: seteffectwithchance's discarded effect-chance roll.
    let mut rng = SequenceRng::new([0, 1, 0, 0]);
    let outcome = resolve_hit(&dex, MoveId(33), &attacker, &defender, false, &mut rng).unwrap();
    assert!(matches!(outcome, HitOutcome::Hit { .. }));
    assert_eq!(rng.draws(), 4);
}

#[test]
fn the_effect_chance_draw_value_never_changes_the_outcome() {
    // The step-7 draw is consumed and discarded: for a plain EFFECT_HIT
    // move `MOVE_EFFECT_BYTE` is 0 upstream, so no value the roll takes
    // can fire an effect. Pin that the extreme values leave the outcome
    // identical (only the stream position moves).
    let dex = Dex::new();
    let attacker = mon(&dex, 1, 5, vec![MoveId(33)]);
    let defender = mon(&dex, 7, 5, vec![MoveId(33)]);
    let mut outcomes = Vec::new();
    for effect_roll in [0u16, 99, u16::MAX] {
        let mut rng = SequenceRng::new([0, 1, 0, effect_roll]);
        outcomes
            .push(resolve_hit(&dex, MoveId(33), &attacker, &defender, false, &mut rng).unwrap());
        assert_eq!(rng.draws(), 4);
    }
    assert_eq!(outcomes[0], outcomes[1]);
    assert_eq!(outcomes[1], outcomes[2]);
}

/// The reference scenario both damage pins below are derived from, hand
/// computed from upstream's arithmetic rather than from this crate's own
/// output:
///
/// Bulbasaur (species 1) level 5, max IVs, Hardy, using Tackle (move 33,
/// power 35, accuracy 95, Normal/physical) against Squirtle (species 7)
/// level 5, max IVs, Hardy.
///
/// - attack  = `CALC_STAT(base 49, iv 31, lvl 5)` = `(2*49+31)*5/100 + 5`
///   = `645/100 = 6`, `+5` = **11** (Hardy is neutral, no scaling).
/// - defense = `CALC_STAT(base 65, iv 31, lvl 5)` = `(2*65+31)*5/100 + 5`
///   = `805/100 = 8`, `+5` = **13**.
/// - `CalculateBaseDamage`: `11 * 35` = 385; `* (2*5/5 + 2 = 4)` = 1540;
///   `/ 13` = 118 (`13*118 = 1534`); `/ 50` = 2; `+ 2` = **4**.
/// - Bulbasaur is Grass/Poison, so a Normal move gets no STAB; Squirtle
///   is pure Water, so Normal is neutral against it — both steps are
///   identity multiplies.
/// - A best-case damage roll (`draw % 16 == 0` -> 100%) leaves it at 4.
///
/// A crit doubles `CalculateBaseDamage`'s result *before* STAB
/// (`Cmd_damagecalc`, `battle_script_commands.c:1296`), and every stat
/// stage here is neutral so the crit stage override is a no-op: 4 -> 8.
const PINNED_NON_CRIT_DAMAGE: u32 = 4;
const PINNED_CRIT_DAMAGE: u32 = 8;

#[test]
fn best_roll_non_critical_damage_matches_the_hand_computed_pin() {
    let dex = Dex::new();
    let attacker = mon(&dex, 1, 5, vec![MoveId(33)]); // Bulbasaur/Tackle
    let defender = mon(&dex, 7, 5, vec![MoveId(33)]); // Squirtle

    // draw0: accuracy roll 1 <= 95 -> hit.
    // draw1: crit roll 1 -> 1%16 != 0 -> no crit.
    // draw2: damage roll 0 -> 100%.
    // draw3: the discarded effect-chance roll.
    let mut rng = SequenceRng::new([0, 1, 0, 0]);
    let outcome = resolve_hit(&dex, MoveId(33), &attacker, &defender, false, &mut rng).unwrap();
    assert_eq!(
        outcome,
        HitOutcome::Hit {
            damage: PINNED_NON_CRIT_DAMAGE,
            is_critical: false,
        }
    );
    assert_eq!(rng.draws(), 4);
}

#[test]
fn a_confirmed_crit_doubles_the_pinned_damage_and_is_reported() {
    let dex = Dex::new();
    let attacker = mon(&dex, 1, 5, vec![MoveId(33)]);
    let defender = mon(&dex, 7, 5, vec![MoveId(33)]);
    // Same scenario and same draws as the test above, except draw1: crit
    // roll 0 -> 0%16 == 0 -> crit.
    let mut rng = SequenceRng::new([0, 0, 0, 0]);
    let outcome = resolve_hit(&dex, MoveId(33), &attacker, &defender, false, &mut rng).unwrap();
    assert_eq!(
        outcome,
        HitOutcome::Hit {
            damage: PINNED_CRIT_DAMAGE,
            is_critical: true,
        }
    );
    assert_eq!(PINNED_CRIT_DAMAGE, 2 * PINNED_NON_CRIT_DAMAGE);
    assert_eq!(rng.draws(), 4);
}

/// The special-category mirror of the physical pin above, with
/// **non-neutral natures on both sides** so the `Stat` routing inside
/// `compute_stats` is load-bearing: a mutation that fed
/// `base.sp_defense` (or the wrong `Stat` to the nature scaler) into
/// `sp_attack` would change this damage value.
///
/// Squirtle (species 7) level 10, max IVs, **Modest** (personality 15:
/// `15 % 25` -> nature 15, +SpAtk/-Atk), using Water Gun (move 55, power
/// 40, accuracy 100, Water/special, plain `EFFECT_HIT`) against Rattata
/// (species 19) level 10, max IVs, **Calm** (personality 20, +SpDef/-Atk).
///
/// - `sp_attack` = `CALC_STAT(base 50, iv 31, lvl 10)` =
///   `(2*50+31)*10/100 + 5` = `1310/100 = 13`, `+5` = 18; Modest favours
///   Sp. Attack: `18*110/100` = **19**.
/// - `sp_defense` = `CALC_STAT(base 35, iv 31, lvl 10)` =
///   `(2*35+31)*10/100 + 5` = `1010/100 = 10`, `+5` = 15; Calm favours
///   Sp. Defense: `15*110/100` = **16**.
/// - `CalculateBaseDamage` (special branch): `19 * 40` = 760;
///   `* (2*10/5 + 2 = 6)` = 4560; `/ 16` = 285; `/ 50` = 5; `+ 2` = **7**.
/// - Squirtle is pure Water and Water Gun is Water: STAB `7*15/10` = 10.
///   Rattata is pure Normal: Water is neutral into it, an identity step.
/// - Best damage roll (100%) leaves it at **10**.
#[test]
fn a_special_move_with_non_neutral_natures_matches_the_hand_computed_pin() {
    let dex = Dex::new();
    let attacker =
        BattlePokemon::new(&dex, SpeciesId(7), 10, MAX_IVS, 15, vec![MoveId(55)]).unwrap();
    let defender =
        BattlePokemon::new(&dex, SpeciesId(19), 10, MAX_IVS, 20, vec![MoveId(33)]).unwrap();
    // The stat legs of the hand computation, pinned separately so a
    // failure points at stats vs. the damage formula.
    assert_eq!(attacker.stats().sp_attack, 19, "Modest-boosted SpAtk");
    assert_eq!(defender.stats().sp_defense, 16, "Calm-boosted SpDef");

    // draw0: accuracy roll 0 -> 1 <= 100 -> hit.
    // draw1: crit roll 1 -> no crit.
    // draw2: damage roll 0 -> 100%.
    // draw3: the discarded effect-chance roll.
    let mut rng = SequenceRng::new([0, 1, 0, 0]);
    let outcome = resolve_hit(&dex, MoveId(55), &attacker, &defender, false, &mut rng).unwrap();
    assert_eq!(
        outcome,
        HitOutcome::Hit {
            damage: 10,
            is_critical: false,
        }
    );
    assert_eq!(rng.draws(), 4);
}

#[test]
fn struggle_ignores_type_immunity_and_damages_a_ghost() {
    let dex = Dex::new();
    // Normal-type moves cannot touch a Ghost, but `Cmd_typecalc` returns
    // before every `ModulateDmgByType` call for MOVE_STRUGGLE
    // (battle_script_commands.c:1360-1364), so Struggle still connects.
    let attacker = mon(&dex, 1, 5, vec![STRUGGLE]); // Bulbasaur, attack 11
    let defender = mon(&dex, 92, 5, vec![MoveId(33)]); // Gastly, Ghost/Poison

    // Struggle: power 50, accuracy 100. Gastly defense =
    // (2*30+31)*5/100 + 5 = 455/100 = 4, +5 = 9.
    // 11*50 = 550; *4 = 2200; /9 = 244; /50 = 4; +2 = 6.
    // No STAB (Struggle is exempt), no type step, 100% roll -> 6.
    // And no effect-chance draw: Struggle's MOVE_EFFECT_CERTAIN takes
    // Cmd_seteffectwithchance's draw-free first branch (module docs,
    // step 7), so a landed Struggle costs 3 draws, not 4. The sequence
    // is exactly 3 long: a stray fourth draw would panic.
    let mut rng = SequenceRng::new([0, 1, 0]);
    let outcome = resolve_hit(&dex, STRUGGLE, &attacker, &defender, false, &mut rng).unwrap();
    assert_eq!(
        outcome,
        HitOutcome::Hit {
            damage: 6,
            is_critical: false,
        }
    );
    assert_eq!(rng.draws(), 3, "no seteffectwithchance draw for Struggle");

    // The control: an ordinary Normal move from the same attacker against
    // the same Ghost defender *is* nullified (and, being an ordinary
    // move, it does take the effect-chance draw).
    let tackler = mon(&dex, 1, 5, vec![MoveId(33)]);
    let mut rng = SequenceRng::new([0, 1, 0, 0]);
    let outcome = resolve_hit(&dex, MoveId(33), &tackler, &defender, false, &mut rng).unwrap();
    assert_eq!(outcome, HitOutcome::NoEffect);
}

#[test]
fn type_immunity_reports_no_effect_and_still_draws_the_full_sequence() {
    let dex = Dex::new();
    // Gastly (Ghost/Poison, id 92) is immune to Normal. (Electric into a
    // Ground type would be the same shape, but no damaging Electric move
    // can back a *3-draw* assertion: they are all either non-plain
    // effects this pipeline rejects -- Thunder Shock and friends are
    // EFFECT_PARALYZE_HIT -- or Shock Wave, which is allow-listed but
    // EFFECT_ALWAYS_HIT, so it skips the accuracy draw and costs 2.)
    let attacker = mon(&dex, 1, 20, vec![MoveId(33)]); // Bulbasaur/Tackle
    let defender = mon(&dex, 92, 20, vec![MoveId(33)]);
    let mut rng = SequenceRng::new([0, 1, 0, 0]);
    let outcome = resolve_hit(&dex, MoveId(33), &attacker, &defender, false, &mut rng).unwrap();
    assert_eq!(outcome, HitOutcome::NoEffect);
    assert_eq!(
        rng.draws(),
        4,
        "immunity still draws crit + damage-roll + effect-chance RNG \
         (Cmd_typecalc falls through; the NO_EFFECT flag is too late in \
         Cmd_seteffectwithchance's && chain to suppress its draw)"
    );
}

#[test]
fn an_accuracy_bypassing_hit_draws_exactly_three_times() {
    let dex = Dex::new();
    // Swift (move 129) is EFFECT_ALWAYS_HIT, which `AccuracyCalcHelper`
    // short-circuits (`battle_script_commands.c:1089`-`:1094`): step 1 of
    // the pipeline makes no draw at all, so a hit costs 3 rather than 4.
    let attacker = mon(&dex, 1, 5, vec![MoveId(129)]); // Bulbasaur/Swift
    let defender = mon(&dex, 7, 5, vec![MoveId(33)]); // Squirtle
    assert!(always_hits(dex.move_data(MoveId(129)).unwrap().effect));

    // draw0: crit roll 1 -> 1 % 16 != 0 -> no crit.
    // draw1: damage roll 0 -> 100%.  No accuracy draw ahead of them.
    // draw2: the discarded effect-chance roll.
    let mut rng = SequenceRng::new([1, 0, 0]);
    let outcome = resolve_hit(&dex, MoveId(129), &attacker, &defender, false, &mut rng).unwrap();
    assert!(matches!(outcome, HitOutcome::Hit { .. }));
    assert_eq!(
        rng.draws(),
        3,
        "an always-hit move skips the accuracy draw entirely"
    );

    // And it cannot miss: the value that would have missed an ordinary
    // 95-accuracy move is simply never consumed as an accuracy roll, so
    // feeding it first would desynchronise a caller's script.
    let mut rng = SequenceRng::new([95, 0, 0]);
    let outcome = resolve_hit(&dex, MoveId(129), &attacker, &defender, false, &mut rng).unwrap();
    assert!(
        matches!(outcome, HitOutcome::Hit { .. }),
        "95 was consumed as the crit roll, not an accuracy roll"
    );
    assert_eq!(rng.draws(), 3);
}

#[test]
fn a_high_crit_move_crits_on_a_draw_a_plain_move_would_not() {
    let dex = Dex::new();
    // Slash (move 163) is EFFECT_HIGH_CRITICAL: crit stage 1, odds 1/8
    // (`sCriticalHitChance[1]`), where a plain move rolls 1/16. Draw 8
    // is the separating value -- 8 % 8 == 0 crits at stage 1, while
    // 8 % 16 != 0 does not at stage 0 -- so this pins both that
    // `resolve_hit` feeds the move's effect into `crit_stage_for_effect`
    // and that stage 1 really means shorter odds.
    let attacker = mon(&dex, 1, 5, vec![MoveId(163), MoveId(33)]); // Slash, Tackle
    let defender = mon(&dex, 7, 5, vec![MoveId(33)]); // Squirtle

    // draws: accuracy 0 (hit), crit 8, damage roll 0, effect chance 0.
    let mut rng = SequenceRng::new([0, 8, 0, 0]);
    let outcome = resolve_hit(&dex, MoveId(163), &attacker, &defender, false, &mut rng).unwrap();
    assert!(
        matches!(
            outcome,
            HitOutcome::Hit {
                is_critical: true,
                ..
            }
        ),
        "Slash crits on draw 8 at stage 1: {outcome:?}"
    );

    // The control: identical draws through the plain-crit pipeline stay
    // non-critical, so the assertion above cannot pass by accident.
    let mut rng = SequenceRng::new([0, 8, 0, 0]);
    let outcome = resolve_hit(&dex, MoveId(33), &attacker, &defender, false, &mut rng).unwrap();
    assert!(
        matches!(
            outcome,
            HitOutcome::Hit {
                is_critical: false,
                ..
            }
        ),
        "Tackle does not crit on draw 8 at stage 0: {outcome:?}"
    );
}

#[test]
fn suppress_crit_skips_the_crit_draw_entirely_and_never_crits() {
    let dex = Dex::new();
    // Slash again: draw 8 would crit at stage 1 (the test above), so a
    // crit surviving here would mean the suppression forgot to skip the
    // *decision*, not just its consequence.
    let attacker = mon(&dex, 1, 5, vec![MoveId(163)]); // Slash
    let defender = mon(&dex, 7, 5, vec![MoveId(33)]);

    // Only 3 values: accuracy, damage roll, effect chance -- the crit
    // slot (upstream's `battle_script_commands.c:1279`-`:1283` chain
    // failing its `BATTLE_TYPE_FIRST_BATTLE` operand) is never drawn, so
    // a stray 4th value would panic `SequenceRng` if resolve_hit still
    // consumed one.
    let mut rng = SequenceRng::new([0, 0, 0]);
    let outcome = resolve_hit(&dex, MoveId(163), &attacker, &defender, true, &mut rng).unwrap();
    assert!(
        matches!(
            outcome,
            HitOutcome::Hit {
                is_critical: false,
                ..
            }
        ),
        "suppressed crit must report non-critical even on a would-crit draw: {outcome:?}"
    );
    assert_eq!(rng.draws(), 3, "no crit draw when suppressed");
}

#[test]
fn suppress_crit_drops_every_draw_count_in_the_module_docs_table_by_one() {
    let dex = Dex::new();
    let attacker = mon(&dex, 1, 5, vec![MoveId(33), MoveId(129)]); // Tackle, Swift
    let defender = mon(&dex, 7, 5, vec![MoveId(33)]);

    // Ordinary hit: 4 -> 3 (accuracy, damage roll, effect chance).
    let mut rng = SequenceRng::new([0, 0, 0]);
    let outcome = resolve_hit(&dex, MoveId(33), &attacker, &defender, true, &mut rng).unwrap();
    assert!(matches!(outcome, HitOutcome::Hit { .. }));
    assert_eq!(rng.draws(), 3);

    // Accuracy-bypassing hit (Swift): 3 -> 2 (damage roll, effect chance).
    let mut rng = SequenceRng::new([0, 0]);
    let outcome = resolve_hit(&dex, MoveId(129), &attacker, &defender, true, &mut rng).unwrap();
    assert!(matches!(outcome, HitOutcome::Hit { .. }));
    assert_eq!(rng.draws(), 2);

    // A miss is unaffected: the crit draw never happens after a miss
    // either way, suppressed or not.
    let mut rng = SequenceRng::new([95]);
    let outcome = resolve_hit(&dex, MoveId(33), &attacker, &defender, true, &mut rng).unwrap();
    assert_eq!(outcome, HitOutcome::Miss);
    assert_eq!(rng.draws(), 1);
}

#[test]
fn zero_power_moves_are_reported_as_unsupported() {
    let dex = Dex::new();
    let attacker = mon(&dex, 1, 5, vec![MoveId(45)]); // Growl (status move)
    let defender = mon(&dex, 7, 5, vec![MoveId(33)]);
    let mut rng = SequenceRng::new([]);
    assert_eq!(
        resolve_hit(&dex, MoveId(45), &attacker, &defender, false, &mut rng),
        Err(BattleError::NonDamagingMove(MoveId(45)))
    );
}

#[test]
fn powered_moves_with_a_different_battle_script_are_rejected() {
    let dex = Dex::new();
    let attacker = mon(&dex, 1, 5, vec![MoveId(33)]);
    let defender = mon(&dex, 7, 5, vec![MoveId(33)]);

    // Every one of these has non-zero base power, so the old `power == 0`
    // filter waved them all through into a pipeline that computes the
    // wrong damage *and* the wrong number of draws.
    for (move_id, what) in [
        (
            MoveId(49),
            "Sonic Boom: EFFECT_SONICBOOM, power 1, flat 20 damage",
        ),
        (MoveId(3), "Double Slap: EFFECT_MULTI_HIT, 2..5 hits"),
        (MoveId(32), "Horn Drill: EFFECT_OHKO"),
        (MoveId(68), "Counter: EFFECT_COUNTER"),
        (MoveId(69), "Seismic Toss: EFFECT_LEVEL_DAMAGE"),
        (MoveId(71), "Absorb: EFFECT_ABSORB (drains)"),
        // These two DO point at BattleScript_EffectHit, but the engine
        // special-cases them outside the script, so they are
        // deliberately absent from ORDINARY_HIT_EFFECTS (module docs) --
        // a contributor completing the allowlist from the C table alone
        // would wrongly admit both.
        (
            MoveId(206),
            "False Swipe: EFFECT_FALSE_SWIPE, damage clamped to leave 1 HP \
             (battle_script_commands.c:1683)",
        ),
        (
            MoveId(228),
            "Pursuit: EFFECT_PURSUIT, re-targeted/re-powered on switch \
             (battle_script_commands.c:8745/:9854)",
        ),
    ] {
        assert!(
            dex.move_data(move_id).unwrap().power > 0,
            "{what}: the point of this case is that power alone does not filter it"
        );
        let mut rng = SequenceRng::new([]);
        assert_eq!(
            resolve_hit(&dex, move_id, &attacker, &defender, false, &mut rng),
            Err(BattleError::UnsupportedMoveEffect(move_id)),
            "{what}"
        );
        // The sequence is empty, so any draw before the rejection would
        // have panicked: an unsupported move never touches the RNG.
        assert_eq!(rng.draws(), 0, "{what}");
    }
}

#[test]
fn plain_hit_shaped_moves_are_accepted() {
    let dex = Dex::new();
    // Effects that dispatch to BattleScript_EffectHit itself
    // (`data/battle_scripts_1.s`), so the pipeline here is the whole
    // script: Tackle (EFFECT_HIT), Slash (EFFECT_HIGH_CRITICAL, a crit
    // stage this module already models), Swift (EFFECT_ALWAYS_HIT, the
    // accuracy bypass), Quick Attack (EFFECT_QUICK_ATTACK, priority only).
    for move_id in [MoveId(33), MoveId(163), MoveId(129), MoveId(98)] {
        let effect = dex.move_data(move_id).unwrap().effect;
        assert!(is_ordinary_hit_effect(effect), "move {}", move_id.0);
        assert_eq!(ensure_resolvable(&dex, move_id), Ok(()));
    }
    // Struggle is the documented exception: EFFECT_RECOIL is not a plain
    // hit effect, but its damage half is exactly this pipeline.
    assert!(!is_ordinary_hit_effect(
        dex.move_data(STRUGGLE).unwrap().effect
    ));
    assert_eq!(ensure_resolvable(&dex, STRUGGLE), Ok(()));
}

#[test]
fn ensure_resolvable_reports_unknown_and_zero_power_moves_without_drawing() {
    let dex = Dex::new();
    let bad = MoveId(60_000);
    assert_eq!(
        ensure_resolvable(&dex, bad),
        Err(BattleError::UnknownMove(bad))
    );
    assert_eq!(
        ensure_resolvable(&dex, MoveId(45)), // Growl
        Err(BattleError::NonDamagingMove(MoveId(45)))
    );
    // MOVE_CURSE (174) is the sole ???-typed move; it is also 0-power, so
    // the power check reaches it first -- the type check behind it stays
    // as a guard for any future caller-supplied move data.
    assert_eq!(
        ensure_resolvable(&dex, MoveId(174)),
        Err(BattleError::NonDamagingMove(MoveId(174)))
    );
}

/// A Battle Armor or Shell Armor **defender** makes `resolve_hit` skip the
/// crit draw the same way `suppress_crit` does (issue #391), needing no
/// caller flag: Slash crits on draw 8 at stage 1 (the earlier
/// `a_high_crit_move_crits_on_a_draw_a_plain_move_would_not` test proves
/// it), so a crit surviving here would mean the ability check forgot to
/// skip the *decision*, not just its consequence. Only 3 values are
/// scripted -- accuracy, damage roll, effect chance -- so a stray crit draw
/// would panic `SequenceRng` before the assertion even runs.
#[test]
fn a_battle_armor_or_shell_armor_defender_skips_the_crit_draw_and_never_crits() {
    let dex = Dex::new();
    let attacker = mon(&dex, 1, 5, vec![MoveId(163)]); // Bulbasaur/Slash
    for armor_species in [390u16, 90] {
        // Anorith (Battle Armor), Shellder (Shell Armor)
        let defender = mon(&dex, armor_species, 5, vec![MoveId(33)]);
        assert!(
            suppresses_critical_hits(defender.ability()),
            "species {armor_species}"
        );
        let mut rng = SequenceRng::new([0, 8, 0]);
        let outcome =
            resolve_hit(&dex, MoveId(163), &attacker, &defender, false, &mut rng).unwrap();
        assert!(
            matches!(
                outcome,
                HitOutcome::Hit {
                    is_critical: false,
                    ..
                }
            ),
            "species {armor_species}: {outcome:?}"
        );
        assert_eq!(
            rng.draws(),
            3,
            "species {armor_species}: armor must skip the crit draw entirely"
        );
    }
}

/// Reference for the Huge Power pins below, hand computed independent of
/// this crate's own output:
///
/// Marill (species 183) level 5, max IVs, Hardy, using Tackle (move 33,
/// power 35, accuracy 95, Normal/physical) against Squirtle (species 7)
/// level 5, max IVs, Hardy -- the same defender [`PINNED_NON_CRIT_DAMAGE`]
/// uses (defense 13).
///
/// - Marill attack = `CALC_STAT(base 20, iv 31, lvl 5)` = `(2*20+31)*5/100 +
///   5` = `355/100 = 3`, `+5` = **8**.
/// - Without Huge Power (ability index 0, Pickup): `8*35 = 280`;
///   `*(2*5/5+2=4)` = 1120; `/13` = 86; `/50` = 1; `+2` = **3**.
/// - With Huge Power (ability index 1, `AbilityId(37)`): the raw stat
///   doubles to 16 before `APPLY_STAT_MOD` (no stat stage is in play here,
///   so the order cannot matter for this pin -- see
///   [`huge_power_doubles_the_raw_stat_before_the_attack_stage_not_after`]
///   for a case where it does): `16*35 = 560`; `*4` = 2240; `/13` = 172;
///   `/50` = 3; `+2` = **5**.
/// - Marill is pure Water; Tackle is Normal, so no STAB, and Normal is
///   neutral into Water, so both figures stand unmodified. Best roll (100%)
///   leaves them at 3 and 5.
#[test]
fn huge_power_in_ability_slot_two_doubles_a_physical_hit() {
    let dex = Dex::new();
    let defender = mon(&dex, 7, 5, vec![MoveId(33)]); // Squirtle

    let pickup = BattlePokemon::new(&dex, SpeciesId(183), 5, MAX_IVS, 0, vec![MoveId(33)])
        .unwrap()
        .with_ability_slot(0);
    assert_eq!(
        pickup.ability(),
        AbilityId(47),
        "Marill ability index 0 is Pickup"
    );
    let mut rng = SequenceRng::new([0, 1, 0, 0]);
    assert_eq!(
        resolve_hit(&dex, MoveId(33), &pickup, &defender, false, &mut rng).unwrap(),
        HitOutcome::Hit {
            damage: 3,
            is_critical: false,
        },
        "no Huge Power in ability index 0"
    );

    let huge_power = BattlePokemon::new(&dex, SpeciesId(183), 5, MAX_IVS, 0, vec![MoveId(33)])
        .unwrap()
        .with_ability_slot(1);
    assert_eq!(
        huge_power.ability(),
        HUGE_POWER,
        "Marill ability index 1 is Huge Power"
    );
    let mut rng = SequenceRng::new([0, 1, 0, 0]);
    assert_eq!(
        resolve_hit(&dex, MoveId(33), &huge_power, &defender, false, &mut rng).unwrap(),
        HitOutcome::Hit {
            damage: 5,
            is_critical: false,
        },
        "Huge Power in ability index 1 must double the raw Attack stat"
    );
}

/// Pure Power (`AbilityId(74)`) is Meditite's sole ability (species 356,
/// ability index 0 -- no `with_ability_slot` needed since its second slot
/// is empty): the same raw-stat doubling as Huge Power, a different id.
///
/// Meditite level 5, max IVs, Hardy, using Tackle against the same Squirtle
/// (defense 13):
/// - attack = `CALC_STAT(base 40, iv 31, lvl 5)` = `(2*40+31)*5/100+5` =
///   `555/100=5`, `+5` = **10**; doubled = **20**.
/// - `20*35 = 700`; `*4 = 2800`; `/13 = 215`; `/50 = 4`; `+2 = 6`.
/// - Meditite is Fighting/Psychic: no STAB against a Normal move, and
///   Normal is neutral into Water, so the figure stands. Best roll (100%)
///   leaves it at 6.
#[test]
fn pure_power_doubles_a_physical_hit() {
    let dex = Dex::new();
    let attacker = mon(&dex, 356, 5, vec![MoveId(33)]); // Meditite
    assert_eq!(attacker.ability(), PURE_POWER, "Meditite's sole ability");
    let defender = mon(&dex, 7, 5, vec![MoveId(33)]); // Squirtle

    let mut rng = SequenceRng::new([0, 1, 0, 0]);
    assert_eq!(
        resolve_hit(&dex, MoveId(33), &attacker, &defender, false, &mut rng).unwrap(),
        HitOutcome::Hit {
            damage: 6,
            is_critical: false,
        }
    );
}

/// Huge Power reads `attacker->attack`, never `attacker->spAttack`
/// (`pokeemerald/src/pokemon.c:3158`-`:3159`), so a special move must come
/// out identically whether the attacker's ability slot lands on Huge Power
/// or not.
#[test]
fn huge_power_never_touches_a_special_move() {
    let dex = Dex::new();
    let defender = mon(&dex, 7, 5, vec![MoveId(33)]); // Squirtle

    let pickup = BattlePokemon::new(&dex, SpeciesId(183), 5, MAX_IVS, 0, vec![MoveId(55)])
        .unwrap()
        .with_ability_slot(0);
    let huge_power = BattlePokemon::new(&dex, SpeciesId(183), 5, MAX_IVS, 0, vec![MoveId(55)])
        .unwrap()
        .with_ability_slot(1);
    assert_ne!(pickup.ability(), huge_power.ability());

    let mut rng = SequenceRng::new([0, 1, 0, 0]);
    let plain = resolve_hit(&dex, MoveId(55), &pickup, &defender, false, &mut rng).unwrap();
    let mut rng = SequenceRng::new([0, 1, 0, 0]);
    let boosted = resolve_hit(&dex, MoveId(55), &huge_power, &defender, false, &mut rng).unwrap();
    assert_eq!(
        plain, boosted,
        "Huge Power must not affect a Water Gun (special move)"
    );
}

/// Huge Power's doubling must land on the *raw* stat before the attack
/// stage's `APPLY_STAT_MOD` multiply, not after -- the two orders can give
/// different final damage once integer truncation is involved
/// ([`crate::ability::huge_power_attack`]'s docs derive the general case;
/// this pins one all the way through the pipeline, past the defense divide
/// and the final `/50`, to prove the discrepancy really does survive to the
/// returned damage and is not just an artifact of the isolated formula).
///
/// Marill (species 183) level 15, max IVs, Hardy, Huge Power (ability index
/// 1), Attack stage **-2**, using Tackle against Abra (species 63) level
/// 15, max IVs, Hardy (base defense 15, chosen so the discrepancy survives
/// every later truncation).
///
/// - Marill's raw attack at level 15 = `CALC_STAT(base 20, iv 31, lvl 15)` =
///   `(2*20+31)*15/100+5` = `1065/100=10`, `+5` = **15**.
/// - Correct order (double, then apply stage -2, ratio `10/20`):
///   `stage(2*15) = stage(30) = 30*10/20 = 15`.
/// - Wrong order (stage, then double) would give `2*stage(15) =
///   2*(15*10/20) = 2*7 = 14` -- one less, because `150/20 = 7.5` truncates
///   down while `300/20 = 15` does not.
/// - Abra's defense = `(2*15+31)*15/100+5` = `915/100=9`, `+5` = **14**.
/// - Correct: `15*35=525`; `*(2*15/5+2=8)=4200`; `/14=300`; `/50=6`; `+2=8`.
/// - Wrong: `14*35=490`; `*8=3920`; `/14=280`; `/50=5`; `+2=7` -- a
///   genuinely different final number, so this pin discriminates the two
///   orders rather than merely happening to agree.
/// - Marill is pure Water and Abra is pure Psychic: Tackle is Normal, so
///   neither STAB nor a type multiplier moves either figure.
#[test]
fn huge_power_doubles_the_raw_stat_before_the_attack_stage_not_after() {
    let dex = Dex::new();
    let mut attacker = BattlePokemon::new(&dex, SpeciesId(183), 15, MAX_IVS, 0, vec![MoveId(33)])
        .unwrap()
        .with_ability_slot(1);
    assert_eq!(attacker.ability(), HUGE_POWER);
    attacker.stages_mut().attack = StatStage::new(-2).unwrap();
    let defender = mon(&dex, 63, 15, vec![MoveId(33)]); // Abra

    let mut rng = SequenceRng::new([0, 1, 0, 0]);
    assert_eq!(
        resolve_hit(&dex, MoveId(33), &attacker, &defender, false, &mut rng).unwrap(),
        HitOutcome::Hit {
            damage: 8,
            is_critical: false,
        },
        "upstream doubles the raw stat before APPLY_STAT_MOD, not after"
    );
}
