//! The `pokedex/data` wire format: all integers little-endian, strings a `u16`
//! byte length then UTF-8.
//!
//! ```text
//! entry_count: u16
//! entry_count x:
//!   dex_no: u16, category: string, height: u16, weight: u16, description: string,
//!   pokemon_scale: u16, pokemon_offset: i16, trainer_scale: u16, trainer_offset: i16
//! for each of alphabetical, weight, height:
//!   member_count: u16, member_count x dex_no: u16
//! ```
//!
//! Entries ascend strictly by `dex_no`. No byte follows the height order.

use super::{
    NationalDexNo, PokedexData, PokedexEntry, PokedexError, PokedexOrderKind, PokedexOrders,
    NATIONAL_DEX_COUNT, NATIONAL_DEX_LAST,
};

const MAX_ENTRIES: usize = NATIONAL_DEX_COUNT as usize + 1;
const MAX_ORDER_MEMBERS: usize = NATIONAL_DEX_LAST as usize;

pub(super) fn check_text_len(text: &str) -> Result<(), PokedexError> {
    if u16::try_from(text.len()).is_err() {
        return Err(PokedexError::TextTooLong(text.len()));
    }
    Ok(())
}

pub(super) fn encode(data: &PokedexData) -> Vec<u8> {
    let mut out = Vec::new();
    put_count(&mut out, data.entries.len());
    for (dex_no, entry) in &data.entries {
        put_u16(&mut out, dex_no.get());
        put_string(&mut out, &entry.category);
        put_u16(&mut out, entry.height);
        put_u16(&mut out, entry.weight);
        put_string(&mut out, &entry.description);
        put_u16(&mut out, entry.pokemon_scale);
        out.extend_from_slice(&entry.pokemon_offset.to_le_bytes());
        put_u16(&mut out, entry.trainer_scale);
        out.extend_from_slice(&entry.trainer_offset.to_le_bytes());
    }
    for order in [
        data.orders.alphabetical(),
        data.orders.weight(),
        data.orders.height(),
    ] {
        put_count(&mut out, order.len());
        for member in order {
            put_u16(&mut out, member.get());
        }
    }
    out
}

fn put_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// Counts are bounded by construction: entries by [`MAX_ENTRIES`], orders by
/// their duplicate-free members.
fn put_count(out: &mut Vec<u8>, count: usize) {
    put_u16(
        out,
        u16::try_from(count).expect("count bounded by dex identities"),
    );
}

/// Length bounded by [`check_text_len`] at construction.
fn put_string(out: &mut Vec<u8>, text: &str) {
    put_count(out, text.len());
    out.extend_from_slice(text.as_bytes());
}

struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], PokedexError> {
        let (head, tail) = self
            .bytes
            .split_at_checked(len)
            .ok_or(PokedexError::Truncated)?;
        self.bytes = tail;
        Ok(head)
    }

    fn u16(&mut self) -> Result<u16, PokedexError> {
        let bytes = self.take(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn i16(&mut self) -> Result<i16, PokedexError> {
        let bytes = self.take(2)?;
        Ok(i16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn string(&mut self) -> Result<String, PokedexError> {
        let len = usize::from(self.u16()?);
        std::str::from_utf8(self.take(len)?)
            .map(str::to_owned)
            .map_err(|_| PokedexError::InvalidString)
    }

    fn dex_no(&mut self) -> Result<NationalDexNo, PokedexError> {
        NationalDexNo::new(self.u16()?)
    }

    fn order(&mut self, kind: PokedexOrderKind) -> Result<Vec<NationalDexNo>, PokedexError> {
        let len = usize::from(self.u16()?);
        if len > MAX_ORDER_MEMBERS {
            return Err(PokedexError::OrderTooLong { order: kind, len });
        }
        (0..len).map(|_| self.dex_no()).collect()
    }
}

pub(super) fn decode(bytes: &[u8]) -> Result<PokedexData, PokedexError> {
    let mut reader = Reader { bytes };
    let count = usize::from(reader.u16()?);
    if count > MAX_ENTRIES {
        return Err(PokedexError::TooManyEntries(count));
    }
    let mut entries: Vec<(NationalDexNo, PokedexEntry)> = Vec::with_capacity(count);
    for _ in 0..count {
        let dex_no = reader.dex_no()?;
        if let Some((previous, _)) = entries.last() {
            if dex_no == *previous {
                return Err(PokedexError::DuplicateEntry(dex_no.get()));
            }
            if dex_no < *previous {
                return Err(PokedexError::EntriesOutOfOrder {
                    previous: previous.get(),
                    current: dex_no.get(),
                });
            }
        }
        let category = reader.string()?;
        let height = reader.u16()?;
        let weight = reader.u16()?;
        let description = reader.string()?;
        let entry = PokedexEntry {
            category,
            description,
            height,
            weight,
            pokemon_scale: reader.u16()?,
            pokemon_offset: reader.i16()?,
            trainer_scale: reader.u16()?,
            trainer_offset: reader.i16()?,
        };
        entries.push((dex_no, entry));
    }
    let alphabetical = reader.order(PokedexOrderKind::Alphabetical)?;
    let weight = reader.order(PokedexOrderKind::Weight)?;
    let height = reader.order(PokedexOrderKind::Height)?;
    if !reader.bytes.is_empty() {
        return Err(PokedexError::TrailingBytes(reader.bytes.len()));
    }
    PokedexData::new(entries, PokedexOrders::new(alphabetical, weight, height)?)
}
