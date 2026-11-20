//! Extracts the interface palettes and the text printer's down-arrow sheet.

use std::path::Path;

use super::scope::{DOWN_ARROW_IMAGE_ID, DOWN_ARROW_PNG, MAIN_MENU_BG_PALETTE_ID};
use super::{push_palette_entry, push_png_entry, ExtractError};
use pack_format::PackWriter;

/// Extracts `graphics/fonts/down_arrow.png` as `interface/image/down_arrow`.
///
/// The sheet carries no palette entry: `TextPrinterDrawDownArrow` blits its
/// pixel indices into the window, where index 0 is the colour key
/// (`src/text.c:819-829`, `src/blit.c:58-66`).
pub(super) fn extract_interface_images(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    push_png_entry(
        &upstream.join(DOWN_ARROW_PNG),
        DOWN_ARROW_IMAGE_ID.to_owned(),
        writer,
    )
}

pub(super) fn extract_interface_palettes(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    push_palette_entry(
        &upstream.join("graphics/interface/main_menu_bg.pal"),
        MAIN_MENU_BG_PALETTE_ID.to_owned(),
        writer,
    )
}

#[cfg(test)]
mod tests {
    use crate::extract::owned_fixture::{assert_pack_path_spares_occupied_file, OwnedDir};
    use crate::extract::scope::{DOWN_ARROW_IMAGE_ID, DOWN_ARROW_PNG};
    use crate::extract::{extract_to, png, repo_root, upstream_present};

    /// A fresh directory the test owns, and the pack path inside it.
    fn scratch_path(name: &str) -> (OwnedDir, std::path::PathBuf) {
        let owned = OwnedDir::create(&format!("pokeemerald-rs-extract-test-{name}")).unwrap();
        let path = owned.path().join(format!("{name}.pack"));
        (owned, path)
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
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn main_menu_bg_palette_gets_an_interface_palette_entry() {
        assert!(upstream_present(), "run ./init.sh first");
        let (_owned, path) = scratch_path("interface-palette");
        let report = extract_to(&path).expect("extraction should succeed against a real checkout");
        let bytes = std::fs::read(&report.output_path).unwrap();
        let id = "interface/palette/main_menu_bg";
        assert_pack_contains_entry_id(&bytes, id);
    }

    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn the_down_arrow_sheet_is_extracted_pixel_for_pixel() {
        assert!(upstream_present(), "run ./init.sh first");
        let (_owned, path) = scratch_path("interface-down-arrow");
        let report = extract_to(&path).expect("extraction should succeed against a real checkout");
        let source = std::fs::read(repo_root().join("pokeemerald").join(DOWN_ARROW_PNG)).unwrap();
        let decoded = png::decode(&source).unwrap();

        let pack = assets::AssetPack::load(&report.output_path).unwrap();
        let sheet = pack.image(DOWN_ARROW_IMAGE_ID).unwrap();
        assert_eq!((sheet.width, sheet.height, sheet.bit_depth), (8, 48, 4));
        assert_eq!(sheet.pixels, decoded.pixels);
        pack.text_printer_down_arrow()
            .expect("the typed handle accepts the extracted sheet");
    }

    #[test]
    fn pack_path_never_reuses_an_occupied_name() {
        assert_pack_path_spares_occupied_file(
            "pokeemerald-rs-extract-test-interface-pack-ownership-probe",
            || scratch_path("interface-pack-ownership-probe"),
        );
    }
}
