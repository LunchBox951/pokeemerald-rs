//! Indexed images paired with their palettes, validated in one place.
//!
//! [`PalettedImage`] is the pack-wide pairing of a pixel sheet with the palette
//! its indices select from. [`AssetPack::paired_image`] is the shared check for
//! that pairing: the starter chooser, battler, trainer picture, opening scene and
//! Pokédex families call it, each passing the upstream shape and the
//! [`PackError`] constructor that names its own failures. This module owns no
//! pack ids and knows no asset family.

use super::{AssetPack, ImageRef, PackError, PaletteRef};

/// Bytes one palette entry occupies in a pack (a 15-bit GBA colour in 2 bytes).
const PALETTE_ENTRY_BYTES: usize = 2;

/// An indexed image paired with the palette its pixel indices select from.
#[derive(Debug, Clone, Copy)]
pub struct PalettedImage<'a> {
    /// One palette index per pixel.
    pub image: ImageRef<'a>,
    /// Palette the pixel indices select from.
    pub palette: PaletteRef<'a>,
}

impl AssetPack {
    /// An image and the palette its indices select from, checked against the
    /// shape upstream gives them; `malformed` names the failure for the caller's family.
    ///
    /// # Errors
    ///
    /// The lookup errors of [`AssetPack::image`] and [`AssetPack::palette`] for a
    /// missing or wrong-kind entry. `malformed` receives the offending id and a
    /// detail when the image is not `width` x `height` at `bit_depth` with one
    /// payload byte per pixel, when the palette does not declare `palette_colors`
    /// colours in matching bytes, or when a pixel index falls outside the palette
    /// or the `bit_depth` range.
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
        if let Some(&pixel) = image.pixels.iter().find(|&&pixel| {
            u16::from(pixel) >= palette.color_count
                || (bit_depth < 8 && u16::from(pixel) >> bit_depth != 0)
        }) {
            return Err(fault(format!(
                "has pixel index {pixel}: outside its palette's {} colours or {bit_depth}-bit range",
                palette.color_count
            )));
        }
        Ok(PalettedImage { image, palette })
    }
}
