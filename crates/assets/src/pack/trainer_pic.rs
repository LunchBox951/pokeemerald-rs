//! Typed trainer front pictures (upstream `gTrainerFrontPicTable` and
//! `gTrainerFrontPicPaletteTable`, `src/data/trainer_graphics/front_pic_tables.h`).
//!
//! The pack ids stay private so scene code composes no strings. Only the
//! identities the pack carries resolve; every lookup re-validates shapes,
//! since a hand-built or corrupt pack can bypass extraction.

use super::{AssetPack, PackError, PalettedImage};
use crate::TrainerPicId;

/// Width and height of a trainer front picture (`TRAINER_PIC_WIDTH`, size 8 tiles).
const TRAINER_PIC_SIDE: u32 = 64;
const TRAINER_PIC_BIT_DEPTH: u8 = 4;
const TRAINER_PIC_PALETTE_COLORS: u16 = 16;

/// One trainer front picture and its palette, borrowing the pack for `'a`.
#[derive(Debug, Clone, Copy)]
pub struct TrainerPicHandle<'a> {
    /// The picture identity.
    pub id: TrainerPicId,
    /// The 64x64 4bpp image with its 16-colour palette.
    pub art: PalettedImage<'a>,
}

/// Image and palette ids for one picture, or `None` for an identity without art.
fn ids(id: TrainerPicId) -> Option<(&'static str, &'static str)> {
    match id {
        TrainerPicId::BRENDAN => Some(("trainer-pic/brendan/front", "trainer-pic/brendan/palette")),
        TrainerPicId::MAY => Some(("trainer-pic/may/front", "trainer-pic/may/palette")),
        _ => None,
    }
}

impl AssetPack {
    /// One trainer front picture paired with its palette, validated.
    ///
    /// # Errors
    ///
    /// [`PackError::UnsupportedTrainerPic`] for an identity the pack has no art
    /// for; [`PackError::UnknownAsset`] or [`PackError::WrongKind`] for a
    /// missing or wrong-kind entry; [`PackError::MalformedTrainerPicAsset`]
    /// when dimensions, bit depth, palette size, or pixel indices differ from
    /// upstream.
    pub fn trainer_pic(&self, id: TrainerPicId) -> Result<TrainerPicHandle<'_>, PackError> {
        let (image_id, palette_id) = ids(id).ok_or(PackError::UnsupportedTrainerPic(id))?;
        let art = self.paired_image(
            image_id,
            palette_id,
            (TRAINER_PIC_SIDE, TRAINER_PIC_SIDE),
            TRAINER_PIC_BIT_DEPTH,
            TRAINER_PIC_PALETTE_COLORS,
            |id, detail| PackError::MalformedTrainerPicAsset { id, detail },
        )?;
        Ok(TrainerPicHandle { id, art })
    }
}
