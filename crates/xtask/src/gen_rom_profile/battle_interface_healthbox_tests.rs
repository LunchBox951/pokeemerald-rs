//! Health-box element table and battle-bar locator tests over a synthetic ROM
//! and pack; the fixture they extend is in the sibling `tests` module.

use pack_format::raw_entry;
use rom_import::Encoding;

use super::locate;
use super::tests::{fixture, with_spare_copy};
use crate::extract::scope::{healthbox_battle_bar_id, healthbox_table_ids};
use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::tests::{distinctive_tiles, lz77_literals, with_context};

pub(super) const ELEMENTS_AT: usize = 0x16_0000;
pub(super) const BAR_AT: usize = 0x17_0000;

/// Tile counts of the thirteen table rows, in table order.
pub(super) const ELEMENT_TILES: [usize; 13] = [12, 9, 15, 11, 18, 1, 4, 1, 15, 15, 15, 1, 1];
const BAR_TILES: usize = 41;

/// The bytes of table row `index`. The three one-tile rows at 5, 11 and 12
/// are the same tile, as the real blank frame ends are alike, so a search for
/// any of them alone cannot tell the rows apart.
pub(super) fn element_bytes(index: usize) -> Vec<u8> {
    if matches!(index, 5 | 11 | 12) {
        return vec![0xA5; 32];
    }
    distinctive_tiles(ELEMENT_TILES[index])
        .into_iter()
        .map(|byte| byte.wrapping_add(u8::try_from(index).expect("small") * 3))
        .collect()
}

pub(super) fn element_table() -> Vec<u8> {
    (0..ELEMENT_TILES.len()).flat_map(element_bytes).collect()
}

pub(super) fn battle_bar() -> Vec<u8> {
    distinctive_tiles(BAR_TILES)
        .into_iter()
        .map(|byte| byte ^ 0x5C)
        .collect()
}

#[test]
fn the_elements_are_consecutive_slices_of_one_unique_run_in_table_order() {
    let (rom, entries) = fixture();
    with_context("battle-interface-table", &rom, entries, |ctx| {
        let mut report = Vec::new();
        let plan = locate(ctx, &mut report).expect("located");
        let base = 0x0800_0000 + u32::try_from(ELEMENTS_AT).expect("small");
        let mut offset = 0;
        let ids = healthbox_table_ids();
        assert_eq!(plan.healthbox_elements.len(), 13);
        for (index, element) in plan.healthbox_elements.iter().enumerate() {
            let len = u32::try_from(ELEMENT_TILES[index] * 32).expect("small");
            assert_eq!(element.id, ids[index]);
            assert_eq!(
                (element.addr, element.encoding, element.len),
                (base + offset, Encoding::Raw, len),
                "{}",
                element.id
            );
            offset += len;
        }
        assert_eq!(offset, 118 * 32);
        let bar = &plan.battle_bar;
        assert_eq!(
            (bar.id.as_str(), bar.addr, bar.encoding, bar.len),
            (
                "battle-interface/raw/battle_bar",
                0x0817_0000,
                Encoding::Lz77,
                1312
            )
        );
        // The identical one-tile rows still land at their own table offsets.
        let at = |name: &str| {
            plan.healthbox_elements
                .iter()
                .find(|e| e.id.ends_with(name))
                .expect("planned")
                .addr
        };
        assert_ne!(at("misc_frameend"), at("healthbox_doubles_frameend"));
        assert_ne!(
            at("healthbox_doubles_frameend"),
            at("healthbox_doubles_frameend_bar")
        );
    });
}

#[test]
fn the_checkout_inventory_follows_the_importers_table_and_bar() {
    let importer: Vec<&str> = rom_import::HEALTHBOX_ELEMENTS
        .iter()
        .map(|(id, _)| *id)
        .collect();
    assert_eq!(healthbox_table_ids(), importer);
    assert_eq!(
        healthbox_battle_bar_id(),
        rom_import::HEALTHBOX_BATTLE_BAR.0
    );
    let tiles: Vec<usize> = rom_import::HEALTHBOX_ELEMENTS
        .iter()
        .map(|(_, tiles)| usize::try_from(*tiles).expect("small"))
        .collect();
    assert_eq!(tiles, ELEMENT_TILES);
    assert_eq!(rom_import::HEALTHBOX_BATTLE_BAR.1 as usize, BAR_TILES);
}

#[test]
fn a_row_boundary_moved_inside_the_run_is_refused() {
    // hpbar loses its last tile to expbar: the concatenation, and so the
    // unique ROM hit, is unchanged, but the rows are not the table's.
    let (rom, mut entries) = fixture();
    let hpbar = healthbox_table_ids()[0].clone();
    let expbar = healthbox_table_ids()[1].clone();
    let (first, second) = (element_bytes(0), element_bytes(1));
    let (kept, moved) = first.split_at(first.len() - 32);
    let shifted: Vec<u8> = moved.iter().chain(&second).copied().collect();
    entries.retain(|entry| entry.id != hpbar && entry.id != expbar);
    entries.push(raw_entry(hpbar.clone(), kept.to_vec()));
    entries.push(raw_entry(expbar, shifted));
    with_context("battle-interface-shifted-row", &rom, entries, |ctx| {
        let mut report = Vec::new();
        let err = locate(ctx, &mut report).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::StructMismatch { id, .. } if *id == hpbar),
            "{err}"
        );
        assert!(
            report
                .iter()
                .all(|line| !line.id.contains("healthbox_element")),
            "no element root is reported"
        );
    });
}

#[test]
fn an_oversized_element_or_bar_payload_is_a_struct_mismatch_not_a_panic() {
    // Zeroed allocation: the kernel maps it lazily, so no 4 GiB is touched.
    let payload = vec![0u8; usize::try_from(u32::MAX).expect("64-bit host") + 1];
    for (expected_id, tiles) in [
        rom_import::HEALTHBOX_ELEMENTS[0],
        rom_import::HEALTHBOX_BATTLE_BAR,
    ] {
        let err = super::check_tiles(expected_id, expected_id, &payload, tiles).unwrap_err();
        assert!(
            matches!(err, GenRomProfileError::StructMismatch { .. }),
            "{expected_id}: {err}"
        );
    }
}

#[test]
fn a_battle_bar_of_the_wrong_length_is_refused() {
    let (rom, mut entries) = fixture();
    let bar_id = healthbox_battle_bar_id();
    entries.retain(|entry| entry.id != bar_id);
    entries.push(raw_entry(bar_id.clone(), battle_bar()[..1280].to_vec()));
    with_context("battle-interface-short-bar", &rom, entries, |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::StructMismatch { id, .. } if *id == bar_id),
            "{err}"
        );
    });
}

#[test]
fn rows_swapped_in_the_rom_are_not_found_as_the_table() {
    // Rows 0 and 1 exchanged: every row's own bytes are still in the ROM, but
    // not as the table in order.
    let (mut rom, entries) = fixture();
    let first = element_bytes(0);
    let second = element_bytes(1);
    let swapped: Vec<u8> = second.iter().chain(&first).copied().collect();
    rom[ELEMENTS_AT..ELEMENTS_AT + swapped.len()].copy_from_slice(&swapped);
    with_context("battle-interface-table-swapped", &rom, entries, |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::NotFound { id } if *id == healthbox_table_ids()[0]),
            "{err}"
        );
    });
}

#[test]
fn a_table_or_bar_present_twice_is_ambiguous() {
    let (rom, entries) = fixture();
    let rom = with_spare_copy(rom, &element_table());
    with_context("battle-interface-table-twice", &rom, entries, |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::Ambiguous { id, .. } if *id == healthbox_table_ids()[0]),
            "{err}"
        );
    });
    let (rom, entries) = fixture();
    let rom = with_spare_copy(rom, &lz77_literals(&battle_bar()));
    with_context("battle-interface-bar-twice", &rom, entries, |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::Ambiguous { id, .. } if *id == healthbox_battle_bar_id()),
            "{err}"
        );
    });
}

#[test]
fn a_battle_bar_stored_raw_is_not_found() {
    let (mut rom, entries) = fixture();
    let stream = lz77_literals(&battle_bar());
    rom[BAR_AT..BAR_AT + stream.len()].fill(0);
    rom[BAR_AT..BAR_AT + BAR_TILES * 32].copy_from_slice(&battle_bar());
    with_context("battle-interface-bar-raw", &rom, entries, |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::NotFound { id } if *id == healthbox_battle_bar_id()),
            "{err}"
        );
    });
}

#[test]
fn the_battle_bar_is_the_three_sheets_in_order_not_any_other_order() {
    // The ROM holds the stream of numbers1 first; the pack's order is
    // hpbar_anim_unused first, so the stream is not the bar.
    let (mut rom, entries) = fixture();
    let bar = battle_bar();
    let (head, tail) = bar.split_at(18 * 32);
    let reordered: Vec<u8> = tail.iter().chain(head).copied().collect();
    let stream = lz77_literals(&bar);
    rom[BAR_AT..BAR_AT + stream.len()].fill(0);
    let reordered_stream = lz77_literals(&reordered);
    rom[BAR_AT..BAR_AT + reordered_stream.len()].copy_from_slice(&reordered_stream);
    with_context("battle-interface-bar-order", &rom, entries, |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::NotFound { id } if *id == healthbox_battle_bar_id()),
            "{err}"
        );
    });
}
