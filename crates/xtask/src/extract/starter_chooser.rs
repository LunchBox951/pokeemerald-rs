use std::path::Path;

use super::{build_image_entry, jasc_pal, png, read_file, read_text, ExtractError};
use super::{build_palette_entry, push_raw_entry};
use pack_format::PackWriter;

const CHOOSER_DIRECTORY: &str = "graphics/starter_choose";

/// Where an indexed sheet's palette comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaletteSource {
    /// The `PLTE` chunk of the sheet's own PNG.
    EmbeddedPng,
    /// A JASC `.pal` file beside the sheet.
    Jasc(&'static str),
}

#[derive(Clone, Copy, Debug)]
struct SheetSource {
    directory: &'static str,
    png: &'static str,
    image_id: &'static str,
    palette_id: &'static str,
    palette: PaletteSource,
    width: u32,
    height: u32,
    colors: usize,
}

#[derive(Clone, Copy, Debug)]
struct TilemapSource {
    path: &'static str,
    id: &'static str,
    byte_len: usize,
}

const fn front(
    species: &'static str,
    image_id: &'static str,
    palette_id: &'static str,
) -> SheetSource {
    SheetSource {
        directory: species,
        png: "front.png",
        image_id,
        palette_id,
        palette: PaletteSource::Jasc("normal.pal"),
        width: 64,
        height: 64,
        colors: 16,
    }
}

/// Image and palette pairs, relative to the checkout's `graphics/`: the
/// chooser sheets (`starter_choose.c:54-61`, `400-422`) and the shared normal
/// fronts (`629-635`).
const SHEETS: [SheetSource; 6] = [
    SheetSource {
        directory: CHOOSER_DIRECTORY,
        png: "tiles.png",
        image_id: "starter-chooser/image/tiles",
        palette_id: "starter-chooser/palette/tiles",
        palette: PaletteSource::EmbeddedPng,
        width: 128,
        height: 128,
        colors: 32,
    },
    SheetSource {
        directory: CHOOSER_DIRECTORY,
        png: "pokeball_selection.png",
        image_id: "starter-chooser/image/pokeball_selection",
        palette_id: "starter-chooser/palette/pokeball_selection",
        palette: PaletteSource::EmbeddedPng,
        width: 32,
        height: 128,
        colors: 16,
    },
    SheetSource {
        directory: CHOOSER_DIRECTORY,
        png: "starter_circle.png",
        image_id: "starter-chooser/image/starter_circle",
        palette_id: "starter-chooser/palette/starter_circle",
        palette: PaletteSource::EmbeddedPng,
        width: 64,
        height: 64,
        colors: 16,
    },
    front(
        "graphics/pokemon/treecko",
        "pokemon/treecko/front",
        "pokemon/treecko/palette/normal",
    ),
    front(
        "graphics/pokemon/torchic",
        "pokemon/torchic/front",
        "pokemon/torchic/palette/normal",
    ),
    front(
        "graphics/pokemon/mudkip",
        "pokemon/mudkip/front",
        "pokemon/mudkip/palette/normal",
    ),
];

const TILEMAPS: [TilemapSource; 2] = [
    TilemapSource {
        path: "graphics/starter_choose/birch_bag.bin",
        id: "starter-chooser/raw/birch_bag",
        byte_len: 1280,
    },
    TilemapSource {
        path: "graphics/starter_choose/birch_grass.bin",
        id: "starter-chooser/raw/birch_grass",
        byte_len: 2048,
    },
];

/// Extracts the starter chooser's fixed manifest: six image and palette pairs
/// and two raw tilemaps.
pub(super) fn extract_starter_chooser(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    for sheet in SHEETS {
        extract_sheet(upstream, sheet, writer)?;
    }
    for tilemap in TILEMAPS {
        extract_tilemap(upstream, tilemap, writer)?;
    }
    Ok(())
}

fn extract_tilemap(
    upstream: &Path,
    tilemap: TilemapSource,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let path = upstream.join(tilemap.path);
    let len = read_file(&path)?.len();
    if len != tilemap.byte_len {
        return Err(mismatch(
            &path,
            format!("is {len} bytes: expected {}", tilemap.byte_len),
        ));
    }
    push_raw_entry(&path, tilemap.id.to_owned(), writer)
}

fn extract_sheet(
    upstream: &Path,
    sheet: SheetSource,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let path = upstream.join(sheet.directory).join(sheet.png);
    let bytes = read_file(&path)?;
    let image = png::decode(&bytes).map_err(|e| ExtractError::Png(path.clone(), e))?;
    if image.width != sheet.width || image.height != sheet.height {
        return Err(mismatch(
            &path,
            format!(
                "is {}x{}: expected {}x{}",
                image.width, image.height, sheet.width, sheet.height
            ),
        ));
    }

    let (palette_path, colors) = match sheet.palette {
        PaletteSource::EmbeddedPng => (
            path.clone(),
            png::decode_palette(&bytes).map_err(|e| ExtractError::Png(path.clone(), e))?,
        ),
        PaletteSource::Jasc(name) => {
            let palette_path = upstream.join(sheet.directory).join(name);
            let text = read_text(&palette_path)?;
            let colors =
                jasc_pal::parse(&text).map_err(|e| ExtractError::Pal(palette_path.clone(), e))?;
            (palette_path, colors)
        }
    };
    if colors.len() != sheet.colors {
        return Err(mismatch(
            &palette_path,
            format!("has {} colours: expected {}", colors.len(), sheet.colors),
        ));
    }
    if let Some(&pixel) = image
        .pixels
        .iter()
        .find(|&&pixel| usize::from(pixel) >= colors.len())
    {
        return Err(mismatch(
            &path,
            format!(
                "has pixel index {pixel}: its palette only has {} colours",
                colors.len()
            ),
        ));
    }

    writer.push(build_palette_entry(
        &palette_path,
        &colors,
        sheet.palette_id.to_owned(),
    )?);
    writer.push(build_image_entry(&path, sheet.image_id.to_owned(), image)?);
    Ok(())
}

fn mismatch(path: &Path, detail: String) -> ExtractError {
    ExtractError::StarterChooserAssetMismatch {
        path: path.to_path_buf(),
        detail,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use assets::{AssetPack, StarterSpecies};

    use super::super::{extract_to, repo_root, upstream_present};
    use super::{extract_sheet, extract_starter_chooser, extract_tilemap, SHEETS, TILEMAPS};
    use crate::extract::ExtractError;
    use pack_format::PackWriter;

    const MANIFEST_ENTRY_COUNT: usize = 14;

    fn manifest_ids() -> Vec<&'static str> {
        SHEETS
            .iter()
            .flat_map(|sheet| [sheet.image_id, sheet.palette_id])
            .chain(TILEMAPS.iter().map(|tilemap| tilemap.id))
            .collect()
    }

    fn scratch_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pokeemerald-rs-starter-chooser-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn manifest_names_six_pairs_and_two_tilemaps_with_unique_ids() {
        let ids = manifest_ids();
        assert_eq!(ids.len(), MANIFEST_ENTRY_COUNT);
        assert_eq!(ids.iter().collect::<BTreeSet<_>>().len(), ids.len());

        let expected: BTreeSet<&str> = [
            "starter-chooser/image/tiles",
            "starter-chooser/palette/tiles",
            "starter-chooser/image/pokeball_selection",
            "starter-chooser/palette/pokeball_selection",
            "starter-chooser/image/starter_circle",
            "starter-chooser/palette/starter_circle",
            "starter-chooser/raw/birch_bag",
            "starter-chooser/raw/birch_grass",
            "pokemon/treecko/front",
            "pokemon/treecko/palette/normal",
            "pokemon/torchic/front",
            "pokemon/torchic/palette/normal",
            "pokemon/mudkip/front",
            "pokemon/mudkip/palette/normal",
        ]
        .into_iter()
        .collect();
        assert_eq!(ids.into_iter().collect::<BTreeSet<_>>(), expected);
    }

    #[test]
    fn manifest_shapes_match_the_upstream_graphics() {
        let shapes: Vec<_> = SHEETS
            .iter()
            .map(|sheet| (sheet.png, sheet.width, sheet.height, sheet.colors))
            .collect();
        assert_eq!(
            shapes,
            [
                ("tiles.png", 128, 128, 32),
                ("pokeball_selection.png", 32, 128, 16),
                ("starter_circle.png", 64, 64, 16),
                ("front.png", 64, 64, 16),
                ("front.png", 64, 64, 16),
                ("front.png", 64, 64, 16),
            ]
        );
        let lengths: Vec<_> = TILEMAPS.iter().map(|tilemap| tilemap.byte_len).collect();
        assert_eq!(lengths, [1280, 2048]);
    }

    #[test]
    fn missing_sources_fail_instead_of_being_skipped() {
        let dir = scratch_dir("missing");
        let mut writer = PackWriter::new();

        let err = extract_starter_chooser(&dir, &mut writer).unwrap_err();
        assert!(matches!(err, ExtractError::ReadFailed(path, _) if path.starts_with(&dir)));
        assert_eq!(writer.len(), 0);

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn tilemaps_of_the_wrong_length_are_rejected() {
        let dir = scratch_dir("tilemap-length");
        let tilemap = TILEMAPS[0];
        let path = dir.join(tilemap.path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, vec![0u8; tilemap.byte_len - 2]).unwrap();
        let mut writer = PackWriter::new();

        let err = extract_tilemap(&dir, tilemap, &mut writer).unwrap_err();
        assert!(matches!(
            err,
            ExtractError::StarterChooserAssetMismatch { path: error_path, .. } if error_path == path
        ));

        std::fs::write(&path, vec![0u8; tilemap.byte_len]).unwrap();
        extract_tilemap(&dir, tilemap, &mut writer).unwrap();
        assert_eq!(writer.len(), 1);

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn sheets_that_are_not_png_are_rejected() {
        let dir = scratch_dir("bad-sheet");
        let sheet = SHEETS[0];
        let path = dir.join(sheet.directory).join(sheet.png);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"not a png").unwrap();
        let mut writer = PackWriter::new();

        let err = extract_sheet(&dir, sheet, &mut writer).unwrap_err();
        assert!(matches!(err, ExtractError::Png(error_path, _) if error_path == path));
        assert_eq!(writer.len(), 0);

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn starter_chooser_graphics_are_extracted() {
        assert!(upstream_present(), "run ./init.sh first");
        let path = std::env::temp_dir().join(format!(
            "pokeemerald-rs-extract-test-starter-chooser-{}.pack",
            std::process::id()
        ));
        extract_to(&path).expect("extraction should succeed against a real checkout");
        let pack = AssetPack::load(&path).unwrap();

        for id in manifest_ids() {
            assert!(
                pack.image(id).is_ok() || pack.palette(id).is_ok() || pack.raw(id).is_ok(),
                "missing pack entry id `{id}`"
            );
        }

        let chooser = pack.starter_chooser().unwrap();
        let upstream = repo_root().join("pokeemerald");
        for (tilemap, source) in [
            (chooser.bag_tilemap, "birch_bag.bin"),
            (chooser.grass_tilemap, "birch_grass.bin"),
        ] {
            let checkout = std::fs::read(upstream.join("graphics/starter_choose").join(source));
            assert_eq!(tilemap, checkout.unwrap().as_slice(), "{source}");
        }
        for starter in [
            StarterSpecies::Treecko,
            StarterSpecies::Torchic,
            StarterSpecies::Mudkip,
        ] {
            pack.starter_preview(starter).unwrap();
        }
        let _ = std::fs::remove_file(path);
    }
}
