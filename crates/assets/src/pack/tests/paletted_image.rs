//! Guard that every standalone palette reader reports through the shared
//! `AssetPack::sized_palette` check, so their diagnostics cannot drift from it.

use super::super::{AssetPack, PackError};
use super::shared::{pack_bytes, write_pack, Entry, PALETTE_KIND_TAG};

const COLORS: u16 = 16;
const PALETTE_ENTRY_BYTES: usize = 2;
const NAMING_STEM: &str = "menu";
const NAMING_ID: &str = "naming-screen/palette/menu";

/// A palette entry declaring `declared` colours over `bytes` payload bytes.
fn palette(id: &'static str, declared: u16, bytes: usize) -> Entry {
    Entry {
        id,
        kind_tag: PALETTE_KIND_TAG,
        meta: declared.to_le_bytes().to_vec(),
        payload: vec![0; bytes],
    }
}

fn load(name: &str, entries: Vec<Entry>) -> AssetPack {
    let path = write_pack(name, &pack_bytes(entries));
    let pack = AssetPack::load(&path).unwrap();
    let _ = std::fs::remove_file(path);
    pack
}

/// The id and detail of any family's malformed-asset error.
fn fault(error: PackError) -> (&'static str, String, String) {
    match error {
        PackError::MalformedEvolutionSceneAsset { id, detail } => ("evolution", id, detail),
        PackError::MalformedOpeningSceneAsset { id, detail } => ("opening", id, detail),
        PackError::MalformedNamingScreenAsset { id, detail } => ("naming", id, detail),
        PackError::MalformedPokedexGraphic { id, detail } => ("pokedex", id, detail),
        PackError::MalformedBagAsset { id, detail } => ("bag", id, detail),
        other => panic!("not a malformed-asset error: {other:?}"),
    }
}

/// What the shared check says about `id`, under a family-neutral constructor.
fn shared_fault(pack: &AssetPack, id: &str) -> (String, String) {
    let error = pack
        .sized_palette(id, COLORS, |id, detail| PackError::MalformedBagAsset {
            id,
            detail,
        })
        .unwrap_err();
    let (_, id, detail) = fault(error);
    (id, detail)
}

#[test]
fn every_palette_reader_reports_what_the_shared_check_reports() {
    // The container only loads a palette whose bytes match its declared count,
    // so a wrong count is the malformed shape a reader can see.
    // `(family, palette id, reader)`; each reader is the family's own entry
    // point, and `COLORS` is the size it is asked to enforce.
    type Reader = fn(&AssetPack, &str) -> Result<(), PackError>;
    let readers: [(&str, &str, Reader); 5] = [
        ("evolution", "evolution-scene/palette/test", |p, id| {
            p.evolution_palette(id, COLORS).map(drop)
        }),
        ("opening", "opening-scene/palette/test", |p, id| {
            p.scene_palette(id, COLORS).map(drop)
        }),
        ("naming", NAMING_ID, |p, _| {
            p.naming_screen_palette(NAMING_STEM).map(drop)
        }),
        ("pokedex", "pokedex-graphics/palette/test", |p, id| {
            p.pokedex_palette_entry(id, COLORS).map(drop)
        }),
        ("bag", "bag/palette/test", |p, id| {
            p.bag_palette(id, COLORS).map(drop)
        }),
    ];
    let full = usize::from(COLORS) * PALETTE_ENTRY_BYTES;
    let wrong_shapes = [
        ("one colour short", COLORS - 1, full - PALETTE_ENTRY_BYTES),
        ("one colour long", COLORS + 1, full + PALETTE_ENTRY_BYTES),
    ];
    for (shape, declared, bytes) in wrong_shapes {
        let pack = load(
            "sized-palette-guard",
            readers
                .iter()
                .map(|&(_, id, _)| palette(id, declared, bytes))
                .collect(),
        );
        for (family, id, read) in readers {
            let (kind, bad_id, detail) = fault(read(&pack, id).unwrap_err());
            assert_eq!(kind, family, "{family} {shape}: error family");
            assert_eq!(
                (bad_id, detail),
                shared_fault(&pack, id),
                "{family} {shape}: diagnostic diverges from `sized_palette`"
            );
        }
    }
}
