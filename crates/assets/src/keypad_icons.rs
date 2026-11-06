//! The keypad-icon sheet: the button and D-pad glyphs a `CHAR_KEYPAD_ICON`
//! token draws.
//!
//! The bitmap lives in the gitignored asset pack as `font/keypad_icons`;
//! fetch it with [`AssetPack::keypad_icons`](crate::pack::AssetPack::keypad_icons)
//! and validate it with [`KeypadIconSheet::new`]. Upstream includes
//! `graphics/fonts/keypad_icons.png` as `sKeypadIconTiles` and copies each
//! icon out of it with `DrawKeypadIcon` (`pokeemerald/src/text.c:117`,
//! `:1609-1623`).
//!
//! # Sheet layout
//!
//! The sheet is [`SHEET_WIDTH`] x [`SHEET_HEIGHT`] pixels, a row-major grid
//! of 8x8 tiles. Icon `id` begins at the tile `sKeypadIcons[id].tileOffset`
//! names (`text.c:95-115`), and `DrawKeypadIcon` reads it with the sheet's
//! full row stride, so an icon is a rectangle of the sheet and not a run of
//! tiles: the last three icons start on the third tile row.
//!
//! # Palette indices
//!
//! Pixels are indices `0..=`[`MAX_PALETTE_INDEX`] into the *window's* palette,
//! not the four glyph roles. Index `0` is transparent
//! (`BlitBitmapRectToWindow` passes colour key `0`, `pokeemerald/src/window.c:398-413`).

use crate::error::AssetError;
use crate::pack::ImageRef;

/// Sheet width in pixels.
pub const SHEET_WIDTH: u32 = 128;
/// Sheet height in pixels.
pub const SHEET_HEIGHT: u32 = 32;
/// Largest palette index a four-bit sheet pixel can name.
pub const MAX_PALETTE_INDEX: u8 = 15;

const TILE_SIZE: u32 = 8;
const SHEET_TILE_COLUMNS: u32 = SHEET_WIDTH / TILE_SIZE;

/// `sKeypadIcons[id].tileOffset` for each icon id (`text.c:95-115`).
const TILE_OFFSETS: [u8; 13] = [
    0x00, 0x01, 0x02, 0x04, 0x06, 0x09, 0x0C, 0x0D, 0x0E, 0x0F, 0x20, 0x21, 0x22,
];

/// A validated, owned copy of the keypad-icon sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeypadIconSheet {
    pixels: Vec<u8>,
}

impl KeypadIconSheet {
    /// Validate and copy the sheet fetched from the asset pack.
    ///
    /// # Errors
    ///
    /// [`AssetError::KeypadSheetWrongShape`] if `image` is not exactly
    /// [`SHEET_WIDTH`] x [`SHEET_HEIGHT`] at four bits per pixel,
    /// [`AssetError::KeypadSheetWrongPixelCount`] if its buffer does not hold
    /// one index per pixel, or [`AssetError::KeypadSheetInvalidPixel`] if an
    /// index exceeds [`MAX_PALETTE_INDEX`].
    pub fn new(image: ImageRef<'_>) -> Result<Self, AssetError> {
        if image.width != SHEET_WIDTH || image.height != SHEET_HEIGHT || image.bit_depth != 4 {
            return Err(AssetError::KeypadSheetWrongShape(
                image.width,
                image.height,
                image.bit_depth,
            ));
        }
        let expected = (SHEET_WIDTH * SHEET_HEIGHT) as usize;
        if image.pixels.len() != expected {
            return Err(AssetError::KeypadSheetWrongPixelCount(
                expected,
                image.pixels.len(),
            ));
        }
        if let Some((index, &value)) = image
            .pixels
            .iter()
            .enumerate()
            .find(|&(_, &value)| value > MAX_PALETTE_INDEX)
        {
            return Err(AssetError::KeypadSheetInvalidPixel(index, value));
        }
        Ok(Self {
            pixels: image.pixels.to_vec(),
        })
    }

    /// The window-palette index at offset `(x, y)` inside icon `id`, or
    /// `None` for an id outside the thirteen icons or an offset that leaves
    /// the sheet. The caller bounds `(x, y)` by the icon's own size.
    #[must_use]
    pub fn pixel(&self, id: u8, x: u32, y: u32) -> Option<u8> {
        let tile = u32::from(*TILE_OFFSETS.get(usize::from(id))?);
        let sheet_x = (tile % SHEET_TILE_COLUMNS) * TILE_SIZE + x;
        let sheet_y = (tile / SHEET_TILE_COLUMNS) * TILE_SIZE + y;
        if sheet_x >= SHEET_WIDTH || sheet_y >= SHEET_HEIGHT {
            return None;
        }
        self.pixels
            .get((sheet_y * SHEET_WIDTH + sheet_x) as usize)
            .copied()
    }
}

#[cfg(test)]
mod tests;
