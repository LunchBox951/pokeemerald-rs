//! The pinned `charmap.txt` and the authored-string mapper, ported from
//! `tools/preproc/{charmap,string_parser}.cpp`.

use std::collections::HashMap;

use super::Fault;

#[derive(Debug, Default)]
pub(in crate::extract) struct Charmap {
    chars: HashMap<char, Vec<u8>>,
    escapes: HashMap<char, Vec<u8>>,
    constants: HashMap<String, Vec<u8>>,
}

fn insert<K: std::hash::Hash + Eq + std::fmt::Debug>(
    map: &mut HashMap<K, Vec<u8>>,
    key: K,
    bytes: Vec<u8>,
    line: usize,
) -> Result<(), Fault> {
    let detail = format!("charmap defines {key:?} twice");
    match map.insert(key, bytes) {
        Some(_) => Err(Fault::new(line, detail)),
        None => Ok(()),
    }
}

fn hex_bytes(text: &str, line: usize) -> Result<Vec<u8>, Fault> {
    let text = text.split('@').next().unwrap_or("");
    let bytes = text
        .split_whitespace()
        .map(|h| {
            if h.len() == 2 {
                u8::from_str_radix(h, 16).ok()
            } else {
                None
            }
        })
        .collect::<Option<Vec<u8>>>()
        .filter(|b| !b.is_empty());
    bytes.ok_or_else(|| {
        Fault::new(
            line,
            format!("charmap value `{}` is not hex bytes", text.trim()),
        )
    })
}

impl Charmap {
    pub(in crate::extract) fn parse(text: &str) -> Result<Self, Fault> {
        let mut map = Self::default();
        for (index, raw) in text.lines().enumerate() {
            let line = index + 1;
            let trimmed = raw.trim();
            if trimmed.is_empty() || trimmed.starts_with('@') {
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix('\'') {
                let mut it = rest.chars();
                let first = it
                    .next()
                    .ok_or_else(|| Fault::new(line, "empty charmap key"))?;
                let (key, escaped) = if first == '\\' {
                    let second = it
                        .next()
                        .ok_or_else(|| Fault::new(line, "dangling escape"))?;
                    (second, !matches!(second, '\'' | '\\' | '"'))
                } else {
                    (first, false)
                };
                if it.next() != Some('\'') {
                    return Err(Fault::new(line, "charmap key is not one quoted character"));
                }
                let value = it
                    .as_str()
                    .trim_start()
                    .strip_prefix('=')
                    .ok_or_else(|| Fault::new(line, "charmap entry has no `=`"))?;
                let bytes = hex_bytes(value, line)?;
                if escaped {
                    insert(&mut map.escapes, key, bytes, line)?;
                } else {
                    insert(&mut map.chars, key, bytes, line)?;
                }
            } else {
                let (name, value) = trimmed
                    .split_once('=')
                    .ok_or_else(|| Fault::new(line, "charmap entry has no `=`"))?;
                insert(
                    &mut map.constants,
                    name.trim().to_owned(),
                    hex_bytes(value, line)?,
                    line,
                )?;
            }
        }
        Ok(map)
    }

    /// Map one `_()` or `.string` literal body to charmap bytes (no terminator).
    pub(in crate::extract) fn convert(&self, body: &str, line: usize) -> Result<Vec<u8>, Fault> {
        let c: Vec<char> = body.chars().collect();
        let mut out = Vec::new();
        let mut i = 0;
        while i < c.len() {
            match c[i] {
                '{' => {
                    let close = c[i..]
                        .iter()
                        .position(|&d| d == '}')
                        .ok_or_else(|| Fault::new(line, "unterminated `{` in string"))?;
                    let inner: String = c[i + 1..i + close].iter().collect();
                    self.bracketed(&inner, line, &mut out)?;
                    i += close + 1;
                }
                '\\' => {
                    let next = *c
                        .get(i + 1)
                        .ok_or_else(|| Fault::new(line, "dangling escape in string"))?;
                    let found = if matches!(next, '"' | '\\') {
                        self.chars.get(&next)
                    } else {
                        self.escapes.get(&next)
                    };
                    out.extend(
                        found.ok_or_else(|| {
                            Fault::new(line, format!("unknown escape `\\{next}`"))
                        })?,
                    );
                    i += 2;
                }
                d => {
                    out.extend(
                        self.chars
                            .get(&d)
                            .ok_or_else(|| Fault::new(line, format!("unmapped character {d:?}")))?,
                    );
                    i += 1;
                }
            }
        }
        Ok(out)
    }

    fn bracketed(&self, inner: &str, line: usize, out: &mut Vec<u8>) -> Result<(), Fault> {
        let mut words = 0;
        for word in inner.split_whitespace() {
            words += 1;
            if word.starts_with(|d: char| d.is_ascii_alphabetic() || d == '_') {
                out.extend(self.constants.get(word).ok_or_else(|| {
                    Fault::new(line, format!("unknown charmap constant `{word}`"))
                })?);
            } else {
                out.extend(integer(word, line)?);
            }
        }
        if words == 0 {
            return Err(Fault::new(line, "empty `{}` in string"));
        }
        Ok(())
    }
}

/// An integer operand, sized as `string_parser.cpp` sizes it.
fn integer(word: &str, line: usize) -> Result<Vec<u8>, Fault> {
    let bad = || Fault::new(line, format!("bad integer operand `{word}`"));
    let (value, size) = if let Some(hex) = word.strip_prefix("0x") {
        let size = match hex.len() {
            2 => 1,
            4 => 2,
            8 => 4,
            _ => return Err(bad()),
        };
        (u32::from_str_radix(hex, 16).map_err(|_| bad())?, size)
    } else {
        let (digits, forced) = match word.as_bytes().last() {
            Some(b'H') => (&word[..word.len() - 1], Some(2)),
            Some(b'W') => (&word[..word.len() - 1], Some(4)),
            _ => (word, None),
        };
        let value: u32 = digits.parse().map_err(|_| bad())?;
        let natural = if value >= 0x1_0000 {
            4
        } else if value >= 0x100 {
            2
        } else {
            1
        };
        if forced == Some(2) && value >= 0x1_0000 {
            return Err(bad());
        }
        (value, forced.unwrap_or(natural))
    };
    Ok(value.to_le_bytes()[..size].to_vec())
}
