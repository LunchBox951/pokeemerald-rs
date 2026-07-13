//! Egg-move lists (S-4): the `gEggMoves` table.
//!
//! Ports the breeding egg-move data from the upstream reference
//! `pokeemerald/src/data/pokemon/egg_moves.h` (`const u16 gEggMoves[]`). That
//! file is not a struct array but a single *flat* `u16` stream produced by the
//! `egg_moves(species, moves...)` macro: each species group begins with a
//! sentinel word `SPECIES_* + EGG_MOVES_SPECIES_OFFSET` (`20000`), followed by
//! that species' egg-move ids, and the group runs until the next sentinel (a
//! word `>= EGG_MOVES_SPECIES_OFFSET`) or the array-terminating
//! `EGG_MOVES_TERMINATOR` (`0xFFFF`).
//!
//! Rather than reproduce that flat sentinel encoding `(no-verbatim)`, the stream
//! is parsed once, at transcription time, into an owned [`EggMoveTable`] of
//! per-species [`EggMoveList`]s — each a [`SpeciesId`] paired with its list of
//! egg [`MoveId`]s `(oop-boundaries)`. The `EGG_MOVES_SPECIES_OFFSET` /
//! `EGG_MOVES_TERMINATOR` constants are preserved as named items so the original
//! encoding stays documented and testable `(behavioral-fidelity)`.
//!
//! The upstream-tie test at the bottom pins several species' full egg-move lists
//! (including the two single-move species and the last group) back to their
//! exact `egg_moves.h` values, and structural tests re-derive the flat-stream
//! word count and guard against duplicate species.

use crate::battle_moves::MoveId;
use crate::error::AssetError;
use crate::species::SpeciesId;

/// The sentinel offset added to a `SPECIES_*` id to mark the start of a species'
/// egg-move group in the flat upstream `gEggMoves` stream
/// (`EGG_MOVES_SPECIES_OFFSET`). A flat word `>= EGG_MOVES_SPECIES_OFFSET` that
/// is not the terminator introduces a new group; subtracting the offset recovers
/// the species id.
pub const EGG_MOVES_SPECIES_OFFSET: u16 = 20000;

/// The word that terminates the flat upstream `gEggMoves` stream
/// (`EGG_MOVES_TERMINATOR`, `0xFFFF`).
pub const EGG_MOVES_TERMINATOR: u16 = 0xFFFF;

/// The number of species that have an egg-move group in `gEggMoves`.
///
/// Anchors the table length: exactly one [`EggMoveList`] per group in the
/// upstream flat stream.
pub const EGG_MOVE_SPECIES_COUNT: usize = 165;

/// One species' egg moves — the owned Rust form of a single `egg_moves(...)`
/// group. The `moves` slice is borrowed from embedded static data (so the table
/// allocates nothing) and preserves the exact upstream order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EggMoveList {
    /// The species this egg-move group belongs to.
    pub species: SpeciesId,
    /// The species' egg moves, in upstream order.
    pub moves: &'static [MoveId],
}

impl EggMoveList {
    /// The species this group belongs to.
    #[must_use]
    pub const fn species(&self) -> SpeciesId {
        self.species
    }

    /// The species' egg moves, in upstream order.
    #[must_use]
    pub const fn moves(&self) -> &'static [MoveId] {
        self.moves
    }

    /// The number of egg moves this species has.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.moves.len()
    }

    /// Whether this species has no egg moves. Never true in the upstream data
    /// (every group has at least one move), but provided to satisfy the
    /// `len`/`is_empty` convention.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.moves.is_empty()
    }

    /// Whether this species can learn `mv` as an egg move.
    #[must_use]
    pub fn teaches(&self, mv: MoveId) -> bool {
        self.moves.contains(&mv)
    }
}

/// Construct one [`EggMoveList`] row. A terse free helper so the generated table
/// stays readable.
const fn g(species: SpeciesId, moves: &'static [MoveId]) -> EggMoveList {
    EggMoveList { species, moves }
}

/// The transcribed `gEggMoves` table, one [`EggMoveList`] per species group in
/// the upstream flat stream, in the same (ascending species-id) order. Each row
/// is a faithful translation of one `egg_moves(...)` group from `egg_moves.h`.
const EGG_MOVES: [EggMoveList; EGG_MOVE_SPECIES_COUNT] = [
    // SPECIES_BULBASAUR
    g(
        SpeciesId(1),
        &[
            MoveId(113),
            MoveId(130),
            MoveId(219),
            MoveId(204),
            MoveId(80),
            MoveId(345),
            MoveId(320),
            MoveId(174),
        ],
    ),
    // SPECIES_CHARMANDER
    g(
        SpeciesId(4),
        &[
            MoveId(187),
            MoveId(246),
            MoveId(157),
            MoveId(44),
            MoveId(200),
            MoveId(251),
            MoveId(14),
            MoveId(349),
        ],
    ),
    // SPECIES_SQUIRTLE
    g(
        SpeciesId(7),
        &[
            MoveId(243),
            MoveId(114),
            MoveId(54),
            MoveId(193),
            MoveId(175),
            MoveId(287),
            MoveId(300),
            MoveId(281),
        ],
    ),
    // SPECIES_PIDGEY
    g(
        SpeciesId(16),
        &[
            MoveId(228),
            MoveId(185),
            MoveId(193),
            MoveId(211),
            MoveId(314),
        ],
    ),
    // SPECIES_RATTATA
    g(
        SpeciesId(19),
        &[
            MoveId(103),
            MoveId(172),
            MoveId(154),
            MoveId(44),
            MoveId(68),
            MoveId(179),
            MoveId(253),
            MoveId(207),
        ],
    ),
    // SPECIES_SPEAROW
    g(
        SpeciesId(21),
        &[
            MoveId(185),
            MoveId(206),
            MoveId(184),
            MoveId(98),
            MoveId(161),
            MoveId(310),
            MoveId(143),
        ],
    ),
    // SPECIES_EKANS
    g(
        SpeciesId(23),
        &[
            MoveId(228),
            MoveId(21),
            MoveId(180),
            MoveId(251),
            MoveId(305),
        ],
    ),
    // SPECIES_SANDSHREW
    g(
        SpeciesId(27),
        &[
            MoveId(175),
            MoveId(219),
            MoveId(68),
            MoveId(229),
            MoveId(157),
            MoveId(232),
            MoveId(14),
            MoveId(306),
        ],
    ),
    // SPECIES_NIDORAN_F
    g(
        SpeciesId(29),
        &[
            MoveId(48),
            MoveId(50),
            MoveId(36),
            MoveId(116),
            MoveId(204),
            MoveId(68),
            MoveId(251),
        ],
    ),
    // SPECIES_NIDORAN_M
    g(
        SpeciesId(32),
        &[
            MoveId(68),
            MoveId(50),
            MoveId(48),
            MoveId(36),
            MoveId(133),
            MoveId(93),
            MoveId(251),
        ],
    ),
    // SPECIES_VULPIX
    g(
        SpeciesId(37),
        &[
            MoveId(185),
            MoveId(95),
            MoveId(175),
            MoveId(180),
            MoveId(50),
            MoveId(336),
            MoveId(244),
            MoveId(257),
        ],
    ),
    // SPECIES_ZUBAT
    g(
        SpeciesId(41),
        &[
            MoveId(98),
            MoveId(228),
            MoveId(185),
            MoveId(16),
            MoveId(18),
            MoveId(174),
        ],
    ),
    // SPECIES_ODDISH
    g(
        SpeciesId(43),
        &[
            MoveId(14),
            MoveId(75),
            MoveId(175),
            MoveId(235),
            MoveId(204),
            MoveId(275),
        ],
    ),
    // SPECIES_PARAS
    g(
        SpeciesId(46),
        &[
            MoveId(206),
            MoveId(103),
            MoveId(68),
            MoveId(60),
            MoveId(175),
            MoveId(230),
            MoveId(113),
            MoveId(228),
        ],
    ),
    // SPECIES_VENONAT
    g(
        SpeciesId(48),
        &[MoveId(226), MoveId(103), MoveId(202), MoveId(324)],
    ),
    // SPECIES_DIGLETT
    g(
        SpeciesId(50),
        &[
            MoveId(185),
            MoveId(103),
            MoveId(246),
            MoveId(228),
            MoveId(251),
            MoveId(253),
            MoveId(157),
        ],
    ),
    // SPECIES_MEOWTH
    g(
        SpeciesId(52),
        &[
            MoveId(180),
            MoveId(204),
            MoveId(95),
            MoveId(133),
            MoveId(244),
            MoveId(274),
        ],
    ),
    // SPECIES_PSYDUCK
    g(
        SpeciesId(54),
        &[
            MoveId(95),
            MoveId(60),
            MoveId(193),
            MoveId(113),
            MoveId(248),
            MoveId(94),
            MoveId(238),
            MoveId(287),
        ],
    ),
    // SPECIES_MANKEY
    g(
        SpeciesId(56),
        &[
            MoveId(157),
            MoveId(193),
            MoveId(96),
            MoveId(68),
            MoveId(179),
            MoveId(251),
            MoveId(279),
            MoveId(265),
        ],
    ),
    // SPECIES_GROWLITHE
    g(
        SpeciesId(58),
        &[
            MoveId(34),
            MoveId(219),
            MoveId(242),
            MoveId(37),
            MoveId(83),
            MoveId(336),
            MoveId(257),
        ],
    ),
    // SPECIES_POLIWAG
    g(
        SpeciesId(60),
        &[
            MoveId(54),
            MoveId(150),
            MoveId(61),
            MoveId(114),
            MoveId(170),
            MoveId(346),
            MoveId(301),
        ],
    ),
    // SPECIES_ABRA
    g(
        SpeciesId(63),
        &[
            MoveId(227),
            MoveId(112),
            MoveId(282),
            MoveId(7),
            MoveId(9),
            MoveId(8),
        ],
    ),
    // SPECIES_MACHOP
    g(
        SpeciesId(66),
        &[
            MoveId(113),
            MoveId(96),
            MoveId(27),
            MoveId(227),
            MoveId(265),
            MoveId(68),
            MoveId(157),
        ],
    ),
    // SPECIES_BELLSPROUT
    g(
        SpeciesId(69),
        &[
            MoveId(14),
            MoveId(227),
            MoveId(115),
            MoveId(235),
            MoveId(141),
            MoveId(275),
            MoveId(345),
        ],
    ),
    // SPECIES_TENTACOOL
    g(
        SpeciesId(72),
        &[
            MoveId(62),
            MoveId(243),
            MoveId(229),
            MoveId(114),
            MoveId(219),
            MoveId(109),
        ],
    ),
    // SPECIES_GEODUDE
    g(SpeciesId(74), &[MoveId(5), MoveId(157), MoveId(335)]),
    // SPECIES_PONYTA
    g(
        SpeciesId(77),
        &[
            MoveId(172),
            MoveId(37),
            MoveId(24),
            MoveId(95),
            MoveId(204),
            MoveId(38),
        ],
    ),
    // SPECIES_SLOWPOKE
    g(
        SpeciesId(79),
        &[
            MoveId(219),
            MoveId(187),
            MoveId(248),
            MoveId(23),
            MoveId(300),
            MoveId(214),
            MoveId(173),
        ],
    ),
    // SPECIES_FARFETCHD
    g(
        SpeciesId(83),
        &[
            MoveId(211),
            MoveId(193),
            MoveId(119),
            MoveId(16),
            MoveId(98),
            MoveId(175),
            MoveId(297),
            MoveId(174),
        ],
    ),
    // SPECIES_DODUO
    g(
        SpeciesId(84),
        &[
            MoveId(98),
            MoveId(48),
            MoveId(114),
            MoveId(185),
            MoveId(175),
            MoveId(283),
        ],
    ),
    // SPECIES_SEEL
    g(
        SpeciesId(86),
        &[
            MoveId(122),
            MoveId(195),
            MoveId(50),
            MoveId(32),
            MoveId(21),
            MoveId(227),
            MoveId(252),
            MoveId(333),
        ],
    ),
    // SPECIES_GRIMER
    g(
        SpeciesId(88),
        &[
            MoveId(114),
            MoveId(212),
            MoveId(122),
            MoveId(286),
            MoveId(174),
            MoveId(325),
            MoveId(153),
        ],
    ),
    // SPECIES_SHELLDER
    g(
        SpeciesId(90),
        &[
            MoveId(61),
            MoveId(36),
            MoveId(112),
            MoveId(229),
            MoveId(103),
            MoveId(333),
        ],
    ),
    // SPECIES_GASTLY
    g(
        SpeciesId(92),
        &[
            MoveId(149),
            MoveId(195),
            MoveId(114),
            MoveId(310),
            MoveId(261),
            MoveId(288),
            MoveId(153),
        ],
    ),
    // SPECIES_ONIX
    g(
        SpeciesId(95),
        &[MoveId(157), MoveId(175), MoveId(153), MoveId(335)],
    ),
    // SPECIES_DROWZEE
    g(
        SpeciesId(96),
        &[
            MoveId(112),
            MoveId(274),
            MoveId(272),
            MoveId(7),
            MoveId(9),
            MoveId(8),
        ],
    ),
    // SPECIES_KRABBY
    g(
        SpeciesId(98),
        &[
            MoveId(91),
            MoveId(114),
            MoveId(133),
            MoveId(175),
            MoveId(21),
            MoveId(282),
            MoveId(14),
        ],
    ),
    // SPECIES_EXEGGCUTE
    g(
        SpeciesId(102),
        &[
            MoveId(235),
            MoveId(236),
            MoveId(115),
            MoveId(246),
            MoveId(244),
            MoveId(275),
            MoveId(174),
        ],
    ),
    // SPECIES_CUBONE
    g(
        SpeciesId(104),
        &[
            MoveId(157),
            MoveId(246),
            MoveId(187),
            MoveId(103),
            MoveId(130),
            MoveId(195),
            MoveId(14),
        ],
    ),
    // SPECIES_LICKITUNG
    g(
        SpeciesId(108),
        &[
            MoveId(187),
            MoveId(222),
            MoveId(34),
            MoveId(174),
            MoveId(265),
            MoveId(214),
            MoveId(173),
            MoveId(164),
        ],
    ),
    // SPECIES_KOFFING
    g(
        SpeciesId(109),
        &[
            MoveId(103),
            MoveId(149),
            MoveId(60),
            MoveId(194),
            MoveId(220),
            MoveId(261),
        ],
    ),
    // SPECIES_RHYHORN
    g(
        SpeciesId(111),
        &[
            MoveId(242),
            MoveId(179),
            MoveId(157),
            MoveId(68),
            MoveId(222),
            MoveId(14),
            MoveId(174),
            MoveId(306),
        ],
    ),
    // SPECIES_CHANSEY
    g(
        SpeciesId(113),
        &[
            MoveId(217),
            MoveId(118),
            MoveId(215),
            MoveId(312),
            MoveId(164),
        ],
    ),
    // SPECIES_TANGELA
    g(
        SpeciesId(114),
        &[
            MoveId(175),
            MoveId(93),
            MoveId(72),
            MoveId(115),
            MoveId(133),
            MoveId(73),
            MoveId(267),
        ],
    ),
    // SPECIES_KANGASKHAN
    g(
        SpeciesId(115),
        &[
            MoveId(23),
            MoveId(193),
            MoveId(116),
            MoveId(219),
            MoveId(50),
            MoveId(68),
            MoveId(306),
            MoveId(164),
        ],
    ),
    // SPECIES_HORSEA
    g(
        SpeciesId(116),
        &[
            MoveId(175),
            MoveId(62),
            MoveId(190),
            MoveId(50),
            MoveId(150),
            MoveId(82),
            MoveId(225),
        ],
    ),
    // SPECIES_GOLDEEN
    g(
        SpeciesId(118),
        &[
            MoveId(60),
            MoveId(114),
            MoveId(56),
            MoveId(214),
            MoveId(300),
        ],
    ),
    // SPECIES_MR_MIME
    g(
        SpeciesId(122),
        &[
            MoveId(248),
            MoveId(95),
            MoveId(102),
            MoveId(244),
            MoveId(252),
            MoveId(271),
        ],
    ),
    // SPECIES_SCYTHER
    g(
        SpeciesId(123),
        &[
            MoveId(68),
            MoveId(219),
            MoveId(226),
            MoveId(13),
            MoveId(179),
            MoveId(113),
            MoveId(203),
            MoveId(318),
        ],
    ),
    // SPECIES_PINSIR
    g(
        SpeciesId(127),
        &[MoveId(31), MoveId(175), MoveId(206), MoveId(185)],
    ),
    // SPECIES_LAPRAS
    g(
        SpeciesId(131),
        &[
            MoveId(193),
            MoveId(164),
            MoveId(321),
            MoveId(287),
            MoveId(349),
            MoveId(174),
            MoveId(214),
            MoveId(32),
        ],
    ),
    // SPECIES_EEVEE
    g(
        SpeciesId(133),
        &[
            MoveId(204),
            MoveId(175),
            MoveId(203),
            MoveId(174),
            MoveId(321),
            MoveId(273),
        ],
    ),
    // SPECIES_OMANYTE
    g(
        SpeciesId(138),
        &[
            MoveId(61),
            MoveId(62),
            MoveId(21),
            MoveId(48),
            MoveId(114),
            MoveId(157),
            MoveId(191),
        ],
    ),
    // SPECIES_KABUTO
    g(
        SpeciesId(140),
        &[
            MoveId(61),
            MoveId(62),
            MoveId(229),
            MoveId(91),
            MoveId(175),
            MoveId(282),
            MoveId(109),
        ],
    ),
    // SPECIES_AERODACTYL
    g(
        SpeciesId(142),
        &[
            MoveId(18),
            MoveId(228),
            MoveId(193),
            MoveId(211),
            MoveId(225),
            MoveId(174),
        ],
    ),
    // SPECIES_SNORLAX
    g(
        SpeciesId(143),
        &[
            MoveId(122),
            MoveId(204),
            MoveId(38),
            MoveId(174),
            MoveId(90),
            MoveId(164),
        ],
    ),
    // SPECIES_DRATINI
    g(
        SpeciesId(147),
        &[
            MoveId(113),
            MoveId(54),
            MoveId(114),
            MoveId(48),
            MoveId(225),
            MoveId(349),
        ],
    ),
    // SPECIES_CHIKORITA
    g(
        SpeciesId(152),
        &[
            MoveId(22),
            MoveId(73),
            MoveId(68),
            MoveId(246),
            MoveId(175),
            MoveId(267),
            MoveId(275),
            MoveId(320),
        ],
    ),
    // SPECIES_CYNDAQUIL
    g(
        SpeciesId(155),
        &[
            MoveId(154),
            MoveId(98),
            MoveId(179),
            MoveId(37),
            MoveId(193),
            MoveId(343),
            MoveId(336),
            MoveId(306),
        ],
    ),
    // SPECIES_TOTODILE
    g(
        SpeciesId(158),
        &[
            MoveId(242),
            MoveId(37),
            MoveId(56),
            MoveId(246),
            MoveId(157),
            MoveId(300),
            MoveId(346),
            MoveId(337),
        ],
    ),
    // SPECIES_SENTRET
    g(
        SpeciesId(161),
        &[
            MoveId(38),
            MoveId(228),
            MoveId(163),
            MoveId(116),
            MoveId(179),
            MoveId(164),
            MoveId(271),
            MoveId(274),
        ],
    ),
    // SPECIES_HOOTHOOT
    g(
        SpeciesId(163),
        &[
            MoveId(119),
            MoveId(48),
            MoveId(185),
            MoveId(17),
            MoveId(18),
            MoveId(143),
            MoveId(297),
        ],
    ),
    // SPECIES_LEDYBA
    g(SpeciesId(165), &[MoveId(60), MoveId(117), MoveId(318)]),
    // SPECIES_SPINARAK
    g(
        SpeciesId(167),
        &[
            MoveId(60),
            MoveId(50),
            MoveId(49),
            MoveId(226),
            MoveId(228),
            MoveId(324),
        ],
    ),
    // SPECIES_CHINCHOU
    g(SpeciesId(170), &[MoveId(175), MoveId(103), MoveId(133)]),
    // SPECIES_PICHU
    g(
        SpeciesId(172),
        &[
            MoveId(179),
            MoveId(117),
            MoveId(217),
            MoveId(227),
            MoveId(3),
            MoveId(273),
            MoveId(268),
        ],
    ),
    // SPECIES_CLEFFA
    g(
        SpeciesId(173),
        &[
            MoveId(217),
            MoveId(118),
            MoveId(133),
            MoveId(187),
            MoveId(150),
            MoveId(102),
            MoveId(273),
            MoveId(164),
        ],
    ),
    // SPECIES_IGGLYBUFF
    g(
        SpeciesId(174),
        &[
            MoveId(195),
            MoveId(217),
            MoveId(185),
            MoveId(273),
            MoveId(313),
        ],
    ),
    // SPECIES_TOGEPI
    g(
        SpeciesId(175),
        &[
            MoveId(217),
            MoveId(119),
            MoveId(64),
            MoveId(193),
            MoveId(248),
            MoveId(164),
            MoveId(244),
        ],
    ),
    // SPECIES_NATU
    g(
        SpeciesId(177),
        &[
            MoveId(114),
            MoveId(65),
            MoveId(98),
            MoveId(185),
            MoveId(211),
            MoveId(244),
            MoveId(297),
            MoveId(287),
        ],
    ),
    // SPECIES_MAREEP
    g(
        SpeciesId(179),
        &[
            MoveId(36),
            MoveId(34),
            MoveId(219),
            MoveId(103),
            MoveId(115),
            MoveId(316),
            MoveId(268),
        ],
    ),
    // SPECIES_MARILL
    g(
        SpeciesId(183),
        &[
            MoveId(113),
            MoveId(217),
            MoveId(133),
            MoveId(248),
            MoveId(187),
            MoveId(195),
            MoveId(48),
            MoveId(164),
        ],
    ),
    // SPECIES_SUDOWOODO
    g(SpeciesId(185), &[MoveId(120)]),
    // SPECIES_HOPPIP
    g(
        SpeciesId(187),
        &[
            MoveId(93),
            MoveId(227),
            MoveId(38),
            MoveId(115),
            MoveId(133),
            MoveId(270),
            MoveId(244),
        ],
    ),
    // SPECIES_AIPOM
    g(
        SpeciesId(190),
        &[
            MoveId(68),
            MoveId(103),
            MoveId(228),
            MoveId(97),
            MoveId(180),
            MoveId(21),
            MoveId(3),
            MoveId(251),
        ],
    ),
    // SPECIES_SUNKERN
    g(
        SpeciesId(191),
        &[
            MoveId(320),
            MoveId(227),
            MoveId(73),
            MoveId(267),
            MoveId(174),
            MoveId(270),
        ],
    ),
    // SPECIES_YANMA
    g(
        SpeciesId(193),
        &[
            MoveId(18),
            MoveId(179),
            MoveId(141),
            MoveId(324),
            MoveId(318),
        ],
    ),
    // SPECIES_WOOPER
    g(
        SpeciesId(194),
        &[
            MoveId(34),
            MoveId(246),
            MoveId(219),
            MoveId(174),
            MoveId(300),
            MoveId(254),
            MoveId(256),
            MoveId(255),
        ],
    ),
    // SPECIES_MURKROW
    g(
        SpeciesId(198),
        &[
            MoveId(18),
            MoveId(65),
            MoveId(119),
            MoveId(17),
            MoveId(143),
            MoveId(109),
            MoveId(297),
            MoveId(195),
        ],
    ),
    // SPECIES_MISDREAVUS
    g(
        SpeciesId(200),
        &[MoveId(103), MoveId(194), MoveId(244), MoveId(286)],
    ),
    // SPECIES_GIRAFARIG
    g(
        SpeciesId(203),
        &[
            MoveId(36),
            MoveId(133),
            MoveId(193),
            MoveId(248),
            MoveId(251),
            MoveId(244),
            MoveId(273),
            MoveId(277),
        ],
    ),
    // SPECIES_PINECO
    g(
        SpeciesId(204),
        &[
            MoveId(115),
            MoveId(42),
            MoveId(175),
            MoveId(129),
            MoveId(68),
            MoveId(328),
        ],
    ),
    // SPECIES_DUNSPARCE
    g(
        SpeciesId(206),
        &[
            MoveId(117),
            MoveId(246),
            MoveId(157),
            MoveId(44),
            MoveId(29),
            MoveId(310),
            MoveId(174),
        ],
    ),
    // SPECIES_GLIGAR
    g(
        SpeciesId(207),
        &[MoveId(232), MoveId(17), MoveId(13), MoveId(68), MoveId(328)],
    ),
    // SPECIES_SNUBBULL
    g(
        SpeciesId(209),
        &[
            MoveId(118),
            MoveId(185),
            MoveId(115),
            MoveId(217),
            MoveId(242),
            MoveId(215),
            MoveId(173),
            MoveId(265),
        ],
    ),
    // SPECIES_QWILFISH
    g(
        SpeciesId(211),
        &[
            MoveId(175),
            MoveId(114),
            MoveId(61),
            MoveId(48),
            MoveId(310),
        ],
    ),
    // SPECIES_SHUCKLE
    g(SpeciesId(213), &[MoveId(230)]),
    // SPECIES_HERACROSS
    g(
        SpeciesId(214),
        &[MoveId(106), MoveId(117), MoveId(175), MoveId(206)],
    ),
    // SPECIES_SNEASEL
    g(
        SpeciesId(215),
        &[
            MoveId(68),
            MoveId(180),
            MoveId(193),
            MoveId(115),
            MoveId(44),
            MoveId(306),
            MoveId(252),
        ],
    ),
    // SPECIES_TEDDIURSA
    g(
        SpeciesId(216),
        &[
            MoveId(242),
            MoveId(36),
            MoveId(69),
            MoveId(68),
            MoveId(232),
            MoveId(313),
            MoveId(281),
            MoveId(214),
        ],
    ),
    // SPECIES_SLUGMA
    g(SpeciesId(218), &[MoveId(151), MoveId(257)]),
    // SPECIES_SWINUB
    g(
        SpeciesId(220),
        &[
            MoveId(36),
            MoveId(44),
            MoveId(34),
            MoveId(157),
            MoveId(246),
            MoveId(341),
            MoveId(333),
            MoveId(38),
        ],
    ),
    // SPECIES_CORSOLA
    g(
        SpeciesId(222),
        &[
            MoveId(157),
            MoveId(103),
            MoveId(54),
            MoveId(133),
            MoveId(112),
            MoveId(275),
            MoveId(109),
            MoveId(333),
        ],
    ),
    // SPECIES_REMORAID
    g(
        SpeciesId(223),
        &[
            MoveId(62),
            MoveId(190),
            MoveId(48),
            MoveId(114),
            MoveId(103),
            MoveId(86),
            MoveId(350),
        ],
    ),
    // SPECIES_DELIBIRD
    g(
        SpeciesId(225),
        &[
            MoveId(62),
            MoveId(98),
            MoveId(248),
            MoveId(150),
            MoveId(229),
            MoveId(301),
        ],
    ),
    // SPECIES_MANTINE
    g(
        SpeciesId(226),
        &[
            MoveId(239),
            MoveId(56),
            MoveId(114),
            MoveId(21),
            MoveId(300),
            MoveId(157),
        ],
    ),
    // SPECIES_SKARMORY
    g(
        SpeciesId(227),
        &[
            MoveId(65),
            MoveId(228),
            MoveId(18),
            MoveId(143),
            MoveId(174),
        ],
    ),
    // SPECIES_HOUNDOUR
    g(
        SpeciesId(228),
        &[
            MoveId(83),
            MoveId(99),
            MoveId(228),
            MoveId(68),
            MoveId(180),
            MoveId(179),
            MoveId(251),
            MoveId(261),
        ],
    ),
    // SPECIES_PHANPY
    g(
        SpeciesId(231),
        &[
            MoveId(116),
            MoveId(34),
            MoveId(246),
            MoveId(173),
            MoveId(68),
            MoveId(90),
        ],
    ),
    // SPECIES_STANTLER
    g(
        SpeciesId(234),
        &[
            MoveId(180),
            MoveId(50),
            MoveId(44),
            MoveId(207),
            MoveId(244),
            MoveId(326),
        ],
    ),
    // SPECIES_TYROGUE
    g(
        SpeciesId(236),
        &[
            MoveId(229),
            MoveId(136),
            MoveId(183),
            MoveId(170),
            MoveId(270),
        ],
    ),
    // SPECIES_SMOOCHUM
    g(
        SpeciesId(238),
        &[MoveId(96), MoveId(244), MoveId(252), MoveId(273), MoveId(8)],
    ),
    // SPECIES_ELEKID
    g(
        SpeciesId(239),
        &[
            MoveId(2),
            MoveId(112),
            MoveId(27),
            MoveId(96),
            MoveId(238),
            MoveId(7),
            MoveId(8),
        ],
    ),
    // SPECIES_MAGBY
    g(
        SpeciesId(240),
        &[
            MoveId(2),
            MoveId(5),
            MoveId(112),
            MoveId(103),
            MoveId(238),
            MoveId(9),
        ],
    ),
    // SPECIES_MILTANK
    g(
        SpeciesId(241),
        &[
            MoveId(217),
            MoveId(179),
            MoveId(69),
            MoveId(203),
            MoveId(244),
            MoveId(174),
            MoveId(270),
            MoveId(214),
        ],
    ),
    // SPECIES_LARVITAR
    g(
        SpeciesId(246),
        &[
            MoveId(228),
            MoveId(23),
            MoveId(200),
            MoveId(116),
            MoveId(246),
            MoveId(349),
            MoveId(174),
        ],
    ),
    // SPECIES_TREECKO
    g(
        SpeciesId(277),
        &[
            MoveId(242),
            MoveId(300),
            MoveId(283),
            MoveId(73),
            MoveId(225),
            MoveId(306),
        ],
    ),
    // SPECIES_TORCHIC
    g(
        SpeciesId(280),
        &[
            MoveId(68),
            MoveId(179),
            MoveId(203),
            MoveId(207),
            MoveId(157),
            MoveId(265),
        ],
    ),
    // SPECIES_MUDKIP
    g(
        SpeciesId(283),
        &[
            MoveId(287),
            MoveId(253),
            MoveId(174),
            MoveId(23),
            MoveId(301),
            MoveId(243),
        ],
    ),
    // SPECIES_POOCHYENA
    g(
        SpeciesId(286),
        &[
            MoveId(310),
            MoveId(305),
            MoveId(343),
            MoveId(43),
            MoveId(281),
        ],
    ),
    // SPECIES_ZIGZAGOON
    g(
        SpeciesId(288),
        &[
            MoveId(204),
            MoveId(228),
            MoveId(164),
            MoveId(321),
            MoveId(271),
        ],
    ),
    // SPECIES_LOTAD
    g(
        SpeciesId(295),
        &[
            MoveId(235),
            MoveId(75),
            MoveId(230),
            MoveId(73),
            MoveId(175),
            MoveId(55),
        ],
    ),
    // SPECIES_SEEDOT
    g(
        SpeciesId(298),
        &[
            MoveId(73),
            MoveId(133),
            MoveId(98),
            MoveId(13),
            MoveId(36),
            MoveId(206),
        ],
    ),
    // SPECIES_NINCADA
    g(
        SpeciesId(301),
        &[MoveId(203), MoveId(185), MoveId(16), MoveId(318)],
    ),
    // SPECIES_TAILLOW
    g(
        SpeciesId(304),
        &[
            MoveId(228),
            MoveId(48),
            MoveId(287),
            MoveId(119),
            MoveId(99),
            MoveId(143),
        ],
    ),
    // SPECIES_SHROOMISH
    g(
        SpeciesId(306),
        &[
            MoveId(313),
            MoveId(207),
            MoveId(204),
            MoveId(206),
            MoveId(270),
        ],
    ),
    // SPECIES_SPINDA
    g(
        SpeciesId(308),
        &[
            MoveId(227),
            MoveId(157),
            MoveId(274),
            MoveId(50),
            MoveId(226),
            MoveId(273),
            MoveId(271),
            MoveId(265),
        ],
    ),
    // SPECIES_WINGULL
    g(
        SpeciesId(309),
        &[MoveId(54), MoveId(239), MoveId(97), MoveId(16), MoveId(346)],
    ),
    // SPECIES_SURSKIT
    g(
        SpeciesId(311),
        &[
            MoveId(193),
            MoveId(341),
            MoveId(60),
            MoveId(56),
            MoveId(170),
        ],
    ),
    // SPECIES_WAILMER
    g(
        SpeciesId(313),
        &[
            MoveId(38),
            MoveId(37),
            MoveId(207),
            MoveId(173),
            MoveId(214),
            MoveId(174),
            MoveId(90),
            MoveId(321),
        ],
    ),
    // SPECIES_SKITTY
    g(
        SpeciesId(315),
        &[
            MoveId(270),
            MoveId(244),
            MoveId(253),
            MoveId(313),
            MoveId(273),
            MoveId(226),
            MoveId(164),
            MoveId(321),
        ],
    ),
    // SPECIES_KECLEON
    g(SpeciesId(317), &[MoveId(50), MoveId(277), MoveId(271)]),
    // SPECIES_NOSEPASS
    g(SpeciesId(320), &[MoveId(222), MoveId(205), MoveId(153)]),
    // SPECIES_TORKOAL
    g(
        SpeciesId(321),
        &[MoveId(284), MoveId(203), MoveId(214), MoveId(281)],
    ),
    // SPECIES_SABLEYE
    g(SpeciesId(322), &[MoveId(244), MoveId(105), MoveId(236)]),
    // SPECIES_BARBOACH
    g(SpeciesId(323), &[MoveId(37), MoveId(250), MoveId(209)]),
    // SPECIES_LUVDISC
    g(
        SpeciesId(325),
        &[MoveId(150), MoveId(48), MoveId(346), MoveId(300)],
    ),
    // SPECIES_CORPHISH
    g(
        SpeciesId(326),
        &[MoveId(300), MoveId(283), MoveId(34), MoveId(246)],
    ),
    // SPECIES_FEEBAS
    g(
        SpeciesId(328),
        &[
            MoveId(243),
            MoveId(225),
            MoveId(300),
            MoveId(95),
            MoveId(113),
            MoveId(109),
        ],
    ),
    // SPECIES_CARVANHA
    g(SpeciesId(330), &[MoveId(56), MoveId(38), MoveId(37)]),
    // SPECIES_TRAPINCH
    g(SpeciesId(332), &[MoveId(116), MoveId(98), MoveId(16)]),
    // SPECIES_MAKUHITA
    g(
        SpeciesId(335),
        &[
            MoveId(185),
            MoveId(197),
            MoveId(193),
            MoveId(270),
            MoveId(238),
            MoveId(279),
            MoveId(223),
            MoveId(68),
        ],
    ),
    // SPECIES_ELECTRIKE
    g(
        SpeciesId(337),
        &[
            MoveId(242),
            MoveId(29),
            MoveId(253),
            MoveId(174),
            MoveId(129),
        ],
    ),
    // SPECIES_NUMEL
    g(
        SpeciesId(339),
        &[
            MoveId(336),
            MoveId(184),
            MoveId(34),
            MoveId(205),
            MoveId(111),
            MoveId(23),
        ],
    ),
    // SPECIES_SPHEAL
    g(
        SpeciesId(341),
        &[
            MoveId(346),
            MoveId(254),
            MoveId(256),
            MoveId(255),
            MoveId(281),
            MoveId(157),
            MoveId(174),
            MoveId(90),
        ],
    ),
    // SPECIES_CACNEA
    g(
        SpeciesId(344),
        &[
            MoveId(320),
            MoveId(51),
            MoveId(298),
            MoveId(223),
            MoveId(68),
        ],
    ),
    // SPECIES_SNORUNT
    g(SpeciesId(346), &[MoveId(335), MoveId(191)]),
    // SPECIES_AZURILL
    g(
        SpeciesId(350),
        &[
            MoveId(227),
            MoveId(47),
            MoveId(287),
            MoveId(21),
            MoveId(321),
        ],
    ),
    // SPECIES_SPOINK
    g(
        SpeciesId(351),
        &[MoveId(248), MoveId(326), MoveId(164), MoveId(271)],
    ),
    // SPECIES_PLUSLE
    g(SpeciesId(353), &[MoveId(164), MoveId(273)]),
    // SPECIES_MINUN
    g(SpeciesId(354), &[MoveId(164), MoveId(273)]),
    // SPECIES_MAWILE
    g(
        SpeciesId(355),
        &[
            MoveId(14),
            MoveId(206),
            MoveId(305),
            MoveId(244),
            MoveId(246),
            MoveId(321),
        ],
    ),
    // SPECIES_MEDITITE
    g(
        SpeciesId(356),
        &[
            MoveId(7),
            MoveId(9),
            MoveId(8),
            MoveId(193),
            MoveId(252),
            MoveId(226),
            MoveId(223),
        ],
    ),
    // SPECIES_SWABLU
    g(
        SpeciesId(358),
        &[MoveId(97), MoveId(114), MoveId(228), MoveId(99)],
    ),
    // SPECIES_DUSKULL
    g(
        SpeciesId(361),
        &[
            MoveId(286),
            MoveId(194),
            MoveId(220),
            MoveId(288),
            MoveId(262),
            MoveId(185),
        ],
    ),
    // SPECIES_ROSELIA
    g(
        SpeciesId(363),
        &[MoveId(191), MoveId(235), MoveId(42), MoveId(178)],
    ),
    // SPECIES_SLAKOTH
    g(
        SpeciesId(364),
        &[
            MoveId(228),
            MoveId(163),
            MoveId(34),
            MoveId(173),
            MoveId(306),
            MoveId(174),
            MoveId(214),
        ],
    ),
    // SPECIES_GULPIN
    g(
        SpeciesId(367),
        &[MoveId(138), MoveId(151), MoveId(123), MoveId(220)],
    ),
    // SPECIES_TROPIUS
    g(
        SpeciesId(369),
        &[MoveId(29), MoveId(21), MoveId(13), MoveId(73), MoveId(267)],
    ),
    // SPECIES_WHISMUR
    g(
        SpeciesId(370),
        &[
            MoveId(36),
            MoveId(173),
            MoveId(207),
            MoveId(326),
            MoveId(265),
        ],
    ),
    // SPECIES_CLAMPERL
    g(
        SpeciesId(373),
        &[
            MoveId(287),
            MoveId(300),
            MoveId(34),
            MoveId(48),
            MoveId(112),
            MoveId(109),
        ],
    ),
    // SPECIES_ABSOL
    g(
        SpeciesId(376),
        &[
            MoveId(226),
            MoveId(185),
            MoveId(38),
            MoveId(277),
            MoveId(174),
            MoveId(164),
        ],
    ),
    // SPECIES_SHUPPET
    g(
        SpeciesId(377),
        &[
            MoveId(50),
            MoveId(194),
            MoveId(193),
            MoveId(310),
            MoveId(286),
        ],
    ),
    // SPECIES_SEVIPER
    g(
        SpeciesId(379),
        &[MoveId(254), MoveId(256), MoveId(255), MoveId(34)],
    ),
    // SPECIES_ZANGOOSE
    g(
        SpeciesId(380),
        &[
            MoveId(175),
            MoveId(24),
            MoveId(13),
            MoveId(68),
            MoveId(46),
            MoveId(174),
        ],
    ),
    // SPECIES_RELICANTH
    g(
        SpeciesId(381),
        &[
            MoveId(222),
            MoveId(130),
            MoveId(346),
            MoveId(133),
            MoveId(214),
            MoveId(157),
        ],
    ),
    // SPECIES_ARON
    g(
        SpeciesId(382),
        &[MoveId(283), MoveId(34), MoveId(23), MoveId(265)],
    ),
    // SPECIES_CASTFORM
    g(SpeciesId(385), &[MoveId(248), MoveId(244)]),
    // SPECIES_VOLBEAT
    g(SpeciesId(386), &[MoveId(226), MoveId(318), MoveId(271)]),
    // SPECIES_ILLUMISE
    g(SpeciesId(387), &[MoveId(226), MoveId(318), MoveId(74)]),
    // SPECIES_LILEEP
    g(
        SpeciesId(388),
        &[MoveId(112), MoveId(105), MoveId(243), MoveId(157)],
    ),
    // SPECIES_ANORITH
    g(
        SpeciesId(390),
        &[MoveId(229), MoveId(282), MoveId(14), MoveId(157)],
    ),
    // SPECIES_RALTS
    g(
        SpeciesId(392),
        &[
            MoveId(50),
            MoveId(261),
            MoveId(212),
            MoveId(262),
            MoveId(194),
        ],
    ),
    // SPECIES_BAGON
    g(
        SpeciesId(395),
        &[MoveId(56), MoveId(37), MoveId(82), MoveId(239), MoveId(349)],
    ),
    // SPECIES_CHIMECHO
    g(
        SpeciesId(411),
        &[MoveId(50), MoveId(174), MoveId(95), MoveId(138)],
    ),
];

/// The owned egg-move table — an idiomatic view over the parsed `gEggMoves`
/// flat stream `(oop-boundaries)`, keyed by [`SpeciesId`].
#[derive(Debug, Clone, Copy)]
pub struct EggMoveTable {
    groups: &'static [EggMoveList; EGG_MOVE_SPECIES_COUNT],
}

impl EggMoveTable {
    /// Construct the table (a thin handle over the embedded static data).
    #[must_use]
    pub const fn new() -> Self {
        Self { groups: &EGG_MOVES }
    }

    /// The number of species with an egg-move group (`EGG_MOVE_SPECIES_COUNT`).
    #[must_use]
    pub const fn len(&self) -> usize {
        EGG_MOVE_SPECIES_COUNT
    }

    /// Always `false`: the table is never empty. Present to satisfy the
    /// `len`/`is_empty` convention.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }

    /// The egg-move list for `species`.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::NoEggMoves`] if `species` has no egg-move group in
    /// `gEggMoves` (most species — only breeding base-forms carry egg moves).
    pub fn get(&self, species: SpeciesId) -> Result<&EggMoveList, AssetError> {
        // The table is stored in ascending species-id order (mirroring the flat
        // stream), so a binary search is both correct and cheap.
        self.groups
            .binary_search_by_key(&species, |group| group.species)
            .map(|idx| &self.groups[idx])
            .map_err(|_| AssetError::NoEggMoves(species.0))
    }

    /// The egg moves for `species`, or `None` if it has no egg-move group.
    #[must_use]
    pub fn moves_for(&self, species: SpeciesId) -> Option<&'static [MoveId]> {
        self.get(species).ok().map(EggMoveList::moves)
    }

    /// Iterate over every species' egg-move list in ascending species-id order.
    pub fn iter(&self) -> impl Iterator<Item = &EggMoveList> {
        self.groups.iter()
    }
}

impl Default for EggMoveTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        EggMoveList, EggMoveTable, EGG_MOVES_SPECIES_OFFSET, EGG_MOVES_TERMINATOR,
        EGG_MOVE_SPECIES_COUNT,
    };
    use crate::battle_moves::MoveId;
    use crate::error::AssetError;
    use crate::species::SpeciesId;

    #[test]
    fn table_length_matches_species_count() {
        // Structural anchor: one EggMoveList per `egg_moves(...)` group.
        let table = EggMoveTable::new();
        assert_eq!(table.len(), 165);
        assert_eq!(EGG_MOVE_SPECIES_COUNT, 165);
        assert_eq!(table.iter().count(), 165);
        assert!(!table.is_empty());
    }

    #[test]
    fn flat_stream_word_count_is_reconstructed() {
        // Re-derive the length of the flat upstream `gEggMoves[]` array from the
        // parsed table: one sentinel word per species group, plus every move
        // word, plus the single trailing EGG_MOVES_TERMINATOR. Upstream's array
        // is exactly 1139 `u16`s (165 sentinels + 973 moves + 1 terminator);
        // this guards the sentinel/terminator accounting of the parse.
        let table = EggMoveTable::new();
        let total_moves: usize = table.iter().map(EggMoveList::len).sum();
        assert_eq!(total_moves, 973);
        let flat_words = table.len() + total_moves + 1;
        assert_eq!(flat_words, 1139);
    }

    #[test]
    fn every_group_is_non_empty_and_species_unique_and_sorted() {
        // Duplicate/round-trip guard: every group has at least one move, no two
        // groups share a species, and the table stays in ascending species-id
        // order (the invariant the binary-search accessor relies on).
        let table = EggMoveTable::new();
        let mut prev: Option<u16> = None;
        for group in table.iter() {
            assert!(
                !group.is_empty(),
                "species {} has no egg moves",
                group.species.0
            );
            if let Some(p) = prev {
                assert!(
                    group.species.0 > p,
                    "species {} out of order after {}",
                    group.species.0,
                    p
                );
            }
            prev = Some(group.species.0);
        }
    }

    #[test]
    fn sentinels_stay_below_the_terminator() {
        // Encoding guard: each group's sentinel word (species + offset) is a real
        // `u16` that is neither the terminator nor collides with another group,
        // exactly as the flat stream requires.
        let table = EggMoveTable::new();
        for group in table.iter() {
            let sentinel = group.species.0 + EGG_MOVES_SPECIES_OFFSET;
            assert!(
                sentinel < EGG_MOVES_TERMINATOR,
                "sentinel {sentinel} for species {} collides with the terminator",
                group.species.0
            );
            assert!(sentinel >= EGG_MOVES_SPECIES_OFFSET);
        }
        assert_eq!(EGG_MOVES_TERMINATOR, 0xFFFF);
        assert_eq!(EGG_MOVES_SPECIES_OFFSET, 20000);
    }

    #[test]
    fn unknown_species_has_no_egg_moves() {
        let table = EggMoveTable::new();
        // SPECIES_NONE (0) and Pikachu (25, which evolves from Pichu and has no
        // egg-move group of its own) both lack an entry.
        assert_eq!(table.get(SpeciesId(0)), Err(AssetError::NoEggMoves(0)));
        assert_eq!(table.get(SpeciesId(25)), Err(AssetError::NoEggMoves(25)));
        assert_eq!(table.moves_for(SpeciesId(25)), None);
    }

    #[test]
    fn upstream_tie_named_egg_move_lists() {
        // Pins several species' full egg-move lists back to their exact
        // `egg_moves.h` values (hardcoded here — CI has no `pokeemerald/`).
        // Species ids from constants/species.h, move ids from constants/moves.h.
        let table = EggMoveTable::new();
        let list = |sid: u16| table.get(SpeciesId(sid)).unwrap().moves;

        // egg_moves(BULBASAUR == 1, ...) — the very first group.
        // LIGHT_SCREEN 113, SKULL_BASH 130, SAFEGUARD 219, CHARM 204,
        // PETAL_DANCE 80, MAGICAL_LEAF 345, GRASS_WHISTLE 320, CURSE 174.
        assert_eq!(
            list(1),
            &[
                MoveId(113),
                MoveId(130),
                MoveId(219),
                MoveId(204),
                MoveId(80),
                MoveId(345),
                MoveId(320),
                MoveId(174),
            ]
        );

        // egg_moves(CHARMANDER == 4, ...): BELLY_DRUM 187, ANCIENT_POWER 246,
        // ROCK_SLIDE 157, BITE 44, OUTRAGE 200, BEAT_UP 251, SWORDS_DANCE 14,
        // DRAGON_DANCE 349.
        assert_eq!(
            list(4),
            &[
                MoveId(187),
                MoveId(246),
                MoveId(157),
                MoveId(44),
                MoveId(200),
                MoveId(251),
                MoveId(14),
                MoveId(349),
            ]
        );

        // egg_moves(SUDOWOODO == 185, MOVE_SELF_DESTRUCT == 120) — a single-move
        // group, exercising the "one move then next sentinel" boundary.
        assert_eq!(list(185), &[MoveId(120)]);

        // egg_moves(SHUCKLE == 213, MOVE_SWEET_SCENT == 230) — the other
        // single-move group.
        assert_eq!(list(213), &[MoveId(230)]);

        // egg_moves(TREECKO == 277, ...): CRUNCH 242, MUD_SPORT 300,
        // ENDEAVOR 283, LEECH_SEED 73, DRAGON_BREATH 225, CRUSH_CLAW 306.
        assert_eq!(
            list(277),
            &[
                MoveId(242),
                MoveId(300),
                MoveId(283),
                MoveId(73),
                MoveId(225),
                MoveId(306),
            ]
        );

        // egg_moves(CHIMECHO == 411, ...): the final group before the
        // terminator. DISABLE 50, CURSE 174, HYPNOSIS 95, DREAM_EATER 138.
        assert_eq!(
            list(411),
            &[MoveId(50), MoveId(174), MoveId(95), MoveId(138)]
        );
        // And it really is the last table entry.
        assert_eq!(table.iter().last().unwrap().species, SpeciesId(411));
    }

    #[test]
    fn teaches_matches_the_move_list() {
        let table = EggMoveTable::new();
        let bulbasaur = table.get(SpeciesId(1)).unwrap();
        assert!(bulbasaur.teaches(MoveId(174))); // MOVE_CURSE
        assert!(!bulbasaur.teaches(MoveId(1))); // MOVE_POUND — not an egg move
    }
}
