//! The roaming Pokémon location sets: for each of the twenty maps the
//! roamer can start a move from, the five maps it may move to next.
//!
//! Source: `sRoamerLocations` (`src/roamer.c:35-58`), a table of
//! `u8[6]` rows. Entry 0 of a row is the map the set belongs to
//! (`MAP_NUM(MAP_ROUTE110)` and so on); entries 1 to 5 are the candidate
//! destinations in upstream order, where a slot with no map holds
//! `MAP_NUM(MAP_UNDEFINED)` (`___`). The twenty sets are followed by one
//! all-empty terminal row (`src/roamer.c:57`); upstream counts it out with
//! `NUM_LOCATION_SETS` (`ARRAY_COUNT(sRoamerLocations) - 1`,
//! `src/roamer.c:61`), so it is not a selectable set and is not exposed here.
//!
//! Upstream stores only the map number because the roamer is pinned to map
//! group 0 (`ROAMER_MAP_GROUP`, `src/roamer.c:9`); this catalog holds the
//! symbolic [`MapId`] instead. Empty candidate slots are kept in place as
//! `None`, never compacted, so a candidate keeps its upstream slot index.
//!
//! Data only: the roamer's saved state, `Random()` draws, the rejection loops
//! in `RoamerMoveToOtherLocationSet` and `RoamerMove`, the location history,
//! and roamer encounters are not ported here.

use std::error::Error;
use std::fmt;

use crate::wild_encounters::MapId;

/// The number of selectable location sets (`NUM_LOCATION_SETS`).
pub const ROAMER_LOCATION_SET_COUNT: usize = 20;

/// The number of candidate destinations in each set
/// (`NUM_LOCATIONS_PER_SET - 1`; entry 0 of an upstream row is the set's own
/// map).
pub const ROAMER_CANDIDATES_PER_SET: usize = 5;

/// A failed lookup in the roamer location sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoamerLocationError {
    /// The set index is not below [`ROAMER_LOCATION_SET_COUNT`]. Upstream's
    /// all-empty terminal row (index 20) is refused here too.
    SetOutOfRange(usize),
    /// The candidate slot is not below [`ROAMER_CANDIDATES_PER_SET`].
    CandidateOutOfRange(usize),
}

impl fmt::Display for RoamerLocationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SetOutOfRange(index) => write!(
                f,
                "roamer location set `{index}` is out of range (expected below \
                 {ROAMER_LOCATION_SET_COUNT})"
            ),
            Self::CandidateOutOfRange(slot) => write!(
                f,
                "roamer candidate slot `{slot}` is out of range (expected below \
                 {ROAMER_CANDIDATES_PER_SET})"
            ),
        }
    }
}

impl Error for RoamerLocationError {}

/// One row of `sRoamerLocations`: the set's own map and its five candidate
/// destinations in upstream order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoamerLocationSet {
    map: MapId,
    candidates: [Option<MapId>; ROAMER_CANDIDATES_PER_SET],
}

impl RoamerLocationSet {
    /// The map the set belongs to (entry 0 of the upstream row).
    #[must_use]
    pub const fn map(&self) -> MapId {
        self.map
    }

    /// The candidate destinations (entries 1 to 5 of the upstream row), with
    /// `None` for an empty slot (`MAP_UNDEFINED`).
    #[must_use]
    pub const fn candidates(&self) -> &[Option<MapId>; ROAMER_CANDIDATES_PER_SET] {
        &self.candidates
    }

    /// Returns candidate `slot` (`0` is upstream entry 1), `None` when the
    /// slot is empty.
    ///
    /// # Errors
    ///
    /// Returns [`RoamerLocationError::CandidateOutOfRange`] when `slot` is not
    /// below [`ROAMER_CANDIDATES_PER_SET`].
    pub fn candidate(&self, slot: usize) -> Result<Option<MapId>, RoamerLocationError> {
        self.candidates
            .get(slot)
            .copied()
            .ok_or(RoamerLocationError::CandidateOutOfRange(slot))
    }
}

/// Builds one [`RoamerLocationSet`] from an upstream row written as route
/// numbers (`MAP_ROUTE<number>`); `_` is `___` (`MAP_UNDEFINED`).
macro_rules! set {
    ($map:literal; $($slot:tt),+ $(,)?) => {
        RoamerLocationSet {
            map: set!(@map $map),
            candidates: [$(set!(@slot $slot)),+],
        }
    };
    (@map $number:literal) => { MapId(concat!("MAP_ROUTE", stringify!($number))) };
    (@slot _) => { None };
    (@slot $number:literal) => { Some(set!(@map $number)) };
}

/// `sRoamerLocations` without its terminal row, in upstream order.
#[rustfmt::skip]
static ROAMER_LOCATION_SETS: [RoamerLocationSet; ROAMER_LOCATION_SET_COUNT] = [
    set!(110; 111, 117, 118, 134, _),
    set!(111; 110, 117, 118, _, _),
    set!(117; 111, 110, 118, _, _),
    set!(118; 117, 110, 111, 119, 123),
    set!(119; 118, 120, _, _, _),
    set!(120; 119, 121, _, _, _),
    set!(121; 120, 122, 123, _, _),
    set!(122; 121, 123, _, _, _),
    set!(123; 122, 118, _, _, _),
    set!(124; 121, 125, 126, _, _),
    set!(125; 124, 127, _, _, _),
    set!(126; 124, 127, _, _, _),
    set!(127; 125, 126, 128, _, _),
    set!(128; 127, 129, _, _, _),
    set!(129; 128, 130, _, _, _),
    set!(130; 129, 131, _, _, _),
    set!(131; 130, 132, _, _, _),
    set!(132; 131, 133, _, _, _),
    set!(133; 132, 134, _, _, _),
    set!(134; 133, 110, _, _, _),
];

/// Returns location set `index`, in upstream row order (`0` is the set for
/// `MAP_ROUTE110`).
///
/// # Errors
///
/// Returns [`RoamerLocationError::SetOutOfRange`] when `index` is not below
/// [`ROAMER_LOCATION_SET_COUNT`]; upstream's terminal row is not a set.
pub fn roamer_location_set(
    index: usize,
) -> Result<&'static RoamerLocationSet, RoamerLocationError> {
    ROAMER_LOCATION_SETS
        .get(index)
        .ok_or(RoamerLocationError::SetOutOfRange(index))
}

/// Returns all twenty location sets in upstream row order.
#[must_use]
pub fn roamer_location_sets() -> &'static [RoamerLocationSet; ROAMER_LOCATION_SET_COUNT] {
    &ROAMER_LOCATION_SETS
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Marks an empty slot (`MAP_UNDEFINED`) in [`EXPECTED`].
    const EMPTY: &str = "MAP_UNDEFINED";

    /// `sRoamerLocations` as the full `MAP_*` names, one `[own map, five
    /// candidates]` row per set, laid out from `src/roamer.c:37-56`.
    #[rustfmt::skip]
    const EXPECTED: [[&str; 6]; 20] = [
        ["MAP_ROUTE110", "MAP_ROUTE111", "MAP_ROUTE117", "MAP_ROUTE118", "MAP_ROUTE134", EMPTY],
        ["MAP_ROUTE111", "MAP_ROUTE110", "MAP_ROUTE117", "MAP_ROUTE118", EMPTY, EMPTY],
        ["MAP_ROUTE117", "MAP_ROUTE111", "MAP_ROUTE110", "MAP_ROUTE118", EMPTY, EMPTY],
        ["MAP_ROUTE118", "MAP_ROUTE117", "MAP_ROUTE110", "MAP_ROUTE111", "MAP_ROUTE119", "MAP_ROUTE123"],
        ["MAP_ROUTE119", "MAP_ROUTE118", "MAP_ROUTE120", EMPTY, EMPTY, EMPTY],
        ["MAP_ROUTE120", "MAP_ROUTE119", "MAP_ROUTE121", EMPTY, EMPTY, EMPTY],
        ["MAP_ROUTE121", "MAP_ROUTE120", "MAP_ROUTE122", "MAP_ROUTE123", EMPTY, EMPTY],
        ["MAP_ROUTE122", "MAP_ROUTE121", "MAP_ROUTE123", EMPTY, EMPTY, EMPTY],
        ["MAP_ROUTE123", "MAP_ROUTE122", "MAP_ROUTE118", EMPTY, EMPTY, EMPTY],
        ["MAP_ROUTE124", "MAP_ROUTE121", "MAP_ROUTE125", "MAP_ROUTE126", EMPTY, EMPTY],
        ["MAP_ROUTE125", "MAP_ROUTE124", "MAP_ROUTE127", EMPTY, EMPTY, EMPTY],
        ["MAP_ROUTE126", "MAP_ROUTE124", "MAP_ROUTE127", EMPTY, EMPTY, EMPTY],
        ["MAP_ROUTE127", "MAP_ROUTE125", "MAP_ROUTE126", "MAP_ROUTE128", EMPTY, EMPTY],
        ["MAP_ROUTE128", "MAP_ROUTE127", "MAP_ROUTE129", EMPTY, EMPTY, EMPTY],
        ["MAP_ROUTE129", "MAP_ROUTE128", "MAP_ROUTE130", EMPTY, EMPTY, EMPTY],
        ["MAP_ROUTE130", "MAP_ROUTE129", "MAP_ROUTE131", EMPTY, EMPTY, EMPTY],
        ["MAP_ROUTE131", "MAP_ROUTE130", "MAP_ROUTE132", EMPTY, EMPTY, EMPTY],
        ["MAP_ROUTE132", "MAP_ROUTE131", "MAP_ROUTE133", EMPTY, EMPTY, EMPTY],
        ["MAP_ROUTE133", "MAP_ROUTE132", "MAP_ROUTE134", EMPTY, EMPTY, EMPTY],
        ["MAP_ROUTE134", "MAP_ROUTE133", "MAP_ROUTE110", EMPTY, EMPTY, EMPTY],
    ];

    /// The upstream row for set `index`: own map, then the candidates, with
    /// `MAP_UNDEFINED` for each empty slot.
    fn actual_row(index: usize) -> [&'static str; 6] {
        let set = roamer_location_set(index).expect("set is in range");
        let mut row = [EMPTY; 6];
        row[0] = set.map().name();
        for (slot, candidate) in set.candidates().iter().enumerate() {
            row[slot + 1] = candidate.map_or(EMPTY, MapId::name);
        }
        row
    }

    /// Whether every catalog row equals the matching row of `table`.
    fn catalog_matches(table: &[[&str; 6]; 20]) -> bool {
        (0..ROAMER_LOCATION_SET_COUNT).all(|index| actual_row(index) == table[index])
    }

    #[test]
    fn counts_match_upstream() {
        assert_eq!(ROAMER_LOCATION_SET_COUNT, 20);
        assert_eq!(ROAMER_CANDIDATES_PER_SET, 5);
        assert_eq!(roamer_location_sets().len(), 20);
        assert_eq!(EXPECTED.len(), 20);
    }

    #[test]
    fn every_slot_matches_upstream() {
        let mut checked = 0;
        for (index, expected_row) in EXPECTED.iter().enumerate() {
            let actual = actual_row(index);
            for (slot, expected) in expected_row.iter().enumerate() {
                assert_eq!(actual[slot], *expected, "set {index}, entry {slot}");
                checked += 1;
            }
        }
        assert_eq!(checked, 120);
    }

    #[test]
    fn whole_table_matches_and_slice_accessor_agrees() {
        assert!(catalog_matches(&EXPECTED));
        for (index, set) in roamer_location_sets().iter().enumerate() {
            assert_eq!(set, roamer_location_set(index).unwrap(), "set {index}");
        }
    }

    #[test]
    fn candidate_slot_lookup_keeps_upstream_positions() {
        let set = roamer_location_set(0).unwrap();
        assert_eq!(set.map(), MapId("MAP_ROUTE110"));
        assert_eq!(set.candidate(0), Ok(Some(MapId("MAP_ROUTE111"))));
        assert_eq!(set.candidate(3), Ok(Some(MapId("MAP_ROUTE134"))));
        assert_eq!(set.candidate(4), Ok(None));

        // Route 118 is the only set with all five slots filled.
        let full = roamer_location_set(3).unwrap();
        assert_eq!(full.candidate(4), Ok(Some(MapId("MAP_ROUTE123"))));
        assert!(full.candidates().iter().all(Option::is_some));
        let full_sets = roamer_location_sets()
            .iter()
            .filter(|set| set.candidates().iter().all(Option::is_some))
            .count();
        assert_eq!(full_sets, 1);
    }

    #[test]
    fn empty_slots_are_kept_in_place() {
        let empties: usize = roamer_location_sets()
            .iter()
            .map(|set| set.candidates().iter().filter(|c| c.is_none()).count())
            .sum();
        let expected_empties = EXPECTED.iter().flatten().filter(|n| **n == EMPTY).count();
        assert_eq!(empties, expected_empties);
        assert_eq!(empties, 50);
        // Upstream fills slots front to back, so no destination follows an empty slot.
        for (index, set) in roamer_location_sets().iter().enumerate() {
            let first_empty = set.candidates().iter().position(Option::is_none);
            if let Some(first_empty) = first_empty {
                assert!(
                    set.candidates()[first_empty..].iter().all(Option::is_none),
                    "set {index}"
                );
            }
        }
    }

    #[test]
    fn every_set_belongs_to_a_distinct_map_and_destinations_exclude_it() {
        for (index, set) in roamer_location_sets().iter().enumerate() {
            let own = set.map();
            assert_eq!(own.name(), EXPECTED[index][0], "set {index}");
            assert!(
                set.candidates().iter().all(|c| *c != Some(own)),
                "set {index} lists its own map"
            );
            for other in &roamer_location_sets()[index + 1..] {
                assert_ne!(own, other.map(), "set {index}");
            }
        }
    }

    #[test]
    fn mutated_fixture_is_rejected() {
        let mut swapped = EXPECTED;
        swapped[0].swap(1, 2);
        assert!(!catalog_matches(&swapped));

        let mut rows_swapped = EXPECTED;
        rows_swapped.swap(4, 5);
        assert!(!catalog_matches(&rows_swapped));

        let mut compacted = EXPECTED;
        compacted[1] = [
            "MAP_ROUTE111",
            EMPTY,
            "MAP_ROUTE110",
            "MAP_ROUTE117",
            "MAP_ROUTE118",
            EMPTY,
        ];
        assert!(!catalog_matches(&compacted));

        let mut shifted = EXPECTED;
        shifted[4] = [
            "MAP_ROUTE119",
            "MAP_ROUTE118",
            "MAP_ROUTE120",
            "MAP_ROUTE120",
            EMPTY,
            EMPTY,
        ];
        assert!(!catalog_matches(&shifted));

        assert!(catalog_matches(&EXPECTED));
    }

    #[test]
    fn set_lookup_refuses_the_terminal_row_and_beyond() {
        for index in [20, 21, 255, usize::MAX] {
            assert_eq!(
                roamer_location_set(index),
                Err(RoamerLocationError::SetOutOfRange(index)),
                "set {index}"
            );
        }
        assert!(roamer_location_set(19).is_ok());
    }

    #[test]
    fn candidate_lookup_refuses_out_of_range_slots() {
        let set = roamer_location_set(0).unwrap();
        for slot in [5, 6, usize::MAX] {
            assert_eq!(
                set.candidate(slot),
                Err(RoamerLocationError::CandidateOutOfRange(slot)),
                "slot {slot}"
            );
        }
    }

    #[test]
    fn errors_name_the_rejected_index() {
        assert_eq!(
            RoamerLocationError::SetOutOfRange(20).to_string(),
            "roamer location set `20` is out of range (expected below 20)"
        );
        assert_eq!(
            RoamerLocationError::CandidateOutOfRange(5).to_string(),
            "roamer candidate slot `5` is out of range (expected below 5)"
        );
    }
}
