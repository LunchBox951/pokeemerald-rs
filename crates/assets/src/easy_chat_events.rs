//! The Events Easy Chat vocabulary group: word identity, text, enabled flag,
//! and alphabetical ordering for every `gEasyChatGroup_Events` row, over the
//! shared model in [`easy_chat_word`](crate::easy_chat_word).
//!
//! Source: `src/data/easy_chat/easy_chat_group_events.h` and the `EC_WORD_*`
//! indices 0 through 28 in `include/constants/easy_chat.h`. The other groups,
//! the group registry, unlock state, and the editor UI are not part of this
//! table.

use crate::easy_chat_word::{
    define_easy_chat_group, EasyChatGroup, EasyChatWordIndex, EasyChatWordInfo,
};

/// The number of words in the Events group (`EC_GROUP_EVENTS`).
pub const EVENTS_WORD_COUNT: usize = 29;

/// Marks the Events group in the shared model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Events {}

/// A word's index within the Events group (`EC_INDEX` of an `EC_WORD_*`).
pub type EventsWordIndex = EasyChatWordIndex<Events>;

/// One `gEasyChatGroup_Events` row.
pub type EventsWord = EasyChatWordInfo<Events>;

/// Lookup over the Events group.
pub type EventsGroup = EasyChatGroup<Events>;

#[rustfmt::skip]
define_easy_chat_group! {
    Events, "events", EVENTS_WORDS, EVENTS_WORD_COUNT, {
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
}

#[cfg(test)]
mod tests {
    use crate::easy_chat_word::EasyChatError;

    use super::{EventsGroup, EventsWordIndex, EVENTS_WORD_COUNT};

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
                Err(EasyChatError::UnknownWord {
                    group: "events",
                    index: raw
                })
            );
        }
        assert_eq!(
            EasyChatError::UnknownWord {
                group: "events",
                index: 29
            }
            .to_string(),
            "unknown events easy chat word index `29`"
        );
    }
}
