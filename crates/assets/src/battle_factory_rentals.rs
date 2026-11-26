//! The Battle Factory fixed-IV rows and the initial rental Frontier-mon ranges.
//!
//! Source: `sFixedIVTable` and `sInitialRentalMonRanges`
//! (`src/battle_factory.c:157-190`). `GetFactoryMonFixedIV` (`:739-757`) reads
//! a row by challenge number and takes its second column for the last battle
//! of a challenge, its first otherwise. The initial-rental reader (`:839-862`)
//! adds `8` to the row for Open Level, so the Level 50 rows are the first
//! eight of the range table and the Open Level rows the next eight. Its
//! `[0]` and `[1]` are both inclusive Frontier-mon indices: the draw spans
//! `[1] - [0] + 1` values. The ranges' endpoints are the
//! `FRONTIER_MON*` ids in `include/constants/battle_frontier_mons.h`.
//!
//! Upstream's reader overruns the IV table at challenge number `8` (and clamps
//! larger ones to the last row); neither is modelled here. A row outside a
//! table is refused rather than clamped.
//!
//! Data only: rental generation, the RNG draws, the streak-to-row selection
//! and its clamping, the Pokémon sets and save state are not ported here.

use std::error::Error;
use std::fmt;

/// The number of rows in the fixed-IV table.
pub const FIXED_IV_ROWS: usize = 8;

/// The number of rows in the initial rental range table of one level mode.
pub const RENTAL_RANGE_ROWS: usize = 8;

/// A failed lookup in the Battle Factory rental tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactoryRentalsError {
    /// The fixed-IV row is not below [`FIXED_IV_ROWS`].
    InvalidFixedIvRow(usize),
    /// The rental range row is not below [`RENTAL_RANGE_ROWS`].
    InvalidRentalRangeRow(usize),
}

impl fmt::Display for FactoryRentalsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFixedIvRow(row) => write!(
                f,
                "Battle Factory fixed-IV row `{row}` is out of range (there are {FIXED_IV_ROWS})"
            ),
            Self::InvalidRentalRangeRow(row) => write!(
                f,
                "Battle Factory rental range row `{row}` is out of range (there are \
                 {RENTAL_RANGE_ROWS} per level mode)"
            ),
        }
    }
}

impl Error for FactoryRentalsError {}

/// The level mode of a rental range (`FRONTIER_LVL_50` or
/// `FRONTIER_LVL_OPEN`, `include/constants/global.h:76-77`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FactoryLevelMode {
    /// The Level 50 mode (`FRONTIER_LVL_50`, `0`).
    Level50 = 0,
    /// The Open Level mode (`FRONTIER_LVL_OPEN`, `1`).
    OpenLevel = 1,
}

/// One row of the fixed-IV table: the IV applied to a rental opponent's
/// Pokémon, by whether the battle is the last of the challenge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FactoryIvRow {
    regular_iv: u8,
    last_battle_iv: u8,
}

impl FactoryIvRow {
    /// Returns the IV used outside the last battle of a challenge (upstream's
    /// column `0`).
    #[must_use]
    pub const fn regular_iv(self) -> u8 {
        self.regular_iv
    }

    /// Returns the IV used in the last battle of a challenge (upstream's
    /// column `1`).
    #[must_use]
    pub const fn last_battle_iv(self) -> u8 {
        self.last_battle_iv
    }
}

/// One initial rental range: the inclusive span of Frontier-mon indices a
/// rental draw may pick from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FactoryRentalRange {
    first_mon: u16,
    last_mon: u16,
}

impl FactoryRentalRange {
    /// Returns the first Frontier-mon index of the range, inclusive.
    #[must_use]
    pub const fn first_mon(self) -> u16 {
        self.first_mon
    }

    /// Returns the last Frontier-mon index of the range, inclusive.
    #[must_use]
    pub const fn last_mon(self) -> u16 {
        self.last_mon
    }
}

/// `sFixedIVTable`, by challenge row.
const FIXED_IVS: [FactoryIvRow; FIXED_IV_ROWS] = [
    iv_row(3, 6),
    iv_row(6, 9),
    iv_row(9, 12),
    iv_row(12, 15),
    iv_row(15, 18),
    iv_row(21, 31),
    iv_row(31, 31),
    iv_row(31, 31),
];

/// The Level 50 half of `sInitialRentalMonRanges`.
const LEVEL_50_RANGES: [FactoryRentalRange; RENTAL_RANGE_ROWS] = [
    range(110, 199),
    range(162, 266),
    range(267, 371),
    range(372, 467),
    range(468, 563),
    range(564, 659),
    range(660, 755),
    range(372, 849),
];

/// The Open Level half of `sInitialRentalMonRanges`.
const OPEN_LEVEL_RANGES: [FactoryRentalRange; RENTAL_RANGE_ROWS] = [
    range(372, 467),
    range(468, 563),
    range(564, 659),
    range(660, 755),
    range(372, 881),
    range(372, 881),
    range(372, 881),
    range(372, 881),
];

const fn iv_row(regular_iv: u8, last_battle_iv: u8) -> FactoryIvRow {
    FactoryIvRow {
        regular_iv,
        last_battle_iv,
    }
}

const fn range(first_mon: u16, last_mon: u16) -> FactoryRentalRange {
    FactoryRentalRange {
        first_mon,
        last_mon,
    }
}

/// Returns a row of the fixed-IV table.
///
/// # Errors
///
/// Returns [`FactoryRentalsError::InvalidFixedIvRow`] when `row` is not below
/// [`FIXED_IV_ROWS`].
pub fn fixed_iv_row(row: usize) -> Result<FactoryIvRow, FactoryRentalsError> {
    FIXED_IVS
        .get(row)
        .copied()
        .ok_or(FactoryRentalsError::InvalidFixedIvRow(row))
}

/// Returns a row of the initial rental ranges of one level mode.
///
/// # Errors
///
/// Returns [`FactoryRentalsError::InvalidRentalRangeRow`] when `row` is not
/// below [`RENTAL_RANGE_ROWS`].
pub fn initial_rental_range(
    mode: FactoryLevelMode,
    row: usize,
) -> Result<FactoryRentalRange, FactoryRentalsError> {
    let ranges = match mode {
        FactoryLevelMode::Level50 => &LEVEL_50_RANGES,
        FactoryLevelMode::OpenLevel => &OPEN_LEVEL_RANGES,
    };
    ranges
        .get(row)
        .copied()
        .ok_or(FactoryRentalsError::InvalidRentalRangeRow(row))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two columns of `sFixedIVTable`, one array per column.
    const REGULAR: [u8; 8] = [3, 6, 9, 12, 15, 21, 31, 31];
    const LAST_BATTLE: [u8; 8] = [6, 9, 12, 15, 18, 31, 31, 31];

    /// The inclusive endpoints of each Level 50 rental row, per the
    /// `// first - last` comments of the upstream table.
    const LEVEL_50_FIRST: [u16; 8] = [110, 162, 267, 372, 468, 564, 660, 372];
    const LEVEL_50_LAST: [u16; 8] = [199, 266, 371, 467, 563, 659, 755, 849];
    /// The same for the Open Level rows.
    const OPEN_FIRST: [u16; 8] = [372, 468, 564, 660, 372, 372, 372, 372];
    const OPEN_LAST: [u16; 8] = [467, 563, 659, 755, 881, 881, 881, 881];

    #[test]
    fn every_fixed_iv_row_is_pinned_field_by_field() {
        assert_eq!(FIXED_IV_ROWS, 8);
        for row in 0..8 {
            let iv = fixed_iv_row(row).expect("row is in range");
            assert_eq!(iv.regular_iv(), REGULAR[row], "regular {row}");
            assert_eq!(iv.last_battle_iv(), LAST_BATTLE[row], "last battle {row}");
        }
    }

    #[test]
    fn the_two_iv_columns_are_not_interchangeable() {
        // Every row but the last two differs by column; the last two are 31
        // in both.
        let differing: Vec<usize> = (0..8)
            .filter(|&row| {
                let iv = fixed_iv_row(row).expect("row is in range");
                iv.regular_iv() != iv.last_battle_iv()
            })
            .collect();
        assert_eq!(differing, [0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn every_rental_range_row_is_pinned_field_by_field() {
        assert_eq!(RENTAL_RANGE_ROWS, 8);
        for row in 0..8 {
            let level_50 =
                initial_rental_range(FactoryLevelMode::Level50, row).expect("row is in range");
            assert_eq!(level_50.first_mon(), LEVEL_50_FIRST[row], "L50 first {row}");
            assert_eq!(level_50.last_mon(), LEVEL_50_LAST[row], "L50 last {row}");
            let open =
                initial_rental_range(FactoryLevelMode::OpenLevel, row).expect("row is in range");
            assert_eq!(open.first_mon(), OPEN_FIRST[row], "open first {row}");
            assert_eq!(open.last_mon(), OPEN_LAST[row], "open last {row}");
        }
    }

    #[test]
    fn the_final_rows_of_the_two_level_modes_differ() {
        let level_50 = initial_rental_range(FactoryLevelMode::Level50, 7).expect("in range");
        let open = initial_rental_range(FactoryLevelMode::OpenLevel, 7).expect("in range");
        assert_eq!((level_50.first_mon(), level_50.last_mon()), (372, 849));
        assert_eq!((open.first_mon(), open.last_mon()), (372, 881));
    }

    #[test]
    fn a_fixed_iv_row_outside_the_table_is_refused() {
        for row in [8, 9, 255, usize::MAX] {
            assert_eq!(
                fixed_iv_row(row),
                Err(FactoryRentalsError::InvalidFixedIvRow(row))
            );
        }
    }

    #[test]
    fn a_rental_range_row_outside_the_table_is_refused() {
        for mode in [FactoryLevelMode::Level50, FactoryLevelMode::OpenLevel] {
            for row in [8, 9, 255, usize::MAX] {
                assert_eq!(
                    initial_rental_range(mode, row),
                    Err(FactoryRentalsError::InvalidRentalRangeRow(row))
                );
            }
        }
    }

    #[test]
    fn the_errors_name_the_offending_row() {
        assert_eq!(
            FactoryRentalsError::InvalidFixedIvRow(8).to_string(),
            "Battle Factory fixed-IV row `8` is out of range (there are 8)"
        );
        assert_eq!(
            FactoryRentalsError::InvalidRentalRangeRow(9).to_string(),
            "Battle Factory rental range row `9` is out of range (there are 8 per level mode)"
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

    fn mon_constants() -> std::collections::HashMap<String, u16> {
        upstream_file("include/constants/battle_frontier_mons.h")
            .lines()
            .filter_map(|line| {
                let mut words = line.split_whitespace();
                (words.next()? == "#define").then_some(())?;
                let name = words.next()?;
                Some((name.to_owned(), words.next()?.parse().ok()?))
            })
            .collect()
    }

    fn endpoint(text: &str, constants: &std::collections::HashMap<String, u16>) -> u16 {
        let text = text.trim();
        let (name, subtract) = match text.split_once('-') {
            Some((name, amount)) => (name.trim(), amount.trim().parse().expect("a number")),
            None => (text, 0),
        };
        constants
            .get(name)
            .unwrap_or_else(|| panic!("`{name}` is not a header constant"))
            - subtract
    }

    fn table_rows(source: &str, declaration: &str) -> Vec<(String, String)> {
        let start = source
            .find(declaration)
            .unwrap_or_else(|| panic!("`{declaration}` not found"));
        let body = &source[start..];
        let open = body.find("=\n{").expect("initializer opens") + 3;
        let close = body.find("\n};").expect("initializer closes");
        body[open..close]
            .lines()
            .map(|line| line.split("//").next().expect("a split has a head").trim())
            .filter(|line| !line.is_empty())
            .map(|line| {
                let inner = line
                    .trim_end_matches(',')
                    .strip_prefix('{')
                    .and_then(|rest| rest.strip_suffix('}'))
                    .unwrap_or_else(|| panic!("braced row expected: `{line}`"));
                let (first, last) = inner.split_once(',').expect("two members");
                (first.trim().to_owned(), last.trim().to_owned())
            })
            .collect()
    }

    #[test]
    #[ignore = "reads the pinned upstream checkout; run ./init.sh first"]
    fn the_upstream_tables_match() {
        let source = upstream_file("src/battle_factory.c");
        let ivs = table_rows(&source, "sFixedIVTable[][2]");
        assert_eq!(ivs.len(), FIXED_IV_ROWS);
        for (row, (regular, last)) in ivs.iter().enumerate() {
            let iv = fixed_iv_row(row).expect("in range");
            assert_eq!(iv.regular_iv().to_string(), *regular, "regular {row}");
            assert_eq!(iv.last_battle_iv().to_string(), *last, "last {row}");
        }

        let constants = mon_constants();
        let ranges = table_rows(&source, "sInitialRentalMonRanges[][2]");
        assert_eq!(ranges.len(), 2 * RENTAL_RANGE_ROWS);
        for (index, (first, last)) in ranges.iter().enumerate() {
            let (mode, row) = if index < RENTAL_RANGE_ROWS {
                (FactoryLevelMode::Level50, index)
            } else {
                (FactoryLevelMode::OpenLevel, index - RENTAL_RANGE_ROWS)
            };
            let range = initial_rental_range(mode, row).expect("in range");
            assert_eq!(range.first_mon(), endpoint(first, &constants), "{index}");
            assert_eq!(range.last_mon(), endpoint(last, &constants), "{index}");
        }
    }
}
