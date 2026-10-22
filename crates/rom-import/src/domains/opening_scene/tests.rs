//! Opening-scene reader tests over a synthetic ROM.
//!
//! One raw image, one raw and one compressed palette, and one compressed
//! tilemap are planted under real `opening-scene/` ids and read back through
//! the domain. The failure tests break one root at a time and require a typed
//! error with nothing queued.

use pack_format::{parse_directory, EntryKind, PackWriter};

use super::write;
use crate::domains::DOMAINS;
use crate::error::{ImportError, Lz77Fault};
use crate::fixture::RomFixture;
use crate::reader::{GbaPtr, ROM_BASE};
use crate::rom::{Rom, ROM_SIZE};
use crate::roots::{BlobRoot, Encoding, ImageRoot, OpeningSceneRoots, PaletteRoot, Roots};

const TILES: usize = 0x1000;
const RAW_PALETTE: usize = 0x2000;
const LZ_PALETTE: usize = 0x2100;
const LZ_TILEMAP: usize = 0x3000;

/// A 4bpp tile whose row 0 counts `0..=7`, then seven zero rows.
fn tile() -> Vec<u8> {
    let mut tile = vec![0x10, 0x32, 0x54, 0x76];
    tile.resize(32, 0);
    tile
}

/// An LZ77 stream of `payload` (at most eight bytes) as literals.
fn lz77_literals(payload: &[u8]) -> Vec<u8> {
    assert!(payload.len() <= 8);
    let len = u32::try_from(payload.len()).unwrap();
    let mut stream = vec![0x10];
    stream.extend_from_slice(&len.to_le_bytes()[..3]);
    stream.push(0);
    stream.extend_from_slice(payload);
    stream
}

fn rom() -> Rom {
    let bytes = RomFixture::new()
        .emerald_header()
        .write(TILES, &tile())
        .write(RAW_PALETTE, &[0x1F, 0x00, 0xE0, 0x03])
        .write(LZ_PALETTE, &lz77_literals(&[0x00, 0x7C, 0xFF, 0x7F]))
        .write(LZ_TILEMAP, &lz77_literals(b"tilemap!"))
        .finish();
    Rom::from_bytes(bytes).expect("the fixture header is valid")
}

/// The bus address of a fixture offset.
const fn at(off: usize) -> GbaPtr {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "fixture offsets are far below u32::MAX"
    )]
    GbaPtr::at(ROM_BASE + off as u32)
}

const IMAGE: ImageRoot = ImageRoot {
    id: "opening-scene/image/birch",
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
const RAW_PALETTE_ROOT: PaletteRoot = PaletteRoot {
    id: "opening-scene/palette/birch",
    addr: at(RAW_PALETTE),
    encoding: Encoding::Raw,
    color_count: 2,
};
const LZ_PALETTE_ROOT: PaletteRoot = PaletteRoot {
    id: "opening-scene/palette/lotad",
    addr: at(LZ_PALETTE),
    encoding: Encoding::Lz77,
    color_count: 2,
};
const TILEMAP: BlobRoot = BlobRoot {
    id: "opening-scene/raw/clock_start",
    addr: at(LZ_TILEMAP),
    encoding: Encoding::Lz77,
    len: 8,
};

fn roots(opening_scene: OpeningSceneRoots) -> Roots {
    Roots {
        opening_scene,
        ..Roots::NONE
    }
}

fn written(roots: &Roots) -> Result<Vec<pack_format::DirectoryEntry>, ImportError> {
    let mut writer = PackWriter::new();
    write(&rom(), roots, &mut writer)?;
    let bytes = writer.finish().expect("no duplicate ids");
    Ok(parse_directory(&bytes).expect("a valid pack"))
}

static IMAGES: &[ImageRoot] = &[IMAGE];
static PALETTES: &[PaletteRoot] = &[RAW_PALETTE_ROOT, LZ_PALETTE_ROOT];
static TILEMAPS: &[BlobRoot] = &[TILEMAP];

fn full() -> Roots {
    roots(OpeningSceneRoots {
        images: IMAGES,
        palettes: PALETTES,
        tilemaps: TILEMAPS,
    })
}

#[test]
fn every_opening_scene_root_is_emitted_with_its_decoded_bytes() {
    let mut writer = PackWriter::new();
    write(&rom(), &full(), &mut writer).expect("every root is well-formed");
    let bytes = writer.finish().expect("no duplicate ids");
    let entries = parse_directory(&bytes).expect("a valid pack");
    let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "opening-scene/image/birch",
            "opening-scene/palette/birch",
            "opening-scene/palette/lotad",
            "opening-scene/raw/clock_start",
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
    assert_eq!(entries[2].kind, EntryKind::Palette { color_count: 2 });
    assert_eq!(entries[3].kind, EntryKind::Raw);
    let payload = |index: usize| {
        let entry = &entries[index];
        let start = entry.offset;
        bytes[start..start + entry.length].to_vec()
    };
    // The compressed palette and tilemap land as their decoded bytes.
    assert_eq!(payload(2), [0x00, 0x7C, 0xFF, 0x7F]);
    assert_eq!(payload(3), b"tilemap!");
}

#[test]
fn the_domain_list_dispatches_the_opening_scene() {
    let rom = rom();
    let mut writer = PackWriter::new();
    for domain in DOMAINS {
        domain(&rom, &full(), &mut writer).expect("every root is well-formed");
    }
    let bytes = writer.finish().expect("no duplicate ids");
    assert_eq!(parse_directory(&bytes).expect("a valid pack").len(), 4);
}

#[test]
fn an_empty_family_writes_nothing() {
    assert!(written(&Roots::NONE).expect("nothing to read").is_empty());
    assert_eq!(Roots::NONE.opening_scene, OpeningSceneRoots::NONE);
}

#[test]
fn opening_scene_images_and_palettes_are_enumerated() {
    let roots = full();
    let images: Vec<&str> = roots.images().map(|root| root.id).collect();
    let palettes: Vec<&str> = roots.palettes().map(|root| root.id).collect();
    assert_eq!(images, ["opening-scene/image/birch"]);
    assert_eq!(
        palettes,
        ["opening-scene/palette/birch", "opening-scene/palette/lotad"]
    );
}

#[test]
fn a_root_past_the_rom_fails_closed() {
    static BAD_PALETTES: &[PaletteRoot] = &[PaletteRoot {
        addr: at(ROM_SIZE - 2),
        ..RAW_PALETTE_ROOT
    }];
    let roots = roots(OpeningSceneRoots {
        images: IMAGES,
        palettes: BAD_PALETTES,
        ..OpeningSceneRoots::NONE
    });
    let mut writer = PackWriter::new();
    let err = write(&rom(), &roots, &mut writer).unwrap_err();
    assert!(matches!(err, ImportError::Truncated { .. }), "{err}");
    // The good image read before the bad palette was not queued.
    let bytes = writer.finish().expect("an empty pack");
    assert!(parse_directory(&bytes).expect("a valid pack").is_empty());
}

#[test]
fn a_compressed_palette_that_is_not_a_stream_is_a_typed_error() {
    static BAD_PALETTES: &[PaletteRoot] = &[PaletteRoot {
        addr: at(RAW_PALETTE),
        ..LZ_PALETTE_ROOT
    }];
    let roots = roots(OpeningSceneRoots {
        palettes: BAD_PALETTES,
        ..OpeningSceneRoots::NONE
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
fn a_compressed_palette_of_the_wrong_length_is_a_typed_error() {
    static BAD_PALETTES: &[PaletteRoot] = &[PaletteRoot {
        color_count: 3,
        ..LZ_PALETTE_ROOT
    }];
    let roots = roots(OpeningSceneRoots {
        palettes: BAD_PALETTES,
        ..OpeningSceneRoots::NONE
    });
    let err = written(&roots).unwrap_err();
    assert!(matches!(err, ImportError::Lz77 { .. }), "{err}");
}
