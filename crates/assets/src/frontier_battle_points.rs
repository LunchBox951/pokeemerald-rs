//! The Battle Frontier Battle Point awards: for each challenge row, facility
//! and battle mode, the Battle Points a cleared challenge pays.
//!
//! Source: `sBattlePointAwards` in `src/frontier_util.c` (`:500-592`), read at
//! `:1888-1901` and indexed by the `FRONTIER_FACILITY_*` and `FRONTIER_MODE_*`
//! values in `include/constants/battle_frontier.h`.
//!
//! The table is rectangular. Upstream's initializer lists only the modes a
//! facility offers, and C zero-fills the rest; those zeros, and the one
//! explicit `0` in the Battle Arena's rows 10 and 11, are stored cells like any
//! other. A zero is never read as "this combination is invalid".
//!
//! Which row a win streak selects, the Brain bonus, and adding awards to the
//! saved total are not ported here.

use std::error::Error;
use std::fmt;

use crate::frontier_brain_thresholds::FRONTIER_FACILITY_COUNT;

/// The number of challenge rows in the table.
pub const BATTLE_POINT_AWARD_ROWS: usize = 30;

/// The number of battle modes (`FRONTIER_MODE_COUNT`).
pub const FRONTIER_MODE_COUNT: usize = 4;

/// A failed conversion or lookup in the Battle Point awards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BattlePointAwardError {
    /// The challenge row is not below [`BATTLE_POINT_AWARD_ROWS`].
    InvalidRow(usize),
    /// The id is not below [`FRONTIER_FACILITY_COUNT`].
    InvalidFacility(usize),
    /// The id is not below [`FRONTIER_MODE_COUNT`].
    InvalidMode(usize),
}

impl fmt::Display for BattlePointAwardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRow(row) => write!(
                f,
                "Battle Point award row `{row}` is out of range (there are \
                 {BATTLE_POINT_AWARD_ROWS})"
            ),
            Self::InvalidFacility(facility) => write!(
                f,
                "Battle Frontier facility `{facility}` is out of range (there are \
                 {FRONTIER_FACILITY_COUNT})"
            ),
            Self::InvalidMode(mode) => write!(
                f,
                "Battle Frontier mode `{mode}` is out of range (there are \
                 {FRONTIER_MODE_COUNT})"
            ),
        }
    }
}

impl Error for BattlePointAwardError {}

/// A Battle Frontier facility, valued as `FRONTIER_FACILITY_*`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FrontierFacility {
    Tower = 0,
    Dome = 1,
    Palace = 2,
    Arena = 3,
    Factory = 4,
    Pike = 5,
    Pyramid = 6,
}

impl TryFrom<usize> for FrontierFacility {
    type Error = BattlePointAwardError;

    fn try_from(id: usize) -> Result<Self, Self::Error> {
        Ok(match id {
            0 => Self::Tower,
            1 => Self::Dome,
            2 => Self::Palace,
            3 => Self::Arena,
            4 => Self::Factory,
            5 => Self::Pike,
            6 => Self::Pyramid,
            _ => return Err(BattlePointAwardError::InvalidFacility(id)),
        })
    }
}

/// A Battle Frontier battle mode, valued as `FRONTIER_MODE_*`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FrontierMode {
    Singles = 0,
    Doubles = 1,
    Multis = 2,
    LinkMultis = 3,
}

impl TryFrom<usize> for FrontierMode {
    type Error = BattlePointAwardError;

    fn try_from(id: usize) -> Result<Self, Self::Error> {
        Ok(match id {
            0 => Self::Singles,
            1 => Self::Doubles,
            2 => Self::Multis,
            3 => Self::LinkMultis,
            _ => return Err(BattlePointAwardError::InvalidMode(id)),
        })
    }
}

/// The awards, indexed `[row][facility][mode]`.
const AWARDS: [[[u8; FRONTIER_MODE_COUNT]; FRONTIER_FACILITY_COUNT]; BATTLE_POINT_AWARD_ROWS] = [
    [
        [1, 2, 3, 3],
        [1, 1, 0, 0],
        [4, 5, 0, 0],
        [1, 0, 0, 0],
        [3, 4, 0, 0],
        [1, 0, 0, 0],
        [5, 0, 0, 0],
    ],
    [
        [2, 3, 4, 4],
        [1, 1, 0, 0],
        [4, 5, 0, 0],
        [1, 0, 0, 0],
        [3, 4, 0, 0],
        [1, 0, 0, 0],
        [5, 0, 0, 0],
    ],
    [
        [3, 4, 5, 5],
        [2, 2, 0, 0],
        [5, 6, 0, 0],
        [1, 0, 0, 0],
        [4, 5, 0, 0],
        [2, 0, 0, 0],
        [6, 0, 0, 0],
    ],
    [
        [4, 5, 6, 6],
        [2, 2, 0, 0],
        [5, 6, 0, 0],
        [2, 0, 0, 0],
        [4, 5, 0, 0],
        [2, 0, 0, 0],
        [6, 0, 0, 0],
    ],
    [
        [5, 6, 7, 7],
        [3, 3, 0, 0],
        [6, 7, 0, 0],
        [2, 0, 0, 0],
        [5, 6, 0, 0],
        [2, 0, 0, 0],
        [7, 0, 0, 0],
    ],
    [
        [6, 7, 8, 8],
        [3, 3, 0, 0],
        [6, 7, 0, 0],
        [2, 0, 0, 0],
        [5, 6, 0, 0],
        [4, 0, 0, 0],
        [7, 0, 0, 0],
    ],
    [
        [7, 8, 9, 9],
        [4, 4, 0, 0],
        [7, 8, 0, 0],
        [3, 0, 0, 0],
        [6, 7, 0, 0],
        [4, 0, 0, 0],
        [8, 0, 0, 0],
    ],
    [
        [8, 9, 10, 10],
        [4, 4, 0, 0],
        [7, 8, 0, 0],
        [3, 0, 0, 0],
        [6, 7, 0, 0],
        [4, 0, 0, 0],
        [8, 0, 0, 0],
    ],
    [
        [9, 10, 11, 11],
        [5, 5, 0, 0],
        [8, 9, 0, 0],
        [4, 0, 0, 0],
        [7, 8, 0, 0],
        [8, 0, 0, 0],
        [9, 0, 0, 0],
    ],
    [
        [10, 11, 12, 12],
        [5, 5, 0, 0],
        [8, 9, 0, 0],
        [4, 0, 0, 0],
        [7, 8, 0, 0],
        [8, 0, 0, 0],
        [9, 0, 0, 0],
    ],
    [
        [11, 12, 13, 13],
        [6, 6, 0, 0],
        [9, 10, 0, 0],
        [5, 0, 0, 0],
        [8, 9, 0, 0],
        [8, 0, 0, 0],
        [10, 0, 0, 0],
    ],
    [
        [12, 13, 14, 14],
        [6, 6, 0, 0],
        [9, 10, 0, 0],
        [6, 0, 0, 0],
        [8, 9, 0, 0],
        [8, 0, 0, 0],
        [10, 0, 0, 0],
    ],
    [
        [13, 14, 15, 15],
        [7, 7, 0, 0],
        [10, 11, 0, 0],
        [7, 0, 0, 0],
        [9, 10, 0, 0],
        [10, 0, 0, 0],
        [11, 0, 0, 0],
    ],
    [
        [14, 15, 15, 15],
        [7, 7, 0, 0],
        [10, 11, 0, 0],
        [8, 0, 0, 0],
        [9, 10, 0, 0],
        [10, 0, 0, 0],
        [11, 0, 0, 0],
    ],
    [
        [15, 15, 15, 15],
        [8, 8, 0, 0],
        [11, 12, 0, 0],
        [9, 0, 0, 0],
        [10, 11, 0, 0],
        [10, 0, 0, 0],
        [12, 0, 0, 0],
    ],
    [
        [15, 15, 15, 15],
        [8, 8, 0, 0],
        [11, 12, 0, 0],
        [10, 0, 0, 0],
        [10, 11, 0, 0],
        [10, 0, 0, 0],
        [12, 0, 0, 0],
    ],
    [
        [15, 15, 15, 15],
        [9, 9, 0, 0],
        [12, 13, 0, 0],
        [11, 0, 0, 0],
        [11, 12, 0, 0],
        [12, 0, 0, 0],
        [13, 0, 0, 0],
    ],
    [
        [15, 15, 15, 15],
        [9, 9, 0, 0],
        [12, 13, 0, 0],
        [12, 0, 0, 0],
        [11, 12, 0, 0],
        [12, 0, 0, 0],
        [13, 0, 0, 0],
    ],
    [
        [15, 15, 15, 15],
        [10, 10, 0, 0],
        [13, 14, 0, 0],
        [13, 0, 0, 0],
        [12, 13, 0, 0],
        [12, 0, 0, 0],
        [14, 0, 0, 0],
    ],
    [
        [15, 15, 15, 15],
        [10, 10, 0, 0],
        [13, 14, 0, 0],
        [14, 0, 0, 0],
        [12, 13, 0, 0],
        [12, 0, 0, 0],
        [14, 0, 0, 0],
    ],
    [
        [15, 15, 15, 15],
        [11, 11, 0, 0],
        [14, 15, 0, 0],
        [15, 0, 0, 0],
        [13, 14, 0, 0],
        [12, 0, 0, 0],
        [15, 0, 0, 0],
    ],
    [
        [15, 15, 15, 15],
        [11, 11, 0, 0],
        [14, 15, 0, 0],
        [15, 0, 0, 0],
        [13, 14, 0, 0],
        [14, 0, 0, 0],
        [15, 0, 0, 0],
    ],
    [
        [15, 15, 15, 15],
        [12, 12, 0, 0],
        [15, 15, 0, 0],
        [15, 0, 0, 0],
        [14, 15, 0, 0],
        [14, 0, 0, 0],
        [15, 0, 0, 0],
    ],
    [
        [15, 15, 15, 15],
        [12, 12, 0, 0],
        [15, 15, 0, 0],
        [15, 0, 0, 0],
        [14, 15, 0, 0],
        [14, 0, 0, 0],
        [15, 0, 0, 0],
    ],
    [
        [15, 15, 15, 15],
        [13, 13, 0, 0],
        [15, 15, 0, 0],
        [15, 0, 0, 0],
        [15, 15, 0, 0],
        [14, 0, 0, 0],
        [15, 0, 0, 0],
    ],
    [
        [15, 15, 15, 15],
        [13, 13, 0, 0],
        [15, 15, 0, 0],
        [15, 0, 0, 0],
        [15, 15, 0, 0],
        [15, 0, 0, 0],
        [15, 0, 0, 0],
    ],
    [
        [15, 15, 15, 15],
        [14, 14, 0, 0],
        [15, 15, 0, 0],
        [15, 0, 0, 0],
        [15, 15, 0, 0],
        [15, 0, 0, 0],
        [15, 0, 0, 0],
    ],
    [
        [15, 15, 15, 15],
        [14, 14, 0, 0],
        [15, 15, 0, 0],
        [15, 0, 0, 0],
        [15, 15, 0, 0],
        [15, 0, 0, 0],
        [15, 0, 0, 0],
    ],
    [
        [15, 15, 15, 15],
        [15, 15, 0, 0],
        [15, 15, 0, 0],
        [15, 0, 0, 0],
        [15, 15, 0, 0],
        [15, 0, 0, 0],
        [15, 0, 0, 0],
    ],
    [
        [15, 15, 15, 15],
        [15, 15, 0, 0],
        [15, 15, 0, 0],
        [15, 0, 0, 0],
        [15, 15, 0, 0],
        [15, 0, 0, 0],
        [15, 0, 0, 0],
    ],
];

/// Returns the Battle Points awarded at a challenge row for a facility and
/// mode. The row is the table row itself, not a win streak.
///
/// # Errors
///
/// Returns [`BattlePointAwardError::InvalidRow`] when `row` is not below
/// [`BATTLE_POINT_AWARD_ROWS`].
pub fn battle_point_award(
    row: usize,
    facility: FrontierFacility,
    mode: FrontierMode,
) -> Result<u8, BattlePointAwardError> {
    AWARDS
        .get(row)
        .map(|facilities| facilities[facility as usize][mode as usize])
        .ok_or(BattlePointAwardError::InvalidRow(row))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The awards written column by column, independently of the row layout of
    /// `AWARDS`: one array of the 30 rows per facility and mode, in facility
    /// order and, within a facility, mode order. Cells upstream leaves to
    /// C's zero fill are written as zeros.
    const COLUMNS: [[u8; BATTLE_POINT_AWARD_ROWS]; 28] = [
        [
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15,
            15, 15, 15, 15, 15, 15,
        ],
        [
            2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15,
            15, 15, 15, 15, 15, 15,
        ],
        [
            3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15,
            15, 15, 15, 15, 15, 15, 15,
        ],
        [
            3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15,
            15, 15, 15, 15, 15, 15, 15,
        ],
        [
            1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13,
            14, 14, 15, 15,
        ],
        [
            1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13,
            14, 14, 15, 15,
        ],
        [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0,
        ],
        [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0,
        ],
        [
            4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13, 14, 14, 15, 15, 15,
            15, 15, 15, 15, 15,
        ],
        [
            5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13, 14, 14, 15, 15, 15, 15,
            15, 15, 15, 15, 15, 15,
        ],
        [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0,
        ],
        [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0,
        ],
        [
            1, 1, 1, 2, 2, 2, 3, 3, 4, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 15, 15, 15, 15,
            15, 15, 15, 15, 15,
        ],
        [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0,
        ],
        [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0,
        ],
        [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0,
        ],
        [
            3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13, 14, 14, 15,
            15, 15, 15, 15, 15,
        ],
        [
            4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13, 14, 14, 15, 15, 15,
            15, 15, 15, 15, 15,
        ],
        [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0,
        ],
        [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0,
        ],
        [
            1, 1, 2, 2, 2, 4, 4, 4, 8, 8, 8, 8, 10, 10, 10, 10, 12, 12, 12, 12, 12, 14, 14, 14, 14,
            15, 15, 15, 15, 15,
        ],
        [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0,
        ],
        [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0,
        ],
        [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0,
        ],
        [
            5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13, 14, 14, 15, 15, 15, 15,
            15, 15, 15, 15, 15, 15,
        ],
        [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0,
        ],
        [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0,
        ],
        [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0,
        ],
    ];

    #[test]
    fn every_cell_is_pinned() {
        assert_eq!(BATTLE_POINT_AWARD_ROWS, 30);
        assert_eq!(FRONTIER_MODE_COUNT, 4);
        for (column, expected) in COLUMNS.iter().enumerate() {
            let facility = FrontierFacility::try_from(column / 4).expect("facility in range");
            let mode = FrontierMode::try_from(column % 4).expect("mode in range");
            for (row, &want) in expected.iter().enumerate() {
                assert_eq!(
                    battle_point_award(row, facility, mode),
                    Ok(want),
                    "row {row} column {column}"
                );
            }
        }
    }

    #[test]
    fn a_zero_cell_is_a_value_not_an_error() {
        // Singles-only Pike, and the Arena's explicit zero in rows 10 and 11.
        let pike_doubles = battle_point_award(0, FrontierFacility::Pike, FrontierMode::Doubles);
        assert_eq!(pike_doubles, Ok(0));
        let arena = |row| battle_point_award(row, FrontierFacility::Arena, FrontierMode::Doubles);
        assert_eq!((arena(10), arena(11), arena(12)), (Ok(0), Ok(0), Ok(0)));
    }

    #[test]
    fn the_final_rows_are_saturated() {
        for row in [28, 29] {
            for facility in 0..7 {
                let facility = FrontierFacility::try_from(facility).expect("in range");
                assert_eq!(
                    battle_point_award(row, facility, FrontierMode::Singles),
                    Ok(15)
                );
            }
        }
    }

    #[test]
    fn a_row_outside_the_table_is_refused() {
        for row in [30, 31, 255, usize::MAX] {
            assert_eq!(
                battle_point_award(row, FrontierFacility::Tower, FrontierMode::Singles),
                Err(BattlePointAwardError::InvalidRow(row))
            );
        }
    }

    #[test]
    fn an_id_outside_its_range_is_refused() {
        for id in [7, 8, usize::MAX] {
            assert_eq!(
                FrontierFacility::try_from(id),
                Err(BattlePointAwardError::InvalidFacility(id))
            );
        }
        for id in [4, 5, usize::MAX] {
            assert_eq!(
                FrontierMode::try_from(id),
                Err(BattlePointAwardError::InvalidMode(id))
            );
        }
    }

    #[test]
    fn ids_convert_to_the_matching_variant() {
        assert_eq!(FrontierFacility::try_from(0), Ok(FrontierFacility::Tower));
        assert_eq!(FrontierFacility::try_from(6), Ok(FrontierFacility::Pyramid));
        assert_eq!(FrontierMode::try_from(0), Ok(FrontierMode::Singles));
        assert_eq!(FrontierMode::try_from(3), Ok(FrontierMode::LinkMultis));
    }

    #[test]
    fn the_errors_name_the_offending_value() {
        assert_eq!(
            BattlePointAwardError::InvalidRow(30).to_string(),
            "Battle Point award row `30` is out of range (there are 30)"
        );
        assert_eq!(
            BattlePointAwardError::InvalidFacility(9).to_string(),
            "Battle Frontier facility `9` is out of range (there are 7)"
        );
        assert_eq!(
            BattlePointAwardError::InvalidMode(5).to_string(),
            "Battle Frontier mode `5` is out of range (there are 4)"
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

    /// Splits `text` into its top-level `{...}` groups, returning each group's
    /// inner text.
    fn brace_groups(text: &str) -> Vec<&str> {
        let mut groups = Vec::new();
        let (mut depth, mut start) = (0_usize, 0);
        for (index, ch) in text.char_indices() {
            match ch {
                '{' => {
                    if depth == 0 {
                        start = index + 1;
                    }
                    depth += 1;
                }
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        groups.push(&text[start..index]);
                    }
                }
                _ => {}
            }
        }
        groups
    }

    #[test]
    #[ignore = "reads the pinned upstream checkout; run ./init.sh first"]
    fn the_table_matches_upstream() {
        let source = upstream_file("src/frontier_util.c");
        let declaration = "sBattlePointAwards[][NUM_FRONTIER_FACILITIES][FRONTIER_MODE_COUNT]";
        let start = source.find(declaration).expect("declaration found");
        let body = &source[start..];
        let open = body.find("=\n{").expect("initializer opens") + 2;
        let close = body.find("\n};").expect("initializer closes");
        let outer = brace_groups(&body[open..close + 2]);
        let rows = brace_groups(outer[0]);
        assert_eq!(rows.len(), BATTLE_POINT_AWARD_ROWS);
        for (row, text) in rows.into_iter().enumerate() {
            let cells = brace_groups(text);
            assert_eq!(cells.len(), FRONTIER_FACILITY_COUNT, "row {row}");
            for (facility, cell) in cells.into_iter().enumerate() {
                // C zero-fills the modes the initializer omits.
                let mut listed: Vec<u8> = cell
                    .split(',')
                    .map(|value| value.trim().parse().expect("a byte"))
                    .collect();
                assert!(
                    listed.len() <= FRONTIER_MODE_COUNT,
                    "row {row} cell {facility}"
                );
                listed.resize(FRONTIER_MODE_COUNT, 0);
                assert_eq!(
                    listed, AWARDS[row][facility],
                    "row {row} facility {facility}"
                );
            }
        }
    }
}
