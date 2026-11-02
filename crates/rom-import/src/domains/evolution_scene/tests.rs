//! Evolution-scene reader tests over a synthetic ROM.
//!
//! One compressed image, one raw palette, and one compressed tilemap are
//! planted under real `evolution-scene/` ids and read back through the domain.
//! The failure tests break one root at a time and require a typed error with
//! nothing queued.

use pack_format::{parse_directory, EntryKind, PackWriter};

use super::write;
use crate::domains::DOMAINS;
use crate::error::{ImportError, Lz77Fault};
use crate::fixture::RomFixture;
use crate::reader::{GbaPtr, ROM_BASE};
use crate::rom::{Rom, ROM_SIZE};
use crate::roots::{BlobRoot, Encoding, EvolutionSceneRoots, ImageRoot, PaletteRoot, Roots};

const LZ_TILES: usize = 0x1000;
const RAW_PALETTE: usize = 0x2000;
const LZ_TILEMAP: usize = 0x3000;

/// A 4bpp tile whose row 0 counts `0..=7`, then seven zero rows.
fn tile() -> Vec<u8> {
    let mut tile = vec![0x10, 0x32, 0x54, 0x76];
    tile.resize(32, 0);
    tile
}

/// An LZ77 stream of `payload` as literals, eight to a flag byte.
fn lz77_literals(payload: &[u8]) -> Vec<u8> {
    let len = u32::try_from(payload.len()).unwrap();
    let mut stream = vec![0x10];
    stream.extend_from_slice(&len.to_le_bytes()[..3]);
    for group in payload.chunks(8) {
        stream.push(0);
        stream.extend_from_slice(group);
    }
    stream
}

fn rom() -> Rom {
    let bytes = RomFixture::new()
        .emerald_header()
        .write(LZ_TILES, &lz77_literals(&tile()))
        .write(RAW_PALETTE, &[0x1F, 0x00, 0xE0, 0x03])
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
    id: "evolution-scene/image/bg",
    addr: at(LZ_TILES),
    encoding: Encoding::Lz77,
    rom_bit_depth: 4,
    pack_bit_depth: 4,
    width: 8,
    height: 8,
    metatile_width: 1,
    metatile_height: 1,
    tile_count: 1,
};
const PALETTE: PaletteRoot = PaletteRoot {
    id: "evolution-scene/palette/bg_anim",
    addr: at(RAW_PALETTE),
    encoding: Encoding::Raw,
    color_count: 2,
};
const TILEMAP: BlobRoot = BlobRoot {
    id: "evolution-scene/raw/bg_inner",
    addr: at(LZ_TILEMAP),
    encoding: Encoding::Lz77,
    len: 8,
};

static IMAGES: &[ImageRoot] = &[IMAGE];
static PALETTES: &[PaletteRoot] = &[PALETTE];
static TILEMAPS: &[BlobRoot] = &[TILEMAP];

fn roots(evolution_scene: EvolutionSceneRoots) -> Roots {
    Roots {
        evolution_scene,
        ..Roots::NONE
    }
}

fn full() -> Roots {
    roots(EvolutionSceneRoots {
        images: IMAGES,
        palettes: PALETTES,
        tilemaps: TILEMAPS,
    })
}

fn written(roots: &Roots) -> Result<Vec<pack_format::DirectoryEntry>, ImportError> {
    let mut writer = PackWriter::new();
    write(&rom(), roots, &mut writer)?;
    let bytes = writer.finish().expect("no duplicate ids");
    Ok(parse_directory(&bytes).expect("a valid pack"))
}

#[test]
fn every_evolution_scene_root_is_emitted_with_its_decoded_bytes() {
    let mut writer = PackWriter::new();
    write(&rom(), &full(), &mut writer).expect("every root is well-formed");
    let bytes = writer.finish().expect("no duplicate ids");
    let entries = parse_directory(&bytes).expect("a valid pack");
    let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "evolution-scene/image/bg",
            "evolution-scene/palette/bg_anim",
            "evolution-scene/raw/bg_inner",
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
    let payload = |index: usize| {
        let entry = &entries[index];
        let start = entry.offset;
        bytes[start..start + entry.length].to_vec()
    };
    assert_eq!(payload(1), [0x1F, 0x00, 0xE0, 0x03]);
    // The compressed tilemap lands as its decoded bytes.
    assert_eq!(payload(2), b"tilemap!");
}

#[test]
fn the_domain_list_dispatches_the_evolution_scene() {
    let rom = rom();
    let mut writer = PackWriter::new();
    for domain in DOMAINS {
        domain(&rom, &full(), &mut writer).expect("every root is well-formed");
    }
    let bytes = writer.finish().expect("no duplicate ids");
    assert_eq!(parse_directory(&bytes).expect("a valid pack").len(), 3);
}

#[test]
fn an_empty_family_writes_nothing() {
    assert!(written(&Roots::NONE).expect("nothing to read").is_empty());
    assert_eq!(Roots::NONE.evolution_scene, EvolutionSceneRoots::NONE);
}

#[test]
fn evolution_scene_images_and_palettes_are_enumerated() {
    let roots = full();
    let images: Vec<&str> = roots.images().map(|root| root.id).collect();
    let palettes: Vec<&str> = roots.palettes().map(|root| root.id).collect();
    assert_eq!(images, ["evolution-scene/image/bg"]);
    assert_eq!(palettes, ["evolution-scene/palette/bg_anim"]);
}

#[test]
fn a_root_past_the_rom_fails_closed() {
    static BAD_PALETTES: &[PaletteRoot] = &[PaletteRoot {
        addr: at(ROM_SIZE - 2),
        ..PALETTE
    }];
    let roots = roots(EvolutionSceneRoots {
        images: IMAGES,
        palettes: BAD_PALETTES,
        ..EvolutionSceneRoots::NONE
    });
    let mut writer = PackWriter::new();
    let err = write(&rom(), &roots, &mut writer).unwrap_err();
    assert!(matches!(err, ImportError::Truncated { .. }), "{err}");
    // The good image read before the bad palette was not queued.
    let bytes = writer.finish().expect("an empty pack");
    assert!(parse_directory(&bytes).expect("a valid pack").is_empty());
}

#[test]
fn a_compressed_tilemap_that_is_not_a_stream_is_a_typed_error() {
    static BAD_TILEMAPS: &[BlobRoot] = &[BlobRoot {
        addr: at(RAW_PALETTE),
        ..TILEMAP
    }];
    let roots = roots(EvolutionSceneRoots {
        tilemaps: BAD_TILEMAPS,
        ..EvolutionSceneRoots::NONE
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
fn a_compressed_tilemap_of_the_wrong_length_is_a_typed_error() {
    static BAD_TILEMAPS: &[BlobRoot] = &[BlobRoot { len: 9, ..TILEMAP }];
    let roots = roots(EvolutionSceneRoots {
        tilemaps: BAD_TILEMAPS,
        ..EvolutionSceneRoots::NONE
    });
    let err = written(&roots).unwrap_err();
    assert!(matches!(err, ImportError::Lz77 { .. }), "{err}");
}
