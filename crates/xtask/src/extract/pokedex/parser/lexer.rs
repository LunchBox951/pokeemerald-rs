//! Comment-aware tokenizer and the token cursor shared by the four grammars.

use super::Result;
use crate::extract::pokedex::error::{PokedexSource, PokedexSourceError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Tok {
    Ident(String),
    /// A run of digits and identifier characters starting with a digit.
    Number(String),
    Str(String),
    Punct(char),
    /// A preprocessor line, without its leading `#` or trailing comment.
    Directive(String),
}

#[derive(Debug, Clone)]
pub(super) struct Token {
    pub(super) tok: Tok,
    pub(super) line: usize,
}

/// The length of the backslash-newline line splice at `i`, or 0 when none.
fn splice_len(chars: &[char], i: usize) -> usize {
    match (chars.get(i), chars.get(i + 1), chars.get(i + 2)) {
        (Some('\\'), Some('\n'), _) => 2,
        (Some('\\'), Some('\r'), Some('\n')) => 3,
        _ => 0,
    }
}

/// Skips the line comment opening at `start` up to its newline; a line splice
/// continues it onto the next line, as in the C preprocessor.
fn skip_line_comment(chars: &[char], start: usize, line: &mut usize) -> usize {
    let mut i = start;
    while i < chars.len() && chars[i] != '\n' {
        let splice = splice_len(chars, i);
        if splice > 0 {
            *line += 1;
            i += splice;
        } else {
            i += 1;
        }
    }
    i
}

/// Skips the block comment opening at `start`; returns the index after it.
pub(super) fn skip_block_comment(
    source: PokedexSource,
    chars: &[char],
    start: usize,
    line: &mut usize,
) -> Result<usize> {
    let opened = *line;
    let mut i = start + 2;
    loop {
        match (chars.get(i), chars.get(i + 1)) {
            (Some('*'), Some('/')) => return Ok(i + 2),
            (Some(c), _) => {
                if *c == '\n' {
                    *line += 1;
                }
                i += 1;
            }
            (None, _) => {
                return Err(PokedexSourceError::UnterminatedComment {
                    source,
                    line: opened,
                });
            }
        }
    }
}

/// Reads the string literal opening at `start`; returns its value and the
/// index after the closing quote.
pub(super) fn lex_string(
    source: PokedexSource,
    chars: &[char],
    start: usize,
    line: usize,
) -> Result<(String, usize)> {
    let unterminated = PokedexSourceError::UnterminatedString { source, line };
    let mut value = String::new();
    let mut i = start + 1;
    loop {
        match chars.get(i) {
            None | Some('\n') => return Err(unterminated),
            Some('"') => return Ok((value, i + 1)),
            Some('\\') => {
                match chars.get(i + 1).copied() {
                    Some('n') => value.push('\n'),
                    Some('\\') => value.push('\\'),
                    Some('"') => value.push('"'),
                    Some(escape) if escape != '\n' => {
                        return Err(PokedexSourceError::UnsupportedEscape {
                            source,
                            line,
                            escape,
                        });
                    }
                    _ => return Err(unterminated),
                }
                i += 2;
            }
            Some(other) => {
                value.push(*other);
                i += 1;
            }
        }
    }
}

pub(super) fn lex(source: PokedexSource, text: &str) -> Result<Vec<Token>> {
    let chars: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    let mut line = 1;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\n' => {
                line += 1;
                i += 1;
            }
            c if c.is_whitespace() => i += 1,
            '/' if chars.get(i + 1) == Some(&'/') => {
                i = skip_line_comment(&chars, i, &mut line);
            }
            '/' if chars.get(i + 1) == Some(&'*') => {
                i = skip_block_comment(source, &chars, i, &mut line)?;
            }
            '#' => {
                let mut body = String::new();
                i += 1;
                while i < chars.len() && chars[i] != '\n' {
                    let splice = splice_len(&chars, i);
                    if splice > 0 {
                        // A line splice continues the directive, as in the C preprocessor.
                        line += 1;
                        i += splice;
                    } else if chars[i] == '/' && chars.get(i + 1) == Some(&'/') {
                        i = skip_line_comment(&chars, i, &mut line);
                    } else if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
                        // A comment is one space, as in the C preprocessor.
                        i = skip_block_comment(source, &chars, i, &mut line)?;
                        body.push(' ');
                    } else {
                        body.push(chars[i]);
                        i += 1;
                    }
                }
                tokens.push(Token {
                    tok: Tok::Directive(body.trim().to_owned()),
                    line,
                });
            }
            '"' => {
                let (value, next) = lex_string(source, &chars, i, line)?;
                i = next;
                tokens.push(Token {
                    tok: Tok::Str(value),
                    line,
                });
            }
            c if c.is_ascii_digit() => {
                let mut word = String::new();
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    word.push(chars[i]);
                    i += 1;
                }
                tokens.push(Token {
                    tok: Tok::Number(word),
                    line,
                });
            }
            c if c.is_alphabetic() || c == '_' => {
                let mut word = String::new();
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    word.push(chars[i]);
                    i += 1;
                }
                tokens.push(Token {
                    tok: Tok::Ident(word),
                    line,
                });
            }
            other => {
                tokens.push(Token {
                    tok: Tok::Punct(other),
                    line,
                });
                i += 1;
            }
        }
    }
    Ok(tokens)
}

/// A cursor over one file's tokens.
pub(super) struct Cursor {
    pub(super) source: PokedexSource,
    pub(super) tokens: Vec<Token>,
    pub(super) pos: usize,
    pub(super) last_line: usize,
}

impl Cursor {
    pub(super) fn new(source: PokedexSource, text: &str) -> Result<Self> {
        let tokens = lex(source, text)?;
        let last_line = tokens.last().map_or(1, |token| token.line);
        Ok(Self {
            source,
            tokens,
            pos: 0,
            last_line,
        })
    }

    pub(super) fn at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    pub(super) fn line(&self) -> usize {
        self.tokens.get(self.pos).map_or(self.last_line, |t| t.line)
    }

    pub(super) fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.pos).map(|token| &token.tok)
    }

    pub(super) fn fail(&self, expected: &'static str) -> PokedexSourceError {
        PokedexSourceError::Malformed {
            source: self.source,
            line: self.line(),
            expected,
        }
    }

    pub(super) fn punct(&mut self, want: char, expected: &'static str) -> Result<()> {
        if self.peek() == Some(&Tok::Punct(want)) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.fail(expected))
        }
    }

    pub(super) fn eat_punct(&mut self, want: char) -> bool {
        let hit = self.peek() == Some(&Tok::Punct(want));
        if hit {
            self.pos += 1;
        }
        hit
    }

    pub(super) fn keyword(&mut self, word: &str, expected: &'static str) -> Result<()> {
        match self.peek() {
            Some(Tok::Ident(found)) if found == word => {
                self.pos += 1;
                Ok(())
            }
            _ => Err(self.fail(expected)),
        }
    }

    pub(super) fn ident(&mut self, expected: &'static str) -> Result<String> {
        if let Some(Tok::Ident(name)) = self.peek() {
            let name = name.clone();
            self.pos += 1;
            Ok(name)
        } else {
            Err(self.fail(expected))
        }
    }

    /// One `_("a" "b")` text, concatenated and unescaped.
    pub(super) fn text_macro(&mut self) -> Result<String> {
        self.keyword("_", "`_(` text macro")?;
        self.punct('(', "`(` after `_`")?;
        let mut value = String::new();
        let mut literals = 0;
        while let Some(Tok::Str(part)) = self.peek() {
            value.push_str(part);
            literals += 1;
            self.pos += 1;
        }
        if literals == 0 {
            return Err(self.fail("a string literal"));
        }
        self.punct(')', "`)` closing the text macro")?;
        Ok(value)
    }

    /// An optionally negated decimal literal in `min..=max`.
    pub(super) fn number(&mut self, field: &str, min: i32, max: i32) -> Result<i32> {
        let line = self.line();
        let negative = self.eat_punct('-');
        let Some(Tok::Number(text)) = self.peek() else {
            return Err(self.fail("a decimal number"));
        };
        let text = text.clone();
        self.pos += 1;
        let shown = if negative {
            format!("-{text}")
        } else {
            text.clone()
        };
        if text.is_empty()
            || !text.bytes().all(|b| b.is_ascii_digit())
            || (text.len() > 1 && text.starts_with('0'))
        {
            return Err(PokedexSourceError::MalformedNumber {
                source: self.source,
                line,
                text: shown,
            });
        }
        let out_of_range = || PokedexSourceError::NumberOutOfRange {
            source: self.source,
            line,
            field: field.to_owned(),
            text: shown.clone(),
        };
        let magnitude: i64 = text.parse().map_err(|_| out_of_range())?;
        let value = if negative { -magnitude } else { magnitude };
        i32::try_from(value)
            .ok()
            .filter(|value| (min..=max).contains(value))
            .ok_or_else(out_of_range)
    }
}
