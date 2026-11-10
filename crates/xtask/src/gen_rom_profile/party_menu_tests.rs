//! Party-menu locator tests over a synthetic ROM and pack.
//!
//! Each root gets its own bytes at its production shape and encoding. The
//! witness tests plant copies of a sheet and sprite-sheet records beside them.

use pack_format::{image_entry_from_tiles, palette_entry, raw_entry, PackEntry};
use rom_import::fixture::RomFixture;
use rom_import::Encoding;

use super::{
    locate, Root, Shape, HELD_ITEM_TAG, POKEBALL_SMALL_TAG, POKEBALL_TAG, ROOTS, SHEET_SIZE_FIELD,
    SHEET_TAG_FIELD,
};
use crate::extract::scope::party_menu_ids;
use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::plan::{ReportLine, Resolution, SymbolExpectation};
use crate::gen_rom_profile::tests::{distinctive_tiles, lz77_literals, with_context};

/// Where root `n` sits: far apart, so no stream overlaps another.
const FIRST_ROOT: usize = 0x10_0000;
const ROOT_STRIDE: usize = 0x1000;
/// The sprite-sheet records.
const RECORDS: usize = 0x30_0000;
/// Copies of a root placed below every real root.
const STRAY: usize = 0x0A_0000;

const BG_SHEET: usize = 0;
const POKEBALL_SHEET: usize = 1;
const HELD_ITEM_SHEET: usize = 2;
const SMALL_SHEET: usize = 3;
const BG_PALETTE: usize = 4;
const BG_MAP: usize = 7;
const SLOT_MAIN: usize = 8;
const CONFIRM_MAP: usize = 13;
/// Roots before the first map: four sheets and three palettes.
const NON_MAPS: usize = 7;

fn bus(offset: usize) -> u32 {
    0x0800_0000 + u32::try_from(offset).expect("a fixture offset fits a bus address")
}

fn offset_of(index: usize) -> usize {
    FIRST_ROOT + index * ROOT_STRIDE
}

/// The decompressed bytes of `ROOTS[index]`, distinct from every other root's.
fn content(index: usize) -> Vec<u8> {
    let salt = u8::try_from(index * 17 + 1).expect("fifteen roots");
    match ROOTS[index].shape {
        Shape::Image { tiles, .. } => distinctive_tiles(tiles as usize)
            .iter()
            .map(|byte| byte ^ salt)
            .collect(),
        Shape::Palette { colors } => (0..u32::from(colors))
            .flat_map(|n| {
                let color = (n * 0x0421 + u32::from(salt) * 0x0123) & 0x7FFF;
                u16::try_from(color)
                    .expect("masked to 15 bits")
                    .to_le_bytes()
            })
            .collect(),
        Shape::Map { len } => (0..len)
            .map(|n| u8::try_from((n * 13 + u32::from(salt) * 29) % 251).expect("modulo 251"))
            .collect(),
    }
}

fn stored(root: &Root, decoded: &[u8]) -> Vec<u8> {
    match root.encoding {
        Encoding::Lz77 => lz77_literals(decoded),
        Encoding::Raw => decoded.to_vec(),
    }
}

fn entry(index: usize) -> PackEntry {
    let root = &ROOTS[index];
    let decoded = content(index);
    let id = root.id.to_owned();
    match root.shape {
        Shape::Image { width, height, .. } => {
            image_entry_from_tiles(id, &decoded, 4, width, height, None).expect("image")
        }
        Shape::Palette { .. } => {
            let colors: Vec<u16> = decoded
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect();
            palette_entry(id, &colors).expect("palette")
        }
        Shape::Map { .. } => raw_entry(id, decoded),
    }
}

fn entries() -> Vec<PackEntry> {
    (0..ROOTS.len()).map(entry).collect()
}

fn record(data: u32, size: u16, tag: u16) -> Vec<u8> {
    let mut row = data.to_le_bytes().to_vec();
    row.extend_from_slice(&size.to_le_bytes());
    row.extend_from_slice(&tag.to_le_bytes());
    row
}

/// The record upstream writes for sheet `index`, naming `target`.
fn sheet_record(index: usize, target: usize) -> Vec<u8> {
    let named = ROOTS[index].record.expect("a sheet with a record");
    record(bus(target), named.size, named.tag)
}

/// Every root in the ROM, with no sprite-sheet record yet.
fn bare() -> RomFixture {
    let mut fixture = RomFixture::new().emerald_header();
    for (index, root) in ROOTS.iter().enumerate() {
        fixture = fixture.write(offset_of(index), &stored(root, &content(index)));
    }
    fixture
}

/// Where the record for sheet `index` sits.
const fn record_slot(index: usize) -> usize {
    RECORDS + index * 0x100
}

/// Every root, and the records naming the three witnessed sheets.
fn fixture() -> RomFixture {
    [HELD_ITEM_SHEET, POKEBALL_SHEET, SMALL_SHEET]
        .into_iter()
        .fold(bare(), |fixture, index| {
            fixture.write(record_slot(index), &sheet_record(index, offset_of(index)))
        })
}

fn line<'a>(report: &'a [ReportLine], id: &str) -> &'a ReportLine {
    report
        .iter()
        .find(|line| line.id == id)
        .unwrap_or_else(|| panic!("no report line for {id}"))
}

fn assert_struct_mismatch(err: &GenRomProfileError, id: &str) {
    assert!(
        matches!(err, GenRomProfileError::StructMismatch { id: found, .. } if found == id),
        "{err}"
    );
}

#[test]
fn the_roots_are_the_checkouts_fixed_inventory() {
    let mut roots: Vec<String> = ROOTS.iter().map(|root| root.id.to_owned()).collect();
    roots.sort();
    let mut expected = party_menu_ids();
    expected.sort();
    assert_eq!(roots, expected);
}

#[test]
fn all_fifteen_roots_are_found_with_their_production_shapes() {
    let rom = fixture().finish();
    with_context("party-menu-found", &rom, entries(), |ctx| {
        let mut report = Vec::new();
        let plan = locate(ctx, &mut report).expect("located");
        assert_eq!(report.len(), 15);
        assert_eq!(
            (plan.images.len(), plan.palettes.len(), plan.tilemaps.len()),
            (4, 3, 8)
        );
        for image in &plan.images {
            let index = ROOTS.iter().position(|r| r.id == image.id).expect("a root");
            assert_eq!(image.addr, bus(offset_of(index)), "{}", image.id);
            assert_eq!(
                (image.rom_bit_depth, image.pack_bit_depth),
                (4, 4),
                "{}",
                image.id
            );
            assert_eq!((image.metatile_width, image.metatile_height), (1, 1));
        }
        let [bg, pokeball, held, small] = &plan.images[..] else {
            panic!("four images");
        };
        assert_eq!(
            (bg.width, bg.height, bg.tile_count, bg.encoding),
            (8, 496, 62, Encoding::Lz77)
        );
        assert_eq!(
            (
                pokeball.width,
                pokeball.height,
                pokeball.tile_count,
                pokeball.encoding
            ),
            (32, 64, 32, Encoding::Lz77)
        );
        assert_eq!(
            (held.width, held.height, held.tile_count, held.encoding),
            (8, 16, 2, Encoding::Raw)
        );
        assert_eq!(
            (small.width, small.height, small.tile_count, small.encoding),
            (16, 96, 24, Encoding::Lz77)
        );
        let colors: Vec<(u16, Encoding)> = plan
            .palettes
            .iter()
            .map(|p| (p.color_count, p.encoding))
            .collect();
        assert_eq!(
            colors,
            [
                (176, Encoding::Lz77),
                (16, Encoding::Lz77),
                (16, Encoding::Raw)
            ]
        );
        let maps: Vec<(u32, Encoding)> =
            plan.tilemaps.iter().map(|m| (m.len, m.encoding)).collect();
        assert_eq!(
            maps,
            [
                (2048, Encoding::Lz77),
                (70, Encoding::Raw),
                (70, Encoding::Raw),
                (54, Encoding::Raw),
                (54, Encoding::Raw),
                (54, Encoding::Raw),
                (28, Encoding::Raw),
                (28, Encoding::Raw),
            ]
        );
        assert_eq!(
            plan.tilemaps[SLOT_MAIN - NON_MAPS].addr,
            bus(offset_of(SLOT_MAIN))
        );
    });
}

#[test]
fn the_report_names_what_a_map_can_and_cannot_confirm() {
    let rom = fixture().finish();
    with_context("party-menu-symbols", &rom, entries(), |ctx| {
        let mut report = Vec::new();
        locate(ctx, &mut report).expect("located");
        let global = |id| SymbolExpectation::Exact(String::from(id));
        let local = |id| SymbolExpectation::Local(String::from(id));
        for (id, expected) in [
            ("party-menu/image/bg", global("gPartyMenuBg_Gfx")),
            ("party-menu/palette/bg", global("gPartyMenuBg_Pal")),
            ("party-menu/raw/bg", global("gPartyMenuBg_Tilemap")),
            (
                "party-menu/image/pokeball",
                global("gPartyMenuPokeball_Gfx"),
            ),
            (
                "party-menu/palette/pokeball",
                global("gPartyMenuPokeball_Pal"),
            ),
            ("party-menu/image/hold_icons", local("sHeldItemGfx")),
            (
                "party-menu/image/pokeball_small",
                global("gPartyMenuPokeballSmall_Gfx"),
            ),
            ("party-menu/palette/hold_icons", local("sHeldItemPalette")),
            (
                "party-menu/raw/slot_main_no_hp",
                local("sSlotTilemap_MainNoHP"),
            ),
            (
                "party-menu/raw/confirm_button",
                local("sConfirmButton_Tilemap"),
            ),
        ] {
            assert_eq!(line(&report, id).symbol, expected, "{id}");
        }
        assert_eq!(line(&report, "party-menu/image/bg").len, 62 * 32);
        assert_eq!(line(&report, "party-menu/palette/bg").len, 176 * 2);
        assert_eq!(line(&report, "party-menu/image/pokeball_small").len, 0x300);
        assert_eq!(line(&report, "party-menu/raw/slot_wide").len, 54);
    });
}

#[test]
fn each_sheet_is_settled_by_its_record_and_the_rest_by_their_bytes() {
    let rom = fixture().finish();
    with_context("party-menu-resolution", &rom, entries(), |ctx| {
        let mut report = Vec::new();
        locate(ctx, &mut report).expect("located");
        for line in &report {
            let expected = if ["pokeball", "hold_icons", "pokeball_small"]
                .map(|stem| format!("party-menu/image/{stem}"))
                .contains(&line.id)
            {
                Resolution::StructDerived
            } else {
                Resolution::UniqueSignature
            };
            assert_eq!(line.resolution, expected, "{}", line.id);
        }
    });
}

#[test]
fn a_witnessed_sheet_takes_the_copy_its_record_names_over_sixteen_lower_ones() {
    for index in [HELD_ITEM_SHEET, POKEBALL_SHEET, SMALL_SHEET] {
        let mut fixture = fixture();
        for copy in 0..17usize {
            fixture = fixture.write(
                STRAY + copy * ROOT_STRIDE,
                &stored(&ROOTS[index], &content(index)),
            );
        }
        // The record names the real copy, which sits above all of them.
        let rom = fixture
            .write(record_slot(index), &sheet_record(index, offset_of(index)))
            .finish();
        with_context("party-menu-witness", &rom, entries(), |ctx| {
            let mut report = Vec::new();
            let plan = locate(ctx, &mut report).expect("located");
            let image = plan
                .images
                .iter()
                .find(|i| i.id == ROOTS[index].id)
                .unwrap();
            assert_eq!(image.addr, bus(offset_of(index)), "{}", ROOTS[index].id);
            let found = line(&report, ROOTS[index].id);
            assert_eq!(found.resolution, Resolution::StructDerived);
            assert!(found.note.as_deref().unwrap().contains("18 addresses"));
        });
    }
}

#[test]
fn a_record_naming_the_lowest_copy_leaves_the_higher_one_unchosen() {
    let stray = STRAY;
    let rom = fixture()
        .write(
            stray,
            &stored(&ROOTS[HELD_ITEM_SHEET], &content(HELD_ITEM_SHEET)),
        )
        .write(
            record_slot(HELD_ITEM_SHEET),
            &sheet_record(HELD_ITEM_SHEET, stray),
        )
        .finish();
    with_context("party-menu-low-copy", &rom, entries(), |ctx| {
        let plan = locate(ctx, &mut Vec::new()).expect("located");
        assert_eq!(plan.images[2].addr, bus(stray));
    });
}

#[test]
fn a_pointer_without_the_whole_record_names_nothing() {
    let named = ROOTS[HELD_ITEM_SHEET].record.unwrap();
    let target = bus(offset_of(HELD_ITEM_SHEET));
    // The default record, overwritten by each decoy in turn.
    let mut shifted = target.to_le_bytes().to_vec();
    shifted.extend_from_slice(&[0, 0]);
    shifted.extend_from_slice(&named.size.to_le_bytes());
    shifted.extend_from_slice(&named.tag.to_le_bytes());
    for (label, decoy) in [
        ("no record", vec![0; 8]),
        ("wrong size", record(target, named.size + 32, named.tag)),
        ("wrong tag", record(target, named.size, named.tag + 1)),
        // The size and tag follow the pointer, but not at the offsets of a
        // sprite-sheet record.
        ("shifted fields", shifted),
    ] {
        let rom = fixture()
            .write(record_slot(HELD_ITEM_SHEET), &decoy)
            .finish();
        with_context("party-menu-decoy", &rom, entries(), |ctx| {
            let err = locate(ctx, &mut Vec::new()).unwrap_err();
            assert!(
                matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == "party-menu/image/hold_icons"),
                "{label}: {err}"
            );
        });
    }
}

#[test]
fn the_small_ball_needs_its_own_size_and_tag_not_the_big_balls() {
    let named = ROOTS[SMALL_SHEET].record.unwrap();
    let target = bus(offset_of(SMALL_SHEET));
    let big = ROOTS[POKEBALL_SHEET].record.unwrap();
    for (label, decoy) in [
        ("no record", vec![0; 8]),
        ("wrong size", record(target, named.size + 32, named.tag)),
        // Both balls load through one function and differ only in tag.
        (
            "the big ball's tag",
            record(target, named.size, POKEBALL_TAG),
        ),
        ("the big ball's size", record(target, big.size, named.tag)),
    ] {
        let rom = fixture().write(record_slot(SMALL_SHEET), &decoy).finish();
        with_context("party-menu-small-decoy", &rom, entries(), |ctx| {
            let err = locate(ctx, &mut Vec::new()).unwrap_err();
            assert!(
                matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == "party-menu/image/pokeball_small"),
                "{label}: {err}"
            );
        });
    }
}

#[test]
fn an_unaligned_record_names_nothing() {
    let rom = fixture()
        .write(record_slot(POKEBALL_SHEET), &[0; 8])
        .write(
            RECORDS + 0x802,
            &sheet_record(POKEBALL_SHEET, offset_of(POKEBALL_SHEET)),
        )
        .finish();
    with_context("party-menu-unaligned", &rom, entries(), |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert_struct_mismatch(&err, "party-menu/image/pokeball");
    });
}

#[test]
fn two_copies_both_named_by_a_record_settle_nothing() {
    let rom = fixture()
        .write(
            STRAY,
            &stored(&ROOTS[POKEBALL_SHEET], &content(POKEBALL_SHEET)),
        )
        .write(RECORDS + 0x800, &sheet_record(POKEBALL_SHEET, STRAY))
        .finish();
    with_context("party-menu-two-named", &rom, entries(), |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert_struct_mismatch(&err, "party-menu/image/pokeball");
    });
}

#[test]
fn the_record_size_and_tag_are_the_ones_upstream_writes() {
    assert_eq!(ROOTS[HELD_ITEM_SHEET].record.unwrap().tag, HELD_ITEM_TAG);
    assert_eq!(HELD_ITEM_TAG, 55120);
    assert_eq!(ROOTS[POKEBALL_SHEET].record.unwrap().size, 0x400);
    assert_eq!(POKEBALL_TAG, 1200);
    assert_eq!(ROOTS[SMALL_SHEET].record.unwrap().size, 0x300);
    assert_eq!(ROOTS[SMALL_SHEET].record.unwrap().tag, POKEBALL_SMALL_TAG);
    assert_eq!(POKEBALL_SMALL_TAG, 1201);
    assert_eq!((SHEET_SIZE_FIELD, SHEET_TAG_FIELD), (4, 6));
}

#[test]
fn a_root_without_a_record_stored_twice_is_an_arbitrary_copy_at_the_lowest_address() {
    for index in [BG_SHEET, BG_PALETTE, BG_MAP, SLOT_MAIN] {
        let rom = fixture()
            .write(STRAY, &stored(&ROOTS[index], &content(index)))
            .finish();
        with_context("party-menu-identical", &rom, entries(), |ctx| {
            let mut report = Vec::new();
            locate(ctx, &mut report).expect("located");
            let found = line(&report, ROOTS[index].id);
            assert_eq!(found.addr, bus(STRAY), "{}", ROOTS[index].id);
            assert_eq!(found.resolution, Resolution::ArbitraryAmongIdentical);
            assert!(found.note.as_deref().unwrap().contains("2 addresses"));
        });
    }
}

#[test]
fn a_near_copy_is_not_a_candidate() {
    let mut near = content(SLOT_MAIN);
    *near.last_mut().unwrap() ^= 1;
    let rom = fixture().write(STRAY, &near).finish();
    with_context("party-menu-near", &rom, entries(), |ctx| {
        let mut report = Vec::new();
        locate(ctx, &mut report).expect("located");
        let found = line(&report, ROOTS[SLOT_MAIN].id);
        assert_eq!(found.addr, bus(offset_of(SLOT_MAIN)));
        assert_eq!(found.resolution, Resolution::UniqueSignature);
    });
}

#[test]
fn art_the_rom_lacks_is_not_found() {
    for (index, root) in ROOTS.iter().enumerate() {
        let rom = fixture()
            .write(offset_of(index), &vec![0xFF; 0xE00])
            .finish();
        with_context("party-menu-missing-art", &rom, entries(), |ctx| {
            let err = locate(ctx, &mut Vec::new()).unwrap_err();
            assert!(
                matches!(&err, GenRomProfileError::NotFound { id } if id == root.id),
                "{}: {err}",
                root.id
            );
        });
    }
}

#[test]
fn a_root_stored_in_the_wrong_encoding_is_not_accepted() {
    // The raw map's bytes, stored as a stream, and the stream's, stored raw.
    for (index, root) in [(SLOT_MAIN, &ROOTS[SLOT_MAIN]), (BG_SHEET, &ROOTS[BG_SHEET])] {
        let flipped = match root.encoding {
            Encoding::Raw => lz77_literals(&content(index)),
            Encoding::Lz77 => content(index),
        };
        let rom = fixture()
            .write(offset_of(index), &vec![0xFF; 0xE00])
            .write(offset_of(index), &flipped)
            .finish();
        with_context("party-menu-wrong-encoding", &rom, entries(), |ctx| {
            let err = locate(ctx, &mut Vec::new()).unwrap_err();
            assert!(
                matches!(&err, GenRomProfileError::NotFound { id } if id == root.id),
                "{}: {err}",
                root.id
            );
        });
    }
}

#[test]
fn a_pack_entry_of_another_shape_is_refused_before_any_report_line() {
    let wrong: Vec<(usize, PackEntry)> = vec![
        // The 64-tile square background `gbagfx` narrows from the PNG.
        (
            BG_SHEET,
            image_entry_from_tiles(
                "party-menu/image/bg".to_owned(),
                &distinctive_tiles(64),
                4,
                64,
                64,
                None,
            )
            .unwrap(),
        ),
        (
            SMALL_SHEET,
            image_entry_from_tiles(
                "party-menu/image/pokeball_small".to_owned(),
                &distinctive_tiles(24),
                4,
                8,
                192,
                None,
            )
            .unwrap(),
        ),
        (
            SMALL_SHEET,
            image_entry_from_tiles(
                "party-menu/image/pokeball_small".to_owned(),
                &distinctive_tiles(48),
                8,
                16,
                96,
                None,
            )
            .unwrap(),
        ),
        (
            BG_PALETTE,
            palette_entry("party-menu/palette/bg".to_owned(), &[1; 16]).unwrap(),
        ),
        (
            SLOT_MAIN,
            raw_entry("party-menu/raw/slot_main".to_owned(), vec![1; 140]),
        ),
        (
            CONFIRM_MAP,
            raw_entry("party-menu/raw/confirm_button".to_owned(), vec![1; 14]),
        ),
    ];
    let rom = fixture().finish();
    for (index, replacement) in wrong {
        let mut pack = entries();
        pack[index] = replacement;
        with_context("party-menu-shape", &rom, pack, |ctx| {
            let mut report = Vec::new();
            let err = locate(ctx, &mut report).unwrap_err();
            assert!(
                matches!(&err, GenRomProfileError::EntryShape { id, .. } if id == ROOTS[index].id),
                "{err}"
            );
            assert!(report.is_empty());
        });
    }
}

#[test]
fn a_pack_missing_an_entry_or_holding_an_extra_one_is_refused() {
    let rom = fixture().finish();
    for index in 0..ROOTS.len() {
        let mut pack = entries();
        let missing = pack.remove(index);
        with_context("party-menu-missing-entry", &rom, pack, |ctx| {
            let mut report = Vec::new();
            let err = locate(ctx, &mut report).unwrap_err();
            assert!(
                matches!(&err, GenRomProfileError::MissingPackEntry(id) if *id == missing.id),
                "{err}"
            );
            assert!(report.is_empty());
        });
    }
    let mut pack = entries();
    pack.push(raw_entry("party-menu/raw/extra".to_owned(), vec![1; 8]));
    with_context("party-menu-extra-entry", &rom, pack, |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert_struct_mismatch(&err, "party-menu/raw/extra");
    });
}
