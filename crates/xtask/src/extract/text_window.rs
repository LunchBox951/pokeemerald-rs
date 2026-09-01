use std::path::Path;

use super::{build_image_entry, jasc_pal, png, read_file, read_text, ExtractError};
use pack_format::PackWriter;

const TEXT_WINDOW_DIRECTORY: &str = "graphics/text_window";
const PNG_EXTENSION: &str = "png";
const PALETTE_EXTENSION: &str = "pal";
const MESSAGE_BOX_STEM: &str = "message_box";

const TEXT_WINDOW_IMAGE_STEMS: [&str; 21] = [
    "1",
    "2",
    "3",
    "4",
    "5",
    "6",
    "7",
    "8",
    "9",
    "10",
    "11",
    "12",
    "13",
    "14",
    "15",
    "16",
    "17",
    "18",
    "19",
    "20",
    MESSAGE_BOX_STEM,
];

const TEXT_WINDOW_PALETTE_STEMS: [&str; 4] = ["text_pal1", "text_pal2", "text_pal3", "text_pal4"];

const COLORS_PER_GBA_PALETTE_BANK: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ImageDimensions {
    width: u32,
    height: u32,
}

const BORDER_FRAME_SHEET_DIMENSIONS: ImageDimensions = ImageDimensions {
    width: 24,
    height: 24,
};
const MESSAGE_BOX_SHEET_DIMENSIONS: ImageDimensions = ImageDimensions {
    width: 56,
    height: 16,
};

fn expected_dimensions(stem: &str) -> ImageDimensions {
    if stem == MESSAGE_BOX_STEM {
        MESSAGE_BOX_SHEET_DIMENSIONS
    } else {
        BORDER_FRAME_SHEET_DIMENSIONS
    }
}

/// Extracts every text-window frame image and palette into the asset pack,
/// rejecting a directory whose contents differ from the manifests.
pub(super) fn extract_text_window(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let directory = upstream.join(TEXT_WINDOW_DIRECTORY);

    validate_text_window_manifest(&directory)?;

    for stem in TEXT_WINDOW_IMAGE_STEMS {
        let path = asset_path(&directory, stem, PNG_EXTENSION);
        let (image_entry, palette_entry) = decode_text_window_image_pair(&path, stem)?;
        writer.push(image_entry);
        writer.push(palette_entry);
    }

    for stem in TEXT_WINDOW_PALETTE_STEMS {
        let path = asset_path(&directory, stem, PALETTE_EXTENSION);
        let text = read_text(&path)?;
        let colors = jasc_pal::parse(&text).map_err(|e| ExtractError::Pal(path.clone(), e))?;
        writer.push(build_text_window_palette_entry(
            &path,
            &colors,
            text_window_palette_id(stem),
        )?);
    }
    Ok(())
}

fn decode_text_window_image_pair(
    path: &Path,
    stem: &str,
) -> Result<(pack_format::PackEntry, pack_format::PackEntry), ExtractError> {
    let bytes = read_file(path)?;
    let image =
        png::decode(&bytes).map_err(|error| ExtractError::Png(path.to_path_buf(), error))?;
    let colors = png::decode_palette(&bytes)
        .map_err(|error| ExtractError::Png(path.to_path_buf(), error))?;

    validate_text_window_dimensions(path, &image, expected_dimensions(stem))?;
    let palette_entry =
        build_text_window_palette_entry(path, &colors, text_window_palette_id(stem))?;
    validate_text_window_pixels(path, &image.pixels, colors.len())?;
    let image_entry = build_image_entry(path, text_window_image_id(stem), image)?;
    Ok((image_entry, palette_entry))
}

fn validate_text_window_dimensions(
    path: &Path,
    image: &png::IndexedImage,
    expected: ImageDimensions,
) -> Result<(), ExtractError> {
    if image.width != expected.width || image.height != expected.height {
        return Err(ExtractError::TextWindowImageWrongDimensions {
            path: path.to_path_buf(),
            width: image.width,
            height: image.height,
            expected_width: expected.width,
            expected_height: expected.height,
        });
    }
    Ok(())
}

fn validate_text_window_pixels(
    path: &Path,
    pixels: &[u8],
    palette_len: usize,
) -> Result<(), ExtractError> {
    if let Some(&pixel) = pixels
        .iter()
        .find(|&&pixel| usize::from(pixel) >= palette_len)
    {
        return Err(ExtractError::TextWindowPixelOutsidePalette(
            path.to_path_buf(),
            pixel,
            palette_len,
        ));
    }
    Ok(())
}

fn validate_text_window_palette_color_count(
    path: &Path,
    color_count: usize,
) -> Result<(), ExtractError> {
    if color_count != COLORS_PER_GBA_PALETTE_BANK {
        return Err(ExtractError::TextWindowPaletteWrongColorCount(
            path.to_path_buf(),
            color_count,
        ));
    }
    Ok(())
}

fn build_text_window_palette_entry(
    path: &Path,
    colors: &[jasc_pal::Rgb888],
    id: String,
) -> Result<pack_format::PackEntry, ExtractError> {
    validate_text_window_palette_color_count(path, colors.len())?;
    super::build_palette_entry(path, colors, id)
}

fn text_window_image_id(stem: &str) -> String {
    format!("text-window/image/{stem}")
}

fn text_window_palette_id(stem: &str) -> String {
    format!("text-window/palette/{stem}")
}

fn asset_path(directory: &Path, stem: &str, extension: &str) -> std::path::PathBuf {
    directory.join(format!("{stem}.{extension}"))
}

fn validate_text_window_manifest(directory: &Path) -> Result<(), ExtractError> {
    for (stems, extension) in [
        (TEXT_WINDOW_IMAGE_STEMS.as_slice(), PNG_EXTENSION),
        (TEXT_WINDOW_PALETTE_STEMS.as_slice(), PALETTE_EXTENSION),
    ] {
        for stem in stems {
            let path = asset_path(directory, stem, extension);
            if !path.is_file() {
                return Err(ExtractError::MissingTextWindowAsset(path));
            }
        }
    }

    let entries = std::fs::read_dir(directory)
        .map_err(|error| ExtractError::ReadFailed(directory.to_path_buf(), error.to_string()))?;
    let mut paths = entries
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| ExtractError::ReadFailed(directory.to_path_buf(), error.to_string()))?;
    paths.sort();
    for path in paths {
        if !is_expected_text_window_asset(&path) {
            return Err(ExtractError::UnexpectedTextWindowAsset(path));
        }
    }
    Ok(())
}

fn is_expected_text_window_asset(path: &Path) -> bool {
    let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
        return false;
    };
    match path.extension().and_then(|extension| extension.to_str()) {
        Some(PNG_EXTENSION) => TEXT_WINDOW_IMAGE_STEMS.contains(&stem),
        Some(PALETTE_EXTENSION) => TEXT_WINDOW_PALETTE_STEMS.contains(&stem),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::super::{extract_to, png, upstream_present};
    use super::{
        build_text_window_palette_entry, expected_dimensions, validate_text_window_dimensions,
        validate_text_window_manifest, validate_text_window_pixels, ExtractError, ImageDimensions,
        COLORS_PER_GBA_PALETTE_BANK, TEXT_WINDOW_IMAGE_STEMS, TEXT_WINDOW_PALETTE_STEMS,
    };

    fn scratch_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "pokeemerald-rs-extract-test-{name}-{}.pack",
            std::process::id()
        ))
    }

    #[test]
    fn text_window_manifest_rejects_each_missing_required_file() {
        let dir = std::env::temp_dir().join(format!(
            "pokeemerald-rs-text-window-manifest-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let required: Vec<_> = TEXT_WINDOW_IMAGE_STEMS
            .iter()
            .map(|stem| format!("{stem}.png"))
            .chain(
                TEXT_WINDOW_PALETTE_STEMS
                    .iter()
                    .map(|stem| format!("{stem}.pal")),
            )
            .collect();
        for filename in &required {
            std::fs::write(dir.join(filename), []).unwrap();
        }

        for filename in &required {
            let missing = dir.join(filename);
            std::fs::remove_file(&missing).unwrap();
            let err = validate_text_window_manifest(&dir).unwrap_err();
            assert!(
                matches!(err, ExtractError::MissingTextWindowAsset(path) if path == missing),
                "wrong error for missing `{filename}`"
            );
            std::fs::write(&missing, []).unwrap();
        }

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn text_window_manifest_rejects_unexpected_assets() {
        let dir = std::env::temp_dir().join(format!(
            "pokeemerald-rs-text-window-unexpected-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        for stem in TEXT_WINDOW_IMAGE_STEMS {
            std::fs::write(dir.join(format!("{stem}.png")), []).unwrap();
        }
        for stem in TEXT_WINDOW_PALETTE_STEMS {
            std::fs::write(dir.join(format!("{stem}.pal")), []).unwrap();
        }

        for filename in ["new_frame.png", "text_pal5.pal"] {
            let unexpected = dir.join(filename);
            std::fs::write(&unexpected, []).unwrap();
            let err = validate_text_window_manifest(&dir).unwrap_err();
            assert!(
                matches!(err, ExtractError::UnexpectedTextWindowAsset(path) if path == unexpected),
                "wrong error for unexpected text-window asset `{filename}`"
            );
            std::fs::remove_file(unexpected).unwrap();
        }

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn text_window_palettes_require_exactly_sixteen_colors() {
        let path = std::path::Path::new("graphics/text_window/example.png");
        let color = super::jasc_pal::Rgb888 { r: 0, g: 0, b: 0 };

        for actual in [0, 1, 15, 17] {
            let colors = vec![color; actual];
            let result = build_text_window_palette_entry(
                path,
                &colors,
                "text-window/palette/example".to_owned(),
            );
            let Err(err) = result else {
                panic!("accepted a text-window palette with {actual} colors");
            };
            assert!(
                matches!(
                    err,
                    ExtractError::TextWindowPaletteWrongColorCount(error_path, count)
                        if error_path == path && count == actual
                ),
                "wrong error for {actual}-colour text-window palette"
            );
        }

        let colors = vec![color; COLORS_PER_GBA_PALETTE_BANK];
        let entry = build_text_window_palette_entry(
            path,
            &colors,
            "text-window/palette/example".to_owned(),
        )
        .unwrap();
        assert_eq!(entry.id, "text-window/palette/example");
        assert!(matches!(
            entry.kind,
            pack_format::EntryKind::Palette { color_count }
                if usize::from(color_count) == COLORS_PER_GBA_PALETTE_BANK
        ));
    }

    #[test]
    fn text_window_images_must_match_their_kind_shape() {
        let path = std::path::Path::new("graphics/text_window/example.png");
        let image = |width: u32, height: u32| png::IndexedImage {
            width,
            height,
            bit_depth: 4,
            pixels: Vec::new(),
            palette: Vec::new(),
        };

        let frame_dimensions = expected_dimensions("7");
        let message_box_dimensions = expected_dimensions("message_box");
        assert_eq!(
            frame_dimensions,
            ImageDimensions {
                width: 24,
                height: 24
            }
        );
        assert_eq!(
            message_box_dimensions,
            ImageDimensions {
                width: 56,
                height: 16,
            }
        );
        validate_text_window_dimensions(path, &image(24, 24), frame_dimensions).unwrap();
        validate_text_window_dimensions(path, &image(56, 16), message_box_dimensions).unwrap();

        for (width, height) in [(8, 8), (56, 16), (0, 2), (2, 0), (0, 0)] {
            let err =
                validate_text_window_dimensions(path, &image(width, height), frame_dimensions)
                    .unwrap_err();
            assert!(
                matches!(
                    err,
                    ExtractError::TextWindowImageWrongDimensions {
                        path: error_path,
                        width: error_width,
                        height: error_height,
                        expected_width,
                        expected_height,
                    } if error_path == path
                        && error_width == width
                        && error_height == height
                        && expected_width == frame_dimensions.width
                        && expected_height == frame_dimensions.height
                ),
                "wrong error for a {width}x{height} text-window image"
            );
        }
    }

    #[test]
    fn text_window_pixels_must_map_through_their_palette() {
        let path = std::path::Path::new("graphics/text_window/example.png");

        validate_text_window_pixels(path, &[0, 3, 15], COLORS_PER_GBA_PALETTE_BANK).unwrap();
        validate_text_window_pixels(path, &[], COLORS_PER_GBA_PALETTE_BANK).unwrap();

        for (pixels, expected_pixel) in [([0u8, 16, 3], 16u8), ([255, 16, 3], 255)] {
            let err = validate_text_window_pixels(path, &pixels, COLORS_PER_GBA_PALETTE_BANK)
                .unwrap_err();
            assert!(
                matches!(
                    err,
                    ExtractError::TextWindowPixelOutsidePalette(error_path, pixel, palette_len)
                        if error_path == path
                            && pixel == expected_pixel
                            && palette_len == COLORS_PER_GBA_PALETTE_BANK
                ),
                "wrong error for pixel {expected_pixel} outside a 16-colour palette"
            );
        }
    }

    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn text_window_frames_are_extracted() {
        assert!(upstream_present(), "run ./init.sh first");
        let path = scratch_path("text-window");
        let report = extract_to(&path).expect("extraction should succeed against a real checkout");
        let bytes = std::fs::read(&report.output_path).unwrap();

        let mut expected_ids: Vec<String> = Vec::new();
        for n in 1..=20 {
            expected_ids.push(format!("text-window/image/{n}"));
            expected_ids.push(format!("text-window/palette/{n}"));
        }
        expected_ids.push("text-window/image/message_box".to_owned());
        expected_ids.push("text-window/palette/message_box".to_owned());
        for n in 1..=4 {
            expected_ids.push(format!("text-window/palette/text_pal{n}"));
        }

        for id in expected_ids {
            assert!(
                bytes
                    .windows(id.len())
                    .any(|window| window == id.as_bytes()),
                "missing pack entry id `{id}`"
            );
        }
        let _ = std::fs::remove_file(report.output_path);
    }
}
