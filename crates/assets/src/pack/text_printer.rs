//! The text printer's page-prompt arrow (upstream `sDownArrowTiles`,
//! `graphics/fonts/down_arrow.png`, `src/text.c:71`).
//!
//! The pack carries the whole 8x48 sheet. `TextPrinterDrawDownArrow` never
//! draws it whole: it blits an 8x16 window of it starting at pixel row
//! `sDownArrowYCoords[downArrowYPosIdx]` (`src/text.c:75,819-829`), a bob of
//! `0, 1, 2, 1` rows, so the four animation frames overlap. The index is a
//! two-bit field (`include/text.h`, `downArrowYPosIdx:2`) that wraps.
//!
//! The sheet has no palette entry: the printer blits its pixel indices into
//! the destination window, where index 0 is transparent
//! (`src/blit.c:58-66`), so the window supplies the colours.
//!
//! The lookup re-validates the shape, since a hand-built or corrupt pack can
//! bypass extraction.

use super::{AssetPack, ImageRef, PackError};

const SHEET_ID: &str = "interface/image/down_arrow";

const SHEET_WIDTH: u32 = 8;
const SHEET_HEIGHT: u32 = 48;
/// The height of one blitted window.
const FRAME_HEIGHT: u32 = 16;
const BIT_DEPTH: u8 = 4;
/// Pixel indices must fit the 4bpp tiles the sheet came from.
const PIXEL_INDEX_LIMIT: u8 = 16;
/// `sDownArrowYCoords` (`src/text.c:75`): the source row of each frame.
const FRAME_ROWS: [u32; 4] = [0, 1, 2, 1];
/// `downArrowYPosIdx` is two bits wide.
const FRAME_INDEX_MASK: u8 = 0b11;

/// The down arrow, borrowing the pack for `'a`.
///
/// [`AssetPack::text_printer_down_arrow`] returns it with the sheet
/// validated; draw a frame with [`frame`](Self::frame).
#[derive(Debug, Clone, Copy)]
pub struct TextPrinterDownArrow<'a> {
    sheet: ImageRef<'a>,
}

impl<'a> TextPrinterDownArrow<'a> {
    /// How many frames the animation cycles through.
    pub const FRAME_COUNT: usize = FRAME_ROWS.len();

    /// The 8x16 window of the sheet the printer blits for `index`, 4bpp
    /// palette indices in row-major order.
    ///
    /// `index` wraps at four, as upstream's two-bit counter does.
    #[must_use]
    pub fn frame(&self, index: u8) -> ImageRef<'a> {
        let row = FRAME_ROWS[usize::from(index & FRAME_INDEX_MASK)];
        let start = to_usize(row * SHEET_WIDTH);
        let len = to_usize(FRAME_HEIGHT * SHEET_WIDTH);
        ImageRef {
            width: SHEET_WIDTH,
            height: FRAME_HEIGHT,
            bit_depth: BIT_DEPTH,
            pixels: &self.sheet.pixels[start..start + len],
        }
    }
}

fn to_usize(value: u32) -> usize {
    usize::try_from(value).expect("a small sheet offset fits usize")
}

impl AssetPack {
    /// The text printer's page-prompt arrow.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] or [`PackError::WrongKind`] for a missing or
    /// wrong-kind sheet; [`PackError::MalformedTextPrinterAsset`] when its
    /// dimensions, bit depth, payload length, or pixel indices differ from
    /// upstream.
    pub fn text_printer_down_arrow(&self) -> Result<TextPrinterDownArrow<'_>, PackError> {
        let sheet = self.image(SHEET_ID)?;
        if (sheet.width, sheet.height) != (SHEET_WIDTH, SHEET_HEIGHT) {
            return Err(malformed(format!(
                "is {}x{}: expected {SHEET_WIDTH}x{SHEET_HEIGHT}",
                sheet.width, sheet.height
            )));
        }
        if sheet.bit_depth != BIT_DEPTH {
            return Err(malformed(format!(
                "has bit depth {}: expected {BIT_DEPTH}",
                sheet.bit_depth
            )));
        }
        if sheet.pixels.len() != to_usize(SHEET_WIDTH * SHEET_HEIGHT) {
            return Err(malformed(format!(
                "carries {} payload bytes: expected one per pixel",
                sheet.pixels.len()
            )));
        }
        if let Some(&pixel) = sheet
            .pixels
            .iter()
            .find(|&&pixel| pixel >= PIXEL_INDEX_LIMIT)
        {
            return Err(malformed(format!(
                "has pixel index {pixel}: a 4bpp index is below {PIXEL_INDEX_LIMIT}"
            )));
        }
        Ok(TextPrinterDownArrow { sheet })
    }
}

fn malformed(detail: String) -> PackError {
    PackError::MalformedTextPrinterAsset {
        id: SHEET_ID.to_owned(),
        detail,
    }
}
