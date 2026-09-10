//! Round-trip tests binding this module's raster-to-tiles packer to
//! [`pack_format`]'s tiles-to-raster unpacker.
//!
//! The two are inverses by contract, not by coincidence:
//! [`super::pack_tile_bytes`] turns a pack image entry back into the GBA's
//! packed tile bytes, and `pack_format::image_entry_from_tiles` is how the
//! ROM importer will turn a ROM's tile bytes into that same pack entry. If
//! either side's pixel or tile order drifts, imported art renders scrambled
//! while every isolated unit test on each side still passes. These tests
//! fail instead.
//!
//! Only the plain row-major tile order round-trips here.
//! `image_entry_from_tiles`'s metatile order is `gbagfx`'s
//! `-mwidth`/`-mheight` shape, which this module's packer does not (and
//! need not) reproduce; `pack_format`'s own tests cover it.

use super::{image_to_tileset, pack_tile_bytes};
use assets::ImageRef;
use rendering::{BitDepth, Tileset};

/// A synthetic tile set of `tile_count` tiles at `bytes_per_tile`, filled
/// with a deterministic byte sequence that repeats on no smaller period
/// than the whole buffer, so a transposed row, tile, or nibble shows up as
/// a mismatch rather than landing on an equal value.
fn synthetic_tiles(tile_count: usize, bytes_per_tile: usize) -> Vec<u8> {
    (0..tile_count * bytes_per_tile)
        .map(|i| {
            // 37 is coprime with 256, so the sequence walks every byte
            // value before repeating.
            u8::try_from((i * 37 + 11) % 256).expect("modulo 256 fits in u8")
        })
        .collect()
}

/// Assert that unpacking `tiles` into a `width` x `height` raster and
/// packing that raster back yields exactly `tiles` again.
fn assert_round_trips(tiles: &[u8], bit_depth: BitDepth, depth_bits: u8, width: u32, height: u32) {
    let entry = pack_format::image_entry_from_tiles(
        "test/tiles".into(),
        tiles,
        depth_bits,
        width,
        height,
        None,
    )
    .expect("synthetic tiles are a whole, exactly-sized tile grid");
    let payload = entry.payload;
    assert_eq!(payload.len(), (width as usize) * (height as usize));

    let repacked = pack_tile_bytes(width as usize, height as usize, &payload, bit_depth);
    assert_eq!(
        repacked, tiles,
        "{depth_bits}bpp {width}x{height} did not survive the raster round trip"
    );

    // The same raster through the production entry point must decode to the
    // tileset the original tile bytes describe.
    let image = ImageRef {
        width,
        height,
        bit_depth: depth_bits,
        pixels: &payload,
    };
    let round_tripped = image_to_tileset("test/tiles", image, bit_depth).unwrap();
    let direct = Tileset::decode(bit_depth, tiles).unwrap();
    assert_eq!(round_tripped.len(), direct.len());
    for index in 0..u16::try_from(direct.len()).expect("synthetic tilesets are tiny") {
        let (a, b) = (
            round_tripped.tile(index).unwrap(),
            direct.tile(index).unwrap(),
        );
        for y in 0..8 {
            for x in 0..8 {
                assert_eq!(a.index(x, y), b.index(x, y), "tile {index} pixel ({x},{y})");
            }
        }
    }
}

#[test]
fn a_4bpp_tile_grid_survives_unpacking_and_repacking() {
    // 4x3 tiles: wider than tall and taller than one row, so a swapped
    // tile-row/tile-column walk cannot pass.
    let tiles = synthetic_tiles(12, 32);
    assert_round_trips(&tiles, BitDepth::Bpp4, 4, 32, 24);
}

#[test]
fn an_8bpp_tile_grid_survives_unpacking_and_repacking() {
    let tiles = synthetic_tiles(12, 64);
    assert_round_trips(&tiles, BitDepth::Bpp8, 8, 32, 24);
}

#[test]
fn a_single_tile_survives_unpacking_and_repacking() {
    assert_round_trips(&synthetic_tiles(1, 32), BitDepth::Bpp4, 4, 8, 8);
    assert_round_trips(&synthetic_tiles(1, 64), BitDepth::Bpp8, 8, 8, 8);
}

#[test]
fn a_single_tile_column_survives_unpacking_and_repacking() {
    // 1x4 tiles: proves the unpacker advances down tile rows, which a
    // width-1 grid is the only shape to isolate.
    assert_round_trips(&synthetic_tiles(4, 64), BitDepth::Bpp8, 8, 8, 32);
}
