//! Compares an offline `mus_title` render with a local mGBA capture.

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

fn rms_error(reference: &[f32], candidate: &[f32], offset: usize) -> f64 {
    assert!(
        reference.len() >= offset + COMPARE_WINDOW && candidate.len() >= COMPARE_WINDOW,
        "rms_error called without a full {COMPARE_WINDOW}-sample window at offset {offset}"
    );
    let sum_sq: f64 = (0..COMPARE_WINDOW)
        .map(|i| {
            let diff = f64::from(reference[offset + i]) - f64::from(candidate[i]);
            diff * diff
        })
        .sum();
    #[expect(
        clippy::cast_precision_loss,
        reason = "the fixed comparison window is exactly representable in f64"
    )]
    let mean = sum_sq / COMPARE_WINDOW as f64;
    mean.sqrt()
}

fn best_alignment(reference: &[f32], candidate: &[f32]) -> usize {
    assert!(
        reference.len() >= ALIGNMENT_SEARCH_WINDOW + COMPARE_WINDOW,
        "reference capture is too short: {} samples per channel, but aligning over \
         {ALIGNMENT_SEARCH_WINDOW} and comparing {COMPARE_WINDOW} needs at least {}. Capture \
         more audio (see this test's doc comment) rather than comparing a shrinking window.",
        reference.len(),
        ALIGNMENT_SEARCH_WINDOW + COMPARE_WINDOW
    );
    assert!(
        candidate.len() >= COMPARE_WINDOW,
        "native render is too short: {} samples per channel, need {COMPARE_WINDOW}",
        candidate.len()
    );
    (0..ALIGNMENT_SEARCH_WINDOW)
        .map(|offset| (offset, rms_error(reference, candidate, offset)))
        .min_by(|(_, a), (_, b)| a.partial_cmp(b).expect("RMS error is always finite"))
        .map(|(offset, _)| offset)
        .expect("ALIGNMENT_SEARCH_WINDOW is nonzero")
}

fn left_channel(interleaved_stereo: &[f32]) -> Vec<f32> {
    interleaved_stereo.iter().copied().step_by(2).collect()
}

/// Capture the title music with the configured mGBA build, then convert it
/// to headerless interleaved-stereo `f32` PCM at the engine mixer rate:
/// `ffmpeg -i capture.wav -ar 13379 -ac 2 -f f32le title_ref.pcm`.
/// Set `POKEEMERALD_RS_MGBA_TITLE_PCM` to that file and run
/// `cargo test -p pokeemerald-rs mgba_reference -- --ignored --nocapture`.
///
/// The comparison searches the first two seconds of the reference for the
/// five-second left-channel window with the lowest RMS error. It rejects
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
    let native_left = left_channel(&native);

    let reference = read_pcm_f32(&path);
    let reference_left = left_channel(&reference);

    let offset = best_alignment(&reference_left, &native_left);
    let error = rms_error(&reference_left, &native_left, offset);
    let reference_rms = rms(&reference_left[offset..offset + COMPARE_WINDOW]);
    assert!(
        reference_rms > MIN_REFERENCE_RMS,
        "the reference capture's aligned window is silent (RMS {reference_rms:.3e} at offset \
         {offset}): it captured no audio, so there is nothing to compare against"
    );
    let tolerance = MAX_RMS_ERROR_FRACTION * reference_rms;
    assert!(
        error < tolerance,
        "native render diverges from the mGBA reference: RMS error {error:.4} at aligned \
         offset {offset} exceeds {MAX_RMS_ERROR_FRACTION} of the reference's own RMS \
         {reference_rms:.4} (tolerance {tolerance:.4})"
    );
}
