//! The row, index and group model shared by every Easy Chat word group.
//!
//! Source: `struct EasyChatWordInfo` (`text`, `alphabeticalOrder`, `enabled`)
//! in `include/easy_chat.h:86-91`, which every `gEasyChatGroup_*` table in
//! `src/data/easy_chat/` uses, and the group-local `EC_INDEX` of each
//! `EC_WORD_*` constant in `include/constants/easy_chat.h`.
//!
//! A group is an uninhabited marker type implementing [`EasyChatWordGroup`].
//! The marker is the only thing that tells groups apart, so an index of one
//! group does not type-check against another group's table, and a raw `u8` is
//! checked against the length of its own group's table only.
//!
//! The group dispatch in `src/data/easy_chat/easy_chat_groups.h`, the word
//! tables of the groups that have no marker yet, unlock state and the editor
//! UI are not part of this model.

use std::error::Error;
use std::fmt::{self, Debug};
use std::hash::Hash;
use std::marker::PhantomData;

/// A failed decoding of an Easy Chat word index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EasyChatError {
    /// `index` is not below the word count of the group named `group`.
    UnknownWord {
        /// The group's [`EasyChatWordGroup::NAME`].
        group: &'static str,
        /// The rejected raw index.
        index: u8,
    },
}

impl fmt::Display for EasyChatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownWord { group, index } => {
                write!(f, "unknown {group} easy chat word index `{index}`")
            }
        }
    }
}

impl Error for EasyChatError {}

/// One word group: its marker type and the rows of its upstream table.
///
/// The table length is the group's word count; a group needs at most 256 rows
/// so that every position fits an [`EasyChatWordIndex`].
pub trait EasyChatWordGroup:
    Debug + Clone + Copy + PartialEq + Eq + PartialOrd + Ord + Hash + 'static
{
    /// The lower-case group name used in [`EasyChatError`] messages.
    const NAME: &'static str;

    /// The rows in upstream table order, each at its own position.
    const WORDS: &'static [EasyChatWordInfo<Self>];
}

/// A word's index within group `G` (`EC_INDEX` of an `EC_WORD_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EasyChatWordIndex<G: EasyChatWordGroup>(u8, PhantomData<G>);

impl<G: EasyChatWordGroup> EasyChatWordIndex<G> {
    /// Validates a raw group index.
    ///
    /// # Errors
    ///
    /// Returns [`EasyChatError::UnknownWord`] when `index` is the number of
    /// words in `G` or more.
    pub const fn new(index: u8) -> Result<Self, EasyChatError> {
        if (index as usize) < G::WORDS.len() {
            Ok(Self(index, PhantomData))
        } else {
            Err(EasyChatError::UnknownWord {
                group: G::NAME,
                index,
            })
        }
    }

    #[must_use]
    pub const fn index(self) -> u8 {
        self.0
    }

    /// Builds the index of a table position without a range check; only
    /// [`define_easy_chat_group`] calls it, after asserting that every
    /// declared position equals the row's place in the table.
    #[must_use]
    pub(crate) const fn at_table_position(index: u8) -> Self {
        Self(index, PhantomData)
    }
}

/// One row of a group's table (`struct EasyChatWordInfo`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EasyChatWordInfo<G: EasyChatWordGroup> {
    /// The row's index, equal to its table position.
    pub index: EasyChatWordIndex<G>,
    /// The word text (`.text`), in the game's charmap notation; charmap
    /// tokens such as `{POKEBLOCK}` stay tokens.
    pub text: &'static str,
    /// The word at this alphabetical position (`.alphabeticalOrder`), as
    /// `SetSelectedWordGroup_GroupMode` reads it while walking rows in order.
    pub alphabetical_order: EasyChatWordIndex<G>,
    /// Whether the word is offered (`.enabled`).
    pub enabled: bool,
}

/// Lookup over the canonical table of group `G`.
#[derive(Debug, Clone, Copy)]
pub struct EasyChatGroup<G: EasyChatWordGroup> {
    group: PhantomData<G>,
}

impl<G: EasyChatWordGroup> EasyChatGroup<G> {
    #[must_use]
    pub const fn new() -> Self {
        Self { group: PhantomData }
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        G::WORDS.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        G::WORDS.is_empty()
    }

    #[must_use]
    pub const fn get(&self, index: EasyChatWordIndex<G>) -> &'static EasyChatWordInfo<G> {
        &G::WORDS[index.0 as usize]
    }

    pub fn iter(&self) -> impl Iterator<Item = &'static EasyChatWordInfo<G>> {
        G::WORDS.iter()
    }

    /// Walks the rows in table order, yielding the row each `alphabetical_order`
    /// names; disabled words are included.
    pub fn alphabetical(&self) -> impl Iterator<Item = &'static EasyChatWordInfo<G>> {
        let group = *self;
        G::WORDS
            .iter()
            .map(move |row| group.get(row.alphabetical_order))
    }

    #[must_use]
    pub fn enabled_count(&self) -> usize {
        self.iter().filter(|word| word.enabled).count()
    }
}

impl<G: EasyChatWordGroup> Default for EasyChatGroup<G> {
    fn default() -> Self {
        Self::new()
    }
}

/// Declares a group's marker impl, its named `EC_WORD_*` index constants and
/// its table from rows written as upstream lists them.
macro_rules! define_easy_chat_group {
    (
        $group:ident, $label:literal, $table:ident, $count:ident,
        { $($name:ident = $index:literal => ($text:literal, $alphabetical:ident, $enabled:literal)),+ $(,)? }
    ) => {
        impl $crate::easy_chat_word::EasyChatWordGroup for $group {
            const NAME: &'static str = $label;
            const WORDS: &'static [$crate::easy_chat_word::EasyChatWordInfo<$group>] = &$table;
        }

        impl $crate::easy_chat_word::EasyChatWordIndex<$group> {
            $(
                #[doc = concat!("`EC_WORD_", stringify!($name), "`.")]
                pub const $name: Self = Self::at_table_position($index);
            )+
        }

        const $table: [$crate::easy_chat_word::EasyChatWordInfo<$group>; $count] = [
            $($crate::easy_chat_word::EasyChatWordInfo {
                index: $crate::easy_chat_word::EasyChatWordIndex::<$group>::$name,
                text: $text,
                alphabetical_order: $crate::easy_chat_word::EasyChatWordIndex::<$group>::$alphabetical,
                enabled: $enabled,
            }),+
        ];

        const _: () = {
            let indices: [usize; $count] = [$($index),+];
            let mut position = 0;
            while position < $count {
                assert!(indices[position] == position);
                position += 1;
            }
        };
    };
}

pub(crate) use define_easy_chat_group;
