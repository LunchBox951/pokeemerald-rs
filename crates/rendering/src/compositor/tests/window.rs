//! Pins hardware window gating: BG-region masks and OBJWIN-mode sprite masks.

use super::super::{compose_frame, compose_frame_with_effects, BgSlot, FrameEffects};
use super::shared::{bpp4_tile_with_every_row, empty_sprite_layer, opaque_bg_fixture};
use crate::oam::{OamEntry, ObjMode, ObjShape};
use crate::palette::{Bgr555, Palette};
use crate::sprite::SpriteLayer;
use crate::tile::{BitDepth, Tileset};
use crate::window::{WindowConfig, WindowLayerEnable, WindowRange, WindowRect};

#[test]
fn no_effects_default_reproduces_compose_frame_byte_for_byte() {
    let (tiles_a, palette_a, map_a) = opaque_bg_fixture(3);
    let (tiles_b, palette_b, map_b) = opaque_bg_fixture(6);
    let layer_a = crate::bg::BgLayer::new(&tiles_a, &palette_a, &map_a);
    let layer_b = crate::bg::BgLayer::new(&tiles_b, &palette_b, &map_b);
    let slots = [
        BgSlot::new(layer_a, 0, 1, 0, 0, true),
        BgSlot::new(layer_b, 1, 0, 0, 0, true),
    ];

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
        2,
        true,
    )];
    let sprites = SpriteLayer::new(&entries, &sprite_tileset, &sprite_tileset, &sprite_palette);

    let via_compose_frame = compose_frame(&sprites, &slots);
    let via_effects_default =
        compose_frame_with_effects(&sprites, &slots, &FrameEffects::default());
    assert_eq!(via_compose_frame.pixels(), via_effects_default.pixels());
}

#[test]
fn window_gates_a_bg_layer_by_region_even_though_its_own_enable_bit_is_on() {
    let (tiles_r, palette_r, map_r) = opaque_bg_fixture(9);
    let (tiles_b, _palette_b, map_b) = opaque_bg_fixture(0);
    let layer_r = crate::bg::BgLayer::new(&tiles_r, &palette_r, &map_r);
    let mut blue_colors = [Bgr555::default(); Palette::LEN];
    blue_colors[15] = Bgr555::from_channels(0, 0, 9);
    let blue_palette = Palette::new(blue_colors);
    let layer_b = crate::bg::BgLayer::new(&tiles_b, &blue_palette, &map_b);
    let slots = [
        BgSlot::new(layer_r, 0, 0, 0, 0, true),
        BgSlot::new(layer_b, 1, 1, 0, 0, true),
    ];
    let entries: [OamEntry; 0] = [];
    let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
    let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);

    let mut bg0_only = WindowLayerEnable::NONE;
    bg0_only.bg[0] = true;
    let mut bg1_only = WindowLayerEnable::NONE;
    bg1_only.bg[1] = true;

    let effects = FrameEffects {
        windows: WindowConfig {
            win0: Some((
                WindowRect::new(WindowRange::new(0, 4), WindowRange::new(0, 8)),
                bg0_only,
            )),
            win1: None,
            obj_window: None,
            winout: bg1_only,
        },
        ..FrameEffects::default()
    };

    let fb = compose_frame_with_effects(&sprites, &slots, &effects);
    assert_eq!(
        fb.pixel(0, 0),
        Some(Bgr555::from_channels(9, 0, 0).to_rgb888()),
        "inside WIN0, only BG0 is enabled"
    );
    assert_eq!(
        fb.pixel(5, 0),
        Some(Bgr555::from_channels(0, 0, 9).to_rgb888()),
        "outside every window (WINOUT), only BG1 is enabled, despite BG0's better priority"
    );
}

#[test]
fn objwin_mode_sprite_gates_a_layer_and_never_draws_its_own_color() {
    let (tiles, palette, map) = opaque_bg_fixture(9);
    let layer = crate::bg::BgLayer::new(&tiles, &palette, &map);
    let slots = [BgSlot::new(layer, 0, 0, 0, 0, true)];

    let mask_tile = bpp4_tile_with_every_row([0, 0, 0, 0, 15, 15, 15, 15]);
    let mask_tileset = Tileset::decode(BitDepth::Bpp4, &mask_tile).unwrap();
    let mut mask_colors = [Bgr555::default(); Palette::LEN];
    mask_colors[15] = Bgr555::from_channels(0, 31, 31);
    let mask_palette = Palette::new(mask_colors);
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
    )
    .with_mode(ObjMode::Window)];
    let sprites = SpriteLayer::new(&entries, &mask_tileset, &mask_tileset, &mask_palette);

    let mut bg0_only = WindowLayerEnable::NONE;
    bg0_only.bg[0] = true;

    let effects = FrameEffects {
        windows: WindowConfig {
            win0: None,
            win1: None,
            obj_window: Some(bg0_only),
            winout: WindowLayerEnable::NONE,
        },
        ..FrameEffects::default()
    };

    let fb = compose_frame_with_effects(&sprites, &slots, &effects);
    assert_eq!(
        fb.pixel(1, 0),
        Some(crate::palette::Rgb888::BLACK),
        "outside the OBJWIN mask, BG0 must stay disabled"
    );
    assert_eq!(
        fb.pixel(5, 0),
        Some(Bgr555::from_channels(9, 0, 0).to_rgb888()),
        "inside the OBJWIN mask, BG0 must be enabled"
    );
    assert_ne!(
        fb.pixel(5, 0),
        Some(Bgr555::from_channels(0, 31, 31).to_rgb888()),
        "the mask sprite itself must never draw a visible pixel"
    );
}
