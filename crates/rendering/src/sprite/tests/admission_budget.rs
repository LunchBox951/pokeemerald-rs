//! Pins the per-scanline OAM admission cycle budget and its cache.

use super::shared::{
    opaque_and_transparent_tiles, FIRST_PALETTE_BANK, FIRST_TILE, HIGHEST_OBJ_PRIORITY,
    SIXTY_FOUR_PIXEL_SQUARE_SIZE, TRANSPARENT_TILE,
};
use crate::framebuffer::Framebuffer;
use crate::oam::{OamEntry, ObjMode, ObjShape};
use crate::palette::Bgr555;
use crate::sprite::SpriteLayer;
use crate::tile::BitDepth;

const NORMAL_BUDGET_WIDE_ENTRY_CAPACITY: usize = 19;

const HBLANK_FREE_BUDGET_WIDE_ENTRY_CAPACITY: usize = 15;

/// A 64x64 regular entry at the origin, costing 62 admission cycles per
/// covered scanline.
fn wide_64_regular(tile: u16) -> OamEntry {
    OamEntry::new(
        0,
        0,
        tile,
        FIRST_PALETTE_BANK,
        BitDepth::Bpp4,
        false,
        false,
        ObjShape::Square,
        SIXTY_FOUR_PIXEL_SQUARE_SIZE,
        HIGHEST_OBJ_PRIORITY,
        true,
    )
}

#[test]
fn resolve_pixel_drops_a_late_opaque_sprite_behind_transparent_fillers_once_the_budget_is_exhausted(
) {
    let (tileset, palette) = opaque_and_transparent_tiles();
    let filler = wide_64_regular(TRANSPARENT_TILE);
    let mut entries = vec![filler; NORMAL_BUDGET_WIDE_ENTRY_CAPACITY];
    entries.push(wide_64_regular(FIRST_TILE));
    let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

    assert_eq!(
        layer.resolve_pixel(0, 0),
        None,
        "the late opaque sprite at OAM index 19 is past the scanline's cycle budget"
    );

    let admitted_entries = entries[1..].to_vec();
    let control_layer = SpriteLayer::new(&admitted_entries, &tileset, &tileset, &palette);
    assert_eq!(
        control_layer.resolve_pixel(0, 0).map(|p| p.color),
        Some(Bgr555::from_channels(0, 0, 0x1F).to_rgb888()),
        "one fewer filler admits the same opaque entry at OAM index 18"
    );
}

#[test]
fn objwin_mask_drops_a_late_objwin_sprite_once_the_budget_is_exhausted() {
    let (tileset, palette) = opaque_and_transparent_tiles();
    let filler = wide_64_regular(TRANSPARENT_TILE);
    let mut entries = vec![filler; NORMAL_BUDGET_WIDE_ENTRY_CAPACITY];
    entries.push(wide_64_regular(FIRST_TILE).with_mode(ObjMode::Window));
    let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

    assert!(!layer.objwin_mask(0, 0));

    let admitted_entries = entries[1..].to_vec();
    let control_layer = SpriteLayer::new(&admitted_entries, &tileset, &tileset, &palette);
    assert!(control_layer.objwin_mask(0, 0));
}

#[test]
fn with_hblank_free_interval_applies_the_reduced_954_cycle_budget() {
    let (tileset, palette) = opaque_and_transparent_tiles();
    let filler = wide_64_regular(TRANSPARENT_TILE);
    let mut entries = vec![filler; HBLANK_FREE_BUDGET_WIDE_ENTRY_CAPACITY];
    entries.push(wide_64_regular(FIRST_TILE));

    let normal = SpriteLayer::new(&entries, &tileset, &tileset, &palette);
    assert!(
        normal.resolve_pixel(0, 0).is_some(),
        "OAM index 15 is still within the normal budget's (index 19) cutoff"
    );

    let hblank_free =
        SpriteLayer::new(&entries, &tileset, &tileset, &palette).with_hblank_free_interval(true);
    assert_eq!(
        hblank_free.resolve_pixel(0, 0),
        None,
        "with_hblank_free_interval must select the reduced budget, dropping OAM index 15"
    );
}

#[test]
fn with_hblank_free_interval_discards_an_already_populated_admission_cache() {
    let (tileset, palette) = opaque_and_transparent_tiles();
    let filler = wide_64_regular(TRANSPARENT_TILE);
    let mut entries = vec![filler; HBLANK_FREE_BUDGET_WIDE_ENTRY_CAPACITY];
    entries.push(wide_64_regular(FIRST_TILE));

    let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);
    assert!(
        layer.resolve_pixel(0, 0).is_some(),
        "populate the cache under the normal 1210-cycle budget first"
    );
    let flipped = layer.with_hblank_free_interval(true);
    assert_eq!(
        flipped.resolve_pixel(0, 0),
        None,
        "a flag flip after use must not serve the stale 1210-cycle admission"
    );
}

#[test]
fn display_and_objwin_queries_share_one_admission_walk_per_scanline() {
    let (tileset, palette) = opaque_and_transparent_tiles();
    let entries = vec![
        wide_64_regular(FIRST_TILE),
        wide_64_regular(FIRST_TILE).with_mode(ObjMode::Window),
    ];
    let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

    crate::oam_budget::reset_walk_count();
    for x in 0..Framebuffer::WIDTH {
        let _ = layer.resolve_pixel(x, 0);
        let _ = layer.objwin_mask(x, 0);
    }
    assert_eq!(
        crate::oam_budget::walk_count(),
        1,
        "480 per-pixel calls on one scanline must share a single OAM walk"
    );

    for x in 0..Framebuffer::WIDTH {
        let _ = layer.objwin_mask(x, 1);
        let _ = layer.resolve_pixel(x, 1);
    }
    assert_eq!(
        crate::oam_budget::walk_count(),
        2,
        "one further walk for scanline 1, whichever path asks first"
    );

    let _ = layer.resolve_pixel(0, 0);
    assert_eq!(crate::oam_budget::walk_count(), 3);
}
