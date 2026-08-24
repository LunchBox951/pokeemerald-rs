//! Draining moves (S-6, issue #321): `BattleScript_EffectAbsorb` —
//! `EFFECT_ABSORB`, carried by Absorb, Mega Drain and Giga Drain.
//!
//! ```text
//! BattleScript_EffectAbsorb::                      @ data/battle_scripts_1.s:322
//!     attackcanceler
//!     accuracycheck BattleScript_PrintMoveMissed, ACC_CURR_MOVE
//!     attackstring / ppreduce
//!     critcalc / damagecalc / typecalc / adjustnormaldamage
//!     ... animation, healthbarupdate BS_TARGET, datahpupdate BS_TARGET ...
//!     critmessage / resultmessage
//!     negativedamage                               @ :343  -- the drain
//!     jumpifability BS_TARGET, ABILITY_LIQUID_OOZE, BattleScript_AbsorbLiquidOoze
//!     setbyte cMULTISTRING_CHOOSER, B_MSG_ABSORB   @ :346
//! BattleScript_AbsorbLiquidOoze::                  @ :348
//!     manipulatedamage DMG_CHANGE_SIGN             @ :349
//!     setbyte cMULTISTRING_CHOOSER, B_MSG_ABSORB_OOZE
//! BattleScript_AbsorbUpdateHp::                    @ :351
//!     healthbarupdate BS_ATTACKER / datahpupdate BS_ATTACKER
//!     jumpifmovehadnoeffect BattleScript_AbsorbTryFainting
//!     printfromtable gAbsorbDrainStringIds         @ :355
//!     tryfaintmon BS_ATTACKER                      @ :358
//!     tryfaintmon BS_TARGET                        @ :359
//!     goto BattleScript_MoveEnd
//! ```
//!
//! # The draw that is *missing*
//!
//! Everything down to `adjustnormaldamage` is the plain hit script step for
//! step ([`crate::hit::damage_core`]) — but the script then ends via `goto
//! BattleScript_MoveEnd` (`:360`) rather than falling through
//! `BattleScript_HitFromAtkAnimation`, so it **never reaches
//! `seteffectwithchance`** (`:265`). A landed drain move therefore costs
//! **3** draws where an ordinary landed move costs 4:
//!
//! | outcome | draws | which |
//! |---|---|---|
//! | missed | **1** | accuracy |
//! | landed, or type-immune | **3** | accuracy, crit, damage roll |
//! | *ordinary* landed move, for contrast | 4 | …plus the discarded effect-chance roll |
//!
//! Either crit suppressor drops the landed row to **2**, exactly as it
//! drops every row in [`crate::hit`]'s table: the caller's `suppress_crit`
//! (`BATTLE_TYPE_FIRST_BATTLE`), or a defender carrying Battle Armor /
//! Shell Armor ([`crate::ability::suppresses_critical_hits`], issue #391),
//! which [`crate::hit::damage_core`] folds into the same short-circuit
//! ahead of the draw.
//!
//! Getting that count wrong desynchronises every later roll in the battle,
//! which is the whole reason this is its own module rather than a flag on
//! [`crate::hit::resolve_hit`].
//!
//! # The drain amount, and the `gHpDealt` contract
//!
//! `Cmd_negativedamage` (`src/battle_script_commands.c:6925`-`:6932`):
//!
//! ```text
//! gBattleMoveDamage = -(gHpDealt / 2);
//! if (gBattleMoveDamage == 0)
//!     gBattleMoveDamage = -1;
//! ```
//!
//! Two details a "half the damage" reimplementation gets wrong:
//!
//! 1. It halves **`gHpDealt`, not `gBattleMoveDamage`** — the HP the target
//!    *actually* lost, which the preceding `datahpupdate BS_TARGET` already
//!    clamped to its remaining HP (`:1928`-`:1932`: the `else` branch sets
//!    `gHpDealt = gBattleMons[].hp` and zeroes the HP). An overkill hit for
//!    999 on a target with 5 HP left drains **2**, not 499.
//! 2. The floor is applied *after* the halving, so a 1-damage hit still
//!    heals 1.
//!
//! This module deliberately cannot get (1) wrong by itself, because it
//! cannot see the target's HP: [`resolve_drain_move`] returns the damage
//! half only and [`drain_amount`] takes `gHpDealt` as its argument. The
//! wiring — that the caller feeds it the *clamped* figure and not the raw
//! formula output — is the part worth pinning, so it is pinned at turn level
//! rather than only here (`crates/battle/tests/turn_engine/pipelines.rs`).
//!
//! The heal itself is then clamped to the attacker's max HP by
//! `Cmd_datahpupdate`'s negative-damage branch (`:1896`-`:1900`), which
//! [`crate::pokemon::BattlePokemon::heal_hp`] applies.
//!
//! # Liquid Ooze
//!
//! `jumpifability BS_TARGET, ABILITY_LIQUID_OOZE` (`:345`) diverts to a
//! single `manipulatedamage DMG_CHANGE_SIGN` (`:349`,
//! `Cmd_manipulatedamage`'s `gBattleMoveDamage *= -1` at `:6744`-`:6746`)
//! and a different string index. The *magnitude* is untouched: the attacker
//! takes exactly what it would have healed, as damage
//! ([`DrainOutcome::amount`]), and the message becomes "it sucked up the
//! liquid ooze!" instead of "energy was drained!"
//! (`gAbsorbDrainStringIds[B_MSG_ABSORB_OOZE]`, `src/battle_message.c:1123`).
//! It draws nothing — see [`crate::ability::inverts_drain`].
//!
//! Two orderings the script fixes and this module preserves:
//!
//! - the sign flip happens **after** `negativedamage`, so the `-1` floor is
//!   applied to the heal and *then* inverted: a 1-damage Absorb into a
//!   Liquid Ooze target costs the attacker 1 HP, it does not round to 0;
//! - `tryfaintmon BS_ATTACKER` comes **before** `tryfaintmon BS_TARGET`
//!   (`:358`-`:359`), so an attacker the ooze killed faints first even when
//!   its own hit was lethal. The caller owns both faints; this module
//!   reports which way the HP moved and leaves the order to it.
//!
//! # Not modelled
//!
//! `orword gHitMarker, HITMARKER_IGNORE_SUBSTITUTE` (`:344`) matters only to
//! a Substitute, which this crate does not represent (see
//! [`crate::stat_change`]'s module docs for that boundary). The
//! `jumpifmovehadnoeffect` at `:353` suppresses the drain *string* on a
//! type-immune hit; since `Cmd_datahpupdate` also gates all HP movement on
//! the same flag (`:1862`), a no-effect drain moves no HP and prints
//! nothing, which is [`DrainOutcome`]'s absence rather than a variant.

use assets::species::AbilityId;
use assets::{MoveEffect, MoveId};

use crate::ability::inverts_drain;
use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::hit::{accuracy_roll, damage_core, HitOutcome};
use crate::pokemon::BattlePokemon;

/// `EFFECT_ABSORB` (`include/constants/battle_move_effects.h:7`): the drain
/// effect id. (`71` is the *move* id `MOVE_ABSORB`; Mega Drain is `72` and
/// Giga Drain `202`, and all three carry effect `3` — three easily-confused
/// numbers.)
pub const EFFECT_ABSORB: MoveEffect = MoveEffect(3);

/// Whether `effect` runs `BattleScript_EffectAbsorb`.
#[must_use]
pub fn is_drain_effect(effect: MoveEffect) -> bool {
    effect == EFFECT_ABSORB
}

/// Which way `Cmd_negativedamage` plus the Liquid Ooze branch moved the
/// **attacker's** HP, and by how much.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DrainOutcome {
    /// The magnitude, from [`drain_amount`]. Never `0`: a landed drain
    /// always moves at least 1 HP, because upstream's floor fires after the
    /// halving.
    pub amount: u32,
    /// The target's ability inverted the drain
    /// ([`crate::ability::inverts_drain`]): `amount` is **damage to the
    /// attacker** rather than a heal, and the message is
    /// `B_MSG_ABSORB_OOZE`.
    pub inverted: bool,
}

/// `Cmd_negativedamage` (`src/battle_script_commands.c:6927`-`:6930`):
/// `-(gHpDealt / 2)`, floored to `-1` when that truncates to zero.
///
/// `hp_dealt` is `gHpDealt` — the HP the target really lost after
/// `Cmd_datahpupdate`'s clamp — **not** the formula's raw output. Returns a
/// positive magnitude; `0` in, `0` out (a move that dealt nothing moves no
/// HP, because the script's `jumpifmovehadnoeffect` skips the whole tail).
#[must_use]
pub const fn drain_amount(hp_dealt: u32) -> u32 {
    if hp_dealt == 0 {
        return 0;
    }
    let half = hp_dealt / 2;
    if half == 0 {
        1
    } else {
        half
    }
}

/// The whole drain tail, from `negativedamage` (`:343`) through the Liquid
/// Ooze branch (`:345`-`:350`): how far and which way the attacker's HP
/// moves for a hit that really took `hp_dealt` HP off a target with
/// `target_ability`.
///
/// Returns `None` for `hp_dealt == 0` — the type-immune path, where
/// `Cmd_datahpupdate` moves no HP and `jumpifmovehadnoeffect` skips the
/// string. Draws nothing: neither the arithmetic nor the ability branch
/// touches `Random()`.
#[must_use]
pub fn resolve_drain(hp_dealt: u32, target_ability: AbilityId) -> Option<DrainOutcome> {
    let amount = drain_amount(hp_dealt);
    if amount == 0 {
        return None;
    }
    Some(DrainOutcome {
        amount,
        inverted: inverts_drain(target_ability),
    })
}

/// Whether [`resolve_drain_move`] can resolve `move_id` — checked *before*
/// any state or RNG is touched, the contract every pipeline's
/// `ensure_resolvable` in this crate follows.
///
/// # Errors
///
/// - [`BattleError::UnknownMove`] if `move_id` is not in `dex`.
/// - [`BattleError::UnsupportedMoveEffect`] if its `EFFECT_*` is not
///   [`EFFECT_ABSORB`].
/// - [`BattleError::UnsupportedMoveType`] for a `???`-typed move, which
///   `Cmd_typecalc` could not classify.
pub fn ensure_resolvable(dex: &Dex, move_id: MoveId) -> Result<(), BattleError> {
    let mv = dex.move_data(move_id)?;
    if !is_drain_effect(mv.effect) {
        return Err(BattleError::UnsupportedMoveEffect(move_id));
    }
    if mv.move_type.battle_type().is_none() {
        return Err(BattleError::UnsupportedMoveType(move_id));
    }
    Ok(())
}

/// Resolve `attacker`'s drain move against `defender`, returning the
/// **damage half only**.
///
/// Draws **1** on a miss and **3** otherwise (module docs) — deliberately
/// one fewer than [`crate::hit::resolve_hit`], because
/// `BattleScript_EffectAbsorb` has no `seteffectwithchance` step.
///
/// The heal is *not* computed here: it depends on `gHpDealt`, the HP the
/// target actually lost, which only the caller knows once it has clamped
/// this damage against the target's remaining HP. Feed that value to
/// [`resolve_drain`].
///
/// `suppress_crit` has the same meaning as in [`crate::hit::damage_core`].
///
/// # Errors
///
/// Whatever [`ensure_resolvable`] reports; nothing is drawn before that
/// check.
pub fn resolve_drain_move(
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
    // Identical to the plain script down to `adjustnormaldamage`, and then
    // it stops -- no `seteffectwithchance` draw (module docs).
    damage_core(dex, move_id, attacker, defender, suppress_crit, rng)
}

#[cfg(test)]
#[path = "drain/tests.rs"]
mod tests;
