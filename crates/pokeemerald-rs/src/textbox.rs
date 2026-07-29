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
    for tile in tiles {
        let Some(pixels) = msgwin::tile_pixels_flipped(sheet, tile.tile, tile.v_flip) else {
            continue;
        };
        let origin_x = tile.col * i32::try_from(TILE).unwrap_or(0);
        let origin_y = tile.row * i32::try_from(TILE).unwrap_or(0);
        for local_y in 0..TILE {
            for local_x in 0..TILE {
                let index = usize::from(pixels[local_y * TILE + local_x]);
                if index == 0 {
                    continue;
                }
                let Some(&color) = palette.get(index) else {
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
/// module docs). Uses the fixed [`GLYPH_COLORS`] mapping (module docs).
pub(crate) fn blit_glyphs(fb: &mut Framebuffer, glyphs: &[RevealedGlyph], origin: (i32, i32)) {
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
                set_pixel_checked(fb, origin.0 + g.x + dx, origin.1 + g.y + dy, *color);
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
