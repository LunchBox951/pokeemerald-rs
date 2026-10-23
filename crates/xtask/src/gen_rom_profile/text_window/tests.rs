//! The text-window palette table is accepted only in the importer's order.

use std::fmt::Write as _;

use pack_format::{image_entry_from_tiles, palette_entry, PackEntry};
use rom_import::fixture::RomFixture;
use rom_import::Encoding;

use super::{locate, locate_window_palettes, DUPLICATED_PALETTE, WINDOW_BANKS};
use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::plan::{PalettePlan, ReportLine, SymbolExpectation};
use crate::gen_rom_profile::tests::with_context;

const TABLE_OFFSET: usize = 0x50_0000;
const TABLE_ADDR: u32 = 0x0850_0000;
const BANK_BYTES: u32 = 32;
const BANK_LEN: usize = 32;

fn bank_bytes(seed: u8) -> Vec<u8> {
    (0..32u8).map(|byte| byte ^ seed).collect()
}

/// A ROM holding the message box's bank followed by four distinct banks.
fn table_rom() -> Vec<u8> {
    let mut fixture = RomFixture::new().emerald_header();
    for bank in 0..5u8 {
        let offset = TABLE_OFFSET + usize::from(bank) * BANK_LEN;
        let seed = if bank == 0 {
            0
        } else {
            bank.wrapping_mul(0x11)
        };
        fixture = fixture.write(offset, &bank_bytes(seed));
    }
    fixture.finish()
}

/// Plans placing `text_pal1..4` at the given table bank indices.
fn plans(banks: [u32; 4]) -> Vec<PalettePlan> {
    let message_box = PalettePlan {
        id: DUPLICATED_PALETTE.to_owned(),
        addr: TABLE_ADDR,
        color_count: 16,
        encoding: Encoding::Raw,
    };
    std::iter::once(message_box)
        .chain(
            WINDOW_BANKS
                .iter()
                .zip(banks)
                .map(|(id, bank)| PalettePlan {
                    id: (*id).to_owned(),
                    addr: TABLE_ADDR + bank * BANK_BYTES,
                    color_count: 16,
                    encoding: Encoding::Raw,
                }),
        )
        .collect()
}

fn locate_table(name: &str, banks: [u32; 4]) -> Result<u32, GenRomProfileError> {
    with_context(name, &table_rom(), Vec::new(), |ctx| {
        locate_window_palettes(ctx, &plans(banks), BANK_BYTES, &bank_bytes(0))
    })
}

#[test]
fn a_gnu_ld_map_that_omits_the_static_window_table_does_not_reject_it() {
    let mut lines = vec![super::window_palettes_line(TABLE_ADDR, BANK_BYTES)];
    crate::gen_rom_profile::tests::assert_survives_ld_map("tw-static-map", TABLE_ADDR, &mut lines);
}

#[test]
fn banks_in_table_order_derive_the_table() {
    assert_eq!(
        locate_table("tw-ordered", [1, 2, 3, 4]).expect("ordered"),
        TABLE_ADDR
    );
}

#[test]
fn swapped_second_and_third_banks_are_refused_by_name() {
    match locate_table("tw-swapped", [1, 3, 2, 4]).unwrap_err() {
        GenRomProfileError::StructMismatch { id, .. } => {
            assert_eq!(id, "text-window/palette/text_pal2");
        }
        other => panic!("expected StructMismatch, got {other}"),
    }
}

const FRAME_COUNT: u32 = 20;
const IMAGE_BASE: usize = 0x10_0000;
const FRAME_PALETTE_BASE: usize = 0x20_0000;
const MESSAGE_BOX_PALETTE: usize = 0x30_0000;
const MESSAGE_BOX_GFX: usize = MESSAGE_BOX_PALETTE + 32;
const WINDOW_TABLE: usize = 0x31_0000;

fn rom_addr(offset: usize) -> u32 {
    0x0800_0000 + u32::try_from(offset).expect("small")
}

fn tile_bytes(seed: u8) -> Vec<u8> {
    (0..64u8)
        .map(|byte| byte.wrapping_mul(5).wrapping_add(seed.wrapping_mul(31)))
        .collect()
}

fn palette_colors(seed: u16) -> Vec<u16> {
    (0..16u16)
        .map(|index| (index * 0x0111 + seed * 0x0007) & 0x7FFF)
        .collect()
}

fn palette_payload(seed: u16) -> Vec<u8> {
    palette_colors(seed)
        .iter()
        .flat_map(|colour| colour.to_le_bytes())
        .collect()
}

fn image(id: &str, seed: u8) -> PackEntry {
    image_entry_from_tiles(id.to_owned(), &tile_bytes(seed), 4, 16, 8, None).expect("image entry")
}

fn palette(id: &str, seed: u16) -> PackEntry {
    palette_entry(id.to_owned(), &palette_colors(seed)).expect("palette entry")
}

/// The whole text-window domain in a synthetic ROM: frames 1..=20 and their
/// palettes, the message box (palette immediately before its tiles), and the
/// `sTextWindowPalettes` table. Returns the ROM and the matching pack entries.
fn domain() -> (Vec<u8>, Vec<PackEntry>) {
    const MESSAGE_BOX_SEED: u16 = 100;
    let mut fixture = RomFixture::new().emerald_header();
    let mut entries = Vec::new();
    for frame in 1..=FRAME_COUNT {
        let slot = usize::try_from(frame).expect("small");
        let seed = u8::try_from(frame).expect("small");
        fixture = fixture
            .write(IMAGE_BASE + slot * 0x100, &tile_bytes(seed))
            .write(
                FRAME_PALETTE_BASE + slot * 0x40,
                &palette_payload(u16::from(seed)),
            );
        entries.push(image(&format!("text-window/image/{frame}"), seed));
        entries.push(palette(
            &format!("text-window/palette/{frame}"),
            u16::from(seed),
        ));
    }
    let message_box = palette_payload(MESSAGE_BOX_SEED);
    fixture = fixture
        .write(MESSAGE_BOX_PALETTE, &message_box)
        .write(MESSAGE_BOX_GFX, &tile_bytes(50))
        .write(WINDOW_TABLE, &message_box);
    entries.push(image("text-window/image/message_box", 50));
    entries.push(palette("text-window/palette/message_box", MESSAGE_BOX_SEED));
    for (bank, id) in (1u16..).zip(WINDOW_BANKS) {
        let seed = 200 + bank;
        fixture = fixture.write(
            WINDOW_TABLE + usize::from(bank) * 32,
            &palette_payload(seed),
        );
        entries.push(palette(id, seed));
    }
    (fixture.finish(), entries)
}

/// Locate the synthetic domain with the production locator.
fn located_report(name: &str) -> Vec<ReportLine> {
    let (rom, entries) = domain();
    with_context(name, &rom, entries, |ctx| {
        let mut report = Vec::new();
        locate(ctx, &mut report).expect("the synthetic domain locates");
        report
    })
}

fn line<'a>(report: &'a [ReportLine], id: &str) -> &'a ReportLine {
    report
        .iter()
        .find(|line| line.id == id)
        .unwrap_or_else(|| panic!("no report line for {id}"))
}

/// Run `report` through the production cross-check against a map shaped like
/// GNU ld `-Map` output: only the global symbols in `globals` are listed.
fn cross_check_against(
    name: &str,
    globals: &[(&str, u32)],
    report: &mut [ReportLine],
) -> Result<crate::gen_rom_profile::MapCheck, GenRomProfileError> {
    let dir = std::env::temp_dir().join(format!(
        "pokeemerald-rs-gen-rom-profile-{name}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("scratch directory");
    let map_path = dir.join("pokeemerald.map");
    let mut map = String::from(".rodata         0x0000000008000000       0x20\n");
    for (symbol, addr) in globals {
        writeln!(map, "               0x{addr:016x}                {symbol}")
            .expect("string write");
    }
    std::fs::write(&map_path, map).expect("write map");
    let result = crate::gen_rom_profile::cross_check(Some(&map_path), report);
    let _ = std::fs::remove_file(&map_path);
    let _ = std::fs::remove_dir(&dir);
    result
}

fn global_symbols(report: &[ReportLine]) -> Vec<(&'static str, u32)> {
    vec![
        (
            "gTextWindowFrame1_Gfx",
            line(report, "text-window/image/1").addr,
        ),
        (
            "gTextWindowFrame1_Pal",
            line(report, "text-window/palette/1").addr,
        ),
        (
            "gMessageBox_Gfx",
            line(report, "text-window/image/message_box").addr,
        ),
        (
            "gMessageBox_Pal",
            line(report, "text-window/palette/message_box").addr,
        ),
    ]
}

#[test]
fn only_frames_two_to_twenty_are_local_and_frame_one_stays_global() {
    let report = located_report("tw-classification");
    for frame in 2..=FRAME_COUNT {
        for (kind, suffix) in [("image", "Gfx"), ("palette", "Pal")] {
            assert_eq!(
                line(&report, &format!("text-window/{kind}/{frame}")).symbol,
                SymbolExpectation::Local(format!("sTextWindowFrame{frame}_{suffix}")),
                "frame {frame} {kind}"
            );
        }
    }
    assert_eq!(
        line(&report, "text-window/image/1").symbol,
        SymbolExpectation::Contains(vec!["TextWindowFrame1_Gfx".to_owned()])
    );
    assert_eq!(
        line(&report, "text-window/palette/1").symbol,
        SymbolExpectation::Contains(vec!["TextWindowFrame1_Pal".to_owned()])
    );
    assert_eq!(
        line(&report, "text-window/image/message_box").symbol,
        SymbolExpectation::Contains(vec!["gMessageBox_Gfx".to_owned()])
    );
    assert_eq!(
        line(&report, "text-window/palette/message_box").symbol,
        SymbolExpectation::Exact("gMessageBox_Pal".to_owned())
    );
}

#[test]
fn located_frame_roots_survive_a_map_that_omits_locals() {
    let mut report = located_report("tw-survives");
    // Sanity: the located addresses are where the synthetic ROM put them.
    assert_eq!(
        line(&report, "text-window/image/2").addr,
        rom_addr(IMAGE_BASE + 2 * 0x100)
    );
    assert_eq!(
        line(&report, "text-window/palette/2").addr,
        rom_addr(FRAME_PALETTE_BASE + 2 * 0x40)
    );
    let globals = global_symbols(&report);
    let check = cross_check_against("tw-survives-map", &globals, &mut report)
        .expect("a GNU ld map without the static frames must not reject the located roots");
    // 19 frame graphics + 19 frame palettes + sTextWindowPalettes.
    assert_eq!(check.local, 39);
    // Frame 1 and the message box are still checked against the map.
    assert_eq!(check.named, 4);
    assert_eq!(check.skipped, 4);
}

#[test]
fn the_global_frame_and_message_box_still_need_their_map_symbols() {
    for (omitted, id) in [
        ("gTextWindowFrame1_Gfx", "text-window/image/1"),
        ("gTextWindowFrame1_Pal", "text-window/palette/1"),
        ("gMessageBox_Gfx", "text-window/image/message_box"),
        ("gMessageBox_Pal", "text-window/palette/message_box"),
    ] {
        let mut report = located_report("tw-global-required");
        let globals: Vec<_> = global_symbols(&report)
            .into_iter()
            .filter(|(name, _)| *name != omitted)
            .collect();
        match cross_check_against("tw-global-required-map", &globals, &mut report) {
            Err(GenRomProfileError::MapMismatch { id: found, .. }) => assert_eq!(found, id),
            other => panic!("expected a MapMismatch for {id}, got {other:?}"),
        }
    }
}

#[test]
fn a_fifteen_colour_window_bank_is_refused() {
    let mut banks = plans([1, 2, 3, 4]);
    banks[4].color_count = 15;
    let result = with_context("tw-short-count", &table_rom(), Vec::new(), |ctx| {
        locate_window_palettes(ctx, &banks, BANK_BYTES, &bank_bytes(0))
    });
    match result.unwrap_err() {
        GenRomProfileError::StructMismatch { id, .. } => {
            assert_eq!(id, "text-window/palette/text_pal4");
        }
        other => panic!("expected StructMismatch, got {other}"),
    }
}

#[test]
fn only_the_canonical_stems_two_to_twenty_are_static_frames() {
    for frame in 2..=FRAME_COUNT {
        assert!(super::is_static_frame(&frame.to_string()), "frame {frame}");
    }
    for stem in [
        "1",
        "21",
        "0",
        "02",
        "+2",
        "020",
        "message_box",
        "text_pal1",
        "",
    ] {
        assert!(!super::is_static_frame(stem), "stem {stem:?}");
    }
}

#[test]
fn a_short_stride_is_refused_even_when_the_count_says_sixteen() {
    let result = with_context("tw-short-stride", &table_rom(), Vec::new(), |ctx| {
        locate_window_palettes(ctx, &plans([1, 2, 3, 4]), 16, &bank_bytes(0)[..16])
    });
    // The old placement check names `sTextWindowPalettes`; only the bank-shape
    // guard names the first bank it measures.
    match result.unwrap_err() {
        GenRomProfileError::StructMismatch { id, .. } => assert_eq!(id, DUPLICATED_PALETTE),
        other => panic!("expected StructMismatch, got {other}"),
    }
}

#[test]
fn a_short_message_box_bank_is_refused() {
    let mut banks = plans([1, 2, 3, 4]);
    banks[0].color_count = 8;
    let result = with_context("tw-short-message-box", &table_rom(), Vec::new(), |ctx| {
        locate_window_palettes(ctx, &banks, BANK_BYTES, &bank_bytes(0))
    });
    match result.unwrap_err() {
        GenRomProfileError::StructMismatch { id, .. } => assert_eq!(id, DUPLICATED_PALETTE),
        other => panic!("expected StructMismatch, got {other}"),
    }
}
