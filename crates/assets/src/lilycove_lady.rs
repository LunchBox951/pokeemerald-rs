//! The stock Lilycove Lady data sets: the Quiz Lady questions, answers and
//! prizes, the Favor Lady requests, accepted-item lists and prizes, the
//! Contest Lady category records, and the three Lady sprite IDs.
//!
//! Source: `src/data/lilycove_lady.h`, with the display text from
//! `src/strings.c:615-619` and `src/strings.c:1595-1611`. Selection, save
//! state and record mixing (`src/lilycove_lady.c`) are not part of this data.
//!
//! Easy Chat words (`EC_WORD_*`, `EC_MOVE2`, `include/constants/easy_chat.h`)
//! and `OBJ_EVENT_GFX_*` sprite IDs stay plain `u16` values: this crate holds
//! no typed table for either.

use std::error::Error;
use std::fmt;

use crate::items::ItemId;
use crate::species::SpeciesId;

/// The number of Easy Chat words in a quiz question (`QUIZ_QUESTION_LEN`).
pub const QUIZ_QUESTION_LEN: usize = 9;

/// The number of stock quizzes (`ARRAY_COUNT(sQuizLadyQuizQuestions)`).
pub const QUIZ_COUNT: usize = 16;

/// The number of Favor Lady requests (`ARRAY_COUNT(sFavorLadyRequests)`).
pub const FAVOR_REQUEST_COUNT: usize = 6;

/// The number of contest categories (`CONTEST_CATEGORIES_COUNT`).
pub const CONTEST_CATEGORY_COUNT: usize = 5;

/// The number of Lilycove Lady kinds (`LILYCOVE_LADY_COUNT`).
pub const LILYCOVE_LADY_COUNT: usize = 3;

/// The blank Easy Chat slot (`EC_EMPTY_WORD`).
pub const EC_EMPTY_WORD: u16 = 0xFFFF;

/// A failed lookup into the Lilycove Lady data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LilycoveLadyError {
    /// The index is not below [`QUIZ_COUNT`].
    UnknownQuiz(usize),
    /// The index is not below [`FAVOR_REQUEST_COUNT`].
    UnknownFavorRequest(usize),
    /// The value is not a `CONTEST_CATEGORY_*` ID.
    UnknownContestCategory(u8),
    /// The value is not a `LILYCOVE_LADY_*` kind.
    UnknownLady(u8),
}

impl fmt::Display for LilycoveLadyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownQuiz(index) => write!(f, "unknown quiz index `{index}`"),
            Self::UnknownFavorRequest(index) => {
                write!(f, "unknown favor request index `{index}`")
            }
            Self::UnknownContestCategory(id) => write!(f, "unknown contest category `{id}`"),
            Self::UnknownLady(id) => write!(f, "unknown Lilycove Lady kind `{id}`"),
        }
    }
}

impl Error for LilycoveLadyError {}

/// One stock quiz: question, correct answer and prize at the same upstream
/// index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuizLadyQuiz {
    /// The question words (`sQuizLadyQuizQuestions[n]`).
    pub question: [u16; QUIZ_QUESTION_LEN],
    /// The correct answer word (`sQuizLadyQuizAnswers[n]`).
    pub answer: u16,
    /// The prize item (`sQuizLadyPrizes[n]`).
    pub prize: ItemId,
}

/// One Favor Lady request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FavorLadyRequest {
    /// The request adjective (`sFavorLadyRequests[n]`).
    pub text: &'static str,
    /// The accepted items in upstream order, ending with the [`ItemId::NONE`]
    /// terminator (`sFavorLadyAcceptedItemLists[n]`).
    pub accepted_items: &'static [ItemId],
    /// The prize item (`sFavorLadyPrizes[n]`).
    pub prize: ItemId,
}

impl FavorLadyRequest {
    /// Returns the accepted items without the terminator.
    #[must_use]
    pub fn items(&self) -> &'static [ItemId] {
        self.accepted_items
            .split_last()
            .map_or(&[], |(_, items)| items)
    }
}

/// A contest category (`CONTEST_CATEGORY_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContestCategory {
    /// `CONTEST_CATEGORY_COOL`.
    Cool,
    /// `CONTEST_CATEGORY_BEAUTY`.
    Beauty,
    /// `CONTEST_CATEGORY_CUTE`.
    Cute,
    /// `CONTEST_CATEGORY_SMART`.
    Smart,
    /// `CONTEST_CATEGORY_TOUGH`.
    Tough,
}

impl ContestCategory {
    /// Decodes an upstream `CONTEST_CATEGORY_*` value.
    ///
    /// # Errors
    ///
    /// Returns [`LilycoveLadyError::UnknownContestCategory`] for any other
    /// value.
    pub const fn from_id(id: u8) -> Result<Self, LilycoveLadyError> {
        match id {
            0 => Ok(Self::Cool),
            1 => Ok(Self::Beauty),
            2 => Ok(Self::Cute),
            3 => Ok(Self::Smart),
            4 => Ok(Self::Tough),
            _ => Err(LilycoveLadyError::UnknownContestCategory(id)),
        }
    }

    /// Returns the upstream `CONTEST_CATEGORY_*` value.
    #[must_use]
    pub const fn id(self) -> u8 {
        self as u8
    }
}

/// The Contest Lady record for one category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContestLadyRecord {
    /// The category the record is for.
    pub category: ContestCategory,
    /// The Pokémon's nickname (`sContestLadyMonNames`).
    pub mon_name: &'static str,
    /// The category word used in her dialogue (`sContestLadyCategoryNames`).
    pub category_name: &'static str,
    /// The contest's name (`sContestNames`).
    pub contest_name: &'static str,
    /// The Pokémon's species (`sContestLadyMonSpecies`).
    pub species: SpeciesId,
    /// The Pokémon's overworld sprite (`sContestLadyMonGfxId`).
    pub mon_gfx_id: u16,
}

/// A Lilycove Lady activity (`LILYCOVE_LADY_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LilycoveLadyKind {
    /// `LILYCOVE_LADY_QUIZ`.
    Quiz,
    /// `LILYCOVE_LADY_FAVOR`.
    Favor,
    /// `LILYCOVE_LADY_CONTEST`.
    Contest,
}

impl LilycoveLadyKind {
    /// Decodes an upstream `LILYCOVE_LADY_*` value.
    ///
    /// # Errors
    ///
    /// Returns [`LilycoveLadyError::UnknownLady`] for any other value.
    pub const fn from_id(id: u8) -> Result<Self, LilycoveLadyError> {
        match id {
            0 => Ok(Self::Quiz),
            1 => Ok(Self::Favor),
            2 => Ok(Self::Contest),
            _ => Err(LilycoveLadyError::UnknownLady(id)),
        }
    }

    /// Returns the upstream `LILYCOVE_LADY_*` value.
    #[must_use]
    pub const fn id(self) -> u8 {
        self as u8
    }
}

/// Lookup over the stock Lilycove Lady data.
#[derive(Debug, Clone, Copy, Default)]
pub struct LilycoveLadyData;

impl LilycoveLadyData {
    /// Creates a handle on the canonical data.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Returns every quiz in upstream order.
    #[must_use]
    pub const fn quizzes(&self) -> &'static [QuizLadyQuiz; QUIZ_COUNT] {
        &QUIZZES
    }

    /// Returns the quiz at `index`.
    ///
    /// # Errors
    ///
    /// Returns [`LilycoveLadyError::UnknownQuiz`] when `index` is
    /// [`QUIZ_COUNT`] or more.
    pub fn quiz(&self, index: usize) -> Result<&'static QuizLadyQuiz, LilycoveLadyError> {
        QUIZZES
            .get(index)
            .ok_or(LilycoveLadyError::UnknownQuiz(index))
    }

    /// Returns every Favor Lady request in upstream order.
    #[must_use]
    pub const fn favor_requests(&self) -> &'static [FavorLadyRequest; FAVOR_REQUEST_COUNT] {
        &FAVOR_REQUESTS
    }

    /// Returns the Favor Lady request at `index`.
    ///
    /// # Errors
    ///
    /// Returns [`LilycoveLadyError::UnknownFavorRequest`] when `index` is
    /// [`FAVOR_REQUEST_COUNT`] or more.
    pub fn favor_request(
        &self,
        index: usize,
    ) -> Result<&'static FavorLadyRequest, LilycoveLadyError> {
        FAVOR_REQUESTS
            .get(index)
            .ok_or(LilycoveLadyError::UnknownFavorRequest(index))
    }

    /// Returns every Contest Lady record in category order.
    #[must_use]
    pub const fn contest_records(&self) -> &'static [ContestLadyRecord; CONTEST_CATEGORY_COUNT] {
        &CONTEST_RECORDS
    }

    /// Returns the Contest Lady record for `category`.
    #[must_use]
    pub const fn contest_record(&self, category: ContestCategory) -> &'static ContestLadyRecord {
        &CONTEST_RECORDS[category as usize]
    }

    /// Returns the `OBJ_EVENT_GFX_*` sprite ID of `lady` (`sLilycoveLadyGfxId`).
    #[must_use]
    pub const fn lady_gfx_id(&self, lady: LilycoveLadyKind) -> u16 {
        LADY_GFX_IDS[lady as usize]
    }
}

const LADY_GFX_IDS: [u16; LILYCOVE_LADY_COUNT] = [
    26, // OBJ_EVENT_GFX_WOMAN_4
    20, // OBJ_EVENT_GFX_WOMAN_2
    10, // OBJ_EVENT_GFX_GIRL_2
];

const CONTEST_RECORDS: [ContestLadyRecord; CONTEST_CATEGORY_COUNT] = [
    ContestLadyRecord {
        category: ContestCategory::Cool,
        mon_name: "HANDSOME",
        category_name: "coolness",
        contest_name: "COOLNESS CONTEST",
        species: SpeciesId::ZIGZAGOON,
        mon_gfx_id: 98, // OBJ_EVENT_GFX_ZIGZAGOON_1
    },
    ContestLadyRecord {
        category: ContestCategory::Beauty,
        mon_name: "VINNY",
        category_name: "beauty",
        contest_name: "BEAUTY CONTEST",
        species: SpeciesId::SKITTY,
        mon_gfx_id: 203, // OBJ_EVENT_GFX_SKITTY
    },
    ContestLadyRecord {
        category: ContestCategory::Cute,
        mon_name: "MOREME",
        category_name: "cuteness",
        contest_name: "CUTENESS CONTEST",
        species: SpeciesId::POOCHYENA,
        mon_gfx_id: 220, // OBJ_EVENT_GFX_POOCHYENA
    },
    ContestLadyRecord {
        category: ContestCategory::Smart,
        mon_name: "IRONHARD",
        category_name: "smartness",
        contest_name: "SMARTNESS CONTEST",
        species: SpeciesId::KECLEON,
        mon_gfx_id: 204, // OBJ_EVENT_GFX_KECLEON
    },
    ContestLadyRecord {
        category: ContestCategory::Tough,
        mon_name: "MUSCLE",
        category_name: "toughness",
        contest_name: "TOUGHNESS CONTEST",
        species: SpeciesId::PIKACHU,
        mon_gfx_id: 209, // OBJ_EVENT_GFX_PIKACHU
    },
];

const QUIZZES: [QuizLadyQuiz; QUIZ_COUNT] = [
    QuizLadyQuiz {
        question: [
            0x101B, // WHICH
            0x1623, // STORES
            0x1812, // INFORMATION
            0x102C, // ON
            0x020E, // POKEMON
            0x0C03, // QUES
            0x1A0B, // CAMERA
            0x0210, // POKEDEX
            0x020D, // POKENAV
        ],
        answer: 0x0210, // POKEDEX
        prize: ItemId::GLITTER_MAIL,
    },
    QuizLadyQuiz {
        question: [
            0x101B, // WHICH
            0x1013, // ISN_T
            0x1020, // A
            0x1A0F, // GAME
            0x020C, // VERSION
            0x0C03, // QUES
            0x0211, // RUBY
            0x0203, // SAPPHIRE
            0x0400, // DARK
        ],
        answer: 0x0400, // DARK
        prize: ItemId::BEAD_MAIL,
    },
    QuizLadyQuiz {
        question: [
            0x0E0F, // HOW
            0x1018, // DO
            0x020E, // POKEMON
            0x0204, // EVOLVE
            0x0C03, // QUES
            0xFFFF, // EMPTY
            0x0212, // LEVEL
            0x0451, // INSOMNIA
            0x0463, // CUTE_CHARM
        ],
        answer: 0x0212, // LEVEL
        prize: ItemId::TROPIC_MAIL,
    },
    QuizLadyQuiz {
        question: [
            0x101B, // WHICH
            0x100B, // IS
            0x0E0D, // THE
            0x141A, // PRETTY
            0x181D, // ITEM
            0x0C03, // QUES
            0x141E, // COLD
            0x1A26, // FLOWERS
            0x1823, // MACHINE
        ],
        answer: 0x1A26, // FLOWERS
        prize: ItemId::MAX_ETHER,
    },
    QuizLadyQuiz {
        question: [
            0x101B, // WHICH
            0x181D, // ITEM
            0x1018, // DO
            0x0A02, // YOU
            0x2014, // BREAK
            0x0C03, // QUES
            0x0208, // EGG
            0x1824, // MAIL
            0x181C, // PHONE
        ],
        answer: 0x0208, // EGG
        prize: ItemId::MAX_ETHER,
    },
    QuizLadyQuiz {
        question: [
            0x101B, // WHICH
            0x1000, // WILL
            0x1C19, // STOP
            0x265D, // MOVE2_CONFUSION
            0x0C03, // QUES
            0xFFFF, // EMPTY
            0x0447, // ILLUMINATE
            0x045D, // OWN_TEMPO
            0x042C, // SWIFT_SWIM
        ],
        answer: 0x045D, // OWN_TEMPO
        prize: ItemId::MAX_ETHER,
    },
    QuizLadyQuiz {
        question: [
            0x101B, // WHICH
            0x1034, // OF
            0x1E11, // THESE
            0x100B, // IS
            0x1A08, // MUSIC
            0x0C03, // QUES
            0x044B, // FLYING
            0x0446, // STEEL
            0x040A, // ROCK
        ],
        answer: 0x040A, // ROCK
        prize: ItemId::WATMEL_BERRY,
    },
    QuizLadyQuiz {
        question: [
            0x101B, // WHICH
            0x1000, // WILL
            0x274F, // MOVE2_BLOCK
            0x0626, // ESCAPE
            0x0C03, // QUES
            0xFFFF, // EMPTY
            0x0442, // RUN_AWAY
            0x0411, // SHADOW_TAG
            0x0450, // WONDER_GUARD
        ],
        answer: 0x0411, // SHADOW_TAG
        prize: ItemId::BELUE_BERRY,
    },
    QuizLadyQuiz {
        question: [
            0x101B, // WHICH
            0x1000, // WILL
            0x1C19, // STOP
            0x043C, // POISON
            0x0C03, // QUES
            0xFFFF, // EMPTY
            0x0421, // GUTS
            0x0464, // IMMUNITY
            0x0435, // SHED_SKIN
        ],
        answer: 0x0464, // IMMUNITY
        prize: ItemId::DURIN_BERRY,
    },
    QuizLadyQuiz {
        question: [
            0x101B, // WHICH
            0x1203, // GOES
            0x1030, // WITH
            0x0207, // CENTER
            0x0C03, // QUES
            0xFFFF, // EMPTY
            0x0210, // POKEDEX
            0x020E, // POKEMON
            0x020D, // POKENAV
        ],
        answer: 0x020E, // POKEMON
        prize: ItemId::LUXURY_BALL,
    },
    QuizLadyQuiz {
        question: [
            0x101B, // WHICH
            0x1623, // STORES
            0x0A05, // YOUR
            0x020E, // POKEMON
            0x0C03, // QUES
            0xFFFF, // EMPTY
            0x1A25, // PC
            0x181A, // DEPT_STORE
            0x181B, // TELEVISION
        ],
        answer: 0x1A25, // PC
        prize: ItemId::TM_HYPER_BEAM,
    },
    QuizLadyQuiz {
        question: [
            0x101B, // WHICH
            0x1823, // MACHINE
            0x1603, // GIVES
            0x0A02, // YOU
            0x1812, // INFORMATION
            0x0C03, // QUES
            0x1A15, // BIKE
            0x1A23, // LOCOMOTIVE
            0x181B, // TELEVISION
        ],
        answer: 0x181B, // TELEVISION
        prize: ItemId::BIG_PEARL,
    },
    QuizLadyQuiz {
        question: [
            0x1020, // A
            0x020E, // POKEMON
            0x1010, // WAS
            0x1043, // ONCE
            0x1E0F, // THIS
            0x0C03, // QUES
            0x181C, // PHONE
            0x1A24, // PLUSH_DOLL
            0x1816, // LETTER
        ],
        answer: 0x1A24, // PLUSH_DOLL
        prize: ItemId::STAR_PIECE,
    },
    QuizLadyQuiz {
        question: [
            0x0446, // STEEL
            0x100B, // IS
            0x0620, // STRONG
            0x061C, // VERSUS
            0x101B, // WHICH
            0x0C03, // QUES
            0x0420, // ICE
            0x0426, // GROUND
            0xFFFF, // EMPTY
        ],
        answer: 0x0420, // ICE
        prize: ItemId::RARE_CANDY,
    },
    QuizLadyQuiz {
        question: [
            0x0400, // DARK
            0x100B, // IS
            0x0639, // WEAK
            0x061C, // VERSUS
            0x101B, // WHICH
            0x0C03, // QUES
            0x040E, // PSYCHIC
            0x0410, // FIGHTING
            0xFFFF, // EMPTY
        ],
        answer: 0x0410, // FIGHTING
        prize: ItemId::RARE_CANDY,
    },
    QuizLadyQuiz {
        question: [
            0x041F, // GHOST
            0x100B, // IS
            0x0639, // WEAK
            0x061C, // VERSUS
            0x101B, // WHICH
            0x0C03, // QUES
            0x0445, // NORMAL
            0x0400, // DARK
            0xFFFF, // EMPTY
        ],
        answer: 0x0400, // DARK
        prize: ItemId::PREMIER_BALL,
    },
];

const FAVOR_REQUESTS: [FavorLadyRequest; FAVOR_REQUEST_COUNT] = [
    FavorLadyRequest {
        text: "slippery",
        accepted_items: &[
            ItemId::REPEL,
            ItemId::SUPER_REPEL,
            ItemId::MAX_REPEL,
            ItemId::ANTIDOTE,
            ItemId::PARALYZE_HEAL,
            ItemId::BURN_HEAL,
            ItemId::BELUE_BERRY,
            ItemId::AWAKENING,
            ItemId::ICE_HEAL,
            ItemId::REVIVE,
            ItemId::MAX_REVIVE,
            ItemId::ENERGY_POWDER,
            ItemId::NONE,
        ],
        prize: ItemId::LUXURY_BALL,
    },
    FavorLadyRequest {
        text: "roundish",
        accepted_items: &[
            ItemId::FLUFFY_TAIL,
            ItemId::PEARL,
            ItemId::BIG_PEARL,
            ItemId::HARD_STONE,
            ItemId::SMOKE_BALL,
            ItemId::SHOAL_SHELL,
            ItemId::TINY_MUSHROOM,
            ItemId::BIG_MUSHROOM,
            ItemId::PECHA_BERRY,
            ItemId::ASPEAR_BERRY,
            ItemId::ORAN_BERRY,
            ItemId::GREPA_BERRY,
            ItemId::MAGOST_BERRY,
            ItemId::WATMEL_BERRY,
            ItemId::POKE_BALL,
            ItemId::ULTRA_BALL,
            ItemId::NONE,
        ],
        prize: ItemId::NUGGET,
    },
    FavorLadyRequest {
        text: "wham-ish",
        accepted_items: &[
            ItemId::REVIVAL_HERB,
            ItemId::POTION,
            ItemId::FRESH_WATER,
            ItemId::SODA_POP,
            ItemId::LEMONADE,
            ItemId::HARD_STONE,
            ItemId::LIGHT_BALL,
            ItemId::LAVA_COOKIE,
            ItemId::CHESTO_BERRY,
            ItemId::NANAB_BERRY,
            ItemId::WEPEAR_BERRY,
            ItemId::KELPSY_BERRY,
            ItemId::NOMEL_BERRY,
            ItemId::DURIN_BERRY,
            ItemId::NONE,
        ],
        prize: ItemId::PROTEIN,
    },
    FavorLadyRequest {
        text: "shiny",
        accepted_items: &[
            ItemId::HEAL_POWDER,
            ItemId::X_SPEED,
            ItemId::X_ATTACK,
            ItemId::X_DEFEND,
            ItemId::BLUE_FLUTE,
            ItemId::YELLOW_FLUTE,
            ItemId::RED_FLUTE,
            ItemId::BLACK_FLUTE,
            ItemId::WHITE_FLUTE,
            ItemId::NUGGET,
            ItemId::SUN_STONE,
            ItemId::STARDUST,
            ItemId::STAR_PIECE,
            ItemId::PEARL,
            ItemId::BIG_PEARL,
            ItemId::TWISTED_SPOON,
            ItemId::SILVER_POWDER,
            ItemId::BRIGHT_POWDER,
            ItemId::LUXURY_BALL,
            ItemId::PREMIER_BALL,
            ItemId::NONE,
        ],
        prize: ItemId::HEART_SCALE,
    },
    FavorLadyRequest {
        text: "sticky",
        accepted_items: &[
            ItemId::ENERGY_ROOT,
            ItemId::FULL_RESTORE,
            ItemId::MAX_POTION,
            ItemId::DIRE_HIT,
            ItemId::X_ACCURACY,
            ItemId::GUARD_SPEC,
            ItemId::WATMEL_BERRY,
            ItemId::LEFTOVERS,
            ItemId::TINY_MUSHROOM,
            ItemId::HEART_SCALE,
            ItemId::NONE,
        ],
        prize: ItemId::RARE_CANDY,
    },
    FavorLadyRequest {
        text: "pointy",
        accepted_items: &[
            ItemId::QUICK_CLAW,
            ItemId::POISON_BARB,
            ItemId::SHARP_BEAK,
            ItemId::DRAGON_FANG,
            ItemId::TAMATO_BERRY,
            ItemId::DURIN_BERRY,
            ItemId::PETAYA_BERRY,
            ItemId::SALAC_BERRY,
            ItemId::STARDUST,
            ItemId::STAR_PIECE,
            ItemId::NONE,
        ],
        prize: ItemId::PP_MAX,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Easy Chat words used by the stock quizzes as (name, group, index).
    const WORDS: &[(&str, u16, u16)] = &[
        ("SAPPHIRE", 1, 3),
        ("EVOLVE", 1, 4),
        ("CENTER", 1, 7),
        ("EGG", 1, 8),
        ("VERSION", 1, 12),
        ("POKENAV", 1, 13),
        ("POKEMON", 1, 14),
        ("POKEDEX", 1, 16),
        ("RUBY", 1, 17),
        ("LEVEL", 1, 18),
        ("DARK", 2, 0),
        ("ROCK", 2, 10),
        ("PSYCHIC", 2, 14),
        ("FIGHTING", 2, 16),
        ("SHADOW_TAG", 2, 17),
        ("GHOST", 2, 31),
        ("ICE", 2, 32),
        ("GUTS", 2, 33),
        ("GROUND", 2, 38),
        ("SWIFT_SWIM", 2, 44),
        ("SHED_SKIN", 2, 53),
        ("POISON", 2, 60),
        ("RUN_AWAY", 2, 66),
        ("NORMAL", 2, 69),
        ("STEEL", 2, 70),
        ("ILLUMINATE", 2, 71),
        ("FLYING", 2, 75),
        ("WONDER_GUARD", 2, 80),
        ("INSOMNIA", 2, 81),
        ("OWN_TEMPO", 2, 93),
        ("CUTE_CHARM", 2, 99),
        ("IMMUNITY", 2, 100),
        ("VERSUS", 3, 28),
        ("STRONG", 3, 32),
        ("ESCAPE", 3, 38),
        ("WEAK", 3, 57),
        ("YOU", 5, 2),
        ("YOUR", 5, 5),
        ("QUES", 6, 3),
        ("THE", 7, 13),
        ("HOW", 7, 15),
        ("WILL", 8, 0),
        ("IS", 8, 11),
        ("WAS", 8, 16),
        ("ISN_T", 8, 19),
        ("DO", 8, 24),
        ("WHICH", 8, 27),
        ("A", 8, 32),
        ("ON", 8, 44),
        ("WITH", 8, 48),
        ("OF", 8, 52),
        ("ONCE", 8, 67),
        ("GOES", 9, 3),
        ("PRETTY", 10, 26),
        ("COLD", 10, 30),
        ("GIVES", 11, 3),
        ("STORES", 11, 35),
        ("INFORMATION", 12, 18),
        ("LETTER", 12, 22),
        ("DEPT_STORE", 12, 26),
        ("TELEVISION", 12, 27),
        ("PHONE", 12, 28),
        ("ITEM", 12, 29),
        ("MACHINE", 12, 35),
        ("MAIL", 12, 36),
        ("MUSIC", 13, 8),
        ("CAMERA", 13, 11),
        ("GAME", 13, 15),
        ("BIKE", 13, 21),
        ("LOCOMOTIVE", 13, 35),
        ("PLUSH_DOLL", 13, 36),
        ("PC", 13, 37),
        ("FLOWERS", 13, 38),
        ("STOP", 14, 25),
        ("THIS", 15, 15),
        ("THESE", 15, 17),
        ("BREAK", 16, 20),
        ("MOVE2_CONFUSION", 19, 93),
        ("MOVE2_BLOCK", 19, 335),
    ];

    fn word(name: &str) -> u16 {
        if name == "EMPTY" {
            return EC_EMPTY_WORD;
        }
        let &(_, group, index) = WORDS
            .iter()
            .find(|(candidate, _, _)| *candidate == name)
            .unwrap_or_else(|| panic!("unlisted word {name}"));
        (group << 9) | index
    }

    #[rustfmt::skip]
    const EXPECTED_QUIZZES: [([&str; QUIZ_QUESTION_LEN], &str, ItemId); QUIZ_COUNT] = [
    (
        ["WHICH", "STORES", "INFORMATION", "ON", "POKEMON", "QUES", "CAMERA", "POKEDEX", "POKENAV"],
        "POKEDEX",
        ItemId::GLITTER_MAIL,
    ),
    (
        ["WHICH", "ISN_T", "A", "GAME", "VERSION", "QUES", "RUBY", "SAPPHIRE", "DARK"],
        "DARK",
        ItemId::BEAD_MAIL,
    ),
    (
        ["HOW", "DO", "POKEMON", "EVOLVE", "QUES", "EMPTY", "LEVEL", "INSOMNIA", "CUTE_CHARM"],
        "LEVEL",
        ItemId::TROPIC_MAIL,
    ),
    (
        ["WHICH", "IS", "THE", "PRETTY", "ITEM", "QUES", "COLD", "FLOWERS", "MACHINE"],
        "FLOWERS",
        ItemId::MAX_ETHER,
    ),
    (
        ["WHICH", "ITEM", "DO", "YOU", "BREAK", "QUES", "EGG", "MAIL", "PHONE"],
        "EGG",
        ItemId::MAX_ETHER,
    ),
    (
        ["WHICH", "WILL", "STOP", "MOVE2_CONFUSION", "QUES", "EMPTY", "ILLUMINATE", "OWN_TEMPO", "SWIFT_SWIM"],
        "OWN_TEMPO",
        ItemId::MAX_ETHER,
    ),
    (
        ["WHICH", "OF", "THESE", "IS", "MUSIC", "QUES", "FLYING", "STEEL", "ROCK"],
        "ROCK",
        ItemId::WATMEL_BERRY,
    ),
    (
        ["WHICH", "WILL", "MOVE2_BLOCK", "ESCAPE", "QUES", "EMPTY", "RUN_AWAY", "SHADOW_TAG", "WONDER_GUARD"],
        "SHADOW_TAG",
        ItemId::BELUE_BERRY,
    ),
    (
        ["WHICH", "WILL", "STOP", "POISON", "QUES", "EMPTY", "GUTS", "IMMUNITY", "SHED_SKIN"],
        "IMMUNITY",
        ItemId::DURIN_BERRY,
    ),
    (
        ["WHICH", "GOES", "WITH", "CENTER", "QUES", "EMPTY", "POKEDEX", "POKEMON", "POKENAV"],
        "POKEMON",
        ItemId::LUXURY_BALL,
    ),
    (
        ["WHICH", "STORES", "YOUR", "POKEMON", "QUES", "EMPTY", "PC", "DEPT_STORE", "TELEVISION"],
        "PC",
        ItemId::TM_HYPER_BEAM,
    ),
    (
        ["WHICH", "MACHINE", "GIVES", "YOU", "INFORMATION", "QUES", "BIKE", "LOCOMOTIVE", "TELEVISION"],
        "TELEVISION",
        ItemId::BIG_PEARL,
    ),
    (
        ["A", "POKEMON", "WAS", "ONCE", "THIS", "QUES", "PHONE", "PLUSH_DOLL", "LETTER"],
        "PLUSH_DOLL",
        ItemId::STAR_PIECE,
    ),
    (
        ["STEEL", "IS", "STRONG", "VERSUS", "WHICH", "QUES", "ICE", "GROUND", "EMPTY"],
        "ICE",
        ItemId::RARE_CANDY,
    ),
    (
        ["DARK", "IS", "WEAK", "VERSUS", "WHICH", "QUES", "PSYCHIC", "FIGHTING", "EMPTY"],
        "FIGHTING",
        ItemId::RARE_CANDY,
    ),
    (
        ["GHOST", "IS", "WEAK", "VERSUS", "WHICH", "QUES", "NORMAL", "DARK", "EMPTY"],
        "DARK",
        ItemId::PREMIER_BALL,
    ),
    ];

    #[test]
    fn quiz_questions_answers_and_prizes_stay_aligned() {
        let data = LilycoveLadyData::new();
        assert_eq!(data.quizzes().len(), 16);
        for (index, (question, answer, prize)) in EXPECTED_QUIZZES.iter().enumerate() {
            let quiz = data.quiz(index).unwrap();
            assert_eq!(quiz.question, question.map(word), "question {index}");
            assert_eq!(quiz.answer, word(answer), "answer {index}");
            assert_eq!(quiz.prize, *prize, "prize {index}");
        }
    }

    #[test]
    fn quiz_answers_are_words_of_their_own_question() {
        for (index, quiz) in LilycoveLadyData::new().quizzes().iter().enumerate() {
            let choices = &quiz.question[6..];
            assert!(choices.contains(&quiz.answer), "quiz {index}");
        }
    }

    #[test]
    fn quiz_blank_slots_use_the_empty_word() {
        let blanks: Vec<(usize, usize)> = LilycoveLadyData::new()
            .quizzes()
            .iter()
            .enumerate()
            .flat_map(|(quiz, row)| {
                row.question
                    .iter()
                    .enumerate()
                    .filter(|(_, word)| **word == EC_EMPTY_WORD)
                    .map(move |(slot, _)| (quiz, slot))
            })
            .collect();
        assert_eq!(
            blanks,
            [
                (2, 5),
                (5, 5),
                (7, 5),
                (8, 5),
                (9, 5),
                (10, 5),
                (13, 8),
                (14, 8),
                (15, 8)
            ]
        );
    }

    #[test]
    fn quiz_move_words_use_the_second_move_group() {
        let data = LilycoveLadyData::new();
        assert_eq!(data.quiz(5).unwrap().question[3], (19 << 9) | 93);
        assert_eq!(data.quiz(7).unwrap().question[2], (19 << 9) | 335);
    }

    #[test]
    fn favor_requests_match_upstream_in_order() {
        const EXPECTED: [(&str, &[ItemId], ItemId); FAVOR_REQUEST_COUNT] = [
            ("slippery", FAVOR_ITEMS[0], ItemId::LUXURY_BALL),
            ("roundish", FAVOR_ITEMS[1], ItemId::NUGGET),
            ("wham-ish", FAVOR_ITEMS[2], ItemId::PROTEIN),
            ("shiny", FAVOR_ITEMS[3], ItemId::HEART_SCALE),
            ("sticky", FAVOR_ITEMS[4], ItemId::RARE_CANDY),
            ("pointy", FAVOR_ITEMS[5], ItemId::PP_MAX),
        ];
        let data = LilycoveLadyData::new();
        for (index, (text, items, prize)) in EXPECTED.iter().enumerate() {
            let request = data.favor_request(index).unwrap();
            assert_eq!(request.text, *text, "request {index}");
            assert_eq!(request.items(), *items, "items {index}");
            assert_eq!(request.prize, *prize, "prize {index}");
        }
    }

    const FAVOR_ITEMS: [&[ItemId]; FAVOR_REQUEST_COUNT] = [
        &[
            ItemId::REPEL,
            ItemId::SUPER_REPEL,
            ItemId::MAX_REPEL,
            ItemId::ANTIDOTE,
            ItemId::PARALYZE_HEAL,
            ItemId::BURN_HEAL,
            ItemId::BELUE_BERRY,
            ItemId::AWAKENING,
            ItemId::ICE_HEAL,
            ItemId::REVIVE,
            ItemId::MAX_REVIVE,
            ItemId::ENERGY_POWDER,
        ],
        &[
            ItemId::FLUFFY_TAIL,
            ItemId::PEARL,
            ItemId::BIG_PEARL,
            ItemId::HARD_STONE,
            ItemId::SMOKE_BALL,
            ItemId::SHOAL_SHELL,
            ItemId::TINY_MUSHROOM,
            ItemId::BIG_MUSHROOM,
            ItemId::PECHA_BERRY,
            ItemId::ASPEAR_BERRY,
            ItemId::ORAN_BERRY,
            ItemId::GREPA_BERRY,
            ItemId::MAGOST_BERRY,
            ItemId::WATMEL_BERRY,
            ItemId::POKE_BALL,
            ItemId::ULTRA_BALL,
        ],
        &[
            ItemId::REVIVAL_HERB,
            ItemId::POTION,
            ItemId::FRESH_WATER,
            ItemId::SODA_POP,
            ItemId::LEMONADE,
            ItemId::HARD_STONE,
            ItemId::LIGHT_BALL,
            ItemId::LAVA_COOKIE,
            ItemId::CHESTO_BERRY,
            ItemId::NANAB_BERRY,
            ItemId::WEPEAR_BERRY,
            ItemId::KELPSY_BERRY,
            ItemId::NOMEL_BERRY,
            ItemId::DURIN_BERRY,
        ],
        &[
            ItemId::HEAL_POWDER,
            ItemId::X_SPEED,
            ItemId::X_ATTACK,
            ItemId::X_DEFEND,
            ItemId::BLUE_FLUTE,
            ItemId::YELLOW_FLUTE,
            ItemId::RED_FLUTE,
            ItemId::BLACK_FLUTE,
            ItemId::WHITE_FLUTE,
            ItemId::NUGGET,
            ItemId::SUN_STONE,
            ItemId::STARDUST,
            ItemId::STAR_PIECE,
            ItemId::PEARL,
            ItemId::BIG_PEARL,
            ItemId::TWISTED_SPOON,
            ItemId::SILVER_POWDER,
            ItemId::BRIGHT_POWDER,
            ItemId::LUXURY_BALL,
            ItemId::PREMIER_BALL,
        ],
        &[
            ItemId::ENERGY_ROOT,
            ItemId::FULL_RESTORE,
            ItemId::MAX_POTION,
            ItemId::DIRE_HIT,
            ItemId::X_ACCURACY,
            ItemId::GUARD_SPEC,
            ItemId::WATMEL_BERRY,
            ItemId::LEFTOVERS,
            ItemId::TINY_MUSHROOM,
            ItemId::HEART_SCALE,
        ],
        &[
            ItemId::QUICK_CLAW,
            ItemId::POISON_BARB,
            ItemId::SHARP_BEAK,
            ItemId::DRAGON_FANG,
            ItemId::TAMATO_BERRY,
            ItemId::DURIN_BERRY,
            ItemId::PETAYA_BERRY,
            ItemId::SALAC_BERRY,
            ItemId::STARDUST,
            ItemId::STAR_PIECE,
        ],
    ];

    #[test]
    fn favor_item_lists_end_with_one_terminator() {
        let lengths = [13, 17, 15, 21, 11, 11];
        for (index, request) in LilycoveLadyData::new().favor_requests().iter().enumerate() {
            let list = request.accepted_items;
            assert_eq!(list.len(), lengths[index], "list {index}");
            assert_eq!(list.last(), Some(&ItemId::NONE), "terminator {index}");
            assert!(!request.items().contains(&ItemId::NONE), "list {index}");
            assert_eq!(request.items().len() + 1, list.len());
        }
    }

    #[test]
    fn contest_records_follow_category_order() {
        let data = LilycoveLadyData::new();
        let expected = [
            (
                ContestCategory::Cool,
                "HANDSOME",
                "coolness",
                "COOLNESS CONTEST",
                288,
                98,
            ),
            (
                ContestCategory::Beauty,
                "VINNY",
                "beauty",
                "BEAUTY CONTEST",
                315,
                203,
            ),
            (
                ContestCategory::Cute,
                "MOREME",
                "cuteness",
                "CUTENESS CONTEST",
                286,
                220,
            ),
            (
                ContestCategory::Smart,
                "IRONHARD",
                "smartness",
                "SMARTNESS CONTEST",
                317,
                204,
            ),
            (
                ContestCategory::Tough,
                "MUSCLE",
                "toughness",
                "TOUGHNESS CONTEST",
                25,
                209,
            ),
        ];
        assert_eq!(data.contest_records().len(), 5);
        for (index, (category, mon, name, contest, species, gfx)) in expected.iter().enumerate() {
            let record = data.contest_record(*category);
            assert_eq!(record, &data.contest_records()[index]);
            assert_eq!(record.category, *category);
            assert_eq!(usize::from(category.id()), index);
            assert_eq!(record.mon_name, *mon);
            assert_eq!(record.category_name, *name);
            assert_eq!(record.contest_name, *contest);
            assert_eq!(record.species, SpeciesId(*species));
            assert_eq!(record.mon_gfx_id, *gfx);
        }
    }

    #[test]
    fn lady_sprites_follow_activity_order() {
        let data = LilycoveLadyData::new();
        assert_eq!(data.lady_gfx_id(LilycoveLadyKind::Quiz), 26);
        assert_eq!(data.lady_gfx_id(LilycoveLadyKind::Favor), 20);
        assert_eq!(data.lady_gfx_id(LilycoveLadyKind::Contest), 10);
    }

    #[test]
    fn out_of_range_lookups_fail() {
        let data = LilycoveLadyData::new();
        assert_eq!(data.quiz(15).map(|_| ()), Ok(()));
        assert_eq!(data.quiz(16), Err(LilycoveLadyError::UnknownQuiz(16)));
        assert_eq!(
            data.quiz(usize::MAX),
            Err(LilycoveLadyError::UnknownQuiz(usize::MAX))
        );
        assert_eq!(data.favor_request(5).map(|_| ()), Ok(()));
        assert_eq!(
            data.favor_request(6),
            Err(LilycoveLadyError::UnknownFavorRequest(6))
        );
        assert_eq!(
            ContestCategory::from_id(5),
            Err(LilycoveLadyError::UnknownContestCategory(5))
        );
        assert_eq!(
            LilycoveLadyKind::from_id(3),
            Err(LilycoveLadyError::UnknownLady(3))
        );
        for id in 0..5 {
            assert_eq!(ContestCategory::from_id(id).unwrap().id(), id);
        }
        for id in 0..3 {
            assert_eq!(LilycoveLadyKind::from_id(id).unwrap().id(), id);
        }
    }

    #[test]
    fn errors_name_the_rejected_value() {
        assert_eq!(
            LilycoveLadyError::UnknownQuiz(16).to_string(),
            "unknown quiz index `16`"
        );
        assert_eq!(
            LilycoveLadyError::UnknownContestCategory(5).to_string(),
            "unknown contest category `5`"
        );
    }

    fn fnv1a64(bytes: &[u8]) -> u64 {
        bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, &byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
    }

    /// Digest of the text a C program prints after including the unmodified
    /// upstream `src/data/lilycove_lady.h` and its constant headers: one line
    /// per quiz, favor request, contest record and Lady.
    #[test]
    fn every_record_matches_the_canonical_digest() {
        use std::fmt::Write;

        let data = LilycoveLadyData::new();
        let mut out = String::new();
        for (index, quiz) in data.quizzes().iter().enumerate() {
            write!(out, "quiz {index}:").unwrap();
            for word in quiz.question {
                write!(out, " {word}").unwrap();
            }
            writeln!(out, " ans {} prize {}", quiz.answer, quiz.prize.index()).unwrap();
        }
        for (index, request) in data.favor_requests().iter().enumerate() {
            write!(out, "favor {index} {}:", request.text).unwrap();
            for item in request.accepted_items {
                write!(out, " {}", item.index()).unwrap();
            }
            writeln!(out, " prize {}", request.prize.index()).unwrap();
        }
        for (index, record) in data.contest_records().iter().enumerate() {
            writeln!(
                out,
                "contest {index} {} {} {} sp {} gfx {}",
                record.mon_name,
                record.category_name,
                record.contest_name,
                record.species.index(),
                record.mon_gfx_id,
            )
            .unwrap();
        }
        for id in 0..3 {
            let lady = LilycoveLadyKind::from_id(id).unwrap();
            writeln!(out, "lady {id} gfx {}", data.lady_gfx_id(lady)).unwrap();
        }
        assert_eq!(out.len(), 1930);
        assert_eq!(
            format!("{:016x}", fnv1a64(out.as_bytes())),
            "602993fcee07b696",
            "Lilycove Lady data diverges from the canonical upstream rows",
        );
    }
}
