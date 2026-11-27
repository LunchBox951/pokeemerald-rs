use super::super::{
    AssetPack, PackError, SightTrainerIntroSpeechError, SightTrainerIntroSpeechTable,
    SIGHT_TRAINER_INTRO_COUNT, SIGHT_TRAINER_INTRO_SCRIPTS,
};
use super::shared::{pack_bytes, write_pack, Entry, PALETTE_KIND_TAG, RAW_KIND_TAG};

const TABLE_PACK_ID: &str = "sight-trainer-intro-speech/table";

/// Hand-listed rather than read from the crate, so a reordered or renamed
/// inventory fails here.
const SCRIPTS: [&str; 9] = [
    "Route103_EventScript_Amy",
    "Route103_EventScript_Andrew",
    "Route103_EventScript_Daisy",
    "Route103_EventScript_Isabelle",
    "Route103_EventScript_Liv",
    "Route103_EventScript_Marcos",
    "Route103_EventScript_Miguel",
    "Route103_EventScript_Pete",
    "Route103_EventScript_Rhett",
];

/// A distinct stream per position, so a lookup that returns the wrong record fails.
fn stream(position: usize) -> Vec<u8> {
    vec![0xA1 + u8::try_from(position).unwrap(), 0xFF]
}

fn streams() -> Vec<Vec<u8>> {
    (0..9).map(stream).collect()
}

/// Hand-written so the tests do not rely on `encode` to check `encode`.
fn record(key: &str, stream: &[u8]) -> Vec<u8> {
    let mut out = u16::try_from(key.len()).unwrap().to_le_bytes().to_vec();
    out.extend_from_slice(key.as_bytes());
    out.extend_from_slice(&u32::try_from(stream.len()).unwrap().to_le_bytes());
    out.extend_from_slice(stream);
    out
}

fn wire(records: &[Vec<u8>]) -> Vec<u8> {
    let mut out = b"STIS".to_vec();
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&u16::try_from(records.len()).unwrap().to_le_bytes());
    for record in records {
        out.extend_from_slice(record);
    }
    out
}

fn records(streams: &[Vec<u8>]) -> Vec<Vec<u8>> {
    SCRIPTS
        .iter()
        .zip(streams)
        .map(|(key, stream)| record(key, stream))
        .collect()
}

fn canonical(streams: &[Vec<u8>]) -> Vec<u8> {
    wire(&records(streams))
}

fn entries(streams: &[Vec<u8>]) -> Vec<(&'static str, &[u8])> {
    SCRIPTS
        .iter()
        .copied()
        .zip(streams.iter().map(Vec::as_slice))
        .collect()
}

#[test]
fn inventory_is_the_nine_route_103_sight_scripts_in_canonical_order() {
    assert_eq!(SIGHT_TRAINER_INTRO_COUNT, 9);
    assert_eq!(SIGHT_TRAINER_INTRO_SCRIPTS, SCRIPTS);
    let mut sorted = SCRIPTS;
    sorted.sort_unstable();
    assert_eq!(sorted, SCRIPTS);
}

#[test]
fn canonical_bytes_decode_and_reencode_identically() {
    let streams = streams();
    let bytes = canonical(&streams);
    let table = SightTrainerIntroSpeechTable::decode(&bytes).unwrap();
    assert_eq!(table.encode(), bytes);
    for (script, stream) in SCRIPTS.iter().zip(&streams) {
        assert_eq!(table.speech(script), Some(stream.as_slice()), "{script}");
    }
    assert_eq!(table.speech("Route103_EventScript_Nobody"), None);
    assert_eq!(table.speech(""), None);
}

#[test]
fn two_scripts_that_share_a_trainer_keep_their_own_speech() {
    let bytes = canonical(&streams());
    let table = SightTrainerIntroSpeechTable::decode(&bytes).unwrap();
    assert_ne!(
        table.speech("Route103_EventScript_Amy"),
        table.speech("Route103_EventScript_Liv")
    );
}

#[test]
fn from_entries_encodes_canonically_in_any_order() {
    let streams = streams();
    let forward = SightTrainerIntroSpeechTable::from_entries(entries(&streams)).unwrap();
    let mut items = entries(&streams);
    items.reverse();
    let reversed = SightTrainerIntroSpeechTable::from_entries(items).unwrap();
    assert_eq!(forward.encode(), canonical(&streams));
    assert_eq!(reversed.encode(), forward.encode());
}

#[test]
fn constructor_refuses_missing_duplicate_and_unknown_scripts() {
    let streams = streams();
    for drop in 0..9 {
        let mut items = entries(&streams);
        items.remove(drop);
        assert_eq!(
            SightTrainerIntroSpeechTable::from_entries(items),
            Err(SightTrainerIntroSpeechError::MissingScript { position: drop })
        );
    }
    for repeat in 0..9 {
        let mut items = entries(&streams);
        items.push((SCRIPTS[repeat], streams[repeat].as_slice()));
        assert_eq!(
            SightTrainerIntroSpeechTable::from_entries(items),
            Err(SightTrainerIntroSpeechError::DuplicateScript { position: repeat })
        );
    }
    for unknown in [
        "Route103_EventScript_Nobody",
        "Route103_EventScript_amy",
        "Route103_EventScript_Amy ",
        "",
    ] {
        let mut items = entries(&streams);
        items[3].0 = unknown;
        assert_eq!(
            SightTrainerIntroSpeechTable::from_entries(items),
            Err(SightTrainerIntroSpeechError::UnknownScript),
            "{unknown:?}"
        );
    }
}

#[test]
fn every_truncated_prefix_is_refused() {
    let bytes = canonical(&streams());
    for len in 0..bytes.len() {
        assert_eq!(
            SightTrainerIntroSpeechTable::decode(&bytes[..len]),
            Err(SightTrainerIntroSpeechError::Truncated),
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
        SightTrainerIntroSpeechTable::decode(&bad),
        Err(SightTrainerIntroSpeechError::BadMagic)
    );
    let mut bad = good.clone();
    bad[4] = 2;
    assert_eq!(
        SightTrainerIntroSpeechTable::decode(&bad),
        Err(SightTrainerIntroSpeechError::UnsupportedVersion(2))
    );
    let mut bad = good.clone();
    bad[6] = 8;
    assert_eq!(
        SightTrainerIntroSpeechTable::decode(&bad),
        Err(SightTrainerIntroSpeechError::HeaderMismatch { found: 8 })
    );
    // The first record's key length claims more bytes than the payload holds.
    let mut bad = good.clone();
    bad[8..10].copy_from_slice(&u16::MAX.to_le_bytes());
    assert_eq!(
        SightTrainerIntroSpeechTable::decode(&bad),
        Err(SightTrainerIntroSpeechError::Truncated)
    );
    let stream_len_at = 8 + 2 + SCRIPTS[0].len();
    let mut bad = good.clone();
    bad[stream_len_at..stream_len_at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(
        SightTrainerIntroSpeechTable::decode(&bad),
        Err(SightTrainerIntroSpeechError::Truncated)
    );
    let mut bad = good;
    bad.push(0);
    assert_eq!(
        SightTrainerIntroSpeechTable::decode(&bad),
        Err(SightTrainerIntroSpeechError::TrailingBytes)
    );
}

#[test]
fn records_missing_duplicated_unknown_or_out_of_order_on_the_wire_are_refused() {
    let streams = streams();
    let good = records(&streams);

    let mut swapped = good.clone();
    swapped.swap(2, 6);
    assert_eq!(
        SightTrainerIntroSpeechTable::decode(&wire(&swapped)),
        Err(SightTrainerIntroSpeechError::OutOfOrder { position: 2 })
    );
    let mut rotated = good.clone();
    rotated.rotate_left(1);
    assert_eq!(
        SightTrainerIntroSpeechTable::decode(&wire(&rotated)),
        Err(SightTrainerIntroSpeechError::OutOfOrder { position: 0 })
    );

    let mut duplicated = good.clone();
    duplicated[4] = good[3].clone();
    assert_eq!(
        SightTrainerIntroSpeechTable::decode(&wire(&duplicated)),
        Err(SightTrainerIntroSpeechError::DuplicateScript { position: 3 })
    );

    let mut unknown = good.clone();
    unknown[4] = record("Route103_EventScript_Nobody", &streams[4]);
    assert_eq!(
        SightTrainerIntroSpeechTable::decode(&wire(&unknown)),
        Err(SightTrainerIntroSpeechError::UnknownScript)
    );

    // Eight records behind a count of nine run off the end.
    assert_eq!(
        SightTrainerIntroSpeechTable::decode(&{
            let mut bytes = wire(&good[..8]);
            bytes[6] = 9;
            bytes
        }),
        Err(SightTrainerIntroSpeechError::Truncated)
    );
}

#[test]
fn token_streams_are_walked_by_token() {
    let with = |stream: &[u8]| {
        let mut streams = streams();
        streams[4] = stream.to_vec();
        canonical(&streams)
    };
    // Controls stay tokens, including an `0xFF` operand that is not the terminator.
    let ok: &[&[u8]] = &[
        &[0xFF],
        &[0xFD, 0x01, 0xFD, 0x05, 0xFF],
        &[0xFC, 0x08, 0x20, 0xFF],
        &[0xFC, 0x08, 0xFF, 0xA1, 0xFF],
    ];
    for stream in ok {
        let bytes = with(stream);
        let table = SightTrainerIntroSpeechTable::decode(&bytes)
            .unwrap_or_else(|e| panic!("{stream:02X?}: {e}"));
        assert_eq!(table.speech("Route103_EventScript_Liv"), Some(*stream));
    }
    let bad: &[&[u8]] = &[
        &[],
        &[0xFD],
        &[0xFD, 0x01],
        &[0xFC, 0x99, 0xFF],
        &[0xFF, 0xFF],
        &[0xFD, 0xFF],
    ];
    for stream in bad {
        assert!(
            matches!(
                SightTrainerIntroSpeechTable::decode(&with(stream)),
                Err(SightTrainerIntroSpeechError::MalformedString { position: 4, .. })
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
    let pack = AssetPack::load(&path).unwrap();
    let _ = std::fs::remove_file(path);
    pack
}

#[test]
fn accessor_valid_missing_wrong_kind_and_malformed() {
    let pack = load("stis-ok", &pack_with(RAW_KIND_TAG, canonical(&streams())));
    assert_eq!(
        pack.sight_trainer_intro_speeches()
            .unwrap()
            .speech("Route103_EventScript_Rhett"),
        Some(stream(8).as_slice())
    );

    let pack = load("stis-missing", &pack_bytes(vec![]));
    assert!(matches!(
        pack.sight_trainer_intro_speeches(),
        Err(PackError::UnknownAsset(_))
    ));

    let pack = load("stis-kind", &pack_with(PALETTE_KIND_TAG, vec![0; 4]));
    assert!(matches!(
        pack.sight_trainer_intro_speeches(),
        Err(PackError::WrongKind { .. })
    ));

    let mut bad = canonical(&streams());
    bad.truncate(bad.len() - 1);
    let pack = load("stis-bad", &pack_with(RAW_KIND_TAG, bad));
    assert!(matches!(
        pack.sight_trainer_intro_speeches(),
        Err(PackError::SightTrainerIntroSpeechDecode {
            source: SightTrainerIntroSpeechError::Truncated,
            ..
        })
    ));

    let mut open = streams();
    open[0] = vec![0xA1];
    let pack = load("stis-open", &pack_with(RAW_KIND_TAG, canonical(&open)));
    assert!(matches!(
        pack.sight_trainer_intro_speeches(),
        Err(PackError::SightTrainerIntroSpeechDecode {
            source: SightTrainerIntroSpeechError::MalformedString { position: 0, .. },
            ..
        })
    ));
}
