//! Locating the Brendan and May trainer battlers (`src/data/trainer_graphics`).
//!
//! The pack's `trainer/{brendan,may}/{front,back,palette}` entries are the
//! expectation. Two halves, with two index spaces:
//!
//! - The front picture and palette are located by [`super::trainer_front`],
//!   indexed by `TRAINER_PIC_*`. The same entries serve as the trainer's
//!   portrait, so the ROM art is searched for once.
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
use super::plan::{ReportLine, SymbolExpectation, TrainerBackPlan, TrainerBattlerPlan};
use super::trainer_front::{self, find_table, Row};
use super::Context;
use crate::extract::scope::TRAINER_BATTLERS;

/// How many rows each back table holds: `TRAINER_BACK_PIC_STEVEN + 1`.
const BACK_TABLE_ROWS: u32 = 8;
/// A back picture row's `size`: four frames of `TRAINER_PIC_SIZE` (0x800).
const BACK_PICTURE_SIZE: u16 = 0x2000;

/// Locate both trainers' battler images, palettes and the two back tables.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] for an absent pack entry;
/// [`GenRomProfileError::StructMismatch`] for a pack `trainer/` entry outside
/// the inventory or a table that cannot be pinned to exactly one base;
/// [`GenRomProfileError::NotFound`] for art the ROM does not hold.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<TrainerBattlerPlan, GenRomProfileError> {
    inventory::require_trainer_battlers(ctx.pack)?;
    let front = trainer_front::locate(ctx, report)?;

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
        .zip(&front.trainers)
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
        .zip(&front.trainers)
        .zip(sheets)
        .map(|((&back_pic, front), back)| TrainerBackPlan {
            trainer_pic: front.trainer_pic,
            back_pic,
            back,
        })
        .collect();
    Ok(TrainerBattlerPlan {
        front,
        back_table,
        back_palette_table,
        backs,
    })
}

/// Require a linker map to name each back sheet `gTrainerBackPic_<Name>`.
///
/// The shared image classifier knows no trainer naming convention, so the
/// expectation is set here on the lines [`locate_images`] just reported.
fn name_back_symbols(lines: &mut [ReportLine]) {
    for trainer in &TRAINER_BATTLERS {
        if let Some(line) = lines.iter_mut().find(|line| line.id == trainer.back_id) {
            line.symbol =
                SymbolExpectation::Exact(format!("gTrainerBackPic_{}", camel_case(trainer.name)));
        }
    }
}

#[cfg(test)]
mod tests;
