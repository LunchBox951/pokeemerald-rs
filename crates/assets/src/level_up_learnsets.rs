//! Per-species level-up learnsets (S-4): the `gLevelUpLearnsets` table.
//!
//! Ports every species' level-up move progression from the upstream reference
//! `pokeemerald/src/data/pokemon/level_up_learnsets.h` (the per-species
//! `static const u16 sXxxLevelUpLearnset[]` arrays) together with the
//! species-to-learnset mapping in
//! `pokeemerald/src/data/pokemon/level_up_learnset_pointers.h`
//! (`const u16 *const gLevelUpLearnsets[NUM_SPECIES]`).
//!
//! Upstream packs each entry into a single `u16` with the macro
//! `LEVEL_UP_MOVE(lvl, move) = ((lvl) << 9) | (move)` and terminates every array
//! with the sentinel `LEVEL_UP_END` (`0xFFFF`, `constants/pokemon.h`). The
//! packing is re-expressed idiomatically here `(no-verbatim)`: each entry is a
//! typed [`LevelUpMove`] carrying a decoded [`level`](LevelUpMove::level) and a
//! reused [`MoveId`], and the terminator is represented structurally by the end
//! of the slice rather than a magic in-band value. The `level << 9` layout means
//! the level occupies the top 7 bits (so it fits a [`u8`], max `127`) and the
//! move the low 9 bits; [`LevelUpMove::packed`] reconstructs the exact upstream
//! `u16` so the round-trip is checked against the real encoding in the tests.
//!
//! The per-species ordering is preserved verbatim from upstream — the game reads
//! these arrays in order, so the sequence (including repeated levels and the
//! level-1 "already knows" moves listed first) is behaviour, not incidental
//! `(behavioral-fidelity)`. Note that upstream's `[SPECIES_NONE]` slot points at
//! Bulbasaur's learnset; that quirk is transcribed faithfully.

use crate::battle_moves::MoveId;
use crate::error::AssetError;
use crate::species::SpeciesId;

/// The number of species slots in the table, matching upstream `NUM_SPECIES`
/// and the `gLevelUpLearnsets[NUM_SPECIES]` pointer array: ids `0..=411`
/// (index `0` is the `SPECIES_NONE` slot, which upstream aliases to Bulbasaur's
/// learnset).
pub const SPECIES_COUNT: usize = 412;

/// The number of bits the level is shifted by in the upstream packed `u16`
/// (`LEVEL_UP_MOVE(lvl, move) = (lvl << 9) | move`). The move id therefore
/// occupies the low `9` bits and the level the high `7`.
const LEVEL_SHIFT: u16 = 9;

/// The low-9-bit mask selecting the move id out of a packed entry.
const MOVE_MASK: u16 = (1 << LEVEL_SHIFT) - 1;

/// One ordered level-up learnset entry — the decoded form of a single upstream
/// `LEVEL_UP_MOVE(level, move)` packed `u16`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LevelUpMove {
    /// The level at which the move is learned (`1..=127`; the top 7 bits of the
    /// packed upstream value).
    pub level: u8,
    /// The move learned, as a reused [`MoveId`] (the low 9 bits upstream).
    pub move_id: MoveId,
}

impl LevelUpMove {
    /// Decode one raw upstream `LEVEL_UP_MOVE(level, move)` packed `u16`
    /// (`(level << 9) | move`) into its typed form — the inverse of
    /// [`packed`](LevelUpMove::packed).
    ///
    /// This is exactly how each transcribed entry was derived from the upstream
    /// arrays; exposing it lets the round-trip be checked against the real
    /// packing. The `LEVEL_UP_END` sentinel (`0xFFFF`) is *not* a valid entry
    /// and is handled structurally (by slice length) rather than passed here.
    #[must_use]
    pub const fn from_packed(packed: u16) -> Self {
        Self {
            #[allow(clippy::cast_possible_truncation)]
            level: (packed >> LEVEL_SHIFT) as u8,
            move_id: MoveId(packed & MOVE_MASK),
        }
    }

    /// Reconstruct the exact upstream packed `u16`
    /// (`(level << 9) | move_id`), the inverse of
    /// [`from_packed`](LevelUpMove::from_packed). Lets the round-trip be checked
    /// against the real `LEVEL_UP_MOVE` encoding.
    #[must_use]
    pub const fn packed(self) -> u16 {
        ((self.level as u16) << LEVEL_SHIFT) | self.move_id.index()
    }
}

/// Construct one [`LevelUpMove`] from a `(level, raw move id)` pair. A terse
/// free helper so the generated per-species tables stay compact and readable;
/// mirrors the two operands of the upstream `LEVEL_UP_MOVE` macro.
const fn e(level: u8, move_id: u16) -> LevelUpMove {
    LevelUpMove {
        level,
        move_id: MoveId(move_id),
    }
}

const LEARNSET_ABRA: &[LevelUpMove] = &[e(1, 100)];
const LEARNSET_ABSOL: &[LevelUpMove] = &[
    e(1, 10),
    e(5, 43),
    e(9, 269),
    e(13, 98),
    e(17, 13),
    e(21, 44),
    e(26, 14),
    e(31, 104),
    e(36, 163),
    e(41, 248),
    e(46, 195),
];
const LEARNSET_AERODACTYL: &[LevelUpMove] = &[
    e(1, 17),
    e(8, 97),
    e(15, 44),
    e(22, 48),
    e(29, 246),
    e(36, 184),
    e(43, 36),
    e(50, 63),
];
const LEARNSET_AGGRON: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 106),
    e(1, 189),
    e(1, 29),
    e(4, 106),
    e(7, 189),
    e(10, 29),
    e(13, 232),
    e(17, 334),
    e(21, 46),
    e(25, 36),
    e(29, 231),
    e(37, 182),
    e(50, 319),
    e(63, 38),
];
const LEARNSET_AIPOM: &[LevelUpMove] = &[
    e(1, 10),
    e(1, 39),
    e(6, 28),
    e(13, 310),
    e(18, 226),
    e(25, 321),
    e(31, 154),
    e(38, 129),
    e(43, 103),
    e(50, 97),
];
const LEARNSET_ALAKAZAM: &[LevelUpMove] = &[
    e(1, 100),
    e(1, 134),
    e(1, 93),
    e(16, 93),
    e(18, 50),
    e(21, 60),
    e(23, 115),
    e(25, 105),
    e(30, 248),
    e(33, 347),
    e(36, 94),
    e(43, 271),
];
const LEARNSET_ALTARIA: &[LevelUpMove] = &[
    e(1, 64),
    e(1, 45),
    e(1, 310),
    e(1, 47),
    e(8, 310),
    e(11, 47),
    e(18, 31),
    e(21, 219),
    e(28, 54),
    e(31, 36),
    e(35, 225),
    e(40, 349),
    e(45, 287),
    e(54, 195),
    e(59, 143),
];
const LEARNSET_AMPHAROS: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 45),
    e(1, 84),
    e(1, 86),
    e(9, 84),
    e(18, 86),
    e(27, 178),
    e(30, 9),
    e(42, 113),
    e(57, 87),
];
const LEARNSET_ANORITH: &[LevelUpMove] = &[
    e(1, 10),
    e(7, 106),
    e(13, 300),
    e(19, 55),
    e(25, 232),
    e(31, 182),
    e(37, 246),
    e(43, 210),
    e(49, 163),
    e(55, 350),
];
const LEARNSET_ARBOK: &[LevelUpMove] = &[
    e(1, 35),
    e(1, 43),
    e(1, 40),
    e(1, 44),
    e(8, 40),
    e(13, 44),
    e(20, 137),
    e(28, 103),
    e(38, 51),
    e(46, 254),
    e(46, 256),
    e(46, 255),
    e(56, 114),
];
const LEARNSET_ARCANINE: &[LevelUpMove] = &[e(1, 44), e(1, 46), e(1, 52), e(1, 316), e(49, 245)];
const LEARNSET_ARIADOS: &[LevelUpMove] = &[
    e(1, 40),
    e(1, 81),
    e(1, 184),
    e(1, 132),
    e(6, 184),
    e(11, 132),
    e(17, 101),
    e(25, 141),
    e(34, 154),
    e(43, 169),
    e(53, 97),
    e(63, 94),
];
const LEARNSET_ARMALDO: &[LevelUpMove] = &[
    e(1, 10),
    e(1, 106),
    e(1, 300),
    e(1, 55),
    e(7, 106),
    e(13, 300),
    e(19, 55),
    e(25, 232),
    e(31, 182),
    e(37, 246),
    e(46, 210),
    e(55, 163),
    e(64, 350),
];
const LEARNSET_ARON: &[LevelUpMove] = &[
    e(1, 33),
    e(4, 106),
    e(7, 189),
    e(10, 29),
    e(13, 232),
    e(17, 334),
    e(21, 46),
    e(25, 36),
    e(29, 231),
    e(34, 182),
    e(39, 319),
    e(44, 38),
];
const LEARNSET_ARTICUNO: &[LevelUpMove] = &[
    e(1, 16),
    e(1, 181),
    e(13, 54),
    e(25, 97),
    e(37, 170),
    e(49, 58),
    e(61, 115),
    e(73, 59),
    e(85, 329),
];
const LEARNSET_AZUMARILL: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 111),
    e(1, 39),
    e(1, 55),
    e(3, 111),
    e(6, 39),
    e(10, 55),
    e(15, 205),
    e(24, 61),
    e(34, 38),
    e(45, 240),
    e(57, 56),
];
const LEARNSET_AZURILL: &[LevelUpMove] = &[
    e(1, 150),
    e(3, 204),
    e(6, 39),
    e(10, 145),
    e(15, 21),
    e(21, 55),
];
const LEARNSET_BAGON: &[LevelUpMove] = &[
    e(1, 99),
    e(5, 44),
    e(9, 43),
    e(17, 29),
    e(21, 116),
    e(25, 52),
    e(33, 225),
    e(37, 184),
    e(41, 242),
    e(49, 337),
    e(53, 38),
];
const LEARNSET_BALTOY: &[LevelUpMove] = &[
    e(1, 93),
    e(3, 106),
    e(5, 229),
    e(7, 189),
    e(11, 60),
    e(15, 317),
    e(19, 120),
    e(25, 246),
    e(31, 201),
    e(37, 322),
    e(45, 153),
];
const LEARNSET_BANETTE: &[LevelUpMove] = &[
    e(1, 282),
    e(1, 103),
    e(1, 101),
    e(1, 174),
    e(8, 103),
    e(13, 101),
    e(20, 174),
    e(25, 180),
    e(32, 261),
    e(39, 185),
    e(48, 247),
    e(55, 289),
    e(64, 288),
];
const LEARNSET_BARBOACH: &[LevelUpMove] = &[
    e(1, 189),
    e(6, 300),
    e(6, 346),
    e(11, 55),
    e(16, 222),
    e(21, 133),
    e(26, 156),
    e(26, 173),
    e(31, 89),
    e(36, 248),
    e(41, 90),
];
const LEARNSET_BAYLEEF: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 45),
    e(1, 75),
    e(1, 115),
    e(8, 75),
    e(12, 115),
    e(15, 77),
    e(23, 235),
    e(31, 34),
    e(39, 113),
    e(47, 219),
    e(55, 76),
];
const LEARNSET_BEAUTIFLY: &[LevelUpMove] = &[
    e(1, 71),
    e(10, 71),
    e(13, 16),
    e(17, 78),
    e(20, 234),
    e(24, 72),
    e(27, 18),
    e(31, 213),
    e(34, 318),
    e(38, 202),
];
const LEARNSET_BEEDRILL: &[LevelUpMove] = &[
    e(1, 31),
    e(10, 31),
    e(15, 116),
    e(20, 41),
    e(25, 99),
    e(30, 228),
    e(35, 42),
    e(40, 97),
    e(45, 283),
];
const LEARNSET_BELDUM: &[LevelUpMove] = &[e(1, 36)];
const LEARNSET_BELLOSSOM: &[LevelUpMove] = &[
    e(1, 71),
    e(1, 230),
    e(1, 78),
    e(1, 345),
    e(44, 80),
    e(55, 76),
];
const LEARNSET_BELLSPROUT: &[LevelUpMove] = &[
    e(1, 22),
    e(6, 74),
    e(11, 35),
    e(15, 79),
    e(17, 77),
    e(19, 78),
    e(23, 51),
    e(30, 230),
    e(37, 75),
    e(45, 21),
];
const LEARNSET_BLASTOISE: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 39),
    e(1, 145),
    e(1, 110),
    e(4, 39),
    e(7, 145),
    e(10, 110),
    e(13, 55),
    e(19, 44),
    e(25, 229),
    e(31, 182),
    e(42, 240),
    e(55, 130),
    e(68, 56),
];
const LEARNSET_BLAZIKEN: &[LevelUpMove] = &[
    e(1, 7),
    e(1, 10),
    e(1, 45),
    e(1, 116),
    e(1, 52),
    e(7, 116),
    e(13, 52),
    e(16, 24),
    e(17, 64),
    e(21, 28),
    e(28, 339),
    e(32, 98),
    e(36, 299),
    e(42, 163),
    e(49, 119),
    e(59, 327),
];
const LEARNSET_BLISSEY: &[LevelUpMove] = &[
    e(1, 1),
    e(1, 45),
    e(4, 39),
    e(7, 287),
    e(10, 135),
    e(13, 3),
    e(18, 107),
    e(23, 47),
    e(28, 121),
    e(33, 111),
    e(40, 113),
    e(47, 38),
];
const LEARNSET_BRELOOM: &[LevelUpMove] = &[
    e(1, 71),
    e(1, 33),
    e(1, 78),
    e(1, 73),
    e(4, 33),
    e(7, 78),
    e(10, 73),
    e(16, 72),
    e(22, 29),
    e(23, 183),
    e(28, 68),
    e(36, 327),
    e(45, 170),
    e(54, 223),
];
const LEARNSET_BULBASAUR: &[LevelUpMove] = &[
    e(1, 33),
    e(4, 45),
    e(7, 73),
    e(10, 22),
    e(15, 77),
    e(15, 79),
    e(20, 75),
    e(25, 230),
    e(32, 74),
    e(39, 235),
    e(46, 76),
];
const LEARNSET_BUTTERFREE: &[LevelUpMove] = &[
    e(1, 93),
    e(10, 93),
    e(13, 77),
    e(14, 78),
    e(15, 79),
    e(18, 48),
    e(23, 18),
    e(28, 16),
    e(34, 60),
    e(40, 219),
    e(47, 318),
];
const LEARNSET_CACNEA: &[LevelUpMove] = &[
    e(1, 40),
    e(1, 43),
    e(5, 71),
    e(9, 74),
    e(13, 73),
    e(17, 28),
    e(21, 42),
    e(25, 275),
    e(29, 185),
    e(33, 191),
    e(37, 302),
    e(41, 178),
    e(45, 201),
];
const LEARNSET_CACTURNE: &[LevelUpMove] = &[
    e(1, 40),
    e(1, 43),
    e(1, 71),
    e(1, 74),
    e(5, 71),
    e(9, 74),
    e(13, 73),
    e(17, 28),
    e(21, 42),
    e(25, 275),
    e(29, 185),
    e(35, 191),
    e(41, 302),
    e(47, 178),
    e(53, 201),
];
const LEARNSET_CAMERUPT: &[LevelUpMove] = &[
    e(1, 45),
    e(1, 33),
    e(1, 52),
    e(1, 222),
    e(11, 52),
    e(19, 222),
    e(25, 116),
    e(29, 36),
    e(31, 133),
    e(33, 157),
    e(37, 89),
    e(45, 284),
    e(55, 90),
];
const LEARNSET_CARVANHA: &[LevelUpMove] = &[
    e(1, 43),
    e(1, 44),
    e(7, 99),
    e(13, 116),
    e(16, 184),
    e(22, 242),
    e(28, 103),
    e(31, 36),
    e(37, 207),
    e(43, 97),
];
const LEARNSET_CASCOON: &[LevelUpMove] = &[e(1, 106), e(7, 106)];
const LEARNSET_CASTFORM: &[LevelUpMove] = &[
    e(1, 33),
    e(10, 55),
    e(10, 52),
    e(10, 181),
    e(20, 240),
    e(20, 241),
    e(20, 258),
    e(30, 311),
];
const LEARNSET_CATERPIE: &[LevelUpMove] = &[e(1, 33), e(1, 81)];
const LEARNSET_CELEBI: &[LevelUpMove] = &[
    e(1, 73),
    e(1, 93),
    e(1, 105),
    e(1, 215),
    e(10, 219),
    e(20, 246),
    e(30, 248),
    e(40, 226),
    e(50, 195),
];
const LEARNSET_CHANSEY: &[LevelUpMove] = &[
    e(1, 1),
    e(1, 45),
    e(5, 39),
    e(9, 287),
    e(13, 135),
    e(17, 3),
    e(23, 107),
    e(29, 47),
    e(35, 121),
    e(41, 111),
    e(49, 113),
    e(57, 38),
];
const LEARNSET_CHARIZARD: &[LevelUpMove] = &[
    e(1, 10),
    e(1, 45),
    e(1, 52),
    e(1, 108),
    e(7, 52),
    e(13, 108),
    e(20, 99),
    e(27, 184),
    e(34, 53),
    e(36, 17),
    e(44, 163),
    e(54, 82),
    e(64, 83),
];
const LEARNSET_CHARMANDER: &[LevelUpMove] = &[
    e(1, 10),
    e(1, 45),
    e(7, 52),
    e(13, 108),
    e(19, 99),
    e(25, 184),
    e(31, 53),
    e(37, 163),
    e(43, 82),
    e(49, 83),
];
const LEARNSET_CHARMELEON: &[LevelUpMove] = &[
    e(1, 10),
    e(1, 45),
    e(1, 52),
    e(7, 52),
    e(13, 108),
    e(20, 99),
    e(27, 184),
    e(34, 53),
    e(41, 163),
    e(48, 82),
    e(55, 83),
];
const LEARNSET_CHIKORITA: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 45),
    e(8, 75),
    e(12, 115),
    e(15, 77),
    e(22, 235),
    e(29, 34),
    e(36, 113),
    e(43, 219),
    e(50, 76),
];
const LEARNSET_CHIMECHO: &[LevelUpMove] = &[
    e(1, 35),
    e(6, 45),
    e(9, 310),
    e(14, 93),
    e(17, 36),
    e(22, 253),
    e(25, 281),
    e(30, 149),
    e(33, 38),
    e(38, 215),
    e(41, 219),
    e(46, 94),
];
const LEARNSET_CHINCHOU: &[LevelUpMove] = &[
    e(1, 145),
    e(1, 86),
    e(5, 48),
    e(13, 175),
    e(17, 55),
    e(25, 209),
    e(29, 109),
    e(37, 36),
    e(41, 56),
    e(49, 268),
];
const LEARNSET_CLAMPERL: &[LevelUpMove] = &[e(1, 128), e(1, 55), e(1, 250), e(1, 334)];
const LEARNSET_CLAYDOL: &[LevelUpMove] = &[
    e(1, 100),
    e(1, 93),
    e(1, 106),
    e(1, 229),
    e(3, 106),
    e(5, 229),
    e(7, 189),
    e(11, 60),
    e(15, 317),
    e(19, 120),
    e(25, 246),
    e(31, 201),
    e(36, 63),
    e(42, 322),
    e(55, 153),
];
const LEARNSET_CLEFABLE: &[LevelUpMove] = &[e(1, 47), e(1, 3), e(1, 107), e(1, 118)];
const LEARNSET_CLEFAIRY: &[LevelUpMove] = &[
    e(1, 1),
    e(1, 45),
    e(5, 227),
    e(9, 47),
    e(13, 3),
    e(17, 266),
    e(21, 107),
    e(25, 111),
    e(29, 118),
    e(33, 322),
    e(37, 236),
    e(41, 113),
    e(45, 309),
];
const LEARNSET_CLEFFA: &[LevelUpMove] = &[e(1, 1), e(1, 204), e(4, 227), e(8, 47), e(13, 186)];
const LEARNSET_CLOYSTER: &[LevelUpMove] = &[
    e(1, 110),
    e(1, 48),
    e(1, 62),
    e(1, 182),
    e(33, 191),
    e(41, 131),
];
const LEARNSET_COMBUSKEN: &[LevelUpMove] = &[
    e(1, 10),
    e(1, 45),
    e(1, 116),
    e(1, 52),
    e(7, 116),
    e(13, 52),
    e(16, 24),
    e(17, 64),
    e(21, 28),
    e(28, 339),
    e(32, 98),
    e(39, 163),
    e(43, 119),
    e(50, 327),
];
const LEARNSET_CORPHISH: &[LevelUpMove] = &[
    e(1, 145),
    e(7, 106),
    e(10, 11),
    e(13, 43),
    e(20, 61),
    e(23, 182),
    e(26, 282),
    e(32, 269),
    e(35, 152),
    e(38, 14),
    e(44, 12),
];
const LEARNSET_CORSOLA: &[LevelUpMove] = &[
    e(1, 33),
    e(6, 106),
    e(12, 145),
    e(17, 105),
    e(17, 287),
    e(23, 61),
    e(28, 131),
    e(34, 350),
    e(39, 243),
    e(45, 246),
];
const LEARNSET_CRADILY: &[LevelUpMove] = &[
    e(1, 310),
    e(1, 132),
    e(1, 51),
    e(1, 275),
    e(8, 132),
    e(15, 51),
    e(22, 275),
    e(29, 109),
    e(36, 133),
    e(48, 246),
    e(60, 254),
    e(60, 255),
    e(60, 256),
];
const LEARNSET_CRAWDAUNT: &[LevelUpMove] = &[
    e(1, 145),
    e(1, 106),
    e(1, 11),
    e(1, 43),
    e(7, 106),
    e(10, 11),
    e(13, 43),
    e(20, 61),
    e(23, 182),
    e(26, 282),
    e(34, 269),
    e(39, 152),
    e(44, 14),
    e(52, 12),
];
const LEARNSET_CROBAT: &[LevelUpMove] = &[
    e(1, 103),
    e(1, 141),
    e(1, 48),
    e(1, 310),
    e(6, 48),
    e(11, 310),
    e(16, 44),
    e(21, 17),
    e(28, 109),
    e(35, 314),
    e(42, 212),
    e(49, 305),
    e(56, 114),
];
const LEARNSET_CROCONAW: &[LevelUpMove] = &[
    e(1, 10),
    e(1, 43),
    e(1, 99),
    e(7, 99),
    e(13, 55),
    e(21, 44),
    e(28, 184),
    e(37, 163),
    e(45, 103),
    e(55, 56),
];
const LEARNSET_CUBONE: &[LevelUpMove] = &[
    e(1, 45),
    e(5, 39),
    e(9, 125),
    e(13, 29),
    e(17, 43),
    e(21, 116),
    e(25, 155),
    e(29, 99),
    e(33, 206),
    e(37, 37),
    e(41, 198),
    e(45, 38),
];
const LEARNSET_CYNDAQUIL: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 43),
    e(6, 108),
    e(12, 52),
    e(19, 98),
    e(27, 172),
    e(36, 129),
    e(46, 53),
];
const LEARNSET_DELCATTY: &[LevelUpMove] = &[e(1, 45), e(1, 213), e(1, 47), e(1, 3)];
const LEARNSET_DELIBIRD: &[LevelUpMove] = &[e(1, 217)];
const LEARNSET_DEOXYS: &[LevelUpMove] = &[
    e(1, 43),
    e(1, 35),
    e(5, 101),
    e(10, 104),
    e(15, 282),
    e(20, 228),
    e(25, 94),
    e(30, 129),
    e(35, 97),
    e(40, 105),
    e(45, 354),
    e(50, 245),
];
const LEARNSET_DEWGONG: &[LevelUpMove] = &[
    e(1, 29),
    e(1, 45),
    e(1, 196),
    e(1, 62),
    e(9, 45),
    e(17, 196),
    e(21, 62),
    e(29, 156),
    e(34, 329),
    e(42, 36),
    e(51, 58),
    e(64, 219),
];
const LEARNSET_DIGLETT: &[LevelUpMove] = &[
    e(1, 10),
    e(1, 28),
    e(5, 45),
    e(9, 222),
    e(17, 91),
    e(25, 189),
    e(33, 163),
    e(41, 89),
    e(49, 90),
];
const LEARNSET_DITTO: &[LevelUpMove] = &[e(1, 144)];
const LEARNSET_DODRIO: &[LevelUpMove] = &[
    e(1, 64),
    e(1, 45),
    e(1, 228),
    e(1, 31),
    e(9, 228),
    e(13, 31),
    e(21, 161),
    e(25, 99),
    e(38, 253),
    e(47, 65),
    e(60, 97),
];
const LEARNSET_DODUO: &[LevelUpMove] = &[
    e(1, 64),
    e(1, 45),
    e(9, 228),
    e(13, 31),
    e(21, 161),
    e(25, 99),
    e(33, 253),
    e(37, 65),
    e(45, 97),
];
const LEARNSET_DONPHAN: &[LevelUpMove] = &[
    e(1, 316),
    e(1, 30),
    e(1, 45),
    e(9, 111),
    e(17, 175),
    e(25, 31),
    e(33, 205),
    e(41, 229),
    e(49, 89),
];
const LEARNSET_DRAGONAIR: &[LevelUpMove] = &[
    e(1, 35),
    e(1, 43),
    e(1, 86),
    e(1, 239),
    e(8, 86),
    e(15, 239),
    e(22, 82),
    e(29, 21),
    e(38, 97),
    e(47, 219),
    e(56, 200),
    e(65, 63),
];
const LEARNSET_DRAGONITE: &[LevelUpMove] = &[
    e(1, 35),
    e(1, 43),
    e(1, 86),
    e(1, 239),
    e(8, 86),
    e(15, 239),
    e(22, 82),
    e(29, 21),
    e(38, 97),
    e(47, 219),
    e(55, 17),
    e(61, 200),
    e(75, 63),
];
const LEARNSET_DRATINI: &[LevelUpMove] = &[
    e(1, 35),
    e(1, 43),
    e(8, 86),
    e(15, 239),
    e(22, 82),
    e(29, 21),
    e(36, 97),
    e(43, 219),
    e(50, 200),
    e(57, 63),
];
const LEARNSET_DROWZEE: &[LevelUpMove] = &[
    e(1, 1),
    e(1, 95),
    e(10, 50),
    e(18, 93),
    e(25, 29),
    e(31, 139),
    e(36, 96),
    e(40, 94),
    e(43, 244),
    e(45, 248),
];
const LEARNSET_DUGTRIO: &[LevelUpMove] = &[
    e(1, 161),
    e(1, 10),
    e(1, 28),
    e(1, 45),
    e(5, 45),
    e(9, 222),
    e(17, 91),
    e(25, 189),
    e(26, 328),
    e(38, 163),
    e(51, 89),
    e(64, 90),
];
const LEARNSET_DUNSPARCE: &[LevelUpMove] = &[
    e(1, 99),
    e(4, 111),
    e(11, 281),
    e(14, 137),
    e(21, 180),
    e(24, 228),
    e(31, 103),
    e(34, 36),
    e(41, 283),
];
const LEARNSET_DUSCLOPS: &[LevelUpMove] = &[
    e(1, 20),
    e(1, 43),
    e(1, 101),
    e(1, 50),
    e(5, 50),
    e(12, 193),
    e(16, 310),
    e(23, 109),
    e(27, 228),
    e(34, 174),
    e(37, 325),
    e(41, 261),
    e(51, 212),
    e(58, 248),
];
const LEARNSET_DUSKULL: &[LevelUpMove] = &[
    e(1, 43),
    e(1, 101),
    e(5, 50),
    e(12, 193),
    e(16, 310),
    e(23, 109),
    e(27, 228),
    e(34, 174),
    e(38, 261),
    e(45, 212),
    e(49, 248),
];
const LEARNSET_DUSTOX: &[LevelUpMove] = &[
    e(1, 93),
    e(10, 93),
    e(13, 16),
    e(17, 182),
    e(20, 236),
    e(24, 60),
    e(27, 18),
    e(31, 113),
    e(34, 318),
    e(38, 92),
];
const LEARNSET_EEVEE: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 39),
    e(1, 270),
    e(8, 28),
    e(16, 45),
    e(23, 98),
    e(30, 44),
    e(36, 226),
    e(42, 36),
];
const LEARNSET_EKANS: &[LevelUpMove] = &[
    e(1, 35),
    e(1, 43),
    e(8, 40),
    e(13, 44),
    e(20, 137),
    e(25, 103),
    e(32, 51),
    e(37, 254),
    e(37, 256),
    e(37, 255),
    e(44, 114),
];
const LEARNSET_ELECTABUZZ: &[LevelUpMove] = &[
    e(1, 98),
    e(1, 43),
    e(1, 9),
    e(9, 9),
    e(17, 113),
    e(25, 129),
    e(36, 103),
    e(47, 85),
    e(58, 87),
];
const LEARNSET_ELECTRIKE: &[LevelUpMove] = &[
    e(1, 33),
    e(4, 86),
    e(9, 43),
    e(12, 336),
    e(17, 98),
    e(20, 209),
    e(25, 316),
    e(28, 46),
    e(33, 44),
    e(36, 87),
    e(41, 268),
];
const LEARNSET_ELECTRODE: &[LevelUpMove] = &[
    e(1, 268),
    e(1, 33),
    e(1, 103),
    e(1, 49),
    e(8, 103),
    e(15, 49),
    e(21, 209),
    e(27, 120),
    e(34, 205),
    e(41, 113),
    e(48, 129),
    e(54, 153),
    e(59, 243),
];
const LEARNSET_ELEKID: &[LevelUpMove] = &[
    e(1, 98),
    e(1, 43),
    e(9, 9),
    e(17, 113),
    e(25, 129),
    e(33, 103),
    e(41, 85),
    e(49, 87),
];
const LEARNSET_ENTEI: &[LevelUpMove] = &[
    e(1, 44),
    e(1, 43),
    e(11, 52),
    e(21, 46),
    e(31, 83),
    e(41, 23),
    e(51, 53),
    e(61, 207),
    e(71, 126),
    e(81, 347),
];
const LEARNSET_ESPEON: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 39),
    e(1, 270),
    e(8, 28),
    e(16, 93),
    e(23, 98),
    e(30, 129),
    e(36, 60),
    e(42, 244),
    e(47, 94),
    e(52, 234),
];
const LEARNSET_EXEGGCUTE: &[LevelUpMove] = &[
    e(1, 140),
    e(1, 253),
    e(1, 95),
    e(7, 115),
    e(13, 73),
    e(19, 93),
    e(25, 78),
    e(31, 77),
    e(37, 79),
    e(43, 76),
];
const LEARNSET_EXEGGUTOR: &[LevelUpMove] = &[e(1, 140), e(1, 95), e(1, 93), e(19, 23), e(31, 121)];
const LEARNSET_EXPLOUD: &[LevelUpMove] = &[
    e(1, 1),
    e(1, 253),
    e(1, 310),
    e(1, 336),
    e(5, 253),
    e(11, 310),
    e(15, 336),
    e(23, 48),
    e(29, 23),
    e(37, 103),
    e(40, 63),
    e(45, 46),
    e(55, 156),
    e(55, 214),
    e(63, 304),
];
const LEARNSET_FARFETCHD: &[LevelUpMove] = &[
    e(1, 64),
    e(6, 28),
    e(11, 43),
    e(16, 31),
    e(21, 282),
    e(26, 210),
    e(31, 14),
    e(36, 97),
    e(41, 163),
    e(46, 206),
];
const LEARNSET_FEAROW: &[LevelUpMove] = &[
    e(1, 64),
    e(1, 45),
    e(1, 43),
    e(1, 31),
    e(7, 43),
    e(13, 31),
    e(26, 228),
    e(32, 119),
    e(40, 65),
    e(47, 97),
];
const LEARNSET_FEEBAS: &[LevelUpMove] = &[e(1, 150), e(15, 33), e(30, 175)];
const LEARNSET_FERALIGATR: &[LevelUpMove] = &[
    e(1, 10),
    e(1, 43),
    e(1, 99),
    e(1, 55),
    e(7, 99),
    e(13, 55),
    e(21, 44),
    e(28, 184),
    e(38, 163),
    e(47, 103),
    e(58, 56),
];
const LEARNSET_FLAAFFY: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 45),
    e(1, 84),
    e(9, 84),
    e(18, 86),
    e(27, 178),
    e(36, 113),
    e(45, 87),
];
const LEARNSET_FLAREON: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 39),
    e(1, 270),
    e(8, 28),
    e(16, 52),
    e(23, 98),
    e(30, 44),
    e(36, 83),
    e(42, 123),
    e(47, 43),
    e(52, 53),
];
const LEARNSET_FLYGON: &[LevelUpMove] = &[
    e(1, 44),
    e(1, 28),
    e(1, 185),
    e(1, 328),
    e(9, 28),
    e(17, 185),
    e(25, 328),
    e(33, 242),
    e(35, 225),
    e(41, 103),
    e(53, 201),
    e(65, 63),
];
const LEARNSET_FORRETRESS: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 182),
    e(1, 120),
    e(8, 120),
    e(15, 36),
    e(22, 229),
    e(29, 117),
    e(39, 153),
    e(49, 191),
    e(59, 38),
];
const LEARNSET_FURRET: &[LevelUpMove] = &[
    e(1, 10),
    e(1, 111),
    e(1, 98),
    e(4, 111),
    e(7, 98),
    e(12, 154),
    e(19, 270),
    e(28, 21),
    e(37, 266),
    e(48, 156),
    e(59, 133),
];
const LEARNSET_GARDEVOIR: &[LevelUpMove] = &[
    e(1, 45),
    e(1, 93),
    e(1, 104),
    e(1, 100),
    e(6, 93),
    e(11, 104),
    e(16, 100),
    e(21, 347),
    e(26, 94),
    e(33, 286),
    e(42, 248),
    e(51, 95),
    e(60, 138),
];
const LEARNSET_GASTLY: &[LevelUpMove] = &[
    e(1, 95),
    e(1, 122),
    e(8, 180),
    e(13, 212),
    e(16, 174),
    e(21, 101),
    e(28, 109),
    e(33, 138),
    e(36, 194),
];
const LEARNSET_GENGAR: &[LevelUpMove] = &[
    e(1, 95),
    e(1, 122),
    e(1, 180),
    e(8, 180),
    e(13, 212),
    e(16, 174),
    e(21, 101),
    e(25, 325),
    e(31, 109),
    e(39, 138),
    e(48, 194),
];
const LEARNSET_GEODUDE: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 111),
    e(6, 300),
    e(11, 88),
    e(16, 222),
    e(21, 120),
    e(26, 205),
    e(31, 350),
    e(36, 89),
    e(41, 153),
    e(46, 38),
];
const LEARNSET_GIRAFARIG: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 45),
    e(7, 310),
    e(13, 93),
    e(19, 23),
    e(25, 316),
    e(31, 97),
    e(37, 226),
    e(43, 60),
    e(49, 242),
];
const LEARNSET_GLALIE: &[LevelUpMove] = &[
    e(1, 181),
    e(1, 43),
    e(1, 104),
    e(1, 44),
    e(7, 104),
    e(10, 44),
    e(16, 196),
    e(19, 29),
    e(25, 182),
    e(28, 242),
    e(34, 58),
    e(42, 258),
    e(53, 59),
    e(61, 329),
];
const LEARNSET_GLIGAR: &[LevelUpMove] = &[
    e(1, 40),
    e(6, 28),
    e(13, 106),
    e(20, 98),
    e(28, 185),
    e(36, 163),
    e(44, 103),
    e(52, 12),
];
const LEARNSET_GLOOM: &[LevelUpMove] = &[
    e(1, 71),
    e(1, 230),
    e(1, 77),
    e(7, 230),
    e(14, 77),
    e(16, 78),
    e(18, 79),
    e(24, 51),
    e(35, 236),
    e(44, 80),
];
const LEARNSET_GOLBAT: &[LevelUpMove] = &[
    e(1, 103),
    e(1, 141),
    e(1, 48),
    e(1, 310),
    e(6, 48),
    e(11, 310),
    e(16, 44),
    e(21, 17),
    e(28, 109),
    e(35, 314),
    e(42, 212),
    e(49, 305),
    e(56, 114),
];
const LEARNSET_GOLDEEN: &[LevelUpMove] = &[
    e(1, 64),
    e(1, 39),
    e(1, 346),
    e(10, 48),
    e(15, 30),
    e(24, 175),
    e(29, 31),
    e(38, 127),
    e(43, 32),
    e(52, 97),
];
const LEARNSET_GOLDUCK: &[LevelUpMove] = &[
    e(1, 346),
    e(1, 10),
    e(1, 39),
    e(1, 50),
    e(5, 39),
    e(10, 50),
    e(16, 93),
    e(23, 103),
    e(31, 244),
    e(44, 154),
    e(58, 56),
];
const LEARNSET_GOLEM: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 111),
    e(1, 300),
    e(1, 88),
    e(6, 300),
    e(11, 88),
    e(16, 222),
    e(21, 120),
    e(29, 205),
    e(37, 350),
    e(45, 89),
    e(53, 153),
    e(62, 38),
];
const LEARNSET_GOREBYSS: &[LevelUpMove] = &[
    e(1, 250),
    e(8, 93),
    e(15, 97),
    e(22, 352),
    e(29, 133),
    e(36, 94),
    e(43, 226),
    e(50, 56),
];
const LEARNSET_GRANBULL: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 184),
    e(4, 39),
    e(8, 204),
    e(13, 44),
    e(19, 122),
    e(28, 46),
    e(38, 99),
    e(49, 36),
    e(61, 242),
];
const LEARNSET_GRAVELER: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 111),
    e(1, 300),
    e(1, 88),
    e(6, 300),
    e(11, 88),
    e(16, 222),
    e(21, 120),
    e(29, 205),
    e(37, 350),
    e(45, 89),
    e(53, 153),
    e(62, 38),
];
const LEARNSET_GRIMER: &[LevelUpMove] = &[
    e(1, 139),
    e(1, 1),
    e(4, 106),
    e(8, 50),
    e(13, 124),
    e(19, 107),
    e(26, 103),
    e(34, 151),
    e(43, 188),
    e(53, 262),
];
const LEARNSET_GROUDON: &[LevelUpMove] = &[
    e(1, 341),
    e(5, 184),
    e(15, 246),
    e(20, 163),
    e(30, 339),
    e(35, 89),
    e(45, 126),
    e(50, 156),
    e(60, 90),
    e(65, 76),
    e(75, 284),
];
const LEARNSET_GROVYLE: &[LevelUpMove] = &[
    e(1, 1),
    e(1, 43),
    e(1, 71),
    e(1, 98),
    e(6, 71),
    e(11, 98),
    e(16, 210),
    e(17, 228),
    e(23, 103),
    e(29, 348),
    e(35, 97),
    e(41, 21),
    e(47, 197),
    e(53, 206),
];
const LEARNSET_GROWLITHE: &[LevelUpMove] = &[
    e(1, 44),
    e(1, 46),
    e(7, 52),
    e(13, 43),
    e(19, 316),
    e(25, 36),
    e(31, 172),
    e(37, 270),
    e(43, 97),
    e(49, 53),
];
const LEARNSET_GRUMPIG: &[LevelUpMove] = &[
    e(1, 150),
    e(1, 149),
    e(1, 316),
    e(1, 60),
    e(7, 149),
    e(10, 316),
    e(16, 60),
    e(19, 244),
    e(25, 109),
    e(28, 277),
    e(37, 94),
    e(43, 156),
    e(43, 173),
    e(55, 340),
];
const LEARNSET_GULPIN: &[LevelUpMove] = &[
    e(1, 1),
    e(6, 281),
    e(9, 139),
    e(14, 124),
    e(17, 133),
    e(23, 227),
    e(28, 92),
    e(34, 254),
    e(34, 255),
    e(34, 256),
    e(39, 188),
];
const LEARNSET_GYARADOS: &[LevelUpMove] = &[
    e(1, 37),
    e(20, 44),
    e(25, 82),
    e(30, 43),
    e(35, 239),
    e(40, 56),
    e(45, 240),
    e(50, 349),
    e(55, 63),
];
const LEARNSET_HARIYAMA: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 116),
    e(1, 28),
    e(1, 292),
    e(4, 28),
    e(10, 292),
    e(13, 233),
    e(19, 252),
    e(22, 18),
    e(29, 282),
    e(33, 265),
    e(40, 187),
    e(44, 203),
    e(51, 69),
    e(55, 179),
];
const LEARNSET_HAUNTER: &[LevelUpMove] = &[
    e(1, 95),
    e(1, 122),
    e(1, 180),
    e(8, 180),
    e(13, 212),
    e(16, 174),
    e(21, 101),
    e(25, 325),
    e(31, 109),
    e(39, 138),
    e(48, 194),
];
const LEARNSET_HERACROSS: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 43),
    e(6, 30),
    e(11, 203),
    e(17, 31),
    e(23, 280),
    e(30, 68),
    e(37, 36),
    e(45, 179),
    e(53, 224),
];
const LEARNSET_HITMONCHAN: &[LevelUpMove] = &[
    e(1, 279),
    e(1, 4),
    e(7, 97),
    e(13, 228),
    e(20, 183),
    e(26, 9),
    e(26, 8),
    e(26, 7),
    e(32, 327),
    e(38, 5),
    e(44, 197),
    e(50, 68),
];
const LEARNSET_HITMONLEE: &[LevelUpMove] = &[
    e(1, 279),
    e(1, 24),
    e(6, 96),
    e(11, 27),
    e(16, 26),
    e(20, 280),
    e(21, 116),
    e(26, 136),
    e(31, 170),
    e(36, 193),
    e(41, 203),
    e(46, 25),
    e(51, 179),
];
const LEARNSET_HITMONTOP: &[LevelUpMove] = &[
    e(1, 279),
    e(1, 27),
    e(7, 116),
    e(13, 228),
    e(19, 98),
    e(20, 167),
    e(25, 229),
    e(31, 68),
    e(37, 97),
    e(43, 197),
    e(49, 283),
];
const LEARNSET_HO_OH: &[LevelUpMove] = &[
    e(1, 18),
    e(11, 219),
    e(22, 16),
    e(33, 105),
    e(44, 126),
    e(55, 241),
    e(66, 129),
    e(77, 221),
    e(88, 246),
    e(99, 248),
];
const LEARNSET_HOOTHOOT: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 45),
    e(6, 193),
    e(11, 64),
    e(16, 95),
    e(22, 115),
    e(28, 36),
    e(34, 93),
    e(48, 138),
];
const LEARNSET_HOPPIP: &[LevelUpMove] = &[
    e(1, 150),
    e(5, 235),
    e(5, 39),
    e(10, 33),
    e(13, 77),
    e(15, 78),
    e(17, 79),
    e(20, 73),
    e(25, 178),
    e(30, 72),
];
const LEARNSET_HORSEA: &[LevelUpMove] = &[
    e(1, 145),
    e(8, 108),
    e(15, 43),
    e(22, 55),
    e(29, 239),
    e(36, 97),
    e(43, 56),
    e(50, 349),
];
const LEARNSET_HOUNDOOM: &[LevelUpMove] = &[
    e(1, 43),
    e(1, 52),
    e(1, 336),
    e(7, 336),
    e(13, 123),
    e(19, 46),
    e(27, 44),
    e(35, 316),
    e(43, 185),
    e(51, 53),
    e(59, 242),
];
const LEARNSET_HOUNDOUR: &[LevelUpMove] = &[
    e(1, 43),
    e(1, 52),
    e(7, 336),
    e(13, 123),
    e(19, 46),
    e(25, 44),
    e(31, 316),
    e(37, 185),
    e(43, 53),
    e(49, 242),
];
const LEARNSET_HUNTAIL: &[LevelUpMove] = &[
    e(1, 250),
    e(8, 44),
    e(15, 103),
    e(22, 352),
    e(29, 184),
    e(36, 242),
    e(43, 226),
    e(50, 56),
];
const LEARNSET_HYPNO: &[LevelUpMove] = &[
    e(1, 1),
    e(1, 95),
    e(1, 50),
    e(1, 93),
    e(10, 50),
    e(18, 93),
    e(25, 29),
    e(33, 139),
    e(40, 96),
    e(49, 94),
    e(55, 244),
    e(60, 248),
];
const LEARNSET_IGGLYBUFF: &[LevelUpMove] = &[e(1, 47), e(1, 204), e(4, 111), e(9, 1), e(14, 186)];
const LEARNSET_ILLUMISE: &[LevelUpMove] = &[
    e(1, 33),
    e(5, 230),
    e(9, 204),
    e(13, 236),
    e(17, 98),
    e(21, 273),
    e(25, 227),
    e(29, 260),
    e(33, 270),
    e(37, 343),
];
const LEARNSET_IVYSAUR: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 45),
    e(1, 73),
    e(4, 45),
    e(7, 73),
    e(10, 22),
    e(15, 77),
    e(15, 79),
    e(22, 75),
    e(29, 230),
    e(38, 74),
    e(47, 235),
    e(56, 76),
];
const LEARNSET_JIGGLYPUFF: &[LevelUpMove] = &[
    e(1, 47),
    e(4, 111),
    e(9, 1),
    e(14, 50),
    e(19, 205),
    e(24, 3),
    e(29, 156),
    e(34, 34),
    e(39, 102),
    e(44, 304),
    e(49, 38),
];
const LEARNSET_JIRACHI: &[LevelUpMove] = &[
    e(1, 273),
    e(1, 93),
    e(5, 156),
    e(10, 129),
    e(15, 270),
    e(20, 94),
    e(25, 287),
    e(30, 156),
    e(35, 38),
    e(40, 248),
    e(45, 322),
    e(50, 353),
];
const LEARNSET_JOLTEON: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 39),
    e(1, 270),
    e(8, 28),
    e(16, 84),
    e(23, 98),
    e(30, 24),
    e(36, 42),
    e(42, 86),
    e(47, 97),
    e(52, 87),
];
const LEARNSET_JUMPLUFF: &[LevelUpMove] = &[
    e(1, 150),
    e(1, 235),
    e(1, 39),
    e(1, 33),
    e(5, 235),
    e(5, 39),
    e(10, 33),
    e(13, 77),
    e(15, 78),
    e(17, 79),
    e(22, 73),
    e(33, 178),
    e(44, 72),
];
const LEARNSET_JYNX: &[LevelUpMove] = &[
    e(1, 1),
    e(1, 122),
    e(1, 142),
    e(1, 181),
    e(9, 142),
    e(13, 181),
    e(21, 3),
    e(25, 8),
    e(35, 212),
    e(41, 313),
    e(51, 34),
    e(57, 195),
    e(67, 59),
];
const LEARNSET_KABUTO: &[LevelUpMove] = &[
    e(1, 10),
    e(1, 106),
    e(13, 71),
    e(19, 43),
    e(25, 341),
    e(31, 28),
    e(37, 203),
    e(43, 319),
    e(49, 72),
    e(55, 246),
];
const LEARNSET_KABUTOPS: &[LevelUpMove] = &[
    e(1, 10),
    e(1, 106),
    e(1, 71),
    e(13, 71),
    e(19, 43),
    e(25, 341),
    e(31, 28),
    e(37, 203),
    e(40, 163),
    e(46, 319),
    e(55, 72),
    e(65, 246),
];
const LEARNSET_KADABRA: &[LevelUpMove] = &[
    e(1, 100),
    e(1, 134),
    e(1, 93),
    e(16, 93),
    e(18, 50),
    e(21, 60),
    e(23, 115),
    e(25, 105),
    e(30, 248),
    e(33, 272),
    e(36, 94),
    e(43, 271),
];
const LEARNSET_KAKUNA: &[LevelUpMove] = &[e(1, 106), e(7, 106)];
const LEARNSET_KANGASKHAN: &[LevelUpMove] = &[
    e(1, 4),
    e(1, 43),
    e(7, 44),
    e(13, 39),
    e(19, 252),
    e(25, 5),
    e(31, 99),
    e(37, 203),
    e(43, 146),
    e(49, 179),
];
const LEARNSET_KECLEON: &[LevelUpMove] = &[
    e(1, 168),
    e(1, 39),
    e(1, 310),
    e(1, 122),
    e(1, 10),
    e(4, 20),
    e(7, 185),
    e(12, 154),
    e(17, 60),
    e(24, 103),
    e(31, 163),
    e(40, 164),
    e(49, 246),
];
const LEARNSET_KINGDRA: &[LevelUpMove] = &[
    e(1, 145),
    e(1, 108),
    e(1, 43),
    e(1, 55),
    e(8, 108),
    e(15, 43),
    e(22, 55),
    e(29, 239),
    e(40, 97),
    e(51, 56),
    e(62, 349),
];
const LEARNSET_KINGLER: &[LevelUpMove] = &[
    e(1, 145),
    e(1, 43),
    e(1, 11),
    e(5, 43),
    e(12, 11),
    e(16, 106),
    e(23, 341),
    e(27, 23),
    e(38, 12),
    e(49, 182),
    e(57, 152),
];
const LEARNSET_KIRLIA: &[LevelUpMove] = &[
    e(1, 45),
    e(1, 93),
    e(1, 104),
    e(1, 100),
    e(6, 93),
    e(11, 104),
    e(16, 100),
    e(21, 347),
    e(26, 94),
    e(33, 286),
    e(40, 248),
    e(47, 95),
    e(54, 138),
];
const LEARNSET_KOFFING: &[LevelUpMove] = &[
    e(1, 139),
    e(1, 33),
    e(9, 123),
    e(17, 120),
    e(21, 124),
    e(25, 108),
    e(33, 114),
    e(41, 153),
    e(45, 194),
    e(49, 262),
];
const LEARNSET_KRABBY: &[LevelUpMove] = &[
    e(1, 145),
    e(5, 43),
    e(12, 11),
    e(16, 106),
    e(23, 341),
    e(27, 23),
    e(34, 12),
    e(41, 182),
    e(45, 152),
];
const LEARNSET_KYOGRE: &[LevelUpMove] = &[
    e(1, 352),
    e(5, 184),
    e(15, 246),
    e(20, 34),
    e(30, 347),
    e(35, 58),
    e(45, 56),
    e(50, 156),
    e(60, 329),
    e(65, 38),
    e(75, 323),
];
const LEARNSET_LAIRON: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 106),
    e(1, 189),
    e(1, 29),
    e(4, 106),
    e(7, 189),
    e(10, 29),
    e(13, 232),
    e(17, 334),
    e(21, 46),
    e(25, 36),
    e(29, 231),
    e(37, 182),
    e(45, 319),
    e(53, 38),
];
const LEARNSET_LANTURN: &[LevelUpMove] = &[
    e(1, 145),
    e(1, 86),
    e(1, 48),
    e(5, 48),
    e(13, 175),
    e(17, 55),
    e(25, 209),
    e(32, 109),
    e(43, 36),
    e(50, 56),
    e(61, 268),
];
const LEARNSET_LAPRAS: &[LevelUpMove] = &[
    e(1, 55),
    e(1, 45),
    e(1, 47),
    e(7, 54),
    e(13, 34),
    e(19, 109),
    e(25, 195),
    e(31, 58),
    e(37, 240),
    e(43, 219),
    e(49, 56),
    e(55, 329),
];
const LEARNSET_LARVITAR: &[LevelUpMove] = &[
    e(1, 44),
    e(1, 43),
    e(8, 201),
    e(15, 103),
    e(22, 157),
    e(29, 37),
    e(36, 184),
    e(43, 242),
    e(50, 89),
    e(57, 63),
];
const LEARNSET_LATIAS: &[LevelUpMove] = &[
    e(1, 149),
    e(5, 273),
    e(10, 270),
    e(15, 219),
    e(20, 225),
    e(25, 346),
    e(30, 287),
    e(35, 296),
    e(40, 94),
    e(45, 105),
    e(50, 204),
];
const LEARNSET_LATIOS: &[LevelUpMove] = &[
    e(1, 149),
    e(5, 262),
    e(10, 270),
    e(15, 219),
    e(20, 225),
    e(25, 182),
    e(30, 287),
    e(35, 295),
    e(40, 94),
    e(45, 105),
    e(50, 349),
];
const LEARNSET_LEDIAN: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 48),
    e(8, 48),
    e(15, 4),
    e(24, 113),
    e(24, 115),
    e(24, 219),
    e(33, 226),
    e(42, 129),
    e(51, 97),
    e(60, 38),
];
const LEARNSET_LEDYBA: &[LevelUpMove] = &[
    e(1, 33),
    e(8, 48),
    e(15, 4),
    e(22, 113),
    e(22, 115),
    e(22, 219),
    e(29, 226),
    e(36, 129),
    e(43, 97),
    e(50, 38),
];
const LEARNSET_LICKITUNG: &[LevelUpMove] = &[
    e(1, 122),
    e(7, 48),
    e(12, 111),
    e(18, 282),
    e(23, 23),
    e(29, 35),
    e(34, 50),
    e(40, 21),
    e(45, 103),
    e(51, 287),
];
const LEARNSET_LILEEP: &[LevelUpMove] = &[
    e(1, 310),
    e(8, 132),
    e(15, 51),
    e(22, 275),
    e(29, 109),
    e(36, 133),
    e(43, 246),
    e(50, 254),
    e(50, 255),
    e(50, 256),
];
const LEARNSET_LINOONE: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 45),
    e(1, 39),
    e(1, 29),
    e(5, 39),
    e(9, 29),
    e(13, 28),
    e(17, 316),
    e(23, 300),
    e(29, 154),
    e(35, 343),
    e(41, 163),
    e(47, 156),
    e(53, 187),
];
const LEARNSET_LOMBRE: &[LevelUpMove] = &[
    e(1, 310),
    e(3, 45),
    e(7, 71),
    e(13, 267),
    e(19, 252),
    e(25, 154),
    e(31, 346),
    e(37, 168),
    e(43, 253),
    e(49, 56),
];
const LEARNSET_LOTAD: &[LevelUpMove] = &[
    e(1, 310),
    e(3, 45),
    e(7, 71),
    e(13, 267),
    e(21, 54),
    e(31, 240),
    e(43, 72),
];
const LEARNSET_LOUDRED: &[LevelUpMove] = &[
    e(1, 1),
    e(1, 253),
    e(1, 310),
    e(1, 336),
    e(5, 253),
    e(11, 310),
    e(15, 336),
    e(23, 48),
    e(29, 23),
    e(37, 103),
    e(43, 46),
    e(51, 156),
    e(51, 214),
    e(57, 304),
];
const LEARNSET_LUDICOLO: &[LevelUpMove] = &[e(1, 310), e(1, 45), e(1, 71), e(1, 267)];
const LEARNSET_LUGIA: &[LevelUpMove] = &[
    e(1, 18),
    e(11, 219),
    e(22, 16),
    e(33, 105),
    e(44, 56),
    e(55, 240),
    e(66, 129),
    e(77, 177),
    e(88, 246),
    e(99, 248),
];
const LEARNSET_LUNATONE: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 106),
    e(7, 93),
    e(13, 88),
    e(19, 95),
    e(25, 149),
    e(31, 322),
    e(37, 94),
    e(43, 248),
    e(49, 153),
];
const LEARNSET_LUVDISC: &[LevelUpMove] = &[
    e(1, 33),
    e(4, 204),
    e(12, 55),
    e(16, 97),
    e(24, 36),
    e(28, 213),
    e(36, 186),
    e(40, 175),
    e(48, 219),
];
const LEARNSET_MACHAMP: &[LevelUpMove] = &[
    e(1, 67),
    e(1, 43),
    e(1, 116),
    e(7, 116),
    e(13, 2),
    e(19, 69),
    e(22, 193),
    e(25, 279),
    e(33, 233),
    e(41, 66),
    e(46, 238),
    e(51, 184),
    e(59, 223),
];
const LEARNSET_MACHOKE: &[LevelUpMove] = &[
    e(1, 67),
    e(1, 43),
    e(1, 116),
    e(7, 116),
    e(13, 2),
    e(19, 69),
    e(22, 193),
    e(25, 279),
    e(33, 233),
    e(41, 66),
    e(46, 238),
    e(51, 184),
    e(59, 223),
];
const LEARNSET_MACHOP: &[LevelUpMove] = &[
    e(1, 67),
    e(1, 43),
    e(7, 116),
    e(13, 2),
    e(19, 69),
    e(22, 193),
    e(25, 279),
    e(31, 233),
    e(37, 66),
    e(40, 238),
    e(43, 184),
    e(49, 223),
];
const LEARNSET_MAGBY: &[LevelUpMove] = &[
    e(1, 52),
    e(7, 43),
    e(13, 123),
    e(19, 7),
    e(25, 108),
    e(31, 241),
    e(37, 53),
    e(43, 109),
    e(49, 126),
];
const LEARNSET_MAGCARGO: &[LevelUpMove] = &[
    e(1, 281),
    e(1, 123),
    e(1, 52),
    e(1, 88),
    e(8, 52),
    e(15, 88),
    e(22, 106),
    e(29, 133),
    e(36, 53),
    e(48, 157),
    e(60, 34),
];
const LEARNSET_MAGIKARP: &[LevelUpMove] = &[e(1, 150), e(15, 33), e(30, 175)];
const LEARNSET_MAGMAR: &[LevelUpMove] = &[
    e(1, 52),
    e(1, 43),
    e(1, 123),
    e(1, 7),
    e(7, 43),
    e(13, 123),
    e(19, 7),
    e(25, 108),
    e(33, 241),
    e(41, 53),
    e(49, 109),
    e(57, 126),
];
const LEARNSET_MAGNEMITE: &[LevelUpMove] = &[
    e(1, 319),
    e(1, 33),
    e(6, 84),
    e(11, 48),
    e(16, 49),
    e(21, 86),
    e(26, 209),
    e(32, 199),
    e(38, 129),
    e(44, 103),
    e(50, 192),
];
const LEARNSET_MAGNETON: &[LevelUpMove] = &[
    e(1, 319),
    e(1, 33),
    e(1, 84),
    e(1, 48),
    e(6, 84),
    e(11, 48),
    e(16, 49),
    e(21, 86),
    e(26, 209),
    e(35, 199),
    e(44, 161),
    e(53, 103),
    e(62, 192),
];
const LEARNSET_MAKUHITA: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 116),
    e(4, 28),
    e(10, 292),
    e(13, 233),
    e(19, 252),
    e(22, 18),
    e(28, 282),
    e(31, 265),
    e(37, 187),
    e(40, 203),
    e(46, 69),
    e(49, 179),
];
const LEARNSET_MANECTRIC: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 86),
    e(1, 43),
    e(1, 336),
    e(4, 86),
    e(9, 43),
    e(12, 336),
    e(17, 98),
    e(20, 209),
    e(25, 316),
    e(31, 46),
    e(39, 44),
    e(45, 87),
    e(53, 268),
];
const LEARNSET_MANKEY: &[LevelUpMove] = &[
    e(1, 10),
    e(1, 43),
    e(9, 67),
    e(15, 2),
    e(21, 154),
    e(27, 116),
    e(33, 69),
    e(39, 238),
    e(45, 103),
    e(51, 37),
];
const LEARNSET_MANTINE: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 145),
    e(8, 48),
    e(15, 61),
    e(22, 36),
    e(29, 97),
    e(36, 17),
    e(43, 352),
    e(50, 109),
];
const LEARNSET_MAREEP: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 45),
    e(9, 84),
    e(16, 86),
    e(23, 178),
    e(30, 113),
    e(37, 87),
];
const LEARNSET_MARILL: &[LevelUpMove] = &[
    e(1, 33),
    e(3, 111),
    e(6, 39),
    e(10, 55),
    e(15, 205),
    e(21, 61),
    e(28, 38),
    e(36, 240),
    e(45, 56),
];
const LEARNSET_MAROWAK: &[LevelUpMove] = &[
    e(1, 45),
    e(1, 39),
    e(1, 125),
    e(1, 29),
    e(5, 39),
    e(9, 125),
    e(13, 29),
    e(17, 43),
    e(21, 116),
    e(25, 155),
    e(32, 99),
    e(39, 206),
    e(46, 37),
    e(53, 198),
    e(61, 38),
];
const LEARNSET_MARSHTOMP: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 45),
    e(1, 189),
    e(1, 55),
    e(6, 189),
    e(10, 55),
    e(15, 117),
    e(16, 341),
    e(20, 193),
    e(25, 300),
    e(31, 36),
    e(37, 330),
    e(42, 182),
    e(46, 89),
    e(53, 283),
];
const LEARNSET_MASQUERAIN: &[LevelUpMove] = &[
    e(1, 145),
    e(1, 98),
    e(1, 230),
    e(1, 346),
    e(7, 98),
    e(13, 230),
    e(19, 346),
    e(26, 16),
    e(33, 184),
    e(40, 78),
    e(47, 318),
    e(53, 18),
];
const LEARNSET_MAWILE: &[LevelUpMove] = &[
    e(1, 310),
    e(6, 313),
    e(11, 44),
    e(16, 230),
    e(21, 11),
    e(26, 185),
    e(31, 226),
    e(36, 242),
    e(41, 334),
    e(46, 254),
    e(46, 256),
    e(46, 255),
];
const LEARNSET_MEDICHAM: &[LevelUpMove] = &[
    e(1, 7),
    e(1, 9),
    e(1, 8),
    e(1, 117),
    e(1, 96),
    e(1, 93),
    e(1, 197),
    e(4, 96),
    e(9, 93),
    e(12, 197),
    e(18, 237),
    e(22, 170),
    e(28, 347),
    e(32, 136),
    e(40, 244),
    e(46, 179),
    e(54, 105),
];
const LEARNSET_MEDITITE: &[LevelUpMove] = &[
    e(1, 117),
    e(4, 96),
    e(9, 93),
    e(12, 197),
    e(18, 237),
    e(22, 170),
    e(28, 347),
    e(32, 136),
    e(38, 244),
    e(42, 179),
    e(48, 105),
];
const LEARNSET_MEGANIUM: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 45),
    e(1, 75),
    e(1, 115),
    e(8, 75),
    e(12, 115),
    e(15, 77),
    e(23, 235),
    e(31, 34),
    e(41, 113),
    e(51, 219),
    e(61, 76),
];
const LEARNSET_MEOWTH: &[LevelUpMove] = &[
    e(1, 10),
    e(1, 45),
    e(11, 44),
    e(20, 6),
    e(28, 185),
    e(35, 103),
    e(41, 154),
    e(46, 163),
    e(50, 252),
];
const LEARNSET_METAGROSS: &[LevelUpMove] = &[
    e(1, 36),
    e(1, 93),
    e(1, 232),
    e(1, 184),
    e(20, 93),
    e(20, 232),
    e(26, 184),
    e(32, 228),
    e(38, 94),
    e(44, 334),
    e(55, 309),
    e(66, 97),
    e(77, 63),
];
const LEARNSET_METANG: &[LevelUpMove] = &[
    e(1, 36),
    e(20, 93),
    e(20, 232),
    e(26, 184),
    e(32, 228),
    e(38, 94),
    e(44, 334),
    e(50, 309),
    e(56, 97),
    e(62, 63),
];
const LEARNSET_METAPOD: &[LevelUpMove] = &[e(1, 106), e(7, 106)];
const LEARNSET_MEW: &[LevelUpMove] = &[
    e(1, 1),
    e(10, 144),
    e(20, 5),
    e(30, 118),
    e(40, 94),
    e(50, 246),
];
const LEARNSET_MEWTWO: &[LevelUpMove] = &[
    e(1, 93),
    e(1, 50),
    e(11, 112),
    e(22, 129),
    e(33, 244),
    e(44, 248),
    e(55, 54),
    e(66, 94),
    e(77, 133),
    e(88, 105),
    e(99, 219),
];
const LEARNSET_MIGHTYENA: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 336),
    e(1, 28),
    e(1, 44),
    e(5, 336),
    e(9, 28),
    e(13, 44),
    e(17, 316),
    e(22, 46),
    e(27, 207),
    e(32, 184),
    e(37, 36),
    e(42, 269),
    e(47, 242),
    e(52, 168),
];
const LEARNSET_MILOTIC: &[LevelUpMove] = &[
    e(1, 55),
    e(5, 35),
    e(10, 346),
    e(15, 287),
    e(20, 352),
    e(25, 239),
    e(30, 105),
    e(35, 240),
    e(40, 56),
    e(45, 213),
    e(50, 219),
];
const LEARNSET_MILTANK: &[LevelUpMove] = &[
    e(1, 33),
    e(4, 45),
    e(8, 111),
    e(13, 23),
    e(19, 208),
    e(26, 117),
    e(34, 205),
    e(43, 34),
    e(53, 215),
];
const LEARNSET_MINUN: &[LevelUpMove] = &[
    e(1, 45),
    e(4, 86),
    e(10, 98),
    e(13, 270),
    e(19, 209),
    e(22, 227),
    e(28, 204),
    e(31, 268),
    e(37, 87),
    e(40, 226),
    e(47, 97),
];
const LEARNSET_MISDREAVUS: &[LevelUpMove] = &[
    e(1, 45),
    e(1, 149),
    e(6, 180),
    e(11, 310),
    e(17, 109),
    e(23, 212),
    e(30, 60),
    e(37, 220),
    e(45, 195),
    e(53, 288),
];
const LEARNSET_MOLTRES: &[LevelUpMove] = &[
    e(1, 17),
    e(1, 52),
    e(13, 83),
    e(25, 97),
    e(37, 203),
    e(49, 53),
    e(61, 219),
    e(73, 257),
    e(85, 143),
];
const LEARNSET_MR_MIME: &[LevelUpMove] = &[
    e(1, 112),
    e(5, 93),
    e(9, 164),
    e(13, 96),
    e(17, 3),
    e(21, 113),
    e(21, 115),
    e(25, 227),
    e(29, 60),
    e(33, 278),
    e(37, 271),
    e(41, 272),
    e(45, 94),
    e(49, 226),
    e(53, 219),
];
const LEARNSET_MUDKIP: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 45),
    e(6, 189),
    e(10, 55),
    e(15, 117),
    e(19, 193),
    e(24, 300),
    e(28, 36),
    e(33, 250),
    e(37, 182),
    e(42, 56),
    e(46, 283),
];
const LEARNSET_MUK: &[LevelUpMove] = &[
    e(1, 139),
    e(1, 1),
    e(1, 106),
    e(4, 106),
    e(8, 50),
    e(13, 124),
    e(19, 107),
    e(26, 103),
    e(34, 151),
    e(47, 188),
    e(61, 262),
];
const LEARNSET_MURKROW: &[LevelUpMove] = &[
    e(1, 64),
    e(9, 310),
    e(14, 228),
    e(22, 114),
    e(27, 101),
    e(35, 185),
    e(40, 269),
    e(48, 212),
];
const LEARNSET_NATU: &[LevelUpMove] = &[
    e(1, 64),
    e(1, 43),
    e(10, 101),
    e(20, 100),
    e(30, 273),
    e(30, 248),
    e(40, 109),
    e(50, 94),
];
const LEARNSET_NIDOKING: &[LevelUpMove] = &[e(1, 64), e(1, 116), e(1, 24), e(1, 40), e(23, 37)];
const LEARNSET_NIDOQUEEN: &[LevelUpMove] = &[e(1, 10), e(1, 39), e(1, 24), e(1, 40), e(23, 34)];
const LEARNSET_NIDORAN_F: &[LevelUpMove] = &[
    e(1, 45),
    e(1, 10),
    e(8, 39),
    e(12, 24),
    e(17, 40),
    e(20, 44),
    e(23, 270),
    e(30, 154),
    e(38, 260),
    e(47, 242),
];
const LEARNSET_NIDORAN_M: &[LevelUpMove] = &[
    e(1, 43),
    e(1, 64),
    e(8, 116),
    e(12, 24),
    e(17, 40),
    e(20, 30),
    e(23, 270),
    e(30, 31),
    e(38, 260),
    e(47, 32),
];
const LEARNSET_NIDORINA: &[LevelUpMove] = &[
    e(1, 45),
    e(1, 10),
    e(8, 39),
    e(12, 24),
    e(18, 40),
    e(22, 44),
    e(26, 270),
    e(34, 154),
    e(43, 260),
    e(53, 242),
];
const LEARNSET_NIDORINO: &[LevelUpMove] = &[
    e(1, 43),
    e(1, 64),
    e(8, 116),
    e(12, 24),
    e(18, 40),
    e(22, 30),
    e(26, 270),
    e(34, 31),
    e(43, 260),
    e(53, 32),
];
const LEARNSET_NINCADA: &[LevelUpMove] = &[
    e(1, 10),
    e(1, 106),
    e(5, 141),
    e(9, 28),
    e(14, 154),
    e(19, 170),
    e(25, 206),
    e(31, 189),
    e(38, 232),
    e(45, 91),
];
const LEARNSET_NINETALES: &[LevelUpMove] = &[e(1, 52), e(1, 98), e(1, 109), e(1, 219), e(45, 83)];
const LEARNSET_NINJASK: &[LevelUpMove] = &[
    e(1, 10),
    e(1, 106),
    e(1, 141),
    e(1, 28),
    e(5, 141),
    e(9, 28),
    e(14, 154),
    e(19, 170),
    e(20, 104),
    e(20, 210),
    e(20, 103),
    e(25, 14),
    e(31, 163),
    e(38, 97),
    e(45, 226),
];
const LEARNSET_NOCTOWL: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 45),
    e(1, 193),
    e(1, 64),
    e(6, 193),
    e(11, 64),
    e(16, 95),
    e(25, 115),
    e(33, 36),
    e(41, 93),
    e(57, 138),
];
const LEARNSET_NOSEPASS: &[LevelUpMove] = &[
    e(1, 33),
    e(7, 106),
    e(13, 88),
    e(16, 335),
    e(22, 86),
    e(28, 157),
    e(31, 201),
    e(37, 156),
    e(43, 192),
    e(46, 199),
];
const LEARNSET_NUMEL: &[LevelUpMove] = &[
    e(1, 45),
    e(1, 33),
    e(11, 52),
    e(19, 222),
    e(25, 116),
    e(29, 36),
    e(31, 133),
    e(35, 89),
    e(41, 53),
    e(49, 38),
];
const LEARNSET_NUZLEAF: &[LevelUpMove] = &[
    e(1, 1),
    e(3, 106),
    e(7, 74),
    e(13, 267),
    e(19, 252),
    e(25, 259),
    e(31, 185),
    e(37, 13),
    e(43, 207),
    e(49, 326),
];
const LEARNSET_OCTILLERY: &[LevelUpMove] = &[
    e(1, 55),
    e(11, 132),
    e(22, 60),
    e(22, 62),
    e(22, 61),
    e(25, 190),
    e(38, 116),
    e(54, 58),
    e(70, 63),
];
const LEARNSET_ODDISH: &[LevelUpMove] = &[
    e(1, 71),
    e(7, 230),
    e(14, 77),
    e(16, 78),
    e(18, 79),
    e(23, 51),
    e(32, 236),
    e(39, 80),
];
const LEARNSET_OMANYTE: &[LevelUpMove] = &[
    e(1, 132),
    e(1, 110),
    e(13, 44),
    e(19, 55),
    e(25, 341),
    e(31, 43),
    e(37, 182),
    e(43, 321),
    e(49, 246),
    e(55, 56),
];
const LEARNSET_OMASTAR: &[LevelUpMove] = &[
    e(1, 132),
    e(1, 110),
    e(1, 44),
    e(13, 44),
    e(19, 55),
    e(25, 341),
    e(31, 43),
    e(37, 182),
    e(40, 131),
    e(46, 321),
    e(55, 246),
    e(65, 56),
];
const LEARNSET_ONIX: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 103),
    e(9, 20),
    e(13, 88),
    e(21, 106),
    e(25, 99),
    e(33, 201),
    e(37, 21),
    e(45, 231),
    e(49, 328),
    e(57, 38),
];
const LEARNSET_PARAS: &[LevelUpMove] = &[
    e(1, 10),
    e(7, 78),
    e(13, 77),
    e(19, 141),
    e(25, 147),
    e(31, 163),
    e(37, 74),
    e(43, 202),
    e(49, 312),
];
const LEARNSET_PARASECT: &[LevelUpMove] = &[
    e(1, 10),
    e(1, 78),
    e(1, 77),
    e(7, 78),
    e(13, 77),
    e(19, 141),
    e(27, 147),
    e(35, 163),
    e(43, 74),
    e(51, 202),
    e(59, 312),
];
const LEARNSET_PELIPPER: &[LevelUpMove] = &[
    e(1, 45),
    e(1, 55),
    e(1, 346),
    e(1, 17),
    e(3, 55),
    e(7, 48),
    e(13, 17),
    e(21, 54),
    e(25, 182),
    e(33, 254),
    e(33, 256),
    e(47, 255),
    e(61, 56),
];
const LEARNSET_PERSIAN: &[LevelUpMove] = &[
    e(1, 10),
    e(1, 45),
    e(1, 44),
    e(11, 44),
    e(20, 6),
    e(29, 185),
    e(38, 103),
    e(46, 154),
    e(53, 163),
    e(59, 252),
];
const LEARNSET_PHANPY: &[LevelUpMove] = &[
    e(1, 316),
    e(1, 33),
    e(1, 45),
    e(9, 111),
    e(17, 175),
    e(25, 36),
    e(33, 205),
    e(41, 203),
    e(49, 38),
];
const LEARNSET_PICHU: &[LevelUpMove] = &[e(1, 84), e(1, 204), e(6, 39), e(8, 86), e(11, 186)];
const LEARNSET_PIDGEOT: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 28),
    e(1, 16),
    e(1, 98),
    e(5, 28),
    e(9, 16),
    e(13, 98),
    e(20, 18),
    e(27, 17),
    e(34, 297),
    e(48, 97),
    e(62, 119),
];
const LEARNSET_PIDGEOTTO: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 28),
    e(1, 16),
    e(5, 28),
    e(9, 16),
    e(13, 98),
    e(20, 18),
    e(27, 17),
    e(34, 297),
    e(43, 97),
    e(52, 119),
];
const LEARNSET_PIDGEY: &[LevelUpMove] = &[
    e(1, 33),
    e(5, 28),
    e(9, 16),
    e(13, 98),
    e(19, 18),
    e(25, 17),
    e(31, 297),
    e(39, 97),
    e(47, 119),
];
const LEARNSET_PIKACHU: &[LevelUpMove] = &[
    e(1, 84),
    e(1, 45),
    e(6, 39),
    e(8, 86),
    e(11, 98),
    e(15, 104),
    e(20, 21),
    e(26, 85),
    e(33, 97),
    e(41, 87),
    e(50, 113),
];
const LEARNSET_PILOSWINE: &[LevelUpMove] = &[
    e(1, 30),
    e(1, 316),
    e(1, 181),
    e(1, 203),
    e(10, 181),
    e(19, 203),
    e(28, 36),
    e(33, 31),
    e(42, 54),
    e(56, 59),
    e(70, 133),
];
const LEARNSET_PINECO: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 182),
    e(8, 120),
    e(15, 36),
    e(22, 229),
    e(29, 117),
    e(36, 153),
    e(43, 191),
    e(50, 38),
];
const LEARNSET_PINSIR: &[LevelUpMove] = &[
    e(1, 11),
    e(1, 116),
    e(7, 20),
    e(13, 69),
    e(19, 106),
    e(25, 279),
    e(31, 280),
    e(37, 12),
    e(43, 66),
    e(49, 14),
];
const LEARNSET_PLUSLE: &[LevelUpMove] = &[
    e(1, 45),
    e(4, 86),
    e(10, 98),
    e(13, 270),
    e(19, 209),
    e(22, 227),
    e(28, 313),
    e(31, 268),
    e(37, 87),
    e(40, 226),
    e(47, 97),
];
const LEARNSET_POLITOED: &[LevelUpMove] = &[
    e(1, 55),
    e(1, 95),
    e(1, 3),
    e(1, 195),
    e(35, 195),
    e(51, 207),
];
const LEARNSET_POLIWAG: &[LevelUpMove] = &[
    e(1, 145),
    e(7, 95),
    e(13, 55),
    e(19, 3),
    e(25, 240),
    e(31, 34),
    e(37, 187),
    e(43, 56),
];
const LEARNSET_POLIWHIRL: &[LevelUpMove] = &[
    e(1, 145),
    e(1, 95),
    e(1, 55),
    e(7, 95),
    e(13, 55),
    e(19, 3),
    e(27, 240),
    e(35, 34),
    e(43, 187),
    e(51, 56),
];
const LEARNSET_POLIWRATH: &[LevelUpMove] =
    &[e(1, 55), e(1, 95), e(1, 3), e(1, 66), e(35, 66), e(51, 170)];
const LEARNSET_PONYTA: &[LevelUpMove] = &[
    e(1, 33),
    e(5, 45),
    e(9, 39),
    e(14, 52),
    e(19, 23),
    e(25, 83),
    e(31, 36),
    e(38, 97),
    e(45, 340),
    e(53, 126),
];
const LEARNSET_POOCHYENA: &[LevelUpMove] = &[
    e(1, 33),
    e(5, 336),
    e(9, 28),
    e(13, 44),
    e(17, 316),
    e(21, 46),
    e(25, 207),
    e(29, 184),
    e(33, 36),
    e(37, 269),
    e(41, 242),
    e(45, 168),
];
const LEARNSET_PORYGON2: &[LevelUpMove] = &[
    e(1, 176),
    e(1, 33),
    e(1, 160),
    e(9, 97),
    e(12, 60),
    e(20, 105),
    e(24, 111),
    e(32, 199),
    e(36, 161),
    e(44, 278),
    e(48, 192),
];
const LEARNSET_PORYGON: &[LevelUpMove] = &[
    e(1, 176),
    e(1, 33),
    e(1, 160),
    e(9, 97),
    e(12, 60),
    e(20, 105),
    e(24, 159),
    e(32, 199),
    e(36, 161),
    e(44, 278),
    e(48, 192),
];
const LEARNSET_PRIMEAPE: &[LevelUpMove] = &[
    e(1, 10),
    e(1, 43),
    e(1, 67),
    e(1, 99),
    e(9, 67),
    e(15, 2),
    e(21, 154),
    e(27, 116),
    e(28, 99),
    e(36, 69),
    e(45, 238),
    e(54, 103),
    e(63, 37),
];
const LEARNSET_PSYDUCK: &[LevelUpMove] = &[
    e(1, 346),
    e(1, 10),
    e(5, 39),
    e(10, 50),
    e(16, 93),
    e(23, 103),
    e(31, 244),
    e(40, 154),
    e(50, 56),
];
const LEARNSET_PUPITAR: &[LevelUpMove] = &[
    e(1, 44),
    e(1, 43),
    e(1, 201),
    e(1, 103),
    e(8, 201),
    e(15, 103),
    e(22, 157),
    e(29, 37),
    e(38, 184),
    e(47, 242),
    e(56, 89),
    e(65, 63),
];
const LEARNSET_QUAGSIRE: &[LevelUpMove] = &[
    e(1, 55),
    e(1, 39),
    e(11, 21),
    e(16, 341),
    e(23, 133),
    e(35, 281),
    e(42, 89),
    e(49, 240),
    e(61, 54),
    e(61, 114),
];
const LEARNSET_QUILAVA: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 43),
    e(1, 108),
    e(6, 108),
    e(12, 52),
    e(21, 98),
    e(31, 172),
    e(42, 129),
    e(54, 53),
];
const LEARNSET_QWILFISH: &[LevelUpMove] = &[
    e(1, 191),
    e(1, 33),
    e(1, 40),
    e(10, 106),
    e(10, 107),
    e(19, 55),
    e(28, 42),
    e(37, 36),
    e(46, 56),
];
const LEARNSET_RAICHU: &[LevelUpMove] = &[e(1, 84), e(1, 39), e(1, 98), e(1, 85)];
const LEARNSET_RAIKOU: &[LevelUpMove] = &[
    e(1, 44),
    e(1, 43),
    e(11, 84),
    e(21, 46),
    e(31, 98),
    e(41, 209),
    e(51, 115),
    e(61, 242),
    e(71, 87),
    e(81, 347),
];
const LEARNSET_RALTS: &[LevelUpMove] = &[
    e(1, 45),
    e(6, 93),
    e(11, 104),
    e(16, 100),
    e(21, 347),
    e(26, 94),
    e(31, 286),
    e(36, 248),
    e(41, 95),
    e(46, 138),
];
const LEARNSET_RAPIDASH: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 45),
    e(1, 39),
    e(1, 52),
    e(5, 45),
    e(9, 39),
    e(14, 52),
    e(19, 23),
    e(25, 83),
    e(31, 36),
    e(38, 97),
    e(40, 31),
    e(50, 340),
    e(63, 126),
];
const LEARNSET_RATICATE: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 39),
    e(1, 98),
    e(7, 98),
    e(13, 158),
    e(20, 184),
    e(30, 228),
    e(40, 162),
    e(50, 283),
];
const LEARNSET_RATTATA: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 39),
    e(7, 98),
    e(13, 158),
    e(20, 116),
    e(27, 228),
    e(34, 162),
    e(41, 283),
];
const LEARNSET_RAYQUAZA: &[LevelUpMove] = &[
    e(1, 239),
    e(5, 184),
    e(15, 246),
    e(20, 337),
    e(30, 349),
    e(35, 242),
    e(45, 19),
    e(50, 156),
    e(60, 245),
    e(65, 200),
    e(75, 63),
];
const LEARNSET_REGICE: &[LevelUpMove] = &[
    e(1, 153),
    e(9, 196),
    e(17, 174),
    e(25, 276),
    e(33, 246),
    e(41, 133),
    e(49, 192),
    e(57, 199),
    e(65, 63),
];
const LEARNSET_REGIROCK: &[LevelUpMove] = &[
    e(1, 153),
    e(9, 88),
    e(17, 174),
    e(25, 276),
    e(33, 246),
    e(41, 334),
    e(49, 192),
    e(57, 199),
    e(65, 63),
];
const LEARNSET_REGISTEEL: &[LevelUpMove] = &[
    e(1, 153),
    e(9, 232),
    e(17, 174),
    e(25, 276),
    e(33, 246),
    e(41, 334),
    e(41, 133),
    e(49, 192),
    e(57, 199),
    e(65, 63),
];
const LEARNSET_RELICANTH: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 106),
    e(8, 55),
    e(15, 317),
    e(22, 281),
    e(29, 36),
    e(36, 300),
    e(43, 246),
    e(50, 156),
    e(57, 38),
    e(64, 56),
];
const LEARNSET_REMORAID: &[LevelUpMove] = &[
    e(1, 55),
    e(11, 199),
    e(22, 60),
    e(22, 62),
    e(22, 61),
    e(33, 116),
    e(44, 58),
    e(55, 63),
];
const LEARNSET_RHYDON: &[LevelUpMove] = &[
    e(1, 30),
    e(1, 39),
    e(1, 23),
    e(1, 31),
    e(10, 23),
    e(15, 31),
    e(24, 184),
    e(29, 350),
    e(38, 32),
    e(46, 36),
    e(58, 89),
    e(66, 224),
];
const LEARNSET_RHYHORN: &[LevelUpMove] = &[
    e(1, 30),
    e(1, 39),
    e(10, 23),
    e(15, 31),
    e(24, 184),
    e(29, 350),
    e(38, 32),
    e(43, 36),
    e(52, 89),
    e(57, 224),
];
const LEARNSET_ROSELIA: &[LevelUpMove] = &[
    e(1, 71),
    e(5, 74),
    e(9, 40),
    e(13, 78),
    e(17, 72),
    e(21, 73),
    e(25, 345),
    e(29, 320),
    e(33, 202),
    e(37, 230),
    e(41, 275),
    e(45, 92),
    e(49, 80),
    e(53, 312),
    e(57, 235),
];
const LEARNSET_SABLEYE: &[LevelUpMove] = &[
    e(1, 43),
    e(1, 10),
    e(5, 193),
    e(9, 101),
    e(13, 310),
    e(17, 154),
    e(21, 252),
    e(25, 197),
    e(29, 185),
    e(33, 282),
    e(37, 109),
    e(41, 247),
    e(45, 212),
];
const LEARNSET_SALAMENCE: &[LevelUpMove] = &[
    e(1, 99),
    e(1, 44),
    e(1, 43),
    e(1, 29),
    e(5, 44),
    e(9, 43),
    e(17, 29),
    e(21, 116),
    e(25, 52),
    e(30, 182),
    e(38, 225),
    e(47, 184),
    e(50, 19),
    e(61, 242),
    e(79, 337),
    e(93, 38),
];
const LEARNSET_SANDSHREW: &[LevelUpMove] = &[
    e(1, 10),
    e(6, 111),
    e(11, 28),
    e(17, 40),
    e(23, 163),
    e(30, 129),
    e(37, 154),
    e(45, 328),
    e(53, 201),
];
const LEARNSET_SANDSLASH: &[LevelUpMove] = &[
    e(1, 10),
    e(1, 111),
    e(1, 28),
    e(6, 111),
    e(11, 28),
    e(17, 40),
    e(24, 163),
    e(33, 129),
    e(42, 154),
    e(52, 328),
    e(62, 201),
];
const LEARNSET_SCEPTILE: &[LevelUpMove] = &[
    e(1, 1),
    e(1, 43),
    e(1, 71),
    e(1, 98),
    e(6, 71),
    e(11, 98),
    e(16, 210),
    e(17, 228),
    e(23, 103),
    e(29, 348),
    e(35, 97),
    e(43, 21),
    e(51, 197),
    e(59, 206),
];
const LEARNSET_SCIZOR: &[LevelUpMove] = &[
    e(1, 98),
    e(1, 43),
    e(6, 116),
    e(11, 228),
    e(16, 206),
    e(21, 97),
    e(26, 232),
    e(31, 163),
    e(36, 14),
    e(41, 104),
    e(46, 210),
];
const LEARNSET_SCYTHER: &[LevelUpMove] = &[
    e(1, 98),
    e(1, 43),
    e(6, 116),
    e(11, 228),
    e(16, 206),
    e(21, 97),
    e(26, 17),
    e(31, 163),
    e(36, 14),
    e(41, 104),
    e(46, 210),
];
const LEARNSET_SEADRA: &[LevelUpMove] = &[
    e(1, 145),
    e(1, 108),
    e(1, 43),
    e(1, 55),
    e(8, 108),
    e(15, 43),
    e(22, 55),
    e(29, 239),
    e(40, 97),
    e(51, 56),
    e(62, 349),
];
const LEARNSET_SEAKING: &[LevelUpMove] = &[
    e(1, 64),
    e(1, 39),
    e(1, 346),
    e(1, 48),
    e(10, 48),
    e(15, 30),
    e(24, 175),
    e(29, 31),
    e(41, 127),
    e(49, 32),
    e(61, 97),
];
const LEARNSET_SEALEO: &[LevelUpMove] = &[
    e(1, 181),
    e(1, 45),
    e(1, 55),
    e(1, 227),
    e(7, 227),
    e(13, 301),
    e(19, 34),
    e(25, 62),
    e(31, 258),
    e(39, 156),
    e(39, 173),
    e(47, 59),
    e(55, 329),
];
const LEARNSET_SEEDOT: &[LevelUpMove] = &[
    e(1, 117),
    e(3, 106),
    e(7, 74),
    e(13, 267),
    e(21, 235),
    e(31, 241),
    e(43, 153),
];
const LEARNSET_SEEL: &[LevelUpMove] = &[
    e(1, 29),
    e(9, 45),
    e(17, 196),
    e(21, 62),
    e(29, 156),
    e(37, 36),
    e(41, 58),
    e(49, 219),
];
const LEARNSET_SENTRET: &[LevelUpMove] = &[
    e(1, 10),
    e(4, 111),
    e(7, 98),
    e(12, 154),
    e(17, 270),
    e(24, 21),
    e(31, 266),
    e(40, 156),
    e(49, 133),
];
const LEARNSET_SEVIPER: &[LevelUpMove] = &[
    e(1, 35),
    e(7, 122),
    e(10, 44),
    e(16, 342),
    e(19, 103),
    e(25, 137),
    e(28, 242),
    e(34, 305),
    e(37, 207),
    e(43, 114),
];
const LEARNSET_SHARPEDO: &[LevelUpMove] = &[
    e(1, 43),
    e(1, 44),
    e(1, 99),
    e(1, 116),
    e(7, 99),
    e(13, 116),
    e(16, 184),
    e(22, 242),
    e(28, 103),
    e(33, 163),
    e(38, 269),
    e(43, 207),
    e(48, 130),
    e(53, 97),
];
const LEARNSET_SHEDINJA: &[LevelUpMove] = &[
    e(1, 10),
    e(1, 106),
    e(5, 141),
    e(9, 28),
    e(14, 154),
    e(19, 170),
    e(25, 180),
    e(31, 109),
    e(38, 247),
    e(45, 288),
];
const LEARNSET_SHELGON: &[LevelUpMove] = &[
    e(1, 99),
    e(1, 44),
    e(1, 43),
    e(1, 29),
    e(5, 44),
    e(9, 43),
    e(17, 29),
    e(21, 116),
    e(25, 52),
    e(30, 182),
    e(38, 225),
    e(47, 184),
    e(56, 242),
    e(69, 337),
    e(78, 38),
];
const LEARNSET_SHELLDER: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 110),
    e(9, 48),
    e(17, 62),
    e(25, 182),
    e(33, 43),
    e(41, 128),
    e(49, 58),
];
const LEARNSET_SHIFTRY: &[LevelUpMove] = &[e(1, 1), e(1, 106), e(1, 74), e(1, 267)];
const LEARNSET_SHROOMISH: &[LevelUpMove] = &[
    e(1, 71),
    e(4, 33),
    e(7, 78),
    e(10, 73),
    e(16, 72),
    e(22, 29),
    e(28, 77),
    e(36, 74),
    e(45, 202),
    e(54, 147),
];
const LEARNSET_SHUCKLE: &[LevelUpMove] = &[
    e(1, 132),
    e(1, 110),
    e(9, 35),
    e(14, 227),
    e(23, 219),
    e(28, 117),
    e(37, 156),
];
const LEARNSET_SHUPPET: &[LevelUpMove] = &[
    e(1, 282),
    e(8, 103),
    e(13, 101),
    e(20, 174),
    e(25, 180),
    e(32, 261),
    e(37, 185),
    e(44, 247),
    e(49, 289),
    e(56, 288),
];
const LEARNSET_SILCOON: &[LevelUpMove] = &[e(1, 106), e(7, 106)];
const LEARNSET_SKARMORY: &[LevelUpMove] = &[
    e(1, 43),
    e(1, 64),
    e(10, 28),
    e(13, 129),
    e(16, 97),
    e(26, 31),
    e(29, 314),
    e(32, 211),
    e(42, 191),
    e(45, 319),
];
const LEARNSET_SKIPLOOM: &[LevelUpMove] = &[
    e(1, 150),
    e(1, 235),
    e(1, 39),
    e(1, 33),
    e(5, 235),
    e(5, 39),
    e(10, 33),
    e(13, 77),
    e(15, 78),
    e(17, 79),
    e(22, 73),
    e(29, 178),
    e(36, 72),
];
const LEARNSET_SKITTY: &[LevelUpMove] = &[
    e(1, 45),
    e(1, 33),
    e(3, 39),
    e(7, 213),
    e(13, 47),
    e(15, 3),
    e(19, 274),
    e(25, 204),
    e(27, 185),
    e(31, 343),
    e(37, 215),
    e(39, 38),
];
const LEARNSET_SLAKING: &[LevelUpMove] = &[
    e(1, 10),
    e(1, 281),
    e(1, 227),
    e(1, 303),
    e(7, 227),
    e(13, 303),
    e(19, 185),
    e(25, 133),
    e(31, 343),
    e(36, 207),
    e(37, 68),
    e(43, 175),
];
const LEARNSET_SLAKOTH: &[LevelUpMove] = &[
    e(1, 10),
    e(1, 281),
    e(7, 227),
    e(13, 303),
    e(19, 185),
    e(25, 133),
    e(31, 343),
    e(37, 68),
    e(43, 175),
];
const LEARNSET_SLOWBRO: &[LevelUpMove] = &[
    e(1, 174),
    e(1, 281),
    e(1, 33),
    e(1, 45),
    e(6, 45),
    e(15, 55),
    e(20, 93),
    e(29, 50),
    e(34, 29),
    e(37, 110),
    e(46, 133),
    e(54, 94),
];
const LEARNSET_SLOWKING: &[LevelUpMove] = &[
    e(1, 174),
    e(1, 281),
    e(1, 33),
    e(6, 45),
    e(15, 55),
    e(20, 93),
    e(29, 50),
    e(34, 29),
    e(43, 207),
    e(48, 94),
];
const LEARNSET_SLOWPOKE: &[LevelUpMove] = &[
    e(1, 174),
    e(1, 281),
    e(1, 33),
    e(6, 45),
    e(15, 55),
    e(20, 93),
    e(29, 50),
    e(34, 29),
    e(43, 133),
    e(48, 94),
];
const LEARNSET_SLUGMA: &[LevelUpMove] = &[
    e(1, 281),
    e(1, 123),
    e(8, 52),
    e(15, 88),
    e(22, 106),
    e(29, 133),
    e(36, 53),
    e(43, 157),
    e(50, 34),
];
const LEARNSET_SMEARGLE: &[LevelUpMove] = &[
    e(1, 166),
    e(11, 166),
    e(21, 166),
    e(31, 166),
    e(41, 166),
    e(51, 166),
    e(61, 166),
    e(71, 166),
    e(81, 166),
    e(91, 166),
];
const LEARNSET_SMOOCHUM: &[LevelUpMove] = &[
    e(1, 1),
    e(1, 122),
    e(9, 186),
    e(13, 181),
    e(21, 93),
    e(25, 47),
    e(33, 212),
    e(37, 313),
    e(45, 94),
    e(49, 195),
    e(57, 59),
];
const LEARNSET_SNEASEL: &[LevelUpMove] = &[
    e(1, 10),
    e(1, 43),
    e(1, 269),
    e(8, 98),
    e(15, 103),
    e(22, 185),
    e(29, 154),
    e(36, 97),
    e(43, 196),
    e(50, 163),
    e(57, 251),
    e(64, 232),
];
const LEARNSET_SNORLAX: &[LevelUpMove] = &[
    e(1, 33),
    e(6, 133),
    e(10, 111),
    e(15, 187),
    e(19, 29),
    e(24, 281),
    e(28, 156),
    e(28, 173),
    e(33, 34),
    e(37, 335),
    e(42, 343),
    e(46, 205),
    e(51, 63),
];
const LEARNSET_SNORUNT: &[LevelUpMove] = &[
    e(1, 181),
    e(1, 43),
    e(7, 104),
    e(10, 44),
    e(16, 196),
    e(19, 29),
    e(25, 182),
    e(28, 242),
    e(34, 58),
    e(37, 258),
    e(43, 59),
];
const LEARNSET_SNUBBULL: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 184),
    e(4, 39),
    e(8, 204),
    e(13, 44),
    e(19, 122),
    e(26, 46),
    e(34, 99),
    e(43, 36),
    e(53, 242),
];
const LEARNSET_SOLROCK: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 106),
    e(7, 93),
    e(13, 88),
    e(19, 83),
    e(25, 149),
    e(31, 322),
    e(37, 157),
    e(43, 76),
    e(49, 153),
];
const LEARNSET_SPEAROW: &[LevelUpMove] = &[
    e(1, 64),
    e(1, 45),
    e(7, 43),
    e(13, 31),
    e(19, 228),
    e(25, 332),
    e(31, 119),
    e(37, 65),
    e(43, 97),
];
const LEARNSET_SPECIES252: &[LevelUpMove] = &[e(1, 33)];
const LEARNSET_SPECIES253: &[LevelUpMove] = &[e(1, 33)];
const LEARNSET_SPECIES254: &[LevelUpMove] = &[e(1, 33)];
const LEARNSET_SPECIES255: &[LevelUpMove] = &[e(1, 33)];
const LEARNSET_SPECIES256: &[LevelUpMove] = &[e(1, 33)];
const LEARNSET_SPECIES257: &[LevelUpMove] = &[e(1, 33)];
const LEARNSET_SPECIES258: &[LevelUpMove] = &[e(1, 33)];
const LEARNSET_SPECIES259: &[LevelUpMove] = &[e(1, 33)];
const LEARNSET_SPECIES260: &[LevelUpMove] = &[e(1, 33)];
const LEARNSET_SPECIES261: &[LevelUpMove] = &[e(1, 33)];
const LEARNSET_SPECIES262: &[LevelUpMove] = &[e(1, 33)];
const LEARNSET_SPECIES263: &[LevelUpMove] = &[e(1, 33)];
const LEARNSET_SPECIES264: &[LevelUpMove] = &[e(1, 33)];
const LEARNSET_SPECIES265: &[LevelUpMove] = &[e(1, 33)];
const LEARNSET_SPECIES266: &[LevelUpMove] = &[e(1, 33)];
const LEARNSET_SPECIES267: &[LevelUpMove] = &[e(1, 33)];
const LEARNSET_SPECIES268: &[LevelUpMove] = &[e(1, 33)];
const LEARNSET_SPECIES269: &[LevelUpMove] = &[e(1, 33)];
const LEARNSET_SPECIES270: &[LevelUpMove] = &[e(1, 33)];
const LEARNSET_SPECIES271: &[LevelUpMove] = &[e(1, 33)];
const LEARNSET_SPECIES272: &[LevelUpMove] = &[e(1, 33)];
const LEARNSET_SPECIES273: &[LevelUpMove] = &[e(1, 33)];
const LEARNSET_SPECIES274: &[LevelUpMove] = &[e(1, 33)];
const LEARNSET_SPECIES275: &[LevelUpMove] = &[e(1, 33)];
const LEARNSET_SPECIES276: &[LevelUpMove] = &[e(1, 33)];
const LEARNSET_SPHEAL: &[LevelUpMove] = &[
    e(1, 181),
    e(1, 45),
    e(1, 55),
    e(7, 227),
    e(13, 301),
    e(19, 34),
    e(25, 62),
    e(31, 258),
    e(37, 156),
    e(37, 173),
    e(43, 59),
    e(49, 329),
];
const LEARNSET_SPINARAK: &[LevelUpMove] = &[
    e(1, 40),
    e(1, 81),
    e(6, 184),
    e(11, 132),
    e(17, 101),
    e(23, 141),
    e(30, 154),
    e(37, 169),
    e(45, 97),
    e(53, 94),
];
const LEARNSET_SPINDA: &[LevelUpMove] = &[
    e(1, 33),
    e(5, 253),
    e(12, 185),
    e(16, 60),
    e(23, 95),
    e(27, 146),
    e(34, 298),
    e(38, 244),
    e(45, 38),
    e(49, 175),
    e(56, 37),
];
const LEARNSET_SPOINK: &[LevelUpMove] = &[
    e(1, 150),
    e(7, 149),
    e(10, 316),
    e(16, 60),
    e(19, 244),
    e(25, 109),
    e(28, 277),
    e(34, 94),
    e(37, 156),
    e(37, 173),
    e(43, 340),
];
const LEARNSET_SQUIRTLE: &[LevelUpMove] = &[
    e(1, 33),
    e(4, 39),
    e(7, 145),
    e(10, 110),
    e(13, 55),
    e(18, 44),
    e(23, 229),
    e(28, 182),
    e(33, 240),
    e(40, 130),
    e(47, 56),
];
const LEARNSET_STANTLER: &[LevelUpMove] = &[
    e(1, 33),
    e(7, 43),
    e(13, 310),
    e(19, 95),
    e(25, 23),
    e(31, 28),
    e(37, 36),
    e(43, 109),
    e(49, 347),
];
const LEARNSET_STARMIE: &[LevelUpMove] = &[e(1, 55), e(1, 229), e(1, 105), e(1, 129), e(33, 109)];
const LEARNSET_STARYU: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 106),
    e(6, 55),
    e(10, 229),
    e(15, 105),
    e(19, 293),
    e(24, 129),
    e(28, 61),
    e(33, 107),
    e(37, 113),
    e(42, 322),
    e(46, 56),
];
const LEARNSET_STEELIX: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 103),
    e(9, 20),
    e(13, 88),
    e(21, 106),
    e(25, 99),
    e(33, 201),
    e(37, 21),
    e(45, 231),
    e(49, 242),
    e(57, 38),
];
const LEARNSET_SUDOWOODO: &[LevelUpMove] = &[
    e(1, 88),
    e(1, 102),
    e(9, 175),
    e(17, 67),
    e(25, 157),
    e(33, 335),
    e(41, 185),
    e(49, 21),
    e(57, 38),
];
const LEARNSET_SUICUNE: &[LevelUpMove] = &[
    e(1, 44),
    e(1, 43),
    e(11, 61),
    e(21, 240),
    e(31, 16),
    e(41, 62),
    e(51, 54),
    e(61, 243),
    e(71, 56),
    e(81, 347),
];
const LEARNSET_SUNFLORA: &[LevelUpMove] = &[
    e(1, 71),
    e(1, 1),
    e(6, 74),
    e(13, 75),
    e(18, 275),
    e(25, 331),
    e(30, 241),
    e(37, 80),
    e(42, 76),
];
const LEARNSET_SUNKERN: &[LevelUpMove] = &[
    e(1, 71),
    e(6, 74),
    e(13, 72),
    e(18, 275),
    e(25, 283),
    e(30, 241),
    e(37, 235),
    e(42, 202),
];
const LEARNSET_SURSKIT: &[LevelUpMove] = &[
    e(1, 145),
    e(7, 98),
    e(13, 230),
    e(19, 346),
    e(25, 61),
    e(31, 97),
    e(37, 54),
    e(37, 114),
];
const LEARNSET_SWABLU: &[LevelUpMove] = &[
    e(1, 64),
    e(1, 45),
    e(8, 310),
    e(11, 47),
    e(18, 31),
    e(21, 219),
    e(28, 54),
    e(31, 36),
    e(38, 119),
    e(41, 287),
    e(48, 195),
];
const LEARNSET_SWALOT: &[LevelUpMove] = &[
    e(1, 1),
    e(1, 281),
    e(1, 139),
    e(1, 124),
    e(6, 281),
    e(9, 139),
    e(14, 124),
    e(17, 133),
    e(23, 227),
    e(26, 34),
    e(31, 92),
    e(40, 254),
    e(40, 255),
    e(40, 256),
    e(48, 188),
];
const LEARNSET_SWAMPERT: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 45),
    e(1, 189),
    e(1, 55),
    e(6, 189),
    e(10, 55),
    e(15, 117),
    e(16, 341),
    e(20, 193),
    e(25, 300),
    e(31, 36),
    e(39, 330),
    e(46, 182),
    e(52, 89),
    e(61, 283),
];
const LEARNSET_SWELLOW: &[LevelUpMove] = &[
    e(1, 64),
    e(1, 45),
    e(1, 116),
    e(1, 98),
    e(4, 116),
    e(8, 98),
    e(13, 17),
    e(19, 104),
    e(28, 283),
    e(38, 332),
    e(49, 97),
];
const LEARNSET_SWINUB: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 316),
    e(10, 181),
    e(19, 203),
    e(28, 36),
    e(37, 54),
    e(46, 59),
    e(55, 133),
];
const LEARNSET_TAILLOW: &[LevelUpMove] = &[
    e(1, 64),
    e(1, 45),
    e(4, 116),
    e(8, 98),
    e(13, 17),
    e(19, 104),
    e(26, 283),
    e(34, 332),
    e(43, 97),
];
const LEARNSET_TANGELA: &[LevelUpMove] = &[
    e(1, 275),
    e(1, 132),
    e(4, 79),
    e(10, 71),
    e(13, 74),
    e(19, 77),
    e(22, 22),
    e(28, 20),
    e(31, 72),
    e(37, 78),
    e(40, 21),
    e(46, 321),
];
const LEARNSET_TAUROS: &[LevelUpMove] = &[
    e(1, 33),
    e(4, 39),
    e(8, 99),
    e(13, 30),
    e(19, 184),
    e(26, 228),
    e(34, 156),
    e(43, 37),
    e(53, 36),
];
const LEARNSET_TEDDIURSA: &[LevelUpMove] = &[
    e(1, 10),
    e(1, 43),
    e(7, 122),
    e(13, 154),
    e(19, 313),
    e(25, 185),
    e(31, 156),
    e(37, 163),
    e(43, 173),
    e(49, 37),
];
const LEARNSET_TENTACOOL: &[LevelUpMove] = &[
    e(1, 40),
    e(6, 48),
    e(12, 132),
    e(19, 51),
    e(25, 61),
    e(30, 35),
    e(36, 112),
    e(43, 103),
    e(49, 56),
];
const LEARNSET_TENTACRUEL: &[LevelUpMove] = &[
    e(1, 40),
    e(1, 48),
    e(1, 132),
    e(6, 48),
    e(12, 132),
    e(19, 51),
    e(25, 61),
    e(30, 35),
    e(38, 112),
    e(47, 103),
    e(55, 56),
];
const LEARNSET_TOGEPI: &[LevelUpMove] = &[
    e(1, 45),
    e(1, 204),
    e(6, 118),
    e(11, 186),
    e(16, 281),
    e(21, 227),
    e(26, 266),
    e(31, 273),
    e(36, 219),
    e(41, 38),
];
const LEARNSET_TOGETIC: &[LevelUpMove] = &[
    e(1, 45),
    e(1, 204),
    e(6, 118),
    e(11, 186),
    e(16, 281),
    e(21, 227),
    e(26, 266),
    e(31, 273),
    e(36, 219),
    e(41, 38),
];
const LEARNSET_TORCHIC: &[LevelUpMove] = &[
    e(1, 10),
    e(1, 45),
    e(7, 116),
    e(10, 52),
    e(16, 64),
    e(19, 28),
    e(25, 83),
    e(28, 98),
    e(34, 163),
    e(37, 119),
    e(43, 53),
];
const LEARNSET_TORKOAL: &[LevelUpMove] = &[
    e(1, 52),
    e(4, 123),
    e(7, 174),
    e(14, 108),
    e(17, 83),
    e(20, 34),
    e(27, 182),
    e(30, 53),
    e(33, 334),
    e(40, 133),
    e(43, 175),
    e(46, 257),
];
const LEARNSET_TOTODILE: &[LevelUpMove] = &[
    e(1, 10),
    e(1, 43),
    e(7, 99),
    e(13, 55),
    e(20, 44),
    e(27, 184),
    e(35, 163),
    e(43, 103),
    e(52, 56),
];
const LEARNSET_TRAPINCH: &[LevelUpMove] = &[
    e(1, 44),
    e(9, 28),
    e(17, 185),
    e(25, 328),
    e(33, 242),
    e(41, 91),
    e(49, 201),
    e(57, 63),
];
const LEARNSET_TREECKO: &[LevelUpMove] = &[
    e(1, 1),
    e(1, 43),
    e(6, 71),
    e(11, 98),
    e(16, 228),
    e(21, 103),
    e(26, 72),
    e(31, 97),
    e(36, 21),
    e(41, 197),
    e(46, 202),
];
const LEARNSET_TROPIUS: &[LevelUpMove] = &[
    e(1, 43),
    e(1, 16),
    e(7, 74),
    e(11, 75),
    e(17, 23),
    e(21, 230),
    e(27, 18),
    e(31, 345),
    e(37, 34),
    e(41, 76),
    e(47, 235),
];
const LEARNSET_TYPHLOSION: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 43),
    e(1, 108),
    e(1, 52),
    e(6, 108),
    e(12, 52),
    e(21, 98),
    e(31, 172),
    e(45, 129),
    e(60, 53),
];
const LEARNSET_TYRANITAR: &[LevelUpMove] = &[
    e(1, 44),
    e(1, 43),
    e(1, 201),
    e(1, 103),
    e(8, 201),
    e(15, 103),
    e(22, 157),
    e(29, 37),
    e(38, 184),
    e(47, 242),
    e(61, 89),
    e(75, 63),
];
const LEARNSET_TYROGUE: &[LevelUpMove] = &[e(1, 33)];
const LEARNSET_UMBREON: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 39),
    e(1, 270),
    e(8, 28),
    e(16, 228),
    e(23, 98),
    e(30, 109),
    e(36, 185),
    e(42, 212),
    e(47, 103),
    e(52, 236),
];
const LEARNSET_UNOWN: &[LevelUpMove] = &[e(1, 237)];
const LEARNSET_URSARING: &[LevelUpMove] = &[
    e(1, 10),
    e(1, 43),
    e(1, 122),
    e(1, 154),
    e(7, 122),
    e(13, 154),
    e(19, 313),
    e(25, 185),
    e(31, 156),
    e(37, 163),
    e(43, 173),
    e(49, 37),
];
const LEARNSET_VAPOREON: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 39),
    e(1, 270),
    e(8, 28),
    e(16, 55),
    e(23, 98),
    e(30, 44),
    e(36, 62),
    e(42, 114),
    e(47, 151),
    e(52, 56),
];
const LEARNSET_VENOMOTH: &[LevelUpMove] = &[
    e(1, 318),
    e(1, 33),
    e(1, 50),
    e(1, 193),
    e(1, 48),
    e(9, 48),
    e(17, 93),
    e(20, 77),
    e(25, 141),
    e(28, 78),
    e(31, 16),
    e(36, 60),
    e(42, 79),
    e(52, 94),
];
const LEARNSET_VENONAT: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 50),
    e(1, 193),
    e(9, 48),
    e(17, 93),
    e(20, 77),
    e(25, 141),
    e(28, 78),
    e(33, 60),
    e(36, 79),
    e(41, 94),
];
const LEARNSET_VENUSAUR: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 45),
    e(1, 73),
    e(1, 22),
    e(4, 45),
    e(7, 73),
    e(10, 22),
    e(15, 77),
    e(15, 79),
    e(22, 75),
    e(29, 230),
    e(41, 74),
    e(53, 235),
    e(65, 76),
];
const LEARNSET_VIBRAVA: &[LevelUpMove] = &[
    e(1, 44),
    e(1, 28),
    e(1, 185),
    e(1, 328),
    e(9, 28),
    e(17, 185),
    e(25, 328),
    e(33, 242),
    e(35, 225),
    e(41, 103),
    e(49, 201),
    e(57, 63),
];
const LEARNSET_VICTREEBEL: &[LevelUpMove] = &[e(1, 22), e(1, 79), e(1, 230), e(1, 75)];
const LEARNSET_VIGOROTH: &[LevelUpMove] = &[
    e(1, 10),
    e(1, 116),
    e(1, 227),
    e(1, 253),
    e(7, 227),
    e(13, 253),
    e(19, 154),
    e(25, 203),
    e(31, 163),
    e(37, 68),
    e(43, 264),
    e(49, 179),
];
const LEARNSET_VILEPLUME: &[LevelUpMove] = &[e(1, 71), e(1, 312), e(1, 78), e(1, 72), e(44, 80)];
const LEARNSET_VOLBEAT: &[LevelUpMove] = &[
    e(1, 33),
    e(5, 109),
    e(9, 104),
    e(13, 236),
    e(17, 98),
    e(21, 294),
    e(25, 324),
    e(29, 182),
    e(33, 270),
    e(37, 38),
];
const LEARNSET_VOLTORB: &[LevelUpMove] = &[
    e(1, 268),
    e(1, 33),
    e(8, 103),
    e(15, 49),
    e(21, 209),
    e(27, 120),
    e(32, 205),
    e(37, 113),
    e(42, 129),
    e(46, 153),
    e(49, 243),
];
const LEARNSET_VULPIX: &[LevelUpMove] = &[
    e(1, 52),
    e(5, 39),
    e(9, 46),
    e(13, 98),
    e(17, 261),
    e(21, 109),
    e(25, 286),
    e(29, 53),
    e(33, 219),
    e(37, 288),
    e(41, 83),
];
const LEARNSET_WAILMER: &[LevelUpMove] = &[
    e(1, 150),
    e(5, 45),
    e(10, 55),
    e(14, 205),
    e(19, 250),
    e(23, 310),
    e(28, 352),
    e(32, 54),
    e(37, 156),
    e(41, 323),
    e(46, 133),
    e(50, 56),
];
const LEARNSET_WAILORD: &[LevelUpMove] = &[
    e(1, 150),
    e(1, 45),
    e(1, 55),
    e(1, 205),
    e(5, 45),
    e(10, 55),
    e(14, 205),
    e(19, 250),
    e(23, 310),
    e(28, 352),
    e(32, 54),
    e(37, 156),
    e(44, 323),
    e(52, 133),
    e(59, 56),
];
const LEARNSET_WALREIN: &[LevelUpMove] = &[
    e(1, 181),
    e(1, 45),
    e(1, 55),
    e(1, 227),
    e(7, 227),
    e(13, 301),
    e(19, 34),
    e(25, 62),
    e(31, 258),
    e(39, 156),
    e(39, 173),
    e(50, 59),
    e(61, 329),
];
const LEARNSET_WARTORTLE: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 39),
    e(1, 145),
    e(4, 39),
    e(7, 145),
    e(10, 110),
    e(13, 55),
    e(19, 44),
    e(25, 229),
    e(31, 182),
    e(37, 240),
    e(45, 130),
    e(53, 56),
];
const LEARNSET_WEEDLE: &[LevelUpMove] = &[e(1, 40), e(1, 81)];
const LEARNSET_WEEPINBELL: &[LevelUpMove] = &[
    e(1, 22),
    e(1, 74),
    e(1, 35),
    e(6, 74),
    e(11, 35),
    e(15, 79),
    e(17, 77),
    e(19, 78),
    e(24, 51),
    e(33, 230),
    e(42, 75),
    e(54, 21),
];
const LEARNSET_WEEZING: &[LevelUpMove] = &[
    e(1, 139),
    e(1, 33),
    e(1, 123),
    e(1, 120),
    e(9, 123),
    e(17, 120),
    e(21, 124),
    e(25, 108),
    e(33, 114),
    e(44, 153),
    e(51, 194),
    e(58, 262),
];
const LEARNSET_WHISCASH: &[LevelUpMove] = &[
    e(1, 321),
    e(1, 189),
    e(1, 300),
    e(1, 346),
    e(6, 300),
    e(6, 346),
    e(11, 55),
    e(16, 222),
    e(21, 133),
    e(26, 156),
    e(26, 173),
    e(36, 89),
    e(46, 248),
    e(56, 90),
];
const LEARNSET_WHISMUR: &[LevelUpMove] = &[
    e(1, 1),
    e(5, 253),
    e(11, 310),
    e(15, 336),
    e(21, 48),
    e(25, 23),
    e(31, 103),
    e(35, 46),
    e(41, 156),
    e(41, 214),
    e(45, 304),
];
const LEARNSET_WIGGLYTUFF: &[LevelUpMove] = &[e(1, 47), e(1, 50), e(1, 111), e(1, 3)];
const LEARNSET_WINGULL: &[LevelUpMove] = &[
    e(1, 45),
    e(1, 55),
    e(7, 48),
    e(13, 17),
    e(21, 54),
    e(31, 98),
    e(43, 228),
    e(55, 97),
];
const LEARNSET_WOBBUFFET: &[LevelUpMove] = &[e(1, 68), e(1, 243), e(1, 219), e(1, 194)];
const LEARNSET_WOOPER: &[LevelUpMove] = &[
    e(1, 55),
    e(1, 39),
    e(11, 21),
    e(16, 341),
    e(21, 133),
    e(31, 281),
    e(36, 89),
    e(41, 240),
    e(51, 54),
    e(51, 114),
];
const LEARNSET_WURMPLE: &[LevelUpMove] = &[e(1, 33), e(1, 81), e(5, 40)];
const LEARNSET_WYNAUT: &[LevelUpMove] = &[
    e(1, 150),
    e(1, 204),
    e(1, 227),
    e(15, 68),
    e(15, 243),
    e(15, 219),
    e(15, 194),
];
const LEARNSET_XATU: &[LevelUpMove] = &[
    e(1, 64),
    e(1, 43),
    e(10, 101),
    e(20, 100),
    e(35, 273),
    e(35, 248),
    e(50, 109),
    e(65, 94),
];
const LEARNSET_YANMA: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 193),
    e(7, 98),
    e(13, 104),
    e(19, 49),
    e(25, 197),
    e(31, 48),
    e(37, 253),
    e(43, 17),
    e(49, 103),
];
const LEARNSET_ZANGOOSE: &[LevelUpMove] = &[
    e(1, 10),
    e(4, 43),
    e(7, 98),
    e(10, 14),
    e(13, 210),
    e(19, 163),
    e(25, 228),
    e(31, 306),
    e(37, 269),
    e(46, 197),
    e(55, 206),
];
const LEARNSET_ZAPDOS: &[LevelUpMove] = &[
    e(1, 64),
    e(1, 84),
    e(13, 86),
    e(25, 97),
    e(37, 197),
    e(49, 65),
    e(61, 268),
    e(73, 113),
    e(85, 87),
];
const LEARNSET_ZIGZAGOON: &[LevelUpMove] = &[
    e(1, 33),
    e(1, 45),
    e(5, 39),
    e(9, 29),
    e(13, 28),
    e(17, 316),
    e(21, 300),
    e(25, 42),
    e(29, 343),
    e(33, 175),
    e(37, 156),
    e(41, 187),
];
const LEARNSET_ZUBAT: &[LevelUpMove] = &[
    e(1, 141),
    e(6, 48),
    e(11, 310),
    e(16, 44),
    e(21, 17),
    e(26, 109),
    e(31, 314),
    e(36, 212),
    e(41, 305),
    e(46, 114),
];

/// The `gLevelUpLearnsets` pointer table: each species' ordered level-up
/// learnset, indexed by `SPECIES_*` id. Entries are shared where upstream
/// shares an array pointer (e.g. `SPECIES_NONE` reuses Bulbasaur's).
const LEARNSETS: [&[LevelUpMove]; SPECIES_COUNT] = [
    LEARNSET_BULBASAUR,  // 0 SPECIES_NONE
    LEARNSET_BULBASAUR,  // 1 SPECIES_BULBASAUR
    LEARNSET_IVYSAUR,    // 2 SPECIES_IVYSAUR
    LEARNSET_VENUSAUR,   // 3 SPECIES_VENUSAUR
    LEARNSET_CHARMANDER, // 4 SPECIES_CHARMANDER
    LEARNSET_CHARMELEON, // 5 SPECIES_CHARMELEON
    LEARNSET_CHARIZARD,  // 6 SPECIES_CHARIZARD
    LEARNSET_SQUIRTLE,   // 7 SPECIES_SQUIRTLE
    LEARNSET_WARTORTLE,  // 8 SPECIES_WARTORTLE
    LEARNSET_BLASTOISE,  // 9 SPECIES_BLASTOISE
    LEARNSET_CATERPIE,   // 10 SPECIES_CATERPIE
    LEARNSET_METAPOD,    // 11 SPECIES_METAPOD
    LEARNSET_BUTTERFREE, // 12 SPECIES_BUTTERFREE
    LEARNSET_WEEDLE,     // 13 SPECIES_WEEDLE
    LEARNSET_KAKUNA,     // 14 SPECIES_KAKUNA
    LEARNSET_BEEDRILL,   // 15 SPECIES_BEEDRILL
    LEARNSET_PIDGEY,     // 16 SPECIES_PIDGEY
    LEARNSET_PIDGEOTTO,  // 17 SPECIES_PIDGEOTTO
    LEARNSET_PIDGEOT,    // 18 SPECIES_PIDGEOT
    LEARNSET_RATTATA,    // 19 SPECIES_RATTATA
    LEARNSET_RATICATE,   // 20 SPECIES_RATICATE
    LEARNSET_SPEAROW,    // 21 SPECIES_SPEAROW
    LEARNSET_FEAROW,     // 22 SPECIES_FEAROW
    LEARNSET_EKANS,      // 23 SPECIES_EKANS
    LEARNSET_ARBOK,      // 24 SPECIES_ARBOK
    LEARNSET_PIKACHU,    // 25 SPECIES_PIKACHU
    LEARNSET_RAICHU,     // 26 SPECIES_RAICHU
    LEARNSET_SANDSHREW,  // 27 SPECIES_SANDSHREW
    LEARNSET_SANDSLASH,  // 28 SPECIES_SANDSLASH
    LEARNSET_NIDORAN_F,  // 29 SPECIES_NIDORAN_F
    LEARNSET_NIDORINA,   // 30 SPECIES_NIDORINA
    LEARNSET_NIDOQUEEN,  // 31 SPECIES_NIDOQUEEN
    LEARNSET_NIDORAN_M,  // 32 SPECIES_NIDORAN_M
    LEARNSET_NIDORINO,   // 33 SPECIES_NIDORINO
    LEARNSET_NIDOKING,   // 34 SPECIES_NIDOKING
    LEARNSET_CLEFAIRY,   // 35 SPECIES_CLEFAIRY
    LEARNSET_CLEFABLE,   // 36 SPECIES_CLEFABLE
    LEARNSET_VULPIX,     // 37 SPECIES_VULPIX
    LEARNSET_NINETALES,  // 38 SPECIES_NINETALES
    LEARNSET_JIGGLYPUFF, // 39 SPECIES_JIGGLYPUFF
    LEARNSET_WIGGLYTUFF, // 40 SPECIES_WIGGLYTUFF
    LEARNSET_ZUBAT,      // 41 SPECIES_ZUBAT
    LEARNSET_GOLBAT,     // 42 SPECIES_GOLBAT
    LEARNSET_ODDISH,     // 43 SPECIES_ODDISH
    LEARNSET_GLOOM,      // 44 SPECIES_GLOOM
    LEARNSET_VILEPLUME,  // 45 SPECIES_VILEPLUME
    LEARNSET_PARAS,      // 46 SPECIES_PARAS
    LEARNSET_PARASECT,   // 47 SPECIES_PARASECT
    LEARNSET_VENONAT,    // 48 SPECIES_VENONAT
    LEARNSET_VENOMOTH,   // 49 SPECIES_VENOMOTH
    LEARNSET_DIGLETT,    // 50 SPECIES_DIGLETT
    LEARNSET_DUGTRIO,    // 51 SPECIES_DUGTRIO
    LEARNSET_MEOWTH,     // 52 SPECIES_MEOWTH
    LEARNSET_PERSIAN,    // 53 SPECIES_PERSIAN
    LEARNSET_PSYDUCK,    // 54 SPECIES_PSYDUCK
    LEARNSET_GOLDUCK,    // 55 SPECIES_GOLDUCK
    LEARNSET_MANKEY,     // 56 SPECIES_MANKEY
    LEARNSET_PRIMEAPE,   // 57 SPECIES_PRIMEAPE
    LEARNSET_GROWLITHE,  // 58 SPECIES_GROWLITHE
    LEARNSET_ARCANINE,   // 59 SPECIES_ARCANINE
    LEARNSET_POLIWAG,    // 60 SPECIES_POLIWAG
    LEARNSET_POLIWHIRL,  // 61 SPECIES_POLIWHIRL
    LEARNSET_POLIWRATH,  // 62 SPECIES_POLIWRATH
    LEARNSET_ABRA,       // 63 SPECIES_ABRA
    LEARNSET_KADABRA,    // 64 SPECIES_KADABRA
    LEARNSET_ALAKAZAM,   // 65 SPECIES_ALAKAZAM
    LEARNSET_MACHOP,     // 66 SPECIES_MACHOP
    LEARNSET_MACHOKE,    // 67 SPECIES_MACHOKE
    LEARNSET_MACHAMP,    // 68 SPECIES_MACHAMP
    LEARNSET_BELLSPROUT, // 69 SPECIES_BELLSPROUT
    LEARNSET_WEEPINBELL, // 70 SPECIES_WEEPINBELL
    LEARNSET_VICTREEBEL, // 71 SPECIES_VICTREEBEL
    LEARNSET_TENTACOOL,  // 72 SPECIES_TENTACOOL
    LEARNSET_TENTACRUEL, // 73 SPECIES_TENTACRUEL
    LEARNSET_GEODUDE,    // 74 SPECIES_GEODUDE
    LEARNSET_GRAVELER,   // 75 SPECIES_GRAVELER
    LEARNSET_GOLEM,      // 76 SPECIES_GOLEM
    LEARNSET_PONYTA,     // 77 SPECIES_PONYTA
    LEARNSET_RAPIDASH,   // 78 SPECIES_RAPIDASH
    LEARNSET_SLOWPOKE,   // 79 SPECIES_SLOWPOKE
    LEARNSET_SLOWBRO,    // 80 SPECIES_SLOWBRO
    LEARNSET_MAGNEMITE,  // 81 SPECIES_MAGNEMITE
    LEARNSET_MAGNETON,   // 82 SPECIES_MAGNETON
    LEARNSET_FARFETCHD,  // 83 SPECIES_FARFETCHD
    LEARNSET_DODUO,      // 84 SPECIES_DODUO
    LEARNSET_DODRIO,     // 85 SPECIES_DODRIO
    LEARNSET_SEEL,       // 86 SPECIES_SEEL
    LEARNSET_DEWGONG,    // 87 SPECIES_DEWGONG
    LEARNSET_GRIMER,     // 88 SPECIES_GRIMER
    LEARNSET_MUK,        // 89 SPECIES_MUK
    LEARNSET_SHELLDER,   // 90 SPECIES_SHELLDER
    LEARNSET_CLOYSTER,   // 91 SPECIES_CLOYSTER
    LEARNSET_GASTLY,     // 92 SPECIES_GASTLY
    LEARNSET_HAUNTER,    // 93 SPECIES_HAUNTER
    LEARNSET_GENGAR,     // 94 SPECIES_GENGAR
    LEARNSET_ONIX,       // 95 SPECIES_ONIX
    LEARNSET_DROWZEE,    // 96 SPECIES_DROWZEE
    LEARNSET_HYPNO,      // 97 SPECIES_HYPNO
    LEARNSET_KRABBY,     // 98 SPECIES_KRABBY
    LEARNSET_KINGLER,    // 99 SPECIES_KINGLER
    LEARNSET_VOLTORB,    // 100 SPECIES_VOLTORB
    LEARNSET_ELECTRODE,  // 101 SPECIES_ELECTRODE
    LEARNSET_EXEGGCUTE,  // 102 SPECIES_EXEGGCUTE
    LEARNSET_EXEGGUTOR,  // 103 SPECIES_EXEGGUTOR
    LEARNSET_CUBONE,     // 104 SPECIES_CUBONE
    LEARNSET_MAROWAK,    // 105 SPECIES_MAROWAK
    LEARNSET_HITMONLEE,  // 106 SPECIES_HITMONLEE
    LEARNSET_HITMONCHAN, // 107 SPECIES_HITMONCHAN
    LEARNSET_LICKITUNG,  // 108 SPECIES_LICKITUNG
    LEARNSET_KOFFING,    // 109 SPECIES_KOFFING
    LEARNSET_WEEZING,    // 110 SPECIES_WEEZING
    LEARNSET_RHYHORN,    // 111 SPECIES_RHYHORN
    LEARNSET_RHYDON,     // 112 SPECIES_RHYDON
    LEARNSET_CHANSEY,    // 113 SPECIES_CHANSEY
    LEARNSET_TANGELA,    // 114 SPECIES_TANGELA
    LEARNSET_KANGASKHAN, // 115 SPECIES_KANGASKHAN
    LEARNSET_HORSEA,     // 116 SPECIES_HORSEA
    LEARNSET_SEADRA,     // 117 SPECIES_SEADRA
    LEARNSET_GOLDEEN,    // 118 SPECIES_GOLDEEN
    LEARNSET_SEAKING,    // 119 SPECIES_SEAKING
    LEARNSET_STARYU,     // 120 SPECIES_STARYU
    LEARNSET_STARMIE,    // 121 SPECIES_STARMIE
    LEARNSET_MR_MIME,    // 122 SPECIES_MR_MIME
    LEARNSET_SCYTHER,    // 123 SPECIES_SCYTHER
    LEARNSET_JYNX,       // 124 SPECIES_JYNX
    LEARNSET_ELECTABUZZ, // 125 SPECIES_ELECTABUZZ
    LEARNSET_MAGMAR,     // 126 SPECIES_MAGMAR
    LEARNSET_PINSIR,     // 127 SPECIES_PINSIR
    LEARNSET_TAUROS,     // 128 SPECIES_TAUROS
    LEARNSET_MAGIKARP,   // 129 SPECIES_MAGIKARP
    LEARNSET_GYARADOS,   // 130 SPECIES_GYARADOS
    LEARNSET_LAPRAS,     // 131 SPECIES_LAPRAS
    LEARNSET_DITTO,      // 132 SPECIES_DITTO
    LEARNSET_EEVEE,      // 133 SPECIES_EEVEE
    LEARNSET_VAPOREON,   // 134 SPECIES_VAPOREON
    LEARNSET_JOLTEON,    // 135 SPECIES_JOLTEON
    LEARNSET_FLAREON,    // 136 SPECIES_FLAREON
    LEARNSET_PORYGON,    // 137 SPECIES_PORYGON
    LEARNSET_OMANYTE,    // 138 SPECIES_OMANYTE
    LEARNSET_OMASTAR,    // 139 SPECIES_OMASTAR
    LEARNSET_KABUTO,     // 140 SPECIES_KABUTO
    LEARNSET_KABUTOPS,   // 141 SPECIES_KABUTOPS
    LEARNSET_AERODACTYL, // 142 SPECIES_AERODACTYL
    LEARNSET_SNORLAX,    // 143 SPECIES_SNORLAX
    LEARNSET_ARTICUNO,   // 144 SPECIES_ARTICUNO
    LEARNSET_ZAPDOS,     // 145 SPECIES_ZAPDOS
    LEARNSET_MOLTRES,    // 146 SPECIES_MOLTRES
    LEARNSET_DRATINI,    // 147 SPECIES_DRATINI
    LEARNSET_DRAGONAIR,  // 148 SPECIES_DRAGONAIR
    LEARNSET_DRAGONITE,  // 149 SPECIES_DRAGONITE
    LEARNSET_MEWTWO,     // 150 SPECIES_MEWTWO
    LEARNSET_MEW,        // 151 SPECIES_MEW
    LEARNSET_CHIKORITA,  // 152 SPECIES_CHIKORITA
    LEARNSET_BAYLEEF,    // 153 SPECIES_BAYLEEF
    LEARNSET_MEGANIUM,   // 154 SPECIES_MEGANIUM
    LEARNSET_CYNDAQUIL,  // 155 SPECIES_CYNDAQUIL
    LEARNSET_QUILAVA,    // 156 SPECIES_QUILAVA
    LEARNSET_TYPHLOSION, // 157 SPECIES_TYPHLOSION
    LEARNSET_TOTODILE,   // 158 SPECIES_TOTODILE
    LEARNSET_CROCONAW,   // 159 SPECIES_CROCONAW
    LEARNSET_FERALIGATR, // 160 SPECIES_FERALIGATR
    LEARNSET_SENTRET,    // 161 SPECIES_SENTRET
    LEARNSET_FURRET,     // 162 SPECIES_FURRET
    LEARNSET_HOOTHOOT,   // 163 SPECIES_HOOTHOOT
    LEARNSET_NOCTOWL,    // 164 SPECIES_NOCTOWL
    LEARNSET_LEDYBA,     // 165 SPECIES_LEDYBA
    LEARNSET_LEDIAN,     // 166 SPECIES_LEDIAN
    LEARNSET_SPINARAK,   // 167 SPECIES_SPINARAK
    LEARNSET_ARIADOS,    // 168 SPECIES_ARIADOS
    LEARNSET_CROBAT,     // 169 SPECIES_CROBAT
    LEARNSET_CHINCHOU,   // 170 SPECIES_CHINCHOU
    LEARNSET_LANTURN,    // 171 SPECIES_LANTURN
    LEARNSET_PICHU,      // 172 SPECIES_PICHU
    LEARNSET_CLEFFA,     // 173 SPECIES_CLEFFA
    LEARNSET_IGGLYBUFF,  // 174 SPECIES_IGGLYBUFF
    LEARNSET_TOGEPI,     // 175 SPECIES_TOGEPI
    LEARNSET_TOGETIC,    // 176 SPECIES_TOGETIC
    LEARNSET_NATU,       // 177 SPECIES_NATU
    LEARNSET_XATU,       // 178 SPECIES_XATU
    LEARNSET_MAREEP,     // 179 SPECIES_MAREEP
    LEARNSET_FLAAFFY,    // 180 SPECIES_FLAAFFY
    LEARNSET_AMPHAROS,   // 181 SPECIES_AMPHAROS
    LEARNSET_BELLOSSOM,  // 182 SPECIES_BELLOSSOM
    LEARNSET_MARILL,     // 183 SPECIES_MARILL
    LEARNSET_AZUMARILL,  // 184 SPECIES_AZUMARILL
    LEARNSET_SUDOWOODO,  // 185 SPECIES_SUDOWOODO
    LEARNSET_POLITOED,   // 186 SPECIES_POLITOED
    LEARNSET_HOPPIP,     // 187 SPECIES_HOPPIP
    LEARNSET_SKIPLOOM,   // 188 SPECIES_SKIPLOOM
    LEARNSET_JUMPLUFF,   // 189 SPECIES_JUMPLUFF
    LEARNSET_AIPOM,      // 190 SPECIES_AIPOM
    LEARNSET_SUNKERN,    // 191 SPECIES_SUNKERN
    LEARNSET_SUNFLORA,   // 192 SPECIES_SUNFLORA
    LEARNSET_YANMA,      // 193 SPECIES_YANMA
    LEARNSET_WOOPER,     // 194 SPECIES_WOOPER
    LEARNSET_QUAGSIRE,   // 195 SPECIES_QUAGSIRE
    LEARNSET_ESPEON,     // 196 SPECIES_ESPEON
    LEARNSET_UMBREON,    // 197 SPECIES_UMBREON
    LEARNSET_MURKROW,    // 198 SPECIES_MURKROW
    LEARNSET_SLOWKING,   // 199 SPECIES_SLOWKING
    LEARNSET_MISDREAVUS, // 200 SPECIES_MISDREAVUS
    LEARNSET_UNOWN,      // 201 SPECIES_UNOWN
    LEARNSET_WOBBUFFET,  // 202 SPECIES_WOBBUFFET
    LEARNSET_GIRAFARIG,  // 203 SPECIES_GIRAFARIG
    LEARNSET_PINECO,     // 204 SPECIES_PINECO
    LEARNSET_FORRETRESS, // 205 SPECIES_FORRETRESS
    LEARNSET_DUNSPARCE,  // 206 SPECIES_DUNSPARCE
    LEARNSET_GLIGAR,     // 207 SPECIES_GLIGAR
    LEARNSET_STEELIX,    // 208 SPECIES_STEELIX
    LEARNSET_SNUBBULL,   // 209 SPECIES_SNUBBULL
    LEARNSET_GRANBULL,   // 210 SPECIES_GRANBULL
    LEARNSET_QWILFISH,   // 211 SPECIES_QWILFISH
    LEARNSET_SCIZOR,     // 212 SPECIES_SCIZOR
    LEARNSET_SHUCKLE,    // 213 SPECIES_SHUCKLE
    LEARNSET_HERACROSS,  // 214 SPECIES_HERACROSS
    LEARNSET_SNEASEL,    // 215 SPECIES_SNEASEL
    LEARNSET_TEDDIURSA,  // 216 SPECIES_TEDDIURSA
    LEARNSET_URSARING,   // 217 SPECIES_URSARING
    LEARNSET_SLUGMA,     // 218 SPECIES_SLUGMA
    LEARNSET_MAGCARGO,   // 219 SPECIES_MAGCARGO
    LEARNSET_SWINUB,     // 220 SPECIES_SWINUB
    LEARNSET_PILOSWINE,  // 221 SPECIES_PILOSWINE
    LEARNSET_CORSOLA,    // 222 SPECIES_CORSOLA
    LEARNSET_REMORAID,   // 223 SPECIES_REMORAID
    LEARNSET_OCTILLERY,  // 224 SPECIES_OCTILLERY
    LEARNSET_DELIBIRD,   // 225 SPECIES_DELIBIRD
    LEARNSET_MANTINE,    // 226 SPECIES_MANTINE
    LEARNSET_SKARMORY,   // 227 SPECIES_SKARMORY
    LEARNSET_HOUNDOUR,   // 228 SPECIES_HOUNDOUR
    LEARNSET_HOUNDOOM,   // 229 SPECIES_HOUNDOOM
    LEARNSET_KINGDRA,    // 230 SPECIES_KINGDRA
    LEARNSET_PHANPY,     // 231 SPECIES_PHANPY
    LEARNSET_DONPHAN,    // 232 SPECIES_DONPHAN
    LEARNSET_PORYGON2,   // 233 SPECIES_PORYGON2
    LEARNSET_STANTLER,   // 234 SPECIES_STANTLER
    LEARNSET_SMEARGLE,   // 235 SPECIES_SMEARGLE
    LEARNSET_TYROGUE,    // 236 SPECIES_TYROGUE
    LEARNSET_HITMONTOP,  // 237 SPECIES_HITMONTOP
    LEARNSET_SMOOCHUM,   // 238 SPECIES_SMOOCHUM
    LEARNSET_ELEKID,     // 239 SPECIES_ELEKID
    LEARNSET_MAGBY,      // 240 SPECIES_MAGBY
    LEARNSET_MILTANK,    // 241 SPECIES_MILTANK
    LEARNSET_BLISSEY,    // 242 SPECIES_BLISSEY
    LEARNSET_RAIKOU,     // 243 SPECIES_RAIKOU
    LEARNSET_ENTEI,      // 244 SPECIES_ENTEI
    LEARNSET_SUICUNE,    // 245 SPECIES_SUICUNE
    LEARNSET_LARVITAR,   // 246 SPECIES_LARVITAR
    LEARNSET_PUPITAR,    // 247 SPECIES_PUPITAR
    LEARNSET_TYRANITAR,  // 248 SPECIES_TYRANITAR
    LEARNSET_LUGIA,      // 249 SPECIES_LUGIA
    LEARNSET_HO_OH,      // 250 SPECIES_HO_OH
    LEARNSET_CELEBI,     // 251 SPECIES_CELEBI
    LEARNSET_SPECIES252, // 252 SPECIES_OLD_UNOWN_B
    LEARNSET_SPECIES253, // 253 SPECIES_OLD_UNOWN_C
    LEARNSET_SPECIES254, // 254 SPECIES_OLD_UNOWN_D
    LEARNSET_SPECIES255, // 255 SPECIES_OLD_UNOWN_E
    LEARNSET_SPECIES256, // 256 SPECIES_OLD_UNOWN_F
    LEARNSET_SPECIES257, // 257 SPECIES_OLD_UNOWN_G
    LEARNSET_SPECIES258, // 258 SPECIES_OLD_UNOWN_H
    LEARNSET_SPECIES259, // 259 SPECIES_OLD_UNOWN_I
    LEARNSET_SPECIES260, // 260 SPECIES_OLD_UNOWN_J
    LEARNSET_SPECIES261, // 261 SPECIES_OLD_UNOWN_K
    LEARNSET_SPECIES262, // 262 SPECIES_OLD_UNOWN_L
    LEARNSET_SPECIES263, // 263 SPECIES_OLD_UNOWN_M
    LEARNSET_SPECIES264, // 264 SPECIES_OLD_UNOWN_N
    LEARNSET_SPECIES265, // 265 SPECIES_OLD_UNOWN_O
    LEARNSET_SPECIES266, // 266 SPECIES_OLD_UNOWN_P
    LEARNSET_SPECIES267, // 267 SPECIES_OLD_UNOWN_Q
    LEARNSET_SPECIES268, // 268 SPECIES_OLD_UNOWN_R
    LEARNSET_SPECIES269, // 269 SPECIES_OLD_UNOWN_S
    LEARNSET_SPECIES270, // 270 SPECIES_OLD_UNOWN_T
    LEARNSET_SPECIES271, // 271 SPECIES_OLD_UNOWN_U
    LEARNSET_SPECIES272, // 272 SPECIES_OLD_UNOWN_V
    LEARNSET_SPECIES273, // 273 SPECIES_OLD_UNOWN_W
    LEARNSET_SPECIES274, // 274 SPECIES_OLD_UNOWN_X
    LEARNSET_SPECIES275, // 275 SPECIES_OLD_UNOWN_Y
    LEARNSET_SPECIES276, // 276 SPECIES_OLD_UNOWN_Z
    LEARNSET_TREECKO,    // 277 SPECIES_TREECKO
    LEARNSET_GROVYLE,    // 278 SPECIES_GROVYLE
    LEARNSET_SCEPTILE,   // 279 SPECIES_SCEPTILE
    LEARNSET_TORCHIC,    // 280 SPECIES_TORCHIC
    LEARNSET_COMBUSKEN,  // 281 SPECIES_COMBUSKEN
    LEARNSET_BLAZIKEN,   // 282 SPECIES_BLAZIKEN
    LEARNSET_MUDKIP,     // 283 SPECIES_MUDKIP
    LEARNSET_MARSHTOMP,  // 284 SPECIES_MARSHTOMP
    LEARNSET_SWAMPERT,   // 285 SPECIES_SWAMPERT
    LEARNSET_POOCHYENA,  // 286 SPECIES_POOCHYENA
    LEARNSET_MIGHTYENA,  // 287 SPECIES_MIGHTYENA
    LEARNSET_ZIGZAGOON,  // 288 SPECIES_ZIGZAGOON
    LEARNSET_LINOONE,    // 289 SPECIES_LINOONE
    LEARNSET_WURMPLE,    // 290 SPECIES_WURMPLE
    LEARNSET_SILCOON,    // 291 SPECIES_SILCOON
    LEARNSET_BEAUTIFLY,  // 292 SPECIES_BEAUTIFLY
    LEARNSET_CASCOON,    // 293 SPECIES_CASCOON
    LEARNSET_DUSTOX,     // 294 SPECIES_DUSTOX
    LEARNSET_LOTAD,      // 295 SPECIES_LOTAD
    LEARNSET_LOMBRE,     // 296 SPECIES_LOMBRE
    LEARNSET_LUDICOLO,   // 297 SPECIES_LUDICOLO
    LEARNSET_SEEDOT,     // 298 SPECIES_SEEDOT
    LEARNSET_NUZLEAF,    // 299 SPECIES_NUZLEAF
    LEARNSET_SHIFTRY,    // 300 SPECIES_SHIFTRY
    LEARNSET_NINCADA,    // 301 SPECIES_NINCADA
    LEARNSET_NINJASK,    // 302 SPECIES_NINJASK
    LEARNSET_SHEDINJA,   // 303 SPECIES_SHEDINJA
    LEARNSET_TAILLOW,    // 304 SPECIES_TAILLOW
    LEARNSET_SWELLOW,    // 305 SPECIES_SWELLOW
    LEARNSET_SHROOMISH,  // 306 SPECIES_SHROOMISH
    LEARNSET_BRELOOM,    // 307 SPECIES_BRELOOM
    LEARNSET_SPINDA,     // 308 SPECIES_SPINDA
    LEARNSET_WINGULL,    // 309 SPECIES_WINGULL
    LEARNSET_PELIPPER,   // 310 SPECIES_PELIPPER
    LEARNSET_SURSKIT,    // 311 SPECIES_SURSKIT
    LEARNSET_MASQUERAIN, // 312 SPECIES_MASQUERAIN
    LEARNSET_WAILMER,    // 313 SPECIES_WAILMER
    LEARNSET_WAILORD,    // 314 SPECIES_WAILORD
    LEARNSET_SKITTY,     // 315 SPECIES_SKITTY
    LEARNSET_DELCATTY,   // 316 SPECIES_DELCATTY
    LEARNSET_KECLEON,    // 317 SPECIES_KECLEON
    LEARNSET_BALTOY,     // 318 SPECIES_BALTOY
    LEARNSET_CLAYDOL,    // 319 SPECIES_CLAYDOL
    LEARNSET_NOSEPASS,   // 320 SPECIES_NOSEPASS
    LEARNSET_TORKOAL,    // 321 SPECIES_TORKOAL
    LEARNSET_SABLEYE,    // 322 SPECIES_SABLEYE
    LEARNSET_BARBOACH,   // 323 SPECIES_BARBOACH
    LEARNSET_WHISCASH,   // 324 SPECIES_WHISCASH
    LEARNSET_LUVDISC,    // 325 SPECIES_LUVDISC
    LEARNSET_CORPHISH,   // 326 SPECIES_CORPHISH
    LEARNSET_CRAWDAUNT,  // 327 SPECIES_CRAWDAUNT
    LEARNSET_FEEBAS,     // 328 SPECIES_FEEBAS
    LEARNSET_MILOTIC,    // 329 SPECIES_MILOTIC
    LEARNSET_CARVANHA,   // 330 SPECIES_CARVANHA
    LEARNSET_SHARPEDO,   // 331 SPECIES_SHARPEDO
    LEARNSET_TRAPINCH,   // 332 SPECIES_TRAPINCH
    LEARNSET_VIBRAVA,    // 333 SPECIES_VIBRAVA
    LEARNSET_FLYGON,     // 334 SPECIES_FLYGON
    LEARNSET_MAKUHITA,   // 335 SPECIES_MAKUHITA
    LEARNSET_HARIYAMA,   // 336 SPECIES_HARIYAMA
    LEARNSET_ELECTRIKE,  // 337 SPECIES_ELECTRIKE
    LEARNSET_MANECTRIC,  // 338 SPECIES_MANECTRIC
    LEARNSET_NUMEL,      // 339 SPECIES_NUMEL
    LEARNSET_CAMERUPT,   // 340 SPECIES_CAMERUPT
    LEARNSET_SPHEAL,     // 341 SPECIES_SPHEAL
    LEARNSET_SEALEO,     // 342 SPECIES_SEALEO
    LEARNSET_WALREIN,    // 343 SPECIES_WALREIN
    LEARNSET_CACNEA,     // 344 SPECIES_CACNEA
    LEARNSET_CACTURNE,   // 345 SPECIES_CACTURNE
    LEARNSET_SNORUNT,    // 346 SPECIES_SNORUNT
    LEARNSET_GLALIE,     // 347 SPECIES_GLALIE
    LEARNSET_LUNATONE,   // 348 SPECIES_LUNATONE
    LEARNSET_SOLROCK,    // 349 SPECIES_SOLROCK
    LEARNSET_AZURILL,    // 350 SPECIES_AZURILL
    LEARNSET_SPOINK,     // 351 SPECIES_SPOINK
    LEARNSET_GRUMPIG,    // 352 SPECIES_GRUMPIG
    LEARNSET_PLUSLE,     // 353 SPECIES_PLUSLE
    LEARNSET_MINUN,      // 354 SPECIES_MINUN
    LEARNSET_MAWILE,     // 355 SPECIES_MAWILE
    LEARNSET_MEDITITE,   // 356 SPECIES_MEDITITE
    LEARNSET_MEDICHAM,   // 357 SPECIES_MEDICHAM
    LEARNSET_SWABLU,     // 358 SPECIES_SWABLU
    LEARNSET_ALTARIA,    // 359 SPECIES_ALTARIA
    LEARNSET_WYNAUT,     // 360 SPECIES_WYNAUT
    LEARNSET_DUSKULL,    // 361 SPECIES_DUSKULL
    LEARNSET_DUSCLOPS,   // 362 SPECIES_DUSCLOPS
    LEARNSET_ROSELIA,    // 363 SPECIES_ROSELIA
    LEARNSET_SLAKOTH,    // 364 SPECIES_SLAKOTH
    LEARNSET_VIGOROTH,   // 365 SPECIES_VIGOROTH
    LEARNSET_SLAKING,    // 366 SPECIES_SLAKING
    LEARNSET_GULPIN,     // 367 SPECIES_GULPIN
    LEARNSET_SWALOT,     // 368 SPECIES_SWALOT
    LEARNSET_TROPIUS,    // 369 SPECIES_TROPIUS
    LEARNSET_WHISMUR,    // 370 SPECIES_WHISMUR
    LEARNSET_LOUDRED,    // 371 SPECIES_LOUDRED
    LEARNSET_EXPLOUD,    // 372 SPECIES_EXPLOUD
    LEARNSET_CLAMPERL,   // 373 SPECIES_CLAMPERL
    LEARNSET_HUNTAIL,    // 374 SPECIES_HUNTAIL
    LEARNSET_GOREBYSS,   // 375 SPECIES_GOREBYSS
    LEARNSET_ABSOL,      // 376 SPECIES_ABSOL
    LEARNSET_SHUPPET,    // 377 SPECIES_SHUPPET
    LEARNSET_BANETTE,    // 378 SPECIES_BANETTE
    LEARNSET_SEVIPER,    // 379 SPECIES_SEVIPER
    LEARNSET_ZANGOOSE,   // 380 SPECIES_ZANGOOSE
    LEARNSET_RELICANTH,  // 381 SPECIES_RELICANTH
    LEARNSET_ARON,       // 382 SPECIES_ARON
    LEARNSET_LAIRON,     // 383 SPECIES_LAIRON
    LEARNSET_AGGRON,     // 384 SPECIES_AGGRON
    LEARNSET_CASTFORM,   // 385 SPECIES_CASTFORM
    LEARNSET_VOLBEAT,    // 386 SPECIES_VOLBEAT
    LEARNSET_ILLUMISE,   // 387 SPECIES_ILLUMISE
    LEARNSET_LILEEP,     // 388 SPECIES_LILEEP
    LEARNSET_CRADILY,    // 389 SPECIES_CRADILY
    LEARNSET_ANORITH,    // 390 SPECIES_ANORITH
    LEARNSET_ARMALDO,    // 391 SPECIES_ARMALDO
    LEARNSET_RALTS,      // 392 SPECIES_RALTS
    LEARNSET_KIRLIA,     // 393 SPECIES_KIRLIA
    LEARNSET_GARDEVOIR,  // 394 SPECIES_GARDEVOIR
    LEARNSET_BAGON,      // 395 SPECIES_BAGON
    LEARNSET_SHELGON,    // 396 SPECIES_SHELGON
    LEARNSET_SALAMENCE,  // 397 SPECIES_SALAMENCE
    LEARNSET_BELDUM,     // 398 SPECIES_BELDUM
    LEARNSET_METANG,     // 399 SPECIES_METANG
    LEARNSET_METAGROSS,  // 400 SPECIES_METAGROSS
    LEARNSET_REGIROCK,   // 401 SPECIES_REGIROCK
    LEARNSET_REGICE,     // 402 SPECIES_REGICE
    LEARNSET_REGISTEEL,  // 403 SPECIES_REGISTEEL
    LEARNSET_KYOGRE,     // 404 SPECIES_KYOGRE
    LEARNSET_GROUDON,    // 405 SPECIES_GROUDON
    LEARNSET_RAYQUAZA,   // 406 SPECIES_RAYQUAZA
    LEARNSET_LATIAS,     // 407 SPECIES_LATIAS
    LEARNSET_LATIOS,     // 408 SPECIES_LATIOS
    LEARNSET_JIRACHI,    // 409 SPECIES_JIRACHI
    LEARNSET_DEOXYS,     // 410 SPECIES_DEOXYS
    LEARNSET_CHIMECHO,   // 411 SPECIES_CHIMECHO
];

/// The `gLevelUpLearnsets` table: owned, read-only access to every species'
/// ordered level-up learnset with typed lookup `(oop-boundaries)`. Holds no
/// mutable state — construct one and query it.
#[derive(Debug, Clone, Copy)]
pub struct LevelUpLearnsets {
    table: &'static [&'static [LevelUpMove]; SPECIES_COUNT],
}

impl LevelUpLearnsets {
    /// The number of addressable species slots (`SPECIES_COUNT`), including the
    /// `SPECIES_NONE` slot at index `0`.
    pub const LEN: usize = SPECIES_COUNT;

    /// [`LevelUpLearnsets::LEN`] as a [`u16`], the width of a [`SpeciesId`].
    ///
    /// The table has far fewer than `u16::MAX` entries, so this conversion is
    /// exact; the `assert!` makes that a compile-time fact.
    pub const LEN_U16: u16 = {
        assert!(SPECIES_COUNT <= u16::MAX as usize);
        #[allow(clippy::cast_possible_truncation)]
        {
            SPECIES_COUNT as u16
        }
    };

    /// Build the table over the extracted upstream data.
    #[must_use]
    pub const fn new() -> Self {
        Self { table: &LEARNSETS }
    }

    /// The ordered level-up learnset for `species`, or `None` if the id is out
    /// of range.
    #[must_use]
    pub fn get(&self, species: SpeciesId) -> Option<&'static [LevelUpMove]> {
        self.table.get(species.0 as usize).copied()
    }

    /// The ordered level-up learnset for `species`, in upstream order.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownSpecies`] if `species` is outside the
    /// extracted range `0..`[`LevelUpLearnsets::LEN`].
    pub fn learnset(&self, species: SpeciesId) -> Result<&'static [LevelUpMove], AssetError> {
        self.get(species)
            .ok_or(AssetError::UnknownSpecies(species.0))
    }

    /// The number of species slots in the table (`SPECIES_COUNT`).
    #[must_use]
    pub const fn len(&self) -> usize {
        SPECIES_COUNT
    }

    /// Always `false` — the table is never empty. Present for API convention.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }

    /// Iterate over every species' learnset in `SPECIES_*` id order.
    pub fn iter(&self) -> impl Iterator<Item = &'static [LevelUpMove]> + '_ {
        self.table.iter().copied()
    }
}

impl Default for LevelUpLearnsets {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{LevelUpLearnsets, LevelUpMove, SPECIES_COUNT};
    use crate::battle_moves::MoveId;
    use crate::error::AssetError;
    use crate::species::SpeciesId;

    // National-dex ids of species pinned by the tie tests.
    const NONE: SpeciesId = SpeciesId(0);
    const BULBASAUR: SpeciesId = SpeciesId(1);
    const PIKACHU: SpeciesId = SpeciesId(25);
    const CHIMECHO: SpeciesId = SpeciesId(411);

    // Raw upstream move ids used below (from `include/constants/moves.h`).
    const MOVE_TACKLE: u16 = 33;
    const MOVE_GROWL: u16 = 45;
    const MOVE_LEECH_SEED: u16 = 73;
    const MOVE_VINE_WHIP: u16 = 22;

    #[test]
    fn table_length_matches_num_species() {
        // Structural anchor: upstream `gLevelUpLearnsets[NUM_SPECIES]`,
        // NUM_SPECIES == 412 (ids 0..=411).
        let ls = LevelUpLearnsets::new();
        assert_eq!(SPECIES_COUNT, 412);
        assert_eq!(ls.len(), 412);
        assert_eq!(LevelUpLearnsets::LEN, 412);
        assert_eq!(ls.iter().count(), 412);
        assert!(!ls.is_empty());
    }

    #[test]
    fn upstream_tie_bulbasaur() {
        // The full Bulbasaur learnset, transcribed straight from
        // `level_up_learnsets.h` (sBulbasaurLevelUpLearnset), in order.
        let ls = LevelUpLearnsets::new();
        let got = ls.learnset(BULBASAUR).unwrap();
        let expected: &[(u8, u16)] = &[
            (1, MOVE_TACKLE),
            (4, MOVE_GROWL),
            (7, MOVE_LEECH_SEED),
            (10, MOVE_VINE_WHIP),
            (15, 77),  // MOVE_POISON_POWDER
            (15, 79),  // MOVE_SLEEP_POWDER
            (20, 75),  // MOVE_RAZOR_LEAF
            (25, 230), // MOVE_SWEET_SCENT
            (32, 74),  // MOVE_GROWTH
            (39, 235), // MOVE_SYNTHESIS
            (46, 76),  // MOVE_SOLAR_BEAM
        ];
        assert_eq!(got.len(), expected.len());
        for (entry, &(lvl, mv)) in got.iter().zip(expected) {
            assert_eq!(
                *entry,
                LevelUpMove {
                    level: lvl,
                    move_id: MoveId(mv)
                }
            );
        }
    }

    #[test]
    fn upstream_tie_species_none_aliases_bulbasaur() {
        // Upstream `[SPECIES_NONE] = sBulbasaurLevelUpLearnset`: the NONE slot
        // shares Bulbasaur's array. Transcribed faithfully.
        let ls = LevelUpLearnsets::new();
        assert_eq!(ls.learnset(NONE).unwrap(), ls.learnset(BULBASAUR).unwrap());
        assert_eq!(ls.learnset(NONE).unwrap()[0].move_id, MoveId(MOVE_TACKLE));
    }

    #[test]
    fn upstream_tie_pikachu() {
        // sPikachuLevelUpLearnset, in order — note the two level-1 moves.
        let ls = LevelUpLearnsets::new();
        let got = ls.learnset(PIKACHU).unwrap();
        let expected: &[(u8, u16)] = &[
            (1, 84), // MOVE_THUNDER_SHOCK
            (1, MOVE_GROWL),
            (6, 39),   // MOVE_TAIL_WHIP
            (8, 86),   // MOVE_THUNDER_WAVE
            (11, 98),  // MOVE_QUICK_ATTACK
            (15, 104), // MOVE_DOUBLE_TEAM
            (20, 21),  // MOVE_SLAM
            (26, 85),  // MOVE_THUNDERBOLT
            (33, 97),  // MOVE_AGILITY
            (41, 87),  // MOVE_THUNDER
            (50, 113), // MOVE_LIGHT_SCREEN
        ];
        assert_eq!(got.len(), expected.len());
        for (entry, &(lvl, mv)) in got.iter().zip(expected) {
            assert_eq!(
                *entry,
                LevelUpMove {
                    level: lvl,
                    move_id: MoveId(mv)
                }
            );
        }
    }

    #[test]
    fn upstream_tie_chimecho_last_species() {
        // The final species slot (id 411), sChimechoLevelUpLearnset.
        let ls = LevelUpLearnsets::new();
        let got = ls.learnset(CHIMECHO).unwrap();
        let expected: &[(u8, u16)] = &[
            (1, 35), // MOVE_WRAP
            (6, MOVE_GROWL),
            (9, 310),  // MOVE_ASTONISH
            (14, 93),  // MOVE_CONFUSION
            (17, 36),  // MOVE_TAKE_DOWN
            (22, 253), // MOVE_UPROAR
            (25, 281), // MOVE_YAWN
            (30, 149), // MOVE_PSYWAVE
            (33, 38),  // MOVE_DOUBLE_EDGE
            (38, 215), // MOVE_HEAL_BELL
            (41, 219), // MOVE_SAFEGUARD
            (46, 94),  // MOVE_PSYCHIC
        ];
        assert_eq!(got.len(), expected.len());
        for (entry, &(lvl, mv)) in got.iter().zip(expected) {
            assert_eq!(
                *entry,
                LevelUpMove {
                    level: lvl,
                    move_id: MoveId(mv)
                }
            );
        }
    }

    #[test]
    fn out_of_range_species_is_an_error() {
        let ls = LevelUpLearnsets::new();
        let bad = SpeciesId(LevelUpLearnsets::LEN_U16);
        assert_eq!(ls.get(bad), None);
        assert_eq!(
            ls.learnset(bad),
            Err(AssetError::UnknownSpecies(LevelUpLearnsets::LEN_U16)),
        );
        assert_eq!(
            ls.learnset(SpeciesId(u16::MAX)),
            Err(AssetError::UnknownSpecies(u16::MAX)),
        );
    }

    #[test]
    fn every_entry_round_trips_through_the_packed_encoding() {
        // Guard the decoding against the real upstream `LEVEL_UP_MOVE` packing:
        // re-packing (level << 9) | move must reproduce a value whose fields
        // decode back unchanged, no entry may collide with the LEVEL_UP_END
        // sentinel (0xFFFF), and every level fits the 7 top bits (<= 127).
        let ls = LevelUpLearnsets::new();
        for entry in ls.iter().flatten() {
            let packed = entry.packed();
            assert_ne!(packed, 0xFFFF, "entry collides with LEVEL_UP_END");
            assert!(entry.level <= 127, "level {} exceeds 7 bits", entry.level);
            // Decoding the re-packed value must reproduce the original fields.
            assert_eq!(LevelUpMove::from_packed(packed), *entry);
        }
    }

    #[test]
    fn learnsets_are_non_empty_and_level_ordered() {
        // Behavioural guard: every species has at least one level-up move, and
        // upstream lists them in non-decreasing level order (the game relies on
        // this when scanning for newly-learnable moves).
        let ls = LevelUpLearnsets::new();
        for id in 0..LevelUpLearnsets::LEN_U16 {
            let entries = ls.learnset(SpeciesId(id)).unwrap();
            assert!(!entries.is_empty(), "species {id} has an empty learnset");
            for pair in entries.windows(2) {
                assert!(
                    pair[0].level <= pair[1].level,
                    "species {id} learnset not level-ordered: {} then {}",
                    pair[0].level,
                    pair[1].level,
                );
            }
        }
    }
}
