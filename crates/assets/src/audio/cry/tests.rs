use super::*;

const FIRST_ID_BYTE: usize = size_of::<u16>();

fn sample_id(position: usize) -> SampleId {
    SampleId(format!("{DIRECT_SOUND_ID_PREFIX}cry_{position}"))
}

fn table() -> CryTable {
    CryTable::new((0..CRY_TABLE_LEN).map(sample_id).collect()).unwrap()
}

fn cry(species: SpeciesId) -> Option<u16> {
    cry_id(species).map(CryId::index)
}

#[test]
fn a_table_round_trips() {
    let table = table();
    assert_eq!(CryTable::decode(&table.encode()).unwrap(), table);
}

#[test]
fn the_wire_format_is_length_prefixed_ids_in_table_order() {
    let bytes = table().encode();
    let first = sample_id(0).0;
    assert_eq!(
        u16::from_le_bytes([bytes[0], bytes[1]]),
        u16::try_from(first.len()).unwrap()
    );
    assert_eq!(
        &bytes[FIRST_ID_BYTE..FIRST_ID_BYTE + first.len()],
        first.as_bytes()
    );
}

#[test]
fn a_table_with_the_wrong_length_is_rejected() {
    for len in [0, CRY_TABLE_LEN - 1, CRY_TABLE_LEN + 1] {
        let samples = (0..len).map(sample_id).collect();
        assert_eq!(CryTable::new(samples), Err(AudioError::CryTableLength(len)));
    }
}

#[test]
fn a_sample_outside_the_direct_sound_family_is_rejected() {
    let mut samples: Vec<_> = (0..CRY_TABLE_LEN).map(sample_id).collect();
    samples[7] = SampleId("audio/sample/programmable-wave/01".to_owned());
    assert_eq!(
        CryTable::new(samples),
        Err(AudioError::CrySampleNotDirectSound { position: 7 })
    );
}

#[test]
fn an_over_long_id_is_rejected() {
    let mut samples: Vec<_> = (0..CRY_TABLE_LEN).map(sample_id).collect();
    let long = format!(
        "{DIRECT_SOUND_ID_PREFIX}{}",
        "a".repeat(usize::from(u16::MAX))
    );
    let len = long.len();
    samples[3] = SampleId(long);
    assert_eq!(CryTable::new(samples), Err(AudioError::IdTooLong(len)));
}

#[test]
fn decode_rejects_a_foreign_sample_id() {
    let mut samples: Vec<_> = (0..CRY_TABLE_LEN).map(sample_id).collect();
    samples[0] = SampleId("audio/song/mus_title".to_owned());
    let mut writer = Writer::new();
    for sample in &samples {
        writer.string(&sample.0);
    }
    assert_eq!(
        CryTable::decode(&writer.into_bytes()),
        Err(AudioError::CrySampleNotDirectSound { position: 0 })
    );
}

#[test]
fn decode_rejects_every_truncation() {
    let bytes = table().encode();
    for cut in [0, 1, FIRST_ID_BYTE, bytes.len() / 2, bytes.len() - 1] {
        assert_eq!(
            CryTable::decode(&bytes[..cut]),
            Err(AudioError::Truncated),
            "cut at {cut}"
        );
    }
}

#[test]
fn decode_rejects_trailing_bytes() {
    let mut bytes = table().encode();
    bytes.extend_from_slice(&[0, 0, 9]);
    assert_eq!(CryTable::decode(&bytes), Err(AudioError::TrailingBytes(3)));
}

#[test]
fn decode_rejects_a_non_utf8_id() {
    let mut bytes = table().encode();
    bytes[FIRST_ID_BYTE] = 0xFF;
    assert_eq!(CryTable::decode(&bytes), Err(AudioError::InvalidString));
}

#[test]
fn ordinary_species_use_their_own_zero_based_position() {
    assert_eq!(cry(SpeciesId(1)), Some(0));
    assert_eq!(cry(SpeciesId::UNOWN), Some(200));
    assert_eq!(cry(SpeciesId::CELEBI), Some(250));
}

#[test]
fn the_placeholder_species_between_celebi_and_treecko_share_unowns_cry() {
    assert_eq!(cry(SpeciesId::OLD_UNOWN_B), Some(200));
    assert_eq!(cry(SpeciesId(SpeciesId::TREECKO.index() - 1)), Some(200));
}

#[test]
fn hoenn_species_use_the_remap_table() {
    assert_eq!(cry(SpeciesId::TREECKO), Some(273));
    assert_eq!(cry(SpeciesId::TORCHIC), Some(270));
    assert_eq!(cry(SpeciesId::MUDKIP), Some(276));
    assert_eq!(cry(SpeciesId::ZIGZAGOON), Some(378));
    assert_eq!(cry(SpeciesId::DEOXYS), Some(386));
    assert_eq!(cry(SpeciesId::CHIMECHO), Some(387));
}

#[test]
fn invalid_species_have_no_cry() {
    assert_eq!(cry(SpeciesId::NONE), None);
    assert_eq!(cry(SpeciesId(SpeciesId::CHIMECHO.index() + 1)), None);
    assert_eq!(cry(SpeciesId(u16::MAX)), None);
}

#[test]
fn every_valid_species_resolves_inside_the_table() {
    for index in 1..=SpeciesId::CHIMECHO.index() {
        let cry = cry_id(SpeciesId(index)).unwrap();
        assert!(usize::from(cry.index()) < CRY_TABLE_LEN, "species {index}");
    }
}

#[test]
fn normal_mode_keeps_the_caller_settings() {
    assert_eq!(
        CryMode::Normal.settings(-5, CRY_VOLUME, CRY_PRIORITY_NORMAL),
        CrySettings {
            direct_sound_mode: DirectSoundMode::Resampled,
            pan: -5,
            volume: CRY_VOLUME,
            pitch: 15360,
            length: 140,
            release: 0,
            chorus: 0,
            priority: CRY_PRIORITY_NORMAL,
        }
    );
}

#[test]
fn echo_start_plays_reversed_with_its_overrides() {
    let settings = CryMode::EchoStart.settings(0, CRY_VOLUME, CRY_PRIORITY_NORMAL);
    assert!(settings.is_reverse());
    assert_eq!(
        settings,
        CrySettings {
            direct_sound_mode: DirectSoundMode::Reverse,
            pan: 0,
            volume: 90,
            pitch: 15600,
            length: 25,
            release: 100,
            chorus: 192,
            priority: CRY_PRIORITY_NORMAL,
        }
    );
}

#[test]
fn growl_start_plays_reversed_and_keeps_the_caller_volume() {
    let settings = CryMode::Growl1.settings(3, 100, CRY_PRIORITY_AMBIENT);
    assert!(settings.is_reverse());
    assert_eq!(
        settings,
        CrySettings {
            direct_sound_mode: DirectSoundMode::Reverse,
            pan: 3,
            volume: 100,
            pitch: 15200,
            length: 15,
            release: 125,
            chorus: 0,
            priority: CRY_PRIORITY_AMBIENT,
        }
    );
}

#[test]
fn only_echo_start_and_growl_start_are_reversed() {
    let modes = [
        (CryMode::Normal, false),
        (CryMode::Doubles, false),
        (CryMode::Encounter, false),
        (CryMode::HighPitch, false),
        (CryMode::EchoStart, true),
        (CryMode::Faint, false),
        (CryMode::EchoEnd, false),
        (CryMode::Roar1, false),
        (CryMode::Roar2, false),
        (CryMode::Growl1, true),
        (CryMode::Growl2, false),
        (CryMode::Weak, false),
        (CryMode::WeakDoubles, false),
    ];
    for (mode, reversed) in modes {
        let settings = mode.settings(0, CRY_VOLUME, CRY_PRIORITY_NORMAL);
        assert_eq!(settings.is_reverse(), reversed, "{mode:?}");
    }
}

#[test]
fn weak_doubles_shortens_the_weak_cry() {
    let weak = CryMode::Weak.settings(0, CRY_VOLUME, CRY_PRIORITY_NORMAL);
    let doubles = CryMode::WeakDoubles.settings(0, CRY_VOLUME, CRY_PRIORITY_NORMAL);
    assert_eq!((weak.pitch, weak.length, weak.release), (15000, 140, 0));
    assert_eq!(
        (doubles.pitch, doubles.length, doubles.release),
        (15000, 20, 225)
    );
}

#[test]
fn resolve_pairs_the_species_sample_with_the_mode_settings() {
    let table = table();
    let resolved = table
        .resolve(
            SpeciesId::TREECKO,
            CryMode::EchoStart,
            0,
            CRY_VOLUME,
            CRY_PRIORITY_NORMAL,
        )
        .unwrap();
    assert_eq!(resolved.sample, &sample_id(273));
    assert!(resolved.settings.is_reverse());
    assert_eq!(
        table.resolve(
            SpeciesId::NONE,
            CryMode::Normal,
            0,
            CRY_VOLUME,
            CRY_PRIORITY_NORMAL
        ),
        None
    );
}

#[test]
fn every_mode_applies_its_upstream_settings() {
    // (mode, length, release, pitch, chorus, volume); volume is the caller's 100 unless overridden.
    let expected = [
        (CryMode::Normal, 140, 0, 15360, 0, 100),
        (CryMode::Doubles, 20, 225, 15360, 0, 100),
        (CryMode::Encounter, 140, 225, 15600, 20, 90),
        (CryMode::HighPitch, 50, 200, 15800, 20, 90),
        (CryMode::EchoStart, 25, 100, 15600, 192, 90),
        (CryMode::Faint, 140, 200, 14440, 0, 100),
        (CryMode::EchoEnd, 140, 220, 15555, 192, 70),
        (CryMode::Roar1, 10, 100, 14848, 0, 100),
        (CryMode::Roar2, 60, 225, 15616, 0, 100),
        (CryMode::Growl1, 15, 125, 15200, 0, 100),
        (CryMode::Growl2, 100, 225, 15200, 0, 100),
        (CryMode::Weak, 140, 0, 15000, 0, 100),
        (CryMode::WeakDoubles, 20, 225, 15000, 0, 100),
    ];
    for (mode, length, release, pitch, chorus, volume) in expected {
        let settings = mode.settings(-7, 100, CRY_PRIORITY_AMBIENT);
        assert_eq!(
            (
                settings.length,
                settings.release,
                settings.pitch,
                settings.chorus,
                settings.volume
            ),
            (length, release, pitch, chorus, volume),
            "{mode:?}"
        );
        assert_eq!(
            (settings.pan, settings.priority),
            (-7, CRY_PRIORITY_AMBIENT)
        );
    }
}
