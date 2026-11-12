//! Pins framebuffer-edge bounds handling for sprite queries.

use super::shared::{opaque_and_transparent_tiles, square_8x8, FIRST_TILE};
use crate::framebuffer::Framebuffer;
use crate::mosaic::MosaicSize;
use crate::oam::{OamEntry, ObjMode, ObjShape};
use crate::sprite::SpriteLayer;
use crate::tile::BitDepth;

fn assert_display_queries_miss(layer: &SpriteLayer<'_>, x: usize, y: usize) {
    assert_eq!(layer.resolve_pixel(x, y), None);
    assert_eq!(
        layer.resolve_pixel_with_mosaic(x, y, MosaicSize::NONE),
        None
    );
}

fn assert_objwin_queries_miss(layer: &SpriteLayer<'_>, x: usize, y: usize) {
    assert!(!layer.objwin_mask(x, y));
    assert!(!layer.objwin_mask_with_mosaic(x, y, MosaicSize::NONE));
}

#[test]
fn queries_at_the_framebuffer_edge_miss_a_sprite_whose_raw_footprint_reaches_past_it() {
    const STRADDLING_RIGHT_EDGE_X: u16 = 236;
    const STRADDLING_BOTTOM_EDGE_Y: u8 = 155;
    const VERTICAL_8X16_SIZE: u8 = 0;

    let (tileset, palette) = opaque_and_transparent_tiles();

    let x_edge_entry = |mode: ObjMode| {
        OamEntry::new(
            STRADDLING_RIGHT_EDGE_X,
            0,
            FIRST_TILE,
            0,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            0,
            0,
            true,
        )
        .with_mode(mode)
    };
    let x_entries = [x_edge_entry(ObjMode::Normal)];
    let x_layer = SpriteLayer::new(&x_entries, &tileset, &tileset, &palette);
    assert!(x_layer.resolve_pixel(Framebuffer::WIDTH - 1, 0).is_some());
    assert_display_queries_miss(&x_layer, Framebuffer::WIDTH, 0);

    let x_objwin_entries = [x_edge_entry(ObjMode::Window)];
    let x_objwin_layer = SpriteLayer::new(&x_objwin_entries, &tileset, &tileset, &palette);
    assert!(x_objwin_layer.objwin_mask(Framebuffer::WIDTH - 1, 0));
    assert_objwin_queries_miss(&x_objwin_layer, Framebuffer::WIDTH, 0);

    let y_edge_entry = |mode: ObjMode| {
        OamEntry::new(
            0,
            STRADDLING_BOTTOM_EDGE_Y,
            FIRST_TILE,
            0,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Vertical,
            VERTICAL_8X16_SIZE,
            0,
            true,
        )
        .with_mode(mode)
    };
    let y_entries = [y_edge_entry(ObjMode::Normal)];
    let y_layer = SpriteLayer::new(&y_entries, &tileset, &tileset, &palette);
    assert!(y_layer.resolve_pixel(0, Framebuffer::HEIGHT - 1).is_some());
    assert_display_queries_miss(&y_layer, 0, Framebuffer::HEIGHT);

    let y_objwin_entries = [y_edge_entry(ObjMode::Window)];
    let y_objwin_layer = SpriteLayer::new(&y_objwin_entries, &tileset, &tileset, &palette);
    assert!(y_objwin_layer.objwin_mask(0, Framebuffer::HEIGHT - 1));
    assert_objwin_queries_miss(&y_objwin_layer, 0, Framebuffer::HEIGHT);
}

#[test]
#[cfg(target_pointer_width = "64")]
fn queries_far_outside_the_framebuffer_miss_a_sprite_at_the_origin() {
    const COORDINATE_THAT_TRUNCATES_TO_ZERO_AS_I32: usize = 1 << 32;

    let (tileset, palette) = opaque_and_transparent_tiles();
    let entries = [square_8x8(FIRST_TILE, 0, 0)];
    let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);
    assert!(layer.resolve_pixel(0, 0).is_some());

    for (x, y) in [
        (COORDINATE_THAT_TRUNCATES_TO_ZERO_AS_I32, 0),
        (0, COORDINATE_THAT_TRUNCATES_TO_ZERO_AS_I32),
    ] {
        assert_display_queries_miss(&layer, x, y);
    }

    let objwin_entries = [square_8x8(FIRST_TILE, 0, 0).with_mode(ObjMode::Window)];
    let objwin_layer = SpriteLayer::new(&objwin_entries, &tileset, &tileset, &palette);
    assert!(objwin_layer.objwin_mask(0, 0));
    for (x, y) in [
        (COORDINATE_THAT_TRUNCATES_TO_ZERO_AS_I32, 0),
        (0, COORDINATE_THAT_TRUNCATES_TO_ZERO_AS_I32),
    ] {
        assert_objwin_queries_miss(&objwin_layer, x, y);
    }
}
