//! Indexed images paired with their palettes, validated in one place.
//!
//! [`PalettedImage`] is the pack-wide pairing of a pixel sheet with the palette
//! its indices select from. [`AssetPack::paired_image`] is the shared check for
//! that pairing: the starter chooser, battler, trainer picture, opening scene and
//! Pokédex families call it, each passing the upstream shape and the
//! [`PackError`] constructor that names its own failures.
//!
//! A family whose sheet and palette are read apart (a tilemap or sprite picks
//! the palette bank) uses the two halves of that check instead:
//! [`AssetPack::bank_image`] validates a 4bpp sheet against one bank and
//! [`AssetPack::sized_palette`] validates a palette on its own size. This
//! module owns no pack ids and knows no asset family.

use super::{AssetPack, ImageRef, PackError, PaletteRef};

/// Bytes one palette entry occupies in a pack (a 15-bit GBA colour in 2 bytes).
const PALETTE_ENTRY_BYTES: usize = 2;
/// The depth of a 4bpp tile sheet.
const BANK_BIT_DEPTH: u8 = 4;
/// Colours in one 4bpp bank, the range a 4bpp pixel index selects within.
const BANK_COLORS: u16 = 16;

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
        check_image_shape(&image, image_id, (width, height), bit_depth, malformed)?;
        let palette = self.palette(palette_id)?;
        check_palette_size(&palette, palette_id, palette_colors, malformed)?;
        if let Some(&pixel) = image.pixels.iter().find(|&&pixel| {
            u16::from(pixel) >= palette.color_count
                || (bit_depth < 8 && u16::from(pixel) >> bit_depth != 0)
        }) {
            return Err(malformed(
                image_id.to_owned(),
                format!(
                "has pixel index {pixel}: outside its palette's {} colours or {bit_depth}-bit range",
                    palette.color_count
                ),
            ));
        }
        Ok(PalettedImage { image, palette })
    }

    /// A 4bpp image checked against the shape upstream gives it, with no
    /// palette: the shared validation for sheets a tilemap or sprite pairs with
    /// a 16-colour bank (or, for a background drawn over several banks, with
    /// a palette this reader validates separately).
    ///
    /// `malformed` names the failure for the caller's family. No palette is
    /// read, so the pixel indices are checked against the one bank a 4bpp tile
    /// selects within, never against the size of a palette that holds several.
    ///
    /// # Errors
    ///
    /// The lookup errors of [`AssetPack::image`] for a missing or wrong-kind
    /// entry. `malformed` receives the offending id and a detail when the image
    /// is not `width` x `height` at 4 bits with one payload byte per pixel, or
    /// when a pixel index is outside one 16-colour bank.
    pub(super) fn bank_image(
        &self,
        id: &str,
        (width, height): (u32, u32),
        malformed: fn(String, String) -> PackError,
    ) -> Result<ImageRef<'_>, PackError> {
        let image = self.image(id)?;
        check_image_shape(&image, id, (width, height), BANK_BIT_DEPTH, malformed)?;
        if let Some(&pixel) = image
            .pixels
            .iter()
            .find(|&&pixel| u16::from(pixel) >= BANK_COLORS)
        {
            return Err(malformed(
                id.to_owned(),
                format!(
                    "has pixel index {pixel}: a 4bpp tile selects within one {BANK_COLORS}-colour bank"
                ),
            ));
        }
        Ok(image)
    }

    /// A palette checked to hold `colors` entries in matching bytes;
    /// `malformed` names the failure for the caller's family.
    ///
    /// # Errors
    ///
    /// The lookup errors of [`AssetPack::palette`], or `malformed` when the
    /// palette declares another colour count or its bytes disagree with it.
    pub(super) fn sized_palette(
        &self,
        id: &str,
        colors: u16,
        malformed: fn(String, String) -> PackError,
    ) -> Result<PaletteRef<'_>, PackError> {
        let palette = self.palette(id)?;
        check_palette_size(&palette, id, colors, malformed)?;
        Ok(palette)
    }
}

/// Dimensions, bit depth and one payload byte per pixel of `image`, shared by
/// every image validation here.
fn check_image_shape(
    image: &ImageRef<'_>,
    id: &str,
    (width, height): (u32, u32),
    bit_depth: u8,
    malformed: fn(String, String) -> PackError,
) -> Result<(), PackError> {
    let fault = |detail: String| malformed(id.to_owned(), detail);
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
    Ok(())
}

fn check_palette_size(
    palette: &PaletteRef<'_>,
    id: &str,
    colors: u16,
    malformed: fn(String, String) -> PackError,
) -> Result<(), PackError> {
    if palette.color_count != colors
        || palette.raw.len() != usize::from(colors) * PALETTE_ENTRY_BYTES
    {
        return Err(malformed(
            id.to_owned(),
            format!(
                "declares {} colours in {} bytes: expected {colors} colours",
                palette.color_count,
                palette.raw.len()
            ),
        ));
    }
    Ok(())
}
