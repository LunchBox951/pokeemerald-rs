use super::super::{AssetPack, BattlerDirection, PackError, PaletteVariant};
use super::shared::{
    image_meta, pack_bytes, write_pack, Entry, IMAGE_KIND_TAG, INDEXED_8_BIT, PALETTE_KIND_TAG,
    RAW_KIND_TAG, TEXT_WINDOW_BIT_DEPTH,
};
use crate::SpeciesId;

const STARTERS: [(SpeciesId, &str); 3] = [
    (SpeciesId::TREECKO, "treecko"),
    (SpeciesId::TORCHIC, "torchic"),
    (SpeciesId::MUDKIP, "mudkip"),
];
const DIRECTIONS: [(BattlerDirection, &str, u8); 2] = [
    (BattlerDirection::Front, "front", 0),
    (BattlerDirection::Back, "back", 8),
];
const VARIANTS: [(PaletteVariant, &str, u16); 2] = [
    (PaletteVariant::Normal, "normal", 0x0100),
    (PaletteVariant::Shiny, "shiny", 0x0200),
];

fn image(id: String, side: (u32, u32), depth: u8, pixel: u8) -> Entry {
    Entry {
        id: Box::leak(id.into_boxed_str()),
        kind_tag: IMAGE_KIND_TAG,
        meta: image_meta(side.0, side.1, depth),
        payload: vec![pixel; (side.0 * side.1) as usize],
    }
}

fn palette(id: String, colors: u16, first: u16) -> Entry {
    let mut payload = vec![0u8; usize::from(colors) * 2];
    payload[..2].copy_from_slice(&first.to_le_bytes());
    Entry {
        id: Box::leak(id.into_boxed_str()),
        kind_tag: PALETTE_KIND_TAG,
        meta: colors.to_le_bytes().to_vec(),
        payload,
    }
}

/// A pixel value unique to each species and direction.
fn pixel(species: usize, direction: u8) -> u8 {
    direction + u8::try_from(species).unwrap() + 1
}

/// A first colour unique to each species and variant.
fn first_color(species: usize, variant: u16) -> u16 {
    variant + u16::try_from(species).unwrap() + 1
}

fn battler_entries() -> Vec<Entry> {
    let mut entries = Vec::new();
    for (species, (_, name)) in STARTERS.iter().enumerate() {
        for (_, direction, offset) in DIRECTIONS {
            entries.push(image(
                format!("pokemon/{name}/{direction}"),
                (64, 64),
                TEXT_WINDOW_BIT_DEPTH,
                pixel(species, offset),
            ));
        }
        for (_, variant, base) in VARIANTS {
            entries.push(palette(
                format!("pokemon/{name}/palette/{variant}"),
                16,
                first_color(species, base),
            ));
        }
    }
    entries
}

fn load(name: &str, entries: Vec<Entry>) -> (AssetPack, std::path::PathBuf) {
    let path = write_pack(name, &pack_bytes(entries));
    (AssetPack::load(&path).unwrap(), path)
}

fn replaced(id: &str, entry: Entry) -> Vec<Entry> {
    let mut entries: Vec<Entry> = battler_entries()
        .into_iter()
        .filter(|e| e.id != id)
        .collect();
    entries.push(entry);
    entries
}

#[test]
fn every_starter_direction_and_variant_resolves_its_own_pair() {
    let (pack, path) = load("battler-all", battler_entries());

    let mut resolved = 0;
    for (species, (id, _)) in STARTERS.iter().enumerate() {
        for (direction, _, offset) in DIRECTIONS {
            for (variant, _, base) in VARIANTS {
                let handle = pack.species_battler(*id, direction, variant).unwrap();
                assert_eq!(handle.species, *id);
                assert_eq!(handle.direction, direction);
                assert_eq!(handle.variant, variant);
                let art = handle.art;
                assert_eq!((art.image.width, art.image.height), (64, 64));
                assert_eq!(art.image.bit_depth, 4);
                assert_eq!(art.image.pixels[0], pixel(species, offset));
                assert_eq!(art.palette.color_count, 16);
                assert_eq!(art.palette.color(0), Some(first_color(species, base)));
                resolved += 1;
            }
        }
    }
    assert_eq!(resolved, 12);

    let _ = std::fs::remove_file(path);
}

#[test]
fn a_species_without_battler_art_is_unsupported() {
    let (pack, path) = load("battler-unsupported", battler_entries());
    for species in [SpeciesId::GROVYLE, SpeciesId(0), SpeciesId(u16::MAX)] {
        assert_eq!(
            pack.species_battler(species, BattlerDirection::Front, PaletteVariant::Normal)
                .unwrap_err(),
            PackError::UnsupportedBattlerSpecies(species)
        );
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_missing_or_wrong_kind_entry_fails_without_substitution() {
    let entries: Vec<Entry> = battler_entries()
        .into_iter()
        .filter(|e| e.id != "pokemon/torchic/back" && e.id != "pokemon/mudkip/palette/shiny")
        .collect();
    let (pack, path) = load("battler-missing", entries);
    assert!(pack
        .species_battler(
            SpeciesId::TORCHIC,
            BattlerDirection::Front,
            PaletteVariant::Shiny
        )
        .is_ok());
    assert!(matches!(
        pack.species_battler(SpeciesId::TORCHIC, BattlerDirection::Back, PaletteVariant::Normal)
            .unwrap_err(),
        PackError::UnknownAsset(id) if id == "pokemon/torchic/back"
    ));
    assert!(matches!(
        pack.species_battler(SpeciesId::MUDKIP, BattlerDirection::Front, PaletteVariant::Shiny)
            .unwrap_err(),
        PackError::UnknownAsset(id) if id == "pokemon/mudkip/palette/shiny"
    ));
    let _ = std::fs::remove_file(path);

    let wrong_kind = Entry {
        id: "pokemon/treecko/back",
        kind_tag: RAW_KIND_TAG,
        meta: vec![],
        payload: vec![0; 4],
    };
    let (pack, path) = load(
        "battler-wrong-kind",
        replaced("pokemon/treecko/back", wrong_kind),
    );
    assert!(matches!(
        pack.species_battler(SpeciesId::TREECKO, BattlerDirection::Back, PaletteVariant::Normal)
            .unwrap_err(),
        PackError::WrongKind { id, expected: "image", .. } if id == "pokemon/treecko/back"
    ));
    let _ = std::fs::remove_file(path);
}

#[test]
fn malformed_entries_are_rejected_on_read() {
    let cases = [
        (
            "pokemon/treecko/front",
            BattlerDirection::Front,
            image(
                "pokemon/treecko/front".into(),
                (32, 32),
                TEXT_WINDOW_BIT_DEPTH,
                1,
            ),
        ),
        (
            "pokemon/torchic/back",
            BattlerDirection::Back,
            image("pokemon/torchic/back".into(), (64, 64), INDEXED_8_BIT, 1),
        ),
        (
            "pokemon/mudkip/back",
            BattlerDirection::Back,
            image(
                "pokemon/mudkip/back".into(),
                (64, 64),
                TEXT_WINDOW_BIT_DEPTH,
                16,
            ),
        ),
        (
            "pokemon/mudkip/palette/shiny",
            BattlerDirection::Front,
            palette("pokemon/mudkip/palette/shiny".into(), 2, 0),
        ),
    ];
    for (n, (id, direction, entry)) in cases.into_iter().enumerate() {
        let (pack, path) = load(&format!("battler-malformed-{n}"), replaced(id, entry));
        let species = STARTERS
            .iter()
            .find(|(_, name)| id.contains(name))
            .unwrap()
            .0;
        assert!(
            matches!(
                pack.species_battler(species, direction, PaletteVariant::Shiny).unwrap_err(),
                PackError::MalformedBattlerAsset { id: bad, .. } if bad == id
            ),
            "wrong error for malformed `{id}`"
        );
        let _ = std::fs::remove_file(path);
    }
}
