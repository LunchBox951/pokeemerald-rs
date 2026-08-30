//! Bounded battle-stat stages and their multipliers.
//!
//! [`StatStage`] exposes the game's `-6..=6` stages as signed offsets while
//! retaining the table index needed by the battle formulas.

use crate::error::BattleError;

/// A Pokémon's bounded `-6..=6` stat-stage offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StatStage {
    offset: i8,
}

impl StatStage {
    const COUNT: usize = 13;

    /// The unmodified stage.
    pub const NEUTRAL: StatStage = StatStage { offset: 0 };

    /// The lowest stage.
    pub const MIN: StatStage = StatStage { offset: -6 };

    /// The highest stage.
    pub const MAX: StatStage = StatStage { offset: 6 };

    /// Build a stage from a signed offset.
    ///
    /// # Errors
    ///
    /// Returns [`BattleError::StatStageOutOfRange`] if `offset` is outside
    /// `-6..=6`.
    pub const fn new(offset: i8) -> Result<Self, BattleError> {
        if offset < Self::MIN.offset || offset > Self::MAX.offset {
            Err(BattleError::StatStageOutOfRange(offset))
        } else {
            Ok(Self { offset })
        }
    }

    /// The signed offset (`-6..=6`).
    #[must_use]
    pub const fn offset(self) -> i8 {
        self.offset
    }

    /// The `0..=12` index into tables ordered from [`Self::MIN`] to [`Self::MAX`].
    #[must_use]
    pub const fn raw_index(self) -> u8 {
        let index_from_minimum = self.offset - Self::MIN.offset;
        #[expect(
            clippy::cast_sign_loss,
            reason = "StatStage guarantees index_from_minimum is in 0..=12"
        )]
        let raw_index = index_from_minimum as u8;
        raw_index
    }

    /// The `(numerator, denominator)` multiplier for this stage.
    #[must_use]
    pub const fn ratio(self) -> (u32, u32) {
        let ratio = RATIOS[self.raw_index() as usize];
        (ratio.numerator, ratio.denominator)
    }

    /// Apply this stage to a base stat value.
    ///
    /// Multiplication precedes integer division, matching `APPLY_STAT_MOD`
    /// (`pokeemerald/src/pokemon.c:3100`) when the ratio does not divide evenly.
    #[must_use]
    pub const fn apply(self, stat: u32) -> u32 {
        let (numerator, denominator) = self.ratio();
        stat * numerator / denominator
    }

    /// Add `delta`, clamping the result to [`Self::MIN`]`..=`[`Self::MAX`].
    #[must_use]
    pub fn saturating_add(self, delta: i8) -> Self {
        let offset = self
            .offset
            .saturating_add(delta)
            .clamp(Self::MIN.offset, Self::MAX.offset);
        Self { offset }
    }
}

impl Default for StatStage {
    fn default() -> Self {
        Self::NEUTRAL
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StatRatio {
    numerator: u32,
    denominator: u32,
}

/// Stat multipliers ordered from [`StatStage::MIN`] through [`StatStage::MAX`].
const RATIOS: [StatRatio; StatStage::COUNT] = [
    StatRatio {
        numerator: 10,
        denominator: 40,
    },
    StatRatio {
        numerator: 10,
        denominator: 35,
    },
    StatRatio {
        numerator: 10,
        denominator: 30,
    },
    StatRatio {
        numerator: 10,
        denominator: 25,
    },
    StatRatio {
        numerator: 10,
        denominator: 20,
    },
    StatRatio {
        numerator: 10,
        denominator: 15,
    },
    StatRatio {
        numerator: 10,
        denominator: 10,
    },
    StatRatio {
        numerator: 15,
        denominator: 10,
    },
    StatRatio {
        numerator: 20,
        denominator: 10,
    },
    StatRatio {
        numerator: 25,
        denominator: 10,
    },
    StatRatio {
        numerator: 30,
        denominator: 10,
    },
    StatRatio {
        numerator: 35,
        denominator: 10,
    },
    StatRatio {
        numerator: 40,
        denominator: 10,
    },
];

#[cfg(test)]
mod tests {
    use super::{StatRatio, StatStage, RATIOS};
    use crate::error::BattleError;

    #[test]
    fn ratios_table_matches_upstream_gstatstageratios() {
        assert_eq!(RATIOS.len(), 13);
        assert_eq!(
            RATIOS[0],
            StatRatio {
                numerator: 10,
                denominator: 40,
            }
        );
        assert_eq!(
            RATIOS[6],
            StatRatio {
                numerator: 10,
                denominator: 10,
            }
        );
        assert_eq!(
            RATIOS[12],
            StatRatio {
                numerator: 40,
                denominator: 10,
            }
        );
    }

    #[test]
    fn new_accepts_the_full_range_and_rejects_outside_it() {
        assert_eq!(StatStage::new(-6), Ok(StatStage::MIN));
        assert_eq!(StatStage::new(0), Ok(StatStage::NEUTRAL));
        assert_eq!(StatStage::new(6), Ok(StatStage::MAX));
        assert_eq!(
            StatStage::new(-7),
            Err(BattleError::StatStageOutOfRange(-7))
        );
        assert_eq!(StatStage::new(7), Err(BattleError::StatStageOutOfRange(7)));
    }

    #[test]
    fn raw_index_matches_upstream_statstages_byte() {
        assert_eq!(StatStage::MIN.raw_index(), 0);
        assert_eq!(StatStage::NEUTRAL.raw_index(), 6);
        assert_eq!(StatStage::MAX.raw_index(), 12);
        assert_eq!(StatStage::new(-2).unwrap().raw_index(), 4);
        assert_eq!(StatStage::new(3).unwrap().raw_index(), 9);
    }

    #[test]
    fn neutral_stage_does_not_change_the_stat() {
        assert_eq!(StatStage::NEUTRAL.apply(100), 100);
        assert_eq!(StatStage::NEUTRAL.apply(1), 1);
    }

    #[test]
    fn min_and_max_stage_landmarks() {
        assert_eq!(StatStage::MIN.apply(100), 25);
        assert_eq!(StatStage::MAX.apply(100), 400);
    }

    #[test]
    fn every_positive_and_negative_stage_matches_hand_computed_multipliers() {
        let expected_by_stage = [
            (-5, 100, 28),
            (-4, 100, 33),
            (-3, 100, 40),
            (-2, 100, 50),
            (-1, 100, 66),
            (1, 100, 150),
            (2, 100, 200),
            (3, 100, 250),
            (4, 100, 300),
            (5, 100, 350),
        ];
        for (offset, stat, expected) in expected_by_stage {
            let stage = StatStage::new(offset).unwrap();
            assert_eq!(stage.apply(stat), expected, "stage {offset:+}");
        }
    }

    #[test]
    fn apply_uses_multiply_then_divide_not_a_fused_fraction() {
        let stage = StatStage::new(-1).unwrap();
        assert_eq!(stage.apply(3), 2);
        let (numerator, denominator) = stage.ratio();
        let divide_before_multiply = numerator / denominator * 3;
        assert_eq!(divide_before_multiply, 0);
    }

    #[test]
    fn default_is_neutral() {
        assert_eq!(StatStage::default(), StatStage::NEUTRAL);
    }

    #[test]
    fn saturating_add_steps_within_range() {
        assert_eq!(
            StatStage::NEUTRAL.saturating_add(-1),
            StatStage::new(-1).unwrap()
        );
        assert_eq!(
            StatStage::new(2).unwrap().saturating_add(1),
            StatStage::new(3).unwrap()
        );
    }

    #[test]
    fn saturating_add_clamps_at_the_floor_and_ceiling() {
        assert_eq!(StatStage::MIN.saturating_add(-1), StatStage::MIN);
        assert_eq!(StatStage::MIN.saturating_add(-6), StatStage::MIN);
        assert_eq!(StatStage::MAX.saturating_add(1), StatStage::MAX);
        assert_eq!(
            StatStage::new(-5).unwrap().saturating_add(-1),
            StatStage::MIN
        );
    }
}
