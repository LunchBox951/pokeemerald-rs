//! Experience gain on faint (S-6): `Cmd_getexp`, reduced to the v1 case.
//!
//! `Cmd_getexp` (`pokeemerald/src/battle_script_commands.c:3255`) is a
//! multi-state battle-script command that redistributes experience across
//! every eligible party member (Exp Share holders included), applies a
//! traded-mon/Lucky Egg/trainer-battle `x1.5` bonus, and skips fainted or
//! not-yet-sent-in party members. Its core formula
//! (`calculatedExp = expYield * enemyLevel / 7`, `:3310`) collapses to a
//! single value for this slice's v1 shape — one player party member, a wild
//! (non-trainer) battle, no held items, mon not traded — since with exactly
//! one eligible, non-Exp-Share recipient the redistribution loop
//! (`SAFE_DIV(calculatedExp, viaSentIn=1)`, `:3311`) is a no-op and none of
//! the `x1.5` bonuses apply. Party-wide redistribution, Exp Share, Lucky
//! Egg, and the traded/trainer-battle bonus are deferred.
//!
//! One exclusion **is** modelled, at the turn-engine level rather than here:
//! a `MAX_LEVEL` recipient gains no experience and sees no "gained EXP"
//! message — `Cmd_getexp` case 2 zeroes the award and jumps past the string
//! (`:3351`-`:3356`) — so [`crate::battle::Battle`] never calls this
//! function (and emits no `ExpGained` event) for a level-100 player mon.

/// The experience a single player mon gains for fainting an opponent of
/// `enemy_base_exp` (`gSpeciesInfo[].expYield`,
/// [`assets::BaseStats::base_exp`]) at `enemy_level`, in this slice's v1
/// shape (see the module docs).
///
/// `calculatedExp = expYield * enemyLevel / 7`, floored to `1` if the
/// division truncates to `0` (`SAFE_DIV`'s `if (*exp == 0) *exp = 1`,
/// `battle_script_commands.c:3312`).
#[must_use]
pub fn wild_faint_exp(enemy_base_exp: u8, enemy_level: u8) -> u32 {
    let calculated = u32::from(enemy_base_exp) * u32::from(enemy_level) / 7;
    calculated.max(1)
}

/// The same award in a `BATTLE_TYPE_TRAINER` battle (S-6, issue #237): one
/// of the `x1.5` bonuses the module docs list above, and the only one this
/// port can reach.
///
/// `Cmd_getexp` case 2 applies the multipliers to the *already floored*
/// per-recipient award, in this order (`battle_script_commands.c:3374`-
/// `:3379`):
///
/// ```text
/// if (holdEffect == HOLD_EFFECT_LUCKY_EGG) gBattleMoveDamage = (gBattleMoveDamage * 150) / 100;
/// if (gBattleTypeFlags & BATTLE_TYPE_TRAINER) gBattleMoveDamage = (gBattleMoveDamage * 150) / 100;
/// ```
///
/// so the trainer bonus multiplies [`wild_faint_exp`]'s result rather than
/// the raw `expYield * level / 7` — an observable difference whenever the
/// `/ 7` truncates. The Lucky Egg factor ahead of it needs held items and is
/// out of scope; the traded-mon bonus at `:3381` needs an OT-name/id
/// comparison against the player's own save data, which this crate has no
/// access to ([`crate::pokemon::BattlePokemon::original_trainer_id`] exists;
/// the *player's* own id does not).
#[must_use]
pub fn trainer_faint_exp(enemy_base_exp: u8, enemy_level: u8) -> u32 {
    wild_faint_exp(enemy_base_exp, enemy_level) * 150 / 100
}

#[cfg(test)]
mod tests {
    use super::{trainer_faint_exp, wild_faint_exp};

    #[test]
    fn matches_hand_computed_upstream_formula() {
        // Bulbasaur base_exp 64, level 5: 64*5/7 = 45 (320/7 = 45.71...).
        assert_eq!(wild_faint_exp(64, 5), 45);
    }

    #[test]
    fn floors_a_truncated_zero_to_one() {
        // base_exp 1, level 1: 1*1/7 = 0, floored to 1.
        assert_eq!(wild_faint_exp(1, 1), 1);
    }

    #[test]
    fn scales_with_level_and_base_exp() {
        assert_eq!(wild_faint_exp(70, 10), 100); // 700/7 = 100 exactly
        assert_eq!(wild_faint_exp(140, 10), 200);
    }

    #[test]
    fn a_trainer_battle_pays_one_and_a_half_times_the_wild_award() {
        // Treecko base_exp 65 at level 5: 65*5/7 = 46 (325/7 = 46.43...),
        // then 46*150/100 = 69 -- the exact award for beating the Route 103
        // rival's level-5 starter.
        assert_eq!(wild_faint_exp(65, 5), 46);
        assert_eq!(trainer_faint_exp(65, 5), 69);
    }

    #[test]
    fn the_bonus_multiplies_the_already_floored_award_not_the_raw_quotient() {
        // base_exp 1, level 1: the wild award floors 0 to 1, and the trainer
        // bonus then scales *that* (1*150/100 = 1) -- not the raw quotient
        // (0*150/100 = 0, which would floor to 0 and lose the message).
        assert_eq!(wild_faint_exp(1, 1), 1);
        assert_eq!(trainer_faint_exp(1, 1), 1);
        // Scaling the raw quotient first would give `1 * 1 * 150 / 100 / 7`
        // == 0 and lose both the award and its message.
        assert_eq!(u32::from(1u8) * 150 / 100 / 7, 0);
    }
}
