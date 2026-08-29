#![allow(
    clippy::cast_possible_truncation,
    reason = "hand-built WAV fixture lengths fit their u32 fields"
)]

use std::mem::size_of;

use super::{decode, SampleFormat, WavError};

const FIXTURE_MONO_CHANNEL_COUNT: u16 = 1;
const FIXTURE_FMT_CHUNK_SIZE: usize = 16;
const FIXTURE_SMPL_HEADER_SIZE: usize = 36;
const FIXTURE_SMPL_LOOP_RECORD_SIZE: usize = 24;
const FIXTURE_SMPL_MIDI_UNITY_NOTE_OFFSET: usize = 12;
const FIXTURE_SMPL_MIDI_PITCH_FRACTION_OFFSET: usize = 16;
const FIXTURE_SMPL_LOOP_COUNT_OFFSET: usize = 28;
const FIXTURE_SMPL_LOOP_TYPE_OFFSET: usize = 40;
const FIXTURE_SMPL_LOOP_START_OFFSET: usize = 44;
const FIXTURE_SMPL_LOOP_END_OFFSET: usize = 48;
const FIXTURE_FORWARD_LOOP_TYPE: u32 = 0;

#[derive(Clone, Copy)]
struct WavFormat {
    tag: u16,
    block_align: u16,
    bits_per_sample: u16,
}

const PCM_U8: WavFormat = WavFormat {
    tag: 1,
    block_align: 1,
    bits_per_sample: 8,
};
const PCM_S16: WavFormat = WavFormat {
    tag: 1,
    block_align: 2,
    bits_per_sample: 16,
};
const PCM_S24: WavFormat = WavFormat {
    tag: 1,
    block_align: 3,
    bits_per_sample: 24,
};
const IEEE_FLOAT_F32: WavFormat = WavFormat {
    tag: 3,
    block_align: 4,
    bits_per_sample: 32,
};

fn fmt_chunk(format: WavFormat, channel_count: u16, sample_rate: u32) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&format.tag.to_le_bytes());
    body.extend_from_slice(&channel_count.to_le_bytes());
    body.extend_from_slice(&sample_rate.to_le_bytes());
    let byte_rate = sample_rate * u32::from(format.block_align);
    body.extend_from_slice(&byte_rate.to_le_bytes());
    body.extend_from_slice(&format.block_align.to_le_bytes());
    body.extend_from_slice(&format.bits_per_sample.to_le_bytes());
    body
}

fn build_riff(chunks: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut riff_body = Vec::new();
    riff_body.extend_from_slice(b"WAVE");
    for (id, body) in chunks {
        riff_body.extend_from_slice(id);
        riff_body.extend_from_slice(&(body.len() as u32).to_le_bytes());
        riff_body.extend_from_slice(body);
        if body.len() % 2 == 1 {
            riff_body.push(0);
        }
    }

    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(riff_body.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&riff_body);
    bytes
}

fn build_wav(
    format: WavFormat,
    sample_rate: u32,
    extra_chunks: &[(&[u8; 4], &[u8])],
    data: &[u8],
) -> Vec<u8> {
    let mut chunks = vec![(
        *b"fmt ",
        fmt_chunk(format, FIXTURE_MONO_CHANNEL_COUNT, sample_rate),
    )];
    chunks.extend(extra_chunks.iter().map(|(id, body)| (**id, body.to_vec())));
    chunks.push((*b"data", data.to_vec()));
    build_riff(&chunks)
}

fn write_u32(body: &mut [u8], offset: usize, value: u32) {
    body[offset..offset + size_of::<u32>()].copy_from_slice(&value.to_le_bytes());
}

fn smpl_chunk(midi_key: u32, tuning_fraction: u32, loop_start: u32, loop_end: u32) -> Vec<u8> {
    let mut body = vec![0u8; FIXTURE_SMPL_HEADER_SIZE + FIXTURE_SMPL_LOOP_RECORD_SIZE];
    write_u32(&mut body, FIXTURE_SMPL_MIDI_UNITY_NOTE_OFFSET, midi_key);
    write_u32(
        &mut body,
        FIXTURE_SMPL_MIDI_PITCH_FRACTION_OFFSET,
        tuning_fraction,
    );
    write_u32(&mut body, FIXTURE_SMPL_LOOP_COUNT_OFFSET, 1);
    write_u32(
        &mut body,
        FIXTURE_SMPL_LOOP_TYPE_OFFSET,
        FIXTURE_FORWARD_LOOP_TYPE,
    );
    write_u32(&mut body, FIXTURE_SMPL_LOOP_START_OFFSET, loop_start);
    write_u32(&mut body, FIXTURE_SMPL_LOOP_END_OFFSET, loop_end);
    body
}

fn u32_chunk(value: u32) -> Vec<u8> {
    value.to_le_bytes().to_vec()
}

#[test]
fn one_shot_u8_sample_decodes() {
    let data = [118u8, 128, 138, 128, 108];
    let bytes = build_wav(PCM_U8, 3344, &[], &data);
    let sample = decode(&bytes).unwrap();
    assert_eq!(sample.base_frequency, 3344 * 1024);
    assert_eq!(sample.loop_start, None);
    assert_eq!(sample.data, vec![-10, 0, 10, 0, -20]);
}

#[test]
fn looped_sample_uses_inclusive_smpl_end_when_agbl_is_absent() {
    let data = [128u8, 129, 130, 131, 132, 133];
    let smpl = smpl_chunk(60, 0, 1, 3);
    let bytes = build_wav(PCM_U8, 8000, &[(b"smpl", &smpl)], &data);
    let sample = decode(&bytes).unwrap();
    assert_eq!(sample.loop_start, Some(1));
    assert_eq!(sample.data, vec![0, 1, 2, 3]);
}

#[test]
fn agbl_override_shortens_the_decoded_data() {
    let data = [128u8, 129, 130, 131, 132, 133];
    let smpl = smpl_chunk(60, 0, 1, 3);
    let agbl = u32_chunk(2);
    let bytes = build_wav(PCM_U8, 8000, &[(b"smpl", &smpl), (b"agbl", &agbl)], &data);
    let sample = decode(&bytes).unwrap();
    assert_eq!(sample.data, vec![0, 1]);
    assert_eq!(sample.loop_start, Some(1));
}

#[test]
fn agbp_override_replaces_the_computed_pitch() {
    let data = [128u8, 129];
    let exact_pitch = 0x0034_2000;
    let agbp = u32_chunk(exact_pitch);
    let bytes = build_wav(PCM_U8, 3344, &[(b"agbp", &agbp)], &data);
    let sample = decode(&bytes).unwrap();
    assert_eq!(sample.base_frequency, exact_pitch);
}

#[test]
fn zero_agbp_falls_back_to_the_computed_pitch() {
    let data = [128u8, 129];
    let agbp = u32_chunk(0);
    let bytes = build_wav(PCM_U8, 3344, &[(b"agbp", &agbp)], &data);
    let sample = decode(&bytes).unwrap();
    assert_eq!(sample.base_frequency, 3344 * 1024);
}

#[test]
fn trailing_partial_chunk_header_is_rejected() {
    let data = [128u8, 129];
    let mut bytes = build_wav(PCM_U8, 8000, &[], &data);
    let partial_chunk_header = b"junk\x02";
    bytes.extend_from_slice(partial_chunk_header);
    let riff_len = (bytes.len() - 8) as u32;
    bytes[4..8].copy_from_slice(&riff_len.to_le_bytes());
    assert_eq!(decode(&bytes).unwrap_err(), WavError::ChunkTruncated);
}

#[test]
fn short_smpl_loop_record_is_rejected() {
    let data = [128u8, 129, 130, 131, 132];
    let missing_record_tail_size = FIXTURE_SMPL_HEADER_SIZE + FIXTURE_SMPL_LOOP_RECORD_SIZE - 4;
    let smpl = &smpl_chunk(60, 0, 1, 3)[..missing_record_tail_size];
    let bytes = build_wav(PCM_U8, 8000, &[(b"smpl", smpl)], &data);
    assert_eq!(decode(&bytes).unwrap_err(), WavError::TruncatedSmplChunk);
}

#[test]
fn truncated_override_chunk_is_rejected() {
    let data = [128u8, 129];
    for id in [b"agbp", b"agbl"] {
        let bytes = build_wav(PCM_U8, 3344, &[(id, &[0u8, 0])], &data);
        assert_eq!(decode(&bytes).unwrap_err(), WavError::ChunkTruncated);
    }
}

#[test]
fn zero_agbl_falls_back_to_the_inclusive_smpl_end() {
    let data = [128u8, 129, 130, 131, 132, 133];
    let smpl = smpl_chunk(60, 0, 1, 3);
    let agbl = u32_chunk(0);
    let bytes = build_wav(PCM_U8, 8000, &[(b"smpl", &smpl), (b"agbl", &agbl)], &data);
    let sample = decode(&bytes).unwrap();
    assert_eq!(sample.data, vec![0, 1, 2, 3]);
    assert_eq!(sample.loop_start, Some(1));
}

#[test]
fn midi_key_one_octave_above_unity_halves_the_computed_pitch() {
    let data = [128u8, 129];
    let smpl = smpl_chunk(72, 0, 0, 0);
    let bytes = build_wav(PCM_U8, 8000, &[(b"smpl", &smpl)], &data);
    let sample = decode(&bytes).unwrap();
    assert_eq!(sample.base_frequency, 4000 * 1024);
}

#[test]
fn s16_format_decodes() {
    let mut data = Vec::new();
    data.extend_from_slice(&i16::MIN.to_le_bytes());
    data.extend_from_slice(&0i16.to_le_bytes());
    data.extend_from_slice(&i16::MAX.to_le_bytes());
    let bytes = build_wav(PCM_S16, 22050, &[], &data);
    let sample = decode(&bytes).unwrap();
    assert_eq!(sample.data, vec![-128, 0, 127]);
}

#[test]
fn s24_format_sign_extends_and_decodes() {
    let data = [0x00, 0x00, 0x80, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0x7F];
    let bytes = build_wav(PCM_S24, 22050, &[], &data);
    let sample = decode(&bytes).unwrap();
    assert_eq!(sample.data, vec![-128, 0, 127]);
}

#[test]
fn f32_format_decodes() {
    let mut data = Vec::new();
    data.extend_from_slice(&(-1.0f32).to_le_bytes());
    data.extend_from_slice(&0.0f32.to_le_bytes());
    let bytes = build_wav(IEEE_FLOAT_F32, 22050, &[], &data);
    let sample = decode(&bytes).unwrap();
    assert_eq!(sample.data, vec![-128, 0]);
}

#[test]
fn bad_riff_magic_is_rejected() {
    let mut bytes = build_wav(PCM_U8, 8000, &[], &[128]);
    bytes[0] = b'X';
    assert_eq!(decode(&bytes).unwrap_err(), WavError::BadRiffMagic);
}

#[test]
fn riff_size_mismatch_is_rejected() {
    let mut bytes = build_wav(PCM_U8, 8000, &[], &[128]);
    bytes.push(0xFF);
    assert!(matches!(
        decode(&bytes).unwrap_err(),
        WavError::RiffSizeMismatch { .. }
    ));
}

#[test]
fn bad_wave_magic_is_rejected() {
    let mut bytes = build_wav(PCM_U8, 8000, &[], &[128]);
    bytes[8..12].copy_from_slice(b"WAVX");
    assert_eq!(decode(&bytes).unwrap_err(), WavError::BadWaveMagic);
}

#[test]
fn missing_fmt_chunk_is_rejected() {
    let bytes = build_riff(&[(*b"data", vec![1, 2, 3, 4])]);
    assert_eq!(decode(&bytes).unwrap_err(), WavError::MissingFmtChunk);
}

#[test]
fn truncated_fmt_chunk_is_rejected() {
    let short_fmt = vec![0u8; FIXTURE_FMT_CHUNK_SIZE / 2];
    let bytes = build_riff(&[(*b"fmt ", short_fmt)]);
    assert_eq!(decode(&bytes).unwrap_err(), WavError::ChunkTruncated);
}

#[test]
fn missing_data_chunk_is_rejected() {
    let fmt = fmt_chunk(PCM_U8, FIXTURE_MONO_CHANNEL_COUNT, 8000);
    let bytes = build_riff(&[(*b"fmt ", fmt)]);
    assert_eq!(decode(&bytes).unwrap_err(), WavError::MissingDataChunk);
}

#[test]
fn stereo_is_rejected() {
    let stereo_channel_count = 2;
    let fmt = fmt_chunk(PCM_U8, stereo_channel_count, 8000);
    let bytes = build_riff(&[(*b"fmt ", fmt), (*b"data", vec![128, 128])]);
    assert_eq!(
        decode(&bytes).unwrap_err(),
        WavError::NotMono {
            channels: stereo_channel_count
        }
    );
}

#[test]
fn unsupported_format_tag_is_rejected() {
    let adpcm = WavFormat {
        tag: 2,
        block_align: 1,
        bits_per_sample: 8,
    };
    let bytes = build_wav(adpcm, 8000, &[], &[128]);
    assert_eq!(
        decode(&bytes).unwrap_err(),
        WavError::UnsupportedFormat {
            format_tag: adpcm.tag,
            block_align: adpcm.block_align,
            bits_per_sample: adpcm.bits_per_sample,
        }
    );
}

#[test]
fn unsupported_bit_depth_combination_is_rejected() {
    let unsupported_pcm = WavFormat {
        tag: 1,
        block_align: 5,
        bits_per_sample: 40,
    };
    let bytes = build_wav(unsupported_pcm, 8000, &[], &[128; 5]);
    assert!(matches!(
        decode(&bytes).unwrap_err(),
        WavError::UnsupportedFormat { .. }
    ));
}

#[test]
fn misaligned_data_length_is_rejected() {
    let bytes = build_wav(PCM_S16, 8000, &[], &[1, 2, 3]);
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
    let unsupported_loop_count = 2;
    let mut smpl = vec![0u8; FIXTURE_SMPL_HEADER_SIZE];
    write_u32(
        &mut smpl,
        FIXTURE_SMPL_LOOP_COUNT_OFFSET,
        unsupported_loop_count,
    );
    let bytes = build_wav(PCM_U8, 8000, &[(b"smpl", &smpl)], &[128, 128]);
    assert_eq!(
        decode(&bytes).unwrap_err(),
        WavError::TooManySampleLoops {
            count: unsupported_loop_count
        }
    );
}

#[test]
fn unsupported_loop_type_is_rejected() {
    let backward_loop_type = 2;
    let mut smpl = smpl_chunk(60, 0, 0, 1);
    write_u32(&mut smpl, FIXTURE_SMPL_LOOP_TYPE_OFFSET, backward_loop_type);
    let bytes = build_wav(PCM_U8, 8000, &[(b"smpl", &smpl)], &[128, 128, 128]);
    assert_eq!(
        decode(&bytes).unwrap_err(),
        WavError::UnsupportedLoopType {
            loop_type: backward_loop_type
        }
    );
}

#[test]
fn truncated_smpl_chunk_is_rejected() {
    let bytes = build_wav(PCM_U8, 8000, &[(b"smpl", &[0u8; 10])], &[128]);
    assert_eq!(decode(&bytes).unwrap_err(), WavError::TruncatedSmplChunk);
}

#[test]
fn agbl_override_past_the_real_sample_count_is_rejected() {
    let unavailable_sample_count = 100;
    let agbl = u32_chunk(unavailable_sample_count);
    let bytes = build_wav(PCM_U8, 8000, &[(b"agbl", &agbl)], &[128, 128]);
    assert_eq!(
        decode(&bytes).unwrap_err(),
        WavError::SizeOutOfRange {
            size: unavailable_sample_count,
            num_samples: 2,
        }
    );
}

#[test]
fn loop_start_at_or_past_the_derived_size_is_rejected() {
    let smpl = smpl_chunk(60, 0, 2, 3);
    let agbl = u32_chunk(1);
    let bytes = build_wav(
        PCM_U8,
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
fn each_sample_format_reports_its_encoded_width() {
    assert_eq!(SampleFormat::U8.bytes_per_sample(), 1);
    assert_eq!(SampleFormat::S16.bytes_per_sample(), 2);
    assert_eq!(SampleFormat::S24.bytes_per_sample(), 3);
    assert_eq!(SampleFormat::S32.bytes_per_sample(), 4);
    assert_eq!(SampleFormat::F32.bytes_per_sample(), 4);
    assert_eq!(SampleFormat::F64.bytes_per_sample(), 8);
}
