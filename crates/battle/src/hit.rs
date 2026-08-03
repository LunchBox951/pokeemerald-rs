//! Single-hit resolution (S-6): assembles [`crate::accuracy`],
//! [`crate::critical`], and [`crate::damage`] into upstream's exact
//! battle-script step order for one damaging move used against one target.
//!
//! Upstream spreads this across several `Cmd_*` battle-script commands run
//! back-to-back for `BattleScript_EffectHit`-shaped moves
//! (`pokeemerald/src/battle_script_commands.c`): `Cmd_accuracycheck`
//! (`:1099`), `Cmd_critcalc` (`:1253`), `Cmd_damagecalc` (`:1290`, which
//! calls `CalculateBaseDamage`, `src/pokemon.c:3106`, then applies
//! `gCritMultiplier` at `:1296`), `Cmd_typecalc` (`:1355` — STAB + type
//! effectiveness), and finally `ApplyRandomDmgMultiplier`. [`resolve_hit`]
//! runs the same sequence and draws the RNG at exactly the same points:
//!
//! 1. accuracy check (0 or 1 draw — see [`crate::accuracy`]); miss ends here.
//! 2. crit roll (1 draw, even if the hit will turn out type-immune — but see
//!    the caveat below).
//! 3. damage core + crit's stat-stage override ([`crate::critical`]) and
//!    `x2` multiply (no draw).
//! 4. STAB (no draw) — skipped for `MOVE_STRUGGLE`.
//! 5. dual-type effectiveness (no draw); a `NoEffect` row is terminal for
//!    the *outcome* but *not* for the RNG — see step 7 — and the type step
//!    is skipped entirely for `MOVE_STRUGGLE`.
//! 6. the `85..=100%` random roll (always 1 draw, even at `0` damage).
//! 7. the secondary-effect-chance roll (1 draw on every landed hit,
//!    `MOVE_STRUGGLE` excepted). `seteffectwithchance` runs between
//!    `resultmessage` and `tryfaintmon` in the hit script
//!    (`BattleScript_HitFromAtkAnimation`, `data/battle_scripts_1.s:265`),
//!    and `Cmd_seteffectwithchance` (`battle_script_commands.c:2908`) puts
//!    `Random() % 100 < percentChance` as the **leading** operand of its
//!    `else if` `&&` chain (`:2923`) — so for every allow-listed move
//!    (`gBattleCommunication[MOVE_EFFECT_BYTE]` is `0`: the plain hit script
//!    never runs `setmoveeffect`) the draw happens unconditionally and its
//!    value is discarded. A miss never gets here
//!    (`BattleScript_PrintMoveMissed` exits via `goto BattleScript_MoveEnd`,
//!    `:273`), but a type-immune hit does: `Cmd_typecalc` records
//!    `MOVE_RESULT_DOESNT_AFFECT_FOE` and falls through rather than jumping,
//!    and the `NO_EFFECT` test is only the *third* `&&` operand, too late to
//!    suppress the draw. Struggle is the exception the other way: its script
//!    (`BattleScript_EffectRecoil`, `:897`) runs `setmoveeffect ... |
//!    MOVE_EFFECT_CERTAIN` before jumping into the hit script, so
//!    `Cmd_seteffectwithchance` takes its *first* branch (`:2917`), which
//!    contains no `Random()` at all (the recoil is applied deterministically).
//!
//! So, **in the world this slice models**, one move resolution costs:
//!
//! | move | draws | which |
//! |------|-------|-------|
//! | ordinary move, missed | **1** | accuracy |
//! | ordinary move, hit (or type-immune) | **4** | accuracy, crit, damage roll, effect chance |
//! | accuracy-bypassing move | **3** | crit, damage roll, effect chance (never misses) |
//! | Struggle, hit | **3** | accuracy, crit, damage roll (no effect-chance draw) |
//!
//! The third row is `AccuracyCalcHelper`'s early `return TRUE`
//! (`battle_script_commands.c:1089`-`:1094`): for `EFFECT_ALWAYS_HIT` (Swift,
//! Shock Wave, Faint Attack, ...) and `EFFECT_VITAL_THROW`, step 1 is skipped
//! outright, so no accuracy draw is made and the move cannot miss — see
//! [`crate::accuracy::always_hits`]. Both effects are on this pipeline's
//! allow-list ([`is_ordinary_hit_effect`]), so the 3-draw shape is reachable
//! and is pinned by this module's tests; a caller scripting an RNG sequence
//! must budget for it.
//!
//! Neither "4" nor "3" is a universal upstream property, because the crit
//! draw is itself conditional: `Cmd_critcalc`'s `Random()` is the last
//! operand of a short-circuiting `&&` chain
//! (`battle_script_commands.c:1279`-`:1283`), so a defender with Battle Armor
//! / Shell Armor, an attacker under `STATUS3_CANT_SCORE_A_CRIT`, or a
//! `BATTLE_TYPE_WALLY_TUTORIAL` / `BATTLE_TYPE_FIRST_BATTLE` battle makes
//! step 2 draw **nothing** as well. None of those three exist here (no
//! abilities, no status3, ordinary post-first-battle wild encounter) — see
//! [`crate::critical`]'s module docs — so the table above is exact for every
//! hit [`resolve_hit`] can currently produce. (Serene Grace doubling
//! `percentChance` in step 7 similarly cannot matter: no abilities, and the
//! draw's value is discarded for every allow-listed move regardless.)
//!
//! # Which moves this pipeline may be handed
//!
//! The sequence above is one specific battle script —
//! `BattleScript_EffectHit` (`pokeemerald/data/battle_scripts_1.s:21`, the
//! script `gBattleScriptsForMoveEffects[EFFECT_HIT]` points at). A move's
//! `EFFECT_*` id selects its script, and most ids select a *different* one
//! that computes damage differently and draws the RNG a different number of
//! times: Sonic Boom's flat 20 damage (`EFFECT_SONICBOOM`, `:151`),
//! multi-hit's 2..5 hits (`EFFECT_MULTI_HIT`, `:50`), OHKO (`:59`),
//! `EFFECT_LEVEL_DAMAGE` (`:108`), Counter (`:110`), every
//! secondary-effect-on-hit script (whose `setmoveeffect` makes step 7's
//! discarded draw *land*, applying a status and possibly drawing again
//! inside `SetMoveEffect` — e.g. a sleep-turn roll), and so on. Feeding one
//! of those to [`resolve_hit`] would be
//! silently wrong twice over — wrong damage *and* a desynchronised shared RNG
//! stream — so [`ensure_resolvable`] rejects it up front
//! `(behavioral-fidelity)`. Base power `0` alone is **not** a sufficient
//! filter: Sonic Boom has power `1`.
//!
//! [`is_ordinary_hit_effect`] is the allow-list: exactly the effect ids whose
//! table entry *is* `BattleScript_EffectHit`, minus the two that the engine
//! still special-cases outside the script (`EFFECT_FALSE_SWIPE`, whose damage
//! is clamped to leave the target at 1 HP,
//! `src/battle_script_commands.c:1683`; and `EFFECT_PURSUIT`, which the
//! engine re-targets and re-powers when the foe switches, `:8745`/`:9854`).
//!
//! [`STRUGGLE`] is the single allowed exception, because its *hit* is exactly
//! this pipeline (with the documented type-calc skip below) even though its
//! effect id is `EFFECT_RECOIL`: the recoil is a separate battle-script step
//! that happens after the hit resolves, so [`resolve_hit`]'s own answer — the
//! damage dealt to the target — is complete and pinned by this module's
//! tests. A caller that applies nothing but the returned damage would still
//! be wrong about the attacker's HP, which is why the turn engine
//! ([`crate::battle::Battle::new`]) refuses Struggle outright rather than
//! reusing this function's slightly wider contract.
//!
//! `MOVE_STRUGGLE` is the one shape-changing special case: `Cmd_typecalc`
//! returns immediately for it (`battle_script_commands.c:1360`-`:1364`),
//! *before* the STAB multiply and before every `ModulateDmgByType` call, so
//! Struggle gets neither STAB nor type effectiveness — it damages a Ghost
//! defender that Normal-type moves cannot touch. Its `EFFECT_RECOIL` half is
//! a separate battle-script step and is not modelled here. Only the damaging
//! (`EFFECT_HIT`-shaped) move path is modelled — see
//! [`crate::error::BattleError::NonDamagingMove`] and
//! [`crate::error::BattleError::UnsupportedMoveEffect`].

use assets::{MoveEffect, MoveId, Type};

use crate::accuracy::accuracy_check;
use crate::critical::{crit_adjusted_stages, crit_roll, crit_stage_for_effect};
use crate::damage::{
    apply_damage_roll, apply_dual_type_effectiveness, apply_stab, base_damage, has_stab, BattleRng,
    DamageInput, MoveCategory, Weather, STRUGGLE,
};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::BattlePokemon;

/// The result of resolving one move against one target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HitOutcome {
    /// The accuracy check failed (`MOVE_RESULT_MISSED`).
    Miss,
    /// The move connected but the target's typing made it deal no damage
    /// (`MOVE_RESULT_DOESNT_AFFECT_FOE`).
    NoEffect,
    /// The move connected and dealt `damage` HP (already floored to at
    /// least `1` by the pipeline).
    Hit {
        /// HP of damage dealt.
        damage: u32,
        /// Whether this was a critical hit.
        is_critical: bool,
    },
}

/// The `EFFECT_*` ids whose `gBattleScriptsForMoveEffects` entry is
/// `BattleScript_EffectHit` itself — the script this module reproduces —
/// listed in table order (`pokeemerald/data/battle_scripts_1.s:21`..`:184`;
/// entry `n` is line `21 + n`).
///
/// Several are `_UP`/`_DOWN`/`UNUSED` slots whose *name* promises a stat
/// change the Gen-3 table never wires up: their entry really is the plain hit
/// script, so a move carrying one behaves as an ordinary damaging move and
/// belongs here. `EFFECT_FALSE_SWIPE` (`101`) and `EFFECT_PURSUIT` (`128`)
/// point at `BattleScript_EffectHit` too but are deliberately **absent** —
/// the engine special-cases both outside the script (see the module docs).
const ORDINARY_HIT_EFFECTS: [MoveEffect; 21] = [
    MoveEffect(0),   // EFFECT_HIT
    MoveEffect(12),  // EFFECT_SPEED_UP
    MoveEffect(14),  // EFFECT_SPECIAL_DEFENSE_UP
    MoveEffect(15),  // EFFECT_ACCURACY_UP
    MoveEffect(17),  // EFFECT_ALWAYS_HIT
    MoveEffect(21),  // EFFECT_SPECIAL_ATTACK_DOWN
    MoveEffect(22),  // EFFECT_SPECIAL_DEFENSE_DOWN
    MoveEffect(43),  // EFFECT_HIGH_CRITICAL
    MoveEffect(55),  // EFFECT_ACCURACY_UP_2
    MoveEffect(56),  // EFFECT_EVASION_UP_2
    MoveEffect(61),  // EFFECT_SPECIAL_ATTACK_DOWN_2
    MoveEffect(63),  // EFFECT_ACCURACY_DOWN_2
    MoveEffect(64),  // EFFECT_EVASION_DOWN_2
    MoveEffect(74),  // EFFECT_EVASION_DOWN_HIT
    MoveEffect(78),  // EFFECT_VITAL_THROW
    MoveEffect(96),  // EFFECT_UNUSED_60
    MoveEffect(103), // EFFECT_QUICK_ATTACK
    MoveEffect(110), // EFFECT_UNUSED_6E
    MoveEffect(131), // EFFECT_UNUSED_83
    MoveEffect(141), // EFFECT_UNUSED_8D
    MoveEffect(163), // EFFECT_UNUSED_A3
];

/// Whether `effect`'s battle script is the plain `BattleScript_EffectHit`
/// that [`resolve_hit`] reproduces — see [`ORDINARY_HIT_EFFECTS`] and the
/// module docs.
#[must_use]
pub fn is_ordinary_hit_effect(effect: MoveEffect) -> bool {
    ORDINARY_HIT_EFFECTS.contains(&effect)
}

/// Whether [`resolve_hit`] can resolve `move_id` at all — checked *before*
/// any state or RNG is touched, so an unsupported move can be rejected by a
/// caller (notably [`crate::battle::Battle::new`]) without leaving a
/// half-mutated turn behind.
///
/// # Errors
///
/// - [`BattleError::UnknownMove`] if `move_id` is not in `dex`.
/// - [`BattleError::NonDamagingMove`] if the move has `0` base power (status
///   moves — nothing for this pipeline to compute).
/// - [`BattleError::UnsupportedMoveType`] if the move is the sole `???`-typed
///   move (`MOVE_CURSE`).
/// - [`BattleError::UnsupportedMoveEffect`] if the move's `EFFECT_*` runs a
///   battle script other than `BattleScript_EffectHit`
///   ([`is_ordinary_hit_effect`]), [`STRUGGLE`] excepted — see the module
///   docs for why power alone is not a sufficient filter.
pub fn ensure_resolvable(dex: &Dex, move_id: MoveId) -> Result<(), BattleError> {
    let mv = dex.move_data(move_id)?;
    if mv.power == 0 {
        return Err(BattleError::NonDamagingMove(move_id));
    }
    if mv.move_type.battle_type().is_none() {
        return Err(BattleError::UnsupportedMoveType(move_id));
    }
    if !is_ordinary_hit_effect(mv.effect) && move_id != STRUGGLE {
        return Err(BattleError::UnsupportedMoveEffect(move_id));
    }
    Ok(())
}

/// Resolve `attacker` using `move_id` against `defender`.
///
/// Status conditions (burn), Reflect/Light Screen, and weather are not
/// modelled this slice: [`crate::damage::DamageInput`]'s corresponding
/// fields are always the "no effect" value here (`false`/[`Weather::None`]).
///
/// # Errors
///
/// Whatever [`ensure_resolvable`] reports for `move_id` — the same check, run
/// here first so this function is safe to call directly. It draws nothing
/// before that check, so a rejected move never disturbs `rng`.
pub fn resolve_hit(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
    rng: &mut impl BattleRng,
) -> Result<HitOutcome, BattleError> {
    ensure_resolvable(dex, move_id)?;
    let mv = dex.move_data(move_id)?;
    let move_type: Type = mv
        .move_type
        .battle_type()
        .ok_or(BattleError::UnsupportedMoveType(move_id))?;
    let category = MoveCategory::for_type(move_type);

    if !accuracy_check(
        mv.accuracy,
        mv.effect,
        attacker.stages().accuracy,
        defender.stages().evasion,
        rng,
    ) {
        return Ok(HitOutcome::Miss);
    }

    let crit_stage = crit_stage_for_effect(mv.effect);
    let is_critical = crit_roll(crit_stage, rng);

    let (attack_stat, attack_stage) = attacker.attacking_stat(category);
    let (defense_stat, defense_stage) = defender.defending_stat(category);
    let (attack_stage, defense_stage) =
        crit_adjusted_stages(attack_stage, defense_stage, is_critical);

    let input = DamageInput {
        attacker_level: attacker.level(),
        power: u32::from(mv.power),
        move_type,
        attack_stat,
        attack_stage,
        defense_stat,
        defense_stage,
        // Status conditions, side statuses, and weather are not modelled
        // this slice; a crit also forces reflect/light_screen off upstream
        // (`pokemon.c:3264`/`:3316`), which the constant `false` already
        // gives us regardless of `is_critical`.
        attacker_burned: false,
        reflect: false,
        light_screen: false,
        weather: Weather::None,
        is_solar_beam: false,
    };

    let mut damage = base_damage(&input);
    if is_critical {
        // Cmd_damagecalc: `gBattleMoveDamage *= gCritMultiplier` (2 on a
        // crit), applied to the whole base-damage result before STAB.
        damage *= 2;
    }
    let stab = has_stab(attacker.types(), move_id, move_type);
    let damage = apply_stab(damage, stab);
    // `Cmd_typecalc` returns at `battle_script_commands.c:1360`-`:1364` for
    // `MOVE_STRUGGLE`, ahead of both the STAB multiply ([`has_stab`] already
    // encodes that half) and every `ModulateDmgByType` call — so Struggle
    // ignores type effectiveness outright, immunities included. That is the
    // caller contract [`crate::damage::has_stab`] documents; honour it by
    // skipping the type step rather than passing a neutral multiplier, which
    // would still zero the damage against a Ghost defender.
    let damage = if move_id == STRUGGLE {
        damage
    } else {
        apply_dual_type_effectiveness(damage, move_type, defender.types())
    };
    let damage = apply_damage_roll(damage, rng);

    // Step 7 (module docs): `Cmd_seteffectwithchance` draws one `Random()`
    // on every landed hit — type-immune included — and discards it for every
    // move this pipeline admits (`MOVE_EFFECT_BYTE` is 0, so the `&&` chain
    // at `battle_script_commands.c:2923` fails after the leading `Random()`
    // operand). Struggle's `MOVE_EFFECT_CERTAIN` takes the draw-free first
    // branch instead (`:2917`), so it must not consume a draw here
    // `(behavioral-fidelity)`.
    if move_id != STRUGGLE {
        let _ = rng.next_u16();
    }

    if damage == 0 {
        Ok(HitOutcome::NoEffect)
    } else {
        Ok(HitOutcome::Hit {
            damage,
            is_critical,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{ensure_resolvable, is_ordinary_hit_effect, resolve_hit, HitOutcome};
    use crate::accuracy::always_hits;
    use crate::damage::{BattleRng, STRUGGLE};
    use crate::dex::Dex;
    use crate::error::BattleError;
    use crate::pokemon::{BattlePokemon, Ivs};
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
        let outcome = resolve_hit(&dex, MoveId(33), &attacker, &defender, &mut rng).unwrap();
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
        let outcome = resolve_hit(&dex, MoveId(33), &attacker, &defender, &mut rng).unwrap();
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
            outcomes.push(resolve_hit(&dex, MoveId(33), &attacker, &defender, &mut rng).unwrap());
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
        let outcome = resolve_hit(&dex, MoveId(33), &attacker, &defender, &mut rng).unwrap();
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
        let outcome = resolve_hit(&dex, MoveId(33), &attacker, &defender, &mut rng).unwrap();
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
        let outcome = resolve_hit(&dex, MoveId(55), &attacker, &defender, &mut rng).unwrap();
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
        let outcome = resolve_hit(&dex, STRUGGLE, &attacker, &defender, &mut rng).unwrap();
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
        let outcome = resolve_hit(&dex, MoveId(33), &tackler, &defender, &mut rng).unwrap();
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
        let outcome = resolve_hit(&dex, MoveId(33), &attacker, &defender, &mut rng).unwrap();
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
        let outcome = resolve_hit(&dex, MoveId(129), &attacker, &defender, &mut rng).unwrap();
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
        let outcome = resolve_hit(&dex, MoveId(129), &attacker, &defender, &mut rng).unwrap();
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
        let outcome = resolve_hit(&dex, MoveId(163), &attacker, &defender, &mut rng).unwrap();
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
        let outcome = resolve_hit(&dex, MoveId(33), &attacker, &defender, &mut rng).unwrap();
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
    fn zero_power_moves_are_reported_as_unsupported() {
        let dex = Dex::new();
        let attacker = mon(&dex, 1, 5, vec![MoveId(45)]); // Growl (status move)
        let defender = mon(&dex, 7, 5, vec![MoveId(33)]);
        let mut rng = SequenceRng::new([]);
        assert_eq!(
            resolve_hit(&dex, MoveId(45), &attacker, &defender, &mut rng),
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
        ] {
            assert!(
                dex.move_data(move_id).unwrap().power > 0,
                "{what}: the point of this case is that power alone does not filter it"
            );
            let mut rng = SequenceRng::new([]);
            assert_eq!(
                resolve_hit(&dex, move_id, &attacker, &defender, &mut rng),
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
}
