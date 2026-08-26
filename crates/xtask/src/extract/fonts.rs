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
//! The other 12 files under `graphics/fonts/` are **not** extracted, for
//! two different reasons. The Japanese sheets are **excluded**: an English
//! retail cartridge never renders them, so no lone player reaches them
//! (`docs/acceptance/v1.md`'s exclusion rule), and so are the two
//! `unused_frlg_*down_arrow` sheets — dead upstream assets with no live
//! caller (`sUnusedFRLGBlankedDownArrow`/`sUnusedFRLGDownArrow`,
//! `pokeemerald/src/text.c:73-74`, are defined and never read), excluded on
//! the same ground as the Japanese sheets. The braille sheet is
//! single-player content — `ScrCmd_braillemessage`
//! (`pokeemerald/src/scrcmd.c:1482`) draws the Regi puzzle's braille signs
//! (Sealed Chamber, Desert Ruins, Island Cave, Ancient Tomb) — and so are
//! the keypad-icon and the two live down-arrow sheets English text itself
//! uses; those are not extracted yet: deferred, still in v1 scope (braille
//! under `C-3`). Either way they stay `pending` in the ledger.

use std::path::Path;

use super::{build_image_entry, png, read_file, ExtractError};
use pack_format::PackWriter;

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

/// Every Latin glyph sheet is 256 pixels wide (16 columns of 16x16 cells)
/// — must match `crates/assets`' read-side `fonts::SHEET_WIDTH`.
const FONT_SHEET_WIDTH: u32 = 256;
/// Every Latin glyph sheet is 512 pixels tall (32 rows of 16x16 cells) —
/// must match `crates/assets`' read-side `fonts::SHEET_HEIGHT`.
const FONT_SHEET_HEIGHT: u32 = 512;
/// Every Latin glyph sheet is 2bpp (`gbagfx`'s `SetFontPalette` shape —
/// see the module docs). 2bpp decoding also bounds every pixel index to
/// `0..=3` by construction, so no separate pixel-range check is needed.
const FONT_SHEET_BIT_DEPTH: u8 = 2;

/// Extract the five Latin font glyph sheets (see [`FONTS`] and the module
/// docs). Per-glyph advance widths are not extracted here — they're ported
/// as Rust data directly in `crates/assets::fonts`.
///
/// Each sheet is validated against the documented 256x512/2bpp shape
/// before it is serialized: the upstream checkout is a moving, unpinned
/// reference, and a reshaped sheet written silently here would only
/// surface later when `crates/assets`' `FontGlyphSheet::new` rejects the
/// generated pack — fail at extraction instead.
pub(super) fn extract_fonts(upstream: &Path, writer: &mut PackWriter) -> Result<(), ExtractError> {
    let dir = upstream.join("graphics/fonts");
    for (name, filename) in FONTS {
        let path = dir.join(filename);
        let bytes = read_file(&path)?;
        let image = png::decode(&bytes).map_err(|e| ExtractError::Png(path.clone(), e))?;
        validate_font_sheet(&path, &image)?;
        writer.push(build_image_entry(
            &path,
            format!("font/{name}/glyphs"),
            image,
        )?);
    }
    Ok(())
}

/// Reject a glyph sheet that is not the exact 256x512/2bpp shape the
/// documented font contract (and `crates/assets`' read side) requires.
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
        validate_font_sheet, ExtractError, FONTS, FONT_SHEET_BIT_DEPTH, FONT_SHEET_HEIGHT,
        FONT_SHEET_WIDTH,
    };

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

        // A reshaped sheet from the unpinned upstream checkout must fail at
        // extraction, not later at pack-read time.
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
