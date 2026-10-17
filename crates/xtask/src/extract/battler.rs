//! Static battler art for the three starters: the checkout side of the
//! `gMonStillFrontPicTable`, `gMonBackPicTable`, `gMonPaletteTable`, and
//! `gMonShinyPaletteTable` rows for `TREECKO`, `TORCHIC`, and `MUDKIP`
//! (`src/data/pokemon_graphics/still_front_pic_table.h:283,286,289`; the
//! other three tables at `:280,283,286`).

use std::path::Path;

use super::scope::{BattlerSource, BATTLERS};
use super::{
    build_image_entry, build_palette_entry, jasc_pal, png, read_file, read_text, ExtractError,
};
use pack_format::PackWriter;

const SIDE: u32 = 64;
const COLORS: usize = 16;

/// Extracts every starter's front and back images and both palettes.
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
    for (file, id) in [
        ("front.png", battler.front_id()),
        ("back.png", battler.back_id()),
    ] {
        let path = directory.join(file);
        let image =
            png::decode(&read_file(&path)?).map_err(|e| ExtractError::Png(path.clone(), e))?;
        if image.width != SIDE || image.height != SIDE {
            return Err(mismatch(
                &path,
                format!(
                    "is {}x{}: expected {SIDE}x{SIDE}",
                    image.width, image.height
                ),
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

    use super::super::{extract_to, repo_root, upstream_present};
    use super::extract_battlers;
    use crate::extract::scope::BATTLERS;
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

    #[test]
    fn manifest_names_twelve_unique_entries() {
        let ids: Vec<String> = BATTLERS.iter().flat_map(|b| b.ids()).collect();
        assert_eq!(ids.len(), 12);
        assert_eq!(ids.iter().collect::<BTreeSet<_>>().len(), 12);
        let expected: BTreeSet<String> = ["treecko", "torchic", "mudkip"]
            .into_iter()
            .flat_map(|name| {
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
        let species: Vec<SpeciesId> = BATTLERS.iter().map(|b| b.species).collect();
        assert_eq!(
            species,
            [SpeciesId::TREECKO, SpeciesId::TORCHIC, SpeciesId::MUDKIP]
        );
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

    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn starter_battlers_are_extracted_and_resolve_through_the_typed_lookup() {
        assert!(upstream_present(), "run ./init.sh first");
        let path = std::env::temp_dir().join(format!(
            "pokeemerald-rs-extract-test-battler-{}.pack",
            std::process::id()
        ));
        extract_to(&path).expect("extraction should succeed against a real checkout");
        let pack = AssetPack::load(&path).unwrap();
        let upstream = repo_root().join("pokeemerald");

        for (species, name) in [
            (SpeciesId::TREECKO, "treecko"),
            (SpeciesId::TORCHIC, "torchic"),
            (SpeciesId::MUDKIP, "mudkip"),
        ] {
            for (direction, file) in [
                (BattlerDirection::Front, "front.png"),
                (BattlerDirection::Back, "back.png"),
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
                    assert_eq!(art.image.pixels, decoded.pixels.as_slice(), "{name} {file}");
                }
            }
        }
        let _ = std::fs::remove_file(path);
    }
}
