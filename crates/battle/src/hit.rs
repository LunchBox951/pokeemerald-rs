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
//! So, **when `suppress_crit` is `false`** (every battle but a first
//! battle — see below), one move resolution costs:
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
//! step 2 draw **nothing** as well. The first two of those three still don't
//! exist anywhere in this crate (no abilities, no status3), but the third
//! does, as of issue #187: [`crate::battle::Battle`]'s `first_battle` flag
//! passes `suppress_crit = true` into every [`resolve_hit`] call for the
//! whole battle, dropping every row above by exactly one draw (accuracy-only
//! on a miss, unaffected; **3** for an ordinary hit, **2** for an
//! accuracy-bypassing hit, **2** for a landed Struggle) and forcing
//! [`HitOutcome::Hit`]'s `is_critical` field to `false` regardless of what
//! the dropped roll would have produced — see [`crate::critical`]'s module
//! docs. (Serene Grace doubling `percentChance` in step 7 similarly cannot
//! matter: no abilities, and the draw's value is discarded for every
//! allow-listed move regardless.)
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
use crate::critical::{crit_adjusted_stages, crit_roll, crit_stage};
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
/// `suppress_crit` reproduces `Cmd_critcalc`'s short-circuiting `&&` chain
/// (`battle_script_commands.c:1279`-`:1283`, [`crate::critical`]'s module
/// docs): pass `true` when any of its three suppressors is in play —
/// currently just `BATTLE_TYPE_WALLY_TUTORIAL | BATTLE_TYPE_FIRST_BATTLE`
/// (`:1281`), which [`crate::battle::Battle`] does via its `first_battle`
/// flag — to skip the crit roll **and its RNG draw** entirely, exactly as
/// [`crate::critical::crit_roll`]'s own docs require of a caller that gains
/// one of the suppressors, rather than drawing and discarding.
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
    suppress_crit: bool,
    rng: &mut impl BattleRng,
) -> Result<HitOutcome, BattleError> {
    ensure_resolvable(dex, move_id)?;
    if !accuracy_roll(dex, move_id, attacker, defender, rng)? {
        return Ok(HitOutcome::Miss);
    }
    let outcome = damage_core(dex, move_id, attacker, defender, suppress_crit, rng)?;

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
    Ok(outcome)
}

/// Step 1 of the pipeline in isolation: `Cmd_accuracycheck`
/// (`battle_script_commands.c:1099`), returning whether the move connected.
///
/// Draws **0 or 1** — nothing at all for an `EFFECT_ALWAYS_HIT` /
/// `EFFECT_VITAL_THROW` move ([`crate::accuracy::always_hits`]), one
/// otherwise, even when the accuracy arithmetic guarantees a hit.
///
/// Split out (issue #293) because the damaging *variants* share this step
/// verbatim while differing in everything after it: `BattleScript_EffectAbsorb`
/// (`data/battle_scripts_1.s:325`), `BattleScript_EffectSonicboom` (`:1722`)
/// and `BattleScript_EffectMultiHit` (`:606`) each open with the identical
/// `accuracycheck BattleScript_PrintMoveMissed, ACC_CURR_MOVE`, and the
/// multi-hit one runs it **once for the whole move** rather than per hit.
///
/// # Errors
///
/// [`BattleError::UnknownMove`] if `move_id` is not in `dex`. Unlike
/// [`resolve_hit`] this does *not* apply the allow-list — its callers are
/// the variant pipelines, each of which screens its own effect.
pub fn accuracy_roll(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
    rng: &mut impl BattleRng,
) -> Result<bool, BattleError> {
    let mv = dex.move_data(move_id)?;
    Ok(accuracy_check(
        mv.accuracy,
        mv.effect,
        attacker.stages().accuracy,
        defender.stages().evasion,
        rng,
    ))
}

/// Steps 2-6 of the pipeline — everything between the accuracy check and
/// `Cmd_seteffectwithchance`: `Cmd_critcalc`, `Cmd_damagecalc` (base damage,
/// the crit `x2`, and Charge's Electric `x2`), then `Cmd_typecalc`'s STAB
/// and type rows, then `ApplyRandomDmgMultiplier`'s `85..=100%` roll.
///
/// **Draws 2, or 1 when `suppress_crit`** — the crit roll and the damage
/// roll, the latter unconditionally, immunity included. Assumes the accuracy
/// check has already passed ([`accuracy_roll`]); it makes no accuracy draw
/// of its own.
///
/// Shared verbatim by [`resolve_hit`] and by [`crate::drain`], which differs
/// from the plain script only *outside* this range. [`crate::multi_hit`]
/// uses [`damage_before_roll`] instead, because its script jumps past the
/// damage roll on an immune iteration. [`crate::fixed_damage`] uses neither:
/// `BattleScript_EffectSonicboom` has no `critcalc`, `damagecalc` or
/// `adjustnormaldamage` at all.
///
/// # Errors
///
/// [`BattleError::UnknownMove`] if `move_id` is not in `dex`, or
/// [`BattleError::UnsupportedMoveType`] for the sole `???`-typed move.
pub fn damage_core(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
    suppress_crit: bool,
    rng: &mut impl BattleRng,
) -> Result<HitOutcome, BattleError> {
    let (damage, is_critical) =
        damage_before_roll(dex, move_id, attacker, defender, suppress_crit, rng)?;
    // `ApplyRandomDmgMultiplier` calls `Random()` at
    // `battle_script_commands.c:1641`, *before* its own
    // `gBattleMoveDamage != 0` guard at `:1644` -- so even a type-immune hit
    // that reaches `adjustnormaldamage` spends this draw.
    let damage = apply_damage_roll(damage, rng);
    if damage == 0 {
        Ok(HitOutcome::NoEffect)
    } else {
        Ok(HitOutcome::Hit {
            damage,
            is_critical,
        })
    }
}

/// [`damage_core`] **minus its final `ApplyRandomDmgMultiplier` roll**:
/// `Cmd_critcalc`, `Cmd_damagecalc` (base damage, the crit `x2`, Charge's
/// Electric `x2`) and `Cmd_typecalc` (STAB, then the type rows), returning
/// the damage the roll would scale and whether the hit crit.
///
/// **Draws 1, or 0 when `suppress_crit`** — the crit roll and nothing else.
///
/// Split out because `BattleScript_EffectMultiHit` is the one script that
/// looks at the type verdict *between* the two:
///
/// ```text
/// critcalc / damagecalc / typecalc                       @ :620-:622
/// jumpifmovehadnoeffect BattleScript_MultiHitNoMoreHits  @ :623
/// adjustnormaldamage                                     @ :624
/// ```
///
/// so an immune multi-hit iteration spends the crit draw and then **jumps
/// past the damage roll**, where the plain hit script (`:249`-`:252`) and
/// `BattleScript_EffectAbsorb` (`:328`-`:331`) have no such jump and always
/// spend both. Reproducing that is one RNG value, which is one desynchronised
/// stream `(behavioral-fidelity)`.
///
/// A returned damage of `0` is exactly `MOVE_RESULT_DOESNT_AFFECT_FOE`: the
/// only way this arithmetic reaches zero is a type immunity, since
/// `CalculateBaseDamage`'s physical branch floors at `1` and
/// `ModulateDmgByType` re-floors any surviving non-immune row
/// (`battle_script_commands.c:1323`-`:1325`).
///
/// # Errors
///
/// [`BattleError::UnknownMove`] if `move_id` is not in `dex`, or
/// [`BattleError::UnsupportedMoveType`] for the sole `???`-typed move.
pub fn damage_before_roll(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
    suppress_crit: bool,
    rng: &mut impl BattleRng,
) -> Result<(u32, bool), BattleError> {
    let mv = dex.move_data(move_id)?;
    let move_type: Type = mv
        .move_type
        .battle_type()
        .ok_or(BattleError::UnsupportedMoveType(move_id))?;
    let category = MoveCategory::for_type(move_type);

    // No crit stage is even computed when suppressed: nothing downstream
    // reads it, and computing it would falsely imply the draw still mattered.
    let is_critical = if suppress_crit {
        false
    } else {
        // `Cmd_critcalc`'s stage includes the attacker's own
        // `STATUS2_FOCUS_ENERGY` (`:1267`), worth `+2` -- see
        // `crate::critical::crit_stage`.
        let stage = crit_stage(mv.effect, attacker.volatiles().focus_energy);
        crit_roll(stage, rng)
    };

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
    // `Cmd_damagecalc`'s very next statement (issue #293): `if
    // (gStatuses3[attacker] & STATUS3_CHARGED_UP && gBattleMoves[move].type
    // == TYPE_ELECTRIC) gBattleMoveDamage *= 2`
    // (`battle_script_commands.c:1298`-`:1299`) -- after the crit multiply,
    // before `Cmd_typecalc`'s STAB, and keyed on the move's **static**
    // `gBattleMoves[].type` rather than a dynamic one, so a hypothetical
    // Electric Hidden Power would not be boosted. See `crate::volatile`.
    if attacker.volatiles().charged_up() && move_type == Type::Electric {
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
    Ok((damage, is_critical))
}

#[cfg(test)]
#[path = "hit/tests.rs"]
mod tests;
