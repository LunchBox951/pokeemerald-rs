use std::collections::BTreeSet;
use std::fmt::Write;
use std::path::{Path, PathBuf};

use assets::species_icons::{
    decode_palette_table, decode_species_sheets, PALETTE_INDICES_ID, PALETTE_TABLE_ID,
    SPECIES_SHEETS_ID,
};
use pack_format::{DirectoryEntry, EntryKind, PackWriter};

use super::fixture::{declarations, header, tables};
use super::{extract_species_icons, pack_ids, source};
use crate::extract::owned_fixture::OwnedDir;
use crate::extract::png::tests::indexed_png_from_raw_with_plte;
use crate::extract::{extract_to, repo_root, upstream_present, ExtractError};

const PALETTE_SEEDS: [u8; 3] = [10, 60, 110];

fn checkout() -> OwnedDir {
    OwnedDir::create("pokeemerald-rs-species-icons").unwrap()
}

fn write(upstream: &Path, relative: &str, bytes: impl AsRef<[u8]>) -> PathBuf {
    let path = upstream.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, bytes).unwrap();
    path
}

/// The sheet a PNG at `png` holds: a distinct raster per file name.
fn sheet_pixels(png: &str, width: u32, height: u32) -> Vec<u8> {
    let seed = png
        .bytes()
        .fold(0u32, |a, b| a.wrapping_mul(31).wrapping_add(u32::from(b)));
    (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .map(|(x, y)| u8::try_from((seed + x + 3 * y) % 16).unwrap())
        .collect()
}

fn sheet_png(png: &str, (width, height): (u32, u32), depth: u8) -> Vec<u8> {
    let pixels = sheet_pixels(png, width, height);
    let mut raw = Vec::new();
    for row in pixels.chunks(width as usize) {
        raw.push(0);
        if depth == 4 {
            raw.extend(row.chunks(2).map(|pair| (pair[0] << 4) | pair[1]));
        } else {
            raw.extend_from_slice(row);
        }
    }
    // Embedded colours the extractor must not publish.
    let plte: Vec<u8> = (0..16u8).flat_map(|i| [i, 255 - i, 7]).collect();
    indexed_png_from_raw_with_plte(depth, width, height, &raw, &plte)
}

fn palette_text(bank: usize, colors: usize) -> String {
    let mut text = format!("JASC-PAL\n0100\n{colors}\n");
    for index in 0..colors {
        let seed = usize::from(PALETTE_SEEDS[bank]);
        writeln!(
            text,
            "{} {} {}",
            (seed + index * 8) % 256,
            index * 8,
            16 * (bank + 1)
        )
        .unwrap();
    }
    text
}

fn expected_palette(bank: usize) -> Vec<u8> {
    (0..16)
        .flat_map(|index| {
            let seed = usize::from(PALETTE_SEEDS[bank]);
            let (r, g, b) = ((seed + index * 8) % 256, index * 8, 16 * (bank + 1));
            let color = (r >> 3) | ((g >> 3) << 5) | ((b >> 3) << 10);
            u16::try_from(color).unwrap().to_le_bytes()
        })
        .collect()
}

fn write_valid_checkout(upstream: &Path) {
    write(upstream, "include/constants/species.h", header());
    write(upstream, "src/pokemon_icon.c", tables());
    write(upstream, "src/data/graphics/pokemon.h", declarations());
    write(
        upstream,
        "src/graphics.c",
        "const u16 gMonIconPalettes[][16] = {};\n",
    );
    let inventory = source::parse(&header(), &tables(), &[&declarations()]).unwrap();
    for (number, sheet) in inventory.sheets.iter().enumerate() {
        let wide = number == 416;
        let size = if wide { (128, 64) } else { (32, 64) };
        write(upstream, &sheet.png, sheet_png(&sheet.png, size, 4));
    }
    for bank in 0..3 {
        write(
            upstream,
            &format!("graphics/pokemon/icon_palettes/icon_palette_{bank}.pal"),
            palette_text(bank, 16),
        );
    }
}

fn extract(upstream: &Path) -> Result<Vec<u8>, ExtractError> {
    let mut writer = PackWriter::new();
    extract_species_icons(upstream, &mut writer)?;
    Ok(writer.finish().unwrap())
}

fn payload<'a>(pack: &'a [u8], entry: &DirectoryEntry) -> &'a [u8] {
    &pack[entry.offset..entry.offset + entry.length]
}

fn mismatch_path(error: ExtractError) -> PathBuf {
    match error {
        ExtractError::SpeciesIconAssetMismatch { path, .. }
        | ExtractError::ReadFailed(path, _)
        | ExtractError::Png(path, _)
        | ExtractError::Pal(path, _) => path,
        other => panic!("expected a species-icon source fault, got {other}"),
    }
}

#[test]
fn the_inventory_is_417_sheets_three_banks_and_three_tables() {
    let ids = pack_ids();
    assert_eq!(ids.len(), 423);
    assert_eq!(ids.iter().collect::<BTreeSet<_>>().len(), 423);
    assert_eq!(ids[0], "species-icon/image/000");
    assert_eq!(ids[416], "species-icon/image/416");
    assert_eq!(
        &ids[417..],
        [
            "species-icon/palette/0",
            "species-icon/palette/1",
            "species-icon/palette/2",
            "species-icon/raw/species-sheets",
            "species-icon/raw/palette-indices",
            "species-icon/raw/palette-table",
        ]
    );
}

#[test]
fn a_complete_checkout_yields_every_entry_with_its_shape_and_payload() {
    let upstream = checkout();
    write_valid_checkout(upstream.path());
    let pack = extract(upstream.path()).unwrap();
    let entries = pack_format::parse_directory(&pack).unwrap();
    assert_eq!(
        entries
            .iter()
            .map(|e| e.id.as_str())
            .collect::<BTreeSet<_>>(),
        pack_ids()
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
    );
    let by_id = |id: &str| entries.iter().find(|e| e.id == id).unwrap();
    let inventory = source::parse(&header(), &tables(), &[&declarations()]).unwrap();

    for (number, sheet) in inventory.sheets.iter().enumerate() {
        let entry = by_id(&format!("species-icon/image/{number:03}"));
        let (width, height) = if number == 416 { (128, 64) } else { (32, 64) };
        assert_eq!(
            entry.kind,
            EntryKind::Image {
                width,
                height,
                bit_depth: 4
            },
            "{number}"
        );
        // The PNG's own indices, not its embedded colours.
        assert_eq!(
            payload(&pack, entry),
            sheet_pixels(&sheet.png, width, height).as_slice(),
            "{number}"
        );
    }
    for bank in 0..3 {
        let entry = by_id(&format!("species-icon/palette/{bank}"));
        assert_eq!(entry.kind, EntryKind::Palette { color_count: 16 });
        assert_eq!(payload(&pack, entry), expected_palette(bank).as_slice());
    }

    let sheets = decode_species_sheets(payload(&pack, by_id(SPECIES_SHEETS_ID))).unwrap();
    assert_eq!(sheets, inventory.species_sheets);
    // Slots 0 and 1 share a sheet, slots 2..=26 share another, and the table
    // keeps all 440 rows over 415 stored sheets.
    assert_eq!(sheets[0], sheets[1]);
    assert!(sheets[2..=26].iter().all(|&s| s == sheets[2]));
    assert_eq!(sheets.iter().collect::<BTreeSet<_>>().len(), 415);
    assert_eq!(
        payload(&pack, by_id(PALETTE_INDICES_ID)),
        inventory.palette_indices.as_slice()
    );
    assert!(decode_palette_table(payload(&pack, by_id(PALETTE_TABLE_ID))).is_ok());
}

#[test]
fn extraction_is_deterministic() {
    let upstream = checkout();
    write_valid_checkout(upstream.path());
    assert_eq!(
        extract(upstream.path()).unwrap(),
        extract(upstream.path()).unwrap()
    );
}

#[test]
fn files_the_manifest_does_not_name_cannot_change_the_pack() {
    let upstream = checkout();
    write_valid_checkout(upstream.path());
    let clean = extract(upstream.path()).unwrap();
    write(
        upstream.path(),
        "graphics/pokemon/mon5/icon_other.png",
        b"not a png",
    );
    write(
        upstream.path(),
        "graphics/pokemon/icon_palettes/icon_palette_3.pal",
        b"junk",
    );
    write(
        upstream.path(),
        "graphics/pokemon/mon6/icon.png.bak",
        b"junk",
    );
    assert_eq!(extract(upstream.path()).unwrap(), clean);
}

#[test]
fn a_missing_source_fails_naming_that_file_and_queues_nothing() {
    let sources = [
        "include/constants/species.h",
        "src/pokemon_icon.c",
        "src/data/graphics/pokemon.h",
        "graphics/pokemon/mon40/icon.png",
        "graphics/pokemon/deoxys/icon_speed_wide.png",
        "graphics/pokemon/icon_palettes/icon_palette_0.pal",
        "graphics/pokemon/icon_palettes/icon_palette_2.pal",
    ];
    for missing in sources {
        let upstream = checkout();
        write_valid_checkout(upstream.path());
        std::fs::remove_file(upstream.path().join(missing)).unwrap();
        let mut writer = PackWriter::new();
        let error = extract_species_icons(upstream.path(), &mut writer).unwrap_err();
        assert_eq!(
            mismatch_path(error),
            upstream.path().join(missing),
            "{missing}"
        );
        assert_eq!(writer.len(), 0, "{missing}");
    }
}

#[test]
fn a_malformed_sheet_or_palette_is_rejected_before_anything_is_queued() {
    let sheet = "graphics/pokemon/mon40/icon.png";
    let wide = "graphics/pokemon/deoxys/icon_speed_wide.png";
    let bad: Vec<(&str, Vec<u8>)> = vec![
        (sheet, sheet_png(sheet, (32, 32), 4)),
        (sheet, sheet_png(sheet, (64, 32), 4)),
        (sheet, sheet_png(sheet, (32, 64), 8)),
        (sheet, sheet_png(sheet, (128, 64), 4)),
        (wide, sheet_png(wide, (32, 64), 4)),
        (sheet, b"not a png".to_vec()),
    ];
    for (n, (path, bytes)) in bad.into_iter().enumerate() {
        let upstream = checkout();
        write_valid_checkout(upstream.path());
        write(upstream.path(), path, bytes);
        let mut writer = PackWriter::new();
        let error = extract_species_icons(upstream.path(), &mut writer).unwrap_err();
        assert_eq!(mismatch_path(error), upstream.path().join(path), "case {n}");
        assert_eq!(writer.len(), 0, "case {n}");
    }

    let bank = "graphics/pokemon/icon_palettes/icon_palette_1.pal";
    for (n, text) in [
        palette_text(1, 15),
        palette_text(1, 32),
        "JASC-PAL\n".to_owned(),
    ]
    .into_iter()
    .enumerate()
    {
        let upstream = checkout();
        write_valid_checkout(upstream.path());
        write(upstream.path(), bank, text);
        let mut writer = PackWriter::new();
        let error = extract_species_icons(upstream.path(), &mut writer).unwrap_err();
        assert_eq!(
            mismatch_path(error),
            upstream.path().join(bank),
            "palette {n}"
        );
        assert_eq!(writer.len(), 0, "palette {n}");
    }
}

#[test]
fn a_source_table_out_of_range_is_rejected() {
    let upstream = checkout();
    write_valid_checkout(upstream.path());
    let doctored = tables().replace("[SPECIES_A9] = 0,", "[SPECIES_A9] = 3,");
    assert_ne!(doctored, tables());
    let path = write(upstream.path(), "src/pokemon_icon.c", doctored);
    let mut writer = PackWriter::new();
    let error = extract_species_icons(upstream.path(), &mut writer).unwrap_err();
    assert_eq!(mismatch_path(error), path);
    assert_eq!(writer.len(), 0);
}

#[test]
#[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
fn the_real_checkout_yields_the_whole_family() {
    assert!(upstream_present(), "run ./init.sh first");
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-extract-test-species-icons-{}.pack",
        std::process::id()
    ));
    extract_to(&path).expect("extraction should succeed against a real checkout");
    let pack = std::fs::read(&path).unwrap();
    let entries = pack_format::parse_directory(&pack).unwrap();
    let family: Vec<_> = entries
        .iter()
        .filter(|e| e.id.starts_with("species-icon/"))
        .collect();
    assert_eq!(family.len(), 423);
    assert_eq!(
        family
            .iter()
            .map(|e| e.id.as_str())
            .collect::<BTreeSet<_>>(),
        pack_ids()
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
    );
    let by_id = |id: &str| *family.iter().find(|e| e.id == id).unwrap();

    // 417 sheets: 416 of 32x64 and the unused 128x64 one.
    for number in 0..417 {
        let entry = by_id(&format!("species-icon/image/{number:03}"));
        let (width, height) = if number == 416 { (128, 64) } else { (32, 64) };
        assert_eq!(
            entry.kind,
            EntryKind::Image {
                width,
                height,
                bit_depth: 4
            }
        );
        assert_eq!(entry.length, (width * height) as usize);
    }

    // The table: 440 rows over 415 stored sheets, with sharing intact.
    let sheets = decode_species_sheets(payload(&pack, by_id(SPECIES_SHEETS_ID))).unwrap();
    assert_eq!(sheets.len(), 440);
    assert_eq!(sheets.iter().collect::<BTreeSet<_>>().len(), 415);
    // Expected sheet numbers, derived from `gMonIconTable`'s first-appearance
    // order: SPECIES_NONE shares Bulbasaur (0); Unown A is 200; the 25 old
    // Unown slots share the question mark (251); Treecko follows Celebi's
    // neighbours at 252; Deoxys 385, Chimecho 386, Egg 387, then the Unown
    // forms 388..=414 ending with ! and ?.
    let expected = [
        (0, 0),
        (1, 0),
        (201, 200),
        (252, 251),
        (260, 251),
        (276, 251),
        (277, 252),
        (410, 385),
        (411, 386),
        (412, 387),
        (413, 388),
        (437, 412),
        (438, 413),
        (439, 414),
    ];
    for (species, sheet) in expected {
        assert_eq!(sheets[species], sheet, "species {species}");
    }
    assert!((252..=276).all(|s| sheets[s] == 251));

    // Palette banks per species: NONE and Bulbasaur share a sheet but not a bank.
    let indices = payload(&pack, by_id(PALETTE_INDICES_ID));
    assert_eq!(indices.len(), 440);
    for (species, bank) in [
        (0, 0),
        (1, 1),
        (8, 2),
        (201, 0),
        (277, 1),
        (412, 1),
        (439, 0),
    ] {
        assert_eq!(indices[species], bank, "species {species}");
    }
    assert!(indices.iter().all(|&b| b < 3));

    // The banks as BGR555: colour 0 is the shared backdrop.
    for (bank, second) in [(0, 0x3A10u16), (1, 0x39CE), (2, 0x3DEF)] {
        let entry = by_id(&format!("species-icon/palette/{bank}"));
        assert_eq!(entry.kind, EntryKind::Palette { color_count: 16 });
        let colors = payload(&pack, entry);
        assert_eq!(colors[..2], 0x426Cu16.to_le_bytes(), "bank {bank}");
        assert_eq!(colors[2..4], second.to_le_bytes(), "bank {bank}");
    }

    // Sheets are the PNG's own indices.
    let upstream = repo_root().join("pokeemerald");
    for (number, png) in [
        (0, "graphics/pokemon/bulbasaur/icon.png"),
        (251, "graphics/pokemon/question_mark/icon.png"),
        (387, "graphics/pokemon/egg/icon.png"),
        (415, "graphics/pokemon/deoxys/icon_speed.png"),
        (416, "graphics/pokemon/deoxys/icon_speed_wide.png"),
    ] {
        let bytes = std::fs::read(upstream.join(png)).unwrap();
        let image = crate::extract::png::decode(&bytes).unwrap();
        let entry = by_id(&format!("species-icon/image/{number:03}"));
        assert_eq!(payload(&pack, entry), image.pixels.as_slice(), "{png}");
    }
    let _ = std::fs::remove_file(path);
}
