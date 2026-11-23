//! PC wallpaper reader tests over a synthetic ROM.
//!
//! Two records (Crag and Volcano, rows 4 and 5 of `sWallpapers`) are planted
//! at production shapes. The cases are a well-formed ROM, a row disagreeing
//! with the profile, a malformed or truncated stream, and every tilemap word
//! `AssetPack::pc_wallpaper` would refuse.

use pack_format::{parse_directory, DirectoryEntry, EntryKind, PackWriter};

use super::{write, TILEMAP_BYTES};
use crate::domains::DOMAINS;
use crate::error::{ImportError, Lz77Fault};
use crate::fixture::RomFixture;
use crate::reader::{GbaPtr, ROM_BASE};
use crate::rom::{Rom, ROM_SIZE};
use crate::roots::{
    BlobRoot, Encoding, ImageRoot, PaletteRoot, PcWallpaperRoot, PcWallpaperRoots, Roots,
};

const TABLE: u32 = 0x1_0000;
const CRAG_TILES: u32 = 0x2_0000;
const CRAG_MAP: u32 = 0x2_1000;
const CRAG_PALETTES: u32 = 0x2_2000;
const VOLCANO_TILES: u32 = 0x3_0000;
const VOLCANO_MAP: u32 = 0x3_1000;
const VOLCANO_PALETTES: u32 = 0x3_2000;
/// Unreferenced copies of Crag's art.
const STRAY_TILES: u32 = 0x4_0000;
const STRAY_PALETTES: u32 = 0x4_2000;
const ROM_END: u32 = 0x0100_0000;
const _: () = assert!(ROM_END as usize == ROM_SIZE);
/// Where the four-byte stream header of a truncated stream fits.
const FOUR_BYTES_FROM_THE_END: u32 = ROM_END - 4;

const CRAG_ROW: usize = 4;
const VOLCANO_ROW: usize = 5;
const RECORD: usize = 12;

const fn at(off: u32) -> GbaPtr {
    GbaPtr::at(ROM_BASE + off)
}

fn idx(off: u32) -> usize {
    usize::try_from(off).expect("a ROM offset fits in usize")
}

/// An LZ77 type `0x10` stream of `payload`, all literals.
fn lz77_literals(payload: &[u8]) -> Vec<u8> {
    let len = u32::try_from(payload.len()).expect("a test payload fits in 24 bits");
    let size = len.to_le_bytes();
    let mut stream = vec![0x10, size[0], size[1], size[2]];
    for chunk in payload.chunks(8) {
        stream.push(0x00);
        stream.extend_from_slice(chunk);
    }
    stream
}

/// Three 4bpp tiles: transparent, then two filled with index nibbles.
fn tiles() -> Vec<u8> {
    let mut bytes = vec![0u8; 32];
    bytes.extend_from_slice(&[0x21; 32]);
    bytes.extend_from_slice(&[0x43; 32]);
    bytes
}

/// The words every cell cycles through: blank, frame tile 1, background
/// tile 2, and a flipped frame tile 1.
const CELLS: [u16; 4] = [0x0000, 0x1001, 0x2002, 0x1C01];

fn tilemap_with(edits: &[(usize, u16)]) -> Vec<u8> {
    let mut words: Vec<u16> = (0..360).map(|cell| CELLS[cell % CELLS.len()]).collect();
    for &(cell, word) in edits {
        words[cell] = word;
    }
    words.iter().flat_map(|word| word.to_le_bytes()).collect()
}

fn palettes() -> Vec<u8> {
    let mut bytes = Vec::new();
    for bank in 0u16..2 {
        for color in 0u16..16 {
            bytes.extend_from_slice(&(bank * 16 + color).to_le_bytes());
        }
    }
    bytes
}

fn row(tiles: u32, tilemap: u32, palettes: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    for off in [tiles, tilemap, palettes] {
        bytes.extend_from_slice(&at(off).raw().to_le_bytes());
    }
    bytes
}

fn row_offset(row: usize) -> usize {
    idx(TABLE) + row * RECORD
}

fn fixture() -> RomFixture {
    RomFixture::new()
        .emerald_header()
        .write(
            row_offset(CRAG_ROW),
            &row(CRAG_TILES, CRAG_MAP, CRAG_PALETTES),
        )
        .write(
            row_offset(VOLCANO_ROW),
            &row(VOLCANO_TILES, VOLCANO_MAP, VOLCANO_PALETTES),
        )
        .write(idx(CRAG_TILES), &lz77_literals(&tiles()))
        .write(idx(STRAY_TILES), &lz77_literals(&tiles()))
        .write(idx(CRAG_MAP), &lz77_literals(&tilemap_with(&[])))
        .write(idx(CRAG_PALETTES), &palettes())
        .write(idx(STRAY_PALETTES), &palettes())
        .write(idx(VOLCANO_TILES), &lz77_literals(&tiles()[..64]))
        .write(idx(VOLCANO_MAP), &lz77_literals(&tilemap_with(&[])))
        .write(idx(VOLCANO_PALETTES), &palettes())
}

fn rom_of(fixture: RomFixture) -> Rom {
    Rom::from_bytes(fixture.finish()).expect("the fixture header is valid")
}

const fn sheet(id: &'static str, addr: GbaPtr, tile_count: u32) -> ImageRoot {
    ImageRoot {
        id,
        addr,
        encoding: Encoding::Lz77,
        rom_bit_depth: 4,
        pack_bit_depth: 4,
        width: 8,
        height: 24,
        metatile_width: 1,
        metatile_height: 1,
        tile_count,
    }
}

const fn tilemap(id: &'static str, addr: GbaPtr) -> BlobRoot {
    BlobRoot {
        id,
        addr,
        encoding: Encoding::Lz77,
        len: TILEMAP_BYTES,
    }
}

const fn bank(id: &'static str, addr: GbaPtr) -> PaletteRoot {
    PaletteRoot {
        id,
        addr,
        color_count: 16,
        encoding: Encoding::Raw,
    }
}

const CRAG: PcWallpaperRoot = PcWallpaperRoot {
    wallpaper: 4,
    tiles: sheet("pc-wallpaper/crag/tiles", at(CRAG_TILES), 3),
    tilemap: tilemap("pc-wallpaper/crag/tilemap", at(CRAG_MAP)),
    frame_palette: bank("pc-wallpaper/crag/palette/frame", at(CRAG_PALETTES)),
    background_palette: bank(
        "pc-wallpaper/crag/palette/background",
        at(CRAG_PALETTES + 32),
    ),
};

/// Two of the three tiles are stored; the third is zero-filled.
const VOLCANO: PcWallpaperRoot = PcWallpaperRoot {
    wallpaper: 5,
    tiles: sheet("pc-wallpaper/volcano/tiles", at(VOLCANO_TILES), 2),
    tilemap: tilemap("pc-wallpaper/volcano/tilemap", at(VOLCANO_MAP)),
    frame_palette: bank("pc-wallpaper/volcano/palette/frame", at(VOLCANO_PALETTES)),
    background_palette: bank(
        "pc-wallpaper/volcano/palette/background",
        at(VOLCANO_PALETTES + 32),
    ),
};

fn roots_of(wallpapers: &'static [PcWallpaperRoot]) -> Roots {
    Roots {
        pc_wallpaper: PcWallpaperRoots {
            table: at(TABLE),
            wallpapers,
        },
        ..Roots::NONE
    }
}

fn written(rom: &Rom, roots: &Roots) -> Result<Vec<DirectoryEntry>, ImportError> {
    let mut writer = PackWriter::new();
    write(rom, roots, &mut writer)?;
    let bytes = writer.finish().expect("no duplicate ids");
    Ok(parse_directory(&bytes).expect("a valid pack"))
}

fn run(fixture: RomFixture, wallpapers: &'static [PcWallpaperRoot]) -> Result<(), ImportError> {
    written(&rom_of(fixture), &roots_of(wallpapers)).map(|_| ())
}

fn assert_mismatch(result: Result<(), ImportError>, expected_field: &str) {
    match result {
        Err(ImportError::StructMismatch { field, .. }) => assert_eq!(field, expected_field),
        other => panic!("expected a {expected_field} mismatch, got {other:?}"),
    }
}

#[test]
fn each_record_emits_its_sheet_tilemap_and_both_palettes() {
    let entries = written(&rom_of(fixture()), &roots_of(&[CRAG, VOLCANO]))
        .expect("a well-formed pair of records");
    let ids: Vec<&str> = entries.iter().map(|entry| entry.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "pc-wallpaper/crag/palette/background",
            "pc-wallpaper/crag/palette/frame",
            "pc-wallpaper/crag/tilemap",
            "pc-wallpaper/crag/tiles",
            "pc-wallpaper/volcano/palette/background",
            "pc-wallpaper/volcano/palette/frame",
            "pc-wallpaper/volcano/tilemap",
            "pc-wallpaper/volcano/tiles",
        ]
    );
    assert_eq!(entries[0].kind, EntryKind::Palette { color_count: 16 });
    assert_eq!(entries[2].kind, EntryKind::Raw);
    assert_eq!(
        entries[3].kind,
        EntryKind::Image {
            width: 8,
            height: 24,
            bit_depth: 4
        }
    );
}

#[test]
fn a_trimmed_sheet_is_zero_filled_and_the_background_palette_follows_the_frame() {
    let bytes = {
        let rom = rom_of(fixture());
        let mut writer = PackWriter::new();
        write(&rom, &roots_of(&[VOLCANO]), &mut writer).expect("a well-formed record");
        writer.finish().expect("no duplicate ids")
    };
    let entries = parse_directory(&bytes).expect("a valid pack");
    let payload = |id: &str| {
        let entry = entries.iter().find(|entry| entry.id == id).expect("entry");
        bytes[entry.offset..entry.offset + entry.length].to_vec()
    };
    let sheet = payload("pc-wallpaper/volcano/tiles");
    assert_eq!(sheet.len(), 8 * 24);
    assert!(sheet[128..].iter().all(|&pixel| pixel == 0));
    assert_eq!(
        &payload("pc-wallpaper/volcano/palette/frame"),
        &palettes()[..32]
    );
    assert_eq!(
        &payload("pc-wallpaper/volcano/palette/background"),
        &palettes()[32..]
    );
}

#[test]
fn the_domain_list_dispatches_the_wallpapers() {
    let rom = rom_of(fixture());
    let roots = roots_of(&[CRAG, VOLCANO]);
    let mut writer = PackWriter::new();
    for domain in DOMAINS {
        domain(&rom, &roots, &mut writer).expect("every root is well-formed");
    }
    let bytes = writer.finish().expect("no duplicate ids");
    assert_eq!(parse_directory(&bytes).expect("a valid pack").len(), 8);
}

#[test]
fn a_profile_without_the_records_writes_nothing() {
    assert!(written(&rom_of(fixture()), &Roots::NONE)
        .expect("nothing to read")
        .is_empty());
    assert_eq!(Roots::NONE.pc_wallpaper, PcWallpaperRoots::NONE);
}

#[test]
fn the_records_are_enumerated_as_sheets_and_palette_pairs() {
    let roots = roots_of(&[CRAG, VOLCANO]);
    let images: Vec<&str> = roots.images().map(|root| root.id).collect();
    let palettes: Vec<&str> = roots.palettes().map(|root| root.id).collect();
    assert_eq!(
        images,
        ["pc-wallpaper/crag/tiles", "pc-wallpaper/volcano/tiles"]
    );
    assert_eq!(
        palettes,
        [
            "pc-wallpaper/crag/palette/frame",
            "pc-wallpaper/crag/palette/background",
            "pc-wallpaper/volcano/palette/frame",
            "pc-wallpaper/volcano/palette/background",
        ]
    );
}

#[test]
fn a_row_naming_another_copy_is_a_mismatch() {
    static STRAY_SHEET: [PcWallpaperRoot; 1] = [PcWallpaperRoot {
        tiles: sheet("pc-wallpaper/crag/tiles", at(STRAY_TILES), 3),
        ..CRAG
    }];
    static STRAY_MAP: [PcWallpaperRoot; 1] = [PcWallpaperRoot {
        tilemap: tilemap("pc-wallpaper/crag/tilemap", at(VOLCANO_MAP)),
        ..CRAG
    }];
    static STRAY_PAIR: [PcWallpaperRoot; 1] = [PcWallpaperRoot {
        frame_palette: bank("pc-wallpaper/crag/palette/frame", at(STRAY_PALETTES)),
        background_palette: bank(
            "pc-wallpaper/crag/palette/background",
            at(STRAY_PALETTES + 32),
        ),
        ..CRAG
    }];
    assert_mismatch(run(fixture(), &STRAY_SHEET), "Wallpaper.tiles");
    assert_mismatch(run(fixture(), &STRAY_MAP), "Wallpaper.tilemap");
    assert_mismatch(run(fixture(), &STRAY_PAIR), "Wallpaper.palettes");
}

#[test]
fn a_record_read_through_another_row_is_a_mismatch() {
    static WRONG_ROW: [PcWallpaperRoot; 1] = [PcWallpaperRoot {
        wallpaper: 5,
        ..CRAG
    }];
    assert_mismatch(run(fixture(), &WRONG_ROW), "Wallpaper.tiles");
}

#[test]
fn a_background_palette_that_does_not_follow_the_frame_is_a_mismatch() {
    static APART: [PcWallpaperRoot; 1] = [PcWallpaperRoot {
        background_palette: bank(
            "pc-wallpaper/crag/palette/background",
            at(STRAY_PALETTES + 32),
        ),
        ..CRAG
    }];
    assert_mismatch(run(fixture(), &APART), "Wallpaper.palettes[1]");
}

#[test]
fn a_row_past_the_table_is_out_of_range() {
    static PAST: [PcWallpaperRoot; 1] = [PcWallpaperRoot {
        wallpaper: 16,
        ..CRAG
    }];
    let err = run(fixture(), &PAST).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::IndexOutOfRange {
                table: "sWallpapers",
                index: 16,
                count: 16
            }
        ),
        "{err}"
    );
}

#[test]
fn a_table_past_the_rom_fails_closed() {
    let mut roots = roots_of(&[CRAG]);
    roots.pc_wallpaper.table = at(FOUR_BYTES_FROM_THE_END);
    let err = written(&rom_of(fixture()), &roots).unwrap_err();
    assert!(matches!(err, ImportError::Truncated { .. }), "{err}");
}

#[test]
fn a_row_pointer_outside_the_cartridge_is_refused() {
    let rom = rom_of(fixture().write(row_offset(CRAG_ROW), &0x0200_0000u32.to_le_bytes()));
    let err = written(&rom, &roots_of(&[CRAG])).unwrap_err();
    assert!(
        matches!(err, ImportError::PointerOutOfRange { .. }),
        "{err}"
    );
}

#[test]
fn a_row_pointer_past_the_rom_image_is_refused() {
    static PAST_THE_END: [PcWallpaperRoot; 1] = [PcWallpaperRoot {
        tiles: sheet("pc-wallpaper/crag/tiles", GbaPtr::at(ROM_BASE + ROM_END), 3),
        ..CRAG
    }];
    let fixture = fixture().write(row_offset(CRAG_ROW), &row(ROM_END, CRAG_MAP, CRAG_PALETTES));
    let fault = lz77_error(run(fixture, &PAST_THE_END).unwrap_err());
    assert!(matches!(fault, Lz77Fault::UnexpectedEnd), "{fault:?}");
}

fn lz77_error(err: ImportError) -> Lz77Fault {
    match err {
        ImportError::Lz77 { fault, .. } => fault,
        other => panic!("expected an LZ77 fault, got {other:?}"),
    }
}

#[test]
fn a_stream_of_the_wrong_type_is_refused_for_both_payloads() {
    for at_offset in [CRAG_TILES, CRAG_MAP] {
        let broken = fixture().write(idx(at_offset), &[0x20, 0x00, 0x00, 0x00]);
        let fault = lz77_error(run(broken, &[CRAG]).unwrap_err());
        assert!(matches!(fault, Lz77Fault::WrongType), "{fault:?}");
    }
}

#[test]
fn a_back_reference_before_any_output_is_refused_for_both_payloads() {
    for (at_offset, size) in [(CRAG_TILES, 0x60u8), (CRAG_MAP, 0xD0)] {
        let stream = [
            0x10,
            size,
            if size == 0xD0 { 0x02 } else { 0 },
            0,
            0x80,
            0,
            0,
        ];
        let broken = fixture().write(idx(at_offset), &stream);
        let fault = lz77_error(run(broken, &[CRAG]).unwrap_err());
        assert!(
            matches!(fault, Lz77Fault::BackReferenceTooFar { .. }),
            "{fault:?}"
        );
    }
}

#[test]
fn a_stream_ending_at_the_end_of_the_rom_is_refused_for_both_payloads() {
    static TILES_AT_END: [PcWallpaperRoot; 1] = [PcWallpaperRoot {
        tiles: sheet("pc-wallpaper/crag/tiles", at(FOUR_BYTES_FROM_THE_END), 3),
        ..CRAG
    }];
    static MAP_AT_END: [PcWallpaperRoot; 1] = [PcWallpaperRoot {
        tilemap: tilemap("pc-wallpaper/crag/tilemap", at(FOUR_BYTES_FROM_THE_END)),
        ..CRAG
    }];
    let header = |len: usize| {
        let size = u32::try_from(len).expect("fits").to_le_bytes();
        [0x10, size[0], size[1], size[2]]
    };
    let tiles_rom = fixture()
        .write(
            row_offset(CRAG_ROW),
            &row(FOUR_BYTES_FROM_THE_END, CRAG_MAP, CRAG_PALETTES),
        )
        .write(idx(FOUR_BYTES_FROM_THE_END), &header(96));
    let map_rom = fixture()
        .write(
            row_offset(CRAG_ROW),
            &row(CRAG_TILES, FOUR_BYTES_FROM_THE_END, CRAG_PALETTES),
        )
        .write(idx(FOUR_BYTES_FROM_THE_END), &header(720));
    for (rom, roots) in [(tiles_rom, &TILES_AT_END), (map_rom, &MAP_AT_END)] {
        let fault = lz77_error(run(rom, roots).unwrap_err());
        assert!(matches!(fault, Lz77Fault::UnexpectedEnd), "{fault:?}");
    }
}

#[test]
fn a_stream_decoding_to_the_wrong_length_is_refused_for_both_payloads() {
    let short_tiles = fixture().write(idx(CRAG_TILES), &lz77_literals(&tiles()[..64]));
    let fault = lz77_error(run(short_tiles, &[CRAG]).unwrap_err());
    assert!(
        matches!(
            fault,
            Lz77Fault::SizeMismatch {
                expected: 96,
                actual: 64
            }
        ),
        "{fault:?}"
    );
    let short_map = fixture().write(idx(CRAG_MAP), &lz77_literals(&tilemap_with(&[])[..718]));
    let fault = lz77_error(run(short_map, &[CRAG]).unwrap_err());
    assert!(
        matches!(
            fault,
            Lz77Fault::SizeMismatch {
                expected: 720,
                actual: 718
            }
        ),
        "{fault:?}"
    );
}

fn with_tilemap(edits: &[(usize, u16)]) -> RomFixture {
    fixture().write(idx(CRAG_MAP), &lz77_literals(&tilemap_with(edits)))
}

#[test]
fn a_tile_at_the_edge_of_the_sheet_is_accepted_and_one_past_it_is_not() {
    run(with_tilemap(&[(1, 0x1002)]), &[CRAG]).expect("tile 2 is the last of three");
    assert_mismatch(
        run(with_tilemap(&[(1, 0x1003)]), &[CRAG]),
        "tilemap tile index",
    );
    assert_mismatch(
        run(with_tilemap(&[(1, 0x13FF)]), &[CRAG]),
        "tilemap tile index",
    );
}

#[test]
fn a_tile_beyond_the_stored_tiles_of_a_trimmed_sheet_is_still_in_the_sheet() {
    let fixture = fixture().write(
        idx(VOLCANO_MAP),
        &lz77_literals(&tilemap_with(&[(1, 0x1002)])),
    );
    run(fixture, &[VOLCANO]).expect("the zero-filled third tile is part of the sheet");
}

#[test]
fn only_the_two_supplied_banks_are_accepted_on_a_nonblank_word() {
    for word in [0x0001u16, 0x3001, 0xF001, 0x0401] {
        assert_mismatch(
            run(with_tilemap(&[(1, word)]), &[CRAG]),
            "tilemap palette bank",
        );
    }
    for word in [0x1001u16, 0x2001, 0x1C02, 0x2802] {
        run(with_tilemap(&[(1, word)]), &[CRAG]).expect("a supplied bank, flips allowed");
    }
}

#[test]
fn blank_cells_need_a_transparent_tile_zero() {
    let opaque_zero = {
        let mut sheet = tiles();
        sheet[0] = 0x01;
        fixture().write(idx(CRAG_TILES), &lz77_literals(&sheet))
    };
    assert_mismatch(
        run(opaque_zero, &[CRAG]),
        "tilemap blank over opaque tile 0",
    );

    let no_blank = {
        let mut sheet = tiles();
        sheet[0] = 0x01;
        fixture()
            .write(idx(CRAG_TILES), &lz77_literals(&sheet))
            .write(
                idx(CRAG_MAP),
                &lz77_literals(&tilemap_with(
                    &(0..360).map(|cell| (cell, 0x1000)).collect::<Vec<_>>(),
                )),
            )
    };
    run(no_blank, &[CRAG]).expect("an opaque tile 0 is fine when no cell is blank");
}

#[test]
fn a_flipped_bank_zero_tile_zero_is_not_a_blank() {
    for word in [0x0400u16, 0x0800] {
        assert_mismatch(
            run(with_tilemap(&[(0, word)]), &[CRAG]),
            "tilemap palette bank",
        );
    }
}

#[test]
fn a_sheet_the_reader_would_refuse_is_a_mismatch() {
    static NARROW: [PcWallpaperRoot; 1] = [PcWallpaperRoot {
        tiles: ImageRoot {
            width: 16,
            ..CRAG.tiles
        },
        ..CRAG
    }];
    static TALL: [PcWallpaperRoot; 1] = [PcWallpaperRoot {
        tiles: ImageRoot {
            height: 2056,
            ..CRAG.tiles
        },
        ..CRAG
    }];
    static EMPTY: [PcWallpaperRoot; 1] = [PcWallpaperRoot {
        tiles: ImageRoot {
            tile_count: 0,
            ..CRAG.tiles
        },
        ..CRAG
    }];
    static EIGHT_BIT: [PcWallpaperRoot; 1] = [PcWallpaperRoot {
        tiles: ImageRoot {
            pack_bit_depth: 8,
            ..CRAG.tiles
        },
        ..CRAG
    }];
    static OVERFULL: [PcWallpaperRoot; 1] = [PcWallpaperRoot {
        tiles: ImageRoot {
            tile_count: 4,
            ..CRAG.tiles
        },
        ..CRAG
    }];
    for roots in [&NARROW, &TALL, &EMPTY, &EIGHT_BIT, &OVERFULL] {
        assert_mismatch(run(fixture(), roots), "sheet shape");
    }
}

#[test]
fn a_tilemap_or_palette_the_reader_would_refuse_is_a_mismatch() {
    static SHORT_MAP: [PcWallpaperRoot; 1] = [PcWallpaperRoot {
        tilemap: BlobRoot {
            len: TILEMAP_BYTES - 2,
            ..CRAG.tilemap
        },
        ..CRAG
    }];
    static RAW_MAP: [PcWallpaperRoot; 1] = [PcWallpaperRoot {
        tilemap: BlobRoot {
            encoding: Encoding::Raw,
            ..CRAG.tilemap
        },
        ..CRAG
    }];
    static SHORT_PALETTE: [PcWallpaperRoot; 1] = [PcWallpaperRoot {
        frame_palette: PaletteRoot {
            color_count: 15,
            ..CRAG.frame_palette
        },
        ..CRAG
    }];
    static COMPRESSED_PALETTE: [PcWallpaperRoot; 1] = [PcWallpaperRoot {
        background_palette: PaletteRoot {
            encoding: Encoding::Lz77,
            ..CRAG.background_palette
        },
        ..CRAG
    }];
    assert_mismatch(run(fixture(), &SHORT_MAP), "tilemap shape");
    assert_mismatch(run(fixture(), &RAW_MAP), "tilemap shape");
    assert_mismatch(run(fixture(), &SHORT_PALETTE), "palette shape");
    assert_mismatch(run(fixture(), &COMPRESSED_PALETTE), "palette shape");
}

#[test]
fn a_failing_last_record_queues_nothing() {
    let rom = rom_of(fixture().write(
        idx(VOLCANO_MAP),
        &lz77_literals(&tilemap_with(&[(7, 0x1003)])),
    ));
    let roots = roots_of(&[CRAG, VOLCANO]);
    let mut writer = PackWriter::new();
    let err = write(&rom, &roots, &mut writer).unwrap_err();
    assert!(matches!(err, ImportError::StructMismatch { .. }), "{err}");
    let bytes = writer.finish().expect("an empty pack");
    assert!(parse_directory(&bytes).expect("a valid pack").is_empty());
}
