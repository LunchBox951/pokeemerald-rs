//! Deleting any fixed root from a complete synthetic pack is refused before
//! a report line is written.

use pack_format::{raw_entry, PackEntry};
use rom_import::fixture::RomFixture;

use super::error::GenRomProfileError;
use super::tests::with_context;
use super::{fonts, inventory, layouts, locate_profile, text_window, tilesets, title};
use crate::extract::scope::{
    layout_ids, text_window_ids, tileset_animation_ids, title_ids, FONTS, LAYOUTS,
    TEXT_WINDOW_IMAGE_STEMS, TEXT_WINDOW_PALETTE_STEMS, TILESETS,
};

/// A placeholder for every fixed root; presence is all the preflight reads.
fn complete_inventory() -> Vec<PackEntry> {
    TILESETS
        .iter()
        .map(|tileset| tileset.tiles_id())
        .chain(tileset_animation_ids())
        .chain(title_ids())
        .chain(FONTS.iter().map(|font| font.pack_id.to_owned()))
        .chain(layout_ids())
        .chain(text_window_ids())
        .map(|id| raw_entry(id, vec![0]))
        .collect()
}

/// Refuse `entries` through the aggregate and the domain locator, expecting `missing`.
fn assert_refused(label: &str, entries: Vec<PackEntry>, missing: &str, domain: &str) {
    let rom = RomFixture::new().emerald_header().finish();
    with_context(label, &rom, entries, |ctx| {
        let mut report = Vec::new();
        let Err(err) = locate_profile(ctx, &mut report) else {
            panic!("{missing} is missing; the aggregate must refuse");
        };
        assert!(
            matches!(&err, GenRomProfileError::MissingPackEntry(id) if id == missing),
            "{err:?}"
        );
        assert!(report.is_empty(), "{report:?}");

        let err = if domain == "tilesets" {
            tilesets::locate(ctx, &mut report).map(|_| ())
        } else if domain == "fonts" {
            fonts::locate(ctx, &mut report).map(|_| ())
        } else if domain == "layouts" {
            layouts::locate(ctx, &mut report).map(|_| ())
        } else if domain == "text_window" {
            text_window::locate(ctx, &mut report).map(|_| ())
        } else {
            title::locate(ctx, &mut report).map(|_| ())
        }
        .expect_err("the domain locator must refuse too");
        assert!(
            matches!(&err, GenRomProfileError::MissingPackEntry(id) if id == missing),
            "{err:?}"
        );
        assert!(report.is_empty(), "{report:?}");
    });
}

fn without(id: &str) -> Vec<PackEntry> {
    let mut entries = complete_inventory();
    entries.retain(|entry| entry.id != id);
    entries
}

#[test]
fn the_fixed_inventory_holds_the_committed_profile_counts() {
    assert_eq!(TILESETS.len(), 5);
    assert_eq!(FONTS.len(), 5);
    let ids = title_ids();
    let count = |prefix: &str| ids.iter().filter(|id| id.starts_with(prefix)).count();
    assert_eq!(count("title/image/"), 6);
    assert_eq!(count("title/raw/"), 3);
    assert_eq!(count("title/palette/"), 5);
    assert_eq!(LAYOUTS.len(), 10);
    assert_eq!(layout_ids().len(), 20);
    let windows = text_window_ids();
    let count = |prefix: &str| windows.iter().filter(|id| id.starts_with(prefix)).count();
    assert_eq!(count("text-window/image/"), TEXT_WINDOW_IMAGE_STEMS.len());
    assert_eq!(count("text-window/image/"), 21);
    assert_eq!(
        count("text-window/palette/"),
        TEXT_WINDOW_IMAGE_STEMS.len() + TEXT_WINDOW_PALETTE_STEMS.len()
    );
    assert_eq!(count("text-window/palette/"), 25);
}

#[test]
fn the_fixed_inventory_ids_are_unique() {
    for ids in [layout_ids(), text_window_ids(), title_ids()] {
        let unique: std::collections::BTreeSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len(), "{ids:?}");
    }
}

#[test]
fn a_pack_missing_a_tileset_is_refused() {
    for tileset in TILESETS {
        let id = tileset.tiles_id();
        assert_refused(
            &format!("no-tileset-{}", tileset.name),
            without(&id),
            &id,
            "tilesets",
        );
    }
}

#[test]
fn a_pack_missing_a_title_image_is_refused() {
    for id in title_ids()
        .iter()
        .filter(|id| id.starts_with("title/image/"))
    {
        let label = format!("no-{}", id.replace('/', "-"));
        assert_refused(&label, without(id), id, "title");
    }
}

#[test]
fn a_pack_missing_a_title_tilemap_is_refused() {
    for id in title_ids().iter().filter(|id| id.starts_with("title/raw/")) {
        let label = format!("no-{}", id.replace('/', "-"));
        assert_refused(&label, without(id), id, "title");
    }
}

#[test]
fn a_pack_missing_a_title_palette_is_refused() {
    for id in title_ids()
        .iter()
        .filter(|id| id.starts_with("title/palette/"))
    {
        let label = format!("no-{}", id.replace('/', "-"));
        assert_refused(&label, without(id), id, "title");
    }
}

#[test]
fn a_pack_missing_a_font_sheet_is_refused() {
    for font in FONTS {
        let label = format!("no-{}", font.pack_id.replace('/', "-"));
        assert_refused(&label, without(font.pack_id), font.pack_id, "fonts");
    }
}

#[test]
fn a_pack_missing_a_layout_map_or_border_is_refused() {
    for id in layout_ids() {
        let label = format!("no-{}", id.replace('/', "-"));
        assert_refused(&label, without(&id), &id, "layouts");
    }
}

#[test]
fn a_pack_missing_a_whole_layout_is_refused_by_id() {
    let mut entries = complete_inventory();
    entries.retain(|entry| !entry.id.starts_with("layout/route103/"));
    assert_refused("no-route103", entries, "layout/route103/map", "layouts");
}

#[test]
fn a_pack_missing_a_text_window_root_is_refused() {
    for id in text_window_ids() {
        let label = format!("no-{}", id.replace('/', "-"));
        assert_refused(&label, without(&id), &id, "text_window");
    }
}

#[test]
fn a_pack_missing_a_whole_window_frame_is_refused_by_id() {
    let mut entries = complete_inventory();
    entries.retain(|entry| !entry.id.ends_with("/20") && !entry.id.contains("/20/"));
    assert_refused(
        "no-frame-20",
        entries,
        "text-window/image/20",
        "text_window",
    );
}

#[test]
fn a_complete_fixed_inventory_passes_preflight() {
    let rom = RomFixture::new().emerald_header().finish();
    with_context("complete", &rom, complete_inventory(), |ctx| {
        inventory::preflight(ctx.pack).expect("a complete inventory passes");
    });
}

#[test]
fn the_animation_inventory_matches_the_committed_profile() {
    let mut expected: Vec<String> = rom_import::EMERALD_US_REV0
        .roots
        .tilesets
        .iter()
        .flat_map(|t| {
            t.anims.iter().flat_map(move |a| {
                (0..a.frames.len()).map(move |n| format!("tileset/{}/anim/{}/{n}", t.name, a.name))
            })
        })
        .collect();
    expected.sort();
    expected.dedup();
    let mut actual = tileset_animation_ids();
    actual.sort();
    assert_eq!(actual, expected);
    assert_eq!(actual.len(), 28);
}

#[test]
fn a_pack_missing_the_last_water_frame_is_refused() {
    let id = "tileset/general/anim/water/7";
    assert_refused("no-water-7", without(id), id, "tilesets");
}

#[test]
fn a_pack_missing_any_one_animation_frame_is_refused() {
    for id in tileset_animation_ids() {
        let label = format!("no-{}", id.replace('/', "-"));
        assert_refused(&label, without(&id), &id, "tilesets");
    }
}

#[test]
fn a_pack_missing_a_whole_animation_is_refused() {
    for (set, anim) in [
        ("general", "flower"),
        ("general", "land_water_edge"),
        ("general", "sand_water_edge"),
        ("general", "water"),
        ("general", "waterfall"),
        ("building", "tv_turned_on"),
    ] {
        let prefix = format!("tileset/{set}/anim/{anim}/");
        let mut entries = complete_inventory();
        entries.retain(|e| !e.id.starts_with(&prefix));
        assert_refused(
            &format!("no-{set}-{anim}"),
            entries,
            &format!("{prefix}0"),
            "tilesets",
        );
    }
}

#[test]
fn a_pack_missing_the_whole_animation_domain_is_refused() {
    let mut entries = complete_inventory();
    entries.retain(|e| !e.id.contains("/anim/"));
    assert_refused(
        "no-animations",
        entries,
        "tileset/general/anim/flower/0",
        "tilesets",
    );
}

type Shape = fn(&str) -> bool;

#[test]
fn a_refused_generation_leaves_the_previous_module_untouched() {
    let rom = RomFixture::new().emerald_header().finish();
    let shapes: [(&str, Shape); 3] = [
        ("one", |id| id == "tileset/general/anim/water/7"),
        ("anim", |id| id.starts_with("tileset/general/anim/water/")),
        ("all", |id| id.contains("/anim/")),
    ];
    for (label, drop) in shapes {
        let mut entries = complete_inventory();
        entries.retain(|e| !drop(&e.id));
        with_context(&format!("keep-{label}"), &rom, entries, |ctx| {
            let dir = std::env::temp_dir()
                .join(format!("rom-profile-keep-{label}-{}", std::process::id()));
            std::fs::create_dir_all(&dir).expect("dir");
            let out = dir.join("module.rs");
            std::fs::write(&out, b"previous module").expect("seed");
            let options = super::Options {
                rom: dir.join("rom.gba"),
                out: Some(out.clone()),
                map: None,
            };
            let err = super::generate(ctx, "sha", &options, out.clone()).expect_err("refused");
            assert!(
                matches!(err, GenRomProfileError::MissingPackEntry(_)),
                "{err:?}"
            );
            assert_eq!(std::fs::read(&out).expect("read"), b"previous module");
            assert_eq!(std::fs::read_dir(&dir).expect("ls").count(), 1);
            std::fs::remove_dir_all(&dir).expect("cleanup");
        });
    }
}
