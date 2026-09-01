//! Affine and double-size sprite texel sampling.

use crate::affine::AffineMatrix;
use crate::oam::{AffineMode, OamEntry};
use crate::palette::Palette;
use crate::sprite::Texel;
use crate::tile::{BitDepth, Tileset};

/// Samples an affine sprite at a bounding-box-local pixel.
///
/// `dx` and `dy` must be within [`OamEntry::bounding_box`].
/// Missing matrices and transformed coordinates outside the nominal texture
/// yield [`Texel::Outside`].
///
/// # Panics
///
/// Panics if `entry` uses [`AffineMode::Regular`].
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "callers provide in-bounds coordinates, sprite bounding boxes are at most 128 by 128, and transformed coordinates are checked before unsigned conversion"
)]
pub(crate) fn sample_texel(
    entry: &OamEntry,
    matrices: &[AffineMatrix],
    tileset_4bpp: &Tileset,
    tileset_8bpp: &Tileset,
    palette: &Palette,
    dx: usize,
    dy: usize,
) -> Texel {
    const FIXED_POINT_HALF: i32 = AffineMatrix::ONE as i32 / 2;

    let matrix_index = match entry.affine() {
        AffineMode::Regular => {
            unreachable!("sample_texel is only called for affine OamEntry values")
        }
        AffineMode::Affine { matrix_num } | AffineMode::AffineDoubleSize { matrix_num } => {
            matrix_num
        }
    };
    let Some(&matrix) = matrices.get(usize::from(matrix_index)) else {
        return Texel::Outside;
    };

    let (bounding_width, bounding_height) = entry.bounding_box();
    let screen_from_center = (
        dx as i32 - bounding_width as i32 / 2,
        dy as i32 - bounding_height as i32 / 2,
    );
    let transformed = matrix.apply(screen_from_center.0, screen_from_center.1);

    let (texture_width, texture_height) = entry.dimensions();
    let texture_center = (
        texture_width as i32 * FIXED_POINT_HALF,
        texture_height as i32 * FIXED_POINT_HALF,
    );
    // `mgba/src/gba/renderers/software-obj.c:241-242` adds the nominal source
    // center before the signed 8.8 shift; reordering changes negative-edge rounding.
    let source = (
        (transformed.0 + texture_center.0) >> AffineMatrix::FRAC_BITS,
        (transformed.1 + texture_center.1) >> AffineMatrix::FRAC_BITS,
    );

    if !(0..texture_width as i32).contains(&source.0)
        || !(0..texture_height as i32).contains(&source.1)
    {
        // `mgba/src/gba/renderers/software-obj.c:30-45` leaves out-of-source
        // positions unwritten instead of emitting a transparent palette index.
        return Texel::Outside;
    }
    let (source_x, source_y) = (source.0 as usize, source.1 as usize);

    let tiles_per_texture_row = texture_width / BitDepth::TILE_DIM;
    let tile_x = source_x / BitDepth::TILE_DIM;
    let tile_y = source_y / BitDepth::TILE_DIM;
    let texture_tile_offset = (tile_y * tiles_per_texture_row + tile_x) as u16;
    let bit_depth = entry.bit_depth();
    let wrapped_tile_index =
        entry.tile_index().wrapping_add(texture_tile_offset) & bit_depth.obj_tile_index_mask();

    let tileset = match bit_depth {
        BitDepth::Bpp4 => tileset_4bpp,
        BitDepth::Bpp8 => tileset_8bpp,
    };
    let Some(tile) = tileset.tile(wrapped_tile_index) else {
        return Texel::Outside;
    };
    let palette_index = tile.index(source_x % BitDepth::TILE_DIM, source_y % BitDepth::TILE_DIM);
    if palette_index == 0 {
        return Texel::Transparent;
    }
    let color = match bit_depth {
        BitDepth::Bpp4 => palette.bank_color(entry.palette_bank(), palette_index),
        BitDepth::Bpp8 => palette.color(palette_index),
    };
    Texel::Opaque(color.to_rgb888())
}

#[cfg(test)]
mod tests {
    use crate::affine::AffineMatrix;
    use crate::framebuffer::Framebuffer;
    use crate::oam::{AffineMode, OamEntry, ObjShape};
    use crate::palette::{Bgr555, Palette};
    use crate::sprite::SpriteLayer;
    use crate::tile::{BitDepth, Tileset};

    const BITS_PER_4BPP_PIXEL: usize = 4;
    const PIXELS_PER_4BPP_BYTE: usize = 2;
    const FULL_CHANNEL: u8 = 0x1F;
    const RED_INDEX: u8 = 1;
    const GREEN_INDEX: u8 = 2;
    const BLUE_INDEX: u8 = 3;
    const YELLOW_INDEX: u8 = 4;
    const SIZE_8_BY_8: u8 = 0;
    const RED: Bgr555 = Bgr555::from_channels(FULL_CHANNEL, 0, 0);
    const GREEN: Bgr555 = Bgr555::from_channels(0, FULL_CHANNEL, 0);
    const BLUE: Bgr555 = Bgr555::from_channels(0, 0, FULL_CHANNEL);
    const YELLOW: Bgr555 = Bgr555::from_channels(FULL_CHANNEL, FULL_CHANNEL, 0);

    fn set_4bpp_pixel(bytes: &mut [u8], x: usize, y: usize, palette_index: u8) {
        let bytes_per_row = BitDepth::TILE_DIM / PIXELS_PER_4BPP_BYTE;
        let byte_index = y * bytes_per_row + x / PIXELS_PER_4BPP_BYTE;
        let shift = (x % PIXELS_PER_4BPP_BYTE) * BITS_PER_4BPP_PIXEL;
        bytes[byte_index] |= palette_index << shift;
    }

    fn corner_marked_tile() -> [u8; BitDepth::Bpp4.tile_byte_len()] {
        let mut bytes = [0u8; BitDepth::Bpp4.tile_byte_len()];
        let last = BitDepth::TILE_DIM - 1;
        set_4bpp_pixel(&mut bytes, 0, 0, RED_INDEX);
        set_4bpp_pixel(&mut bytes, last, 0, GREEN_INDEX);
        set_4bpp_pixel(&mut bytes, 0, last, BLUE_INDEX);
        set_4bpp_pixel(&mut bytes, last, last, YELLOW_INDEX);
        bytes
    }

    fn corner_marked_palette() -> Palette {
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[usize::from(RED_INDEX)] = RED;
        colors[usize::from(GREEN_INDEX)] = GREEN;
        colors[usize::from(BLUE_INDEX)] = BLUE;
        colors[usize::from(YELLOW_INDEX)] = YELLOW;
        Palette::new(colors)
    }

    fn square_entry(y: u8, size: u8, h_flip: bool, v_flip: bool) -> OamEntry {
        let x = 0;
        let tile_index = 0;
        let palette_bank = 0;
        let priority = 0;
        let enabled = true;
        OamEntry::new(
            x,
            y,
            tile_index,
            palette_bank,
            BitDepth::Bpp4,
            h_flip,
            v_flip,
            ObjShape::Square,
            size,
            priority,
            enabled,
        )
    }

    fn small_square_entry() -> OamEntry {
        square_entry(0, SIZE_8_BY_8, false, false)
    }

    fn affine_entry(mode: AffineMode) -> OamEntry {
        small_square_entry().with_affine(mode)
    }

    #[test]
    fn identity_matrix_affine_sprite_matches_the_regular_render() {
        let tileset = Tileset::decode(BitDepth::Bpp4, &corner_marked_tile()).unwrap();
        let palette = corner_marked_palette();

        let regular = [small_square_entry()];
        let regular_layer = SpriteLayer::new(&regular, &tileset, &tileset, &palette);
        let mut regular_fb = Framebuffer::new();
        regular_layer.composite(&mut regular_fb);

        let affine = [affine_entry(AffineMode::Affine { matrix_num: 0 })];
        let matrices = [AffineMatrix::IDENTITY];
        let affine_layer =
            SpriteLayer::new(&affine, &tileset, &tileset, &palette).with_affine_matrices(&matrices);
        let mut affine_fb = Framebuffer::new();
        affine_layer.composite(&mut affine_fb);

        for y in 0..BitDepth::TILE_DIM {
            for x in 0..BitDepth::TILE_DIM {
                assert_eq!(
                    regular_fb.pixel(x, y),
                    affine_fb.pixel(x, y),
                    "pixel ({x},{y})"
                );
            }
        }
    }

    #[test]
    fn quarter_turn_clips_the_even_width_edge_and_rotates_the_other_pixels() {
        let tileset = Tileset::decode(BitDepth::Bpp4, &corner_marked_tile()).unwrap();
        let palette = corner_marked_palette();
        let entries = [affine_entry(AffineMode::Affine { matrix_num: 0 })];
        let quarter_turn = AffineMatrix::new(0, -AffineMatrix::ONE, AffineMatrix::ONE, 0);
        let matrices = [quarter_turn];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette)
            .with_affine_matrices(&matrices);

        let clipped_top_row = 0;
        for x in 0..BitDepth::TILE_DIM {
            assert_eq!(layer.resolve_pixel(x, clipped_top_row), None);
        }

        let first_sampled_row = clipped_top_row + 1;
        let last_x = BitDepth::TILE_DIM - 1;
        assert_eq!(
            layer.resolve_pixel(0, first_sampled_row).map(|p| p.color),
            Some(GREEN.to_rgb888()),
            "tex(7,0) (green) rotates to screen (0,1)"
        );
        assert_eq!(
            layer
                .resolve_pixel(last_x, first_sampled_row)
                .map(|p| p.color),
            Some(YELLOW.to_rgb888()),
            "tex(7,7) (yellow) rotates to screen (7,1)"
        );
    }

    #[test]
    fn double_size_sprite_centers_the_source_texture_in_the_doubled_box() {
        let tileset = Tileset::decode(BitDepth::Bpp4, &corner_marked_tile()).unwrap();
        let palette = corner_marked_palette();
        let entries = [affine_entry(AffineMode::AffineDoubleSize { matrix_num: 0 })];
        let matrices = [AffineMatrix::IDENTITY];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette)
            .with_affine_matrices(&matrices);

        let source_size = BitDepth::TILE_DIM;
        let bounding_size = source_size * 2;
        let source_margin = (bounding_size - source_size) / 2;
        let source_last = source_margin + source_size - 1;
        assert_eq!(layer.resolve_pixel(0, 0), None);
        assert_eq!(
            layer.resolve_pixel(bounding_size - 1, bounding_size - 1),
            None
        );
        assert_eq!(
            layer
                .resolve_pixel(source_margin, source_margin)
                .map(|p| p.color),
            Some(RED.to_rgb888())
        );
        assert_eq!(
            layer
                .resolve_pixel(source_last, source_last)
                .map(|p| p.color),
            Some(YELLOW.to_rgb888())
        );
        assert_eq!(layer.resolve_pixel(source_margin - 1, source_margin), None);
        assert_eq!(
            layer.resolve_pixel(source_margin + source_size, source_margin),
            None
        );
    }

    #[test]
    fn plain_affine_sprite_is_clipped_to_its_undoubled_box() {
        let tileset = Tileset::decode(BitDepth::Bpp4, &corner_marked_tile()).unwrap();
        let palette = corner_marked_palette();
        let entries = [affine_entry(AffineMode::Affine { matrix_num: 0 })];
        let matrices = [AffineMatrix::IDENTITY];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette)
            .with_affine_matrices(&matrices);

        let first_pixel_outside_box = BitDepth::TILE_DIM;
        assert_eq!(
            layer.resolve_pixel(first_pixel_outside_box, first_pixel_outside_box),
            None
        );
        assert_eq!(
            layer.resolve_pixel(0, 0).map(|p| p.color),
            Some(RED.to_rgb888())
        );
    }

    #[test]
    fn matrix_num_selects_the_right_slot_out_of_several() {
        let tileset = Tileset::decode(BitDepth::Bpp4, &corner_marked_tile()).unwrap();
        let palette = corner_marked_palette();
        let identity_matrix_index = 0;
        let half_turn_matrix_index = 1;
        let identity_entry = affine_entry(AffineMode::Affine {
            matrix_num: identity_matrix_index,
        });
        let rotated_entry = affine_entry(AffineMode::Affine {
            matrix_num: half_turn_matrix_index,
        });
        let half_turn = AffineMatrix::new(-AffineMatrix::ONE, 0, 0, -AffineMatrix::ONE);
        let matrices = [AffineMatrix::IDENTITY, half_turn];

        let identity_layer = SpriteLayer::new(
            std::slice::from_ref(&identity_entry),
            &tileset,
            &tileset,
            &palette,
        )
        .with_affine_matrices(&matrices);
        assert_eq!(
            identity_layer.resolve_pixel(0, 0).map(|p| p.color),
            Some(RED.to_rgb888()),
            "matrix_num=0 (identity) keeps red at the top-left"
        );

        let rotated_layer = SpriteLayer::new(
            std::slice::from_ref(&rotated_entry),
            &tileset,
            &tileset,
            &palette,
        )
        .with_affine_matrices(&matrices);
        assert_eq!(
            rotated_layer.resolve_pixel(1, 1).map(|p| p.color),
            Some(YELLOW.to_rgb888()),
            "matrix_num=1 (180-degree rotation) puts yellow at screen (1,1)"
        );
    }

    #[test]
    fn double_size_box_at_wrapping_y_renders_only_the_top_band_not_a_second_copy() {
        const SIZE_64_BY_64: u8 = 3;
        const RAW_Y: u8 = 140;
        const OPAQUE_INDEX: u8 = (1 << BITS_PER_4BPP_PIXEL) - 1;
        const PACKED_OPAQUE_PIXELS: u8 = OPAQUE_INDEX | (OPAQUE_INDEX << BITS_PER_4BPP_PIXEL);

        let entry = square_entry(RAW_Y, SIZE_64_BY_64, false, false)
            .with_affine(AffineMode::AffineDoubleSize { matrix_num: 0 });
        let source_dimension = entry.dimensions().0;
        let tiles_per_axis = source_dimension / BitDepth::TILE_DIM;
        let tile_count = tiles_per_axis * tiles_per_axis;
        let opaque = vec![PACKED_OPAQUE_PIXELS; BitDepth::Bpp4.tile_byte_len() * tile_count];
        let tileset = Tileset::decode(BitDepth::Bpp4, &opaque).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[usize::from(OPAQUE_INDEX)] = RED;
        let palette = Palette::new(colors);

        let entries = [entry];
        let half_scale = AffineMatrix::ONE / 2;
        let matrices = [AffineMatrix::new(half_scale, 0, 0, half_scale)];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette)
            .with_affine_matrices(&matrices);

        let wrapped_origin = i32::from(RAW_Y) - OamEntry::Y_SPACE;
        let (_, bounding_height) = entry.bounding_box();
        let bounding_height = i32::try_from(bounding_height).unwrap();
        let visible_band_end = usize::try_from(wrapped_origin + bounding_height).unwrap();
        let visible_band_last_row = visible_band_end - 1;
        assert_eq!(
            layer.resolve_pixel(0, 0).map(|p| p.color),
            Some(RED.to_rgb888()),
            "top-wrapped band row 0 must render"
        );
        assert_eq!(
            layer
                .resolve_pixel(0, visible_band_last_row)
                .map(|p| p.color),
            Some(RED.to_rgb888()),
            "top-wrapped band's last row must render"
        );
        assert_eq!(
            layer.resolve_pixel(0, visible_band_end),
            None,
            "the wrapped band ends before this row"
        );
        assert_eq!(
            layer.resolve_pixel(0, usize::from(RAW_Y)),
            None,
            "no second copy begins at the raw Y origin"
        );
        assert_eq!(
            layer.resolve_pixel(0, Framebuffer::HEIGHT - 1),
            None,
            "no second copy reaches the screen's last row"
        );
    }

    #[test]
    fn out_of_range_matrix_num_is_treated_as_outside_not_a_panic() {
        let tileset = Tileset::decode(BitDepth::Bpp4, &corner_marked_tile()).unwrap();
        let palette = corner_marked_palette();
        let matrices = [AffineMatrix::IDENTITY];
        let missing_matrix_index = u8::try_from(matrices.len()).unwrap();
        let entries = [affine_entry(AffineMode::Affine {
            matrix_num: missing_matrix_index,
        })];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette)
            .with_affine_matrices(&matrices);

        assert_eq!(layer.resolve_pixel(0, 0), None);
    }

    #[test]
    fn h_and_v_flip_are_ignored_on_an_affine_entry() {
        let tileset = Tileset::decode(BitDepth::Bpp4, &corner_marked_tile()).unwrap();
        let palette = corner_marked_palette();
        let entry = square_entry(0, SIZE_8_BY_8, true, true)
            .with_affine(AffineMode::Affine { matrix_num: 0 });
        let entries = [entry];
        let matrices = [AffineMatrix::IDENTITY];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette)
            .with_affine_matrices(&matrices);

        assert_eq!(
            layer.resolve_pixel(0, 0).map(|p| p.color),
            Some(RED.to_rgb888()),
            "red stays at the top-left despite h_flip/v_flip being set"
        );
    }
}
