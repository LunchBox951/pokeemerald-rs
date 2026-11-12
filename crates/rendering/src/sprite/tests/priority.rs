//! Pins sprite-vs-sprite priority resolution and OAM-index ties.

use super::shared::{
    opaque_and_transparent_tiles, square_8x8, FIRST_TILE, OPAQUE_PALETTE_INDEX, TRANSPARENT_TILE,
};
use crate::palette::{Bgr555, Palette};
use crate::sprite::SpriteLayer;
use crate::tile::{BitDepth, Tileset};

#[test]
fn resolve_pixel_sprite_vs_sprite_lower_oam_index_wins_a_priority_tie() {
    let tileset = Tileset::decode(BitDepth::Bpp4, &[0xFFu8; 32]).unwrap();
    let mut colors = [Bgr555::default(); Palette::LEN];
    let bank_zero_opaque_index = OPAQUE_PALETTE_INDEX as usize;
    let bank_one_opaque_index = Palette::BANK_LEN + bank_zero_opaque_index;
    colors[bank_zero_opaque_index] = Bgr555::from_channels(0x1F, 0, 0);
    colors[bank_one_opaque_index] = Bgr555::from_channels(0, 0x1F, 0);
    let palette = Palette::new(colors);

    let low_index_red = square_8x8(FIRST_TILE, 2, 0);
    let high_index_green = square_8x8(FIRST_TILE, 2, 1);
    let entries = [low_index_red, high_index_green];
    let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

    assert_eq!(
        layer.resolve_pixel(0, 0).map(|p| p.color),
        Some(colors[bank_zero_opaque_index].to_rgb888())
    );

    let entries_reversed = [high_index_green, low_index_red];
    let layer_reversed = SpriteLayer::new(&entries_reversed, &tileset, &tileset, &palette);
    assert_eq!(
        layer_reversed.resolve_pixel(0, 0).map(|p| p.color),
        Some(colors[bank_one_opaque_index].to_rgb888())
    );
}

#[test]
fn resolve_pixel_lower_priority_number_wins_regardless_of_oam_index() {
    let tileset = Tileset::decode(BitDepth::Bpp4, &[0xFFu8; 32]).unwrap();
    let mut colors = [Bgr555::default(); Palette::LEN];
    let bank_zero_opaque_index = OPAQUE_PALETTE_INDEX as usize;
    let bank_one_opaque_index = Palette::BANK_LEN + bank_zero_opaque_index;
    colors[bank_zero_opaque_index] = Bgr555::from_channels(0x1F, 0, 0);
    colors[bank_one_opaque_index] = Bgr555::from_channels(0, 0x1F, 0);
    let palette = Palette::new(colors);

    let worse_priority_red = square_8x8(FIRST_TILE, 3, 0);
    let better_priority_green = square_8x8(FIRST_TILE, 0, 1);
    let entries = [worse_priority_red, better_priority_green];
    let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

    assert_eq!(
        layer.resolve_pixel(0, 0).map(|p| p.color),
        Some(colors[bank_one_opaque_index].to_rgb888())
    );
}

#[test]
fn resolve_pixel_better_sprites_hole_over_opaque_worse_sprite_upgrades_priority() {
    let (tileset, palette) = opaque_and_transparent_tiles();
    let opaque_priority_two = square_8x8(FIRST_TILE, 2, 0);
    let transparent_priority_zero = square_8x8(TRANSPARENT_TILE, 0, 0);
    let entries = [opaque_priority_two, transparent_priority_zero];
    let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

    let pixel = layer.resolve_pixel(0, 0).unwrap();
    assert_eq!(pixel.color, Bgr555::from_channels(0, 0, 0x1F).to_rgb888());
    assert_eq!(pixel.priority, 0);
}

#[test]
fn resolve_pixel_better_transparent_sprite_before_opaque_worse_does_not_upgrade() {
    let (tileset, palette) = opaque_and_transparent_tiles();
    let transparent_priority_zero = square_8x8(TRANSPARENT_TILE, 0, 0);
    let opaque_priority_two = square_8x8(FIRST_TILE, 2, 0);
    let entries = [transparent_priority_zero, opaque_priority_two];
    let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

    let pixel = layer.resolve_pixel(0, 0).unwrap();
    assert_eq!(pixel.color, Bgr555::from_channels(0, 0, 0x1F).to_rgb888());
    assert_eq!(pixel.priority, 2);
}
