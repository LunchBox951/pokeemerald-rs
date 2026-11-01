//! Battle-effect sprite sheets and palettes (S-4).
//!
//! `Cmd_loadspritegfx` loads one sheet from `gBattleAnimPicTable` and one
//! palette from `gBattleAnimPaletteTable` per `ANIM_TAG_*`
//! (`src/battle_anim.c:340-341`, `src/data/battle_anim.h:1010,1303`). This
//! extractor emits that pair, as `battle-effect/<NNN>/{image,palette}`, for
//! each record [`BattleEffectRecord::inventory`] derives from the catalog; no
//! unreferenced sheet is imported for sharing `graphics/battle_anims`.
//!
//! Both tables are joined to the tag by its constant
//! (`include/constants/battle_anim.h`), never by row position, and each row's
//! `gBattleAnimSpriteGfx_*` / `gBattleAnimSpritePal_*` symbol to its
//! `INCGFX_U32` declaration in `src/graphics.c`. Only the selected
//! declarations and the files they name are read. A tag keeps its own
//! entries even where upstream shares a payload (`gBattleAnimSpriteGfx_GlowyRedOrb`
//! backs three tags), and the palette is the one the palette table selects,
//! not the sheet's own.
//!
//! A sheet declared as one `.png` keeps that PNG's dimensions, indexed raster
//! and bit depth. Five sheets are declared as `.4bpp` files that
//! `graphics_file_rules.mk` builds by concatenating several PNGs' tile streams;
//! their parts differ in width, so the pack image is one 8-pixel-wide column of
//! tiles in stream order (`COMPOSITES`). A palette declared as a `.png` is that
//! PNG's `PLTE`; one declared as a `.pal` is the JASC file.
//!
//! The registered size is checked only against a composite's tile stream:
//! three single-PNG sheets (`bent_spoon`, `eye_sparkle`, `sparkle_6`) are
//! registered with a size other than their PNG's 4bpp byte count, and the
//! engine loads the registered size, so the PNG stays whole.
//!
//! Nothing is queued unless every record's pair is built.

#[cfg(test)]
mod tests;

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use assets::pack::BattleEffectRecord;
use assets::{BattleAnimationCatalog, EffectSpriteId};
use pack_format::PackWriter;

use super::jasc_pal::{self, Rgb888};
use super::png::{self, IndexedImage};
use super::{build_image_entry, build_palette_entry, read_file, read_text, ExtractError};

const CONSTANTS_PATH: &str = "include/constants/battle_anim.h";
const TABLES_PATH: &str = "src/data/battle_anim.h";
const DECLARATIONS_PATH: &str = "src/graphics.c";
const PIC_TABLE: &str = "gBattleAnimPicTable";
const PALETTE_TABLE: &str = "gBattleAnimPaletteTable";
const GFX_PREFIX: &str = "gBattleAnimSpriteGfx_";
const PAL_PREFIX: &str = "gBattleAnimSpritePal_";
/// Where every declared source lives.
const SOURCE_ROOT: &str = "graphics/battle_anims/";
/// Every sheet is `*.4bpp.lz` (`src/data/battle_anim.h`).
const BIT_DEPTH: u8 = 4;
const TILE_SIDE: u32 = 8;
/// A 4bpp palette bank.
const BANK_COLORS: usize = 16;

/// The sheets declared as a generated `.4bpp`, with the PNGs
/// `graphics_file_rules.mk:64-68,114-131` concatenates into each, in rule order.
const COMPOSITES: [(&str, &[&str]); 5] = [
    (
        "graphics/battle_anims/sprites/ice_cube.4bpp",
        &[
            "graphics/battle_anims/sprites/ice_cube_0.png",
            "graphics/battle_anims/sprites/ice_cube_1.png",
            "graphics/battle_anims/sprites/ice_cube_2.png",
            "graphics/battle_anims/sprites/ice_cube_3.png",
        ],
    ),
    (
        "graphics/battle_anims/sprites/ice_crystals.4bpp",
        &[
            "graphics/battle_anims/sprites/ice_crystals_0.png",
            "graphics/battle_anims/sprites/ice_crystals_1.png",
            "graphics/battle_anims/sprites/ice_crystals_2.png",
            "graphics/battle_anims/sprites/ice_crystals_3.png",
            "graphics/battle_anims/sprites/ice_crystals_4.png",
        ],
    ),
    (
        "graphics/battle_anims/sprites/mud_sand.4bpp",
        &[
            "graphics/battle_anims/sprites/mud_sand_0.png",
            "graphics/battle_anims/sprites/mud_sand_1.png",
        ],
    ),
    (
        "graphics/battle_anims/sprites/flower.4bpp",
        &[
            "graphics/battle_anims/sprites/flower_0.png",
            "graphics/battle_anims/sprites/flower_1.png",
        ],
    ),
    (
        "graphics/battle_anims/sprites/spark.4bpp",
        &[
            "graphics/battle_anims/sprites/spark_0.png",
            "graphics/battle_anims/sprites/spark_1.png",
        ],
    ),
];

/// Extracts the sheet and palette of every battle-effect record `catalog`
/// requires, in record order.
pub(super) fn extract_battle_effects(
    upstream: &Path,
    catalog: &BattleAnimationCatalog,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let records: Vec<BattleEffectRecord> = BattleEffectRecord::inventory(catalog).collect();

    let tags = parse_tags(upstream)?;
    let tables_path = upstream.join(TABLES_PATH);
    let tables = read_text(&tables_path)?;
    let pictures = parse_table(&tables_path, &tables, PIC_TABLE, &tags)?;
    let palettes = parse_table(&tables_path, &tables, PALETTE_TABLE, &tags)?;

    let mut selected = Vec::with_capacity(records.len());
    for record in &records {
        let picture = select(&tables_path, &pictures, PIC_TABLE, record.sprite())?;
        let palette = select(&tables_path, &palettes, PALETTE_TABLE, record.sprite())?;
        selected.push((record, picture, palette));
    }

    let symbols: BTreeSet<&str> = selected
        .iter()
        .flat_map(|(_, picture, palette)| [picture.symbol.as_str(), palette.symbol.as_str()])
        .collect();
    let declarations_path = upstream.join(DECLARATIONS_PATH);
    let declarations = parse_declarations(
        &declarations_path,
        &read_text(&declarations_path)?,
        &symbols,
    )?;

    let mut entries = Vec::with_capacity(2 * selected.len());
    for (record, picture, palette) in selected {
        let gfx = declaration(&declarations_path, &declarations, picture)?;
        let pal = declaration(&declarations_path, &declarations, palette)?;
        let (image_path, image) = decode_sheet(upstream, &declarations_path, gfx, picture)?;
        let (palette_path, colors) = decode_palette(upstream, &declarations_path, pal)?;
        check_pixels(&image_path, &image, colors.len())?;
        entries.push(build_image_entry(&image_path, record.image_id(), image)?);
        entries.push(build_palette_entry(
            &palette_path,
            &colors,
            record.palette_id(),
        )?);
    }
    for entry in entries {
        writer.push(entry);
    }
    Ok(())
}

fn source_fault(path: &Path, line: usize, detail: String) -> ExtractError {
    ExtractError::BattleEffectSource {
        path: path.to_path_buf(),
        line,
        detail,
    }
}

fn mismatch(path: &Path, detail: String) -> ExtractError {
    ExtractError::BattleEffectAssetMismatch {
        path: path.to_path_buf(),
        detail,
    }
}

/// The text before a `//` comment, trimmed.
fn code(line: &str) -> &str {
    line.split("//").next().unwrap_or("").trim()
}

/// A C integer literal, decimal or `0x` hexadecimal.
fn number(text: &str) -> Option<u32> {
    let text = text.trim();
    match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        Some(hex) => u32::from_str_radix(hex, 16).ok(),
        None => text.parse().ok(),
    }
}

/// Every `ANIM_TAG_*` and its effect sprite index.
///
/// The header defines each as `(ANIM_SPRITES_START + <index>)`, so the index
/// `GET_TRUE_SPRITE_INDEX` yields is the offset.
fn parse_tags(upstream: &Path) -> Result<HashMap<String, EffectSpriteId>, ExtractError> {
    let path = upstream.join(CONSTANTS_PATH);
    let text = read_text(&path)?;
    let mut base_defined = false;
    let mut tags = HashMap::new();
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let Some(define) = code(raw).strip_prefix("#define") else {
            continue;
        };
        let mut words = define.trim().splitn(2, char::is_whitespace);
        let (name, value) = (words.next().unwrap_or(""), words.next().unwrap_or(""));
        if name == "ANIM_SPRITES_START" {
            base_defined = number(value).is_some();
            if !base_defined {
                return Err(source_fault(
                    &path,
                    line,
                    format!(
                        "ANIM_SPRITES_START is `{}`: expected an integer",
                        value.trim()
                    ),
                ));
            }
        } else if name.starts_with("ANIM_TAG_") {
            let offset = value
                .trim()
                .strip_prefix('(')
                .and_then(|rest| rest.strip_suffix(')'))
                .and_then(|inner| inner.split_once('+'))
                .filter(|(base, _)| base.trim() == "ANIM_SPRITES_START")
                .and_then(|(_, offset)| number(offset))
                .and_then(|offset| u16::try_from(offset).ok())
                .and_then(|offset| EffectSpriteId::new(offset).ok())
                .ok_or_else(|| {
                    source_fault(
                        &path,
                        line,
                        format!(
                            "{name} is `{}`: expected `(ANIM_SPRITES_START + <index>)` \
                             within the effect sprite range",
                            value.trim()
                        ),
                    )
                })?;
            if tags.insert(name.to_owned(), offset).is_some() {
                return Err(source_fault(
                    &path,
                    line,
                    format!("{name} is defined twice"),
                ));
            }
        }
    }
    if !base_defined {
        return Err(source_fault(
            &path,
            0,
            "ANIM_SPRITES_START is not defined".to_owned(),
        ));
    }
    Ok(tags)
}

/// One registration row of a table.
#[derive(Debug)]
struct Row {
    line: usize,
    /// The `gBattleAnimSpriteGfx_*` or `gBattleAnimSpritePal_*` symbol.
    symbol: String,
    /// The registered size in bytes; the palette table has none.
    size: Option<u32>,
    tag: String,
    sprite: EffectSpriteId,
}

/// Parses every row of `array`, one `{symbol, [size,] ANIM_TAG_*},` per line.
fn parse_table(
    path: &Path,
    text: &str,
    array: &str,
    tags: &HashMap<String, EffectSpriteId>,
) -> Result<Vec<Row>, ExtractError> {
    let (prefix, columns) = if array == PIC_TABLE {
        (GFX_PREFIX, 3)
    } else {
        (PAL_PREFIX, 2)
    };
    let heading = format!("{array}[]");
    let mut lines = text.lines().enumerate();
    lines
        .by_ref()
        .find(|(_, line)| line.starts_with("const struct") && line.contains(&heading));
    if lines.next().map(|(_, line)| line.trim()) != Some("{") {
        return Err(source_fault(
            path,
            0,
            format!("`{array}[] =` followed by `{{` was not found"),
        ));
    }
    let mut rows = Vec::new();
    for (index, raw) in lines {
        let (line, row) = (index + 1, code(raw));
        if row.is_empty() {
            continue;
        }
        if row == "};" {
            return Ok(rows);
        }
        let fields: Vec<&str> = row
            .strip_prefix('{')
            .and_then(|inner| inner.strip_suffix("},").or_else(|| inner.strip_suffix('}')))
            .map(|inner| inner.split(',').map(str::trim).collect())
            .unwrap_or_default();
        let malformed = || {
            source_fault(
                path,
                line,
                format!("{array} row `{row}`: expected {columns} comma-separated fields in braces"),
            )
        };
        if fields.len() != columns || !fields[0].starts_with(prefix) {
            return Err(malformed());
        }
        let size = if columns == 3 {
            Some(number(fields[1]).ok_or_else(|| {
                source_fault(
                    path,
                    line,
                    format!(
                        "{array} row `{row}`: size `{}` is not an integer",
                        fields[1]
                    ),
                )
            })?)
        } else {
            None
        };
        let tag = fields[columns - 1];
        let sprite = *tags.get(tag).ok_or_else(|| {
            source_fault(
                path,
                line,
                format!("{array} row names `{tag}`, which {CONSTANTS_PATH} does not define"),
            )
        })?;
        rows.push(Row {
            line,
            symbol: fields[0].to_owned(),
            size,
            tag: tag.to_owned(),
            sprite,
        });
    }
    Err(source_fault(
        path,
        0,
        format!("{array} is not closed by `}};`"),
    ))
}

/// The single row registering `sprite`.
fn select<'a>(
    path: &Path,
    rows: &'a [Row],
    table: &str,
    sprite: EffectSpriteId,
) -> Result<&'a Row, ExtractError> {
    let mut matching = rows.iter().filter(|row| row.sprite == sprite);
    let first = matching.next().ok_or_else(|| {
        source_fault(
            path,
            0,
            format!("{table} has no row for effect sprite {}", sprite.get()),
        )
    })?;
    if let Some(second) = matching.next() {
        return Err(source_fault(
            path,
            second.line,
            format!(
                "{table} registers `{}` twice (lines {} and {})",
                first.tag, first.line, second.line
            ),
        ));
    }
    Ok(first)
}

/// One `const u32 <symbol>[] = INCGFX_U32("<path>", "<suffix>");` line.
#[derive(Debug)]
struct Declaration {
    line: usize,
    path: String,
    suffix: String,
}

/// The declarations of `symbols`, each of which must appear exactly once.
/// Every other line of the file is ignored.
fn parse_declarations(
    path: &Path,
    text: &str,
    symbols: &BTreeSet<&str>,
) -> Result<HashMap<String, Declaration>, ExtractError> {
    let mut found = HashMap::new();
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let Some(rest) = raw.strip_prefix("const u32 ") else {
            continue;
        };
        let Some((name, rest)) = rest.split_once("[]") else {
            continue;
        };
        if !symbols.contains(name) {
            continue;
        }
        let parts: Vec<&str> = rest.split('"').collect();
        let well_formed = parts.len() == 5
            && parts[0].trim().strip_prefix('=').map(str::trim) == Some("INCGFX_U32(")
            && parts[2].trim() == ","
            && parts[4].trim() == ");";
        if !well_formed {
            return Err(source_fault(
                path,
                line,
                format!("`{name}` is not `INCGFX_U32(\"<path>\", \"<suffix>\");`"),
            ));
        }
        let declaration = Declaration {
            line,
            path: parts[1].to_owned(),
            suffix: parts[3].to_owned(),
        };
        if let Some(first) = found.insert(name.to_owned(), declaration) {
            return Err(source_fault(
                path,
                line,
                format!("`{name}` is declared twice (first at line {})", first.line),
            ));
        }
    }
    Ok(found)
}

fn declaration<'a>(
    path: &Path,
    declarations: &'a HashMap<String, Declaration>,
    row: &Row,
) -> Result<&'a Declaration, ExtractError> {
    declarations.get(&row.symbol).ok_or_else(|| {
        source_fault(
            path,
            0,
            format!(
                "`{}` (registered at line {} of {TABLES_PATH}) is not declared",
                row.symbol, row.line
            ),
        )
    })
}

/// Rejects a declared source outside `graphics/battle_anims`.
fn check_source_path(
    declarations_path: &Path,
    declaration: &Declaration,
) -> Result<(), ExtractError> {
    let path = declaration.path.as_str();
    if path.starts_with(SOURCE_ROOT) && !path.split('/').any(|part| part == ".." || part.is_empty())
    {
        return Ok(());
    }
    Err(source_fault(
        declarations_path,
        declaration.line,
        format!("source `{path}` is not a path under {SOURCE_ROOT}"),
    ))
}

/// Decodes the sheet `declaration` names, validated as a 4-bit whole-tile image.
fn decode_sheet(
    upstream: &Path,
    declarations_path: &Path,
    declaration: &Declaration,
    row: &Row,
) -> Result<(PathBuf, IndexedImage), ExtractError> {
    check_source_path(declarations_path, declaration)?;
    let path = upstream.join(&declaration.path);
    match (declaration.path.as_str(), declaration.suffix.as_str()) {
        (png_path, ".4bpp.lz") if png_path.strip_suffix(".png").is_some() => {
            let image = read_sheet(&path)?;
            Ok((path, image))
        }
        (stream, ".lz") if stream.strip_suffix(".4bpp").is_some() => {
            let parts = COMPOSITES
                .iter()
                .find(|(name, _)| *name == stream)
                .map(|(_, parts)| *parts)
                .ok_or_else(|| {
                    source_fault(
                        declarations_path,
                        declaration.line,
                        format!("`{stream}` is not a known concatenated sheet"),
                    )
                })?;
            let image = concatenate(upstream, parts)?;
            if let Some(size) = row
                .size
                .filter(|&size| image.pixels.len() != 2 * size as usize)
            {
                return Err(mismatch(
                    &path,
                    format!(
                        "concatenates to {} bytes of 4bpp tiles: {PIC_TABLE} registers {size}",
                        image.pixels.len() / 2
                    ),
                ));
            }
            Ok((path, image))
        }
        (source, suffix) => Err(source_fault(
            declarations_path,
            declaration.line,
            format!("`{source}` with suffix `{suffix}` is not a supported sheet declaration"),
        )),
    }
}

/// Reads one PNG sheet and checks it is a whole-tile 4-bit image.
fn read_sheet(path: &Path) -> Result<IndexedImage, ExtractError> {
    let image =
        png::decode(&read_file(path)?).map_err(|e| ExtractError::Png(path.to_path_buf(), e))?;
    if image.bit_depth != BIT_DEPTH {
        return Err(mismatch(
            path,
            format!("is {}-bit: expected {BIT_DEPTH}-bit", image.bit_depth),
        ));
    }
    let whole_tiles = |side: u32| side != 0 && side.is_multiple_of(TILE_SIDE);
    if !whole_tiles(image.width) || !whole_tiles(image.height) {
        return Err(mismatch(
            path,
            format!(
                "is {}x{}: expected whole {TILE_SIDE}x{TILE_SIDE} tiles",
                image.width, image.height
            ),
        ));
    }
    Ok(image)
}

/// The tile stream of `parts` one after another, as one tile-wide column.
///
/// `gbagfx` writes each PNG's tiles row-major with the left pixel low, and
/// `cat` joins the files, so a part's tiles are copied in that order whatever
/// its width.
fn concatenate(upstream: &Path, parts: &[&str]) -> Result<IndexedImage, ExtractError> {
    let side = TILE_SIDE as usize;
    let mut pixels = Vec::new();
    for part in parts {
        let image = read_sheet(&upstream.join(part))?;
        let width = image.width as usize;
        for tile_y in 0..image.height as usize / side {
            for tile_x in 0..width / side {
                for row in 0..side {
                    let start = (tile_y * side + row) * width + tile_x * side;
                    pixels.extend_from_slice(&image.pixels[start..start + side]);
                }
            }
        }
    }
    let height = u32::try_from(pixels.len() / side).expect("a few KiB of tiles");
    Ok(IndexedImage {
        width: TILE_SIDE,
        height,
        bit_depth: BIT_DEPTH,
        pixels,
        palette: Vec::new(),
    })
}

/// Decodes the palette `declaration` names: a PNG's `PLTE` or a JASC file,
/// a whole number of 16-colour banks.
fn decode_palette(
    upstream: &Path,
    declarations_path: &Path,
    declaration: &Declaration,
) -> Result<(PathBuf, Vec<Rgb888>), ExtractError> {
    check_source_path(declarations_path, declaration)?;
    let path = upstream.join(&declaration.path);
    let colors = match (declaration.path.as_str(), declaration.suffix.as_str()) {
        (png_path, ".gbapal.lz") if png_path.strip_suffix(".png").is_some() => {
            png::decode_palette(&read_file(&path)?)
                .map_err(|e| ExtractError::Png(path.clone(), e))?
        }
        (pal_path, ".gbapal.lz") if pal_path.strip_suffix(".pal").is_some() => {
            jasc_pal::parse(&read_text(&path)?).map_err(|e| ExtractError::Pal(path.clone(), e))?
        }
        (source, suffix) => {
            return Err(source_fault(
                declarations_path,
                declaration.line,
                format!("`{source}` with suffix `{suffix}` is not a supported palette declaration"),
            ))
        }
    };
    if colors.is_empty() || !colors.len().is_multiple_of(BANK_COLORS) {
        return Err(mismatch(
            &path,
            format!(
                "has {} colours: expected a whole number of {BANK_COLORS}-colour banks",
                colors.len()
            ),
        ));
    }
    Ok((path, colors))
}

/// Rejects a pixel the sheet's palette cannot colour.
fn check_pixels(path: &Path, image: &IndexedImage, colors: usize) -> Result<(), ExtractError> {
    match image
        .pixels
        .iter()
        .find(|&&pixel| usize::from(pixel) >= colors || pixel >= 1 << BIT_DEPTH)
    {
        Some(pixel) => Err(mismatch(
            path,
            format!("has pixel index {pixel}: its palette has {colors} colours"),
        )),
        None => Ok(()),
    }
}
