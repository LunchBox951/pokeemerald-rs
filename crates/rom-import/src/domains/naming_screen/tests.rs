//! Naming-screen reader tests over a synthetic ROM.
//!
//! All 26 records are planted under the real `naming-screen/` ids at their
//! production shapes: the compressed 24x128 menu sheet, fourteen raw sheets
//! (the three cursors as 8x40 strips of five tiles), seven raw 16-colour
//! banks, and four compressed 1280-byte tilemaps. The failure tests break one
//! root at a time and require a typed error with nothing queued.

use pack_format::{parse_directory, DirectoryEntry, EntryKind, PackWriter};

use super::write;
use crate::domains::DOMAINS;
use crate::error::{ImportError, Lz77Fault};
use crate::fixture::RomFixture;
use crate::reader::{GbaPtr, ROM_BASE};
use crate::rom::{Rom, ROM_SIZE};
use crate::roots::{BlobRoot, Encoding, ImageRoot, NamingScreenRoots, PaletteRoot, Roots};

const IMAGE_AT: usize = 0x10_0000;
const MENU_PALETTE_AT: usize = 0x20_0000;
const KEYBOARD_PALETTE_AT: usize = 0x21_0000;
const TILEMAP_AT: usize = 0x30_0000;
const SLOT: usize = 0x1000;
const TILEMAP_LEN: usize = 1280;

/// `(id, width, height, stored tiles, LZ77)` for every sheet, in manifest order.
const SHEETS: [(&str, u32, u32, u32, bool); 15] = [
    ("naming-screen/image/menu", 24, 128, 48, true),
    ("naming-screen/image/page_swap_frame", 40, 32, 20, false),
    ("naming-screen/image/back_button", 40, 24, 15, false),
    ("naming-screen/image/ok_button", 40, 24, 15, false),
    ("naming-screen/image/page_swap_upper", 40, 8, 5, false),
    ("naming-screen/image/page_swap_lower", 40, 8, 5, false),
    ("naming-screen/image/page_swap_others", 40, 8, 5, false),
    ("naming-screen/image/cursor", 8, 40, 5, false),
    ("naming-screen/image/cursor_squished", 8, 40, 5, false),
    ("naming-screen/image/cursor_filled", 8, 40, 5, false),
    ("naming-screen/image/page_swap_button", 32, 16, 8, false),
    ("naming-screen/image/input_arrow", 8, 8, 1, false),
    ("naming-screen/image/underscore", 8, 8, 1, false),
    ("naming-screen/image/pc_icon_off", 16, 24, 6, false),
    ("naming-screen/image/pc_icon_on", 16, 24, 6, false),
];

/// The six `gNamingScreenMenu_Pal` rows in array order, then `sKeyboard_Pal`.
const PALETTE_IDS: [&str; 7] = [
    "naming-screen/palette/menu",
    "naming-screen/palette/page_swap_upper",
    "naming-screen/palette/page_swap_lower",
    "naming-screen/palette/page_swap_others",
    "naming-screen/palette/buttons",
    "naming-screen/palette/cursor",
    "naming-screen/palette/keyboard",
];

const TILEMAP_IDS: [&str; 4] = [
    "naming-screen/raw/background",
    "naming-screen/raw/keyboard_upper",
    "naming-screen/raw/keyboard_lower",
    "naming-screen/raw/keyboard_symbols",
];

/// The bus address of a fixture offset.
#[expect(
    clippy::cast_possible_truncation,
    reason = "fixture offsets are far below u32::MAX"
)]
const fn at(off: usize) -> GbaPtr {
    GbaPtr::at(ROM_BASE + off as u32)
}

const fn encoding(lz77: bool) -> Encoding {
    if lz77 {
        Encoding::Lz77
    } else {
        Encoding::Raw
    }
}

const fn image_root(index: usize) -> ImageRoot {
    let (id, width, height, tile_count, lz77) = SHEETS[index];
    ImageRoot {
        id,
        addr: at(IMAGE_AT + index * SLOT),
        encoding: encoding(lz77),
        rom_bit_depth: 4,
        pack_bit_depth: 4,
        width,
        height,
        metatile_width: width / 8,
        metatile_height: height / 8,
        tile_count,
    }
}

const fn palette_root(index: usize) -> PaletteRoot {
    let off = if index < 6 {
        MENU_PALETTE_AT + index * 32
    } else {
        KEYBOARD_PALETTE_AT
    };
    PaletteRoot {
        id: PALETTE_IDS[index],
        addr: at(off),
        encoding: Encoding::Raw,
        color_count: 16,
    }
}

const fn tilemap_root(index: usize) -> BlobRoot {
    BlobRoot {
        id: TILEMAP_IDS[index],
        addr: at(TILEMAP_AT + index * SLOT),
        encoding: Encoding::Lz77,
        len: 1280,
    }
}

static IMAGES: [ImageRoot; 15] = {
    let mut roots = [image_root(0); 15];
    let mut index = 0;
    while index < 15 {
        roots[index] = image_root(index);
        index += 1;
    }
    roots
};
static PALETTES: [PaletteRoot; 7] = {
    let mut roots = [palette_root(0); 7];
    let mut index = 0;
    while index < 7 {
        roots[index] = palette_root(index);
        index += 1;
    }
    roots
};
static TILEMAPS: [BlobRoot; 4] = [
    tilemap_root(0),
    tilemap_root(1),
    tilemap_root(2),
    tilemap_root(3),
];

fn full() -> Roots {
    roots(NamingScreenRoots {
        images: &IMAGES,
        palettes: &PALETTES,
        tilemaps: &TILEMAPS,
    })
}

fn roots(naming_screen: NamingScreenRoots) -> Roots {
    Roots {
        naming_screen,
        ..Roots::NONE
    }
}

/// The palette index tile `tile` of sheet `sheet` is filled with: 1..=15, so a
/// misplaced or dropped tile changes a pixel.
fn colour(sheet: usize, tile: usize) -> u8 {
    u8::try_from((sheet + tile) % 15 + 1).expect("a nibble")
}

/// 4bpp tiles where tile `n` of sheet `sheet` is uniformly [`colour`].
fn sheet_tiles(sheet: usize) -> Vec<u8> {
    let tiles = usize::try_from(SHEETS[sheet].3).expect("small");
    (0..tiles)
        .flat_map(|tile| {
            let c = colour(sheet, tile);
            [c | c << 4; 32]
        })
        .collect()
}

/// A sixteen-colour BGR555 bank starting at `first`, as ROM bytes.
fn bank(first: u16) -> Vec<u8> {
    (0..16u16)
        .flat_map(|n| (first + n * 0x21).to_le_bytes())
        .collect()
}

/// Distinct banks per palette slot, so a swapped row changes its bytes.
fn palette_bytes(index: usize) -> Vec<u8> {
    bank(0x0100 + 0x0100 * u16::try_from(index).expect("small"))
}

fn tilemap(index: usize) -> Vec<u8> {
    (0..TILEMAP_LEN)
        .map(|n| u8::try_from((n * 7 + index * 13) % 251).expect("below 251"))
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

fn fixture() -> RomFixture {
    let mut fixture = RomFixture::new().emerald_header();
    for (index, sheet) in SHEETS.iter().enumerate() {
        let tiles = sheet_tiles(index);
        let stored = if sheet.4 {
            lz77_literals(&tiles)
        } else {
            tiles
        };
        fixture = fixture.write(IMAGE_AT + index * SLOT, &stored);
    }
    for index in 0..6 {
        fixture = fixture.write(MENU_PALETTE_AT + index * 32, &palette_bytes(index));
    }
    fixture = fixture.write(KEYBOARD_PALETTE_AT, &palette_bytes(6));
    for index in 0..4 {
        fixture = fixture.write(TILEMAP_AT + index * SLOT, &lz77_literals(&tilemap(index)));
    }
    fixture
}

fn rom() -> Rom {
    Rom::from_bytes(fixture().finish()).expect("the fixture header is valid")
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

#[test]
fn the_twenty_six_records_are_emitted_with_their_production_shapes() {
    let (_, entries) = written(&full()).expect("every root is well-formed");
    assert_eq!(entries.len(), 26);
    let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(ids, sorted, "the pack is sorted by id");
    for (id, width, height, _, _) in SHEETS {
        let entry = entries.iter().find(|e| e.id == id).expect(id);
        assert_eq!(
            entry.kind,
            EntryKind::Image {
                width,
                height,
                bit_depth: 4
            },
            "{id}"
        );
    }
    for id in PALETTE_IDS {
        let entry = entries.iter().find(|e| e.id == id).expect(id);
        assert_eq!(entry.kind, EntryKind::Palette { color_count: 16 }, "{id}");
    }
    for id in TILEMAP_IDS {
        let entry = entries.iter().find(|e| e.id == id).expect(id);
        assert_eq!(entry.kind, EntryKind::Raw, "{id}");
    }
}

#[test]
fn every_sheet_lands_one_byte_per_pixel_in_tile_order() {
    let (bytes, entries) = written(&full()).expect("every root is well-formed");
    for (sheet, (id, width, height, _, _)) in SHEETS.iter().enumerate() {
        let pixels = payload(&bytes, &entries, id);
        let (width, height) = (*width as usize, *height as usize);
        assert_eq!(pixels.len(), width * height, "{id}");
        for (index, &pixel) in pixels.iter().enumerate() {
            let (x, y) = (index % width, index / width);
            let tile = (y / 8) * (width / 8) + x / 8;
            assert_eq!(pixel, colour(sheet, tile), "{id} pixel ({x}, {y})");
        }
    }
}

#[test]
fn a_tile_unpacks_low_nibble_first() {
    // 0x10 is pixel 0 then pixel 1, as the GBA reads a 4bpp byte.
    let tile: Vec<u8> = (0..8u8)
        .flat_map(|row| [0x10, 0x32, 0x54, 0x76].map(|pair| if row == 0 { pair } else { 0 }))
        .collect();
    let bytes = fixture().write(IMAGE_AT + 11 * SLOT, &tile).finish();
    let rom = Rom::from_bytes(bytes).expect("valid header");
    let mut writer = PackWriter::new();
    write(&rom, &full(), &mut writer).expect("every root is well-formed");
    let bytes = writer.finish().expect("no duplicate ids");
    let entries = parse_directory(&bytes).expect("a valid pack");
    let arrow = payload(&bytes, &entries, "naming-screen/image/input_arrow");
    assert_eq!(arrow[..8], [0, 1, 2, 3, 4, 5, 6, 7]);
    assert!(arrow[8..].iter().all(|&pixel| pixel == 0));
}

#[test]
fn each_cursor_is_an_eight_by_forty_strip_of_its_five_stored_tiles() {
    let (bytes, entries) = written(&full()).expect("every root is well-formed");
    for (sheet, stem) in [(7, "cursor"), (8, "cursor_squished"), (9, "cursor_filled")] {
        let id = format!("naming-screen/image/{stem}");
        let entry = entries.iter().find(|e| e.id == id).expect(&id);
        assert_eq!(
            entry.kind,
            EntryKind::Image {
                width: 8,
                height: 40,
                bit_depth: 4
            }
        );
        let pixels = payload(&bytes, &entries, &id);
        assert_eq!(pixels.len(), 8 * 40);
        for tile in 0..5 {
            assert!(
                pixels[tile * 64..(tile + 1) * 64]
                    .iter()
                    .all(|&pixel| pixel == colour(sheet, tile)),
                "{id} tile {tile}"
            );
        }
    }
}

#[test]
fn the_six_menu_banks_and_the_keyboard_bank_keep_their_own_bytes() {
    let (bytes, entries) = written(&full()).expect("every root is well-formed");
    for (index, id) in PALETTE_IDS.iter().enumerate() {
        assert_eq!(payload(&bytes, &entries, id), palette_bytes(index), "{id}");
    }
}

#[test]
fn the_tilemaps_land_as_their_full_decoded_bytes() {
    let (bytes, entries) = written(&full()).expect("every root is well-formed");
    for (index, id) in TILEMAP_IDS.iter().enumerate() {
        let decoded = payload(&bytes, &entries, id);
        assert_eq!(decoded.len(), TILEMAP_LEN, "{id}");
        assert_eq!(decoded, tilemap(index), "{id}");
    }
}

#[test]
fn the_domain_list_dispatches_the_naming_screen() {
    let rom = rom();
    let mut writer = PackWriter::new();
    for domain in DOMAINS {
        domain(&rom, &full(), &mut writer).expect("every root is well-formed");
    }
    let bytes = writer.finish().expect("no duplicate ids");
    let entries = parse_directory(&bytes).expect("a valid pack");
    assert_eq!(entries.len(), 26);
    assert!(entries
        .iter()
        .all(|entry| entry.id.starts_with("naming-screen/")));
}

#[test]
fn an_empty_family_writes_nothing() {
    let (_, entries) = written(&Roots::NONE).expect("nothing to read");
    assert!(entries.is_empty());
    assert_eq!(Roots::NONE.naming_screen, NamingScreenRoots::NONE);
}

#[test]
fn the_whole_root_iterators_enumerate_the_sheets_and_the_palettes() {
    let roots = full();
    let images: Vec<&str> = roots.images().map(|root| root.id).collect();
    let palettes: Vec<&str> = roots.palettes().map(|root| root.id).collect();
    assert_eq!(images, SHEETS.map(|sheet| sheet.0));
    assert_eq!(palettes, PALETTE_IDS);
}

#[test]
fn a_root_past_the_rom_fails_with_nothing_queued() {
    static LATE_PALETTES: [PaletteRoot; 1] = [PaletteRoot {
        addr: at(ROM_SIZE - 2),
        ..palette_root(6)
    }];
    let roots = roots(NamingScreenRoots {
        images: &IMAGES,
        palettes: &LATE_PALETTES,
        tilemaps: &TILEMAPS,
    });
    let mut writer = PackWriter::new();
    let err = write(&rom(), &roots, &mut writer).unwrap_err();
    assert!(matches!(err, ImportError::Truncated { .. }), "{err}");
    // The good images read before the bad palette were not queued.
    let bytes = writer.finish().expect("an empty pack");
    assert!(parse_directory(&bytes).expect("a valid pack").is_empty());
}

#[test]
fn a_raw_sheet_past_the_rom_is_truncated() {
    static LATE: [ImageRoot; 1] = [ImageRoot {
        addr: at(ROM_SIZE - 32),
        ..image_root(7)
    }];
    let roots = roots(NamingScreenRoots {
        images: &LATE,
        ..NamingScreenRoots::NONE
    });
    let err = written(&roots).unwrap_err();
    assert!(matches!(err, ImportError::Truncated { .. }), "{err}");
}

#[test]
fn a_compressed_root_that_is_not_a_stream_is_a_typed_error() {
    // A raw palette does not begin with the BIOS type byte.
    static TILEMAP_AT_PALETTE: [BlobRoot; 1] = [BlobRoot {
        addr: at(KEYBOARD_PALETTE_AT),
        ..tilemap_root(0)
    }];
    let roots = roots(NamingScreenRoots {
        tilemaps: &TILEMAP_AT_PALETTE,
        ..NamingScreenRoots::NONE
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
fn a_tilemap_declared_at_half_its_length_is_a_size_mismatch_not_a_truncation() {
    static HALF: [BlobRoot; 1] = [BlobRoot {
        len: 640,
        ..tilemap_root(0)
    }];
    static SHORT_MENU: [ImageRoot; 1] = [ImageRoot {
        tile_count: 24,
        height: 64,
        metatile_height: 8,
        ..image_root(0)
    }];
    let cases = [
        NamingScreenRoots {
            tilemaps: &HALF,
            ..NamingScreenRoots::NONE
        },
        NamingScreenRoots {
            images: &SHORT_MENU,
            ..NamingScreenRoots::NONE
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
fn a_six_tile_cursor_overfills_its_eight_by_forty_raster() {
    static SIX_TILES: [ImageRoot; 1] = [ImageRoot {
        tile_count: 6,
        ..image_root(7)
    }];
    let roots = roots(NamingScreenRoots {
        images: &SIX_TILES,
        ..NamingScreenRoots::NONE
    });
    let err = written(&roots).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::EntryShape {
                id: "naming-screen/image/cursor",
                ..
            }
        ),
        "{err}"
    );
}
