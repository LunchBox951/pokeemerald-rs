use super::super::{
    AssetPack, DoorAnimationRecord, DoorAnimationTable, DoorSize, DoorSound, PackError,
};
use super::shared::{
    image_meta, pack_bytes, write_pack, Entry, IMAGE_KIND_TAG, INDEXED_8_BIT, RAW_KIND_TAG,
    TEXT_WINDOW_BIT_DEPTH,
};

const TABLE: &str = "door-animation/table";
const LITTLEROOT: &str = "door-animation/image/littleroot";
const BIRCHS_LAB: &str = "door-animation/image/birchs_lab";
const MULTI_CORRIDOR: &str = "door-animation/image/battle_tower_multi_corridor";

const LITTLEROOT_DOOR: u16 = 0x248;
const BIRCHS_LAB_DOOR: u16 = 0x249;
const LARGE_DOOR: u16 = 0x2AD;

fn record(
    metatile: u16,
    sound: DoorSound,
    size: DoorSize,
    palette_slots: [u8; 8],
    sheet: &str,
) -> DoorAnimationRecord {
    DoorAnimationRecord {
        metatile,
        sound,
        size,
        palette_slots,
        sheet: sheet.to_owned(),
    }
}

fn records() -> Vec<DoorAnimationRecord> {
    vec![
        record(
            LITTLEROOT_DOOR,
            DoorSound::Normal,
            DoorSize::OneTile,
            [10, 10, 6, 6, 6, 6, 6, 6],
            "littleroot",
        ),
        record(
            BIRCHS_LAB_DOOR,
            DoorSound::Normal,
            DoorSize::OneTile,
            [8; 8],
            "birchs_lab",
        ),
        record(
            LARGE_DOOR,
            DoorSound::Sliding,
            DoorSize::Large,
            [7; 8],
            "battle_tower_multi_corridor",
        ),
    ]
}

fn table_bytes(records: Vec<DoorAnimationRecord>) -> Vec<u8> {
    DoorAnimationTable::new(records).unwrap().encode()
}

fn image(id: &'static str, size: (u32, u32), depth: u8, pixel: u8) -> Entry {
    Entry {
        id,
        kind_tag: IMAGE_KIND_TAG,
        meta: image_meta(size.0, size.1, depth),
        payload: vec![pixel; (size.0 * size.1) as usize],
    }
}

fn table_entry(payload: Vec<u8>) -> Entry {
    Entry {
        id: TABLE,
        kind_tag: RAW_KIND_TAG,
        meta: vec![],
        payload,
    }
}

fn entries() -> Vec<Entry> {
    vec![
        table_entry(table_bytes(records())),
        image(LITTLEROOT, (16, 96), TEXT_WINDOW_BIT_DEPTH, 3),
        image(BIRCHS_LAB, (16, 96), TEXT_WINDOW_BIT_DEPTH, 4),
        image(MULTI_CORRIDOR, (32, 96), TEXT_WINDOW_BIT_DEPTH, 5),
    ]
}

fn replaced(id: &str, entry: Entry) -> Vec<Entry> {
    let mut entries = without(id);
    entries.push(entry);
    entries
}

fn without(id: &str) -> Vec<Entry> {
    entries().into_iter().filter(|e| e.id != id).collect()
}

fn load(name: &str, entries: Vec<Entry>) -> (AssetPack, std::path::PathBuf) {
    let path = write_pack(name, &pack_bytes(entries));
    (AssetPack::load(&path).unwrap(), path)
}

#[test]
fn each_door_metatile_resolves_to_its_sheet_size_sound_and_banks() {
    let (pack, path) = load("door-animation-all", entries());
    let cases = [
        (
            LITTLEROOT_DOOR,
            (16, 96),
            3,
            DoorSize::OneTile,
            [10, 10, 6, 6, 6, 6, 6, 6],
        ),
        (BIRCHS_LAB_DOOR, (16, 96), 4, DoorSize::OneTile, [8; 8]),
        (LARGE_DOOR, (32, 96), 5, DoorSize::Large, [7; 8]),
    ];
    for (metatile, dimensions, pixel, size, banks) in cases {
        let door = pack.door_animation(metatile).unwrap().unwrap();
        assert_eq!((door.sheet.width, door.sheet.height), dimensions);
        assert_eq!(door.sheet.bit_depth, 4);
        assert!(door.sheet.pixels.iter().all(|&p| p == pixel));
        assert_eq!(door.size, size, "{metatile:#x}");
        assert_eq!(door.palette_slots, banks, "{metatile:#x}");
    }
    let large = pack.door_animation(LARGE_DOOR).unwrap().unwrap();
    assert_eq!(large.sound, DoorSound::Sliding);
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_metatile_the_table_does_not_list_is_a_typed_miss() {
    let (pack, path) = load("door-animation-miss", entries());
    for metatile in [0, 0x247, 0x24A, u16::MAX] {
        assert!(pack.door_animation(metatile).unwrap().is_none());
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_missing_table_or_sheet_is_named() {
    for id in [TABLE, LITTLEROOT] {
        let (pack, path) = load("door-animation-missing", without(id));
        assert!(
            matches!(
                pack.door_animation(LITTLEROOT_DOOR).unwrap_err(),
                PackError::UnknownAsset(missing) if missing == id
            ),
            "{id}"
        );
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn a_wrong_kind_table_or_sheet_is_named() {
    let raw = |id| Entry {
        id,
        kind_tag: RAW_KIND_TAG,
        meta: vec![],
        payload: vec![0; 4],
    };
    let mut wrong_table = without(TABLE);
    wrong_table.push(image(TABLE, (16, 96), TEXT_WINDOW_BIT_DEPTH, 0));
    let cases = [
        (TABLE, wrong_table),
        (LITTLEROOT, replaced(LITTLEROOT, raw(LITTLEROOT))),
    ];
    for (id, entries) in cases {
        let (pack, path) = load("door-animation-wrong-kind", entries);
        assert!(
            matches!(
                pack.door_animation(LITTLEROOT_DOOR).unwrap_err(),
                PackError::WrongKind { id: wrong, .. } if wrong == id
            ),
            "{id}"
        );
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn a_malformed_sheet_is_rejected_on_read() {
    let cases = [
        // The table says one-tile: sixteen pixels wide.
        (LITTLEROOT, (32, 96), TEXT_WINDOW_BIT_DEPTH, 1),
        (LITTLEROOT, (16, 100), TEXT_WINDOW_BIT_DEPTH, 1),
        (LITTLEROOT, (16, 96), INDEXED_8_BIT, 1),
        (LITTLEROOT, (16, 96), TEXT_WINDOW_BIT_DEPTH, 16),
    ];
    for (index, (id, size, depth, pixel)) in cases.into_iter().enumerate() {
        let entry = image(LITTLEROOT, size, depth, pixel);
        let (pack, path) = load(&format!("door-animation-bad-{index}"), replaced(id, entry));
        assert!(
            matches!(
                pack.door_animation(LITTLEROOT_DOOR).unwrap_err(),
                PackError::MalformedDoorAnimationAsset { id: bad, .. } if bad == id
            ),
            "case {index}"
        );
        let _ = std::fs::remove_file(path);
    }
    // A large door's sheet is thirty-two pixels wide.
    let narrow = image(MULTI_CORRIDOR, (16, 96), TEXT_WINDOW_BIT_DEPTH, 1);
    let (pack, path) = load("door-animation-narrow", replaced(MULTI_CORRIDOR, narrow));
    assert!(matches!(
        pack.door_animation(LARGE_DOOR).unwrap_err(),
        PackError::MalformedDoorAnimationAsset { .. }
    ));
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_malformed_table_is_rejected_on_read() {
    let valid = table_bytes(records());
    let mut trailing = valid.clone();
    trailing.push(0);
    let cases = [
        valid[..valid.len() - 1].to_vec(),
        trailing,
        vec![1],
        Vec::new(),
    ];
    for (index, bytes) in cases.into_iter().enumerate() {
        let (pack, path) = load(
            &format!("door-animation-bad-table-{index}"),
            replaced(TABLE, table_entry(bytes)),
        );
        assert!(
            matches!(
                pack.door_animation(LITTLEROOT_DOOR).unwrap_err(),
                PackError::MalformedDoorAnimationAsset { id, .. } if id == TABLE
            ),
            "case {index}"
        );
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn the_table_codec_round_trips_and_refuses_what_the_schema_forbids() {
    let table = DoorAnimationTable::new(records()).unwrap();
    assert_eq!(DoorAnimationTable::decode(&table.encode()).unwrap(), table);
    assert_eq!(table.records(), records());
    assert_eq!(table.get(BIRCHS_LAB_DOOR).unwrap().sheet, "birchs_lab");
    assert!(table.get(0).is_none());

    let one = |edit: fn(&mut DoorAnimationRecord)| {
        let mut record = records().remove(0);
        edit(&mut record);
        DoorAnimationTable::new(vec![record])
    };
    assert!(one(|r| r.sheet.clear()).is_err());
    assert!(one(|r| r.sheet = "Upper".to_owned()).is_err());
    assert!(one(|r| r.sheet = "../escape".to_owned()).is_err());
    assert!(one(|r| r.sheet = "a".repeat(256)).is_err());
    assert!(one(|r| r.palette_slots[7] = 16).is_err());
    let mut twice = records();
    twice.push(twice[0].clone());
    assert!(DoorAnimationTable::new(twice).is_err());

    // The first record's sound and size tags sit at bytes 4 and 5.
    let mut bytes = table.encode();
    bytes[4] = 3;
    assert!(DoorAnimationTable::decode(&bytes).is_err());
    let mut bytes = table.encode();
    bytes[5] = 0;
    assert!(DoorAnimationTable::decode(&bytes).is_err());
}
