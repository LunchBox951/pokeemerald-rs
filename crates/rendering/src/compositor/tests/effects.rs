//! Pins color special-effect blending (alpha, brighten, backdrop, OBJWIN reblend) end to end.

use super::super::{compose_frame, compose_frame_with_effects, BgSlot, FrameEffects};
use super::shared::{empty_sprite_layer, opaque_bg_fixture};
use crate::effects::{ColorEffect, EffectsConfig, LayerTargets};
use crate::oam::{OamEntry, ObjMode, ObjShape};
use crate::palette::{Bgr555, Palette, Rgb888};
use crate::sprite::SpriteLayer;
use crate::tile::{BitDepth, Tileset};

#[test]
fn alpha_blend_end_to_end_over_two_bg_layers() {
    // eva=evb=8 (50/50) blend of r channels 0 and 255: (0*8+255*8)/16 = 127.
    let (tiles_a, palette_a, map_a) = opaque_bg_fixture(0);
    let (tiles_b, palette_b, map_b) = opaque_bg_fixture(31);
    let layer_a = crate::bg::BgLayer::new(&tiles_a, &palette_a, &map_a);
    let layer_b = crate::bg::BgLayer::new(&tiles_b, &palette_b, &map_b);
    let slots = [
        BgSlot::new(layer_a, 0, 0, 0, 0, true),
        BgSlot::new(layer_b, 1, 1, 0, 0, true),
    ];
    let entries: [OamEntry; 0] = [];
    let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
    let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);

    let effects = FrameEffects {
        color: EffectsConfig {
            effect: ColorEffect::AlphaBlend,
            target1: LayerTargets {
                bg: [true, false, false, false],
                obj: false,
                backdrop: false,
            },
            target2: LayerTargets {
                bg: [false, true, false, false],
                obj: false,
                backdrop: false,
            },
            eva: 8,
            evb: 8,
            evy: 0,
        },
        ..FrameEffects::default()
    };
    let fb = compose_frame_with_effects(&sprites, &slots, &effects);
    assert_eq!(fb.pixel(0, 0), Some(Rgb888 { r: 127, g: 0, b: 0 }));
}

#[test]
fn semi_transparent_obj_forces_blend_end_to_end_overriding_brighten() {
    // OBJ is not configured target1 here, but a semi-transparent OBJ
    // still forces alpha blend regardless of BLDCNT's selected effect
    // (OamEntry::with_mode's contract).
    let (tiles, palette, map) = opaque_bg_fixture(31);
    let bg_layer = crate::bg::BgLayer::new(&tiles, &palette, &map);
    let slots = [BgSlot::new(bg_layer, 0, 1, 0, 0, true)];

    let sprite_tileset = Tileset::decode(BitDepth::Bpp4, &[0xFFu8; 32]).unwrap();
    let mut sprite_colors = [Bgr555::default(); Palette::LEN];
    sprite_colors[15] = Bgr555::from_channels(0, 0, 0);
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
        0,
        true,
    )
    .with_mode(ObjMode::SemiTransparent)];
    let sprites = SpriteLayer::new(&entries, &sprite_tileset, &sprite_tileset, &sprite_palette);

    let effects = FrameEffects {
        color: EffectsConfig {
            effect: ColorEffect::Brighten,
            target1: LayerTargets::default(), // OBJ not configured target1
            target2: LayerTargets {
                bg: [true, false, false, false],
                obj: false,
                backdrop: false,
            },
            eva: 8,
            evb: 8,
            evy: 16,
        },
        ..FrameEffects::default()
    };
    let fb = compose_frame_with_effects(&sprites, &slots, &effects);
    // eva=evb=8 blend of the sprite's r=0 and the BG's r=255:
    // (0*8+255*8)/16 = 127.
    assert_eq!(
        fb.pixel(0, 0),
        Some(Rgb888 { r: 127, g: 0, b: 0 }),
        "semi-transparency must force alpha blend even though BRIGHTEN was selected"
    );
}

#[test]
fn brighten_end_to_end_over_a_single_bg_layer() {
    let (tiles, palette, map) = opaque_bg_fixture(0);
    let layer = crate::bg::BgLayer::new(&tiles, &palette, &map);
    let slots = [BgSlot::new(layer, 0, 0, 0, 0, true)];
    let entries: [OamEntry; 0] = [];
    let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
    let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);

    let effects = FrameEffects {
        color: EffectsConfig {
            effect: ColorEffect::Brighten,
            target1: LayerTargets {
                bg: [true, false, false, false],
                obj: false,
                backdrop: false,
            },
            target2: LayerTargets::default(),
            eva: 0,
            evb: 0,
            evy: 16,
        },
        ..FrameEffects::default()
    };
    let fb = compose_frame_with_effects(&sprites, &slots, &effects);
    assert_eq!(
        fb.pixel(0, 0),
        Some(Bgr555::from_channels(31, 31, 31).to_rgb888()),
        "full brighten (evy=16) of black must reach white"
    );
}

#[test]
fn backdrop_blending_end_to_end_when_nothing_is_behind_the_front_layer() {
    let (tiles, palette, map) = opaque_bg_fixture(0);
    let layer = crate::bg::BgLayer::new(&tiles, &palette, &map);
    let slots = [BgSlot::new(layer, 0, 0, 0, 0, true)];
    let entries: [OamEntry; 0] = [];
    let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
    let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);

    let effects = FrameEffects {
        color: EffectsConfig {
            effect: ColorEffect::AlphaBlend,
            target1: LayerTargets {
                bg: [true, false, false, false],
                obj: false,
                backdrop: false,
            },
            target2: LayerTargets {
                bg: [false; 4],
                obj: false,
                backdrop: true,
            },
            eva: 8,
            evb: 8,
            evy: 0,
        },
        backdrop: Bgr555::from_channels(31, 31, 31).to_rgb888(),
        ..FrameEffects::default()
    };
    let fb = compose_frame_with_effects(&sprites, &slots, &effects);
    // eva=evb=8 blend of black (0,0,0) and white (255,255,255) per
    // channel: (0*8+255*8)/16 = 127 on every channel.
    assert_eq!(
        fb.pixel(0, 0),
        Some(Rgb888 {
            r: 127,
            g: 127,
            b: 127
        }),
        "BG blended 50/50 with the white backdrop"
    );
}

#[test]
fn objwin_transparent_hole_promotes_a_worse_sprite_over_the_bg() {
    // Opaque sprite B (priority 2) sits under a priority-0 OBJWIN-mode
    // sprite whose texel here is a transparent hole, with a BG between
    // them at priority 1. Per SpritePixel's flag-only-overwrite contract,
    // that transparent texel upgrades B's stored priority to 0 without
    // replacing its color, so the OBJ layer beats the BG despite B's own
    // priority (2) being worse than the BG's (1).
    let (ts, pal, tm) = opaque_bg_fixture(7);
    let bg_layer = crate::bg::BgLayer::new(&ts, &pal, &tm);
    let slots = [BgSlot::new(bg_layer, 0, 1, 0, 0, true)]; // BG priority 1

    // tile 0 fully opaque (B), tile 1 fully transparent (the OBJWIN hole).
    let mut two_tiles = [0u8; 64];
    two_tiles[..32].copy_from_slice(&[0xFFu8; 32]);
    let shared = Tileset::decode(BitDepth::Bpp4, &two_tiles).unwrap();
    let mut colors = [Bgr555::default(); Palette::LEN];
    colors[15] = Bgr555::from_channels(0, 0, 9); // B's color (blue)
    let palette = Palette::new(colors);

    let b_opaque_prio2 = OamEntry::new(
        0,
        0,
        0, // tile 0 (opaque)
        0,
        BitDepth::Bpp4,
        false,
        false,
        ObjShape::Square,
        0,
        2,
        true,
    );
    let objwin_hole_prio0 = OamEntry::new(
        0,
        0,
        1, // tile 1 (transparent)
        0,
        BitDepth::Bpp4,
        false,
        false,
        ObjShape::Square,
        0,
        0,
        true,
    )
    .with_mode(ObjMode::Window);

    let entries = [b_opaque_prio2, objwin_hole_prio0];
    let sprites = SpriteLayer::new(&entries, &shared, &shared, &palette);
    let fb = compose_frame(&sprites, &slots);
    assert_eq!(
        fb.pixel(0, 0),
        Some(Bgr555::from_channels(0, 0, 9).to_rgb888()),
        "the OBJWIN hole upgrades B to priority 0, beating the BG"
    );

    let control_entries = [b_opaque_prio2];
    let control_sprites = SpriteLayer::new(&control_entries, &shared, &shared, &palette);
    let control_fb = compose_frame(&control_sprites, &slots);
    assert_eq!(
        control_fb.pixel(0, 0),
        Some(Bgr555::from_channels(7, 0, 0).to_rgb888()),
        "without the OBJWIN hole, B keeps priority 2 and the BG wins"
    );
}

#[test]
fn transparent_semi_transparent_obj_reblends_a_normal_objs_retained_variant_color() {
    // A transparent, better-priority semi-transparent OBJ promotes
    // priority over an already-brightened worse-priority Normal OBJ
    // without replacing its color (SpritePixel::color_semi_transparent).
    // mGBA re-brightens that surviving color again in its reblend
    // postpass (`video-software.c:982-1013`), so this crate must double-
    // brighten it too.
    let (tiles_a, palette_a, map_a) = opaque_bg_fixture(1); // BG0: priority 1, not target2
    let (tiles_b, palette_b, map_b) = opaque_bg_fixture(2); // BG1: priority 2, target2
    let layer_a = crate::bg::BgLayer::new(&tiles_a, &palette_a, &map_a);
    let layer_b = crate::bg::BgLayer::new(&tiles_b, &palette_b, &map_b);
    let slots = [
        BgSlot::new(layer_a, 0, 1, 0, 0, true),
        BgSlot::new(layer_b, 1, 2, 0, 0, true),
    ];

    // Tile 0: opaque everywhere at palette index 15 (the worse-priority
    // Normal OBJ). Tile 1: fully transparent (the better-priority
    // semi-transparent OBJ, transparent at (0, 0)).
    let mut two_tiles = [0u8; 64];
    two_tiles[..32].fill(0xFF);
    let sprite_tileset = Tileset::decode(BitDepth::Bpp4, &two_tiles).unwrap();
    let mut sprite_colors = [Bgr555::default(); Palette::LEN];
    sprite_colors[15] = Bgr555::from_channels(8, 4, 2);
    let sprite_palette = Palette::new(sprite_colors);
    let worse_priority_opaque_normal = OamEntry::new(
        0,
        0,
        0, // tile 0 (opaque)
        0,
        BitDepth::Bpp4,
        false,
        false,
        ObjShape::Square,
        0,
        2, // worse priority
        true,
    );
    let better_priority_transparent_semi = OamEntry::new(
        0,
        0,
        1, // tile 1 (transparent)
        0,
        BitDepth::Bpp4,
        false,
        false,
        ObjShape::Square,
        0,
        0, // better priority
        true,
    )
    .with_mode(ObjMode::SemiTransparent);
    let entries = [
        worse_priority_opaque_normal,
        better_priority_transparent_semi,
    ];
    let sprites = SpriteLayer::new(&entries, &sprite_tileset, &sprite_tileset, &sprite_palette);

    let effects = FrameEffects {
        color: EffectsConfig {
            effect: ColorEffect::Brighten,
            target1: LayerTargets {
                bg: [false; 4],
                obj: true,
                backdrop: false,
            },
            target2: LayerTargets {
                bg: [false, true, false, false],
                obj: false,
                backdrop: false,
            },
            eva: 8,
            evb: 8,
            evy: 8,
        },
        ..FrameEffects::default()
    };

    let fb = compose_frame_with_effects(&sprites, &slots, &effects);
    assert_eq!(
        fb.pixel(0, 0),
        Some(Rgb888 {
            r: 207,
            g: 199,
            b: 195,
        }),
        "the retained Normal OBJ variant color is brightened again by the reblend postpass"
    );
}

#[test]
fn semi_transparent_obj_reblend_brightens_with_a_deeper_enabled_target2_bg() {
    // A semi-transparent OBJ (forced alpha) sits over BG_a
    // (priority 1, its immediate neighbour, NOT a target2) with BG_b
    // (priority 2, a target2) enabled deeper in the frame. See
    // effects::resolve_pixel_color's contract for why a global target2
    // still brightens this surviving pixel. The control drops BG_b's
    // target2 bit -> no target2 anywhere -> brightened either way.
    let (tiles_a, palette_a, map_a) = opaque_bg_fixture(5);
    let (tiles_b, palette_b, map_b) = opaque_bg_fixture(10);
    let layer_a = crate::bg::BgLayer::new(&tiles_a, &palette_a, &map_a);
    let layer_b = crate::bg::BgLayer::new(&tiles_b, &palette_b, &map_b);
    let slots = [
        BgSlot::new(layer_a, 0, 1, 0, 0, true), // BG0, priority 1 (immediate next)
        BgSlot::new(layer_b, 1, 2, 0, 0, true), // BG1, priority 2 (deeper)
    ];

    let sprite_tileset = Tileset::decode(BitDepth::Bpp4, &[0xFFu8; 32]).unwrap();
    let mut sprite_colors = [Bgr555::default(); Palette::LEN];
    sprite_colors[15] = Bgr555::from_channels(0, 0, 0);
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
        0, // priority 0: the front layer
        true,
    )
    .with_mode(ObjMode::SemiTransparent)];
    let sprites = SpriteLayer::new(&entries, &sprite_tileset, &sprite_tileset, &sprite_palette);

    let base_color = EffectsConfig {
        effect: ColorEffect::Brighten,
        target1: LayerTargets {
            bg: [false; 4],
            obj: true,
            backdrop: false,
        },
        target2: LayerTargets {
            bg: [false, true, false, false], // BG1 (deeper) is a target2
            obj: false,
            backdrop: false,
        },
        eva: 8,
        evb: 8,
        evy: 16,
    };

    let effects = FrameEffects {
        color: base_color,
        ..FrameEffects::default()
    };
    let fb = compose_frame_with_effects(&sprites, &slots, &effects);
    assert_eq!(
        fb.pixel(0, 0),
        Some(Bgr555::from_channels(31, 31, 31).to_rgb888()),
        "a deeper enabled target2 BG clears the variant, but the surviving reblend OBJ is postprocessed to white"
    );

    let mut control_color = base_color;
    control_color.target2 = LayerTargets::default();
    let control_effects = FrameEffects {
        color: control_color,
        ..FrameEffects::default()
    };
    let control_fb = compose_frame_with_effects(&sprites, &slots, &control_effects);
    assert_eq!(
        control_fb.pixel(0, 0),
        Some(Bgr555::from_channels(31, 31, 31).to_rgb888()),
        "no target2 anywhere -> the semi-transparent OBJ is brightened to white"
    );
}
