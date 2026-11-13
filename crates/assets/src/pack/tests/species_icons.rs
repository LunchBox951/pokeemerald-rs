use super::super::{AssetPack, PackError};
use super::shared::{
    image_meta, pack_bytes, write_pack, Entry, IMAGE_KIND_TAG, PALETTE_KIND_TAG, RAW_KIND_TAG,
    TEXT_WINDOW_BIT_DEPTH,
};
use crate::species::SpeciesId;
use crate::species_icons::{
    encode_palette_table, encode_species_sheets, SpeciesIconSheetId, PALETTE_INDICES_ID,
    PALETTE_TABLE_ID, SHEETS, SPECIES_SHEETS_ID, SPECIES_SLOTS, SPEED_SHEET, SPEED_WIDE_SHEET,
};

fn leak(id: String) -> &'static str {
    Box::leak(id.into_boxed_str())
}

fn sheet_id(n: u16) -> &'static str {
    leak(SpeciesIconSheetId::new(n).unwrap().pack_id())
}

fn palette_id(bank: u8) -> &'static str {
    leak(crate::species_icons::palette_pack_id(bank))
}

fn image(id: &'static str, (width, height): (u32, u32), depth: u8, pixel: u8) -> Entry {
    Entry {
        id,
        kind_tag: IMAGE_KIND_TAG,
        meta: image_meta(width, height, depth),
        payload: (0..width * height)
            .map(|i| if i == 0 { pixel } else { (i % 16) as u8 })
            .collect(),
    }
}

fn palette(id: &'static str, colors: u16, first: u16) -> Entry {
    let mut payload = vec![0u8; usize::from(colors) * 2];
    payload[..2].copy_from_slice(&first.to_le_bytes());
    Entry {
        id,
        kind_tag: PALETTE_KIND_TAG,
        meta: colors.to_le_bytes().to_vec(),
        payload,
    }
}

fn raw(id: &'static str, payload: Vec<u8>) -> Entry {
    Entry {
        id,
        kind_tag: RAW_KIND_TAG,
        meta: vec![],
        payload,
    }
}

/// Species 0 and 1 share sheet 0 but pick banks 0 and 1; species 2..=26 share
/// sheet 5; the rest take sheets in order.
fn species_sheets() -> Vec<u16> {
    (0..SPECIES_SLOTS)
        .map(|slot| match slot {
            0 | 1 => 0,
            2..=26 => 5,
            _ => u16::try_from(slot % 100).unwrap() + 10,
        })
        .collect()
}

fn palette_indices() -> Vec<u8> {
    (0..SPECIES_SLOTS)
        .map(|slot| u8::try_from(slot % 3).unwrap())
        .collect()
}

fn fixture_entries() -> Vec<Entry> {
    let mut entries = Vec::new();
    for n in 0..SHEETS {
        let size = SpeciesIconSheetId::new(n).unwrap().size();
        entries.push(image(sheet_id(n), size, TEXT_WINDOW_BIT_DEPTH, 15));
    }
    for bank in 0..3u8 {
        entries.push(palette(palette_id(bank), 16, 0x1000 + u16::from(bank)));
    }
    entries.push(raw(
        SPECIES_SHEETS_ID,
        encode_species_sheets(&species_sheets()).unwrap(),
    ));
    entries.push(raw(PALETTE_INDICES_ID, palette_indices()));
    entries.push(raw(PALETTE_TABLE_ID, encode_palette_table()));
    entries
}

fn replaced(entry: Entry) -> Vec<Entry> {
    let mut entries: Vec<Entry> = fixture_entries()
        .into_iter()
        .filter(|e| e.id != entry.id)
        .collect();
    entries.push(entry);
    entries
}

fn load(name: &str, entries: Vec<Entry>) -> (AssetPack, std::path::PathBuf) {
    let path = write_pack(name, &pack_bytes(entries));
    (AssetPack::load(&path).unwrap(), path)
}

fn assert_malformed(error: &PackError, id: &str, context: &str) {
    assert!(
        matches!(error, PackError::MalformedSpeciesIconAsset { id: bad, .. } if bad == id),
        "{context}: {error:?}"
    );
}

#[test]
fn shared_sheets_keep_their_identity_and_own_palette_rows() {
    let (pack, path) = load("species-icon-shared", fixture_entries());

    let none = pack.species_icon(SpeciesId(0)).unwrap();
    let bulbasaur = pack.species_icon(SpeciesId(1)).unwrap();
    assert_eq!(none.sheet_id, bulbasaur.sheet_id);
    assert!(std::ptr::eq(none.sheet.pixels, bulbasaur.sheet.pixels));
    assert_eq!((none.palette_index, bulbasaur.palette_index), (0, 1));
    assert_eq!((none.palette_tag, bulbasaur.palette_tag), (56000, 56001));
    assert_eq!(none.palette.color(0), Some(0x1000));
    assert_eq!(bulbasaur.palette.color(0), Some(0x1001));
    assert_eq!((none.sheet.width, none.sheet.height), (32, 64));
    assert_eq!(none.sheet.bit_depth, 4);

    for slot in 2..=26 {
        assert_eq!(
            pack.species_icon_sheet_id(SpeciesId(slot)).unwrap().index(),
            5
        );
    }
    let last = pack.species_icon(SpeciesId(439)).unwrap();
    assert_eq!(last.palette_index, 1);
    assert_eq!(last.species, SpeciesId(439));

    let _ = std::fs::remove_file(path);
}

#[test]
fn species_past_the_table_has_no_row() {
    let (pack, path) = load("species-icon-range", fixture_entries());
    for species in [440, 1000, u16::MAX] {
        assert_malformed(
            &pack.species_icon(SpeciesId(species)).unwrap_err(),
            SPECIES_SHEETS_ID,
            "species past the sheet table",
        );
        assert_malformed(
            &pack
                .species_icon_palette_index(SpeciesId(species))
                .unwrap_err(),
            PALETTE_INDICES_ID,
            "species past the index table",
        );
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn the_deoxys_only_sheets_read_directly_but_no_slot_names_them() {
    let (pack, path) = load("species-icon-extra", fixture_entries());
    let speed = pack
        .species_icon_sheet(SpeciesIconSheetId::new(SPEED_SHEET).unwrap())
        .unwrap();
    assert_eq!((speed.width, speed.height), (32, 64));
    let wide = pack
        .species_icon_sheet(SpeciesIconSheetId::new(SPEED_WIDE_SHEET).unwrap())
        .unwrap();
    assert_eq!((wide.width, wide.height), (128, 64));
    for bad in [SPEED_SHEET, SPEED_WIDE_SHEET, u16::MAX] {
        let mut sheets = species_sheets();
        sheets[7] = bad;
        let (pack, bad_path) = load(
            "species-icon-extra-slot",
            replaced(raw(
                SPECIES_SHEETS_ID,
                sheets.iter().flat_map(|s| s.to_le_bytes()).collect(),
            )),
        );
        assert_malformed(
            &pack.species_icon(SpeciesId(7)).unwrap_err(),
            SPECIES_SHEETS_ID,
            "a slot naming a Deoxys-only sheet",
        );
        let _ = std::fs::remove_file(bad_path);
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn palette_records_keep_the_three_banks_past_the_array_unreadable() {
    let (pack, path) = load("species-icon-records", fixture_entries());
    let records = pack.species_icon_palette_records().unwrap();
    for (index, record) in (0..6u8).zip(&records) {
        assert_eq!(record.tag, 56000 + u16::from(index));
        assert_eq!(record.bank, (index < 3).then_some(index));
    }
    for bank in 0..3 {
        assert_eq!(pack.species_icon_palette(bank).unwrap().color_count, 16);
    }
    for bank in [3, 4, 5, 255] {
        assert_malformed(
            &pack.species_icon_palette(bank).unwrap_err(),
            &crate::species_icons::palette_pack_id(bank),
            "a bank past gMonIconPalettes",
        );
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn missing_or_wrong_kind_entries_fail_without_substitution() {
    let ids: Vec<&str> = fixture_entries().iter().map(|e| e.id).collect();
    assert_eq!(ids.len(), 423);
    // Every constituent of a species' record, via the composed reader.
    let wanted: Vec<&str> = vec![
        sheet_id(5),
        sheet_id(0),
        palette_id(0),
        palette_id(1),
        SPECIES_SHEETS_ID,
        PALETTE_INDICES_ID,
        PALETTE_TABLE_ID,
    ];
    for missing in &wanted {
        let entries = fixture_entries()
            .into_iter()
            .filter(|e| e.id != *missing)
            .collect();
        let (pack, path) = load("species-icon-missing", entries);
        let error = [0u16, 1, 3]
            .into_iter()
            .map(|s| pack.species_icon(SpeciesId(s)))
            .find_map(Result::err)
            .unwrap();
        assert!(
            matches!(&error, PackError::UnknownAsset(id) if id == *missing),
            "missing `{missing}`: {error:?}"
        );
        let _ = std::fs::remove_file(path);
    }
    for wrong in &wanted {
        let own_kind = fixture_entries()
            .iter()
            .find(|e| e.id == *wrong)
            .unwrap()
            .kind_tag;
        let others: [Entry; 2] = match own_kind {
            IMAGE_KIND_TAG => [palette(wrong, 16, 0), raw(wrong, vec![0; 8])],
            PALETTE_KIND_TAG => [raw(wrong, vec![0; 8]), image(wrong, (8, 8), 4, 0)],
            _ => [palette(wrong, 16, 0), image(wrong, (8, 8), 4, 0)],
        };
        for substitute in others {
            let (pack, path) = load("species-icon-wrong-kind", replaced(substitute));
            let error = [0u16, 1, 3]
                .into_iter()
                .map(|s| pack.species_icon(SpeciesId(s)))
                .find_map(Result::err)
                .unwrap();
            assert!(
                matches!(&error, PackError::WrongKind { id, .. } if id == *wrong),
                "wrong kind for `{wrong}`: {error:?}"
            );
            let _ = std::fs::remove_file(path);
        }
    }
}

#[test]
fn malformed_entries_are_rejected_on_read() {
    let sheet = sheet_id(5);
    let wide = sheet_id(SPEED_WIDE_SHEET);
    let bad_sheets = {
        let mut sheets = species_sheets();
        sheets[3] = 415;
        sheets
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect::<Vec<u8>>()
    };
    let mut bad_indices = palette_indices();
    bad_indices[3] = 3;
    let mut bad_table = encode_palette_table();
    bad_table[2] = 0;
    let cases = vec![
        (sheet, image(sheet, (32, 32), 4, 0)),
        (sheet, image(sheet, (64, 32), 4, 0)),
        (sheet, image(sheet, (32, 64), 8, 0)),
        (sheet, image(sheet, (128, 64), 4, 0)),
        (wide, image(wide, (32, 64), 4, 0)),
        (palette_id(0), palette(palette_id(0), 15, 0)),
        (palette_id(0), palette(palette_id(0), 48, 0)),
        (SPECIES_SHEETS_ID, raw(SPECIES_SHEETS_ID, vec![0; 879])),
        (SPECIES_SHEETS_ID, raw(SPECIES_SHEETS_ID, vec![0; 881])),
        (SPECIES_SHEETS_ID, raw(SPECIES_SHEETS_ID, bad_sheets)),
        (PALETTE_INDICES_ID, raw(PALETTE_INDICES_ID, vec![0; 439])),
        (PALETTE_INDICES_ID, raw(PALETTE_INDICES_ID, bad_indices)),
        (PALETTE_TABLE_ID, raw(PALETTE_TABLE_ID, vec![0; 23])),
        (PALETTE_TABLE_ID, raw(PALETTE_TABLE_ID, bad_table)),
    ];
    for (n, (id, entry)) in cases.into_iter().enumerate() {
        let (pack, path) = load(&format!("species-icon-malformed-{n}"), replaced(entry));
        let error = if id == wide {
            pack.species_icon_sheet(SpeciesIconSheetId::new(SPEED_WIDE_SHEET).unwrap())
                .map(drop)
                .unwrap_err()
        } else {
            (0..=4u16)
                .map(|s| pack.species_icon(SpeciesId(s)))
                .find_map(Result::err)
                .unwrap()
        };
        assert_malformed(&error, id, &format!("case {n}"));
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn a_pixel_outside_one_bank_is_rejected() {
    for pixel in [16, 31, 47, 255] {
        let id = sheet_id(0);
        let (pack, path) = load(
            "species-icon-pixel",
            replaced(image(id, (32, 64), TEXT_WINDOW_BIT_DEPTH, pixel)),
        );
        assert_malformed(
            &pack.species_icon(SpeciesId(0)).unwrap_err(),
            id,
            &format!("pixel {pixel}"),
        );
        let _ = std::fs::remove_file(path);
    }
    let (pack, path) = load("species-icon-pixel-15", fixture_entries());
    pack.species_icon(SpeciesId(0)).unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn individual_readers_need_only_their_own_records() {
    let entries: Vec<Entry> = fixture_entries()
        .into_iter()
        .filter(|e| e.id == PALETTE_TABLE_ID || e.id == palette_id(2))
        .collect();
    let (pack, path) = load("species-icon-partial", entries);
    assert_eq!(pack.species_icon_palette_records().unwrap().len(), 6);
    assert_eq!(pack.species_icon_palette(2).unwrap().color(0), Some(0x1002));
    assert!(matches!(
        pack.species_icon_palette(0).unwrap_err(),
        PackError::UnknownAsset(id) if id == palette_id(0)
    ));
    assert!(matches!(
        pack.species_icon_sheet_id(SpeciesId(0)).unwrap_err(),
        PackError::UnknownAsset(id) if id == SPECIES_SHEETS_ID
    ));
    let _ = std::fs::remove_file(path);
}
