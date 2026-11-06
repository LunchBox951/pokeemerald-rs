//! Compares an offline `mus_title` render with a local mGBA capture.

use std::cell::Cell;
use std::env;
use std::fs;

use audio::{Sequencer, MIXER_RATE};

use super::super::load_song_from_pack;

const ALIGNMENT_SEARCH_SECONDS: usize = 2;
const COMPARE_SECONDS: usize = 5;
const ALIGNMENT_SEARCH_WINDOW: usize = MIXER_RATE as usize * ALIGNMENT_SEARCH_SECONDS;
const COMPARE_WINDOW: usize = MIXER_RATE as usize * COMPARE_SECONDS;
const MAX_RMS_ERROR_FRACTION: f64 = 0.25;
const MIN_REFERENCE_RMS: f64 = 1e-4;
/// Spacing of the cross-correlation samples that seed the alignment search.
const SEED_STRIDE_FRAMES: usize = 128;
/// Frames between prefix-error checks that drop a losing alignment.
const PRUNE_BLOCK_FRAMES: usize = 64;

thread_local! {
    /// Squared-difference terms summed on this thread by the error functions.
    static ERROR_TERMS: Cell<usize> = const { Cell::new(0) };
}

fn count_error_terms(terms: usize) {
    ERROR_TERMS.with(|count| count.set(count.get() + terms));
}

fn read_pcm_f32(path: &str) -> Vec<f32> {
    let bytes = fs::read(path).unwrap_or_else(|e| panic!("reading `{path}`: {e}"));
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn rms(xs: &[f32]) -> f64 {
    assert!(!xs.is_empty(), "RMS of an empty window");
    let sum_sq: f64 = xs.iter().map(|&s| f64::from(s) * f64::from(s)).sum();
    #[expect(
        clippy::cast_precision_loss,
        reason = "the fixed comparison windows are exactly representable in f64"
    )]
    let mean = sum_sq / xs.len() as f64;
    mean.sqrt()
}

/// RMS over interleaved stereo, comparing `frames` audio frames of `candidate`
/// against `reference` starting `offset_frames` frames in.
fn rms_error(reference: &[f32], candidate: &[f32], offset_frames: usize, frames: usize) -> f64 {
    let (start, len) = (offset_frames * 2, frames * 2);
    assert!(
        reference.len() >= start + len && candidate.len() >= len,
        "rms_error called without a full {frames}-frame stereo window at frame offset \
         {offset_frames}"
    );
    count_error_terms(len);
    let sum_sq: f64 = reference[start..start + len]
        .iter()
        .zip(&candidate[..len])
        .map(|(&r, &c)| {
            let diff = f64::from(r) - f64::from(c);
            diff * diff
        })
        .sum();
    #[expect(
        clippy::cast_precision_loss,
        reason = "the comparison windows are exactly representable in f64"
    )]
    let mean = sum_sq / len as f64;
    mean.sqrt()
}

/// RMS of an accumulated squared-error sum over `len` interleaved samples.
fn rms_of_sum(sum_sq: f64, len: usize) -> f64 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "the comparison windows are exactly representable in f64"
    )]
    let mean = sum_sq / len as f64;
    mean.sqrt()
}

/// [`rms_error`] that gives up as soon as the running prefix already exceeds
/// `ceiling`. The squared differences are added in the same left-to-right
/// order as `rms_error`, so a finished window returns the identical value and
/// a prefix can only grow: `None` means the full RMS error is strictly greater
/// than `ceiling`.
fn rms_error_within(
    reference: &[f32],
    candidate: &[f32],
    offset_frames: usize,
    frames: usize,
    ceiling: f64,
) -> Option<f64> {
    let (start, len) = (offset_frames * 2, frames * 2);
    let reference = &reference[start..start + len];
    let candidate = &candidate[..len];
    let mut sum_sq = 0.0_f64;
    for (r_block, c_block) in reference
        .chunks(PRUNE_BLOCK_FRAMES * 2)
        .zip(candidate.chunks(PRUNE_BLOCK_FRAMES * 2))
    {
        count_error_terms(r_block.len());
        for (&r, &c) in r_block.iter().zip(c_block) {
            let diff = f64::from(r) - f64::from(c);
            sum_sq += diff * diff;
        }
        if rms_of_sum(sum_sq, len) > ceiling {
            return None;
        }
    }
    Some(rms_of_sum(sum_sq, len))
}

/// Offset with the lowest estimated squared error, from a stereo
/// cross-correlation sampled every [`SEED_STRIDE_FRAMES`] frames. The error
/// `sum((r - n)^2)` is `sum(r^2) - 2 * sum(r * n) + sum(n^2)`: the candidate
/// energy is the same at every offset and drops out, the reference energy
/// rolls one frame per offset, and the cross term over both channels is
/// sampled. Only a starting guess for [`best_alignment`], never its answer.
fn alignment_seed(
    reference: &[f32],
    candidate: &[f32],
    search_frames: usize,
    compare_frames: usize,
) -> usize {
    let frame_energy = |frame: usize| -> f64 {
        let (l, r) = (reference[2 * frame], reference[2 * frame + 1]);
        f64::from(l) * f64::from(l) + f64::from(r) * f64::from(r)
    };
    let sampled = compare_frames.div_ceil(SEED_STRIDE_FRAMES);
    #[expect(
        clippy::cast_precision_loss,
        reason = "frame counts are far below 2^53, so both are exact in f64"
    )]
    let scale = compare_frames as f64 / sampled as f64;
    let mut energy: f64 = (0..compare_frames).map(frame_energy).sum();
    let mut best = (0, f64::INFINITY);
    for offset in 0..search_frames {
        let cross: f64 = (0..compare_frames)
            .step_by(SEED_STRIDE_FRAMES)
            .map(|k| {
                let r = 2 * (offset + k);
                f64::from(reference[r]) * f64::from(candidate[2 * k])
                    + f64::from(reference[r + 1]) * f64::from(candidate[2 * k + 1])
            })
            .sum();
        let score = energy - 2.0 * scale * cross;
        if score < best.1 {
            best = (offset, score);
        }
        if offset + 1 < search_frames {
            energy += frame_energy(offset + compare_frames) - frame_energy(offset);
        }
    }
    best.0
}

/// Frame offset of the lowest-error window, the earliest one on equal error,
/// identical to scanning every offset with [`rms_error`]. Offsets move whole
/// stereo frames, so the left and right channels can never be aligned
/// independently.
///
/// A stereo cross-correlation sampled every [`SEED_STRIDE_FRAMES`] frames,
/// with a rolling reference-energy sum, only picks the first incumbent; every
/// offset stays eligible. A window is dropped once its error over a prefix,
/// checked every [`PRUNE_BLOCK_FRAMES`] frames, strictly exceeds the
/// incumbent's. The worst case, such as unequal constant signals that nothing
/// prunes, stays `O(search_frames * compare_frames)`.
///
/// # Panics
///
/// On a window too short for the search or on a non-finite sample.
fn best_alignment(
    reference: &[f32],
    candidate: &[f32],
    search_frames: usize,
    compare_frames: usize,
) -> usize {
    assert!(
        reference.len() >= 2 * (search_frames + compare_frames),
        "reference capture is too short: {} stereo frames, but aligning over {search_frames} \
         and comparing {compare_frames} needs at least {}. Capture more audio (see this test's \
         doc comment) rather than comparing a shrinking window.",
        reference.len() / 2,
        search_frames + compare_frames
    );
    assert!(
        candidate.len() >= 2 * compare_frames,
        "native render is too short: {} stereo frames, need {compare_frames}",
        candidate.len() / 2
    );
    assert!(
        reference[..2 * (search_frames + compare_frames)]
            .iter()
            .chain(&candidate[..2 * compare_frames])
            .all(|sample| sample.is_finite()),
        "the compared audio holds a NaN or infinite sample"
    );
    assert!(search_frames > 0, "the alignment search window is nonzero");
    assert!(compare_frames > 0, "the comparison window is nonzero");
    let mut best_offset = alignment_seed(reference, candidate, search_frames, compare_frames);
    let mut best_error = rms_error(reference, candidate, best_offset, compare_frames);
    for offset in 0..search_frames {
        if best_error.total_cmp(&0.0).is_eq() && offset >= best_offset {
            break;
        }
        if let Some(error) =
            rms_error_within(reference, candidate, offset, compare_frames, best_error)
        {
            let ordering = error.total_cmp(&best_error);
            if ordering.is_lt() || (ordering.is_eq() && offset < best_offset) {
                (best_offset, best_error) = (offset, error);
            }
        }
    }
    best_offset
}

/// Aligns both interleaved-stereo buffers on whole frames, then rejects a
/// silent reference or an error above the tolerance over both channels.
fn check_against_reference(
    reference: &[f32],
    native: &[f32],
    search_frames: usize,
    compare_frames: usize,
) -> Result<(), String> {
    let offset = best_alignment(reference, native, search_frames, compare_frames);
    let error = rms_error(reference, native, offset, compare_frames);
    let reference_rms = rms(&reference[2 * offset..2 * (offset + compare_frames)]);
    if reference_rms <= MIN_REFERENCE_RMS {
        return Err(format!(
            "the reference capture's aligned window is silent (RMS {reference_rms:.3e} at \
             offset {offset}): it captured no audio, so there is nothing to compare against"
        ));
    }
    let tolerance = MAX_RMS_ERROR_FRACTION * reference_rms;
    if error >= tolerance {
        return Err(format!(
            "native render diverges from the mGBA reference: RMS error {error:.4} at aligned \
             offset {offset} exceeds {MAX_RMS_ERROR_FRACTION} of the reference's own RMS \
             {reference_rms:.4} (tolerance {tolerance:.4})"
        ));
    }
    Ok(())
}

/// Capture the title music with the configured mGBA build, then convert it
/// to headerless interleaved-stereo `f32` PCM at the engine mixer rate:
/// `ffmpeg -i capture.wav -ar 13379 -ac 2 -f f32le title_ref.pcm`.
/// Set `POKEEMERALD_RS_MGBA_TITLE_PCM` to that file and run
/// `cargo test -p pokeemerald-rs mgba_reference -- --ignored --nocapture`.
///
/// The comparison searches the first two seconds of the reference for the
/// five-second stereo window with the lowest RMS error across both channels. It rejects
/// silent references and treats error above 25% of the reference RMS as
/// gross behavioural divergence, not sample-exact inequality.
#[test]
#[ignore = "needs a local pack and a local mGBA reference capture: see this test's doc comment"]
fn native_render_matches_local_mgba_reference_within_tolerance() {
    let Ok(path) = env::var("POKEEMERALD_RS_MGBA_TITLE_PCM") else {
        eprintln!(
            "skipped: set POKEEMERALD_RS_MGBA_TITLE_PCM to a local mGBA reference capture \
             (interleaved-stereo f32 PCM at MIXER_RATE) -- see this test's doc comment for \
             how to produce one"
        );
        return;
    };

    let pack = assets::AssetPack::load_repo().expect("run `cargo xtask extract` first");
    let song = load_song_from_pack(&pack, "mus_title").expect("mus_title must resolve cleanly");
    let mut seq = Sequencer::new(song);

    let native_frames =
        (ALIGNMENT_SEARCH_WINDOW + COMPARE_WINDOW).div_ceil(audio::SAMPLES_PER_FRAME);
    let mut native = vec![0.0_f32; native_frames * Sequencer::FRAME_SAMPLES];
    seq.mix_into(&mut native);
    let reference = read_pcm_f32(&path);
    if let Err(message) =
        check_against_reference(&reference, &native, ALIGNMENT_SEARCH_WINDOW, COMPARE_WINDOW)
    {
        panic!("{message}");
    }
}

#[test]
fn oracle_rejects_right_channel_divergence() {
    let reference: Vec<f32> = [0.5, 0.5].repeat(8);
    let native: Vec<f32> = [0.5, -0.5].repeat(8);
    let message = check_against_reference(&reference, &native, 2, 4)
        .expect_err("a diverging right channel must fail the oracle");
    assert!(message.contains("diverges"), "{message}");
}

#[test]
fn oracle_accepts_matching_stereo() {
    let reference: Vec<f32> = [0.5, -0.25].repeat(8);
    check_against_reference(&reference, &reference, 2, 4).expect("identical stereo must pass");
}

#[test]
fn oracle_aligns_on_the_right_channel() {
    // Left is constant, so only the right channel identifies the offset.
    let reference: Vec<f32> = [0.0, 0.0, 0.5, 0.1, 0.5, 0.2, 0.5, 0.3, 0.5, 0.4, 0.5, 0.5].to_vec();
    let native = &reference[4..];
    assert_eq!(best_alignment(&reference, native, 3, 3), 2);
}

#[test]
fn oracle_accepts_right_only_reference_and_rejects_silence() {
    let right_only: Vec<f32> = [0.0, 0.5].repeat(8);
    check_against_reference(&right_only, &right_only, 2, 4).expect("right-only audio is audible");
    let silent = vec![0.0_f32; 16];
    let message = check_against_reference(&silent, &silent, 2, 4).expect_err("silence rejected");
    assert!(message.contains("silent"), "{message}");
}

/// Exhaustive reference for [`best_alignment`]: every offset, first minimum.
fn brute_force_alignment(
    reference: &[f32],
    candidate: &[f32],
    search_frames: usize,
    compare_frames: usize,
) -> usize {
    (0..search_frames)
        .map(|offset| {
            (
                offset,
                rms_error(reference, candidate, offset, compare_frames),
            )
        })
        .min_by(|(_, a), (_, b)| a.partial_cmp(b).expect("RMS error is always finite"))
        .map(|(offset, _)| offset)
        .expect("the alignment search window is nonzero")
}

/// Asserts the fast search picks exactly the exhaustive scan's offset.
fn assert_matches_brute_force(
    reference: &[f32],
    candidate: &[f32],
    search_frames: usize,
    compare_frames: usize,
) -> usize {
    let fast = best_alignment(reference, candidate, search_frames, compare_frames);
    let slow = brute_force_alignment(reference, candidate, search_frames, compare_frames);
    assert_eq!(fast, slow, "fast search diverged from the exhaustive scan");
    fast
}

fn stereo(frames: &[(f32, f32)]) -> Vec<f32> {
    frames.iter().flat_map(|&(l, r)| [l, r]).collect()
}

/// Deterministic samples in `[-1, 1)` from a linear congruential generator.
fn lcg_samples(seed: u32, count: usize) -> Vec<f32> {
    let mut state = seed;
    (0..count)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            f32::from(u16::try_from(state >> 16).expect("16 bits")) / 32768.0 - 1.0
        })
        .collect()
}

#[test]
fn fast_alignment_finds_planted_stereo_offsets() {
    let reference = stereo(&[
        (0.0, 0.0),
        (1.0, 2.0),
        (-2.0, 1.0),
        (3.0, -1.0),
        (0.5, 4.0),
        (-1.0, -3.0),
        (2.0, 0.5),
        (4.0, 2.0),
    ]);
    for planted in [0, 1, 3] {
        let candidate = &reference[2 * planted..2 * (planted + 4)];
        assert_eq!(
            assert_matches_brute_force(&reference, candidate, 4, 4),
            planted
        );
    }
}

#[test]
fn fast_alignment_finds_right_only_offset() {
    let reference = stereo(&[
        (0.0, -2.0),
        (0.0, 0.5),
        (0.0, 3.0),
        (0.0, -1.0),
        (0.0, 2.0),
        (0.0, 4.0),
    ]);
    assert_eq!(
        assert_matches_brute_force(&reference, &reference[4..], 3, 3),
        2
    );
}

#[test]
fn fast_alignment_on_silence_picks_the_first_offset() {
    let reference = vec![0.0_f32; 16];
    let candidate = vec![0.0_f32; 8];
    assert_eq!(assert_matches_brute_force(&reference, &candidate, 4, 4), 0);
}

#[test]
fn fast_alignment_keeps_the_earliest_of_equal_nonzero_errors() {
    let reference = stereo(&[(0.25, -0.5); 8]);
    let candidate = stereo(&[(-0.25, 0.5); 4]);
    assert_eq!(assert_matches_brute_force(&reference, &candidate, 4, 4), 0);
}

#[test]
fn fast_alignment_is_global_when_the_seed_samples_miss_the_signal() {
    // Only right-channel impulses carry information, and the candidate's
    // lies between the sampled frames, so the sampled correlation sees none
    // of it and the exact scan has to find the alignment.
    let (search, compare) = (260, 257);
    let mut reference = vec![0.0_f32; 2 * (search + compare)];
    reference[2 * 146 + 1] = 1.0;
    let mut candidate = vec![0.0_f32; 2 * compare];
    candidate[2 * 17 + 1] = 1.0;
    assert_eq!(
        assert_matches_brute_force(&reference, &candidate, search, compare),
        129
    );
}

#[test]
fn fast_alignment_matches_brute_force_on_long_random_audio() {
    let (search, compare, planted) = (769, 4097, 513);
    let reference = lcg_samples(0x3115, 2 * (search + compare));
    let exact = &reference[2 * planted..2 * (planted + compare)];
    assert_eq!(
        assert_matches_brute_force(&reference, exact, search, compare),
        planted
    );

    let noise = lcg_samples(0x5113, 2 * compare);
    let perturbed: Vec<f32> = exact
        .iter()
        .zip(&noise)
        .map(|(&s, &n)| s + 0.001 * n)
        .collect();
    assert_eq!(
        assert_matches_brute_force(&reference, &perturbed, search, compare),
        planted
    );
}

#[test]
fn fast_alignment_matches_brute_force_on_unrelated_audio() {
    let (search, compare) = (97, 300);
    let reference = lcg_samples(7, 2 * (search + compare));
    let candidate = lcg_samples(11, 2 * compare);
    assert_matches_brute_force(&reference, &candidate, search, compare);
}

#[test]
#[should_panic(expected = "NaN or infinite")]
fn oracle_rejects_non_finite_samples() {
    let _ = check_against_reference(&[f32::NAN; 16], &[0.0; 8], 2, 4);
}

#[test]
fn fast_alignment_prunes_most_of_the_exhaustive_work() {
    let (search, compare, planted) = (769, 4097, 513);
    let reference = lcg_samples(0x3115, 2 * (search + compare));
    let candidate = &reference[2 * planted..2 * (planted + compare)];
    ERROR_TERMS.with(|c| c.set(0));
    assert_eq!(
        best_alignment(&reference, candidate, search, compare),
        planted
    );
    let terms = ERROR_TERMS.with(Cell::get);
    let exhaustive = 2 * search * compare;
    assert!(
        terms * 10 < exhaustive,
        "alignment summed {terms} error terms; the exhaustive scan sums {exhaustive}"
    );
}
