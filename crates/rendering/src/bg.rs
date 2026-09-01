//! Regular-background tile sampling and composition.
//!
//! [`BgLayer`] resolves screen entries through a tileset and palette. Scrolled
//! samples mask offsets to the GBA's nine-bit regular-background scroll
//! registers, then wrap within the tilemap's pixel dimensions. The
//! [`crate::compositor`] owns ordering, windows, color effects, and mosaic.

use crate::framebuffer::Framebuffer;
use crate::palette::{Palette, Rgb888};
use crate::tile::{BitDepth, Tileset};
use crate::tilemap::Tilemap;

/// A regular-background tile layer backed by a tileset, palette, and tilemap.
#[derive(Debug, Clone, Copy)]
pub struct BgLayer<'a> {
    tileset: &'a Tileset,
    palette: &'a Palette,
    tilemap: &'a Tilemap,
}

impl<'a> BgLayer<'a> {
    const SCROLL_REGISTER_MASK: u16 = (1 << 9) - 1;

    /// Borrows the resources for one composable regular-background layer.
    #[must_use]
    pub const fn new(tileset: &'a Tileset, palette: &'a Palette, tilemap: &'a Tilemap) -> Self {
        Self {
            tileset,
            palette,
            tilemap,
        }
    }

    /// Composites from tilemap origin without scrolling or wraparound.
    ///
    /// Content outside the framebuffer is clipped. Pixels outside a smaller
    /// tilemap and entries without a matching tile leave the framebuffer
    /// untouched.
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
                        let Some(color) = self.sample_tile_pixel(col, row, local_x, local_y) else {
                            continue;
                        };
                        framebuffer.set_pixel(origin_x + local_x, origin_y + local_y, color);
                    }
                }
            }
        }
    }

    /// Composites a scrolled view that wraps within the tilemap.
    ///
    /// Scroll offsets are masked to the nine-bit `BGxHOFS` and `BGxVOFS`
    /// register widths. Transparent pixels and entries without a matching
    /// tile leave the framebuffer untouched.
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

    /// Samples a scrolled framebuffer coordinate, wrapping within the tilemap.
    ///
    /// Returns `None` for an empty tilemap, a missing entry or tile, or palette
    /// index zero. Index zero is transparent in every 4bpp bank and in 8bpp.
    #[must_use]
    pub(crate) fn sample_scrolled(
        &self,
        x: usize,
        y: usize,
        scroll_x: u16,
        scroll_y: u16,
    ) -> Option<Rgb888> {
        const DIM: usize = BitDepth::TILE_DIM;
        let width_tiles = self.tilemap.width_tiles();
        let height_tiles = self.tilemap.height_tiles();
        if width_tiles == 0 || height_tiles == 0 {
            return None;
        }
        let bg_width_px = width_tiles.checked_mul(DIM)?;
        let bg_height_px = height_tiles.checked_mul(DIM)?;
        let scroll_x = usize::from(scroll_x & Self::SCROLL_REGISTER_MASK);
        let scroll_y = usize::from(scroll_y & Self::SCROLL_REGISTER_MASK);
        let src_x = (x + scroll_x) % bg_width_px;
        let src_y = (y + scroll_y) % bg_height_px;
        self.sample_tile_pixel(src_x / DIM, src_y / DIM, src_x % DIM, src_y % DIM)
    }

    fn sample_tile_pixel(
        &self,
        col: usize,
        row: usize,
        tile_x: usize,
        tile_y: usize,
    ) -> Option<Rgb888> {
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
        let palette_index = tile.index(flipped_x, flipped_y);
        if palette_index == 0 {
            return None;
        }
        let color = match self.tileset.bit_depth() {
            BitDepth::Bpp4 => self.palette.bank_color(entry.palette_bank(), palette_index),
            BitDepth::Bpp8 => self.palette.color(palette_index),
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

    const MAX_CHANNEL: u8 = 0x1F;
    const BITS_PER_4BPP_PIXEL: u32 = 4;
    const PALETTE_BANK_COUNT: usize = Palette::LEN / Palette::BANK_LEN;

    #[expect(
        clippy::cast_possible_truncation,
        reason = "the generated palette indices are at most 15"
    )]
    fn asymmetric_4bpp_tile() -> [u8; 32] {
        const PIXELS_PER_BYTE: usize = 2;
        const BYTES_PER_ROW: usize = BitDepth::TILE_DIM / PIXELS_PER_BYTE;
        let mut bytes = [0u8; BitDepth::TILE_DIM * BYTES_PER_ROW];
        for (row, packed_row) in bytes.chunks_exact_mut(BYTES_PER_ROW).enumerate() {
            for (packed_x, packed_pixels) in packed_row.iter_mut().enumerate() {
                let first_index =
                    ((row % 2) * BitDepth::TILE_DIM + packed_x * PIXELS_PER_BYTE) as u8;
                *packed_pixels = first_index | ((first_index + 1) << BITS_PER_4BPP_PIXEL);
            }
        }
        bytes
    }

    #[test]
    fn composite_resolves_flip_bits_and_palette_banks() {
        let tile_bytes = asymmetric_4bpp_tile();
        let tileset = Tileset::decode(BitDepth::Bpp4, &tile_bytes).unwrap();

        let red = Bgr555::from_channels(MAX_CHANNEL, 0, 0);
        let green = Bgr555::from_channels(0, MAX_CHANNEL, 0);
        let blue = Bgr555::from_channels(0, 0, MAX_CHANNEL);
        let yellow = Bgr555::from_channels(MAX_CHANNEL, MAX_CHANNEL, 0);

        let mut colors = [Bgr555::default(); Palette::LEN];
        let red_index = 1;
        let green_index = BitDepth::TILE_DIM - 1;
        let blue_index = BitDepth::TILE_DIM;
        colors[red_index] = red;
        colors[green_index] = green;
        colors[blue_index] = blue;
        let second_bank = Palette::BANK_LEN;
        colors[second_bank + red_index] = yellow;
        colors[second_bank + green_index] = yellow;
        colors[second_bank + blue_index] = yellow;
        let palette = Palette::new(colors);

        let entries = vec![
            ScreenEntry::new(0, false, false, 0),
            ScreenEntry::new(0, true, true, 1),
        ];
        let tilemap = Tilemap::new(2, 1, entries).unwrap();

        let layer = BgLayer::new(&tileset, &palette, &tilemap);
        let mut fb = Framebuffer::new();
        let sentinel = Rgb888 { r: 9, g: 9, b: 9 };
        fb.fill(sentinel);

        layer.composite(&mut fb);

        let tile_side = BitDepth::TILE_DIM;
        let second_tile_origin_x = tile_side;
        assert_eq!(fb.pixel(0, 0), Some(sentinel));
        assert_eq!(fb.pixel(1, 0), Some(red.to_rgb888()));
        assert_eq!(fb.pixel(tile_side - 1, 0), Some(green.to_rgb888()));
        assert_eq!(fb.pixel(0, 1), Some(blue.to_rgb888()));
        assert_eq!(
            fb.pixel(second_tile_origin_x + tile_side - 1, 0),
            Some(yellow.to_rgb888())
        );
        assert_eq!(
            fb.pixel(second_tile_origin_x, tile_side - 1),
            Some(yellow.to_rgb888())
        );
        assert_eq!(fb.pixel(second_tile_origin_x, 0), Some(Rgb888::BLACK));

        assert_eq!(
            fb.pixel(second_tile_origin_x + tile_side, 0),
            Some(sentinel)
        );
        assert_eq!(
            fb.pixel(second_tile_origin_x + tile_side + 4, 0),
            Some(sentinel)
        );
        assert_eq!(fb.pixel(0, tile_side), Some(sentinel));
        assert_eq!(fb.pixel(fb.width() - 1, fb.height() - 1), Some(sentinel));
    }

    #[test]
    fn composite_8bpp_uses_the_flat_palette_directly() {
        let mut tile_bytes = [0u8; BitDepth::TILE_DIM * BitDepth::TILE_DIM];
        let orange_index = 200;
        let unassigned_index = 5;
        tile_bytes[0] = orange_index;
        tile_bytes[1] = unassigned_index;
        let tileset = Tileset::decode(BitDepth::Bpp8, &tile_bytes).unwrap();

        let mut colors = [Bgr555::default(); Palette::LEN];
        let orange = Bgr555::from_channels(MAX_CHANNEL, 0x10, 0);
        colors[usize::from(orange_index)] = orange;
        let palette = Palette::new(colors);

        let ignored_palette_bank = 3;
        let entries = vec![ScreenEntry::new(0, false, false, ignored_palette_bank)];
        let tilemap = Tilemap::new(1, 1, entries).unwrap();

        let layer = BgLayer::new(&tileset, &palette, &tilemap);
        let mut fb = Framebuffer::new();
        layer.composite(&mut fb);

        assert_eq!(fb.pixel(0, 0), Some(orange.to_rgb888()));
        assert_eq!(fb.pixel(1, 0), Some(Rgb888::BLACK));
    }

    #[test]
    fn composite_skips_screen_entries_with_no_matching_tile() {
        let tile_bytes = vec![0; BitDepth::Bpp4.tile_byte_len()];
        let tileset = Tileset::decode(BitDepth::Bpp4, &tile_bytes).unwrap();
        let palette = Palette::new([Bgr555::default(); Palette::LEN]);
        let missing_tile_index = 5;
        let entries = vec![ScreenEntry::new(missing_tile_index, false, false, 0)];
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
        let tile_bytes = vec![u8::MAX; BitDepth::Bpp4.tile_byte_len()];
        let tileset = Tileset::decode(BitDepth::Bpp4, &tile_bytes).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        let opaque_index = Palette::BANK_LEN - 1;
        colors[opaque_index] = Bgr555::from_channels(MAX_CHANNEL, MAX_CHANNEL, MAX_CHANNEL);
        let palette = Palette::new(colors);
        let width_tiles = Framebuffer::WIDTH / BitDepth::TILE_DIM + 1;
        let height_tiles = Framebuffer::HEIGHT / BitDepth::TILE_DIM + 1;
        let entries = vec![ScreenEntry::new(0, false, false, 0); width_tiles * height_tiles];
        let tilemap = Tilemap::new(width_tiles, height_tiles, entries).unwrap();

        let layer = BgLayer::new(&tileset, &palette, &tilemap);
        let mut fb = Framebuffer::new();
        layer.composite(&mut fb);

        assert_eq!(
            fb.pixel(Framebuffer::WIDTH - 1, Framebuffer::HEIGHT - 1),
            Some(colors[opaque_index].to_rgb888())
        );
    }

    #[test]
    fn composite_leaves_index_0_pixels_transparent_over_a_backdrop() {
        let transparent_index = 0;
        let red_index = 1;
        let mut tile_bytes = vec![0; BitDepth::Bpp4.tile_byte_len()];
        tile_bytes[0] = transparent_index | (red_index << BITS_PER_4BPP_PIXEL);
        let tileset = Tileset::decode(BitDepth::Bpp4, &tile_bytes).unwrap();

        let mut colors = [Bgr555::default(); Palette::LEN];
        let red = Bgr555::from_channels(MAX_CHANNEL, 0, 0);
        colors[usize::from(red_index)] = red;
        let palette = Palette::new(colors);

        let entries = vec![ScreenEntry::new(0, false, false, 0)];
        let tilemap = Tilemap::new(1, 1, entries).unwrap();

        let layer = BgLayer::new(&tileset, &palette, &tilemap);
        let mut fb = Framebuffer::new();
        let backdrop = Rgb888 { r: 7, g: 8, b: 9 };
        fb.fill(backdrop);

        layer.composite(&mut fb);

        assert_eq!(fb.pixel(0, 0), Some(backdrop));
        assert_eq!(fb.pixel(1, 0), Some(red.to_rgb888()));
    }

    #[test]
    fn composite_does_not_paint_bank_color_for_index_0_in_a_nonzero_bank() {
        let tile_bytes = vec![0; BitDepth::Bpp4.tile_byte_len()];
        let tileset = Tileset::decode(BitDepth::Bpp4, &tile_bytes).unwrap();

        let mut colors = [Bgr555::default(); Palette::LEN];
        let bank1_index0 = Bgr555::from_channels(0x1F, 0, 0x1F);
        colors[Palette::BANK_LEN] = bank1_index0;
        let palette = Palette::new(colors);

        let entries = vec![ScreenEntry::new(0, false, false, 1)];
        let tilemap = Tilemap::new(1, 1, entries).unwrap();

        let layer = BgLayer::new(&tileset, &palette, &tilemap);
        let mut fb = Framebuffer::new();
        let backdrop = Rgb888 { r: 3, g: 4, b: 5 };
        fb.fill(backdrop);

        layer.composite(&mut fb);

        assert_eq!(fb.pixel(0, 0), Some(backdrop));
        assert_ne!(fb.pixel(0, 0), Some(bank1_index0.to_rgb888()));
    }

    fn marked_256px_tilemap() -> (Tileset, Palette, Tilemap) {
        const TILEMAP_SIDE_TILES: usize = 32;
        let opaque_index = 1u8;
        let mut tile_bytes = vec![0; BitDepth::Bpp4.tile_byte_len()];
        tile_bytes[0] = opaque_index;
        let tileset = Tileset::decode(BitDepth::Bpp4, &tile_bytes).unwrap();

        let mut colors = [Bgr555::default(); Palette::LEN];
        let palette_bank_count = u8::try_from(PALETTE_BANK_COUNT).unwrap();
        for bank in 0..palette_bank_count {
            colors[usize::from(bank) * Palette::BANK_LEN + usize::from(opaque_index)] =
                marker_color(bank);
        }
        let palette = Palette::new(colors);

        let mut entries =
            vec![ScreenEntry::new(0, false, false, 0); TILEMAP_SIDE_TILES * TILEMAP_SIDE_TILES];
        for (col, entry) in entries.iter_mut().take(TILEMAP_SIDE_TILES).enumerate() {
            let bank = u8::try_from(col % PALETTE_BANK_COUNT).unwrap();
            *entry = ScreenEntry::new(0, false, false, bank);
        }
        for (row, entry) in entries.iter_mut().step_by(TILEMAP_SIDE_TILES).enumerate() {
            let bank = u8::try_from(row % PALETTE_BANK_COUNT).unwrap();
            *entry = ScreenEntry::new(0, false, false, bank);
        }
        let tilemap = Tilemap::new(TILEMAP_SIDE_TILES, TILEMAP_SIDE_TILES, entries).unwrap();
        (tileset, palette, tilemap)
    }

    fn marker_color(bank: u8) -> Bgr555 {
        Bgr555::from_channels(bank, MAX_CHANNEL - bank, 0)
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

        let tilemap_width_pixels = tilemap.width_tiles() * BitDepth::TILE_DIM;
        let last_tile_scroll = tilemap_width_pixels - BitDepth::TILE_DIM;
        let mut fb = Framebuffer::new();
        layer.composite_scrolled(&mut fb, u16::try_from(last_tile_scroll).unwrap(), 0);

        let last_bank = u8::try_from(PALETTE_BANK_COUNT - 1).unwrap();
        assert_eq!(fb.pixel(0, 0), Some(marker_color(last_bank).to_rgb888()));
        assert_eq!(
            fb.pixel(BitDepth::TILE_DIM, 0),
            Some(marker_color(0).to_rgb888())
        );
    }

    #[test]
    fn composite_scrolled_wraps_vertically_across_the_tilemap_edge() {
        let (tileset, palette, tilemap) = marked_256px_tilemap();
        let layer = BgLayer::new(&tileset, &palette, &tilemap);

        let tilemap_height_pixels = tilemap.height_tiles() * BitDepth::TILE_DIM;
        let last_tile_scroll = tilemap_height_pixels - BitDepth::TILE_DIM;
        let mut fb = Framebuffer::new();
        layer.composite_scrolled(&mut fb, 0, u16::try_from(last_tile_scroll).unwrap());

        let last_bank = u8::try_from(PALETTE_BANK_COUNT - 1).unwrap();
        assert_eq!(fb.pixel(0, 0), Some(marker_color(last_bank).to_rgb888()));
        assert_eq!(
            fb.pixel(0, BitDepth::TILE_DIM),
            Some(marker_color(0).to_rgb888())
        );
    }

    #[test]
    fn composite_scrolled_masks_scroll_registers_to_9_bits() {
        const TEST_TILEMAP_WIDTH_TILES: usize = 25;
        const HIGHEST_NINE_BIT_REGISTER_BIT: u16 = 1 << 8;
        const FIRST_BIT_OUTSIDE_NINE_BIT_REGISTER: u16 = 1 << 9;
        const EXPECTED_BANK_AT_MASKED_ORIGIN: u8 = 7;

        let (tileset, palette, _) = marked_256px_tilemap();
        let entries = (0..TEST_TILEMAP_WIDTH_TILES)
            .map(|col| {
                let bank = u8::try_from(col % PALETTE_BANK_COUNT).unwrap();
                ScreenEntry::new(0, false, false, bank)
            })
            .collect();
        let tilemap = Tilemap::new(TEST_TILEMAP_WIDTH_TILES, 1, entries).unwrap();
        let layer = BgLayer::new(&tileset, &palette, &tilemap);

        let scroll = FIRST_BIT_OUTSIDE_NINE_BIT_REGISTER | HIGHEST_NINE_BIT_REGISTER_BIT;
        let mut framebuffer = Framebuffer::new();
        layer.composite_scrolled(&mut framebuffer, scroll, 0);

        assert_eq!(
            framebuffer.pixel(0, 0),
            Some(marker_color(EXPECTED_BANK_AT_MASKED_ORIGIN).to_rgb888())
        );
    }

    #[test]
    fn composite_scrolled_applies_flip_bits_the_same_as_composite() {
        let tile_bytes = asymmetric_4bpp_tile();
        let tileset = Tileset::decode(BitDepth::Bpp4, &tile_bytes).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[1] = Bgr555::from_channels(MAX_CHANNEL, 0, 0);
        let palette = Palette::new(colors);
        let entries = vec![ScreenEntry::new(0, true, false, 0)];
        let tilemap = Tilemap::new(1, 1, entries).unwrap();
        let layer = BgLayer::new(&tileset, &palette, &tilemap);

        let mut unscrolled = Framebuffer::new();
        layer.composite(&mut unscrolled);
        let mut scrolled = Framebuffer::new();
        let tilemap_width = u16::try_from(tilemap.width_tiles() * BitDepth::TILE_DIM).unwrap();
        layer.composite_scrolled(&mut scrolled, tilemap_width, 0);

        assert_eq!(unscrolled.pixel(0, 0), scrolled.pixel(0, 0));
        assert_eq!(
            unscrolled.pixel(BitDepth::TILE_DIM - 1, 0),
            scrolled.pixel(BitDepth::TILE_DIM - 1, 0)
        );
    }

    #[test]
    fn sample_scrolled_returns_none_for_extreme_zero_area_tilemaps() {
        let tileset = Tileset::decode(BitDepth::Bpp4, &[0u8; 32]).unwrap();
        let palette = Palette::new([Bgr555::default(); Palette::LEN]);

        for (width_tiles, height_tiles) in [(usize::MAX, 0), (0, usize::MAX)] {
            let tilemap = Tilemap::new(width_tiles, height_tiles, Vec::new()).unwrap();
            let layer = BgLayer::new(&tileset, &palette, &tilemap);
            assert_eq!(layer.sample_scrolled(0, 0, 0, 0), None);
        }
    }
}
