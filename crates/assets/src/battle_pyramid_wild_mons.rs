//! The Battle Pyramid wild Pokémon rosters: for each of the two level modes,
//! 20 rounds of 8 wild Pokémon records.
//!
//! Source: `sLevel50WildMons_Round1` to `Round20` and `sLevel50WildMonPointers`
//! (`src/data/battle_frontier/battle_pyramid_level_50_wild_mons.h`), and
//! `sOpenLevelWildMons_Round1` to `Round20` and `sOpenLevelWildMonPointers`
//! (`src/data/battle_frontier/battle_pyramid_open_level_wild_mons.h`). Each
//! record is a `struct PyramidWildMon` (`src/battle_pyramid.c:45-51`): a
//! species, `lvl`, `abilityNum`, and four moves. Rounds and slots are indexed
//! from `0`, in the order the pointer tables and record lists give.
//!
//! `lvl` has two meanings that this module keeps apart in the types.
//! `GenerateBattlePyramidWildMon` (`src/battle_pyramid.c:1344-1375`) reads it
//! as the level itself in Level 50 mode, and subtracts it from the facility
//! level in Open Level mode. A Level 50 record holds a [`PyramidMonLevel`];
//! an Open Level record holds a [`PyramidLevelOffset`]. The two are different
//! types with no conversion between them, so an Open Level offset cannot be
//! read as a level:
//!
//! ```compile_fail
//! use assets::battle_pyramid_wild_mons::{PyramidLevelOffset, PyramidMonLevel};
//! let _level: PyramidMonLevel = PyramidLevelOffset::new(15);
//! ```
//!
//! The same line compiles when it builds the type it names:
//!
//! ```
//! use assets::battle_pyramid_wild_mons::PyramidMonLevel;
//! let _level: PyramidMonLevel = PyramidMonLevel::new(35).unwrap();
//! ```
//!
//! `abilityNum` is a selector, not an ability: `0` and `1` pick the species'
//! first and second ability slot, and `ABILITY_RANDOM` (`2`,
//! `src/battle_pyramid.c:102`) leaves the choice to the generated Pokémon. It
//! is therefore a [`PyramidAbilitySelector`] and not an
//! [`AbilityId`](crate::species::AbilityId), where `2` would be a different
//! ability. Data only: wild-Pokémon generation, its RNG draws, ability and
//! move assignment, the round index from the win streak, Pyramid floors,
//! items, and trainers are not ported here.

use std::error::Error;
use std::fmt;

use crate::battle_moves::MoveId;
use crate::experience::MAX_LEVEL;
use crate::species::SpeciesId;

/// The number of rounds in each level mode (`TOTAL_PYRAMID_ROUNDS`).
pub const PYRAMID_ROUNDS: usize = 20;

/// The number of records in each round.
pub const PYRAMID_WILD_MON_SLOTS: usize = 8;

/// The number of moves in a record (`MAX_MON_MOVES`).
pub const PYRAMID_WILD_MON_MOVES: usize = 4;

/// A failed construction or lookup in the Battle Pyramid rosters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PyramidWildMonError {
    /// The round is not below [`PYRAMID_ROUNDS`].
    InvalidRound(usize),
    /// The slot is not below [`PYRAMID_WILD_MON_SLOTS`].
    InvalidSlot(usize),
    /// The value is not an `abilityNum` the rosters use (`0`, `1`, or `2`).
    InvalidAbilitySelector(u8),
    /// The value is `0` or above [`MAX_LEVEL`].
    InvalidLevel(u8),
}

impl fmt::Display for PyramidWildMonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRound(round) => write!(
                f,
                "Battle Pyramid round `{round}` is out of range (there are {PYRAMID_ROUNDS})"
            ),
            Self::InvalidSlot(slot) => write!(
                f,
                "Battle Pyramid slot `{slot}` is out of range (there are {PYRAMID_WILD_MON_SLOTS})"
            ),
            Self::InvalidAbilitySelector(value) => {
                write!(
                    f,
                    "Battle Pyramid ability selector `{value}` is not 0, 1, or 2"
                )
            }
            Self::InvalidLevel(value) => {
                write!(
                    f,
                    "Battle Pyramid level `{value}` is not in 1..={MAX_LEVEL}"
                )
            }
        }
    }
}

impl Error for PyramidWildMonError {}

/// Which roster set a lookup reads (`FRONTIER_LVL_50` or the Open Level mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PyramidLevelMode {
    /// The Level 50 mode: `lvl` is the level of the wild Pokémon.
    Level50,
    /// The Open Level mode: `lvl` is an offset from the facility level.
    OpenLevel,
}

/// A wild Pokémon's level in the Level 50 roster (`lvl` read as a level).
///
/// The wrapped value is private so every instance is in `1..=`[`MAX_LEVEL`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PyramidMonLevel(u8);

impl PyramidMonLevel {
    /// Validates a raw level.
    ///
    /// # Errors
    ///
    /// Returns [`PyramidWildMonError::InvalidLevel`] when `level` is `0` or
    /// above [`MAX_LEVEL`].
    pub const fn new(level: u8) -> Result<Self, PyramidWildMonError> {
        if level >= 1 && level <= MAX_LEVEL {
            Ok(Self(level))
        } else {
            Err(PyramidWildMonError::InvalidLevel(level))
        }
    }

    /// Returns the level.
    #[must_use]
    pub const fn level(self) -> u8 {
        self.0
    }
}

/// The amount an Open Level record subtracts from the facility level
/// (`lvl` read as an offset, `src/battle_pyramid.c:1370-1371`).
///
/// This is not a level: it has no accessor named for one and no conversion
/// to [`PyramidMonLevel`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PyramidLevelOffset(u8);

impl PyramidLevelOffset {
    /// Wraps a raw offset.
    #[must_use]
    pub const fn new(offset: u8) -> Self {
        Self(offset)
    }

    /// Returns the amount to subtract from the facility level.
    #[must_use]
    pub const fn offset(self) -> u8 {
        self.0
    }
}

/// The `lvl` of a record, tagged with the meaning its level mode gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PyramidWildLevel {
    /// A Level 50 record's level.
    Level50(PyramidMonLevel),
    /// An Open Level record's offset from the facility level.
    OpenLevelOffset(PyramidLevelOffset),
}

impl PyramidWildLevel {
    /// Returns the level mode this value belongs to.
    #[must_use]
    pub const fn mode(self) -> PyramidLevelMode {
        match self {
            Self::Level50(_) => PyramidLevelMode::Level50,
            Self::OpenLevelOffset(_) => PyramidLevelMode::OpenLevel,
        }
    }

    /// Returns the level of a Level 50 record, or `None` for an Open Level
    /// offset.
    #[must_use]
    pub const fn level(self) -> Option<PyramidMonLevel> {
        match self {
            Self::Level50(level) => Some(level),
            Self::OpenLevelOffset(_) => None,
        }
    }

    /// Returns the offset of an Open Level record, or `None` for a Level 50
    /// level.
    #[must_use]
    pub const fn offset(self) -> Option<PyramidLevelOffset> {
        match self {
            Self::Level50(_) => None,
            Self::OpenLevelOffset(offset) => Some(offset),
        }
    }
}

/// A record's `abilityNum`: which ability the generated Pokémon takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PyramidAbilitySelector {
    /// `0`: the species' first ability slot.
    First,
    /// `1`: the species' second ability slot.
    Second,
    /// `ABILITY_RANDOM` (`2`): left to the generated Pokémon.
    Random,
}

impl PyramidAbilitySelector {
    /// Validates a raw `abilityNum`.
    ///
    /// # Errors
    ///
    /// Returns [`PyramidWildMonError::InvalidAbilitySelector`] when `value`
    /// is not `0`, `1`, or `2`.
    pub const fn new(value: u8) -> Result<Self, PyramidWildMonError> {
        match value {
            0 => Ok(Self::First),
            1 => Ok(Self::Second),
            2 => Ok(Self::Random),
            _ => Err(PyramidWildMonError::InvalidAbilitySelector(value)),
        }
    }

    /// Returns the raw upstream `abilityNum`.
    #[must_use]
    pub const fn raw(self) -> u8 {
        match self {
            Self::First => 0,
            Self::Second => 1,
            Self::Random => 2,
        }
    }
}

/// One roster record (`struct PyramidWildMon`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PyramidWildMon {
    species: SpeciesId,
    level: PyramidWildLevel,
    ability: PyramidAbilitySelector,
    moves: [MoveId; PYRAMID_WILD_MON_MOVES],
}

impl PyramidWildMon {
    /// A Level 50 record; panics at compile time on a level outside
    /// `1..=`[`MAX_LEVEL`], so a bad table row cannot build.
    const fn level_50(
        species: SpeciesId,
        level: u8,
        ability: PyramidAbilitySelector,
        moves: [MoveId; PYRAMID_WILD_MON_MOVES],
    ) -> Self {
        let Ok(level) = PyramidMonLevel::new(level) else {
            panic!("Level 50 roster level is outside 1..=100")
        };
        Self {
            species,
            level: PyramidWildLevel::Level50(level),
            ability,
            moves,
        }
    }

    /// An Open Level record.
    const fn open_level(
        species: SpeciesId,
        offset: u8,
        ability: PyramidAbilitySelector,
        moves: [MoveId; PYRAMID_WILD_MON_MOVES],
    ) -> Self {
        Self {
            species,
            level: PyramidWildLevel::OpenLevelOffset(PyramidLevelOffset::new(offset)),
            ability,
            moves,
        }
    }

    /// Returns the species (`species`).
    #[must_use]
    pub const fn species(&self) -> SpeciesId {
        self.species
    }

    /// Returns `lvl`, tagged with its meaning: a level for a Level 50 record,
    /// an offset for an Open Level record.
    #[must_use]
    pub const fn level(&self) -> PyramidWildLevel {
        self.level
    }

    /// Returns the ability selector (`abilityNum`).
    #[must_use]
    pub const fn ability_selector(&self) -> PyramidAbilitySelector {
        self.ability
    }

    /// Returns the four moves (`moves`); an unused slot holds
    /// [`MoveId::NONE`].
    #[must_use]
    pub const fn moves(&self) -> [MoveId; PYRAMID_WILD_MON_MOVES] {
        self.moves
    }
}

/// A validated round and slot position in a roster.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PyramidRosterPosition {
    round: usize,
    slot: usize,
}

impl PyramidRosterPosition {
    /// Validates a zero-based round and slot. An invalid round is reported
    /// before an invalid slot.
    ///
    /// # Errors
    ///
    /// Returns [`PyramidWildMonError::InvalidRound`] when `round` is
    /// [`PYRAMID_ROUNDS`] or more, and [`PyramidWildMonError::InvalidSlot`]
    /// when `slot` is [`PYRAMID_WILD_MON_SLOTS`] or more.
    pub const fn new(round: usize, slot: usize) -> Result<Self, PyramidWildMonError> {
        if round >= PYRAMID_ROUNDS {
            Err(PyramidWildMonError::InvalidRound(round))
        } else if slot >= PYRAMID_WILD_MON_SLOTS {
            Err(PyramidWildMonError::InvalidSlot(slot))
        } else {
            Ok(Self { round, slot })
        }
    }

    /// Returns the zero-based round.
    #[must_use]
    pub const fn round(self) -> usize {
        self.round
    }

    /// Returns the zero-based slot.
    #[must_use]
    pub const fn slot(self) -> usize {
        self.slot
    }
}

/// Returns the roster record at `round` and `slot` in `mode`.
///
/// # Errors
///
/// Returns [`PyramidWildMonError::InvalidRound`] when `round` is
/// [`PYRAMID_ROUNDS`] or more, and [`PyramidWildMonError::InvalidSlot`] when
/// `slot` is [`PYRAMID_WILD_MON_SLOTS`] or more; the round is checked first.
pub fn pyramid_wild_mon(
    mode: PyramidLevelMode,
    round: usize,
    slot: usize,
) -> Result<&'static PyramidWildMon, PyramidWildMonError> {
    let position = PyramidRosterPosition::new(round, slot)?;
    Ok(&roster(mode)[position.round()][position.slot()])
}

/// Returns the eight records of `round` in `mode`.
///
/// # Errors
///
/// Returns [`PyramidWildMonError::InvalidRound`] when `round` is
/// [`PYRAMID_ROUNDS`] or more.
pub fn pyramid_round(
    mode: PyramidLevelMode,
    round: usize,
) -> Result<&'static [PyramidWildMon; PYRAMID_WILD_MON_SLOTS], PyramidWildMonError> {
    roster(mode)
        .get(round)
        .ok_or(PyramidWildMonError::InvalidRound(round))
}

type Roster = [[PyramidWildMon; PYRAMID_WILD_MON_SLOTS]; PYRAMID_ROUNDS];

const fn roster(mode: PyramidLevelMode) -> &'static Roster {
    match mode {
        PyramidLevelMode::Level50 => &LEVEL_50_ROSTER,
        PyramidLevelMode::OpenLevel => &OPEN_LEVEL_ROSTER,
    }
}

/// Builds a Level 50 record from the part of each upstream name after the
/// `SPECIES_` and `MOVE_` prefixes.
macro_rules! level_50 {
    ($species:ident, $level:literal, $ability:ident, [$($move:ident),+ $(,)?]) => {
        PyramidWildMon::level_50(
            SpeciesId::$species,
            $level,
            PyramidAbilitySelector::$ability,
            [$(MoveId::$move),+],
        )
    };
}

/// Builds an Open Level record, the same way as `level_50!`.
macro_rules! open_level {
    ($species:ident, $offset:literal, $ability:ident, [$($move:ident),+ $(,)?]) => {
        PyramidWildMon::open_level(
            SpeciesId::$species,
            $offset,
            PyramidAbilitySelector::$ability,
            [$(MoveId::$move),+],
        )
    };
}

/// `sLevel50WildMons_Round1` to `Round20`, in `sLevel50WildMonPointers` order.
#[rustfmt::skip]
static LEVEL_50_ROSTER: Roster = [
    [
        level_50!(PLUSLE, 35, Random, [THUNDER_WAVE, SPARK, ENCORE, NONE]),
        level_50!(MINUN, 35, Random, [THUNDER_WAVE, THUNDERBOLT, QUICK_ATTACK, NONE]),
        level_50!(PIKACHU, 37, Random, [THUNDER_WAVE, THUNDERBOLT, SLAM, NONE]),
        level_50!(ELECTABUZZ, 37, Random, [THUNDER_PUNCH, SWIFT, SCREECH, NONE]),
        level_50!(VILEPLUME, 39, Random, [STUN_SPORE, GIGA_DRAIN, PROTECT, NONE]),
        level_50!(MANECTRIC, 39, Random, [THUNDER_WAVE, THUNDER, QUICK_ATTACK, NONE]),
        level_50!(BRELOOM, 40, Random, [STUN_SPORE, FOCUS_PUNCH, GIGA_DRAIN, MACH_PUNCH]),
        level_50!(JOLTEON, 40, Random, [THUNDER_WAVE, THUNDER, PIN_MISSILE, QUICK_ATTACK]),
    ],
    [
        level_50!(GULPIN, 36, Random, [TOXIC, SLUDGE, PROTECT, NONE]),
        level_50!(ROSELIA, 36, Random, [TOXIC, GIGA_DRAIN, MAGICAL_LEAF, PETAL_DANCE]),
        level_50!(BUTTERFREE, 38, Random, [POISON_POWDER, GUST, PSYBEAM, NONE]),
        level_50!(SEVIPER, 38, Random, [POISON_FANG, SWAGGER, CRUNCH, POISON_TAIL]),
        level_50!(SKARMORY, 40, Random, [TOXIC, FLY, STEEL_WING, NONE]),
        level_50!(LUDICOLO, 40, Second, [TOXIC, PROTECT, DIVE, RAIN_DANCE]),
        level_50!(CROBAT, 41, Random, [TOXIC, CONFUSE_RAY, MEAN_LOOK, BITE]),
        level_50!(GENGAR, 41, Random, [TOXIC, SHADOW_PUNCH, NIGHT_SHADE, NONE]),
    ],
    [
        level_50!(GROWLITHE, 37, Random, [FLAME_WHEEL, TAKE_DOWN, NONE, NONE]),
        level_50!(VULPIX, 37, Random, [WILL_O_WISP, FLAMETHROWER, NONE, NONE]),
        level_50!(MAGCARGO, 39, Second, [FLAMETHROWER, ROCK_SLIDE, PROTECT, NONE]),
        level_50!(NINETALES, 39, Random, [WILL_O_WISP, QUICK_ATTACK, FLAMETHROWER, NONE]),
        level_50!(MEDICHAM, 41, Random, [FIRE_PUNCH, HI_JUMP_KICK, NONE, NONE]),
        level_50!(WEEZING, 41, Random, [WILL_O_WISP, FLAMETHROWER, PROTECT, NONE]),
        level_50!(DUSCLOPS, 42, Random, [WILL_O_WISP, CONFUSE_RAY, MEAN_LOOK, SHADOW_PUNCH]),
        level_50!(HOUNDOOM, 42, Random, [FLAMETHROWER, BITE, SOLAR_BEAM, OVERHEAT]),
    ],
    [
        level_50!(DUNSPARCE, 42, Random, [SPITE, TOXIC, PROTECT, NONE]),
        level_50!(BANETTE, 41, Random, [GRUDGE, WILL_O_WISP, NIGHT_SHADE, NONE]),
        level_50!(MISDREAVUS, 42, Random, [GRUDGE, SPITE, SHADOW_BALL, NONE]),
        level_50!(NINETALES, 42, Random, [GRUDGE, WILL_O_WISP, OVERHEAT, NONE]),
        level_50!(ABSOL, 44, Random, [BITE, AERIAL_ACE, SHADOW_BALL, PROTECT]),
        level_50!(DUSCLOPS, 44, Random, [WILL_O_WISP, PROTECT, TOXIC, SHADOW_BALL]),
        level_50!(SHEDINJA, 45, Random, [GRUDGE, TOXIC, SPITE, NONE]),
        level_50!(GENGAR, 45, Random, [GRUDGE, SPITE, NIGHT_SHADE, NONE]),
    ],
    [
        level_50!(HAUNTER, 41, Random, [NIGHT_SHADE, THUNDERBOLT, SLUDGE_BOMB, NONE]),
        level_50!(CHIMECHO, 41, Random, [DOUBLE_EDGE, TOXIC, PSYCHIC, PROTECT]),
        level_50!(SOLROCK, 42, Random, [EARTHQUAKE, ROCK_SLIDE, FIRE_BLAST, TOXIC]),
        level_50!(MISDREAVUS, 42, Random, [PSYCHIC, SPITE, SHADOW_BALL, PAIN_SPLIT]),
        level_50!(CLAYDOL, 44, Random, [EARTHQUAKE, ANCIENT_POWER, SELF_DESTRUCT, PSYCHIC]),
        level_50!(WEEZING, 44, Random, [SLUDGE_BOMB, SELF_DESTRUCT, PROTECT, NONE]),
        level_50!(FLYGON, 45, Random, [EARTHQUAKE, CRUNCH, DRAGON_CLAW, DRAGON_BREATH]),
        level_50!(GENGAR, 45, Random, [THUNDERBOLT, PSYCHIC, GIGA_DRAIN, NIGHT_SHADE]),
    ],
    [
        level_50!(DIGLETT, 41, Random, [ROCK_SLIDE, SLASH, DIG, NONE]),
        level_50!(TRAPINCH, 41, Random, [ROCK_SLIDE, EARTHQUAKE, GIGA_DRAIN, NONE]),
        level_50!(WYNAUT, 42, First, [DESTINY_BOND, SPLASH, COUNTER, MIRROR_COAT]),
        level_50!(DIGLETT, 42, Random, [EARTHQUAKE, ROCK_SLIDE, MAGNITUDE, TOXIC]),
        level_50!(TRAPINCH, 44, Random, [ROCK_SLIDE, EARTHQUAKE, GIGA_DRAIN, PROTECT]),
        level_50!(WYNAUT, 44, First, [COUNTER, MIRROR_COAT, DESTINY_BOND, NONE]),
        level_50!(WOBBUFFET, 45, First, [COUNTER, MIRROR_COAT, DESTINY_BOND, NONE]),
        level_50!(DUGTRIO, 45, Second, [ROCK_SLIDE, SLUDGE_BOMB, EARTHQUAKE, PROTECT]),
    ],
    [
        level_50!(GLALIE, 41, Random, [ICE_BEAM, CRUNCH, PROTECT, NONE]),
        level_50!(SNEASEL, 41, Random, [ICE_BEAM, CRUSH_CLAW, SPITE, NONE]),
        level_50!(DEWGONG, 42, Random, [BLIZZARD, DOUBLE_EDGE, SURF, NONE]),
        level_50!(PILOSWINE, 42, Random, [ICE_BEAM, EARTHQUAKE, TOXIC, NONE]),
        level_50!(JYNX, 44, Random, [BLIZZARD, LOVELY_KISS, PSYCHIC, NONE]),
        level_50!(CLOYSTER, 44, Random, [ICE_BEAM, SURF, PROTECT, NONE]),
        level_50!(WALREIN, 45, Random, [BLIZZARD, BODY_SLAM, SURF, NONE]),
        level_50!(LAPRAS, 45, Random, [SING, BODY_SLAM, ICE_BEAM, PSYCHIC]),
    ],
    [
        level_50!(WEEZING, 41, Random, [SELF_DESTRUCT, SLUDGE_BOMB, FIRE_BLAST, NONE]),
        level_50!(ELECTRODE, 41, Random, [SELF_DESTRUCT, THUNDERBOLT, ROLLOUT, NONE]),
        level_50!(GENGAR, 42, Random, [DESTINY_BOND, LICK, SHADOW_BALL, NONE]),
        level_50!(GOLEM, 42, Random, [SELF_DESTRUCT, PROTECT, EARTHQUAKE, NONE]),
        level_50!(PINECO, 44, Random, [EXPLOSION, DOUBLE_EDGE, GIGA_DRAIN, NONE]),
        level_50!(SOLROCK, 44, Random, [EXPLOSION, FIRE_SPIN, PSYWAVE, NONE]),
        level_50!(FORRETRESS, 45, Random, [EXPLOSION, TOXIC, ROCK_SLIDE, NONE]),
        level_50!(SHIFTRY, 45, Random, [EXPLOSION, GIGA_DRAIN, SOLAR_BEAM, PROTECT]),
    ],
    [
        level_50!(WOBBUFFET, 41, Random, [COUNTER, MIRROR_COAT, SAFEGUARD, DESTINY_BOND]),
        level_50!(METANG, 41, Random, [EARTHQUAKE, TOXIC, SLUDGE_BOMB, PSYCHIC]),
        level_50!(EXEGGUTOR, 42, Random, [EGG_BOMB, PSYCHIC, HYPNOSIS, NONE]),
        level_50!(SLOWKING, 42, Random, [SHADOW_BALL, SURF, ICE_BEAM, FLAMETHROWER]),
        level_50!(XATU, 44, Random, [CONFUSE_RAY, SHADOW_BALL, PSYCHIC, STEEL_WING]),
        level_50!(ALAKAZAM, 44, Random, [PSYCHIC, FIRE_PUNCH, ICE_PUNCH, TOXIC]),
        level_50!(STARMIE, 45, Random, [PSYCHIC, THUNDERBOLT, SURF, ICE_BEAM]),
        level_50!(ESPEON, 45, Random, [PSYCHIC, DIG, SHADOW_BALL, NONE]),
    ],
    [
        level_50!(GOLEM, 41, Random, [SELF_DESTRUCT, EARTHQUAKE, PROTECT, NONE]),
        level_50!(STEELIX, 41, Random, [IRON_TAIL, CRUNCH, EARTHQUAKE, NONE]),
        level_50!(OMASTAR, 42, Random, [SURF, MUD_SHOT, ANCIENT_POWER, NONE]),
        level_50!(LUNATONE, 42, Random, [HYPNOSIS, PSYWAVE, EXPLOSION, NONE]),
        level_50!(SHUCKLE, 44, Random, [TOXIC, PROTECT, WRAP, NONE]),
        level_50!(ARMALDO, 44, Random, [ANCIENT_POWER, PROTECT, AERIAL_ACE, NONE]),
        level_50!(CRADILY, 45, Random, [SLUDGE_BOMB, GIGA_DRAIN, CONFUSE_RAY, NONE]),
        level_50!(AERODACTYL, 45, Random, [HYPER_BEAM, ROCK_SLIDE, BITE, NONE]),
    ],
    [
        level_50!(POLIWRATH, 41, Random, [SUBMISSION, FOCUS_PUNCH, SURF, NONE]),
        level_50!(HARIYAMA, 41, Random, [FAKE_OUT, SURF, FOCUS_PUNCH, NONE]),
        level_50!(BRELOOM, 42, Random, [SPORE, FOCUS_PUNCH, PROTECT, NONE]),
        level_50!(MEDICHAM, 42, Random, [THUNDER_PUNCH, FIRE_PUNCH, ICE_PUNCH, FOCUS_PUNCH]),
        level_50!(HITMONCHAN, 44, Random, [THUNDER_PUNCH, FIRE_PUNCH, ICE_PUNCH, FOCUS_PUNCH]),
        level_50!(HITMONLEE, 44, Random, [MEGA_KICK, FOCUS_PUNCH, NONE, NONE]),
        level_50!(HERACROSS, 45, Random, [MEGAHORN, EARTHQUAKE, FOCUS_PUNCH, ROCK_SLIDE]),
        level_50!(MACHAMP, 50, Random, [ROCK_SLIDE, EARTHQUAKE, FOCUS_PUNCH, SEISMIC_TOSS]),
    ],
    [
        level_50!(QUAGSIRE, 41, Random, [RAIN_DANCE, SURF, PROTECT, NONE]),
        level_50!(TROPIUS, 41, Random, [SUNNY_DAY, SOLAR_BEAM, NONE, NONE]),
        level_50!(PUPITAR, 42, Random, [SANDSTORM, EARTHQUAKE, ROCK_SLIDE, NONE]),
        level_50!(LAPRAS, 42, Random, [HAIL, ICE_BEAM, NONE, NONE]),
        level_50!(CACTURNE, 44, Random, [SANDSTORM, GIGA_DRAIN, SOLAR_BEAM, NONE]),
        level_50!(FLAREON, 44, Random, [SUNNY_DAY, FLAMETHROWER, PROTECT, NONE]),
        level_50!(WALREIN, 45, Random, [HAIL, ICE_BEAM, NONE, NONE]),
        level_50!(GYARADOS, 50, Random, [RAIN_DANCE, THUNDER, HYDRO_PUMP, NONE]),
    ],
    [
        level_50!(PINECO, 42, Random, [EXPLOSION, TAKE_DOWN, NONE, NONE]),
        level_50!(SHUCKLE, 42, Random, [TOXIC, EARTHQUAKE, PROTECT, NONE]),
        level_50!(VENOMOTH, 46, Random, [SILVER_WIND, POISON_POWDER, SLEEP_POWDER, PSYCHIC]),
        level_50!(SCIZOR, 46, Random, [QUICK_ATTACK, METAL_CLAW, FURY_CUTTER, PURSUIT]),
        level_50!(HERACROSS, 48, Random, [MEGAHORN, BRICK_BREAK, EARTHQUAKE, ROCK_SLIDE]),
        level_50!(FORRETRESS, 48, Random, [EXPLOSION, EARTHQUAKE, PROTECT, NONE]),
        level_50!(ARMALDO, 50, Random, [WATER_PULSE, PROTECT, ROCK_SLIDE, NONE]),
        level_50!(SHEDINJA, 50, Random, [SHADOW_BALL, TOXIC, SPITE, GRUDGE]),
    ],
    [
        level_50!(SABLEYE, 42, Random, [NIGHT_SHADE, PSYCHIC, AERIAL_ACE, NONE]),
        level_50!(SNEASEL, 42, Random, [ICE_BEAM, TAUNT, FAINT_ATTACK, QUICK_ATTACK]),
        level_50!(CRAWDAUNT, 46, Random, [CRABHAMMER, ICE_BEAM, SURF, NONE]),
        level_50!(SHIFTRY, 46, Random, [EXPLOSION, SHADOW_BALL, AERIAL_ACE, GIGA_DRAIN]),
        level_50!(CACTURNE, 48, Random, [TOXIC, GIGA_DRAIN, NEEDLE_ARM, NONE]),
        level_50!(ABSOL, 48, Random, [BITE, PROTECT, SLASH, NONE]),
        level_50!(HOUNDOOM, 50, Random, [OVERHEAT, CRUNCH, SHADOW_BALL, PROTECT]),
        level_50!(UMBREON, 50, Random, [PSYCHIC, SHADOW_BALL, IRON_TAIL, QUICK_ATTACK]),
    ],
    [
        level_50!(OCTILLERY, 42, Random, [OCTAZOOKA, ICE_BEAM, FIRE_BLAST, NONE]),
        level_50!(DEWGONG, 42, Random, [WATER_PULSE, ICE_BEAM, HEADBUTT, NONE]),
        level_50!(PELIPPER, 46, Random, [PROTECT, SUPERSONIC, SURF, NONE]),
        level_50!(QUAGSIRE, 46, Random, [EARTHQUAKE, ROCK_TOMB, SURF, NONE]),
        level_50!(LUDICOLO, 48, Random, [PROTECT, SOLAR_BEAM, TOXIC, ICE_BEAM]),
        level_50!(SLOWKING, 48, Random, [PSYCHIC, HEADBUTT, SWAGGER, NONE]),
        level_50!(STARMIE, 50, Random, [WATER_PULSE, THUNDERBOLT, CONFUSE_RAY, BLIZZARD]),
        level_50!(BLASTOISE, 50, Random, [HYDRO_PUMP, BITE, ICE_BEAM, NONE]),
    ],
    [
        level_50!(DUSKULL, 42, Random, [NIGHT_SHADE, WILL_O_WISP, SHADOW_BALL, PROTECT]),
        level_50!(HAUNTER, 42, Random, [TOXIC, SPITE, HYPNOSIS, SHADOW_BALL]),
        level_50!(BANETTE, 46, Random, [SHADOW_BALL, SPITE, WILL_O_WISP, NONE]),
        level_50!(MISDREAVUS, 46, Random, [PERISH_SONG, SPITE, MEAN_LOOK, NONE]),
        level_50!(SABLEYE, 48, Random, [SHADOW_BALL, MEAN_LOOK, DIG, NIGHT_SHADE]),
        level_50!(DUSCLOPS, 48, Random, [WILL_O_WISP, TOXIC, SHADOW_BALL, NONE]),
        level_50!(SHEDINJA, 50, Random, [SHADOW_BALL, SPITE, GRUDGE, PROTECT]),
        level_50!(GENGAR, 50, Random, [PSYCHIC, DESTINY_BOND, SPITE, NIGHT_SHADE]),
    ],
    [
        level_50!(MAWILE, 42, Random, [CRUNCH, TOXIC, ICE_BEAM, NONE]),
        level_50!(MAGNETON, 42, Random, [THUNDERBOLT, THUNDER_WAVE, NONE, NONE]),
        level_50!(STEELIX, 46, Random, [ROCK_THROW, DOUBLE_EDGE, EARTHQUAKE, NONE]),
        level_50!(SCIZOR, 46, Random, [METAL_CLAW, SLASH, NONE, NONE]),
        level_50!(FORRETRESS, 48, Random, [EXPLOSION, TOXIC, NONE, NONE]),
        level_50!(SKARMORY, 48, Random, [STEEL_WING, TOXIC, FLY, PROTECT]),
        level_50!(AGGRON, 50, Random, [EARTHQUAKE, TAKE_DOWN, SURF, ICE_BEAM]),
        level_50!(METAGROSS, 50, Random, [EARTHQUAKE, PSYCHIC, SHADOW_BALL, BRICK_BREAK]),
    ],
    [
        level_50!(DRAGONAIR, 42, Random, [THUNDER_WAVE, TOXIC, ICE_BEAM, NONE]),
        level_50!(VIBRAVA, 42, Random, [EARTHQUAKE, DRAGON_BREATH, CRUNCH, STEEL_WING]),
        level_50!(ALTARIA, 46, Random, [EARTHQUAKE, DRAGON_CLAW, SING, PROTECT]),
        level_50!(FLYGON, 46, Random, [EARTHQUAKE, DRAGON_CLAW, FIRE_BLAST, NONE]),
        level_50!(AERODACTYL, 48, Random, [EARTHQUAKE, ROCK_SLIDE, DRAGON_CLAW, NONE]),
        level_50!(GYARADOS, 48, Random, [EARTHQUAKE, SURF, THRASH, BITE]),
        level_50!(KINGDRA, 50, Random, [HYDRO_PUMP, ICE_BEAM, PROTECT, NONE]),
        level_50!(CHARIZARD, 50, Random, [FLAMETHROWER, FOCUS_PUNCH, FIRE_BLAST, IRON_TAIL]),
    ],
    [
        level_50!(ARCANINE, 42, Random, [FIRE_BLAST, TAKE_DOWN, NONE, NONE]),
        level_50!(POLIWRATH, 42, Random, [HYDRO_PUMP, ICE_BEAM, NONE, NONE]),
        level_50!(RAICHU, 46, Random, [THUNDER, THUNDER_WAVE, SLAM, NONE]),
        level_50!(VAPOREON, 46, Random, [SURF, ICE_BEAM, NONE, NONE]),
        level_50!(JOLTEON, 48, Random, [THUNDERBOLT, PIN_MISSILE, NONE, NONE]),
        level_50!(FLAREON, 48, Random, [FLAMETHROWER, BITE, NONE, NONE]),
        level_50!(NINETALES, 50, Random, [FLAMETHROWER, WILL_O_WISP, PROTECT, NONE]),
        level_50!(STARMIE, 50, Random, [ICE_BEAM, SURF, THUNDERBOLT, PSYCHIC]),
    ],
    [
        level_50!(KANGASKHAN, 42, Random, [HYPER_BEAM, FLAMETHROWER, SURF, DIZZY_PUNCH]),
        level_50!(SWELLOW, 42, Random, [AERIAL_ACE, HYPER_BEAM, TOXIC, NONE]),
        level_50!(URSARING, 46, Random, [HYPER_BEAM, EARTHQUAKE, FOCUS_PUNCH, PROTECT]),
        level_50!(PORYGON2, 46, Random, [PSYBEAM, HYPER_BEAM, SHADOW_BALL, ICE_BEAM]),
        level_50!(TAUROS, 48, Random, [EARTHQUAKE, HYPER_BEAM, SURF, THUNDERBOLT]),
        level_50!(FEAROW, 48, Random, [HYPER_BEAM, FLY, MIRROR_MOVE, PROTECT]),
        level_50!(SNORLAX, 50, Random, [HYPER_BEAM, BODY_SLAM, SHADOW_BALL, EARTHQUAKE]),
        level_50!(SLAKING, 50, Random, [HYPER_BEAM, EARTHQUAKE, SHADOW_BALL, ICE_BEAM]),
    ],
];

/// `sOpenLevelWildMons_Round1` to `Round20`, in `sOpenLevelWildMonPointers` order.
#[rustfmt::skip]
static OPEN_LEVEL_ROSTER: Roster = [
    [
        open_level!(PLUSLE, 15, Random, [THUNDER_WAVE, SPARK, ENCORE, NONE]),
        open_level!(MINUN, 15, Random, [THUNDER_WAVE, THUNDERBOLT, QUICK_ATTACK, NONE]),
        open_level!(PIKACHU, 13, Random, [THUNDER_WAVE, THUNDERBOLT, SLAM, NONE]),
        open_level!(ELECTABUZZ, 13, Random, [THUNDER_PUNCH, SWIFT, SCREECH, NONE]),
        open_level!(VILEPLUME, 8, Random, [STUN_SPORE, GIGA_DRAIN, PROTECT, NONE]),
        open_level!(MANECTRIC, 8, Random, [THUNDER_WAVE, THUNDER, QUICK_ATTACK, NONE]),
        open_level!(BRELOOM, 6, Random, [STUN_SPORE, FOCUS_PUNCH, GIGA_DRAIN, MACH_PUNCH]),
        open_level!(JOLTEON, 6, Random, [THUNDER_WAVE, THUNDER, PIN_MISSILE, QUICK_ATTACK]),
    ],
    [
        open_level!(GULPIN, 14, Random, [TOXIC, SLUDGE, PROTECT, NONE]),
        open_level!(ROSELIA, 14, Random, [TOXIC, GIGA_DRAIN, MAGICAL_LEAF, PETAL_DANCE]),
        open_level!(BUTTERFREE, 12, Random, [POISON_POWDER, GUST, PSYBEAM, NONE]),
        open_level!(SEVIPER, 12, Random, [POISON_FANG, SWAGGER, CRUNCH, POISON_TAIL]),
        open_level!(SKARMORY, 7, Random, [TOXIC, FLY, STEEL_WING, NONE]),
        open_level!(LUDICOLO, 7, Second, [TOXIC, PROTECT, DIVE, RAIN_DANCE]),
        open_level!(CROBAT, 5, Random, [TOXIC, CONFUSE_RAY, MEAN_LOOK, BITE]),
        open_level!(GENGAR, 5, Random, [TOXIC, SHADOW_PUNCH, NIGHT_SHADE, NONE]),
    ],
    [
        open_level!(GROWLITHE, 13, Random, [FLAME_WHEEL, TAKE_DOWN, NONE, NONE]),
        open_level!(VULPIX, 13, Random, [WILL_O_WISP, FLAMETHROWER, NONE, NONE]),
        open_level!(MAGCARGO, 11, Second, [FLAMETHROWER, ROCK_SLIDE, PROTECT, NONE]),
        open_level!(NINETALES, 11, Random, [WILL_O_WISP, QUICK_ATTACK, FLAMETHROWER, NONE]),
        open_level!(MEDICHAM, 6, Random, [FIRE_PUNCH, HI_JUMP_KICK, NONE, NONE]),
        open_level!(WEEZING, 6, Random, [WILL_O_WISP, FLAMETHROWER, PROTECT, NONE]),
        open_level!(DUSCLOPS, 5, Random, [WILL_O_WISP, CONFUSE_RAY, MEAN_LOOK, SHADOW_PUNCH]),
        open_level!(HOUNDOOM, 5, Random, [FLAMETHROWER, BITE, SOLAR_BEAM, OVERHEAT]),
    ],
    [
        open_level!(DUNSPARCE, 10, Random, [SPITE, TOXIC, PROTECT, NONE]),
        open_level!(BANETTE, 8, Random, [GRUDGE, WILL_O_WISP, NIGHT_SHADE, NONE]),
        open_level!(MISDREAVUS, 8, Random, [GRUDGE, SPITE, SHADOW_BALL, NONE]),
        open_level!(NINETALES, 6, Random, [GRUDGE, WILL_O_WISP, OVERHEAT, NONE]),
        open_level!(ABSOL, 10, Random, [BITE, AERIAL_ACE, SHADOW_BALL, PROTECT]),
        open_level!(DUSCLOPS, 6, Random, [WILL_O_WISP, PROTECT, TOXIC, SHADOW_BALL]),
        open_level!(SHEDINJA, 5, Random, [GRUDGE, TOXIC, SPITE, NONE]),
        open_level!(GENGAR, 5, Random, [GRUDGE, SPITE, NIGHT_SHADE, NONE]),
    ],
    [
        open_level!(HAUNTER, 10, Random, [NIGHT_SHADE, THUNDERBOLT, SLUDGE_BOMB, NONE]),
        open_level!(CHIMECHO, 10, Random, [DOUBLE_EDGE, TOXIC, PSYCHIC, PROTECT]),
        open_level!(SOLROCK, 8, Random, [EARTHQUAKE, ROCK_SLIDE, FIRE_BLAST, TOXIC]),
        open_level!(MISDREAVUS, 8, Random, [PSYCHIC, SPITE, SHADOW_BALL, PAIN_SPLIT]),
        open_level!(CLAYDOL, 6, Random, [EARTHQUAKE, ANCIENT_POWER, SELF_DESTRUCT, PSYCHIC]),
        open_level!(WEEZING, 6, Random, [SLUDGE_BOMB, SELF_DESTRUCT, PROTECT, NONE]),
        open_level!(FLYGON, 5, Random, [EARTHQUAKE, CRUNCH, DRAGON_CLAW, DRAGON_BREATH]),
        open_level!(GENGAR, 5, Random, [THUNDERBOLT, PSYCHIC, GIGA_DRAIN, NIGHT_SHADE]),
    ],
    [
        open_level!(DIGLETT, 10, Random, [ROCK_SLIDE, SLASH, DIG, NONE]),
        open_level!(TRAPINCH, 10, Random, [ROCK_SLIDE, EARTHQUAKE, GIGA_DRAIN, NONE]),
        open_level!(WYNAUT, 8, First, [DESTINY_BOND, SPLASH, COUNTER, MIRROR_COAT]),
        open_level!(DIGLETT, 8, Second, [EARTHQUAKE, ROCK_SLIDE, MAGNITUDE, TOXIC]),
        open_level!(TRAPINCH, 6, Second, [ROCK_SLIDE, EARTHQUAKE, GIGA_DRAIN, PROTECT]),
        open_level!(WYNAUT, 6, First, [COUNTER, MIRROR_COAT, DESTINY_BOND, NONE]),
        open_level!(WOBBUFFET, 5, First, [COUNTER, MIRROR_COAT, DESTINY_BOND, NONE]),
        open_level!(DUGTRIO, 5, Second, [ROCK_SLIDE, SLUDGE_BOMB, EARTHQUAKE, PROTECT]),
    ],
    [
        open_level!(GLALIE, 10, Random, [ICE_BEAM, CRUNCH, PROTECT, NONE]),
        open_level!(SNEASEL, 10, Random, [ICE_BEAM, CRUSH_CLAW, SPITE, NONE]),
        open_level!(DEWGONG, 8, Random, [BLIZZARD, DOUBLE_EDGE, SURF, NONE]),
        open_level!(PILOSWINE, 8, Random, [ICE_BEAM, EARTHQUAKE, TOXIC, NONE]),
        open_level!(JYNX, 6, Random, [BLIZZARD, LOVELY_KISS, PSYCHIC, NONE]),
        open_level!(CLOYSTER, 6, Random, [ICE_BEAM, SURF, PROTECT, NONE]),
        open_level!(WALREIN, 5, Random, [BLIZZARD, BODY_SLAM, SURF, NONE]),
        open_level!(LAPRAS, 5, Random, [SING, BODY_SLAM, ICE_BEAM, PSYCHIC]),
    ],
    [
        open_level!(WEEZING, 10, Random, [SELF_DESTRUCT, SLUDGE_BOMB, FIRE_BLAST, NONE]),
        open_level!(ELECTRODE, 10, Random, [SELF_DESTRUCT, THUNDERBOLT, ROLLOUT, NONE]),
        open_level!(GENGAR, 8, Random, [DESTINY_BOND, LICK, SHADOW_BALL, NONE]),
        open_level!(GOLEM, 8, Random, [SELF_DESTRUCT, PROTECT, EARTHQUAKE, NONE]),
        open_level!(PINECO, 6, Random, [EXPLOSION, DOUBLE_EDGE, GIGA_DRAIN, NONE]),
        open_level!(SOLROCK, 5, Random, [EXPLOSION, FIRE_SPIN, PSYWAVE, NONE]),
        open_level!(FORRETRESS, 5, Random, [EXPLOSION, TOXIC, ROCK_SLIDE, NONE]),
        open_level!(SHIFTRY, 5, Random, [EXPLOSION, GIGA_DRAIN, SOLAR_BEAM, PROTECT]),
    ],
    [
        open_level!(WOBBUFFET, 10, Random, [COUNTER, MIRROR_COAT, SAFEGUARD, DESTINY_BOND]),
        open_level!(METANG, 10, Random, [EARTHQUAKE, TOXIC, SLUDGE_BOMB, PSYCHIC]),
        open_level!(EXEGGUTOR, 8, Random, [EGG_BOMB, PSYCHIC, HYPNOSIS, NONE]),
        open_level!(SLOWKING, 8, Random, [SHADOW_BALL, SURF, ICE_BEAM, FLAMETHROWER]),
        open_level!(XATU, 6, Random, [CONFUSE_RAY, SHADOW_BALL, PSYCHIC, STEEL_WING]),
        open_level!(ALAKAZAM, 6, Random, [PSYCHIC, FIRE_PUNCH, ICE_PUNCH, TOXIC]),
        open_level!(STARMIE, 5, Random, [PSYCHIC, THUNDERBOLT, SURF, ICE_BEAM]),
        open_level!(ESPEON, 5, Random, [PSYCHIC, DIG, SHADOW_BALL, NONE]),
    ],
    [
        open_level!(GOLEM, 10, Random, [SELF_DESTRUCT, EARTHQUAKE, PROTECT, NONE]),
        open_level!(STEELIX, 10, Random, [IRON_TAIL, CRUNCH, EARTHQUAKE, NONE]),
        open_level!(OMASTAR, 8, Random, [SURF, MUD_SHOT, ANCIENT_POWER, NONE]),
        open_level!(LUNATONE, 8, Random, [HYPNOSIS, PSYWAVE, EXPLOSION, NONE]),
        open_level!(SHUCKLE, 6, Random, [TOXIC, PROTECT, WRAP, NONE]),
        open_level!(ARMALDO, 6, Random, [ANCIENT_POWER, PROTECT, AERIAL_ACE, NONE]),
        open_level!(CRADILY, 5, Random, [SLUDGE_BOMB, GIGA_DRAIN, CONFUSE_RAY, NONE]),
        open_level!(AERODACTYL, 5, Random, [HYPER_BEAM, ROCK_SLIDE, BITE, NONE]),
    ],
    [
        open_level!(POLIWRATH, 10, Random, [SUBMISSION, FOCUS_PUNCH, SURF, NONE]),
        open_level!(HARIYAMA, 10, Random, [FAKE_OUT, SURF, FOCUS_PUNCH, NONE]),
        open_level!(BRELOOM, 8, Random, [SPORE, FOCUS_PUNCH, PROTECT, NONE]),
        open_level!(MEDICHAM, 8, Random, [THUNDER_PUNCH, FIRE_PUNCH, ICE_PUNCH, FOCUS_PUNCH]),
        open_level!(HITMONCHAN, 6, Random, [THUNDER_PUNCH, FIRE_PUNCH, ICE_PUNCH, FOCUS_PUNCH]),
        open_level!(HITMONLEE, 6, Random, [MEGA_KICK, FOCUS_PUNCH, NONE, NONE]),
        open_level!(HERACROSS, 5, Random, [MEGAHORN, EARTHQUAKE, FOCUS_PUNCH, ROCK_SLIDE]),
        open_level!(MACHAMP, 5, Random, [ROCK_SLIDE, EARTHQUAKE, FOCUS_PUNCH, SEISMIC_TOSS]),
    ],
    [
        open_level!(QUAGSIRE, 10, Random, [RAIN_DANCE, SURF, PROTECT, NONE]),
        open_level!(TROPIUS, 10, Random, [SUNNY_DAY, SOLAR_BEAM, NONE, NONE]),
        open_level!(PUPITAR, 8, Random, [SANDSTORM, EARTHQUAKE, ROCK_SLIDE, NONE]),
        open_level!(LAPRAS, 8, Random, [HAIL, ICE_BEAM, NONE, NONE]),
        open_level!(CACTURNE, 6, Random, [SANDSTORM, GIGA_DRAIN, SOLAR_BEAM, NONE]),
        open_level!(FLAREON, 6, Random, [SUNNY_DAY, FLAMETHROWER, PROTECT, NONE]),
        open_level!(WALREIN, 5, Random, [HAIL, ICE_BEAM, NONE, NONE]),
        open_level!(GYARADOS, 5, Random, [RAIN_DANCE, THUNDER, HYDRO_PUMP, NONE]),
    ],
    [
        open_level!(PINECO, 10, Random, [EXPLOSION, TAKE_DOWN, NONE, NONE]),
        open_level!(SHUCKLE, 8, Random, [TOXIC, EARTHQUAKE, PROTECT, NONE]),
        open_level!(VENOMOTH, 8, Random, [SILVER_WIND, POISON_POWDER, SLEEP_POWDER, PSYCHIC]),
        open_level!(SCIZOR, 6, Random, [QUICK_ATTACK, METAL_CLAW, FURY_CUTTER, PURSUIT]),
        open_level!(HERACROSS, 6, Random, [MEGAHORN, BRICK_BREAK, EARTHQUAKE, ROCK_SLIDE]),
        open_level!(FORRETRESS, 5, Random, [EXPLOSION, EARTHQUAKE, PROTECT, NONE]),
        open_level!(ARMALDO, 5, Random, [WATER_PULSE, PROTECT, ROCK_SLIDE, NONE]),
        open_level!(SHEDINJA, 5, Random, [SHADOW_BALL, TOXIC, SPITE, GRUDGE]),
    ],
    [
        open_level!(SABLEYE, 10, Random, [NIGHT_SHADE, PSYCHIC, AERIAL_ACE, NONE]),
        open_level!(SNEASEL, 10, Random, [ICE_BEAM, TAUNT, FAINT_ATTACK, QUICK_ATTACK]),
        open_level!(CRAWDAUNT, 8, Random, [CRABHAMMER, ICE_BEAM, SURF, NONE]),
        open_level!(SHIFTRY, 8, Random, [EXPLOSION, SHADOW_BALL, AERIAL_ACE, GIGA_DRAIN]),
        open_level!(CACTURNE, 6, Random, [TOXIC, GIGA_DRAIN, NEEDLE_ARM, NONE]),
        open_level!(ABSOL, 6, Random, [BITE, PROTECT, SLASH, NONE]),
        open_level!(HOUNDOOM, 5, Random, [OVERHEAT, CRUNCH, SHADOW_BALL, PROTECT]),
        open_level!(UMBREON, 5, Random, [PSYCHIC, SHADOW_BALL, IRON_TAIL, QUICK_ATTACK]),
    ],
    [
        open_level!(OCTILLERY, 10, Random, [OCTAZOOKA, ICE_BEAM, FIRE_BLAST, NONE]),
        open_level!(DEWGONG, 10, Random, [WATER_PULSE, ICE_BEAM, HEADBUTT, NONE]),
        open_level!(PELIPPER, 8, Random, [PROTECT, SUPERSONIC, SURF, NONE]),
        open_level!(QUAGSIRE, 8, Random, [EARTHQUAKE, ROCK_TOMB, SURF, NONE]),
        open_level!(LUDICOLO, 6, Random, [PROTECT, SOLAR_BEAM, TOXIC, ICE_BEAM]),
        open_level!(SLOWKING, 6, Random, [PSYCHIC, HEADBUTT, SWAGGER, NONE]),
        open_level!(STARMIE, 5, Random, [WATER_PULSE, THUNDERBOLT, CONFUSE_RAY, BLIZZARD]),
        open_level!(BLASTOISE, 5, Random, [HYDRO_PUMP, BITE, ICE_BEAM, NONE]),
    ],
    [
        open_level!(DUSKULL, 10, Random, [NIGHT_SHADE, WILL_O_WISP, SHADOW_BALL, PROTECT]),
        open_level!(HAUNTER, 10, Random, [TOXIC, SPITE, HYPNOSIS, SHADOW_BALL]),
        open_level!(BANETTE, 8, Random, [SHADOW_BALL, SPITE, WILL_O_WISP, NONE]),
        open_level!(MISDREAVUS, 8, Random, [PERISH_SONG, SPITE, MEAN_LOOK, NONE]),
        open_level!(SABLEYE, 6, Random, [SHADOW_BALL, MEAN_LOOK, DIG, NIGHT_SHADE]),
        open_level!(DUSCLOPS, 6, Random, [WILL_O_WISP, TOXIC, SHADOW_BALL, NONE]),
        open_level!(SHEDINJA, 5, Random, [SHADOW_BALL, SPITE, GRUDGE, PROTECT]),
        open_level!(GENGAR, 5, Random, [PSYCHIC, DESTINY_BOND, SPITE, NIGHT_SHADE]),
    ],
    [
        open_level!(MAWILE, 10, Random, [CRUNCH, TOXIC, ICE_BEAM, NONE]),
        open_level!(MAGNETON, 10, Random, [THUNDERBOLT, THUNDER_WAVE, NONE, NONE]),
        open_level!(STEELIX, 8, Random, [ROCK_THROW, DOUBLE_EDGE, EARTHQUAKE, NONE]),
        open_level!(SCIZOR, 8, Random, [METAL_CLAW, SLASH, NONE, NONE]),
        open_level!(FORRETRESS, 6, Random, [EXPLOSION, TOXIC, NONE, NONE]),
        open_level!(SKARMORY, 6, Random, [STEEL_WING, TOXIC, FLY, PROTECT]),
        open_level!(AGGRON, 5, Random, [EARTHQUAKE, TAKE_DOWN, SURF, ICE_BEAM]),
        open_level!(METAGROSS, 5, Random, [EARTHQUAKE, PSYCHIC, SHADOW_BALL, BRICK_BREAK]),
    ],
    [
        open_level!(DRAGONAIR, 10, Random, [THUNDER_WAVE, TOXIC, ICE_BEAM, NONE]),
        open_level!(VIBRAVA, 10, Random, [EARTHQUAKE, DRAGON_BREATH, CRUNCH, STEEL_WING]),
        open_level!(ALTARIA, 8, Random, [EARTHQUAKE, DRAGON_CLAW, SING, PROTECT]),
        open_level!(FLYGON, 8, Random, [EARTHQUAKE, DRAGON_CLAW, FIRE_BLAST, NONE]),
        open_level!(AERODACTYL, 6, Random, [EARTHQUAKE, ROCK_SLIDE, DRAGON_CLAW, NONE]),
        open_level!(GYARADOS, 6, Random, [EARTHQUAKE, SURF, THRASH, BITE]),
        open_level!(KINGDRA, 5, Random, [HYDRO_PUMP, ICE_BEAM, PROTECT, NONE]),
        open_level!(CHARIZARD, 5, Random, [FLAMETHROWER, FOCUS_PUNCH, FIRE_BLAST, IRON_TAIL]),
    ],
    [
        open_level!(ARCANINE, 10, Random, [FIRE_BLAST, TAKE_DOWN, NONE, NONE]),
        open_level!(POLIWRATH, 10, Random, [HYDRO_PUMP, ICE_BEAM, NONE, NONE]),
        open_level!(RAICHU, 8, Random, [THUNDER, THUNDER_WAVE, SLAM, NONE]),
        open_level!(VAPOREON, 8, Random, [SURF, ICE_BEAM, NONE, NONE]),
        open_level!(JOLTEON, 6, Random, [THUNDERBOLT, PIN_MISSILE, NONE, NONE]),
        open_level!(FLAREON, 6, Random, [FLAMETHROWER, BITE, NONE, NONE]),
        open_level!(NINETALES, 5, Random, [FLAMETHROWER, WILL_O_WISP, PROTECT, NONE]),
        open_level!(STARMIE, 5, Random, [ICE_BEAM, SURF, THUNDERBOLT, PSYCHIC]),
    ],
    [
        open_level!(KANGASKHAN, 10, Random, [HYPER_BEAM, FLAMETHROWER, SURF, DIZZY_PUNCH]),
        open_level!(SWELLOW, 10, Random, [AERIAL_ACE, HYPER_BEAM, TOXIC, NONE]),
        open_level!(URSARING, 8, Random, [HYPER_BEAM, EARTHQUAKE, FOCUS_PUNCH, PROTECT]),
        open_level!(PORYGON2, 8, Random, [PSYBEAM, HYPER_BEAM, SHADOW_BALL, ICE_BEAM]),
        open_level!(TAUROS, 6, Random, [EARTHQUAKE, HYPER_BEAM, SURF, THUNDERBOLT]),
        open_level!(FEAROW, 6, Random, [HYPER_BEAM, FLY, MIRROR_MOVE, PROTECT]),
        open_level!(SNORLAX, 5, Random, [HYPER_BEAM, BODY_SLAM, SHADOW_BALL, EARTHQUAKE]),
        open_level!(SLAKING, 5, Random, [HYPER_BEAM, EARTHQUAKE, SHADOW_BALL, ICE_BEAM]),
    ],
];

#[cfg(test)]
mod tests {
    use super::*;

    /// One record as upstream's numeric values: `species`, `lvl`,
    /// `abilityNum`, and the four `moves`.
    type Row = (u16, u8, u8, [u16; PYRAMID_WILD_MON_MOVES]);
    type Fixture = [[Row; PYRAMID_WILD_MON_SLOTS]; PYRAMID_ROUNDS];

    /// `sLevel50WildMonPointers`' rounds as `SPECIES_*`, `lvl`, `abilityNum`
    /// (`ABILITY_RANDOM` is `2`), and `MOVE_*` values from
    /// `include/constants/species.h` and `moves.h`.
    #[rustfmt::skip]
    const EXPECTED_LEVEL_50: Fixture = [
    [
        (353, 35, 2, [86, 209, 227, 0]),
        (354, 35, 2, [86, 85, 98, 0]),
        (25, 37, 2, [86, 85, 21, 0]),
        (125, 37, 2, [9, 129, 103, 0]),
        (45, 39, 2, [78, 202, 182, 0]),
        (338, 39, 2, [86, 87, 98, 0]),
        (307, 40, 2, [78, 264, 202, 183]),
        (135, 40, 2, [86, 87, 42, 98]),
    ],
    [
        (367, 36, 2, [92, 124, 182, 0]),
        (363, 36, 2, [92, 202, 345, 80]),
        (12, 38, 2, [77, 16, 60, 0]),
        (379, 38, 2, [305, 207, 242, 342]),
        (227, 40, 2, [92, 19, 211, 0]),
        (297, 40, 1, [92, 182, 291, 240]),
        (169, 41, 2, [92, 109, 212, 44]),
        (94, 41, 2, [92, 325, 101, 0]),
    ],
    [
        (58, 37, 2, [172, 36, 0, 0]),
        (37, 37, 2, [261, 53, 0, 0]),
        (219, 39, 1, [53, 157, 182, 0]),
        (38, 39, 2, [261, 98, 53, 0]),
        (357, 41, 2, [7, 136, 0, 0]),
        (110, 41, 2, [261, 53, 182, 0]),
        (362, 42, 2, [261, 109, 212, 325]),
        (229, 42, 2, [53, 44, 76, 315]),
    ],
    [
        (206, 42, 2, [180, 92, 182, 0]),
        (378, 41, 2, [288, 261, 101, 0]),
        (200, 42, 2, [288, 180, 247, 0]),
        (38, 42, 2, [288, 261, 315, 0]),
        (376, 44, 2, [44, 332, 247, 182]),
        (362, 44, 2, [261, 182, 92, 247]),
        (303, 45, 2, [288, 92, 180, 0]),
        (94, 45, 2, [288, 180, 101, 0]),
    ],
    [
        (93, 41, 2, [101, 85, 188, 0]),
        (411, 41, 2, [38, 92, 94, 182]),
        (349, 42, 2, [89, 157, 126, 92]),
        (200, 42, 2, [94, 180, 247, 220]),
        (319, 44, 2, [89, 246, 120, 94]),
        (110, 44, 2, [188, 120, 182, 0]),
        (334, 45, 2, [89, 242, 337, 225]),
        (94, 45, 2, [85, 94, 202, 101]),
    ],
    [
        (50, 41, 2, [157, 163, 91, 0]),
        (332, 41, 2, [157, 89, 202, 0]),
        (360, 42, 0, [194, 150, 68, 243]),
        (50, 42, 2, [89, 157, 222, 92]),
        (332, 44, 2, [157, 89, 202, 182]),
        (360, 44, 0, [68, 243, 194, 0]),
        (202, 45, 0, [68, 243, 194, 0]),
        (51, 45, 1, [157, 188, 89, 182]),
    ],
    [
        (347, 41, 2, [58, 242, 182, 0]),
        (215, 41, 2, [58, 306, 180, 0]),
        (87, 42, 2, [59, 38, 57, 0]),
        (221, 42, 2, [58, 89, 92, 0]),
        (124, 44, 2, [59, 142, 94, 0]),
        (91, 44, 2, [58, 57, 182, 0]),
        (343, 45, 2, [59, 34, 57, 0]),
        (131, 45, 2, [47, 34, 58, 94]),
    ],
    [
        (110, 41, 2, [120, 188, 126, 0]),
        (101, 41, 2, [120, 85, 205, 0]),
        (94, 42, 2, [194, 122, 247, 0]),
        (76, 42, 2, [120, 182, 89, 0]),
        (204, 44, 2, [153, 38, 202, 0]),
        (349, 44, 2, [153, 83, 149, 0]),
        (205, 45, 2, [153, 92, 157, 0]),
        (300, 45, 2, [153, 202, 76, 182]),
    ],
    [
        (202, 41, 2, [68, 243, 219, 194]),
        (399, 41, 2, [89, 92, 188, 94]),
        (103, 42, 2, [121, 94, 95, 0]),
        (199, 42, 2, [247, 57, 58, 53]),
        (178, 44, 2, [109, 247, 94, 211]),
        (65, 44, 2, [94, 7, 8, 92]),
        (121, 45, 2, [94, 85, 57, 58]),
        (196, 45, 2, [94, 91, 247, 0]),
    ],
    [
        (76, 41, 2, [120, 89, 182, 0]),
        (208, 41, 2, [231, 242, 89, 0]),
        (139, 42, 2, [57, 341, 246, 0]),
        (348, 42, 2, [95, 149, 153, 0]),
        (213, 44, 2, [92, 182, 35, 0]),
        (391, 44, 2, [246, 182, 332, 0]),
        (389, 45, 2, [188, 202, 109, 0]),
        (142, 45, 2, [63, 157, 44, 0]),
    ],
    [
        (62, 41, 2, [66, 264, 57, 0]),
        (336, 41, 2, [252, 57, 264, 0]),
        (307, 42, 2, [147, 264, 182, 0]),
        (357, 42, 2, [9, 7, 8, 264]),
        (107, 44, 2, [9, 7, 8, 264]),
        (106, 44, 2, [25, 264, 0, 0]),
        (214, 45, 2, [224, 89, 264, 157]),
        (68, 50, 2, [157, 89, 264, 69]),
    ],
    [
        (195, 41, 2, [240, 57, 182, 0]),
        (369, 41, 2, [241, 76, 0, 0]),
        (247, 42, 2, [201, 89, 157, 0]),
        (131, 42, 2, [258, 58, 0, 0]),
        (345, 44, 2, [201, 202, 76, 0]),
        (136, 44, 2, [241, 53, 182, 0]),
        (343, 45, 2, [258, 58, 0, 0]),
        (130, 50, 2, [240, 87, 56, 0]),
    ],
    [
        (204, 42, 2, [153, 36, 0, 0]),
        (213, 42, 2, [92, 89, 182, 0]),
        (49, 46, 2, [318, 77, 79, 94]),
        (212, 46, 2, [98, 232, 210, 228]),
        (214, 48, 2, [224, 280, 89, 157]),
        (205, 48, 2, [153, 89, 182, 0]),
        (391, 50, 2, [352, 182, 157, 0]),
        (303, 50, 2, [247, 92, 180, 288]),
    ],
    [
        (322, 42, 2, [101, 94, 332, 0]),
        (215, 42, 2, [58, 269, 185, 98]),
        (327, 46, 2, [152, 58, 57, 0]),
        (300, 46, 2, [153, 247, 332, 202]),
        (345, 48, 2, [92, 202, 302, 0]),
        (376, 48, 2, [44, 182, 163, 0]),
        (229, 50, 2, [315, 242, 247, 182]),
        (197, 50, 2, [94, 247, 231, 98]),
    ],
    [
        (224, 42, 2, [190, 58, 126, 0]),
        (87, 42, 2, [352, 58, 29, 0]),
        (310, 46, 2, [182, 48, 57, 0]),
        (195, 46, 2, [89, 317, 57, 0]),
        (297, 48, 2, [182, 76, 92, 58]),
        (199, 48, 2, [94, 29, 207, 0]),
        (121, 50, 2, [352, 85, 109, 59]),
        (9, 50, 2, [56, 44, 58, 0]),
    ],
    [
        (361, 42, 2, [101, 261, 247, 182]),
        (93, 42, 2, [92, 180, 95, 247]),
        (378, 46, 2, [247, 180, 261, 0]),
        (200, 46, 2, [195, 180, 212, 0]),
        (322, 48, 2, [247, 212, 91, 101]),
        (362, 48, 2, [261, 92, 247, 0]),
        (303, 50, 2, [247, 180, 288, 182]),
        (94, 50, 2, [94, 194, 180, 101]),
    ],
    [
        (355, 42, 2, [242, 92, 58, 0]),
        (82, 42, 2, [85, 86, 0, 0]),
        (208, 46, 2, [88, 38, 89, 0]),
        (212, 46, 2, [232, 163, 0, 0]),
        (205, 48, 2, [153, 92, 0, 0]),
        (227, 48, 2, [211, 92, 19, 182]),
        (384, 50, 2, [89, 36, 57, 58]),
        (400, 50, 2, [89, 94, 247, 280]),
    ],
    [
        (148, 42, 2, [86, 92, 58, 0]),
        (333, 42, 2, [89, 225, 242, 211]),
        (359, 46, 2, [89, 337, 47, 182]),
        (334, 46, 2, [89, 337, 126, 0]),
        (142, 48, 2, [89, 157, 337, 0]),
        (130, 48, 2, [89, 57, 37, 44]),
        (230, 50, 2, [56, 58, 182, 0]),
        (6, 50, 2, [53, 264, 126, 231]),
    ],
    [
        (59, 42, 2, [126, 36, 0, 0]),
        (62, 42, 2, [56, 58, 0, 0]),
        (26, 46, 2, [87, 86, 21, 0]),
        (134, 46, 2, [57, 58, 0, 0]),
        (135, 48, 2, [85, 42, 0, 0]),
        (136, 48, 2, [53, 44, 0, 0]),
        (38, 50, 2, [53, 261, 182, 0]),
        (121, 50, 2, [58, 57, 85, 94]),
    ],
    [
        (115, 42, 2, [63, 53, 57, 146]),
        (305, 42, 2, [332, 63, 92, 0]),
        (217, 46, 2, [63, 89, 264, 182]),
        (233, 46, 2, [60, 63, 247, 58]),
        (128, 48, 2, [89, 63, 57, 85]),
        (22, 48, 2, [63, 19, 119, 182]),
        (143, 50, 2, [63, 34, 247, 89]),
        (366, 50, 2, [63, 89, 247, 58]),
    ],
    ];

    /// `sOpenLevelWildMonPointers`' rounds, in the same form; `lvl` is the
    /// offset subtracted from the facility level.
    #[rustfmt::skip]
    const EXPECTED_OPEN_LEVEL: Fixture = [
    [
        (353, 15, 2, [86, 209, 227, 0]),
        (354, 15, 2, [86, 85, 98, 0]),
        (25, 13, 2, [86, 85, 21, 0]),
        (125, 13, 2, [9, 129, 103, 0]),
        (45, 8, 2, [78, 202, 182, 0]),
        (338, 8, 2, [86, 87, 98, 0]),
        (307, 6, 2, [78, 264, 202, 183]),
        (135, 6, 2, [86, 87, 42, 98]),
    ],
    [
        (367, 14, 2, [92, 124, 182, 0]),
        (363, 14, 2, [92, 202, 345, 80]),
        (12, 12, 2, [77, 16, 60, 0]),
        (379, 12, 2, [305, 207, 242, 342]),
        (227, 7, 2, [92, 19, 211, 0]),
        (297, 7, 1, [92, 182, 291, 240]),
        (169, 5, 2, [92, 109, 212, 44]),
        (94, 5, 2, [92, 325, 101, 0]),
    ],
    [
        (58, 13, 2, [172, 36, 0, 0]),
        (37, 13, 2, [261, 53, 0, 0]),
        (219, 11, 1, [53, 157, 182, 0]),
        (38, 11, 2, [261, 98, 53, 0]),
        (357, 6, 2, [7, 136, 0, 0]),
        (110, 6, 2, [261, 53, 182, 0]),
        (362, 5, 2, [261, 109, 212, 325]),
        (229, 5, 2, [53, 44, 76, 315]),
    ],
    [
        (206, 10, 2, [180, 92, 182, 0]),
        (378, 8, 2, [288, 261, 101, 0]),
        (200, 8, 2, [288, 180, 247, 0]),
        (38, 6, 2, [288, 261, 315, 0]),
        (376, 10, 2, [44, 332, 247, 182]),
        (362, 6, 2, [261, 182, 92, 247]),
        (303, 5, 2, [288, 92, 180, 0]),
        (94, 5, 2, [288, 180, 101, 0]),
    ],
    [
        (93, 10, 2, [101, 85, 188, 0]),
        (411, 10, 2, [38, 92, 94, 182]),
        (349, 8, 2, [89, 157, 126, 92]),
        (200, 8, 2, [94, 180, 247, 220]),
        (319, 6, 2, [89, 246, 120, 94]),
        (110, 6, 2, [188, 120, 182, 0]),
        (334, 5, 2, [89, 242, 337, 225]),
        (94, 5, 2, [85, 94, 202, 101]),
    ],
    [
        (50, 10, 2, [157, 163, 91, 0]),
        (332, 10, 2, [157, 89, 202, 0]),
        (360, 8, 0, [194, 150, 68, 243]),
        (50, 8, 1, [89, 157, 222, 92]),
        (332, 6, 1, [157, 89, 202, 182]),
        (360, 6, 0, [68, 243, 194, 0]),
        (202, 5, 0, [68, 243, 194, 0]),
        (51, 5, 1, [157, 188, 89, 182]),
    ],
    [
        (347, 10, 2, [58, 242, 182, 0]),
        (215, 10, 2, [58, 306, 180, 0]),
        (87, 8, 2, [59, 38, 57, 0]),
        (221, 8, 2, [58, 89, 92, 0]),
        (124, 6, 2, [59, 142, 94, 0]),
        (91, 6, 2, [58, 57, 182, 0]),
        (343, 5, 2, [59, 34, 57, 0]),
        (131, 5, 2, [47, 34, 58, 94]),
    ],
    [
        (110, 10, 2, [120, 188, 126, 0]),
        (101, 10, 2, [120, 85, 205, 0]),
        (94, 8, 2, [194, 122, 247, 0]),
        (76, 8, 2, [120, 182, 89, 0]),
        (204, 6, 2, [153, 38, 202, 0]),
        (349, 5, 2, [153, 83, 149, 0]),
        (205, 5, 2, [153, 92, 157, 0]),
        (300, 5, 2, [153, 202, 76, 182]),
    ],
    [
        (202, 10, 2, [68, 243, 219, 194]),
        (399, 10, 2, [89, 92, 188, 94]),
        (103, 8, 2, [121, 94, 95, 0]),
        (199, 8, 2, [247, 57, 58, 53]),
        (178, 6, 2, [109, 247, 94, 211]),
        (65, 6, 2, [94, 7, 8, 92]),
        (121, 5, 2, [94, 85, 57, 58]),
        (196, 5, 2, [94, 91, 247, 0]),
    ],
    [
        (76, 10, 2, [120, 89, 182, 0]),
        (208, 10, 2, [231, 242, 89, 0]),
        (139, 8, 2, [57, 341, 246, 0]),
        (348, 8, 2, [95, 149, 153, 0]),
        (213, 6, 2, [92, 182, 35, 0]),
        (391, 6, 2, [246, 182, 332, 0]),
        (389, 5, 2, [188, 202, 109, 0]),
        (142, 5, 2, [63, 157, 44, 0]),
    ],
    [
        (62, 10, 2, [66, 264, 57, 0]),
        (336, 10, 2, [252, 57, 264, 0]),
        (307, 8, 2, [147, 264, 182, 0]),
        (357, 8, 2, [9, 7, 8, 264]),
        (107, 6, 2, [9, 7, 8, 264]),
        (106, 6, 2, [25, 264, 0, 0]),
        (214, 5, 2, [224, 89, 264, 157]),
        (68, 5, 2, [157, 89, 264, 69]),
    ],
    [
        (195, 10, 2, [240, 57, 182, 0]),
        (369, 10, 2, [241, 76, 0, 0]),
        (247, 8, 2, [201, 89, 157, 0]),
        (131, 8, 2, [258, 58, 0, 0]),
        (345, 6, 2, [201, 202, 76, 0]),
        (136, 6, 2, [241, 53, 182, 0]),
        (343, 5, 2, [258, 58, 0, 0]),
        (130, 5, 2, [240, 87, 56, 0]),
    ],
    [
        (204, 10, 2, [153, 36, 0, 0]),
        (213, 8, 2, [92, 89, 182, 0]),
        (49, 8, 2, [318, 77, 79, 94]),
        (212, 6, 2, [98, 232, 210, 228]),
        (214, 6, 2, [224, 280, 89, 157]),
        (205, 5, 2, [153, 89, 182, 0]),
        (391, 5, 2, [352, 182, 157, 0]),
        (303, 5, 2, [247, 92, 180, 288]),
    ],
    [
        (322, 10, 2, [101, 94, 332, 0]),
        (215, 10, 2, [58, 269, 185, 98]),
        (327, 8, 2, [152, 58, 57, 0]),
        (300, 8, 2, [153, 247, 332, 202]),
        (345, 6, 2, [92, 202, 302, 0]),
        (376, 6, 2, [44, 182, 163, 0]),
        (229, 5, 2, [315, 242, 247, 182]),
        (197, 5, 2, [94, 247, 231, 98]),
    ],
    [
        (224, 10, 2, [190, 58, 126, 0]),
        (87, 10, 2, [352, 58, 29, 0]),
        (310, 8, 2, [182, 48, 57, 0]),
        (195, 8, 2, [89, 317, 57, 0]),
        (297, 6, 2, [182, 76, 92, 58]),
        (199, 6, 2, [94, 29, 207, 0]),
        (121, 5, 2, [352, 85, 109, 59]),
        (9, 5, 2, [56, 44, 58, 0]),
    ],
    [
        (361, 10, 2, [101, 261, 247, 182]),
        (93, 10, 2, [92, 180, 95, 247]),
        (378, 8, 2, [247, 180, 261, 0]),
        (200, 8, 2, [195, 180, 212, 0]),
        (322, 6, 2, [247, 212, 91, 101]),
        (362, 6, 2, [261, 92, 247, 0]),
        (303, 5, 2, [247, 180, 288, 182]),
        (94, 5, 2, [94, 194, 180, 101]),
    ],
    [
        (355, 10, 2, [242, 92, 58, 0]),
        (82, 10, 2, [85, 86, 0, 0]),
        (208, 8, 2, [88, 38, 89, 0]),
        (212, 8, 2, [232, 163, 0, 0]),
        (205, 6, 2, [153, 92, 0, 0]),
        (227, 6, 2, [211, 92, 19, 182]),
        (384, 5, 2, [89, 36, 57, 58]),
        (400, 5, 2, [89, 94, 247, 280]),
    ],
    [
        (148, 10, 2, [86, 92, 58, 0]),
        (333, 10, 2, [89, 225, 242, 211]),
        (359, 8, 2, [89, 337, 47, 182]),
        (334, 8, 2, [89, 337, 126, 0]),
        (142, 6, 2, [89, 157, 337, 0]),
        (130, 6, 2, [89, 57, 37, 44]),
        (230, 5, 2, [56, 58, 182, 0]),
        (6, 5, 2, [53, 264, 126, 231]),
    ],
    [
        (59, 10, 2, [126, 36, 0, 0]),
        (62, 10, 2, [56, 58, 0, 0]),
        (26, 8, 2, [87, 86, 21, 0]),
        (134, 8, 2, [57, 58, 0, 0]),
        (135, 6, 2, [85, 42, 0, 0]),
        (136, 6, 2, [53, 44, 0, 0]),
        (38, 5, 2, [53, 261, 182, 0]),
        (121, 5, 2, [58, 57, 85, 94]),
    ],
    [
        (115, 10, 2, [63, 53, 57, 146]),
        (305, 10, 2, [332, 63, 92, 0]),
        (217, 8, 2, [63, 89, 264, 182]),
        (233, 8, 2, [60, 63, 247, 58]),
        (128, 6, 2, [89, 63, 57, 85]),
        (22, 6, 2, [63, 19, 119, 182]),
        (143, 5, 2, [63, 34, 247, 89]),
        (366, 5, 2, [63, 89, 247, 58]),
    ],
    ];

    /// The species of slot 0 of each round, in pointer-table order.
    const L50_ROUND_LEAD_SPECIES: [u16; PYRAMID_ROUNDS] = [
        353, 367, 58, 206, 93, 50, 347, 110, 202, 76, 62, 195, 204, 322, 224, 361, 355, 148, 59,
        115,
    ];
    const OPEN_ROUND_LEAD_SPECIES: [u16; PYRAMID_ROUNDS] = [
        353, 367, 58, 206, 93, 50, 347, 110, 202, 76, 62, 195, 204, 322, 224, 361, 355, 148, 59,
        115,
    ];

    fn expected(mode: PyramidLevelMode) -> &'static Fixture {
        match mode {
            PyramidLevelMode::Level50 => &EXPECTED_LEVEL_50,
            PyramidLevelMode::OpenLevel => &EXPECTED_OPEN_LEVEL,
        }
    }

    fn raw_level(mon: &PyramidWildMon) -> u8 {
        match mon.level() {
            PyramidWildLevel::Level50(level) => level.level(),
            PyramidWildLevel::OpenLevelOffset(offset) => offset.offset(),
        }
    }

    fn check_every_record(mode: PyramidLevelMode) {
        let mut checked = 0;
        for (round, rows) in expected(mode).iter().enumerate() {
            for (slot, (species, lvl, ability, moves)) in rows.iter().enumerate() {
                let mon = pyramid_wild_mon(mode, round, slot).expect("position is in range");
                let context = format!("{mode:?} round {round} slot {slot}");
                assert_eq!(mon.species(), SpeciesId(*species), "{context}");
                assert_eq!(mon.level().mode(), mode, "{context}");
                assert_eq!(raw_level(mon), *lvl, "{context}");
                assert_eq!(mon.ability_selector().raw(), *ability, "{context}");
                assert_eq!(moves.map(MoveId), mon.moves(), "{context}");
                checked += 1;
            }
        }
        assert_eq!(checked, 160);
    }

    #[test]
    fn level_50_records_match_upstream() {
        check_every_record(PyramidLevelMode::Level50);
    }

    #[test]
    fn open_level_records_match_upstream() {
        check_every_record(PyramidLevelMode::OpenLevel);
    }

    #[test]
    fn each_round_holds_eight_records_in_both_modes() {
        assert_eq!(PYRAMID_ROUNDS, 20);
        assert_eq!(PYRAMID_WILD_MON_SLOTS, 8);
        for mode in [PyramidLevelMode::Level50, PyramidLevelMode::OpenLevel] {
            let mut records = 0;
            for round in 0..20 {
                let slots = pyramid_round(mode, round).expect("round is in range");
                assert_eq!(slots.len(), 8, "{mode:?} round {round}");
                records += slots.len();
            }
            assert_eq!(records, 160, "{mode:?}");
        }
    }

    #[test]
    fn rounds_follow_the_pointer_table_order() {
        for (round, species) in L50_ROUND_LEAD_SPECIES.iter().enumerate() {
            let lead = pyramid_wild_mon(PyramidLevelMode::Level50, round, 0).unwrap();
            assert_eq!(
                lead.species(),
                SpeciesId(*species),
                "Level 50 round {round}"
            );
        }
        for (round, species) in OPEN_ROUND_LEAD_SPECIES.iter().enumerate() {
            let lead = pyramid_wild_mon(PyramidLevelMode::OpenLevel, round, 0).unwrap();
            assert_eq!(
                lead.species(),
                SpeciesId(*species),
                "Open Level round {round}"
            );
        }
        // Round 1 of each mode opens with Plusle, round 20 with Kangaskhan.
        assert_eq!(L50_ROUND_LEAD_SPECIES[0], 353);
        assert_eq!(OPEN_ROUND_LEAD_SPECIES[0], 353);
        assert_eq!(L50_ROUND_LEAD_SPECIES[19], 115);
        assert_eq!(OPEN_ROUND_LEAD_SPECIES[19], 115);
    }

    #[test]
    fn lookup_matches_hand_read_upstream_records() {
        use PyramidAbilitySelector::{First, Random, Second};

        let plusle = pyramid_wild_mon(PyramidLevelMode::Level50, 0, 0).unwrap();
        assert_eq!(plusle.species(), SpeciesId::PLUSLE);
        assert_eq!(plusle.species().index(), 353);
        assert_eq!(plusle.level().level().map(PyramidMonLevel::level), Some(35));
        assert_eq!(plusle.level().offset(), None);
        assert_eq!(plusle.ability_selector(), Random);
        assert_eq!(plusle.moves().map(MoveId::index), [86, 209, 227, 0]);

        let plusle_open = pyramid_wild_mon(PyramidLevelMode::OpenLevel, 0, 0).unwrap();
        assert_eq!(plusle_open.species(), SpeciesId::PLUSLE);
        assert_eq!(plusle_open.level().level(), None);
        assert_eq!(
            plusle_open.level().offset().map(PyramidLevelOffset::offset),
            Some(15)
        );
        assert_eq!(plusle_open.ability_selector(), Random);
        assert_eq!(plusle_open.moves(), plusle.moves());

        let slaking = pyramid_wild_mon(PyramidLevelMode::Level50, 19, 7).unwrap();
        assert_eq!(slaking.species().index(), 366);
        assert_eq!(raw_level(slaking), 50);
        assert_eq!(slaking.moves().map(MoveId::index), [63, 89, 247, 58]);
        let slaking_open = pyramid_wild_mon(PyramidLevelMode::OpenLevel, 19, 7).unwrap();
        assert_eq!(slaking_open.species(), slaking.species());
        assert_eq!(raw_level(slaking_open), 5);
        assert_eq!(slaking_open.moves(), slaking.moves());

        // The ability selector is not always `ABILITY_RANDOM`, and the two
        // modes do not always agree on it.
        let wynaut = pyramid_wild_mon(PyramidLevelMode::Level50, 5, 2).unwrap();
        assert_eq!(wynaut.ability_selector(), First);
        assert_eq!(raw_level(wynaut), 42);
        let wynaut_open = pyramid_wild_mon(PyramidLevelMode::OpenLevel, 5, 2).unwrap();
        assert_eq!(wynaut_open.ability_selector(), First);
        assert_eq!(raw_level(wynaut_open), 8);
        for slot in [3, 4] {
            let level_50 = pyramid_wild_mon(PyramidLevelMode::Level50, 5, slot).unwrap();
            let open = pyramid_wild_mon(PyramidLevelMode::OpenLevel, 5, slot).unwrap();
            assert_eq!(level_50.species(), open.species());
            assert_eq!(level_50.ability_selector(), Random, "slot {slot}");
            assert_eq!(open.ability_selector(), Second, "slot {slot}");
        }
    }

    #[test]
    fn both_modes_use_each_selector() {
        for mode in [PyramidLevelMode::Level50, PyramidLevelMode::OpenLevel] {
            let mut counts = [0_usize; 3];
            for round in 0..PYRAMID_ROUNDS {
                for mon in pyramid_round(mode, round).unwrap() {
                    counts[usize::from(mon.ability_selector().raw())] += 1;
                }
            }
            assert_eq!(counts.iter().sum::<usize>(), 160, "{mode:?}");
            assert!(
                counts.iter().all(|count| *count > 0),
                "{mode:?}: {counts:?}"
            );
        }
    }

    #[test]
    fn open_level_values_are_offsets_not_levels() {
        for round in 0..PYRAMID_ROUNDS {
            for slot in 0..PYRAMID_WILD_MON_SLOTS {
                let level_50 = pyramid_wild_mon(PyramidLevelMode::Level50, round, slot).unwrap();
                let open = pyramid_wild_mon(PyramidLevelMode::OpenLevel, round, slot).unwrap();
                assert!(level_50.level().level().is_some() && level_50.level().offset().is_none());
                assert!(open.level().level().is_none() && open.level().offset().is_some());
                assert_ne!(level_50.level(), open.level());
                assert_eq!(level_50.level().mode(), PyramidLevelMode::Level50);
                assert_eq!(open.level().mode(), PyramidLevelMode::OpenLevel);
            }
        }
    }

    #[test]
    fn lookup_rejects_positions_outside_the_roster() {
        for mode in [PyramidLevelMode::Level50, PyramidLevelMode::OpenLevel] {
            for (round, slot, error) in [
                (20, 0, PyramidWildMonError::InvalidRound(20)),
                (0, 8, PyramidWildMonError::InvalidSlot(8)),
                (usize::MAX, 0, PyramidWildMonError::InvalidRound(usize::MAX)),
                (0, usize::MAX, PyramidWildMonError::InvalidSlot(usize::MAX)),
                // An invalid round is reported before an invalid slot.
                (20, 8, PyramidWildMonError::InvalidRound(20)),
            ] {
                assert_eq!(
                    pyramid_wild_mon(mode, round, slot),
                    Err(error),
                    "{mode:?} round {round} slot {slot}"
                );
                assert_eq!(
                    PyramidRosterPosition::new(round, slot),
                    Err(error),
                    "round {round} slot {slot}"
                );
            }
            assert_eq!(
                pyramid_round(mode, 20),
                Err(PyramidWildMonError::InvalidRound(20))
            );
            // The axes are not interchangeable: round 8 exists, slot 8 does not.
            assert!(pyramid_wild_mon(mode, 8, 0).is_ok());
            assert!(pyramid_wild_mon(mode, 0, 8).is_err());
            assert!(pyramid_wild_mon(mode, 0, 0).is_ok());
            assert!(pyramid_wild_mon(mode, 19, 7).is_ok());
        }
        let position = PyramidRosterPosition::new(19, 7).unwrap();
        assert_eq!((position.round(), position.slot()), (19, 7));
    }

    #[test]
    fn selector_level_and_error_constructors_reject_bad_values() {
        for (value, selector) in [
            (0, PyramidAbilitySelector::First),
            (1, PyramidAbilitySelector::Second),
            (2, PyramidAbilitySelector::Random),
        ] {
            assert_eq!(PyramidAbilitySelector::new(value), Ok(selector));
            assert_eq!(selector.raw(), value);
        }
        for value in [3, 4, 255] {
            assert_eq!(
                PyramidAbilitySelector::new(value),
                Err(PyramidWildMonError::InvalidAbilitySelector(value))
            );
        }
        assert_eq!(PyramidMonLevel::new(1).map(PyramidMonLevel::level), Ok(1));
        assert_eq!(
            PyramidMonLevel::new(100).map(PyramidMonLevel::level),
            Ok(100)
        );
        for value in [0, 101, 255] {
            assert_eq!(
                PyramidMonLevel::new(value),
                Err(PyramidWildMonError::InvalidLevel(value))
            );
        }
        assert_eq!(PyramidLevelOffset::new(15).offset(), 15);
    }

    #[test]
    fn errors_name_the_rejected_value() {
        assert_eq!(
            PyramidWildMonError::InvalidRound(20).to_string(),
            "Battle Pyramid round `20` is out of range (there are 20)"
        );
        assert_eq!(
            PyramidWildMonError::InvalidSlot(8).to_string(),
            "Battle Pyramid slot `8` is out of range (there are 8)"
        );
        assert_eq!(
            PyramidWildMonError::InvalidAbilitySelector(3).to_string(),
            "Battle Pyramid ability selector `3` is not 0, 1, or 2"
        );
        assert_eq!(
            PyramidWildMonError::InvalidLevel(0).to_string(),
            "Battle Pyramid level `0` is not in 1..=100"
        );
    }

    /// Resolves the checked-out upstream tree, failing loudly when it is
    /// missing (`./init.sh` fetches it).
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

    /// Strictly parses one upstream roster header: the rounds by name, in
    /// file order, and the names the pointer table lists, in order.
    fn parse_header(
        text: &str,
        pointer_table: &str,
        species: &std::collections::HashMap<String, u32>,
        moves: &std::collections::HashMap<String, u32>,
        random: u32,
    ) -> (Vec<(String, Vec<Row>)>, Vec<String>) {
        let value = |line: &str, field: &str| -> String {
            line.trim()
                .strip_prefix(&format!(".{field} = "))
                .unwrap_or_else(|| panic!("expected `.{field} = ` in `{line}`"))
                .trim_end_matches(',')
                .to_owned()
        };
        let mut rounds: Vec<(String, Vec<Row>)> = Vec::new();
        let mut pointers = Vec::new();
        let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
        while let Some(line) = lines.next() {
            if let Some(rest) = line.strip_prefix("static const struct PyramidWildMon ") {
                if rest.starts_with(&format!("*const {pointer_table}[")) {
                    assert_eq!(lines.next(), Some("{"));
                    for entry in lines.by_ref() {
                        if entry == "};" {
                            break;
                        }
                        pointers.push(entry.trim_end_matches(',').to_owned());
                    }
                    continue;
                }
                let name = rest.strip_suffix("[] =").expect("round header").to_owned();
                assert_eq!(lines.next(), Some("{"));
                let mut rows: Vec<Row> = Vec::new();
                loop {
                    match lines.next().expect("round ends with `};`") {
                        "};" => break,
                        "{" => {
                            let species_line = lines.next().unwrap();
                            let species_name = value(species_line, "species");
                            let level = value(lines.next().unwrap(), "lvl");
                            let ability = value(lines.next().unwrap(), "abilityNum");
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
                            let ability = if ability == "ABILITY_RANDOM" {
                                random
                            } else {
                                ability.parse().expect("numeric abilityNum")
                            };
                            rows.push((
                                u16::try_from(species[&species_name]).unwrap(),
                                level.parse().expect("numeric lvl"),
                                u8::try_from(ability).unwrap(),
                                ids,
                            ));
                        }
                        other => panic!("unexpected line `{other}`"),
                    }
                }
                rounds.push((name, rows));
            } else {
                panic!("unexpected line `{line}`");
            }
        }
        (rounds, pointers)
    }

    #[test]
    #[ignore = "reads the pinned upstream checkout; run ./init.sh first"]
    fn upstream_headers_match_the_rosters() {
        let species = defines(&upstream_file("include/constants/species.h"));
        let moves = defines(&upstream_file("include/constants/moves.h"));
        let random = defines(&upstream_file("src/battle_pyramid.c"))["ABILITY_RANDOM"];
        assert_eq!(random, 2);
        for (mode, file, rounds_prefix, pointer_table) in [
            (
                PyramidLevelMode::Level50,
                "src/data/battle_frontier/battle_pyramid_level_50_wild_mons.h",
                "sLevel50WildMons_Round",
                "sLevel50WildMonPointers",
            ),
            (
                PyramidLevelMode::OpenLevel,
                "src/data/battle_frontier/battle_pyramid_open_level_wild_mons.h",
                "sOpenLevelWildMons_Round",
                "sOpenLevelWildMonPointers",
            ),
        ] {
            let (rounds, pointers) = parse_header(
                &upstream_file(file),
                pointer_table,
                &species,
                &moves,
                random,
            );
            let names: Vec<String> = (1..=20).map(|n| format!("{rounds_prefix}{n}")).collect();
            assert_eq!(pointers, names, "{mode:?} pointer table");
            assert_eq!(
                rounds
                    .iter()
                    .map(|(name, _)| name.clone())
                    .collect::<Vec<_>>(),
                names,
                "{mode:?} round declarations"
            );
            let mut checked = 0;
            for (round, (name, rows)) in rounds.iter().enumerate() {
                assert_eq!(rows.len(), 8, "{name}");
                assert_eq!(rows.as_slice(), expected(mode)[round].as_slice(), "{name}");
                for (slot, row) in rows.iter().enumerate() {
                    let mon = pyramid_wild_mon(mode, round, slot).unwrap();
                    assert_eq!(mon.species(), SpeciesId(row.0), "{name} slot {slot}");
                    assert_eq!(raw_level(mon), row.1, "{name} slot {slot}");
                    assert_eq!(mon.ability_selector().raw(), row.2, "{name} slot {slot}");
                    assert_eq!(mon.moves().map(MoveId::index), row.3, "{name} slot {slot}");
                    checked += 1;
                }
            }
            assert_eq!(checked, 160, "{mode:?}");
        }
    }
}
