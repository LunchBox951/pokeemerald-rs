//! Extracts the title screen graphics.

use std::path::Path;

use super::scope::{
    TITLE_SCREEN_EMBEDDED_PALETTE_SHEETS, TITLE_SCREEN_FILE_PALETTES, TITLE_SCREEN_IMAGES,
    TITLE_SCREEN_PALETTE_CUTS, TITLE_SCREEN_TILEMAPS,
};
use super::{
    build_image_entry, build_palette_entry, jasc_pal, png, push_palette_entry_with_limit,
    push_raw_bytes, read_file, read_text, ExtractError,
};
use pack_format::PackWriter;

/// Every source the fixed inventory names, in manifest order: images and
/// embedded-palette sheets as `.png`, tilemaps as `.bin`, palettes as `.pal`.
fn title_source_names() -> impl Iterator<Item = String> {
    TITLE_SCREEN_IMAGES
        .iter()
        .chain(&TITLE_SCREEN_EMBEDDED_PALETTE_SHEETS)
        .map(|stem| format!("{stem}.png"))
        .chain(
            TITLE_SCREEN_TILEMAPS
                .iter()
                .map(|stem| format!("{stem}.bin")),
        )
        .chain(
            TITLE_SCREEN_FILE_PALETTES
                .iter()
                .map(|stem| format!("{stem}.pal")),
        )
}

/// Refuse a title directory missing a source the generator requires.
fn require_title_sources(dir: &Path) -> Result<(), ExtractError> {
    for name in title_source_names() {
        let path = dir.join(&name);
        if !path.is_file() {
            return Err(ExtractError::ReadFailed(
                path,
                "required title screen source is missing".to_owned(),
            ));
        }
    }
    Ok(())
}

/// Side, in tiles, of every title background tilemap
/// (`BG_DIM_TILES`, `crates/pokeemerald-rs/src/title.rs:20`).
const BG_DIM_TILES: usize = 32;
/// Pixels per tile side (`BitDepth::TILE_DIM`, `crates/rendering/src/bg.rs:40`).
const TILE_DIM: u32 = 8;
/// The highest palette index a 4bpp tile nibble holds
/// (`BPP4_MAX_INDEX`, `crates/pokeemerald-rs/src/title/asset_tilesets.rs:43`).
const BPP4_MAX_INDEX: u8 = 0x0F;
/// Colours in one palette bank (`Palette::BANK_LEN`,
/// `crates/rendering/src/palette.rs:105`).
const PALETTE_BANK_COLORS: usize = 16;
/// `emerald_version.png` is exactly this size (`VERSION_SHEET_W/H`,
/// `crates/pokeemerald-rs/src/title.rs:32-33`).
const VERSION_SHEET: (u32, u32) = (128, 32);
/// `press_start.png` is exactly this size (`PRESS_START_SHEET_W/H`,
/// `crates/pokeemerald-rs/src/title.rs:35-36`).
const PRESS_START_SHEET: (u32, u32) = (160, 24);
/// Height of the two 160-wide frame rows `press_start_tileset` crops first
/// (`2 * PRESS_START_FRAME_H`, `crates/pokeemerald-rs/src/title.rs:38`).
const PRESS_START_FRAME_ROWS_H: u32 = 16;

/// Refuse a source the title scene cannot load.
fn contract_violation(path: &Path, detail: &str) -> ExtractError {
    ExtractError::ReadFailed(
        path.to_path_buf(),
        format!("title screen source breaks the title scene contract: {detail}"),
    )
}

/// Refuse a decoded title image the title scene would reject at load time.
///
/// Each rule mirrors what `TitleScene::from_pack`
/// (`crates/pokeemerald-rs/src/title.rs:230-260`) enforces, so a source that
/// passes here loads, and one that fails here would not:
///
/// - `rayquaza` and `clouds` are tiled as 4bpp (`title.rs:231-240`): both
///   sides must be a whole number of 8-pixel tiles (`asset_tilesets.rs:30-36`)
///   and every index must fit one nibble (`asset_tilesets.rs:56-60`).
/// - `pokemon_logo` is tiled as 8bpp (`title.rs:241-245`): both sides must be
///   a whole number of 8-pixel tiles (`asset_tilesets.rs:30-36`).
/// - `emerald_version` must be exactly 128x32 (`title.rs:32-33`,
///   `asset_tilesets.rs:122-146`; the banner halves are cropped from it at
///   `asset_tilesets.rs:155-173`).
/// - `press_start` must be exactly 160x24 (`title.rs:35-36`,
///   `asset_tilesets.rs:122-153`), and every index in the 4bpp-cropped region
///   (the top 160x16 and the 8x8 block below it, `asset_tilesets.rs:182-205`)
///   must fit one nibble (`asset_tilesets.rs:56-60`).
///
/// `logo_shine` is packed but not read by `from_pack`, so it has no shape
/// rule here.
fn validate_title_image(
    path: &Path,
    stem: &str,
    image: &png::IndexedImage,
) -> Result<(), ExtractError> {
    let (width, height) = (image.width, image.height);
    let tile_aligned = |path: &Path| {
        if width == 0 || height == 0 || width % TILE_DIM != 0 || height % TILE_DIM != 0 {
            return Err(contract_violation(
                path,
                &format!("is {width}x{height}: expected non-empty multiples of {TILE_DIM} pixels"),
            ));
        }
        Ok(())
    };
    let exact_size = |path: &Path, expected: (u32, u32)| {
        if (width, height) != expected {
            return Err(contract_violation(
                path,
                &format!(
                    "is {width}x{height}: expected {}x{}",
                    expected.0, expected.1
                ),
            ));
        }
        Ok(())
    };
    let nibble_indices = |path: &Path, region: &dyn Fn(usize, usize) -> bool| {
        let row_len = width as usize;
        let too_high = image.pixels.iter().enumerate().find(|&(offset, &index)| {
            index > BPP4_MAX_INDEX && region(offset % row_len, offset / row_len)
        });
        match too_high {
            Some((_, &index)) => Err(contract_violation(
                path,
                &format!("has palette index {index}: a 4bpp tile holds at most {BPP4_MAX_INDEX}"),
            )),
            None => Ok(()),
        }
    };
    match stem {
        "rayquaza" | "clouds" => {
            tile_aligned(path)?;
            nibble_indices(path, &|_, _| true)
        }
        "pokemon_logo" => tile_aligned(path),
        "emerald_version" => exact_size(path, VERSION_SHEET),
        "press_start" => {
            exact_size(path, PRESS_START_SHEET)?;
            nibble_indices(path, &|x, y| {
                y < PRESS_START_FRAME_ROWS_H as usize || x < TILE_DIM as usize
            })
        }
        _ => Ok(()),
    }
}

/// The byte length the title scene requires of `stem`'s tilemap.
///
/// `rayquaza` and `clouds` are regular maps of 32x32 two-byte screen entries
/// (`regular_tilemap_from_raw`, `crates/pokeemerald-rs/src/title/tile_decode.rs:41-49`,
/// `BG_DIM_TILES` at `title.rs:20`); `pokemon_logo` is an affine map of 32x32
/// one-byte tile indices (`affine_tilemap_from_raw`, `tile_decode.rs:57-59`,
/// `AffineTilemap::new` at `crates/rendering/src/bg_affine.rs:48-58`).
fn expected_tilemap_len(stem: &str) -> usize {
    let entries = BG_DIM_TILES * BG_DIM_TILES;
    if stem == "pokemon_logo" {
        entries
    } else {
        entries * 2
    }
}

/// Refuse a palette the title scene would silently leave part-blank.
///
/// `title_palette_from_refs` and `sprite_palette_from_refs`
/// (`crates/pokeemerald-rs/src/title/tile_decode.rs:7-29`,
/// `asset_tilesets.rs:210-236`) copy up to a bank of colours (16) from
/// `rayquaza_and_clouds` and from the `press_start` PNG's embedded palette and
/// leave the rest transparent black, so a shorter palette loads but renders
/// wrong. `pokemon_logo`'s 224 colours (`LOGO_PALETTE_COLORS`, `title.rs:21`)
/// are enforced by its build cut (`TITLE_SCREEN_PALETTE_CUTS`).
fn require_bank_of_colors(path: &Path, colors: usize) -> Result<(), ExtractError> {
    if colors < PALETTE_BANK_COLORS {
        return Err(contract_violation(
            path,
            &format!("has {colors} colours: expected at least {PALETTE_BANK_COLORS}"),
        ));
    }
    Ok(())
}

/// Extract the fixed title inventory from `graphics/title_screen`.
///
/// The manifest in `scope.rs` is authoritative: only the files it names are
/// read, so an untracked file in the directory never gains a `title/*` id
/// the ROM profile does not describe.
pub(super) fn extract_title_screen(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let dir = upstream.join("graphics/title_screen");
    require_title_sources(&dir)?;

    for stem in TITLE_SCREEN_IMAGES {
        let path = dir.join(format!("{stem}.png"));
        let bytes = read_file(&path)?;
        let image = png::decode(&bytes).map_err(|e| ExtractError::Png(path.clone(), e))?;
        validate_title_image(&path, stem, &image)?;
        if TITLE_SCREEN_EMBEDDED_PALETTE_SHEETS.contains(&stem) {
            if image.palette.is_empty() {
                return Err(ExtractError::MissingEmbeddedPalette(path.clone()));
            }
            if stem == "press_start" {
                require_bank_of_colors(&path, image.palette.len())?;
            }
            let colors: Vec<jasc_pal::Rgb888> = image
                .palette
                .iter()
                .map(|&[r, g, b]| jasc_pal::Rgb888 { r, g, b })
                .collect();
            let entry = build_palette_entry(&path, &colors, format!("title/palette/{stem}"))?;
            writer.push(entry);
        }
        writer.push(build_image_entry(
            &path,
            format!("title/image/{stem}"),
            image,
        )?);
    }
    for stem in TITLE_SCREEN_FILE_PALETTES {
        let path = dir.join(format!("{stem}.pal"));
        if stem == "rayquaza_and_clouds" {
            let colors = jasc_pal::parse(&read_text(&path)?)
                .map_err(|e| ExtractError::Pal(path.clone(), e))?;
            require_bank_of_colors(&path, colors.len())?;
        }
        let color_limit = TITLE_SCREEN_PALETTE_CUTS
            .iter()
            .find_map(|&(sheet, limit)| (sheet == stem).then_some(limit));
        push_palette_entry_with_limit(&path, format!("title/palette/{stem}"), color_limit, writer)?;
    }
    for stem in TITLE_SCREEN_TILEMAPS {
        let path = dir.join(format!("{stem}.bin"));
        let payload = read_file(&path)?;
        let (len, expected) = (payload.len(), expected_tilemap_len(stem));
        if len != expected {
            return Err(contract_violation(
                &path,
                &format!("is {len} bytes: expected {expected}"),
            ));
        }
        push_raw_bytes(format!("title/raw/{stem}"), payload, writer);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        extract_title_screen, ExtractError, PackWriter, TITLE_SCREEN_EMBEDDED_PALETTE_SHEETS,
        TITLE_SCREEN_FILE_PALETTES, TITLE_SCREEN_IMAGES, TITLE_SCREEN_TILEMAPS,
    };
    use crate::extract::png::tests::indexed_png_from_raw_with_plte;
    use crate::extract::{extract_to, upstream_present};

    fn scratch_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "pokeemerald-rs-extract-test-{name}-{}.pack",
            std::process::id()
        ))
    }

    fn assert_pack_contains_entry_id(bytes: &[u8], id: &str) {
        assert!(
            bytes
                .windows(id.len())
                .any(|window| window == id.as_bytes()),
            "missing pack entry id `{id}`"
        );
    }

    /// The upstream shape of each title PNG: width, height, bit depth, and
    /// the `PLTE` entries it carries.
    fn png_shape(stem: &str) -> (u32, u32, u8, usize) {
        match stem {
            "clouds" => (128, 56, 4, 16),
            "rayquaza" => (128, 128, 4, 16),
            "logo_shine" => (64, 64, 4, 16),
            "pokemon_logo" => (256, 64, 8, 256),
            "emerald_version" => (128, 32, 8, 16),
            "press_start" => (160, 24, 8, 16),
            other => panic!("`{other}` is not a title PNG"),
        }
    }

    /// An indexed PNG of `width` x `height` whose pixel at `(x, y)` is
    /// `pixel(x, y)`, with `plte` palette entries.
    fn png_with(
        bit_depth: u8,
        (width, height): (u32, u32),
        plte: usize,
        pixel: impl Fn(u32, u32) -> u8,
    ) -> Vec<u8> {
        let mut raw = Vec::new();
        for y in 0..height {
            raw.push(0);
            let row: Vec<u8> = (0..width).map(|x| pixel(x, y)).collect();
            if bit_depth == 4 {
                raw.extend(row.chunks(2).map(|p| (p[0] << 4) | p[1]));
            } else {
                raw.extend_from_slice(&row);
            }
        }
        indexed_png_from_raw_with_plte(bit_depth, width, height, &raw, &vec![0x40; plte * 3])
    }

    /// The upstream-shaped PNG for `stem`, every index at most 15.
    fn title_png(stem: &str) -> Vec<u8> {
        let (width, height, bit_depth, plte) = png_shape(stem);
        png_with(bit_depth, (width, height), plte, |x, y| {
            u8::try_from((x + y) % 16).unwrap()
        })
    }

    /// The byte length the title scene requires of each tilemap source.
    fn tilemap_len(stem: &str) -> usize {
        if stem == "pokemon_logo" {
            32 * 32
        } else {
            32 * 32 * 2
        }
    }

    /// A JASC palette of `count` colours.
    fn jasc_text(count: usize) -> String {
        use std::fmt::Write as _;
        let mut text = format!("JASC-PAL\r\n0100\r\n{count}\r\n");
        for n in 0..count {
            write!(text, "{} 0 0\r\n", n % 256).unwrap();
        }
        text
    }

    /// The upstream colour count of each `.pal` source.
    fn pal_colors(stem: &str) -> usize {
        match stem {
            "pokemon_logo" => 256,
            "rayquaza_and_clouds" => 16,
            _ => 112,
        }
    }

    /// Write a synthetic `graphics/title_screen` holding exactly the fixed
    /// inventory, shaped like the upstream assets the title scene loads, and
    /// return the upstream root and the title directory.
    fn synthetic_title_checkout(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let upstream = std::env::temp_dir().join(format!(
            "pokeemerald-rs-extract-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&upstream);
        let dir = upstream.join("graphics/title_screen");
        std::fs::create_dir_all(&dir).unwrap();
        for stem in TITLE_SCREEN_IMAGES {
            std::fs::write(dir.join(format!("{stem}.png")), title_png(stem)).unwrap();
        }
        for stem in TITLE_SCREEN_TILEMAPS {
            std::fs::write(
                dir.join(format!("{stem}.bin")),
                vec![0u8; tilemap_len(stem)],
            )
            .unwrap();
        }
        for stem in TITLE_SCREEN_FILE_PALETTES {
            std::fs::write(dir.join(format!("{stem}.pal")), jasc_text(pal_colors(stem))).unwrap();
        }
        (upstream, dir)
    }

    /// Extract a synthetic checkout in which `file` of the title directory was
    /// replaced by `bytes`, and return the error it must be refused with.
    fn refusal_of(name: &str, file: &str, bytes: &[u8]) -> (ExtractError, std::path::PathBuf) {
        let (upstream, dir) = synthetic_title_checkout(name);
        std::fs::write(dir.join(file), bytes).unwrap();
        let result = extract_title_screen(&upstream, &mut PackWriter::new());
        let _ = std::fs::remove_dir_all(&upstream);
        (
            result.expect_err("a malformed title source was accepted"),
            dir.join(file),
        )
    }

    /// Assert `err` refuses `path` with a message containing `detail`.
    fn assert_refuses(err: &ExtractError, path: &std::path::Path, detail: &str) {
        assert!(
            matches!(err, ExtractError::ReadFailed(p, msg)
                if p == path && msg.contains("title scene contract") && msg.contains(detail)),
            "{err:?}"
        );
    }

    fn pack_ids_with_prefix(bytes: &[u8], prefix: &str) -> usize {
        bytes
            .windows(prefix.len())
            .filter(|w| *w == prefix.as_bytes())
            .count()
    }

    #[test]
    fn the_fixed_inventory_extracts_from_a_synthetic_checkout() {
        let (upstream, _) = synthetic_title_checkout("title-baseline");
        let mut writer = PackWriter::new();
        let result = extract_title_screen(&upstream, &mut writer);
        let _ = std::fs::remove_dir_all(&upstream);
        result.unwrap();
        let bytes = writer.finish().unwrap();
        for id in crate::extract::scope::title_ids() {
            assert_pack_contains_entry_id(&bytes, &id);
        }
        assert_eq!(pack_ids_with_prefix(&bytes, "title/"), 14);
    }

    #[test]
    fn a_title_tilemap_of_the_wrong_length_is_rejected() {
        for stem in TITLE_SCREEN_TILEMAPS {
            let right = tilemap_len(stem);
            for wrong in [0, 7, right - 1, right + 1, right * 2] {
                let file = format!("{stem}.bin");
                let (err, path) = refusal_of("title-map-length", &file, &vec![0u8; wrong]);
                assert_refuses(&err, &path, &format!("is {wrong} bytes: expected {right}"));
            }
        }
    }

    #[test]
    fn the_regular_and_affine_tilemap_sizes_are_not_interchangeable() {
        let (err, path) = refusal_of("title-affine-as-regular", "rayquaza.bin", &[0; 32 * 32]);
        assert_refuses(&err, &path, "expected 2048");
        let (err, path) = refusal_of(
            "title-regular-as-affine",
            "pokemon_logo.bin",
            &[0; 32 * 32 * 2],
        );
        assert_refuses(&err, &path, "expected 1024");
    }

    #[test]
    fn a_tiled_title_image_off_the_tile_grid_is_rejected() {
        for (stem, depth, plte) in [
            ("rayquaza", 4, 16),
            ("clouds", 4, 16),
            ("pokemon_logo", 8, 256),
        ] {
            for size in [(128, 60), (132, 56), (6, 8)] {
                let png = png_with(depth, size, plte, |_, _| 1);
                let (err, path) = refusal_of("title-off-grid", &format!("{stem}.png"), &png);
                assert_refuses(&err, &path, &format!("is {}x{}", size.0, size.1));
            }
        }
    }

    #[test]
    fn a_4bpp_title_image_with_an_index_above_fifteen_is_rejected() {
        for stem in ["rayquaza", "clouds"] {
            let (width, height, ..) = png_shape(stem);
            let png = png_with(8, (width, height), 256, |x, y| {
                if (x, y) == (3, 5) {
                    16
                } else {
                    1
                }
            });
            let (err, path) = refusal_of("title-wide-index", &format!("{stem}.png"), &png);
            assert_refuses(&err, &path, "has palette index 16");
        }
    }

    #[test]
    fn a_8bpp_logo_may_use_every_index() {
        let (upstream, dir) = synthetic_title_checkout("title-logo-indices");
        let png = png_with(8, (256, 64), 256, |x, _| u8::try_from(x % 256).unwrap());
        std::fs::write(dir.join("pokemon_logo.png"), png).unwrap();
        let result = extract_title_screen(&upstream, &mut PackWriter::new());
        let _ = std::fs::remove_dir_all(&upstream);
        result.unwrap();
    }

    #[test]
    fn a_title_sprite_sheet_of_the_wrong_size_is_rejected() {
        for (stem, expected, wrong) in [
            (
                "emerald_version",
                "128x32",
                [(8, 8), (128, 24), (64, 32), (128, 40)],
            ),
            (
                "press_start",
                "160x24",
                [(8, 8), (160, 16), (128, 24), (160, 32)],
            ),
        ] {
            for size in wrong {
                let png = png_with(8, size, 16, |_, _| 1);
                let (err, path) = refusal_of("title-sheet-size", &format!("{stem}.png"), &png);
                assert_refuses(
                    &err,
                    &path,
                    &format!("is {}x{}: expected {expected}", size.0, size.1),
                );
            }
        }
    }

    #[test]
    fn a_press_start_index_the_4bpp_crop_reads_above_fifteen_is_rejected() {
        // Inside the two frame rows, and inside the 8x8 block below them.
        for at in [(159, 15), (0, 0), (7, 23), (0, 16)] {
            let png = png_with(8, (160, 24), 16, |x, y| if (x, y) == at { 16 } else { 2 });
            let (err, path) = refusal_of("title-press-start-index", "press_start.png", &png);
            assert_refuses(&err, &path, "has palette index 16");
        }
    }

    #[test]
    fn a_press_start_index_outside_the_4bpp_crop_is_not_read_by_the_scene() {
        let (upstream, dir) = synthetic_title_checkout("title-press-start-uncropped");
        let png = png_with(
            8,
            (160, 24),
            16,
            |x, y| if (x, y) == (8, 16) { 200 } else { 2 },
        );
        std::fs::write(dir.join("press_start.png"), png).unwrap();
        let result = extract_title_screen(&upstream, &mut PackWriter::new());
        let _ = std::fs::remove_dir_all(&upstream);
        result.unwrap();
    }

    #[test]
    fn a_press_start_palette_shorter_than_a_bank_is_rejected() {
        let png = png_with(8, (160, 24), 15, |_, _| 1);
        let (err, path) = refusal_of("title-press-start-palette", "press_start.png", &png);
        assert_refuses(&err, &path, "has 15 colours: expected at least 16");
    }

    #[test]
    fn a_title_palette_shorter_than_the_scene_reads_is_rejected() {
        let (err, path) = refusal_of(
            "title-clouds-palette",
            "rayquaza_and_clouds.pal",
            jasc_text(15).as_bytes(),
        );
        assert_refuses(&err, &path, "has 15 colours: expected at least 16");
        let (err, path) = refusal_of(
            "title-logo-palette",
            "pokemon_logo.pal",
            jasc_text(223).as_bytes(),
        );
        assert!(
            matches!(&err, ExtractError::PaletteShorterThanCut { path: p, cut: 224, actual: 223 } if p == &path),
            "{err:?}"
        );
    }

    #[test]
    fn a_title_image_that_is_not_a_png_is_rejected() {
        let (err, path) = refusal_of("title-not-png", "clouds.png", b"not a png");
        assert!(
            matches!(&err, ExtractError::Png(p, _) if p == &path),
            "{err:?}"
        );
    }

    /// The pack a valid checkout extracts to loads through the title scene
    /// itself, so the fixture is the consumer's contract and not a lookalike.
    #[cfg(feature = "scenes")]
    #[test]
    fn the_extracted_fixture_loads_through_the_title_scene() {
        let (upstream, _) = synthetic_title_checkout("title-consumer");
        let mut writer = PackWriter::new();
        let result = extract_title_screen(&upstream, &mut writer);
        let _ = std::fs::remove_dir_all(&upstream);
        result.unwrap();
        let path = scratch_path("title-consumer");
        std::fs::write(&path, writer.finish().unwrap()).unwrap();
        let pack = assets::AssetPack::load(&path);
        let _ = std::fs::remove_file(&path);
        pokeemerald_rs::title::TitleScene::from_pack(&pack.unwrap()).unwrap();
    }

    #[test]
    fn untracked_title_files_are_not_packed() {
        let (upstream, dir) = synthetic_title_checkout("untracked-title");
        // Neither readable as its extension nor in the inventory: a
        // manifest-driven extractor never opens them.
        std::fs::write(dir.join("zz_untracked.png"), b"not a png").unwrap();
        std::fs::write(dir.join("zz_untracked.pal"), b"not a palette").unwrap();
        std::fs::write(dir.join("zz_untracked.bin"), b"UNTRACKED").unwrap();
        // An inventory stem under another extension is not an inventory file.
        std::fs::write(dir.join("logo_shine.bin"), b"UNTRACKED").unwrap();
        std::fs::write(dir.join("unused.png"), b"not a png").unwrap();
        let mut writer = PackWriter::new();
        let result = extract_title_screen(&upstream, &mut writer);
        let _ = std::fs::remove_dir_all(&upstream);
        result.unwrap();
        let bytes = writer.finish().unwrap();
        assert_eq!(pack_ids_with_prefix(&bytes, "title/"), 14);
        assert!(!bytes.windows(9).any(|w| w == b"UNTRACKED"));
        for id in [
            "title/raw/zz_untracked",
            "title/image/zz_untracked",
            "title/palette/zz_untracked",
            "title/raw/logo_shine",
            "title/image/unused",
        ] {
            assert!(
                !bytes.windows(id.len()).any(|w| w == id.as_bytes()),
                "untracked title file was packed as `{id}`"
            );
        }
    }

    #[test]
    fn a_title_directory_missing_a_required_source_is_refused() {
        let upstream = std::env::temp_dir().join(format!(
            "pokeemerald-rs-extract-missing-title-source-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&upstream);
        let dir = upstream.join("graphics/title_screen");
        std::fs::create_dir_all(&dir).unwrap();
        // Every required source except `rayquaza.bin`; contents are never
        // read because the presence check runs first.
        for stem in TITLE_SCREEN_IMAGES {
            std::fs::write(dir.join(format!("{stem}.png")), b"").unwrap();
        }
        for stem in ["clouds", "pokemon_logo"] {
            std::fs::write(dir.join(format!("{stem}.bin")), b"").unwrap();
        }
        for stem in TITLE_SCREEN_FILE_PALETTES {
            std::fs::write(dir.join(format!("{stem}.pal")), b"").unwrap();
        }

        let err = extract_title_screen(&upstream, &mut PackWriter::new()).unwrap_err();
        let _ = std::fs::remove_dir_all(&upstream);
        assert!(
            matches!(&err, ExtractError::ReadFailed(path, _) if path == &dir.join("rayquaza.bin")),
            "{err:?}"
        );
    }

    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn embedded_palette_sheets_get_a_title_palette_entry() {
        assert!(upstream_present(), "run ./init.sh first");
        let path = scratch_path("embedded-palettes");
        let report = extract_to(&path).expect("extraction should succeed against a real checkout");
        let bytes = std::fs::read(&report.output_path).unwrap();
        for name in TITLE_SCREEN_EMBEDDED_PALETTE_SHEETS {
            let id = format!("title/palette/{name}");
            assert_pack_contains_entry_id(&bytes, &id);
        }
        let _ = std::fs::remove_file(report.output_path);
    }
}
