//! Lexing, per-file grammar, and closure validation for the Pokédex tables.

use std::collections::{HashMap, HashSet};

use assets::pokedex::{
    NationalDexNo, PokedexData, PokedexEntry, PokedexOrderKind, PokedexOrders, NATIONAL_DEX_COUNT,
    NATIONAL_DEX_LAST,
};

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

/// The observed member counts of the three orders.
const ORDER_LENGTHS: [(PokedexOrderKind, &str, usize); 3] = [
    (
        PokedexOrderKind::Alphabetical,
        "gPokedexOrder_Alphabetical",
        411,
    ),
    (PokedexOrderKind::Weight, "gPokedexOrder_Weight", 386),
    (PokedexOrderKind::Height, "gPokedexOrder_Height", 386),
];

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

// ---- lexer ----------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    Ident(String),
    /// A run of digits and identifier characters starting with a digit.
    Number(String),
    Str(String),
    Punct(char),
    /// A preprocessor line, without its leading `#` or trailing comment.
    Directive(String),
}

#[derive(Debug, Clone)]
struct Token {
    tok: Tok,
    line: usize,
}

/// Skips the block comment opening at `start`; returns the index after it.
fn skip_block_comment(
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
fn lex_string(
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

fn lex(source: PokedexSource, text: &str) -> Result<Vec<Token>> {
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
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '/' if chars.get(i + 1) == Some(&'*') => {
                i = skip_block_comment(source, &chars, i, &mut line)?;
            }
            '#' => {
                let mut body = String::new();
                i += 1;
                while i < chars.len() && chars[i] != '\n' {
                    if chars[i] == '/' && chars.get(i + 1) == Some(&'/') {
                        while i < chars.len() && chars[i] != '\n' {
                            i += 1;
                        }
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
struct Cursor {
    source: PokedexSource,
    tokens: Vec<Token>,
    pos: usize,
    last_line: usize,
}

impl Cursor {
    fn new(source: PokedexSource, text: &str) -> Result<Self> {
        let tokens = lex(source, text)?;
        let last_line = tokens.last().map_or(1, |token| token.line);
        Ok(Self {
            source,
            tokens,
            pos: 0,
            last_line,
        })
    }

    fn at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    fn line(&self) -> usize {
        self.tokens.get(self.pos).map_or(self.last_line, |t| t.line)
    }

    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.pos).map(|token| &token.tok)
    }

    fn fail(&self, expected: &'static str) -> PokedexSourceError {
        PokedexSourceError::Malformed {
            source: self.source,
            line: self.line(),
            expected,
        }
    }

    fn punct(&mut self, want: char, expected: &'static str) -> Result<()> {
        if self.peek() == Some(&Tok::Punct(want)) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.fail(expected))
        }
    }

    fn eat_punct(&mut self, want: char) -> bool {
        let hit = self.peek() == Some(&Tok::Punct(want));
        if hit {
            self.pos += 1;
        }
        hit
    }

    fn keyword(&mut self, word: &str, expected: &'static str) -> Result<()> {
        match self.peek() {
            Some(Tok::Ident(found)) if found == word => {
                self.pos += 1;
                Ok(())
            }
            _ => Err(self.fail(expected)),
        }
    }

    fn ident(&mut self, expected: &'static str) -> Result<String> {
        if let Some(Tok::Ident(name)) = self.peek() {
            let name = name.clone();
            self.pos += 1;
            Ok(name)
        } else {
            Err(self.fail(expected))
        }
    }

    /// One `_("a" "b")` text, concatenated and unescaped.
    fn text_macro(&mut self) -> Result<String> {
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
    fn number(&mut self, field: &str, min: i32, max: i32) -> Result<i32> {
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

// ---- constants ------------------------------------------------------------

struct Constants {
    values: HashMap<String, u16>,
}

impl Constants {
    fn resolve(&self, source: PokedexSource, line: usize, symbol: &str) -> Result<NationalDexNo> {
        let unresolved = || PokedexSourceError::UnresolvedConstant {
            source,
            line,
            symbol: symbol.to_owned(),
        };
        let value = *self.values.get(symbol).ok_or_else(unresolved)?;
        Ok(NationalDexNo::new(value)?)
    }
}

fn parse_constants(text: &str) -> Result<Constants> {
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

// ---- text -----------------------------------------------------------------

fn parse_texts(text: &str) -> Result<HashMap<String, String>> {
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

// ---- entries --------------------------------------------------------------

struct EntryRow {
    symbol: String,
    line: usize,
    category: String,
    description: String,
    height: u16,
    weight: u16,
    pokemon_scale: u16,
    pokemon_offset: i16,
    trainer_scale: u16,
    trainer_offset: i16,
}

fn parse_entries(text: &str) -> Result<Vec<EntryRow>> {
    let mut cur = Cursor::new(PokedexSource::Entries, text)?;
    cur.keyword("const", "`const struct PokedexEntry gPokedexEntries[] =`")?;
    cur.keyword("struct", "`struct` in the entries declaration")?;
    cur.keyword("PokedexEntry", "`PokedexEntry` in the entries declaration")?;
    cur.keyword("gPokedexEntries", "`gPokedexEntries`")?;
    cur.punct('[', "`[` after gPokedexEntries")?;
    cur.punct(']', "`]` after gPokedexEntries")?;
    cur.punct('=', "`=` after gPokedexEntries")?;
    cur.punct('{', "`{` opening gPokedexEntries")?;
    let mut rows = Vec::new();
    while !cur.eat_punct('}') {
        rows.push(parse_entry_row(&mut cur)?);
    }
    cur.punct(';', "`;` closing gPokedexEntries")?;
    if !cur.at_end() {
        return Err(cur.fail("end of file after gPokedexEntries"));
    }
    Ok(rows)
}

#[derive(Default)]
struct Members {
    category: Option<String>,
    description: Option<String>,
    height: Option<u16>,
    weight: Option<u16>,
    pokemon_scale: Option<u16>,
    pokemon_offset: Option<i16>,
    trainer_scale: Option<u16>,
    trainer_offset: Option<i16>,
}

fn parse_entry_row(cur: &mut Cursor) -> Result<EntryRow> {
    let line = cur.line();
    cur.punct('[', "`[` opening a designated entry key")?;
    let symbol = cur.ident("an entry key symbol")?;
    cur.punct(']', "`]` closing the entry key")?;
    cur.punct('=', "`=` after the entry key")?;
    cur.punct('{', "`{` opening the entry")?;
    let mut members = Members::default();
    while !cur.eat_punct('}') {
        let member_line = cur.line();
        cur.punct('.', "`.member` designator")?;
        let member = cur.ident("an entry member name")?;
        cur.punct('=', "`=` after the member")?;
        let duplicate = || PokedexSourceError::DuplicateEntryMember {
            symbol: symbol.clone(),
            member: member.clone(),
        };
        macro_rules! set {
            ($slot:expr, $value:expr) => {{
                if $slot.is_some() {
                    return Err(duplicate());
                }
                $slot = Some($value);
            }};
        }
        match member.as_str() {
            "categoryName" => set!(members.category, cur.text_macro()?),
            "description" => set!(members.description, cur.ident("a description text symbol")?),
            "height" => set!(members.height, num_u16(cur, "height")?),
            "weight" => set!(members.weight, num_u16(cur, "weight")?),
            "pokemonScale" => set!(members.pokemon_scale, num_u16(cur, "pokemonScale")?),
            "trainerScale" => set!(members.trainer_scale, num_u16(cur, "trainerScale")?),
            "pokemonOffset" => set!(members.pokemon_offset, num_i16(cur, "pokemonOffset")?),
            "trainerOffset" => set!(members.trainer_offset, num_i16(cur, "trainerOffset")?),
            _ => {
                return Err(PokedexSourceError::UnknownEntryMember {
                    source: cur.source,
                    line: member_line,
                    member,
                });
            }
        }
        cur.punct(',', "`,` after the member")?;
    }
    cur.punct(',', "`,` after the entry")?;

    let missing = |name: &'static str| PokedexSourceError::MissingEntryMember {
        symbol: symbol.clone(),
        member: name,
    };
    Ok(EntryRow {
        category: members.category.ok_or_else(|| missing("categoryName"))?,
        description: members.description.ok_or_else(|| missing("description"))?,
        height: members.height.ok_or_else(|| missing("height"))?,
        weight: members.weight.ok_or_else(|| missing("weight"))?,
        pokemon_scale: members
            .pokemon_scale
            .ok_or_else(|| missing("pokemonScale"))?,
        pokemon_offset: members
            .pokemon_offset
            .ok_or_else(|| missing("pokemonOffset"))?,
        trainer_scale: members
            .trainer_scale
            .ok_or_else(|| missing("trainerScale"))?,
        trainer_offset: members
            .trainer_offset
            .ok_or_else(|| missing("trainerOffset"))?,
        symbol,
        line,
    })
}

fn num_u16(cur: &mut Cursor, field: &str) -> Result<u16> {
    let value = cur.number(field, 0, i32::from(u16::MAX))?;
    Ok(u16::try_from(value).unwrap_or(u16::MAX))
}

fn num_i16(cur: &mut Cursor, field: &str) -> Result<i16> {
    let value = cur.number(field, i32::from(i16::MIN), i32::from(i16::MAX))?;
    Ok(i16::try_from(value).unwrap_or(0))
}

// ---- orders ---------------------------------------------------------------

struct RawOrders {
    alphabetical: Vec<NationalDexNo>,
    weight: Vec<NationalDexNo>,
    height: Vec<NationalDexNo>,
}

fn parse_orders(text: &str, constants: &Constants) -> Result<RawOrders> {
    let mut cur = Cursor::new(PokedexSource::Orders, text)?;
    let mut found: HashMap<usize, Vec<NationalDexNo>> = HashMap::new();
    while !cur.at_end() {
        cur.keyword("const", "`const u16` order definition")?;
        cur.keyword("u16", "`u16` in an order definition")?;
        let line = cur.line();
        let symbol = cur.ident("an order symbol")?;
        cur.punct('[', "`[` after the order symbol")?;
        cur.punct(']', "`]` after the order symbol")?;
        cur.punct('=', "`=` after the order symbol")?;
        cur.punct('{', "`{` opening the order")?;
        let Some(slot) = ORDER_LENGTHS
            .iter()
            .position(|(_, name, _)| *name == symbol)
        else {
            return Err(PokedexSourceError::UnknownOrder { line, symbol });
        };
        let mut members = Vec::new();
        while !cur.eat_punct('}') {
            let line = cur.line();
            let member = cur.ident("a National Dex constant")?;
            members.push(constants.resolve(PokedexSource::Orders, line, &member)?);
            if !cur.eat_punct(',') {
                cur.punct('}', "`,` or `}` after an order member")?;
                break;
            }
        }
        cur.punct(';', "`;` closing the order")?;
        if found.insert(slot, members).is_some() {
            return Err(PokedexSourceError::DuplicateOrder {
                order: ORDER_LENGTHS[slot].0,
            });
        }
    }
    let mut take = |slot: usize| -> Result<Vec<NationalDexNo>> {
        let (order, _, expected) = ORDER_LENGTHS[slot];
        let members = found
            .remove(&slot)
            .ok_or(PokedexSourceError::MissingOrder { order })?;
        if members.len() != expected {
            return Err(PokedexSourceError::OrderLength {
                order,
                expected,
                actual: members.len(),
            });
        }
        Ok(members)
    };
    Ok(RawOrders {
        alphabetical: take(0)?,
        weight: take(1)?,
        height: take(2)?,
    })
}

#[cfg(test)]
mod tests;
