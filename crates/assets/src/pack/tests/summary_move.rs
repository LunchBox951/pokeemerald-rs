use super::super::{
    AssetPack, ContestCategory, ContestEffectId, ContestEffectRecord, ContestEffectType,
    ContestMoveRecord, PackError, SummaryMoveError, SummaryMoveSection, SummaryMoveTable,
    CONTEST_EFFECT_COUNT, CONTEST_MOVE_COUNT, MOVE_DESCRIPTION_COUNT,
};
use super::shared::{pack_bytes, write_pack, Entry, PALETTE_KIND_TAG, RAW_KIND_TAG};
use crate::MoveId;

const TABLE_PACK_ID: &str = "summary-move/table";
/// Offsets into the canonical payload: magic, version, three counts.
const HEADER: usize = 12;
/// Slot whose description is a bare `EOS`, as upstream's empty entries are.
const EMPTY_SLOT: usize = 7;
const TYPES: [u8; 8] = [0, 1, 2, 3, 4, 5, 6, 8];

fn description(slot: usize) -> Vec<u8> {
    if slot == EMPTY_SLOT {
        vec![0xFF]
    } else {
        vec![0xBB, u8::try_from(slot % 200).unwrap(), 0xFF]
    }
}

fn effect_description(id: usize) -> Vec<u8> {
    vec![0xC0, u8::try_from(id).unwrap(), 0xFF]
}

fn move_record(index: usize) -> (u16, u8, u8) {
    (
        u16::try_from(index).unwrap(),
        u8::try_from(index % CONTEST_EFFECT_COUNT).unwrap(),
        u8::try_from(index % 5).unwrap(),
    )
}

fn effect_record(id: usize) -> (u16, u8, u8, u8) {
    (
        u16::try_from(id).unwrap(),
        TYPES[id % TYPES.len()],
        u8::try_from(id * 5).unwrap(),
        u8::try_from(id).unwrap(),
    )
}

/// Hand-written so the tests do not rely on `encode` to check `encode`.
fn canonical() -> Vec<u8> {
    let mut out = b"SMVE".to_vec();
    out.extend_from_slice(&1u16.to_le_bytes());
    for count in [
        MOVE_DESCRIPTION_COUNT,
        CONTEST_MOVE_COUNT,
        CONTEST_EFFECT_COUNT,
    ] {
        out.extend_from_slice(&u16::try_from(count).unwrap().to_le_bytes());
    }
    for slot in 0..MOVE_DESCRIPTION_COUNT {
        let stream = description(slot);
        out.extend_from_slice(&u16::try_from(slot).unwrap().to_le_bytes());
        out.extend_from_slice(&u32::try_from(stream.len()).unwrap().to_le_bytes());
        out.extend_from_slice(&stream);
    }
    for index in 0..CONTEST_MOVE_COUNT {
        let (id, effect, category) = move_record(index);
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(&[effect, category]);
    }
    for id in 0..CONTEST_EFFECT_COUNT {
        let (id16, kind, appeal, jam) = effect_record(id);
        let stream = effect_description(id);
        out.extend_from_slice(&id16.to_le_bytes());
        out.extend_from_slice(&[kind, appeal, jam]);
        out.extend_from_slice(&u32::try_from(stream.len()).unwrap().to_le_bytes());
        out.extend_from_slice(&stream);
    }
    out
}

fn build(
    mutate: impl FnOnce(
        &mut Vec<(u16, Vec<u8>)>,
        &mut Vec<ContestMoveRecord>,
        &mut Vec<ContestEffectRecordOwned>,
    ),
) -> Result<Vec<u8>, SummaryMoveError> {
    let mut descriptions: Vec<_> = (0..MOVE_DESCRIPTION_COUNT)
        .map(|slot| (u16::try_from(slot).unwrap(), description(slot)))
        .collect();
    let mut moves: Vec<_> = (0..CONTEST_MOVE_COUNT)
        .map(|index| {
            let (move_id, effect, category) = move_record(index);
            ContestMoveRecord {
                move_id,
                effect,
                category,
            }
        })
        .collect();
    let mut effects: Vec<_> = (0..CONTEST_EFFECT_COUNT)
        .map(|id| {
            let (id16, effect_type, appeal, jam) = effect_record(id);
            ContestEffectRecordOwned {
                id: id16,
                effect_type,
                appeal,
                jam,
                description: effect_description(id),
            }
        })
        .collect();
    mutate(&mut descriptions, &mut moves, &mut effects);
    SummaryMoveTable::from_entries(
        descriptions.iter().map(|(slot, s)| (*slot, s.as_slice())),
        moves,
        effects.iter().map(|e| ContestEffectRecord {
            id: e.id,
            effect_type: e.effect_type,
            appeal: e.appeal,
            jam: e.jam,
            description: &e.description,
        }),
    )
    .map(|table| table.encode())
}

struct ContestEffectRecordOwned {
    id: u16,
    effect_type: u8,
    appeal: u8,
    jam: u8,
    description: Vec<u8>,
}

fn decode_err(bytes: &[u8]) -> SummaryMoveError {
    SummaryMoveTable::decode(bytes).expect_err("a faulty payload")
}

#[test]
fn counts_are_upstreams() {
    assert_eq!(MOVE_DESCRIPTION_COUNT, 354);
    assert_eq!(CONTEST_MOVE_COUNT, 355);
    assert_eq!(CONTEST_EFFECT_COUNT, 48);
}

#[test]
fn canonical_bytes_decode_and_reencode_identically() {
    let bytes = canonical();
    let table = SummaryMoveTable::decode(&bytes).unwrap();
    assert_eq!(table.encode(), bytes);
}

#[test]
fn from_entries_encodes_canonically_whatever_the_input_order() {
    assert_eq!(build(|_, _, _| {}).unwrap(), canonical());
    let shuffled = build(|d, m, e| {
        d.reverse();
        m.reverse();
        e.reverse();
    })
    .unwrap();
    assert_eq!(shuffled, canonical());
}

#[test]
fn readers_index_descriptions_by_move_minus_one() {
    let bytes = canonical();
    let table = SummaryMoveTable::decode(&bytes).unwrap();
    assert_eq!(table.move_description(MoveId(0)), None);
    assert_eq!(
        table.move_description(MoveId(1)),
        Some(description(0).as_slice())
    );
    assert_eq!(
        table.move_description(MoveId(354)),
        Some(description(353).as_slice())
    );
    assert_eq!(table.move_description(MoveId(355)), None);
    // The empty entry keeps its slot rather than shifting the ones after it.
    assert_eq!(
        table.move_description(MoveId(u16::try_from(EMPTY_SLOT + 1).unwrap())),
        Some([0xFF].as_slice())
    );
    assert_eq!(
        table.move_description(MoveId(u16::try_from(EMPTY_SLOT + 2).unwrap())),
        Some(description(EMPTY_SLOT + 1).as_slice())
    );
}

#[test]
fn readers_resolve_contest_moves_and_effects() {
    let bytes = canonical();
    let table = SummaryMoveTable::decode(&bytes).unwrap();
    let none = table.contest_move(MoveId(0)).unwrap();
    assert_eq!(none.category, ContestCategory::Cool);
    assert_eq!(none.effect, ContestEffectId::new(0).unwrap());
    let last = table.contest_move(MoveId(354)).unwrap();
    assert_eq!(last.category, ContestCategory::Tough);
    assert_eq!(last.effect.index(), 18);
    assert_eq!(table.contest_move(MoveId(355)), None);
    assert_eq!(table.contest_effect_of(MoveId(355)), None);

    let effect = table.contest_effect(ContestEffectId::new(47).unwrap());
    assert_eq!(effect.effect_type, ContestEffectType::Unknown);
    assert_eq!(effect.appeal, 235);
    assert_eq!(effect.jam, 47);
    assert_eq!(effect.description, effect_description(47));
    assert_eq!(
        table
            .contest_effect(ContestEffectId::new(6).unwrap())
            .effect_type,
        ContestEffectType::TurnOrder
    );
    assert_eq!(
        table
            .contest_effect(ContestEffectId::new(7).unwrap())
            .effect_type,
        ContestEffectType::Unknown
    );
    assert_eq!(ContestEffectId::new(48), None);
    assert_eq!(
        table.contest_effect_of(MoveId(1)),
        Some(table.contest_effect(ContestEffectId::new(1).unwrap()))
    );
    assert_eq!(table.descriptions().count(), MOVE_DESCRIPTION_COUNT);
    assert_eq!(table.contest_moves().count(), CONTEST_MOVE_COUNT);
    assert_eq!(table.contest_effects().count(), CONTEST_EFFECT_COUNT);
}

#[test]
fn enums_round_trip_their_upstream_values() {
    for raw in 0..=u8::MAX {
        assert_eq!(
            ContestCategory::from_raw(raw).map(ContestCategory::raw),
            (raw < 5).then_some(raw)
        );
        assert_eq!(
            ContestEffectType::from_raw(raw).map(ContestEffectType::raw),
            TYPES.contains(&raw).then_some(raw)
        );
    }
}

#[test]
fn constructor_refuses_missing_duplicate_and_out_of_range_records() {
    use SummaryMoveSection::{ContestEffect, ContestMove, Description};
    assert_eq!(
        build(|d, _, _| {
            d.pop();
        }),
        Err(SummaryMoveError::MissingId {
            section: Description,
            id: 353
        })
    );
    assert_eq!(
        build(|d, _, _| d[3].0 = 4),
        Err(SummaryMoveError::DuplicateId {
            section: Description,
            id: 4
        })
    );
    // Upstream's table has no MOVE_NONE slot, so move 354's slot is 353 and 354 is past it.
    assert_eq!(
        build(|d, _, _| d[0].0 = 354),
        Err(SummaryMoveError::IdOutOfRange {
            section: Description,
            id: 354
        })
    );
    assert_eq!(
        build(|_, m, _| m[9].move_id = 355),
        Err(SummaryMoveError::IdOutOfRange {
            section: ContestMove,
            id: 355
        })
    );
    assert_eq!(
        build(|_, m, _| m[9].move_id = 8),
        Err(SummaryMoveError::DuplicateId {
            section: ContestMove,
            id: 8
        })
    );
    assert_eq!(
        build(|_, m, _| {
            m.remove(0);
        }),
        Err(SummaryMoveError::MissingId {
            section: ContestMove,
            id: 0
        })
    );
    assert_eq!(
        build(|_, _, e| e[0].id = 48),
        Err(SummaryMoveError::IdOutOfRange {
            section: ContestEffect,
            id: 48
        })
    );
    assert_eq!(
        build(|_, _, e| {
            e.pop();
        }),
        Err(SummaryMoveError::MissingId {
            section: ContestEffect,
            id: 47
        })
    );
}

#[test]
fn cross_table_and_value_references_are_checked() {
    assert_eq!(
        build(|_, m, _| m[354].effect = 48),
        Err(SummaryMoveError::EffectOutOfRange {
            move_id: 354,
            effect: 48
        })
    );
    assert_eq!(
        build(|_, m, _| m[2].category = 5),
        Err(SummaryMoveError::CategoryOutOfRange {
            move_id: 2,
            category: 5
        })
    );
    assert_eq!(
        build(|_, _, e| e[4].effect_type = 7),
        Err(SummaryMoveError::EffectTypeInvalid {
            id: 4,
            effect_type: 7
        })
    );
}

#[test]
fn text_records_are_walked_by_token() {
    let ok: &[&[u8]] = &[&[0xFF], &[0xFD, 0xFF, 0xFF], &[0xFC, 0x08, 0x20, 0xFF]];
    for stream in ok {
        build(|d, _, _| d[2].1 = stream.to_vec()).unwrap_or_else(|e| panic!("{stream:02X?}: {e}"));
    }
    let bad: &[&[u8]] = &[&[], &[0xFD], &[0xFC, 0x99, 0xFF], &[0xFF, 0xFF], &[0x80]];
    for stream in bad {
        assert!(
            matches!(
                build(|d, _, _| d[2].1 = stream.to_vec()),
                Err(SummaryMoveError::MalformedString {
                    section: SummaryMoveSection::Description,
                    id: 2,
                    ..
                })
            ),
            "{stream:02X?}"
        );
        assert!(
            matches!(
                build(|_, _, e| e[3].description = stream.to_vec()),
                Err(SummaryMoveError::MalformedString {
                    section: SummaryMoveSection::ContestEffect,
                    id: 3,
                    ..
                })
            ),
            "{stream:02X?}"
        );
    }
}

#[test]
fn every_truncated_prefix_is_refused() {
    let bytes = canonical();
    for len in 0..bytes.len() {
        assert_eq!(
            decode_err(&bytes[..len]),
            SummaryMoveError::Truncated,
            "prefix {len}"
        );
    }
}

#[test]
fn header_and_record_faults_are_refused() {
    let good = canonical();
    let mut bad = good.clone();
    bad[0] = b'X';
    assert_eq!(decode_err(&bad), SummaryMoveError::BadMagic);
    let mut bad = good.clone();
    bad[4] = 2;
    assert_eq!(decode_err(&bad), SummaryMoveError::UnsupportedVersion(2));
    for (offset, section, found) in [
        (6, SummaryMoveSection::Description, 355u16),
        (8, SummaryMoveSection::ContestMove, 354),
        (10, SummaryMoveSection::ContestEffect, 49),
    ] {
        let mut bad = good.clone();
        bad[offset..offset + 2].copy_from_slice(&found.to_le_bytes());
        assert_eq!(
            decode_err(&bad),
            SummaryMoveError::HeaderMismatch { section, found }
        );
    }
    // First description record: slot 0, then its length.
    let mut bad = good.clone();
    bad[HEADER + 2..HEADER + 6].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(decode_err(&bad), SummaryMoveError::Truncated);
    let mut bad = good.clone();
    bad.push(0);
    assert_eq!(decode_err(&bad), SummaryMoveError::TrailingBytes);
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
    let pack = load("smve-ok", &pack_with(RAW_KIND_TAG, canonical()));
    assert_eq!(
        pack.summary_moves().unwrap().move_description(MoveId(1)),
        Some(description(0).as_slice())
    );

    let pack = load("smve-missing", &pack_bytes(vec![]));
    assert!(matches!(
        pack.summary_moves(),
        Err(PackError::UnknownAsset(_))
    ));

    let pack = load("smve-kind", &pack_with(PALETTE_KIND_TAG, vec![0; 4]));
    assert!(matches!(
        pack.summary_moves(),
        Err(PackError::WrongKind { .. })
    ));

    let mut bad = canonical();
    bad.truncate(bad.len() - 1);
    let pack = load("smve-bad", &pack_with(RAW_KIND_TAG, bad));
    assert!(matches!(
        pack.summary_moves(),
        Err(PackError::SummaryMoveDecode {
            source: SummaryMoveError::Truncated,
            ..
        })
    ));

    // A dangling effect reference is caught on every access, not only at extraction.
    let mut dangling = canonical();
    let first_move = HEADER
        + (0..MOVE_DESCRIPTION_COUNT)
            .map(|s| 6 + description(s).len())
            .sum::<usize>();
    dangling[first_move + 2] = 48;
    let pack = load("smve-dangling", &pack_with(RAW_KIND_TAG, dangling));
    assert!(matches!(
        pack.summary_moves(),
        Err(PackError::SummaryMoveDecode {
            source: SummaryMoveError::EffectOutOfRange {
                move_id: 0,
                effect: 48
            },
            ..
        })
    ));
}

/// `charmap.txt` bytes for the plain characters the sampled upstream texts use.
fn charmap_text(text: &str) -> Vec<u8> {
    let mut bytes: Vec<u8> = text
        .chars()
        .map(|c| match c {
            ' ' => 0x00,
            '.' => 0xAD,
            '-' => 0xAE,
            ',' => 0xB8,
            '\n' => 0xFE,
            'A'..='Z' => 0xBB + (c as u8 - b'A'),
            'a'..='z' => 0xD5 + (c as u8 - b'a'),
            other => panic!("unmapped {other:?}"),
        })
        .collect();
    bytes.push(0xFF);
    bytes
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_summary_move_table_has_upstreams_lengths_and_records() {
    let pack = AssetPack::load_repo().expect("run `cargo xtask extract` first");
    let table = pack.summary_moves().expect("the summary-move table");

    // `gMoveDescriptionPointers[MOVES_COUNT - 1]`, `gContestMoves[MOVES_COUNT]`,
    // `gContestEffects`, and `gContestEffectDescriptionPointers` (CONTEST_EFFECT_COUNT 48).
    assert_eq!(table.descriptions().count(), 354);
    assert_eq!(table.contest_moves().count(), 355);
    assert_eq!(table.contest_effects().count(), 48);

    assert_eq!(table.move_description(MoveId(0)), None);
    assert_eq!(
        table.move_description(MoveId(1)),
        Some(charmap_text("Pounds the foe with\nforelegs or tail.").as_slice())
    );
    assert_eq!(
        table.move_description(MoveId(354)),
        Some(charmap_text("Allows a full-power attack,\nbut sharply lowers SP. ATK.").as_slice())
    );

    for (move_id, category, effect) in [
        (0, ContestCategory::Cool, 0),
        (1, ContestCategory::Tough, 0),
        (3, ContestCategory::Tough, 17),
        (354, ContestCategory::Smart, 1),
    ] {
        let row = table.contest_move(MoveId(move_id)).unwrap();
        assert_eq!(
            (row.category, row.effect.index()),
            (category, effect),
            "move {move_id}"
        );
    }
    for (id, kind, appeal, jam) in [
        (0, ContestEffectType::Appeal, 40, 0),
        (1, ContestEffectType::Appeal, 60, 0),
        (17, ContestEffectType::StartleMons, 20, 10),
        (47, ContestEffectType::Worsen, 30, 0),
    ] {
        let effect = table.contest_effect(ContestEffectId::new(id).unwrap());
        assert_eq!(
            (effect.effect_type, effect.appeal, effect.jam),
            (kind, appeal, jam),
            "effect {id}"
        );
    }
    assert_eq!(
        table
            .contest_effect(ContestEffectId::new(0).unwrap())
            .description,
        charmap_text("A highly appealing move.")
    );
    assert_eq!(
        table
            .contest_effect(ContestEffectId::new(47).unwrap())
            .description,
        charmap_text("Temporarily stops the\ncrowd from growing excited.")
    );

    // Every move resolves to a contest effect with a description, and the
    // texts are terminated. The move table keeps its names and battle
    // parameters; this table adds none of them.
    for move_id in 0..u16::try_from(CONTEST_MOVE_COUNT).unwrap() {
        let effect = table.contest_effect_of(MoveId(move_id)).unwrap();
        assert!(effect.description.len() > 1, "move {move_id}");
    }
    assert!(table.descriptions().all(|s| s.last() == Some(&0xFF)));
}
