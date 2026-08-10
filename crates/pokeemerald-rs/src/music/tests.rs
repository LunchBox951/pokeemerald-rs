//! Unit tests for [`super`]/[`super::player`]: a synthetic, pack-free
//! [`MusicPlayer`] round-trip (continuous playback, restart-on-finish, no
//! underrun when drained each frame), plus real-pack coverage of
//! [`load_song_from_pack`] against `mus_title`.
//!
//! The ignored, env-gated mGBA reference comparison (Discussion #227's
//! fidelity oracle) lives in [`oracle`] below.

use std::sync::Arc;

use audio::{Adsr, Event, Instrument, Sequencer, Song, ToneData, WaveData};
use platform::AudioOutput;

use super::{load_song_from_pack, MusicPlayer, RING_CAPACITY_FRAMES};

/// A short, loud, tied square-ish wave -- loud enough that `push`ed frames
/// are trivially distinguishable from silence.
fn loud_wave() -> Arc<WaveData> {
    Arc::new(WaveData::one_shot(1 << 20, vec![100; 64]))
}

/// A song that loops forever via its own `Goto`, exactly like a real BGM
/// (never reaching `Fine`) -- see `super`'s module docs on why continuous
/// playback needs no extra loop-restart logic beyond a song's own jump
/// commands.
fn looping_song() -> Song {
    let voices = vec![Instrument::DirectSound(ToneData::new(
        loud_wave(),
        Adsr::flat(),
    ))];
    let events = vec![
        Event::Voice(0),
        Event::Note {
            key: 60,
            velocity: 127,
            gate: 0,
        },
        Event::Wait(50),
        Event::Goto(0),
    ];
    Song::new(voices, vec![events], 150)
}

/// A song that reaches `Fine` quickly -- for
/// [`a_finished_song_restarts_instead_of_falling_permanently_silent`].
fn short_one_shot_song() -> Song {
    let voices = vec![Instrument::DirectSound(ToneData::new(
        loud_wave(),
        Adsr::flat(),
    ))];
    let events = vec![
        Event::Voice(0),
        Event::Note {
            key: 60,
            velocity: 127,
            gate: 4,
        },
        Event::Wait(8),
        Event::Fine,
    ];
    Song::new(voices, vec![events], 150)
}

#[test]
fn advance_frame_produces_audible_output_and_never_underruns_when_drained_each_frame() {
    let output = AudioOutput::null(RING_CAPACITY_FRAMES);
    let mut player = MusicPlayer::start(looping_song(), output).expect("null backend never errors");
    assert!(player.is_running());

    let mut any_audible = false;
    let mut drained = vec![0.0_f32; Sequencer::FRAME_SAMPLES];
    for _ in 0..64 {
        player.advance_frame();
        player.drain_null_for_test(&mut drained);
        if drained.iter().any(|&s| s != 0.0) {
            any_audible = true;
        }
    }
    assert!(any_audible, "a looping song must produce audible output");
    assert_eq!(
        player.underruns(),
        0,
        "draining exactly one frame per advance must never starve the ring"
    );
}

#[test]
fn a_finished_song_restarts_instead_of_falling_permanently_silent() {
    let output = AudioOutput::null(RING_CAPACITY_FRAMES);
    let mut player =
        MusicPlayer::start(short_one_shot_song(), output).expect("null backend never errors");

    // Advance well past when the short song's note (and its release tail)
    // would have finished, draining every frame so the ring never just
    // accumulates the prefill's own leftover audio.
    let mut drained = vec![0.0_f32; Sequencer::FRAME_SAMPLES];
    let mut audible_after_expected_finish = false;
    for frame in 0..200 {
        player.advance_frame();
        player.drain_null_for_test(&mut drained);
        if frame > 100 && drained.iter().any(|&s| s != 0.0) {
            audible_after_expected_finish = true;
        }
    }
    assert!(
        audible_after_expected_finish,
        "a one-shot song must restart rather than staying silent forever once finished"
    );
}

/// Real-pack coverage of [`load_song_from_pack`] (S-3, issue #185): resolves
/// `mus_title` end to end (voicegroup indirection, samples, reverb) and
/// proves the result behaves like continuous BGM -- audible, self-looping,
/// never reaching `Sequencer::is_finished`.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn mus_title_resolves_and_plays_continuously_with_its_real_reverb_level() {
    let pack = assets::AssetPack::load_default().expect("run `cargo xtask extract` first");
    let song = load_song_from_pack(&pack, "mus_title").expect("mus_title must resolve cleanly");

    // `-R50` in `pokeemerald/sound/songs/midi/midi.cfg`'s `mus_title.mid`
    // entry (cited in this crate's `music` module docs).
    assert_eq!(song.reverb(), 50);

    let mut seq = Sequencer::new(song);
    let mut buffer = vec![0.0_f32; Sequencer::FRAME_SAMPLES];
    let mut any_audible = false;
    // A few seconds of frames: long enough for the real song's intro to
    // start sounding, nowhere near long enough to reach a real BGM's loop
    // point if it had one -- the point here is "does not finish", not
    // "audible on every single frame" (rests are legitimate).
    for _ in 0..300 {
        seq.render_frame(&mut buffer);
        if buffer.iter().any(|&s| s != 0.0) {
            any_audible = true;
        }
        assert!(
            !seq.is_finished(),
            "mus_title must keep looping via its own jump commands, never reach Fine"
        );
    }
    assert!(any_audible, "mus_title must actually produce sound");
}

/// The ignored, env-gated end-to-end fidelity oracle (Discussion #227's
/// owner decision): compares this crate's native offline PCM render of
/// `mus_title` -- before any device resampling -- against a local mGBA
/// reference capture that is never committed to the repository (asset
/// policy, Discussion #71).
///
/// # Producing a reference capture
///
/// 1. Build `mgba` (pinned by `init.sh`, or any recent mGBA) and a Pokémon
///    Emerald ROM (not shipped by this project -- BYO, same policy as every
///    other real-pack/real-ROM tool in this repo).
/// 2. Boot to the title screen and let `MUS_TITLE` play; use mGBA's
///    "Record audio" feature (`Tools > Record A/V` or the equivalent CLI
///    front end) to capture raw PCM at its native output rate.
/// 3. Convert the capture to headerless, interleaved-stereo `f32` PCM at
///    exactly [`audio::pitch::MIXER_RATE`] (13379 Hz) -- e.g.
///    `ffmpeg -i capture.wav -ar 13379 -ac 2 -f f32le title_ref.pcm`. This
///    must be the *native* mGBA render rate resampled down to the engine's
///    own mixer rate, not mGBA's host-audio-driver output rate, so both
///    sides are compared before either one's own device resampling.
/// 4. Point `POKEEMERALD_RS_MGBA_TITLE_PCM` at the resulting file and run
///    `cargo test -p pokeemerald-rs --ignored mgba_reference -- --nocapture`.
///
/// # Alignment and tolerance
///
/// The two renders are not guaranteed to start at the same absolute sample
/// (mGBA's own boot sequence differs from where this render starts): find
/// the best-aligning offset within the first
/// [`ALIGNMENT_SEARCH_WINDOW`](oracle::ALIGNMENT_SEARCH_WINDOW) samples by
/// maximizing cross-correlation, then compare an [`COMPARE_WINDOW`](oracle::COMPARE_WINDOW)-sample
/// window from there. Tolerance is RMS-error-relative, not sample-exact
/// (`(behavioral-fidelity)`: player-audible result, not byte parity) --
/// [`RMS_ERROR_TOLERANCE`](oracle::RMS_ERROR_TOLERANCE) is derived from (and
/// must be re-derived and documented against) the *first* real reference
/// comparison run, per Discussion #227's decision; until then it is a
/// deliberately generous placeholder that still catches a grossly wrong
/// render (silence, garbage, wrong pitch).
mod oracle {
    use std::env;
    use std::fs;

    use audio::{Sequencer, MIXER_RATE};

    use super::load_song_from_pack;

    /// How far into each render to search for the best-aligning offset.
    pub(super) const ALIGNMENT_SEARCH_WINDOW: usize = MIXER_RATE as usize * 2; // 2 seconds
    /// How many aligned samples to compare once an offset is chosen.
    pub(super) const COMPARE_WINDOW: usize = MIXER_RATE as usize * 5; // 5 seconds
    /// Placeholder RMS-error tolerance (module docs): re-derive from the
    /// first real mGBA comparison and replace this constant (and this
    /// comment) with the justified value.
    pub(super) const RMS_ERROR_TOLERANCE: f64 = 0.35;

    /// Read a headerless, interleaved-stereo `f32` PCM file.
    fn read_pcm_f32(path: &str) -> Vec<f32> {
        let bytes = fs::read(path).unwrap_or_else(|e| panic!("reading `{path}`: {e}"));
        bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    }

    /// Left-channel RMS error at `offset` between `reference` and
    /// `candidate`, over up to [`COMPARE_WINDOW`] samples.
    fn rms_error(reference: &[f32], candidate: &[f32], offset: usize) -> f64 {
        let len = COMPARE_WINDOW.min(reference.len().saturating_sub(offset).min(candidate.len()));
        assert!(len > 0, "no overlap at offset {offset}");
        let sum_sq: f64 = (0..len)
            .map(|i| {
                let diff = f64::from(reference[offset + i]) - f64::from(candidate[i]);
                diff * diff
            })
            .sum();
        #[allow(clippy::cast_precision_loss)] // `len` is a handful of seconds of samples.
        let mean = sum_sq / len as f64;
        mean.sqrt()
    }

    /// Search `0..ALIGNMENT_SEARCH_WINDOW` for the offset into `reference`
    /// with the lowest RMS error against `candidate`'s start.
    fn best_alignment(reference: &[f32], candidate: &[f32]) -> usize {
        (0..ALIGNMENT_SEARCH_WINDOW.min(reference.len()))
            .min_by(|&a, &b| {
                rms_error(reference, candidate, a)
                    .partial_cmp(&rms_error(reference, candidate, b))
                    .expect("RMS error is always finite")
            })
            .unwrap_or(0)
    }

    #[test]
    #[ignore = "needs a local pack and a local mGBA reference capture: see this module's docs"]
    fn native_render_matches_local_mgba_reference_within_tolerance() {
        let Ok(path) = env::var("POKEEMERALD_RS_MGBA_TITLE_PCM") else {
            eprintln!(
                "skipped: set POKEEMERALD_RS_MGBA_TITLE_PCM to a local mGBA reference capture \
                 (interleaved-stereo f32 PCM at MIXER_RATE) -- see this module's doc comment for \
                 how to produce one"
            );
            return;
        };

        let pack = assets::AssetPack::load_default().expect("run `cargo xtask extract` first");
        let song = load_song_from_pack(&pack, "mus_title").expect("mus_title must resolve cleanly");
        let mut seq = Sequencer::new(song);

        let native_frames =
            (ALIGNMENT_SEARCH_WINDOW + COMPARE_WINDOW).div_ceil(audio::SAMPLES_PER_FRAME);
        let mut native = vec![0.0_f32; native_frames * Sequencer::FRAME_SAMPLES];
        seq.mix_into(&mut native);
        // Left channel only, matching `rms_error`/`read_pcm_f32`'s layout.
        let native_left: Vec<f32> = native.iter().copied().step_by(2).collect();

        let reference = read_pcm_f32(&path);
        let reference_left: Vec<f32> = reference.iter().copied().step_by(2).collect();

        let offset = best_alignment(&reference_left, &native_left);
        let error = rms_error(&reference_left, &native_left, offset);
        assert!(
            error < RMS_ERROR_TOLERANCE,
            "native render diverges from the mGBA reference: RMS error {error:.4} at aligned \
             offset {offset} exceeds the documented tolerance {RMS_ERROR_TOLERANCE}"
        );
    }
}
