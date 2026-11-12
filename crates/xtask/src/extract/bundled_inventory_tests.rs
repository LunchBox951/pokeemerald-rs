//! Ties the extraction manifest to the consumers that restate it: the
//! `assets` crate's bundled layout and tileset tables, and the committed ROM
//! profile's roots. Each comparison needs neither a checkout nor a ROM, so a
//! manifest change fails an ordinary `cargo test --workspace` until those
//! consumers follow it.

use std::collections::BTreeSet;

use assets::bundled_overworld::{BUNDLED_LAYOUT_IDS, BUNDLED_TILESETS};
use rom_import::profiles::bpee_rev0::EMERALD_US_REV0_ROOTS;

use super::scope::{
    sprite_palette_ids, sprite_sheet_ids, trainer_battler_ids, INTERFACE_IMAGE_IDS,
    INTERFACE_PALETTE_IDS, LAYOUTS, TILESETS,
};

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

#[test]
fn the_committed_profile_interface_roots_cover_exactly_the_manifest() {
    let roots = &EMERALD_US_REV0_ROOTS.interface;
    assert_eq!(
        set(INTERFACE_IMAGE_IDS),
        set(roots.images.iter().map(|image| image.id)),
        "interface images"
    );
    assert_eq!(
        set(INTERFACE_PALETTE_IDS),
        set(roots.palettes.iter().map(|palette| palette.id)),
        "interface palettes"
    );
}

#[test]
fn the_text_printer_down_arrow_is_pinned_in_the_manifest_and_the_profile() {
    // A literal, so dropping the arrow from the manifest and the profile
    // together still fails here.
    assert!(INTERFACE_IMAGE_IDS.contains(&"interface/image/down_arrow"));
    assert!(EMERALD_US_REV0_ROOTS
        .images()
        .any(|image| image.id == "interface/image/down_arrow"));
}

#[test]
fn the_trainer_front_art_is_published_once_under_the_battler_ids() {
    // Literals, so the manifest and the profile cannot both drop an entry or
    // regrow a second family unnoticed.
    const IDS: [&str; 6] = [
        "trainer/brendan/front",
        "trainer/brendan/back",
        "trainer/brendan/palette",
        "trainer/may/front",
        "trainer/may/back",
        "trainer/may/palette",
    ];
    let manifest = trainer_battler_ids();
    assert_eq!(manifest, IDS);

    let roots = &EMERALD_US_REV0_ROOTS;
    let mut images: Vec<&str> = roots
        .images()
        .map(|image| image.id)
        .filter(|id| id.starts_with("trainer"))
        .collect();
    let mut palettes: Vec<&str> = roots
        .palettes()
        .map(|palette| palette.id)
        .filter(|id| id.starts_with("trainer"))
        .collect();
    images.sort_unstable();
    palettes.sort_unstable();
    assert_eq!(
        images,
        [
            "trainer/brendan/back",
            "trainer/brendan/front",
            "trainer/may/back",
            "trainer/may/front"
        ]
    );
    assert_eq!(palettes, ["trainer/brendan/palette", "trainer/may/palette"]);
    let pics: Vec<u16> = roots
        .trainer_battlers
        .front
        .trainers
        .iter()
        .map(|trainer| trainer.trainer_pic)
        .collect();
    assert_eq!(pics, [71, 72]);
    let backs: Vec<u16> = roots
        .trainer_battlers
        .backs
        .iter()
        .map(|back| back.back_pic)
        .collect();
    assert_eq!(backs, [0, 1]);
}
