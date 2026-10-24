//! Battle-interface reader tests over a synthetic ROM.
//!
//! The five records are planted under the real `battle-interface/` ids at
//! their production shapes: a compressed 128x128 sheet, a compressed
//! 32-colour textbox palette whose two banks differ, a compressed and a raw
//! 16-colour text palette, and a compressed 4096-byte tilemap. The failure
//! tests break one root at a time and require a typed error with nothing
//! queued.

use pack_format::{parse_directory, DirectoryEntry, EntryKind, PackWriter};

use super::write;
use crate::domains::DOMAINS;
use crate::error::{ImportError, Lz77Fault};
use crate::fixture::RomFixture;
use crate::reader::{GbaPtr, ROM_BASE};
use crate::rom::{Rom, ROM_SIZE};
use crate::roots::{BattleInterfaceRoots, BlobRoot, Encoding, ImageRoot, PaletteRoot, Roots};

const SHEET: usize = 0x1_0000;
const TEXTBOX_PALETTE: usize = 0x2_0000;
const WINDOW_TEXT_PALETTE: usize = 0x2_1000;
const PP_TEXT_PALETTE: usize = 0x2_2000;
const TILEMAP_AT: usize = 0x3_0000;

const SHEET_TILES: usize = 256;
const TILEMAP_LEN: usize = 4096;
const PIXELS_PER_SHEET_ROW: usize = 128;

/// 4bpp tiles where tile `n` is uniformly colour `n % 16`, so the sheet's
/// 16x16 tile grid makes every pixel's expected index computable from its
/// position alone.
fn sheet_tiles() -> Vec<u8> {
    (0..SHEET_TILES)
        .flat_map(|tile| {
            let colour = u8::try_from(tile % 16).expect("a nibble");
            [colour | colour << 4; 32]
        })
        .collect()
}

/// A sixteen-colour BGR555 bank starting at `first`, as ROM bytes.
fn bank(first: u16) -> Vec<u8> {
    (0..16u16)
        .flat_map(|n| (first + n * 0x21).to_le_bytes())
        .collect()
}

fn textbox_palette() -> Vec<u8> {
    [bank(0x0100), bank(0x4000)].concat()
}

fn tilemap() -> Vec<u8> {
    (0..TILEMAP_LEN)
        .map(|n| u8::try_from(n * 7 % 251).expect("below 251"))
        .collect()
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

fn rom() -> Rom {
    let bytes = RomFixture::new()
        .emerald_header()
        .write(SHEET, &lz77_literals(&sheet_tiles()))
        .write(TEXTBOX_PALETTE, &lz77_literals(&textbox_palette()))
        .write(WINDOW_TEXT_PALETTE, &lz77_literals(&bank(0x0200)))
        .write(PP_TEXT_PALETTE, &bank(0x0300))
        .write(TILEMAP_AT, &lz77_literals(&tilemap()))
        .finish();
    Rom::from_bytes(bytes).expect("the fixture header is valid")
}

/// The bus address of a fixture offset.
#[expect(
    clippy::cast_possible_truncation,
    reason = "fixture offsets are far below u32::MAX"
)]
const fn at(off: usize) -> GbaPtr {
    GbaPtr::at(ROM_BASE + off as u32)
}

const TEXTBOX_IMAGE: ImageRoot = ImageRoot {
    id: "battle-interface/image/textbox",
    addr: at(SHEET),
    encoding: Encoding::Lz77,
    rom_bit_depth: 4,
    pack_bit_depth: 4,
    width: 128,
    height: 128,
    metatile_width: 1,
    metatile_height: 1,
    tile_count: 256,
};
const TEXTBOX_PALETTE_ROOT: PaletteRoot = PaletteRoot {
    id: "battle-interface/palette/textbox",
    addr: at(TEXTBOX_PALETTE),
    encoding: Encoding::Lz77,
    color_count: 32,
};
const WINDOW_TEXT_ROOT: PaletteRoot = PaletteRoot {
    id: "battle-interface/palette/window_text",
    addr: at(WINDOW_TEXT_PALETTE),
    encoding: Encoding::Lz77,
    color_count: 16,
};
const PP_TEXT_ROOT: PaletteRoot = PaletteRoot {
    id: "battle-interface/palette/pp_text",
    addr: at(PP_TEXT_PALETTE),
    encoding: Encoding::Raw,
    color_count: 16,
};
const TEXTBOX_MAP: BlobRoot = BlobRoot {
    id: "battle-interface/raw/textbox_map",
    addr: at(TILEMAP_AT),
    encoding: Encoding::Lz77,
    len: 4096,
};

static IMAGES: &[ImageRoot] = &[TEXTBOX_IMAGE];
static PALETTES: &[PaletteRoot] = &[TEXTBOX_PALETTE_ROOT, WINDOW_TEXT_ROOT, PP_TEXT_ROOT];
static TILEMAPS: &[BlobRoot] = &[TEXTBOX_MAP];

fn full() -> Roots {
    roots(BattleInterfaceRoots {
        images: IMAGES,
        palettes: PALETTES,
        tilemaps: TILEMAPS,
    })
}

fn roots(battle_interface: BattleInterfaceRoots) -> Roots {
    Roots {
        battle_interface,
        ..Roots::NONE
    }
}

fn written(roots: &Roots) -> Result<(Vec<u8>, Vec<DirectoryEntry>), ImportError> {
    let mut writer = PackWriter::new();
    write(&rom(), roots, &mut writer)?;
    let bytes = writer.finish().expect("no duplicate ids");
    let entries = parse_directory(&bytes).expect("a valid pack");
    Ok((bytes, entries))
}

fn payload<'a>(bytes: &'a [u8], entries: &[DirectoryEntry], id: &str) -> &'a [u8] {
    let entry = entries.iter().find(|e| e.id == id).expect("entry exists");
    &bytes[entry.offset..entry.offset + entry.length]
}

fn palette_colours(bytes: &[u8]) -> Vec<u16> {
    bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect()
}

#[test]
fn the_five_records_are_emitted_with_their_production_shapes() {
    let (_, entries) = written(&full()).expect("every root is well-formed");
    let shapes: Vec<(&str, EntryKind)> = entries.iter().map(|e| (e.id.as_str(), e.kind)).collect();
    assert_eq!(
        shapes,
        [
            (
                "battle-interface/image/textbox",
                EntryKind::Image {
                    width: 128,
                    height: 128,
                    bit_depth: 4
                }
            ),
            (
                "battle-interface/palette/pp_text",
                EntryKind::Palette { color_count: 16 }
            ),
            (
                "battle-interface/palette/textbox",
                EntryKind::Palette { color_count: 32 }
            ),
            (
                "battle-interface/palette/window_text",
                EntryKind::Palette { color_count: 16 }
            ),
            ("battle-interface/raw/textbox_map", EntryKind::Raw),
        ]
    );
}

#[test]
fn the_textbox_sheet_lands_one_byte_per_pixel_in_tile_order() {
    let (bytes, entries) = written(&full()).expect("every root is well-formed");
    let pixels = payload(&bytes, &entries, "battle-interface/image/textbox");
    assert_eq!(pixels.len(), 128 * 128);
    for (index, &pixel) in pixels.iter().enumerate() {
        let (x, y) = (index % PIXELS_PER_SHEET_ROW, index / PIXELS_PER_SHEET_ROW);
        let tile = (y / 8) * 16 + x / 8;
        assert_eq!(usize::from(pixel), tile % 16, "pixel ({x}, {y})");
    }
}

#[test]
fn the_composed_textbox_palette_is_one_root_holding_both_banks_in_order() {
    let (bytes, entries) = written(&full()).expect("every root is well-formed");
    let textbox = payload(&bytes, &entries, "battle-interface/palette/textbox");
    assert_eq!(textbox, textbox_palette());
    let colours = palette_colours(textbox);
    assert_eq!(colours[..16], palette_colours(&bank(0x0100)));
    assert_eq!(colours[16..], palette_colours(&bank(0x4000)));
}

#[test]
fn the_text_palettes_and_the_tilemap_land_as_their_decoded_bytes() {
    let (bytes, entries) = written(&full()).expect("every root is well-formed");
    assert_eq!(
        payload(&bytes, &entries, "battle-interface/palette/window_text"),
        bank(0x0200)
    );
    assert_eq!(
        payload(&bytes, &entries, "battle-interface/palette/pp_text"),
        bank(0x0300)
    );
    assert_eq!(
        payload(&bytes, &entries, "battle-interface/raw/textbox_map"),
        tilemap()
    );
}

#[test]
fn the_domain_list_dispatches_the_battle_interface() {
    let rom = rom();
    let mut writer = PackWriter::new();
    for domain in DOMAINS {
        domain(&rom, &full(), &mut writer).expect("every root is well-formed");
    }
    let bytes = writer.finish().expect("no duplicate ids");
    let entries = parse_directory(&bytes).expect("a valid pack");
    assert_eq!(entries.len(), 5);
}

#[test]
fn an_empty_family_writes_nothing() {
    let (_, entries) = written(&Roots::NONE).expect("nothing to read");
    assert!(entries.is_empty());
    assert_eq!(Roots::NONE.battle_interface, BattleInterfaceRoots::NONE);
}

#[test]
fn the_whole_root_iterators_enumerate_the_image_and_the_three_palettes() {
    let roots = full();
    let images: Vec<&str> = roots.images().map(|root| root.id).collect();
    let palettes: Vec<&str> = roots.palettes().map(|root| root.id).collect();
    assert_eq!(images, ["battle-interface/image/textbox"]);
    assert_eq!(
        palettes,
        [
            "battle-interface/palette/textbox",
            "battle-interface/palette/window_text",
            "battle-interface/palette/pp_text"
        ]
    );
}

#[test]
fn a_root_past_the_rom_fails_with_nothing_queued() {
    static LATE_PALETTES: &[PaletteRoot] = &[PaletteRoot {
        addr: at(ROM_SIZE - 2),
        ..PP_TEXT_ROOT
    }];
    let roots = roots(BattleInterfaceRoots {
        images: IMAGES,
        palettes: LATE_PALETTES,
        tilemaps: TILEMAPS,
    });
    let mut writer = PackWriter::new();
    let err = write(&rom(), &roots, &mut writer).unwrap_err();
    assert!(matches!(err, ImportError::Truncated { .. }), "{err}");
    // The good image read before the bad palette was not queued.
    let bytes = writer.finish().expect("an empty pack");
    assert!(parse_directory(&bytes).expect("a valid pack").is_empty());
}

#[test]
fn a_compressed_root_that_is_not_a_stream_is_a_typed_error() {
    // The raw PP palette bytes do not begin with the BIOS type byte.
    static TILEMAPS_AT_PALETTE: &[BlobRoot] = &[BlobRoot {
        addr: at(PP_TEXT_PALETTE),
        ..TEXTBOX_MAP
    }];
    let roots = roots(BattleInterfaceRoots {
        tilemaps: TILEMAPS_AT_PALETTE,
        ..BattleInterfaceRoots::NONE
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
fn a_stream_of_the_wrong_length_is_a_size_mismatch() {
    static SHORT_MAP: &[BlobRoot] = &[BlobRoot {
        len: 2048,
        ..TEXTBOX_MAP
    }];
    static SHORT_PALETTE: &[PaletteRoot] = &[PaletteRoot {
        color_count: 16,
        ..TEXTBOX_PALETTE_ROOT
    }];
    static SHORT_IMAGE: &[ImageRoot] = &[ImageRoot {
        tile_count: 128,
        height: 64,
        ..TEXTBOX_IMAGE
    }];
    let cases = [
        BattleInterfaceRoots {
            tilemaps: SHORT_MAP,
            ..BattleInterfaceRoots::NONE
        },
        BattleInterfaceRoots {
            palettes: SHORT_PALETTE,
            ..BattleInterfaceRoots::NONE
        },
        BattleInterfaceRoots {
            images: SHORT_IMAGE,
            ..BattleInterfaceRoots::NONE
        },
    ];
    for family in cases {
        let err = written(&roots(family)).unwrap_err();
        assert!(
            matches!(
                err,
                ImportError::Lz77 {
                    fault: Lz77Fault::SizeMismatch { .. },
                    ..
                }
            ),
            "{err}"
        );
    }
}

#[test]
fn tiles_that_overfill_the_raster_are_a_shape_error() {
    // Raw, so the length check passes and the raster check is what fires.
    static OVERFULL: &[ImageRoot] = &[ImageRoot {
        encoding: Encoding::Raw,
        tile_count: 257,
        ..TEXTBOX_IMAGE
    }];
    let roots = roots(BattleInterfaceRoots {
        images: OVERFULL,
        ..BattleInterfaceRoots::NONE
    });
    let err = written(&roots).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::EntryShape {
                id: "battle-interface/image/textbox",
                ..
            }
        ),
        "{err}"
    );
}
