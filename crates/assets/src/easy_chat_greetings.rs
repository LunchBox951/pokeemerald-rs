//! The Greetings Easy Chat vocabulary group: word identity, text, enabled
//! flag, and alphabetical ordering for every `gEasyChatGroup_Greetings` row.
//!
//! Source: `src/data/easy_chat/easy_chat_group_greetings.h`, the
//! `struct EasyChatWordInfo` layout in `include/easy_chat.h`, and the
//! `EC_WORD_*` indices 0 through 41 in `include/constants/easy_chat.h`. The
//! other groups, the group registry, unlock state, and the editor UI are not
//! part of this table.

use std::error::Error;
use std::fmt;

/// The number of words in the Greetings group (`EC_GROUP_GREETINGS`).
pub const GREETINGS_WORD_COUNT: usize = 42;

/// A failed decoding of a Greetings word index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EasyChatError {
    /// The value is not below [`GREETINGS_WORD_COUNT`].
    UnknownGreetingsWord(u8),
}

impl fmt::Display for EasyChatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownGreetingsWord(index) => {
                write!(f, "unknown greetings easy chat word index `{index}`")
            }
        }
    }
}

impl Error for EasyChatError {}

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
