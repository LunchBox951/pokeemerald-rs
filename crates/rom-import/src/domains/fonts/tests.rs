//! Glyph-sheet reader tests over a synthetic ROM.
//!
//! The decoder's whole job is the `.latfont` walk, so the cases plant a
//! sheet with one marked pixel per quadrant of one glyph and check each
//! lands where the layout says it should. The generator's packer is the
//! other direction of the same walk; the equivalence harness checks the two
//! against a real sheet.

use pack_format::{EntryKind, PackWriter};

use super::{font, write, SHEET_BYTES};
use crate::error::ImportError;
use crate::fixture::RomFixture;
use crate::reader::{GbaPtr, ROM_BASE};
use crate::rom::Rom;
use crate::roots::{FontRoot, Roots};

/// Where the fixture plants the sheet.
const SHEET: usize = 0x1000;
/// The address of that sheet.
const SHEET_AT: GbaPtr = GbaPtr::at(ROM_BASE + 0x1000);
/// An address whose 32 KiB run off the end of the image.
const NEAR_THE_END: GbaPtr = GbaPtr::at(ROM_BASE + 0x00FF_F000);

static FONT: FontRoot = FontRoot {
    id: "font/test/glyphs",
    addr: SHEET_AT,
    len: 32768,
    width: 256,
    height: 512,
    bit_depth: 2,
};

static FONTS: [FontRoot; 1] = [FONT];

/// A sheet that is zero except for glyph (column 1, row 2), whose four
/// tiles each carry one distinct pixel in their first line.
///
/// Tile 0 (top-left): pixel 0 of the line, value 1. Stored in the *second*
/// byte of the pair, since the tool writes the right half first.
/// Tile 1 (top-right): pixel 7, value 2. First byte, lowest pair.
/// Tile 2 (bottom-left): pixel 3, value 3. Second byte, lowest pair.
/// Tile 3 (bottom-right): pixel 4, value 1. First byte, highest pair.
fn marked_sheet() -> Vec<u8> {
    let mut sheet = vec![0u8; SHEET_BYTES];
    // Glyph index in the walk: row * 16 + column; 64 bytes each, 16 per
    // tile, 2 per line.
    let glyph = (2 * 16 + 1) * 64;
    sheet[glyph + 1] = 0b0100_0000;
    sheet[glyph + 16] = 0b0000_0010;
    sheet[glyph + 32 + 1] = 0b0000_0011;
    sheet[glyph + 48] = 0b0100_0000;
    sheet
}

fn rom() -> Rom {
    let bytes = RomFixture::new()
        .emerald_header()
        .write(SHEET, &marked_sheet())
        .finish();
    Rom::from_bytes(bytes).expect("the fixture header is valid")
}

fn roots(fonts: &'static [FontRoot]) -> Roots {
    Roots {
        fonts,
        ..Roots::NONE
    }
}

#[test]
fn the_latfont_walk_is_inverted() {
    let rom = rom();
    let entry = font(&rom.reader(), &FONT).expect("a well-formed root");
    assert_eq!(entry.id, "font/test/glyphs");
    assert_eq!(
        entry.kind,
        EntryKind::Image {
            width: 256,
            height: 512,
            bit_depth: 2,
        }
    );
    let at = |x: usize, y: usize| entry.payload[y * 256 + x];
    // Glyph (1, 2) starts at pixel (16, 32).
    assert_eq!(at(16, 32), 1, "tile 0, pixel 0");
    assert_eq!(at(16 + 8 + 7, 32), 2, "tile 1, pixel 7");
    assert_eq!(at(16 + 3, 32 + 8), 3, "tile 2, pixel 3");
    assert_eq!(at(16 + 8 + 4, 32 + 8), 1, "tile 3, pixel 4");
    assert_eq!(
        entry.payload.iter().filter(|&&p| p != 0).count(),
        4,
        "nothing else is set"
    );
}

#[test]
fn the_domain_writes_one_entry_per_root() {
    let rom = rom();
    let mut writer = PackWriter::new();
    write(&rom, &roots(&FONTS), &mut writer).expect("a well-formed table");
    assert_eq!(writer.len(), 1);
}

#[test]
fn a_root_of_another_shape_is_refused_before_reading() {
    let rom = rom();
    for root in [
        FontRoot { width: 128, ..FONT },
        FontRoot {
            height: 256,
            ..FONT
        },
        FontRoot {
            bit_depth: 4,
            ..FONT
        },
        FontRoot { len: 16384, ..FONT },
        // Even when the bytes would not be there to read.
        FontRoot {
            addr: NEAR_THE_END,
            len: 16384,
            ..FONT
        },
    ] {
        let err = font(&rom.reader(), &root).unwrap_err();
        assert!(
            matches!(
                err,
                ImportError::FontShape {
                    id: "font/test/glyphs",
                    ..
                }
            ),
            "{err}"
        );
        assert!(err.to_string().contains("256x512/2bpp"), "{err}");
    }
}

#[test]
fn a_sheet_past_the_image_is_truncated() {
    let rom = rom();
    let root = FontRoot {
        addr: NEAR_THE_END,
        ..FONT
    };
    let err = font(&rom.reader(), &root).unwrap_err();
    assert!(matches!(err, ImportError::Truncated { .. }), "{err}");
}
