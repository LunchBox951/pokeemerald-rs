//! Deleting any fixed root from a complete synthetic pack is refused before
//! a report line is written.

use pack_format::{raw_entry, PackEntry};
use rom_import::fixture::RomFixture;

use super::error::GenRomProfileError;
use super::tests::with_context;
use super::{locate_profile, tilesets, title};
use crate::extract::scope::{title_ids, TILESETS};

/// A placeholder for every fixed root; presence is all the preflight reads.
fn complete_inventory() -> Vec<PackEntry> {
    TILESETS
        .iter()
        .map(|tileset| tileset.tiles_id())
        .chain(title_ids())
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
    let ids = title_ids();
    let count = |prefix: &str| ids.iter().filter(|id| id.starts_with(prefix)).count();
    assert_eq!(count("title/image/"), 6);
    assert_eq!(count("title/raw/"), 3);
    assert_eq!(count("title/palette/"), 5);
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
