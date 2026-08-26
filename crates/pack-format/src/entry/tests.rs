//! Unit tests for the entry constructors.

use super::{
    image_entry, image_entry_from_tiles, palette_entry, raw_entry, tiles_from_image,
    EntryShapeError, TILE_DIM,
};
use crate::layout::EntryKind;

/// Build one 8bpp tile whose every pixel is `value`.
fn solid_tile_8bpp(value: u8) -> Vec<u8> {
    vec![value; 64]
}

/// Build one 4bpp tile whose every pixel is `value` (0..=15).
fn solid_tile_4bpp(value: u8) -> Vec<u8> {
    let nibble = value & 0x0F;
    vec![nibble | (nibble << 4); 32]
}

#[test]
fn palette_entry_packs_little_endian_u16_per_colour() {
    let entry = palette_entry("p".into(), &[0x7FFF, 0x0000, 0x1234]).unwrap();
    assert_eq!(entry.kind, EntryKind::Palette { color_count: 3 });
    assert_eq!(entry.payload, vec![0xFF, 0x7F, 0x00, 0x00, 0x34, 0x12]);
}

#[test]
fn palette_entry_payload_length_matches_its_declared_colour_count() {
    let entry = palette_entry("p".into(), &[0; 256]).unwrap();
    let EntryKind::Palette { color_count } = entry.kind else {
        panic!("expected a palette entry");
    };
    assert_eq!(entry.payload.len(), usize::from(color_count) * 2);
}

#[test]
fn palette_entry_rejects_more_colours_than_u16_can_count() {
    let colors = vec![0u16; usize::from(u16::MAX) + 1];
    assert_eq!(
        palette_entry("p".into(), &colors).unwrap_err(),
        EntryShapeError::PaletteColorCountUnrepresentable(65536)
    );
}

#[test]
fn image_entry_keeps_the_raster_verbatim() {
    let entry = image_entry("i".into(), 2, 3, 4, vec![1, 2, 3, 4, 5, 6]).unwrap();
    assert_eq!(
        entry.kind,
        EntryKind::Image {
            width: 2,
            height: 3,
            bit_depth: 4,
        }
    );
    assert_eq!(entry.payload, vec![1, 2, 3, 4, 5, 6]);
}

#[test]
fn image_entry_rejects_a_buffer_that_is_not_width_times_height() {
    assert_eq!(
        image_entry("i".into(), 2, 3, 4, vec![0; 5]).unwrap_err(),
        EntryShapeError::ImageSizeMismatch {
            width: 2,
            height: 3,
            len: 5,
        }
    );
}

#[test]
fn raw_entry_copies_its_payload_under_the_raw_tag() {
    let entry = raw_entry("r".into(), vec![9, 8, 7]);
    assert_eq!(entry.kind, EntryKind::Raw);
    assert_eq!(entry.payload, vec![9, 8, 7]);
    assert_eq!(entry.id, "r");
}

#[test]
fn tiles_unpack_8bpp_one_byte_per_pixel() {
    let mut tiles = solid_tile_8bpp(0xAA);
    tiles.extend(solid_tile_8bpp(0xBB));
    let entry = image_entry_from_tiles("i".into(), &tiles, 8, 16, 8, None).unwrap();
    assert_eq!(entry.payload.len(), 128);
    // Row 0: eight 0xAA (left tile) then eight 0xBB (right tile).
    assert_eq!(&entry.payload[0..8], &[0xAA; 8]);
    assert_eq!(&entry.payload[8..16], &[0xBB; 8]);
}

#[test]
fn tiles_unpack_4bpp_low_nibble_first() {
    // One tile whose every byte is 0x21: low nibble 1 (left pixel), high
    // nibble 2 (right pixel).
    let tiles = vec![0x21u8; 32];
    let entry = image_entry_from_tiles("i".into(), &tiles, 4, 8, 8, None).unwrap();
    assert_eq!(entry.payload.len(), 64);
    assert_eq!(&entry.payload[0..8], &[1, 2, 1, 2, 1, 2, 1, 2]);
}

#[test]
fn tiles_fill_the_grid_row_major_without_a_metatile() {
    // 2x2 tiles, each solid with its own index, so the raster's tile
    // corners identify which tile landed where.
    let mut tiles = Vec::new();
    for value in 1..=4u8 {
        tiles.extend(solid_tile_8bpp(value));
    }
    let entry = image_entry_from_tiles("i".into(), &tiles, 8, 16, 16, None).unwrap();
    let at = |x: usize, y: usize| entry.payload[y * 16 + x];
    assert_eq!((at(0, 0), at(8, 0), at(0, 8), at(8, 8)), (1, 2, 3, 4));
}

#[test]
fn metatiles_group_tiles_before_advancing_across_the_image() {
    // 4x2 tiles cut into 2x2 metatiles: tiles 1..=4 fill the left
    // metatile, 5..=8 the right one. Plain row-major order would instead
    // put tile 3 at the raster's bottom-left.
    let mut tiles = Vec::new();
    for value in 1..=8u8 {
        tiles.extend(solid_tile_8bpp(value));
    }
    let entry = image_entry_from_tiles("i".into(), &tiles, 8, 32, 16, Some((2, 2))).unwrap();
    let at = |x: usize, y: usize| entry.payload[y * 32 + x];
    assert_eq!((at(0, 0), at(8, 0), at(16, 0), at(24, 0)), (1, 2, 5, 6));
    assert_eq!((at(0, 8), at(8, 8), at(16, 8), at(24, 8)), (3, 4, 7, 8));
}

#[test]
fn a_one_by_one_metatile_is_the_plain_row_major_order() {
    let mut tiles = Vec::new();
    for value in 1..=4u8 {
        tiles.extend(solid_tile_8bpp(value));
    }
    let plain = image_entry_from_tiles("i".into(), &tiles, 8, 16, 16, None).unwrap();
    let unit = image_entry_from_tiles("i".into(), &tiles, 8, 16, 16, Some((1, 1))).unwrap();
    assert_eq!(plain.payload, unit.payload);
}

#[test]
fn missing_trailing_tiles_zero_fill_the_rest_of_the_raster() {
    // `gbagfx`'s `-num_tiles` cut drops all-zero trailing tiles, so a
    // 2x2-tile image can arrive as one tile of data.
    let tiles = solid_tile_8bpp(7);
    let entry = image_entry_from_tiles("i".into(), &tiles, 8, 16, 16, None).unwrap();
    assert_eq!(entry.payload.len(), 256);
    assert_eq!(entry.payload[0], 7);
    assert!(entry.payload[8..16].iter().all(|&p| p == 0));
    assert!(entry.payload[128..].iter().all(|&p| p == 0));
}

#[test]
fn more_tiles_than_the_raster_needs_is_an_error() {
    let mut tiles = solid_tile_8bpp(1);
    tiles.extend(solid_tile_8bpp(2));
    assert_eq!(
        image_entry_from_tiles("i".into(), &tiles, 8, 8, 8, None).unwrap_err(),
        EntryShapeError::TileDataLength {
            expected: 64,
            actual: 128,
        }
    );
}

#[test]
fn a_partial_trailing_tile_is_an_error() {
    assert_eq!(
        image_entry_from_tiles("i".into(), &[0u8; 40], 8, 16, 8, None).unwrap_err(),
        EntryShapeError::TileDataLength {
            expected: 128,
            actual: 40,
        }
    );
}

#[test]
fn a_non_tile_aligned_size_is_an_error() {
    assert_eq!(
        image_entry_from_tiles("i".into(), &[], 8, 12, 8, None).unwrap_err(),
        EntryShapeError::NotTileAligned {
            width: 12,
            height: 8,
        }
    );
}

#[test]
fn a_bit_depth_other_than_4_or_8_is_an_error() {
    assert_eq!(
        image_entry_from_tiles("i".into(), &[], 2, 8, 8, None).unwrap_err(),
        EntryShapeError::UnsupportedBitDepth(2)
    );
}

#[test]
fn a_metatile_that_does_not_divide_the_grid_is_an_error() {
    assert_eq!(
        image_entry_from_tiles("i".into(), &[], 8, 24, 8, Some((2, 1))).unwrap_err(),
        EntryShapeError::MetatileMisaligned {
            metatile_width: 2,
            metatile_height: 1,
            tiles_wide: 3,
            tiles_high: 1,
        }
    );
}

#[test]
fn a_zero_sided_metatile_is_an_error_rather_than_a_division_by_zero() {
    assert!(matches!(
        image_entry_from_tiles("i".into(), &[], 8, 8, 8, Some((0, 1))).unwrap_err(),
        EntryShapeError::MetatileMisaligned { .. }
    ));
}

#[test]
fn errors_render_a_message_naming_the_offending_shape() {
    let rendered = EntryShapeError::ImageSizeMismatch {
        width: 4,
        height: 4,
        len: 3,
    }
    .to_string();
    assert!(rendered.contains("4x4"), "{rendered}");
    assert!(rendered.contains('3'), "{rendered}");
}

#[test]
fn tile_dim_matches_the_gba_tile_side() {
    assert_eq!(TILE_DIM, 8);
    // The 4bpp/8bpp tile sizes the unpacker assumes follow from it.
    assert_eq!(TILE_DIM * TILE_DIM / 2, 32);
    assert_eq!(TILE_DIM * TILE_DIM, 64);
}

#[test]
fn solid_4bpp_tiles_unpack_to_their_index() {
    let tiles = solid_tile_4bpp(5);
    let entry = image_entry_from_tiles("i".into(), &tiles, 4, 8, 8, None).unwrap();
    assert!(entry.payload.iter().all(|&p| p == 5));
}

#[test]
fn tiles_round_trip_through_the_raster_for_every_metatile_shape() {
    // A 32x32 (4x4 tile) sheet whose every pixel is distinct enough that a
    // wrong metatile walk cannot round-trip by accident.
    let tiles_wide = 4u32;
    let tiles_high = 4u32;
    let mut tiles = vec![0u8; (tiles_wide * tiles_high) as usize * 32];
    for (index, byte) in tiles.iter_mut().enumerate() {
        let value = u8::try_from(index % 251).unwrap();
        *byte = (value & 0x0F) | ((value.wrapping_add(7) & 0x0F) << 4);
    }
    for mw in [1, 2, 4] {
        for mh in [1, 2, 4] {
            let shape = Some((mw, mh));
            let entry = image_entry_from_tiles(
                "i".into(),
                &tiles,
                4,
                tiles_wide * 8,
                tiles_high * 8,
                shape,
            )
            .unwrap();
            let back =
                tiles_from_image(&entry.payload, 4, tiles_wide * 8, tiles_high * 8, shape).unwrap();
            assert_eq!(back, tiles, "4bpp round trip failed at metatile {mw}x{mh}");
        }
    }
}

#[test]
fn tiles_round_trip_at_8bpp() {
    let tiles: Vec<u8> = (0..64 * 6)
        .map(|i| u8::try_from(i % 256).unwrap())
        .collect();
    let entry = image_entry_from_tiles("i".into(), &tiles, 8, 24, 16, Some((3, 1))).unwrap();
    let back = tiles_from_image(&entry.payload, 8, 24, 16, Some((3, 1))).unwrap();
    assert_eq!(back, tiles);
}

#[test]
fn packing_tiles_keeps_only_the_low_nibble_at_4bpp() {
    // A raster whose indices exceed 15 (an 8-bit-indexed PNG feeding a 4bpp
    // ROM sheet, e.g. the title screen's press-start banner) packs to the
    // low nibble, which is all a 4bpp tile can hold.
    let pixels = vec![0x1Au8; 64];
    let tiles = tiles_from_image(&pixels, 4, 8, 8, None).unwrap();
    assert!(tiles.iter().all(|&b| b == 0xAA), "{tiles:?}");
}

#[test]
fn packing_tiles_rejects_a_raster_of_the_wrong_length() {
    assert!(matches!(
        tiles_from_image(&[0u8; 10], 4, 8, 8, None),
        Err(EntryShapeError::ImageSizeMismatch {
            width: 8,
            height: 8,
            len: 10
        })
    ));
    assert!(matches!(
        tiles_from_image(&[0u8; 64], 3, 8, 8, None),
        Err(EntryShapeError::UnsupportedBitDepth(3))
    ));
    assert!(matches!(
        tiles_from_image(&[0u8; 20], 4, 5, 4, None),
        Err(EntryShapeError::NotTileAligned {
            width: 5,
            height: 4
        })
    ));
    assert!(matches!(
        tiles_from_image(&[0u8; 64], 4, 8, 8, Some((3, 1))),
        Err(EntryShapeError::MetatileMisaligned { .. })
    ));
}
