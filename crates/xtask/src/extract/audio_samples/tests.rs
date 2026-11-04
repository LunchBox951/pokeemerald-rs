use super::{
    encode_direct_sound, encode_programmable_wave, extract_audio_samples, read_file, wav,
    ExtractError, SampleSource, WavSample, PROGRAMMABLE_WAVE_SIZE,
};
use assets::{AudioError, DirectSoundSample, ProgrammableWave, Sample};
use pack_format::PackWriter;
use std::collections::BTreeSet;
use std::fmt::Write as _;

/// The `mus_title` closure's samples, pinned independently of the closure resolver so a
/// dropped one fails the real-checkout test below.
const TITLE_DIRECT_SOUND_SAMPLES: [&str; 41] = [
    "classical_choir_voice_ahhs",
    "ethnic_flavours_hyoushigi",
    "ethnic_flavours_ohtsuzumi",
    "sc88pro_flute",
    "sc88pro_french_horn_60",
    "sc88pro_french_horn_72",
    "sc88pro_glockenspiel",
    "sc88pro_harp",
    "sc88pro_mute_high_conga",
    "sc88pro_open_low_conga",
    "sc88pro_orchestra_cymbal_crash",
    "sc88pro_orchestra_snare",
    "sc88pro_piano1_48",
    "sc88pro_piano1_60",
    "sc88pro_piano1_72",
    "sc88pro_piano1_84",
    "sc88pro_rnd_kick",
    "sc88pro_rnd_snare",
    "sc88pro_string_ensemble_60",
    "sc88pro_string_ensemble_72",
    "sc88pro_string_ensemble_84",
    "sc88pro_tambourine",
    "sc88pro_timpani",
    "sc88pro_tr909_hand_clap",
    "sc88pro_trumpet_60",
    "sc88pro_trumpet_72",
    "sc88pro_trumpet_84",
    "sc88pro_tuba_39",
    "sc88pro_tuba_51",
    "sc88pro_tubular_bell",
    "sc88pro_xylophone",
    "sd90_classical_oboe",
    "trinity_cymbal_crash",
    "unknown_bell",
    "unknown_close_hihat",
    "unknown_open_hihat",
    "unknown_snare",
    "unused_guitar_separates_power_chord",
    "unused_sc55_tom",
    "unused_sc88pro_unison_slap",
    "unused_sd90_oboe",
];

const TITLE_PROGRAMMABLE_WAVE_SAMPLES: [u32; 5] = [1, 2, 3, 5, 6];

fn wav_sample(
    base_frequency: u32,
    loop_start: Option<u32>,
    sample_count: u32,
    data: &[i8],
) -> WavSample {
    WavSample {
        base_frequency,
        loop_start,
        sample_count,
        data: data.to_vec(),
    }
}

const FLUTE_BASE_FREQUENCY: u32 = 3_425_024;
const FLUTE_LOOP_START: u32 = 1_312;
const FLUTE_SAMPLE_COUNT: u32 = 1_874;
const PROGRAMMABLE_WAVE_01: [u8; PROGRAMMABLE_WAVE_SIZE] = [
    0x01, 0x25, 0x8a, 0xde, 0xfe, 0xc9, 0x63, 0x10, 0x01, 0x25, 0x8a, 0xde, 0xfe, 0xc9, 0x63, 0x10,
];

fn assert_pack_contains(bytes: &[u8], expected: &[u8], description: &str) {
    assert!(
        bytes
            .windows(expected.len())
            .any(|window| window == expected),
        "pack does not contain {description}"
    );
}

fn ids(list: &[&str]) -> BTreeSet<String> {
    list.iter().map(|id| (*id).to_owned()).collect()
}

/// A checkout holding only the sample definitions `define` lists: `(label, incbin path)`
/// pairs for each of `direct_sound_data.inc` and `programmable_wave_data.inc`.
fn scratch_checkout(
    name: &str,
    direct_sound: &[(&str, &str)],
    programmable_wave: &[(&str, &str)],
) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "pokeemerald-rs-audio-samples-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("sound/direct_sound_samples")).unwrap();
    std::fs::create_dir_all(root.join("sound/programmable_wave_samples")).unwrap();
    let definitions = |entries: &[(&str, &str)]| {
        let mut text = String::new();
        for (label, path) in entries {
            let _ = write!(text, "\t.align 2\n{label}::\n\t.incbin \"{path}\"\n\n");
        }
        text
    };
    std::fs::write(
        root.join("sound/direct_sound_data.inc"),
        definitions(direct_sound),
    )
    .unwrap();
    std::fs::write(
        root.join("sound/programmable_wave_data.inc"),
        definitions(programmable_wave),
    )
    .unwrap();
    root
}

#[test]
#[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
fn the_voicegroup_closure_samples_are_extracted() {
    use super::super::{repo_root, upstream_present, voicegroups};

    assert!(upstream_present(), "run ./init.sh first");
    let upstream = repo_root().join("pokeemerald");
    let mut writer = PackWriter::new();
    let sample_ids = voicegroups::extract_voicegroups(&upstream, &mut writer)
        .expect("the voicegroup closure should resolve against a real checkout");
    let voicegroup_count = writer.len();
    extract_audio_samples(&upstream, &sample_ids, &mut writer)
        .expect("every referenced sample should extract");
    assert_eq!(writer.len(), voicegroup_count + sample_ids.len());
    let pack = writer.finish().unwrap();

    for id in &sample_ids {
        assert_pack_contains(&pack, id.as_bytes(), &format!("entry id `{id}`"));
    }
    for name in TITLE_DIRECT_SOUND_SAMPLES {
        let id = format!("audio/sample/direct-sound/{name}");
        assert!(sample_ids.contains(&id), "the closure dropped `{id}`");
    }
    for number in TITLE_PROGRAMMABLE_WAVE_SAMPLES {
        let id = format!("audio/sample/programmable-wave/{number:02}");
        assert!(sample_ids.contains(&id), "the closure dropped `{id}`");
    }

    let flute_path = upstream.join("sound/direct_sound_samples/sc88pro_flute.wav");
    let flute_source =
        read_file(&flute_path).expect("the flute sample source exists in a real checkout");
    let flute = wav::decode(&flute_source).expect("the flute sample source should decode");
    assert_eq!(flute.base_frequency, FLUTE_BASE_FREQUENCY);
    assert_eq!(flute.loop_start, Some(FLUTE_LOOP_START));
    assert_eq!(flute.sample_count, FLUTE_SAMPLE_COUNT);
    // `data` retains one interpolation-guard sample past `sample_count`:
    // flute's raw source has one more encoded sample than its `agbl`
    // override.
    assert_eq!(
        flute.data.len(),
        usize::try_from(FLUTE_SAMPLE_COUNT + 1).unwrap()
    );
    let flute_payload = encode_direct_sound(flute).expect("the flute sample is inside the schema");
    assert_pack_contains(&pack, &flute_payload, "the flute sample's encoded payload");

    let wave_path = upstream.join("sound/programmable_wave_samples/01.pcm");
    let wave_source =
        read_file(&wave_path).expect("programmable-wave 01 exists in a real checkout");
    assert_eq!(wave_source, PROGRAMMABLE_WAVE_01);
    let wave_payload = encode_programmable_wave(PROGRAMMABLE_WAVE_01);
    assert_pack_contains(
        &pack,
        &wave_payload,
        "programmable-wave 01's encoded payload",
    );
}

#[test]
fn canonical_sample_ids_name_their_source() {
    assert!(matches!(
        SampleSource::from_id("audio/sample/direct-sound/sc88pro_flute"),
        Some(SampleSource::DirectSound("sc88pro_flute"))
    ));
    assert!(matches!(
        SampleSource::from_id("audio/sample/programmable-wave/06"),
        Some(SampleSource::ProgrammableWave(6))
    ));
}

#[test]
fn non_canonical_sample_ids_name_no_source() {
    for id in [
        "audio/sample/direct-sound/",
        "audio/sample/direct-sound/cry/kyogre",
        "audio/sample/direct-sound/../escape",
        "audio/sample/direct-sound/Upper",
        "audio/sample/programmable-wave/",
        "audio/sample/programmable-wave/6",
        "audio/sample/programmable-wave/006",
        "audio/sample/programmable-wave/0x",
        "audio/voicegroup/title",
    ] {
        assert!(SampleSource::from_id(id).is_none(), "{id}");
    }
}

const WAVE_07: (&str, &str) = (
    "ProgrammableWaveData_7",
    "sound/programmable_wave_samples/07.pcm",
);

#[test]
fn extraction_emits_exactly_the_requested_samples() {
    let root = scratch_checkout(
        "exact",
        &[],
        &[
            WAVE_07,
            (
                "ProgrammableWaveData_8",
                "sound/programmable_wave_samples/08.pcm",
            ),
        ],
    );
    for file in ["07", "08"] {
        std::fs::write(
            root.join(format!("sound/programmable_wave_samples/{file}.pcm")),
            PROGRAMMABLE_WAVE_01,
        )
        .unwrap();
    }
    let mut writer = PackWriter::new();
    let requested = ids(&["audio/sample/programmable-wave/07"]);
    let result = extract_audio_samples(&root, &requested, &mut writer);
    std::fs::remove_dir_all(&root).unwrap();

    result.unwrap();
    assert_eq!(writer.len(), 1);
}

#[test]
fn a_symbol_reads_the_source_its_definition_binds_not_one_named_for_it() {
    let root = scratch_checkout(
        "bound",
        &[(
            "DirectSoundWaveData_unknown_1",
            "sound/direct_sound_samples/unknown_01.bin",
        )],
        &[],
    );
    let result = extract_audio_samples(
        &root,
        &ids(&["audio/sample/direct-sound/unknown_1"]),
        &mut PackWriter::new(),
    );
    std::fs::remove_dir_all(&root).unwrap();

    let Err(ExtractError::ReadFailed(path, _)) = result else {
        panic!("expected an unreadable source, got {result:?}");
    };
    assert!(
        path.ends_with("sound/direct_sound_samples/unknown_01.wav"),
        "{path:?}"
    );
}

#[test]
fn a_missing_requested_sample_source_or_definition_fails_closed() {
    let root = scratch_checkout(
        "missing",
        &[(
            "DirectSoundWaveData_ghost",
            "sound/direct_sound_samples/ghost.bin",
        )],
        &[WAVE_07],
    );
    let mut writer = PackWriter::new();
    let direct = extract_audio_samples(
        &root,
        &ids(&["audio/sample/direct-sound/ghost"]),
        &mut writer,
    );
    let wave = extract_audio_samples(
        &root,
        &ids(&["audio/sample/programmable-wave/07"]),
        &mut writer,
    );
    let undefined = extract_audio_samples(
        &root,
        &ids(&["audio/sample/programmable-wave/09"]),
        &mut writer,
    );
    std::fs::remove_dir_all(&root).unwrap();

    assert!(matches!(direct, Err(ExtractError::ReadFailed(..))));
    assert!(matches!(wave, Err(ExtractError::ReadFailed(..))));
    assert!(matches!(
        undefined,
        Err(ExtractError::UndefinedSample { .. })
    ));
}

#[test]
fn an_invalid_requested_wave_or_id_fails_closed() {
    let root = scratch_checkout(
        "invalid",
        &[(
            "DirectSoundWaveData_junk",
            "sound/direct_sound_samples/junk.bin",
        )],
        &[(
            "ProgrammableWaveData_4",
            "sound/programmable_wave_samples/04.pcm",
        )],
    );
    std::fs::write(
        root.join("sound/programmable_wave_samples/04.pcm"),
        [0u8; 15],
    )
    .unwrap();
    std::fs::write(
        root.join("sound/direct_sound_samples/junk.wav"),
        b"not a wav",
    )
    .unwrap();
    let mut writer = PackWriter::new();
    let short_wave = extract_audio_samples(
        &root,
        &ids(&["audio/sample/programmable-wave/04"]),
        &mut writer,
    );
    let junk_wav = extract_audio_samples(
        &root,
        &ids(&["audio/sample/direct-sound/junk"]),
        &mut writer,
    );
    let bad_id = extract_audio_samples(&root, &ids(&["audio/sample/other/x"]), &mut writer);
    std::fs::remove_dir_all(&root).unwrap();

    assert!(matches!(
        short_wave,
        Err(ExtractError::ProgrammableWaveWrongSize { actual: 15, .. })
    ));
    assert!(matches!(junk_wav, Err(ExtractError::Wav(..))));
    assert!(matches!(bad_id, Err(ExtractError::MalformedSampleId(_))));
}

#[test]
fn a_definition_outside_the_sample_directory_fails_closed() {
    let root = scratch_checkout(
        "escape",
        &[("DirectSoundWaveData_out", "sound/elsewhere/out.bin")],
        &[],
    );
    let result = extract_audio_samples(
        &root,
        &ids(&["audio/sample/direct-sound/out"]),
        &mut PackWriter::new(),
    );
    std::fs::remove_dir_all(&root).unwrap();

    assert!(matches!(
        result,
        Err(ExtractError::SampleDefinitions { line: 0, .. })
    ));
}

/// The expected bytes are literals, so these tests pin the extracted
/// payload bytes independently of `assets`' encoder.
#[test]
fn encode_direct_sound_matches_the_documented_wire_format() {
    // `pcm` is the buffer: 4 logical samples plus one retained guard.
    let pcm = [-1, 0, 1, 127, -128];
    let sample_count = 4;
    let bytes = encode_direct_sound(wav_sample(0x0012_3456, Some(2), sample_count, &pcm)).unwrap();
    let encoded_pcm = [0xFF, 0x00, 0x01, 0x7F, 0x80];

    let mut expected = vec![0];
    expected.extend_from_slice(&0x0012_3456u32.to_le_bytes());
    expected.push(1);
    expected.extend_from_slice(&2u32.to_le_bytes());
    expected.extend_from_slice(&sample_count.to_le_bytes());
    expected.extend_from_slice(&encoded_pcm);
    assert_eq!(bytes, expected);
}

#[test]
fn encode_direct_sound_one_shot_writes_a_false_loop_flag_and_zero_start() {
    let pcm = [-128, -1, 0, 1, 127];
    let sample_count = 4;
    let bytes = encode_direct_sound(wav_sample(1 << 20, None, sample_count, &pcm)).unwrap();
    let encoded_pcm = [0x80, 0xFF, 0x00, 0x01, 0x7F];

    let mut expected = vec![0];
    expected.extend_from_slice(&(1u32 << 20).to_le_bytes());
    expected.push(0);
    expected.extend_from_slice(&0u32.to_le_bytes());
    expected.extend_from_slice(&sample_count.to_le_bytes());
    expected.extend_from_slice(&encoded_pcm);
    assert_eq!(bytes, expected);
}

/// The schema tags are owned by `crates/assets`'s `Sample` wire format, so
/// the expected bytes here are literals that fail if the schema drifts.
#[test]
fn encoders_write_the_literal_schema_kind_tags() {
    assert_eq!(
        encode_direct_sound(wav_sample(0, None, 0, &[0])).unwrap()[0],
        0
    );
    assert_eq!(
        encode_programmable_wave([0u8; PROGRAMMABLE_WAVE_SIZE])[0],
        1
    );
}

#[test]
fn encode_programmable_wave_matches_the_documented_wire_format() {
    let mut table = [0u8; PROGRAMMABLE_WAVE_SIZE];
    for (index, byte) in table.iter_mut().enumerate() {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the 16-entry test index multiplied by 17 fits in u8"
        )]
        {
            *byte = (index * 17) as u8;
        }
    }

    let bytes = encode_programmable_wave(table);
    let mut expected = vec![1];
    expected.extend_from_slice(&table);
    assert_eq!(bytes, expected);
}

/// The runtime decoder accepts what the extractor writes and recovers
/// the mapped `assets` value.
#[test]
fn encoded_payloads_decode_through_the_assets_schema() {
    let pcm = [-1, 0, 1, 127, -128];
    let bytes = encode_direct_sound(wav_sample(7, Some(2), 4, &pcm)).unwrap();
    assert_eq!(
        Sample::decode(&bytes),
        Ok(Sample::DirectSound(
            DirectSoundSample::new(7, Some(2), 4, pcm.to_vec()).unwrap()
        ))
    );

    let table = [9u8; PROGRAMMABLE_WAVE_SIZE];
    assert_eq!(
        Sample::decode(&encode_programmable_wave(table)),
        Ok(Sample::ProgrammableWave(ProgrammableWave { table }))
    );
}

#[test]
fn encode_direct_sound_surfaces_assets_schema_errors() {
    // The loop starts at the logical end, which `assets` rejects.
    assert_eq!(
        encode_direct_sound(wav_sample(1, Some(4), 4, &[0; 5])),
        Err(AudioError::LoopStartOutOfRange {
            loop_start: 4,
            sample_count: 4,
        })
    );
    // The guard sample is missing.
    assert_eq!(
        encode_direct_sound(wav_sample(1, None, 4, &[0; 4])),
        Err(AudioError::DirectSoundBufferLength {
            sample_count: 4,
            buffer_len: 4,
        })
    );
}

#[test]
fn schema_rejection_is_a_typed_extract_error_naming_the_source() {
    let path = std::path::PathBuf::from("sound/direct_sound_samples/demo.wav");
    let error = ExtractError::InvalidSampleSchema {
        path: path.clone(),
        source: AudioError::LoopStartOutOfRange {
            loop_start: 4,
            sample_count: 4,
        },
    };
    let message = error.to_string();
    assert!(message.contains("demo.wav"), "{message}");
    assert!(message.contains("schema rejected"), "{message}");
}

#[test]
fn the_pinned_title_sample_lists_are_sorted_and_free_of_duplicates() {
    let unique: BTreeSet<_> = TITLE_DIRECT_SOUND_SAMPLES.iter().collect();
    assert_eq!(unique.len(), TITLE_DIRECT_SOUND_SAMPLES.len());
    assert!(TITLE_DIRECT_SOUND_SAMPLES.is_sorted());
    let waves: BTreeSet<_> = TITLE_PROGRAMMABLE_WAVE_SAMPLES.iter().collect();
    assert_eq!(waves.len(), TITLE_PROGRAMMABLE_WAVE_SAMPLES.len());
}
