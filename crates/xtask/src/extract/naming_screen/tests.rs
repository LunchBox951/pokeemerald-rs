use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::super::png::tests::indexed_png_from_raw_with_plte;
use super::super::scope::{naming_screen_ids, NAMING_SCREEN_DIRECTORY};
use super::super::{extract_to, repo_root, upstream_present};
use super::{extract_naming_screen, extract_palette, extract_sheet, extract_tilemap};
use crate::extract::scope::{NAMING_SCREEN_PALETTES, NAMING_SCREEN_SHEETS};
use crate::extract::{read_file, ExtractError};
use pack_format::{EntryKind, PackWriter};

/// Every id the naming screen publishes, written out independently of the
/// manifest the extractor reads so that dropping an entry there fails here.
const EXPECTED_IDS: [&str; 26] = [
    "naming-screen/image/menu",
    "naming-screen/image/page_swap_frame",
    "naming-screen/image/back_button",
    "naming-screen/image/ok_button",
    "naming-screen/image/page_swap_upper",
    "naming-screen/image/page_swap_lower",
    "naming-screen/image/page_swap_others",
    "naming-screen/image/cursor",
    "naming-screen/image/cursor_squished",
    "naming-screen/image/cursor_filled",
    "naming-screen/image/page_swap_button",
    "naming-screen/image/input_arrow",
    "naming-screen/image/underscore",
    "naming-screen/image/pc_icon_off",
    "naming-screen/image/pc_icon_on",
    "naming-screen/palette/menu",
    "naming-screen/palette/page_swap_upper",
    "naming-screen/palette/page_swap_lower",
    "naming-screen/palette/page_swap_others",
    "naming-screen/palette/buttons",
    "naming-screen/palette/cursor",
    "naming-screen/palette/keyboard",
    "naming-screen/raw/background",
    "naming-screen/raw/keyboard_upper",
    "naming-screen/raw/keyboard_lower",
    "naming-screen/raw/keyboard_symbols",
];

/// Every source file name, with the published shape of its image if it is one.
const IMAGE_SHAPES: [(&str, u32, u32); 15] = [
    ("menu", 24, 128),
    ("page_swap_frame", 40, 32),
    ("back_button", 40, 24),
    ("ok_button", 40, 24),
    ("page_swap_upper", 40, 8),
    ("page_swap_lower", 40, 8),
    ("page_swap_others", 40, 8),
    ("cursor", 8, 40),
    ("cursor_squished", 8, 40),
    ("cursor_filled", 8, 40),
    ("page_swap_button", 32, 16),
    ("input_arrow", 8, 8),
    ("underscore", 8, 8),
    ("pc_icon_off", 16, 24),
    ("pc_icon_on", 16, 24),
];
const PALETTE_STEMS: [&str; 7] = [
    "menu",
    "page_swap_upper",
    "page_swap_lower",
    "page_swap_others",
    "buttons",
    "cursor",
    "keyboard",
];
const TILEMAP_STEMS: [&str; 4] = [
    "background",
    "keyboard_upper",
    "keyboard_lower",
    "keyboard_symbols",
];
const CURSORS: [&str; 3] = ["cursor", "cursor_squished", "cursor_filled"];
const TILEMAP_BYTES: usize = 1280;

fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pokeemerald-rs-naming-screen-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(NAMING_SCREEN_DIRECTORY)).unwrap();
    dir
}

fn source_dir(upstream: &Path) -> PathBuf {
    upstream.join(NAMING_SCREEN_DIRECTORY)
}

fn source_files() -> Vec<String> {
    let images = IMAGE_SHAPES.iter().map(|(stem, ..)| format!("{stem}.png"));
    let palettes = PALETTE_STEMS.iter().map(|stem| format!("{stem}.pal"));
    let tilemaps = TILEMAP_STEMS.iter().map(|stem| format!("{stem}.bin"));
    images.chain(palettes).chain(tilemaps).collect()
}

/// The source sheet's index raster: a distinct non-zero pattern per stem,
/// except that a cursor's bottom-right tile (the one the build drops) is blank.
fn sheet_pixels(stem: &str, width: u32, height: u32) -> Vec<u8> {
    let seed = stem.bytes().map(u32::from).sum::<u32>();
    (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .map(|(x, y)| {
            if CURSORS.contains(&stem) && x >= 8 && y >= 16 {
                0
            } else {
                u8::try_from((x + 3 * y + seed) % 15 + 1).unwrap()
            }
        })
        .collect()
}

fn source_shape(stem: &str) -> (u32, u32) {
    let (_, width, height) = IMAGE_SHAPES.iter().find(|(s, ..)| *s == stem).unwrap();
    if CURSORS.contains(&stem) {
        (16, 24)
    } else {
        (*width, *height)
    }
}

fn png_from_pixels(bit_depth: u8, width: u32, height: u32, pixels: &[u8], plte: u8) -> Vec<u8> {
    let mut raw = Vec::new();
    for row in pixels.chunks(width as usize) {
        raw.push(0);
        if bit_depth == 4 {
            raw.extend(row.chunks(2).map(|pair| (pair[0] << 4) | pair[1]));
        } else {
            raw.extend_from_slice(row);
        }
    }
    let colors = if bit_depth == 4 { 16 } else { 256 };
    indexed_png_from_raw_with_plte(bit_depth, width, height, &raw, &vec![plte; colors * 3])
}

fn sheet_png(stem: &str) -> Vec<u8> {
    let (width, height) = source_shape(stem);
    png_from_pixels(4, width, height, &sheet_pixels(stem, width, height), 9)
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

fn palette_seed(stem: &str) -> u8 {
    8 * (1 + u8::try_from(PALETTE_STEMS.iter().position(|s| *s == stem).unwrap()).unwrap())
}

fn tilemap_bytes(stem: &str) -> Vec<u8> {
    let seed = TILEMAP_STEMS.iter().position(|s| *s == stem).unwrap();
    (0..TILEMAP_BYTES)
        .map(|i| u8::try_from((i + seed * 61) % 251).unwrap())
        .collect()
}

fn write(upstream: &Path, file: &str, bytes: impl AsRef<[u8]>) -> PathBuf {
    let path = source_dir(upstream).join(file);
    std::fs::write(&path, bytes).unwrap();
    path
}

fn write_valid_checkout(upstream: &Path) {
    for (stem, ..) in IMAGE_SHAPES {
        write(upstream, &format!("{stem}.png"), sheet_png(stem));
    }
    for stem in PALETTE_STEMS {
        write(
            upstream,
            &format!("{stem}.pal"),
            jasc(16, palette_seed(stem)),
        );
    }
    for stem in TILEMAP_STEMS {
        write(upstream, &format!("{stem}.bin"), tilemap_bytes(stem));
    }
}

fn mismatch_path(err: ExtractError) -> PathBuf {
    match err {
        ExtractError::NamingScreenAssetMismatch { path, .. } => path,
        other => panic!("expected a naming-screen mismatch, got {other}"),
    }
}

fn finished(writer: PackWriter) -> Vec<u8> {
    writer.finish().unwrap()
}

#[test]
fn the_inventory_is_the_twenty_six_listed_ids_with_all_four_tilemaps() {
    let ids = naming_screen_ids();
    assert_eq!(ids, EXPECTED_IDS);
    assert_eq!(ids.iter().collect::<BTreeSet<_>>().len(), 26);
    for stem in TILEMAP_STEMS {
        assert!(ids.contains(&format!("naming-screen/raw/{stem}")), "{stem}");
    }
    assert_eq!(ids.iter().filter(|id| id.contains("/image/")).count(), 15);
    assert_eq!(ids.iter().filter(|id| id.contains("/palette/")).count(), 7);
}

#[test]
fn the_manifest_keeps_the_upstream_unused_and_shared_walking_art_out() {
    let ids = naming_screen_ids().join("\n");
    assert!(!ids.contains("rival"));
    assert!(!ids.contains("brendan") && !ids.contains("may"));
    assert_eq!(NAMING_SCREEN_PALETTES, PALETTE_STEMS);
    assert_eq!(
        NAMING_SCREEN_SHEETS.map(|sheet| sheet.stem),
        IMAGE_SHAPES.map(|(stem, ..)| stem)
    );
}

#[test]
fn a_complete_checkout_yields_every_entry_with_its_kind_shape_and_payload() {
    let upstream = scratch_dir("valid");
    write_valid_checkout(&upstream);
    // Unlisted neighbours (upstream's unused rival palette, build outputs,
    // strays) must not change the pack.
    write(&upstream, "rival.pal", jasc(16, 200));
    write(&upstream, "menu.4bpp", b"built");
    write(&upstream, "stray.png", b"not a png");

    let mut writer = PackWriter::new();
    extract_naming_screen(&upstream, &mut writer).unwrap();
    assert_eq!(writer.len(), 26);
    let pack = finished(writer);
    let entries = pack_format::parse_directory(&pack).unwrap();
    let payload = |e: &pack_format::DirectoryEntry| pack[e.offset..e.offset + e.length].to_vec();

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
                let (_, w, h) = IMAGE_SHAPES.iter().find(|(s, ..)| *s == stem).unwrap();
                assert_eq!((*width, *height, *bit_depth), (*w, *h, 4), "{}", entry.id);
                assert_eq!(entry.length, (w * h) as usize, "{}", entry.id);
            }
            EntryKind::Palette { color_count } => {
                assert_eq!(*color_count, 16, "{}", entry.id);
                assert_eq!(entry.length, 32, "{}", entry.id);
                let expected: Vec<u8> = jasc_colors(palette_seed(stem));
                assert_eq!(payload(entry), expected, "{}", entry.id);
            }
            EntryKind::Raw => {
                assert!(entry.id.starts_with("naming-screen/raw/"));
                assert_eq!(payload(entry), tilemap_bytes(stem), "{}", entry.id);
            }
        }
    }
    std::fs::remove_dir_all(upstream).unwrap();
}

/// The BGR555 little-endian bytes `jasc(16, seed)` converts to.
fn jasc_colors(seed: u8) -> Vec<u8> {
    (0..16u8)
        .flat_map(|index| {
            let red = u16::from(seed.wrapping_add(index * 8));
            let color = (red >> 3) | ((16 >> 3) << 5) | ((24 >> 3) << 10);
            color.to_le_bytes()
        })
        .collect()
}

#[test]
fn the_palette_is_the_pal_file_not_the_png_plte() {
    let upstream = scratch_dir("pal-wins");
    write_valid_checkout(&upstream);
    // `menu.png` carries a PLTE (all 9s) that matches no JASC colour.
    let mut writer = PackWriter::new();
    extract_naming_screen(&upstream, &mut writer).unwrap();
    let pack = finished(writer);
    let entries = pack_format::parse_directory(&pack).unwrap();
    let menu = entries
        .iter()
        .find(|e| e.id == "naming-screen/palette/menu")
        .unwrap();
    assert_eq!(
        &pack[menu.offset..menu.offset + menu.length],
        jasc_colors(palette_seed("menu")).as_slice()
    );
    assert_eq!(
        entries
            .iter()
            .filter(|e| matches!(e.kind, EntryKind::Palette { .. }))
            .count(),
        7
    );
    std::fs::remove_dir_all(upstream).unwrap();
}

#[test]
fn a_missing_source_fails_extraction_naming_that_file() {
    for file in source_files() {
        let upstream = scratch_dir("missing-one");
        write_valid_checkout(&upstream);
        std::fs::remove_file(source_dir(&upstream).join(&file)).unwrap();
        let err = extract_naming_screen(&upstream, &mut PackWriter::new()).unwrap_err();
        assert!(
            matches!(&err, ExtractError::ReadFailed(path, _) if path.ends_with(&file)),
            "{file}: {err}"
        );
        std::fs::remove_dir_all(upstream).unwrap();
    }
}

#[test]
fn a_sheet_with_the_wrong_depth_or_size_is_rejected_before_it_is_queued() {
    let upstream = scratch_dir("sheet-shape");
    for sheet in NAMING_SCREEN_SHEETS {
        let file = format!("{}.png", sheet.stem);
        let mut writer = PackWriter::new();

        let path = write(&upstream, &file, b"not a png");
        let err = extract_sheet(&source_dir(&upstream), sheet, &mut writer).unwrap_err();
        assert!(
            matches!(&err, ExtractError::Png(p, _) if *p == path),
            "{file}"
        );

        let (w, h) = (sheet.width, sheet.height);
        let eight_bit = png_from_pixels(8, w, h, &vec![1; (w * h) as usize], 9);
        write(&upstream, &file, eight_bit);
        let err = extract_sheet(&source_dir(&upstream), sheet, &mut writer).unwrap_err();
        assert_eq!(mismatch_path(err), path, "{file} 8-bit");

        let taller = png_from_pixels(4, w, h + 8, &vec![1; (w * (h + 8)) as usize], 9);
        write(&upstream, &file, taller);
        let err = extract_sheet(&source_dir(&upstream), sheet, &mut writer).unwrap_err();
        assert_eq!(mismatch_path(err), path, "{file} taller");

        let wider = png_from_pixels(4, w + 8, h, &vec![1; ((w + 8) * h) as usize], 9);
        write(&upstream, &file, wider);
        let err = extract_sheet(&source_dir(&upstream), sheet, &mut writer).unwrap_err();
        assert_eq!(mismatch_path(err), path, "{file} wider");

        assert_eq!(writer.len(), 0, "{file}");
        write(&upstream, &file, sheet_png(sheet.stem));
        extract_sheet(&source_dir(&upstream), sheet, &mut writer).unwrap();
        assert_eq!(writer.len(), 1, "{file}");
    }
    std::fs::remove_dir_all(upstream).unwrap();
}

#[test]
fn a_cursor_is_its_first_five_tiles_stacked_in_tile_order() {
    let upstream = scratch_dir("cursor-cut");
    write_valid_checkout(&upstream);
    let mut writer = PackWriter::new();
    extract_naming_screen(&upstream, &mut writer).unwrap();
    let pack = finished(writer);
    let entries = pack_format::parse_directory(&pack).unwrap();

    for stem in CURSORS {
        let entry = entries
            .iter()
            .find(|e| e.id == format!("naming-screen/image/{stem}"))
            .unwrap();
        let strip = &pack[entry.offset..entry.offset + entry.length];
        assert_eq!(
            entry.kind,
            EntryKind::Image {
                width: 8,
                height: 40,
                bit_depth: 4
            }
        );
        assert_eq!(strip.len(), 320);

        let source = sheet_pixels(stem, 16, 24);
        let tile_pixels = |tile_x: usize, tile_y: usize| -> Vec<u8> {
            (0..8)
                .flat_map(|row| {
                    let start = (tile_y * 8 + row) * 16 + tile_x * 8;
                    source[start..start + 8].to_vec()
                })
                .collect()
        };
        let expected: Vec<u8> = [(0, 0), (1, 0), (0, 1), (1, 1), (0, 2)]
            .into_iter()
            .flat_map(|(x, y)| tile_pixels(x, y))
            .collect();
        assert_eq!(strip, expected.as_slice(), "{stem}");

        // The strip packs to the same bytes as the first 5 of the sheet's 6
        // built tiles (`-num_tiles 5`), and not to a raster-order truncation.
        let built = pack_format::tiles_from_image(&source, 4, 16, 24, None).unwrap();
        let recovered = pack_format::tiles_from_image(strip, 4, 8, 40, None).unwrap();
        assert_eq!(recovered.len(), 160);
        assert_eq!(recovered, built[..160], "{stem}");
        assert_ne!(strip, &source[..320], "{stem}");
    }
    std::fs::remove_dir_all(upstream).unwrap();
}

#[test]
fn a_cursor_with_colour_in_the_dropped_tile_is_rejected() {
    let upstream = scratch_dir("cursor-dropped-tile");
    let sheet = NAMING_SCREEN_SHEETS
        .into_iter()
        .find(|sheet| sheet.stem == "cursor")
        .unwrap();
    let mut pixels = sheet_pixels("cursor", 16, 24);
    *pixels.last_mut().unwrap() = 3;
    let path = write(
        &upstream,
        "cursor.png",
        png_from_pixels(4, 16, 24, &pixels, 9),
    );
    let mut writer = PackWriter::new();
    let err = extract_sheet(&source_dir(&upstream), sheet, &mut writer).unwrap_err();
    assert_eq!(mismatch_path(err), path);
    assert_eq!(writer.len(), 0);
    std::fs::remove_dir_all(upstream).unwrap();
}

#[test]
fn palettes_that_are_malformed_or_miscounted_are_rejected() {
    let upstream = scratch_dir("palette");
    let dir = source_dir(&upstream);
    let mut writer = PackWriter::new();

    let path = write(&upstream, "keyboard.pal", "not a palette");
    let err = extract_palette(&dir, "keyboard", &mut writer).unwrap_err();
    assert!(matches!(err, ExtractError::Pal(p, _) if p == path));

    for colors in [15, 17, 32] {
        write(&upstream, "keyboard.pal", jasc(colors, 8));
        let err = extract_palette(&dir, "keyboard", &mut writer).unwrap_err();
        assert_eq!(mismatch_path(err), path, "{colors} colours");
    }
    assert_eq!(writer.len(), 0);

    write(&upstream, "keyboard.pal", jasc(16, 8));
    extract_palette(&dir, "keyboard", &mut writer).unwrap();
    assert_eq!(writer.len(), 1);
    std::fs::remove_dir_all(upstream).unwrap();
}

#[test]
fn tilemaps_of_the_wrong_length_are_rejected() {
    let upstream = scratch_dir("tilemap-length");
    let dir = source_dir(&upstream);
    let mut writer = PackWriter::new();
    for stem in TILEMAP_STEMS {
        for len in [0, TILEMAP_BYTES - 1, TILEMAP_BYTES + 1] {
            let path = write(&upstream, &format!("{stem}.bin"), vec![0; len]);
            let err = extract_tilemap(&dir, stem, &mut read_file, &mut writer).unwrap_err();
            assert_eq!(mismatch_path(err), path, "{stem} {len} bytes");
        }
    }
    assert_eq!(writer.len(), 0);
    std::fs::remove_dir_all(upstream).unwrap();
}

#[test]
fn a_tilemap_entry_holds_the_validated_bytes() {
    let expected = tilemap_bytes("background");
    let mut reads = 0;
    let mut read = |_: &Path| {
        reads += 1;
        // Any read after the first sees a different, wrong-length file.
        Ok(if reads == 1 {
            expected.clone()
        } else {
            vec![0xFF; TILEMAP_BYTES - 1]
        })
    };
    let mut writer = PackWriter::new();
    extract_tilemap(Path::new("upstream"), "background", &mut read, &mut writer).unwrap();
    assert_eq!(reads, 1);
    let pack = finished(writer);
    let entries = pack_format::parse_directory(&pack).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, "naming-screen/raw/background");
    assert_eq!(
        &pack[entries[0].offset..entries[0].offset + entries[0].length],
        expected.as_slice()
    );
}

#[test]
fn extraction_is_deterministic() {
    let upstream = scratch_dir("determinism");
    write_valid_checkout(&upstream);
    let build = || {
        let mut writer = PackWriter::new();
        extract_naming_screen(&upstream, &mut writer).unwrap();
        finished(writer)
    };
    assert_eq!(build(), build());
    std::fs::remove_dir_all(upstream).unwrap();
}

#[test]
#[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
fn the_real_checkout_yields_the_whole_family() {
    assert!(upstream_present(), "run ./init.sh first");
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-extract-test-naming-screen-{}.pack",
        std::process::id()
    ));
    extract_to(&path).expect("extraction should succeed against a real checkout");
    let pack = std::fs::read(&path).unwrap();
    let entries = pack_format::parse_directory(&pack).unwrap();
    let naming: Vec<_> = entries
        .iter()
        .filter(|e| e.id.starts_with("naming-screen/"))
        .collect();
    assert_eq!(
        naming
            .iter()
            .map(|e| e.id.as_str())
            .collect::<BTreeSet<_>>(),
        EXPECTED_IDS.into_iter().collect::<BTreeSet<_>>()
    );

    let upstream = repo_root()
        .join("pokeemerald")
        .join(NAMING_SCREEN_DIRECTORY);
    for entry in naming {
        let stem = entry.id.rsplit('/').next().unwrap();
        let payload = &pack[entry.offset..entry.offset + entry.length];
        match &entry.kind {
            EntryKind::Image {
                width,
                height,
                bit_depth,
            } => {
                let (_, w, h) = IMAGE_SHAPES.iter().find(|(s, ..)| *s == stem).unwrap();
                assert_eq!((*width, *height, *bit_depth), (*w, *h, 4), "{}", entry.id);
                let source = crate::extract::png::decode(
                    &std::fs::read(upstream.join(format!("{stem}.png"))).unwrap(),
                )
                .unwrap();
                if CURSORS.contains(&stem) {
                    // The built `.4bpp` is the sheet's first five tiles.
                    let built =
                        pack_format::tiles_from_image(&source.pixels, 4, 16, 24, None).unwrap();
                    let recovered = pack_format::tiles_from_image(payload, 4, 8, 40, None).unwrap();
                    assert_eq!(recovered, built[..160], "{stem}");
                } else {
                    assert_eq!(payload, source.pixels.as_slice(), "{stem}");
                }
            }
            EntryKind::Palette { color_count } => {
                assert_eq!(*color_count, 16, "{}", entry.id);
            }
            EntryKind::Raw => {
                let source = std::fs::read(upstream.join(format!("{stem}.bin"))).unwrap();
                assert_eq!(payload, source.as_slice(), "{stem}");
            }
        }
    }
    let _ = std::fs::remove_file(path);
}
