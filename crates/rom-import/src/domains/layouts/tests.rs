//! Map-layout reader tests over synthetic ROMs.
//!
//! The reader's whole job is corroborating `struct MapLayout` and copying
//! two grids, so the cases here are the struct's six fields, the padded-grid
//! allowance, and a grid addressed past the end of the image.

use pack_format::{EntryKind, PackWriter};

use super::write;
use crate::error::ImportError;
use crate::fixture::RomFixture;
use crate::reader::{GbaPtr, ROM_BASE};
use crate::rom::Rom;
use crate::roots::{BlobRoot, Encoding, MapLayoutRoot, Roots};

// Each fixture region is named twice, as a write offset and as the pointer
// the struct carries, so nothing here has to cast between the two.
/// Where the fixture's `struct MapLayout` sits.
const STRUCT: usize = 0x1000;
/// The address of that struct.
const STRUCT_AT: GbaPtr = GbaPtr::at(ROM_BASE + 0x1000);
/// Where the border block sits.
const BORDER: usize = 0x2000;
/// The address of the border block.
const BORDER_AT: GbaPtr = GbaPtr::at(ROM_BASE + 0x2000);
/// Where the metatile grid sits.
const MAP: usize = 0x3000;
/// The address of the metatile grid.
const MAP_AT: GbaPtr = GbaPtr::at(ROM_BASE + 0x3000);
/// The primary tileset the struct points at. No tileset is read here, so
/// only the address has to agree.
const PRIMARY: GbaPtr = GbaPtr::at(ROM_BASE + 0x4000);
/// The secondary tileset the struct points at.
const SECONDARY: GbaPtr = GbaPtr::at(ROM_BASE + 0x5000);
/// An address inside the cartridge window but past a 16 MiB image.
const PAST_THE_ROM: GbaPtr = GbaPtr::at(ROM_BASE + 0x00FF_FFF0);

/// A 4x3 grid, stored with two bytes of padding past `width * height * 2`.
/// A handful of upstream `map.bin` files carry exactly that, so the reader
/// has to take the profile's length rather than recompute one.
const GRID_LEN: u32 = 26;

static LAYOUT: MapLayoutRoot = MapLayoutRoot {
    name: "l",
    struct_addr: STRUCT_AT,
    width: 4,
    height: 3,
    map: BlobRoot {
        id: "layout/l/map",
        addr: MAP_AT,
        encoding: Encoding::Raw,
        len: GRID_LEN,
    },
    border: BlobRoot {
        id: "layout/l/border",
        addr: BORDER_AT,
        encoding: Encoding::Raw,
        len: 8,
    },
    primary_tileset: PRIMARY,
    secondary_tileset: SECONDARY,
};

static LAYOUTS: [MapLayoutRoot; 1] = [LAYOUT];

/// The same layout with a grid too short for its declared size.
static SHORT_GRID: [MapLayoutRoot; 1] = [MapLayoutRoot {
    map: BlobRoot {
        id: "layout/l/map",
        addr: MAP_AT,
        encoding: Encoding::Raw,
        len: 16,
    },
    ..LAYOUT
}];

/// The same layout with its grid addressed past the image's end.
static FAR_GRID: [MapLayoutRoot; 1] = [MapLayoutRoot {
    map: BlobRoot {
        id: "layout/l/map",
        addr: PAST_THE_ROM,
        encoding: Encoding::Raw,
        len: GRID_LEN,
    },
    ..LAYOUT
}];

fn roots(layouts: &'static [MapLayoutRoot]) -> Roots {
    Roots {
        layouts,
        ..Roots::NONE
    }
}

/// A fixture holding a complete, self-consistent layout.
fn fixture() -> RomFixture {
    RomFixture::new()
        .emerald_header()
        // `struct MapLayout`, in declaration order.
        .write(STRUCT, &4u32.to_le_bytes())
        .write(STRUCT + 0x04, &3u32.to_le_bytes())
        .write_ptr(STRUCT + 0x08, BORDER_AT)
        .write_ptr(STRUCT + 0x0C, MAP_AT)
        .write_ptr(STRUCT + 0x10, PRIMARY)
        .write_ptr(STRUCT + 0x14, SECONDARY)
        .write(BORDER, &[1, 0, 2, 0, 3, 0, 4, 0])
        .write(MAP, &[0x5A; 26])
}

fn run_with(
    fixture: RomFixture,
    layouts: &'static [MapLayoutRoot],
) -> Result<Vec<u8>, ImportError> {
    let rom = Rom::from_bytes(fixture.finish()).expect("the fixture header is valid");
    let mut writer = PackWriter::new();
    write(&rom, &roots(layouts), &mut writer)?;
    Ok(writer.finish()?)
}

/// Run the reader over `fixture` and the default root table, returning the
/// pack it produced.
fn run(fixture: RomFixture) -> Result<Vec<u8>, ImportError> {
    run_with(fixture, &LAYOUTS)
}

#[test]
fn a_layout_reads_into_a_grid_and_a_border() {
    let bytes = run(fixture()).expect("the fixture is self-consistent");
    let entries = pack_format::parse_directory(&bytes).expect("the pack parses");
    assert_eq!(entries.len(), 2);

    let by_id = |id: &str| {
        entries
            .iter()
            .find(|entry| entry.id == id)
            .unwrap_or_else(|| panic!("`{id}` is missing"))
            .clone()
    };

    let map = by_id("layout/l/map");
    assert_eq!(map.kind, EntryKind::Raw);
    // The two padding bytes past `4 * 3 * 2` are read, not trimmed.
    assert_eq!(map.length, 26);
    assert_eq!(&bytes[map.offset..map.offset + 26], &[0x5A; 26]);

    let border = by_id("layout/l/border");
    assert_eq!(border.kind, EntryKind::Raw);
    assert_eq!(
        &bytes[border.offset..border.offset + 8],
        &[1, 0, 2, 0, 3, 0, 4, 0]
    );
}

#[test]
fn a_dimension_disagreeing_with_the_profile_is_refused() {
    for (offset, field) in [(0x00usize, "MapLayout.width"), (0x04, "MapLayout.height")] {
        let err = run(fixture().write(STRUCT + offset, &99u32.to_le_bytes())).unwrap_err();
        match err {
            ImportError::StructMismatch {
                root: "l",
                field: seen,
            } => assert_eq!(seen, field),
            other => panic!("{field} accepted a wrong value: {other}"),
        }
    }
}

#[test]
fn every_struct_pointer_is_corroborated() {
    let cases = [
        (0x08usize, "MapLayout.border"),
        (0x0C, "MapLayout.map"),
        (0x10, "MapLayout.primaryTileset"),
        (0x14, "MapLayout.secondaryTileset"),
    ];
    for (offset, field) in cases {
        let elsewhere = GbaPtr::at(ROM_BASE + 0x6000);
        let err = run(fixture().write_ptr(STRUCT + offset, elsewhere)).unwrap_err();
        match err {
            ImportError::StructMismatch {
                root: "l",
                field: seen,
            } => assert_eq!(seen, field),
            other => panic!("{field} accepted a wrong pointer: {other}"),
        }
    }
}

#[test]
fn a_pointer_field_outside_the_cartridge_window_is_refused() {
    let err = run(fixture().write(STRUCT + 0x0C, &0u32.to_le_bytes())).unwrap_err();
    assert!(matches!(err, ImportError::PointerOutOfRange { .. }));
}

#[test]
fn a_grid_shorter_than_its_declared_size_is_refused() {
    let err = run_with(fixture(), &SHORT_GRID).unwrap_err();
    assert!(matches!(
        err,
        ImportError::StructMismatch {
            root: "l",
            field: "MapLayout.map"
        }
    ));
}

#[test]
fn a_grid_past_the_rom_end_is_refused() {
    let fixture = fixture().write_ptr(STRUCT + 0x0C, PAST_THE_ROM);
    let err = run_with(fixture, &FAR_GRID).unwrap_err();
    assert!(matches!(err, ImportError::Truncated { .. }));
}

#[test]
fn an_empty_table_writes_nothing() {
    let rom = Rom::from_bytes(RomFixture::new().emerald_header().finish())
        .expect("the fixture header is valid");
    let mut writer = PackWriter::new();
    write(&rom, &Roots::NONE, &mut writer).expect("no roots is no work");
    assert_eq!(writer.len(), 0);
}
