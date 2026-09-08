//! Text-window reader tests over a synthetic ROM.
//!
//! The reader's own logic is the check of `sTextWindowPalettes`, so the
//! cases are a table that agrees with the profile, one whose first bank is
//! not the message box's colours, one whose `text_pal*` banks are not
//! where the profile says, a profile missing one of the five, and a table
//! addressed past the image.

use pack_format::{parse_directory, DirectoryEntry, PackWriter};

use super::{write, BANK_BYTES};
use crate::error::ImportError;
use crate::fixture::RomFixture;
use crate::reader::{GbaPtr, ROM_BASE};
use crate::rom::Rom;
use crate::roots::{Encoding, ImageRoot, PaletteRoot, Roots, TextWindowRoots};

/// Where the fixture's `sTextWindowPalettes` sits: five banks.
const TABLE: usize = 0x1000;
/// The address of that table.
const TABLE_AT: GbaPtr = GbaPtr::at(ROM_BASE + 0x1000);
/// Where the message box's own palette copy sits.
const MESSAGE_BOX: usize = 0x2000;
/// The address of that copy.
const MESSAGE_BOX_AT: GbaPtr = GbaPtr::at(ROM_BASE + 0x2000);
/// Where a frame's tiles sit.
const FRAME: usize = 0x3000;
/// The address of those tiles.
const FRAME_AT: GbaPtr = GbaPtr::at(ROM_BASE + 0x3000);
/// A table whose first bank is not the message box's.
const WRONG_TABLE: usize = 0x4000;
/// The address of that table.
const WRONG_TABLE_AT: GbaPtr = GbaPtr::at(ROM_BASE + 0x4000);
/// An address inside the cartridge window but past a 16 MiB image.
const PAST_THE_ROM: GbaPtr = GbaPtr::at(ROM_BASE + 0x00FF_FFF0);

/// Bank `n` of the fixture table, 32 bytes each.
const fn bank_at(n: u32) -> GbaPtr {
    GbaPtr::at(ROM_BASE + 0x1000 + n * 32)
}

/// One bank whose every colour is `fill`.
fn bank(fill: u8) -> [u8; BANK_BYTES] {
    [fill; BANK_BYTES]
}

static IMAGES: [ImageRoot; 1] = [ImageRoot {
    id: "text-window/image/1",
    addr: FRAME_AT,
    encoding: Encoding::Raw,
    rom_bit_depth: 4,
    pack_bit_depth: 4,
    width: 8,
    height: 8,
    metatile_width: 1,
    metatile_height: 1,
    tile_count: 1,
}];

const fn pal(id: &'static str, addr: GbaPtr) -> PaletteRoot {
    PaletteRoot {
        id,
        addr,
        color_count: 16,
    }
}

static PALETTES: [PaletteRoot; 5] = [
    pal("text-window/palette/message_box", MESSAGE_BOX_AT),
    pal("text-window/palette/text_pal1", bank_at(1)),
    pal("text-window/palette/text_pal2", bank_at(2)),
    pal("text-window/palette/text_pal3", bank_at(3)),
    pal("text-window/palette/text_pal4", bank_at(4)),
];

/// `text_pal3` recorded one bank off.
static SHIFTED: [PaletteRoot; 5] = [
    PALETTES[0],
    PALETTES[1],
    PALETTES[2],
    pal("text-window/palette/text_pal3", bank_at(4)),
    PALETTES[4],
];

/// `text_pal2` missing.
static MISSING: [PaletteRoot; 4] = [PALETTES[0], PALETTES[1], PALETTES[3], PALETTES[4]];

fn rom() -> Rom {
    let mut table = bank(0x11).to_vec();
    for fill in [0x22, 0x33, 0x44, 0x55] {
        table.extend_from_slice(&bank(fill));
    }
    let bytes = RomFixture::new()
        .emerald_header()
        .write(TABLE, &table)
        .write(MESSAGE_BOX, &bank(0x11))
        .write(FRAME, &[0x10, 0x32, 0x54, 0x76])
        .write(WRONG_TABLE, &bank(0x99))
        .finish();
    Rom::from_bytes(bytes).expect("the fixture header is valid")
}

fn roots(table: GbaPtr, palettes: &'static [PaletteRoot]) -> Roots {
    Roots {
        text_window: TextWindowRoots {
            images: &IMAGES,
            palettes,
            window_palettes: table,
        },
        ..Roots::NONE
    }
}

/// Run the domain and hand back the sorted directory it wrote.
fn run(roots: &Roots) -> Result<Vec<DirectoryEntry>, ImportError> {
    let rom = rom();
    let mut writer = PackWriter::new();
    write(&rom, roots, &mut writer)?;
    let pack = writer.finish()?;
    Ok(parse_directory(&pack).expect("a readable pack"))
}

fn mismatch(err: &ImportError, field: &str) -> bool {
    matches!(
        err,
        ImportError::StructMismatch {
            root: "sTextWindowPalettes",
            field: got,
        } if *got == field
    )
}

#[test]
fn an_agreeing_table_writes_every_root() {
    let entries = run(&roots(TABLE_AT, &PALETTES)).expect("a well-formed table");
    assert_eq!(entries.len(), 6);
    assert_eq!(entries[0].id, "text-window/image/1");
    assert_eq!(entries[1].id, "text-window/palette/message_box");
}

#[test]
fn a_first_bank_that_is_not_the_message_box_is_refused() {
    let err = run(&roots(WRONG_TABLE_AT, &PALETTES)).unwrap_err();
    assert!(mismatch(&err, "message_box"), "{err}");
}

#[test]
fn a_window_bank_recorded_elsewhere_is_refused() {
    let err = run(&roots(TABLE_AT, &SHIFTED)).unwrap_err();
    assert!(mismatch(&err, "text-window/palette/text_pal3"), "{err}");
}

#[test]
fn a_profile_missing_a_window_bank_is_refused() {
    let err = run(&roots(TABLE_AT, &MISSING)).unwrap_err();
    assert!(mismatch(&err, "text-window/palette/text_pal2"), "{err}");
}

#[test]
fn a_table_past_the_image_is_truncated() {
    let err = run(&roots(PAST_THE_ROM, &PALETTES)).unwrap_err();
    assert!(matches!(err, ImportError::Truncated { .. }), "{err}");
}

#[test]
fn a_profile_with_no_palettes_reads_no_table() {
    let entries = run(&roots(PAST_THE_ROM, &[])).expect("no table to check");
    assert_eq!(entries.len(), 1);
}
