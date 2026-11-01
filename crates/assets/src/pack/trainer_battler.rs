//! Typed trainer battlers (upstream `gTrainerFrontPicTable`,
//! `gTrainerFrontPicPaletteTable`, `gTrainerBackPicTable` and
//! `gTrainerBackPicPaletteTable`).
//!
//! A trainer's front and back art live in two tables with two index spaces:
//! the front tables are indexed by the `TRAINER_PIC_*` identity
//! ([`TrainerPicId`]), the back tables by `TRAINER_BACK_PIC_*`, and upstream
//! selects them through separate operations (`DecompressTrainerFrontPic` and
//! `DecompressTrainerBackPic`, `src/battle_gfx_sfx_util.c`). This module owns
//! the correspondence between the two spaces, so a front identity is never
//! used as a back index.
//!
//! Each trainer's front and back share one palette. The front image is a
//! single 64x64 picture; the back image is the whole four-frame sheet, 64x256,
//! because the pack keeps the source graphic and which frame shows is the
//! animation tables' business, not this lookup's.
//!
//! The pack ids stay private so battle code composes no strings, and every
//! lookup re-validates shapes, since a hand-built or corrupt pack can bypass
//! extraction.

use super::battler::BattlerDirection;
use super::PalettedImage;
use super::{AssetPack, PackError};
use crate::TrainerPicId;

/// Width of both trainer images (`TRAINER_PIC_WIDTH`).
const TRAINER_BATTLER_WIDTH: u32 = 64;
const FRONT_HEIGHT: u32 = 64;
/// A back image stacks four 64x64 frames.
const BACK_HEIGHT: u32 = 256;
const BIT_DEPTH: u8 = 4;
const PALETTE_COLORS: u16 = 16;

/// One trainer's battler art, borrowing the pack for `'a`.
#[derive(Debug, Clone, Copy)]
pub struct TrainerBattlerHandle<'a> {
    /// The trainer, named by its front picture identity.
    pub id: TrainerPicId,
    /// Which side the image shows.
    pub direction: BattlerDirection,
    /// The image with the 16-colour palette both directions share: 64x64 for
    /// [`BattlerDirection::Front`], the 64x256 four-frame sheet for
    /// [`BattlerDirection::Back`].
    pub art: PalettedImage<'a>,
}

/// What the pack carries for one trainer.
struct TrainerSource {
    /// The front tables' index, `TRAINER_PIC_*`.
    front_pic: TrainerPicId,
    /// The back tables' index, `TRAINER_BACK_PIC_*`.
    back_pic: u16,
    front_id: &'static str,
    back_id: &'static str,
    palette_id: &'static str,
}

/// The Emerald player characters, the only trainers the pack carries
/// battlers for.
const TRAINERS: [TrainerSource; 2] = [
    TrainerSource {
        front_pic: TrainerPicId::BRENDAN,
        back_pic: 0,
        front_id: "trainer/brendan/front",
        back_id: "trainer/brendan/back",
        palette_id: "trainer/brendan/palette",
    },
    TrainerSource {
        front_pic: TrainerPicId::MAY,
        back_pic: 1,
        front_id: "trainer/may/front",
        back_id: "trainer/may/back",
        palette_id: "trainer/may/palette",
    },
];

fn source(id: TrainerPicId) -> Option<&'static TrainerSource> {
    TRAINERS.iter().find(|trainer| trainer.front_pic == id)
}

impl TrainerPicId {
    /// This trainer's index in the back picture tables (`TRAINER_BACK_PIC_*`),
    /// a different index space from the front picture identity; `None` for a
    /// trainer without battler art in the pack.
    #[must_use]
    pub fn battler_back_index(self) -> Option<u16> {
        source(self).map(|trainer| trainer.back_pic)
    }
}

impl AssetPack {
    /// One trainer's battler image in one direction paired with the trainer's
    /// palette, validated.
    ///
    /// # Errors
    ///
    /// [`PackError::UnsupportedTrainerBattler`] for a trainer the pack has no
    /// battler art for; [`PackError::UnknownAsset`] or [`PackError::WrongKind`]
    /// for a missing or wrong-kind entry; [`PackError::MalformedTrainerBattlerAsset`]
    /// when dimensions, bit depth, palette size, or pixel indices differ from
    /// upstream.
    pub fn trainer_battler(
        &self,
        id: TrainerPicId,
        direction: BattlerDirection,
    ) -> Result<TrainerBattlerHandle<'_>, PackError> {
        let trainer = source(id).ok_or(PackError::UnsupportedTrainerBattler(id))?;
        let (image_id, height) = match direction {
            BattlerDirection::Front => (trainer.front_id, FRONT_HEIGHT),
            BattlerDirection::Back => (trainer.back_id, BACK_HEIGHT),
        };
        let art = self.paired_image(
            image_id,
            trainer.palette_id,
            (TRAINER_BATTLER_WIDTH, height),
            BIT_DEPTH,
            PALETTE_COLORS,
            |id, detail| PackError::MalformedTrainerBattlerAsset { id, detail },
        )?;
        Ok(TrainerBattlerHandle { id, direction, art })
    }
}
