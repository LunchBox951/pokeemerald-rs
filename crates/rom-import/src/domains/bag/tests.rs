//! Bag reader tests over a synthetic ROM.
//!
//! One image, palette and tilemap are planted under real `bag/` ids and read
//! back through the domain. The failure tests break one root at a time and
//! require a typed error with nothing queued.

use pack_format::{parse_directory, EntryKind, PackWriter};

use super::write;
use crate::domains::DOMAINS;
use crate::error::{ImportError, Lz77Fault};
use crate::fixture::RomFixture;
use crate::reader::{GbaPtr, ROM_BASE};
use crate::rom::{Rom, ROM_SIZE};
use crate::roots::{BagRoots, BlobRoot, Encoding, ImageRoot, PaletteRoot, Roots};

const TILES: usize = 0x1000;
const PALETTE: usize = 0x2000;
const TILEMAP: usize = 0x3000;

/// A 4bpp tile whose row 0 counts `0..=7`, then seven zero rows.
fn tile() -> Vec<u8> {
    let mut tile = vec![0x10, 0x32, 0x54, 0x76];
    tile.resize(32, 0);
    tile
}

fn rom() -> Rom {
    let bytes = RomFixture::new()
        .emerald_header()
        .write(TILES, &tile())
        .write(PALETTE, &[0x1F, 0x00, 0xE0, 0x03])
        .write(TILEMAP, b"tilemap!")
        .finish();
    Rom::from_bytes(bytes).expect("the fixture header is valid")
}

const IMAGE: ImageRoot = ImageRoot {
    id: "bag/item-icon/image/test",
    addr: at(TILES),
    encoding: Encoding::Raw,
    rom_bit_depth: 4,
    pack_bit_depth: 4,
    width: 8,
    height: 8,
    metatile_width: 1,
    metatile_height: 1,
    tile_count: 1,
};
const PALETTE_ROOT: PaletteRoot = PaletteRoot {
    id: "bag/item-icon/palette/test",
    addr: at(PALETTE),
    encoding: Encoding::Raw,
    color_count: 2,
};
const RAW: BlobRoot = BlobRoot {
    id: "bag/raw/menu",
    addr: at(TILEMAP),
    encoding: Encoding::Raw,
    len: 8,
};

/// The bus address of a fixture offset.
const fn at(off: usize) -> GbaPtr {
    #[allow(clippy::cast_possible_truncation)]
    GbaPtr::at(ROM_BASE + off as u32)
}

fn roots(bag: BagRoots) -> Roots {
    Roots { bag, ..Roots::NONE }
}

fn written(roots: &Roots) -> Result<Vec<pack_format::DirectoryEntry>, ImportError> {
    let mut writer = PackWriter::new();
    write(&rom(), roots, &mut writer)?;
    let bytes = writer.finish().expect("no duplicate ids");
    Ok(parse_directory(&bytes).expect("a valid pack"))
}

#[test]
fn every_bag_root_is_emitted() {
    static IMAGES: &[ImageRoot] = &[IMAGE];
    static PALETTES: &[PaletteRoot] = &[PALETTE_ROOT];
    static RAWS: &[BlobRoot] = &[RAW];
    let roots = roots(BagRoots {
        images: IMAGES,
        palettes: PALETTES,
        raws: RAWS,
    });
    let entries = written(&roots).expect("every root is well-formed");
    let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "bag/item-icon/image/test",
            "bag/item-icon/palette/test",
            "bag/raw/menu"
        ]
    );
    assert_eq!(
        entries[0].kind,
        EntryKind::Image {
            width: 8,
            height: 8,
            bit_depth: 4
        }
    );
    assert_eq!(entries[1].kind, EntryKind::Palette { color_count: 2 });
    assert_eq!(entries[2].kind, EntryKind::Raw);
}

#[test]
fn the_domain_list_dispatches_the_bag() {
    static IMAGES: &[ImageRoot] = &[IMAGE];
    let roots = roots(BagRoots {
        images: IMAGES,
        ..BagRoots::NONE
    });
    let rom = rom();
    let mut writer = PackWriter::new();
    for domain in DOMAINS {
        domain(&rom, &roots, &mut writer).expect("every root is well-formed");
    }
    let bytes = writer.finish().expect("no duplicate ids");
    let entries = parse_directory(&bytes).expect("a valid pack");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, "bag/item-icon/image/test");
}

#[test]
fn an_empty_family_writes_nothing() {
    assert_eq!(written(&Roots::NONE).expect("nothing to read").len(), 0);
    assert_eq!(Roots::NONE.bag, BagRoots::NONE);
}

#[test]
fn bag_images_and_palettes_are_enumerated() {
    static IMAGES: &[ImageRoot] = &[IMAGE];
    static PALETTES: &[PaletteRoot] = &[PALETTE_ROOT];
    let roots = roots(BagRoots {
        images: IMAGES,
        palettes: PALETTES,
        ..BagRoots::NONE
    });
    let images: Vec<&str> = roots.images().map(|root| root.id).collect();
    let palettes: Vec<&str> = roots.palettes().map(|root| root.id).collect();
    assert_eq!(images, ["bag/item-icon/image/test"]);
    assert_eq!(palettes, ["bag/item-icon/palette/test"]);
}

#[test]
fn a_root_past_the_rom_fails_closed() {
    static IMAGES: &[ImageRoot] = &[IMAGE];
    static PALETTES: &[PaletteRoot] = &[PaletteRoot {
        addr: at(ROM_SIZE - 2),
        ..PALETTE_ROOT
    }];
    let roots = roots(BagRoots {
        images: IMAGES,
        palettes: PALETTES,
        ..BagRoots::NONE
    });
    let mut writer = PackWriter::new();
    let err = write(&rom(), &roots, &mut writer).unwrap_err();
    assert!(matches!(err, ImportError::Truncated { .. }), "{err}");
    // The good image read before the bad palette was not queued.
    let bytes = writer.finish().expect("an empty pack");
    assert_eq!(parse_directory(&bytes).expect("a valid pack").len(), 0);
}

#[test]
fn a_malformed_compressed_root_is_a_typed_error() {
    // The palette bytes are not an LZ77 stream.
    static RAWS: &[BlobRoot] = &[BlobRoot {
        addr: at(PALETTE),
        encoding: Encoding::Lz77,
        ..RAW
    }];
    let roots = roots(BagRoots {
        raws: RAWS,
        ..BagRoots::NONE
    });
    let err = written(&roots).unwrap_err();
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
fn tiles_that_overfill_the_raster_are_a_shape_error() {
    static IMAGES: &[ImageRoot] = &[ImageRoot {
        tile_count: 2,
        ..IMAGE
    }];
    let roots = roots(BagRoots {
        images: IMAGES,
        ..BagRoots::NONE
    });
    let err = written(&roots).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::EntryShape {
                id: "bag/item-icon/image/test",
                ..
            }
        ),
        "{err}"
    );
}
