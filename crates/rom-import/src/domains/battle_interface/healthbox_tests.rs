//! Health-box element table and battle bar reader tests over the synthetic
//! ROM the sibling `tests` module plants.
//!
//! The thirteen raw element records sit back to back at their production
//! lengths and the battle bar is one compressed stream of its three sheets.
//! The failure tests break one root at a time and require a typed error with
//! nothing queued.

use pack_format::{parse_directory, EntryKind, PackWriter};

use super::tests::{
    at, battle_bar, element_bytes, element_roots, health_box, lz77_literals, payload, rom, written,
    BAR_AT, BAR_ID, BATTLE_BAR_ROOT, ELEMENTS, SPARE_AT,
};
use super::write;
use crate::domains::DOMAINS;
use crate::error::{ImportError, Lz77Fault};
use crate::fixture::RomFixture;
use crate::reader::GbaPtr;
use crate::rom::Rom;
use crate::roots::{BlobRoot, Encoding, Roots};

#[test]
fn the_thirteen_table_rows_land_as_their_exact_bytes_in_table_order() {
    let (bytes, entries) =
        written(&health_box(element_roots(), None)).expect("the table is well-formed");
    let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
    let mut expected: Vec<&str> = ELEMENTS.iter().map(|(id, _)| *id).collect();
    expected.sort_unstable();
    assert_eq!(ids, expected);
    assert!(entries.iter().all(|e| e.kind == EntryKind::Raw));
    for (index, (id, tiles)) in ELEMENTS.iter().enumerate() {
        assert_eq!(
            payload(&bytes, &entries, id),
            element_bytes(index, *tiles),
            "{id}"
        );
    }
}

#[test]
fn the_battle_bar_decodes_to_its_three_sheets_concatenated() {
    let (bytes, entries) =
        written(&health_box(Vec::new(), Some(BATTLE_BAR_ROOT))).expect("the bar is well-formed");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].kind, EntryKind::Raw);
    assert_eq!(payload(&bytes, &entries, BAR_ID), battle_bar());
}

#[test]
fn the_health_box_family_adds_fourteen_entries_and_no_image_or_palette() {
    let roots = health_box(element_roots(), Some(BATTLE_BAR_ROOT));
    let (_, entries) = written(&roots).expect("every root is well-formed");
    assert_eq!(entries.len(), 14);
    assert_eq!(roots.images().count(), 0);
    assert_eq!(roots.palettes().count(), 0);
    let mut writer = PackWriter::new();
    for domain in DOMAINS {
        domain(&rom(), &roots, &mut writer).expect("every root is well-formed");
    }
    let bytes = writer.finish().expect("no duplicate ids");
    assert_eq!(parse_directory(&bytes).expect("a valid pack").len(), 14);
}

/// The failing roots leave the writer untouched, whatever else was valid.
fn assert_table_mismatch(roots: &Roots, field: &str) {
    let mut writer = PackWriter::new();
    let err = write(&rom(), roots, &mut writer).unwrap_err();
    assert!(
        matches!(
            &err,
            ImportError::StructMismatch {
                root: "gHealthboxElementsGfxTable",
                field: found
            } if *found == field
        ),
        "{field}: {err}"
    );
    let bytes = writer.finish().expect("an empty pack");
    assert!(parse_directory(&bytes).expect("a valid pack").is_empty());
}

#[test]
fn a_partial_table_is_refused() {
    let mut elements = element_roots();
    elements.pop();
    assert_table_mismatch(&health_box(elements, None), "element count");
    let mut elements = element_roots();
    elements.push(elements[0]);
    assert_table_mismatch(&health_box(elements, None), "element count");
}

#[test]
fn two_swapped_rows_are_refused_as_out_of_table_order() {
    // Same ids, same total bytes: only the order says which row is which.
    let mut elements = element_roots();
    elements.swap(0, 1);
    assert_table_mismatch(&health_box(elements, None), "element order");
}

#[test]
fn a_row_of_the_wrong_length_or_encoding_is_refused() {
    let mut elements = element_roots();
    elements[2].len -= 32;
    assert_table_mismatch(&health_box(elements, None), "element length");
    let mut elements = element_roots();
    elements[3].encoding = Encoding::Lz77;
    assert_table_mismatch(&health_box(elements, None), "element length");
}

#[test]
fn rows_that_are_not_back_to_back_are_refused() {
    // A gap, an overlap, and a row relocated to bytes that happen to match
    // are all a break in the one run the table is.
    for shift in [32i64, -32] {
        let mut elements = element_roots();
        let moved = i64::from(elements[5].addr.raw()) + shift;
        elements[5].addr = GbaPtr::at(u32::try_from(moved).expect("in range"));
        assert_table_mismatch(&health_box(elements, None), "element contiguity");
    }
    let mut elements = element_roots();
    elements[0].addr = at(SPARE_AT);
    assert_table_mismatch(&health_box(elements, None), "element contiguity");
}

#[test]
fn a_misshapen_battle_bar_is_refused() {
    let wrong = [
        BlobRoot {
            encoding: Encoding::Raw,
            ..BATTLE_BAR_ROOT
        },
        BlobRoot {
            len: 1280,
            ..BATTLE_BAR_ROOT
        },
        BlobRoot {
            id: "battle-interface/raw/healthbox_element/hpbar",
            ..BATTLE_BAR_ROOT
        },
    ];
    for bar in wrong {
        let mut writer = PackWriter::new();
        let err = write(&rom(), &health_box(Vec::new(), Some(bar)), &mut writer).unwrap_err();
        assert!(
            matches!(
                err,
                ImportError::StructMismatch {
                    root: "gBattleInterfaceGfx_BattleBar",
                    ..
                }
            ),
            "{err}"
        );
    }
}

#[test]
fn a_battle_bar_of_the_wrong_stream_length_is_a_size_mismatch() {
    // Declared shape is right; the stream in the ROM decodes to a different
    // length, which the shared reader refuses.
    let roots = health_box(Vec::new(), Some(BATTLE_BAR_ROOT));
    let bytes = RomFixture::new()
        .emerald_header()
        .write(BAR_AT, &lz77_literals(&battle_bar()[..1280]))
        .finish();
    let rom = Rom::from_bytes(bytes).expect("the fixture header is valid");
    let mut writer = PackWriter::new();
    let err = write(&rom, &roots, &mut writer).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::Lz77 {
                fault: Lz77Fault::SizeMismatch { .. },
                ..
            }
        ),
        "{err}"
    );
}
