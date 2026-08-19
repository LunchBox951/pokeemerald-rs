//! Multi-hit moves (S-6, issue #321): `BattleScript_EffectMultiHit` —
//! `EFFECT_MULTI_HIT`, carried by Double Slap, Fury Attack, Pin Missile,
//! Bullet Seed, Arm Thrust and friends.
//!
//! The script (`pokeemerald/data/battle_scripts_1.s:604`-`:652`) is a
//! prologue, a loop, and an epilogue. The prologue runs the attack canceler,
//! **one** accuracy check (`:606` — a miss ends the whole move), the attack
//! string, the PP deduction, and the hit-count roll (`setmultihitcounter 0`,
//! `:609`). The loop body (`BattleScript_MultiHitLoop`, from `:612`) first
//! bails out if either battler has no HP left (attacker → straight to the
//! epilogue, target → to the hit-count string), then per iteration rolls the
//! crit, computes damage, applies the type chart (`:620`-`:622`), leaves the
//! loop early on a no-effect result (`:623`, **before** the damage roll at
//! `:624`), applies the `85..=100%` roll and the damage, and loops while the
//! counter has hits left (`:639`). The epilogue
//! (`BattleScript_MultiHitEnd`, `:650`) spends **one**
//! `seteffectwithchance` draw (`:651`) and then tries the target's faint.
//!
//! # Once vs. per hit — the whole reason this is its own pipeline
//!
//! | step | frequency |
//! |---|---|
//! | `accuracycheck` | **once**, before the loop (`:606`) — a multi-hit move lands all its hits or none of them |
//! | `setmultihitcounter` | once (`:609`) |
//! | `critcalc` | **per hit** (`:620`) — each hit rolls its own crit |
//! | `damagecalc`/`typecalc`/`adjustnormaldamage` | **per hit** (`:621`-`:624`) — each hit rolls its own `85..=100%` |
//! | `seteffectwithchance` | **once**, after the whole sequence (`:651`) |
//!
//! So a landed hit costs **2** ([`crate::hit::damage_core`]: crit + damage
//! roll) around a fixed `1 + 1..2 + 1` of accuracy, hit count and effect
//! chance. A 3-hit Double Slap costs **9 or 10**; a missed one costs **1**.
//! Under `suppress_crit` each hit costs 1 instead of 2.
//!
//! (`seteffectwithchance` running once at the end is also why Twineedle, the
//! other user of this loop, poisons at most once rather than per hit — the
//! `sMULTIHIT_EFFECT` byte is copied into `cEFFECT_CHOOSER` every iteration
//! at `:619` but nothing consumes it until the end. Twineedle is a different
//! effect id and is not executable here.)
//!
//! # The hit count is a *two*-draw scheme
//!
//! `Cmd_setmultihitcounter` (`src/battle_script_commands.c:7139`-`:7155`),
//! for the `gBattlescriptCurrInstr[1] == 0` case this effect uses:
//!
//! ```text
//! gMultiHitCounter = Random() & 3;
//! if (gMultiHitCounter > 1)
//!     gMultiHitCounter = (Random() & 3) + 2;
//! else
//!     gMultiHitCounter += 2;
//! ```
//!
//! A first `Random() & 3` of `0` or `1` settles the count at 2 or 3 for
//! **one** draw; `2` or `3` **redraws**, masks again, and adds 2 — i.e.
//! 2..=5 — for **two** draws. The resulting distribution is the familiar
//! 3/8, 3/8, 1/8, 1/8 over 2, 3, 4, 5 — but sampling that distribution
//! directly would spend the wrong number of draws half the time, so
//! [`roll_hit_count`] reproduces the *branch* `(behavioral-fidelity)`.
//! Skill Link does not exist in Emerald; there is no ability branch here.
//!
//! # Stopping early
//!
//! Four of the loop's guards can end the sequence before the counter runs
//! out, and each abandons the remaining hits **without spending their
//! draws**:
//!
//! - `jumpifhasnohp BS_ATTACKER` (`:613`) and `jumpifhasnohp BS_TARGET`
//!   (`:614`) — checked at the *top* of the next iteration rather than at
//!   the moment of the KO, so the killing hit completes normally and "Hit N
//!   time(s)!" reports the hits that landed;
//! - `jumpifstatus BS_ATTACKER, STATUS1_SLEEP` (`:616`) — not modelled (no
//!   status1 in this crate; issue #323), and unreachable, since a sleeping
//!   battler's move is cancelled long before the script starts;
//! - `jumpifmovehadnoeffect` (`:623`) — a mid-sequence immunity, which costs
//!   that iteration its crit draw but **not** its damage roll, since it
//!   jumps before `adjustnormaldamage`. Unreachable in practice: typing does
//!   not change between hits, so an immune move is immune on hit 1;
//! - `MOVE_RESULT_FOE_ENDURED` (`:638`) — Endure/Focus Band, neither
//!   modelled.
//!
//! The two HP guards are the reachable ones, and [`resolve_multi_hit`]
//! leaves them to the caller: the loop needs both battlers' live HP after
//! each hit, and only [`crate::battle::Battle`] owns that. This module
//! therefore covers the prologue and the trailing draw, and the caller runs
//! [`crate::hit::damage_core`] once per hit in between — the split is why
//! the loop's *interruptibility* stays honest instead of being flattened
//! into a precomputed damage list.

use assets::{MoveEffect, MoveId};

use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::hit::accuracy_roll;
use crate::pokemon::BattlePokemon;
use crate::secondary::spend_effect_chance_draw;

/// `EFFECT_MULTI_HIT` (`include/constants/battle_move_effects.h:33`).
pub const EFFECT_MULTI_HIT: MoveEffect = MoveEffect(29);

/// The fewest hits `Cmd_setmultihitcounter`'s scheme can produce.
pub const MIN_HITS: u8 = 2;

/// The most hits it can produce.
pub const MAX_HITS: u8 = 5;

/// Whether `effect` runs `BattleScript_EffectMultiHit`.
#[must_use]
pub fn is_multi_hit_effect(effect: MoveEffect) -> bool {
    effect == EFFECT_MULTI_HIT
}

/// `Cmd_setmultihitcounter`'s `gBattlescriptCurrInstr[1] == 0` branch
/// (`src/battle_script_commands.c:7147`-`:7151`), reproduced as the
/// two-stage draw it is rather than as its output distribution — see the
/// module docs.
///
/// Draws **1** when the first `Random() & 3` lands on `0` or `1`, and **2**
/// otherwise. The result is always in [`MIN_HITS`]`..=`[`MAX_HITS`].
#[must_use]
pub fn roll_hit_count(rng: &mut impl BattleRng) -> u8 {
    // No truncation suppression needed: the `& 3` mask bounds the u16 to
    // 0..=3 before the narrowing.
    let first = (rng.next_u16() & 3) as u8;
    if first > 1 {
        (rng.next_u16() & 3) as u8 + 2
    } else {
        first + 2
    }
}

/// Whether [`resolve_multi_hit`] can resolve `move_id` — checked before any
/// state or RNG is touched.
///
/// # Errors
///
/// - [`BattleError::UnknownMove`] if `move_id` is not in `dex`.
/// - [`BattleError::UnsupportedMoveEffect`] if its `EFFECT_*` is not
///   [`EFFECT_MULTI_HIT`].
/// - [`BattleError::UnsupportedMoveType`] for a `???`-typed move, which
///   `Cmd_typecalc` could not classify.
pub fn ensure_resolvable(dex: &Dex, move_id: MoveId) -> Result<(), BattleError> {
    let mv = dex.move_data(move_id)?;
    if !is_multi_hit_effect(mv.effect) {
        return Err(BattleError::UnsupportedMoveEffect(move_id));
    }
    if mv.move_type.battle_type().is_none() {
        return Err(BattleError::UnsupportedMoveType(move_id));
    }
    Ok(())
}

/// The prologue of `BattleScript_EffectMultiHit`: the one accuracy check
/// (`:606`) and, if it passes, the hit-count roll (`:609`).
///
/// Returns `None` for a miss — **1 draw**, and the whole move is over.
/// Returns `Some(hits)` otherwise, having spent **2 or 3** draws (accuracy
/// plus [`roll_hit_count`]'s one or two).
///
/// The per-hit loop is deliberately *not* here (module docs); the caller
/// runs [`crate::hit::damage_core`] per hit and then spends the single
/// trailing draw with [`spend_multi_hit_effect_chance_draw`].
///
/// # Errors
///
/// Whatever [`ensure_resolvable`] reports; nothing is drawn before that
/// check.
pub fn resolve_multi_hit(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
    rng: &mut impl BattleRng,
) -> Result<Option<u8>, BattleError> {
    ensure_resolvable(dex, move_id)?;
    if !accuracy_roll(dex, move_id, attacker, defender, rng)? {
        return Ok(None);
    }
    Ok(Some(roll_hit_count(rng)))
}

/// The single trailing `seteffectwithchance` at `:651`, which runs once per
/// *move* rather than once per hit.
///
/// A named wrapper over [`crate::secondary::spend_effect_chance_draw`]
/// rather than a bare call at the site, so the "once, not per hit" reason
/// travels with it. `any_hit_had_effect` is the epilogue's view of
/// `gMoveResultFlags`: `false` only when the sequence ended on the
/// type-immune branch.
///
/// # Errors
///
/// Whatever [`crate::secondary::spend_effect_chance_draw`] reports — never
/// [`BattleError::UnportedSecondaryEffect`] for an `EFFECT_MULTI_HIT` move,
/// which is not a [`crate::secondary`] trampoline.
pub fn spend_multi_hit_effect_chance_draw(
    dex: &Dex,
    move_id: MoveId,
    any_hit_had_effect: bool,
    rng: &mut impl BattleRng,
) -> Result<(), BattleError> {
    spend_effect_chance_draw(dex, move_id, any_hit_had_effect, rng)
}

#[cfg(test)]
#[path = "multi_hit/tests.rs"]
mod tests;
