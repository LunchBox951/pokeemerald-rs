//! Evolution table (S-4): the `gEvolutionTable` species-evolution data.
//!
//! Ports every species' evolution entries from the upstream reference
//! `pokeemerald/src/data/pokemon/evolution.h` (`const struct Evolution
//! gEvolutionTable[NUM_SPECIES][EVOS_PER_MON]`). The record layout is `struct
//! Evolution` in `pokeemerald/include/pokemon.h` (`{ u16 method; u16 param; u16
//! targetSpecies; }`); the `EVO_*` method constants live in
//! `pokeemerald/include/constants/pokemon.h`.
//!
//! The upstream `param` field is a single overloaded `u16` whose meaning
//! depends on the method (a level, a used/held [`ItemId`], a beauty threshold,
//! or unused). Rather than carry a bare integer `(behavioral-fidelity)`, each
//! method is modelled as an [`EvoMethod`] variant that carries its param in its
//! own typed shape, so an ill-formed pairing (e.g. an item id where a level
//! belongs) is unrepresentable. Empty `{0, 0, 0}` filler slots — upstream pads
//! every species out to `EVOS_PER_MON` (5) — carry no information and are
//! dropped: a species with no evolutions simply has an empty entry list.
//!
//! The data is re-expressed idiomatically rather than the C copied
//! `(no-verbatim)`, and pinned back to the upstream values by the unit tests
//! below, which sample a level evolution, a used-item evolution, a
//! traded-with-item evolution, a friendship evolution, and both a two- and a
//! five-way branch, and assert the total entry count.

use crate::error::AssetError;
use crate::species::{ItemId, SpeciesId};

/// How a species evolves — the upstream `EVO_*` method together with its
/// (method-specific) `param`.
///
/// Discriminant values are *not* significant here; the raw upstream method id
/// is recovered through [`EvoMethod::method_id`], and the param through the
/// data each variant carries. Variants map one-to-one onto the fifteen
/// `EVO_*` constants (`1..=15`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EvoMethod {
    /// `EVO_FRIENDSHIP` (1): levels up with friendship high enough. No param.
    Friendship,
    /// `EVO_FRIENDSHIP_DAY` (2): as [`Friendship`](Self::Friendship), by day.
    FriendshipDay,
    /// `EVO_FRIENDSHIP_NIGHT` (3): as [`Friendship`](Self::Friendship), by
    /// night.
    FriendshipNight,
    /// `EVO_LEVEL` (4): reaches the carried level.
    Level(u8),
    /// `EVO_TRADE` (5): is traded. No param.
    Trade,
    /// `EVO_TRADE_ITEM` (6): is traded while holding the carried item.
    TradeItem(ItemId),
    /// `EVO_ITEM` (7): the carried item is used on it.
    Item(ItemId),
    /// `EVO_LEVEL_ATK_GT_DEF` (8): reaches the carried level with attack >
    /// defense.
    LevelAtkGtDef(u8),
    /// `EVO_LEVEL_ATK_EQ_DEF` (9): reaches the carried level with attack =
    /// defense.
    LevelAtkEqDef(u8),
    /// `EVO_LEVEL_ATK_LT_DEF` (10): reaches the carried level with attack <
    /// defense.
    LevelAtkLtDef(u8),
    /// `EVO_LEVEL_SILCOON` (11): reaches the carried level with a Silcoon
    /// personality.
    LevelSilcoon(u8),
    /// `EVO_LEVEL_CASCOON` (12): reaches the carried level with a Cascoon
    /// personality.
    LevelCascoon(u8),
    /// `EVO_LEVEL_NINJASK` (13): reaches the carried level (the Ninjask half of
    /// Nincada's split).
    LevelNinjask(u8),
    /// `EVO_LEVEL_SHEDINJA` (14): reaches the carried level (the Shedinja half
    /// of Nincada's split).
    LevelShedinja(u8),
    /// `EVO_BEAUTY` (15): levels up with beauty at least the carried value.
    Beauty(u16),
}

impl EvoMethod {
    /// The raw upstream `EVO_*` method id (`1..=15`).
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

    /// The raw upstream `param` value for this method — a level, an [`ItemId`]
    /// raw id, a beauty threshold, or `0` for the param-less methods.
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

/// One evolution: the [`method`](EvoMethod) that triggers it and the
/// [`SpeciesId`] it produces — the owned Rust form of `struct Evolution` (the
/// source species is the map key, not a field).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Evolution {
    /// How the evolution is triggered, with its param.
    pub method: EvoMethod,
    /// The species the source evolves into (`targetSpecies` upstream).
    pub target: SpeciesId,
}

/// Construct one [`Evolution`]. A terse free helper so the transcribed table
/// stays readable.
const fn e(method: EvoMethod, target: SpeciesId) -> Evolution {
    Evolution { method, target }
}

/// The transcribed `gEvolutionTable`, as `(source, entries)` pairs in ascending
/// [`SpeciesId`] order. Only species that actually evolve appear; the upstream
/// `{0, 0, 0}` filler slots are dropped, so each list holds exactly the real
/// evolutions for that species (one, or several for a branched evolver).
const ENTRIES: &[(SpeciesId, &[Evolution])] = &[
    // BULBASAUR -> IVYSAUR
    (SpeciesId(1), &[e(EvoMethod::Level(16), SpeciesId(2))]),
    // IVYSAUR -> VENUSAUR
    (SpeciesId(2), &[e(EvoMethod::Level(32), SpeciesId(3))]),
    // CHARMANDER -> CHARMELEON
    (SpeciesId(4), &[e(EvoMethod::Level(16), SpeciesId(5))]),
    // CHARMELEON -> CHARIZARD
    (SpeciesId(5), &[e(EvoMethod::Level(36), SpeciesId(6))]),
    // SQUIRTLE -> WARTORTLE
    (SpeciesId(7), &[e(EvoMethod::Level(16), SpeciesId(8))]),
    // WARTORTLE -> BLASTOISE
    (SpeciesId(8), &[e(EvoMethod::Level(36), SpeciesId(9))]),
    // CATERPIE -> METAPOD
    (SpeciesId(10), &[e(EvoMethod::Level(7), SpeciesId(11))]),
    // METAPOD -> BUTTERFREE
    (SpeciesId(11), &[e(EvoMethod::Level(10), SpeciesId(12))]),
    // WEEDLE -> KAKUNA
    (SpeciesId(13), &[e(EvoMethod::Level(7), SpeciesId(14))]),
    // KAKUNA -> BEEDRILL
    (SpeciesId(14), &[e(EvoMethod::Level(10), SpeciesId(15))]),
    // PIDGEY -> PIDGEOTTO
    (SpeciesId(16), &[e(EvoMethod::Level(18), SpeciesId(17))]),
    // PIDGEOTTO -> PIDGEOT
    (SpeciesId(17), &[e(EvoMethod::Level(36), SpeciesId(18))]),
    // RATTATA -> RATICATE
    (SpeciesId(19), &[e(EvoMethod::Level(20), SpeciesId(20))]),
    // SPEAROW -> FEAROW
    (SpeciesId(21), &[e(EvoMethod::Level(20), SpeciesId(22))]),
    // EKANS -> ARBOK
    (SpeciesId(23), &[e(EvoMethod::Level(22), SpeciesId(24))]),
    // PIKACHU -> RAICHU
    (
        SpeciesId(25),
        &[e(EvoMethod::Item(ItemId(96)), SpeciesId(26))],
    ),
    // SANDSHREW -> SANDSLASH
    (SpeciesId(27), &[e(EvoMethod::Level(22), SpeciesId(28))]),
    // NIDORAN_F -> NIDORINA
    (SpeciesId(29), &[e(EvoMethod::Level(16), SpeciesId(30))]),
    // NIDORINA -> NIDOQUEEN
    (
        SpeciesId(30),
        &[e(EvoMethod::Item(ItemId(94)), SpeciesId(31))],
    ),
    // NIDORAN_M -> NIDORINO
    (SpeciesId(32), &[e(EvoMethod::Level(16), SpeciesId(33))]),
    // NIDORINO -> NIDOKING
    (
        SpeciesId(33),
        &[e(EvoMethod::Item(ItemId(94)), SpeciesId(34))],
    ),
    // CLEFAIRY -> CLEFABLE
    (
        SpeciesId(35),
        &[e(EvoMethod::Item(ItemId(94)), SpeciesId(36))],
    ),
    // VULPIX -> NINETALES
    (
        SpeciesId(37),
        &[e(EvoMethod::Item(ItemId(95)), SpeciesId(38))],
    ),
    // JIGGLYPUFF -> WIGGLYTUFF
    (
        SpeciesId(39),
        &[e(EvoMethod::Item(ItemId(94)), SpeciesId(40))],
    ),
    // ZUBAT -> GOLBAT
    (SpeciesId(41), &[e(EvoMethod::Level(22), SpeciesId(42))]),
    // GOLBAT -> CROBAT
    (SpeciesId(42), &[e(EvoMethod::Friendship, SpeciesId(169))]),
    // ODDISH -> GLOOM
    (SpeciesId(43), &[e(EvoMethod::Level(21), SpeciesId(44))]),
    // GLOOM (branched)
    (
        SpeciesId(44),
        &[
            e(EvoMethod::Item(ItemId(98)), SpeciesId(45)), // -> VILEPLUME
            e(EvoMethod::Item(ItemId(93)), SpeciesId(182)), // -> BELLOSSOM
        ],
    ),
    // PARAS -> PARASECT
    (SpeciesId(46), &[e(EvoMethod::Level(24), SpeciesId(47))]),
    // VENONAT -> VENOMOTH
    (SpeciesId(48), &[e(EvoMethod::Level(31), SpeciesId(49))]),
    // DIGLETT -> DUGTRIO
    (SpeciesId(50), &[e(EvoMethod::Level(26), SpeciesId(51))]),
    // MEOWTH -> PERSIAN
    (SpeciesId(52), &[e(EvoMethod::Level(28), SpeciesId(53))]),
    // PSYDUCK -> GOLDUCK
    (SpeciesId(54), &[e(EvoMethod::Level(33), SpeciesId(55))]),
    // MANKEY -> PRIMEAPE
    (SpeciesId(56), &[e(EvoMethod::Level(28), SpeciesId(57))]),
    // GROWLITHE -> ARCANINE
    (
        SpeciesId(58),
        &[e(EvoMethod::Item(ItemId(95)), SpeciesId(59))],
    ),
    // POLIWAG -> POLIWHIRL
    (SpeciesId(60), &[e(EvoMethod::Level(25), SpeciesId(61))]),
    // POLIWHIRL (branched)
    (
        SpeciesId(61),
        &[
            e(EvoMethod::Item(ItemId(97)), SpeciesId(62)), // -> POLIWRATH
            e(EvoMethod::TradeItem(ItemId(187)), SpeciesId(186)), // -> POLITOED
        ],
    ),
    // ABRA -> KADABRA
    (SpeciesId(63), &[e(EvoMethod::Level(16), SpeciesId(64))]),
    // KADABRA -> ALAKAZAM
    (SpeciesId(64), &[e(EvoMethod::Trade, SpeciesId(65))]),
    // MACHOP -> MACHOKE
    (SpeciesId(66), &[e(EvoMethod::Level(28), SpeciesId(67))]),
    // MACHOKE -> MACHAMP
    (SpeciesId(67), &[e(EvoMethod::Trade, SpeciesId(68))]),
    // BELLSPROUT -> WEEPINBELL
    (SpeciesId(69), &[e(EvoMethod::Level(21), SpeciesId(70))]),
    // WEEPINBELL -> VICTREEBEL
    (
        SpeciesId(70),
        &[e(EvoMethod::Item(ItemId(98)), SpeciesId(71))],
    ),
    // TENTACOOL -> TENTACRUEL
    (SpeciesId(72), &[e(EvoMethod::Level(30), SpeciesId(73))]),
    // GEODUDE -> GRAVELER
    (SpeciesId(74), &[e(EvoMethod::Level(25), SpeciesId(75))]),
    // GRAVELER -> GOLEM
    (SpeciesId(75), &[e(EvoMethod::Trade, SpeciesId(76))]),
    // PONYTA -> RAPIDASH
    (SpeciesId(77), &[e(EvoMethod::Level(40), SpeciesId(78))]),
    // SLOWPOKE (branched)
    (
        SpeciesId(79),
        &[
            e(EvoMethod::Level(37), SpeciesId(80)), // -> SLOWBRO
            e(EvoMethod::TradeItem(ItemId(187)), SpeciesId(199)), // -> SLOWKING
        ],
    ),
    // MAGNEMITE -> MAGNETON
    (SpeciesId(81), &[e(EvoMethod::Level(30), SpeciesId(82))]),
    // DODUO -> DODRIO
    (SpeciesId(84), &[e(EvoMethod::Level(31), SpeciesId(85))]),
    // SEEL -> DEWGONG
    (SpeciesId(86), &[e(EvoMethod::Level(34), SpeciesId(87))]),
    // GRIMER -> MUK
    (SpeciesId(88), &[e(EvoMethod::Level(38), SpeciesId(89))]),
    // SHELLDER -> CLOYSTER
    (
        SpeciesId(90),
        &[e(EvoMethod::Item(ItemId(97)), SpeciesId(91))],
    ),
    // GASTLY -> HAUNTER
    (SpeciesId(92), &[e(EvoMethod::Level(25), SpeciesId(93))]),
    // HAUNTER -> GENGAR
    (SpeciesId(93), &[e(EvoMethod::Trade, SpeciesId(94))]),
    // ONIX -> STEELIX
    (
        SpeciesId(95),
        &[e(EvoMethod::TradeItem(ItemId(199)), SpeciesId(208))],
    ),
    // DROWZEE -> HYPNO
    (SpeciesId(96), &[e(EvoMethod::Level(26), SpeciesId(97))]),
    // KRABBY -> KINGLER
    (SpeciesId(98), &[e(EvoMethod::Level(28), SpeciesId(99))]),
    // VOLTORB -> ELECTRODE
    (SpeciesId(100), &[e(EvoMethod::Level(30), SpeciesId(101))]),
    // EXEGGCUTE -> EXEGGUTOR
    (
        SpeciesId(102),
        &[e(EvoMethod::Item(ItemId(98)), SpeciesId(103))],
    ),
    // CUBONE -> MAROWAK
    (SpeciesId(104), &[e(EvoMethod::Level(28), SpeciesId(105))]),
    // KOFFING -> WEEZING
    (SpeciesId(109), &[e(EvoMethod::Level(35), SpeciesId(110))]),
    // RHYHORN -> RHYDON
    (SpeciesId(111), &[e(EvoMethod::Level(42), SpeciesId(112))]),
    // CHANSEY -> BLISSEY
    (SpeciesId(113), &[e(EvoMethod::Friendship, SpeciesId(242))]),
    // HORSEA -> SEADRA
    (SpeciesId(116), &[e(EvoMethod::Level(32), SpeciesId(117))]),
    // SEADRA -> KINGDRA
    (
        SpeciesId(117),
        &[e(EvoMethod::TradeItem(ItemId(201)), SpeciesId(230))],
    ),
    // GOLDEEN -> SEAKING
    (SpeciesId(118), &[e(EvoMethod::Level(33), SpeciesId(119))]),
    // STARYU -> STARMIE
    (
        SpeciesId(120),
        &[e(EvoMethod::Item(ItemId(97)), SpeciesId(121))],
    ),
    // SCYTHER -> SCIZOR
    (
        SpeciesId(123),
        &[e(EvoMethod::TradeItem(ItemId(199)), SpeciesId(212))],
    ),
    // MAGIKARP -> GYARADOS
    (SpeciesId(129), &[e(EvoMethod::Level(20), SpeciesId(130))]),
    // EEVEE (branched)
    (
        SpeciesId(133),
        &[
            e(EvoMethod::Item(ItemId(96)), SpeciesId(135)), // -> JOLTEON
            e(EvoMethod::Item(ItemId(97)), SpeciesId(134)), // -> VAPOREON
            e(EvoMethod::Item(ItemId(95)), SpeciesId(136)), // -> FLAREON
            e(EvoMethod::FriendshipDay, SpeciesId(196)),    // -> ESPEON
            e(EvoMethod::FriendshipNight, SpeciesId(197)),  // -> UMBREON
        ],
    ),
    // PORYGON -> PORYGON2
    (
        SpeciesId(137),
        &[e(EvoMethod::TradeItem(ItemId(218)), SpeciesId(233))],
    ),
    // OMANYTE -> OMASTAR
    (SpeciesId(138), &[e(EvoMethod::Level(40), SpeciesId(139))]),
    // KABUTO -> KABUTOPS
    (SpeciesId(140), &[e(EvoMethod::Level(40), SpeciesId(141))]),
    // DRATINI -> DRAGONAIR
    (SpeciesId(147), &[e(EvoMethod::Level(30), SpeciesId(148))]),
    // DRAGONAIR -> DRAGONITE
    (SpeciesId(148), &[e(EvoMethod::Level(55), SpeciesId(149))]),
    // CHIKORITA -> BAYLEEF
    (SpeciesId(152), &[e(EvoMethod::Level(16), SpeciesId(153))]),
    // BAYLEEF -> MEGANIUM
    (SpeciesId(153), &[e(EvoMethod::Level(32), SpeciesId(154))]),
    // CYNDAQUIL -> QUILAVA
    (SpeciesId(155), &[e(EvoMethod::Level(14), SpeciesId(156))]),
    // QUILAVA -> TYPHLOSION
    (SpeciesId(156), &[e(EvoMethod::Level(36), SpeciesId(157))]),
    // TOTODILE -> CROCONAW
    (SpeciesId(158), &[e(EvoMethod::Level(18), SpeciesId(159))]),
    // CROCONAW -> FERALIGATR
    (SpeciesId(159), &[e(EvoMethod::Level(30), SpeciesId(160))]),
    // SENTRET -> FURRET
    (SpeciesId(161), &[e(EvoMethod::Level(15), SpeciesId(162))]),
    // HOOTHOOT -> NOCTOWL
    (SpeciesId(163), &[e(EvoMethod::Level(20), SpeciesId(164))]),
    // LEDYBA -> LEDIAN
    (SpeciesId(165), &[e(EvoMethod::Level(18), SpeciesId(166))]),
    // SPINARAK -> ARIADOS
    (SpeciesId(167), &[e(EvoMethod::Level(22), SpeciesId(168))]),
    // CHINCHOU -> LANTURN
    (SpeciesId(170), &[e(EvoMethod::Level(27), SpeciesId(171))]),
    // PICHU -> PIKACHU
    (SpeciesId(172), &[e(EvoMethod::Friendship, SpeciesId(25))]),
    // CLEFFA -> CLEFAIRY
    (SpeciesId(173), &[e(EvoMethod::Friendship, SpeciesId(35))]),
    // IGGLYBUFF -> JIGGLYPUFF
    (SpeciesId(174), &[e(EvoMethod::Friendship, SpeciesId(39))]),
    // TOGEPI -> TOGETIC
    (SpeciesId(175), &[e(EvoMethod::Friendship, SpeciesId(176))]),
    // NATU -> XATU
    (SpeciesId(177), &[e(EvoMethod::Level(25), SpeciesId(178))]),
    // MAREEP -> FLAAFFY
    (SpeciesId(179), &[e(EvoMethod::Level(15), SpeciesId(180))]),
    // FLAAFFY -> AMPHAROS
    (SpeciesId(180), &[e(EvoMethod::Level(30), SpeciesId(181))]),
    // MARILL -> AZUMARILL
    (SpeciesId(183), &[e(EvoMethod::Level(18), SpeciesId(184))]),
    // HOPPIP -> SKIPLOOM
    (SpeciesId(187), &[e(EvoMethod::Level(18), SpeciesId(188))]),
    // SKIPLOOM -> JUMPLUFF
    (SpeciesId(188), &[e(EvoMethod::Level(27), SpeciesId(189))]),
    // SUNKERN -> SUNFLORA
    (
        SpeciesId(191),
        &[e(EvoMethod::Item(ItemId(93)), SpeciesId(192))],
    ),
    // WOOPER -> QUAGSIRE
    (SpeciesId(194), &[e(EvoMethod::Level(20), SpeciesId(195))]),
    // PINECO -> FORRETRESS
    (SpeciesId(204), &[e(EvoMethod::Level(31), SpeciesId(205))]),
    // SNUBBULL -> GRANBULL
    (SpeciesId(209), &[e(EvoMethod::Level(23), SpeciesId(210))]),
    // TEDDIURSA -> URSARING
    (SpeciesId(216), &[e(EvoMethod::Level(30), SpeciesId(217))]),
    // SLUGMA -> MAGCARGO
    (SpeciesId(218), &[e(EvoMethod::Level(38), SpeciesId(219))]),
    // SWINUB -> PILOSWINE
    (SpeciesId(220), &[e(EvoMethod::Level(33), SpeciesId(221))]),
    // REMORAID -> OCTILLERY
    (SpeciesId(223), &[e(EvoMethod::Level(25), SpeciesId(224))]),
    // HOUNDOUR -> HOUNDOOM
    (SpeciesId(228), &[e(EvoMethod::Level(24), SpeciesId(229))]),
    // PHANPY -> DONPHAN
    (SpeciesId(231), &[e(EvoMethod::Level(25), SpeciesId(232))]),
    // TYROGUE (branched)
    (
        SpeciesId(236),
        &[
            e(EvoMethod::LevelAtkLtDef(20), SpeciesId(107)), // -> HITMONCHAN
            e(EvoMethod::LevelAtkGtDef(20), SpeciesId(106)), // -> HITMONLEE
            e(EvoMethod::LevelAtkEqDef(20), SpeciesId(237)), // -> HITMONTOP
        ],
    ),
    // SMOOCHUM -> JYNX
    (SpeciesId(238), &[e(EvoMethod::Level(30), SpeciesId(124))]),
    // ELEKID -> ELECTABUZZ
    (SpeciesId(239), &[e(EvoMethod::Level(30), SpeciesId(125))]),
    // MAGBY -> MAGMAR
    (SpeciesId(240), &[e(EvoMethod::Level(30), SpeciesId(126))]),
    // LARVITAR -> PUPITAR
    (SpeciesId(246), &[e(EvoMethod::Level(30), SpeciesId(247))]),
    // PUPITAR -> TYRANITAR
    (SpeciesId(247), &[e(EvoMethod::Level(55), SpeciesId(248))]),
    // TREECKO -> GROVYLE
    (SpeciesId(277), &[e(EvoMethod::Level(16), SpeciesId(278))]),
    // GROVYLE -> SCEPTILE
    (SpeciesId(278), &[e(EvoMethod::Level(36), SpeciesId(279))]),
    // TORCHIC -> COMBUSKEN
    (SpeciesId(280), &[e(EvoMethod::Level(16), SpeciesId(281))]),
    // COMBUSKEN -> BLAZIKEN
    (SpeciesId(281), &[e(EvoMethod::Level(36), SpeciesId(282))]),
    // MUDKIP -> MARSHTOMP
    (SpeciesId(283), &[e(EvoMethod::Level(16), SpeciesId(284))]),
    // MARSHTOMP -> SWAMPERT
    (SpeciesId(284), &[e(EvoMethod::Level(36), SpeciesId(285))]),
    // POOCHYENA -> MIGHTYENA
    (SpeciesId(286), &[e(EvoMethod::Level(18), SpeciesId(287))]),
    // ZIGZAGOON -> LINOONE
    (SpeciesId(288), &[e(EvoMethod::Level(20), SpeciesId(289))]),
    // WURMPLE (branched)
    (
        SpeciesId(290),
        &[
            e(EvoMethod::LevelSilcoon(7), SpeciesId(291)), // -> SILCOON
            e(EvoMethod::LevelCascoon(7), SpeciesId(293)), // -> CASCOON
        ],
    ),
    // SILCOON -> BEAUTIFLY
    (SpeciesId(291), &[e(EvoMethod::Level(10), SpeciesId(292))]),
    // CASCOON -> DUSTOX
    (SpeciesId(293), &[e(EvoMethod::Level(10), SpeciesId(294))]),
    // LOTAD -> LOMBRE
    (SpeciesId(295), &[e(EvoMethod::Level(14), SpeciesId(296))]),
    // LOMBRE -> LUDICOLO
    (
        SpeciesId(296),
        &[e(EvoMethod::Item(ItemId(97)), SpeciesId(297))],
    ),
    // SEEDOT -> NUZLEAF
    (SpeciesId(298), &[e(EvoMethod::Level(14), SpeciesId(299))]),
    // NUZLEAF -> SHIFTRY
    (
        SpeciesId(299),
        &[e(EvoMethod::Item(ItemId(98)), SpeciesId(300))],
    ),
    // NINCADA (branched)
    (
        SpeciesId(301),
        &[
            e(EvoMethod::LevelNinjask(20), SpeciesId(302)), // -> NINJASK
            e(EvoMethod::LevelShedinja(20), SpeciesId(303)), // -> SHEDINJA
        ],
    ),
    // TAILLOW -> SWELLOW
    (SpeciesId(304), &[e(EvoMethod::Level(22), SpeciesId(305))]),
    // SHROOMISH -> BRELOOM
    (SpeciesId(306), &[e(EvoMethod::Level(23), SpeciesId(307))]),
    // WINGULL -> PELIPPER
    (SpeciesId(309), &[e(EvoMethod::Level(25), SpeciesId(310))]),
    // SURSKIT -> MASQUERAIN
    (SpeciesId(311), &[e(EvoMethod::Level(22), SpeciesId(312))]),
    // WAILMER -> WAILORD
    (SpeciesId(313), &[e(EvoMethod::Level(40), SpeciesId(314))]),
    // SKITTY -> DELCATTY
    (
        SpeciesId(315),
        &[e(EvoMethod::Item(ItemId(94)), SpeciesId(316))],
    ),
    // BALTOY -> CLAYDOL
    (SpeciesId(318), &[e(EvoMethod::Level(36), SpeciesId(319))]),
    // BARBOACH -> WHISCASH
    (SpeciesId(323), &[e(EvoMethod::Level(30), SpeciesId(324))]),
    // CORPHISH -> CRAWDAUNT
    (SpeciesId(326), &[e(EvoMethod::Level(30), SpeciesId(327))]),
    // FEEBAS -> MILOTIC
    (SpeciesId(328), &[e(EvoMethod::Beauty(170), SpeciesId(329))]),
    // CARVANHA -> SHARPEDO
    (SpeciesId(330), &[e(EvoMethod::Level(30), SpeciesId(331))]),
    // TRAPINCH -> VIBRAVA
    (SpeciesId(332), &[e(EvoMethod::Level(35), SpeciesId(333))]),
    // VIBRAVA -> FLYGON
    (SpeciesId(333), &[e(EvoMethod::Level(45), SpeciesId(334))]),
    // MAKUHITA -> HARIYAMA
    (SpeciesId(335), &[e(EvoMethod::Level(24), SpeciesId(336))]),
    // ELECTRIKE -> MANECTRIC
    (SpeciesId(337), &[e(EvoMethod::Level(26), SpeciesId(338))]),
    // NUMEL -> CAMERUPT
    (SpeciesId(339), &[e(EvoMethod::Level(33), SpeciesId(340))]),
    // SPHEAL -> SEALEO
    (SpeciesId(341), &[e(EvoMethod::Level(32), SpeciesId(342))]),
    // SEALEO -> WALREIN
    (SpeciesId(342), &[e(EvoMethod::Level(44), SpeciesId(343))]),
    // CACNEA -> CACTURNE
    (SpeciesId(344), &[e(EvoMethod::Level(32), SpeciesId(345))]),
    // SNORUNT -> GLALIE
    (SpeciesId(346), &[e(EvoMethod::Level(42), SpeciesId(347))]),
    // AZURILL -> MARILL
    (SpeciesId(350), &[e(EvoMethod::Friendship, SpeciesId(183))]),
    // SPOINK -> GRUMPIG
    (SpeciesId(351), &[e(EvoMethod::Level(32), SpeciesId(352))]),
    // MEDITITE -> MEDICHAM
    (SpeciesId(356), &[e(EvoMethod::Level(37), SpeciesId(357))]),
    // SWABLU -> ALTARIA
    (SpeciesId(358), &[e(EvoMethod::Level(35), SpeciesId(359))]),
    // WYNAUT -> WOBBUFFET
    (SpeciesId(360), &[e(EvoMethod::Level(15), SpeciesId(202))]),
    // DUSKULL -> DUSCLOPS
    (SpeciesId(361), &[e(EvoMethod::Level(37), SpeciesId(362))]),
    // SLAKOTH -> VIGOROTH
    (SpeciesId(364), &[e(EvoMethod::Level(18), SpeciesId(365))]),
    // VIGOROTH -> SLAKING
    (SpeciesId(365), &[e(EvoMethod::Level(36), SpeciesId(366))]),
    // GULPIN -> SWALOT
    (SpeciesId(367), &[e(EvoMethod::Level(26), SpeciesId(368))]),
    // WHISMUR -> LOUDRED
    (SpeciesId(370), &[e(EvoMethod::Level(20), SpeciesId(371))]),
    // LOUDRED -> EXPLOUD
    (SpeciesId(371), &[e(EvoMethod::Level(40), SpeciesId(372))]),
    // CLAMPERL (branched)
    (
        SpeciesId(373),
        &[
            e(EvoMethod::TradeItem(ItemId(192)), SpeciesId(374)), // -> HUNTAIL
            e(EvoMethod::TradeItem(ItemId(193)), SpeciesId(375)), // -> GOREBYSS
        ],
    ),
    // SHUPPET -> BANETTE
    (SpeciesId(377), &[e(EvoMethod::Level(37), SpeciesId(378))]),
    // ARON -> LAIRON
    (SpeciesId(382), &[e(EvoMethod::Level(32), SpeciesId(383))]),
    // LAIRON -> AGGRON
    (SpeciesId(383), &[e(EvoMethod::Level(42), SpeciesId(384))]),
    // LILEEP -> CRADILY
    (SpeciesId(388), &[e(EvoMethod::Level(40), SpeciesId(389))]),
    // ANORITH -> ARMALDO
    (SpeciesId(390), &[e(EvoMethod::Level(40), SpeciesId(391))]),
    // RALTS -> KIRLIA
    (SpeciesId(392), &[e(EvoMethod::Level(20), SpeciesId(393))]),
    // KIRLIA -> GARDEVOIR
    (SpeciesId(393), &[e(EvoMethod::Level(30), SpeciesId(394))]),
    // BAGON -> SHELGON
    (SpeciesId(395), &[e(EvoMethod::Level(30), SpeciesId(396))]),
    // SHELGON -> SALAMENCE
    (SpeciesId(396), &[e(EvoMethod::Level(50), SpeciesId(397))]),
    // BELDUM -> METANG
    (SpeciesId(398), &[e(EvoMethod::Level(20), SpeciesId(399))]),
    // METANG -> METAGROSS
    (SpeciesId(399), &[e(EvoMethod::Level(45), SpeciesId(400))]),
];

/// The evolution table: an owned lookup from a source [`SpeciesId`] to its
/// evolution entries `(oop-boundaries)`.
#[derive(Debug, Clone, Copy)]
pub struct EvolutionTable {
    entries: &'static [(SpeciesId, &'static [Evolution])],
}

impl EvolutionTable {
    /// The number of species that have at least one evolution.
    pub const EVOLVING_SPECIES: usize = ENTRIES.len();

    /// Build the table over the extracted upstream data.
    #[must_use]
    pub const fn new() -> Self {
        Self { entries: ENTRIES }
    }

    /// The evolution entries for `species`, or `None` if it does not evolve.
    ///
    /// The upstream data has no species with an empty-but-present entry, so a
    /// `Some(&[])` is never returned: a hit always carries at least one
    /// [`Evolution`].
    #[must_use]
    pub fn get(&self, species: SpeciesId) -> Option<&'static [Evolution]> {
        self.entries
            .binary_search_by_key(&species, |&(id, _)| id)
            .ok()
            .map(|i| self.entries[i].1)
    }

    /// The evolution entries for `species`.
    ///
    /// Unlike [`get`](Self::get), a species that simply does not evolve is not
    /// an error — it yields an empty slice. An error is reserved for a
    /// [`SpeciesId`] outside the extracted range.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownSpecies`] if `species` is outside the
    /// extracted `SpeciesId` range (`0..`[`SpeciesTable::LEN`]).
    ///
    /// [`SpeciesTable::LEN`]: crate::species::SpeciesTable::LEN
    pub fn evolutions(&self, species: SpeciesId) -> Result<&'static [Evolution], AssetError> {
        if species.0 as usize >= crate::species::SpeciesTable::LEN {
            return Err(AssetError::UnknownSpecies(species.0));
        }
        Ok(self.get(species).unwrap_or(&[]))
    }

    /// The total number of evolution entries across every species (branched
    /// evolvers contribute one per branch).
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

    // National-dex ids used by the tie tests.
    const EEVEE: SpeciesId = SpeciesId(133);
    const GLOOM: SpeciesId = SpeciesId(44);
    const PIKACHU: SpeciesId = SpeciesId(25);
    const RAICHU: SpeciesId = SpeciesId(26);
    const SEADRA: SpeciesId = SpeciesId(117);
    const KINGDRA: SpeciesId = SpeciesId(230);
    const GOLBAT: SpeciesId = SpeciesId(42);
    const CROBAT: SpeciesId = SpeciesId(169);
    const BULBASAUR: SpeciesId = SpeciesId(1);
    const IVYSAUR: SpeciesId = SpeciesId(2);

    // Item ids referenced by the tie tests (constants/items.h).
    const ITEM_THUNDER_STONE: ItemId = ItemId(96);
    const ITEM_DRAGON_SCALE: ItemId = ItemId(201);
    const ITEM_WATER_STONE: ItemId = ItemId(97);
    const ITEM_FIRE_STONE: ItemId = ItemId(95);
    const ITEM_SUN_STONE: ItemId = ItemId(93);
    const ITEM_LEAF_STONE: ItemId = ItemId(98);

    #[test]
    fn level_evolution_pins_bulbasaur() {
        // [SPECIES_BULBASAUR] = {{EVO_LEVEL, 16, SPECIES_IVYSAUR}}
        let table = EvolutionTable::new();
        let evs = table.get(BULBASAUR).unwrap();
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].method, EvoMethod::Level(16));
        assert_eq!(evs[0].target, IVYSAUR);
    }

    #[test]
    fn used_item_evolution_pins_pikachu() {
        // [SPECIES_PIKACHU] = {{EVO_ITEM, ITEM_THUNDER_STONE, SPECIES_RAICHU}}
        let table = EvolutionTable::new();
        let evs = table.get(PIKACHU).unwrap();
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].method, EvoMethod::Item(ITEM_THUNDER_STONE));
        assert_eq!(evs[0].target, RAICHU);
    }

    #[test]
    fn trade_item_evolution_pins_seadra() {
        // [SPECIES_SEADRA] = {{EVO_TRADE_ITEM, ITEM_DRAGON_SCALE, SPECIES_KINGDRA}}
        let table = EvolutionTable::new();
        let evs = table.get(SEADRA).unwrap();
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].method, EvoMethod::TradeItem(ITEM_DRAGON_SCALE));
        assert_eq!(evs[0].target, KINGDRA);
    }

    #[test]
    fn friendship_evolution_pins_golbat() {
        // [SPECIES_GOLBAT] = {{EVO_FRIENDSHIP, 0, SPECIES_CROBAT}}
        let table = EvolutionTable::new();
        let evs = table.get(GOLBAT).unwrap();
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].method, EvoMethod::Friendship);
        assert_eq!(evs[0].method.param(), 0);
        assert_eq!(evs[0].target, CROBAT);
    }

    #[test]
    fn two_way_branch_pins_gloom() {
        // [SPECIES_GLOOM] = {{EVO_ITEM, ITEM_LEAF_STONE, SPECIES_VILEPLUME},
        //                    {EVO_ITEM, ITEM_SUN_STONE, SPECIES_BELLOSSOM}}
        let table = EvolutionTable::new();
        let evs = table.get(GLOOM).unwrap();
        assert_eq!(evs.len(), 2);
        assert_eq!(evs[0].method, EvoMethod::Item(ITEM_LEAF_STONE));
        assert_eq!(evs[0].target, SpeciesId(45)); // VILEPLUME
        assert_eq!(evs[1].method, EvoMethod::Item(ITEM_SUN_STONE));
        assert_eq!(evs[1].target, SpeciesId(182)); // BELLOSSOM
    }

    #[test]
    fn five_way_branch_pins_eevee() {
        // Eevee is the widest evolver: five branches, in upstream order.
        let table = EvolutionTable::new();
        let evs = table.get(EEVEE).unwrap();
        assert_eq!(evs.len(), 5);
        assert_eq!(evs[0].method, EvoMethod::Item(ITEM_THUNDER_STONE));
        assert_eq!(evs[0].target, SpeciesId(135)); // JOLTEON
        assert_eq!(evs[1].method, EvoMethod::Item(ITEM_WATER_STONE));
        assert_eq!(evs[1].target, SpeciesId(134)); // VAPOREON
        assert_eq!(evs[2].method, EvoMethod::Item(ITEM_FIRE_STONE));
        assert_eq!(evs[2].target, SpeciesId(136)); // FLAREON
        assert_eq!(evs[3].method, EvoMethod::FriendshipDay);
        assert_eq!(evs[3].target, SpeciesId(196)); // ESPEON
        assert_eq!(evs[4].method, EvoMethod::FriendshipNight);
        assert_eq!(evs[4].target, SpeciesId(197)); // UMBREON
    }

    #[test]
    fn method_id_and_param_round_trip() {
        // Anchor each variant's raw method id to its `EVO_*` constant, and
        // check the overloaded param is recovered in each shape.
        assert_eq!(EvoMethod::Friendship.method_id(), 1);
        assert_eq!(EvoMethod::FriendshipDay.method_id(), 2);
        assert_eq!(EvoMethod::FriendshipNight.method_id(), 3);
        assert_eq!(EvoMethod::Level(16).method_id(), 4);
        assert_eq!(EvoMethod::Trade.method_id(), 5);
        assert_eq!(EvoMethod::TradeItem(ItemId(199)).method_id(), 6);
        assert_eq!(EvoMethod::Item(ItemId(96)).method_id(), 7);
        assert_eq!(EvoMethod::LevelAtkGtDef(20).method_id(), 8);
        assert_eq!(EvoMethod::LevelAtkEqDef(20).method_id(), 9);
        assert_eq!(EvoMethod::LevelAtkLtDef(20).method_id(), 10);
        assert_eq!(EvoMethod::LevelSilcoon(7).method_id(), 11);
        assert_eq!(EvoMethod::LevelCascoon(7).method_id(), 12);
        assert_eq!(EvoMethod::LevelNinjask(20).method_id(), 13);
        assert_eq!(EvoMethod::LevelShedinja(20).method_id(), 14);
        assert_eq!(EvoMethod::Beauty(170).method_id(), 15);

        assert_eq!(EvoMethod::Level(16).param(), 16);
        assert_eq!(EvoMethod::Item(ItemId(96)).param(), 96);
        assert_eq!(EvoMethod::TradeItem(ItemId(199)).param(), 199);
        assert_eq!(EvoMethod::Beauty(170).param(), 170);
        assert_eq!(EvoMethod::Trade.param(), 0);
    }

    #[test]
    fn counts_match_upstream() {
        // 172 species evolve; 184 evolution entries in total (branches counted
        // individually) — the exact totals of `gEvolutionTable`.
        let table = EvolutionTable::new();
        assert_eq!(EvolutionTable::EVOLVING_SPECIES, 172);
        assert_eq!(table.total_evolutions(), 184);
    }

    #[test]
    fn entries_are_sorted_and_unique_by_source() {
        // The binary search in `get` relies on ascending, duplicate-free keys.
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
        // Filler `{0,0,0}` slots are dropped, so no present list is empty.
        for &(src, evs) in super::ENTRIES {
            assert!(!evs.is_empty(), "empty entry list for {src:?}");
        }
    }

    #[test]
    fn non_evolving_species_yields_empty_not_error() {
        // Charizard (id 6) is fully evolved: no entry, but a valid species.
        let table = EvolutionTable::new();
        assert_eq!(table.get(SpeciesId(6)), None);
        assert_eq!(table.evolutions(SpeciesId(6)), Ok(&[][..]));
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
        // No evolution points outside the extracted species range.
        let len = crate::species::SpeciesTable::LEN_U16;
        for &(_, evs) in super::ENTRIES {
            for ev in evs {
                assert!(ev.target.0 < len, "target {:?} out of range", ev.target);
            }
        }
    }
}
