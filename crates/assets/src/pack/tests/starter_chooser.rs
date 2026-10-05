use super::super::{AssetPack, PackError, StarterSpecies};
use super::shared::{
    image_meta, pack_bytes, write_pack, Entry, IMAGE_KIND_TAG, INDEXED_8_BIT, PALETTE_KIND_TAG,
    RAW_KIND_TAG, TEXT_WINDOW_BIT_DEPTH,
};

const BAG_BYTES: usize = 1280;
const GRASS_BYTES: usize = 2048;

fn image(id: &'static str, side: (u32, u32), depth: u8, pixel: u8) -> Entry {
    Entry {
        id,
        kind_tag: IMAGE_KIND_TAG,
        meta: image_meta(side.0, side.1, depth),
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

fn starter_entries() -> Vec<Entry> {
    vec![
        image("starter-chooser/image/tiles", (128, 128), INDEXED_8_BIT, 31),
        palette("starter-chooser/palette/tiles", 32, 0x0101),
        image(
            "starter-chooser/image/pokeball_selection",
            (32, 128),
            TEXT_WINDOW_BIT_DEPTH,
            15,
        ),
        palette("starter-chooser/palette/pokeball_selection", 16, 0x0202),
        image(
            "starter-chooser/image/starter_circle",
            (64, 64),
            TEXT_WINDOW_BIT_DEPTH,
            1,
        ),
        palette("starter-chooser/palette/starter_circle", 16, 0x0303),
        raw("starter-chooser/raw/birch_bag", BAG_BYTES, 0xB1),
        raw("starter-chooser/raw/birch_grass", GRASS_BYTES, 0xB2),
        image("pokemon/treecko/front", (64, 64), TEXT_WINDOW_BIT_DEPTH, 2),
        palette("pokemon/treecko/palette/normal", 16, 0x0401),
        image("pokemon/torchic/front", (64, 64), TEXT_WINDOW_BIT_DEPTH, 3),
        palette("pokemon/torchic/palette/normal", 16, 0x0402),
        image("pokemon/mudkip/front", (64, 64), TEXT_WINDOW_BIT_DEPTH, 4),
        palette("pokemon/mudkip/palette/normal", 16, 0x0403),
    ]
}

fn load(name: &str, entries: Vec<Entry>) -> (AssetPack, std::path::PathBuf) {
    let path = write_pack(name, &pack_bytes(entries));
    (AssetPack::load(&path).unwrap(), path)
}

fn replaced(id: &str, entry: Entry) -> Vec<Entry> {
    let mut entries: Vec<Entry> = starter_entries()
        .into_iter()
        .filter(|e| e.id != id)
        .collect();
    entries.push(entry);
    entries
}

#[test]
fn starter_chooser_bundles_every_graphic_and_tilemap() {
    let (pack, path) = load("starter-chooser", starter_entries());

    let chooser = pack.starter_chooser().unwrap();
    assert_eq!(chooser.tiles.image.width, 128);
    assert_eq!(chooser.tiles.image.bit_depth, 8);
    assert_eq!(chooser.tiles.palette.color_count, 32);
    assert_eq!(chooser.tiles.palette.color(0), Some(0x0101));
    assert_eq!(
        (
            chooser.pokeball_selection.image.width,
            chooser.pokeball_selection.image.height
        ),
        (32, 128)
    );
    assert_eq!(chooser.pokeball_selection.palette.color(0), Some(0x0202));
    assert_eq!(chooser.starter_circle.image.pixels[0], 1);
    assert_eq!(chooser.starter_circle.palette.color(0), Some(0x0303));
    assert_eq!(chooser.bag_tilemap, vec![0xB1; BAG_BYTES].as_slice());
    assert_eq!(chooser.grass_tilemap, vec![0xB2; GRASS_BYTES].as_slice());

    let _ = std::fs::remove_file(path);
}

#[test]
fn starter_preview_returns_each_species_own_front_pair() {
    let (pack, path) = load("starter-preview", starter_entries());

    for (starter, pixel, first) in [
        (StarterSpecies::Treecko, 2, 0x0401),
        (StarterSpecies::Torchic, 3, 0x0402),
        (StarterSpecies::Mudkip, 4, 0x0403),
    ] {
        let front = pack.starter_preview(starter).unwrap().front;
        assert_eq!((front.image.width, front.image.height), (64, 64));
        assert_eq!(front.image.pixels[0], pixel);
        assert_eq!(front.palette.color_count, 16);
        assert_eq!(front.palette.color(0), Some(first));
    }

    let _ = std::fs::remove_file(path);
}

#[test]
fn missing_or_wrong_kind_chooser_entries_fail_without_substitution() {
    let entries: Vec<Entry> = starter_entries()
        .into_iter()
        .filter(|e| e.id != "pokemon/torchic/front")
        .collect();
    let (pack, path) = load("starter-missing", entries);
    assert!(pack.starter_preview(StarterSpecies::Treecko).is_ok());
    assert!(matches!(
        pack.starter_preview(StarterSpecies::Torchic).unwrap_err(),
        PackError::UnknownAsset(id) if id == "pokemon/torchic/front"
    ));
    let _ = std::fs::remove_file(path);

    let (pack, path) = load(
        "starter-wrong-kind",
        replaced(
            "starter-chooser/raw/birch_bag",
            image("starter-chooser/raw/birch_bag", (2, 2), INDEXED_8_BIT, 0),
        ),
    );
    assert!(matches!(
        pack.starter_chooser().unwrap_err(),
        PackError::WrongKind { id, expected: "raw blob", .. } if id == "starter-chooser/raw/birch_bag"
    ));
    let _ = std::fs::remove_file(path);
}

#[test]
fn malformed_chooser_entries_are_rejected_on_read() {
    let cases = [
        (
            "starter-chooser/image/starter_circle",
            image(
                "starter-chooser/image/starter_circle",
                (32, 32),
                TEXT_WINDOW_BIT_DEPTH,
                1,
            ),
        ),
        (
            "starter-chooser/palette/tiles",
            palette("starter-chooser/palette/tiles", 16, 0),
        ),
        (
            "starter-chooser/image/pokeball_selection",
            image(
                "starter-chooser/image/pokeball_selection",
                (32, 128),
                TEXT_WINDOW_BIT_DEPTH,
                16,
            ),
        ),
        (
            "starter-chooser/raw/birch_grass",
            raw("starter-chooser/raw/birch_grass", GRASS_BYTES - 2, 0),
        ),
    ];
    for (n, (id, entry)) in cases.into_iter().enumerate() {
        let (pack, path) = load(&format!("starter-malformed-{n}"), replaced(id, entry));
        assert!(
            matches!(
                pack.starter_chooser().unwrap_err(),
                PackError::MalformedStarterAsset { id: bad, .. } if bad == id
            ),
            "wrong error for malformed `{id}`"
        );
        let _ = std::fs::remove_file(path);
    }

    let (pack, path) = load(
        "starter-malformed-front",
        replaced(
            "pokemon/mudkip/palette/normal",
            palette("pokemon/mudkip/palette/normal", 2, 0),
        ),
    );
    assert!(matches!(
        pack.starter_preview(StarterSpecies::Mudkip).unwrap_err(),
        PackError::MalformedStarterAsset { id, .. } if id == "pokemon/mudkip/palette/normal"
    ));
    let _ = std::fs::remove_file(path);
}
