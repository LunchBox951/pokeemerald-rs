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

/// Icon stems as the C table pairs them, read back from the public lookup's
/// source of truth: the first rows and the shared-sheet and shared-palette
/// cases the tests pin by name.
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
    // ITEM_TIMER_BALL reuses the Repeat Ball palette with its own sheet.
    assert_ne!(stems[10].0, stems[9].0);
    assert_eq!(stems[10].1, stems[9].1);
    // Status heals share a sheet under different palettes.
    assert_eq!(stems[15].0, stems[16].0);
    assert_ne!(stems[15].1, stems[16].1);
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
