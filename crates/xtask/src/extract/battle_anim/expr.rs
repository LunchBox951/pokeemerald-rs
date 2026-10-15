//! Upstream constants and the integer expressions the script operands use.
//!
//! Names come from the C headers the script `#include`s (`#define` values,
//! function-like defines, and enumerators) and from the assembler constant
//! files (`.set`). Resolution is lazy, so a header may reference a name that
//! only a later file defines. Operands are evaluated with the GNU assembler's
//! operator precedence, headers with C's.

use std::collections::HashMap;

use super::Fault;

/// How many nested definitions one expression may expand before it is
/// treated as a cycle.
const MAX_DEPTH: usize = 32;

/// How many unary operators and parentheses one expression may nest.
const MAX_NESTING: usize = 64;

/// Which operator precedence an expression follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Style {
    /// The C preprocessor and compiler: `* / %`, `+ -`, shifts, `&`, `^`, `|`.
    C,
    /// GNU `as`: `* / % << >>` bind tighter than `| & ^`, which bind tighter
    /// than `+ -`.
    Gas,
}

impl Style {
    /// The binding strength of binary `op`; higher binds tighter.
    fn level(self, op: &str) -> Option<u8> {
        let levels: &[(&[&str], u8)] = match self {
            Self::C => &[
                (&["*", "/", "%"], 6),
                (&["+", "-"], 5),
                (&["<<", ">>"], 4),
                (&["&"], 3),
                (&["^"], 2),
                (&["|"], 1),
            ],
            Self::Gas => &[
                (&["*", "/", "%", "<<", ">>"], 3),
                (&["|", "&", "^"], 2),
                (&["+", "-"], 1),
            ],
        };
        levels
            .iter()
            .find_map(|(ops, level)| ops.contains(&op).then_some(*level))
    }
}

#[derive(Debug, Clone)]
enum Definition {
    Value { style: Style, text: String },
    Function { params: Vec<String>, body: String },
}

/// Every named constant the script and its macros can reference.
#[derive(Debug, Default)]
pub(super) struct Constants {
    definitions: HashMap<String, Definition>,
}

impl Constants {
    /// Whether `name` is defined (the `.ifdef` test).
    pub(super) fn defines(&self, name: &str) -> bool {
        self.definitions.contains_key(name)
    }

    /// Adds the `#define` and `enum` definitions of one C header.
    pub(super) fn load_header(&mut self, file: &str, text: &str) -> Result<(), Fault> {
        let mut in_comment = false;
        let mut enum_state = EnumState::Outside;
        for (line, code) in logical_lines(text, &mut in_comment) {
            let fault = |detail: String| Fault::new(file, line, detail);
            let code = code.trim();
            if let Some(rest) = code.strip_prefix("#define") {
                self.define(rest).map_err(fault)?;
            } else if !code.starts_with('#') {
                enum_state = self.scan_enum(enum_state, code).map_err(fault)?;
            }
        }
        if !matches!(enum_state, EnumState::Outside) {
            return Err(Fault::new(file, 0, "unterminated enum".to_owned()));
        }
        Ok(())
    }

    /// Adds the `.set` definitions of one assembler constants file.
    pub(super) fn load_assembler(&mut self, file: &str, text: &str) -> Result<(), Fault> {
        for (index, raw) in text.lines().enumerate() {
            let code = raw.split('@').next().unwrap_or_default().trim();
            let Some(rest) = code.strip_prefix(".set") else {
                continue;
            };
            let (name, value) = rest
                .split_once(',')
                .ok_or_else(|| Fault::new(file, index + 1, "`.set` lacks a value".to_owned()))?;
            self.insert_value(name.trim(), value.trim(), Style::Gas)
                .map_err(|detail| Fault::new(file, index + 1, detail))?;
        }
        Ok(())
    }

    /// Evaluates `text` as one integer expression.
    pub(super) fn eval(&self, text: &str, style: Style) -> Result<i64, String> {
        self.eval_scoped(text, style, &HashMap::new(), 0)
    }

    fn define(&mut self, rest: &str) -> Result<(), String> {
        let rest = rest.trim_start();
        let name_end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        let (name, tail) = rest.split_at(name_end);
        if name.is_empty() {
            return Err("`#define` lacks a name".to_owned());
        }
        if let Some(params) = tail.strip_prefix('(') {
            let (params, body) = params
                .split_once(')')
                .ok_or_else(|| format!("`{name}` has an unclosed parameter list"))?;
            let params = params.split(',').map(|p| p.trim().to_owned()).collect();
            let body = body.trim().to_owned();
            self.definitions
                .insert(name.to_owned(), Definition::Function { params, body });
            return Ok(());
        }
        let value = tail.trim();
        if !value.is_empty() {
            self.insert_value(name, value, Style::C)?;
        }
        Ok(())
    }

    fn insert_value(&mut self, name: &str, text: &str, style: Style) -> Result<(), String> {
        if !is_identifier(name) {
            return Err(format!("`{name}` is not an identifier"));
        }
        self.definitions.insert(
            name.to_owned(),
            Definition::Value {
                style,
                text: text.to_owned(),
            },
        );
        Ok(())
    }

    fn scan_enum(&mut self, state: EnumState, code: &str) -> Result<EnumState, String> {
        let (mut state, mut rest) = (state, code);
        if matches!(state, EnumState::Outside) {
            if !(rest == "enum" || rest.starts_with("enum ") || rest.starts_with("enum{")) {
                return Ok(state);
            }
            state = EnumState::Header;
        }
        if matches!(state, EnumState::Header) {
            if rest.ends_with(';') && !rest.contains('{') {
                return Ok(EnumState::Outside);
            }
            let Some((_, body)) = rest.split_once('{') else {
                return Ok(state);
            };
            state = EnumState::Body { previous: None };
            rest = body;
        }
        let (body, closed) = rest
            .split_once('}')
            .map_or((rest, false), |(b, _)| (b, true));
        let EnumState::Body { mut previous } = state else {
            return Ok(state);
        };
        for entry in body.split(',').map(str::trim).filter(|e| !e.is_empty()) {
            let (name, text) = match entry.split_once('=') {
                Some((name, value)) => (name.trim(), value.trim().to_owned()),
                None => (
                    entry,
                    previous
                        .as_ref()
                        .map_or_else(|| "0".to_owned(), |p| format!("{p} + 1")),
                ),
            };
            self.insert_value(name, &text, Style::C)?;
            previous = Some(name.to_owned());
        }
        Ok(if closed {
            EnumState::Outside
        } else {
            EnumState::Body { previous }
        })
    }

    fn eval_scoped(
        &self,
        text: &str,
        style: Style,
        scope: &HashMap<String, i64>,
        depth: usize,
    ) -> Result<i64, String> {
        if depth > MAX_DEPTH {
            return Err(format!("`{text}` expands too deeply (cyclic definition?)"));
        }
        let tokens = tokenize(text)?;
        let mut parser = Parser {
            constants: self,
            tokens: &tokens,
            position: 0,
            style,
            scope,
            depth,
            nesting: 0,
        };
        let value = parser.expression(0)?;
        if parser.position != tokens.len() {
            return Err(format!("`{text}` has trailing tokens"));
        }
        Ok(value)
    }
}

#[derive(Debug, Clone)]
enum EnumState {
    Outside,
    Header,
    Body { previous: Option<String> },
}

/// Joins `\` continuations and drops comments, yielding each logical line with
/// the number of its first physical line.
fn logical_lines<'a>(
    text: &'a str,
    in_comment: &'a mut bool,
) -> impl Iterator<Item = (usize, String)> + 'a {
    let mut pending: Option<(usize, String)> = None;
    let mut lines = text.lines().enumerate();
    std::iter::from_fn(move || {
        for (index, raw) in lines.by_ref() {
            let stripped = strip_comments(raw, in_comment);
            let (first, mut line) = pending.take().unwrap_or((index + 1, String::new()));
            if let Some(joined) = stripped.trim_end().strip_suffix('\\') {
                line.push_str(joined);
                line.push(' ');
                pending = Some((first, line));
                continue;
            }
            line.push_str(&stripped);
            return Some((first, line));
        }
        pending.take()
    })
}

fn strip_comments(raw: &str, in_comment: &mut bool) -> String {
    let mut out = String::new();
    let mut rest = raw;
    loop {
        if *in_comment {
            let Some((_, after)) = rest.split_once("*/") else {
                return out;
            };
            *in_comment = false;
            rest = after;
            continue;
        }
        let line_comment = rest.find("//");
        let block = rest.find("/*");
        match (line_comment, block) {
            (Some(l), b) if b.is_none_or(|b| l < b) => {
                out.push_str(&rest[..l]);
                return out;
            }
            (_, Some(b)) => {
                out.push_str(&rest[..b]);
                *in_comment = true;
                rest = &rest[b + 2..];
            }
            _ => {
                out.push_str(rest);
                return out;
            }
        }
    }
}

pub(super) fn is_identifier(text: &str) -> bool {
    let mut bytes = text.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Number(i64),
    Name(String),
    Op(&'static str),
}

const OPERATORS: [&str; 14] = [
    "<<", ">>", "+", "-", "*", "/", "%", "|", "&", "^", "~", "!", "(", ")",
];

fn tokenize(text: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut rest = text.trim_start();
    while let Some(first) = rest.chars().next() {
        if first.is_ascii_digit() {
            let end = rest
                .find(|c: char| !c.is_ascii_alphanumeric())
                .unwrap_or(rest.len());
            tokens.push(Token::Number(parse_number(&rest[..end])?));
            rest = &rest[end..];
        } else if first.is_ascii_alphabetic() || first == '_' {
            let end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(rest.len());
            tokens.push(Token::Name(rest[..end].to_owned()));
            rest = &rest[end..];
        } else if first == ',' {
            tokens.push(Token::Op(","));
            rest = &rest[1..];
        } else if let Some(op) = OPERATORS.iter().find(|op| rest.starts_with(**op)) {
            tokens.push(Token::Op(op));
            rest = &rest[op.len()..];
        } else {
            return Err(format!("unexpected `{first}` in `{text}`"));
        }
        rest = rest.trim_start();
    }
    Ok(tokens)
}

fn parse_number(word: &str) -> Result<i64, String> {
    let digits = word.trim_end_matches(['u', 'U', 'l', 'L']);
    let parsed = match digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        Some(hex) => i64::from_str_radix(hex, 16),
        None => digits.parse(),
    };
    parsed.map_err(|_| format!("`{word}` is not an integer"))
}

struct Parser<'a> {
    constants: &'a Constants,
    tokens: &'a [Token],
    position: usize,
    style: Style,
    scope: &'a HashMap<String, i64>,
    depth: usize,
    nesting: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.position)
    }

    fn expect(&mut self, op: &'static str) -> Result<(), String> {
        if self.peek() == Some(&Token::Op(op)) {
            self.position += 1;
            Ok(())
        } else {
            Err(format!("expected `{op}`"))
        }
    }

    fn expression(&mut self, minimum: u8) -> Result<i64, String> {
        let mut left = self.unary()?;
        while let Some(&Token::Op(op)) = self.peek() {
            let Some(level) = self.style.level(op) else {
                break;
            };
            if level < minimum {
                break;
            }
            self.position += 1;
            let right = self.expression(level + 1)?;
            left = apply(op, left, right)?;
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<i64, String> {
        if self.nesting >= MAX_NESTING {
            return Err("expression nests too deeply".to_owned());
        }
        self.nesting += 1;
        let value = self.unary_operand();
        self.nesting -= 1;
        value
    }

    fn unary_operand(&mut self) -> Result<i64, String> {
        let token = self.peek().cloned().ok_or("expression ends early")?;
        self.position += 1;
        match token {
            Token::Number(value) => Ok(value),
            Token::Op("-") => self
                .unary()?
                .checked_neg()
                .ok_or_else(|| "integer overflow".to_owned()),
            Token::Op("+") => self.unary(),
            Token::Op("~") => Ok(!self.unary()?),
            Token::Op("!") => Ok(i64::from(self.unary()? == 0)),
            Token::Op("(") => {
                let value = self.expression(0)?;
                self.expect(")")?;
                Ok(value)
            }
            Token::Name(name) => self.name(&name),
            Token::Op(op) => Err(format!("unexpected `{op}`")),
        }
    }

    fn name(&mut self, name: &str) -> Result<i64, String> {
        if let Some(value) = self.scope.get(name) {
            return Ok(*value);
        }
        match self.constants.definitions.get(name) {
            Some(Definition::Value { style, text }) => {
                self.constants
                    .eval_scoped(text, *style, &HashMap::new(), self.depth + 1)
            }
            Some(Definition::Function { params, body }) => {
                self.expect("(")?;
                let mut scope = HashMap::new();
                for (index, param) in params.iter().enumerate() {
                    if index > 0 {
                        self.expect(",")?;
                    }
                    scope.insert(param.clone(), self.expression(0)?);
                }
                self.expect(")")?;
                self.constants
                    .eval_scoped(body, Style::C, &scope, self.depth + 1)
            }
            None => Err(format!("unknown constant `{name}`")),
        }
    }
}

fn apply(op: &str, left: i64, right: i64) -> Result<i64, String> {
    let overflow = || "integer overflow".to_owned();
    match op {
        "*" => left.checked_mul(right).ok_or_else(overflow),
        "/" => left
            .checked_div(right)
            .ok_or_else(|| "division by zero".to_owned()),
        "%" => left
            .checked_rem(right)
            .ok_or_else(|| "division by zero".to_owned()),
        "+" => left.checked_add(right).ok_or_else(overflow),
        "-" => left.checked_sub(right).ok_or_else(overflow),
        "<<" => u32::try_from(right)
            .ok()
            .filter(|shift| *shift < 63)
            .and_then(|shift| left.checked_mul(1_i64 << shift))
            .ok_or_else(overflow),
        ">>" => u32::try_from(right)
            .ok()
            .filter(|shift| *shift < 63)
            .map(|shift| left >> shift)
            .ok_or_else(overflow),
        "&" => Ok(left & right),
        "|" => Ok(left | right),
        "^" => Ok(left ^ right),
        _ => Err(format!("unsupported operator `{op}`")),
    }
}
