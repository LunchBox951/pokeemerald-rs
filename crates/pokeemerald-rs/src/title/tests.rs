//! Unit tests for [`super::TitleScene`] and its private decoding/animation
//! helpers.
//!
//! Every test builds small **synthetic** fixtures rather than touching the
//! real extracted pack, per `assets::pack::tests`' CI caveat (CI has no
//! `pokeemerald/` checkout and no real pack). The one exception,
//! [`real_pack_composes_non_blank_deterministic_title_frames`], is
//! `#[ignore]`d.

use super::{
    affine_tilemap_from_raw, cloud_scroll_y, crop_and_pack_tile_bytes, image_to_tileset,
    press_start_visible, regular_tilemap_from_raw, sprite_entries, title_palette_from_refs,
    TitleSceneError, LOGO_PALETTE_COLORS, NUM_COPYRIGHT_FRAMES, NUM_PRESS_START_FRAMES,
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

/// Write a minimal synthetic pack (mirroring `assets::pack::tests`'
/// fixture style) holding the supplied `Palette` entries, so
/// [`title_palette_from_refs`] can be exercised against real
/// [`assets::PaletteRef`] values -- their `raw` field is crate-private to
/// `assets::pack`.
fn write_synthetic_palette_pack(entries: &[(&str, &[u8])]) -> std::path::PathBuf {
    let header_size = 8 + 4 + 4;
    let directory_size: usize = entries
        .iter()
        .map(|(id, _)| 2 + id.len() + 1 + 8 + 8 + 2)
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
        out.push(1); // EntryKind tag 1 = Palette
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
    let logo = [0x1F, 0x00].repeat(256); // red
    let rayquaza_clouds = [0xE0, 0x03].repeat(16); // green
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
    let logo: [u8; 2] = [0x1F, 0x00]; // one color: red
    let rayquaza_clouds: [u8; 2] = [0xE0, 0x03]; // one color: green
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
fn stale_pack_reports_the_re_extract_remedy() {
    // A pack that loads but predates this build (missing one of the
    // `title/*` entries this module requires) must render an actionable
    // "re-extract" message, not just the bare "no entry with id" symptom
    // -- and be distinguishable via `is_pack_stale`.
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
///
/// Checks frames 0 and 20 specifically (I-2, issue #116): both must be
/// non-blank and each deterministic (composing the same frame index twice
/// is pixel-identical), and the two must *differ* from each other. Frame 20
/// differs from frame 0 on two independent axes: the "Press Start" banner
/// has toggled from invisible to visible (`press_start_visible(0)` is
/// `false`, `press_start_visible(20)` is `true`) and the clouds have
/// scrolled (`cloud_scroll_y(0) == 0`, `cloud_scroll_y(20) == 5`). The logo
/// shine is deliberately not composed (see the module docs' "Documented
/// fidelity deltas"), so it plays no part in this distinction.
///
/// Loads [`AssetPack::load_repo`] directly rather than
/// [`super::load_default`] (issue #412) -- see [`AssetPack::load_repo`]'s
/// own docs for why a checkout-validation gate must not go through
/// [`AssetPack::default_path`].
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

// -- OBJ sprite animation cadences (I-2, issue #116) -- all pure functions
// of `frame`, so none of these need a pack at all. --------------------------

#[test]
fn press_start_blinks_every_16_ticks() {
    // `sTimer` starts at 0 and is incremented *before* the bit-16 test
    // (`SpriteCB_PressStartCopyrightBanner`), so frame 0 -> timer 1 (bit 4
    // clear -> invisible), frames 15 and 16 straddle the flip to visible.
    assert!(!press_start_visible(0));
    assert!(!press_start_visible(14));
    assert!(press_start_visible(15));
    assert!(press_start_visible(30));
    assert!(!press_start_visible(31));
    assert!(!press_start_visible(46));
    assert!(press_start_visible(47));
}

#[test]
#[allow(clippy::cast_possible_truncation)] // `tb_g1_y` stays tiny over 20 ticks.
fn cloud_scroll_advances_roughly_one_pixel_every_4_ticks() {
    // Hand-simulated from `Task_TitleScreenPhase3`'s own per-tick update
    // (module docs): `tCounter` increments every tick, `tBg1Y` only on odd
    // `tCounter` values, and the applied scroll is `tBg1Y / 2`.
    let mut counter: u32 = 0;
    let mut tb_g1_y: u32 = 0;
    let mut expected = Vec::new();
    for _ in 0..20 {
        counter += 1;
        if counter & 1 != 0 {
            tb_g1_y += 1;
        }
        expected.push((tb_g1_y / 2) as u16);
    }
    let actual: Vec<u16> = (0..20).map(cloud_scroll_y).collect();
    assert_eq!(actual, expected);
    // Confirms genuine (if slow) forward motion, not a stuck value.
    assert!(actual[19] > actual[0]);
}

#[test]
fn cloud_scroll_is_a_pure_function_of_frame() {
    assert_eq!(cloud_scroll_y(37), cloud_scroll_y(37));
}

#[test]
fn cloud_scroll_wraps_like_upstreams_signed_16_bit_accumulator() {
    // Upstream's `tBg1Y` is a *signed* 16-bit task field: after 32,767
    // increments it wraps to -32768, and `tBg1Y / 2` is then a signed
    // truncate-toward-zero division. At frame 65,534 the accumulator's
    // raw bits are 0x8000 (-32768), so the scroll must be -16384 as raw
    // bits (0xC000) -- not the 0x4000 a plain u32 divide would give.
    assert_eq!(cloud_scroll_y(65_532), 0x3FFF); // tb_g1_y = 32767 (last positive)
    assert_eq!(cloud_scroll_y(65_534), 0xC000); // tb_g1_y = -32768
                                                // -32767 / 2 truncates toward zero: -16383 = 0xC001.
    assert_eq!(cloud_scroll_y(65_536), 0xC001); // tb_g1_y = -32767
                                                // A full 16-bit lap later the sequence repeats exactly.
    assert_eq!(cloud_scroll_y(131_070), cloud_scroll_y(0));
}

#[test]
fn sprite_entries_always_includes_the_settled_version_banner() {
    // The version banner is permanently visible/settled in this module's
    // modeled idle state (module docs' "Documented fidelity deltas") --
    // true at frame 0 and arbitrarily far into the future alike.
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
    // pokeemerald's CreateSprite coordinates are centers. Its sprite runtime
    // subtracts half the sprite dimensions before writing OAM; our renderer
    // consumes those final top-left OAM coordinates directly.
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
        // Exactly 5 "Press Start" + 5 copyright, and never any more: the
        // logo shine is not composed (module docs' "Documented fidelity
        // deltas"), so this stays 10 at every frame.
        assert_eq!(
            four_bpp_count,
            NUM_PRESS_START_FRAMES + NUM_COPYRIGHT_FRAMES,
            "frame {frame}"
        );
    }
}

#[test]
fn sprite_entries_never_includes_the_logo_shine() {
    // The shine is an OBJ-window lighten belonging to an un-modeled boot
    // phase (module docs' "Documented fidelity deltas"): it must never be
    // composed as an opaque OBJ, at any frame -- including the ticks where
    // upstream's sweep would have been on-screen (its old start, mid, and
    // post-despawn frames) and arbitrarily far into the future.
    let expected = 2 + NUM_PRESS_START_FRAMES + NUM_COPYRIGHT_FRAMES;
    for frame in [0, 1, 36, 67, 1000] {
        assert_eq!(sprite_entries(frame).len(), expected, "frame {frame}");
    }
}

#[test]
fn sprite_entries_press_start_visibility_tracks_the_blink_cadence_but_copyright_never_blinks() {
    let entries_invisible_tick = sprite_entries(0); // press_start_visible(0) == false
    let press_start: Vec<_> = entries_invisible_tick[2..2 + NUM_PRESS_START_FRAMES].to_vec();
    let copyright: Vec<_> = entries_invisible_tick
        [2 + NUM_PRESS_START_FRAMES..2 + NUM_PRESS_START_FRAMES + NUM_COPYRIGHT_FRAMES]
        .to_vec();
    assert!(press_start.iter().all(|e| !e.enabled()));
    assert!(copyright.iter().all(|e| e.enabled()));

    let entries_visible_tick = sprite_entries(15); // press_start_visible(15) == true
    let press_start: Vec<_> = entries_visible_tick[2..2 + NUM_PRESS_START_FRAMES].to_vec();
    assert!(press_start.iter().all(|e| e.enabled()));
}

#[test]
fn crop_and_pack_tile_bytes_crops_the_requested_sub_rectangle() {
    // A 16x8 (2x1 tile) source image: left tile all index 1, right tile all
    // index 2 (same fixture style as `image_to_tileset`'s own tests above).
    let pixels = tiled_image(16, 8, |col, _row| if col == 0 { 1 } else { 2 });
    let image = ImageRef {
        width: 16,
        height: 8,
        bit_depth: 4,
        pixels: &pixels,
    };
    // Crop just the right 8x8 tile.
    let packed = crop_and_pack_tile_bytes("test", image, 8, 0, 8, 8, BitDepth::Bpp4).unwrap();
    let tileset = rendering::Tileset::decode(BitDepth::Bpp4, &packed).unwrap();
    assert_eq!(tileset.len(), 1);
    assert_eq!(tileset.tile(0).unwrap().index(0, 0), 2);
}

#[test]
fn crop_and_pack_tile_bytes_rejects_a_short_payload_instead_of_panicking() {
    // A sheet that *declares* 16x8 (2x1 tiles) but whose payload is one
    // pixel short: cropping the second tile would slice past the end of
    // `pixels`. The guard must turn that into a typed error, not a panic
    // (mirrors `image_to_tileset`'s own BG-path guard).
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
