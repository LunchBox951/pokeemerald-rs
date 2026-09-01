//! Extracts normalized `DirectSound` and CGB programmable-wave samples used by
//! `mus_title` into raw pack entries.
//!
//! The manifests are explicit because sample extraction runs before voicegroup
//! resolution. Real-pack integration tests verify that every sample id reached
//! through the resolved voicegroups exists in the pack.
//!
//! Payload encoding intentionally duplicates `crates/assets`'s `Sample` wire
//! format so the extraction tool and runtime asset crate remain independent.
//! Cross-crate real-pack tests decode these payloads through that schema.

use std::mem::size_of;
use std::path::Path;

use super::wav;
use super::{read_file, ExtractError};
use pack_format::{PackEntry, PackWriter};

const DIRECT_SOUND_SAMPLES: [&str; 33] = [
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
    "trinity_cymbal_crash",
    "unknown_bell",
    "unknown_close_hihat",
    "unknown_open_hihat",
    "unused_sc55_tom",
];

const PROGRAMMABLE_WAVE_SAMPLES: [u32; 4] = [1, 2, 5, 6];

const PROGRAMMABLE_WAVE_SIZE: usize = 16;
const SAMPLE_KIND_DIRECT_SOUND: u8 = 0;
const SAMPLE_KIND_PROGRAMMABLE_WAVE: u8 = 1;
const SAMPLE_KIND_SIZE: usize = size_of::<u8>();
const BASE_FREQUENCY_SIZE: usize = size_of::<u32>();
const LOOP_FLAG_SIZE: usize = size_of::<u8>();
const LOOP_START_SIZE: usize = size_of::<u32>();
const SAMPLE_COUNT_SIZE: usize = size_of::<u32>();
const DIRECT_SOUND_FIXED_PAYLOAD_SIZE: usize =
    SAMPLE_KIND_SIZE + BASE_FREQUENCY_SIZE + LOOP_FLAG_SIZE + LOOP_START_SIZE + SAMPLE_COUNT_SIZE;
const PROGRAMMABLE_WAVE_PAYLOAD_SIZE: usize = SAMPLE_KIND_SIZE + PROGRAMMABLE_WAVE_SIZE;

fn encode_direct_sound(base_frequency: u32, loop_start: Option<u32>, data: &[i8]) -> Vec<u8> {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "wav::decode bounds its output length with a u32 sample count"
    )]
    let sample_count = data.len() as u32;
    let loop_start_field = loop_start.unwrap_or_default();

    let mut payload = Vec::with_capacity(DIRECT_SOUND_FIXED_PAYLOAD_SIZE + data.len());
    payload.push(SAMPLE_KIND_DIRECT_SOUND);
    payload.extend_from_slice(&base_frequency.to_le_bytes());
    payload.push(u8::from(loop_start.is_some()));
    payload.extend_from_slice(&loop_start_field.to_le_bytes());
    payload.extend_from_slice(&sample_count.to_le_bytes());
    for &sample in data {
        payload.extend_from_slice(&sample.to_le_bytes());
    }
    payload
}

fn encode_programmable_wave(table: &[u8; PROGRAMMABLE_WAVE_SIZE]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(PROGRAMMABLE_WAVE_PAYLOAD_SIZE);
    payload.push(SAMPLE_KIND_PROGRAMMABLE_WAVE);
    payload.extend_from_slice(table);
    payload
}

fn direct_sound_entry(upstream: &Path, name: &str) -> Result<PackEntry, ExtractError> {
    let source_path = upstream
        .join("sound/direct_sound_samples")
        .join(format!("{name}.wav"));
    let source = read_file(&source_path)?;
    let sample = wav::decode(&source).map_err(|error| ExtractError::Wav(source_path, error))?;

    Ok(pack_format::raw_entry(
        format!("audio/sample/direct-sound/{name}"),
        encode_direct_sound(sample.base_frequency, sample.loop_start, &sample.data),
    ))
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
        encode_programmable_wave(&table),
    ))
}

/// Extracts every `DirectSound`/programmable-wave sample [`DIRECT_SOUND_SAMPLES`]/
/// [`PROGRAMMABLE_WAVE_SAMPLES`] name (see the module docs for how that set
/// was derived) as `audio/sample/*` [`EntryKind::Raw`] entries.
///
/// # Errors
///
/// Returns [`ExtractError::ReadFailed`] for an unreadable source,
/// [`ExtractError::Wav`] for an invalid WAV, or
/// [`ExtractError::ProgrammableWaveWrongSize`] for a table that is not 16 bytes.
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
        encode_direct_sound, encode_programmable_wave, read_file, wav, DIRECT_SOUND_SAMPLES,
        PROGRAMMABLE_WAVE_SAMPLES, PROGRAMMABLE_WAVE_SIZE, SAMPLE_KIND_DIRECT_SOUND,
        SAMPLE_KIND_PROGRAMMABLE_WAVE,
    };

    const FLUTE_BASE_FREQUENCY: u32 = 3_425_024;
    const FLUTE_LOOP_START: u32 = 1_312;
    const FLUTE_SAMPLE_COUNT: usize = 1_874;
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
        assert_eq!(flute.data.len(), FLUTE_SAMPLE_COUNT);
        let flute_payload =
            encode_direct_sound(flute.base_frequency, flute.loop_start, &flute.data);
        assert_pack_contains(&pack, &flute_payload, "the flute sample's encoded payload");

        let wave_path = repo_root().join("pokeemerald/sound/programmable_wave_samples/01.pcm");
        let wave_source =
            read_file(&wave_path).expect("programmable-wave 01 exists in a real checkout");
        assert_eq!(wave_source, PROGRAMMABLE_WAVE_01);
        let wave_payload = encode_programmable_wave(&PROGRAMMABLE_WAVE_01);
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

    #[test]
    fn encode_direct_sound_matches_the_documented_wire_format() {
        let pcm = [-1, 0, 1, 127, -128];
        let bytes = encode_direct_sound(0x0012_3456, Some(42), &pcm);
        let encoded_pcm = [0xFF, 0x00, 0x01, 0x7F, 0x80];
        let sample_count = u32::try_from(pcm.len()).unwrap();

        let mut expected = vec![SAMPLE_KIND_DIRECT_SOUND];
        expected.extend_from_slice(&0x0012_3456u32.to_le_bytes());
        expected.push(u8::from(true));
        expected.extend_from_slice(&42u32.to_le_bytes());
        expected.extend_from_slice(&sample_count.to_le_bytes());
        expected.extend_from_slice(&encoded_pcm);
        assert_eq!(bytes, expected);
    }

    #[test]
    fn encode_direct_sound_one_shot_writes_a_false_loop_flag_and_zero_start() {
        let pcm = [-128, -1, 0, 1, 127];
        let bytes = encode_direct_sound(1 << 20, None, &pcm);
        let encoded_pcm = [0x80, 0xFF, 0x00, 0x01, 0x7F];
        let sample_count = u32::try_from(pcm.len()).unwrap();

        let mut expected = vec![SAMPLE_KIND_DIRECT_SOUND];
        expected.extend_from_slice(&(1u32 << 20).to_le_bytes());
        expected.push(u8::from(false));
        expected.extend_from_slice(&0u32.to_le_bytes());
        expected.extend_from_slice(&sample_count.to_le_bytes());
        expected.extend_from_slice(&encoded_pcm);
        assert_eq!(bytes, expected);
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

        let bytes = encode_programmable_wave(&table);
        let mut expected = vec![SAMPLE_KIND_PROGRAMMABLE_WAVE];
        expected.extend_from_slice(&table);
        assert_eq!(bytes, expected);
    }
}
