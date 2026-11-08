//! Opening-scene locator tests over a synthetic ROM and pack.

use pack_format::{image_entry_from_tiles, palette_entry, raw_entry, PackEntry};
use rom_import::fixture::RomFixture;
use rom_import::Encoding;

use super::locate;
use crate::extract::scope::opening_scene_ids;
use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::plan::{Resolution, SymbolExpectation};
use crate::gen_rom_profile::tests::{
    cross_check_ld_map, distinctive_tiles, lz77_literals, with_context,
};

const IMAGE_IDS: [&str; 5] = [
    "opening-scene/image/birch",
    "opening-scene/image/clock_hand",
    "opening-scene/image/lotad",
    "opening-scene/image/shadow",
    "opening-scene/image/wall_clock",
];
const PALETTE_IDS: [&str; 8] = [
    "opening-scene/palette/bg0",
    "opening-scene/palette/bg1",
    "opening-scene/palette/bg2",
    "opening-scene/palette/birch",
    "opening-scene/palette/clock_female",
    "opening-scene/palette/clock_male",
    "opening-scene/palette/clock_text_prompt",
    "opening-scene/palette/lotad",
];
const TILEMAP_IDS: [&str; 3] = [
    "opening-scene/raw/birch_speech_map",
    "opening-scene/raw/clock_start",
    "opening-scene/raw/clock_view",
];

fn palette_colors(seed: u16, colors: u16) -> (Vec<u16>, Vec<u8>) {
    let colors: Vec<u16> = (0..colors).map(|n| n * 0x0421 + seed).collect();
    let bytes = colors.iter().flat_map(|c| c.to_le_bytes()).collect();
    (colors, bytes)
}

fn tilemap(seed: u8) -> Vec<u8> {
    (0..64u8)
        .map(|n| n.wrapping_mul(7).wrapping_add(seed))
        .collect()
}

/// Tiles that differ per sheet.
fn tiles(seed: u8) -> Vec<u8> {
    distinctive_tiles(4).iter().map(|b| b ^ seed).collect()
}

fn sheet(id: &str, tiles: &[u8]) -> PackEntry {
    image_entry_from_tiles(id.to_owned(), tiles, 4, 16, 16, None).expect("sheet entry")
}

/// A ROM and pack holding all sixteen roots, one distinct copy each, at
/// `0x10_0000 + 0x1_0000 * n` in id order (sheets, palettes, tilemaps).
fn fixture() -> (Vec<u8>, Vec<PackEntry>) {
    let mut rom = RomFixture::new().emerald_header();
    let mut entries = Vec::new();
    let mut slot = 0usize;
    let mut place = |rom: RomFixture, bytes: &[u8]| {
        let at = 0x10_0000 + 0x1_0000 * slot;
        slot += 1;
        rom.write(at, bytes)
    };
    for (n, id) in IMAGE_IDS.into_iter().enumerate() {
        let sheet_tiles = tiles(u8::try_from(n).unwrap() * 3 + 1);
        // Birch's sheet is stored uncompressed; the rest are LZ77 streams.
        let stored = if id == "opening-scene/image/birch" {
            sheet_tiles.clone()
        } else {
            lz77_literals(&sheet_tiles)
        };
        rom = place(rom, &stored);
        entries.push(sheet(id, &sheet_tiles));
    }
    for (n, id) in PALETTE_IDS.into_iter().enumerate() {
        let count = if id.ends_with("text_prompt") { 4 } else { 16 };
        let (colors, bytes) = palette_colors(0x0101 * (u16::try_from(n).unwrap() + 1), count);
        // Lotad's normal palette is the one compressed bank.
        let stored = if id.ends_with("/lotad") {
            lz77_literals(&bytes)
        } else {
            bytes
        };
        rom = place(rom, &stored);
        entries.push(palette_entry(id.to_owned(), &colors).expect("palette"));
    }
    for (n, id) in TILEMAP_IDS.into_iter().enumerate() {
        let map = tilemap(u8::try_from(n).unwrap() * 11 + 3);
        rom = place(rom, &lz77_literals(&map));
        entries.push(raw_entry(id.to_owned(), map));
    }
    (rom.finish(), entries)
}

#[test]
fn the_fixture_covers_the_whole_inventory() {
    let mut fixed: Vec<String> = opening_scene_ids();
    fixed.sort();
    let mut fixture_ids: Vec<String> = fixture().1.into_iter().map(|e| e.id).collect();
    fixture_ids.sort();
    assert_eq!(fixed, fixture_ids);
}

#[test]
fn every_family_is_located_with_its_own_encoding() {
    let (rom, entries) = fixture();
    with_context("scene-families", &rom, entries, |ctx| {
        let mut report = Vec::new();
        let plan = locate(ctx, &mut report).expect("located");
        let images: Vec<_> = plan
            .images
            .iter()
            .map(|i| (i.id.as_str(), i.addr, i.encoding, i.rom_bit_depth))
            .collect();
        let expected_images: Vec<_> = IMAGE_IDS
            .iter()
            .enumerate()
            .map(|(n, id)| {
                let encoding = if *id == "opening-scene/image/birch" {
                    Encoding::Raw
                } else {
                    Encoding::Lz77
                };
                (
                    *id,
                    0x0810_0000 + 0x1_0000 * u32::try_from(n).unwrap(),
                    encoding,
                    4,
                )
            })
            .collect();
        assert_eq!(images, expected_images);

        let palettes: Vec<_> = plan
            .palettes
            .iter()
            .map(|p| (p.id.as_str(), p.addr, p.encoding, p.color_count))
            .collect();
        let expected_palettes: Vec<_> = PALETTE_IDS
            .iter()
            .enumerate()
            .map(|(n, id)| {
                let encoding = if id.ends_with("/lotad") {
                    Encoding::Lz77
                } else {
                    Encoding::Raw
                };
                let count = if id.ends_with("text_prompt") { 4 } else { 16 };
                (
                    *id,
                    0x0815_0000 + 0x1_0000 * u32::try_from(n).unwrap(),
                    encoding,
                    count,
                )
            })
            .collect();
        assert_eq!(palettes, expected_palettes);

        let tilemaps: Vec<_> = plan
            .tilemaps
            .iter()
            .map(|t| (t.id.as_str(), t.addr, t.encoding, t.len))
            .collect();
        let expected_tilemaps: Vec<_> = TILEMAP_IDS
            .iter()
            .enumerate()
            .map(|(n, id)| {
                (
                    *id,
                    0x081D_0000 + 0x1_0000 * u32::try_from(n).unwrap(),
                    Encoding::Lz77,
                    64,
                )
            })
            .collect();
        assert_eq!(tilemaps, expected_tilemaps);
        assert_eq!(report.len(), 16);
        assert!(report
            .iter()
            .all(|line| line.resolution == Resolution::UniqueSignature));
    });
}

#[test]
fn a_raw_lotad_palette_is_not_the_compressed_one() {
    let (mut rom, entries) = fixture();
    // Lotad's bank is slot 12 (sheets 0-4, then the sorted palettes 5-12).
    let at = 0x10_0000 + 0x1_0000 * 12;
    let (_, bytes) = palette_colors(0x0101 * 8, 16);
    rom[at..at + 64].fill(0);
    rom[at..at + bytes.len()].copy_from_slice(&bytes);
    with_context("scene-lotad-raw", &rom, entries, |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::NotFound { id, .. } if id == "opening-scene/palette/lotad"),
            "{err}"
        );
    });
}

#[test]
fn a_compressed_birch_sheet_is_not_the_uncompressed_one() {
    let (mut rom, entries) = fixture();
    let at = 0x10_0000;
    let packed = lz77_literals(&tiles(1));
    rom[at..at + 128].fill(0);
    rom[at..at + packed.len()].copy_from_slice(&packed);
    with_context("scene-birch-compressed", &rom, entries, |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert!(matches!(err, GenRomProfileError::NotFound { .. }), "{err}");
    });
}

#[test]
fn a_tilemap_present_twice_is_ambiguous() {
    let (mut rom, entries) = fixture();
    let packed = lz77_literals(&tilemap(3));
    rom[0x30_0000..0x30_0000 + packed.len()].copy_from_slice(&packed);
    with_context("scene-tilemap-twice", &rom, entries, |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert!(matches!(err, GenRomProfileError::Ambiguous { .. }), "{err}");
    });
}

#[test]
fn a_surplus_or_missing_scene_entry_is_refused_before_any_report_line() {
    let (rom, mut entries) = fixture();
    entries.push(raw_entry("opening-scene/raw/extra".into(), vec![0]));
    with_context("scene-surplus", &rom, entries, |ctx| {
        let mut report = Vec::new();
        let err = locate(ctx, &mut report).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == "opening-scene/raw/extra"),
            "{err}"
        );
        assert!(report.is_empty());
    });

    let (rom, mut entries) = fixture();
    entries.retain(|entry| entry.id != "opening-scene/raw/clock_view");
    with_context("scene-missing", &rom, entries, |ctx| {
        let mut report = Vec::new();
        let err = locate(ctx, &mut report).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::MissingPackEntry(id) if id == "opening-scene/raw/clock_view"),
            "{err}"
        );
        assert!(report.is_empty());
    });
}

#[test]
fn a_wrong_kind_scene_entry_is_refused() {
    let (rom, mut entries) = fixture();
    entries.retain(|entry| entry.id != "opening-scene/palette/bg0");
    entries.push(raw_entry("opening-scene/palette/bg0".into(), vec![0; 32]));
    with_context("scene-wrong-kind", &rom, entries, |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert!(
            matches!(err, GenRomProfileError::WrongPackEntryKind { .. }),
            "{err}"
        );
    });
}

/// The scene roots upstream declares `static`, with their symbols. `bg0` and
/// `bg1` are the two rows of one `sBirchSpeechBgPals` (`main_menu.c:250-253`).
const STATIC_ROOTS: [(&str, &str); 8] = [
    ("opening-scene/image/shadow", "sBirchSpeechShadowGfx"), // main_menu.c:255
    ("opening-scene/image/birch", "sNewGameBirch_Gfx"),      // field_effect.c:245
    ("opening-scene/image/clock_hand", "sHand_Gfx"),         // wallclock.c:74
    ("opening-scene/palette/bg0", "sBirchSpeechBgPals"),     // main_menu.c:250
    ("opening-scene/palette/bg1", "sBirchSpeechBgPals"),     // main_menu.c:250
    ("opening-scene/palette/bg2", "sBirchSpeechBgGradientPal"), // main_menu.c:257
    ("opening-scene/palette/birch", "sNewGameBirch_Pal"),    // field_effect.c:247
    ("opening-scene/raw/birch_speech_map", "sBirchSpeechBgMap"), // main_menu.c:256
];

/// Every other scene root is global: `gMonStillFrontPic_Lotad` and
/// `gMonPalette_Lotad` (`pokemon.h:1888-1889`), `gWallClock_Gfx`,
/// `gWallClock{Male,Female}_Pal` and the two `gWallClock*_Tilemap`
/// (`graphics.c:1350-1354`). `sTextPrompt_Pal` is static (`wallclock.c:75`).
const GLOBAL_ROOTS: [&str; 7] = [
    "opening-scene/image/lotad",
    "opening-scene/image/wall_clock",
    "opening-scene/palette/clock_female",
    "opening-scene/palette/clock_male",
    "opening-scene/palette/lotad",
    "opening-scene/raw/clock_start",
    "opening-scene/raw/clock_view",
];

#[test]
fn static_scene_roots_survive_a_real_ld_map() {
    let (rom, entries) = fixture();
    with_context("scene-static-map", &rom, entries, |ctx| {
        let mut report = Vec::new();
        locate(ctx, &mut report).expect("located");
        assert_eq!(report.len(), 16);
        let mut statics = STATIC_ROOTS.to_vec();
        statics.push(("opening-scene/palette/clock_text_prompt", "sTextPrompt_Pal"));
        for (id, symbol) in &statics {
            let line = report.iter().find(|line| line.id == *id).expect(id);
            assert_eq!(
                line.symbol,
                SymbolExpectation::Local((*symbol).to_owned()),
                "{id}"
            );
        }
        for id in GLOBAL_ROOTS {
            let line = report.iter().find(|line| line.id == id).expect(id);
            assert_eq!(line.symbol, SymbolExpectation::Unnamed, "{id}");
        }
        let check = cross_check_ld_map("scene-static-map", &mut report, &[])
            .expect("a correct ROM and an ordinary ld map must pass");
        assert_eq!((check.local, check.confirmed), (9, 7));
    });
}

#[test]
fn a_global_scene_root_still_needs_its_map_symbol() {
    for id in GLOBAL_ROOTS {
        let (rom, entries) = fixture();
        with_context("scene-global-map", &rom, entries, |ctx| {
            let mut report = Vec::new();
            locate(ctx, &mut report).expect("located");
            let err = cross_check_ld_map("scene-global-map", &mut report, &[id])
                .expect_err("a global root missing from the map is a mismatch");
            assert!(
                matches!(&err, GenRomProfileError::MapMismatch { id: bad, .. } if bad == id),
                "{id}: {err}"
            );
        });
    }
}
