//! Tileset reader tests over synthetic ROMs.
//!
//! Each case builds a fixture whose bytes and root table were written
//! together, so the happy path proves the reader walks `struct Tileset` the
//! way `global.fieldmap.h` declares it, and each fault case proves one wrong
//! byte is a refusal rather than a wrong asset.
//!
//! The fixture carries three palette banks where a real tileset carries
//! sixteen. The reader loops over whatever banks its roots list, so three
//! exercise the same path and keep the table readable; all sixteen of all
//! five real tilesets are covered by the real-ROM comparison.

use pack_format::{EntryKind, PackWriter};

use super::write;
use crate::error::{ImportError, Lz77Fault};
use crate::fixture::RomFixture;
use crate::reader::{GbaPtr, ROM_BASE};
use crate::rom::{Rom, ROM_SIZE};
use crate::roots::{BlobRoot, Encoding, ImageRoot, PaletteRoot, Roots, TileAnimRoot, TilesetRoot};

// Each fixture region is named twice, as a write offset and as the pointer
// the struct carries, so nothing in this file has to cast between the two.
/// Where the fixture's `struct Tileset` sits.
const STRUCT: usize = 0x1000;
/// The address of that struct.
const STRUCT_AT: GbaPtr = GbaPtr::at(ROM_BASE + 0x1000);
/// Where its palette block sits.
const PALETTES: usize = 0x2000;
/// The address of the palette block, which is bank 0's own address.
const PALETTES_AT: GbaPtr = GbaPtr::at(ROM_BASE + 0x2000);
/// Where its tile sheet sits.
const TILES: usize = 0x3000;
/// The address of the tile sheet.
const TILES_AT: GbaPtr = GbaPtr::at(ROM_BASE + 0x3000);
/// Where its metatile table sits.
const METATILES: usize = 0x4000;
/// The address of the metatile table.
const METATILES_AT: GbaPtr = GbaPtr::at(ROM_BASE + 0x4000);
/// Where its attribute table sits.
const ATTRIBUTES: usize = 0x5000;
/// The address of the attribute table.
const ATTRIBUTES_AT: GbaPtr = GbaPtr::at(ROM_BASE + 0x5000);
/// The address of its one animation frame.
const ANIM_AT: GbaPtr = GbaPtr::at(ROM_BASE + 0x6000);
/// Where that frame's bytes sit.
const ANIM: usize = 0x6000;
/// An address inside the cartridge window but past a 16 MiB image.
const PAST_THE_ROM: GbaPtr = GbaPtr::at(ROM_BASE + 0x00FF_FFF0);
/// The last four bytes of a 16 MiB image.
const AT_ROM_END: GbaPtr = GbaPtr::at(ROM_BASE + 0x00FF_FFFC);
/// The `callback` the fixture's struct carries.
const CALLBACK: u32 = 0x080A_0B21;

/// The palette banks, at consecutive 32-byte addresses.
static BANKS: [PaletteRoot; 3] = [
    PaletteRoot {
        id: "tileset/t/palette/00",
        addr: PALETTES_AT,
        color_count: 16,
    },
    PaletteRoot {
        id: "tileset/t/palette/01",
        addr: GbaPtr::at(ROM_BASE + 0x2020),
        color_count: 16,
    },
    PaletteRoot {
        id: "tileset/t/palette/02",
        addr: GbaPtr::at(ROM_BASE + 0x2040),
        color_count: 16,
    },
];

/// The one animation frame: a raw 2x2-tile 4bpp array.
static FRAME: ImageRoot = ImageRoot {
    id: "tileset/t/anim/blink/0",
    addr: ANIM_AT,
    encoding: Encoding::Raw,
    rom_bit_depth: 4,
    pack_bit_depth: 4,
    width: 16,
    height: 16,
    metatile_width: 1,
    metatile_height: 1,
    tile_count: 4,
};

static FRAMES: [ImageRoot; 1] = [FRAME];

static ANIMS: [TileAnimRoot; 1] = [TileAnimRoot {
    name: "blink",
    frames: &FRAMES,
}];

/// The one tileset the fixture describes: two 8x8 tiles, LZ77-compressed,
/// in a 16x8 raster.
static TILESET: TilesetRoot = TilesetRoot {
    name: "t",
    struct_addr: STRUCT_AT,
    is_compressed: true,
    is_secondary: false,
    tiles: ImageRoot {
        id: "tileset/t/tiles",
        addr: TILES_AT,
        encoding: Encoding::Lz77,
        rom_bit_depth: 4,
        pack_bit_depth: 4,
        width: 16,
        height: 8,
        metatile_width: 1,
        metatile_height: 1,
        tile_count: 2,
    },
    palettes: &BANKS,
    metatiles: BlobRoot {
        id: "tileset/t/metatiles",
        addr: METATILES_AT,
        encoding: Encoding::Raw,
        len: 16,
    },
    metatile_attributes: BlobRoot {
        id: "tileset/t/metatile-attributes",
        addr: ATTRIBUTES_AT,
        encoding: Encoding::Raw,
        len: 4,
    },
    callback: CALLBACK,
    anims: &ANIMS,
};

static TILESETS: [TilesetRoot; 1] = [TILESET];

/// The same tileset with its metatile table addressed past the image's end.
static FAR_METATILES: [TilesetRoot; 1] = [TilesetRoot {
    metatiles: BlobRoot {
        id: "tileset/t/metatiles",
        addr: PAST_THE_ROM,
        encoding: Encoding::Raw,
        len: 64,
    },
    ..TILESET
}];

/// The same tileset with its animation frame addressed past the end.
static FAR_FRAMES: [ImageRoot; 1] = [ImageRoot {
    addr: PAST_THE_ROM,
    ..FRAME
}];

static FAR_ANIMS: [TileAnimRoot; 1] = [TileAnimRoot {
    name: "blink",
    frames: &FAR_FRAMES,
}];

static FAR_ANIM: [TilesetRoot; 1] = [TilesetRoot {
    anims: &FAR_ANIMS,
    ..TILESET
}];

/// The same tileset with its tile stream in the image's last four bytes.
static TILES_AT_ROM_END: [TilesetRoot; 1] = [TilesetRoot {
    tiles: ImageRoot {
        addr: AT_ROM_END,
        ..TILESET.tiles
    },
    ..TILESET
}];

fn roots(tilesets: &'static [TilesetRoot]) -> Roots {
    Roots {
        tilesets,
        ..Roots::NONE
    }
}

/// An LZ77 stream carrying `bytes` as plain literals.
fn lz77_literals(bytes: &[u8]) -> Vec<u8> {
    let len = u32::try_from(bytes.len()).expect("a test stream is small");
    let size = len.to_le_bytes();
    let mut stream = vec![0x10, size[0], size[1], size[2]];
    for chunk in bytes.chunks(8) {
        stream.push(0x00);
        stream.extend_from_slice(chunk);
    }
    stream
}

/// The 64 bytes of two 4bpp tiles, every byte distinct.
fn tile_bytes() -> Vec<u8> {
    (0..64u8).collect()
}

/// A fixture holding a complete, self-consistent tileset.
fn fixture() -> RomFixture {
    let mut palette_block = Vec::new();
    for bank in 0..3u16 {
        for color in 0..16u16 {
            palette_block.extend_from_slice(&(bank * 16 + color).to_le_bytes());
        }
    }
    RomFixture::new()
        .emerald_header()
        // `struct Tileset`, in declaration order.
        .write(STRUCT, &[1, 0, 0, 0])
        .write_ptr(STRUCT + 0x04, TILES_AT)
        .write_ptr(STRUCT + 0x08, PALETTES_AT)
        .write_ptr(STRUCT + 0x0C, METATILES_AT)
        .write_ptr(STRUCT + 0x10, ATTRIBUTES_AT)
        .write(STRUCT + 0x14, &CALLBACK.to_le_bytes())
        .write(PALETTES, &palette_block)
        .write(TILES, &lz77_literals(&tile_bytes()))
        .write(METATILES, &[0xAA; 16])
        .write(ATTRIBUTES, &[0xBB; 4])
        .write(ANIM, &[0xCC; 128])
}

fn rom_from(fixture: RomFixture) -> Rom {
    Rom::from_bytes(fixture.finish()).expect("the fixture header is valid")
}

/// Run the reader over `fixture` and the default root table, returning the
/// pack it produced.
fn run(fixture: RomFixture) -> Result<Vec<u8>, ImportError> {
    run_with(fixture, &TILESETS)
}

fn run_with(fixture: RomFixture, tilesets: &'static [TilesetRoot]) -> Result<Vec<u8>, ImportError> {
    let rom = rom_from(fixture);
    let mut writer = PackWriter::new();
    write(&rom, &roots(tilesets), &mut writer)?;
    Ok(writer.finish()?)
}

#[test]
fn a_whole_tileset_reads_into_entries() {
    let bytes = run(fixture()).expect("the fixture is self-consistent");
    let entries = pack_format::parse_directory(&bytes).expect("the pack parses");
    // Tiles, three palette banks, two tables, one animation frame.
    assert_eq!(entries.len(), 7);

    let by_id = |id: &str| {
        entries
            .iter()
            .find(|entry| entry.id == id)
            .unwrap_or_else(|| panic!("`{id}` is missing"))
            .clone()
    };

    let tiles = by_id("tileset/t/tiles");
    assert_eq!(
        tiles.kind,
        EntryKind::Image {
            width: 16,
            height: 8,
            bit_depth: 4
        }
    );
    // 4bpp unpacks the low nibble first, so tile byte 1 (0x01) becomes
    // pixels 1 and 0.
    assert_eq!(&bytes[tiles.offset..tiles.offset + 4], &[0, 0, 1, 0]);

    let bank = by_id("tileset/t/palette/02");
    assert_eq!(bank.kind, EntryKind::Palette { color_count: 16 });
    assert_eq!(&bytes[bank.offset..bank.offset + 2], &32u16.to_le_bytes());

    let metatiles = by_id("tileset/t/metatiles");
    assert_eq!(metatiles.kind, EntryKind::Raw);
    assert_eq!(&bytes[metatiles.offset..metatiles.offset + 16], &[0xAA; 16]);
    assert_eq!(by_id("tileset/t/metatile-attributes").length, 4);

    let frame = by_id("tileset/t/anim/blink/0");
    assert_eq!(
        frame.kind,
        EntryKind::Image {
            width: 16,
            height: 16,
            bit_depth: 4
        }
    );
    assert_eq!(frame.length, 256);
}

#[test]
fn a_struct_bool_outside_zero_or_one_is_refused() {
    let err = run(fixture().write(STRUCT, &[2])).unwrap_err();
    assert!(matches!(
        err,
        ImportError::StructMismatch {
            root: "t",
            field: "Tileset.isCompressed"
        }
    ));
}

#[test]
fn a_struct_bool_disagreeing_with_the_profile_is_refused() {
    // The profile says this tileset is primary; the ROM says secondary.
    let err = run(fixture().write(STRUCT + 0x01, &[1])).unwrap_err();
    assert!(matches!(
        err,
        ImportError::StructMismatch {
            root: "t",
            field: "Tileset.isSecondary"
        }
    ));
}

#[test]
fn every_struct_pointer_is_corroborated() {
    let cases = [
        (0x04usize, "Tileset.tiles"),
        (0x08, "Tileset.palettes"),
        (0x0C, "Tileset.metatiles"),
        (0x10, "Tileset.metatileAttributes"),
    ];
    for (offset, field) in cases {
        let elsewhere = GbaPtr::at(ROM_BASE + 0x7000);
        let err = run(fixture().write_ptr(STRUCT + offset, elsewhere)).unwrap_err();
        match err {
            ImportError::StructMismatch {
                root: "t",
                field: seen,
            } => assert_eq!(seen, field),
            other => panic!("{field} accepted a wrong pointer: {other}"),
        }
    }
}

#[test]
fn a_callback_disagreeing_with_the_profile_is_refused() {
    let err = run(fixture().write(STRUCT + 0x14, &0u32.to_le_bytes())).unwrap_err();
    assert!(matches!(
        err,
        ImportError::StructMismatch {
            root: "t",
            field: "Tileset.callback"
        }
    ));
}

#[test]
fn a_pointer_field_outside_the_cartridge_window_is_refused() {
    let err = run(fixture().write(STRUCT + 0x04, &0u32.to_le_bytes())).unwrap_err();
    assert!(matches!(err, ImportError::PointerOutOfRange { .. }));
}

#[test]
fn a_tile_stream_that_runs_past_its_data_is_refused() {
    // Declare far more bytes than the stream carries. Cartridge filler
    // follows, so the decoder does not run out of input; it decodes filler
    // as a back-reference reaching before the start of the output.
    let mut stream = lz77_literals(&tile_bytes());
    stream[1] = 0xFF;
    let err = run(fixture().write(TILES, &stream)).unwrap_err();
    assert!(matches!(
        err,
        ImportError::Lz77 {
            fault: Lz77Fault::BackReferenceTooFar { .. },
            ..
        }
    ));
}

#[test]
fn a_tile_stream_at_the_rom_end_is_refused() {
    // Only the four header bytes fit, so the first flag byte is past the
    // end of the image.
    let fixture = fixture()
        .write_ptr(STRUCT + 0x04, AT_ROM_END)
        .write(ROM_SIZE - 4, &[0x10, 0x40, 0x00, 0x00]);
    let err = run_with(fixture, &TILES_AT_ROM_END).unwrap_err();
    assert!(matches!(
        err,
        ImportError::Lz77 {
            fault: Lz77Fault::UnexpectedEnd,
            ..
        }
    ));
}

#[test]
fn a_tile_stream_of_the_wrong_size_is_refused() {
    // A valid stream that decodes to one tile, where the profile says two.
    let stream = lz77_literals(&tile_bytes()[..32]);
    let err = run(fixture().write(TILES, &stream)).unwrap_err();
    assert!(matches!(
        err,
        ImportError::Lz77 {
            fault: Lz77Fault::SizeMismatch {
                expected: 64,
                actual: 32
            },
            ..
        }
    ));
}

#[test]
fn a_table_past_the_rom_end_is_refused() {
    let fixture = fixture().write_ptr(STRUCT + 0x0C, PAST_THE_ROM);
    let err = run_with(fixture, &FAR_METATILES).unwrap_err();
    assert!(matches!(err, ImportError::Truncated { .. }));
}

#[test]
fn an_animation_frame_past_the_rom_end_is_refused() {
    let err = run_with(fixture(), &FAR_ANIM).unwrap_err();
    assert!(matches!(err, ImportError::Truncated { .. }));
}

#[test]
fn an_empty_table_writes_nothing() {
    let rom = rom_from(RomFixture::new().emerald_header());
    let mut writer = PackWriter::new();
    write(&rom, &Roots::NONE, &mut writer).expect("no roots is no work");
    assert_eq!(writer.len(), 0);
}
