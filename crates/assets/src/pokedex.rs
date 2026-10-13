//! Backend-neutral Pokédex record payload (S-4, issue #2016): the per-species
//! entries of upstream `gPokedexEntries` (`src/data/pokemon/pokedex_entries.h`)
//! and the three list orders of `src/data/pokemon/pokedex_orders.h`, in the one
//! raw pack entry both extraction backends fill.
//!
//! [`PokedexData::encode`] and [`PokedexData::decode`] are inverses over the
//! wire format owned by [`codec`]; construction and decoding enforce the same
//! rules, so a value that exists always encodes.
//!
//! The orders are independent lists: upstream's alphabetical order carries 411
//! members (the 386 species plus the 25 `NATIONAL_DEX_OLD_UNOWN_*`
//! identities) while weight and height carry 386 each. The schema therefore
//! imposes no equal-length rule and no rule that every order member has an
//! entry; it only forbids a repeated or empty member within one order.

mod codec;
mod error;

use std::fmt;

pub use error::PokedexError;

/// The last National Dex number upstream gives an entry (`NATIONAL_DEX_DEOXYS`).
pub const NATIONAL_DEX_COUNT: u16 = 386;

/// How many `NATIONAL_DEX_OLD_UNOWN_B`..`NATIONAL_DEX_OLD_UNOWN_Z` identities follow
/// [`NATIONAL_DEX_COUNT`].
pub const NATIONAL_DEX_OLD_UNOWN_COUNT: u16 = 25;

/// The last National Dex identity (`NATIONAL_DEX_OLD_UNOWN_Z`).
pub const NATIONAL_DEX_LAST: u16 = NATIONAL_DEX_COUNT + NATIONAL_DEX_OLD_UNOWN_COUNT;

/// A National Dex identity: `0` (`NATIONAL_DEX_NONE`) through
/// [`NATIONAL_DEX_LAST`], including the old-Unown identities above
/// [`NATIONAL_DEX_COUNT`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NationalDexNo(u16);

impl NationalDexNo {
    /// `NATIONAL_DEX_NONE`, the placeholder entry's identity.
    pub const NONE: Self = Self(0);

    /// Wraps `number`.
    ///
    /// # Errors
    ///
    /// [`PokedexError::InvalidDexNo`] when `number` exceeds [`NATIONAL_DEX_LAST`].
    pub const fn new(number: u16) -> Result<Self, PokedexError> {
        if number > NATIONAL_DEX_LAST {
            return Err(PokedexError::InvalidDexNo(number));
        }
        Ok(Self(number))
    }

    /// The numeric identity.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// One `struct PokedexEntry` (`include/pokedex.h:21-32`), minus its unused
/// `u16` padding field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PokedexEntry {
    /// `categoryName`, e.g. `SEED`.
    pub category: String,
    /// `description`, the page text with its `\n` line breaks.
    pub description: String,
    /// `height`, in decimeters.
    pub height: u16,
    /// `weight`, in hectograms.
    pub weight: u16,
    /// `pokemonScale`, the Pokémon sprite's affine scale in 1/256 units.
    pub pokemon_scale: u16,
    /// `pokemonOffset`, the Pokémon sprite's vertical offset in pixels. Upstream
    /// declares `u16` but writes negative literals; this is that value as signed.
    pub pokemon_offset: i16,
    /// `trainerScale`, the trainer sprite's affine scale in 1/256 units.
    pub trainer_scale: u16,
    /// `trainerOffset`, the trainer sprite's vertical offset in pixels, signed like
    /// [`pokemon_offset`](Self::pokemon_offset).
    pub trainer_offset: i16,
}

/// Which of the three Pokédex list orders a value or failure concerns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PokedexOrderKind {
    /// `gPokedexOrder_Alphabetical`.
    Alphabetical,
    /// `gPokedexOrder_Weight`.
    Weight,
    /// `gPokedexOrder_Height`.
    Height,
}

impl fmt::Display for PokedexOrderKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Alphabetical => "alphabetical",
            Self::Weight => "weight",
            Self::Height => "height",
        })
    }
}

/// The three list orders, each an independent sequence of National Dex identities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PokedexOrders {
    alphabetical: Vec<NationalDexNo>,
    weight: Vec<NationalDexNo>,
    height: Vec<NationalDexNo>,
}

impl PokedexOrders {
    /// Bundles the orders, preserving each sequence as given.
    ///
    /// # Errors
    ///
    /// [`PokedexError::NoneInOrder`] for a [`NationalDexNo::NONE`] member;
    /// [`PokedexError::DuplicateOrderMember`] for a repeated member. An order
    /// cannot exceed [`NATIONAL_DEX_LAST`] members without repeating one.
    pub fn new(
        alphabetical: Vec<NationalDexNo>,
        weight: Vec<NationalDexNo>,
        height: Vec<NationalDexNo>,
    ) -> Result<Self, PokedexError> {
        check_order(PokedexOrderKind::Alphabetical, &alphabetical)?;
        check_order(PokedexOrderKind::Weight, &weight)?;
        check_order(PokedexOrderKind::Height, &height)?;
        Ok(Self {
            alphabetical,
            weight,
            height,
        })
    }

    /// `gPokedexOrder_Alphabetical`.
    #[must_use]
    pub fn alphabetical(&self) -> &[NationalDexNo] {
        &self.alphabetical
    }

    /// `gPokedexOrder_Weight`, lightest first.
    #[must_use]
    pub fn weight(&self) -> &[NationalDexNo] {
        &self.weight
    }

    /// `gPokedexOrder_Height`, shortest first.
    #[must_use]
    pub fn height(&self) -> &[NationalDexNo] {
        &self.height
    }
}

fn check_order(order: PokedexOrderKind, members: &[NationalDexNo]) -> Result<(), PokedexError> {
    let mut seen = [false; NATIONAL_DEX_LAST as usize + 1];
    for member in members {
        if *member == NationalDexNo::NONE {
            return Err(PokedexError::NoneInOrder(order));
        }
        let slot = &mut seen[usize::from(member.get())];
        if *slot {
            return Err(PokedexError::DuplicateOrderMember {
                order,
                dex_no: member.get(),
            });
        }
        *slot = true;
    }
    Ok(())
}

/// Every Pokédex entry and order the pack's `pokedex/data` payload carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PokedexData {
    entries: Vec<(NationalDexNo, PokedexEntry)>,
    orders: PokedexOrders,
}

impl PokedexData {
    /// Bundles `entries` (in any order, sorted by identity) with `orders`.
    ///
    /// # Errors
    ///
    /// [`PokedexError::EntryDexNoOutOfRange`] for an entry above
    /// [`NATIONAL_DEX_COUNT`]; [`PokedexError::DuplicateEntry`] for a repeated
    /// identity; [`PokedexError::TextTooLong`] for text beyond its `u16` prefix.
    pub fn new(
        mut entries: Vec<(NationalDexNo, PokedexEntry)>,
        orders: PokedexOrders,
    ) -> Result<Self, PokedexError> {
        entries.sort_by_key(|(dex_no, _)| *dex_no);
        for (dex_no, entry) in &entries {
            if dex_no.get() > NATIONAL_DEX_COUNT {
                return Err(PokedexError::EntryDexNoOutOfRange(dex_no.get()));
            }
            codec::check_text_len(&entry.category)?;
            codec::check_text_len(&entry.description)?;
        }
        if let Some(pair) = entries.windows(2).find(|pair| pair[0].0 == pair[1].0) {
            return Err(PokedexError::DuplicateEntry(pair[0].0.get()));
        }
        Ok(Self { entries, orders })
    }

    /// The entries in ascending identity order.
    #[must_use]
    pub fn entries(&self) -> &[(NationalDexNo, PokedexEntry)] {
        &self.entries
    }

    /// The entry for `dex_no`, if the payload carries one.
    #[must_use]
    pub fn entry(&self, dex_no: NationalDexNo) -> Option<&PokedexEntry> {
        self.entries
            .binary_search_by_key(&dex_no, |(key, _)| *key)
            .ok()
            .map(|index| &self.entries[index].1)
    }

    /// The three list orders.
    #[must_use]
    pub fn orders(&self) -> &PokedexOrders {
        &self.orders
    }

    /// Serializes to the `pokedex/data` wire format.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        codec::encode(self)
    }

    /// Parses the `pokedex/data` wire format.
    ///
    /// # Errors
    ///
    /// [`PokedexError::Truncated`], [`PokedexError::InvalidString`], or
    /// [`PokedexError::TrailingBytes`] for a malformed payload;
    /// [`PokedexError::TooManyEntries`] or [`PokedexError::OrderTooLong`] for a
    /// count beyond what the identities can fill;
    /// [`PokedexError::EntriesOutOfOrder`] or [`PokedexError::DuplicateEntry`]
    /// for entries not strictly ascending; plus every error of
    /// [`NationalDexNo::new`], [`PokedexOrders::new`], and
    /// [`PokedexData::new`].
    pub fn decode(bytes: &[u8]) -> Result<Self, PokedexError> {
        codec::decode(bytes)
    }
}

#[cfg(test)]
mod tests;
