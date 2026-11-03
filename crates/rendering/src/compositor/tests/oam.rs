//! Pins the once-per-scanline OAM admission decision and its cross-consumer agreement.

use super::super::{compose_frame, compose_frame_with_effects, BgSlot, FrameEffects};
use super::shared::opaque_bg_fixture;
use crate::oam::{OamEntry, ObjMode, ObjShape};
use crate::palette::{Bgr555, Palette};
use crate::sprite::SpriteLayer;
use crate::tile::{BitDepth, Tileset};
use crate::window::{WindowConfig, WindowLayerEnable};

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
