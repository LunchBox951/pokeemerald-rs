//! `src/data/pokemon/pokedex_text.h`: the `g*PokedexText` descriptions.

use std::collections::HashMap;

use super::lexer::Cursor;
use super::Result;
use crate::extract::pokedex::error::{PokedexSource, PokedexSourceError};

pub(super) fn parse_texts(text: &str) -> Result<HashMap<String, String>> {
    let mut cur = Cursor::new(PokedexSource::Text, text)?;
    let mut texts = HashMap::new();
    while !cur.at_end() {
        cur.keyword("const", "`const u8` text definition")?;
        cur.keyword("u8", "`u8` in a text definition")?;
        let symbol = cur.ident("a text symbol")?;
        cur.punct('[', "`[` after the text symbol")?;
        cur.punct(']', "`]` after the text symbol")?;
        cur.punct('=', "`=` after the text symbol")?;
        let value = cur.text_macro()?;
        cur.punct(';', "`;` after the text")?;
        if texts.insert(symbol.clone(), value).is_some() {
            return Err(PokedexSourceError::DuplicateText { symbol });
        }
    }
    Ok(texts)
}
