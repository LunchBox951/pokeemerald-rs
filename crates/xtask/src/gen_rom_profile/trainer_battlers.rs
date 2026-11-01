//! Locating the Brendan and May trainer battlers (`src/data/trainer_graphics`).
//!
//! The pack's `trainer/{brendan,may}/{front,back,palette}` entries are the
//! expectation. Two halves, with two index spaces:
//!
//! - The front picture and palette are the ROM art the portraits already
//!   located ([`super::trainer_front`]), indexed by `TRAINER_PIC_*`. The
//!   battler entries must carry the very bytes of the portrait entries; they
//!   are then the same plans under the battler ids, rather than a second
//!   search that would find the one ROM copy twice.
//! - The back sheet is raw, uncompressed art found by its tiles, and
//!   `gTrainerBackPicTable` (`{data, size, tag}`) indexes it by
//!   `TRAINER_BACK_PIC_*`. Nothing points from the front tables to the back
//!   ones, so the table is found through the pointers to the sheets, whose
//!   rows must carry the size of four frames and the back index as the tag.
//!   `gTrainerBackPicPaletteTable` (`{data, tag}`) is found through the front
//!   palette's address: upstream names the same `gTrainerPalette_*` symbol in
//!   both palette tables, so the two rows must point at the one stream, not at
//!   two streams that happen to hold the same colours.

use super::error::GenRomProfileError;
use super::images::{locate_images, ImageQuery};
use super::inventory;
use super::locate::camel_case;
use super::plan::{
    ImagePlan, PalettePlan, ReportLine, SymbolExpectation, TrainerBackPlan, TrainerBattlerPlan,
    TrainerFrontPicPlan, TrainerFrontPlan,
};
use super::trainer_front::{find_table, Row};
use super::Context;
use crate::extract::scope::TRAINER_BATTLERS;

/// How many rows each back table holds: `TRAINER_BACK_PIC_STEVEN + 1`.
const BACK_TABLE_ROWS: u32 = 8;
/// A back picture row's `size`: four frames of `TRAINER_PIC_SIZE` (0x800).
const BACK_PICTURE_SIZE: u16 = 0x2000;

/// Locate both trainers' battler images, palettes and the two back tables.
///
/// `portraits` is the plan [`super::trainer_front::locate`] derived for the
/// same ROM; its report lines must already be in `report`.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] for an absent pack entry;
/// [`GenRomProfileError::StructMismatch`] if a battler front or palette
/// differs from its portrait, or a back table cannot be pinned to exactly one
/// base; [`GenRomProfileError::NotFound`] for a back sheet the ROM does not
/// hold.
pub fn locate(
    ctx: &Context<'_>,
    portraits: &TrainerFrontPlan,
    report: &mut Vec<ReportLine>,
) -> Result<TrainerBattlerPlan, GenRomProfileError> {
    inventory::require_trainer_battlers(ctx.pack)?;

    let mut fronts = Vec::with_capacity(TRAINER_BATTLERS.len());
    for trainer in &TRAINER_BATTLERS {
        let portrait = portraits
            .trainers
            .iter()
            .find(|portrait| portrait.trainer_pic == u16::from(trainer.pic.trainer_pic.index()))
            .ok_or_else(|| GenRomProfileError::NotFound {
                id: trainer.front_id.to_owned(),
            })?;
        fronts.push(front_alias(
            ctx,
            trainer.front_id,
            trainer.palette_id,
            portrait,
        )?);
        repeat_report_line(report, &portrait.front.id, trainer.front_id)?;
        repeat_report_line(report, &portrait.palette.id, trainer.palette_id)?;
    }

    let queries: Vec<ImageQuery> = TRAINER_BATTLERS
        .iter()
        .map(|trainer| ImageQuery::raw(trainer.back_id))
        .collect();
    let first_line = report.len();
    let sheets = locate_images(ctx, &queries, report)?;
    name_back_symbols(&mut report[first_line..]);

    let back_pics: Vec<u16> = TRAINER_BATTLERS
        .iter()
        .map(|trainer| {
            trainer
                .pic
                .trainer_pic
                .battler_back_index()
                .expect("the manifest names only trainers with battler art")
        })
        .collect();
    let picture_rows: Vec<(u16, Vec<u32>)> = back_pics
        .iter()
        .zip(&sheets)
        .map(|(&back_pic, sheet)| (back_pic, vec![sheet.addr]))
        .collect();
    let palette_rows: Vec<(u16, Vec<u32>)> = back_pics
        .iter()
        .zip(&fronts)
        .map(|(&back_pic, front)| (back_pic, vec![front.palette.addr]))
        .collect();
    let back_table = find_table(
        ctx,
        "gTrainerBackPicTable",
        &picture_rows,
        Row::Picture {
            size: BACK_PICTURE_SIZE,
        },
        BACK_TABLE_ROWS,
        report,
    )?;
    let back_palette_table = find_table(
        ctx,
        "gTrainerBackPicPaletteTable",
        &palette_rows,
        Row::Palette,
        BACK_TABLE_ROWS,
        report,
    )?;

    let backs = back_pics
        .iter()
        .zip(&fronts)
        .zip(sheets)
        .map(|((&back_pic, front), back)| TrainerBackPlan {
            trainer_pic: front.trainer_pic,
            back_pic,
            back,
        })
        .collect();
    Ok(TrainerBattlerPlan {
        front: TrainerFrontPlan {
            front_table: portraits.front_table,
            palette_table: portraits.palette_table,
            trainers: fronts,
        },
        back_table,
        back_palette_table,
        backs,
    })
}

/// A portrait's plans under the battler ids, once the pack's battler entries
/// are shown to be the portrait's own bytes.
fn front_alias(
    ctx: &Context<'_>,
    front_id: &str,
    palette_id: &str,
    portrait: &TrainerFrontPicPlan,
) -> Result<TrainerFrontPicPlan, GenRomProfileError> {
    require_same_asset(ctx, front_id, &portrait.front.id)?;
    require_same_asset(ctx, palette_id, &portrait.palette.id)?;
    Ok(TrainerFrontPicPlan {
        trainer_pic: portrait.trainer_pic,
        front: ImagePlan {
            id: front_id.to_owned(),
            ..portrait.front.clone()
        },
        palette: PalettePlan {
            id: palette_id.to_owned(),
            ..portrait.palette.clone()
        },
    })
}

/// Refuse unless the pack entries `id` and `portrait_id` share kind and payload.
fn require_same_asset(
    ctx: &Context<'_>,
    id: &str,
    portrait_id: &str,
) -> Result<(), GenRomProfileError> {
    let (battler, portrait) = (ctx.pack.get(id)?, ctx.pack.get(portrait_id)?);
    if battler.kind != portrait.kind || battler.payload != portrait.payload {
        return Err(GenRomProfileError::StructMismatch {
            id: id.to_owned(),
            reason: format!("differs from `{portrait_id}`, whose ROM art it must repeat"),
        });
    }
    Ok(())
}

/// Report `id` where `portrait_id` was reported: the same address, symbol and
/// resolution, under the battler id.
fn repeat_report_line(
    report: &mut Vec<ReportLine>,
    portrait_id: &str,
    id: &str,
) -> Result<(), GenRomProfileError> {
    let mut line = report
        .iter()
        .find(|line| line.id == portrait_id)
        .cloned()
        .ok_or_else(|| GenRomProfileError::NotFound {
            id: portrait_id.to_owned(),
        })?;
    id.clone_into(&mut line.id);
    report.push(line);
    Ok(())
}

/// Require a linker map to name each back sheet `gTrainerBackPic_<Name>`.
///
/// The shared image classifier knows no trainer naming convention, so the
/// expectation is set here on the lines [`locate_images`] just reported.
fn name_back_symbols(lines: &mut [ReportLine]) {
    for trainer in &TRAINER_BATTLERS {
        if let Some(line) = lines.iter_mut().find(|line| line.id == trainer.back_id) {
            line.symbol = SymbolExpectation::Exact(format!(
                "gTrainerBackPic_{}",
                camel_case(trainer.pic.name)
            ));
        }
    }
}

#[cfg(test)]
mod tests;
