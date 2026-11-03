//! Ties the extraction manifest to the consumers that restate it: the
//! `assets` crate's bundled layout and tileset tables, and the committed ROM
//! profile's roots. Each comparison needs neither a checkout nor a ROM, so a
//! manifest change fails an ordinary `cargo test --workspace` until those
//! consumers follow it.

use std::collections::BTreeSet;

use assets::bundled_overworld::{BUNDLED_LAYOUT_IDS, BUNDLED_TILESETS};
use rom_import::profiles::bpee_rev0::EMERALD_US_REV0_ROOTS;

use super::scope::{sprite_palette_ids, sprite_sheet_ids, LAYOUTS, TILESETS};

fn set<'a>(names: impl IntoIterator<Item = &'a str>) -> BTreeSet<&'a str> {
    names.into_iter().collect()
}

#[test]
fn the_bundled_layout_set_is_pinned_for_the_tables_derived_from_it() {
    let manifest: Vec<&str> = LAYOUTS.iter().map(|layout| layout.upstream_id).collect();
    let bundled: Vec<&str> = BUNDLED_LAYOUT_IDS.iter().map(|id| id.name()).collect();
    assert_eq!(
        manifest, bundled,
        "`assets::bundled_overworld::BUNDLED_LAYOUT_IDS` must follow the extraction manifest"
    );
}

#[test]
fn the_bundled_tileset_pack_names_are_pinned_to_the_manifest() {
    assert_eq!(
        set(TILESETS.iter().map(|tileset| tileset.name)),
        set(BUNDLED_TILESETS.iter().map(|tileset| tileset.pack_name)),
        "`assets::bundled_overworld::BUNDLED_TILESETS` must follow the extraction manifest"
    );
}

#[test]
fn the_committed_profile_roots_cover_exactly_the_manifest() {
    let roots = &EMERALD_US_REV0_ROOTS;
    assert_eq!(
        set(TILESETS.iter().map(|tileset| tileset.name)),
        set(roots.tilesets.iter().map(|tileset| tileset.name)),
        "tilesets"
    );
    assert_eq!(
        set(LAYOUTS.iter().map(|layout| layout.pack_name)),
        set(roots.layouts.iter().map(|layout| layout.name)),
        "layouts"
    );
    let sheets = sprite_sheet_ids();
    assert_eq!(
        set(sheets.iter().map(String::as_str)),
        set(roots.sprites.sheets.iter().map(|sheet| sheet.id)),
        "sprite sheets"
    );
    let palettes = sprite_palette_ids();
    assert_eq!(
        set(palettes.iter().map(String::as_str)),
        set(roots.sprites.palettes.iter().map(|palette| palette.id)),
        "sprite palettes"
    );
}
