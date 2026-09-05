//! Species evolution rules.

use crate::error::AssetError;
use crate::species::{ItemId, SpeciesId};

impl ItemId {
    const FIRE_STONE: Self = Self(95);
    const THUNDER_STONE: Self = Self(96);
    const WATER_STONE: Self = Self(97);
    const LEAF_STONE: Self = Self(98);
    const DEEP_SEA_TOOTH: Self = Self(192);
    const DEEP_SEA_SCALE: Self = Self(193);
    const UP_GRADE: Self = Self(218);
}

/// A species evolution trigger and its method-specific parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EvoMethod {
    /// Levels up with sufficient friendship.
    Friendship,
    /// Levels up with sufficient friendship during the day.
    FriendshipDay,
    /// Levels up with sufficient friendship at night.
    FriendshipNight,
    /// Reaches the given level.
    Level(u8),
    /// Is traded.
    Trade,
    /// Is traded while holding the given item.
    TradeItem(ItemId),
    /// Has the given item used on it.
    Item(ItemId),
    /// Reaches the given level with Attack greater than Defence.
    LevelAtkGtDef(u8),
    /// Reaches the given level with equal Attack and Defence.
    LevelAtkEqDef(u8),
    /// Reaches the given level with Attack less than Defence.
    LevelAtkLtDef(u8),
    /// Reaches the given level with a Silcoon personality value.
    LevelSilcoon(u8),
    /// Reaches the given level with a Cascoon personality value.
    LevelCascoon(u8),
    /// Reaches the given level for Nincada's Ninjask branch.
    LevelNinjask(u8),
    /// Reaches the given level for Nincada's Shedinja branch.
    LevelShedinja(u8),
    /// Levels up with at least the given beauty value.
    Beauty(u16),
}

impl EvoMethod {
    /// Returns the stored method identifier.
    #[must_use]
    pub const fn method_id(self) -> u16 {
        match self {
            Self::Friendship => 1,
            Self::FriendshipDay => 2,
            Self::FriendshipNight => 3,
            Self::Level(_) => 4,
            Self::Trade => 5,
            Self::TradeItem(_) => 6,
            Self::Item(_) => 7,
            Self::LevelAtkGtDef(_) => 8,
            Self::LevelAtkEqDef(_) => 9,
            Self::LevelAtkLtDef(_) => 10,
            Self::LevelSilcoon(_) => 11,
            Self::LevelCascoon(_) => 12,
            Self::LevelNinjask(_) => 13,
            Self::LevelShedinja(_) => 14,
            Self::Beauty(_) => 15,
        }
    }

    /// Returns the stored level, item identifier, or beauty threshold.
    ///
    /// Methods without a parameter return zero.
    #[must_use]
    pub const fn param(self) -> u16 {
        match self {
            Self::Friendship | Self::FriendshipDay | Self::FriendshipNight | Self::Trade => 0,
            Self::Level(lvl)
            | Self::LevelAtkGtDef(lvl)
            | Self::LevelAtkEqDef(lvl)
            | Self::LevelAtkLtDef(lvl)
            | Self::LevelSilcoon(lvl)
            | Self::LevelCascoon(lvl)
            | Self::LevelNinjask(lvl)
            | Self::LevelShedinja(lvl) => lvl as u16,
            Self::TradeItem(item) | Self::Item(item) => item.0,
            Self::Beauty(beauty) => beauty,
        }
    }
}

/// One evolution rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Evolution {
    /// The trigger and its parameter.
    pub method: EvoMethod,
    /// The resulting species.
    pub target: SpeciesId,
}

const fn evolution(method: EvoMethod, target: SpeciesId) -> Evolution {
    Evolution { method, target }
}

#[rustfmt::skip]
const ENTRIES: &[(SpeciesId, &[Evolution])] = &[
    (SpeciesId::BULBASAUR, &[evolution(EvoMethod::Level(16), SpeciesId::IVYSAUR)]),
    (SpeciesId::IVYSAUR, &[evolution(EvoMethod::Level(32), SpeciesId::VENUSAUR)]),
    (SpeciesId::CHARMANDER, &[evolution(EvoMethod::Level(16), SpeciesId::CHARMELEON)]),
    (SpeciesId::CHARMELEON, &[evolution(EvoMethod::Level(36), SpeciesId::CHARIZARD)]),
    (SpeciesId::SQUIRTLE, &[evolution(EvoMethod::Level(16), SpeciesId::WARTORTLE)]),
    (SpeciesId::WARTORTLE, &[evolution(EvoMethod::Level(36), SpeciesId::BLASTOISE)]),
    (SpeciesId::CATERPIE, &[evolution(EvoMethod::Level(7), SpeciesId::METAPOD)]),
    (SpeciesId::METAPOD, &[evolution(EvoMethod::Level(10), SpeciesId::BUTTERFREE)]),
    (SpeciesId::WEEDLE, &[evolution(EvoMethod::Level(7), SpeciesId::KAKUNA)]),
    (SpeciesId::KAKUNA, &[evolution(EvoMethod::Level(10), SpeciesId::BEEDRILL)]),
    (SpeciesId::PIDGEY, &[evolution(EvoMethod::Level(18), SpeciesId::PIDGEOTTO)]),
    (SpeciesId::PIDGEOTTO, &[evolution(EvoMethod::Level(36), SpeciesId::PIDGEOT)]),
    (SpeciesId::RATTATA, &[evolution(EvoMethod::Level(20), SpeciesId::RATICATE)]),
    (SpeciesId::SPEAROW, &[evolution(EvoMethod::Level(20), SpeciesId::FEAROW)]),
    (SpeciesId::EKANS, &[evolution(EvoMethod::Level(22), SpeciesId::ARBOK)]),
    (
        SpeciesId::PIKACHU,
        &[evolution(EvoMethod::Item(ItemId::THUNDER_STONE), SpeciesId::RAICHU)],
    ),
    (SpeciesId::SANDSHREW, &[evolution(EvoMethod::Level(22), SpeciesId::SANDSLASH)]),
    (SpeciesId::NIDORAN_F, &[evolution(EvoMethod::Level(16), SpeciesId::NIDORINA)]),
    (
        SpeciesId::NIDORINA,
        &[evolution(EvoMethod::Item(ItemId::MOON_STONE), SpeciesId::NIDOQUEEN)],
    ),
    (SpeciesId::NIDORAN_M, &[evolution(EvoMethod::Level(16), SpeciesId::NIDORINO)]),
    (
        SpeciesId::NIDORINO,
        &[evolution(EvoMethod::Item(ItemId::MOON_STONE), SpeciesId::NIDOKING)],
    ),
    (
        SpeciesId::CLEFAIRY,
        &[evolution(EvoMethod::Item(ItemId::MOON_STONE), SpeciesId::CLEFABLE)],
    ),
    (
        SpeciesId::VULPIX,
        &[evolution(EvoMethod::Item(ItemId::FIRE_STONE), SpeciesId::NINETALES)],
    ),
    (
        SpeciesId::JIGGLYPUFF,
        &[evolution(EvoMethod::Item(ItemId::MOON_STONE), SpeciesId::WIGGLYTUFF)],
    ),
    (SpeciesId::ZUBAT, &[evolution(EvoMethod::Level(22), SpeciesId::GOLBAT)]),
    (SpeciesId::GOLBAT, &[evolution(EvoMethod::Friendship, SpeciesId::CROBAT)]),
    (SpeciesId::ODDISH, &[evolution(EvoMethod::Level(21), SpeciesId::GLOOM)]),
    (
        SpeciesId::GLOOM,
        &[
            evolution(EvoMethod::Item(ItemId::LEAF_STONE), SpeciesId::VILEPLUME),
            evolution(EvoMethod::Item(ItemId::SUN_STONE), SpeciesId::BELLOSSOM),
        ],
    ),
    (SpeciesId::PARAS, &[evolution(EvoMethod::Level(24), SpeciesId::PARASECT)]),
    (SpeciesId::VENONAT, &[evolution(EvoMethod::Level(31), SpeciesId::VENOMOTH)]),
    (SpeciesId::DIGLETT, &[evolution(EvoMethod::Level(26), SpeciesId::DUGTRIO)]),
    (SpeciesId::MEOWTH, &[evolution(EvoMethod::Level(28), SpeciesId::PERSIAN)]),
    (SpeciesId::PSYDUCK, &[evolution(EvoMethod::Level(33), SpeciesId::GOLDUCK)]),
    (SpeciesId::MANKEY, &[evolution(EvoMethod::Level(28), SpeciesId::PRIMEAPE)]),
    (
        SpeciesId::GROWLITHE,
        &[evolution(EvoMethod::Item(ItemId::FIRE_STONE), SpeciesId::ARCANINE)],
    ),
    (SpeciesId::POLIWAG, &[evolution(EvoMethod::Level(25), SpeciesId::POLIWHIRL)]),
    (
        SpeciesId::POLIWHIRL,
        &[
            evolution(EvoMethod::Item(ItemId::WATER_STONE), SpeciesId::POLIWRATH),
            evolution(EvoMethod::TradeItem(ItemId::KINGS_ROCK), SpeciesId::POLITOED),
        ],
    ),
    (SpeciesId::ABRA, &[evolution(EvoMethod::Level(16), SpeciesId::KADABRA)]),
    (SpeciesId::KADABRA, &[evolution(EvoMethod::Trade, SpeciesId::ALAKAZAM)]),
    (SpeciesId::MACHOP, &[evolution(EvoMethod::Level(28), SpeciesId::MACHOKE)]),
    (SpeciesId::MACHOKE, &[evolution(EvoMethod::Trade, SpeciesId::MACHAMP)]),
    (SpeciesId::BELLSPROUT, &[evolution(EvoMethod::Level(21), SpeciesId::WEEPINBELL)]),
    (
        SpeciesId::WEEPINBELL,
        &[evolution(EvoMethod::Item(ItemId::LEAF_STONE), SpeciesId::VICTREEBEL)],
    ),
    (SpeciesId::TENTACOOL, &[evolution(EvoMethod::Level(30), SpeciesId::TENTACRUEL)]),
    (SpeciesId::GEODUDE, &[evolution(EvoMethod::Level(25), SpeciesId::GRAVELER)]),
    (SpeciesId::GRAVELER, &[evolution(EvoMethod::Trade, SpeciesId::GOLEM)]),
    (SpeciesId::PONYTA, &[evolution(EvoMethod::Level(40), SpeciesId::RAPIDASH)]),
    (
        SpeciesId::SLOWPOKE,
        &[
            evolution(EvoMethod::Level(37), SpeciesId::SLOWBRO),
            evolution(EvoMethod::TradeItem(ItemId::KINGS_ROCK), SpeciesId::SLOWKING),
        ],
    ),
    (SpeciesId::MAGNEMITE, &[evolution(EvoMethod::Level(30), SpeciesId::MAGNETON)]),
    (SpeciesId::DODUO, &[evolution(EvoMethod::Level(31), SpeciesId::DODRIO)]),
    (SpeciesId::SEEL, &[evolution(EvoMethod::Level(34), SpeciesId::DEWGONG)]),
    (SpeciesId::GRIMER, &[evolution(EvoMethod::Level(38), SpeciesId::MUK)]),
    (
        SpeciesId::SHELLDER,
        &[evolution(EvoMethod::Item(ItemId::WATER_STONE), SpeciesId::CLOYSTER)],
    ),
    (SpeciesId::GASTLY, &[evolution(EvoMethod::Level(25), SpeciesId::HAUNTER)]),
    (SpeciesId::HAUNTER, &[evolution(EvoMethod::Trade, SpeciesId::GENGAR)]),
    (
        SpeciesId::ONIX,
        &[evolution(EvoMethod::TradeItem(ItemId::METAL_COAT), SpeciesId::STEELIX)],
    ),
    (SpeciesId::DROWZEE, &[evolution(EvoMethod::Level(26), SpeciesId::HYPNO)]),
    (SpeciesId::KRABBY, &[evolution(EvoMethod::Level(28), SpeciesId::KINGLER)]),
    (SpeciesId::VOLTORB, &[evolution(EvoMethod::Level(30), SpeciesId::ELECTRODE)]),
    (
        SpeciesId::EXEGGCUTE,
        &[evolution(EvoMethod::Item(ItemId::LEAF_STONE), SpeciesId::EXEGGUTOR)],
    ),
    (SpeciesId::CUBONE, &[evolution(EvoMethod::Level(28), SpeciesId::MAROWAK)]),
    (SpeciesId::KOFFING, &[evolution(EvoMethod::Level(35), SpeciesId::WEEZING)]),
    (SpeciesId::RHYHORN, &[evolution(EvoMethod::Level(42), SpeciesId::RHYDON)]),
    (SpeciesId::CHANSEY, &[evolution(EvoMethod::Friendship, SpeciesId::BLISSEY)]),
    (SpeciesId::HORSEA, &[evolution(EvoMethod::Level(32), SpeciesId::SEADRA)]),
    (
        SpeciesId::SEADRA,
        &[evolution(EvoMethod::TradeItem(ItemId::DRAGON_SCALE), SpeciesId::KINGDRA)],
    ),
    (SpeciesId::GOLDEEN, &[evolution(EvoMethod::Level(33), SpeciesId::SEAKING)]),
    (
        SpeciesId::STARYU,
        &[evolution(EvoMethod::Item(ItemId::WATER_STONE), SpeciesId::STARMIE)],
    ),
    (
        SpeciesId::SCYTHER,
        &[evolution(EvoMethod::TradeItem(ItemId::METAL_COAT), SpeciesId::SCIZOR)],
    ),
    (SpeciesId::MAGIKARP, &[evolution(EvoMethod::Level(20), SpeciesId::GYARADOS)]),
    (
        SpeciesId::EEVEE,
        &[
            evolution(EvoMethod::Item(ItemId::THUNDER_STONE), SpeciesId::JOLTEON),
            evolution(EvoMethod::Item(ItemId::WATER_STONE), SpeciesId::VAPOREON),
            evolution(EvoMethod::Item(ItemId::FIRE_STONE), SpeciesId::FLAREON),
            evolution(EvoMethod::FriendshipDay, SpeciesId::ESPEON),
            evolution(EvoMethod::FriendshipNight, SpeciesId::UMBREON),
        ],
    ),
    (
        SpeciesId::PORYGON,
        &[evolution(EvoMethod::TradeItem(ItemId::UP_GRADE), SpeciesId::PORYGON2)],
    ),
    (SpeciesId::OMANYTE, &[evolution(EvoMethod::Level(40), SpeciesId::OMASTAR)]),
    (SpeciesId::KABUTO, &[evolution(EvoMethod::Level(40), SpeciesId::KABUTOPS)]),
    (SpeciesId::DRATINI, &[evolution(EvoMethod::Level(30), SpeciesId::DRAGONAIR)]),
    (SpeciesId::DRAGONAIR, &[evolution(EvoMethod::Level(55), SpeciesId::DRAGONITE)]),
    (SpeciesId::CHIKORITA, &[evolution(EvoMethod::Level(16), SpeciesId::BAYLEEF)]),
    (SpeciesId::BAYLEEF, &[evolution(EvoMethod::Level(32), SpeciesId::MEGANIUM)]),
    (SpeciesId::CYNDAQUIL, &[evolution(EvoMethod::Level(14), SpeciesId::QUILAVA)]),
    (SpeciesId::QUILAVA, &[evolution(EvoMethod::Level(36), SpeciesId::TYPHLOSION)]),
    (SpeciesId::TOTODILE, &[evolution(EvoMethod::Level(18), SpeciesId::CROCONAW)]),
    (SpeciesId::CROCONAW, &[evolution(EvoMethod::Level(30), SpeciesId::FERALIGATR)]),
    (SpeciesId::SENTRET, &[evolution(EvoMethod::Level(15), SpeciesId::FURRET)]),
    (SpeciesId::HOOTHOOT, &[evolution(EvoMethod::Level(20), SpeciesId::NOCTOWL)]),
    (SpeciesId::LEDYBA, &[evolution(EvoMethod::Level(18), SpeciesId::LEDIAN)]),
    (SpeciesId::SPINARAK, &[evolution(EvoMethod::Level(22), SpeciesId::ARIADOS)]),
    (SpeciesId::CHINCHOU, &[evolution(EvoMethod::Level(27), SpeciesId::LANTURN)]),
    (SpeciesId::PICHU, &[evolution(EvoMethod::Friendship, SpeciesId::PIKACHU)]),
    (SpeciesId::CLEFFA, &[evolution(EvoMethod::Friendship, SpeciesId::CLEFAIRY)]),
    (SpeciesId::IGGLYBUFF, &[evolution(EvoMethod::Friendship, SpeciesId::JIGGLYPUFF)]),
    (SpeciesId::TOGEPI, &[evolution(EvoMethod::Friendship, SpeciesId::TOGETIC)]),
    (SpeciesId::NATU, &[evolution(EvoMethod::Level(25), SpeciesId::XATU)]),
    (SpeciesId::MAREEP, &[evolution(EvoMethod::Level(15), SpeciesId::FLAAFFY)]),
    (SpeciesId::FLAAFFY, &[evolution(EvoMethod::Level(30), SpeciesId::AMPHAROS)]),
    (SpeciesId::MARILL, &[evolution(EvoMethod::Level(18), SpeciesId::AZUMARILL)]),
    (SpeciesId::HOPPIP, &[evolution(EvoMethod::Level(18), SpeciesId::SKIPLOOM)]),
    (SpeciesId::SKIPLOOM, &[evolution(EvoMethod::Level(27), SpeciesId::JUMPLUFF)]),
    (
        SpeciesId::SUNKERN,
        &[evolution(EvoMethod::Item(ItemId::SUN_STONE), SpeciesId::SUNFLORA)],
    ),
    (SpeciesId::WOOPER, &[evolution(EvoMethod::Level(20), SpeciesId::QUAGSIRE)]),
    (SpeciesId::PINECO, &[evolution(EvoMethod::Level(31), SpeciesId::FORRETRESS)]),
    (SpeciesId::SNUBBULL, &[evolution(EvoMethod::Level(23), SpeciesId::GRANBULL)]),
    (SpeciesId::TEDDIURSA, &[evolution(EvoMethod::Level(30), SpeciesId::URSARING)]),
    (SpeciesId::SLUGMA, &[evolution(EvoMethod::Level(38), SpeciesId::MAGCARGO)]),
    (SpeciesId::SWINUB, &[evolution(EvoMethod::Level(33), SpeciesId::PILOSWINE)]),
    (SpeciesId::REMORAID, &[evolution(EvoMethod::Level(25), SpeciesId::OCTILLERY)]),
    (SpeciesId::HOUNDOUR, &[evolution(EvoMethod::Level(24), SpeciesId::HOUNDOOM)]),
    (SpeciesId::PHANPY, &[evolution(EvoMethod::Level(25), SpeciesId::DONPHAN)]),
    (
        SpeciesId::TYROGUE,
        &[
            evolution(EvoMethod::LevelAtkLtDef(20), SpeciesId::HITMONCHAN),
            evolution(EvoMethod::LevelAtkGtDef(20), SpeciesId::HITMONLEE),
            evolution(EvoMethod::LevelAtkEqDef(20), SpeciesId::HITMONTOP),
        ],
    ),
    (SpeciesId::SMOOCHUM, &[evolution(EvoMethod::Level(30), SpeciesId::JYNX)]),
    (SpeciesId::ELEKID, &[evolution(EvoMethod::Level(30), SpeciesId::ELECTABUZZ)]),
    (SpeciesId::MAGBY, &[evolution(EvoMethod::Level(30), SpeciesId::MAGMAR)]),
    (SpeciesId::LARVITAR, &[evolution(EvoMethod::Level(30), SpeciesId::PUPITAR)]),
    (SpeciesId::PUPITAR, &[evolution(EvoMethod::Level(55), SpeciesId::TYRANITAR)]),
    (SpeciesId::TREECKO, &[evolution(EvoMethod::Level(16), SpeciesId::GROVYLE)]),
    (SpeciesId::GROVYLE, &[evolution(EvoMethod::Level(36), SpeciesId::SCEPTILE)]),
    (SpeciesId::TORCHIC, &[evolution(EvoMethod::Level(16), SpeciesId::COMBUSKEN)]),
    (SpeciesId::COMBUSKEN, &[evolution(EvoMethod::Level(36), SpeciesId::BLAZIKEN)]),
    (SpeciesId::MUDKIP, &[evolution(EvoMethod::Level(16), SpeciesId::MARSHTOMP)]),
    (SpeciesId::MARSHTOMP, &[evolution(EvoMethod::Level(36), SpeciesId::SWAMPERT)]),
    (SpeciesId::POOCHYENA, &[evolution(EvoMethod::Level(18), SpeciesId::MIGHTYENA)]),
    (SpeciesId::ZIGZAGOON, &[evolution(EvoMethod::Level(20), SpeciesId::LINOONE)]),
    (
        SpeciesId::WURMPLE,
        &[
            evolution(EvoMethod::LevelSilcoon(7), SpeciesId::SILCOON),
            evolution(EvoMethod::LevelCascoon(7), SpeciesId::CASCOON),
        ],
    ),
    (SpeciesId::SILCOON, &[evolution(EvoMethod::Level(10), SpeciesId::BEAUTIFLY)]),
    (SpeciesId::CASCOON, &[evolution(EvoMethod::Level(10), SpeciesId::DUSTOX)]),
    (SpeciesId::LOTAD, &[evolution(EvoMethod::Level(14), SpeciesId::LOMBRE)]),
    (
        SpeciesId::LOMBRE,
        &[evolution(EvoMethod::Item(ItemId::WATER_STONE), SpeciesId::LUDICOLO)],
    ),
    (SpeciesId::SEEDOT, &[evolution(EvoMethod::Level(14), SpeciesId::NUZLEAF)]),
    (
        SpeciesId::NUZLEAF,
        &[evolution(EvoMethod::Item(ItemId::LEAF_STONE), SpeciesId::SHIFTRY)],
    ),
    (
        SpeciesId::NINCADA,
        &[
            evolution(EvoMethod::LevelNinjask(20), SpeciesId::NINJASK),
            evolution(EvoMethod::LevelShedinja(20), SpeciesId::SHEDINJA),
        ],
    ),
    (SpeciesId::TAILLOW, &[evolution(EvoMethod::Level(22), SpeciesId::SWELLOW)]),
    (SpeciesId::SHROOMISH, &[evolution(EvoMethod::Level(23), SpeciesId::BRELOOM)]),
    (SpeciesId::WINGULL, &[evolution(EvoMethod::Level(25), SpeciesId::PELIPPER)]),
    (SpeciesId::SURSKIT, &[evolution(EvoMethod::Level(22), SpeciesId::MASQUERAIN)]),
    (SpeciesId::WAILMER, &[evolution(EvoMethod::Level(40), SpeciesId::WAILORD)]),
    (
        SpeciesId::SKITTY,
        &[evolution(EvoMethod::Item(ItemId::MOON_STONE), SpeciesId::DELCATTY)],
    ),
    (SpeciesId::BALTOY, &[evolution(EvoMethod::Level(36), SpeciesId::CLAYDOL)]),
    (SpeciesId::BARBOACH, &[evolution(EvoMethod::Level(30), SpeciesId::WHISCASH)]),
    (SpeciesId::CORPHISH, &[evolution(EvoMethod::Level(30), SpeciesId::CRAWDAUNT)]),
    (SpeciesId::FEEBAS, &[evolution(EvoMethod::Beauty(170), SpeciesId::MILOTIC)]),
    (SpeciesId::CARVANHA, &[evolution(EvoMethod::Level(30), SpeciesId::SHARPEDO)]),
    (SpeciesId::TRAPINCH, &[evolution(EvoMethod::Level(35), SpeciesId::VIBRAVA)]),
    (SpeciesId::VIBRAVA, &[evolution(EvoMethod::Level(45), SpeciesId::FLYGON)]),
    (SpeciesId::MAKUHITA, &[evolution(EvoMethod::Level(24), SpeciesId::HARIYAMA)]),
    (SpeciesId::ELECTRIKE, &[evolution(EvoMethod::Level(26), SpeciesId::MANECTRIC)]),
    (SpeciesId::NUMEL, &[evolution(EvoMethod::Level(33), SpeciesId::CAMERUPT)]),
    (SpeciesId::SPHEAL, &[evolution(EvoMethod::Level(32), SpeciesId::SEALEO)]),
    (SpeciesId::SEALEO, &[evolution(EvoMethod::Level(44), SpeciesId::WALREIN)]),
    (SpeciesId::CACNEA, &[evolution(EvoMethod::Level(32), SpeciesId::CACTURNE)]),
    (SpeciesId::SNORUNT, &[evolution(EvoMethod::Level(42), SpeciesId::GLALIE)]),
    (SpeciesId::AZURILL, &[evolution(EvoMethod::Friendship, SpeciesId::MARILL)]),
    (SpeciesId::SPOINK, &[evolution(EvoMethod::Level(32), SpeciesId::GRUMPIG)]),
    (SpeciesId::MEDITITE, &[evolution(EvoMethod::Level(37), SpeciesId::MEDICHAM)]),
    (SpeciesId::SWABLU, &[evolution(EvoMethod::Level(35), SpeciesId::ALTARIA)]),
    (SpeciesId::WYNAUT, &[evolution(EvoMethod::Level(15), SpeciesId::WOBBUFFET)]),
    (SpeciesId::DUSKULL, &[evolution(EvoMethod::Level(37), SpeciesId::DUSCLOPS)]),
    (SpeciesId::SLAKOTH, &[evolution(EvoMethod::Level(18), SpeciesId::VIGOROTH)]),
    (SpeciesId::VIGOROTH, &[evolution(EvoMethod::Level(36), SpeciesId::SLAKING)]),
    (SpeciesId::GULPIN, &[evolution(EvoMethod::Level(26), SpeciesId::SWALOT)]),
    (SpeciesId::WHISMUR, &[evolution(EvoMethod::Level(20), SpeciesId::LOUDRED)]),
    (SpeciesId::LOUDRED, &[evolution(EvoMethod::Level(40), SpeciesId::EXPLOUD)]),
    (
        SpeciesId::CLAMPERL,
        &[
            evolution(EvoMethod::TradeItem(ItemId::DEEP_SEA_TOOTH), SpeciesId::HUNTAIL),
            evolution(EvoMethod::TradeItem(ItemId::DEEP_SEA_SCALE), SpeciesId::GOREBYSS),
        ],
    ),
    (SpeciesId::SHUPPET, &[evolution(EvoMethod::Level(37), SpeciesId::BANETTE)]),
    (SpeciesId::ARON, &[evolution(EvoMethod::Level(32), SpeciesId::LAIRON)]),
    (SpeciesId::LAIRON, &[evolution(EvoMethod::Level(42), SpeciesId::AGGRON)]),
    (SpeciesId::LILEEP, &[evolution(EvoMethod::Level(40), SpeciesId::CRADILY)]),
    (SpeciesId::ANORITH, &[evolution(EvoMethod::Level(40), SpeciesId::ARMALDO)]),
    (SpeciesId::RALTS, &[evolution(EvoMethod::Level(20), SpeciesId::KIRLIA)]),
    (SpeciesId::KIRLIA, &[evolution(EvoMethod::Level(30), SpeciesId::GARDEVOIR)]),
    (SpeciesId::BAGON, &[evolution(EvoMethod::Level(30), SpeciesId::SHELGON)]),
    (SpeciesId::SHELGON, &[evolution(EvoMethod::Level(50), SpeciesId::SALAMENCE)]),
    (SpeciesId::BELDUM, &[evolution(EvoMethod::Level(20), SpeciesId::METANG)]),
    (SpeciesId::METANG, &[evolution(EvoMethod::Level(45), SpeciesId::METAGROSS)]),
];

/// Evolution rules indexed by source species.
#[derive(Debug, Clone, Copy)]
pub struct EvolutionTable {
    entries: &'static [(SpeciesId, &'static [Evolution])],
}

impl EvolutionTable {
    /// The number of species that have at least one evolution.
    pub const EVOLVING_SPECIES: usize = ENTRIES.len();

    /// Creates the canonical evolution table.
    #[must_use]
    pub const fn new() -> Self {
        Self { entries: ENTRIES }
    }

    /// Returns the rules for `species`, or `None` if it cannot evolve.
    #[must_use]
    pub fn get(&self, species: SpeciesId) -> Option<&'static [Evolution]> {
        self.entries
            .binary_search_by_key(&species, |&(id, _)| id)
            .ok()
            .map(|i| self.entries[i].1)
    }

    /// Returns the rules for `species`, or an empty slice if it cannot evolve.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownSpecies`] if `species` is outside
    /// `0..`[`SpeciesTable::LEN`].
    ///
    /// [`SpeciesTable::LEN`]: crate::species::SpeciesTable::LEN
    pub fn evolutions(&self, species: SpeciesId) -> Result<&'static [Evolution], AssetError> {
        if species.0 as usize >= crate::species::SpeciesTable::LEN {
            return Err(AssetError::UnknownSpecies(species.0));
        }
        Ok(self.get(species).unwrap_or(&[]))
    }

    /// Returns the number of evolution rules, including every branch.
    #[must_use]
    pub fn total_evolutions(&self) -> usize {
        self.entries.iter().map(|&(_, evs)| evs.len()).sum()
    }
}

impl Default for EvolutionTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{EvoMethod, EvolutionTable};
    use crate::error::AssetError;
    use crate::species::{ItemId, SpeciesId};

    #[test]
    fn level_evolution_pins_bulbasaur() {
        let table = EvolutionTable::new();
        let evs = table.get(SpeciesId::BULBASAUR).unwrap();
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].method, EvoMethod::Level(16));
        assert_eq!(evs[0].target, SpeciesId::IVYSAUR);
    }

    #[test]
    fn used_item_evolution_pins_pikachu() {
        let table = EvolutionTable::new();
        let evs = table.get(SpeciesId::PIKACHU).unwrap();
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].method, EvoMethod::Item(ItemId::THUNDER_STONE));
        assert_eq!(evs[0].target, SpeciesId::RAICHU);
    }

    #[test]
    fn trade_item_evolution_pins_seadra() {
        let table = EvolutionTable::new();
        let evs = table.get(SpeciesId::SEADRA).unwrap();
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].method, EvoMethod::TradeItem(ItemId::DRAGON_SCALE));
        assert_eq!(evs[0].target, SpeciesId::KINGDRA);
    }

    #[test]
    fn friendship_evolution_pins_golbat() {
        let table = EvolutionTable::new();
        let evs = table.get(SpeciesId::GOLBAT).unwrap();
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].method, EvoMethod::Friendship);
        assert_eq!(evs[0].method.param(), 0);
        assert_eq!(evs[0].target, SpeciesId::CROBAT);
    }

    #[test]
    fn two_way_branch_pins_gloom() {
        let table = EvolutionTable::new();
        let evs = table.get(SpeciesId::GLOOM).unwrap();
        assert_eq!(evs.len(), 2);
        assert_eq!(evs[0].method, EvoMethod::Item(ItemId::LEAF_STONE));
        assert_eq!(evs[0].target, SpeciesId::VILEPLUME);
        assert_eq!(evs[1].method, EvoMethod::Item(ItemId::SUN_STONE));
        assert_eq!(evs[1].target, SpeciesId::BELLOSSOM);
    }

    #[test]
    fn five_way_branch_pins_eevee() {
        let table = EvolutionTable::new();
        let evs = table.get(SpeciesId::EEVEE).unwrap();
        assert_eq!(evs.len(), 5);
        assert_eq!(evs[0].method, EvoMethod::Item(ItemId::THUNDER_STONE));
        assert_eq!(evs[0].target, SpeciesId::JOLTEON);
        assert_eq!(evs[1].method, EvoMethod::Item(ItemId::WATER_STONE));
        assert_eq!(evs[1].target, SpeciesId::VAPOREON);
        assert_eq!(evs[2].method, EvoMethod::Item(ItemId::FIRE_STONE));
        assert_eq!(evs[2].target, SpeciesId::FLAREON);
        assert_eq!(evs[3].method, EvoMethod::FriendshipDay);
        assert_eq!(evs[3].target, SpeciesId::ESPEON);
        assert_eq!(evs[4].method, EvoMethod::FriendshipNight);
        assert_eq!(evs[4].target, SpeciesId::UMBREON);
    }

    #[test]
    fn stone_evolution_parameters_pin_raw_item_ids() {
        let table = EvolutionTable::new();
        assert_eq!(table.get(SpeciesId::VULPIX).unwrap()[0].method.param(), 95);
        assert_eq!(table.get(SpeciesId::EEVEE).unwrap()[1].method.param(), 97);
        assert_eq!(table.get(SpeciesId::GLOOM).unwrap()[0].method.param(), 98);
    }

    #[test]
    fn trade_item_evolution_parameters_pin_raw_item_ids() {
        let table = EvolutionTable::new();
        assert_eq!(
            table.get(SpeciesId::CLAMPERL).unwrap()[0].method.param(),
            192
        );
        assert_eq!(
            table.get(SpeciesId::CLAMPERL).unwrap()[1].method.param(),
            193
        );
        assert_eq!(
            table.get(SpeciesId::PORYGON).unwrap()[0].method.param(),
            218
        );
    }

    #[test]
    fn method_ids_and_parameters_match_stored_values() {
        assert_eq!(EvoMethod::Friendship.method_id(), 1);
        assert_eq!(EvoMethod::FriendshipDay.method_id(), 2);
        assert_eq!(EvoMethod::FriendshipNight.method_id(), 3);
        assert_eq!(EvoMethod::Level(16).method_id(), 4);
        assert_eq!(EvoMethod::Trade.method_id(), 5);
        assert_eq!(EvoMethod::TradeItem(ItemId::METAL_COAT).method_id(), 6);
        assert_eq!(EvoMethod::Item(ItemId::THUNDER_STONE).method_id(), 7);
        assert_eq!(EvoMethod::LevelAtkGtDef(20).method_id(), 8);
        assert_eq!(EvoMethod::LevelAtkEqDef(20).method_id(), 9);
        assert_eq!(EvoMethod::LevelAtkLtDef(20).method_id(), 10);
        assert_eq!(EvoMethod::LevelSilcoon(7).method_id(), 11);
        assert_eq!(EvoMethod::LevelCascoon(7).method_id(), 12);
        assert_eq!(EvoMethod::LevelNinjask(20).method_id(), 13);
        assert_eq!(EvoMethod::LevelShedinja(20).method_id(), 14);
        assert_eq!(EvoMethod::Beauty(170).method_id(), 15);

        assert_eq!(EvoMethod::Level(16).param(), 16);
        assert_eq!(EvoMethod::Item(ItemId::THUNDER_STONE).param(), 96);
        assert_eq!(EvoMethod::TradeItem(ItemId::METAL_COAT).param(), 199);
        assert_eq!(EvoMethod::Beauty(170).param(), 170);
        assert_eq!(EvoMethod::Trade.param(), 0);
    }

    #[test]
    fn counts_match_upstream() {
        let table = EvolutionTable::new();
        assert_eq!(EvolutionTable::EVOLVING_SPECIES, 172);
        assert_eq!(table.total_evolutions(), 184);
    }

    #[test]
    fn entries_are_sorted_and_unique_by_source() {
        for w in super::ENTRIES.windows(2) {
            assert!(
                w[0].0 < w[1].0,
                "sources not strictly increasing near {:?}",
                w[0].0
            );
        }
    }

    #[test]
    fn every_entry_list_is_non_empty() {
        for &(src, evs) in super::ENTRIES {
            assert!(!evs.is_empty(), "empty entry list for {src:?}");
        }
    }

    #[test]
    fn non_evolving_species_yields_empty_not_error() {
        let table = EvolutionTable::new();
        assert_eq!(table.get(SpeciesId::CHARIZARD), None);
        assert_eq!(table.evolutions(SpeciesId::CHARIZARD), Ok(&[][..]));
    }

    #[test]
    fn out_of_range_species_is_an_error() {
        let table = EvolutionTable::new();
        let bad = SpeciesId(u16::MAX);
        assert_eq!(
            table.evolutions(bad),
            Err(AssetError::UnknownSpecies(u16::MAX)),
        );
    }

    #[test]
    fn every_target_is_in_range() {
        let len = crate::species::SpeciesTable::LEN_U16;
        for &(_, evs) in super::ENTRIES {
            for ev in evs {
                assert!(ev.target.0 < len, "target {:?} out of range", ev.target);
            }
        }
    }
}
