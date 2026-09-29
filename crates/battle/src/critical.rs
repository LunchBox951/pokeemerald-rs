//! Critical-hit stage calculation, RNG rolls, and stat-stage overrides.
//!
//! Callers perform suppression checks before [`crit_roll`]. Upstream places
//! the random draw last in a short-circuiting condition, so a suppressed
//! critical hit consumes no RNG value
//! (`pokeemerald/src/battle_script_commands.c:1279`).

use assets::MoveEffect;

use crate::damage::BattleRng;
use crate::stat_stage::StatStage;

const EFFECT_HIGH_CRITICAL: MoveEffect = MoveEffect(43);
const EFFECT_SKY_ATTACK: MoveEffect = MoveEffect(75);
const EFFECT_BLAZE_KICK: MoveEffect = MoveEffect(200);
const EFFECT_POISON_TAIL: MoveEffect = MoveEffect(209);

const FOCUS_ENERGY_STAGE_BONUS: u8 = 2;
const HIGH_CRIT_EFFECT_STAGE_BONUS: u8 = 1;
const MAX_CRIT_STAGE: u8 = 4;

const CRIT_ROLL_DIVISORS_BY_STAGE: [u16; 5] = [16, 8, 4, 3, 2];

const HIGH_CRIT_EFFECTS: [MoveEffect; 4] = [
    EFFECT_HIGH_CRITICAL,
    EFFECT_SKY_ATTACK,
    EFFECT_BLAZE_KICK,
    EFFECT_POISON_TAIL,
];

/// Returns the critical-hit stage contributed by a move effect.
#[must_use]
pub fn crit_stage_for_effect(move_effect: MoveEffect) -> u8 {
    if HIGH_CRIT_EFFECTS.contains(&move_effect) {
        HIGH_CRIT_EFFECT_STAGE_BONUS
    } else {
        0
    }
}

/// Combines the modeled move-effect and Focus Energy stage bonuses.
///
/// Held-item bonuses are not inputs because held-item effects are not yet
/// modeled by this crate.
#[must_use]
pub fn crit_stage(move_effect: MoveEffect, focus_energy: bool) -> u8 {
    let focus_energy_bonus = if focus_energy {
        FOCUS_ENERGY_STAGE_BONUS
    } else {
        0
    };
    focus_energy_bonus + crit_stage_for_effect(move_effect)
}

/// Rolls for a critical hit, clamping `stage` to the highest modeled stage.
///
/// This always consumes one RNG value. Callers must resolve conditions that
/// suppress the roll before calling this function.
#[must_use]
pub fn crit_roll(stage: u8, rng: &mut impl BattleRng) -> bool {
    let clamped_stage = stage.min(MAX_CRIT_STAGE);
    let roll_divisor = u32::from(CRIT_ROLL_DIVISORS_BY_STAGE[usize::from(clamped_stage)]);
    let random_value = u32::from(rng.next_u16());
    random_value % roll_divisor == 0
}

/// Replaces a defender-favoring stage with neutral on a critical hit.
#[must_use]
pub fn crit_adjusted_stage(
    stage: StatStage,
    is_critical: bool,
    stage_favors_defender: impl Fn(StatStage) -> bool,
) -> StatStage {
    if is_critical && stage_favors_defender(stage) {
        StatStage::NEUTRAL
    } else {
        stage
    }
}

fn attack_stage_favors_defender(stage: StatStage) -> bool {
    stage.offset() < 0
}

fn defense_stage_favors_defender(stage: StatStage) -> bool {
    stage.offset() > 0
}

/// Applies critical-hit stage rules to an attack and defense stage pair.
#[must_use]
pub fn crit_adjusted_stages(
    attack_stage: StatStage,
    defense_stage: StatStage,
    is_critical: bool,
) -> (StatStage, StatStage) {
    let effective_attack_stage =
        crit_adjusted_stage(attack_stage, is_critical, attack_stage_favors_defender);
    let effective_defense_stage =
        crit_adjusted_stage(defense_stage, is_critical, defense_stage_favors_defender);
    (effective_attack_stage, effective_defense_stage)
}

#[cfg(test)]
mod tests {
    use super::{
        crit_adjusted_stages, crit_roll, crit_stage_for_effect, CRIT_ROLL_DIVISORS_BY_STAGE,
        HIGH_CRIT_EFFECTS, MAX_CRIT_STAGE,
    };
    use crate::damage::BattleRng;
    use crate::stat_stage::StatStage;
    use assets::MoveEffect;

    const ORDINARY_HIT_EFFECT: MoveEffect = MoveEffect(0);
    const STAGE_ABOVE_MAXIMUM: u8 = 99;

    struct FixedRng(u16);
    impl BattleRng for FixedRng {
        fn next_u16(&mut self) -> u16 {
            self.0
        }
    }

    struct CountingRng {
        value: u16,
        draws: u32,
    }
    impl BattleRng for CountingRng {
        fn next_u16(&mut self) -> u16 {
            self.draws += 1;
            self.value
        }
    }

    #[test]
    fn crit_stage_for_effect_covers_the_four_high_crit_effects() {
        for effect in HIGH_CRIT_EFFECTS {
            assert_eq!(crit_stage_for_effect(effect), 1, "{effect:?}");
        }
        assert_eq!(crit_stage_for_effect(ORDINARY_HIT_EFFECT), 0);
    }

    #[test]
    fn crit_roll_draws_exactly_once() {
        let mut rng = CountingRng { value: 1, draws: 0 };
        let _ = crit_roll(0, &mut rng);
        assert_eq!(rng.draws, 1);
    }

    #[test]
    fn crit_roll_stage_zero_is_one_in_sixteen() {
        let divisor = CRIT_ROLL_DIVISORS_BY_STAGE[0];
        assert!(crit_roll(0, &mut FixedRng(0)));
        assert!(crit_roll(0, &mut FixedRng(divisor)));
        assert!(!crit_roll(0, &mut FixedRng(1)));
        assert!(!crit_roll(0, &mut FixedRng(divisor - 1)));
    }

    #[test]
    fn maximum_crit_stage_uses_one_in_two_odds() {
        assert!(crit_roll(MAX_CRIT_STAGE, &mut FixedRng(2)));
        assert!(!crit_roll(MAX_CRIT_STAGE, &mut FixedRng(1)));
    }

    #[test]
    fn every_crit_roll_divisor_has_distinct_boundary_behavior() {
        assert_eq!(CRIT_ROLL_DIVISORS_BY_STAGE, [16, 8, 4, 3, 2]);

        let stage_one_only_draw = 8;
        assert!(crit_roll(1, &mut FixedRng(stage_one_only_draw)));
        assert!(!crit_roll(0, &mut FixedRng(stage_one_only_draw)));

        let stage_two_only_draw = 4;
        assert!(crit_roll(2, &mut FixedRng(stage_two_only_draw)));
        assert!(!crit_roll(1, &mut FixedRng(stage_two_only_draw)));

        let stage_three_only_draw = 3;
        assert!(crit_roll(3, &mut FixedRng(stage_three_only_draw)));
        assert!(!crit_roll(2, &mut FixedRng(stage_three_only_draw)));
        assert!(!crit_roll(3, &mut FixedRng(stage_two_only_draw)));
    }

    #[test]
    fn crit_roll_clamps_stages_above_the_table() {
        for draw in [0u16, 1, 2, 3] {
            assert_eq!(
                crit_roll(MAX_CRIT_STAGE, &mut FixedRng(draw)),
                crit_roll(STAGE_ABOVE_MAXIMUM, &mut FixedRng(draw)),
                "draw {draw}"
            );
        }
    }

    #[test]
    fn crit_adjusted_stages_are_unchanged_on_a_non_critical_hit() {
        let attack = StatStage::new(-2).unwrap();
        let defense = StatStage::new(3).unwrap();
        assert_eq!(
            crit_adjusted_stages(attack, defense, false),
            (attack, defense)
        );
    }

    #[test]
    fn crit_ignores_an_attack_drop_but_keeps_an_attack_boost() {
        let dropped = StatStage::new(-3).unwrap();
        let (attack, _) = crit_adjusted_stages(dropped, StatStage::NEUTRAL, true);
        assert_eq!(attack, StatStage::NEUTRAL);

        let boosted = StatStage::new(2).unwrap();
        let (attack, _) = crit_adjusted_stages(boosted, StatStage::NEUTRAL, true);
        assert_eq!(attack, boosted);
    }

    #[test]
    fn crit_ignores_a_defense_boost_but_keeps_a_defense_drop() {
        let boosted = StatStage::new(4).unwrap();
        let (_, defense) = crit_adjusted_stages(StatStage::NEUTRAL, boosted, true);
        assert_eq!(defense, StatStage::NEUTRAL);

        let dropped = StatStage::new(-1).unwrap();
        let (_, defense) = crit_adjusted_stages(StatStage::NEUTRAL, dropped, true);
        assert_eq!(defense, dropped);
    }

    #[test]
    fn crit_at_neutral_stages_stays_neutral() {
        let (attack, defense) = crit_adjusted_stages(StatStage::NEUTRAL, StatStage::NEUTRAL, true);
        assert_eq!(attack, StatStage::NEUTRAL);
        assert_eq!(defense, StatStage::NEUTRAL);
    }
}
