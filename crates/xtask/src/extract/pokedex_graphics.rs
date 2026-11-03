//! Extracts the fixed Pokédex screen graphics manifest (upstream
//! `graphics/pokedex/`).
//!
//! The manifest names every one of the 35 source files the typed
//! `assets::PokedexGraphicsHandle` reads, so a missing file, a neighbour the
//! handle never reads, or an optional source cannot change the pack.

use std::path::Path;

use super::{build_image_entry, build_palette_entry, jasc_pal, png, push_raw_bytes};
use super::{read_file, read_text, ExtractError};
use pack_format::PackWriter;

const DIRECTORY: &str = "graphics/pokedex";
const IMAGE_PREFIX: &str = "pokedex-graphics/image";
const IMAGE_PALETTE_PREFIX: &str = "pokedex-graphics/image-palette";
const FILE_PALETTE_PREFIX: &str = "pokedex-graphics/palette";
const RAW_PREFIX: &str = "pokedex-graphics/raw";

const BANK_COLORS: usize = 16;
const FULL_PALETTE_COLORS: usize = 256;
const TILEMAP_ENTRY_BYTES: usize = 2;

/// An indexed sheet whose palette is the `PLTE` chunk of its own PNG.
#[derive(Clone, Copy, Debug)]
struct SheetSource {
    name: &'static str,
    width: u32,
    height: u32,
    colors: usize,
}

impl SheetSource {
    const fn bit_depth(self) -> u8 {
        if self.colors == FULL_PALETTE_COLORS {
            8
        } else {
            4
        }
    }
}

/// A standalone JASC palette upstream loads beside the sheets.
#[derive(Clone, Copy, Debug)]
struct PaletteSource {
    name: &'static str,
    colors: usize,
}

/// A raw file copied verbatim; `file` keeps the extensionless `cry_screen_bg`.
#[derive(Clone, Copy, Debug)]
struct RawSource {
    name: &'static str,
    file: &'static str,
    byte_len: usize,
}

const fn sheet(name: &'static str, width: u32, height: u32, colors: usize) -> SheetSource {
    SheetSource {
        name,
        width,
        height,
        colors,
    }
}

const fn tilemap(name: &'static str, file: &'static str, width: usize, height: usize) -> RawSource {
    RawSource {
        name,
        file,
        byte_len: width * height * TILEMAP_ENTRY_BYTES,
    }
}

/// Includes the upstream-unused `noball_unused` and the unreferenced `arrows`.
const SHEETS: [SheetSource; 14] = [
    sheet("menu", 128, 128, BANK_COLORS),
    sheet("interface", 32, 512, BANK_COLORS),
    sheet("noball_unused", 8, 32, BANK_COLORS),
    sheet("caught_ball", 8, 16, BANK_COLORS),
    sheet("arrows", 8, 96, BANK_COLORS),
    sheet("search_menu", 128, 64, BANK_COLORS),
    sheet("area_glow", 32, 96, BANK_COLORS),
    sheet("area_marker", 16, 16, BANK_COLORS),
    sheet("area_unknown", 32, 96, BANK_COLORS),
    sheet("region_map", 128, 120, FULL_PALETTE_COLORS),
    sheet("region_map_affine", 128, 120, FULL_PALETTE_COLORS),
    sheet("cry_meter", 80, 64, BANK_COLORS),
    sheet("cry_meter_needle", 64, 64, BANK_COLORS),
    sheet("cry_screen_bg", 8, 8, BANK_COLORS),
];

const PALETTES: [PaletteSource; 6] = [
    PaletteSource {
        name: "bg_hoenn",
        colors: 96,
    },
    PaletteSource {
        name: "bg_national",
        colors: 96,
    },
    PaletteSource {
        name: "search_results_bg",
        colors: 96,
    },
    PaletteSource {
        name: "search_menu",
        colors: 64,
    },
    PaletteSource {
        name: "region_map",
        colors: 48,
    },
    PaletteSource {
        name: "size_silhouette",
        colors: 16,
    },
];

/// Includes the upstream-unused `cry_meter_map` and the tracked but
/// unreferenced extensionless `cry_screen_bg`, a 32-byte file.
const RAW_FILES: [RawSource; 15] = [
    tilemap("start_menu_main", "start_menu_main.bin", 32, 12),
    tilemap(
        "start_menu_search_results",
        "start_menu_search_results.bin",
        32,
        12,
    ),
    tilemap("list", "list.bin", 32, 20),
    tilemap("list_underlay", "list_underlay.bin", 32, 20),
    tilemap("info_screen", "info_screen.bin", 32, 20),
    tilemap("size_screen", "size_screen.bin", 32, 20),
    tilemap(
        "screen_select_bar_main",
        "screen_select_bar_main.bin",
        32,
        3,
    ),
    tilemap(
        "screen_select_bar_submenu",
        "screen_select_bar_submenu.bin",
        32,
        3,
    ),
    tilemap("search_menu_hoenn", "search_menu_hoenn.bin", 32, 20),
    tilemap("search_menu_national", "search_menu_national.bin", 32, 20),
    tilemap("region_map", "region_map.bin", 32, 32),
    RawSource {
        name: "region_map_affine",
        file: "region_map_affine.bin",
        byte_len: 64 * 64,
    },
    tilemap("cry_screen", "cry_screen.bin", 32, 20),
    tilemap("cry_meter_map", "cry_meter_map.bin", 10, 8),
    RawSource {
        name: "cry_screen_bg",
        file: "cry_screen_bg",
        byte_len: 32,
    },
];

/// Extracts the Pokédex graphics manifest: 14 sheets with their embedded
/// palettes, six JASC palettes, and 15 raw files.
pub(super) fn extract_pokedex_graphics(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    for source in SHEETS {
        extract_sheet(upstream, source, writer)?;
    }
    for source in PALETTES {
        extract_palette(upstream, source, writer)?;
    }
    for source in RAW_FILES {
        extract_raw(upstream, source, &mut read_file, writer)?;
    }
    Ok(())
}

fn extract_sheet(
    upstream: &Path,
    source: SheetSource,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let path = upstream
        .join(DIRECTORY)
        .join(format!("{}.png", source.name));
    let bytes = read_file(&path)?;
    let image = png::decode(&bytes).map_err(|e| ExtractError::Png(path.clone(), e))?;
    if image.bit_depth != source.bit_depth() {
        return Err(mismatch(
            &path,
            format!(
                "is {}-bit: expected {}-bit",
                image.bit_depth,
                source.bit_depth()
            ),
        ));
    }
    if image.width != source.width || image.height != source.height {
        return Err(mismatch(
            &path,
            format!(
                "is {}x{}: expected {}x{}",
                image.width, image.height, source.width, source.height
            ),
        ));
    }
    if image.palette.len() != source.colors {
        return Err(mismatch(
            &path,
            format!(
                "embeds {} palette colours: expected {}",
                image.palette.len(),
                source.colors
            ),
        ));
    }
    if let Some(&pixel) = image
        .pixels
        .iter()
        .find(|&&pixel| usize::from(pixel) >= image.palette.len())
    {
        return Err(mismatch(
            &path,
            format!(
                "has pixel index {pixel}: its palette only has {} colours",
                image.palette.len()
            ),
        ));
    }

    let colors: Vec<jasc_pal::Rgb888> = image
        .palette
        .iter()
        .map(|&[r, g, b]| jasc_pal::Rgb888 { r, g, b })
        .collect();
    writer.push(build_palette_entry(
        &path,
        &colors,
        format!("{IMAGE_PALETTE_PREFIX}/{}", source.name),
    )?);
    writer.push(build_image_entry(
        &path,
        format!("{IMAGE_PREFIX}/{}", source.name),
        image,
    )?);
    Ok(())
}

fn extract_palette(
    upstream: &Path,
    source: PaletteSource,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let path = upstream
        .join(DIRECTORY)
        .join(format!("{}.pal", source.name));
    let text = read_text(&path)?;
    let colors = jasc_pal::parse(&text).map_err(|e| ExtractError::Pal(path.clone(), e))?;
    if colors.len() != source.colors {
        return Err(mismatch(
            &path,
            format!("has {} colours: expected {}", colors.len(), source.colors),
        ));
    }
    writer.push(build_palette_entry(
        &path,
        &colors,
        format!("{FILE_PALETTE_PREFIX}/{}", source.name),
    )?);
    Ok(())
}

/// The buffer validated is the buffer queued: a second read of the path
/// could publish bytes that changed after the length check (#2105).
fn extract_raw(
    upstream: &Path,
    source: RawSource,
    read: &mut dyn FnMut(&Path) -> Result<Vec<u8>, ExtractError>,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let path = upstream.join(DIRECTORY).join(source.file);
    let payload = read(&path)?;
    if payload.len() != source.byte_len {
        return Err(mismatch(
            &path,
            format!("is {} bytes: expected {}", payload.len(), source.byte_len),
        ));
    }
    push_raw_bytes(format!("{RAW_PREFIX}/{}", source.name), payload, writer);
    Ok(())
}

fn mismatch(path: &Path, detail: String) -> ExtractError {
    ExtractError::PokedexGraphicsAssetMismatch {
        path: path.to_path_buf(),
        detail,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    use assets::AssetPack;

    use super::super::png::tests::indexed_png_from_raw_with_plte;
    use super::super::{extract_to, repo_root, upstream_present};
    use super::{
        extract_palette, extract_pokedex_graphics, extract_raw, extract_sheet, RawSource,
        SheetSource, DIRECTORY, PALETTES, RAW_FILES, SHEETS,
    };
    use crate::extract::ExtractError;
    use pack_format::PackWriter;

    const MANIFEST_ENTRY_COUNT: usize = 49;
    const SOURCE_FILE_COUNT: usize = 35;

    fn manifest_ids() -> Vec<String> {
        SHEETS
            .iter()
            .flat_map(|s| {
                [
                    format!("pokedex-graphics/image/{}", s.name),
                    format!("pokedex-graphics/image-palette/{}", s.name),
                ]
            })
            .chain(
                PALETTES
                    .iter()
                    .map(|p| format!("pokedex-graphics/palette/{}", p.name)),
            )
            .chain(
                RAW_FILES
                    .iter()
                    .map(|r| format!("pokedex-graphics/raw/{}", r.name)),
            )
            .collect()
    }

    fn source_files() -> Vec<String> {
        SHEETS
            .iter()
            .map(|s| format!("{}.png", s.name))
            .chain(PALETTES.iter().map(|p| format!("{}.pal", p.name)))
            .chain(RAW_FILES.iter().map(|r| r.file.to_owned()))
            .collect()
    }

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pokeemerald-rs-pokedex-graphics-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(DIRECTORY)).unwrap();
        dir
    }

    /// An indexed PNG whose every pixel is `pixel` and whose `PLTE` holds
    /// `colors` entries.
    fn png_bytes(bit_depth: u8, width: u32, height: u32, colors: usize, pixel: u8) -> Vec<u8> {
        let row = if bit_depth == 4 {
            vec![(pixel << 4) | pixel; width as usize / 2]
        } else {
            vec![pixel; width as usize]
        };
        let mut raw = Vec::new();
        for _ in 0..height {
            raw.push(0);
            raw.extend_from_slice(&row);
        }
        indexed_png_from_raw_with_plte(bit_depth, width, height, &raw, &vec![7; colors * 3])
    }

    fn sheet_png(source: SheetSource) -> Vec<u8> {
        png_bytes(
            source.bit_depth(),
            source.width,
            source.height,
            source.colors,
            0,
        )
    }

    fn jasc(colors: usize) -> String {
        let mut text = format!("JASC-PAL\r\n0100\r\n{colors}\r\n");
        for _ in 0..colors {
            text.push_str("8 16 24\r\n");
        }
        text
    }

    fn write_sheet(dir: &Path, source: SheetSource, bytes: &[u8]) -> PathBuf {
        let path = dir.join(DIRECTORY).join(format!("{}.png", source.name));
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn write_valid_checkout(dir: &Path) {
        for source in SHEETS {
            write_sheet(dir, source, &sheet_png(source));
        }
        for source in PALETTES {
            let path = dir.join(DIRECTORY).join(format!("{}.pal", source.name));
            std::fs::write(path, jasc(source.colors)).unwrap();
        }
        for source in RAW_FILES {
            let path = dir.join(DIRECTORY).join(source.file);
            std::fs::write(path, vec![0x5A; source.byte_len]).unwrap();
        }
    }

    fn mismatch_path(err: ExtractError) -> PathBuf {
        match err {
            ExtractError::PokedexGraphicsAssetMismatch { path, .. } => path,
            other => panic!("expected a pokedex-graphics mismatch, got {other}"),
        }
    }

    fn area_marker() -> SheetSource {
        SHEETS[7]
    }

    fn region_map() -> SheetSource {
        SHEETS[9]
    }

    #[test]
    fn manifest_names_49_unique_entries_from_35_unique_files() {
        let ids = manifest_ids();
        assert_eq!(ids.len(), MANIFEST_ENTRY_COUNT);
        assert_eq!(ids.iter().collect::<BTreeSet<_>>().len(), ids.len());
        let files = source_files();
        assert_eq!(files.len(), SOURCE_FILE_COUNT);
        assert_eq!(files.iter().collect::<BTreeSet<_>>().len(), files.len());
    }

    #[test]
    fn manifest_shapes_match_the_upstream_graphics() {
        let sheets: Vec<_> = SHEETS
            .iter()
            .map(|s| (s.name, s.width, s.height, s.colors))
            .collect();
        assert_eq!(
            sheets,
            [
                ("menu", 128, 128, 16),
                ("interface", 32, 512, 16),
                ("noball_unused", 8, 32, 16),
                ("caught_ball", 8, 16, 16),
                ("arrows", 8, 96, 16),
                ("search_menu", 128, 64, 16),
                ("area_glow", 32, 96, 16),
                ("area_marker", 16, 16, 16),
                ("area_unknown", 32, 96, 16),
                ("region_map", 128, 120, 256),
                ("region_map_affine", 128, 120, 256),
                ("cry_meter", 80, 64, 16),
                ("cry_meter_needle", 64, 64, 16),
                ("cry_screen_bg", 8, 8, 16),
            ]
        );
        let palettes: Vec<_> = PALETTES.iter().map(|p| (p.name, p.colors)).collect();
        assert_eq!(
            palettes,
            [
                ("bg_hoenn", 96),
                ("bg_national", 96),
                ("search_results_bg", 96),
                ("search_menu", 64),
                ("region_map", 48),
                ("size_silhouette", 16),
            ]
        );
        let raws: Vec<_> = RAW_FILES
            .iter()
            .map(|r| (r.name, r.file, r.byte_len))
            .collect();
        assert_eq!(
            raws,
            [
                ("start_menu_main", "start_menu_main.bin", 768),
                (
                    "start_menu_search_results",
                    "start_menu_search_results.bin",
                    768
                ),
                ("list", "list.bin", 1280),
                ("list_underlay", "list_underlay.bin", 1280),
                ("info_screen", "info_screen.bin", 1280),
                ("size_screen", "size_screen.bin", 1280),
                ("screen_select_bar_main", "screen_select_bar_main.bin", 192),
                (
                    "screen_select_bar_submenu",
                    "screen_select_bar_submenu.bin",
                    192
                ),
                ("search_menu_hoenn", "search_menu_hoenn.bin", 1280),
                ("search_menu_national", "search_menu_national.bin", 1280),
                ("region_map", "region_map.bin", 2048),
                ("region_map_affine", "region_map_affine.bin", 4096),
                ("cry_screen", "cry_screen.bin", 1280),
                ("cry_meter_map", "cry_meter_map.bin", 160),
                ("cry_screen_bg", "cry_screen_bg", 32),
            ]
        );
    }

    #[test]
    fn a_valid_manifest_yields_the_ids_the_typed_handle_reads() {
        let dir = scratch_dir("valid");
        write_valid_checkout(&dir);
        let mut writer = PackWriter::new();
        extract_pokedex_graphics(&dir, &mut writer).unwrap();
        assert_eq!(writer.len(), MANIFEST_ENTRY_COUNT);
        let pack_path = dir.join("pokedex.pack");
        std::fs::write(&pack_path, writer.finish().unwrap()).unwrap();

        let pack = AssetPack::load(&pack_path).unwrap();
        let handle = pack.pokedex_graphics().unwrap();
        assert_eq!(handle.cry_screen_bg_raw, [0x5A; 32]);

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn missing_sources_fail_instead_of_being_skipped() {
        let dir = scratch_dir("missing");
        let mut writer = PackWriter::new();
        let err = extract_pokedex_graphics(&dir, &mut writer).unwrap_err();
        assert!(matches!(err, ExtractError::ReadFailed(path, _) if path.starts_with(&dir)));
        assert_eq!(writer.len(), 0);

        let err = extract_palette(&dir, PALETTES[0], &mut writer).unwrap_err();
        assert!(matches!(err, ExtractError::ReadFailed(..)));
        let err = extract_raw(
            &dir,
            RAW_FILES[0],
            &mut crate::extract::read_file,
            &mut writer,
        )
        .unwrap_err();
        assert!(matches!(err, ExtractError::ReadFailed(..)));
        assert_eq!(writer.len(), 0);

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_missing_file_in_a_complete_checkout_fails_closed() {
        for file in source_files() {
            let dir = scratch_dir("missing-one");
            write_valid_checkout(&dir);
            std::fs::remove_file(dir.join(DIRECTORY).join(&file)).unwrap();
            let mut writer = PackWriter::new();
            let err = extract_pokedex_graphics(&dir, &mut writer).unwrap_err();
            assert!(
                matches!(&err, ExtractError::ReadFailed(path, _) if path.ends_with(&file)),
                "{file}: {err}"
            );
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn sheets_that_are_not_png_are_rejected() {
        let dir = scratch_dir("not-png");
        let path = write_sheet(&dir, area_marker(), b"not a png");
        let mut writer = PackWriter::new();
        let err = extract_sheet(&dir, area_marker(), &mut writer).unwrap_err();
        assert!(matches!(err, ExtractError::Png(error_path, _) if error_path == path));
        assert_eq!(writer.len(), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn sheets_of_the_wrong_geometry_are_rejected() {
        let dir = scratch_dir("geometry");
        let source = area_marker();
        let path = write_sheet(&dir, source, &png_bytes(4, 16, 8, 16, 0));
        let mut writer = PackWriter::new();
        let err = extract_sheet(&dir, source, &mut writer).unwrap_err();
        assert_eq!(mismatch_path(err), path);
        assert_eq!(writer.len(), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn sheets_of_the_wrong_bit_depth_are_rejected() {
        let dir = scratch_dir("bit-depth");
        let mut writer = PackWriter::new();
        let bank = area_marker();
        let path = write_sheet(&dir, bank, &png_bytes(8, 16, 16, 16, 0));
        assert_eq!(
            mismatch_path(extract_sheet(&dir, bank, &mut writer).unwrap_err()),
            path
        );
        let full = region_map();
        let path = write_sheet(&dir, full, &png_bytes(4, 128, 120, 16, 0));
        assert_eq!(
            mismatch_path(extract_sheet(&dir, full, &mut writer).unwrap_err()),
            path
        );
        assert_eq!(writer.len(), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn sheets_with_a_wrong_count_embedded_palette_are_rejected() {
        let dir = scratch_dir("embedded-count");
        let mut writer = PackWriter::new();
        let source = area_marker();
        for colors in [15, 17] {
            let path = write_sheet(&dir, source, &png_bytes(4, 16, 16, colors, 0));
            assert_eq!(
                mismatch_path(extract_sheet(&dir, source, &mut writer).unwrap_err()),
                path,
                "{colors} colours"
            );
        }
        assert_eq!(writer.len(), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn sheets_with_an_empty_embedded_palette_are_rejected() {
        let dir = scratch_dir("embedded-empty");
        let source = area_marker();
        let path = write_sheet(&dir, source, &png_bytes(4, 16, 16, 0, 0));
        let mut writer = PackWriter::new();
        let err = extract_sheet(&dir, source, &mut writer).unwrap_err();
        assert!(matches!(err, ExtractError::Png(error_path, _) if error_path == path));
        assert_eq!(writer.len(), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn sheets_with_a_pixel_outside_the_palette_are_rejected() {
        let dir = scratch_dir("pixel");
        let source = SheetSource {
            colors: 8,
            ..area_marker()
        };
        let mut writer = PackWriter::new();
        let path = write_sheet(&dir, source, &png_bytes(4, 16, 16, 8, 9));
        assert_eq!(
            mismatch_path(extract_sheet(&dir, source, &mut writer).unwrap_err()),
            path
        );
        assert_eq!(writer.len(), 0);

        write_sheet(&dir, source, &png_bytes(4, 16, 16, 8, 7));
        extract_sheet(&dir, source, &mut writer).unwrap();
        assert_eq!(writer.len(), 2);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn palette_files_that_are_malformed_or_miscounted_are_rejected() {
        let dir = scratch_dir("palette");
        let source = PALETTES[5];
        let path = dir.join(DIRECTORY).join("size_silhouette.pal");
        let mut writer = PackWriter::new();

        std::fs::write(&path, "not a palette").unwrap();
        let err = extract_palette(&dir, source, &mut writer).unwrap_err();
        assert!(matches!(err, ExtractError::Pal(error_path, _) if error_path == path));

        for colors in [15, 17] {
            std::fs::write(&path, jasc(colors)).unwrap();
            assert_eq!(
                mismatch_path(extract_palette(&dir, source, &mut writer).unwrap_err()),
                path
            );
        }
        assert_eq!(writer.len(), 0);

        std::fs::write(&path, jasc(16)).unwrap();
        extract_palette(&dir, source, &mut writer).unwrap();
        assert_eq!(writer.len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn raw_files_of_the_wrong_length_are_rejected() {
        let dir = scratch_dir("raw-length");
        for source in RAW_FILES {
            let path = dir.join(DIRECTORY).join(source.file);
            let mut writer = PackWriter::new();
            for len in [0, source.byte_len - 1, source.byte_len + 1] {
                std::fs::write(&path, vec![0; len]).unwrap();
                let err = extract_raw(&dir, source, &mut crate::extract::read_file, &mut writer)
                    .unwrap_err();
                assert_eq!(mismatch_path(err), path, "{} at {len}", source.file);
            }
            assert_eq!(writer.len(), 0);
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn raw_pack_entry_holds_the_validated_bytes() {
        for source in RAW_FILES {
            let expected: Vec<u8> = (0..=u8::MAX).cycle().take(source.byte_len).collect();
            let mut reads = 0;
            let mut read = |_: &Path| {
                reads += 1;
                // Any read after the first sees a different, wrong-length file.
                Ok(if reads == 1 {
                    expected.clone()
                } else {
                    vec![0xFF; source.byte_len - 1]
                })
            };
            let mut writer = PackWriter::new();
            extract_raw(Path::new("upstream"), source, &mut read, &mut writer).unwrap();
            assert_eq!(reads, 1);
            let pack = writer.finish().unwrap();
            let entries = pack_format::parse_directory(&pack).unwrap();
            assert_eq!(entries.len(), 1);
            assert_eq!(
                entries[0].id,
                format!("pokedex-graphics/raw/{}", source.name)
            );
            assert_eq!(
                &pack[entries[0].offset..entries[0].offset + entries[0].length],
                expected.as_slice()
            );
        }
    }

    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn pokedex_graphics_are_extracted() {
        assert!(upstream_present(), "run ./init.sh first");
        let path = std::env::temp_dir().join(format!(
            "pokeemerald-rs-extract-test-pokedex-graphics-{}.pack",
            std::process::id()
        ));
        extract_to(&path).expect("extraction should succeed against a real checkout");
        let pack = AssetPack::load(&path).unwrap();

        for source in SHEETS {
            let image = pack
                .image(&format!("pokedex-graphics/image/{}", source.name))
                .unwrap();
            assert_eq!((image.width, image.height), (source.width, source.height));
            let palette = pack
                .palette(&format!("pokedex-graphics/image-palette/{}", source.name))
                .unwrap();
            assert_eq!(usize::from(palette.color_count), source.colors);
        }
        for source in PALETTES {
            let palette = pack
                .palette(&format!("pokedex-graphics/palette/{}", source.name))
                .unwrap();
            assert_eq!(usize::from(palette.color_count), source.colors);
        }
        let upstream = repo_root().join("pokeemerald").join(DIRECTORY);
        for RawSource { name, file, .. } in RAW_FILES {
            let packed = pack.raw(&format!("pokedex-graphics/raw/{name}")).unwrap();
            assert_eq!(
                packed,
                std::fs::read(upstream.join(file)).unwrap(),
                "{file}"
            );
        }
        let handle = pack.pokedex_graphics().unwrap();
        assert_eq!(handle.cry_screen_bg_raw.len(), 32);
        let _ = std::fs::remove_file(path);
    }
}
