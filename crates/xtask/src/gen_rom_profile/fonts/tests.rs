//! The keypad-icon sheet is found as plain row-major 4bpp tiles.

use pack_format::image_entry_from_tiles;
use rom_import::fixture::RomFixture;
use rom_import::Encoding;

use super::{locate_keypad_icons, KEYPAD_ICONS_ID};
use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::plan::SymbolExpectation;
use crate::gen_rom_profile::tests::with_context;

/// The sheet is 16x4 tiles of 32 bytes.
const SHEET_TILES: usize = 64;

/// Sixty-four tiles no two of which share a byte pattern.
fn sheet_tiles() -> Vec<u8> {
    (0..SHEET_TILES * 32)
        .map(|n| u8::try_from(n / 32).unwrap().wrapping_mul(37) ^ u8::try_from(n % 32).unwrap())
        .collect()
}

fn sheet_entry(tiles: &[u8]) -> pack_format::PackEntry {
    image_entry_from_tiles(KEYPAD_ICONS_ID.into(), tiles, 4, 128, 32, Some((1, 1))).expect("entry")
}

#[test]
fn the_sheet_is_found_once_as_row_major_tiles_and_named_for_its_static() {
    let tiles = sheet_tiles();
    let rom = RomFixture::new()
        .emerald_header()
        .write(0x10_0000, &tiles)
        .finish();

    with_context("keypad-found", &rom, vec![sheet_entry(&tiles)], |ctx| {
        let mut report = Vec::new();
        let plans = locate_keypad_icons(ctx, &mut report).expect("found");
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].id, KEYPAD_ICONS_ID);
        assert_eq!(plans[0].addr, 0x0810_0000);
        assert_eq!(plans[0].encoding, Encoding::Raw);
        assert_eq!((plans[0].width, plans[0].height), (128, 32));
        assert_eq!(plans[0].rom_bit_depth, 4);
        assert_eq!(plans[0].tile_count, 64);
        // Several walks give the same bytes for a whole-image grid; the
        // largest is reported, as upstream wrote it.
        assert_eq!((plans[0].metatile_width, plans[0].metatile_height), (16, 4));
        assert_eq!(
            report[0].symbol,
            SymbolExpectation::Local("sKeypadIconTiles".to_owned())
        );
    });
}

#[test]
fn a_sheet_with_two_tiles_swapped_is_not_found() {
    // No metatile walk reproduces a lone swap, so the bytes are not the sheet.
    let tiles = sheet_tiles();
    let mut swapped = tiles.clone();
    swapped[..64].rotate_left(32);
    let rom = RomFixture::new()
        .emerald_header()
        .write(0x10_0000, &swapped)
        .finish();

    with_context("keypad-swapped", &rom, vec![sheet_entry(&tiles)], |ctx| {
        let err = locate_keypad_icons(ctx, &mut Vec::new()).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::NotFound { id } if id == KEYPAD_ICONS_ID),
            "{err}"
        );
    });
}

#[test]
fn a_pack_without_the_sheet_is_refused() {
    let rom = RomFixture::new().emerald_header().finish();
    with_context("keypad-missing", &rom, Vec::new(), |ctx| {
        let err = locate_keypad_icons(ctx, &mut Vec::new()).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::MissingPackEntry(id) if id == KEYPAD_ICONS_ID),
            "{err}"
        );
    });
}
