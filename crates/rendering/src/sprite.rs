//! The OAM-equivalent sprite layer: compositing [`OamEntry`](crate::oam::OamEntry)
//! values into pixels (S-2 slice 2).
//!
//! A sprite renderer compositing enabled sprites into framebuffer output
//! with correct transparency (palette index 0), horizontal/vertical flip,
//! and partial off-screen clipping (including the position-wrapping
//! semantics documented on [`OamEntry`](crate::oam::OamEntry)).
//!
//! Per-sprite tile layout matches pokeemerald's OBJ character mapping: nearly
//! every `SetGpuReg(REG_OFFSET_DISPCNT, ...)` call in `pokeemerald/src`
//! includes `DISPCNT_OBJ_1D_MAP`, so a sprite's own tiles are laid out
//! contiguously, row-major, starting at its OAM tile index. 2D OBJ character
//! mapping (a fixed-width VRAM sheet shared across sprites) is the kind of
//! "tile memory mapping mode beyond what composition needs" issue #64 calls
//! out of scope.
//!
//! Sprite-vs-sprite ordering is verified against
//! `mgba/src/gba/renderers/software-obj.c` and `video-software.c`: among
//! sprites covering the same pixel, the lowest OBJ priority wins; a
//! same-priority tie is won by the lower OAM index (see
//! [`SpriteLayer::resolve_pixel`]) `(behavioral-fidelity)`.

use crate::framebuffer::Framebuffer;
use crate::oam::OamEntry;
use crate::palette::{Palette, Rgb888};
use crate::tile::{BitDepth, Tileset};

/// One resolved, opaque sprite pixel: a color plus the OBJ priority it
/// composited at (needed by the cross-layer priority compositor to compare
/// against BG layer priorities).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpritePixel {
    /// The resolved color.
    pub color: Rgb888,
    /// The winning sprite's OBJ priority (`0..=3`).
    pub priority: u8,
}

/// The full regular-sprite OBJ layer: up to 128 [`OamEntry`] values plus the
/// tile/palette data they're drawn from, ready to resolve into pixels.
///
/// Sprites are addressed by their position in `entries` — that position
/// *is* this engine's OAM index, so `entries[0]` is OAM slot 0 and so on.
/// Lower-indexed sprites win ties against higher-indexed sprites at the same
/// priority (see [`SpriteLayer::resolve_pixel`]).
///
/// 4bpp and 8bpp sprites draw from separate [`Tileset`]s (matching the
/// bit-depth split modelled by [`Tileset`] itself); which one an entry uses
/// is selected by [`OamEntry::bit_depth`].
#[derive(Debug, Clone, Copy)]
pub struct SpriteLayer<'a> {
    entries: &'a [OamEntry],
    tileset_4bpp: &'a Tileset,
    tileset_8bpp: &'a Tileset,
    palette: &'a Palette,
}

impl<'a> SpriteLayer<'a> {
    /// Borrow a sprite entry list together with the tile/palette data it
    /// draws from.
    #[must_use]
    pub const fn new(
        entries: &'a [OamEntry],
        tileset_4bpp: &'a Tileset,
        tileset_8bpp: &'a Tileset,
        palette: &'a Palette,
    ) -> Self {
        Self {
            entries,
            tileset_4bpp,
            tileset_8bpp,
            palette,
        }
    }

    /// Composite only the sprite layer into `framebuffer` (no BG layers) —
    /// useful standalone and for testing sprite-vs-sprite ordering in
    /// isolation. The full BG+sprite priority compositor is
    /// [`compose_frame`](crate::compositor::compose_frame).
    pub fn composite(&self, framebuffer: &mut Framebuffer) {
        for y in 0..framebuffer.height() {
            for x in 0..framebuffer.width() {
                if let Some(pixel) = self.resolve_pixel(x, y) {
                    framebuffer.set_pixel(x, y, pixel.color);
                }
            }
        }
    }

    /// Resolve the winning sprite pixel at `(x, y)`, or `None` if every
    /// sprite is disabled, transparent, or clipped there.
    ///
    /// Implements the OBJ ordering rule verified against
    /// `mgba/src/gba/renderers/software-obj.c` and `video-software.c`: among
    /// sprites that cover this pixel, the lowest [`OamEntry::priority`]
    /// wins; a tie is won by the lower OAM index (this method only replaces
    /// the current winner on a strictly lower priority, so the
    /// first-encountered — lowest-indexed — entry keeps a tie)
    /// `(behavioral-fidelity)`.
    #[must_use]
    pub fn resolve_pixel(&self, x: usize, y: usize) -> Option<SpritePixel> {
        let mut winner: Option<SpritePixel> = None;
        for entry in self.entries {
            if !entry.enabled() {
                continue;
            }
            let Some(color) = self.sample_entry(entry, x, y) else {
                continue;
            };
            let candidate = SpritePixel {
                color,
                priority: entry.priority(),
            };
            winner = match winner {
                Some(w) if w.priority <= candidate.priority => Some(w),
                _ => Some(candidate),
            };
        }
        winner
    }

    /// Sample one sprite's pixel at framebuffer coordinate `(x, y)`, or
    /// `None` if `(x, y)` is outside the sprite's footprint or lands on a
    /// transparent (palette index 0) texel.
    ///
    /// `x`/`y` are always framebuffer coordinates (`<240`, `<160`) and
    /// sprite dimensions never exceed 64, so the `i32` round-trips below
    /// never truncate, wrap, or lose their sign in practice — the
    /// `#[allow]`s document that, rather than threading `TryFrom` through
    /// arithmetic that cannot actually fail here.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss
    )]
    fn sample_entry(&self, entry: &OamEntry, x: usize, y: usize) -> Option<Rgb888> {
        const DIM: usize = BitDepth::TILE_DIM;
        let (width, height) = entry.dimensions();

        // X: no positional wrap (the 9-bit field already decoded to a
        // signed screen position, see oam.rs's module docs) — just
        // offset+clip.
        let dx = x as i32 - i32::from(entry.x());
        if dx < 0 || dx as usize >= width {
            return None;
        }

        // Y: OBJ Y-space wraps modulo 256, so a sprite hanging off the
        // bottom re-appears at the top (oam.rs's module docs).
        let dy = (y as i32 - i32::from(entry.y())).rem_euclid(OamEntry::Y_SPACE) as usize;
        if dy >= height {
            return None;
        }

        // H/V flip mirrors the whole sprite footprint, not each tile
        // independently (unlike a BG ScreenEntry's per-tile flip bits).
        let local_col = if entry.h_flip() {
            width - 1 - dx as usize
        } else {
            dx as usize
        };
        let local_row = if entry.v_flip() { height - 1 - dy } else { dy };

        let tiles_per_row = width / DIM;
        let tile_col = local_col / DIM;
        let tile_row = local_row / DIM;
        #[allow(clippy::cast_possible_truncation)] // OAM tile indices fit in u16.
        let tile_offset = (tile_row * tiles_per_row + tile_col) as u16;
        let tile_idx = entry.tile_index().wrapping_add(tile_offset);

        let tileset = match entry.bit_depth() {
            BitDepth::Bpp4 => self.tileset_4bpp,
            BitDepth::Bpp8 => self.tileset_8bpp,
        };
        let tile = tileset.tile(tile_idx)?;
        let index = tile.index(local_col % DIM, local_row % DIM);
        // Palette index 0 is transparent, in every bank and for 8bpp, same
        // as a regular BG (see bg.rs's sample_pixel).
        if index == 0 {
            return None;
        }
        let color = match entry.bit_depth() {
            BitDepth::Bpp4 => self.palette.bank_color(entry.palette_bank(), index),
            BitDepth::Bpp8 => self.palette.color(index),
        };
        Some(color.to_rgb888())
    }
}

#[cfg(test)]
mod tests {
    use super::SpriteLayer;
    use crate::framebuffer::Framebuffer;
    use crate::oam::{OamEntry, ObjShape};
    use crate::palette::{Bgr555, Palette, Rgb888};
    use crate::tile::{BitDepth, Tileset};

    fn entry(x_raw: u16, y: u8, enabled: bool) -> OamEntry {
        OamEntry::new(
            x_raw,
            y,
            0,
            0,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            0, // 8x8
            0,
            enabled,
        )
    }

    /// A 4bpp 8x8 tile whose top-left 2x2 block is index 0 (transparent),
    /// index 1 (red), index 2 (green), index 3 (blue), row-major:
    /// (0,0)=0 (1,0)=1
    /// (0,1)=2 (1,1)=3
    fn quadrant_tile() -> [u8; 32] {
        let mut bytes = [0u8; 32];
        bytes[0] = 0x10; // (0,0)=0 low nibble, (1,0)=1 high nibble
        bytes[4] = 0x32; // (0,1)=2, (1,1)=3
        bytes
    }

    fn quadrant_palette() -> Palette {
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[1] = Bgr555::from_channels(0x1F, 0, 0); // red
        colors[2] = Bgr555::from_channels(0, 0x1F, 0); // green
        colors[3] = Bgr555::from_channels(0, 0, 0x1F); // blue
        Palette::new(colors)
    }

    #[test]
    fn composite_skips_transparent_index_0_pixels() {
        let tileset = Tileset::decode(BitDepth::Bpp4, &quadrant_tile()).unwrap();
        let palette = quadrant_palette();
        let entries = [entry(0, 0, true)];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        let mut fb = Framebuffer::new();
        let backdrop = Rgb888 { r: 5, g: 6, b: 7 };
        fb.fill(backdrop);
        layer.composite(&mut fb);

        assert_eq!(fb.pixel(0, 0), Some(backdrop)); // index 0 -> transparent
        assert_eq!(
            fb.pixel(1, 0),
            Some(Bgr555::from_channels(0x1F, 0, 0).to_rgb888())
        );
        assert_eq!(
            fb.pixel(0, 1),
            Some(Bgr555::from_channels(0, 0x1F, 0).to_rgb888())
        );
        assert_eq!(
            fb.pixel(1, 1),
            Some(Bgr555::from_channels(0, 0, 0x1F).to_rgb888())
        );
    }

    /// A 4bpp 8x8 tile with a distinct opaque color at each of its four
    /// corners — (0,0)=1 (red), (7,0)=2 (green), (0,7)=3 (blue), (7,7)=4
    /// (yellow) — and every other pixel transparent (index 0), so h/v flip
    /// across the *whole* 8-wide/8-tall sprite footprint is observable
    /// (unlike `quadrant_tile`, whose marks all sit in one 2x2 corner and so
    /// can't distinguish "flip the whole sprite" from "flip within one
    /// 2x2 block").
    fn corner_marked_tile() -> [u8; 32] {
        let mut bytes = [0u8; 32];
        bytes[0] = 0x01; // row 0: pixel(0,0) low nibble = 1
        bytes[3] = 0x20; // row 0: pixel(7,0) high nibble = 2
        bytes[7 * 4] = 0x03; // row 7: pixel(0,7) low nibble = 3
        bytes[7 * 4 + 3] = 0x40; // row 7: pixel(7,7) high nibble = 4
        bytes
    }

    fn corner_marked_palette() -> Palette {
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[1] = Bgr555::from_channels(0x1F, 0, 0); // red
        colors[2] = Bgr555::from_channels(0, 0x1F, 0); // green
        colors[3] = Bgr555::from_channels(0, 0, 0x1F); // blue
        colors[4] = Bgr555::from_channels(0x1F, 0x1F, 0); // yellow
        Palette::new(colors)
    }

    #[test]
    fn composite_h_flip_mirrors_the_whole_sprite_footprint() {
        let tileset = Tileset::decode(BitDepth::Bpp4, &corner_marked_tile()).unwrap();
        let palette = corner_marked_palette();
        let entries = [OamEntry::new(
            0,
            0,
            0,
            0,
            BitDepth::Bpp4,
            true, // h_flip
            false,
            ObjShape::Square,
            0,
            0,
            true,
        )];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        let mut fb = Framebuffer::new();
        layer.composite(&mut fb);

        // Unflipped (7,0)=green is now at (0,0); unflipped (0,0)=red is now
        // at (7,0).
        assert_eq!(
            fb.pixel(0, 0),
            Some(Bgr555::from_channels(0, 0x1F, 0).to_rgb888())
        );
        assert_eq!(
            fb.pixel(7, 0),
            Some(Bgr555::from_channels(0x1F, 0, 0).to_rgb888())
        );
    }

    #[test]
    fn composite_v_flip_mirrors_the_whole_sprite_footprint() {
        let tileset = Tileset::decode(BitDepth::Bpp4, &corner_marked_tile()).unwrap();
        let palette = corner_marked_palette();
        let entries = [OamEntry::new(
            0,
            0,
            0,
            0,
            BitDepth::Bpp4,
            false,
            true, // v_flip
            ObjShape::Square,
            0,
            0,
            true,
        )];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        let mut fb = Framebuffer::new();
        layer.composite(&mut fb);

        // Unflipped (0,7)=blue is now at (0,0); unflipped (0,0)=red is now
        // at (0,7).
        assert_eq!(
            fb.pixel(0, 0),
            Some(Bgr555::from_channels(0, 0, 0x1F).to_rgb888())
        );
        assert_eq!(
            fb.pixel(0, 7),
            Some(Bgr555::from_channels(0x1F, 0, 0).to_rgb888())
        );
    }

    #[test]
    fn composite_clips_a_sprite_partially_off_the_left_edge() {
        // x=-4 means only the sprite's rightmost 4 columns (local col 4..8)
        // are on-screen, landing at framebuffer x=0..4.
        let mut bytes = [0xFFu8; 32]; // every pixel index 15 (opaque)
        bytes[0] = 0x00; // still make col0-1 of row0 transparent as a marker
        let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[15] = Bgr555::from_channels(0x1F, 0x1F, 0x1F);
        let palette = Palette::new(colors);

        let entries = [entry(0x1FC /* 508 -> -4 */, 0, true)];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        let mut fb = Framebuffer::new();
        layer.composite(&mut fb); // must not panic

        // Local col 4 (on-screen col 0) is opaque (index 15).
        assert_eq!(fb.pixel(0, 0), Some(colors[15].to_rgb888()));
        // Local col 8 would be off the 8-wide sprite; on-screen col 4 must
        // stay untouched (default black backdrop).
        assert_eq!(fb.pixel(4, 0), Some(Rgb888::BLACK));
    }

    #[test]
    fn composite_wraps_a_sprite_hanging_off_the_bottom_to_the_top() {
        // y=250 with an 8-tall sprite covers OBJ rows 250..258. Row 256
        // wraps to screen row 0, i.e. local sprite row 6 (250+6=256).
        // Screen row 0 -> dy = (0 - 250).rem_euclid(256) = 6, so an opaque
        // pixel at tile row 6 must be visible at screen row 0.
        let mut bytes = [0u8; 32];
        bytes[6 * 4] = 0x11; // tile row 6, col 0 -> index 1 (opaque)
        let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[1] = Bgr555::from_channels(0x1F, 0, 0);
        let palette = Palette::new(colors);

        let entries = [entry(0, 250, true)];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        assert_eq!(
            layer.resolve_pixel(0, 0).map(|p| p.color),
            Some(colors[1].to_rgb888())
        );
        // Screen row 5 -> dy = (5-250).rem_euclid(256) = 11, outside the
        // 8-tall footprint (>= height), so nothing is drawn there.
        assert_eq!(layer.resolve_pixel(0, 5), None);
    }

    #[test]
    fn composite_disabled_sprite_draws_nothing() {
        let tileset = Tileset::decode(BitDepth::Bpp4, &[0xFFu8; 32]).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[15] = Bgr555::from_channels(0x1F, 0x1F, 0x1F);
        let palette = Palette::new(colors);
        let entries = [entry(0, 0, false)];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        assert_eq!(layer.resolve_pixel(0, 0), None);
    }

    #[test]
    fn resolve_pixel_sprite_vs_sprite_lower_oam_index_wins_a_priority_tie() {
        let tileset = Tileset::decode(BitDepth::Bpp4, &[0xFFu8; 32]).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[15] = Bgr555::from_channels(0x1F, 0, 0); // red, bank 0
        colors[16 + 15] = Bgr555::from_channels(0, 0x1F, 0); // green, bank 1
        let palette = Palette::new(colors);

        // Both fully opaque 8x8 sprites at the same position and priority;
        // entry index 0 (red, bank 0) must win over entry index 1 (green,
        // bank 1) despite being declared first only by array position.
        let low_index_red = OamEntry::new(
            0,
            0,
            0,
            0,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            0,
            2,
            true,
        );
        let high_index_green = OamEntry::new(
            0,
            0,
            0,
            1,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            0,
            2,
            true,
        );
        let entries = [low_index_red, high_index_green];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        assert_eq!(
            layer.resolve_pixel(0, 0).map(|p| p.color),
            Some(colors[15].to_rgb888())
        );

        // Reversed array order: now the green (bank 1) entry is OAM index 0
        // and must win instead, proving the winner tracks array position,
        // not palette/color identity.
        let entries_reversed = [high_index_green, low_index_red];
        let layer_reversed = SpriteLayer::new(&entries_reversed, &tileset, &tileset, &palette);
        assert_eq!(
            layer_reversed.resolve_pixel(0, 0).map(|p| p.color),
            Some(colors[16 + 15].to_rgb888())
        );
    }

    #[test]
    fn resolve_pixel_lower_priority_number_wins_regardless_of_oam_index() {
        let tileset = Tileset::decode(BitDepth::Bpp4, &[0xFFu8; 32]).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[15] = Bgr555::from_channels(0x1F, 0, 0);
        colors[16 + 15] = Bgr555::from_channels(0, 0x1F, 0);
        let palette = Palette::new(colors);

        // Entry 0 (red) has the *worse* (higher) priority 3; entry 1
        // (green) has the better priority 0 and must win despite the
        // higher OAM index.
        let worse_priority_red = OamEntry::new(
            0,
            0,
            0,
            0,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            0,
            3,
            true,
        );
        let better_priority_green = OamEntry::new(
            0,
            0,
            0,
            1,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            0,
            0,
            true,
        );
        let entries = [worse_priority_red, better_priority_green];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        assert_eq!(
            layer.resolve_pixel(0, 0).map(|p| p.color),
            Some(colors[16 + 15].to_rgb888())
        );
    }

    #[test]
    fn multi_tile_sprite_addresses_tiles_row_major_from_its_base_index() {
        // A 16x8 (2 tiles wide, 1 tall) sprite: tile 5 (top-left, opaque
        // red) then tile 6 (top-right, opaque green), matching 1D OBJ
        // character mapping's contiguous row-major layout.
        let mut tiles = vec![0u8; 32 * 7];
        tiles[5 * 32] = 0x11; // tile 5, pixel(0,0) index 1
        tiles[6 * 32] = 0x22; // tile 6, pixel(0,0) index 2
        let tileset = Tileset::decode(BitDepth::Bpp4, &tiles).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[1] = Bgr555::from_channels(0x1F, 0, 0);
        colors[2] = Bgr555::from_channels(0, 0x1F, 0);
        let palette = Palette::new(colors);

        let entries = [OamEntry::new(
            0,
            0,
            5,
            0,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Horizontal,
            0, // 16x8
            0,
            true,
        )];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        assert_eq!(
            layer.resolve_pixel(0, 0).map(|p| p.color),
            Some(colors[1].to_rgb888())
        );
        assert_eq!(
            layer.resolve_pixel(8, 0).map(|p| p.color),
            Some(colors[2].to_rgb888())
        );
    }

    #[test]
    fn composite_8bpp_sprite_uses_the_flat_palette_and_its_own_tileset() {
        let mut bytes = [0u8; 64];
        bytes[0] = 200;
        let tileset_8bpp = Tileset::decode(BitDepth::Bpp8, &bytes).unwrap();
        let tileset_4bpp = Tileset::decode(BitDepth::Bpp4, &[0u8; 32]).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[200] = Bgr555::from_channels(0x1F, 0x10, 0);
        let palette = Palette::new(colors);

        let entries = [OamEntry::new(
            0,
            0,
            0,
            3, // palette bank bits set but must be ignored for 8bpp
            BitDepth::Bpp8,
            false,
            false,
            ObjShape::Square,
            0,
            0,
            true,
        )];
        let layer = SpriteLayer::new(&entries, &tileset_4bpp, &tileset_8bpp, &palette);

        assert_eq!(
            layer.resolve_pixel(0, 0).map(|p| p.color),
            Some(colors[200].to_rgb888())
        );
    }
}
