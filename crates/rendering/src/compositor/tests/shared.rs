//! Pins the cross-domain compositor test fixtures every domain module shares.

use crate::bg_affine::AffineTilemap;
use crate::oam::OamEntry;
use crate::palette::{Bgr555, Palette};
use crate::sprite::SpriteLayer;
use crate::tile::{BitDepth, Tileset};
use crate::tilemap::{ScreenEntry, Tilemap};

/// A fully opaque 1x1-tile (8x8px) BG layer using palette index 15 in
/// bank 0, plus its owning tileset/palette/tilemap (kept alive by the
/// caller for the lifetime of the returned [`crate::bg::BgLayer`]).
pub(super) fn opaque_bg_fixture(color_channel: u8) -> (Tileset, Palette, Tilemap) {
    let tileset = Tileset::decode(BitDepth::Bpp4, &[0xFFu8; 32]).unwrap();
    let mut colors = [Bgr555::default(); Palette::LEN];
    colors[15] = Bgr555::from_channels(color_channel, 0, 0);
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
    let tileset = Tileset::decode(BitDepth::Bpp8, &[200u8; 64]).unwrap();
    let mut colors = [Bgr555::default(); Palette::LEN];
    colors[200] = Bgr555::from_channels(color_channel, 0, 0);
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
