use std::path::Path;

use super::{build_image_entry, png, read_file, ExtractError};
use super::{build_palette_entry, push_raw_bytes};
use pack_format::PackWriter;

const CHOOSER_DIRECTORY: &str = "graphics/starter_choose";

#[derive(Clone, Copy, Debug)]
/// An indexed sheet whose palette is its own PNG's `PLTE` chunk.
struct SheetSource {
    directory: &'static str,
    png: &'static str,
    image_id: &'static str,
    palette_id: &'static str,
    width: u32,
    height: u32,
    /// The PNG bit depth the typed accessor requires of this sheet.
    bit_depth: u8,
    colors: usize,
}

#[derive(Clone, Copy, Debug)]
struct TilemapSource {
    path: &'static str,
    id: &'static str,
    byte_len: usize,
}

/// Image and palette pairs, relative to the checkout's `graphics/`: the
/// chooser sheets (`starter_choose.c:54-61`, `400-422`). The starters' shared
/// battler art (`629-635`) is the battler manifest's.
const SHEETS: [SheetSource; 3] = [
    SheetSource {
        directory: CHOOSER_DIRECTORY,
        png: "tiles.png",
        image_id: "starter-chooser/image/tiles",
        palette_id: "starter-chooser/palette/tiles",
        width: 128,
        height: 128,
        bit_depth: 8,
        colors: 32,
    },
    SheetSource {
        directory: CHOOSER_DIRECTORY,
        png: "pokeball_selection.png",
        image_id: "starter-chooser/image/pokeball_selection",
        palette_id: "starter-chooser/palette/pokeball_selection",
        width: 32,
        height: 128,
        bit_depth: 4,
        colors: 16,
    },
    SheetSource {
        directory: CHOOSER_DIRECTORY,
        png: "starter_circle.png",
        image_id: "starter-chooser/image/starter_circle",
        palette_id: "starter-chooser/palette/starter_circle",
        width: 64,
        height: 64,
        bit_depth: 4,
        colors: 16,
    },
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

/// Extracts the starter chooser's fixed manifest: three image and palette pairs
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
    extract_tilemap_with(upstream, tilemap, &mut read_file, writer)
}

fn extract_tilemap_with(
    upstream: &Path,
    tilemap: TilemapSource,
    read: &mut dyn FnMut(&Path) -> Result<Vec<u8>, ExtractError>,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let path = upstream.join(tilemap.path);
    let payload = read(&path)?;
    push_tilemap(&path, tilemap, payload, writer)
}

/// The buffer validated is the buffer queued: a second read of the path
/// could publish bytes that changed after the length check (#2105).
fn push_tilemap(
    path: &Path,
    tilemap: TilemapSource,
    payload: Vec<u8>,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let len = payload.len();
    if len != tilemap.byte_len {
        return Err(mismatch(
            path,
            format!("is {len} bytes: expected {}", tilemap.byte_len),
        ));
    }
    push_raw_bytes(tilemap.id.to_owned(), payload, writer);
    Ok(())
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
    if image.bit_depth != sheet.bit_depth {
        return Err(mismatch(
            &path,
            format!(
                "is {}-bit: expected {}-bit",
                image.bit_depth, sheet.bit_depth
            ),
        ));
    }

    let palette_path = path.clone();
    let colors = png::decode_palette(&bytes).map_err(|e| ExtractError::Png(path.clone(), e))?;
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
    use std::path::Path;

    use assets::{AssetPack, StarterSpecies};

    use super::super::png::tests::indexed_png_from_raw_with_plte;
    use super::super::{extract_to, repo_root, upstream_present};
    use super::{
        extract_sheet, extract_starter_chooser, extract_tilemap, extract_tilemap_with,
        push_tilemap, SheetSource, SHEETS, TILEMAPS,
    };
    use crate::extract::ExtractError;
    use pack_format::PackWriter;

    const MANIFEST_ENTRY_COUNT: usize = 8;

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
    fn manifest_names_three_pairs_and_two_tilemaps_with_unique_ids() {
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
    fn tilemap_pack_entry_holds_the_validated_bytes() {
        for tilemap in TILEMAPS {
            let expected: Vec<u8> = (0..=u8::MAX).cycle().take(tilemap.byte_len).collect();
            let mut reads = 0;
            let mut read = |_: &Path| {
                reads += 1;
                // Any read after the first sees a different, wrong-length file.
                Ok(if reads == 1 {
                    expected.clone()
                } else {
                    vec![0xFF; tilemap.byte_len - 2]
                })
            };

            let mut writer = PackWriter::new();
            extract_tilemap_with(Path::new("upstream"), tilemap, &mut read, &mut writer).unwrap();
            assert_eq!(reads, 1);
            let pack = writer.finish().unwrap();
            let entries = pack_format::parse_directory(&pack).unwrap();
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].id, tilemap.id);
            assert_eq!(entries[0].length, tilemap.byte_len);
            assert_eq!(
                &pack[entries[0].offset..entries[0].offset + entries[0].length],
                expected.as_slice()
            );
        }
    }

    #[test]
    fn tilemap_bytes_of_the_wrong_length_are_not_queued() {
        let mut writer = PackWriter::new();
        let err =
            push_tilemap(Path::new("t.bin"), TILEMAPS[0], vec![0; 3], &mut writer).unwrap_err();
        assert!(matches!(
            err,
            ExtractError::StarterChooserAssetMismatch { .. }
        ));
        assert_eq!(writer.len(), 0);
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

    /// A sheet-shaped PNG at `bit_depth` with the sheet's own palette size.
    fn sheet_png(sheet: SheetSource, bit_depth: u8) -> Vec<u8> {
        let row_bytes = (sheet.width as usize * usize::from(bit_depth)).div_ceil(8);
        let mut raw = Vec::new();
        for _ in 0..sheet.height {
            raw.push(0);
            raw.resize(raw.len() + row_bytes, 0);
        }
        let plte = vec![7; sheet.colors * 3];
        indexed_png_from_raw_with_plte(bit_depth, sheet.width, sheet.height, &raw, &plte)
    }

    #[test]
    fn sheets_of_the_wrong_bit_depth_are_rejected() {
        let required = [
            ("tiles.png", 8u8),
            ("pokeball_selection.png", 4),
            ("starter_circle.png", 4),
        ];
        for (sheet, (png, depth)) in SHEETS.into_iter().zip(required) {
            assert_eq!(sheet.png, png);
            for wrong in [2u8, 4, 8].into_iter().filter(|&d| d != depth) {
                let dir = scratch_dir("bit-depth");
                let path = dir.join(sheet.directory).join(sheet.png);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(&path, sheet_png(sheet, wrong)).unwrap();
                let mut writer = PackWriter::new();

                let err = extract_sheet(&dir, sheet, &mut writer).unwrap_err();
                match &err {
                    ExtractError::StarterChooserAssetMismatch {
                        path: error_path,
                        detail,
                    } => {
                        assert_eq!(error_path, &path);
                        assert!(
                            detail.contains(&format!("{wrong}-bit"))
                                && detail.contains(&format!("{depth}-bit")),
                            "{png} at {wrong}: {detail}"
                        );
                    }
                    other => panic!("{png} at {wrong}-bit: wrong error {other:?}"),
                }
                assert_eq!(writer.len(), 0);

                std::fs::write(&path, sheet_png(sheet, depth)).unwrap();
                extract_sheet(&dir, sheet, &mut writer).unwrap();
                assert_eq!(writer.len(), 2);
                std::fs::remove_dir_all(dir).unwrap();
            }
        }
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
