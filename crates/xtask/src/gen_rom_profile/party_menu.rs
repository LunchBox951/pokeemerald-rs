//! Locating the party menu's sheets, palettes, and maps.
//!
//! The expectation is the checkout's fixed party-menu inventory, which
//! extraction also reads ([`crate::extract::scope::party_menu_ids`]): three
//! sheets, three palettes, and eight maps. Every root is found from the pack
//! payload alone; how the ROM stores each is read from upstream's C
//! (`src/graphics.c:1111-1117`, `src/data/party_menu.h`):
//!
//! - The background sheet, palette and screen, and the menu Poké Ball sheet
//!   and palette, are `.lz` streams.
//! - The held-item sheet and palette, the five `u8` slot maps, and the two
//!   button maps are raw `INCBIN`s, all `static`.
//! - The background sheet is built with `-num_tiles 62`, so the ROM stores 62
//!   tiles, which the pack publishes as an 8x496 strip. The tiles are searched
//!   in plain tile order, since a strip has no metatile shape.
//!
//! # Repeated bytes
//!
//! Every root is searched without a cap so that no copy is hidden. A root with
//! more than one copy is settled in this order:
//!
//! 1. The held-item and Poké Ball sheets are named by the whole sprite-sheet
//!    record upstream writes for them (`data`, `size`, `tag`). A pointer alone,
//!    or a size or tag elsewhere, proves nothing, so a copy qualifies only when
//!    a pointer to it sits in a record whose own size and tag fields match.
//!    Exactly one copy must qualify.
//! 2. Any other root with several copies is [`Resolution::ArbitraryAmongIdentical`]
//!    at the lowest address: every copy decodes to the same pack entry, and
//!    nothing in the ROM tells them apart.
//!
//! The planner loads no ELF symbols, so a map label never decides a copy.

use rom_import::Encoding;

use super::error::GenRomProfileError;
use super::inventory;
use super::locate::{only_one_matching, to_addr, u16_at_addr};
use super::pack_source::image_tiles;
use super::plan::{
    BlobPlan, ImagePlan, PalettePlan, PartyMenuPlan, ReportLine, Resolution, SymbolExpectation,
};
use super::tilesets::len32;
use super::Context;

const TILE_BYTES: u32 = 32;
const COLOR_BYTES: u32 = 2;
/// A sprite-sheet record is `{data: u32, size: u16, tag: u16}`.
const SHEET_SIZE_FIELD: u32 = 4;
const SHEET_TAG_FIELD: u32 = 6;

/// Whether upstream declares a root `static`, so a linker map cannot list it.
#[derive(Clone, Copy)]
enum Declaration {
    Global(&'static str),
    Static(&'static str),
}

impl Declaration {
    fn expectation(self) -> SymbolExpectation {
        match self {
            Self::Global(name) => SymbolExpectation::Exact(name.to_owned()),
            Self::Static(name) => SymbolExpectation::Local(name.to_owned()),
        }
    }
}

/// The `{data, size, tag}` sprite-sheet record that names a sheet.
#[derive(Clone, Copy)]
struct SheetRecord {
    size: u16,
    tag: u16,
}

/// What the pack must hold for a root, and so what the ROM stores.
#[derive(Clone, Copy)]
enum Shape {
    /// A 4bpp sheet of `tiles` tiles in a `width` x `height` raster.
    Image { width: u32, height: u32, tiles: u32 },
    /// A palette of this many colours.
    Palette { colors: u16 },
    /// A raw map of this many bytes.
    Map { len: u32 },
}

struct Root {
    id: &'static str,
    shape: Shape,
    encoding: Encoding,
    declared: Declaration,
    /// The record that decides between copies of the root, if upstream has one.
    record: Option<SheetRecord>,
}

const fn root(
    id: &'static str,
    shape: Shape,
    encoding: Encoding,
    declared: Declaration,
    record: Option<SheetRecord>,
) -> Root {
    Root {
        id,
        shape,
        encoding,
        declared,
        record,
    }
}

const fn map(id: &'static str, len: u32, encoding: Encoding, declared: Declaration) -> Root {
    root(id, Shape::Map { len }, encoding, declared, None)
}

/// `TAG_HELD_ITEM` (`src/party_menu.c:148`).
const HELD_ITEM_TAG: u16 = 55120;
/// `TAG_POKEBALL` (`src/party_menu.c:143`).
const POKEBALL_TAG: u16 = 1200;

/// Every root, in extraction order. `sSpriteSheet_HeldItem`
/// (`src/data/party_menu.h:839-842`) and `sSpriteSheet_MenuPokeball`
/// (`:895-898`) are the records that name the two sheets.
const ROOTS: [Root; 14] = [
    root(
        "party-menu/image/bg",
        Shape::Image {
            width: 8,
            height: 62 * 8,
            tiles: 62,
        },
        Encoding::Lz77,
        Declaration::Global("gPartyMenuBg_Gfx"),
        None,
    ),
    root(
        "party-menu/image/pokeball",
        Shape::Image {
            width: 32,
            height: 64,
            tiles: 32,
        },
        Encoding::Lz77,
        Declaration::Global("gPartyMenuPokeball_Gfx"),
        Some(SheetRecord {
            size: 0x400,
            tag: POKEBALL_TAG,
        }),
    ),
    root(
        "party-menu/image/hold_icons",
        Shape::Image {
            width: 8,
            height: 16,
            tiles: 2,
        },
        Encoding::Raw,
        Declaration::Static("sHeldItemGfx"),
        Some(SheetRecord {
            size: 64,
            tag: HELD_ITEM_TAG,
        }),
    ),
    root(
        "party-menu/palette/bg",
        Shape::Palette { colors: 11 * 16 },
        Encoding::Lz77,
        Declaration::Global("gPartyMenuBg_Pal"),
        None,
    ),
    root(
        "party-menu/palette/pokeball",
        Shape::Palette { colors: 16 },
        Encoding::Lz77,
        Declaration::Global("gPartyMenuPokeball_Pal"),
        None,
    ),
    root(
        "party-menu/palette/hold_icons",
        Shape::Palette { colors: 16 },
        Encoding::Raw,
        Declaration::Static("sHeldItemPalette"),
        None,
    ),
    map(
        "party-menu/raw/bg",
        32 * 32 * 2,
        Encoding::Lz77,
        Declaration::Global("gPartyMenuBg_Tilemap"),
    ),
    map(
        "party-menu/raw/slot_main",
        10 * 7,
        Encoding::Raw,
        Declaration::Static("sSlotTilemap_Main"),
    ),
    map(
        "party-menu/raw/slot_main_no_hp",
        10 * 7,
        Encoding::Raw,
        Declaration::Static("sSlotTilemap_MainNoHP"),
    ),
    map(
        "party-menu/raw/slot_wide",
        18 * 3,
        Encoding::Raw,
        Declaration::Static("sSlotTilemap_Wide"),
    ),
    map(
        "party-menu/raw/slot_wide_no_hp",
        18 * 3,
        Encoding::Raw,
        Declaration::Static("sSlotTilemap_WideNoHP"),
    ),
    map(
        "party-menu/raw/slot_wide_empty",
        18 * 3,
        Encoding::Raw,
        Declaration::Static("sSlotTilemap_WideEmpty"),
    ),
    map(
        "party-menu/raw/confirm_button",
        7 * 2 * 2,
        Encoding::Raw,
        Declaration::Static("sConfirmButton_Tilemap"),
    ),
    map(
        "party-menu/raw/cancel_button",
        7 * 2 * 2,
        Encoding::Raw,
        Declaration::Static("sCancelButton_Tilemap"),
    ),
];

impl Root {
    /// The decompressed length the ROM yields for this root.
    const fn len(&self) -> u32 {
        match self.shape {
            Shape::Image { tiles, .. } => tiles * TILE_BYTES,
            Shape::Palette { colors } => colors as u32 * COLOR_BYTES,
            Shape::Map { len } => len,
        }
    }

    /// The decompressed bytes the ROM must hold for this root.
    fn needle(&self, ctx: &Context<'_>) -> Result<Vec<u8>, GenRomProfileError> {
        match self.shape {
            Shape::Image { .. } => image_tiles(ctx.pack, self.id, 4, (1, 1)),
            Shape::Palette { .. } => {
                Ok(ctx.pack.get(self.id)?.palette_payload(self.id)?.0.to_vec())
            }
            Shape::Map { .. } => Ok(ctx.pack.raw_payload(self.id)?.to_vec()),
        }
    }
}

/// Locate every party-menu root the checkout expects.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] for an expected entry the pack
/// lacks, [`GenRomProfileError::StructMismatch`] for a surplus `party-menu/`
/// pack entry or a sheet whose copies the sprite-sheet record does not narrow
/// to exactly one, [`GenRomProfileError::WrongPackEntryKind`] or
/// [`GenRomProfileError::EntryShape`] for a malformed entry, and
/// [`GenRomProfileError::NotFound`] for a root the ROM does not hold.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<PartyMenuPlan, GenRomProfileError> {
    inventory::require_party_menu(ctx.pack)?;
    validate_shapes(ctx)?;
    let copies = find_copies(ctx)?;
    let mut plan = PartyMenuPlan {
        images: Vec::new(),
        palettes: Vec::new(),
        tilemaps: Vec::new(),
    };
    for (root, copies) in ROOTS.iter().zip(copies) {
        let (addr, line) = settle(ctx, root, &copies)?;
        report.push(line);
        match root.shape {
            Shape::Image {
                width,
                height,
                tiles,
            } => plan.images.push(ImagePlan {
                id: root.id.to_owned(),
                addr,
                encoding: root.encoding,
                rom_bit_depth: 4,
                pack_bit_depth: 4,
                width,
                height,
                metatile_width: 1,
                metatile_height: 1,
                tile_count: tiles,
            }),
            Shape::Palette { colors } => plan.palettes.push(PalettePlan {
                id: root.id.to_owned(),
                addr,
                color_count: colors,
                encoding: root.encoding,
            }),
            Shape::Map { len } => plan.tilemaps.push(BlobPlan {
                id: root.id.to_owned(),
                addr,
                encoding: root.encoding,
                len,
            }),
        }
    }
    Ok(plan)
}

/// Refuse a pack entry that is not the shape the profile table will declare,
/// before any report line is written.
///
/// A pack built from another extractor could hold a valid entry that is not
/// the cut upstream stores (the 64-tile 8-bit background, a 16-colour
/// background palette, a slot map of entries rather than bytes); the located
/// bytes would then describe a different asset.
fn validate_shapes(ctx: &Context<'_>) -> Result<(), GenRomProfileError> {
    for root in &ROOTS {
        let id = root.id;
        let mismatch = |reason: String| GenRomProfileError::EntryShape {
            id: id.to_owned(),
            reason,
        };
        match root.shape {
            Shape::Image {
                width,
                height,
                tiles,
            } => {
                let (_, found_width, found_height, depth) = ctx.pack.get(id)?.image_raster(id)?;
                if (found_width, found_height, depth) != (width, height, 4) {
                    return Err(mismatch(format!(
                        "is {found_width}x{found_height} at {depth} bits: the party menu stores \
                         {width}x{height} at 4 bits"
                    )));
                }
                debug_assert_eq!(width / 8 * (height / 8), tiles);
            }
            Shape::Palette { colors } => {
                let (payload, found) = ctx.pack.get(id)?.palette_payload(id)?;
                if found != colors || len32(payload) != u32::from(colors) * COLOR_BYTES {
                    return Err(mismatch(format!(
                        "holds {found} colours: the party menu stores {colors}"
                    )));
                }
            }
            Shape::Map { len } => {
                let found = ctx.pack.raw_payload(id)?.len();
                if found != len as usize {
                    return Err(mismatch(format!(
                        "is {found} bytes: the party menu stores {len}"
                    )));
                }
            }
        }
    }
    Ok(())
}

/// Every address each root's bytes turn up at, in [`ROOTS`] order, ascending.
///
/// One raw and one LZ77 pass cover all fourteen. Uncapped, so a sheet copied
/// more often than the usual cap still shows the copy its record names.
fn find_copies(ctx: &Context<'_>) -> Result<Vec<Vec<u32>>, GenRomProfileError> {
    let mut copies = vec![Vec::new(); ROOTS.len()];
    for encoding in [Encoding::Raw, Encoding::Lz77] {
        let group: Vec<usize> = (0..ROOTS.len())
            .filter(|&index| ROOTS[index].encoding == encoding)
            .collect();
        let needles = group
            .iter()
            .map(|&index| ROOTS[index].needle(ctx))
            .collect::<Result<Vec<_>, _>>()?;
        let hits = match encoding {
            Encoding::Raw => ctx.raw.find_all_uncapped(&needles),
            Encoding::Lz77 => ctx.lz77.find_all_uncapped(&needles),
        };
        for (&index, offsets) in group.iter().zip(hits) {
            copies[index] = offsets.into_iter().map(to_addr).collect();
        }
    }
    Ok(copies)
}

/// Whether a pointer to `target` sits in a record carrying `record`'s size
/// and tag.
fn named_by_record(ctx: &Context<'_>, target: u32, record: SheetRecord) -> bool {
    ctx.pointers.refs_to(target).iter().any(|&pointer| {
        u16_at_addr(ctx.rom, pointer + SHEET_SIZE_FIELD) == Some(record.size)
            && u16_at_addr(ctx.rom, pointer + SHEET_TAG_FIELD) == Some(record.tag)
    })
}

/// The address to record for `root` among its `copies`, and its report line.
fn settle(
    ctx: &Context<'_>,
    root: &Root,
    copies: &[u32],
) -> Result<(u32, ReportLine), GenRomProfileError> {
    let Some(&lowest) = copies.first() else {
        return Err(GenRomProfileError::NotFound {
            id: root.id.to_owned(),
        });
    };
    let (addr, resolution, note) = if let Some(record) = root.record {
        let addr = only_one_matching(
            root.id,
            "a {data, size, tag} sprite-sheet record pointing at the copy",
            copies.iter().copied(),
            |&copy| named_by_record(ctx, copy, record),
        )?;
        let note = format!(
            "named by a sprite-sheet record with size {} and tag {}{}",
            record.size,
            record.tag,
            repeats(copies.len()),
        );
        (addr, Resolution::StructDerived, Some(note))
    } else if copies.len() == 1 {
        (lowest, Resolution::UniqueSignature, None)
    } else {
        let note = format!(
            "the bytes{} hold identical data and nothing distinguishes them, so this is the \
             lowest",
            repeats(copies.len()),
        );
        (lowest, Resolution::ArbitraryAmongIdentical, Some(note))
    };
    let mut line = ReportLine::unique(root.id, addr, root.len())
        .with(resolution)
        .with_symbol(root.declared.expectation());
    if let Some(note) = note {
        line = line.note(note);
    }
    Ok((addr, line))
}

/// The suffix saying how many copies the bytes had, when more than one.
fn repeats(copies: usize) -> String {
    if copies > 1 {
        format!(" (the bytes repeat at {copies} addresses)")
    } else {
        String::new()
    }
}

#[cfg(test)]
#[path = "party_menu_tests.rs"]
mod tests;
