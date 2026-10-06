//! Typed starter-chooser graphics (upstream `src/starter_choose.c`).
//!
//! The pack ids stay private to this module so scene code composes no strings
//! and cannot substitute other art. Each lookup re-checks the shapes
//! extraction enforces, because a corrupt or hand-built pack must not reach a
//! renderer with an unmappable pixel or a short tilemap.

use super::{AssetPack, ImageRef, PackError, PaletteRef};

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
const FRONT_SIDE: u32 = 64;
const BANK_PALETTE_COLORS: u16 = 16;
/// A 32x20 screenblock of 16-bit text-mode entries.
const BAG_TILEMAP_BYTES: usize = 1280;
/// A 32x32 screenblock of 16-bit text-mode entries.
const GRASS_TILEMAP_BYTES: usize = 2048;

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
    const fn front_ids(self) -> (&'static str, &'static str) {
        match self {
            Self::Treecko => ("pokemon/treecko/front", "pokemon/treecko/palette/normal"),
            Self::Torchic => ("pokemon/torchic/front", "pokemon/torchic/palette/normal"),
            Self::Mudkip => ("pokemon/mudkip/front", "pokemon/mudkip/palette/normal"),
        }
    }
}

/// An indexed sheet with the palette its indices map through.
#[derive(Debug, Clone, Copy)]
pub struct PalettedImage<'a> {
    /// Palette-index bitmap.
    pub image: ImageRef<'a>,
    /// Palette used by the bitmap.
    pub palette: PaletteRef<'a>,
}

/// Every graphic the starter chooser draws, borrowing the pack for `'a`.
///
/// [`AssetPack::starter_chooser`] requires each entry and validates its shape
/// before returning this handle. The hand and the Poké Balls share one sheet.
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
/// [`AssetPack::starter_preview`] returns the generic species art that the
/// chooser preview and battle rendering share.
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
    /// [`PackError::UnknownAsset`] or [`PackError::WrongKind`] when an entry is
    /// missing or of the wrong kind; [`PackError::MalformedStarterAsset`] when
    /// an entry's dimensions, palette size, pixel indices, or tilemap length
    /// differ from the upstream graphic.
    pub fn starter_chooser(&self) -> Result<StarterChooserHandle<'_>, PackError> {
        Ok(StarterChooserHandle {
            tiles: self.starter_image(
                TILES_IMAGE_ID,
                TILES_PALETTE_ID,
                (TILES_SIDE, TILES_SIDE),
                TILES_PALETTE_COLORS,
            )?,
            pokeball_selection: self.starter_image(
                POKEBALL_IMAGE_ID,
                POKEBALL_PALETTE_ID,
                (POKEBALL_WIDTH, POKEBALL_HEIGHT),
                BANK_PALETTE_COLORS,
            )?,
            starter_circle: self.starter_image(
                CIRCLE_IMAGE_ID,
                CIRCLE_PALETTE_ID,
                (CIRCLE_SIDE, CIRCLE_SIDE),
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
        let (image_id, palette_id) = starter.front_ids();
        Ok(StarterPreviewHandle {
            front: self.starter_image(
                image_id,
                palette_id,
                (FRONT_SIDE, FRONT_SIDE),
                BANK_PALETTE_COLORS,
            )?,
        })
    }

    fn starter_image(
        &self,
        image_id: &str,
        palette_id: &str,
        (width, height): (u32, u32),
        palette_colors: u16,
    ) -> Result<PalettedImage<'_>, PackError> {
        let image = self.image(image_id)?;
        let fault = |detail: String| PackError::MalformedStarterAsset {
            id: image_id.to_owned(),
            detail,
        };
        if image.width != width || image.height != height {
            return Err(fault(format!(
                "is {}x{}: expected {width}x{height}",
                image.width, image.height
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
            || palette.raw.len() != usize::from(palette_colors) * 2
        {
            return Err(PackError::MalformedStarterAsset {
                id: palette_id.to_owned(),
                detail: format!(
                    "declares {} colours in {} bytes: expected {palette_colors} colours",
                    palette.color_count,
                    palette.raw.len()
                ),
            });
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
