//! Battle-environment reader tests over a synthetic ROM.
//!
//! The reader's own logic is the corroboration of the two table rows, so the
//! cases are a well-formed ROM (raw and compressed) and, per pointer of each
//! row, a root naming a different copy. The images, palettes and tilemaps go
//! through the shared codecs; the cases here pin that each member of a row
//! lands as the right pack entry with its decoded bytes.

use pack_format::{parse_directory, DirectoryEntry, EntryKind, PackWriter};

use super::{write, RECORD_BYTES};
use crate::domains::DOMAINS;
use crate::error::{ImportError, Lz77Fault};
use crate::fixture::RomFixture;
use crate::reader::{GbaPtr, ROM_BASE};
use crate::rom::{Rom, ROM_SIZE};
use crate::roots::{
    BattleEnvironmentRoot, BattleEnvironmentRoots, BlobRoot, Encoding, ImageRoot, PaletteRoot,
    Roots,
};

const TABLE: u32 = 0x1_0000;
const GRASS_BASE: u32 = 0x2_0000;
const PLAIN_BASE: u32 = 0x8_0000;
/// A stray copy of every kind of stream, unreferenced by the table.
const STRAY: u32 = 0xE_0000;

const SHEET_BYTES: usize = 256 * 32;
const MAIN_MAP_BYTES: usize = 4096;
const ENTRY_MAP_BYTES: usize = 2048;
const PALETTE_BYTES: usize = 96;
const GRASS_ROW: usize = 0;
const PLAIN_ROW: usize = 9;

const FIELDS: [&str; 5] = [
    "BattleBackground.tileset",
    "BattleBackground.tilemap",
    "BattleBackground.entryTileset",
    "BattleBackground.entryTilemap",
    "BattleBackground.palette",
];

const fn at(off: u32) -> GbaPtr {
    GbaPtr::at(ROM_BASE + off)
}

fn idx(off: u32) -> usize {
    usize::try_from(off).expect("a ROM offset fits in usize")
}

/// The address of member `k` (in struct order) of the grass or plain record.
const fn member(base: u32, k: u32) -> u32 {
    base + k * 0x1_0000
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

fn stored(encoding: Encoding, payload: &[u8]) -> Vec<u8> {
    match encoding {
        Encoding::Raw => payload.to_vec(),
        Encoding::Lz77 => lz77_literals(payload),
    }
}

/// Packed 4bpp tiles whose tile 17 (second row, second column of a 16-wide
/// sheet) alone carries pixels, `0..=7` in its first row.
fn sheet(marker: u8) -> Vec<u8> {
    let mut tiles = vec![0u8; SHEET_BYTES];
    tiles[17 * 32..17 * 32 + 4].copy_from_slice(&[0x10, 0x32, 0x54, 0x76]);
    tiles[0] = marker;
    tiles
}

/// Distinct little-endian tilemap bytes, bank bits and all.
fn tilemap(len: usize, marker: u8) -> Vec<u8> {
    (0..len)
        .map(|i| u8::try_from(i % 251).unwrap().wrapping_add(marker))
        .collect()
}

fn colors(marker: u8) -> Vec<u8> {
    let mut palette = vec![0u8; PALETTE_BYTES];
    palette[..2].copy_from_slice(&(0x7C00 | u16::from(marker)).to_le_bytes());
    palette
}

/// The decoded bytes of member `k` of the record at `base`.
fn payload(base: u32, k: u32) -> Vec<u8> {
    let marker = u8::try_from(base >> 16).unwrap() * 16 + u8::try_from(k).unwrap();
    match k {
        0 | 2 => sheet(marker),
        1 => tilemap(MAIN_MAP_BYTES, marker),
        3 => tilemap(ENTRY_MAP_BYTES, marker),
        _ => colors(marker),
    }
}

fn row(base: u32) -> Vec<u8> {
    (0..5)
        .flat_map(|k| at(member(base, k)).raw().to_le_bytes())
        .collect()
}

fn fixture(encoding: Encoding) -> RomFixture {
    let mut fixture = RomFixture::new()
        .emerald_header()
        .write(idx(TABLE) + GRASS_ROW * RECORD_BYTES, &row(GRASS_BASE))
        .write(idx(TABLE) + PLAIN_ROW * RECORD_BYTES, &row(PLAIN_BASE));
    for base in [GRASS_BASE, PLAIN_BASE] {
        for k in 0..5 {
            fixture = fixture.write(idx(member(base, k)), &stored(encoding, &payload(base, k)));
        }
    }
    for k in 0..5 {
        fixture = fixture.write(
            idx(STRAY) + 0x4000 * idx(k),
            &stored(encoding, &payload(STRAY, k)),
        );
    }
    fixture
}

fn rom_of(fixture: RomFixture) -> Rom {
    Rom::from_bytes(fixture.finish()).expect("the fixture header is valid")
}

const fn image(id: &'static str, addr: GbaPtr, encoding: Encoding) -> ImageRoot {
    ImageRoot {
        id,
        addr,
        encoding,
        rom_bit_depth: 4,
        pack_bit_depth: 4,
        width: 128,
        height: 128,
        metatile_width: 1,
        metatile_height: 1,
        tile_count: 256,
    }
}

const fn blob(id: &'static str, addr: GbaPtr, encoding: Encoding, len: u32) -> BlobRoot {
    BlobRoot {
        id,
        addr,
        encoding,
        len,
    }
}

fn grass(encoding: Encoding) -> BattleEnvironmentRoot {
    let at = |k| at(member(GRASS_BASE, k));
    BattleEnvironmentRoot {
        tiles: image("battle-environment/image/tall_grass/tiles", at(0), encoding),
        tilemap: blob(
            "battle-environment/raw/tall_grass/map",
            at(1),
            encoding,
            4096,
        ),
        entry_tiles: image(
            "battle-environment/image/tall_grass/anim_tiles",
            at(2),
            encoding,
        ),
        entry_tilemap: blob(
            "battle-environment/raw/tall_grass/anim_map",
            at(3),
            encoding,
            2048,
        ),
        palette: PaletteRoot {
            id: "battle-environment/palette/tall_grass",
            addr: at(4),
            color_count: 48,
            encoding,
        },
    }
}

fn plain(encoding: Encoding) -> BattleEnvironmentRoot {
    let at = |k| at(member(PLAIN_BASE, k));
    BattleEnvironmentRoot {
        tiles: image("battle-environment/image/building/tiles", at(0), encoding),
        tilemap: blob("battle-environment/raw/building/map", at(1), encoding, 4096),
        entry_tiles: image(
            "battle-environment/image/building/anim_tiles",
            at(2),
            encoding,
        ),
        entry_tilemap: blob(
            "battle-environment/raw/building/anim_map",
            at(3),
            encoding,
            2048,
        ),
        palette: PaletteRoot {
            id: "battle-environment/palette/plain",
            addr: at(4),
            color_count: 48,
            encoding,
        },
    }
}

fn roots_of(tall_grass: BattleEnvironmentRoot, plain: BattleEnvironmentRoot) -> Roots {
    Roots {
        battle_environment: BattleEnvironmentRoots {
            table: at(TABLE),
            tall_grass: Some(tall_grass),
            plain: Some(plain),
        },
        ..Roots::NONE
    }
}

fn full(encoding: Encoding) -> Roots {
    roots_of(grass(encoding), plain(encoding))
}

fn written_bytes(rom: &Rom, roots: &Roots) -> Result<Vec<u8>, ImportError> {
    let mut writer = PackWriter::new();
    write(rom, roots, &mut writer)?;
    Ok(writer.finish().expect("no duplicate ids"))
}

fn written(rom: &Rom, roots: &Roots) -> Result<Vec<DirectoryEntry>, ImportError> {
    written_bytes(rom, roots).map(|bytes| parse_directory(&bytes).expect("a valid pack"))
}

fn body<'a>(bytes: &'a [u8], entries: &[DirectoryEntry], id: &str) -> &'a [u8] {
    let entry = entries.iter().find(|e| e.id == id).expect(id);
    &bytes[entry.offset..entry.offset + entry.length]
}

#[test]
fn raw_and_compressed_roots_emit_the_same_ten_entries() {
    let compressed = written_bytes(&rom_of(fixture(Encoding::Lz77)), &full(Encoding::Lz77))
        .expect("a well-formed compressed ROM");
    let raw = written_bytes(&rom_of(fixture(Encoding::Raw)), &full(Encoding::Raw))
        .expect("a well-formed raw ROM");
    assert_eq!(compressed, raw);

    let entries = parse_directory(&compressed).expect("a valid pack");
    let mut ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
    ids.sort_unstable();
    assert_eq!(
        ids,
        [
            "battle-environment/image/building/anim_tiles",
            "battle-environment/image/building/tiles",
            "battle-environment/image/tall_grass/anim_tiles",
            "battle-environment/image/tall_grass/tiles",
            "battle-environment/palette/plain",
            "battle-environment/palette/tall_grass",
            "battle-environment/raw/building/anim_map",
            "battle-environment/raw/building/map",
            "battle-environment/raw/tall_grass/anim_map",
            "battle-environment/raw/tall_grass/map",
        ]
    );
}

#[test]
fn each_member_lands_as_its_decoded_bytes() {
    let bytes = written_bytes(&rom_of(fixture(Encoding::Lz77)), &full(Encoding::Lz77))
        .expect("a well-formed ROM");
    let entries = parse_directory(&bytes).expect("a valid pack");
    let kind = |id: &str| entries.iter().find(|e| e.id == id).expect(id).kind;

    for (base, name) in [(GRASS_BASE, "tall_grass"), (PLAIN_BASE, "building")] {
        let id = |kind: &str, leaf: &str| format!("battle-environment/{kind}/{name}/{leaf}");
        for (k, leaf) in [(0, "tiles"), (2, "anim_tiles")] {
            let id = id("image", leaf);
            assert_eq!(
                kind(&id),
                EntryKind::Image {
                    width: 128,
                    height: 128,
                    bit_depth: 4
                }
            );
            let pixels = body(&bytes, &entries, &id);
            assert_eq!(pixels.len(), 128 * 128);
            // Tile 0 opens with the member's marker byte: low nibble `k`, high
            // nibble the record's base, so a swapped member fails.
            assert_eq!(
                pixels[..2],
                [u8::try_from(k).unwrap(), u8::try_from(base >> 16).unwrap()]
            );
            // Tile 17 sits at column 1, row 1 of a 16-wide sheet: its first
            // row is pixels (8..16, 8), which proves 1x1 tile ordering.
            assert_eq!(pixels[8 * 128 + 8..8 * 128 + 16], [0, 1, 2, 3, 4, 5, 6, 7]);
            assert_eq!(
                pixels.iter().filter(|&&p| p != 0).count(),
                7 + 1 + usize::from(k != 0)
            );
        }
        assert_eq!(kind(&id("raw", "map")), EntryKind::Raw);
        assert_eq!(body(&bytes, &entries, &id("raw", "map")), payload(base, 1));
        assert_eq!(
            body(&bytes, &entries, &id("raw", "anim_map")),
            payload(base, 3)
        );
    }
    let palette = |id: &str| body(&bytes, &entries, id).to_vec();
    assert_eq!(
        palette("battle-environment/palette/tall_grass"),
        payload(GRASS_BASE, 4)
    );
    assert_eq!(
        palette("battle-environment/palette/plain"),
        payload(PLAIN_BASE, 4)
    );
    assert_eq!(
        kind("battle-environment/palette/plain"),
        EntryKind::Palette { color_count: 48 }
    );
}

#[test]
fn the_domain_list_dispatches_the_battle_environments() {
    let rom = rom_of(fixture(Encoding::Lz77));
    let roots = full(Encoding::Lz77);
    let mut writer = PackWriter::new();
    for domain in DOMAINS {
        domain(&rom, &roots, &mut writer).expect("every root is well-formed");
    }
    let bytes = writer.finish().expect("no duplicate ids");
    assert_eq!(parse_directory(&bytes).expect("a valid pack").len(), 10);
}

#[test]
fn a_profile_without_the_records_writes_nothing() {
    assert!(written(&rom_of(fixture(Encoding::Lz77)), &Roots::NONE)
        .expect("nothing to read")
        .is_empty());
    assert_eq!(Roots::NONE.battle_environment, BattleEnvironmentRoots::NONE);
}

#[test]
fn a_single_record_profile_writes_only_that_record() {
    let mut roots = full(Encoding::Lz77);
    roots.battle_environment.plain = None;
    let entries = written(&rom_of(fixture(Encoding::Lz77)), &roots).expect("a grass-only ROM");
    assert_eq!(entries.len(), 5);
    assert!(entries.iter().all(|e| e.id.contains("tall_grass")));
}

#[test]
fn the_records_are_enumerated_as_four_images_and_two_palettes() {
    let roots = full(Encoding::Lz77);
    let images: Vec<&str> = roots.images().map(|root| root.id).collect();
    let palettes: Vec<&str> = roots.palettes().map(|root| root.id).collect();
    assert_eq!(
        images,
        [
            "battle-environment/image/tall_grass/tiles",
            "battle-environment/image/tall_grass/anim_tiles",
            "battle-environment/image/building/tiles",
            "battle-environment/image/building/anim_tiles",
        ]
    );
    assert_eq!(
        palettes,
        [
            "battle-environment/palette/tall_grass",
            "battle-environment/palette/plain",
        ]
    );
}

/// A record with member `k` replaced by the same kind of stream at `addr`.
fn redirected(record: BattleEnvironmentRoot, k: usize, addr: GbaPtr) -> BattleEnvironmentRoot {
    let mut record = record;
    match k {
        0 => record.tiles.addr = addr,
        1 => record.tilemap.addr = addr,
        2 => record.entry_tiles.addr = addr,
        3 => record.entry_tilemap.addr = addr,
        _ => record.palette.addr = addr,
    }
    record
}

#[test]
fn every_pointer_of_both_rows_must_match_the_profile() {
    let rom = rom_of(fixture(Encoding::Lz77));
    for (k, field) in FIELDS.into_iter().enumerate() {
        let stray = at(STRAY + 0x4000 * u32::try_from(k).unwrap());
        for row in [GRASS_ROW, PLAIN_ROW] {
            let roots = if row == GRASS_ROW {
                roots_of(
                    redirected(grass(Encoding::Lz77), k, stray),
                    plain(Encoding::Lz77),
                )
            } else {
                roots_of(
                    grass(Encoding::Lz77),
                    redirected(plain(Encoding::Lz77), k, stray),
                )
            };
            match written(&rom, &roots) {
                Err(ImportError::StructMismatch { field: named, .. }) => {
                    assert_eq!(named, field, "row {row}");
                }
                other => panic!("row {row} {field}: expected a mismatch, got {other:?}"),
            }
        }
    }
}

#[test]
fn a_row_holding_a_non_cartridge_pointer_fails_closed() {
    let mut bad_row = row(GRASS_BASE);
    bad_row[..4].copy_from_slice(&0x0200_0000u32.to_le_bytes());
    let rom = rom_of(fixture(Encoding::Lz77).write(idx(TABLE), &bad_row));
    let err = written(&rom, &full(Encoding::Lz77)).unwrap_err();
    assert!(
        matches!(err, ImportError::PointerOutOfRange { .. }),
        "{err}"
    );
}

#[test]
fn a_table_or_payload_past_the_rom_fails_closed() {
    let past_the_end = u32::try_from(ROM_SIZE).expect("the ROM fits a bus address") - 4;
    let mut roots = full(Encoding::Lz77);
    roots.battle_environment.table = at(past_the_end);
    let err = written(&rom_of(fixture(Encoding::Lz77)), &roots).unwrap_err();
    assert!(matches!(err, ImportError::Truncated { .. }), "{err}");

    // A raw root whose row agrees but whose bytes run off the image.
    let end = at(past_the_end);
    let mut tail_row = row(GRASS_BASE);
    tail_row[16..20].copy_from_slice(&end.raw().to_le_bytes());
    let rom = rom_of(fixture(Encoding::Raw).write(idx(TABLE), &tail_row));
    let mut roots = full(Encoding::Raw);
    roots.battle_environment.tall_grass = Some(redirected(grass(Encoding::Raw), 4, end));
    let err = written(&rom, &roots).unwrap_err();
    assert!(matches!(err, ImportError::Truncated { .. }), "{err}");
}

#[test]
fn a_malformed_stream_is_a_typed_error_per_member_kind() {
    let wrong_type = |k: u32| {
        let rom = rom_of(fixture(Encoding::Lz77).write(idx(member(GRASS_BASE, k)), &[0x11]));
        written(&rom, &full(Encoding::Lz77)).unwrap_err()
    };
    for k in 0..5 {
        assert!(
            matches!(
                wrong_type(k),
                ImportError::Lz77 {
                    fault: Lz77Fault::WrongType,
                    ..
                }
            ),
            "member {k}"
        );
    }
}

#[test]
fn a_stream_of_the_wrong_length_is_a_size_mismatch() {
    let short = |k: u32, bytes: usize| {
        let rom = rom_of(
            fixture(Encoding::Lz77)
                .write(idx(member(PLAIN_BASE, k)), &lz77_literals(&vec![0; bytes])),
        );
        written(&rom, &full(Encoding::Lz77)).unwrap_err()
    };
    for (k, bytes) in [
        (0, SHEET_BYTES / 2),
        (1, ENTRY_MAP_BYTES),
        (2, SHEET_BYTES / 2),
        (3, MAIN_MAP_BYTES),
        (4, PALETTE_BYTES / 3),
    ] {
        assert!(
            matches!(
                short(k, bytes),
                ImportError::Lz77 {
                    fault: Lz77Fault::SizeMismatch { .. },
                    ..
                }
            ),
            "member {k}"
        );
    }
}

#[test]
fn a_failing_last_root_queues_nothing() {
    // Every grass read succeeds before the plain palette fails.
    let rom = rom_of(fixture(Encoding::Lz77).write(idx(member(PLAIN_BASE, 4)), &[0x11]));
    let mut writer = PackWriter::new();
    write(&rom, &full(Encoding::Lz77), &mut writer).unwrap_err();
    assert_eq!(writer.len(), 0);
}
