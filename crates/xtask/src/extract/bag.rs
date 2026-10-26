//! The ordinary bag's fixed graphics manifest and the item-icon closure.
//!
//! The bag screen, bag animation sheets, rotating ball, select and HM marks
//! come from `src/graphics.c`, `src/item_menu_icons.c`, and `src/item_menu.c`.
//! The item icons are exactly the images and palettes `gItemIconTable`
//! (`src/data/item_icon_table.h`) selects for `0..=ITEMS_COUNT`, resolved
//! through `src/data/graphics/items.h`. Upstream shares sheets and palettes
//! across items, so the two sets are deduplicated independently.
//!
//! The list-menu scroll arrows (`graphics/interface/scroll_indicator.png` with
//! `red.pal`, `src/list_menu.c:289-290`) and the swap line
//! (`graphics/interface/swap_line.png`, `gSwapLineGfx`/`gSwapLinePal`,
//! `src/graphics.c:1151-1152`, whose palette is the PNG's own) are the two
//! sprite sheets the bag screen draws beyond its own directory.
//!
//! The menu-info icon sheet (`graphics/interface/menu_info.png`,
//! `gMenuInfoElements_Gfx`) and its three palettes (`menu_info{1,2,3}.pal`,
//! `gMenuInfoElements{1,2,3}_Pal`, `src/graphics.c:1264-1267`) hold the type
//! and TYPE/POWER/ACCURACY/PP label icons the TM/HM move-data window blits
//! (`src/menu.c` `BlitMenuInfoIcon`).
//!
//! Ids are `bag/{image,palette,raw}/<name>` and
//! `bag/item-icon/{image,palette}/<stem>`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::{build_image_entry, build_palette_entry, jasc_pal, png, push_raw_bytes};
use super::{read_file, read_text, ExtractError};
use pack_format::PackWriter;

const BAG_DIRECTORY: &str = "graphics/bag";
const ICON_IMAGE_DIRECTORY: &str = "graphics/items/icons/";
const ICON_PALETTE_DIRECTORY: &str = "graphics/items/icon_palettes/";
const ITEM_CONSTANTS: &str = "include/constants/items.h";
const ICON_TABLE: &str = "src/data/item_icon_table.h";
const ICON_GRAPHICS: &str = "src/data/graphics/items.h";

const SOURCE_BIT_DEPTH: u8 = 4;
/// `menu_info.png` is an 8-bit indexed PNG over three 16-colour banks (48
/// colours). Its rule is `.4bpp`, and `gbagfx` narrows an 8-bit PNG to 4bpp by
/// keeping each index modulo 16 (`tools/gbagfx/convert_png.c` `ConvertBitDepth`),
/// so the sheet the ROM holds, and the pack stores, is that narrowed 4bpp image.
/// Keeps an index within its 16-colour bank: modulo 16.
const BANK_INDEX_MASK: u8 = 0x0F;
const MENU_INFO_PNG_BIT_DEPTH: u8 = 8;
const BANK_COLORS: usize = 16;
const MENU_COLORS: usize = 32;
const ICON_SIDE: u32 = 24;
const TILEMAP_BYTES: usize = 32 * 32 * 2;

#[derive(Clone, Copy, Debug)]
struct ImageSource {
    /// Path under the checkout root.
    png: &'static str,
    id: &'static str,
    width: u32,
    height: u32,
    /// Bit depth the PNG is stored at. An 8-bit PNG is narrowed to the 4bpp
    /// image the pack carries, as `gbagfx` does for a `.4bpp` rule.
    png_bit_depth: u8,
    /// Pack id of the PNG's embedded palette, when the pack carries it.
    embedded_palette_id: Option<&'static str>,
}

#[derive(Clone, Copy, Debug)]
struct PaletteSource {
    /// Path under the checkout root.
    pal: &'static str,
    id: &'static str,
    colors: usize,
}

const fn image(png: &'static str, id: &'static str, width: u32, height: u32) -> ImageSource {
    ImageSource {
        png,
        id,
        width,
        height,
        png_bit_depth: SOURCE_BIT_DEPTH,
        embedded_palette_id: None,
    }
}

/// `graphics.c:1136-1143,1151,1267,1269`, `list_menu.c:290`,
/// `item_menu_icons.c:36-37`, `item_menu.c:374`. `gSwapLinePal` is the PNG's own
/// palette; the menu-info sheet's three palettes are separate `.pal` files.
const IMAGES: [ImageSource; 9] = [
    image("graphics/bag/menu.png", "bag/image/menu", 128, 32),
    image("graphics/bag/bag_male.png", "bag/image/male", 64, 384),
    image("graphics/bag/bag_female.png", "bag/image/female", 64, 384),
    ImageSource {
        embedded_palette_id: Some("bag/palette/rotating_ball"),
        ..image(
            "graphics/bag/rotating_ball.png",
            "bag/image/rotating_ball",
            16,
            16,
        )
    },
    image(
        "graphics/bag/select_button.png",
        "bag/image/select_button",
        24,
        16,
    ),
    image("graphics/bag/hm.png", "bag/image/hm", 16, 16),
    image(
        "graphics/interface/scroll_indicator.png",
        "bag/image/scroll_indicator",
        16,
        32,
    ),
    ImageSource {
        embedded_palette_id: Some("bag/palette/swap_line"),
        ..image(
            "graphics/interface/swap_line.png",
            "bag/image/swap_line",
            16,
            32,
        )
    },
    ImageSource {
        png_bit_depth: MENU_INFO_PNG_BIT_DEPTH,
        ..image(
            "graphics/interface/menu_info.png",
            "bag/image/menu_info",
            128,
            128,
        )
    },
];

/// `graphics.c:1138-1141,1264-1266`, `list_menu.c:289`.
const PALETTES: [PaletteSource; 7] = [
    PaletteSource {
        pal: "graphics/bag/menu_male.pal",
        id: "bag/palette/menu/male",
        colors: MENU_COLORS,
    },
    PaletteSource {
        pal: "graphics/bag/menu_female.pal",
        id: "bag/palette/menu/female",
        colors: MENU_COLORS,
    },
    PaletteSource {
        pal: "graphics/bag/bag.pal",
        id: "bag/palette/bag",
        colors: BANK_COLORS,
    },
    PaletteSource {
        pal: "graphics/interface/red.pal",
        id: "bag/palette/scroll_indicator",
        colors: BANK_COLORS,
    },
    PaletteSource {
        pal: "graphics/interface/menu_info1.pal",
        id: "bag/palette/menu_info1",
        colors: BANK_COLORS,
    },
    PaletteSource {
        pal: "graphics/interface/menu_info2.pal",
        id: "bag/palette/menu_info2",
        colors: BANK_COLORS,
    },
    PaletteSource {
        pal: "graphics/interface/menu_info3.pal",
        id: "bag/palette/menu_info3",
        colors: BANK_COLORS,
    },
];

const TILEMAP_PATH: &str = "menu.bin";
const TILEMAP_ID: &str = "bag/raw/menu";

/// The icon image and palette source paths of one `gItemIconTable` row,
/// relative to the checkout root.
type IconRow = (String, String);

/// Every pack id [`extract_bag`] emits for the fixed manifest.
pub(crate) fn fixed_ids() -> Vec<String> {
    let embedded = IMAGES
        .iter()
        .filter_map(|source| source.embedded_palette_id);
    IMAGES
        .iter()
        .map(|source| source.id)
        .chain(embedded)
        .chain(PALETTES.iter().map(|source| source.id))
        .chain([TILEMAP_ID])
        .map(str::to_owned)
        .collect()
}

/// Every pack id [`extract_bag`] emits for the item icons, images before
/// palettes, each in id order.
///
/// # Errors
///
/// [`ExtractError`] if the checkout's icon table cannot be read.
pub(crate) fn icon_ids(upstream: &Path) -> Result<Vec<String>, ExtractError> {
    let rows = icon_rows(upstream)?;
    let images: BTreeSet<&str> = rows.iter().map(|(image, _)| image.as_str()).collect();
    let palettes: BTreeSet<&str> = rows.iter().map(|(_, palette)| palette.as_str()).collect();
    let ids = |kind: &str, paths: BTreeSet<&str>| -> Vec<String> {
        paths
            .into_iter()
            .map(|path| format!("bag/item-icon/{kind}/{}", stem(path)))
            .collect()
    };
    Ok([ids("image", images), ids("palette", palettes)].concat())
}

/// Extracts the fixed bag manifest and every icon `gItemIconTable` selects.
pub(super) fn extract_bag(upstream: &Path, writer: &mut PackWriter) -> Result<(), ExtractError> {
    for source in IMAGES {
        extract_image(&upstream.join(source.png), &source, writer)?;
    }
    for source in PALETTES {
        extract_palette(&upstream.join(source.pal), source, writer)?;
    }
    extract_tilemap_with(
        &upstream.join(BAG_DIRECTORY).join(TILEMAP_PATH),
        &mut read_file,
        writer,
    )?;

    let rows = icon_rows(upstream)?;
    let images: BTreeSet<&str> = rows.iter().map(|(image, _)| image.as_str()).collect();
    let palettes: BTreeSet<&str> = rows.iter().map(|(_, palette)| palette.as_str()).collect();
    for image_path in images {
        let id = format!("bag/item-icon/image/{}", stem(image_path));
        let shape = (ICON_SIDE, ICON_SIDE, SOURCE_BIT_DEPTH);
        decode_into(&upstream.join(image_path), id, shape, None, writer)?;
    }
    for palette_path in palettes {
        let path = upstream.join(palette_path);
        write_palette(
            &path,
            BANK_COLORS,
            format!("bag/item-icon/palette/{}", stem(palette_path)),
            writer,
        )?;
    }
    Ok(())
}

fn extract_image(
    path: &Path,
    source: &ImageSource,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let shape = (source.width, source.height, source.png_bit_depth);
    decode_into(
        path,
        source.id.to_owned(),
        shape,
        source.embedded_palette_id,
        writer,
    )
}

fn decode_into(
    path: &Path,
    id: String,
    (width, height, png_bit_depth): (u32, u32, u8),
    embedded_palette_id: Option<&str>,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let bytes = read_file(path)?;
    let mut image = png::decode(&bytes).map_err(|e| ExtractError::Png(path.to_path_buf(), e))?;
    if image.width != width || image.height != height {
        return Err(mismatch(
            path,
            format!(
                "is {}x{}: expected {width}x{height}",
                image.width, image.height
            ),
        ));
    }
    if image.bit_depth != png_bit_depth {
        return Err(mismatch(
            path,
            format!(
                "has bit depth {}: expected {png_bit_depth}",
                image.bit_depth
            ),
        ));
    }
    if png_bit_depth != SOURCE_BIT_DEPTH {
        for pixel in &mut image.pixels {
            *pixel &= BANK_INDEX_MASK;
        }
        image.bit_depth = SOURCE_BIT_DEPTH;
    }
    if let Some(&pixel) = image
        .pixels
        .iter()
        .find(|&&pixel| usize::from(pixel) >= BANK_COLORS)
    {
        return Err(mismatch(
            path,
            format!("has pixel index {pixel}: a 4bpp index is below {BANK_COLORS}"),
        ));
    }
    if let Some(palette_id) = embedded_palette_id {
        let colors =
            png::decode_palette(&bytes).map_err(|e| ExtractError::Png(path.to_path_buf(), e))?;
        if colors.len() != BANK_COLORS {
            return Err(mismatch(
                path,
                format!(
                    "has {} embedded colours: expected {BANK_COLORS}",
                    colors.len()
                ),
            ));
        }
        writer.push(build_palette_entry(path, &colors, palette_id.to_owned())?);
    }
    writer.push(build_image_entry(path, id, image)?);
    Ok(())
}

fn extract_palette(
    path: &Path,
    source: PaletteSource,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    write_palette(path, source.colors, source.id.to_owned(), writer)
}

fn write_palette(
    path: &Path,
    expected_colors: usize,
    id: String,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let text = read_text(path)?;
    let colors = jasc_pal::parse(&text).map_err(|e| ExtractError::Pal(path.to_path_buf(), e))?;
    if colors.len() != expected_colors {
        return Err(mismatch(
            path,
            format!("has {} colours: expected {expected_colors}", colors.len()),
        ));
    }
    writer.push(build_palette_entry(path, &colors, id)?);
    Ok(())
}

fn stem(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.split('.').next().unwrap_or(name)
}

/// Resolves the 378 `gItemIconTable` rows (`0..=ITEMS_COUNT`) to their image
/// and palette source paths, in row order.
fn icon_rows(upstream: &Path) -> Result<Vec<IconRow>, ExtractError> {
    let constants = read_text(&upstream.join(ITEM_CONSTANTS))?;
    let graphics_path = upstream.join(ICON_GRAPHICS);
    let graphics = read_text(&graphics_path)?;
    let table_path = upstream.join(ICON_TABLE);
    let table = read_text(&table_path)?;
    parse_icon_rows(&constants, &graphics, &table).map_err(|detail| mismatch(&table_path, detail))
}

fn parse_icon_rows(constants: &str, graphics: &str, table: &str) -> Result<Vec<IconRow>, String> {
    let numbers = define_numbers(constants);
    let item_count = *numbers
        .get("ITEMS_COUNT")
        .ok_or("`ITEMS_COUNT` is not defined")?;

    let mut paths: BTreeMap<&str, &str> = BTreeMap::new();
    for line in graphics.lines() {
        let Some(rest) = line.strip_prefix("const u32 gItemIcon") else {
            continue;
        };
        let (name, rest) = rest
            .split_once("[] = INCGFX_U32(\"")
            .ok_or_else(|| format!("unsupported icon declaration `{line}`"))?;
        let (path, _) = rest
            .split_once('"')
            .ok_or_else(|| format!("unterminated icon path in `{line}`"))?;
        if name.is_empty() || name.contains(char::is_whitespace) {
            return Err(format!("unsupported icon declaration `{line}`"));
        }
        if paths.insert(name, path).is_some() {
            return Err(format!("`{name}` is declared twice"));
        }
    }

    let (_, body) = table
        .split_once("gItemIconTable")
        .and_then(|(_, declaration)| declaration.split_once('\n'))
        .ok_or("`gItemIconTable` is missing")?;
    let mut rows: BTreeMap<usize, IconRow> = BTreeMap::new();
    for line in body.lines().map(str::trim).filter(|l| l.starts_with('[')) {
        let (key, rest) = line
            .strip_prefix('[')
            .and_then(|l| l.split_once("] = {"))
            .ok_or_else(|| format!("unsupported icon row `{line}`"))?;
        let (pair, _) = rest
            .split_once('}')
            .ok_or_else(|| format!("unterminated icon row `{line}`"))?;
        let (image, palette) = pair
            .split_once(", ")
            .ok_or_else(|| format!("icon row `{line}` does not pair an image and a palette"))?;
        let index = *numbers
            .get(key)
            .ok_or_else(|| format!("unknown item constant `{key}`"))?;
        let resolve = |symbol: &str, directory: &str, extension: &str| {
            let path = paths
                .get(symbol.strip_prefix("gItemIcon").unwrap_or(symbol))
                .ok_or_else(|| format!("icon symbol `{symbol}` has no declaration"))?;
            if path.starts_with(directory) && path.ends_with(extension) {
                Ok((*path).to_owned())
            } else {
                Err(format!(
                    "icon symbol `{symbol}` names `{path}`: expected {directory}*{extension}"
                ))
            }
        };
        let row = (
            resolve(image, ICON_IMAGE_DIRECTORY, ".png")?,
            resolve(palette, ICON_PALETTE_DIRECTORY, ".pal")?,
        );
        if rows.insert(index, row).is_some() {
            return Err(format!("icon row `{key}` appears twice"));
        }
    }
    if !(0..=item_count).all(|index| rows.contains_key(&index)) || rows.len() != item_count + 1 {
        return Err(format!(
            "has {} icon rows: expected exactly rows 0..={item_count}",
            rows.len()
        ));
    }
    Ok(rows.into_values().collect())
}

/// Numeric `#define NAME <digits>` lines; other definitions are skipped.
fn define_numbers(header: &str) -> BTreeMap<&str, usize> {
    let mut numbers = BTreeMap::new();
    for line in header.lines() {
        let mut words = line.split_whitespace();
        if words.next() != Some("#define") {
            continue;
        }
        if let (Some(name), Some(value)) = (words.next(), words.next()) {
            if let Ok(value) = value.parse() {
                numbers.insert(name, value);
            }
        }
    }
    numbers
}

/// Reads the tilemap once, so the buffer validated is the buffer queued: a
/// second read could publish bytes that changed after the length check.
fn extract_tilemap_with(
    path: &Path,
    read: &mut dyn FnMut(&Path) -> Result<Vec<u8>, ExtractError>,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let payload = read(path)?;
    let len = payload.len();
    if len != TILEMAP_BYTES {
        return Err(mismatch(
            path,
            format!("is {len} bytes: expected {TILEMAP_BYTES}"),
        ));
    }
    push_raw_bytes(TILEMAP_ID.to_owned(), payload, writer);
    Ok(())
}

fn mismatch(path: &Path, detail: String) -> ExtractError {
    ExtractError::BagAssetMismatch {
        path: path.to_path_buf(),
        detail,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    use assets::items::{ItemId, ITEMS_COUNT};
    use assets::{AssetPack, ItemDisplayTable};

    use super::super::{extract_to, repo_root, upstream_present};
    use super::{
        define_numbers, extract_bag, extract_image, extract_palette, extract_tilemap_with,
        icon_rows, parse_icon_rows, stem, IMAGES, PALETTES, TILEMAP_ID,
    };
    use crate::extract::ExtractError;
    use pack_format::PackWriter;

    const CONSTANTS: &str = "#define ITEM_NONE 0\n#define ITEM_A 1\n#define ITEM_B 2\n\
                             #define ITEMS_COUNT 3\n";
    const GRAPHICS: &str = "\
const u32 gItemIcon_Q[] = INCGFX_U32(\"graphics/items/icons/q.png\", \".4bpp.lz\");\n\
const u32 gItemIconPalette_Q[] = INCGFX_U32(\"graphics/items/icon_palettes/q.pal\", \".gbapal.lz\");\n\
const u32 gItemIcon_Box[] = INCGFX_U32(\"graphics/items/icons/box.png\", \".4bpp.lz\");\n\
const u32 gItemIconPalette_Red[] = INCGFX_U32(\"graphics/items/icon_palettes/red.pal\", \".gbapal.lz\");\n\
const u32 gItemIconPalette_Blue[] = INCGFX_U32(\"graphics/items/icon_palettes/blue.pal\", \".gbapal.lz\");\n";
    const TABLE: &str = "const u32 *const gItemIconTable[ITEMS_COUNT + 1][2] =\n{\n\
    [ITEM_NONE] = {gItemIcon_Q, gItemIconPalette_Q},\n\
    // comment\n\
    [ITEM_A] = {gItemIcon_Box, gItemIconPalette_Red},\n\
    [ITEM_B] = {gItemIcon_Box, gItemIconPalette_Blue},\n\
    [ITEMS_COUNT] = {gItemIcon_Q, gItemIconPalette_Red},\n};\n";

    fn scratch_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("pokeemerald-rs-bag-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fixed_ids() -> Vec<&'static str> {
        IMAGES
            .iter()
            .map(|source| source.id)
            .chain(
                IMAGES
                    .iter()
                    .filter_map(|source| source.embedded_palette_id),
            )
            .chain(PALETTES.iter().map(|source| source.id))
            .chain([TILEMAP_ID])
            .collect()
    }

    #[test]
    fn tilemap_pack_entry_holds_the_validated_bytes() {
        let expected: Vec<u8> = (0..=u8::MAX).cycle().take(super::TILEMAP_BYTES).collect();
        let mut reads = 0;
        let mut read = |_: &Path| {
            reads += 1;
            // Any read after the first sees a different, wrong-length file.
            Ok(if reads == 1 {
                expected.clone()
            } else {
                vec![0xFF; super::TILEMAP_BYTES - 2]
            })
        };

        let mut writer = PackWriter::new();
        extract_tilemap_with(Path::new("menu.bin"), &mut read, &mut writer).unwrap();
        assert_eq!(reads, 1);
        let pack = writer.finish().unwrap();
        let entries = pack_format::parse_directory(&pack).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, TILEMAP_ID);
        assert_eq!(
            &pack[entries[0].offset..entries[0].offset + entries[0].length],
            expected.as_slice()
        );
    }

    #[test]
    fn fixed_manifest_names_the_ordinary_bag_ids_once() {
        let ids = fixed_ids();
        let expected: BTreeSet<&str> = [
            "bag/image/menu",
            "bag/palette/menu/male",
            "bag/palette/menu/female",
            "bag/raw/menu",
            "bag/image/male",
            "bag/image/female",
            "bag/palette/bag",
            "bag/image/rotating_ball",
            "bag/palette/rotating_ball",
            "bag/image/select_button",
            "bag/image/hm",
            "bag/image/scroll_indicator",
            "bag/palette/scroll_indicator",
            "bag/image/swap_line",
            "bag/palette/swap_line",
            "bag/image/menu_info",
            "bag/palette/menu_info1",
            "bag/palette/menu_info2",
            "bag/palette/menu_info3",
        ]
        .into_iter()
        .collect();
        assert_eq!(ids.len(), expected.len());
        assert_eq!(ids.into_iter().collect::<BTreeSet<_>>(), expected);
    }

    #[test]
    fn manifest_shapes_match_the_upstream_graphics() {
        let images: Vec<_> = IMAGES
            .iter()
            .map(|source| (source.png, source.width, source.height))
            .collect();
        assert_eq!(
            images,
            [
                ("graphics/bag/menu.png", 128, 32),
                ("graphics/bag/bag_male.png", 64, 384),
                ("graphics/bag/bag_female.png", 64, 384),
                ("graphics/bag/rotating_ball.png", 16, 16),
                ("graphics/bag/select_button.png", 24, 16),
                ("graphics/bag/hm.png", 16, 16),
                ("graphics/interface/scroll_indicator.png", 16, 32),
                ("graphics/interface/swap_line.png", 16, 32),
                ("graphics/interface/menu_info.png", 128, 128),
            ]
        );
        let palettes: Vec<_> = PALETTES
            .iter()
            .map(|source| (source.pal, source.colors))
            .collect();
        assert_eq!(
            palettes,
            [
                ("graphics/bag/menu_male.pal", 32),
                ("graphics/bag/menu_female.pal", 32),
                ("graphics/bag/bag.pal", 16),
                ("graphics/interface/red.pal", 16),
                ("graphics/interface/menu_info1.pal", 16),
                ("graphics/interface/menu_info2.pal", 16),
                ("graphics/interface/menu_info3.pal", 16),
            ]
        );
    }

    #[test]
    fn icon_rows_resolve_every_table_row_with_independent_sharing() {
        let numbers = define_numbers(CONSTANTS);
        assert_eq!(numbers.get("ITEMS_COUNT"), Some(&3));
        let rows = parse_icon_rows(CONSTANTS, GRAPHICS, TABLE).unwrap();
        let stems: Vec<_> = rows
            .iter()
            .map(|(image, palette)| (stem(image), stem(palette)))
            .collect();
        assert_eq!(
            stems,
            [("q", "q"), ("box", "red"), ("box", "blue"), ("q", "red")]
        );
        assert_eq!(rows[1].0, "graphics/items/icons/box.png");
        assert_eq!(rows[2].1, "graphics/items/icon_palettes/blue.pal");
    }

    #[test]
    fn icon_table_contract_violations_are_rejected() {
        let missing_row = TABLE.replace("[ITEM_B] = {gItemIcon_Box, gItemIconPalette_Blue},\n", "");
        let duplicate = TABLE.replace("[ITEM_B]", "[ITEM_A]");
        let unknown_symbol = TABLE.replace("gItemIcon_Box", "gItemIcon_Nope");
        let unknown_item = TABLE.replace("[ITEM_B]", "[ITEM_C]");
        let unsupported = TABLE.replace("{gItemIcon_Box, gItemIconPalette_Blue}", "gItemIcon_Box");
        for (name, table) in [
            ("missing row", missing_row),
            ("duplicate row", duplicate),
            ("unknown symbol", unknown_symbol),
            ("unknown item", unknown_item),
            ("unsupported syntax", unsupported),
        ] {
            assert!(
                parse_icon_rows(CONSTANTS, GRAPHICS, &table).is_err(),
                "{name} accepted"
            );
        }
        let swapped_directory = GRAPHICS.replace("icon_palettes/red.pal", "icons/red.png");
        assert!(parse_icon_rows(CONSTANTS, &swapped_directory, TABLE).is_err());
        let doubled = format!("{GRAPHICS}{GRAPHICS}");
        assert!(parse_icon_rows(CONSTANTS, &doubled, TABLE).is_err());
    }

    #[test]
    fn missing_sources_fail_instead_of_being_skipped() {
        let dir = scratch_dir("missing");
        let mut writer = PackWriter::new();

        let err = extract_bag(&dir, &mut writer).unwrap_err();
        assert!(matches!(err, ExtractError::ReadFailed(path, _) if path.starts_with(&dir)));
        assert_eq!(writer.len(), 0);
        assert!(icon_rows(&dir).is_err());

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn sheets_and_palettes_that_are_malformed_are_rejected() {
        let dir = scratch_dir("malformed");
        let image_path = dir.join("menu.png");
        std::fs::write(&image_path, b"not a png").unwrap();
        let mut writer = PackWriter::new();
        let err = extract_image(&image_path, &IMAGES[0], &mut writer).unwrap_err();
        assert!(matches!(err, ExtractError::Png(path, _) if path == image_path));

        let palette_path = dir.join("bag.pal");
        std::fs::write(&palette_path, "JASC-PAL\n0100\n2\n0 0 0\n248 248 248\n").unwrap();
        let err = extract_palette(&palette_path, PALETTES[2], &mut writer).unwrap_err();
        assert!(matches!(
            err,
            ExtractError::BagAssetMismatch { path, .. } if path == palette_path
        ));
        assert_eq!(writer.len(), 0);

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_menu_info_sheet_is_narrowed_to_4bpp_like_gbagfx() {
        use super::super::png::tests::indexed_png_from_raw_with_plte;

        let dir = scratch_dir("menu-info-narrowing");
        let path = dir.join("menu_info.png");
        // One 8-bit row of indices spanning all three 16-colour banks.
        let indices: Vec<u8> = (0..128u32).map(|n| u8::try_from(n % 48).unwrap()).collect();
        let mut raw = Vec::new();
        for _ in 0..128 {
            raw.push(0);
            raw.extend_from_slice(&indices);
        }
        std::fs::write(
            &path,
            indexed_png_from_raw_with_plte(8, 128, 128, &raw, &[7; 48 * 3]),
        )
        .unwrap();

        let source = IMAGES
            .iter()
            .find(|source| source.id == "bag/image/menu_info")
            .unwrap();
        let mut writer = PackWriter::new();
        extract_image(&path, source, &mut writer).unwrap();
        let pack = writer.finish().unwrap();
        let entries = pack_format::parse_directory(&pack).unwrap();
        assert_eq!(entries.len(), 1);
        let entry = &entries[0];
        let pixels = &pack[entry.offset..entry.offset + entry.length];
        // gbagfx keeps each index modulo 16: 16..=47 land back on 0..=15.
        let expected: Vec<u8> = (0..128u32)
            .map(|n| u8::try_from(n % 48 % 16).unwrap())
            .collect();
        assert_eq!(&pixels[..128], expected.as_slice());
        assert!(pixels.iter().all(|&pixel| pixel < 16));

        // A 4-bit PNG is not what upstream ships for this sheet.
        let narrow = indexed_png_from_raw_with_plte(4, 128, 128, &vec![0; 128 * 65], &[7; 48]);
        std::fs::write(&path, narrow).unwrap();
        let err = extract_image(&path, source, &mut PackWriter::new()).unwrap_err();
        assert!(matches!(err, ExtractError::BagAssetMismatch { path: bad, .. } if bad == path));

        std::fs::remove_dir_all(dir).unwrap();
    }

    fn display_rows(upstream: &Path) -> Vec<(usize, String, String)> {
        let read = |relative: &str| std::fs::read_to_string(upstream.join(relative)).unwrap();
        let mut numbers: std::collections::BTreeMap<String, usize> =
            define_numbers(&read("include/constants/items.h"))
                .into_iter()
                .map(|(name, value)| (name.to_owned(), value))
                .collect();
        let moves = read("include/constants/tms_hms.h");
        for (prefix, first, list) in [
            ("ITEM_TM_", "ITEM_TM01", "FOREACH_TM"),
            ("ITEM_HM_", "ITEM_HM01", "FOREACH_HM"),
        ] {
            let (_, tail) = moves.split_once(&format!("#define {list}(F)")).unwrap();
            let names = tail
                .lines()
                .take_while(|line| line.trim_end().ends_with('\\') || line.contains("F("))
                .filter_map(|line| line.split("F(").nth(1)?.split(')').next())
                .map(str::to_owned);
            for (offset, name) in names.enumerate() {
                numbers.insert(format!("{prefix}{name}"), numbers[first] + offset);
            }
        }

        let mut descriptions = std::collections::BTreeMap::new();
        let mut current: Option<(String, String)> = None;
        for line in read("src/data/text/item_descriptions.h").lines() {
            if let Some(rest) = line.strip_prefix("static const u8 ") {
                current = Some((rest.split('[').next().unwrap().to_owned(), String::new()));
            }
            if let Some((_, text)) = current.as_mut() {
                if let Some((_, quoted)) = line.split_once('"') {
                    text.push_str(quoted.rsplit_once('"').unwrap().0);
                }
                if line.trim_end().ends_with(");") {
                    let (name, text) = current.take().unwrap();
                    descriptions.insert(name, text.replace("\\n", "\n"));
                }
            }
        }

        let mut rows = Vec::new();
        let mut designator = None;
        let mut name = None;
        for line in read("src/data/items.h").lines().map(str::trim) {
            if let Some(rest) = line.strip_prefix("[ITEM_") {
                designator = Some(format!("ITEM_{}", rest.split(']').next().unwrap()));
            } else if let Some(rest) = line.strip_prefix(".name = _(\"") {
                name = Some(rest.rsplit_once('"').unwrap().0.to_owned());
            } else if let Some(rest) = line.strip_prefix(".description = ") {
                let symbol = rest.trim_end_matches(',');
                rows.push((
                    numbers[designator.take().unwrap().as_str()],
                    name.take().unwrap(),
                    descriptions[symbol].clone(),
                ));
            }
        }
        rows
    }

    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn ordinary_bag_graphics_and_item_text_are_extracted() {
        assert!(upstream_present(), "run ./init.sh first");
        let path = std::env::temp_dir().join(format!(
            "pokeemerald-rs-extract-test-bag-{}.pack",
            std::process::id()
        ));
        extract_to(&path).expect("extraction should succeed against a real checkout");
        let pack = AssetPack::load(&path).unwrap();
        let upstream = repo_root().join("pokeemerald");

        for id in fixed_ids() {
            assert!(
                pack.image(id).is_ok() || pack.palette(id).is_ok() || pack.raw(id).is_ok(),
                "missing pack entry id `{id}`"
            );
        }
        let bag = pack.bag_assets().unwrap();
        assert_eq!(
            bag.screen_tilemap,
            std::fs::read(upstream.join("graphics/bag/menu.bin")).unwrap()
        );

        let pixels = |relative: &str| {
            crate::extract::png::decode(&std::fs::read(upstream.join(relative)).unwrap())
                .unwrap()
                .pixels
        };
        let colors = |relative: &str| -> Vec<Option<u16>> {
            crate::extract::jasc_pal::parse(
                &std::fs::read_to_string(upstream.join(relative)).unwrap(),
            )
            .unwrap()
            .iter()
            .map(|color| Some(color.to_gba555()))
            .collect()
        };
        let arrows = bag.scroll_indicator().unwrap();
        assert_eq!(
            arrows.image.pixels,
            pixels("graphics/interface/scroll_indicator.png")
        );
        let actual: Vec<_> = (0..16).map(|i| arrows.palette.color(i)).collect();
        assert_eq!(actual, colors("graphics/interface/red.pal"));
        let line = bag.swap_line().unwrap();
        assert_eq!(
            line.image.pixels,
            pixels("graphics/interface/swap_line.png")
        );
        let embedded = crate::extract::png::decode_palette(
            &std::fs::read(upstream.join("graphics/interface/swap_line.png")).unwrap(),
        )
        .unwrap();
        let expected: Vec<_> = embedded.iter().map(|c| Some(c.to_gba555())).collect();
        let actual: Vec<_> = (0..16).map(|i| line.palette.color(i)).collect();
        assert_eq!(actual, expected);

        let info = bag.menu_info().unwrap();
        let sheet = pixels("graphics/interface/menu_info.png");
        // `gbagfx` keeps each PNG index modulo 16 for a `.4bpp` rule.
        let narrowed: Vec<u8> = sheet.iter().map(|pixel| pixel % 16).collect();
        assert_eq!(info.sheet.pixels, narrowed.as_slice());
        for (n, file) in ["menu_info1.pal", "menu_info2.pal", "menu_info3.pal"]
            .into_iter()
            .enumerate()
        {
            let actual: Vec<_> = (0..16).map(|i| info.palettes[n].color(i)).collect();
            assert_eq!(actual, colors(&format!("graphics/interface/{file}")));
        }

        let rows = icon_rows(&upstream).unwrap();
        assert_eq!(rows.len(), ITEMS_COUNT + 1);
        let image_ids: BTreeSet<_> = rows.iter().map(|(image, _)| stem(image)).collect();
        let palette_ids: BTreeSet<_> = rows.iter().map(|(_, palette)| stem(palette)).collect();
        assert_eq!((image_ids.len(), palette_ids.len()), (218, 253));
        for (index, (image_path, palette_path)) in rows.iter().enumerate() {
            let item = if index == ITEMS_COUNT {
                ItemId(u16::MAX)
            } else {
                ItemId(u16::try_from(index).unwrap())
            };
            let icon = bag.item_icon(item).unwrap();
            let source =
                crate::extract::png::decode(&std::fs::read(upstream.join(image_path)).unwrap())
                    .unwrap();
            assert_eq!(icon.image.pixels, source.pixels.as_slice(), "{image_path}");
            let colors = crate::extract::jasc_pal::parse(
                &std::fs::read_to_string(upstream.join(palette_path)).unwrap(),
            )
            .unwrap();
            let expected: Vec<_> = colors.iter().map(|color| Some(color.to_gba555())).collect();
            let actual: Vec<_> = (0..16).map(|i| icon.palette.color(i)).collect();
            assert_eq!(actual, expected, "{palette_path}");
        }

        let display = display_rows(&upstream);
        assert_eq!(display.len(), ITEMS_COUNT);
        let table = ItemDisplayTable::new();
        for (index, name, description) in display {
            let item = table.get(ItemId(u16::try_from(index).unwrap())).unwrap();
            assert_eq!(
                (item.name, item.description),
                (name.as_str(), description.as_str())
            );
        }
        let _ = std::fs::remove_file(path);
    }
}
