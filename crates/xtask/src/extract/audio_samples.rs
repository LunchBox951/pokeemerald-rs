//! Extracts normalized `DirectSound` and CGB programmable-wave samples used by
//! `mus_title` into raw pack entries.
//!
//! The manifests are explicit because sample extraction runs before voicegroup
//! resolution. Real-pack integration tests verify that every sample id reached
//! through the resolved voicegroups exists in the pack.
//!
//! The payload schema is owned by `crates/assets`: each decoded WAV or
//! programmable-wave table is mapped onto [`assets::Sample`] and written by
//! [`assets::Sample::encode`], so the kind tags, field order, and guard-sample
//! framing exist in exactly one place and the runtime decoder validates what
//! the extractor writes. Building a [`assets::DirectSoundSample`] re-runs
//! `assets`' own validation, so a value outside the schema's domain surfaces
//! as [`ExtractError::InvalidSampleSchema`] rather than as a pack the runtime
//! would refuse to load. The WAV decoder already rejects those values with
//! source-specific errors first, so the extractor never reports that variant
//! for upstream's real samples.

use std::path::Path;

use super::wav::{self, WavSample};
use super::{read_file, ExtractError};
use assets::{DirectSoundSample, ProgrammableWave, Sample};
use pack_format::{PackEntry, PackWriter};

const DIRECT_SOUND_SAMPLES: [&str; 41] = [
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

const PROGRAMMABLE_WAVE_SAMPLES: [u32; 5] = [1, 2, 3, 5, 6];

const PROGRAMMABLE_WAVE_SIZE: usize = 16;

/// Maps one decoded WAV onto [`assets::Sample`] and encodes it through
/// `assets`' canonical encoder.
///
/// `sample.data` always holds exactly one more value than `sample_count`: a
/// retained interpolation-guard sample past the logical end (see
/// `wav.rs`'s module docs and `assets::DirectSoundSample`).
///
/// # Errors
///
/// Returns the [`assets::AudioError`] `assets` raises when the decoded fields
/// are outside the sample schema's domain.
fn encode_direct_sound(sample: WavSample) -> Result<Vec<u8>, assets::AudioError> {
    let direct_sound = DirectSoundSample::new(
        sample.base_frequency,
        sample.loop_start,
        sample.sample_count,
        sample.data,
    )?;
    Ok(Sample::DirectSound(direct_sound).encode())
}

fn encode_programmable_wave(table: [u8; PROGRAMMABLE_WAVE_SIZE]) -> Vec<u8> {
    Sample::ProgrammableWave(ProgrammableWave { table }).encode()
}

fn direct_sound_entry(upstream: &Path, name: &str) -> Result<PackEntry, ExtractError> {
    let source_path = upstream
        .join("sound/direct_sound_samples")
        .join(format!("{name}.wav"));
    direct_sound_entry_from_path(&source_path, format!("audio/sample/direct-sound/{name}"))
}

/// Decodes one `DirectSound` WAV and wraps its payload as the raw pack entry `id`.
///
/// The one codec every `DirectSound` family shares, so instrument and cry samples are
/// byte-identical for the same source.
pub(super) fn direct_sound_entry_from_path(
    source_path: &Path,
    id: String,
) -> Result<PackEntry, ExtractError> {
    let source = read_file(source_path)?;
    let sample = wav::decode(&source)
        .map_err(|error| ExtractError::Wav(source_path.to_path_buf(), error))?;

    let payload =
        encode_direct_sound(sample).map_err(|source| ExtractError::InvalidSampleSchema {
            path: source_path.to_path_buf(),
            source,
        })?;

    Ok(pack_format::raw_entry(id, payload))
}

fn programmable_wave_entry(upstream: &Path, number: u32) -> Result<PackEntry, ExtractError> {
    let source_path = upstream
        .join("sound/programmable_wave_samples")
        .join(format!("{number:02}.pcm"));
    let source = read_file(&source_path)?;
    let table = <[u8; PROGRAMMABLE_WAVE_SIZE]>::try_from(source.as_slice()).map_err(|_| {
        ExtractError::ProgrammableWaveWrongSize {
            path: source_path,
            actual: source.len(),
        }
    })?;

    Ok(pack_format::raw_entry(
        format!("audio/sample/programmable-wave/{number:02}"),
        encode_programmable_wave(table),
    ))
}

/// Extracts every `DirectSound`/programmable-wave sample [`DIRECT_SOUND_SAMPLES`]/
/// [`PROGRAMMABLE_WAVE_SAMPLES`] name (see the module docs for how that set
/// was derived) as `audio/sample/*` [`pack_format::EntryKind::Raw`] entries.
///
/// # Errors
///
/// Returns [`ExtractError::ReadFailed`] for an unreadable source,
/// [`ExtractError::Wav`] for an invalid WAV,
/// [`ExtractError::ProgrammableWaveWrongSize`] for a table that is not 16 bytes, or
/// [`ExtractError::InvalidSampleSchema`] when `assets` rejects a decoded sample.
pub(super) fn extract_audio_samples(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    for name in DIRECT_SOUND_SAMPLES {
        writer.push(direct_sound_entry(upstream, name)?);
    }
    for number in PROGRAMMABLE_WAVE_SAMPLES {
        writer.push(programmable_wave_entry(upstream, number)?);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        encode_direct_sound, encode_programmable_wave, read_file, wav, ExtractError, WavSample,
        DIRECT_SOUND_SAMPLES, PROGRAMMABLE_WAVE_SAMPLES, PROGRAMMABLE_WAVE_SIZE,
    };
    use assets::{AudioError, DirectSoundSample, ProgrammableWave, Sample};

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
        0x01, 0x25, 0x8a, 0xde, 0xfe, 0xc9, 0x63, 0x10, 0x01, 0x25, 0x8a, 0xde, 0xfe, 0xc9, 0x63,
        0x10,
    ];

    fn scratch_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "pokeemerald-rs-extract-test-{name}-{}.pack",
            std::process::id()
        ))
    }

    fn assert_pack_contains(bytes: &[u8], expected: &[u8], description: &str) {
        assert!(
            bytes
                .windows(expected.len())
                .any(|window| window == expected),
            "pack does not contain {description}"
        );
    }

    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn direct_sound_and_programmable_wave_samples_are_extracted() {
        use super::super::{extract_to, repo_root, upstream_present};

        assert!(upstream_present(), "run ./init.sh first");
        let path = scratch_path("audio-samples");
        let report = extract_to(&path).expect("extraction should succeed against a real checkout");
        let pack = std::fs::read(&report.output_path).unwrap();

        for name in DIRECT_SOUND_SAMPLES {
            let id = format!("audio/sample/direct-sound/{name}");
            assert_pack_contains(&pack, id.as_bytes(), &format!("entry id `{id}`"));
        }
        for number in PROGRAMMABLE_WAVE_SAMPLES {
            let id = format!("audio/sample/programmable-wave/{number:02}");
            assert_pack_contains(&pack, id.as_bytes(), &format!("entry id `{id}`"));
        }

        let flute_path =
            repo_root().join("pokeemerald/sound/direct_sound_samples/sc88pro_flute.wav");
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
        let flute_payload =
            encode_direct_sound(flute).expect("the flute sample is inside the schema");
        assert_pack_contains(&pack, &flute_payload, "the flute sample's encoded payload");

        let wave_path = repo_root().join("pokeemerald/sound/programmable_wave_samples/01.pcm");
        let wave_source =
            read_file(&wave_path).expect("programmable-wave 01 exists in a real checkout");
        assert_eq!(wave_source, PROGRAMMABLE_WAVE_01);
        let wave_payload = encode_programmable_wave(PROGRAMMABLE_WAVE_01);
        assert_pack_contains(
            &pack,
            &wave_payload,
            "programmable-wave 01's encoded payload",
        );

        let _ = std::fs::remove_file(report.output_path);
    }

    #[test]
    fn direct_sound_sample_list_has_no_duplicates() {
        let unique: std::collections::HashSet<_> = DIRECT_SOUND_SAMPLES.iter().collect();
        assert_eq!(unique.len(), DIRECT_SOUND_SAMPLES.len());
        assert!(
            DIRECT_SOUND_SAMPLES.is_sorted(),
            "DIRECT_SOUND_SAMPLES must remain sorted"
        );
        for name in DIRECT_SOUND_SAMPLES {
            assert!(name.chars().all(|character| character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == '_'));
        }
    }

    #[test]
    fn programmable_wave_sample_list_has_no_duplicates() {
        let unique: std::collections::HashSet<_> = PROGRAMMABLE_WAVE_SAMPLES.iter().collect();
        assert_eq!(unique.len(), PROGRAMMABLE_WAVE_SAMPLES.len());
        for number in PROGRAMMABLE_WAVE_SAMPLES {
            assert_ne!(number, 0);
        }
    }

    /// The expected bytes are literals, so these tests pin the extracted
    /// payload bytes independently of `assets`' encoder.
    #[test]
    fn encode_direct_sound_matches_the_documented_wire_format() {
        // `pcm` is the buffer: 4 logical samples plus one retained guard.
        let pcm = [-1, 0, 1, 127, -128];
        let sample_count = 4;
        let bytes =
            encode_direct_sound(wav_sample(0x0012_3456, Some(2), sample_count, &pcm)).unwrap();
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
}
