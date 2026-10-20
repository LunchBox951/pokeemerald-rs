//! `include/constants/pokedex.h`: the National Dex enum and its count aliases.

use std::collections::HashMap;

use assets::pokedex::{NationalDexNo, NATIONAL_DEX_COUNT, NATIONAL_DEX_LAST};

use super::lexer::{Cursor, Tok};
use super::Result;
use crate::extract::pokedex::error::{PokedexSource, PokedexSourceError};

/// The count aliases and the constants they must name.
const COUNT_ALIASES: [(&str, &str, u16); 3] = [
    ("KANTO_DEX_COUNT", "NATIONAL_DEX_MEW", 151),
    ("JOHTO_DEX_COUNT", "NATIONAL_DEX_CELEBI", 251),
    (
        "NATIONAL_DEX_COUNT",
        "NATIONAL_DEX_DEOXYS",
        NATIONAL_DEX_COUNT,
    ),
];

pub(super) struct Constants {
    pub(super) values: HashMap<String, u16>,
}

impl Constants {
    pub(super) fn resolve(
        &self,
        source: PokedexSource,
        line: usize,
        symbol: &str,
    ) -> Result<NationalDexNo> {
        let unresolved = || PokedexSourceError::UnresolvedConstant {
            source,
            line,
            symbol: symbol.to_owned(),
        };
        let value = *self.values.get(symbol).ok_or_else(unresolved)?;
        Ok(NationalDexNo::new(value)?)
    }
}

pub(super) fn parse_constants(text: &str) -> Result<Constants> {
    let mut cur = Cursor::new(PokedexSource::Constants, text)?;
    while matches!(cur.peek(), Some(Tok::Directive(_))) {
        cur.pos += 1;
    }
    cur.keyword("enum", "the National Dex `enum {`")?;
    cur.punct('{', "`{` opening the National Dex enum")?;
    let mut values: HashMap<String, u16> = HashMap::new();
    let mut count = 0usize;
    let mut first = true;
    loop {
        if cur.eat_punct('}') {
            break;
        }
        let name = cur.ident("a National Dex constant")?;
        if first {
            first = false;
            if name != "NATIONAL_DEX_NONE" {
                return Err(PokedexSourceError::FirstConstantNotNone { name });
            }
        }
        let value = u16::try_from(count).unwrap_or(u16::MAX);
        if values.insert(name.clone(), value).is_some() {
            return Err(PokedexSourceError::DuplicateConstant { name });
        }
        count += 1;
        if !cur.eat_punct(',') {
            cur.punct('}', "`,` or `}` after a constant")?;
            break;
        }
    }
    cur.punct(';', "`;` closing the National Dex enum")?;
    let expected = usize::from(NATIONAL_DEX_LAST) + 1;
    if first {
        return Err(PokedexSourceError::FirstConstantNotNone {
            name: String::new(),
        });
    }
    if count != expected {
        return Err(PokedexSourceError::ConstantCount {
            expected,
            actual: count,
        });
    }

    let mut aliases: HashMap<String, String> = HashMap::new();
    for token in &cur.tokens[cur.pos..] {
        let Tok::Directive(body) = &token.tok else {
            continue;
        };
        let words: Vec<&str> = body.split_whitespace().collect();
        let (Some(&directive), Some(&name)) = (words.first(), words.get(1)) else {
            continue;
        };
        if !matches!(directive, "define" | "undef") {
            continue;
        }
        // Redefining or undefining a declared constant would change its identity.
        if values.contains_key(name) {
            return Err(PokedexSourceError::Malformed {
                source: PokedexSource::Constants,
                line: token.line,
                expected: "no preprocessor override of a National Dex constant",
            });
        }
        if !COUNT_ALIASES.iter().any(|(alias, _, _)| *alias == name) {
            continue;
        }
        // A count alias must be defined once as `#define NAME CONSTANT`.
        if directive != "define" || words.len() != 3 || aliases.contains_key(name) {
            return Err(PokedexSourceError::Malformed {
                source: PokedexSource::Constants,
                line: token.line,
                expected: "one plain `#define` of each count alias",
            });
        }
        aliases.insert(name.to_owned(), words[2].to_owned());
    }
    for (name, _, expected) in COUNT_ALIASES {
        let Some(target) = aliases.get(name) else {
            return Err(PokedexSourceError::MissingCountAlias { name });
        };
        let actual = values.get(target).copied();
        if actual != Some(expected) {
            return Err(PokedexSourceError::WrongCountAlias {
                name,
                expected,
                actual,
            });
        }
    }
    Ok(Constants { values })
}
