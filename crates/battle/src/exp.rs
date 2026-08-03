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

#[cfg(test)]
mod tests {
    use super::wild_faint_exp;

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
}
