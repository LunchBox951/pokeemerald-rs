//! Pins OBJWIN masking: its WIN0/WIN1 precedence, backdrop variant, and slow-path reblend.

use super::super::{compose_frame_with_effects, BgSlot, FrameEffects};
use super::shared::opaque_bg_fixture;
use crate::effects::{ColorEffect, EffectsConfig, LayerTargets};
use crate::oam::{OamEntry, ObjMode, ObjShape};
use crate::palette::{Bgr555, Palette, Rgb888};
use crate::sprite::SpriteLayer;
use crate::tile::{BitDepth, Tileset};
use crate::window::{WindowConfig, WindowLayerEnable, WindowRange, WindowRect};

#[test]
fn objwin_sprite_hole_must_not_upgrade_obj_order_inside_win0() {
    // mgba drops an OBJWIN sprite outright inside WIN0/WIN1 (software-obj.c:161,
    // video-software.c:131-134), so its hole cannot promote a worse-priority OBJ
    // there, but still does so in WINOUT.
    let (tiles, palette, map) = opaque_bg_fixture(9); // red BG0
    let layer = crate::bg::BgLayer::new(&tiles, &palette, &map);
    let slots = [BgSlot::new(layer, 0, 1, 0, 0, true)]; // BG0 at priority 1

    // Tile 0: solid index 15 (the normal sprite). Tile 1: columns 0..4
    // transparent, columns 4..8 index 14 (the OBJWIN mask sprite).
    let mut tile_bytes = [0xFFu8; 64];
    for row in tile_bytes[32..].chunks_exact_mut(4) {
        row.copy_from_slice(&[0x00, 0x00, 0xEE, 0xEE]);
    }
    let sprite_tiles = Tileset::decode(BitDepth::Bpp4, &tile_bytes).unwrap();
    let mut sprite_colors = [Bgr555::default(); Palette::LEN];
    sprite_colors[15] = Bgr555::from_channels(0, 9, 0); // green: the normal sprite
    sprite_colors[14] = Bgr555::from_channels(0, 31, 31); // must never render
    let sprite_palette = Palette::new(sprite_colors);
    let entries = [
        // OAM 0: normal sprite, worst priority, opaque everywhere.
        OamEntry::new(
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
        ),
        // OAM 1: OBJWIN sprite, best priority, transparent over x < 4.
        OamEntry::new(
            0,
            0,
            1,
            0,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            0,
            0,
            true,
        )
        .with_mode(ObjMode::Window),
    ];
    let sprites = SpriteLayer::new(&entries, &sprite_tiles, &sprite_tiles, &sprite_palette);

    let mut bg0_and_obj = WindowLayerEnable::NONE;
    bg0_and_obj.bg[0] = true;
    bg0_and_obj.obj = true;
    let effects = FrameEffects {
        windows: WindowConfig {
            // WIN0 covers x < 2 only; x = 3 falls through to WINOUT.
            win0: Some((
                WindowRect::new(WindowRange::new(0, 2), WindowRange::new(0, 8)),
                bg0_and_obj,
            )),
            win1: None,
            obj_window: Some(bg0_and_obj),
            winout: bg0_and_obj,
        },
        ..FrameEffects::default()
    };

    let fb = compose_frame_with_effects(&sprites, &slots, &effects);
    assert_eq!(
        fb.pixel(3, 0),
        Some(Bgr555::from_channels(0, 9, 0).to_rgb888()),
        "WINOUT: the OBJWIN hole is drawn, promoting the priority-3 OBJ ahead of BG0"
    );
    assert_eq!(
        fb.pixel(1, 0),
        Some(Bgr555::from_channels(9, 0, 0).to_rgb888()),
        "WIN0 outranks OBJWIN, so the OBJWIN sprite is skipped there and the \
         OBJ stays at priority 3, behind BG0's priority 1"
    );
}

#[test]
fn objwin_mask_never_changes_the_uncovered_backdrop_variant() {
    // The backdrop variant is chosen once per static span
    // (`effects::backdrop_variant`), not per OBJWIN-masked pixel: WINOUT
    // enables effects here while OBJWIN doesn't, so both columns brighten.
    let mut mask_tile = [0u8; 32];
    for row in mask_tile.chunks_exact_mut(4) {
        row.copy_from_slice(&[0x00, 0x00, 0xFF, 0xFF]); // columns 4..8 opaque
    }
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

    // WINOUT enables color effects; OBJWIN enables nothing at all.
    let mut winout = WindowLayerEnable::NONE;
    winout.effects = true;

    let effects = FrameEffects {
        windows: WindowConfig {
            win0: None,
            win1: None,
            obj_window: Some(WindowLayerEnable::NONE),
            winout,
        },
        color: EffectsConfig {
            effect: ColorEffect::Brighten,
            target1: LayerTargets {
                bg: [false; 4],
                obj: false,
                backdrop: true,
            },
            target2: LayerTargets::default(),
            eva: 0,
            evb: 0,
            evy: 16,
        },
        backdrop: Rgb888::BLACK,
        ..FrameEffects::default()
    };

    let white = Bgr555::from_channels(31, 31, 31).to_rgb888();
    let fb = compose_frame_with_effects(&sprites, &[], &effects);
    assert_eq!(
        fb.pixel(1, 0),
        Some(white),
        "outside the OBJWIN mask the WINOUT span brightens the backdrop"
    );
    assert_eq!(
        fb.pixel(5, 0),
        Some(white),
        "the OBJWIN mask must not re-select the backdrop variant"
    );
}

#[test]
fn objwin_slow_path_reblends_a_normal_obj_that_is_not_a_target1_layer() {
    // OBJWIN enabled with a blend-enable bit that differs from WINOUT's
    // own triggers mGBA's `objwinSlowPath` (`software-obj.c:176,180-192`),
    // which reblends a plain Normal-mode OBJ even though BLDCNT never
    // marks it as a target1 layer.
    let sprite_tileset = Tileset::decode(BitDepth::Bpp4, &[0xFFu8; 32]).unwrap();
    let mut sprite_colors = [Bgr555::default(); Palette::LEN];
    sprite_colors[15] = Bgr555::from_channels(31, 31, 31);
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
    )]; // Normal mode (default), never a target1 OBJ below
    let sprites = SpriteLayer::new(&entries, &sprite_tileset, &sprite_tileset, &sprite_palette);

    let mut winout = WindowLayerEnable::NONE;
    winout.obj = true;
    winout.effects = true;

    let color = EffectsConfig {
        effect: ColorEffect::Darken,
        target1: LayerTargets::default(), // deliberately excludes LayerKind::Obj
        target2: LayerTargets {
            bg: [false; 4],
            obj: false,
            backdrop: true,
        },
        eva: 0,
        evb: 0,
        evy: 16,
    };

    let effects = FrameEffects {
        windows: WindowConfig {
            win0: None,
            win1: None,
            obj_window: Some(WindowLayerEnable::NONE), // OBJWIN blend bit off, WINOUT's is on
            winout,
        },
        color,
        backdrop: Rgb888::BLACK,
        ..FrameEffects::default()
    };

    let fb = compose_frame_with_effects(&sprites, &[], &effects);
    assert_eq!(
        fb.pixel(0, 0),
        Some(Rgb888::BLACK),
        "objwin_slow_path reblends the Normal OBJ even though it is not a target1 layer"
    );

    // Control: OBJWIN's own blend-enable bit now matches WINOUT's, so
    // objwin_slow_path is false -- the non-target1 Normal OBJ is never a
    // reblend candidate and stays raw white.
    let mut matched_obj_window = WindowLayerEnable::NONE;
    matched_obj_window.effects = true;
    let control_effects = FrameEffects {
        windows: WindowConfig {
            obj_window: Some(matched_obj_window),
            ..effects.windows
        },
        ..effects
    };
    let control_fb = compose_frame_with_effects(&sprites, &[], &control_effects);
    let white = Bgr555::from_channels(31, 31, 31).to_rgb888();
    assert_eq!(
        control_fb.pixel(0, 0),
        Some(white),
        "without objwin_slow_path, a non-target1 Normal OBJ is never reblended"
    );
}

/// Draws overlapping solid sprites under Brighten with OBJWIN effects off,
/// returning the composed pixel and whether the final OBJWIN mask covers it.
fn objwin_variant_pixel(entries: &[OamEntry]) -> (Option<Rgb888>, bool) {
    // Tile 0 is solid index 15; tile 1 is fully transparent.
    let mut tile_bytes = [0u8; 64];
    tile_bytes[..32].fill(0xFF);
    let sprite_tileset = Tileset::decode(BitDepth::Bpp4, &tile_bytes).unwrap();
    let mut sprite_colors = [Bgr555::default(); Palette::LEN];
    sprite_colors[15] = Bgr555::from_channels(16, 16, 16);
    let sprite_palette = Palette::new(sprite_colors);
    let sprites = SpriteLayer::new(entries, &sprite_tileset, &sprite_tileset, &sprite_palette);

    let mut winout = WindowLayerEnable::NONE;
    winout.obj = true;
    winout.effects = true;
    let mut obj_window = WindowLayerEnable::NONE;
    obj_window.obj = true; // OBJWIN effects stay off

    let effects = FrameEffects {
        windows: WindowConfig {
            win0: None,
            win1: None,
            obj_window: Some(obj_window),
            winout,
        },
        color: EffectsConfig {
            effect: ColorEffect::Brighten,
            target1: LayerTargets {
                bg: [false; 4],
                obj: true,
                backdrop: false,
            },
            target2: LayerTargets::default(),
            eva: 0,
            evb: 0,
            evy: 16,
        },
        backdrop: Rgb888::BLACK,
        ..FrameEffects::default()
    };
    let fb = compose_frame_with_effects(&sprites, &[], &effects);
    (fb.pixel(0, 0), sprites.objwin_mask(0, 0))
}

fn solid_sprite(priority: u8, mode: ObjMode) -> OamEntry {
    OamEntry::new(
        0,
        0,
        0,
        0,
        BitDepth::Bpp4,
        false,
        false,
        ObjShape::Square,
        0,
        priority,
        true,
    )
    .with_mode(mode)
}

fn transparent_sprite(priority: u8) -> OamEntry {
    OamEntry::new(
        0,
        0,
        1,
        0,
        BitDepth::Bpp4,
        false,
        false,
        ObjShape::Square,
        0,
        priority,
        true,
    )
}

#[test]
fn objwin_sprite_drawn_after_a_normal_sprite_keeps_its_brighten_variant() {
    // mGBA picks the palette from the row mask at write time, and an OBJWIN
    // write only sets it afterwards (`software-obj.c:88-105`).
    let white = Bgr555::from_channels(31, 31, 31).to_rgb888();
    let gray = Bgr555::from_channels(16, 16, 16).to_rgb888();

    let (normal_first, masked) = objwin_variant_pixel(&[
        solid_sprite(1, ObjMode::Normal),
        solid_sprite(0, ObjMode::Window),
    ]);
    assert!(masked, "the OBJWIN sprite covers the pixel either way");
    assert_eq!(
        normal_first,
        Some(white),
        "a later OBJWIN sprite must not retroactively strip the earlier variant"
    );

    let (objwin_first, masked) = objwin_variant_pixel(&[
        solid_sprite(0, ObjMode::Window),
        solid_sprite(1, ObjMode::Normal),
    ]);
    assert!(masked);
    assert_eq!(
        objwin_first,
        Some(gray),
        "an earlier OBJWIN sprite draws the later sprite from objwinPalette, \
         with effects off there"
    );
}

#[test]
fn objwin_write_time_mask_survives_a_flag_only_priority_overwrite() {
    // A better-priority transparent texel keeps the stored color's palette
    // choice (`software-obj.c:120-126`); the mask write itself ignores
    // priority (`software-obj.c:94-105`).
    let white = Bgr555::from_channels(31, 31, 31).to_rgb888();
    let gray = Bgr555::from_channels(16, 16, 16).to_rgb888();
    // Opaque nonzero palette index everywhere, so only order differs.
    let (kept, _) = objwin_variant_pixel(&[
        solid_sprite(2, ObjMode::Normal),
        solid_sprite(3, ObjMode::Window),
        transparent_sprite(0),
    ]);
    assert_eq!(kept, Some(white), "the earlier variant survives promotion");

    let (kept, _) = objwin_variant_pixel(&[
        solid_sprite(3, ObjMode::Window),
        solid_sprite(2, ObjMode::Normal),
        transparent_sprite(0),
    ]);
    assert_eq!(
        kept,
        Some(gray),
        "the objwinPalette color survives promotion"
    );

    // Control: an opaque priority-0 sprite recolors after the OBJWIN write.
    let (recolored, _) = objwin_variant_pixel(&[
        solid_sprite(2, ObjMode::Normal),
        solid_sprite(3, ObjMode::Window),
        solid_sprite(0, ObjMode::Normal),
    ]);
    assert_eq!(
        recolored,
        Some(gray),
        "a color written after OBJWIN uses its palette"
    );
}
