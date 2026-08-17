//! Fixed-damage moves (S-6, issue #293): `BattleScript_EffectSonicboom` and
//! its byte-identical twin `BattleScript_EffectDragonrage`.
//!
//! `BattleScript_EffectSonicboom` (`data/battle_scripts_1.s:1720`-`:1729`)
//! runs, in order: the attack canceler, the accuracy check (a miss prints
//! the ordinary missed string and ends the move), the attack string and PP
//! deduction, `typecalc`, a mask that clears the super-/not-very-effective
//! result bits, a literal store of `20` into the damage word, the
//! roll-free damage adjustment (`adjustsetdamage`), and a jump into the
//! plain hit script's animation tail. `BattleScript_EffectDragonrage`
//! (`:819`-`:828`) differs in exactly one byte — its literal is `40`. Both
//! literals are recorded in [`FIXED_DAMAGE_EFFECTS`].
//!
//! # Four things this script does *not* do
//!
//! Everything the plain hit script computes between `ppreduce` and the
//! animation is simply absent, and each absence is observable:
//!
//! 1. **No `critcalc`.** `gCritMultiplier` stays `1` (reset by
//!    `MoveValuesCleanUp`, `src/battle_script_commands.c:3621`), so Sonic
//!    Boom can never crit **and never spends the crit draw**.
//! 2. **No `damagecalc`.** The damage is a literal `setword`, so no stat,
//!    stage, level, burn, Reflect or Charge term reaches it. (There is no
//!    `Cmd_setfixeddamage` in Emerald — the "fixed damage" is just this
//!    store.)
//! 3. **No STAB, in effect.** `typecalc` *does* run first and *does* apply
//!    STAB to `gBattleMoveDamage` (`:1369`-`:1373`) — but the very next
//!    instruction overwrites the whole value with the literal, so the STAB
//!    multiply is dead. Damage is exactly `20`, always.
//! 4. **No damage roll.** `Cmd_adjustsetdamage` (`:5861`) is
//!    `Cmd_adjustnormaldamage` minus the `ApplyRandomDmgMultiplier()` call
//!    (compare `:5861`-`:5863` with `:1658`-`:1662`), so the `85..=100%`
//!    roll never happens either.
//!
//! # What it *does* do
//!
//! **Type immunity still applies.** `typecalc` runs at `:1725` and
//! `ModulateDmgByType` sets `MOVE_RESULT_DOESNT_AFFECT_FOE` on a
//! `TYPE_MUL_NO_EFFECT` row regardless of the move's power
//! (`:1329`-`:1333`); the `bicbyte` at `:1726` clears only the
//! super-/not-very-effective bits, **not** that one, and `Cmd_datahpupdate`
//! gates all damage on `!(gMoveResultFlags & MOVE_RESULT_NO_EFFECT)`
//! (`:1862`). So Sonic Boom, a Normal move, does nothing at all to a Ghost —
//! while a merely resisted or super-effective matchup still takes the flat
//! 20, because the multiplier itself was thrown away.
//!
//! **`seteffectwithchance` still runs.** The script's tail is
//! `goto BattleScript_HitFromAtkAnimation` (`:1729`), which lands inside the
//! plain hit script at `:253` and therefore reaches `:265`. Sonic Boom's
//! `secondaryEffectChance` is `0`, so nothing fires — but the leading
//! `Random() % 100` operand at `:2923` is still consumed and discarded.
//!
//! # RNG draws
//!
//! | outcome | draws | which |
//! |---|---|---|
//! | missed | **1** | accuracy |
//! | landed | **2** | accuracy, `seteffectwithchance`'s discarded roll |
//! | type-immune | **2** | the same two — the immunity is only discovered at `typecalc`, after the accuracy roll, and is too late to suppress the effect-chance draw |
//!
//! Two, not four: a caller scripting a stream must budget one fewer than an
//! ordinary hit for the missing crit roll and one fewer again for the
//! missing damage roll.

use assets::{MoveEffect, MoveId, Type};

use crate::damage::{apply_dual_type_effectiveness, BattleRng};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::hit::{accuracy_roll, HitOutcome};
use crate::pokemon::BattlePokemon;

/// `EFFECT_SONICBOOM` (`include/constants/battle_move_effects.h:134`): Sonic
/// Boom's effect id. (`49` is the *move* id, `MOVE_SONIC_BOOM`, and `41` is
/// `EFFECT_DRAGON_RAGE` — three easily-confused numbers.)
pub const EFFECT_SONICBOOM: MoveEffect = MoveEffect(130);

/// `EFFECT_DRAGON_RAGE` (`:45`): Dragon Rage's effect id.
pub const EFFECT_DRAGON_RAGE: MoveEffect = MoveEffect(41);

/// The two fixed-damage effects and the literal each one's `setword` stores
/// into `gBattleMoveDamage`.
pub const FIXED_DAMAGE_EFFECTS: [(MoveEffect, u32); 2] = [
    (EFFECT_SONICBOOM, 20),   // data/battle_scripts_1.s:1727
    (EFFECT_DRAGON_RAGE, 40), // :826
];

/// The flat damage `effect`'s script stores, or `None` if it is not a
/// fixed-damage effect.
#[must_use]
pub fn fixed_damage_for_effect(effect: MoveEffect) -> Option<u32> {
    FIXED_DAMAGE_EFFECTS
        .iter()
        .find(|(id, _)| *id == effect)
        .map(|(_, damage)| *damage)
}

/// Whether `effect` runs `BattleScript_EffectSonicboom`'s shape.
#[must_use]
pub fn is_fixed_damage_effect(effect: MoveEffect) -> bool {
    fixed_damage_for_effect(effect).is_some()
}

/// Whether [`resolve_fixed_damage_move`] can resolve `move_id` — checked
/// before any state or RNG is touched, the same contract every other
/// pipeline's `ensure_resolvable` follows.
///
/// # Errors
///
/// - [`BattleError::UnknownMove`] if `move_id` is not in `dex`.
/// - [`BattleError::UnsupportedMoveEffect`] if its `EFFECT_*` is neither
///   fixed-damage effect.
/// - [`BattleError::UnsupportedMoveType`] for a `???`-typed move, which
///   `Cmd_typecalc` could not classify.
pub fn ensure_resolvable(dex: &Dex, move_id: MoveId) -> Result<(), BattleError> {
    let mv = dex.move_data(move_id)?;
    if !is_fixed_damage_effect(mv.effect) {
        return Err(BattleError::UnsupportedMoveEffect(move_id));
    }
    if mv.move_type.battle_type().is_none() {
        return Err(BattleError::UnsupportedMoveType(move_id));
    }
    Ok(())
}

/// Resolve `attacker`'s fixed-damage move against `defender`.
///
/// Draws exactly as the module docs' table says: **1** on a miss, **2**
/// otherwise. Returns [`HitOutcome::NoEffect`] for a type-immune matchup and
/// [`HitOutcome::Hit`] with `is_critical: false` (this script cannot crit)
/// and the script's literal damage otherwise.
///
/// # Errors
///
/// Whatever [`ensure_resolvable`] reports; it draws nothing before that
/// check.
pub fn resolve_fixed_damage_move(
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
    let flat =
        fixed_damage_for_effect(mv.effect).ok_or(BattleError::UnsupportedMoveEffect(move_id))?;

    if !accuracy_roll(dex, move_id, attacker, defender, rng)? {
        return Ok(HitOutcome::Miss);
    }

    // `typecalc` at `:1725`, read for its immunity verdict only: the
    // `bicbyte` at `:1726` throws away the super-/not-very-effective bits
    // and the `setword` at `:1727` throws away the magnitude, so a x0.5 or
    // x2 matchup still takes exactly `flat`. Probing the immunity through
    // `apply_dual_type_effectiveness` over a non-zero seed reuses the one
    // type fold this crate has rather than a second, divergent copy.
    let immune = apply_dual_type_effectiveness(flat, move_type, defender.types()) == 0;

    // `goto BattleScript_HitFromAtkAnimation` lands in the plain hit script
    // above its `seteffectwithchance` (`:265`), so the discarded
    // effect-chance draw happens here too -- immunity included, exactly as
    // in `crate::hit`'s step 7.
    let _ = rng.next_u16();

    if immune {
        Ok(HitOutcome::NoEffect)
    } else {
        Ok(HitOutcome::Hit {
            damage: flat,
            is_critical: false,
        })
    }
}

#[cfg(test)]
#[path = "fixed_damage/tests.rs"]
mod tests;
