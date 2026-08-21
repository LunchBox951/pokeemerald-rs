//! Flag-only moves (S-6, issue #321): the self-targeting scripts whose
//! *entire* effect is setting one [`crate::volatile::Volatiles`] bit (or
//! nothing at all) and printing a string — Splash, Focus Energy and Charge.
//!
//! Each has its own one-off script rather than sharing a tail the way
//! [`crate::stat_change`]'s family does, so all three are transcribed below.
//! What makes them one concept is what they have in common: every one is
//! `MOVE_TARGET_USER`, none has an `accuracycheck`, none computes damage,
//! none reaches `seteffectwithchance`, and **none draws a single
//! `Random()`** — so a caller scripting an RNG sequence budgets exactly
//! **zero** draws for any of them, on every path, failure paths included.
//!
//! # `BattleScript_EffectSplash` (`data/battle_scripts_1.s:1172`-`:1181`)
//!
//! Canceler / attack string / `ppreduce`; the animation; a
//! `GAME_STAT_USED_SPLASH` counter bump; `STRINGID_BUTNOTHINGHAPPENED`; move
//! end. No `accuracycheck`, no `damagecalc`, no state change of any kind:
//! Splash spends a PP and prints its string. `GAME_STAT_USED_SPLASH` is a
//! save-block counter this crate has no home for and is the one thing here
//! left unmodelled.
//!
//! # `BattleScript_EffectFocusEnergy` (`:885`-`:895`)
//!
//! Canceler / string / `ppreduce`, then a jump to "But it failed!" if the
//! **attacker** already carries `STATUS2_FOCUS_ENERGY` (`:889`), then
//! `setfocusenergy` and the "getting pumped" string table.
//!
//! The already-pumped check is the **script's** `jumpifstatus2` at `:889`,
//! not `Cmd_setfocusenergy`'s own `else` branch
//! (`src/battle_script_commands.c:7747`-`:7752`) — that branch is
//! unreachable from this script, so [`resolve_flag_move`] reproduces the
//! script's test and [`crate::volatile::Volatiles::set_focus_energy`] stays
//! unconditional.
//!
//! # `BattleScript_EffectCharge` (`:2297`-`:2306`)
//!
//! Canceler / string / `ppreduce`, `setcharge`, the animation, and
//! `STRINGID_PKMNCHARGINGPOWER`. It has **no failure branch at all**: using
//! Charge while already charged simply restarts the timer.
//!
//! **Gen 3's Charge does not raise Sp. Defense.** The script has no
//! `setstatchanger`/`statbuffchange` pair (compare Defense Curl's
//! `:2019`-`:2020`); the Sp. Def raise is a Gen-4 addition. It also cannot
//! fail and has no accuracy check despite carrying `accuracy = 100` in
//! `gBattleMoves` — an inert byte.
//!
//! # Why Defense Curl is *not* here
//!
//! `BattleScript_EffectDefenseCurl` (`:2014`-`:2025`) sets the Rollout bit
//! and *then* raises the user's Defense through the same `statbuffchange
//! MOVE_EFFECT_AFFECTS_USER` machinery the stat-up scripts use. It is
//! therefore not flag-*only*: half of it is a stat change with its own
//! clamp, its own "won't go any higher!" message and its own ability guards,
//! which is the family issue #322 widens. Splitting it across two slices
//! would leave one of them shipping a Defense Curl that silently skips its
//! stat half, so the whole move stays with the stat-change child. This
//! module's boundary is exactly "the script sets a flag and prints, and does
//! nothing else".
//!
//! # Not modelled
//!
//! `attackcanceler`'s status rolls belong to every move alike (issue #323).
//! `Cmd_setcharge`'s `chargeTimerStartValue`, read only by the
//! battle-recording code, and Splash's `GAME_STAT_USED_SPLASH` are noted
//! above. Snatch — Focus Energy and Charge are both `FLAG_SNATCH_AFFECTED`,
//! Splash's `flags` are `0` — is not modelled anywhere in this crate.

use assets::{MoveEffect, MoveId};

use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::BattlePokemon;

/// `EFFECT_FOCUS_ENERGY` (`include/constants/battle_move_effects.h:51`):
/// Focus Energy's effect id. (`116` is the *move* id, `MOVE_FOCUS_ENERGY`.)
pub const EFFECT_FOCUS_ENERGY: MoveEffect = MoveEffect(47);

/// `EFFECT_SPLASH` (`:89`): Splash's effect id. (`150` is the *move* id.)
pub const EFFECT_SPLASH: MoveEffect = MoveEffect(85);

/// `EFFECT_CHARGE` (`:178`): Charge's effect id. (`268` is the *move* id,
/// `MOVE_CHARGE`; upstream reuses neither number.)
pub const EFFECT_CHARGE: MoveEffect = MoveEffect(174);

/// The three effect ids this module executes, in `battle_move_effects.h` id
/// order.
const FLAG_MOVE_EFFECTS: [MoveEffect; 3] = [EFFECT_FOCUS_ENERGY, EFFECT_SPLASH, EFFECT_CHARGE];

/// Whether `effect`'s battle script is one of the three this module
/// reproduces.
#[must_use]
pub fn is_flag_move_effect(effect: MoveEffect) -> bool {
    FLAG_MOVE_EFFECTS.contains(&effect)
}

/// What one of the three scripts did — one variant per distinct upstream
/// message, since the message is the only thing two of these moves produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FlagMoveOutcome {
    /// Splash: `STRINGID_BUTNOTHINGHAPPENED` ("But nothing happened!").
    /// No state changes.
    NothingHappened,
    /// Focus Energy on an attacker that was not already pumped:
    /// `STATUS2_FOCUS_ENERGY` is set (the caller applies it via
    /// [`crate::volatile::Volatiles::set_focus_energy`]) and
    /// `gFocusEnergyUsedStringIds[B_MSG_GETTING_PUMPED]` prints.
    GettingPumped,
    /// Focus Energy on an attacker that already carried the bit: the
    /// script's `jumpifstatus2` at `:889` diverted to
    /// `BattleScript_ButItFailed`. Nothing changes, nothing else prints.
    Failed,
    /// Charge: the timer is (re)started (the caller applies it via
    /// [`crate::volatile::Volatiles::set_charge`]) and
    /// `STRINGID_PKMNCHARGINGPOWER` prints. Charge has no failure branch.
    ChargingPower,
}

/// Whether [`resolve_flag_move`] can resolve `move_id` — checked before any
/// state or RNG is touched, the same contract every other pipeline's
/// `ensure_resolvable` follows.
///
/// # Errors
///
/// - [`BattleError::UnknownMove`] if `move_id` is not in `dex`.
/// - [`BattleError::UnsupportedMoveEffect`] if its `EFFECT_*` is none of the
///   three.
pub fn ensure_resolvable(dex: &Dex, move_id: MoveId) -> Result<(), BattleError> {
    if is_flag_move_effect(dex.move_data(move_id)?.effect) {
        Ok(())
    } else {
        Err(BattleError::UnsupportedMoveEffect(move_id))
    }
}

/// Run `attacker`'s flag-only move.
///
/// Takes **no `rng`**: that is the module's headline claim, made
/// unforgeable by the signature rather than only by the docs — none of the
/// three scripts contains a `Random()`, on any path.
///
/// Returns what to print and (implicitly) which bit to set; the caller owns
/// `attacker`'s [`crate::volatile::Volatiles`] and applies the change, the
/// same split [`crate::stat_change`] uses for stat stages.
///
/// # Errors
///
/// Whatever [`ensure_resolvable`] reports.
pub fn resolve_flag_move(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
) -> Result<FlagMoveOutcome, BattleError> {
    let effect = dex.move_data(move_id)?.effect;
    if effect == EFFECT_SPLASH {
        Ok(FlagMoveOutcome::NothingHappened)
    } else if effect == EFFECT_FOCUS_ENERGY {
        // The script's own `jumpifstatus2 BS_ATTACKER, STATUS2_FOCUS_ENERGY`
        // (`:889`), one instruction ahead of `Cmd_setfocusenergy`.
        if attacker.volatiles().focus_energy {
            Ok(FlagMoveOutcome::Failed)
        } else {
            Ok(FlagMoveOutcome::GettingPumped)
        }
    } else if effect == EFFECT_CHARGE {
        Ok(FlagMoveOutcome::ChargingPower)
    } else {
        Err(BattleError::UnsupportedMoveEffect(move_id))
    }
}

#[cfg(test)]
#[path = "flag_move/tests.rs"]
mod tests;
