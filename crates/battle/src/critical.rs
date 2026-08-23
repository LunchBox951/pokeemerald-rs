//! Critical hits (S-6): `Cmd_critcalc` plus the crit-only stat-stage
//! override inside `CalculateBaseDamage`.
//!
//! The issue's suggested name, `CalcCritChanceStage`, does not exist in the
//! upstream checkout at `pokeemerald/`; the real function is `Cmd_critcalc`
//! (`pokeemerald/src/battle_script_commands.c:1253`). It builds a crit-stage
//! index (`0..=4`) from Focus Energy / a handful of high-crit move effects /
//! Scope Lens / Lucky Punch / Stick, then rolls `Random() %
//! sCriticalHitChance[critChance] == 0` (`:606`). [`crit_stage`] models two
//! of those terms: the move-effect contribution
//! ([`crit_stage_for_effect`]) and — since issue #321 gave the engine a move
//! that can set it — Focus Energy's `+2`
//! ([`crate::volatile::Volatiles::focus_energy`], written by
//! [`crate::flag_move`]). The three held-item bonuses remain out of scope:
//! held-item effects are not modelled anywhere in this crate, so no state
//! could make them non-zero `(behavioral-fidelity)`.
//!
//! # When the draw does not happen at all
//!
//! The `Random()` call is the **last** operand of a four-way `&&` chain
//! (`battle_script_commands.c:1279`-`:1283`):
//!
//! ```text
//! if ((target ability != ABILITY_BATTLE_ARMOR && != ABILITY_SHELL_ARMOR)   // :1279
//!  && !(gStatuses3[attacker] & STATUS3_CANT_SCORE_A_CRIT)                  // :1280
//!  && !(gBattleTypeFlags & (BATTLE_TYPE_WALLY_TUTORIAL | BATTLE_TYPE_FIRST_BATTLE)) // :1281
//!  && !(Random() % sCriticalHitChance[critChance]))                        // :1282
//!     gCritMultiplier = 2;
//! ```
//!
//! C's `&&` short-circuits, so any of the first three conditions failing
//! means **no crit draw is made** and the hit consumes one fewer RNG value
//! than an ordinary hit does `(behavioral-fidelity)`. [`crit_roll`] models
//! only the case where all three pass — the *caller* is responsible for not
//! invoking it at all when one of them fails, which is exactly what
//! [`crate::hit::damage_before_roll`] does. The ability suppressor (Battle
//! Armor/Shell Armor) closed as of issue #391:
//! [`crate::ability::suppresses_critical_hits`] reads
//! `BattlePokemon::ability` (`crate::ability`) and
//! [`crate::hit::damage_before_roll`] folds it into the same short-circuit
//! `suppress_crit` already gets, ahead of the draw. It is reachable, not
//! hypothetical: Anorith/Armaldo (species 390/391,
//! `crates/assets/src/species.rs`) carry Battle Armor, and a save-restored
//! party keeps whichever ability slot the save stored
//! (`BattlePokemon::with_ability_slot`, read by `pokeemerald-rs`'s `party`
//! module on decode) rather than re-deriving it from personality, so a
//! loaded Anorith reaches this path exactly as a freshly caught one does.
//! `STATUS3_CANT_SCORE_A_CRIT` is only ever set by Future Sight/Doom Desire.
//! The third, `BATTLE_TYPE_FIRST_BATTLE`, belongs to a scripted one-off
//! battle (set for the Route 101 intro Zigzagoon fight and nothing else,
//! `SetUpBattleVarsAndBirchZigzagoon`, `src/battle_controllers.c:67`-`:72`,
//! triggered from `src/battle_setup.c:937`; `BATTLE_TYPE_WALLY_TUTORIAL`
//! covers the sibling Wally's-catching-tutorial case, also unmodelled) — as
//! of issue #187, [`crate::battle::Battle`]'s `first_battle` flag *does*
//! exist, and it is exactly this suppressor: [`crate::hit::resolve_hit`]'s
//! `suppress_crit` parameter is the caller skipping [`crit_roll`] entirely
//! (rather than calling it and discarding the result) that this paragraph
//! used to describe only as a requirement for a future caller.
//!
//! A confirmed crit changes two things elsewhere in the pipeline, both from
//! `CalculateBaseDamage` (`pokeemerald/src/pokemon.c:3106`), reproduced here
//! rather than in [`crate::damage`] so that module's existing (already
//! merged, already tested) formula stays untouched:
//!
//! - [`crit_adjusted_stages`]: a crit **ignores** a stat-stage change that
//!   would help the *defender* — an attack-stat drop or a defense-stat
//!   raise — using the stage as-is only when it instead helps the
//!   *attacker* (`pokemon.c:3234`, `:3249`, and the mirrored Sp. Atk/Sp. Def
//!   block at `:3290`/`:3305`).
//! - Reflect/Light Screen are skipped entirely on a crit (`gCritMultiplier
//!   == 1` guards both halvings, `pokemon.c:3264`/`:3316`) — modelled by
//!   [`crate::hit::resolve_hit`] forcing `reflect`/`light_screen` off before
//!   building [`crate::damage::DamageInput`], not here.
//!
//! The `x2` multiplier itself is *not* applied inside `CalculateBaseDamage`
//! — it is a separate multiply in `Cmd_damagecalc`
//! (`gBattleMoveDamage = gBattleMoveDamage * gCritMultiplier * ...`,
//! `battle_script_commands.c:1296`), reproduced the same way by
//! [`crate::hit::resolve_hit`].

use assets::MoveEffect;

use crate::damage::BattleRng;
use crate::stat_stage::StatStage;

/// `sCriticalHitChance` (`pokeemerald/src/battle_script_commands.c:606`):
/// `1 / value` odds per crit-chance stage `0..=4`.
const CRIT_CHANCE_TABLE: [u16; 5] = [16, 8, 4, 3, 2];

/// The four move effects `Cmd_critcalc` counts toward the crit-chance stage
/// (`battle_script_commands.c:1268`): `EFFECT_HIGH_CRITICAL` (Slash, Crabhammer,
/// Karate Chop, ...), `EFFECT_SKY_ATTACK`, `EFFECT_BLAZE_KICK`, and
/// `EFFECT_POISON_TAIL`, each worth `+1` stage
/// (`pokeemerald/include/constants/battle_move_effects.h:47,79,204,213`).
const HIGH_CRIT_EFFECTS: [MoveEffect; 4] = [
    MoveEffect(43),
    MoveEffect(75),
    MoveEffect(200),
    MoveEffect(209),
];

/// The crit-chance stage (`0..=4`) contributed by `move_effect` alone —
/// [`crit_stage`] without the Focus Energy term, kept as its own function
/// because the move's contribution is the part that depends on nothing but
/// the move.
#[must_use]
pub fn crit_stage_for_effect(move_effect: MoveEffect) -> u8 {
    u8::from(HIGH_CRIT_EFFECTS.contains(&move_effect))
}

/// `Cmd_critcalc`'s whole `critChance` sum
/// (`pokeemerald/src/battle_script_commands.c:1267`-`:1274`), as far as this
/// crate models its terms: `2 *` the attacker's `STATUS2_FOCUS_ENERGY` bit
/// plus [`crit_stage_for_effect`]'s move term.
///
/// The four held-item terms (Scope Lens, and the species-locked Lucky Punch
/// and Stick) are absent: held-item *effects* are not modelled anywhere in
/// this crate, so there is no state that could make them non-zero. The
/// result is **not** clamped here — [`crit_roll`] clamps to the table's last
/// index exactly as upstream does at `:1277`-`:1278`, and clamping twice
/// would hide a caller that summed too much.
///
/// `focus_energy` is [`crate::volatile::Volatiles::focus_energy`], set by
/// [`crate::flag_move`]'s Focus Energy script.
#[must_use]
pub fn crit_stage(move_effect: MoveEffect, focus_energy: bool) -> u8 {
    2 * u8::from(focus_energy) + crit_stage_for_effect(move_effect)
}

/// Roll for a critical hit at crit-chance `stage` (clamped to the table's
/// last index, `4`, matching `Cmd_critcalc`'s own clamp): `Random() %
/// sCriticalHitChance[stage] == 0`.
///
/// Calling this *is itself* the decision that upstream's `&&` chain reached
/// its `Random()` operand — see the module docs' "When the draw does not
/// happen at all". Given that, the draw is unconditional:
/// [`crate::hit::resolve_hit`] must call this even for a move that will
/// later prove to miss the defender's type (immunity), matching upstream's
/// battle-script step order.
#[must_use]
pub fn crit_roll(stage: u8, rng: &mut impl BattleRng) -> bool {
    let index = (stage as usize).min(CRIT_CHANCE_TABLE.len() - 1);
    u32::from(rng.next_u16()) % u32::from(CRIT_CHANCE_TABLE[index]) == 0
}

/// The crit-only stat-stage override (see the module docs): on a crit, a
/// stage that would help the defender is replaced with [`StatStage::NEUTRAL`];
/// a non-crit hit always uses the stage as-is.
///
/// `favors_defender` decides which direction is "helps the defender" for the
/// stage being adjusted: pass `|s| s.offset() < 0` for the attack stage
/// (upstream: apply only `if (attacker->statStages[STAT_ATK] >
/// DEFAULT_STAT_STAGE)`, i.e. skip the override when the stage is at or
/// below neutral) or `|s| s.offset() > 0` for the defense stage (upstream:
/// apply only `if (defender->statStages[STAT_DEF] < DEFAULT_STAT_STAGE)`).
#[must_use]
pub fn crit_adjusted_stage(
    stage: StatStage,
    is_critical: bool,
    favors_defender: impl Fn(StatStage) -> bool,
) -> StatStage {
    if is_critical && favors_defender(stage) {
        StatStage::NEUTRAL
    } else {
        stage
    }
}

/// [`crit_adjusted_stage`] applied to an attack/defense stage pair in one
/// call — the exact pairing [`crate::hit::resolve_hit`] needs.
#[must_use]
pub fn crit_adjusted_stages(
    attack_stage: StatStage,
    defense_stage: StatStage,
    is_critical: bool,
) -> (StatStage, StatStage) {
    let attack = crit_adjusted_stage(attack_stage, is_critical, |s| s.offset() < 0);
    let defense = crit_adjusted_stage(defense_stage, is_critical, |s| s.offset() > 0);
    (attack, defense)
}

#[cfg(test)]
mod tests {
    use super::{crit_adjusted_stages, crit_roll, crit_stage_for_effect, HIGH_CRIT_EFFECTS};
    use crate::damage::BattleRng;
    use crate::stat_stage::StatStage;
    use assets::MoveEffect;

    struct FixedRng(u16);
    impl BattleRng for FixedRng {
        fn next_u16(&mut self) -> u16 {
            self.0
        }
    }

    struct CountingRng {
        value: u16,
        draws: u32,
    }
    impl BattleRng for CountingRng {
        fn next_u16(&mut self) -> u16 {
            self.draws += 1;
            self.value
        }
    }

    #[test]
    fn crit_stage_for_effect_covers_the_four_high_crit_effects() {
        for effect in HIGH_CRIT_EFFECTS {
            assert_eq!(crit_stage_for_effect(effect), 1, "{effect:?}");
        }
        assert_eq!(crit_stage_for_effect(MoveEffect(0)), 0); // EFFECT_HIT
    }

    #[test]
    fn crit_roll_draws_exactly_once() {
        let mut rng = CountingRng { value: 1, draws: 0 };
        let _ = crit_roll(0, &mut rng);
        assert_eq!(rng.draws, 1);
    }

    #[test]
    fn crit_roll_stage_zero_is_one_in_sixteen() {
        assert!(crit_roll(0, &mut FixedRng(0))); // 0 % 16 == 0
        assert!(crit_roll(0, &mut FixedRng(16))); // 16 % 16 == 0
        assert!(!crit_roll(0, &mut FixedRng(1)));
        assert!(!crit_roll(0, &mut FixedRng(15)));
    }

    #[test]
    fn crit_roll_higher_stages_use_shorter_odds() {
        // Stage 4 (max): 1 % 2 != 0 -> a value that misses stage 0 (1 % 16
        // != 0) also misses here; but an even draw always crits at stage 4.
        assert!(crit_roll(4, &mut FixedRng(2)));
        assert!(!crit_roll(4, &mut FixedRng(1)));
    }

    #[test]
    fn every_crit_table_entry_is_pinned_at_its_own_boundary() {
        // sCriticalHitChance = [16, 8, 4, 3, 2]: each middle stage gets a
        // draw that crits at that stage but NOT at the stage below, so no
        // entry can silently degrade to a neighbour's odds. (Stages 2 and 3
        // need Focus Energy / held items and are unreachable through
        // `crit_stage_for_effect` this slice, but the transcribed table is
        // upstream data and is pinned as such.)
        //
        // stage 1 (1/8): 8 % 8 == 0 crits; 8 % 16 != 0 does not at stage 0.
        assert!(crit_roll(1, &mut FixedRng(8)));
        assert!(!crit_roll(0, &mut FixedRng(8)));
        // stage 2 (1/4): 4 % 4 == 0 crits; 4 % 8 != 0 does not at stage 1.
        assert!(crit_roll(2, &mut FixedRng(4)));
        assert!(!crit_roll(1, &mut FixedRng(4)));
        // stage 3 (1/3): 3 % 3 == 0 crits; 3 % 4 != 0 does not at stage 2 --
        // and 4 % 3 != 0 misses at stage 3 where 4 % 4 == 0 crits at stage
        // 2, so 3 and 4 cannot be swapped either.
        assert!(crit_roll(3, &mut FixedRng(3)));
        assert!(!crit_roll(2, &mut FixedRng(3)));
        assert!(!crit_roll(3, &mut FixedRng(4)));
    }

    #[test]
    fn crit_roll_clamps_stages_above_the_table() {
        // Stage 4 and stage 99 must behave identically (both clamp to index 4).
        for draw in [0u16, 1, 2, 3] {
            assert_eq!(
                crit_roll(4, &mut FixedRng(draw)),
                crit_roll(99, &mut FixedRng(draw)),
                "draw {draw}"
            );
        }
    }

    #[test]
    fn crit_adjusted_stages_are_unchanged_on_a_non_critical_hit() {
        let atk = StatStage::new(-2).unwrap();
        let def = StatStage::new(3).unwrap();
        assert_eq!(crit_adjusted_stages(atk, def, false), (atk, def));
    }

    #[test]
    fn crit_ignores_an_attack_drop_but_keeps_an_attack_boost() {
        let dropped = StatStage::new(-3).unwrap();
        let (atk, _) = crit_adjusted_stages(dropped, StatStage::NEUTRAL, true);
        assert_eq!(
            atk,
            StatStage::NEUTRAL,
            "a lowered attack is ignored on crit"
        );

        let boosted = StatStage::new(2).unwrap();
        let (atk, _) = crit_adjusted_stages(boosted, StatStage::NEUTRAL, true);
        assert_eq!(atk, boosted, "a raised attack still applies on crit");
    }

    #[test]
    fn crit_ignores_a_defense_boost_but_keeps_a_defense_drop() {
        let boosted = StatStage::new(4).unwrap();
        let (_, def) = crit_adjusted_stages(StatStage::NEUTRAL, boosted, true);
        assert_eq!(
            def,
            StatStage::NEUTRAL,
            "a raised defense is ignored on crit"
        );

        let dropped = StatStage::new(-1).unwrap();
        let (_, def) = crit_adjusted_stages(StatStage::NEUTRAL, dropped, true);
        assert_eq!(def, dropped, "a lowered defense still applies on crit");
    }

    #[test]
    fn crit_at_neutral_stages_stays_neutral() {
        let (atk, def) = crit_adjusted_stages(StatStage::NEUTRAL, StatStage::NEUTRAL, true);
        assert_eq!(atk, StatStage::NEUTRAL);
        assert_eq!(def, StatStage::NEUTRAL);
    }
}
