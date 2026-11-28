//! The Frontier Maniac streak thresholds: for each of the ten facility and
//! mode choices the Maniac can be asked about, the two win streaks that
//! separate his three comments.
//!
//! Source: `sFrontierManiacStreakThresholds` in `src/field_specials.c`
//! (`:2126-2138`, read by the loop at `:2200`), indexed by the
//! `FRONTIER_MANIAC_*` values in `include/constants/battle_frontier.h`
//! (`:72-82`). The index is the Maniac's own ten-entry one (Tower singles,
//! doubles, multis and link, then Dome, Factory, Palace, Arena, Pike and
//! Pyramid); it is not the seven-entry `FRONTIER_FACILITY_*` id of
//! [`crate::frontier_brain_thresholds`].
//!
//! The values are upstream's as written. The Tower multis and link rows are
//! `255` for both thresholds (win streaks are `u16` upstream, so a streak of
//! 256 or more still passes them), and the Pike
//! row's second threshold, `112`, is marked upstream as probably a mistake;
//! neither is corrected.
//!
//! Data only: the win streak selection, the saved-state reading, the choice
//! of comment and all dialogue are not ported here. A facility index outside
//! the table is refused rather than clamped.

use std::error::Error;
use std::fmt;

/// The number of facility and mode choices the Maniac distinguishes
/// (`FRONTIER_MANIAC_FACILITY_COUNT`).
pub const FRONTIER_MANIAC_FACILITY_COUNT: usize = 10;

/// A failed lookup in the Frontier Maniac thresholds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontierManiacThresholdsError {
    /// The index is not below [`FRONTIER_MANIAC_FACILITY_COUNT`].
    InvalidFacility(usize),
}

impl fmt::Display for FrontierManiacThresholdsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFacility(facility) => write!(
                f,
                "Frontier Maniac facility `{facility}` is out of range (there are \
                 {FRONTIER_MANIAC_FACILITY_COUNT})"
            ),
        }
    }
}

impl Error for FrontierManiacThresholdsError {}

/// One facility's two threshold bytes, in upstream's column order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrontierManiacThresholds {
    first: u8,
    second: u8,
}

impl FrontierManiacThresholds {
    /// Returns the first threshold: a win streak at or below it gets the
    /// Maniac's first comment.
    #[must_use]
    pub const fn first(self) -> u8 {
        self.first
    }

    /// Returns the second threshold: a win streak above the first threshold
    /// and at or below this one gets the second comment, and one above it
    /// gets the third. Where the two are equal (`255`) no streak gets the
    /// second comment.
    #[must_use]
    pub const fn second(self) -> u8 {
        self.second
    }
}

/// The thresholds, indexed by `FRONTIER_MANIAC_*`: Tower singles, Tower
/// doubles, Tower multis, Tower link, Dome, Factory, Palace, Arena, Pike,
/// Pyramid.
const THRESHOLDS: [FrontierManiacThresholds; FRONTIER_MANIAC_FACILITY_COUNT] = [
    row(21, 56),
    row(21, 35),
    row(255, 255),
    row(255, 255),
    row(2, 4),
    row(7, 21),
    row(7, 21),
    row(14, 28),
    row(13, 112),
    row(7, 56),
];

/// Builds a table row from upstream's two bytes in their column order.
const fn row(first: u8, second: u8) -> FrontierManiacThresholds {
    FrontierManiacThresholds { first, second }
}

/// Returns the Maniac's streak thresholds for a facility choice, indexed as
/// `FRONTIER_MANIAC_*` (`0` for Tower singles through `9` for the Battle
/// Pyramid).
///
/// # Errors
///
/// Returns [`FrontierManiacThresholdsError::InvalidFacility`] when
/// `facility` is not below [`FRONTIER_MANIAC_FACILITY_COUNT`].
pub fn frontier_maniac_thresholds(
    facility: usize,
) -> Result<FrontierManiacThresholds, FrontierManiacThresholdsError> {
    THRESHOLDS
        .get(facility)
        .copied()
        .ok_or(FrontierManiacThresholdsError::InvalidFacility(facility))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Upstream's table, one `(first, second)` pair per facility choice, in
    /// index order.
    const EXPECTED: [(u8, u8); 10] = [
        (21, 56),
        (21, 35),
        (255, 255),
        (255, 255),
        (2, 4),
        (7, 21),
        (7, 21),
        (14, 28),
        (13, 112),
        (7, 56),
    ];

    #[test]
    fn every_row_is_pinned_in_order() {
        assert_eq!(FRONTIER_MANIAC_FACILITY_COUNT, 10);
        for (facility, &(first, second)) in EXPECTED.iter().enumerate() {
            let row = frontier_maniac_thresholds(facility).expect("facility is in range");
            assert_eq!(row.first(), first, "first {facility}");
            assert_eq!(row.second(), second, "second {facility}");
        }
    }

    #[test]
    fn the_named_rows_from_the_issue_hold() {
        let pair = |facility| {
            let row = frontier_maniac_thresholds(facility).expect("in range");
            (row.first(), row.second())
        };
        assert_eq!(pair(0), (21, 56), "Tower singles");
        assert_eq!(pair(4), (2, 4), "Dome");
        assert_eq!(pair(8), (13, 112), "Pike");
        assert_eq!(pair(9), (7, 56), "Pyramid");
    }

    #[test]
    fn the_tower_multis_and_link_rows_stay_at_255() {
        for facility in [2, 3] {
            let row = frontier_maniac_thresholds(facility).expect("in range");
            assert_eq!((row.first(), row.second()), (255, 255), "{facility}");
        }
    }

    #[test]
    fn an_index_outside_the_table_is_refused() {
        for facility in [10, 11, 255, usize::MAX] {
            assert_eq!(
                frontier_maniac_thresholds(facility),
                Err(FrontierManiacThresholdsError::InvalidFacility(facility))
            );
        }
    }

    #[test]
    fn the_error_names_the_offending_index() {
        assert_eq!(
            FrontierManiacThresholdsError::InvalidFacility(10).to_string(),
            "Frontier Maniac facility `10` is out of range (there are 10)"
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

    #[test]
    #[ignore = "reads the pinned upstream checkout; run ./init.sh first"]
    fn the_table_matches_upstream() {
        let header = upstream_file("include/constants/battle_frontier.h");
        let ids: std::collections::HashMap<&str, usize> = header
            .lines()
            .filter_map(|line| {
                let mut words = line.split_whitespace();
                (words.next()? == "#define").then_some(())?;
                let name = words.next()?;
                name.starts_with("FRONTIER_MANIAC_").then_some(())?;
                Some((name, words.next()?.parse().ok()?))
            })
            .collect();
        assert_eq!(ids["FRONTIER_MANIAC_FACILITY_COUNT"], FRONTIER_MANIAC_FACILITY_COUNT);

        let source = upstream_file("src/field_specials.c");
        let start = source
            .find("sFrontierManiacStreakThresholds[][")
            .expect("table declared");
        let body = &source[start..];
        let open = body.find("=\n").expect("initializer starts");
        let close = body.find("};").expect("initializer ends");
        let mut rows = [None; FRONTIER_MANIAC_FACILITY_COUNT];
        for line in body[open..close].lines() {
            let Some((index, values)) = line.trim().split_once(']') else {
                continue;
            };
            let facility = ids[index.trim_start_matches('[').trim()];
            let values = values
                .split_once('{')
                .and_then(|(_, rest)| rest.split_once('}'))
                .map(|(inside, _)| inside)
                .unwrap_or_else(|| panic!("braced row expected: `{line}`"));
            let bytes: Vec<u8> = values
                .split(',')
                .map(|value| value.trim().parse().expect("a byte"))
                .collect();
            rows[facility] = Some((bytes[0], bytes[1]));
            assert_eq!(bytes.len(), 2, "{line}");
        }
        for (facility, row) in rows.iter().enumerate() {
            assert_eq!(*row, Some(EXPECTED[facility]), "facility {facility}");
        }
    }
}
