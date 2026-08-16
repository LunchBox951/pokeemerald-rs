//! Multi-hit moves (S-6, issue #293): `BattleScript_EffectMultiHit` —
//! `EFFECT_MULTI_HIT`, carried by Double Slap, Arm Thrust, Fury Attack, Pin
//! Missile, Bullet Seed and friends.
//!
//! ```text
//! BattleScript_EffectMultiHit::                    @ data/battle_scripts_1.s:604
//!     attackcanceler
//!     accuracycheck BattleScript_PrintMoveMissed, ACC_CURR_MOVE   @ ONCE, :606
//!     attackstring / ppreduce
//!     setmultihitcounter 0                                        @ :609
//!     initmultihitstring / setbyte sMULTIHIT_EFFECT, 0
//! BattleScript_MultiHitLoop::                                     @ :612
//!     jumpifhasnohp BS_ATTACKER, BattleScript_MultiHitEnd
//!     jumpifhasnohp BS_TARGET,   BattleScript_MultiHitPrintStrings
//!     ...
//!     critcalc / damagecalc / typecalc                            @ :620-:622
//!     jumpifmovehadnoeffect BattleScript_MultiHitNoMoreHits       @ :623
//!     adjustnormaldamage                                          @ :624
//!     ... animation, healthbarupdate, datahpupdate ...
//!     decrementmultihit BattleScript_MultiHitLoop                 @ :639
//! BattleScript_MultiHitEnd::                                      @ :650
//!     seteffectwithchance                                         @ :651, ONCE
//!     tryfaintmon BS_TARGET
//! ```
//!
//! # Once vs. per hit — the whole reason this is its own pipeline
//!
//! | step | frequency |
//! |---|---|
//! | `accuracycheck` | **once**, before the loop (`:606`) — a multi-hit move hits all its hits or none of them |
//! | `setmultihitcounter` | once (`:609`) |
//! | `critcalc` | **per hit** (`:620`) — each hit rolls its own crit |
//! | `damagecalc`/`typecalc`/`adjustnormaldamage` | **per hit** (`:621`-`:624`) — each hit rolls its own `85..=100%` |
//! | `seteffectwithchance` | **once**, after the whole sequence (`:651`) |
//!
//! So per landed hit the cost is **2** (crit + damage roll), around a
//! fixed **1 + 1..2 + 1** of accuracy, hit count and effect chance. A 3-hit
//! Double Slap costs **9 or 10**; a missed one costs **1**.
//!
//! (`seteffectwithchance` running once at the end is also why Twineedle, the
//! other user of this loop, poisons at most once rather than per hit — the
//! `sMULTIHIT_EFFECT` byte is copied into `cEFFECT_CHOOSER` every iteration
//! at `:619` but nothing consumes it until the end.)
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
//! A first draw of `0`/`1` settles it at 2 or 3 for **one** draw; a first
//! draw of `2`/`3` **redraws** and takes `(second & 3) + 2`, i.e. 2..5, for
//! **two**. The resulting distribution is the familiar 3/8, 3/8, 1/8, 1/8
//! over 2, 3, 4, 5 — but reproducing it by sampling that distribution
//! directly would spend the wrong number of draws half the time, so
//! [`roll_hit_count`] reproduces the branch instead `(behavioral-fidelity)`.
//! Skill Link does not exist in Emerald; there is no ability branch here.
//!
//! # Stopping early
//!
//! Three of the loop's guards can end the sequence before the counter runs
//! out, and each abandons the remaining hits **without spending their
//! draws**:
//!
//! - `jumpifhasnohp BS_TARGET` (`:614`) — the target fainted, checked at the
//!   *top* of the next iteration rather than at the moment of the KO, so the
//!   killing hit completes normally and "Hit N time(s)!" reports the hits
//!   that landed;
//! - `jumpifmovehadnoeffect` (`:623`) — a mid-sequence immunity, which costs
//!   that iteration its crit draw but **not** its damage roll, since it
//!   jumps before `adjustnormaldamage`;
//! - `MOVE_RESULT_FOE_ENDURED` (`:638`) — Endure/Focus Band, neither
//!   modelled.
//!
//! Only the first is reachable here, and [`resolve_multi_hit`] leaves it to
//! the caller: the loop needs the target's live HP after each hit, which
//! only [`crate::battle::Battle`] owns. The immunity case is handled inside
//! this module, because a type-immune multi-hit move produces
//! [`crate::hit::HitOutcome::NoEffect`] on its very first iteration and can
//! stop there.

use assets::{MoveEffect, MoveId};

use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::hit::accuracy_roll;
use crate::pokemon::BattlePokemon;

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
    // `Random() & 3` on a `u16`, narrowed: the mask leaves 0..=3.
    #[allow(clippy::cast_possible_truncation)]
    let first = (rng.next_u16() & 3) as u8;
    if first > 1 {
        #[allow(clippy::cast_possible_truncation)]
        let second = (rng.next_u16() & 3) as u8;
        second + 2
    } else {
        first + 2
    }
}

/// Whether [`resolve_multi_hit`] can resolve `move_id`.
///
/// # Errors
///
/// - [`BattleError::UnknownMove`] if `move_id` is not in `dex`.
/// - [`BattleError::UnsupportedMoveEffect`] if its `EFFECT_*` is not
///   [`EFFECT_MULTI_HIT`].
pub fn ensure_resolvable(dex: &Dex, move_id: MoveId) -> Result<(), BattleError> {
    if is_multi_hit_effect(dex.move_data(move_id)?.effect) {
        Ok(())
    } else {
        Err(BattleError::UnsupportedMoveEffect(move_id))
    }
}

/// The opening of `BattleScript_EffectMultiHit`: the one accuracy check
/// (`:606`) and, if it passes, the hit-count roll (`:609`).
///
/// Returns `None` for a miss — **1 draw**, and the whole move is over.
/// Returns `Some(hits)` otherwise, having spent **2 or 3** draws (accuracy
/// plus [`roll_hit_count`]'s one or two).
///
/// The per-hit loop is deliberately *not* here: it needs the target's HP
/// after each hit to reproduce `jumpifhasnohp BS_TARGET` (`:614`), and only
/// [`crate::battle::Battle`] owns that. The caller runs
/// [`crate::hit::damage_core`] once per hit and then spends the single
/// trailing `seteffectwithchance` draw ([`spend_effect_chance_draw`]).
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

/// The single trailing `seteffectwithchance` draw at `:651`, which runs once
/// per *move* rather than once per hit and is discarded for every
/// `EFFECT_MULTI_HIT` move (none carries a secondary effect; Twineedle,
/// which does, is a different effect id and not modelled).
///
/// A named function rather than a bare `rng.next_u16()` at the call site so
/// the reason the draw exists travels with it.
pub fn spend_effect_chance_draw(rng: &mut impl BattleRng) {
    let _ = rng.next_u16();
}

#[cfg(test)]
#[path = "multi_hit/tests.rs"]
mod tests;
