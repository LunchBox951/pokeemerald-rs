//! Shared reading of upstream `_()` string definitions: the C tokenizer, the
//! `charmap.txt` mapper, and the definition scanner.
//!
//! Consumers pick the symbols they need; nothing here knows any message
//! inventory.

mod charmap;
mod lex;

use std::collections::HashMap;
use std::fmt;

pub(in crate::extract) use charmap::Charmap;
pub(in crate::extract) use lex::{tokenize, Tok, Token};

/// A parse failure inside one source file.
#[derive(Debug)]
pub(in crate::extract) struct Fault {
    pub(in crate::extract) line: usize,
    pub(in crate::extract) detail: String,
}

impl Fault {
    pub(in crate::extract) fn new(line: usize, detail: impl fmt::Display) -> Self {
        Self {
            line,
            detail: detail.to_string(),
        }
    }
}

/// Skip zero or more `[...]` groups from `j`; returns the index after them.
pub(in crate::extract) fn skip_brackets(tokens: &[Token], mut j: usize) -> usize {
    while tokens.get(j).map(|t| &t.tok) == Some(&Tok::Punct('[')) {
        let mut depth = 0usize;
        while let Some(token) = tokens.get(j) {
            match token.tok {
                Tok::Punct('[') => depth += 1,
                Tok::Punct(']') => depth -= 1,
                _ => {}
            }
            j += 1;
            if depth == 0 {
                break;
            }
        }
    }
    j
}

pub(in crate::extract) fn is_punct(tokens: &[Token], index: usize, p: char) -> bool {
    tokens.get(index).map(|t| &t.tok) == Some(&Tok::Punct(p))
}

pub(in crate::extract) fn ident(tokens: &[Token], index: usize) -> Option<&str> {
    match tokens.get(index).map(|t| &t.tok) {
        Some(Tok::Ident(name)) => Some(name),
        _ => None,
    }
}

/// Adjacent literals from `start` up to the closing `)`: raw body and next index.
pub(in crate::extract) fn string_args(
    tokens: &[Token],
    mut start: usize,
) -> Result<(String, usize), Fault> {
    let line = tokens.get(start).map_or(0, |t| t.line);
    let mut body = String::new();
    while let Some(token) = tokens.get(start) {
        match &token.tok {
            Tok::Str(piece) => body.push_str(piece),
            Tok::Punct(')') => return Ok((body, start + 1)),
            _ => return Err(Fault::new(token.line, "unexpected token inside `_()`")),
        }
        start += 1;
    }
    Err(Fault::new(line, "unterminated `_()`"))
}

/// Every `NAME[...] = _("...")` definition: raw body and line. Forward
/// declarations carry no initializer and are skipped; a second definition of a
/// name is refused.
pub(in crate::extract) fn parse_definitions(
    tokens: &[Token],
) -> Result<HashMap<String, (String, usize)>, Fault> {
    let mut out = HashMap::new();
    for (i, token) in tokens.iter().enumerate() {
        let Tok::Ident(name) = &token.tok else {
            continue;
        };
        let j = skip_brackets(tokens, i + 1);
        if j == i + 1
            || !is_punct(tokens, j, '=')
            || ident(tokens, j + 1) != Some("_")
            || !is_punct(tokens, j + 2, '(')
        {
            continue;
        }
        let (body, _) = string_args(tokens, j + 3)?;
        if out.insert(name.clone(), (body, token.line)).is_some() {
            return Err(Fault::new(token.line, format!("`{name}` is defined twice")));
        }
    }
    Ok(out)
}
