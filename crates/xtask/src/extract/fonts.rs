//! Latin font glyph-sheet extraction (S-4, issue #114).
//!
//! The five upstream Latin glyph sheets
//! (`graphics/fonts/latin_{normal,narrow,short,small,small_narrow}.png` —
//! see [`FONTS`]) are each a 256x512, 16-column x 32-row grid of 512
//! 16x16-pixel glyph cells, decoded via [`png::decode`]'s bit-depth-2
//! support (these sheets are 2bpp — 4 colours, `gbagfx`'s
//! `SetFontPalette` — unlike tilesets'/sprites' 4/8bpp). Per-glyph advance
//! widths (upstream `gFont*LatinGlyphWidths`) are *not* in the pack —
//! they're a small, stable table of constants, ported directly as Rust
//! data in `crates/assets::fonts` (see that module's docs).
//! Japanese/braille/keypad/arrow glyph sheets (the other 12 files under
//! `graphics/fonts/`) are **not** extracted — v1 is English-only text, so
//! they stay `pending` in the ledger.

use std::path::Path;

use super::pack::PackWriter;
use super::{decode_png_entry, ExtractError};

/// `(upstream `FONT_*` id, lowercased; `graphics/fonts/` filename)` — the
/// five Latin glyph sheets this pipeline extracts. See the module docs for
/// why only these five, not the other 12 files under `graphics/fonts/`
/// (Japanese, braille, arrows, the keypad icon sheet).
const FONTS: [(&str, &str); 5] = [
    ("small", "latin_small.png"),
    ("normal", "latin_normal.png"),
    ("short", "latin_short.png"),
    ("narrow", "latin_narrow.png"),
    ("small_narrow", "latin_small_narrow.png"),
];

/// Extract the five Latin font glyph sheets (see [`FONTS`] and the module
/// docs). Per-glyph advance widths are not extracted here — they're ported
/// as Rust data directly in `crates/assets::fonts`.
pub(super) fn extract_fonts(upstream: &Path, writer: &mut PackWriter) -> Result<(), ExtractError> {
    let dir = upstream.join("graphics/fonts");
    for (name, filename) in FONTS {
        decode_png_entry(&dir.join(filename), format!("font/{name}/glyphs"), writer)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::{extract_to, upstream_present};
    use super::FONTS;

    fn scratch_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "pokeemerald-rs-extract-test-{name}-{}.pack",
            std::process::id()
        ))
    }

    #[test]
    fn fonts_list_has_no_duplicate_names_or_filenames() {
        // Pure data check -- no filesystem access -- so it runs everywhere.
        let names: Vec<_> = FONTS.iter().map(|(name, _)| *name).collect();
        let filenames: Vec<_> = FONTS.iter().map(|(_, filename)| *filename).collect();
        let unique_names: std::collections::HashSet<_> = names.iter().collect();
        let unique_filenames: std::collections::HashSet<_> = filenames.iter().collect();
        assert_eq!(names.len(), unique_names.len(), "duplicate font name");
        assert_eq!(
            filenames.len(),
            unique_filenames.len(),
            "duplicate filename"
        );
        for name in &names {
            // Pack ids are ASCII lowercase + digits + underscores + `/` only
            // (see `crate::extract`'s "Asset id scheme" docs).
            assert!(name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'));
        }
        for filename in &filenames {
            assert!(filename.starts_with("latin_"));
            assert!(std::path::Path::new(filename)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("png")));
        }
    }

    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn font_glyph_sheets_are_extracted() {
        // Same crude substring-search strategy as
        // `extract::tests::layout_grids_are_extracted` (no pack reader
        // lives in this crate -- see its comment).
        assert!(upstream_present(), "run ./init.sh first");
        let path = scratch_path("fonts");
        let report = extract_to(&path).expect("extraction should succeed against a real checkout");
        let bytes = std::fs::read(&report.output_path).unwrap();
        for (name, _) in FONTS {
            let id = format!("font/{name}/glyphs");
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
