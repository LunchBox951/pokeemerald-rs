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
const EXPECTED_CHECKPOINT_COUNT: usize = 11;
const EXPECTED_CARRIED_COUNT: usize = EXPECTED_STARTER_COUNT + EXPECTED_CHECKPOINT_COUNT;
const EXPECTED_PAIR_COUNT: usize =
    EXPECTED_CARRIED_COUNT * EXPECTED_DIRECTION_COUNT * EXPECTED_VARIANT_COUNT;
const PALETTE_ENTRY_BYTES: usize = std::mem::size_of::<u16>();
const SHORT_PALETTE_COLORS: u16 = 2;
const WRONG_KIND_PAYLOAD_BYTES: usize = 4;

const STARTERS: [(SpeciesId, &str); EXPECTED_STARTER_COUNT] = [
    (SpeciesId::TREECKO, "treecko"),
    (SpeciesId::TORCHIC, "torchic"),
    (SpeciesId::MUDKIP, "mudkip"),
];
const CHECKPOINT_POKEMON: [(SpeciesId, &str); EXPECTED_CHECKPOINT_COUNT] = [
    (SpeciesId::POOCHYENA, "poochyena"),
    (SpeciesId::MIGHTYENA, "mightyena"),
    (SpeciesId::WURMPLE, "wurmple"),
    (SpeciesId::SILCOON, "silcoon"),
    (SpeciesId::BEAUTIFLY, "beautifly"),
    (SpeciesId::CASCOON, "cascoon"),
    (SpeciesId::DUSTOX, "dustox"),
    (SpeciesId::ZIGZAGOON, "zigzagoon"),
    (SpeciesId::LINOONE, "linoone"),
    (SpeciesId::WINGULL, "wingull"),
    (SpeciesId::PELIPPER, "pelipper"),
];
/// Every species the pack carries battler art for, in a fixed order whose
/// index marks that species' pixels and first colours.
fn carried() -> Vec<(SpeciesId, &'static str)> {
    STARTERS.into_iter().chain(CHECKPOINT_POKEMON).collect()
}
const DIRECTIONS: [(BattlerDirection, &str, u8); EXPECTED_DIRECTION_COUNT] = [
    (BattlerDirection::Front, "front", 0),
    (BattlerDirection::Back, "back", 1),
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

/// The palette index filling the rows below Poochyena's front picture.
const SHEET_FILL: u8 = 15;
/// Rows of the stored image: Poochyena's front is a 64x256 sheet.
const SHEET_HEIGHT: u32 = 256;

fn stored_height(id: &str) -> u32 {
    if id == "pokemon/poochyena/front" {
        SHEET_HEIGHT
    } else {
        64
    }
}

/// An image whose first pixel names its species and second its direction;
/// a sheet's rows below the picture are [`SHEET_FILL`].
fn battler_image(id: String, species: usize, direction: u8) -> Entry {
    let height = stored_height(&id);
    let mut entry = image(
        id,
        (64, height),
        EXPECTED_BATTLER_BIT_DEPTH,
        species_marker(species),
    );
    entry.payload[1] = direction;
    entry.payload[64 * 64..].fill(SHEET_FILL);
    entry
}

/// A pixel value unique to each species, below the palette's 16 colours.
fn species_marker(species: usize) -> u8 {
    u8::try_from(species).unwrap()
}

fn species_variant_first_color(species_index: usize, variant_color_base: u16) -> u16 {
    variant_color_base + u16::try_from(species_index).unwrap() + 1
}

fn battler_entries() -> Vec<Entry> {
    let mut entries = Vec::new();
    for (species_index, (_, name)) in carried().iter().enumerate() {
        for (_, direction, direction_code) in DIRECTIONS {
            entries.push(battler_image(
                format!("pokemon/{name}/{direction}"),
                species_index,
                direction_code,
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
fn every_carried_species_direction_and_variant_resolves_its_own_pair() {
    let (pack, path) = load("battler-all", battler_entries());

    let mut resolved = 0;
    for (species_index, (id, _)) in carried().iter().enumerate() {
        for (direction, _, direction_code) in DIRECTIONS {
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
                assert_eq!(
                    art.image.pixels.len(),
                    (EXPECTED_BATTLER_SIDE * EXPECTED_BATTLER_SIDE) as usize,
                    "{pair}"
                );
                assert_eq!(art.image.bit_depth, EXPECTED_BATTLER_BIT_DEPTH, "{pair}");
                assert_eq!(art.image.pixels[0], species_marker(species_index), "{pair}");
                assert_eq!(art.image.pixels[1], direction_code, "{pair}");
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
        "every carried species/view/palette combination must resolve"
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn the_eleven_checkpoint_species_each_resolve_four_distinct_variants() {
    let (pack, path) = load("battler-checkpoint", battler_entries());

    let mut resolved = 0;
    for (id, name) in CHECKPOINT_POKEMON {
        let mut seen = Vec::new();
        for (direction, _, _) in DIRECTIONS {
            for (variant, _, _) in VARIANTS {
                let art = pack.species_battler(id, direction, variant).unwrap().art;
                seen.push((art.image.pixels[1], art.palette.color(0)));
                resolved += 1;
            }
        }
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), 4, "{name}: variants must not collapse");
    }
    assert_eq!(resolved, 44);

    let _ = std::fs::remove_file(path);
}

#[test]
fn poochyenas_front_sheet_is_stored_whole_and_read_as_its_top_picture() {
    let (pack, path) = load("battler-sheet", battler_entries());

    let stored = pack.image("pokemon/poochyena/front").unwrap();
    assert_eq!((stored.width, stored.height), (64, SHEET_HEIGHT));
    assert_eq!(stored.pixels.len(), 64 * 256);
    assert!(stored.pixels[64 * 64..].iter().all(|&p| p == SHEET_FILL));

    for variant in [PaletteVariant::Normal, PaletteVariant::Shiny] {
        let art = pack
            .species_battler(SpeciesId::POOCHYENA, BattlerDirection::Front, variant)
            .unwrap()
            .art;
        assert_eq!((art.image.width, art.image.height), (64, 64));
        assert_eq!(art.image.bit_depth, 4);
        assert_eq!(art.image.pixels, &stored.pixels[..64 * 64]);
    }
    let back = pack
        .species_battler(
            SpeciesId::POOCHYENA,
            BattlerDirection::Back,
            PaletteVariant::Normal,
        )
        .unwrap()
        .art;
    assert_eq!((back.image.width, back.image.height), (64, 64));

    let _ = std::fs::remove_file(path);
}

#[test]
fn only_poochyenas_front_may_be_a_sheet_and_only_a_256_row_one() {
    let image_of_height =
        |id: &str, height: u32| image(id.into(), (64, height), EXPECTED_BATTLER_BIT_DEPTH, 1);
    // Poochyena's front at any height but 256 is refused, expecting 64x256.
    for (n, (width, height)) in [(64, 64), (64, 128), (64, 192), (64, 320), (32, 256)]
        .into_iter()
        .enumerate()
    {
        let entry = image(
            "pokemon/poochyena/front".into(),
            (width, height),
            EXPECTED_BATTLER_BIT_DEPTH,
            1,
        );
        let (pack, path) = load(
            &format!("battler-sheet-height-{n}"),
            replaced("pokemon/poochyena/front", entry),
        );
        let err = pack
            .species_battler(
                SpeciesId::POOCHYENA,
                BattlerDirection::Front,
                PaletteVariant::Normal,
            )
            .unwrap_err();
        assert!(
            matches!(
                &err,
                PackError::MalformedBattlerAsset { detail, .. }
                    if detail.ends_with("expected 64x256")
            ),
            "{width}x{height}: {err}"
        );
        let _ = std::fs::remove_file(path);
    }
    // Every other image is a 64x64 picture, so a 256-row sheet is refused.
    for (species, name) in carried() {
        for (direction, stem, _) in DIRECTIONS {
            if species == SpeciesId::POOCHYENA && stem == "front" {
                continue;
            }
            let id = format!("pokemon/{name}/{stem}");
            let (pack, path) = load(
                &format!("battler-tall-{name}-{stem}"),
                replaced(&id, image_of_height(&id, SHEET_HEIGHT)),
            );
            let err = pack
                .species_battler(species, direction, PaletteVariant::Normal)
                .unwrap_err();
            assert!(
                matches!(
                    &err,
                    PackError::MalformedBattlerAsset { detail, .. }
                        if detail.ends_with("expected 64x64")
                ),
                "{id}: {err}"
            );
            let _ = std::fs::remove_file(path);
        }
    }
}

#[test]
fn a_bad_pixel_below_the_picture_is_refused_before_cropping() {
    for (n, bad_at) in [64 * 64, 64 * 256 - 1].into_iter().enumerate() {
        let mut entry = battler_image("pokemon/poochyena/front".into(), 3, 0);
        entry.payload[bad_at] = 16;
        let (pack, path) = load(
            &format!("battler-sheet-tail-{n}"),
            replaced("pokemon/poochyena/front", entry),
        );
        assert!(matches!(
            pack.species_battler(
                SpeciesId::POOCHYENA,
                BattlerDirection::Front,
                PaletteVariant::Normal
            )
            .unwrap_err(),
            PackError::MalformedBattlerAsset { id, .. } if id == "pokemon/poochyena/front"
        ));
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn a_species_without_battler_art_is_unsupported() {
    let (pack, path) = load("battler-unsupported", battler_entries());
    // The neighbours on both sides of the checkpoint interval stay out of scope.
    for species in [
        SpeciesId::GROVYLE,
        SpeciesId::SWAMPERT,
        SpeciesId::LOTAD,
        SpeciesId::SEEDOT,
        SpeciesId::NINCADA,
        SpeciesId::TAILLOW,
        SpeciesId::SPINDA,
        SpeciesId::SURSKIT,
        SpeciesId(0),
        SpeciesId(u16::MAX),
    ] {
        assert_eq!(
            pack.species_battler(species, BattlerDirection::Front, PaletteVariant::Normal)
                .unwrap_err(),
            PackError::UnsupportedBattlerSpecies(species),
            "unsupported species {species:?} must not substitute starter art"
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
    let _ = std::fs::remove_file(path);

    let wrong_kind = Entry {
        id: "pokemon/treecko/back",
        kind_tag: RAW_KIND_TAG,
        meta: vec![],
        payload: vec![0; WRONG_KIND_PAYLOAD_BYTES],
    };
    let (pack, path) = load(
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
    let _ = std::fs::remove_file(path);
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
        (
            "pixel outside the 16-colour palette in a checkpoint front image",
            SpeciesId::PELIPPER,
            "pokemon/pelipper/front",
            BattlerDirection::Front,
            image(
                "pokemon/pelipper/front".into(),
                (EXPECTED_BATTLER_SIDE, EXPECTED_BATTLER_SIDE),
                EXPECTED_BATTLER_BIT_DEPTH,
                u8::try_from(EXPECTED_BATTLER_PALETTE_COLORS).unwrap(),
            ),
        ),
        (
            "15-colour checkpoint shiny palette",
            SpeciesId::WURMPLE,
            "pokemon/wurmple/palette/shiny",
            BattlerDirection::Back,
            palette(
                "pokemon/wurmple/palette/shiny".into(),
                EXPECTED_BATTLER_PALETTE_COLORS - 1,
                0,
            ),
        ),
    ];
    for (n, (case, species, id, direction, entry)) in cases.into_iter().enumerate() {
        let (pack, path) = load(&format!("battler-malformed-{n}"), replaced(id, entry));
        assert!(
            matches!(
                pack.species_battler(species, direction, PaletteVariant::Shiny).unwrap_err(),
                PackError::MalformedBattlerAsset { id: bad, .. } if bad == id
            ),
            "wrong error for {case} ({species:?}/{direction:?}, `{id}`)"
        );
        let _ = std::fs::remove_file(path);
    }
}
