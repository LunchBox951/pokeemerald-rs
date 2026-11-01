//! Failures from constructing, encoding, or decoding the Pokédex payload.

use std::fmt;

use super::PokedexOrderKind;

/// A Pokédex payload construction or decoding failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PokedexError {
    /// The payload is incomplete.
    Truncated,
    /// The payload carries unread bytes after the last order. The value is how many.
    TrailingBytes(usize),
    /// A length-prefixed string is not valid UTF-8.
    InvalidString,
    /// A text field's byte length exceeds its `u16` wire prefix. The value is that length.
    TextTooLong(usize),
    /// A National Dex number lies beyond `NATIONAL_DEX_OLD_UNOWN_Z`. The value is that number.
    InvalidDexNo(u16),
    /// An entry is keyed by a number beyond `NATIONAL_DEX_COUNT`, the last number that
    /// upstream gives an entry. The value is that number.
    EntryDexNoOutOfRange(u16),
    /// Two entries share a National Dex number. The value is that number.
    DuplicateEntry(u16),
    /// The encoded entries are not in ascending number order.
    EntriesOutOfOrder {
        /// The preceding entry's number.
        previous: u16,
        /// The following entry's number, not above `previous`.
        current: u16,
    },
    /// The payload declares more entries than `NATIONAL_DEX_COUNT + 1` numbers can key.
    /// The value is the declared count.
    TooManyEntries(usize),
    /// An order is longer than the 411 identities it can hold.
    OrderTooLong { order: PokedexOrderKind, len: usize },
    /// An order lists `NATIONAL_DEX_NONE`, which is no Pokémon.
    NoneInOrder(PokedexOrderKind),
    /// An order lists one National Dex number twice.
    DuplicateOrderMember {
        order: PokedexOrderKind,
        /// The repeated number.
        dex_no: u16,
    },
}

impl fmt::Display for PokedexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => write!(f, "Pokédex payload is truncated"),
            Self::TrailingBytes(count) => {
                write!(f, "Pokédex payload has {count} trailing bytes")
            }
            Self::InvalidString => write!(f, "Pokédex text is not valid UTF-8"),
            Self::TextTooLong(len) => {
                write!(
                    f,
                    "Pokédex text of {len} bytes exceeds its u16 length prefix"
                )
            }
            Self::InvalidDexNo(number) => {
                write!(
                    f,
                    "National Dex number {number} is beyond the last identity"
                )
            }
            Self::EntryDexNoOutOfRange(number) => {
                write!(f, "National Dex number {number} has no upstream entry")
            }
            Self::DuplicateEntry(number) => {
                write!(f, "National Dex number {number} has two entries")
            }
            Self::EntriesOutOfOrder { previous, current } => write!(
                f,
                "entry {current} follows entry {previous}: entries must ascend"
            ),
            Self::TooManyEntries(count) => {
                write!(f, "Pokédex payload declares {count} entries")
            }
            Self::OrderTooLong { order, len } => {
                write!(f, "{order} order declares {len} members")
            }
            Self::NoneInOrder(order) => {
                write!(f, "{order} order lists the empty National Dex number")
            }
            Self::DuplicateOrderMember { order, dex_no } => {
                write!(f, "{order} order lists National Dex number {dex_no} twice")
            }
        }
    }
}

impl std::error::Error for PokedexError {}
