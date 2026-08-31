//! Latin font glyph-sheet extraction.

use std::path::Path;

use super::pack::{PackEntry, PackKind, PackWriter};
use super::{png, read_file, ExtractError};

#[derive(Clone, Copy)]
struct FontSource {
    filename: &'static str,
    pack_id: &'static str,
}

const FONTS: [FontSource; 5] = [
    FontSource {
        filename: "latin_small.png",
        pack_id: "font/small/glyphs",
    },
    FontSource {
        filename: "latin_normal.png",
        pack_id: "font/normal/glyphs",
    },
    FontSource {
        filename: "latin_short.png",
        pack_id: "font/short/glyphs",
    },
    FontSource {
        filename: "latin_narrow.png",
        pack_id: "font/narrow/glyphs",
    },
    FontSource {
        filename: "latin_small_narrow.png",
        pack_id: "font/small_narrow/glyphs",
    },
];

const FONT_SHEET_WIDTH: u32 = 256;
const FONT_SHEET_HEIGHT: u32 = 512;
const FONT_SHEET_BIT_DEPTH: u8 = 2;

/// Extracts the configured Latin glyph sheets into the asset pack.
pub(super) fn extract_fonts(upstream: &Path, writer: &mut PackWriter) -> Result<(), ExtractError> {
    let fonts_dir = upstream.join("graphics/fonts");
    for font in FONTS {
        let path = fonts_dir.join(font.filename);
        let bytes = read_file(&path)?;
        let image = png::decode(&bytes).map_err(|e| ExtractError::Png(path.clone(), e))?;
        validate_font_sheet(&path, &image)?;
        writer.push(PackEntry {
            id: font.pack_id.to_owned(),
            kind: PackKind::Image {
                width: image.width,
                height: image.height,
                bit_depth: image.bit_depth,
            },
            payload: image.pixels,
        });
    }
    Ok(())
}

fn validate_font_sheet(path: &Path, image: &png::IndexedImage) -> Result<(), ExtractError> {
    if image.width != FONT_SHEET_WIDTH
        || image.height != FONT_SHEET_HEIGHT
        || image.bit_depth != FONT_SHEET_BIT_DEPTH
    {
        return Err(ExtractError::FontSheetWrongShape {
            path: path.to_path_buf(),
            width: image.width,
            height: image.height,
            bit_depth: image.bit_depth,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::{extract_to, png, upstream_present};
    use super::{
        validate_font_sheet, ExtractError, FontSource, FONTS, FONT_SHEET_BIT_DEPTH,
        FONT_SHEET_HEIGHT, FONT_SHEET_WIDTH,
    };

    fn scratch_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "pokeemerald-rs-extract-test-{name}-{}.pack",
            std::process::id()
        ))
    }

    fn is_normalized_font_pack_id(id: &str) -> bool {
        let Some(name) = id
            .strip_prefix("font/")
            .and_then(|rest| rest.strip_suffix("/glyphs"))
        else {
            return false;
        };
        !name.is_empty()
            && name.chars().all(|character| {
                character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
            })
    }

    #[test]
    fn font_sources_have_unique_filenames_and_normalized_pack_ids() {
        let mut filenames = std::collections::HashSet::new();
        let mut pack_ids = std::collections::HashSet::new();

        for FontSource { filename, pack_id } in FONTS {
            assert!(
                filenames.insert(filename),
                "duplicate filename `{filename}`"
            );
            assert!(pack_ids.insert(pack_id), "duplicate pack id `{pack_id}`");
            assert!(filename.starts_with("latin_"));
            assert!(std::path::Path::new(filename)
                .extension()
                .is_some_and(|extension| extension == "png"));
            assert!(is_normalized_font_pack_id(pack_id));
        }
    }

    #[test]
    fn font_sheets_must_match_the_documented_shape() {
        let path = std::path::Path::new("graphics/fonts/latin_normal.png");
        let sheet = |width: u32, height: u32, bit_depth: u8| png::IndexedImage {
            width,
            height,
            bit_depth,
            pixels: Vec::new(),
            palette: Vec::new(),
        };

        validate_font_sheet(
            path,
            &sheet(FONT_SHEET_WIDTH, FONT_SHEET_HEIGHT, FONT_SHEET_BIT_DEPTH),
        )
        .unwrap();

        for (width, height, bit_depth) in [
            (128, FONT_SHEET_HEIGHT, FONT_SHEET_BIT_DEPTH),
            (FONT_SHEET_WIDTH, 256, FONT_SHEET_BIT_DEPTH),
            (FONT_SHEET_WIDTH, FONT_SHEET_HEIGHT, 4),
        ] {
            let err = validate_font_sheet(path, &sheet(width, height, bit_depth)).unwrap_err();
            assert!(
                matches!(
                    &err,
                    ExtractError::FontSheetWrongShape {
                        path: error_path,
                        width: error_width,
                        height: error_height,
                        bit_depth: error_bit_depth,
                    } if error_path == path
                        && *error_width == width
                        && *error_height == height
                        && *error_bit_depth == bit_depth
                ),
                "wrong error for a {width}x{height}/{bit_depth}bpp sheet"
            );
        }
    }

    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn font_glyph_sheets_are_extracted() {
        assert!(upstream_present(), "run ./init.sh first");
        let path = scratch_path("fonts");
        let report = extract_to(&path).expect("extraction should succeed against a real checkout");
        let bytes = std::fs::read(&report.output_path).unwrap();
        for font in FONTS {
            assert!(
                bytes
                    .windows(font.pack_id.len())
                    .any(|window| window == font.pack_id.as_bytes()),
                "missing pack entry id `{}`",
                font.pack_id
            );
        }
        let _ = std::fs::remove_file(report.output_path);
    }
}
