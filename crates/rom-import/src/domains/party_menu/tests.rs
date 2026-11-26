//! Party-menu reader tests over a synthetic ROM.
//!
//! All fifteen roots are planted under the real `party-menu/` ids at their
//! production shapes and encodings, then read back through the domain. The
//! failure tests break one root at a time and require a typed error with
//! nothing queued.

use pack_format::{parse_directory, DirectoryEntry, EntryKind, PackWriter};

use super::write;
use crate::domains::DOMAINS;
use crate::error::{ImportError, Lz77Fault};
use crate::fixture::RomFixture;
use crate::reader::{GbaPtr, ROM_BASE};
use crate::rom::{Rom, ROM_SIZE};
use crate::roots::{BlobRoot, Encoding, ImageRoot, PaletteRoot, PartyMenuRoots, Roots};

/// Bytes between planted payloads, wide enough for the largest stream.
const SLOT: usize = 0x4000;

const BACKGROUND_TILES: u32 = 62;
const BACKGROUND_COLORS: u16 = 11 * 16;
const SMALL_BALL_TILES: u32 = 24;

/// The decompressed 4bpp tile data of the synthetic background: every tile
/// differs from every other, and the bytes within a tile differ too.
fn background_tiles() -> Vec<u8> {
    (0..BACKGROUND_TILES)
        .flat_map(|tile| {
            (0..32u32).map(move |byte| u8::try_from((tile * 7 + byte * 5) % 251).unwrap())
        })
        .collect()
}

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

/// The small Poké Ball sheet's decompressed tiles: all of tile `t` is the
/// colour `t % 16`, so a pixel's value names the tile it came from.
fn small_ball_tiles() -> Vec<u8> {
    (0..SMALL_BALL_TILES)
        .flat_map(|tile| {
            let color = u8::try_from(tile % 16).unwrap();
            [(color << 4) | color; 32]
        })
        .collect()
}

/// The fifteen roots in the order the planted offsets run.
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
    vec![
        plant("party-menu/image/bg", background_tiles(), Encoding::Lz77),
        plant(
            "party-menu/image/pokeball",
            counting(32 * 32, 3),
            Encoding::Lz77,
        ),
        plant(
            "party-menu/image/hold_icons",
            counting(64, 9),
            Encoding::Raw,
        ),
        plant(
            "party-menu/palette/bg",
            colors(BACKGROUND_COLORS, 0x100),
            Encoding::Lz77,
        ),
        plant(
            "party-menu/palette/pokeball",
            colors(16, 0x200),
            Encoding::Lz77,
        ),
        plant(
            "party-menu/palette/hold_icons",
            colors(16, 0x300),
            Encoding::Raw,
        ),
        plant(
            "party-menu/raw/bg",
            counting(32 * 32 * 2, 11),
            Encoding::Lz77,
        ),
        plant(
            "party-menu/raw/slot_main",
            counting(10 * 7, 21),
            Encoding::Raw,
        ),
        plant(
            "party-menu/raw/slot_main_no_hp",
            counting(10 * 7, 22),
            Encoding::Raw,
        ),
        plant(
            "party-menu/raw/slot_wide",
            counting(18 * 3, 23),
            Encoding::Raw,
        ),
        plant(
            "party-menu/raw/slot_wide_no_hp",
            counting(18 * 3, 24),
            Encoding::Raw,
        ),
        plant(
            "party-menu/raw/slot_wide_empty",
            counting(18 * 3, 25),
            Encoding::Raw,
        ),
        plant(
            "party-menu/raw/confirm_button",
            counting(7 * 2 * 2, 31),
            Encoding::Raw,
        ),
        plant(
            "party-menu/raw/cancel_button",
            counting(7 * 2 * 2, 32),
            Encoding::Raw,
        ),
        plant(
            "party-menu/image/pokeball_small",
            small_ball_tiles(),
            Encoding::Lz77,
        ),
    ]
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
        "party-menu/image/bg",
        SLOT,
        Encoding::Lz77,
        (8, 62 * 8, BACKGROUND_TILES),
    ),
    image(
        "party-menu/image/pokeball",
        2 * SLOT,
        Encoding::Lz77,
        (32, 64, 32),
    ),
    image(
        "party-menu/image/hold_icons",
        3 * SLOT,
        Encoding::Raw,
        (8, 16, 2),
    ),
    image(
        "party-menu/image/pokeball_small",
        15 * SLOT,
        Encoding::Lz77,
        (16, 96, SMALL_BALL_TILES),
    ),
];
static PALETTES: &[PaletteRoot] = &[
    palette("party-menu/palette/bg", 4 * SLOT, Encoding::Lz77, 176),
    palette("party-menu/palette/pokeball", 5 * SLOT, Encoding::Lz77, 16),
    palette("party-menu/palette/hold_icons", 6 * SLOT, Encoding::Raw, 16),
];
static MAPS: &[BlobRoot] = &[
    map("party-menu/raw/bg", 7 * SLOT, Encoding::Lz77, 2048),
    map("party-menu/raw/slot_main", 8 * SLOT, Encoding::Raw, 70),
    map(
        "party-menu/raw/slot_main_no_hp",
        9 * SLOT,
        Encoding::Raw,
        70,
    ),
    map("party-menu/raw/slot_wide", 10 * SLOT, Encoding::Raw, 54),
    map(
        "party-menu/raw/slot_wide_no_hp",
        11 * SLOT,
        Encoding::Raw,
        54,
    ),
    map(
        "party-menu/raw/slot_wide_empty",
        12 * SLOT,
        Encoding::Raw,
        54,
    ),
    map(
        "party-menu/raw/confirm_button",
        13 * SLOT,
        Encoding::Raw,
        28,
    ),
    map("party-menu/raw/cancel_button", 14 * SLOT, Encoding::Raw, 28),
];

fn roots(party_menu: PartyMenuRoots) -> Roots {
    Roots {
        party_menu,
        ..Roots::NONE
    }
}

fn full() -> Roots {
    roots(PartyMenuRoots {
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
fn every_party_menu_root_is_emitted_with_its_decoded_bytes() {
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
        ("party-menu/image/bg", 8, 496),
        ("party-menu/image/pokeball", 32, 64),
        ("party-menu/image/hold_icons", 8, 16),
        ("party-menu/image/pokeball_small", 16, 96),
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
        kind("party-menu/palette/bg"),
        EntryKind::Palette { color_count: 176 }
    );
    for id in [
        "party-menu/palette/pokeball",
        "party-menu/palette/hold_icons",
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
}

#[test]
fn the_background_sheet_is_sixty_two_tiles_stacked_in_tile_order() {
    let (bytes, entries) = written(&full()).expect("every root is well-formed");
    let pixels = payload(&bytes, &entries, "party-menu/image/bg");
    let tiles = background_tiles();
    assert_eq!(pixels.len(), 8 * 496);
    for tile in 0..62usize {
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
                    pixels[(tile * 8 + row) * 8 + column],
                    expected,
                    "tile {tile} row {row} column {column}"
                );
            }
        }
    }
}

#[test]
fn the_held_item_sheet_is_two_raw_tiles() {
    let (bytes, entries) = written(&full()).expect("every root is well-formed");
    let pixels = payload(&bytes, &entries, "party-menu/image/hold_icons");
    let tiles = counting(64, 9);
    assert_eq!(pixels.len(), 8 * 16);
    assert_eq!(pixels[0], tiles[0] & 0x0F);
    assert_eq!(pixels[1], tiles[0] >> 4);
    // The second tile starts on the ninth raster row.
    assert_eq!(pixels[8 * 8], tiles[32] & 0x0F);
}

#[test]
fn the_small_ball_sheet_is_two_tiles_wide_and_twenty_four_tiles_in_all() {
    let (bytes, entries) = written(&full()).expect("every root is well-formed");
    let pixels = payload(&bytes, &entries, "party-menu/image/pokeball_small");
    assert_eq!(pixels.len(), 16 * 96);
    for (index, &pixel) in pixels.iter().enumerate() {
        let (x, y) = (index % 16, index / 16);
        let tile = (y / 8) * 2 + x / 8;
        assert_eq!(usize::from(pixel), tile % 16, "x {x} y {y}");
    }
    // It draws with the Poké Ball palette, so it has none of its own.
    assert!(!entries
        .iter()
        .any(|entry| entry.id == "party-menu/palette/pokeball_small"));
}

#[test]
fn every_background_colour_survives() {
    let (bytes, entries) = written(&full()).expect("every root is well-formed");
    let colours = payload(&bytes, &entries, "party-menu/palette/bg");
    assert_eq!(colours.len(), 176 * 2);
    assert_eq!(&colours[175 * 2..], 0x1AF_u16.to_le_bytes());
}

#[test]
fn the_domain_list_dispatches_the_party_menu() {
    let rom = rom();
    let mut writer = PackWriter::new();
    for domain in DOMAINS {
        domain(&rom, &full(), &mut writer).expect("every root is well-formed");
    }
    let bytes = writer.finish().expect("no duplicate ids");
    assert_eq!(parse_directory(&bytes).expect("a valid pack").len(), 15);
}

#[test]
fn an_empty_family_writes_nothing() {
    let (_, entries) = written(&Roots::NONE).expect("nothing to read");
    assert!(entries.is_empty());
    assert_eq!(Roots::NONE.party_menu, PartyMenuRoots::NONE);
}

#[test]
fn party_menu_images_and_palettes_are_enumerated() {
    let roots = full();
    let images: Vec<&str> = roots.images().map(|root| root.id).collect();
    let palettes: Vec<&str> = roots.palettes().map(|root| root.id).collect();
    assert_eq!(
        images,
        [
            "party-menu/image/bg",
            "party-menu/image/pokeball",
            "party-menu/image/hold_icons",
            "party-menu/image/pokeball_small"
        ]
    );
    assert_eq!(
        palettes,
        [
            "party-menu/palette/bg",
            "party-menu/palette/pokeball",
            "party-menu/palette/hold_icons"
        ]
    );
}

#[test]
fn a_late_map_past_the_rom_leaves_nothing_queued() {
    static BAD_MAPS: &[BlobRoot] = &[
        MAPS[0],
        BlobRoot {
            addr: at(ROM_SIZE - 2),
            ..MAPS[7]
        },
    ];
    let roots = roots(PartyMenuRoots {
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
        addr: at(8 * SLOT),
        ..MAPS[0]
    }];
    let roots = roots(PartyMenuRoots {
        tilemaps: BAD_MAPS,
        ..PartyMenuRoots::NONE
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
        tile_count: 61,
        ..IMAGES[0]
    }];
    let roots = roots(PartyMenuRoots {
        images: BAD_IMAGES,
        ..PartyMenuRoots::NONE
    });
    let err = written(&roots).unwrap_err();
    assert!(matches!(err, ImportError::Lz77 { .. }), "{err}");
}

#[test]
fn a_small_ball_stream_of_twenty_three_tiles_is_a_typed_error() {
    static SHORT_IMAGES: &[ImageRoot] = &[ImageRoot {
        tile_count: SMALL_BALL_TILES - 1,
        ..IMAGES[3]
    }];
    let roots = roots(PartyMenuRoots {
        images: SHORT_IMAGES,
        ..PartyMenuRoots::NONE
    });
    let err = written(&roots).unwrap_err();
    assert!(matches!(err, ImportError::Lz77 { .. }), "{err}");
}
