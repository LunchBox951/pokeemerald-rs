//! Shared pixel-blit helpers for text-window chrome and glyph text (issue
//! #149): drawing a [`window::FrameTile`] layout and a
//! [`render::RevealedGlyph`] stream directly into a [`Framebuffer`] via
//! [`Framebuffer::set_pixel`], rather than through the BG/OAM tile
//! compositor [`crate::title`]/[`crate::overworld`] use.
//!
//! # Why pixel blits instead of the BG/sprite compositor
//!
//! [`crate::title::TitleScene`] and [`crate::overworld::OverworldScene`]
//! both build real [`rendering::BgLayer`]/[`rendering::SpriteLayer`] values
//! and hand them to [`rendering::compose_frame_with_effects`], matching
//! upstream's own hardware BG/OBJ layering exactly. Text windows are
//! different: a font glyph is a 16x16 cell that does **not** sit on an 8x8
//! tile boundary the way every other asset this workspace renders does (see
//! `assets::fonts`' module docs), and [`engine::text::render::Printer`]
//! reveals glyphs one at a time, at arbitrary sub-tile pixel offsets, as a
//! pure frame-driven stream -- there is no upstream-shaped "tilemap" to hand
//! the BG compositor for that. Blitting straight into the already-composed
//! [`Framebuffer`] is the direct, honest equivalent of what upstream's own
//! `CopyGlyphToWindow`/`CopyWindowToVram` pipeline does (write glyph pixels
//! into a window buffer, then that buffer's bytes reach the screen) without
//! inventing a fake tile layer to route them through
//! `(behavioral-fidelity)`.
//!
//! # Font glyph colour mapping -- a documented fidelity delta
//!
//! [`assets::fonts`]' own docs are explicit that a glyph's four palette-index
//! bytes (`0..=3`) are "upstream's fixed 4-colour bg/fg/shadow/box font
//! palette -- not extracted anywhere in this crate... a renderer maps those
//! four indices to real on-screen colours itself." Upstream's real mapping
//! (`text.c`'s `sTextColors`/`gFontDefaultFontColor`-driven behaviour, one of
//! several `TEXT_COLOR_*` schemes per printer, selected by whichever
//! `AddTextPrinterParameterized` caller is drawing) is deep, per-window
//! state this crate does not model. [`GLYPH_COLORS`] instead fixes one
//! simple, readable scheme -- opaque dark foreground, mid-grey shadow,
//! transparent background/box -- used uniformly by every caller of
//! [`blit_glyphs`]. Never upstream-exact, always legible.
//!
//! Window/message-box **frame** tiles are different: their pixel indices
//! *are* meaningful positions into the real extracted
//! [`assets::pack::WindowFrameHandle::palette`], so [`blit_frame_tiles`]
//! uses those colours directly -- no invented palette there.

use assets::pack::ImageRef;
use engine::text::render::RevealedGlyph;
use engine::text::window::{self as msgwin, FrameTile};
use rendering::{Bgr555, Framebuffer, Rgb888};

/// Fixed glyph palette-index -> colour mapping (module docs): `None` means
/// "transparent, leave the framebuffer pixel beneath untouched" (upstream's
/// index 0, always the background/off pixels of a glyph cell); the other
/// three are readable, fixed opaque colours.
const GLYPH_COLORS: [Option<Rgb888>; 4] = [
    None,
    Some(Rgb888 {
        r: 24,
        g: 24,
        b: 24,
    }),
    Some(Rgb888 {
        r: 160,
        g: 160,
        b: 160,
    }),
    None,
];

/// Blit one [`window::FrameTile`](msgwin::FrameTile) list -- the chrome
/// around a menu or dialogue box ([`engine::text::window::border_tiles`] /
/// [`engine::text::window::MessageBoxLayout::frame_tiles`]) -- into `fb`,
/// reading each tile's pixels out of `sheet` (a
/// [`assets::pack::WindowFrameHandle::tiles`] image) and mapping them
/// through `palette`'s real extracted colours
/// ([`assets::pack::WindowFrameHandle::palette`]). Palette index 0 is
/// transparent (GBA convention, matching every other layer this workspace
/// composites), so it is skipped rather than painted.
// `engine::text::window::TILE_SIZE` (8): a plain `usize` literal avoids any
// narrowing/sign-loss cast ambiguity from the `u32` const.
const TILE: usize = 8;
const _: () = assert!(msgwin::TILE_SIZE == 8);

// `assets::fonts::GLYPH_SIZE` (16): see the `TILE` comment above.
const GLYPH_DIM: usize = 16;
const _: () = assert!(assets::fonts::GLYPH_SIZE == 16);

pub(crate) fn blit_frame_tiles(
    fb: &mut Framebuffer,
    tiles: &[FrameTile],
    sheet: ImageRef<'_>,
    palette: &[Rgb888],
) {
    for (tile_index, tile) in tiles.iter().enumerate() {
        // Resolve final tile placement before blitting: some layouts (e.g.
        // `MessageBoxLayout::frame_tiles`'s own documented "last write
        // wins" bottom-border-over-fill overlap) deliberately list more
        // than one `FrameTile` for the same `(col, row)` cell, mirroring
        // upstream's own sequential `FillBgTilemapBufferRect` calls
        // reassigning one tilemap cell to a different source tile. A GBA
        // tilemap cell only ever shows its *most recently assigned* tile in
        // full -- never a per-pixel blend of two -- so an earlier tile's
        // opaque pixels must not keep showing through a later tile's own
        // transparent (index 0) ones at a shared cell. Skipping every
        // non-final write for a cell here (rather than blitting each tile
        // in list order and merely skipping transparent pixels, which would
        // leave exactly that stale-pixel artifact) reproduces the
        // one-tile-per-cell rule directly.
        let is_final_write_for_cell = !tiles[tile_index + 1..]
            .iter()
            .any(|later| later.col == tile.col && later.row == tile.row);
        if !is_final_write_for_cell {
            continue;
        }

        let Some(pixels) = msgwin::tile_pixels_flipped(sheet, tile.tile, tile.v_flip) else {
            continue;
        };
        let origin_x = tile.col * i32::try_from(TILE).unwrap_or(0);
        let origin_y = tile.row * i32::try_from(TILE).unwrap_or(0);
        for local_y in 0..TILE {
            for local_x in 0..TILE {
                let pixel_index = usize::from(pixels[local_y * TILE + local_x]);
                if pixel_index == 0 {
                    continue;
                }
                let Some(&color) = palette.get(pixel_index) else {
                    continue;
                };
                let (Ok(dx), Ok(dy)) = (i32::try_from(local_x), i32::try_from(local_y)) else {
                    continue;
                };
                set_pixel_checked(fb, origin_x + dx, origin_y + dy, color);
            }
        }
    }
}

/// Blit every already-revealed glyph in `glyphs` into `fb`, offset by
/// `origin` (the text window's own top-left pixel position on screen --
/// [`engine::text::render::Printer`]'s coordinates are window-local, see its
/// module docs) and clipped to `content_size` (`(width, height)` in pixels,
/// window-local like `g.x`/`g.y` themselves -- the window's own content
/// rect, e.g. [`engine::text::window::MessageBoxLayout::content_width`]/
/// `content_height` converted to pixels).
///
/// This clip matters most during a `\l` prompt scroll
/// ([`crate::intro::IntroScene::tick`]'s `TickEvent::Scrolling` arm, which
/// shifts every already-revealed glyph's `g.y` upward): without it, a
/// glyph's now-negative window-local `y` still lands on a valid
/// *framebuffer* pixel (just above the window, e.g. over its own top
/// border), so the outgoing line stays visible above the box instead of
/// scrolling out of it. Mirrors upstream `CopyGlyphToWindow`
/// (`pokeemerald/src/text.c`), which clamps `glyphWidth`/`glyphHeight`
/// against `template->width * 8 - currentX` / `template->height * 8 -
/// currentY` before ever copying a pixel -- a glyph can never draw outside
/// its own window's pixel buffer there either, since `ScrollWindow`
/// (`pokeemerald/src/window.c`) only ever shifts bytes *within* that
/// same fixed-size buffer `(behavioral-fidelity)`. Uses the fixed
/// [`GLYPH_COLORS`] mapping (module docs).
pub(crate) fn blit_glyphs(
    fb: &mut Framebuffer,
    glyphs: &[RevealedGlyph],
    origin: (i32, i32),
    content_size: (i32, i32),
) {
    for g in glyphs {
        for local_y in 0..GLYPH_DIM {
            for local_x in 0..GLYPH_DIM {
                let index = usize::from(g.glyph.pixels[local_y * GLYPH_DIM + local_x]);
                let Some(Some(color)) = GLYPH_COLORS.get(index) else {
                    continue;
                };
                let (Ok(dx), Ok(dy)) = (i32::try_from(local_x), i32::try_from(local_y)) else {
                    continue;
                };
                let px = g.x + dx;
                let py = g.y + dy;
                if px < 0 || py < 0 || px >= content_size.0 || py >= content_size.1 {
                    continue;
                }
                set_pixel_checked(fb, origin.0 + px, origin.1 + py, *color);
            }
        }
    }
}

/// A window/message-box frame's tile bitmap and palette, owned rather than
/// borrowed from a live [`assets::pack::AssetPack`] -- the same "own every
/// byte, rebuild views fresh" shape [`crate::title::TitleScene`]/
/// [`crate::overworld::OverworldScene`] already use for their own pack
/// entries, shared here by [`crate::main_menu::MainMenuScene`] and
/// [`crate::intro::IntroScene`] so both draw their chrome identically.
///
/// Deliberately holds the palette as already-converted [`Rgb888`] (via
/// [`palette_colors`]) rather than a borrowed
/// [`assets::pack::PaletteRef`]: `PaletteRef`'s raw bytes are only
/// constructible inside the `assets` crate itself (see
/// [`assets::pack::AssetPack::palette`]'s callers), so a converted, owned
/// `Vec<Rgb888>` is also what lets unit tests build a synthetic
/// [`FrameAssets`] by hand, without a real pack.
#[derive(Debug, Clone)]
pub(crate) struct FrameAssets {
    pub(crate) pixels: Vec<u8>,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) palette: Vec<Rgb888>,
}

impl FrameAssets {
    /// Copy a pack-borrowed [`assets::pack::WindowFrameHandle`]'s bytes out
    /// into an owned [`FrameAssets`].
    pub(crate) fn from_handle(handle: assets::pack::WindowFrameHandle<'_>) -> Self {
        Self {
            pixels: handle.tiles.pixels.to_vec(),
            width: handle.tiles.width,
            height: handle.tiles.height,
            palette: palette_colors(handle.palette),
        }
    }

    /// A fresh [`ImageRef`] view over the owned tile bitmap, for
    /// [`blit_frame_tiles`].
    pub(crate) fn image(&self) -> ImageRef<'_> {
        ImageRef {
            width: self.width,
            height: self.height,
            bit_depth: 4,
            pixels: &self.pixels,
        }
    }
}

/// Convert a [`assets::pack::PaletteRef`]'s raw BGR555 colours into
/// [`Rgb888`], for use with [`blit_frame_tiles`]. Unassigned trailing slots
/// (beyond `color_count`) decode as [`Bgr555::default`] (raw `0`, i.e.
/// black) -- never read by a real [`assets::pack::WindowFrameHandle`], whose
/// extraction already guarantees a full 16-colour bank (see
/// `assets::pack`'s `MalformedTextWindowPalette` guard).
pub(crate) fn palette_colors(raw: assets::pack::PaletteRef<'_>) -> Vec<Rgb888> {
    raw.colors()
        .map(|c| Bgr555::from_raw(c).to_rgb888())
        .collect()
}

/// Fill a `width`x`height` pixel rectangle at `origin` solid `color` --
/// upstream's `FillWindowPixelBuffer(windowId, PIXEL_FILL(n))`
/// (`pokeemerald/src/window.c`), the window-content fill every standard/
/// selectable window performs *before* drawing its border/text
/// (`pokeemerald/src/main_menu.c:2231`/`2269`,
/// `WindowFunc_DrawStandardFrame`-style callers in `menu.c`). Needed because
/// [`blit_frame_tiles`] only ever draws a window's border *ring*
/// ([`window::border_tiles`](msgwin::border_tiles)'s own docs: "the
/// content rect's own fill is a separate concern, left to the caller") --
/// without this, a window's interior stays whatever the framebuffer already
/// held beneath it, which for every caller in this crate is opaque black
/// (each `compose`'s own `fb.fill(Rgb888::BLACK)`).
pub(crate) fn fill_rect(
    fb: &mut Framebuffer,
    origin: (i32, i32),
    width: i32,
    height: i32,
    color: Rgb888,
) {
    for dy in 0..height {
        for dx in 0..width {
            set_pixel_checked(fb, origin.0 + dx, origin.1 + dy, color);
        }
    }
}

/// [`Framebuffer::set_pixel`], skipping silently instead of panicking when
/// `(x, y)` falls outside the visible 240x160 screen -- a scrolled dialogue
/// box or an off-screen glyph column is expected, not a bug (mirrors
/// [`Framebuffer::pixel`]'s own `None`-on-out-of-range contract).
fn set_pixel_checked(fb: &mut Framebuffer, x: i32, y: i32, color: Rgb888) {
    let Ok(x) = usize::try_from(x) else { return };
    let Ok(y) = usize::try_from(y) else { return };
    if x < Framebuffer::WIDTH && y < Framebuffer::HEIGHT {
        fb.set_pixel(x, y, color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assets::{Glyph, GLYPH_PIXELS};
    use engine::text::render::RevealedGlyph;

    /// A fully opaque 16x16 [`RevealedGlyph`] (every pixel palette index 1,
    /// [`GLYPH_COLORS`]' opaque foreground) at window-local `(x, y)` --
    /// cheap to hand-verify pixel-by-pixel.
    fn opaque_glyph_at(x: i32, y: i32) -> RevealedGlyph {
        RevealedGlyph {
            x,
            y,
            glyph: Glyph {
                advance_width: 8,
                pixels: [1u8; GLYPH_PIXELS],
            },
        }
    }

    // Finding 2 (Codex re-review): a glyph scrolled above a window's own
    // content rect (`\l` prompt scroll, `IntroScene::tick`'s
    // `TickEvent::Scrolling` arm shifting every revealed glyph's `y`
    // upward) must not paint anywhere outside that rect, even though the
    // resulting screen pixel would still fall inside the full 240x160
    // framebuffer.

    #[test]
    fn blit_glyphs_draws_nothing_for_a_glyph_scrolled_entirely_above_the_content_rect() {
        let mut fb = Framebuffer::new();
        // Every window-local row (`-16..0`) is above the content rect's own
        // `y == 0` top edge.
        let glyph = opaque_glyph_at(4, -16);

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
        // Window-local y -8..8: rows 0..8 (window-local -8..0) are above the
        // content rect and must not draw; rows 8..16 (window-local 0..8)
        // are inside and must.
        let glyph = opaque_glyph_at(0, -8);

        blit_glyphs(&mut fb, &[glyph], origin, (200, 32));

        // Screen row `origin.1 - 1` is window-local y == -1 -- just above
        // the content rect (e.g. the window's own top border row in a real
        // composition) -- and must stay untouched.
        assert_eq!(fb.pixel(20, 29), Some(Rgb888::BLACK));
        // Screen row `origin.1` is window-local y == 0 -- the content
        // rect's own first row -- and must be painted.
        assert_ne!(fb.pixel(20, 30), Some(Rgb888::BLACK));
    }

    #[test]
    fn blit_glyphs_clips_a_glyph_past_the_content_rects_right_or_bottom_edge() {
        let mut fb = Framebuffer::new();
        let origin = (0, 0);
        let content_size = (10, 10); // smaller than one 16x16 glyph cell.
        let glyph = opaque_glyph_at(0, 0);

        blit_glyphs(&mut fb, &[glyph], origin, content_size);

        // Inside both axes: painted.
        assert_ne!(fb.pixel(5, 5), Some(Rgb888::BLACK));
        // Past the right edge (content_size.0 == 10) and past the bottom
        // edge (content_size.1 == 10) respectively: not painted, even
        // though both are well inside the 240x160 framebuffer.
        assert_eq!(fb.pixel(12, 5), Some(Rgb888::BLACK));
        assert_eq!(fb.pixel(5, 12), Some(Rgb888::BLACK));
    }

    #[test]
    fn blit_glyphs_draws_a_fully_in_bounds_glyph_unclipped() {
        let mut fb = Framebuffer::new();
        let origin = (10, 10);
        let glyph = opaque_glyph_at(0, 0);

        blit_glyphs(&mut fb, &[glyph], origin, (200, 100));

        for y in 10..26 {
            for x in 10..26 {
                assert_ne!(
                    fb.pixel(x, y),
                    Some(Rgb888::BLACK),
                    "({x}, {y}) is within the fully-visible glyph and content rect"
                );
            }
        }
    }

    // Finding 3 (Codex re-review): when two `FrameTile`s target the same
    // `(col, row)` cell (`MessageBoxLayout::frame_tiles`'s own documented
    // fill-then-bottom-border overlap), the earlier tile's opaque pixels
    // must not keep showing through the later (final) tile's own
    // transparent (index 0) pixels.

    /// A synthetic 2-tile sheet (16x8px: tile 0 at columns 0..8, tile 1 at
    /// columns 8..16): tile 0 is fully opaque (palette index 3, standing in
    /// for a window's "interior fill" tile); tile 1 is opaque only on its
    /// own top row (palette index 2, standing in for a "border" tile with a
    /// transparent interior) and transparent (index 0) everywhere else.
    fn fill_then_border_sheet() -> Vec<u8> {
        let mut pixels = vec![0u8; 16 * 8];
        for y in 0..8usize {
            for x in 0..16usize {
                pixels[y * 16 + x] = if x < 8 {
                    3
                } else if y == 0 {
                    2
                } else {
                    0
                };
            }
        }
        pixels
    }

    #[test]
    fn blit_frame_tiles_final_writes_transparent_pixels_show_the_true_backdrop_not_a_superseded_tile(
    ) {
        let pixels = fill_then_border_sheet();
        let sheet = ImageRef {
            width: 16,
            height: 8,
            bit_depth: 4,
            pixels: &pixels,
        };
        let mut palette = vec![Rgb888::BLACK; 4];
        palette[2] = Rgb888 { r: 200, g: 0, b: 0 };
        palette[3] = Rgb888 { r: 0, g: 200, b: 0 };

        // Same cell, tile 0 ("fill") listed first, tile 1 ("border") last --
        // mirrors `MessageBoxLayout::frame_tiles`'s own documented draw
        // order, where the later entry is meant to fully replace the cell.
        let tiles = [
            FrameTile {
                col: 0,
                row: 0,
                tile: 0,
                v_flip: false,
            },
            FrameTile {
                col: 0,
                row: 0,
                tile: 1,
                v_flip: false,
            },
        ];

        let mut fb = Framebuffer::new(); // all-black backdrop by default.
        blit_frame_tiles(&mut fb, &tiles, sheet, &palette);

        // The final tile's own opaque top row draws its own colour.
        assert_eq!(fb.pixel(0, 0), Some(Rgb888 { r: 200, g: 0, b: 0 }));
        // Its transparent rows must show the true backdrop (black) -- not
        // the earlier, superseded "fill" tile's now-stale opaque colour.
        for y in 1..8 {
            assert_eq!(
                fb.pixel(0, y),
                Some(Rgb888::BLACK),
                "row {y} must not still show the superseded fill tile's pixel"
            );
        }
    }

    #[test]
    fn blit_frame_tiles_draws_every_tile_when_no_two_share_a_cell() {
        // Non-overlapping placement (`window::border_tiles`'s own shape,
        // e.g.) must still draw every tile in full -- the "last write wins"
        // resolution must never suppress a cell nothing else ever touches.
        let pixels = fill_then_border_sheet();
        let sheet = ImageRef {
            width: 16,
            height: 8,
            bit_depth: 4,
            pixels: &pixels,
        };
        let mut palette = vec![Rgb888::BLACK; 4];
        palette[3] = Rgb888 { r: 0, g: 200, b: 0 };

        let tiles = [
            FrameTile {
                col: 0,
                row: 0,
                tile: 0,
                v_flip: false,
            },
            FrameTile {
                col: 5,
                row: 5,
                tile: 0,
                v_flip: false,
            },
        ];

        let mut fb = Framebuffer::new();
        blit_frame_tiles(&mut fb, &tiles, sheet, &palette);

        assert_eq!(fb.pixel(0, 0), Some(Rgb888 { r: 0, g: 200, b: 0 }));
        assert_eq!(fb.pixel(40, 40), Some(Rgb888 { r: 0, g: 200, b: 0 }));
    }
}
