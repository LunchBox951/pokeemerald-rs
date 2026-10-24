//! Battle-interface locator tests over a synthetic ROM and pack: the fixture
//! the sibling test modules extend, and the textbox sheet, palette and
//! tilemap coverage.

use pack_format::{image_entry_from_tiles, palette_entry, raw_entry, PackEntry};
use rom_import::fixture::RomFixture;

use super::frame_tests::{
    shadow_tiles, BALL_DISPLAY_AT, BALL_DISPLAY_ID, BALL_STATUS_AT, BALL_STATUS_ID, OPPONENT,
    OPPONENT_AT, PLAYER, PLAYER_AT, PLAYER_ID, SHADOW_AT, SHADOW_ID,
};
use super::healthbox_tests::{battle_bar, element_bytes, element_table, BAR_AT, ELEMENTS_AT};
use super::locate;
use crate::extract::scope::{battle_interface_ids, healthbox_battle_bar_id, healthbox_table_ids};
use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::tests::{distinctive_tiles, lz77_literals, with_context};

const SHEET_AT: usize = 0x10_0000;
const TEXTBOX_PALETTE_AT: usize = 0x11_0000;
const WINDOW_TEXT_AT: usize = 0x12_0000;
const PP_TEXT_AT: usize = 0x13_0000;
const TILEMAP_AT: usize = 0x14_0000;
/// Free ROM space for a second copy of a root.
const SPARE_AT: usize = 0x15_0000;

pub(super) const TEXTBOX_ID: &str = "battle-interface/image/textbox";
pub(super) const TEXTBOX_PALETTE_ID: &str = "battle-interface/palette/textbox";
pub(super) const WINDOW_TEXT_ID: &str = "battle-interface/palette/window_text";
pub(super) const PP_TEXT_ID: &str = "battle-interface/palette/pp_text";
pub(super) const TILEMAP_ID: &str = "battle-interface/raw/textbox_map";

/// `count` BGR555 colours starting from `seed`, as values and ROM bytes.
pub(super) fn palette_bytes(seed: u16, count: u16) -> (Vec<u16>, Vec<u8>) {
    let colors: Vec<u16> = (0..count).map(|n| (seed + n * 0x21) & 0x7FFF).collect();
    let bytes = colors.iter().flat_map(|c| c.to_le_bytes()).collect();
    (colors, bytes)
}

pub(super) fn tilemap() -> Vec<u8> {
    (0..4096u32)
        .map(|n| u8::try_from(n * 13 % 251).expect("below 251"))
        .collect()
}

/// A ROM and pack holding all twenty-four roots, one distinct copy each.
pub(super) fn fixture() -> (Vec<u8>, Vec<PackEntry>) {
    let tiles = distinctive_tiles(256);
    let (textbox_colors, textbox_bytes) = palette_bytes(0x1000, 32);
    let (window_colors, window_bytes) = palette_bytes(0x2000, 16);
    let (pp_colors, pp_bytes) = palette_bytes(0x3000, 16);
    let map = tilemap();
    let (status_colors, status_bytes) = palette_bytes(0x4000, 16);
    let (display_colors, display_bytes) = palette_bytes(0x5000, 16);
    let rom = RomFixture::new()
        .emerald_header()
        .write(SHEET_AT, &lz77_literals(&tiles))
        .write(TEXTBOX_PALETTE_AT, &lz77_literals(&textbox_bytes))
        .write(WINDOW_TEXT_AT, &lz77_literals(&window_bytes))
        .write(PP_TEXT_AT, &pp_bytes)
        .write(TILEMAP_AT, &lz77_literals(&map))
        .write(ELEMENTS_AT, &element_table())
        .write(BAR_AT, &lz77_literals(&battle_bar()))
        .write(PLAYER_AT, &lz77_literals(&PLAYER.tiles()))
        .write(OPPONENT_AT, &lz77_literals(&OPPONENT.tiles()))
        .write(SHADOW_AT, &lz77_literals(&shadow_tiles()))
        .write(BALL_STATUS_AT, &status_bytes)
        .write(BALL_DISPLAY_AT, &display_bytes)
        .finish();
    let mut entries = vec![
        image_entry_from_tiles(TEXTBOX_ID.to_owned(), &tiles, 4, 128, 128, None).expect("sheet"),
        palette_entry(TEXTBOX_PALETTE_ID.to_owned(), &textbox_colors).expect("palette"),
        palette_entry(WINDOW_TEXT_ID.to_owned(), &window_colors).expect("palette"),
        palette_entry(PP_TEXT_ID.to_owned(), &pp_colors).expect("palette"),
        raw_entry(TILEMAP_ID.to_owned(), map),
    ];
    entries.extend(
        healthbox_table_ids()
            .into_iter()
            .enumerate()
            .map(|(index, id)| raw_entry(id, element_bytes(index))),
    );
    entries.push(raw_entry(healthbox_battle_bar_id(), battle_bar()));
    entries.extend([
        PLAYER.entry(),
        OPPONENT.entry(),
        image_entry_from_tiles(SHADOW_ID.to_owned(), &shadow_tiles(), 4, 32, 8, None)
            .expect("shadow"),
        palette_entry(BALL_STATUS_ID.to_owned(), &status_colors).expect("palette"),
        palette_entry(BALL_DISPLAY_ID.to_owned(), &display_colors).expect("palette"),
    ]);
    (rom, entries)
}

/// Plant `bytes` over the spare area of `rom`.
pub(super) fn with_spare_copy(mut rom: Vec<u8>, bytes: &[u8]) -> Vec<u8> {
    rom[SPARE_AT..SPARE_AT + bytes.len()].copy_from_slice(bytes);
    rom
}

#[test]
fn the_fixed_inventory_is_one_sheet_three_palettes_and_one_tilemap() {
    assert_eq!(
        battle_interface_ids(),
        [
            TEXTBOX_ID,
            TEXTBOX_PALETTE_ID,
            WINDOW_TEXT_ID,
            PP_TEXT_ID,
            TILEMAP_ID
        ]
    );
}

#[test]
fn each_root_must_be_stored_in_its_own_encoding() {
    // Compressed palettes stored raw, the raw palette stored compressed, and
    // the tilemap stored raw each leave the root unfound.
    let (_, textbox_bytes) = palette_bytes(0x1000, 32);
    let (_, window_bytes) = palette_bytes(0x2000, 16);
    let (_, pp_bytes) = palette_bytes(0x3000, 16);
    let (_, status_bytes) = palette_bytes(0x4000, 16);
    let (_, display_bytes) = palette_bytes(0x5000, 16);
    let cases: [(&str, usize, Vec<u8>, Vec<u8>); 8] = [
        (
            TEXTBOX_PALETTE_ID,
            TEXTBOX_PALETTE_AT,
            textbox_bytes.clone(),
            lz77_literals(&textbox_bytes),
        ),
        (
            WINDOW_TEXT_ID,
            WINDOW_TEXT_AT,
            window_bytes.clone(),
            lz77_literals(&window_bytes),
        ),
        (
            PP_TEXT_ID,
            PP_TEXT_AT,
            lz77_literals(&pp_bytes),
            pp_bytes.clone(),
        ),
        (TILEMAP_ID, TILEMAP_AT, tilemap(), lz77_literals(&tilemap())),
        // The two ball palettes are `.gbapal` runs, never compressed
        // (`src/graphics.c:354,356`).
        (
            BALL_STATUS_ID,
            BALL_STATUS_AT,
            lz77_literals(&status_bytes),
            status_bytes.clone(),
        ),
        (
            BALL_DISPLAY_ID,
            BALL_DISPLAY_AT,
            lz77_literals(&display_bytes),
            display_bytes.clone(),
        ),
        // The frames and the shadow are `.4bpp.lz`, never raw tiles
        // (`src/graphics.c:628-629,1037`).
        (
            SHADOW_ID,
            SHADOW_AT,
            shadow_tiles(),
            lz77_literals(&shadow_tiles()),
        ),
        (
            PLAYER_ID,
            PLAYER_AT,
            PLAYER.tiles(),
            lz77_literals(&PLAYER.tiles()),
        ),
    ];
    for (id, at, replacement, original) in cases {
        let (mut rom, entries) = fixture();
        rom[at..at + original.len()].fill(0);
        rom[at..at + replacement.len()].copy_from_slice(&replacement);
        with_context("battle-interface-encoding", &rom, entries, |ctx| {
            let err = locate(ctx, &mut Vec::new()).unwrap_err();
            assert!(
                matches!(&err, GenRomProfileError::NotFound { id: missing, .. } if missing == id),
                "{id}: {err}"
            );
        });
    }
}

#[test]
fn a_compressed_palette_or_tilemap_present_twice_is_ambiguous() {
    let (_, textbox_bytes) = palette_bytes(0x1000, 32);
    let (_, window_bytes) = palette_bytes(0x2000, 16);
    for (id, copy) in [
        (TEXTBOX_PALETTE_ID, lz77_literals(&textbox_bytes)),
        (WINDOW_TEXT_ID, lz77_literals(&window_bytes)),
        (TILEMAP_ID, lz77_literals(&tilemap())),
    ] {
        let (rom, entries) = fixture();
        let rom = with_spare_copy(rom, &copy);
        with_context("battle-interface-twice", &rom, entries, |ctx| {
            let err = locate(ctx, &mut Vec::new()).unwrap_err();
            assert!(
                matches!(&err, GenRomProfileError::Ambiguous { id: named, .. } if named == id),
                "{id}: {err}"
            );
        });
    }
}
