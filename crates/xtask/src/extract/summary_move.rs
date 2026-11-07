//! Summary-screen move and contest records (upstream
//! `src/data/text/move_descriptions.h`, `src/data/contest_moves.h`,
//! `src/data/contest_text_tables.h`, `data/text/contest_strings.inc`).
//!
//! Emits one raw pack entry, `summary-move/table`, in the schema
//! [`assets::pack::SummaryMoveTable`] validates:
//!
//! - `gMoveDescriptionPointers`, keyed by `MOVE_* - 1` as upstream indexes it,
//!   each pointed-to `_()` string mapped to charmap bytes through the
//!   checkout's own `charmap.txt` with the terminator appended;
//! - the effect and category of every `gContestMoves` row (the combo fields are
//!   contest mechanics and stay behind);
//! - every `gContestEffects` row, with the raw appeal and jam; and
//! - `gContestEffectDescriptionPointers`, whose targets are assembly `.string`
//!   labels in `contest_strings.inc`, concatenated up to their explicit `$`.
//!
//! IDs come from the checkout's own `#define`s. Nothing is compiled or executed.

use std::collections::HashMap;
use std::path::Path;

use assets::pack::{
    ContestEffectRecord, ContestMoveRecord, SummaryMoveError, SummaryMoveSection, SummaryMoveTable,
    CONTEST_CATEGORY_COUNT, CONTEST_EFFECT_COUNT, CONTEST_MOVE_COUNT, MOVE_DESCRIPTION_COUNT,
};
use pack_format::PackWriter;

use super::text_source::{
    ident, initializer_start, is_punct, parse_defines, parse_definitions, resolve, tokenize,
    Charmap, Fault, Tok, Token,
};
use super::{read_text, ExtractError};

const PACK_ID: &str = "summary-move/table";
const CHARMAP_PATH: &str = "charmap.txt";
const MOVES_PATH: &str = "include/constants/moves.h";
const CONTEST_PATH: &str = "include/constants/contest.h";
const GLOBAL_PATH: &str = "include/constants/global.h";
const DESCRIPTIONS_PATH: &str = "src/data/text/move_descriptions.h";
const CONTEST_MOVES_PATH: &str = "src/data/contest_moves.h";
const CONTEST_TEXT_PATH: &str = "src/data/contest_text_tables.h";
const CONTEST_STRINGS_PATH: &str = "data/text/contest_strings.inc";
const DESCRIPTION_TABLE: &str = "gMoveDescriptionPointers";
const CONTEST_MOVES_TABLE: &str = "gContestMoves";
const CONTEST_EFFECTS_TABLE: &str = "gContestEffects";
const EFFECT_DESCRIPTION_TABLE: &str = "gContestEffectDescriptionPointers";

#[derive(Debug, Clone, Copy)]
enum File {
    Charmap,
    Moves,
    Contest,
    Global,
    Descriptions,
    ContestMoves,
    ContestText,
    ContestStrings,
}

impl File {
    const fn path(self) -> &'static str {
        match self {
            Self::Charmap => CHARMAP_PATH,
            Self::Moves => MOVES_PATH,
            Self::Contest => CONTEST_PATH,
            Self::Global => GLOBAL_PATH,
            Self::Descriptions => DESCRIPTIONS_PATH,
            Self::ContestMoves => CONTEST_MOVES_PATH,
            Self::ContestText => CONTEST_TEXT_PATH,
            Self::ContestStrings => CONTEST_STRINGS_PATH,
        }
    }
}

type Located<T> = Result<T, (File, Fault)>;

fn at(file: File) -> impl FnOnce(Fault) -> (File, Fault) {
    move |fault| (file, fault)
}

/// The upstream sources the table is built from.
struct Sources<'a> {
    charmap: &'a str,
    moves: &'a str,
    contest: &'a str,
    global: &'a str,
    descriptions: &'a str,
    contest_moves: &'a str,
    contest_text: &'a str,
    contest_strings: &'a str,
}

/// Extracts the summary-screen move and contest table from the checkout.
pub(super) fn extract_summary_move(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let bytes = extract_payload(upstream)?;
    writer.push(pack_format::raw_entry(PACK_ID.to_owned(), bytes));
    Ok(())
}

/// The canonical `summary-move/table` payload the checkout yields.
fn extract_payload(upstream: &Path) -> Result<Vec<u8>, ExtractError> {
    let read = |file: File| read_text(&upstream.join(file.path()));
    let (charmap, moves, contest, global) = (
        read(File::Charmap)?,
        read(File::Moves)?,
        read(File::Contest)?,
        read(File::Global)?,
    );
    let (descriptions, contest_moves, contest_text, contest_strings) = (
        read(File::Descriptions)?,
        read(File::ContestMoves)?,
        read(File::ContestText)?,
        read(File::ContestStrings)?,
    );
    build(&Sources {
        charmap: &charmap,
        moves: &moves,
        contest: &contest,
        global: &global,
        descriptions: &descriptions,
        contest_moves: &contest_moves,
        contest_text: &contest_text,
        contest_strings: &contest_strings,
    })
    .map_err(|(file, fault)| ExtractError::SummaryMoveSource {
        path: upstream.join(file.path()),
        line: fault.line,
        detail: fault.detail,
    })
}

type Defines = HashMap<String, (String, usize)>;

fn build(src: &Sources<'_>) -> Located<Vec<u8>> {
    let charmap = Charmap::parse(src.charmap).map_err(at(File::Charmap))?;
    let mut moves = parse_defines(src.moves);
    check_count(&moves, "MOVES_COUNT", CONTEST_MOVE_COUNT).map_err(at(File::Moves))?;
    let contest = parse_defines(src.contest);
    let global = parse_defines(src.global);
    check_count(
        &global,
        "CONTEST_CATEGORIES_COUNT",
        usize::from(CONTEST_CATEGORY_COUNT),
    )
    .map_err(at(File::Global))?;
    check_count(
        &contest,
        "CONTEST_EFFECT_DONT_EXCITE_AUDIENCE",
        CONTEST_EFFECT_COUNT - 1,
    )
    .map_err(at(File::Contest))?;
    let mut defines = Defines::new();
    defines.extend(global);
    defines.extend(contest);
    defines.extend(moves.drain());

    let descriptions = descriptions(src, &charmap, &defines)?;
    let moves = contest_moves(src, &defines)?;
    let effects = contest_effects(src, &defines)?;
    let texts = effect_descriptions(src, &charmap, &defines)?;

    let effects: Vec<ContestEffectRecord<'_>> = effects
        .iter()
        .map(|row| {
            let description = texts.get(&row.id).ok_or_else(|| {
                (
                    File::ContestText,
                    Fault::new(0, format!("contest effect {} has no description", row.id)),
                )
            })?;
            Ok(ContestEffectRecord {
                id: row.id,
                effect_type: row.effect_type,
                appeal: row.appeal,
                jam: row.jam,
                description,
            })
        })
        .collect::<Located<_>>()?;
    // A description with no effect row would otherwise be dropped silently.
    let mut surplus: Vec<u16> = texts
        .keys()
        .copied()
        .filter(|id| !effects.iter().any(|row| row.id == *id))
        .collect();
    surplus.sort_unstable();
    if let Some(extra) = surplus.first() {
        return Err((
            File::ContestText,
            Fault::new(
                0,
                format!("contest effect {extra} has a description but no row"),
            ),
        ));
    }
    let table = SummaryMoveTable::from_entries(
        descriptions.iter().map(|(slot, b)| (*slot, b.as_slice())),
        moves,
        effects,
    )
    .map_err(|e| (error_file(&e), Fault::new(0, e)))?;
    Ok(table.encode())
}

/// The source file a schema refusal is about.
fn error_file(error: &SummaryMoveError) -> File {
    match error {
        SummaryMoveError::IdOutOfRange { section, .. }
        | SummaryMoveError::DuplicateId { section, .. }
        | SummaryMoveError::MissingId { section, .. }
        | SummaryMoveError::MalformedString { section, .. } => match section {
            SummaryMoveSection::Description => File::Descriptions,
            SummaryMoveSection::ContestMove => File::ContestMoves,
            SummaryMoveSection::ContestEffect => {
                if matches!(error, SummaryMoveError::MalformedString { .. }) {
                    File::ContestStrings
                } else {
                    File::ContestMoves
                }
            }
        },
        _ => File::ContestMoves,
    }
}

/// A `#define` that must equal the count this reader's schema is pinned to.
fn check_count(defines: &Defines, name: &str, expected: usize) -> Result<(), Fault> {
    let found = resolve(defines, name)?;
    if usize::try_from(found) != Ok(expected) {
        return Err(Fault::new(
            0,
            format!("`{name}` is {found}, expected the pinned {expected}"),
        ));
    }
    Ok(())
}

/// A `[NAME]` or `[NAME - n]` designator: the resolved value and its line.
fn designator(tokens: &[Token], k: usize, defines: &Defines) -> Result<(u32, usize, usize), Fault> {
    let line = tokens.get(k).map_or(0, |t| t.line);
    let (true, Some(name)) = (is_punct(tokens, k, '['), ident(tokens, k + 1)) else {
        return Err(Fault::new(line, "expected a `[NAME]` designator"));
    };
    let mut value = resolve(defines, name)?;
    let mut next = k + 2;
    if is_punct(tokens, next, '-') {
        let (n, after) = number(tokens, next + 1)
            .ok_or_else(|| Fault::new(line, format!("`{name}` is not followed by a number")))?;
        value = value
            .checked_sub(n)
            .ok_or_else(|| Fault::new(line, format!("`{name} - {n}` is negative")))?;
        next = after;
    }
    if !is_punct(tokens, next, ']') || !is_punct(tokens, next + 1, '=') {
        return Err(Fault::new(line, "designator is not `] =`"));
    }
    Ok((value, line, next + 2))
}

/// A decimal literal from consecutive digit tokens on one line; the shared
/// tokenizer emits each digit on its own.
fn number(tokens: &[Token], mut k: usize) -> Option<(u32, usize)> {
    let line = tokens.get(k)?.line;
    let mut value: u32 = 0;
    let start = k;
    while let Some(token) = tokens.get(k).filter(|t| t.line == line) {
        let Tok::Punct(c) = token.tok else {
            break;
        };
        let Some(digit) = c.to_digit(10) else { break };
        value = value.checked_mul(10)?.checked_add(digit)?;
        k += 1;
    }
    // A hex or suffixed literal leaves letters behind; refuse it rather than misread it.
    let glued = tokens
        .get(k)
        .filter(|t| t.line == line)
        .is_some_and(|t| matches!(t.tok, Tok::Ident(_)));
    (k > start && !glued).then_some((value, k))
}

fn id16(value: u32, line: usize, what: &str) -> Result<u16, Fault> {
    u16::try_from(value).map_err(|_| Fault::new(line, format!("{what} {value} does not fit")))
}

fn id8(value: u32, line: usize, what: &str) -> Result<u8, Fault> {
    u8::try_from(value).map_err(|_| Fault::new(line, format!("{what} {value} does not fit")))
}

/// A designated field of a record: its name, value, and line.
type Field = (String, Value, usize);

/// A parsed record: its fields, whether it was the zeroed `{0}` form, and the
/// index past its closing brace.
type Record = (Vec<Field>, bool, usize);

/// One initializer value.
#[derive(Debug)]
enum Value {
    Number(u32),
    Name(String),
    /// A nested `{...}` group, whose contents this table does not carry.
    Group,
}

/// A brace-delimited record: `{0}` or `{ .field = value, ... }`. Returns the
/// designated fields and the index past the closing brace.
fn record(tokens: &[Token], mut k: usize) -> Result<Record, Fault> {
    let line = tokens.get(k).map_or(0, |t| t.line);
    if !is_punct(tokens, k, '{') {
        return Err(Fault::new(line, "record does not open with `{`"));
    }
    k += 1;
    // `{0}`: a zeroed row.
    if let Some((0, after)) = number(tokens, k) {
        if is_punct(tokens, after, '}') {
            return Ok((Vec::new(), true, after + 1));
        }
    }
    let mut fields = Vec::new();
    while !is_punct(tokens, k, '}') {
        let line = tokens.get(k).map_or(0, |t| t.line);
        if is_punct(tokens, k, '{') {
            // C initializes the member after the last designated one: upstream
            // writes `.comboStarterId = 0,{0, COMBO_STARTER_LEER}` for `comboMoves`.
            let member = match fields.last() {
                Some((prev, _, _)) if prev == "comboStarterId" => "comboMoves",
                _ => {
                    return Err(Fault::new(
                        line,
                        "a positional group follows no known member",
                    ))
                }
            };
            k = skip_group(tokens, k)?;
            fields.push((member.to_owned(), Value::Group, line));
            if is_punct(tokens, k, ',') {
                k += 1;
            } else if !is_punct(tokens, k, '}') {
                return Err(Fault::new(line, "field is not comma-separated"));
            }
            continue;
        }
        let (true, Some(name), true) = (
            is_punct(tokens, k, '.'),
            ident(tokens, k + 1),
            is_punct(tokens, k + 2, '='),
        ) else {
            return Err(Fault::new(line, "expected a `.field =` designator"));
        };
        k += 3;
        let value = if is_punct(tokens, k, '{') {
            k = skip_group(tokens, k)?;
            Value::Group
        } else if let Some((n, after)) = number(tokens, k) {
            k = after;
            Value::Number(n)
        } else if let Some(word) = ident(tokens, k) {
            k += 1;
            Value::Name(word.to_owned())
        } else {
            return Err(Fault::new(
                line,
                format!("`.{name}` has an unsupported value"),
            ));
        };
        fields.push((name.to_owned(), value, line));
        if is_punct(tokens, k, ',') {
            k += 1;
        } else if !is_punct(tokens, k, '}') {
            return Err(Fault::new(line, "field is not comma-separated"));
        }
    }
    Ok((fields, false, k + 1))
}

/// Index past the `}` matching the `{` at `k`.
fn skip_group(tokens: &[Token], mut k: usize) -> Result<usize, Fault> {
    let line = tokens.get(k).map_or(0, |t| t.line);
    let mut depth = 0usize;
    while let Some(token) = tokens.get(k) {
        match token.tok {
            Tok::Punct('{') => depth += 1,
            Tok::Punct('}') => depth -= 1,
            _ => {}
        }
        k += 1;
        if depth == 0 {
            return Ok(k);
        }
    }
    Err(Fault::new(line, "unterminated `{`"))
}

/// Take a field of `fields` exactly once, as the kind the caller expects.
fn field<'a>(
    fields: &'a [(String, Value, usize)],
    name: &str,
    row_line: usize,
) -> Result<&'a Field, Fault> {
    let mut found = fields.iter().filter(|(n, _, _)| n == name);
    let first = found
        .next()
        .ok_or_else(|| Fault::new(row_line, format!("record has no `.{name}`")))?;
    match found.next() {
        None => Ok(first),
        Some((_, _, line)) => Err(Fault::new(*line, format!("`.{name}` is set twice"))),
    }
}

fn numeric_field(
    fields: &[Field],
    name: &str,
    row_line: usize,
    defines: &Defines,
) -> Result<(u32, usize), Fault> {
    let (_, value, line) = field(fields, name, row_line)?;
    match value {
        Value::Number(n) => Ok((*n, *line)),
        Value::Name(word) => Ok((resolve(defines, word)?, *line)),
        Value::Group => Err(Fault::new(*line, format!("`.{name}` is not a scalar"))),
    }
}

/// Reject a field outside `allowed`, so a renamed upstream field cannot be skipped.
fn only_fields(fields: &[Field], allowed: &[&str]) -> Result<(), Fault> {
    match fields
        .iter()
        .find(|(n, _, _)| !allowed.contains(&n.as_str()))
    {
        Some((name, _, line)) => Err(Fault::new(*line, format!("unexpected field `.{name}`"))),
        None => Ok(()),
    }
}

fn descriptions(
    src: &Sources<'_>,
    charmap: &Charmap,
    defines: &Defines,
) -> Located<Vec<(u16, Vec<u8>)>> {
    let tokens = tokenize(src.descriptions).map_err(at(File::Descriptions))?;
    let texts = parse_definitions(&tokens).map_err(at(File::Descriptions))?;
    let mut k = initializer_start(&tokens, DESCRIPTION_TABLE).map_err(at(File::Descriptions))?;
    let mut rows = Vec::with_capacity(MOVE_DESCRIPTION_COUNT);
    while !is_punct(&tokens, k, '}') {
        let (slot, line, next) = designator(&tokens, k, defines).map_err(at(File::Descriptions))?;
        let slot = id16(slot, line, "slot").map_err(at(File::Descriptions))?;
        let symbol = ident(&tokens, next).ok_or((
            File::Descriptions,
            Fault::new(line, "slot does not name a string"),
        ))?;
        let (body, body_line) = texts.get(symbol).ok_or_else(|| {
            (
                File::Descriptions,
                Fault::new(line, format!("`{symbol}` has no definition")),
            )
        })?;
        let mut bytes = charmap
            .convert(body, *body_line)
            .map_err(at(File::Descriptions))?;
        bytes.push(0xFF);
        rows.push((slot, bytes));
        k = next + 1;
        if is_punct(&tokens, k, ',') {
            k += 1;
        } else if !is_punct(&tokens, k, '}') {
            return Err((
                File::Descriptions,
                Fault::new(line, "slot is not comma-separated"),
            ));
        }
    }
    Ok(rows)
}

fn contest_moves(src: &Sources<'_>, defines: &Defines) -> Located<Vec<ContestMoveRecord>> {
    let tokens = tokenize(src.contest_moves).map_err(at(File::ContestMoves))?;
    let mut k = initializer_start(&tokens, CONTEST_MOVES_TABLE).map_err(at(File::ContestMoves))?;
    let mut rows = Vec::with_capacity(CONTEST_MOVE_COUNT);
    while !is_punct(&tokens, k, '}') {
        let row = (|| {
            let (move_id, line, next) = designator(&tokens, k, defines)?;
            let (fields, zeroed, after) = record(&tokens, next)?;
            let (effect, category) = if zeroed {
                (0, 0)
            } else {
                only_fields(
                    &fields,
                    &["effect", "contestCategory", "comboStarterId", "comboMoves"],
                )?;
                let (effect, effect_line) = numeric_field(&fields, "effect", line, defines)?;
                let (category, category_line) =
                    numeric_field(&fields, "contestCategory", line, defines)?;
                (
                    id8(effect, effect_line, "effect")?,
                    id8(category, category_line, "category")?,
                )
            };
            Ok::<_, Fault>((
                ContestMoveRecord {
                    move_id: id16(move_id, line, "move")?,
                    effect,
                    category,
                },
                after,
                line,
            ))
        })()
        .map_err(at(File::ContestMoves))?;
        rows.push(row.0);
        k = row.1;
        if is_punct(&tokens, k, ',') {
            k += 1;
        } else if !is_punct(&tokens, k, '}') {
            return Err((
                File::ContestMoves,
                Fault::new(row.2, "row is not comma-separated"),
            ));
        }
    }
    Ok(rows)
}

/// A `gContestEffects` row before its description joins it.
struct EffectRow {
    id: u16,
    effect_type: u8,
    appeal: u8,
    jam: u8,
}

fn contest_effects(src: &Sources<'_>, defines: &Defines) -> Located<Vec<EffectRow>> {
    let tokens = tokenize(src.contest_moves).map_err(at(File::ContestMoves))?;
    let mut k =
        initializer_start(&tokens, CONTEST_EFFECTS_TABLE).map_err(at(File::ContestMoves))?;
    let mut rows = Vec::with_capacity(CONTEST_EFFECT_COUNT);
    while !is_punct(&tokens, k, '}') {
        let (row, after, line) = (|| {
            let (id, line, next) = designator(&tokens, k, defines)?;
            let (fields, zeroed, after) = record(&tokens, next)?;
            if zeroed {
                return Err(Fault::new(line, "a contest effect row cannot be `{0}`"));
            }
            only_fields(&fields, &["effectType", "appeal", "jam"])?;
            let scalar = |name: &str| -> Result<u8, Fault> {
                let (value, value_line) = numeric_field(&fields, name, line, defines)?;
                id8(value, value_line, name)
            };
            Ok::<_, Fault>((
                EffectRow {
                    id: id16(id, line, "contest effect")?,
                    effect_type: scalar("effectType")?,
                    appeal: scalar("appeal")?,
                    jam: scalar("jam")?,
                },
                after,
                line,
            ))
        })()
        .map_err(at(File::ContestMoves))?;
        rows.push(row);
        k = after;
        if is_punct(&tokens, k, ',') {
            k += 1;
        } else if !is_punct(&tokens, k, '}') {
            return Err((
                File::ContestMoves,
                Fault::new(line, "row is not comma-separated"),
            ));
        }
    }
    Ok(rows)
}

/// `gContestEffectDescriptionPointers` resolved through `contest_strings.inc`.
fn effect_descriptions(
    src: &Sources<'_>,
    charmap: &Charmap,
    defines: &Defines,
) -> Located<HashMap<u16, Vec<u8>>> {
    let tokens = tokenize(src.contest_text).map_err(at(File::ContestText))?;
    let mut k =
        initializer_start(&tokens, EFFECT_DESCRIPTION_TABLE).map_err(at(File::ContestText))?;
    let mut symbols: Vec<(u16, String, usize)> = Vec::with_capacity(CONTEST_EFFECT_COUNT);
    while !is_punct(&tokens, k, '}') {
        let (id, line, next) = designator(&tokens, k, defines).map_err(at(File::ContestText))?;
        let id = id16(id, line, "contest effect").map_err(at(File::ContestText))?;
        let symbol = ident(&tokens, next).ok_or((
            File::ContestText,
            Fault::new(line, "slot does not name a string"),
        ))?;
        symbols.push((id, symbol.to_owned(), line));
        k = next + 1;
        if is_punct(&tokens, k, ',') {
            k += 1;
        } else if !is_punct(&tokens, k, '}') {
            return Err((
                File::ContestText,
                Fault::new(line, "slot is not comma-separated"),
            ));
        }
    }
    let wanted: Vec<&str> = symbols.iter().map(|(_, s, _)| s.as_str()).collect();
    let strings =
        parse_labels(src.contest_strings, charmap, &wanted).map_err(at(File::ContestStrings))?;
    let mut out = HashMap::new();
    for (id, symbol, line) in &symbols {
        let bytes = strings.get(symbol.as_str()).ok_or_else(|| {
            (
                File::ContestText,
                Fault::new(*line, format!("`{symbol}` has no definition")),
            )
        })?;
        if out.insert(*id, bytes.clone()).is_some() {
            return Err((
                File::ContestText,
                Fault::new(*line, format!("contest effect {id} is described twice")),
            ));
        }
    }
    Ok(out)
}

/// The wanted assembly labels: `.string` pieces concatenated up to the next
/// label. The explicit `$` supplies the terminator.
fn parse_labels<'a>(
    text: &str,
    charmap: &Charmap,
    wanted: &[&'a str],
) -> Result<HashMap<&'a str, Vec<u8>>, Fault> {
    let mut out: HashMap<&'a str, Vec<u8>> = HashMap::new();
    let mut current: Option<&'a str> = None;
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let trimmed = raw.trim();
        if let Some(label) = trimmed
            .strip_suffix("::")
            .or_else(|| trimmed.strip_suffix(':'))
        {
            current = wanted.iter().copied().find(|w| *w == label);
            if let Some(label) = current {
                if out.insert(label, Vec::new()).is_some() {
                    return Err(Fault::new(line, format!("`{label}` is defined twice")));
                }
            }
            continue;
        }
        let Some(label) = current else { continue };
        if trimmed.is_empty() || trimmed.starts_with('@') {
            continue;
        }
        let body = trimmed
            .strip_prefix(".string")
            .and_then(|rest| rest.trim().strip_prefix('"'))
            .and_then(|rest| rest.strip_suffix('"'))
            .ok_or_else(|| Fault::new(line, format!("unsupported line in `{label}`")))?;
        out.get_mut(label)
            .expect("label was inserted when it was opened")
            .extend(charmap.convert(body, line)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
