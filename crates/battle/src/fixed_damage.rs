//! Admission and resolution for fixed-damage moves.
//!
//! Dragon Rage and Sonic Boom deal literal amounts. Seismic Toss and Night
//! Shade use the attacker's level. Psywave scales the attacker's level by a
//! rejection-sampled random factor (`Cmd_psywavedamageeffect`,
//! `battle_script_commands.c:7932-7941`). Resolution consumes an accuracy
//! draw, then Psywave's sample draws, and, after any landed move, a trailing
//! effect-chance draw. Type effectiveness
//! decides only immunity; other multipliers do not alter the damage, except
//! that a Wonder Guard holder blocks every result short of strictly super
//! effective (`battle_script_commands.c:1409-1418`), including an
//! independently immune matchup. Fixed damage never consumes critical-hit or
//! damage-variance draws.
//!
//! Upstream assigns the fixed amount after type calculation and then joins the
//! ordinary-hit tail (`data/battle_scripts_1.s:819-828`, `:1195-1204`,
//! `:1720-1729`). This ordering preserves immunity and the trailing draw while
//! discarding every nonzero type multiplier. Psywave's script runs its damage
//! command at that same point (`data/battle_scripts_1.s:1206-1215`), so its
//! sample draws follow the accuracy draw and precede the trailing draw even
//! when the target turns out to be immune.

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
pub const EFFECT_DRAGON_RAGE: MoveEffect = MoveEffect::DRAGON_RAGE;

/// Seismic Toss's and Night Shade's move effect.
pub const EFFECT_LEVEL_DAMAGE: MoveEffect = MoveEffect::LEVEL_DAMAGE;

/// Sonic Boom's move effect.
pub const EFFECT_SONICBOOM: MoveEffect = MoveEffect::SONICBOOM;

/// Psywave's move effect.
pub const EFFECT_PSYWAVE: MoveEffect = MoveEffect::PSYWAVE;

const DRAGON_RAGE_DAMAGE: u32 = 40;
const SONIC_BOOM_DAMAGE: u32 = 20;
/// Psywave samples `Random() % 16` and redraws any value above this.
const PSYWAVE_MAX_SAMPLE: u16 = 10;
const PSYWAVE_SAMPLE_MODULUS: u16 = 16;
const PSYWAVE_SAMPLE_STEP: u32 = 10;
const PSYWAVE_BASE_PERCENT: u32 = 50;
const PSYWAVE_PERCENT_DIVISOR: u32 = 100;

/// The source of a fixed-damage move's damage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FixedDamage {
    /// A literal damage amount.
    Literal(u32),
    /// The attacker's level.
    AttackerLevel,
    /// The attacker's level scaled by 50% to 150% in steps of 10%, from a
    /// sample redrawn until it falls in `0..=10`.
    LevelScaledRandom,
}

impl FixedDamage {
    /// Returns the damage for `attacker_level`.
    ///
    /// Only [`Self::LevelScaledRandom`] draws: it draws `rng` once per
    /// sample, including each rejected one, and floors the scaled level, so
    /// a level-1 attacker's lowest samples deal zero.
    pub fn amount(self, attacker_level: u8, rng: &mut impl BattleRng) -> u32 {
        match self {
            Self::Literal(damage) => damage,
            Self::AttackerLevel => u32::from(attacker_level),
            Self::LevelScaledRandom => {
                let sample = loop {
                    let sample = rng.next_u16() % PSYWAVE_SAMPLE_MODULUS;
                    if sample <= PSYWAVE_MAX_SAMPLE {
                        break sample;
                    }
                };
                u32::from(attacker_level)
                    * (u32::from(sample) * PSYWAVE_SAMPLE_STEP + PSYWAVE_BASE_PERCENT)
                    / PSYWAVE_PERCENT_DIVISOR
            }
        }
    }
}

/// The supported effects and their fixed-damage sources, in effect order.
pub const FIXED_DAMAGE_EFFECTS: [(MoveEffect, FixedDamage); 4] = [
    (EFFECT_DRAGON_RAGE, FixedDamage::Literal(DRAGON_RAGE_DAMAGE)),
    (EFFECT_LEVEL_DAMAGE, FixedDamage::AttackerLevel),
    (EFFECT_PSYWAVE, FixedDamage::LevelScaledRandom),
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
/// immune. Psywave adds its sample draws between the two, rejected samples
/// included, also when the defender is immune.
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

    if !accuracy_roll(dex, move_id, attacker, defender, rng)? {
        return classify_accuracy_failure(dex, move_id, defender);
    }

    let damage = damage_source.amount(attacker.level(), rng);

    // Every [`FIXED_DAMAGE_EFFECTS`] move carries nonzero power (`power: 1` in
    // each case, pinned by `five_moves_map_to_the_four_supported_effects`), so
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
