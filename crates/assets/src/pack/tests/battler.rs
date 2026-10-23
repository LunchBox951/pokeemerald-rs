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
const CHECKPOINT_POKEMON: [(SpeciesId, &str); 11] = [
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
const DIRECTIONS: [(BattlerDirection, &str, u8); 2] = [
    (BattlerDirection::Front, "front", 0),
    (BattlerDirection::Back, "back", 1),
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
        TEXT_WINDOW_BIT_DEPTH,
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

/// A first colour unique to each species and variant.
fn first_color(species: usize, variant: u16) -> u16 {
    variant + u16::try_from(species).unwrap() + 1
}

fn battler_entries() -> Vec<Entry> {
    let mut entries = Vec::new();
    for (species, (_, name)) in carried().iter().enumerate() {
        for (_, direction, code) in DIRECTIONS {
            entries.push(battler_image(
                format!("pokemon/{name}/{direction}"),
                species,
                code,
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
fn every_carried_species_direction_and_variant_resolves_its_own_pair() {
    let (pack, path) = load("battler-all", battler_entries());

    let mut resolved = 0;
    for (species, (id, _)) in carried().iter().enumerate() {
        for (direction, _, code) in DIRECTIONS {
            for (variant, _, base) in VARIANTS {
                let handle = pack.species_battler(*id, direction, variant).unwrap();
                assert_eq!(handle.species, *id);
                assert_eq!(handle.direction, direction);
                assert_eq!(handle.variant, variant);
                let art = handle.art;
                assert_eq!((art.image.width, art.image.height), (64, 64));
                assert_eq!(art.image.pixels.len(), 64 * 64);
                assert_eq!(art.image.bit_depth, 4);
                assert_eq!(art.image.pixels[0], species_marker(species));
                assert_eq!(art.image.pixels[1], code);
                assert_eq!(art.palette.color_count, 16);
                assert_eq!(art.palette.color(0), Some(first_color(species, base)));
                resolved += 1;
            }
        }
    }
    assert_eq!(resolved, 14 * 2 * 2);

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
        |id: &str, height: u32| image(id.into(), (64, height), TEXT_WINDOW_BIT_DEPTH, 1);
    // Poochyena's front at any height but 256 is refused, expecting 64x256.
    for (n, (width, height)) in [(64, 64), (64, 128), (64, 192), (64, 320), (32, 256)]
        .into_iter()
        .enumerate()
    {
        let entry = image(
            "pokemon/poochyena/front".into(),
            (width, height),
            TEXT_WINDOW_BIT_DEPTH,
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
        (
            "pokemon/pelipper/front",
            BattlerDirection::Front,
            image(
                "pokemon/pelipper/front".into(),
                (64, 64),
                TEXT_WINDOW_BIT_DEPTH,
                16,
            ),
        ),
        (
            "pokemon/wurmple/palette/shiny",
            BattlerDirection::Back,
            palette("pokemon/wurmple/palette/shiny".into(), 15, 0),
        ),
    ];
    for (n, (id, direction, entry)) in cases.into_iter().enumerate() {
        let (pack, path) = load(&format!("battler-malformed-{n}"), replaced(id, entry));
        let species = carried()
            .into_iter()
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
