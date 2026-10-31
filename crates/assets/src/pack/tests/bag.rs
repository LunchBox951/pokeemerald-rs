use std::collections::BTreeSet;

use super::super::super::items::{ItemId, ITEMS_COUNT};
use super::super::{AssetPack, PackError};
use super::shared::{
    image_meta, pack_bytes, write_pack, Entry, IMAGE_KIND_TAG, INDEXED_8_BIT, PALETTE_KIND_TAG,
    RAW_KIND_TAG, TEXT_WINDOW_BIT_DEPTH,
};

const TILEMAP_BYTES: usize = 2048;
const END: u16 = 377;
const MENU: &str = "bag/image/menu";
const ICON_IMAGE: &str = "bag/item-icon/image/";
const ICON_PALETTE: &str = "bag/item-icon/palette/";

fn image(id: &'static str, side: (u32, u32), pixel: u8) -> Entry {
    Entry {
        id,
        kind_tag: IMAGE_KIND_TAG,
        meta: image_meta(side.0, side.1, TEXT_WINDOW_BIT_DEPTH),
        payload: vec![pixel; (side.0 * side.1) as usize],
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

/// A 128x128 sheet whose pixel at (x, y) is `((x / 8) ^ (y / 8)) & 15`, so a read
/// at the wrong tile shows up in the value.
fn menu_info_sheet() -> Entry {
    let mut sheet = image("bag/image/menu_info", (128, 128), 0);
    for (n, pixel) in sheet.payload.iter_mut().enumerate() {
        let (x, y) = (n % 128, n / 128);
        *pixel = u8::try_from(((x / 8) ^ (y / 8)) & 15).unwrap();
    }
    sheet
}

fn raw(id: &'static str, len: usize, fill: u8) -> Entry {
    Entry {
        id,
        kind_tag: RAW_KIND_TAG,
        meta: vec![],
        payload: vec![fill; len],
    }
}

fn leaked(prefix: &str, stem: &str) -> &'static str {
    Box::leak(format!("{prefix}{stem}").into_boxed_str())
}

fn icon_entries() -> Vec<Entry> {
    let stems = super::super::bag::icon_stems();
    let images: BTreeSet<&str> = stems.iter().map(|&(image, _)| image).collect();
    let palettes: BTreeSet<&str> = stems.iter().map(|&(_, palette)| palette).collect();
    let mut entries = Vec::new();
    for (n, stem) in images.into_iter().enumerate() {
        entries.push(image(
            leaked(ICON_IMAGE, stem),
            (24, 24),
            u8::try_from(n % 16).unwrap(),
        ));
    }
    for (n, stem) in palettes.into_iter().enumerate() {
        entries.push(palette(
            leaked(ICON_PALETTE, stem),
            16,
            u16::try_from(n + 1).unwrap(),
        ));
    }
    entries
}

fn bag_entries() -> Vec<Entry> {
    let mut entries = vec![
        image(MENU, (128, 32), 15),
        palette("bag/palette/menu/male", 32, 0x0101),
        palette("bag/palette/menu/female", 32, 0x0102),
        raw("bag/raw/menu", TILEMAP_BYTES, 0xB1),
        image("bag/image/male", (64, 384), 1),
        image("bag/image/female", (64, 384), 2),
        palette("bag/palette/bag", 16, 0x0201),
        image("bag/image/rotating_ball", (16, 16), 3),
        palette("bag/palette/rotating_ball", 16, 0x0301),
        image("bag/image/select_button", (24, 16), 4),
        image("bag/image/hm", (16, 16), 5),
        image("bag/image/scroll_indicator", (16, 32), 6),
        palette("bag/palette/scroll_indicator", 16, 0x0401),
        image("bag/image/swap_line", (16, 32), 7),
        palette("bag/palette/swap_line", 16, 0x0501),
        menu_info_sheet(),
        palette("bag/palette/menu_info1", 16, 0x0601),
        palette("bag/palette/menu_info2", 16, 0x0602),
        palette("bag/palette/menu_info3", 16, 0x0603),
    ];
    entries.extend(icon_entries());
    entries
}

fn load(name: &str, entries: Vec<Entry>) -> (AssetPack, std::path::PathBuf) {
    let path = write_pack(name, &pack_bytes(entries));
    (AssetPack::load(&path).unwrap(), path)
}

fn replaced(id: &str, entry: Entry) -> Vec<Entry> {
    let mut entries: Vec<Entry> = bag_entries().into_iter().filter(|e| e.id != id).collect();
    entries.push(entry);
    entries
}

fn without(id: &str) -> Vec<Entry> {
    bag_entries().into_iter().filter(|e| e.id != id).collect()
}

#[test]
fn bag_assets_bundle_every_fixed_graphic() {
    let (pack, path) = load("bag-fixed", bag_entries());

    let bag = pack.bag_assets().unwrap();
    assert_eq!((bag.screen.width, bag.screen.height), (128, 32));
    assert_eq!(bag.screen_tilemap, vec![0xB1; TILEMAP_BYTES].as_slice());
    assert_eq!(bag.screen_male_palette.color(0), Some(0x0101));
    assert_eq!(bag.screen_female_palette.color(0), Some(0x0102));
    assert_eq!(bag.screen_male_palette.color_count, 32);
    assert_eq!((bag.male.image.width, bag.male.image.height), (64, 384));
    assert_eq!(bag.male.image.pixels[0], 1);
    assert_eq!(bag.female.image.pixels[0], 2);
    assert_eq!(bag.male.palette.color(0), Some(0x0201));
    assert_eq!(bag.female.palette.color(0), Some(0x0201));
    assert_eq!(bag.rotating_ball.image.pixels[0], 3);
    assert_eq!(bag.rotating_ball.palette.color(0), Some(0x0301));
    assert_eq!(
        (bag.registered_select.width, bag.registered_select.height),
        (24, 16)
    );
    assert_eq!((bag.hm.width, bag.hm.height), (16, 16));

    let _ = std::fs::remove_file(path);
}

#[test]
fn scroll_indicator_and_swap_line_return_their_own_sheet_and_palette() {
    let (pack, path) = load("bag-scroll-swap", bag_entries());
    let bag = pack.bag_assets().unwrap();

    let arrows = bag.scroll_indicator().unwrap();
    assert_eq!((arrows.image.width, arrows.image.height), (16, 32));
    assert_eq!(arrows.image.pixels[0], 6);
    assert_eq!(arrows.palette.color_count, 16);
    assert_eq!(arrows.palette.color(0), Some(0x0401));

    let line = bag.swap_line().unwrap();
    assert_eq!((line.image.width, line.image.height), (16, 32));
    assert_eq!(line.image.pixels[0], 7);
    assert_eq!(line.palette.color_count, 16);
    assert_eq!(line.palette.color(0), Some(0x0501));

    let _ = std::fs::remove_file(path);
}

#[test]
fn missing_or_malformed_scroll_and_swap_entries_are_rejected_on_lookup() {
    let scroll = true;
    let swap = false;
    let cases: [(&str, bool, Option<Entry>); 8] = [
        ("bag/image/scroll_indicator", scroll, None),
        ("bag/palette/scroll_indicator", scroll, None),
        ("bag/image/swap_line", swap, None),
        ("bag/palette/swap_line", swap, None),
        (
            "bag/image/scroll_indicator",
            scroll,
            Some(image("bag/image/scroll_indicator", (16, 16), 0)),
        ),
        (
            "bag/palette/scroll_indicator",
            scroll,
            Some(palette("bag/palette/scroll_indicator", 15, 0)),
        ),
        (
            "bag/image/swap_line",
            swap,
            Some(image("bag/image/swap_line", (16, 32), 16)),
        ),
        (
            "bag/palette/swap_line",
            swap,
            Some(palette("bag/palette/swap_line", 17, 0)),
        ),
    ];
    for (n, (id, is_scroll, entry)) in cases.into_iter().enumerate() {
        let entries = match entry {
            Some(entry) => replaced(id, entry),
            None => without(id),
        };
        let (pack, path) = load(&format!("bag-scroll-swap-bad-{n}"), entries);
        // Unrelated fixed graphics still open; only the lookup refuses.
        let bag = pack.bag_assets().unwrap();
        let err = if is_scroll {
            bag.scroll_indicator().unwrap_err()
        } else {
            bag.swap_line().unwrap_err()
        };
        assert!(
            matches!(
                &err,
                PackError::UnknownAsset(bad) | PackError::MalformedBagAsset { id: bad, .. }
                    if bad == id
            ),
            "case {n} `{id}`: {err:?}"
        );
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn every_identity_resolves_its_own_icon_pair() {
    let (pack, path) = load("bag-icons", bag_entries());
    let bag = pack.bag_assets().unwrap();
    let stems = super::super::bag::icon_stems();
    assert_eq!(stems.len(), ITEMS_COUNT + 1);

    for id in 0..u16::try_from(ITEMS_COUNT).unwrap() {
        let icon = bag.item_icon(ItemId(id)).unwrap();
        let (image_stem, palette_stem) = stems[usize::from(id)];
        let expected_image = pack
            .image(&format!("{ICON_IMAGE}{image_stem}"))
            .unwrap()
            .pixels;
        let expected_palette = pack
            .palette(&format!("{ICON_PALETTE}{palette_stem}"))
            .unwrap()
            .color(0);
        assert_eq!(icon.image.pixels, expected_image, "{id}");
        assert_eq!(icon.palette.color(0), expected_palette, "{id}");
        assert_eq!((icon.image.width, icon.image.height), (24, 24));
        assert_eq!(icon.palette.color_count, 16);
    }

    let _ = std::fs::remove_file(path);
}

#[test]
fn shared_images_and_palettes_pair_independently() {
    let stems = super::super::bag::icon_stems();
    let (timer_ball, repeat_ball) = (stems[10], stems[9]);
    assert_ne!(timer_ball.0, repeat_ball.0, "timer ball has its own sheet");
    assert_eq!(
        timer_ball.1, repeat_ball.1,
        "timer ball reuses the repeat ball palette"
    );
    let (burn_heal, ice_heal) = (stems[15], stems[16]);
    assert_eq!(burn_heal.0, ice_heal.0, "status heals share a sheet");
    assert_ne!(
        burn_heal.1, ice_heal.1,
        "status heals have distinct palettes"
    );
}

#[test]
fn reserved_list_end_and_out_of_range_rows_follow_upstream() {
    let (pack, path) = load("bag-sentinels", bag_entries());
    let bag = pack.bag_assets().unwrap();
    let stems = super::super::bag::icon_stems();
    assert_eq!(stems[0], ("question_mark", "question_mark"));
    assert_eq!(stems[52], ("question_mark", "question_mark"));
    assert_eq!(
        stems[ITEMS_COUNT],
        ("return_to_field_arrow", "return_to_field_arrow")
    );

    let none = bag.item_icon(ItemId::NONE).unwrap();
    let arrow = bag.list_end_icon().unwrap();
    let arrow_pixels = pack
        .image("bag/item-icon/image/return_to_field_arrow")
        .unwrap()
        .pixels;
    assert_eq!(arrow.image.pixels, arrow_pixels);
    assert_eq!(
        bag.item_icon(ItemId(u16::MAX)).unwrap().image.pixels,
        arrow.image.pixels
    );
    for out_of_range in [END, 1000, u16::MAX - 1] {
        let icon = bag.item_icon(ItemId(out_of_range)).unwrap();
        assert_eq!(icon.image.pixels, none.image.pixels);
        assert_eq!(icon.palette.color(0), none.palette.color(0));
    }

    let _ = std::fs::remove_file(path);
}

#[test]
fn missing_or_wrong_kind_entries_fail_without_substitution() {
    for id in [MENU, "bag/palette/bag", "bag/raw/menu", "bag/image/hm"] {
        let (pack, path) = load("bag-missing", without(id));
        assert!(matches!(
            pack.bag_assets().unwrap_err(),
            PackError::UnknownAsset(missing) if missing == id
        ));
        let _ = std::fs::remove_file(path);
    }

    let (pack, path) = load(
        "bag-wrong-kind",
        replaced("bag/raw/menu", image("bag/raw/menu", (2, 2), 0)),
    );
    assert!(matches!(
        pack.bag_assets().unwrap_err(),
        PackError::WrongKind { id, expected: "raw blob", .. } if id == "bag/raw/menu"
    ));
    let _ = std::fs::remove_file(path);

    let (pack, path) = load("bag-icon-missing", without("bag/item-icon/image/potion"));
    let bag = pack.bag_assets().unwrap();
    assert!(matches!(
        bag.item_icon(ItemId(13)).unwrap_err(),
        PackError::UnknownAsset(id) if id == "bag/item-icon/image/potion"
    ));
    assert!(bag.item_icon(ItemId(14)).is_ok());
    let _ = std::fs::remove_file(path);
}

#[test]
fn malformed_fixed_entries_are_rejected_on_read() {
    let cases = [
        (MENU, image(MENU, (64, 32), 0)),
        (MENU, image(MENU, (128, 32), 16)),
        (
            "bag/image/male",
            Entry {
                id: "bag/image/male",
                kind_tag: IMAGE_KIND_TAG,
                meta: image_meta(64, 384, INDEXED_8_BIT),
                payload: vec![0; 64 * 384],
            },
        ),
        (
            "bag/palette/menu/male",
            palette("bag/palette/menu/male", 16, 0),
        ),
        ("bag/palette/bag", palette("bag/palette/bag", 17, 0)),
        (
            "bag/palette/rotating_ball",
            palette("bag/palette/rotating_ball", 15, 0),
        ),
        ("bag/raw/menu", raw("bag/raw/menu", TILEMAP_BYTES - 2, 0)),
    ];
    for (n, (id, entry)) in cases.into_iter().enumerate() {
        let (pack, path) = load(&format!("bag-malformed-{n}"), replaced(id, entry));
        assert!(
            matches!(
                pack.bag_assets().unwrap_err(),
                PackError::MalformedBagAsset { id: bad, .. } if bad == id
            ),
            "wrong error for malformed `{id}`"
        );
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn malformed_icon_entries_are_rejected_on_lookup() {
    let image_id = "bag/item-icon/image/potion";
    let palette_id = "bag/item-icon/palette/potion";
    let cases = [
        (image_id, image(image_id, (32, 32), 0)),
        (palette_id, palette(palette_id, 15, 0)),
    ];
    for (n, (id, entry)) in cases.into_iter().enumerate() {
        let (pack, path) = load(&format!("bag-icon-malformed-{n}"), replaced(id, entry));
        let bag = pack.bag_assets().unwrap();
        assert!(
            matches!(
                bag.item_icon(ItemId(13)).unwrap_err(),
                PackError::MalformedBagAsset { id: bad, .. } if bad == id
            ),
            "wrong error for malformed `{id}`"
        );
        let _ = std::fs::remove_file(path);
    }
}

fn bag_fault(
    entries: Vec<Entry>,
    name: &str,
    lookup: impl Fn(&AssetPack) -> PackError,
) -> (String, String) {
    let (pack, path) = load(name, entries);
    let error = lookup(&pack);
    let _ = std::fs::remove_file(path);
    match error {
        PackError::MalformedBagAsset { id, detail } => (id, detail),
        other => panic!("expected MalformedBagAsset, got {other:?}"),
    }
}

#[test]
fn paired_sheet_faults_follow_the_shared_validation_order_and_wording() {
    // An out-of-range pixel names the sheet in the shared wording.
    let (id, detail) = bag_fault(
        replaced("bag/image/male", image("bag/image/male", (64, 384), 16)),
        "bag-shared-pixel",
        |pack| pack.bag_assets().unwrap_err(),
    );
    assert_eq!(id, "bag/image/male");
    assert_eq!(
        detail,
        "has pixel index 16: outside its palette's 16 colours or 4-bit range"
    );

    // The palette is checked before pixel indices: a sheet with a bad pixel
    // paired with a malformed palette reports the palette.
    let mut entries = replaced("bag/image/male", image("bag/image/male", (64, 384), 16));
    entries.retain(|e| e.id != "bag/palette/bag");
    entries.push(palette("bag/palette/bag", 17, 0));
    let (id, detail) = bag_fault(entries, "bag-shared-order", |pack| {
        pack.bag_assets().unwrap_err()
    });
    assert_eq!(id, "bag/palette/bag");
    assert_eq!(
        detail,
        "declares 17 colours in 34 bytes: expected 16 colours"
    );

    // A sheet at the wrong depth reports the shared bit-depth wording.
    let (id, detail) = bag_fault(
        replaced(
            "bag/image/male",
            Entry {
                id: "bag/image/male",
                kind_tag: IMAGE_KIND_TAG,
                meta: image_meta(64, 384, INDEXED_8_BIT),
                payload: vec![0; 64 * 384],
            },
        ),
        "bag-shared-depth",
        |pack| pack.bag_assets().unwrap_err(),
    );
    assert_eq!(id, "bag/image/male");
    assert_eq!(detail, "is 8-bit: expected 4-bit");

    // Item icons share the validator.
    let icon = "bag/item-icon/image/potion";
    let (id, detail) = bag_fault(
        replaced(icon, image(icon, (24, 24), 16)),
        "bag-shared-icon",
        |pack| {
            pack.bag_assets()
                .unwrap()
                .item_icon(ItemId(13))
                .unwrap_err()
        },
    );
    assert_eq!(id, icon);
    assert_eq!(
        detail,
        "has pixel index 16: outside its palette's 16 colours or 4-bit range"
    );
}

/// `sMenuInfoIcons` (`src/menu.c:113-141`) in pixels: tile offset `t` is at
/// `(t % 16 * 8, t / 16 * 8)` in the 16-tile-wide sheet.
const TYPE_ICON_RECTS: [(u32, u32); 18] = [
    (0, 16),  // NORMAL   0x20
    (32, 48), // FIGHTING 0x64
    (0, 48),  // FLYING   0x60
    (0, 64),  // POISON   0x80
    (64, 32), // GROUND   0x48
    (32, 32), // ROCK     0x44
    (96, 48), // BUG      0x6C
    (64, 48), // GHOST    0x68
    (64, 64), // STEEL    0x88
    (32, 80), // MYSTERY  0xA4
    (32, 16), // FIRE     0x24
    (64, 16), // WATER    0x28
    (96, 16), // GRASS    0x2C
    (0, 32),  // ELECTRIC 0x40
    (32, 64), // PSYCHIC  0x84
    (96, 32), // ICE      0x4C
    (0, 80),  // DRAGON   0xA0
    (96, 64), // DARK     0x8C
];

#[test]
fn menu_info_yields_each_type_and_label_icon_at_its_upstream_offset() {
    use super::super::{MenuInfoIcon, MenuInfoLabel};

    let (pack, path) = load("bag-menu-info", bag_entries());
    let info = pack.bag_assets().unwrap().menu_info().unwrap();
    assert_eq!((info.sheet.width, info.sheet.height), (128, 128));
    for (n, color) in [0x0601, 0x0602, 0x0603].into_iter().enumerate() {
        assert_eq!(info.palettes[n].color_count, 16);
        assert_eq!(info.palettes[n].color(0), Some(color));
    }

    for (type_id, (x, y)) in TYPE_ICON_RECTS.into_iter().enumerate() {
        let icon = info.type_icon(u8::try_from(type_id).unwrap()).unwrap();
        assert_eq!(
            icon,
            MenuInfoIcon {
                x,
                y,
                width: 32,
                height: 12
            },
            "type {type_id}"
        );
    }
    assert_eq!(info.type_icon(18), None);
    assert_eq!(info.type_icon(u8::MAX), None);

    let labels = [
        (MenuInfoLabel::Type, (64, 80)),
        (MenuInfoLabel::Power, (0, 96)),
        (MenuInfoLabel::Accuracy, (64, 96)),
        (MenuInfoLabel::Pp, (0, 112)),
    ];
    assert_eq!(MenuInfoLabel::ALL, labels.map(|(label, _)| label));
    for (label, (x, y)) in labels {
        let icon = info.label_icon(label);
        assert_eq!(
            icon,
            MenuInfoIcon {
                x,
                y,
                width: 42,
                height: 12
            },
            "{label:?}"
        );
        // Rows come from the sheet at the icon's own position.
        for row in 0..12 {
            let pixels = info.icon_row(icon, row).unwrap();
            assert_eq!(pixels.len(), 42);
            let tile = |dx: u32, dy: u32| (((x + dx) / 8) ^ ((y + dy) / 8)) & 15;
            assert_eq!(u32::from(pixels[0]), tile(0, row), "{label:?} row {row}");
            assert_eq!(u32::from(pixels[41]), tile(41, row), "{label:?} row {row}");
        }
        assert_eq!(info.icon_row(icon, 12), None);
    }

    // A rectangle that leaves the sheet reads nothing.
    let outside = MenuInfoIcon {
        x: 100,
        y: 120,
        width: 42,
        height: 12,
    };
    assert_eq!(info.icon_row(outside, 0), None);
    // An icon that runs off the bottom reads nothing on any row, including
    // the rows that would still land inside the sheet.
    let below = MenuInfoIcon {
        x: 0,
        y: 125,
        width: 8,
        height: 12,
    };
    for row in [0, 2, 4, 11] {
        assert_eq!(info.icon_row(below, row), None, "row {row}");
    }
    // Rectangles at the edge of `u32` neither panic nor wrap onto the sheet.
    for icon in [
        MenuInfoIcon {
            x: 0,
            y: u32::MAX,
            width: 1,
            height: 2,
        },
        MenuInfoIcon {
            x: u32::MAX,
            y: 0,
            width: 2,
            height: 1,
        },
    ] {
        assert_eq!(info.icon_row(icon, 0), None);
        assert_eq!(info.icon_row(icon, 1), None);
    }
    // The last row of the last icon on the sheet still reads.
    let last = MenuInfoIcon {
        x: 120,
        y: 127,
        width: 8,
        height: 1,
    };
    assert_eq!(info.icon_row(last, 0).map(<[u8]>::len), Some(8));

    let _ = std::fs::remove_file(path);
}

#[test]
fn missing_or_malformed_menu_info_entries_are_rejected_on_lookup() {
    let cases: [(&str, Option<Entry>); 9] = [
        ("bag/image/menu_info", None),
        ("bag/palette/menu_info1", None),
        ("bag/palette/menu_info3", None),
        (
            "bag/image/menu_info",
            Some(image("bag/image/menu_info", (128, 64), 0)),
        ),
        (
            "bag/image/menu_info",
            Some({
                let mut sheet = menu_info_sheet();
                sheet.payload[0] = 16;
                sheet
            }),
        ),
        (
            "bag/image/menu_info",
            Some({
                let mut sheet = menu_info_sheet();
                sheet.meta = image_meta(128, 128, INDEXED_8_BIT);
                sheet
            }),
        ),
        (
            "bag/palette/menu_info1",
            Some(palette("bag/palette/menu_info1", 15, 0)),
        ),
        (
            "bag/palette/menu_info2",
            Some(palette("bag/palette/menu_info2", 32, 0)),
        ),
        (
            "bag/palette/menu_info3",
            Some(image("bag/palette/menu_info3", (16, 16), 0)),
        ),
    ];
    for (n, (id, entry)) in cases.into_iter().enumerate() {
        let entries = match entry {
            Some(entry) => replaced(id, entry),
            None => without(id),
        };
        let (pack, path) = load(&format!("bag-menu-info-bad-{n}"), entries);
        // Unrelated fixed graphics still open; only the lookup refuses.
        let err = pack.bag_assets().unwrap().menu_info().unwrap_err();
        assert!(
            matches!(
                &err,
                PackError::UnknownAsset(bad)
                    | PackError::WrongKind { id: bad, .. }
                    | PackError::MalformedBagAsset { id: bad, .. }
                    if bad == id
            ),
            "case {n} `{id}`: {err:?}"
        );
        let _ = std::fs::remove_file(path);
    }
}
