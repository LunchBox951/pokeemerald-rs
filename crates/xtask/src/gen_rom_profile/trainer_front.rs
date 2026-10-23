//! Locating the Birch-speech trainer front pictures (`src/data/trainer_graphics`).
//!
//! The pack's `trainer-pic/{brendan,may}/{front,palette}` entries are the
//! expectation. The ROM stores each as an LZ77 stream, and two 8-byte-row
//! tables index them by `TRAINER_PIC_*`: `gTrainerFrontPicTable`
//! (`{data, size, tag}`) and `gTrainerFrontPicPaletteTable` (`{data, tag}`).
//! The object-event sprite tables name no trainer picture and are not evidence.
//!
//! The walk:
//!
//! 1. Images are found by their decompressed tiles ([`super::images`]).
//!    Palettes are found by their decompressed colours, which can match more
//!    than one stream.
//! 2. Each table is found through the pointers to the art: biased back by
//!    each trainer's own row, they name candidate table bases, and exactly one
//!    base must hold, at both rows, a pointer to that trainer's art and the
//!    size and tag upstream's `TRAINER_SPRITE` and `TRAINER_PAL` macros write.
//! 3. A palette's address is the pointer its table row holds, which settles
//!    a palette the ROM stores more than once.

use std::collections::BTreeSet;

use rom_import::Encoding;

use super::error::GenRomProfileError;
use super::images::{locate_images, ImageQuery};
use super::inventory;
use super::locate::{camel_case, only_one_matching, to_addr, u16_at_addr, u32_at_addr};
use super::plan::{
    PalettePlan, ReportLine, Resolution, SymbolExpectation, TrainerFrontPicPlan, TrainerFrontPlan,
};
use super::tilesets::len32;
use super::Context;
use crate::extract::scope::TRAINER_FRONT_PICS;

/// One table row, in bytes.
const RECORD_BYTES: u32 = 8;
/// How many rows each table holds: `TRAINER_PIC_RS_MAY + 1`.
const TABLE_ROWS: u32 = 93;
/// `TRAINER_PIC_SIZE`, the `size` of a front picture row: 64x64 pixels at 4bpp.
const PICTURE_SIZE: u16 = 0x800;
/// Offset of a picture row's `size` field.
const PICTURE_SIZE_FIELD: u32 = 4;
/// Offset of a picture row's `tag` field.
const PICTURE_TAG_FIELD: u32 = 6;
/// Offset of a palette row's `tag` field.
const PALETTE_TAG_FIELD: u32 = 4;

/// What a table's row for one trainer picture must carry besides the pointer.
#[derive(Clone, Copy)]
enum Row {
    /// `{data, size: TRAINER_PIC_SIZE, tag: trainer_pic}`.
    Picture,
    /// `{data, tag: trainer_pic}`.
    Palette,
}

impl Row {
    fn holds(self, ctx: &Context<'_>, row: u32, trainer_pic: u16) -> bool {
        match self {
            Self::Picture => {
                u16_at_addr(ctx.rom, row + PICTURE_SIZE_FIELD) == Some(PICTURE_SIZE)
                    && u16_at_addr(ctx.rom, row + PICTURE_TAG_FIELD) == Some(trainer_pic)
            }
            Self::Palette => u16_at_addr(ctx.rom, row + PALETTE_TAG_FIELD) == Some(trainer_pic),
        }
    }
}

/// Locate both trainer pictures' images, palettes, and the two tables.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] for an absent pack entry;
/// [`GenRomProfileError::NotFound`] for art the ROM does not hold;
/// [`GenRomProfileError::StructMismatch`] if a table cannot be pinned to
/// exactly one base.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<TrainerFrontPlan, GenRomProfileError> {
    inventory::require_trainer_front(ctx.pack)?;

    let queries: Vec<ImageQuery> = TRAINER_FRONT_PICS
        .iter()
        .map(|pic| ImageQuery::lz77(pic.image_id))
        .collect();
    let first_line = report.len();
    let images = locate_images(ctx, &queries, report)?;
    name_image_symbols(&mut report[first_line..]);
    let palette_candidates = palette_candidates(ctx)?;

    let picture_rows: Vec<(u16, Vec<u32>)> = TRAINER_FRONT_PICS
        .iter()
        .zip(&images)
        .map(|(pic, image)| (u16::from(pic.trainer_pic.index()), vec![image.addr]))
        .collect();
    let palette_rows: Vec<(u16, Vec<u32>)> = TRAINER_FRONT_PICS
        .iter()
        .zip(&palette_candidates)
        .map(|(pic, addrs)| (u16::from(pic.trainer_pic.index()), addrs.clone()))
        .collect();
    let front_table = find_table(
        ctx,
        "gTrainerFrontPicTable",
        &picture_rows,
        Row::Picture,
        report,
    )?;
    let palette_table = find_table(
        ctx,
        "gTrainerFrontPicPaletteTable",
        &palette_rows,
        Row::Palette,
        report,
    )?;

    let mut trainers = Vec::with_capacity(TRAINER_FRONT_PICS.len());
    for (index, pic) in TRAINER_FRONT_PICS.iter().enumerate() {
        let trainer_pic = u16::from(pic.trainer_pic.index());
        let palette = palette_plan(
            ctx,
            pic.palette_id,
            row_pointer(ctx, palette_table, trainer_pic),
            &palette_candidates[index],
            &format!("gTrainerPalette_{}", camel_case(pic.name)),
            report,
        )?;
        trainers.push(TrainerFrontPicPlan {
            trainer_pic,
            front: images[index].clone(),
            palette,
        });
    }

    Ok(TrainerFrontPlan {
        front_table,
        palette_table,
        trainers,
    })
}

/// Require a linker map to name each front picture `gTrainerFrontPic_<Name>`.
///
/// The shared image classifier knows no trainer naming convention, so the
/// expectation is set here on the lines [`locate_images`] just reported.
fn name_image_symbols(lines: &mut [ReportLine]) {
    for pic in &TRAINER_FRONT_PICS {
        if let Some(line) = lines.iter_mut().find(|line| line.id == pic.image_id) {
            line.symbol =
                SymbolExpectation::Exact(format!("gTrainerFrontPic_{}", camel_case(pic.name)));
        }
    }
}

/// Every address each palette's decompressed colours turn up at, Brendan then May.
fn palette_candidates(ctx: &Context<'_>) -> Result<Vec<Vec<u32>>, GenRomProfileError> {
    let mut needles = Vec::with_capacity(TRAINER_FRONT_PICS.len());
    for pic in &TRAINER_FRONT_PICS {
        needles.push(
            ctx.pack
                .get(pic.palette_id)?
                .palette_payload(pic.palette_id)?
                .0
                .to_vec(),
        );
    }
    let hits = ctx.lz77.find_all(&needles);
    TRAINER_FRONT_PICS
        .iter()
        .zip(&hits)
        .map(|(pic, offsets)| {
            if offsets.is_empty() {
                return Err(GenRomProfileError::NotFound {
                    id: pic.palette_id.to_owned(),
                });
            }
            Ok(offsets.iter().copied().map(to_addr).collect())
        })
        .collect()
}

/// The pointer in `trainer_pic`'s row of the table at `table`.
fn row_pointer(ctx: &Context<'_>, table: u32, trainer_pic: u16) -> Option<u32> {
    u32_at_addr(ctx.rom, table + u32::from(trainer_pic) * RECORD_BYTES)
}

/// The palette plan for the stream a table row points at.
fn palette_plan(
    ctx: &Context<'_>,
    id: &str,
    row_target: Option<u32>,
    candidates: &[u32],
    symbol: &str,
    report: &mut Vec<ReportLine>,
) -> Result<PalettePlan, GenRomProfileError> {
    let addr = row_target
        .filter(|addr| candidates.contains(addr))
        .ok_or_else(|| GenRomProfileError::NotFound { id: id.to_owned() })?;
    let asset = ctx.pack.get(id)?;
    let mut line = ReportLine::unique(id, addr, len32(&asset.payload)).symbol(symbol);
    if candidates.len() > 1 {
        line = line.with(Resolution::StructDerived).note(format!(
            "{} identical streams in the ROM; the palette table names this one",
            candidates.len()
        ));
    }
    report.push(line);
    Ok(PalettePlan {
        id: id.to_owned(),
        addr,
        encoding: Encoding::Lz77,
        color_count: asset.palette_colors(id)?,
    })
}

/// Find the one table whose every trainer row points at that trainer's art.
fn find_table(
    ctx: &Context<'_>,
    name: &'static str,
    rows: &[(u16, Vec<u32>)],
    shape: Row,
    report: &mut Vec<ReportLine>,
) -> Result<u32, GenRomProfileError> {
    let Some((first_trainer, first_art)) = rows.first() else {
        return Err(GenRomProfileError::StructMismatch {
            id: name.to_owned(),
            reason: "no trainer picture to anchor the table".to_owned(),
        });
    };
    let first_bias = u32::from(*first_trainer) * RECORD_BYTES;
    let bases: BTreeSet<u32> = first_art
        .iter()
        .flat_map(|&art| ctx.pointers.refs_to(art).iter().copied())
        .filter_map(|at| at.checked_sub(first_bias))
        .collect();
    let base = only_one_matching(
        name,
        "every trainer row pointing at its art with its tag",
        bases,
        |&base| {
            rows.iter().all(|(trainer_pic, art)| {
                let row = base + u32::from(*trainer_pic) * RECORD_BYTES;
                u32_at_addr(ctx.rom, row).is_some_and(|ptr| art.contains(&ptr))
                    && shape.holds(ctx, row, *trainer_pic)
            })
        },
    )?;
    report.push(
        ReportLine::unique(name, base, TABLE_ROWS * RECORD_BYTES)
            .with(Resolution::StructDerived)
            .symbol(name)
            .note("anchored by the pointers from its Birch-speech rows"),
    );
    Ok(base)
}

#[cfg(test)]
mod tests;
