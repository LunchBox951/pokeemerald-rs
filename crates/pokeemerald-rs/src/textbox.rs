//! Draws text-window frames, revealed glyphs, and keypad icons directly into
//! framebuffers.
//!
//! Revealed glyphs have pixel positions inside a window, so they do not fit
//! the tile-aligned background compositor. Glyphs, keypad icons, and frame
//! tiles all draw with colors read from the loaded window palette
//! `(behavioral-fidelity)`.

use assets::pack::ImageRef;
use assets::KeypadIconSheet;
use engine::text::render::{ClearedSpan, KeypadIconPlacement, RevealedGlyph, TextColors};
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
const GLYPH_BACKGROUND_INDEX: usize = 0;
/// Glyph value the real fonts use for the rest of a cell's background.
const GLYPH_CELL_BACKGROUND_INDEX: usize = 3;
const GLYPH_FOREGROUND_INDEX: usize = 1;
const GLYPH_SHADOW_INDEX: usize = 2;

mod glyph_colors;
pub(crate) use glyph_colors::GlyphColors;

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

/// Draws one keypad icon at its window-local placement, clipped to the
/// content bounds the same way [`blit_glyphs_colored_tracked`] clips a glyph.
///
/// Upstream's `DrawKeypadIcon` blits the sheet rectangle with colour key `0`
/// and copies every other index unchanged into the window
/// (`pokeemerald/src/text.c:1609-1623`, `pokeemerald/src/window.c:398-413`), so
/// an icon pixel indexes the whole window `palette` directly rather than the
/// printer's glyph-role colours.
fn blit_keypad_icon(
    fb: &mut Framebuffer,
    icon: KeypadIconPlacement,
    icons: &KeypadIconSheet,
    palette: &[Rgb888],
    origin: (i32, i32),
    content_size: (i32, i32),
) {
    let (content_width, content_height) = content_size;
    for pixel_y in 0..u32::from(icon.height) {
        for pixel_x in 0..u32::from(icon.width) {
            let Some(palette_index) = icons.pixel(icon.id, pixel_x, pixel_y) else {
                continue;
            };
            if usize::from(palette_index) == TRANSPARENT_PALETTE_INDEX {
                continue;
            }
            let Some(&color) = palette.get(usize::from(palette_index)) else {
                continue;
            };
            let (Ok(pixel_x), Ok(pixel_y)) = (i32::try_from(pixel_x), i32::try_from(pixel_y))
            else {
                continue;
            };
            let (Some(content_x), Some(content_y)) =
                (icon.x.checked_add(pixel_x), icon.y.checked_add(pixel_y))
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
            set_pixel_checked(
                fb,
                &mut Coverage::disabled(),
                origin.0 + content_x,
                origin.1 + content_y,
                color,
            );
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
    /// A keypad icon the printer placed (`Printer::keypad_icon`).
    KeypadIcon(KeypadIconPlacement),
    /// The rows a window scroll exposed ([`scroll_window_ops`]), filled with
    /// the background in force. Unlike a `ClearSpan`, a background of index 0
    /// writes transparent pixels, as `ScrollWindow`'s `PIXEL_FILL(0)` does.
    ScrollFill(ClearedSpan),
}

/// Scrolls a window's retained `ops` up by `dy` and fills the rows that
/// scroll into view with `colors.background`, as `ScrollWindow` does with
/// `PIXEL_FILL(bgColor)` (`pokeemerald/src/text.c:1196-1201`).
///
/// The fill is a [`WindowOp::ScrollFill`] across the content width at the
/// bottom `dy` rows, so a later scroll carries it up with everything else.
pub(crate) fn scroll_window_ops(
    ops: &mut Vec<WindowOp>,
    dy: i32,
    colors: TextColors,
    content_size: (i32, i32),
) {
    for op in ops.iter_mut() {
        op.shift_y(-dy);
    }
    ops.push(WindowOp::ScrollFill(ClearedSpan {
        x: 0,
        y: content_size.1.saturating_sub(dy),
        width: content_size.0,
        height: dy,
        colors,
    }));
}

impl WindowOp {
    /// Shifts this op vertically by a window scroll.
    pub(crate) fn shift_y(&mut self, dy: i32) {
        match self {
            Self::Glyph(glyph) => glyph.y = glyph.y.saturating_add(dy),
            Self::ClearSpan(span) | Self::ScrollFill(span) => span.y = span.y.saturating_add(dy),
            Self::KeypadIcon(icon) => icon.y = icon.y.saturating_add(dy),
        }
    }

    /// This op without its rows at or below a window's bottom edge, or `None`
    /// when nothing of it lies inside a window `content_height` pixels tall.
    /// Rows above the top edge need no clip: a scroll only moves content up,
    /// so they never return.
    ///
    /// Upstream draws into the window bitmap, which clips at its bottom edge
    /// (`CopyGlyphToWindow`, `src/text.c`); `ScrollWindow` later moves only
    /// pixels already in that bitmap, so a row clipped at reveal never comes
    /// back into view. A window that retains ops and replays them after every
    /// scroll shift reproduces that by clipping once, at reveal time, through
    /// this method or [`push_clipped`].
    pub(crate) fn clipped_below(self, content_height: i32) -> Option<Self> {
        match self {
            Self::Glyph(mut revealed) => {
                let visible_rows = content_height.saturating_sub(revealed.y);
                if visible_rows <= 0 {
                    return None;
                }
                let visible_rows = usize::try_from(visible_rows).unwrap_or(usize::MAX);
                if let Some(hidden) = revealed
                    .glyph
                    .pixels
                    .get_mut(visible_rows.saturating_mul(GLYPH_SIZE)..)
                {
                    hidden.fill(0);
                }
                Some(Self::Glyph(revealed))
            }
            Self::ClearSpan(mut span) => {
                span.height = span.height.min(content_height.saturating_sub(span.y));
                (span.height > 0).then_some(Self::ClearSpan(span))
            }
            Self::ScrollFill(mut span) => {
                span.height = span.height.min(content_height.saturating_sub(span.y));
                (span.height > 0).then_some(Self::ScrollFill(span))
            }
            Self::KeypadIcon(mut icon) => {
                let visible_rows = content_height.saturating_sub(icon.y);
                if visible_rows <= 0 {
                    return None;
                }
                icon.height = icon
                    .height
                    .min(u8::try_from(visible_rows).unwrap_or(u8::MAX));
                Some(Self::KeypadIcon(icon))
            }
        }
    }
}

/// Retains `op` in `ops` with the rows below a `content_height`-pixel window
/// removed; an op wholly below it is dropped. See [`WindowOp::clipped_below`].
pub(crate) fn push_clipped(ops: &mut Vec<WindowOp>, op: WindowOp, content_height: i32) {
    if let Some(op) = op.clipped_below(content_height) {
        ops.push(op);
    }
}

/// Replays ordered glyph, keypad-icon, and clear-span ops onto `fb`, which
/// must already hold the window's frame tiles and nothing else.
///
/// Each glyph maps the [`TextColors`] it carries through `palette`, the
/// window's loaded palette, so a `COLOR` or `SHADOW` control recolors only the
/// glyphs revealed after it. `window_fill` is the colors the latest
/// whole-window clear erased with (`Printer::cleared_window_colors`); `scene`
/// is the framebuffer as it was before the frame tiles were drawn, which a
/// transparent (index 0) fill or scroll fill shows ([`needs_scene`]).
///
/// A `ClearSpan` erases its window-local rectangle with its own background
/// (see [`erase_with_background`]) before any later glyph lands, covering
/// whatever an earlier op painted there, partial glyph included
/// (`pokeemerald/src/text.c:649-674`, `:1063-1090`).
#[expect(
    clippy::too_many_arguments,
    reason = "one window replay needs its geometry, icon sheet, palette, and both fill sources"
)]
pub(crate) fn compose_window_ops(
    fb: &mut Framebuffer,
    ops: &[WindowOp],
    origin: (i32, i32),
    content_size: (i32, i32),
    icons: &KeypadIconSheet,
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
            WindowOp::KeypadIcon(icon) => {
                blit_keypad_icon(fb, *icon, icons, palette, origin, content_size);
            }
            WindowOp::ClearSpan(span) => {
                erase_with_background(fb, &background, None, palette, *span, origin, content_size);
            }
            WindowOp::ScrollFill(span) => {
                erase_with_background(fb, &background, scene, palette, *span, origin, content_size);
            }
        }
    }
}

/// Whether composing `ops` and `window_fill` needs the pre-frame scene: a
/// whole-window fill or a `ScrollFill` with background 0, which
/// [`compose_window_ops`] restores from it.
pub(crate) fn needs_scene(ops: &[WindowOp], window_fill: Option<TextColors>) -> bool {
    window_fill.is_some_and(|fill| fill.background == 0)
        || ops.iter().any(|op| match op {
            WindowOp::ScrollFill(span) => span.colors.background == 0,
            WindowOp::Glyph(_) | WindowOp::ClearSpan(_) | WindowOp::KeypadIcon(_) => false,
        })
}

/// Erases `span` with `span.colors.background`.
///
/// Index 0 is transparent. A scroll fill and a whole-window fill write pixel
/// value 0 (`ScrollWindow`, `pokeemerald/src/text.c:1196-1201`), so they
/// restore the `scene` beneath the window; with no `scene` there is nothing to
/// restore and nothing is painted. A `CLEAR`/`CLEAR_TO` span never gets a
/// `scene`: upstream's `ClearTextSpan` skips painting when the background is
/// transparent (`pokeemerald/src/text.c:649-674`). Index
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

/// A keypad-icon sheet that is transparent except at the given sheet
/// positions, each `(x, y, palette index)`.
#[cfg(test)]
pub(crate) fn test_keypad_icons(marks: &[(u32, u32, u8)]) -> KeypadIconSheet {
    use assets::keypad_icons::{SHEET_HEIGHT, SHEET_WIDTH};

    let mut pixels = vec![0u8; (SHEET_WIDTH * SHEET_HEIGHT) as usize];
    for &(x, y, index) in marks {
        pixels[(y * SHEET_WIDTH + x) as usize] = index;
    }
    KeypadIconSheet::new(ImageRef {
        width: SHEET_WIDTH,
        height: SHEET_HEIGHT,
        bit_depth: 4,
        pixels: &pixels,
    })
    .expect("this is the exact real keypad-icon sheet shape")
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

    pub(super) fn opaque_glyph_at(x: i32, y: i32) -> RevealedGlyph {
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

    const ICON_FIRST_COLOR: Rgb888 = Rgb888 { r: 1, g: 2, b: 3 };
    const ICON_SECOND_COLOR: Rgb888 = Rgb888 { r: 4, g: 5, b: 6 };
    const ICON_THIRD_COLOR: Rgb888 = Rgb888 { r: 7, g: 8, b: 9 };
    const ICON_CONTENT_SIZE: (i32, i32) = (200, 32);
    const ICON_ORIGIN: (i32, i32) = (20, 30);
    /// Icon `A` (id 0, tile 0, 8x12), drawn at the window origin.
    const ICON_A_AT_ORIGIN: KeypadIconPlacement = KeypadIconPlacement {
        x: 0,
        y: 0,
        id: 0,
        width: 8,
        height: 12,
    };

    /// A window palette whose slots 1 to 3 are distinct and whose glyph-role
    /// colors differ from all of them, so an icon recoloured through the
    /// glyph roles is caught.
    fn icon_palette() -> Vec<Rgb888> {
        let mut palette = vec![Rgb888::BLACK; 16];
        palette[1] = ICON_FIRST_COLOR;
        palette[2] = ICON_SECOND_COLOR;
        palette[3] = ICON_THIRD_COLOR;
        palette
    }

    fn compose_icon_ops(ops: &[WindowOp], icons: &KeypadIconSheet) -> Framebuffer {
        let mut fb = Framebuffer::new();
        compose_window_ops(
            &mut fb,
            ops,
            ICON_ORIGIN,
            ICON_CONTENT_SIZE,
            icons,
            &icon_palette(),
            None,
            None,
        );
        fb
    }

    #[test]
    fn a_keypad_icon_copies_window_palette_indices_and_skips_zero() {
        let icons = test_keypad_icons(&[(0, 0, 1), (1, 0, 2), (2, 0, 3), (7, 11, 2)]);

        let fb = compose_icon_ops(&[WindowOp::KeypadIcon(ICON_A_AT_ORIGIN)], &icons);

        // Index 1 is the window palette's slot 1, not the glyph foreground role.
        assert_eq!(pixel_at(&fb, (20, 30)), Some(ICON_FIRST_COLOR));
        assert_eq!(pixel_at(&fb, (21, 30)), Some(ICON_SECOND_COLOR));
        assert_eq!(pixel_at(&fb, (22, 30)), Some(ICON_THIRD_COLOR));
        assert_eq!(pixel_at(&fb, (27, 41)), Some(ICON_SECOND_COLOR));
        assert_eq!(pixel_at(&fb, (23, 30)), Some(Rgb888::BLACK), "index 0");
    }

    #[test]
    fn a_keypad_icon_is_drawn_at_its_reported_position_and_confined_to_its_size() {
        // Marks one pixel right of and one below icon A's 8x12 rectangle.
        let icons = test_keypad_icons(&[(0, 0, 2), (8, 0, 3), (0, 12, 3)]);
        let placement = KeypadIconPlacement {
            x: 10,
            y: 4,
            ..ICON_A_AT_ORIGIN
        };

        let fb = compose_icon_ops(&[WindowOp::KeypadIcon(placement)], &icons);

        assert_eq!(pixel_at(&fb, (30, 34)), Some(ICON_SECOND_COLOR));
        assert_eq!(pixel_at(&fb, (38, 34)), Some(Rgb888::BLACK), "past width");
        assert_eq!(pixel_at(&fb, (30, 46)), Some(Rgb888::BLACK), "past height");
        assert_eq!(pixel_at(&fb, (20, 30)), Some(Rgb888::BLACK), "origin");
    }

    #[test]
    fn a_keypad_icon_on_the_third_tile_row_reads_the_sheet_at_its_own_origin() {
        // Icon 10 (UPDOWN) is tile 0x20: sheet position (0, 16).
        let icons = test_keypad_icons(&[(0, 16, 3), (0, 0, 2)]);
        let placement = KeypadIconPlacement {
            id: 10,
            ..ICON_A_AT_ORIGIN
        };

        let fb = compose_icon_ops(&[WindowOp::KeypadIcon(placement)], &icons);

        assert_eq!(pixel_at(&fb, (20, 30)), Some(ICON_THIRD_COLOR));
    }

    #[test]
    fn a_keypad_icon_is_clipped_to_the_content_rect() {
        let rows: Vec<(u32, u32, u8)> = (0..12).map(|y| (0, y, 2)).collect();
        let icons = test_keypad_icons(&rows);
        let above = KeypadIconPlacement {
            y: -6,
            ..ICON_A_AT_ORIGIN
        };

        let fb = compose_icon_ops(&[WindowOp::KeypadIcon(above)], &icons);

        assert_eq!(
            pixel_at(&fb, (20, 29)),
            Some(Rgb888::BLACK),
            "above the window"
        );
        assert_eq!(pixel_at(&fb, (20, 30)), Some(ICON_SECOND_COLOR));
        assert_eq!(pixel_at(&fb, (20, 35)), Some(ICON_SECOND_COLOR));
        assert_eq!(
            pixel_at(&fb, (20, 36)),
            Some(Rgb888::BLACK),
            "past the icon"
        );

        let past_right = KeypadIconPlacement {
            x: ICON_CONTENT_SIZE.0 - 1,
            ..ICON_A_AT_ORIGIN
        };
        let fb = compose_icon_ops(&[WindowOp::KeypadIcon(past_right)], &icons);
        assert_eq!(pixel_at(&fb, (219, 30)), Some(ICON_SECOND_COLOR));
        assert_eq!(pixel_at(&fb, (220, 30)), Some(Rgb888::BLACK), "outside");
    }

    #[test]
    fn a_later_clear_span_erases_an_earlier_keypad_icon() {
        let icons = test_keypad_icons(&[(0, 0, 2)]);
        let ops = [
            WindowOp::KeypadIcon(ICON_A_AT_ORIGIN),
            WindowOp::ClearSpan(ClearedSpan {
                x: 0,
                y: 0,
                width: 8,
                height: 12,
                colors: TextColors::default(),
            }),
        ];

        let fb = compose_icon_ops(&ops, &icons);

        assert_eq!(pixel_at(&fb, (20, 30)), Some(Rgb888::BLACK));
    }

    #[test]
    fn a_window_scroll_moves_a_keypad_icon() {
        let mut op = WindowOp::KeypadIcon(ICON_A_AT_ORIGIN);

        op.shift_y(-4);

        assert_eq!(
            op,
            WindowOp::KeypadIcon(KeypadIconPlacement {
                y: -4,
                ..ICON_A_AT_ORIGIN
            })
        );
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

    const WINDOW_HEIGHT_PX: i32 = 32;

    fn icon_at(y: i32) -> WindowOp {
        WindowOp::KeypadIcon(KeypadIconPlacement {
            x: 0,
            y,
            id: 0,
            width: 8,
            height: 12,
        })
    }

    #[test]
    fn a_keypad_icon_keeps_only_the_rows_inside_the_window() {
        assert_eq!(
            icon_at(20).clipped_below(WINDOW_HEIGHT_PX),
            Some(icon_at(20))
        );
        let Some(WindowOp::KeypadIcon(partial)) = icon_at(28).clipped_below(WINDOW_HEIGHT_PX)
        else {
            panic!("an icon straddling the edge keeps its top rows");
        };
        assert_eq!(partial.height, 4);
        assert_eq!(
            icon_at(WINDOW_HEIGHT_PX).clipped_below(WINDOW_HEIGHT_PX),
            None
        );
    }

    #[test]
    fn a_cleared_span_is_cut_at_the_window_bottom() {
        let span = |y| {
            WindowOp::ClearSpan(ClearedSpan {
                x: 0,
                y,
                width: 8,
                height: 16,
                colors: TextColors::default(),
            })
        };
        let Some(WindowOp::ClearSpan(partial)) = span(24).clipped_below(WINDOW_HEIGHT_PX) else {
            panic!("a span straddling the edge keeps its top rows");
        };
        assert_eq!(partial.height, 8);
        assert_eq!(span(WINDOW_HEIGHT_PX).clipped_below(WINDOW_HEIGHT_PX), None);
    }

    #[test]
    fn a_glyph_loses_the_rows_below_the_window_and_is_dropped_when_wholly_outside() {
        let painted_rows = |y| {
            let op = WindowOp::Glyph(Box::new(opaque_glyph_at(0, y)));
            let Some(WindowOp::Glyph(glyph)) = op.clipped_below(WINDOW_HEIGHT_PX) else {
                return None;
            };
            Some(
                glyph
                    .glyph
                    .pixels
                    .chunks(GLYPH_SIZE)
                    .filter(|row| row.iter().any(|&pixel| pixel != 0))
                    .count(),
            )
        };
        assert_eq!(painted_rows(0), Some(GLYPH_SIZE));
        assert_eq!(painted_rows(WINDOW_HEIGHT_PX - 5), Some(5));
        assert_eq!(painted_rows(WINDOW_HEIGHT_PX), None);
    }
}

#[cfg(test)]
#[path = "textbox/color_tests.rs"]
mod color_tests;
