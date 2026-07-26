//! Unit tests for [`super::AssetPack`].
//!
//! Every test builds a small **synthetic** pack in memory rather than
//! touching real upstream art, per the issue's CI caveat (CI has no
//! `pokeemerald/` checkout and no real pack). The one exception,
//! [`real_pack_loads_and_every_typed_accessor_works`], is `#[ignore]`d.

use super::{AssetPack, PackError, MAGIC};
use crate::fonts::FontId;

/// A valid one-GBA-bank text-window palette payload (16 colours, 32
/// bytes — the shape the typed text-window accessors enforce), with the
/// first two colours set so each fixture entry stays distinguishable.
fn text_window_palette_payload(first: u16, second: u16) -> Vec<u8> {
    let mut payload = Vec::with_capacity(32);
    payload.extend_from_slice(&first.to_le_bytes());
    payload.extend_from_slice(&second.to_le_bytes());
    payload.resize(32, 0);
    payload
}

/// The 16 colours of [`text_window_palette_payload`], as the values
/// [`PaletteRef::colors`](super::PaletteRef::colors) should yield.
fn text_window_palette_colors(first: u16, second: u16) -> Vec<u16> {
    let mut colors = vec![first, second];
    colors.resize(16, 0);
    colors
}

/// Fixed image-entry metadata: width, height, bit depth.
fn image_meta(width: u32, height: u32, bit_depth: u8) -> Vec<u8> {
    let mut m = Vec::new();
    m.extend_from_slice(&width.to_le_bytes());
    m.extend_from_slice(&height.to_le_bytes());
    m.push(bit_depth);
    m
}

/// A valid 24x24 border-frame pixel buffer (the exact shape
/// `text_window_frame` requires), seeded so each fixture frame stays
/// distinguishable; every index stays within a 16-colour palette.
fn frame_pixels(seed: u8) -> Vec<u8> {
    (0..24u16 * 24)
        .map(|i| u8::try_from((i + u16::from(seed)) % 16).unwrap())
        .collect()
}

/// A valid 56x16 message-box pixel buffer (the exact shape `message_box`
/// requires); every index stays within a 16-colour palette.
fn message_box_pixels() -> Vec<u8> {
    (0..56u16 * 16)
        .map(|i| u8::try_from(i % 16).unwrap())
        .collect()
}

/// Build a tiny, synthetic pack in memory (never real upstream art — per
/// the issue's CI caveat, no test in this crate touches `pokeemerald/` or
/// the real extracted pack) with one entry of each kind: an `Image`, a
/// `Palette`, and a `Raw` blob.
// Long because it lists one fixture entry per accessor this module tests
// (tileset/layout/font/text-window) — splitting the literal list across
// helper functions would just move the line count, not reduce it.
#[allow(clippy::too_many_lines)]
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
            id: "tileset/test/metatile_attributes",
            kind_tag: 2,
            meta: vec![],
            // Three valid behavior/layer-type pairs, little-endian.
            payload: vec![0x01, 0x00, 0x02, 0x10, 0x03, 0x20],
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
        Entry {
            id: "font/normal/glyphs",
            kind_tag: 0,
            meta: {
                let mut m = Vec::new();
                m.extend_from_slice(&2u32.to_le_bytes()); // width
                m.extend_from_slice(&2u32.to_le_bytes()); // height
                m.push(2); // bit_depth
                m
            },
            payload: vec![0, 1, 2, 3],
        },
        Entry {
            id: "text-window/image/1",
            kind_tag: 0,
            meta: image_meta(24, 24, 4),
            payload: frame_pixels(0),
        },
        Entry {
            id: "text-window/palette/1",
            kind_tag: 1,
            meta: 16u16.to_le_bytes().to_vec(),
            payload: text_window_palette_payload(0x0011, 0x0022),
        },
        // Frame source `2`'s image is fine, but its palette is malformed
        // (2 declared colours in 4 bytes) — read-side validation fodder.
        Entry {
            id: "text-window/image/2",
            kind_tag: 0,
            meta: image_meta(24, 24, 4),
            payload: frame_pixels(1),
        },
        Entry {
            id: "text-window/palette/2",
            kind_tag: 1,
            meta: 2u16.to_le_bytes().to_vec(),
            payload: vec![0x11, 0x00, 0x22, 0x00],
        },
        // Frame source `3` inverts it: a valid 16-colour palette, but an
        // 8-bit-indexed image whose pixel 16 that palette cannot map.
        Entry {
            id: "text-window/image/3",
            kind_tag: 0,
            meta: image_meta(24, 24, 8),
            payload: {
                let mut pixels = frame_pixels(2);
                pixels[5] = 16;
                pixels
            },
        },
        Entry {
            id: "text-window/palette/3",
            kind_tag: 1,
            meta: 16u16.to_le_bytes().to_vec(),
            payload: text_window_palette_payload(0x0099, 0x00AA),
        },
        // Frame source `4` declares the right 24x24 shape but carries only
        // 3 payload bytes — the ImageRef pixel-count invariant would be
        // false.
        Entry {
            id: "text-window/image/4",
            kind_tag: 0,
            meta: image_meta(24, 24, 4),
            payload: vec![0, 1, 2],
        },
        Entry {
            id: "text-window/palette/4",
            kind_tag: 1,
            meta: 16u16.to_le_bytes().to_vec(),
            payload: text_window_palette_payload(0x00BB, 0x00CC),
        },
        // Frame source `5` is a self-consistent 8x8 bitmap (payload
        // matches its declared dimensions) — but a border frame must be a
        // complete 3x3 grid of 8x8 tiles, i.e. exactly 24x24.
        Entry {
            id: "text-window/image/5",
            kind_tag: 0,
            meta: image_meta(8, 8, 4),
            payload: vec![0; 64],
        },
        Entry {
            id: "text-window/palette/5",
            kind_tag: 1,
            meta: 16u16.to_le_bytes().to_vec(),
            payload: text_window_palette_payload(0x00DD, 0x00EE),
        },
        Entry {
            id: "text-window/image/20",
            kind_tag: 0,
            meta: image_meta(24, 24, 4),
            payload: frame_pixels(3),
        },
        Entry {
            id: "text-window/palette/20",
            kind_tag: 1,
            meta: 16u16.to_le_bytes().to_vec(),
            payload: text_window_palette_payload(0x0077, 0x0088),
        },
        Entry {
            id: "text-window/image/message_box",
            kind_tag: 0,
            meta: image_meta(56, 16, 4),
            payload: message_box_pixels(),
        },
        Entry {
            id: "text-window/palette/message_box",
            kind_tag: 1,
            meta: 16u16.to_le_bytes().to_vec(),
            payload: text_window_palette_payload(0x0033, 0x0044),
        },
        Entry {
            id: "text-window/palette/text_pal1",
            kind_tag: 1,
            meta: 16u16.to_le_bytes().to_vec(),
            payload: text_window_palette_payload(0x0055, 0x0066),
        },
        // Declares the right 16-colour metadata over a truncated 8-byte
        // payload — metadata alone must not be trusted on read.
        Entry {
            id: "text-window/palette/text_pal2",
            kind_tag: 1,
            meta: 16u16.to_le_bytes().to_vec(),
            payload: vec![0x55, 0x00, 0x66, 0x00, 0x77, 0x00, 0x88, 0x00],
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
fn font_accessor_reaches_the_glyph_sheet_image() {
    let path = write_synthetic_pack("font");
    let pack = AssetPack::load(&path).unwrap();

    let source = pack.font(FontId::Normal).unwrap();
    assert_eq!(source.font(), FontId::Normal);
    let image = source.image();
    assert_eq!(image.width, 2);
    assert_eq!(image.height, 2);
    assert_eq!(image.bit_depth, 2);
    assert_eq!(image.pixels, &[0, 1, 2, 3]);

    let err = pack.font(FontId::Small).unwrap_err();
    assert!(matches!(err, PackError::UnknownAsset(id) if id == "font/small/glyphs"));

    let _ = std::fs::remove_file(path);
}

#[test]
fn text_window_frame_bundles_tiles_and_its_own_plte_derived_palette() {
    let path = write_synthetic_pack("text-window-frame");
    let pack = AssetPack::load(&path).unwrap();

    let frame = pack.text_window_frame(0).unwrap();
    assert_eq!(frame.tiles.width, 24);
    assert_eq!(frame.tiles.height, 24);
    assert_eq!(frame.tiles.pixels, frame_pixels(0).as_slice());
    assert_eq!(frame.palette.color_count, 16);
    assert_eq!(frame.palette.color(0), Some(0x0011));
    assert_eq!(frame.palette.color(1), Some(0x0022));

    let err = pack.text_window_frame(5).unwrap_err();
    assert!(matches!(err, PackError::UnknownAsset(id) if id == "text-window/image/6"));

    let last = pack.text_window_frame(19).unwrap();
    assert_eq!(last.tiles.pixels, frame_pixels(3).as_slice());
    assert_eq!(
        last.palette.colors().collect::<Vec<_>>(),
        text_window_palette_colors(0x0077, 0x0088)
    );

    let fallback = pack.text_window_frame(20).unwrap();
    assert_eq!(fallback.tiles.pixels, frame.tiles.pixels);
    assert_eq!(
        fallback.palette.colors().collect::<Vec<_>>(),
        text_window_palette_colors(0x0011, 0x0022)
    );

    let far_out_of_range = pack.text_window_frame(u8::MAX).unwrap();
    assert_eq!(far_out_of_range.tiles.pixels, frame.tiles.pixels);

    let _ = std::fs::remove_file(path);
}

#[test]
fn message_box_bundles_its_own_tiles_and_palette() {
    let path = write_synthetic_pack("message-box");
    let pack = AssetPack::load(&path).unwrap();

    let handle = pack.message_box().unwrap();
    assert_eq!(handle.tiles.width, 56);
    assert_eq!(handle.tiles.height, 16);
    assert_eq!(handle.tiles.pixels, message_box_pixels().as_slice());
    assert_eq!(handle.palette.color_count, 16);
    assert_eq!(handle.palette.color(0), Some(0x0033));
    assert_eq!(handle.palette.color(1), Some(0x0044));

    let _ = std::fs::remove_file(path);
}

#[test]
fn malformed_text_window_palettes_are_rejected_on_read() {
    let path = write_synthetic_pack("malformed-text-window-palette");
    let pack = AssetPack::load(&path).unwrap();

    // Frame id 1 selects source `2`: its image entry is fine, but the
    // palette declares 2 colours in 4 bytes — the typed handle's
    // 16-colour invariant would be false.
    let err = pack.text_window_frame(1).unwrap_err();
    assert!(matches!(
        &err,
        PackError::MalformedTextWindowPalette {
            id,
            color_count: 2,
            byte_len: 4,
        } if id == "text-window/palette/2"
    ));

    // `text_pal2` declares the right 16-colour metadata over a truncated
    // 8-byte payload — metadata alone must not be trusted, or
    // `PaletteRef::colors()` (which walks the raw payload) would disagree
    // with `color_count`.
    let err = pack.text_window_extra_palette(2).unwrap_err();
    assert!(matches!(
        &err,
        PackError::MalformedTextWindowPalette {
            id,
            color_count: 16,
            byte_len: 8,
        } if id == "text-window/palette/text_pal2"
    ));

    // Frame id 2 selects source `3`: a valid 16-colour palette, but an
    // 8-bit-indexed tile bitmap holding pixel 16 — the read side must
    // reject the pair rather than hand a renderer an unmappable pixel.
    let err = pack.text_window_frame(2).unwrap_err();
    assert!(matches!(
        &err,
        PackError::TextWindowPixelOutsidePalette {
            id,
            pixel: 16,
            palette_len: 16,
        } if id == "text-window/image/3"
    ));

    // Frame id 3 selects source `4`: the right 24x24 shape over a 3-byte
    // payload — the ImageRef pixel-count invariant would be false, so the
    // typed accessor rejects it.
    let err = pack.text_window_frame(3).unwrap_err();
    assert!(matches!(
        &err,
        PackError::MalformedTextWindowImage {
            id,
            width: 24,
            height: 24,
            byte_len: 3,
        } if id == "text-window/image/4"
    ));

    // Frame id 4 selects source `5`: a self-consistent 8x8 bitmap — but a
    // border frame must be the complete 3x3 grid of 8x8 tiles (24x24), so
    // the typed accessor rejects the wrong shape outright.
    let err = pack.text_window_frame(4).unwrap_err();
    assert!(matches!(
        &err,
        PackError::TextWindowImageWrongDimensions {
            id,
            width: 8,
            height: 8,
            expected_width: 24,
            expected_height: 24,
        } if id == "text-window/image/5"
    ));

    // The generic untyped accessors still expose the entries as-is; only
    // the typed text-window accessors enforce the pairing invariants.
    assert!(pack.palette("text-window/palette/2").is_ok());
    assert!(pack.image("text-window/image/3").is_ok());
    assert!(pack.image("text-window/image/4").is_ok());

    let _ = std::fs::remove_file(path);
}

#[test]
fn text_window_extra_palette_reaches_the_sibling_pal_files() {
    let path = write_synthetic_pack("text-window-extra-palette");
    let pack = AssetPack::load(&path).unwrap();

    let palette = pack.text_window_extra_palette(1).unwrap();
    assert_eq!(palette.color(0), Some(0x0055));
    assert_eq!(palette.color(1), Some(0x0066));

    let err = pack.text_window_extra_palette(9).unwrap_err();
    assert!(matches!(err, PackError::UnknownAsset(id) if id == "text-window/palette/text_pal9"));

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

    // Build the handle from the synthetic pack's typed entries so this
    // exercises `TilesetHandle::metatile_attribute_table` without needing
    // all 16 distinct palette slots the full `tileset()` bundler requires.
    let path = write_synthetic_pack("metatile-attrs");
    let pack = AssetPack::load(&path).unwrap();
    let palette = pack.palette("tileset/test/palette/00").unwrap();
    let handle = super::TilesetHandle {
        tiles: pack.image("tileset/test/tiles").unwrap(),
        palettes: [palette; 16],
        metatiles: pack.raw("tileset/test/metatiles").unwrap(),
        metatile_attributes: pack.raw("tileset/test/metatile_attributes").unwrap(),
    };

    let table = handle.metatile_attribute_table();
    assert_eq!(table.len(), 3);
    for (id, raw) in [0x0001u16, 0x1002, 0x2003].into_iter().enumerate() {
        let attr = table
            .attribute_at(u16::try_from(id).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(attr, MetatileAttribute::from_raw(raw).unwrap());
    }
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
// Long because it's one end-to-end smoke test exercising every typed
// accessor this module offers (tileset/sprite/title/layout/font/text-window)
// against the real extracted pack -- splitting it up would just scatter the
// same "does the real pack round-trip" assertion across several `#[ignore]`d
// tests that all need the same `cargo xtask extract` precondition.
#[allow(clippy::too_many_lines)]
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_loads_and_every_typed_accessor_works() {
    use crate::fonts::FontId;
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

    // Every Latin font's glyph sheet should be present and decode through
    // `crate::fonts::FontGlyphSheet`, glyph 0 (the space) included.
    for font in FontId::ALL {
        let image = pack
            .font(font)
            .unwrap_or_else(|e| panic!("font `{}` should be in the pack: {e}", font.pack_name()));
        assert_eq!(image.image().width, crate::fonts::SHEET_WIDTH);
        assert_eq!(image.image().height, crate::fonts::SHEET_HEIGHT);
        let sheet = crate::fonts::FontGlyphSheet::new(image)
            .unwrap_or_else(|e| panic!("font `{}` sheet shape: {e}", font.pack_name()));
        let glyph = sheet
            .glyph(0)
            .unwrap_or_else(|| panic!("font `{}` glyph 0 should decode", font.pack_name()));
        assert_eq!(glyph.advance_width, font.glyph_width(0).unwrap());
    }

    // Every numbered text-window border frame should bundle a tile bitmap
    // with a 16-colour palette read out of its own PNG `PLTE` chunk, the
    // default message-box frame likewise, and the four extra textbox
    // palettes should each be a 16-colour palette too.
    for frame_id in 0..20u8 {
        let frame = pack.text_window_frame(frame_id).unwrap_or_else(|e| {
            panic!("text-window frame id {frame_id} should be in the pack: {e}")
        });
        assert!(frame.tiles.width > 0 && frame.tiles.height > 0);
        assert_eq!(frame.palette.color_count, 16);
        assert_eq!(frame.palette.colors().count(), 16);
    }
    let message_box = pack
        .message_box()
        .expect("message-box frame should be in the pack");
    assert!(message_box.tiles.width > 0 && message_box.tiles.height > 0);
    assert_eq!(message_box.palette.color_count, 16);
    for n in 1..=4u8 {
        let palette = pack
            .text_window_extra_palette(n)
            .unwrap_or_else(|e| panic!("text-window extra palette {n} should be in the pack: {e}"));
        assert_eq!(palette.color_count, 16);
    }
}
