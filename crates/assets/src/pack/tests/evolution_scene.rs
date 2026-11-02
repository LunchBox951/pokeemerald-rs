use super::super::{AssetPack, PackError};
use super::shared::{
    image_meta, pack_bytes, write_pack, Entry, IMAGE_KIND_TAG, PALETTE_KIND_TAG, RAW_KIND_TAG,
    TEXT_WINDOW_BIT_DEPTH,
};

const MAP_BYTES: usize = 32 * 32 * 2;

const BACKGROUND: &str = "evolution-scene/image/bg";
const SPARKLE: &str = "evolution-scene/image/evo_sparkle";
const INTRO_PALETTE: &str = "evolution-scene/palette/bg_anim_intro";
const ANIMATION_PALETTE: &str = "evolution-scene/palette/bg_anim";
const SPARKLE_PALETTE: &str = "evolution-scene/palette/evo_sparkle";
const INNER_MAP: &str = "evolution-scene/raw/bg_inner";
const OUTER_MAP: &str = "evolution-scene/raw/bg_outer";

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
        image(BACKGROUND, (128, 112), depth, 9),
        image(SPARKLE, (8, 8), depth, 5),
        palette(INTRO_PALETTE, 256, 0x0201),
        palette(ANIMATION_PALETTE, 32, 0x0202),
        palette(SPARKLE_PALETTE, 16, 0x0203),
        raw(INNER_MAP, MAP_BYTES, 0xA1),
        raw(OUTER_MAP, MAP_BYTES, 0xA2),
    ]
}

fn load(name: &str, entries: Vec<Entry>) -> (AssetPack, std::path::PathBuf) {
    let path = write_pack(name, &pack_bytes(entries));
    (AssetPack::load(&path).unwrap(), path)
}

fn without(id: &str) -> Vec<Entry> {
    scene_entries()
        .into_iter()
        .filter(|entry| entry.id != id)
        .collect()
}

fn replaced(id: &str, entry: Entry) -> Vec<Entry> {
    let mut entries = without(id);
    entries.push(entry);
    entries
}

#[test]
fn a_complete_pack_bundles_the_evolution_scene() {
    let (pack, path) = load("evolution-scene-complete", scene_entries());
    let scene = pack.evolution_scene().unwrap();
    assert_eq!(
        (scene.background.width, scene.background.height),
        (128, 112)
    );
    assert_eq!(scene.background.bit_depth, 4);
    assert!(scene.background.pixels.iter().all(|&pixel| pixel == 9));
    assert_eq!(scene.inner_tilemap, vec![0xA1; MAP_BYTES]);
    assert_eq!(scene.outer_tilemap, vec![0xA2; MAP_BYTES]);
    assert_eq!(scene.intro_palette.color_count, 256);
    assert_eq!(scene.intro_palette.colors().next(), Some(0x0201));
    assert_eq!(scene.animation_palette.color_count, 32);
    assert_eq!(scene.animation_palette.colors().next(), Some(0x0202));
    assert_eq!(
        (scene.sparkle.image.width, scene.sparkle.image.height),
        (8, 8)
    );
    assert_eq!(scene.sparkle.image.bit_depth, 4);
    assert_eq!(scene.sparkle.palette.color_count, 16);
    assert_eq!(scene.sparkle.palette.colors().next(), Some(0x0203));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn a_missing_constituent_is_refused_by_id() {
    for entry in scene_entries() {
        let (pack, path) = load("evolution-scene-missing", without(entry.id));
        let err = pack.evolution_scene().unwrap_err();
        assert!(
            matches!(&err, PackError::UnknownAsset(id) if id == entry.id),
            "{}: {err:?}",
            entry.id
        );
        std::fs::remove_file(path).unwrap();
    }
}

#[test]
fn a_constituent_of_the_wrong_shape_is_refused_by_id() {
    let depth = TEXT_WINDOW_BIT_DEPTH;
    let cases: Vec<(&str, Entry)> = vec![
        (BACKGROUND, image(BACKGROUND, (128, 96), depth, 9)),
        (BACKGROUND, image(BACKGROUND, (128, 112), 8, 9)),
        (BACKGROUND, image(BACKGROUND, (128, 112), depth, 16)),
        (SPARKLE, image(SPARKLE, (8, 16), depth, 5)),
        (SPARKLE, image(SPARKLE, (8, 8), 8, 5)),
        (SPARKLE, image(SPARKLE, (8, 8), depth, 16)),
        (INTRO_PALETTE, palette(INTRO_PALETTE, 16, 1)),
        (ANIMATION_PALETTE, palette(ANIMATION_PALETTE, 16, 1)),
        (SPARKLE_PALETTE, palette(SPARKLE_PALETTE, 32, 1)),
        (INNER_MAP, raw(INNER_MAP, MAP_BYTES - 2, 0)),
        (OUTER_MAP, raw(OUTER_MAP, MAP_BYTES + 2, 0)),
    ];
    for (id, entry) in cases {
        let (pack, path) = load("evolution-scene-bad", replaced(id, entry));
        let err = pack.evolution_scene().unwrap_err();
        assert!(
            matches!(&err, PackError::MalformedEvolutionSceneAsset { id: bad, .. } if bad == id),
            "{id}: {err:?}"
        );
        std::fs::remove_file(path).unwrap();
    }
}

#[test]
fn a_constituent_of_the_wrong_kind_is_refused() {
    let cases: Vec<(&str, Entry)> = vec![
        (BACKGROUND, raw(BACKGROUND, 4, 0)),
        (SPARKLE_PALETTE, raw(SPARKLE_PALETTE, 32, 0)),
        (INNER_MAP, palette(INNER_MAP, 16, 1)),
    ];
    for (id, entry) in cases {
        let (pack, path) = load("evolution-scene-wrong-kind", replaced(id, entry));
        assert!(
            matches!(pack.evolution_scene(), Err(PackError::WrongKind { .. })),
            "{id}"
        );
        std::fs::remove_file(path).unwrap();
    }
}
