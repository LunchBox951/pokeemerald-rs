//! Extracts the field-effect sprite sheets and the palette they draw with.
//!
//! Upstream keeps these outside `graphics/object_events`: the pictures live in
//! `graphics/field_effects/pics` and bind palettes from
//! `graphics/field_effects/palettes` through `gFieldEffectObjectTemplate_*`.

use std::path::Path;

use super::scope::{GENERAL_1_PALETTE_ID, TALL_GRASS_IMAGE_ID};
use super::{push_palette_entry, push_png_entry, ExtractError};
use pack_format::PackWriter;

pub(super) fn extract_field_effects(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let root = upstream.join("graphics/field_effects");
    push_png_entry(
        &root.join("pics/tall_grass.png"),
        TALL_GRASS_IMAGE_ID.to_owned(),
        writer,
    )?;
    push_palette_entry(
        &root.join("palettes/general_1.pal"),
        GENERAL_1_PALETTE_ID.to_owned(),
        writer,
    )
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{extract_field_effects, ExtractError};
    use crate::extract::owned_fixture::OwnedDir;
    use pack_format::PackWriter;

    fn scratch_checkout(base: &Path) -> OwnedDir {
        OwnedDir::create_in(base, "pokeemerald-rs-extract-field-effect").expect("claim fixture")
    }

    fn assert_missing_tall_grass_sources_are_refused(upstream: &Path) {
        let pics = upstream.join("graphics/field_effects/pics");
        let palettes = upstream.join("graphics/field_effects/palettes");
        std::fs::create_dir_all(&pics).expect("dir");
        std::fs::create_dir_all(&palettes).expect("dir");
        let png = crate::extract::png::tests::tiny_indexed_png(8, 1, 1, &[0]);
        std::fs::write(pics.join("tall_grass.png"), &png).expect("sheet");
        std::fs::write(
            palettes.join("general_1.pal"),
            "JASC-PAL\r\n0100\r\n1\r\n0 0 0\r\n",
        )
        .expect("palette");

        let mut writer = PackWriter::new();
        extract_field_effects(upstream, &mut writer).expect("complete");
        assert_eq!(writer.len(), 2, "the sheet and its palette");

        std::fs::remove_file(palettes.join("general_1.pal")).expect("remove palette");
        let palette = extract_field_effects(upstream, &mut PackWriter::new());
        std::fs::remove_file(pics.join("tall_grass.png")).expect("remove sheet");
        let sheet = extract_field_effects(upstream, &mut PackWriter::new());

        let palette = palette.expect_err("palette");
        assert!(
            matches!(&palette, ExtractError::ReadFailed(p, _) if p.ends_with("palettes/general_1.pal")),
            "{palette:?}"
        );
        let sheet = sheet.expect_err("sheet");
        assert!(
            matches!(&sheet, ExtractError::ReadFailed(p, _) if p.ends_with("pics/tall_grass.png")),
            "{sheet:?}"
        );
    }

    #[test]
    fn a_checkout_missing_the_tall_grass_sources_is_refused() {
        let upstream = scratch_checkout(&std::env::temp_dir());
        assert_missing_tall_grass_sources_are_refused(upstream.path());
    }

    #[test]
    fn a_populated_preexisting_directory_at_the_fixture_name_survives() {
        let base = OwnedDir::create("pokeemerald-rs-extract-field-effect-base").expect("base");
        let occupied = scratch_checkout(base.path());
        std::fs::write(occupied.path().join("sentinel"), "precious").expect("sentinel");

        let upstream = scratch_checkout(base.path());
        assert_missing_tall_grass_sources_are_refused(upstream.path());

        assert_ne!(upstream.path(), occupied.path());
        assert_eq!(
            std::fs::read(occupied.path().join("sentinel"))
                .ok()
                .as_deref(),
            Some(&b"precious"[..]),
            "the fixture reused a directory it never created"
        );
    }
}
