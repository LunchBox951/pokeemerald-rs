//! Closure validation for the Pokédex tables.
//!
//! The lexer and the four per-file grammars live in the child modules; this one
//! joins their output and validates the whole closure.

mod constants;
mod entries;
mod lexer;
mod orders;
mod texts;

use std::collections::{HashMap, HashSet};

use assets::pokedex::{
    NationalDexNo, PokedexData, PokedexEntry, PokedexOrders, NATIONAL_DEX_COUNT,
};

use self::constants::{parse_constants, Constants};
use self::entries::{parse_entries, EntryRow};
use self::orders::parse_orders;
use self::texts::parse_texts;
use super::error::{PokedexSource, PokedexSourceError};

type Result<T> = std::result::Result<T, PokedexSourceError>;

/// The four authority texts [`parse`] reads.
#[derive(Debug, Clone, Copy)]
pub struct PokedexSources<'a> {
    /// `src/data/pokemon/pokedex_entries.h`.
    pub entries: &'a str,
    /// `src/data/pokemon/pokedex_text.h`.
    pub text: &'a str,
    /// `src/data/pokemon/pokedex_orders.h`.
    pub orders: &'a str,
    /// `include/constants/pokedex.h`.
    pub constants: &'a str,
}

/// Parses the four authorities and validates their closure.
///
/// # Errors
///
/// A [`PokedexSourceError`] naming the first fault: grammar, numeric, duplicate,
/// missing, unresolved-symbol, or order-length violation.
pub fn parse(sources: &PokedexSources<'_>) -> Result<PokedexData> {
    let constants = parse_constants(sources.constants)?;
    let texts = parse_texts(sources.text)?;
    let rows = parse_entries(sources.entries)?;
    let orders = parse_orders(sources.orders, &constants)?;

    let entries = resolve_entries(rows, &constants, &texts)?;
    let orders = PokedexOrders::new(orders.alphabetical, orders.weight, orders.height)?;
    Ok(PokedexData::new(entries, orders)?)
}

fn resolve_entries(
    rows: Vec<EntryRow>,
    constants: &Constants,
    texts: &HashMap<String, String>,
) -> Result<Vec<(NationalDexNo, PokedexEntry)>> {
    let mut entries = Vec::with_capacity(rows.len());
    let mut seen = HashSet::new();
    for row in rows {
        let dex_no = constants.resolve(PokedexSource::Entries, row.line, &row.symbol)?;
        if dex_no.get() > NATIONAL_DEX_COUNT {
            return Err(assets::pokedex::PokedexError::EntryDexNoOutOfRange(dex_no.get()).into());
        }
        if !seen.insert(row.symbol.clone()) {
            return Err(PokedexSourceError::DuplicateEntry { symbol: row.symbol });
        }
        let Some(description) = texts.get(&row.description) else {
            return Err(PokedexSourceError::UnresolvedDescription {
                entry: row.symbol,
                symbol: row.description,
            });
        };
        entries.push((
            dex_no,
            PokedexEntry {
                category: row.category,
                description: description.clone(),
                height: row.height,
                weight: row.weight,
                pokemon_scale: row.pokemon_scale,
                pokemon_offset: row.pokemon_offset,
                trainer_scale: row.trainer_scale,
                trainer_offset: row.trainer_offset,
            },
        ));
    }
    let present: HashSet<u16> = entries.iter().map(|(no, _)| no.get()).collect();
    if let Some(missing) = (0..=NATIONAL_DEX_COUNT).find(|number| !present.contains(number)) {
        return Err(PokedexSourceError::MissingEntry { dex_no: missing });
    }
    Ok(entries)
}

#[cfg(test)]
mod tests;
