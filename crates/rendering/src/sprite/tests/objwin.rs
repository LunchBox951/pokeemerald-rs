//! Pins OBJ-window masks and their interaction with display priority and OBJ mosaic.

use super::shared::{
    bpp4_tile, entry, half_scale_matrix, horizontal_mosaic_fixture, opaque_and_transparent_tiles,
    quadrant_palette, quadrant_tile, solid_4bpp_tile, square_8x8,
    wrap_boundary_affine_double_size_texture, wrap_boundary_double_size_affine_entry,
    EIGHT_PIXEL_SQUARE_SIZE, FIRST_PALETTE_BANK, FIRST_TILE, HIGHEST_OBJ_PRIORITY,
    TRANSPARENT_TILE,
};
use crate::affine::AffineMatrix;
use crate::framebuffer::Framebuffer;
use crate::mosaic::MosaicSize;
use crate::oam::{AffineMode, OamEntry, ObjMode, ObjShape};
use crate::palette::{Bgr555, Palette};
use crate::sprite::{SpriteLayer, WindowSpans};
use crate::tile::{BitDepth, Tileset};
use crate::window::{WindowLayerEnable, WindowPass, WindowRegion};

#[test]
fn resolve_pixel_objwin_transparent_hole_upgrades_an_opaque_worse_sprite() {
    let (tileset, palette) = opaque_and_transparent_tiles();
    let opaque_priority_two = square_8x8(FIRST_TILE, 2, 0);
    let objwin_hole_priority_zero = square_8x8(TRANSPARENT_TILE, 0, 0).with_mode(ObjMode::Window);
    let entries = [opaque_priority_two, objwin_hole_priority_zero];
    let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

    let pixel = layer.resolve_pixel(0, 0).unwrap();
    assert_eq!(pixel.color, Bgr555::from_channels(0, 0, 0x1F).to_rgb888());
    assert_eq!(pixel.priority, 0);

    let entries_control = [opaque_priority_two];
    let control = SpriteLayer::new(&entries_control, &tileset, &tileset, &palette);
    assert_eq!(control.resolve_pixel(0, 0).unwrap().priority, 2);
}

#[test]
fn resolve_pixel_with_mosaic_windowed_suppresses_objwin_hole_when_flagged() {
    let (tileset, palette) = opaque_and_transparent_tiles();
    let opaque_priority_two = square_8x8(FIRST_TILE, 2, 0);
    let objwin_hole_priority_zero = square_8x8(TRANSPARENT_TILE, 0, 0).with_mode(ObjMode::Window);
    let entries = [opaque_priority_two, objwin_hole_priority_zero];
    let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

    let win0_pass = [WindowPass {
        start: 0,
        control: WindowLayerEnable::ALL,
        region: WindowRegion::Win0,
    }];
    let suppressing_span = WindowSpans::new(&win0_pass, false);
    let suppressed = layer
        .resolve_pixel_with_mosaic_windowed(0, 0, MosaicSize::NONE, suppressing_span)
        .unwrap();
    assert_eq!(suppressed.priority, 2);

    let unsuppressed = layer
        .resolve_pixel_with_mosaic_windowed(0, 0, MosaicSize::NONE, WindowSpans::WHOLE_SCANLINE)
        .unwrap();
    assert_eq!(unsuppressed.priority, 0);
}

#[test]
fn resolve_pixel_objwin_opaque_texel_supplies_no_display_pixel() {
    let (tileset, palette) = opaque_and_transparent_tiles();
    let objwin_opaque = square_8x8(FIRST_TILE, 0, 0).with_mode(ObjMode::Window);
    let entries = [objwin_opaque];
    let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);
    assert_eq!(layer.resolve_pixel(0, 0), None);
    assert!(layer.objwin_mask(0, 0));
}

/// The OBJWIN affine path shares `sample_affine_local` with the normal
/// affine path, so it must be pinned to the same forced bottom row.
#[test]
fn affine_objwin_mosaic_box_ending_at_y_space_samples_its_bottom_row() {
    const RAW_Y: u8 = 128;
    const SCREEN_X: usize = 2;
    const VERTICAL_MOSAIC_SIZE: u8 = 2;

    let (bytes, palette) = wrap_boundary_affine_double_size_texture();
    let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
    let entries = [wrap_boundary_double_size_affine_entry(RAW_Y)
        .with_mode(ObjMode::Window)
        .with_mosaic(true)];
    let matrices = [half_scale_matrix()];
    let layer =
        SpriteLayer::new(&entries, &tileset, &tileset, &palette).with_affine_matrices(&matrices);
    let vertical_mosaic = MosaicSize::new(1, VERTICAL_MOSAIC_SIZE);

    assert!(
        !layer.objwin_mask(SCREEN_X, usize::from(RAW_Y)),
        "without mosaic, the top row's source column is transparent"
    );
    for y in usize::from(RAW_Y)..Framebuffer::HEIGHT {
        assert!(
            layer.objwin_mask_with_mosaic(SCREEN_X, y, vertical_mosaic),
            "screen y = {y}"
        );
    }
}

#[test]
fn affine_objwin_mask_ignores_horizontal_obj_mosaic() {
    const SPRITE_X: u16 = 2;
    const HORIZONTAL_MOSAIC_SIZE: u8 = 4;
    const OBSERVED_WIDTH: usize = 16;

    let (bytes, palette) = horizontal_mosaic_fixture();
    let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
    let entries = [entry(SPRITE_X, 0, true)
        .with_mode(ObjMode::Window)
        .with_mosaic(true)
        .with_affine(AffineMode::Affine { matrix_num: 0 })];
    let matrices = [AffineMatrix::IDENTITY];
    let layer =
        SpriteLayer::new(&entries, &tileset, &tileset, &palette).with_affine_matrices(&matrices);
    let horizontal_mosaic = MosaicSize::new(HORIZONTAL_MOSAIC_SIZE, 1);
    for x in 0..OBSERVED_WIDTH {
        assert_eq!(
            layer.objwin_mask_with_mosaic(x, 0, horizontal_mosaic),
            layer.objwin_mask(x, 0),
            "screen x = {x}"
        );
    }
}

#[test]
fn regular_objwin_mask_keeps_the_mosaic_rounded_draw_condition() {
    const SPRITE_X: u16 = 2;
    const RAW_END_X: usize = 10;
    const ROUNDED_END_X: usize = 12;
    const HORIZONTAL_MOSAIC_SIZE: u8 = 4;

    let mut bytes = [0u8; 64];
    bytes[..32].copy_from_slice(&solid_4bpp_tile(1));
    bytes[32..].copy_from_slice(&bpp4_tile(&[((0, 0), 1), ((1, 0), 1)]));
    let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
    let palette = Palette::new([Bgr555::default(); Palette::LEN]);
    let entries = [entry(SPRITE_X, 0, true)
        .with_mode(ObjMode::Window)
        .with_mosaic(true)];
    let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);
    let mosaic = MosaicSize::new(HORIZONTAL_MOSAIC_SIZE, 1);

    for x in usize::from(SPRITE_X)..RAW_END_X {
        assert!(layer.objwin_mask_with_mosaic(x, 0, mosaic));
    }
    for x in RAW_END_X..ROUNDED_END_X {
        assert!(layer.objwin_mask_with_mosaic(x, 0, mosaic));
    }
    assert!(!layer.objwin_mask_with_mosaic(ROUNDED_END_X, 0, mosaic));
}

#[test]
fn flipped_regular_objwin_mask_wraps_the_mosaic_tail_below_the_sprite_tiles() {
    const SPRITE_TILE_INDEX: u16 = 1;
    const SPRITE_X: u16 = 2;
    const RAW_END_X: usize = 10;
    const OPAQUE_WRAPPED_TAIL_X: usize = 10;
    const TRANSPARENT_WRAPPED_TAIL_X: usize = 11;
    const ROUNDED_END_X: usize = 12;
    const HORIZONTAL_MOSAIC_SIZE: u8 = 4;

    let mut bytes = [0u8; 64];
    bytes[..32].copy_from_slice(&bpp4_tile(&[((7, 0), 1)]));
    bytes[32..].copy_from_slice(&solid_4bpp_tile(1));
    let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
    let palette = Palette::new([Bgr555::default(); Palette::LEN]);
    let entries = [OamEntry::new(
        SPRITE_X,
        0,
        SPRITE_TILE_INDEX,
        FIRST_PALETTE_BANK,
        BitDepth::Bpp4,
        true,
        false,
        ObjShape::Square,
        EIGHT_PIXEL_SQUARE_SIZE,
        HIGHEST_OBJ_PRIORITY,
        true,
    )
    .with_mode(ObjMode::Window)
    .with_mosaic(true)];
    let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);
    let mosaic = MosaicSize::new(HORIZONTAL_MOSAIC_SIZE, 1);

    for x in usize::from(SPRITE_X)..RAW_END_X {
        assert!(layer.objwin_mask_with_mosaic(x, 0, mosaic));
    }
    assert!(layer.objwin_mask_with_mosaic(OPAQUE_WRAPPED_TAIL_X, 0, mosaic));
    assert!(!layer.objwin_mask_with_mosaic(TRANSPARENT_WRAPPED_TAIL_X, 0, mosaic));
    assert!(!layer.objwin_mask_with_mosaic(ROUNDED_END_X, 0, mosaic));
}

#[test]
fn regular_objwin_mask_ignores_horizontal_obj_mosaic() {
    const SPRITE_X: u16 = 2;
    const HORIZONTAL_MOSAIC_SIZE: u8 = 4;
    const OBSERVED_WIDTH: usize = 16;

    let (bytes, palette) = horizontal_mosaic_fixture();
    let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
    let entries = [entry(SPRITE_X, 0, true)
        .with_mode(ObjMode::Window)
        .with_mosaic(true)];
    let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);
    let horizontal_mosaic = MosaicSize::new(HORIZONTAL_MOSAIC_SIZE, 1);
    for x in 0..OBSERVED_WIDTH {
        assert_eq!(
            layer.objwin_mask_with_mosaic(x, 0, horizontal_mosaic),
            layer.objwin_mask(x, 0),
            "screen x = {x}"
        );
    }
}

#[test]
fn objwin_mask_still_applies_vertical_obj_mosaic() {
    const OPAQUE_SOURCE_ROW: usize = 1;
    const VERTICAL_MOSAIC_SIZE: u8 = 4;

    let tileset = Tileset::decode(BitDepth::Bpp4, &quadrant_tile()).unwrap();
    let palette = quadrant_palette();
    let entries = [entry(0, 0, true)
        .with_mode(ObjMode::Window)
        .with_mosaic(true)];
    let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

    assert!(layer.objwin_mask_with_mosaic(0, OPAQUE_SOURCE_ROW, MosaicSize::NONE));
    let vertical_mosaic = MosaicSize::new(1, VERTICAL_MOSAIC_SIZE);
    assert!(!layer.objwin_mask_with_mosaic(0, OPAQUE_SOURCE_ROW, vertical_mosaic));
}
