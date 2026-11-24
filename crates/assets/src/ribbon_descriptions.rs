//! The two description lines the ribbon summary screen prints for each of the
//! 25 normal ribbons.
//!
//! Source: `src/data/text/ribbon_descriptions.h` (`gRibbonDescriptionPointers`,
//! indexed by the `CHAMPION_RIBBON` through `EFFORT_RIBBON` IDs of
//! `include/constants/pokemon.h`), read by
//! `src/pokenav_ribbons_summary.c` `PrintRibbonNameAndDescription` for every
//! ribbon below `FIRST_GIFT_RIBBON`. The gift ribbons (`MARINE_RIBBON` and
//! later) have their own text and are not part of this table.

use std::error::Error;
use std::fmt;

/// The number of normal ribbons (`FIRST_GIFT_RIBBON`): `CHAMPION_RIBBON`
/// through `EFFORT_RIBBON`.
pub const NORMAL_RIBBON_COUNT: usize = 25;

/// A failed decoding of a normal-ribbon ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RibbonError {
    /// The value is not a normal-ribbon ID below [`NORMAL_RIBBON_COUNT`].
    UnknownNormalRibbon(u8),
}

impl fmt::Display for RibbonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownNormalRibbon(id) => write!(f, "unknown normal ribbon id `{id}`"),
        }
    }
}

impl Error for RibbonError {}

/// A normal-ribbon identifier. The named values are the upstream `*_RIBBON` IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NormalRibbonId(u8);

impl NormalRibbonId {
    /// Validates a raw ribbon ID.
    ///
    /// # Errors
    ///
    /// Returns [`RibbonError::UnknownNormalRibbon`] when `id` is
    /// [`NORMAL_RIBBON_COUNT`] or more, which includes every gift ribbon.
    pub const fn new(id: u8) -> Result<Self, RibbonError> {
        if (id as usize) < NORMAL_RIBBON_COUNT {
            Ok(Self(id))
        } else {
            Err(RibbonError::UnknownNormalRibbon(id))
        }
    }

    /// Returns the raw upstream ID.
    #[must_use]
    pub const fn index(self) -> u8 {
        self.0
    }
}

/// The two lines printed under a ribbon (`gRibbonDescriptionPointers[id][0]`
/// and `[1]`). Each line is one text-window row, so neither holds a newline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RibbonDescription {
    /// The first printed line, at the window's top row.
    pub line1: &'static str,
    /// The second printed line, 16 pixels below the first.
    pub line2: &'static str,
}

/// Lookup over the normal-ribbon descriptions.
#[derive(Debug, Clone, Copy)]
pub struct RibbonDescriptionTable {
    rows: &'static [RibbonDescription; NORMAL_RIBBON_COUNT],
}

impl RibbonDescriptionTable {
    /// Creates a handle on the canonical table.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            rows: &DESCRIPTIONS,
        }
    }

    /// Returns the number of rows.
    #[must_use]
    pub const fn len(&self) -> usize {
        NORMAL_RIBBON_COUNT
    }

    /// Returns `false`; the table is never empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }

    /// Returns the description for `id`.
    #[must_use]
    pub const fn get(&self, id: NormalRibbonId) -> &'static RibbonDescription {
        &self.rows[id.0 as usize]
    }

    /// Returns the description for a raw ribbon ID.
    ///
    /// # Errors
    ///
    /// Returns [`RibbonError::UnknownNormalRibbon`] when `id` is not a normal
    /// ribbon.
    pub const fn get_raw(&self, id: u8) -> Result<&'static RibbonDescription, RibbonError> {
        match NormalRibbonId::new(id) {
            Ok(id) => Ok(self.get(id)),
            Err(error) => Err(error),
        }
    }

    /// Iterates in ascending [`NormalRibbonId`] order.
    pub fn iter(&self) -> impl Iterator<Item = &RibbonDescription> {
        self.rows.iter()
    }
}

impl Default for RibbonDescriptionTable {
    fn default() -> Self {
        Self::new()
    }
}

const COOL_CONTEST: &str = "COOL CONTEST";
const BEAUTY_CONTEST: &str = "BEAUTY CONTEST";
const CUTE_CONTEST: &str = "CUTE CONTEST";
const SMART_CONTEST: &str = "SMART CONTEST";
const TOUGH_CONTEST: &str = "TOUGH CONTEST";
const NORMAL_RANK: &str = "Normal Rank winner!";
const SUPER_RANK: &str = "Super Rank winner!";
const HYPER_RANK: &str = "Hyper Rank winner!";
const MASTER_RANK: &str = "Master Rank winner!";

macro_rules! define_normal_ribbons {
    ($($name:ident = $id:literal => ($line1:expr, $line2:expr)),+ $(,)?) => {
        impl NormalRibbonId {
            $(
                #[doc = concat!("`", stringify!($name), "`.")]
                pub const $name: Self = Self($id);
            )+
        }

        const DESCRIPTIONS: [RibbonDescription; NORMAL_RIBBON_COUNT] = [
            $(RibbonDescription { line1: $line1, line2: $line2 }),+
        ];

        const _: () = {
            let ids: [usize; NORMAL_RIBBON_COUNT] = [$($id),+];
            let mut index = 0;
            while index < NORMAL_RIBBON_COUNT {
                assert!(ids[index] == index);
                index += 1;
            }
        };
    };
}

#[rustfmt::skip]
define_normal_ribbons! {
    CHAMPION_RIBBON = 0 => ("CHAMPION-beating, HALL", "OF FAME Member RIBBON"),
    COOL_RIBBON_NORMAL = 1 => (COOL_CONTEST, NORMAL_RANK),
    COOL_RIBBON_SUPER = 2 => (COOL_CONTEST, SUPER_RANK),
    COOL_RIBBON_HYPER = 3 => (COOL_CONTEST, HYPER_RANK),
    COOL_RIBBON_MASTER = 4 => (COOL_CONTEST, MASTER_RANK),
    BEAUTY_RIBBON_NORMAL = 5 => (BEAUTY_CONTEST, NORMAL_RANK),
    BEAUTY_RIBBON_SUPER = 6 => (BEAUTY_CONTEST, SUPER_RANK),
    BEAUTY_RIBBON_HYPER = 7 => (BEAUTY_CONTEST, HYPER_RANK),
    BEAUTY_RIBBON_MASTER = 8 => (BEAUTY_CONTEST, MASTER_RANK),
    CUTE_RIBBON_NORMAL = 9 => (CUTE_CONTEST, NORMAL_RANK),
    CUTE_RIBBON_SUPER = 10 => (CUTE_CONTEST, SUPER_RANK),
    CUTE_RIBBON_HYPER = 11 => (CUTE_CONTEST, HYPER_RANK),
    CUTE_RIBBON_MASTER = 12 => (CUTE_CONTEST, MASTER_RANK),
    SMART_RIBBON_NORMAL = 13 => (SMART_CONTEST, NORMAL_RANK),
    SMART_RIBBON_SUPER = 14 => (SMART_CONTEST, SUPER_RANK),
    SMART_RIBBON_HYPER = 15 => (SMART_CONTEST, HYPER_RANK),
    SMART_RIBBON_MASTER = 16 => (SMART_CONTEST, MASTER_RANK),
    TOUGH_RIBBON_NORMAL = 17 => (TOUGH_CONTEST, NORMAL_RANK),
    TOUGH_RIBBON_SUPER = 18 => (TOUGH_CONTEST, SUPER_RANK),
    TOUGH_RIBBON_HYPER = 19 => (TOUGH_CONTEST, HYPER_RANK),
    TOUGH_RIBBON_MASTER = 20 => (TOUGH_CONTEST, MASTER_RANK),
    WINNING_RIBBON = 21 => ("For clearing LV50", "at the BATTLE TOWER."),
    VICTORY_RIBBON = 22 => ("For clearing Open Level", "at the BATTLE TOWER."),
    ARTIST_RIBBON = 23 => ("RIBBON for being chosen", "as a super sketch model."),
    EFFORT_RIBBON = 24 => ("RIBBON awarded for", "being a hard worker."),
}

#[cfg(test)]
mod tests {
    use super::{
        NormalRibbonId, RibbonDescription, RibbonDescriptionTable, RibbonError, NORMAL_RIBBON_COUNT,
    };

    fn line_pair(id: u8) -> (&'static str, &'static str) {
        let row = RibbonDescriptionTable::new()
            .get_raw(id)
            .expect("normal ribbon");
        (row.line1, row.line2)
    }

    #[test]
    fn the_full_table_matches_the_independent_expectation() {
        let expected: [(&str, &str); NORMAL_RIBBON_COUNT] = [
            ("CHAMPION-beating, HALL", "OF FAME Member RIBBON"),
            ("COOL CONTEST", "Normal Rank winner!"),
            ("COOL CONTEST", "Super Rank winner!"),
            ("COOL CONTEST", "Hyper Rank winner!"),
            ("COOL CONTEST", "Master Rank winner!"),
            ("BEAUTY CONTEST", "Normal Rank winner!"),
            ("BEAUTY CONTEST", "Super Rank winner!"),
            ("BEAUTY CONTEST", "Hyper Rank winner!"),
            ("BEAUTY CONTEST", "Master Rank winner!"),
            ("CUTE CONTEST", "Normal Rank winner!"),
            ("CUTE CONTEST", "Super Rank winner!"),
            ("CUTE CONTEST", "Hyper Rank winner!"),
            ("CUTE CONTEST", "Master Rank winner!"),
            ("SMART CONTEST", "Normal Rank winner!"),
            ("SMART CONTEST", "Super Rank winner!"),
            ("SMART CONTEST", "Hyper Rank winner!"),
            ("SMART CONTEST", "Master Rank winner!"),
            ("TOUGH CONTEST", "Normal Rank winner!"),
            ("TOUGH CONTEST", "Super Rank winner!"),
            ("TOUGH CONTEST", "Hyper Rank winner!"),
            ("TOUGH CONTEST", "Master Rank winner!"),
            ("For clearing LV50", "at the BATTLE TOWER."),
            ("For clearing Open Level", "at the BATTLE TOWER."),
            ("RIBBON for being chosen", "as a super sketch model."),
            ("RIBBON awarded for", "being a hard worker."),
        ];
        let table = RibbonDescriptionTable::new();
        assert_eq!(table.len(), NORMAL_RIBBON_COUNT);
        assert_eq!(table.iter().count(), NORMAL_RIBBON_COUNT);
        assert!(!table.is_empty());
        for (id, pair) in (0..).zip(expected) {
            assert_eq!(line_pair(id), pair, "ribbon {id}");
            let named = NormalRibbonId::new(id).expect("normal ribbon");
            assert_eq!(table.get(named).line1, pair.0, "ribbon {id}");
            assert_eq!(named.index(), id);
        }
    }

    #[test]
    fn the_named_ids_sit_at_the_upstream_values() {
        for (id, value) in [
            (NormalRibbonId::CHAMPION_RIBBON, 0),
            (NormalRibbonId::COOL_RIBBON_NORMAL, 1),
            (NormalRibbonId::BEAUTY_RIBBON_NORMAL, 5),
            (NormalRibbonId::CUTE_RIBBON_NORMAL, 9),
            (NormalRibbonId::SMART_RIBBON_NORMAL, 13),
            (NormalRibbonId::TOUGH_RIBBON_NORMAL, 17),
            (NormalRibbonId::WINNING_RIBBON, 21),
            (NormalRibbonId::VICTORY_RIBBON, 22),
            (NormalRibbonId::ARTIST_RIBBON, 23),
            (NormalRibbonId::EFFORT_RIBBON, 24),
        ] {
            assert_eq!(id.index(), value);
        }
    }

    #[test]
    fn each_contest_category_runs_normal_super_hyper_master() {
        let ranks = [
            "Normal Rank winner!",
            "Super Rank winner!",
            "Hyper Rank winner!",
            "Master Rank winner!",
        ];
        let categories = [
            (1, "COOL CONTEST"),
            (5, "BEAUTY CONTEST"),
            (9, "CUTE CONTEST"),
            (13, "SMART CONTEST"),
            (17, "TOUGH CONTEST"),
        ];
        for (first, category) in categories {
            for (offset, rank) in (0..).zip(ranks) {
                assert_eq!(
                    line_pair(first + offset),
                    (category, rank),
                    "{category} rank {offset}"
                );
            }
        }
    }

    #[test]
    fn the_rank_boundaries_do_not_bleed_into_the_next_category() {
        // Master is the last rank of a category; the next ID is the next
        // category's Normal rank.
        assert_eq!(line_pair(4), ("COOL CONTEST", "Master Rank winner!"));
        assert_eq!(line_pair(5), ("BEAUTY CONTEST", "Normal Rank winner!"));
        assert_eq!(line_pair(16), ("SMART CONTEST", "Master Rank winner!"));
        assert_eq!(line_pair(17), ("TOUGH CONTEST", "Normal Rank winner!"));
        // The champion ribbon precedes the first contest ribbon, and the last
        // contest ribbon precedes the Battle Tower ribbons.
        assert_eq!(line_pair(0).0, "CHAMPION-beating, HALL");
        assert_eq!(line_pair(20), ("TOUGH CONTEST", "Master Rank winner!"));
        assert_eq!(line_pair(21).0, "For clearing LV50");
    }

    #[test]
    fn no_line_holds_a_newline_or_is_empty() {
        for RibbonDescription { line1, line2 } in RibbonDescriptionTable::new().iter() {
            for line in [line1, line2] {
                assert!(!line.is_empty());
                assert!(!line.contains('\n'));
            }
        }
    }

    #[test]
    fn ids_at_or_past_the_gift_ribbons_are_rejected() {
        let table = RibbonDescriptionTable::new();
        // 25 is MARINE_RIBBON, FIRST_GIFT_RIBBON; 31 is WORLD_RIBBON.
        for id in [25, 26, 31, 32, 255] {
            assert_eq!(
                NormalRibbonId::new(id),
                Err(RibbonError::UnknownNormalRibbon(id))
            );
            assert_eq!(
                table.get_raw(id).map(|_| ()),
                Err(RibbonError::UnknownNormalRibbon(id))
            );
        }
        assert!(NormalRibbonId::new(24).is_ok());
        assert_eq!(
            RibbonError::UnknownNormalRibbon(25).to_string(),
            "unknown normal ribbon id `25`"
        );
    }
}
