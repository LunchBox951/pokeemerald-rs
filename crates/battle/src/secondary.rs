//! Damaging moves with a secondary effect (S-6, issue #293): the
//! `setmoveeffect X; goto BattleScript_EffectHit` trampolines, where
//! `Cmd_seteffectwithchance`'s draw stops being discarded and starts
//! deciding something.
//!
//! Two so far — `EFFECT_POISON_HIT` (Poison Sting) and
//! `EFFECT_SPEED_DOWN_HIT` (Constrict). Each is two lines of script:
//!
//! `BattleScript_EffectPoisonHit` (`data/battle_scripts_1.s:319`-`:321`)
//! sets the poison move-effect byte and jumps into the plain hit script;
//! `BattleScript_EffectSpeedDownHit` (`:1048`-`:1050`) does the same with
//! the one-stage speed drop.
//!
//! So the damage half is [`crate::hit::resolve_hit`]'s, unchanged and
//! undamaged — same accuracy roll, same crit roll, same damage roll. What
//! changes is step 7.
//!
//! # The draw that stops being discarded
//!
//! `Cmd_seteffectwithchance`'s deciding condition
//! (`src/battle_script_commands.c:2908`-`:2939`) tests three things joined
//! by short-circuit ANDs, with the `Random() % 100` roll against
//! `percentChance` as the **first** operand (`:2923`) — so the draw is
//! spent before the effect byte or the type-immunity flag is consulted,
//! on every landed hit.
//!
//! For a plain `EFFECT_HIT` move the effect byte is `0`, so the draw
//! happens and the chain fails — [`crate::hit`]'s step 7, one wasted value.
//! For these two the `setmoveeffect` has written a real byte, so the roll
//! *lands* whenever it comes up under `percentChance` (`30` for Poison
//! Sting, `10` for Constrict) **and** the hit was not type-immune.
//!
//! **The draw count does not change**: 4 for a landed hit either way. What
//! changes is whether the value does anything, which is why this is not a
//! stream risk but is very much a behaviour one.
//!
//! # And `SetMoveEffect` adds nothing
//!
//! Neither secondary costs a further draw:
//!
//! - `MOVE_EFFECT_POISON` → `case STATUS1_POISON:`
//!   (`:2299`-`:2340`) — type and existing-status guards, then a flag write.
//!   No `Random()` ([`crate::status::can_inflict`]). Contrast
//!   `MOVE_EFFECT_SLEEP`/`_CONFUSION`, which *do* roll a duration.
//! - `MOVE_EFFECT_SPD_MINUS_1` → the stat-drop group at `:2665`-`:2685`,
//!   which calls `ChangeStatBuffs(SET_STAT_BUFF_VALUE(1) |
//!   STAT_BUFF_NEGATIVE, STAT_SPEED, 0, 0)` — the same drawless bookkeeping
//!   [`crate::stat_change`] already reproduces, and it fails silently
//!   against a target already at [`crate::StatStage::MIN`].
//!
//! # Shield Dust, and why it does not apply
//!
//! Shield Dust blocks secondary effects only for `MOVE_EFFECT_BYTE <= 9`
//! (`:2253`-`:2255`). `MOVE_EFFECT_POISON` is `2` and would be blocked;
//! `MOVE_EFFECT_SPD_MINUS_1` is `24` and would not. Neither matters here —
//! no abilities are modelled — but the asymmetry is recorded so a future
//! ability slice does not apply one rule to both.

use assets::{MoveEffect, MoveId};

use crate::stat_change::{ChangedStat, StatChangeDirection, StatChangeEffect};
use crate::status::Status1;

/// `EFFECT_POISON_HIT` (`include/constants/battle_move_effects.h:6`): Poison
/// Sting's effect id. (`40` is the *move* id, `MOVE_POISON_STING`.)
pub const EFFECT_POISON_HIT: MoveEffect = MoveEffect(2);

/// `EFFECT_SPEED_DOWN_HIT` (`:74`): Constrict's effect id.
pub const EFFECT_SPEED_DOWN_HIT: MoveEffect = MoveEffect(70);

/// The `setmoveeffect` byte one of these trampolines writes, as far as this
/// crate can apply it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecondaryEffect {
    /// `MOVE_EFFECT_POISON` (`2`): inflict [`Status1::Poisoned`], subject to
    /// [`crate::status::can_inflict`]'s guards.
    Poison,
    /// `MOVE_EFFECT_SPD_MINUS_1` (`24`): the target's Speed, one stage down.
    SpeedDown,
}

impl SecondaryEffect {
    /// The [`Status1`] this effect inflicts, or `None` for one that changes
    /// no non-volatile status.
    #[must_use]
    pub const fn status(self) -> Option<Status1> {
        match self {
            Self::Poison => Some(Status1::Poisoned),
            Self::SpeedDown => None,
        }
    }

    /// The stat change this effect applies, or `None` for one that changes
    /// no stage — expressed as the very same [`StatChangeEffect`]
    /// [`crate::stat_change`] uses, because upstream reaches the same
    /// `ChangeStatBuffs` with the same `SET_STAT_BUFF_VALUE(1) |
    /// STAT_BUFF_NEGATIVE` argument it does.
    #[must_use]
    pub const fn stat_change(self) -> Option<StatChangeEffect> {
        match self {
            Self::Poison => None,
            Self::SpeedDown => Some(StatChangeEffect {
                stat: ChangedStat::Speed,
                magnitude: 1,
                direction: StatChangeDirection::Lower,
            }),
        }
    }
}

/// Every `setmoveeffect`-then-`goto BattleScript_EffectHit` trampoline this
/// module models, with the script line each was read from.
pub const SECONDARY_EFFECTS: [(MoveEffect, SecondaryEffect); 2] = [
    (EFFECT_POISON_HIT, SecondaryEffect::Poison), // data/battle_scripts_1.s:319
    (EFFECT_SPEED_DOWN_HIT, SecondaryEffect::SpeedDown), // :1048
];

/// The [`SecondaryEffect`] `effect`'s trampoline writes, or `None` if it is
/// not one of them.
#[must_use]
pub fn secondary_for_effect(effect: MoveEffect) -> Option<SecondaryEffect> {
    SECONDARY_EFFECTS
        .iter()
        .find(|(id, _)| *id == effect)
        .map(|(_, secondary)| *secondary)
}

/// Whether `effect` is one of the modelled trampolines.
#[must_use]
pub fn is_secondary_effect(effect: MoveEffect) -> bool {
    secondary_for_effect(effect).is_some()
}

/// Whether the move's own damage pipeline can run it — the damage half is
/// [`crate::hit::resolve_hit`]'s, so a trampoline move must satisfy
/// everything an ordinary hit does *except* the allow-list, which by
/// definition excludes it.
///
/// # Errors
///
/// - [`crate::error::BattleError::UnknownMove`] if `move_id` is not in
///   `dex`.
/// - [`crate::error::BattleError::UnsupportedMoveEffect`] if its `EFFECT_*`
///   is not a modelled trampoline.
/// - [`crate::error::BattleError::UnsupportedMoveType`] for a `???`-typed
///   move.
pub fn ensure_resolvable(
    dex: &crate::dex::Dex,
    move_id: MoveId,
) -> Result<(), crate::error::BattleError> {
    let mv = dex.move_data(move_id)?;
    if !is_secondary_effect(mv.effect) {
        return Err(crate::error::BattleError::UnsupportedMoveEffect(move_id));
    }
    if mv.move_type.battle_type().is_none() {
        return Err(crate::error::BattleError::UnsupportedMoveType(move_id));
    }
    Ok(())
}

/// `Cmd_seteffectwithchance`'s leading operand
/// (`src/battle_script_commands.c:2923`): `Random() % 100 < percentChance`.
///
/// **Always draws exactly once** — that is the whole point of the operand
/// order, and it is the same single draw [`crate::hit::resolve_hit`] takes
/// and throws away for a plain move. `chance` is the move's
/// `gBattleMoves[].secondaryEffectChance`; Serene Grace's doubling is an
/// ability and is not modelled.
#[must_use]
pub fn secondary_chance_roll(chance: u8, rng: &mut impl crate::damage::BattleRng) -> bool {
    u32::from(rng.next_u16()) % 100 < u32::from(chance)
}

#[cfg(test)]
#[path = "secondary/tests.rs"]
mod tests;
