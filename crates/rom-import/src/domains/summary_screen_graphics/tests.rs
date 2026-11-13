//! Summary-screen graphics reader tests over a synthetic ROM.
//!
//! All sixteen roots are planted under the real `summary-screen-graphics/` ids
//! at their production shapes and encodings, then read back through the
//! domain. The failure tests break one root at a time and require a typed
//! error with nothing queued.

use pack_format::{parse_directory, DirectoryEntry, EntryKind, PackWriter};

use super::write;
use crate::domains::DOMAINS;
use crate::error::{ImportError, Lz77Fault};
use crate::fixture::RomFixture;
use crate::reader::{GbaPtr, ROM_BASE};
use crate::rom::{Rom, ROM_SIZE};
use crate::roots::{BlobRoot, Encoding, ImageRoot, PaletteRoot, Roots, SummaryScreenGraphicsRoots};

/// Bytes between planted payloads, wide enough for the largest stream.
const SLOT: usize = 0x4000;

const TILE_SHEET_TILES: u32 = 240;
const TILE_COLORS: u16 = 8 * 16;
/// Where the two button rows sit: `sButtons_Gfx` is one array.
const BUTTONS: usize = 3 * SLOT;
const BUTTON_BYTES: usize = 4 * 32;

/// Distinct little-endian colour bytes for `count` colours.
fn colors(count: u16, seed: u16) -> Vec<u8> {
    (0..count)
        .flat_map(|color| (seed + color).to_le_bytes())
        .collect()
}

/// `len` bytes counting up from `seed`.
fn counting(len: usize, seed: u8) -> Vec<u8> {
    (0..len)
        .map(|index| seed.wrapping_add(u8::try_from(index % 251).unwrap()))
        .collect()
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

/// One planted payload: its pack id, the bytes the importer must emit, and
/// how the ROM holds them.
struct Planted {
    id: &'static str,
    decoded: Vec<u8>,
    encoding: Encoding,
    offset: usize,
}

/// The sixteen roots, in extraction order; the second button sits right after
/// the first, as the array lays them out.
fn planted() -> Vec<Planted> {
    let mut next = 0;
    let mut plant = |id, decoded, encoding| {
        next += 1;
        Planted {
            id,
            decoded,
            encoding,
            offset: next * SLOT,
        }
    };
    let mut roots = vec![
        plant(
            "summary-screen-graphics/image/tiles",
            counting(TILE_SHEET_TILES as usize * 32, 3),
            Encoding::Lz77,
        ),
        plant(
            "summary-screen-graphics/image/move_select",
            counting(32 * 32, 5),
            Encoding::Lz77,
        ),
        plant(
            "summary-screen-graphics/image/a_button",
            counting(BUTTON_BYTES, 9),
            Encoding::Raw,
        ),
        plant(
            "summary-screen-graphics/image/b_button",
            counting(BUTTON_BYTES, 13),
            Encoding::Raw,
        ),
        plant(
            "summary-screen-graphics/palette/tiles",
            colors(TILE_COLORS, 0x100),
            Encoding::Lz77,
        ),
        plant(
            "summary-screen-graphics/palette/move_select",
            colors(16, 0x200),
            Encoding::Lz77,
        ),
        plant(
            "summary-screen-graphics/palette/markings",
            colors(16, 0x300),
            Encoding::Raw,
        ),
    ];
    for (id, seed) in [
        ("summary-screen-graphics/raw/page_info", 11),
        ("summary-screen-graphics/raw/page_info_egg", 12),
        ("summary-screen-graphics/raw/page_skills", 13),
        ("summary-screen-graphics/raw/page_battle_moves", 14),
        ("summary-screen-graphics/raw/page_contest_moves", 15),
    ] {
        roots.push(plant(id, counting(32 * 32 * 2, seed), Encoding::Lz77));
    }
    for (id, len, seed) in [
        ("summary-screen-graphics/raw/effect_battle", 80 * 2, 21),
        ("summary-screen-graphics/raw/effect_contest", 80 * 2, 22),
        ("summary-screen-graphics/raw/effect_cancel", 64 * 2, 23),
        ("summary-screen-graphics/raw/status_tilemap", 10 * 2 * 2, 24),
    ] {
        roots.push(plant(id, counting(len, seed), Encoding::Raw));
    }
    // The buttons are one array: the second row follows the first.
    roots[2].offset = BUTTONS;
    roots[3].offset = BUTTONS + BUTTON_BYTES;
    roots
}

fn rom() -> Rom {
    let mut fixture = RomFixture::new().emerald_header();
    for item in planted() {
        let stored = match item.encoding {
            Encoding::Lz77 => lz77_literals(&item.decoded),
            Encoding::Raw => item.decoded.clone(),
        };
        fixture = fixture.write(item.offset, &stored);
    }
    Rom::from_bytes(fixture.finish()).expect("the fixture header is valid")
}

/// The bus address of a fixture offset.
const fn at(off: usize) -> GbaPtr {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "fixture offsets are far below u32::MAX"
    )]
    GbaPtr::at(ROM_BASE + off as u32)
}

const fn image(
    id: &'static str,
    offset: usize,
    encoding: Encoding,
    (width, height, tile_count): (u32, u32, u32),
) -> ImageRoot {
    ImageRoot {
        id,
        addr: at(offset),
        encoding,
        rom_bit_depth: 4,
        pack_bit_depth: 4,
        width,
        height,
        metatile_width: 1,
        metatile_height: 1,
        tile_count,
    }
}

const fn palette(
    id: &'static str,
    offset: usize,
    encoding: Encoding,
    color_count: u16,
) -> PaletteRoot {
    PaletteRoot {
        id,
        addr: at(offset),
        encoding,
        color_count,
    }
}

const fn map(id: &'static str, offset: usize, encoding: Encoding, len: u32) -> BlobRoot {
    BlobRoot {
        id,
        addr: at(offset),
        encoding,
        len,
    }
}

static IMAGES: &[ImageRoot] = &[
    image(
        "summary-screen-graphics/image/tiles",
        SLOT,
        Encoding::Lz77,
        (128, 120, TILE_SHEET_TILES),
    ),
    image(
        "summary-screen-graphics/image/move_select",
        2 * SLOT,
        Encoding::Lz77,
        (16, 128, 32),
    ),
    image(
        "summary-screen-graphics/image/a_button",
        BUTTONS,
        Encoding::Raw,
        (16, 16, 4),
    ),
    image(
        "summary-screen-graphics/image/b_button",
        BUTTONS + BUTTON_BYTES,
        Encoding::Raw,
        (16, 16, 4),
    ),
];
static PALETTES: &[PaletteRoot] = &[
    palette(
        "summary-screen-graphics/palette/tiles",
        5 * SLOT,
        Encoding::Lz77,
        TILE_COLORS,
    ),
    palette(
        "summary-screen-graphics/palette/move_select",
        6 * SLOT,
        Encoding::Lz77,
        16,
    ),
    palette(
        "summary-screen-graphics/palette/markings",
        7 * SLOT,
        Encoding::Raw,
        16,
    ),
];
static MAPS: &[BlobRoot] = &[
    map(
        "summary-screen-graphics/raw/page_info",
        8 * SLOT,
        Encoding::Lz77,
        2048,
    ),
    map(
        "summary-screen-graphics/raw/page_info_egg",
        9 * SLOT,
        Encoding::Lz77,
        2048,
    ),
    map(
        "summary-screen-graphics/raw/page_skills",
        10 * SLOT,
        Encoding::Lz77,
        2048,
    ),
    map(
        "summary-screen-graphics/raw/page_battle_moves",
        11 * SLOT,
        Encoding::Lz77,
        2048,
    ),
    map(
        "summary-screen-graphics/raw/page_contest_moves",
        12 * SLOT,
        Encoding::Lz77,
        2048,
    ),
    map(
        "summary-screen-graphics/raw/effect_battle",
        13 * SLOT,
        Encoding::Raw,
        160,
    ),
    map(
        "summary-screen-graphics/raw/effect_contest",
        14 * SLOT,
        Encoding::Raw,
        160,
    ),
    map(
        "summary-screen-graphics/raw/effect_cancel",
        15 * SLOT,
        Encoding::Raw,
        128,
    ),
    map(
        "summary-screen-graphics/raw/status_tilemap",
        16 * SLOT,
        Encoding::Raw,
        40,
    ),
];

fn roots(summary_screen_graphics: SummaryScreenGraphicsRoots) -> Roots {
    Roots {
        summary_screen_graphics,
        ..Roots::NONE
    }
}

fn full() -> Roots {
    roots(SummaryScreenGraphicsRoots {
        images: IMAGES,
        palettes: PALETTES,
        tilemaps: MAPS,
    })
}

fn written(roots: &Roots) -> Result<(Vec<u8>, Vec<DirectoryEntry>), ImportError> {
    let mut writer = PackWriter::new();
    write(&rom(), roots, &mut writer)?;
    let bytes = writer.finish().expect("no duplicate ids");
    let entries = parse_directory(&bytes).expect("a valid pack");
    Ok((bytes, entries))
}

fn payload<'a>(bytes: &'a [u8], entries: &'a [DirectoryEntry], id: &str) -> &'a [u8] {
    let entry = entries
        .iter()
        .find(|entry| entry.id == id)
        .expect("an emitted id");
    &bytes[entry.offset..entry.offset + entry.length]
}

#[test]
fn the_fixture_offsets_match_the_roots() {
    for item in planted() {
        let in_roots = IMAGES
            .iter()
            .map(|root| (root.id, root.addr))
            .chain(PALETTES.iter().map(|root| (root.id, root.addr)))
            .chain(MAPS.iter().map(|root| (root.id, root.addr)))
            .find(|(id, _)| *id == item.id)
            .expect("every planted id has a root");
        assert_eq!(in_roots.1, at(item.offset), "{}", item.id);
    }
    assert_eq!(planted().len(), IMAGES.len() + PALETTES.len() + MAPS.len());
}

#[test]
fn every_summary_screen_root_is_emitted_with_its_decoded_bytes() {
    let (bytes, entries) = written(&full()).expect("every root is well-formed");
    let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
    let mut expected: Vec<&str> = planted().iter().map(|item| item.id).collect();
    expected.sort_unstable();
    assert_eq!(ids, expected);
    let kind = |id: &str| {
        entries
            .iter()
            .find(|entry| entry.id == id)
            .expect("an emitted id")
            .kind
    };
    for (id, width, height) in [
        ("summary-screen-graphics/image/tiles", 128, 120),
        ("summary-screen-graphics/image/move_select", 16, 128),
        ("summary-screen-graphics/image/a_button", 16, 16),
        ("summary-screen-graphics/image/b_button", 16, 16),
    ] {
        assert_eq!(
            kind(id),
            EntryKind::Image {
                width,
                height,
                bit_depth: 4
            },
            "{id}"
        );
    }
    assert_eq!(
        kind("summary-screen-graphics/palette/tiles"),
        EntryKind::Palette { color_count: 128 }
    );
    for id in [
        "summary-screen-graphics/palette/move_select",
        "summary-screen-graphics/palette/markings",
    ] {
        assert_eq!(kind(id), EntryKind::Palette { color_count: 16 }, "{id}");
    }
    // Palettes and maps pass through the ROM bytes unchanged, whether the ROM
    // holds them raw or as a stream.
    for item in planted().iter().filter(|item| !item.id.contains("/image/")) {
        assert_eq!(
            payload(&bytes, &entries, item.id),
            item.decoded,
            "{}",
            item.id
        );
    }
    // Every effect map is published in full, including the entries its window
    // never reads.
    for (id, len) in [
        ("summary-screen-graphics/raw/effect_battle", 160),
        ("summary-screen-graphics/raw/effect_contest", 160),
        ("summary-screen-graphics/raw/effect_cancel", 128),
    ] {
        assert_eq!(payload(&bytes, &entries, id).len(), len, "{id}");
    }
}

#[test]
fn the_tile_sheet_is_all_two_hundred_forty_tiles_in_tile_order() {
    let (bytes, entries) = written(&full()).expect("every root is well-formed");
    let pixels = payload(&bytes, &entries, "summary-screen-graphics/image/tiles");
    let tiles = counting(240 * 32, 3);
    assert_eq!(pixels.len(), 128 * 120);
    for tile in 0..240usize {
        let (tile_column, tile_row) = (tile % 16, tile / 16);
        for row in 0..8usize {
            for column in 0..8usize {
                let byte = tiles[tile * 32 + row * 4 + column / 2];
                // The low nibble is the left pixel.
                let expected = if column % 2 == 0 {
                    byte & 0x0F
                } else {
                    byte >> 4
                };
                assert_eq!(
                    pixels[(tile_row * 8 + row) * 128 + tile_column * 8 + column],
                    expected,
                    "tile {tile} row {row} column {column}"
                );
            }
        }
    }
}

#[test]
fn the_buttons_are_two_raw_four_tile_sheets_of_one_array() {
    let (bytes, entries) = written(&full()).expect("every root is well-formed");
    for (id, seed) in [
        ("summary-screen-graphics/image/a_button", 9),
        ("summary-screen-graphics/image/b_button", 13),
    ] {
        let pixels = payload(&bytes, &entries, id);
        let tiles = counting(BUTTON_BYTES, seed);
        assert_eq!(pixels.len(), 16 * 16, "{id}");
        assert_eq!(pixels[0], tiles[0] & 0x0F, "{id}");
        assert_eq!(pixels[1], tiles[0] >> 4, "{id}");
        // The second tile starts eight pixels along the first row of tiles.
        assert_eq!(pixels[8], tiles[32] & 0x0F, "{id}");
        // The third starts on the ninth raster row.
        assert_eq!(pixels[8 * 16], tiles[64] & 0x0F, "{id}");
    }
}

#[test]
fn every_tile_palette_colour_survives() {
    let (bytes, entries) = written(&full()).expect("every root is well-formed");
    let colours = payload(&bytes, &entries, "summary-screen-graphics/palette/tiles");
    assert_eq!(colours.len(), 128 * 2);
    assert_eq!(&colours[127 * 2..], 0x17F_u16.to_le_bytes());
}

#[test]
fn the_domain_list_dispatches_the_summary_screen_graphics() {
    let rom = rom();
    let mut writer = PackWriter::new();
    for domain in DOMAINS {
        domain(&rom, &full(), &mut writer).expect("every root is well-formed");
    }
    let bytes = writer.finish().expect("no duplicate ids");
    assert_eq!(parse_directory(&bytes).expect("a valid pack").len(), 16);
}

#[test]
fn an_empty_family_writes_nothing() {
    let (_, entries) = written(&Roots::NONE).expect("nothing to read");
    assert!(entries.is_empty());
    assert_eq!(
        Roots::NONE.summary_screen_graphics,
        SummaryScreenGraphicsRoots::NONE
    );
}

#[test]
fn summary_screen_images_and_palettes_are_enumerated() {
    let roots = full();
    let images: Vec<&str> = roots.images().map(|root| root.id).collect();
    let palettes: Vec<&str> = roots.palettes().map(|root| root.id).collect();
    assert_eq!(
        images,
        [
            "summary-screen-graphics/image/tiles",
            "summary-screen-graphics/image/move_select",
            "summary-screen-graphics/image/a_button",
            "summary-screen-graphics/image/b_button",
        ]
    );
    assert_eq!(
        palettes,
        [
            "summary-screen-graphics/palette/tiles",
            "summary-screen-graphics/palette/move_select",
            "summary-screen-graphics/palette/markings",
        ]
    );
}

#[test]
fn a_late_map_past_the_rom_leaves_nothing_queued() {
    static BAD_MAPS: &[BlobRoot] = &[
        MAPS[0],
        BlobRoot {
            addr: at(ROM_SIZE - 2),
            ..MAPS[8]
        },
    ];
    let roots = roots(SummaryScreenGraphicsRoots {
        images: IMAGES,
        palettes: PALETTES,
        tilemaps: BAD_MAPS,
    });
    let mut writer = PackWriter::new();
    let err = write(&rom(), &roots, &mut writer).unwrap_err();
    assert!(matches!(err, ImportError::Truncated { .. }), "{err}");
    let bytes = writer.finish().expect("an empty pack");
    assert!(parse_directory(&bytes).expect("a valid pack").is_empty());
}

#[test]
fn a_compressed_map_that_is_not_a_stream_is_a_typed_error() {
    static BAD_MAPS: &[BlobRoot] = &[BlobRoot {
        addr: at(13 * SLOT),
        ..MAPS[0]
    }];
    let roots = roots(SummaryScreenGraphicsRoots {
        tilemaps: BAD_MAPS,
        ..SummaryScreenGraphicsRoots::NONE
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
fn a_compressed_sheet_of_the_wrong_length_is_a_typed_error() {
    static BAD_IMAGES: &[ImageRoot] = &[ImageRoot {
        tile_count: 239,
        ..IMAGES[0]
    }];
    let roots = roots(SummaryScreenGraphicsRoots {
        images: BAD_IMAGES,
        ..SummaryScreenGraphicsRoots::NONE
    });
    let err = written(&roots).unwrap_err();
    assert!(matches!(err, ImportError::Lz77 { .. }), "{err}");
}
