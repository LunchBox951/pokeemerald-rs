//! The Trainer Easy Chat vocabulary group: word identity, text, enabled flag,
//! and alphabetical ordering for every `gEasyChatGroup_Trainer` row.
//!
//! Source: `src/data/easy_chat/easy_chat_group_trainer.h` and the
//! `EC_WORD_*` indices 0 through 26 in `include/constants/easy_chat.h`. The
//! other groups, the group registry, unlock state, and the editor UI are not
//! part of this table.

use std::error::Error;
use std::fmt;

/// The number of words in the Trainer group (`EC_GROUP_TRAINER`).
pub const TRAINER_WORD_COUNT: usize = 27;

/// A failed decoding of a Trainer word index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EasyChatError {
    /// The value is not below [`TRAINER_WORD_COUNT`].
    UnknownTrainerWord(u8),
}

impl fmt::Display for EasyChatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownTrainerWord(index) => {
                write!(f, "unknown trainer easy chat word index `{index}`")
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
