//! A single regular (non-affine, non-scrolling) BG tile layer compositor
//! (S-2 slice 1).
//!
//! Ports the regular-background screen-entry semantics of
//! `pokeemerald/src/bg.c`: a tilemap is a grid of 16-bit screen entries,
//! each selecting a tile index, a horizontal/vertical flip, and (for 4bpp
//! tiles) a palette bank. The bitfield layout matches the standard GBA
//! text-mode screen entry, verified against `mgba`'s `GBA_TEXT_MAP_*` decode
//! macros (`mgba/src/gba/renderers/software-mode0.c`): bits 0-9 tile index,
//! bit 10 h-flip, bit 11 v-flip, bits 12-15 palette bank
//! `(behavioral-fidelity)`.
//!
//! Scrolling offsets, affine transforms, multi-layer priority ordering, and
//! windows/blending are out of scope for this slice — see issue #50. A
//! layer always composites starting at tilemap origin `(0, 0)`; a tilemap
//! larger than the framebuffer is naturally clipped to the visible area,
//! same as an unscrolled GBA regular BG.

use crate::error::RenderError;
use crate::framebuffer::Framebuffer;
use crate::palette::Palette;
use crate::tile::{BitDepth, Tile, Tileset};

/// One 16-bit regular-BG tilemap screen entry.
///
/// Mirrors the standard GBA text-mode screen entry layout: tile index in
/// bits 0-9, horizontal flip in bit 10, vertical flip in bit 11, and 4bpp
/// palette bank in bits 12-15 (ignored for 8bpp tiles).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScreenEntry {
    tile_index: u16,
    h_flip: bool,
    v_flip: bool,
    palette_bank: u8,
}

impl ScreenEntry {
    const TILE_INDEX_MASK: u16 = 0x03FF;
    const H_FLIP_BIT: u16 = 0x0400;
    const V_FLIP_BIT: u16 = 0x0800;

    /// Decode a raw 16-bit screen entry.
    #[must_use]
    pub const fn from_raw(raw: u16) -> Self {
        #[allow(clippy::cast_possible_truncation)] // raw >> 12 fits in 4 bits.
        let palette_bank = (raw >> 12) as u8;
        Self {
            tile_index: raw & Self::TILE_INDEX_MASK,
            h_flip: raw & Self::H_FLIP_BIT != 0,
            v_flip: raw & Self::V_FLIP_BIT != 0,
            palette_bank,
        }
    }

    /// Build a screen entry directly from its decoded fields. `tile_index`
    /// is masked to 10 bits, `palette_bank` to 4 bits.
    #[must_use]
    pub const fn new(tile_index: u16, h_flip: bool, v_flip: bool, palette_bank: u8) -> Self {
        Self {
            tile_index: tile_index & Self::TILE_INDEX_MASK,
            h_flip,
            v_flip,
            palette_bank: palette_bank & 0x0F,
        }
    }

    /// The tile index into the [`Tileset`] (`0..1024`).
    #[must_use]
    pub const fn tile_index(self) -> u16 {
        self.tile_index
    }

    /// Whether the tile is drawn mirrored horizontally.
    #[must_use]
    pub const fn h_flip(self) -> bool {
        self.h_flip
    }

    /// Whether the tile is drawn mirrored vertically.
    #[must_use]
    pub const fn v_flip(self) -> bool {
        self.v_flip
    }

    /// The 4bpp palette bank (`0..16`); unused for 8bpp tiles.
    #[must_use]
    pub const fn palette_bank(self) -> u8 {
        self.palette_bank
    }
}

/// A static grid of [`ScreenEntry`] values: one regular BG layer's tilemap.
///
/// No scrolling offset is modelled — the layer always composites starting
/// at tilemap origin `(0, 0)` (see issue #50's scope).
#[derive(Debug, Clone)]
pub struct Tilemap {
    width_tiles: usize,
    height_tiles: usize,
    entries: Vec<ScreenEntry>,
}

impl Tilemap {
    /// Build a tilemap from a row-major grid of screen entries.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::TilemapSizeMismatch`] if `entries.len() !=
    /// width_tiles * height_tiles`.
    /// Slice-1 scope: sizes are caller-supplied and the compositor CLIPS at the
    /// framebuffer edge. Real regular BGs are fixed hardware sizes (256x256 /
    /// 512x256 / 256x512 / 512x512) and WRAP; once scrolling lands, wraparound
    /// must replace this clip behaviour — do not generalize from it.
    pub fn new(
        width_tiles: usize,
        height_tiles: usize,
        entries: Vec<ScreenEntry>,
    ) -> Result<Self, RenderError> {
        let expected = width_tiles * height_tiles;
        if entries.len() != expected {
            return Err(RenderError::TilemapSizeMismatch {
                expected,
                actual: entries.len(),
            });
        }
        Ok(Self {
            width_tiles,
            height_tiles,
            entries,
        })
    }

    /// Width in tiles.
    #[must_use]
    pub const fn width_tiles(&self) -> usize {
        self.width_tiles
    }

    /// Height in tiles.
    #[must_use]
    pub const fn height_tiles(&self) -> usize {
        self.height_tiles
    }

    /// The screen entry at tile coordinates `(col, row)`, or `None` if out
    /// of range.
    #[must_use]
    pub fn entry(&self, col: usize, row: usize) -> Option<ScreenEntry> {
        if col >= self.width_tiles || row >= self.height_tiles {
            return None;
        }
        self.entries.get(row * self.width_tiles + col).copied()
    }
}

/// A single regular (non-affine) BG tile layer: a [`Tileset`], a
/// [`Palette`], and a [`Tilemap`], ready to composite into a
/// [`Framebuffer`].
///
/// No scrolling offset, no priority-vs-other-layers ordering, no affine
/// transform, and no windows/blending — see issue #50's scope.
#[derive(Debug, Clone, Copy)]
pub struct BgLayer<'a> {
    tileset: &'a Tileset,
    palette: &'a Palette,
    tilemap: &'a Tilemap,
}

impl<'a> BgLayer<'a> {
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
    /// `(0, 0)`.
    ///
    /// A tilemap larger than the framebuffer is clipped to the visible
    /// 240x160 area; a tilemap smaller than the framebuffer leaves the
    /// uncovered area untouched. A screen entry whose tile index has no
    /// matching tile in the tileset is skipped (left untouched) rather than
    /// treated as an error.
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
                let Some(entry) = self.tilemap.entry(col, row) else {
                    continue;
                };
                let Some(tile) = self.tileset.tile(entry.tile_index()) else {
                    continue;
                };
                self.composite_tile(framebuffer, tile, entry, origin_x, origin_y);
            }
        }
    }

    /// Composite one 8x8 tile at framebuffer origin `(origin_x, origin_y)`,
    /// applying the screen entry's flip bits and resolving palette indices
    /// through this layer's palette (bank-relative for 4bpp, flat for
    /// 8bpp).
    fn composite_tile(
        &self,
        framebuffer: &mut Framebuffer,
        tile: &Tile,
        entry: ScreenEntry,
        origin_x: usize,
        origin_y: usize,
    ) {
        const DIM: usize = BitDepth::TILE_DIM;
        for local_y in 0..DIM {
            let tile_y = if entry.v_flip() {
                DIM - 1 - local_y
            } else {
                local_y
            };
            for local_x in 0..DIM {
                let tile_x = if entry.h_flip() {
                    DIM - 1 - local_x
                } else {
                    local_x
                };
                // Slice-1 scope: color index 0 is written literally. On hardware,
                // BG color index 0 is TRANSPARENT (backdrop/lower layer shows
                // through); multi-layer compositing must SKIP index-0 pixels here
                // rather than paint them, or upper layers will occlude lower ones.
                let index = tile.index(tile_x, tile_y);
                let color = match self.tileset.bit_depth() {
                    BitDepth::Bpp4 => self.palette.bank_color(entry.palette_bank(), index),
                    BitDepth::Bpp8 => self.palette.color(index),
                };
                framebuffer.set_pixel(origin_x + local_x, origin_y + local_y, color.to_rgb888());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BgLayer, ScreenEntry, Tilemap};
    use crate::error::RenderError;
    use crate::framebuffer::Framebuffer;
    use crate::palette::{Bgr555, Palette, Rgb888};
    use crate::tile::{BitDepth, Tileset};

    #[test]
    fn screen_entry_decodes_gba_text_map_bitfields() {
        // 0x1234 = 0001 0010 0011 0100: tile index bits0-9 = 0x234 = 564,
        // h-flip bit10 clear, v-flip bit11 clear, palette bank = 0x1.
        let entry = ScreenEntry::from_raw(0x1234);
        assert_eq!(entry.tile_index(), 0x234);
        assert!(!entry.h_flip());
        assert!(!entry.v_flip());
        assert_eq!(entry.palette_bank(), 1);

        // 0xFC00 = 1111 1100 0000 0000: tile index = 0, h-flip and v-flip
        // set, palette bank = 0xF.
        let entry = ScreenEntry::from_raw(0xFC00);
        assert_eq!(entry.tile_index(), 0);
        assert!(entry.h_flip());
        assert!(entry.v_flip());
        assert_eq!(entry.palette_bank(), 0xF);
    }

    #[test]
    fn screen_entry_new_masks_out_of_range_fields() {
        let entry = ScreenEntry::new(0xFFFF, true, false, 0xFF);
        assert_eq!(entry.tile_index(), 0x03FF);
        assert_eq!(entry.palette_bank(), 0x0F);
    }

    #[test]
    fn tilemap_new_rejects_entry_count_mismatch() {
        let entries = vec![ScreenEntry::new(0, false, false, 0); 5];
        assert_eq!(
            Tilemap::new(2, 3, entries).unwrap_err(),
            RenderError::TilemapSizeMismatch {
                expected: 6,
                actual: 5,
            }
        );
    }

    #[test]
    fn tilemap_entry_out_of_range_is_none() {
        let entries = vec![ScreenEntry::new(0, false, false, 0); 4];
        let tilemap = Tilemap::new(2, 2, entries).unwrap();
        assert!(tilemap.entry(2, 0).is_none());
        assert!(tilemap.entry(0, 2).is_none());
        assert!(tilemap.entry(1, 1).is_some());
    }

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
        assert_eq!(fb.pixel(0, 0), Some(Rgb888::BLACK)); // index 0
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
}
