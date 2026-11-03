//! Extracts the interface palettes.

use std::path::Path;

use super::scope::MAIN_MENU_BG_PALETTE_ID;
use super::{push_palette_entry, ExtractError};
use pack_format::PackWriter;

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
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn main_menu_bg_palette_gets_an_interface_palette_entry() {
        assert!(upstream_present(), "run ./init.sh first");
        let path = scratch_path("interface-palette");
        let report = extract_to(&path).expect("extraction should succeed against a real checkout");
        let bytes = std::fs::read(&report.output_path).unwrap();
        let id = "interface/palette/main_menu_bg";
        assert_pack_contains_entry_id(&bytes, id);
        let _ = std::fs::remove_file(report.output_path);
    }
}
