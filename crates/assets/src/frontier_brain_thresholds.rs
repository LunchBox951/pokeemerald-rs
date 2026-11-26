//! The Frontier Brain appearance thresholds: for each of the seven Battle
//! Frontier facilities, the win streaks at which its Brain is scheduled.
//!
//! Source: `sFrontierBrainStreakAppearances` in `src/frontier_util.c`
//! (`:86-95`, read at `:1659-1689` and `:2595-2600`), indexed by the
//! `FRONTIER_FACILITY_*` values in `include/constants/battle_frontier.h`.
//! `src/battle_pike.c:467-477` carries a second, byte-identical copy of the
//! table and reads only its Battle Pike row (`:1516-1525`); both copies are
//! one table here.
//!
//! The streak adjustment is not a threshold. `GetFrontierBrainStatus`
//! (`src/frontier_util.c:1662`) adds it to the player's win streak before
//! comparing, and the Pike reader (`src/battle_pike.c:1516-1525`) subtracts
//! it from each threshold instead. It is `1` for every facility except the
//! Battle Dome and the Battle Pyramid, where it is `0`.
//!
//! Data only: the Brain eligibility check, symbol awarding, the Brain teams
//! and speeches, and any battle wiring are not ported here. A facility id
//! outside the table is refused rather than clamped.

use std::error::Error;
use std::fmt;

/// The number of Battle Frontier facilities with a Brain
/// (`NUM_FRONTIER_FACILITIES`).
pub const FRONTIER_FACILITY_COUNT: usize = 7;

/// A failed lookup in the Frontier Brain thresholds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontierBrainThresholdsError {
    /// The facility is not below [`FRONTIER_FACILITY_COUNT`].
    InvalidFacility(usize),
}

impl fmt::Display for FrontierBrainThresholdsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFacility(facility) => write!(
                f,
                "Battle Frontier facility `{facility}` is out of range (there are \
                 {FRONTIER_FACILITY_COUNT})"
            ),
        }
    }
}

impl Error for FrontierBrainThresholdsError {}

/// One facility's four threshold bytes, in upstream's column order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrontierBrainThresholds {
    silver_streak: u8,
    gold_streak: u8,
    repeat_interval: u8,
    streak_adjustment: u8,
}

impl FrontierBrainThresholds {
    /// Returns the adjusted win streak at which the Brain offers the Silver
    /// Symbol, and later the first streak rematch.
    #[must_use]
    pub const fn silver_streak(self) -> u8 {
        self.silver_streak
    }

    /// Returns the adjusted win streak at which the Brain offers the Gold
    /// Symbol, and later the first long-streak rematch.
    #[must_use]
    pub const fn gold_streak(self) -> u8 {
        self.gold_streak
    }

    /// Returns the number of wins between long-streak rematches past the Gold
    /// streak.
    #[must_use]
    pub const fn repeat_interval(self) -> u8 {
        self.repeat_interval
    }

    /// Returns the amount added to the player's win streak before it is
    /// compared with the other three values.
    #[must_use]
    pub const fn streak_adjustment(self) -> u8 {
        self.streak_adjustment
    }
}

/// The thresholds, indexed by `FRONTIER_FACILITY_*`: Tower, Dome, Palace,
/// Arena, Factory, Pike, Pyramid.
const THRESHOLDS: [FrontierBrainThresholds; FRONTIER_FACILITY_COUNT] = [
    row(35, 70, 35, 1),
    row(4, 9, 5, 0),
    row(21, 42, 21, 1),
    row(28, 56, 28, 1),
    row(21, 42, 21, 1),
    row(28, 140, 56, 1),
    row(21, 70, 35, 0),
];

/// Builds a table row from upstream's four bytes in their column order.
const fn row(
    silver_streak: u8,
    gold_streak: u8,
    repeat_interval: u8,
    streak_adjustment: u8,
) -> FrontierBrainThresholds {
    FrontierBrainThresholds {
        silver_streak,
        gold_streak,
        repeat_interval,
        streak_adjustment,
    }
}

/// Returns the Brain appearance thresholds of a facility, indexed as
/// `FRONTIER_FACILITY_*` (`0` for the Battle Tower through `6` for the
/// Battle Pyramid).
///
/// # Errors
///
/// Returns [`FrontierBrainThresholdsError::InvalidFacility`] when `facility`
/// is not below [`FRONTIER_FACILITY_COUNT`].
pub fn frontier_brain_thresholds(
    facility: usize,
) -> Result<FrontierBrainThresholds, FrontierBrainThresholdsError> {
    THRESHOLDS
        .get(facility)
        .copied()
        .ok_or(FrontierBrainThresholdsError::InvalidFacility(facility))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The four columns of the upstream table, one array per column, in
    /// facility order.
    const SILVER: [u8; 7] = [35, 4, 21, 28, 21, 28, 21];
    const GOLD: [u8; 7] = [70, 9, 42, 56, 42, 140, 70];
    const INTERVAL: [u8; 7] = [35, 5, 21, 28, 21, 56, 35];
    const ADJUSTMENT: [u8; 7] = [1, 0, 1, 1, 1, 1, 0];

    #[test]
    fn every_facility_row_is_pinned_field_by_field() {
        assert_eq!(FRONTIER_FACILITY_COUNT, 7);
        for facility in 0..7 {
            let row = frontier_brain_thresholds(facility).expect("facility is in range");
            assert_eq!(row.silver_streak(), SILVER[facility], "silver {facility}");
            assert_eq!(row.gold_streak(), GOLD[facility], "gold {facility}");
            assert_eq!(
                row.repeat_interval(),
                INTERVAL[facility],
                "interval {facility}"
            );
            assert_eq!(
                row.streak_adjustment(),
                ADJUSTMENT[facility],
                "adjustment {facility}"
            );
        }
    }

    #[test]
    fn dome_and_pyramid_alone_have_no_streak_adjustment() {
        let zero: Vec<usize> = (0..7)
            .filter(|&facility| {
                frontier_brain_thresholds(facility)
                    .expect("facility is in range")
                    .streak_adjustment()
                    == 0
            })
            .collect();
        // Facility 1 is the Dome and 6 the Pyramid.
        assert_eq!(zero, [1, 6]);
    }

    #[test]
    fn a_facility_outside_the_table_is_refused() {
        for facility in [7, 8, 255, usize::MAX] {
            assert_eq!(
                frontier_brain_thresholds(facility),
                Err(FrontierBrainThresholdsError::InvalidFacility(facility))
            );
        }
    }

    #[test]
    fn the_error_names_the_offending_facility() {
        assert_eq!(
            FrontierBrainThresholdsError::InvalidFacility(9).to_string(),
            "Battle Frontier facility `9` is out of range (there are 7)"
        );
    }

    /// Reads a file of the pinned upstream checkout.
    fn upstream_file(relative: &str) -> String {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(std::path::Path::parent)
            .expect("crate sits two levels below the workspace root")
            .join("pokeemerald");
        std::fs::read_to_string(root.join(relative)).unwrap_or_else(|error| {
            panic!("cannot read pokeemerald/{relative}: {error}; run ./init.sh")
        })
    }

    /// The `FRONTIER_FACILITY_*` ids, by name, from the upstream header.
    fn facility_ids() -> std::collections::HashMap<String, usize> {
        upstream_file("include/constants/battle_frontier.h")
            .lines()
            .filter_map(|line| {
                let mut words = line.split_whitespace();
                (words.next()? == "#define").then_some(())?;
                let name = words.next()?;
                name.starts_with("FRONTIER_FACILITY_").then_some(())?;
                Some((name.to_owned(), words.next()?.parse().ok()?))
            })
            .collect()
    }

    /// Parses the table out of one upstream source file, as rows of four
    /// bytes in facility order.
    fn parse_copy(source: &str, ids: &std::collections::HashMap<String, usize>) -> Vec<[u8; 4]> {
        let declaration = "sFrontierBrainStreakAppearances[NUM_FRONTIER_FACILITIES][4]";
        let start = source
            .find(declaration)
            .unwrap_or_else(|| panic!("`{declaration}` not found"));
        let body = &source[start..];
        let open = body.find("=\n{").expect("initializer opens") + 3;
        let close = body.find("\n};").expect("initializer closes");
        let mut rows = vec![None; ids.len().max(FRONTIER_FACILITY_COUNT)];
        for line in body[open..close].trim().lines() {
            let (index, values) = line.trim().split_once(']').expect("indexed row");
            let facility = ids[index.trim_start_matches('[').trim()];
            let values = values
                .trim()
                .strip_prefix('=')
                .unwrap_or_else(|| panic!("`=` expected: `{line}`"))
                .trim()
                .trim_end_matches(',')
                .strip_prefix('{')
                .and_then(|rest| rest.strip_suffix('}'))
                .unwrap_or_else(|| panic!("braced row expected: `{line}`"));
            let bytes: Vec<u8> = values
                .split(',')
                .map(|value| value.trim().parse().expect("a byte"))
                .collect();
            rows[facility] = Some(<[u8; 4]>::try_from(bytes).expect("four bytes"));
        }
        rows.into_iter()
            .map(|row| row.expect("every facility has a row"))
            .collect()
    }

    #[test]
    #[ignore = "reads the pinned upstream checkout; run ./init.sh first"]
    fn both_upstream_copies_match_the_table() {
        let ids = facility_ids();
        assert_eq!(ids.len(), FRONTIER_FACILITY_COUNT);
        let expected: Vec<[u8; 4]> = (0..FRONTIER_FACILITY_COUNT)
            .map(|facility| {
                let row = frontier_brain_thresholds(facility).expect("in range");
                [
                    row.silver_streak(),
                    row.gold_streak(),
                    row.repeat_interval(),
                    row.streak_adjustment(),
                ]
            })
            .collect();
        for file in ["src/frontier_util.c", "src/battle_pike.c"] {
            assert_eq!(parse_copy(&upstream_file(file), &ids), expected, "{file}");
        }
    }
}
