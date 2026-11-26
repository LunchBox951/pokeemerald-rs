//! The inclusive Frontier trainer-index ranges opponents are drawn from, one
//! per challenge number, in a normal and a hard table.
//!
//! Source: `sFrontierTrainerIdRanges` and `sFrontierTrainerIdRangesHard`
//! (`src/battle_tower.c:855-877`), whose endpoints are the
//! `FRONTIER_TRAINER_*` values in `include/constants/battle_frontier_trainers.h`.
//! Both are read at `src/battle_tower.c:1113-1142`, where the last normal row
//! (`:1125`) serves every challenge past the eighth. Adjacent rows overlap, and
//! the last row of each table is the same `200..=299` range.
//!
//! A Frontier trainer index counts rows of the Battle Frontier trainer list; it
//! is not an [`assets::trainers::TrainerId`](crate::trainers::TrainerId).
//!
//! Data only: choosing a table or row from a streak or battle number, drawing
//! a trainer, and building the battle are not ported here. A row outside the
//! table is refused rather than clamped.

use std::error::Error;
use std::fmt;

/// The number of rows in each range table.
pub const FRONTIER_TRAINER_RANGE_ROWS: usize = 8;

/// A Battle Frontier trainer index (`FRONTIER_TRAINER_*`).
///
/// It is a different namespace from the ordinary trainer roster, so it does
/// not convert to or from [`TrainerId`](crate::trainers::TrainerId):
///
/// ```compile_fail
/// use assets::frontier_trainer_ranges::{trainer_range, FrontierTrainerTable};
/// use assets::trainers::TrainerId;
///
/// fn ordinary(_: TrainerId) {}
/// let range = trainer_range(FrontierTrainerTable::Normal, 0).unwrap();
/// ordinary(range.first());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FrontierTrainerId(pub u16);

impl FrontierTrainerId {
    /// Returns the numeric Frontier trainer index.
    #[must_use]
    pub const fn index(self) -> u16 {
        self.0
    }
}

/// Which of the two range tables a lookup reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FrontierTrainerTable {
    /// `sFrontierTrainerIdRanges`.
    Normal,
    /// `sFrontierTrainerIdRangesHard`.
    Hard,
}

/// A failed lookup in the Frontier trainer ranges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontierTrainerRangeError {
    /// The row is not below [`FRONTIER_TRAINER_RANGE_ROWS`].
    InvalidRow(usize),
}

impl fmt::Display for FrontierTrainerRangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRow(row) => write!(
                f,
                "Frontier trainer range row `{row}` is out of range (there are \
                 {FRONTIER_TRAINER_RANGE_ROWS})"
            ),
        }
    }
}

impl Error for FrontierTrainerRangeError {}

/// An inclusive span of Frontier trainer indexes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrontierTrainerRange {
    first: FrontierTrainerId,
    last: FrontierTrainerId,
}

impl FrontierTrainerRange {
    /// Returns the lowest index in the range.
    #[must_use]
    pub const fn first(self) -> FrontierTrainerId {
        self.first
    }

    /// Returns the highest index in the range, which belongs to it.
    #[must_use]
    pub const fn last(self) -> FrontierTrainerId {
        self.last
    }

    /// Returns whether `id` lies between [`Self::first`] and [`Self::last`],
    /// both included.
    #[must_use]
    pub const fn contains(self, id: FrontierTrainerId) -> bool {
        self.first.0 <= id.0 && id.0 <= self.last.0
    }
}

const fn range(first: u16, last: u16) -> FrontierTrainerRange {
    FrontierTrainerRange {
        first: FrontierTrainerId(first),
        last: FrontierTrainerId(last),
    }
}

/// `sFrontierTrainerIdRanges`, by challenge row.
const NORMAL: [FrontierTrainerRange; FRONTIER_TRAINER_RANGE_ROWS] = [
    range(0, 99),
    range(80, 119),
    range(100, 139),
    range(120, 159),
    range(140, 179),
    range(160, 199),
    range(180, 219),
    range(200, 299),
];

/// `sFrontierTrainerIdRangesHard`, by challenge row.
const HARD: [FrontierTrainerRange; FRONTIER_TRAINER_RANGE_ROWS] = [
    range(100, 119),
    range(120, 139),
    range(140, 159),
    range(160, 179),
    range(180, 199),
    range(200, 219),
    range(220, 239),
    range(200, 299),
];

/// Returns the range of one row of the normal or hard table.
///
/// # Errors
///
/// Returns [`FrontierTrainerRangeError::InvalidRow`] when `row` is not below
/// [`FRONTIER_TRAINER_RANGE_ROWS`].
pub fn trainer_range(
    table: FrontierTrainerTable,
    row: usize,
) -> Result<FrontierTrainerRange, FrontierTrainerRangeError> {
    let rows = match table {
        FrontierTrainerTable::Normal => &NORMAL,
        FrontierTrainerTable::Hard => &HARD,
    };
    rows.get(row)
        .copied()
        .ok_or(FrontierTrainerRangeError::InvalidRow(row))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sixteen `(first, last)` endpoint pairs, written out by hand.
    const NORMAL_ENDPOINTS: [(u16, u16); 8] = [
        (0, 99),
        (80, 119),
        (100, 139),
        (120, 159),
        (140, 179),
        (160, 199),
        (180, 219),
        (200, 299),
    ];
    const HARD_ENDPOINTS: [(u16, u16); 8] = [
        (100, 119),
        (120, 139),
        (140, 159),
        (160, 179),
        (180, 199),
        (200, 219),
        (220, 239),
        (200, 299),
    ];

    fn endpoints(table: FrontierTrainerTable) -> Vec<(u16, u16)> {
        (0..FRONTIER_TRAINER_RANGE_ROWS)
            .map(|row| {
                let range = trainer_range(table, row).expect("row is in range");
                (range.first().index(), range.last().index())
            })
            .collect()
    }

    #[test]
    fn all_sixteen_ranges_are_pinned_in_order() {
        assert_eq!(FRONTIER_TRAINER_RANGE_ROWS, 8);
        assert_eq!(endpoints(FrontierTrainerTable::Normal), NORMAL_ENDPOINTS);
        assert_eq!(endpoints(FrontierTrainerTable::Hard), HARD_ENDPOINTS);
    }

    #[test]
    fn adjacent_normal_rows_overlap() {
        let overlaps: Vec<(u16, u16)> = NORMAL_ENDPOINTS
            .windows(2)
            .map(|pair| (pair[1].0, pair[0].1))
            .collect();
        // Each row starts at or below the previous row's last index.
        assert!(overlaps
            .iter()
            .all(|&(next_first, last)| next_first <= last));
        let first = trainer_range(FrontierTrainerTable::Normal, 0).unwrap();
        let second = trainer_range(FrontierTrainerTable::Normal, 1).unwrap();
        assert!(first.contains(FrontierTrainerId(80)));
        assert!(second.contains(FrontierTrainerId(99)));
    }

    #[test]
    fn hard_rows_touch_without_overlapping_except_the_last() {
        for row in 0..6 {
            let this = trainer_range(FrontierTrainerTable::Hard, row).unwrap();
            let next = trainer_range(FrontierTrainerTable::Hard, row + 1).unwrap();
            assert_eq!(this.last().index() + 1, next.first().index(), "row {row}");
        }
    }

    #[test]
    fn the_last_range_is_shared_by_both_tables() {
        let normal = trainer_range(FrontierTrainerTable::Normal, 7).unwrap();
        let hard = trainer_range(FrontierTrainerTable::Hard, 7).unwrap();
        assert_eq!(normal, hard);
        assert_eq!((normal.first().index(), normal.last().index()), (200, 299));
    }

    #[test]
    fn both_bounds_are_inclusive() {
        for table in [FrontierTrainerTable::Normal, FrontierTrainerTable::Hard] {
            for row in 0..FRONTIER_TRAINER_RANGE_ROWS {
                let range = trainer_range(table, row).unwrap();
                let (first, last) = (range.first().index(), range.last().index());
                assert!(range.contains(FrontierTrainerId(first)));
                assert!(range.contains(FrontierTrainerId(last)));
                assert!(!range.contains(FrontierTrainerId(last + 1)));
                if first > 0 {
                    assert!(!range.contains(FrontierTrainerId(first - 1)));
                }
            }
        }
    }

    #[test]
    fn a_row_outside_the_table_is_refused() {
        for table in [FrontierTrainerTable::Normal, FrontierTrainerTable::Hard] {
            for row in [8, 9, 255, usize::MAX] {
                assert_eq!(
                    trainer_range(table, row),
                    Err(FrontierTrainerRangeError::InvalidRow(row))
                );
            }
        }
    }

    #[test]
    fn the_error_names_the_offending_row() {
        assert_eq!(
            FrontierTrainerRangeError::InvalidRow(9).to_string(),
            "Frontier trainer range row `9` is out of range (there are 8)"
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

    /// The `FRONTIER_TRAINER_*` values, by name, from the upstream header.
    fn trainer_ids() -> std::collections::HashMap<String, u16> {
        upstream_file("include/constants/battle_frontier_trainers.h")
            .lines()
            .filter_map(|line| {
                let mut words = line.split_whitespace();
                (words.next()? == "#define").then_some(())?;
                let name = words.next()?;
                name.starts_with("FRONTIER_TRAINER_").then_some(())?;
                Some((name.to_owned(), words.next()?.parse().ok()?))
            })
            .collect()
    }

    /// Parses one table out of `src/battle_tower.c` as `(first, last)` pairs.
    fn parse_table(
        source: &str,
        declaration: &str,
        ids: &std::collections::HashMap<String, u16>,
    ) -> Vec<(u16, u16)> {
        let start = source
            .find(declaration)
            .unwrap_or_else(|| panic!("`{declaration}` not found"));
        let body = &source[start..];
        let open = body.find("=\n{").expect("initializer opens") + 3;
        let close = body.find("\n};").expect("initializer closes");
        body[open..close]
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                let (names, _) = line.split_once('}').expect("braced row");
                let (_, names) = names.split_once('{').expect("braced row");
                let (first, last) = names.split_once(',').expect("two endpoints");
                (ids[first.trim()], ids[last.trim()])
            })
            .collect()
    }

    #[test]
    #[ignore = "reads the pinned upstream checkout; run ./init.sh first"]
    fn upstream_tables_match() {
        let ids = trainer_ids();
        let source = upstream_file("src/battle_tower.c");
        for (table, declaration) in [
            (
                FrontierTrainerTable::Normal,
                "sFrontierTrainerIdRanges[][2]",
            ),
            (
                FrontierTrainerTable::Hard,
                "sFrontierTrainerIdRangesHard[][2]",
            ),
        ] {
            assert_eq!(
                parse_table(&source, declaration, &ids),
                endpoints(table),
                "{declaration}"
            );
        }
    }
}
