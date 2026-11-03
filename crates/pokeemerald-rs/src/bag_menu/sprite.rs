//! 16x16 object sprites cut from a two-frame sheet: the scroll arrows and the
//! swap line draw from sheets of this shape.

use assets::pack::PalettedImage;
use rendering::{Framebuffer, Rgb888};

use super::chrome::put;
use crate::textbox;

/// The side of one sprite frame, and the sheet's width.
pub(super) const SIDE: usize = 16;

/// A 16-pixel-wide sheet of stacked 16x16 frames with its own palette.
#[derive(Debug)]
pub(super) struct SpriteSheet {
    pixels: Vec<u8>,
    palette: Vec<Rgb888>,
}

impl SpriteSheet {
    pub(super) fn from_paletted(source: PalettedImage<'_>) -> Self {
        Self {
            pixels: source.image.pixels.to_vec(),
            palette: textbox::palette_colors(source.palette),
        }
    }

    /// Draws the frame whose first sheet row is `first_row` with its top-left
    /// at `origin`, flipped vertically when `v_flip`. Index 0 stays
    /// transparent and the framebuffer clips the sprite.
    pub(super) fn draw_frame(
        &self,
        fb: &mut Framebuffer,
        origin: (i32, i32),
        first_row: usize,
        v_flip: bool,
    ) {
        for y in 0..SIDE {
            let source_row = first_row + if v_flip { SIDE - 1 - y } else { y };
            for x in 0..SIDE {
                let Some(&index) = self.pixels.get(source_row * SIDE + x) else {
                    continue;
                };
                if index == 0 {
                    continue;
                }
                if let Some(&color) = self.palette.get(usize::from(index)) {
                    put(
                        fb,
                        origin.0 + i32::try_from(x).unwrap_or(0),
                        origin.1 + i32::try_from(y).unwrap_or(0),
                        color,
                    );
                }
            }
        }
    }
}
