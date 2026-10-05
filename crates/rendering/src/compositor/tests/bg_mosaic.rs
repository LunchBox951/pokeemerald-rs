//! Pins regular- and affine-BG mosaic snapping, holds, and their window/OBJWIN interplay.

use super::super::{compose_frame_with_effects, BgSlot, FrameEffects};
use super::shared::{
    bpp4_tile_with_every_row, bpp4_tile_with_top_left_2x2, empty_sprite_layer,
    opaque_affine_bg_fixture,
};
use crate::affine::AffineMatrix;
use crate::bg_affine::{AffineBgLayer, AffineTilemap, Overflow};
use crate::mosaic::MosaicSize;
use crate::oam::{OamEntry, ObjMode, ObjShape};
use crate::palette::{Bgr555, Palette};
use crate::sprite::SpriteLayer;
use crate::tile::{BitDepth, Tileset};
use crate::tilemap::{ScreenEntry, Tilemap};
use crate::window::{WindowConfig, WindowLayerEnable, WindowRange, WindowRect};

/// A single affine BG tile whose first row (y=0) has a distinct opaque
/// color per column (channel `column + 1`) -- distinguishes "holds its
/// own column's texel" from "holds a neighbor's" at column granularity,
/// unlike [`opaque_affine_bg_fixture`]'s single flat color.
fn gradient_affine_bg_fixture() -> (Tileset, Palette, AffineTilemap) {
    let tile_byte_len = BitDepth::Bpp8.tile_byte_len();
    let mut bytes = vec![0u8; tile_byte_len];
    for (column, byte) in bytes[..BitDepth::TILE_DIM].iter_mut().enumerate() {
        *byte = u8::try_from(column + 1).unwrap();
    }
    let tileset = Tileset::decode(BitDepth::Bpp8, &bytes).unwrap();
    let mut colors = [Bgr555::default(); Palette::LEN];
    for (column, color) in colors[1..=BitDepth::TILE_DIM].iter_mut().enumerate() {
        *color = Bgr555::from_channels(u8::try_from(column + 1).unwrap(), 0, 0);
    }
    let palette = Palette::new(colors);
    let tilemap = AffineTilemap::new(1, 1, vec![0]).unwrap();
    (tileset, palette, tilemap)
}

/// An 8-tile-wide, 1-tile-tall *affine* BG layer whose tiles 0..8 are
/// each flat-filled with a distinct opaque color (channel
/// `tile_index + 1`) -- for tests that need to distinguish which *tile*
/// (not just which column within one tile) a sample landed in, e.g. with
/// a scaled affine matrix that crosses tile boundaries over a handful of
/// screen columns.
fn eight_tile_gradient_affine_bg_fixture() -> (Tileset, Palette, AffineTilemap) {
    const TILE_COUNT: usize = 8;
    let tile_byte_len = BitDepth::Bpp8.tile_byte_len();
    let mut bytes = vec![0u8; tile_byte_len * TILE_COUNT];
    for (tile_index, chunk) in bytes.chunks_exact_mut(tile_byte_len).enumerate() {
        chunk.fill(u8::try_from(tile_index + 1).unwrap());
    }
    let tileset = Tileset::decode(BitDepth::Bpp8, &bytes).unwrap();
    let mut colors = [Bgr555::default(); Palette::LEN];
    for (tile_index, color) in colors[1..=TILE_COUNT].iter_mut().enumerate() {
        *color = Bgr555::from_channels(u8::try_from(tile_index + 1).unwrap(), 0, 0);
    }
    let palette = Palette::new(colors);
    let tile_indices: Vec<u8> = (0..TILE_COUNT).map(|i| u8::try_from(i).unwrap()).collect();
    let tilemap = AffineTilemap::new(TILE_COUNT, 1, tile_indices).unwrap();
    (tileset, palette, tilemap)
}

fn quadrant_bg_fixture() -> (Tileset, Palette, Tilemap) {
    let bytes = bpp4_tile_with_top_left_2x2([[1, 2], [3, 4]]);
    let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
    let mut colors = [Bgr555::default(); Palette::LEN];
    colors[1] = Bgr555::from_channels(1, 0, 0);
    colors[2] = Bgr555::from_channels(2, 0, 0);
    colors[3] = Bgr555::from_channels(3, 0, 0);
    colors[4] = Bgr555::from_channels(4, 0, 0);
    let palette = Palette::new(colors);
    let entries = vec![ScreenEntry::new(0, false, false, 0)];
    let tilemap = Tilemap::new(1, 1, entries).unwrap();
    (tileset, palette, tilemap)
}

#[test]
fn mosaic_snaps_bg_sampling_to_its_block_origin() {
    let (tileset, palette, tilemap) = quadrant_bg_fixture();
    let layer = crate::bg::BgLayer::new(&tileset, &palette, &tilemap);
    let slot = BgSlot::new(layer, 0, 0, 0, 0, true).with_mosaic(true);
    let entries: [OamEntry; 0] = [];
    let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
    let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);

    let effects = FrameEffects {
        mosaic: crate::mosaic::MosaicConfig {
            bg: MosaicSize::new(2, 2),
            obj: MosaicSize::NONE,
        },
        ..FrameEffects::default()
    };
    let fb = compose_frame_with_effects(&sprites, &[slot], &effects);

    let origin_color = Bgr555::from_channels(1, 0, 0).to_rgb888();
    assert_eq!(fb.pixel(0, 0), Some(origin_color));
    assert_eq!(
        fb.pixel(1, 0),
        Some(origin_color),
        "snapped from (1,0) to (0,0)"
    );
    assert_eq!(
        fb.pixel(0, 1),
        Some(origin_color),
        "snapped from (0,1) to (0,0)"
    );
    assert_eq!(
        fb.pixel(1, 1),
        Some(origin_color),
        "snapped from (1,1) to (0,0)"
    );
}

#[test]
fn mosaic_only_applies_to_bg_slots_with_their_own_mosaic_bit_set() {
    let (tileset, palette, tilemap) = quadrant_bg_fixture();
    let layer = crate::bg::BgLayer::new(&tileset, &palette, &tilemap);
    let slot = BgSlot::new(layer, 0, 0, 0, 0, true);
    let entries: [OamEntry; 0] = [];
    let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
    let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);

    let effects = FrameEffects {
        mosaic: crate::mosaic::MosaicConfig {
            bg: MosaicSize::new(2, 2),
            obj: MosaicSize::NONE,
        },
        ..FrameEffects::default()
    };
    let fb = compose_frame_with_effects(&sprites, &[slot], &effects);

    assert_eq!(
        fb.pixel(1, 1),
        Some(Bgr555::from_channels(4, 0, 0).to_rgb888())
    );
}

#[test]
fn affine_mosaic_retries_the_next_pixel_when_the_block_origin_is_out_of_bounds() {
    // mGBA's out-of-bounds mode-2 mosaic fetch leaves `mosaicWait`
    // unchanged, so the next column's retry seeds the hold instead
    // (`MODE_2_COORD_NO_OVERFLOW`/`MODE_2_MOSAIC`,
    // `mgba/src/gba/renderers/software-bg.c:24-42`) `(behavioral-fidelity)`.
    let (tiles, palette, tilemap) = opaque_affine_bg_fixture(9);
    let layer = AffineBgLayer::new(&tiles, &palette, &tilemap);
    let one_texture_pixel = i32::from(AffineMatrix::ONE);
    let slot = BgSlot::new_affine(
        layer,
        0,
        0,
        AffineMatrix::IDENTITY,
        -one_texture_pixel,
        0,
        Overflow::Transparent,
        true,
    )
    .with_mosaic(true);
    let entries: [OamEntry; 0] = [];
    let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
    let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);

    let effects = FrameEffects {
        mosaic: crate::mosaic::MosaicConfig {
            bg: MosaicSize::new(4, 1),
            obj: MosaicSize::NONE,
        },
        ..FrameEffects::default()
    };
    let fb = compose_frame_with_effects(&sprites, &[slot], &effects);

    let held = Bgr555::from_channels(9, 0, 0).to_rgb888();
    assert_eq!(
        fb.pixel(0, 0),
        Some(crate::palette::Rgb888::BLACK),
        "texture x=-1 is out of bounds, so this pixel draws nothing"
    );
    assert_eq!(
        fb.pixel(1, 0),
        Some(held),
        "the retry at screen x=1 samples texture x=0 and draws it"
    );
    assert_eq!(
        fb.pixel(2, 0),
        Some(held),
        "x=2 holds the value the retry drew at x=1, not a fresh (still in-bounds) sample"
    );
    assert_eq!(
        fb.pixel(3, 0),
        Some(held),
        "x=3 holds the value the retry drew at x=1, completing the 4-wide block"
    );
}

#[test]
fn affine_mosaic_wrap_overflow_still_snaps_to_the_block_origin() {
    let (tiles, palette, tilemap) = gradient_affine_bg_fixture();
    let layer = AffineBgLayer::new(&tiles, &palette, &tilemap);
    let one_texture_pixel = i32::from(AffineMatrix::ONE);
    let slot = BgSlot::new_affine(
        layer,
        0,
        0,
        AffineMatrix::IDENTITY,
        -one_texture_pixel,
        0,
        Overflow::Wrap,
        true,
    )
    .with_mosaic(true);
    let entries: [OamEntry; 0] = [];
    let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
    let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);

    let effects = FrameEffects {
        mosaic: crate::mosaic::MosaicConfig {
            bg: MosaicSize::new(4, 1),
            obj: MosaicSize::NONE,
        },
        ..FrameEffects::default()
    };
    let fb = compose_frame_with_effects(&sprites, &[slot], &effects);

    let wrapped_origin = Bgr555::from_channels(8, 0, 0).to_rgb888();
    for x in 0..4 {
        assert_eq!(
            fb.pixel(x, 0),
            Some(wrapped_origin),
            "x={x} must draw the 4-wide block origin's wrapped texel"
        );
    }
    assert_eq!(
        fb.pixel(4, 0),
        Some(Bgr555::from_channels(4, 0, 0).to_rgb888()),
        "x=4 starts the next block and snaps to its own origin"
    );
}

#[test]
fn affine_mosaic_wrap_overflow_bypasses_snapping_at_a_decoded_block_size_of_two() {
    let (tiles, palette, tilemap) = gradient_affine_bg_fixture();
    let layer = AffineBgLayer::new(&tiles, &palette, &tilemap);
    let slot = BgSlot::new_affine(
        layer,
        0,
        0,
        AffineMatrix::IDENTITY,
        0,
        0,
        Overflow::Wrap,
        true,
    )
    .with_mosaic(true);
    let entries: [OamEntry; 0] = [];
    let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
    let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);

    let effects = FrameEffects {
        mosaic: crate::mosaic::MosaicConfig {
            bg: MosaicSize::new(2, 1),
            obj: MosaicSize::NONE,
        },
        ..FrameEffects::default()
    };
    let fb = compose_frame_with_effects(&sprites, &[slot], &effects);

    let column0 = Bgr555::from_channels(1, 0, 0).to_rgb888();
    let column1 = Bgr555::from_channels(2, 0, 0).to_rgb888();
    assert_eq!(fb.pixel(0, 0), Some(column0), "x=0 samples its own column");
    assert_eq!(
        fb.pixel(1, 0),
        Some(column1),
        "x=1 must sample its own column, not snap to x=0's block origin"
    );
}

#[test]
fn affine_mosaic_hold_reopens_seeded_from_the_snapped_origin_not_the_pre_gap_hold() {
    // mGBA restarts the mode-2 draw routine once per hardware-window
    // region, re-seeding its mosaic hold from that region's own snapped
    // block origin rather than resuming the pre-gap hold
    // (`mgba/src/gba/renderers/video-software.c:628-675`,
    // `software-private.h:173-192`, `software-bg.c:66-73`)
    // `(behavioral-fidelity)`.
    let (tiles, palette, tilemap) = gradient_affine_bg_fixture();
    let layer = AffineBgLayer::new(&tiles, &palette, &tilemap);
    let slot = BgSlot::new_affine(
        layer,
        0,
        0,
        AffineMatrix::IDENTITY,
        0,
        0,
        Overflow::Transparent,
        true,
    )
    .with_mosaic(true);
    let entries: [OamEntry; 0] = [];
    let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
    let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);

    let bg0_off = WindowLayerEnable::NONE;
    let mut bg0_on = WindowLayerEnable::NONE;
    bg0_on.bg[0] = true;

    let effects = FrameEffects {
        windows: WindowConfig {
            win0: Some((
                WindowRect::new(WindowRange::new(3, 5), WindowRange::new(0, 1)),
                bg0_off,
            )),
            win1: None,
            obj_window: None,
            winout: bg0_on,
        },
        mosaic: crate::mosaic::MosaicConfig {
            bg: MosaicSize::new(4, 1),
            obj: MosaicSize::NONE,
        },
        ..FrameEffects::default()
    };
    let fb = compose_frame_with_effects(&sprites, &[slot], &effects);

    let column0 = Bgr555::from_channels(1, 0, 0).to_rgb888();
    let column4 = Bgr555::from_channels(5, 0, 0).to_rgb888();
    assert_eq!(
        fb.pixel(2, 0),
        Some(column0),
        "x=2 holds column 0's texel, fetched at x=0; the window closes right after"
    );
    assert_eq!(
        fb.pixel(3, 0),
        Some(crate::palette::Rgb888::BLACK),
        "x=3 is window-closed: nothing from this slot draws here"
    );
    assert_eq!(
        fb.pixel(5, 0),
        Some(column4),
        "x=5 reopens mid-block (phase 1 of 4): seeded from the snapped \
         origin at x=4 (column 4), not blank and not column 0's pre-gap hold"
    );
    assert_eq!(
        fb.pixel(6, 0),
        Some(column4),
        "x=6 still holds the seed from x=5's span"
    );
    assert_eq!(
        fb.pixel(7, 0),
        Some(column4),
        "x=7 still holds the seed from x=5's span"
    );
}

#[test]
fn affine_mosaic_hold_does_not_cross_a_same_enabled_window_region_boundary() {
    // mGBA restarts BG drawing per hardware-window region and never
    // coalesces adjacent regions sharing the same control bits
    // (`mgba/src/gba/renderers/video-software.c:458-505,628-675`)
    // `(behavioral-fidelity)`.
    let (tiles, palette, tilemap) = eight_tile_gradient_affine_bg_fixture();
    let layer = AffineBgLayer::new(&tiles, &palette, &tilemap);
    let scale = 8 * AffineMatrix::ONE;
    let matrix = AffineMatrix::new(scale, 0, 0, AffineMatrix::ONE);
    let reference_x = -24 * i32::from(AffineMatrix::ONE);
    let slot = BgSlot::new_affine(
        layer,
        0,
        0,
        matrix,
        reference_x,
        0,
        Overflow::Transparent,
        true,
    )
    .with_mosaic(true);
    let entries: [OamEntry; 0] = [];
    let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
    let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);

    let mut bg0_on = WindowLayerEnable::NONE;
    bg0_on.bg[0] = true;

    let effects = FrameEffects {
        windows: WindowConfig {
            win0: Some((
                WindowRect::new(WindowRange::new(0, 5), WindowRange::new(0, 1)),
                bg0_on,
            )),
            win1: None,
            obj_window: None,
            winout: bg0_on,
        },
        mosaic: crate::mosaic::MosaicConfig {
            bg: MosaicSize::new(4, 1),
            obj: MosaicSize::NONE,
        },
        ..FrameEffects::default()
    };
    let fb = compose_frame_with_effects(&sprites, &[slot], &effects);

    let first_tile_color = Bgr555::from_channels(1, 0, 0).to_rgb888();
    let second_tile_color = Bgr555::from_channels(2, 0, 0).to_rgb888();
    assert_eq!(
        fb.pixel(4, 0),
        Some(first_tile_color),
        "x=4 still holds tile 0, fetched at x=3 inside WIN0"
    );
    assert_eq!(
        fb.pixel(5, 0),
        Some(second_tile_color),
        "x=5 starts a fresh span at the WIN0/WINOUT boundary, seeded from the \
         snapped origin (screen x=4, texture tile 1) -- not tile 0's held \
         color carried over from inside WIN0"
    );
}

#[test]
fn affine_mosaic_hold_does_not_cross_a_zero_width_window_boundary() {
    // mGBA's `_breakWindowInner` still splits the scanline around a
    // zero-width WIN0, resetting the mosaic hold at the split
    // (`mgba/src/gba/renderers/video-software.c:458-496,628-675`)
    // `(behavioral-fidelity)`.
    let (tiles, palette, tilemap) = eight_tile_gradient_affine_bg_fixture();
    let layer = AffineBgLayer::new(&tiles, &palette, &tilemap);
    let scale = 8 * AffineMatrix::ONE;
    let matrix = AffineMatrix::new(scale, 0, 0, AffineMatrix::ONE);
    let reference_x = -24 * i32::from(AffineMatrix::ONE);
    let slot = BgSlot::new_affine(
        layer,
        0,
        0,
        matrix,
        reference_x,
        0,
        Overflow::Transparent,
        true,
    )
    .with_mosaic(true);
    let entries: [OamEntry; 0] = [];
    let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
    let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);

    let mut bg0_on = WindowLayerEnable::NONE;
    bg0_on.bg[0] = true;

    let effects = FrameEffects {
        windows: WindowConfig {
            win0: Some((
                WindowRect::new(WindowRange::new(5, 5), WindowRange::new(0, 1)),
                bg0_on,
            )),
            win1: None,
            obj_window: None,
            winout: bg0_on,
        },
        mosaic: crate::mosaic::MosaicConfig {
            bg: MosaicSize::new(4, 1),
            obj: MosaicSize::NONE,
        },
        ..FrameEffects::default()
    };
    let fb = compose_frame_with_effects(&sprites, &[slot], &effects);

    let first_tile_color = Bgr555::from_channels(1, 0, 0).to_rgb888();
    let second_tile_color = Bgr555::from_channels(2, 0, 0).to_rgb888();
    assert_eq!(
        fb.pixel(4, 0),
        Some(first_tile_color),
        "x=4 still holds tile 0, fetched at x=3 before the split"
    );
    assert_eq!(
        fb.pixel(5, 0),
        Some(second_tile_color),
        "x=5 starts mGBA's suffix segment, so the span reopens seeded from the \
         snapped origin (screen x=4, texture tile 1) -- not tile 0's held color"
    );
}

#[test]
fn affine_mosaic_hold_survives_an_objwin_mask_edge() {
    let (tiles, palette, tilemap) = gradient_affine_bg_fixture();
    let layer = AffineBgLayer::new(&tiles, &palette, &tilemap);
    let one_texture_pixel = i32::from(AffineMatrix::ONE);
    let slot = BgSlot::new_affine(
        layer,
        0,
        0,
        AffineMatrix::IDENTITY,
        -3 * one_texture_pixel,
        0,
        Overflow::Transparent,
        true,
    )
    .with_mosaic(true);

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

    let mut bg0_on = WindowLayerEnable::NONE;
    bg0_on.bg[0] = true;

    let effects = FrameEffects {
        windows: WindowConfig {
            win0: None,
            win1: None,
            obj_window: Some(bg0_on),
            winout: bg0_on,
        },
        mosaic: crate::mosaic::MosaicConfig {
            bg: MosaicSize::new(4, 1),
            obj: MosaicSize::NONE,
        },
        ..FrameEffects::default()
    };
    let fb = compose_frame_with_effects(&sprites, &[slot], &effects);

    let column0 = Bgr555::from_channels(1, 0, 0).to_rgb888();
    assert_eq!(
        fb.pixel(3, 0),
        Some(column0),
        "x=3 (outside the mask) establishes the hold"
    );
    assert_eq!(
        fb.pixel(5, 0),
        Some(column0),
        "x=5 (inside the mask, past the x=4 OBJWIN edge) must still hold \
         column 0's texel -- the edge is not a region boundary"
    );
}

#[test]
fn affine_mosaic_hold_advances_through_pixels_hidden_by_objwin() {
    let (tiles, palette, tilemap) = gradient_affine_bg_fixture();
    let layer = AffineBgLayer::new(&tiles, &palette, &tilemap);
    let one_texture_pixel = i32::from(AffineMatrix::ONE);
    let slot = BgSlot::new_affine(
        layer,
        0,
        0,
        AffineMatrix::IDENTITY,
        -3 * one_texture_pixel,
        0,
        Overflow::Transparent,
        true,
    )
    .with_mosaic(true);

    let mask_tile = bpp4_tile_with_every_row([0, 0, 0, 0, 15, 15, 0, 0]);
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

    let mut bg0_on = WindowLayerEnable::NONE;
    bg0_on.bg[0] = true;

    let effects = FrameEffects {
        windows: WindowConfig {
            win0: None,
            win1: None,
            obj_window: Some(WindowLayerEnable::NONE),
            winout: bg0_on,
        },
        mosaic: crate::mosaic::MosaicConfig {
            bg: MosaicSize::new(4, 1),
            obj: MosaicSize::NONE,
        },
        ..FrameEffects::default()
    };
    let fb = compose_frame_with_effects(&sprites, &[slot], &effects);

    let column0 = Bgr555::from_channels(1, 0, 0).to_rgb888();
    assert_eq!(
        fb.pixel(3, 0),
        Some(column0),
        "x=3 (unmasked) establishes the hold"
    );
    assert_eq!(
        fb.pixel(4, 0),
        Some(crate::palette::Rgb888::BLACK),
        "x=4 is OBJWIN-hidden: nothing composites, but the hold must still advance"
    );
    assert_eq!(
        fb.pixel(5, 0),
        Some(crate::palette::Rgb888::BLACK),
        "x=5 is still OBJWIN-hidden"
    );
    assert_eq!(
        fb.pixel(6, 0),
        Some(column0),
        "x=6 (unmasked again) must still show column 0's held texel -- the \
         hidden columns advanced the hold invisibly rather than resetting it"
    );
    assert_eq!(
        fb.pixel(7, 0),
        Some(Bgr555::from_channels(5, 0, 0).to_rgb888()),
        "x=7 reloads the hold at column 4 only if hidden x=4..5 decremented \
         the counter; a hold paused across the hidden columns would still \
         show column 0 here"
    );
}

#[test]
fn affine_mosaic_hold_advances_through_unmasked_columns_an_objwin_only_bg_cannot_show() {
    let (tiles, palette, tilemap) = gradient_affine_bg_fixture();
    let layer = AffineBgLayer::new(&tiles, &palette, &tilemap);
    let one_texture_pixel = i32::from(AffineMatrix::ONE);
    let slot = BgSlot::new_affine(
        layer,
        0,
        0,
        AffineMatrix::IDENTITY,
        -3 * one_texture_pixel,
        0,
        Overflow::Transparent,
        true,
    )
    .with_mosaic(true);

    let mask_tile = bpp4_tile_with_every_row([0, 0, 0, 0, 15, 15, 0, 0]);
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

    let mut bg0_on = WindowLayerEnable::NONE;
    bg0_on.bg[0] = true;

    let effects = FrameEffects {
        windows: WindowConfig {
            win0: None,
            win1: None,
            obj_window: Some(bg0_on),
            winout: WindowLayerEnable::NONE,
        },
        mosaic: crate::mosaic::MosaicConfig {
            bg: MosaicSize::new(4, 1),
            obj: MosaicSize::NONE,
        },
        ..FrameEffects::default()
    };
    let fb = compose_frame_with_effects(&sprites, &[slot], &effects);

    let column0 = Bgr555::from_channels(1, 0, 0).to_rgb888();
    assert_eq!(
        fb.pixel(3, 0),
        Some(crate::palette::Rgb888::BLACK),
        "x=3 is unmasked (WINOUT never enables BG0 here) but must still \
         establish the hold from OBJWIN's participation alone"
    );
    assert_eq!(
        fb.pixel(4, 0),
        Some(column0),
        "x=4 is masked, so OBJWIN's enable composites the held texel"
    );
    assert_eq!(fb.pixel(5, 0), Some(column0), "x=5 is still masked");
    assert_eq!(
        fb.pixel(6, 0),
        Some(crate::palette::Rgb888::BLACK),
        "x=6 is unmasked again -- composites nothing, but the hold state \
         established at x=3 must have kept advancing underneath"
    );
}
