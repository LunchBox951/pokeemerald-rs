//! An affine (rotation/scaling, GBA modes 1/2) BG tile layer compositor
//! (S-2 slice 3).
//!
//! Ports the affine-BG semantics of `pokeemerald/src/bg.c`'s `SetBgAffine`/
//! `SetBgAffineInternal` (which packs a `BgAffineDstData` into
//! `BG2PA..PD`/`BG2X`/`BG2Y`, or the BG3 equivalents) and verifies texture
//! sampling against `mgba`'s software mode-1/2 renderer
//! (`mgba/src/gba/renderers/software-bg.c`'s `GBAVideoSoftwareRendererDrawBackgroundMode2`
//! and the `MODE_2_*` macros it expands).
//!
//! # Per-scanline reference restore, as pokeemerald actually uses it
//!
//! On hardware, `BG2X`/`BG2Y` (`sx`/`sy`) are *internal* registers: a write
//! reloads them immediately, but every scanline they otherwise accumulate by
//! `dmx`/`dmy` (`pb`/`pd`) on their own
//! (`mgba/src/gba/renderers/video-software.c:680-686`,
//! `:742-745` reloads `sx`/`sy` from the latched `refx`/`refy` once per
//! frame). `pokeemerald` never exploits mid-frame reference-point writes for
//! its affine BG usage — `SetBgAffineInternal`
//! (`pokeemerald/src/bg.c:244-282`) writes `BG2PA..PD`/`BG2X`/`BG2Y` once,
//! typically from a per-frame task, and leaves the hardware to accumulate
//! `pb`/`pd` for the rest of the frame. Under that (universal, observed)
//! usage pattern the per-scanline accumulation telescopes into one static,
//! closed-form sample per pixel:
//!
//! ```text
//! texX(x, y) = refX + pa*x + pb*y
//! texY(x, y) = refY + pc*x + pd*y
//! ```
//!
//! where `(x, y)` is the on-screen pixel and `(refX, refY)` is the reference
//! point latched at the start of the frame — exactly what
//! [`AffineBgLayer::composite`] computes per pixel, without needing to walk
//! scanlines one at a time. A game that rewrote `BG2X`/`BG2Y` mid-frame would
//! observe different behavior on real hardware; that usage is out of scope
//! here `(behavioral-fidelity)`.
//!
//! # Overflow (wrap vs. transparent)
//!
//! A texture-space sample outside the tilemap's own pixel bounds either
//! wraps (`overflow=1`, `MODE_2_COORD_OVERFLOW`: `localX = x &
//! (sizeAdjusted - 1)`) or renders transparent (`overflow=0`,
//! `MODE_2_COORD_NO_OVERFLOW`: skip the pixel if any bit outside the mask is
//! set) — see [`Overflow`].
//!
//! # 8bpp-only, no per-tile flip or palette bank
//!
//! An affine BG's screen data is a flat array of raw 8-bit tile indices
//! (`mgba`'s `mapData = screenBase[...]`, one byte per entry), not the
//! 16-bit flip/palette-bank-carrying regular-BG [`ScreenEntry`](crate::tilemap::ScreenEntry) —
//! affine BGs are always 8bpp with one flat 256-color palette, so there is no
//! per-tile palette bank to select and no per-tile flip bits (`bg.c`'s
//! affine tilemaps are `u8` arrays, unlike regular BGs' `u16` screen block
//! arrays) `(behavioral-fidelity)`. See [`AffineTilemap`].

use crate::affine::AffineMatrix;
use crate::error::RenderError;
use crate::framebuffer::Framebuffer;
use crate::palette::{Palette, Rgb888};
use crate::tile::{BitDepth, Tileset};

/// A flat grid of raw 8-bit affine-BG tile indices — the mode-1/2 analogue of
/// [`Tilemap`](crate::tilemap::Tilemap), with no per-entry flip bits or
/// palette bank (see the module docs).
#[derive(Debug, Clone)]
pub struct AffineTilemap {
    width_tiles: usize,
    height_tiles: usize,
    tile_indices: Vec<u8>,
}

impl AffineTilemap {
    /// Build an affine tilemap from a grid of `width_tiles * height_tiles`
    /// raw tile indices, row-major.
    ///
    /// Real affine-BG hardware sizes are 16x16, 32x32, 64x64, or 128x128
    /// tiles (128/256/512/1024 px square); as with
    /// [`Tilemap::new`](crate::tilemap::Tilemap::new), `width_tiles`/
    /// `height_tiles` are caller-supplied rather than restricted to those
    /// four sizes — a non-hardware size still composites and wraps
    /// correctly against its own actual pixel dimensions.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::AffineTilemapSizeMismatch`] if
    /// `tile_indices.len() != width_tiles * height_tiles`.
    pub fn new(
        width_tiles: usize,
        height_tiles: usize,
        tile_indices: Vec<u8>,
    ) -> Result<Self, RenderError> {
        let expected = width_tiles * height_tiles;
        if tile_indices.len() != expected {
            return Err(RenderError::AffineTilemapSizeMismatch {
                expected,
                actual: tile_indices.len(),
            });
        }
        Ok(Self {
            width_tiles,
            height_tiles,
            tile_indices,
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

    /// The raw tile index at logical tile coordinates `(col, row)`, or
    /// `None` if out of range.
    #[must_use]
    pub fn tile_index(&self, col: usize, row: usize) -> Option<u8> {
        if col >= self.width_tiles || row >= self.height_tiles {
            return None;
        }
        self.tile_indices.get(row * self.width_tiles + col).copied()
    }
}

/// How an affine BG samples a texture-space coordinate that falls outside
/// its tilemap's own pixel bounds (the BG control register's `overflow`/
/// `areaOver` bit).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overflow {
    /// `overflow=0`: an out-of-bounds sample is transparent (the backdrop or
    /// a lower layer shows through) — mgba's `MODE_2_COORD_NO_OVERFLOW`.
    Transparent,
    /// `overflow=1`: an out-of-bounds sample wraps modulo the tilemap's own
    /// pixel size — mgba's `MODE_2_COORD_OVERFLOW`.
    Wrap,
}

/// A single affine (mode 1/2) BG tile layer: an 8bpp [`Tileset`], a
/// [`Palette`], and an [`AffineTilemap`], sampled through an
/// [`AffineMatrix`] and a reference point.
///
/// No cross-layer priority ordering. Windows, color effects, and mosaic
/// (issue #99) are handled around this type by
/// [`compositor::BgSlot`](crate::compositor::BgSlot) — see
/// [`compositor`](crate::compositor).
#[derive(Debug, Clone, Copy)]
pub struct AffineBgLayer<'a> {
    tileset: &'a Tileset,
    palette: &'a Palette,
    tilemap: &'a AffineTilemap,
}

impl<'a> AffineBgLayer<'a> {
    /// Borrow an 8bpp tileset, palette, and affine tilemap together as one
    /// composable layer. The tileset's bit depth is not validated here — a
    /// 4bpp tileset simply resolves every pixel through
    /// [`Palette::color`](crate::palette::Palette::color) using the raw tile
    /// index as if it were an 8bpp value, which is meaningless but not
    /// unsafe; callers are expected to pass an 8bpp tileset, matching real
    /// affine-BG hardware.
    #[must_use]
    pub const fn new(
        tileset: &'a Tileset,
        palette: &'a Palette,
        tilemap: &'a AffineTilemap,
    ) -> Self {
        Self {
            tileset,
            palette,
            tilemap,
        }
    }

    /// Composite this layer into `framebuffer`, sampling every pixel through
    /// `matrix` from reference point `(ref_x, ref_y)` — both in 20.8
    /// fixed-point (28-bit signed) texture-space units, matching `BG2X`/
    /// `BG2Y`'s hardware representation — with `overflow` controlling
    /// out-of-bounds sampling (see the module docs for the closed-form
    /// per-pixel formula this implements).
    pub fn composite(
        &self,
        framebuffer: &mut Framebuffer,
        matrix: AffineMatrix,
        ref_x: i32,
        ref_y: i32,
        overflow: Overflow,
    ) {
        for y in 0..framebuffer.height() {
            for x in 0..framebuffer.width() {
                let Some(color) = self.sample_pixel(matrix, ref_x, ref_y, overflow, x, y) else {
                    continue;
                };
                framebuffer.set_pixel(x, y, color);
            }
        }
    }

    /// Sample this layer's resolved color at framebuffer coordinate
    /// `(screen_x, screen_y)`, the per-pixel primitive behind
    /// [`composite`](Self::composite), shared with the cross-layer
    /// [priority compositor](crate::compositor).
    ///
    /// Returns `None` if this layer has an empty tilemap, the sample falls
    /// outside the tilemap under [`Overflow::Transparent`], or the resolved
    /// pixel is transparent (palette index 0).
    ///
    /// `screen_x`/`screen_y` are always framebuffer coordinates (`<240`,
    /// `<160`) and this layer's tilemap is always far smaller than `i32`'s
    /// range in practice, so the `i32` round-trips below never truncate,
    /// wrap, or lose their sign in practice — the `#[allow]` documents that,
    /// matching sprite.rs's `sample_entry` convention.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss
    )]
    pub(crate) fn sample_pixel(
        &self,
        matrix: AffineMatrix,
        ref_x: i32,
        ref_y: i32,
        overflow: Overflow,
        screen_x: usize,
        screen_y: usize,
    ) -> Option<Rgb888> {
        const DIM: usize = BitDepth::TILE_DIM;
        let bg_width_px = self.tilemap.width_tiles() * DIM;
        let bg_height_px = self.tilemap.height_tiles() * DIM;
        if bg_width_px == 0 || bg_height_px == 0 {
            return None;
        }

        let (dx, dy) = (screen_x as i32, screen_y as i32);
        let (tx, ty) = matrix.apply(dx, dy);
        // >> 8 truncates the 8.8 fixed-point product toward negative
        // infinity, matching the GBA's arithmetic right shift on a signed
        // accumulator (mgba's `x >> 8` on an `int32_t`).
        let tex_x = (ref_x.wrapping_add(tx)) >> AffineMatrix::FRAC_BITS;
        let tex_y = (ref_y.wrapping_add(ty)) >> AffineMatrix::FRAC_BITS;

        let (w, h) = (bg_width_px as i32, bg_height_px as i32);
        let (px, py) = match overflow {
            Overflow::Wrap => (tex_x.rem_euclid(w), tex_y.rem_euclid(h)),
            Overflow::Transparent => {
                if tex_x < 0 || tex_y < 0 || tex_x >= w || tex_y >= h {
                    return None;
                }
                (tex_x, tex_y)
            }
        };
        let (px, py) = (px as usize, py as usize);

        let tile_index = self.tilemap.tile_index(px / DIM, py / DIM)?;
        let tile = self.tileset.tile(u16::from(tile_index))?;
        let index = tile.index(px % DIM, py % DIM);
        if index == 0 {
            return None;
        }
        Some(self.palette.color(index).to_rgb888())
    }
}

#[cfg(test)]
mod tests {
    use super::{AffineBgLayer, AffineTilemap, Overflow};
    use crate::affine::AffineMatrix;
    use crate::bg::BgLayer;
    use crate::framebuffer::Framebuffer;
    use crate::palette::{Bgr555, Palette};
    use crate::tile::{BitDepth, Tileset};
    use crate::tilemap::{ScreenEntry, Tilemap};

    /// A 2x2-tile (16x16px) affine tilemap: tile `n` at `(col, row)` reads
    /// raw index `n`, so tile 0 is top-left, tile 1 top-right, tile 2
    /// bottom-left, tile 3 bottom-right.
    fn marked_2x2_affine_tilemap() -> AffineTilemap {
        AffineTilemap::new(2, 2, vec![0, 1, 2, 3]).unwrap()
    }

    /// A 4-tile 8bpp tileset where tile `n`'s every pixel is opaque flat
    /// palette index `n + 1` (so index 0, transparent, is never emitted),
    /// paired with a palette mapping index `n+1` to a distinct color.
    fn marked_8bpp_tileset_and_palette() -> (Tileset, Palette) {
        let mut bytes = [0u8; 64 * 4];
        for (tile, chunk) in bytes.chunks_exact_mut(64).enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            chunk.fill(tile as u8 + 1);
        }
        let tileset = Tileset::decode(BitDepth::Bpp8, &bytes).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[1] = Bgr555::from_channels(0x1F, 0, 0); // tile 0: red
        colors[2] = Bgr555::from_channels(0, 0x1F, 0); // tile 1: green
        colors[3] = Bgr555::from_channels(0, 0, 0x1F); // tile 2: blue
        colors[4] = Bgr555::from_channels(0x1F, 0x1F, 0); // tile 3: yellow
        (tileset, Palette::new(colors))
    }

    #[test]
    fn identity_matrix_reproduces_the_non_affine_render_byte_for_byte() {
        // Build the same 2x2-tile pattern as both an affine tilemap (via
        // AffineBgLayer) and a regular one (via BgLayer, 8bpp, no flip, bank
        // ignored) and confirm identity-matrix affine sampling at reference
        // point (0,0) is pixel-identical to the unscrolled regular render.
        let (tileset, palette) = marked_8bpp_tileset_and_palette();
        let affine_tilemap = marked_2x2_affine_tilemap();
        let affine_layer = AffineBgLayer::new(&tileset, &palette, &affine_tilemap);

        let entries = vec![
            ScreenEntry::new(0, false, false, 0),
            ScreenEntry::new(1, false, false, 0),
            ScreenEntry::new(2, false, false, 0),
            ScreenEntry::new(3, false, false, 0),
        ];
        let regular_tilemap = Tilemap::new(2, 2, entries).unwrap();
        let regular_layer = BgLayer::new(&tileset, &palette, &regular_tilemap);

        let mut affine_fb = Framebuffer::new();
        affine_layer.composite(
            &mut affine_fb,
            AffineMatrix::IDENTITY,
            0,
            0,
            Overflow::Transparent,
        );
        let mut regular_fb = Framebuffer::new();
        regular_layer.composite(&mut regular_fb);

        assert_eq!(affine_fb.pixels(), regular_fb.pixels());
        // Sanity-check a couple of the actual colors, not just that the two
        // renders agree with each other.
        assert_eq!(
            affine_fb.pixel(0, 0),
            Some(Bgr555::from_channels(0x1F, 0, 0).to_rgb888())
        );
        assert_eq!(
            affine_fb.pixel(15, 15),
            Some(Bgr555::from_channels(0x1F, 0x1F, 0).to_rgb888())
        );
    }

    #[test]
    fn pure_scale_samples_texture_space_at_twice_the_rate() {
        // pa=2.0, pd=2.0 (pb=pc=0): screen pixel (x,y) samples texture pixel
        // (2x, 2y), so tile boundaries land at half the on-screen spacing.
        // Screen (0,0) -> tex (0,0) -> tile 0 (red). Screen (4,0) -> tex
        // (8,0) -> tile 1's local column 0, i.e. still tile 1 (green) since
        // tex x=8 is the first column of the second 8px tile.
        let (tileset, palette) = marked_8bpp_tileset_and_palette();
        let tilemap = marked_2x2_affine_tilemap();
        let layer = AffineBgLayer::new(&tileset, &palette, &tilemap);
        let matrix = AffineMatrix::new(512, 0, 0, 512); // 2.0 in 8.8 fixed point

        let mut fb = Framebuffer::new();
        layer.composite(&mut fb, matrix, 0, 0, Overflow::Transparent);

        assert_eq!(
            fb.pixel(0, 0),
            Some(Bgr555::from_channels(0x1F, 0, 0).to_rgb888()), // tile 0 red
        );
        assert_eq!(
            fb.pixel(4, 0),
            Some(Bgr555::from_channels(0, 0x1F, 0).to_rgb888()), // tile 1 green
        );
        assert_eq!(
            fb.pixel(0, 4),
            Some(Bgr555::from_channels(0, 0, 0x1F).to_rgb888()), // tile 2 blue
        );
    }

    #[test]
    fn pure_90_degree_rotation_matches_hand_computed_sampling() {
        // A 90-degree rotation matrix (pa=0, pb=-1.0, pc=1.0, pd=0): texture
        // delta (tx,ty) = (pa*dx+pb*dy, pc*dx+pd*dy) = (-dy, dx). With
        // reference point (8, 0) (the tilemap's horizontal center), screen
        // (0,0) has (dx,dy)=(0,0) -> tex (8,0) -> tile 1 (green, top-right).
        // Screen (0,8) has (dx,dy)=(0,8) -> texture delta (-8, 0) -> tex
        // (0,0) -> tile 0 (red, top-left).
        let (tileset, palette) = marked_8bpp_tileset_and_palette();
        let tilemap = marked_2x2_affine_tilemap();
        let layer = AffineBgLayer::new(&tileset, &palette, &tilemap);
        let matrix = AffineMatrix::new(0, -256, 256, 0);

        let mut fb = Framebuffer::new();
        layer.composite(&mut fb, matrix, 8 * 256, 0, Overflow::Transparent);

        assert_eq!(
            fb.pixel(0, 0),
            Some(Bgr555::from_channels(0, 0x1F, 0).to_rgb888()), // tile 1 green
        );
        assert_eq!(
            fb.pixel(0, 8),
            Some(Bgr555::from_channels(0x1F, 0, 0).to_rgb888()), // tile 0 red
        );
    }

    #[test]
    fn overflow_transparent_leaves_out_of_bounds_samples_untouched() {
        let (tileset, palette) = marked_8bpp_tileset_and_palette();
        let tilemap = marked_2x2_affine_tilemap();
        let layer = AffineBgLayer::new(&tileset, &palette, &tilemap);

        // Reference point far outside the 16x16px tilemap: every sample is
        // out of bounds.
        let mut fb = Framebuffer::new();
        let backdrop = crate::palette::Rgb888 { r: 5, g: 6, b: 7 };
        fb.fill(backdrop);
        layer.composite(
            &mut fb,
            AffineMatrix::IDENTITY,
            1000 * 256,
            0,
            Overflow::Transparent,
        );

        assert_eq!(fb.pixel(0, 0), Some(backdrop));
    }

    #[test]
    fn overflow_wrap_samples_modulo_the_tilemap_pixel_size() {
        let (tileset, palette) = marked_8bpp_tileset_and_palette();
        let tilemap = marked_2x2_affine_tilemap();
        let layer = AffineBgLayer::new(&tileset, &palette, &tilemap);

        // Reference point one full tilemap width (16px) to the right wraps
        // back to (0,0), so this must match the identity/zero-reference
        // render exactly.
        let mut wrapped = Framebuffer::new();
        layer.composite(
            &mut wrapped,
            AffineMatrix::IDENTITY,
            16 * 256,
            0,
            Overflow::Wrap,
        );
        let mut zero = Framebuffer::new();
        layer.composite(&mut zero, AffineMatrix::IDENTITY, 0, 0, Overflow::Wrap);

        assert_eq!(wrapped.pixels(), zero.pixels());
    }

    #[test]
    fn overflow_wrap_vs_transparent_differ_at_the_same_out_of_bounds_sample() {
        let (tileset, palette) = marked_8bpp_tileset_and_palette();
        let tilemap = marked_2x2_affine_tilemap();
        let layer = AffineBgLayer::new(&tileset, &palette, &tilemap);

        // Reference point one tilemap width to the right: transparent
        // overflow shows the backdrop; wrap overflow shows tile 0's color
        // (wraps back to the map's own origin).
        let mut transparent_fb = Framebuffer::new();
        let backdrop = crate::palette::Rgb888 { r: 9, g: 9, b: 9 };
        transparent_fb.fill(backdrop);
        layer.composite(
            &mut transparent_fb,
            AffineMatrix::IDENTITY,
            16 * 256,
            0,
            Overflow::Transparent,
        );
        let mut wrap_fb = Framebuffer::new();
        layer.composite(
            &mut wrap_fb,
            AffineMatrix::IDENTITY,
            16 * 256,
            0,
            Overflow::Wrap,
        );

        assert_eq!(transparent_fb.pixel(0, 0), Some(backdrop));
        assert_eq!(
            wrap_fb.pixel(0, 0),
            Some(Bgr555::from_channels(0x1F, 0, 0).to_rgb888())
        );
    }

    #[test]
    fn affine_tilemap_new_rejects_tile_index_count_mismatch() {
        assert_eq!(
            AffineTilemap::new(2, 2, vec![0, 1, 2]).unwrap_err(),
            crate::error::RenderError::AffineTilemapSizeMismatch {
                expected: 4,
                actual: 3,
            }
        );
    }

    #[test]
    fn affine_tilemap_entry_out_of_range_is_none() {
        let tilemap = marked_2x2_affine_tilemap();
        assert!(tilemap.tile_index(2, 0).is_none());
        assert!(tilemap.tile_index(0, 2).is_none());
        assert_eq!(tilemap.tile_index(1, 1), Some(3));
    }

    #[test]
    fn sample_pixel_skips_tile_index_0_transparent_pixels() {
        // A tileset whose tile 0 is fully transparent (index 0) and a
        // 1x1-tile affine tilemap referencing it.
        let tileset = Tileset::decode(BitDepth::Bpp8, &[0u8; 64]).unwrap();
        let palette = Palette::new([Bgr555::default(); Palette::LEN]);
        let tilemap = AffineTilemap::new(1, 1, vec![0]).unwrap();
        let layer = AffineBgLayer::new(&tileset, &palette, &tilemap);

        let mut fb = Framebuffer::new();
        let backdrop = crate::palette::Rgb888 { r: 1, g: 2, b: 3 };
        fb.fill(backdrop);
        layer.composite(&mut fb, AffineMatrix::IDENTITY, 0, 0, Overflow::Transparent);

        assert_eq!(fb.pixel(0, 0), Some(backdrop));
    }
}
