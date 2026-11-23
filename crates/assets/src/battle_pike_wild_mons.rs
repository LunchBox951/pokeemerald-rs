//! The Battle Pike wild Pokémon rosters: for each of the two level modes,
//! four headers of three wild Pokémon records.
//!
//! Source: `sLvl50_Mons1` to `sLvl50_Mons4` and `sLvl50Mons`, and
//! `sLvlOpen_Mons1` to `sLvlOpen_Mons4` and `sLvlOpenMons`
//! (`src/battle_pike.c:93-265`), selected by the `sWildMons` table
//! (`FRONTIER_LVL_50` and `FRONTIER_LVL_OPEN`). Each record is a
//! `struct PikeWildMon` (`src/battle_pike.c:35-40`): a species, a
//! `levelDelta`, and four ordered moves. Headers and slots are indexed from
//! `0`, in the order the pointer tables and record lists give.
//!
//! `levelDelta` is a decrement, never a level. `TryGenerateBattlePikeWildMon`
//! (`src/battle_pike.c:1105-1151`) subtracts it from the facility level. In
//! Level 50 mode that is `FRONTIER_MAX_LEVEL_50`. In Open Level mode it is
//! the highest level in the player's party, and the wild level never falls
//! below `FRONTIER_MIN_LEVEL_OPEN`: a party below that floor meets the floor
//! with nothing subtracted, and any other result is raised to the floor after
//! the subtraction (`:1116-1128`). A highest level of 60 with a decrement of
//! 4 therefore gives 60, not 56. It is a [`PikeLevelDecrement`] with no
//! conversion to a level, and no accessor named for one.
//!
//! Data only: wild-Pokémon generation, its RNG draws, the level arithmetic,
//! experience and ability assignment, the Keen Eye and Intimidate encounter
//! check, and the room layout and NPCs are not ported here.

use std::error::Error;
use std::fmt;

use crate::battle_moves::MoveId;
use crate::species::SpeciesId;

/// The number of headers in each level mode (`ARRAY_COUNT(sLvl50Mons)`).
pub const PIKE_WILD_HEADERS: usize = 4;

/// The number of records in each header.
pub const PIKE_WILD_MON_SLOTS: usize = 3;

/// The number of moves in a record (`MAX_MON_MOVES`).
pub const PIKE_WILD_MON_MOVES: usize = 4;

/// A failed lookup in the Battle Pike rosters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PikeWildMonError {
    /// The header is not below [`PIKE_WILD_HEADERS`].
    InvalidHeader(usize),
    /// The slot is not below [`PIKE_WILD_MON_SLOTS`].
    InvalidSlot(usize),
}

impl fmt::Display for PikeWildMonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidHeader(header) => write!(
                f,
                "Battle Pike header `{header}` is out of range (there are {PIKE_WILD_HEADERS})"
            ),
            Self::InvalidSlot(slot) => write!(
                f,
                "Battle Pike slot `{slot}` is out of range (there are {PIKE_WILD_MON_SLOTS})"
            ),
        }
    }
}

impl Error for PikeWildMonError {}

/// Which roster set a lookup reads (`FRONTIER_LVL_50` or `FRONTIER_LVL_OPEN`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PikeLevelMode {
    /// The Level 50 mode (`FRONTIER_LVL_50`, `0`).
    Level50,
    /// The Open Level mode (`FRONTIER_LVL_OPEN`, `1`).
    OpenLevel,
}

/// The amount a record subtracts from the facility level (`levelDelta`,
/// `src/battle_pike.c:1117-1133`).
///
/// This is not a level: it has no accessor named for one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PikeLevelDecrement(u8);

impl PikeLevelDecrement {
    /// Wraps a raw decrement.
    #[must_use]
    pub const fn new(decrement: u8) -> Self {
        Self(decrement)
    }

    /// Returns the amount to subtract from the facility level.
    #[must_use]
    pub const fn decrement(self) -> u8 {
        self.0
    }
}

/// One roster record (`struct PikeWildMon`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PikeWildMon {
    species: SpeciesId,
    level_decrement: PikeLevelDecrement,
    moves: [MoveId; PIKE_WILD_MON_MOVES],
}

impl PikeWildMon {
    const fn new(
        species: SpeciesId,
        level_decrement: u8,
        moves: [MoveId; PIKE_WILD_MON_MOVES],
    ) -> Self {
        Self {
            species,
            level_decrement: PikeLevelDecrement::new(level_decrement),
            moves,
        }
    }

    /// Returns the species (`species`).
    #[must_use]
    pub const fn species(&self) -> SpeciesId {
        self.species
    }

    /// Returns the decrement from the facility level (`levelDelta`).
    #[must_use]
    pub const fn level_decrement(&self) -> PikeLevelDecrement {
        self.level_decrement
    }

    /// Returns the four moves in slot order (`moves`).
    #[must_use]
    pub const fn moves(&self) -> [MoveId; PIKE_WILD_MON_MOVES] {
        self.moves
    }
}

/// Returns the roster record at `header` and `slot` in `mode`.
///
/// # Errors
///
/// Returns [`PikeWildMonError::InvalidHeader`] when `header` is
/// [`PIKE_WILD_HEADERS`] or more, and [`PikeWildMonError::InvalidSlot`] when
/// `slot` is [`PIKE_WILD_MON_SLOTS`] or more; the header is checked first.
pub fn pike_wild_mon(
    mode: PikeLevelMode,
    header: usize,
    slot: usize,
) -> Result<&'static PikeWildMon, PikeWildMonError> {
    pike_header(mode, header)?
        .get(slot)
        .ok_or(PikeWildMonError::InvalidSlot(slot))
}

/// Returns the three records of `header` in `mode`.
///
/// # Errors
///
/// Returns [`PikeWildMonError::InvalidHeader`] when `header` is
/// [`PIKE_WILD_HEADERS`] or more.
pub fn pike_header(
    mode: PikeLevelMode,
    header: usize,
) -> Result<&'static [PikeWildMon; PIKE_WILD_MON_SLOTS], PikeWildMonError> {
    roster(mode)
        .get(header)
        .ok_or(PikeWildMonError::InvalidHeader(header))
}

type Roster = [[PikeWildMon; PIKE_WILD_MON_SLOTS]; PIKE_WILD_HEADERS];

const fn roster(mode: PikeLevelMode) -> &'static Roster {
    match mode {
        PikeLevelMode::Level50 => &LEVEL_50_ROSTER,
        PikeLevelMode::OpenLevel => &OPEN_LEVEL_ROSTER,
    }
}

/// Builds a record from the part of each upstream name after the `SPECIES_`
/// and `MOVE_` prefixes.
macro_rules! pike_mon {
    ($species:ident, $decrement:literal, [$($move:ident),+ $(,)?]) => {
        PikeWildMon::new(SpeciesId::$species, $decrement, [$(MoveId::$move),+])
    };
}

/// `sLvl50_Mons1` to `sLvl50_Mons4`, in `sLvl50Mons` order.
#[rustfmt::skip]
static LEVEL_50_ROSTER: Roster = [
    [
        pike_mon!(SEVIPER, 4, [TOXIC, GLARE, BODY_SLAM, SLUDGE_BOMB]),
        pike_mon!(MILOTIC, 4, [TOXIC, HYPNOSIS, BODY_SLAM, SURF]),
        pike_mon!(DUSCLOPS, 5, [WILL_O_WISP, MEAN_LOOK, TOXIC, SHADOW_PUNCH]),
    ],
    [
        pike_mon!(SEVIPER, 4, [TOXIC, GLARE, BODY_SLAM, SLUDGE_BOMB]),
        pike_mon!(MILOTIC, 4, [TOXIC, HYPNOSIS, BODY_SLAM, SURF]),
        pike_mon!(ELECTRODE, 5, [EXPLOSION, SELF_DESTRUCT, THUNDER, TOXIC]),
    ],
    [
        pike_mon!(SEVIPER, 4, [TOXIC, GLARE, BODY_SLAM, SLUDGE_BOMB]),
        pike_mon!(MILOTIC, 4, [TOXIC, HYPNOSIS, BODY_SLAM, SURF]),
        pike_mon!(BRELOOM, 5, [SPORE, STUN_SPORE, POISON_POWDER, HIDDEN_POWER]),
    ],
    [
        pike_mon!(SEVIPER, 4, [TOXIC, GLARE, BODY_SLAM, SLUDGE_BOMB]),
        pike_mon!(MILOTIC, 4, [TOXIC, HYPNOSIS, BODY_SLAM, SURF]),
        pike_mon!(WOBBUFFET, 5, [COUNTER, MIRROR_COAT, SAFEGUARD, DESTINY_BOND]),
    ],
];

/// `sLvlOpen_Mons1` to `sLvlOpen_Mons4`, in `sLvlOpenMons` order.
#[rustfmt::skip]
static OPEN_LEVEL_ROSTER: Roster = [
    [
        pike_mon!(SEVIPER, 4, [TOXIC, GLARE, POISON_FANG, SLUDGE_BOMB]),
        pike_mon!(MILOTIC, 4, [TOXIC, HYPNOSIS, BODY_SLAM, ICE_BEAM]),
        pike_mon!(DUSCLOPS, 5, [WILL_O_WISP, MEAN_LOOK, TOXIC, ICE_BEAM]),
    ],
    [
        pike_mon!(SEVIPER, 4, [TOXIC, GLARE, POISON_FANG, SLUDGE_BOMB]),
        pike_mon!(MILOTIC, 4, [TOXIC, HYPNOSIS, BODY_SLAM, ICE_BEAM]),
        pike_mon!(ELECTRODE, 5, [EXPLOSION, SELF_DESTRUCT, THUNDER, TOXIC]),
    ],
    [
        pike_mon!(SEVIPER, 4, [TOXIC, GLARE, POISON_FANG, SLUDGE_BOMB]),
        pike_mon!(MILOTIC, 4, [TOXIC, HYPNOSIS, BODY_SLAM, ICE_BEAM]),
        pike_mon!(BRELOOM, 5, [SPORE, STUN_SPORE, POISON_POWDER, HIDDEN_POWER]),
    ],
    [
        pike_mon!(SEVIPER, 4, [TOXIC, GLARE, POISON_FANG, SLUDGE_BOMB]),
        pike_mon!(MILOTIC, 4, [TOXIC, HYPNOSIS, BODY_SLAM, ICE_BEAM]),
        pike_mon!(WOBBUFFET, 5, [COUNTER, MIRROR_COAT, SAFEGUARD, ENCORE]),
    ],
];

#[cfg(test)]
mod tests {
    use super::*;

    /// One record as upstream's numeric values: `species`, `levelDelta`, and
    /// the four `moves`.
    type Row = (u16, u8, [u16; PIKE_WILD_MON_MOVES]);
    /// An upstream array's name and its records.
    type NamedRows = (String, Vec<Row>);
    /// A pointer table's name and the names it lists.
    type NamedNames = (String, Vec<String>);
    type Fixture = [[Row; PIKE_WILD_MON_SLOTS]; PIKE_WILD_HEADERS];

    /// `sLvl50Mons`' headers as `SPECIES_*`, `levelDelta` and `MOVE_*` values
    /// from `include/constants/species.h` and `moves.h`.
    #[rustfmt::skip]
    const EXPECTED_LEVEL_50: Fixture = [
        [
            (379, 4, [92, 137, 34, 188]),
            (329, 4, [92, 95, 34, 57]),
            (362, 5, [261, 212, 92, 325]),
        ],
        [
            (379, 4, [92, 137, 34, 188]),
            (329, 4, [92, 95, 34, 57]),
            (101, 5, [153, 120, 87, 92]),
        ],
        [
            (379, 4, [92, 137, 34, 188]),
            (329, 4, [92, 95, 34, 57]),
            (307, 5, [147, 78, 77, 237]),
        ],
        [
            (379, 4, [92, 137, 34, 188]),
            (329, 4, [92, 95, 34, 57]),
            (202, 5, [68, 243, 219, 194]),
        ],
    ];

    /// `sLvlOpenMons`' headers, in the same form.
    #[rustfmt::skip]
    const EXPECTED_OPEN_LEVEL: Fixture = [
        [
            (379, 4, [92, 137, 305, 188]),
            (329, 4, [92, 95, 34, 58]),
            (362, 5, [261, 212, 92, 58]),
        ],
        [
            (379, 4, [92, 137, 305, 188]),
            (329, 4, [92, 95, 34, 58]),
            (101, 5, [153, 120, 87, 92]),
        ],
        [
            (379, 4, [92, 137, 305, 188]),
            (329, 4, [92, 95, 34, 58]),
            (307, 5, [147, 78, 77, 237]),
        ],
        [
            (379, 4, [92, 137, 305, 188]),
            (329, 4, [92, 95, 34, 58]),
            (202, 5, [68, 243, 219, 227]),
        ],
    ];

    fn expected(mode: PikeLevelMode) -> &'static Fixture {
        match mode {
            PikeLevelMode::Level50 => &EXPECTED_LEVEL_50,
            PikeLevelMode::OpenLevel => &EXPECTED_OPEN_LEVEL,
        }
    }

    fn check_every_record(mode: PikeLevelMode) {
        let mut checked = 0;
        for (header, rows) in expected(mode).iter().enumerate() {
            for (slot, (species, decrement, moves)) in rows.iter().enumerate() {
                let mon = pike_wild_mon(mode, header, slot).expect("position is in range");
                let context = format!("{mode:?} header {header} slot {slot}");
                assert_eq!(mon.species(), SpeciesId(*species), "{context}");
                assert_eq!(mon.level_decrement().decrement(), *decrement, "{context}");
                assert_eq!(moves.map(MoveId), mon.moves(), "{context}");
                checked += 1;
            }
        }
        assert_eq!(checked, 12);
    }

    #[test]
    fn level_50_records_match_upstream() {
        check_every_record(PikeLevelMode::Level50);
    }

    #[test]
    fn open_level_records_match_upstream() {
        check_every_record(PikeLevelMode::OpenLevel);
    }

    #[test]
    fn each_mode_holds_four_headers_of_three_records() {
        assert_eq!(PIKE_WILD_HEADERS, 4);
        assert_eq!(PIKE_WILD_MON_SLOTS, 3);
        let mut records = 0;
        for mode in [PikeLevelMode::Level50, PikeLevelMode::OpenLevel] {
            for header in 0..4 {
                let slots = pike_header(mode, header).expect("header is in range");
                assert_eq!(slots.len(), 3, "{mode:?} header {header}");
                records += slots.len();
            }
        }
        assert_eq!(records, 24);
    }

    #[test]
    fn lookup_matches_hand_read_upstream_records() {
        let seviper = pike_wild_mon(PikeLevelMode::Level50, 0, 0).unwrap();
        assert_eq!(seviper.species(), SpeciesId::SEVIPER);
        assert_eq!(seviper.level_decrement(), PikeLevelDecrement::new(4));
        assert_eq!(
            seviper.moves(),
            [
                MoveId::TOXIC,
                MoveId::GLARE,
                MoveId::BODY_SLAM,
                MoveId::SLUDGE_BOMB
            ]
        );

        let wobbuffet = pike_wild_mon(PikeLevelMode::OpenLevel, 3, 2).unwrap();
        assert_eq!(wobbuffet.species(), SpeciesId::WOBBUFFET);
        assert_eq!(wobbuffet.level_decrement().decrement(), 5);
        assert_eq!(
            wobbuffet.moves(),
            [
                MoveId::COUNTER,
                MoveId::MIRROR_COAT,
                MoveId::SAFEGUARD,
                MoveId::ENCORE
            ]
        );
    }

    #[test]
    fn the_third_slot_differs_by_header_and_the_first_two_do_not() {
        let third: Vec<SpeciesId> = (0..PIKE_WILD_HEADERS)
            .map(|header| {
                pike_wild_mon(PikeLevelMode::Level50, header, 2)
                    .unwrap()
                    .species()
            })
            .collect();
        assert_eq!(
            third,
            [
                SpeciesId::DUSCLOPS,
                SpeciesId::ELECTRODE,
                SpeciesId::BRELOOM,
                SpeciesId::WOBBUFFET
            ]
        );
        for mode in [PikeLevelMode::Level50, PikeLevelMode::OpenLevel] {
            for header in 1..PIKE_WILD_HEADERS {
                for slot in 0..2 {
                    assert_eq!(
                        pike_wild_mon(mode, header, slot),
                        pike_wild_mon(mode, 0, slot),
                        "{mode:?} header {header} slot {slot}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_two_modes_differ_in_exactly_these_moves() {
        let mut differences = Vec::new();
        for header in 0..PIKE_WILD_HEADERS {
            for slot in 0..PIKE_WILD_MON_SLOTS {
                let a = pike_wild_mon(PikeLevelMode::Level50, header, slot).unwrap();
                let b = pike_wild_mon(PikeLevelMode::OpenLevel, header, slot).unwrap();
                assert_eq!(a.species(), b.species());
                assert_eq!(a.level_decrement(), b.level_decrement());
                for (index, (x, y)) in a.moves().iter().zip(b.moves()).enumerate() {
                    if *x != y {
                        differences.push((header, slot, index, x.index(), y.index()));
                    }
                }
            }
        }
        // Body Slam to Poison Fang, Surf to Ice Beam, Shadow Punch to Ice
        // Beam on Dusclops, and Destiny Bond to Encore on Wobbuffet.
        let mut expected = Vec::new();
        for header in 0..4 {
            expected.push((header, 0, 2, 34, 305));
            expected.push((header, 1, 3, 57, 58));
        }
        expected.push((0, 2, 3, 325, 58));
        expected.push((3, 2, 3, 194, 227));
        expected.sort_unstable();
        differences.sort_unstable();
        assert_eq!(differences, expected);
    }

    #[test]
    fn lookup_rejects_positions_outside_the_roster() {
        for mode in [PikeLevelMode::Level50, PikeLevelMode::OpenLevel] {
            assert_eq!(
                pike_wild_mon(mode, 4, 0),
                Err(PikeWildMonError::InvalidHeader(4))
            );
            assert_eq!(
                pike_wild_mon(mode, 0, 3),
                Err(PikeWildMonError::InvalidSlot(3))
            );
            assert_eq!(
                pike_wild_mon(mode, usize::MAX, usize::MAX),
                Err(PikeWildMonError::InvalidHeader(usize::MAX))
            );
            assert_eq!(
                pike_header(mode, 4),
                Err(PikeWildMonError::InvalidHeader(4))
            );
            assert!(pike_header(mode, 3).is_ok());
            assert!(pike_wild_mon(mode, 3, 2).is_ok());
        }
    }

    #[test]
    fn errors_name_the_rejected_value() {
        assert_eq!(
            PikeWildMonError::InvalidHeader(9).to_string(),
            "Battle Pike header `9` is out of range (there are 4)"
        );
        assert_eq!(
            PikeWildMonError::InvalidSlot(7).to_string(),
            "Battle Pike slot `7` is out of range (there are 3)"
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
    fn defines(text: &str) -> std::collections::HashMap<String, u32> {
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

    /// Strictly parses the `PikeWildMon` arrays and the pointer tables out of
    /// `src/battle_pike.c`: the arrays by name in file order, and the names
    /// each pointer table lists in order.
    fn parse_source(
        text: &str,
        species: &std::collections::HashMap<String, u32>,
        moves: &std::collections::HashMap<String, u32>,
    ) -> (Vec<NamedRows>, Vec<NamedNames>) {
        let value = |line: &str, field: &str| -> String {
            line.trim()
                .strip_prefix(&format!(".{field} = "))
                .unwrap_or_else(|| panic!("expected `.{field} = ` in `{line}`"))
                .trim_end_matches(',')
                .to_owned()
        };
        let mut arrays: Vec<NamedRows> = Vec::new();
        let mut tables: Vec<NamedNames> = Vec::new();
        let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
        while let Some(line) = lines.next() {
            if let Some(rest) = line.strip_prefix("static const struct PikeWildMon *const ") {
                if let Some(name) = rest.strip_suffix("[] =") {
                    assert_eq!(lines.next(), Some("{"));
                    let mut entries = Vec::new();
                    for entry in lines.by_ref() {
                        if entry == "};" {
                            break;
                        }
                        entries.push(entry.trim_end_matches(',').to_owned());
                    }
                    tables.push((name.to_owned(), entries));
                }
            } else if let Some(rest) = line.strip_prefix("static const struct PikeWildMon ") {
                let name = rest.strip_suffix("[] =").expect("array header").to_owned();
                assert_eq!(lines.next(), Some("{"));
                let mut rows: Vec<Row> = Vec::new();
                loop {
                    match lines.next().expect("array ends with `};`") {
                        "};" => break,
                        "{" => {
                            let species_name = value(lines.next().unwrap(), "species");
                            let delta = value(lines.next().unwrap(), "levelDelta");
                            let moves_line = value(lines.next().unwrap(), "moves");
                            let close = lines.next().expect("record ends with `}`");
                            assert!(matches!(close, "}" | "},"), "`{close}`");
                            let list = moves_line
                                .strip_prefix('{')
                                .and_then(|list| list.strip_suffix('}'))
                                .expect("moves list is braced");
                            let names: Vec<&str> = list.split(", ").collect();
                            assert_eq!(names.len(), 4, "{moves_line}");
                            let mut ids = [0_u16; 4];
                            for (id, name) in ids.iter_mut().zip(names) {
                                *id = u16::try_from(moves[name]).unwrap();
                            }
                            rows.push((
                                u16::try_from(species[&species_name]).unwrap(),
                                delta.parse().expect("numeric levelDelta"),
                                ids,
                            ));
                        }
                        other => panic!("unexpected line `{other}`"),
                    }
                }
                arrays.push((name, rows));
            }
        }
        (arrays, tables)
    }

    #[test]
    #[ignore = "reads the pinned upstream checkout; run ./init.sh first"]
    fn upstream_source_matches_the_rosters() {
        let species = defines(&upstream_file("include/constants/species.h"));
        let moves = defines(&upstream_file("include/constants/moves.h"));
        let (arrays, tables) = parse_source(&upstream_file("src/battle_pike.c"), &species, &moves);
        let by_name: std::collections::HashMap<_, _> = arrays.iter().cloned().collect();
        assert_eq!(arrays.len(), 8, "eight PikeWildMon arrays");
        let table: std::collections::HashMap<_, _> = tables.into_iter().collect();
        for (mode, prefix, pointer_table) in [
            (PikeLevelMode::Level50, "sLvl50_Mons", "sLvl50Mons"),
            (PikeLevelMode::OpenLevel, "sLvlOpen_Mons", "sLvlOpenMons"),
        ] {
            let names: Vec<String> = (1..=4).map(|n| format!("{prefix}{n}")).collect();
            assert_eq!(table[pointer_table], names, "{mode:?} pointer table");
            let mut checked = 0;
            for (header, name) in names.iter().enumerate() {
                let rows = &by_name[name];
                assert_eq!(rows.len(), 3, "{name}");
                assert_eq!(rows.as_slice(), expected(mode)[header].as_slice(), "{name}");
                for (slot, row) in rows.iter().enumerate() {
                    let mon = pike_wild_mon(mode, header, slot).unwrap();
                    assert_eq!(mon.species(), SpeciesId(row.0), "{name} slot {slot}");
                    assert_eq!(
                        mon.level_decrement().decrement(),
                        row.1,
                        "{name} slot {slot}"
                    );
                    assert_eq!(mon.moves().map(MoveId::index), row.2, "{name} slot {slot}");
                    checked += 1;
                }
            }
            assert_eq!(checked, 12, "{mode:?}");
        }
    }
}
