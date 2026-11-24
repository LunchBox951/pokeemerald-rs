use super::super::{AssetPack, PackError, TrainerDefeatSpeechError};
use super::shared::{
    pack_bytes, write_pack, write_synthetic_pack, Entry, PALETTE_KIND_TAG, RAW_KIND_TAG,
};
use crate::{TrainerDefeatSpeechContext, TrainerDefeatSpeechTable, TRAINER_DEFEAT_SPEECH_COUNT};

const TABLE_PACK_ID: &str = "trainer-defeat-speech/table";

/// A distinct stream per ID, so a lookup that returns the wrong record fails.
fn stream(id: u16) -> Vec<u8> {
    vec![0xA1 + u8::try_from(id).unwrap(), 0xFF]
}

fn streams() -> Vec<Vec<u8>> {
    (0..14).map(stream).collect()
}

/// Hand-written so the tests do not rely on `encode` to check `encode`.
fn canonical(streams: &[Vec<u8>]) -> Vec<u8> {
    let mut out = b"TDFS".to_vec();
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&14u16.to_le_bytes());
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

#[test]
fn inventory_is_the_route_103_defeat_labels_in_id_order() {
    use TrainerDefeatSpeechContext as C;
    assert_eq!(TRAINER_DEFEAT_SPEECH_COUNT, 14);
    let expected = [
        (C::Route103Daisy, "Route103_Text_DaisyDefeated"),
        (C::Route103Amy, "Route103_Text_AmyDefeated"),
        (C::Route103Liv, "Route103_Text_LivDefeated"),
        (C::Route103AmyRematch, "Route103_Text_AmyRematchDefeated"),
        (C::Route103LivRematch, "Route103_Text_LivRematchDefeated"),
        (C::Route103Andrew, "Route103_Text_AndrewDefeated"),
        (C::Route103Miguel, "Route103_Text_MiguelDefeated"),
        (
            C::Route103MiguelRematch,
            "Route103_Text_MiguelRematchDefeated",
        ),
        (C::Route103Pete, "Route103_Text_PeteDefeated"),
        (C::Route103Isabelle, "Route103_Text_IsabelleDefeated"),
        (C::Route103Rhett, "Route103_Text_RhettDefeated"),
        (C::Route103Marcos, "Route103_Text_MarcosDefeated"),
        (C::Route103May, "Route103_Text_MayDefeated"),
        (C::Route103Brendan, "Route103_Text_BrendanDefeated"),
    ];
    assert_eq!(C::ALL.len(), expected.len());
    for (id, (context, symbol)) in expected.into_iter().enumerate() {
        assert_eq!(C::ALL[id], context);
        assert_eq!(usize::from(context.id()), id);
        assert_eq!(context.symbol(), symbol);
    }
}

#[test]
fn canonical_bytes_decode_and_reencode_identically() {
    let streams = streams();
    let bytes = canonical(&streams);
    assert_eq!(bytes.len(), 8 + 14 * (2 + 4 + 2));
    let table = TrainerDefeatSpeechTable::decode(&bytes).unwrap();
    assert_eq!(table.encode(), bytes);
    for context in TrainerDefeatSpeechContext::ALL {
        assert_eq!(table.speech(context), streams[usize::from(context.id())]);
    }
}

#[test]
fn the_four_shared_trainer_contexts_and_both_rivals_stay_distinct() {
    use TrainerDefeatSpeechContext as C;
    let bytes = canonical(&streams());
    let table = TrainerDefeatSpeechTable::decode(&bytes).unwrap();
    let shared_trainer = [
        C::Route103Amy,
        C::Route103Liv,
        C::Route103AmyRematch,
        C::Route103LivRematch,
    ];
    for (i, a) in shared_trainer.iter().enumerate() {
        for b in &shared_trainer[i + 1..] {
            assert_ne!(table.speech(*a), table.speech(*b), "{a:?} {b:?}");
        }
    }
    assert_ne!(
        table.speech(C::Route103May),
        table.speech(C::Route103Brendan)
    );
}

#[test]
fn from_entries_encodes_canonically_in_any_order() {
    let streams = streams();
    let mut items = entries(&streams);
    items.reverse();
    let table = TrainerDefeatSpeechTable::from_entries(items).unwrap();
    assert_eq!(table.encode(), canonical(&streams));
}

#[test]
fn constructor_refuses_missing_duplicate_and_unknown_ids() {
    let streams = streams();
    let mut items = entries(&streams);
    let dropped = items.remove(5);
    assert_eq!(
        TrainerDefeatSpeechTable::from_entries(items.clone()),
        Err(TrainerDefeatSpeechError::MissingId { id: 5 })
    );
    items.push((3, streams[3].as_slice()));
    assert_eq!(
        TrainerDefeatSpeechTable::from_entries(items.clone()),
        Err(TrainerDefeatSpeechError::DuplicateId { id: 3 })
    );
    items.pop();
    items.push((14, dropped.1));
    assert_eq!(
        TrainerDefeatSpeechTable::from_entries(items),
        Err(TrainerDefeatSpeechError::IdOutOfRange { id: 14 })
    );
}

#[test]
fn every_truncated_prefix_is_refused() {
    let bytes = canonical(&streams());
    for len in 0..bytes.len() {
        assert_eq!(
            TrainerDefeatSpeechTable::decode(&bytes[..len]),
            Err(TrainerDefeatSpeechError::Truncated),
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
        TrainerDefeatSpeechTable::decode(&bad),
        Err(TrainerDefeatSpeechError::BadMagic)
    );
    let mut bad = good.clone();
    bad[4] = 2;
    assert_eq!(
        TrainerDefeatSpeechTable::decode(&bad),
        Err(TrainerDefeatSpeechError::UnsupportedVersion(2))
    );
    let mut bad = good.clone();
    bad[6] = 13;
    assert_eq!(
        TrainerDefeatSpeechTable::decode(&bad),
        Err(TrainerDefeatSpeechError::HeaderMismatch { found: 13 })
    );
    let mut bad = good.clone();
    bad[8] = 14;
    assert_eq!(
        TrainerDefeatSpeechTable::decode(&bad),
        Err(TrainerDefeatSpeechError::IdOutOfRange { id: 14 })
    );
    let mut bad = good.clone();
    bad[8] = 1;
    assert_eq!(
        TrainerDefeatSpeechTable::decode(&bad),
        Err(TrainerDefeatSpeechError::DuplicateId { id: 1 })
    );
    let mut bad = good.clone();
    bad[10..14].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(
        TrainerDefeatSpeechTable::decode(&bad),
        Err(TrainerDefeatSpeechError::Truncated)
    );
    let mut bad = good;
    bad.push(0);
    assert_eq!(
        TrainerDefeatSpeechTable::decode(&bad),
        Err(TrainerDefeatSpeechError::TrailingBytes)
    );
}

#[test]
fn token_streams_are_walked_by_token() {
    let with = |stream: &[u8]| {
        let mut streams = streams();
        streams[12] = stream.to_vec();
        canonical(&streams)
    };
    // `{PLAYER}{KUN}` stay control sequences (`FD 01`, `FD 05`).
    let ok: &[&[u8]] = &[
        &[0xFF],
        &[0xFD, 0x01, 0xFD, 0x05, 0xFF],
        &[0xFC, 0x08, 0x20, 0xFF],
    ];
    for stream in ok {
        TrainerDefeatSpeechTable::decode(&with(stream))
            .unwrap_or_else(|e| panic!("{stream:02X?}: {e}"));
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
                TrainerDefeatSpeechTable::decode(&with(stream)),
                Err(TrainerDefeatSpeechError::MalformedString { id: 12, .. })
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
    let pack = load("tdfs-ok", &pack_with(RAW_KIND_TAG, canonical(&streams())));
    assert_eq!(
        pack.trainer_defeat_speeches()
            .unwrap()
            .speech(TrainerDefeatSpeechContext::Route103Brendan),
        stream(13)
    );

    let pack = load("tdfs-missing", &pack_bytes(vec![]));
    assert!(matches!(
        pack.trainer_defeat_speeches(),
        Err(PackError::UnknownAsset(_))
    ));

    let pack = load("tdfs-kind", &pack_with(PALETTE_KIND_TAG, vec![0; 4]));
    assert!(matches!(
        pack.trainer_defeat_speeches(),
        Err(PackError::WrongKind { .. })
    ));

    let mut bad = canonical(&streams());
    bad.truncate(bad.len() - 1);
    let pack = load("tdfs-bad", &pack_with(RAW_KIND_TAG, bad));
    assert!(matches!(
        pack.trainer_defeat_speeches(),
        Err(PackError::TrainerDefeatSpeechDecode {
            source: TrainerDefeatSpeechError::Truncated,
            ..
        })
    ));

    let mut open = streams();
    open[0] = vec![0xA1];
    let pack = load("tdfs-open", &pack_with(RAW_KIND_TAG, canonical(&open)));
    assert!(matches!(
        pack.trainer_defeat_speeches(),
        Err(PackError::TrainerDefeatSpeechDecode {
            source: TrainerDefeatSpeechError::MalformedString { id: 0, .. },
            ..
        })
    ));
}

#[test]
fn a_pack_from_the_format_without_defeat_speeches_is_refused_on_load() {
    const PREVIOUS_FORMAT_VERSION: u32 = 13;
    let path = write_synthetic_pack("trainer-defeat-speech-previous-version");
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[8..12].copy_from_slice(&PREVIOUS_FORMAT_VERSION.to_le_bytes());
    std::fs::write(&path, bytes).unwrap();
    assert_eq!(
        AssetPack::load(&path).map(|_| ()),
        Err(PackError::UnsupportedVersion(PREVIOUS_FORMAT_VERSION))
    );
    let _ = std::fs::remove_file(path);
}
