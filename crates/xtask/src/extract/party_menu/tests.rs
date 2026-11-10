use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::super::png::tests::indexed_png_from_raw_with_plte;
use super::super::{extract_to, repo_root, upstream_present};
use super::{extract_map, extract_party_menu, extract_sheet, pack_ids, PARTY_MENU_DIRECTORY};
use super::{PaletteSource, MAPS, SHEETS};
use crate::extract::jasc_pal;
use crate::extract::{read_file, ExtractError};
use pack_format::{DirectoryEntry, EntryKind, PackWriter};

/// Every id the party menu publishes, written out independently of the manifest
/// the extractor reads so that dropping an entry there fails here.
const EXPECTED_IDS: [&str; 15] = [
    "party-menu/image/bg",
    "party-menu/image/pokeball",
    "party-menu/image/hold_icons",
    "party-menu/image/pokeball_small",
    "party-menu/palette/bg",
    "party-menu/palette/pokeball",
    "party-menu/palette/hold_icons",
    "party-menu/raw/bg",
    "party-menu/raw/slot_main",
    "party-menu/raw/slot_main_no_hp",
    "party-menu/raw/slot_wide",
    "party-menu/raw/slot_wide_no_hp",
    "party-menu/raw/slot_wide_empty",
    "party-menu/raw/confirm_button",
    "party-menu/raw/cancel_button",
];

/// Source `(stem, width, height, PNG bit depth, PLTE colours)`, and the published
/// `(width, height)` of its image.
type SheetShape = (&'static str, u32, u32, u8, usize, (u32, u32));

const SHEET_SHAPES: [SheetShape; 4] = [
    ("bg", 64, 64, 8, 176, (8, 496)),
    ("pokeball", 32, 64, 4, 16, (32, 64)),
    ("hold_icons", 8, 16, 4, 16, (8, 16)),
    ("pokeball_small", 16, 96, 4, 16, (16, 96)),
];
const MAP_LENGTHS: [(&str, usize); 8] = [
    ("bg", 2048),
    ("slot_main", 70),
    ("slot_main_no_hp", 70),
    ("slot_wide", 54),
    ("slot_wide_no_hp", 54),
    ("slot_wide_empty", 54),
    ("confirm_button", 28),
    ("cancel_button", 28),
];

fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pokeemerald-rs-party-menu-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(PARTY_MENU_DIRECTORY)).unwrap();
    dir
}

fn source_dir(upstream: &Path) -> PathBuf {
    upstream.join(PARTY_MENU_DIRECTORY)
}

fn write(upstream: &Path, file: &str, bytes: impl AsRef<[u8]>) -> PathBuf {
    let path = source_dir(upstream).join(file);
    std::fs::write(&path, bytes).unwrap();
    path
}

fn shape(stem: &str) -> SheetShape {
    *SHEET_SHAPES.iter().find(|s| s.0 == stem).unwrap()
}

/// The source raster: every tile distinct and non-uniform, with indices up to 63
/// in `bg` so that narrowing matters, and `bg`'s last two tiles blank (as the
/// ones upstream's `-num_tiles 62` drops are).
fn source_pixels(stem: &str) -> Vec<u8> {
    let (_, width, height, depth, ..) = shape(stem);
    (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .map(|(x, y)| {
            let tile = (y / 8) * (width / 8) + x / 8;
            if stem == "bg" && tile >= 62 {
                return 0;
            }
            let value = tile * 5 + (x % 8) + 3 * (y % 8) + 1;
            u8::try_from(if depth == 8 { value % 64 } else { value % 16 }).unwrap()
        })
        .collect()
}

fn plte(colors: usize, seed: u8) -> Vec<u8> {
    (0..colors)
        .flat_map(|index| {
            [
                seed.wrapping_add(u8::try_from(index % 32).unwrap() * 8),
                16,
                u8::try_from(index / 32).unwrap() * 8,
            ]
        })
        .collect()
}

fn png(stem: &str, pixels: &[u8], width: u32, height: u32, depth: u8, colors: usize) -> Vec<u8> {
    let mut raw = Vec::new();
    for row in pixels.chunks(width as usize) {
        raw.push(0);
        if depth == 4 {
            raw.extend(row.chunks(2).map(|pair| (pair[0] << 4) | pair[1]));
        } else {
            raw.extend_from_slice(row);
        }
    }
    indexed_png_from_raw_with_plte(depth, width, height, &raw, &plte(colors, seed(stem)))
}

fn seed(stem: &str) -> u8 {
    stem.bytes().fold(0u8, u8::wrapping_add)
}

/// The sheet whose palette `stem` draws with: the small ball shares the big
/// ball's, so its PNG embeds the same colours.
fn palette_stem(stem: &str) -> &str {
    if stem == "pokeball_small" {
        "pokeball"
    } else {
        stem
    }
}

fn sheet_png(stem: &str) -> Vec<u8> {
    let (_, width, height, depth, colors, _) = shape(stem);
    png(
        palette_stem(stem),
        &source_pixels(stem),
        width,
        height,
        depth,
        colors,
    )
}

/// The colours `pokeball.png` embeds, which the small ball must match.
fn ball_colors() -> Vec<jasc_pal::Rgb888> {
    crate::extract::png::decode_palette(&sheet_png("pokeball")).unwrap()
}

fn map_bytes(stem: &str) -> Vec<u8> {
    let (index, (_, len)) = MAP_LENGTHS
        .iter()
        .enumerate()
        .find(|(_, (s, _))| *s == stem)
        .unwrap();
    (0..*len)
        .map(|i| u8::try_from((i + index * 37) % 251).unwrap())
        .collect()
}

fn write_valid_checkout(upstream: &Path) {
    for (stem, ..) in SHEET_SHAPES {
        write(upstream, &format!("{stem}.png"), sheet_png(stem));
    }
    for (stem, _) in MAP_LENGTHS {
        write(upstream, &format!("{stem}.bin"), map_bytes(stem));
    }
}

fn source_files() -> Vec<String> {
    let sheets = SHEET_SHAPES.iter().map(|s| format!("{}.png", s.0));
    let maps = MAP_LENGTHS.iter().map(|m| format!("{}.bin", m.0));
    sheets.chain(maps).collect()
}

fn mismatch_path(err: ExtractError) -> PathBuf {
    match err {
        ExtractError::PartyMenuAssetMismatch { path, .. } => path,
        other => panic!("expected a party-menu mismatch, got {other}"),
    }
}

fn payload<'a>(pack: &'a [u8], entry: &DirectoryEntry) -> &'a [u8] {
    &pack[entry.offset..entry.offset + entry.length]
}

fn extracted(upstream: &Path) -> Vec<u8> {
    let mut writer = PackWriter::new();
    extract_party_menu(upstream, &mut writer).unwrap();
    writer.finish().unwrap()
}

/// BGR555 little-endian bytes of `plte(colors, seed)`.
fn expected_palette(colors: usize, seed: u8) -> Vec<u8> {
    plte(colors, seed)
        .chunks(3)
        .flat_map(|rgb| {
            let color = (u16::from(rgb[0]) >> 3)
                | ((u16::from(rgb[1]) >> 3) << 5)
                | ((u16::from(rgb[2]) >> 3) << 10);
            color.to_le_bytes()
        })
        .collect()
}

/// The first `kept` tiles of `pixels` (`width` wide, 8 tall tiles), each tile's
/// rows contiguous, with every index narrowed to its low nibble.
fn leading_tiles(pixels: &[u8], width: usize, kept: usize) -> Vec<u8> {
    let per_row = width / 8;
    (0..kept)
        .flat_map(|tile| {
            let (column, tile_row) = (tile % per_row, tile / per_row);
            (0..8).flat_map(move |row| {
                let start = (tile_row * 8 + row) * width + column * 8;
                start..start + 8
            })
        })
        .map(|i| pixels[i] & 0x0F)
        .collect()
}

#[test]
fn the_inventory_is_the_fifteen_listed_ids_with_no_palette_for_the_small_ball() {
    let ids = pack_ids();
    assert_eq!(ids, EXPECTED_IDS);
    assert_eq!(ids.iter().collect::<BTreeSet<_>>().len(), 15);
    assert_eq!(
        SHEETS.iter().map(|s| s.stem).collect::<Vec<_>>(),
        ["bg", "pokeball", "hold_icons", "pokeball_small"]
    );
    assert_eq!(MAPS, MAP_LENGTHS);
    assert!(!ids.contains(&"party-menu/palette/pokeball_small".to_owned()));
}

#[test]
fn a_shared_palette_names_a_sheet_listed_before_it() {
    for (index, sheet) in SHEETS.iter().enumerate() {
        if let PaletteSource::Shared { with } = sheet.palette {
            assert!(with < index, "{}", sheet.stem);
            assert!(matches!(SHEETS[with].palette, PaletteSource::Own { .. }));
        }
    }
}

#[test]
fn a_complete_checkout_yields_every_entry_with_its_shape_and_payload() {
    let upstream = scratch_dir("valid");
    write_valid_checkout(&upstream);
    // Unlisted neighbours must not change the pack and are never opened.
    write(&upstream, "bg.4bpp", b"built");
    write(&upstream, "stray.bin", b"stray");

    let pack = extracted(&upstream);
    let entries = pack_format::parse_directory(&pack).unwrap();
    assert_eq!(entries.len(), 15);
    assert_eq!(
        entries
            .iter()
            .map(|e| e.id.as_str())
            .collect::<BTreeSet<_>>(),
        EXPECTED_IDS.into_iter().collect::<BTreeSet<_>>()
    );

    for entry in &entries {
        let stem = entry.id.rsplit('/').next().unwrap();
        match &entry.kind {
            EntryKind::Image {
                width,
                height,
                bit_depth,
            } => {
                let (_, _, _, _, _, (w, h)) = shape(stem);
                assert_eq!((*width, *height, *bit_depth), (w, h, 4), "{}", entry.id);
                assert_eq!(entry.length, (w * h) as usize, "{}", entry.id);
                let (_, source_w, _, _, _, _) = shape(stem);
                let source = source_pixels(stem);
                let expected: Vec<u8> = if stem == "bg" {
                    leading_tiles(&source, source_w as usize, 62)
                } else {
                    source
                };
                assert_eq!(payload(&pack, entry), expected.as_slice(), "{}", entry.id);
            }
            EntryKind::Palette { color_count } => {
                let (_, _, _, _, colors, _) = shape(stem);
                assert_eq!(usize::from(*color_count), colors, "{}", entry.id);
                assert_eq!(
                    payload(&pack, entry),
                    expected_palette(colors, seed(stem)).as_slice(),
                    "{}",
                    entry.id
                );
            }
            EntryKind::Raw => {
                assert_eq!(payload(&pack, entry), map_bytes(stem), "{}", entry.id);
            }
        }
    }
    std::fs::remove_dir_all(upstream).unwrap();
}

#[test]
fn the_background_is_narrowed_then_cut_to_its_first_62_tiles_in_tile_order() {
    let upstream = scratch_dir("bg-cut");
    write_valid_checkout(&upstream);
    let pack = extracted(&upstream);
    let entries = pack_format::parse_directory(&pack).unwrap();
    let bg = entries
        .iter()
        .find(|e| e.id == "party-menu/image/bg")
        .unwrap();
    let strip = payload(&pack, bg);
    assert_eq!(strip.len(), 8 * 496);
    assert!(strip.iter().all(|&p| p < 16));

    let source = source_pixels("bg");
    assert!(source.iter().any(|&p| p >= 16), "narrowing must matter");
    // Not a raster-order truncation, and not an un-narrowed one.
    let raster_prefix: Vec<u8> = source[..strip.len()].iter().map(|p| p & 0x0F).collect();
    assert_ne!(strip, raster_prefix.as_slice());
    // The strip packs to upstream's 62 built tiles (1,984 bytes).
    let built =
        pack_format::tiles_from_image(&leading_tiles(&source, 64, 62), 4, 8, 496, None).unwrap();
    let recovered = pack_format::tiles_from_image(strip, 4, 8, 496, None).unwrap();
    assert_eq!(recovered.len(), 62 * 32);
    assert_eq!(recovered, built);
    std::fs::remove_dir_all(upstream).unwrap();
}

#[test]
fn a_background_with_colour_in_a_dropped_tile_is_rejected_after_narrowing() {
    let upstream = scratch_dir("bg-dropped");
    let sheet = &SHEETS[0];
    let dir = source_dir(&upstream);
    let (_, width, height, depth, colors, _) = shape("bg");
    let path = dir.join("bg.png");
    for (index, accepted) in [(0x10, true), (0x11, false), (0x01, false), (0x3F, false)] {
        // A pixel of the last tile (63), bottom-right.
        let mut pixels = source_pixels("bg");
        *pixels.last_mut().unwrap() = index;
        std::fs::write(&path, png("bg", &pixels, width, height, depth, colors)).unwrap();
        let mut writer = PackWriter::new();
        let result = extract_sheet(&dir, sheet, &[], &mut writer);
        if accepted {
            // 0x10 narrows to blank, as `gbagfx` narrows before it cuts.
            result.unwrap();
        } else {
            assert_eq!(mismatch_path(result.unwrap_err()), path, "{index:#x}");
            assert_eq!(writer.len(), 0);
        }
    }
    // A colour in tile 62 is rejected the same way.
    let mut pixels = source_pixels("bg");
    pixels[62 / 8 * 8 * 64 + (62 % 8) * 8] = 2;
    std::fs::write(&path, png("bg", &pixels, width, height, depth, colors)).unwrap();
    assert_eq!(
        mismatch_path(extract_sheet(&dir, sheet, &[], &mut PackWriter::new()).unwrap_err()),
        path
    );
    std::fs::remove_dir_all(upstream).unwrap();
}

#[test]
fn a_missing_source_fails_extraction_naming_that_file() {
    for file in source_files() {
        let upstream = scratch_dir("missing-one");
        write_valid_checkout(&upstream);
        std::fs::remove_file(source_dir(&upstream).join(&file)).unwrap();
        let err = extract_party_menu(&upstream, &mut PackWriter::new()).unwrap_err();
        assert!(
            matches!(&err, ExtractError::ReadFailed(path, _) if path.ends_with(&file)),
            "{file}: {err}"
        );
        std::fs::remove_dir_all(upstream).unwrap();
    }
}

#[test]
fn a_sheet_with_the_wrong_depth_size_or_palette_count_is_rejected_before_it_is_queued() {
    let upstream = scratch_dir("sheet-shape");
    let dir = source_dir(&upstream);
    for (sheet, shape) in SHEETS.iter().zip(SHEET_SHAPES) {
        let (stem, w, h, depth, colors, _) = shape;
        let file = format!("{stem}.png");
        let path = write(&upstream, &file, b"not a png");
        let mut writer = PackWriter::new();
        let ball = ball_colors();
        let err = extract_sheet(&dir, sheet, &ball, &mut writer).unwrap_err();
        assert!(
            matches!(&err, ExtractError::Png(p, _) if *p == path),
            "{file}"
        );

        let other_depth = if depth == 8 { 4 } else { 8 };
        let cases: Vec<(&str, Vec<u8>)> = vec![
            (
                "other depth",
                png(stem, &vec![1; (w * h) as usize], w, h, other_depth, 16),
            ),
            (
                "taller",
                png(
                    stem,
                    &vec![1; (w * (h + 8)) as usize],
                    w,
                    h + 8,
                    depth,
                    colors,
                ),
            ),
            (
                "wider",
                png(
                    stem,
                    &vec![1; ((w + 8) * h) as usize],
                    w + 8,
                    h,
                    depth,
                    colors,
                ),
            ),
            (
                "one colour fewer",
                png(stem, &source_pixels(stem), w, h, depth, colors - 1),
            ),
        ];
        for (name, bytes) in cases {
            write(&upstream, &file, bytes);
            let err = extract_sheet(&dir, sheet, &ball, &mut writer).unwrap_err();
            assert_eq!(mismatch_path(err), path, "{file} {name}");
        }
        if colors == 16 {
            // A 4-bit PNG cannot embed more than 16 colours; the 8-bit
            // background can embed too many.
            continue;
        }
        write(
            &upstream,
            &file,
            png(stem, &source_pixels(stem), w, h, depth, colors + 1),
        );
        let err = extract_sheet(&dir, sheet, &ball, &mut writer).unwrap_err();
        assert_eq!(mismatch_path(err), path, "{file} one colour more");

        assert_eq!(writer.len(), 0, "{file}");
        write(&upstream, &file, sheet_png(stem));
        extract_sheet(&dir, sheet, &ball, &mut writer).unwrap();
        assert_eq!(writer.len(), 2, "{file}");
    }
    std::fs::remove_dir_all(upstream).unwrap();
}

#[test]
fn the_small_ball_publishes_an_image_and_no_palette_when_its_colours_match_the_ball() {
    let upstream = scratch_dir("small-ball");
    let dir = source_dir(&upstream);
    let small = SHEETS.iter().find(|s| s.stem == "pokeball_small").unwrap();
    write(&upstream, "pokeball_small.png", sheet_png("pokeball_small"));
    let mut writer = PackWriter::new();
    let colors = extract_sheet(&dir, small, &ball_colors(), &mut writer).unwrap();
    assert_eq!(colors, ball_colors());
    let pack = writer.finish().unwrap();
    let entries = pack_format::parse_directory(&pack).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, "party-menu/image/pokeball_small");
    assert_eq!(
        entries[0].kind,
        EntryKind::Image {
            width: 16,
            height: 96,
            bit_depth: 4
        }
    );
    // The raster is kept whole and in row order, not cut into tiles.
    assert_eq!(payload(&pack, &entries[0]), source_pixels("pokeball_small"));
    std::fs::remove_dir_all(upstream).unwrap();
}

#[test]
fn the_small_ball_is_rejected_when_one_colour_differs_from_the_ball() {
    let upstream = scratch_dir("small-ball-colour");
    let dir = source_dir(&upstream);
    let small = SHEETS.iter().find(|s| s.stem == "pokeball_small").unwrap();
    let (_, width, height, depth, colors, _) = shape("pokeball_small");
    let pixels = source_pixels("pokeball_small");
    for changed in [0, 7, colors - 1] {
        let mut plte = plte(colors, seed("pokeball"));
        plte[changed * 3 + 1] ^= 8;
        let mut raw = Vec::new();
        for row in pixels.chunks(width as usize) {
            raw.push(0);
            raw.extend(row.chunks(2).map(|pair| (pair[0] << 4) | pair[1]));
        }
        let bytes = indexed_png_from_raw_with_plte(depth, width, height, &raw, &plte);
        let path = write(&upstream, "pokeball_small.png", bytes);
        let mut writer = PackWriter::new();
        let err = extract_sheet(&dir, small, &ball_colors(), &mut writer).unwrap_err();
        assert_eq!(mismatch_path(err), path, "colour {changed}");
        assert_eq!(writer.len(), 0);
    }
    std::fs::remove_dir_all(upstream).unwrap();
}

#[test]
fn maps_of_the_wrong_length_are_rejected() {
    let upstream = scratch_dir("map-length");
    let dir = source_dir(&upstream);
    let mut writer = PackWriter::new();
    for (stem, expected) in MAP_LENGTHS {
        for len in [0, expected - 1, expected + 1, expected * 2] {
            let path = write(&upstream, &format!("{stem}.bin"), vec![0; len]);
            let err = extract_map(&dir, stem, expected, &mut read_file, &mut writer).unwrap_err();
            assert_eq!(mismatch_path(err), path, "{stem} {len} bytes");
        }
    }
    assert_eq!(writer.len(), 0);
    std::fs::remove_dir_all(upstream).unwrap();
}

#[test]
fn a_map_entry_holds_the_validated_bytes() {
    let expected = map_bytes("slot_wide");
    let mut reads = 0;
    let mut read = |_: &Path| {
        reads += 1;
        // Any read after the first sees a different, wrong-length file.
        Ok(if reads == 1 {
            expected.clone()
        } else {
            vec![0xFF; 53]
        })
    };
    let mut writer = PackWriter::new();
    extract_map(
        Path::new("upstream"),
        "slot_wide",
        54,
        &mut read,
        &mut writer,
    )
    .unwrap();
    assert_eq!(reads, 1);
    let pack = writer.finish().unwrap();
    let entries = pack_format::parse_directory(&pack).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, "party-menu/raw/slot_wide");
    assert_eq!(payload(&pack, &entries[0]), expected.as_slice());
}

#[test]
fn extraction_is_deterministic() {
    let upstream = scratch_dir("determinism");
    write_valid_checkout(&upstream);
    assert_eq!(extracted(&upstream), extracted(&upstream));
    std::fs::remove_dir_all(upstream).unwrap();
}

#[test]
#[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
fn the_real_checkout_yields_the_whole_family() {
    assert!(upstream_present(), "run ./init.sh first");
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-extract-test-party-menu-{}.pack",
        std::process::id()
    ));
    extract_to(&path).expect("extraction should succeed against a real checkout");
    let pack = std::fs::read(&path).unwrap();
    let entries = pack_format::parse_directory(&pack).unwrap();
    let family: Vec<_> = entries
        .iter()
        .filter(|e| e.id.starts_with("party-menu/"))
        .collect();
    assert_eq!(
        family
            .iter()
            .map(|e| e.id.as_str())
            .collect::<BTreeSet<_>>(),
        EXPECTED_IDS.into_iter().collect::<BTreeSet<_>>()
    );

    let upstream = repo_root().join("pokeemerald").join(PARTY_MENU_DIRECTORY);
    let source = |stem: &str| {
        let bytes = std::fs::read(upstream.join(format!("{stem}.png"))).unwrap();
        (
            crate::extract::png::decode(&bytes).unwrap(),
            crate::extract::png::decode_palette(&bytes).unwrap(),
        )
    };
    let by_id = |id: &str| *family.iter().find(|e| e.id == id).unwrap();

    // Sheets: upstream's built sizes (1,984 / 1,024 / 64 / 768 tile bytes).
    for (stem, _, _, _, colors, (w, h)) in SHEET_SHAPES {
        let entry = by_id(&format!("party-menu/image/{stem}"));
        assert_eq!(
            entry.kind,
            EntryKind::Image {
                width: w,
                height: h,
                bit_depth: 4
            },
            "{stem}"
        );
        let (image, palette) = source(stem);
        assert_eq!(palette.len(), colors, "{stem}");
        let expected: Vec<u8> = if stem == "bg" {
            leading_tiles(&image.pixels, 64, 62)
        } else {
            image.pixels.clone()
        };
        assert_eq!(payload(&pack, entry), expected.as_slice(), "{stem}");
        let tiles = pack_format::tiles_from_image(payload(&pack, entry), 4, w, h, None).unwrap();
        let tile_bytes = match stem {
            "bg" => 1_984,
            "pokeball" => 1_024,
            "pokeball_small" => 768,
            _ => 64,
        };
        assert_eq!(tiles.len(), tile_bytes, "{stem}");
        if stem == "pokeball_small" {
            // It draws with the ball palette, which its PNG embeds unchanged.
            assert_eq!(palette, source("pokeball").1);
            assert!(!family
                .iter()
                .any(|e| e.id == "party-menu/palette/pokeball_small"));
            continue;
        }
        let palette_entry = by_id(&format!("party-menu/palette/{stem}"));
        assert_eq!(
            palette_entry.kind,
            EntryKind::Palette {
                color_count: u16::try_from(colors).unwrap()
            }
        );
        assert_eq!(palette_entry.length, colors * 2, "{stem}");
    }
    // The first palette entries, as BGR555.
    for (stem, first) in [
        ("bg", 0x3A6Fu16),
        ("pokeball", 0x6605),
        ("hold_icons", 0x3727),
    ] {
        let entry = by_id(&format!("party-menu/palette/{stem}"));
        assert_eq!(
            payload(&pack, entry)[..2],
            first.to_le_bytes(),
            "{stem} first colour"
        );
    }

    // Maps are the source bytes unchanged.
    for (stem, len) in MAP_LENGTHS {
        let entry = by_id(&format!("party-menu/raw/{stem}"));
        let source = std::fs::read(upstream.join(format!("{stem}.bin"))).unwrap();
        assert_eq!(source.len(), len, "{stem}");
        assert_eq!(payload(&pack, entry), source.as_slice(), "{stem}");
    }
    let _ = std::fs::remove_file(path);
}
