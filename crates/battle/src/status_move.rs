//! Self-targeting flag moves (S-6, issue #293): the four one-off battle
//! scripts whose whole effect is setting a [`crate::volatile::Volatiles`] bit
//! — Splash, Focus Energy, Charge, and Defense Curl.
//!
//! Each has its own script rather than sharing a tail the way
//! [`crate::stat_change`]'s family does, so each is reproduced separately
//! below. What they have in common is the reason they live together: all four
//! are `MOVE_TARGET_USER`, none has an `accuracycheck`, and **none draws a
//! single `Random()`** — so a caller scripting an RNG sequence budgets
//! exactly zero draws for any of them, on every path, including the failure
//! paths.
//!
//! # `BattleScript_EffectSplash` (`data/battle_scripts_1.s:1172`-`:1181`)
//!
//! ```text
//! attackcanceler / attackstring / ppreduce
//! attackanimation / waitanimation
//! incrementgamestat GAME_STAT_USED_SPLASH
//! printstring STRINGID_BUTNOTHINGHAPPENED
//! waitmessage / goto BattleScript_MoveEnd
//! ```
//!
//! There is no `accuracycheck`, no `damagecalc`, no `seteffectwithchance`,
//! and no state change of any kind: Splash spends a PP and prints "But
//! nothing happened!". `GAME_STAT_USED_SPLASH` is a save-block counter this
//! crate has no home for and is the one thing here left unmodelled.
//!
//! # `BattleScript_EffectFocusEnergy` (`:885`-`:895`)
//!
//! ```text
//! attackcanceler / attackstring / ppreduce
//! jumpifstatus2 BS_ATTACKER, STATUS2_FOCUS_ENERGY, BattleScript_ButItFailed
//! setfocusenergy
//! ...
//! printfromtable gFocusEnergyUsedStringIds
//! ```
//!
//! The already-pumped check is the **script's** `jumpifstatus2` at `:889`,
//! not `Cmd_setfocusenergy`'s own `else` branch
//! (`src/battle_script_commands.c:7751`-`:7755`) — that branch is
//! unreachable from this script, so [`resolve_status_move`] reproduces the
//! script's test and [`crate::volatile::Volatiles::set_focus_energy`] stays
//! unconditional.
//!
//! # `BattleScript_EffectCharge` (`:2297`-`:2306`)
//!
//! ```text
//! attackcanceler / attackstring / ppreduce
//! setcharge
//! attackanimation / waitanimation
//! printstring STRINGID_PKMNCHARGINGPOWER
//! ```
//!
//! **Gen 3's Charge does not raise Sp. Defense.** The script has no
//! `setstatchanger`/`statbuffchange` pair at all (compare Defense Curl's
//! `:2019`-`:2020`); the Sp. Def raise is a Gen-4 addition. It also cannot
//! fail and has no accuracy check, despite carrying `accuracy = 100` in
//! `gBattleMoves` — an inert byte, exactly like Tail Glow's
//! ([`crate::stat_change`]'s tests pin the same shape).
//!
//! # `BattleScript_EffectDefenseCurl` (`:2014`-`:2025`)
//!
//! ```text
//! attackcanceler / attackstring / ppreduce
//! setdefensecurlbit
//! setstatchanger STAT_DEF, 1, FALSE
//! statbuffchange MOVE_EFFECT_AFFECTS_USER | STAT_CHANGE_ALLOW_PTR, ...
//! jumpifbyte CMP_EQUAL, cMULTISTRING_CHOOSER, B_MSG_STAT_WONT_INCREASE, ...
//! ```
//!
//! The ordering is load-bearing and reproduced: `setdefensecurlbit` runs
//! **first**, so a user already at `+6` Defense still gets the Rollout flag
//! while its stat raise reports "won't go any higher!". The stat half is the
//! same `statbuffchange MOVE_EFFECT_AFFECTS_USER` the
//! [`crate::stat_change`] raising tail uses, which is why this module hands
//! back a [`crate::stat_change::StatChangeEffect`] rather than re-deriving
//! the clamp.
//!
//! # What is not modelled
//!
//! `attackcanceler`'s status rolls belong to every move alike and live in
//! [`crate::battle::Battle::act`]. `Cmd_setcharge`'s
//! `chargeTimerStartValue` (read only by the battle-recording code) and
//! Splash's `GAME_STAT_USED_SPLASH` are noted above. Snatch — all four moves
//! are `FLAG_SNATCH_AFFECTED` except Splash, whose `flags` are `0` — is not
//! modelled anywhere in this crate.

use assets::{MoveEffect, MoveId};

use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::BattlePokemon;
use crate::stat_change::{stage_of, ChangedStat, StatChangeDirection, StatChangeEffect};
use crate::stat_stage::StatStage;

/// `EFFECT_FOCUS_ENERGY` (`include/constants/battle_move_effects.h:51`):
/// Focus Energy's effect id.
pub const EFFECT_FOCUS_ENERGY: MoveEffect = MoveEffect(47);

/// `EFFECT_SPLASH` (`:89`): Splash's effect id.
pub const EFFECT_SPLASH: MoveEffect = MoveEffect(85);

/// `EFFECT_DEFENSE_CURL` (`:160`): Defense Curl's effect id.
pub const EFFECT_DEFENSE_CURL: MoveEffect = MoveEffect(156);

/// `EFFECT_CHARGE` (`:178`): Charge's effect id. (`268` is the *move* id,
/// `MOVE_CHARGE`; the two are easy to confuse and upstream reuses neither.)
pub const EFFECT_CHARGE: MoveEffect = MoveEffect(174);

/// The `setstatchanger STAT_DEF, 1, FALSE` Defense Curl's script issues
/// (`data/battle_scripts_1.s:2019`) — a `+1` raise on the **user**, i.e.
/// exactly the shape [`crate::stat_change`]'s raising tail applies, which is
/// why [`StatusMoveOutcome::DefenseCurl`] can hand it back verbatim.
pub const DEFENSE_CURL_STAT_CHANGE: StatChangeEffect = StatChangeEffect {
    stat: ChangedStat::Defense,
    magnitude: 1,
    direction: StatChangeDirection::Raise,
};

/// The four effect ids this module executes, in `battle_move_effects.h` id
/// order.
const STATUS_MOVE_EFFECTS: [MoveEffect; 4] = [
    EFFECT_FOCUS_ENERGY,
    EFFECT_SPLASH,
    EFFECT_DEFENSE_CURL,
    EFFECT_CHARGE,
];

/// Whether `effect`'s battle script is one of the four this module
/// reproduces.
#[must_use]
pub fn is_status_move_effect(effect: MoveEffect) -> bool {
    STATUS_MOVE_EFFECTS.contains(&effect)
}

/// What one of the four scripts did — one variant per distinct upstream
/// message, so a presentation layer never has to re-derive which string to
/// print.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StatusMoveOutcome {
    /// Splash: `STRINGID_BUTNOTHINGHAPPENED`, "But nothing happened!". No
    /// state changes at all.
    NothingHappened,
    /// Focus Energy while `STATUS2_FOCUS_ENERGY` is already set:
    /// `BattleScript_ButItFailed` (`data/battle_scripts_1.s:889`), and no
    /// state changes.
    Failed,
    /// Focus Energy succeeding: set `STATUS2_FOCUS_ENERGY`,
    /// `STRINGID_PKMNGETTINGPUMPED`.
    FocusEnergy,
    /// Charge: set `STATUS3_CHARGED_UP` and restart the two-turn timer,
    /// `STRINGID_PKMNCHARGINGPOWER`. Never fails.
    Charge,
    /// Defense Curl: set `STATUS2_DEFENSE_CURL` **and** raise the user's
    /// Defense by one stage.
    DefenseCurl {
        /// The user's Defense stage after the raise — unchanged from before
        /// it when `capped` is `true`.
        new_stage: StatStage,
        /// Whether the user was already at [`StatStage::MAX`], so the raise
        /// reported "won't go any higher!". The `STATUS2_DEFENSE_CURL` bit
        /// is set either way (module docs).
        capped: bool,
    },
}

/// Whether [`resolve_status_move`] can resolve `move_id` at all — checked
/// *before* any state or RNG is touched, the same contract
/// [`crate::hit::ensure_resolvable`] and
/// [`crate::stat_change::ensure_resolvable`] follow.
///
/// # Errors
///
/// - [`BattleError::UnknownMove`] if `move_id` is not in `dex`.
/// - [`BattleError::UnsupportedMoveEffect`] if its `EFFECT_*` is not one of
///   [`is_status_move_effect`]'s four.
pub fn ensure_resolvable(dex: &Dex, move_id: MoveId) -> Result<(), BattleError> {
    let mv = dex.move_data(move_id)?;
    if is_status_move_effect(mv.effect) {
        Ok(())
    } else {
        Err(BattleError::UnsupportedMoveEffect(move_id))
    }
}

/// Resolve `user`'s use of `move_id`, one of the four self-targeting flag
/// moves.
///
/// Takes no target and no `rng`: all four are `MOVE_TARGET_USER` and **none
/// of the four scripts contains a single `Random()`** (module docs), so the
/// absence of an `rng` parameter is the type system carrying that fact
/// rather than a comment claiming it.
///
/// Like [`crate::stat_change::resolve_stat_change_move`], this decides but
/// does not mutate: the caller owns the battler
/// ([`crate::battle::Battle::execute_status_move`]).
///
/// # Errors
///
/// Whatever [`ensure_resolvable`] reports for `move_id`.
pub fn resolve_status_move(
    dex: &Dex,
    move_id: MoveId,
    user: &BattlePokemon,
) -> Result<StatusMoveOutcome, BattleError> {
    ensure_resolvable(dex, move_id)?;
    let effect = dex.move_data(move_id)?.effect;

    if effect == EFFECT_SPLASH {
        return Ok(StatusMoveOutcome::NothingHappened);
    }
    if effect == EFFECT_FOCUS_ENERGY {
        // The script's own `jumpifstatus2 BS_ATTACKER, STATUS2_FOCUS_ENERGY,
        // BattleScript_ButItFailed` (`:889`), not the command's dead `else`.
        return Ok(if user.volatiles().focus_energy {
            StatusMoveOutcome::Failed
        } else {
            StatusMoveOutcome::FocusEnergy
        });
    }
    if effect == EFFECT_CHARGE {
        // `BattleScript_EffectCharge` has no failure branch at all: using
        // Charge while already charged simply restarts the timer.
        return Ok(StatusMoveOutcome::Charge);
    }
    if effect == EFFECT_DEFENSE_CURL {
        let current = stage_of(user, DEFENSE_CURL_STAT_CHANGE.stat);
        let capped = current == DEFENSE_CURL_STAT_CHANGE.cap();
        return Ok(StatusMoveOutcome::DefenseCurl {
            new_stage: if capped {
                current
            } else {
                current.saturating_add(DEFENSE_CURL_STAT_CHANGE.delta())
            },
            capped,
        });
    }
    // `ensure_resolvable` already narrowed `effect` to the four above; this
    // is re-derived through the fallible path rather than an `unreachable!`,
    // the same posture `stat_change` takes.
    Err(BattleError::UnsupportedMoveEffect(move_id))
}

#[cfg(test)]
#[path = "status_move/tests.rs"]
mod tests;
