//! The Events Easy Chat vocabulary group: word identity, text, enabled flag,
//! and alphabetical ordering for every `gEasyChatGroup_Events` row.
//!
//! Source: `src/data/easy_chat/easy_chat_group_events.h`, the
//! `struct EasyChatWordInfo` layout in `include/easy_chat.h`, and the
//! `EC_WORD_*` indices 0 through 28 in `include/constants/easy_chat.h`. The
//! other groups, the group registry, unlock state, and the editor UI are not
//! part of this table.

use std::error::Error;
use std::fmt;

/// The number of words in the Events group (`EC_GROUP_EVENTS`).
pub const EVENTS_WORD_COUNT: usize = 29;

/// A failed decoding of an Events word index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EasyChatError {
    /// The value is not below [`EVENTS_WORD_COUNT`].
    UnknownEventsWord(u8),
}

impl fmt::Display for EasyChatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownEventsWord(index) => {
                write!(f, "unknown events easy chat word index `{index}`")
            }
        }
    }
}

impl Error for EasyChatError {}

/// A word's index within the Events group (`EC_INDEX` of an `EC_WORD_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventsWordIndex(u8);

impl EventsWordIndex {
    /// Validates a raw group index.
    ///
    /// # Errors
    ///
    /// Returns [`EasyChatError::UnknownEventsWord`] when `index` is
    /// [`EVENTS_WORD_COUNT`] or more.
    pub const fn new(index: u8) -> Result<Self, EasyChatError> {
        if (index as usize) < EVENTS_WORD_COUNT {
            Ok(Self(index))
        } else {
            Err(EasyChatError::UnknownEventsWord(index))
        }
    }

    /// Returns the raw group index.
    #[must_use]
    pub const fn index(self) -> u8 {
        self.0
    }
}

/// One `gEasyChatGroup_Events` row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventsWord {
    /// The row's index, equal to its table position.
    pub index: EventsWordIndex,
    /// The word text (`.text`), in the game's charmap notation; the
    /// `{POKEBLOCK}` charmap token stays a token.
    pub text: &'static str,
    /// The word at this alphabetical position (`.alphabeticalOrder`), as
    /// `SetSelectedWordGroup_GroupMode` reads it while walking rows in order.
    pub alphabetical_order: EventsWordIndex,
    /// Whether the word is offered (`.enabled`).
    pub enabled: bool,
}

/// Lookup over the Events group.
#[derive(Debug, Clone, Copy)]
pub struct EventsGroup {
    rows: &'static [EventsWord; EVENTS_WORD_COUNT],
}

impl EventsGroup {
    /// Creates a handle on the canonical group.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            rows: &EVENTS_WORDS,
        }
    }

    /// Returns the number of words.
    #[must_use]
    pub const fn len(&self) -> usize {
        EVENTS_WORD_COUNT
    }

    /// Returns `false`; the group is never empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }

    /// Returns the row for `index`.
    #[must_use]
    pub const fn get(&self, index: EventsWordIndex) -> &EventsWord {
        &self.rows[index.0 as usize]
    }

    /// Iterates in ascending [`EventsWordIndex`] order.
    pub fn iter(&self) -> impl Iterator<Item = &EventsWord> {
        self.rows.iter()
    }

    /// Iterates the words in alphabetical order, enabled or not.
    pub fn alphabetical(&self) -> impl Iterator<Item = &EventsWord> {
        self.rows.iter().map(|row| self.get(row.alphabetical_order))
    }

    /// Returns how many words have `enabled` set.
    #[must_use]
    pub fn enabled_count(&self) -> usize {
        self.iter().filter(|word| word.enabled).count()
    }
}

impl Default for EventsGroup {
    fn default() -> Self {
        Self::new()
    }
}

macro_rules! define_events_words {
    ($($name:ident = $index:literal => ($text:literal, $alphabetical:ident, $enabled:literal)),+ $(,)?) => {
        impl EventsWordIndex {
            $(
                #[doc = concat!("`EC_WORD_", stringify!($name), "`.")]
                pub const $name: Self = Self($index);
            )+
        }

        const EVENTS_WORDS: [EventsWord; EVENTS_WORD_COUNT] = [
            $(EventsWord {
                index: EventsWordIndex::$name,
                text: $text,
                alphabetical_order: EventsWordIndex::$alphabetical,
                enabled: $enabled,
            }),+
        ];

        const _: () = {
            let indices: [usize; EVENTS_WORD_COUNT] = [$($index),+];
            let mut position = 0;
            while position < EVENTS_WORD_COUNT {
                assert!(indices[position] == position);
                position += 1;
            }
        };
    };
}

#[rustfmt::skip]
define_events_words! {
    APPEAL = 0 => ("APPEAL", APPEAL, true),
    EVENTS = 1 => ("EVENTS", BATTLE_ROOM, true),
    STAY_AT_HOME = 2 => ("STAY-AT-HOME", BATTLE_TOWER, true),
    BERRY = 3 => ("BERRY", BERRY, true),
    CONTEST = 4 => ("CONTEST", BLEND, true),
    MC = 5 => ("MC", CONTEST, true),
    JUDGE = 6 => ("JUDGE", CRUSH, true),
    SUPER = 7 => ("SUPER", DIRECT, true),
    STAGE = 8 => ("STAGE", EVENTS, true),
    HALL_OF_FAME = 9 => ("HALL OF FAME", EVOLUTION, true),
    EVOLUTION = 10 => ("EVOLUTION", FRONTIER, true),
    HYPER = 11 => ("HYPER", HALL_OF_FAME, true),
    BATTLE_TOWER = 12 => ("BATTLE TOWER", HIDDEN, true),
    LEADERS = 13 => ("LEADERS", HYPER, true),
    BATTLE_ROOM = 14 => ("BATTLE ROOM", JUDGE, true),
    HIDDEN = 15 => ("HIDDEN", LEADERS, true),
    SECRET_BASE = 16 => ("SECRET BASE", MASTER, true),
    BLEND = 17 => ("BLEND", MC, true),
    POKEBLOCK = 18 => ("{POKEBLOCK}", POKEBLOCK, true),
    MASTER = 19 => ("MASTER", RANK, true),
    RANK = 20 => ("RANK", RIBBON, true),
    RIBBON = 21 => ("RIBBON", ROOM, true),
    CRUSH = 22 => ("CRUSH", SECRET_BASE, true),
    DIRECT = 23 => ("DIRECT", STAGE, true),
    TOWER = 24 => ("TOWER", STAY_AT_HOME, true),
    UNION = 25 => ("UNION", SUPER, true),
    ROOM = 26 => ("ROOM", TOWER, true),
    WIRELESS = 27 => ("WIRELESS", UNION, true),
    FRONTIER = 28 => ("FRONTIER", WIRELESS, true),
}

#[cfg(test)]
mod tests {
    use super::{EasyChatError, EventsGroup, EventsWordIndex, EVENTS_WORD_COUNT};

    /// `(text, alphabetical rank, enabled)` per row, in upstream table order.
    const ROWS: [(&str, u8, bool); EVENTS_WORD_COUNT] = [
        ("APPEAL", 0, true),
        ("EVENTS", 14, true),
        ("STAY-AT-HOME", 12, true),
        ("BERRY", 3, true),
        ("CONTEST", 17, true),
        ("MC", 4, true),
        ("JUDGE", 22, true),
        ("SUPER", 23, true),
        ("STAGE", 1, true),
        ("HALL OF FAME", 10, true),
        ("EVOLUTION", 28, true),
        ("HYPER", 9, true),
        ("BATTLE TOWER", 15, true),
        ("LEADERS", 11, true),
        ("BATTLE ROOM", 6, true),
        ("HIDDEN", 13, true),
        ("SECRET BASE", 19, true),
        ("BLEND", 5, true),
        ("{POKEBLOCK}", 18, true),
        ("MASTER", 20, true),
        ("RANK", 21, true),
        ("RIBBON", 26, true),
        ("CRUSH", 16, true),
        ("DIRECT", 8, true),
        ("TOWER", 2, true),
        ("UNION", 7, true),
        ("ROOM", 24, true),
        ("WIRELESS", 25, true),
        ("FRONTIER", 27, true),
    ];

    /// The texts in the order the editor lists them.
    const ALPHABETICAL: [&str; EVENTS_WORD_COUNT] = [
        "APPEAL",
        "BATTLE ROOM",
        "BATTLE TOWER",
        "BERRY",
        "BLEND",
        "CONTEST",
        "CRUSH",
        "DIRECT",
        "EVENTS",
        "EVOLUTION",
        "FRONTIER",
        "HALL OF FAME",
        "HIDDEN",
        "HYPER",
        "JUDGE",
        "LEADERS",
        "MASTER",
        "MC",
        "{POKEBLOCK}",
        "RANK",
        "RIBBON",
        "ROOM",
        "SECRET BASE",
        "STAGE",
        "STAY-AT-HOME",
        "SUPER",
        "TOWER",
        "UNION",
        "WIRELESS",
    ];

    /// The collation key: the `{POKEBLOCK}` token sorts as the letters P-O-K-E-B-L-O-C-K.
    fn collation_key(text: &str) -> String {
        text.replace("{POKEBLOCK}", "POKEBLOCK")
    }

    #[test]
    fn every_row_sits_at_its_declared_index() {
        let group = EventsGroup::new();
        assert_eq!(group.len(), EVENTS_WORD_COUNT);
        assert_eq!(group.iter().count(), EVENTS_WORD_COUNT);
        assert!(!group.is_empty());
        for (position, word) in group.iter().enumerate() {
            assert_eq!(usize::from(word.index.index()), position);
            assert_eq!(group.get(word.index), word);
        }
    }

    #[test]
    fn all_twenty_nine_entries_are_pinned_in_upstream_order() {
        let group = EventsGroup::new();
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
        // include/constants/easy_chat.h: EC_WORD_APPEAL 0 .. EC_WORD_FRONTIER 28.
        let group = EventsGroup::new();
        assert_eq!(EventsWordIndex::APPEAL.index(), 0);
        assert_eq!(EventsWordIndex::EVENTS.index(), 1);
        assert_eq!(
            group.get(EventsWordIndex::HALL_OF_FAME).text,
            "HALL OF FAME"
        );
        assert_eq!(group.get(EventsWordIndex::POKEBLOCK).text, "{POKEBLOCK}");
        assert_eq!(group.get(EventsWordIndex::POKEBLOCK).index.index(), 18);
        assert_eq!(EventsWordIndex::FRONTIER.index(), 28);
    }

    #[test]
    fn the_alphabetical_ranks_are_a_permutation_of_the_indices() {
        let group = EventsGroup::new();
        let mut seen = [false; EVENTS_WORD_COUNT];
        for word in group.iter() {
            let slot = usize::from(word.alphabetical_order.index());
            assert!(!seen[slot], "index {slot} appears twice");
            seen[slot] = true;
        }
        assert!(seen.iter().all(|&hit| hit));
    }

    #[test]
    fn walking_the_ranks_visits_the_words_in_text_order() {
        let texts: Vec<&str> = EventsGroup::new()
            .alphabetical()
            .map(|word| word.text)
            .collect();
        assert_eq!(texts, ALPHABETICAL);
        assert!(
            texts
                .windows(2)
                .all(|pair| collation_key(pair[0]) < collation_key(pair[1])),
            "alphabetical walk is not strictly ascending: {texts:?}"
        );
    }

    #[test]
    fn every_events_word_is_enabled() {
        let group = EventsGroup::new();
        assert_eq!(group.enabled_count(), EVENTS_WORD_COUNT);
        assert!(group.iter().all(|word| word.enabled));
    }

    #[test]
    fn an_out_of_range_index_is_rejected() {
        assert_eq!(EventsWordIndex::new(0), Ok(EventsWordIndex::APPEAL));
        assert_eq!(EventsWordIndex::new(28), Ok(EventsWordIndex::FRONTIER));
        for raw in [29, 30, u8::MAX] {
            assert_eq!(
                EventsWordIndex::new(raw),
                Err(EasyChatError::UnknownEventsWord(raw))
            );
        }
        assert_eq!(
            EasyChatError::UnknownEventsWord(29).to_string(),
            "unknown events easy chat word index `29`"
        );
    }
}
