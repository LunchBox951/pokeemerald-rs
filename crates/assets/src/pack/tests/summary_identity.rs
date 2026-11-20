use super::super::{
    AssetPack, NatureId, PackError, SpecialLocationNames, SummaryIdentityError,
    SummaryIdentitySection, SummaryIdentityTable, MAPSEC_AQUA_HIDEOUT_OLD, MAPSEC_DYNAMIC,
    MAPSEC_SECRET_BASE, NATURE_COUNT, REGION_MAP_SECTION_COUNT, SPECIES_TO_NATIONAL_COUNT,
};
use super::shared::{pack_bytes, write_pack, Entry, PALETTE_KIND_TAG, RAW_KIND_TAG};
use crate::pokedex::{NationalDexNo, NATIONAL_DEX_LAST};
use crate::species::SpeciesId;

const TABLE_PACK_ID: &str = "summary-identity/table";
/// Magic, version, and three counts.
const HEADER: usize = 12;
/// The section whose name is a bare `EOS`, as `MAPSEC_DYNAMIC`'s is upstream.
const EMPTY_SECTION: usize = 87;

const FERRY: [u8; 3] = [0xC0, 0xC1, 0xFF];
const SECRET_BASE: [u8; 3] = [0xC2, 0xC3, 0xFF];
const HIDEOUT: [u8; 3] = [0xC4, 0xC5, 0xFF];

fn nature(id: usize) -> Vec<u8> {
    vec![0xBB, u8::try_from(id).unwrap(), 0xFF]
}

fn section(id: usize) -> Vec<u8> {
    if id == EMPTY_SECTION {
        vec![0xFF]
    } else {
        vec![0xD5, u8::try_from(id).unwrap(), 0xFF]
    }
}

/// A distinct dex identity per species, reaching the old-Unown range.
fn national(species: usize) -> u16 {
    u16::try_from(species % (usize::from(NATIONAL_DEX_LAST) + 1)).unwrap()
}

fn specials() -> SpecialLocationNames<'static> {
    SpecialLocationNames {
        ferry: &FERRY,
        secret_base: &SECRET_BASE,
        hideout: &HIDEOUT,
    }
}

fn push_named(out: &mut Vec<u8>, id: usize, stream: &[u8]) {
    out.extend_from_slice(&u16::try_from(id).unwrap().to_le_bytes());
    push_stream(out, stream);
}

fn push_stream(out: &mut Vec<u8>, stream: &[u8]) {
    out.extend_from_slice(&u32::try_from(stream.len()).unwrap().to_le_bytes());
    out.extend_from_slice(stream);
}

/// Hand-written so the tests do not rely on `encode` to check `encode`.
fn canonical() -> Vec<u8> {
    let mut out = b"SIDN".to_vec();
    out.extend_from_slice(&1u16.to_le_bytes());
    for count in [
        NATURE_COUNT,
        REGION_MAP_SECTION_COUNT,
        SPECIES_TO_NATIONAL_COUNT,
    ] {
        out.extend_from_slice(&u16::try_from(count).unwrap().to_le_bytes());
    }
    for id in 0..NATURE_COUNT {
        push_named(&mut out, id, &nature(id));
    }
    for id in 0..REGION_MAP_SECTION_COUNT {
        push_named(&mut out, id, &section(id));
    }
    for stream in [FERRY, SECRET_BASE, HIDEOUT] {
        push_stream(&mut out, &stream);
    }
    for species in 1..=SPECIES_TO_NATIONAL_COUNT {
        out.extend_from_slice(&u16::try_from(species).unwrap().to_le_bytes());
        out.extend_from_slice(&national(species).to_le_bytes());
    }
    out
}

struct Records {
    natures: Vec<(u16, Vec<u8>)>,
    sections: Vec<(u16, Vec<u8>)>,
    specials: [Vec<u8>; 3],
    conversions: Vec<(u16, u16)>,
}

fn records() -> Records {
    Records {
        natures: (0..NATURE_COUNT)
            .map(|id| (u16::try_from(id).unwrap(), nature(id)))
            .collect(),
        sections: (0..REGION_MAP_SECTION_COUNT)
            .map(|id| (u16::try_from(id).unwrap(), section(id)))
            .collect(),
        specials: [FERRY.to_vec(), SECRET_BASE.to_vec(), HIDEOUT.to_vec()],
        conversions: (1..=SPECIES_TO_NATIONAL_COUNT)
            .map(|species| (u16::try_from(species).unwrap(), national(species)))
            .collect(),
    }
}

fn build(mutate: impl FnOnce(&mut Records)) -> Result<Vec<u8>, SummaryIdentityError> {
    let mut records = records();
    mutate(&mut records);
    SummaryIdentityTable::from_entries(
        records.natures.iter().map(|(id, s)| (*id, s.as_slice())),
        records.sections.iter().map(|(id, s)| (*id, s.as_slice())),
        SpecialLocationNames {
            ferry: &records.specials[0],
            secret_base: &records.specials[1],
            hideout: &records.specials[2],
        },
        records.conversions.iter().copied(),
    )
    .map(|table| table.encode())
}

fn decode_err(bytes: &[u8]) -> SummaryIdentityError {
    SummaryIdentityTable::decode(bytes).expect_err("a faulty payload")
}

#[test]
fn counts_are_upstreams() {
    assert_eq!(NATURE_COUNT, 25);
    assert_eq!(REGION_MAP_SECTION_COUNT, 213);
    assert_eq!(SPECIES_TO_NATIONAL_COUNT, 411);
    assert_eq!(MAPSEC_AQUA_HIDEOUT_OLD, 66);
    assert_eq!(MAPSEC_SECRET_BASE, 86);
    assert_eq!(MAPSEC_DYNAMIC, 87);
}

#[test]
fn canonical_bytes_decode_and_reencode_identically() {
    let bytes = canonical();
    let table = SummaryIdentityTable::decode(&bytes).unwrap();
    assert_eq!(table.encode(), bytes);
    assert_eq!(build(|_| {}).unwrap(), bytes);
}

#[test]
fn from_entries_encodes_canonically_whatever_the_input_order() {
    let reversed = build(|r| {
        r.natures.reverse();
        r.sections.reverse();
        r.conversions.reverse();
    })
    .unwrap();
    assert_eq!(reversed, canonical());
}

#[test]
fn readers_return_each_table() {
    let bytes = canonical();
    let table = SummaryIdentityTable::decode(&bytes).unwrap();
    assert_eq!(table.nature_names().count(), NATURE_COUNT);
    assert_eq!(
        table.region_section_names().count(),
        REGION_MAP_SECTION_COUNT
    );
    assert_eq!(
        table.species_conversions().count(),
        SPECIES_TO_NATIONAL_COUNT
    );
    assert_eq!(table.special_location_names(), specials());
    assert_eq!(
        table.nature_name(NatureId::new(24).unwrap()),
        nature(24).as_slice()
    );
    assert_eq!(NatureId::new(25), None);
    assert_eq!(table.region_section_name(16), Some(section(16).as_slice()));
    assert_eq!(
        table.region_section_name(212),
        Some(section(212).as_slice())
    );
    assert_eq!(table.region_section_name(213), None);
    let (species, dex) = table.species_conversions().last().unwrap();
    assert_eq!(
        (species, dex),
        (SpeciesId(411), NationalDexNo::new(national(411)).unwrap())
    );
}

#[test]
fn location_names_apply_the_ferry_secret_base_and_hideout_rules() {
    let bytes = canonical();
    let table = SummaryIdentityTable::decode(&bytes).unwrap();
    // The raw field is untouched; the generic reader swaps the ferry and the
    // secret base; only the memo reader swaps the old Aqua hideout.
    for (id, raw, generic, memo) in [
        (
            MAPSEC_AQUA_HIDEOUT_OLD,
            Some(section(66)),
            Some(section(66)),
            Some(HIDEOUT.to_vec()),
        ),
        (
            MAPSEC_SECRET_BASE,
            Some(section(86)),
            Some(SECRET_BASE.to_vec()),
            Some(SECRET_BASE.to_vec()),
        ),
        (
            MAPSEC_DYNAMIC,
            Some(vec![0xFF]),
            Some(FERRY.to_vec()),
            Some(FERRY.to_vec()),
        ),
        (16, Some(section(16)), Some(section(16)), Some(section(16))),
        (213, None, None, None),
        (255, None, None, None),
    ] {
        assert_eq!(
            table.region_section_name(id).map(<[u8]>::to_vec),
            raw,
            "{id}"
        );
        assert_eq!(
            table.map_name_generic(id).map(<[u8]>::to_vec),
            generic,
            "{id}"
        );
        assert_eq!(
            table.met_location_name(id).map(<[u8]>::to_vec),
            memo,
            "{id}"
        );
    }
}

#[test]
fn species_conversion_keeps_the_offset_and_its_sentinels() {
    let bytes = canonical();
    let table = SummaryIdentityTable::decode(&bytes).unwrap();
    let dex = |species: u16| {
        table
            .species_to_national(SpeciesId(species))
            .map(NationalDexNo::get)
    };
    assert_eq!(
        dex(0),
        Some(0),
        "SPECIES_NONE has no slot and reads as none"
    );
    assert_eq!(dex(1), Some(national(1)));
    assert_eq!(dex(411), Some(national(411)));
    assert_eq!(dex(412), None, "SPECIES_EGG is past the table");
    assert_eq!(dex(u16::MAX), None);
}

#[test]
fn constructor_refuses_missing_duplicate_and_out_of_range_records() {
    type Section = SummaryIdentitySection;
    type Case = (
        &'static str,
        Box<dyn FnOnce(&mut Records)>,
        SummaryIdentityError,
    );
    let cases: [Case; 9] = [
        (
            "nature missing",
            Box::new(|r| {
                r.natures.pop();
            }),
            SummaryIdentityError::MissingId {
                section: Section::Nature,
                id: 24,
            },
        ),
        (
            "nature duplicate",
            Box::new(|r| r.natures[3].0 = 2),
            SummaryIdentityError::DuplicateId {
                section: Section::Nature,
                id: 2,
            },
        ),
        (
            "nature out of range",
            Box::new(|r| r.natures[3].0 = 25),
            SummaryIdentityError::IdOutOfRange {
                section: Section::Nature,
                id: 25,
            },
        ),
        (
            "section missing",
            Box::new(|r| {
                r.sections.remove(100);
            }),
            SummaryIdentityError::MissingId {
                section: Section::RegionSection,
                id: 100,
            },
        ),
        (
            "section out of range",
            Box::new(|r| r.sections[0].0 = 213),
            SummaryIdentityError::IdOutOfRange {
                section: Section::RegionSection,
                id: 213,
            },
        ),
        (
            "species zero has no slot",
            Box::new(|r| r.conversions[0].0 = 0),
            SummaryIdentityError::IdOutOfRange {
                section: Section::SpeciesConversion,
                id: 0,
            },
        ),
        (
            "species past the table",
            Box::new(|r| r.conversions[410].0 = 412),
            SummaryIdentityError::IdOutOfRange {
                section: Section::SpeciesConversion,
                id: 412,
            },
        ),
        (
            "species missing",
            Box::new(|r| {
                r.conversions.remove(0);
            }),
            SummaryIdentityError::MissingId {
                section: Section::SpeciesConversion,
                id: 1,
            },
        ),
        (
            "undefined dex identity",
            Box::new(|r| r.conversions[4].1 = NATIONAL_DEX_LAST + 1),
            SummaryIdentityError::InvalidNationalDexNo {
                species: 5,
                national: NATIONAL_DEX_LAST + 1,
            },
        ),
    ];
    for (name, mutate, expected) in cases {
        assert_eq!(build(mutate), Err(expected), "{name}");
    }
}

#[test]
fn text_records_are_walked_by_token() {
    // A control argument of 0xFF is not a terminator.
    assert!(build(|r| r.sections[5].1 = vec![0xFD, 0xFF, 0xFF]).is_ok());
    for (name, stream) in [
        ("empty", vec![]),
        ("unterminated", vec![0xBB]),
        ("bytes after EOS", vec![0xFF, 0x00]),
        ("cut control", vec![0xFD, 0xFF]),
    ] {
        for place in 0..3 {
            let result = build(|r| match place {
                0 => r.natures[0].1 = stream.clone(),
                1 => r.sections[0].1 = stream.clone(),
                _ => r.specials[2] = stream.clone(),
            });
            assert!(
                matches!(result, Err(SummaryIdentityError::MalformedString { .. })),
                "{name} in table {place}: {result:?}"
            );
        }
    }
    assert_eq!(
        build(|r| r.specials[1] = vec![0xBB]),
        Err(SummaryIdentityError::MalformedString {
            section: SummaryIdentitySection::Special,
            id: 1,
            detail: "missing terminator",
        })
    );
}

#[test]
fn every_truncated_prefix_is_refused() {
    let good = canonical();
    for len in (0..good.len()).step_by(7).chain([good.len() - 1]) {
        assert!(
            SummaryIdentityTable::decode(&good[..len]).is_err(),
            "prefix of {len} bytes"
        );
    }
}

#[test]
fn header_and_record_faults_are_refused() {
    let good = canonical();
    let mut bad = good.clone();
    bad[0] = b'X';
    assert_eq!(decode_err(&bad), SummaryIdentityError::BadMagic);
    let mut bad = good.clone();
    bad[4] = 2;
    assert_eq!(
        decode_err(&bad),
        SummaryIdentityError::UnsupportedVersion(2)
    );
    for (offset, section) in [
        (6, SummaryIdentitySection::Nature),
        (8, SummaryIdentitySection::RegionSection),
        (10, SummaryIdentitySection::SpeciesConversion),
    ] {
        let mut bad = good.clone();
        bad[offset] ^= 1;
        assert!(
            matches!(
                decode_err(&bad),
                SummaryIdentityError::HeaderMismatch { section: s, .. } if s == section
            ),
            "{section}"
        );
    }
    let mut bad = good.clone();
    bad.push(0);
    assert_eq!(decode_err(&bad), SummaryIdentityError::TrailingBytes);
    // A record id that repeats its neighbour.
    let mut bad = good;
    bad[HEADER + 9] = 0;
    assert_eq!(
        decode_err(&bad),
        SummaryIdentityError::DuplicateId {
            section: SummaryIdentitySection::Nature,
            id: 0
        }
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
    let pack = load("sidn-ok", &pack_with(RAW_KIND_TAG, canonical()));
    assert_eq!(
        pack.summary_identity()
            .unwrap()
            .nature_name(NatureId::new(0).unwrap()),
        nature(0).as_slice()
    );

    let pack = load("sidn-missing", &pack_bytes(vec![]));
    assert!(matches!(
        pack.summary_identity(),
        Err(PackError::UnknownAsset(_))
    ));

    let pack = load("sidn-kind", &pack_with(PALETTE_KIND_TAG, vec![0; 4]));
    assert!(matches!(
        pack.summary_identity(),
        Err(PackError::WrongKind { .. })
    ));

    let mut bad = canonical();
    bad.truncate(bad.len() - 1);
    let pack = load("sidn-bad", &pack_with(RAW_KIND_TAG, bad));
    assert!(matches!(
        pack.summary_identity(),
        Err(PackError::SummaryIdentityDecode {
            source: SummaryIdentityError::Truncated,
            ..
        })
    ));
}

/// `charmap.txt` bytes for the plain characters the sampled upstream names use.
fn charmap_text(text: &str) -> Vec<u8> {
    let mut bytes: Vec<u8> = text
        .chars()
        .map(|c| match c {
            ' ' => 0x00,
            '0'..='9' => 0xA1 + (c as u8 - b'0'),
            'A'..='Z' => 0xBB + (c as u8 - b'A'),
            other => panic!("unmapped {other:?}"),
        })
        .collect();
    bytes.push(0xFF);
    bytes
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_summary_identity_table_has_upstreams_lengths_and_records() {
    let pack = AssetPack::load_repo().expect("run `cargo xtask extract` first");
    let table = pack.summary_identity().expect("the summary-identity table");

    // `NUM_NATURES`, `MAPSEC_NONE`, and `NUM_SPECIES - 1`.
    assert_eq!(table.nature_names().count(), 25);
    assert_eq!(table.region_section_names().count(), 213);
    assert_eq!(table.species_conversions().count(), 411);

    for (nature, name) in [
        (0, "HARDY"),
        (3, "ADAMANT"),
        (9, "LAX"),
        (18, "BASHFUL"),
        (24, "QUIRKY"),
    ] {
        assert_eq!(
            table.nature_name(NatureId::new(nature).unwrap()),
            charmap_text(name),
            "nature {nature}"
        );
    }

    for (id, name) in [
        (0, "LITTLEROOT TOWN"),
        (16, "ROUTE 101"),
        (86, "SECRET BASE"),
        (197, "AQUA HIDEOUT"),
        (212, "TRAINER HILL"),
    ] {
        assert_eq!(
            table.region_section_name(id),
            Some(charmap_text(name).as_slice()),
            "section {id}"
        );
    }
    // The Aqua Hideout rule: the old section is `{AQUA} HIDEOUT` (the AQUA
    // placeholder, 0xFD 0x08) in the table and a plain `HIDEOUT` in the memo.
    let mut old_hideout = vec![0xFD, 0x08, 0x00];
    old_hideout.extend(charmap_text("HIDEOUT"));
    assert_eq!(
        table.region_section_name(MAPSEC_AQUA_HIDEOUT_OLD),
        Some(old_hideout.as_slice())
    );
    assert_eq!(
        table.met_location_name(MAPSEC_AQUA_HIDEOUT_OLD),
        Some(charmap_text("HIDEOUT").as_slice())
    );
    assert_eq!(
        table.map_name_generic(MAPSEC_AQUA_HIDEOUT_OLD),
        Some(old_hideout.as_slice())
    );
    assert_eq!(
        table.region_section_name(MAPSEC_DYNAMIC),
        Some([0xFF].as_slice())
    );
    assert_eq!(
        table.map_name_generic(MAPSEC_DYNAMIC),
        Some(charmap_text("FERRY").as_slice())
    );
    assert_eq!(
        table.met_location_name(MAPSEC_SECRET_BASE),
        Some(charmap_text("SECRET BASE").as_slice())
    );
    assert_eq!(table.met_location_name(213), None);

    // 1 -> 1, 251 -> 251, the old-Unown slots 252..=276 -> 387..=411, then
    // TREECKO (277) -> 252, DEOXYS (410) -> 386 and CHIMECHO (411) -> 358.
    let dex = |species: u16| table.species_to_national(SpeciesId(species)).unwrap().get();
    assert_eq!(dex(0), 0);
    assert_eq!(dex(1), 1);
    assert_eq!(dex(251), 251);
    assert_eq!(dex(252), 387);
    assert_eq!(dex(276), 411);
    assert_eq!(dex(277), 252);
    assert_eq!(dex(410), 386);
    assert_eq!(dex(411), 358);
    assert_eq!(table.species_to_national(SpeciesId(412)), None);
}
