//! Naming-screen packs and ROMs the locator refuses, by id.

use pack_format::{image_entry_from_tiles, palette_entry, raw_entry};

use super::super::locate;
use super::fixture::{
    fixture, image_id, palette_id, sheet_tiles, tilemap, tilemap_id, without, IMAGE_AT, IMAGE_SLOT,
    TILEMAP_AT,
};
use crate::extract::scope::naming_screen_ids;
use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::tests::{lz77_literals, with_context};

#[test]
fn each_root_must_be_stored_in_its_own_encoding() {
    // The menu sheet stored raw, a raw sheet stored compressed, and a tilemap
    // stored raw each leave the root unfound.
    let cases: [(String, usize, Vec<u8>, usize); 3] = [
        (
            image_id("menu"),
            IMAGE_AT,
            sheet_tiles(0),
            lz77_literals(&sheet_tiles(0)).len(),
        ),
        (
            image_id("input_arrow"),
            IMAGE_AT + 11 * IMAGE_SLOT,
            lz77_literals(&sheet_tiles(11)),
            sheet_tiles(11).len(),
        ),
        (
            tilemap_id("background"),
            TILEMAP_AT,
            tilemap(0),
            lz77_literals(&tilemap(0)).len(),
        ),
    ];
    for (id, at, replacement, original_len) in cases {
        let (mut rom, entries) = fixture();
        rom[at..at + original_len.max(replacement.len())].fill(0);
        rom[at..at + replacement.len()].copy_from_slice(&replacement);
        with_context("naming-screen-encoding", &rom, entries, |ctx| {
            let err = locate(ctx, &mut Vec::new()).unwrap_err();
            assert!(
                matches!(&err, GenRomProfileError::NotFound { id: missing } if *missing == id),
                "{id}: {err}"
            );
        });
    }
}

#[test]
fn a_missing_entry_or_family_is_refused_before_any_report_line() {
    for id in naming_screen_ids() {
        let (rom, entries) = fixture();
        with_context(
            "naming-screen-missing",
            &rom,
            without(entries, &id),
            |ctx| {
                let mut report = Vec::new();
                let err = locate(ctx, &mut report).unwrap_err();
                assert!(
                    matches!(&err, GenRomProfileError::MissingPackEntry(missing) if *missing == id),
                    "{err}"
                );
                assert!(report.is_empty());
            },
        );
    }
    let (rom, _) = fixture();
    with_context("naming-screen-no-family", &rom, Vec::new(), |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert!(
            matches!(err, GenRomProfileError::MissingPackEntry(_)),
            "{err}"
        );
    });
}

#[test]
fn a_surplus_entry_is_refused_before_any_report_line() {
    let (rom, mut entries) = fixture();
    entries.push(raw_entry("naming-screen/raw/extra".into(), vec![0]));
    with_context("naming-screen-surplus", &rom, entries, |ctx| {
        let mut report = Vec::new();
        let err = locate(ctx, &mut report).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == "naming-screen/raw/extra"),
            "{err}"
        );
        assert!(report.is_empty());
    });
}

#[test]
fn a_wrong_kind_entry_is_refused() {
    for (id, wrong) in [
        (
            image_id("menu"),
            palette_entry(image_id("menu"), &[0; 16]).expect("palette"),
        ),
        (
            palette_id("keyboard"),
            raw_entry(palette_id("keyboard"), vec![0; 32]),
        ),
        (
            tilemap_id("background"),
            palette_entry(tilemap_id("background"), &[0; 16]).expect("palette"),
        ),
    ] {
        let (rom, entries) = fixture();
        let mut entries = without(entries, &id);
        entries.push(wrong);
        with_context("naming-screen-wrong-kind", &rom, entries, |ctx| {
            let err = locate(ctx, &mut Vec::new()).unwrap_err();
            assert!(
                matches!(err, GenRomProfileError::WrongPackEntryKind { .. }),
                "{id}: {err}"
            );
        });
    }
}

#[test]
fn a_valid_entry_of_the_wrong_production_shape_is_refused_by_name() {
    let (_, cursor_bytes) = (0, sheet_tiles(7));
    let wrong_cursor = {
        // 16x24 holds six tiles: the whole-sheet cursor upstream does not store.
        let mut tiles = cursor_bytes;
        tiles.extend_from_slice(&[0; 32]);
        image_entry_from_tiles(image_id("cursor"), &tiles, 4, 16, 24, None).expect("entry")
    };
    let cases = [
        (image_id("cursor"), wrong_cursor),
        (
            palette_id("cursor"),
            palette_entry(palette_id("cursor"), &[0x0421; 8]).expect("palette"),
        ),
        (
            tilemap_id("background"),
            raw_entry(tilemap_id("background"), vec![0; 640]),
        ),
    ];
    for (id, wrong) in cases {
        let (rom, entries) = fixture();
        let mut entries = without(entries, &id);
        entries.push(wrong);
        with_context("naming-screen-wrong-shape", &rom, entries, |ctx| {
            let mut report = Vec::new();
            let err = locate(ctx, &mut report).unwrap_err();
            assert!(
                matches!(&err, GenRomProfileError::EntryShape { id: named, .. } if *named == id),
                "{id}: {err}"
            );
            assert!(report.is_empty());
        });
    }
}
