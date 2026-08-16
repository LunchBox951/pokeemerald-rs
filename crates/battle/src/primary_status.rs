//! Status moves whose whole effect is a condition (S-6, issue #293): the
//! `setmoveeffect X; seteffectprimary` scripts.
//!
//! Two so far — `EFFECT_PARALYZE` (Thunder Wave, Stun Spore, Glare) and
//! `EFFECT_CONFUSE` (Supersonic, Confuse Ray, Sweet Kiss). They share a
//! shape but not a script, and the differences are exactly where the RNG
//! goes.
//!
//! # `BattleScript_EffectParalyze` (`data/battle_scripts_1.s:1007`-`:1025`)
//!
//! ```text
//! attackcanceler / attackstring / ppreduce
//! jumpifability BS_TARGET, ABILITY_LIMBER, BattleScript_LimberProtected
//! jumpifstatus2 BS_TARGET, STATUS2_SUBSTITUTE, BattleScript_ButItFailed
//! typecalc                                                   @ :1013
//! jumpifmovehadnoeffect BattleScript_ButItFailed             @ :1014
//! jumpifstatus BS_TARGET, STATUS1_PARALYSIS, BattleScript_AlreadyParalyzed
//! jumpifstatus BS_TARGET, STATUS1_ANY, BattleScript_ButItFailed
//! accuracycheck BattleScript_ButItFailed, ACC_CURR_MOVE      @ :1017  <-- the only draw
//! jumpifsideaffecting BS_TARGET, SIDE_STATUS_SAFEGUARD, ...
//! setmoveeffect MOVE_EFFECT_PARALYSIS / seteffectprimary     @ :1021-:1022
//! ```
//!
//! Two orderings here are load-bearing and easy to get backwards:
//!
//! 1. **`ppreduce` is third**, before every failure branch, so a Thunder
//!    Wave that fails for *any* reason still spends its PP.
//! 2. **`typecalc` and both status checks come before `accuracycheck`.** A
//!    Thunder Wave aimed at a Ground type, or at an already-statused target,
//!    therefore costs **zero** draws — it never reaches the accuracy roll.
//!    Only a move that gets as far as actually being able to land pays for
//!    the roll.
//!
//! Note the type immunity is a property of the *move* (Electric vs Ground),
//! not of paralysis: Gen 3 has no Electric-type immunity to paralysis at
//! all, and Stun Spore, being Grass, is stopped by nothing here. See
//! [`crate::status`]'s module docs.
//!
//! # `BattleScript_EffectConfuse` (`:903`-`:918`)
//!
//! ```text
//! attackcanceler / attackstring / ppreduce
//! jumpifability BS_TARGET, ABILITY_OWN_TEMPO, BattleScript_OwnTempoPrevents
//! jumpifstatus2 BS_TARGET, STATUS2_SUBSTITUTE, BattleScript_ButItFailed
//! jumpifstatus2 BS_TARGET, STATUS2_CONFUSION,  BattleScript_AlreadyConfused
//! accuracycheck BattleScript_ButItFailed, ACC_CURR_MOVE      @ :910  <-- draw 1
//! jumpifsideaffecting BS_TARGET, SIDE_STATUS_SAFEGUARD, ...
//! setmoveeffect MOVE_EFFECT_CONFUSION / seteffectprimary     @ :914-:915  <-- draw 2
//! ```
//!
//! **There is no `typecalc` in this script at all**, so Supersonic has no
//! type-based failure: a Ghost, a Ground type, anything is confusable. And
//! it costs a **second** draw the paralysis script does not:
//! `SetMoveEffect`'s `MOVE_EFFECT_CONFUSION` case rolls the duration,
//! `STATUS2_CONFUSION_TURN(((Random()) % 4) + 2)`
//! (`src/battle_script_commands.c:2541`) — `% 4`, not `& 3`, though the two
//! agree for every value.
//!
//! # RNG draws
//!
//! | move | outcome | draws |
//! |---|---|---|
//! | `EFFECT_PARALYZE` | type-immune, or target already statused | **0** |
//! | | missed | **1** (accuracy) |
//! | | landed | **1** (accuracy; `SetMoveEffect` adds none for paralysis) |
//! | `EFFECT_CONFUSE` | target already confused | **0** |
//! | | missed | **1** |
//! | | landed | **2** (accuracy, then the 2..5-turn duration) |
//!
//! # Not modelled
//!
//! Limber and Own Tempo (abilities), Substitute, and Safeguard (a side
//! status). None exists in this crate and none draws, so each branch is
//! unreachable rather than skipped — the same posture
//! [`crate::stat_change`] takes for its own ability guards.

use assets::{MoveEffect, MoveId, Type};

use crate::damage::{apply_dual_type_effectiveness, BattleRng};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::hit::accuracy_roll;
use crate::pokemon::BattlePokemon;
use crate::status::{can_inflict, Status1};

/// `EFFECT_PARALYZE` (`include/constants/battle_move_effects.h:71`): Thunder
/// Wave's and Stun Spore's effect id.
pub const EFFECT_PARALYZE: MoveEffect = MoveEffect(67);

/// `EFFECT_CONFUSE` (`:53`): Supersonic's effect id.
pub const EFFECT_CONFUSE: MoveEffect = MoveEffect(49);

/// The two effect ids this module executes.
const PRIMARY_STATUS_EFFECTS: [MoveEffect; 2] = [EFFECT_PARALYZE, EFFECT_CONFUSE];

/// Whether `effect` runs one of the two scripts this module reproduces.
#[must_use]
pub fn is_primary_status_effect(effect: MoveEffect) -> bool {
    PRIMARY_STATUS_EFFECTS.contains(&effect)
}

/// What one of the two scripts did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrimaryStatusOutcome {
    /// The accuracy check failed (`BattleScript_ButItFailed` via
    /// `accuracycheck`'s own jump).
    Miss,
    /// The move connected but did nothing: the target was type-immune
    /// (`EFFECT_PARALYZE` only, `jumpifmovehadnoeffect` at `:1014`), already
    /// carried the very status this move inflicts
    /// (`BattleScript_AlreadyParalyzed` / `AlreadyConfused`), or carried
    /// some *other* non-volatile status (`jumpifstatus BS_TARGET,
    /// STATUS1_ANY, BattleScript_ButItFailed`).
    ///
    /// Every one of those branches sits **before** the accuracy check, so
    /// this outcome costs no draws at all.
    Failed,
    /// The target is now paralysed.
    Paralysed,
    /// The target is now confused, for this many turns
    /// (`STATUS2_CONFUSION_TURN(((Random()) % 4) + 2)`, so `2..=5`).
    Confused(u8),
}

/// Whether [`resolve_primary_status_move`] can resolve `move_id` — checked
/// before any state or RNG is touched.
///
/// # Errors
///
/// - [`BattleError::UnknownMove`] if `move_id` is not in `dex`.
/// - [`BattleError::UnsupportedMoveEffect`] if its `EFFECT_*` is neither of
///   the two.
/// - [`BattleError::UnsupportedMoveType`] for a `???`-typed move, which
///   `EFFECT_PARALYZE`'s `typecalc` could not classify.
pub fn ensure_resolvable(dex: &Dex, move_id: MoveId) -> Result<(), BattleError> {
    let mv = dex.move_data(move_id)?;
    if !is_primary_status_effect(mv.effect) {
        return Err(BattleError::UnsupportedMoveEffect(move_id));
    }
    if mv.move_type.battle_type().is_none() {
        return Err(BattleError::UnsupportedMoveType(move_id));
    }
    Ok(())
}

/// Resolve `attacker`'s status move against `defender`.
///
/// Draws per the module docs' table — **0** when a pre-accuracy branch
/// fails the move, **1** for paralysis, **2** for a landed confusion.
///
/// Decides but does not mutate: [`crate::battle::Battle`] owns the target.
///
/// # Errors
///
/// Whatever [`ensure_resolvable`] reports; nothing is drawn before that
/// check.
pub fn resolve_primary_status_move(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
    rng: &mut impl BattleRng,
) -> Result<PrimaryStatusOutcome, BattleError> {
    ensure_resolvable(dex, move_id)?;
    let mv = dex.move_data(move_id)?;

    if mv.effect == EFFECT_CONFUSE {
        // `jumpifstatus2 BS_TARGET, STATUS2_CONFUSION` (`:909`), before the
        // accuracy check. No `typecalc` in this script at all.
        if defender.volatiles().is_confused() {
            return Ok(PrimaryStatusOutcome::Failed);
        }
        if !accuracy_roll(dex, move_id, attacker, defender, rng)? {
            return Ok(PrimaryStatusOutcome::Miss);
        }
        return Ok(PrimaryStatusOutcome::Confused(
            crate::volatile::roll_confusion_turns(rng),
        ));
    }

    // `EFFECT_PARALYZE`. `typecalc` (`:1013`) then `jumpifmovehadnoeffect`
    // (`:1014`): a Ground type stops Thunder Wave here, before the roll.
    // Probed over a non-zero seed, the same way `crate::fixed_damage` reads
    // its own immunity verdict.
    let move_type: Type = mv
        .move_type
        .battle_type()
        .ok_or(BattleError::UnsupportedMoveType(move_id))?;
    if apply_dual_type_effectiveness(64, move_type, defender.types()) == 0 {
        return Ok(PrimaryStatusOutcome::Failed);
    }
    // `jumpifstatus BS_TARGET, STATUS1_PARALYSIS` (`:1015`) and
    // `jumpifstatus BS_TARGET, STATUS1_ANY` (`:1016`) -- two branches, one
    // question here, since both land on a no-op with no draw and this
    // crate emits no messages to tell them apart.
    if !can_inflict(defender, Status1::Paralysed) {
        return Ok(PrimaryStatusOutcome::Failed);
    }
    if !accuracy_roll(dex, move_id, attacker, defender, rng)? {
        return Ok(PrimaryStatusOutcome::Miss);
    }
    // `seteffectprimary` -> `SetMoveEffect(TRUE, 0)`: **no draw** for
    // paralysis (`src/battle_script_commands.c:2483`, the non-sleep arm).
    Ok(PrimaryStatusOutcome::Paralysed)
}

#[cfg(test)]
#[path = "primary_status/tests.rs"]
mod tests;
