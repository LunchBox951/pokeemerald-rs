//! Static battler art for the species in [`BATTLERS`]: the checkout side of
//! their `gMonStillFrontPicTable`, `gMonBackPicTable`, `gMonPaletteTable`, and
//! `gMonShinyPaletteTable` rows (the species and table lines are cited on
//! [`BATTLERS`]).

use std::path::Path;

use super::scope::{BattlerSource, BATTLERS, BATTLER_SIDE};
use super::{
    build_image_entry, build_palette_entry, jasc_pal, png, read_file, read_text, ExtractError,
};
use pack_format::PackWriter;

const COLORS: usize = 16;
/// The PNG bit depth the typed accessor requires of every battler image.
const BIT_DEPTH: u8 = 4;

/// Extracts every listed species' front and back images and both palettes.
pub(super) fn extract_battlers(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    for battler in BATTLERS {
        extract_battler(upstream, battler, writer)?;
    }
    Ok(())
}

fn extract_battler(
    upstream: &Path,
    battler: BattlerSource,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let directory = upstream.join("graphics/pokemon").join(battler.name);
    let normal = read_palette(&directory.join("normal.pal"))?;
    let shiny = read_palette(&directory.join("shiny.pal"))?;

    let mut entries = Vec::with_capacity(4);
    for (file, id, height) in [
        ("front.png", battler.front_id(), battler.front_height()),
        ("back.png", battler.back_id(), BATTLER_SIDE),
    ] {
        let path = directory.join(file);
        let image =
            png::decode(&read_file(&path)?).map_err(|e| ExtractError::Png(path.clone(), e))?;
        if image.width != BATTLER_SIDE || image.height != height {
            return Err(mismatch(
                &path,
                format!(
                    "is {}x{}: expected {BATTLER_SIDE}x{height}",
                    image.width, image.height
                ),
            ));
        }
        if image.bit_depth != BIT_DEPTH {
            return Err(mismatch(
                &path,
                format!("is {}-bit: expected {BIT_DEPTH}-bit", image.bit_depth),
            ));
        }
        if let Some(&pixel) = image
            .pixels
            .iter()
            .find(|&&pixel| usize::from(pixel) >= COLORS)
        {
            return Err(mismatch(
                &path,
                format!("has pixel index {pixel}: its palettes only have {COLORS} colours"),
            ));
        }
        entries.push(build_image_entry(&path, id, image)?);
    }
    for (file, id, colors) in [
        ("normal.pal", battler.normal_id(), &normal),
        ("shiny.pal", battler.shiny_id(), &shiny),
    ] {
        entries.push(build_palette_entry(&directory.join(file), colors, id)?);
    }
    for entry in entries {
        writer.push(entry);
    }
    Ok(())
}

fn read_palette(path: &Path) -> Result<Vec<jasc_pal::Rgb888>, ExtractError> {
    let colors =
        jasc_pal::parse(&read_text(path)?).map_err(|e| ExtractError::Pal(path.to_path_buf(), e))?;
    if colors.len() != COLORS {
        return Err(mismatch(
            path,
            format!("has {} colours: expected {COLORS}", colors.len()),
        ));
    }
    Ok(colors)
}

fn mismatch(path: &Path, detail: String) -> ExtractError {
    ExtractError::BattlerAssetMismatch {
        path: path.to_path_buf(),
        detail,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use assets::{AssetPack, BattlerDirection, PaletteVariant, SpeciesId};

    use super::super::png::tests::indexed_png_from_raw_with_plte;
    use super::super::{extract_to, repo_root, upstream_present};
    use super::{extract_battler, extract_battlers};
    use crate::extract::scope::{BattlerSource, BATTLERS};
    use crate::extract::ExtractError;
    use pack_format::PackWriter;

    fn scratch_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pokeemerald-rs-battler-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The species the pack carries, as the manifest must list them.
    const SPECIES: [(SpeciesId, &str); 14] = [
        (SpeciesId::TREECKO, "treecko"),
        (SpeciesId::TORCHIC, "torchic"),
        (SpeciesId::MUDKIP, "mudkip"),
        (SpeciesId::POOCHYENA, "poochyena"),
        (SpeciesId::MIGHTYENA, "mightyena"),
        (SpeciesId::WURMPLE, "wurmple"),
        (SpeciesId::SILCOON, "silcoon"),
        (SpeciesId::BEAUTIFLY, "beautifly"),
        (SpeciesId::CASCOON, "cascoon"),
        (SpeciesId::DUSTOX, "dustox"),
        (SpeciesId::ZIGZAGOON, "zigzagoon"),
        (SpeciesId::LINOONE, "linoone"),
        (SpeciesId::WINGULL, "wingull"),
        (SpeciesId::PELIPPER, "pelipper"),
    ];

    #[test]
    fn manifest_names_fifty_six_unique_entries() {
        let ids: Vec<String> = BATTLERS.iter().flat_map(|b| b.ids()).collect();
        assert_eq!(ids.len(), 56);
        assert_eq!(ids.iter().collect::<BTreeSet<_>>().len(), 56);
        let expected: BTreeSet<String> = SPECIES
            .into_iter()
            .flat_map(|(_, name)| {
                [
                    format!("pokemon/{name}/front"),
                    format!("pokemon/{name}/back"),
                    format!("pokemon/{name}/palette/normal"),
                    format!("pokemon/{name}/palette/shiny"),
                ]
            })
            .collect();
        assert_eq!(ids.into_iter().collect::<BTreeSet<_>>(), expected);
    }

    #[test]
    fn manifest_species_match_the_typed_lookup() {
        let manifest: Vec<(SpeciesId, &str)> =
            BATTLERS.iter().map(|b| (b.species, b.name)).collect();
        assert_eq!(manifest, SPECIES);
    }

    #[test]
    fn missing_sources_fail_instead_of_being_skipped() {
        let dir = scratch_dir("missing");
        let mut writer = PackWriter::new();

        let err = extract_battlers(&dir, &mut writer).unwrap_err();
        assert!(matches!(err, ExtractError::ReadFailed(path, _) if path.starts_with(&dir)));
        assert_eq!(writer.len(), 0);

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn sources_of_the_wrong_shape_are_rejected_without_queuing_anything() {
        let dir = scratch_dir("shape");
        let source = BATTLERS[0];
        let upstream = dir.join("graphics/pokemon").join(source.name);
        std::fs::create_dir_all(&upstream).unwrap();
        let palette = |colors: usize| {
            let mut text = format!("JASC-PAL\n0100\n{colors}\n");
            for _ in 0..colors {
                text.push_str("0 0 0\n");
            }
            text
        };
        std::fs::write(upstream.join("normal.pal"), palette(15)).unwrap();
        std::fs::write(upstream.join("shiny.pal"), palette(16)).unwrap();
        let mut writer = PackWriter::new();

        let err = extract_battlers(&dir, &mut writer).unwrap_err();
        assert!(matches!(
            err,
            ExtractError::BattlerAssetMismatch { path, .. } if path == upstream.join("normal.pal")
        ));

        std::fs::write(upstream.join("normal.pal"), palette(16)).unwrap();
        std::fs::write(upstream.join("front.png"), b"not a png").unwrap();
        let err = extract_battlers(&dir, &mut writer).unwrap_err();
        assert!(matches!(err, ExtractError::Png(path, _) if path == upstream.join("front.png")));
        assert_eq!(writer.len(), 0);

        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A 64-wide all-zero image of `height` rows at `bit_depth` with a
    /// 16-colour palette.
    fn battler_png(bit_depth: u8, height: u32) -> Vec<u8> {
        sheet_png(bit_depth, height, 0, 0)
    }

    /// A 64-wide image whose first 64 rows hold `top` in every byte and the
    /// rest `below`.
    fn sheet_png(bit_depth: u8, height: u32, top: u8, below: u8) -> Vec<u8> {
        let row_bytes = (64 * usize::from(bit_depth)).div_ceil(8);
        let mut raw = Vec::new();
        for row in 0..height {
            raw.push(0);
            raw.resize(raw.len() + row_bytes, if row < 64 { top } else { below });
        }
        indexed_png_from_raw_with_plte(bit_depth, 64, height, &raw, &[7; 48])
    }

    /// Writes `source`'s palettes and the two images into a scratch checkout.
    fn write_source(
        dir: &std::path::Path,
        source: BattlerSource,
        front: Vec<u8>,
        back: Vec<u8>,
    ) -> std::path::PathBuf {
        let palette = "JASC-PAL\n0100\n16\n".to_string() + &"0 0 0\n".repeat(16);
        let upstream = dir.join("graphics/pokemon").join(source.name);
        std::fs::create_dir_all(&upstream).unwrap();
        std::fs::write(upstream.join("normal.pal"), &palette).unwrap();
        std::fs::write(upstream.join("shiny.pal"), &palette).unwrap();
        std::fs::write(upstream.join("front.png"), front).unwrap();
        std::fs::write(upstream.join("back.png"), back).unwrap();
        upstream
    }

    #[test]
    fn images_of_the_wrong_bit_depth_are_rejected_without_queuing_anything() {
        let source = BATTLERS[0];
        let palette = "JASC-PAL\n0100\n16\n".to_string() + &"0 0 0\n".repeat(16);
        for bad_file in ["front.png", "back.png"] {
            for wrong in [2u8, 8] {
                let dir = scratch_dir("bit-depth");
                let upstream = dir.join("graphics/pokemon").join(source.name);
                std::fs::create_dir_all(&upstream).unwrap();
                std::fs::write(upstream.join("normal.pal"), &palette).unwrap();
                std::fs::write(upstream.join("shiny.pal"), &palette).unwrap();
                for file in ["front.png", "back.png"] {
                    let depth = if file == bad_file { wrong } else { 4 };
                    std::fs::write(upstream.join(file), battler_png(depth, 64)).unwrap();
                }
                let mut writer = PackWriter::new();

                let err = extract_battlers(&dir, &mut writer).unwrap_err();
                match &err {
                    ExtractError::BattlerAssetMismatch { path, detail } => {
                        assert_eq!(path, &upstream.join(bad_file));
                        assert!(
                            detail.contains(&format!("{wrong}-bit")) && detail.contains("4-bit"),
                            "{bad_file} at {wrong}: {detail}"
                        );
                    }
                    other => panic!("{bad_file} at {wrong}-bit: wrong error {other:?}"),
                }
                assert_eq!(writer.len(), 0);
                std::fs::remove_dir_all(dir).unwrap();
            }
        }
    }

    fn source_of(species: SpeciesId) -> BattlerSource {
        *BATTLERS.iter().find(|b| b.species == species).unwrap()
    }

    #[test]
    fn poochyenas_front_sheet_is_extracted_whole() {
        let dir = scratch_dir("sheet");
        let source = source_of(SpeciesId::POOCHYENA);
        write_source(
            &dir,
            source,
            sheet_png(4, 256, 0x33, 0xFF),
            battler_png(4, 64),
        );
        let mut writer = PackWriter::new();

        extract_battler(&dir, source, &mut writer).unwrap();

        assert_eq!(writer.len(), 4);
        let pack = writer.finish().unwrap();
        let entries = pack_format::parse_directory(&pack).unwrap();
        let front = entries
            .iter()
            .find(|e| e.id == "pokemon/poochyena/front")
            .unwrap();
        let pixels = &pack[front.offset..front.offset + front.length];
        assert_eq!(pixels.len(), 64 * 256);
        assert!(pixels[..64 * 64].iter().all(|&p| p == 3));
        assert!(pixels[64 * 64..].iter().all(|&p| p == 15));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn only_poochyenas_front_may_be_taller_than_its_picture() {
        // (species, file, rows written, message the rejection carries)
        let cases = [
            (
                SpeciesId::POOCHYENA,
                "front.png",
                64,
                "is 64x64: expected 64x256",
            ),
            (
                SpeciesId::POOCHYENA,
                "front.png",
                128,
                "is 64x128: expected 64x256",
            ),
            (
                SpeciesId::POOCHYENA,
                "front.png",
                320,
                "is 64x320: expected 64x256",
            ),
            (
                SpeciesId::POOCHYENA,
                "back.png",
                256,
                "is 64x256: expected 64x64",
            ),
            (
                SpeciesId::MIGHTYENA,
                "front.png",
                256,
                "is 64x256: expected 64x64",
            ),
            (
                SpeciesId::PELIPPER,
                "back.png",
                256,
                "is 64x256: expected 64x64",
            ),
        ];
        for (species, bad_file, rows, message) in cases {
            let dir = scratch_dir("tall");
            let source = source_of(species);
            let front_rows = if species == SpeciesId::POOCHYENA {
                256
            } else {
                64
            };
            let png = |file: &str| {
                let height = if file == bad_file {
                    rows
                } else if file == "front.png" {
                    front_rows
                } else {
                    64
                };
                battler_png(4, height)
            };
            let upstream = write_source(&dir, source, png("front.png"), png("back.png"));
            let mut writer = PackWriter::new();

            let err = extract_battler(&dir, source, &mut writer).unwrap_err();

            match &err {
                ExtractError::BattlerAssetMismatch { path, detail } => {
                    assert_eq!(path, &upstream.join(bad_file));
                    assert_eq!(detail, message);
                }
                other => panic!(
                    "{} {bad_file} at {rows}: wrong error {other:?}",
                    source.name
                ),
            }
            assert_eq!(writer.len(), 0);
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn every_battler_is_extracted_and_resolves_through_the_typed_lookup() {
        assert!(upstream_present(), "run ./init.sh first");
        let path = std::env::temp_dir().join(format!(
            "pokeemerald-rs-extract-test-battler-{}.pack",
            std::process::id()
        ));
        extract_to(&path).expect("extraction should succeed against a real checkout");
        let pack = AssetPack::load(&path).unwrap();
        let upstream = repo_root().join("pokeemerald");

        for (species, name) in SPECIES {
            for (direction, file, stem) in [
                (BattlerDirection::Front, "front.png", "front"),
                (BattlerDirection::Back, "back.png", "back"),
            ] {
                for variant in [PaletteVariant::Normal, PaletteVariant::Shiny] {
                    let art = pack
                        .species_battler(species, direction, variant)
                        .unwrap()
                        .art;
                    let source =
                        std::fs::read(upstream.join("graphics/pokemon").join(name).join(file))
                            .unwrap();
                    let decoded = super::png::decode(&source).unwrap();
                    let stored = pack.image(&format!("pokemon/{name}/{stem}")).unwrap();
                    assert_eq!(stored.pixels, decoded.pixels.as_slice(), "{name} {file}");
                    assert_eq!(
                        art.image.pixels,
                        &decoded.pixels[..64 * 64],
                        "{name} {file}: the picture is the top rows"
                    );
                }
            }
        }
        let _ = std::fs::remove_file(path);
    }
}
