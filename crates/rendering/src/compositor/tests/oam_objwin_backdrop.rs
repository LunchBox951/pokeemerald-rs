//! Pins per-scanline OAM admission, OBJWIN masking, and forced-alpha backdrop blending.

use super::super::{compose_frame, compose_frame_with_effects, BgSlot, FrameEffects};
use super::shared::opaque_bg_fixture;
use crate::effects::{ColorEffect, EffectsConfig, LayerTargets};
use crate::oam::{OamEntry, ObjMode, ObjShape};
use crate::palette::{Bgr555, Palette, Rgb888};
use crate::sprite::SpriteLayer;
use crate::tile::{BitDepth, Tileset};
use crate::window::{WindowConfig, WindowLayerEnable, WindowRange, WindowRect};

#[test]
fn compositor_agrees_between_the_objwin_mask_and_visible_sprite_layer_under_exhaustion() {
    // Both late entries cost 62 (64px wide, on-screen), identical to the
    // filler sprites ahead of them, so the oam_budget cutoff at OAM index
    // 19 (`oam_budget.rs`) applies uniformly: 19 fillers exhaust it
    // before either late entry is reached, 17 leave both inside it. The
    // OBJWIN mask and the visible OBJ layer both read that one cached
    // admission decision (`crate::oam_budget`), so they move together.
    let (bg0_tiles, bg0_palette, bg0_map) = opaque_bg_fixture(9);
    let bg0 = crate::bg::BgLayer::new(&bg0_tiles, &bg0_palette, &bg0_map);
    let (bg1_tiles, bg1_palette, bg1_map) = opaque_bg_fixture(4);
    let bg1 = crate::bg::BgLayer::new(&bg1_tiles, &bg1_palette, &bg1_map);
    let slots = [
        BgSlot::new(bg0, 0, 3, 0, 0, true), // worst priority: the "floor"
        BgSlot::new(bg1, 1, 0, 0, 0, true), // only ever shown via the OBJWIN mask
    ];

    let mut two_tiles = [0u8; 64];
    two_tiles[..32].copy_from_slice(&[0xFFu8; 32]); // tile 0: opaque (index 15)
    let sprite_tileset = Tileset::decode(BitDepth::Bpp4, &two_tiles).unwrap();
    let mut sprite_colors = [Bgr555::default(); Palette::LEN];
    sprite_colors[15] = Bgr555::from_channels(31, 31, 31);
    let sprite_palette = Palette::new(sprite_colors);

    let wide_64 = |x_raw: u16, tile: u16| {
        OamEntry::new(
            x_raw,
            0,
            tile,
            0,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            3, // 64x64
            0,
            true,
        )
    };

    let mut obj_bg1_only = WindowLayerEnable::NONE;
    obj_bg1_only.bg[1] = true;
    let mut winout = WindowLayerEnable::NONE;
    winout.bg[0] = true;
    winout.obj = true;
    let effects = FrameEffects {
        windows: WindowConfig {
            win0: None,
            win1: None,
            obj_window: Some(obj_bg1_only),
            winout,
        },
        ..FrameEffects::default()
    };

    let build_entries = |filler_count: usize| -> Vec<OamEntry> {
        let mut entries = vec![wide_64(0, 1); filler_count]; // transparent fillers
        entries.push(wide_64(0, 0).with_mode(ObjMode::Window)); // mask sprite, x=0
        entries.push(wide_64(100, 0)); // visible sprite, x=100, priority 0
        entries
    };

    // Exhausted: 19 fillers push both late entries past the budget.
    let exhausted_entries = build_entries(19);
    let exhausted_sprites = SpriteLayer::new(
        &exhausted_entries,
        &sprite_tileset,
        &sprite_tileset,
        &sprite_palette,
    );
    let exhausted_fb = compose_frame_with_effects(&exhausted_sprites, &slots, &effects);
    assert_eq!(
        exhausted_fb.pixel(0, 0),
        Some(Bgr555::from_channels(9, 0, 0).to_rgb888()),
        "the mask sprite is dropped -> BG1 stays hidden, BG0 (winout) shows through"
    );
    assert_eq!(
        exhausted_fb.pixel(100, 0),
        Some(Bgr555::from_channels(9, 0, 0).to_rgb888()),
        "the visible sprite is dropped -> BG0 shows through instead"
    );

    // Admitted: 17 fillers leave both late entries inside the budget.
    let admitted_entries = build_entries(17);
    let admitted_sprites = SpriteLayer::new(
        &admitted_entries,
        &sprite_tileset,
        &sprite_tileset,
        &sprite_palette,
    );
    let admitted_fb = compose_frame_with_effects(&admitted_sprites, &slots, &effects);
    assert_eq!(
        admitted_fb.pixel(0, 0),
        Some(Bgr555::from_channels(4, 0, 0).to_rgb888()),
        "the mask sprite is admitted -> BG1 shows through the OBJWIN mask"
    );
    assert_eq!(
        admitted_fb.pixel(100, 0),
        Some(Bgr555::from_channels(31, 31, 31).to_rgb888()),
        "the visible sprite is admitted -> its own color beats BG0 by priority"
    );
}

#[test]
fn composing_a_frame_walks_oam_once_per_scanline() {
    // The per-scanline OAM admission stage is consulted by both
    // `SpriteLayer::resolve_pixel_with_mosaic` and
    // `objwin_mask_with_mosaic`, both of which `compose_pixel` calls per
    // pixel. `SpriteLayer`'s one-slot per-scanline cache is what keeps
    // that at one walk per row (160 a frame) instead of one per pixel
    // per path (up to 76,800) -- and both paths reading that one slot is
    // what makes it structurally impossible for them to disagree.
    let sprite_tileset = Tileset::decode(BitDepth::Bpp4, &[0xFFu8; 32]).unwrap();
    let mut sprite_colors = [Bgr555::default(); Palette::LEN];
    sprite_colors[15] = Bgr555::from_channels(0, 9, 0);
    let sprite_palette = Palette::new(sprite_colors);
    let entries = vec![
        OamEntry::new(
            0,
            0,
            0,
            0,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            3, // 64x64
            0,
            true,
        ),
        OamEntry::new(
            0,
            0,
            0,
            0,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            3,
            0,
            true,
        )
        .with_mode(ObjMode::Window),
    ];
    let sprites = SpriteLayer::new(&entries, &sprite_tileset, &sprite_tileset, &sprite_palette);

    crate::oam_budget::reset_walk_count();
    let _ = compose_frame(&sprites, &[]);
    assert_eq!(
        crate::oam_budget::walk_count(),
        crate::framebuffer::Framebuffer::HEIGHT,
        "one OAM walk per scanline, not per pixel"
    );

    // Enabling OBJWIN adds a second per-pixel consumer of the admission
    // stage; the walk count must not move.
    let mut winout = WindowLayerEnable::NONE;
    winout.obj = true;
    let effects = FrameEffects {
        windows: WindowConfig {
            win0: None,
            win1: None,
            obj_window: Some(WindowLayerEnable::NONE),
            winout,
        },
        ..FrameEffects::default()
    };
    crate::oam_budget::reset_walk_count();
    let _ = compose_frame_with_effects(&sprites, &[], &effects);
    assert_eq!(
        crate::oam_budget::walk_count(),
        crate::framebuffer::Framebuffer::HEIGHT,
        "the OBJWIN mask path shares the visible path's cached admission"
    );
}

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

#[test]
fn forced_alpha_blends_against_the_span_backdrop_variant() {
    // No BG or OBJ is a second target; the backdrop is the configured
    // target2, and effects::backdrop_variant resolves its colour to the
    // span's variant before the blend.
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

    let white = Bgr555::from_channels(31, 31, 31).to_rgb888();
    // EVA = 0, EVB = 16 shows the second target alone, so the displayed
    // pixel is whichever backdrop variant mgba blended against.
    for (effect, backdrop, expected) in [
        (ColorEffect::Brighten, Rgb888::BLACK, white),
        (ColorEffect::Darken, white, Rgb888::BLACK),
    ] {
        let effects = FrameEffects {
            color: EffectsConfig {
                effect,
                target1: LayerTargets {
                    bg: [false; 4],
                    obj: false,
                    backdrop: true,
                },
                target2: LayerTargets {
                    bg: [false; 4],
                    obj: false,
                    backdrop: true,
                },
                eva: 0,
                evb: 16,
                evy: 16,
            },
            backdrop,
            ..FrameEffects::default()
        };

        let fb = compose_frame_with_effects(&sprites, &[], &effects);
        assert_eq!(
            fb.pixel(0, 0),
            Some(expected),
            "forced alpha blends against the span's backdrop variant, not the raw backdrop"
        );
    }
}
