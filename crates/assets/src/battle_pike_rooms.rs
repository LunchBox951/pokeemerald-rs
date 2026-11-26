//! The Battle Pike room-type hint categories and the Queen-healing counts.
//!
//! Source: `sRoomTypeHints` (`src/battle_pike.c:512-522`), read by
//! `GetRoomTypeHint` (`:1382-1385`) with the room type saved as
//! `pikeHintedRoomType`; and `sNumMonsToHealBeforePikeQueen`
//! (`src/battle_pike.c:524-532`), read by `HealSomeMonsBeforePikeQueen`
//! (`:1538-1544`). The ids come from `include/constants/battle_pike.h`.
//!
//! Several room types share one hint category, so [`PikeHint`] is its own
//! type and [`room_hint`] maps many room types onto it.
//!
//! The heal table is indexed by the saved `pikeHintedRoomIndex` (its row) and
//! by `gSpecialVar_0x8007` (its column), which the Queen scripts set to one
//! of the three room positions, `PIKE_ROOM_LEFT`, `PIKE_ROOM_CENTER` or
//! `PIKE_ROOM_RIGHT` (`data/scripts/battle_pike.inc:73-83`). The six rows are
//! only reached before the Queen: `SetHintedRoom` draws the row from six
//! values then and from three otherwise (`src/battle_pike.c:1343,1348`).
//! Each row gives, per room position, how many Pokémon `TryHealMons`
//! is asked to heal; no probability model is implied.
//!
//! Data only: room selection, the hint dialogue, party healing, RNG draws,
//! the speeches and the Queen battle are not ported here.

use std::error::Error;
use std::fmt;

/// The number of room types (`NUM_PIKE_ROOM_TYPES`).
pub const PIKE_ROOM_TYPES: usize = 9;

/// The number of rows in the Queen-healing table.
pub const PIKE_QUEEN_HEAL_ROWS: usize = 6;

/// The number of room positions, the columns of the Queen-healing table.
pub const PIKE_ROOM_POSITIONS: usize = 3;

/// A failed lookup in the Battle Pike room tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PikeRoomError {
    /// The room-type id is not below [`PIKE_ROOM_TYPES`].
    InvalidRoomId(usize),
    /// The hinted room index is not below [`PIKE_QUEEN_HEAL_ROWS`].
    InvalidHintedRoomIndex(usize),
    /// The room position is not below [`PIKE_ROOM_POSITIONS`].
    InvalidRoomPosition(usize),
}

impl fmt::Display for PikeRoomError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRoomId(id) => write!(
                f,
                "Battle Pike room type `{id}` is out of range (there are {PIKE_ROOM_TYPES})"
            ),
            Self::InvalidHintedRoomIndex(index) => write!(
                f,
                "Battle Pike hinted room index `{index}` is out of range (there are {PIKE_QUEEN_HEAL_ROWS})"
            ),
            Self::InvalidRoomPosition(position) => write!(
                f,
                "Battle Pike room position `{position}` is out of range (there are {PIKE_ROOM_POSITIONS})"
            ),
        }
    }
}

impl Error for PikeRoomError {}

/// A Battle Pike room type (`PIKE_ROOM_*`, `include/constants/battle_pike.h:6-14`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PikeRoomType {
    /// `PIKE_ROOM_SINGLE_BATTLE`, `0`.
    SingleBattle = 0,
    /// `PIKE_ROOM_HEAL_FULL`, `1`.
    HealFull = 1,
    /// `PIKE_ROOM_NPC`, `2`.
    Npc = 2,
    /// `PIKE_ROOM_STATUS`, `3`.
    Status = 3,
    /// `PIKE_ROOM_HEAL_PART`, `4`.
    HealPart = 4,
    /// `PIKE_ROOM_WILD_MONS`, `5`.
    WildMons = 5,
    /// `PIKE_ROOM_HARD_BATTLE`, `6`.
    HardBattle = 6,
    /// `PIKE_ROOM_DOUBLE_BATTLE`, `7`.
    DoubleBattle = 7,
    /// `PIKE_ROOM_BRAIN`, `8`.
    Brain = 8,
}

impl PikeRoomType {
    /// Every room type, in id order.
    pub const ALL: [Self; PIKE_ROOM_TYPES] = [
        Self::SingleBattle,
        Self::HealFull,
        Self::Npc,
        Self::Status,
        Self::HealPart,
        Self::WildMons,
        Self::HardBattle,
        Self::DoubleBattle,
        Self::Brain,
    ];

    /// Returns the room type with upstream id `id`.
    ///
    /// # Errors
    ///
    /// Returns [`PikeRoomError::InvalidRoomId`] when `id` is
    /// [`PIKE_ROOM_TYPES`] or more.
    pub fn from_id(id: usize) -> Result<Self, PikeRoomError> {
        Self::ALL
            .get(id)
            .copied()
            .ok_or(PikeRoomError::InvalidRoomId(id))
    }

    /// Returns the upstream id (`PIKE_ROOM_*`).
    #[must_use]
    pub const fn id(self) -> u8 {
        self as u8
    }

    /// Returns the hint category this room type gives.
    #[must_use]
    pub const fn hint(self) -> PikeHint {
        ROOM_TYPE_HINTS[self as usize]
    }
}

/// A Battle Pike hint category (`PIKE_HINT_*`,
/// `include/constants/battle_pike.h:21-25`). It is an id; the hint dialogue
/// is not part of this type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PikeHint {
    /// `PIKE_HINT_NOSTALGIA`, `0`.
    Nostalgia = 0,
    /// `PIKE_HINT_WHISPERING`, `1`.
    Whispering = 1,
    /// `PIKE_HINT_POKEMON`, `2`.
    Pokemon = 2,
    /// `PIKE_HINT_PEOPLE`, `3`.
    People = 3,
    /// `PIKE_HINT_BRAIN`, `4`.
    Brain = 4,
}

impl PikeHint {
    /// Returns the upstream id (`PIKE_HINT_*`).
    #[must_use]
    pub const fn id(self) -> u8 {
        self as u8
    }
}

/// `sRoomTypeHints`, in room-type id order.
static ROOM_TYPE_HINTS: [PikeHint; PIKE_ROOM_TYPES] = [
    PikeHint::People,     // PIKE_ROOM_SINGLE_BATTLE
    PikeHint::People,     // PIKE_ROOM_HEAL_FULL
    PikeHint::Whispering, // PIKE_ROOM_NPC
    PikeHint::Nostalgia,  // PIKE_ROOM_STATUS
    PikeHint::Nostalgia,  // PIKE_ROOM_HEAL_PART
    PikeHint::Pokemon,    // PIKE_ROOM_WILD_MONS
    PikeHint::Pokemon,    // PIKE_ROOM_HARD_BATTLE
    PikeHint::Whispering, // PIKE_ROOM_DOUBLE_BATTLE
    PikeHint::Brain,      // PIKE_ROOM_BRAIN
];

/// `sNumMonsToHealBeforePikeQueen`: rows by hinted room index, columns by
/// room position.
static QUEEN_HEAL_COUNTS: [[u8; PIKE_ROOM_POSITIONS]; PIKE_QUEEN_HEAL_ROWS] = [
    [2, 1, 0],
    [2, 0, 1],
    [1, 2, 0],
    [1, 0, 2],
    [0, 2, 1],
    [0, 1, 2],
];

/// Returns the hint category of the room type with upstream id `room_id`.
///
/// # Errors
///
/// Returns [`PikeRoomError::InvalidRoomId`] when `room_id` is
/// [`PIKE_ROOM_TYPES`] or more.
pub fn room_hint(room_id: usize) -> Result<PikeHint, PikeRoomError> {
    PikeRoomType::from_id(room_id).map(PikeRoomType::hint)
}

/// Returns how many Pokémon the Queen's pre-fight healing is asked to heal
/// for the saved `hinted_room_index` and the `room_position` of the room.
///
/// # Errors
///
/// Returns [`PikeRoomError::InvalidHintedRoomIndex`] when `hinted_room_index`
/// is [`PIKE_QUEEN_HEAL_ROWS`] or more, and
/// [`PikeRoomError::InvalidRoomPosition`] when `room_position` is
/// [`PIKE_ROOM_POSITIONS`] or more; the hinted room index is checked first.
pub fn queen_heal_count(
    hinted_room_index: usize,
    room_position: usize,
) -> Result<u8, PikeRoomError> {
    QUEEN_HEAL_COUNTS
        .get(hinted_room_index)
        .ok_or(PikeRoomError::InvalidHintedRoomIndex(hinted_room_index))?
        .get(room_position)
        .copied()
        .ok_or(PikeRoomError::InvalidRoomPosition(room_position))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `PIKE_HINT_*` values by `PIKE_ROOM_*` value, laid out from
    /// `include/constants/battle_pike.h`: rooms 0 to 8, hints 0 Nostalgia,
    /// 1 Whispering, 2 Pokemon, 3 People, 4 Brain.
    const EXPECTED_HINTS: [u8; 9] = [3, 3, 1, 0, 0, 2, 2, 1, 4];

    /// The heal counts, listed column by column (left, center, right) and
    /// transposed to rows below, so the literal is not a copy of the table.
    const EXPECTED_HEAL_COLUMNS: [[u8; 6]; 3] =
        [[2, 2, 1, 1, 0, 0], [1, 0, 2, 0, 2, 1], [0, 1, 0, 2, 1, 2]];

    #[test]
    fn hints_match_upstream_for_every_room_id() {
        let hints: Vec<u8> = (0..9).map(|id| room_hint(id).unwrap().id()).collect();
        assert_eq!(hints, EXPECTED_HINTS);
    }

    #[test]
    fn typed_ids_match_the_header_values() {
        for (id, room) in PikeRoomType::ALL.into_iter().enumerate() {
            assert_eq!(usize::from(room.id()), id);
            assert_eq!(PikeRoomType::from_id(id), Ok(room));
            assert_eq!(room.hint(), room_hint(id).unwrap());
        }
        assert_eq!(PikeRoomType::Brain.id(), 8);
        assert_eq!(PikeHint::Brain.id(), 4);
        assert_eq!(PikeHint::Nostalgia.id(), 0);
    }

    #[test]
    fn hint_categories_are_shared_between_room_types() {
        assert_eq!(PikeRoomType::SingleBattle.hint(), PikeHint::People);
        assert_eq!(PikeRoomType::HealFull.hint(), PikeHint::People);
        assert_eq!(PikeRoomType::Npc.hint(), PikeRoomType::DoubleBattle.hint());
        assert_eq!(PikeRoomType::Status.hint(), PikeRoomType::HealPart.hint());
        assert_eq!(
            PikeRoomType::WildMons.hint(),
            PikeRoomType::HardBattle.hint()
        );
        assert_ne!(
            PikeRoomType::Brain.hint(),
            PikeRoomType::SingleBattle.hint()
        );
    }

    #[test]
    fn heal_counts_match_upstream_in_order() {
        let mut counts = Vec::new();
        for row in 0..6 {
            for column in 0..3 {
                counts.push(queen_heal_count(row, column).unwrap());
            }
        }
        let expected: Vec<u8> = (0..6)
            .flat_map(|row| (0..3).map(move |column| EXPECTED_HEAL_COLUMNS[column][row]))
            .collect();
        assert_eq!(counts.len(), 18);
        assert_eq!(counts, expected);
    }

    #[test]
    fn every_heal_row_is_a_permutation_of_zero_one_two() {
        for row in 0..6 {
            let mut counts: Vec<u8> = (0..3).map(|c| queen_heal_count(row, c).unwrap()).collect();
            counts.sort_unstable();
            assert_eq!(counts, [0, 1, 2], "row {row}");
        }
    }

    #[test]
    fn invalid_room_ids_are_refused() {
        assert_eq!(room_hint(9), Err(PikeRoomError::InvalidRoomId(9)));
        assert_eq!(
            room_hint(usize::MAX),
            Err(PikeRoomError::InvalidRoomId(usize::MAX))
        );
        assert_eq!(
            PikeRoomType::from_id(14),
            Err(PikeRoomError::InvalidRoomId(14))
        );
    }

    #[test]
    fn out_of_range_rows_and_columns_are_refused() {
        assert_eq!(
            queen_heal_count(6, 0),
            Err(PikeRoomError::InvalidHintedRoomIndex(6))
        );
        assert_eq!(
            queen_heal_count(0, 3),
            Err(PikeRoomError::InvalidRoomPosition(3))
        );
        assert_eq!(
            queen_heal_count(6, 3),
            Err(PikeRoomError::InvalidHintedRoomIndex(6))
        );
        assert_eq!(
            queen_heal_count(usize::MAX, usize::MAX),
            Err(PikeRoomError::InvalidHintedRoomIndex(usize::MAX))
        );
    }

    #[test]
    fn errors_name_the_rejected_value() {
        assert_eq!(
            PikeRoomError::InvalidRoomId(9).to_string(),
            "Battle Pike room type `9` is out of range (there are 9)"
        );
        assert_eq!(
            PikeRoomError::InvalidHintedRoomIndex(7).to_string(),
            "Battle Pike hinted room index `7` is out of range (there are 6)"
        );
        assert_eq!(
            PikeRoomError::InvalidRoomPosition(4).to_string(),
            "Battle Pike room position `4` is out of range (there are 3)"
        );
    }

    /// Reads a file of the checked-out upstream tree, failing loudly when it
    /// is missing (`./init.sh` fetches it).
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

    /// Reads `#define NAME <integer>` lines into a lookup.
    fn defines(text: &str) -> std::collections::HashMap<String, usize> {
        text.lines()
            .filter_map(|line| {
                let mut words = line.split_whitespace();
                (words.next()? == "#define").then_some(())?;
                let name = words.next()?.to_owned();
                let value = words.next()?.parse().ok()?;
                Some((name, value))
            })
            .collect()
    }

    /// Returns the lines between `static const u8 <header>` and its `};`,
    /// trimmed and without blanks.
    fn array_body<'a>(text: &'a str, header: &str) -> Vec<&'a str> {
        let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
        lines
            .find(|line| line.starts_with(&format!("static const u8 {header}")))
            .unwrap_or_else(|| panic!("`{header}` not found"));
        let mut body = Vec::new();
        for line in lines {
            if line == "{" {
                continue;
            }
            if line == "};" {
                return body;
            }
            body.push(line);
        }
        panic!("`{header}` has no closing `}};`");
    }

    #[test]
    #[ignore = "reads the pinned upstream checkout; run ./init.sh first"]
    fn upstream_source_matches_the_tables() {
        let constants = defines(&upstream_file("include/constants/battle_pike.h"));
        assert_eq!(constants["NUM_PIKE_ROOM_TYPES"], PIKE_ROOM_TYPES);
        let source = upstream_file("src/battle_pike.c");

        let hints = array_body(&source, "sRoomTypeHints[] =");
        assert_eq!(hints.len(), PIKE_ROOM_TYPES);
        for (id, line) in hints.iter().enumerate() {
            let (hint, room) = line
                .split_once("//")
                .unwrap_or_else(|| panic!("unexpected hint line `{line}`"));
            let room = room.trim();
            assert_eq!(constants[room], id, "{room} is entry {id}");
            assert_eq!(
                room_hint(id).unwrap().id(),
                u8::try_from(constants[hint.trim().trim_end_matches(',')]).unwrap(),
                "{room}"
            );
            assert_eq!(
                usize::from(PikeRoomType::from_id(constants[room]).unwrap().id()),
                id
            );
        }

        let rows = array_body(&source, "sNumMonsToHealBeforePikeQueen[][3] =");
        assert_eq!(rows.len(), PIKE_QUEEN_HEAL_ROWS);
        for (row, line) in rows.iter().enumerate() {
            let counts: Vec<u8> = line
                .trim_start_matches('{')
                .trim_end_matches(',')
                .trim_end_matches('}')
                .split(", ")
                .map(|count| count.parse().expect("numeric count"))
                .collect();
            assert_eq!(counts.len(), PIKE_ROOM_POSITIONS, "row {row}");
            for (column, count) in counts.iter().enumerate() {
                assert_eq!(queen_heal_count(row, column), Ok(*count), "{row},{column}");
            }
        }
    }
}
