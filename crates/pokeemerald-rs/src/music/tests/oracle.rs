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

/// RMS over interleaved stereo, comparing `frames` audio frames of `candidate`
/// against `reference` starting `offset_frames` frames in.
fn rms_error(reference: &[f32], candidate: &[f32], offset_frames: usize, frames: usize) -> f64 {
    let (start, len) = (offset_frames * 2, frames * 2);
    assert!(
        reference.len() >= start + len && candidate.len() >= len,
        "rms_error called without a full {frames}-frame stereo window at frame offset \
         {offset_frames}"
    );
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

/// Frame offset of the lowest-error window. Offsets move whole stereo frames,
/// so the left and right channels can never be aligned independently.
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
