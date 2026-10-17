use super::super::{
    AssetPack, BattleMessageError, BattleMessageSection, BattleMessageTable, PackError,
    SpecialBattleMessage, BATTLE_MESSAGE_SLOT_COUNT, BATTLE_MESSAGE_SPECIAL_COUNT,
    BATTLE_MESSAGE_SPECIAL_TEMPLATES,
};
use super::shared::{pack_bytes, write_pack, Entry, PALETTE_KIND_TAG, RAW_KIND_TAG};

const TABLE_PACK_ID: &str = "battle-message/table";

fn slot_stream(id: u16) -> Vec<u8> {
    vec![0x10 + u8::try_from(id % 0x40).unwrap(), 0xFF]
}

fn special_stream(index: u16) -> Vec<u8> {
    vec![0x50, u8::try_from(index).unwrap(), 0xFF]
}

fn slots() -> Vec<(u16, Vec<u8>)> {
    (12..381).map(|id| (id, slot_stream(id))).collect()
}

fn specials() -> Vec<(u16, Vec<u8>)> {
    (0..74).map(|i| (i, special_stream(i))).collect()
}

/// Hand-written so the tests do not rely on `encode` to check `encode`.
fn canonical(slots: &[(u16, Vec<u8>)], specials: &[(u16, Vec<u8>)]) -> Vec<u8> {
    let mut out = b"BMSG".to_vec();
    for v in [1u16, 12, 381, 369, 74] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for (id, s) in slots.iter().chain(specials) {
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(&u32::try_from(s.len()).unwrap().to_le_bytes());
        out.extend_from_slice(s);
    }
    out
}

fn canonical_payload() -> Vec<u8> {
    canonical(&slots(), &specials())
}

fn borrowed(v: &[(u16, Vec<u8>)]) -> Vec<(u16, &[u8])> {
    v.iter().map(|(id, s)| (*id, s.as_slice())).collect()
}

#[test]
fn canonical_bytes_decode_and_reencode_identically() {
    let bytes = canonical_payload();
    let table = BattleMessageTable::decode(&bytes).unwrap();
    assert_eq!(table.encode(), bytes);
    assert_eq!(table.strings().count(), BATTLE_MESSAGE_SLOT_COUNT);
    assert_eq!(table.string(12).unwrap(), slot_stream(12));
    assert_eq!(table.string(380).unwrap(), slot_stream(380));
    assert_eq!(table.special(73).unwrap(), special_stream(73));
}

#[test]
fn from_entries_in_any_order_encodes_canonically() {
    let (s, p) = (slots(), specials());
    let (mut sb, mut pb) = (borrowed(&s), borrowed(&p));
    sb.reverse();
    pb.reverse();
    let table = BattleMessageTable::from_entries(sb, pb).unwrap();
    assert_eq!(table.encode(), canonical_payload());
}

#[test]
fn lookup_bounds() {
    let bytes = canonical_payload();
    let table = BattleMessageTable::decode(&bytes).unwrap();
    for id in [0, 11, 381, u16::MAX] {
        assert_eq!(
            table.string(id),
            Err(BattleMessageError::IdOutOfRange {
                section: BattleMessageSection::Table,
                id
            })
        );
    }
    assert!(table.special(74).is_none());
    assert!(table.special_by_symbol("sText_Nope").is_none());
}

#[test]
fn every_truncation_is_refused() {
    let bytes = canonical_payload();
    for len in 0..bytes.len() {
        assert!(BattleMessageTable::decode(&bytes[..len]).is_err(), "{len}");
    }
}

#[test]
fn header_mutations_are_refused() {
    let mut bytes = canonical_payload();
    bytes[0] = b'X';
    assert_eq!(
        BattleMessageTable::decode(&bytes),
        Err(BattleMessageError::BadMagic)
    );
    for (offset, field) in [
        (6, "start"),
        (8, "end"),
        (10, "slot_count"),
        (12, "special_count"),
    ] {
        let mut bytes = canonical_payload();
        bytes[offset] ^= 1;
        assert!(
            matches!(
                BattleMessageTable::decode(&bytes),
                Err(BattleMessageError::HeaderMismatch { field: f, .. }) if f == field
            ),
            "{field}"
        );
    }
    let mut bytes = canonical_payload();
    bytes[4] = 2;
    assert_eq!(
        BattleMessageTable::decode(&bytes),
        Err(BattleMessageError::UnsupportedVersion(2))
    );
}

#[test]
fn trailing_bytes_and_oversized_length_are_refused() {
    let mut bytes = canonical_payload();
    bytes.push(0);
    assert_eq!(
        BattleMessageTable::decode(&bytes),
        Err(BattleMessageError::TrailingBytes)
    );
    let mut bytes = canonical_payload();
    bytes[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(
        BattleMessageTable::decode(&bytes),
        Err(BattleMessageError::Truncated)
    );
}

#[test]
fn sparse_duplicate_unordered_and_out_of_range_records_are_refused() {
    assert_eq!(BATTLE_MESSAGE_SPECIAL_COUNT, 74);

    let mut s = slots();
    s[5].0 = s[4].0; // 17 becomes a second 16
    assert_eq!(
        BattleMessageTable::decode(&canonical(&s, &specials())),
        Err(BattleMessageError::DuplicateId {
            section: BattleMessageSection::Table,
            id: 16
        })
    );

    let mut s = slots();
    s.swap(3, 4);
    assert!(matches!(
        BattleMessageTable::decode(&canonical(&s, &specials())),
        Err(BattleMessageError::OutOfOrder { .. })
    ));

    let mut s = slots();
    s[368].0 = 382; // sparse (380 absent) and out of range
    assert_eq!(
        BattleMessageTable::decode(&canonical(&s, &specials())),
        Err(BattleMessageError::IdOutOfRange {
            section: BattleMessageSection::Table,
            id: 382
        })
    );

    let mut s = slots();
    s.remove(10);
    s.push((380, slot_stream(380))); // duplicate 380 appended unsorted
    assert!(BattleMessageTable::decode(&canonical(&s, &specials())).is_err());

    let mut p = specials();
    p[73].0 = 75;
    assert_eq!(
        BattleMessageTable::decode(&canonical(&slots(), &p)),
        Err(BattleMessageError::IdOutOfRange {
            section: BattleMessageSection::Special,
            id: 75
        })
    );
}

#[test]
fn constructor_refuses_missing_duplicate_and_out_of_range() {
    let (s, p) = (slots(), specials());
    let mut sb = borrowed(&s);
    sb.pop();
    assert_eq!(
        BattleMessageTable::from_entries(sb, borrowed(&p)),
        Err(BattleMessageError::MissingId {
            section: BattleMessageSection::Table,
            id: 380
        })
    );
    let mut pb = borrowed(&p);
    pb.push(pb[0]);
    assert!(matches!(
        BattleMessageTable::from_entries(borrowed(&s), pb),
        Err(BattleMessageError::DuplicateId { .. })
    ));
    let mut sb = borrowed(&s);
    sb.push((11, &[0xFF]));
    assert!(matches!(
        BattleMessageTable::from_entries(sb, borrowed(&p)),
        Err(BattleMessageError::IdOutOfRange { .. })
    ));
}

fn decode_with_first_slot(stream: &[u8]) -> Result<(), BattleMessageError> {
    let mut s = slots();
    s[0].1 = stream.to_vec();
    BattleMessageTable::decode(&canonical(&s, &specials())).map(|_| ())
}

#[test]
fn token_streams_representative_placeholders_and_controls() {
    // Placeholder, line-break, pause, sound, and wait tokens, each ending in `EOS`.
    let ok: &[&[u8]] = &[
        &[0xFF],
        &[0xFD, 0x01, 0xFD, 0x00, 0xFE, 0xFA, 0xFB, 0xFF],
        &[0xFC, 0x08, 0x20, 0xFF],
        &[0xFC, 0x10, 0x11, 0x00, 0xFF],
        &[0xFC, 0x0B, 0x60, 0x01, 0xFC, 0x0A, 0xFF],
        &[0xFC, 0x0C, 0xFB, 0xFF],
        &[0xFD, 0xFF, 0xFF], // `0xFF` as a placeholder argument is not `EOS`
    ];
    for stream in ok {
        assert_eq!(decode_with_first_slot(stream), Ok(()), "{stream:02X?}");
    }
}

#[test]
fn malformed_token_streams_are_refused() {
    let bad: &[&[u8]] = &[
        &[],
        &[0x10],
        &[0xFF, 0xFF],
        &[0xFF, 0x10],
        &[0xFD, 0xFF],
        &[0xFD],
        &[0xFC],
        &[0xFC, 0x10, 0x11, 0xFF],
        &[0xFC, 0x99, 0xFF],
        &[0xFC, 0x08, 0xFF],
        &[0xF9],
    ];
    for stream in bad {
        assert!(
            matches!(
                decode_with_first_slot(stream),
                Err(BattleMessageError::MalformedString { .. })
            ),
            "{stream:02X?}"
        );
    }
}

#[test]
fn special_inventory_groups() {
    let bytes = canonical_payload();
    let table = BattleMessageTable::decode(&bytes).unwrap();
    assert_eq!(BATTLE_MESSAGE_SPECIAL_TEMPLATES.len(), 74);
    for (message, count) in [
        (SpecialBattleMessage::IntroMsg, 10),
        (SpecialBattleMessage::IntroSendOut, 10),
        (SpecialBattleMessage::ReturnMon, 7),
        (SpecialBattleMessage::SwitchInMon, 8),
        (SpecialBattleMessage::UsedMove, 26),
        (SpecialBattleMessage::BattleEnd, 13),
    ] {
        assert_eq!(
            table.special_templates(message).count(),
            count,
            "{message:?}"
        );
    }
    let mut symbols: Vec<_> = BATTLE_MESSAGE_SPECIAL_TEMPLATES
        .iter()
        .map(|t| t.symbol)
        .collect();
    symbols.sort_unstable();
    symbols.dedup();
    assert_eq!(symbols.len(), 74);
    assert_eq!(
        table.special_by_symbol("sText_AttackerUsedX").unwrap(),
        special_stream(35)
    );
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
    let pack = load("bmsg-ok", &pack_with(RAW_KIND_TAG, canonical_payload()));
    assert_eq!(
        pack.battle_messages().unwrap().encode(),
        canonical_payload()
    );

    let pack = load("bmsg-missing", &pack_bytes(vec![]));
    assert!(matches!(
        pack.battle_messages(),
        Err(PackError::UnknownAsset(_))
    ));

    let pack = load("bmsg-kind", &pack_with(PALETTE_KIND_TAG, vec![0; 4]));
    assert!(matches!(
        pack.battle_messages(),
        Err(PackError::WrongKind { .. })
    ));

    let mut bad = canonical_payload();
    bad.truncate(bad.len() - 1);
    let pack = load("bmsg-bad", &pack_with(RAW_KIND_TAG, bad));
    assert!(matches!(
        pack.battle_messages(),
        Err(PackError::BattleMessageDecode {
            source: BattleMessageError::Truncated,
            ..
        })
    ));
}
