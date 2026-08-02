//! [`decode`]/[`WavSample`] fixture tests, split out of `wav.rs` itself
//! to keep that file under this crate's ~600-line-per-file guideline
//! (`oop-boundaries`) -- mirrors `crates::assets::audio::song`'s own
//! `mod tests;` split.

// Every length cast below narrows a hand-built in-memory fixture's byte
// count (always a handful of bytes) into the WAV chunk-length field's `u32`
// -- provably in range, so the truncation lint would only add noise here.
#![allow(clippy::cast_possible_truncation)]

use super::{decode, SampleFormat, WavError};

/// Build a minimal well-formed mono WAV file's bytes. `extra_chunks` is
/// inserted between `fmt ` and `data` (e.g. `smpl`/`agbp`/`agbl`).
fn build_wav(
    format_tag: u16,
    block_align: u16,
    bits_per_sample: u16,
    sample_rate: u32,
    extra_chunks: &[(&[u8; 4], &[u8])],
    data: &[u8],
) -> Vec<u8> {
    let mut fmt_body = Vec::new();
    fmt_body.extend_from_slice(&format_tag.to_le_bytes());
    fmt_body.extend_from_slice(&1u16.to_le_bytes()); // mono
    fmt_body.extend_from_slice(&sample_rate.to_le_bytes());
    let byte_rate = sample_rate * u32::from(block_align);
    fmt_body.extend_from_slice(&byte_rate.to_le_bytes());
    fmt_body.extend_from_slice(&block_align.to_le_bytes());
    fmt_body.extend_from_slice(&bits_per_sample.to_le_bytes());

    let mut chunks = Vec::new();
    chunks.push((*b"fmt ", fmt_body));
    for (id, body) in extra_chunks {
        chunks.push((**id, body.to_vec()));
    }
    chunks.push((*b"data", data.to_vec()));

    let mut riff_body = Vec::new();
    riff_body.extend_from_slice(b"WAVE");
    for (id, body) in &chunks {
        riff_body.extend_from_slice(id);
        #[allow(clippy::cast_possible_truncation)]
        riff_body.extend_from_slice(&(body.len() as u32).to_le_bytes());
        riff_body.extend_from_slice(body);
        if body.len() % 2 == 1 {
            riff_body.push(0);
        }
    }

    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    #[allow(clippy::cast_possible_truncation)]
    out.extend_from_slice(&(riff_body.len() as u32).to_le_bytes());
    out.extend_from_slice(&riff_body);
    out
}

fn smpl_chunk(midi_key: u32, tuning_fraction: u32, loop_start: u32, loop_end: u32) -> Vec<u8> {
    let mut body = vec![0u8; 32 + 24];
    body[12..16].copy_from_slice(&midi_key.to_le_bytes());
    body[16..20].copy_from_slice(&tuning_fraction.to_le_bytes());
    body[28..32].copy_from_slice(&1u32.to_le_bytes()); // num loops
                                                       // loop record: cue point id (36), type (40), start (44), end (48), fraction (52), play count (56)
    body[40..44].copy_from_slice(&0u32.to_le_bytes());
    body[44..48].copy_from_slice(&loop_start.to_le_bytes());
    body[48..52].copy_from_slice(&loop_end.to_le_bytes());
    body
}

fn u32_chunk(value: u32) -> Vec<u8> {
    value.to_le_bytes().to_vec()
}

#[test]
fn one_shot_u8_sample_decodes() {
    // 5 unsigned 8-bit samples, no smpl chunk -> one-shot, key=60/no
    // tuning, size == num_samples.
    let data = [118u8, 128, 138, 128, 108]; // -10, 0, 10, 0, -20 once centered
    let bytes = build_wav(1, 1, 8, 3344, &[], &data);
    let sample = decode(&bytes).unwrap();
    assert_eq!(sample.base_frequency, 3344 * 1024);
    assert_eq!(sample.loop_start, None);
    assert_eq!(sample.data, vec![-10, 0, 10, 0, -20]);
}

#[test]
fn looped_sample_uses_naive_loop_end_plus_one_when_no_agbl_override() {
    let data = [128u8, 129, 130, 131, 132, 133];
    let smpl = smpl_chunk(60, 0, 1, 3); // loop start=1, end=3 (inclusive)
    let bytes = build_wav(1, 1, 8, 8000, &[(b"smpl", &smpl)], &data);
    let sample = decode(&bytes).unwrap();
    assert_eq!(sample.loop_start, Some(1));
    // naive end = loop_end(3) + 1 = 4 samples.
    assert_eq!(sample.data, vec![0, 1, 2, 3]);
}

#[test]
fn agbl_override_shortens_the_decoded_data() {
    let data = [128u8, 129, 130, 131, 132, 133];
    let smpl = smpl_chunk(60, 0, 1, 3);
    let agbl = u32_chunk(2); // override: only 2 samples of real content
    let bytes = build_wav(1, 1, 8, 8000, &[(b"smpl", &smpl), (b"agbl", &agbl)], &data);
    let sample = decode(&bytes).unwrap();
    assert_eq!(sample.data, vec![0, 1]);
    assert_eq!(sample.loop_start, Some(1));
}

#[test]
fn agbp_override_replaces_the_computed_pitch() {
    let data = [128u8, 129];
    let agbp = u32_chunk(0x0034_2000);
    let bytes = build_wav(1, 1, 8, 3344, &[(b"agbp", &agbp)], &data);
    let sample = decode(&bytes).unwrap();
    assert_eq!(sample.base_frequency, 0x0034_2000);
}

/// Upstream reads `agbp` out of a field that defaults to `0` and applies it
/// only when non-zero (`converter.cpp:392-397`), so a present-but-zero
/// `agbp` chunk must fall back to the computed pitch rather than compiling
/// a zero pitch word. No shipped sample carries one -- hence the crafted
/// fixture.
#[test]
fn a_zero_agbp_chunk_falls_back_to_the_computed_pitch() {
    let data = [128u8, 129];
    let agbp = u32_chunk(0);
    let bytes = build_wav(1, 1, 8, 3344, &[(b"agbp", &agbp)], &data);
    let sample = decode(&bytes).unwrap();
    assert_eq!(sample.base_frequency, 3344 * 1024);
}

/// The `agbl` half of the same upstream rule (`converter.cpp:399-402`): a
/// zero override leaves the naive loop end in place instead of shrinking
/// the decoded sample to zero length.
#[test]
fn a_zero_agbl_chunk_falls_back_to_the_naive_loop_end() {
    let data = [128u8, 129, 130, 131, 132, 133];
    let smpl = smpl_chunk(60, 0, 1, 3);
    let agbl = u32_chunk(0);
    let bytes = build_wav(1, 1, 8, 8000, &[(b"smpl", &smpl), (b"agbl", &agbl)], &data);
    let sample = decode(&bytes).unwrap();
    // Same result as
    // `looped_sample_uses_naive_loop_end_plus_one_when_no_agbl_override`:
    // naive end = loop_end(3) + 1 = 4 samples, not 0.
    assert_eq!(sample.data, vec![0, 1, 2, 3]);
    assert_eq!(sample.loop_start, Some(1));
}

#[test]
fn non_default_midi_key_shifts_the_computed_pitch() {
    // One octave down (key 72 vs unity 60) halves the naive pitch.
    let data = [128u8, 129];
    let smpl = smpl_chunk(72, 0, 0, 0);
    let bytes = build_wav(1, 1, 8, 8000, &[(b"smpl", &smpl)], &data);
    let sample = decode(&bytes).unwrap();
    assert_eq!(sample.base_frequency, 4000 * 1024);
}

#[test]
fn s16_format_decodes() {
    let mut data = Vec::new();
    data.extend_from_slice(&(-32768i16).to_le_bytes());
    data.extend_from_slice(&0i16.to_le_bytes());
    data.extend_from_slice(&32767i16.to_le_bytes());
    let bytes = build_wav(1, 2, 16, 22050, &[], &data);
    let sample = decode(&bytes).unwrap();
    assert_eq!(sample.data, vec![-128, 0, 127]);
}

#[test]
fn f32_format_decodes() {
    let mut data = Vec::new();
    data.extend_from_slice(&(-1.0f32).to_le_bytes());
    data.extend_from_slice(&(0.0f32).to_le_bytes());
    let bytes = build_wav(3, 4, 32, 22050, &[], &data);
    let sample = decode(&bytes).unwrap();
    assert_eq!(sample.data, vec![-128, 0]);
}

#[test]
fn bad_riff_magic_is_rejected() {
    let mut bytes = build_wav(1, 1, 8, 8000, &[], &[128]);
    bytes[0] = b'X';
    assert_eq!(decode(&bytes).unwrap_err(), WavError::BadRiffMagic);
}

#[test]
fn riff_size_mismatch_is_rejected() {
    let mut bytes = build_wav(1, 1, 8, 8000, &[], &[128]);
    bytes.push(0xFF); // trailing byte the RIFF length doesn't account for
    assert!(matches!(
        decode(&bytes).unwrap_err(),
        WavError::RiffSizeMismatch { .. }
    ));
}

#[test]
fn bad_wave_magic_is_rejected() {
    let mut bytes = build_wav(1, 1, 8, 8000, &[], &[128]);
    bytes[8..12].copy_from_slice(b"WAVX");
    assert_eq!(decode(&bytes).unwrap_err(), WavError::BadWaveMagic);
}

#[test]
fn missing_fmt_chunk_is_rejected() {
    // Hand-build a WAV with only a `data` chunk.
    let mut riff_body = Vec::new();
    riff_body.extend_from_slice(b"WAVE");
    riff_body.extend_from_slice(b"data");
    riff_body.extend_from_slice(&4u32.to_le_bytes());
    riff_body.extend_from_slice(&[1, 2, 3, 4]);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(riff_body.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&riff_body);
    assert_eq!(decode(&bytes).unwrap_err(), WavError::MissingFmtChunk);
}

#[test]
fn truncated_fmt_chunk_is_rejected() {
    // A `fmt ` chunk shorter than the fixed 16-byte layout this reader
    // requires (the real chunk always carries at least that much).
    let mut riff_body = Vec::new();
    riff_body.extend_from_slice(b"WAVE");
    riff_body.extend_from_slice(b"fmt ");
    riff_body.extend_from_slice(&8u32.to_le_bytes());
    riff_body.extend_from_slice(&[0u8; 8]);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(riff_body.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&riff_body);
    assert_eq!(decode(&bytes).unwrap_err(), WavError::ChunkTruncated);
}

#[test]
fn missing_data_chunk_is_rejected() {
    let mut fmt_body = Vec::new();
    fmt_body.extend_from_slice(&1u16.to_le_bytes());
    fmt_body.extend_from_slice(&1u16.to_le_bytes());
    fmt_body.extend_from_slice(&8000u32.to_le_bytes());
    fmt_body.extend_from_slice(&8000u32.to_le_bytes());
    fmt_body.extend_from_slice(&1u16.to_le_bytes());
    fmt_body.extend_from_slice(&8u16.to_le_bytes());
    let mut riff_body = Vec::new();
    riff_body.extend_from_slice(b"WAVE");
    riff_body.extend_from_slice(b"fmt ");
    riff_body.extend_from_slice(&(fmt_body.len() as u32).to_le_bytes());
    riff_body.extend_from_slice(&fmt_body);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(riff_body.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&riff_body);
    assert_eq!(decode(&bytes).unwrap_err(), WavError::MissingDataChunk);
}

#[test]
fn stereo_is_rejected() {
    let mut fmt_body = Vec::new();
    fmt_body.extend_from_slice(&1u16.to_le_bytes());
    fmt_body.extend_from_slice(&2u16.to_le_bytes()); // stereo
    fmt_body.extend_from_slice(&8000u32.to_le_bytes());
    fmt_body.extend_from_slice(&16000u32.to_le_bytes());
    fmt_body.extend_from_slice(&2u16.to_le_bytes());
    fmt_body.extend_from_slice(&8u16.to_le_bytes());
    let mut riff_body = Vec::new();
    riff_body.extend_from_slice(b"WAVE");
    riff_body.extend_from_slice(b"fmt ");
    riff_body.extend_from_slice(&(fmt_body.len() as u32).to_le_bytes());
    riff_body.extend_from_slice(&fmt_body);
    riff_body.extend_from_slice(b"data");
    riff_body.extend_from_slice(&2u32.to_le_bytes());
    riff_body.extend_from_slice(&[128, 128]);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(riff_body.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&riff_body);
    assert_eq!(
        decode(&bytes).unwrap_err(),
        WavError::NotMono { channels: 2 }
    );
}

#[test]
fn unsupported_format_tag_is_rejected() {
    // format tag 2 (ADPCM) isn't one of the six supported combinations.
    let bytes = build_wav(2, 1, 8, 8000, &[], &[128]);
    assert_eq!(
        decode(&bytes).unwrap_err(),
        WavError::UnsupportedFormat {
            format_tag: 2,
            block_align: 1,
            bits_per_sample: 8,
        }
    );
}

#[test]
fn unsupported_bit_depth_combination_is_rejected() {
    // Integer PCM but a width SampleFormat::resolve doesn't recognize.
    let bytes = build_wav(1, 5, 40, 8000, &[], &[128, 128, 128, 128, 128]);
    assert!(matches!(
        decode(&bytes).unwrap_err(),
        WavError::UnsupportedFormat { .. }
    ));
}

#[test]
fn misaligned_data_length_is_rejected() {
    // s16 format but an odd number of bytes.
    let bytes = build_wav(1, 2, 16, 8000, &[], &[1, 2, 3]);
    assert_eq!(
        decode(&bytes).unwrap_err(),
        WavError::DataLengthNotAligned {
            data_len: 3,
            bytes_per_sample: 2,
        }
    );
}

#[test]
fn too_many_sample_loops_is_rejected() {
    let mut smpl = vec![0u8; 32];
    smpl[28..32].copy_from_slice(&2u32.to_le_bytes());
    let bytes = build_wav(1, 1, 8, 8000, &[(b"smpl", &smpl)], &[128, 128]);
    assert_eq!(
        decode(&bytes).unwrap_err(),
        WavError::TooManySampleLoops { count: 2 }
    );
}

#[test]
fn unsupported_loop_type_is_rejected() {
    let mut smpl = smpl_chunk(60, 0, 0, 1);
    smpl[40..44].copy_from_slice(&2u32.to_le_bytes()); // backward loop
    let bytes = build_wav(1, 1, 8, 8000, &[(b"smpl", &smpl)], &[128, 128, 128]);
    assert_eq!(
        decode(&bytes).unwrap_err(),
        WavError::UnsupportedLoopType { loop_type: 2 }
    );
}

#[test]
fn truncated_smpl_chunk_is_rejected() {
    let bytes = build_wav(1, 1, 8, 8000, &[(b"smpl", &[0u8; 10])], &[128]);
    assert_eq!(decode(&bytes).unwrap_err(), WavError::TruncatedSmplChunk);
}

#[test]
fn agbl_override_past_the_real_sample_count_is_rejected() {
    let agbl = u32_chunk(100);
    let bytes = build_wav(1, 1, 8, 8000, &[(b"agbl", &agbl)], &[128, 128]);
    assert_eq!(
        decode(&bytes).unwrap_err(),
        WavError::SizeOutOfRange {
            size: 100,
            num_samples: 2,
        }
    );
}

#[test]
fn loop_start_past_the_derived_size_is_rejected() {
    // agbl shrinks size to 1, but the loop starts at sample 2.
    let smpl = smpl_chunk(60, 0, 2, 3);
    let agbl = u32_chunk(1);
    let bytes = build_wav(
        1,
        1,
        8,
        8000,
        &[(b"smpl", &smpl), (b"agbl", &agbl)],
        &[128, 129, 130, 131],
    );
    assert_eq!(
        decode(&bytes).unwrap_err(),
        WavError::LoopStartOutOfRange {
            loop_start: 2,
            size: 1,
        }
    );
}

#[test]
fn sample_format_bytes_per_sample_matches_the_documented_widths() {
    assert_eq!(SampleFormat::U8.bytes_per_sample(), 1);
    assert_eq!(SampleFormat::S16.bytes_per_sample(), 2);
    assert_eq!(SampleFormat::S24.bytes_per_sample(), 3);
    assert_eq!(SampleFormat::S32.bytes_per_sample(), 4);
    assert_eq!(SampleFormat::F32.bytes_per_sample(), 4);
    assert_eq!(SampleFormat::F64.bytes_per_sample(), 8);
}
