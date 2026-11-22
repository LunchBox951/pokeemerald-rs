use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use assets::AssetPack;
use pack_format::PackWriter;

use super::super::png::tests::indexed_png_from_raw_with_plte;
use super::super::scope::PC_WALLPAPER_DIRECTORY;
use super::super::{extract_to, jasc_pal, png, repo_root, upstream_present};
use super::extract_pc_wallpapers;
use crate::extract::ExtractError;

/// One style written out from `graphics_file_rules.mk`, independently of the
/// manifest the extractor reads: its directory, the PNG sizes, and the tiles
/// `-num_tiles` keeps of the frame and of the background.
struct Style {
    stem: &'static str,
    frame_size: (u32, u32),
    frame_tiles: u32,
    background_size: (u32, u32),
    background_tiles: u32,
}

const fn style(
    stem: &'static str,
    frame_size: (u32, u32),
    frame_tiles: u32,
    background_size: (u32, u32),
    background_tiles: u32,
) -> Style {
    Style {
        stem,
        frame_size,
        frame_tiles,
        background_size,
        background_tiles,
    }
}

/// `sWallpapers` entries 4 to 15 in order.
const STYLES: [Style; 12] = [
    style("crag", (128, 32), 49, (32, 32), 16),
    style("volcano", (128, 32), 56, (16, 104), 26),
    style("snow", (128, 32), 57, (16, 88), 22),
    style("cave", (128, 32), 55, (16, 16), 4),
    // `beach/bg.png` and `river/bg.png` are cut by their own `-num_tiles`.
    style("beach", (120, 32), 46, (24, 64), 23),
    style("seafloor", (128, 32), 54, (32, 16), 8),
    style("river", (128, 32), 51, (24, 32), 11),
    style("sky", (96, 32), 45, (32, 32), 16),
    style("polkadot", (128, 32), 54, (16, 24), 6),
    style("pokecenter", (80, 32), 35, (88, 16), 22),
    style("machine", (64, 40), 33, (16, 32), 8),
    style("plain", (24, 48), 18, (32, 8), 4),
];

const TILEMAP_BYTES: usize = 720;
const TILE_PIXELS: usize = 64;

impl Style {
    fn byte(index: usize) -> u8 {
        u8::try_from(4 + index).unwrap()
    }

    fn tiles(&self) -> u32 {
        self.frame_tiles + self.background_tiles
    }

    /// The pixel value of kept tile `tile` of a half: never zero, and
    /// different for the two halves and for neighbouring tiles.
    fn pixel(&self, half: Half, tile: u32) -> u8 {
        let seed =
            u32::try_from(self.stem.len()).unwrap() + if half == Half::Frame { 0 } else { 7 };
        u8::try_from((tile + seed) % 15 + 1).unwrap()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Half {
    Frame,
    Background,
}

fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pokeemerald-rs-pc-wallpaper-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn style_dir(upstream: &Path, style: &Style) -> PathBuf {
    upstream.join(PC_WALLPAPER_DIRECTORY).join(style.stem)
}

/// A 4-bit PNG laid out like the source art: kept tiles in row-major tile
/// order carry their tile's pixel value and every dropped tile is blank.
fn half_png(style: &Style, half: Half, size: (u32, u32), kept: u32, colors: usize) -> Vec<u8> {
    let (width, height) = size;
    let tiles_per_row = width / 8;
    let mut raw = Vec::new();
    for y in 0..height {
        raw.push(0);
        let row: Vec<u8> = (0..width)
            .map(|x| {
                let tile = (y / 8) * tiles_per_row + x / 8;
                if tile < kept {
                    style.pixel(half, tile)
                } else {
                    0
                }
            })
            .collect();
        raw.extend(row.chunks(2).map(|pair| pair[0] << 4 | pair[1]));
    }
    let shade = if half == Half::Frame { 8 } else { 16 };
    indexed_png_from_raw_with_plte(4, width, height, &raw, &vec![shade; colors * 3])
}

fn frame_png(style: &Style) -> Vec<u8> {
    half_png(style, Half::Frame, style.frame_size, style.frame_tiles, 16)
}

fn background_png(style: &Style) -> Vec<u8> {
    half_png(
        style,
        Half::Background,
        style.background_size,
        style.background_tiles,
        16,
    )
}

/// Every word names a sheet tile, alternates the two banks and mixes flips, so
/// the accessor accepts it and a wrong sheet length would fail it.
fn tilemap_bytes(style: &Style) -> Vec<u8> {
    (0..TILEMAP_BYTES / 2)
        .flat_map(|cell| {
            let tile = u16::try_from(cell % style.tiles() as usize).unwrap();
            let flips = u16::try_from(cell % 4).unwrap() << 10;
            let bank = u16::try_from(1 + cell % 2).unwrap() << 12;
            (tile | flips | bank).to_le_bytes()
        })
        .collect()
}

fn write(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

fn write_valid_checkout(upstream: &Path) {
    for style in &STYLES {
        let dir = style_dir(upstream, style);
        write(&dir.join("frame.png"), &frame_png(style));
        write(&dir.join("bg.png"), &background_png(style));
        write(&dir.join("tilemap.bin"), &tilemap_bytes(style));
    }
}

fn extract(upstream: &Path) -> (Result<(), ExtractError>, usize) {
    let mut writer = PackWriter::new();
    let result = extract_pc_wallpapers(upstream, &mut writer);
    (result, writer.len())
}

fn mismatch_path(err: ExtractError) -> PathBuf {
    match err {
        ExtractError::PcWallpaperAssetMismatch { path, .. } => path,
        other => panic!("expected a PC wallpaper mismatch, got {other}"),
    }
}

/// The ids written out from the documented scheme, not from `PcWallpaperId`.
fn expected_ids() -> BTreeSet<String> {
    STYLES
        .iter()
        .flat_map(|style| {
            let stem = style.stem;
            [
                format!("pc-wallpaper/{stem}/tiles"),
                format!("pc-wallpaper/{stem}/tilemap"),
                format!("pc-wallpaper/{stem}/palette/frame"),
                format!("pc-wallpaper/{stem}/palette/background"),
            ]
        })
        .collect()
}

#[test]
fn the_inventory_is_forty_eight_ids_in_wallpaper_order() {
    let ids = super::super::scope::pc_wallpaper_ids();
    assert_eq!(ids.len(), 48);
    assert_eq!(ids.iter().cloned().collect::<BTreeSet<_>>(), expected_ids());
    let stems: Vec<&str> = ids
        .chunks(4)
        .map(|chunk| chunk[0].split('/').nth(1).unwrap())
        .collect();
    assert_eq!(stems, STYLES.map(|style| style.stem));
}

#[test]
fn a_complete_checkout_yields_typed_wallpapers_with_the_upstream_sheet_shape() {
    let upstream = scratch_dir("valid");
    write_valid_checkout(&upstream);
    // A neighbouring Walda or default style must not change the pack.
    write(
        &upstream
            .join(PC_WALLPAPER_DIRECTORY)
            .join("forest")
            .join("frame.png"),
        b"not a png",
    );

    let mut writer = PackWriter::new();
    extract_pc_wallpapers(&upstream, &mut writer).unwrap();
    assert_eq!(writer.len(), 48);
    let pack_path = upstream.join("pack.bin");
    write(&pack_path, &writer.finish().unwrap());
    let pack = AssetPack::load(&pack_path).unwrap();

    for (index, style) in STYLES.iter().enumerate() {
        let wallpaper = pack.pc_wallpaper(Style::byte(index)).unwrap();
        assert_eq!(wallpaper.id.stem(), style.stem);
        // The frame's kept tiles come first, then the background's.
        assert_eq!(
            (wallpaper.tiles.width, wallpaper.tiles.height),
            (8, style.tiles() * 8),
            "{}",
            style.stem
        );
        for tile in 0..style.tiles() {
            let (half, source_tile) = if tile < style.frame_tiles {
                (Half::Frame, tile)
            } else {
                (Half::Background, tile - style.frame_tiles)
            };
            let start = tile as usize * TILE_PIXELS;
            assert!(
                wallpaper.tiles.pixels[start..start + TILE_PIXELS]
                    .iter()
                    .all(|&pixel| pixel == style.pixel(half, source_tile)),
                "{} tile {tile}",
                style.stem
            );
        }
        assert_eq!(wallpaper.tilemap, tilemap_bytes(style), "{}", style.stem);
        // Shade 8 and 16 become 1 and 2 in each BGR555 field.
        assert_eq!(wallpaper.palettes[0].color(0), Some(0b00001_00001_00001));
        assert_eq!(wallpaper.palettes[1].color(15), Some(0b00010_00010_00010));
    }
    std::fs::remove_dir_all(upstream).unwrap();
}

#[test]
fn a_missing_source_fails_naming_that_file_and_queues_nothing() {
    let upstream = scratch_dir("missing");
    write_valid_checkout(&upstream);
    for style in &STYLES {
        for file in ["frame.png", "bg.png", "tilemap.bin"] {
            let path = style_dir(&upstream, style).join(file);
            let bytes = std::fs::read(&path).unwrap();
            std::fs::remove_file(&path).unwrap();

            let (result, queued) = extract(&upstream);
            let err = result.unwrap_err();
            assert!(
                matches!(&err, ExtractError::ReadFailed(missing, _) if *missing == path),
                "{} {file}: {err}",
                style.stem
            );
            assert_eq!(queued, 0, "{} {file}", style.stem);
            write(&path, &bytes);
        }
    }
    std::fs::remove_dir_all(upstream).unwrap();
}

#[test]
fn a_tilemap_that_is_not_twenty_by_eighteen_words_is_rejected() {
    for length in [
        0,
        1,
        TILEMAP_BYTES - 2,
        TILEMAP_BYTES + 2,
        2 * TILEMAP_BYTES,
    ] {
        let upstream = scratch_dir("tilemap-length");
        write_valid_checkout(&upstream);
        let path = style_dir(&upstream, &STYLES[5]).join("tilemap.bin");
        write(&path, &vec![0; length]);

        let (result, queued) = extract(&upstream);
        assert_eq!(mismatch_path(result.unwrap_err()), path, "{length} bytes");
        assert_eq!(queued, 0, "{length} bytes");
        std::fs::remove_dir_all(upstream).unwrap();
    }
}

#[test]
fn a_tilemap_word_the_accessor_would_refuse_is_rejected() {
    let style = &STYLES[0];
    let past_the_sheet = u16::try_from(style.tiles()).unwrap() | 1 << 12;
    // Every synthetic tile 0 carries colour, so the blank word is refused.
    for (case, word) in [
        ("tile past the sheet", past_the_sheet),
        ("bank 0", 0x0001),
        ("bank 3", 0x3001),
        ("blank over a coloured tile 0", 0x0000),
    ] {
        let upstream = scratch_dir("tilemap-word");
        write_valid_checkout(&upstream);
        let path = style_dir(&upstream, style).join("tilemap.bin");
        let mut tilemap = tilemap_bytes(style);
        tilemap[200..202].copy_from_slice(&word.to_le_bytes());
        write(&path, &tilemap);

        let (result, queued) = extract(&upstream);
        assert_eq!(mismatch_path(result.unwrap_err()), path, "{case}");
        assert_eq!(queued, 0, "{case}");
        std::fs::remove_dir_all(upstream).unwrap();
    }
}

#[test]
fn a_palette_that_is_not_sixteen_colours_is_rejected() {
    for (file, half) in [("frame.png", Half::Frame), ("bg.png", Half::Background)] {
        for colors in [1, 15] {
            let upstream = scratch_dir("palette-colors");
            write_valid_checkout(&upstream);
            let style = &STYLES[2];
            let path = style_dir(&upstream, style).join(file);
            let bytes = if half == Half::Frame {
                half_png(style, half, style.frame_size, style.frame_tiles, colors)
            } else {
                half_png(
                    style,
                    half,
                    style.background_size,
                    style.background_tiles,
                    colors,
                )
            };
            write(&path, &bytes);

            let (result, queued) = extract(&upstream);
            assert_eq!(
                mismatch_path(result.unwrap_err()),
                path,
                "{file} with {colors} colours"
            );
            assert_eq!(queued, 0);
            std::fs::remove_dir_all(upstream).unwrap();
        }
    }
}

#[test]
fn a_png_of_the_wrong_size_depth_or_cut_is_rejected() {
    let style = &STYLES[0];
    let wrong_size = half_png(style, Half::Frame, (128, 24), style.frame_tiles, 16);
    let eight_bit = {
        let raw: Vec<u8> = (0..32)
            .flat_map(|_| std::iter::once(0).chain(std::iter::repeat_n(1, 128)))
            .collect();
        indexed_png_from_raw_with_plte(8, 128, 32, &raw, &[8; 256 * 3])
    };
    // Colour past the tiles `-num_tiles` keeps would be silently dropped.
    let colour_past_the_cut = half_png(style, Half::Frame, (128, 32), 50, 16);
    for (case, bytes) in [
        ("wrong size", wrong_size),
        ("8-bit", eight_bit),
        ("colour past the cut", colour_past_the_cut),
    ] {
        let upstream = scratch_dir("png-shape");
        write_valid_checkout(&upstream);
        let path = style_dir(&upstream, style).join("frame.png");
        write(&path, &bytes);

        let (result, queued) = extract(&upstream);
        assert_eq!(mismatch_path(result.unwrap_err()), path, "{case}");
        assert_eq!(queued, 0, "{case}");
        std::fs::remove_dir_all(upstream).unwrap();
    }
}

fn real_pack(name: &str) -> (PathBuf, AssetPack) {
    assert!(upstream_present(), "run ./init.sh first");
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-pc-wallpaper-real-{name}-{}.bin",
        std::process::id()
    ));
    extract_to(&path).unwrap();
    let pack = AssetPack::load(&path).unwrap();
    (path, pack)
}

/// One source half read straight from the checkout PNG.
struct SourceHalf {
    image: png::IndexedImage,
    colors: Vec<u16>,
}

fn source_half(path: &Path) -> SourceHalf {
    let bytes = std::fs::read(path).unwrap();
    SourceHalf {
        image: png::decode(&bytes).unwrap(),
        colors: png::decode_palette(&bytes)
            .unwrap()
            .into_iter()
            .map(jasc_pal::Rgb888::to_gba555)
            .collect(),
    }
}

impl SourceHalf {
    /// The pixel index at (`column`, `row`) of tile `tile` in PNG tile order.
    fn index(&self, tile: u32, column: u32, row: u32) -> usize {
        let tiles_per_row = self.image.width / 8;
        let x = tile % tiles_per_row * 8 + column;
        let y = tile / tiles_per_row * 8 + row;
        usize::from(self.image.pixels[(y * self.image.width + x) as usize])
    }
}

const WINDOW_WIDTH: usize = 160;

/// The 160x144 window `DrawWallpaper` draws, as BGR555 values (`None` for a
/// blank cell), read from the shipped pack.
fn render_pack(wallpaper: &assets::PcWallpaperHandle<'_>) -> Vec<Option<u16>> {
    let mut window = vec![None; WINDOW_WIDTH * 144];
    for (cell, entry) in wallpaper.entries().enumerate() {
        if entry.is_blank() {
            continue;
        }
        let palette = &wallpaper.palettes[usize::from(entry.palette_bank) - 1];
        for row in 0..8usize {
            for column in 0..8usize {
                let (tile_column, tile_row) = (
                    if entry.h_flip { 7 - column } else { column },
                    if entry.v_flip { 7 - row } else { row },
                );
                let index = wallpaper.tiles.pixels
                    [usize::from(entry.tile_index) * TILE_PIXELS + tile_row * 8 + tile_column];
                let (x, y) = (cell % 20 * 8 + column, cell / 20 * 8 + row);
                window[y * WINDOW_WIDTH + x] = palette.color(usize::from(index));
            }
        }
    }
    window
}

/// The same window drawn from the checkout's own PNGs and tilemap: tile `t` is
/// the frame's tile `t` below `frame_tiles` and the background's tile
/// `t - frame_tiles` after, and bank 1 and 2 are the frame and background
/// `PLTE`s.
fn render_source(style: &Style, source_root: &Path) -> Vec<Option<u16>> {
    let dir = source_root.join(style.stem);
    let frame = source_half(&dir.join("frame.png"));
    let background = source_half(&dir.join("bg.png"));
    let tilemap = std::fs::read(dir.join("tilemap.bin")).unwrap();
    let mut window = vec![None; WINDOW_WIDTH * 144];
    for (cell, pair) in tilemap.chunks_exact(2).enumerate() {
        let word = u16::from_le_bytes([pair[0], pair[1]]);
        if word == 0 {
            continue;
        }
        let tile = u32::from(word & 0x3ff);
        let (source, source_tile) = if tile < style.frame_tiles {
            (&frame, tile)
        } else {
            (&background, tile - style.frame_tiles)
        };
        let palette = if word >> 12 == 1 { &frame } else { &background };
        for row in 0..8u32 {
            for column in 0..8u32 {
                let (tile_column, tile_row) = (
                    if word & 0x400 != 0 {
                        7 - column
                    } else {
                        column
                    },
                    if word & 0x800 != 0 { 7 - row } else { row },
                );
                let index = source.index(source_tile, tile_column, tile_row);
                let (x, y) = (
                    cell % 20 * 8 + column as usize,
                    cell / 20 * 8 + row as usize,
                );
                window[y * WINDOW_WIDTH + x] = Some(palette.colors[index]);
            }
        }
    }
    window
}

#[test]
#[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
fn the_real_checkout_decodes_all_twelve_wallpapers_through_the_typed_accessor() {
    let (path, pack) = real_pack("inventory");
    let ids: BTreeSet<String> = pack
        .entries()
        .filter(|entry| entry.id.starts_with("pc-wallpaper/"))
        .map(|entry| entry.id.clone())
        .collect();
    assert_eq!(ids, expected_ids());
    for (index, style) in STYLES.iter().enumerate() {
        let wallpaper = pack.pc_wallpaper(Style::byte(index)).unwrap();
        assert_eq!(wallpaper.id.stem(), style.stem);
        assert_eq!(
            wallpaper.tiles.height,
            style.tiles() * 8,
            "{} sheet",
            style.stem
        );
        // Upstream's tilemaps address the whole sheet and no further.
        let highest = wallpaper
            .entries()
            .map(|entry| u32::from(entry.tile_index))
            .max();
        assert_eq!(highest, Some(style.tiles() - 1), "{}", style.stem);
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
#[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
fn a_real_wallpaper_renders_like_its_source_with_flips_and_both_palette_banks() {
    let (path, pack) = real_pack("render");
    let source_root = repo_root().join("pokeemerald").join(PC_WALLPAPER_DIRECTORY);
    for (index, style) in STYLES.iter().enumerate() {
        let wallpaper = pack.pc_wallpaper(Style::byte(index)).unwrap();
        let rendered = render_pack(&wallpaper);
        assert!(
            rendered == render_source(style, &source_root),
            "{} renders differently from its checkout sources",
            style.stem
        );
        assert!(rendered.iter().any(Option::is_none), "{} blank", style.stem);
    }

    // Seafloor mirrors tiles both ways and draws from both banks, so a
    // renderer that dropped a flip or swapped the banks would not match above.
    let seafloor = pack.pc_wallpaper(Style::byte(5)).unwrap();
    let cells: Vec<_> = seafloor.entries().collect();
    assert!(cells.iter().any(|cell| cell.h_flip));
    assert!(cells.iter().any(|cell| cell.v_flip));
    assert!(cells.iter().any(|cell| cell.palette_bank == 1));
    assert!(cells.iter().any(|cell| cell.palette_bank == 2));
    std::fs::remove_file(path).unwrap();
}
