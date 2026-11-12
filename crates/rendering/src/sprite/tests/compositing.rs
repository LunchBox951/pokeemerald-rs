//! Pins basic sprite compositing: transparency, flips, edge clipping, wrap, and disabled entries.

use super::shared::{
    bpp4_tile, entry, palette_with_colors, quadrant_palette, quadrant_tile, solid_4bpp_tile, BLUE,
    BLUE_INDEX, BPP4_TILE_BYTES, EIGHT_PIXEL_SQUARE_SIZE, FIRST_PALETTE_BANK, FIRST_TILE, GREEN,
    GREEN_INDEX, HIGHEST_OBJ_PRIORITY, RED, RED_INDEX, YELLOW, YELLOW_INDEX,
};
use crate::framebuffer::Framebuffer;
use crate::oam::{OamEntry, ObjShape};
use crate::palette::{Palette, Rgb888};
use crate::sprite::SpriteLayer;
use crate::tile::{BitDepth, Tileset};

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

    assert_eq!(fb.pixel(0, 0), Some(backdrop));
    assert_eq!(fb.pixel(1, 0), Some(RED.to_rgb888()));
    assert_eq!(fb.pixel(0, 1), Some(GREEN.to_rgb888()));
    assert_eq!(fb.pixel(1, 1), Some(BLUE.to_rgb888()));
}

fn corner_marked_tile() -> [u8; BPP4_TILE_BYTES] {
    let last_pixel = BitDepth::TILE_DIM - 1;
    bpp4_tile(&[
        ((0, 0), RED_INDEX),
        ((last_pixel, 0), GREEN_INDEX),
        ((0, last_pixel), BLUE_INDEX),
        ((last_pixel, last_pixel), YELLOW_INDEX),
    ])
}

fn corner_marked_palette() -> Palette {
    palette_with_colors(&[
        (RED_INDEX, RED),
        (GREEN_INDEX, GREEN),
        (BLUE_INDEX, BLUE),
        (YELLOW_INDEX, YELLOW),
    ])
}

#[test]
fn composite_h_flip_mirrors_the_whole_sprite_footprint() {
    let tileset = Tileset::decode(BitDepth::Bpp4, &corner_marked_tile()).unwrap();
    let palette = corner_marked_palette();
    let entries = [OamEntry::new(
        0,
        0,
        FIRST_TILE,
        FIRST_PALETTE_BANK,
        BitDepth::Bpp4,
        true,
        false,
        ObjShape::Square,
        EIGHT_PIXEL_SQUARE_SIZE,
        HIGHEST_OBJ_PRIORITY,
        true,
    )];
    let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

    let mut fb = Framebuffer::new();
    layer.composite(&mut fb);

    assert_eq!(fb.pixel(0, 0), Some(GREEN.to_rgb888()));
    assert_eq!(fb.pixel(7, 0), Some(RED.to_rgb888()));
}

#[test]
fn composite_v_flip_mirrors_the_whole_sprite_footprint() {
    let tileset = Tileset::decode(BitDepth::Bpp4, &corner_marked_tile()).unwrap();
    let palette = corner_marked_palette();
    let entries = [OamEntry::new(
        0,
        0,
        FIRST_TILE,
        FIRST_PALETTE_BANK,
        BitDepth::Bpp4,
        false,
        true,
        ObjShape::Square,
        EIGHT_PIXEL_SQUARE_SIZE,
        HIGHEST_OBJ_PRIORITY,
        true,
    )];
    let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

    let mut fb = Framebuffer::new();
    layer.composite(&mut fb);

    assert_eq!(fb.pixel(0, 0), Some(BLUE.to_rgb888()));
    assert_eq!(fb.pixel(0, 7), Some(RED.to_rgb888()));
}

#[test]
fn composite_clips_a_sprite_partially_off_the_left_edge() {
    const NEGATIVE_FOUR_RAW_X: u16 = 508;
    const SOURCE_X_AT_SCREEN_EDGE: usize = 4;

    // The whole first row is opaque, so a sampler that ran past the
    // sprite's width would paint the backdrop at screen x=4.
    let opaque_row: Vec<((usize, usize), u8)> = (0..BitDepth::TILE_DIM)
        .map(|x| ((x, 0), RED_INDEX))
        .collect();
    let tile = bpp4_tile(&opaque_row);
    let tileset = Tileset::decode(BitDepth::Bpp4, &tile).unwrap();
    let palette = palette_with_colors(&[(RED_INDEX, RED)]);
    let entries = [entry(NEGATIVE_FOUR_RAW_X, 0, true)];
    let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

    let mut fb = Framebuffer::new();
    layer.composite(&mut fb);

    assert_eq!(fb.pixel(0, 0), Some(RED.to_rgb888()));
    assert_eq!(
        fb.pixel(SOURCE_X_AT_SCREEN_EDGE - 1, 0),
        Some(RED.to_rgb888())
    );
    assert_eq!(fb.pixel(SOURCE_X_AT_SCREEN_EDGE, 0), Some(Rgb888::BLACK));
}

#[test]
fn composite_wraps_a_sprite_hanging_off_the_bottom_to_the_top() {
    const WRAPPED_RAW_Y: u8 = 250;
    const SOURCE_ROW_AT_SCREEN_TOP: usize = 6;
    const ROW_BELOW_WRAPPED_FOOTPRINT: usize = 5;
    const UNRELATED_SCREEN_ROW: usize = 100;

    let tile = bpp4_tile(&[((0, SOURCE_ROW_AT_SCREEN_TOP), RED_INDEX)]);
    let tileset = Tileset::decode(BitDepth::Bpp4, &tile).unwrap();
    let palette = palette_with_colors(&[(RED_INDEX, RED)]);
    let entries = [entry(0, WRAPPED_RAW_Y, true)];
    let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

    assert_eq!(
        layer.resolve_pixel(0, 0).map(|p| p.color),
        Some(RED.to_rgb888())
    );
    assert_eq!(layer.resolve_pixel(0, ROW_BELOW_WRAPPED_FOOTPRINT), None);
    assert_eq!(
        layer.resolve_pixel(0, UNRELATED_SCREEN_ROW),
        None,
        "Y wrapping creates one contiguous footprint"
    );
}

#[test]
fn composite_disabled_sprite_draws_nothing() {
    let tileset = Tileset::decode(BitDepth::Bpp4, &solid_4bpp_tile(RED_INDEX)).unwrap();
    let palette = palette_with_colors(&[(RED_INDEX, RED)]);
    let entries = [entry(0, 0, false)];
    let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

    assert_eq!(layer.resolve_pixel(0, 0), None);
}
