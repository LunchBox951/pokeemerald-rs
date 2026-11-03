use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use assets::{AssetPack, PackError};

use super::super::png::tests::{grayscale_png_from_raw, indexed_png_from_raw_with_plte};
use super::super::{extract_to, repo_root, upstream_present};
use super::{
    extract_opening_scene, pack_ids, PaletteFile, IMAGES, PALETTES, SCREEN_TILEMAP_BYTES, TILEMAPS,
};
use crate::extract::ExtractError;
use pack_format::PackWriter;

const MANIFEST_ENTRY_COUNT: usize = 16;

fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pokeemerald-rs-opening-scene-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn palette_text(colors: usize, first: u8) -> String {
    let mut text = format!("JASC-PAL\r\n0100\r\n{colors}\r\n");
    for i in 0..colors {
        let channel = if i == 0 { first } else { 0 };
        let _ = write!(text, "{channel} 0 0\r\n");
    }
    text
}

/// A PNG of `bit_depth`-bit samples all equal to `pixel`; the hand is the
/// grayscale one, every other sheet carries a 16-entry `PLTE`.
fn sheet_png(width: u32, height: u32, bit_depth: u8, pixel: u8, grayscale: bool) -> Vec<u8> {
    let row_bytes = (width as usize * usize::from(bit_depth)).div_ceil(8);
    let mut raw = Vec::new();
    for _ in 0..height {
        raw.push(0);
        raw.resize(raw.len() + row_bytes, pixel);
    }
    if grayscale {
        grayscale_png_from_raw(bit_depth, width, height, &raw)
    } else {
        let plte: Vec<u8> = (0..16u8).flat_map(|i| [i, 2 * i, 3 * i]).collect();
        indexed_png_from_raw_with_plte(bit_depth, width, height, &raw, &plte)
    }
}

fn graphics(dir: &Path) -> PathBuf {
    dir.join("graphics")
}

/// A complete synthetic checkout whose sheets, banks, and tilemaps differ.
fn write_checkout(dir: &Path) {
    for (i, image) in IMAGES.iter().enumerate() {
        let path = graphics(dir).join(image.png);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let pixel = 0x11 * u8::try_from(i + 1).unwrap();
        let grayscale = image.png.ends_with("hand.png");
        std::fs::write(
            path,
            sheet_png(image.width, image.height, 4, pixel, grayscale),
        )
        .unwrap();
    }
    for (i, palette) in PALETTES.iter().enumerate() {
        let (PaletteFile::Jasc(name) | PaletteFile::PngPlte(name)) = palette.file;
        if let PaletteFile::Jasc(_) = palette.file {
            let path = graphics(dir).join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let first = 8 * u8::try_from(i + 1).unwrap();
            std::fs::write(path, palette_text(palette.colors, first)).unwrap();
        }
    }
    for (i, tilemap) in TILEMAPS.iter().enumerate() {
        let path = dir.join(tilemap.path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            path,
            vec![0xA0 + u8::try_from(i).unwrap(); SCREEN_TILEMAP_BYTES],
        )
        .unwrap();
    }
}

#[test]
fn manifest_names_every_sheet_palette_and_tilemap_with_unique_ids() {
    let ids = pack_ids();
    assert_eq!(ids.len(), MANIFEST_ENTRY_COUNT);
    assert_eq!(ids.iter().collect::<BTreeSet<_>>().len(), ids.len());
    let expected: BTreeSet<&str> = [
        "opening-scene/image/shadow",
        "opening-scene/image/birch",
        "opening-scene/image/lotad",
        "opening-scene/image/wall_clock",
        "opening-scene/image/clock_hand",
        "opening-scene/palette/bg0",
        "opening-scene/palette/bg1",
        "opening-scene/palette/bg2",
        "opening-scene/palette/birch",
        "opening-scene/palette/lotad",
        "opening-scene/palette/clock_male",
        "opening-scene/palette/clock_female",
        "opening-scene/palette/clock_text_prompt",
        "opening-scene/raw/birch_speech_map",
        "opening-scene/raw/clock_start",
        "opening-scene/raw/clock_view",
    ]
    .into_iter()
    .collect();
    assert_eq!(
        ids.iter().map(String::as_str).collect::<BTreeSet<_>>(),
        expected
    );
}

#[test]
fn a_complete_checkout_extracts_every_entry_and_resolves_through_the_typed_handles() {
    let dir = scratch_dir("complete");
    write_checkout(&dir);
    let mut writer = PackWriter::new();
    extract_opening_scene(&dir, &mut writer).unwrap();
    assert_eq!(writer.len(), MANIFEST_ENTRY_COUNT);
    let pack_path = dir.join("t.pack");
    std::fs::write(&pack_path, writer.finish().unwrap()).unwrap();
    let pack = AssetPack::load(&pack_path).unwrap();

    let scene = pack.birch_speech().unwrap();
    assert_eq!((scene.shadow.width, scene.shadow.height), (128, 24));
    assert_eq!(scene.birch.palette.color_count, 16);
    assert_eq!(
        (scene.lotad.image.width, scene.lotad.image.height),
        (64, 64)
    );
    assert_ne!(scene.birch.image.pixels, scene.lotad.image.pixels);
    assert_eq!(scene.background_tilemap.len(), SCREEN_TILEMAP_BYTES);

    let clock = pack.wall_clock().unwrap();
    assert_eq!((clock.screen.width, clock.screen.height), (128, 64));
    assert_eq!((clock.hand.width, clock.hand.height), (64, 144));
    assert_eq!(clock.text_prompt_palette.color_count, 4);
    assert_ne!(
        clock.male_palette.colors().collect::<Vec<_>>(),
        clock.female_palette.colors().collect::<Vec<_>>()
    );
    assert_ne!(clock.start_tilemap, clock.view_tilemap);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn missing_sources_fail_instead_of_being_skipped() {
    let dir = scratch_dir("missing");
    let mut writer = PackWriter::new();
    let err = extract_opening_scene(&dir, &mut writer).unwrap_err();
    assert!(matches!(err, ExtractError::ReadFailed(path, _) if path.starts_with(&dir)));
    assert_eq!(writer.len(), 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn sources_of_the_wrong_shape_are_rejected_without_queuing_anything() {
    type Corrupt = fn(&Path);
    let cases: [(&str, Corrupt, &str); 5] = [
        (
            "palette-count",
            |dir| {
                let path = graphics(dir).join("wallclock/text_prompt.pal");
                std::fs::write(path, palette_text(3, 1)).unwrap();
            },
            "wallclock/text_prompt.pal",
        ),
        (
            "dimensions",
            |dir| {
                let path = graphics(dir).join("wallclock/clock.png");
                std::fs::write(path, sheet_png(64, 64, 4, 0, false)).unwrap();
            },
            "wallclock/clock.png",
        ),
        (
            "bit-depth",
            |dir| {
                let path = graphics(dir).join("pokemon/lotad/front.png");
                std::fs::write(path, sheet_png(64, 64, 8, 0, false)).unwrap();
            },
            "pokemon/lotad/front.png",
        ),
        (
            "plte-count",
            |dir| {
                let path = graphics(dir).join("birch_speech/birch.png");
                let raw: Vec<u8> = (0..64).flat_map(|_| [0u8; 33]).collect();
                let png = indexed_png_from_raw_with_plte(4, 64, 64, &raw, &[1; 8 * 3]);
                std::fs::write(path, png).unwrap();
            },
            "birch_speech/birch.png",
        ),
        (
            "tilemap-length",
            |dir| {
                std::fs::write(dir.join("graphics/wallclock/clock_view.bin"), [0u8; 10]).unwrap();
            },
            "wallclock/clock_view.bin",
        ),
    ];
    for (name, corrupt, file) in cases {
        let dir = scratch_dir(name);
        write_checkout(&dir);
        corrupt(&dir);
        let mut writer = PackWriter::new();
        let err = extract_opening_scene(&dir, &mut writer).unwrap_err();
        assert!(
            matches!(&err, ExtractError::OpeningSceneAssetMismatch { path, .. }
                if *path == graphics(&dir).join(file)),
            "{name}: {err:?}"
        );
        assert_eq!(writer.len(), 0, "{name}");
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn sources_that_are_not_png_are_rejected() {
    let dir = scratch_dir("bad-png");
    write_checkout(&dir);
    let path = graphics(&dir).join("birch_speech/shadow.png");
    std::fs::write(&path, b"not a png").unwrap();
    let mut writer = PackWriter::new();
    let err = extract_opening_scene(&dir, &mut writer).unwrap_err();
    assert!(matches!(err, ExtractError::Png(p, _) if p == path));
    assert_eq!(writer.len(), 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
#[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
fn opening_scene_assets_are_extracted_and_resolve_through_the_typed_handles() {
    assert!(upstream_present(), "run ./init.sh first");
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-extract-test-opening-scene-{}.pack",
        std::process::id()
    ));
    extract_to(&path).expect("extraction should succeed against a real checkout");
    let pack = AssetPack::load(&path).unwrap();
    let graphics = repo_root().join("pokeemerald/graphics");

    let scene = pack.birch_speech().unwrap();
    let decode = |relative: &str| {
        super::png::decode(&std::fs::read(graphics.join(relative)).unwrap()).unwrap()
    };
    assert_eq!(
        scene.shadow.pixels,
        decode("birch_speech/shadow.png").pixels
    );
    assert_eq!(
        scene.birch.image.pixels,
        decode("birch_speech/birch.png").pixels
    );
    assert_eq!(
        scene.lotad.image.pixels,
        decode("pokemon/lotad/front.png").pixels
    );
    assert_eq!(scene.background_tilemap.len(), 1280);

    let clock = pack.wall_clock().unwrap();
    assert_eq!(clock.screen.pixels, decode("wallclock/clock.png").pixels);
    // The hand is grayscale upstream: its samples are the palette indices.
    assert_eq!(clock.hand.pixels, decode("wallclock/hand.png").pixels);
    assert!(clock.hand.pixels.iter().any(|&pixel| pixel != 0));
    assert_ne!(
        clock.male_palette.colors().collect::<Vec<_>>(),
        clock.female_palette.colors().collect::<Vec<_>>()
    );
    assert!(matches!(
        pack.palette("opening-scene/palette/missing"),
        Err(PackError::UnknownAsset(_))
    ));
    let _ = std::fs::remove_file(path);
}
