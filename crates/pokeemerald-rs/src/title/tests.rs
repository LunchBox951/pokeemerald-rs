use std::mem::size_of;

use super::{
    affine_tilemap_from_raw, cloud_scroll_y, crop_and_pack_tile_bytes, image_to_tileset,
    press_start_visible, regular_tilemap_from_raw, sprite_entries, title_palette_from_refs,
    TitleSceneError, LOGO_PALETTE_COLORS, NUM_COPYRIGHT_FRAMES, NUM_PRESS_START_FRAMES,
};
use assets::{AssetPack, ImageRef};
use rendering::{BitDepth, RenderError};

const RED_BGR555_LE: [u8; 2] = 0x001F_u16.to_le_bytes();
const GREEN_BGR555_LE: [u8; 2] = 0x03E0_u16.to_le_bytes();
const PALETTE_ENTRY_KIND_TAG: u8 = 1;

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

fn write_synthetic_palette_pack(entries: &[(&str, &[u8])]) -> std::path::PathBuf {
    let header_size = assets::pack::MAGIC.len() + size_of::<u32>() + size_of::<u32>();
    let directory_size: usize = entries
        .iter()
        .map(|(id, _)| {
            size_of::<u16>()
                + id.len()
                + size_of::<u8>()
                + size_of::<u64>()
                + size_of::<u64>()
                + size_of::<u16>()
        })
        .sum();
    let mut payload_offset = header_size + directory_size;

    let mut out = Vec::new();
    out.extend_from_slice(&assets::pack::MAGIC);
    out.extend_from_slice(&assets::pack::FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&u32::try_from(entries.len()).unwrap().to_le_bytes());
    for (entry_id, colors) in entries {
        let color_count = u16::try_from(colors.len() / 2).unwrap();
        out.extend_from_slice(&u16::try_from(entry_id.len()).unwrap().to_le_bytes());
        out.extend_from_slice(entry_id.as_bytes());
        out.push(PALETTE_ENTRY_KIND_TAG);
        out.extend_from_slice(&(payload_offset as u64).to_le_bytes());
        out.extend_from_slice(&(colors.len() as u64).to_le_bytes());
        out.extend_from_slice(&color_count.to_le_bytes());
        payload_offset += colors.len();
    }
    for (_, colors) in entries {
        out.extend_from_slice(colors);
    }

    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-title-test-palette-{}-{}.pack",
        std::process::id(),
        out.len()
    ));
    std::fs::write(&path, &out).unwrap();
    path
}

#[test]
fn title_palette_splices_rayquaza_clouds_after_224_logo_colors() {
    let logo = RED_BGR555_LE.repeat(256);
    let rayquaza_clouds = GREEN_BGR555_LE.repeat(16);
    let path = write_synthetic_palette_pack(&[
        ("title/palette/pokemon_logo", &logo),
        ("title/palette/rayquaza_and_clouds", &rayquaza_clouds),
    ]);
    let pack = AssetPack::load(&path).unwrap();
    let palette = title_palette_from_refs(
        pack.palette("title/palette/pokemon_logo").unwrap(),
        pack.palette("title/palette/rayquaza_and_clouds").unwrap(),
    );
    let split = u8::try_from(LOGO_PALETTE_COLORS).unwrap();
    assert_eq!(palette.color(split - 1).to_rgb888().r, 255);
    assert_eq!(palette.color(split).to_rgb888().g, 255);
    assert_eq!(palette.color(split + 15).to_rgb888().g, 255);
    assert_eq!(palette.color(split + 16), rendering::Bgr555::default());

    let _ = std::fs::remove_file(path);
}

#[test]
fn title_palette_never_reads_past_either_entrys_own_color_count() {
    let logo = RED_BGR555_LE;
    let rayquaza_clouds = GREEN_BGR555_LE;
    let path = write_synthetic_palette_pack(&[
        ("title/palette/pokemon_logo", &logo),
        ("title/palette/rayquaza_and_clouds", &rayquaza_clouds),
    ]);
    let pack = AssetPack::load(&path).unwrap();
    let palette = title_palette_from_refs(
        pack.palette("title/palette/pokemon_logo").unwrap(),
        pack.palette("title/palette/rayquaza_and_clouds").unwrap(),
    );
    assert_eq!(palette.color(0).to_rgb888().r, 255);
    assert_eq!(palette.color(1), rendering::Bgr555::default());
    let split = u8::try_from(LOGO_PALETTE_COLORS).unwrap();
    assert_eq!(palette.color(split).to_rgb888().g, 255);
    assert_eq!(palette.color(split + 1), rendering::Bgr555::default());

    let _ = std::fs::remove_file(path);
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
fn stale_pack_reports_the_re_extract_remedy() {
    let stale = TitleSceneError::from(assets::PackError::UnknownAsset(
        "title/palette/emerald_version".to_owned(),
    ));
    assert!(stale.is_pack_stale());
    assert!(!stale.is_pack_missing());
    let rendered = stale.to_string();
    assert!(rendered.contains("title/palette/emerald_version"));
    assert!(rendered.contains("cargo xtask extract"));

    let other = TitleSceneError::from(assets::PackError::BadMagic);
    assert!(!other.is_pack_stale());
}

#[test]
fn load_default_reports_pack_missing_when_no_pack_is_extracted() {
    if AssetPack::default_path().is_file() {
        return;
    }
    let err = super::load_default().unwrap_err();
    assert!(err.is_pack_missing());
    assert!(err.to_string().contains("init.sh"));
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_composes_non_blank_deterministic_title_frames() {
    let pack = AssetPack::load_repo().expect("run `cargo xtask extract` first");
    let scene = super::TitleScene::from_pack(&pack).expect("run `cargo xtask extract` first");

    let frame0_first = scene.compose(0);
    let frame0_second = scene.compose(0);
    assert_eq!(
        frame0_first.pixels(),
        frame0_second.pixels(),
        "composing frame 0 twice must be deterministic"
    );
    assert!(
        frame0_first
            .pixels()
            .iter()
            .any(|&p| p != rendering::Rgb888::BLACK),
        "frame 0 must be a non-blank frame"
    );

    let moved_first = scene.compose(20);
    let moved_second = scene.compose(20);
    assert_eq!(
        moved_first.pixels(),
        moved_second.pixels(),
        "composing frame 20 twice must be deterministic"
    );
    assert!(
        moved_first
            .pixels()
            .iter()
            .any(|&p| p != rendering::Rgb888::BLACK),
        "frame 20 must be a non-blank frame"
    );

    assert_ne!(
        frame0_first.pixels(),
        moved_first.pixels(),
        "frame 0 and frame 20 must differ (Press Start blink / cloud scroll)"
    );
}

#[test]
fn press_start_blinks_every_16_ticks() {
    assert!(!press_start_visible(0));
    assert!(!press_start_visible(14));
    assert!(press_start_visible(15));
    assert!(press_start_visible(30));
    assert!(!press_start_visible(31));
    assert!(!press_start_visible(46));
    assert!(press_start_visible(47));
}

#[test]
#[expect(
    clippy::cast_possible_truncation,
    reason = "the simulated scroll stays within u16 for 20 frames"
)]
fn cloud_scroll_advances_roughly_one_pixel_every_4_ticks() {
    let mut counter: u32 = 0;
    let mut cloud_accumulator: u32 = 0;
    let mut expected = Vec::new();
    for _ in 0..20 {
        counter += 1;
        if counter & 1 != 0 {
            cloud_accumulator += 1;
        }
        expected.push((cloud_accumulator / 2) as u16);
    }
    let actual: Vec<u16> = (0..20).map(cloud_scroll_y).collect();
    assert_eq!(actual, expected);
    assert!(actual[19] > actual[0]);
}

#[test]
fn cloud_scroll_is_a_pure_function_of_frame() {
    assert_eq!(cloud_scroll_y(37), cloud_scroll_y(37));
}

#[test]
fn cloud_scroll_wraps_like_upstreams_signed_16_bit_accumulator() {
    const LAST_POSITIVE_FRAME: u32 = 65_532;
    const FIRST_NEGATIVE_FRAME: u32 = 65_534;
    const SECOND_NEGATIVE_FRAME: u32 = 65_536;
    const ACCUMULATOR_PERIOD_FRAMES: u32 = 131_070;

    assert_eq!(
        cloud_scroll_y(LAST_POSITIVE_FRAME),
        (i16::MAX / 2).cast_unsigned()
    );
    assert_eq!(
        cloud_scroll_y(FIRST_NEGATIVE_FRAME),
        (i16::MIN / 2).cast_unsigned()
    );
    assert_eq!(
        cloud_scroll_y(SECOND_NEGATIVE_FRAME),
        ((i16::MIN + 1) / 2).cast_unsigned()
    );
    assert_eq!(cloud_scroll_y(ACCUMULATOR_PERIOD_FRAMES), cloud_scroll_y(0));
}

#[test]
fn sprite_entries_always_includes_the_settled_version_banner() {
    for frame in [0, 37, 10_000] {
        let entries = sprite_entries(frame);
        let version_banner_count = entries
            .iter()
            .filter(|e| e.bit_depth() == BitDepth::Bpp8)
            .count();
        assert_eq!(version_banner_count, 2, "frame {frame}");
        assert!(
            entries
                .iter()
                .filter(|e| e.bit_depth() == BitDepth::Bpp8)
                .all(|e| e.enabled()),
            "frame {frame}: both version banner halves must be visible"
        );
    }
}

#[test]
fn sprite_entries_convert_upstream_centers_to_oam_origins() {
    let entries = sprite_entries(0);

    assert_eq!((entries[0].x(), entries[0].y()), (66, 50));
    assert_eq!((entries[1].x(), entries[1].y()), (130, 50));
    assert_eq!(entries[0].dimensions(), (64, 32));
    assert_eq!(entries[1].dimensions(), (64, 32));

    let press_start = &entries[2..2 + NUM_PRESS_START_FRAMES];
    let press_start_origins: Vec<_> = press_start.iter().map(|entry| entry.x()).collect();
    assert_eq!(press_start_origins, [48, 80, 112, 144, 176]);
    assert!(press_start.iter().all(|entry| entry.y() == 104));
    assert!(press_start
        .iter()
        .all(|entry| entry.dimensions() == (32, 8)));

    let copyright =
        &entries[2 + NUM_PRESS_START_FRAMES..2 + NUM_PRESS_START_FRAMES + NUM_COPYRIGHT_FRAMES];
    let copyright_origins: Vec<_> = copyright.iter().map(|entry| entry.x()).collect();
    assert_eq!(copyright_origins, [48, 80, 112, 144, 176]);
    assert!(copyright.iter().all(|entry| entry.y() == 144));
}

#[test]
fn sprite_entries_always_includes_5_press_start_and_5_copyright_segments() {
    for frame in [0, 15, 16, 37] {
        let entries = sprite_entries(frame);
        let four_bpp_count = entries
            .iter()
            .filter(|e| e.bit_depth() == BitDepth::Bpp4)
            .count();
        assert_eq!(
            four_bpp_count,
            NUM_PRESS_START_FRAMES + NUM_COPYRIGHT_FRAMES,
            "frame {frame}"
        );
    }
}

#[test]
fn sprite_entries_never_includes_the_logo_shine() {
    let expected = 2 + NUM_PRESS_START_FRAMES + NUM_COPYRIGHT_FRAMES;
    for frame in [0, 1, 36, 67, 1000] {
        assert_eq!(sprite_entries(frame).len(), expected, "frame {frame}");
    }
}

#[test]
fn sprite_entries_press_start_visibility_tracks_the_blink_cadence_but_copyright_never_blinks() {
    let hidden_frame_entries = sprite_entries(0);
    let press_start: Vec<_> = hidden_frame_entries[2..2 + NUM_PRESS_START_FRAMES].to_vec();
    let copyright: Vec<_> = hidden_frame_entries
        [2 + NUM_PRESS_START_FRAMES..2 + NUM_PRESS_START_FRAMES + NUM_COPYRIGHT_FRAMES]
        .to_vec();
    assert!(press_start.iter().all(|e| !e.enabled()));
    assert!(copyright.iter().all(|e| e.enabled()));

    let visible_frame_entries = sprite_entries(15);
    let press_start: Vec<_> = visible_frame_entries[2..2 + NUM_PRESS_START_FRAMES].to_vec();
    assert!(press_start.iter().all(|e| e.enabled()));
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
