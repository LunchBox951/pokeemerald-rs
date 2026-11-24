//! The Roulette wheel's twelve slots: the betting-grid square and hit flag
//! each wheel slot scores.
//!
//! Source: `sRouletteSlots` (`src/roulette.c`), read by `RecordHit` through
//! `gridSquare` and `flag`, with the `SQU_*` and `F_*` defines it names. The
//! struct's `id1` and `id2` fields are never read upstream and are left out.
//!
//! Slots are in wheel order, from slot `0`; that order is not the order of the
//! grid squares. Every lookup refuses a slot or grid square that is out of
//! range instead of clamping it. Grid rendering, bet selection, ball physics,
//! payouts and coin changes are not ported here.

use std::error::Error;
use std::fmt;

/// The number of wheel slots (`NUM_ROULETTE_SLOTS`).
pub const WHEEL_SLOT_COUNT: usize = 12;

/// A failed lookup in the Roulette wheel-slot data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouletteSlotError {
    /// The wheel slot is not below [`WHEEL_SLOT_COUNT`].
    UnknownSlot(usize),
    /// The value is not the `SQU_*` value of any of the twelve grid squares.
    UnknownGridSquare(u8),
}

impl fmt::Display for RouletteSlotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownSlot(slot) => write!(
                f,
                "unknown Roulette wheel slot `{slot}` (there are {WHEEL_SLOT_COUNT})"
            ),
            Self::UnknownGridSquare(value) => {
                write!(f, "`{value}` is not a Roulette grid square")
            }
        }
    }
}

impl Error for RouletteSlotError {}

/// A wheel slot, in wheel order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WheelSlot(u8);

impl WheelSlot {
    /// Validates a raw wheel-slot index.
    ///
    /// # Errors
    ///
    /// Returns [`RouletteSlotError::UnknownSlot`] when `index` is not below
    /// [`WHEEL_SLOT_COUNT`].
    pub const fn new(index: usize) -> Result<Self, RouletteSlotError> {
        if index < WHEEL_SLOT_COUNT {
            Ok(Self(index as u8))
        } else {
            Err(RouletteSlotError::UnknownSlot(index))
        }
    }

    /// Returns the raw wheel-slot index.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// One of the twelve betting-grid squares (a color row and a Pokémon column).
/// The discriminants are the upstream `SQU_*` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum GridSquare {
    OrangeWynaut = 6,
    OrangeAzurill = 7,
    OrangeSkitty = 8,
    OrangeMakuhita = 9,
    GreenWynaut = 11,
    GreenAzurill = 12,
    GreenSkitty = 13,
    GreenMakuhita = 14,
    PurpleWynaut = 16,
    PurpleAzurill = 17,
    PurpleSkitty = 18,
    PurpleMakuhita = 19,
}

impl GridSquare {
    /// Every grid square, in `SQU_*` order.
    pub const ALL: [Self; WHEEL_SLOT_COUNT] = [
        Self::OrangeWynaut,
        Self::OrangeAzurill,
        Self::OrangeSkitty,
        Self::OrangeMakuhita,
        Self::GreenWynaut,
        Self::GreenAzurill,
        Self::GreenSkitty,
        Self::GreenMakuhita,
        Self::PurpleWynaut,
        Self::PurpleAzurill,
        Self::PurpleSkitty,
        Self::PurpleMakuhita,
    ];

    /// Validates a raw `SQU_*` value.
    ///
    /// # Errors
    ///
    /// Returns [`RouletteSlotError::UnknownGridSquare`] when `value` is not
    /// the value of one of the twelve squares; the row and column selections
    /// (`ROW_*`, `COL_*`) are not squares.
    pub const fn new(value: u8) -> Result<Self, RouletteSlotError> {
        let mut i = 0;
        while i < Self::ALL.len() {
            if Self::ALL[i].value() == value {
                return Ok(Self::ALL[i]);
            }
            i += 1;
        }
        Err(RouletteSlotError::UnknownGridSquare(value))
    }

    /// Returns the raw upstream `SQU_*` value.
    #[must_use]
    pub const fn value(self) -> u8 {
        self as u8
    }
}

/// What one wheel slot scores (`gridSquare` and `flag` of `struct RouletteSlot`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RouletteSlot {
    /// The grid square the slot hits.
    pub grid_square: GridSquare,
    /// The bit `RecordHit` sets in the hit flags: the matching `F_*` define.
    pub flag: u32,
}

/// `sRouletteSlots`, in wheel order.
const WHEEL_SLOTS: [RouletteSlot; WHEEL_SLOT_COUNT] = {
    use GridSquare::{
        GreenAzurill, GreenMakuhita, GreenSkitty, GreenWynaut, OrangeAzurill, OrangeMakuhita,
        OrangeSkitty, OrangeWynaut, PurpleAzurill, PurpleMakuhita, PurpleSkitty, PurpleWynaut,
    };
    const fn slot(grid_square: GridSquare) -> RouletteSlot {
        RouletteSlot {
            grid_square,
            flag: 1 << grid_square.value(),
        }
    }
    [
        slot(OrangeWynaut),
        slot(GreenAzurill),
        slot(PurpleSkitty),
        slot(OrangeMakuhita),
        slot(GreenWynaut),
        slot(PurpleAzurill),
        slot(OrangeSkitty),
        slot(GreenMakuhita),
        slot(PurpleWynaut),
        slot(OrangeAzurill),
        slot(GreenSkitty),
        slot(PurpleMakuhita),
    ]
};

/// Returns what `slot` scores.
#[must_use]
pub const fn wheel_slot(slot: WheelSlot) -> RouletteSlot {
    WHEEL_SLOTS[slot.index()]
}

/// Returns every wheel slot, in wheel order from slot `0`.
#[must_use]
pub const fn wheel_slots() -> &'static [RouletteSlot; WHEEL_SLOT_COUNT] {
    &WHEEL_SLOTS
}

/// Returns what the slot at `index` scores.
///
/// # Errors
///
/// Returns [`RouletteSlotError::UnknownSlot`] when `index` is not below
/// [`WHEEL_SLOT_COUNT`].
pub fn wheel_slot_at(index: usize) -> Result<RouletteSlot, RouletteSlotError> {
    WheelSlot::new(index).map(wheel_slot)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rows as (`SQU_*` value, `F_*` bit) in wheel order, laid out from
    /// the defines in `src/roulette.c` rather than from `WHEEL_SLOTS`: a
    /// `COL_*` is 1 to 4 (Wynaut, Azurill, Skitty, Makuhita), `ROW_ORANGE` is
    /// 5, `ROW_GREEN` 10, `ROW_PURPLE` 15, and a square is its row plus its
    /// column.
    const EXPECTED: [(u8, u32); WHEEL_SLOT_COUNT] = [
        (6, 1 << 6),   // orange Wynaut
        (12, 1 << 12), // green Azurill
        (18, 1 << 18), // purple Skitty
        (9, 1 << 9),   // orange Makuhita
        (11, 1 << 11), // green Wynaut
        (17, 1 << 17), // purple Azurill
        (8, 1 << 8),   // orange Skitty
        (14, 1 << 14), // green Makuhita
        (16, 1 << 16), // purple Wynaut
        (7, 1 << 7),   // orange Azurill
        (13, 1 << 13), // green Skitty
        (19, 1 << 19), // purple Makuhita
    ];

    fn raw(rows: &[RouletteSlot; WHEEL_SLOT_COUNT]) -> [(u8, u32); WHEEL_SLOT_COUNT] {
        rows.map(|row| (row.grid_square.value(), row.flag))
    }

    #[test]
    fn every_wheel_slot_is_pinned() {
        assert_eq!(raw(wheel_slots()), EXPECTED);
        for (index, &(square, flag)) in EXPECTED.iter().enumerate() {
            let row = wheel_slot_at(index).unwrap();
            assert_eq!(row.grid_square, GridSquare::new(square).unwrap(), "{index}");
            assert_eq!(row.flag, flag, "{index}");
            assert_eq!(
                wheel_slot(WheelSlot::new(index).unwrap()),
                row,
                "slot {index}"
            );
        }
    }

    #[test]
    fn rows_stay_in_wheel_order_not_grid_order() {
        let squares = EXPECTED.map(|(square, _)| square);
        assert!(!squares.is_sorted());
        assert_ne!(raw(wheel_slots()), {
            let mut sorted = EXPECTED;
            sorted.sort_by_key(|&(square, _)| square);
            sorted
        });
        assert_eq!(
            wheel_slot_at(0).unwrap().grid_square,
            GridSquare::OrangeWynaut
        );
        assert_eq!(
            wheel_slot_at(1).unwrap().grid_square,
            GridSquare::GreenAzurill
        );
    }

    #[test]
    fn mutating_the_real_table_is_caught_by_the_fixture() {
        let real = *wheel_slots();
        assert_eq!(raw(&real), EXPECTED);

        let mut sorted = real;
        sorted.sort_by_key(|row| row.grid_square);
        assert_ne!(raw(&sorted), EXPECTED);

        let mut swapped = real;
        swapped.swap(3, 4);
        assert_ne!(raw(&swapped), EXPECTED);

        let mut wrong_flag = real;
        wrong_flag[7].flag ^= 1;
        assert_ne!(raw(&wrong_flag), EXPECTED);

        let mut wrong_square = real;
        wrong_square[11].grid_square = GridSquare::PurpleSkitty;
        assert_ne!(raw(&wrong_square), EXPECTED);
    }

    #[test]
    fn each_square_is_hit_by_exactly_one_slot() {
        for square in GridSquare::ALL {
            let hits = wheel_slots()
                .iter()
                .filter(|row| row.grid_square == square)
                .count();
            assert_eq!(hits, 1, "{square:?}");
        }
    }

    #[test]
    fn each_flag_is_the_bit_of_its_square() {
        for row in wheel_slots() {
            assert_eq!(row.flag.count_ones(), 1);
            assert_eq!(row.flag, 1 << row.grid_square.value());
        }
    }

    #[test]
    fn grid_square_values_round_trip() {
        for square in GridSquare::ALL {
            assert_eq!(GridSquare::new(square.value()), Ok(square));
        }
    }

    #[test]
    fn grid_squares_are_the_row_plus_column_values() {
        let rows = [5_u8, 10, 15];
        let columns = [1_u8, 2, 3, 4];
        let mut expected = Vec::new();
        for row in rows {
            for column in columns {
                expected.push(row + column);
            }
        }
        assert_eq!(GridSquare::ALL.map(GridSquare::value).to_vec(), expected);
    }

    #[test]
    fn a_non_square_value_is_refused() {
        // 0 is `SELECTION_NONE`; 1 to 4 are columns; 5, 10 and 15 are rows.
        for value in [0, 1, 4, 5, 10, 15, 20, u8::MAX] {
            assert_eq!(
                GridSquare::new(value),
                Err(RouletteSlotError::UnknownGridSquare(value))
            );
        }
    }

    #[test]
    fn an_out_of_range_slot_is_refused() {
        assert_eq!(WheelSlot::new(11).map(WheelSlot::index), Ok(11));
        assert_eq!(WheelSlot::new(12), Err(RouletteSlotError::UnknownSlot(12)));
        assert_eq!(wheel_slot_at(12), Err(RouletteSlotError::UnknownSlot(12)));
        assert_eq!(
            wheel_slot_at(usize::MAX),
            Err(RouletteSlotError::UnknownSlot(usize::MAX))
        );
    }

    #[test]
    fn errors_name_the_offending_value() {
        assert!(RouletteSlotError::UnknownSlot(12)
            .to_string()
            .contains("12"));
        assert!(RouletteSlotError::UnknownGridSquare(77)
            .to_string()
            .contains("77"));
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
    fn upstream_source_matches_the_table() {
        let source = upstream_file("src/roulette.c");
        let start = source
            .find("static const struct RouletteSlot sRouletteSlots[] =")
            .expect("sRouletteSlots not found");
        let body = &source[start..];
        let body = &body[..body.find("\n};").expect("end of sRouletteSlots")];

        let field = |line: &str, name: &str| -> Option<String> {
            let rest = line.trim().strip_prefix(name)?;
            Some(
                rest.trim_start_matches([' ', '='])
                    .trim_end_matches(',')
                    .to_owned(),
            )
        };
        let squares: Vec<String> = body
            .lines()
            .filter_map(|line| field(line, ".gridSquare"))
            .collect();
        let flags: Vec<String> = body
            .lines()
            .filter_map(|line| field(line, ".flag"))
            .collect();
        assert_eq!(squares.len(), WHEEL_SLOT_COUNT);
        assert_eq!(flags.len(), WHEEL_SLOT_COUNT);

        let name = |square: GridSquare| format!("{square:?}");
        let camel = |define: &str, prefix: &str| -> String {
            define
                .strip_prefix(prefix)
                .unwrap()
                .split('_')
                .map(|word| {
                    let mut chars = word.chars();
                    let first = chars.next().unwrap().to_ascii_uppercase();
                    format!("{first}{}", chars.as_str().to_ascii_lowercase())
                })
                .collect()
        };
        for (index, row) in wheel_slots().iter().enumerate() {
            assert_eq!(
                camel(&squares[index], "SQU_"),
                name(row.grid_square),
                "{index}"
            );
            assert_eq!(camel(&flags[index], "F_"), name(row.grid_square), "{index}");
        }
    }
}
