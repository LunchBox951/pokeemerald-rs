//! Pins multi-tile footprint addressing and the OBJ VRAM tile-index wrap.

use super::shared::{
    palette_with_colors, BPP4_TILE_BYTES, FIRST_PALETTE_BANK, GREEN, GREEN_INDEX,
    HIGHEST_OBJ_PRIORITY, RED, RED_INDEX,
};
use crate::oam::{OamEntry, ObjShape};
use crate::sprite::SpriteLayer;
use crate::tile::{BitDepth, Tileset};

#[test]
fn multi_tile_sprite_addresses_tiles_row_major_from_its_base_index() {
    const BASE_TILE: u16 = 5;
    const NEXT_TILE: u16 = BASE_TILE + 1;
    const HORIZONTAL_16_BY_8_SIZE: u8 = 0;

    let tile_count = usize::from(NEXT_TILE) + 1;
    let mut tiles = vec![0u8; BPP4_TILE_BYTES * tile_count];
    let base_start = usize::from(BASE_TILE) * BPP4_TILE_BYTES;
    let next_start = usize::from(NEXT_TILE) * BPP4_TILE_BYTES;
    tiles[base_start] = RED_INDEX;
    tiles[next_start] = GREEN_INDEX;
    let tileset = Tileset::decode(BitDepth::Bpp4, &tiles).unwrap();
    let palette = palette_with_colors(&[(RED_INDEX, RED), (GREEN_INDEX, GREEN)]);

    let entries = [OamEntry::new(
        0,
        0,
        BASE_TILE,
        FIRST_PALETTE_BANK,
        BitDepth::Bpp4,
        false,
        false,
        ObjShape::Horizontal,
        HORIZONTAL_16_BY_8_SIZE,
        HIGHEST_OBJ_PRIORITY,
        true,
    )];
    let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

    assert_eq!(
        layer.resolve_pixel(0, 0).map(|p| p.color),
        Some(RED.to_rgb888())
    );
    assert_eq!(
        layer.resolve_pixel(8, 0).map(|p| p.color),
        Some(GREEN.to_rgb888())
    );
}

#[test]
fn multi_tile_sprite_wraps_a_4bpp_tile_index_off_the_end_of_obj_vram() {
    const HORIZONTAL_16_BY_8_SIZE: u8 = 0;

    let bit_depth = BitDepth::Bpp4;
    let tile_count = usize::from(bit_depth.obj_tile_index_mask()) + 1;
    let last_tile = bit_depth.obj_tile_index_mask();
    let last_tile_start = usize::from(last_tile) * BPP4_TILE_BYTES;
    let mut tiles = vec![0u8; BPP4_TILE_BYTES * tile_count];
    tiles[0] = GREEN_INDEX;
    tiles[last_tile_start] = RED_INDEX;
    let tileset = Tileset::decode(bit_depth, &tiles).unwrap();
    let palette = palette_with_colors(&[(RED_INDEX, RED), (GREEN_INDEX, GREEN)]);

    let entries = [OamEntry::new(
        0,
        0,
        last_tile,
        FIRST_PALETTE_BANK,
        bit_depth,
        false,
        false,
        ObjShape::Horizontal,
        HORIZONTAL_16_BY_8_SIZE,
        HIGHEST_OBJ_PRIORITY,
        true,
    )];
    let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

    assert_eq!(
        layer.resolve_pixel(0, 0).map(|p| p.color),
        Some(RED.to_rgb888())
    );
    assert_eq!(
        layer.resolve_pixel(8, 0).map(|p| p.color),
        Some(GREEN.to_rgb888())
    );
}

#[test]
fn multi_tile_sprite_wraps_an_8bpp_tile_index_off_the_end_of_obj_vram() {
    const HORIZONTAL_16_BY_8_SIZE: u8 = 0;

    let bit_depth = BitDepth::Bpp8;
    let tile_bytes = bit_depth.tile_byte_len();
    let tile_count = usize::from(bit_depth.obj_tile_index_mask()) + 1;
    let last_tile = bit_depth.obj_tile_index_mask();
    let last_tile_start = usize::from(last_tile) * tile_bytes;
    let mut tiles = vec![0u8; tile_bytes * tile_count];
    tiles[0] = GREEN_INDEX;
    tiles[last_tile_start] = RED_INDEX;
    let tileset = Tileset::decode(bit_depth, &tiles).unwrap();
    let palette = palette_with_colors(&[(RED_INDEX, RED), (GREEN_INDEX, GREEN)]);

    let entries = [OamEntry::new(
        0,
        0,
        last_tile,
        FIRST_PALETTE_BANK,
        bit_depth,
        false,
        false,
        ObjShape::Horizontal,
        HORIZONTAL_16_BY_8_SIZE,
        HIGHEST_OBJ_PRIORITY,
        true,
    )];
    let empty_4bpp_tile = [0u8; BPP4_TILE_BYTES];
    let tileset_4bpp = Tileset::decode(BitDepth::Bpp4, &empty_4bpp_tile).unwrap();
    let layer = SpriteLayer::new(&entries, &tileset_4bpp, &tileset, &palette);

    assert_eq!(
        layer.resolve_pixel(0, 0).map(|p| p.color),
        Some(RED.to_rgb888())
    );
    assert_eq!(
        layer.resolve_pixel(8, 0).map(|p| p.color),
        Some(GREEN.to_rgb888())
    );
}
