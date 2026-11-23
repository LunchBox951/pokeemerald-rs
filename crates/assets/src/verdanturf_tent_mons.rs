//! The Verdanturf Battle Tent's 45 rental Pokémon sets.
//!
//! Source: `gVerdanturfBattleTentMons` (`src/data/battle_frontier/battle_tent.h:1807-2124`),
//! an array of `struct FacilityMon` (`include/battle_tower.h:27-34`), with the
//! designated set IDs and `NUM_VERDANTURF_TENT_MONS` of
//! `include/constants/battle_tent_mons.h:77-123`. A set is addressed by its
//! [`VerdanturfTentSetId`], the position upstream's designated initializer
//! gives it, not by species: several species have more than one set
//! ([`VerdanturfTentSetId::NUMEL_1`] and [`VerdanturfTentSetId::NUMEL_2`],
//! the two Goldeen sets, the two Aron sets), and the sets of one species
//! differ in moves, item, EV spread and nature. The `_1`/`_2` suffixes are
//! upstream's own names for those IDs.
//!
//! Each [`VerdanturfTentMon`] keeps its four moves in upstream slot order
//! (`MOVE_NONE` stays where upstream puts it), and three fields as
//! upstream's raw bytes, each with its own meaning:
//!
//! - the held item is a [`TentHeldItemIndex`], upstream's `itemTableId`: an
//!   index into the `BATTLE_FRONTIER_ITEM_*` table
//!   (`include/constants/battle_frontier.h:126-188`), not an item ID. Looking
//!   the index up in `gBattleFrontierHeldItems` is not part of this module.
//! - the EV spread is a [`TentEvSpread`], the `F_EV_SPREAD_*` selection bits
//!   (`include/constants/battle_frontier.h:119-124`). They choose which
//!   stats receive EVs; they are neither EV amounts nor stat yields.
//!   `CreateMonWithEVSpread` (`src/pokemon.c:2391-2409`) splits
//!   `MAX_TOTAL_EVS` evenly across the selected stats. An empty spread
//!   (Pelipper's) is stored as upstream gives it.
//! - the nature is upstream's `NATURE_*` number `0..=24`
//!   (`include/constants/pokemon.h:47-72`), read with
//!   [`VerdanturfTentMon::nature_id`]. As with the Pokéblock table, this
//!   crate stays independent of the `battle` crate, so a caller maps the
//!   number onto `battle::Nature`.
//!
//! Data only: the Tent's trainer pools, the other Tents' rosters, facility
//! rules, the held-item lookup, and party construction (level, IVs, EV
//! allocation, friendship) are not ported here.

use std::error::Error;
use std::fmt;

use crate::battle_moves::MoveId;
use crate::pokeblock_flavors::NATURE_COUNT;
use crate::species::SpeciesId;

/// The number of sets (`NUM_VERDANTURF_TENT_MONS`).
pub const VERDANTURF_TENT_MON_COUNT: usize = 45;

/// The number of moves in a set (`MAX_MON_MOVES`).
pub const VERDANTURF_TENT_MON_MOVES: usize = 4;

/// A set's upstream ID (`VERDANTURF_TENT_MON_*`), its position in
/// `gVerdanturfBattleTentMons`.
///
/// Wrapping a raw number does not check it: [`verdanturf_tent_mon`] rejects
/// one that is [`VERDANTURF_TENT_MON_COUNT`] or more. The named constants
/// are the sets upstream names, with only the `VERDANTURF_TENT_MON_` prefix
/// removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VerdanturfTentSetId(pub u16);

impl VerdanturfTentSetId {
    /// Returns the raw upstream ID.
    #[must_use]
    pub const fn index(self) -> u16 {
        self.0
    }
}

/// A failed lookup in the Verdanturf Battle Tent roster.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerdanturfTentMonError {
    /// The set ID is not below [`VERDANTURF_TENT_MON_COUNT`].
    InvalidSetId(u16),
}

impl fmt::Display for VerdanturfTentMonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSetId(id) => write!(
                f,
                "Verdanturf Battle Tent set id `{id}` is out of range (there are {VERDANTURF_TENT_MON_COUNT})"
            ),
        }
    }
}

impl Error for VerdanturfTentMonError {}

/// A set's held item as upstream stores it (`itemTableId`): an index into
/// the `BATTLE_FRONTIER_ITEM_*` table, where `0` is
/// `BATTLE_FRONTIER_ITEM_NONE`.
///
/// This is not an `ItemId`; this module does not resolve it to one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TentHeldItemIndex(u8);

impl TentHeldItemIndex {
    /// Wraps a raw `BATTLE_FRONTIER_ITEM_*` index.
    #[must_use]
    pub const fn new(index: u8) -> Self {
        Self(index)
    }

    /// Returns the raw `BATTLE_FRONTIER_ITEM_*` index.
    #[must_use]
    pub const fn index(self) -> u8 {
        self.0
    }
}

/// The stats a set trains (`evSpread`): upstream's `F_EV_SPREAD_*` bits.
///
/// These select stats; they are not EV amounts. See the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TentEvSpread(u8);

impl TentEvSpread {
    /// No stat selected (`0`).
    pub const NONE: Self = Self(0);
    /// `F_EV_SPREAD_HP` (`1 << 0`).
    pub const HP: Self = Self(1 << 0);
    /// `F_EV_SPREAD_ATTACK` (`1 << 1`).
    pub const ATTACK: Self = Self(1 << 1);
    /// `F_EV_SPREAD_DEFENSE` (`1 << 2`).
    pub const DEFENSE: Self = Self(1 << 2);
    /// `F_EV_SPREAD_SPEED` (`1 << 3`).
    pub const SPEED: Self = Self(1 << 3);
    /// `F_EV_SPREAD_SP_ATTACK` (`1 << 4`).
    pub const SP_ATTACK: Self = Self(1 << 4);
    /// `F_EV_SPREAD_SP_DEFENSE` (`1 << 5`).
    pub const SP_DEFENSE: Self = Self(1 << 5);

    /// Every defined bit.
    const ALL_BITS: u8 = 0x3f;

    /// Returns this spread with `stat`'s bits added.
    const fn with(self, stat: Self) -> Self {
        let bits = self.0 | stat.0;
        assert!(bits & !Self::ALL_BITS == 0, "undefined EV spread bit");
        Self(bits)
    }

    /// Returns the raw `F_EV_SPREAD_*` bits.
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// Returns whether every stat selected in `stat` is selected here.
    #[must_use]
    pub const fn contains(self, stat: Self) -> bool {
        self.0 & stat.0 == stat.0
    }

    /// Returns how many stats are selected (`0..=6`).
    #[must_use]
    pub const fn selected_count(self) -> u32 {
        self.0.count_ones()
    }
}

/// Upstream's `NATURE_*` numbers (`include/constants/pokemon.h:47-71`). The
/// whole table is listed so a number reads as its name; the sets use some.
#[expect(
    dead_code,
    reason = "the table lists every nature; the sets use only some"
)]
mod nature {
    pub(super) const HARDY: u8 = 0;
    pub(super) const LONELY: u8 = 1;
    pub(super) const BRAVE: u8 = 2;
    pub(super) const ADAMANT: u8 = 3;
    pub(super) const NAUGHTY: u8 = 4;
    pub(super) const BOLD: u8 = 5;
    pub(super) const DOCILE: u8 = 6;
    pub(super) const RELAXED: u8 = 7;
    pub(super) const IMPISH: u8 = 8;
    pub(super) const LAX: u8 = 9;
    pub(super) const TIMID: u8 = 10;
    pub(super) const HASTY: u8 = 11;
    pub(super) const SERIOUS: u8 = 12;
    pub(super) const JOLLY: u8 = 13;
    pub(super) const NAIVE: u8 = 14;
    pub(super) const MODEST: u8 = 15;
    pub(super) const MILD: u8 = 16;
    pub(super) const QUIET: u8 = 17;
    pub(super) const BASHFUL: u8 = 18;
    pub(super) const RASH: u8 = 19;
    pub(super) const CALM: u8 = 20;
    pub(super) const GENTLE: u8 = 21;
    pub(super) const SASSY: u8 = 22;
    pub(super) const CAREFUL: u8 = 23;
    pub(super) const QUIRKY: u8 = 24;
}

/// One roster set (`struct FacilityMon`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VerdanturfTentMon {
    species: SpeciesId,
    moves: [MoveId; VERDANTURF_TENT_MON_MOVES],
    held_item: TentHeldItemIndex,
    ev_spread: TentEvSpread,
    nature: u8,
}

impl VerdanturfTentMon {
    const fn new(
        species: SpeciesId,
        moves: [MoveId; VERDANTURF_TENT_MON_MOVES],
        held_item: u8,
        ev_spread: TentEvSpread,
        nature: u8,
    ) -> Self {
        assert!((nature as usize) < NATURE_COUNT, "undefined nature");
        Self {
            species,
            moves,
            held_item: TentHeldItemIndex::new(held_item),
            ev_spread,
            nature,
        }
    }

    /// Returns the species (`species`).
    #[must_use]
    pub const fn species(&self) -> SpeciesId {
        self.species
    }

    /// Returns the four moves in slot order (`moves`); an empty slot is
    /// `MoveId::NONE`.
    #[must_use]
    pub const fn moves(&self) -> [MoveId; VERDANTURF_TENT_MON_MOVES] {
        self.moves
    }

    /// Returns the held item's index in the `BATTLE_FRONTIER_ITEM_*` table
    /// (`itemTableId`).
    #[must_use]
    pub const fn held_item(&self) -> TentHeldItemIndex {
        self.held_item
    }

    /// Returns the stats selected for EVs (`evSpread`).
    #[must_use]
    pub const fn ev_spread(&self) -> TentEvSpread {
        self.ev_spread
    }

    /// Returns the nature's upstream `NATURE_*` number (`nature`), below
    /// [`NATURE_COUNT`].
    #[must_use]
    pub const fn nature_id(&self) -> u8 {
        self.nature
    }
}

/// Returns the set with `id`.
///
/// # Errors
///
/// Returns [`VerdanturfTentMonError::InvalidSetId`] when `id` is
/// [`VERDANTURF_TENT_MON_COUNT`] or more.
pub fn verdanturf_tent_mon(
    id: VerdanturfTentSetId,
) -> Result<&'static VerdanturfTentMon, VerdanturfTentMonError> {
    ROSTER
        .get(usize::from(id.index()))
        .ok_or(VerdanturfTentMonError::InvalidSetId(id.index()))
}

/// Returns all sets in upstream ID order.
#[must_use]
pub fn verdanturf_tent_roster() -> &'static [VerdanturfTentMon; VERDANTURF_TENT_MON_COUNT] {
    &ROSTER
}

/// Declares the roster. Each line is `NAME = id => SPECIES, [moves], item
/// index, [EV stats], NATURE;` with the part of each upstream name after its
/// `VERDANTURF_TENT_MON_`, `SPECIES_`, `MOVE_`, `F_EV_SPREAD_` and `NATURE_`
/// prefix. It also names the set IDs, and refuses an ID that is not its
/// line's position.
macro_rules! tent_roster {
    ($($name:ident = $id:literal => $species:ident, [$($mv:ident),+], $item:literal,
       [$($ev:ident),*], $nature:ident;)+) => {
        impl VerdanturfTentSetId {
            $(
                #[doc = concat!("`VERDANTURF_TENT_MON_", stringify!($name), "` (`", $id, "`).")]
                pub const $name: Self = Self($id);
            )+
        }

        const _: () = {
            let ids: [u16; VERDANTURF_TENT_MON_COUNT] = [$($id),+];
            let mut position = 0;
            while position < ids.len() {
                assert!(ids[position] as usize == position, "set ID is not its position");
                position += 1;
            }
        };

        /// `gVerdanturfBattleTentMons`, in upstream ID order.
        static ROSTER: [VerdanturfTentMon; VERDANTURF_TENT_MON_COUNT] = [
            $(
                VerdanturfTentMon::new(
                    SpeciesId::$species,
                    [$(MoveId::$mv),+],
                    $item,
                    TentEvSpread::NONE$(.with(TentEvSpread::$ev))*,
                    nature::$nature,
                ),
            )+
        ];
    };
}

// The trailing comment on each line is the `BATTLE_FRONTIER_ITEM_*` name of
// its item index.
tent_roster! {
    POOCHYENA = 0 => POOCHYENA, [ROAR, THIEF, BITE, HOWL], 13, [SP_DEFENSE, SPEED, DEFENSE, ATTACK, HP], NAUGHTY; // BLACK_GLASSES
    ZIGZAGOON = 1 => ZIGZAGOON, [COVET, GROWL, PIN_MISSILE, MUD_SPORT], 2, [ATTACK, HP], ADAMANT; // SITRUS_BERRY
    DUSTOX = 2 => DUSTOX, [CONFUSION, MOONLIGHT, PROTECT, FLASH], 11, [SP_ATTACK, SPEED, DEFENSE], MODEST; // SILVER_POWDER
    TAILLOW = 3 => TAILLOW, [AGILITY, DOUBLE_TEAM, WING_ATTACK, GROWL], 3, [SPEED, ATTACK, HP], RELAXED; // ORAN_BERRY
    SHROOMISH = 4 => SHROOMISH, [HEADBUTT, STUN_SPORE, LEECH_SEED, FLASH], 41, [SPEED, HP], RASH; // LUM_BERRY
    MACHOP = 5 => MACHOP, [LOW_KICK, LEER, FOCUS_ENERGY, ENCORE], 7, [SPEED, DEFENSE, ATTACK], LONELY; // PERSIM_BERRY
    MAGIKARP = 6 => MAGIKARP, [SPLASH, FLAIL, TACKLE, NONE], 18, [ATTACK], HARDY; // SILK_SCARF
    BARBOACH = 7 => BARBOACH, [MUD_SLAP, WATER_GUN, RAIN_DANCE, SPARK], 30, [SPEED], HASTY; // ASPEAR_BERRY
    WINGULL = 8 => WINGULL, [ATTRACT, TWISTER, WING_ATTACK, SUPERSONIC], 43, [SP_DEFENSE, DEFENSE, ATTACK], IMPISH; // DRAGON_FANG
    ARON_1 = 9 => ARON, [ENDEAVOR, METAL_CLAW, TACKLE, ATTRACT], 39, [DEFENSE, ATTACK], HARDY; // PECHA_BERRY
    NUMEL_1 = 10 => NUMEL, [EMBER, ROCK_SLIDE, ROLLOUT, GROWL], 38, [SP_ATTACK, ATTACK], SASSY; // CHARCOAL
    SWALOT = 11 => SWALOT, [YAWN, SLUDGE, ACID_ARMOR, AMNESIA], 28, [ATTACK, HP], BOLD; // POISON_BARB
    KECLEON = 12 => KECLEON, [SUBSTITUTE, FAINT_ATTACK, DOUBLE_TEAM, BIND], 54, [SP_ATTACK, SPEED], QUIET; // LAX_INCENSE
    BALTOY = 13 => BALTOY, [SANDSTORM, FLASH, SELF_DESTRUCT, HARDEN], 25, [SP_DEFENSE, DEFENSE, ATTACK], NAUGHTY; // LEFTOVERS
    NINJASK = 14 => NINJASK, [DIG, SCREECH, SWORDS_DANCE, LEECH_LIFE], 60, [SP_DEFENSE, SP_ATTACK, SPEED, DEFENSE, HP], SASSY; // PETAYA_BERRY
    MAKUHITA = 15 => MAKUHITA, [BULK_UP, ARM_THRUST, SMELLING_SALT, SAND_ATTACK], 48, [DEFENSE, ATTACK], NAUGHTY; // SALAC_BERRY
    SWABLU = 16 => SWABLU, [REFRESH, SAFEGUARD, ATTRACT, MIRROR_MOVE], 32, [SP_ATTACK, SPEED], MILD; // BRIGHT_POWDER
    GOLDEEN_1 = 17 => GOLDEEN, [WATERFALL, MUD_SPORT, SUPERSONIC, DOUBLE_TEAM], 22, [SP_ATTACK, SPEED], GENTLE; // MYSTIC_WATER
    LOTAD = 18 => LOTAD, [GROWL, WATER_GUN, MEGA_DRAIN, PROTECT], 44, [SP_DEFENSE, DEFENSE, HP], QUIRKY; // IAPAPA_BERRY
    NUMEL_2 = 19 => NUMEL, [MAGNITUDE, EMBER, GROWL, DEFENSE_CURL], 38, [SP_ATTACK, ATTACK], RELAXED; // CHARCOAL
    LINOONE = 20 => LINOONE, [HEADBUTT, TICKLE, PROTECT, REST], 24, [SP_DEFENSE, DEFENSE, ATTACK, HP], IMPISH; // QUICK_CLAW
    SEVIPER = 21 => SEVIPER, [WRAP, GLARE, POISON_FANG, SWAGGER], 30, [SP_DEFENSE, SP_ATTACK, HP], LONELY; // ASPEAR_BERRY
    SKARMORY = 22 => SKARMORY, [AIR_CUTTER, SANDSTORM, LEER, AGILITY], 10, [SP_DEFENSE, ATTACK], BASHFUL; // MACHO_BRACE
    SWELLOW = 23 => SWELLOW, [AGILITY, WING_ATTACK, DOUBLE_TEAM, SUPERSONIC], 23, [SPEED, ATTACK], NAUGHTY; // SHARP_BEAK
    GOLDEEN_2 = 24 => GOLDEEN, [PECK, SUPERSONIC, PSYBEAM, RAIN_DANCE], 1, [SP_ATTACK, SPEED, HP], GENTLE; // KINGS_ROCK
    ELECTRIKE = 25 => ELECTRIKE, [SHOCK_WAVE, TACKLE, CHARGE, THUNDER_WAVE], 6, [SP_DEFENSE, DEFENSE, HP], MILD; // FOCUS_BAND
    NUZLEAF = 26 => NUZLEAF, [SUNNY_DAY, TORMENT, FAKE_OUT, THIEF], 13, [SP_DEFENSE, SP_ATTACK, DEFENSE, HP], BOLD; // BLACK_GLASSES
    ARON_2 = 27 => ARON, [TAKE_DOWN, METAL_CLAW, IRON_DEFENSE, ATTRACT], 24, [ATTACK, HP], MODEST; // QUICK_CLAW
    MIGHTYENA = 28 => MIGHTYENA, [HOWL, TAKE_DOWN, ODOR_SLEUTH, SAND_ATTACK], 1, [DEFENSE, ATTACK, HP], QUIRKY; // KINGS_ROCK
    SOLROCK = 29 => SOLROCK, [HARDEN, FIRE_SPIN, MIMIC, PSYWAVE], 35, [SP_DEFENSE, SP_ATTACK, DEFENSE, ATTACK], ADAMANT; // TWISTED_SPOON
    GRAVELER = 30 => GRAVELER, [ROLLOUT, ROCK_SMASH, SANDSTORM, BLOCK], 6, [DEFENSE, ATTACK], IMPISH; // FOCUS_BAND
    KADABRA = 31 => KADABRA, [ENCORE, PSYBEAM, DISABLE, KINESIS], 6, [SP_ATTACK, SPEED, HP], QUIET; // FOCUS_BAND
    VOLTORB = 32 => VOLTORB, [PROTECT, SELF_DESTRUCT, SPARK, SCREECH], 34, [SPEED, ATTACK], BASHFUL; // SCOPE_LENS
    MAGNEMITE = 33 => MAGNEMITE, [METAL_SOUND, SUPERSONIC, THUNDER_WAVE, SHOCK_WAVE], 17, [SP_DEFENSE, SP_ATTACK, SPEED, DEFENSE, ATTACK, HP], SERIOUS; // MAGNET
    PELIPPER = 34 => PELIPPER, [WING_ATTACK, MIST, AGILITY, PROTECT], 56, [], BRAVE; // FIGY_BERRY
    LOUDRED = 35 => LOUDRED, [HOWL, UPROAR, REST, SCREECH], 25, [SP_DEFENSE, DEFENSE, HP], CALM; // LEFTOVERS
    MARILL = 36 => MARILL, [ROLLOUT, DEFENSE_CURL, TAIL_WHIP, BUBBLE_BEAM], 26, [SP_DEFENSE, DEFENSE, HP], JOLLY; // RAWST_BERRY
    TRAPINCH = 37 => TRAPINCH, [DIG, ROCK_TOMB, SANDSTORM, FAINT_ATTACK], 0, [SPEED, ATTACK, HP], NAUGHTY; // NONE
    TENTACOOL = 38 => TENTACOOL, [SCREECH, CONFUSE_RAY, WRAP, ATTRACT], 32, [ATTACK, HP], RELAXED; // BRIGHT_POWDER
    VIGOROTH = 39 => VIGOROTH, [ENCORE, SLACK_OFF, YAWN, SECRET_POWER], 37, [SP_DEFENSE, DEFENSE, ATTACK], QUIRKY; // MENTAL_HERB
    WAILMER = 40 => WAILMER, [WATER_PULSE, ROLLOUT, CURSE, PROTECT], 19, [HP], RELAXED; // WHITE_HERB
    ILLUMISE = 41 => ILLUMISE, [TACKLE, SWEET_SCENT, WISH, ENCORE], 32, [SP_DEFENSE, DEFENSE, HP], QUIRKY; // BRIGHT_POWDER
    MACHOKE = 42 => MACHOKE, [LEER, KARATE_CHOP, BULK_UP, REST], 4, [SPEED, ATTACK], CALM; // CHESTO_BERRY
    WHISMUR = 43 => WHISMUR, [SECRET_POWER, SUPERSONIC, STOMP, ROAR], 26, [SP_ATTACK, SPEED], QUIRKY; // RAWST_BERRY
    CACNEA = 44 => CACNEA, [INGRAIN, LEECH_SEED, NEEDLE_ARM, GROWTH], 34, [SP_DEFENSE, DEFENSE, HP], SERIOUS; // SCOPE_LENS
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One set as upstream's numeric values: `species`, the four `moves`,
    /// `itemTableId`, `evSpread` and `nature`.
    type Row = (u16, [u16; VERDANTURF_TENT_MON_MOVES], u8, u8, u8);

    /// `gVerdanturfBattleTentMons` in designated-ID order, read from
    /// `battle_tent.h:1807-2124` with the numbers of `species.h`, `moves.h`,
    /// `battle_frontier.h` (`F_EV_SPREAD_*` OR-ed) and `pokemon.h`. Generated
    /// by a script that parses the upstream file and checks every record's
    /// designator against `battle_tent_mons.h`.
    #[rustfmt::skip]
    const EXPECTED: [Row; VERDANTURF_TENT_MON_COUNT] = [
        (286, [46, 168, 44, 336], 13, 47, 4), // 0 POOCHYENA
        (288, [343, 45, 42, 300], 2, 3, 3), // 1 ZIGZAGOON
        (294, [93, 236, 182, 148], 11, 28, 15), // 2 DUSTOX
        (304, [97, 104, 17, 45], 3, 11, 7), // 3 TAILLOW
        (306, [29, 78, 73, 148], 41, 9, 19), // 4 SHROOMISH
        (66, [67, 43, 116, 227], 7, 14, 1), // 5 MACHOP
        (129, [150, 175, 33, 0], 18, 2, 0), // 6 MAGIKARP
        (323, [189, 55, 240, 209], 30, 8, 11), // 7 BARBOACH
        (309, [213, 239, 17, 48], 43, 38, 8), // 8 WINGULL
        (382, [283, 232, 33, 213], 39, 6, 0), // 9 ARON_1
        (339, [52, 157, 205, 45], 38, 18, 22), // 10 NUMEL_1
        (368, [281, 124, 151, 133], 28, 3, 5), // 11 SWALOT
        (317, [164, 185, 104, 20], 54, 24, 17), // 12 KECLEON
        (318, [201, 148, 120, 106], 25, 38, 4), // 13 BALTOY
        (302, [91, 103, 14, 141], 60, 61, 22), // 14 NINJASK
        (335, [339, 292, 265, 28], 48, 6, 4), // 15 MAKUHITA
        (358, [287, 219, 213, 119], 32, 24, 16), // 16 SWABLU
        (118, [127, 300, 48, 104], 22, 24, 21), // 17 GOLDEEN_1
        (295, [45, 55, 72, 182], 44, 37, 24), // 18 LOTAD
        (339, [222, 52, 45, 111], 38, 18, 7), // 19 NUMEL_2
        (289, [29, 321, 182, 156], 24, 39, 8), // 20 LINOONE
        (379, [35, 137, 305, 207], 30, 49, 1), // 21 SEVIPER
        (227, [314, 201, 43, 97], 10, 34, 18), // 22 SKARMORY
        (305, [97, 17, 104, 48], 23, 10, 4), // 23 SWELLOW
        (118, [64, 48, 60, 240], 1, 25, 21), // 24 GOLDEEN_2
        (337, [351, 33, 268, 86], 6, 37, 16), // 25 ELECTRIKE
        (299, [241, 259, 252, 168], 13, 53, 5), // 26 NUZLEAF
        (382, [36, 232, 334, 213], 24, 3, 15), // 27 ARON_2
        (287, [336, 36, 316, 28], 1, 7, 24), // 28 MIGHTYENA
        (349, [106, 83, 102, 149], 35, 54, 3), // 29 SOLROCK
        (75, [205, 249, 201, 335], 6, 6, 8), // 30 GRAVELER
        (64, [227, 60, 50, 134], 6, 25, 17), // 31 KADABRA
        (100, [182, 120, 209, 103], 34, 10, 18), // 32 VOLTORB
        (81, [319, 48, 86, 351], 17, 63, 12), // 33 MAGNEMITE
        (310, [17, 54, 97, 182], 56, 0, 2), // 34 PELIPPER
        (371, [336, 253, 156, 103], 25, 37, 20), // 35 LOUDRED
        (183, [205, 111, 39, 61], 26, 37, 13), // 36 MARILL
        (332, [91, 317, 201, 185], 0, 11, 4), // 37 TRAPINCH
        (72, [103, 109, 35, 213], 32, 3, 7), // 38 TENTACOOL
        (365, [227, 303, 281, 290], 37, 38, 24), // 39 VIGOROTH
        (313, [352, 205, 174, 182], 19, 1, 7), // 40 WAILMER
        (387, [33, 230, 273, 227], 32, 37, 24), // 41 ILLUMISE
        (67, [43, 2, 339, 156], 4, 10, 20), // 42 MACHOKE
        (370, [290, 48, 23, 46], 26, 24, 24), // 43 WHISMUR
        (344, [275, 73, 302, 74], 34, 37, 12), // 44 CACNEA
    ];

    fn set(id: u16) -> &'static VerdanturfTentMon {
        verdanturf_tent_mon(VerdanturfTentSetId(id)).expect("set id is in range")
    }

    fn row(mon: &VerdanturfTentMon) -> Row {
        (
            mon.species().0,
            mon.moves().map(|move_id| move_id.0),
            mon.held_item().index(),
            mon.ev_spread().bits(),
            mon.nature_id(),
        )
    }

    #[test]
    fn every_set_matches_upstream() {
        let mut checked = 0;
        for (id, expected) in EXPECTED.iter().enumerate() {
            let id = u16::try_from(id).unwrap();
            assert_eq!(row(set(id)), *expected, "set {id}");
            checked += 1;
        }
        assert_eq!(checked, VERDANTURF_TENT_MON_COUNT);
        let all = verdanturf_tent_roster();
        assert_eq!(all.map(|mon| row(&mon)), EXPECTED);
    }

    #[test]
    fn named_ids_are_their_upstream_numbers() {
        assert_eq!(VerdanturfTentSetId::POOCHYENA, VerdanturfTentSetId(0));
        assert_eq!(VerdanturfTentSetId::ARON_1, VerdanturfTentSetId(9));
        assert_eq!(VerdanturfTentSetId::NUMEL_1, VerdanturfTentSetId(10));
        assert_eq!(VerdanturfTentSetId::GOLDEEN_1, VerdanturfTentSetId(17));
        assert_eq!(VerdanturfTentSetId::NUMEL_2, VerdanturfTentSetId(19));
        assert_eq!(VerdanturfTentSetId::GOLDEEN_2, VerdanturfTentSetId(24));
        assert_eq!(VerdanturfTentSetId::ARON_2, VerdanturfTentSetId(27));
        assert_eq!(VerdanturfTentSetId::CACNEA, VerdanturfTentSetId(44));
    }

    #[test]
    fn goldeen_sets_keep_distinct_moves_items_and_spreads() {
        let first = set(17);
        let second = set(24);
        assert_eq!(
            first,
            verdanturf_tent_mon(VerdanturfTentSetId::GOLDEEN_1).unwrap()
        );
        assert_eq!(
            second,
            verdanturf_tent_mon(VerdanturfTentSetId::GOLDEEN_2).unwrap()
        );
        assert_eq!(first.species(), SpeciesId::GOLDEEN);
        assert_eq!(second.species(), SpeciesId::GOLDEEN);
        assert_eq!(
            first.moves(),
            [
                MoveId::WATERFALL,
                MoveId::MUD_SPORT,
                MoveId::SUPERSONIC,
                MoveId::DOUBLE_TEAM
            ]
        );
        assert_eq!(
            second.moves(),
            [
                MoveId::PECK,
                MoveId::SUPERSONIC,
                MoveId::PSYBEAM,
                MoveId::RAIN_DANCE
            ]
        );
        // BATTLE_FRONTIER_ITEM_MYSTIC_WATER and BATTLE_FRONTIER_ITEM_KINGS_ROCK.
        assert_eq!(first.held_item(), TentHeldItemIndex::new(22));
        assert_eq!(second.held_item(), TentHeldItemIndex::new(1));
        assert_eq!(
            first.ev_spread().bits(),
            TentEvSpread::SP_ATTACK.bits() | TentEvSpread::SPEED.bits()
        );
        assert_eq!(
            second.ev_spread().bits(),
            TentEvSpread::SP_ATTACK.bits() | TentEvSpread::SPEED.bits() | TentEvSpread::HP.bits()
        );
        // Both are NATURE_GENTLE.
        assert_eq!((first.nature_id(), second.nature_id()), (21, 21));
        assert_ne!(first, second);
    }

    #[test]
    fn aron_sets_keep_distinct_moves_items_spreads_and_natures() {
        let first = set(9);
        let second = set(27);
        assert_eq!(
            first,
            verdanturf_tent_mon(VerdanturfTentSetId::ARON_1).unwrap()
        );
        assert_eq!(
            second,
            verdanturf_tent_mon(VerdanturfTentSetId::ARON_2).unwrap()
        );
        assert_eq!(first.species(), SpeciesId::ARON);
        assert_eq!(second.species(), SpeciesId::ARON);
        assert_eq!(
            first.moves(),
            [
                MoveId::ENDEAVOR,
                MoveId::METAL_CLAW,
                MoveId::TACKLE,
                MoveId::ATTRACT
            ]
        );
        assert_eq!(
            second.moves(),
            [
                MoveId::TAKE_DOWN,
                MoveId::METAL_CLAW,
                MoveId::IRON_DEFENSE,
                MoveId::ATTRACT
            ]
        );
        // BATTLE_FRONTIER_ITEM_PECHA_BERRY and BATTLE_FRONTIER_ITEM_QUICK_CLAW.
        assert_eq!(first.held_item(), TentHeldItemIndex::new(39));
        assert_eq!(second.held_item(), TentHeldItemIndex::new(24));
        assert_eq!(
            first.ev_spread().bits(),
            TentEvSpread::DEFENSE.bits() | TentEvSpread::ATTACK.bits()
        );
        assert_eq!(
            second.ev_spread().bits(),
            TentEvSpread::ATTACK.bits() | TentEvSpread::HP.bits()
        );
        // NATURE_HARDY and NATURE_MODEST.
        assert_eq!((first.nature_id(), second.nature_id()), (0, 15));
        assert_ne!(first, second);
    }

    #[test]
    fn numel_sets_are_told_apart_too() {
        let first = set(10);
        let second = set(19);
        assert_eq!(first.species(), SpeciesId::NUMEL);
        assert_eq!(second.species(), SpeciesId::NUMEL);
        // NATURE_SASSY and NATURE_RELAXED.
        assert_eq!((first.nature_id(), second.nature_id()), (22, 7));
        assert_ne!(first.moves(), second.moves());
    }

    #[test]
    fn unusual_fields_are_kept_as_upstream_has_them() {
        let magikarp = set(6);
        assert_eq!(
            magikarp.moves(),
            [MoveId::SPLASH, MoveId::FLAIL, MoveId::TACKLE, MoveId::NONE]
        );
        assert_eq!(magikarp.ev_spread(), TentEvSpread::ATTACK);
        let magnemite = set(33);
        assert_eq!(magnemite.ev_spread().bits(), 0x3f);
        assert_eq!(magnemite.ev_spread().selected_count(), 6);
        let pelipper = set(34);
        assert_eq!(pelipper.ev_spread(), TentEvSpread::NONE);
        assert_eq!(pelipper.ev_spread().selected_count(), 0);
        // Trapinch holds BATTLE_FRONTIER_ITEM_NONE.
        assert_eq!(set(37).held_item(), TentHeldItemIndex::new(0));
    }

    #[test]
    fn ev_spread_bits_have_their_upstream_meanings() {
        assert_eq!(TentEvSpread::HP.bits(), 1);
        assert_eq!(TentEvSpread::ATTACK.bits(), 2);
        assert_eq!(TentEvSpread::DEFENSE.bits(), 4);
        assert_eq!(TentEvSpread::SPEED.bits(), 8);
        assert_eq!(TentEvSpread::SP_ATTACK.bits(), 16);
        assert_eq!(TentEvSpread::SP_DEFENSE.bits(), 32);
        let zigzagoon = set(1).ev_spread();
        assert!(zigzagoon.contains(TentEvSpread::ATTACK));
        assert!(zigzagoon.contains(TentEvSpread::HP));
        assert!(!zigzagoon.contains(TentEvSpread::SPEED));
        assert_eq!(zigzagoon.selected_count(), 2);
    }

    #[test]
    fn every_nature_is_a_defined_nature() {
        for id in 0..VERDANTURF_TENT_MON_COUNT {
            let mon = verdanturf_tent_roster()[id];
            assert!(usize::from(mon.nature_id()) < NATURE_COUNT, "set {id}");
        }
    }

    #[test]
    fn lookup_covers_the_first_and_last_sets_and_rejects_the_rest() {
        assert_eq!(set(0).species(), SpeciesId::POOCHYENA);
        assert_eq!(set(44).species(), SpeciesId::CACNEA);
        for id in [45, 46, 1000, u16::MAX] {
            assert_eq!(
                verdanturf_tent_mon(VerdanturfTentSetId(id)),
                Err(VerdanturfTentMonError::InvalidSetId(id))
            );
        }
    }

    #[test]
    fn error_names_the_rejected_value() {
        assert_eq!(
            VerdanturfTentMonError::InvalidSetId(45).to_string(),
            "Verdanturf Battle Tent set id `45` is out of range (there are 45)"
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

    /// Reads `#define NAME <integer>` and `#define NAME (1 << <n>)` lines.
    fn defines(text: &str) -> std::collections::HashMap<String, u32> {
        text.lines()
            .filter_map(|line| {
                let mut words = line.split_whitespace();
                (words.next()? == "#define").then_some(())?;
                let name = words.next()?.to_owned();
                let rest: Vec<&str> = words.collect();
                let value = match rest.as_slice() {
                    [number] => number.parse().ok()?,
                    ["(1", "<<", shift] => {
                        1_u32 << shift.trim_end_matches(')').parse::<u32>().ok()?
                    }
                    _ => return None,
                };
                Some((name, value))
            })
            .collect()
    }

    /// Strictly parses `gVerdanturfBattleTentMons` into each record's
    /// designated ID and numeric row, in file order.
    fn parse_source(source: &str) -> Vec<(u32, Row)> {
        let species = defines(&upstream_file("include/constants/species.h"));
        let moves = defines(&upstream_file("include/constants/moves.h"));
        let natures = defines(&upstream_file("include/constants/pokemon.h"));
        let frontier = defines(&upstream_file("include/constants/battle_frontier.h"));
        let tent = defines(&upstream_file("include/constants/battle_tent_mons.h"));

        let start = source
            .find("gVerdanturfBattleTentMons[NUM_VERDANTURF_TENT_MONS] =")
            .expect("declaration found");
        let body = &source[start..];
        let open = body.find("=\n{").expect("initializer opens") + 3;
        let close = body.find("\n};").expect("initializer closes");

        let field = |line: &str, name: &str| -> String {
            line.strip_prefix(&format!(".{name} = "))
                .unwrap_or_else(|| panic!("expected `.{name} = ` in `{line}`"))
                .trim_end_matches(',')
                .to_owned()
        };
        let mut records = Vec::new();
        let mut lines = body[open..close]
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty());
        while let Some(line) = lines.next() {
            let designator = line
                .strip_prefix('[')
                .and_then(|rest| rest.strip_suffix("] = {"))
                .unwrap_or_else(|| panic!("expected a designated record, got `{line}`"));
            let id = tent[designator];
            let species_line = field(lines.next().unwrap(), "species");
            let moves_line = field(lines.next().unwrap(), "moves");
            let item_line = field(lines.next().unwrap(), "itemTableId");
            let ev_line = field(lines.next().unwrap(), "evSpread");
            let nature_line = lines
                .next()
                .unwrap()
                .strip_prefix(".nature = ")
                .unwrap()
                .to_owned();
            let close = lines.next().expect("record closes");
            assert!(matches!(close, "}" | "},"), "`{close}`");

            let list = moves_line
                .strip_prefix('{')
                .and_then(|list| list.strip_suffix('}'))
                .expect("moves are braced");
            let mut move_ids = [0_u16; VERDANTURF_TENT_MON_MOVES];
            let names: Vec<&str> = list.split(", ").collect();
            assert_eq!(names.len(), VERDANTURF_TENT_MON_MOVES, "{moves_line}");
            for (slot, name) in move_ids.iter_mut().zip(names) {
                *slot = u16::try_from(moves[name]).unwrap();
            }
            let ev_bits = if ev_line == "0" {
                0
            } else {
                ev_line
                    .split(" | ")
                    .map(|name| frontier[name])
                    .fold(0, |all, bit| all | bit)
            };
            records.push((
                id,
                (
                    u16::try_from(species[&species_line]).unwrap(),
                    move_ids,
                    u8::try_from(frontier[&item_line]).unwrap(),
                    u8::try_from(ev_bits).unwrap(),
                    u8::try_from(natures[&nature_line]).unwrap(),
                ),
            ));
        }
        records
    }

    #[test]
    #[ignore = "reads the pinned upstream checkout; run ./init.sh first"]
    fn upstream_source_matches_the_roster() {
        let records = parse_source(&upstream_file("src/data/battle_frontier/battle_tent.h"));
        let tent = defines(&upstream_file("include/constants/battle_tent_mons.h"));
        assert_eq!(records.len(), VERDANTURF_TENT_MON_COUNT);
        assert_eq!(
            tent["NUM_VERDANTURF_TENT_MONS"],
            u32::try_from(VERDANTURF_TENT_MON_COUNT).unwrap()
        );
        for (position, (id, parsed)) in records.iter().enumerate() {
            assert_eq!(*id, u32::try_from(position).unwrap(), "record order");
            assert_eq!(parsed, &EXPECTED[position], "fixture, set {position}");
            assert_eq!(
                &row(set(u16::try_from(position).unwrap())),
                parsed,
                "roster, set {position}"
            );
        }
    }
}
