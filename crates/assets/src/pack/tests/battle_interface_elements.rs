use super::super::{AssetPack, HealthboxElement, PackError};
use super::shared::{
    image_meta, pack_bytes, write_pack, Entry, IMAGE_KIND_TAG, RAW_KIND_TAG, TEXT_WINDOW_BIT_DEPTH,
};

const TILE_BYTES: usize = 32;
/// `gHealthboxElementsGfxTable` constituents with their tile counts, in the
/// order `graphics.c:358-370` lists them.
const UPSTREAM_TABLE: [(&str, usize); 13] = [
    ("battle-interface/raw/healthbox_element/hpbar", 12),
    ("battle-interface/raw/healthbox_element/expbar", 9),
    ("battle-interface/raw/healthbox_element/status", 15),
    ("battle-interface/raw/healthbox_element/misc", 11),
    ("battle-interface/raw/healthbox_element/hpbar_anim", 18),
    ("battle-interface/raw/healthbox_element/misc_frameend", 1),
    ("battle-interface/raw/healthbox_element/ball_display", 4),
    (
        "battle-interface/raw/healthbox_element/ball_caught_indicator",
        1,
    ),
    ("battle-interface/raw/healthbox_element/status2", 15),
    ("battle-interface/raw/healthbox_element/status3", 15),
    ("battle-interface/raw/healthbox_element/status4", 15),
    (
        "battle-interface/raw/healthbox_element/healthbox_doubles_frameend",
        1,
    ),
    (
        "battle-interface/raw/healthbox_element/healthbox_doubles_frameend_bar",
        1,
    ),
];
const TABLE_TILES: usize = 118;
const BATTLE_BAR_ID: &str = "battle-interface/raw/battle_bar";
const BATTLE_BAR_BYTES: usize = (18 + 11 + 12) * TILE_BYTES;

fn raw(id: &'static str, len: usize, fill: u8) -> Entry {
    Entry {
        id,
        kind_tag: RAW_KIND_TAG,
        meta: vec![],
        payload: vec![fill; len],
    }
}

/// Sheet `n` of the table is filled with byte `n + 1`, so order shows in the bytes.
fn fixture_entries() -> Vec<Entry> {
    let mut entries: Vec<Entry> = UPSTREAM_TABLE
        .iter()
        .zip(1..)
        .map(|(&(id, tiles), fill)| raw(id, tiles * TILE_BYTES, fill))
        .collect();
    entries.push(raw(BATTLE_BAR_ID, BATTLE_BAR_BYTES, 0xB0));
    entries
}

fn load(name: &str, entries: Vec<Entry>) -> (AssetPack, std::path::PathBuf) {
    let path = write_pack(name, &pack_bytes(entries));
    (AssetPack::load(&path).unwrap(), path)
}

fn replaced(entry: Entry) -> Vec<Entry> {
    let mut entries: Vec<Entry> = fixture_entries()
        .into_iter()
        .filter(|e| e.id != entry.id)
        .collect();
    entries.push(entry);
    entries
}

#[test]
fn healthbox_elements_keep_the_upstream_table_order() {
    let (pack, path) = load("healthbox-elements", fixture_entries());
    let handle = pack.healthbox_elements().unwrap();

    assert!(HealthboxElement::TABLE
        .iter()
        .enumerate()
        .all(|(ordinal, &element)| element as usize == ordinal));
    let sheets: Vec<_> = handle.sheets().collect();
    assert_eq!(sheets.len(), UPSTREAM_TABLE.len());
    for (ordinal, ((element, bytes), &(_, tiles))) in sheets.iter().zip(&UPSTREAM_TABLE).enumerate()
    {
        let fill = u8::try_from(ordinal + 1).unwrap();
        assert_eq!(*element, HealthboxElement::TABLE[ordinal]);
        assert_eq!(*bytes, vec![fill; tiles * TILE_BYTES].as_slice());
        assert_eq!(element.tile_count(), tiles);
        assert_eq!(handle.sheet(*element), *bytes);
    }
    assert_eq!(
        handle.sheet(HealthboxElement::HealthboxDoublesFrameEndBar),
        vec![13; TILE_BYTES].as_slice()
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn table_tiles_are_indexed_across_constituents() {
    let (pack, path) = load("healthbox-elements-tiles", fixture_entries());
    let handle = pack.healthbox_elements().unwrap();

    let first_tile_of_each_sheet = [0, 12, 21, 36, 47, 65, 66, 70, 71, 86, 101, 116, 117];
    for (ordinal, first) in first_tile_of_each_sheet.into_iter().enumerate() {
        let fill = u8::try_from(ordinal + 1).unwrap();
        assert_eq!(handle.tile(first), Some(vec![fill; TILE_BYTES].as_slice()));
        if first > 0 {
            assert_eq!(
                handle.tile(first - 1),
                Some(vec![fill - 1; TILE_BYTES].as_slice())
            );
        }
    }
    assert_eq!(
        handle.tile(TABLE_TILES - 1).map(<[u8]>::len),
        Some(TILE_BYTES)
    );
    assert_eq!(handle.tile(TABLE_TILES), None);

    let _ = std::fs::remove_file(path);
}

#[test]
fn battle_bar_is_the_raw_assembled_blob() {
    let mut entries = fixture_entries();
    entries.retain(|e| e.id != BATTLE_BAR_ID);
    let blob: Vec<u8> = (0..=u8::MAX).cycle().take(BATTLE_BAR_BYTES).collect();
    entries.push(Entry {
        payload: blob.clone(),
        ..raw(BATTLE_BAR_ID, 0, 0)
    });
    let (pack, path) = load("battle-bar", entries);

    assert_eq!(pack.battle_bar().unwrap().tiles, blob.as_slice());

    let _ = std::fs::remove_file(path);
}

#[test]
fn missing_or_wrong_kind_entries_fail_without_substitution() {
    let battle_bar_missing = pack_bytes(
        fixture_entries()
            .into_iter()
            .filter(|e| e.id != BATTLE_BAR_ID)
            .collect(),
    );
    let path = write_pack("battle-bar-missing", &battle_bar_missing);
    assert!(matches!(
        AssetPack::load(&path).unwrap().battle_bar().unwrap_err(),
        PackError::UnknownAsset(id) if id == BATTLE_BAR_ID
    ));
    let _ = std::fs::remove_file(path);

    for (missing, _) in UPSTREAM_TABLE {
        let entries: Vec<Entry> = fixture_entries()
            .into_iter()
            .filter(|e| e.id != missing)
            .collect();
        let (pack, path) = load("healthbox-elements-missing", entries);
        assert!(matches!(
            pack.healthbox_elements().unwrap_err(),
            PackError::UnknownAsset(id) if id == missing
        ));
        let _ = std::fs::remove_file(path);
    }

    for id in [UPSTREAM_TABLE[4].0, BATTLE_BAR_ID] {
        let wrong_kind = Entry {
            id,
            kind_tag: IMAGE_KIND_TAG,
            meta: image_meta(8, 8, TEXT_WINDOW_BIT_DEPTH),
            payload: vec![0; 64],
        };
        let (pack, path) = load("healthbox-elements-wrong-kind", replaced(wrong_kind));
        let error = if id == BATTLE_BAR_ID {
            pack.battle_bar().unwrap_err()
        } else {
            pack.healthbox_elements().unwrap_err()
        };
        assert!(
            matches!(error, PackError::WrongKind { id: bad, expected: "raw blob", .. } if bad == id)
        );
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn sheets_of_the_wrong_length_are_rejected() {
    for &(id, tiles) in &UPSTREAM_TABLE {
        let right = tiles * TILE_BYTES;
        for len in [0, right - 1, right + 1, right + TILE_BYTES] {
            let (pack, path) = load("healthbox-elements-malformed", replaced(raw(id, len, 0)));
            assert!(
                matches!(
                    pack.healthbox_elements().unwrap_err(),
                    PackError::MalformedBattleInterfaceAsset { id: bad, .. } if bad == id
                ),
                "`{id}` at {len} bytes"
            );
            let _ = std::fs::remove_file(path);
        }
    }

    for len in [0, BATTLE_BAR_BYTES - 1, BATTLE_BAR_BYTES + 1] {
        let (pack, path) = load("battle-bar-malformed", replaced(raw(BATTLE_BAR_ID, len, 0)));
        assert!(
            matches!(
                pack.battle_bar().unwrap_err(),
                PackError::MalformedBattleInterfaceAsset { id, .. } if id == BATTLE_BAR_ID
            ),
            "{len} bytes"
        );
        let _ = std::fs::remove_file(path);
    }
}
