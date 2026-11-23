//! The Battle Pyramid item distributions: the floor-item matrices and rows
//! that place items on a floor, and the thresholds the Pickup ability reads.
//!
//! Source: `src/battle_pyramid.c`. Three separate tables live there, and this
//! module keeps them as three separate things:
//!
//! 1. **The item matrices** `sPickupItemsLvl50` and `sPickupItemsLvlOpen`
//!    (`:289-335`): for each level mode, 20 rounds of 10 items. A round's
//!    item list is indexed by a column from `0` to `9`, and both the floor
//!    placement and the Pickup ability read from it. See
//!    [`pyramid_round_items`] and [`pyramid_item`].
//! 2. **The floor rows** `sPickupItemSlots`, with their per-floor start
//!    `sPickupItemOffsets` (`:337-411`). Seven floors of nine
//!    `{threshold, column}` rows each, stored back to back. Floor `f` owns
//!    the nine rows from its offset. A floor item draws a percentile and takes
//!    the first row whose threshold is greater than it
//!    (`src/battle_pyramid.c:993-1002`), then reads that row's column from the
//!    round's items. See [`floor_item_rows`] and [`floor_item_row_offset`].
//! 3. **The Pickup thresholds** `sPickupPercentages` (`:832`): ten ascending
//!    values, the last `100`. The Pickup ability takes the first one that is
//!    greater than its percentile as the column to read
//!    (`GetBattlePyramidPickupItemId`, `src/battle_pyramid.c:1958-1983`). See
//!    [`pickup_thresholds`] and [`pickup_threshold`].
//!
//! Rounds, floors, rows, and columns are all indexed from `0`, in the order
//! the upstream tables give. Every lookup refuses an index that is out of
//! range instead of clamping it. The upstream round clamp for long win
//! streaks, the random draws, the floor and round selection from the save
//! file, and Pickup eligibility are not ported here.
//!
//! This is separate from the overworld Pickup distribution in
//! [`pickup`](crate::pickup); the two share nothing.

use std::error::Error;
use std::fmt;

use crate::items::ItemId;

/// The number of rounds in each level mode (`TOTAL_PYRAMID_ROUNDS`).
pub const PYRAMID_ROUNDS: usize = 20;

/// The number of items, and so of columns, in each round
/// (`PICKUP_ITEMS_PER_ROUND`).
pub const PICKUP_ITEMS_PER_ROUND: usize = 10;

/// The number of Battle Pyramid floors with their own floor rows
/// (`FRONTIER_STAGES_PER_CHALLENGE`).
pub const PYRAMID_FLOORS: usize = 7;

/// The number of rows each floor owns in the floor-row list.
pub const FLOOR_ITEM_ROWS_PER_FLOOR: usize = 9;

/// The number of rows in the floor-row list, over all floors.
pub const FLOOR_ITEM_ROW_COUNT: usize = PYRAMID_FLOORS * FLOOR_ITEM_ROWS_PER_FLOOR;

/// A failed lookup in the Battle Pyramid item tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PyramidItemError {
    /// The round is not below [`PYRAMID_ROUNDS`].
    InvalidRound(usize),
    /// The column is not below [`PICKUP_ITEMS_PER_ROUND`].
    InvalidColumn(usize),
    /// The floor is not below [`PYRAMID_FLOORS`].
    InvalidFloor(usize),
    /// The Pickup threshold index is not below [`PICKUP_ITEMS_PER_ROUND`].
    InvalidThresholdIndex(usize),
}

impl fmt::Display for PyramidItemError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRound(round) => write!(
                f,
                "Battle Pyramid round `{round}` is out of range (there are {PYRAMID_ROUNDS})"
            ),
            Self::InvalidColumn(column) => write!(
                f,
                "Battle Pyramid item column `{column}` is out of range (there are \
                 {PICKUP_ITEMS_PER_ROUND})"
            ),
            Self::InvalidFloor(floor) => write!(
                f,
                "Battle Pyramid floor `{floor}` is out of range (there are {PYRAMID_FLOORS})"
            ),
            Self::InvalidThresholdIndex(index) => write!(
                f,
                "Battle Pyramid Pickup threshold `{index}` is out of range (there are \
                 {PICKUP_ITEMS_PER_ROUND})"
            ),
        }
    }
}

impl Error for PyramidItemError {}

/// Which item matrix a lookup reads (`FRONTIER_LVL_50` or the Open Level
/// mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PyramidItemMode {
    /// `sPickupItemsLvl50`.
    Level50,
    /// `sPickupItemsLvlOpen`.
    OpenLevel,
}

/// One floor row: a percentile threshold and the column it selects.
///
/// A percentile `p` in `0..100` takes the first row of a floor with
/// `p < threshold`. The last row of every floor has threshold `100`, so a
/// row always matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FloorItemRow {
    threshold: u8,
    column: u8,
}

impl FloorItemRow {
    /// A row; fails to compile when the column is not below
    /// [`PICKUP_ITEMS_PER_ROUND`], so a bad table row cannot build.
    const fn new(threshold: u8, column: u8) -> Self {
        assert!(
            (column as usize) < PICKUP_ITEMS_PER_ROUND,
            "floor row column is outside the round's items"
        );
        Self { threshold, column }
    }

    /// Returns the exclusive upper bound of the percentiles this row takes.
    #[must_use]
    pub const fn threshold(self) -> u8 {
        self.threshold
    }

    /// Returns the column of the round's items this row selects.
    #[must_use]
    pub const fn column(self) -> usize {
        self.column as usize
    }
}

/// `sPickupItemsLvl50` (`src/battle_pyramid.c:289-311`).
#[rustfmt::skip]
const LEVEL_50_ITEMS: [[ItemId; PICKUP_ITEMS_PER_ROUND]; PYRAMID_ROUNDS] = [
    [ItemId::HYPER_POTION, ItemId::FLUFFY_TAIL, ItemId::CHERI_BERRY, ItemId::ETHER, ItemId::LUM_BERRY, ItemId::REVIVE, ItemId::BRIGHT_POWDER, ItemId::SHELL_BELL, ItemId::MAX_REVIVE, ItemId::SACRED_ASH],
    [ItemId::HYPER_POTION, ItemId::DIRE_HIT, ItemId::PECHA_BERRY, ItemId::ETHER, ItemId::LEPPA_BERRY, ItemId::REVIVE, ItemId::LEFTOVERS, ItemId::CHOICE_BAND, ItemId::FULL_RESTORE, ItemId::MAX_ELIXIR],
    [ItemId::HYPER_POTION, ItemId::X_ATTACK, ItemId::RAWST_BERRY, ItemId::ETHER, ItemId::LUM_BERRY, ItemId::REVIVE, ItemId::SCOPE_LENS, ItemId::FOCUS_BAND, ItemId::MAX_REVIVE, ItemId::SACRED_ASH],
    [ItemId::HYPER_POTION, ItemId::X_DEFEND, ItemId::LUM_BERRY, ItemId::ETHER, ItemId::LEPPA_BERRY, ItemId::REVIVE, ItemId::QUICK_CLAW, ItemId::KINGS_ROCK, ItemId::FULL_RESTORE, ItemId::MAX_ELIXIR],
    [ItemId::HYPER_POTION, ItemId::X_SPEED, ItemId::CHESTO_BERRY, ItemId::ETHER, ItemId::LUM_BERRY, ItemId::REVIVE, ItemId::BRIGHT_POWDER, ItemId::SHELL_BELL, ItemId::MAX_REVIVE, ItemId::SACRED_ASH],
    [ItemId::HYPER_POTION, ItemId::X_ACCURACY, ItemId::LUM_BERRY, ItemId::ETHER, ItemId::LEPPA_BERRY, ItemId::REVIVE, ItemId::LEFTOVERS, ItemId::CHOICE_BAND, ItemId::FULL_RESTORE, ItemId::MAX_ELIXIR],
    [ItemId::HYPER_POTION, ItemId::X_SPECIAL, ItemId::LUM_BERRY, ItemId::ETHER, ItemId::LUM_BERRY, ItemId::REVIVE, ItemId::SCOPE_LENS, ItemId::FOCUS_BAND, ItemId::MAX_REVIVE, ItemId::SACRED_ASH],
    [ItemId::HYPER_POTION, ItemId::GUARD_SPEC, ItemId::LUM_BERRY, ItemId::ETHER, ItemId::LEPPA_BERRY, ItemId::REVIVE, ItemId::QUICK_CLAW, ItemId::KINGS_ROCK, ItemId::FULL_RESTORE, ItemId::MAX_ELIXIR],
    [ItemId::HYPER_POTION, ItemId::FLUFFY_TAIL, ItemId::LUM_BERRY, ItemId::ETHER, ItemId::LUM_BERRY, ItemId::REVIVE, ItemId::BRIGHT_POWDER, ItemId::SHELL_BELL, ItemId::MAX_REVIVE, ItemId::SACRED_ASH],
    [ItemId::HYPER_POTION, ItemId::DIRE_HIT, ItemId::LUM_BERRY, ItemId::ETHER, ItemId::LEPPA_BERRY, ItemId::REVIVE, ItemId::LEFTOVERS, ItemId::CHOICE_BAND, ItemId::FULL_RESTORE, ItemId::MAX_ELIXIR],
    [ItemId::HYPER_POTION, ItemId::X_ATTACK, ItemId::LUM_BERRY, ItemId::ETHER, ItemId::LUM_BERRY, ItemId::REVIVE, ItemId::SCOPE_LENS, ItemId::FOCUS_BAND, ItemId::MAX_REVIVE, ItemId::SACRED_ASH],
    [ItemId::HYPER_POTION, ItemId::X_DEFEND, ItemId::LUM_BERRY, ItemId::ETHER, ItemId::LEPPA_BERRY, ItemId::REVIVE, ItemId::QUICK_CLAW, ItemId::KINGS_ROCK, ItemId::FULL_RESTORE, ItemId::MAX_ELIXIR],
    [ItemId::HYPER_POTION, ItemId::X_SPEED, ItemId::LUM_BERRY, ItemId::ETHER, ItemId::LUM_BERRY, ItemId::REVIVE, ItemId::BRIGHT_POWDER, ItemId::SHELL_BELL, ItemId::MAX_REVIVE, ItemId::SACRED_ASH],
    [ItemId::HYPER_POTION, ItemId::X_ACCURACY, ItemId::LUM_BERRY, ItemId::ETHER, ItemId::LEPPA_BERRY, ItemId::REVIVE, ItemId::LEFTOVERS, ItemId::CHOICE_BAND, ItemId::FULL_RESTORE, ItemId::MAX_ELIXIR],
    [ItemId::HYPER_POTION, ItemId::X_SPECIAL, ItemId::LUM_BERRY, ItemId::ETHER, ItemId::LUM_BERRY, ItemId::REVIVE, ItemId::SCOPE_LENS, ItemId::FOCUS_BAND, ItemId::MAX_REVIVE, ItemId::SACRED_ASH],
    [ItemId::HYPER_POTION, ItemId::GUARD_SPEC, ItemId::LUM_BERRY, ItemId::ETHER, ItemId::LEPPA_BERRY, ItemId::REVIVE, ItemId::QUICK_CLAW, ItemId::KINGS_ROCK, ItemId::FULL_RESTORE, ItemId::MAX_ELIXIR],
    [ItemId::HYPER_POTION, ItemId::FLUFFY_TAIL, ItemId::LUM_BERRY, ItemId::ETHER, ItemId::LUM_BERRY, ItemId::REVIVE, ItemId::BRIGHT_POWDER, ItemId::SHELL_BELL, ItemId::MAX_REVIVE, ItemId::SACRED_ASH],
    [ItemId::HYPER_POTION, ItemId::DIRE_HIT, ItemId::LUM_BERRY, ItemId::ETHER, ItemId::LEPPA_BERRY, ItemId::REVIVE, ItemId::LEFTOVERS, ItemId::CHOICE_BAND, ItemId::FULL_RESTORE, ItemId::MAX_ELIXIR],
    [ItemId::HYPER_POTION, ItemId::X_ATTACK, ItemId::LUM_BERRY, ItemId::ETHER, ItemId::LUM_BERRY, ItemId::REVIVE, ItemId::SCOPE_LENS, ItemId::FOCUS_BAND, ItemId::MAX_REVIVE, ItemId::SACRED_ASH],
    [ItemId::HYPER_POTION, ItemId::X_DEFEND, ItemId::LUM_BERRY, ItemId::ETHER, ItemId::LEPPA_BERRY, ItemId::REVIVE, ItemId::QUICK_CLAW, ItemId::KINGS_ROCK, ItemId::FULL_RESTORE, ItemId::MAX_ELIXIR],
];

/// `sPickupItemsLvlOpen` (`src/battle_pyramid.c:313-335`).
#[rustfmt::skip]
const OPEN_LEVEL_ITEMS: [[ItemId; PICKUP_ITEMS_PER_ROUND]; PYRAMID_ROUNDS] = [
    [ItemId::HYPER_POTION, ItemId::FLUFFY_TAIL, ItemId::CHERI_BERRY, ItemId::ETHER, ItemId::LUM_BERRY, ItemId::REVIVE, ItemId::BRIGHT_POWDER, ItemId::SHELL_BELL, ItemId::MAX_REVIVE, ItemId::SACRED_ASH],
    [ItemId::HYPER_POTION, ItemId::DIRE_HIT, ItemId::PECHA_BERRY, ItemId::ETHER, ItemId::LEPPA_BERRY, ItemId::REVIVE, ItemId::LEFTOVERS, ItemId::CHOICE_BAND, ItemId::FULL_RESTORE, ItemId::MAX_ELIXIR],
    [ItemId::HYPER_POTION, ItemId::X_ATTACK, ItemId::RAWST_BERRY, ItemId::ETHER, ItemId::LUM_BERRY, ItemId::REVIVE, ItemId::SCOPE_LENS, ItemId::FOCUS_BAND, ItemId::MAX_REVIVE, ItemId::SACRED_ASH],
    [ItemId::HYPER_POTION, ItemId::X_DEFEND, ItemId::LUM_BERRY, ItemId::ETHER, ItemId::LEPPA_BERRY, ItemId::REVIVE, ItemId::QUICK_CLAW, ItemId::KINGS_ROCK, ItemId::FULL_RESTORE, ItemId::MAX_ELIXIR],
    [ItemId::HYPER_POTION, ItemId::X_SPEED, ItemId::CHESTO_BERRY, ItemId::ETHER, ItemId::LUM_BERRY, ItemId::REVIVE, ItemId::BRIGHT_POWDER, ItemId::SHELL_BELL, ItemId::MAX_REVIVE, ItemId::SACRED_ASH],
    [ItemId::HYPER_POTION, ItemId::X_ACCURACY, ItemId::LUM_BERRY, ItemId::ETHER, ItemId::LEPPA_BERRY, ItemId::REVIVE, ItemId::LEFTOVERS, ItemId::CHOICE_BAND, ItemId::FULL_RESTORE, ItemId::MAX_ELIXIR],
    [ItemId::HYPER_POTION, ItemId::X_SPECIAL, ItemId::LUM_BERRY, ItemId::ETHER, ItemId::LUM_BERRY, ItemId::REVIVE, ItemId::SCOPE_LENS, ItemId::FOCUS_BAND, ItemId::MAX_REVIVE, ItemId::SACRED_ASH],
    [ItemId::HYPER_POTION, ItemId::GUARD_SPEC, ItemId::LUM_BERRY, ItemId::ETHER, ItemId::LEPPA_BERRY, ItemId::REVIVE, ItemId::QUICK_CLAW, ItemId::KINGS_ROCK, ItemId::FULL_RESTORE, ItemId::MAX_ELIXIR],
    [ItemId::HYPER_POTION, ItemId::FLUFFY_TAIL, ItemId::LUM_BERRY, ItemId::ETHER, ItemId::LUM_BERRY, ItemId::REVIVE, ItemId::BRIGHT_POWDER, ItemId::SHELL_BELL, ItemId::MAX_REVIVE, ItemId::SACRED_ASH],
    [ItemId::HYPER_POTION, ItemId::DIRE_HIT, ItemId::LUM_BERRY, ItemId::ETHER, ItemId::LEPPA_BERRY, ItemId::REVIVE, ItemId::LEFTOVERS, ItemId::CHOICE_BAND, ItemId::FULL_RESTORE, ItemId::MAX_ELIXIR],
    [ItemId::HYPER_POTION, ItemId::X_ATTACK, ItemId::LUM_BERRY, ItemId::ETHER, ItemId::LUM_BERRY, ItemId::REVIVE, ItemId::SCOPE_LENS, ItemId::FOCUS_BAND, ItemId::MAX_REVIVE, ItemId::SACRED_ASH],
    [ItemId::HYPER_POTION, ItemId::X_DEFEND, ItemId::LUM_BERRY, ItemId::ETHER, ItemId::LEPPA_BERRY, ItemId::REVIVE, ItemId::QUICK_CLAW, ItemId::KINGS_ROCK, ItemId::FULL_RESTORE, ItemId::MAX_ELIXIR],
    [ItemId::HYPER_POTION, ItemId::X_SPEED, ItemId::LUM_BERRY, ItemId::ETHER, ItemId::LUM_BERRY, ItemId::REVIVE, ItemId::BRIGHT_POWDER, ItemId::SHELL_BELL, ItemId::MAX_REVIVE, ItemId::SACRED_ASH],
    [ItemId::HYPER_POTION, ItemId::X_ACCURACY, ItemId::LUM_BERRY, ItemId::ETHER, ItemId::LEPPA_BERRY, ItemId::REVIVE, ItemId::LEFTOVERS, ItemId::CHOICE_BAND, ItemId::FULL_RESTORE, ItemId::MAX_ELIXIR],
    [ItemId::HYPER_POTION, ItemId::X_SPECIAL, ItemId::LUM_BERRY, ItemId::ETHER, ItemId::LUM_BERRY, ItemId::REVIVE, ItemId::SCOPE_LENS, ItemId::FOCUS_BAND, ItemId::MAX_REVIVE, ItemId::SACRED_ASH],
    [ItemId::HYPER_POTION, ItemId::GUARD_SPEC, ItemId::LUM_BERRY, ItemId::ETHER, ItemId::LEPPA_BERRY, ItemId::REVIVE, ItemId::QUICK_CLAW, ItemId::KINGS_ROCK, ItemId::FULL_RESTORE, ItemId::MAX_ELIXIR],
    [ItemId::HYPER_POTION, ItemId::FLUFFY_TAIL, ItemId::LUM_BERRY, ItemId::ETHER, ItemId::LUM_BERRY, ItemId::REVIVE, ItemId::BRIGHT_POWDER, ItemId::SHELL_BELL, ItemId::MAX_REVIVE, ItemId::SACRED_ASH],
    [ItemId::HYPER_POTION, ItemId::DIRE_HIT, ItemId::LUM_BERRY, ItemId::ETHER, ItemId::LEPPA_BERRY, ItemId::REVIVE, ItemId::LEFTOVERS, ItemId::CHOICE_BAND, ItemId::FULL_RESTORE, ItemId::MAX_ELIXIR],
    [ItemId::HYPER_POTION, ItemId::X_ATTACK, ItemId::LUM_BERRY, ItemId::ETHER, ItemId::LUM_BERRY, ItemId::REVIVE, ItemId::SCOPE_LENS, ItemId::FOCUS_BAND, ItemId::MAX_REVIVE, ItemId::SACRED_ASH],
    [ItemId::HYPER_POTION, ItemId::X_DEFEND, ItemId::LUM_BERRY, ItemId::ETHER, ItemId::LEPPA_BERRY, ItemId::REVIVE, ItemId::QUICK_CLAW, ItemId::KINGS_ROCK, ItemId::FULL_RESTORE, ItemId::MAX_ELIXIR],
];

/// `sPickupItemSlots` (`src/battle_pyramid.c:337-409`), floor after floor.
#[rustfmt::skip]
const FLOOR_ITEM_ROW_LIST: [FloorItemRow; FLOOR_ITEM_ROW_COUNT] = [
    FloorItemRow::new(31, 0),
    FloorItemRow::new(46, 1),
    FloorItemRow::new(61, 2),
    FloorItemRow::new(71, 3),
    FloorItemRow::new(81, 4),
    FloorItemRow::new(91, 5),
    FloorItemRow::new(94, 6),
    FloorItemRow::new(97, 7),
    FloorItemRow::new(100, 8),
    FloorItemRow::new(15, 0),
    FloorItemRow::new(46, 1),
    FloorItemRow::new(61, 2),
    FloorItemRow::new(71, 3),
    FloorItemRow::new(81, 4),
    FloorItemRow::new(91, 5),
    FloorItemRow::new(94, 6),
    FloorItemRow::new(97, 8),
    FloorItemRow::new(100, 9),
    FloorItemRow::new(15, 0),
    FloorItemRow::new(30, 1),
    FloorItemRow::new(61, 2),
    FloorItemRow::new(71, 3),
    FloorItemRow::new(81, 4),
    FloorItemRow::new(91, 5),
    FloorItemRow::new(94, 6),
    FloorItemRow::new(97, 7),
    FloorItemRow::new(100, 8),
    FloorItemRow::new(28, 0),
    FloorItemRow::new(43, 1),
    FloorItemRow::new(58, 2),
    FloorItemRow::new(68, 3),
    FloorItemRow::new(78, 4),
    FloorItemRow::new(88, 5),
    FloorItemRow::new(92, 7),
    FloorItemRow::new(96, 8),
    FloorItemRow::new(100, 9),
    FloorItemRow::new(15, 0),
    FloorItemRow::new(43, 1),
    FloorItemRow::new(58, 2),
    FloorItemRow::new(68, 3),
    FloorItemRow::new(78, 4),
    FloorItemRow::new(88, 5),
    FloorItemRow::new(92, 6),
    FloorItemRow::new(96, 7),
    FloorItemRow::new(100, 9),
    FloorItemRow::new(15, 0),
    FloorItemRow::new(30, 1),
    FloorItemRow::new(58, 2),
    FloorItemRow::new(68, 3),
    FloorItemRow::new(78, 4),
    FloorItemRow::new(88, 5),
    FloorItemRow::new(92, 6),
    FloorItemRow::new(96, 7),
    FloorItemRow::new(100, 8),
    FloorItemRow::new(28, 0),
    FloorItemRow::new(43, 1),
    FloorItemRow::new(58, 2),
    FloorItemRow::new(68, 3),
    FloorItemRow::new(78, 4),
    FloorItemRow::new(88, 5),
    FloorItemRow::new(92, 6),
    FloorItemRow::new(96, 8),
    FloorItemRow::new(100, 9),
];

/// `sPickupItemOffsets` (`src/battle_pyramid.c:411`): the first row of each
/// floor in [`FLOOR_ITEM_ROW_LIST`].
const FLOOR_ITEM_OFFSETS: [usize; PYRAMID_FLOORS] = [0, 9, 18, 27, 36, 45, 54];

/// `sPickupPercentages` (`src/battle_pyramid.c:832`).
const PICKUP_THRESHOLDS: [u8; PICKUP_ITEMS_PER_ROUND] = [30, 40, 50, 60, 70, 80, 85, 90, 95, 100];

/// Whether the tables keep the invariants the lookups and the upstream
/// scans depend on: a percentile always finds a row, and each floor's rows
/// are the nine at its offset.
const fn tables_are_well_formed() -> bool {
    let mut floor = 0;
    while floor < PYRAMID_FLOORS {
        if FLOOR_ITEM_OFFSETS[floor] != floor * FLOOR_ITEM_ROWS_PER_FLOOR {
            return false;
        }
        let first = FLOOR_ITEM_OFFSETS[floor];
        let mut row = 1;
        while row < FLOOR_ITEM_ROWS_PER_FLOOR {
            if FLOOR_ITEM_ROW_LIST[first + row].threshold
                <= FLOOR_ITEM_ROW_LIST[first + row - 1].threshold
            {
                return false;
            }
            row += 1;
        }
        if FLOOR_ITEM_ROW_LIST[first + FLOOR_ITEM_ROWS_PER_FLOOR - 1].threshold != 100 {
            return false;
        }
        floor += 1;
    }
    let mut index = 1;
    while index < PICKUP_ITEMS_PER_ROUND {
        if PICKUP_THRESHOLDS[index] <= PICKUP_THRESHOLDS[index - 1] {
            return false;
        }
        index += 1;
    }
    PICKUP_THRESHOLDS[PICKUP_ITEMS_PER_ROUND - 1] == 100
}

const _: () = assert!(
    tables_are_well_formed(),
    "Battle Pyramid floor rows or Pickup thresholds are malformed"
);

/// Returns the ten items of one round in one level mode, in column order.
///
/// # Errors
///
/// Returns [`PyramidItemError::InvalidRound`] when `round` is not below
/// [`PYRAMID_ROUNDS`].
pub fn pyramid_round_items(
    mode: PyramidItemMode,
    round: usize,
) -> Result<&'static [ItemId; PICKUP_ITEMS_PER_ROUND], PyramidItemError> {
    let matrix = match mode {
        PyramidItemMode::Level50 => &LEVEL_50_ITEMS,
        PyramidItemMode::OpenLevel => &OPEN_LEVEL_ITEMS,
    };
    matrix
        .get(round)
        .ok_or(PyramidItemError::InvalidRound(round))
}

/// Returns the item at one column of one round in one level mode.
///
/// # Errors
///
/// Returns [`PyramidItemError::InvalidRound`] when `round` is not below
/// [`PYRAMID_ROUNDS`], and [`PyramidItemError::InvalidColumn`] when `column`
/// is not below [`PICKUP_ITEMS_PER_ROUND`].
pub fn pyramid_item(
    mode: PyramidItemMode,
    round: usize,
    column: usize,
) -> Result<ItemId, PyramidItemError> {
    pyramid_round_items(mode, round)?
        .get(column)
        .copied()
        .ok_or(PyramidItemError::InvalidColumn(column))
}

/// Returns the position of a floor's first row in the floor-row list
/// (`sPickupItemOffsets`).
///
/// # Errors
///
/// Returns [`PyramidItemError::InvalidFloor`] when `floor` is not below
/// [`PYRAMID_FLOORS`].
pub fn floor_item_row_offset(floor: usize) -> Result<usize, PyramidItemError> {
    FLOOR_ITEM_OFFSETS
        .get(floor)
        .copied()
        .ok_or(PyramidItemError::InvalidFloor(floor))
}

/// Returns the nine rows of one floor, in upstream order (`sPickupItemSlots`
/// from that floor's offset).
///
/// # Errors
///
/// Returns [`PyramidItemError::InvalidFloor`] when `floor` is not below
/// [`PYRAMID_FLOORS`].
pub fn floor_item_rows(floor: usize) -> Result<&'static [FloorItemRow], PyramidItemError> {
    let offset = floor_item_row_offset(floor)?;
    Ok(&FLOOR_ITEM_ROW_LIST[offset..offset + FLOOR_ITEM_ROWS_PER_FLOOR])
}

/// Returns the ten Pickup thresholds (`sPickupPercentages`), in order.
#[must_use]
pub const fn pickup_thresholds() -> &'static [u8; PICKUP_ITEMS_PER_ROUND] {
    &PICKUP_THRESHOLDS
}

/// Returns one Pickup threshold: the exclusive upper bound of the percentiles
/// that select the column of the same index.
///
/// # Errors
///
/// Returns [`PyramidItemError::InvalidThresholdIndex`] when `index` is not
/// below [`PICKUP_ITEMS_PER_ROUND`].
pub fn pickup_threshold(index: usize) -> Result<u8, PyramidItemError> {
    PICKUP_THRESHOLDS
        .get(index)
        .copied()
        .ok_or(PyramidItemError::InvalidThresholdIndex(index))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `sPickupItemsLvl50`, as raw `ITEM_*` values from
    /// `include/constants/items.h`.
    #[rustfmt::skip]
    const EXPECTED_LEVEL_50: [[u16; PICKUP_ITEMS_PER_ROUND]; PYRAMID_ROUNDS] = [
        [21, 81, 133, 34, 141, 24, 179, 219, 25, 45],
        [21, 74, 135, 34, 138, 24, 200, 186, 19, 37],
        [21, 75, 136, 34, 141, 24, 198, 196, 25, 45],
        [21, 76, 141, 34, 138, 24, 183, 187, 19, 37],
        [21, 77, 134, 34, 141, 24, 179, 219, 25, 45],
        [21, 78, 141, 34, 138, 24, 200, 186, 19, 37],
        [21, 79, 141, 34, 141, 24, 198, 196, 25, 45],
        [21, 73, 141, 34, 138, 24, 183, 187, 19, 37],
        [21, 81, 141, 34, 141, 24, 179, 219, 25, 45],
        [21, 74, 141, 34, 138, 24, 200, 186, 19, 37],
        [21, 75, 141, 34, 141, 24, 198, 196, 25, 45],
        [21, 76, 141, 34, 138, 24, 183, 187, 19, 37],
        [21, 77, 141, 34, 141, 24, 179, 219, 25, 45],
        [21, 78, 141, 34, 138, 24, 200, 186, 19, 37],
        [21, 79, 141, 34, 141, 24, 198, 196, 25, 45],
        [21, 73, 141, 34, 138, 24, 183, 187, 19, 37],
        [21, 81, 141, 34, 141, 24, 179, 219, 25, 45],
        [21, 74, 141, 34, 138, 24, 200, 186, 19, 37],
        [21, 75, 141, 34, 141, 24, 198, 196, 25, 45],
        [21, 76, 141, 34, 138, 24, 183, 187, 19, 37],
    ];

    /// `sPickupItemsLvlOpen`, as raw `ITEM_*` values.
    #[rustfmt::skip]
    const EXPECTED_OPEN_LEVEL: [[u16; PICKUP_ITEMS_PER_ROUND]; PYRAMID_ROUNDS] = [
        [21, 81, 133, 34, 141, 24, 179, 219, 25, 45],
        [21, 74, 135, 34, 138, 24, 200, 186, 19, 37],
        [21, 75, 136, 34, 141, 24, 198, 196, 25, 45],
        [21, 76, 141, 34, 138, 24, 183, 187, 19, 37],
        [21, 77, 134, 34, 141, 24, 179, 219, 25, 45],
        [21, 78, 141, 34, 138, 24, 200, 186, 19, 37],
        [21, 79, 141, 34, 141, 24, 198, 196, 25, 45],
        [21, 73, 141, 34, 138, 24, 183, 187, 19, 37],
        [21, 81, 141, 34, 141, 24, 179, 219, 25, 45],
        [21, 74, 141, 34, 138, 24, 200, 186, 19, 37],
        [21, 75, 141, 34, 141, 24, 198, 196, 25, 45],
        [21, 76, 141, 34, 138, 24, 183, 187, 19, 37],
        [21, 77, 141, 34, 141, 24, 179, 219, 25, 45],
        [21, 78, 141, 34, 138, 24, 200, 186, 19, 37],
        [21, 79, 141, 34, 141, 24, 198, 196, 25, 45],
        [21, 73, 141, 34, 138, 24, 183, 187, 19, 37],
        [21, 81, 141, 34, 141, 24, 179, 219, 25, 45],
        [21, 74, 141, 34, 138, 24, 200, 186, 19, 37],
        [21, 75, 141, 34, 141, 24, 198, 196, 25, 45],
        [21, 76, 141, 34, 138, 24, 183, 187, 19, 37],
    ];

    /// `sPickupItemSlots` as `(threshold, column)`, floor after floor.
    #[rustfmt::skip]
    const EXPECTED_ROWS: [(u8, usize); FLOOR_ITEM_ROW_COUNT] = [
        (31, 0),
        (46, 1),
        (61, 2),
        (71, 3),
        (81, 4),
        (91, 5),
        (94, 6),
        (97, 7),
        (100, 8),
        (15, 0),
        (46, 1),
        (61, 2),
        (71, 3),
        (81, 4),
        (91, 5),
        (94, 6),
        (97, 8),
        (100, 9),
        (15, 0),
        (30, 1),
        (61, 2),
        (71, 3),
        (81, 4),
        (91, 5),
        (94, 6),
        (97, 7),
        (100, 8),
        (28, 0),
        (43, 1),
        (58, 2),
        (68, 3),
        (78, 4),
        (88, 5),
        (92, 7),
        (96, 8),
        (100, 9),
        (15, 0),
        (43, 1),
        (58, 2),
        (68, 3),
        (78, 4),
        (88, 5),
        (92, 6),
        (96, 7),
        (100, 9),
        (15, 0),
        (30, 1),
        (58, 2),
        (68, 3),
        (78, 4),
        (88, 5),
        (92, 6),
        (96, 7),
        (100, 8),
        (28, 0),
        (43, 1),
        (58, 2),
        (68, 3),
        (78, 4),
        (88, 5),
        (92, 6),
        (96, 8),
        (100, 9),
    ];

    fn raw_round(mode: PyramidItemMode, round: usize) -> [u16; PICKUP_ITEMS_PER_ROUND] {
        pyramid_round_items(mode, round).unwrap().map(ItemId::index)
    }

    #[test]
    fn level_50_matrix_is_pinned_record_by_record() {
        for (round, expected) in EXPECTED_LEVEL_50.iter().enumerate() {
            assert_eq!(
                &raw_round(PyramidItemMode::Level50, round),
                expected,
                "round {round}"
            );
            for (column, &id) in expected.iter().enumerate() {
                assert_eq!(
                    pyramid_item(PyramidItemMode::Level50, round, column),
                    Ok(ItemId(id)),
                    "round {round} column {column}"
                );
            }
        }
    }

    #[test]
    fn open_level_matrix_is_pinned_record_by_record() {
        for (round, expected) in EXPECTED_OPEN_LEVEL.iter().enumerate() {
            assert_eq!(
                &raw_round(PyramidItemMode::OpenLevel, round),
                expected,
                "round {round}"
            );
            for (column, &id) in expected.iter().enumerate() {
                assert_eq!(
                    pyramid_item(PyramidItemMode::OpenLevel, round, column),
                    Ok(ItemId(id)),
                    "round {round} column {column}"
                );
            }
        }
    }

    #[test]
    fn matrices_have_the_upstream_dimensions() {
        assert_eq!(PYRAMID_ROUNDS, 20);
        assert_eq!(PICKUP_ITEMS_PER_ROUND, 10);
        assert_eq!(EXPECTED_LEVEL_50.len(), PYRAMID_ROUNDS);
        assert_eq!(EXPECTED_OPEN_LEVEL.len(), PYRAMID_ROUNDS);
    }

    #[test]
    fn spot_checked_records_name_the_expected_items() {
        // The first round's first, berry and last columns, and later rounds'
        // records, by item name.
        let mode = PyramidItemMode::Level50;
        assert_eq!(pyramid_item(mode, 0, 0), Ok(ItemId::HYPER_POTION));
        assert_eq!(pyramid_item(mode, 0, 2), Ok(ItemId::CHERI_BERRY));
        assert_eq!(pyramid_item(mode, 0, 9), Ok(ItemId::SACRED_ASH));
        assert_eq!(pyramid_item(mode, 19, 1), Ok(ItemId::X_DEFEND));
        assert_eq!(pyramid_item(mode, 19, 9), Ok(ItemId::MAX_ELIXIR));
        let open = PyramidItemMode::OpenLevel;
        assert_eq!(pyramid_item(open, 7, 1), Ok(ItemId::GUARD_SPEC));
        assert_eq!(pyramid_item(open, 18, 6), Ok(ItemId::SCOPE_LENS));
    }

    #[test]
    fn floor_offsets_are_pinned() {
        let offsets: Vec<usize> = (0..PYRAMID_FLOORS)
            .map(|floor| floor_item_row_offset(floor).unwrap())
            .collect();
        assert_eq!(offsets, [0, 9, 18, 27, 36, 45, 54]);
    }

    #[test]
    fn floor_rows_are_pinned_in_order() {
        let mut all = Vec::new();
        for floor in 0..PYRAMID_FLOORS {
            let rows = floor_item_rows(floor).unwrap();
            assert_eq!(rows.len(), FLOOR_ITEM_ROWS_PER_FLOOR, "floor {floor}");
            all.extend(rows.iter().map(|row| (row.threshold(), row.column())));
        }
        assert_eq!(all, EXPECTED_ROWS);
    }

    #[test]
    fn floor_rows_start_at_their_offsets_and_end_at_100() {
        for floor in 0..PYRAMID_FLOORS {
            let offset = floor_item_row_offset(floor).unwrap();
            let rows = floor_item_rows(floor).unwrap();
            let expected = &EXPECTED_ROWS[offset..offset + FLOOR_ITEM_ROWS_PER_FLOOR];
            let actual: Vec<_> = rows.iter().map(|r| (r.threshold(), r.column())).collect();
            assert_eq!(actual, expected, "floor {floor}");
            assert_eq!(rows.last().unwrap().threshold(), 100, "floor {floor}");
            assert!(
                rows.windows(2)
                    .all(|pair| pair[0].threshold() < pair[1].threshold()),
                "floor {floor} thresholds rise"
            );
        }
    }

    #[test]
    fn floors_differ_where_upstream_differs() {
        // Floors 0, 2 and 5 take columns 0 to 8; floors 1, 3, 4 and 6 each
        // skip one column and end on column 9.
        let columns = |floor: usize| -> Vec<usize> {
            floor_item_rows(floor)
                .unwrap()
                .iter()
                .map(|r| r.column())
                .collect()
        };
        assert_eq!(columns(0), [0, 1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(columns(1), [0, 1, 2, 3, 4, 5, 6, 8, 9]);
        assert_eq!(columns(3), [0, 1, 2, 3, 4, 5, 7, 8, 9]);
        assert_eq!(columns(4), [0, 1, 2, 3, 4, 5, 6, 7, 9]);
        assert_eq!(columns(6), [0, 1, 2, 3, 4, 5, 6, 8, 9]);
    }

    #[test]
    fn pickup_thresholds_are_pinned_with_the_final_100() {
        assert_eq!(
            pickup_thresholds(),
            &[30, 40, 50, 60, 70, 80, 85, 90, 95, 100]
        );
        for (index, &expected) in pickup_thresholds().iter().enumerate() {
            assert_eq!(pickup_threshold(index), Ok(expected));
        }
        assert_eq!(pickup_threshold(0), Ok(30));
        assert_eq!(pickup_threshold(9), Ok(100));
    }

    #[test]
    fn invalid_indices_are_refused() {
        for mode in [PyramidItemMode::Level50, PyramidItemMode::OpenLevel] {
            assert_eq!(
                pyramid_round_items(mode, PYRAMID_ROUNDS),
                Err(PyramidItemError::InvalidRound(PYRAMID_ROUNDS))
            );
            assert_eq!(
                pyramid_item(mode, PYRAMID_ROUNDS, 0),
                Err(PyramidItemError::InvalidRound(PYRAMID_ROUNDS))
            );
            assert_eq!(
                pyramid_item(mode, 0, PICKUP_ITEMS_PER_ROUND),
                Err(PyramidItemError::InvalidColumn(PICKUP_ITEMS_PER_ROUND))
            );
            assert_eq!(
                pyramid_item(mode, usize::MAX, usize::MAX),
                Err(PyramidItemError::InvalidRound(usize::MAX))
            );
        }
        assert_eq!(
            floor_item_row_offset(PYRAMID_FLOORS),
            Err(PyramidItemError::InvalidFloor(PYRAMID_FLOORS))
        );
        assert_eq!(
            floor_item_rows(PYRAMID_FLOORS),
            Err(PyramidItemError::InvalidFloor(PYRAMID_FLOORS))
        );
        assert_eq!(
            pickup_threshold(PICKUP_ITEMS_PER_ROUND),
            Err(PyramidItemError::InvalidThresholdIndex(
                PICKUP_ITEMS_PER_ROUND
            ))
        );
    }

    #[test]
    fn errors_name_the_offending_index() {
        assert_eq!(
            PyramidItemError::InvalidRound(20).to_string(),
            "Battle Pyramid round `20` is out of range (there are 20)"
        );
        assert_eq!(
            PyramidItemError::InvalidColumn(10).to_string(),
            "Battle Pyramid item column `10` is out of range (there are 10)"
        );
        assert_eq!(
            PyramidItemError::InvalidFloor(7).to_string(),
            "Battle Pyramid floor `7` is out of range (there are 7)"
        );
        assert_eq!(
            PyramidItemError::InvalidThresholdIndex(10).to_string(),
            "Battle Pyramid Pickup threshold `10` is out of range (there are 10)"
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

    /// The text of the initializer that follows `declaration`, up to its `};`.
    fn initializer<'a>(source: &'a str, declaration: &str) -> &'a str {
        let start = source
            .find(declaration)
            .unwrap_or_else(|| panic!("`{declaration}` not found"));
        let body = &source[start..];
        let open = body.find("=\n{").expect("initializer opens") + 3;
        let close = body.find("\n};").expect("initializer closes");
        body[open..close].trim()
    }

    /// Parses a matrix initializer into rows of raw item ids.
    fn parse_matrix(
        source: &str,
        items: &std::collections::HashMap<String, u16>,
        declaration: &str,
    ) -> Vec<Vec<u16>> {
        initializer(source, declaration)
            .lines()
            .map(|line| {
                let list = line
                    .trim()
                    .trim_end_matches(',')
                    .strip_prefix('{')
                    .and_then(|rest| rest.strip_suffix('}'))
                    .unwrap_or_else(|| panic!("braced row expected: `{line}`"));
                list.split(", ").map(|name| items[name.trim()]).collect()
            })
            .collect()
    }

    #[test]
    #[ignore = "reads the pinned upstream checkout; run ./init.sh first"]
    fn upstream_source_matches_the_tables() {
        let items: std::collections::HashMap<String, u16> =
            upstream_file("include/constants/items.h")
                .lines()
                .filter_map(|line| {
                    let mut words = line.split_whitespace();
                    (words.next()? == "#define").then_some(())?;
                    let name = words.next()?.to_owned();
                    let value = words.next()?.parse().ok()?;
                    Some((name, value))
                })
                .collect();
        let source = upstream_file("src/battle_pyramid.c");

        for (declaration, mode) in [
            (
                "sPickupItemsLvl50[TOTAL_PYRAMID_ROUNDS][PICKUP_ITEMS_PER_ROUND]",
                PyramidItemMode::Level50,
            ),
            (
                "sPickupItemsLvlOpen[TOTAL_PYRAMID_ROUNDS][PICKUP_ITEMS_PER_ROUND]",
                PyramidItemMode::OpenLevel,
            ),
        ] {
            let rows = parse_matrix(&source, &items, declaration);
            assert_eq!(rows.len(), PYRAMID_ROUNDS, "{declaration}");
            for (round, row) in rows.iter().enumerate() {
                assert_eq!(
                    row.as_slice(),
                    raw_round(mode, round).as_slice(),
                    "{declaration} round {round}"
                );
            }
        }

        let slots: Vec<(u8, usize)> = initializer(&source, "sPickupItemSlots[][2]")
            .lines()
            .filter(|line| !line.trim().starts_with("//"))
            .map(|line| {
                let inner = line
                    .trim()
                    .trim_end_matches(',')
                    .strip_prefix('{')
                    .and_then(|rest| rest.strip_suffix('}'))
                    .unwrap_or_else(|| panic!("braced row expected: `{line}`"));
                let (threshold, column) = inner.split_once(',').expect("two fields");
                (
                    threshold.trim().parse().unwrap(),
                    column.trim().parse().unwrap(),
                )
            })
            .collect();
        assert_eq!(slots, EXPECTED_ROWS);

        let list_after = |declaration: &str| -> Vec<usize> {
            let start = source.find(declaration).expect("declaration present");
            let rest = &source[start..];
            let open = rest.find('{').expect("list opens") + 1;
            let close = rest.find('}').expect("list closes");
            rest[open..close]
                .split(',')
                .map(|value| value.trim().parse().unwrap())
                .collect()
        };
        assert_eq!(
            list_after("sPickupItemOffsets[FRONTIER_STAGES_PER_CHALLENGE]"),
            FLOOR_ITEM_OFFSETS
        );
        let thresholds: Vec<usize> = list_after("sPickupPercentages[PICKUP_ITEMS_PER_ROUND]");
        assert_eq!(
            thresholds,
            pickup_thresholds()
                .iter()
                .map(|&t| usize::from(t))
                .collect::<Vec<_>>()
        );
    }
}
