//! Locating the five bundled tilesets.
//!
//! A tileset's own `struct Tileset` is found through its metatile table,
//! not by scanning for the struct: the table is thousands of bytes of
//! unique data, while the struct is six pointers and two flags. So the
//! locator matches the two flat tables the pack already holds
//! (`metatiles.bin`, `metatile_attributes.bin`), asks the pointer index
//! what references the first, and accepts the one candidate whose fields
//! line up as `pokeemerald/include/global.fieldmap.h` declares them.
//!
//! Everything else in the tileset falls out of that struct's pointers. The
//! animation frames do not: `src/tileset_anims.c` stores them uncompressed
//! and out of frame order, with padding in between, and nothing points at
//! them from the struct, so each frame is matched on its own bytes.

use std::collections::BTreeMap;

use rom_import::{lz77_decompress, Encoding};

use super::error::GenRomProfileError;
use super::locate::{
    camel_case, exactly_one, only_one_matching, slice_at_addr, tile_count_of_prefix, u32_at_addr,
    u8_at_addr,
};
use super::pack_source::image_tiles;
use super::plan::{
    BlobPlan, ImagePlan, PalettePlan, ReportLine, Resolution, TileAnimPlan, TilesetPlan,
};
use super::Context;

/// Field offsets inside `struct Tileset`.
const FIELD_TILES: u32 = 0x04;
/// Offset of the `palettes` field.
const FIELD_PALETTES: u32 = 0x08;
/// Offset of the `metatiles` field.
const FIELD_METATILES: u32 = 0x0C;
/// Offset of the `metatileAttributes` field.
const FIELD_METATILE_ATTRIBUTES: u32 = 0x10;
/// Offset of the `callback` field.
const FIELD_CALLBACK: u32 = 0x14;

/// Every tileset carries 16 palette banks of 16 colours.
const PALETTE_BANKS: u32 = 16;
/// One 16-colour GBA bank is 32 bytes.
const BANK_BYTES: u32 = 32;
/// Tileset art is always 4bpp, so one tile is 32 bytes.
const TILE_BYTES: usize = 32;

/// Locate every tileset the pack holds.
///
/// # Errors
///
/// Any [`GenRomProfileError`] a locator raises: a missing pack entry, a
/// root that matches nothing or matches twice, or a struct whose fields do
/// not agree with the tables they should point at.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<Vec<TilesetPlan>, GenRomProfileError> {
    let mut plans = Vec::new();
    for name in tileset_names(ctx) {
        plans.push(locate_one(ctx, &name, report)?);
    }
    Ok(plans)
}

/// Every tileset name the pack holds, ascending.
fn tileset_names(ctx: &Context<'_>) -> Vec<String> {
    ctx.pack
        .ids_with_prefix("tileset/")
        .into_iter()
        .filter_map(|id| id.strip_suffix("/tiles").map(str::to_owned))
        .filter_map(|id| id.strip_prefix("tileset/").map(str::to_owned))
        .collect()
}

fn locate_one(
    ctx: &Context<'_>,
    name: &str,
    report: &mut Vec<ReportLine>,
) -> Result<TilesetPlan, GenRomProfileError> {
    let metatiles_id = format!("tileset/{name}/metatiles");
    let attributes_id = format!("tileset/{name}/metatile-attributes");
    let tiles_id = format!("tileset/{name}/tiles");

    let anim_ids = anim_frame_ids(ctx, name);
    let mut needles = vec![
        ctx.pack.get(&metatiles_id)?.payload.clone(),
        ctx.pack.get(&attributes_id)?.payload.clone(),
    ];
    let mut anim_shapes = Vec::new();
    for (_, frames) in &anim_ids {
        for id in frames {
            let asset = ctx.pack.get(id)?;
            let (width, height, _) = asset.image_shape(id)?;
            anim_shapes.push((width, height));
            needles.push(image_tiles(ctx.pack, id, 4, (1, 1))?);
        }
    }
    let hits = ctx.raw.find_all(&needles);

    let metatiles_addr = exactly_one(&metatiles_id, &hits[0])?;
    let attributes_addr = exactly_one(&attributes_id, &hits[1])?;

    // The struct is whatever points at the metatile table 0x0C bytes into
    // itself and agrees about everything else.
    let struct_addr = only_one_matching(
        name,
        "the `struct Tileset` layout",
        ctx.pointers
            .refs_to(metatiles_addr)
            .iter()
            .filter_map(|at| at.checked_sub(FIELD_METATILES)),
        |&base| is_tileset_struct(ctx, base, metatiles_addr, attributes_addr),
    )?;

    let is_compressed = u8_at_addr(ctx.rom, struct_addr) == Some(1);
    let is_secondary = u8_at_addr(ctx.rom, struct_addr + 1) == Some(1);
    let tiles_addr = field(ctx, struct_addr, FIELD_TILES, name)?;
    let palettes_addr = field(ctx, struct_addr, FIELD_PALETTES, name)?;
    let callback = u32_at_addr(ctx.rom, struct_addr + FIELD_CALLBACK).unwrap_or(0);

    let upstream = camel_case(name);
    report.push(
        ReportLine::unique(format!("tileset/{name}"), struct_addr, 0x18)
            .with(Resolution::StructDerived)
            .symbol(format!("gTileset_{upstream}"))
            .note("found through its metatile table"),
    );
    report.push(
        ReportLine::unique(
            &metatiles_id,
            metatiles_addr,
            len32(&ctx.pack.get(&metatiles_id)?.payload),
        )
        .symbol(format!("gMetatiles_{upstream}")),
    );
    report.push(
        ReportLine::unique(
            &attributes_id,
            attributes_addr,
            len32(&ctx.pack.get(&attributes_id)?.payload),
        )
        .symbol(format!("gMetatileAttributes_{upstream}")),
    );

    let tiles = locate_tiles(ctx, &tiles_id, tiles_addr, is_compressed)?;
    report.push(
        ReportLine::unique(&tiles_id, tiles_addr, tiles.tile_count * 32)
            .with(Resolution::PointerWalk)
            .symbol(format!("gTilesetTiles_{upstream}"))
            .note(format!("{} tiles stored", tiles.tile_count)),
    );

    let palettes = locate_palettes(ctx, name, palettes_addr, report)?;
    let anims = build_anims(ctx, &anim_ids, &anim_shapes, &hits[2..], report)?;

    Ok(TilesetPlan {
        name: name.to_owned(),
        struct_addr,
        is_compressed,
        is_secondary,
        tiles,
        palettes,
        metatiles: BlobPlan {
            id: metatiles_id.clone(),
            addr: metatiles_addr,
            encoding: Encoding::Raw,
            len: len32(&ctx.pack.get(&metatiles_id)?.payload),
        },
        metatile_attributes: BlobPlan {
            id: attributes_id.clone(),
            addr: attributes_addr,
            encoding: Encoding::Raw,
            len: len32(&ctx.pack.get(&attributes_id)?.payload),
        },
        callback,
        anims,
    })
}

/// Whether the bytes at `base` read as a `struct Tileset` for these tables.
fn is_tileset_struct(ctx: &Context<'_>, base: u32, metatiles: u32, attributes: u32) -> bool {
    let flag_ok = |at: u32| matches!(u8_at_addr(ctx.rom, at), Some(0 | 1));
    let ptr_ok = |at: u32| {
        u32_at_addr(ctx.rom, base + at)
            .is_some_and(|value| super::locate::to_offset(value).is_some())
    };
    flag_ok(base)
        && flag_ok(base + 1)
        && ptr_ok(FIELD_TILES)
        && ptr_ok(FIELD_PALETTES)
        && u32_at_addr(ctx.rom, base + FIELD_METATILES) == Some(metatiles)
        && u32_at_addr(ctx.rom, base + FIELD_METATILE_ATTRIBUTES) == Some(attributes)
}

/// Read a pointer field, failing if it is not a cartridge address.
fn field(
    ctx: &Context<'_>,
    struct_addr: u32,
    offset: u32,
    name: &str,
) -> Result<u32, GenRomProfileError> {
    u32_at_addr(ctx.rom, struct_addr + offset)
        .filter(|value| super::locate::to_offset(*value).is_some())
        .ok_or_else(|| GenRomProfileError::StructMismatch {
            id: name.to_owned(),
            reason: format!("field +{offset:#04x} is not a cartridge address"),
        })
}

/// Check the ROM's tile data against the pack's raster and describe it.
fn locate_tiles(
    ctx: &Context<'_>,
    id: &str,
    addr: u32,
    is_compressed: bool,
) -> Result<ImagePlan, GenRomProfileError> {
    let asset = ctx.pack.get(id)?;
    let (width, height, pack_bit_depth) = asset.image_shape(id)?;
    let expected = image_tiles(ctx.pack, id, 4, (1, 1))?;

    let offset =
        super::locate::to_offset(addr).ok_or_else(|| GenRomProfileError::StructMismatch {
            id: id.to_owned(),
            reason: "the tiles pointer is not a cartridge address".to_owned(),
        })?;
    let rom_tiles = if is_compressed {
        lz77_decompress(&ctx.rom[offset..], None).map_err(|fault| {
            GenRomProfileError::StructMismatch {
                id: id.to_owned(),
                reason: format!("the tiles stream does not decompress: {fault}"),
            }
        })?
    } else {
        slice_at_addr(ctx.rom, addr, expected.len())
            .ok_or_else(|| GenRomProfileError::StructMismatch {
                id: id.to_owned(),
                reason: "the tiles run past the end of the ROM".to_owned(),
            })?
            .to_vec()
    };
    let tile_count = tile_count_of_prefix(id, &rom_tiles, &expected, TILE_BYTES)?;

    Ok(ImagePlan {
        id: id.to_owned(),
        addr,
        encoding: if is_compressed {
            Encoding::Lz77
        } else {
            Encoding::Raw
        },
        rom_bit_depth: 4,
        pack_bit_depth,
        width,
        height,
        metatile_width: 1,
        metatile_height: 1,
        tile_count,
    })
}

/// Check all 16 palette banks against the block the struct points at.
fn locate_palettes(
    ctx: &Context<'_>,
    name: &str,
    base: u32,
    report: &mut Vec<ReportLine>,
) -> Result<Vec<PalettePlan>, GenRomProfileError> {
    let mut plans = Vec::new();
    for bank in 0..PALETTE_BANKS {
        let id = format!("tileset/{name}/palette/{bank:02}");
        let asset = ctx.pack.get(&id)?;
        let color_count = asset.palette_colors(&id)?;
        let addr = base + bank * BANK_BYTES;
        let rom_bank = slice_at_addr(ctx.rom, addr, asset.payload.len());
        if rom_bank != Some(asset.payload.as_slice()) {
            return Err(GenRomProfileError::StructMismatch {
                id,
                reason: format!("bank {bank} of the palette block at {base:08X} differs"),
            });
        }
        // The block itself is a symbol; the 15 banks after the first are
        // addresses inside it.
        let mut line =
            ReportLine::unique(&id, addr, len32(&asset.payload)).with(Resolution::PointerWalk);
        line = if bank == 0 {
            line.symbol(format!("gTilesetPalettes_{}", camel_case(name)))
        } else {
            line.interior()
        };
        report.push(line);
        plans.push(PalettePlan {
            id,
            addr,
            color_count,
        });
    }
    Ok(plans)
}

/// Every animation's frame ids, grouped by animation name.
fn anim_frame_ids(ctx: &Context<'_>, name: &str) -> Vec<(String, Vec<String>)> {
    let prefix = format!("tileset/{name}/anim/");
    let mut grouped: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for id in ctx.pack.ids_with_prefix(&prefix) {
        let rest = &id[prefix.len()..];
        let Some((anim, _frame)) = rest.split_once('/') else {
            continue;
        };
        grouped.entry(anim.to_owned()).or_default().push(id.clone());
    }
    for frames in grouped.values_mut() {
        frames.sort_by_key(|id| frame_number(id));
    }
    grouped.into_iter().collect()
}

/// The trailing frame number of an animation frame id, for ordering.
fn frame_number(id: &str) -> u32 {
    id.rsplit('/')
        .next()
        .and_then(|tail| tail.parse().ok())
        .unwrap_or(u32::MAX)
}

/// Turn the animation frames' search hits into plans.
fn build_anims(
    ctx: &Context<'_>,
    anim_ids: &[(String, Vec<String>)],
    shapes: &[(u32, u32)],
    hits: &[Vec<u32>],
    report: &mut Vec<ReportLine>,
) -> Result<Vec<TileAnimPlan>, GenRomProfileError> {
    let mut plans = Vec::new();
    let mut cursor = 0usize;
    for (anim, frame_ids) in anim_ids {
        let mut frames = Vec::new();
        for id in frame_ids {
            let (width, height) = shapes[cursor];
            let addr = exactly_one(id, &hits[cursor])?;
            let pack_bit_depth = ctx.pack.get(id)?.image_shape(id)?.2;
            let tile_count = (width / 8) * (height / 8);
            report.push(
                ReportLine::unique(id, addr, tile_count * 32).symbol_contains(["gTilesetAnims_"]),
            );
            frames.push(ImagePlan {
                id: id.clone(),
                addr,
                encoding: Encoding::Raw,
                rom_bit_depth: 4,
                pack_bit_depth,
                width,
                height,
                metatile_width: 1,
                metatile_height: 1,
                tile_count,
            });
            cursor += 1;
        }
        plans.push(TileAnimPlan {
            name: anim.clone(),
            frames,
        });
    }
    Ok(plans)
}

/// A payload length as the `u32` the profile records.
pub fn len32(payload: &[u8]) -> u32 {
    u32::try_from(payload.len()).expect("a pack payload fits in u32")
}

#[cfg(test)]
mod tests {
    use super::frame_number;

    #[test]
    fn frames_order_numerically_not_lexically() {
        assert_eq!(frame_number("tileset/general/anim/water/7"), 7);
        assert_eq!(frame_number("tileset/general/anim/water/10"), 10);
        assert!(frame_number("tileset/general/anim/water/2") < frame_number("a/10"));
        // A frame that is not a number sorts last rather than panicking.
        assert_eq!(frame_number("a/b"), u32::MAX);
    }
}
