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
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        reason = "framebuffer dimensions and screen coordinates fit in i32, and sampled pixels are nonnegative"
    )]
    pub(crate) fn sample_pixel(
        &self,
        matrix: AffineMatrix,
        reference_x: i32,
        reference_y: i32,
        overflow: Overflow,
        screen_x: usize,
        screen_y: usize,
    ) -> Option<Rgb888> {
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
        let (sample_x, sample_y) = (sample_x as usize, sample_y as usize);

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
}
