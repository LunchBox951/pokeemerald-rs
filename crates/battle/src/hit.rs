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
//! 3. damage core + crit's stat-stage override ([`crate::critical`]), the
//!    `x2` crit multiply, the attacker's pinch-ability power boost
//!    ([`crate::ability::pinch_boosts_power`]) and Charge's Electric
//!    doubling ([`crate::volatile::Volatiles::charged_up`]) — no draw.
//! 4. STAB (no draw) — skipped for `MOVE_STRUGGLE`.
//! 5. dual-type effectiveness (no draw); a `NoEffect` row is terminal for
//!    the *outcome* but *not* for the RNG — see step 7 — and the type step
//!    is skipped entirely for `MOVE_STRUGGLE`.
//! 6. the `85..=100%` random roll (always 1 draw, even at `0` damage).
//! 7. the secondary-effect-chance roll, [`crate::secondary`]'s shared hook
//!    (1 draw on every landed hit, `MOVE_STRUGGLE` excepted).
//!
//! Steps 1, 2-6 and 7 are three separate functions —
//! [`accuracy_roll`], [`damage_core`] (or [`damage_before_roll`], which
//! stops one instruction earlier), and
//! [`crate::secondary::spend_effect_chance_draw`] — because the other
//! damaging scripts this crate reproduces reuse *some* of them and not
//! others: [`crate::drain`] runs 1-6 and never reaches 7,
//! [`crate::fixed_damage`] runs 1 and 7 and neither 2 nor 6,
//! [`crate::multi_hit`] runs 1 once, 2-6 per hit (leaving before 6 on an
//! immunity), and 7 once at the end. [`resolve_hit`] is the composition of
//! all three, i.e. `BattleScript_EffectHit` itself.
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
//! exist anywhere in this crate (neither armour ability is modelled — see
//! [`crate::ability`] for the two that are — and there is no
//! `STATUS3_CANT_SCORE_A_CRIT`), but the third
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
//! times: OHKO (`:59`), Counter (`:110`), every secondary-effect-on-hit
//! trampoline (whose `setmoveeffect` makes step 7's discarded draw *land*,
//! applying a status and possibly drawing again inside `SetMoveEffect` —
//! e.g. a sleep-turn roll; [`crate::secondary::SECONDARY_TRAMPOLINES`]
//! lists all 31), and so on. Feeding one of those to [`resolve_hit`] would
//! be silently wrong twice over — wrong damage *and* a desynchronised
//! shared RNG stream — so [`ensure_resolvable`] rejects it up front
//! `(behavioral-fidelity)`. Base power `0` alone is **not** a sufficient
//! filter: Sonic Boom has power `1`.
//!
//! Four of those scripts *are* reproduced now, each by its own module with
//! its own draw table — [`crate::drain`] (`EFFECT_ABSORB`, `:24`),
//! [`crate::fixed_damage`] (`EFFECT_SONICBOOM` at `:151`,
//! `EFFECT_DRAGON_RAGE` at `:62`, `EFFECT_LEVEL_DAMAGE` at `:108`),
//! [`crate::multi_hit`] (`EFFECT_MULTI_HIT`, `:50`) and
//! [`crate::flag_move`] (`EFFECT_SPLASH`/`_FOCUS_ENERGY`/`_CHARGE`) — but
//! none of them belongs *here*, which is the point of the rejection.
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

use crate::ability::pinch_boosts_power;
use crate::accuracy::accuracy_check;
use crate::critical::{crit_adjusted_stages, crit_roll, crit_stage};
use crate::damage::{
    apply_damage_roll, apply_dual_type_effectiveness, apply_stab, base_damage, has_stab, BattleRng,
    DamageInput, MoveCategory, Weather, STRUGGLE,
};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::BattlePokemon;
use crate::secondary::spend_effect_chance_draw;

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

/// Step 1 alone: `Cmd_accuracycheck` for `move_id`, returning whether the
/// move connected.
///
/// Split out of [`resolve_hit`] (issue #321) because every damaging script
/// this crate reproduces opens with the *same* `accuracycheck
/// BattleScript_PrintMoveMissed, ACC_CURR_MOVE` instruction and then
/// diverges — [`crate::drain`], [`crate::fixed_damage`] and
/// [`crate::multi_hit`] all call this rather than each re-deriving which
/// stages feed it. Draws **0 or 1** (see [`crate::accuracy`]).
///
/// # Errors
///
/// [`BattleError::UnknownMove`] if `move_id` is not in `dex`. Nothing is
/// drawn before that lookup.
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

/// The damage a hit would deal *before* `adjustnormaldamage`'s `85..=100%`
/// roll, and whether it crit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RawDamage {
    /// Post-crit, post-STAB, post-type-chart damage. `0` means the target's
    /// typing nullified the move (`MOVE_RESULT_DOESNT_AFFECT_FOE`).
    pub damage: u32,
    /// Whether `critcalc` confirmed a critical hit.
    pub is_critical: bool,
}

/// Steps 2-5: `critcalc` / `damagecalc` / `typecalc`, stopping **before**
/// `adjustnormaldamage`.
///
/// Split out of [`damage_core`] because that boundary is exactly where
/// [`crate::multi_hit`]'s loop leaves on an immunity: its
/// `jumpifmovehadnoeffect` sits at `data/battle_scripts_1.s:623`, one
/// instruction *ahead* of `adjustnormaldamage` at `:624`, so a type-immune
/// multi-hit move spends its crit draw but **not** its damage roll — while
/// the plain hit script spends both. A single fused function could not
/// express both shapes `(behavioral-fidelity)`.
///
/// Draws **1** when `suppress_crit` is `false` and **0** when it is `true`.
///
/// # Errors
///
/// [`BattleError::UnknownMove`] if `move_id` is not in `dex`;
/// [`BattleError::UnsupportedMoveType`] for a `???`-typed move. Nothing is
/// drawn before either check.
pub fn damage_before_roll(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
    suppress_crit: bool,
    rng: &mut impl BattleRng,
) -> Result<RawDamage, BattleError> {
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
        attacker_pinch_boost: pinch_boosts_power(
            attacker.ability(),
            move_type,
            attacker.current_hp(),
            attacker.stats().max_hp,
        ),
    };

    let mut damage = base_damage(&input);
    if is_critical {
        // Cmd_damagecalc: `gBattleMoveDamage *= gCritMultiplier` (2 on a
        // crit), applied to the whole base-damage result before STAB.
        damage *= 2;
    }
    if attacker.volatiles().charged_up() && move_type == Type::Electric {
        // Cmd_damagecalc `:1298`-`:1299`: Charge doubles the *whole*
        // post-crit figure, after the crit multiply and before typecalc.
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

    Ok(RawDamage {
        damage,
        is_critical,
    })
}

/// Steps 2-6: [`damage_before_roll`] plus `adjustnormaldamage`'s
/// `85..=100%` roll — the run that every *non*-multi-hit damage-computing
/// script shares, with **no** `seteffectwithchance` draw of its own. The
/// trailing step-7 draw belongs to the script, and the scripts disagree
/// about it ([`crate::drain`] never reaches it, [`crate::multi_hit`] takes
/// it once for the whole move).
///
/// Draws **2** when `suppress_crit` is `false` (crit roll, damage roll) and
/// **1** when it is `true` — the damage roll happens even at `0` damage.
///
/// Status conditions (burn), Reflect/Light Screen, and weather are not
/// modelled this slice: [`crate::damage::DamageInput`]'s corresponding
/// fields are always the "no effect" value here (`false`/[`Weather::None`]).
/// Two things that *are* modelled, both added by issue #321, both drawing
/// nothing: the attacker's pinch ability
/// ([`crate::ability::pinch_boosts_power`], `CalculateBaseDamage`'s
/// `gBattleMovePower = 150 * gBattleMovePower / 100`, `src/pokemon.c:3219`)
/// and Charge's Electric doubling (`Cmd_damagecalc`'s
/// `STATUS3_CHARGED_UP` test, `battle_script_commands.c:1298`-`:1299`).
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
/// [`BattleError::UnknownMove`] if `move_id` is not in `dex`;
/// [`BattleError::UnsupportedMoveType`] for a `???`-typed move. Nothing is
/// drawn before either check.
pub fn damage_core(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
    suppress_crit: bool,
    rng: &mut impl BattleRng,
) -> Result<HitOutcome, BattleError> {
    let raw = damage_before_roll(dex, move_id, attacker, defender, suppress_crit, rng)?;
    let damage = apply_damage_roll(raw.damage, rng);

    if damage == 0 {
        Ok(HitOutcome::NoEffect)
    } else {
        Ok(HitOutcome::Hit {
            damage,
            is_critical: raw.is_critical,
        })
    }
}

/// Resolve `attacker` using `move_id` against `defender` — the whole
/// `BattleScript_EffectHit` script, i.e. [`accuracy_roll`] then
/// [`damage_core`] then the trailing step-7 draw
/// ([`crate::secondary::spend_effect_chance_draw`]).
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
        spend_effect_chance_draw(dex, move_id, outcome != HitOutcome::NoEffect, rng)?;
    }

    Ok(outcome)
}

#[cfg(test)]
#[path = "hit/tests.rs"]
mod tests;
