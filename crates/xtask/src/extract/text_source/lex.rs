//! A tiny C tokenizer: identifiers, string-literal bodies, and punctuation.
//!
//! Comments, preprocessor lines, and character literals are dropped. String
//! bodies keep their escapes for the charmap converter. No source text is ever
//! handed to a compiler or a shell.

use super::Fault;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::extract) enum Tok {
    Ident(String),
    Str(String),
    Punct(char),
}

#[derive(Debug, Clone)]
pub(in crate::extract) struct Token {
    pub(in crate::extract) tok: Tok,
    pub(in crate::extract) line: usize,
}

pub(in crate::extract) fn tokenize(src: &str) -> Result<Vec<Token>, Fault> {
    let c: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let (mut i, mut line, mut line_start) = (0, 1, true);
    while i < c.len() {
        let ch = c[i];
        match ch {
            '\n' => {
                line += 1;
                line_start = true;
                i += 1;
            }
            _ if ch.is_whitespace() => i += 1,
            '#' if line_start => {
                // Preprocessor line, with backslash continuations.
                while i < c.len() && c[i] != '\n' {
                    if c[i] == '\\' && c.get(i + 1) == Some(&'\n') {
                        line += 1;
                        i += 1;
                    }
                    i += 1;
                }
            }
            '/' if c.get(i + 1) == Some(&'/') => {
                while i < c.len() && c[i] != '\n' {
                    i += 1;
                }
            }
            '/' if c.get(i + 1) == Some(&'*') => {
                let start = line;
                i += 2;
                loop {
                    match (c.get(i), c.get(i + 1)) {
                        (Some('*'), Some('/')) => {
                            i += 2;
                            break;
                        }
                        (Some(&d), _) => {
                            if d == '\n' {
                                line += 1;
                            }
                            i += 1;
                        }
                        (None, _) => return Err(Fault::new(start, "unterminated block comment")),
                    }
                }
            }
            '"' => {
                line_start = false;
                let start = line;
                let (body, next) = string_body(&c, i + 1, start)?;
                i = next;
                out.push(Token {
                    tok: Tok::Str(body),
                    line: start,
                });
            }
            '\'' => {
                line_start = false;
                i += 1;
                while i < c.len() && c[i] != '\'' {
                    if c[i] == '\n' {
                        return Err(Fault::new(line, "unterminated character literal"));
                    }
                    i += if c[i] == '\\' { 2 } else { 1 };
                }
                if i >= c.len() {
                    return Err(Fault::new(line, "unterminated character literal"));
                }
                i += 1;
            }
            _ if ch.is_ascii_alphabetic() || ch == '_' => {
                line_start = false;
                let start = i;
                while i < c.len() && (c[i].is_ascii_alphanumeric() || c[i] == '_') {
                    i += 1;
                }
                out.push(Token {
                    tok: Tok::Ident(c[start..i].iter().collect()),
                    line,
                });
            }
            _ => {
                line_start = false;
                out.push(Token {
                    tok: Tok::Punct(ch),
                    line,
                });
                i += 1;
            }
        }
    }
    Ok(out)
}

/// Read a string-literal body from just past the opening quote; returns the raw
/// body (escapes kept) and the index past the closing quote.
fn string_body(c: &[char], mut i: usize, line: usize) -> Result<(String, usize), Fault> {
    let mut body = String::new();
    loop {
        match c.get(i) {
            None | Some('\n') => return Err(Fault::new(line, "unterminated string literal")),
            Some('"') => return Ok((body, i + 1)),
            Some('\\') => {
                body.push('\\');
                match c.get(i + 1) {
                    Some(&next) if next != '\n' => body.push(next),
                    _ => return Err(Fault::new(line, "unterminated string literal")),
                }
                i += 2;
            }
            Some(&d) => {
                body.push(d);
                i += 1;
            }
        }
    }
}
