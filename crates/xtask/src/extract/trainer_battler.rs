//! The Brendan and May trainer battlers: the checkout side of the front and
//! back picture tables and their palette tables
//! (`src/data/trainer_graphics/front_pic_tables.h`, `back_pic_tables.h`).
//!
//! Each trainer publishes three entries: the front picture, the whole
//! four-frame back sheet, and the one palette both directions draw with
//! (`gTrainerPalette_Brendan` is the row of both palette tables). The front
//! picture and palette are also the trainer's portrait
//! (`AssetPack::trainer_pic`), so they are published once.

use std::path::{Path, PathBuf};

use pack_format::PackWriter;

use super::{
    build_image_entry, build_palette_entry, jasc_pal, png, read_file, read_text, ExtractError,
};

/// Width of every trainer picture, and the height of a front picture.
const SIDE: u32 = 64;
const COLORS: usize = 16;
/// The PNG bit depth the typed accessors require of every trainer picture.
const BIT_DEPTH: u8 = 4;
/// The height of a back sheet: four 64x64 frames.
const BACK_HEIGHT: u32 = SIDE * 4;

/// One trainer's battler files, relative to `graphics/trainers/`, and the
/// pack ids they publish under.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TrainerSource {
    /// The lower-case trainer name; the picture and palette symbols spell it in camel case.
    pub(crate) name: &'static str,
    /// The row of `gTrainerFrontPicTable` and `gTrainerFrontPicPaletteTable` this trainer fills.
    pub(crate) trainer_pic: assets::TrainerPicId,
    pub(crate) front_png: &'static str,
    pub(crate) back_png: &'static str,
    pub(crate) palette: &'static str,
    pub(crate) front_id: &'static str,
    pub(crate) back_id: &'static str,
    pub(crate) palette_id: &'static str,
}

pub(crate) const TRAINERS: [TrainerSource; 2] = [
    TrainerSource {
        name: "brendan",
        trainer_pic: assets::TrainerPicId::BRENDAN,
        front_png: "front_pics/brendan.png",
        back_png: "back_pics/brendan.png",
        palette: "palettes/brendan.pal",
        front_id: "trainer/brendan/front",
        back_id: "trainer/brendan/back",
        palette_id: "trainer/brendan/palette",
    },
    TrainerSource {
        name: "may",
        trainer_pic: assets::TrainerPicId::MAY,
        front_png: "front_pics/may.png",
        back_png: "back_pics/may.png",
        palette: "palettes/may.pal",
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
        let (front_path, front) = read_picture(&directory, trainer.front_png, SIDE)?;
        let (back_path, back) = read_picture(&directory, trainer.back_png, BACK_HEIGHT)?;
        let (palette_path, colors) = read_palette(&directory, trainer.palette)?;
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

/// Where the trainer graphics sit in the checkout.
const TRAINER_GRAPHICS: &str = "graphics/trainers";

/// Reads one 16-colour trainer palette file.
fn read_palette(
    directory: &Path,
    file: &str,
) -> Result<(PathBuf, Vec<jasc_pal::Rgb888>), ExtractError> {
    let path = directory.join(file);
    let colors =
        jasc_pal::parse(&read_text(&path)?).map_err(|e| ExtractError::Pal(path.clone(), e))?;
    if colors.len() != COLORS {
        return Err(mismatch(
            &path,
            format!("has {} colours: expected {COLORS}", colors.len()),
        ));
    }
    Ok((path, colors))
}

/// Reads one 4bpp, 64-pixel-wide trainer picture of `height` pixels whose
/// indices all fit a 16-colour palette.
fn read_picture(
    directory: &Path,
    file: &str,
    height: u32,
) -> Result<(PathBuf, png::IndexedImage), ExtractError> {
    let path = directory.join(file);
    let image = png::decode(&read_file(&path)?).map_err(|e| ExtractError::Png(path.clone(), e))?;
    if image.width != SIDE || image.height != height {
        return Err(mismatch(
            &path,
            format!(
                "is {}x{}: expected {SIDE}x{height}",
                image.width, image.height
            ),
        ));
    }
    if image.bit_depth != BIT_DEPTH {
        return Err(mismatch(
            &path,
            format!("is {}-bit: expected {BIT_DEPTH}-bit", image.bit_depth),
        ));
    }
    if let Some(&pixel) = image
        .pixels
        .iter()
        .find(|&&pixel| usize::from(pixel) >= COLORS)
    {
        return Err(mismatch(
            &path,
            format!("has pixel index {pixel}: its palette only has {COLORS} colours"),
        ));
    }
    Ok((path, image))
}

fn mismatch(path: &Path, detail: String) -> ExtractError {
    ExtractError::TrainerPicAssetMismatch {
        path: path.to_path_buf(),
        detail,
    }
}

#[cfg(test)]
mod tests;
