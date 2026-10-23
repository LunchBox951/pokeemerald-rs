//! Typed Poké Ball graphics (upstream `gBallSpriteSheets[BALL_POKE]`,
//! `gBallSpritePalettes[BALL_POKE]`, and `gOpenPokeballGfx`, `src/pokeball.c`).
//!
//! The player's thrown Poké Ball and both trainer send-out and refusal paths
//! draw this one record. The pack ids stay private so battle code composes no
//! strings, and every lookup re-validates shapes, since a hand-built or
//! corrupt pack can bypass extraction.

use super::starter_chooser::PalettedImage;
use super::{AssetPack, ImageRef, PackError};

const SHEET_ID: &str = "poke-ball/image/poke";
const PALETTE_ID: &str = "poke-ball/palette/poke";
const OPEN_SHEET_ID: &str = "poke-ball/image/open";

/// The closed ball's sheet: twelve 4bpp tiles (`384` bytes).
const SHEET_SIZE: (u32, u32) = (16, 48);
/// The opened ball is one 16x16 frame.
const OPEN_SHEET_SIZE: (u32, u32) = (16, 16);
const BIT_DEPTH: u8 = 4;
const PALETTE_COLORS: u16 = 16;

/// The Poké Ball's sprite art, borrowing the pack for `'a`.
///
/// [`AssetPack::poke_ball`] returns it with every constituent validated.
#[derive(Debug, Clone, Copy)]
pub struct PokeBallHandle<'a> {
    /// The 16x48 closed-ball sheet with its 16-colour palette.
    pub sheet: PalettedImage<'a>,
    /// The 16x16 opened-ball sheet. Upstream links no palette of its own: it
    /// is drawn with [`sheet`](Self::sheet)'s palette, which its pixel
    /// indices were validated against.
    pub open_sheet: ImageRef<'a>,
}

impl AssetPack {
    /// Bundle the Poké Ball's sheet, palette, and opened-ball sheet.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] or [`PackError::WrongKind`] for a missing or
    /// wrong-kind constituent; [`PackError::MalformedPokeBallAsset`] when
    /// dimensions, bit depth, palette size, or pixel indices differ from
    /// upstream.
    pub fn poke_ball(&self) -> Result<PokeBallHandle<'_>, PackError> {
        let sheet = self.poke_ball_image(SHEET_ID, SHEET_SIZE)?;
        let open_sheet = self.poke_ball_image(OPEN_SHEET_ID, OPEN_SHEET_SIZE)?;
        Ok(PokeBallHandle {
            sheet,
            open_sheet: open_sheet.image,
        })
    }

    fn poke_ball_image(
        &self,
        image_id: &str,
        size: (u32, u32),
    ) -> Result<PalettedImage<'_>, PackError> {
        self.paired_image(
            image_id,
            PALETTE_ID,
            size,
            BIT_DEPTH,
            PALETTE_COLORS,
            |id, detail| PackError::MalformedPokeBallAsset { id, detail },
        )
    }
}
