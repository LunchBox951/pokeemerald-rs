//! Extracts the singles health-box frames, their two palettes, and the enemy
//! shadow from `graphics/battle_interface/`.
//!
//! Five records: the player and opponent frame images (`graphics.c:628-629`),
//! the palettes embedded in `ball_status_bar.png` and `ball_display.png`
//! (`graphics.c:354-356`), and `enemy_mon_shadow.png` (`graphics.c:1037`). The
//! two frames differ in metatile geometry, 8x8 tiles for the player and 8x4 for
//! the opponent, so each is checked against its own side's sheet size. Images
//! keep PNG raster order; the two palette sources contribute only their `PLTE`.

use std::path::Path;

use assets::pack::SinglesHealthboxSide;

use super::{
    build_image_entry, build_palette_entry, mismatch, png, read_file, ExtractError, BANK_COLORS,
    DIRECTORY, IMAGE_PREFIX, PALETTE_PREFIX, TEXTBOX_BIT_DEPTH,
};
use pack_format::PackWriter;

const SHADOW_SIZE: (u32, u32) = (32, 8);

/// A source image and the sheet size its upstream PNG has.
#[derive(Clone, Copy, Debug)]
struct ImageSource {
    stem: &'static str,
    size: (u32, u32),
}

const FRAME_SOURCES: [(SinglesHealthboxSide, &str); 2] = [
    (SinglesHealthboxSide::Player, "healthbox_singles_player"),
    (SinglesHealthboxSide::Opponent, "healthbox_singles_opponent"),
];

const SHADOW_STEM: &str = "enemy_mon_shadow";

/// Sources whose embedded `PLTE` becomes a palette record of the same stem.
const PALETTE_STEMS: [&str; 2] = ["ball_status_bar", "ball_display"];

/// Every pack id [`extract_healthbox_frames`] emits: the two frames and the
/// enemy shadow, then the two palettes.
pub(in crate::extract) fn pack_ids() -> Vec<String> {
    FRAME_SOURCES
        .iter()
        .map(|(_, stem)| format!("{IMAGE_PREFIX}/{stem}"))
        .chain([format!("{IMAGE_PREFIX}/{SHADOW_STEM}")])
        .chain(
            PALETTE_STEMS
                .iter()
                .map(|stem| format!("{PALETTE_PREFIX}/{stem}")),
        )
        .collect()
}

/// Extracts the two frames, the enemy shadow, and the two palettes.
pub(in crate::extract) fn extract_healthbox_frames(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    for (side, stem) in FRAME_SOURCES {
        let source = ImageSource {
            stem,
            size: side.image_size(),
        };
        extract_image(upstream, source, writer)?;
    }
    let shadow = ImageSource {
        stem: SHADOW_STEM,
        size: SHADOW_SIZE,
    };
    extract_image(upstream, shadow, writer)?;
    for stem in PALETTE_STEMS {
        extract_embedded_palette(upstream, stem, writer)?;
    }
    Ok(())
}

/// A 4-bit PNG decodes to indices below 16, so depth and size are the whole check.
fn extract_image(
    upstream: &Path,
    source: ImageSource,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let path = upstream
        .join(DIRECTORY)
        .join(format!("{}.png", source.stem));
    let bytes = read_file(&path)?;
    let image = png::decode(&bytes).map_err(|e| ExtractError::Png(path.clone(), e))?;
    if image.bit_depth != TEXTBOX_BIT_DEPTH {
        return Err(mismatch(
            &path,
            format!(
                "is {}-bit: expected {TEXTBOX_BIT_DEPTH}-bit",
                image.bit_depth
            ),
        ));
    }
    let (width, height) = source.size;
    if (image.width, image.height) != (width, height) {
        return Err(mismatch(
            &path,
            format!(
                "is {}x{}: expected {width}x{height}",
                image.width, image.height
            ),
        ));
    }
    writer.push(build_image_entry(
        &path,
        format!("{IMAGE_PREFIX}/{}", source.stem),
        image,
    )?);
    Ok(())
}

fn extract_embedded_palette(
    upstream: &Path,
    stem: &str,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let path = upstream.join(DIRECTORY).join(format!("{stem}.png"));
    let bytes = read_file(&path)?;
    let colors = png::decode_palette(&bytes).map_err(|e| ExtractError::Png(path.clone(), e))?;
    if colors.len() != BANK_COLORS {
        return Err(mismatch(
            &path,
            format!(
                "has {} embedded colours: expected {BANK_COLORS}",
                colors.len()
            ),
        ));
    }
    writer.push(build_palette_entry(
        &path,
        &colors,
        format!("{PALETTE_PREFIX}/{stem}"),
    )?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use assets::AssetPack;

    use super::super::super::png::tests::{indexed_png_from_raw_with_plte, tiny_indexed_png};
    use super::super::super::png::{self, PngError};
    use super::super::super::{extract_to, repo_root, upstream_present};
    use super::{extract_healthbox_frames, pack_ids, DIRECTORY, FRAME_SOURCES};
    use crate::extract::ExtractError;
    use pack_format::PackWriter;

    const PLAYER: &str = "healthbox_singles_player";
    const OPPONENT: &str = "healthbox_singles_opponent";
    const SHADOW: &str = "enemy_mon_shadow";
    const STATUS_BAR: &str = "ball_status_bar";
    const BALL_DISPLAY: &str = "ball_display";
    const SOURCES: [&str; 5] = [PLAYER, OPPONENT, SHADOW, STATUS_BAR, BALL_DISPLAY];
    const RECORD_IDS: [&str; 5] = [
        "battle-interface/image/healthbox_singles_player",
        "battle-interface/image/healthbox_singles_opponent",
        "battle-interface/image/enemy_mon_shadow",
        "battle-interface/palette/ball_status_bar",
        "battle-interface/palette/ball_display",
    ];

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pokeemerald-rs-healthbox-frames-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(DIRECTORY)).unwrap();
        dir
    }

    /// Each 8x8 tile holds its own index: `column + 3 * row` modulo 16.
    fn tile_index(x: u32, y: u32) -> u8 {
        u8::try_from((x / 8 + 3 * (y / 8)) % 16).unwrap()
    }

    fn raster(width: u32, height: u32) -> Vec<u8> {
        (0..height)
            .flat_map(|y| (0..width).map(move |x| tile_index(x, y)))
            .collect()
    }

    /// A PLTE of `colors` entries whose entry `i` is `rgb(i)`.
    fn plte(colors: u32, rgb: impl Fn(u8) -> [u8; 3]) -> Vec<u8> {
        (0..colors)
            .flat_map(|i| rgb(u8::try_from(i).unwrap()))
            .collect()
    }

    /// A PNG of `width` x `height` carrying `tile_index`, with a 16-colour
    /// PLTE; 4-bit rows are packed high nibble first.
    fn frame_png(width: u32, height: u32, depth: u8) -> Vec<u8> {
        let mut raw = Vec::new();
        for y in 0..height {
            raw.push(0);
            if depth == 4 {
                for x in (0..width).step_by(2) {
                    raw.push(tile_index(x, y) << 4 | tile_index(x + 1, y));
                }
            } else {
                raw.extend((0..width).map(|x| tile_index(x, y)));
            }
        }
        indexed_png_from_raw_with_plte(depth, width, height, &raw, &plte(16, |_| [7; 3]))
    }

    /// A 1x1 PNG whose PLTE has `colors` entries; the picture is not a frame.
    /// It is 8-bit, the only depth that can index the 17 and 32 entry cases.
    fn palette_png(colors: u32, rgb: impl Fn(u8) -> [u8; 3]) -> Vec<u8> {
        indexed_png_from_raw_with_plte(8, 1, 1, &[0, 0], &plte(colors, rgb))
    }

    fn status_bar_rgb(i: u8) -> [u8; 3] {
        [8 * i, 0, 0]
    }

    fn ball_display_rgb(i: u8) -> [u8; 3] {
        [0, 8 * i, 0]
    }

    fn write(dir: &Path, stem: &str, bytes: impl AsRef<[u8]>) -> PathBuf {
        let path = dir.join(DIRECTORY).join(format!("{stem}.png"));
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn write_valid_checkout(dir: &Path) {
        write(dir, PLAYER, frame_png(128, 64, 4));
        write(dir, OPPONENT, frame_png(128, 32, 4));
        write(dir, SHADOW, frame_png(32, 8, 4));
        write(dir, STATUS_BAR, palette_png(16, status_bar_rgb));
        write(dir, BALL_DISPLAY, palette_png(16, ball_display_rgb));
    }

    fn mismatch_path(err: ExtractError) -> PathBuf {
        match err {
            ExtractError::BattleInterfaceAssetMismatch { path, .. } => path,
            other => panic!("expected a battle-interface mismatch, got {other}"),
        }
    }

    #[test]
    fn the_manifest_pairs_each_side_with_its_own_geometry() {
        assert_eq!(
            FRAME_SOURCES.map(|(side, stem)| (stem, side.metatile_tiles(), side.image_size())),
            [(PLAYER, (8, 8), (128, 64)), (OPPONENT, (8, 4), (128, 32))]
        );
    }

    #[test]
    fn a_valid_manifest_yields_five_records_the_typed_accessors_read() {
        let dir = scratch_dir("valid");
        write_valid_checkout(&dir);
        // A neighbour the manifest never reads cannot change the output.
        write(&dir, "healthbox_doubles_player", b"not a png");
        let mut writer = PackWriter::new();
        extract_healthbox_frames(&dir, &mut writer).unwrap();
        assert_eq!(writer.len(), 5);
        let bytes = writer.finish().unwrap();
        let mut ids: Vec<_> = pack_format::parse_directory(&bytes)
            .unwrap()
            .into_iter()
            .map(|entry| entry.id)
            .collect();
        ids.sort();
        let mut expected = RECORD_IDS.map(str::to_owned);
        expected.sort();
        let mut declared = pack_ids();
        declared.sort();
        assert_eq!(ids, expected);
        assert_eq!(declared, expected);

        let pack_path = dir.join("frames.pack");
        std::fs::write(&pack_path, bytes).unwrap();
        let pack = AssetPack::load(&pack_path).unwrap();
        let frames = pack.singles_healthbox_frames().unwrap();
        let player = frames.player.image();
        assert_eq!((player.width, player.height), (128, 64));
        assert_eq!(player.pixels, raster(128, 64).as_slice());
        // Tile (1, 0) holds 1, (0, 1) holds 3, and tile (15, 7) holds 4.
        assert_eq!(
            [
                player.pixels[8],
                player.pixels[8 * 128],
                player.pixels[64 * 128 - 1]
            ],
            [1, 3, 4]
        );
        let opponent = frames.opponent.image();
        assert_eq!((opponent.width, opponent.height), (128, 32));
        assert_eq!(opponent.pixels, raster(128, 32).as_slice());
        let shadow = pack.enemy_mon_shadow().unwrap();
        assert_eq!((shadow.width, shadow.height), (32, 8));
        assert_eq!(shadow.pixels, raster(32, 8).as_slice());

        // Channel value `8 * i` is 5-bit level `i`, in red for the status bar
        // (bits 0-4) and in green for the ball display (bits 5-9).
        for index in 0..16 {
            let level = u16::try_from(index).unwrap();
            assert_eq!(frames.ball_status_bar_palette.color(index), Some(level));
            assert_eq!(frames.ball_display_palette.color(index), Some(level << 5));
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_missing_source_in_a_complete_checkout_fails_closed() {
        for stem in SOURCES {
            let dir = scratch_dir("missing-one");
            write_valid_checkout(&dir);
            std::fs::remove_file(dir.join(DIRECTORY).join(format!("{stem}.png"))).unwrap();
            let mut writer = PackWriter::new();
            let err = extract_healthbox_frames(&dir, &mut writer).unwrap_err();
            assert!(
                matches!(&err, ExtractError::ReadFailed(path, _)
                    if path.ends_with(format!("{stem}.png"))),
                "{stem}: {err}"
            );
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn the_frames_are_not_interchangeable() {
        let dir = scratch_dir("swapped");
        // Each side given the other side's otherwise valid sheet.
        for (stem, other_height) in [(PLAYER, 32), (OPPONENT, 64)] {
            write_valid_checkout(&dir);
            let path = write(&dir, stem, frame_png(128, other_height, 4));
            let err = extract_healthbox_frames(&dir, &mut PackWriter::new()).unwrap_err();
            assert_eq!(mismatch_path(err), path, "{stem}");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn images_off_the_upstream_shape_are_rejected() {
        let dir = scratch_dir("shape");
        let cases = [
            (PLAYER, 64, 128, 4),
            (PLAYER, 128, 64, 8),
            (OPPONENT, 64, 64, 4),
            (OPPONENT, 128, 32, 8),
            (SHADOW, 8, 32, 4),
            (SHADOW, 32, 8, 8),
        ];
        for (stem, width, height, depth) in cases {
            write_valid_checkout(&dir);
            let path = write(&dir, stem, frame_png(width, height, depth));
            let err = extract_healthbox_frames(&dir, &mut PackWriter::new()).unwrap_err();
            assert_eq!(
                mismatch_path(err),
                path,
                "{stem} {width}x{height} at {depth}-bit"
            );
        }

        write_valid_checkout(&dir);
        let path = write(&dir, SHADOW, b"not a png");
        let err = extract_healthbox_frames(&dir, &mut PackWriter::new()).unwrap_err();
        assert!(matches!(err, ExtractError::Png(error_path, _) if error_path == path));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn palette_sources_with_the_wrong_colour_count_are_rejected() {
        let dir = scratch_dir("palette");
        for stem in [STATUS_BAR, BALL_DISPLAY] {
            for colors in [15, 17, 32] {
                write_valid_checkout(&dir);
                let path = write(&dir, stem, palette_png(colors, status_bar_rgb));
                let err = extract_healthbox_frames(&dir, &mut PackWriter::new()).unwrap_err();
                assert_eq!(mismatch_path(err), path, "{stem} with {colors} colours");
            }
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_palette_source_without_a_usable_plte_is_rejected() {
        let dir = scratch_dir("plte");
        write_valid_checkout(&dir);
        let path = write(&dir, STATUS_BAR, tiny_indexed_png(8, 2, 1, &[1, 2]));
        let err = extract_healthbox_frames(&dir, &mut PackWriter::new()).unwrap_err();
        assert!(matches!(
            err,
            ExtractError::Png(error_path, PngError::MissingOrBadPalette) if error_path == path
        ));

        write_valid_checkout(&dir);
        let mut corrupt = palette_png(16, status_bar_rgb);
        let plte_at = corrupt.windows(4).position(|w| w == b"PLTE").unwrap();
        corrupt[plte_at + 4] ^= 0xFF;
        let path = write(&dir, BALL_DISPLAY, corrupt);
        let err = extract_healthbox_frames(&dir, &mut PackWriter::new()).unwrap_err();
        assert!(matches!(
            err,
            ExtractError::Png(error_path, PngError::ChunkCrcMismatch(kind))
                if error_path == path && &kind == b"PLTE"
        ));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn healthbox_frames_are_extracted() {
        assert!(upstream_present(), "run ./init.sh first");
        let path = std::env::temp_dir().join(format!(
            "pokeemerald-rs-extract-test-healthbox-frames-{}.pack",
            std::process::id()
        ));
        extract_to(&path).expect("extraction should succeed against a real checkout");
        let pack = AssetPack::load(&path).unwrap();
        let frames = pack.singles_healthbox_frames().unwrap();
        let shadow = pack.enemy_mon_shadow().unwrap();

        let upstream = repo_root().join("pokeemerald").join(DIRECTORY);
        let source = |stem: &str| std::fs::read(upstream.join(format!("{stem}.png"))).unwrap();
        for (stem, image, size) in [
            (PLAYER, frames.player.image(), (128, 64)),
            (OPPONENT, frames.opponent.image(), (128, 32)),
            (SHADOW, shadow, (32, 8)),
        ] {
            let decoded = png::decode(&source(stem)).unwrap();
            assert_eq!((image.width, image.height), size, "{stem}");
            assert_eq!((decoded.width, decoded.height), size, "{stem}");
            assert_eq!(image.pixels, decoded.pixels.as_slice(), "{stem}");
        }
        for (stem, palette) in [
            (STATUS_BAR, frames.ball_status_bar_palette),
            (BALL_DISPLAY, frames.ball_display_palette),
        ] {
            let expected: Vec<u16> = png::decode_palette(&source(stem))
                .unwrap()
                .iter()
                .map(|color| color.to_gba555())
                .collect();
            assert_eq!(palette.colors().collect::<Vec<_>>(), expected, "{stem}");
        }
        let _ = std::fs::remove_file(path);
    }
}
