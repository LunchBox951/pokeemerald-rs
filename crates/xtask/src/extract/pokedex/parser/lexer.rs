//! Comment-aware tokenizer and the token cursor shared by the four grammars.

use super::Result;
use crate::extract::pokedex::error::{PokedexSource, PokedexSourceError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Tok {
    Ident(String),
    /// A run of digits and identifier characters starting with a digit.
    Number(String),
    Str(String),
    /// A character literal, quotes and escapes included.
    Char(String),
    Punct(char),
    /// A preprocessor line, without its leading `#` or trailing comment.
    Directive(String),
}

#[derive(Debug, Clone)]
pub(super) struct Token {
    pub(super) tok: Tok,
    pub(super) line: usize,
}

/// Source text after translation phase 2: every backslash-newline line
/// splice removed, as in the C preprocessor, so comment boundaries and tokens
/// are recognized across them. `lines[i]` is the physical line of `chars[i]`.
pub(super) struct Spliced {
    pub(super) chars: Vec<char>,
    lines: Vec<usize>,
    last_line: usize,
}

impl Spliced {
    pub(super) fn new(text: &str) -> Self {
        let raw: Vec<char> = text.chars().collect();
        let mut chars = Vec::with_capacity(raw.len());
        let mut lines = Vec::with_capacity(raw.len());
        let mut line = 1;
        let mut i = 0;
        while i < raw.len() {
            let splice = match (raw.get(i), raw.get(i + 1), raw.get(i + 2)) {
                (Some('\\'), Some('\n'), _) => 2,
                (Some('\\'), Some('\r'), Some('\n')) => 3,
                _ => 0,
            };
            if splice > 0 {
                // A splice consumes a physical line.
                line += 1;
                i += splice;
                continue;
            }
            chars.push(raw[i]);
            lines.push(line);
            if raw[i] == '\n' {
                line += 1;
            }
            i += 1;
        }
        Self {
            chars,
            lines,
            last_line: line,
        }
    }

    /// The physical line of the character at `i`, or of the end of the text.
    pub(super) fn line(&self, i: usize) -> usize {
        self.lines.get(i).copied().unwrap_or(self.last_line)
    }
}

/// Skips the line comment opening at `start` up to its newline.
fn skip_line_comment(chars: &[char], start: usize) -> usize {
    let mut i = start;
    while i < chars.len() && chars[i] != '\n' {
        i += 1;
    }
    i
}

/// Copies the quoted literal opening at `start` into `out`, honouring
/// backslash escapes; returns the index after it. Stops before a newline or
/// the end of the text, so an unterminated literal ends with its line.
fn copy_quoted(chars: &[char], start: usize, out: &mut String) -> usize {
    let quote = chars[start];
    out.push(quote);
    let mut i = start + 1;
    while i < chars.len() && chars[i] != '\n' {
        let c = chars[i];
        out.push(c);
        i += 1;
        if c == quote {
            break;
        }
        if c == '\\' && i < chars.len() && chars[i] != '\n' {
            out.push(chars[i]);
            i += 1;
        }
    }
    i
}

/// Skips the block comment opening at `start`; returns the index after it.
pub(super) fn skip_block_comment(
    source: PokedexSource,
    text: &Spliced,
    start: usize,
) -> Result<usize> {
    let chars = &text.chars;
    let mut i = start + 2;
    loop {
        match (chars.get(i), chars.get(i + 1)) {
            (Some('*'), Some('/')) => return Ok(i + 2),
            (Some(_), _) => i += 1,
            (None, _) => {
                return Err(PokedexSourceError::UnterminatedComment {
                    source,
                    line: text.line(start),
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

/// The blanks GNU cpp skips between a splicing backslash and its newline:
/// space, tab, vertical tab and form feed.
fn is_horizontal_blank(c: Option<&char>) -> bool {
    matches!(c, Some(' ' | '\t' | '\u{b}' | '\u{c}'))
}

/// Refuses line-ending forms whose preprocessor reading this lexer does not
/// model: a bare carriage return, which GNU cpp does not read as a line end,
/// and a backslash separated from its newline by blanks, which GNU cpp still
/// splices. Either could hide a token from the lexer, so both fail closed.
fn refuse_unmodelled_line_endings(source: PokedexSource, text: &str) -> Result<()> {
    let chars: Vec<char> = text.chars().collect();
    let mut line = 1;
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '\n' => line += 1,
            '\r' if chars.get(i + 1) != Some(&'\n') => {
                return Err(PokedexSourceError::Malformed {
                    source,
                    line,
                    expected: "no bare carriage return outside a `\\r\\n` line ending",
                });
            }
            '\\' if is_horizontal_blank(chars.get(i + 1)) => {
                let mut j = i + 1;
                while is_horizontal_blank(chars.get(j)) {
                    j += 1;
                }
                if matches!(chars.get(j), Some('\n' | '\r')) {
                    return Err(PokedexSourceError::Malformed {
                        source,
                        line,
                        expected: "no blanks between a line-splicing backslash and its newline",
                    });
                }
            }
            _ => {}
        }
        i += 1;
    }
    Ok(())
}

pub(super) fn lex(source: PokedexSource, text: &str) -> Result<Vec<Token>> {
    refuse_unmodelled_line_endings(source, text)?;
    let text = Spliced::new(text);
    let chars = &text.chars;
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let line = text.line(i);
        match c {
            c if c.is_whitespace() => i += 1,
            '/' if chars.get(i + 1) == Some(&'/') => {
                i = skip_line_comment(chars, i);
            }
            '/' if chars.get(i + 1) == Some(&'*') => {
                i = skip_block_comment(source, &text, i)?;
            }
            // `%:` is the digraph spelling of `#`.
            '#' | '%' if c == '#' || chars.get(i + 1) == Some(&':') => {
                let mut body = String::new();
                i += if c == '#' { 1 } else { 2 };
                while i < chars.len() && chars[i] != '\n' {
                    if chars[i] == '"' || chars[i] == '\'' {
                        // Comment delimiters inside a literal are its text.
                        i = copy_quoted(chars, i, &mut body);
                    } else if chars[i] == '/' && chars.get(i + 1) == Some(&'/') {
                        i = skip_line_comment(chars, i);
                    } else if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
                        // A comment is one space, as in the C preprocessor.
                        i = skip_block_comment(source, &text, i)?;
                        body.push(' ');
                    } else {
                        body.push(chars[i]);
                        i += 1;
                    }
                }
                // The directive's line is where it ends, past any splices.
                tokens.push(Token {
                    tok: Tok::Directive(body.trim().to_owned()),
                    line: text.line(i),
                });
            }
            '"' => {
                let (value, next) = lex_string(source, chars, i, line)?;
                i = next;
                tokens.push(Token {
                    tok: Tok::Str(value),
                    line,
                });
            }
            '\'' => {
                // Copied whole, so comment delimiters inside are its text.
                let mut literal = String::new();
                i = copy_quoted(chars, i, &mut literal);
                tokens.push(Token {
                    tok: Tok::Char(literal),
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
