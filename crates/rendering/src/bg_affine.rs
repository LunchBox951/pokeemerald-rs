//! Affine background tilemaps and texture-space sampling.
//!
//! [`AffineBgLayer`] accepts one reference point for the whole frame. Pokémon
//! Emerald writes the affine parameters once per frame
//! (`pokeemerald/src/bg.c:244-282`), while mGBA reloads the latched reference
//! once per frame and advances it per scanline
//! (`mgba/src/gba/renderers/video-software.c:680-686,742-745`). The static
//! per-pixel transform matches that usage; mid-frame reference writes are not
//! modeled.

use crate::affine::AffineMatrix;
use crate::error::RenderError;
use crate::framebuffer::Framebuffer;
use crate::palette::{Palette, Rgb888};
use crate::tile::{BitDepth, Tileset};

/// A row-major grid of one-byte affine-background tile indices.
///
/// Entries have no flip bits or palette bank.
#[derive(Debug, Clone)]
pub struct AffineTilemap {
    width_tiles: usize,
    height_tiles: usize,
    tile_indices: Vec<u8>,
}

impl AffineTilemap {
    /// Builds a row-major tilemap with caller-defined dimensions.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::AffineTilemapDimensionsInvalid`] when the tile
    /// area overflows `usize`, or when either axis's pixel extent
    /// (`width_tiles` or `height_tiles` times the tile side length) leaves the
    /// signed texture coordinates [`AffineBgLayer`] samples in, which would
    /// make every sample of an accepted map transparent. Returns
    /// [`RenderError::AffineTilemapSizeMismatch`] unless the tile-index count
    /// equals `width_tiles * height_tiles`.
    pub fn new(
        width_tiles: usize,
        height_tiles: usize,
        tile_indices: Vec<u8>,
    ) -> Result<Self, RenderError> {
        let dimensions_invalid = || RenderError::AffineTilemapDimensionsInvalid {
            width_tiles,
            height_tiles,
        };
        let expected = width_tiles
            .checked_mul(height_tiles)
            .ok_or_else(dimensions_invalid)?;
        sampled_pixel_extent(width_tiles).ok_or_else(dimensions_invalid)?;
        sampled_pixel_extent(height_tiles).ok_or_else(dimensions_invalid)?;
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

    /// Returns the width in tiles.
    #[must_use]
    pub const fn width_tiles(&self) -> usize {
        self.width_tiles
    }

    /// Returns the height in tiles.
    #[must_use]
    pub const fn height_tiles(&self) -> usize {
        self.height_tiles
    }

    /// Returns the tile index at `(column, row)`, or `None` when out of bounds.
    #[must_use]
    pub fn tile_index(&self, column: usize, row: usize) -> Option<u8> {
        if column >= self.width_tiles || row >= self.height_tiles {
            return None;
        }
        self.tile_indices
            .get(row * self.width_tiles + column)
            .copied()
    }
}

/// Returns an axis's pixel extent in the signed texture coordinates
/// [`AffineBgLayer`] samples in, or `None` when `tiles` leaves that range.
fn sampled_pixel_extent(tiles: usize) -> Option<i32> {
    i32::try_from(tiles.checked_mul(BitDepth::TILE_DIM)?).ok()
}

/// How samples outside an affine background's texture bounds are handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overflow {
    /// Leaves the destination pixel unchanged.
    Transparent,
    /// Wraps the coordinate modulo the texture dimensions.
    Wrap,
}

/// An affine background layer of 8bpp tiles, a palette, and a tilemap.
#[derive(Debug, Clone, Copy)]
pub struct AffineBgLayer<'a> {
    tileset: &'a Tileset,
    palette: &'a Palette,
    tilemap: &'a AffineTilemap,
}

impl<'a> AffineBgLayer<'a> {
    /// Borrows the resources for an affine background layer.
    ///
    /// The caller must supply an 8bpp tileset; its bit depth is not validated.
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

    /// Composites the transformed layer into `framebuffer`.
    ///
    /// `reference_x` and `reference_y` are signed 20.8 fixed-point texture
    /// coordinates. `matrix` contains signed 8.8 fixed-point coefficients.
    pub fn composite(
        &self,
        framebuffer: &mut Framebuffer,
        matrix: AffineMatrix,
        reference_x: i32,
        reference_y: i32,
        overflow: Overflow,
    ) {
        for screen_y in 0..framebuffer.height() {
            for screen_x in 0..framebuffer.width() {
                let Some(color) = self.sample_pixel(
                    matrix,
                    reference_x,
                    reference_y,
                    overflow,
                    screen_x,
                    screen_y,
                ) else {
                    continue;
                };
                framebuffer.set_pixel(screen_x, screen_y, color);
            }
        }
    }

    #[must_use]
    pub(crate) fn sample_pixel(
        &self,
        matrix: AffineMatrix,
        reference_x: i32,
        reference_y: i32,
        overflow: Overflow,
        screen_x: usize,
        screen_y: usize,
    ) -> Option<Rgb888> {
        let (sample_x, sample_y) = self.sampled_coordinate(
            matrix,
            reference_x,
            reference_y,
            overflow,
            screen_x,
            screen_y,
        )?;
        self.sample_texel(sample_x, sample_y)
    }

    /// Resolves `screen_x`/`screen_y` to a texture-space coordinate, or
    /// `None` when the texture is degenerate/unsampleable, or (under
    /// `Overflow::Transparent`) the transformed coordinate itself falls
    /// outside the texture. [`Overflow::Wrap`] never returns `None` once the
    /// texture itself is sampleable.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        reason = "framebuffer dimensions and screen coordinates fit in i32, and sampled pixels are nonnegative"
    )]
    fn sampled_coordinate(
        &self,
        matrix: AffineMatrix,
        reference_x: i32,
        reference_y: i32,
        overflow: Overflow,
        screen_x: usize,
        screen_y: usize,
    ) -> Option<(usize, usize)> {
        let width_tiles = self.tilemap.width_tiles();
        let height_tiles = self.tilemap.height_tiles();
        if width_tiles == 0 || height_tiles == 0 {
            return None;
        }
        let texture_width = sampled_pixel_extent(width_tiles)?;
        let texture_height = sampled_pixel_extent(height_tiles)?;

        let (tx, ty) = matrix.apply(screen_x as i32, screen_y as i32);
        let tex_x = reference_x.wrapping_add(tx) >> AffineMatrix::FRAC_BITS;
        let tex_y = reference_y.wrapping_add(ty) >> AffineMatrix::FRAC_BITS;

        let (sample_x, sample_y) = match overflow {
            Overflow::Wrap => (
                tex_x.rem_euclid(texture_width),
                tex_y.rem_euclid(texture_height),
            ),
            Overflow::Transparent => {
                if tex_x < 0 || tex_y < 0 || tex_x >= texture_width || tex_y >= texture_height {
                    return None;
                }
                (tex_x, tex_y)
            }
        };
        Some((sample_x as usize, sample_y as usize))
    }

    /// Resolves an already-in-bounds texture coordinate to a color, or `None`
    /// for a palette-index-0 (transparent) texel.
    fn sample_texel(&self, sample_x: usize, sample_y: usize) -> Option<Rgb888> {
        let tile_index = self
            .tilemap
            .tile_index(sample_x / BitDepth::TILE_DIM, sample_y / BitDepth::TILE_DIM)?;
        let tile = self.tileset.tile(u16::from(tile_index))?;
        let palette_index =
            tile.index(sample_x % BitDepth::TILE_DIM, sample_y % BitDepth::TILE_DIM);
        if palette_index == 0 {
            return None;
        }
        Some(self.palette.color(palette_index).to_rgb888())
    }

    /// Advances `hold` by one column and returns that column's affine
    /// `Overflow::Transparent` mosaic sample, replacing the stateless
    /// block-origin snap [`crate::mosaic::MosaicSize::snap`] otherwise
    /// applies with mGBA's per-column retry/hold state machine.
    ///
    /// `screen_y` is already snapped to the mosaic block's top row: vertical
    /// mosaic has no retry quirk to model, since mGBA backs the affine
    /// reference point up by `inY % mosaicV` before the scanline starts
    /// (`mgba/src/gba/renderers/software-private.h:186-191`), equivalent for
    /// this crate's static per-pixel transform to sampling at the block's
    /// top-row `y`.
    ///
    /// Within one open [`AffineMosaicHold`] span: mGBA's mode-2 affine
    /// renderer advances its raw texture coordinate every screen column
    /// (`mgba/src/gba/renderers/software-bg.c:44-53`). When "no overflow"
    /// rejects that coordinate, the fetch's `continue` skips both the
    /// composite and the `mosaicWait` reload
    /// (`MODE_2_COORD_NO_OVERFLOW`/`MODE_2_MOSAIC`,
    /// `mgba/src/gba/renderers/software-bg.c:24-42`), so the very next
    /// column retries — an out-of-bounds block origin blanks only the
    /// columns up to but excluding the first successful fetch, which draws
    /// immediately and then holds, not the whole block. The reload
    /// (`mosaicWait = mosaicH`) sits inside that same fetch, before the
    /// later `pixelData` write test
    /// (`mgba/src/gba/renderers/software-bg.c:36-42,49-53`), so it applies
    /// even when the fetched texel is itself palette-index-0 (transparent):
    /// the reload depends only on the coordinate being accepted, not on
    /// what it draws `(behavioral-fidelity)`.
    ///
    /// See [`AffineMosaicHold`]'s docs for what "one open span" means and why
    /// a caller can't just run this over every column of a scanline
    /// unconditionally.
    ///
    /// A `block_h` of 1 or 2 skips the retry/hold state machine entirely and
    /// samples every column directly: mGBA decodes the register's block size
    /// and immediately decrements it (`mgba/src/gba/renderers/software-private.h:181-185`),
    /// then only engages its mode-2 mosaic fetch/hold macro when that
    /// decremented value exceeds 1 -- i.e. only for a decoded block size of
    /// 3 or more; sizes 1 and 2 take the plain per-pixel
    /// `MODE_2_NO_MOSAIC` branch instead, with no hold at all
    /// (`mgba/src/gba/renderers/software-bg.c:56-76`)
    /// `(behavioral-fidelity)`.
    #[must_use]
    #[expect(
        clippy::too_many_arguments,
        reason = "mirrors the affine BG's full per-frame register set plus the caller's per-column position and hold state"
    )]
    pub(crate) fn sample_column_with_mosaic_hold(
        &self,
        hold: &mut AffineMosaicHold,
        matrix: AffineMatrix,
        reference_x: i32,
        reference_y: i32,
        screen_x: usize,
        screen_y: usize,
        block_h: u8,
    ) -> Option<Rgb888> {
        if block_h <= 2 {
            return self.sample_pixel(
                matrix,
                reference_x,
                reference_y,
                Overflow::Transparent,
                screen_x,
                screen_y,
            );
        }
        let block_h = usize::from(block_h);
        if !hold.span_open {
            // A fresh span (scanline start, or the first column after a
            // window-closed gap) starts exactly like a fresh mGBA renderer
            // invocation: `mosaicWait` is re-derived from this column's
            // absolute position, not carried over from before the gap
            // (`mgba/src/gba/renderers/software-private.h:181-183`).
            let phase = screen_x % block_h;
            #[expect(
                clippy::cast_possible_truncation,
                reason = "phase < block_h, and block_h originated from a u8"
            )]
            {
                hold.remaining = if phase == 0 {
                    0
                } else {
                    (block_h - phase) as u8
                };
            }
            hold.held = None;
            hold.span_open = true;
        }
        if hold.remaining == 0 {
            let (sample_x, sample_y) = self.sampled_coordinate(
                matrix,
                reference_x,
                reference_y,
                Overflow::Transparent,
                screen_x,
                screen_y,
            )?;
            hold.held = self.sample_texel(sample_x, sample_y);
            #[expect(
                clippy::cast_possible_truncation,
                reason = "block_h originated from a u8, so block_h - 1 fits back in one"
            )]
            {
                hold.remaining = (block_h - 1) as u8;
            }
        } else {
            hold.remaining -= 1;
        }
        hold.held
    }

    /// Runs [`Self::sample_column_with_mosaic_hold`] across one whole,
    /// uninterrupted scanline (no window gaps) — a convenience for callers
    /// (and tests) that don't need [`AffineMosaicHold`]'s span-close
    /// tracking.
    #[cfg(test)]
    #[must_use]
    fn sample_row_with_mosaic_hold(
        &self,
        matrix: AffineMatrix,
        reference_x: i32,
        reference_y: i32,
        screen_y: usize,
        width: usize,
        block_h: u8,
    ) -> Vec<Option<Rgb888>> {
        let mut hold = AffineMosaicHold::default();
        (0..width)
            .map(|screen_x| {
                self.sample_column_with_mosaic_hold(
                    &mut hold,
                    matrix,
                    reference_x,
                    reference_y,
                    screen_x,
                    screen_y,
                    block_h,
                )
            })
            .collect()
    }
}

/// Per-scanline retry/hold state for
/// [`AffineBgLayer::sample_column_with_mosaic_hold`] (issue #872): how many
/// more columns to hold the last-fetched sample, what that sample was, and
/// whether the current run of columns is contiguous with the last one this
/// state advanced through.
///
/// A "span" is a run of columns [`AffineMosaicHold`] has advanced through
/// without a [`Self::close`] in between. mGBA's software renderer processes
/// one scanline as a sequence of hardware-window regions, re-invoking its
/// mode-2 background draw routine (and re-deriving `mosaicWait` fresh from
/// that region's own start column) once per region in which the layer is
/// enabled (`mgba/src/gba/renderers/video-software.c:628-675`,
/// `mgba/src/gba/renderers/software-private.h:173-192`) — so retry/hold
/// state never survives a column this slot's window enable bit excluded.
/// [`Self::close`], called for every excluded column, is what lets the next
/// included column detect that and start a fresh span instead of continuing
/// the old one `(behavioral-fidelity)`.
///
/// Not modeled: mGBA still starts a fresh invocation, and so a fresh span,
/// at a window*-region* boundary even where this slot stays enabled on both
/// sides of it (e.g. `WIN0` and `WINOUT` both enabling the same BG). This
/// type only detects a *disabled* gap, since that's what
/// [`crate::window::WindowConfig::classify_with_region`] exposes per pixel;
/// a same-enabled region boundary is a narrower, undetected case left for a
/// follow-up.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct AffineMosaicHold {
    remaining: u8,
    held: Option<Rgb888>,
    span_open: bool,
}

impl AffineMosaicHold {
    /// Marks the current span closed: this column is excluded (e.g. by a
    /// hardware window), so it draws nothing here, and the next column that
    /// resumes sampling must start a fresh span rather than continue this
    /// one. See the type docs for why.
    pub(crate) fn close(&mut self) {
        self.span_open = false;
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

    const TILEMAP_WIDTH_TILES: usize = 2;
    const TILEMAP_HEIGHT_TILES: usize = 2;
    const TILE_COUNT: usize = TILEMAP_WIDTH_TILES * TILEMAP_HEIGHT_TILES;
    const DISTANT_REFERENCE_PIXELS: usize = 1_000;
    const CHANNEL_MAX: u8 = 0b1_1111;
    const RED: Bgr555 = Bgr555::from_channels(CHANNEL_MAX, 0, 0);
    const GREEN: Bgr555 = Bgr555::from_channels(0, CHANNEL_MAX, 0);
    const BLUE: Bgr555 = Bgr555::from_channels(0, 0, CHANNEL_MAX);
    const YELLOW: Bgr555 = Bgr555::from_channels(CHANNEL_MAX, CHANNEL_MAX, 0);
    const TILE_COLORS: [Bgr555; TILE_COUNT] = [RED, GREEN, BLUE, YELLOW];

    fn marked_2x2_affine_tilemap() -> AffineTilemap {
        AffineTilemap::new(TILEMAP_WIDTH_TILES, TILEMAP_HEIGHT_TILES, vec![0, 1, 2, 3]).unwrap()
    }

    fn marked_8bpp_tileset_and_palette() -> (Tileset, Palette) {
        let mut bytes = [0u8; BitDepth::Bpp8.tile_byte_len() * TILE_COUNT];
        let tile_byte_len = BitDepth::Bpp8.tile_byte_len();
        let mut colors = [Bgr555::default(); Palette::LEN];
        for (tile_position, chunk) in bytes.chunks_exact_mut(tile_byte_len).enumerate() {
            let palette_index = u8::try_from(tile_position + 1).unwrap();
            chunk.fill(palette_index);
            colors[usize::from(palette_index)] = TILE_COLORS[tile_position];
        }
        let tileset = Tileset::decode(BitDepth::Bpp8, &bytes).unwrap();
        (tileset, Palette::new(colors))
    }

    fn pixels_to_fixed(pixels: usize) -> i32 {
        i32::try_from(pixels).unwrap() * i32::from(AffineMatrix::ONE)
    }

    fn tilemap_width_fixed() -> i32 {
        pixels_to_fixed(TILEMAP_WIDTH_TILES * BitDepth::TILE_DIM)
    }

    #[test]
    fn identity_matrix_reproduces_the_non_affine_render_byte_for_byte() {
        let (tileset, palette) = marked_8bpp_tileset_and_palette();
        let affine_tilemap = marked_2x2_affine_tilemap();
        let affine_layer = AffineBgLayer::new(&tileset, &palette, &affine_tilemap);

        let entries = vec![
            ScreenEntry::new(0, false, false, 0),
            ScreenEntry::new(1, false, false, 0),
            ScreenEntry::new(2, false, false, 0),
            ScreenEntry::new(3, false, false, 0),
        ];
        let regular_tilemap =
            Tilemap::new(TILEMAP_WIDTH_TILES, TILEMAP_HEIGHT_TILES, entries).unwrap();
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
        assert_eq!(affine_fb.pixel(0, 0), Some(RED.to_rgb888()));
        let bottom_right = TILEMAP_WIDTH_TILES * BitDepth::TILE_DIM - 1;
        assert_eq!(
            affine_fb.pixel(bottom_right, bottom_right),
            Some(YELLOW.to_rgb888())
        );
    }

    #[test]
    fn pure_scale_samples_texture_space_at_twice_the_rate() {
        let (tileset, palette) = marked_8bpp_tileset_and_palette();
        let tilemap = marked_2x2_affine_tilemap();
        let layer = AffineBgLayer::new(&tileset, &palette, &tilemap);
        let doubled = 2 * AffineMatrix::ONE;
        let matrix = AffineMatrix::new(doubled, 0, 0, doubled);
        let half_tile = BitDepth::TILE_DIM / 2;

        let mut fb = Framebuffer::new();
        layer.composite(&mut fb, matrix, 0, 0, Overflow::Transparent);

        assert_eq!(fb.pixel(0, 0), Some(RED.to_rgb888()));
        assert_eq!(fb.pixel(half_tile, 0), Some(GREEN.to_rgb888()));
        assert_eq!(fb.pixel(0, half_tile), Some(BLUE.to_rgb888()));
    }

    #[test]
    fn pure_90_degree_rotation_matches_hand_computed_sampling() {
        let (tileset, palette) = marked_8bpp_tileset_and_palette();
        let tilemap = marked_2x2_affine_tilemap();
        let layer = AffineBgLayer::new(&tileset, &palette, &tilemap);
        let matrix = AffineMatrix::new(0, -AffineMatrix::ONE, AffineMatrix::ONE, 0);
        let tile_width = BitDepth::TILE_DIM;

        let mut fb = Framebuffer::new();
        layer.composite(
            &mut fb,
            matrix,
            pixels_to_fixed(tile_width),
            0,
            Overflow::Transparent,
        );

        assert_eq!(fb.pixel(0, 0), Some(GREEN.to_rgb888()));
        assert_eq!(fb.pixel(0, tile_width), Some(RED.to_rgb888()));
    }

    #[test]
    fn overflow_transparent_leaves_out_of_bounds_samples_untouched() {
        let (tileset, palette) = marked_8bpp_tileset_and_palette();
        let tilemap = marked_2x2_affine_tilemap();
        let layer = AffineBgLayer::new(&tileset, &palette, &tilemap);

        let mut fb = Framebuffer::new();
        let backdrop = crate::palette::Rgb888 { r: 5, g: 6, b: 7 };
        fb.fill(backdrop);
        layer.composite(
            &mut fb,
            AffineMatrix::IDENTITY,
            pixels_to_fixed(DISTANT_REFERENCE_PIXELS),
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

        let mut wrapped = Framebuffer::new();
        layer.composite(
            &mut wrapped,
            AffineMatrix::IDENTITY,
            tilemap_width_fixed(),
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

        let mut transparent_fb = Framebuffer::new();
        let backdrop = crate::palette::Rgb888 { r: 9, g: 9, b: 9 };
        transparent_fb.fill(backdrop);
        layer.composite(
            &mut transparent_fb,
            AffineMatrix::IDENTITY,
            tilemap_width_fixed(),
            0,
            Overflow::Transparent,
        );
        let mut wrap_fb = Framebuffer::new();
        layer.composite(
            &mut wrap_fb,
            AffineMatrix::IDENTITY,
            tilemap_width_fixed(),
            0,
            Overflow::Wrap,
        );

        assert_eq!(transparent_fb.pixel(0, 0), Some(backdrop));
        assert_eq!(wrap_fb.pixel(0, 0), Some(RED.to_rgb888()));
    }

    #[test]
    fn affine_tilemap_new_rejects_tile_index_count_mismatch() {
        let too_few_indices = vec![0, 1, 2];
        assert_eq!(
            AffineTilemap::new(TILEMAP_WIDTH_TILES, TILEMAP_HEIGHT_TILES, too_few_indices)
                .unwrap_err(),
            crate::error::RenderError::AffineTilemapSizeMismatch {
                expected: TILE_COUNT,
                actual: TILE_COUNT - 1,
            }
        );
    }

    #[test]
    fn affine_tilemap_new_returns_an_error_when_the_area_overflows() {
        assert_eq!(
            AffineTilemap::new(usize::MAX, 2, Vec::new()).unwrap_err(),
            crate::error::RenderError::AffineTilemapDimensionsInvalid {
                width_tiles: usize::MAX,
                height_tiles: 2,
            }
        );
    }

    #[test]
    fn affine_tilemap_new_returns_an_error_when_a_pixel_extent_overflows() {
        let overflowing_pixel_width_tiles = usize::MAX / BitDepth::TILE_DIM + 1;
        for (width_tiles, height_tiles) in [
            (overflowing_pixel_width_tiles, 0),
            (0, overflowing_pixel_width_tiles),
        ] {
            assert_eq!(
                AffineTilemap::new(width_tiles, height_tiles, Vec::new()).unwrap_err(),
                crate::error::RenderError::AffineTilemapDimensionsInvalid {
                    width_tiles,
                    height_tiles,
                },
                "({width_tiles}, {height_tiles}) should have been rejected"
            );
        }
    }

    #[test]
    fn affine_tilemap_new_returns_an_error_when_a_pixel_extent_is_unsampleable() {
        // 2^28 tiles need only a 256 MiB tile-index vec, so the area and its pixel extent both
        // fit `usize`; the extent is 2^31 pixels, one past the coordinates sampling addresses.
        let unsampleable_pixel_width_tiles = 1 << 28;
        for (width_tiles, height_tiles) in [
            (unsampleable_pixel_width_tiles, 1),
            (1, unsampleable_pixel_width_tiles),
        ] {
            assert_eq!(
                AffineTilemap::new(width_tiles, height_tiles, Vec::new()).unwrap_err(),
                crate::error::RenderError::AffineTilemapDimensionsInvalid {
                    width_tiles,
                    height_tiles,
                },
                "({width_tiles}, {height_tiles}) should have been rejected"
            );
        }
    }

    #[test]
    fn affine_tilemap_new_allows_zero_area_when_the_pixel_extent_fits() {
        let max_pixel_width_tiles = usize::try_from(i32::MAX).unwrap() / BitDepth::TILE_DIM;
        for (width_tiles, height_tiles) in [(max_pixel_width_tiles, 0), (0, max_pixel_width_tiles)]
        {
            let tilemap = AffineTilemap::new(width_tiles, height_tiles, Vec::new()).unwrap();
            assert!(tilemap.tile_index(0, 0).is_none());
        }
    }

    #[test]
    fn affine_tilemap_entry_out_of_range_is_none() {
        let tilemap = marked_2x2_affine_tilemap();
        assert!(tilemap.tile_index(TILEMAP_WIDTH_TILES, 0).is_none());
        assert!(tilemap.tile_index(0, TILEMAP_HEIGHT_TILES).is_none());
        assert_eq!(
            tilemap.tile_index(TILEMAP_WIDTH_TILES - 1, TILEMAP_HEIGHT_TILES - 1),
            Some(u8::try_from(TILE_COUNT - 1).unwrap())
        );
    }

    /// Composites a tilemap built past [`AffineTilemap::new`]'s validation, so the sampler's own
    /// guards answer for dimensions no caller can construct.
    fn unvalidated_layer_leaves_the_backdrop(width_tiles: usize, height_tiles: usize) -> bool {
        let (tileset, palette) = marked_8bpp_tileset_and_palette();
        let tilemap = AffineTilemap {
            width_tiles,
            height_tiles,
            tile_indices: Vec::new(),
        };
        let layer = AffineBgLayer::new(&tileset, &palette, &tilemap);
        let mut fb = Framebuffer::new();
        let backdrop = crate::palette::Rgb888 { r: 4, g: 5, b: 6 };
        fb.fill(backdrop);
        layer.composite(&mut fb, AffineMatrix::IDENTITY, 0, 0, Overflow::Wrap);
        fb.pixel(0, 0) == Some(backdrop)
    }

    #[test]
    fn huge_dimensions_never_reach_an_overflowing_pixel_size() {
        let overflowing_pixel_width_tiles = usize::MAX / BitDepth::TILE_DIM + 1;
        for (width_tiles, height_tiles) in [
            (overflowing_pixel_width_tiles, 0),
            (0, overflowing_pixel_width_tiles),
            (usize::MAX, 2),
        ] {
            assert!(
                AffineTilemap::new(width_tiles, height_tiles, Vec::new()).is_err(),
                "({width_tiles}, {height_tiles}) should have been rejected"
            );
            assert!(
                unvalidated_layer_leaves_the_backdrop(width_tiles, height_tiles),
                "({width_tiles}, {height_tiles}) should have composited nothing"
            );
        }
    }

    #[test]
    fn sample_pixel_never_panics_when_a_pixel_extent_exceeds_i32() {
        // 2^29 tiles * TILE_DIM (8) == 2^32, which narrows to 0 and would make `rem_euclid`
        // divide by zero. `AffineTilemap::new` rejects the width, so this reaches the sampler's
        // own boundary through the private fields.
        let unsampleable_width_tiles = 1 << 29;
        assert!(AffineTilemap::new(unsampleable_width_tiles, 1, Vec::new()).is_err());
        assert!(unvalidated_layer_leaves_the_backdrop(
            unsampleable_width_tiles,
            1
        ));
    }

    #[test]
    fn sample_pixel_skips_tile_index_0_transparent_pixels() {
        let transparent_tile = [0u8; BitDepth::Bpp8.tile_byte_len()];
        let tileset = Tileset::decode(BitDepth::Bpp8, &transparent_tile).unwrap();
        let palette = Palette::new([Bgr555::default(); Palette::LEN]);
        let tilemap = AffineTilemap::new(1, 1, vec![0]).unwrap();
        let layer = AffineBgLayer::new(&tileset, &palette, &tilemap);

        let mut fb = Framebuffer::new();
        let backdrop = crate::palette::Rgb888 { r: 1, g: 2, b: 3 };
        fb.fill(backdrop);
        layer.composite(&mut fb, AffineMatrix::IDENTITY, 0, 0, Overflow::Transparent);

        assert_eq!(fb.pixel(0, 0), Some(backdrop));
    }

    #[test]
    fn sample_row_with_mosaic_hold_retries_past_a_rejected_origin_and_holds_for_the_full_block() {
        // Two 8x8 tiles side by side (16 texture px wide): tile 0 flat color
        // A, tile 1 flat color B. A x4 horizontal scale and `reference_x` one
        // texture pixel left of the origin puts screen x=0 at texture x=-1
        // (rejected under `Overflow::Transparent`), screen x=1 at texture
        // x=3 (tile 0, A), and screen x=3/x=4 at texture x=11/x=15 (tile 1,
        // B) -- distinct colors close enough together to prove the held
        // value, not a fresh per-pixel sample, is what draws.
        let tile_byte_len = BitDepth::Bpp8.tile_byte_len();
        let mut bytes = vec![1u8; 2 * tile_byte_len];
        bytes[tile_byte_len..].fill(2);
        let tileset = Tileset::decode(BitDepth::Bpp8, &bytes).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        let color_a = Bgr555::from_channels(9, 0, 0);
        let color_b = Bgr555::from_channels(0, 9, 0);
        colors[1] = color_a;
        colors[2] = color_b;
        let palette = Palette::new(colors);
        let tilemap = AffineTilemap::new(2, 1, vec![0, 1]).unwrap();
        let layer = AffineBgLayer::new(&tileset, &palette, &tilemap);

        let scale = 4 * AffineMatrix::ONE;
        let matrix = AffineMatrix::new(scale, 0, 0, AffineMatrix::ONE);
        let reference_x = -i32::from(AffineMatrix::ONE);
        let block_h = 4;

        let row = layer.sample_row_with_mosaic_hold(matrix, reference_x, 0, 0, 7, block_h);

        assert_eq!(
            row,
            vec![
                None,                      // x=0: texture x=-1, rejected
                Some(color_a.to_rgb888()), // x=1: retries, texture x=3 (tile 0)
                Some(color_a.to_rgb888()), // x=2: held (fresh sample would still be A)
                Some(color_a.to_rgb888()), // x=3: held (fresh sample would be B -- proves override)
                Some(color_a.to_rgb888()), // x=4: held, 4th and last column of the block
                None,                      // x=5: block expired, retries, texture x=19 is OOB
                None,                      // x=6: still OOB
            ]
        );
    }

    #[test]
    fn sample_row_with_mosaic_hold_reloads_on_an_in_bounds_transparent_texel() {
        // A single 8x8 tile whose column 0 is palette index 0 (transparent)
        // and columns 1..8 are opaque. Identity transform samples texture x
        // == screen x directly. With a 3-pixel block, the transparent fetch
        // at x=0 must still reload the hold (mGBA's `mosaicWait` reload only
        // depends on the coordinate being in bounds, not on the fetched
        // pixel data) -- so x=1 and x=2 must stay blank even though a fresh
        // sample there would be opaque, and only x=3 (the next block) draws.
        let tile_byte_len = BitDepth::Bpp8.tile_byte_len();
        let mut bytes = vec![5u8; tile_byte_len];
        for row in 0..BitDepth::TILE_DIM {
            bytes[row * BitDepth::TILE_DIM] = 0;
        }
        let tileset = Tileset::decode(BitDepth::Bpp8, &bytes).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        let opaque = Bgr555::from_channels(0, 0, 9);
        colors[5] = opaque;
        let palette = Palette::new(colors);
        let tilemap = AffineTilemap::new(1, 1, vec![0]).unwrap();
        let layer = AffineBgLayer::new(&tileset, &palette, &tilemap);

        let block_h = 3;
        let row = layer.sample_row_with_mosaic_hold(AffineMatrix::IDENTITY, 0, 0, 0, 8, block_h);

        assert_eq!(
            row,
            vec![
                None,                     // x=0: in-bounds but index-0, reloads the hold anyway
                None,                     // x=1: held transparent (fresh sample would be opaque)
                None,                     // x=2: held transparent
                Some(opaque.to_rgb888()), // x=3: next block, fresh fetch finds the opaque texel
                Some(opaque.to_rgb888()), // x=4: held opaque
                Some(opaque.to_rgb888()), // x=5: held opaque
                Some(opaque.to_rgb888()), // x=6: next block, fresh fetch (still opaque)
                Some(opaque.to_rgb888()), // x=7: held opaque
            ]
        );
    }

    #[test]
    fn sample_row_with_mosaic_hold_applies_no_hold_at_all_for_a_block_size_of_two() {
        // mGBA only engages its mode-2 mosaic hold/retry macro for a decoded
        // block size of 3 or more; sizes 1 and 2 sample every column
        // directly, with no snapping or holding
        // (`mgba/src/gba/renderers/software-bg.c:56-76`,
        // `mgba/src/gba/renderers/software-private.h:181-185`). A single 8x8
        // tile with a distinct opaque color per column proves this: if a
        // 2-pixel hold were (wrongly) still applied, adjacent columns within
        // a block would repeat a color instead of each sampling its own.
        let tile_byte_len = BitDepth::Bpp8.tile_byte_len();
        let mut bytes = vec![0u8; tile_byte_len];
        for (column, byte) in bytes[..BitDepth::TILE_DIM].iter_mut().enumerate() {
            *byte = u8::try_from(column + 1).unwrap();
        }
        let tileset = Tileset::decode(BitDepth::Bpp8, &bytes).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        for (column, color) in colors[1..=BitDepth::TILE_DIM].iter_mut().enumerate() {
            *color = Bgr555::from_channels(u8::try_from(column + 1).unwrap(), 0, 0);
        }
        let palette = Palette::new(colors);
        let tilemap = AffineTilemap::new(1, 1, vec![0]).unwrap();
        let layer = AffineBgLayer::new(&tileset, &palette, &tilemap);

        let block_h = 2;
        let row = layer.sample_row_with_mosaic_hold(AffineMatrix::IDENTITY, 0, 0, 0, 8, block_h);

        let expected: Vec<Option<crate::palette::Rgb888>> = (1..=8)
            .map(|column| Some(Bgr555::from_channels(column, 0, 0).to_rgb888()))
            .collect();
        assert_eq!(
            row, expected,
            "every column samples its own texel; none of them hold a neighbor's"
        );
    }
}
