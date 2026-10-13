//! Pins regular 8bpp BG mosaic prefetch/hold behavior under window spans.

use super::super::{compose_frame_with_effects, BgSlot, FrameEffects};
use super::shared::{bg0_spans, empty_sprite_layer, two_tile_gradient_8bpp_fixture};
use crate::mosaic::MosaicSize;
use crate::oam::OamEntry;
use crate::palette::{Bgr555, Palette};
use crate::tile::{BitDepth, Tileset};
use crate::tilemap::{ScreenEntry, Tilemap};
use crate::window::WindowConfig;

/// Row 0 (x = 0..12) of a regular 8bpp mosaic BG (`h` x 1) under `windows`,
/// as red channels; `0` is the black backdrop.
fn regular_8bpp_mosaic_row(
    h: u8,
    scroll_x: u16,
    first_hflip: bool,
    windows: WindowConfig,
) -> Vec<u8> {
    let (tileset, palette, tilemap) = two_tile_gradient_8bpp_fixture(first_hflip);
    let layer = crate::bg::BgLayer::new(&tileset, &palette, &tilemap);
    let slot = BgSlot::new(layer, 0, 0, scroll_x, 0, true).with_mosaic(true);
    let entries: [OamEntry; 0] = [];
    let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
    let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);
    let effects = FrameEffects {
        windows,
        mosaic: crate::mosaic::MosaicConfig {
            bg: MosaicSize::new(h, 1),
            obj: MosaicSize::NONE,
        },
        ..FrameEffects::default()
    };
    let fb = compose_frame_with_effects(&sprites, &[slot], &effects);
    (0..12)
        .map(|x| fb.pixel(x, 0).unwrap())
        .map(|rgb| {
            (0..=31u8)
                .find(|c| Bgr555::from_channels(*c, 0, 0).to_rgb888() == rgb)
                .unwrap()
        })
        .collect()
}

#[test]
fn regular_8bpp_mosaic_window_span_starting_mid_block_prefetches_span_start_texel() {
    // mGBA `DRAW_BACKGROUND_MODE_0_MOSAIC_256` (software-mode0.c:320-366)
    // prefetches the pixel column `inX & 7` of the span start, not the
    // block origin's, and holds it for `mosaicWait` columns; the draw call
    // restarts per window span (video-software.c:628-639).
    let row = regular_8bpp_mosaic_row(4, 0, false, bg0_spans((1, 4), None));
    assert_eq!(&row[..6], &[0, 2, 2, 2, 0, 0]);
}

#[test]
fn regular_8bpp_mosaic_prefetch_reloads_at_the_next_block_boundary() {
    let row = regular_8bpp_mosaic_row(4, 0, false, bg0_spans((1, 8), None));
    assert_eq!(&row[..9], &[0, 2, 2, 2, 5, 5, 5, 5, 0]);
}

#[test]
fn regular_8bpp_mosaic_prefetch_honors_horizontal_flip() {
    // Pixel column `7 - x` of the flipped entry; the boundary reload then
    // samples the flipped column under x = 4 (column 3).
    let row = regular_8bpp_mosaic_row(4, 0, true, bg0_spans((1, 8), None));
    assert_eq!(&row[1..8], &[7, 7, 7, 4, 4, 4, 4]);
}

#[test]
fn regular_8bpp_mosaic_prefetch_uses_the_scrolled_span_start_column() {
    // HOFS=2: span start 1 scrolls to pixel column 3 (held), and the
    // reload at x=4 samples scrolled column 6.
    let row = regular_8bpp_mosaic_row(4, 2, false, bg0_spans((1, 8), None));
    assert_eq!(&row[1..8], &[4, 4, 4, 7, 7, 7, 7]);
}

#[test]
fn regular_8bpp_mosaic_prefetch_steps_back_a_tile_when_the_origin_is_negative() {
    // HOFS=7: span start 1 scrolls to tile 1 pixel column 0, but the block
    // origin lies in the previous tile, which mGBA's `(16 + baseX) >> 3`
    // rule selects while keeping pixel column 0 (software-mode0.c:325-335).
    let row = regular_8bpp_mosaic_row(4, 7, false, bg0_spans((1, 8), None));
    assert_eq!(&row[1..8], &[1, 1, 1, 12, 12, 12, 12]);
}

#[test]
fn regular_8bpp_mosaic_prefetch_restarts_at_every_pass_boundary() {
    // A second span reopened by WIN1 starts mid-block again and prefetches
    // its own start texel (column 5 at x=5), not the first span's.
    let row = regular_8bpp_mosaic_row(4, 0, false, bg0_spans((1, 3), Some((5, 8))));
    assert_eq!(&row[..9], &[0, 2, 2, 0, 0, 6, 6, 6, 0]);
}

#[test]
fn regular_8bpp_mosaic_span_starting_on_a_block_boundary_matches_the_snap() {
    let row = regular_8bpp_mosaic_row(4, 0, false, bg0_spans((4, 8), None));
    assert_eq!(&row[3..9], &[0, 5, 5, 5, 5, 0]);
}

#[test]
fn regular_8bpp_mosaic_prefetch_wraps_within_a_non_power_of_two_tilemap() {
    // A 3-tile-wide map wraps the scrolled coordinate at 24 pixels, not at
    // the 512-pixel register width: HOFS=511 puts span start 5 on tile
    // `516 % 24 / 8 = 1`, which the prefetch must hold (flat color 2).
    const TILE_COUNT: usize = 3;
    let tile_byte_len = BitDepth::Bpp8.tile_byte_len();
    let mut bytes = vec![0u8; tile_byte_len * TILE_COUNT];
    for (tile, chunk) in bytes.chunks_exact_mut(tile_byte_len).enumerate() {
        chunk.fill(u8::try_from(tile + 1).unwrap());
    }
    let tileset = Tileset::decode(BitDepth::Bpp8, &bytes).unwrap();
    let mut colors = [Bgr555::default(); Palette::LEN];
    for (index, color) in colors[1..=TILE_COUNT].iter_mut().enumerate() {
        *color = Bgr555::from_channels(u8::try_from(index + 1).unwrap(), 0, 0);
    }
    let palette = Palette::new(colors);
    let entries = (0..TILE_COUNT)
        .map(|tile| ScreenEntry::new(u16::try_from(tile).unwrap(), false, false, 0))
        .collect();
    let tilemap = Tilemap::new(TILE_COUNT, 1, entries).unwrap();
    let layer = crate::bg::BgLayer::new(&tileset, &palette, &tilemap);
    let slot = BgSlot::new(layer, 0, 0, 511, 0, true).with_mosaic(true);
    let sprite_entries: [OamEntry; 0] = [];
    let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
    let sprites = empty_sprite_layer(&sprite_entries, &no_sprite_tiles);
    let effects = FrameEffects {
        windows: bg0_spans((5, 8), None),
        mosaic: crate::mosaic::MosaicConfig {
            bg: MosaicSize::new(4, 1),
            obj: MosaicSize::NONE,
        },
        ..FrameEffects::default()
    };
    let fb = compose_frame_with_effects(&sprites, &[slot], &effects);
    let held = Some(Bgr555::from_channels(2, 0, 0).to_rgb888());
    assert_eq!(
        (5..8).map(|x| fb.pixel(x, 0)).collect::<Vec<_>>(),
        vec![held; 3]
    );
}
