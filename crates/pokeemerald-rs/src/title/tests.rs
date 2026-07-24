//! Unit tests for [`super::TitleScene`] and its private decoding helpers.
//!
//! Every test builds small **synthetic** fixtures rather than touching the
//! real extracted pack, per `assets::pack::tests`' CI caveat (CI has no
//! `pokeemerald/` checkout and no real pack). The one exception,
//! [`real_pack_composes_a_non_blank_deterministic_title_frame`], is
//! `#[ignore]`d.

use super::{
    affine_tilemap_from_raw, image_to_tileset, palette_from_ref, regular_tilemap_from_raw,
    TitleSceneError,
};
use assets::{AssetPack, ImageRef};
use rendering::{BitDepth, RenderError};

/// Build a row-major `width x height` pixel bitmap where pixel `(x, y)`
/// gets whichever tile-index value `tile_value(x / 8, y / 8)` returns --
/// lets a test describe a fixture in terms of "which 8x8 tile", not raw
/// byte offsets, and get row-major packing right regardless of the tile
/// grid's shape.
fn tiled_image(width: usize, height: usize, tile_value: impl Fn(usize, usize) -> u8) -> Vec<u8> {
    let mut pixels = vec![0u8; width * height];
    for y in 0..height {
        for x in 0..width {
            pixels[y * width + x] = tile_value(x / 8, y / 8);
        }
    }
    pixels
}

#[test]
fn image_to_tileset_packs_tiles_side_by_side_row_major() {
    // 16x8 (2x1 tiles): left tile (col 0) all index 1, right tile
    // (col 1) all index 2.
    let pixels = tiled_image(16, 8, |col, _row| if col == 0 { 1 } else { 2 });
    let image = ImageRef {
        width: 16,
        height: 8,
        bit_depth: 4,
        pixels: &pixels,
    };
    let tileset = image_to_tileset("test", image, BitDepth::Bpp4).unwrap();
    assert_eq!(tileset.len(), 2);
    assert_eq!(tileset.tile(0).unwrap().index(0, 0), 1);
    assert_eq!(tileset.tile(1).unwrap().index(0, 0), 2);
}

#[test]
fn image_to_tileset_packs_tiles_top_to_bottom_row_major() {
    // 8x16 (1x2 tiles): top tile (row 0) all index 3, bottom tile
    // (row 1) all index 4 -- proves tile rows are packed in the right
    // order, not just tile columns.
    let pixels = tiled_image(8, 16, |_col, row| if row == 0 { 3 } else { 4 });
    let image = ImageRef {
        width: 8,
        height: 16,
        bit_depth: 4,
        pixels: &pixels,
    };
    let tileset = image_to_tileset("test", image, BitDepth::Bpp4).unwrap();
    assert_eq!(tileset.len(), 2);
    assert_eq!(tileset.tile(0).unwrap().index(0, 0), 3);
    assert_eq!(tileset.tile(1).unwrap().index(0, 0), 4);
}

#[test]
fn image_to_tileset_4bpp_reads_low_nibble_as_left_pixel() {
    // A single 8x8 tile whose pixel (0,0)=1, (1,0)=2, matching
    // `rendering::tile`'s documented 4bpp nibble order.
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

/// Write a minimal, single-entry synthetic pack (mirroring
/// `assets::pack::tests`' fixture style) holding one `Palette` entry at
/// `title/palette/test`, so [`palette_from_ref`] can be exercised
/// against a real [`assets::PaletteRef`] -- its `raw` field is
/// crate-private to `assets::pack`, so this is the only way to build
/// one from outside that crate.
fn write_synthetic_palette_pack(colors: &[u8]) -> std::path::PathBuf {
    let entry_id = "title/palette/test";
    let color_count = u16::try_from(colors.len() / 2).unwrap();

    let header_size = 8 + 4 + 4;
    let directory_size = 2 + entry_id.len() + 1 + 8 + 8 + 2; // + color_count meta (2 bytes)
    let payload_offset = header_size + directory_size;

    let mut out = Vec::new();
    out.extend_from_slice(&assets::pack::MAGIC);
    out.extend_from_slice(&assets::pack::FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes()); // entry_count
    out.extend_from_slice(&u16::try_from(entry_id.len()).unwrap().to_le_bytes());
    out.extend_from_slice(entry_id.as_bytes());
    out.push(1); // EntryKind tag 1 = Palette
    out.extend_from_slice(&(payload_offset as u64).to_le_bytes());
    out.extend_from_slice(&(colors.len() as u64).to_le_bytes());
    out.extend_from_slice(&color_count.to_le_bytes());
    out.extend_from_slice(colors);

    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-title-test-palette-{}-{}.pack",
        std::process::id(),
        colors.len()
    ));
    std::fs::write(&path, &out).unwrap();
    path
}

#[test]
fn palette_from_ref_caps_to_usable_colors() {
    // 4 BGR555 colors: red, green, blue, white.
    let raw: [u8; 8] = [
        0x1F, 0x00, // red   (r=0x1F, g=0, b=0)
        0xE0, 0x03, // green (r=0, g=0x1F, b=0)
        0x00, 0x7C, // blue  (r=0, g=0, b=0x1F)
        0xFF, 0x7F, // white
    ];
    let path = write_synthetic_palette_pack(&raw);
    let pack = AssetPack::load(&path).unwrap();
    let palette_ref = pack.palette("title/palette/test").unwrap();

    let palette = palette_from_ref(palette_ref, 2);
    assert_eq!(palette.color(0).to_rgb888().r, 255);
    assert_eq!(palette.color(1).to_rgb888().g, 255);
    // Capped out: index 2 (blue) was never loaded, stays default/black.
    assert_eq!(palette.color(2), rendering::Bgr555::default());

    let _ = std::fs::remove_file(path);
}

#[test]
fn palette_from_ref_never_reads_past_the_entrys_own_color_count() {
    let raw: [u8; 2] = [0x1F, 0x00]; // one color: red
    let path = write_synthetic_palette_pack(&raw);
    let pack = AssetPack::load(&path).unwrap();
    let palette_ref = pack.palette("title/palette/test").unwrap();

    // Cap requested (240) far exceeds this entry's actual 1 color.
    let palette = palette_from_ref(palette_ref, 240);
    assert_eq!(palette.color(0).to_rgb888().r, 255);
    assert_eq!(palette.color(1), rendering::Bgr555::default());

    let _ = std::fs::remove_file(path);
}

#[test]
fn regular_tilemap_from_raw_decodes_screen_entries() {
    // tile=5, h-flip set, v-flip clear, palette bank=3.
    let raw_entry: u16 = 5 | 0x0400 | (3 << 12);
    let mut bytes = raw_entry.to_le_bytes().to_vec();
    bytes.extend(std::iter::repeat_n(0u8, 2 * (32 * 32 - 1)));

    let tilemap = regular_tilemap_from_raw(&bytes).unwrap();
    let entry = tilemap.entry(0, 0).unwrap();
    assert_eq!(entry.tile_index(), 5);
    assert!(entry.h_flip());
    assert!(!entry.v_flip());
    assert_eq!(entry.palette_bank(), 3);
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
fn title_scene_error_display_messages_are_informative() {
    let err = TitleSceneError::ImageNotTileAligned {
        id: "title/image/x",
        width: 12,
        height: 8,
    };
    let rendered = err.to_string();
    assert!(rendered.contains("title/image/x"));
    assert!(rendered.contains("12"));
}

#[test]
fn is_pack_missing_is_true_only_for_not_found() {
    let missing = TitleSceneError::from(assets::PackError::NotFound("/x".into()));
    assert!(missing.is_pack_missing());
    let other = TitleSceneError::from(assets::PackError::BadMagic);
    assert!(!other.is_pack_missing());
}

#[test]
fn load_default_reports_pack_missing_when_no_pack_is_extracted() {
    // This crate's tests run with `cargo test`'s cwd set to
    // `crates/pokeemerald-rs`, which never has `assets-pack/` -- unlike
    // `assets`'/`xtask`'s own tests, there is no reachable-by-accident
    // real pack path to guard against here (nothing in this crate
    // writes one). If this ever legitimately has a local pack, this
    // assertion would need the same environment-dependent handling as
    // `xtask`'s `extract_dispatch_fails_closed_without_local_checkout`.
    if AssetPack::default_path().is_file() {
        return;
    }
    let err = super::load_default().unwrap_err();
    assert!(err.is_pack_missing());
    assert!(err.to_string().contains("init.sh"));
}

/// Loads the *real* local pack (`cargo xtask extract`'s output) and
/// exercises the full `TitleScene::from_pack` + `compose` pipeline
/// against it -- proof this module's decoding agrees with the real
/// extracted title-screen assets, not just synthetic fixtures. Needs a
/// local pack: run `cargo xtask extract` first, then `cargo test -p
/// pokeemerald-rs -- --ignored`.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_composes_a_non_blank_deterministic_title_frame() {
    let scene = super::load_default().expect("run `cargo xtask extract` first");
    let first = scene.compose();
    let second = scene.compose();
    assert_eq!(
        first.pixels(),
        second.pixels(),
        "composing must be deterministic"
    );
    assert!(
        first
            .pixels()
            .iter()
            .any(|&p| p != rendering::Rgb888::BLACK),
        "the real title screen must produce a non-blank frame"
    );
}
