//! Typed starter-chooser graphics (upstream `src/starter_choose.c`).
//!
//! Pack ids stay private so scene code composes no strings. Every lookup
//! re-validates shapes, since a hand-built or corrupt pack can bypass extraction.

use super::battler::{self, BattlerDirection, PaletteVariant};
use super::{AssetPack, ImageRef, PackError, PaletteRef};
use crate::SpeciesId;

const TILES_IMAGE_ID: &str = "starter-chooser/image/tiles";
const TILES_PALETTE_ID: &str = "starter-chooser/palette/tiles";
const POKEBALL_IMAGE_ID: &str = "starter-chooser/image/pokeball_selection";
const POKEBALL_PALETTE_ID: &str = "starter-chooser/palette/pokeball_selection";
const CIRCLE_IMAGE_ID: &str = "starter-chooser/image/starter_circle";
const CIRCLE_PALETTE_ID: &str = "starter-chooser/palette/starter_circle";
const BAG_TILEMAP_ID: &str = "starter-chooser/raw/birch_bag";
const GRASS_TILEMAP_ID: &str = "starter-chooser/raw/birch_grass";

const TILES_SIDE: u32 = 128;
const TILES_PALETTE_COLORS: u16 = 32;
const POKEBALL_WIDTH: u32 = 32;
const POKEBALL_HEIGHT: u32 = 128;
const CIRCLE_SIDE: u32 = 64;
const BANK_PALETTE_COLORS: u16 = 16;
const TILES_BIT_DEPTH: u8 = 8;
const BANK_BIT_DEPTH: u8 = 4;
const TILEMAP_ENTRY_BYTES: usize = 2;
const PALETTE_ENTRY_BYTES: usize = 2;
const BAG_TILEMAP_BYTES: usize = 32 * 20 * TILEMAP_ENTRY_BYTES;
const GRASS_TILEMAP_BYTES: usize = 32 * 32 * TILEMAP_ENTRY_BYTES;

/// The three starters offered by the chooser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StarterSpecies {
    /// Treecko, the left ball.
    Treecko,
    /// Torchic, the middle ball.
    Torchic,
    /// Mudkip, the right ball.
    Mudkip,
}

impl StarterSpecies {
    /// The species identity the battler lookup keys this starter by.
    #[must_use]
    pub const fn species_id(self) -> SpeciesId {
        match self {
            Self::Treecko => SpeciesId::TREECKO,
            Self::Torchic => SpeciesId::TORCHIC,
            Self::Mudkip => SpeciesId::MUDKIP,
        }
    }
}

/// An indexed image paired with the palette its pixel indices select from.
#[derive(Debug, Clone, Copy)]
pub struct PalettedImage<'a> {
    /// One palette index per pixel.
    pub image: ImageRef<'a>,
    /// Palette the pixel indices select from.
    pub palette: PaletteRef<'a>,
}

/// Every graphic the starter chooser draws, borrowing the pack for `'a`.
///
/// [`AssetPack::starter_chooser`] returns it with every entry validated.
/// The hand and the Poké Balls share one sheet.
#[derive(Debug, Clone, Copy)]
pub struct StarterChooserHandle<'a> {
    /// Bag and grass background tile sheet with its 32-colour palette.
    pub tiles: PalettedImage<'a>,
    /// Poké Ball and hand sheet with its 16-colour palette.
    pub pokeball_selection: PalettedImage<'a>,
    /// Starter preview circle sheet with its 16-colour palette.
    pub starter_circle: PalettedImage<'a>,
    /// Birch's bag background tilemap (`birch_bag.bin`).
    pub bag_tilemap: &'a [u8],
    /// Birch's grass background tilemap (`birch_grass.bin`).
    pub grass_tilemap: &'a [u8],
}

/// One starter's shared normal front image and 16-colour palette.
///
/// The generic species art the chooser preview shares with battle rendering.
#[derive(Debug, Clone, Copy)]
pub struct StarterPreviewHandle<'a> {
    /// Normal front image and palette.
    pub front: PalettedImage<'a>,
}

impl AssetPack {
    /// Bundle every chooser graphic and tilemap.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] or [`PackError::WrongKind`] for a missing or
    /// wrong-kind entry; [`PackError::MalformedStarterAsset`] when dimensions,
    /// source bit depth, palette size, pixel indices, or tilemap length differ from upstream.
    pub fn starter_chooser(&self) -> Result<StarterChooserHandle<'_>, PackError> {
        Ok(StarterChooserHandle {
            tiles: self.starter_image(
                TILES_IMAGE_ID,
                TILES_PALETTE_ID,
                (TILES_SIDE, TILES_SIDE),
                TILES_BIT_DEPTH,
                TILES_PALETTE_COLORS,
            )?,
            pokeball_selection: self.starter_image(
                POKEBALL_IMAGE_ID,
                POKEBALL_PALETTE_ID,
                (POKEBALL_WIDTH, POKEBALL_HEIGHT),
                BANK_BIT_DEPTH,
                BANK_PALETTE_COLORS,
            )?,
            starter_circle: self.starter_image(
                CIRCLE_IMAGE_ID,
                CIRCLE_PALETTE_ID,
                (CIRCLE_SIDE, CIRCLE_SIDE),
                BANK_BIT_DEPTH,
                BANK_PALETTE_COLORS,
            )?,
            bag_tilemap: self.starter_tilemap(BAG_TILEMAP_ID, BAG_TILEMAP_BYTES)?,
            grass_tilemap: self.starter_tilemap(GRASS_TILEMAP_ID, GRASS_TILEMAP_BYTES)?,
        })
    }

    /// Bundle one starter's shared normal front image and palette.
    ///
    /// # Errors
    ///
    /// Same as [`starter_chooser`](Self::starter_chooser).
    pub fn starter_preview(
        &self,
        starter: StarterSpecies,
    ) -> Result<StarterPreviewHandle<'_>, PackError> {
        let species = starter.species_id();
        let (image_id, palette_id) =
            battler::ids(species, BattlerDirection::Front, PaletteVariant::Normal)
                .ok_or(PackError::UnsupportedBattlerSpecies(species))?;
        Ok(StarterPreviewHandle {
            front: self.starter_image(
                image_id,
                palette_id,
                (battler::BATTLER_SIDE, battler::BATTLER_SIDE),
                BANK_BIT_DEPTH,
                BANK_PALETTE_COLORS,
            )?,
        })
    }

    fn starter_image(
        &self,
        image_id: &str,
        palette_id: &str,
        shape: (u32, u32),
        bit_depth: u8,
        palette_colors: u16,
    ) -> Result<PalettedImage<'_>, PackError> {
        self.paired_image(
            image_id,
            palette_id,
            shape,
            bit_depth,
            palette_colors,
            |id, detail| PackError::MalformedStarterAsset { id, detail },
        )
    }

    /// An image and the palette its indices select from, checked against the
    /// shape upstream gives them; `malformed` names the failure for the caller's family.
    pub(super) fn paired_image(
        &self,
        image_id: &str,
        palette_id: &str,
        (width, height): (u32, u32),
        bit_depth: u8,
        palette_colors: u16,
        malformed: fn(String, String) -> PackError,
    ) -> Result<PalettedImage<'_>, PackError> {
        let image = self.image(image_id)?;
        let fault = |detail: String| malformed(image_id.to_owned(), detail);
        if image.width != width || image.height != height {
            return Err(fault(format!(
                "is {}x{}: expected {width}x{height}",
                image.width, image.height
            )));
        }
        if image.bit_depth != bit_depth {
            return Err(fault(format!(
                "is {}-bit: expected {bit_depth}-bit",
                image.bit_depth
            )));
        }
        if u64::try_from(image.pixels.len()).ok() != Some(u64::from(width) * u64::from(height)) {
            return Err(fault(format!(
                "carries {} payload bytes: expected one per pixel",
                image.pixels.len()
            )));
        }
        let palette = self.palette(palette_id)?;
        if palette.color_count != palette_colors
            || palette.raw.len() != usize::from(palette_colors) * PALETTE_ENTRY_BYTES
        {
            return Err(malformed(
                palette_id.to_owned(),
                format!(
                    "declares {} colours in {} bytes: expected {palette_colors} colours",
                    palette.color_count,
                    palette.raw.len()
                ),
            ));
        }
        if let Some(&pixel) = image
            .pixels
            .iter()
            .find(|&&pixel| u16::from(pixel) >= palette.color_count)
        {
            return Err(fault(format!(
                "has pixel index {pixel}: its palette only has {} colours",
                palette.color_count
            )));
        }
        Ok(PalettedImage { image, palette })
    }

    fn starter_tilemap(&self, id: &str, expected_len: usize) -> Result<&[u8], PackError> {
        let bytes = self.raw(id)?;
        if bytes.len() != expected_len {
            return Err(PackError::MalformedStarterAsset {
                id: id.to_owned(),
                detail: format!("is {} bytes: expected {expected_len}", bytes.len()),
            });
        }
        Ok(bytes)
    }
}
