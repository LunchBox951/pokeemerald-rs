use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use assets::AssetPack;

use super::super::png::tests::{grayscale_png_from_raw, indexed_png_from_raw_with_plte};
use super::super::{extract_to, repo_root, upstream_present};
use super::{
    extract_evolution_scene, pack_ids, PaletteFile, BACKGROUND_TILEMAP_BYTES, IMAGES, PALETTES,
    TILEMAPS,
};
use crate::extract::ExtractError;
use pack_format::PackWriter;

const MANIFEST_ENTRY_COUNT: usize = 7;
/// Both nibbles of this byte are the grayscale sample 4.
const BACKGROUND_SAMPLE: u8 = 0x44;
/// `gbagfx` stores a 4-bit grayscale sample `s` as palette index `15 - s`.
const INVERTED_BACKGROUND_INDEX: u8 = 15 - 4;

fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pokeemerald-rs-evolution-scene-{name}-{}",
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

/// A 4-bit PNG whose samples all equal `pixel`; the background is grayscale,
/// the sparkle carries a 16-entry `PLTE`.
fn image_png(width: u32, height: u32, bit_depth: u8, pixel: u8, grayscale: bool) -> Vec<u8> {
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

fn write_file(path: &Path, bytes: impl AsRef<[u8]>) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

/// A complete synthetic checkout, with a decoy `unused_1.pal` beside the real
/// palettes.
fn write_checkout(dir: &Path) {
    for image in IMAGES {
        let grayscale = image.png.ends_with("bg.png");
        write_file(
            &graphics(dir).join(image.png),
            image_png(image.width, image.height, 4, BACKGROUND_SAMPLE, grayscale),
        );
    }
    for (i, palette) in PALETTES.iter().enumerate() {
        if let PaletteFile::Jasc(name) = palette.file {
            let first = 8 * u8::try_from(i + 1).unwrap();
            write_file(
                &graphics(dir).join(name),
                palette_text(palette.colors, first),
            );
        }
    }
    for (i, tilemap) in TILEMAPS.iter().enumerate() {
        write_file(
            &dir.join(tilemap.path),
            vec![0xA0 + u8::try_from(i).unwrap(); BACKGROUND_TILEMAP_BYTES],
        );
    }
    write_file(
        &graphics(dir).join("evolution_scene/unused_1.pal"),
        palette_text(3, 99),
    );
}

#[test]
fn manifest_names_every_constituent_and_no_unused_palette() {
    let ids = pack_ids();
    assert_eq!(ids.len(), MANIFEST_ENTRY_COUNT);
    let expected: BTreeSet<&str> = [
        "evolution-scene/image/bg",
        "evolution-scene/image/evo_sparkle",
        "evolution-scene/palette/bg_anim_intro",
        "evolution-scene/palette/bg_anim",
        "evolution-scene/palette/evo_sparkle",
        "evolution-scene/raw/bg_inner",
        "evolution-scene/raw/bg_outer",
    ]
    .into_iter()
    .collect();
    assert_eq!(
        ids.iter().map(String::as_str).collect::<BTreeSet<_>>(),
        expected
    );
}

#[test]
fn a_complete_checkout_extracts_every_entry_and_resolves_through_the_typed_handle() {
    let dir = scratch_dir("complete");
    write_checkout(&dir);
    let mut writer = PackWriter::new();
    extract_evolution_scene(&dir, &mut writer).unwrap();
    assert_eq!(writer.len(), MANIFEST_ENTRY_COUNT);
    let pack_path = dir.join("t.pack");
    std::fs::write(&pack_path, writer.finish().unwrap()).unwrap();
    let pack = AssetPack::load(&pack_path).unwrap();

    let scene = pack.evolution_scene().unwrap();
    assert_eq!(
        (scene.background.width, scene.background.height),
        (128, 112)
    );
    assert_eq!(scene.background.bit_depth, 4);
    assert!(scene
        .background
        .pixels
        .iter()
        .all(|&pixel| pixel == INVERTED_BACKGROUND_INDEX));
    assert_eq!(scene.intro_palette.color_count, 256);
    assert_eq!(scene.animation_palette.color_count, 32);
    assert_eq!(scene.sparkle.palette.color_count, 16);
    assert_eq!(scene.inner_tilemap, vec![0xA0; BACKGROUND_TILEMAP_BYTES]);
    assert_eq!(scene.outer_tilemap, vec![0xA1; BACKGROUND_TILEMAP_BYTES]);
    assert_ne!(
        scene.intro_palette.colors().next(),
        scene.animation_palette.colors().next()
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn each_missing_source_fails_instead_of_being_skipped() {
    let sources: Vec<PathBuf> = IMAGES
        .iter()
        .map(|image| graphics(Path::new("")).join(image.png))
        .chain(PALETTES.iter().filter_map(|palette| match palette.file {
            PaletteFile::Jasc(name) => Some(graphics(Path::new("")).join(name)),
            PaletteFile::PngPlte(_) => None,
        }))
        .chain(TILEMAPS.iter().map(|tilemap| PathBuf::from(tilemap.path)))
        .collect();
    assert_eq!(
        sources.len(),
        6,
        "the sparkle palette shares its image file"
    );
    for (index, source) in sources.iter().enumerate() {
        let dir = scratch_dir(&format!("missing-{index}"));
        write_checkout(&dir);
        std::fs::remove_file(dir.join(source)).unwrap();
        let mut writer = PackWriter::new();
        let err = extract_evolution_scene(&dir, &mut writer).unwrap_err();
        assert!(
            matches!(&err, ExtractError::ReadFailed(path, _) if *path == dir.join(source)),
            "{}: {err:?}",
            source.display()
        );
        assert_eq!(writer.len(), 0, "{}", source.display());
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn sources_of_the_wrong_shape_are_rejected_without_queuing_anything() {
    type Corrupt = fn(&Path);
    let cases: [(&str, Corrupt, &str); 7] = [
        (
            "intro-palette-count",
            |dir| {
                let path = graphics(dir).join("evolution_scene/bg_anim_intro.pal");
                std::fs::write(path, palette_text(16, 1)).unwrap();
            },
            "evolution_scene/bg_anim_intro.pal",
        ),
        (
            "animation-palette-count",
            |dir| {
                let path = graphics(dir).join("evolution_scene/bg_anim.pal");
                std::fs::write(path, palette_text(16, 1)).unwrap();
            },
            "evolution_scene/bg_anim.pal",
        ),
        (
            "background-dimensions",
            |dir| {
                let path = graphics(dir).join("evolution_scene/bg.png");
                std::fs::write(path, image_png(128, 96, 4, 0, true)).unwrap();
            },
            "evolution_scene/bg.png",
        ),
        (
            "sparkle-bit-depth",
            |dir| {
                let path = graphics(dir).join("misc/evo_sparkle.png");
                std::fs::write(path, image_png(8, 8, 8, 0, false)).unwrap();
            },
            "misc/evo_sparkle.png",
        ),
        (
            "sparkle-plte-count",
            |dir| {
                let path = graphics(dir).join("misc/evo_sparkle.png");
                let raw: Vec<u8> = (0..8).flat_map(|_| [0u8; 5]).collect();
                let png = indexed_png_from_raw_with_plte(4, 8, 8, &raw, &[1; 8 * 3]);
                std::fs::write(path, png).unwrap();
            },
            "misc/evo_sparkle.png",
        ),
        (
            "inner-tilemap-length",
            |dir| {
                std::fs::write(dir.join("graphics/evolution_scene/bg_inner.bin"), [0u8; 10])
                    .unwrap();
            },
            "evolution_scene/bg_inner.bin",
        ),
        (
            "outer-tilemap-length",
            |dir| {
                std::fs::write(
                    dir.join("graphics/evolution_scene/bg_outer.bin"),
                    vec![0u8; BACKGROUND_TILEMAP_BYTES + 2],
                )
                .unwrap();
            },
            "evolution_scene/bg_outer.bin",
        ),
    ];
    for (name, corrupt, file) in cases {
        let dir = scratch_dir(name);
        write_checkout(&dir);
        corrupt(&dir);
        let mut writer = PackWriter::new();
        let err = extract_evolution_scene(&dir, &mut writer).unwrap_err();
        assert!(
            matches!(&err, ExtractError::EvolutionSceneAssetMismatch { path, .. }
                if *path == graphics(&dir).join(file)),
            "{name}: {err:?}"
        );
        assert_eq!(writer.len(), 0, "{name}");
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn a_background_that_is_not_a_png_is_rejected() {
    let dir = scratch_dir("bad-png");
    write_checkout(&dir);
    let path = graphics(&dir).join("evolution_scene/bg.png");
    std::fs::write(&path, b"not a png").unwrap();
    let mut writer = PackWriter::new();
    let err = extract_evolution_scene(&dir, &mut writer).unwrap_err();
    assert!(matches!(err, ExtractError::Png(p, _) if p == path));
    assert_eq!(writer.len(), 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
#[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
fn evolution_scene_assets_are_extracted_and_resolve_through_the_typed_handle() {
    assert!(upstream_present(), "run ./init.sh first");
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-extract-test-evolution-scene-{}.pack",
        std::process::id()
    ));
    extract_to(&path).expect("extraction should succeed against a real checkout");
    let pack = AssetPack::load(&path).unwrap();
    let checkout = repo_root().join("pokeemerald");
    let decode = |relative: &str| {
        super::png::decode(&std::fs::read(checkout.join("graphics").join(relative)).unwrap())
            .unwrap()
    };

    let scene = pack.evolution_scene().unwrap();
    assert_eq!(
        scene.background.pixels,
        decode("evolution_scene/bg.png").pixels
    );
    assert!(scene.background.pixels.iter().any(|&pixel| pixel != 0));
    assert_eq!(
        scene.sparkle.image.pixels,
        decode("misc/evo_sparkle.png").pixels
    );
    assert_eq!(
        scene.inner_tilemap,
        std::fs::read(checkout.join("graphics/evolution_scene/bg_inner.bin")).unwrap()
    );
    assert_eq!(
        scene.outer_tilemap,
        std::fs::read(checkout.join("graphics/evolution_scene/bg_outer.bin")).unwrap()
    );
    assert_ne!(scene.inner_tilemap, scene.outer_tilemap);
    assert_ne!(
        scene.intro_palette.colors().collect::<Vec<_>>(),
        scene
            .animation_palette
            .colors()
            .take(256)
            .collect::<Vec<_>>()
    );
    let _ = std::fs::remove_file(path);
}
