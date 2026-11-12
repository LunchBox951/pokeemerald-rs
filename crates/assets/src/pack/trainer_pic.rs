//! Typed trainer front pictures (upstream `gTrainerFrontPicTable` and
//! `gTrainerFrontPicPaletteTable`, `src/data/trainer_graphics/front_pic_tables.h`).
//!
//! A portrait is the front view of a trainer battler: both lookups read the
//! `trainer/<name>/{front,palette}` entries, so the pack stores the art once
//! and the battler module owns its ids and shape checks.

use super::battler::BattlerDirection;
use super::{AssetPack, PackError, PalettedImage};
use crate::TrainerPicId;

/// One trainer front picture and its palette, borrowing the pack for `'a`.
#[derive(Debug, Clone, Copy)]
pub struct TrainerPicHandle<'a> {
    /// The picture identity.
    pub id: TrainerPicId,
    /// The 64x64 4bpp image with its 16-colour palette.
    pub art: PalettedImage<'a>,
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
        let battler =
            self.trainer_battler(id, BattlerDirection::Front)
                .map_err(|error| match error {
                    PackError::UnsupportedTrainerBattler(id) => {
                        PackError::UnsupportedTrainerPic(id)
                    }
                    PackError::MalformedTrainerBattlerAsset { id, detail } => {
                        PackError::MalformedTrainerPicAsset { id, detail }
                    }
                    other => other,
                })?;
        Ok(TrainerPicHandle {
            id,
            art: battler.art,
        })
    }
}
