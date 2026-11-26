//! The text printer's page-prompt arrows: the light one (upstream
//! `sDownArrowTiles`, `graphics/fonts/down_arrow.png`, `src/text.c:71`) and
//! the dark one (`sDarkDownArrowTiles`, `graphics/fonts/down_arrow_alt.png`,
//! `src/text.c:72`) that windows setting `gTextFlags.useAlternateDownArrow`
//! blit instead (`src/text.c:808-817`).
//!
//! The pack carries each whole 8x48 sheet. `TextPrinterDrawDownArrow` never
//! draws it whole: it blits an 8x16 window of it starting at pixel row
//! `sDownArrowYCoords[downArrowYPosIdx]` (`src/text.c:75,819-829`), a bob of
//! `0, 1, 2, 1` rows, so the four animation frames overlap. The index is a
//! two-bit field (`include/text.h`, `downArrowYPosIdx:2`) that wraps.
//!
//! Neither sheet has a palette entry: the printer blits its pixel indices into
//! the destination window, where index 0 is transparent
//! (`src/blit.c:58-66`), so the window supplies the colours. The two sheets
//! share one shape and one frame table, so both accessors return the same
//! [`TextPrinterDownArrow`].
//!
//! The lookup re-validates the shape, since a hand-built or corrupt pack can
//! bypass extraction.

use super::{AssetPack, ImageRef, PackError};

const SHEET_ID: &str = "interface/image/down_arrow";
const DARK_SHEET_ID: &str = "interface/image/down_arrow_alt";

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

/// A down arrow, borrowing the pack for `'a`.
///
/// [`AssetPack::text_printer_down_arrow`] and
/// [`AssetPack::text_printer_dark_down_arrow`] return it with the sheet
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
    /// The text printer's light page-prompt arrow (`sDownArrowTiles`).
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] or [`PackError::WrongKind`] for a missing or
    /// wrong-kind sheet; [`PackError::MalformedTextPrinterAsset`] when its
    /// dimensions, bit depth, payload length, or pixel indices differ from
    /// upstream.
    pub fn text_printer_down_arrow(&self) -> Result<TextPrinterDownArrow<'_>, PackError> {
        self.down_arrow(SHEET_ID)
    }

    /// The text printer's dark page-prompt arrow (`sDarkDownArrowTiles`),
    /// the one windows with `useAlternateDownArrow` blit
    /// (`src/text.c:808-817`). Same shape contract and frames as
    /// [`text_printer_down_arrow`](Self::text_printer_down_arrow).
    ///
    /// # Errors
    ///
    /// The same as [`text_printer_down_arrow`](Self::text_printer_down_arrow),
    /// for the dark sheet.
    pub fn text_printer_dark_down_arrow(&self) -> Result<TextPrinterDownArrow<'_>, PackError> {
        self.down_arrow(DARK_SHEET_ID)
    }

    fn down_arrow(&self, id: &str) -> Result<TextPrinterDownArrow<'_>, PackError> {
        let sheet = self.image(id)?;
        if (sheet.width, sheet.height) != (SHEET_WIDTH, SHEET_HEIGHT) {
            return Err(malformed(
                id,
                format!(
                    "is {}x{}: expected {SHEET_WIDTH}x{SHEET_HEIGHT}",
                    sheet.width, sheet.height
                ),
            ));
        }
        if sheet.bit_depth != BIT_DEPTH {
            return Err(malformed(
                id,
                format!("has bit depth {}: expected {BIT_DEPTH}", sheet.bit_depth),
            ));
        }
        if sheet.pixels.len() != to_usize(SHEET_WIDTH * SHEET_HEIGHT) {
            return Err(malformed(
                id,
                format!(
                    "carries {} payload bytes: expected one per pixel",
                    sheet.pixels.len()
                ),
            ));
        }
        if let Some(&pixel) = sheet
            .pixels
            .iter()
            .find(|&&pixel| pixel >= PIXEL_INDEX_LIMIT)
        {
            return Err(malformed(
                id,
                format!("has pixel index {pixel}: a 4bpp index is below {PIXEL_INDEX_LIMIT}"),
            ));
        }
        Ok(TextPrinterDownArrow { sheet })
    }
}

fn malformed(id: &str, detail: String) -> PackError {
    PackError::MalformedTextPrinterAsset {
        id: id.to_owned(),
        detail,
    }
}
