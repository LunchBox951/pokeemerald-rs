use super::super::{AssetPack, PackError};
use super::shared::{
    image_meta, pack_bytes, write_pack, Entry, IMAGE_KIND_TAG, INDEXED_8_BIT, PALETTE_KIND_TAG,
    RAW_KIND_TAG, TEXT_WINDOW_BIT_DEPTH,
};

const PALETTE_COLORS: u16 = 16;
const PALETTE_ENTRY_BYTES: usize = 2;
const TILEMAP_BYTES: usize = 32 * 20 * 2;
const WALKING_SIZE: (u32, u32) = (144, 32);
const TEXT_WINDOW_PALETTE_ID: &str = "text-window/palette/text_pal2";
/// A palette's first colour is `BASE + its position`, so a test can tell
/// which palette a sheet was paired with.
const PALETTE_FIRST_COLOR_BASE: u16 = 0x0300;

/// `(stem, width, height, palette stem upstream binds it to)`: the packed
/// shape, so the cursor cuts are the 8x40 strips extraction publishes.
const SHEETS: [(&str, u32, u32, &str); 15] = [
    ("menu", 24, 128, "menu"),
    ("page_swap_frame", 40, 32, "buttons"),
    ("back_button", 40, 24, "buttons"),
    ("ok_button", 40, 24, "buttons"),
    ("page_swap_upper", 40, 8, "buttons"),
    ("page_swap_lower", 40, 8, "buttons"),
    ("page_swap_others", 40, 8, "buttons"),
    ("cursor", 8, 40, "cursor"),
    ("cursor_squished", 8, 40, "cursor"),
    ("cursor_filled", 8, 40, "cursor"),
    ("page_swap_button", 32, 16, "page_swap_upper"),
    ("input_arrow", 8, 8, "page_swap_others"),
    ("underscore", 8, 8, "page_swap_others"),
    ("pc_icon_off", 16, 24, "menu"),
    ("pc_icon_on", 16, 24, "menu"),
];
const PALETTES: [&str; 7] = [
    "menu",
    "page_swap_upper",
    "page_swap_lower",
    "page_swap_others",
    "buttons",
    "cursor",
    "keyboard",
];
const TILEMAPS: [&str; 4] = [
    "background",
    "keyboard_upper",
    "keyboard_lower",
    "keyboard_symbols",
];
const PLAYERS: [&str; 2] = ["brendan", "may"];

fn leak(id: String) -> &'static str {
    Box::leak(id.into_boxed_str())
}
fn image_id(stem: &str) -> &'static str {
    leak(format!("naming-screen/image/{stem}"))
}
fn palette_id(stem: &str) -> &'static str {
    leak(format!("naming-screen/palette/{stem}"))
}
fn raw_id(stem: &str) -> &'static str {
    leak(format!("naming-screen/raw/{stem}"))
}
fn walking_id(who: &str) -> &'static str {
    leak(format!("sprite/{who}/walking"))
}
fn player_palette_id(who: &str) -> &'static str {
    leak(format!("sprite/palette/{who}"))
}

fn image(id: &'static str, (width, height): (u32, u32), pixel: u8) -> Entry {
    Entry {
        id,
        kind_tag: IMAGE_KIND_TAG,
        meta: image_meta(width, height, TEXT_WINDOW_BIT_DEPTH),
        payload: vec![pixel; (width * height) as usize],
    }
}

fn palette(id: &'static str, colors: u16, first_color: u16) -> Entry {
    let mut payload = vec![0u8; usize::from(colors) * PALETTE_ENTRY_BYTES];
    payload[..PALETTE_ENTRY_BYTES].copy_from_slice(&first_color.to_le_bytes());
    Entry {
        id,
        kind_tag: PALETTE_KIND_TAG,
        meta: colors.to_le_bytes().to_vec(),
        payload,
    }
}

fn raw(id: &'static str, len: usize, fill: u8) -> Entry {
    Entry {
        id,
        kind_tag: RAW_KIND_TAG,
        meta: vec![],
        payload: vec![fill; len],
    }
}

fn first_color(stem: &str) -> u16 {
    let index = PALETTES.iter().position(|p| *p == stem).unwrap();
    PALETTE_FIRST_COLOR_BASE + u16::try_from(index).unwrap()
}

fn fixture_entries() -> Vec<Entry> {
    let mut entries = Vec::new();
    for (stem, width, height, _) in SHEETS {
        entries.push(image(image_id(stem), (width, height), 15));
    }
    for stem in PALETTES {
        entries.push(palette(palette_id(stem), PALETTE_COLORS, first_color(stem)));
    }
    for (index, stem) in (1u8..).zip(TILEMAPS) {
        entries.push(raw(raw_id(stem), TILEMAP_BYTES, index));
    }
    for (index, who) in (1u16..).zip(PLAYERS) {
        entries.push(image(walking_id(who), WALKING_SIZE, 15));
        entries.push(palette(
            player_palette_id(who),
            PALETTE_COLORS,
            0x0400 + index,
        ));
    }
    entries.push(palette(TEXT_WINDOW_PALETTE_ID, PALETTE_COLORS, 0x0500));
    entries
}

fn load(name: &str, entries: Vec<Entry>) -> AssetPack {
    let path = write_pack(name, &pack_bytes(entries));
    AssetPack::load(&path).unwrap()
}

fn replaced(id: &str, entry: Entry) -> Vec<Entry> {
    let mut entries: Vec<Entry> = fixture_entries()
        .into_iter()
        .filter(|e| e.id != id)
        .collect();
    entries.push(entry);
    entries
}

fn all_ids() -> Vec<&'static str> {
    fixture_entries().iter().map(|e| e.id).collect()
}

#[test]
fn contract_matches_the_extraction_manifest() {
    // 15 sheets + 7 palettes + 4 tilemaps, then the player icon and text window.
    assert_eq!(SHEETS.len() + PALETTES.len() + TILEMAPS.len(), 26);
    assert_eq!(fixture_entries().len(), 26 + 2 * 2 + 1);
}

#[test]
fn naming_screen_bundles_every_handle_group() {
    let pack = load("naming-screen", fixture_entries());
    let screen = pack.naming_screen().unwrap();

    let tilemaps = [
        screen.background_tilemap,
        screen.keyboard_upper_tilemap,
        screen.keyboard_lower_tilemap,
        screen.keyboard_symbols_tilemap,
    ];
    for (index, (bytes, stem)) in (1u8..).zip(tilemaps.into_iter().zip(TILEMAPS)) {
        assert_eq!(
            bytes,
            vec![index; TILEMAP_BYTES].as_slice(),
            "`{stem}` tilemap"
        );
    }

    let sheets = [
        screen.menu,
        screen.page_swap_frame,
        screen.back_button,
        screen.ok_button,
        screen.page_swap_upper,
        screen.page_swap_lower,
        screen.page_swap_others,
        screen.cursor,
        screen.cursor_squished,
        screen.cursor_filled,
        screen.page_swap_button,
        screen.input_arrow,
        screen.underscore,
        screen.pc_icon_off,
        screen.pc_icon_on,
    ];
    for (sheet, (stem, width, height, bound_palette)) in sheets.into_iter().zip(SHEETS) {
        assert_eq!(
            (sheet.image.width, sheet.image.height),
            (width, height),
            "`{stem}` dimensions"
        );
        assert_eq!(sheet.image.bit_depth, 4, "`{stem}` bit depth");
        assert_eq!(
            sheet.image.pixels.len(),
            (width * height) as usize,
            "`{stem}` pixel bytes"
        );
        assert_eq!(
            sheet.palette.color(0),
            Some(first_color(bound_palette)),
            "`{stem}` is paired with `{bound_palette}`"
        );
    }

    let palettes = [
        screen.menu_palette,
        screen.page_swap_upper_palette,
        screen.page_swap_lower_palette,
        screen.page_swap_others_palette,
        screen.buttons_palette,
        screen.cursor_palette,
        screen.keyboard_palette,
    ];
    for (palette, stem) in palettes.into_iter().zip(PALETTES) {
        assert_eq!(palette.color_count, PALETTE_COLORS, "`{stem}` colours");
        assert_eq!(
            palette.color(0),
            Some(first_color(stem)),
            "`{stem}` palette"
        );
    }
    let banks = screen.menu_palette_banks();
    for (bank, stem) in banks.into_iter().zip(&PALETTES[..6]) {
        assert_eq!(bank.color(0), Some(first_color(stem)), "bank `{stem}`");
    }

    assert_eq!(screen.player_icon_brendan.image.width, WALKING_SIZE.0);
    assert_eq!(screen.player_icon_brendan.palette.color(0), Some(0x0401));
    assert_eq!(screen.player_icon_may.image.height, WALKING_SIZE.1);
    assert_eq!(screen.player_icon_may.palette.color(0), Some(0x0402));
    assert_eq!(screen.text_window_palette.color(0), Some(0x0500));
}

#[test]
fn every_entry_reports_missing_and_wrong_kind() {
    for (n, id) in all_ids().into_iter().enumerate() {
        let missing: Vec<Entry> = fixture_entries()
            .into_iter()
            .filter(|e| e.id != id)
            .collect();
        let error = load(&format!("naming-screen-missing-{n}"), missing)
            .naming_screen()
            .unwrap_err();
        assert!(
            matches!(&error, PackError::UnknownAsset(bad) if bad == id),
            "missing `{id}`: {error:?}"
        );

        let is_raw = id.starts_with("naming-screen/raw/");
        let other_kind = if is_raw {
            image(id, (2, 2), 0)
        } else {
            raw(id, 1, 0)
        };
        let error = load(
            &format!("naming-screen-wrong-kind-{n}"),
            replaced(id, other_kind),
        )
        .naming_screen()
        .unwrap_err();
        assert!(
            matches!(&error, PackError::WrongKind { id: bad, .. } if bad == id),
            "wrong-kind `{id}`: {error:?}"
        );
    }
}

fn malformed_detail(n: usize, id: &'static str, entry: Entry) -> String {
    let error = load(&format!("naming-screen-malformed-{n}"), replaced(id, entry))
        .naming_screen()
        .unwrap_err();
    match error {
        PackError::MalformedNamingScreenAsset { id: bad, detail } if bad == id => detail,
        other => panic!("wrong error for malformed `{id}`: {other:?}"),
    }
}

#[test]
fn every_sheet_rejects_off_by_one_dimensions_depth_and_indices() {
    for (n, (stem, width, height, _)) in SHEETS.into_iter().enumerate() {
        let id = image_id(stem);
        let wrong_shapes = [
            ("one column narrower", (width - 1, height)),
            ("one column wider", (width + 1, height)),
            ("one row shorter", (width, height - 1)),
            ("one row taller", (width, height + 1)),
        ];
        for (label, shape) in wrong_shapes {
            let detail = malformed_detail(n, id, image(id, shape, 0));
            assert_eq!(
                detail,
                format!("is {}x{}: expected {width}x{height}", shape.0, shape.1),
                "`{stem}` {label}"
            );
        }
        let eight_bit = Entry {
            meta: image_meta(width, height, INDEXED_8_BIT),
            ..image(id, (width, height), 0)
        };
        assert_eq!(
            malformed_detail(n, id, eight_bit),
            "is 8-bit: expected 4-bit",
            "`{stem}` bit depth"
        );
        let mut last_pixel_outside = image(id, (width, height), 15);
        *last_pixel_outside.payload.last_mut().unwrap() = 16;
        assert!(
            malformed_detail(n, id, last_pixel_outside).contains("has pixel index 16"),
            "`{stem}` first invalid index at the final pixel"
        );
    }
}

#[test]
fn every_palette_rejects_one_colour_short_and_long() {
    let palette_ids = PALETTES
        .iter()
        .map(|stem| palette_id(stem))
        .chain(PLAYERS.iter().map(|who| player_palette_id(who)));
    for (n, id) in palette_ids.enumerate() {
        for (label, colors) in [("short", PALETTE_COLORS - 1), ("long", PALETTE_COLORS + 1)] {
            let detail = malformed_detail(n, id, palette(id, colors, 0));
            assert_eq!(
                detail,
                format!(
                    "declares {colors} colours in {} bytes: expected 16 colours",
                    usize::from(colors) * PALETTE_ENTRY_BYTES
                ),
                "`{id}` {label}"
            );
        }
    }
}

#[test]
fn every_tilemap_rejects_one_byte_short_and_long() {
    for (n, stem) in TILEMAPS.into_iter().enumerate() {
        let id = raw_id(stem);
        for len in [TILEMAP_BYTES - 1, TILEMAP_BYTES + 1] {
            let detail = malformed_detail(n, id, raw(id, len, 0));
            assert_eq!(detail, format!("is {len} bytes: expected {TILEMAP_BYTES}"));
        }
    }
}

#[test]
fn player_icon_sheets_are_checked_as_walking_sheets() {
    for (n, who) in PLAYERS.into_iter().enumerate() {
        let id = walking_id(who);
        let detail = malformed_detail(n, id, image(id, (WALKING_SIZE.0, 16), 0));
        assert_eq!(detail, "is 144x16: expected 144x32", "`{who}` height");
        let detail = malformed_detail(n, id, image(id, (WALKING_SIZE.0, WALKING_SIZE.1), 16));
        assert!(detail.contains("has pixel index 16"), "`{who}` index");
    }
}

#[test]
fn text_window_palette_two_is_the_shared_accessor_and_checked_as_a_bank() {
    let error = load(
        "naming-screen-text-window",
        replaced(
            TEXT_WINDOW_PALETTE_ID,
            palette(TEXT_WINDOW_PALETTE_ID, 2, 0),
        ),
    )
    .naming_screen()
    .unwrap_err();
    assert!(
        matches!(
            &error,
            PackError::MalformedTextWindowPalette { id, color_count: 2, .. }
                if id == TEXT_WINDOW_PALETTE_ID
        ),
        "{error:?}"
    );
    assert_eq!(
        load("naming-screen-text-window-ok", fixture_entries())
            .text_window_extra_palette(2)
            .unwrap()
            .color(0),
        Some(0x0500)
    );
}

#[test]
fn malformed_error_names_the_naming_screen() {
    let error = PackError::MalformedNamingScreenAsset {
        id: "naming-screen/raw/background".to_owned(),
        detail: "is 1 bytes: expected 1280".to_owned(),
    };
    assert_eq!(
        error.to_string(),
        "asset pack: naming-screen entry `naming-screen/raw/background` is 1 bytes: expected 1280"
    );
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_naming_screen_handle_validates() {
    let pack = AssetPack::load_repo().expect("run `cargo xtask extract` first");
    let screen = pack
        .naming_screen()
        .expect("every naming-screen entry should pass the typed handle");
    assert_eq!(screen.cursor.image.height, 40);
    assert_eq!(screen.background_tilemap.len(), TILEMAP_BYTES);
}
