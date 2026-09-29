//! Pins the generic container: loading the synthetic fixture's untyped
//! entry kinds, and the header/directory error paths (bad magic, version
//! skew, truncation, unknown ids, wrong kinds) independent of any one typed
//! accessor.

use super::super::{AssetPack, EntryKind, PackError, FORMAT_VERSION};
use super::shared::{
    image_meta, pack_bytes, synthetic_pack, write_pack, write_synthetic_pack, Entry,
};

#[test]
fn loads_and_reads_every_entry_kind() {
    let path = write_synthetic_pack("read-kinds");
    let pack = AssetPack::load(&path).unwrap();

    let image = pack.image("tileset/test/tiles").unwrap();
    assert_eq!(image.width, 2);
    assert_eq!(image.height, 2);
    assert_eq!(image.bit_depth, 8);
    assert_eq!(image.pixels, &[1, 2, 3, 4]);

    let palette = pack.palette("tileset/test/palette/00").unwrap();
    assert_eq!(palette.color_count, 2);
    assert_eq!(palette.color(0), Some(0x7FFF));
    assert_eq!(palette.color(1), Some(0x0000));
    assert_eq!(palette.color(2), None);
    assert_eq!(palette.colors().collect::<Vec<_>>(), vec![0x7FFF, 0x0000]);

    let raw = pack.raw("tileset/test/metatiles").unwrap();
    assert_eq!(raw, &[9, 9, 9]);

    let _ = std::fs::remove_file(path);
}

#[test]
fn entries_walks_the_whole_directory_in_sorted_order() {
    let path = write_synthetic_pack("entries");
    let pack = AssetPack::load(&path).unwrap();

    let ids: Vec<&str> = pack.entries().map(|e| e.id.as_str()).collect();
    assert!(ids.contains(&"tileset/test/tiles"));
    assert!(ids.contains(&"tileset/test/palette/00"));
    assert!(ids.contains(&"tileset/test/metatiles"));
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(ids, sorted);

    // Every entry's offset/length addresses the same payload the typed
    // accessors hand out, so a caller can compare two packs entry by entry
    // without going through a lookup id.
    let entry = pack
        .entries()
        .find(|e| e.id == "tileset/test/metatiles")
        .unwrap();
    assert_eq!(entry.kind, EntryKind::Raw);
    assert_eq!(
        &pack.bytes()[entry.offset..entry.offset + entry.length],
        pack.raw("tileset/test/metatiles").unwrap()
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn tileset_handle_needs_all_sixteen_palettes() {
    // The synthetic pack only has palette slot 00, not 01..15, so bundling
    // should fail with UnknownAsset for the first missing slot rather than
    // panicking.
    let path = write_synthetic_pack("missing-slots");
    let pack = AssetPack::load(&path).unwrap();
    let err = pack.tileset("test").unwrap_err();
    assert!(matches!(err, PackError::UnknownAsset(id) if id == "tileset/test/palette/01"));
    let _ = std::fs::remove_file(path);
}

#[test]
fn unknown_asset_id_is_reported() {
    let path = write_synthetic_pack("unknown-id");
    let pack = AssetPack::load(&path).unwrap();
    let err = pack.image("does/not/exist").unwrap_err();
    assert!(matches!(err, PackError::UnknownAsset(id) if id == "does/not/exist"));
    let _ = std::fs::remove_file(path);
}

#[test]
fn wrong_kind_is_reported() {
    let path = write_synthetic_pack("wrong-kind");
    let pack = AssetPack::load(&path).unwrap();
    let err = pack.palette("tileset/test/tiles").unwrap_err();
    assert!(matches!(
        err,
        PackError::WrongKind {
            expected: "palette",
            actual: "image",
            ..
        }
    ));
    let _ = std::fs::remove_file(path);
}

#[test]
fn missing_pack_file_gives_the_required_diagnostic() {
    let err = AssetPack::load(std::path::Path::new("/definitely/does/not/exist.pack")).unwrap_err();
    assert!(matches!(err, PackError::NotFound(_)));
    // Pinned whole: the message serves two audiences, and dropping either
    // half strands one of them (Discussion #71 policy A and policy C).
    assert_eq!(
        err.to_string(),
        "asset pack not found at `/definitely/does/not/exist.pack`: players run \
         `pokeemerald-rs --import-rom <path to your Pokemon Emerald (US) ROM>`; developers \
         run `./init.sh` then `cargo xtask extract`"
    );
}

#[test]
fn bad_magic_is_rejected() {
    let path = write_synthetic_pack("bad-magic");
    let mut bytes = synthetic_pack();
    bytes[0] = 0;
    std::fs::write(&path, &bytes).unwrap();
    let err = AssetPack::load(&path).unwrap_err();
    assert_eq!(err, PackError::BadMagic);
    let _ = std::fs::remove_file(path);
}

#[test]
fn unsupported_version_is_rejected() {
    let path = write_synthetic_pack("bad-version");
    let mut bytes = synthetic_pack();
    bytes[8..12].copy_from_slice(&99u32.to_le_bytes());
    std::fs::write(&path, &bytes).unwrap();
    let err = AssetPack::load(&path).unwrap_err();
    assert_eq!(err, PackError::UnsupportedVersion(99));
    // Same two audiences as the missing-pack diagnostic: an incompatible
    // pack (99 postdates this build's FORMAT_VERSION) is rebuilt by
    // whichever route built it in the first place.
    assert_eq!(
        err.to_string(),
        "asset pack: unsupported format version `99`: the pack uses a newer format than \
         this build supports; players rebuild it with `pokeemerald-rs --import-rom <path \
         to your Pokemon Emerald (US) ROM>`, developers with `cargo xtask extract`"
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_pack_newer_than_this_build_is_not_described_as_older() {
    let newer = PackError::UnsupportedVersion(FORMAT_VERSION + 1).to_string();
    assert!(
        newer.contains(&(FORMAT_VERSION + 1).to_string()),
        "the diagnostic must name the version found: {newer}"
    );
    assert!(
        !newer.contains("predates"),
        "a pack newer than this build must not be reported as older: {newer}"
    );
    assert_ne!(
        newer,
        PackError::UnsupportedVersion(FORMAT_VERSION - 1).to_string(),
        "an older and a newer pack stand in opposite relations to this build"
    );
}

#[test]
fn a_pack_older_than_this_build_is_described_as_predating_it() {
    let path = write_synthetic_pack("older-version");
    let mut bytes = synthetic_pack();
    let older = FORMAT_VERSION - 1;
    bytes[8..12].copy_from_slice(&older.to_le_bytes());
    std::fs::write(&path, &bytes).unwrap();
    let err = AssetPack::load(&path).unwrap_err();
    assert_eq!(err, PackError::UnsupportedVersion(older));
    assert_eq!(
        err.to_string(),
        format!(
            "asset pack: unsupported format version `{older}`: the pack predates this \
             build's format; players rebuild it with `pokeemerald-rs --import-rom <path \
             to your Pokemon Emerald (US) ROM>`, developers with `cargo xtask extract`"
        )
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn truncated_pack_is_rejected() {
    let path = write_synthetic_pack("truncated");
    let bytes = synthetic_pack();
    std::fs::write(&path, &bytes[..bytes.len() / 2]).unwrap();
    let err = AssetPack::load(&path).unwrap_err();
    assert_eq!(err, PackError::Truncated);
    let _ = std::fs::remove_file(path);
}

/// A payload that contradicts the shape its own metadata declares never
/// reaches an accessor: the format owns that invariant
/// (`pack_format::parse_directory`), so the whole pack is refused at load
/// rather than each typed accessor catching its own entry. `PaletteRef` and
/// `ImageRef` promise those shapes to every consumer, and the ones outside
/// the text-window family (the overworld's and the title screen's palette
/// walks) read them without a second check.
#[test]
fn an_entry_whose_payload_contradicts_its_metadata_is_refused_at_load() {
    let truncated_palette = write_pack(
        "misshapen-palette",
        &pack_bytes(vec![Entry {
            id: "text-window/palette/text_pal2",
            kind_tag: 1,
            meta: 16u16.to_le_bytes().to_vec(),
            payload: vec![0x55, 0x00, 0x66, 0x00, 0x77, 0x00, 0x88, 0x00],
        }]),
    );
    assert!(matches!(
        AssetPack::load(&truncated_palette).unwrap_err(),
        PackError::Truncated
    ));
    let _ = std::fs::remove_file(truncated_palette);

    let truncated_image = write_pack(
        "misshapen-image",
        &pack_bytes(vec![Entry {
            id: "text-window/image/4",
            kind_tag: 0,
            meta: image_meta(24, 24, 4),
            payload: vec![0, 1, 2],
        }]),
    );
    assert!(matches!(
        AssetPack::load(&truncated_image).unwrap_err(),
        PackError::Truncated
    ));
    let _ = std::fs::remove_file(truncated_image);
}

#[test]
fn default_path_ends_with_expected_relative_path() {
    // Rungs 1 to 3 of `pack_format::default_pack_path` redirect the path on
    // purpose; only the plain developer checkout is deterministic.
    if std::env::var_os(pack_format::PACK_PATH_ENV).is_some()
        || pack_format::user_pack_path().is_some_and(|p| p.is_file())
    {
        return;
    }
    let path = AssetPack::default_path();
    assert!(path.ends_with("assets-pack/pokeemerald.pack"));
}

/// What a checkout-validation gate needs from
/// [`AssetPack::repo_pack_path`], and all it needs: an absolute path to
/// `cargo xtask extract`'s output inside *this* workspace, identified by
/// the root that holds the workspace manifest. Holds whatever
/// [`AssetPack::default_path`] resolves to for a running game, and on a
/// machine with any number of packs installed elsewhere.
#[test]
fn repo_pack_path_is_the_workspace_roots_own_extract_output() {
    let path = AssetPack::repo_pack_path();
    assert!(path.is_absolute(), "{} must be absolute", path.display());
    assert!(path.ends_with(pack_format::OUTPUT_RELATIVE_PATH));

    let root = path
        .parent()
        .and_then(std::path::Path::parent)
        .expect("the pack sits two components under the root it names");
    let manifest = std::fs::read_to_string(root.join("Cargo.toml"))
        .expect("the root repo_pack_path names must hold a Cargo manifest");
    assert!(
        manifest.contains("[workspace]"),
        "{} must be this workspace's root, not an install location",
        root.display()
    );
}
