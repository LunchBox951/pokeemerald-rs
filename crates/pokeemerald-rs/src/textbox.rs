//! Draws text-window frames and revealed glyphs directly into framebuffers.
//!
//! Revealed glyphs have pixel positions inside a window, so they do not fit
//! the tile-aligned background compositor. Glyphs and frame tiles both draw
//! with colors read from the loaded window palette `(behavioral-fidelity)`.

use assets::pack::ImageRef;
use engine::text::render::{ClearedSpan, RevealedGlyph, TextColors};
use engine::text::window::{self as msgwin, FrameTile};
use rendering::{Bgr555, Framebuffer, Rgb888};

/// Palette indices upstream's message printers select for text:
/// `TEXT_COLOR_DARK_GRAY` (foreground) and `TEXT_COLOR_LIGHT_GRAY` (shadow)
/// (`pokeemerald/src/menu.c:191-201`,
/// `pokeemerald/include/constants/characters.h:234-237`) -- indices into the
/// palette `LoadMessageBoxGfx` loads for the window
/// (`pokeemerald/src/text_window.c:93-97`). They are [`TextColors::default`]'s
/// fields; a printer's inline `COLOR`/`SHADOW` controls override them per
/// glyph through [`GlyphColors::Palette`].
const MESSAGE_BOX_FOREGROUND_PALETTE_INDEX: usize = TextColors::MESSAGE_BOX.foreground as usize;
const MESSAGE_BOX_SHADOW_PALETTE_INDEX: usize = TextColors::MESSAGE_BOX.shadow as usize;
/// The window-interior palette index the message box is filled with before
/// any text prints (`TEXT_COLOR_WHITE`).
const MESSAGE_BOX_BACKGROUND_PALETTE_INDEX: u8 = TextColors::MESSAGE_BOX.background;

/// Glyph-role color slots [`blit_glyphs_colored`] indexes a revealed glyph's
/// pixel through -- distinct from the palette indices above.
const GLYPH_COLOR_COUNT: usize = 4;
const GLYPH_BACKGROUND_INDEX: u8 = 0;
const GLYPH_FOREGROUND_INDEX: usize = 1;
const GLYPH_SHADOW_INDEX: usize = 2;

const TILE_SIZE: usize = msgwin::TILE_SIZE as usize;
const TILE_SIZE_PX: i32 = msgwin::TILE_SIZE.cast_signed();
const GLYPH_SIZE: usize = assets::fonts::GLYPH_SIZE as usize;
// GBA hardware sizes: deriving alone would let a source-constant slip
// resize every frame and glyph in lockstep and still pass the tests.
const _: () = assert!(TILE_SIZE == 8 && GLYPH_SIZE == 16);
const TRANSPARENT_PALETTE_INDEX: usize = 0;
const FRAME_TILE_BIT_DEPTH: u8 = 4;

/// Window-local text origin `AddTextPrinterParameterized2` gives every message
/// printer: `x = currentX = 0`, `y = currentY = 1` (`pokeemerald/src/menu.c:169-180`).
pub(crate) const STANDARD_PRINTER_ORIGIN: (i32, i32) = (0, 1);

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
        if x >= Framebuffer::WIDTH || y >= Framebuffer::HEIGHT {
            return false;
        }
        self.painted_pixels
            .get(y * Framebuffer::WIDTH + x)
            .copied()
            .unwrap_or(false)
    }

    fn mark(&mut self, x: usize, y: usize) {
        if x >= Framebuffer::WIDTH || y >= Framebuffer::HEIGHT {
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

/// Where [`blit_glyphs_colored`] gets a glyph pixel's color.
#[derive(Debug, Clone, Copy)]
pub(crate) enum GlyphColors<'a> {
    /// Fixed role slots shared by every glyph: slot 1 foreground, slot 2
    /// shadow (see [`FrameAssets::glyph_colors`]); `None` is transparent.
    Roles(&'a [Option<Rgb888>; GLYPH_COLOR_COUNT]),
    /// The window palette: each glyph maps its own carried
    /// [`TextColors`] through it, as upstream's
    /// `GenerateFontHalfRowLookupTable` does (`pokeemerald/src/text.c:363-420`).
    /// Palette index 0 is transparent, not `palette[0]`.
    Palette(&'a [Rgb888]),
}

impl<'a> From<&'a [Option<Rgb888>; GLYPH_COLOR_COUNT]> for GlyphColors<'a> {
    fn from(roles: &'a [Option<Rgb888>; GLYPH_COLOR_COUNT]) -> Self {
        Self::Roles(roles)
    }
}

impl GlyphColors<'_> {
    /// The color `revealed`'s pixel `value` at glyph-local (`x`, `y`) paints,
    /// or `None` to leave the framebuffer alone.
    fn resolve(self, revealed: &RevealedGlyph, value: u8, x: usize, y: usize) -> Option<Rgb888> {
        match self {
            Self::Roles(roles) => *roles.get(usize::from(value))?,
            Self::Palette(palette) => {
                let colors = revealed.colors;
                let index = match usize::from(value) {
                    GLYPH_FOREGROUND_INDEX => colors.foreground,
                    GLYPH_SHADOW_INDEX => colors.shadow,
                    _ if value == GLYPH_BACKGROUND_INDEX
                        && revealed.paint_background
                        && i32::try_from(x)
                            .is_ok_and(|x| x < i32::from(revealed.glyph.advance_width))
                        && i32::try_from(y).is_ok_and(|y| y < revealed.raster_height) =>
                    {
                        colors.background
                    }
                    _ => return None,
                };
                palette_entry(palette, index)
            }
        }
    }
}

/// `palette[index]`, with index 0 transparent and a missing entry skipped.
fn palette_entry(palette: &[Rgb888], index: u8) -> Option<Rgb888> {
    if usize::from(index) == TRANSPARENT_PALETTE_INDEX {
        return None;
    }
    palette.get(usize::from(index)).copied()
}

/// Draws revealed glyphs inside a window's content bounds using caller-supplied colors.
pub(crate) fn blit_glyphs_colored<'a>(
    fb: &mut Framebuffer,
    glyphs: &[RevealedGlyph],
    origin: (i32, i32),
    content_size: (i32, i32),
    colors: impl Into<GlyphColors<'a>>,
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
pub(crate) fn blit_glyphs_colored_tracked<'a>(
    fb: &mut Framebuffer,
    coverage: &mut Coverage,
    glyphs: &[RevealedGlyph],
    origin: (i32, i32),
    content_size: (i32, i32),
    colors: impl Into<GlyphColors<'a>>,
) {
    let colors = colors.into();
    let (content_width, content_height) = content_size;
    for revealed in glyphs {
        for pixel_y in 0..GLYPH_SIZE {
            for pixel_x in 0..GLYPH_SIZE {
                let value = revealed.glyph.pixels[pixel_y * GLYPH_SIZE + pixel_x];
                let Some(color) = colors.resolve(revealed, value, pixel_x, pixel_y) else {
                    continue;
                };
                let (Ok(pixel_x), Ok(pixel_y)) = (i32::try_from(pixel_x), i32::try_from(pixel_y))
                else {
                    continue;
                };
                let (Some(content_x), Some(content_y)) = (
                    revealed.x.checked_add(pixel_x),
                    revealed.y.checked_add(pixel_y),
                ) else {
                    continue;
                };
                if content_x < 0
                    || content_y < 0
                    || content_x >= content_width
                    || content_y >= content_height
                {
                    continue;
                }
                let screen_x = origin.0 + content_x;
                let screen_y = origin.1 + content_y;
                set_pixel_checked(fb, coverage, screen_x, screen_y, color);
            }
        }
    }
}

/// One retained window effect, in the order the printer produced it.
///
/// A dialog or intro scene keeps these instead of a flat glyph list so a
/// `CLEAR`/`CLEAR_TO` can restore only its own rectangle -- including one
/// that lands mid-glyph after a backward cursor move -- while glyphs
/// revealed later still draw on top of the restored patch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WindowOp {
    /// A glyph the printer revealed. Boxed: [`RevealedGlyph`] carries a full
    /// glyph bitmap, dwarfing [`ClearedSpan`] and bloating every `WindowOp`.
    Glyph(Box<RevealedGlyph>),
    /// A span a `CLEAR`/`CLEAR_TO` control erased (`Printer::cleared_span`).
    ClearSpan(ClearedSpan),
}

impl WindowOp {
    /// Shifts this op vertically by a window scroll.
    pub(crate) fn shift_y(&mut self, dy: i32) {
        match self {
            Self::Glyph(glyph) => glyph.y = glyph.y.saturating_add(dy),
            Self::ClearSpan(span) => span.y = span.y.saturating_add(dy),
        }
    }
}

/// Replays ordered glyph and clear-span ops onto `fb`, which must already
/// hold the window's frame tiles and nothing else.
///
/// Each glyph maps the [`TextColors`] it carries through `palette`, the
/// window's loaded palette, so a `COLOR` or `SHADOW` control recolors only the
/// glyphs revealed after it. `window_fill` is the colors the latest
/// whole-window clear erased with (`Printer::cleared_window_colors`); `scene`
/// is the framebuffer as it was before the frame tiles were drawn, which a
/// transparent (index 0) fill shows.
///
/// A `ClearSpan` erases its window-local rectangle with its own background
/// (see [`erase_with_background`]) before any later glyph lands, covering
/// whatever an earlier op painted there, partial glyph included
/// (`pokeemerald/src/text.c:649-674`, `:1063-1090`).
pub(crate) fn compose_window_ops(
    fb: &mut Framebuffer,
    ops: &[WindowOp],
    origin: (i32, i32),
    content_size: (i32, i32),
    palette: &[Rgb888],
    window_fill: Option<TextColors>,
    scene: Option<&Framebuffer>,
) {
    let background = fb.clone();
    if let Some(fill) = window_fill {
        let full = ClearedSpan {
            x: 0,
            y: 0,
            width: content_size.0,
            height: content_size.1,
            colors: fill,
        };
        erase_with_background(fb, &background, scene, palette, full, origin, content_size);
    }
    for op in ops {
        match op {
            WindowOp::Glyph(glyph) => {
                blit_glyphs_colored(
                    fb,
                    std::slice::from_ref(glyph.as_ref()),
                    origin,
                    content_size,
                    GlyphColors::Palette(palette),
                );
            }
            WindowOp::ClearSpan(span) => {
                erase_with_background(fb, &background, None, palette, *span, origin, content_size);
            }
        }
    }
}

/// Erases `span` with `span.colors.background`.
///
/// Index 0 is transparent: a span paints nothing, and a whole-window fill
/// (which passes its `scene`) shows the scene beneath the window. Index
/// `TEXT_COLOR_WHITE` is the window interior the frame tiles already drew, so
/// it restores from `background`, the pre-glyph composed framebuffer. Any other
/// index fills with that palette entry; a missing entry is skipped.
fn erase_with_background(
    fb: &mut Framebuffer,
    background: &Framebuffer,
    scene: Option<&Framebuffer>,
    palette: &[Rgb888],
    span: ClearedSpan,
    origin: (i32, i32),
    content_size: (i32, i32),
) {
    match span.colors.background {
        index if usize::from(index) == TRANSPARENT_PALETTE_INDEX => {
            if let Some(scene) = scene {
                restore_cleared_span(fb, scene, span, origin, content_size);
            }
        }
        MESSAGE_BOX_BACKGROUND_PALETTE_INDEX => {
            restore_cleared_span(fb, background, span, origin, content_size);
        }
        index => {
            if let Some(&color) = palette.get(usize::from(index)) {
                paint_cleared_span(fb, color, span, origin, content_size);
            }
        }
    }
}

/// Fills `span`'s window-local rectangle with `color`, clipped the way
/// [`restore_cleared_span`] clips.
fn paint_cleared_span(
    fb: &mut Framebuffer,
    color: Rgb888,
    span: ClearedSpan,
    origin: (i32, i32),
    content_size: (i32, i32),
) {
    for_each_span_pixel(span, origin, content_size, |x, y| fb.set_pixel(x, y, color));
}

/// Calls `visit` with the framebuffer position of every pixel of `span` that
/// lies inside the content bounds. A pixel whose coordinate leaves the `i32`
/// range lies outside the content and is skipped.
fn for_each_span_pixel(
    span: ClearedSpan,
    origin: (i32, i32),
    content_size: (i32, i32),
    mut visit: impl FnMut(usize, usize),
) {
    let (content_width, content_height) = content_size;
    for dy in 0..span.height {
        for dx in 0..span.width {
            let (Some(content_x), Some(content_y)) =
                (span.x.checked_add(dx), span.y.checked_add(dy))
            else {
                continue;
            };
            if content_x < 0
                || content_y < 0
                || content_x >= content_width
                || content_y >= content_height
            {
                continue;
            }
            let screen_x = origin.0 + content_x;
            let screen_y = origin.1 + content_y;
            let (Ok(x), Ok(y)) = (usize::try_from(screen_x), usize::try_from(screen_y)) else {
                continue;
            };
            visit(x, y);
        }
    }
}

/// Copies `span`'s window-local rectangle from `background` onto `fb`,
/// clipped to the content bounds the same way [`blit_glyphs_colored_tracked`]
/// clips a glyph.
fn restore_cleared_span(
    fb: &mut Framebuffer,
    background: &Framebuffer,
    span: ClearedSpan,
    origin: (i32, i32),
    content_size: (i32, i32),
) {
    for_each_span_pixel(span, origin, content_size, |x, y| {
        if let Some(color) = background.pixel(x, y) {
            fb.set_pixel(x, y, color);
        }
    });
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

    /// Message-box glyph colors read from this frame's loaded palette, at
    /// the indices upstream's message printers select
    /// (`pokeemerald/src/menu.c:191-201`, `pokeemerald/src/text_window.c:93-97`).
    /// A palette shorter than the loaded index leaves that role transparent.
    ///
    /// These are fixed role slots for callers that blit glyphs with one
    /// default color pair ([`GlyphColors::Roles`]); the message-box and intro
    /// compositors instead map each glyph's carried inline `COLOR`/`SHADOW`
    /// selection through the palette ([`compose_window_ops`]).
    pub(crate) fn glyph_colors(&self) -> [Option<Rgb888>; GLYPH_COLOR_COUNT] {
        let palette_color = |index: usize| self.palette.get(index).copied();
        let mut colors = [None; GLYPH_COLOR_COUNT];
        colors[GLYPH_FOREGROUND_INDEX] = palette_color(MESSAGE_BOX_FOREGROUND_PALETTE_INDEX);
        colors[GLYPH_SHADOW_INDEX] = palette_color(MESSAGE_BOX_SHADOW_PALETTE_INDEX);
        colors
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
    const OPAQUE_GLYPH_COLOR: Rgb888 = Rgb888 {
        r: 24,
        g: 24,
        b: 24,
    };

    /// Test-only glyph colors: only [`OPAQUE_GLYPH_PALETTE_INDEX`] paints,
    /// matching the shape [`FrameAssets::glyph_colors`] produces.
    const TEST_GLYPH_COLORS: [Option<Rgb888>; GLYPH_COLOR_COUNT] = {
        let mut colors = [None; GLYPH_COLOR_COUNT];
        colors[OPAQUE_GLYPH_PALETTE_INDEX as usize] = Some(OPAQUE_GLYPH_COLOR);
        colors
    };

    fn opaque_glyph_at(x: i32, y: i32) -> RevealedGlyph {
        RevealedGlyph {
            x,
            y,
            glyph: Glyph {
                advance_width: u8::try_from(msgwin::TILE_SIZE).unwrap(),
                pixels: [OPAQUE_GLYPH_PALETTE_INDEX; GLYPH_PIXELS],
            },
            colors: TextColors::default(),
            raster_height: 15,
            paint_background: false,
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

        blit_glyphs_colored(&mut fb, &[glyph], (20, 30), (200, 32), &TEST_GLYPH_COLORS);

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

        blit_glyphs_colored(&mut fb, &[glyph], origin, (200, 32), &TEST_GLYPH_COLORS);

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

        blit_glyphs_colored(&mut fb, &[glyph], origin, content_size, &TEST_GLYPH_COLORS);

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

        blit_glyphs_colored(&mut fb, &[glyph], origin, (200, 100), &TEST_GLYPH_COLORS);

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
    fn blit_glyphs_skips_a_glyph_at_the_i32_maximum_without_overflowing() {
        let mut fb = Framebuffer::new();
        let glyph = opaque_glyph_at(i32::MAX, i32::MAX);

        blit_glyphs_colored(&mut fb, &[glyph], (0, 0), (200, 32), &TEST_GLYPH_COLORS);

        assert!(fb.pixels().iter().all(|&p| p == Rgb888::BLACK));
    }

    #[test]
    fn restore_cleared_span_skips_a_span_reaching_past_the_i32_maximum() {
        let mut fb = Framebuffer::new();
        let background = fb.clone();
        let span = ClearedSpan {
            x: i32::MAX - 10,
            y: i32::MAX,
            width: 255,
            height: 14,
            colors: TextColors::default(),
        };

        restore_cleared_span(&mut fb, &background, span, (0, 0), (200, 32));

        assert!(fb.pixels().iter().all(|&p| p == Rgb888::BLACK));
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

    // `WIDTH` is 240 == 15 * 2^4, so this `y` makes `y * WIDTH` a multiple
    // of `2^usize::BITS`: an unchecked index would wrap back to the origin.
    #[test]
    fn coverage_rejects_out_of_range_y_that_would_alias_a_painted_pixel() {
        let mut coverage = Coverage::recording();
        coverage.mark(0, 0);
        let wrapping_y = 1usize << (usize::BITS - 4);

        assert!(coverage.is_painted(0, 0));
        assert!(!coverage.is_painted(0, wrapping_y));
    }

    #[test]
    fn coverage_mark_rejects_out_of_range_y_without_touching_real_pixels() {
        let mut coverage = Coverage::recording();
        let wrapping_y = 1usize << (usize::BITS - 4);

        coverage.mark(0, usize::MAX);
        coverage.mark(0, wrapping_y);

        assert!(!coverage.is_painted(0, 0));
    }

    /// A window palette whose every entry is distinct, so a wrong index shows.
    fn distinct_palette() -> Vec<Rgb888> {
        (0..16u8)
            .map(|i| Rgb888 {
                r: 10 + i,
                g: 100 + i,
                b: 200 + i,
            })
            .collect()
    }

    /// A 6-wide glyph: foreground at (0,0), shadow at (1,0), background
    /// pixels at (2,0) inside its footprint, (10,0) beyond its advance width,
    /// and (2,20) below the raster height.
    fn mixed_glyph(colors: TextColors, paint_background: bool) -> RevealedGlyph {
        let mut pixels = [0u8; GLYPH_PIXELS];
        pixels[0] = 1;
        pixels[1] = 2;
        pixels[3] = 3;
        let mut glyph = opaque_glyph_at(0, 0);
        glyph.glyph = Glyph {
            advance_width: 6,
            pixels,
        };
        glyph.colors = colors;
        glyph.paint_background = paint_background;
        glyph
    }

    fn colors(foreground: u8, background: u8, shadow: u8) -> TextColors {
        TextColors {
            foreground,
            background,
            shadow,
        }
    }

    fn marker_fb() -> Framebuffer {
        let mut fb = Framebuffer::new();
        fb.fill(Rgb888 { r: 1, g: 2, b: 3 });
        fb
    }

    fn glyph_ops(glyphs: Vec<RevealedGlyph>) -> Vec<WindowOp> {
        glyphs
            .into_iter()
            .map(|g| WindowOp::Glyph(Box::new(g)))
            .collect()
    }

    #[test]
    fn a_color_blue_glyph_paints_palette_index_8_where_the_plain_one_paints_2() {
        let palette = distinct_palette();
        let marker = marker_fb();
        let paint = |c: TextColors| {
            let mut fb = marker.clone();
            compose_window_ops(
                &mut fb,
                &glyph_ops(vec![mixed_glyph(c, false)]),
                (0, 0),
                (200, 32),
                &palette,
                None,
                None,
            );
            fb
        };
        let plain = paint(TextColors::default());
        let blue = paint(colors(8, 1, 3));
        assert_eq!(plain.pixel(0, 0), Some(palette[2]));
        assert_eq!(blue.pixel(0, 0), Some(palette[8]));
        // The shadow and the untouched background are unaffected by COLOR.
        assert_eq!(plain.pixel(1, 0), Some(palette[3]));
        assert_eq!(blue.pixel(1, 0), Some(palette[3]));
        assert_eq!(blue.pixel(2, 0), marker.pixel(2, 0));
        // Role 3 never paints.
        assert_eq!(blue.pixel(3, 0), marker.pixel(3, 0));
        let shadowed = paint(colors(2, 1, 9));
        assert_eq!(shadowed.pixel(0, 0), Some(palette[2]));
        assert_eq!(shadowed.pixel(1, 0), Some(palette[9]));
    }

    #[test]
    fn colors_apply_per_glyph_not_to_the_whole_page() {
        let palette = distinct_palette();
        let mut first = mixed_glyph(TextColors::default(), false);
        let mut second = mixed_glyph(colors(8, 1, 3), false);
        first.x = 0;
        second.x = 20;
        let mut fb = marker_fb();
        compose_window_ops(
            &mut fb,
            &glyph_ops(vec![first, second]),
            (0, 0),
            (200, 32),
            &palette,
            None,
            None,
        );
        assert_eq!(fb.pixel(0, 0), Some(palette[2]));
        assert_eq!(fb.pixel(20, 0), Some(palette[8]));
    }

    #[test]
    fn default_colors_through_the_palette_match_the_fixed_role_slots_byte_for_byte() {
        let frame = FrameAssets {
            pixels: Vec::new(),
            width: 0,
            height: 0,
            palette: distinct_palette(),
        };
        let mut pixels = [0u8; GLYPH_PIXELS];
        for (i, pixel) in pixels.iter_mut().enumerate() {
            *pixel = u8::try_from(i % 4).unwrap();
        }
        let mut glyph = mixed_glyph(TextColors::default(), false);
        glyph.glyph.pixels = pixels;

        let mut roles = marker_fb();
        blit_glyphs_colored(
            &mut roles,
            std::slice::from_ref(&glyph),
            (0, 0),
            (200, 32),
            &frame.glyph_colors(),
        );
        let mut via_palette = marker_fb();
        compose_window_ops(
            &mut via_palette,
            &glyph_ops(vec![glyph]),
            (0, 0),
            (200, 32),
            &frame.palette,
            None,
            None,
        );
        assert_eq!(roles.pixels(), via_palette.pixels());
    }

    #[test]
    fn a_highlight_paints_only_the_glyphs_actual_footprint() {
        let palette = distinct_palette();
        let mut fb = marker_fb();
        compose_window_ops(
            &mut fb,
            &glyph_ops(vec![mixed_glyph(colors(2, 4, 3), true)]),
            (0, 0),
            (200, 32),
            &palette,
            None,
            None,
        );
        assert_eq!(fb.pixel(2, 0), Some(palette[4]), "inside the footprint");
        assert_eq!(
            fb.pixel(10, 0),
            marker_fb().pixel(10, 0),
            "past the advance"
        );
        assert_eq!(
            fb.pixel(2, 20),
            marker_fb().pixel(2, 20),
            "below the raster"
        );
        assert_eq!(fb.pixel(2, 14), Some(palette[4]), "last raster row");
    }

    #[test]
    fn an_explicit_highlight_of_the_default_background_repaints_it() {
        let palette = distinct_palette();
        let mut fb = marker_fb();
        compose_window_ops(
            &mut fb,
            &glyph_ops(vec![mixed_glyph(colors(2, 1, 3), true)]),
            (0, 0),
            (200, 32),
            &palette,
            None,
            None,
        );
        assert_eq!(fb.pixel(2, 0), Some(palette[1]));
    }

    #[test]
    fn transparent_and_missing_palette_entries_skip_safely() {
        let palette = distinct_palette();
        let mut fb = marker_fb();
        compose_window_ops(
            &mut fb,
            &glyph_ops(vec![mixed_glyph(colors(0, 0, 200), true)]),
            (0, 0),
            (200, 32),
            &palette,
            None,
            None,
        );
        assert_eq!(fb.pixels(), marker_fb().pixels());
    }

    fn span_with(background: u8) -> WindowOp {
        WindowOp::ClearSpan(ClearedSpan {
            x: 0,
            y: 0,
            width: 4,
            height: 2,
            colors: colors(2, background, 3),
        })
    }

    #[test]
    fn a_clear_erases_with_its_own_background() {
        let palette = distinct_palette();
        let marker = marker_fb();
        let compose = |background: u8| {
            let mut fb = marker.clone();
            let mut ops = glyph_ops(vec![mixed_glyph(TextColors::default(), false)]);
            ops.push(span_with(background));
            compose_window_ops(&mut fb, &ops, (0, 0), (200, 32), &palette, None, None);
            fb
        };
        // Index 1 restores the window interior, covering the glyph.
        let white = compose(1);
        assert_eq!(white.pixel(0, 0), marker.pixel(0, 0));
        // Another index fills with that palette entry.
        let red = compose(4);
        assert_eq!(red.pixel(0, 0), Some(palette[4]));
        assert_eq!(red.pixel(3, 1), Some(palette[4]));
        assert_eq!(red.pixel(4, 0), marker.pixel(4, 0));
        // Transparent erases nothing.
        let transparent = compose(0);
        assert_eq!(transparent.pixel(0, 0), Some(palette[2]));
    }

    #[test]
    fn a_window_fill_covers_the_content_with_its_background() {
        let palette = distinct_palette();
        let marker = marker_fb();
        let scene = {
            let mut scene = Framebuffer::new();
            scene.fill(Rgb888 { r: 9, g: 9, b: 9 });
            scene
        };
        let compose = |background: u8| {
            let mut fb = marker.clone();
            compose_window_ops(
                &mut fb,
                &[],
                (8, 8),
                (16, 16),
                &palette,
                Some(colors(2, background, 3)),
                Some(&scene),
            );
            fb
        };
        let filled = compose(4);
        assert_eq!(filled.pixel(8, 8), Some(palette[4]));
        assert_eq!(filled.pixel(23, 23), Some(palette[4]));
        assert_eq!(
            filled.pixel(24, 24),
            marker.pixel(24, 24),
            "border untouched"
        );
        assert_eq!(compose(1).pixels(), marker.pixels());
        let transparent = compose(0);
        assert_eq!(transparent.pixel(8, 8), scene.pixel(8, 8));
        assert_eq!(transparent.pixel(7, 7), marker.pixel(7, 7));
    }
}
