//! Unit tests for [`super::OverworldScene`] and its private helpers.
//!
//! [`overworld_scene_from_pack_composes_a_non_blank_deterministic_frame`]
//! builds a small **synthetic** pack in memory (mirroring
//! `assets::pack::tests`' fixture style -- CI has no `pokeemerald/`
//! checkout and no real pack) and exercises the full
//! `OverworldScene::from_pack` + `compose` pipeline against it. The one
//! exception, [`real_pack_composes_non_blank_deterministic_overworld_frames`],
//! is `#[ignore]`d and needs a real local pack.

use super::{layout_pack_name, pack_4bpp_region, resolve_tileset_pack_name, OverworldSceneError};
use assets::{AssetPack, ImageRef, LayoutId, MapLayout};
use engine::overworld::{Direction, PlayerState};

// -- `resolve_tileset_pack_name` / `layout_pack_name` -----------------------

#[test]
fn resolve_tileset_pack_name_covers_every_bundled_tileset() {
    assert_eq!(resolve_tileset_pack_name("gTileset_General"), Ok("general"));
    assert_eq!(
        resolve_tileset_pack_name("gTileset_Building"),
        Ok("building")
    );
    assert_eq!(
        resolve_tileset_pack_name("gTileset_Petalburg"),
        Ok("petalburg")
    );
    assert_eq!(
        resolve_tileset_pack_name("gTileset_BrendansMaysHouse"),
        Ok("brendans_mays_house")
    );
    assert_eq!(resolve_tileset_pack_name("gTileset_Lab"), Ok("lab"));
}

#[test]
fn resolve_tileset_pack_name_rejects_an_unbundled_symbol() {
    assert_eq!(
        resolve_tileset_pack_name("gTileset_Underwater"),
        Err(OverworldSceneError::UnknownTileset("gTileset_Underwater"))
    );
}

#[test]
fn layout_pack_name_strips_the_prefix_and_lowercases() {
    assert_eq!(
        layout_pack_name(LayoutId("LAYOUT_LITTLEROOT_TOWN")),
        "littleroot_town"
    );
    assert_eq!(
        layout_pack_name(LayoutId(
            "LAYOUT_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB_WITH_TABLE"
        )),
        "littleroot_town_professor_birchs_lab_with_table"
    );
}

// -- `pack_4bpp_region` ------------------------------------------------------

#[test]
fn pack_4bpp_region_rejects_a_payload_shorter_than_declared_dimensions() {
    let pixels = vec![0u8; 63];
    let image = ImageRef {
        width: 8,
        height: 8,
        bit_depth: 8,
        pixels: &pixels,
    };
    let err = pack_4bpp_region("bogus", image, 0, 0, 8, 8).unwrap_err();
    assert_eq!(
        err,
        OverworldSceneError::ImagePixelCountMismatch {
            label: "bogus",
            width: 8,
            height: 8,
            actual: 63,
        }
    );
}

#[test]
fn pack_4bpp_region_rejects_a_region_not_a_multiple_of_8() {
    let pixels = vec![0u8; 8 * 8];
    let image = ImageRef {
        width: 8,
        height: 8,
        bit_depth: 8,
        pixels: &pixels,
    };
    let err = pack_4bpp_region("bogus", image, 0, 0, 5, 8).unwrap_err();
    assert_eq!(
        err,
        OverworldSceneError::ImageNotTileAligned {
            label: "bogus",
            width: 8,
            height: 8,
        }
    );
}

#[test]
fn pack_4bpp_region_crops_the_requested_sub_rectangle() {
    // 16x8 (2x1 tiles): left tile all pixel value 3, right tile all value 4.
    let mut pixels = vec![3u8; 16 * 8];
    for y in 0..8 {
        pixels[y * 16 + 8..y * 16 + 16].fill(4);
    }
    let image = ImageRef {
        width: 16,
        height: 8,
        bit_depth: 8,
        pixels: &pixels,
    };
    let packed = pack_4bpp_region("test", image, 8, 0, 8, 8).unwrap();
    let tileset = rendering::Tileset::decode(rendering::BitDepth::Bpp4, &packed).unwrap();
    assert_eq!(tileset.len(), 1);
    assert_eq!(tileset.tile(0).unwrap().index(0, 0), 4);
}

// -- `OverworldSceneError` ---------------------------------------------------

#[test]
fn is_pack_missing_is_true_only_for_not_found() {
    let missing = OverworldSceneError::from(assets::PackError::NotFound("/x".into()));
    assert!(missing.is_pack_missing());
    let other = OverworldSceneError::from(assets::PackError::BadMagic);
    assert!(!other.is_pack_missing());
}

#[test]
fn load_default_room_reports_pack_missing_when_no_pack_is_extracted() {
    // Mirrors `crate::title::tests::load_default_reports_pack_missing_when_no_pack_is_extracted`'s
    // reasoning: this crate's tests run with `cargo test`'s cwd set to
    // `crates/pokeemerald-rs`, which never has `assets-pack/`.
    if AssetPack::default_path().is_file() {
        return;
    }
    let err = super::load_default_room().unwrap_err();
    assert!(err.is_pack_missing());
}

// -- End-to-end against a synthetic pack -------------------------------------

/// One directory entry for [`write_synthetic_pack`], mirroring
/// `assets::pack::tests`' own fixture-building style (that module's helper
/// is private to `assets`, so this is a small independent copy rather than
/// a shared one).
struct Entry {
    id: &'static str,
    kind_tag: u8,
    meta: Vec<u8>,
    payload: Vec<u8>,
}

/// Serialize `entries` (sorted by id, as the real format requires -- see
/// `assets::pack`'s module docs) into a version-1 pack file's bytes.
fn write_synthetic_pack(mut entries: Vec<Entry>) -> Vec<u8> {
    entries.sort_by(|a, b| a.id.cmp(b.id));

    let header_size = 8 + 4 + 4;
    let mut directory_size = 0usize;
    for e in &entries {
        directory_size += 2 + e.id.len() + 1 + 8 + 8 + e.meta.len();
    }
    let mut offset = header_size + directory_size;
    let mut offsets = Vec::new();
    for e in &entries {
        offsets.push(offset);
        offset += e.payload.len();
    }

    let mut out = Vec::new();
    out.extend_from_slice(&assets::pack::MAGIC);
    out.extend_from_slice(&assets::pack::FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&u32::try_from(entries.len()).unwrap().to_le_bytes());
    for (e, &off) in entries.iter().zip(&offsets) {
        out.extend_from_slice(&u16::try_from(e.id.len()).unwrap().to_le_bytes());
        out.extend_from_slice(e.id.as_bytes());
        out.push(e.kind_tag);
        out.extend_from_slice(&u64::try_from(off).unwrap().to_le_bytes());
        out.extend_from_slice(&u64::try_from(e.payload.len()).unwrap().to_le_bytes());
        out.extend_from_slice(&e.meta);
    }
    for e in &entries {
        out.extend_from_slice(&e.payload);
    }
    out
}

fn image_meta(width: u32, height: u32, bit_depth: u8) -> Vec<u8> {
    let mut m = Vec::new();
    m.extend_from_slice(&width.to_le_bytes());
    m.extend_from_slice(&height.to_le_bytes());
    m.push(bit_depth);
    m
}

/// A minimal pack covering exactly what [`super::OverworldScene::from_pack`]
/// needs for one synthetic room: a single-tile "general" tileset (used as
/// both primary and secondary), a 4x4 layout whose every cell -- and whose
/// border block -- is that tileset's one opaque metatile, and Brendan's
/// walking sheet/palette.
fn synthetic_overworld_pack_bytes() -> Vec<u8> {
    // A single opaque 8x8 tile, every pixel palette index 5.
    let tile_pixels = vec![5u8; 8 * 8];

    // metatiles.bin: one metatile (id 0), every one of its 8 raw entries
    // pointing at combined tile index 0, palette bank 0, no flip -- raw
    // `u16` value `0x0000` for all 8.
    let metatiles: Vec<u8> = std::iter::repeat_n(0u16.to_le_bytes(), 8)
        .flatten()
        .collect();
    // metatile_attributes.bin: metatile 0 is `METATILE_LAYER_TYPE_COVERED`
    // (1) -- bottom/middle draw the opaque tile, top stays transparent, so
    // the player OBJ (drawn between the middle and top layers) is never
    // hidden by this fixture's otherwise-uniform world content.
    let metatile_attrs = (1u16 << 12).to_le_bytes().to_vec();

    // A 4x4 grid, every cell metatile 0, elevation 3
    // (`MetatileCell{metatile_id:0, collision:0, elevation:3}.pack()`).
    let grid_cell = assets::MetatileCell {
        metatile_id: 0,
        collision: 0,
        elevation: 3,
    }
    .pack();
    let grid: Vec<u8> = std::iter::repeat_n(grid_cell.to_le_bytes(), 16)
        .flatten()
        .collect();
    let border: Vec<u8> = std::iter::repeat_n(grid_cell.to_le_bytes(), 4)
        .flatten()
        .collect();

    // Palette bank 0: color index 5 (matching `tile_pixels`) is bright red
    // (raw BGR555 `0x001F`); every other bank is empty.
    let bank0_payload = {
        let mut p = vec![0u8; 5 * 2]; // indices 0..4, unused, zeroed.
        p.extend_from_slice(&0x001Fu16.to_le_bytes()); // index 5: red.
        p
    };

    let mut entries = vec![
        Entry {
            id: "tileset/general/tiles",
            kind_tag: 0,
            meta: image_meta(8, 8, 4),
            payload: tile_pixels,
        },
        Entry {
            id: "tileset/general/metatiles",
            kind_tag: 2,
            meta: vec![],
            payload: metatiles,
        },
        Entry {
            id: "tileset/general/metatile-attributes",
            kind_tag: 2,
            meta: vec![],
            payload: metatile_attrs,
        },
        Entry {
            id: "layout/map_test/map",
            kind_tag: 2,
            meta: vec![],
            payload: grid,
        },
        Entry {
            id: "layout/map_test/border",
            kind_tag: 2,
            meta: vec![],
            payload: border,
        },
        Entry {
            id: "sprite/brendan/walking",
            kind_tag: 0,
            meta: image_meta(144, 32, 8),
            // Every pixel palette index 9 (opaque, distinct from the world
            // tileset's index 5).
            payload: vec![9u8; 144 * 32],
        },
        Entry {
            id: "sprite/palette/brendan",
            kind_tag: 1,
            meta: 16u16.to_le_bytes().to_vec(),
            payload: {
                let mut p = vec![0u8; 9 * 2]; // indices 0..9, unused, zeroed.
                p.extend_from_slice(&0x7C00u16.to_le_bytes()); // index 9: blue.
                p.resize(32, 0);
                p
            },
        },
    ];
    for slot in 0..16u8 {
        let id: &'static str =
            &*Box::leak(format!("tileset/general/palette/{slot:02}").into_boxed_str());
        entries.push(Entry {
            id,
            kind_tag: 1,
            meta: 0u16.to_le_bytes().to_vec(), // color_count 0, filled in below for bank 0.
            payload: vec![],
        });
    }
    // Overwrite the bank-0 placeholder with the real payload above.
    if let Some(bank0) = entries
        .iter_mut()
        .find(|e| e.id == "tileset/general/palette/00")
    {
        bank0.meta = 6u16.to_le_bytes().to_vec();
        bank0.payload = bank0_payload;
    }

    write_synthetic_pack(entries)
}

fn write_synthetic_overworld_pack() -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-overworld-test-{}.pack",
        std::process::id()
    ));
    std::fs::write(&path, synthetic_overworld_pack_bytes()).unwrap();
    path
}

#[test]
fn overworld_scene_from_pack_composes_a_non_blank_deterministic_frame() {
    let path = write_synthetic_overworld_pack();
    let pack = AssetPack::load(&path).unwrap();
    let layout = MapLayout {
        id: LayoutId("LAYOUT_MAP_TEST"),
        name: "MapTest",
        width: 4,
        height: 4,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_General",
    };
    let scene = super::OverworldScene::from_pack(&pack, &layout, super::PlayerCharacter::Brendan)
        .expect("synthetic pack should decode cleanly");

    let player = PlayerState::new((0, 0), 3, Direction::South);
    let first = scene.compose(&player);
    let second = scene.compose(&player);
    assert_eq!(
        first.pixels(),
        second.pixels(),
        "composing the same player state twice must be deterministic"
    );
    assert!(
        first
            .pixels()
            .iter()
            .any(|&p| p != rendering::Rgb888::BLACK),
        "the composed viewport must be non-blank"
    );

    // The world's single opaque metatile (red, palette index 5) covers the
    // whole visible screen -- both the grid interior and the border
    // fallback resolve to it in this fixture.
    assert_eq!(
        first.pixel(0, 0),
        Some(rendering::Bgr555::from_raw(0x001F).to_rgb888())
    );

    // The player OBJ (blue, palette index 9) must be visible at its fixed
    // screen position, drawn over the world BG layers.
    assert_eq!(
        first.pixel(
            usize::from(super::avatar::PLAYER_OBJ_X),
            usize::from(super::avatar::PLAYER_OBJ_Y)
        ),
        Some(rendering::Bgr555::from_raw(0x7C00).to_rgb888())
    );

    let _ = std::fs::remove_file(path);
}

/// Loads the *real* local pack (`cargo xtask extract`'s output) and
/// exercises the full `load_default_room` + `compose` pipeline against
/// the protagonist's bedroom (Brendan's house 2F) -- proof this module's
/// decoding agrees with the real extracted overworld assets, not just the
/// synthetic fixture above. Needs a local pack: run `cargo xtask extract`
/// first, then `cargo test -p pokeemerald-rs -- --ignored`.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_composes_non_blank_deterministic_overworld_frames() {
    let scene = super::load_default_room().expect("run `cargo xtask extract` first");

    // A couple of on-foot player states across the bedroom's 9x8 interior
    // (`LAYOUT_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F`), including one mid-step,
    // so the scroll-lag path is exercised too.
    let standing = PlayerState::new((5, 5), 3, Direction::South);
    let frame_a = scene.compose(&standing);
    let frame_b = scene.compose(&standing);
    assert_eq!(
        frame_a.pixels(),
        frame_b.pixels(),
        "composing the same player state twice must be deterministic"
    );
    assert!(
        frame_a
            .pixels()
            .iter()
            .any(|&p| p != rendering::Rgb888::BLACK),
        "a standing player's frame must be non-blank"
    );

    let facing_north = PlayerState::new((5, 4), 3, Direction::North);
    let frame_c = scene.compose(&facing_north);
    assert!(
        frame_c
            .pixels()
            .iter()
            .any(|&p| p != rendering::Rgb888::BLACK),
        "a different facing/position must also compose non-blank"
    );
    assert_ne!(
        frame_a.pixels(),
        frame_c.pixels(),
        "a different player position/facing must change the composed frame"
    );
}
