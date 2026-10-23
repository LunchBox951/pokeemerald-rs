//! Locates image assets in the ROM by searching for their packed tiles.
//!
//! A pack image entry carries no `gbagfx` bit depth or `-mwidth`/`-mheight`
//! metatile shape. Each raster is packed under every candidate depth and
//! metatile shape, all needles are searched at once, and the shape that
//! matches is kept. A wrong metatile walk permutes tiles, so a wrong shape
//! does not match; when several shapes give identical bytes (one-metatile
//! grid), the largest is reported, as upstream wrote it.
//!
//! # Duplicate art
//!
//! Some sheets are byte-identical and appear under two symbols. Neither the
//! bytes nor the struct chain separates them: the object-event
//! graphics-info table distinguishes them only by `OBJ_EVENT_GFX_*` names,
//! which this generator does not carry. Duplicates are paired with the ids
//! sharing the art in address order and recorded as
//! [`Resolution::ArbitraryAmongIdentical`] with a note; the importer's pack
//! entry is the same whichever copy it reads.

use std::collections::BTreeMap;

use rom_import::Encoding;

use super::error::GenRomProfileError;
use super::locate::to_addr;
use super::pack_source::{image_tiles, metatile_candidates, PackSource};
use super::plan::{ImagePlan, ReportLine, Resolution};
use super::Context;

/// One image to locate.
#[derive(Debug, Clone)]
pub struct ImageQuery {
    /// The pack id whose art to find.
    pub id: String,
    /// How the ROM stores it.
    pub encoding: Encoding,
}

impl ImageQuery {
    /// An image stored verbatim.
    pub fn raw(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            encoding: Encoding::Raw,
        }
    }

    /// An image stored as an LZ77 stream.
    pub fn lz77(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            encoding: Encoding::Lz77,
        }
    }
}

/// ROM bit depth, metatile shape, and stored tile count of one image.
type Packing = (u8, (u32, u32), u32);

/// Each address one image matched at, with the matching packing.
type Candidates = BTreeMap<u32, Packing>;

/// One candidate packing of the query at `query_index`.
struct Variant {
    query_index: usize,
    rom_bit_depth: u8,
    metatile: (u32, u32),
    tile_count: u32,
}

/// The upstream name of an image root declared `static` (so a linker map
/// cannot list it), or `None` for a global root.
///
/// `title_screen.c:63-66`, `item_menu_icons.c:37`, `item_menu.c:374`.
pub(super) fn local_image_symbol(id: &str) -> Option<String> {
    if let Some(name) = id.strip_prefix("title/image/") {
        let camel = super::locate::camel_case(name);
        return matches!(camel.as_str(), "Rayquaza" | "LogoShine" | "Clouds")
            .then(|| format!("sTitleScreen{camel}Gfx"));
    }
    match id {
        "bag/image/rotating_ball" => Some("sRotatingBall_Gfx".to_owned()),
        "bag/image/select_button" => Some("sRegisteredSelect_Gfx".to_owned()),
        _ => None,
    }
}

/// The fragments a linker-map symbol for `id` must contain; empty when the
/// domain has no naming convention.
fn symbol_fragments(id: &str) -> Vec<String> {
    if let Some(name) = id.strip_prefix("title/image/") {
        return vec![
            "TitleScreen".to_owned(),
            super::locate::camel_case(name),
            "Gfx".to_owned(),
        ];
    }
    if id.starts_with("sprite/") {
        return vec!["gObjectEventPic_".to_owned()];
    }
    if let Some(species) = id.strip_prefix("pokemon/") {
        if let Some((name, direction)) = species.split_once('/') {
            // `front.png` is `gMonStillFrontPic_*`; `gMonFrontPic_*` is the animation sheet.
            let table = if direction == "back" {
                "Back"
            } else {
                "StillFront"
            };
            return vec![format!(
                "gMon{table}Pic_{}",
                super::locate::camel_case(name)
            )];
        }
    }
    if let Some(stem) = id.strip_prefix("text-window/image/") {
        if stem == "message_box" {
            return vec!["gMessageBox_Gfx".to_owned()];
        }
        return vec![format!("TextWindowFrame{stem}_Gfx")];
    }
    match id {
        "poke-ball/image/poke" => vec!["gBallGfx_Poke".to_owned()],
        "poke-ball/image/open" => vec!["gOpenPokeballGfx".to_owned()],
        _ => Vec::new(),
    }
}

/// The ROM bit depths a pack entry of `pack_bit_depth` may be stored at,
/// authoritative depth first.
///
/// 2bpp and 4bpp entries are stored as 4bpp only. An 8bpp entry may also be
/// 4bpp (the title press-start banner has nibble-sized indices); that second
/// depth is a probe ([`probe_image_tiles`]).
fn rom_depths(pack_bit_depth: u8) -> &'static [u8] {
    match pack_bit_depth {
        8 => &[8, 4],
        _ => &[4],
    }
}

/// [`image_tiles`] for a probe depth of [`rom_depths`]; a palette index that
/// does not fit the depth gives `None` instead of an error.
///
/// # Errors
///
/// Same as [`image_tiles`], except
/// [`pack_format::EntryShapeError::ImagePaletteIndexOutOfRange`] resolves to
/// `Ok(None)`.
fn probe_image_tiles(
    pack: &PackSource,
    id: &str,
    rom_bit_depth: u8,
    metatile: (u32, u32),
) -> Result<Option<Vec<u8>>, GenRomProfileError> {
    let asset = pack.get(id)?;
    let (width, height, _) = asset.image_shape(id)?;
    match pack_format::tiles_from_image(
        &asset.payload,
        rom_bit_depth,
        width,
        height,
        Some(metatile),
    ) {
        Ok(tiles) => Ok(Some(tiles)),
        Err(pack_format::EntryShapeError::ImagePaletteIndexOutOfRange { .. }) => Ok(None),
        Err(err) => Err(GenRomProfileError::EntryShape {
            id: id.to_owned(),
            reason: err.to_string(),
        }),
    }
}

/// The byte length of `tiles` without its trailing all-zero tiles.
fn trim_zero_tiles(tiles: &[u8], bytes_per_tile: usize) -> usize {
    let mut kept = tiles.len();
    while kept >= bytes_per_tile
        && tiles[kept - bytes_per_tile..kept]
            .iter()
            .all(|&byte| byte == 0)
    {
        kept -= bytes_per_tile;
    }
    kept
}

/// Locates every image in `queries` with one raw and one LZ77 ROM search.
///
/// # Errors
///
/// [`GenRomProfileError::NotFound`] if an image's art is nowhere in the
/// ROM under any packing, or [`GenRomProfileError::Ambiguous`] if it turns
/// up at fewer addresses than there are ids sharing its bytes, leaving
/// them no copy each. More copies than ids is the accepted case.
pub fn locate_images(
    ctx: &Context<'_>,
    queries: &[ImageQuery],
    report: &mut Vec<ReportLine>,
) -> Result<Vec<ImagePlan>, GenRomProfileError> {
    let mut raw_needles: Vec<Vec<u8>> = Vec::new();
    let mut raw_variants: Vec<Variant> = Vec::new();
    let mut lz_needles: Vec<Vec<u8>> = Vec::new();
    let mut lz_variants: Vec<Variant> = Vec::new();

    for (query_index, query) in queries.iter().enumerate() {
        let asset = ctx.pack.get(&query.id)?;
        // Validates the dimensions against the payload before they drive packing.
        let (_, width, height, pack_bit_depth) = asset.image_raster(&query.id)?;
        for (depth_index, &rom_bit_depth) in rom_depths(pack_bit_depth).iter().enumerate() {
            let bytes_per_tile = if rom_bit_depth == 4 { 32 } else { 64 };
            for metatile in metatile_candidates(width, height) {
                // Only depths after the first are probes; see `rom_depths`.
                let tiles = if depth_index == 0 {
                    image_tiles(ctx.pack, &query.id, rom_bit_depth, metatile)?
                } else {
                    let Some(tiles) =
                        probe_image_tiles(ctx.pack, &query.id, rom_bit_depth, metatile)?
                    else {
                        continue;
                    };
                    tiles
                };
                let full = tiles.len();
                // LZ77 art may be cut by `-num_tiles` or trailing-zero trimming, so
                // search every length from the last non-zero tile to the full
                // sheet. Raw art is stored whole.
                let kept = match query.encoding {
                    Encoding::Raw => full,
                    Encoding::Lz77 => trim_zero_tiles(&tiles, bytes_per_tile).min(full),
                };
                let (needles, variants) = match query.encoding {
                    Encoding::Raw => (&mut raw_needles, &mut raw_variants),
                    Encoding::Lz77 => (&mut lz_needles, &mut lz_variants),
                };
                // One tile minimum: an empty needle would match every declared-size-zero
                // LZ77 stream in the ROM.
                for len in (kept.max(bytes_per_tile)..=full).step_by(bytes_per_tile) {
                    variants.push(Variant {
                        query_index,
                        rom_bit_depth,
                        metatile,
                        tile_count: u32::try_from(len / bytes_per_tile)
                            .expect("a tile count fits in u32"),
                    });
                    needles.push(tiles[..len].to_vec());
                }
            }
        }
    }

    let raw_hits = ctx.raw.find_all(&raw_needles);
    let lz_hits = ctx.lz77.find_all(&lz_needles);

    // Per query, every (address, packing) the ROM matched.
    let mut found: Vec<Candidates> = vec![BTreeMap::new(); queries.len()];
    for (variants, hits) in [(&raw_variants, &raw_hits), (&lz_variants, &lz_hits)] {
        for (variant, offsets) in variants.iter().zip(hits.iter()) {
            for &offset in offsets {
                let slot = found[variant.query_index]
                    .entry(to_addr(offset))
                    .or_insert((variant.rom_bit_depth, variant.metatile, variant.tile_count));
                // Shapes are enumerated largest first, so the first packing at an
                // address stays unless a longer tile run (the whole sheet) matches.
                if variant.tile_count > slot.2 {
                    *slot = (variant.rom_bit_depth, variant.metatile, variant.tile_count);
                }
            }
        }
    }

    assign(ctx, queries, &found, report)
}

/// One plan per query from its candidate addresses; ids with identical
/// address sets are paired in address order (see the module docs).
fn assign(
    ctx: &Context<'_>,
    queries: &[ImageQuery],
    found: &[Candidates],
    report: &mut Vec<ReportLine>,
) -> Result<Vec<ImagePlan>, GenRomProfileError> {
    // Ids with the same address set share art.
    let mut groups: BTreeMap<Vec<u32>, Vec<usize>> = BTreeMap::new();
    for (index, candidates) in found.iter().enumerate() {
        groups
            .entry(candidates.keys().copied().collect())
            .or_default()
            .push(index);
    }
    let mut chosen: Vec<Option<u32>> = vec![None; queries.len()];
    let mut notes: Vec<Option<String>> = vec![None; queries.len()];
    for (addrs, members) in groups {
        if addrs.is_empty() {
            continue;
        }
        if addrs.len() == 1 && members.len() == 1 {
            for index in members {
                chosen[index] = Some(addrs[0]);
            }
            continue;
        }
        if addrs.len() < members.len() {
            return Err(GenRomProfileError::Ambiguous {
                id: queries[members[0]].id.clone(),
                addrs,
            });
        }
        // At least as many copies as ids: pair in address order; copies are identical.
        for (slot, index) in members.iter().enumerate() {
            chosen[*index] = Some(addrs[slot]);
            notes[*index] = Some(format!(
                "art repeats at {} addresses holding identical bytes; nothing distinguishes \
                 them, so this is the one at index {slot} in address order",
                addrs.len()
            ));
        }
    }

    let mut plans = Vec::new();
    for (index, query) in queries.iter().enumerate() {
        let addr = chosen[index].ok_or_else(|| GenRomProfileError::NotFound {
            id: query.id.clone(),
        })?;
        let (rom_bit_depth, metatile, tile_count) = found[index][&addr];
        let asset = ctx.pack.get(&query.id)?;
        let (width, height, pack_bit_depth) = asset.image_shape(&query.id)?;
        let mut line = ReportLine::unique(
            &query.id,
            addr,
            tile_count * if rom_bit_depth == 4 { 32 } else { 64 },
        );
        if let Some(note) = notes[index].clone() {
            line = line.with(Resolution::ArbitraryAmongIdentical).note(note);
        }
        if let Some(name) = local_image_symbol(&query.id) {
            line = line.local_symbol(name);
        } else {
            let fragments = symbol_fragments(&query.id);
            if !fragments.is_empty() {
                line = line.symbol_contains(fragments);
            }
        }
        report.push(line);
        plans.push(ImagePlan {
            id: query.id.clone(),
            addr,
            encoding: query.encoding,
            rom_bit_depth,
            pack_bit_depth,
            width,
            height,
            metatile_width: metatile.0,
            metatile_height: metatile.1,
            tile_count,
        });
    }
    Ok(plans)
}

#[cfg(test)]
mod tests {
    use super::{rom_depths, symbol_fragments, trim_zero_tiles};

    #[test]
    fn a_four_bit_entry_can_only_come_from_four_bit_tiles() {
        assert_eq!(rom_depths(4), &[4]);
        assert_eq!(rom_depths(2), &[4]);
        assert_eq!(rom_depths(8), &[8, 4]);
    }

    #[test]
    fn battler_images_expect_the_still_front_and_back_symbols() {
        assert_eq!(
            symbol_fragments("pokemon/treecko/front"),
            ["gMonStillFrontPic_Treecko"]
        );
        assert_eq!(
            symbol_fragments("pokemon/mudkip/back"),
            ["gMonBackPic_Mudkip"]
        );
    }

    #[test]
    fn symbol_fragments_name_the_family_not_the_symbol() {
        assert_eq!(
            symbol_fragments("title/image/logo_shine"),
            ["TitleScreen", "LogoShine", "Gfx"]
        );
        assert_eq!(
            symbol_fragments("sprite/brendan/walking"),
            ["gObjectEventPic_"]
        );
        assert_eq!(
            symbol_fragments("text-window/image/12"),
            ["TextWindowFrame12_Gfx"]
        );
        assert_eq!(
            symbol_fragments("text-window/image/message_box"),
            ["gMessageBox_Gfx"]
        );
        // No convention asserts nothing; `symbol_contains` then accepts any symbol.
        assert!(symbol_fragments("tileset/general/anim/water/0").is_empty());
    }

    #[test]
    fn trailing_zero_tiles_are_dropped_whole() {
        let mut tiles = vec![1u8; 32];
        tiles.extend_from_slice(&[0u8; 64]);
        assert_eq!(trim_zero_tiles(&tiles, 32), 32);
        // A partly-zero tile stays.
        tiles[95] = 9;
        assert_eq!(trim_zero_tiles(&tiles, 32), 96);
        // An all-zero sheet trims to nothing.
        assert_eq!(trim_zero_tiles(&[0u8; 64], 32), 0);
    }
}
