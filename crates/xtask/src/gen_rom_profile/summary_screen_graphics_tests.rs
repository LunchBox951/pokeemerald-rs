//! Summary-screen graphics locator tests over a synthetic ROM and pack.
//!
//! Each root gets its own bytes at its production shape and encoding. The
//! witness tests plant copies of a root and the upstream records beside them.

use pack_format::{image_entry_from_tiles, palette_entry, raw_entry, PackEntry};
use rom_import::fixture::RomFixture;
use rom_import::Encoding;

use super::{
    locate, Root, Shape, Witness, BUTTON_BYTES, MOVE_SELECTOR_TAG, PALETTE_TAG_FIELD, ROOTS,
    SHEET_SIZE_FIELD, SHEET_TAG_FIELD,
};
use crate::extract::scope::summary_screen_graphics_ids;
use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::plan::{ReportLine, Resolution, SymbolExpectation};
use crate::gen_rom_profile::tests::{distinctive_tiles, lz77_literals, with_context};

/// Where root `n` sits: far apart, so no stream overlaps another.
const FIRST_ROOT: usize = 0x10_0000;
const ROOT_STRIDE: usize = 0x4000;
/// The sprite records.
const RECORDS: usize = 0x30_0000;
/// Copies of a root placed below every real root.
const STRAY: usize = 0x0A_0000;
/// Longest stored root (the tile sheet's stream) fits well inside a stride.
const BLANK: usize = 0x3000;

const TILES_SHEET: usize = 0;
const MOVE_SELECT_SHEET: usize = 1;
const A_BUTTON: usize = 2;
const B_BUTTON: usize = 3;
const TILES_PALETTE: usize = 4;
const MOVE_SELECT_PALETTE: usize = 5;
const MARKINGS_PALETTE: usize = 6;
const PAGE_INFO: usize = 7;
const EFFECT_CANCEL: usize = 14;
const STATUS_MAP: usize = 15;

fn bus(offset: usize) -> u32 {
    0x0800_0000 + u32::try_from(offset).expect("a fixture offset fits a bus address")
}

/// Where root `n` sits; the second button follows the first, as in the array.
fn offset_of(index: usize) -> usize {
    if index == B_BUTTON {
        offset_of(A_BUTTON) + BUTTON_BYTES as usize
    } else {
        FIRST_ROOT + index * ROOT_STRIDE
    }
}

/// The decompressed bytes of `ROOTS[index]`, distinct from every other root's.
fn content(index: usize) -> Vec<u8> {
    let salt = u8::try_from(index * 15 + 1).expect("sixteen roots");
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
            .map(|n| {
                u8::try_from((n * n * 7 + n * 13 + u32::from(salt) * 29) % 251).expect("modulo 251")
            })
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

/// A `{data, size, tag}` sprite-sheet record.
fn sheet_record(data: u32, size: u16, tag: u16) -> Vec<u8> {
    let mut row = data.to_le_bytes().to_vec();
    row.extend_from_slice(&size.to_le_bytes());
    row.extend_from_slice(&tag.to_le_bytes());
    row
}

/// A `{data, tag}` sprite-palette record.
fn palette_record(data: u32, tag: u16) -> Vec<u8> {
    let mut row = data.to_le_bytes().to_vec();
    row.extend_from_slice(&tag.to_le_bytes());
    row.extend_from_slice(&[0, 0]);
    row
}

/// The record upstream writes for `index`, naming `target`.
fn record_for(index: usize, target: usize) -> Vec<u8> {
    match ROOTS[index].witness {
        Witness::SpriteSheet { size, tag } => sheet_record(bus(target), size, tag),
        Witness::SpritePalette { tag } => palette_record(bus(target), tag),
        Witness::None | Witness::ButtonRow(_) => panic!("root {index} has no record"),
    }
}

/// Every root in the ROM, with no sprite record yet.
fn bare() -> RomFixture {
    let mut fixture = RomFixture::new().emerald_header();
    for (index, root) in ROOTS.iter().enumerate() {
        fixture = fixture.write(offset_of(index), &stored(root, &content(index)));
    }
    fixture
}

/// Where the record for root `index` sits.
const fn record_slot(index: usize) -> usize {
    RECORDS + index * 0x100
}

/// Every root, and the records naming the move-select sheet and palette.
fn fixture() -> RomFixture {
    [MOVE_SELECT_SHEET, MOVE_SELECT_PALETTE]
        .into_iter()
        .fold(bare(), |fixture, index| {
            fixture.write(record_slot(index), &record_for(index, offset_of(index)))
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
    let roots: Vec<String> = ROOTS.iter().map(|root| root.id.to_owned()).collect();
    assert_eq!(roots, summary_screen_graphics_ids());
}

#[test]
fn all_sixteen_roots_are_found_with_their_production_shapes() {
    let rom = fixture().finish();
    with_context("summary-screen-found", &rom, entries(), |ctx| {
        let mut report = Vec::new();
        let plan = locate(ctx, &mut report).expect("located");
        assert_eq!(report.len(), 16);
        assert_eq!(
            (plan.images.len(), plan.palettes.len(), plan.tilemaps.len()),
            (4, 3, 9)
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
        let shapes: Vec<(u32, u32, u32, Encoding)> = plan
            .images
            .iter()
            .map(|i| (i.width, i.height, i.tile_count, i.encoding))
            .collect();
        assert_eq!(
            shapes,
            [
                (128, 120, 240, Encoding::Lz77),
                (16, 128, 32, Encoding::Lz77),
                (16, 16, 4, Encoding::Raw),
                (16, 16, 4, Encoding::Raw),
            ]
        );
        let colors: Vec<(u16, Encoding)> = plan
            .palettes
            .iter()
            .map(|p| (p.color_count, p.encoding))
            .collect();
        assert_eq!(
            colors,
            [
                (128, Encoding::Lz77),
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
                (2048, Encoding::Lz77),
                (2048, Encoding::Lz77),
                (2048, Encoding::Lz77),
                (2048, Encoding::Lz77),
                (160, Encoding::Raw),
                (160, Encoding::Raw),
                (128, Encoding::Raw),
                (40, Encoding::Raw),
            ]
        );
        assert_eq!(
            plan.tilemaps[STATUS_MAP - 7].addr,
            bus(offset_of(STATUS_MAP))
        );
    });
}

#[test]
fn the_report_names_what_a_map_can_and_cannot_confirm() {
    let rom = fixture().finish();
    with_context("summary-screen-symbols", &rom, entries(), |ctx| {
        let mut report = Vec::new();
        locate(ctx, &mut report).expect("located");
        let global = |id| SymbolExpectation::Exact(String::from(id));
        let local = |id| SymbolExpectation::Local(String::from(id));
        for (id, expected) in [
            (
                "summary-screen-graphics/image/tiles",
                global("gSummaryScreen_Gfx"),
            ),
            (
                "summary-screen-graphics/palette/tiles",
                global("gSummaryScreen_Pal"),
            ),
            (
                "summary-screen-graphics/image/move_select",
                global("gSummaryMoveSelect_Gfx"),
            ),
            (
                "summary-screen-graphics/palette/move_select",
                global("gSummaryMoveSelect_Pal"),
            ),
            (
                "summary-screen-graphics/raw/page_info",
                global("gSummaryPage_Info_Tilemap"),
            ),
            (
                "summary-screen-graphics/raw/page_info_egg",
                global("gSummaryPage_InfoEgg_Tilemap"),
            ),
            (
                "summary-screen-graphics/raw/page_skills",
                global("gSummaryPage_Skills_Tilemap"),
            ),
            (
                "summary-screen-graphics/raw/page_battle_moves",
                global("gSummaryPage_BattleMoves_Tilemap"),
            ),
            (
                "summary-screen-graphics/raw/page_contest_moves",
                global("gSummaryPage_ContestMoves_Tilemap"),
            ),
            (
                "summary-screen-graphics/raw/effect_battle",
                global("gSummaryScreen_MoveEffect_Battle_Tilemap"),
            ),
            (
                "summary-screen-graphics/raw/effect_contest",
                global("gSummaryScreen_MoveEffect_Contest_Tilemap"),
            ),
            (
                "summary-screen-graphics/raw/effect_cancel",
                global("gSummaryScreen_MoveEffect_Cancel_Tilemap"),
            ),
            (
                "summary-screen-graphics/raw/status_tilemap",
                local("sStatusTilemap"),
            ),
            (
                "summary-screen-graphics/palette/markings",
                local("sMarkings_Pal"),
            ),
            (
                "summary-screen-graphics/image/a_button",
                local("sButtons_Gfx"),
            ),
            (
                "summary-screen-graphics/image/b_button",
                SymbolExpectation::Interior,
            ),
        ] {
            assert_eq!(line(&report, id).symbol, expected, "{id}");
        }
        assert_eq!(
            line(&report, "summary-screen-graphics/image/tiles").len,
            240 * 32
        );
        assert_eq!(
            line(&report, "summary-screen-graphics/palette/tiles").len,
            128 * 2
        );
        assert_eq!(
            line(&report, "summary-screen-graphics/raw/effect_cancel").len,
            128
        );
    });
}

#[test]
fn records_and_the_button_array_settle_their_roots_and_the_rest_are_unique() {
    let rom = fixture().finish();
    with_context("summary-screen-resolution", &rom, entries(), |ctx| {
        let mut report = Vec::new();
        locate(ctx, &mut report).expect("located");
        for line in &report {
            let expected = if line.id.contains("move_select") || line.id.contains("_button") {
                Resolution::StructDerived
            } else {
                Resolution::UniqueSignature
            };
            assert_eq!(line.resolution, expected, "{}", line.id);
        }
    });
}

#[test]
fn the_buttons_are_the_two_rows_of_one_array() {
    let rom = fixture().finish();
    with_context("summary-screen-buttons", &rom, entries(), |ctx| {
        let mut report = Vec::new();
        let plan = locate(ctx, &mut report).expect("located");
        let [.., a, b] = &plan.images[..] else {
            panic!("four images");
        };
        assert_eq!(
            (a.id.as_str(), b.id.as_str()),
            (
                "summary-screen-graphics/image/a_button",
                "summary-screen-graphics/image/b_button"
            )
        );
        assert_eq!(b.addr, a.addr + BUTTON_BYTES);
        let note = line(&report, &b.id).note.as_deref().unwrap();
        assert!(note.contains("row 1 of sButtons_Gfx"), "{note}");
    });
}

#[test]
fn a_button_stored_alone_elsewhere_does_not_move_the_array() {
    // Lone copies of either button, below the array, are not the array.
    let rom = fixture()
        .write(STRAY, &content(A_BUTTON))
        .write(STRAY + ROOT_STRIDE, &content(B_BUTTON))
        .write(STRAY + 2 * ROOT_STRIDE, &content(B_BUTTON))
        .write(
            STRAY + 2 * ROOT_STRIDE + BUTTON_BYTES as usize,
            &content(A_BUTTON),
        )
        .finish();
    with_context("summary-screen-lone-button", &rom, entries(), |ctx| {
        let mut report = Vec::new();
        let plan = locate(ctx, &mut report).expect("located");
        let [.., a, b] = &plan.images[..] else {
            panic!("four images");
        };
        assert_eq!(a.addr, bus(offset_of(A_BUTTON)));
        assert_eq!(b.addr, bus(offset_of(B_BUTTON)));
        assert_eq!(line(&report, &a.id).resolution, Resolution::StructDerived);
    });
}

#[test]
fn a_swapped_button_array_is_not_the_array() {
    // B followed by A is a different 256 bytes: the planted A+B array is the
    // only one, wherever the lone rows are.
    let rom = fixture()
        .write(STRAY, &[content(B_BUTTON), content(A_BUTTON)].concat())
        .finish();
    with_context("summary-screen-swapped-buttons", &rom, entries(), |ctx| {
        let plan = locate(ctx, &mut Vec::new()).expect("located");
        assert_eq!(plan.images[2].addr, bus(offset_of(A_BUTTON)));
    });
}

#[test]
fn a_button_array_stored_twice_is_an_arbitrary_copy_at_the_lowest_address() {
    let rom = fixture()
        .write(STRAY, &[content(A_BUTTON), content(B_BUTTON)].concat())
        .finish();
    with_context("summary-screen-two-arrays", &rom, entries(), |ctx| {
        let mut report = Vec::new();
        let plan = locate(ctx, &mut report).expect("located");
        assert_eq!(plan.images[2].addr, bus(STRAY));
        assert_eq!(plan.images[3].addr, bus(STRAY) + BUTTON_BYTES);
        for id in [plan.images[2].id.as_str(), plan.images[3].id.as_str()] {
            let found = line(&report, id);
            assert_eq!(
                found.resolution,
                Resolution::ArbitraryAmongIdentical,
                "{id}"
            );
            assert!(found.note.as_deref().unwrap().contains("2 addresses"));
        }
    });
}

#[test]
fn a_witnessed_root_takes_the_copy_its_record_names_over_sixteen_lower_ones() {
    for index in [MOVE_SELECT_SHEET, MOVE_SELECT_PALETTE] {
        let mut fixture = fixture();
        for copy in 0..17usize {
            fixture = fixture.write(
                STRAY + copy * ROOT_STRIDE,
                &stored(&ROOTS[index], &content(index)),
            );
        }
        // The record names the real copy, which sits above all of them.
        let rom = fixture
            .write(record_slot(index), &record_for(index, offset_of(index)))
            .finish();
        with_context("summary-screen-witness", &rom, entries(), |ctx| {
            let mut report = Vec::new();
            let plan = locate(ctx, &mut report).expect("located");
            let addr = plan
                .images
                .iter()
                .map(|i| (&i.id, i.addr))
                .chain(plan.palettes.iter().map(|p| (&p.id, p.addr)))
                .find(|(id, _)| *id == ROOTS[index].id)
                .map(|(_, addr)| addr)
                .unwrap();
            assert_eq!(addr, bus(offset_of(index)), "{}", ROOTS[index].id);
            let found = line(&report, ROOTS[index].id);
            assert_eq!(found.resolution, Resolution::StructDerived);
            assert!(found.note.as_deref().unwrap().contains("18 addresses"));
        });
    }
}

#[test]
fn a_record_naming_the_lowest_copy_leaves_the_higher_one_unchosen() {
    for index in [MOVE_SELECT_SHEET, MOVE_SELECT_PALETTE] {
        let rom = fixture()
            .write(STRAY, &stored(&ROOTS[index], &content(index)))
            .write(record_slot(index), &record_for(index, STRAY))
            .finish();
        with_context("summary-screen-low-copy", &rom, entries(), |ctx| {
            let mut report = Vec::new();
            locate(ctx, &mut report).expect("located");
            assert_eq!(line(&report, ROOTS[index].id).addr, bus(STRAY));
        });
    }
}

#[test]
fn a_pointer_without_the_whole_record_names_nothing() {
    let target = bus(offset_of(MOVE_SELECT_SHEET));
    // The size and tag follow the pointer, but not at the offsets of a
    // sprite-sheet record.
    let mut shifted = target.to_le_bytes().to_vec();
    shifted.extend_from_slice(&[0, 0]);
    shifted.extend_from_slice(&0x400u16.to_le_bytes());
    shifted.extend_from_slice(&MOVE_SELECTOR_TAG.to_le_bytes());
    for (label, decoy) in [
        ("no record", vec![0; 8]),
        (
            "wrong size",
            sheet_record(target, 0x400 + 32, MOVE_SELECTOR_TAG),
        ),
        (
            "wrong tag",
            sheet_record(target, 0x400, MOVE_SELECTOR_TAG + 2),
        ),
        ("shifted fields", shifted),
    ] {
        let rom = fixture()
            .write(record_slot(MOVE_SELECT_SHEET), &decoy)
            .finish();
        with_context("summary-screen-decoy", &rom, entries(), |ctx| {
            let err = locate(ctx, &mut Vec::new()).unwrap_err();
            assert!(
                matches!(&err, GenRomProfileError::StructMismatch { id, .. }
                    if id == "summary-screen-graphics/image/move_select"),
                "{label}: {err}"
            );
        });
    }
}

#[test]
fn a_palette_pointer_without_the_tag_names_nothing() {
    let target = bus(offset_of(MOVE_SELECT_PALETTE));
    for (label, decoy) in [
        ("no record", vec![0; 8]),
        ("wrong tag", palette_record(target, MOVE_SELECTOR_TAG + 2)),
        // A sheet record puts the tag two bytes later than a palette record.
        (
            "sheet layout",
            sheet_record(target, 0x400, MOVE_SELECTOR_TAG),
        ),
    ] {
        let rom = fixture()
            .write(record_slot(MOVE_SELECT_PALETTE), &decoy)
            .finish();
        with_context("summary-screen-palette-decoy", &rom, entries(), |ctx| {
            let err = locate(ctx, &mut Vec::new()).unwrap_err();
            assert!(
                matches!(&err, GenRomProfileError::StructMismatch { id, .. }
                    if id == "summary-screen-graphics/palette/move_select"),
                "{label}: {err}"
            );
        });
    }
}

#[test]
fn an_unaligned_record_names_nothing() {
    let rom = fixture()
        .write(record_slot(MOVE_SELECT_SHEET), &[0; 8])
        .write(
            RECORDS + 0x802,
            &record_for(MOVE_SELECT_SHEET, offset_of(MOVE_SELECT_SHEET)),
        )
        .finish();
    with_context("summary-screen-unaligned", &rom, entries(), |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert_struct_mismatch(&err, "summary-screen-graphics/image/move_select");
    });
}

#[test]
fn two_copies_both_named_by_a_record_settle_nothing() {
    let rom = fixture()
        .write(
            STRAY,
            &stored(&ROOTS[MOVE_SELECT_SHEET], &content(MOVE_SELECT_SHEET)),
        )
        .write(RECORDS + 0x800, &record_for(MOVE_SELECT_SHEET, STRAY))
        .finish();
    with_context("summary-screen-two-named", &rom, entries(), |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert_struct_mismatch(&err, "summary-screen-graphics/image/move_select");
    });
}

#[test]
fn the_record_fields_are_the_ones_upstream_writes() {
    assert_eq!(MOVE_SELECTOR_TAG, 30000);
    assert!(matches!(
        ROOTS[MOVE_SELECT_SHEET].witness,
        Witness::SpriteSheet {
            size: 0x400,
            tag: 30000
        }
    ));
    assert!(matches!(
        ROOTS[MOVE_SELECT_PALETTE].witness,
        Witness::SpritePalette { tag: 30000 }
    ));
    assert_eq!(
        (SHEET_SIZE_FIELD, SHEET_TAG_FIELD, PALETTE_TAG_FIELD),
        (4, 6, 4)
    );
    assert_eq!(BUTTON_BYTES, 128);
}

#[test]
fn a_root_without_a_record_stored_twice_is_an_arbitrary_copy_at_the_lowest_address() {
    for index in [
        TILES_SHEET,
        TILES_PALETTE,
        MARKINGS_PALETTE,
        PAGE_INFO,
        EFFECT_CANCEL,
        STATUS_MAP,
    ] {
        let rom = fixture()
            .write(STRAY, &stored(&ROOTS[index], &content(index)))
            .finish();
        with_context("summary-screen-identical", &rom, entries(), |ctx| {
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
    let mut near = content(STATUS_MAP);
    *near.last_mut().unwrap() ^= 1;
    let rom = fixture().write(STRAY, &near).finish();
    with_context("summary-screen-near", &rom, entries(), |ctx| {
        let mut report = Vec::new();
        locate(ctx, &mut report).expect("located");
        let found = line(&report, ROOTS[STATUS_MAP].id);
        assert_eq!(found.addr, bus(offset_of(STATUS_MAP)));
        assert_eq!(found.resolution, Resolution::UniqueSignature);
    });
}

#[test]
fn art_the_rom_lacks_is_not_found() {
    for (index, root) in ROOTS.iter().enumerate() {
        if index == B_BUTTON {
            // The second button is a row of the first's array: blanking the
            // array is blanking the first button.
            continue;
        }
        let rom = fixture()
            .write(offset_of(index), &vec![0xFF; BLANK])
            .finish();
        with_context("summary-screen-missing-art", &rom, entries(), |ctx| {
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
fn a_second_button_the_array_lacks_is_not_found() {
    let rom = fixture()
        .write(offset_of(B_BUTTON), &vec![0xFF; BUTTON_BYTES as usize])
        .finish();
    with_context("summary-screen-no-b-button", &rom, entries(), |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::NotFound { id }
                if id == "summary-screen-graphics/image/a_button"),
            "{err}"
        );
    });
}

#[test]
fn a_root_stored_in_the_wrong_encoding_is_not_accepted() {
    // The raw map's bytes, stored as a stream, and the stream's, stored raw.
    for index in [STATUS_MAP, PAGE_INFO] {
        let root = &ROOTS[index];
        let flipped = match root.encoding {
            Encoding::Raw => lz77_literals(&content(index)),
            Encoding::Lz77 => content(index),
        };
        let rom = fixture()
            .write(offset_of(index), &vec![0xFF; BLANK])
            .write(offset_of(index), &flipped)
            .finish();
        with_context("summary-screen-wrong-encoding", &rom, entries(), |ctx| {
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
        // The 62-tile cut of the party menu's background is not this sheet.
        (
            TILES_SHEET,
            image_entry_from_tiles(
                ROOTS[TILES_SHEET].id.to_owned(),
                &distinctive_tiles(62),
                4,
                8,
                496,
                None,
            )
            .unwrap(),
        ),
        // One bank, not the eight the screen loads.
        (
            TILES_PALETTE,
            palette_entry(ROOTS[TILES_PALETTE].id.to_owned(), &[1; 16]).unwrap(),
        ),
        // The 10x7 window, not the 80 entries the declaration stores.
        (12, raw_entry(ROOTS[12].id.to_owned(), vec![1; 140])),
        (
            EFFECT_CANCEL,
            raw_entry(ROOTS[EFFECT_CANCEL].id.to_owned(), vec![1; 120]),
        ),
        (
            A_BUTTON,
            image_entry_from_tiles(
                ROOTS[A_BUTTON].id.to_owned(),
                &distinctive_tiles(2),
                4,
                8,
                16,
                None,
            )
            .unwrap(),
        ),
    ];
    let rom = fixture().finish();
    for (index, replacement) in wrong {
        let mut pack = entries();
        pack[index] = replacement;
        with_context("summary-screen-shape", &rom, pack, |ctx| {
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
        with_context("summary-screen-missing-entry", &rom, pack, |ctx| {
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
    pack.push(raw_entry(
        "summary-screen-graphics/raw/extra".to_owned(),
        vec![1; 8],
    ));
    with_context("summary-screen-extra-entry", &rom, pack, |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert_struct_mismatch(&err, "summary-screen-graphics/raw/extra");
    });
}
