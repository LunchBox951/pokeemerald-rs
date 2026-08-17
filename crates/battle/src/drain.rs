//! Draining moves (S-6, issue #293): `BattleScript_EffectAbsorb` —
//! `EFFECT_ABSORB`, carried by Absorb, Mega Drain and Giga Drain.
//!
//! `BattleScript_EffectAbsorb` (`data/battle_scripts_1.s:323`-`:360`)
//! opens exactly like the plain hit script — attack canceler, accuracy
//! check (a miss prints the ordinary missed string and ends the move),
//! attack string and PP deduction, then the crit/damage/type/damage-roll
//! chain — and applies the hit to the target. Only then does it diverge:
//! the drain step (`negativedamage`, `:343`) converts the HP dealt into a
//! negative "damage" aimed back at the attacker, a Liquid Ooze ability
//! check can flip that heal into harm, the attacker's own HP update
//! applies it, both sides get a faint check (attacker first), and the
//! script jumps straight to the move end.
//!
//! # The one draw that is *missing*
//!
//! Everything down to `adjustnormaldamage` is the plain hit script step for
//! step ([`crate::hit::damage_core`]) — but the script then ends via
//! `goto BattleScript_MoveEnd` (`:360`) rather than falling through
//! `BattleScript_HitFromAtkAnimation`, so it **never reaches
//! `seteffectwithchance`** (`:265`). A landed drain move therefore costs
//! **3** draws where an ordinary landed move costs 4:
//!
//! | move | draws | which |
//! |---|---|---|
//! | drain, missed | **1** | accuracy |
//! | drain, landed (or type-immune) | **3** | accuracy, crit, damage roll |
//! | *ordinary* landed move, for contrast | 4 | …plus the discarded effect-chance roll |
//!
//! Getting that wrong desynchronises every later roll in the battle, which
//! is the whole reason this is its own module rather than a flag on
//! [`crate::hit::resolve_hit`].
//!
//! # The drain amount
//!
//! `Cmd_negativedamage` (`src/battle_script_commands.c:6925`-`:6932`)
//! stores half the HP dealt, negated, as the new damage word, then floors
//! a zero result to a 1-HP heal. Two details a "half the damage"
//! reimplementation gets wrong:
//!
//! 1. It halves **`gHpDealt`, not `gBattleMoveDamage`** — the HP the target
//!    *actually* lost, which `Cmd_datahpupdate` already clamped to its
//!    remaining HP (`:1920`-`:1929`). An overkill hit for 999 on a target
//!    with 5 HP left drains `2`, not `499`.
//! 2. The floor is applied *after* the halving, so a 1-damage hit still
//!    heals `1`.
//!
//! Healing is then clamped to the attacker's max HP by `Cmd_datahpupdate`
//! (`:1897`-`:1901`), which [`resolve_drain_move`]'s caller applies.
//!
//! # Not modelled
//!
//! **Liquid Ooze** (`:345`, `manipulatedamage DMG_CHANGE_SIGN` at `:349`)
//! flips the sign so the attacker takes the drain as damage instead. It is
//! an ability, and this crate models no abilities at all, so the branch is
//! unreachable rather than wrong — it draws nothing either way. The two
//! `tryfaintmon` calls at `:358`-`:359` matter only for that branch and for
//! a target the drain killed, both of which the caller already handles.

use assets::{MoveEffect, MoveId};

use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::hit::{accuracy_roll, damage_core, HitOutcome};
use crate::pokemon::BattlePokemon;

/// `EFFECT_ABSORB` (`include/constants/battle_move_effects.h:7`): the drain
/// effect id. (`71` is the *move* id `MOVE_ABSORB`; Mega Drain is `72` and
/// Giga Drain `202`, and all three carry effect `3`.)
pub const EFFECT_ABSORB: MoveEffect = MoveEffect(3);

/// Whether `effect` runs `BattleScript_EffectAbsorb`.
#[must_use]
pub fn is_drain_effect(effect: MoveEffect) -> bool {
    effect == EFFECT_ABSORB
}

/// `Cmd_negativedamage` (`src/battle_script_commands.c:6927`-`:6930`):
/// `-(gHpDealt / 2)`, floored to `-1` when that truncates to zero.
///
/// `hp_dealt` is `gHpDealt` — the HP the target really lost after
/// `Cmd_datahpupdate`'s clamp — **not** the formula's raw output. Returns a
/// positive heal amount, and the floor is **unconditional**: upstream tests
/// the quotient, not the input, so even `gHpDealt == 0` becomes a 1-HP heal
/// (issue #293 review, round 5). That input is reachable — a queued Absorb
/// against a target that already fainted this turn deals 0 — and the user
/// really does drink 1 HP from the corpse, clamped as ever by
/// [`crate::pokemon::BattlePokemon::restore_hp`].
#[must_use]
pub const fn drain_amount(hp_dealt: u32) -> u32 {
    let half = hp_dealt / 2;
    if half == 0 {
        1
    } else {
        half
    }
}

/// Whether [`resolve_drain_move`] can resolve `move_id`.
///
/// # Errors
///
/// - [`BattleError::UnknownMove`] if `move_id` is not in `dex`.
/// - [`BattleError::UnsupportedMoveEffect`] if its `EFFECT_*` is not
///   [`EFFECT_ABSORB`].
pub fn ensure_resolvable(dex: &Dex, move_id: MoveId) -> Result<(), BattleError> {
    if is_drain_effect(dex.move_data(move_id)?.effect) {
        Ok(())
    } else {
        Err(BattleError::UnsupportedMoveEffect(move_id))
    }
}

/// Resolve `attacker`'s drain move against `defender`, returning the damage
/// half only.
///
/// Draws **1** on a miss and **3** otherwise (module docs) — deliberately
/// one fewer than [`crate::hit::resolve_hit`], because
/// `BattleScript_EffectAbsorb` has no `seteffectwithchance` step.
///
/// The heal is *not* computed here: it depends on `gHpDealt`, the HP the
/// target actually lost, which only the caller knows once it has clamped the
/// damage against the target's remaining HP. Feed that value to
/// [`drain_amount`] — `Battle::execute_drain_move` does.
///
/// `suppress_crit` has the same meaning as in
/// [`crate::hit::resolve_hit`]: pass `true` for a
/// `BATTLE_TYPE_FIRST_BATTLE`-shaped battle, which skips the crit roll and
/// its draw, taking a landed drain move down to 2.
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
