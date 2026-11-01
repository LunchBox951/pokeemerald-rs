//! Tile conversion contract: raster images, tilemaps, and cropped sheets pack into
//! the tile and tilemap forms the renderer consumes, or fail with a typed error.

use super::test_support::tiled_image;
use super::{
    affine_tilemap_from_raw, crop_and_pack_tile_bytes, image_to_tileset, press_start_tileset,
    regular_tilemap_from_raw, TitleSceneError, PRESS_START_SHEET_TILES,
};
use assets::ImageRef;
use rendering::{BitDepth, RenderError};

#[test]
fn image_to_tileset_packs_tiles_side_by_side_row_major() {
    const LEFT_TILE_VALUE: u8 = 1;
    const RIGHT_TILE_VALUE: u8 = 2;
    let pixels = tiled_image(16, 8, |col, _row| {
        if col == 0 {
            LEFT_TILE_VALUE
        } else {
            RIGHT_TILE_VALUE
        }
    });
    let image = ImageRef {
        width: 16,
        height: 8,
        bit_depth: 4,
        pixels: &pixels,
    };
    let tileset = image_to_tileset("test", image, BitDepth::Bpp4).unwrap();
    assert_eq!(tileset.len(), 2);
    assert_eq!(tileset.tile(0).unwrap().index(0, 0), LEFT_TILE_VALUE);
    assert_eq!(tileset.tile(1).unwrap().index(0, 0), RIGHT_TILE_VALUE);
}

#[test]
fn image_to_tileset_packs_tiles_top_to_bottom_row_major() {
    const TOP_TILE_VALUE: u8 = 3;
    const BOTTOM_TILE_VALUE: u8 = 4;
    let pixels = tiled_image(8, 16, |_col, row| {
        if row == 0 {
            TOP_TILE_VALUE
        } else {
            BOTTOM_TILE_VALUE
        }
    });
    let image = ImageRef {
        width: 8,
        height: 16,
        bit_depth: 4,
        pixels: &pixels,
    };
    let tileset = image_to_tileset("test", image, BitDepth::Bpp4).unwrap();
    assert_eq!(tileset.len(), 2);
    assert_eq!(tileset.tile(0).unwrap().index(0, 0), TOP_TILE_VALUE);
    assert_eq!(tileset.tile(1).unwrap().index(0, 0), BOTTOM_TILE_VALUE);
}

#[test]
fn image_to_tileset_4bpp_reads_low_nibble_as_left_pixel() {
    let mut pixels = vec![0u8; 64];
    pixels[0] = 1;
    pixels[1] = 2;
    let image = ImageRef {
        width: 8,
        height: 8,
        bit_depth: 4,
        pixels: &pixels,
    };
    let tileset = image_to_tileset("test", image, BitDepth::Bpp4).unwrap();
    let tile = tileset.tile(0).unwrap();
    assert_eq!(tile.index(0, 0), 1);
    assert_eq!(tile.index(1, 0), 2);
}

#[test]
fn image_to_tileset_8bpp_is_a_direct_per_tile_copy() {
    let mut pixels = vec![0u8; 64];
    pixels[0] = 200;
    let image = ImageRef {
        width: 8,
        height: 8,
        bit_depth: 8,
        pixels: &pixels,
    };
    let tileset = image_to_tileset("test", image, BitDepth::Bpp8).unwrap();
    assert_eq!(tileset.tile(0).unwrap().index(0, 0), 200);
}

#[test]
fn image_to_tileset_rejects_non_tile_aligned_dimensions() {
    let pixels = vec![0u8; 12 * 8];
    let image = ImageRef {
        width: 12,
        height: 8,
        bit_depth: 4,
        pixels: &pixels,
    };
    let err = image_to_tileset("bogus/id", image, BitDepth::Bpp4).unwrap_err();
    assert_eq!(
        err,
        TitleSceneError::ImageNotTileAligned {
            id: "bogus/id",
            width: 12,
            height: 8,
        }
    );
}

#[test]
fn image_to_tileset_rejects_payload_shorter_than_declared_dimensions() {
    let pixels = vec![0u8; 63];
    let image = ImageRef {
        width: 8,
        height: 8,
        bit_depth: 4,
        pixels: &pixels,
    };
    let err = image_to_tileset("bogus/id", image, BitDepth::Bpp4).unwrap_err();
    assert_eq!(
        err,
        TitleSceneError::ImagePixelCountMismatch {
            id: "bogus/id",
            width: 8,
            height: 8,
            actual: 63,
        }
    );
}

#[test]
fn image_to_tileset_rejects_a_palette_index_a_4bpp_tile_cannot_hold() {
    let mut pixels = vec![0u8; 64];
    pixels[0] = 16;
    let image = ImageRef {
        width: 8,
        height: 8,
        bit_depth: 8,
        pixels: &pixels,
    };
    let err = image_to_tileset("bogus/id", image, BitDepth::Bpp4).unwrap_err();
    assert_eq!(
        err,
        TitleSceneError::ImagePaletteIndexOutOfRange {
            id: "bogus/id",
            index: 16,
        }
    );
}

#[test]
fn image_to_tileset_packs_an_8bpp_source_whose_indices_fit_four_bits() {
    // `title/image/press_start` ships pack-8bpp but is consumed as Bpp4.
    let mut pixels = vec![0u8; 64];
    pixels[0] = 15;
    let image = ImageRef {
        width: 8,
        height: 8,
        bit_depth: 8,
        pixels: &pixels,
    };
    let tileset = image_to_tileset("title/image/press_start", image, BitDepth::Bpp4).unwrap();
    assert_eq!(tileset.tile(0).unwrap().index(0, 0), 15);
}

#[test]
fn regular_tilemap_from_raw_decodes_screen_entries() {
    const TILE_INDEX: u16 = 5;
    const HORIZONTAL_FLIP_FLAG: u16 = 1 << 10;
    const PALETTE_BANK: u8 = 3;
    const PALETTE_BANK_SHIFT: u32 = 12;
    let raw_entry =
        TILE_INDEX | HORIZONTAL_FLIP_FLAG | (u16::from(PALETTE_BANK) << PALETTE_BANK_SHIFT);
    let mut bytes = raw_entry.to_le_bytes().to_vec();
    bytes.extend(std::iter::repeat_n(0u8, 2 * (32 * 32 - 1)));

    let tilemap = regular_tilemap_from_raw(&bytes).unwrap();
    let entry = tilemap.entry(0, 0).unwrap();
    assert_eq!(entry.tile_index(), TILE_INDEX);
    assert!(entry.h_flip());
    assert!(!entry.v_flip());
    assert_eq!(entry.palette_bank(), PALETTE_BANK);
}

#[test]
fn regular_tilemap_from_raw_rejects_wrong_length() {
    let err = regular_tilemap_from_raw(&[0u8; 4]).unwrap_err();
    assert!(matches!(
        err,
        TitleSceneError::Render(RenderError::TilemapSizeMismatch { .. })
    ));
}

#[test]
fn regular_tilemap_from_raw_rejects_an_odd_trailing_byte() {
    let err = regular_tilemap_from_raw(&[0u8; 2 * 32 * 32 + 1]).unwrap_err();
    assert!(matches!(
        err,
        TitleSceneError::Render(RenderError::TilemapSizeMismatch { .. })
    ));
}

#[test]
fn regular_tilemap_from_raw_reports_a_distinct_count_when_short_by_one_byte() {
    let err = regular_tilemap_from_raw(&[0u8; 2 * 32 * 32 - 1]).unwrap_err();
    let TitleSceneError::Render(RenderError::TilemapSizeMismatch { expected, actual }) = err else {
        panic!("expected a TilemapSizeMismatch error, got {err:?}");
    };
    assert_eq!(expected, 1024);
    assert_eq!(
        actual, 1023,
        "a 2,047-byte map reports the whole entries it holds"
    );
}

#[test]
fn affine_tilemap_from_raw_decodes_flat_tile_indices() {
    let mut bytes = vec![0u8; 32 * 32];
    bytes[0] = 42;
    let tilemap = affine_tilemap_from_raw(&bytes).unwrap();
    assert_eq!(tilemap.tile_index(0, 0), Some(42));
}

#[test]
fn affine_tilemap_from_raw_rejects_wrong_length() {
    let err = affine_tilemap_from_raw(&[0u8; 4]).unwrap_err();
    assert!(matches!(
        err,
        TitleSceneError::Render(RenderError::AffineTilemapSizeMismatch { .. })
    ));
}

#[test]
fn press_start_tileset_packs_the_upstream_41_tile_raster_sheet() {
    // Each tile carries its raster index in its first two 4bpp nibbles, so a
    // transposed, dropped, or duplicated tile mismatches; only 0..=40 pack.
    const SHEET_TILE_COLS: usize = 20;
    let mut pixels = vec![0u8; 160 * 24];
    for tile_row in 0..3usize {
        for tile_col in 0..SHEET_TILE_COLS {
            let tile_number = tile_row * SHEET_TILE_COLS + tile_col;
            let low = u8::try_from(tile_number & 0x0F).unwrap();
            let high = u8::try_from((tile_number >> 4) & 0x0F).unwrap();
            let row_start = tile_row * 8 * 160 + tile_col * 8;
            pixels[row_start] = low;
            pixels[row_start + 1] = high;
        }
    }
    let image = ImageRef {
        width: 160,
        height: 24,
        bit_depth: 4,
        pixels: &pixels,
    };
    let tileset = press_start_tileset("test", image).unwrap();

    assert_eq!(tileset.len(), usize::from(PRESS_START_SHEET_TILES));
    for index in 0..PRESS_START_SHEET_TILES {
        let tile = tileset.tile(index).unwrap();
        let decoded = tile.index(0, 0) | (tile.index(1, 0) << 4);
        assert_eq!(decoded, u8::try_from(index).unwrap(), "tile {index}");
    }
}

#[test]
fn crop_and_pack_tile_bytes_crops_the_requested_sub_rectangle() {
    let pixels = tiled_image(16, 8, |col, _row| if col == 0 { 1 } else { 2 });
    let image = ImageRef {
        width: 16,
        height: 8,
        bit_depth: 4,
        pixels: &pixels,
    };
    let packed = crop_and_pack_tile_bytes("test", image, 8, 0, 8, 8, BitDepth::Bpp4).unwrap();
    let tileset = rendering::Tileset::decode(BitDepth::Bpp4, &packed).unwrap();
    assert_eq!(tileset.len(), 1);
    assert_eq!(tileset.tile(0).unwrap().index(0, 0), 2);
}

#[test]
fn crop_and_pack_tile_bytes_rejects_a_short_payload_instead_of_panicking() {
    let pixels = vec![0u8; 16 * 8 - 1];
    let image = ImageRef {
        width: 16,
        height: 8,
        bit_depth: 4,
        pixels: &pixels,
    };
    let err =
        crop_and_pack_tile_bytes("bogus/sheet", image, 8, 0, 8, 8, BitDepth::Bpp4).unwrap_err();
    assert_eq!(
        err,
        TitleSceneError::ImagePixelCountMismatch {
            id: "bogus/sheet",
            width: 16,
            height: 8,
            actual: 16 * 8 - 1,
        }
    );
}

#[test]
fn crop_and_pack_tile_bytes_rejects_a_palette_index_a_4bpp_tile_cannot_hold() {
    let pixels = tiled_image(16, 8, |col, _row| if col == 0 { 1 } else { 16 });
    let image = ImageRef {
        width: 16,
        height: 8,
        bit_depth: 4,
        pixels: &pixels,
    };
    let err =
        crop_and_pack_tile_bytes("bogus/sheet", image, 8, 0, 8, 8, BitDepth::Bpp4).unwrap_err();
    assert_eq!(
        err,
        TitleSceneError::ImagePaletteIndexOutOfRange {
            id: "bogus/sheet",
            index: 16,
        }
    );
}

#[test]
fn press_start_tileset_rejects_a_palette_index_a_4bpp_tile_cannot_hold() {
    // The shipping ROM-4bpp/pack-8bpp sheet takes this same crop path.
    let mut pixels = vec![0u8; 160 * 24];
    pixels[0] = 16;
    let image = ImageRef {
        width: 160,
        height: 24,
        bit_depth: 8,
        pixels: &pixels,
    };
    let err = press_start_tileset("title/image/press_start", image).unwrap_err();
    assert_eq!(
        err,
        TitleSceneError::ImagePaletteIndexOutOfRange {
            id: "title/image/press_start",
            index: 16,
        }
    );
}
