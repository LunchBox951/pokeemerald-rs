//! Pins the cross-domain compositor test fixtures every domain module shares.

use crate::bg_affine::AffineTilemap;
use crate::oam::OamEntry;
use crate::palette::{Bgr555, Palette};
use crate::sprite::SpriteLayer;
use crate::tile::{BitDepth, Tileset};
use crate::tilemap::{ScreenEntry, Tilemap};
use crate::window::{WindowConfig, WindowLayerEnable, WindowRange, WindowRect};

/// A fully opaque 1x1-tile (8x8px) BG layer using palette index 15 in
/// bank 0, plus its owning tileset/palette/tilemap (kept alive by the
/// caller for the lifetime of the returned [`crate::bg::BgLayer`]).
pub(super) fn opaque_bg_fixture(color_channel: u8) -> (Tileset, Palette, Tilemap) {
    opaque_bg_color_fixture(Bgr555::from_channels(color_channel, 0, 0))
}

/// An opaque regular BG fixture with the supplied full palette color.
pub(super) fn opaque_bg_color_fixture(color: Bgr555) -> (Tileset, Palette, Tilemap) {
    let tileset = Tileset::decode(BitDepth::Bpp4, &[0xFFu8; 32]).unwrap();
    let mut colors = [Bgr555::default(); Palette::LEN];
    colors[15] = color;
    let palette = Palette::new(colors);
    let entries = vec![ScreenEntry::new(0, false, false, 0)];
    let tilemap = Tilemap::new(1, 1, entries).unwrap();
    (tileset, palette, tilemap)
}

pub(super) fn empty_sprite_layer<'a>(
    entries: &'a [OamEntry],
    tileset: &'a Tileset,
) -> SpriteLayer<'a> {
    SpriteLayer::new(entries, tileset, tileset, &EMPTY_PALETTE)
}

// A palette of all-default (black, and every index resolves to the same
// color) used only where sprite entries are empty and never sampled.
static EMPTY_PALETTE: Palette = Palette::new([Bgr555::from_raw(0); Palette::LEN]);

/// A fully opaque 1x1-tile (8x8px) *affine* BG layer using flat 8bpp
/// palette index 200, plus its owning tileset/palette/tilemap.
pub(super) fn opaque_affine_bg_fixture(color_channel: u8) -> (Tileset, Palette, AffineTilemap) {
    opaque_affine_bg_color_fixture(Bgr555::from_channels(color_channel, 0, 0))
}

/// An opaque affine BG fixture with the supplied full palette color.
pub(super) fn opaque_affine_bg_color_fixture(color: Bgr555) -> (Tileset, Palette, AffineTilemap) {
    let tileset = Tileset::decode(BitDepth::Bpp8, &[200u8; 64]).unwrap();
    let mut colors = [Bgr555::default(); Palette::LEN];
    colors[200] = color;
    let palette = Palette::new(colors);
    let tilemap = AffineTilemap::new(1, 1, vec![0]).unwrap();
    (tileset, palette, tilemap)
}

pub(super) fn bpp4_row(
    palette_indices_by_column: [u8; BitDepth::TILE_DIM],
) -> [u8; BitDepth::TILE_DIM / 2] {
    let mut row = [0u8; BitDepth::TILE_DIM / 2];
    for (byte, [left, right]) in row
        .iter_mut()
        .zip(palette_indices_by_column.as_chunks::<2>().0)
    {
        *byte = (right << 4) | left;
    }
    row
}

pub(super) fn bpp4_tile_with_top_left_2x2(
    palette_indices_by_row: [[u8; 2]; 2],
) -> [u8; BitDepth::Bpp4.tile_byte_len()] {
    const BYTES_PER_ROW: usize = BitDepth::TILE_DIM / 2;
    let mut bytes = [0u8; BitDepth::Bpp4.tile_byte_len()];
    for (row, [left, right]) in palette_indices_by_row.into_iter().enumerate() {
        bytes[row * BYTES_PER_ROW] = (right << 4) | left;
    }
    bytes
}

pub(super) fn bpp4_tile_with_every_row(
    palette_indices_by_column: [u8; BitDepth::TILE_DIM],
) -> [u8; BitDepth::Bpp4.tile_byte_len()] {
    let mut bytes = [0u8; BitDepth::Bpp4.tile_byte_len()];
    for row in bytes.as_chunks_mut::<{ BitDepth::TILE_DIM / 2 }>().0 {
        row.copy_from_slice(&bpp4_row(palette_indices_by_column));
    }
    bytes
}

/// A 2x1 regular 8bpp BG whose first row gives every screen texel a distinct
/// opaque color: tile `t`, column `c` is palette index (and red channel)
/// `8 * t + c + 1`. `first_hflip` flips the first map entry.
pub(super) fn two_tile_gradient_8bpp_fixture(first_hflip: bool) -> (Tileset, Palette, Tilemap) {
    const TILE_COUNT: usize = 2;
    let tile_byte_len = BitDepth::Bpp8.tile_byte_len();
    let mut bytes = vec![0u8; tile_byte_len * TILE_COUNT];
    for (tile, chunk) in bytes.chunks_exact_mut(tile_byte_len).enumerate() {
        for (column, byte) in chunk[..BitDepth::TILE_DIM].iter_mut().enumerate() {
            *byte = u8::try_from(tile * BitDepth::TILE_DIM + column + 1).unwrap();
        }
    }
    let tileset = Tileset::decode(BitDepth::Bpp8, &bytes).unwrap();
    let mut colors = [Bgr555::default(); Palette::LEN];
    for (index, color) in colors[1..=TILE_COUNT * BitDepth::TILE_DIM]
        .iter_mut()
        .enumerate()
    {
        *color = Bgr555::from_channels(u8::try_from(index + 1).unwrap(), 0, 0);
    }
    let entries = vec![
        ScreenEntry::new(0, first_hflip, false, 0),
        ScreenEntry::new(1, false, false, 0),
    ];
    let tilemap = Tilemap::new(2, 1, entries).unwrap();
    (tileset, Palette::new(colors), tilemap)
}

/// WIN0/WIN1 spans on line 0 showing BG0 only inside them.
pub(super) fn bg0_spans(win0: (u8, u8), win1: Option<(u8, u8)>) -> WindowConfig {
    let mut bg0_on = WindowLayerEnable::NONE;
    bg0_on.bg[0] = true;
    let rect = |(start, end)| WindowRect::new(WindowRange::new(start, end), WindowRange::new(0, 1));
    WindowConfig {
        win0: Some((rect(win0), bg0_on)),
        win1: win1.map(|span| (rect(span), bg0_on)),
        obj_window: None,
        winout: WindowLayerEnable::NONE,
    }
}
