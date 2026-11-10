use super::super::{AssetPack, PackError, PartyMenuButton, PartyMenuSlot};
use super::shared::{
    image_meta, pack_bytes, write_pack, Entry, IMAGE_KIND_TAG, INDEXED_8_BIT, PALETTE_KIND_TAG,
    RAW_KIND_TAG, TEXT_WINDOW_BIT_DEPTH,
};

const BG_IMAGE: &str = "party-menu/image/bg";
const BG_PALETTE: &str = "party-menu/palette/bg";
const BG_MAP: &str = "party-menu/raw/bg";
const BALL_IMAGE: &str = "party-menu/image/pokeball";
const BALL_PALETTE: &str = "party-menu/palette/pokeball";
const SMALL_BALL_IMAGE: &str = "party-menu/image/pokeball_small";
const HELD_IMAGE: &str = "party-menu/image/hold_icons";
const HELD_PALETTE: &str = "party-menu/palette/hold_icons";
const SLOTS: [(&str, usize); 5] = [
    ("party-menu/raw/slot_main", 70),
    ("party-menu/raw/slot_main_no_hp", 70),
    ("party-menu/raw/slot_wide", 54),
    ("party-menu/raw/slot_wide_no_hp", 54),
    ("party-menu/raw/slot_wide_empty", 54),
];
const CONFIRM: &str = "party-menu/raw/confirm_button";
const CANCEL: &str = "party-menu/raw/cancel_button";

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

fn slot_fill(slot: usize) -> u8 {
    0x30 + u8::try_from(slot).unwrap()
}

fn fixture_entries() -> Vec<Entry> {
    let mut entries = vec![
        image(BG_IMAGE, (8, 496), TEXT_WINDOW_BIT_DEPTH, 15),
        palette(BG_PALETTE, 176, 0x3A6F),
        raw(BG_MAP, 2048, 0x11),
        image(BALL_IMAGE, (32, 64), TEXT_WINDOW_BIT_DEPTH, 15),
        palette(BALL_PALETTE, 16, 0x6605),
        image(SMALL_BALL_IMAGE, (16, 96), TEXT_WINDOW_BIT_DEPTH, 15),
        image(HELD_IMAGE, (8, 16), TEXT_WINDOW_BIT_DEPTH, 15),
        palette(HELD_PALETTE, 16, 0x3727),
        raw(CONFIRM, 28, 0x21),
        raw(CANCEL, 28, 0x22),
    ];
    for (n, (id, len)) in SLOTS.into_iter().enumerate() {
        entries.push(raw(id, len, slot_fill(n)));
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
        matches!(error, PackError::MalformedPartyMenuAsset { id: bad, .. } if bad == id),
        "{context}: {error:?}"
    );
}

#[test]
fn party_menu_bundles_every_record() {
    let (pack, path) = load("party-menu", fixture_entries());

    let handle = pack.party_menu().unwrap();
    let bg = handle.background;
    assert_eq!((bg.sheet.width, bg.sheet.height), (8, 496));
    assert_eq!(bg.sheet.bit_depth, 4);
    assert_eq!(bg.palette.color_count, 176);
    assert_eq!(bg.palette.color(0), Some(0x3A6F));
    assert_eq!(bg.tilemap, vec![0x11; 2048].as_slice());
    assert_eq!(
        (handle.poke_ball.image.width, handle.poke_ball.image.height),
        (32, 64)
    );
    assert_eq!(handle.poke_ball.palette.color(0), Some(0x6605));
    let small = handle.poke_ball_small;
    assert_eq!((small.width, small.height, small.bit_depth), (16, 96, 4));
    assert_eq!(small.pixels.len(), 16 * 96);
    assert_eq!(small.pixels[0], 15);
    assert_eq!(small.pixels[1], 1);
    assert_eq!(
        pack.party_menu_poke_ball_small().unwrap().pixels,
        small.pixels
    );
    assert_eq!(
        (handle.held_item.image.width, handle.held_item.image.height),
        (8, 16)
    );
    assert_eq!(handle.held_item.palette.color_count, 16);
    assert_eq!(handle.held_item.palette.color(0), Some(0x3727));
    assert_eq!(handle.confirm_button, vec![0x21; 28].as_slice());
    assert_eq!(handle.cancel_button, vec![0x22; 28].as_slice());
    assert_eq!(
        handle.button(PartyMenuButton::Confirm),
        handle.confirm_button
    );
    assert_eq!(handle.button(PartyMenuButton::Cancel), handle.cancel_button);
    for (n, slot) in PartyMenuSlot::ALL.into_iter().enumerate() {
        let (width, height) = slot.size_in_tiles();
        assert_eq!(width * height, SLOTS[n].1);
        assert_eq!(handle.slot(slot), vec![slot_fill(n); SLOTS[n].1].as_slice());
        assert_eq!(
            pack.party_menu_slot_tilemap(slot).unwrap(),
            handle.slot(slot)
        );
    }

    let _ = std::fs::remove_file(path);
}

#[test]
fn missing_or_wrong_kind_entries_fail_without_substitution() {
    let ids: Vec<&str> = fixture_entries().iter().map(|e| e.id).collect();
    assert_eq!(ids.len(), 15);
    for missing in &ids {
        let entries = fixture_entries()
            .into_iter()
            .filter(|e| e.id != *missing)
            .collect();
        let (pack, path) = load("party-menu-missing", entries);
        assert!(
            matches!(
                pack.party_menu().unwrap_err(),
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
            let (pack, path) = load("party-menu-wrong-kind", replaced(substitute));
            assert!(
                matches!(
                    pack.party_menu().unwrap_err(),
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
        // Wrong image geometry, depth, and a background whose cut was lost.
        (BG_IMAGE, image(BG_IMAGE, (64, 64), 4, 0)),
        (BG_IMAGE, image(BG_IMAGE, (8, 504), 4, 0)),
        (BG_IMAGE, image(BG_IMAGE, (8, 496), INDEXED_8_BIT, 0)),
        (BALL_IMAGE, image(BALL_IMAGE, (16, 96), 4, 0)),
        (BALL_IMAGE, image(BALL_IMAGE, (32, 64), INDEXED_8_BIT, 0)),
        (HELD_IMAGE, image(HELD_IMAGE, (16, 8), 4, 0)),
        // The small ball sheet: the big sheet's raster, the same tile count
        // laid out another way, a taller sheet, and an 8-bit depth.
        (SMALL_BALL_IMAGE, image(SMALL_BALL_IMAGE, (32, 64), 4, 0)),
        (SMALL_BALL_IMAGE, image(SMALL_BALL_IMAGE, (8, 192), 4, 0)),
        (SMALL_BALL_IMAGE, image(SMALL_BALL_IMAGE, (16, 104), 4, 0)),
        (
            SMALL_BALL_IMAGE,
            image(SMALL_BALL_IMAGE, (16, 96), INDEXED_8_BIT, 0),
        ),
        // Wrong palette colour counts: a bank for the background, or the
        // background's banks for a sprite.
        (BG_PALETTE, palette(BG_PALETTE, 16, 0)),
        (BG_PALETTE, palette(BG_PALETTE, 175, 0)),
        (BALL_PALETTE, palette(BALL_PALETTE, 176, 0)),
        (HELD_PALETTE, palette(HELD_PALETTE, 32, 0)),
        // Maps a byte off.
        (BG_MAP, raw(BG_MAP, 2047, 0)),
        (BG_MAP, raw(BG_MAP, 2049, 0)),
        (CONFIRM, raw(CONFIRM, 27, 0)),
        (CANCEL, raw(CANCEL, 29, 0)),
        // The wide map is not the main map's size and neither is read as entries.
        (SLOTS[0].0, raw(SLOTS[0].0, 54, 0)),
        (SLOTS[2].0, raw(SLOTS[2].0, 70, 0)),
        (SLOTS[4].0, raw(SLOTS[4].0, 108, 0)),
    ];
    for (id, _) in SLOTS {
        cases.push((id, raw(id, 0, 0)));
    }
    for (n, (id, entry)) in cases.into_iter().enumerate() {
        let (pack, path) = load(&format!("party-menu-malformed-{n}"), replaced(entry));
        assert_malformed(&pack.party_menu().unwrap_err(), id, &format!("case {n}"));
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn a_pixel_outside_one_bank_is_rejected_even_over_the_wide_background_palette() {
    // The background owns 176 colours, but a 4bpp tile still selects within one
    // 16-colour bank; the screen entry picks the bank.
    for pixel in [16, 31, 175, 255] {
        for (id, size) in [
            (BG_IMAGE, (8, 496)),
            (BALL_IMAGE, (32, 64)),
            (SMALL_BALL_IMAGE, (16, 96)),
            (HELD_IMAGE, (8, 16)),
        ] {
            let (pack, path) = load(
                "party-menu-pixel",
                replaced(image(id, size, TEXT_WINDOW_BIT_DEPTH, pixel)),
            );
            assert_malformed(
                &pack.party_menu().unwrap_err(),
                id,
                &format!("pixel {pixel} in `{id}`"),
            );
            let _ = std::fs::remove_file(path);
        }
    }
    let (pack, path) = load(
        "party-menu-pixel-15",
        replaced(image(BG_IMAGE, (8, 496), TEXT_WINDOW_BIT_DEPTH, 15)),
    );
    pack.party_menu().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn individual_readers_need_only_their_own_records() {
    // A pack with only the buttons still serves the button readers.
    let entries: Vec<Entry> = fixture_entries()
        .into_iter()
        .filter(|e| e.id == CONFIRM || e.id == CANCEL)
        .collect();
    let (pack, path) = load("party-menu-partial", entries);
    assert_eq!(
        pack.party_menu_button_tilemap(PartyMenuButton::Confirm)
            .unwrap()
            .len(),
        28
    );
    assert!(matches!(
        pack.party_menu_background().unwrap_err(),
        PackError::UnknownAsset(id) if id == BG_IMAGE
    ));
    let _ = std::fs::remove_file(path);
}

#[test]
fn the_small_ball_sheet_needs_no_palette_of_its_own() {
    // It draws with the Poké Ball palette (`TAG_POKEBALL`), so the pack holds
    // no palette record for it and its reader needs no palette at all.
    let entries: Vec<Entry> = fixture_entries()
        .into_iter()
        .filter(|e| e.id == SMALL_BALL_IMAGE)
        .collect();
    let (pack, path) = load("party-menu-small-ball-only", entries);
    let small = pack.party_menu_poke_ball_small().unwrap();
    assert_eq!((small.width, small.height), (16, 96));
    assert!(matches!(
        pack.party_menu_poke_ball().unwrap_err(),
        PackError::UnknownAsset(id) if id == BALL_IMAGE
    ));
    let _ = std::fs::remove_file(path);
}
