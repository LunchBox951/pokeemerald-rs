//! Starter-chooser reader tests over a synthetic ROM.
//!
//! One image, palette and tilemap are planted under real `starter-chooser/` ids and read
//! back through the domain. The failure tests break one root at a time and
//! require a typed error with nothing queued.

use pack_format::{parse_directory, EntryKind, PackWriter};

use super::write;
use crate::domains::DOMAINS;
use crate::error::{ImportError, Lz77Fault};
use crate::fixture::RomFixture;
use crate::reader::{GbaPtr, ROM_BASE};
use crate::rom::{Rom, ROM_SIZE};
use crate::roots::{BlobRoot, Encoding, ImageRoot, PaletteRoot, Roots, StarterChooserRoots};

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
    id: "starter-chooser/image/test",
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
    id: "starter-chooser/palette/test",
    addr: at(PALETTE),
    encoding: Encoding::Raw,
    color_count: 2,
};
const RAW: BlobRoot = BlobRoot {
    id: "starter-chooser/raw/test",
    addr: at(TILEMAP),
    encoding: Encoding::Raw,
    len: 8,
};

/// The bus address of a fixture offset.
const fn at(off: usize) -> GbaPtr {
    #[allow(clippy::cast_possible_truncation)]
    GbaPtr::at(ROM_BASE + off as u32)
}

fn roots(chooser: StarterChooserRoots) -> Roots {
    Roots {
        starter_chooser: chooser,
        ..Roots::NONE
    }
}

fn written(roots: &Roots) -> Result<Vec<pack_format::DirectoryEntry>, ImportError> {
    let mut writer = PackWriter::new();
    write(&rom(), roots, &mut writer)?;
    let bytes = writer.finish().expect("no duplicate ids");
    Ok(parse_directory(&bytes).expect("a valid pack"))
}

#[test]
fn every_chooser_root_is_emitted() {
    static IMAGES: &[ImageRoot] = &[IMAGE];
    static PALETTES: &[PaletteRoot] = &[PALETTE_ROOT];
    static TILEMAPS: &[BlobRoot] = &[RAW];
    let roots = roots(StarterChooserRoots {
        images: IMAGES,
        palettes: PALETTES,
        tilemaps: TILEMAPS,
    });
    let entries = written(&roots).expect("every root is well-formed");
    let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "starter-chooser/image/test",
            "starter-chooser/palette/test",
            "starter-chooser/raw/test"
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
fn the_domain_list_dispatches_the_starter_chooser() {
    static IMAGES: &[ImageRoot] = &[IMAGE];
    let roots = roots(StarterChooserRoots {
        images: IMAGES,
        ..StarterChooserRoots::NONE
    });
    let rom = rom();
    let mut writer = PackWriter::new();
    for domain in DOMAINS {
        domain(&rom, &roots, &mut writer).expect("every root is well-formed");
    }
    let bytes = writer.finish().expect("no duplicate ids");
    let entries = parse_directory(&bytes).expect("a valid pack");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, "starter-chooser/image/test");
}

#[test]
fn an_empty_family_writes_nothing() {
    assert!(written(&Roots::NONE).expect("nothing to read").is_empty());
    assert_eq!(Roots::NONE.starter_chooser, StarterChooserRoots::NONE);
}

#[test]
fn chooser_images_and_palettes_are_enumerated() {
    static IMAGES: &[ImageRoot] = &[IMAGE];
    static PALETTES: &[PaletteRoot] = &[PALETTE_ROOT];
    let roots = roots(StarterChooserRoots {
        images: IMAGES,
        palettes: PALETTES,
        ..StarterChooserRoots::NONE
    });
    let images: Vec<&str> = roots.images().map(|root| root.id).collect();
    let palettes: Vec<&str> = roots.palettes().map(|root| root.id).collect();
    assert_eq!(images, ["starter-chooser/image/test"]);
    assert_eq!(palettes, ["starter-chooser/palette/test"]);
}

#[test]
fn a_root_past_the_rom_fails_closed() {
    static IMAGES: &[ImageRoot] = &[IMAGE];
    static PALETTES: &[PaletteRoot] = &[PaletteRoot {
        addr: at(ROM_SIZE - 2),
        ..PALETTE_ROOT
    }];
    let roots = roots(StarterChooserRoots {
        images: IMAGES,
        palettes: PALETTES,
        ..StarterChooserRoots::NONE
    });
    let mut writer = PackWriter::new();
    let err = write(&rom(), &roots, &mut writer).unwrap_err();
    assert!(matches!(err, ImportError::Truncated { .. }), "{err}");
    // The good image read before the bad palette was not queued.
    let bytes = writer.finish().expect("an empty pack");
    assert!(parse_directory(&bytes).expect("a valid pack").is_empty());
}

#[test]
fn a_malformed_compressed_root_is_a_typed_error() {
    // The palette bytes are not an LZ77 stream.
    static TILEMAPS: &[BlobRoot] = &[BlobRoot {
        addr: at(PALETTE),
        encoding: Encoding::Lz77,
        ..RAW
    }];
    let roots = roots(StarterChooserRoots {
        tilemaps: TILEMAPS,
        ..StarterChooserRoots::NONE
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
    let roots = roots(StarterChooserRoots {
        images: IMAGES,
        ..StarterChooserRoots::NONE
    });
    let err = written(&roots).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::EntryShape {
                id: "starter-chooser/image/test",
                ..
            }
        ),
        "{err}"
    );
}

/// A literals-only LZ77 stream of `data`, which is all the decoder needs.
fn lz77_literals(data: &[u8]) -> Vec<u8> {
    let len = u32::try_from(data.len()).expect("small").to_le_bytes();
    let mut out = vec![0x10, len[0], len[1], len[2]];
    for chunk in data.chunks(8) {
        out.push(0);
        out.extend_from_slice(chunk);
    }
    out
}

#[test]
fn a_compressed_four_bit_sheet_and_tilemap_round_trip() {
    const SHEET: usize = 0x4000;
    const MAP: usize = 0x5000;
    let tilemap = [0x01, 0x10, 0x02, 0x20];
    let bytes = RomFixture::new()
        .emerald_header()
        .write(SHEET, &lz77_literals(&tile()))
        .write(MAP, &lz77_literals(&tilemap))
        .finish();
    let rom = Rom::from_bytes(bytes).expect("the fixture header is valid");
    static IMAGES: &[ImageRoot] = &[ImageRoot {
        addr: at(0x4000),
        encoding: Encoding::Lz77,
        ..IMAGE
    }];
    static TILEMAPS: &[BlobRoot] = &[BlobRoot {
        addr: at(0x5000),
        encoding: Encoding::Lz77,
        len: 4,
        ..RAW
    }];
    let roots = roots(StarterChooserRoots {
        images: IMAGES,
        tilemaps: TILEMAPS,
        ..StarterChooserRoots::NONE
    });
    let mut writer = PackWriter::new();
    write(&rom, &roots, &mut writer).expect("both streams decode");
    let bytes = writer.finish().expect("no duplicate ids");
    let entries = parse_directory(&bytes).expect("a valid pack");
    assert_eq!(entries.len(), 2);
    assert_eq!(
        entries[0].kind,
        EntryKind::Image {
            width: 8,
            height: 8,
            bit_depth: 4
        }
    );
    assert_eq!(entries[1].kind, EntryKind::Raw);
}
