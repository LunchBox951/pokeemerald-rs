//! Admission and resolution for fixed-damage moves.
//!
//! Dragon Rage and Sonic Boom deal literal amounts. Seismic Toss and Night
//! Shade use the attacker's level. Resolution consumes an accuracy draw and,
//! after any landed move, a trailing effect-chance draw. Type effectiveness
//! decides only immunity; other multipliers do not alter the damage, except
//! that a Wonder Guard holder blocks every result short of strictly super
//! effective (`battle_script_commands.c:1409-1418`), including an
//! independently immune matchup. Fixed damage never consumes critical-hit or
//! damage-variance draws.
//!
//! Upstream assigns the fixed amount after type calculation and then joins the
//! ordinary-hit tail (`data/battle_scripts_1.s:819-828`, `:1195-1204`,
//! `:1720-1729`). This ordering preserves immunity and the trailing draw while
//! discarding every nonzero type multiplier.

use assets::{MoveEffect, MoveId};

use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::hit::{
    accuracy_roll, classify_accuracy_failure, defender_is_immune, defender_wonder_guard_blocked,
    HitOutcome,
};
use crate::move_gate::ensure_resolvable_effect;
use crate::pokemon::BattlePokemon;
use crate::secondary::spend_effect_chance_draw;

/// Dragon Rage's move effect.
pub const EFFECT_DRAGON_RAGE: MoveEffect = MoveEffect(41);

/// Seismic Toss's and Night Shade's move effect.
pub const EFFECT_LEVEL_DAMAGE: MoveEffect = MoveEffect(87);

/// Sonic Boom's move effect.
pub const EFFECT_SONICBOOM: MoveEffect = MoveEffect(130);

const DRAGON_RAGE_DAMAGE: u32 = 40;
const SONIC_BOOM_DAMAGE: u32 = 20;
/// The source of a fixed-damage move's damage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FixedDamage {
    /// A literal damage amount.
    Literal(u32),
    /// The attacker's level.
    AttackerLevel,
}

impl FixedDamage {
    /// Returns the damage for `attacker_level`.
    #[must_use]
    pub const fn amount(self, attacker_level: u8) -> u32 {
        match self {
            Self::Literal(damage) => damage,
            Self::AttackerLevel => attacker_level as u32,
        }
    }
}

/// The supported effects and their fixed-damage sources, in effect order.
pub const FIXED_DAMAGE_EFFECTS: [(MoveEffect, FixedDamage); 3] = [
    (EFFECT_DRAGON_RAGE, FixedDamage::Literal(DRAGON_RAGE_DAMAGE)),
    (EFFECT_LEVEL_DAMAGE, FixedDamage::AttackerLevel),
    (EFFECT_SONICBOOM, FixedDamage::Literal(SONIC_BOOM_DAMAGE)),
];

/// Returns the damage source for `effect` when it is supported.
#[must_use]
pub fn fixed_damage_for_effect(effect: MoveEffect) -> Option<FixedDamage> {
    FIXED_DAMAGE_EFFECTS
        .iter()
        .find(|(supported_effect, _)| *supported_effect == effect)
        .map(|(_, damage_source)| *damage_source)
}

/// Returns whether `effect` uses a fixed-damage script.
#[must_use]
pub fn is_fixed_damage_effect(effect: MoveEffect) -> bool {
    fixed_damage_for_effect(effect).is_some()
}

/// Validates that `move_id` can use [`resolve_fixed_damage_move`].
///
/// Validation completes before any state or RNG is touched.
///
/// # Errors
///
/// Returns [`BattleError::UnknownMove`],
/// [`BattleError::UnsupportedMoveEffect`], or
/// [`BattleError::UnsupportedMoveType`] for the corresponding unsupported
/// move property.
pub fn ensure_resolvable(dex: &Dex, move_id: MoveId) -> Result<(), BattleError> {
    ensure_resolvable_effect(dex, move_id, is_fixed_damage_effect)
}

/// Resolves one fixed-damage move against `defender`.
///
/// A failed accuracy roll consumes one accuracy draw and reports
/// [`classify_accuracy_failure`]'s verdict. A landed move consumes the
/// accuracy and trailing effect-chance draws, including when the defender is
/// immune.
///
/// # Errors
///
/// Returns the errors from [`ensure_resolvable`], [`accuracy_roll`],
/// [`classify_accuracy_failure`], or [`spend_effect_chance_draw`]. Admission
/// completes before any draw.
pub fn resolve_fixed_damage_move(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
    rng: &mut impl BattleRng,
) -> Result<HitOutcome, BattleError> {
    ensure_resolvable(dex, move_id)?;

    let move_data = dex.move_data(move_id)?;
    let move_type = move_data
        .move_type
        .battle_type()
        .ok_or(BattleError::UnsupportedMoveType(move_id))?;
    let damage_source = fixed_damage_for_effect(move_data.effect)
        .ok_or(BattleError::UnsupportedMoveEffect(move_id))?;
    let damage = damage_source.amount(attacker.level());

    if !accuracy_roll(dex, move_id, attacker, defender, rng)? {
        return classify_accuracy_failure(dex, move_id, defender);
    }

    // Every [`FIXED_DAMAGE_EFFECTS`] move carries nonzero power (`power: 1` in
    // each case, pinned by `four_moves_map_to_the_three_supported_effects`), so
    // upstream's `gBattleMoves[gCurrentMove].power` gate on the Wonder Guard
    // branch (`battle_script_commands.c:1411`) is always satisfied here.
    let defender_is_immune = defender_is_immune(move_type, defender);
    let defender_wonder_guard_blocked = defender_wonder_guard_blocked(move_type, defender);
    let hit_had_effect = !defender_is_immune && !defender_wonder_guard_blocked;
    // No `FIXED_DAMAGE_EFFECTS` entry is `EFFECT_POISON_HIT`, so the poison
    // signal is always `false` here.
    spend_effect_chance_draw(dex, move_id, hit_had_effect, defender, rng)?;

    if defender_wonder_guard_blocked {
        // Wonder Guard's `B_MSG_AVOIDED_DMG` (index 3) outranks the ordinary
        // typing-immunity message (`Cmd_resultmessage`,
        // `battle_script_commands.c:2048-2059`, comparing against
        // `B_MSG_AVOIDED_ATK`, index 2), so it is reported even when the
        // move is also independently type-immune.
        Ok(HitOutcome::WonderGuardBlocked)
    } else if defender_is_immune {
        Ok(HitOutcome::NoEffect)
    } else {
        Ok(HitOutcome::Hit {
            damage,
            is_critical: false,
        })
    }
}

#[cfg(test)]
#[path = "fixed_damage/tests.rs"]
mod tests;
