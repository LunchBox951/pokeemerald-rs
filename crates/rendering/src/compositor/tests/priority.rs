//! Pins cross-layer priority composition: BG-vs-BG, sprite-vs-BG, and affine-BG ordering.

use super::super::{compose_frame, BgSlot};
use super::shared::{empty_sprite_layer, opaque_affine_bg_fixture, opaque_bg_fixture};
use crate::affine::AffineMatrix;
use crate::bg_affine::{AffineBgLayer, Overflow};
use crate::oam::{OamEntry, ObjShape};
use crate::palette::{Bgr555, Palette};
use crate::sprite::SpriteLayer;
use crate::tile::{BitDepth, Tileset};

#[test]
fn bg_vs_bg_lower_priority_number_wins() {
    let (tiles_x, palette_x, map_x) = opaque_bg_fixture(1);
    let (tiles_y, palette_y, map_y) = opaque_bg_fixture(2);
    let layer_a = crate::bg::BgLayer::new(&tiles_x, &palette_x, &map_x);
    let layer_b = crate::bg::BgLayer::new(&tiles_y, &palette_y, &map_y);

    let slots = [
        BgSlot::new(layer_a, 1, 3, 0, 0, true),
        BgSlot::new(layer_b, 0, 0, 0, 0, true),
    ];
    let entries: [OamEntry; 0] = [];
    let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
    let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);

    let fb = compose_frame(&sprites, &slots);
    assert_eq!(
        fb.pixel(0, 0),
        Some(Bgr555::from_channels(2, 0, 0).to_rgb888())
    );
}

#[test]
fn bg_vs_bg_same_priority_lower_bg_index_wins() {
    let (tiles_x, palette_x, map_x) = opaque_bg_fixture(1);
    let (tiles_y, palette_y, map_y) = opaque_bg_fixture(2);
    let layer_a = crate::bg::BgLayer::new(&tiles_x, &palette_x, &map_x);
    let layer_b = crate::bg::BgLayer::new(&tiles_y, &palette_y, &map_y);

    let slots = [
        BgSlot::new(layer_a, 2, 1, 0, 0, true),
        BgSlot::new(layer_b, 0, 1, 0, 0, true),
    ];
    let entries: [OamEntry; 0] = [];
    let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
    let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);

    let fb = compose_frame(&sprites, &slots);
    assert_eq!(
        fb.pixel(0, 0),
        Some(Bgr555::from_channels(2, 0, 0).to_rgb888())
    );
}

#[test]
fn disabled_bg_slot_contributes_nothing() {
    let (ts, pal, tm) = opaque_bg_fixture(9);
    let layer = crate::bg::BgLayer::new(&ts, &pal, &tm);
    let slots = [BgSlot::new(layer, 0, 0, 0, 0, false)];
    let entries: [OamEntry; 0] = [];
    let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
    let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);

    let fb = compose_frame(&sprites, &slots);
    assert_eq!(fb.pixel(0, 0), Some(crate::palette::Rgb888::BLACK));
}

#[test]
fn sprite_vs_bg_same_priority_sprite_wins() {
    let (ts, pal, tm) = opaque_bg_fixture(1);
    let bg_layer = crate::bg::BgLayer::new(&ts, &pal, &tm);
    let slots = [BgSlot::new(bg_layer, 0, 2, 0, 0, true)];

    let sprite_tileset = Tileset::decode(BitDepth::Bpp4, &[0xFFu8; 32]).unwrap();
    let mut sprite_colors = [Bgr555::default(); Palette::LEN];
    sprite_colors[15] = Bgr555::from_channels(0, 9, 0);
    let sprite_palette = Palette::new(sprite_colors);
    let entries = [OamEntry::new(
        0,
        0,
        0,
        0,
        BitDepth::Bpp4,
        false,
        false,
        ObjShape::Square,
        0,
        2, // same priority as the BG
        true,
    )];
    let sprites = SpriteLayer::new(&entries, &sprite_tileset, &sprite_tileset, &sprite_palette);

    let fb = compose_frame(&sprites, &slots);
    assert_eq!(
        fb.pixel(0, 0),
        Some(Bgr555::from_channels(0, 9, 0).to_rgb888())
    );
}

#[test]
fn bg_with_better_priority_number_beats_a_sprite() {
    let (ts, pal, tm) = opaque_bg_fixture(1);
    let bg_layer = crate::bg::BgLayer::new(&ts, &pal, &tm);
    let slots = [BgSlot::new(bg_layer, 0, 0, 0, 0, true)];

    let sprite_tileset = Tileset::decode(BitDepth::Bpp4, &[0xFFu8; 32]).unwrap();
    let mut sprite_colors = [Bgr555::default(); Palette::LEN];
    sprite_colors[15] = Bgr555::from_channels(0, 9, 0);
    let sprite_palette = Palette::new(sprite_colors);
    // The sprite's priority is worse than the BG's, so the BG must win.
    let entries = [OamEntry::new(
        0,
        0,
        0,
        0,
        BitDepth::Bpp4,
        false,
        false,
        ObjShape::Square,
        0,
        3,
        true,
    )];
    let sprites = SpriteLayer::new(&entries, &sprite_tileset, &sprite_tileset, &sprite_palette);

    let fb = compose_frame(&sprites, &slots);
    assert_eq!(
        fb.pixel(0, 0),
        Some(Bgr555::from_channels(1, 0, 0).to_rgb888())
    );
}

#[test]
fn transparent_sprite_pixel_lets_the_bg_show_through() {
    let (ts, pal, tm) = opaque_bg_fixture(4);
    let bg_layer = crate::bg::BgLayer::new(&ts, &pal, &tm);
    let slots = [BgSlot::new(bg_layer, 0, 3, 0, 0, true)];

    // 0x00 fills every texel with palette index 0 (transparent).
    let sprite_tileset = Tileset::decode(BitDepth::Bpp4, &[0x00u8; 32]).unwrap();
    let sprite_palette = Palette::new([Bgr555::default(); Palette::LEN]);
    let entries = [OamEntry::new(
        0,
        0,
        0,
        0,
        BitDepth::Bpp4,
        false,
        false,
        ObjShape::Square,
        0,
        0,
        true,
    )];
    let sprites = SpriteLayer::new(&entries, &sprite_tileset, &sprite_tileset, &sprite_palette);

    let fb = compose_frame(&sprites, &slots);
    assert_eq!(
        fb.pixel(0, 0),
        Some(Bgr555::from_channels(4, 0, 0).to_rgb888())
    );
}

#[test]
fn better_sprites_transparent_hole_promotes_a_worse_sprite_over_the_bg() {
    // B (OAM index 0, opaque, priority 2) sits under A (OAM index 1,
    // transparent, priority 0), with the BG between them at priority 1.
    // On hardware, A's transparent texel upgrades B's already-stored OBJ
    // order to priority 0, so the OBJ layer (still B's color) beats the
    // BG even though B's own priority (2) is worse than the BG's (1)
    // (`mgba/src/gba/renderers/software-obj.c:76-85,116-125`).
    const OPAQUE_TILE: u16 = 0;
    const TRANSPARENT_TILE: u16 = 1;
    const OPAQUE_PALETTE_INDEX: u8 = 15;
    const B_COLOR: Bgr555 = Bgr555::from_channels(0, 0, 9);

    let (ts, pal, tm) = opaque_bg_fixture(7);
    let bg_layer = crate::bg::BgLayer::new(&ts, &pal, &tm);
    let slots = [BgSlot::new(bg_layer, 0, 1, 0, 0, true)];

    let mut two_tiles = [0u8; 64];
    two_tiles[..32].fill(0xFF);
    let shared = Tileset::decode(BitDepth::Bpp4, &two_tiles).unwrap();
    let mut colors = [Bgr555::default(); Palette::LEN];
    colors[usize::from(OPAQUE_PALETTE_INDEX)] = B_COLOR;
    let palette = Palette::new(colors);

    let b_opaque_prio2 = OamEntry::new(
        0,
        0,
        OPAQUE_TILE,
        0,
        BitDepth::Bpp4,
        false,
        false,
        ObjShape::Square,
        0,
        2,
        true,
    );
    let a_transparent_prio0 = OamEntry::new(
        0,
        0,
        TRANSPARENT_TILE,
        0,
        BitDepth::Bpp4,
        false,
        false,
        ObjShape::Square,
        0,
        0,
        true,
    );
    let entries = [b_opaque_prio2, a_transparent_prio0];
    let sprites = SpriteLayer::new(&entries, &shared, &shared, &palette);

    let fb = compose_frame(&sprites, &slots);
    assert_eq!(
        fb.pixel(0, 0),
        Some(B_COLOR.to_rgb888()),
        "B's color must beat the BG because A's hole upgrades it to priority 0"
    );
}

#[test]
fn pixel_with_no_opaque_layer_stays_at_the_backdrop() {
    let entries: [OamEntry; 0] = [];
    let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
    let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);
    let fb = compose_frame(&sprites, &[]);
    assert_eq!(fb.pixel(0, 0), Some(crate::palette::Rgb888::BLACK));
}

#[test]
fn affine_bg_slot_participates_in_priority_ordering_like_a_regular_bg() {
    // Affine slots share compose_frame's ordering, not a separate path.
    let (affine_tiles, affine_palette, affine_tilemap) = opaque_affine_bg_fixture(9);
    let affine_layer = AffineBgLayer::new(&affine_tiles, &affine_palette, &affine_tilemap);
    let (regular_tiles, regular_palette, regular_map) = opaque_bg_fixture(1);
    let regular_layer = crate::bg::BgLayer::new(&regular_tiles, &regular_palette, &regular_map);

    let slots = [
        BgSlot::new_affine(
            affine_layer,
            0,
            0, // best priority
            AffineMatrix::IDENTITY,
            0,
            0,
            Overflow::Transparent,
            true,
        ),
        BgSlot::new(regular_layer, 1, 1, 0, 0, true), // worse priority
    ];
    let entries: [OamEntry; 0] = [];
    let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
    let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);

    let fb = compose_frame(&sprites, &slots);
    assert_eq!(
        fb.pixel(0, 0),
        Some(Bgr555::from_channels(9, 0, 0).to_rgb888()),
        "the affine BG's better priority must win"
    );
}

#[test]
fn disabled_affine_bg_slot_contributes_nothing() {
    let (tiles, palette, tilemap) = opaque_affine_bg_fixture(9);
    let layer = AffineBgLayer::new(&tiles, &palette, &tilemap);
    let slots = [BgSlot::new_affine(
        layer,
        0,
        0,
        AffineMatrix::IDENTITY,
        0,
        0,
        Overflow::Transparent,
        false, // disabled
    )];
    let entries: [OamEntry; 0] = [];
    let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
    let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);

    let fb = compose_frame(&sprites, &slots);
    assert_eq!(fb.pixel(0, 0), Some(crate::palette::Rgb888::BLACK));
}
