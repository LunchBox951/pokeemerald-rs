use super::super::{AssetPack, PackError};
use super::shared::{
    image_meta, pack_bytes, write_pack, Entry, IMAGE_KIND_TAG, PALETTE_KIND_TAG, RAW_KIND_TAG,
    TEXT_WINDOW_BIT_DEPTH,
};

const MAP_BYTES: usize = 1280;

fn image(id: &'static str, (width, height): (u32, u32), depth: u8, pixel: u8) -> Entry {
    Entry {
        id,
        kind_tag: IMAGE_KIND_TAG,
        meta: image_meta(width, height, depth),
        payload: vec![pixel; (width * height) as usize],
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

fn scene_entries() -> Vec<Entry> {
    let depth = TEXT_WINDOW_BIT_DEPTH;
    vec![
        image("opening-scene/image/shadow", (128, 24), depth, 3),
        image("opening-scene/image/birch", (64, 64), depth, 4),
        image("opening-scene/image/lotad", (64, 64), depth, 5),
        image("opening-scene/image/wall_clock", (128, 64), depth, 6),
        image("opening-scene/image/clock_hand", (64, 144), depth, 15),
        palette("opening-scene/palette/bg0", 16, 0x0101),
        palette("opening-scene/palette/bg1", 16, 0x0102),
        palette("opening-scene/palette/bg2", 16, 0x0103),
        palette("opening-scene/palette/birch", 16, 0x0104),
        palette("opening-scene/palette/lotad", 16, 0x0105),
        palette("opening-scene/palette/clock_male", 16, 0x0106),
        palette("opening-scene/palette/clock_female", 16, 0x0107),
        palette("opening-scene/palette/clock_text_prompt", 4, 0x0108),
        raw("opening-scene/raw/birch_speech_map", MAP_BYTES, 0xB1),
        raw("opening-scene/raw/clock_start", MAP_BYTES, 0xC1),
        raw("opening-scene/raw/clock_view", MAP_BYTES, 0xC2),
    ]
}

fn load(name: &str, entries: Vec<Entry>) -> (AssetPack, std::path::PathBuf) {
    let path = write_pack(name, &pack_bytes(entries));
    (AssetPack::load(&path).unwrap(), path)
}

fn replaced(id: &str, entry: Entry) -> Vec<Entry> {
    let mut entries = without(id);
    entries.push(entry);
    entries
}

fn without(id: &str) -> Vec<Entry> {
    scene_entries()
        .into_iter()
        .filter(|entry| entry.id != id)
        .collect()
}

#[test]
fn a_complete_pack_bundles_the_birch_speech_scene() {
    let (pack, path) = load("opening-scene-birch", scene_entries());
    let scene = pack.birch_speech().unwrap();
    assert_eq!((scene.shadow.width, scene.shadow.height), (128, 24));
    assert_eq!(scene.shadow.bit_depth, 4);
    assert_eq!(scene.background_palettes[0].color_count, 16);
    assert_eq!(scene.background_palettes[0].colors().next(), Some(0x0101));
    assert_eq!(scene.background_palettes[1].colors().next(), Some(0x0102));
    assert_eq!(scene.gradient_palette.colors().next(), Some(0x0103));
    assert_eq!(scene.background_tilemap, vec![0xB1; MAP_BYTES]);
    assert_eq!(
        (scene.birch.image.width, scene.birch.image.height),
        (64, 64)
    );
    assert_eq!(scene.birch.palette.colors().next(), Some(0x0104));
    assert_eq!(
        (scene.lotad.image.width, scene.lotad.image.height),
        (64, 64)
    );
    assert_eq!(scene.lotad.palette.colors().next(), Some(0x0105));
    assert_ne!(scene.birch.image.pixels, scene.lotad.image.pixels);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn a_complete_pack_bundles_the_wall_clock() {
    let (pack, path) = load("opening-scene-clock", scene_entries());
    let clock = pack.wall_clock().unwrap();
    assert_eq!((clock.screen.width, clock.screen.height), (128, 64));
    assert_eq!((clock.hand.width, clock.hand.height), (64, 144));
    assert_eq!(clock.hand.bit_depth, 4);
    assert!(clock.hand.pixels.iter().all(|&pixel| pixel == 15));
    assert_eq!(clock.male_palette.colors().next(), Some(0x0106));
    assert_eq!(clock.female_palette.colors().next(), Some(0x0107));
    assert_eq!(clock.text_prompt_palette.color_count, 4);
    assert_eq!(clock.start_tilemap, vec![0xC1; MAP_BYTES]);
    assert_eq!(clock.view_tilemap, vec![0xC2; MAP_BYTES]);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn a_missing_entry_is_refused_by_id() {
    for entry in scene_entries() {
        let (pack, path) = load("opening-scene-missing", without(entry.id));
        let birch = pack.birch_speech().err();
        let clock = pack.wall_clock().err();
        let refused = [birch, clock]
            .into_iter()
            .flatten()
            .any(|err| matches!(err, PackError::UnknownAsset(id) if id == entry.id));
        assert!(refused, "{}", entry.id);
        std::fs::remove_file(path).unwrap();
    }
}

#[test]
fn entries_of_the_wrong_shape_are_refused() {
    let depth = TEXT_WINDOW_BIT_DEPTH;
    let cases: Vec<(&str, Entry)> = vec![
        (
            "opening-scene/image/shadow",
            image("opening-scene/image/shadow", (128, 16), depth, 3),
        ),
        (
            "opening-scene/image/birch",
            image("opening-scene/image/birch", (64, 64), 8, 4),
        ),
        (
            "opening-scene/image/lotad",
            image("opening-scene/image/lotad", (64, 64), depth, 16),
        ),
        (
            "opening-scene/palette/bg1",
            palette("opening-scene/palette/bg1", 15, 1),
        ),
        (
            "opening-scene/palette/bg2",
            palette("opening-scene/palette/bg2", 8, 1),
        ),
        (
            "opening-scene/raw/birch_speech_map",
            raw("opening-scene/raw/birch_speech_map", MAP_BYTES - 2, 0),
        ),
    ];
    for (id, entry) in cases {
        let (pack, path) = load("opening-scene-birch-bad", replaced(id, entry));
        let err = pack.birch_speech().unwrap_err();
        assert!(
            matches!(&err, PackError::MalformedOpeningSceneAsset { id: bad, .. } if bad == id),
            "{id}: {err:?}"
        );
        std::fs::remove_file(path).unwrap();
    }

    let clock_cases: Vec<(&str, Entry)> = vec![
        (
            "opening-scene/image/wall_clock",
            image("opening-scene/image/wall_clock", (64, 64), depth, 1),
        ),
        (
            "opening-scene/image/clock_hand",
            image("opening-scene/image/clock_hand", (64, 144), 8, 1),
        ),
        (
            "opening-scene/palette/clock_female",
            palette("opening-scene/palette/clock_female", 32, 1),
        ),
        (
            "opening-scene/palette/clock_text_prompt",
            palette("opening-scene/palette/clock_text_prompt", 16, 1),
        ),
        (
            "opening-scene/raw/clock_view",
            raw("opening-scene/raw/clock_view", MAP_BYTES + 2, 0),
        ),
    ];
    for (id, entry) in clock_cases {
        let (pack, path) = load("opening-scene-clock-bad", replaced(id, entry));
        let err = pack.wall_clock().unwrap_err();
        assert!(
            matches!(&err, PackError::MalformedOpeningSceneAsset { id: bad, .. } if bad == id),
            "{id}: {err:?}"
        );
        std::fs::remove_file(path).unwrap();
    }
}

#[test]
fn a_wrong_kind_entry_is_refused() {
    let (pack, path) = load(
        "opening-scene-wrong-kind",
        replaced(
            "opening-scene/raw/clock_start",
            palette("opening-scene/raw/clock_start", 16, 1),
        ),
    );
    assert!(matches!(
        pack.wall_clock(),
        Err(PackError::WrongKind { .. })
    ));
    std::fs::remove_file(path).unwrap();
}
