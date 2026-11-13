use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::super::png::tests::indexed_png_from_raw_with_plte;
use super::super::{extract_to, repo_root, upstream_present};
use super::{
    extract_map, extract_markings, extract_sheet, extract_summary_screen_graphics, pack_ids,
    SUMMARY_SCREEN_DIRECTORY,
};
use super::{MAPS, SHEETS};
use crate::extract::{read_file, ExtractError};
use pack_format::{DirectoryEntry, EntryKind, PackWriter};

/// Every id the summary screen's graphics publish, written out independently of
/// the manifest the extractor reads so that dropping an entry there fails here.
const EXPECTED_IDS: [&str; 16] = [
    "summary-screen-graphics/image/tiles",
    "summary-screen-graphics/image/move_select",
    "summary-screen-graphics/image/a_button",
    "summary-screen-graphics/image/b_button",
    "summary-screen-graphics/palette/tiles",
    "summary-screen-graphics/palette/move_select",
    "summary-screen-graphics/palette/markings",
    "summary-screen-graphics/raw/page_info",
    "summary-screen-graphics/raw/page_info_egg",
    "summary-screen-graphics/raw/page_skills",
    "summary-screen-graphics/raw/page_battle_moves",
    "summary-screen-graphics/raw/page_contest_moves",
    "summary-screen-graphics/raw/effect_battle",
    "summary-screen-graphics/raw/effect_contest",
    "summary-screen-graphics/raw/effect_cancel",
    "summary-screen-graphics/raw/status_tilemap",
];

/// Source `(stem, width, height, PNG bit depth, PLTE colours)`; the published
/// image is the same size at 4 bits. A button has no palette entry, so its PLTE
/// colours are only the file's own.
type SheetShape = (&'static str, u32, u32, u8, usize);

const SHEET_SHAPES: [SheetShape; 4] = [
    ("tiles", 128, 120, 8, 128),
    ("move_select", 16, 128, 4, 16),
    ("a_button", 16, 16, 4, 16),
    ("b_button", 16, 16, 4, 16),
];
const MAP_LENGTHS: [(&str, usize); 9] = [
    ("page_info", 2048),
    ("page_info_egg", 2048),
    ("page_skills", 2048),
    ("page_battle_moves", 2048),
    ("page_contest_moves", 2048),
    ("effect_battle", 160),
    ("effect_contest", 160),
    ("effect_cancel", 128),
    ("status_tilemap", 40),
];
/// Sheets whose PLTE becomes a palette entry.
const PALETTE_SHEETS: [&str; 2] = ["tiles", "move_select"];

fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pokeemerald-rs-summary-screen-graphics-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(SUMMARY_SCREEN_DIRECTORY)).unwrap();
    dir
}

fn source_dir(upstream: &Path) -> PathBuf {
    upstream.join(SUMMARY_SCREEN_DIRECTORY)
}

fn write(upstream: &Path, file: &str, bytes: impl AsRef<[u8]>) -> PathBuf {
    let path = source_dir(upstream).join(file);
    std::fs::write(&path, bytes).unwrap();
    path
}

fn shape(stem: &str) -> SheetShape {
    *SHEET_SHAPES.iter().find(|s| s.0 == stem).unwrap()
}

/// The source raster: every tile distinct and non-uniform, with indices up to
/// 127 in `tiles` so that narrowing matters.
fn source_pixels(stem: &str) -> Vec<u8> {
    let (_, width, height, depth, _) = shape(stem);
    (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .map(|(x, y)| {
            let tile = (y / 8) * (width / 8) + x / 8;
            let value = tile * 5 + (x % 8) + 3 * (y % 8) + 1;
            u8::try_from(if depth == 8 { value % 128 } else { value % 16 }).unwrap()
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

fn sheet_png(stem: &str) -> Vec<u8> {
    let (_, width, height, depth, colors) = shape(stem);
    png(stem, &source_pixels(stem), width, height, depth, colors)
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

/// A 16-colour JASC palette whose first channel is `seed`.
fn jasc(colors: usize, seed: u8) -> String {
    let header = format!("JASC-PAL\r\n0100\r\n{colors}\r\n");
    let rows = (0..colors).map(|index| {
        let red = seed.wrapping_add(u8::try_from(index).unwrap() * 8);
        format!("{red} 16 24\r\n")
    });
    header + &rows.collect::<String>()
}

fn jasc_colors(seed: u8) -> Vec<u8> {
    (0..16u8)
        .flat_map(|index| {
            let red = u16::from(seed.wrapping_add(index * 8));
            let color = (red >> 3) | ((16 >> 3) << 5) | ((24 >> 3) << 10);
            color.to_le_bytes()
        })
        .collect()
}

fn write_valid_checkout(upstream: &Path) {
    for (stem, ..) in SHEET_SHAPES {
        write(upstream, &format!("{stem}.png"), sheet_png(stem));
    }
    write(upstream, "markings.pal", jasc(16, 40));
    for (stem, _) in MAP_LENGTHS {
        write(upstream, &format!("{stem}.bin"), map_bytes(stem));
    }
}

fn source_files() -> Vec<String> {
    let sheets = SHEET_SHAPES.iter().map(|s| format!("{}.png", s.0));
    let maps = MAP_LENGTHS.iter().map(|m| format!("{}.bin", m.0));
    sheets
        .chain(["markings.pal".to_string()])
        .chain(maps)
        .collect()
}

fn mismatch_path(err: ExtractError) -> PathBuf {
    match err {
        ExtractError::SummaryScreenGraphicsAssetMismatch { path, .. } => path,
        other => panic!("expected a summary-screen-graphics mismatch, got {other}"),
    }
}

fn payload<'a>(pack: &'a [u8], entry: &DirectoryEntry) -> &'a [u8] {
    &pack[entry.offset..entry.offset + entry.length]
}

fn extracted(upstream: &Path) -> Vec<u8> {
    let mut writer = PackWriter::new();
    extract_summary_screen_graphics(upstream, &mut writer).unwrap();
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

#[test]
fn the_inventory_is_the_sixteen_listed_ids_with_no_button_palette() {
    let ids = pack_ids();
    assert_eq!(ids, EXPECTED_IDS);
    assert_eq!(ids.iter().collect::<BTreeSet<_>>().len(), 16);
    assert_eq!(
        SHEETS.iter().map(|s| s.stem).collect::<Vec<_>>(),
        ["tiles", "move_select", "a_button", "b_button"]
    );
    assert_eq!(MAPS, MAP_LENGTHS);
    assert!(!ids
        .iter()
        .any(|id| id.ends_with("_button") && id.contains("/palette/")));
}

#[test]
fn a_complete_checkout_yields_every_entry_with_its_shape_and_payload() {
    let upstream = scratch_dir("valid");
    write_valid_checkout(&upstream);
    // Unlisted neighbours must not change the pack and are never opened.
    write(&upstream, "tiles.4bpp", b"built");
    write(&upstream, "tiles.gbapal", b"built");
    write(&upstream, "stray.bin", b"stray");
    write(&upstream, "stray.png", b"not a png");

    let pack = extracted(&upstream);
    let entries = pack_format::parse_directory(&pack).unwrap();
    assert_eq!(entries.len(), 16);
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
                let (_, w, h, ..) = shape(stem);
                assert_eq!((*width, *height, *bit_depth), (w, h, 4), "{}", entry.id);
                assert_eq!(entry.length, (w * h) as usize, "{}", entry.id);
                let expected: Vec<u8> = source_pixels(stem).iter().map(|p| p & 0x0F).collect();
                assert_eq!(payload(&pack, entry), expected.as_slice(), "{}", entry.id);
            }
            EntryKind::Palette { color_count } if stem == "markings" => {
                assert_eq!(*color_count, 16);
                assert_eq!(
                    payload(&pack, entry),
                    jasc_colors(40).as_slice(),
                    "{}",
                    entry.id
                );
            }
            EntryKind::Palette { color_count } => {
                let (.., colors) = shape(stem);
                assert!(PALETTE_SHEETS.contains(&stem), "{}", entry.id);
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
fn the_tile_sheet_is_narrowed_whole_and_keeps_all_eight_palette_banks() {
    let upstream = scratch_dir("tiles-narrowed");
    write_valid_checkout(&upstream);
    let pack = extracted(&upstream);
    let entries = pack_format::parse_directory(&pack).unwrap();
    let find = |id: &str| entries.iter().find(|e| e.id == id).unwrap();

    let sheet = find("summary-screen-graphics/image/tiles");
    let pixels = payload(&pack, sheet);
    assert_eq!(pixels.len(), 128 * 120);
    assert!(pixels.iter().all(|&p| p < 16));
    let source = source_pixels("tiles");
    assert!(source.iter().any(|&p| p >= 16), "narrowing must matter");
    // 0x10, 0x49 and 0x7F narrow to 0, 9 and 15 without dropping any tile.
    for (wide, narrow) in [(0x10u8, 0u8), (0x49, 9), (0x7F, 15)] {
        let at = source.iter().position(|&p| p == wide).unwrap();
        assert_eq!(pixels[at], narrow, "{wide:#x}");
    }
    // All 240 tiles pack back to upstream's 7,680 built bytes.
    let tiles = pack_format::tiles_from_image(pixels, 4, 128, 120, None).unwrap();
    assert_eq!(tiles.len(), 240 * 32);

    let palette = find("summary-screen-graphics/palette/tiles");
    assert_eq!(palette.kind, EntryKind::Palette { color_count: 128 });
    assert_eq!(palette.length, 256);
    std::fs::remove_dir_all(upstream).unwrap();
}

#[test]
fn a_missing_source_fails_extraction_naming_that_file() {
    for file in source_files() {
        let upstream = scratch_dir("missing-one");
        write_valid_checkout(&upstream);
        std::fs::remove_file(source_dir(&upstream).join(&file)).unwrap();
        let err = extract_summary_screen_graphics(&upstream, &mut PackWriter::new()).unwrap_err();
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
        let (stem, w, h, depth, colors) = shape;
        let file = format!("{stem}.png");
        let path = write(&upstream, &file, b"not a png");
        let mut writer = PackWriter::new();
        let err = extract_sheet(&dir, sheet, &mut writer).unwrap_err();
        assert!(
            matches!(&err, ExtractError::Png(p, _) if *p == path),
            "{file}"
        );

        let other_depth = if depth == 8 { 4 } else { 8 };
        let mut cases: Vec<(&str, Vec<u8>)> = vec![
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
        ];
        if PALETTE_SHEETS.contains(&stem) {
            cases.push((
                "one colour fewer",
                png(stem, &source_pixels(stem), w, h, depth, colors - 1),
            ));
        }
        for (name, bytes) in cases {
            write(&upstream, &file, bytes);
            let err = extract_sheet(&dir, sheet, &mut writer).unwrap_err();
            assert_eq!(mismatch_path(err), path, "{file} {name}");
        }
        if depth == 8 {
            // A 4-bit PNG cannot embed more than 16 colours; the 8-bit
            // tile sheet can embed too many.
            write(
                &upstream,
                &file,
                png(stem, &source_pixels(stem), w, h, depth, colors + 1),
            );
            let err = extract_sheet(&dir, sheet, &mut writer).unwrap_err();
            assert_eq!(mismatch_path(err), path, "{file} one colour more");
        }

        assert_eq!(writer.len(), 0, "{file}");
        write(&upstream, &file, sheet_png(stem));
        extract_sheet(&dir, sheet, &mut writer).unwrap();
        let published = if PALETTE_SHEETS.contains(&stem) { 2 } else { 1 };
        assert_eq!(writer.len(), published, "{file}");
    }
    std::fs::remove_dir_all(upstream).unwrap();
}

#[test]
fn the_markings_palette_is_the_pal_file_of_exactly_sixteen_wellformed_colours() {
    let upstream = scratch_dir("markings");
    let dir = source_dir(&upstream);
    let mut writer = PackWriter::new();

    let path = write(&upstream, "markings.pal", "not a palette");
    let err = extract_markings(&dir, &mut writer).unwrap_err();
    assert!(matches!(err, ExtractError::Pal(p, _) if p == path));

    for colors in [15, 17, 32] {
        write(&upstream, "markings.pal", jasc(colors, 8));
        let err = extract_markings(&dir, &mut writer).unwrap_err();
        assert_eq!(mismatch_path(err), path, "{colors} colours");
    }
    assert_eq!(writer.len(), 0);

    write(&upstream, "markings.pal", jasc(16, 8));
    extract_markings(&dir, &mut writer).unwrap();
    assert_eq!(writer.len(), 1);
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
    let expected = map_bytes("effect_cancel");
    let mut reads = 0;
    let mut read = |_: &Path| {
        reads += 1;
        // Any read after the first sees a different, wrong-length file.
        Ok(if reads == 1 {
            expected.clone()
        } else {
            vec![0xFF; 127]
        })
    };
    let mut writer = PackWriter::new();
    extract_map(
        Path::new("upstream"),
        "effect_cancel",
        128,
        &mut read,
        &mut writer,
    )
    .unwrap();
    assert_eq!(reads, 1);
    let pack = writer.finish().unwrap();
    let entries = pack_format::parse_directory(&pack).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, "summary-screen-graphics/raw/effect_cancel");
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
        "pokeemerald-rs-extract-test-summary-screen-graphics-{}.pack",
        std::process::id()
    ));
    extract_to(&path).expect("extraction should succeed against a real checkout");
    let pack = std::fs::read(&path).unwrap();
    let entries = pack_format::parse_directory(&pack).unwrap();
    let family: Vec<_> = entries
        .iter()
        .filter(|e| e.id.starts_with("summary-screen-graphics/"))
        .collect();
    assert_eq!(
        family
            .iter()
            .map(|e| e.id.as_str())
            .collect::<BTreeSet<_>>(),
        EXPECTED_IDS.into_iter().collect::<BTreeSet<_>>()
    );

    let upstream = repo_root()
        .join("pokeemerald")
        .join(SUMMARY_SCREEN_DIRECTORY);
    let by_id = |id: &str| *family.iter().find(|e| e.id == id).unwrap();

    // Sheets: upstream's built sizes (7,680 / 1,024 / 128 / 128 tile bytes).
    for (stem, w, h, depth, colors) in SHEET_SHAPES {
        let bytes = std::fs::read(upstream.join(format!("{stem}.png"))).unwrap();
        let image = crate::extract::png::decode(&bytes).unwrap();
        assert_eq!((image.width, image.height, image.bit_depth), (w, h, depth));
        let palette = crate::extract::png::decode_palette(&bytes).unwrap();
        assert_eq!(palette.len(), colors, "{stem}");

        let entry = by_id(&format!("summary-screen-graphics/image/{stem}"));
        assert_eq!(
            entry.kind,
            EntryKind::Image {
                width: w,
                height: h,
                bit_depth: 4
            },
            "{stem}"
        );
        let expected: Vec<u8> = image.pixels.iter().map(|p| p & 0x0F).collect();
        assert_eq!(payload(&pack, entry), expected.as_slice(), "{stem}");
        let tiles = pack_format::tiles_from_image(payload(&pack, entry), 4, w, h, None).unwrap();
        let tile_bytes = match stem {
            "tiles" => 7_680,
            "move_select" => 1_024,
            _ => 128,
        };
        assert_eq!(tiles.len(), tile_bytes, "{stem}");

        if PALETTE_SHEETS.contains(&stem) {
            let palette_entry = by_id(&format!("summary-screen-graphics/palette/{stem}"));
            assert_eq!(
                palette_entry.kind,
                EntryKind::Palette {
                    color_count: u16::try_from(colors).unwrap()
                }
            );
            assert_eq!(palette_entry.length, colors * 2, "{stem}");
        }
    }
    // The first and last palette entries, as BGR555.
    let tiles_palette = payload(&pack, by_id("summary-screen-graphics/palette/tiles"));
    assert_eq!(tiles_palette[..2], 0x3AD6u16.to_le_bytes());
    assert_eq!(tiles_palette[254..], 0x7FFFu16.to_le_bytes());
    let move_select = payload(&pack, by_id("summary-screen-graphics/palette/move_select"));
    assert_eq!(move_select[..2], 0u16.to_le_bytes());
    let markings = by_id("summary-screen-graphics/palette/markings");
    assert_eq!(markings.kind, EntryKind::Palette { color_count: 16 });
    let colors = payload(&pack, markings);
    for (index, expected) in [0u16, 0x6E54, 0x7675, 0x7FFF].into_iter().enumerate() {
        assert_eq!(
            colors[index * 2..index * 2 + 2],
            expected.to_le_bytes(),
            "markings colour {index}"
        );
    }

    // Maps are the source bytes unchanged.
    for (stem, len) in MAP_LENGTHS {
        let entry = by_id(&format!("summary-screen-graphics/raw/{stem}"));
        let source = std::fs::read(upstream.join(format!("{stem}.bin"))).unwrap();
        assert_eq!(source.len(), len, "{stem}");
        assert_eq!(payload(&pack, entry), source.as_slice(), "{stem}");
    }
    let _ = std::fs::remove_file(path);
}
