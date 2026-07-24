//! Unit tests for [`super::AssetPack`].
//!
//! Every test builds a small **synthetic** pack in memory rather than
//! touching real upstream art, per the issue's CI caveat (CI has no
//! `pokeemerald/` checkout and no real pack). The one exception,
//! [`real_pack_loads_and_every_typed_accessor_works`], is `#[ignore]`d.

use super::{AssetPack, PackError, MAGIC};

/// Build a tiny, synthetic pack in memory (never real upstream art — per
/// the issue's CI caveat, no test in this crate touches `pokeemerald/` or
/// the real extracted pack) with one entry of each kind: an `Image`, a
/// `Palette`, and a `Raw` blob.
fn synthetic_pack() -> Vec<u8> {
    struct Entry {
        id: &'static str,
        kind_tag: u8,
        meta: Vec<u8>,
        payload: Vec<u8>,
    }
    let mut entries = vec![
        Entry {
            id: "tileset/test/tiles",
            kind_tag: 0,
            meta: {
                let mut m = Vec::new();
                m.extend_from_slice(&2u32.to_le_bytes()); // width
                m.extend_from_slice(&2u32.to_le_bytes()); // height
                m.push(8); // bit_depth
                m
            },
            payload: vec![1, 2, 3, 4],
        },
        Entry {
            id: "tileset/test/palette/00",
            kind_tag: 1,
            meta: 2u16.to_le_bytes().to_vec(),     // color_count
            payload: vec![0xFF, 0x7F, 0x00, 0x00], // two GBA555 colors
        },
        Entry {
            id: "tileset/test/metatiles",
            kind_tag: 2,
            meta: vec![],
            payload: vec![9, 9, 9],
        },
        Entry {
            id: "layout/test/map",
            kind_tag: 2,
            meta: vec![],
            // A 2x1 grid: two raw MetatileCell u16s, little-endian.
            payload: vec![0x01, 0x00, 0x02, 0x00],
        },
        Entry {
            id: "layout/test/border",
            kind_tag: 2,
            meta: vec![],
            // A fixed 2x2 border block (8 bytes).
            payload: vec![0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0x04, 0x00],
        },
    ];
    // Directory entries must be written in id-sorted order, exactly like
    // the real writer (`extract::pack::PackWriter::finish`) -- sort here
    // rather than trusting the literal array order above, so
    // reordering/adding fixture entries later can't quietly reintroduce an
    // unsorted directory the reader's binary search then misses.
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
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&1u32.to_le_bytes()); // format_version
    out.extend_from_slice(&u32::try_from(entries.len()).unwrap().to_le_bytes());
    for (e, &off) in entries.iter().zip(&offsets) {
        out.extend_from_slice(&u16::try_from(e.id.len()).unwrap().to_le_bytes());
        out.extend_from_slice(e.id.as_bytes());
        out.push(e.kind_tag);
        out.extend_from_slice(&(off as u64).to_le_bytes());
        out.extend_from_slice(&(e.payload.len() as u64).to_le_bytes());
        out.extend_from_slice(&e.meta);
    }
    for e in &entries {
        out.extend_from_slice(&e.payload);
    }
    out
}

fn write_synthetic_pack(dir_hint: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-assets-pack-test-{dir_hint}-{}.pack",
        std::process::id()
    ));
    std::fs::write(&path, synthetic_pack()).unwrap();
    path
}

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
    let rendered = err.to_string();
    assert!(rendered.contains("init.sh"));
    assert!(rendered.contains("cargo xtask extract"));
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

#[test]
fn layout_map_and_border_are_raw_blobs() {
    let path = write_synthetic_pack("layout-raw");
    let pack = AssetPack::load(&path).unwrap();

    let map_bytes = pack.layout_map("test").unwrap();
    assert_eq!(map_bytes, &[0x01, 0x00, 0x02, 0x00]);

    let border_bytes = pack.layout_border("test").unwrap();
    assert_eq!(
        border_bytes,
        &[0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0x04, 0x00]
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn layout_map_bytes_decode_through_map_layouts_layout_grid() {
    // Exercises the intended pipeline end to end: pack bytes -> a caller-
    // supplied `MapLayout` -> `LayoutGrid` decode. This crate's pack loader
    // never constructs a `LayoutGrid` itself (see `pack`'s module docs);
    // this test proves the two sides agree on the byte shape regardless.
    use crate::map_layouts::{LayoutGrid, LayoutId, MapLayout, MetatileCell};

    let path = write_synthetic_pack("layout-decode");
    let pack = AssetPack::load(&path).unwrap();
    let map_bytes = pack.layout_map("test").unwrap();

    let layout = MapLayout {
        id: LayoutId("LAYOUT_TEST"),
        name: "Test_Layout",
        width: 2,
        height: 1,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_General",
    };
    let grid = LayoutGrid::new(&layout, map_bytes).unwrap();
    let cells: Vec<_> = grid.cells().collect();
    assert_eq!(
        cells,
        vec![MetatileCell::from_raw(1), MetatileCell::from_raw(2)]
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn layout_border_bytes_decode_through_map_layouts_border_grid() {
    use crate::map_layouts::{BorderGrid, MetatileCell};

    let path = write_synthetic_pack("border-decode");
    let pack = AssetPack::load(&path).unwrap();
    let border_bytes = pack.layout_border("test").unwrap();

    let border = BorderGrid::new(border_bytes).unwrap();
    let cells: Vec<_> = border.cells().collect();
    assert_eq!(
        cells,
        vec![
            MetatileCell::from_raw(1),
            MetatileCell::from_raw(2),
            MetatileCell::from_raw(3),
            MetatileCell::from_raw(4),
        ]
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn unknown_layout_name_reports_missing_asset() {
    let path = write_synthetic_pack("layout-unknown");
    let pack = AssetPack::load(&path).unwrap();
    let err = pack.layout_map("does_not_exist").unwrap_err();
    assert!(matches!(err, PackError::UnknownAsset(id) if id == "layout/does_not_exist/map"));
    let _ = std::fs::remove_file(path);
}

#[test]
fn tileset_metatile_attribute_table_decodes_from_the_bundled_raw_bytes() {
    use crate::metatile_attributes::MetatileAttribute;

    // Build a pack whose `tileset/test/metatiles` payload doubles as (for
    // this test's purposes) the metatile_attributes bytes too -- simplest
    // way to exercise `TilesetHandle::metatile_attribute_table` without
    // needing all 16 palette slots the full `tileset()` bundler requires.
    let path = write_synthetic_pack("metatile-attrs");
    let pack = AssetPack::load(&path).unwrap();
    let raw = pack.raw("tileset/test/metatiles").unwrap();
    let table = crate::metatile_attributes::MetatileAttributeTable::new(raw);
    assert_eq!(table.len(), 1);
    let attr = table.attribute_at(0).unwrap().unwrap();
    assert_eq!(
        attr,
        MetatileAttribute::from_raw(u16::from_le_bytes([9, 9])).unwrap()
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn default_path_ends_with_expected_relative_path() {
    let path = AssetPack::default_path();
    assert!(path.ends_with("assets-pack/pokeemerald.pack"));
}

/// Loads the *real* local pack (`cargo xtask extract`'s output) and
/// exercises every typed accessor against it -- proof the writer
/// (`xtask::extract::pack`) and this reader agree byte-for-byte on the
/// format, not just on the synthetic fixtures above. Needs a local pack:
/// run `cargo xtask extract` first, then `cargo test -p assets -- --ignored`.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_loads_and_every_typed_accessor_works() {
    use crate::map_layouts::{BorderGrid, LayoutId, LayoutTable};

    let pack = AssetPack::load_default().expect("run `cargo xtask extract` first");

    let general = pack
        .tileset("general")
        .expect("general tileset should be in the pack");
    assert!(general.tiles.width > 0 && general.tiles.height > 0);
    assert_eq!(
        general.tiles.pixels.len(),
        general.tiles.width as usize * general.tiles.height as usize
    );
    for palette in &general.palettes {
        assert_eq!(palette.color_count, 16);
        assert_eq!(palette.colors().count(), 16);
    }
    assert!(!general.metatiles.is_empty());
    assert!(!general.metatile_attributes.is_empty());

    for name in [
        "general",
        "building",
        "petalburg",
        "brendans_mays_house",
        "lab",
    ] {
        pack.tileset(name)
            .unwrap_or_else(|e| panic!("tileset `{name}` should load: {e}"));
    }

    let walking = pack
        .sprite("brendan/walking")
        .expect("brendan/walking sprite");
    assert!(walking.width > 0 && walking.height > 0);
    let brendan_palette = pack.sprite_palette("brendan").expect("brendan palette");
    assert_eq!(brendan_palette.color_count, 16);
    pack.sprite_palette("may").expect("may palette");

    let logo = pack
        .image("title/image/pokemon_logo")
        .expect("title logo image");
    assert!(logo.width > 0 && logo.height > 0);
    pack.palette("title/palette/pokemon_logo")
        .expect("title logo palette");
    pack.raw("title/raw/pokemon_logo")
        .expect("title logo raw tilemap");

    // `general`'s metatile_attributes should decode cleanly end to end
    // (every real upstream layer-type value is 0..=2).
    let attr_table = general.metatile_attribute_table();
    assert!(!attr_table.is_empty());
    for attr in attr_table.attributes() {
        attr.unwrap_or_else(|e| panic!("metatile attribute decode failed: {e}"));
    }

    // Every Littleroot Town layout's map/border bytes should be present and
    // decode through `crate::map_layouts`'s typed grid views, using the
    // hand-transcribed `LayoutTable` metadata for dimensions.
    let table = LayoutTable::new();
    for (layout_id, pack_name) in [
        ("LAYOUT_LITTLEROOT_TOWN", "littleroot_town"),
        (
            "LAYOUT_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F",
            "littleroot_town_brendans_house_1f",
        ),
        (
            "LAYOUT_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F",
            "littleroot_town_brendans_house_2f",
        ),
        (
            "LAYOUT_LITTLEROOT_TOWN_MAYS_HOUSE_1F",
            "littleroot_town_mays_house_1f",
        ),
        (
            "LAYOUT_LITTLEROOT_TOWN_MAYS_HOUSE_2F",
            "littleroot_town_mays_house_2f",
        ),
        (
            "LAYOUT_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB",
            "littleroot_town_professor_birchs_lab",
        ),
        (
            "LAYOUT_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB_WITH_TABLE",
            "littleroot_town_professor_birchs_lab_with_table",
        ),
    ] {
        let layout = table
            .layout(LayoutId(layout_id))
            .unwrap_or_else(|e| panic!("{layout_id} should be in LayoutTable: {e}"));
        let map_bytes = pack
            .layout_map(pack_name)
            .unwrap_or_else(|e| panic!("layout/{pack_name}/map should be in the pack: {e}"));
        let grid = layout
            .grid(map_bytes)
            .unwrap_or_else(|e| panic!("{layout_id}'s map.bin should decode: {e}"));
        assert_eq!(grid.cell_count(), layout.cell_count());

        let border_bytes = pack
            .layout_border(pack_name)
            .unwrap_or_else(|e| panic!("layout/{pack_name}/border should be in the pack: {e}"));
        let border = BorderGrid::new(border_bytes)
            .unwrap_or_else(|e| panic!("{layout_id}'s border.bin should decode: {e}"));
        assert_eq!(border.cells().count(), crate::map_layouts::BORDER_CELLS);
    }
}
