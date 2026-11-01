//! The Brendan and May trainer battlers: the checkout side of the front and
//! back picture tables and their palette tables
//! (`src/data/trainer_graphics/front_pic_tables.h`, `back_pic_tables.h`).
//!
//! Each trainer publishes three entries: the front picture, the whole
//! four-frame back sheet, and the one palette both directions draw with
//! (`gTrainerPalette_Brendan` is the row of both palette tables). The front
//! picture and palette are the same files the Birch-speech portraits use, so
//! they are read by the same functions and published again under the battler
//! ids.

use std::path::Path;

use pack_format::PackWriter;

use super::trainer_pic::{read_palette, read_picture, PicSource, PICS, SIDE, TRAINER_GRAPHICS};
use super::{build_image_entry, build_palette_entry, ExtractError};

/// The height of a back sheet: four 64x64 frames.
const BACK_HEIGHT: u32 = SIDE * 4;

/// One trainer's battler files and the pack ids they publish under.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TrainerSource {
    /// The front picture and palette files, shared with the portraits.
    pub(crate) pic: PicSource,
    /// The back sheet, relative to `graphics/trainers/`.
    back_png: &'static str,
    pub(crate) front_id: &'static str,
    pub(crate) back_id: &'static str,
    pub(crate) palette_id: &'static str,
}

pub(crate) const TRAINERS: [TrainerSource; 2] = [
    TrainerSource {
        pic: PICS[0],
        back_png: "back_pics/brendan.png",
        front_id: "trainer/brendan/front",
        back_id: "trainer/brendan/back",
        palette_id: "trainer/brendan/palette",
    },
    TrainerSource {
        pic: PICS[1],
        back_png: "back_pics/may.png",
        front_id: "trainer/may/front",
        back_id: "trainer/may/back",
        palette_id: "trainer/may/palette",
    },
];

/// Extracts the Brendan and May front pictures, back sheets and palettes.
pub(super) fn extract_trainer_battlers(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let directory = upstream.join(TRAINER_GRAPHICS);
    let mut entries = Vec::with_capacity(TRAINERS.len() * 3);
    for trainer in TRAINERS {
        let (front_path, front) = read_picture(&directory, trainer.pic.png, SIDE)?;
        let (back_path, back) = read_picture(&directory, trainer.back_png, BACK_HEIGHT)?;
        let (palette_path, colors) = read_palette(&directory, trainer.pic.palette)?;
        entries.push(build_image_entry(
            &front_path,
            trainer.front_id.to_owned(),
            front,
        )?);
        entries.push(build_image_entry(
            &back_path,
            trainer.back_id.to_owned(),
            back,
        )?);
        entries.push(build_palette_entry(
            &palette_path,
            &colors,
            trainer.palette_id.to_owned(),
        )?);
    }
    // Queue only once both trainers validated, so a refusal publishes neither.
    for entry in entries {
        writer.push(entry);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
