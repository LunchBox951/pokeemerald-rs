//! Pins 8bpp sprite compositing against the flat palette.

use super::shared::{
    palette_with_colors, BPP4_TILE_BYTES, EIGHT_PIXEL_SQUARE_SIZE, FIRST_TILE, GREEN, GREEN_INDEX,
    HIGHEST_OBJ_PRIORITY, RED, RED_INDEX,
};
use crate::oam::{OamEntry, ObjShape};
use crate::sprite::SpriteLayer;
use crate::tile::{BitDepth, Tileset};

#[test]
fn composite_8bpp_sprite_uses_the_flat_palette_and_its_own_tileset() {
    const FLAT_PALETTE_INDEX: u8 = 200;
    const IGNORED_4BPP_PALETTE_BANK: u8 = 3;

    let mut bytes = [0u8; BitDepth::Bpp8.tile_byte_len()];
    bytes[0] = FLAT_PALETTE_INDEX;
    let tileset_8bpp = Tileset::decode(BitDepth::Bpp8, &bytes).unwrap();
    let empty_4bpp_tile = [0u8; BPP4_TILE_BYTES];
    let tileset_4bpp = Tileset::decode(BitDepth::Bpp4, &empty_4bpp_tile).unwrap();
    let palette = palette_with_colors(&[(FLAT_PALETTE_INDEX, RED)]);

    let entries = [OamEntry::new(
        0,
        0,
        FIRST_TILE,
        IGNORED_4BPP_PALETTE_BANK,
        BitDepth::Bpp8,
        false,
        false,
        ObjShape::Square,
        EIGHT_PIXEL_SQUARE_SIZE,
        HIGHEST_OBJ_PRIORITY,
        true,
    )];
    let layer = SpriteLayer::new(&entries, &tileset_4bpp, &tileset_8bpp, &palette);

    assert_eq!(
        layer.resolve_pixel(0, 0).map(|p| p.color),
        Some(RED.to_rgb888())
    );
}

/// Regular sampling applies the half-tile offset that
/// [`Tileset::obj_pixel_index`] defines, across the decoded tile
/// boundary.
#[test]
fn composite_8bpp_sprite_honours_a_half_tile_offset_across_the_decoded_tile_boundary() {
    const IGNORED_4BPP_PALETTE_BANK: u8 = 3;
    const UNUSED_FIRST_TILE_TOP_HALF: u8 = 9;
    const HALF_TILE_ROW_BYTES: usize = BitDepth::TILE_DIM * (BitDepth::TILE_DIM / 2);

    let mut bytes = [0u8; 2 * BitDepth::Bpp8.tile_byte_len()];
    bytes[0] = UNUSED_FIRST_TILE_TOP_HALF;
    bytes[HALF_TILE_ROW_BYTES] = GREEN_INDEX;
    bytes[BitDepth::Bpp8.tile_byte_len()] = RED_INDEX;
    let tileset_8bpp = Tileset::decode(BitDepth::Bpp8, &bytes).unwrap();
    let empty_4bpp_tile = [0u8; BPP4_TILE_BYTES];
    let tileset_4bpp = Tileset::decode(BitDepth::Bpp4, &empty_4bpp_tile).unwrap();
    let palette = palette_with_colors(&[(RED_INDEX, RED), (GREEN_INDEX, GREEN)]);

    let entries = [OamEntry::new(
        0,
        0,
        0,
        IGNORED_4BPP_PALETTE_BANK,
        BitDepth::Bpp8,
        false,
        false,
        ObjShape::Square,
        EIGHT_PIXEL_SQUARE_SIZE,
        HIGHEST_OBJ_PRIORITY,
        true,
    )
    .with_half_tile_offset(true)];
    let layer = SpriteLayer::new(&entries, &tileset_4bpp, &tileset_8bpp, &palette);

    assert_eq!(
        layer.resolve_pixel(0, 0).map(|p| p.color),
        Some(GREEN.to_rgb888()),
        "row 0 of the offset sprite is row 4 of decoded tile 0"
    );
    assert_eq!(
        layer.resolve_pixel(0, 4).map(|p| p.color),
        Some(RED.to_rgb888()),
        "row 4 of the offset sprite is row 0 of decoded tile 1"
    );
}
