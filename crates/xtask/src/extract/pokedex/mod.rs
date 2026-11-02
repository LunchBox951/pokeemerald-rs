//! Parser and closure validator for the upstream Pokédex tables.
//!
//! [`parse`] accepts the text of four authorities and builds an
//! [`assets::pokedex::PokedexData`] only after the whole closure validates:
//!
//! - `src/data/pokemon/pokedex_entries.h`, the `gPokedexEntries` designated rows;
//! - `src/data/pokemon/pokedex_text.h`, every `g*PokedexText` description;
//! - `src/data/pokemon/pokedex_orders.h`, the three list-order initializers; and
//! - `include/constants/pokedex.h`, the National Dex enum and its count aliases.
//!
//! It performs no filesystem access and writes no pack entry; reading the
//! checkout and publishing the payload belong to the extraction stage.

// Nothing outside the tests calls the parser until the extraction stage lands.
#![cfg_attr(
    not(test),
    expect(dead_code, reason = "consumed by the extraction stage, not yet wired")
)]

mod error;
mod parser;

#[expect(unused_imports, reason = "re-exported for the extraction stage")]
pub use error::{PokedexSource, PokedexSourceError};
#[expect(unused_imports, reason = "re-exported for the extraction stage")]
pub use parser::{parse, PokedexSources};
