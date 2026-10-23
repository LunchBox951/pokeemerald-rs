//! Evolution-scene locator tests over a synthetic ROM and pack.

use pack_format::{image_entry_from_tiles, palette_entry, raw_entry, PackEntry};
use rom_import::fixture::RomFixture;
use rom_import::Encoding;

use super::locate;
use crate::extract::scope::evolution_scene_ids;
use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::plan::{Resolution, SymbolExpectation};
use crate::gen_rom_profile::tests::{distinctive_tiles, lz77_literals, with_context};

const BACKGROUND_TILES: usize = 128 / 8 * (112 / 8);
const TILEMAP_BYTES: usize = 32 * 32 * 2;

fn palette_bytes(seed: u16, colors: u16) -> (Vec<u16>, Vec<u8>) {
    let colors: Vec<u16> = (0..colors)
        .map(|n| n.wrapping_mul(0x0421).wrapping_add(seed))
        .collect();
    let bytes = colors.iter().flat_map(|c| c.to_le_bytes()).collect();
    (colors, bytes)
}

fn tilemap(seed: u8) -> Vec<u8> {
    (0..TILEMAP_BYTES)
        .map(|n| {
            u8::try_from(n % 251)
                .unwrap()
                .wrapping_mul(7)
                .wrapping_add(seed)
        })
        .collect()
}

/// A ROM and pack holding all seven scene roots, one distinct copy each.
fn fixture() -> (Vec<u8>, Vec<PackEntry>) {
    let background = distinctive_tiles(BACKGROUND_TILES);
    let sparkle: Vec<u8> = distinctive_tiles(1).iter().map(|b| b ^ 0x33).collect();
    let (intro_colors, intro_bytes) = palette_bytes(0x1111, 256);
    let (anim_colors, anim_bytes) = palette_bytes(0x2222, 32);
    let (sparkle_colors, sparkle_bytes) = palette_bytes(0x3333, 16);
    let (inner, outer) = (tilemap(3), tilemap(9));
    let rom = RomFixture::new()
        .emerald_header()
        .write(0x10_0000, &lz77_literals(&background))
        .write(0x11_0000, &lz77_literals(&sparkle))
        .write(0x12_0000, &intro_bytes)
        .write(0x13_0000, &anim_bytes)
        .write(0x14_0000, &sparkle_bytes)
        .write(0x15_0000, &lz77_literals(&inner))
        .write(0x16_0000, &lz77_literals(&outer))
        .finish();
    let entries = vec![
        image_entry_from_tiles(
            "evolution-scene/image/bg".into(),
            &background,
            4,
            128,
            112,
            None,
        )
        .expect("background entry"),
        image_entry_from_tiles(
            "evolution-scene/image/evo_sparkle".into(),
            &sparkle,
            4,
            8,
            8,
            None,
        )
        .expect("sparkle entry"),
        palette_entry(
            "evolution-scene/palette/bg_anim_intro".into(),
            &intro_colors,
        )
        .expect("palette"),
        palette_entry("evolution-scene/palette/bg_anim".into(), &anim_colors).expect("palette"),
        palette_entry(
            "evolution-scene/palette/evo_sparkle".into(),
            &sparkle_colors,
        )
        .expect("palette"),
        raw_entry("evolution-scene/raw/bg_inner".into(), inner),
        raw_entry("evolution-scene/raw/bg_outer".into(), outer),
    ];
    (rom, entries)
}

#[test]
fn the_fixed_inventory_is_two_images_three_palettes_and_two_tilemaps() {
    let ids = evolution_scene_ids();
    let count = |prefix: &str| ids.iter().filter(|id| id.starts_with(prefix)).count();
    assert_eq!(ids.len(), 7);
    assert_eq!(count("evolution-scene/image/"), 2);
    assert_eq!(count("evolution-scene/palette/"), 3);
    assert_eq!(count("evolution-scene/raw/"), 2);
}

#[test]
fn every_scene_family_is_located_with_its_own_encoding() {
    let (rom, entries) = fixture();
    with_context("evolution-families", &rom, entries, |ctx| {
        let mut report = Vec::new();
        let plan = locate(ctx, &mut report).expect("located");
        let images: Vec<_> = plan
            .images
            .iter()
            .map(|i| {
                (
                    i.id.as_str(),
                    i.addr,
                    i.encoding,
                    i.rom_bit_depth,
                    i.pack_bit_depth,
                )
            })
            .collect();
        assert_eq!(
            images,
            [
                (
                    "evolution-scene/image/bg",
                    0x0810_0000,
                    Encoding::Lz77,
                    4,
                    4
                ),
                (
                    "evolution-scene/image/evo_sparkle",
                    0x0811_0000,
                    Encoding::Lz77,
                    4,
                    4
                ),
            ]
        );
        let palettes: Vec<_> = plan
            .palettes
            .iter()
            .map(|p| (p.id.as_str(), p.addr, p.encoding, p.color_count))
            .collect();
        assert_eq!(
            palettes,
            [
                (
                    "evolution-scene/palette/bg_anim",
                    0x0813_0000,
                    Encoding::Raw,
                    32
                ),
                (
                    "evolution-scene/palette/bg_anim_intro",
                    0x0812_0000,
                    Encoding::Raw,
                    256
                ),
                (
                    "evolution-scene/palette/evo_sparkle",
                    0x0814_0000,
                    Encoding::Raw,
                    16
                ),
            ]
        );
        let tilemaps: Vec<_> = plan
            .tilemaps
            .iter()
            .map(|t| (t.id.as_str(), t.addr, t.encoding, t.len))
            .collect();
        assert_eq!(
            tilemaps,
            [
                (
                    "evolution-scene/raw/bg_inner",
                    0x0815_0000,
                    Encoding::Lz77,
                    2048
                ),
                (
                    "evolution-scene/raw/bg_outer",
                    0x0816_0000,
                    Encoding::Lz77,
                    2048
                ),
            ]
        );
        assert_eq!(report.len(), 7);
        assert!(report
            .iter()
            .all(|line| line.resolution == Resolution::UniqueSignature));
    });
}

#[test]
fn every_root_is_expected_as_the_static_symbol_upstream_names() {
    let (rom, entries) = fixture();
    with_context("evolution-symbols", &rom, entries, |ctx| {
        let mut report = Vec::new();
        locate(ctx, &mut report).expect("located");
        let symbol = |id: &str| {
            report
                .iter()
                .find(|line| line.id == id)
                .map(|line| line.symbol.clone())
        };
        let local = |name: &str| Some(SymbolExpectation::Local(name.to_owned()));
        assert_eq!(symbol("evolution-scene/image/bg"), local("sBgAnim_Gfx"));
        assert_eq!(
            symbol("evolution-scene/image/evo_sparkle"),
            local("sEvoSparkle_Gfx")
        );
        assert_eq!(
            symbol("evolution-scene/palette/bg_anim_intro"),
            local("sBgAnim_Intro_Pal")
        );
        assert_eq!(
            symbol("evolution-scene/palette/bg_anim"),
            local("sBgAnim_Pal")
        );
        assert_eq!(
            symbol("evolution-scene/palette/evo_sparkle"),
            local("sEvoSparkle_Pal")
        );
        assert_eq!(
            symbol("evolution-scene/raw/bg_inner"),
            local("sBgAnim_Inner_Tilemap")
        );
        assert_eq!(
            symbol("evolution-scene/raw/bg_outer"),
            local("sBgAnim_Outer_Tilemap")
        );
    });
}

#[test]
fn a_compressed_palette_is_not_a_raw_scene_palette() {
    let (rom, entries) = fixture();
    let (_, anim_bytes) = palette_bytes(0x2222, 32);
    let mut rom = rom;
    // Replace the raw animation palette with its LZ77 form: the root is raw.
    rom[0x13_0000..0x13_0000 + anim_bytes.len()].fill(0);
    let packed = lz77_literals(&anim_bytes);
    rom[0x13_0000..0x13_0000 + packed.len()].copy_from_slice(&packed);
    with_context("evolution-palette-compressed", &rom, entries, |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert!(matches!(err, GenRomProfileError::NotFound { .. }), "{err}");
    });
}

#[test]
fn an_uncompressed_tilemap_is_not_a_scene_tilemap() {
    let (rom, entries) = fixture();
    let mut rom = rom;
    // Replace the inner tilemap's stream with the bare map: the root is LZ77.
    let inner = tilemap(3);
    let packed_len = lz77_literals(&inner).len();
    rom[0x15_0000..0x15_0000 + packed_len].fill(0);
    rom[0x15_0000..0x15_0000 + inner.len()].copy_from_slice(&inner);
    with_context("evolution-tilemap-uncompressed", &rom, entries, |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::NotFound { id } if id == "evolution-scene/raw/bg_inner"),
            "{err}"
        );
    });
}

#[test]
fn a_tilemap_present_twice_is_ambiguous() {
    let (rom, entries) = fixture();
    let mut rom = rom;
    let packed = lz77_literals(&tilemap(3));
    rom[0x18_0000..0x18_0000 + packed.len()].copy_from_slice(&packed);
    with_context("evolution-tilemap-twice", &rom, entries, |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert!(matches!(err, GenRomProfileError::Ambiguous { .. }), "{err}");
    });
}

#[test]
fn a_surplus_or_missing_scene_entry_is_refused_before_any_report_line() {
    let (rom, mut entries) = fixture();
    entries.push(raw_entry("evolution-scene/raw/extra".into(), vec![0]));
    with_context("evolution-surplus", &rom, entries, |ctx| {
        let mut report = Vec::new();
        let err = locate(ctx, &mut report).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == "evolution-scene/raw/extra"),
            "{err}"
        );
        assert!(report.is_empty());
    });

    for id in evolution_scene_ids() {
        let (rom, mut entries) = fixture();
        entries.retain(|entry| entry.id != id);
        with_context("evolution-missing", &rom, entries, |ctx| {
            let mut report = Vec::new();
            let err = locate(ctx, &mut report).unwrap_err();
            assert!(
                matches!(&err, GenRomProfileError::MissingPackEntry(missing) if *missing == id),
                "{id}: {err}"
            );
            assert!(report.is_empty());
        });
    }
}

#[test]
fn a_wrong_kind_scene_entry_is_refused() {
    let (rom, mut entries) = fixture();
    entries.retain(|entry| entry.id != "evolution-scene/palette/bg_anim");
    entries.push(raw_entry(
        "evolution-scene/palette/bg_anim".into(),
        vec![0; 64],
    ));
    with_context("evolution-wrong-kind", &rom, entries, |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert!(
            matches!(err, GenRomProfileError::WrongPackEntryKind { .. }),
            "{err}"
        );
    });
}
