//! Locating an image asset without being told how it is stored.
//!
//! A pack image entry says what the art *looks like*, not how upstream
//! packed it: the bit depth `gbagfx` used and the `-mwidth`/`-mheight`
//! metatile walk it wrote the tiles in are build options, invisible in the
//! pack. Rather than restate those options here, this module lets the ROM
//! answer. It packs the raster under every depth and metatile shape that
//! could produce it, searches for all of them at once, and keeps the one
//! that turns up.
//!
//! That is a real signature, not a guess: a wrong metatile walk permutes
//! tiles, and a permuted sheet does not appear anywhere in the ROM. Where
//! several shapes produce byte-identical output (a sheet whose whole grid
//! is one metatile), the largest is reported, which is the one upstream
//! wrote.
//!
//! # Duplicate art
//!
//! A handful of sheets are byte-identical to another and appear twice in
//! the ROM under two symbols. Nothing in the *bytes* tells the two symbols
//! apart, and neither does the struct chain around them: the object-event
//! graphics-info table distinguishes them only by an `OBJ_EVENT_GFX_*`
//! constant, which is a name, not evidence, and carrying those names is
//! exactly the hand-maintained knowledge this design exists to avoid.
//!
//! So the choice is arbitrary, and says so: duplicates are paired off in
//! address order against the ids that share the art, each gets a note, and
//! each is recorded as [`Resolution::ArbitraryAmongIdentical`] rather than
//! as a struct-derived resolution it is not. That is sound only because
//! every candidate holds identical bytes, so the pack entry the importer
//! writes is the same whichever one it reads.

use std::collections::BTreeMap;

use rom_import::Encoding;

use super::error::GenRomProfileError;
use super::locate::to_addr;
use super::pack_source::{image_tiles, metatile_candidates};
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

/// How one image's tiles are laid out: ROM bit depth, metatile shape, and
/// how many tiles are stored.
type Packing = (u8, (u32, u32), u32);

/// Every address one image's art was found at, with the packing that
/// matched there.
type Candidates = BTreeMap<u32, Packing>;

/// One candidate packing of one image.
struct Variant {
    query_index: usize,
    rom_bit_depth: u8,
    metatile: (u32, u32),
    tile_count: u32,
}

/// The fragments a linker map's symbol for `id` must contain.
///
/// Upstream's naming is only loosely conventional -- a `static` sheet and a
/// global one differ in their prefix, and `walking.png` becomes `Normal` --
/// so the check asserts the family the symbol belongs to, not a name this
/// generator would have to guess.
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
    if let Some(stem) = id.strip_prefix("text-window/image/") {
        if stem == "message_box" {
            return vec!["gMessageBox_Gfx".to_owned()];
        }
        return vec![format!("TextWindowFrame{stem}_Gfx")];
    }
    Vec::new()
}

/// Which ROM bit depths could produce a pack entry of this depth.
///
/// A 4bpp entry can only have come from 4bpp tiles. An 8bpp entry usually
/// came from 8bpp tiles, but not always: the title screen's press-start
/// banner is an 8-bit-indexed PNG whose indices all fit a nibble, and
/// upstream stores it as 4bpp.
fn rom_depths(pack_bit_depth: u8) -> &'static [u8] {
    match pack_bit_depth {
        8 => &[8, 4],
        _ => &[4],
    }
}

/// Drop trailing all-zero tiles, the way upstream's own art is stored.
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

/// Locate every image in `queries`, in as few ROM passes as possible.
///
/// # Errors
///
/// [`GenRomProfileError::NotFound`] if an image's art is nowhere in the
/// ROM under any packing, or [`GenRomProfileError::Ambiguous`] if it turns
/// up at more addresses than there are ids sharing its bytes.
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
        // Shape validated against the payload before anything enumerates
        // over it: a malformed pack's dimensions drive the metatile walk
        // and the tile packing, so they must be backed by real bytes.
        let (_, width, height, pack_bit_depth) = asset.image_raster(&query.id)?;
        for &rom_bit_depth in rom_depths(pack_bit_depth) {
            let bytes_per_tile = if rom_bit_depth == 4 { 32 } else { 64 };
            for metatile in metatile_candidates(width, height) {
                let tiles = image_tiles(ctx.pack, &query.id, rom_bit_depth, metatile)?;
                let full = tiles.len();
                // Upstream cuts art short in two ways -- `-num_tiles`, and
                // dropping trailing all-zero tiles -- and the two do not
                // always agree, so the exact cut is not derivable. What is
                // derivable is the range it must lie in: no shorter than
                // the last tile with art in it, no longer than the whole
                // sheet. Uncompressed art is stored whole, so a raw search
                // only looks for the whole sheet.
                let kept = match query.encoding {
                    Encoding::Raw => full,
                    Encoding::Lz77 => trim_zero_tiles(&tiles, bytes_per_tile).min(full),
                };
                let (needles, variants) = match query.encoding {
                    Encoding::Raw => (&mut raw_needles, &mut raw_variants),
                    Encoding::Lz77 => (&mut lz_needles, &mut lz_variants),
                };
                for len in (kept..=full).step_by(bytes_per_tile) {
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

    // Collect, per query, every (address, packing) the ROM agreed with.
    let mut found: Vec<Candidates> = vec![BTreeMap::new(); queries.len()];
    for (variants, hits) in [(&raw_variants, &raw_hits), (&lz_variants, &lz_hits)] {
        for (variant, offsets) in variants.iter().zip(hits.iter()) {
            for &offset in offsets {
                let slot = found[variant.query_index]
                    .entry(to_addr(offset))
                    .or_insert((variant.rom_bit_depth, variant.metatile, variant.tile_count));
                // Shapes are enumerated largest first, so the first packing
                // recorded for an address is the one to keep; a longer tile
                // run at the same address wins, since it is the whole sheet
                // rather than a prefix of it.
                if variant.tile_count > slot.2 {
                    *slot = (variant.rom_bit_depth, variant.metatile, variant.tile_count);
                }
            }
        }
    }

    assign(ctx, queries, &found, report)
}

/// Turn per-query candidate addresses into one plan each.
///
/// Ids whose art is byte-identical share a duplicate set and are paired off
/// in address order; see the module docs.
fn assign(
    ctx: &Context<'_>,
    queries: &[ImageQuery],
    found: &[Candidates],
    report: &mut Vec<ReportLine>,
) -> Result<Vec<ImagePlan>, GenRomProfileError> {
    // Group ids that landed on the exact same address set: those are the
    // ones sharing art.
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
        if addrs.len() == 1 {
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
        // More copies than ids, or exactly as many: pair them off in
        // address order. Every copy holds the same bytes, so the pack entry
        // is the same whichever one a reader takes.
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
        let fragments = symbol_fragments(&query.id);
        if !fragments.is_empty() {
            line = line.symbol_contains(fragments);
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
        // A domain with no convention asserts nothing, which
        // `symbol_contains` turns into "any symbol starts here".
        assert!(symbol_fragments("tileset/general/anim/water/0").is_empty());
    }

    #[test]
    fn trailing_zero_tiles_are_dropped_whole() {
        let mut tiles = vec![1u8; 32];
        tiles.extend_from_slice(&[0u8; 64]);
        assert_eq!(trim_zero_tiles(&tiles, 32), 32);
        // A tile that is only partly zero survives.
        tiles[95] = 9;
        assert_eq!(trim_zero_tiles(&tiles, 32), 96);
        // An entirely zero sheet trims to nothing.
        assert_eq!(trim_zero_tiles(&[0u8; 64], 32), 0);
    }
}
