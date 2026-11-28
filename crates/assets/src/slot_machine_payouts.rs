//! The Game Corner slot machine's payout for each kind of winning match.
//!
//! Source: `src/slot_machine.c` (`sSlotPayouts` and the `MATCH_*` enum whose
//! values index it).
//!
//! A [`SlotMatch`] is a match id, which is not a reel-symbol id: it names a
//! winning pattern, and the two id spaces differ in size and meaning. The
//! no-match id (`MATCH_NONE`) and any other raw id outside the nine matches
//! are refused by [`SlotMatch::new`] instead of indexing the table. Only the
//! payout table is ported here: match detection, reel stopping, luck
//! selection, coin changes, the payout animation and replay scheduling are
//! not.

use std::error::Error;
use std::fmt;

/// The number of matches that have a payout entry (every `MATCH_*` value
/// below `MATCH_NONE`).
pub const SLOT_MATCH_COUNT: usize = 9;

/// A raw match id that does not name a match with a payout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotMatchError {
    /// The id is not below [`SLOT_MATCH_COUNT`]. This includes `MATCH_NONE`,
    /// which is the value [`SLOT_MATCH_COUNT`] and has no payout.
    UnknownMatch(u8),
}

impl fmt::Display for SlotMatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownMatch(id) => write!(
                f,
                "slot-machine match id `{id}` has no payout (there are {SLOT_MATCH_COUNT})"
            ),
        }
    }
}

impl Error for SlotMatchError {}

/// A winning match. The discriminants are the upstream `MATCH_*` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum SlotMatch {
    /// A cherry in the center of the first reel (`MATCH_CHERRY`).
    Cherry,
    /// A cherry in the top or bottom of the first reel (`MATCH_TOPBOT_CHERRY`).
    TopBottomCherry,
    /// The replay symbol (`MATCH_REPLAY`).
    Replay,
    /// The Lotad symbol (`MATCH_LOTAD`).
    Lotad,
    /// The Azurill symbol (`MATCH_AZURILL`).
    Azurill,
    /// The power symbol (`MATCH_POWER`).
    Power,
    /// The first two sevens share a color and the last is the other color
    /// (`MATCH_MIXED_7`).
    MixedSeven,
    /// Red sevens (`MATCH_RED_7`).
    RedSeven,
    /// Blue sevens (`MATCH_BLUE_7`).
    BlueSeven,
}

impl SlotMatch {
    /// Every match, in upstream order.
    pub const ALL: [Self; SLOT_MATCH_COUNT] = [
        Self::Cherry,
        Self::TopBottomCherry,
        Self::Replay,
        Self::Lotad,
        Self::Azurill,
        Self::Power,
        Self::MixedSeven,
        Self::RedSeven,
        Self::BlueSeven,
    ];

    /// Validates a raw `MATCH_*` value.
    ///
    /// # Errors
    ///
    /// Returns [`SlotMatchError::UnknownMatch`] when `value` is not below
    /// [`SLOT_MATCH_COUNT`], which includes the no-match id.
    pub const fn new(value: u8) -> Result<Self, SlotMatchError> {
        if (value as usize) < SLOT_MATCH_COUNT {
            Ok(Self::ALL[value as usize])
        } else {
            Err(SlotMatchError::UnknownMatch(value))
        }
    }

    /// Returns the raw upstream value.
    #[must_use]
    pub const fn index(self) -> u8 {
        self as u8
    }
}

/// `sSlotPayouts`, indexed by [`SlotMatch`].
const SLOT_PAYOUTS: [u16; SLOT_MATCH_COUNT] = [2, 4, 0, 6, 12, 3, 90, 300, 300];

/// Returns the payout for `slot_match`. The replay match pays `0`.
#[must_use]
pub const fn slot_match_payout(slot_match: SlotMatch) -> u16 {
    SLOT_PAYOUTS[slot_match.index() as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The payouts in `MATCH_*` order, read independently of `SLOT_PAYOUTS`
    /// from the designated initializers of `sSlotPayouts` in
    /// `src/slot_machine.c`.
    const EXPECTED_PAYOUTS: [u16; SLOT_MATCH_COUNT] = [2, 4, 0, 6, 12, 3, 90, 300, 300];

    #[test]
    fn every_payout_is_pinned_in_match_id_order() {
        for (value, expected) in (0..).zip(EXPECTED_PAYOUTS) {
            let slot_match = SlotMatch::new(value).unwrap();
            assert_eq!(slot_match_payout(slot_match), expected, "{slot_match:?}");
        }
    }

    #[test]
    fn each_match_pays_its_own_amount() {
        assert_eq!(slot_match_payout(SlotMatch::Cherry), 2);
        assert_eq!(slot_match_payout(SlotMatch::TopBottomCherry), 4);
        assert_eq!(slot_match_payout(SlotMatch::Replay), 0);
        assert_eq!(slot_match_payout(SlotMatch::Lotad), 6);
        assert_eq!(slot_match_payout(SlotMatch::Azurill), 12);
        assert_eq!(slot_match_payout(SlotMatch::Power), 3);
        assert_eq!(slot_match_payout(SlotMatch::MixedSeven), 90);
        assert_eq!(slot_match_payout(SlotMatch::RedSeven), 300);
        assert_eq!(slot_match_payout(SlotMatch::BlueSeven), 300);
    }

    #[test]
    fn match_values_round_trip() {
        for (value, slot_match) in (0..).zip(SlotMatch::ALL) {
            assert_eq!(slot_match.index(), value);
            assert_eq!(SlotMatch::new(value), Ok(slot_match));
        }
    }

    #[test]
    fn the_no_match_id_and_any_larger_id_are_refused() {
        let none = u8::try_from(SLOT_MATCH_COUNT).unwrap();
        assert_eq!(none, 9);
        assert_eq!(SlotMatch::new(none), Err(SlotMatchError::UnknownMatch(9)));
        assert_eq!(SlotMatch::new(10), Err(SlotMatchError::UnknownMatch(10)));
        assert_eq!(
            SlotMatch::new(u8::MAX),
            Err(SlotMatchError::UnknownMatch(u8::MAX))
        );
    }

    #[test]
    fn the_error_names_the_offending_id() {
        assert!(SlotMatchError::UnknownMatch(9).to_string().contains('9'));
        assert!(SlotMatchError::UnknownMatch(200)
            .to_string()
            .contains("200"));
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

    /// The text between `start` and the next `end` after it.
    fn between<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
        let from = source
            .find(start)
            .unwrap_or_else(|| panic!("`{start}` not found"))
            + start.len();
        let length = source[from..]
            .find(end)
            .unwrap_or_else(|| panic!("`{end}` not found after `{start}`"));
        &source[from..from + length]
    }

    #[test]
    #[ignore = "reads the pinned upstream checkout; run ./init.sh first"]
    fn upstream_source_matches_the_table() {
        let source = upstream_file("src/slot_machine.c");

        let names: Vec<&str> = between(&source, "enum {\n    MATCH_CHERRY,", "};")
            .lines()
            .map(|line| line.split("//").next().unwrap())
            .flat_map(|line| line.split(','))
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .collect();
        let mut matches = vec!["MATCH_CHERRY"];
        matches.extend(names);
        assert_eq!(matches.pop(), Some("MATCH_NONE"));
        assert_eq!(matches.len(), SLOT_MATCH_COUNT);

        let table = between(&source, "static const u16 sSlotPayouts[] = {", "};");
        let mut parsed = [None; SLOT_MATCH_COUNT];
        for line in table.lines().filter(|line| line.contains('=')) {
            let (name, value) = line.split_once('=').unwrap();
            let name = name.trim().trim_start_matches('[').trim_end_matches(']');
            let index = matches
                .iter()
                .position(|candidate| *candidate == name)
                .unwrap_or_else(|| panic!("unknown match `{name}`"));
            let value: u16 = value.trim().trim_end_matches(',').parse().unwrap();
            assert!(parsed[index].replace(value).is_none(), "{name} repeated");
        }
        for slot_match in SlotMatch::ALL {
            assert_eq!(
                parsed[usize::from(slot_match.index())],
                Some(slot_match_payout(slot_match)),
                "{slot_match:?}"
            );
        }
    }
}
