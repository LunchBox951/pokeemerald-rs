//! A single regular (non-affine) BG tile layer compositor, with wrapping
//! scroll offsets (S-2 slices 1 and 2).
//!
//! [`BgLayer`] composites a [`Tilemap`](crate::tilemap::Tilemap) — see that
//! module for the tilemap/screen-entry data model this draws from.
//!
//! [`BgLayer::composite_scrolled`] ports `ChangeBgX`/`ChangeBgY`'s 9-bit
//! wrapping regular-BG scroll offsets (`pokeemerald/src/bg.c`): the visible
//! 240x160 window wraps within the tilemap's own pixel size (256 or 512 px
//! per axis for a hardware-sized regular BG), so scrolling off one edge
//! reveals the opposite edge `(behavioral-fidelity)`.
//!
//! Affine transforms, multi-layer priority ordering (see
//! [`compositor`](crate::compositor)), and windows/blending are out of
//! scope — see issue #64.

use crate::framebuffer::Framebuffer;
use crate::palette::{Palette, Rgb888};
use crate::tile::{BitDepth, Tileset};
use crate::tilemap::Tilemap;

/// A single regular (non-affine) BG tile layer: a [`Tileset`], a
/// [`Palette`], and a [`Tilemap`], ready to composite into a
/// [`Framebuffer`].
///
/// No priority-vs-other-layers ordering (see
/// [`compositor`](crate::compositor)), no affine transform, and no
/// windows/blending — see issue #64's scope.
#[derive(Debug, Clone, Copy)]
pub struct BgLayer<'a> {
    tileset: &'a Tileset,
    palette: &'a Palette,
    tilemap: &'a Tilemap,
}

impl<'a> BgLayer<'a> {
    /// A regular BG's hardware scroll registers (`BGxHOFS`/`BGxVOFS`) are
    /// each 9 bits wide, giving a 0..=511 wrapping coordinate space
    /// (`pokeemerald/src/bg.c`'s `ChangeBgX`/`ChangeBgY`).
    const SCROLL_MASK: u16 = 0x01FF;

    /// Borrow a tileset, palette, and tilemap together as one composable
    /// layer.
    #[must_use]
    pub const fn new(tileset: &'a Tileset, palette: &'a Palette, tilemap: &'a Tilemap) -> Self {
        Self {
            tileset,
            palette,
            tilemap,
        }
    }

    /// Composite this layer into `framebuffer`, starting at tilemap origin
    /// `(0, 0)`, with no scrolling and no wraparound.
    ///
    /// A tilemap larger than the framebuffer is clipped to the visible
    /// 240x160 area; a tilemap smaller than the framebuffer leaves the
    /// uncovered area untouched. This is the slice-1 (#50) behaviour, kept
    /// unchanged for callers that don't need scrolling — see
    /// [`composite_scrolled`](Self::composite_scrolled) for the wrapping
    /// regular-BG behaviour added in slice 2 (#64). A screen entry whose
    /// tile index has no matching tile in the tileset is skipped (left
    /// untouched) rather than treated as an error.
    pub fn composite(&self, framebuffer: &mut Framebuffer) {
        const DIM: usize = BitDepth::TILE_DIM;
        for row in 0..self.tilemap.height_tiles() {
            let origin_y = row * DIM;
            if origin_y >= framebuffer.height() {
                break;
            }
            for col in 0..self.tilemap.width_tiles() {
                let origin_x = col * DIM;
                if origin_x >= framebuffer.width() {
                    break;
                }
                for local_y in 0..DIM {
                    for local_x in 0..DIM {
                        let Some(color) = self.sample_pixel(col, row, local_x, local_y) else {
                            continue;
                        };
                        framebuffer.set_pixel(origin_x + local_x, origin_y + local_y, color);
                    }
                }
            }
        }
    }

    /// Composite this layer into `framebuffer`, offset by a regular-BG
    /// scroll position that wraps within this layer's tilemap.
    ///
    /// `scroll_x`/`scroll_y` are masked to 9 bits, matching the GBA's
    /// `BGxHOFS`/`BGxVOFS` scroll registers. The visible window then wraps
    /// within the tilemap's own pixel size (`width_tiles * 8` by
    /// `height_tiles * 8` — 256 or 512 px per axis for a hardware-sized
    /// regular BG), so scrolling off one edge reveals the opposite edge,
    /// exactly like the GBA's regular (non-affine) BG scroll behaviour. A
    /// screen entry whose tile index has no matching tile in the tileset is
    /// skipped (left untouched) rather than treated as an error.
    pub fn composite_scrolled(&self, framebuffer: &mut Framebuffer, scroll_x: u16, scroll_y: u16) {
        for fb_y in 0..framebuffer.height() {
            for fb_x in 0..framebuffer.width() {
                let Some(color) = self.sample_scrolled(fb_x, fb_y, scroll_x, scroll_y) else {
                    continue;
                };
                framebuffer.set_pixel(fb_x, fb_y, color);
            }
        }
    }

    /// Sample this layer's resolved color at framebuffer coordinate
    /// `(x, y)`, offset by a wrapping regular-BG scroll position — the
    /// per-pixel primitive behind
    /// [`composite_scrolled`](Self::composite_scrolled), shared with the
    /// cross-layer [priority compositor](crate::compositor) so both apply
    /// the identical 9-bit-masked, tilemap-wrapped scroll math.
    ///
    /// Returns `None` if this layer has an empty tilemap or the sampled
    /// pixel is transparent (see [`sample_pixel`](Self::sample_pixel)).
    #[must_use]
    pub(crate) fn sample_scrolled(
        &self,
        x: usize,
        y: usize,
        scroll_x: u16,
        scroll_y: u16,
    ) -> Option<Rgb888> {
        const DIM: usize = BitDepth::TILE_DIM;
        let bg_width_px = self.tilemap.width_tiles() * DIM;
        let bg_height_px = self.tilemap.height_tiles() * DIM;
        if bg_width_px == 0 || bg_height_px == 0 {
            return None;
        }
        let scroll_x = usize::from(scroll_x & Self::SCROLL_MASK);
        let scroll_y = usize::from(scroll_y & Self::SCROLL_MASK);
        let src_x = (x + scroll_x) % bg_width_px;
        let src_y = (y + scroll_y) % bg_height_px;
        self.sample_pixel(src_x / DIM, src_y / DIM, src_x % DIM, src_y % DIM)
    }

    /// Sample this layer's resolved color at tilemap tile `(col, row)`,
    /// within-tile pixel `(tile_x, tile_y)` (pre-flip, `0..8` each), or
    /// `None` if there is no screen entry, no matching tile, or the pixel is
    /// transparent (palette index 0).
    ///
    /// Used by [`composite`](Self::composite) directly (no scrolling) and by
    /// [`sample_scrolled`](Self::sample_scrolled) (wrapping scroll math).
    ///
    /// Palette index 0 is transparent on a regular BG — in every 4bpp bank
    /// and for 8bpp — so index-0 pixels resolve to `None`, letting the
    /// backdrop or a lower layer show through, matching mgba's software
    /// mode-0 renderer.
    fn sample_pixel(&self, col: usize, row: usize, tile_x: usize, tile_y: usize) -> Option<Rgb888> {
        const DIM: usize = BitDepth::TILE_DIM;
        let entry = self.tilemap.entry(col, row)?;
        let tile = self.tileset.tile(entry.tile_index())?;
        let flipped_x = if entry.h_flip() {
            DIM - 1 - tile_x
        } else {
            tile_x
        };
        let flipped_y = if entry.v_flip() {
            DIM - 1 - tile_y
        } else {
            tile_y
        };
        let index = tile.index(flipped_x, flipped_y);
        if index == 0 {
            return None;
        }
        let color = match self.tileset.bit_depth() {
            BitDepth::Bpp4 => self.palette.bank_color(entry.palette_bank(), index),
            BitDepth::Bpp8 => self.palette.color(index),
        };
        Some(color.to_rgb888())
    }
}

#[cfg(test)]
mod tests {
    use super::BgLayer;
    use crate::framebuffer::Framebuffer;
    use crate::palette::{Bgr555, Palette, Rgb888};
    use crate::tile::{BitDepth, Tileset};
    use crate::tilemap::{ScreenEntry, Tilemap};

    /// A hand-built 4bpp fixture: one tile whose two-row pattern repeats
    /// every other row, so pixel(x, y) has palette index `((y % 2) * 8 +
    /// x) % 16` — asymmetric across both axes, so flip bits are observable.
    /// Row 0 (and every even row): indices 0..7. Row 1 (and every odd row):
    /// indices 8..15.
    fn checkerboard_4bpp_tile() -> [u8; 32] {
        let even_row = [0x10, 0x32, 0x54, 0x76]; // pixel(0,0)=0 .. pixel(7,0)=7
        let odd_row = [0x98, 0xBA, 0xDC, 0xFE]; // pixel(0,1)=8 .. pixel(7,1)=15
        let mut bytes = [0u8; 32];
        for row in 0..8 {
            let src = if row % 2 == 0 { &even_row } else { &odd_row };
            bytes[row * 4..row * 4 + 4].copy_from_slice(src);
        }
        bytes
    }

    #[test]
    fn composite_resolves_flip_bits_and_palette_banks() {
        let tile_bytes = checkerboard_4bpp_tile();
        let tileset = Tileset::decode(BitDepth::Bpp4, &tile_bytes).unwrap();

        let red = Bgr555::from_channels(0x1F, 0, 0);
        let green = Bgr555::from_channels(0, 0x1F, 0);
        let blue = Bgr555::from_channels(0, 0, 0x1F);
        let yellow = Bgr555::from_channels(0x1F, 0x1F, 0);

        let mut colors = [Bgr555::default(); Palette::LEN];
        // Bank 0: index 1 -> red, index 7 -> green, index 8 -> blue
        // (index 0 stays default/black).
        colors[1] = red;
        colors[7] = green;
        colors[8] = blue;
        // Bank 1 (flat offset 16): indices 1, 7, and 8 all -> yellow, so
        // any non-zero pixel drawn through bank 1 is unambiguously yellow.
        colors[16 + 1] = yellow;
        colors[16 + 7] = yellow;
        colors[16 + 8] = yellow;
        let palette = Palette::new(colors);

        // Entry 0: tile 0, no flip, bank 0.
        // Entry 1: tile 0, both flips, bank 1.
        let entries = vec![
            ScreenEntry::new(0, false, false, 0),
            ScreenEntry::new(0, true, true, 1),
        ];
        let tilemap = Tilemap::new(2, 1, entries).unwrap();

        let layer = BgLayer::new(&tileset, &palette, &tilemap);
        let mut fb = Framebuffer::new();
        // Sentinel fill distinguishes "left untouched" from "composited to
        // black" (palette index 0 is genuinely black in this fixture).
        let sentinel = Rgb888 { r: 9, g: 9, b: 9 };
        fb.fill(sentinel);

        layer.composite(&mut fb);

        // Entry 0 (unflipped, bank 0): pixel(x,y) = tile pixel(x,y) through
        // bank 0.
        // (test-ratchet) This used to assert `Some(Rgb888::BLACK)`, pinning
        // the pre-fix defect where index 0 was painted as colors[0]. Index 0
        // is transparent on a regular BG, so the sentinel backdrop must show
        // through untouched here.
        assert_eq!(fb.pixel(0, 0), Some(sentinel)); // index 0 -> transparent
        assert_eq!(fb.pixel(1, 0), Some(red.to_rgb888())); // index 1
        assert_eq!(fb.pixel(7, 0), Some(green.to_rgb888())); // index 7
        assert_eq!(fb.pixel(0, 1), Some(blue.to_rgb888())); // index 8

        // Entry 1 (both flips, bank 1) occupies framebuffer x in 8..16.
        // Output local (lx, ly) reads tile pixel (7-lx, 7-ly).
        // local (7,0) -> tile (0,7) -> odd-row pattern x=0 -> index 8 -> yellow.
        assert_eq!(fb.pixel(8 + 7, 0), Some(yellow.to_rgb888()));
        // local (0,7) -> tile (7,0) -> even-row pattern x=7 -> index 7 -> yellow.
        assert_eq!(fb.pixel(8, 7), Some(yellow.to_rgb888()));
        // local (0,0) -> tile (7,7) -> odd-row pattern x=7 -> index 15,
        // unassigned in either bank -> stays default black.
        assert_eq!(fb.pixel(8, 0), Some(Rgb888::BLACK));

        // Outside the 16x8 tilemap footprint the sentinel fill survives
        // untouched.
        assert_eq!(fb.pixel(20, 0), Some(sentinel));
        assert_eq!(fb.pixel(0, 8), Some(sentinel));
        assert_eq!(fb.pixel(239, 159), Some(sentinel));
    }

    #[test]
    fn composite_8bpp_uses_the_flat_palette_directly() {
        let mut tile_bytes = [0u8; 64];
        tile_bytes[0] = 200; // pixel(0,0)
        tile_bytes[1] = 5; // pixel(1,0)
        let tileset = Tileset::decode(BitDepth::Bpp8, &tile_bytes).unwrap();

        let mut colors = [Bgr555::default(); Palette::LEN];
        let orange = Bgr555::from_channels(0x1F, 0x10, 0);
        colors[200] = orange;
        let palette = Palette::new(colors);

        // Palette bank bits are set but must be ignored for 8bpp tiles.
        let entries = vec![ScreenEntry::new(0, false, false, 3)];
        let tilemap = Tilemap::new(1, 1, entries).unwrap();

        let layer = BgLayer::new(&tileset, &palette, &tilemap);
        let mut fb = Framebuffer::new();
        layer.composite(&mut fb);

        assert_eq!(fb.pixel(0, 0), Some(orange.to_rgb888()));
        assert_eq!(fb.pixel(1, 0), Some(Rgb888::BLACK)); // index 5, unassigned
    }

    #[test]
    fn composite_skips_screen_entries_with_no_matching_tile() {
        let tileset = Tileset::decode(BitDepth::Bpp4, &[0u8; 32]).unwrap(); // 1 tile
        let palette = Palette::new([Bgr555::default(); Palette::LEN]);
        let entries = vec![ScreenEntry::new(5, false, false, 0)]; // no tile 5
        let tilemap = Tilemap::new(1, 1, entries).unwrap();

        let layer = BgLayer::new(&tileset, &palette, &tilemap);
        let mut fb = Framebuffer::new();
        let sentinel = Rgb888 { r: 1, g: 2, b: 3 };
        fb.fill(sentinel);

        layer.composite(&mut fb);

        assert_eq!(fb.pixel(0, 0), Some(sentinel));
    }

    #[test]
    fn composite_clips_a_tilemap_larger_than_the_framebuffer() {
        // A 31x21-tile tilemap (248x168 px) is one tile wider/taller than
        // the 240x160 framebuffer in both axes; the extra row/column must
        // not panic and must be dropped.
        let tileset = Tileset::decode(BitDepth::Bpp4, &[0xFFu8; 32]).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[15] = Bgr555::from_channels(0x1F, 0x1F, 0x1F);
        let palette = Palette::new(colors);
        let entries = vec![ScreenEntry::new(0, false, false, 0); 31 * 21];
        let tilemap = Tilemap::new(31, 21, entries).unwrap();

        let layer = BgLayer::new(&tileset, &palette, &tilemap);
        let mut fb = Framebuffer::new();
        layer.composite(&mut fb); // must not panic

        assert_eq!(fb.pixel(239, 159), Some(colors[15].to_rgb888()));
    }

    #[test]
    fn composite_leaves_index_0_pixels_transparent_over_a_backdrop() {
        // A tile whose top-left pixel is palette index 0 and whose next pixel
        // is index 1; index 0 must let the backdrop show through.
        let mut tile_bytes = [0u8; 32];
        tile_bytes[0] = 0x10; // pixel(0,0)=0, pixel(1,0)=1
        let tileset = Tileset::decode(BitDepth::Bpp4, &tile_bytes).unwrap();

        let mut colors = [Bgr555::default(); Palette::LEN];
        let red = Bgr555::from_channels(0x1F, 0, 0);
        colors[1] = red;
        let palette = Palette::new(colors);

        let entries = vec![ScreenEntry::new(0, false, false, 0)];
        let tilemap = Tilemap::new(1, 1, entries).unwrap();

        let layer = BgLayer::new(&tileset, &palette, &tilemap);
        let mut fb = Framebuffer::new();
        let backdrop = Rgb888 { r: 7, g: 8, b: 9 };
        fb.fill(backdrop);

        layer.composite(&mut fb);

        // Index-0 pixel: backdrop shows through untouched.
        assert_eq!(fb.pixel(0, 0), Some(backdrop));
        // Index-1 pixel: painted red over the backdrop.
        assert_eq!(fb.pixel(1, 0), Some(red.to_rgb888()));
    }

    #[test]
    fn composite_does_not_paint_bank_color_for_index_0_in_a_nonzero_bank() {
        // Regression: a 4bpp tile drawn through bank>0 must NOT paint
        // colors[bank*16] for its index-0 pixels — index 0 is transparent in
        // every bank, so the backdrop must survive.
        let mut tile_bytes = [0u8; 32];
        tile_bytes[0] = 0x00; // pixel(0,0)=0, pixel(1,0)=0
        let tileset = Tileset::decode(BitDepth::Bpp4, &tile_bytes).unwrap();

        let mut colors = [Bgr555::default(); Palette::LEN];
        // Bank 1 index 0 is a vivid colour we must never see on screen.
        let bank1_index0 = Bgr555::from_channels(0x1F, 0, 0x1F);
        colors[16] = bank1_index0;
        let palette = Palette::new(colors);

        let entries = vec![ScreenEntry::new(0, false, false, 1)];
        let tilemap = Tilemap::new(1, 1, entries).unwrap();

        let layer = BgLayer::new(&tileset, &palette, &tilemap);
        let mut fb = Framebuffer::new();
        let backdrop = Rgb888 { r: 3, g: 4, b: 5 };
        fb.fill(backdrop);

        layer.composite(&mut fb);

        // Backdrop survives; the bank-1 index-0 colour never appears.
        assert_eq!(fb.pixel(0, 0), Some(backdrop));
        assert_ne!(fb.pixel(0, 0), Some(bank1_index0.to_rgb888()));
    }

    /// Build a 32x32-tile (256x256px, the smallest hardware regular-BG size)
    /// tilemap where tile `n` (at tilemap col `n`, row 0) is opaque index 1
    /// through a distinct palette bank `n`, so reading back which bank
    /// painted a framebuffer pixel identifies which source tile it came
    /// from.
    fn marked_256px_tilemap() -> (Tileset, Palette, Tilemap) {
        let mut tile_bytes = [0u8; 32];
        tile_bytes[0] = 0x01; // pixel(0,0) = index 1, opaque
        let tileset = Tileset::decode(BitDepth::Bpp4, &tile_bytes).unwrap();

        let mut colors = [Bgr555::default(); Palette::LEN];
        for bank in 0..16u8 {
            colors[usize::from(bank) * 16 + 1] = Bgr555::from_channels(bank, 0x1F - bank, 0);
        }
        let palette = Palette::new(colors);

        let mut entries = vec![ScreenEntry::new(0, false, false, 0); 32 * 32];
        // Mark tilemap column 0 of every row with a bank equal to (row % 16),
        // and row 0 with a bank equal to (col % 16), so both axes carry an
        // identifiable signal for wrap tests.
        for (col, entry) in entries.iter_mut().take(32).enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            let bank = (col % 16) as u8;
            *entry = ScreenEntry::new(0, false, false, bank);
        }
        for (row, entry) in entries.iter_mut().step_by(32).enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            let bank = (row % 16) as u8;
            *entry = ScreenEntry::new(0, false, false, bank);
        }
        let tilemap = Tilemap::new(32, 32, entries).unwrap();
        (tileset, palette, tilemap)
    }

    #[test]
    fn composite_scrolled_with_zero_offset_matches_composite() {
        let (tileset, palette, tilemap) = marked_256px_tilemap();
        let layer = BgLayer::new(&tileset, &palette, &tilemap);

        let mut unscrolled = Framebuffer::new();
        layer.composite(&mut unscrolled);
        let mut scrolled = Framebuffer::new();
        layer.composite_scrolled(&mut scrolled, 0, 0);

        assert_eq!(unscrolled.pixels(), scrolled.pixels());
    }

    #[test]
    fn composite_scrolled_wraps_horizontally_across_the_tilemap_edge() {
        let (tileset, palette, tilemap) = marked_256px_tilemap();
        let layer = BgLayer::new(&tileset, &palette, &tilemap);

        // scroll_x=248 (31 tiles in): framebuffer col 0 samples source col
        // 248, i.e. tilemap col 31 (248/8=31) -> bank (31 % 16) = 15.
        // Framebuffer col 8 samples source col 256, which wraps modulo 256
        // back to tilemap col 0 -> bank 0.
        let mut fb = Framebuffer::new();
        layer.composite_scrolled(&mut fb, 248, 0);

        let bank15 = Bgr555::from_channels(15, 0x1F - 15, 0).to_rgb888();
        let bank0 = Bgr555::from_channels(0, 0x1F, 0).to_rgb888();
        assert_eq!(fb.pixel(0, 0), Some(bank15));
        assert_eq!(fb.pixel(8, 0), Some(bank0));
    }

    #[test]
    fn composite_scrolled_wraps_vertically_across_the_tilemap_edge() {
        let (tileset, palette, tilemap) = marked_256px_tilemap();
        let layer = BgLayer::new(&tileset, &palette, &tilemap);

        // scroll_y=248: framebuffer row 0 samples source row 248 -> tilemap
        // row 31 -> bank 15. Framebuffer row 8 samples source row 256,
        // wraps to tilemap row 0 -> bank 0.
        let mut fb = Framebuffer::new();
        layer.composite_scrolled(&mut fb, 0, 248);

        let bank15 = Bgr555::from_channels(15, 0x1F - 15, 0).to_rgb888();
        let bank0 = Bgr555::from_channels(0, 0x1F, 0).to_rgb888();
        assert_eq!(fb.pixel(0, 0), Some(bank15));
        assert_eq!(fb.pixel(0, 8), Some(bank0));
    }

    #[test]
    fn composite_scrolled_masks_scroll_registers_to_9_bits() {
        let (tileset, palette, tilemap) = marked_256px_tilemap();
        let layer = BgLayer::new(&tileset, &palette, &tilemap);

        // 768 (0x300) & 0x1FF == 256, and 256 % 256 == 0, so this must be
        // pixel-identical to an explicit scroll of 0.
        let mut masked = Framebuffer::new();
        layer.composite_scrolled(&mut masked, 768, 0);
        let mut zero = Framebuffer::new();
        layer.composite_scrolled(&mut zero, 0, 0);

        assert_eq!(masked.pixels(), zero.pixels());
    }

    #[test]
    fn composite_scrolled_applies_flip_bits_the_same_as_composite() {
        // Scrolling must not disturb per-entry flip resolution: reuse the
        // flip fixture from composite_resolves_flip_bits_and_palette_banks,
        // scrolled by exactly one tilemap width so it's pixel-identical to
        // the unscrolled result once wrapped.
        let tile_bytes = checkerboard_4bpp_tile();
        let tileset = Tileset::decode(BitDepth::Bpp4, &tile_bytes).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[1] = Bgr555::from_channels(0x1F, 0, 0);
        let palette = Palette::new(colors);
        let entries = vec![ScreenEntry::new(0, true, false, 0)]; // h-flipped
        let tilemap = Tilemap::new(1, 1, entries).unwrap();
        let layer = BgLayer::new(&tileset, &palette, &tilemap);

        let mut unscrolled = Framebuffer::new();
        layer.composite(&mut unscrolled);
        // A full-width (8px) scroll wraps back to the same content.
        let mut scrolled = Framebuffer::new();
        layer.composite_scrolled(&mut scrolled, 8, 0);

        assert_eq!(unscrolled.pixel(0, 0), scrolled.pixel(0, 0));
        assert_eq!(unscrolled.pixel(7, 0), scrolled.pixel(7, 0));
    }
}
