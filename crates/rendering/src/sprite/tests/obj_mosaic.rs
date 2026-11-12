//! Pins regular and affine OBJ mosaic block sampling.

use super::shared::{
    bpp4_tile, entry, half_scale_matrix, horizontal_mosaic_fixture, palette_with_colors,
    solid_4bpp_tile, wrap_boundary_affine_double_size_texture,
    wrap_boundary_double_size_affine_entry, BLUE, BLUE_INDEX, GREEN, GREEN_INDEX, RED, RED_INDEX,
    YELLOW, YELLOW_INDEX,
};
use crate::affine::AffineMatrix;
use crate::framebuffer::Framebuffer;
use crate::mosaic::MosaicSize;
use crate::oam::AffineMode;
use crate::sprite::SpriteLayer;
use crate::tile::{BitDepth, Tileset};

#[test]
fn regular_mosaic_leading_partial_block_replicates_the_edge_column() {
    const SPRITE_X: u16 = 2;
    const MOSAIC_WIDTH: u8 = 4;

    let (bytes, palette) = horizontal_mosaic_fixture();
    let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
    let entries = [entry(SPRITE_X, 0, true).with_mosaic(true)];
    let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);
    let mosaic = MosaicSize::new(MOSAIC_WIDTH, 1);

    for x in usize::from(SPRITE_X)..usize::from(MOSAIC_WIDTH) {
        assert_eq!(
            layer
                .resolve_pixel_with_mosaic(x, 0, mosaic)
                .map(|p| p.color),
            Some(RED.to_rgb888()),
            "screen x = {x}"
        );
    }
    for x in usize::from(MOSAIC_WIDTH)..usize::from(MOSAIC_WIDTH) * 2 {
        assert_eq!(
            layer
                .resolve_pixel_with_mosaic(x, 0, mosaic)
                .map(|p| p.color),
            Some(GREEN.to_rgb888()),
            "screen x = {x}"
        );
    }
}

#[test]
fn regular_mosaic_trailing_partial_block_extends_past_the_raw_edge() {
    const NEGATIVE_FOUR_RAW_X: u16 = 0x01FC;
    const HORIZONTAL_MOSAIC_SIZE: u8 = 3;
    const LAST_COVERED_X: usize = 5;
    const FIRST_UNCOVERED_X: usize = 6;

    let bytes = solid_4bpp_tile(RED_INDEX);
    let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
    let palette = palette_with_colors(&[(RED_INDEX, RED)]);
    let entries = [entry(NEGATIVE_FOUR_RAW_X, 0, true).with_mosaic(true)];
    let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);
    let mosaic = MosaicSize::new(HORIZONTAL_MOSAIC_SIZE, 1);

    for x in 0..=LAST_COVERED_X {
        assert_eq!(
            layer
                .resolve_pixel_with_mosaic(x, 0, mosaic)
                .map(|p| p.color),
            Some(RED.to_rgb888()),
            "screen x = {x}"
        );
    }
    assert_eq!(
        layer.resolve_pixel_with_mosaic(FIRST_UNCOVERED_X, 0, mosaic),
        None,
        "screen x = {FIRST_UNCOVERED_X}"
    );
}

#[test]
fn regular_mosaic_leading_partial_block_uses_the_nearest_source_column() {
    const SPRITE_X: u16 = 2;
    const MOSAIC_WIDTH: u8 = 4;

    let (bytes, palette) = horizontal_mosaic_fixture();
    let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
    let entries = [entry(SPRITE_X, 0, true).with_mosaic(true)];
    let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);
    let mosaic = MosaicSize::new(MOSAIC_WIDTH, 1);

    for x in usize::from(SPRITE_X)..usize::from(MOSAIC_WIDTH) {
        assert_eq!(
            layer
                .resolve_pixel_with_mosaic(x, 0, mosaic)
                .map(|p| p.color),
            Some(RED.to_rgb888()),
            "screen x = {x}"
        );
    }
}

#[test]
fn affine_mosaic_leading_partial_block_leaves_the_source_unwritten() {
    const SPRITE_X: u16 = 2;
    const MOSAIC_WIDTH: u8 = 4;

    let (bytes, palette) = horizontal_mosaic_fixture();
    let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
    let entries = [entry(SPRITE_X, 0, true)
        .with_mosaic(true)
        .with_affine(AffineMode::Affine { matrix_num: 0 })];
    let matrices = [AffineMatrix::IDENTITY];
    let layer =
        SpriteLayer::new(&entries, &tileset, &tileset, &palette).with_affine_matrices(&matrices);
    let mosaic = MosaicSize::new(MOSAIC_WIDTH, 1);

    for x in usize::from(SPRITE_X)..usize::from(MOSAIC_WIDTH) {
        assert_eq!(
            layer.resolve_pixel_with_mosaic(x, 0, mosaic),
            None,
            "screen x = {x}"
        );
    }
    for x in usize::from(MOSAIC_WIDTH)..usize::from(MOSAIC_WIDTH) * 2 {
        assert_eq!(
            layer
                .resolve_pixel_with_mosaic(x, 0, mosaic)
                .map(|p| p.color),
            Some(GREEN.to_rgb888()),
            "screen x = {x}"
        );
    }
    for x in usize::from(MOSAIC_WIDTH) * 2..usize::from(MOSAIC_WIDTH) * 3 {
        assert_eq!(
            layer
                .resolve_pixel_with_mosaic(x, 0, mosaic)
                .map(|p| p.color),
            Some(BLUE.to_rgb888()),
            "screen x = {x}"
        );
    }
}

#[test]
fn affine_mosaic_leading_partial_block_holds_the_transform_not_the_footprint_edge() {
    const SPRITE_X: u16 = 2;
    const MOSAIC_WIDTH: u8 = 4;
    const TRANSFORMED_SOURCE_ROW: usize = 2;

    let bytes = bpp4_tile(&[
        ((0, TRANSFORMED_SOURCE_ROW), YELLOW_INDEX),
        ((1, TRANSFORMED_SOURCE_ROW), RED_INDEX),
        ((3, TRANSFORMED_SOURCE_ROW), GREEN_INDEX),
        ((5, TRANSFORMED_SOURCE_ROW), BLUE_INDEX),
    ]);
    let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
    let palette = palette_with_colors(&[
        (RED_INDEX, RED),
        (GREEN_INDEX, GREEN),
        (BLUE_INDEX, BLUE),
        (YELLOW_INDEX, YELLOW),
    ]);
    let entries = [entry(SPRITE_X, 0, true)
        .with_mosaic(true)
        .with_affine(AffineMode::Affine { matrix_num: 0 })];
    let two_times_magnification = AffineMatrix::ONE / 2;
    let matrices = [AffineMatrix::new(
        two_times_magnification,
        0,
        0,
        two_times_magnification,
    )];
    let layer =
        SpriteLayer::new(&entries, &tileset, &tileset, &palette).with_affine_matrices(&matrices);
    let mosaic = MosaicSize::new(MOSAIC_WIDTH, 1);

    for x in usize::from(SPRITE_X)..usize::from(MOSAIC_WIDTH) {
        assert_eq!(
            layer
                .resolve_pixel_with_mosaic(x, 0, mosaic)
                .map(|p| p.color),
            Some(RED.to_rgb888()),
            "screen x = {x}"
        );
    }
    for x in usize::from(MOSAIC_WIDTH)..usize::from(MOSAIC_WIDTH) * 2 {
        assert_eq!(
            layer
                .resolve_pixel_with_mosaic(x, 0, mosaic)
                .map(|p| p.color),
            Some(GREEN.to_rgb888()),
            "screen x = {x}"
        );
    }
    for x in usize::from(MOSAIC_WIDTH) * 2..usize::from(MOSAIC_WIDTH) * 3 {
        assert_eq!(
            layer
                .resolve_pixel_with_mosaic(x, 0, mosaic)
                .map(|p| p.color),
            Some(BLUE.to_rgb888()),
            "screen x = {x}"
        );
    }
}

/// A double-size affine box (128 tall) at raw Y=128 ends exactly at the
/// Y-space boundary, the case `OamEntry::floor_dy_to_wrap_boundary_bottom`
/// documents.
#[test]
fn affine_mosaic_box_ending_at_y_space_samples_its_bottom_row() {
    const RAW_Y: u8 = 128;
    const VERTICAL_MOSAIC_SIZE: u8 = 2;

    let (bytes, palette) = wrap_boundary_affine_double_size_texture();
    let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
    let entries = [wrap_boundary_double_size_affine_entry(RAW_Y).with_mosaic(true)];
    let matrices = [half_scale_matrix()];
    let layer =
        SpriteLayer::new(&entries, &tileset, &tileset, &palette).with_affine_matrices(&matrices);
    let vertical_mosaic = MosaicSize::new(1, VERTICAL_MOSAIC_SIZE);

    assert_eq!(
        layer.resolve_pixel(0, usize::from(RAW_Y)).map(|p| p.color),
        Some(RED.to_rgb888()),
        "without mosaic, the top row samples its own source row"
    );
    for y in usize::from(RAW_Y)..Framebuffer::HEIGHT {
        assert_eq!(
            layer
                .resolve_pixel_with_mosaic(0, y, vertical_mosaic)
                .map(|p| p.color),
            Some(GREEN.to_rgb888()),
            "screen y = {y}"
        );
    }
}
