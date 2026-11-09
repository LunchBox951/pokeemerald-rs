use super::super::{AssetPack, PackError, TrainerClassNameError};
use super::shared::{
    pack_bytes, write_pack, write_synthetic_pack, Entry, PALETTE_KIND_TAG, RAW_KIND_TAG,
};
use crate::{
    TrainerClass, TrainerClassNameTable, TRAINER_CLASS_NAME_COUNT, TRAINER_CLASS_NAME_ROW_BYTES,
};

const TABLE_PACK_ID: &str = "trainer-class-name/table";

/// The rival's name: `{PKMN} TRAINER` (`53 54` are the `PK` and `MN` glyphs).
const RIVAL: [u8; 11] = [
    0x53, 0x54, 0x00, 0xCE, 0xCC, 0xBB, 0xC3, 0xC8, 0xBF, 0xCC, 0xFF,
];

/// A distinct stream per ID, so a lookup that returns the wrong record fails.
fn stream(id: u16) -> Vec<u8> {
    vec![0xA1 + u8::try_from(id % 60).unwrap(), 0x00, 0xFF]
}

fn streams() -> Vec<Vec<u8>> {
    (0..66).map(stream).collect()
}

/// Hand-written so the tests do not rely on `encode` to check `encode`.
fn canonical(streams: &[Vec<u8>]) -> Vec<u8> {
    let mut out = b"TCLN".to_vec();
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&66u16.to_le_bytes());
    for (id, stream) in streams.iter().enumerate() {
        out.extend_from_slice(&u16::try_from(id).unwrap().to_le_bytes());
        out.extend_from_slice(&u32::try_from(stream.len()).unwrap().to_le_bytes());
        out.extend_from_slice(stream);
    }
    out
}

fn entries(streams: &[Vec<u8>]) -> Vec<(u16, &[u8])> {
    streams
        .iter()
        .enumerate()
        .map(|(id, s)| (u16::try_from(id).unwrap(), s.as_slice()))
        .collect()
}

/// Fixed-width rows as upstream lays them out: stream, then zero padding.
fn fixed_rows(streams: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    for stream in streams {
        out.extend_from_slice(stream);
        out.resize(out.len() + 13 - stream.len(), 0);
    }
    out
}

#[test]
fn the_inventory_is_one_row_of_thirteen_bytes_per_class() {
    assert_eq!(TRAINER_CLASS_NAME_COUNT, 66);
    assert_eq!(usize::from(TrainerClass::COUNT), TRAINER_CLASS_NAME_COUNT);
    assert_eq!(TRAINER_CLASS_NAME_ROW_BYTES, 13);
    assert_eq!(TrainerClass::all().count(), 66);
    assert_eq!(TrainerClass::all().last(), Some(TrainerClass::RS_PROTAG));
}

#[test]
fn canonical_bytes_decode_and_reencode_identically() {
    let streams = streams();
    let bytes = canonical(&streams);
    assert_eq!(bytes.len(), 8 + 66 * (2 + 4 + 3));
    let table = TrainerClassNameTable::decode(&bytes).unwrap();
    assert_eq!(table.encode(), bytes);
    for class in TrainerClass::all() {
        assert_eq!(table.name(class).unwrap(), streams[usize::from(class.0)]);
    }
}

#[test]
fn lookup_by_class_refuses_an_identity_outside_the_table() {
    let bytes = canonical(&streams());
    let table = TrainerClassNameTable::decode(&bytes).unwrap();
    assert_eq!(table.name(TrainerClass::RS_PROTAG), Some(&stream(65)[..]));
    assert_eq!(table.name(TrainerClass(66)), None);
    assert_eq!(table.name(TrainerClass(255)), None);
}

#[test]
fn from_entries_encodes_canonically_in_any_order() {
    let streams = streams();
    let mut items = entries(&streams);
    items.reverse();
    let table = TrainerClassNameTable::from_entries(items).unwrap();
    assert_eq!(table.encode(), canonical(&streams));
}

#[test]
fn equal_names_at_different_indices_stay_separate_records() {
    let mut streams = streams();
    for index in [0, 1, 50, 65] {
        streams[index] = RIVAL.to_vec();
    }
    let bytes = canonical(&streams);
    let table = TrainerClassNameTable::decode(&bytes).unwrap();
    for class in [
        TrainerClass::PKMN_TRAINER_1,
        TrainerClass::PKMN_TRAINER_2,
        TrainerClass::RIVAL,
        TrainerClass::RS_PROTAG,
    ] {
        assert_eq!(table.name(class), Some(&RIVAL[..]));
    }
    assert_eq!(table.encode(), bytes);
}

#[test]
fn constructor_refuses_missing_duplicate_and_unknown_ids() {
    let streams = streams();
    let mut items = entries(&streams);
    let dropped = items.remove(5);
    assert_eq!(
        TrainerClassNameTable::from_entries(items.clone()),
        Err(TrainerClassNameError::MissingId { id: 5 })
    );
    items.push((3, streams[3].as_slice()));
    assert_eq!(
        TrainerClassNameTable::from_entries(items.clone()),
        Err(TrainerClassNameError::DuplicateId { id: 3 })
    );
    items.pop();
    items.push((66, dropped.1));
    assert_eq!(
        TrainerClassNameTable::from_entries(items),
        Err(TrainerClassNameError::IdOutOfRange { id: 66 })
    );
}

#[test]
fn every_truncated_prefix_is_refused() {
    let bytes = canonical(&streams());
    for len in 0..bytes.len() {
        assert_eq!(
            TrainerClassNameTable::decode(&bytes[..len]),
            Err(TrainerClassNameError::Truncated),
            "prefix {len}"
        );
    }
}

#[test]
fn header_and_record_faults_are_refused() {
    let good = canonical(&streams());
    let mut bad = good.clone();
    bad[0] = b'B';
    assert_eq!(
        TrainerClassNameTable::decode(&bad),
        Err(TrainerClassNameError::BadMagic)
    );
    let mut bad = good.clone();
    bad[4] = 2;
    assert_eq!(
        TrainerClassNameTable::decode(&bad),
        Err(TrainerClassNameError::UnsupportedVersion(2))
    );
    let mut bad = good.clone();
    bad[6] = 65;
    assert_eq!(
        TrainerClassNameTable::decode(&bad),
        Err(TrainerClassNameError::HeaderMismatch { found: 65 })
    );
    let mut bad = good.clone();
    bad[8] = 66;
    assert_eq!(
        TrainerClassNameTable::decode(&bad),
        Err(TrainerClassNameError::IdOutOfRange { id: 66 })
    );
    let mut bad = good.clone();
    bad[8] = 1;
    assert_eq!(
        TrainerClassNameTable::decode(&bad),
        Err(TrainerClassNameError::DuplicateId { id: 1 })
    );
    let mut bad = good.clone();
    bad[10..14].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(
        TrainerClassNameTable::decode(&bad),
        Err(TrainerClassNameError::Truncated)
    );
    let mut bad = good;
    bad.push(0);
    assert_eq!(
        TrainerClassNameTable::decode(&bad),
        Err(TrainerClassNameError::TrailingBytes)
    );
}

#[test]
fn token_streams_are_walked_by_token_and_bounded_by_the_row() {
    let with = |stream: &[u8]| {
        let mut streams = streams();
        streams[50] = stream.to_vec();
        canonical(&streams)
    };
    let ok: &[&[u8]] = &[
        &[0xFF],
        &RIVAL,
        // Thirteen bytes, `EOS` included, exactly fill the row.
        &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 0xFF],
        &[0xFC, 0x08, 0x20, 0xFF],
    ];
    for stream in ok {
        TrainerClassNameTable::decode(&with(stream))
            .unwrap_or_else(|e| panic!("{stream:02X?}: {e}"));
    }
    let bad: &[&[u8]] = &[
        &[],
        &[0xFD],
        &[0xFC, 0x99, 0xFF],
        &[0xFF, 0xFF],
        &[0xFD, 0xFF],
        &[0x01],
    ];
    for stream in bad {
        assert!(
            matches!(
                TrainerClassNameTable::decode(&with(stream)),
                Err(TrainerClassNameError::MalformedString { id: 50, .. })
            ),
            "{stream:02X?}"
        );
    }
    let over = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 0xFF];
    assert_eq!(
        TrainerClassNameTable::decode(&with(&over)),
        Err(TrainerClassNameError::NameTooLong { id: 50, len: 14 })
    );
}

#[test]
fn fixed_rows_split_each_name_from_its_zero_padding() {
    let mut streams = streams();
    streams[50] = RIVAL.to_vec();
    // A zero byte inside a name is a space, not padding.
    streams[2] = vec![0xBB, 0x00, 0xBB, 0xFF];
    streams[44] = vec![0xBB; 12].into_iter().chain([0xFF]).collect();
    let rows = fixed_rows(&streams);
    assert_eq!(rows.len(), 858);
    let table = TrainerClassNameTable::from_fixed_rows(&rows).unwrap();
    assert_eq!(table.name(TrainerClass::RIVAL), Some(&RIVAL[..]));
    assert_eq!(table.name(TrainerClass::HIKER), Some(&streams[2][..]));
    assert_eq!(table.encode(), canonical(&streams));
    assert_eq!(table.to_fixed_rows(), rows);
}

#[test]
fn fixed_rows_fail_closed() {
    let rows = fixed_rows(&streams());
    assert_eq!(
        TrainerClassNameTable::from_fixed_rows(&rows[..857]),
        Err(TrainerClassNameError::BadRowTable { found: 857 })
    );
    let mut longer = rows.clone();
    longer.push(0);
    assert_eq!(
        TrainerClassNameTable::from_fixed_rows(&longer),
        Err(TrainerClassNameError::BadRowTable { found: 859 })
    );
    // Nonzero padding after a name.
    let mut bad = rows.clone();
    bad[13 * 7 + 12] = 1;
    assert_eq!(
        TrainerClassNameTable::from_fixed_rows(&bad),
        Err(TrainerClassNameError::NonzeroPadding { id: 7 })
    );
    // A row whose terminator is missing must not borrow the next row's.
    let mut bad = rows.clone();
    for byte in &mut bad[13 * 9..13 * 10] {
        *byte = 0x01;
    }
    assert!(matches!(
        TrainerClassNameTable::from_fixed_rows(&bad),
        Err(TrainerClassNameError::MalformedString { id: 9, .. })
    ));
    // A control sequence cannot swallow its row's terminator.
    let mut bad = rows;
    bad[13 * 9..13 * 9 + 3].copy_from_slice(&[0xBB, 0xFD, 0xFF]);
    bad[13 * 9 + 3..13 * 10].fill(0);
    assert!(matches!(
        TrainerClassNameTable::from_fixed_rows(&bad),
        Err(TrainerClassNameError::MalformedString { id: 9, .. })
    ));
}

fn pack_with(kind_tag: u8, payload: Vec<u8>) -> Vec<u8> {
    pack_bytes(vec![Entry {
        id: TABLE_PACK_ID,
        kind_tag,
        meta: if kind_tag == PALETTE_KIND_TAG {
            2u16.to_le_bytes().to_vec()
        } else {
            vec![]
        },
        payload,
    }])
}

fn load(name: &str, bytes: &[u8]) -> AssetPack {
    let path = write_pack(name, bytes);
    let pack = AssetPack::load(&path).unwrap();
    let _ = std::fs::remove_file(path);
    pack
}

#[test]
fn accessor_valid_missing_wrong_kind_and_malformed() {
    let pack = load("tcln-ok", &pack_with(RAW_KIND_TAG, canonical(&streams())));
    assert_eq!(
        pack.trainer_class_names()
            .unwrap()
            .name(TrainerClass::RS_PROTAG),
        Some(&stream(65)[..])
    );

    let pack = load("tcln-missing", &pack_bytes(vec![]));
    assert!(matches!(
        pack.trainer_class_names(),
        Err(PackError::UnknownAsset(_))
    ));

    let pack = load("tcln-kind", &pack_with(PALETTE_KIND_TAG, vec![0; 4]));
    assert!(matches!(
        pack.trainer_class_names(),
        Err(PackError::WrongKind { .. })
    ));

    let mut bad = canonical(&streams());
    bad.truncate(bad.len() - 1);
    let pack = load("tcln-bad", &pack_with(RAW_KIND_TAG, bad));
    assert!(matches!(
        pack.trainer_class_names(),
        Err(PackError::TrainerClassNameDecode {
            source: TrainerClassNameError::Truncated,
            ..
        })
    ));

    let mut open = streams();
    open[0] = vec![0xA1];
    let pack = load("tcln-open", &pack_with(RAW_KIND_TAG, canonical(&open)));
    assert!(matches!(
        pack.trainer_class_names(),
        Err(PackError::TrainerClassNameDecode {
            source: TrainerClassNameError::MalformedString { id: 0, .. },
            ..
        })
    ));
}

/// A pack stamped with the version before the class table landed (#3229's
/// extractor writes version 14 without `trainer-class-name/table`) is refused
/// at load as stale, not accepted and failed at lookup.
#[test]
fn a_pack_from_before_the_class_table_is_refused_on_load() {
    const PREVIOUS_FORMAT_VERSION: u32 = 14;
    assert_ne!(pack_format::FORMAT_VERSION, PREVIOUS_FORMAT_VERSION);
    let path = write_synthetic_pack("trainer-class-name-previous-version");
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[8..12].copy_from_slice(&PREVIOUS_FORMAT_VERSION.to_le_bytes());
    std::fs::write(&path, bytes).unwrap();
    assert_eq!(
        AssetPack::load(&path).map(|_| ()),
        Err(PackError::UnsupportedVersion(PREVIOUS_FORMAT_VERSION))
    );
    let _ = std::fs::remove_file(path);
}
