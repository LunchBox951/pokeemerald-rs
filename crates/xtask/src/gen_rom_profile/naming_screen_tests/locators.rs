//! Naming-screen roots are located at their planted addresses, one array
//! or one copy at a time.

use rom_import::Encoding;

use super::super::locate;
use super::fixture::{
    addr, bank, default_seeds, fixture, fixture_with, image_id, palette_id, palette_seed,
    sheet_tiles, table_bytes, tilemap, tilemap_id, with_spare_copy, IMAGE_AT, IMAGE_SLOT,
    KEYBOARD_AT, MENU_TABLE_AT, PALETTES, SHEETS, TILEMAPS, TILEMAP_AT,
};
use crate::extract::scope::naming_screen_ids;
use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::plan::Resolution;
use crate::gen_rom_profile::tests::{lz77_literals, with_context};

#[test]
fn the_fixed_inventory_is_fifteen_sheets_seven_palettes_and_four_tilemaps() {
    let ids = naming_screen_ids();
    assert_eq!(ids.len(), 26);
    assert_eq!(
        ids.iter().filter(|id| id.contains("/image/")).count(),
        SHEETS.len()
    );
    assert_eq!(
        ids.iter().filter(|id| id.contains("/palette/")).count(),
        PALETTES.len()
    );
    assert_eq!(
        ids.iter().filter(|id| id.contains("/raw/")).count(),
        TILEMAPS.len()
    );
}

#[test]
fn every_root_is_located_at_its_planted_address_in_its_own_encoding() {
    let (rom, entries) = fixture();
    with_context("naming-screen-plan", &rom, entries, |ctx| {
        let mut report = Vec::new();
        let plan = locate(ctx, &mut report).expect("located");
        assert_eq!(plan.images.len(), 15);
        assert_eq!(plan.palettes.len(), 7);
        assert_eq!(plan.tilemaps.len(), 4);
        assert_eq!(report.len(), 26);

        for (index, &(stem, width, height, tiles, _)) in SHEETS.iter().enumerate() {
            let image = plan
                .images
                .iter()
                .find(|image| image.id == image_id(stem))
                .expect(stem);
            let encoding = if stem == "menu" {
                Encoding::Lz77
            } else {
                Encoding::Raw
            };
            assert_eq!(image.addr, addr(IMAGE_AT + index * IMAGE_SLOT), "{stem}");
            assert_eq!(image.encoding, encoding, "{stem}");
            assert_eq!(
                (image.rom_bit_depth, image.pack_bit_depth),
                (4, 4),
                "{stem}"
            );
            assert_eq!((image.width, image.height), (width, height), "{stem}");
            assert_eq!(usize::try_from(image.tile_count).unwrap(), tiles, "{stem}");
        }
        for (index, stem) in PALETTES.iter().enumerate() {
            let palette = plan
                .palettes
                .iter()
                .find(|palette| palette.id == palette_id(stem))
                .expect(stem);
            let at = if index < 6 {
                MENU_TABLE_AT + index * 32
            } else {
                KEYBOARD_AT
            };
            assert_eq!(palette.addr, addr(at), "{stem}");
            assert_eq!((palette.encoding, palette.color_count), (Encoding::Raw, 16));
        }
        for (index, (stem, _)) in TILEMAPS.iter().enumerate() {
            let map = plan
                .tilemaps
                .iter()
                .find(|map| map.id == tilemap_id(stem))
                .expect(stem);
            assert_eq!(map.addr, addr(TILEMAP_AT + index * 0x1000), "{stem}");
            assert_eq!((map.encoding, map.len), (Encoding::Lz77, 1280), "{stem}");
        }
        assert!(
            report
                .iter()
                .all(|line| line.resolution != Resolution::ArbitraryAmongIdentical),
            "no naming-screen root may be chosen among copies"
        );
    });
}

#[test]
fn a_cursor_is_the_five_stored_tiles_even_with_a_nonzero_sixth_beside_it() {
    let (mut rom, entries) = fixture();
    let cursor = 7;
    let after = IMAGE_AT + cursor * IMAGE_SLOT + 5 * 32;
    rom[after..after + 32].fill(0xFF);
    with_context("naming-screen-cursor", &rom, entries, |ctx| {
        let plan = locate(ctx, &mut Vec::new()).expect("located");
        for stem in ["cursor", "cursor_squished", "cursor_filled"] {
            let image = plan
                .images
                .iter()
                .find(|image| image.id == image_id(stem))
                .expect(stem);
            assert_eq!(
                (image.width, image.height, image.tile_count),
                (8, 40, 5),
                "{stem}"
            );
        }
    });
}

#[test]
fn a_bank_copied_below_the_table_does_not_move_a_menu_row() {
    let (rom, entries) = fixture();
    // A lone copy of row 4 at the spare area, after the table.
    let rom = with_spare_copy(rom, &bank(palette_seed(4)).1);
    with_context("naming-screen-decoy", &rom, entries, |ctx| {
        let plan = locate(ctx, &mut Vec::new()).expect("the table decides, not a lone bank");
        let buttons = plan
            .palettes
            .iter()
            .find(|palette| palette.id == palette_id("buttons"))
            .expect("located");
        assert_eq!(buttons.addr, addr(MENU_TABLE_AT + 4 * 32));
    });
}

#[test]
fn equal_rows_stay_ordinal_at_their_own_stride() {
    // `page_swap_upper` and `page_swap_lower` carrying the same colours.
    let mut seeds = default_seeds();
    seeds[2] = seeds[1];
    let (rom, entries) = fixture_with(seeds);
    with_context("naming-screen-equal-rows", &rom, entries, |ctx| {
        let mut report = Vec::new();
        let plan = locate(ctx, &mut report).expect("the array settles equal rows");
        let at = |stem: &str| {
            plan.palettes
                .iter()
                .find(|palette| palette.id == palette_id(stem))
                .expect(stem)
                .addr
        };
        assert_eq!(at("page_swap_upper"), addr(MENU_TABLE_AT + 32));
        assert_eq!(at("page_swap_lower"), addr(MENU_TABLE_AT + 64));
        assert!(report
            .iter()
            .filter(|line| line.id.starts_with("naming-screen/palette/"))
            .all(|line| line.resolution != Resolution::ArbitraryAmongIdentical));
    });
}

#[test]
fn a_menu_table_missing_a_row_or_present_twice_is_refused() {
    // Row 3 overwritten: no complete array remains.
    let (mut rom, entries) = fixture();
    let row = MENU_TABLE_AT + 3 * 32;
    rom[row..row + 32].fill(0x55);
    with_context("naming-screen-broken-table", &rom, entries, |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::NotFound { id } if *id == palette_id("menu")),
            "{err}"
        );
    });

    let (rom, entries) = fixture();
    let rom = with_spare_copy(rom, &table_bytes(default_seeds()));
    with_context("naming-screen-two-tables", &rom, entries, |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::Ambiguous { id, .. } if *id == palette_id("menu")),
            "{err}"
        );
    });
}

#[test]
fn a_sheet_or_the_keyboard_bank_present_twice_is_ambiguous_not_a_first_match() {
    let mut copies: Vec<(String, Vec<u8>)> = vec![
        (image_id("ok_button"), sheet_tiles(3)),
        (image_id("cursor"), sheet_tiles(7)),
        (image_id("pc_icon_on"), sheet_tiles(14)),
        (image_id("underscore"), sheet_tiles(12)),
        (image_id("menu"), lz77_literals(&sheet_tiles(0))),
        (palette_id("keyboard"), bank(palette_seed(6)).1),
    ];
    copies.push((tilemap_id("background"), lz77_literals(&tilemap(0))));
    for (id, copy) in copies {
        let (rom, entries) = fixture();
        let rom = with_spare_copy(rom, &copy);
        with_context("naming-screen-twice", &rom, entries, |ctx| {
            let err = locate(ctx, &mut Vec::new()).unwrap_err();
            assert!(
                matches!(&err, GenRomProfileError::Ambiguous { id: named, .. } if *named == id),
                "{id}: {err}"
            );
        });
    }
}
