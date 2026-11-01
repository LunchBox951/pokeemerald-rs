use super::super::{
    AssetPack, FieldMessage, FieldMessageError, FieldMessageTable, PackError, FIELD_MESSAGE_COUNT,
};
use super::shared::{
    pack_bytes, synthetic_pack, write_pack, Entry, PALETTE_KIND_TAG, RAW_KIND_TAG,
};

const TABLE_PACK_ID: &str = "field-message/table";

/// `gText_PkmnFainted_FldPsn` (`src/strings.c`): `{STR_VAR_1}`, space,
/// `fainted`, `…`, `\p`, `\n`, `EOS`, spelled through `charmap.txt`.
const FAINTED: [u8; 14] = [
    0xFD, 0x02, 0x00, 0xDA, 0xD5, 0xDD, 0xE2, 0xE8, 0xD9, 0xD8, 0xB0, 0xFB, 0xFE, 0xFF,
];

/// Hand-written so the tests do not rely on `encode` to check `encode`.
fn canonical(stream: &[u8]) -> Vec<u8> {
    let mut out = b"FMSG".to_vec();
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&u32::try_from(stream.len()).unwrap().to_le_bytes());
    out.extend_from_slice(stream);
    out
}

#[test]
fn inventory_is_the_one_poison_faint_symbol() {
    assert_eq!(FIELD_MESSAGE_COUNT, 1);
    assert_eq!(FieldMessage::ALL, [FieldMessage::PkmnFaintedFldPsn]);
    assert_eq!(FieldMessage::PkmnFaintedFldPsn.id(), 0);
    assert_eq!(
        FieldMessage::PkmnFaintedFldPsn.symbol(),
        "gText_PkmnFainted_FldPsn"
    );
}

#[test]
fn canonical_bytes_decode_and_reencode_identically() {
    let bytes = canonical(&FAINTED);
    assert_eq!(bytes.len(), 28);
    let table = FieldMessageTable::decode(&bytes).unwrap();
    assert_eq!(table.encode(), bytes);
    assert_eq!(table.string(FieldMessage::PkmnFaintedFldPsn), FAINTED);
}

#[test]
fn from_entries_encodes_canonically() {
    let table = FieldMessageTable::from_entries([(0, FAINTED.as_slice())]).unwrap();
    assert_eq!(table.encode(), canonical(&FAINTED));
}

#[test]
fn constructor_refuses_missing_duplicate_and_unknown_ids() {
    assert_eq!(
        FieldMessageTable::from_entries([]),
        Err(FieldMessageError::MissingId { id: 0 })
    );
    assert_eq!(
        FieldMessageTable::from_entries([(0, FAINTED.as_slice()), (0, FAINTED.as_slice())]),
        Err(FieldMessageError::DuplicateId { id: 0 })
    );
    assert_eq!(
        FieldMessageTable::from_entries([(1, FAINTED.as_slice())]),
        Err(FieldMessageError::IdOutOfRange { id: 1 })
    );
}

#[test]
fn every_truncated_prefix_is_refused() {
    let bytes = canonical(&FAINTED);
    for len in 0..bytes.len() {
        assert_eq!(
            FieldMessageTable::decode(&bytes[..len]),
            Err(FieldMessageError::Truncated),
            "prefix {len}"
        );
    }
}

#[test]
fn header_and_record_faults_are_refused() {
    let good = canonical(&FAINTED);
    let mut bad = good.clone();
    bad[0] = b'B';
    assert_eq!(
        FieldMessageTable::decode(&bad),
        Err(FieldMessageError::BadMagic)
    );
    let mut bad = good.clone();
    bad[4] = 2;
    assert_eq!(
        FieldMessageTable::decode(&bad),
        Err(FieldMessageError::UnsupportedVersion(2))
    );
    let mut bad = good.clone();
    bad[6] = 2;
    assert_eq!(
        FieldMessageTable::decode(&bad),
        Err(FieldMessageError::HeaderMismatch { found: 2 })
    );
    let mut bad = good.clone();
    bad[8] = 1;
    assert_eq!(
        FieldMessageTable::decode(&bad),
        Err(FieldMessageError::IdOutOfRange { id: 1 })
    );
    let mut bad = good.clone();
    bad[10..14].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(
        FieldMessageTable::decode(&bad),
        Err(FieldMessageError::Truncated)
    );
    let mut bad = good;
    bad.push(0);
    assert_eq!(
        FieldMessageTable::decode(&bad),
        Err(FieldMessageError::TrailingBytes)
    );
}

#[test]
fn token_streams_are_walked_by_token() {
    let ok: &[&[u8]] = &[
        &[0xFF],
        &FAINTED,
        &[0xFD, 0xFF, 0xFF],
        &[0xFC, 0x08, 0x20, 0xFF],
    ];
    for stream in ok {
        FieldMessageTable::decode(&canonical(stream))
            .unwrap_or_else(|e| panic!("{stream:02X?}: {e}"));
    }
    let bad: &[&[u8]] = &[
        &[],
        &[0xFD],
        &[0xFC],
        &[0xFC, 0x99, 0xFF],
        &[0xFF, 0xFF],
        &[0xFD, 0x02, 0x00],
        &[0xFD, 0xFF],
    ];
    for stream in bad {
        assert!(
            matches!(
                FieldMessageTable::decode(&canonical(stream)),
                Err(FieldMessageError::MalformedString { id: 0, .. })
            ),
            "{stream:02X?}"
        );
    }
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
    AssetPack::load(&path).unwrap()
}

#[test]
fn accessor_valid_missing_wrong_kind_and_malformed() {
    let pack = load("fmsg-ok", &pack_with(RAW_KIND_TAG, canonical(&FAINTED)));
    assert_eq!(
        pack.field_messages()
            .unwrap()
            .string(FieldMessage::PkmnFaintedFldPsn),
        FAINTED
    );

    let pack = load("fmsg-missing", &pack_bytes(vec![]));
    assert!(matches!(
        pack.field_messages(),
        Err(PackError::UnknownAsset(_))
    ));

    let pack = load("fmsg-kind", &pack_with(PALETTE_KIND_TAG, vec![0; 4]));
    assert!(matches!(
        pack.field_messages(),
        Err(PackError::WrongKind { .. })
    ));

    let mut bad = canonical(&FAINTED);
    bad.truncate(bad.len() - 1);
    let pack = load("fmsg-bad", &pack_with(RAW_KIND_TAG, bad));
    assert!(matches!(
        pack.field_messages(),
        Err(PackError::FieldMessageDecode {
            source: FieldMessageError::Truncated,
            ..
        })
    ));

    let pack = load(
        "fmsg-open",
        &pack_with(RAW_KIND_TAG, canonical(&FAINTED[..13])),
    );
    assert!(matches!(
        pack.field_messages(),
        Err(PackError::FieldMessageDecode {
            source: FieldMessageError::MalformedString { .. },
            ..
        })
    ));
}

#[test]
fn a_pack_from_the_format_without_field_messages_is_refused_on_load() {
    const PREVIOUS_FORMAT_VERSION: u32 = 11;
    let mut bytes = synthetic_pack();
    bytes[8..12].copy_from_slice(&PREVIOUS_FORMAT_VERSION.to_le_bytes());
    let path = write_pack("field-message-previous-version", &bytes);
    assert_eq!(
        AssetPack::load(&path).map(|_| ()),
        Err(PackError::UnsupportedVersion(PREVIOUS_FORMAT_VERSION))
    );
}
