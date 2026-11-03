//! Extracts the field-effect sprite sheets and the palette they draw with.
//!
//! Upstream keeps these outside `graphics/object_events`: the pictures live in
//! `graphics/field_effects/pics` and bind palettes from
//! `graphics/field_effects/palettes` through `gFieldEffectObjectTemplate_*`.

use std::path::Path;

use super::{push_palette_entry, push_png_entry, ExtractError};
use pack_format::PackWriter;

/// The tall-grass picture: five 16x16 frames
/// (`gFieldEffectObjectPic_TallGrass`, `sPicTable_TallGrass`).
const TALL_GRASS_IMAGE_ID: &str = "field-effect/image/tall_grass";

/// `gFieldEffectObjectPalette1`, which `FLDEFF_PAL_TAG_GENERAL_1` names and the
/// tall-grass template binds (`field_effect_objects.h:2,96`).
const GENERAL_1_PALETTE_ID: &str = "field-effect/palette/general_1";

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
    use super::{extract_field_effects, ExtractError};
    use pack_format::PackWriter;

    fn scratch_checkout(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "pokeemerald-rs-extract-field-effect-{name}-{}",
            std::process::id()
        ))
    }

    #[test]
    fn a_checkout_missing_the_tall_grass_sources_is_refused() {
        let upstream = scratch_checkout("missing");
        let _ = std::fs::remove_dir_all(&upstream);
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
        extract_field_effects(&upstream, &mut writer).expect("complete");
        assert_eq!(writer.len(), 2, "the sheet and its palette");

        std::fs::remove_file(palettes.join("general_1.pal")).expect("remove palette");
        let palette = extract_field_effects(&upstream, &mut PackWriter::new());
        std::fs::remove_file(pics.join("tall_grass.png")).expect("remove sheet");
        let sheet = extract_field_effects(&upstream, &mut PackWriter::new());
        std::fs::remove_dir_all(&upstream).expect("cleanup");

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
}
