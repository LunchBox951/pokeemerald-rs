//! Locating the summary screen's sheets, palettes, and maps.
//!
//! The expectation is the checkout's fixed summary-screen inventory, which
//! extraction also reads
//! ([`crate::extract::scope::summary_screen_graphics_ids`]): four sheets, three
//! palettes, and nine maps. Every root is found from the pack payload alone; how
//! the ROM stores each is read from upstream's C (`src/graphics.c:1125-1134`,
//! `:1253-1255`, `src/pokemon_summary_screen.c:369`, `:725-728`, `:1097`):
//!
//! - The tile sheet and palette, the five page screens, and the move-select
//!   sheet and palette are `.lz` streams.
//! - The three move-effect maps, the status map, the markings palette, and the
//!   two button sheets are raw `INCBIN`s. The status map, the markings palette
//!   and the buttons are `static`.
//! - Nothing is cut: the tile sheet stores all 240 tiles of its raster, with the
//!   index narrowing already applied by the extractor.
//!
//! # Repeated bytes
//!
//! Every root is searched without a cap so that no copy is hidden. A root with
//! more than one copy is settled in this order:
//!
//! 1. The move-select sheet and palette are named by the record upstream writes
//!    for them (`sMoveSelectorSpriteSheet` `{data, size, tag}` and
//!    `sMoveSelectorSpritePal` `{data, tag}`, `src/pokemon_summary_screen.c:1002-1012`).
//!    A pointer alone, or a size or tag elsewhere, proves nothing, so a copy
//!    qualifies only when a pointer to it sits in a record whose own fields
//!    match. Exactly one copy must qualify.
//! 2. The two button sheets are the rows of one `sButtons_Gfx[2]` array, so
//!    they are searched as the 256 bytes the array holds and row `n` sits at
//!    `n * 128` past it; a button repeated alone cannot move a row.
//! 3. Any other root with several copies is [`Resolution::ArbitraryAmongIdentical`]
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
    BlobPlan, ImagePlan, PalettePlan, ReportLine, Resolution, SummaryScreenGraphicsPlan,
    SymbolExpectation,
};
use super::tilesets::len32;
use super::Context;

const TILE_BYTES: u32 = 32;
const COLOR_BYTES: u32 = 2;
/// A sprite-sheet record is `{data: u32, size: u16, tag: u16}`; a sprite-palette
/// record is `{data: u32, tag: u16}`.
const SHEET_SIZE_FIELD: u32 = 4;
const SHEET_TAG_FIELD: u32 = 6;
const PALETTE_TAG_FIELD: u32 = 4;
/// Bytes of one button sheet, the stride of `sButtons_Gfx`.
const BUTTON_BYTES: u32 = 4 * TILE_BYTES;

/// `TAG_MOVE_SELECTOR` (`src/pokemon_summary_screen.c:752`).
const MOVE_SELECTOR_TAG: u16 = 30000;

/// Whether upstream declares a root `static`, so a linker map cannot list it,
/// or the root sits inside another symbol.
#[derive(Clone, Copy)]
enum Declaration {
    Global(&'static str),
    Static(&'static str),
    Interior,
}

impl Declaration {
    fn expectation(self) -> SymbolExpectation {
        match self {
            Self::Global(name) => SymbolExpectation::Exact(name.to_owned()),
            Self::Static(name) => SymbolExpectation::Local(name.to_owned()),
            Self::Interior => SymbolExpectation::Interior,
        }
    }
}

/// What in the ROM, beyond the root's own bytes, decides between copies.
#[derive(Clone, Copy)]
enum Witness {
    /// Nothing upstream references the root alone.
    None,
    /// The `{data, size, tag}` sprite-sheet record that names a sheet.
    SpriteSheet { size: u16, tag: u16 },
    /// The `{data, tag}` sprite-palette record that names a palette.
    SpritePalette { tag: u16 },
    /// Row `n` of the two-row `sButtons_Gfx` array.
    ButtonRow(u32),
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
    witness: Witness,
}

const fn root(
    id: &'static str,
    shape: Shape,
    encoding: Encoding,
    declared: Declaration,
    witness: Witness,
) -> Root {
    Root {
        id,
        shape,
        encoding,
        declared,
        witness,
    }
}

const fn map(id: &'static str, len: u32, encoding: Encoding, declared: Declaration) -> Root {
    root(id, Shape::Map { len }, encoding, declared, Witness::None)
}

/// A 32x32 `u16` page screen, always an LZ77 stream.
const fn page(id: &'static str, symbol: &'static str) -> Root {
    map(id, 32 * 32 * 2, Encoding::Lz77, Declaration::Global(symbol))
}

const ROW_A: u32 = 0;
const ROW_B: u32 = 1;

/// Every root, in extraction order.
const ROOTS: [Root; 16] = [
    root(
        "summary-screen-graphics/image/tiles",
        Shape::Image {
            width: 128,
            height: 120,
            tiles: 240,
        },
        Encoding::Lz77,
        Declaration::Global("gSummaryScreen_Gfx"),
        Witness::None,
    ),
    root(
        "summary-screen-graphics/image/move_select",
        Shape::Image {
            width: 16,
            height: 128,
            tiles: 32,
        },
        Encoding::Lz77,
        Declaration::Global("gSummaryMoveSelect_Gfx"),
        Witness::SpriteSheet {
            size: 0x400,
            tag: MOVE_SELECTOR_TAG,
        },
    ),
    root(
        "summary-screen-graphics/image/a_button",
        Shape::Image {
            width: 16,
            height: 16,
            tiles: 4,
        },
        Encoding::Raw,
        Declaration::Static("sButtons_Gfx"),
        Witness::ButtonRow(ROW_A),
    ),
    root(
        "summary-screen-graphics/image/b_button",
        Shape::Image {
            width: 16,
            height: 16,
            tiles: 4,
        },
        Encoding::Raw,
        Declaration::Interior,
        Witness::ButtonRow(ROW_B),
    ),
    root(
        "summary-screen-graphics/palette/tiles",
        Shape::Palette { colors: 8 * 16 },
        Encoding::Lz77,
        Declaration::Global("gSummaryScreen_Pal"),
        Witness::None,
    ),
    root(
        "summary-screen-graphics/palette/move_select",
        Shape::Palette { colors: 16 },
        Encoding::Lz77,
        Declaration::Global("gSummaryMoveSelect_Pal"),
        Witness::SpritePalette {
            tag: MOVE_SELECTOR_TAG,
        },
    ),
    root(
        "summary-screen-graphics/palette/markings",
        Shape::Palette { colors: 16 },
        Encoding::Raw,
        Declaration::Static("sMarkings_Pal"),
        Witness::None,
    ),
    page(
        "summary-screen-graphics/raw/page_info",
        "gSummaryPage_Info_Tilemap",
    ),
    page(
        "summary-screen-graphics/raw/page_info_egg",
        "gSummaryPage_InfoEgg_Tilemap",
    ),
    page(
        "summary-screen-graphics/raw/page_skills",
        "gSummaryPage_Skills_Tilemap",
    ),
    page(
        "summary-screen-graphics/raw/page_battle_moves",
        "gSummaryPage_BattleMoves_Tilemap",
    ),
    page(
        "summary-screen-graphics/raw/page_contest_moves",
        "gSummaryPage_ContestMoves_Tilemap",
    ),
    map(
        "summary-screen-graphics/raw/effect_battle",
        80 * 2,
        Encoding::Raw,
        Declaration::Global("gSummaryScreen_MoveEffect_Battle_Tilemap"),
    ),
    map(
        "summary-screen-graphics/raw/effect_contest",
        80 * 2,
        Encoding::Raw,
        Declaration::Global("gSummaryScreen_MoveEffect_Contest_Tilemap"),
    ),
    map(
        "summary-screen-graphics/raw/effect_cancel",
        64 * 2,
        Encoding::Raw,
        Declaration::Global("gSummaryScreen_MoveEffect_Cancel_Tilemap"),
    ),
    map(
        "summary-screen-graphics/raw/status_tilemap",
        10 * 2 * 2,
        Encoding::Raw,
        Declaration::Static("sStatusTilemap"),
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

    /// The decompressed bytes the ROM must hold for this root; for a button,
    /// the whole `sButtons_Gfx` array both rows belong to.
    fn needle(&self, ctx: &Context<'_>) -> Result<Vec<u8>, GenRomProfileError> {
        match self.shape {
            Shape::Image { .. } if matches!(self.witness, Witness::ButtonRow(_)) => ROOTS
                .iter()
                .filter(|root| matches!(root.witness, Witness::ButtonRow(_)))
                .map(|root| image_tiles(ctx.pack, root.id, 4, (1, 1)))
                .collect::<Result<Vec<_>, _>>()
                .map(|rows| rows.concat()),
            Shape::Image { .. } => image_tiles(ctx.pack, self.id, 4, (1, 1)),
            Shape::Palette { .. } => {
                Ok(ctx.pack.get(self.id)?.palette_payload(self.id)?.0.to_vec())
            }
            Shape::Map { .. } => Ok(ctx.pack.raw_payload(self.id)?.to_vec()),
        }
    }
}

/// Locate every summary-screen root the checkout expects.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] for an expected entry the pack
/// lacks, [`GenRomProfileError::StructMismatch`] for a surplus
/// `summary-screen-graphics/` pack entry or a sheet or palette whose copies the
/// record does not narrow to exactly one, [`GenRomProfileError::WrongPackEntryKind`]
/// or [`GenRomProfileError::EntryShape`] for a malformed entry, and
/// [`GenRomProfileError::NotFound`] for a root the ROM does not hold.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<SummaryScreenGraphicsPlan, GenRomProfileError> {
    inventory::require_summary_screen_graphics(ctx.pack)?;
    validate_shapes(ctx)?;
    let copies = find_copies(ctx)?;
    let mut plan = SummaryScreenGraphicsPlan {
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
/// the shape upstream stores (a 16-colour tile palette, a 140-byte effect map
/// cut to the window it draws); the located bytes would then describe a
/// different asset.
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
                        "is {found_width}x{found_height} at {depth} bits: the summary screen \
                         stores {width}x{height} at 4 bits"
                    )));
                }
                debug_assert_eq!(width / 8 * (height / 8), tiles);
            }
            Shape::Palette { colors } => {
                let (payload, found) = ctx.pack.get(id)?.palette_payload(id)?;
                if found != colors || len32(payload) != u32::from(colors) * COLOR_BYTES {
                    return Err(mismatch(format!(
                        "holds {found} colours: the summary screen stores {colors}"
                    )));
                }
            }
            Shape::Map { len } => {
                let found = ctx.pack.raw_payload(id)?.len();
                if found != len as usize {
                    return Err(mismatch(format!(
                        "is {found} bytes: the summary screen stores {len}"
                    )));
                }
            }
        }
    }
    Ok(())
}

/// Every address each root's bytes turn up at, in [`ROOTS`] order, ascending.
/// A button's addresses are those of the array holding both rows.
///
/// One raw and one LZ77 pass cover all sixteen. Uncapped, so a sheet copied
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

/// Whether a pointer to `target` sits in a record `witness` describes.
fn named_by_record(ctx: &Context<'_>, target: u32, witness: Witness) -> bool {
    ctx.pointers
        .refs_to(target)
        .iter()
        .any(|&pointer| match witness {
            Witness::SpriteSheet { size, tag } => {
                u16_at_addr(ctx.rom, pointer + SHEET_SIZE_FIELD) == Some(size)
                    && u16_at_addr(ctx.rom, pointer + SHEET_TAG_FIELD) == Some(tag)
            }
            Witness::SpritePalette { tag } => {
                u16_at_addr(ctx.rom, pointer + PALETTE_TAG_FIELD) == Some(tag)
            }
            Witness::None | Witness::ButtonRow(_) => false,
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
    let (addr, resolution, note) = match root.witness {
        Witness::SpriteSheet { size, tag } => {
            let addr = only_one_matching(
                root.id,
                "a {data, size, tag} sprite-sheet record pointing at the copy",
                copies.iter().copied(),
                |&copy| named_by_record(ctx, copy, root.witness),
            )?;
            let note = format!(
                "named by a sprite-sheet record with size {size} and tag {tag}{}",
                repeats(copies.len()),
            );
            (addr, Resolution::StructDerived, Some(note))
        }
        Witness::SpritePalette { tag } => {
            let addr = only_one_matching(
                root.id,
                "a {data, tag} sprite-palette record pointing at the copy",
                copies.iter().copied(),
                |&copy| named_by_record(ctx, copy, root.witness),
            )?;
            let note = format!(
                "named by a sprite-palette record with tag {tag}{}",
                repeats(copies.len()),
            );
            (addr, Resolution::StructDerived, Some(note))
        }
        Witness::ButtonRow(row) if copies.len() == 1 => {
            let note = format!("row {row} of sButtons_Gfx[2], found as the whole 2-row array");
            (
                lowest + row * BUTTON_BYTES,
                Resolution::StructDerived,
                Some(note),
            )
        }
        Witness::None if copies.len() == 1 => (lowest, Resolution::UniqueSignature, None),
        Witness::None | Witness::ButtonRow(_) => {
            let row = match root.witness {
                Witness::ButtonRow(row) => row,
                _ => 0,
            };
            let note = format!(
                "the bytes{} hold identical data and nothing distinguishes them, so this is the \
                 lowest",
                repeats(copies.len()),
            );
            (
                lowest + row * BUTTON_BYTES,
                Resolution::ArbitraryAmongIdentical,
                Some(note),
            )
        }
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
#[path = "summary_screen_graphics_tests.rs"]
mod tests;
