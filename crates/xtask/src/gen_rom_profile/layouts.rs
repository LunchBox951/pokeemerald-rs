//! Locating the bundled map layouts.
//!
//! A layout's `map.bin` is unique, so it is searched for directly. Its
//! `border.bin` is four `u16` cells that repeat throughout the ROM, so the
//! border address is read from the `struct MapLayout` that points at the map
//! grid.
//!
//! `mapjson` generates layout symbols at build time, so a `--map` cross-check
//! asserts only that some symbol starts at each address, not which.
//!
//! The struct field order is `pokeemerald/include/global.fieldmap.h`'s:
//! `width`, `height`, `border`, `map`, then the two tilesets. Some upstream
//! `map.bin` files carry trailing padding beyond `width * height * 2`, so
//! `width` and `height` bound the grid from below rather than equalling it.
//!
//! A pack with no `layout/*/map` entries is refused rather than treated as
//! an empty domain.

use rom_import::Encoding;

use super::error::GenRomProfileError;
use super::locate::{exactly_one, only_one_matching, slice_at_addr, to_offset, u32_at_addr};
use super::plan::{BlobPlan, MapLayoutPlan, ReportLine, Resolution};
use super::tilesets::len32;
use super::Context;

/// Offset of the `width` field in `struct MapLayout`.
const FIELD_WIDTH: u32 = 0x00;
/// Offset of the `height` field.
const FIELD_HEIGHT: u32 = 0x04;
/// Offset of the `border` field.
const FIELD_BORDER: u32 = 0x08;
/// Offset of the `map` field.
const FIELD_MAP: u32 = 0x0C;
/// Offset of the `primaryTileset` field.
const FIELD_PRIMARY_TILESET: u32 = 0x10;
/// Offset of the `secondaryTileset` field.
const FIELD_SECONDARY_TILESET: u32 = 0x14;
/// Size of `struct MapLayout`.
const MAP_LAYOUT_BYTES: u32 = 0x18;
/// Each map grid cell is one `u16` metatile entry.
const MAP_CELL_BYTES: usize = 2;

/// Upper bound on the struct's `width` and `height`, to reject garbage
/// words.
const MAX_LAYOUT_SIDE: u32 = 1024;

/// Locate every map layout the pack holds.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] naming the first fixed layout
/// map or border the pack lacks, or any other [`GenRomProfileError`] a
/// locator raises.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<Vec<MapLayoutPlan>, GenRomProfileError> {
    super::inventory::require_layouts(ctx.pack)?;
    let names = layout_names(ctx);
    let mut needles = Vec::with_capacity(names.len());
    for name in &names {
        needles.push(ctx.pack.get(&format!("layout/{name}/map"))?.payload.clone());
    }
    let hits = ctx.raw.find_all(&needles);

    let mut plans = Vec::new();
    for (index, name) in names.iter().enumerate() {
        plans.push(locate_one(ctx, name, &hits[index], report)?);
    }
    Ok(plans)
}

/// Every layout name in the pack, ascending.
fn layout_names(ctx: &Context<'_>) -> Vec<String> {
    ctx.pack
        .ids_with_prefix("layout/")
        .into_iter()
        .filter_map(|id| id.strip_suffix("/map").map(str::to_owned))
        .filter_map(|id| id.strip_prefix("layout/").map(str::to_owned))
        .collect()
}

fn locate_one(
    ctx: &Context<'_>,
    name: &str,
    map_hits: &[u32],
    report: &mut Vec<ReportLine>,
) -> Result<MapLayoutPlan, GenRomProfileError> {
    let map_id = format!("layout/{name}/map");
    let border_id = format!("layout/{name}/border");
    let map_bytes = &ctx.pack.get(&map_id)?.payload;
    let border_bytes = &ctx.pack.get(&border_id)?.payload;

    let map_addr = exactly_one(&map_id, map_hits)?;
    let struct_addr = only_one_matching(
        name,
        "the `struct MapLayout` layout",
        ctx.pointers
            .refs_to(map_addr)
            .iter()
            .filter_map(|at| at.checked_sub(FIELD_MAP)),
        |&base| is_map_layout(ctx, base, map_bytes.len(), border_bytes),
    )?;

    let width = u32_at_addr(ctx.rom, struct_addr + FIELD_WIDTH).unwrap_or(0);
    let height = u32_at_addr(ctx.rom, struct_addr + FIELD_HEIGHT).unwrap_or(0);
    let border_addr = u32_at_addr(ctx.rom, struct_addr + FIELD_BORDER).unwrap_or(0);
    let primary_tileset = u32_at_addr(ctx.rom, struct_addr + FIELD_PRIMARY_TILESET).unwrap_or(0);
    let secondary_tileset =
        u32_at_addr(ctx.rom, struct_addr + FIELD_SECONDARY_TILESET).unwrap_or(0);

    report.push(
        ReportLine::unique(format!("layout/{name}"), struct_addr, MAP_LAYOUT_BYTES)
            .with(Resolution::StructDerived)
            .note("found through its map grid"),
    );
    report.push(ReportLine::unique(&map_id, map_addr, len32(map_bytes)));
    report.push(
        ReportLine::unique(&border_id, border_addr, len32(border_bytes))
            .with(Resolution::StructDerived)
            .note("four cells repeat everywhere; taken from the layout struct"),
    );

    Ok(MapLayoutPlan {
        name: name.to_owned(),
        struct_addr,
        width,
        height,
        map: BlobPlan {
            id: map_id,
            addr: map_addr,
            encoding: Encoding::Raw,
            len: len32(map_bytes),
        },
        border: BlobPlan {
            id: border_id,
            addr: border_addr,
            encoding: Encoding::Raw,
            len: len32(border_bytes),
        },
        primary_tileset,
        secondary_tileset,
    })
}

/// Whether the bytes at `base` read as the `struct MapLayout` for this grid.
fn is_map_layout(ctx: &Context<'_>, base: u32, map_len: usize, border: &[u8]) -> bool {
    let Some(width) = u32_at_addr(ctx.rom, base + FIELD_WIDTH) else {
        return false;
    };
    let Some(height) = u32_at_addr(ctx.rom, base + FIELD_HEIGHT) else {
        return false;
    };
    if width == 0 || height == 0 || width > MAX_LAYOUT_SIDE || height > MAX_LAYOUT_SIDE {
        return false;
    }
    if (width as usize) * (height as usize) * MAP_CELL_BYTES > map_len {
        return false;
    }
    let Some(border_addr) = u32_at_addr(ctx.rom, base + FIELD_BORDER) else {
        return false;
    };
    if slice_at_addr(ctx.rom, border_addr, border.len()) != Some(border) {
        return false;
    }
    [FIELD_PRIMARY_TILESET, FIELD_SECONDARY_TILESET]
        .iter()
        .all(|field| {
            u32_at_addr(ctx.rom, base + field).is_some_and(|value| to_offset(value).is_some())
        })
}
