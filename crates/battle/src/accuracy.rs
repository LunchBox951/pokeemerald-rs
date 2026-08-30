//! Move accuracy after accuracy and evasion stat stages.
//!
//! Always-hit effects bypass the roll. Other moves combine the two stages and
//! consume one random draw before comparing it with the adjusted accuracy.

use assets::MoveEffect;

use crate::damage::BattleRng;
use crate::stat_stage::StatStage;

/// The move effect for attacks that bypass accuracy rolls.
pub const EFFECT_ALWAYS_HIT: MoveEffect = MoveEffect(17);

/// Vital Throw's always-hit move effect.
pub const EFFECT_VITAL_THROW: MoveEffect = MoveEffect(78);

/// Whether this move effect bypasses the accuracy roll and its RNG draw.
#[must_use]
pub fn always_hits(effect: MoveEffect) -> bool {
    effect == EFFECT_ALWAYS_HIT || effect == EFFECT_VITAL_THROW
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AccuracyRatio {
    dividend: u32,
    divisor: u32,
}

impl AccuracyRatio {
    fn apply_to(self, move_accuracy: u8) -> u32 {
        self.dividend * u32::from(move_accuracy) / self.divisor
    }
}

/// Accuracy multipliers ordered from [`StatStage::MIN`] through [`StatStage::MAX`].
const RATIOS: [AccuracyRatio; 13] = [
    AccuracyRatio {
        dividend: 33,
        divisor: 100,
    },
    AccuracyRatio {
        dividend: 36,
        divisor: 100,
    },
    AccuracyRatio {
        dividend: 43,
        divisor: 100,
    },
    AccuracyRatio {
        dividend: 50,
        divisor: 100,
    },
    AccuracyRatio {
        dividend: 60,
        divisor: 100,
    },
    AccuracyRatio {
        dividend: 75,
        divisor: 100,
    },
    AccuracyRatio {
        dividend: 1,
        divisor: 1,
    },
    AccuracyRatio {
        dividend: 133,
        divisor: 100,
    },
    AccuracyRatio {
        dividend: 166,
        divisor: 100,
    },
    AccuracyRatio {
        dividend: 2,
        divisor: 1,
    },
    AccuracyRatio {
        dividend: 233,
        divisor: 100,
    },
    AccuracyRatio {
        dividend: 133,
        divisor: 50,
    },
    AccuracyRatio {
        dividend: 3,
        divisor: 1,
    },
];

/// Subtract evasion from accuracy and clamp the combined stage to its bounds.
#[must_use]
pub fn combined_stage(accuracy_stage: StatStage, evasion_stage: StatStage) -> StatStage {
    accuracy_stage.saturating_add(-evasion_stage.offset())
}

const ACCURACY_ROLL_RANGE: u32 = 100;

/// Return whether a move hits and consume any required accuracy RNG draw.
///
/// Always-hit effects consume no draw. Every other effect consumes exactly one,
/// even when its adjusted accuracy guarantees a hit, matching
/// `Cmd_accuracycheck` (`pokeemerald/src/battle_script_commands.c:1176`). Zero
/// accuracy therefore misses unless the effect is always-hit.
#[must_use]
pub fn accuracy_check(
    move_accuracy: u8,
    move_effect: MoveEffect,
    accuracy_stage: StatStage,
    evasion_stage: StatStage,
    rng: &mut impl BattleRng,
) -> bool {
    if always_hits(move_effect) {
        return true;
    }
    let effective_stage = combined_stage(accuracy_stage, evasion_stage);
    let ratio = RATIOS[effective_stage.raw_index() as usize];
    let hit_threshold = ratio.apply_to(move_accuracy);
    let accuracy_roll = u32::from(rng.next_u16()) % ACCURACY_ROLL_RANGE + 1;
    accuracy_roll <= hit_threshold
}

#[cfg(test)]
mod tests {
    use super::{
        accuracy_check, always_hits, combined_stage, EFFECT_ALWAYS_HIT, EFFECT_VITAL_THROW,
    };
    use crate::damage::BattleRng;
    use crate::stat_stage::StatStage;
    use assets::MoveEffect;

    const ORDINARY_HIT_EFFECT: MoveEffect = MoveEffect(0);

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
    fn always_hits_covers_swift_and_vital_throw_only() {
        assert!(always_hits(EFFECT_ALWAYS_HIT));
        assert_eq!(EFFECT_VITAL_THROW, MoveEffect(78));
        assert!(always_hits(EFFECT_VITAL_THROW));
        assert!(!always_hits(ORDINARY_HIT_EFFECT));
    }

    #[test]
    fn combined_stage_is_neutral_at_neutral_inputs() {
        assert_eq!(
            combined_stage(StatStage::NEUTRAL, StatStage::NEUTRAL),
            StatStage::NEUTRAL
        );
    }

    #[test]
    fn combined_stage_subtracts_evasion_from_accuracy_and_clamps() {
        let plus_two = StatStage::new(2).unwrap();
        let minus_two = StatStage::new(-2).unwrap();
        assert_eq!(
            combined_stage(plus_two, StatStage::NEUTRAL),
            StatStage::new(2).unwrap()
        );
        assert_eq!(
            combined_stage(StatStage::NEUTRAL, minus_two),
            StatStage::new(2).unwrap()
        );
        assert_eq!(
            combined_stage(StatStage::MIN, StatStage::MAX),
            StatStage::MIN
        );
        assert_eq!(
            combined_stage(StatStage::MAX, StatStage::MIN),
            StatStage::MAX
        );
    }

    #[test]
    fn accuracy_check_bypasses_the_roll_for_always_hit_moves() {
        let mut rng = CountingRng { value: 0, draws: 0 };
        let hit = accuracy_check(
            0,
            EFFECT_ALWAYS_HIT,
            StatStage::NEUTRAL,
            StatStage::NEUTRAL,
            &mut rng,
        );
        assert!(hit);
        assert_eq!(rng.draws, 0, "always-hit moves draw no RNG");
    }

    #[test]
    fn accuracy_check_draws_exactly_once_for_a_normal_move() {
        let mut rng = CountingRng { value: 0, draws: 0 };
        let _ = accuracy_check(
            95,
            ORDINARY_HIT_EFFECT,
            StatStage::NEUTRAL,
            StatStage::NEUTRAL,
            &mut rng,
        );
        assert_eq!(rng.draws, 1);
    }

    #[test]
    fn accuracy_check_uses_a_one_through_one_hundred_roll() {
        for draw in [0u16, 50, 99, 100, 65535] {
            let mut rng = FixedRng(draw);
            assert!(accuracy_check(
                100,
                ORDINARY_HIT_EFFECT,
                StatStage::NEUTRAL,
                StatStage::NEUTRAL,
                &mut rng,
            ));
        }
    }

    #[test]
    fn accuracy_check_misses_when_the_roll_exceeds_calc() {
        let first_miss_draw = 50;
        let mut rng = FixedRng(first_miss_draw);
        assert!(!accuracy_check(
            50,
            ORDINARY_HIT_EFFECT,
            StatStage::NEUTRAL,
            StatStage::NEUTRAL,
            &mut rng,
        ));
        let last_hit_draw = 49;
        let mut rng = FixedRng(last_hit_draw);
        assert!(accuracy_check(
            50,
            ORDINARY_HIT_EFFECT,
            StatStage::NEUTRAL,
            StatStage::NEUTRAL,
            &mut rng,
        ));
    }

    #[test]
    fn accuracy_check_positive_stage_scales_up_hit_chance() {
        let plus_six = StatStage::MAX;
        for draw in [0u16, 99] {
            let mut rng = FixedRng(draw);
            assert!(accuracy_check(
                50,
                ORDINARY_HIT_EFFECT,
                plus_six,
                StatStage::NEUTRAL,
                &mut rng,
            ));
        }
    }

    #[test]
    fn every_accuracy_stage_ratio_is_pinned_at_its_own_boundary() {
        let last_hit_roll_by_stage = [
            (-6i8, 50u8, 16u16),
            (-5, 50, 18),
            (-4, 50, 21),
            (-3, 50, 25),
            (-2, 50, 30),
            (-1, 50, 37),
            (0, 50, 50),
            (1, 50, 66),
            (2, 50, 83),
            (3, 30, 60),
            (4, 30, 69),
            (5, 30, 79),
            (6, 30, 90),
        ];
        for (offset, base_accuracy, last_hit_roll) in last_hit_roll_by_stage {
            let stage = StatStage::new(offset).unwrap();
            let mut rng = FixedRng(last_hit_roll - 1);
            assert!(
                accuracy_check(
                    base_accuracy,
                    ORDINARY_HIT_EFFECT,
                    stage,
                    StatStage::NEUTRAL,
                    &mut rng,
                ),
                "stage {offset:+}: roll {last_hit_roll} must hit"
            );
            let mut rng = FixedRng(last_hit_roll);
            assert!(
                !accuracy_check(
                    base_accuracy,
                    ORDINARY_HIT_EFFECT,
                    stage,
                    StatStage::NEUTRAL,
                    &mut rng,
                ),
                "stage {offset:+}: roll {} must miss",
                last_hit_roll + 1
            );
        }
    }
}
