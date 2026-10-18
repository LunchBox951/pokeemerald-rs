//! Pack-name resolution, 4bpp packing, palette-bank filling, and
//! missing-pack reporting over throwaway in-memory packs.

use crate::overworld::{
    fill_palette_bank, layout_pack_name, pack_4bpp_region, resolve_tileset_pack_name,
    OverworldSceneError,
};
use assets::{AssetPack, ImageRef, LayoutId};

use crate::overworld::test_support::{write_synthetic_pack, Entry, PALETTE_KIND_TAG};

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
fn pack_4bpp_region_rejects_a_palette_index_a_4bpp_tile_cannot_hold() {
    let mut pixels = vec![0u8; 8 * 8];
    pixels[0] = 16;
    let image = ImageRef {
        width: 8,
        height: 8,
        bit_depth: 8,
        pixels: &pixels,
    };
    let err = pack_4bpp_region("bogus", image, 0, 0, 8, 8).unwrap_err();
    assert_eq!(
        err,
        OverworldSceneError::ImagePaletteIndexOutOfRange {
            label: "bogus",
            index: 16,
        }
    );
}

struct TempPackGuard(std::path::PathBuf);

impl Drop for TempPackGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Runs `check` with a pack palette of `color_count` colours whose raw values
/// are `0x8000 | (index + 1)` (bit 15 set, to prove it is cleared).
fn with_palette(color_count: u16, check: impl FnOnce(assets::PaletteRef<'_>)) {
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-palette-bank-test-{}-{:?}.pack",
        std::process::id(),
        std::thread::current().id()
    ));
    with_palette_at(&path, color_count, check);
}

fn with_palette_at(
    path: &std::path::Path,
    color_count: u16,
    check: impl FnOnce(assets::PaletteRef<'_>),
) {
    let payload: Vec<u8> = (0..color_count)
        .flat_map(|i| (0x8000 | (i + 1)).to_le_bytes())
        .collect();
    let bytes = write_synthetic_pack(vec![Entry {
        id: "p".into(),
        kind_tag: PALETTE_KIND_TAG,
        meta: color_count.to_le_bytes().to_vec(),
        payload,
    }]);
    let _guard = TempPackGuard(path.to_path_buf());
    std::fs::write(path, bytes).unwrap();
    let pack = AssetPack::load(path).unwrap();
    check(pack.palette("p").unwrap());
}

fn regression_pack_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "pokeemerald-rs-palette-bank-{name}-{}.pack",
        std::process::id()
    ))
}

#[test]
fn with_palette_removes_its_pack_after_normal_completion() {
    let path = regression_pack_path("cleanup-normal");
    with_palette_at(&path, 3, |_| assert!(path.exists()));
    assert!(!path.exists());
}

#[test]
fn with_palette_removes_its_pack_when_the_check_panics() {
    let path = regression_pack_path("cleanup-unwind");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        with_palette_at(&path, 3, |_| panic!("check failed"));
    }));
    assert!(result.is_err());
    assert!(!path.exists());
}

fn sentinel_colors() -> [rendering::Bgr555; rendering::Palette::LEN] {
    [rendering::Bgr555::from_raw(0x7FFF); rendering::Palette::LEN]
}

#[test]
fn fill_palette_bank_copies_a_partial_palette_and_leaves_the_rest() {
    with_palette(3, |palette| {
        let mut colors = sentinel_colors();
        fill_palette_bank(&mut colors, 4, palette);
        for (i, color) in colors.iter().enumerate() {
            let expected = match i {
                64..=66 => rendering::Bgr555::from_raw(0x8000 | (u16::try_from(i).unwrap() - 63)),
                _ => rendering::Bgr555::from_raw(0x7FFF),
            };
            assert_eq!(*color, expected, "slot {i}");
        }
    });
}

#[test]
fn fill_palette_bank_fills_the_last_bank_without_touching_earlier_ones() {
    with_palette(16, |palette| {
        let mut colors = sentinel_colors();
        fill_palette_bank(&mut colors, 15, palette);
        for (i, color) in colors.iter().enumerate() {
            let expected = if i >= 240 {
                rendering::Bgr555::from_raw(0x8000 | (u16::try_from(i).unwrap() - 239))
            } else {
                rendering::Bgr555::from_raw(0x7FFF)
            };
            assert_eq!(*color, expected, "slot {i}");
        }
    });
}

#[test]
fn fill_palette_bank_clips_a_palette_longer_than_one_bank() {
    with_palette(20, |palette| {
        let mut colors = sentinel_colors();
        fill_palette_bank(&mut colors, 2, palette);
        for (i, color) in colors.iter().enumerate() {
            let expected = if (32..48).contains(&i) {
                rendering::Bgr555::from_raw(0x8000 | (u16::try_from(i).unwrap() - 31))
            } else {
                rendering::Bgr555::from_raw(0x7FFF)
            };
            assert_eq!(*color, expected, "slot {i}");
        }
    });
}

#[test]
fn pack_4bpp_region_packs_palette_index_15_unchanged() {
    let pixels = vec![15u8; 8 * 8];
    let image = ImageRef {
        width: 8,
        height: 8,
        bit_depth: 8,
        pixels: &pixels,
    };
    let packed = pack_4bpp_region("test", image, 0, 0, 8, 8).unwrap();
    assert_eq!(
        packed,
        vec![0xFFu8; rendering::BitDepth::Bpp4.tile_byte_len()]
    );
}

#[test]
fn pack_4bpp_region_crops_the_requested_sub_rectangle() {
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

#[test]
fn is_pack_missing_is_true_only_for_not_found() {
    let missing = OverworldSceneError::from(assets::PackError::NotFound("/x".into()));
    assert!(missing.is_pack_missing());
    let other = OverworldSceneError::from(assets::PackError::BadMagic);
    assert!(!other.is_pack_missing());
}

#[test]
fn load_default_room_reports_pack_missing_when_no_pack_is_extracted() {
    if AssetPack::default_path().is_file() {
        return;
    }
    let err =
        crate::overworld::load_default_room(&engine::event_data::EventData::new()).unwrap_err();
    assert!(err.is_pack_missing());
}

#[test]
fn load_repo_default_room_looks_only_at_the_checkout_pack() {
    // Unlike `load_default_room`, this loader must never fall back to
    // `AssetPack::default_path`'s later search rungs: with no checkout pack
    // it reports "pack missing" even when a user pack is installed.
    if pack_format::repo_pack_path().is_file() {
        return;
    }
    let err = crate::overworld::load_repo_default_room(&engine::event_data::EventData::new())
        .unwrap_err();
    assert!(err.is_pack_missing());
}

#[test]
fn load_room_reports_pack_missing_when_no_pack_is_extracted() {
    if AssetPack::default_path().is_file() {
        return;
    }
    let err = crate::overworld::load_room(
        assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F"),
        crate::overworld::PlayerCharacter::Brendan,
        &engine::event_data::EventData::new(),
    )
    .unwrap_err();
    assert!(err.is_pack_missing());
}
