//! Text-window frame extraction (S-4, issue #114): `graphics/text_window/`.
//!
//! Everything in the directory is extracted — all 20 numbered border-frame
//! tile sheets (`1.png`..`20.png`, upstream `sWindowFrames` /
//! `WINDOW_FRAMES_COUNT`, `pokeemerald/src/text_window.c`), the default
//! message-box tile sheet (`message_box.png`, upstream `gMessageBox_Gfx`),
//! and the four extra textbox colour palettes
//! (`text_pal1.pal`..`text_pal4.pal`, upstream `sTextWindowPalettes`).
//! Unlike tilesets/sprites, the frame and message-box PNGs have no sibling
//! `.pal` file of their own — upstream's own `INCGFX_U16(..., ".gbapal")`
//! build rule for them reads the palette straight out of each PNG's own
//! `PLTE` chunk, so this pipeline does too, via [`png::decode_palette`]
//! (see that function's docs for why it's a separate read path from
//! [`png::decode`]). This is every file in the directory, so
//! `graphics/text_window` closes fully `ported` in the ledger.
//!
//! Each frame's image and palette are validated **as a pair** before either
//! entry is serialized: the palette must be exactly one 16-colour GBA bank,
//! and every pixel index must be mappable through it (an 8-bit-indexed PNG
//! can hold indices >= 16 alongside a valid 16-entry `PLTE`).
//! `crates/assets`' typed accessors re-run the same checks on read — packs
//! are untrusted input there.

use std::path::Path;

use super::pack::{PackEntry, PackKind, PackWriter};
use super::{jasc_pal, png, read_file, read_text, ExtractError};

/// Filename stems for every PNG required from `graphics/text_window/`.
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
    "message_box",
];

/// Filename stems for every standalone palette required from
/// `graphics/text_window/`.
const TEXT_WINDOW_PALETTE_STEMS: [&str; 4] = ["text_pal1", "text_pal2", "text_pal3", "text_pal4"];

/// Every text-window palette occupies one 16-colour GBA palette bank.
const TEXT_WINDOW_PALETTE_COLORS: usize = 16;

/// Every numbered border frame is a 3x3 grid of 8x8 tiles — a 24x24
/// source sheet (upstream `sWindowFrames`).
const FRAME_DIMENSIONS: (u32, u32) = (24, 24);

/// `message_box.png` (upstream `gMessageBox_Gfx`) is a 56x16 (7x2-tile)
/// strip, distinct from the border frames' 24x24 shape.
const MESSAGE_BOX_DIMENSIONS: (u32, u32) = (56, 16);

/// The exact shape `stem`'s image must have (see the two constants above).
fn expected_dimensions(stem: &str) -> (u32, u32) {
    if stem == "message_box" {
        MESSAGE_BOX_DIMENSIONS
    } else {
        FRAME_DIMENSIONS
    }
}

/// Extract `graphics/text_window/`'s full contents (see the module docs).
pub(super) fn extract_text_window(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let dir = upstream.join("graphics/text_window");

    validate_text_window_manifest(&dir)?;

    for stem in TEXT_WINDOW_IMAGE_STEMS {
        let path = dir.join(format!("{stem}.png"));
        let bytes = read_file(&path)?;
        let image = png::decode(&bytes).map_err(|e| ExtractError::Png(path.clone(), e))?;
        let colors = png::decode_palette(&bytes).map_err(|e| ExtractError::Png(path.clone(), e))?;
        // Validate the pair as a unit, before either entry is serialized
        // (see the module docs): exact per-kind shape first (the read
        // side rejects any other shape), then the colour count before the
        // pixel check so an undersized palette reports as a palette
        // problem, not as a pixel out of range.
        validate_text_window_dimensions(&path, &image, expected_dimensions(stem))?;
        if colors.len() != TEXT_WINDOW_PALETTE_COLORS {
            return Err(ExtractError::TextWindowPaletteWrongColorCount(
                path,
                colors.len(),
            ));
        }
        validate_text_window_pixels(&path, &image.pixels, colors.len())?;
        writer.push(PackEntry {
            id: format!("text-window/image/{stem}"),
            kind: PackKind::Image {
                width: image.width,
                height: image.height,
                bit_depth: image.bit_depth,
            },
            payload: image.pixels,
        });
        push_text_window_palette_entry(
            &path,
            &colors,
            format!("text-window/palette/{stem}"),
            writer,
        )?;
    }
    for stem in TEXT_WINDOW_PALETTE_STEMS {
        let path = dir.join(format!("{stem}.pal"));
        let text = read_text(&path)?;
        let colors = jasc_pal::parse(&text).map_err(|e| ExtractError::Pal(path.clone(), e))?;
        push_text_window_palette_entry(
            &path,
            &colors,
            format!("text-window/palette/{stem}"),
            writer,
        )?;
    }
    Ok(())
}

/// Reject a text-window bitmap that is not the exact shape its kind
/// requires (see [`extract_text_window`]'s pairing validation and
/// [`expected_dimensions`]) — the read side rejects any other shape, so
/// extraction must not produce one.
fn validate_text_window_dimensions(
    path: &Path,
    image: &png::IndexedImage,
    (expected_width, expected_height): (u32, u32),
) -> Result<(), ExtractError> {
    if image.width != expected_width || image.height != expected_height {
        return Err(ExtractError::TextWindowImageWrongDimensions {
            path: path.to_path_buf(),
            width: image.width,
            height: image.height,
            expected_width,
            expected_height,
        });
    }
    Ok(())
}

/// Reject any text-window pixel index that cannot be mapped through the
/// frame's own bundled palette (see [`extract_text_window`]'s pairing
/// validation).
fn validate_text_window_pixels(
    path: &Path,
    pixels: &[u8],
    palette_len: usize,
) -> Result<(), ExtractError> {
    match pixels
        .iter()
        .find(|&&pixel| usize::from(pixel) >= palette_len)
    {
        Some(&pixel) => Err(ExtractError::TextWindowPixelOutsidePalette(
            path.to_path_buf(),
            pixel,
            palette_len,
        )),
        None => Ok(()),
    }
}

fn push_text_window_palette_entry(
    path: &Path,
    colors: &[jasc_pal::Rgb888],
    id: String,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    if colors.len() != TEXT_WINDOW_PALETTE_COLORS {
        return Err(ExtractError::TextWindowPaletteWrongColorCount(
            path.to_path_buf(),
            colors.len(),
        ));
    }
    writer.push(super::build_palette_entry(path, colors, id)?);
    Ok(())
}

fn validate_text_window_manifest(dir: &Path) -> Result<(), ExtractError> {
    for (stems, extension) in [
        (TEXT_WINDOW_IMAGE_STEMS.as_slice(), "png"),
        (TEXT_WINDOW_PALETTE_STEMS.as_slice(), "pal"),
    ] {
        for stem in stems {
            let path = dir.join(format!("{stem}.{extension}"));
            if !path.is_file() {
                return Err(ExtractError::MissingTextWindowAsset(path));
            }
        }
    }

    let entries = std::fs::read_dir(dir)
        .map_err(|e| ExtractError::ReadFailed(dir.to_path_buf(), e.to_string()))?;
    let mut paths = entries
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ExtractError::ReadFailed(dir.to_path_buf(), e.to_string()))?;
    paths.sort();
    for path in paths {
        let stem = path.file_stem().and_then(|stem| stem.to_str());
        let extension = path.extension().and_then(|extension| extension.to_str());
        let is_expected = match extension {
            Some("png") => stem.is_some_and(|stem| TEXT_WINDOW_IMAGE_STEMS.contains(&stem)),
            Some("pal") => stem.is_some_and(|stem| TEXT_WINDOW_PALETTE_STEMS.contains(&stem)),
            _ => false,
        };
        if !is_expected {
            return Err(ExtractError::UnexpectedTextWindowAsset(path));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::{extract_to, png, upstream_present};
    use super::{
        expected_dimensions, push_text_window_palette_entry, validate_text_window_dimensions,
        validate_text_window_manifest, validate_text_window_pixels, ExtractError,
        TEXT_WINDOW_IMAGE_STEMS, TEXT_WINDOW_PALETTE_COLORS, TEXT_WINDOW_PALETTE_STEMS,
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
            let mut writer = super::super::pack::PackWriter::new();
            let err = push_text_window_palette_entry(
                path,
                &colors,
                "text-window/palette/example".to_owned(),
                &mut writer,
            )
            .unwrap_err();
            assert!(
                matches!(
                    err,
                    ExtractError::TextWindowPaletteWrongColorCount(error_path, count)
                        if error_path == path && count == actual
                ),
                "wrong error for {actual}-colour text-window palette"
            );
            assert_eq!(writer.len(), 0, "invalid palette must not be serialized");
        }

        let colors = vec![color; TEXT_WINDOW_PALETTE_COLORS];
        let mut writer = super::super::pack::PackWriter::new();
        push_text_window_palette_entry(
            path,
            &colors,
            "text-window/palette/example".to_owned(),
            &mut writer,
        )
        .unwrap();
        assert_eq!(writer.len(), 1);
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

        // Numbered border frames are 24x24; message_box is 56x16.
        assert_eq!(expected_dimensions("7"), (24, 24));
        assert_eq!(expected_dimensions("message_box"), (56, 16));
        validate_text_window_dimensions(path, &image(24, 24), expected_dimensions("7")).unwrap();
        validate_text_window_dimensions(path, &image(56, 16), expected_dimensions("message_box"))
            .unwrap();

        // Anything else — including a self-consistent single 8x8 tile and
        // zero-area shapes — must fail at extraction, not later on read.
        for (width, height) in [(8, 8), (56, 16), (0, 2), (2, 0), (0, 0)] {
            let err =
                validate_text_window_dimensions(path, &image(width, height), (24, 24)).unwrap_err();
            assert!(
                matches!(
                    err,
                    ExtractError::TextWindowImageWrongDimensions {
                        path: error_path,
                        width: error_width,
                        height: error_height,
                        expected_width: 24,
                        expected_height: 24,
                    } if error_path == path && error_width == width && error_height == height
                ),
                "wrong error for a {width}x{height} text-window image"
            );
        }
    }

    #[test]
    fn text_window_pixels_must_map_through_their_palette() {
        let path = std::path::Path::new("graphics/text_window/example.png");

        // Every index strictly below the palette length is fine, including
        // the 4bpp maximum on a full 16-entry palette.
        validate_text_window_pixels(path, &[0, 3, 15], TEXT_WINDOW_PALETTE_COLORS).unwrap();
        validate_text_window_pixels(path, &[], TEXT_WINDOW_PALETTE_COLORS).unwrap();

        // An 8-bit-indexed PNG can carry indices >= 16 alongside a valid
        // 16-entry PLTE; the first offending pixel is reported.
        for (pixels, expected_pixel) in [([0u8, 16, 3], 16u8), ([255, 16, 3], 255)] {
            let err =
                validate_text_window_pixels(path, &pixels, TEXT_WINDOW_PALETTE_COLORS).unwrap_err();
            assert!(
                matches!(
                    err,
                    ExtractError::TextWindowPixelOutsidePalette(error_path, pixel, palette_len)
                        if error_path == path
                            && pixel == expected_pixel
                            && palette_len == TEXT_WINDOW_PALETTE_COLORS
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
