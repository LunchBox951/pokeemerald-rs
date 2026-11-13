//! Reads the checkout's icon declarations and the species tables from the
//! upstream C source.
//!
//! Only the named declarations and initializer bodies are read, and anything
//! outside the supported shape is an error rather than skipped: a table row
//! the parser could not place would otherwise shift or drop a species.

use std::collections::{BTreeMap, BTreeSet};

use assets::species_icons::{SHEETS, SPECIES_SLOTS, TABLE_SHEETS};

/// Where each icon sheet's PNG lives, relative to the checkout.
const ICON_DIRECTORY: &str = "graphics/pokemon/";
const ICON_DECLARATION: &str = "const u8 gMonIcon_";
const SHEET_TABLE: &str = "gMonIconTable[]";
const PALETTE_INDEX_TABLE: &str = "gMonIconPaletteIndices[]";
const SPEED_SYMBOL: &str = "DeoxysSpeed";
const SPEED_WIDE_SYMBOL: &str = "DeoxysSpeedWide";

/// One declared icon sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SheetSource {
    /// The `gMonIcon_<symbol>` suffix.
    pub(crate) symbol: String,
    /// The PNG, relative to the checkout.
    pub(crate) png: String,
}

/// The sheets and the two per-species tables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Inventory {
    /// Every declared sheet in pack order: the table's sheets by first
    /// appearance, then Deoxys's speed and wide sheets.
    pub(crate) sheets: Vec<SheetSource>,
    /// `gMonIconTable` as a sheet number per species slot.
    pub(crate) species_sheets: Vec<u16>,
    /// `gMonIconPaletteIndices`, one bank per species slot.
    pub(crate) palette_indices: Vec<u8>,
}

/// Resolve the inventory from the four source files' text.
///
/// # Errors
///
/// A description of the first thing the sources do not hold in the shape the
/// pack schema transcribes.
pub(crate) fn parse(
    species_constants: &str,
    icon_table_source: &str,
    declarations: &[&str],
) -> Result<Inventory, String> {
    let numbers = species_numbers(species_constants)?;
    let paths = declared_paths(declarations)?;

    let symbols = table_rows(icon_table_source, SHEET_TABLE, &numbers)?;
    let mut order: Vec<&str> = Vec::new();
    let mut species_sheets = Vec::with_capacity(SPECIES_SLOTS);
    for (slot, value) in symbols.iter().enumerate() {
        let symbol = value
            .strip_prefix("gMonIcon_")
            .ok_or_else(|| format!("species {slot} names `{value}`, not a `gMonIcon_` symbol"))?;
        let sheet = if let Some(sheet) = order.iter().position(|seen| *seen == symbol) {
            sheet
        } else {
            order.push(symbol);
            order.len() - 1
        };
        species_sheets.push(u16::try_from(sheet).map_err(|_| "too many sheets".to_owned())?);
    }
    if order.len() != usize::from(TABLE_SHEETS) {
        return Err(format!(
            "`gMonIconTable` names {} distinct sheets: expected {TABLE_SHEETS}",
            order.len()
        ));
    }
    order.push(SPEED_SYMBOL);
    order.push(SPEED_WIDE_SYMBOL);
    debug_assert_eq!(order.len(), usize::from(SHEETS));
    let sheets = order
        .into_iter()
        .map(|symbol| {
            let png = paths
                .get(symbol)
                .ok_or_else(|| format!("icon symbol `gMonIcon_{symbol}` has no declaration"))?;
            Ok(SheetSource {
                symbol: symbol.to_owned(),
                png: (*png).to_owned(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    let indices = table_rows(icon_table_source, PALETTE_INDEX_TABLE, &numbers)?;
    let palette_indices = indices
        .iter()
        .enumerate()
        .map(|(slot, value)| {
            value
                .parse::<u8>()
                .map_err(|_| format!("species {slot} names palette `{value}`: expected a number"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Inventory {
        sheets,
        species_sheets,
        palette_indices,
    })
}

/// `symbol -> png` for every `gMonIcon_` declaration, each of the form
/// `const u8 gMonIcon_<symbol>[] = INCGFX_U8("<png>", ".4bpp");`.
fn declared_paths<'a>(sources: &[&'a str]) -> Result<BTreeMap<&'a str, &'a str>, String> {
    let mut paths = BTreeMap::new();
    for line in sources.iter().flat_map(|source| source.lines()) {
        let Some(rest) = line.strip_prefix(ICON_DECLARATION) else {
            continue;
        };
        let (symbol, rest) = rest
            .split_once("[] = INCGFX_U8(\"")
            .ok_or_else(|| format!("unsupported icon declaration `{line}`"))?;
        let (path, rest) = rest
            .split_once("\", \".4bpp\");")
            .ok_or_else(|| format!("unsupported icon declaration `{line}`"))?;
        let comment = rest.trim();
        if !comment.is_empty() && !comment.starts_with("//") {
            return Err(format!("unsupported icon declaration `{line}`"));
        }
        if symbol.is_empty() || symbol.contains(char::is_whitespace) {
            return Err(format!("unsupported icon declaration `{line}`"));
        }
        if !path.starts_with(ICON_DIRECTORY) || !is_png(path) {
            return Err(format!(
                "icon symbol `gMonIcon_{symbol}` names `{path}`: expected {ICON_DIRECTORY}*.png"
            ));
        }
        if paths.insert(symbol, path).is_some() {
            return Err(format!("`gMonIcon_{symbol}` is declared twice"));
        }
    }
    Ok(paths)
}

fn is_png(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .is_some_and(|extension| extension == "png")
}

/// The value of each `[SPECIES_X] = <value>,` row of the table named `table`,
/// in species order. The table must hold exactly rows `0..SPECIES_SLOTS`.
fn table_rows(
    source: &str,
    table: &str,
    numbers: &BTreeMap<String, u32>,
) -> Result<Vec<String>, String> {
    let (_, declaration) = source
        .split_once(table)
        .ok_or_else(|| format!("`{table}` is missing"))?;
    let (_, body) = declaration
        .split_once('{')
        .ok_or_else(|| format!("`{table}` has no initializer"))?;
    let (body, _) = body
        .split_once("};")
        .ok_or_else(|| format!("`{table}` is unterminated"))?;
    let mut rows: BTreeMap<usize, String> = BTreeMap::new();
    for line in body.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let (key, value) = line
            .strip_prefix('[')
            .and_then(|l| l.split_once("] = "))
            .ok_or_else(|| format!("unsupported `{table}` row `{line}`"))?;
        let value = value
            .strip_suffix(',')
            .ok_or_else(|| format!("unsupported `{table}` row `{line}`"))?;
        let index = usize::try_from(
            *numbers
                .get(key)
                .ok_or_else(|| format!("unknown species constant `{key}`"))?,
        )
        .map_err(|_| format!("species constant `{key}` is too large"))?;
        if rows.insert(index, value.to_owned()).is_some() {
            return Err(format!("`{table}` row `{key}` appears twice"));
        }
    }
    if rows.len() != SPECIES_SLOTS || !(0..SPECIES_SLOTS).all(|slot| rows.contains_key(&slot)) {
        return Err(format!(
            "`{table}` has {} rows: expected exactly rows 0..{SPECIES_SLOTS}",
            rows.len()
        ));
    }
    Ok(rows.into_values().collect())
}

/// Every `#define NAME <expr>` of the species header as a number. An
/// expression is a number, another definition, or a parenthesised sum of
/// those; anything else is an error when its name is needed, never a guess.
fn species_numbers(header: &str) -> Result<BTreeMap<String, u32>, String> {
    let mut definitions: BTreeMap<&str, &str> = BTreeMap::new();
    for line in header.lines() {
        let Some(rest) = line.strip_prefix("#define ") else {
            continue;
        };
        let Some((name, expression)) = rest.split_once(' ') else {
            continue;
        };
        if !is_species_name(name) {
            continue;
        }
        if definitions.insert(name, expression.trim()).is_some() {
            return Err(format!("`{name}` is defined twice"));
        }
    }
    let mut numbers = BTreeMap::new();
    for name in definitions.keys() {
        let value = evaluate(name, &definitions, &mut BTreeSet::new())?;
        numbers.insert((*name).to_owned(), value);
    }
    Ok(numbers)
}

fn is_species_name(name: &str) -> bool {
    name.starts_with("SPECIES_") || name == "NUM_SPECIES"
}

fn evaluate(
    name: &str,
    definitions: &BTreeMap<&str, &str>,
    visiting: &mut BTreeSet<String>,
) -> Result<u32, String> {
    let expression = definitions
        .get(name)
        .ok_or_else(|| format!("`{name}` is not defined"))?;
    if !visiting.insert(name.to_owned()) {
        return Err(format!("`{name}` is defined in terms of itself"));
    }
    let inner = expression
        .strip_prefix('(')
        .and_then(|e| e.strip_suffix(')'))
        .unwrap_or(expression);
    let mut total: u32 = 0;
    for term in inner.split(" + ") {
        let term = term.trim();
        let value = if let Ok(number) = term.parse::<u32>() {
            number
        } else if is_species_name(term) {
            evaluate(term, definitions, visiting)?
        } else {
            return Err(format!("`{name}` uses unsupported term `{term}`"));
        };
        total = total
            .checked_add(value)
            .ok_or_else(|| format!("`{name}` overflows"))?;
    }
    visiting.remove(name);
    Ok(total)
}

#[cfg(test)]
#[path = "source_tests.rs"]
mod tests;
