//! Domain readers over a synthetic ROM.
//!
//! A [`RomFixture`] with a hand-built LZ77 stream, a palette, and a raw
//! tile sheet planted at chosen addresses, plus a [`Roots`] table pointing
//! at them. No real ROM is involved, so these run in the default gate.
//!
//! The tests come in pairs: what the readers write when the table is right,
//! and what they raise when it is not. Every fault path a domain can hit is
//! covered, because the importer runs on a file the user supplied and a
//! wrong address must be a typed error, never a panic or a wrong asset.

use pack_format::{EntryKind, PackWriter};

use super::{write_blob, write_image, write_palette, DOMAINS};
use crate::error::{ImportError, Lz77Fault};
use crate::fixture::RomFixture;
use crate::reader::{GbaPtr, ROM_BASE};
use crate::rom::{Rom, ROM_SIZE};
use crate::roots::{
    BlobRoot, Encoding, ImageRoot, InterfaceRoots, PaletteRoot, Roots, TitleScreenRoots,
};

/// The image's size as a bus offset.
///
/// [`ROM_SIZE`] is a `usize`; these roots need it as a `u32`, and the
/// assertion below is what keeps the two spellings in step.
const ROM_SIZE_U32: u32 = 0x0100_0000;
const _: () = assert!(ROM_SIZE_U32 as usize == ROM_SIZE);

/// Where the fixture plants each asset, as bus offsets.
const TILES_OFF: u32 = 0x1000;
const PALETTE_OFF: u32 = 0x2000;
const TILEMAP_OFF: u32 = 0x3000;
const RAW_TILES_OFF: u32 = 0x4000;
/// A stream whose declared size runs off the end of the image.
const TRUNCATED_OFF: u32 = ROM_SIZE_U32 - 6;

/// The cartridge address of a ROM offset.
const fn at(off: u32) -> GbaPtr {
    GbaPtr::at(ROM_BASE + off)
}

/// The same offset, as an index into a fixture's bytes.
const fn off(at: u32) -> usize {
    at as usize
}

/// One 4bpp tile: pixel `n` of row 0 is `n`, every other row is zero.
///
/// 4bpp packs two pixels per byte, low nibble first, so row 0's eight
/// pixels `0..=7` are the four bytes below.
const TILE_ROW0: [u8; 4] = [0x10, 0x32, 0x54, 0x76];

/// That tile, packed as the ROM stores it: 32 bytes, row 0 then seven zero
/// rows.
fn one_tile() -> Vec<u8> {
    let mut tile = TILE_ROW0.to_vec();
    tile.resize(32, 0);
    tile
}

/// An LZ77 type `0x10` stream of `payload`, all literals.
///
/// Literal-only is the one encoding a test can write by hand and still read
/// back; `crate::lz77`'s own tests cover back-references.
fn lz77_literals(payload: &[u8]) -> Vec<u8> {
    let len = u32::try_from(payload.len()).expect("a test payload fits in 24 bits");
    let size = len.to_le_bytes();
    let mut stream = vec![0x10, size[0], size[1], size[2]];
    for chunk in payload.chunks(8) {
        // A clear flag bit per literal, most significant bit first.
        stream.push(0x00);
        stream.extend_from_slice(chunk);
    }
    stream
}

/// A fixture ROM holding one compressed tile, one palette, one compressed
/// tilemap, and one uncompressed tile.
fn fixture_rom() -> Rom {
    let bytes = RomFixture::new()
        .emerald_header()
        .write(off(TILES_OFF), &lz77_literals(&one_tile()))
        .write(off(PALETTE_OFF), &[0x1F, 0x00, 0xE0, 0x03])
        .write(off(TILEMAP_OFF), &lz77_literals(b"tilemap!"))
        .write(off(RAW_TILES_OFF), &one_tile())
        // Declares 64 bytes, then the image ends after one literal.
        .write(off(TRUNCATED_OFF), &[0x10, 0x40, 0x00, 0x00, 0x00, b'A'])
        .finish();
    Rom::from_bytes(bytes).expect("the fixture header is valid")
}

fn image_root() -> ImageRoot {
    ImageRoot {
        id: "title/image/test",
        addr: at(TILES_OFF),
        encoding: Encoding::Lz77,
        rom_bit_depth: 4,
        pack_bit_depth: 4,
        width: 8,
        height: 8,
        metatile_width: 1,
        metatile_height: 1,
        tile_count: 1,
    }
}

fn palette_root() -> PaletteRoot {
    PaletteRoot {
        id: "title/palette/test",
        addr: at(PALETTE_OFF),
        color_count: 2,
    }
}

fn blob_root() -> BlobRoot {
    BlobRoot {
        id: "title/raw/test",
        addr: at(TILEMAP_OFF),
        encoding: Encoding::Lz77,
        len: 8,
    }
}

/// Queue one root and hand back the entry it produced.
fn written<T>(
    write: impl Fn(&Rom, &T, &mut PackWriter) -> Result<(), ImportError>,
    root: &T,
) -> Result<pack_format::PackEntry, ImportError> {
    let mut writer = PackWriter::new();
    write(&fixture_rom(), root, &mut writer)?;
    let bytes = writer.finish().expect("one entry is a valid pack");
    let entries = pack_format::parse_directory(&bytes).expect("its own writer's output parses");
    assert_eq!(entries.len(), 1);
    let entry = &entries[0];
    Ok(pack_format::PackEntry {
        id: entry.id.clone(),
        kind: entry.kind,
        payload: bytes[entry.offset..entry.offset + entry.length].to_vec(),
    })
}

#[test]
fn a_compressed_image_unpacks_into_a_raster() {
    let entry = written(write_image, &image_root()).expect("a well-formed root");
    assert_eq!(entry.id, "title/image/test");
    assert_eq!(
        entry.kind,
        EntryKind::Image {
            width: 8,
            height: 8,
            bit_depth: 4,
        }
    );
    // Row 0 is the unpacked nibbles; the other seven rows are zero.
    assert_eq!(&entry.payload[..8], &[0, 1, 2, 3, 4, 5, 6, 7]);
    assert_eq!(&entry.payload[8..], &[0u8; 56]);
}

#[test]
fn an_uncompressed_image_reads_straight_through() {
    let root = ImageRoot {
        addr: at(RAW_TILES_OFF),
        encoding: Encoding::Raw,
        ..image_root()
    };
    let entry = written(write_image, &root).expect("a well-formed root");
    assert_eq!(&entry.payload[..8], &[0, 1, 2, 3, 4, 5, 6, 7]);
}

#[test]
fn the_pack_records_the_source_depth_not_the_roms() {
    // `title/image/press_start`'s case: 4bpp tiles, an 8bpp pack entry. The
    // payload is one byte per pixel either way, so only the metadata moves.
    let root = ImageRoot {
        pack_bit_depth: 8,
        ..image_root()
    };
    let entry = written(write_image, &root).expect("a well-formed root");
    assert_eq!(
        entry.kind,
        EntryKind::Image {
            width: 8,
            height: 8,
            bit_depth: 8,
        }
    );
    assert_eq!(&entry.payload[..8], &[0, 1, 2, 3, 4, 5, 6, 7]);
}

#[test]
fn a_palette_reads_gba_native_colours() {
    let entry = written(write_palette, &palette_root()).expect("a well-formed root");
    assert_eq!(entry.id, "title/palette/test");
    assert_eq!(entry.kind, EntryKind::Palette { color_count: 2 });
    // BGR555 passes through unconverted: red, then green.
    assert_eq!(entry.payload, vec![0x1F, 0x00, 0xE0, 0x03]);
}

#[test]
fn a_compressed_blob_copies_through() {
    let entry = written(write_blob, &blob_root()).expect("a well-formed root");
    assert_eq!(entry.id, "title/raw/test");
    assert_eq!(entry.kind, EntryKind::Raw);
    assert_eq!(entry.payload, b"tilemap!");
}

#[test]
fn a_truncated_stream_is_rejected() {
    // The stream declares 64 bytes and the image ends after one literal, so
    // the next token is past the end of the ROM.
    let root = BlobRoot {
        addr: at(TRUNCATED_OFF),
        len: 64,
        ..blob_root()
    };
    let err = written(write_blob, &root).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::Lz77 {
                fault: Lz77Fault::UnexpectedEnd,
                ..
            }
        ),
        "{err}"
    );
}

#[test]
fn a_wrong_decompressed_length_is_rejected() {
    // The stream decodes cleanly, to a length the table does not expect.
    let root = BlobRoot {
        len: 7,
        ..blob_root()
    };
    let err = written(write_blob, &root).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::Lz77 {
                fault: Lz77Fault::SizeMismatch {
                    expected: 7,
                    actual: 8
                },
                ..
            }
        ),
        "{err}"
    );
}

#[test]
fn a_stream_that_is_not_lz77_is_rejected() {
    let root = BlobRoot {
        addr: at(PALETTE_OFF),
        ..blob_root()
    };
    let err = written(write_blob, &root).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::Lz77 {
                fault: Lz77Fault::WrongType,
                ..
            }
        ),
        "{err}"
    );
}

#[test]
fn a_palette_past_the_rom_end_is_rejected() {
    // Inside the cartridge window, past this 16 MiB image.
    let root = PaletteRoot {
        addr: GbaPtr::at(ROM_BASE + ROM_SIZE_U32 + 0x100),
        ..palette_root()
    };
    let err = written(write_palette, &root).unwrap_err();
    assert!(matches!(err, ImportError::Truncated { .. }), "{err}");

    // Straddling the end: the first colour is in the image, the second is
    // not.
    let root = PaletteRoot {
        addr: at(ROM_SIZE_U32 - 2),
        ..palette_root()
    };
    let err = written(write_palette, &root).unwrap_err();
    assert!(matches!(err, ImportError::Truncated { .. }), "{err}");
}

#[test]
fn an_uncompressed_read_past_the_rom_end_is_rejected() {
    let root = ImageRoot {
        addr: at(ROM_SIZE_U32 - 16),
        encoding: Encoding::Raw,
        ..image_root()
    };
    let err = written(write_image, &root).unwrap_err();
    assert!(matches!(err, ImportError::Truncated { .. }), "{err}");
}

#[test]
fn tiles_that_do_not_fit_the_raster_are_rejected() {
    // Fewer stored tiles than the raster needs is legal:
    // `image_entry_from_tiles` zero-fills what upstream's `-num_tiles` cut
    // dropped. More than it needs is not.
    let root = ImageRoot {
        addr: at(RAW_TILES_OFF),
        encoding: Encoding::Raw,
        tile_count: 2,
        ..image_root()
    };
    let err = written(write_image, &root).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::EntryShape {
                id: "title/image/test",
                ..
            }
        ),
        "{err}"
    );
    assert!(err.to_string().contains("title/image/test"));
}

#[test]
fn an_unsupported_rom_depth_is_rejected() {
    // The GBA packs no other tile format, so a table claiming one is a
    // corrupt table rather than an asset this reader could salvage.
    let root = ImageRoot {
        addr: at(RAW_TILES_OFF),
        encoding: Encoding::Raw,
        rom_bit_depth: 2,
        ..image_root()
    };
    let err = written(write_image, &root).unwrap_err();
    assert!(matches!(err, ImportError::EntryShape { .. }), "{err}");
}

#[test]
fn the_domain_list_writes_every_root_it_is_given() {
    static IMAGES: &[ImageRoot] = &[ImageRoot {
        id: "title/image/test",
        addr: at(TILES_OFF),
        encoding: Encoding::Lz77,
        rom_bit_depth: 4,
        pack_bit_depth: 4,
        width: 8,
        height: 8,
        metatile_width: 1,
        metatile_height: 1,
        tile_count: 1,
    }];
    static TILEMAPS: &[BlobRoot] = &[BlobRoot {
        id: "title/raw/test",
        addr: at(TILEMAP_OFF),
        encoding: Encoding::Lz77,
        len: 8,
    }];
    static TITLE_PALETTES: &[PaletteRoot] = &[PaletteRoot {
        id: "title/palette/test",
        addr: at(PALETTE_OFF),
        color_count: 2,
    }];
    static INTERFACE_PALETTES: &[PaletteRoot] = &[PaletteRoot {
        id: "interface/palette/test",
        addr: at(PALETTE_OFF),
        color_count: 2,
    }];

    let roots = Roots {
        title_screen: TitleScreenRoots {
            images: IMAGES,
            tilemaps: TILEMAPS,
            palettes: TITLE_PALETTES,
            bg_palettes: at(PALETTE_OFF),
        },
        interface: InterfaceRoots {
            palettes: INTERFACE_PALETTES,
        },
        ..Roots::NONE
    };

    let rom = fixture_rom();
    let mut writer = PackWriter::new();
    for domain in DOMAINS {
        domain(&rom, &roots, &mut writer).expect("every root is well-formed");
    }
    let bytes = writer.finish().expect("no duplicate ids");
    let entries = pack_format::parse_directory(&bytes).expect("a valid pack");

    let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "interface/palette/test",
            "title/image/test",
            "title/palette/test",
            "title/raw/test",
        ]
    );
}
