//! Closed-form experience-growth curves.
//!
//! `pokeemerald/src/data/pokemon/experience_tables.h` stores explicit values
//! for levels zero and one before applying its formulas. Higher levels use
//! integer division here to preserve the formulas' truncation.

use crate::error::AssetError;
use crate::species::GrowthRate;

/// The highest supported Pokémon level.
pub const MAX_LEVEL: u8 = 100;

const fn cube(n: u32) -> u32 {
    n * n * n
}

const fn medium_fast_curve(n: u32) -> u32 {
    cube(n)
}

const fn fast_curve(n: u32) -> u32 {
    4 * cube(n) / 5
}

const fn slow_curve(n: u32) -> u32 {
    5 * cube(n) / 4
}

const fn medium_slow_curve(n: u32) -> u32 {
    6 * cube(n) / 5 + 100 * n - 15 * n * n - 140
}

const fn erratic_curve(n: u32) -> u32 {
    if n <= 50 {
        (100 - n) * cube(n) / 50
    } else if n <= 68 {
        (150 - n) * cube(n) / 100
    } else if n <= 98 {
        ((1911 - 10 * n) / 3) * cube(n) / 500
    } else {
        (160 - n) * cube(n) / 100
    }
}

const fn fluctuating_curve(n: u32) -> u32 {
    if n <= 15 {
        ((n + 1) / 3 + 24) * cube(n) / 50
    } else if n <= 36 {
        (n + 14) * cube(n) / 50
    } else {
        (n / 2 + 32) * cube(n) / 50
    }
}

/// The total experience needed to reach `level` on `growth_rate`'s curve.
///
/// # Errors
///
/// Returns [`AssetError::InvalidLevel`] if `level` exceeds [`MAX_LEVEL`].
///
/// # Behaviour
///
/// Levels zero and one return zero and one for every growth rate. Higher
/// levels use the selected growth formula.
pub fn experience_for_level(growth_rate: GrowthRate, level: u8) -> Result<u32, AssetError> {
    if level > MAX_LEVEL {
        return Err(AssetError::InvalidLevel(level));
    }
    if level == 0 {
        return Ok(0);
    }
    if level == 1 {
        return Ok(1);
    }
    let n = u32::from(level);
    Ok(match growth_rate {
        GrowthRate::MediumFast => medium_fast_curve(n),
        GrowthRate::Erratic => erratic_curve(n),
        GrowthRate::Fluctuating => fluctuating_curve(n),
        GrowthRate::MediumSlow => medium_slow_curve(n),
        GrowthRate::Fast => fast_curve(n),
        GrowthRate::Slow => slow_curve(n),
    })
}

#[cfg(test)]
mod tests {
    use super::{experience_for_level, MAX_LEVEL};
    use crate::error::AssetError;
    use crate::species::GrowthRate;

    const CURVES: [GrowthRate; 6] = [
        GrowthRate::MediumFast,
        GrowthRate::Erratic,
        GrowthRate::Fluctuating,
        GrowthRate::MediumSlow,
        GrowthRate::Fast,
        GrowthRate::Slow,
    ];

    #[test]
    fn max_level_matches_upstream() {
        assert_eq!(MAX_LEVEL, 100);
    }

    #[test]
    fn level_zero_and_one_are_zero_and_one_for_every_curve() {
        for &curve in &CURVES {
            assert_eq!(experience_for_level(curve, 0), Ok(0), "{curve:?} level 0");
            assert_eq!(experience_for_level(curve, 1), Ok(1), "{curve:?} level 1");
        }
    }

    #[test]
    fn level_100_matches_known_curve_totals() {
        assert_eq!(
            experience_for_level(GrowthRate::MediumFast, MAX_LEVEL),
            Ok(1_000_000)
        );
        assert_eq!(
            experience_for_level(GrowthRate::Erratic, MAX_LEVEL),
            Ok(600_000)
        );
        assert_eq!(
            experience_for_level(GrowthRate::Fluctuating, MAX_LEVEL),
            Ok(1_640_000)
        );
        assert_eq!(
            experience_for_level(GrowthRate::MediumSlow, MAX_LEVEL),
            Ok(1_059_860)
        );
        assert_eq!(
            experience_for_level(GrowthRate::Fast, MAX_LEVEL),
            Ok(800_000)
        );
        assert_eq!(
            experience_for_level(GrowthRate::Slow, MAX_LEVEL),
            Ok(1_250_000)
        );
    }

    #[test]
    fn erratic_breakpoints() {
        assert_eq!(experience_for_level(GrowthRate::Erratic, 50), Ok(125_000));
        assert_eq!(experience_for_level(GrowthRate::Erratic, 51), Ok(131_324));
        assert_eq!(experience_for_level(GrowthRate::Erratic, 68), Ok(257_834));
        assert_eq!(experience_for_level(GrowthRate::Erratic, 69), Ok(267_406));
        assert_eq!(experience_for_level(GrowthRate::Erratic, 98), Ok(583_539));
        assert_eq!(experience_for_level(GrowthRate::Erratic, 99), Ok(591_882));
    }

    #[test]
    fn fluctuating_breakpoints() {
        assert_eq!(experience_for_level(GrowthRate::Fluctuating, 15), Ok(1_957));
        assert_eq!(experience_for_level(GrowthRate::Fluctuating, 16), Ok(2_457));
        assert_eq!(
            experience_for_level(GrowthRate::Fluctuating, 36),
            Ok(46_656)
        );
        assert_eq!(
            experience_for_level(GrowthRate::Fluctuating, 37),
            Ok(50_653)
        );
    }

    #[test]
    fn out_of_range_level_errors() {
        for &curve in &CURVES {
            assert_eq!(
                experience_for_level(curve, MAX_LEVEL + 1),
                Err(AssetError::InvalidLevel(MAX_LEVEL + 1))
            );
            assert_eq!(
                experience_for_level(curve, u8::MAX),
                Err(AssetError::InvalidLevel(u8::MAX))
            );
        }
    }

    #[test]
    fn every_curve_is_monotonic_over_the_full_level_range() {
        for &curve in &CURVES {
            let mut prev = experience_for_level(curve, 0).unwrap();
            assert_eq!(prev, 0);
            for level in 1..=MAX_LEVEL {
                let value = experience_for_level(curve, level).unwrap();
                assert!(
                    value >= prev,
                    "{curve:?} decreased from {prev} to {value} at level {level}"
                );
                prev = value;
            }
        }
    }
}
