use super::super::{AssetPack, BattlerDirection, PackError, PaletteVariant};
use super::shared::{
    image_meta, pack_bytes, write_pack, Entry, IMAGE_KIND_TAG, INDEXED_8_BIT, PALETTE_KIND_TAG,
    RAW_KIND_TAG,
};
use crate::SpeciesId;

const EXPECTED_BATTLER_SIDE: u32 = 64;
const EXPECTED_BATTLER_BIT_DEPTH: u8 = 4;
const EXPECTED_BATTLER_PALETTE_COLORS: u16 = 16;
const EXPECTED_STARTER_COUNT: usize = 3;
const EXPECTED_DIRECTION_COUNT: usize = 2;
const EXPECTED_VARIANT_COUNT: usize = 2;
const EXPECTED_PAIR_COUNT: usize =
    EXPECTED_STARTER_COUNT * EXPECTED_DIRECTION_COUNT * EXPECTED_VARIANT_COUNT;
const PALETTE_ENTRY_BYTES: usize = std::mem::size_of::<u16>();
const SHORT_PALETTE_COLORS: u16 = 2;
const WRONG_KIND_PAYLOAD_BYTES: usize = 4;

const STARTERS: [(SpeciesId, &str); EXPECTED_STARTER_COUNT] = [
    (SpeciesId::TREECKO, "treecko"),
    (SpeciesId::TORCHIC, "torchic"),
    (SpeciesId::MUDKIP, "mudkip"),
];
const DIRECTIONS: [(BattlerDirection, &str, u8); EXPECTED_DIRECTION_COUNT] = [
    (BattlerDirection::Front, "front", 0),
    (BattlerDirection::Back, "back", 8),
];
const VARIANTS: [(PaletteVariant, &str, u16); EXPECTED_VARIANT_COUNT] = [
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

fn palette(id: String, colors: u16, first_color: u16) -> Entry {
    let mut payload = vec![0u8; usize::from(colors) * PALETTE_ENTRY_BYTES];
    payload[..PALETTE_ENTRY_BYTES].copy_from_slice(&first_color.to_le_bytes());
    Entry {
        id: Box::leak(id.into_boxed_str()),
        kind_tag: PALETTE_KIND_TAG,
        meta: colors.to_le_bytes().to_vec(),
        payload,
    }
}

fn species_direction_pixel(species_index: usize, direction_pixel_offset: u8) -> u8 {
    direction_pixel_offset + u8::try_from(species_index).unwrap() + 1
}

fn species_variant_first_color(species_index: usize, variant_color_base: u16) -> u16 {
    variant_color_base + u16::try_from(species_index).unwrap() + 1
}

fn battler_entries() -> Vec<Entry> {
    let mut entries = Vec::new();
    for (species_index, (_, name)) in STARTERS.iter().enumerate() {
        for (_, direction, direction_pixel_offset) in DIRECTIONS {
            entries.push(image(
                format!("pokemon/{name}/{direction}"),
                (EXPECTED_BATTLER_SIDE, EXPECTED_BATTLER_SIDE),
                EXPECTED_BATTLER_BIT_DEPTH,
                species_direction_pixel(species_index, direction_pixel_offset),
            ));
        }
        for (_, variant, variant_color_base) in VARIANTS {
            entries.push(palette(
                format!("pokemon/{name}/palette/{variant}"),
                EXPECTED_BATTLER_PALETTE_COLORS,
                species_variant_first_color(species_index, variant_color_base),
            ));
        }
    }
    entries
}

fn load(name: &str, entries: Vec<Entry>) -> AssetPack {
    let path = write_pack(name, &pack_bytes(entries));
    AssetPack::load(&path).unwrap()
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
    let pack = load("battler-all", battler_entries());

    let mut resolved = 0;
    for (species_index, (id, _)) in STARTERS.iter().enumerate() {
        for (direction, _, direction_pixel_offset) in DIRECTIONS {
            for (variant, _, variant_color_base) in VARIANTS {
                let pair = format!("requested {id:?}/{direction:?}/{variant:?}");
                let handle = pack
                    .species_battler(*id, direction, variant)
                    .unwrap_or_else(|err| panic!("{pair} must resolve: {err:?}"));
                assert_eq!(handle.species, *id, "{pair}");
                assert_eq!(handle.direction, direction, "{pair}");
                assert_eq!(handle.variant, variant, "{pair}");
                let art = handle.art;
                assert_eq!(
                    (art.image.width, art.image.height),
                    (EXPECTED_BATTLER_SIDE, EXPECTED_BATTLER_SIDE),
                    "{pair}"
                );
                assert_eq!(art.image.bit_depth, EXPECTED_BATTLER_BIT_DEPTH, "{pair}");
                assert_eq!(
                    art.image.pixels[0],
                    species_direction_pixel(species_index, direction_pixel_offset),
                    "{pair}"
                );
                assert_eq!(
                    art.palette.color_count, EXPECTED_BATTLER_PALETTE_COLORS,
                    "{pair}"
                );
                assert_eq!(
                    art.palette.color(0),
                    Some(species_variant_first_color(
                        species_index,
                        variant_color_base
                    )),
                    "{pair}"
                );
                resolved += 1;
            }
        }
    }
    assert_eq!(
        resolved, EXPECTED_PAIR_COUNT,
        "every starter/view/palette combination must resolve"
    );
}

#[test]
fn a_species_without_battler_art_is_unsupported() {
    let pack = load("battler-unsupported", battler_entries());
    for species in [SpeciesId::GROVYLE, SpeciesId(0), SpeciesId(u16::MAX)] {
        assert_eq!(
            pack.species_battler(species, BattlerDirection::Front, PaletteVariant::Normal)
                .unwrap_err(),
            PackError::UnsupportedBattlerSpecies(species),
            "unsupported species {species:?} must not substitute starter art"
        );
    }
}

#[test]
fn a_missing_or_wrong_kind_entry_fails_without_substitution() {
    let entries: Vec<Entry> = battler_entries()
        .into_iter()
        .filter(|e| e.id != "pokemon/torchic/back" && e.id != "pokemon/mudkip/palette/shiny")
        .collect();
    let pack = load("battler-missing", entries);
    assert!(
        pack.species_battler(
            SpeciesId::TORCHIC,
            BattlerDirection::Front,
            PaletteVariant::Shiny
        )
        .is_ok(),
        "unrelated missing entries must not block Torchic's front/shiny pair"
    );
    assert!(
        matches!(
            pack.species_battler(SpeciesId::TORCHIC, BattlerDirection::Back, PaletteVariant::Normal)
                .unwrap_err(),
            PackError::UnknownAsset(id) if id == "pokemon/torchic/back"
        ),
        "missing back art must not substitute front art"
    );
    assert!(
        matches!(
            pack.species_battler(SpeciesId::MUDKIP, BattlerDirection::Front, PaletteVariant::Shiny)
                .unwrap_err(),
            PackError::UnknownAsset(id) if id == "pokemon/mudkip/palette/shiny"
        ),
        "missing shiny palette must not substitute the normal palette"
    );

    let wrong_kind = Entry {
        id: "pokemon/treecko/back",
        kind_tag: RAW_KIND_TAG,
        meta: vec![],
        payload: vec![0; WRONG_KIND_PAYLOAD_BYTES],
    };
    let pack = load(
        "battler-wrong-kind",
        replaced("pokemon/treecko/back", wrong_kind),
    );
    assert!(
        matches!(
            pack.species_battler(SpeciesId::TREECKO, BattlerDirection::Back, PaletteVariant::Normal)
                .unwrap_err(),
            PackError::WrongKind { id, expected: "image", .. } if id == "pokemon/treecko/back"
        ),
        "raw back-art entry must report WrongKind expecting image"
    );
}

#[test]
fn malformed_entries_are_rejected_on_read() {
    let cases = [
        (
            "half-size front image",
            SpeciesId::TREECKO,
            "pokemon/treecko/front",
            BattlerDirection::Front,
            image(
                "pokemon/treecko/front".into(),
                (EXPECTED_BATTLER_SIDE / 2, EXPECTED_BATTLER_SIDE / 2),
                EXPECTED_BATTLER_BIT_DEPTH,
                1,
            ),
        ),
        (
            "8-bit back image",
            SpeciesId::TORCHIC,
            "pokemon/torchic/back",
            BattlerDirection::Back,
            image(
                "pokemon/torchic/back".into(),
                (EXPECTED_BATTLER_SIDE, EXPECTED_BATTLER_SIDE),
                INDEXED_8_BIT,
                1,
            ),
        ),
        (
            "pixel outside the 16-colour palette",
            SpeciesId::MUDKIP,
            "pokemon/mudkip/back",
            BattlerDirection::Back,
            image(
                "pokemon/mudkip/back".into(),
                (EXPECTED_BATTLER_SIDE, EXPECTED_BATTLER_SIDE),
                EXPECTED_BATTLER_BIT_DEPTH,
                u8::try_from(EXPECTED_BATTLER_PALETTE_COLORS).unwrap(),
            ),
        ),
        (
            "two-colour shiny palette",
            SpeciesId::MUDKIP,
            "pokemon/mudkip/palette/shiny",
            BattlerDirection::Front,
            palette(
                "pokemon/mudkip/palette/shiny".into(),
                SHORT_PALETTE_COLORS,
                0,
            ),
        ),
    ];
    for (n, (case, species, id, direction, entry)) in cases.into_iter().enumerate() {
        let pack = load(&format!("battler-malformed-{n}"), replaced(id, entry));
        assert!(
            matches!(
                pack.species_battler(species, direction, PaletteVariant::Shiny).unwrap_err(),
                PackError::MalformedBattlerAsset { id: bad, .. } if bad == id
            ),
            "wrong error for {case} ({species:?}/{direction:?}, `{id}`)"
        );
    }
}
