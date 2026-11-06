//! Interface-domain tests over a synthetic ROM.
//!
//! The down-arrow sheet goes through the shared image reader, covered
//! there; these cases check that the domain writes it as an image entry with
//! no palette beside it, that [`Roots::images`] sees it exactly once, and
//! that a root addressed past the image fails.

use pack_format::{parse_directory, EntryKind, PackWriter};

use super::write;
use crate::error::ImportError;
use crate::fixture::RomFixture;
use crate::reader::{GbaPtr, ROM_BASE};
use crate::rom::Rom;
use crate::roots::{Encoding, ImageRoot, InterfaceRoots, Roots};

/// Where the fixture plants the sheet.
const SHEET: usize = 0x1000;
/// The address of that sheet.
const SHEET_AT: GbaPtr = GbaPtr::at(ROM_BASE + 0x1000);
/// An address whose tile data runs off the end of the image.
const NEAR_THE_END: GbaPtr = GbaPtr::at(ROM_BASE + 0x00FF_FFF0);
/// Tiles in the sheet: one column of six.
const TILES: usize = 6;

static SHEETS: [ImageRoot; 1] = [ImageRoot {
    id: "interface/image/test",
    addr: SHEET_AT,
    encoding: Encoding::Raw,
    rom_bit_depth: 4,
    pack_bit_depth: 4,
    width: 8,
    height: 48,
    metatile_width: 1,
    metatile_height: 6,
    tile_count: 6,
}];

static PAST_THE_ROM: [ImageRoot; 1] = [ImageRoot {
    addr: NEAR_THE_END,
    ..SHEETS[0]
}];

/// Tile `k` has pixel 0 of its first row at `k + 1` and pixel 1 at `k + 2`,
/// and no other pixel set.
fn sheet_tiles() -> Vec<u8> {
    let mut tiles = vec![0u8; TILES * 32];
    for tile in 0..TILES {
        let first = u8::try_from(tile + 1).expect("a nibble");
        tiles[tile * 32] = first | ((first + 1) << 4);
    }
    tiles
}

fn rom() -> Rom {
    let bytes = RomFixture::new()
        .emerald_header()
        .write(SHEET, &sheet_tiles())
        .finish();
    Rom::from_bytes(bytes).expect("the fixture header is valid")
}

fn roots(images: &'static [ImageRoot]) -> Roots {
    Roots {
        interface: InterfaceRoots {
            images,
            palettes: &[],
        },
        ..Roots::NONE
    }
}

#[test]
fn the_down_arrow_sheet_is_written_as_a_bare_image_entry() {
    let mut writer = PackWriter::new();
    write(&rom(), &roots(&SHEETS), &mut writer).expect("a well-formed root");
    let pack = writer.finish().expect("no duplicate ids");
    let entries = parse_directory(&pack).expect("a readable pack");

    assert_eq!(entries.len(), 1, "no palette entry rides with the sheet");
    assert_eq!(entries[0].id, "interface/image/test");
    assert_eq!(
        entries[0].kind,
        EntryKind::Image {
            width: 8,
            height: 48,
            bit_depth: 4
        }
    );
    let mut expected = vec![0u8; 8 * 48];
    for tile in 0..TILES {
        expected[tile * 8 * 8] = u8::try_from(tile + 1).expect("a nibble");
        expected[tile * 8 * 8 + 1] = u8::try_from(tile + 2).expect("a nibble");
    }
    assert_eq!(&pack[entries[0].offset..][..entries[0].length], expected);
}

#[test]
fn the_aggregate_image_iterator_names_the_interface_sheet_once() {
    let roots = roots(&SHEETS);
    let ids: Vec<&str> = roots.images().map(|root| root.id).collect();
    assert_eq!(ids, ["interface/image/test"]);
    assert_eq!(Roots::NONE.images().count(), 0);
}

#[test]
fn a_sheet_past_the_image_is_truncated() {
    let mut writer = PackWriter::new();
    let err = write(&rom(), &roots(&PAST_THE_ROM), &mut writer).unwrap_err();
    assert!(matches!(err, ImportError::Truncated { .. }), "{err}");
}
