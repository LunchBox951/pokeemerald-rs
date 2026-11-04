//! What the checkout registers for each selected effect sprite.
//!
//! `gBattleAnimPicTable` registers a sheet's symbol, byte size and tag and
//! `gBattleAnimPaletteTable` a palette's symbol and tag (`src/data/battle_anim.h`).
//! The pack keeps neither: it holds each sheet whole, and the size the engine
//! loads can differ from the PNG's (`ANIM_TAG_BENT_SPOON`). This reads only
//! the selected rows, as the expectation the ROM's tables are held to; it
//! supplies no address.
//!
//! The ROM tables are indexed by `tag - ANIM_SPRITES_START`
//! (`GET_TRUE_SPRITE_INDEX`), so a selected row is refused unless the checkout
//! puts it at that position under that tag.

use std::collections::BTreeMap;
use std::path::Path;

use crate::gen_rom_profile::error::GenRomProfileError;

const CONSTANTS_PATH: &str = "include/constants/battle_anim.h";
const TABLES_PATH: &str = "src/data/battle_anim.h";
const SHEET_TABLE: &str = "gBattleAnimPicTable";
const PALETTE_TABLE: &str = "gBattleAnimPaletteTable";
/// `ANIM_SPRITES_START`, which a tag is offset by.
pub(super) const SPRITES_START: u16 = 10_000;

/// One selected sprite's registration rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Registration {
    /// The `gBattleAnimSpriteGfx_*` symbol the sheet row names.
    pub sheet_symbol: String,
    /// The `size` the sheet row registers.
    pub sheet_bytes: u16,
    /// The `gBattleAnimSpritePal_*` symbol the palette row names.
    pub palette_symbol: String,
}

/// The registration of each of `sprites`, by sprite index.
///
/// # Errors
///
/// [`GenRomProfileError::UpstreamSource`] if a source cannot be read, or a
/// selected sprite has no row at its own index in either table, or its row
/// names another tag or has no usable symbol or size.
pub(super) fn load(
    upstream: &Path,
    sprites: &[u16],
) -> Result<BTreeMap<u16, Registration>, GenRomProfileError> {
    let constants_path = upstream.join(CONSTANTS_PATH);
    let tags = parse_tags(&constants_path, &read(&constants_path)?)?;
    let tables_path = upstream.join(TABLES_PATH);
    let tables = read(&tables_path)?;
    let sheets = parse_table(&tables_path, &tables, SHEET_TABLE, &tags)?;
    let palettes = parse_table(&tables_path, &tables, PALETTE_TABLE, &tags)?;

    let mut registrations = BTreeMap::new();
    for &sprite in sprites {
        let sheet = row_of(&tables_path, SHEET_TABLE, &sheets, sprite)?;
        let palette = row_of(&tables_path, PALETTE_TABLE, &palettes, sprite)?;
        let sheet_bytes = sheet.size.ok_or_else(|| {
            fault(
                &tables_path,
                format!("{SHEET_TABLE} row {sprite} registers no size"),
            )
        })?;
        registrations.insert(
            sprite,
            Registration {
                sheet_symbol: sheet.symbol.clone(),
                sheet_bytes,
                palette_symbol: palette.symbol.clone(),
            },
        );
    }
    Ok(registrations)
}

fn fault(path: &Path, reason: String) -> GenRomProfileError {
    GenRomProfileError::UpstreamSource {
        path: path.to_path_buf(),
        reason,
    }
}

fn read(path: &Path) -> Result<String, GenRomProfileError> {
    std::fs::read_to_string(path).map_err(|err| fault(path, err.to_string()))
}

/// The text before a `//` comment, trimmed.
fn code(line: &str) -> &str {
    line.split("//").next().unwrap_or("").trim()
}

/// A C integer literal, decimal or `0x` hexadecimal.
fn number(text: &str) -> Option<u16> {
    let text = text.trim();
    match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        Some(hex) => u16::from_str_radix(hex, 16).ok(),
        None => text.parse().ok(),
    }
}

/// Every `ANIM_TAG_*` and its offset from `ANIM_SPRITES_START`, which must be
/// [`SPRITES_START`].
fn parse_tags(path: &Path, text: &str) -> Result<BTreeMap<String, u16>, GenRomProfileError> {
    let mut start = None;
    let mut tags = BTreeMap::new();
    for raw in text.lines() {
        let Some(define) = code(raw).strip_prefix("#define") else {
            continue;
        };
        let mut words = define.trim().splitn(2, char::is_whitespace);
        let (name, value) = (words.next().unwrap_or(""), words.next().unwrap_or(""));
        if name == "ANIM_SPRITES_START" {
            start = number(value);
        } else if name.starts_with("ANIM_TAG_") {
            let offset = value
                .trim()
                .strip_prefix('(')
                .and_then(|rest| rest.strip_suffix(')'))
                .and_then(|inner| inner.split_once('+'))
                .filter(|(base, _)| base.trim() == "ANIM_SPRITES_START")
                .and_then(|(_, offset)| number(offset))
                .ok_or_else(|| {
                    fault(
                        path,
                        format!(
                            "{name} is `{}`: expected `(ANIM_SPRITES_START + <index>)`",
                            value.trim()
                        ),
                    )
                })?;
            tags.insert(name.to_owned(), offset);
        }
    }
    if start != Some(SPRITES_START) {
        return Err(fault(
            path,
            format!("ANIM_SPRITES_START is {start:?}: expected {SPRITES_START}"),
        ));
    }
    Ok(tags)
}

/// One row of a registration table.
#[derive(Debug)]
struct Row {
    symbol: String,
    /// The registered size; the palette table has none.
    size: Option<u16>,
    /// The tag's offset from `ANIM_SPRITES_START`.
    tag: u16,
}

/// Every row of `array`, in table order. A row is `{symbol, [size,] ANIM_TAG_*},`.
fn parse_table(
    path: &Path,
    text: &str,
    array: &str,
    tags: &BTreeMap<String, u16>,
) -> Result<Vec<Row>, GenRomProfileError> {
    let columns = if array == SHEET_TABLE { 3 } else { 2 };
    let heading = format!("{array}[]");
    let mut lines = text.lines();
    lines
        .by_ref()
        .find(|line| line.starts_with("const struct") && line.contains(&heading));
    if lines.next().map(str::trim) != Some("{") {
        return Err(fault(
            path,
            format!("`{array}[] =` followed by `{{` was not found"),
        ));
    }
    let mut rows = Vec::new();
    for raw in lines {
        let line = code(raw);
        if line.is_empty() {
            continue;
        }
        if line == "};" {
            return Ok(rows);
        }
        let cells: Vec<&str> = line
            .strip_prefix('{')
            .and_then(|rest| rest.strip_suffix("},"))
            .map(|inner| inner.split(',').map(str::trim).collect())
            .filter(|cells: &Vec<&str>| cells.len() == columns)
            .ok_or_else(|| {
                fault(
                    path,
                    format!(
                        "{array} row {} is `{line}`: expected {columns} cells",
                        rows.len()
                    ),
                )
            })?;
        let tag_name = cells[columns - 1];
        let tag = tags.get(tag_name).copied().ok_or_else(|| {
            fault(
                path,
                format!(
                    "{array} row {} names the unknown tag `{tag_name}`",
                    rows.len()
                ),
            )
        })?;
        let size = if columns == 3 {
            Some(number(cells[1]).ok_or_else(|| {
                fault(
                    path,
                    format!(
                        "{array} row {} size `{}` is not a u16",
                        rows.len(),
                        cells[1]
                    ),
                )
            })?)
        } else {
            None
        };
        rows.push(Row {
            symbol: cells[0].to_owned(),
            size,
            tag,
        });
    }
    Err(fault(path, format!("{array} is not closed by `}};`")))
}

/// Row `sprite` of a table, which must carry that sprite's own tag.
fn row_of<'a>(
    path: &Path,
    array: &str,
    rows: &'a [Row],
    sprite: u16,
) -> Result<&'a Row, GenRomProfileError> {
    let row = rows
        .get(usize::from(sprite))
        .ok_or_else(|| fault(path, format!("{array} has no row {sprite}")))?;
    if row.tag != sprite {
        return Err(fault(
            path,
            format!(
                "{array} row {sprite} names the tag of sprite {}: the engine indexes by tag",
                row.tag
            ),
        ));
    }
    Ok(row)
}
