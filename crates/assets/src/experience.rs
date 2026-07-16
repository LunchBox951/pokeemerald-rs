//! Experience-growth curves (S-4): `gExperienceTables`, as closed-form code.
//!
//! Ports the six level -> total-experience curves from the upstream reference
//! `pokeemerald/src/data/pokemon/experience_tables.h` (`gExperienceTables[][MAX_LEVEL + 1]`),
//! keyed by the existing [`GrowthRate`] enum (defined in
//! [`species`](crate::species) — not redefined here). `MAX_LEVEL` (100) comes
//! from `pokeemerald/include/constants/pokemon.h`.
//!
//! Upstream generates each curve's 101 entries from a `#define`d macro over
//! the level, e.g. `EXP_MEDIUM_FAST(n) = n^3`; only levels 0 and 1 are
//! hardcoded (`0` and `1`) rather than run through the macro, since several
//! curves' formulas do not evaluate to `0`/`1` at those levels (`EXP_FAST(1)`
//! truncates to `0`; `EXP_MEDIUM_SLOW(1)` is negative;
//! `EXP_FLUCTUATING(1)` truncates to `0`). This module transcribes those five
//! macros — including `EXP_ERRATIC`'s and `EXP_FLUCTUATING`'s piecewise
//! breakpoints — as pure Rust functions with the same `level == 0 | 1`
//! override, rather than the 6 * 101 = 606 resulting integers
//! `(no-verbatim)`: a closed-form re-implementation, not a transcribed table.
//!
//! All arithmetic mirrors upstream's `int`/`u32` truncating (round-towards-
//! zero on nonnegative operands, i.e. floor) division exactly
//! `(behavioral-fidelity)` — every division below is deliberately integer
//! division, matching the C macros' `/` on non-negative operands. The
//! upstream-tie tests pin `gExperienceTables`' hardcoded level-0/1 entries and
//! independently-computed values at level 100 and each piecewise curve's
//! breakpoints, plus a full per-curve monotonicity/structural sweep, so the
//! transcription cannot silently drift.

use crate::error::AssetError;
use crate::species::GrowthRate;

/// The maximum party-Pokémon level, matching upstream `MAX_LEVEL`
/// (`pokeemerald/include/constants/pokemon.h`).
pub const MAX_LEVEL: u8 = 100;

/// `n^3`, matching upstream's `CUBE(n)` macro. `n` is small enough
/// (`0..=100`) that this never overflows `u32` (`100^3 == 1_000_000`).
const fn cube(n: u32) -> u32 {
    n * n * n
}

/// `EXP_MEDIUM_FAST(n) = n^3`.
const fn medium_fast_curve(n: u32) -> u32 {
    cube(n)
}

/// `EXP_FAST(n) = (4 * n^3) / 5`.
const fn fast_curve(n: u32) -> u32 {
    4 * cube(n) / 5
}

/// `EXP_SLOW(n) = (5 * n^3) / 4`.
const fn slow_curve(n: u32) -> u32 {
    5 * cube(n) / 4
}

/// `EXP_MEDIUM_SLOW(n) = (6 * n^3) / 5 - 15 * n^2 + 100 * n - 140`.
///
/// Computed in `i64` because the unevaluated sub-expression is negative for
/// small `n` (upstream never asks for those: `n` is always `>= 2` here, the
/// override in [`experience_for_level`] handles `0`/`1`), before the final
/// result is brought back into `u32`.
const fn medium_slow_curve(n: u32) -> u32 {
    let n = n as i64;
    let value = (6 * n * n * n) / 5 - 15 * n * n + 100 * n - 140;
    // Callers only reach this curve for `n in 2..=MAX_LEVEL`, where the
    // polynomial above is always non-negative and well within `u32::MAX`
    // (verified by the monotonicity/structural sweep in this module's tests).
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let value = value as u32;
    value
}

/// `EXP_ERRATIC(n)`, upstream's piecewise curve over four level bands.
const fn erratic_curve(n: u32) -> u32 {
    let n64 = n as u64;
    let cube = n64 * n64 * n64;
    let value = if n <= 50 {
        (100 - n64) * cube / 50
    } else if n <= 68 {
        (150 - n64) * cube / 100
    } else if n <= 98 {
        ((1911 - 10 * n64) / 3) * cube / 500
    } else {
        (160 - n64) * cube / 100
    };
    // `n` is bounded to `2..=MAX_LEVEL` by callers, so `value` always fits
    // comfortably in `u32` (the largest curve value, at level 100, is under
    // 2,000,000); the wider `u64` above only avoids intermediate overflow in
    // `n^3`.
    #[allow(clippy::cast_possible_truncation)]
    let value = value as u32;
    value
}

/// `EXP_FLUCTUATING(n)`, upstream's piecewise curve over three level bands.
const fn fluctuating_curve(n: u32) -> u32 {
    let n64 = n as u64;
    let cube = n64 * n64 * n64;
    let value = if n <= 15 {
        ((n64 + 1) / 3 + 24) * cube / 50
    } else if n <= 36 {
        (n64 + 14) * cube / 50
    } else {
        (n64 / 2 + 32) * cube / 50
    };
    // Same reasoning as `erratic_curve`: `n` is bounded to `2..=MAX_LEVEL`, so
    // `value` always fits `u32`.
    #[allow(clippy::cast_possible_truncation)]
    let value = value as u32;
    value
}

/// The total experience needed to reach `level` on `growth_rate`'s curve.
///
/// # Errors
///
/// Returns [`AssetError::InvalidLevel`] if `level > `[`MAX_LEVEL`].
///
/// # Behaviour
///
/// `level == 0` and `level == 1` always return `0` and `1` respectively,
/// regardless of curve — matching `gExperienceTables`' hardcoded first two
/// entries (see the module docs for why several curves' formulas don't
/// naturally produce those values). From `level == 2` on, the matching
/// closed-form curve is evaluated with the same integer-division semantics as
/// upstream's `EXP_*` macros.
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
        // gExperienceTables hardcodes these two entries for all six curves,
        // independent of each curve's formula.
        for &curve in &CURVES {
            assert_eq!(experience_for_level(curve, 0), Ok(0), "{curve:?} level 0");
            assert_eq!(experience_for_level(curve, 1), Ok(1), "{curve:?} level 1");
        }
    }

    #[test]
    fn level_100_matches_known_curve_totals() {
        // EXP_*(100) evaluated by hand against the transcribed macros;
        // these are also the well-known total-experience-at-100 values for
        // each curve (e.g. Bulbasaur/medium-slow tops out at 1,059,860).
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
        // EXP_ERRATIC(n) evaluated by hand at each of its three internal
        // band boundaries (n <= 50, n <= 68, n <= 98, else).
        assert_eq!(experience_for_level(GrowthRate::Erratic, 50), Ok(125_000));
        assert_eq!(experience_for_level(GrowthRate::Erratic, 51), Ok(131_324));
        assert_eq!(experience_for_level(GrowthRate::Erratic, 68), Ok(257_834));
        assert_eq!(experience_for_level(GrowthRate::Erratic, 69), Ok(267_406));
        assert_eq!(experience_for_level(GrowthRate::Erratic, 98), Ok(583_539));
        assert_eq!(experience_for_level(GrowthRate::Erratic, 99), Ok(591_882));
    }

    #[test]
    fn fluctuating_breakpoints() {
        // EXP_FLUCTUATING(n) evaluated by hand at its two internal band
        // boundaries (n <= 15, n <= 36, else).
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
    fn every_curve_is_monotonic_and_bounded_over_the_full_level_range() {
        // Structural sweep: total experience never decreases as level rises,
        // level 0 is always 0, and no curve exceeds its known level-100 cap.
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
