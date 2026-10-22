//! Extracts the bundled map layouts: each grid and border named by `LAYOUTS`.

use std::path::Path;

use super::scope::LAYOUTS;
use super::{layouts_json, read_file, read_text, ExtractError};
use pack_format::PackWriter;

const BORDER_SIDE_CELLS: usize = 2;
/// Bytes in one border block, which `crates/assets` fixes independently; this
/// pipeline never depends on that crate.
const BORDER_BLOCK_BYTES: usize =
    BORDER_SIDE_CELLS * BORDER_SIDE_CELLS * std::mem::size_of::<u16>();

pub(super) fn extract_layouts(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let json_path = upstream.join("data/layouts/layouts.json");
    let text = read_text(&json_path)?;
    let entries =
        layouts_json::parse(&text).map_err(|e| ExtractError::LayoutsJson(json_path.clone(), e))?;

    for layout in LAYOUTS {
        let entry = entries
            .iter()
            .find(|entry| entry.id == layout.upstream_id)
            .ok_or(ExtractError::UnknownLayoutInJson(
                json_path.clone(),
                layout.upstream_id,
            ))?;

        let map_bytes = read_file(&upstream.join(&entry.blockdata_filepath))?;
        let width = usize::try_from(entry.width).unwrap_or(usize::MAX);
        let height = usize::try_from(entry.height).unwrap_or(usize::MAX);
        let expected_min = width
            .saturating_mul(height)
            .saturating_mul(std::mem::size_of::<u16>());
        if map_bytes.len() < expected_min {
            return Err(ExtractError::LayoutGridTooShort {
                layout_id: layout.upstream_id,
                expected: expected_min,
                actual: map_bytes.len(),
            });
        }
        writer.push(pack_format::raw_entry(
            format!("layout/{}/map", layout.pack_name),
            map_bytes,
        ));

        let border_bytes = read_file(&upstream.join(&entry.border_filepath))?;
        if border_bytes.len() != BORDER_BLOCK_BYTES {
            return Err(ExtractError::LayoutBorderWrongSize {
                layout_id: layout.upstream_id,
                actual: border_bytes.len(),
            });
        }
        writer.push(pack_format::raw_entry(
            format!("layout/{}/border", layout.pack_name),
            border_bytes,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::LAYOUTS;
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

    // Changes to `LAYOUTS` must also update `object_event_flags.rs`'s
    // `BUNDLED_LAYOUTS`, `overworld/npc.rs`'s `EXTRACTED_MAPS`,
    // `overworld/mod.rs`'s `resolve_tileset_pack_name`, and
    // `overworld_phase/decoration_tests.rs`'s `BUNDLED_LAYOUTS`.
    #[test]
    fn the_bundled_layout_set_is_pinned_for_the_tables_derived_from_it() {
        let ids: Vec<&str> = LAYOUTS.iter().map(|layout| layout.upstream_id).collect();
        assert_eq!(
            ids,
            [
                "LAYOUT_LITTLEROOT_TOWN",
                "LAYOUT_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F",
                "LAYOUT_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F",
                "LAYOUT_LITTLEROOT_TOWN_MAYS_HOUSE_1F",
                "LAYOUT_LITTLEROOT_TOWN_MAYS_HOUSE_2F",
                "LAYOUT_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB",
                "LAYOUT_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB_WITH_TABLE",
                "LAYOUT_ROUTE101",
                "LAYOUT_OLDALE_TOWN",
                "LAYOUT_ROUTE103",
                "LAYOUT_INSIDE_OF_TRUCK",
                "LAYOUT_HOUSE1",
                "LAYOUT_HOUSE2",
                "LAYOUT_MART",
                "LAYOUT_POKEMON_CENTER_1F",
                "LAYOUT_POKEMON_CENTER_2F",
            ],
            "update the cross-crate LAYOUTS mirrors with this manifest"
        );
    }

    #[test]
    fn layouts_list_has_no_duplicate_ids_or_names() {
        let ids: Vec<_> = LAYOUTS.iter().map(|layout| layout.upstream_id).collect();
        let names: Vec<_> = LAYOUTS.iter().map(|layout| layout.pack_name).collect();
        let unique_ids: std::collections::HashSet<_> = ids.iter().collect();
        let unique_names: std::collections::HashSet<_> = names.iter().collect();
        assert_eq!(ids.len(), unique_ids.len(), "duplicate LAYOUT_* id");
        assert_eq!(names.len(), unique_names.len(), "duplicate pack name");
        for id in &ids {
            assert!(
                id.starts_with("LAYOUT_LITTLEROOT_TOWN")
                    || *id == "LAYOUT_ROUTE101"
                    || *id == "LAYOUT_OLDALE_TOWN"
                    || *id == "LAYOUT_ROUTE103"
                    || *id == "LAYOUT_INSIDE_OF_TRUCK"
                    || id.starts_with("LAYOUT_HOUSE")
                    || *id == "LAYOUT_MART"
                    || id.starts_with("LAYOUT_POKEMON_CENTER_")
            );
        }
        for name in &names {
            assert!(
                name.starts_with("littleroot_town")
                    || *name == "route101"
                    || *name == "oldale_town"
                    || *name == "route103"
                    || *name == "inside_of_truck"
                    || name.starts_with("house")
                    || *name == "mart"
                    || name.starts_with("pokemon_center_")
            );
            assert!(name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'));
        }
    }

    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn layout_grids_are_extracted() {
        assert!(upstream_present(), "run ./init.sh first");
        let path = scratch_path("layouts");
        let report = extract_to(&path).expect("extraction should succeed against a real checkout");
        let bytes = std::fs::read(&report.output_path).unwrap();
        for layout in LAYOUTS {
            for suffix in ["map", "border"] {
                let id = format!("layout/{}/{suffix}", layout.pack_name);
                assert_pack_contains_entry_id(&bytes, &id);
            }
        }
        let _ = std::fs::remove_file(report.output_path);
    }
}
