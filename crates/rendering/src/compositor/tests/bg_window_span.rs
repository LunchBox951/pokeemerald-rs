//! Pins which source texel a regular BG samples inside a narrow window pass,
//! where mGBA's mode-0 suffix macros read a different column than ordinary
//! sampling would (software-mode0.c:18-32, 177-220, 426-446).

use super::super::{compose_frame_with_effects, BgSlot, FrameEffects};
use super::shared::{bg0_spans, empty_sprite_layer, two_tile_gradient_8bpp_fixture};
use crate::oam::OamEntry;
use crate::palette::{Bgr555, Palette};
use crate::tile::{BitDepth, Tileset};
use crate::tilemap::{ScreenEntry, Tilemap};

/// One 4bpp tile whose row 0 gives source column `c` palette index `c + 1`
/// (red channel `c + 1`), in a 1x1 map with the entry's horizontal flip set
/// to `hflip`.
fn column_gradient_4bpp_fixture(hflip: bool) -> (Tileset, Palette, Tilemap) {
    let mut bytes = vec![0u8; BitDepth::Bpp4.tile_byte_len()];
    for (pair, byte) in bytes[..BitDepth::TILE_DIM / 2].iter_mut().enumerate() {
        let low = u8::try_from(2 * pair + 1).unwrap();
        *byte = low | ((low + 1) << 4);
    }
    let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
    let mut colors = [Bgr555::default(); Palette::LEN];
    for (index, color) in colors[1..=BitDepth::TILE_DIM].iter_mut().enumerate() {
        *color = Bgr555::from_channels(u8::try_from(index + 1).unwrap(), 0, 0);
    }
    let tilemap = Tilemap::new(1, 1, vec![ScreenEntry::new(0, hflip, false, 0)]).unwrap();
    (tileset, Palette::new(colors), tilemap)
}

/// Row 0 (x = 0..8) of an unscrolled regular BG0 with no mosaic, shown only
/// inside WIN0 `span`, as red channels; `0` is the black backdrop.
fn regular_row_in_span(fixture: (Tileset, Palette, Tilemap), span: (u8, u8)) -> Vec<u8> {
    let (tileset, palette, tilemap) = fixture;
    let layer = crate::bg::BgLayer::new(&tileset, &palette, &tilemap);
    let slot = BgSlot::new(layer, 0, 0, 0, 0, true);
    let entries: [OamEntry; 0] = [];
    let no_sprite_tiles = Tileset::decode(BitDepth::Bpp4, &[]).unwrap();
    let sprites = empty_sprite_layer(&entries, &no_sprite_tiles);
    let effects = FrameEffects {
        windows: bg0_spans(span, None),
        ..FrameEffects::default()
    };
    let fb = compose_frame_with_effects(&sprites, &[slot], &effects);
    (0..8)
        .map(|x| fb.pixel(x, 0).unwrap())
        .map(|rgb| {
            (0..=31u8)
                .find(|c| Bgr555::from_channels(*c, 0, 0).to_rgb888() == rgb)
                .unwrap()
        })
        .collect()
}

#[test]
fn regular_flipped_4bpp_narrow_window_suffix_counts_back_from_the_span_end() {
    // `DRAW_BACKGROUND_MODE_0_TILE_SUFFIX_16` flipped (software-mode0.c:18-32)
    // for span [5, 6) reads source column 0, not the flipped column 2.
    let row = regular_row_in_span(column_gradient_4bpp_fixture(true), (5, 6));
    assert_eq!(row, vec![0, 0, 0, 0, 0, 1, 0, 0]);
    let row = regular_row_in_span(column_gradient_4bpp_fixture(true), (1, 3));
    assert_eq!(row, vec![0, 2, 1, 0, 0, 0, 0, 0]);
}

#[test]
fn regular_flipped_4bpp_full_and_tile_aligned_spans_match_ordinary_flipping() {
    let fixture = || column_gradient_4bpp_fixture(true);
    assert_eq!(
        regular_row_in_span(fixture(), (0, 8)),
        vec![8, 7, 6, 5, 4, 3, 2, 1]
    );
    assert_eq!(
        regular_row_in_span(fixture(), (1, 8)),
        vec![0, 7, 6, 5, 4, 3, 2, 1]
    );
    assert_eq!(
        regular_row_in_span(fixture(), (0, 3)),
        vec![8, 7, 6, 0, 0, 0, 0, 0]
    );
}

#[test]
fn regular_non_flipped_4bpp_narrow_window_span_matches_ordinary_sampling() {
    let fixture = || column_gradient_4bpp_fixture(false);
    assert_eq!(
        regular_row_in_span(fixture(), (5, 6)),
        vec![0, 0, 0, 0, 0, 6, 0, 0]
    );
    assert_eq!(
        regular_row_in_span(fixture(), (1, 3)),
        vec![0, 2, 3, 0, 0, 0, 0, 0]
    );
}

#[test]
fn regular_8bpp_narrow_window_suffix_matches_mgba_texel_selection() {
    // `DRAW_BACKGROUND_MODE_0_TILE_SUFFIX_256` (software-mode0.c:177-199)
    // for span [1, 3): `end2 = end - 4 = -1` skips the first word, so the
    // second word (columns 4..7) is shifted by `inX & 3 = 1` and columns 5
    // and 6 land on x = 1 and x = 2 (red channels 6 and 7).
    let fixture = || two_tile_gradient_8bpp_fixture(false);
    assert_eq!(
        regular_row_in_span(fixture(), (1, 3)),
        vec![0, 6, 7, 0, 0, 0, 0, 0]
    );
    assert_eq!(
        regular_row_in_span(fixture(), (1, 2)),
        vec![0, 6, 0, 0, 0, 0, 0, 0]
    );
    // The second word runs out before the span does: x = 4 is transparent.
    assert_eq!(
        regular_row_in_span(fixture(), (1, 5)),
        vec![0, 6, 7, 8, 0, 0, 0, 0]
    );
    // A suffix longer than four pixels takes the first-word branch.
    assert_eq!(
        regular_row_in_span(fixture(), (1, 6)),
        vec![0, 2, 5, 6, 7, 8, 0, 0]
    );
}

#[test]
fn regular_flipped_8bpp_narrow_window_suffix_counts_back_from_the_span_end() {
    let fixture = || two_tile_gradient_8bpp_fixture(true);
    assert_eq!(
        regular_row_in_span(fixture(), (1, 3)),
        vec![0, 2, 1, 0, 0, 0, 0, 0]
    );
}

#[test]
fn regular_8bpp_full_and_tile_aligned_spans_match_ordinary_sampling() {
    let fixture = || two_tile_gradient_8bpp_fixture(false);
    assert_eq!(
        regular_row_in_span(fixture(), (0, 8)),
        vec![1, 2, 3, 4, 5, 6, 7, 8]
    );
    assert_eq!(
        regular_row_in_span(fixture(), (1, 8)),
        vec![0, 2, 3, 4, 5, 6, 7, 8]
    );
    assert_eq!(
        regular_row_in_span(fixture(), (0, 3)),
        vec![1, 2, 3, 0, 0, 0, 0, 0]
    );
}
