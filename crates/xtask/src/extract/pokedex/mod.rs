//! The Pokédex record payload: parser, closure validator, and extraction stage.
//!
//! [`parse`] accepts the text of four authorities and builds an
//! [`assets::pokedex::PokedexData`] only after the whole closure validates:
//!
//! - `src/data/pokemon/pokedex_entries.h`, the `gPokedexEntries` designated rows;
//! - `src/data/pokemon/pokedex_text.h`, every `g*PokedexText` description;
//! - `src/data/pokemon/pokedex_orders.h`, the three list-order initializers; and
//! - `include/constants/pokedex.h`, the National Dex enum and its count aliases.
//!
//! [`extract_pokedex`] reads those files from the checkout, runs [`parse`], and
//! adds the encoded payload as the one raw `pokedex/data` pack entry
//! [`assets::pack::AssetPack::pokedex_data`] opens. Nothing is compiled or
//! executed, and no other Pokédex data is read into the pack.

mod error;
mod parser;

use std::path::Path;

use pack_format::PackWriter;

use self::error::{PokedexSource, PokedexSourceError};
use self::parser::{parse, PokedexSources};
use super::scope::POKEDEX_DATA_ID;
use super::{read_text, ExtractError};

/// Extracts the Pokédex record payload from the checkout.
pub(super) fn extract_pokedex(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let read = |source: PokedexSource| read_text(&upstream.join(source.path()));
    let entries = read(PokedexSource::Entries)?;
    let text = read(PokedexSource::Text)?;
    let orders = read(PokedexSource::Orders)?;
    let constants = read(PokedexSource::Constants)?;
    let data = parse(&PokedexSources {
        entries: &entries,
        text: &text,
        orders: &orders,
        constants: &constants,
    })
    .map_err(|error| source_error(upstream, &error))?;
    writer.push(pack_format::raw_entry(
        POKEDEX_DATA_ID.to_owned(),
        data.encode(),
    ));
    Ok(())
}

/// Maps a parser fault to the authority and line it names, or to the authority
/// that owns the broken rule with line zero when no single line is at fault.
fn source_error(upstream: &Path, error: &PokedexSourceError) -> ExtractError {
    let (source, line) = locate(error);
    let text = error.to_string();
    let located_prefix = format!("{}:{line}: ", source.path());
    ExtractError::PokedexSource {
        path: upstream.join(source.path()),
        line,
        detail: text
            .strip_prefix(&located_prefix)
            .map_or(text.clone(), str::to_owned),
    }
}

fn locate(error: &PokedexSourceError) -> (PokedexSource, usize) {
    use PokedexSourceError as E;
    match error {
        E::Malformed { source, line, .. }
        | E::MalformedNumber { source, line, .. }
        | E::NumberOutOfRange { source, line, .. }
        | E::UnsupportedEscape { source, line, .. }
        | E::UnterminatedString { source, line }
        | E::UnterminatedComment { source, line }
        | E::UnresolvedConstant { source, line, .. }
        | E::UnknownEntryMember { source, line, .. } => (*source, *line),
        E::UnknownOrder { line, .. } | E::OrderMemberNotSpecies { line, .. } => {
            (PokedexSource::Orders, *line)
        }
        E::DuplicateConstant { .. }
        | E::ConstantCount { .. }
        | E::FirstConstantNotNone { .. }
        | E::MissingCountAlias { .. }
        | E::WrongCountAlias { .. } => (PokedexSource::Constants, 0),
        E::DuplicateEntry { .. }
        | E::MissingEntry { .. }
        | E::DuplicateEntryMember { .. }
        | E::MissingEntryMember { .. }
        | E::UnresolvedDescription { .. } => (PokedexSource::Entries, 0),
        E::DuplicateText { .. } => (PokedexSource::Text, 0),
        E::DuplicateOrder { .. } | E::MissingOrder { .. } | E::OrderLength { .. } => {
            (PokedexSource::Orders, 0)
        }
        E::Payload(payload) => (payload_source(payload), 0),
    }
}

/// The authority whose rows or lists broke a payload rule `assets` enforces.
fn payload_source(error: &assets::pokedex::PokedexError) -> PokedexSource {
    use assets::pokedex::PokedexError as P;
    match error {
        P::NoneInOrder(_) | P::DuplicateOrderMember { .. } | P::OrderTooLong { .. } => {
            PokedexSource::Orders
        }
        _ => PokedexSource::Entries,
    }
}

#[cfg(test)]
mod tests;
