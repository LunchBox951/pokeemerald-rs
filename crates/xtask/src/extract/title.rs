//! Extracts the title screen graphics.

use std::path::Path;

use super::scope::{
    TITLE_SCREEN_EMBEDDED_PALETTE_SHEETS, TITLE_SCREEN_FILE_PALETTES, TITLE_SCREEN_IMAGES,
    TITLE_SCREEN_PALETTE_CUTS, TITLE_SCREEN_TILEMAPS,
};
use super::{
    build_image_entry, build_palette_entry, jasc_pal, png, push_palette_entry_with_limit,
    push_raw_entry, read_file, ExtractError,
};
use pack_format::PackWriter;

/// Every source the fixed inventory names, in manifest order: images and
/// embedded-palette sheets as `.png`, tilemaps as `.bin`, palettes as `.pal`.
fn title_source_names() -> impl Iterator<Item = String> {
    TITLE_SCREEN_IMAGES
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
        )
}

/// Refuse a title directory missing a source the generator requires.
fn require_title_sources(dir: &Path) -> Result<(), ExtractError> {
    for name in title_source_names() {
        let path = dir.join(&name);
        if !path.is_file() {
            return Err(ExtractError::ReadFailed(
                path,
                "required title screen source is missing".to_owned(),
            ));
        }
    }
    Ok(())
}

/// Extract the fixed title inventory from `graphics/title_screen`.
///
/// The manifest in `scope.rs` is authoritative: only the files it names are
/// read, so an untracked file in the directory never gains a `title/*` id
/// the ROM profile does not describe.
pub(super) fn extract_title_screen(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let dir = upstream.join("graphics/title_screen");
    require_title_sources(&dir)?;

    for stem in TITLE_SCREEN_IMAGES {
        let path = dir.join(format!("{stem}.png"));
        let bytes = read_file(&path)?;
        let image = png::decode(&bytes).map_err(|e| ExtractError::Png(path.clone(), e))?;
        if TITLE_SCREEN_EMBEDDED_PALETTE_SHEETS.contains(&stem) {
            if image.palette.is_empty() {
                return Err(ExtractError::MissingEmbeddedPalette(path.clone()));
            }
            let colors: Vec<jasc_pal::Rgb888> = image
                .palette
                .iter()
                .map(|&[r, g, b]| jasc_pal::Rgb888 { r, g, b })
                .collect();
            let entry = build_palette_entry(&path, &colors, format!("title/palette/{stem}"))?;
            writer.push(entry);
        }
        writer.push(build_image_entry(
            &path,
            format!("title/image/{stem}"),
            image,
        )?);
    }
    for stem in TITLE_SCREEN_FILE_PALETTES {
        let path = dir.join(format!("{stem}.pal"));
        let color_limit = TITLE_SCREEN_PALETTE_CUTS
            .iter()
            .find_map(|&(sheet, limit)| (sheet == stem).then_some(limit));
        push_palette_entry_with_limit(&path, format!("title/palette/{stem}"), color_limit, writer)?;
    }
    for stem in TITLE_SCREEN_TILEMAPS {
        push_raw_entry(
            &dir.join(format!("{stem}.bin")),
            format!("title/raw/{stem}"),
            writer,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        extract_title_screen, ExtractError, PackWriter, TITLE_SCREEN_EMBEDDED_PALETTE_SHEETS,
        TITLE_SCREEN_FILE_PALETTES, TITLE_SCREEN_IMAGES, TITLE_SCREEN_TILEMAPS,
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

    /// A synthetic 8x8 4-bit indexed PNG with a 16-entry `PLTE` chunk,
    /// valid for both image and embedded-palette sheets.
    const TINY_PNG: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x08, 0x04, 0x03, 0x00, 0x00, 0x00, 0x36,
        0x21, 0xa3, 0xb8, 0x00, 0x00, 0x00, 0x30, 0x50, 0x4c, 0x54, 0x45, 0x00, 0xff, 0x00, 0x10,
        0xef, 0x25, 0x20, 0xdf, 0x4a, 0x30, 0xcf, 0x6f, 0x40, 0xbf, 0x94, 0x50, 0xaf, 0xb9, 0x60,
        0x9f, 0xde, 0x70, 0x8f, 0x03, 0x80, 0x7f, 0x28, 0x90, 0x6f, 0x4d, 0xa0, 0x5f, 0x72, 0xb0,
        0x4f, 0x97, 0xc0, 0x3f, 0xbc, 0xd0, 0x2f, 0xe1, 0xe0, 0x1f, 0x06, 0xf0, 0x0f, 0x2b, 0x06,
        0xc2, 0xf3, 0x38, 0x00, 0x00, 0x00, 0x31, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x60,
        0x54, 0x76, 0x4d, 0x67, 0x10, 0x31, 0x8b, 0xa8, 0x62, 0x50, 0xf7, 0xcc, 0xee, 0x65, 0xb0,
        0x8a, 0xa9, 0x9b, 0xc0, 0xe0, 0x9b, 0xdf, 0xb8, 0x98, 0x21, 0xa0, 0x68, 0xca, 0x36, 0x86,
        0xe4, 0xd6, 0xe5, 0x27, 0x19, 0xca, 0x66, 0xec, 0xba, 0x03, 0x00, 0xdb, 0xc0, 0x0d, 0xe1,
        0xef, 0xfd, 0xcf, 0xcc, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60,
        0x82,
    ];

    /// A 256-colour JASC palette, long enough for the `pokemon_logo` cut.
    fn jasc_text() -> String {
        use std::fmt::Write as _;
        let mut text = String::from("JASC-PAL\r\n0100\r\n256\r\n");
        for n in 0..256 {
            write!(text, "{n} 0 0\r\n").unwrap();
        }
        text
    }

    /// Write a synthetic `graphics/title_screen` holding exactly the fixed
    /// inventory, and return the upstream root and the title directory.
    fn synthetic_title_checkout(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let upstream = std::env::temp_dir().join(format!(
            "pokeemerald-rs-extract-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&upstream);
        let dir = upstream.join("graphics/title_screen");
        std::fs::create_dir_all(&dir).unwrap();
        for stem in TITLE_SCREEN_IMAGES {
            std::fs::write(dir.join(format!("{stem}.png")), TINY_PNG).unwrap();
        }
        for stem in TITLE_SCREEN_TILEMAPS {
            std::fs::write(dir.join(format!("{stem}.bin")), b"TILEMAP").unwrap();
        }
        for stem in TITLE_SCREEN_FILE_PALETTES {
            std::fs::write(dir.join(format!("{stem}.pal")), jasc_text()).unwrap();
        }
        (upstream, dir)
    }

    fn pack_ids_with_prefix(bytes: &[u8], prefix: &str) -> usize {
        bytes
            .windows(prefix.len())
            .filter(|w| *w == prefix.as_bytes())
            .count()
    }

    #[test]
    fn the_fixed_inventory_extracts_from_a_synthetic_checkout() {
        let (upstream, _) = synthetic_title_checkout("title-baseline");
        let mut writer = PackWriter::new();
        let result = extract_title_screen(&upstream, &mut writer);
        let _ = std::fs::remove_dir_all(&upstream);
        result.unwrap();
        let bytes = writer.finish().unwrap();
        for id in crate::extract::scope::title_ids() {
            assert_pack_contains_entry_id(&bytes, &id);
        }
        assert_eq!(pack_ids_with_prefix(&bytes, "title/"), 14);
    }

    #[test]
    fn untracked_title_files_are_not_packed() {
        let (upstream, dir) = synthetic_title_checkout("untracked-title");
        // Neither readable as its extension nor in the inventory: a
        // manifest-driven extractor never opens them.
        std::fs::write(dir.join("zz_untracked.png"), b"not a png").unwrap();
        std::fs::write(dir.join("zz_untracked.pal"), b"not a palette").unwrap();
        std::fs::write(dir.join("zz_untracked.bin"), b"UNTRACKED").unwrap();
        // An inventory stem under another extension is not an inventory file.
        std::fs::write(dir.join("logo_shine.bin"), b"UNTRACKED").unwrap();
        std::fs::write(dir.join("unused.png"), b"not a png").unwrap();
        let mut writer = PackWriter::new();
        let result = extract_title_screen(&upstream, &mut writer);
        let _ = std::fs::remove_dir_all(&upstream);
        result.unwrap();
        let bytes = writer.finish().unwrap();
        assert_eq!(pack_ids_with_prefix(&bytes, "title/"), 14);
        assert!(!bytes.windows(9).any(|w| w == b"UNTRACKED"));
        for id in [
            "title/raw/zz_untracked",
            "title/image/zz_untracked",
            "title/palette/zz_untracked",
            "title/raw/logo_shine",
            "title/image/unused",
        ] {
            assert!(
                !bytes.windows(id.len()).any(|w| w == id.as_bytes()),
                "untracked title file was packed as `{id}`"
            );
        }
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
