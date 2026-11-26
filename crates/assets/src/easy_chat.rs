//! The Trainer and Greetings Easy Chat vocabulary groups: word identity, text,
//! enabled flag, and alphabetical ordering for every `gEasyChatGroup_Trainer`
//! and `gEasyChatGroup_Greetings` row.
//!
//! Source: `src/data/easy_chat/easy_chat_group_trainer.h`,
//! `src/data/easy_chat/easy_chat_group_greetings.h`, the
//! `struct EasyChatWordInfo` layout in `include/easy_chat.h`, and the
//! `EC_WORD_*` indices of both groups in `include/constants/easy_chat.h`
//! (0 through 26 for Trainer, 0 through 41 for Greetings). The other groups,
//! the group registry, unlock state, and the editor UI are not part of these
//! tables.

use std::error::Error;
use std::fmt;

/// The number of words in the Trainer group (`EC_GROUP_TRAINER`).
pub const TRAINER_WORD_COUNT: usize = 27;

/// The number of words in the Greetings group (`EC_GROUP_GREETINGS`).
pub const GREETINGS_WORD_COUNT: usize = 42;

/// A failed decoding of an Easy Chat word index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EasyChatError {
    /// The value is not below [`TRAINER_WORD_COUNT`].
    UnknownTrainerWord(u8),
    /// The value is not below [`GREETINGS_WORD_COUNT`].
    UnknownGreetingsWord(u8),
}

impl fmt::Display for EasyChatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownTrainerWord(index) => {
                write!(f, "unknown trainer easy chat word index `{index}`")
            }
            Self::UnknownGreetingsWord(index) => {
                write!(f, "unknown greetings easy chat word index `{index}`")
            }
        }
    }
}

impl Error for EasyChatError {}

/// A word's index within the Trainer group (`EC_INDEX` of an `EC_WORD_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TrainerWordIndex(u8);

impl TrainerWordIndex {
    /// Validates a raw group index.
    ///
    /// # Errors
    ///
    /// Returns [`EasyChatError::UnknownTrainerWord`] when `index` is
    /// [`TRAINER_WORD_COUNT`] or more.
    pub const fn new(index: u8) -> Result<Self, EasyChatError> {
        if (index as usize) < TRAINER_WORD_COUNT {
            Ok(Self(index))
        } else {
            Err(EasyChatError::UnknownTrainerWord(index))
        }
    }

    /// Returns the raw group index.
    #[must_use]
    pub const fn index(self) -> u8 {
        self.0
    }
}

/// One `gEasyChatGroup_Trainer` row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrainerWord {
    /// The row's index, equal to its table position.
    pub index: TrainerWordIndex,
    /// The word text (`.text`), in the game's charmap notation.
    pub text: &'static str,
    /// The word at this alphabetical position (`.alphabeticalOrder`), as
    /// `SetSelectedWordGroup_GroupMode` reads it while walking rows in order.
    pub alphabetical_order: TrainerWordIndex,
    /// Whether the word is offered (`.enabled`).
    pub enabled: bool,
}

/// Lookup over the Trainer group.
#[derive(Debug, Clone, Copy)]
pub struct TrainerGroup {
    rows: &'static [TrainerWord; TRAINER_WORD_COUNT],
}

impl TrainerGroup {
    /// Creates a handle on the canonical group.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            rows: &TRAINER_WORDS,
        }
    }

    /// Returns the number of words.
    #[must_use]
    pub const fn len(&self) -> usize {
        TRAINER_WORD_COUNT
    }

    /// Returns `false`; the group is never empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }

    /// Returns the row for `index`.
    #[must_use]
    pub const fn get(&self, index: TrainerWordIndex) -> &TrainerWord {
        &self.rows[index.0 as usize]
    }

    /// Iterates in ascending [`TrainerWordIndex`] order.
    pub fn iter(&self) -> impl Iterator<Item = &TrainerWord> {
        self.rows.iter()
    }

    /// Iterates the words in alphabetical order, enabled or not.
    pub fn alphabetical(&self) -> impl Iterator<Item = &TrainerWord> {
        self.rows.iter().map(|row| self.get(row.alphabetical_order))
    }

    /// Returns how many words have `enabled` set.
    #[must_use]
    pub fn enabled_count(&self) -> usize {
        self.iter().filter(|word| word.enabled).count()
    }
}

impl Default for TrainerGroup {
    fn default() -> Self {
        Self::new()
    }
}

macro_rules! define_trainer_words {
    ($($name:ident = $index:literal => ($text:literal, $alphabetical:ident, $enabled:literal)),+ $(,)?) => {
        impl TrainerWordIndex {
            $(
                #[doc = concat!("`EC_WORD_", stringify!($name), "`.")]
                pub const $name: Self = Self($index);
            )+
        }

        const TRAINER_WORDS: [TrainerWord; TRAINER_WORD_COUNT] = [
            $(TrainerWord {
                index: TrainerWordIndex::$name,
                text: $text,
                alphabetical_order: TrainerWordIndex::$alphabetical,
                enabled: $enabled,
            }),+
        ];

        const _: () = {
            let indices: [usize; TRAINER_WORD_COUNT] = [$($index),+];
            let mut position = 0;
            while position < TRAINER_WORD_COUNT {
                assert!(indices[position] == position);
                position += 1;
            }
        };
    };
}

#[rustfmt::skip]
define_trainer_words! {
    I_CHOOSE_YOU = 0 => ("I CHOOSE YOU", BAG, true),
    GOTCHA = 1 => ("GOTCHA", CENTER, true),
    TRADE = 2 => ("TRADE", EGG, true),
    SAPPHIRE = 3 => ("SAPPHIRE", EMERALD, true),
    EVOLVE = 4 => ("EVOLVE", ENCYCLOPEDIA, true),
    ENCYCLOPEDIA = 5 => ("ENCYCLOPEDIA", EVOLVE, true),
    NATURE = 6 => ("NATURE", FLAME, true),
    CENTER = 7 => ("CENTER", GET, true),
    EGG = 8 => ("EGG", GOLD, true),
    LINK = 9 => ("LINK", GOTCHA, true),
    SP_ABILITY = 10 => ("SP. ABILITY", GREEN, true),
    TRAINER = 11 => ("TRAINER", I_CHOOSE_YOU, true),
    VERSION = 12 => ("VERSION", LEAF, true),
    POKENAV = 13 => ("POKéNAV", LEVEL, true),
    POKEMON = 14 => ("POKéMON", LINK, true),
    GET = 15 => ("GET", NATURE, true),
    POKEDEX = 16 => ("POKéDEX", POKEDEX, true),
    RUBY = 17 => ("RUBY", POKEMON, true),
    LEVEL = 18 => ("LEVEL", POKENAV, true),
    RED = 19 => ("RED", RED, false),
    GREEN = 20 => ("GREEN", RUBY, false),
    BAG = 21 => ("BAG", SAPPHIRE, true),
    FLAME = 22 => ("FLAME", SILVER, false),
    GOLD = 23 => ("GOLD", SP_ABILITY, false),
    LEAF = 24 => ("LEAF", TRADE, false),
    SILVER = 25 => ("SILVER", TRAINER, false),
    EMERALD = 26 => ("EMERALD", VERSION, true),
}

/// A word's index within the Greetings group (`EC_INDEX` of an `EC_WORD_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GreetingsWordIndex(u8);

impl GreetingsWordIndex {
    /// Validates a raw group index.
    ///
    /// # Errors
    ///
    /// Returns [`EasyChatError::UnknownGreetingsWord`] when `index` is
    /// [`GREETINGS_WORD_COUNT`] or more.
    pub const fn new(index: u8) -> Result<Self, EasyChatError> {
        if (index as usize) < GREETINGS_WORD_COUNT {
            Ok(Self(index))
        } else {
            Err(EasyChatError::UnknownGreetingsWord(index))
        }
    }

    /// Returns the raw group index.
    #[must_use]
    pub const fn index(self) -> u8 {
        self.0
    }
}

/// One `gEasyChatGroup_Greetings` row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GreetingsWord {
    /// The row's index, equal to its table position.
    pub index: GreetingsWordIndex,
    /// The word text (`.text`), in the game's charmap notation.
    pub text: &'static str,
    /// The word at this alphabetical position (`.alphabeticalOrder`), as
    /// `SetSelectedWordGroup_GroupMode` reads it while walking rows in order.
    pub alphabetical_order: GreetingsWordIndex,
    /// Whether the word is offered (`.enabled`).
    pub enabled: bool,
}

/// Lookup over the Greetings group.
#[derive(Debug, Clone, Copy)]
pub struct GreetingsGroup {
    rows: &'static [GreetingsWord; GREETINGS_WORD_COUNT],
}

impl GreetingsGroup {
    /// Creates a handle on the canonical group.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            rows: &GREETINGS_WORDS,
        }
    }

    /// Returns the number of words.
    #[must_use]
    pub const fn len(&self) -> usize {
        GREETINGS_WORD_COUNT
    }

    /// Returns `false`; the group is never empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }

    /// Returns the row for `index`.
    #[must_use]
    pub const fn get(&self, index: GreetingsWordIndex) -> &GreetingsWord {
        &self.rows[index.0 as usize]
    }

    /// Iterates in ascending [`GreetingsWordIndex`] order.
    pub fn iter(&self) -> impl Iterator<Item = &GreetingsWord> {
        self.rows.iter()
    }

    /// Iterates the words in alphabetical order, enabled or not.
    pub fn alphabetical(&self) -> impl Iterator<Item = &GreetingsWord> {
        self.rows.iter().map(|row| self.get(row.alphabetical_order))
    }

    /// Returns how many words have `enabled` set.
    #[must_use]
    pub fn enabled_count(&self) -> usize {
        self.iter().filter(|word| word.enabled).count()
    }
}

impl Default for GreetingsGroup {
    fn default() -> Self {
        Self::new()
    }
}

macro_rules! define_greetings_words {
    ($($name:ident = $index:literal => ($text:literal, $alphabetical:ident, $enabled:literal)),+ $(,)?) => {
        impl GreetingsWordIndex {
            $(
                #[doc = concat!("`EC_WORD_", stringify!($name), "`.")]
                pub const $name: Self = Self($index);
            )+
        }

        const GREETINGS_WORDS: [GreetingsWord; GREETINGS_WORD_COUNT] = [
            $(GreetingsWord {
                index: GreetingsWordIndex::$name,
                text: $text,
                alphabetical_order: GreetingsWordIndex::$alphabetical,
                enabled: $enabled,
            }),+
        ];

        const _: () = {
            let indices: [usize; GREETINGS_WORD_COUNT] = [$($index),+];
            let mut position = 0;
            while position < GREETINGS_WORD_COUNT {
                assert!(indices[position] == position);
                position += 1;
            }
        };
    };
}

#[rustfmt::skip]
define_greetings_words! {
    THANKS = 0 => ("THANKS", APOLOGIZE, true),
    YES = 1 => ("YES", APPRECIATE, true),
    HERE_GOES = 2 => ("HERE GOES", BYE_BYE, true),
    HERE_I_COME = 3 => ("HERE I COME", COME_OVER, true),
    HERE_IT_IS = 4 => ("HERE IT IS", CONGRATS, true),
    YEAH = 5 => ("YEAH", COUNT_ON, true),
    WELCOME = 6 => ("WELCOME", EXCUSE, true),
    OI = 7 => ("OI", EXCUSE_ME, true),
    HOW_DO = 8 => ("HOW DO", FORGIVE, true),
    CONGRATS = 9 => ("CONGRATS", GIVE_ME, true),
    GIVE_ME = 10 => ("GIVE ME", GO_AHEAD, true),
    SORRY = 11 => ("SORRY", GOOD_BYE, true),
    APOLOGIZE = 12 => ("APOLOGIZE", HELLO, true),
    FORGIVE = 13 => ("FORGIVE", HERE_GOES, true),
    HEY_THERE = 14 => ("HEY, THERE", HERE_I_COME, true),
    HELLO = 15 => ("HELLO", HERE_IT_IS, true),
    GOOD_BYE = 16 => ("GOOD-BYE", HEY, true),
    THANK_YOU = 17 => ("THANK YOU", HEY_THERE, true),
    I_VE_ARRIVED = 18 => ("I'VE ARRIVED", HEY_QUES, true),
    PARDON = 19 => ("PARDON", HI, true),
    EXCUSE = 20 => ("EXCUSE", HOO_HAH, true),
    SEE_YA = 21 => ("SEE YA", HOW_DO, true),
    EXCUSE_ME = 22 => ("EXCUSE ME", HUH_QUES, true),
    WELL_THEN = 23 => ("WELL, THEN", I_VE_ARRIVED, true),
    GO_AHEAD = 24 => ("GO AHEAD", LISTENING, true),
    APPRECIATE = 25 => ("APPRECIATE", MEET_YOU, true),
    HEY_QUES = 26 => ("HEY?", NO, true),
    WHAT_S_UP_QUES = 27 => ("WHAT'S UP?", OI, true),
    HUH_QUES = 28 => ("HUH?", PARDON, true),
    NO = 29 => ("NO", SEE_YA, true),
    HI = 30 => ("HI", SMELL, true),
    YEAH_YEAH = 31 => ("YEAH, YEAH", SORRY, true),
    BYE_BYE = 32 => ("BYE-BYE", THANK_YOU, true),
    MEET_YOU = 33 => ("MEET YOU", THANKS, true),
    HEY = 34 => ("HEY", WELCOME, true),
    SMELL = 35 => ("SMELL", WELL_THEN, true),
    LISTENING = 36 => ("LISTENING", WHAT_S_UP_QUES, true),
    HOO_HAH = 37 => ("HOO-HAH", YAHOO, true),
    YAHOO = 38 => ("YAHOO", YEAH, true),
    YO = 39 => ("YO", YEAH_YEAH, true),
    COME_OVER = 40 => ("COME OVER", YES, true),
    COUNT_ON = 41 => ("COUNT ON", YO, true),
}

#[cfg(test)]
mod tests {
    use super::{EasyChatError, TrainerGroup, TrainerWordIndex, TRAINER_WORD_COUNT};

    const TEXTS: [&str; TRAINER_WORD_COUNT] = [
        "I CHOOSE YOU",
        "GOTCHA",
        "TRADE",
        "SAPPHIRE",
        "EVOLVE",
        "ENCYCLOPEDIA",
        "NATURE",
        "CENTER",
        "EGG",
        "LINK",
        "SP. ABILITY",
        "TRAINER",
        "VERSION",
        "POKéNAV",
        "POKéMON",
        "GET",
        "POKéDEX",
        "RUBY",
        "LEVEL",
        "RED",
        "GREEN",
        "BAG",
        "FLAME",
        "GOLD",
        "LEAF",
        "SILVER",
        "EMERALD",
    ];

    const ALPHABETICAL: [&str; TRAINER_WORD_COUNT] = [
        "BAG",
        "CENTER",
        "EGG",
        "EMERALD",
        "ENCYCLOPEDIA",
        "EVOLVE",
        "FLAME",
        "GET",
        "GOLD",
        "GOTCHA",
        "GREEN",
        "I CHOOSE YOU",
        "LEAF",
        "LEVEL",
        "LINK",
        "NATURE",
        "POKéDEX",
        "POKéMON",
        "POKéNAV",
        "RED",
        "RUBY",
        "SAPPHIRE",
        "SILVER",
        "SP. ABILITY",
        "TRADE",
        "TRAINER",
        "VERSION",
    ];

    #[test]
    fn every_row_sits_at_its_declared_index() {
        let group = TrainerGroup::new();
        assert_eq!(group.len(), TRAINER_WORD_COUNT);
        assert_eq!(group.iter().count(), TRAINER_WORD_COUNT);
        assert!(!group.is_empty());
        for (position, word) in group.iter().enumerate() {
            assert_eq!(usize::from(word.index.index()), position);
            assert_eq!(group.get(word.index), word);
        }
    }

    #[test]
    fn the_full_table_text_is_pinned() {
        let texts: Vec<&str> = TrainerGroup::new().iter().map(|word| word.text).collect();
        assert_eq!(texts, TEXTS);
    }

    #[test]
    fn named_indices_match_upstream_constants() {
        // include/constants/easy_chat.h: EC_WORD_I_CHOOSE_YOU 0 .. EC_WORD_EMERALD 26.
        let group = TrainerGroup::new();
        assert_eq!(TrainerWordIndex::I_CHOOSE_YOU.index(), 0);
        assert_eq!(group.get(TrainerWordIndex::SP_ABILITY).text, "SP. ABILITY");
        assert_eq!(group.get(TrainerWordIndex::POKEMON).index.index(), 14);
        assert_eq!(group.get(TrainerWordIndex::BAG).index.index(), 21);
        assert_eq!(TrainerWordIndex::EMERALD.index(), 26);
    }

    #[test]
    fn the_alphabetical_order_is_a_permutation_in_text_order() {
        let group = TrainerGroup::new();
        let mut seen = [false; TRAINER_WORD_COUNT];
        for word in group.iter() {
            let slot = usize::from(word.alphabetical_order.index());
            assert!(!seen[slot], "index {slot} appears twice");
            seen[slot] = true;
        }
        assert!(seen.iter().all(|&hit| hit));

        let texts: Vec<&str> = group.alphabetical().map(|word| word.text).collect();
        assert_eq!(texts, ALPHABETICAL);
    }

    #[test]
    fn the_alphabetical_order_pins_spot_rows() {
        let group = TrainerGroup::new();
        assert_eq!(
            group.get(TrainerWordIndex::I_CHOOSE_YOU).alphabetical_order,
            TrainerWordIndex::BAG
        );
        assert_eq!(
            group.get(TrainerWordIndex::POKEDEX).alphabetical_order,
            TrainerWordIndex::POKEDEX
        );
        assert_eq!(
            group.get(TrainerWordIndex::EMERALD).alphabetical_order,
            TrainerWordIndex::VERSION
        );
    }

    #[test]
    fn twenty_one_words_are_enabled_and_six_are_not() {
        let group = TrainerGroup::new();
        assert_eq!(group.enabled_count(), 21);
        let disabled: Vec<&str> = group
            .iter()
            .filter(|word| !word.enabled)
            .map(|word| word.text)
            .collect();
        assert_eq!(
            disabled,
            ["RED", "GREEN", "FLAME", "GOLD", "LEAF", "SILVER"]
        );
    }

    #[test]
    fn an_out_of_range_index_is_rejected() {
        assert_eq!(TrainerWordIndex::new(0), Ok(TrainerWordIndex::I_CHOOSE_YOU));
        assert_eq!(TrainerWordIndex::new(26), Ok(TrainerWordIndex::EMERALD));
        for raw in [27, 28, u8::MAX] {
            assert_eq!(
                TrainerWordIndex::new(raw),
                Err(EasyChatError::UnknownTrainerWord(raw))
            );
        }
        assert_eq!(
            EasyChatError::UnknownTrainerWord(27).to_string(),
            "unknown trainer easy chat word index `27`"
        );
    }
}

#[cfg(test)]
mod greetings_tests {
    use super::{EasyChatError, GreetingsGroup, GreetingsWordIndex, GREETINGS_WORD_COUNT};

    /// `(text, alphabetical rank, enabled)` per row, in upstream table order.
    const ROWS: [(&str, u8, bool); GREETINGS_WORD_COUNT] = [
        ("THANKS", 12, true),
        ("YES", 25, true),
        ("HERE GOES", 32, true),
        ("HERE I COME", 40, true),
        ("HERE IT IS", 9, true),
        ("YEAH", 41, true),
        ("WELCOME", 20, true),
        ("OI", 22, true),
        ("HOW DO", 13, true),
        ("CONGRATS", 10, true),
        ("GIVE ME", 24, true),
        ("SORRY", 16, true),
        ("APOLOGIZE", 15, true),
        ("FORGIVE", 2, true),
        ("HEY, THERE", 3, true),
        ("HELLO", 4, true),
        ("GOOD-BYE", 34, true),
        ("THANK YOU", 14, true),
        ("I'VE ARRIVED", 26, true),
        ("PARDON", 30, true),
        ("EXCUSE", 37, true),
        ("SEE YA", 8, true),
        ("EXCUSE ME", 28, true),
        ("WELL, THEN", 18, true),
        ("GO AHEAD", 36, true),
        ("APPRECIATE", 33, true),
        ("HEY?", 29, true),
        ("WHAT'S UP?", 7, true),
        ("HUH?", 19, true),
        ("NO", 21, true),
        ("HI", 35, true),
        ("YEAH, YEAH", 11, true),
        ("BYE-BYE", 17, true),
        ("MEET YOU", 0, true),
        ("HEY", 6, true),
        ("SMELL", 23, true),
        ("LISTENING", 27, true),
        ("HOO-HAH", 38, true),
        ("YAHOO", 5, true),
        ("YO", 31, true),
        ("COME OVER", 1, true),
        ("COUNT ON", 39, true),
    ];

    #[test]
    fn every_row_sits_at_its_declared_index() {
        let group = GreetingsGroup::new();
        assert_eq!(group.len(), GREETINGS_WORD_COUNT);
        assert_eq!(group.iter().count(), GREETINGS_WORD_COUNT);
        assert!(!group.is_empty());
        for (position, word) in group.iter().enumerate() {
            assert_eq!(usize::from(word.index.index()), position);
            assert_eq!(group.get(word.index), word);
        }
    }

    #[test]
    fn all_forty_two_entries_are_pinned_in_upstream_order() {
        let group = GreetingsGroup::new();
        for (position, word) in group.iter().enumerate() {
            let (text, rank, enabled) = ROWS[position];
            assert_eq!(word.text, text, "text of row {position}");
            assert_eq!(
                word.alphabetical_order.index(),
                rank,
                "alphabetical rank of row {position}"
            );
            assert_eq!(word.enabled, enabled, "enabled flag of row {position}");
        }
    }

    #[test]
    fn named_indices_match_upstream_constants() {
        // include/constants/easy_chat.h: EC_WORD_THANKS 0 .. EC_WORD_COUNT_ON 41.
        let group = GreetingsGroup::new();
        assert_eq!(GreetingsWordIndex::THANKS.index(), 0);
        assert_eq!(
            group.get(GreetingsWordIndex::I_VE_ARRIVED).text,
            "I'VE ARRIVED"
        );
        assert_eq!(
            group.get(GreetingsWordIndex::WHAT_S_UP_QUES).index.index(),
            27
        );
        assert_eq!(group.get(GreetingsWordIndex::HOO_HAH).text, "HOO-HAH");
        assert_eq!(GreetingsWordIndex::COUNT_ON.index(), 41);
    }

    #[test]
    fn the_alphabetical_ranks_are_a_permutation_of_the_indices() {
        let group = GreetingsGroup::new();
        let mut seen = [false; GREETINGS_WORD_COUNT];
        for word in group.iter() {
            let slot = usize::from(word.alphabetical_order.index());
            assert!(!seen[slot], "index {slot} appears twice");
            seen[slot] = true;
        }
        assert!(seen.iter().all(|&hit| hit));
    }

    #[test]
    fn walking_the_ranks_visits_the_words_in_text_order() {
        let texts: Vec<&str> = GreetingsGroup::new()
            .alphabetical()
            .map(|word| word.text)
            .collect();
        assert_eq!(texts.len(), GREETINGS_WORD_COUNT);
        assert!(
            texts.windows(2).all(|pair| pair[0] < pair[1]),
            "alphabetical walk is not strictly ascending: {texts:?}"
        );
        assert_eq!(texts.first(), Some(&"APOLOGIZE"));
        assert_eq!(texts.last(), Some(&"YO"));
    }

    #[test]
    fn every_greetings_word_is_enabled() {
        let group = GreetingsGroup::new();
        assert_eq!(group.enabled_count(), GREETINGS_WORD_COUNT);
        assert!(group.iter().all(|word| word.enabled));
    }

    #[test]
    fn an_out_of_range_index_is_rejected() {
        assert_eq!(GreetingsWordIndex::new(0), Ok(GreetingsWordIndex::THANKS));
        assert_eq!(
            GreetingsWordIndex::new(41),
            Ok(GreetingsWordIndex::COUNT_ON)
        );
        for raw in [42, 43, u8::MAX] {
            assert_eq!(
                GreetingsWordIndex::new(raw),
                Err(EasyChatError::UnknownGreetingsWord(raw))
            );
        }
        assert_eq!(
            EasyChatError::UnknownGreetingsWord(42).to_string(),
            "unknown greetings easy chat word index `42`"
        );
    }
}
