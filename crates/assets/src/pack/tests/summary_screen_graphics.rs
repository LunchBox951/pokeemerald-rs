use super::super::{
    AssetPack, PackError, SummaryScreenMoveEffect, SummaryScreenPage, SummaryScreenPalette,
    SummaryScreenSheet,
};
use super::shared::{
    image_meta, pack_bytes, write_pack, Entry, IMAGE_KIND_TAG, INDEXED_8_BIT, PALETTE_KIND_TAG,
    RAW_KIND_TAG, TEXT_WINDOW_BIT_DEPTH,
};

const TILES_IMAGE: &str = "summary-screen-graphics/image/tiles";
const MOVE_SELECT_IMAGE: &str = "summary-screen-graphics/image/move_select";
const A_BUTTON_IMAGE: &str = "summary-screen-graphics/image/a_button";
const B_BUTTON_IMAGE: &str = "summary-screen-graphics/image/b_button";
const TILES_PALETTE: &str = "summary-screen-graphics/palette/tiles";
const MOVE_SELECT_PALETTE: &str = "summary-screen-graphics/palette/move_select";
const MARKINGS_PALETTE: &str = "summary-screen-graphics/palette/markings";
const PAGES: [(&str, usize); 5] = [
    ("summary-screen-graphics/raw/page_info", 2048),
    ("summary-screen-graphics/raw/page_info_egg", 2048),
    ("summary-screen-graphics/raw/page_skills", 2048),
    ("summary-screen-graphics/raw/page_battle_moves", 2048),
    ("summary-screen-graphics/raw/page_contest_moves", 2048),
];
const EFFECTS: [(&str, usize); 3] = [
    ("summary-screen-graphics/raw/effect_battle", 160),
    ("summary-screen-graphics/raw/effect_contest", 160),
    ("summary-screen-graphics/raw/effect_cancel", 128),
];
const STATUS: &str = "summary-screen-graphics/raw/status_tilemap";

fn image(id: &'static str, (width, height): (u32, u32), depth: u8, pixel: u8) -> Entry {
    Entry {
        id,
        kind_tag: IMAGE_KIND_TAG,
        meta: image_meta(width, height, depth),
        payload: (0..width * height)
            .map(|i| if i == 0 { pixel } else { (i % 16) as u8 })
            .collect(),
    }
}

fn palette(id: &'static str, colors: u16, first: u16) -> Entry {
    let mut payload = vec![0u8; usize::from(colors) * 2];
    payload[..2].copy_from_slice(&first.to_le_bytes());
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

fn page_fill(page: usize) -> u8 {
    0x10 + u8::try_from(page).unwrap()
}

fn effect_fill(effect: usize) -> u8 {
    0x20 + u8::try_from(effect).unwrap()
}

fn fixture_entries() -> Vec<Entry> {
    let mut entries = vec![
        image(TILES_IMAGE, (128, 120), TEXT_WINDOW_BIT_DEPTH, 15),
        image(MOVE_SELECT_IMAGE, (16, 128), TEXT_WINDOW_BIT_DEPTH, 15),
        image(A_BUTTON_IMAGE, (16, 16), TEXT_WINDOW_BIT_DEPTH, 15),
        image(B_BUTTON_IMAGE, (16, 16), TEXT_WINDOW_BIT_DEPTH, 14),
        palette(TILES_PALETTE, 128, 0x3AD6),
        palette(MOVE_SELECT_PALETTE, 16, 0x0001),
        palette(MARKINGS_PALETTE, 16, 0x6E54),
        raw(STATUS, 40, 0x31),
    ];
    for (n, (id, len)) in PAGES.into_iter().enumerate() {
        entries.push(raw(id, len, page_fill(n)));
    }
    for (n, (id, len)) in EFFECTS.into_iter().enumerate() {
        entries.push(raw(id, len, effect_fill(n)));
    }
    entries
}

fn load(name: &str, entries: Vec<Entry>) -> (AssetPack, std::path::PathBuf) {
    let path = write_pack(name, &pack_bytes(entries));
    (AssetPack::load(&path).unwrap(), path)
}

fn replaced(entry: Entry) -> Vec<Entry> {
    let mut entries: Vec<Entry> = fixture_entries()
        .into_iter()
        .filter(|e| e.id != entry.id)
        .collect();
    entries.push(entry);
    entries
}

fn assert_malformed(error: &PackError, id: &str, context: &str) {
    assert!(
        matches!(error, PackError::MalformedSummaryScreenGraphicsAsset { id: bad, .. } if bad == id),
        "{context}: {error:?}"
    );
}

#[test]
fn summary_screen_graphics_bundles_every_record() {
    let (pack, path) = load("summary-screen", fixture_entries());

    let handle = pack.summary_screen_graphics().unwrap();
    for (sheet, size, first) in [
        (SummaryScreenSheet::Tiles, (128, 120), 15),
        (SummaryScreenSheet::MoveSelect, (16, 128), 15),
        (SummaryScreenSheet::AButton, (16, 16), 15),
        (SummaryScreenSheet::BButton, (16, 16), 14),
    ] {
        let found = handle.sheet(sheet);
        assert_eq!((found.width, found.height), size, "{sheet:?}");
        assert_eq!(sheet.size(), size, "{sheet:?}");
        assert_eq!(found.bit_depth, 4, "{sheet:?}");
        assert_eq!(found.pixels[0], first, "{sheet:?}");
        assert_eq!(
            pack.summary_screen_sheet(sheet).unwrap().pixels,
            found.pixels
        );
    }
    for (palette, colors, first) in [
        (SummaryScreenPalette::Tiles, 128, 0x3AD6),
        (SummaryScreenPalette::MoveSelect, 16, 0x0001),
        (SummaryScreenPalette::Markings, 16, 0x6E54),
    ] {
        let found = handle.palette(palette);
        assert_eq!(found.color_count, colors, "{palette:?}");
        assert_eq!(palette.color_count(), colors, "{palette:?}");
        assert_eq!(found.color(0), Some(first), "{palette:?}");
        assert_eq!(
            pack.summary_screen_palette(palette).unwrap().color(0),
            Some(first)
        );
    }
    for (n, page) in SummaryScreenPage::ALL.into_iter().enumerate() {
        assert_eq!(
            handle.page_tilemap(page),
            vec![page_fill(n); PAGES[n].1].as_slice()
        );
        assert_eq!(
            pack.summary_screen_page_tilemap(page).unwrap(),
            handle.page_tilemap(page)
        );
    }
    for (n, effect) in SummaryScreenMoveEffect::ALL.into_iter().enumerate() {
        assert_eq!(effect.entry_count() * 2, EFFECTS[n].1);
        assert_eq!(
            handle.move_effect_tilemap(effect),
            vec![effect_fill(n); EFFECTS[n].1].as_slice()
        );
        assert_eq!(
            pack.summary_screen_move_effect_tilemap(effect).unwrap(),
            handle.move_effect_tilemap(effect)
        );
    }
    assert_eq!(handle.status_tilemap, vec![0x31; 40].as_slice());
    assert_eq!(
        pack.summary_screen_status_tilemap().unwrap(),
        handle.status_tilemap
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn missing_or_wrong_kind_entries_fail_without_substitution() {
    let ids: Vec<&str> = fixture_entries().iter().map(|e| e.id).collect();
    assert_eq!(ids.len(), 16);
    for missing in &ids {
        let entries = fixture_entries()
            .into_iter()
            .filter(|e| e.id != *missing)
            .collect();
        let (pack, path) = load("summary-screen-missing", entries);
        assert!(
            matches!(
                pack.summary_screen_graphics().unwrap_err(),
                PackError::UnknownAsset(id) if id == *missing
            ),
            "missing `{missing}`"
        );
        let _ = std::fs::remove_file(path);
    }

    // Each record of a kind is replaced with one of each other kind.
    for wrong in &ids {
        let own_kind = fixture_entries()
            .iter()
            .find(|e| e.id == *wrong)
            .unwrap()
            .kind_tag;
        let others: [Entry; 2] = match own_kind {
            IMAGE_KIND_TAG => [palette(wrong, 16, 0), raw(wrong, 28, 0)],
            PALETTE_KIND_TAG => [raw(wrong, 28, 0), image(wrong, (8, 8), 4, 0)],
            _ => [palette(wrong, 16, 0), image(wrong, (8, 8), 4, 0)],
        };
        for substitute in others {
            let (pack, path) = load("summary-screen-wrong-kind", replaced(substitute));
            assert!(
                matches!(
                    pack.summary_screen_graphics().unwrap_err(),
                    PackError::WrongKind { id, .. } if id == *wrong
                ),
                "wrong kind for `{wrong}`"
            );
            let _ = std::fs::remove_file(path);
        }
    }
}

#[test]
fn malformed_entries_are_rejected_on_read() {
    let mut cases = vec![
        // Wrong image geometry and depth for every sheet.
        (TILES_IMAGE, image(TILES_IMAGE, (64, 240), 4, 0)),
        (TILES_IMAGE, image(TILES_IMAGE, (128, 128), 4, 0)),
        (
            TILES_IMAGE,
            image(TILES_IMAGE, (128, 120), INDEXED_8_BIT, 0),
        ),
        (MOVE_SELECT_IMAGE, image(MOVE_SELECT_IMAGE, (32, 64), 4, 0)),
        (
            MOVE_SELECT_IMAGE,
            image(MOVE_SELECT_IMAGE, (16, 128), INDEXED_8_BIT, 0),
        ),
        (A_BUTTON_IMAGE, image(A_BUTTON_IMAGE, (8, 32), 4, 0)),
        (
            A_BUTTON_IMAGE,
            image(A_BUTTON_IMAGE, (16, 16), INDEXED_8_BIT, 0),
        ),
        (B_BUTTON_IMAGE, image(B_BUTTON_IMAGE, (16, 24), 4, 0)),
        // Wrong palette colour counts: one bank for the tile sheet, or the
        // tile sheet's banks for a one-bank palette.
        (TILES_PALETTE, palette(TILES_PALETTE, 16, 0)),
        (TILES_PALETTE, palette(TILES_PALETTE, 127, 0)),
        (TILES_PALETTE, palette(TILES_PALETTE, 129, 0)),
        (MOVE_SELECT_PALETTE, palette(MOVE_SELECT_PALETTE, 128, 0)),
        (MARKINGS_PALETTE, palette(MARKINGS_PALETTE, 15, 0)),
        // Maps a byte off, and the window's size in place of the stored size.
        (STATUS, raw(STATUS, 39, 0)),
        (STATUS, raw(STATUS, 41, 0)),
        (EFFECTS[0].0, raw(EFFECTS[0].0, 140, 0)),
        (EFFECTS[1].0, raw(EFFECTS[1].0, 161, 0)),
        (EFFECTS[2].0, raw(EFFECTS[2].0, 120, 0)),
        (EFFECTS[2].0, raw(EFFECTS[2].0, 160, 0)),
    ];
    for (id, _) in PAGES {
        cases.push((id, raw(id, 2047, 0)));
        cases.push((id, raw(id, 2049, 0)));
        cases.push((id, raw(id, 0, 0)));
    }
    for (n, (id, entry)) in cases.into_iter().enumerate() {
        let (pack, path) = load(&format!("summary-screen-malformed-{n}"), replaced(entry));
        assert_malformed(
            &pack.summary_screen_graphics().unwrap_err(),
            id,
            &format!("case {n}"),
        );
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn a_pixel_outside_one_bank_is_rejected_even_over_the_wide_tile_palette() {
    // The tile sheet's palette owns 128 colours, but a 4bpp tile still selects
    // within one 16-colour bank; the screen entry picks the bank.
    for pixel in [16, 31, 127, 255] {
        for (id, size) in [
            (TILES_IMAGE, (128, 120)),
            (MOVE_SELECT_IMAGE, (16, 128)),
            (A_BUTTON_IMAGE, (16, 16)),
            (B_BUTTON_IMAGE, (16, 16)),
        ] {
            let (pack, path) = load(
                "summary-screen-pixel",
                replaced(image(id, size, TEXT_WINDOW_BIT_DEPTH, pixel)),
            );
            assert_malformed(
                &pack.summary_screen_graphics().unwrap_err(),
                id,
                &format!("pixel {pixel} in `{id}`"),
            );
            let _ = std::fs::remove_file(path);
        }
    }
    let (pack, path) = load(
        "summary-screen-pixel-15",
        replaced(image(TILES_IMAGE, (128, 120), TEXT_WINDOW_BIT_DEPTH, 15)),
    );
    pack.summary_screen_graphics().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn individual_readers_need_only_their_own_records() {
    // A pack with only the buttons still serves the button sheets, which carry
    // no palette; the tile sheet and its palette are then unknown.
    let entries: Vec<Entry> = fixture_entries()
        .into_iter()
        .filter(|e| e.id == A_BUTTON_IMAGE || e.id == B_BUTTON_IMAGE)
        .collect();
    let (pack, path) = load("summary-screen-partial", entries);
    for button in [SummaryScreenSheet::AButton, SummaryScreenSheet::BButton] {
        assert_eq!(pack.summary_screen_sheet(button).unwrap().pixels.len(), 256);
    }
    assert!(matches!(
        pack.summary_screen_sheet(SummaryScreenSheet::Tiles).unwrap_err(),
        PackError::UnknownAsset(id) if id == TILES_IMAGE
    ));
    assert!(matches!(
        pack.summary_screen_palette(SummaryScreenPalette::Tiles).unwrap_err(),
        PackError::UnknownAsset(id) if id == TILES_PALETTE
    ));
    assert!(matches!(
        pack.summary_screen_status_tilemap().unwrap_err(),
        PackError::UnknownAsset(id) if id == STATUS
    ));
    let _ = std::fs::remove_file(path);
}
