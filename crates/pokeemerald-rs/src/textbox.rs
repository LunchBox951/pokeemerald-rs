//! Draws text-window frames and revealed glyphs directly into framebuffers.
//!
//! Revealed glyphs have pixel positions inside a window, so they do not fit
//! the tile-aligned background compositor. Glyphs draw with this module's
//! fixed fallback colors; frame tiles use their extracted palettes unchanged
//! `(behavioral-fidelity)`.

use assets::pack::ImageRef;
use engine::text::render::RevealedGlyph;
use engine::text::window::{self as msgwin, FrameTile};
use rendering::{Bgr555, Framebuffer, Rgb888};

const DEFAULT_GLYPH_BACKGROUND: Option<Rgb888> = None;
const DEFAULT_GLYPH_FOREGROUND: Option<Rgb888> = Some(Rgb888 {
    r: 24,
    g: 24,
    b: 24,
});
const DEFAULT_GLYPH_SHADOW: Option<Rgb888> = Some(Rgb888 {
    r: 160,
    g: 160,
    b: 160,
});
const DEFAULT_GLYPH_BOX: Option<Rgb888> = None;

const GLYPH_COLORS: [Option<Rgb888>; 4] = [
    DEFAULT_GLYPH_BACKGROUND,
    DEFAULT_GLYPH_FOREGROUND,
    DEFAULT_GLYPH_SHADOW,
    DEFAULT_GLYPH_BOX,
];

const TILE_SIZE: usize = msgwin::TILE_SIZE as usize;
const TILE_SIZE_PX: i32 = msgwin::TILE_SIZE.cast_signed();
const GLYPH_SIZE: usize = assets::fonts::GLYPH_SIZE as usize;
// GBA hardware sizes: deriving alone would let a source-constant slip
// resize every frame and glyph in lockstep and still pass the tests.
const _: () = assert!(TILE_SIZE == 8 && GLYPH_SIZE == 16);
const TRANSPARENT_PALETTE_INDEX: usize = 0;
const FRAME_TILE_BIT_DEPTH: u8 = 4;

/// Window-local text origin for the standard field message box.
pub(crate) const STANDARD_PRINTER_ORIGIN: (i32, i32) = (2, 2);

/// Screen origin of the standard field message box's content.
pub(crate) const STANDARD_BOX_SCREEN_ORIGIN: (i32, i32) = (
    msgwin::STANDARD_TILEMAP_LEFT * TILE_SIZE_PX,
    msgwin::STANDARD_TILEMAP_TOP * TILE_SIZE_PX,
);

/// Pixel size of the standard field message box's content.
pub(crate) const STANDARD_BOX_CONTENT_SIZE_PX: (i32, i32) = (
    msgwin::STANDARD_CONTENT_WIDTH * TILE_SIZE_PX,
    msgwin::STANDARD_CONTENT_HEIGHT * TILE_SIZE_PX,
);

/// Which framebuffer pixels a blit actually painted.
#[derive(Debug, Clone)]
pub(crate) struct Coverage {
    painted_pixels: Vec<bool>,
}

impl Coverage {
    /// A tracker sized to the framebuffer, with nothing painted yet.
    pub(crate) fn recording() -> Self {
        Self {
            painted_pixels: vec![false; Framebuffer::WIDTH * Framebuffer::HEIGHT],
        }
    }

    const fn disabled() -> Self {
        Self {
            painted_pixels: Vec::new(),
        }
    }

    /// Whether a blit painted this pixel. A disabled tracker and an
    /// out-of-range coordinate both answer `false`.
    pub(crate) fn is_painted(&self, x: usize, y: usize) -> bool {
        x < Framebuffer::WIDTH
            && self
                .painted_pixels
                .get(y * Framebuffer::WIDTH + x)
                .copied()
                .unwrap_or(false)
    }

    fn mark(&mut self, x: usize, y: usize) {
        if x >= Framebuffer::WIDTH {
            return;
        }
        if let Some(slot) = self.painted_pixels.get_mut(y * Framebuffer::WIDTH + x) {
            *slot = true;
        }
    }
}

/// Draws the final frame tile assigned to each cell.
pub(crate) fn blit_frame_tiles(
    fb: &mut Framebuffer,
    tiles: &[FrameTile],
    sheet: ImageRef<'_>,
    palette: &[Rgb888],
) {
    blit_frame_tiles_tracked(fb, &mut Coverage::disabled(), tiles, sheet, palette);
}

/// Draws frame tiles and records their opaque pixels.
pub(crate) fn blit_frame_tiles_tracked(
    fb: &mut Framebuffer,
    coverage: &mut Coverage,
    tiles: &[FrameTile],
    sheet: ImageRef<'_>,
    palette: &[Rgb888],
) {
    for (tile_index, tile) in tiles.iter().enumerate() {
        // A later tilemap assignment replaces the entire cell, including transparent pixels.
        let is_superseded = tiles[tile_index + 1..]
            .iter()
            .any(|later| later.col == tile.col && later.row == tile.row);
        if is_superseded {
            continue;
        }

        let Some(pixels) = msgwin::tile_pixels_flipped(sheet, tile.tile, tile.v_flip) else {
            continue;
        };
        let origin_x = tile.col * TILE_SIZE_PX;
        let origin_y = tile.row * TILE_SIZE_PX;
        for local_y in 0..TILE_SIZE {
            for local_x in 0..TILE_SIZE {
                let palette_index = usize::from(pixels[local_y * TILE_SIZE + local_x]);
                if palette_index == TRANSPARENT_PALETTE_INDEX {
                    continue;
                }
                let Some(&color) = palette.get(palette_index) else {
                    continue;
                };
                let (Ok(dx), Ok(dy)) = (i32::try_from(local_x), i32::try_from(local_y)) else {
                    continue;
                };
                set_pixel_checked(fb, coverage, origin_x + dx, origin_y + dy, color);
            }
        }
    }
}

/// Draws revealed glyphs inside a window's content bounds using fallback colors.
pub(crate) fn blit_glyphs(
    fb: &mut Framebuffer,
    glyphs: &[RevealedGlyph],
    origin: (i32, i32),
    content_size: (i32, i32),
) {
    blit_glyphs_colored(fb, glyphs, origin, content_size, &GLYPH_COLORS);
}

/// Draws revealed glyphs inside a window's content bounds using caller-supplied colors.
pub(crate) fn blit_glyphs_colored(
    fb: &mut Framebuffer,
    glyphs: &[RevealedGlyph],
    origin: (i32, i32),
    content_size: (i32, i32),
    colors: &[Option<Rgb888>; 4],
) {
    blit_glyphs_colored_tracked(
        fb,
        &mut Coverage::disabled(),
        glyphs,
        origin,
        content_size,
        colors,
    );
}

/// Draws revealed glyphs and records their opaque pixels.
pub(crate) fn blit_glyphs_colored_tracked(
    fb: &mut Framebuffer,
    coverage: &mut Coverage,
    glyphs: &[RevealedGlyph],
    origin: (i32, i32),
    content_size: (i32, i32),
    colors: &[Option<Rgb888>; 4],
) {
    let (content_width, content_height) = content_size;
    for revealed in glyphs {
        for pixel_y in 0..GLYPH_SIZE {
            for pixel_x in 0..GLYPH_SIZE {
                let palette_index =
                    usize::from(revealed.glyph.pixels[pixel_y * GLYPH_SIZE + pixel_x]);
                let Some(Some(color)) = colors.get(palette_index) else {
                    continue;
                };
                let (Ok(pixel_x), Ok(pixel_y)) = (i32::try_from(pixel_x), i32::try_from(pixel_y))
                else {
                    continue;
                };
                let content_x = revealed.x + pixel_x;
                let content_y = revealed.y + pixel_y;
                if content_x < 0
                    || content_y < 0
                    || content_x >= content_width
                    || content_y >= content_height
                {
                    continue;
                }
                let screen_x = origin.0 + content_x;
                let screen_y = origin.1 + content_y;
                set_pixel_checked(fb, coverage, screen_x, screen_y, *color);
            }
        }
    }
}

/// Owned frame tile pixels and their decoded palette.
#[derive(Debug, Clone)]
pub(crate) struct FrameAssets {
    pub(crate) pixels: Vec<u8>,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) palette: Vec<Rgb888>,
}

impl FrameAssets {
    /// Owned copies of a pack window frame's tiles and decoded palette.
    pub(crate) fn from_handle(handle: assets::pack::WindowFrameHandle<'_>) -> Self {
        Self {
            pixels: handle.tiles.pixels.to_vec(),
            width: handle.tiles.width,
            height: handle.tiles.height,
            palette: palette_colors(handle.palette),
        }
    }

    /// The frame tiles as the blitters' borrowed image shape.
    pub(crate) fn image(&self) -> ImageRef<'_> {
        ImageRef {
            width: self.width,
            height: self.height,
            bit_depth: FRAME_TILE_BIT_DEPTH,
            pixels: &self.pixels,
        }
    }
}

/// Decodes every BGR555 entry in a packed palette.
pub(crate) fn palette_colors(palette: assets::pack::PaletteRef<'_>) -> Vec<Rgb888> {
    palette
        .colors()
        .map(|color| Bgr555::from_raw(color).to_rgb888())
        .collect()
}

/// Fills a pixel rectangle and records its coverage.
pub(crate) fn fill_rect_tracked(
    fb: &mut Framebuffer,
    coverage: &mut Coverage,
    origin: (i32, i32),
    width: i32,
    height: i32,
    color: Rgb888,
) {
    for dy in 0..height {
        for dx in 0..width {
            set_pixel_checked(fb, coverage, origin.0 + dx, origin.1 + dy, color);
        }
    }
}

/// Fills a pixel rectangle.
pub(crate) fn fill_rect(
    fb: &mut Framebuffer,
    origin: (i32, i32),
    width: i32,
    height: i32,
    color: Rgb888,
) {
    fill_rect_tracked(fb, &mut Coverage::disabled(), origin, width, height, color);
}

fn set_pixel_checked(fb: &mut Framebuffer, coverage: &mut Coverage, x: i32, y: i32, color: Rgb888) {
    let Ok(x) = usize::try_from(x) else { return };
    let Ok(y) = usize::try_from(y) else { return };
    if x < Framebuffer::WIDTH && y < Framebuffer::HEIGHT {
        fb.set_pixel(x, y, color);
        coverage.mark(x, y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assets::{Glyph, GLYPH_PIXELS};
    use engine::text::render::RevealedGlyph;

    const GLYPH_SIZE_PX: i32 = assets::fonts::GLYPH_SIZE.cast_signed();
    const OPAQUE_GLYPH_PALETTE_INDEX: u8 = 1;
    const FILL_TILE_ID: u8 = 0;
    const BORDER_TILE_ID: u8 = 1;
    const TRANSPARENT_PIXEL: u8 = 0;
    const BORDER_PIXEL: u8 = 2;
    const FILL_PIXEL: u8 = 3;
    const BORDER_COLOR: Rgb888 = Rgb888 { r: 200, g: 0, b: 0 };
    const FILL_COLOR: Rgb888 = Rgb888 { r: 0, g: 200, b: 0 };

    fn opaque_glyph_at(x: i32, y: i32) -> RevealedGlyph {
        RevealedGlyph {
            x,
            y,
            glyph: Glyph {
                advance_width: u8::try_from(msgwin::TILE_SIZE).unwrap(),
                pixels: [OPAQUE_GLYPH_PALETTE_INDEX; GLYPH_PIXELS],
            },
        }
    }

    fn frame_sheet(pixels: &[u8]) -> ImageRef<'_> {
        ImageRef {
            width: msgwin::TILE_SIZE * 2,
            height: msgwin::TILE_SIZE,
            bit_depth: FRAME_TILE_BIT_DEPTH,
            pixels,
        }
    }

    fn fill_then_border_sheet() -> Vec<u8> {
        let sheet_width = TILE_SIZE * 2;
        let mut pixels = vec![TRANSPARENT_PIXEL; sheet_width * TILE_SIZE];
        for y in 0..TILE_SIZE {
            for x in 0..sheet_width {
                pixels[y * sheet_width + x] = if x < TILE_SIZE {
                    FILL_PIXEL
                } else if y == 0 {
                    BORDER_PIXEL
                } else {
                    TRANSPARENT_PIXEL
                };
            }
        }
        pixels
    }

    fn test_palette() -> [Rgb888; 4] {
        [Rgb888::BLACK, Rgb888::BLACK, BORDER_COLOR, FILL_COLOR]
    }

    fn pixel_at(fb: &Framebuffer, point: (i32, i32)) -> Option<Rgb888> {
        let x = usize::try_from(point.0).ok()?;
        let y = usize::try_from(point.1).ok()?;
        fb.pixel(x, y)
    }

    #[test]
    fn blit_glyphs_draws_nothing_for_a_glyph_scrolled_entirely_above_the_content_rect() {
        let mut fb = Framebuffer::new();
        let glyph = opaque_glyph_at(4, -GLYPH_SIZE_PX);

        blit_glyphs(&mut fb, &[glyph], (20, 30), (200, 32));

        assert!(
            fb.pixels().iter().all(|&p| p == Rgb888::BLACK),
            "a glyph fully above the content rect must not touch the framebuffer at all"
        );
    }

    #[test]
    fn blit_glyphs_clips_a_glyph_straddling_the_content_rects_top_edge() {
        let mut fb = Framebuffer::new();
        let origin = (20, 30);
        let glyph = opaque_glyph_at(0, -(GLYPH_SIZE_PX / 2));

        blit_glyphs(&mut fb, &[glyph], origin, (200, 32));

        let row_above_content = usize::try_from(origin.1 - 1).unwrap();
        let first_content_row = usize::try_from(origin.1).unwrap();
        let first_content_column = usize::try_from(origin.0).unwrap();
        assert_eq!(
            fb.pixel(first_content_column, row_above_content),
            Some(Rgb888::BLACK)
        );
        assert_ne!(
            fb.pixel(first_content_column, first_content_row),
            Some(Rgb888::BLACK)
        );
    }

    #[test]
    fn blit_glyphs_clips_a_glyph_past_the_content_rects_right_or_bottom_edge() {
        let mut fb = Framebuffer::new();
        let origin = (0, 0);
        let content_size = (10, 10);
        let glyph = opaque_glyph_at(0, 0);

        blit_glyphs(&mut fb, &[glyph], origin, content_size);

        let inside = (content_size.0 / 2, content_size.1 / 2);
        let past_right_edge = (content_size.0 + 2, inside.1);
        let past_bottom_edge = (inside.0, content_size.1 + 2);
        assert_ne!(pixel_at(&fb, inside), Some(Rgb888::BLACK));
        assert_eq!(pixel_at(&fb, past_right_edge), Some(Rgb888::BLACK));
        assert_eq!(pixel_at(&fb, past_bottom_edge), Some(Rgb888::BLACK));
    }

    #[test]
    fn blit_glyphs_draws_a_fully_in_bounds_glyph_unclipped() {
        let mut fb = Framebuffer::new();
        let origin = (10, 10);
        let glyph = opaque_glyph_at(0, 0);

        blit_glyphs(&mut fb, &[glyph], origin, (200, 100));

        for y in origin.1..origin.1 + GLYPH_SIZE_PX {
            for x in origin.0..origin.0 + GLYPH_SIZE_PX {
                let x = usize::try_from(x).unwrap();
                let y = usize::try_from(y).unwrap();
                assert_ne!(
                    fb.pixel(x, y),
                    Some(Rgb888::BLACK),
                    "({x}, {y}) is within the fully-visible glyph and content rect"
                );
            }
        }
    }

    #[test]
    fn final_frame_tile_replaces_transparent_pixels_in_a_superseded_tile() {
        let pixels = fill_then_border_sheet();
        let sheet = frame_sheet(&pixels);
        let palette = test_palette();
        let tiles = [
            FrameTile {
                col: 0,
                row: 0,
                tile: FILL_TILE_ID,
                v_flip: false,
            },
            FrameTile {
                col: 0,
                row: 0,
                tile: BORDER_TILE_ID,
                v_flip: false,
            },
        ];

        let mut fb = Framebuffer::new();
        blit_frame_tiles(&mut fb, &tiles, sheet, &palette);

        assert_eq!(fb.pixel(0, 0), Some(BORDER_COLOR));
        for y in 1..TILE_SIZE {
            assert_eq!(
                fb.pixel(0, y),
                Some(Rgb888::BLACK),
                "row {y} must not still show the superseded fill tile's pixel"
            );
        }
    }

    #[test]
    fn blit_frame_tiles_draws_every_tile_when_no_two_share_a_cell() {
        let pixels = fill_then_border_sheet();
        let sheet = frame_sheet(&pixels);
        let palette = test_palette();
        let first_cell = FrameTile {
            col: 0,
            row: 0,
            tile: FILL_TILE_ID,
            v_flip: false,
        };
        let second_cell = FrameTile {
            col: 5,
            row: 5,
            tile: FILL_TILE_ID,
            v_flip: false,
        };
        let tiles = [first_cell, second_cell];

        let mut fb = Framebuffer::new();
        blit_frame_tiles(&mut fb, &tiles, sheet, &palette);

        let second_x = usize::try_from(second_cell.col * TILE_SIZE_PX).unwrap();
        let second_y = usize::try_from(second_cell.row * TILE_SIZE_PX).unwrap();
        assert_eq!(fb.pixel(0, 0), Some(FILL_COLOR));
        assert_eq!(fb.pixel(second_x, second_y), Some(FILL_COLOR));
    }
}
