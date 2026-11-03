//! Repeated WAV chunks resolve like wav2agb: the last matching chunk wins and every one is validated.

use super::*;

// Repeated chunks follow wav2agb's overwrite order: the last matching chunk
// is effective (`wav_file.cpp:125-192`).

#[test]
fn a_short_override_chunk_before_a_valid_one_yields_the_valid_one() {
    let smpl = smpl_chunk(60, 0, 0, 3);
    let data = [128u8, 128, 128, 128];
    let valid_pitch = u32_chunk(0x0034_2000);
    let valid_len = u32_chunk(2);
    let bytes = build_wav(
        PCM_U8,
        8000,
        &[
            (b"smpl", &smpl),
            (b"agbp", &[0u8, 0]),
            (b"agbp", &valid_pitch),
            (b"agbl", &[0u8, 0]),
            (b"agbl", &valid_len),
        ],
        &data,
    );
    let sample = decode(&bytes).unwrap();
    assert_eq!(sample.base_frequency, 0x0034_2000);
    assert_eq!(sample.sample_count, 2);
}

#[test]
fn a_later_short_override_chunk_keeps_the_earlier_valid_override_like_wav2agb() {
    // wav2agb only assigns agbPitch/agbLoopEnd when chunkLen >= 4
    // (wav_file.cpp:183-193), so a later short chunk leaves the earlier value.
    let smpl = smpl_chunk(60, 0, 0, 3);
    let data = [128u8, 128, 128, 128];
    let valid_pitch = u32_chunk(0x0034_2000);
    let valid_len = u32_chunk(2);
    let bytes = build_wav(
        PCM_U8,
        8000,
        &[
            (b"smpl", &smpl),
            (b"agbp", &valid_pitch),
            (b"agbp", &[0u8, 0]),
            (b"agbl", &valid_len),
            (b"agbl", &[0u8, 0]),
        ],
        &data,
    );
    let sample = decode(&bytes).unwrap();
    assert_eq!(sample.base_frequency, 0x0034_2000);
    assert_eq!(sample.sample_count, 2);
}

#[test]
fn duplicate_fmt_and_data_chunks_use_the_last_like_wav2agb() {
    let bytes = build_riff(&[
        (
            *b"fmt ",
            fmt_chunk(PCM_U8, FIXTURE_MONO_CHANNEL_COUNT, 8000),
        ),
        (*b"data", vec![128u8, 128]),
        (
            *b"fmt ",
            fmt_chunk(PCM_U8, FIXTURE_MONO_CHANNEL_COUNT, 4000),
        ),
        (*b"data", vec![138u8, 118, 128]),
    ]);
    let sample = decode(&bytes).unwrap();
    assert_eq!(sample.base_frequency, 4000 * 1024);
    assert_eq!(sample.sample_count, 3);
    assert_eq!(sample.data, vec![10, -10, 0, 0]);
}

#[test]
fn a_later_fmt_chunk_replaces_the_sample_width_of_an_earlier_one() {
    let bytes = build_riff(&[
        (
            *b"fmt ",
            fmt_chunk(PCM_U8, FIXTURE_MONO_CHANNEL_COUNT, 8000),
        ),
        (
            *b"fmt ",
            fmt_chunk(PCM_S16, FIXTURE_MONO_CHANNEL_COUNT, 8000),
        ),
        (*b"data", vec![0u8, 0, 0, 0]),
    ]);
    assert_eq!(decode(&bytes).unwrap().sample_count, 2);
}

#[test]
fn every_fmt_chunk_is_validated_even_when_an_earlier_one_is_not_effective() {
    let mut bad = fmt_chunk(PCM_U8, FIXTURE_MONO_CHANNEL_COUNT, 8000);
    bad[2] = 2; // stereo
    let bytes = build_riff(&[
        (*b"fmt ", bad),
        (
            *b"fmt ",
            fmt_chunk(PCM_U8, FIXTURE_MONO_CHANNEL_COUNT, 8000),
        ),
        (*b"data", vec![128u8]),
    ]);
    assert!(matches!(decode(&bytes), Err(WavError::NotMono { .. })));
}

#[test]
fn duplicate_smpl_chunks_use_the_last() {
    let first = smpl_chunk(60, 0, 0, 1);
    let last = smpl_chunk(70, 0, 1, 2);
    let bytes = build_wav(
        PCM_U8,
        8000,
        &[(b"smpl", &first), (b"smpl", &last)],
        &[128u8, 128, 128, 128],
    );
    let sample = decode(&bytes).unwrap();
    assert_eq!(sample.loop_start, Some(1));
    assert_eq!(sample.sample_count, 3);
    let single = build_wav(PCM_U8, 8000, &[(b"smpl", &last)], &[128u8, 128, 128, 128]);
    assert_eq!(
        sample.base_frequency,
        decode(&single).unwrap().base_frequency
    );
    let only_first = build_wav(PCM_U8, 8000, &[(b"smpl", &first)], &[128u8, 128, 128, 128]);
    assert_ne!(
        sample.base_frequency,
        decode(&only_first).unwrap().base_frequency
    );
}

#[test]
fn duplicate_agbp_and_agbl_chunks_use_the_last() {
    let (p1, p2) = (u32_chunk(0x0010_0000), u32_chunk(0x0034_2000));
    let (l1, l2) = (u32_chunk(1), u32_chunk(3));
    let smpl = smpl_chunk(60, 0, 0, 3);
    let bytes = build_wav(
        PCM_U8,
        8000,
        &[
            (b"smpl", &smpl),
            (b"agbp", &p1),
            (b"agbp", &p2),
            (b"agbl", &l1),
            (b"agbl", &l2),
        ],
        &[128u8, 128, 128, 128],
    );
    let sample = decode(&bytes).unwrap();
    assert_eq!(sample.base_frequency, 0x0034_2000);
    assert_eq!(sample.sample_count, 3);
}

#[test]
fn a_later_zero_loop_smpl_keeps_the_earlier_loop_but_takes_its_key() {
    let looped = smpl_chunk(60, 0, 1, 2);
    let mut unlooped = smpl_chunk(72, 0, 0, 0);
    write_u32(&mut unlooped, FIXTURE_SMPL_LOOP_COUNT_OFFSET, 0);
    let data = [128u8, 128, 128, 128];
    let bytes = build_wav(
        PCM_U8,
        8000,
        &[(b"smpl", &looped), (b"smpl", &unlooped)],
        &data,
    );
    let sample = decode(&bytes).unwrap();
    assert_eq!(sample.loop_start, Some(1));
    assert_eq!(sample.sample_count, 3);
    let key_only = build_wav(PCM_U8, 8000, &[(b"smpl", &unlooped)], &data);
    assert_eq!(
        sample.base_frequency,
        decode(&key_only).unwrap().base_frequency
    );
}

#[test]
fn every_smpl_chunk_is_validated_even_when_a_later_one_is_effective() {
    let good = smpl_chunk(60, 0, 0, 1);
    let mut bad = smpl_chunk(60, 0, 0, 1);
    write_u32(&mut bad, FIXTURE_SMPL_LOOP_COUNT_OFFSET, 2);
    let bytes = build_wav(
        PCM_U8,
        8000,
        &[(b"smpl", &good), (b"smpl", &bad), (b"smpl", &good)],
        &[128u8, 128],
    );
    assert!(matches!(
        decode(&bytes),
        Err(WavError::TooManySampleLoops { count: 2 })
    ));
}

#[test]
fn a_later_invalid_fmt_chunk_is_rejected_after_a_valid_one() {
    let mut bad = fmt_chunk(PCM_U8, FIXTURE_MONO_CHANNEL_COUNT, 8000);
    bad[2] = 2; // stereo
    let bytes = build_riff(&[
        (
            *b"fmt ",
            fmt_chunk(PCM_U8, FIXTURE_MONO_CHANNEL_COUNT, 8000),
        ),
        (*b"fmt ", bad),
        (*b"data", vec![128u8]),
    ]);
    assert!(matches!(decode(&bytes), Err(WavError::NotMono { .. })));
}
