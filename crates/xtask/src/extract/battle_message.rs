//! Battle-message token table (upstream `src/battle_message.c`).
//!
//! Emits one raw pack entry, `battle-message/table`, in the schema
//! [`assets::pack::BattleMessageTable`] validates: the full
//! `gBattleStringsTable` (IDs `BATTLESTRINGS_TABLE_START` through
//! `BATTLESTRINGS_COUNT` exclusive, `include/constants/battle_string_ids.h`)
//! and the fixed special-template inventory [`BATTLE_MESSAGE_SPECIAL_TEMPLATES`].
//! Strings are mapped to charmap bytes through the checkout's own `charmap.txt`
//! the way `tools/preproc` maps them, with the `_()` terminator appended.
//! Nothing is compiled or executed; selection and expansion are not ported.
//!
//! Four table slots point at assembly-authored text in `data/text/pc_transfer.inc`;
//! those four labels are read from there, with their explicit `$` terminator.

use std::collections::HashMap;
use std::path::Path;

use assets::pack::{
    BattleMessageTable, BATTLE_MESSAGE_SLOT_COUNT, BATTLE_MESSAGE_SPECIAL_TEMPLATES,
    BATTLE_MESSAGE_TABLE_END, BATTLE_MESSAGE_TABLE_START,
};
use pack_format::PackWriter;

use super::text_source::{
    ident, is_punct, parse_definitions, skip_brackets, string_args, tokenize, Charmap, Fault, Tok,
    Token,
};
use super::{read_text, ExtractError};

const PACK_ID: &str = "battle-message/table";
const CHARMAP_PATH: &str = "charmap.txt";
const IDS_PATH: &str = "include/constants/battle_string_ids.h";
const MESSAGE_PATH: &str = "src/battle_message.c";
const PC_TRANSFER_PATH: &str = "data/text/pc_transfer.inc";
const TABLE_SYMBOL: &str = "gBattleStringsTable";
const TYPE_TABLE_SYMBOL: &str = "sATypeMove_Table";
const TYPE_ROW_CAPACITY: usize = 17;
/// Table targets authored in assembly rather than in `battle_message.c`.
const EXTERNAL_LABELS: [&str; 4] = [
    "gText_PkmnTransferredSomeonesPC",
    "gText_PkmnTransferredLanettesPC",
    "gText_PkmnTransferredSomeonesPCBoxFull",
    "gText_PkmnTransferredLanettesPCBoxFull",
];

#[derive(Debug, Clone, Copy)]
enum File {
    Charmap,
    Ids,
    Message,
    PcTransfer,
}

impl File {
    const fn path(self) -> &'static str {
        match self {
            Self::Charmap => CHARMAP_PATH,
            Self::Ids => IDS_PATH,
            Self::Message => MESSAGE_PATH,
            Self::PcTransfer => PC_TRANSFER_PATH,
        }
    }
}

type Located<T> = Result<T, (File, Fault)>;

fn at(file: File) -> impl FnOnce(Fault) -> (File, Fault) {
    move |fault| (file, fault)
}

/// The four upstream sources the table is built from.
struct Sources<'a> {
    charmap: &'a str,
    ids: &'a str,
    message: &'a str,
    pc_transfer: &'a str,
}

/// Extracts the battle-message table from the checkout.
pub(super) fn extract_battle_message(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let charmap = read_text(&upstream.join(CHARMAP_PATH))?;
    let ids = read_text(&upstream.join(IDS_PATH))?;
    let message = read_text(&upstream.join(MESSAGE_PATH))?;
    let pc_transfer = read_text(&upstream.join(PC_TRANSFER_PATH))?;
    let bytes = build(&Sources {
        charmap: &charmap,
        ids: &ids,
        message: &message,
        pc_transfer: &pc_transfer,
    })
    .map_err(|(file, fault)| ExtractError::BattleMessageSource {
        path: upstream.join(file.path()),
        line: fault.line,
        detail: fault.detail,
    })?;
    writer.push(pack_format::raw_entry(PACK_ID.to_owned(), bytes));
    Ok(())
}

fn build(src: &Sources<'_>) -> Located<Vec<u8>> {
    let charmap = Charmap::parse(src.charmap).map_err(at(File::Charmap))?;
    let defines = parse_defines(src.ids);
    check_bounds(&defines).map_err(at(File::Ids))?;

    let tokens = tokenize(src.message).map_err(at(File::Message))?;
    let definitions = parse_definitions(&tokens).map_err(at(File::Message))?;
    let rows = parse_table_rows(&tokens, &defines).map_err(at(File::Message))?;
    let type_rows = parse_type_rows(&tokens, &charmap).map_err(at(File::Message))?;
    let external = parse_external(src.pc_transfer, &charmap).map_err(at(File::PcTransfer))?;

    let convert = |symbol: &str| -> Located<Vec<u8>> {
        if let Some((body, line)) = definitions.get(symbol) {
            let mut bytes = charmap.convert(body, *line).map_err(at(File::Message))?;
            bytes.push(0xFF);
            Ok(bytes)
        } else if let Some(bytes) = external.get(symbol) {
            Ok(bytes.clone())
        } else if let Some(bytes) = type_rows.get(symbol) {
            Ok(bytes.clone())
        } else {
            Err((
                File::Message,
                Fault::new(0, format!("symbol `{symbol}` has no definition")),
            ))
        }
    };

    for label in EXTERNAL_LABELS {
        if definitions.contains_key(label) {
            return Err((
                File::Message,
                Fault::new(0, format!("`{label}` is also defined in C")),
            ));
        }
    }

    let mut slots = Vec::with_capacity(rows.len());
    for row in &rows {
        slots.push((row.id, convert(&row.symbol)?));
    }
    let mut special = Vec::with_capacity(BATTLE_MESSAGE_SPECIAL_TEMPLATES.len());
    for (index, template) in BATTLE_MESSAGE_SPECIAL_TEMPLATES.iter().enumerate() {
        let id = u16::try_from(index).expect("inventory is far below u16::MAX");
        special.push((id, convert(template.symbol)?));
    }

    let table = BattleMessageTable::from_entries(
        slots.iter().map(|(id, b)| (*id, b.as_slice())),
        special.iter().map(|(id, b)| (*id, b.as_slice())),
    )
    .map_err(|e| (File::Message, Fault::new(0, e)))?;
    Ok(table.encode())
}

/// `#define NAME value` lines: value text and line number.
fn parse_defines(text: &str) -> HashMap<String, (String, usize)> {
    let mut defines = HashMap::new();
    for (index, raw) in text.lines().enumerate() {
        let Some(rest) = raw.trim_start().strip_prefix("#define") else {
            continue;
        };
        let rest = rest.split("//").next().unwrap_or("");
        let rest = rest.split("/*").next().unwrap_or("");
        let mut words = rest.split_whitespace();
        if let Some(name) = words.next() {
            // Keep every value token so an expression such as `381 + 1` is never read as `381`.
            let value = words.collect::<Vec<_>>().join(" ");
            if !value.is_empty() {
                defines.insert(name.to_owned(), (value, index + 1));
            }
        }
    }
    defines
}

fn resolve(defines: &HashMap<String, (String, usize)>, name: &str) -> Result<u32, Fault> {
    let mut current = name;
    for _ in 0..8 {
        let (value, line) = defines
            .get(current)
            .ok_or_else(|| Fault::new(0, format!("`{current}` is not defined")))?;
        if let Ok(n) = value.parse::<u32>() {
            return Ok(n);
        }
        if !value.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
            return Err(Fault::new(*line, format!("`{name}` is not an integer")));
        }
        current = value;
    }
    Err(Fault::new(0, format!("`{name}` aliases too deeply")))
}

fn check_bounds(defines: &HashMap<String, (String, usize)>) -> Result<(), Fault> {
    for (name, expected) in [
        ("BATTLESTRINGS_TABLE_START", BATTLE_MESSAGE_TABLE_START),
        ("BATTLESTRINGS_COUNT", BATTLE_MESSAGE_TABLE_END),
    ] {
        let found = resolve(defines, name)?;
        if found != u32::from(expected) {
            return Err(Fault::new(
                0,
                format!("`{name}` is {found}, expected the pinned {expected}"),
            ));
        }
    }
    Ok(())
}

struct Row {
    id: u16,
    symbol: String,
}

/// The `gBattleStringsTable` initializer, keyed by numeric ID so declaration
/// order never matters.
fn parse_table_rows(
    tokens: &[Token],
    defines: &HashMap<String, (String, usize)>,
) -> Result<Vec<Row>, Fault> {
    let start = initializer_start(tokens, TABLE_SYMBOL)?;
    let mut rows: Vec<Row> = Vec::new();
    let mut seen = HashMap::new();
    let mut k = start;
    while !is_punct(tokens, k, '}') {
        let line = tokens.get(k).map_or(0, |t| t.line);
        let shape = (
            is_punct(tokens, k, '['),
            ident(tokens, k + 1),
            is_punct(tokens, k + 2, '-'),
            ident(tokens, k + 3),
            is_punct(tokens, k + 4, ']'),
            is_punct(tokens, k + 5, '='),
            ident(tokens, k + 6),
        );
        let (
            true,
            Some(id_name),
            true,
            Some("BATTLESTRINGS_TABLE_START"),
            true,
            true,
            Some(symbol),
        ) = shape
        else {
            return Err(Fault::new(line, "unexpected shape in table initializer"));
        };
        let id = resolve(defines, id_name)?;
        let id = u16::try_from(id)
            .ok()
            .filter(|id| (BATTLE_MESSAGE_TABLE_START..BATTLE_MESSAGE_TABLE_END).contains(id))
            .ok_or_else(|| Fault::new(line, format!("`{id_name}` is outside the table range")))?;
        if let Some(first) = seen.insert(id, line) {
            return Err(Fault::new(
                line,
                format!("`{id_name}` repeats the slot of line {first}"),
            ));
        }
        rows.push(Row {
            id,
            symbol: symbol.to_owned(),
        });
        k += 7;
        if is_punct(tokens, k, ',') {
            k += 1;
        } else if !is_punct(tokens, k, '}') {
            return Err(Fault::new(line, "table row is not comma-separated"));
        }
    }
    if rows.len() != BATTLE_MESSAGE_SLOT_COUNT {
        return Err(Fault::new(
            tokens[start].line,
            format!(
                "table has {} slots, expected {BATTLE_MESSAGE_SLOT_COUNT}",
                rows.len()
            ),
        ));
    }
    Ok(rows)
}

/// Index just past the `{` of the one `NAME[...] = {` initializer.
fn initializer_start(tokens: &[Token], name: &str) -> Result<usize, Fault> {
    let mut found = None;
    for (i, token) in tokens.iter().enumerate() {
        if token.tok != Tok::Ident(name.to_owned()) {
            continue;
        }
        let j = skip_brackets(tokens, i + 1);
        if j > i + 1 && is_punct(tokens, j, '=') && is_punct(tokens, j + 1, '{') {
            if found.is_some() {
                return Err(Fault::new(
                    token.line,
                    format!("`{name}` is initialized twice"),
                ));
            }
            found = Some(j + 2);
        }
    }
    found.ok_or_else(|| Fault::new(0, format!("no initializer for `{name}`")))
}

/// `sATypeMove_Table` designated rows, keyed `sATypeMove_Table[TYPE_*]`.
fn parse_type_rows(tokens: &[Token], charmap: &Charmap) -> Result<HashMap<String, Vec<u8>>, Fault> {
    let mut k = initializer_start(tokens, TYPE_TABLE_SYMBOL)?;
    let mut rows = HashMap::new();
    while !is_punct(tokens, k, '}') {
        let line = tokens.get(k).map_or(0, |t| t.line);
        let (true, Some(kind), true, true, Some("_"), true) = (
            is_punct(tokens, k, '['),
            ident(tokens, k + 1),
            is_punct(tokens, k + 2, ']'),
            is_punct(tokens, k + 3, '='),
            ident(tokens, k + 4),
            is_punct(tokens, k + 5, '('),
        ) else {
            return Err(Fault::new(line, "unexpected shape in type-name table"));
        };
        let (body, next) = string_args(tokens, k + 6)?;
        let mut bytes = charmap.convert(&body, line)?;
        bytes.push(0xFF);
        if bytes.len() > TYPE_ROW_CAPACITY {
            return Err(Fault::new(
                line,
                format!("`{kind}` row exceeds its {TYPE_ROW_CAPACITY} bytes"),
            ));
        }
        if rows
            .insert(format!("{TYPE_TABLE_SYMBOL}[{kind}]"), bytes)
            .is_some()
        {
            return Err(Fault::new(line, format!("`{kind}` row appears twice")));
        }
        k = next;
        if is_punct(tokens, k, ',') {
            k += 1;
        } else if !is_punct(tokens, k, '}') {
            return Err(Fault::new(line, "type-name row is not comma-separated"));
        }
    }
    Ok(rows)
}

/// The four whitelisted assembly labels: `.string` pieces concatenated up to the
/// next label. The explicit `$` supplies the terminator.
fn parse_external(text: &str, charmap: &Charmap) -> Result<HashMap<String, Vec<u8>>, Fault> {
    let mut out: HashMap<String, Vec<u8>> = HashMap::new();
    let mut current: Option<&str> = None;
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let trimmed = raw.trim();
        if let Some(label) = trimmed
            .strip_suffix("::")
            .or_else(|| trimmed.strip_suffix(':'))
        {
            current = EXTERNAL_LABELS.iter().copied().find(|l| *l == label);
            if let Some(label) = current {
                if out.insert(label.to_owned(), Vec::new()).is_some() {
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
        let bytes = charmap.convert(body, line)?;
        out.get_mut(label)
            .expect("label was inserted when it was opened")
            .extend(bytes);
    }
    for label in EXTERNAL_LABELS {
        if !out.contains_key(label) {
            return Err(Fault::new(0, format!("`{label}` is not defined")));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
