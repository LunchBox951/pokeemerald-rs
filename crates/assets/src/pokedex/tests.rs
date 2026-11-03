use super::{
    NationalDexNo, PokedexData, PokedexEntry, PokedexError, PokedexOrderKind, PokedexOrders,
    NATIONAL_DEX_COUNT, NATIONAL_DEX_LAST, NATIONAL_DEX_OLD_UNOWN_COUNT,
};

fn dex(number: u16) -> NationalDexNo {
    NationalDexNo::new(number).unwrap()
}

fn dexes(numbers: impl IntoIterator<Item = u16>) -> Vec<NationalDexNo> {
    numbers.into_iter().map(dex).collect()
}

fn entry(tag: u16) -> PokedexEntry {
    PokedexEntry {
        category: format!("CAT{tag}"),
        description: format!("Line one {tag}\nLine two é"),
        height: tag + 1,
        weight: tag + 2,
        pokemon_scale: 356,
        pokemon_offset: -17,
        trainer_scale: 256,
        trainer_offset: 9,
    }
}

fn orders() -> PokedexOrders {
    PokedexOrders::new(dexes([3, 1, 2, 390]), dexes([2, 1]), dexes([1])).unwrap()
}

fn data() -> PokedexData {
    PokedexData::new(
        vec![
            (dex(2), entry(2)),
            (dex(0), entry(0)),
            (dex(386), entry(386)),
        ],
        orders(),
    )
    .unwrap()
}

/// Bytes for zero entries and three empty orders.
const EMPTY_PAYLOAD: [u8; 8] = [0; 8];

#[test]
fn identity_range_spans_none_through_the_old_unown_identities() {
    assert_eq!(NATIONAL_DEX_LAST, 411);
    assert_eq!(NATIONAL_DEX_COUNT + NATIONAL_DEX_OLD_UNOWN_COUNT, 411);
    assert_eq!(NationalDexNo::new(0), Ok(NationalDexNo::NONE));
    assert_eq!(NationalDexNo::new(411).unwrap().get(), 411);
    assert_eq!(
        NationalDexNo::new(412),
        Err(PokedexError::InvalidDexNo(412))
    );
}

#[test]
fn round_trip_preserves_entries_and_orders() {
    let original = data();
    let decoded = PokedexData::decode(&original.encode()).unwrap();
    assert_eq!(decoded, original);
    assert_eq!(decoded.entries()[0].0, dex(0));
    assert_eq!(decoded.entry(dex(2)), Some(&entry(2)));
    assert_eq!(decoded.entry(dex(3)), None);
    assert_eq!(decoded.orders().alphabetical(), dexes([3, 1, 2, 390]));
}

#[test]
fn upstream_sized_orders_with_independent_lengths_round_trip() {
    let alphabetical = dexes(1..=NATIONAL_DEX_LAST);
    let weight = dexes((1..=NATIONAL_DEX_COUNT).rev());
    let height = dexes(1..=NATIONAL_DEX_COUNT);
    let orders = PokedexOrders::new(alphabetical, weight, height).unwrap();
    let data = PokedexData::new(vec![], orders).unwrap();
    let decoded = PokedexData::decode(&data.encode()).unwrap();
    assert_eq!(decoded.orders().alphabetical().len(), 411);
    assert_eq!(decoded.orders().weight().len(), 386);
    assert_eq!(decoded.orders().height().len(), 386);
    assert_eq!(decoded, data);
}

#[test]
fn orders_need_not_cover_the_entries_or_each_other() {
    let orders = PokedexOrders::new(dexes([5]), vec![], dexes([1, 2, 3])).unwrap();
    let data = PokedexData::new(vec![(dex(9), entry(9))], orders).unwrap();
    assert_eq!(PokedexData::decode(&data.encode()).unwrap(), data);
}

#[test]
fn full_entry_set_round_trips() {
    let entries = (0..=NATIONAL_DEX_COUNT)
        .map(|number| (dex(number), entry(number)))
        .collect();
    let data = PokedexData::new(entries, orders()).unwrap();
    assert_eq!(data.entries().len(), 387);
    assert_eq!(PokedexData::decode(&data.encode()).unwrap(), data);
}

#[test]
fn empty_payload_decodes_to_empty_data() {
    let decoded = PokedexData::decode(&EMPTY_PAYLOAD).unwrap();
    assert!(decoded.entries().is_empty());
    assert!(decoded.orders().height().is_empty());
}

#[test]
fn every_proper_prefix_is_truncated() {
    let bytes = data().encode();
    for len in 0..bytes.len() {
        assert_eq!(
            PokedexData::decode(&bytes[..len]),
            Err(PokedexError::Truncated),
            "prefix of {len} bytes"
        );
    }
}

#[test]
fn trailing_bytes_are_rejected() {
    let mut bytes = data().encode();
    bytes.extend_from_slice(&[0, 0, 7]);
    assert_eq!(
        PokedexData::decode(&bytes),
        Err(PokedexError::TrailingBytes(3))
    );
}

#[test]
fn invalid_utf8_text_is_rejected() {
    // entry_count 1, dex 1, category length 1 holding 0xFF.
    let bytes = [1, 0, 1, 0, 1, 0, 0xFF];
    assert_eq!(
        PokedexData::decode(&bytes),
        Err(PokedexError::InvalidString)
    );
}

#[test]
fn entry_count_beyond_the_identities_is_rejected() {
    let bytes = 388u16.to_le_bytes();
    assert_eq!(
        PokedexData::decode(&bytes),
        Err(PokedexError::TooManyEntries(388))
    );
}

#[test]
fn order_count_beyond_the_identities_is_rejected() {
    let mut bytes = vec![0, 0];
    bytes.extend_from_slice(&412u16.to_le_bytes());
    assert_eq!(
        PokedexData::decode(&bytes),
        Err(PokedexError::OrderTooLong {
            order: PokedexOrderKind::Alphabetical,
            len: 412
        })
    );
}

#[test]
fn decode_rejects_identity_beyond_the_last_in_orders() {
    let mut bytes = vec![0, 0, 1, 0];
    bytes.extend_from_slice(&412u16.to_le_bytes());
    assert_eq!(
        PokedexData::decode(&bytes),
        Err(PokedexError::InvalidDexNo(412))
    );
}

#[test]
fn decode_rejects_entry_identity_beyond_the_last() {
    let mut bytes = vec![1, 0];
    bytes.extend_from_slice(&412u16.to_le_bytes());
    assert_eq!(
        PokedexData::decode(&bytes),
        Err(PokedexError::InvalidDexNo(412))
    );
}

#[test]
fn entries_above_the_last_species_are_rejected_at_construction_and_decode() {
    assert_eq!(
        PokedexData::new(vec![(dex(387), entry(1))], orders()),
        Err(PokedexError::EntryDexNoOutOfRange(387))
    );
    let mut bytes = PokedexData::new(vec![(dex(386), entry(1))], orders())
        .unwrap()
        .encode();
    bytes[2..4].copy_from_slice(&387u16.to_le_bytes());
    assert_eq!(
        PokedexData::decode(&bytes),
        Err(PokedexError::EntryDexNoOutOfRange(387))
    );
}

#[test]
fn duplicate_entries_are_rejected_at_construction_and_decode() {
    assert_eq!(
        PokedexData::new(vec![(dex(4), entry(1)), (dex(4), entry(2))], orders()),
        Err(PokedexError::DuplicateEntry(4))
    );
    let one = PokedexData::new(vec![(dex(4), entry(1))], orders())
        .unwrap()
        .encode();
    let two = PokedexData::new(vec![(dex(4), entry(1)), (dex(5), entry(1))], orders())
        .unwrap()
        .encode();
    let second_key = 2 + (two.len() - one.len());
    let mut bytes = two;
    bytes[second_key..second_key + 2].copy_from_slice(&4u16.to_le_bytes());
    assert_eq!(
        PokedexData::decode(&bytes),
        Err(PokedexError::DuplicateEntry(4))
    );
}

#[test]
fn descending_entries_are_rejected_on_decode() {
    let two = PokedexData::new(vec![(dex(4), entry(1)), (dex(5), entry(1))], orders())
        .unwrap()
        .encode();
    let one = PokedexData::new(vec![(dex(4), entry(1))], orders())
        .unwrap()
        .encode();
    let second_key = 2 + (two.len() - one.len());
    let mut bytes = two;
    bytes[2..4].copy_from_slice(&6u16.to_le_bytes());
    assert_eq!(bytes[second_key..second_key + 2], 5u16.to_le_bytes());
    assert_eq!(
        PokedexData::decode(&bytes),
        Err(PokedexError::EntriesOutOfOrder {
            previous: 6,
            current: 5
        })
    );
}

#[test]
fn oversized_text_is_rejected_at_construction() {
    let mut long = entry(1);
    long.description = "x".repeat(usize::from(u16::MAX) + 1);
    assert_eq!(
        PokedexData::new(vec![(dex(1), long)], orders()),
        Err(PokedexError::TextTooLong(65536))
    );
    let mut longest = entry(1);
    longest.category = "x".repeat(usize::from(u16::MAX));
    let data = PokedexData::new(vec![(dex(1), longest)], orders()).unwrap();
    assert_eq!(PokedexData::decode(&data.encode()).unwrap(), data);
}

#[test]
fn orders_reject_none_and_duplicate_members() {
    assert_eq!(
        PokedexOrders::new(dexes([1, 0]), vec![], vec![]),
        Err(PokedexError::NoneInOrder(PokedexOrderKind::Alphabetical))
    );
    assert_eq!(
        PokedexOrders::new(vec![], dexes([7, 8, 7]), vec![]),
        Err(PokedexError::DuplicateOrderMember {
            order: PokedexOrderKind::Weight,
            dex_no: 7
        })
    );
    assert_eq!(
        PokedexOrders::new(vec![], vec![], dexes([400, 400])),
        Err(PokedexError::DuplicateOrderMember {
            order: PokedexOrderKind::Height,
            dex_no: 400
        })
    );
}

#[test]
fn decode_rejects_a_duplicate_order_member() {
    let mut bytes = vec![0, 0, 2, 0];
    bytes.extend_from_slice(&[9, 0, 9, 0, 0, 0, 0, 0]);
    assert_eq!(
        PokedexData::decode(&bytes),
        Err(PokedexError::DuplicateOrderMember {
            order: PokedexOrderKind::Alphabetical,
            dex_no: 9
        })
    );
}

#[test]
fn errors_render_a_message() {
    for error in [
        PokedexError::Truncated,
        PokedexError::TrailingBytes(1),
        PokedexError::InvalidString,
        PokedexError::TextTooLong(70000),
        PokedexError::InvalidDexNo(500),
        PokedexError::EntryDexNoOutOfRange(400),
        PokedexError::DuplicateEntry(3),
        PokedexError::EntriesOutOfOrder {
            previous: 4,
            current: 3,
        },
        PokedexError::TooManyEntries(500),
        PokedexError::OrderTooLong {
            order: PokedexOrderKind::Height,
            len: 500,
        },
        PokedexError::NoneInOrder(PokedexOrderKind::Weight),
        PokedexError::DuplicateOrderMember {
            order: PokedexOrderKind::Alphabetical,
            dex_no: 2,
        },
    ] {
        assert!(!error.to_string().is_empty(), "{error:?}");
    }
}
