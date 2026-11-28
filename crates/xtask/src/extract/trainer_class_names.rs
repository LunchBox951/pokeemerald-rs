//! Trainer class display names (upstream `gTrainerClassNames`,
//! `src/data/text/trainer_class_names.h`).
//!
//! Emits one raw pack entry, `trainer-class-name/table`, in the schema
//! [`assets::pack::TrainerClassNameError`] guards: the display name of each
//! [`assets::TrainerClass`], as the charmap bytes `tools/preproc` gives `_()`
//! (glyph tokens such as `{PKMN}` stay their charmap bytes) followed by the
//! `EOS` it appends. `include/constants/trainers.h` supplies the class index of
//! each designator, and the table must cover indices `0..66` exactly once, each
//! define holding the index of the [`assets::TrainerClass`] constant of its
//! name, so a class added, renumbered, or swapped upstream fails loudly instead
//! of shifting a name.
//! Nothing is compiled or executed.

use std::collections::HashMap;
use std::path::Path;

use assets::{
    TrainerClass, TrainerClassNameTable, TRAINER_CLASS_NAME_COUNT, TRAINER_CLASS_NAME_ROW_BYTES,
};
use pack_format::PackWriter;

use super::text_source::{
    ident, is_punct, string_args, tokenize_with_directive, Charmap, Fault, Tok, Token,
};
use super::{read_text, ExtractError};

const PACK_ID: &str = "trainer-class-name/table";
const CHARMAP_PATH: &str = "charmap.txt";
const CONSTANTS_PATH: &str = "include/constants/trainers.h";
const NAMES_PATH: &str = "src/data/text/trainer_class_names.h";
const TABLE_SYMBOL: &str = "gTrainerClassNames";
const CLASS_PREFIX: &str = "TRAINER_CLASS_";
const EOS: u8 = 0xFF;

/// Each `TRAINER_CLASS_*` symbol beside the stable Rust identity it must
/// equal, spelled once so the name and the constant cannot disagree.
macro_rules! class_symbols {
    ($($name:ident),+ $(,)?) => {
        [$((concat!("TRAINER_CLASS_", stringify!($name)), TrainerClass::$name)),+]
    };
}

const CLASS_SYMBOLS: [(&str, TrainerClass); TRAINER_CLASS_NAME_COUNT] = class_symbols!(
    PKMN_TRAINER_1,
    PKMN_TRAINER_2,
    HIKER,
    TEAM_AQUA,
    PKMN_BREEDER,
    COOLTRAINER,
    BIRD_KEEPER,
    COLLECTOR,
    SWIMMER_M,
    TEAM_MAGMA,
    EXPERT,
    AQUA_ADMIN,
    BLACK_BELT,
    AQUA_LEADER,
    HEX_MANIAC,
    AROMA_LADY,
    RUIN_MANIAC,
    INTERVIEWER,
    TUBER_F,
    TUBER_M,
    LADY,
    BEAUTY,
    RICH_BOY,
    POKEMANIAC,
    GUITARIST,
    KINDLER,
    CAMPER,
    PICNICKER,
    BUG_MANIAC,
    PSYCHIC,
    GENTLEMAN,
    ELITE_FOUR,
    LEADER,
    SCHOOL_KID,
    SR_AND_JR,
    WINSTRATE,
    POKEFAN,
    YOUNGSTER,
    CHAMPION,
    FISHERMAN,
    TRIATHLETE,
    DRAGON_TAMER,
    NINJA_BOY,
    BATTLE_GIRL,
    PARASOL_LADY,
    SWIMMER_F,
    TWINS,
    SAILOR,
    COOLTRAINER_2,
    MAGMA_ADMIN,
    RIVAL,
    BUG_CATCHER,
    PKMN_RANGER,
    MAGMA_LEADER,
    LASS,
    YOUNG_COUPLE,
    OLD_COUPLE,
    SIS_AND_BRO,
    SALON_MAIDEN,
    DOME_ACE,
    PALACE_MAVEN,
    ARENA_TYCOON,
    FACTORY_HEAD,
    PIKE_QUEEN,
    PYRAMID_KING,
    RS_PROTAG,
);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum File {
    Charmap,
    Constants,
    Names,
}

impl File {
    const fn path(self) -> &'static str {
        match self {
            Self::Charmap => CHARMAP_PATH,
            Self::Constants => CONSTANTS_PATH,
            Self::Names => NAMES_PATH,
        }
    }
}

type Located<T> = Result<T, (File, Fault)>;

fn at(file: File) -> impl FnOnce(Fault) -> (File, Fault) {
    move |fault| (file, fault)
}

/// Extracts the trainer class-name table from the checkout.
pub(super) fn extract_trainer_class_names(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let charmap = read_text(&upstream.join(CHARMAP_PATH))?;
    let constants = read_text(&upstream.join(CONSTANTS_PATH))?;
    let names = read_text(&upstream.join(NAMES_PATH))?;
    let bytes = build(&charmap, &constants, &names).map_err(|(file, fault)| {
        ExtractError::TrainerClassNameSource {
            path: upstream.join(file.path()),
            line: fault.line,
            detail: fault.detail,
        }
    })?;
    writer.push(pack_format::raw_entry(PACK_ID.to_owned(), bytes));
    Ok(())
}

fn build(charmap: &str, constants: &str, names: &str) -> Located<Vec<u8>> {
    let charmap = Charmap::parse(charmap).map_err(at(File::Charmap))?;
    let indices = parse_class_indices(constants).map_err(at(File::Constants))?;
    let rows = parse_rows(names, &charmap, &indices).map_err(at(File::Names))?;
    let table = TrainerClassNameTable::from_entries(
        rows.iter().map(|(id, stream)| (*id, stream.as_slice())),
    )
    .map_err(|e| (File::Names, Fault::new(0, e)))?;
    Ok(table.encode())
}

/// A C integer literal, decimal or `0x` hexadecimal. A longer literal with a
/// leading `0` is octal in C, so it is refused instead of read as decimal.
fn number(text: &str) -> Option<usize> {
    match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        Some(hex) => usize::from_str_radix(hex, 16).ok(),
        None if text.len() > 1 && text.starts_with('0') => None,
        None => text.parse().ok(),
    }
}

/// Every `TRAINER_CLASS_*` define and its index. The set must be exactly the
/// indices `0..66`, one define each, and each define must equal the
/// [`TrainerClass`] constant of its name, so an added, renumbered, or swapped
/// class is refused.
fn parse_class_indices(text: &str) -> Result<HashMap<String, usize>, Fault> {
    let mut out: HashMap<String, usize> = HashMap::new();
    let mut taken = [false; TRAINER_CLASS_NAME_COUNT];
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let code = raw.split("//").next().unwrap_or("").trim();
        let Some(rest) = code.strip_prefix("#define") else {
            continue;
        };
        let mut words = rest.split_whitespace();
        let Some(name) = words.next().filter(|n| n.starts_with(CLASS_PREFIX)) else {
            continue;
        };
        let value = words
            .next()
            .and_then(number)
            .filter(|_| words.next().is_none())
            .ok_or_else(|| Fault::new(line, format!("`{name}` is not an integer literal")))?;
        let slot = taken.get_mut(value).ok_or_else(|| {
            Fault::new(
                line,
                format!("`{name}` is {value}, outside the {TRAINER_CLASS_NAME_COUNT} classes"),
            )
        })?;
        if std::mem::replace(slot, true) {
            return Err(Fault::new(
                line,
                format!("`{name}` repeats class index {value}"),
            ));
        }
        if out.insert(name.to_owned(), value).is_some() {
            return Err(Fault::new(line, format!("`{name}` is defined twice")));
        }
        let (_, class) = CLASS_SYMBOLS
            .iter()
            .find(|(symbol, _)| *symbol == name)
            .ok_or_else(|| Fault::new(line, format!("`{name}` names no `TrainerClass`")))?;
        if usize::from(class.index()) != value {
            return Err(Fault::new(
                line,
                format!(
                    "`{name}` is {value}, but its `TrainerClass` identity is {}",
                    class.index()
                ),
            ));
        }
    }
    if out.len() != TRAINER_CLASS_NAME_COUNT {
        return Err(Fault::new(
            0,
            format!(
                "{} `{CLASS_PREFIX}*` defines, expected {TRAINER_CLASS_NAME_COUNT}",
                out.len()
            ),
        ));
    }
    Ok(out)
}

/// The `gTrainerClassNames[][13]` designated rows: `(class index, stream)` with
/// `EOS` appended, in source order.
fn parse_rows(
    text: &str,
    charmap: &Charmap,
    indices: &HashMap<String, usize>,
) -> Result<Vec<(u16, Vec<u8>)>, Fault> {
    let (tokens, directive) = tokenize_with_directive(text)?;
    // The scanner does not splice lines, so a comment closed across a
    // backslash-newline hides the directive after it; a physical line that
    // begins with `#` is refused as well.
    let hash_line = text
        .lines()
        .position(|line| line.trim_start().starts_with('#'))
        .map(|index| index + 1);
    if let Some(line) = directive.into_iter().chain(hash_line).min() {
        return Err(Fault::new(
            line,
            "preprocessor lines are not supported in the class-name table",
        ));
    }
    let mut k = initializer_start(&tokens)?;
    let mut rows = Vec::with_capacity(TRAINER_CLASS_NAME_COUNT);
    let mut seen = [false; TRAINER_CLASS_NAME_COUNT];
    while !is_punct(&tokens, k, '}') {
        let line = tokens.get(k).map_or(0, |t| t.line);
        let (true, Some(class), true, true, Some("_"), true) = (
            is_punct(&tokens, k, '['),
            ident(&tokens, k + 1),
            is_punct(&tokens, k + 2, ']'),
            is_punct(&tokens, k + 3, '='),
            ident(&tokens, k + 4),
            is_punct(&tokens, k + 5, '('),
        ) else {
            return Err(Fault::new(line, "unexpected shape in class-name table"));
        };
        let index = *indices
            .get(class)
            .ok_or_else(|| Fault::new(line, format!("`{class}` is not a trainer class")))?;
        if std::mem::replace(&mut seen[index], true) {
            return Err(Fault::new(line, format!("`{class}` row appears twice")));
        }
        let (body, next) = string_args(&tokens, k + 6)?;
        let mut bytes = charmap.convert(&body, line)?;
        bytes.push(EOS);
        if bytes.len() > TRAINER_CLASS_NAME_ROW_BYTES {
            return Err(Fault::new(
                line,
                format!("`{class}` row exceeds its {TRAINER_CLASS_NAME_ROW_BYTES} bytes"),
            ));
        }
        rows.push((u16::try_from(index).expect("index is below 66"), bytes));
        k = next;
        if is_punct(&tokens, k, ',') {
            k += 1;
        } else if !is_punct(&tokens, k, '}') {
            return Err(Fault::new(line, "class-name row is not comma-separated"));
        }
    }
    if !is_punct(&tokens, k + 1, ';') || tokens.len() != k + 2 {
        return Err(Fault::new(
            tokens.get(k).map_or(0, |t| t.line),
            "unexpected text after the class-name table",
        ));
    }
    if let Some(missing) = seen.iter().position(|seen| !seen) {
        return Err(Fault::new(
            0,
            format!("class index {missing} has no row in the class-name table"),
        ));
    }
    Ok(rows)
}

/// Index just past the `{` of the one `gTrainerClassNames[][13] = {`.
fn initializer_start(tokens: &[Token]) -> Result<usize, Fault> {
    let mut found = None;
    for (i, token) in tokens.iter().enumerate() {
        if token.tok != Tok::Ident(TABLE_SYMBOL.to_owned()) {
            continue;
        }
        let line = token.line;
        let width = row_width(tokens, i + 1)
            .ok_or_else(|| Fault::new(line, format!("`{TABLE_SYMBOL}` has an unexpected shape")))?;
        if found.is_some() {
            return Err(Fault::new(
                line,
                format!("`{TABLE_SYMBOL}` is initialized twice"),
            ));
        }
        if width != TRAINER_CLASS_NAME_ROW_BYTES {
            return Err(Fault::new(
                line,
                format!(
                    "`{TABLE_SYMBOL}` rows are {width} bytes wide, expected {TRAINER_CLASS_NAME_ROW_BYTES}"
                ),
            ));
        }
        found = Some(i + 9);
    }
    found.ok_or_else(|| Fault::new(0, format!("no initializer for `{TABLE_SYMBOL}`")))
}

/// The `N` of a `[][N] = {` at `start`, if the tokens have exactly that shape
/// with `N` of two digits. The tokenizer reads each digit as punctuation.
fn row_width(tokens: &[Token], start: usize) -> Option<usize> {
    let digit = |index: usize| match tokens.get(index).map(|t| &t.tok) {
        Some(Tok::Punct(c)) => c.to_digit(10),
        _ => None,
    };
    let shape = is_punct(tokens, start, '[')
        && is_punct(tokens, start + 1, ']')
        && is_punct(tokens, start + 2, '[')
        && is_punct(tokens, start + 5, ']')
        && is_punct(tokens, start + 6, '=')
        && is_punct(tokens, start + 7, '{');
    if !shape {
        return None;
    }
    let width = digit(start + 3)? * 10 + digit(start + 4)?;
    usize::try_from(width).ok()
}

#[cfg(test)]
mod tests;
