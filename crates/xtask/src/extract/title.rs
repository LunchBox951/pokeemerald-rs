//! Extracts the title screen graphics.

use std::path::{Path, PathBuf};

use super::scope::{
    TITLE_SCREEN_EMBEDDED_PALETTE_SHEETS, TITLE_SCREEN_FILE_PALETTES, TITLE_SCREEN_IMAGES,
    TITLE_SCREEN_PALETTE_CUTS, TITLE_SCREEN_TILEMAPS,
};
use super::{
    build_image_entry, build_palette_entry, jasc_pal, png, push_palette_entry_with_limit,
    push_raw_entry, read_file, ExtractError,
};
use pack_format::PackWriter;

/// Refuse a title directory missing a source the generator requires.
fn require_title_sources(dir: &Path, entries: &[PathBuf]) -> Result<(), ExtractError> {
    let required = TITLE_SCREEN_IMAGES
        .iter()
        .chain(&TITLE_SCREEN_EMBEDDED_PALETTE_SHEETS)
        .map(|stem| format!("{stem}.png"))
        .chain(
            TITLE_SCREEN_TILEMAPS
                .iter()
                .map(|stem| format!("{stem}.bin")),
        )
        .chain(
            TITLE_SCREEN_FILE_PALETTES
                .iter()
                .map(|stem| format!("{stem}.pal")),
        );
    for name in required {
        if !entries
            .iter()
            .any(|p| p.file_name().is_some_and(|f| *f == *name))
        {
            return Err(ExtractError::ReadFailed(
                dir.join(&name),
                "required title screen source is missing".to_owned(),
            ));
        }
    }
    Ok(())
}

pub(super) fn extract_title_screen(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let dir = upstream.join("graphics/title_screen");

    let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)
        .and_then(|it| {
            it.map(|entry| entry.map(|e| e.path()))
                .collect::<std::io::Result<_>>()
        })
        .map_err(|e| ExtractError::ReadFailed(dir.clone(), e.to_string()))?;
    entries.sort();
    require_title_sources(&dir, &entries)?;

    for path in entries {
        let Some(stem) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
            continue;
        };
        match path.extension().and_then(|e| e.to_str()) {
            Some("png") => {
                let bytes = read_file(&path)?;
                let image = png::decode(&bytes).map_err(|e| ExtractError::Png(path.clone(), e))?;
                if TITLE_SCREEN_EMBEDDED_PALETTE_SHEETS.contains(&stem.as_str()) {
                    if image.palette.is_empty() {
                        return Err(ExtractError::MissingEmbeddedPalette(path.clone()));
                    }
                    let colors: Vec<jasc_pal::Rgb888> = image
                        .palette
                        .iter()
                        .map(|&[r, g, b]| jasc_pal::Rgb888 { r, g, b })
                        .collect();
                    let entry =
                        build_palette_entry(&path, &colors, format!("title/palette/{stem}"))?;
                    writer.push(entry);
                }
                writer.push(build_image_entry(
                    &path,
                    format!("title/image/{stem}"),
                    image,
                )?);
            }
            Some("pal") => {
                let color_limit = TITLE_SCREEN_PALETTE_CUTS
                    .iter()
                    .find_map(|&(sheet, limit)| (sheet == stem).then_some(limit));
                push_palette_entry_with_limit(
                    &path,
                    format!("title/palette/{stem}"),
                    color_limit,
                    writer,
                )?;
            }
            Some("bin") => push_raw_entry(&path, format!("title/raw/{stem}"), writer)?,
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        extract_title_screen, ExtractError, PackWriter, TITLE_SCREEN_EMBEDDED_PALETTE_SHEETS,
        TITLE_SCREEN_FILE_PALETTES, TITLE_SCREEN_IMAGES,
    };
    use crate::extract::{extract_to, upstream_present};

    fn scratch_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "pokeemerald-rs-extract-test-{name}-{}.pack",
            std::process::id()
        ))
    }

    fn assert_pack_contains_entry_id(bytes: &[u8], id: &str) {
        assert!(
            bytes
                .windows(id.len())
                .any(|window| window == id.as_bytes()),
            "missing pack entry id `{id}`"
        );
    }

    #[test]
    fn a_title_directory_missing_a_required_source_is_refused() {
        let upstream = std::env::temp_dir().join(format!(
            "pokeemerald-rs-extract-missing-title-source-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&upstream);
        let dir = upstream.join("graphics/title_screen");
        std::fs::create_dir_all(&dir).unwrap();
        // Every required source except `rayquaza.bin`; contents are never
        // read because the presence check runs first.
        for stem in TITLE_SCREEN_IMAGES {
            std::fs::write(dir.join(format!("{stem}.png")), b"").unwrap();
        }
        for stem in ["clouds", "pokemon_logo"] {
            std::fs::write(dir.join(format!("{stem}.bin")), b"").unwrap();
        }
        for stem in TITLE_SCREEN_FILE_PALETTES {
            std::fs::write(dir.join(format!("{stem}.pal")), b"").unwrap();
        }

        let err = extract_title_screen(&upstream, &mut PackWriter::new()).unwrap_err();
        let _ = std::fs::remove_dir_all(&upstream);
        assert!(
            matches!(&err, ExtractError::ReadFailed(path, _) if path == &dir.join("rayquaza.bin")),
            "{err:?}"
        );
    }

    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn embedded_palette_sheets_get_a_title_palette_entry() {
        assert!(upstream_present(), "run ./init.sh first");
        let path = scratch_path("embedded-palettes");
        let report = extract_to(&path).expect("extraction should succeed against a real checkout");
        let bytes = std::fs::read(&report.output_path).unwrap();
        for name in TITLE_SCREEN_EMBEDDED_PALETTE_SHEETS {
            let id = format!("title/palette/{name}");
            assert_pack_contains_entry_id(&bytes, &id);
        }
        let _ = std::fs::remove_file(report.output_path);
    }
}
